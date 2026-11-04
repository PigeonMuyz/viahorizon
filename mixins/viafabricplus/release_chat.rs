// SPDX-License-Identifier: GPL-3.0-or-later
// 1.19/1.19.1 chat predates the 1.19.3 session/index signature scheme.
// Sign the actual old payload with the same native certificate provider.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_chat_signing::{self as chat_signing, Identity},
    vfp_release_codec as codec,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
struct Seen {
    sender: u128,
    signature: Vec<u8>,
}
pub struct Chat {
    identity: Option<Arc<Identity>>,
    preceding: Option<Vec<u8>>,
    seen: Vec<Seen>,
    last: Option<Seen>,
    unacknowledged: usize,
}
impl Chat {
    pub fn new(identity: Option<Arc<Identity>>) -> Self {
        Self {
            identity,
            preceding: None,
            seen: Vec::new(),
            last: None,
            unacknowledged: 0,
        }
    }
    pub fn clear(&mut self) {
        self.preceding = None;
        self.seen.clear();
        self.last = None;
        self.unacknowledged = 0;
    }
    fn remember(&mut self, signature: Seen) {
        if signature.sender == 0
            || signature.signature.is_empty()
            || self.last.as_ref() == Some(&signature)
        {
            return;
        }
        self.last = Some(signature.clone());
        self.seen.retain(|entry| entry.sender != signature.sender);
        self.seen.insert(0, signature);
        self.seen.truncate(5);
        self.unacknowledged = self.unacknowledged.saturating_add(1);
    }
    fn last_seen(&mut self, w: &mut Writer) {
        w.varint(self.seen.len() as i32);
        for entry in &self.seen {
            w.bytes(&entry.sender.to_be_bytes());
            w.byte_array(&entry.signature);
        }
        w.bool(false); // No last unacknowledged signature.
        self.unacknowledged = 0;
    }
    pub fn acknowledgement(&mut self) -> Option<Vec<u8>> {
        if self.unacknowledged < 64 {
            return None;
        }
        let mut w = Writer::new();
        self.last_seen(&mut w);
        Some(w.data)
    }
    pub fn player_1191(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        if codec::boolean(&mut r)? {
            r.byte_array(4096)?;
        }
        let sender = r.uuid()?;
        let signature = r.byte_array(4096)?;
        self.remember(Seen { sender, signature });
        let plain = r.string(256)?;
        let decorated = if codec::boolean(&mut r)? {
            Some(r.string(262144)?)
        } else {
            None
        };
        r.i64()?;
        r.i64()?;
        let seen = r.count(5)?;
        for _ in 0..seen {
            r.uuid()?;
            r.byte_array(4096)?;
        }
        let unsigned = if codec::boolean(&mut r)? {
            Some(r.string(262144)?)
        } else {
            None
        };
        let filter = r.varint()?;
        let content = match filter {
            0 => unsigned
                .or(decorated)
                .unwrap_or_else(|| serde_json::json!({"text":plain}).to_string()),
            1 => {
                return Ok(None);
            }
            2 => {
                let mask = r.long_array()?;
                if mask.len() > 4 {
                    return Err(Error::new("Old chat filter exceeds message bound"));
                }
                let mut offset = 0usize;
                let filtered: String = plain
                    .chars()
                    .map(|character| {
                        let filtered = (offset..offset + character.len_utf16()).any(|at| {
                            mask.get(at / 64)
                                .is_some_and(|word| (*word as u64) & (1 << (at % 64)) != 0)
                        });
                        offset += character.len_utf16();
                        if filtered { '#' } else { character }
                    })
                    .collect();
                serde_json::json!({"text":filtered}).to_string()
            }
            _ => return Err(Error::new("Invalid legacy chat filter kind")),
        };
        w.string(&content);
        w.varint(r.varint()?); // Bound chat type, then sender and optional target.
        w.string(&r.string(262144)?);
        codec::optional_text(&mut r, &mut w)?;
        codec::end(&r)?;
        Ok(Some(w.data))
    }
    fn sign(&mut self, protocol: i32, message: &str, timestamp: i64, salt: i64) -> Result<Vec<u8>> {
        let Some(identity) = &self.identity else {
            return Ok(Vec::new());
        };
        if identity.expires_at <= chat_signing::now() {
            return Err(Error::new(
                "Chat certificate expired; reconnect to refresh it",
            ));
        }
        let mut payload = Vec::new();
        if protocol == 759 {
            payload.extend_from_slice(&salt.to_be_bytes());
            payload.extend_from_slice(&identity.profile.to_be_bytes());
            payload.extend_from_slice(&timestamp.div_euclid(1000).to_be_bytes());
            // Gson's sorted literal component has this same single-key JSON.
            payload.extend_from_slice(serde_json::json!({"text":message}).to_string().as_bytes());
        } else {
            let mut body = Vec::new();
            body.extend_from_slice(&salt.to_be_bytes());
            body.extend_from_slice(&timestamp.div_euclid(1000).to_be_bytes());
            body.extend_from_slice(message.as_bytes());
            body.push(70);
            for entry in &self.seen {
                body.push(70);
                body.extend_from_slice(&entry.sender.to_be_bytes());
                body.extend_from_slice(&entry.signature);
            }
            if let Some(previous) = &self.preceding {
                payload.extend_from_slice(previous);
            }
            payload.extend_from_slice(&identity.profile.to_be_bytes());
            payload.extend_from_slice(&Sha256::digest(body));
        }
        let signature = (identity.sign)(&payload).map_err(Error::new)?;
        if signature.len() != 256 {
            return Err(Error::new("Legacy chat signature is not RSA-2048"));
        }
        if protocol == 760 {
            self.preceding = Some(signature.clone());
        }
        Ok(signature)
    }
    pub fn outgoing_1193(&mut self, data: &[u8], command: bool) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let text = r.string(if command { 32767 } else { 256 })?;
        let timestamp = r.i64()?;
        let salt = r.i64()?;
        w.string(&text);
        w.i64(timestamp);
        w.i64(salt);
        if command {
            let arguments = r.count(8)?;
            for _ in 0..arguments {
                r.string(32767)?;
                r.bytes(256)?;
            }
            // Argument signing needs the target server's parsed command tree.
            // Do not reuse a session-format signature for the older algorithm.
            w.varint(0);
        } else {
            if codec::boolean(&mut r)? {
                r.bytes(256)?;
            }
            w.byte_array(&self.sign(760, &text, timestamp, salt)?);
        }
        w.bool(false);
        self.last_seen(&mut w);
        r.varint()?;
        r.bytes(3)?;
        codec::end(&r)?;
        Ok(w.data)
    }
    pub fn outgoing_1191(&mut self, data: &[u8], command: bool) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let text = r.string(if command { 32767 } else { 256 })?;
        let timestamp = r.i64()?;
        let salt = r.i64()?;
        w.string(&text);
        w.i64(timestamp);
        w.i64(salt);
        if command {
            let count = r.count(8)?;
            for _ in 0..count {
                r.string(32767)?;
                r.byte_array(4096)?;
            }
            w.varint(0);
        } else {
            r.byte_array(4096)?;
            w.byte_array(&self.sign(759, &text, timestamp, salt)?);
        }
        codec::boolean(&mut r)?;
        w.bool(false);
        let seen = r.count(5)?;
        for _ in 0..seen {
            r.uuid()?;
            r.byte_array(4096)?;
        }
        if codec::boolean(&mut r)? {
            r.uuid()?;
            r.byte_array(4096)?;
        }
        codec::end(&r)?;
        Ok(w.data)
    }
}
