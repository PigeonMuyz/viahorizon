// SPDX-License-Identifier: GPL-3.0-or-later
// Child of the host auth module: access tokens/private keys never leave memory.
use horizon_protocol::{
    rsa::{RsaPublicKey, SignatureHash},
    vfp_chat_signing::{self, Identity},
};
use std::{
    io::Read,
    sync::{Arc, Mutex},
    time::Duration,
};
use ureq::rustls::{
    SignatureScheme,
    crypto::ring::sign::any_supported_type,
    pki_types::{PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer},
};

fn base64(text: &str) -> Result<Vec<u8>, &'static str> {
    if text.len() > 16384 {
        return Err("Chat certificate encoding is too large");
    }
    let mut out = Vec::new();
    let mut bits = 0u32;
    let mut count = 0;
    let mut ended = false;
    for byte in text.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if byte == b'=' {
            ended = true;
            continue;
        }
        if ended {
            return Err("Invalid chat certificate encoding");
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("Invalid chat certificate encoding"),
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    if count >= 6 || (count > 0 && bits & ((1 << count) - 1) != 0) {
        return Err("Invalid chat certificate encoding");
    }
    Ok(out)
}
fn pem(text: &str) -> Result<Vec<u8>, &'static str> {
    let data = text
        .lines()
        .filter(|s| !s.starts_with("-----"))
        .collect::<String>();
    base64(&data)
}
pub fn parse(profile: u128, json: &str) -> Result<Arc<Identity>, &'static str> {
    let data: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "Invalid chat certificate response")?;
    let field = |name| {
        data.pointer(name)
            .and_then(|v| v.as_str())
            .ok_or("Incomplete chat certificate response")
    };
    let expires = chrono::DateTime::parse_from_rfc3339(field("/expiresAt")?)
        .map_err(|_| "Invalid chat certificate expiry")?
        .timestamp_millis();
    let refresh = chrono::DateTime::parse_from_rfc3339(field("/refreshedAfter")?)
        .map_err(|_| "Invalid chat certificate refresh time")?
        .timestamp_millis();
    if expires <= vfp_chat_signing::now() {
        return Err("Chat certificate has expired");
    }
    let public_key = pem(field("/keyPair/publicKey")?)?;
    let public = RsaPublicKey::from_der(&public_key).map_err(|_| "Invalid chat public key")?;
    if public.size() != 256 {
        return Err("Chat public key is not RSA-2048");
    }
    let der = pem(field("/keyPair/privateKey")?)?;
    // Java's PEM label may say RSA while the encoded key is PKCS#8.
    let signing = any_supported_type(&PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(der.clone())))
        .or_else(|_| any_supported_type(&PrivateKeyDer::Pkcs1(PrivatePkcs1KeyDer::from(der))))
        .map_err(|_| "Invalid chat private key")?;
    let signer = signing
        .choose_scheme(&[SignatureScheme::RSA_PKCS1_SHA256])
        .ok_or("Chat key cannot sign RSA/SHA256")?;
    let proof = signer
        .sign(b"Horizon chat key validation")
        .map_err(|_| "Chat private key validation failed")?;
    if !public.verify(
        SignatureHash::Sha256,
        b"Horizon chat key validation",
        &proof,
    ) {
        return Err("Chat certificate key pair does not match");
    }
    let key_signature = base64(field("/publicKeySignatureV2")?)?;
    if key_signature.is_empty() || key_signature.len() > 4096 {
        return Err("Invalid chat certificate signature");
    }
    let key_signature_v1 = match data
        .pointer("/publicKeySignature")
        .and_then(serde_json::Value::as_str)
        .filter(|encoded| !encoded.is_empty())
    {
        Some(encoded) => {
            let bytes = base64(encoded)?;
            if bytes.is_empty() || bytes.len() > 4096 {
                return Err("Invalid legacy chat certificate signature");
            }
            Some(bytes)
        }
        None => None,
    };
    Ok(Arc::new(Identity {
        profile,
        expires_at: expires,
        refresh_after: refresh,
        public_key,
        key_signature,
        key_signature_v1,
        sign: Arc::new(move |message| {
            signer
                .sign(message)
                .map_err(|_| "Chat signature failed".to_owned())
        }),
    }))
}
pub fn get(account: &super::AuthSession) -> Result<Arc<Identity>, &'static str> {
    static CACHE: Mutex<Option<Arc<Identity>>> = Mutex::new(None);
    let mut cache = CACHE
        .lock()
        .map_err(|_| "Chat certificate cache unavailable")?;
    if let Some(identity) = cache.as_ref().filter(|key| {
        key.profile == account.uuid
            && key.expires_at > vfp_chat_signing::now() + 60_000
            && key.refresh_after > vfp_chat_signing::now()
    }) {
        return Ok(identity.clone());
    }
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .redirects(0)
        .build()
        .post("https://api.minecraftservices.com/player/certificates")
        .set("Authorization", &format!("Bearer {}", account.access_token))
        .set("Content-Type", "application/json")
        .send_string("{}")
        .map_err(
            |_| "Could not retrieve Microsoft chat certificate; check account sign-in and network",
        )?;
    let mut body = String::new();
    response
        .into_reader()
        .take(65537)
        .read_to_string(&mut body)
        .map_err(|_| "Could not read chat certificate")?;
    if body.len() > 65536 {
        return Err("Chat certificate response too large");
    }
    let identity = parse(account.uuid, &body)?;
    *cache = Some(identity.clone());
    Ok(identity)
}
