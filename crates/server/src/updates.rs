use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use reqwest::{Client, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseSummary {
    version: String,
    published_at: Option<String>,
    url: String,
    size: u64,
    notes_url: String,
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
    history: ReleaseHistory,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubRelease {
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
    let (_, manifest) = source.files(repository).ok_or(())?;
    let response = client
        .get(&manifest.browser_download_url)
        .send()
        .await
        .map_err(|_| ())?;
    let release = read_json::<Release>(response, MAX_MANIFEST).await?;
    if !validate_manifest(&release, source, repository) {
        return Err(());
    }
    Ok(release)
}

async fn fetch_catalog(repository: &str) -> Result<Catalog, ()> {
    let client = release_client()?;
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
    let mut history = ReleaseHistory {
        releases: Vec::new(),
        url: format!("https://github.com/{repository}/releases"),
    };
    if found.is_empty() {
        return Ok(Catalog {
            latest: None,
            history,
        });
    }
    for source in found.iter().take(3) {
        if let Ok(release) = fetch_manifest(&client, source, repository).await {
            let latest_version = version(&release.version).ok_or(())?;
            history.releases = found
                .iter()
                .filter_map(|previous| {
                    let (value, number) = previous.stable_version()?;
                    if number > latest_version {
                        return None;
                    }
                    let (apk, _) = previous.files(repository)?;
                    Some(ReleaseSummary {
                        version: value.into(),
                        source: (*previous).clone(),
                        published_at: previous.published_at.clone(),
                        url: apk.browser_download_url.clone(),
                        size: apk.size,
                        notes_url: format!(
                            "https://github.com/{repository}/releases/tag/{}",
                            previous.tag_name
                        ),
                    })
                })
                .collect();
            history.releases.dedup_by(|a, b| a.version == b.version);
            return Ok(Catalog {
                latest: Some(release),
                history,
            });
        }
    }
    Err(())
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
                        .is_ok_and(|catalog| catalog.latest.is_some())
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
        tracing::warn!("GitHub 정식 릴리스 정보를 확인하지 못했습니다.");
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
    if version(value).is_none() {
        return Ok(None);
    }
    let catalog = release_catalog().await?;
    if let Some(release) = catalog.latest.filter(|r| r.version == value) {
        return Ok(Some(release));
    }
    let Some(previous) = catalog.history.releases.iter().find(|r| r.version == value) else {
        return Ok(None);
    };
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
