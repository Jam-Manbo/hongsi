use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use reqwest::{Client, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
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
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
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

fn asset_url(value: &str, repository: &str, tag: &str, name: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let expected = format!("/{repository}/releases/download/{tag}/{name}");
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path().eq_ignore_ascii_case(&expected)
}

impl GitHubRelease {
    fn stable_version(&self) -> Option<(&str, [u64; 3])> {
        if self.draft || self.prerelease {
            return None;
        }
        let value = self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name);
        Some((value, version(value)?))
    }
    fn files(&self, repository: &str) -> Option<(&Asset, &Asset)> {
        let (version, _) = self.stable_version()?;
        let apk_name = format!("hongsi-{version}.apk");
        let find = |name: &str| {
            self.assets.iter().find(|a| {
                a.name == name
                    && a.state == "uploaded"
                    && asset_url(&a.browser_download_url, repository, &self.tag_name, name)
            })
        };
        let apk = find(&apk_name)?;
        let manifest = find("latest.json")?;
        if !(1..=MAX_APK).contains(&apk.size) || !(1..=MAX_MANIFEST as u64).contains(&manifest.size)
        {
            return None;
        }
        Some((apk, manifest))
    }
}

fn candidates<'a>(releases: &'a [GitHubRelease], repository: &str) -> Vec<&'a GitHubRelease> {
    let mut found: Vec<_> = releases
        .iter()
        .filter(|r| r.files(repository).is_some())
        .collect();
    found.sort_by_key(|r| std::cmp::Reverse(r.stable_version().unwrap().1));
    found
}

fn validate_manifest(release: &Release, source: &GitHubRelease, repository: &str) -> bool {
    let Some((expected_version, _)) = source.stable_version() else {
        return false;
    };
    let Some((apk, _)) = source.files(repository) else {
        return false;
    };
    release.version == expected_version
        && release.version_code > 0
        && release.version_code <= i32::MAX as i64
        && release.size == apk.size
        && release.notes.len() <= 10000
        && release.sha256.len() == 64
        && release.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        && release.url == apk.browser_download_url
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

async fn fetch_latest(repository: &str) -> Result<Option<Release>, ()> {
    let client = Client::builder()
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
        .map_err(|_| ())?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{repository}/releases?per_page=100"
        ))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|_| ())?;
    let releases: Vec<GitHubRelease> = read_json(response, 2 * 1024 * 1024).await?;
    let found = candidates(&releases, repository);
    if found.is_empty() {
        return Ok(None);
    }
    for source in found.into_iter().take(3) {
        let (_, manifest) = source.files(repository).ok_or(())?;
        let Ok(response) = client.get(&manifest.browser_download_url).send().await else {
            continue;
        };
        let Ok(release) = read_json::<Release>(response, MAX_MANIFEST).await else {
            continue;
        };
        if validate_manifest(&release, source, repository) {
            return Ok(Some(release));
        }
    }
    Err(())
}

struct Cached {
    repository: String,
    checked_at: Instant,
    succeeded_at: Option<Instant>,
    result: Result<Option<Release>, ()>,
}

impl Cached {
    fn refreshed(
        repository: String,
        result: Result<Option<Release>, ()>,
        previous: Option<&Self>,
        now: Instant,
    ) -> Self {
        let succeeded_at = result.is_ok().then_some(now);
        if result.is_err() {
            if let Some(old) = previous.filter(|c| {
                c.repository == repository
                    && matches!(c.result, Ok(Some(_)))
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

async fn latest_release() -> Result<Option<Release>, ()> {
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
    let result = tokio::time::timeout(Duration::from_secs(20), fetch_latest(&repository))
        .await
        .unwrap_or(Err(()));
    if result.is_err() {
        tracing::warn!("GitHub 정식 릴리스 정보를 확인하지 못했습니다.");
    }
    let fresh = Cached::refreshed(repository, result, cache.as_ref(), Instant::now());
    let result = fresh.result.clone();
    *cache = Some(fresh);
    result
}

pub async fn latest() -> Response {
    let mut response = match latest_release().await {
        Ok(Some(release)) => Json(release).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(()) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
