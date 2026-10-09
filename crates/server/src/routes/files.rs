use axum::extract::{Path, Query};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::auth::CurrentUser;
use crate::error::ApiError;

use super::classroom::find_assignment;

pub(crate) async fn avatar(user: CurrentUser) -> Result<Response, ApiError> {
    let (mime, bytes) = user
        .session
        .school()?
        .avatar()
        .await?
        .ok_or_else(|| ApiError::not_found("프로필 사진이 없어요."))?;
    if !mime.starts_with("image/") {
        return Err(ApiError::not_found("프로필 사진이 없어요."));
    }
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'; style-src 'unsafe-inline'".to_string(),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
pub(crate) struct FileQuery {
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

pub(crate) async fn download_file(
    user: CurrentUser,
    Path((cmid, index)): Path<(i64, usize)>,
    Query(q): Query<FileQuery>,
) -> Result<Response, ApiError> {
    let key = format!("assign:{cmid}");
    let cached = {
        let cache = user.session.calendar_cache.lock().await;
        cache.as_ref().and_then(|(_, d)| {
            d.items
                .iter()
                .find(|i| i.key == key)
                .and_then(|i| i.attachments.get(index).cloned())
        })
    };
    let file = match cached {
        Some(f) => f,
        None => find_assignment(&user, cmid)
            .await?
            .attachments
            .into_iter()
            .nth(index)
            .ok_or_else(|| ApiError::not_found("파일을 찾지 못했어요."))?,
    };
    let (mime, bytes) = user.session.school()?.download(&file.url, 100 * 1024 * 1024).await?;
    file_response(&file.name, &mime, bytes, q.inline.unwrap_or(0) == 1)
}

fn file_response(name: &str, mime: &str, bytes: Vec<u8>, inline: bool) -> Result<Response, ApiError> {
    let media_type = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let inline = inline
        && matches!(
            media_type.as_str(),
            "application/pdf"
                | "text/plain"
                | "image/jpeg"
                | "image/png"
                | "image/gif"
                | "image/webp"
                | "image/avif"
                | "image/bmp"
                | "image/x-icon"
                | "audio/mpeg"
                | "audio/mp4"
                | "audio/ogg"
                | "audio/wav"
                | "audio/webm"
                | "video/mp4"
                | "video/ogg"
                | "video/webm"
        );
    let name = name.rsplit('/').next().unwrap_or(name);
    let disposition = format!(
        "{}; filename*=UTF-8''{}",
        if inline { "inline" } else { "attachment" },
        encode_filename(name)
    );
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|_| ApiError::bad_request("파일 이름 오류"))?,
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    Ok(response)
}

pub(crate) async fn module_file(
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
        .ok_or_else(|| ApiError::not_found("파일을 찾지 못했어요."))?;
    let (mime, bytes) = school.download(&file.url, 100 * 1024 * 1024).await?;
    file_response(&file.name, &mime, bytes, q.inline.unwrap_or(0) == 1)
}

pub(crate) async fn board_file(
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
        .ok_or_else(|| ApiError::not_found("파일을 찾지 못했어요."))?;
    let (mime, bytes) = school.download(&file.url, 100 * 1024 * 1024).await?;
    file_response(&file.name, &mime, bytes, q.inline.unwrap_or(0) == 1)
}
