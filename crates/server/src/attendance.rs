use axum::{extract::State, Json};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use hongsi_core::models::{ActiveLecture, AttendanceReceipt, MarkKind};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};

use crate::{auth::CurrentUser, error::ApiError, state::Shared};

fn today() -> NaiveDate { (Utc::now() + Duration::hours(9)).date_naive() }

fn validate(receipt: &AttendanceReceipt) -> Result<NaiveDate, ApiError> {
    let day = today();
    let date = NaiveDate::parse_from_str(&receipt.date, "%Y-%m-%d").ok();
    let at = DateTime::from_timestamp_millis(receipt.confirmed_at);
    let lecture = &receipt.lecture;
    if date != Some(day)
        || !at.is_some_and(|at| at <= Utc::now() + Duration::seconds(60) && (at + Duration::hours(9)).date_naive() == day)
        || !matches!(receipt.kind, MarkKind::Present | MarkKind::Late | MarkKind::Excused)
        || lecture.key.is_empty() || lecture.key.len() > 1000
        || lecture.name.is_empty() || lecture.name.len() > 500
        || lecture.time.len() > 300 || lecture.code.as_ref().is_some_and(|code| code.len() > 32)
        || lecture.key != format!("{}|{}|{}", lecture.code.as_deref().unwrap_or(""), lecture.name, lecture.time)
    {
        return Err(ApiError::bad_request("오늘 출석 처리가 확인된 기록만 공유할 수 있어요"));
    }
    Ok(day)
}

pub async fn save(db: &PgPool, uid: i64, receipt: &AttendanceReceipt) -> Result<(), ApiError> {
    let date = validate(receipt)?;
    let kind = match receipt.kind { MarkKind::Present => "present", MarkKind::Late => "late", MarkKind::Excused => "excused", _ => unreachable!() };
    sqlx::query("insert into attendance_receipts(user_id,school_date,lecture_key,course_code,course_name,lecture_time,kind,confirmed_at) values($1,$2,$3,$4,$5,$6,$7,to_timestamp($8)) on conflict(user_id,school_date,lecture_key) do update set kind=excluded.kind,confirmed_at=excluded.confirmed_at where attendance_receipts.confirmed_at<excluded.confirmed_at")
        .bind(uid).bind(date).bind(&receipt.lecture.key).bind(&receipt.lecture.code).bind(&receipt.lecture.name).bind(&receipt.lecture.time).bind(kind).bind(receipt.confirmed_at as f64 / 1000.0)
        .execute(db).await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ReceiptInput { account: String, receipt: AttendanceReceipt }

pub async fn record(State(st): State<Shared>, user: CurrentUser, Json(body): Json<ReceiptInput>) -> Result<Json<Value>, ApiError> {
    if body.account != user.session.student_id { return Err(ApiError::conflict("현재 계정의 출석 기록만 공유할 수 있어요")); }
    save(&st.db, user.session.user_id, &body.receipt).await?;
    Ok(Json(json!({"ok":true})))
}

#[derive(FromRow)]
struct Row { school_date: NaiveDate, lecture_key: String, course_code: Option<String>, course_name: String, lecture_time: String, kind: String, confirmed_at: DateTime<Utc> }

pub async fn list(State(st): State<Shared>, user: CurrentUser) -> Result<Json<Vec<AttendanceReceipt>>, ApiError> {
    let rows: Vec<Row> = sqlx::query_as("select school_date,lecture_key,course_code,course_name,lecture_time,kind,confirmed_at from attendance_receipts where user_id=$1 and school_date=(now() at time zone 'Asia/Seoul')::date order by confirmed_at")
        .bind(user.session.user_id).fetch_all(&st.db).await?;
    Ok(Json(rows.into_iter().map(|row| AttendanceReceipt {
        lecture: ActiveLecture { key:row.lecture_key, code:row.course_code, name:row.course_name, time:row.lecture_time },
        date:row.school_date.to_string(), confirmed_at:row.confirmed_at.timestamp_millis(),
        kind:match row.kind.as_str() { "present"=>MarkKind::Present,"late"=>MarkKind::Late,_=>MarkKind::Excused },
    }).collect()))
}

pub async fn clean(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query("delete from attendance_receipts where school_date<(now() at time zone 'Asia/Seoul')::date").execute(db).await?;
    Ok(())
}
