use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

use crate::{auth::CurrentUser, calendar, error::ApiError, state::Shared};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub meal_place: String,
    pub timetable_display: String,
    pub alert_leads: Vec<i32>,
    pub updated_at: i64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { meal_place: "dorm".into(), timetable_display: "fit".into(), alert_leads: vec![60], updated_at: 0 }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Changes {
    meal_place: Option<String>,
    timetable_display: Option<String>,
    alert_leads: Option<Vec<i32>>,
}

pub async fn load(db: &PgPool, uid: i64) -> sqlx::Result<Preferences> {
    Ok(sqlx::query_as("select meal_place,timetable_display,alert_leads,floor(extract(epoch from updated_at)*1000)::bigint as updated_at from account_preferences where user_id=$1")
        .bind(uid).fetch_optional(db).await?.unwrap_or_default())
}

pub async fn get(State(st): State<Shared>, user: CurrentUser) -> Result<Json<Preferences>, ApiError> {
    Ok(Json(load(&st.db, user.session.user_id).await?))
}

pub async fn patch(State(st): State<Shared>, user: CurrentUser, Json(mut changes): Json<Changes>) -> Result<Json<Preferences>, ApiError> {
    if changes.meal_place.as_deref().is_some_and(|v| !matches!(v, "dorm" | "staff"))
        || changes.timetable_display.as_deref().is_some_and(|v| !matches!(v, "full" | "fit"))
        || changes.alert_leads.as_deref().is_some_and(|v| !calendar::valid_alert_leads(v)) {
        return Err(ApiError::bad_request("설정값을 확인해 주세요."));
    }
    if let Some(leads) = &mut changes.alert_leads {
        leads.sort_unstable_by(|a, b| b.cmp(a));
    }
    let uid = user.session.user_id;
    let mut tx = st.db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-uid).execute(&mut *tx).await?;
    let saved = sqlx::query_as("insert into account_preferences(user_id,meal_place,timetable_display,alert_leads)
        values($1,coalesce($2,'dorm'),coalesce($3,'fit'),coalesce($4,array[60]))
        on conflict(user_id) do update set meal_place=coalesce($2,account_preferences.meal_place),
        timetable_display=coalesce($3,account_preferences.timetable_display),
        alert_leads=coalesce($4,account_preferences.alert_leads),updated_at=clock_timestamp()
        returning meal_place,timetable_display,alert_leads,floor(extract(epoch from updated_at)*1000)::bigint as updated_at")
        .bind(uid).bind(&changes.meal_place).bind(&changes.timetable_display).bind(&changes.alert_leads)
        .fetch_one(&mut *tx).await?;
    if let Some(leads) = &changes.alert_leads {
        sqlx::query("delete from notification_outbox o using notification_devices d
            where o.device_id=d.id and d.user_id=$1 and o.sent_at is null and o.event_key like 'due:%'
            and not (regexp_replace(o.event_key,'^.*:','')::integer=any($2))
            and ((o.payload#>>'{intent,target,kind}'='item' and not exists
                (select 1 from item_alert_leads a where a.user_id=$1 and a.item_key=o.payload#>>'{intent,target,key}'))
            or (o.payload#>>'{intent,target,kind}'='todo' and exists
                (select 1 from todos t where t.user_id=$1 and t.id::text=o.payload#>>'{intent,target,id}' and t.alert_leads is null)))")
            .bind(uid).bind(leads).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(saved))
}
