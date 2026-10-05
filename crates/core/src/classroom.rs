use std::collections::HashMap;

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use futures::future::join_all;
use regex::Regex;
use reqwest::header::{LOCATION, USER_AGENT};
use scraper::Html;
use serde_json::Value;

use crate::models::{
    AcademicTerm, Assignment, Attachment, BoardArticle, Course, ModuleContents, Notification, Profile, SemesterDisplay, SubmissionInfo, SubmissionState,
    SubmitConfig, Vod, VodState,
};
use crate::session::MoodleAuth;
use crate::util::{parse_kst, sel, text_excluding, text_of};
use crate::{CoreError, Result, SchoolSession};

pub const CN2: &str = "https://cn2.hongik.ac.kr";

impl SchoolSession {
    async fn ensure_moodle_web(&self) -> Result<()> {
        self.moodle_web_ready.get_or_try_init(|| async {
            use reqwest::cookie::CookieStore;
            let origin = url::Url::parse(CN2).expect("클래스룸 주소");
            let has_session = self.cookie_jar.cookies(&origin).and_then(|header| header.to_str().ok().map(|value| value.contains("MoodleSession="))).unwrap_or(false);
            if has_session {
                let body = self.client.get(format!("{CN2}/")).send().await?.text().await?;
                if body.contains("\"sesskey\"") && !body.contains("loginform") { return Ok(()); }
            }
            if let Some(auth) = self.moodle.get().filter(|auth| auth.private_token.is_some()) {
                match self.autologin_for(auth, &format!("{CN2}/")).await {
                    Ok(url) => {
                        let body = self.client.get(url).send().await?.text().await?;
                        if body.contains("\"sesskey\"") { return Ok(()); }
                    }
                    Err(error @ (CoreError::Network(_) | CoreError::ClassroomTokenExpired)) => return Err(error),
                    Err(_) => {}
                }
            }
            let body = self.client.get(format!("{CN2}/login/index.php")).send().await?.text().await?;
            if !body.contains("\"sesskey\"") {
                return Err(CoreError::SessionExpired);
            }
            Ok::<(), CoreError>(())
        }).await?;
        Ok(())
    }

    async fn moodle_page(&self, url: &str) -> Result<String> {
        self.moodle_page_with_user_agent(url, crate::session::UA).await
    }

    async fn moodle_page_with_user_agent(&self, url: &str, user_agent: &str) -> Result<String> {
        self.ensure_moodle_web().await?;
        let body = self.client.get(url).header(USER_AGENT, user_agent).send().await?.text().await?;
        if body.contains("loginform") || body.contains("id=\"login\"") {
            return Err(CoreError::SessionExpired);
        }
        Ok(body)
    }

    async fn moodle(&self) -> Result<&MoodleAuth> {
        self.moodle
            .get_or_try_init(|| async {
                self.ensure_moodle_web().await?;
                let passport = chrono::Utc::now().timestamp().to_string();
                let response = self
                    .no_redirect
                    .get(format!("{CN2}/admin/tool/mobile/launch.php"))
                    .query(&[("service", "moodle_mobile_app"), ("passport", passport.as_str()), ("urlscheme", "moodlemobile")])
                    .send()
                    .await?;
                let location = response.headers().get(LOCATION).and_then(|v| v.to_str().ok()).unwrap_or("");
                let encoded = location
                    .strip_prefix("moodlemobile://token=")
                    .ok_or_else(|| CoreError::Upstream("클래스룸 모바일 토큰을 받지 못했어요.".into()))?
                    .trim();
                let bytes = STANDARD
                    .decode(encoded)
                    .or_else(|_| STANDARD_NO_PAD.decode(encoded.trim_end_matches('=')))
                    .map_err(|_| CoreError::Parse("클래스룸 토큰".into()))?;
                let decoded = String::from_utf8(bytes).map_err(|_| CoreError::Parse("클래스룸 토큰".into()))?;
                let mut parts = decoded.split(":::");
                let token = parts
                    .nth(1)
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| CoreError::Parse("클래스룸 토큰".into()))?
                    .to_string();
                let private_token = parts.next().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
                let info = ws_call(&self.client, &token, "core_webservice_get_site_info", &[]).await?;
                let user_id = info["userid"].as_i64().ok_or_else(|| CoreError::Parse("클래스룸 사용자".into()))?;
                let department = ws_call(
                    &self.client,
                    &token,
                    "core_user_get_users_by_field",
                    &[("field".into(), "id".into()), ("values[0]".into(), user_id.to_string())],
                )
                .await
                .ok()
                .and_then(|v| v[0]["department"].as_str().map(str::trim).filter(|d| !d.is_empty()).map(str::to_string));
                Ok::<MoodleAuth, CoreError>(MoodleAuth {
                    token,
                    private_token,
                    user_id,
                    full_name: info["fullname"].as_str().unwrap_or("").to_string(),
                    picture_url: info["userpictureurl"].as_str().map(str::to_string),
                    department,
                })
            })
            .await
    }

    async fn ws(&self, function: &str, params: &[(String, String)]) -> Result<Value> {
        let auth = self.moodle().await?;
        ws_call(&self.client, &auth.token, function, params).await
    }

    pub fn cached_profile(&self) -> Option<Profile> {
        self.moodle.get().map(|auth| Profile {
            name: auth.full_name.clone(),
            has_picture: auth.picture_url.as_deref().is_some_and(|u| u.contains("pluginfile.php")),
            department: auth.department.clone(),
        })
    }

    pub async fn profile(&self) -> Result<Profile> {
        let auth = self.moodle().await?;
        let has_picture = auth.picture_url.as_deref().is_some_and(|u| u.contains("pluginfile.php"));
        Ok(Profile { name: auth.full_name.clone(), has_picture, department: auth.department.clone() })
    }

    pub async fn moodle_token(&self) -> Result<String> {
        Ok(self.moodle().await?.token.clone())
    }

    pub async fn refresh_moodle(&self) -> Result<Self> {
        let mut snapshot = self.snapshot();
        snapshot.moodle = None;
        let session = Self::from_snapshot(snapshot)?;
        session.moodle().await?;
        Ok(session)
    }

    pub async fn autologin_url(&self, target: &str) -> Result<String> {
        if !target.starts_with(&format!("{CN2}/")) {
            return Err(CoreError::NotFound("클래스룸 주소가 아니에요.".into()));
        }
        self.autologin_for(self.moodle().await?, target).await
    }

    pub async fn restore_classroom_web(&self) -> Result<()> {
        self.ensure_moodle_web().await
    }

    async fn autologin_for(&self, auth: &MoodleAuth, target: &str) -> Result<String> {
        let private = auth
            .private_token
            .as_deref()
            .ok_or_else(|| CoreError::Upstream("이 계정은 자동 로그인을 쓸 수 없어요.".into()))?;
        let v = ws_call(&self.client, &auth.token, "tool_mobile_get_autologin_key", &[("privatetoken".into(), private.into())]).await?;
        let key = v["key"].as_str().ok_or_else(|| CoreError::Parse("자동 로그인 키".into()))?;
        let base = v["autologinurl"].as_str().unwrap_or("").to_string();
        let base = if base.starts_with(&format!("{CN2}/")) { base } else { format!("{CN2}/admin/tool/mobile/autologin.php") };
        let mut url = url::Url::parse(&base).map_err(|_| CoreError::Parse("자동 로그인 주소".into()))?;
        url.query_pairs_mut()
            .append_pair("userid", &auth.user_id.to_string())
            .append_pair("key", key)
            .append_pair("urltogo", target);
        Ok(url.to_string())
    }

    pub async fn download(&self, url: &str, max_bytes: u64) -> Result<(String, Vec<u8>)> {
        if !url.starts_with(&format!("{CN2}/")) {
            return Err(CoreError::NotFound("클래스룸 파일이 아니에요.".into()));
        }
        let url = url.replacen(&format!("{CN2}/pluginfile.php/"), &format!("{CN2}/webservice/pluginfile.php/"), 1);
        let auth = self.moodle().await?;
        let response = self.client.get(&url).query(&[("token", auth.token.as_str())]).send().await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CoreError::ClassroomTokenExpired);
        }
        if !response.status().is_success() {
            crate::api_error::report("GET", "/api/files/:id", response.status().as_u16(), "school_error", crate::api_error::response_format(response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("")));
            return Err(CoreError::Upstream("다운로드에 실패했어요.".into()));
        }
        if response.content_length().is_some_and(|n| n > max_bytes) {
            return Err(CoreError::Upstream("파일을 다운받을 수 없어요.".into()));
        }
        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        if mime.starts_with("text/html") {
            return Err(CoreError::ClassroomTokenExpired);
        }
        let bytes = response.bytes().await?;
        if bytes.len() as u64 > max_bytes {
            return Err(CoreError::Upstream("파일을 다운받을 수 없어요.".into()));
        }
        Ok((mime, bytes.to_vec()))
    }

    pub async fn avatar(&self) -> Result<Option<(String, Vec<u8>)>> {
        let Some(url) = self.moodle().await?.picture_url.clone() else { return Ok(None) };
        Ok(Some(self.download(&url, 5 * 1024 * 1024).await?))
    }

    pub async fn module_contents(&self, cmid: i64) -> Result<ModuleContents> {
        let not_found = || CoreError::NotFound("활동을 찾지 못했어요.".into());
        let cm = self.ws("core_course_get_course_module", &[("cmid".into(), cmid.to_string())]).await?;
        let course_id = cm["cm"]["course"].as_i64().ok_or_else(not_found)?;
        let contents = self
            .ws(
                "core_course_get_contents",
                &[
                    ("courseid".into(), course_id.to_string()),
                    ("options[0][name]".into(), "cmid".into()),
                    ("options[0][value]".into(), cmid.to_string()),
                ],
            )
            .await?;
        let module = contents
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|section| section["modules"].as_array().cloned().unwrap_or_default())
            .find(|m| m["id"].as_i64() == Some(cmid))
            .ok_or_else(not_found)?;
        let mut files = Vec::new();
        let mut link = None;
        for c in module["contents"].as_array().into_iter().flatten() {
            match c["type"].as_str() {
                Some("file") => {
                    let dir = c["filepath"].as_str().unwrap_or("/").trim_matches('/');
                    let file = c["filename"].as_str().unwrap_or("");
                    files.push(Attachment {
                        name: if dir.is_empty() { file.to_string() } else { format!("{dir}/{file}") },
                        size: c["filesize"].as_i64(),
                        mime: c["mimetype"].as_str().map(str::to_string),
                        url: c["fileurl"].as_str().unwrap_or("").to_string(),
                    });
                }
                Some("url") => {
                    link = c["fileurl"].as_str().filter(|u| u.starts_with("https://") || u.starts_with("http://")).map(str::to_string)
                }
                _ => {}
            }
        }
        Ok(ModuleContents {
            cmid,
            course_id,
            modname: module["modname"].as_str().or_else(|| cm["cm"]["modname"].as_str()).unwrap_or("").to_string(),
            name: module["name"].as_str().unwrap_or("").to_string(),
            files,
            link,
        })
    }

    pub async fn board_article(&self, cmid: i64, bwid: i64) -> Result<BoardArticle> {
        let body = self.moodle_page(&format!("{CN2}/mod/ubboard/article.php?id={cmid}&bwid={bwid}")).await?;
        let mut article = parse_article(&body, cmid, bwid)
            .ok_or_else(|| CoreError::NotFound("글을 찾지 못했어요.".into()))?;
        article.html = self.inline_images(&article.html).await;
        Ok(article)
    }

    async fn inline_images(&self, html: &str) -> String {
        const EACH: u64 = 6 * 1024 * 1024;
        const TOTAL: usize = 16 * 1024 * 1024;
        let prefixes = [format!("{CN2}/pluginfile.php/"), format!("{CN2}/webservice/pluginfile.php/")];
        let mut srcs: Vec<String> = Vec::new();
        for img in Html::parse_fragment(html).select(&sel("img[src]")) {
            let src = img.value().attr("src").unwrap_or("");
            if prefixes.iter().any(|p| src.starts_with(p.as_str())) && !srcs.iter().any(|s| s == src) {
                srcs.push(src.to_string());
            }
        }
        srcs.truncate(12);
        let fetched = join_all(srcs.iter().map(|src| self.download(src, EACH))).await;
        let mut out = html.to_string();
        let mut total = 0;
        for (src, result) in srcs.iter().zip(fetched) {
            let Ok((mime, bytes)) = result else { continue };
            if !mime.starts_with("image/") || total + bytes.len() > TOTAL {
                continue;
            }
            total += bytes.len();
            let data = format!("data:{};base64,{}", mime.split(';').next().unwrap_or("image/png"), STANDARD.encode(&bytes));
            out = out.replace(&format!("src=\"{}\"", escape_attr(src)), &format!("src=\"{data}\""));
        }
        out
    }

    pub async fn submission_info(&self, assign_id: i64) -> Result<SubmissionInfo> {
        let v = self.ws("mod_assign_get_submission_status", &[("assignid".into(), assign_id.to_string())]).await?;
        let last = &v["lastattempt"];
        let sub = &last["submission"];
        let status = match sub["status"].as_str().unwrap_or("") {
            "submitted" => SubmissionState::Submitted,
            "draft" | "new" | "" => SubmissionState::NotSubmitted,
            _ => SubmissionState::Unknown,
        };
        let files = sub["plugins"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["type"] == "file")
            .flat_map(|p| p["fileareas"].as_array().cloned().unwrap_or_default())
            .flat_map(|area| area["files"].as_array().cloned().unwrap_or_default())
            .map(|f| Attachment {
                name: f["filename"].as_str().unwrap_or("").to_string(),
                size: f["filesize"].as_i64(),
                mime: f["mimetype"].as_str().map(str::to_string),
                url: f["fileurl"].as_str().unwrap_or("").to_string(),
            })
            .collect();
        Ok(SubmissionInfo {
            status,
            can_edit: last["canedit"].as_bool().unwrap_or(false),
            locked: last["locked"].as_bool().unwrap_or(false),
            files,
            modified: sub["timemodified"].as_i64().filter(|t| *t > 0),
        })
    }

    pub async fn submit_files_with_progress(
        &self,
        assign_id: i64,
        files: Vec<(String, Vec<u8>)>,
        drafts: bool,
        accept_statement: bool,
        job: std::sync::Arc<crate::submission::Job>,
    ) -> Result<()> {
        use crate::submission::{Progress, Stage};
        let token = self.moodle().await?.token.clone();
        let mut item_id: i64 = 0;
        let file_count = files.len();
        for (uploaded_files, (name, bytes)) in files.into_iter().enumerate() {
            let total_bytes = bytes.len() as u64;
            let progress = Progress { stage: Stage::Upload, file_name: Some(name.clone()), file_count, uploaded_files, sent_bytes: 0, total_bytes };
            job.progress(progress.clone());
            let upload_job = job.clone();
            let stream = futures::stream::unfold((bytes, 0usize, std::time::Instant::now()), move |(bytes, offset, mut notified)| {
                let job = upload_job.clone();
                let mut progress = progress.clone();
                async move {
                    if offset == bytes.len() { return None; }
                    let end = (offset + 64 * 1024).min(bytes.len());
                    let chunk = bytes[offset..end].to_vec();
                    if end == bytes.len() || notified.elapsed() >= std::time::Duration::from_millis(100) {
                        progress.sent_bytes = end as u64;
                        job.progress(progress);
                        notified = std::time::Instant::now();
                    }
                    Some((Ok::<_, std::io::Error>(chunk), (bytes, end, notified)))
                }
            });
            let part = reqwest::multipart::Part::stream_with_length(reqwest::Body::wrap_stream(stream), total_bytes).file_name(name.clone());
            let form = reqwest::multipart::Form::new()
                .text("token", token.clone())
                .text("filearea", "draft")
                .text("itemid", item_id.to_string())
                .part("file_1", part);
            let v: Value = self.client.post(format!("{CN2}/webservice/upload.php")).timeout(std::time::Duration::from_secs(120)).multipart(form).send().await?.json().await?;
            if v["errorcode"].as_str() == Some("invalidtoken") {
                return Err(CoreError::ClassroomTokenExpired);
            }
            let uploaded = v.as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
            item_id = uploaded["itemid"]
                .as_i64()
                .ok_or_else(|| CoreError::Upstream(format!("'{name}' 파일을 올리지 못했어요: {}", v["error"].as_str().unwrap_or("알 수 없는 오류"))))?;
            job.progress(Progress { stage: Stage::Upload, file_name: Some(name), file_count, uploaded_files: uploaded_files + 1, sent_bytes: total_bytes, total_bytes });
        }
        if item_id == 0 {
            return Err(CoreError::Upstream("제출할 파일이 없어요.".into()));
        }
        job.progress(Progress::at(Stage::Submit));
        let saved = self
            .ws(
                "mod_assign_save_submission",
                &[("assignmentid".into(), assign_id.to_string()), ("plugindata[files_filemanager]".into(), item_id.to_string())],
            )
            .await?;
        if let Some(w) = saved.as_array().and_then(|a| a.first()) {
            return Err(CoreError::Upstream(format!("제출이 저장되지 않았어요: {}", w["message"].as_str().unwrap_or(""))));
        }
        if drafts {
            let submitted = self
                .ws(
                    "mod_assign_submit_for_grading",
                    &[
                        ("assignmentid".into(), assign_id.to_string()),
                        ("acceptsubmissionstatement".into(), (accept_statement as u8).to_string()),
                    ],
                )
                .await?;
            if let Some(w) = submitted.as_array().and_then(|a| a.first()) {
                return Err(CoreError::Upstream(format!("제출(채점 요청)이 되지 않았어요: {}", w["message"].as_str().unwrap_or(""))));
            }
        }
        Ok(())
    }

    pub fn remember_current_term(&self, term: AcademicTerm) {
        if term.valid() {
            *self.current_term.write().expect("학기 잠금") = Some(term);
        }
    }

    pub async fn current_term(&self) -> Result<AcademicTerm> {
        let result = self.moodle_page(&format!("{CN2}/local/ubion/user/")).await
            .and_then(|body| parse_current_term(&body).ok_or_else(|| CoreError::Parse("클래스룸 기본 학기".into())));
        match result {
            Ok(term) => {
                self.remember_current_term(term);
                Ok(term)
            }
            Err(error) => self.current_term.read().expect("학기 잠금").as_ref().copied().ok_or(error),
        }
    }

    pub async fn courses_for(&self, display: SemesterDisplay) -> Result<(Option<AcademicTerm>, Vec<Course>)> {
        let (courses, term) = futures::join!(self.all_courses(), self.current_term());
        let courses = courses?;
        match display {
            SemesterDisplay::All => Ok((term.ok(), courses)),
            SemesterDisplay::Current => {
                let term = term?;
                Ok((Some(term), courses.into_iter().filter(|c| c.term == Some(term)).collect()))
            }
        }
    }

    pub async fn all_courses(&self) -> Result<Vec<Course>> {
        let user_id = self.moodle().await?.user_id;
        let list = self.ws("core_enrol_get_users_courses", &[("userid".into(), user_id.to_string())]).await?;
        Ok(parse_courses(&list))
    }

    pub async fn notifications(&self, page: u32) -> Result<Vec<Notification>> {
        let body = self.moodle_page(&format!("{CN2}/local/ubnotification/index.php?page={page}")).await?;
        Ok(parse_notifications(&body))
    }

    pub async fn assignment(&self, cmid: i64) -> Result<Assignment> {
        let not_found = || CoreError::NotFound("과제를 찾지 못했어요.".into());
        if cmid <= 0 { return Err(not_found()); }
        let module = self.ws("core_course_get_course_module", &[("cmid".into(), cmid.to_string())]).await?;
        let cm = &module["cm"];
        if cm["id"].as_i64() != Some(cmid) || cm["modname"].as_str() != Some("assign") {
            return Err(not_found());
        }
        let course_id = cm["course"].as_i64().filter(|id| *id > 0).ok_or_else(not_found)?;
        let api = self.ws("mod_assign_get_assignments", &[("courseids[0]".into(), course_id.to_string())]).await?;
        parse_assignments(&api, &HashMap::new()).into_iter()
            .find(|a| a.cmid == cmid && a.course_id == course_id && a.id > 0)
            .ok_or_else(not_found)
    }

    pub async fn assignments(&self, courses: &[Course]) -> Result<Vec<Assignment>> {
        if courses.is_empty() {
            return Ok(vec![]);
        }
        let params: Vec<(String, String)> =
            courses.iter().enumerate().map(|(i, c)| (format!("courseids[{i}]"), c.id.to_string())).collect();
        let (api, states) = futures::join!(self.ws("mod_assign_get_assignments", &params), self.submission_states(courses));
        let (api, states) = (api?, states?);
        Ok(parse_assignments(&api, &states))
    }

    async fn submission_states(&self, courses: &[Course]) -> Result<HashMap<i64, SubmissionState>> {
        let pages = join_all(courses.iter().map(|c| async move {
            let url = format!("{CN2}/mod/assign/index.php?id={}", c.id);
            self.moodle_page(&url).await
        }))
        .await;
        let mut states = HashMap::new();
        for page in pages {
            states.extend(parse_submission_states(&page?));
        }
        Ok(states)
    }

    pub async fn vods(&self, courses: &[Course]) -> Result<Vec<Vod>> {
        let results = join_all(courses.iter().map(|c| self.course_vods(c.id))).await;
        let mut out = Vec::new();
        for r in results {
            out.extend(r?);
        }
        Ok(out)
    }

    async fn course_vods(&self, course_id: i64) -> Result<Vec<Vod>> {
        let page = self.moodle_page(&format!("{CN2}/course/view.php?id={course_id}")).await?;
        let periods = parse_vod_periods(&page);
        if periods.is_empty() {
            return Ok(vec![]);
        }
        let progress = self.moodle_page_with_user_agent(
            &format!("{CN2}/report/ubcompletion/progress.php?id={course_id}"),
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
        ).await?;
        let contents = self.ws("core_course_get_contents", &[
            ("courseid".into(), course_id.to_string()),
            ("options[0][name]".into(), "modname".into()),
            ("options[0][value]".into(), "vod".into()),
        ]).await?;
        let rows = parse_progress(&progress, &vod_instance_cmids(&contents));
        let now = chrono::Utc::now().timestamp();
        Ok(merge_vod_progress(course_id, periods, &rows, now))
    }
}

fn parse_assignments(api: &Value, states: &HashMap<i64, SubmissionState>) -> Vec<Assignment> {
    let mut out = Vec::new();
    for course in api["courses"].as_array().into_iter().flatten() {
        let course_id = course["id"].as_i64().unwrap_or_default();
        for a in course["assignments"].as_array().into_iter().flatten() {
            let Some(cmid) = a["cmid"].as_i64() else { continue };
            let ts = |key: &str| a[key].as_i64().filter(|t| *t > 0);
            out.push(Assignment {
                id: a["id"].as_i64().unwrap_or_default(),
                config: submit_config(a),
                cmid,
                course_id,
                name: a["name"].as_str().unwrap_or("").to_string(),
                due: ts("duedate"),
                cutoff: ts("cutoffdate"),
                opens: ts("allowsubmissionsfromdate"),
                modified: a["timemodified"].as_i64().unwrap_or_default(),
                intro_html: a["intro"].as_str().unwrap_or("").to_string(),
                attachments: a["introattachments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| Attachment {
                        name: f["filename"].as_str().unwrap_or("").to_string(),
                        size: f["filesize"].as_i64(),
                        mime: f["mimetype"].as_str().map(str::to_string),
                        url: f["fileurl"].as_str().unwrap_or("").to_string(),
                    })
                    .collect(),
                submission: states.get(&cmid).copied().unwrap_or(SubmissionState::Unknown),
            });
        }
    }
    out
}

fn parse_current_term(body: &str) -> Option<AcademicTerm> {
    let doc = Html::parse_document(body);
    let option_selector = sel("option[selected]");
    let selected = |name: &str| {
        let select = doc.select(&sel(&format!("select[name='{name}']"))).next()?;
        let mut options = select.select(&option_selector);
        let option = options.next()?;
        if options.next().is_some() { return None; }
        option.value().attr("value")?.parse::<i32>().ok()
    };
    let term = AcademicTerm { year: selected("year")?, semester: selected("semester")? };
    term.valid().then_some(term)
}

fn parse_courses(list: &Value) -> Vec<Course> {
    let pattern = Regex::new(r"^(\d{4})_(\d{2})_([0-9A-Za-z]+)_([0-9A-Za-z]+)").expect("정규식");
    let suffix = Regex::new(r"\s*\(\d{4}년도.*$").expect("정규식");
    let mut courses = Vec::new();
    for c in list.as_array().into_iter().flatten() {
        let Some(id) = c["id"].as_i64() else { continue };
        let Some(cap) = pattern.captures(c["idnumber"].as_str().unwrap_or("")) else { continue };
        let term = AcademicTerm { year: cap[1].parse().unwrap_or_default(), semester: cap[2].parse().unwrap_or_default() };
        if !term.valid() { continue; }
        let raw_name = c["fullname"].as_str().or_else(|| c["shortname"].as_str()).unwrap_or("");
        courses.push(Course {
            id, name: suffix.replace(raw_name, "").trim().to_string(),
            code: Some(format!("{}-{}", &cap[3], &cap[4])), term: Some(term),
        });
    }
    courses.sort_by(|a, b| {
        let term = |c: &Course| c.term.map(|t| (t.year, t.semester));
        term(b).cmp(&term(a)).then(a.code.cmp(&b.code)).then(a.name.cmp(&b.name))
    });
    courses
}

pub async fn token_owner(client: &reqwest::Client, token: &str) -> Result<(String, String)> {
    let info = ws_call(client, token, "core_webservice_get_site_info", &[]).await?;
    let username = info["username"].as_str().map(str::trim).filter(|u| !u.is_empty());
    let username = username.ok_or_else(|| CoreError::Parse("클래스룸 사용자".into()))?.to_uppercase();
    Ok((username, info["fullname"].as_str().unwrap_or("").to_string()))
}

async fn ws_call(client: &reqwest::Client, token: &str, function: &str, params: &[(String, String)]) -> Result<Value> {
    let mut form: Vec<(String, String)> = vec![
        ("wstoken".into(), token.into()),
        ("wsfunction".into(), function.into()),
        ("moodlewsrestformat".into(), "json".into()),
    ];
    form.extend_from_slice(params);
    let value: Value = client.post(format!("{CN2}/webservice/rest/server.php")).form(&form).send().await?.json().await?;
    if value.get("exception").is_some() {
        let code = value["errorcode"].as_str().unwrap_or("unknown");
        if code == "invalidtoken" {
            return Err(CoreError::ClassroomTokenExpired);
        }
        return Err(CoreError::Upstream(format!("클래스룸 API 오류 ({code})")));
    }
    Ok(value)
}

fn parse_notifications(body: &str) -> Vec<Notification> {
    let doc = Html::parse_document(body);
    let kind_re = Regex::new(r"/timeline/([a-z_]+)").expect("정규식");
    let mut out = Vec::new();
    for media in doc.select(&sel(".media")) {
        let Some(link) = media.select(&sel("a[href]")).next() else { continue };
        let url = link.value().attr("href").unwrap_or("").to_string();
        let heading = media.select(&sel(".media-heading")).next().map(text_of).unwrap_or_default();
        let section = media.select(&sel(".sectionname")).next().map(text_of).unwrap_or_default();
        let course = heading.split(" - ").next().unwrap_or("").trim().to_string();
        let when = media.select(&sel(".timeago")).next().map(text_of).unwrap_or_default();
        let message = media
            .select(&sel(".media-body p"))
            .filter(|p| !p.value().classes().any(|c| c == "timeago"))
            .map(text_of)
            .collect::<Vec<_>>()
            .join(" ");
        let kind = media
            .select(&sel(".media-left img"))
            .next()
            .and_then(|img| img.value().attr("src"))
            .and_then(|src| kind_re.captures(src).map(|c| c[1].to_string()))
            .unwrap_or_else(|| "etc".into());
        if url.starts_with(CN2) && !message.is_empty() {
            out.push(Notification { url, course, section, when, message, kind });
        }
    }
    out
}

fn parse_article(body: &str, cmid: i64, bwid: i64) -> Option<BoardArticle> {
    let doc = Html::parse_document(body);
    let view = doc.select(&sel(".ubboard_view")).next()?;
    let title = view.select(&sel(".subject")).next().map(text_of).filter(|t| !t.is_empty())?;
    let field = |css: &str| {
        view.select(&sel(css))
            .next()
            .map(|el| text_excluding(el, "title").trim_start_matches(':').trim().to_string())
    };
    let posted = field(".info .date").and_then(|d| parse_kst(&format!("{d}:00")));
    let html = view
        .select(&sel(".content .text_to_html"))
        .next()
        .or_else(|| view.select(&sel(".content")).next())
        .map(|el| el.inner_html())
        .unwrap_or_default();
    let attachments = view
        .select(&sel(".files li"))
        .filter_map(|li| {
            let a = li.select(&sel("a[href]")).next()?;
            let url = a.value().attr("href")?;
            if !url.starts_with(&format!("{CN2}/")) {
                return None;
            }
            let mime = li.select(&sel("img[alt]")).next().and_then(|i| i.value().attr("alt")).filter(|m| m.contains('/'));
            Some(Attachment { name: text_of(a), size: None, mime: mime.map(str::to_string), url: url.to_string() })
        })
        .collect();
    Some(BoardArticle {
        cmid,
        bwid,
        board: doc.select(&sel("h2.main")).next().map(text_of).unwrap_or_default(),
        title,
        writer: field(".info .writer").unwrap_or_default(),
        posted,
        html,
        attachments,
    })
}

fn escape_attr(value: &str) -> String {
    value.replace('&', "&amp;").replace('\u{a0}', "&nbsp;").replace('"', "&quot;")
}

fn submit_config(a: &Value) -> SubmitConfig {
    let configs = a["configs"].as_array().cloned().unwrap_or_default();
    let get = |plugin: &str, name: &str| {
        configs
            .iter()
            .find(|c| c["subtype"] == "assignsubmission" && c["plugin"] == plugin && c["name"] == name)
            .and_then(|c| c["value"].as_str().map(str::to_string))
    };
    SubmitConfig {
        files: get("file", "enabled").as_deref() == Some("1"),
        max_files: get("file", "maxfilesubmissions").and_then(|v| v.parse().ok()).unwrap_or(1),
        max_bytes: get("file", "maxsubmissionsizebytes").and_then(|v| v.parse().ok()).unwrap_or(0),
        text: get("onlinetext", "enabled").as_deref() == Some("1"),
        drafts: a["submissiondrafts"].as_i64() == Some(1),
        statement: a["requiresubmissionstatement"].as_i64() == Some(1),
    }
}

fn submission_state(text: &str) -> SubmissionState {
    let low = text.to_lowercase();
    if text.contains("초안") || text.contains("임시저장") || text.contains("임시 저장") || low.contains("draft")
        || text.contains("미제출") || text.contains("제출 안 함") || low.contains("no submission")
        || low.contains("no attempt") || low.contains("not submitted")
    {
        SubmissionState::NotSubmitted
    } else if text.contains("제출 완료") || low.contains("submitted") {
        SubmissionState::Submitted
    } else {
        SubmissionState::Unknown
    }
}

fn parse_submission_states(body: &str) -> Vec<(i64, SubmissionState)> {
    let doc = Html::parse_document(body);
    let id_re = Regex::new(r"[?&]id=(\d+)").expect("정규식");
    let mut out = Vec::new();
    for tr in doc.select(&sel("table tr")) {
        let Some(link) = tr.select(&sel("a[href*='/mod/assign/view.php']")).next() else { continue };
        let cells: Vec<String> = tr.select(&sel("td")).map(text_of).collect();
        let Some(cmid) = link.value().attr("href").and_then(|h| id_re.captures(h)).and_then(|c| c[1].parse().ok()) else {
            continue;
        };
        if cells.len() >= 4 {
            out.push((cmid, submission_state(&cells[3])));
        }
    }
    out
}

struct VodPeriod {
    cmid: Option<i64>,
    name: String,
    start: i64,
    end: i64,
    late_until: Option<i64>,
}

fn parse_vod_periods(body: &str) -> Vec<VodPeriod> {
    let doc = Html::parse_document(body);
    let period = Regex::new(
        r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}) ~ (\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})(?: \(기간 외 시청 : (\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\))?",
    )
    .expect("정규식");
    let mut out = Vec::new();
    for li in doc.select(&sel("li.activity.vod")) {
        let Some(name_el) = li.select(&sel(".instancename")).next() else { continue };
        let name = text_excluding(name_el, "accesshide");
        let text = text_of(li);
        let Some(cap) = period.captures(&text) else { continue };
        let (Some(start), Some(end)) = (parse_kst(&cap[1]), parse_kst(&cap[2])) else { continue };
        let cmid = li.value().attr("id").and_then(|id| id.strip_prefix("module-")).and_then(|n| n.parse().ok());
        out.push(VodPeriod { cmid, name, start, end, late_until: cap.get(3).and_then(|m| parse_kst(m.as_str())) });
    }
    out
}

struct VodProgress {
    required: String,
    watched: String,
    mark: String,
}

fn vod_instance_cmids(contents: &Value) -> HashMap<i64, i64> {
    contents.as_array().into_iter().flatten()
        .flat_map(|section| section["modules"].as_array().into_iter().flatten())
        .filter(|module| module["modname"].as_str() == Some("vod"))
        .filter_map(|module| {
            let instance = module["instance"].as_i64().filter(|id| *id > 0)?;
            let cmid = module["id"].as_i64().filter(|id| *id > 0)?;
            Some((instance, cmid))
        })
        .collect()
}

fn parse_progress(body: &str, instance_cmids: &HashMap<i64, i64>) -> HashMap<i64, VodProgress> {
    let doc = Html::parse_document(body);
    let time = Regex::new(r"^\d+:\d{2}").expect("정규식");
    let module = sel("[data-modname='vod'][data-modid]");
    let td = sel("td");
    let mut rows = HashMap::new();
    for tr in doc.select(&sel("table tr")) {
        let Some(instance) = tr.select(&module).next()
            .and_then(|el| el.value().attr("data-modid"))
            .and_then(|id| id.parse::<i64>().ok()) else { continue };
        let Some(&cmid) = instance_cmids.get(&instance) else { continue };
        let cells: Vec<_> = tr.select(&td).collect();
        let Some(detail) = cells.len().checked_sub(3).map(|start| &cells[start..]) else { continue };
        let required = text_of(detail[0]);
        if !time.is_match(&required) { continue; }
        rows.insert(cmid, VodProgress {
            required,
            watched: text_excluding(detail[1], "track_detail"),
            mark: text_of(detail[2]),
        });
    }
    rows
}

fn merge_vod_progress(course_id: i64, periods: Vec<VodPeriod>, rows: &HashMap<i64, VodProgress>, now: i64) -> Vec<Vod> {
    periods.into_iter().map(|p| {
        let row = p.cmid.and_then(|cmid| rows.get(&cmid));
        let mark = row.map(|r| r.mark.clone());
        let state = vod_state(mark.as_deref(), p.start, p.end, now);
        Vod {
            cmid: p.cmid,
            course_id,
            name: p.name,
            start: p.start,
            end: p.end,
            late_until: p.late_until,
            required: row.map(|r| r.required.clone()),
            watched: row.map(|r| r.watched.clone()),
            mark,
            state,
        }
    }).collect()
}

fn vod_state(mark: Option<&str>, start: i64, end: i64, now: i64) -> VodState {
    match mark.map(str::trim) {
        Some("O") => VodState::Done,
        Some(m) if m.ends_with("100%") => VodState::Done,
        Some("▲") => VodState::Partial,
        _ if now > end => VodState::Missed,
        _ if now < start => VodState::Upcoming,
        _ => VodState::Todo,
    }
}
