use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
pub struct Sealed {
    pub name: String,
    #[serde(default)]
    pub student_id: String,
    pub cookies: Vec<(String, String)>,
}

pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn cipher(pepper: &[u8], token: &str) -> Aes256Gcm {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(pepper).expect("HMAC 키");
    mac.update(b"remember:");
    mac.update(token.as_bytes());
    let key = mac.finalize().into_bytes();
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key))
}

pub fn seal(pepper: &[u8], token: &str, data: &Sealed) -> Option<(Vec<u8>, Vec<u8>)> {
    let nonce: [u8; 12] = rand::random();
    let plain = serde_json::to_vec(data).ok()?;
    let ciphertext = cipher(pepper, token).encrypt(Nonce::from_slice(&nonce), plain.as_ref()).ok()?;
    Some((nonce.to_vec(), ciphertext))
}

pub fn open(pepper: &[u8], token: &str, nonce: &[u8], ciphertext: &[u8]) -> Option<Sealed> {
    if nonce.len() != 12 {
        return None;
    }
    let plain = cipher(pepper, token).decrypt(Nonce::from_slice(nonce), ciphertext).ok()?;
    serde_json::from_slice(&plain).ok()
}
