use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::Mutex;
use web_push::{ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushMessageBuilder};

#[derive(Deserialize)]
struct Firebase {
    project_id: String,
    client_email: String,
    private_key: String,
}
struct Apns {
    key: EncodingKey,
    key_id: String,
    team: String,
    topic: String,
    host: &'static str,
}
pub struct Push {
    http: reqwest::Client,
    vapid: Option<Vec<u8>>,
    contact: String,
    pub public_key: Option<String>,
    firebase: Option<Firebase>,
    apns: Option<Apns>,
    fcm_token: Mutex<Option<(i64, String)>>,
    apns_token: Mutex<Option<(i64, String)>>,
}
#[derive(Debug)]
pub enum PushError {
    Gone,
    Retry,
    Configuration,
}
impl Push {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let read = |key| -> Result<Option<Vec<u8>>, std::io::Error> {
            std::env::var(key).ok().map(std::fs::read).transpose()
        };
        let vapid = read("HONGSI_VAPID_PRIVATE_KEY")?;
        let public_key = vapid
            .as_ref()
            .map(|p| {
                VapidSignatureBuilder::from_pem_no_sub(p.as_slice())
                    .map(|b| URL_SAFE_NO_PAD.encode(b.get_public_key()))
            })
            .transpose()?;
        let contact = std::env::var("HONGSI_VAPID_SUBJECT").unwrap_or_default();
        if vapid.is_some() && !(contact.starts_with("mailto:") || contact.starts_with("https://")) {
            return Err("HONGSI_VAPID_SUBJECT must be a mailto: or https: contact".into());
        }
        let firebase: Option<Firebase> = read("HONGSI_FCM_SERVICE_ACCOUNT")?
            .map(|b| serde_json::from_slice(&b))
            .transpose()?;
        if let Some(f) = &firebase {
            EncodingKey::from_rsa_pem(f.private_key.as_bytes())?;
        }
        let apns = if let Some(key) = read("HONGSI_APNS_PRIVATE_KEY")? {
            Some(Apns {
                key: EncodingKey::from_ec_pem(&key)?,
                key_id: std::env::var("HONGSI_APNS_KEY_ID")?,
                team: std::env::var("HONGSI_APNS_TEAM_ID")?,
                topic: std::env::var("HONGSI_APNS_TOPIC")?,
                host: if std::env::var("HONGSI_APNS_SANDBOX").as_deref() == Ok("1") {
                    "https://api.sandbox.push.apple.com"
                } else {
                    "https://api.push.apple.com"
                },
            })
        } else {
            None
        };
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            vapid,
            contact,
            public_key,
            firebase,
            apns,
            fcm_token: Mutex::new(None),
            apns_token: Mutex::new(None),
        })
    }
    pub fn ready(&self, kind: &str) -> bool {
        match kind {
            "web" => self.vapid.is_some(),
            "fcm" => self.firebase.is_some(),
            "apns" => self.apns.is_some(),
            _ => false,
        }
    }
    pub async fn send(
        &self,
        kind: &str,
        destination: &Value,
        payload: &Value,
        key: &str,
    ) -> Result<(), PushError> {
        let intent = payload["intent"].to_string();
        let title = payload["title"].as_str().unwrap_or("홍시 알림");
        let body = payload["body"].as_str().unwrap_or("");
        let tag = crate::vault::token_hash(key);
        let response = match kind {
            "web" => {
                if !valid_web_destination(destination) { return Err(PushError::Gone); }
                let subscription: SubscriptionInfo = serde_json::from_value(destination.clone()).map_err(|_| PushError::Gone)?;
                let mut signature = VapidSignatureBuilder::from_pem(self.vapid.as_ref().ok_or(PushError::Configuration)?.as_slice(), &subscription).map_err(|_| PushError::Configuration)?;
                signature.add_claim("sub", self.contact.as_str());
                let mut builder = WebPushMessageBuilder::new(&subscription);
                let bytes = serde_json::to_vec(&json!({"title":title,"body":body,"intent":payload["intent"],"tag":tag})).map_err(|_| PushError::Configuration)?;
                builder.set_payload(ContentEncoding::Aes128Gcm, &bytes);
                builder.set_ttl(900);
                builder.set_vapid_signature(signature.build().map_err(|_| PushError::Configuration)?);
                let message = builder.build().map_err(|_| PushError::Gone)?;
                let mut request = self.http.post(message.endpoint.to_string()).header("TTL", message.ttl).header("Urgency", "normal");
                if let Some(p) = message.payload {
                    request = request.header("Content-Encoding", "aes128gcm").header("Content-Type", "application/octet-stream");
                    for (k, v) in p.crypto_headers { request = request.header(k, v); }
                    request = request.body(p.content);
                }
                request.send().await
            }
            "fcm" => {
                let f = self.firebase.as_ref().ok_or(PushError::Configuration)?;
                let token = self.fcm_access_token(f).await?;
                self.http.post(format!("https://fcm.googleapis.com/v1/projects/{}/messages:send", f.project_id)).bearer_auth(token)
                    .json(&json!({"message":{"token":destination["token"],"notification":{"title":title,"body":body},"data":{"intent":intent},"android":{"ttl":"900s","priority":"high","notification":{"tag":tag}}}})).send().await
            }
            "apns" => {
                let a = self.apns.as_ref().ok_or(PushError::Configuration)?;
                let token = self.apns_access_token(a).await?;
                let device = destination["token"].as_str().ok_or(PushError::Gone)?;
                self.http.post(format!("{}/3/device/{device}", a.host)).bearer_auth(token)
                    .header("apns-topic", &a.topic).header("apns-push-type", "alert").header("apns-priority", "10")
                    .header("apns-expiration", (Utc::now().timestamp() + 900).to_string()).header("apns-collapse-id", tag)
                    .json(&json!({"aps":{"alert":{"title":title,"body":body},"sound":"default"},"intent":intent})).send().await
            }
            _ => return Err(PushError::Configuration),
        }.map_err(|_| PushError::Retry)?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        if status.as_u16() == 410 || (kind == "web" && status.as_u16() == 404) {
            return Err(PushError::Gone);
        }
        let data: Value = response.json().await.unwrap_or(Value::Null);
        let unregistered = data["error"]["details"]
            .as_array()
            .is_some_and(|d| d.iter().any(|d| d["errorCode"] == "UNREGISTERED"));
        if unregistered
            || (kind == "apns"
                && matches!(
                    data["reason"].as_str(),
                    Some("BadDeviceToken" | "Unregistered")
                ))
        {
            return Err(PushError::Gone);
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(PushError::Configuration);
        }
        Err(PushError::Retry)
    }
    async fn fcm_access_token(&self, f: &Firebase) -> Result<String, PushError> {
        let mut cache = self.fcm_token.lock().await;
        let now = Utc::now().timestamp();
        if let Some((until, token)) = &*cache {
            if *until > now + 60 {
                return Ok(token.clone());
            }
        }
        let key = EncodingKey::from_rsa_pem(f.private_key.as_bytes())
            .map_err(|_| PushError::Configuration)?;
        let assertion = encode(&Header::new(Algorithm::RS256), &json!({"iss":f.client_email,"scope":"https://www.googleapis.com/auth/firebase.messaging","aud":"https://oauth2.googleapis.com/token","iat":now,"exp":now+3600}), &key).map_err(|_| PushError::Configuration)?;
        let response = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|_| PushError::Retry)?;
        if !response.status().is_success() {
            return Err(PushError::Configuration);
        }
        let data: Value = response.json().await.map_err(|_| PushError::Retry)?;
        let token = data["access_token"]
            .as_str()
            .ok_or(PushError::Configuration)?
            .to_owned();
        *cache = Some((
            now + data["expires_in"].as_i64().unwrap_or(3600).min(3600),
            token.clone(),
        ));
        Ok(token)
    }
    async fn apns_access_token(&self, a: &Apns) -> Result<String, PushError> {
        let mut cache = self.apns_token.lock().await;
        let now = Utc::now().timestamp();
        if let Some((until, token)) = &*cache {
            if *until > now {
                return Ok(token.clone());
            }
        }
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(a.key_id.clone());
        let token = encode(&header, &json!({"iss":a.team,"iat":now}), &a.key)
            .map_err(|_| PushError::Configuration)?;
        *cache = Some((now + 3000, token.clone()));
        Ok(token)
    }
}

pub fn valid_web_destination(value: &Value) -> bool {
    let Some(endpoint) = value["endpoint"].as_str() else {
        return false;
    };
    let Ok(url) = reqwest::Url::parse(endpoint) else {
        return false;
    };
    let host = url.host_str().unwrap_or("");
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && (host == "fcm.googleapis.com"
            || host == "web.push.apple.com"
            || host.ends_with(".push.apple.com")
            || host.ends_with(".push.services.mozilla.com"))
        && ["p256dh", "auth"].iter().all(|k| {
            value["keys"][k]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() < 200)
        })
}
