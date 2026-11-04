// SPDX-License-Identifier: GPL-3.0-or-later
// Connection-owned signed chat. The private key stays in the TLS provider.
use crate::{
    packets::play::{self, Serverbound},
    secure_chat::{self, Signature},
    vfp_core as core,
};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

pub type Sign = dyn Fn(&[u8]) -> core::Result<Vec<u8>> + Send + Sync;
pub struct Identity {
    pub profile: u128,
    pub expires_at: i64,
    pub refresh_after: i64,
    pub public_key: Vec<u8>,
    pub key_signature: Vec<u8>,
    // 1.19.0 binds a different certificate payload than 1.19.1 and newer.
    pub key_signature_v1: Option<Vec<u8>>,
    pub sign: Arc<Sign>,
}
thread_local! {static PREPARED: RefCell<Option<Arc<Identity>>> = const {RefCell::new(None)};}
pub fn prepare(identity: Option<Arc<Identity>>) {
    PREPARED.with_borrow_mut(|slot| *slot = identity);
}
pub fn take_prepared() -> Option<Signing> {
    PREPARED.with_borrow_mut(Option::take).map(Signing::new)
}
pub fn has_prepared() -> bool {
    PREPARED.with_borrow(|slot| slot.is_some())
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis().min(i64::MAX as u128) as i64)
}

pub struct Signing {
    identity: Arc<Identity>,
    session: u128,
    index: i32,
    ready: bool,
    announced: bool,
    seen: VecDeque<Option<Signature>>,
    last: Option<Signature>,
    offset: i32,
}
impl Signing {
    pub fn identity(&self) -> Arc<Identity> {
        self.identity.clone()
    }
    pub fn new(identity: Arc<Identity>) -> Self {
        Self {
            identity,
            session: 0,
            index: 0,
            ready: false,
            announced: false,
            seen: std::iter::repeat_n(None, 20).collect(),
            last: None,
            offset: 0,
        }
    }
    pub fn observe(&mut self, id: i32, payload: &[u8]) -> core::Result<()> {
        let name = crate::vfp_tables::NEW_CLIENTBOUND.get(id as usize).copied();
        if name == Some("LOGIN") {
            self.ready = true;
        } else if name == Some("PLAYER_CHAT") {
            if let play::Clientbound::PlayerChat(message) =
                play::Clientbound::decode(id, payload).map_err(|e| e.to_string())?
            {
                if let Some(signature) = message
                    .signature
                    .as_deref()
                    .and_then(secure_chat::signature)
                {
                    if self.last.as_ref() != Some(&signature) {
                        self.last = Some(signature);
                        self.seen.pop_front();
                        self.seen.push_back(Some(signature));
                        self.offset = self
                            .offset
                            .checked_add(1)
                            .ok_or("Chat acknowledgement overflow")?;
                    }
                }
            }
        }
        Ok(())
    }
    fn update(&mut self) -> (play::LastSeenUpdate, Vec<Signature>) {
        let mut acknowledged = [0; 3];
        let mut signatures = Vec::new();
        for (index, signature) in self.seen.iter().enumerate() {
            if let Some(signature) = signature {
                acknowledged[index / 8] |= 1 << (index % 8);
                signatures.push(*signature);
            }
        }
        let update = play::LastSeenUpdate {
            offset: std::mem::take(&mut self.offset),
            acknowledged,
            checksum: secure_chat::checksum(&signatures),
        };
        (update, signatures)
    }
    pub fn outgoing(&mut self, body: &[u8]) -> core::Result<Vec<Vec<u8>>> {
        if !self.ready {
            return Ok(vec![body.to_vec()]);
        }
        let mut cursor = core::Cursor::new(body);
        let id = cursor.varint()?;
        let name = crate::vfp_tables::NEW_SERVERBOUND.get(id as usize).copied();
        let mut packets = Vec::new();
        if !self.announced {
            if self.identity.expires_at <= now() {
                return Err("Chat certificate expired; reconnect to refresh it".into());
            }
            let mut bytes = [0; 16];
            crate::crypto::random_bytes(&mut bytes).map_err(|e| e.to_string())?;
            bytes[6] = (bytes[6] & 15) | 64;
            bytes[8] = (bytes[8] & 63) | 128;
            self.session = u128::from_be_bytes(bytes);
            packets.push(
                Serverbound::ChatSessionUpdate(play::ChatSessionUpdate {
                    session: self.session,
                    expires_at: self.identity.expires_at,
                    key: self.identity.public_key.clone(),
                    key_signature: self.identity.key_signature.clone(),
                })
                .encode(),
            );
            self.announced = true;
        }
        if name == Some("CHAT") {
            let Serverbound::Chat(mut chat) =
                Serverbound::decode(id, &body[cursor.position()..]).map_err(|e| e.to_string())?
            else {
                unreachable!()
            };
            if self.identity.expires_at <= now() {
                return Err("Chat certificate expired; reconnect to refresh it".into());
            }
            let (update, last_seen) = self.update();
            let mut salt = [0; 8];
            crate::crypto::random_bytes(&mut salt).map_err(|e| e.to_string())?;
            chat.salt = i64::from_be_bytes(salt);
            chat.last_seen = update;
            let payload = secure_chat::signed_payload(
                self.identity.profile,
                self.session,
                self.index,
                &secure_chat::Body {
                    content: &chat.message,
                    timestamp: chat.timestamp,
                    salt: chat.salt,
                    last_seen: &last_seen,
                },
            );
            let signature = (self.identity.sign)(&payload)?;
            if signature.len() != 256 {
                return Err("Chat signing key is not RSA-2048".into());
            }
            chat.signature = Some(signature);
            self.index = self.index.checked_add(1).ok_or("Chat chain exhausted")?;
            packets.push(Serverbound::Chat(chat).encode());
        } else {
            if self.offset > 64 {
                packets.push(
                    Serverbound::ChatAck(play::ChatAck {
                        offset: std::mem::take(&mut self.offset),
                    })
                    .encode(),
                );
            }
            packets.push(body.to_vec());
        }
        Ok(packets)
    }
}
