pub(crate) mod cyber;

use regex::Regex;
use reqwest::header::{ORIGIN, REFERER};
use scraper::Html;

use crate::models::{
    ActiveLecture, ActiveLectures, AttendanceCourse, AttendanceMark, AttendanceReceipt, AttendanceSubmission, AttendanceSummary,
    AttendanceWeek, Course, MarkKind,
};
use crate::util::{form_inputs, looks_like_login, sel, squash, text_of};
use crate::{CoreError, Result, SchoolSession};

const AT: &str = "https://at.hongik.ac.kr/";

impl SchoolSession {
    async fn ensure_attendance(&self) -> Result<Option<String>> {
        let mut initial_body = None;
        self.attendance_ready
            .get_or_try_init(|| async {
                use reqwest::cookie::CookieStore;
                let origin = reqwest::Url::parse(AT).expect("출결 주소");
                let has_session = self.cookie_jar.cookies(&origin).and_then(|header| header.to_str().ok().map(|value| value.contains("JSESSIONID="))).unwrap_or(false);
                if has_session {
                    match self.get_text(&format!("{AT}index.jsp"), None).await {
                        Ok(body) if Html::parse_document(&body).select(&sel("table")).next().is_some() => {
                            initial_body = Some(body);
                            return Ok(());
                        }
                        Ok(_) | Err(CoreError::SessionExpired) => {}
                        Err(error) => return Err(error),
                    }
                }
                self.client
                    .get(format!("{AT}login.jsp"))
                    .header(REFERER, "https://my.hongik.ac.kr/")
                    .send()
                    .await?;
                let body = self.get_text(&format!("{AT}index.jsp"), Some(&format!("{AT}login.jsp"))).await?;
                if looks_like_login(&body) {
                    return Err(CoreError::SessionExpired);
                }
                initial_body = Some(body);
                Ok::<(), CoreError>(())
            })
            .await?;
        Ok(initial_body)
    }

    async fn active_rows(
        &self,
    ) -> Result<(Vec<(ActiveLecture, Vec<(String, String)>)>, Option<String>)> {
        let body = match self.ensure_attendance().await? {
            Some(body) => body,
            None => self.get_text(&format!("{AT}index.jsp"), Some(&format!("{AT}login.jsp"))).await?,
        };
        if looks_like_login(&body) {
            return Err(CoreError::SessionExpired);
        }
        let doc = Html::parse_document(&body);
        let table = doc
            .select(&sel("table"))
            .next()
            .ok_or_else(|| CoreError::Parse("출결 목록 표".into()))?;
        let mut rows = Vec::new();
        for tr in table.select(&sel("tbody > tr")) {
            let Some(form) = tr.select(&sel("form[action*='stud02.jsp']")).next() else {
                continue;
            };
            let cells: Vec<String> = tr.select(&sel("td")).map(text_of).collect();
            let name = cells.get(2).cloned().unwrap_or_default();
            let time = cells.get(4).cloned().unwrap_or_default();
            let inputs = form_inputs(form);
            let code = match (field(&inputs, "haksu"), field(&inputs, "bunban")) {
                (h, b) if !h.is_empty() && !b.is_empty() => Some(format!("{h}-{b}")),
                _ => None,
            };
            let lecture = ActiveLecture {
                key: format!("{}|{name}|{time}", code.as_deref().unwrap_or("")),
                name,
                time,
                code,
            };
            rows.push((lecture, inputs));
        }
        let message = if rows.is_empty() {
            table
                .select(&sel("tbody"))
                .next()
                .map(text_of)
                .filter(|m| !m.is_empty())
        } else {
            None
        };
        Ok((rows, message))
    }

    pub async fn active_lectures(&self) -> Result<ActiveLectures> {
        let (rows, message) = self.active_rows().await?;
        Ok(ActiveLectures {
            items: rows.into_iter().map(|(l, _)| l).collect(),
            message,
        })
    }

    pub async fn submit_attendance(
        &self,
        lecture_key: &str,
        code: &str,
        latitude: f64,
        longitude: f64,
    ) -> Result<AttendanceSubmission> {
        let (rows, _) = self.active_rows().await?;
        let (lecture, mut form) = rows
            .into_iter()
            .find(|(lecture, _)| lecture.key == lecture_key)
            .ok_or_else(|| CoreError::NotFound("지금 출석할 수 있는 수업이 아니에요.".into()))?;
        form.push(("key".into(), code.to_string()));
        form.push(("latitude".into(), latitude.to_string()));
        form.push(("longitude".into(), longitude.to_string()));
        let submitted_at = chrono::Utc::now();
        let response = self
            .client
            .post(format!("{AT}stud02_proc.jsp"))
            .header(ORIGIN, "https://at.hongik.ac.kr")
            .header(REFERER, format!("{AT}stud02.jsp"))
            .form(&form)
            .send()
            .await?;
        let body = crate::session::school_text(response).await?;
        if looks_like_login(&body) {
            return Err(CoreError::SessionExpired);
        }
        let message = submission_message(&body)
            .ok_or_else(|| CoreError::Upstream("학교 응답을 확인하지 못했어요. 출석 처리 여부를 확인해 주세요.".into()))?;
        let receipt = confirmed_attendance(&message).map(|kind| AttendanceReceipt {
            lecture,
            date: (submitted_at + chrono::Duration::hours(9)).format("%Y-%m-%d").to_string(),
            kind,
            confirmed_at: submitted_at.timestamp_millis(),
        });
        Ok(AttendanceSubmission { message, receipt })
    }

    pub async fn attendance_status(&self) -> Result<Vec<AttendanceCourse>> {
        let _ = self.ensure_attendance().await?;
        let (forms, enrolled) = futures::try_join!(self.fetch_attendance_forms(), self.enrolled_courses())?;
        let classroom = if enrolled.iter().any(|c| c.cyber) { self.all_courses().await? } else { vec![] };
        let mut courses = Vec::new();
        for data in forms {
            let code = format!("{}-{}", field(&data, "haksu"), field(&data, "bunban"));
            if let Some(course) = enrolled.iter().find(|c| c.code == code && c.cyber) {
                courses.push(self.cyber_course_status(&data, course, &classroom).await?);
            } else {
                courses.push(self.offline_course_status(&data).await?);
            }
        }
        Ok(courses)
    }

    pub async fn attendance_course(&self, code: &str) -> Result<AttendanceCourse> {
        let _ = self.ensure_attendance().await?;
        let (forms, enrolled) = futures::try_join!(
            self.attendance_forms.get_or_try_init(|| self.fetch_attendance_forms()),
            self.enrolled_courses(),
        )?;
        let data = forms.iter()
            .find(|f| format!("{}-{}", field(f, "haksu"), field(f, "bunban")) == code)
            .ok_or_else(|| CoreError::NotFound("이번 학기 수강 과목이 아니에요.".into()))?;
        if let Some(course) = enrolled.iter().find(|c| c.code == code && c.cyber) {
            return self.cyber_course_status(data, course, &self.all_courses().await?).await;
        }
        self.offline_course_status(data).await
    }

    async fn offline_course_status(&self, data: &[(String, String)]) -> Result<AttendanceCourse> {
        let body = self.post_form_text(&format!("{AT}stud05.jsp"), data, Some(&format!("{AT}stud04.jsp"))).await?;
        if looks_like_login(&body) { return Err(CoreError::SessionExpired); }
        Ok(parse_course_status(&body, data))
    }

    async fn cyber_course_status(&self, data: &[(String, String)], enrolled: &crate::timetable::EnrolledCourse, courses: &[Course]) -> Result<AttendanceCourse> {
        let year = field(data, "yy").parse::<i32>().ok();
        let semester = field(data, "hakgi").parse::<i32>().ok().map(|n| if n < 10 { n * 10 } else { n });
        let course = courses.iter().find(|c| c.code.as_deref() == Some(enrolled.code.as_str())
            && c.term.is_some_and(|t| Some(t.year) == year && Some(t.semester) == semester));
        if let Some(course) = course {
            match self.cyber_attendance(course).await {
                Ok(attendance) => return Ok(attendance),
                Err(error @ (CoreError::SessionExpired | CoreError::ClassroomTokenExpired)) => return Err(error),
                Err(error) => tracing::warn!(error = %error, "사이버 출석부 조회 실패"),
            }
        }
        Ok(AttendanceCourse {
            cyber: true, code: enrolled.code.clone(), name: enrolled.name.clone(), published: false,
            notice: Some("사이버 출석부를 불러오지 못했어요.".into()), weeks: vec![], summary: AttendanceSummary::default(),
        })
    }

    async fn fetch_attendance_forms(&self) -> Result<Vec<Vec<(String, String)>>> {
        let list = self
            .get_text(&format!("{AT}stud04.jsp"), Some(&format!("{AT}stud01.jsp")))
            .await?;
        if looks_like_login(&list) {
            return Err(CoreError::SessionExpired);
        }
        let doc = Html::parse_document(&list);
        if doc.select(&sel("table")).next().is_none() {
            return Err(CoreError::Parse("출결 과목 목록".into()));
        }
        Ok(doc
            .select(&sel("form[action='stud05.jsp']"))
            .map(form_inputs)
            .collect())
    }
}

pub fn confirmed_attendance(message: &str) -> Option<MarkKind> {
    let compact: String = message.chars().filter(|c| !c.is_whitespace()).collect();
    match compact.as_str() {
        "출석확인이완료되었습니다.[출석]" => Some(MarkKind::Present),
        "출석확인이완료되었습니다.[지각]" => Some(MarkKind::Late),
        "출석확인이완료되었습니다.[공결]" => Some(MarkKind::Excused),
        _ => None,
    }
}

pub fn submission_message(body: &str) -> Option<String> {
    if let Some(message) = alert_message(body) { return Some(message); }
    Html::parse_document(body).select(&sel(".alert.alert-warning"))
        .next().map(text_of).filter(|message| !message.is_empty())
}

fn field<'a>(data: &'a [(String, String)], name: &str) -> &'a str {
    data.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

fn parse_course_status(body: &str, data: &[(String, String)]) -> AttendanceCourse {
    let doc = Html::parse_document(body);
    let heading = doc
        .select(&sel("h4"))
        .next()
        .map(text_of)
        .unwrap_or_default();
    let name = heading
        .split_once(" - ")
        .map(|(_, rest)| rest.to_string())
        .unwrap_or(heading);
    let code = format!("{}-{}", field(data, "haksu"), field(data, "bunban"));
    if let Some(notice) = doc.select(&sel(".alert-warning")).next() {
        return AttendanceCourse {
            cyber: false,
            code,
            name,
            published: false,
            notice: Some(text_of(notice)),
            weeks: vec![],
            summary: AttendanceSummary::default(),
        };
    }
    let mut weeks = Vec::new();
    let mut summary = AttendanceSummary::default();
    for tr in doc.select(&sel("table tr")) {
        let cells: Vec<String> = tr.select(&sel("td")).map(text_of).collect();
        let Some(week) = cells.first().and_then(|c| c.parse::<u32>().ok()) else {
            continue;
        };
        let sessions: Vec<AttendanceMark> = cells[1..]
            .chunks(2)
            .filter(|pair| pair.len() == 2)
            .map(|pair| {
                let kind = classify(&pair[0], &pair[1]);
                match kind {
                    MarkKind::Present => summary.present += 1,
                    MarkKind::Late => summary.late += 1,
                    MarkKind::Absent => summary.absent += 1,
                    MarkKind::Excused => summary.excused += 1,
                    MarkKind::None => summary.none += 1,
                    MarkKind::Planned => summary.planned += 1,
                    MarkKind::Other => {}
                }
                AttendanceMark {
                    title: None,
                    period: None,
                    date: short_date(&pair[0]),
                    mark: pair[1].clone(),
                    kind,
                }
            })
            .collect();
        weeks.push(AttendanceWeek { week, sessions });
    }
    AttendanceCourse {
        cyber: false,
        code,
        name,
        published: true,
        notice: None,
        weeks,
        summary,
    }
}

fn classify(date: &str, mark: &str) -> MarkKind {
    if date.contains("미입력") {
        MarkKind::Planned
    } else if mark == "-" || mark.is_empty() {
        MarkKind::None
    } else if mark.contains("결석") {
        MarkKind::Absent
    } else if mark.contains("지각") || mark.contains("조퇴") {
        MarkKind::Late
    } else if mark.contains("공결") || mark.contains("인정") {
        MarkKind::Excused
    } else if mark.contains("출석") {
        MarkKind::Present
    } else {
        MarkKind::Other
    }
}

fn short_date(cell: &str) -> String {
    let re = Regex::new(r"(\d{4}[-./])?(\d{1,2})[-./](\d{1,2})").expect("정규식");
    match re.captures(cell) {
        Some(c) => format!(
            "{}/{}",
            c[2].trim_start_matches('0'),
            c[3].trim_start_matches('0')
        ),
        None => squash(cell),
    }
}

fn alert_message(body: &str) -> Option<String> {
    let re = Regex::new(r#"alert\s*\(\s*(?:'((?:\\.|[^'\\])*)'|"((?:\\.|[^"\\])*)")\s*\)"#)
        .expect("정규식");
    let cap = re.captures(body)?;
    let raw = cap.get(1).or_else(|| cap.get(2))?.as_str();
    let message = raw
        .replace("\\n", "\n")
        .replace("\\'", "'")
        .replace("\\\"", "\"");
    let message = message.trim().to_string();
    (!message.is_empty()).then_some(message)
}
