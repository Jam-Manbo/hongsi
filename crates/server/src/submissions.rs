use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use hongsi_core::calendar::{check_submission, SubmitRejection};
use hongsi_core::submission::{Job, Progress, Snapshot, Stage, Status, JOB_TIMEOUT};
use serde::Deserialize;

use crate::{routes::classroom, auth::CurrentUser, db, error::ApiError, state::Shared};

struct Upload {
    keep: Vec<String>,
    files: Vec<(String, Vec<u8>)>,
    late: bool,
    statement: bool,
}

async fn read_upload(mut form: Multipart) -> Result<Upload, ApiError> {
    let mut upload = Upload { keep: Vec::new(), files: Vec::new(), late: false, statement: false };
    while let Some(field) = form.next_field().await.map_err(|_| ApiError::bad_request("파일을 읽지 못했어요."))? {
        match field.name().unwrap_or("") {
            "keep" => upload.keep.push(field.text().await.map_err(|_| ApiError::bad_request("파일 목록을 읽지 못했어요."))?),
            "lateConfirmed" => upload.late = field.text().await.unwrap_or_default() == "1",
            "acceptStatement" => upload.statement = field.text().await.unwrap_or_default() == "1",
            "file" => {
                let name = field.file_name().unwrap_or("file").to_owned();
                let bytes = field.bytes().await.map_err(|_| ApiError::bad_request("파일 용량이 너무 크거나 업로드가 중단됐어요."))?;
                upload.files.push((name, bytes.to_vec()));
            }
            _ => {}
        }
        if upload.files.len() + upload.keep.len() > 100 { return Err(ApiError::bad_request("파일이 너무 많아요.")); }
    }
    Ok(upload)
}

pub async fn start(
    State(st): State<Shared>, user: CurrentUser, Path((cmid, id)): Path<(i64, String)>, form: Multipart,
) -> Result<(StatusCode, Json<Snapshot>), ApiError> {
    user.session.school()?;
    let (job, created) = st.submissions.start(&user.session.user_id.to_string(), cmid, &id).map_err(ApiError::conflict)?;
    if !created { return Ok((StatusCode::OK, Json(job.snapshot()))); }
    let guard = job.guard();
    job.progress(Progress::at(Stage::Transfer));
    let upload = match tokio::time::timeout(Duration::from_secs(120), read_upload(form)).await {
        Ok(Ok(upload)) => upload,
        result => {
            let error = match result {
                Ok(Err(error)) => error,
                _ => ApiError::bad_request("파일 전송 시간이 초과됐어요."),
            };
            job.fail(error.status.as_u16(), error.code, error.message);
            return Ok((StatusCode::OK, Json(job.snapshot())));
        }
    };
    job.progress(Progress::at(Stage::Upload));
    let running = job.clone();
    tokio::spawn(async move {
        let _guard = guard;
        match tokio::time::timeout(JOB_TIMEOUT, submit(&st, &user, cmid, upload, &running)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => running.fail(error.status.as_u16(), error.code, error.message),
            Err(_) => running.fail(504, "timeout", "학교의 응답이 늦어지고 있어요."),
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job.snapshot())))
}

async fn submit(st: &Shared, user: &CurrentUser, cmid: i64, upload: Upload, job: &Arc<Job>) -> Result<(), ApiError> {
    let school = user.session.school()?;
    let assignment = classroom::find_assignment(user, cmid).await?;
    let before = school.submission_info(assignment.id).await?;
    let sizes: Vec<_> = upload.files.iter().map(|(name, bytes)| (name.clone(), bytes.len())).collect();
    check_submission(&assignment, &before, chrono::Utc::now().timestamp(), &upload.keep, &sizes, upload.late, upload.statement)
        .map_err(|error| match error {
            SubmitRejection::BadRequest(message) => ApiError::bad_request(message),
            SubmitRejection::Conflict(message) => ApiError::conflict(message),
            SubmitRejection::LateConfirmRequired => ApiError::new(StatusCode::PRECONDITION_REQUIRED, "late_confirm_required", "지각 제출을 확인해 주세요."),
        })?;
    let mut files = Vec::with_capacity(upload.keep.len() + upload.files.len());
    for name in &upload.keep {
        let file = before.files.iter().find(|file| &file.name == name).ok_or_else(|| ApiError::bad_request("기존 파일을 찾지 못했어요."))?;
        let (_, bytes) = school.download(&file.url, 110 * 1024 * 1024).await?;
        files.push((file.name.clone(), bytes));
    }
    files.extend(upload.files);
    job.expect(&before, &files);
    school.submit_files_with_progress(assignment.id, files, assignment.config.drafts, upload.statement, job.clone()).await?;
    job.acknowledge();
    job.progress(Progress::at(Stage::Verify));
    let info = school.submission_info(assignment.id).await?;
    if !job.matches(&info) {
        return Err(ApiError::new(StatusCode::BAD_GATEWAY, "submission_unconfirmed", "제출 상태와 파일 목록이 아직 확인되지 않았어요."));
    }
    complete(st, user, cmid).await;
    job.complete(classroom::submission_view(&assignment, &info));
    Ok(())
}

async fn complete(st: &Shared, user: &CurrentUser, cmid: i64) {
    let key = format!("assign:{cmid}");
    if let Err(error) = db::set_item_check(&st.db, user.session.user_id, &key, true).await {
        tracing::warn!(code=?error.as_database_error().and_then(|error| error.code()), "제출 완료 기록 동기화 실패");
    }
    if let Some((_, data)) = user.session.calendar_cache.lock().await.as_mut() {
        for item in data.items.iter_mut().filter(|item| item.key == key) {
            item.status = "submitted";
            item.done = true;
        }
    }
}

#[derive(Default, Deserialize)]
pub struct StatusQuery { #[serde(default)] verify: bool }

pub async fn status(
    State(st): State<Shared>, user: CurrentUser, Path((cmid, id)): Path<(i64, String)>, Query(query): Query<StatusQuery>,
) -> Result<Json<Snapshot>, ApiError> {
    let job = st.submissions.get(&user.session.user_id.to_string(), cmid, &id)
        .ok_or_else(|| ApiError::not_found("제출 진행 기록을 찾지 못했어요. 클래스룸에서 제출 상태를 확인해 주세요."))?;
    if query.verify && job.snapshot().status == Status::Uncertain {
        let assignment = classroom::find_assignment(&user, cmid).await?;
        let info = user.session.school()?.submission_info(assignment.id).await?;
        if job.matches(&info) {
            complete(&st, &user, cmid).await;
            job.complete(classroom::submission_view(&assignment, &info));
        }
    }
    Ok(Json(job.snapshot()))
}
