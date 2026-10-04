use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use reqwest::{Client, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const DEFAULT_REPOSITORY: &str = "Jam-Manbo/hongsi";
const CACHE_TTL: Duration = Duration::from_secs(300);
const STALE_TTL: Duration = Duration::from_secs(3600);
const MAX_APK: u64 = 256 * 1024 * 1024;
const MAX_MANIFEST: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Release {
    version: String,
    version_code: i64,
    url: String,
    sha256: String,
    size: u64,
    #[serde(default)]
    notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseSummary {
    id: String,
    tag: String,
    version: String,
    published_at: Option<String>,
    url: String,
    size: u64,
    notes_url: String,
    prerelease: bool,
    is_latest: bool,
    #[serde(skip)]
    source: GitHubRelease,
}

#[derive(Clone, Debug, Serialize)]
struct ReleaseHistory {
    releases: Vec<ReleaseSummary>,
    url: String,
}

#[derive(Clone, Debug)]
struct Catalog {
    latest: Option<Release>,
    latest_failed: bool,
    history: ReleaseHistory,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubRelease {
    id: u64,
    tag_name: String,
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    state: String,
    size: u64,
    digest: Option<String>,
}

fn valid_repository(repository: &str) -> bool {
    let parts: Vec<_> = repository.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        })
}

fn version(value: &str) -> Option<[u64; 3]> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut result = [0; 3];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty()
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        result[i] = part.parse().ok()?;
    }
    Some(result)
}

fn url_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn asset_url(value: &str, repository: &str, tag: &str, name: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let expected = format!(
        "/{repository}/releases/download/{}/{}",
        url_segment(tag),
        url_segment(name)
    );
    let slash_tag = tag
        .split('/')
        .map(url_segment)
        .collect::<Vec<_>>()
        .join("/");
    let slash_path = format!(
        "/{repository}/releases/download/{slash_tag}/{}",
        url_segment(name)
    );
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && (url.path().eq_ignore_ascii_case(&expected)
            || url.path().eq_ignore_ascii_case(&slash_path))
}

impl GitHubRelease {
    fn release_version(&self) -> Option<&str> {
        if self.draft
            || self.tag_name.is_empty()
            || self.tag_name.len() > 1024
            || self.tag_name.chars().any(|c| c.is_control() || c == ' ')
        {
            return None;
        }
        if !self.prerelease {
            return self.stable_version().map(|(value, _)| value);
        }
        Some(
            self.tag_name
                .strip_prefix('v')
                .filter(|value| value.starts_with(|c: char| c.is_ascii_digit()))
                .unwrap_or(&self.tag_name),
        )
    }
    fn stable_version(&self) -> Option<(&str, [u64; 3])> {
        if self.draft || self.prerelease {
            return None;
        }
        let value = self.tag_name.strip_prefix('v')?;
        Some((value, version(value)?))
    }
    fn files(&self, repository: &str) -> Option<(&Asset, &Asset)> {
        let value = self.release_version()?;
        let valid = |asset: &&Asset| {
            asset.state == "uploaded"
                && asset_url(
                    &asset.browser_download_url,
                    repository,
                    &self.tag_name,
                    &asset.name,
                )
        };
        let mut apks = self
            .assets
            .iter()
            .filter(|asset| asset.name.ends_with(".apk") && (1..=MAX_APK).contains(&asset.size))
            .filter(valid);
        let preferred_name = if self.prerelease {
            let suffix = if value.len() <= 100
                && value
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
            {
                value.to_string()
            } else {
                hex::encode(Sha256::digest(self.tag_name.as_bytes()))[..20].to_string()
            };
            format!("hongsi-beta-{suffix}.apk")
        } else {
            format!("hongsi-{value}.apk")
        };
        let apk = apks
            .clone()
            .find(|a| a.name == preferred_name)
            .or_else(|| {
                let first = apks.next()?;
                apks.next().is_none().then_some(first)
            })?;
        let manifest = self
            .assets
            .iter()
            .filter(valid)
            .find(|a| a.name == "latest.json" && (1..=MAX_MANIFEST as u64).contains(&a.size))?;
        Some((apk, manifest))
    }
}

fn candidates<'a>(releases: &'a [GitHubRelease], repository: &str) -> Vec<&'a GitHubRelease> {
    let mut found: Vec<_> = releases
        .iter()
        .filter(|r| r.files(repository).is_some())
        .collect();
    found.sort_by(|a, b| match (a.stable_version(), b.stable_version()) {
        (Some((_, a)), Some((_, b))) => b.cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b
            .published_at
            .cmp(&a.published_at)
            .then_with(|| b.id.cmp(&a.id)),
    });
    found
}

fn validate_manifest(release: &Release, source: &GitHubRelease, repository: &str) -> bool {
    let Some(expected_version) = source.release_version() else {
        return false;
    };
    let Some((apk, _)) = source.files(repository) else {
        return false;
    };
    release.version == expected_version
        && release
            .tag
            .as_ref()
            .is_none_or(|tag| tag == &source.tag_name)
        && release.version_code > 0
        && release.version_code <= i32::MAX as i64
        && release.size == apk.size
        && release.notes.len() <= 10000
        && release.sha256.len() == 64
        && release.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        && asset_url(&release.url, repository, &source.tag_name, &apk.name)
        && apk
            .digest
            .as_ref()
            .is_none_or(|d| d.eq_ignore_ascii_case(&format!("sha256:{}", release.sha256)))
}

async fn read_json<T: DeserializeOwned>(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<T, ()> {
    if !response.status().is_success()
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        return Err(());
    }
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if data.len() + chunk.len() > limit {
            return Err(());
        }
        data.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&data).map_err(|_| ())
}

fn release_client() -> Result<Client, ()> {
    Client::builder()
        .user_agent("Hongsi-Update/1.0")
        .https_only(true)
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let host = attempt.url().host_str().unwrap_or_default();
            if attempt.previous().len() >= 5
                || !(host == "github.com"
                    || host == "api.github.com"
                    || host.ends_with(".githubusercontent.com"))
            {
                attempt.error("Unexpected release redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| ())
}

async fn fetch_manifest(
    client: &Client,
    source: &GitHubRelease,
    repository: &str,
) -> Result<Release, ()> {
    let (apk, manifest) = source.files(repository).ok_or(())?;
    let response = client
        .get(&manifest.browser_download_url)
        .send()
        .await
        .map_err(|_| ())?;
    let mut release = read_json::<Release>(response, MAX_MANIFEST).await?;
    if !validate_manifest(&release, source, repository) {
        return Err(());
    }
    release.url = apk.browser_download_url.clone();
    Ok(release)
}

fn release_history(
    found: &[&GitHubRelease],
    latest: Option<&Release>,
    repository: &str,
) -> ReleaseHistory {
    let releases = found
        .iter()
        .filter_map(|source| {
            let value = source.release_version()?;
            let (apk, _) = source.files(repository)?;
            Some(ReleaseSummary {
                id: source.id.to_string(),
                tag: source.tag_name.clone(),
                version: value.into(),
                source: (*source).clone(),
                published_at: source.published_at.clone(),
                url: apk.browser_download_url.clone(),
                size: apk.size,
                notes_url: format!(
                    "https://github.com/{repository}/releases/tag/{}",
                    url_segment(&source.tag_name)
                ),
                prerelease: source.prerelease,
                is_latest: latest.is_some_and(|release| release.url == apk.browser_download_url),
            })
        })
        .collect();
    ReleaseHistory {
        releases,
        url: format!("https://github.com/{repository}/releases"),
    }
}

async fn fetch_catalog(repository: &str) -> Result<Catalog, ()> {
    let client = release_client()?;
    let mut releases = Vec::new();
    for page in 1.. {
        let response = client
            .get(format!(
                "https://api.github.com/repos/{repository}/releases?per_page=100&page={page}"
            ))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|_| ())?;
        let batch: Vec<GitHubRelease> = read_json(response, 2 * 1024 * 1024).await?;
        let has_more = batch.len() == 100;
        releases.extend(batch);
        if !has_more {
            break;
        }
    }
    let found = candidates(&releases, repository);
    let latest_result = async {
        let response = client
            .get(format!(
                "https://api.github.com/repos/{repository}/releases/latest"
            ))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|_| ())?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let source: GitHubRelease = read_json(response, 2 * 1024 * 1024).await?;
        if source.stable_version().is_none() {
            return Ok(None);
        }
        fetch_manifest(&client, &source, repository).await.map(Some)
    }
    .await;
    let latest_failed = latest_result.is_err();
    let latest = latest_result.unwrap_or(None);
    let history = release_history(&found, latest.as_ref(), repository);
    Ok(Catalog {
        latest,
        latest_failed,
        history,
    })
}

struct Cached {
    repository: String,
    checked_at: Instant,
    succeeded_at: Option<Instant>,
    result: Result<Catalog, ()>,
}

impl Cached {
    fn refreshed(
        repository: String,
        result: Result<Catalog, ()>,
        previous: Option<&Self>,
        now: Instant,
    ) -> Self {
        let succeeded_at = result.is_ok().then_some(now);
        if result.is_err() {
            if let Some(old) = previous.filter(|c| {
                c.repository == repository
                    && c.result
                        .as_ref()
                        .is_ok_and(|catalog| !catalog.history.releases.is_empty())
                    && c.succeeded_at
                        .is_some_and(|t| now.duration_since(t) < STALE_TTL)
            }) {
                return Self {
                    repository,
                    checked_at: now,
                    succeeded_at: old.succeeded_at,
                    result: old.result.clone(),
                };
            }
        }
        Self {
            repository,
            checked_at: now,
            succeeded_at,
            result,
        }
    }
}

async fn release_catalog() -> Result<Catalog, ()> {
    let repository =
        std::env::var("HONGSI_RELEASE_REPOSITORY").unwrap_or_else(|_| DEFAULT_REPOSITORY.into());
    if !valid_repository(&repository) {
        return Err(());
    }
    static CACHE: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(|| Mutex::new(None)).lock().await;
    if let Some(entry) = cache
        .as_ref()
        .filter(|c| c.repository == repository && c.checked_at.elapsed() < CACHE_TTL)
    {
        return entry.result.clone();
    }
    let result = tokio::time::timeout(Duration::from_secs(20), fetch_catalog(&repository))
        .await
        .unwrap_or(Err(()));
    if result.is_err() {
        tracing::warn!("GitHub 릴리스 정보를 확인하지 못했습니다.");
    }
    let fresh = Cached::refreshed(repository, result, cache.as_ref(), Instant::now());
    let result = fresh.result.clone();
    *cache = Some(fresh);
    result
}

pub async fn latest() -> Response {
    let response = match release_catalog().await {
        Ok(Catalog {
            latest: Some(release),
            ..
        }) => Json(release).into_response(),
        Ok(Catalog {
            latest_failed: true,
            ..
        }) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(()) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(response)
}

pub async fn history() -> Response {
    let response = match release_catalog().await {
        Ok(catalog) => Json(catalog.history).into_response(),
        Err(()) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(response)
}

async fn previous_release(value: &str) -> Result<Option<Release>, ()> {
    if value.is_empty() || value.len() > 1024 {
        return Ok(None);
    }
    let catalog = release_catalog().await?;
    let Some(previous) = catalog
        .history
        .releases
        .iter()
        .find(|r| r.id == value)
        .or_else(|| {
            catalog
                .history
                .releases
                .iter()
                .find(|r| r.tag == value || r.version == value)
        })
    else {
        return Ok(None);
    };
    if let Some(release) = catalog.latest.filter(|r| r.url == previous.url) {
        return Ok(Some(release));
    }
    let repository =
        std::env::var("HONGSI_RELEASE_REPOSITORY").unwrap_or_else(|_| DEFAULT_REPOSITORY.into());
    if !valid_repository(&repository) {
        return Err(());
    }
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Release)>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .await;
    cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
    if let Some((_, release)) = cache.get(&previous.url) {
        return Ok(Some(release.clone()));
    }
    let release = fetch_manifest(&release_client()?, &previous.source, &repository).await?;
    cache.insert(previous.url.clone(), (Instant::now(), release.clone()));
    Ok(Some(release))
}

pub async fn detail(Path(value): Path<String>) -> Response {
    let response = match previous_release(&value).await {
        Ok(Some(release)) => Json(release).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(()) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(response)
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
