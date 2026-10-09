use std::collections::BTreeMap;

use scraper::{ElementRef, Html};

use crate::classroom::VodPeriod;
use crate::models::{AttendanceCourse, AttendanceMark, AttendanceSummary, AttendanceWeek, Course, MarkKind};
use crate::util::{sel, text_of};
use crate::{CoreError, Result};

pub(crate) fn parse(course: &Course, body: &str, periods: &[VodPeriod], now: i64) -> Result<AttendanceCourse> {
    let doc = Html::parse_document(body);
    let table = doc.select(&sel("table.user_progress_table")).next()
        .filter(|t| t.select(&sel("th")).any(|h| text_of(h).contains("시청인정")))
        .ok_or_else(|| CoreError::Parse("사이버 출석부".into()))?;
    let mut weeks: BTreeMap<u32, Vec<AttendanceMark>> = BTreeMap::new();
    let mut current_week = None;
    let mut remaining = 0usize;
    let mut summary = AttendanceSummary::default();
    for tr in table.select(&sel("tbody > tr")) {
        let cells: Vec<_> = tr.children().filter_map(ElementRef::wrap).filter(|e| e.value().name() == "td").collect();
        if cells.is_empty() { continue; }
        let cells = if cells.len() == 5 {
            let week = text_of(cells[0]).trim_end_matches("주차").trim().parse::<u32>()
                .ok().filter(|w| *w > 0 && *w <= 60)
                .ok_or_else(|| CoreError::Parse("사이버 출석부 주차".into()))?;
            current_week = Some(week);
            remaining = cells[0].value().attr("rowspan").and_then(|s| s.parse().ok()).unwrap_or(1);
            weeks.entry(week).or_default();
            &cells[1..]
        } else if cells.len() == 4 && remaining > 0 {
            &cells[..]
        } else {
            return Err(CoreError::Parse("사이버 출석부 영상 행".into()));
        };
        remaining = remaining.saturating_sub(1);
        let week = current_week.ok_or_else(|| CoreError::Parse("사이버 출석부 주차".into()))?;
        let video = cells[0].select(&sel("img")).any(|img| img.value().attr("src").is_some_and(|s| s.contains("/vod/")))
            || tr.select(&sel("[data-modname='vod']")).next().is_some();
        if !video { continue; }
        let title = text_of(cells[0]);
        if title.is_empty() { return Err(CoreError::Parse("사이버 출석부 영상 제목".into())); }
        let candidates: Vec<_> = periods.iter().filter(|p| p.week == Some(week) && p.name == title).collect();
        let period = candidates.first().copied().filter(|p| candidates.iter()
            .all(|other| p.cmid == other.cmid && p.start == other.start && p.end == other.end));
        let school_mark = text_of(cells[3]);
        let kind = classify(&school_mark, period, now);
        match kind {
            MarkKind::Present => summary.present += 1,
            MarkKind::Late => summary.late += 1,
            MarkKind::Absent => summary.absent += 1,
            MarkKind::Excused => summary.excused += 1,
            MarkKind::None => summary.none += 1,
            MarkKind::Planned => summary.planned += 1,
            MarkKind::Other => {},
        }
        weeks.get_mut(&week).expect("주차 생성").push(AttendanceMark {
            title: Some(title),
            period: period.and_then(|p| Some(format!("{} ~ {}", format_time(p.start, "%-m/%-d %H:%M")?, format_time(p.end, "%-m/%-d %H:%M")?))),
            date: period.and_then(|p| format_time(p.end, "%-m/%-d")).unwrap_or_default(), mark: school_mark, kind,
        });
    }
    if weeks.is_empty() { return Err(CoreError::Parse("사이버 출석부 주차 목록".into())); }
    Ok(AttendanceCourse {
        cyber: true, code: course.code.clone().unwrap_or_default(), name: course.name.clone(), published: true, notice: None,
        weeks: weeks.into_iter().map(|(week, sessions)| AttendanceWeek { week, sessions }).collect(), summary,
    })
}

fn classify(mark: &str, period: Option<&VodPeriod>, now: i64) -> MarkKind {
    match mark.trim() {
        "O" | "○" | "출석" => MarkKind::Present,
        "▲" | "지각" => MarkKind::Late,
        "공결" => MarkKind::Excused,
        "" | "-" | "X" | "×" | "결석" => {
            if period.is_some_and(|p| now < p.start) { MarkKind::Planned }
            else if period.is_some_and(|p| now <= p.end) { MarkKind::None }
            else if period.is_some_and(|p| now > p.end) || matches!(mark.trim(), "X" | "×" | "결석") { MarkKind::Absent }
            else { MarkKind::None }
        },
        _ => MarkKind::Other,
    }
}

fn format_time(timestamp: i64, format: &str) -> Option<String> {
    let at = chrono::DateTime::from_timestamp(timestamp, 0)? + chrono::Duration::hours(9);
    Some(at.format(format).to_string())
}
