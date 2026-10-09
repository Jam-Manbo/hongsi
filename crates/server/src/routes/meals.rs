use std::time::{Duration, Instant};

use axum::extract::State;
use axum::Json;
use hongsi_core::models::MealDay;

use crate::state::Shared;

use crate::api::ApiResult;

pub(crate) async fn meals(State(st): State<Shared>) -> ApiResult<Vec<MealDay>> {
    let mut cache = st.meals.lock().await;
    if let Some((at, days)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(1800) {
            return Ok(Json(days.clone()));
        }
    }
    let days = hongsi_core::food::fetch_week(&st.http).await?;
    *cache = Some((Instant::now(), days.clone()));
    Ok(Json(days))
}
