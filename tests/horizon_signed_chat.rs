// SPDX-License-Identifier: GPL-3.0-or-later
// Uses generated, deliberately public fixture keys, never player credentials.
use horizon_client_render::auth::vfp_certificates;
use horizon_protocol::{
    packets::{Empty, play},
    rsa::RsaPublicKey,
    secure_chat::{self, Body, Chain, ChatSession, LastSeenValidator, ProfileKey},
    vfp_chat_signing::{self, Signing},
    vfp_core, vfp_wire,
};
use horizon_world::nbt::Tag;

fn fixture() -> std::sync::Arc<vfp_chat_signing::Identity> {
    vfp_certificates::parse(1, include_str!("data/chat-key.json")).unwrap()
}
fn decode(bytes: &[u8]) -> play::Serverbound {
    let mut cursor = vfp_core::Cursor::new(bytes);
    let id = cursor.varint().unwrap();
    play::Serverbound::decode(id, &bytes[cursor.position()..]).unwrap()
}
fn chat(text: &str) -> Vec<u8> {
    play::Serverbound::Chat(play::Chat {
        message: text.into(),
        timestamp: vfp_chat_signing::now(),
        salt: 0,
        signature: None,
        last_seen: play::LastSeenUpdate {
            offset: 0,
            acknowledged: [0; 3],
            checksum: 0,
        },
    })
    .encode()
}
fn receive(signing: &mut Signing, signature: secure_chat::Signature, index: i32) {
    let body = play::Clientbound::PlayerChat(play::PlayerChat {
        global_index: index,
        sender: 2,
        index,
        signature: Some(signature.to_vec()),
        content: "fixture".into(),
        timestamp: vfp_chat_signing::now(),
        salt: 1,
        last_seen: vec![],
        unsigned_content: None,
        filter: (0, vec![]),
        chat_type: play::ChatTypeBound {
            chat_type: 0,
            name: Tag::String("fixture".into()),
            target: None,
        },
    })
    .encode();
    let mut cursor = vfp_core::Cursor::new(&body);
    let id = cursor.varint().unwrap();
    signing.observe(id, &body[cursor.position()..]).unwrap();
}
fn ready(signing: &mut Signing) {
    signing
        .observe(
            vfp_wire::packet_id(horizon_protocol::vfp_tables::NEW_CLIENTBOUND, "LOGIN").unwrap(),
            &[],
        )
        .unwrap();
}

#[test]
fn certificate_announces_valid_key_and_messages_verify_in_the_server_chain() {
    let identity = fixture();
    let mut signing = Signing::new(identity);
    ready(&mut signing);
    let mut chain = None;
    let mut validator = LastSeenValidator::default();
    for (index, text) in ["hello", "Türkçe 😀", "third message"]
        .into_iter()
        .enumerate()
    {
        let packets = signing.outgoing(&chat(text)).unwrap();
        assert_eq!(packets.len(), if index == 0 { 2 } else { 1 });
        for packet in packets {
            match decode(&packet) {
                play::Serverbound::ChatSessionUpdate(key) => {
                    let services =
                        RsaPublicKey::from_der(include_bytes!("data/chat-services.der")).unwrap();
                    chain = Some(Chain::keyed(
                        ChatSession::validate(
                            key.session,
                            ProfileKey {
                                expires_at: key.expires_at,
                                key: key.key,
                                signature: key.key_signature,
                            },
                            &[services],
                            1,
                        )
                        .unwrap(),
                    ));
                }
                play::Serverbound::Chat(message) => {
                    let update = message.last_seen;
                    let seen = validator
                        .apply_update(update.offset, update.acknowledged, update.checksum)
                        .unwrap();
                    let signature =
                        secure_chat::signature(message.signature.as_deref().unwrap()).unwrap();
                    let unpacked = chain
                        .as_mut()
                        .unwrap()
                        .unpack(
                            1,
                            Some(&signature),
                            &Body {
                                content: &message.message,
                                timestamp: message.timestamp,
                                salt: message.salt,
                                last_seen: &seen,
                            },
                            true,
                            vfp_chat_signing::now(),
                        )
                        .unwrap();
                    assert!(
                        matches!(unpacked,secure_chat::Unpacked::Signed {index:i,..} if i==index as i32)
                    );
                    validator.add_pending(&signature);
                    receive(&mut signing, signature, index as i32);
                }
                _ => panic!("unexpected chat packet"),
            }
        }
    }
}

#[test]
fn acknowledgement_window_survives_rollover_duplicates_and_ack_only_packets() {
    let mut signing = Signing::new(fixture());
    ready(&mut signing);
    let mut validator = LastSeenValidator::default();
    for index in 0..65 {
        let signature = [index as u8; 256];
        validator.add_pending(&signature);
        receive(&mut signing, signature, index);
        // The server validator and client tracker both ignore immediate duplicates.
        validator.add_pending(&signature);
        receive(&mut signing, signature, index);
    }
    let tick = play::Serverbound::ClientTickEnd(Empty).encode();
    let packets = signing.outgoing(&tick).unwrap();
    assert_eq!(packets.len(), 3);
    let play::Serverbound::ChatAck(ack) = decode(&packets[1]) else {
        panic!("expected ack")
    };
    assert_eq!(ack.offset, 65);
    validator.apply_offset(ack.offset).unwrap();
    let packets = signing.outgoing(&chat("after rollover")).unwrap();
    let play::Serverbound::Chat(message) = decode(&packets[0]) else {
        panic!("expected chat")
    };
    assert_eq!(message.last_seen.offset, 0);
    let seen = validator
        .apply_update(
            message.last_seen.offset,
            message.last_seen.acknowledged,
            message.last_seen.checksum,
        )
        .unwrap();
    assert_eq!(seen.len(), 20);
    assert_eq!(seen[0], [45; 256]);
    assert_eq!(seen[19], [64; 256]);
}

#[test]
fn reconnect_starts_a_new_chain_and_prepared_keys_are_consumed_once() {
    vfp_chat_signing::prepare(Some(fixture()));
    let mut first = vfp_chat_signing::take_prepared().unwrap();
    assert!(vfp_chat_signing::take_prepared().is_none());
    ready(&mut first);
    let first = first.outgoing(&chat("first")).unwrap();
    let play::Serverbound::ChatSessionUpdate(first) = decode(&first[0]) else {
        panic!()
    };
    let mut next = Signing::new(fixture());
    ready(&mut next);
    let next = next.outgoing(&chat("new connection")).unwrap();
    let play::Serverbound::ChatSessionUpdate(next) = decode(&next[0]) else {
        panic!()
    };
    assert_ne!(first.session, next.session);
    vfp_chat_signing::prepare(None);
}

#[test]
fn malformed_or_expired_certificates_do_not_expose_private_fields() {
    assert!(vfp_certificates::parse(1, "not JSON").is_err());
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("data/chat-key.json")).unwrap();
    for field in [
        "/keyPair/privateKey",
        "/keyPair/publicKey",
        "/publicKeySignatureV2",
        "/expiresAt",
    ] {
        let mut bad = fixture.clone();
        *bad.pointer_mut(field).unwrap() = serde_json::Value::String("PRIVATE_SENTINEL".into());
        let error = vfp_certificates::parse(1, &bad.to_string()).err().unwrap();
        assert!(!error.contains("PRIVATE_SENTINEL"));
    }
    let mut bad = fixture.clone();
    bad["expiresAt"] = "2000-01-01T00:00:00Z".into();
    assert!(vfp_certificates::parse(1, &bad.to_string()).is_err());
    let mut bad = fixture.clone();
    bad["keyPair"]["publicKey"] = format!(
        "-----BEGIN RSA PUBLIC KEY-----\n{}\n-----END RSA PUBLIC KEY-----",
        fixture["fixtureServicesPublicKey"].as_str().unwrap()
    )
    .into();
    assert!(vfp_certificates::parse(1, &bad.to_string()).is_err());
}
