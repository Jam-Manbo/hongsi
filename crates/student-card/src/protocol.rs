use std::sync::Arc;
use std::time::{Duration, Instant};

use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use base64::{engine::general_purpose::STANDARD, Engine};
use qrcode::{Color, EcLevel, QrCode};
use reqwest::{cookie::Jar, redirect::Policy, Client};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::{Context, Failure, Ready, Trace};

const BASE: &str = "https://campus.heyoung.co.kr:18091";
pub const LOCALE_VERSION: &str = "20261006173647";

pub(crate) struct Session {
    pub owner: String,
    pub platform: String,
    pub locale_version: String,
    client: Client,
    agent: String,
    valid_for: Duration,
}

impl Session {
    pub fn new(owner: &str, context: &Context) -> Result<Self, Failure> {
        if !matches!(context.platform.as_str(), "android" | "ios") {
            return Err(Failure::unavailable());
        }
        let client = Client::builder()
            .cookie_provider(Arc::new(Jar::default()))
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .user_agent(&context.user_agent)
            .build()
            .map_err(|_| Failure::unavailable())?;
        Ok(Self {
            owner: owner.into(),
            platform: context.platform.clone(),
            locale_version: String::new(),
            client,
            agent: context.user_agent.to_lowercase(),
            valid_for: Duration::from_secs(30),
        })
    }

    async fn call(
        &self,
        trace: &Trace,
        service: &str,
        screen: &str,
        data: Value,
    ) -> Result<Value, Failure> {
        let started = Instant::now();
        let body = Zeroizing::new(
            serde_json::to_vec(&json!({
                "REQ_COM": {"serviceId": service, "screenId": screen, "langCd": "KO",
                    "clientAgent": self.agent, "clientOS": self.platform, "univCd": "HIUV"},
                "REQ_DAT": data,
            }))
            .map_err(|_| Failure::unavailable())?,
        );
        let mut response = self
            .client
            .post(format!("{BASE}/service/{service}"))
            .header("Content-Type", "application/json;charset=UTF-8")
            .header("Accept", "application/json")
            .header("Origin", BASE)
            .header("Referer", format!("{BASE}/static/"))
            .header("X-Requested-With", "com.shinhan.heyoung")
            .body(body.to_vec())
            .send()
            .await
            .map_err(|_| Failure::unavailable())?;
        let headers_ms = started.elapsed().as_millis();
        if matches!(response.status().as_u16(), 401 | 403) {
            return Err(Failure::authentication(
                "학생증 인증이 만료됐어요. 다시 시도해 주세요.",
            ));
        }
        if !response.status().is_success() {
            return Err(Failure::unavailable());
        }
        let limit = if service == "HCO0201S01" {
            8 * 1024 * 1024
        } else {
            1024 * 1024
        };
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(Failure::response_size());
        }
        let mut raw = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(|_| Failure::unavailable())? {
            if raw.len() + chunk.len() > limit {
                return Err(Failure::response_size());
            }
            raw.extend_from_slice(&chunk);
        }
        let read_ms = started.elapsed().as_millis();
        let mut response: Value =
            serde_json::from_slice(&raw).map_err(|_| Failure::unavailable())?;
        trace.response(
            service,
            started.elapsed().as_millis(),
            raw.len(),
            headers_ms,
            read_ms,
        );
        if response["RES_COM"]["tranState"] != "Y"
            || response["RES_ERR"]["errorCode"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        {
            return Err(Failure::message(
                "헤이영에서 요청을 처리하지 못했어요. 계정 상태를 확인한 후 다시 시도해 주세요.",
            ));
        }
        if !response["RES_DAT"].is_object() {
            return Err(Failure::unavailable());
        }
        Ok(response["RES_DAT"].take())
    }

    pub async fn login(
        &mut self,
        password: &str,
        context: &Context,
        trace: &Trace,
    ) -> Result<(), Failure> {
        let (code, version) = if self.platform == "ios" {
            ("4.0.8.9", "1.6.2")
        } else {
            ("94", "1.6.9")
        };
        let initial = self.call(trace, "HCO0201S01", "AppRoot", json!({
            "univ": "HIUV", "plat": self.platform, "code": code, "version": version,
            "versionSource": "TARGET", "locale": "",
            "localeVersion": if context.locale_version.is_empty() { LOCALE_VERSION } else { &context.locale_version },
            "heyTalkPolicyVersion": "",
        })).await?;
        self.locale_version = initial["localeVersion"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(128)
            .collect();
        let challenge = Zeroizing::new(initial["enckey"].as_str().unwrap_or("").to_owned());
        self.call(trace, "HCO0112S02", "newLogin", json!({"univ": "HIUV"}))
            .await?;
        let started = Instant::now();
        let encrypted = Zeroizing::new(STANDARD.encode(encrypt_password(password, &challenge)?));
        trace.stage("encrypt", started.elapsed().as_millis());
        let login = self.call(trace, "HCO0204S01", "newLogin", json!({
            "univ": "HIUV", "idno": self.owner, "pass": encrypted.as_str(), "pass_type": "E",
            "iddi": "", "mnuIddi": "", "moco": context.device_id, "tokn": "X",
            "plat": self.platform, "auto": false, "cookies": [], "heyKey": "",
            "oldMoco": "", "locale": "ko", "code": challenge.as_str(), "isUse": true,
        })).await?;
        if !matches!(login["code"].as_str(), Some("000" | "002")) {
            return Err(Failure::authentication(
                "헤이영에 로그인하지 못했어요. 학교 계정 로그인을 확인해 주세요.",
            ));
        }
        if !login["data"]["idno"]
            .as_str()
            .is_some_and(|id| id.eq_ignore_ascii_case(&self.owner))
        {
            return Err(Failure::authentication(
                "학생증 계정이 현재 홍시 계정과 일치하지 않아요. 다시 로그인해 주세요.",
            ));
        }
        let timer = self
            .call(
                trace,
                "CMM0101S05",
                "HCO0401P01",
                json!({"code_grp": "MOBILE_TIMER", "univ_code": "G.CODE"}),
            )
            .await?;
        self.valid_for = Duration::from_secs(timer_seconds(&timer["data"][0]["code2"])?);
        Ok(())
    }

    pub async fn qr(&self, trace: &Trace) -> Result<Ready, Failure> {
        let issued = Instant::now();
        let data = self
            .call(trace, "HCO0501S03", "HCO0401P01", json!({"univ": ""}))
            .await?;
        let value = Zeroizing::new(data["qrcode"].as_str().unwrap_or("").to_owned());
        if value.trim().is_empty() || value.len() > 4096 {
            return Err(Failure::message(
                "발급된 학생증 QR이 없어요. 헤이영에서 학생증 등록 상태를 확인해 주세요.",
            ));
        }
        let render = Instant::now();
        let image = qr_png(&value)?;
        trace.stage("render", render.elapsed().as_millis());
        let remaining = self
            .valid_for
            .checked_sub(issued.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| Failure::message("QR 유효시간이 지났어요. 다시 시도해 주세요."))?;
        Ok(Ready {
            image_data_url: format!("data:image/png;base64,{}", STANDARD.encode(image)),
            valid_for_ms: remaining.as_millis() as u64,
            processing_ms: trace.started.elapsed().as_millis() as u64,
            timing_id: trace.id,
        })
    }
}

pub(crate) fn encrypt_password(password: &str, challenge: &str) -> Result<Vec<u8>, Failure> {
    if challenge.len() != 96 || !challenge.is_ascii() {
        return Err(Failure::unavailable());
    }
    let hex = |s: &str| -> Result<Vec<u8>, Failure> {
        s.as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let hi = (pair[0] as char)
                    .to_digit(16)
                    .ok_or_else(Failure::unavailable)?;
                let lo = (pair[1] as char)
                    .to_digit(16)
                    .ok_or_else(Failure::unavailable)?;
                Ok((hi * 16 + lo) as u8)
            })
            .collect()
    };
    let salt = hex(&challenge[32..64])?;
    let iv = hex(&challenge[64..])?;
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(&challenge.as_bytes()[..32], &salt, 1000, &mut *key);
    let cipher = cbc::Encryptor::<aes::Aes256>::new_from_slices(&*key, &iv)
        .map_err(|_| Failure::unavailable())?;
    Ok(cipher.encrypt_padded_vec_mut::<Pkcs7>(password.as_bytes()))
}

pub(crate) fn timer_seconds(value: &Value) -> Result<u64, Failure> {
    let seconds = value
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| value.as_f64())
        .unwrap_or(30.0);
    if seconds <= 0.0 {
        return Ok(30);
    }
    if !seconds.is_finite() || !(1.0..=3600.0).contains(&seconds) || seconds.fract() != 0.0 {
        return Err(Failure::message("학생증 QR 유효시간을 확인하지 못했어요."));
    }
    Ok(seconds as u64)
}

pub(crate) fn qr_png(value: &str) -> Result<Vec<u8>, Failure> {
    let code = if value.is_ascii() {
        QrCode::with_error_correction_level(value.as_bytes(), EcLevel::M).ok()
    } else {
        (1..=40).find_map(|version| {
            let mut bits = qrcode::bits::Bits::new(qrcode::Version::Normal(version));
            bits.push_eci_designator(26).ok()?;
            bits.push_byte_data(value.as_bytes()).ok()?;
            bits.push_terminator(EcLevel::M).ok()?;
            QrCode::with_bits(bits, EcLevel::M).ok()
        })
    }
    .ok_or_else(Failure::unavailable)?;
    let modules = code.width() + 8;
    let scale = (512 / modules).max(1);
    let width = modules * scale;
    let mut pixels = Zeroizing::new(vec![255u8; width * width]);
    for y in 0..code.width() {
        for x in 0..code.width() {
            if code[(x, y)] == Color::Dark {
                for row in 0..scale {
                    let offset = ((y + 4) * scale + row) * width + (x + 4) * scale;
                    pixels[offset..offset + scale].fill(0);
                }
            }
        }
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width as u32, width as u32);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|_| Failure::unavailable())?;
        writer
            .write_image_data(&pixels)
            .map_err(|_| Failure::unavailable())?;
    }
    Ok(output)
}
