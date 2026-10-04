use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{ACCEPT_LANGUAGE, ORIGIN, REFERER};
use reqwest::redirect::Policy;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::OnceCell;

use crate::{CoreError, Result};

pub(crate) const UA: &str = "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) \
                             Chrome/128.0.0.0 Mobile Safari/537.36";
const TIMEOUT: Duration = Duration::from_secs(20);
const SSO_CHECK: &str = "https://ap.hongik.ac.kr/login/LoginCheck_SSO.php";
const SSO_EXEC: &str = "https://ap.hongik.ac.kr/login/LoginExec3.php";

fn clients() -> Result<(Arc<Jar>, Client, Client)> {
    let jar = Arc::new(Jar::default());
    let builder = || Client::builder().user_agent(UA).timeout(TIMEOUT).cookie_provider(jar.clone());
    let client = builder().build()?;
    let no_redirect = builder().redirect(Policy::none()).build()?;
    Ok((jar, client, no_redirect))
}

fn add_sso_cookies(jar: &Jar, cookies: &[(String, String)]) {
    let origin: Url = "https://www.hongik.ac.kr/".parse().expect("URL");
    for (name, value) in cookies {
        jar.add_cookie_str(&format!("{name}={value}; Domain=.hongik.ac.kr; Path=/"), &origin);
    }
}

pub fn public_client() -> Client {
    Client::builder()
        .user_agent(UA)
        .timeout(TIMEOUT)
        .build()
        .expect("HTTP 클라이언트 생성")
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MoodleAuth {
    pub token: String,
    pub private_token: Option<String>,
    pub user_id: i64,
    pub full_name: String,
    pub picture_url: Option<String>,
    pub department: Option<String>,
}

const SERVICE_ORIGINS: [&str; 4] = ["https://cn2.hongik.ac.kr/", "https://at.hongik.ac.kr/", "https://my.hongik.ac.kr/", "https://cn.hongik.ac.kr/"];

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct ServiceCookies {
    origin: String,
    cookies: Vec<(String, String)>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SchoolSessionSnapshot {
    version: u8,
    sso_cookies: Vec<(String, String)>,
    pub(crate) moodle: Option<MoodleAuth>,
    #[serde(default)]
    service_cookies: Vec<ServiceCookies>,
}

pub struct SchoolSession {
    pub(crate) cookie_jar: Arc<Jar>,
    pub(crate) client: Client,
    pub(crate) no_redirect: Client,
    pub(crate) attendance_ready: OnceCell<()>,
    pub(crate) attendance_forms: OnceCell<Vec<Vec<(String, String)>>>,
    pub(crate) moodle_web_ready: OnceCell<()>,
    pub(crate) moodle: OnceCell<MoodleAuth>,
    sso_cookies: Vec<(String, String)>,
}

impl SchoolSession {
    pub async fn login(user_id: &str, password: &str) -> Result<Self> {
        let (jar, client, no_redirect) = clients()?;

        let form = [("USER_ID", user_id), ("PASSWD", password)];
        client
            .get("https://my.hongik.ac.kr/my/login.do")
            .header(ACCEPT_LANGUAGE, "ko-KR,ko;q=0.9")
            .send()
            .await?;

        let check_text = client.post(SSO_CHECK).form(&form).send().await?.text().await?;
        let check: Value = serde_json::from_str(check_text.trim())
            .map_err(|_| CoreError::Parse("SSO 인증 응답".into()))?;
        if check.get("result_code").and_then(Value::as_str) != Some("Y") {
            let message = check
                .get("result_msg")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .unwrap_or("학번 또는 비밀번호를 확인해 주세요.");
            return Err(CoreError::LoginRejected(message.to_string()));
        }

        let body = client
            .post(SSO_EXEC)
            .form(&form)
            .header(REFERER, "https://ap.hongik.ac.kr/login/login.jsp")
            .header(ORIGIN, "https://ap.hongik.ac.kr")
            .send()
            .await?
            .text()
            .await?;
        let pattern = Regex::new(r"SetCookie\s*\(\s*'([^']+)'\s*,\s*'([^']*)'").expect("정규식");
        let cookies: Vec<(String, String)> =
            pattern.captures_iter(&body).map(|cap| (cap[1].to_string(), cap[2].to_string())).collect();
        if cookies.is_empty() {
            return Err(CoreError::Parse("SSO 세션 쿠키".into()));
        }
        add_sso_cookies(&jar, &cookies);
        tracing::debug!(cookies = cookies.len(), "SSO 로그인 성공");
        Ok(Self::with_cookies(jar, client, no_redirect, cookies))
    }

    fn with_cookies(cookie_jar: Arc<Jar>, client: Client, no_redirect: Client, sso_cookies: Vec<(String, String)>) -> Self {
        Self {
            cookie_jar,
            client,
            no_redirect,
            attendance_ready: OnceCell::new(),
            attendance_forms: OnceCell::new(),
            moodle_web_ready: OnceCell::new(),
            moodle: OnceCell::new(),
            sso_cookies,
        }
    }

    pub fn from_sso_cookies(cookies: Vec<(String, String)>) -> Result<Self> {
        let (jar, client, no_redirect) = clients()?;
        add_sso_cookies(&jar, &cookies);
        Ok(Self::with_cookies(jar, client, no_redirect, cookies))
    }

    pub fn sso_cookies(&self) -> &[(String, String)] {
        &self.sso_cookies
    }

    pub fn snapshot(&self) -> SchoolSessionSnapshot {
        let service_cookies = SERVICE_ORIGINS.iter().filter_map(|origin| {
            let url = Url::parse(origin).ok()?;
            let header = self.cookie_jar.cookies(&url)?;
            let mut cookies: Vec<(String, String)> = header.to_str().ok()?.split(';').filter_map(|pair| {
                let (name, value) = pair.trim().split_once('=')?;
                if self.sso_cookies.iter().any(|(key, _)| key == name) { return None; }
                Some((name.to_string(), value.to_string()))
            }).collect();
            cookies.sort();
            (!cookies.is_empty()).then(|| ServiceCookies { origin: origin.to_string(), cookies })
        }).collect();
        SchoolSessionSnapshot { version: 1, sso_cookies: self.sso_cookies.clone(), moodle: self.moodle.get().cloned(), service_cookies }
    }

    pub fn from_snapshot(snapshot: SchoolSessionSnapshot) -> Result<Self> {
        let valid_token = |token: &str| !token.is_empty() && token.len() <= 128 && token.bytes().all(|b| b.is_ascii_alphanumeric());
        let invalid_cookie = |(name, value): &(String, String)| {
            name.is_empty() || name.len() > 100 || value.len() > 8000
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
                || value.bytes().any(|b| b.is_ascii_control() || b == b';')
        };
        if snapshot.version != 1 || snapshot.sso_cookies.is_empty() || snapshot.sso_cookies.len() > 30
            || snapshot.sso_cookies.iter().any(invalid_cookie)
            || snapshot.service_cookies.len() > SERVICE_ORIGINS.len()
            || snapshot.service_cookies.iter().any(|service| {
                !SERVICE_ORIGINS.contains(&service.origin.as_str()) || service.cookies.len() > 50
                    || service.cookies.iter().any(invalid_cookie)
            })
            || snapshot.moodle.as_ref().is_some_and(|auth| {
                !valid_token(&auth.token) || auth.private_token.as_deref().is_some_and(|token| !valid_token(token)) || auth.user_id <= 0
            })
        {
            return Err(CoreError::Parse("저장된 학교 세션".into()));
        }
        let mut session = Self::from_sso_cookies(snapshot.sso_cookies)?;
        session.moodle = OnceCell::new_with(snapshot.moodle);
        for service in snapshot.service_cookies {
            let origin = Url::parse(&service.origin).map_err(|_| CoreError::Parse("저장된 학교 주소".into()))?;
            for (name, value) in service.cookies {
                session.cookie_jar.add_cookie_str(&format!("{name}={value}; Path=/; Secure"), &origin);
            }
        }
        Ok(session)
    }

    pub(crate) async fn get_text(&self, url: &str, referer: Option<&str>) -> Result<String> {
        let mut req = self.client.get(url);
        if let Some(referer) = referer {
            req = req.header(REFERER, referer);
        }
        Ok(req.send().await?.text().await?)
    }

    pub(crate) async fn post_form_text(
        &self,
        url: &str,
        form: &[(String, String)],
        referer: Option<&str>,
    ) -> Result<String> {
        let mut req = self.client.post(url).form(form);
        if let Some(referer) = referer {
            req = req.header(REFERER, referer);
        }
        Ok(req.send().await?.text().await?)
    }
}
