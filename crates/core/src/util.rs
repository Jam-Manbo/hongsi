use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use scraper::{ElementRef, Selector};

pub(crate) fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("정적 CSS 선택자")
}

pub(crate) fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn text_of(el: ElementRef<'_>) -> String {
    squash(&el.text().collect::<Vec<_>>().join(" "))
}

pub(crate) fn text_excluding(el: ElementRef<'_>, hidden_class: &str) -> String {
    let mut out = String::new();
    for node in el.descendants() {
        let Some(text) = node.value().as_text() else { continue };
        let hidden = node.ancestors().take_while(|a| a.id() != el.id()).any(|a| {
            a.value().as_element().is_some_and(|e| e.classes().any(|c| c == hidden_class))
        });
        if !hidden {
            out.push_str(text);
            out.push(' ');
        }
    }
    squash(&out)
}

pub(crate) fn form_inputs(form: ElementRef<'_>) -> Vec<(String, String)> {
    form.select(&sel("input[name]"))
        .filter_map(|input| {
            let name = input.value().attr("name")?;
            Some((name.to_string(), input.value().attr("value").unwrap_or("").to_string()))
        })
        .collect()
}

pub(crate) fn looks_like_login(body: &str) -> bool {
    body.contains("통합 로그인")
        || body.contains("name=\"USER_ID\"")
        || body.contains("name='USER_ID'")
        || body.contains("name=\"PASSWD\"")
}

pub(crate) fn kst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("KST")
}

pub(crate) fn parse_kst(text: &str) -> Option<i64> {
    let naive = NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    kst().from_local_datetime(&naive).single().map(|d| d.timestamp())
}
