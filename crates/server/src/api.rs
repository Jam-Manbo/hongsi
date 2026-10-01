use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post, put};
use axum::{Json, Router};
use chrono::{Duration as Span, TimeZone, Utc};
use hongsi_core::models::{ActiveLectures, AttendanceCourse, BoardArticle, MealDay, ModuleContents, Timetable};
use hongsi_core::seats::validity_hours;
use hongsi_core::SchoolSession;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, CurrentUser};
use crate::calendar::{self, CalendarData};
use crate::db::{self, NewSeatSession, RecentSeat, SeatSession};
use crate::error::ApiError;
use crate::sessions::{UserSession, REMEMBER_DAYS, SESSION_HOURS};
use crate::state::{SeatSnapshot, Shared};
use crate::todos::{self, Todo, TodoInput};
use crate::vault;

type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .route("/api/push/status", get(crate::background::status))
        .route("/api/push/device", post(crate::background::register).delete(crate::background::disable))
        .route("/api/push/renew", post(crate::background::renew))
        .route("/api/push/preferences", put(crate::background::preferences))
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/app-update", get(crate::updates::latest))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/device", post(device_login))
        .route("/api/me", get(me))
        .route("/api/me/avatar", get(avatar))
        .route("/api/files/{cmid}/{index}", get(download_file))
        .route("/api/modules/{cmid}", get(module_get))
        .route("/api/modules/{cmid}/files/{index}", get(module_file))
        .route("/api/board/{cmid}/{bwid}", get(board_get))
        .route("/api/board/{cmid}/{bwid}/files/{index}", get(board_file))
        .route("/api/calendar/items/{key}/verify", post(verify_item))
        .route("/api/notifications", get(notifications))
        .route("/api/notices/seen", get(notices_seen_get).post(notices_seen_add))
        .route(
            "/api/assign/{cmid}/submission",
            get(submission_get).post(submission_post).layer(DefaultBodyLimit::max(110 * 1024 * 1024)),
        )
        .route("/api/todos", get(todo_list).post(todo_create))
        .route("/api/todos/{id}", put(todo_update).delete(todo_delete))
        .route("/api/todos/{id}/done", post(todo_done))
        .route("/api/attendance/active", get(attendance_active))
        .route("/api/attendance/submit", post(attendance_submit))
        .route("/api/attendance/status", get(attendance_status))
        .route("/api/attendance/course", get(attendance_course))
        .route("/api/timetable", get(timetable))
        .route("/api/calendar", get(calendar_data))
        .route("/api/calendar/state", post(calendar_state))
        .route("/api/calendar/items/{key}/done", put(set_done))
        .route("/api/calendar/items/{key}/alert", put(set_alert))
        .route("/api/meals", get(meals))
        .route("/api/seats", get(seats))
        .route("/api/seats/recent", get(seats_recent))
        .route("/api/seats/session", get(seat_session).post(seat_start).patch(seat_adjust))
        .route("/api/seats/session/extend", post(seat_extend))
        .route("/api/seats/session/end", post(seat_end))
}

async fn api_not_found() -> (axum::http::StatusCode, Json<Value>) {
    (axum::http::StatusCode::NOT_FOUND, Json(json!({ "error": {
        "code": "api_not_found", "message": "서버에서 이 기능을 찾지 못했어요. 서버 버전을 확인해 주세요."
    } })))
}


#[derive(Deserialize)]
struct LoginBody {
    id: String,
    password: String,
    #[serde(default)]
    remember: bool,
}

async fn login(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<LoginBody>) -> Result<Response, ApiError> {
    let id = body.id.trim().to_uppercase();
    if id.is_empty() || body.password.is_empty() || id.len() > 32 {
        return Err(ApiError::bad_request("학번과 비밀번호를 입력해 주세요"));
    }
    st.login_attempts.check(&auth::user_key(&st.pepper, &id))?;
    let school = SchoolSession::login(&id, &body.password).await?;
    let name = match school.profile().await {
        Ok(p) => p.name,
        Err(e) => {
            tracing::warn!("프로필 조회 실패: {e}");
            String::new()
        }
    };
    let user_id = db::upsert_user(&st.db, &auth::user_key(&st.pepper, &id)).await?;
    let sealed = vault::Sealed { name: name.clone(), student_id: id.clone(), cookies: school.sso_cookies().to_vec() };
    let session = UserSession::new(Some(school), user_id, name.clone(), id.clone(), body.remember);
    let expires_at = session.expires_at;
    let token = st.sessions.insert(session);
    if body.remember {
        let (nonce, ciphertext) = vault::seal(&st.pepper, &token, &sealed)
            .ok_or_else(|| ApiError::bad_request("로그인 상태를 저장하지 못했어요"))?;
        if let Err(error) = db::save_remembered(&st.db, &vault::token_hash(&token), user_id, &nonce, &ciphertext, expires_at).await {
            st.sessions.remove(&token);
            return Err(error.into());
        }
    }

    let mut payload = json!({ "profile": { "name": name } });
    if headers.get("x-client").and_then(|v| v.to_str().ok()) == Some("tauri") {
        payload["token"] = json!(token);
    }
    let secure = if st.config.cookie_secure { "; Secure" } else { "" };
    let max_age = if body.remember { REMEMBER_DAYS as i64 * 86_400 } else { SESSION_HOURS * 3600 };
    let cookie = format!("{}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{secure}", auth::COOKIE);
    Ok(([(header::SET_COOKIE, cookie)], Json(payload)).into_response())
}

async fn logout(State(st): State<Shared>, user: CurrentUser) -> Response {
    if crate::background::revoke_login(&st.db, &user.token).await.is_err() {
        tracing::warn!("로그아웃 알림 해제 실패");
    }
    st.sessions.remove(&user.token);
    if let Err(e) = db::delete_remembered(&st.db, &vault::token_hash(&user.token)).await {
        tracing::warn!("보관 세션 삭제 실패: {e}");
    }
    let cookie = format!("{}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0", auth::COOKIE);
    ([(header::SET_COOKIE, cookie)], Json(json!({ "ok": true }))).into_response()
}

async fn me(user: CurrentUser) -> ApiResult<Value> {
    let Some(school) = user.session.school.as_ref() else {
        return Ok(Json(json!({
            "profile": { "name": user.session.name, "hasPicture": false, "studentId": user.session.student_id },
            "remembered": false,
        })));
    };
    let profile = school.profile().await?;
    Ok(Json(json!({
        "profile": {
            "name": if profile.name.is_empty() { user.session.name.clone() } else { profile.name },
            "hasPicture": profile.has_picture,
            "studentId": user.session.student_id,
            "department": profile.department,
        },
        "remembered": user.session.remembered,
    })))
}

#[derive(Deserialize)]
struct DeviceLoginBody {
    token: String,
}

async fn device_login(State(st): State<Shared>, Json(b): Json<DeviceLoginBody>) -> ApiResult<Value> {
    let token = b.token.trim();
    if token.is_empty() || token.len() > 128 || !token.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ApiError::bad_request("잘못된 요청이에요"));
    }
    let (student_id, name) = hongsi_core::classroom::token_owner(&st.http, token).await.map_err(|e| match e {
        hongsi_core::CoreError::SessionExpired => ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "login_rejected",
            "클래스룸 로그인을 확인하지 못했어요",
        ),
        other => ApiError::from(other),
    })?;
    let user_id = db::upsert_user(&st.db, &auth::user_key(&st.pepper, &student_id)).await?;
    let session = st.sessions.insert(UserSession::new(None, user_id, name.clone(), student_id, false));
    tracing::info!("앱 기기 로그인");
    Ok(Json(json!({ "token": session, "profile": { "name": name } })))
}

async fn avatar(user: CurrentUser) -> Result<Response, ApiError> {
    let (mime, bytes) = user.session.school()?.avatar().await?.ok_or_else(|| ApiError::not_found("프로필 사진이 없어요"))?;
    if !mime.starts_with("image/") {
        return Err(ApiError::not_found("프로필 사진이 없어요"));
    }
    Ok(([
        (header::CONTENT_TYPE, mime),
        (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
        (header::CONTENT_SECURITY_POLICY, "sandbox; default-src 'none'; style-src 'unsafe-inline'".to_string()),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
    ], bytes).into_response())
}


#[derive(Deserialize)]
struct FileQuery {
    inline: Option<u8>,
}

fn encode_filename(name: &str) -> String {
    name.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn download_file(
    user: CurrentUser,
    Path((cmid, index)): Path<(i64, usize)>,
    Query(q): Query<FileQuery>,
) -> Result<Response, ApiError> {
    let key = format!("assign:{cmid}");
    let cached = {
        let cache = user.session.calendar_cache.lock().await;
        cache.as_ref().and_then(|(_, d)| d.items.iter().find(|i| i.key == key).and_then(|i| i.attachments.get(index).cloned()))
    };
    let file = match cached {
        Some(f) => f,
        None => {
            let courses = user.session.school()?.courses().await?;
            let assignments = user.session.school()?.assignments(&courses).await?;
            assignments
                .into_iter()
                .find(|a| a.cmid == cmid)
                .and_then(|a| a.attachments.into_iter().nth(index))
                .ok_or_else(|| ApiError::not_found("파일을 찾지 못했어요"))?
        }
    };
    let (mime, bytes) = user.session.school()?.download(&file.url, 100 * 1024 * 1024).await?;
    file_response(&file.name, &mime, bytes, q.inline.unwrap_or(0) == 1)
}

fn file_response(name: &str, mime: &str, bytes: Vec<u8>, inline: bool) -> Result<Response, ApiError> {
    let media_type = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let inline = inline && matches!(media_type.as_str(),
        "application/pdf" | "text/plain" | "image/jpeg" | "image/png" | "image/gif"
        | "image/webp" | "image/avif" | "image/bmp" | "image/x-icon"
        | "audio/mpeg" | "audio/mp4" | "audio/ogg" | "audio/wav" | "audio/webm"
        | "video/mp4" | "video/ogg" | "video/webm"
    );
    let name = name.rsplit('/').next().unwrap_or(name);
    let disposition = format!("{}; filename*=UTF-8''{}", if inline { "inline" } else { "attachment" }, encode_filename(name));
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    headers.insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).map_err(|_| ApiError::bad_request("파일 이름 오류"))?);
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    Ok(response)
}


async fn module_get(user: CurrentUser, Path(cmid): Path<i64>) -> ApiResult<ModuleContents> {
    Ok(Json(user.session.school()?.module_contents(cmid).await?))
}

async fn module_file(
    user: CurrentUser,
    Path((cmid, index)): Path<(i64, usize)>,
    Query(q): Query<FileQuery>,
) -> Result<Response, ApiError> {
    let school = user.session.school()?;
    let file = school
        .module_contents(cmid)
        .await?
        .files
        .into_iter()
        .nth(index)
        .ok_or_else(|| ApiError::not_found("파일을 찾지 못했어요"))?;
    let (mime, bytes) = school.download(&file.url, 100 * 1024 * 1024).await?;
    file_response(&file.name, &mime, bytes, q.inline.unwrap_or(0) == 1)
}

async fn board_get(user: CurrentUser, Path((cmid, bwid)): Path<(i64, i64)>) -> ApiResult<BoardArticle> {
    Ok(Json(user.session.school()?.board_article(cmid, bwid).await?))
}

async fn board_file(
    user: CurrentUser,
    Path((cmid, bwid, index)): Path<(i64, i64, usize)>,
    Query(q): Query<FileQuery>,
) -> Result<Response, ApiError> {
    let school = user.session.school()?;
    let file = school
        .board_article(cmid, bwid)
        .await?
        .attachments
        .into_iter()
        .nth(index)
        .ok_or_else(|| ApiError::not_found("파일을 찾지 못했어요"))?;
    let (mime, bytes) = school.download(&file.url, 100 * 1024 * 1024).await?;
    file_response(&file.name, &mime, bytes, q.inline.unwrap_or(0) == 1)
}


#[derive(Deserialize)]
struct VerifyQuery {
    course: i64,
}

async fn verify_item(user: CurrentUser, Path(key): Path<String>, Query(q): Query<VerifyQuery>) -> ApiResult<Value> {
    use hongsi_core::models::{SubmissionState, VodState};
    let (kind, id) = key.split_once(':').ok_or_else(|| ApiError::bad_request("잘못된 항목이에요"))?;
    let cmid: i64 = id.parse().map_err(|_| ApiError::bad_request("잘못된 항목이에요"))?;
    let status: &'static str = match kind {
        "assign" => match user.session.school()?.submission_state(q.course, cmid).await? {
            SubmissionState::Submitted => "submitted",
            SubmissionState::NotSubmitted => "not_submitted",
            SubmissionState::Unknown => "unknown",
        },
        "vod" => match user.session.school()?.vod_state(q.course, cmid).await? {
            Some(VodState::Done) => "done",
            Some(VodState::Partial) => "partial",
            Some(VodState::Missed) => "missed",
            Some(VodState::Todo) => "todo",
            Some(VodState::Upcoming) => "upcoming",
            None => "unknown",
        },
        _ => return Err(ApiError::bad_request("과제와 강의만 확인할 수 있어요")),
    };
    if status != "unknown" {
        if let Some((_, data)) = user.session.calendar_cache.lock().await.as_mut() {
            for item in data.items.iter_mut().filter(|i| i.key == key) {
                item.status = status;
                if item.done_override.is_none() {
                    item.done = status == "submitted" || status == "done";
                }
            }
        }
    }
    Ok(Json(json!({ "key": key, "status": status, "finished": status == "submitted" || status == "done" })))
}


async fn attendance_active(user: CurrentUser) -> ApiResult<ActiveLectures> {
    let mut cache = user.session.lectures_cache.lock().await;
    if let Some((at, data)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(3) {
            return Ok(Json(data.clone()));
        }
    }
    let data = user.session.school()?.active_lectures().await?;
    *cache = Some((Instant::now(), data.clone()));
    Ok(Json(data))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmitBody {
    lecture_key: String,
    code: String,
    latitude: f64,
    longitude: f64,
}

async fn attendance_submit(user: CurrentUser, Json(b): Json<SubmitBody>) -> ApiResult<Value> {
    let code = b.code.trim();
    if code.is_empty() || code.len() > 12 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ApiError::bad_request("인증번호를 확인해 주세요"));
    }
    if !(-90.0..=90.0).contains(&b.latitude) || !(-180.0..=180.0).contains(&b.longitude) {
        return Err(ApiError::bad_request("위치 정보가 올바르지 않아요"));
    }
    let message = user.session.school()?.submit_attendance(&b.lecture_key, code, b.latitude, b.longitude).await?;
    *user.session.lectures_cache.lock().await = None;
    user.session.course_cache.lock().await.clear();
    Ok(Json(json!({ "message": message })))
}

async fn attendance_status(user: CurrentUser) -> ApiResult<Vec<AttendanceCourse>> {
    Ok(Json(user.session.school()?.attendance_status().await?))
}

#[derive(Deserialize)]
struct CourseQuery {
    code: String,
}

fn valid_course_code(code: &str) -> bool {
    code.len() <= 16
        && code.split_once('-').is_some_and(|(haksu, bunban)| {
            !haksu.is_empty()
                && !bunban.is_empty()
                && haksu.chars().all(|c| c.is_ascii_digit())
                && bunban.chars().all(|c| c.is_ascii_digit())
        })
}

async fn attendance_course(user: CurrentUser, Query(q): Query<CourseQuery>) -> ApiResult<AttendanceCourse> {
    let code = q.code.trim();
    if !valid_course_code(code) {
        return Err(ApiError::bad_request("과목 코드가 올바르지 않아요"));
    }
    let mut cache = user.session.course_cache.lock().await;
    if let Some((at, data)) = cache.get(code) {
        if at.elapsed() < Duration::from_secs(60) {
            return Ok(Json(data.clone()));
        }
    }
    let data = user.session.school()?.attendance_course(code).await?;
    cache.insert(code.to_string(), (Instant::now(), data.clone()));
    Ok(Json(data))
}

async fn timetable(user: CurrentUser, Query(q): Query<RefreshQuery>) -> ApiResult<Timetable> {
    let max_age = if q.refresh == Some(1) { 60 } else { 6 * 3600 };
    let mut cache = user.session.timetable_cache.lock().await;
    if let Some((at, data)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(max_age) {
            return Ok(Json(data.clone()));
        }
    }
    let data = user.session.school()?.timetable().await?;
    *cache = Some((Instant::now(), data.clone()));
    Ok(Json(data))
}


#[derive(Deserialize)]
struct RefreshQuery {
    refresh: Option<u8>,
}

async fn calendar_data(State(st): State<Shared>, user: CurrentUser, Query(q): Query<RefreshQuery>) -> ApiResult<CalendarData> {
    let s = &user.session;
    let mut cache = s.calendar_cache.lock().await;
    if q.refresh.unwrap_or(0) == 0 {
        if let Some((at, data)) = cache.as_ref() {
            if at.elapsed() < Duration::from_secs(180) {
                return Ok(Json(data.clone()));
            }
        }
    }
    let school = s.school()?;
    let courses = school.courses().await?;
    let (assignments, vods) = tokio::join!(school.assignments(&courses), school.vods(&courses));
    let (assignments, vods) = (assignments?, vods?);
    let snapshots = db::sync_assignments(&st.db, s.user_id, &assignments).await?;
    let checks = db::item_checks(&st.db, s.user_id).await?;
    let alerts_off = db::item_alerts_off(&st.db, s.user_id).await?;
    let now = Utc::now().timestamp();
    let data = CalendarData {
        items: calendar::build(&assignments, &vods, &calendar::snapshot_infos(snapshots), &checks, &alerts_off, now),
        courses,
        fetched_at: now,
    };
    *cache = Some((Instant::now(), data.clone()));
    Ok(Json(data))
}

#[derive(Deserialize)]
struct StateBody {
    courses: Vec<hongsi_core::models::Course>,
    assignments: Vec<hongsi_core::models::Assignment>,
}

async fn calendar_state(State(st): State<Shared>, user: CurrentUser, Json(b): Json<StateBody>) -> ApiResult<Value> {
    if b.courses.len() > 200 || b.assignments.len() > 3000 {
        return Err(ApiError::bad_request("요청이 너무 커요"));
    }
    *user.session.device_courses.lock().expect("세션 잠금") = b.courses.iter().map(|c| c.id).collect();
    let snapshots = db::sync_assignments(&st.db, user.session.user_id, &b.assignments).await?;
    let checks = db::item_checks(&st.db, user.session.user_id).await?;
    let alerts_off = db::item_alerts_off(&st.db, user.session.user_id).await?;
    Ok(Json(json!({ "snapshots": calendar::snapshot_infos(snapshots), "checks": checks, "alertsOff": alerts_off })))
}

#[derive(Deserialize)]
struct DoneBody {
    done: bool,
}

#[derive(Deserialize)]
struct CalendarDoneBody {
    #[serde(deserialize_with = "Option::<bool>::deserialize")]
    done: Option<bool>,
}

async fn set_done(State(st): State<Shared>, user: CurrentUser, Path(key): Path<String>, Json(b): Json<CalendarDoneBody>) -> ApiResult<Value> {
    if key.len() > 300 || !(key.starts_with("assign:") || key.starts_with("vod:")) {
        return Err(ApiError::bad_request("잘못된 항목이에요"));
    }
    match b.done {
        Some(done) => db::set_item_check(&st.db, user.session.user_id, &key, done).await?,
        None => db::clear_item_check(&st.db, user.session.user_id, &key).await?,
    }
    if let Some((_, data)) = user.session.calendar_cache.lock().await.as_mut() {
        for item in data.items.iter_mut().filter(|i| i.key == key) {
            item.done = b.done.unwrap_or(item.status == "submitted" || item.status == "done");
            item.done_override = b.done;
        }
    }
    Ok(Json(json!({ "key": key, "done": b.done })))
}

#[derive(Deserialize)]
struct AlertBody {
    on: bool,
}

async fn set_alert(State(st): State<Shared>, user: CurrentUser, Path(key): Path<String>, Json(b): Json<AlertBody>) -> ApiResult<Value> {
    if key.len() > 300 || !(key.starts_with("assign:") || key.starts_with("vod:")) {
        return Err(ApiError::bad_request("잘못된 항목이에요"));
    }
    db::set_item_alert(&st.db, user.session.user_id, &key, b.on).await?;
    if let Some((_, data)) = user.session.calendar_cache.lock().await.as_mut() {
        for item in data.items.iter_mut().filter(|i| i.key == key) {
            item.alert = b.on;
        }
    }
    Ok(Json(json!({ "key": key, "on": b.on })))
}


async fn notices_seen_get(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Value> {
    let urls = db::notices_seen(&st.db, user.session.user_id).await?;
    Ok(Json(json!({ "urls": urls })))
}

#[derive(Deserialize)]
struct SeenBody {
    urls: Vec<String>,
}

async fn notices_seen_add(State(st): State<Shared>, user: CurrentUser, Json(b): Json<SeenBody>) -> ApiResult<Value> {
    if b.urls.len() > 200 || b.urls.iter().any(|u| u.is_empty() || u.len() > 1000 || !u.starts_with("http")) {
        return Err(ApiError::bad_request("잘못된 알림이에요"));
    }
    if !b.urls.is_empty() {
        db::add_notices_seen(&st.db, user.session.user_id, &b.urls).await?;
    }
    Ok(Json(json!({ "ok": true })))
}


async fn find_assignment(user: &CurrentUser, cmid: i64) -> Result<hongsi_core::models::Assignment, ApiError> {
    let school = user.session.school()?;
    let courses = school.courses().await?;
    school
        .assignments(&courses)
        .await?
        .into_iter()
        .find(|a| a.cmid == cmid)
        .ok_or_else(|| ApiError::not_found("과제를 찾지 못했어요"))
}

fn submission_view(a: &hongsi_core::models::Assignment, info: &hongsi_core::models::SubmissionInfo) -> Value {
    let now = Utc::now().timestamp();
    json!({
        "info": info,
        "config": a.config,
        "due": a.due,
        "cutoff": a.cutoff,
        "late": a.due.is_some_and(|d| now > d),
        "closed": a.cutoff.is_some_and(|c| now > c) || info.locked || !info.can_edit,
    })
}

async fn submission_get(user: CurrentUser, Path(cmid): Path<i64>) -> ApiResult<Value> {
    let a = find_assignment(&user, cmid).await?;
    let info = user.session.school()?.submission_info(a.id).await?;
    Ok(Json(submission_view(&a, &info)))
}

async fn submission_post(State(st): State<Shared>, user: CurrentUser, Path(cmid): Path<i64>, mut form: Multipart) -> ApiResult<Value> {
    let mut keep: Vec<String> = Vec::new();
    let mut new_files: Vec<(String, Vec<u8>)> = Vec::new();
    let (mut late_confirmed, mut accept_statement) = (false, false);
    while let Some(field) = form.next_field().await.map_err(|_| ApiError::bad_request("파일을 읽지 못했어요"))? {
        match field.name().unwrap_or("") {
            "keep" => keep.push(field.text().await.unwrap_or_default()),
            "lateConfirmed" => late_confirmed = field.text().await.unwrap_or_default() == "1",
            "acceptStatement" => accept_statement = field.text().await.unwrap_or_default() == "1",
            "file" => {
                let name = field.file_name().unwrap_or("file").to_string();
                let bytes = field.bytes().await.map_err(|_| ApiError::bad_request("파일이 너무 크거나 끊겼어요"))?;
                new_files.push((name, bytes.to_vec()));
            }
            _ => {}
        }
    }

    let a = find_assignment(&user, cmid).await?;
    let info = user.session.school()?.submission_info(a.id).await?;
    let now = Utc::now().timestamp();
    let sizes: Vec<(String, usize)> = new_files.iter().map(|(n, b)| (n.clone(), b.len())).collect();
    hongsi_core::calendar::check_submission(&a, &info, now, &keep, &sizes, late_confirmed, accept_statement).map_err(|r| {
        use hongsi_core::calendar::SubmitRejection;
        match r {
            SubmitRejection::BadRequest(m) => ApiError::bad_request(m),
            SubmitRejection::Conflict(m) => ApiError::conflict(m),
            SubmitRejection::LateConfirmRequired => ApiError::new(
                axum::http::StatusCode::PRECONDITION_REQUIRED,
                "late_confirm_required",
                "마감이 지난 과제예요. 지각 제출을 확인해 주세요",
            ),
        }
    })?;
    let total = keep.len() + new_files.len();

    let mut files = Vec::with_capacity(total);
    for name in &keep {
        let existing = info.files.iter().find(|f| &f.name == name).ok_or_else(|| ApiError::bad_request(format!("'{name}' 파일이 이미 없어요")))?;
        let (_, bytes) = user.session.school()?.download(&existing.url, 110 * 1024 * 1024).await?;
        files.push((existing.name.clone(), bytes));
    }
    files.extend(new_files);

    user.session.school()?.submit_files(a.id, files, a.config.drafts, accept_statement).await?;
    tracing::info!(cmid, "과제 제출 완료");

    let key = format!("assign:{cmid}");
    db::set_item_check(&st.db, user.session.user_id, &key, true).await?;
    if let Some((_, data)) = user.session.calendar_cache.lock().await.as_mut() {
        for item in data.items.iter_mut().filter(|i| i.key == key) {
            item.status = "submitted";
            item.done = true;
        }
    }
    let info = user.session.school()?.submission_info(a.id).await?;
    Ok(Json(submission_view(&a, &info)))
}


#[derive(Deserialize)]
struct PageQuery {
    page: Option<u32>,
}

async fn notifications(user: CurrentUser, Query(q): Query<PageQuery>) -> ApiResult<Vec<hongsi_core::models::Notification>> {
    let days = |w: &str| -> u32 {
        let n: u32 = w.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
        if w.contains('일') { n } else if w.contains("주") { n * 7 } else if w.contains("달") || w.contains("개월") || w.contains("년") { 365 } else { 0 }
    };
    let pages = q.page.unwrap_or(6).clamp(1, 6);
    let mut all: Vec<hongsi_core::models::Notification> = Vec::new();
    for page in 1..=pages {
        let items = user.session.school()?.notifications(page).await?;
        let count = items.len();
        let mut stop = count < 15;
        for n in items {
            if days(&n.when) >= 7 {
                stop = true;
                continue;
            }
            if !all.iter().any(|x| x.url == n.url && x.when == n.when) {
                all.push(n);
            }
        }
        if stop {
            break;
        }
    }
    Ok(Json(all))
}


async fn check_todo(user: &CurrentUser, t: &TodoInput) -> Result<Option<chrono::DateTime<Utc>>, ApiError> {
    let title = t.title.trim();
    if title.is_empty() || title.chars().count() > 200 || t.note.chars().count() > 2000 {
        return Err(ApiError::bad_request("할 일 제목은 1~200자, 메모는 2000자까지예요"));
    }
    if let Some(key) = &t.parent_key {
        if !(key.starts_with("assign:") || key.starts_with("vod:")) || key.len() > 300 {
            return Err(ApiError::bad_request("잘못된 연결 항목이에요"));
        }
    }
    if let Some(course) = t.course_id {
        let cached = user
            .session
            .calendar_cache
            .lock()
            .await
            .as_ref()
            .map(|(_, d)| d.courses.iter().any(|c| c.id == course));
        let known = match (cached, user.session.school.as_ref()) {
            (Some(k), _) => k,
            (None, Some(school)) => school.courses().await?.iter().any(|c| c.id == course),
            (None, None) => {
                let known = user.session.device_courses.lock().expect("세션 잠금");
                known.is_empty() || known.contains(&course)
            }
        };
        if !known {
            return Err(ApiError::bad_request("이번 학기 수강 과목이 아니에요"));
        }
    }
    match t.due_at {
        None => Ok(None),
        Some(ts) => Utc
            .timestamp_opt(ts, 0)
            .single()
            .map(Some)
            .ok_or_else(|| ApiError::bad_request("날짜가 올바르지 않아요")),
    }
}

async fn todo_list(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Vec<Todo>> {
    Ok(Json(todos::list(&st.db, user.session.user_id).await?))
}

async fn todo_create(State(st): State<Shared>, user: CurrentUser, Json(t): Json<TodoInput>) -> ApiResult<Todo> {
    let due = check_todo(&user, &t).await?;
    Ok(Json(todos::create(&st.db, user.session.user_id, &t, due).await?))
}

async fn todo_update(State(st): State<Shared>, user: CurrentUser, Path(id): Path<i64>, Json(t): Json<TodoInput>) -> ApiResult<Todo> {
    let due = check_todo(&user, &t).await?;
    todos::update(&st.db, user.session.user_id, id, &t, due)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("할 일을 찾지 못했어요"))
}

async fn todo_done(State(st): State<Shared>, user: CurrentUser, Path(id): Path<i64>, Json(b): Json<DoneBody>) -> ApiResult<Todo> {
    todos::set_done(&st.db, user.session.user_id, id, b.done)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("할 일을 찾지 못했어요"))
}

async fn todo_delete(State(st): State<Shared>, user: CurrentUser, Path(id): Path<i64>) -> ApiResult<Value> {
    if !todos::delete(&st.db, user.session.user_id, id).await? {
        return Err(ApiError::not_found("할 일을 찾지 못했어요"));
    }
    Ok(Json(json!({ "ok": true })))
}


async fn meals(State(st): State<Shared>) -> ApiResult<Vec<MealDay>> {
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


async fn ensure_snapshot(st: &Shared) -> Result<(), ApiError> {
    if let Some(s) = st.seats.read().await.as_ref() {
        if (Utc::now() - s.fetched_at).num_seconds() < 90 {
            return Ok(());
        }
    }
    let buildings = hongsi_core::seats::fetch_all(&st.http).await?;
    *st.seats.write().await = Some(SeatSnapshot { fetched_at: Utc::now(), buildings });
    Ok(())
}

async fn seats(State(st): State<Shared>) -> ApiResult<Value> {
    ensure_snapshot(&st).await?;
    let snap = st.seats.read().await;
    let s = snap.as_ref().ok_or_else(|| ApiError::not_found("좌석 정보를 받지 못했어요"))?;
    Ok(Json(json!({ "fetchedAt": s.fetched_at.timestamp(), "watching": st.config.seat_poll_secs > 0, "buildings": s.buildings })))
}

#[derive(Deserialize)]
struct RecentQuery {
    building: String,
    room: i32,
    minutes: Option<i64>,
}

async fn seats_recent(State(st): State<Shared>, Query(q): Query<RecentQuery>) -> ApiResult<Vec<RecentSeat>> {
    let minutes = q.minutes.unwrap_or(15).clamp(1, 120);
    Ok(Json(db::recent_assigned(&st.db, &q.building, q.room, Utc::now() - Span::minutes(minutes)).await?))
}

async fn seat_info(st: &Shared, building: &str, room_no: i32, seat_no: i32) -> Option<(String, String)> {
    let snap = st.seats.read().await;
    let b = snap.as_ref()?.buildings.iter().find(|b| b.id == building)?;
    let room = b.rooms.iter().find(|r| r.no as i32 == room_no)?;
    let state = room.grid.iter().flatten().flatten().find(|c| c.no as i32 == seat_no)?.state;
    Some((room.name.clone(), state.as_str().to_string()))
}

fn building_name(id: &str) -> &'static str {
    hongsi_core::seats::BUILDINGS.iter().find(|(b, _, _)| *b == id).map(|(_, name, _)| *name).unwrap_or("")
}

fn normalize_period(period: Option<&str>) -> &'static str {
    match period {
        Some("exam") => "exam",
        Some("vacation") => "vacation",
        _ => "semester",
    }
}

async fn view(st: &Shared, s: SeatSession) -> Value {
    let seat_state = seat_info(st, &s.building, s.room_no, s.seat_no).await.map(|(_, state)| state);
    let mut v = serde_json::to_value(&s).unwrap_or_else(|_| json!({}));
    v["seatState"] = json!(seat_state);
    v["validityHours"] = json!(validity_hours(&s.period));
    v["buildingName"] = json!(building_name(&s.building));
    v
}

async fn seat_session(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Value> {
    let _ = ensure_snapshot(&st).await;
    let session = db::active_seat_session(&st.db, user.session.user_id).await?;
    let session = match session {
        Some(s) => Some(view(&st, s).await),
        None => None,
    };
    Ok(Json(json!({ "session": session })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartBody {
    building: String,
    room_no: i32,
    seat_no: i32,
    period: Option<String>,
}

async fn seat_start(State(st): State<Shared>, user: CurrentUser, Json(b): Json<StartBody>) -> ApiResult<Value> {
    let user_id = user.session.user_id;
    if db::active_seat_session(&st.db, user_id).await?.is_some() {
        return Err(ApiError::conflict("이미 입실 중인 좌석이 있어요. 퇴실한 뒤 다시 입실해 주세요"));
    }
    ensure_snapshot(&st).await?;
    let (room_name, _) = seat_info(&st, &b.building, b.room_no, b.seat_no)
        .await
        .ok_or_else(|| ApiError::not_found("그 좌석을 찾지 못했어요"))?;
    let period = normalize_period(b.period.as_deref());
    let now = Utc::now();
    let (started_at, source) = match db::seat_state(&st.db, &b.building, b.room_no, b.seat_no).await? {
        Some(s) if s.state == "used" && s.since_known && now - s.since <= Span::minutes(30) => (s.since, "detected"),
        _ => (now, "manual"),
    };
    let session = db::insert_seat_session(
        &st.db,
        user_id,
        NewSeatSession {
            building: &b.building,
            room_no: b.room_no,
            room_name: &room_name,
            seat_no: b.seat_no,
            period,
            started_at,
            start_source: source,
            hours: validity_hours(period) as i32,
        },
    )
    .await?;
    Ok(Json(json!({ "session": view(&st, session).await })))
}

async fn seat_extend(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Value> {
    let user_id = user.session.user_id;
    let current = db::active_seat_session(&st.db, user_id).await?.ok_or_else(|| ApiError::not_found("입실 중인 좌석이 없어요"))?;
    let session = db::extend_seat_session(&st.db, user_id, validity_hours(&current.period) as i32)
        .await?
        .ok_or_else(|| ApiError::not_found("입실 중인 좌석이 없어요"))?;
    Ok(Json(json!({ "session": view(&st, session).await })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdjustBody {
    started_at: i64,
    period: Option<String>,
}

async fn seat_adjust(State(st): State<Shared>, user: CurrentUser, Json(b): Json<AdjustBody>) -> ApiResult<Value> {
    let now = Utc::now();
    let started_at = Utc
        .timestamp_opt(b.started_at, 0)
        .single()
        .filter(|t| *t <= now + Span::minutes(5) && *t >= now - Span::hours(12))
        .ok_or_else(|| ApiError::bad_request("입실 시각은 최근 12시간 안이어야 해요"))?;
    let period = normalize_period(b.period.as_deref());
    let session = db::adjust_seat_session(&st.db, user.session.user_id, started_at, validity_hours(period) as i32, period)
        .await?
        .ok_or_else(|| ApiError::not_found("입실 중인 좌석이 없어요"))?;
    Ok(Json(json!({ "session": view(&st, session).await })))
}

async fn seat_end(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Value> {
    let ended = db::end_seat_session(&st.db, user.session.user_id, "manual")
        .await?
        .ok_or_else(|| ApiError::not_found("입실 중인 좌석이 없어요"))?;
    Ok(Json(json!({ "session": null, "ended": ended })))
}
