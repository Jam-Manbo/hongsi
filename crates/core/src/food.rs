use chrono::{Datelike, NaiveDate, Utc};
use futures::future::join_all;
use regex::Regex;
use reqwest::Client;
use scraper::Html;

use crate::models::{Meal, MealDay, MealPlace};
use crate::util::{kst, sel, squash, text_of};
use crate::{CoreError, Result};

const FOOD: &str = "https://apps.hongik.ac.kr/food/food_m.php";

pub async fn fetch_week(client: &Client) -> Result<Vec<MealDay>> {
    let pages = join_all((1..=5).map(|p| async move {
        let body = client.get(FOOD).query(&[("p", p)]).send().await?.text().await?;
        parse_day(&body)
    }))
    .await;
    let mut days: Vec<MealDay> = pages.into_iter().filter_map(|d| d.ok()).collect();
    if days.is_empty() {
        return Err(CoreError::Parse("학식 메뉴".into()));
    }
    days.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(days)
}

fn parse_day(body: &str) -> Result<MealDay> {
    let doc = Html::parse_document(body);
    let head = doc.select(&sel("thead td.title")).next().ok_or_else(|| CoreError::Parse("학식 날짜".into()))?;
    let weekday = head.select(&sel("strong")).next().map(text_of).unwrap_or_default();
    let date_re = Regex::new(r"\((\d{1,2})월 (\d{1,2})일\)").expect("정규식");
    let head_text = text_of(head);
    let cap = date_re.captures(&head_text).ok_or_else(|| CoreError::Parse("학식 날짜".into()))?;
    let (month, day): (u32, u32) = (cap[1].parse().unwrap_or(1), cap[2].parse().unwrap_or(1));
    let date = infer_date(month, day).ok_or_else(|| CoreError::Parse("학식 날짜".into()))?;

    let mut places: Vec<MealPlace> = Vec::new();
    for tr in doc.select(&sel("tbody tr")) {
        if let Some(place) = tr.select(&sel("td.time strong")).next() {
            places.push(MealPlace { name: text_of(place), meals: vec![] });
            continue;
        }
        let (Some(th), Some(td)) = (tr.select(&sel("th")).next(), tr.select(&sel("td")).next()) else { continue };
        let Some(place) = places.last_mut() else { continue };
        let items: Vec<String> = td.text().map(squash).filter(|t| !t.is_empty()).collect();
        if items.is_empty() {
            continue;
        }
        place.meals.push(parse_meal_header(&text_of(th), items));
    }
    places.retain(|p| !p.meals.is_empty());
    Ok(MealDay { date: date.format("%Y-%m-%d").to_string(), weekday, places })
}

fn parse_meal_header(header: &str, items: Vec<String>) -> Meal {
    let re = Regex::new(r"^(.*?)\s*\((\d{1,2}:\d{2})\s*~\s*(\d{1,2}:\d{2})\)\s*(.*)$").expect("정규식");
    match re.captures(header) {
        Some(c) => Meal {
            name: c[1].trim().to_string(),
            start: Some(c[2].to_string()),
            end: Some(c[3].to_string()),
            price: tidy_price(&c[4]),
            items,
        },
        None => Meal { name: header.to_string(), start: None, end: None, price: None, items },
    }
}

fn tidy_price(raw: &str) -> Option<String> {
    let number = Regex::new(r"(\d+)원").expect("정규식");
    let parts: Vec<String> = raw
        .split(',')
        .map(|part| {
            let part = part.replace("식대", "").replace(':', " ");
            let part = number.replace_all(&part, |c: &regex::Captures| format!("{}원", thousands(&c[1])));
            squash(&part)
        })
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn thousands(digits: &str) -> String {
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn infer_date(month: u32, day: u32) -> Option<NaiveDate> {
    let today = Utc::now().with_timezone(&kst()).date_naive();
    let mut year = today.year();
    if month + 6 < today.month() {
        year += 1;
    } else if month > today.month() + 6 {
        year -= 1;
    }
    NaiveDate::from_ymd_opt(year, month, day)
}
