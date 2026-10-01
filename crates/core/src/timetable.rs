use std::collections::HashMap;

use regex::Regex;
use scraper::{ElementRef, Html};

use crate::models::{ClassSlot, Timetable};
use crate::util::{looks_like_login, sel, squash, text_of};
use crate::{CoreError, Result, SchoolSession};

const GRID: &str = "https://cn.hongik.ac.kr/stud/C/02000/02040.jsp";
const LIST: &str = "https://cn.hongik.ac.kr/stud/C/02000/02030.jsp";
const MAIN: &str = "https://cn.hongik.ac.kr/stud/include/main.jsp";
const WEEKDAYS: [char; 7] = ['월', '화', '수', '목', '금', '토', '일'];

impl SchoolSession {
    pub async fn timetable(&self) -> Result<Timetable> {
        let grid = self.get_text(GRID, Some(MAIN)).await?;
        if needs_login(&grid) {
            return Err(CoreError::SessionExpired);
        }
        let mut slots = parse_grid(&grid)?;
        match self.post_form_text(LIST, &[], Some(GRID)).await {
            Ok(list) if !needs_login(&list) => {
                let codes = parse_codes(&list);
                for slot in &mut slots {
                    slot.code = codes.get(&slot.name).cloned();
                }
            }
            Ok(_) => tracing::warn!("시간표 목록(학수번호)이 로그인 화면으로 넘어감"),
            Err(e) => tracing::warn!(error = %e, "시간표 목록(학수번호)을 읽지 못함"),
        }
        Ok(Timetable { slots })
    }
}

fn needs_login(body: &str) -> bool {
    body.contains("통합로그인 후") || body.contains("login.do?Refer") || looks_like_login(body)
}

fn parse_grid(body: &str) -> Result<Vec<ClassSlot>> {
    let doc = Html::parse_document(body);
    let table = doc
        .select(&sel("table"))
        .find(|t| t.select(&sel("th")).next().is_some_and(|th| text_of(th).contains("교시")))
        .ok_or_else(|| CoreError::Parse("시간표 표".into()))?;
    let days: Vec<Option<u32>> = table
        .select(&sel("th"))
        .map(|th| text_of(th).chars().next().and_then(|c| WEEKDAYS.iter().position(|w| *w == c)).map(|i| i as u32))
        .collect();

    let period_re = Regex::new(r"(\d+)\s*교시").expect("정규식");
    let time_re = Regex::new(r"(\d{1,2}):(\d{2})").expect("정규식");
    let mut cells: Vec<(u32, u32, String, String, Option<String>)> = Vec::new();
    for tr in table.select(&sel("tr")) {
        let tds: Vec<ElementRef> = tr.select(&sel("td")).collect();
        let Some(label) = tds.first().map(|td| text_of(*td)) else { continue };
        let Some(period) = period_re.captures(&label).and_then(|c| c[1].parse::<u32>().ok()) else { continue };
        let Some(t) = time_re.captures(&label) else { continue };
        let start = format!("{:02}:{}", t[1].parse::<u32>().unwrap_or(0), &t[2]);
        for (i, td) in tds.iter().enumerate().skip(1) {
            let Some(Some(weekday)) = days.get(i) else { continue };
            if let Some((name, room)) = parse_cell(*td) {
                cells.push((*weekday, period, start.clone(), name, room));
            }
        }
    }

    cells.sort_by_key(|c| (c.0, c.1));
    let mut slots: Vec<ClassSlot> = Vec::new();
    for (weekday, period, start, name, room) in cells {
        if let Some(last) = slots.last_mut() {
            let follows = last.periods.last().map(|p| p + 1) == Some(period);
            if last.weekday == weekday && last.name == name && follows {
                last.periods.push(period);
                continue;
            }
        }
        slots.push(ClassSlot { code: None, name, weekday, start, periods: vec![period], room });
    }
    Ok(slots)
}

fn parse_cell(td: ElementRef<'_>) -> Option<(String, Option<String>)> {
    let parts: Vec<String> = td
        .children()
        .filter_map(|n| n.value().as_text().map(|t| squash(t)))
        .filter(|t| !t.is_empty())
        .collect();
    let name = parts.first()?.clone();
    let room = parts
        .get(1..)
        .and_then(|rest| rest.last())
        .and_then(|r| r.strip_prefix('(').and_then(|r| r.strip_suffix(')')))
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    Some((name, room))
}

fn parse_codes(body: &str) -> HashMap<String, String> {
    let doc = Html::parse_document(body);
    let code_re = Regex::new(r"^\d{5,7}-\d{1,3}$").expect("정규식");
    let mut codes = HashMap::new();
    for tr in doc.select(&sel("tr")) {
        let tds: Vec<ElementRef> = tr.select(&sel("td")).collect();
        let (Some(code_td), Some(name_td)) = (tds.first(), tds.get(1)) else { continue };
        let code = text_of(*code_td);
        if !code_re.is_match(&code) {
            continue;
        }
        let name = name_td.select(&sel("a")).next().and_then(|a| {
            a.children().filter_map(|n| n.value().as_text().map(|t| squash(t))).find(|t| !t.is_empty())
        });
        if let Some(name) = name {
            codes.insert(name, code);
        }
    }
    codes
}
