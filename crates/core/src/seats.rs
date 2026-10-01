use futures::future::join_all;
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html};

use crate::models::{Building, Room, SeatCell, SeatState};
use crate::util::{form_inputs, sel, text_of};
use crate::{CoreError, Result};

pub const BUILDINGS: [(&str, &str, &str); 3] = [
    ("G", "학생회관", "http://203.249.67.222/"),
    ("T", "제4공학관", "http://203.249.65.81/"),
    ("R", "홍문관", "http://223.194.83.66/"),
];

pub fn validity_hours(period: &str) -> i64 {
    match period {
        "exam" => 4,
        "vacation" => 8,
        _ => 6,
    }
}

pub async fn fetch_all(client: &Client) -> Result<Vec<Building>> {
    let results = join_all(BUILDINGS.iter().map(|(id, name, url)| fetch_building(client, id, name, url))).await;
    let buildings: Vec<Building> = results.into_iter().filter_map(|r| r.map_err(|e| tracing::warn!("열람실 조회 실패: {e}")).ok()).collect();
    if buildings.is_empty() {
        return Err(CoreError::Upstream("열람실 서버에 연결하지 못했어요".into()));
    }
    Ok(buildings)
}

async fn get_euc_kr(request: reqwest::RequestBuilder) -> Result<String> {
    let bytes = request.send().await?.bytes().await?;
    Ok(encoding_rs::EUC_KR.decode(&bytes).0.into_owned())
}

async fn fetch_building(client: &Client, id: &str, name: &str, url: &str) -> Result<Building> {
    let root = get_euc_kr(client.get(url)).await?;
    let (summary, mut fields) = {
        let doc = Html::parse_document(&root);
        let fields = doc.select(&sel("form")).next().map(form_inputs).unwrap_or_default();
        (parse_summary(&doc), fields)
    };
    let mut rooms = Vec::new();
    for (index, (room_name, total, used, free)) in summary.into_iter().enumerate() {
        let no = index as u32 + 1;
        let mut form: Vec<(String, String)> = fields.iter().filter(|(k, _)| k != "Roon_no").cloned().collect();
        form.push(("Roon_no".into(), no.to_string()));
        let page = get_euc_kr(client.post(format!("{url}RoomStatus.aspx")).form(&form)).await?;
        let doc = Html::parse_document(&page);
        let next = doc.select(&sel("form")).next().map(form_inputs).unwrap_or_default();
        if !next.is_empty() {
            fields = next;
        }
        rooms.push(Room { no, name: room_name, total, used, free, grid: parse_grid(&doc) });
    }
    Ok(Building { id: id.to_string(), name: name.to_string(), rooms })
}

fn direct_cells(tr: ElementRef<'_>) -> Vec<String> {
    tr.children()
        .filter_map(ElementRef::wrap)
        .filter(|e| e.value().name() == "td")
        .map(text_of)
        .collect()
}

fn parse_summary(doc: &Html) -> Vec<(String, u32, u32, u32)> {
    let mut rows = Vec::new();
    for tr in doc.select(&sel("tr")) {
        let cells = direct_cells(tr);
        if cells.len() == 5 && cells[0] != "계" && cells[4].ends_with('%') {
            let nums: Vec<u32> = cells[1..4].iter().filter_map(|c| c.parse().ok()).collect();
            if nums.len() == 3 {
                rows.push((cells[0].clone(), nums[0], nums[1], nums[2]));
            }
        }
    }
    rows
}

fn parse_grid(doc: &Html) -> Vec<Vec<Option<SeatCell>>> {
    let style = Regex::new(r"^Style(\d+)$").expect("정규식");
    let Some(table) = doc.select(&sel("table.BlackText_Status")).next() else { return vec![] };
    let mut grid: Vec<Vec<Option<SeatCell>>> = Vec::new();
    for tr in table.select(&sel("tr")) {
        let row: Vec<Option<SeatCell>> = tr
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|e| e.value().name() == "td")
            .map(|td| {
                let class = td.value().attr("class").unwrap_or("");
                let cap = style.captures(class)?;
                let no: u32 = text_of(td).parse().ok()?;
                let state = match &cap[1] {
                    "1" => SeatState::Free,
                    "10" => SeatState::Used,
                    _ => SeatState::Blocked,
                };
                Some(SeatCell { no, state })
            })
            .collect();
        grid.push(row);
    }
    trim_grid(grid)
}

fn trim_grid(mut grid: Vec<Vec<Option<SeatCell>>>) -> Vec<Vec<Option<SeatCell>>> {
    while grid.first().is_some_and(|r| r.iter().all(Option::is_none)) {
        grid.remove(0);
    }
    while grid.last().is_some_and(|r| r.iter().all(Option::is_none)) {
        grid.pop();
    }
    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    for row in grid.iter_mut() {
        row.resize_with(width, || None);
    }
    let used_col = |c: usize, g: &Vec<Vec<Option<SeatCell>>>| g.iter().any(|r| r[c].is_some());
    let first = (0..width).find(|&c| used_col(c, &grid)).unwrap_or(0);
    let last = (0..width).rev().find(|&c| used_col(c, &grid)).map(|c| c + 1).unwrap_or(0);
    grid.into_iter().map(|r| r.into_iter().skip(first).take(last.saturating_sub(first)).collect()).collect()
}
