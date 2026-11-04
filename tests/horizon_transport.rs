// SPDX-License-Identifier: GPL-3.0-or-later
// Compiled against the transformed REAL horizon-protocol, by transport-proof.py.
// This file is deliberately outside Cargo's automatic integration-test list.
use horizon_protocol::{
    connection::{Connection, Pipe},
    vfp_core as vfp,
};
use std::{net::TcpListener, sync::Mutex, time::Duration};
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn configuration_tags_map_builtin_ids_without_moving_dynamic_ids() {
    use horizon_protocol::{
        packets::configuration as cfg, vfp_configuration as c, vfp_tables as t, vfp_wire as w,
    };
    let packet = cfg::Clientbound::UpdateTags(cfg::UpdateTags {
        registries: vec![
            cfg::RegistryTags {
                registry: "minecraft:block".into(),
                tags: vec![cfg::TagEntry {
                    name: "minecraft:convertable_to_mud".into(),
                    ids: vec![23],
                }],
            },
            cfg::RegistryTags {
                registry: "minecraft:worldgen/biome".into(),
                tags: vec![cfg::TagEntry {
                    name: "test:custom".into(),
                    ids: vec![200],
                }],
            },
        ],
    });
    let body = packet.encode();
    let mut reader = horizon_protocol::buf::Reader::new(&body);
    reader.varint().unwrap();
    let (id, payload) = c::clientbound_262(
        w::packet_id(t::OLD_CONFIGURATION, "UPDATE_TAGS").unwrap(),
        reader.rest(),
    )
    .unwrap();
    match cfg::Clientbound::decode(id, &payload).unwrap() {
        cfg::Clientbound::UpdateTags(p) => {
            assert_eq!(p.registries[0].tags[0].ids, vec![24]);
            assert_eq!(p.registries[0].tags[0].name, "minecraft:convertible_to_mud");
            assert_eq!(p.registries[1].tags[0].ids, vec![200]);
        }
        _ => panic!("wrong tag packet"),
    }
}
#[test]
fn configuration_never_claims_native_known_packs_or_enters_play() {
    use horizon_protocol::{
        packets::{Empty, configuration as cfg},
        vfp_configuration as c,
    };
    let body = cfg::Serverbound::SelectKnownPacks(cfg::SelectKnownPacks {
        packs: vec![cfg::KnownPack {
            namespace: "minecraft".into(),
            id: "core".into(),
            version: "26.3".into(),
        }],
    })
    .encode();
    let converted = c::serverbound_262(&body).unwrap();
    let mut reader = horizon_protocol::buf::Reader::new(&converted);
    let id = reader.varint().unwrap();
    match cfg::Serverbound::decode(id, reader.rest()).unwrap() {
        cfg::Serverbound::SelectKnownPacks(p) => assert!(p.packs.is_empty()),
        _ => panic!("wrong known-pack response"),
    }
    assert!(c::serverbound_262(&cfg::Serverbound::FinishConfiguration(Empty).encode()).is_err());
}
#[test]
fn configuration_trim_material_renames_assets_and_preserves_order() {
    use horizon_protocol::{packets::configuration as cfg, vfp_configuration as c};
    use horizon_world::nbt::Tag;
    let body = cfg::Clientbound::RegistryData(cfg::RegistryData {
        registry: "minecraft:trim_material".into(),
        entries: vec![cfg::RegistryEntry {
            id: "minecraft:gold".into(),
            data: Some(Tag::compound([
                ("asset_name", Tag::String("gold".into())),
                ("item_model_index", Tag::Float(0.6f32.to_bits())),
            ])),
        }],
    })
    .encode();
    let (id, payload) = c::clientbound_262(7, &body[1..]).unwrap();
    match cfg::Clientbound::decode(id, &payload).unwrap() {
        cfg::Clientbound::RegistryData(p) => {
            assert_eq!(p.entries[0].id, "minecraft:gold");
            let data = p.entries[0].data.as_ref().unwrap();
            assert_eq!(
                data.get("palette_id"),
                Some(&Tag::String("minecraft:trim/gold".into()))
            );
            assert!(data.get("asset_name").is_none());
            assert!(data.get("item_model_index").is_some());
        }
        _ => panic!("wrong registry packet"),
    }
}

#[test]
fn decorated_pot_sides_upgrade_in_order_and_supply_missing_bricks() {
    use horizon_protocol::vfp_world::upgrade_block_entity;
    use horizon_world::nbt::{JavaString, Tag};
    let mut tag = Tag::compound([
        (
            "sherds",
            Tag::List(vec![Tag::String(JavaString::from(
                "minecraft:angler_pottery_sherd",
            ))]),
        ),
        ("keep", Tag::Int(7)),
    ]);
    upgrade_block_entity(&mut tag).unwrap();
    assert_eq!(tag.get("keep"), Some(&Tag::Int(7)));
    let sides = tag.get("sherds").unwrap();
    assert_eq!(
        sides.get("back").unwrap().get("id"),
        Some(&Tag::String(JavaString::from(
            "minecraft:angler_pottery_sherd"
        )))
    );
    for side in ["left", "right", "front"] {
        assert_eq!(
            sides.get(side).unwrap().get("id"),
            Some(&Tag::String(JavaString::from("minecraft:brick")))
        );
    }
    let once = tag.clone();
    upgrade_block_entity(&mut tag).unwrap();
    assert_eq!(tag, once);
    let mut invalid = Tag::compound([("sherds", Tag::List(vec![Tag::Int(4)]))]);
    let original = invalid.clone();
    assert!(upgrade_block_entity(&mut invalid).is_err());
    assert_eq!(invalid, original);
}

#[test]
fn chunk_block_entities_and_light_decode_through_the_combined_router() {
    use horizon_protocol::{
        buf::{Codec, Writer},
        chunk::{BlockEntityInfo, ChunkData},
        packets::play,
        vfp_items, vfp_tables as t, vfp_wire as w,
    };
    use horizon_world::nbt::Tag;
    let section = vec![0, 1, 0, 0, 0, 27, 0, 2];
    let mut old = Writer::new();
    old.i32(-2);
    old.i32(3);
    let chest = Tag::compound([("CustomName", Tag::String("unchanged".into()))]);
    let expected_chest = Tag::compound([
        ("CustomName", Tag::String("unchanged".into())),
        ("id", Tag::String("minecraft:chest".into())),
    ]);
    let data = ChunkData {
        heightmaps: Vec::new(),
        buffer: section,
        block_entities: vec![
            BlockEntityInfo {
                packed_xz: 0x12,
                y: 64,
                kind: 1,
                tag: chest.clone(),
            },
            BlockEntityInfo {
                packed_xz: 0x34,
                y: 65,
                kind: 2,
                tag: Tag::compound([("sherds", Tag::List(Vec::new()))]),
            },
        ],
    };
    ChunkData::write(&data, &mut old);
    old.bytes(&[0, 0, 0, 0, 0, 0]); // Four empty masks, two empty layer lists.
    let packets = vfp_items::clientbound_262(
        w::packet_id(t::OLD_CLIENTBOUND, "LEVEL_CHUNK_WITH_LIGHT").unwrap(),
        &old.data,
    )
    .unwrap();
    match play::Clientbound::decode(packets[0].0, &packets[0].1).unwrap() {
        play::Clientbound::LevelChunkWithLight(p) => {
            assert_eq!((p.x, p.z), (-2, 3));
            assert_eq!(p.chunk.block_entities.len(), 2);
            assert_eq!(p.chunk.block_entities[0].tag, expected_chest);
            assert!(
                p.chunk.block_entities[1]
                    .tag
                    .get("sherds")
                    .unwrap()
                    .get("front")
                    .is_some()
            );
            assert_eq!(p.chunk.sections(1, 16, 7).unwrap()[0].states.get(0), 28);
            assert!(p.light.sky.is_empty());
        }
        _ => panic!("wrong chunk packet"),
    }
    old.data.push(0);
    assert!(
        vfp_items::clientbound_262(
            w::packet_id(t::OLD_CLIENTBOUND, "LEVEL_CHUNK_WITH_LIGHT").unwrap(),
            &old.data
        )
        .is_err()
    );
}

#[test]
fn standalone_block_entity_updates_use_the_same_conversion() {
    use horizon_protocol::{
        buf::{Codec, Writer},
        packets::play::{self, BlockEntityData},
        vfp_items, vfp_tables as t, vfp_wire as w,
    };
    use horizon_world::nbt::Tag;
    let packet = BlockEntityData {
        pos: [-1, 70, 8],
        kind: 2,
        tag: Tag::compound([("sherds", Tag::List(Vec::new()))]),
    };
    let mut old = Writer::new();
    BlockEntityData::write(&packet, &mut old);
    let translated = vfp_items::clientbound_262(
        w::packet_id(t::OLD_CLIENTBOUND, "BLOCK_ENTITY_DATA").unwrap(),
        &old.data,
    )
    .unwrap();
    match play::Clientbound::decode(translated[0].0, &translated[0].1).unwrap() {
        play::Clientbound::BlockEntityData(p) => {
            assert_eq!(p.pos, packet.pos);
            assert!(p.tag.get("sherds").unwrap().get("back").is_some());
        }
        _ => panic!("wrong block entity packet"),
    }
}

fn handshake() -> Vec<u8> {
    vfp::Handshake {
        protocol: 777,
        host: "virtual.test".into(),
        port: 25565,
        intent: 2,
    }
    .body()
}
fn success(protocol: i32) -> Vec<u8> {
    let mut out = vec![2];
    out.extend_from_slice(&[0x12; 16]);
    vfp::put_string(&mut out, "Horizon");
    out.push(0);
    if protocol == 776 {
        out.extend_from_slice(&[0x56; 16]);
    }
    out
}
fn configuration(target: &str, enabled: bool) {
    vfp::configure(vfp::Config {
        target: vfp::Selection::parse(target).unwrap(),
        login_preview: true,
        enabled,
        ..vfp::Config::default()
    });
}

#[test]
fn default_configuration_detects_172_and_retargets_the_connection() {
    let _lock = SERIAL.lock().unwrap();
    vfp::configure(vfp::Config::default());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (mut login, _) = listener.accept().unwrap();
        login
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (mut probe, _) = listener.accept().unwrap();
        probe
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let status = vfp::Handshake::parse(&vfp::read_frame(&mut probe).unwrap()).unwrap();
        assert_eq!(status.protocol, 777);
        assert_eq!(status.intent, 1);
        assert_eq!(vfp::read_frame(&mut probe).unwrap(), vec![0]);
        let mut response = vec![0];
        vfp::put_string(
            &mut response,
            r#"{"version":{"name":"1.7.2","protocol":4}}"#,
        );
        vfp::write_frame(&mut probe, &response).unwrap();
        let translated = vfp::Handshake::parse(&vfp::read_frame(&mut login).unwrap()).unwrap();
        assert_eq!(translated.protocol, 4);
        assert_eq!(translated.intent, 2);
        assert_eq!(translated.host, "127.0.0.1");
        assert_eq!(translated.port, address.port());
    });
    let mut client =
        Connection::connect("127.0.0.1", address.port(), Duration::from_secs(3)).unwrap();
    client
        .send(
            &vfp::Handshake {
                protocol: 777,
                host: "127.0.0.1".into(),
                port: address.port(),
                intent: 2,
            }
            .body(),
        )
        .unwrap();
    drop(client);
    worker.join().unwrap();
}

#[test]
fn transfer_handshakes_are_adapted_by_the_actual_connection_hook() {
    let _lock = SERIAL.lock().unwrap();
    for version in ["1.19.3", "1.20.4", "1.20.6", "26.2", "26.3"] {
        let protocol = vfp::release(version).unwrap().protocol;
        vfp::configure(vfp::Config {
            target: vfp::Selection::parse(version).unwrap(),
            ..vfp::Config::default()
        });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = Connection::from_stream(stream).unwrap();
            server
                .reader
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let (id, data) = server.recv().unwrap();
            assert_eq!(id, 0);
            let mut body = vec![0];
            body.extend(data);
            let received = vfp::Handshake::parse(&body).unwrap();
            assert_eq!(received.protocol, protocol);
            assert_eq!(received.intent, if protocol < 766 { 2 } else { 3 });
            assert_eq!(received.host, "virtual.test");
            assert_eq!(received.port, 25565);
        });
        let mut client =
            Connection::connect("127.0.0.1", address.port(), Duration::from_secs(3)).unwrap();
        let mut transferred = vfp::Handshake::parse(&handshake()).unwrap();
        transferred.intent = 3;
        client.send(&transferred.body()).unwrap();
        drop(client);
        worker.join().unwrap();
    }
    vfp::configure(vfp::Config::default());
}

#[test]
fn signed_native_connections_announce_the_key_without_retargeting_the_handshake() {
    use horizon_protocol::{packets::play, vfp_chat_signing, vfp_tables, vfp_wire};
    use std::sync::Arc;
    let _lock = SERIAL.lock().unwrap();
    vfp::configure(vfp::Config {
        target: vfp::Selection::Native,
        ..vfp::Config::default()
    });
    let captured = Arc::new(Mutex::new(Vec::new()));
    let capture = captured.clone();
    vfp_chat_signing::prepare(Some(Arc::new(vfp_chat_signing::Identity {
        profile: 1,
        expires_at: i64::MAX,
        refresh_after: i64::MAX,
        public_key: vec![1; 294],
        key_signature: vec![2; 256],
        key_signature_v1: None,
        sign: Arc::new(move |body| {
            capture.lock().unwrap().push(body.to_vec());
            Ok(vec![3; 256])
        }),
    })));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut peer = Connection::from_stream(stream).unwrap();
        peer.reader
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (id, payload) = peer.recv().unwrap();
        let mut handshake = vec![id as u8];
        handshake.extend(payload);
        assert_eq!(vfp::Handshake::parse(&handshake).unwrap().protocol, 777);
        let mut login = Vec::new();
        vfp::put_varint(
            &mut login,
            vfp_wire::packet_id(vfp_tables::NEW_CLIENTBOUND, "LOGIN").unwrap(),
        );
        peer.send(&login).unwrap();
        let (id, payload) = peer.recv().unwrap();
        let play::Serverbound::ChatSessionUpdate(key) =
            play::Serverbound::decode(id, &payload).unwrap()
        else {
            panic!("missing session key")
        };
        assert_eq!(key.key, vec![1; 294]);
        let (id, payload) = peer.recv().unwrap();
        let play::Serverbound::Chat(chat) = play::Serverbound::decode(id, &payload).unwrap() else {
            panic!("missing signed chat")
        };
        assert_eq!(chat.message, "native signed fixture");
        assert_eq!(chat.signature, Some(vec![3; 256]));
        key.session
    });
    let mut client =
        Connection::connect("127.0.0.1", address.port(), Duration::from_secs(3)).unwrap();
    client
        .reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    client.send(&handshake()).unwrap();
    client.recv().unwrap();
    client
        .send(
            &play::Serverbound::Chat(play::Chat {
                message: "native signed fixture".into(),
                timestamp: vfp_chat_signing::now(),
                salt: 0,
                signature: None,
                last_seen: play::LastSeenUpdate {
                    offset: 0,
                    acknowledged: [0; 3],
                    checksum: 0,
                },
            })
            .encode(),
        )
        .unwrap();
    let session = worker.join().unwrap();
    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(&captured[0][4..20], &1u128.to_be_bytes());
    assert_eq!(&captured[0][20..36], &session.to_be_bytes());
    assert!(!vfp_chat_signing::has_prepared());
    drop(client);
    vfp::configure(vfp::Config::default());
}

#[test]
fn normal_connection_enters_play_without_a_fixture_or_preview_with_encryption_and_compression() {
    use horizon_protocol::{
        packets::{login, play},
        vfp_tables as tables, vfp_wire as wire,
    };
    let _lock = SERIAL.lock().unwrap();
    vfp::configure(vfp::Config {
        target: vfp::Selection::parse("26.2").unwrap(),
        ..vfp::Config::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut server = Connection::from_stream(stream).unwrap();
        server
            .reader
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (id, data) = server.recv().unwrap();
        let mut body = vec![id as u8];
        body.extend(data);
        assert_eq!(vfp::Handshake::parse(&body).unwrap().protocol, 776);
        let (id, data) = server.recv().unwrap();
        assert!(matches!(
            login::Serverbound::decode(id, &data).unwrap(),
            login::Serverbound::Hello(_)
        ));
        server.enable_encryption(&[0x42; 16]);
        server.send(&[3, 16]).unwrap();
        server.set_compression(16);
        server.send(&success(776)).unwrap();
        assert_eq!(server.recv().unwrap(), (3, Vec::new()));
        server.send(&[3]).unwrap();
        assert_eq!(server.recv().unwrap(), (3, Vec::new()));
        let mut keepalive = Vec::new();
        vfp::put_varint(
            &mut keepalive,
            wire::packet_id(tables::OLD_CLIENTBOUND, "KEEP_ALIVE").unwrap(),
        );
        keepalive.extend_from_slice(&42i64.to_be_bytes());
        server.send(&keepalive).unwrap();
        let (id, data) = server.recv().unwrap();
        assert_eq!(
            id,
            wire::packet_id(tables::OLD_SERVERBOUND, "KEEP_ALIVE").unwrap()
        );
        assert_eq!(data, 42i64.to_be_bytes());
    });
    let mut client =
        Connection::connect("127.0.0.1", address.port(), Duration::from_secs(3)).unwrap();
    client
        .reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    client.send(&handshake()).unwrap();
    client
        .send(
            &login::Serverbound::Hello(login::ServerHello {
                name: "Horizon".into(),
                uuid: 42,
            })
            .encode(),
        )
        .unwrap();
    client.enable_encryption(&[0x42; 16]);
    assert_eq!(client.recv().unwrap(), (3, vec![16]));
    client.set_compression(16);
    assert_eq!(client.recv().unwrap().0, 2);
    client.send(&[3]).unwrap();
    for id in [7, 7, 3] {
        assert_eq!(client.recv().unwrap().0, id);
    }
    client.send(&[3]).unwrap();
    let (id, data) = client.recv().unwrap();
    let play::Clientbound::KeepAlive(value) = play::Clientbound::decode(id, &data).unwrap() else {
        panic!()
    };
    assert_eq!(value.id, 42);
    client
        .send(&play::Serverbound::KeepAlive(value).encode())
        .unwrap();
    drop(client);
    worker.join().unwrap();
    vfp::configure(vfp::Config::default());
}
fn preview(protocol: i32) {
    let _lock = SERIAL.lock().unwrap();
    configuration(if protocol == 775 { "26.1" } else { "26.2" }, true);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut server = Connection::from_stream(stream).unwrap();
        server
            .reader
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(id, 0);
        let mut packet = vec![0];
        packet.extend_from_slice(&body);
        let h = vfp::Handshake::parse(&packet).unwrap();
        assert_eq!(h.protocol, protocol);
        assert_eq!(h.host, "virtual.test");
        assert_eq!(server.recv().unwrap(), (0, vec![42]));
        // The transforms must still work when Horizon encrypts AND compresses.
        server.enable_encryption(&[0x25; 16]);
        server.send(&[3, 16]).unwrap();
        server.set_compression(16);
        server.send(&success(protocol)).unwrap();
        {
            assert_eq!(server.recv().unwrap(), (3, Vec::new()));
            // Old STORE_COOKIE is 10; new POST_EFFECTS occupies that slot.
            let mut cookie = vec![10];
            vfp::put_string(&mut cookie, "test:key");
            cookie.extend([2, 7, 9]);
            server.send(&cookie).unwrap();
            server.send(&[3]).unwrap();
        }
        // Neither preview may acknowledge configuration or enter play.
        assert!(server.recv().is_err());
    });
    let mut client = Connection::connect("127.0.0.1", addr.port(), Duration::from_secs(3)).unwrap();
    client
        .reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    client.send(&handshake()).unwrap();
    client.send(&[0, 42]).unwrap();
    client.enable_encryption(&[0x25; 16]);
    assert_eq!(client.recv().unwrap(), (3, vec![16]));
    client.set_compression(16);
    let (id, body) = client.recv().unwrap();
    assert_eq!(id, 2);
    let parsed = horizon_protocol::packets::login::Clientbound::decode(id, &body).unwrap();
    match parsed {
        horizon_protocol::packets::login::Clientbound::LoginFinished(p) => {
            assert_eq!(p.profile.name, "Horizon");
            assert_eq!(p.profile.uuid, u128::from_be_bytes([0x12; 16]));
            assert_eq!(
                p.session,
                if protocol == 776 {
                    u128::from_be_bytes([0x56; 16])
                } else {
                    0
                }
            );
        }
        _ => panic!("unexpected login response"),
    }
    {
        client.send(&[3]).unwrap();
        let (id, payload) = client.recv().unwrap();
        match horizon_protocol::packets::configuration::Clientbound::decode(id, &payload).unwrap() {
            horizon_protocol::packets::configuration::Clientbound::StoreCookie(c) => {
                assert_eq!(c.key, "test:key");
                assert_eq!(c.payload, vec![7, 9]);
            }
            _ => panic!("configuration packet ID was not remapped"),
        }
        for expected in [
            "minecraft:decorated_pot_pattern",
            "minecraft:block_transformer",
        ] {
            let (id, payload) = client.recv().unwrap();
            match horizon_protocol::packets::configuration::Clientbound::decode(id, &payload)
                .unwrap()
            {
                horizon_protocol::packets::configuration::Clientbound::RegistryData(r) => {
                    assert_eq!(r.registry, expected)
                }
                _ => panic!("missing synthesized registry"),
            }
        }
        assert!(
            client
                .recv()
                .unwrap_err()
                .to_string()
                .contains("world-entry boundary")
        );
    }
    drop(client);
    worker.join().unwrap();
    vfp::configure(vfp::Config::default());
}
#[test]
fn encrypted_compressed_261_preview() {
    preview(775);
}
#[test]
fn encrypted_compressed_262_preview() {
    preview(776);
}
#[test]
fn split_reader_still_translates_after_moving_to_another_thread() {
    let _lock = SERIAL.lock().unwrap();
    configuration("26.1", true);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = Connection::from_stream(stream).unwrap();
        c.recv().unwrap();
        c.send(&success(775)).unwrap();
    });
    let mut c = Connection::connect("127.0.0.1", addr.port(), Duration::from_secs(3)).unwrap();
    c.reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    c.send(&handshake()).unwrap();
    let (mut reader, _writer) = c.split();
    let packet = std::thread::spawn(move || reader.recv().unwrap())
        .join()
        .unwrap();
    assert_eq!(packet.0, 2);
    assert_eq!(packet.1.len(), success(775).len() - 1 + 16);
    assert!(matches!(
        horizon_protocol::packets::login::Clientbound::decode(packet.0, &packet.1).unwrap(),
        horizon_protocol::packets::login::Clientbound::LoginFinished(_)
    ));
    server.join().unwrap();
    vfp::configure(vfp::Config::default());
}

#[test]
fn native_chunk_codec_preserves_advisory_server_counts() {
    use horizon_protocol::{
        buf::Reader,
        chunk::Section,
        vfp_mapping::{Maps, V121, V261, V262, V1215},
        vfp_wire as w,
    };
    fn check<M: Maps>() {
        for count in [i16::MIN, -1, 0, 4096, 4097, 8192, i16::MAX] {
            let mut section = count.to_be_bytes().to_vec();
            if M::PROTOCOL >= 775 {
                section.extend_from_slice(&(-1i16).to_be_bytes());
            }
            section.extend([0, 1]); // Single-value non-air block palette.
            if M::PROTOCOL < 770 {
                section.push(0); // Old long-array length.
            }
            section.extend([0, 2]); // Single-value biome palette.
            if M::PROTOCOL < 770 {
                section.push(0);
            }
            let mapped = w::chunk_sections_in_dimension::<M>(&section, 64, Some(1)).unwrap();
            let mut reader = Reader::new(&mapped);
            let decoded = Section::read(&mut reader, 16, 6).unwrap();
            assert_eq!(decoded.non_empty, count);
            assert_eq!(decoded.fluids, if M::PROTOCOL >= 775 { -1 } else { 0 });
            assert_eq!(decoded.states.get(0), M::BLOCKSTATES.map(1).unwrap());
            assert_eq!(decoded.states.get(4095), M::BLOCKSTATES.map(1).unwrap());
            assert_eq!(decoded.biomes.get(63), 2);
            assert_eq!(reader.remaining(), 0);
        }
    }
    check::<V121>(); // 1.21/1.21.1, explicit palette lengths.
    check::<V1215>(); // Implicit palette lengths, synthesized fluid counter.
    check::<V261>(); // Both signed counters on the source wire.
    check::<V262>();
}

#[test]
fn native_decoder_accepts_translated_entity_steps_and_chunk_palettes() {
    use horizon_protocol::{buf::Reader, chunk::Section, vfp_tables as t, vfp_wire as w};
    let mut section = vec![0x10, 0, 0, 0, 0];
    vfp::put_varint(&mut section, 27);
    section.extend([0, 2]);
    let mapped = w::chunk_sections(&section).unwrap();
    let mut r = Reader::new(&mapped);
    let decoded = Section::read(&mut r, 16, 7).unwrap();
    assert_eq!(decoded.states.get(0), 28);
    assert_eq!(decoded.biomes.get(0), 2);
    assert_eq!(r.remaining(), 0);
    let mut payload = vec![1];
    payload.extend_from_slice(
        &[-1i16, 10, 20]
            .iter()
            .flat_map(|i| i.to_be_bytes())
            .collect::<Vec<_>>(),
    );
    payload.extend([0, 0, 1]);
    let packets = w::clientbound_262(
        w::packet_id(t::OLD_CLIENTBOUND, "MOVE_ENTITY_POS_ROT").unwrap(),
        &payload,
    )
    .unwrap();
    let decoded =
        horizon_protocol::packets::play::Clientbound::decode(packets[0].0, &packets[0].1).unwrap();
    match decoded {
        horizon_protocol::packets::play::Clientbound::MoveEntityPosRot(p) => {
            assert!(p.0.on_ground);
            assert_eq!(p.0.id, 1);
            assert!(
                matches!(p.0.delta,Some(horizon_protocol::packets::play::VecDelta::Stepped(ref steps)) if steps==&vec![([-1,10,20],3)])
            );
        }
        _ => panic!("wrong translated movement variant"),
    }
}

#[test]
fn item_ids_and_scalar_components_round_trip_through_native_codecs() {
    use horizon_protocol::{
        buf::{Codec, Reader, Writer},
        item::{Component, Slot, component},
        vfp_items as items, vfp_tables as t,
    };
    let mut old = Writer::new();
    old.varint(2);
    old.varint(72);
    old.varint(1);
    old.varint(0);
    old.varint(t::DATA_COMPONENT_TYPE.inverse(component::DAMAGE).unwrap());
    old.varint(9);
    let mut mapped = Writer::new();
    items::stack_262(&mut Reader::new(&old.data), &mut mapped, true, false).unwrap();
    let stack = Slot::read(&mut Reader::new(&mapped.data)).unwrap().unwrap();
    assert_eq!(stack.item, 73);
    assert_eq!(stack.count, 2);
    assert_eq!(stack.component(component::DAMAGE), Some(&Component::Int(9)));
    let mut restored = Writer::new();
    items::stack_262(&mut Reader::new(&mapped.data), &mut restored, false, false).unwrap();
    assert_eq!(restored.data, old.data);
}
#[test]
fn inventory_packets_translate_all_slots_and_reject_unconverted_components() {
    use horizon_protocol::{packets::play, vfp_items as items, vfp_tables as t, vfp_wire as wire};
    // Container 0, state 1, two slots: 3 old item 72, empty; empty cursor.
    let raw = vec![0, 1, 2, 3, 72, 0, 0, 0, 0];
    let packets = items::clientbound_262(
        wire::packet_id(t::OLD_CLIENTBOUND, "CONTAINER_SET_CONTENT").unwrap(),
        &raw,
    )
    .unwrap();
    match play::Clientbound::decode(packets[0].0, &packets[0].1).unwrap() {
        play::Clientbound::ContainerSetContent(p) => {
            assert_eq!(p.items.len(), 2);
            assert_eq!(p.items[0].as_ref().unwrap().item, 73);
            assert!(p.items[1].is_none());
            assert!(p.carried.is_none());
        }
        _ => panic!("wrong inventory packet"),
    }
    let mut invalid = horizon_protocol::buf::Writer::new();
    invalid.varint(1);
    invalid.varint(72);
    invalid.varint(1);
    invalid.varint(0);
    invalid.varint(
        t::DATA_COMPONENT_TYPE
            .inverse(horizon_protocol::item::component::PROFILE)
            .unwrap(),
    );
    assert!(
        items::stack_262(
            &mut horizon_protocol::buf::Reader::new(&invalid.data),
            &mut horizon_protocol::buf::Writer::new(),
            true,
            false
        )
        .is_err()
    );
}
#[test]
fn native_and_disabled_connections_remain_byte_exact() {
    let _lock = SERIAL.lock().unwrap();
    for (target, enabled) in [("native", true), ("26.1", false)] {
        configuration(target, enabled);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut server, _) = listener.accept().unwrap();
            assert_eq!(vfp::read_frame(&mut server).unwrap(), handshake());
        });
        let mut client =
            Connection::connect("127.0.0.1", addr.port(), Duration::from_secs(3)).unwrap();
        client.send(&handshake()).unwrap();
        worker.join().unwrap();
    }
    vfp::configure(vfp::Config::default());
}
#[test]
fn integrated_server_is_excluded_even_when_translation_is_forced() {
    let _lock = SERIAL.lock().unwrap();
    configuration("26.1", true);
    let (a, b) = Pipe::pair();
    let mut client = Connection::local(a);
    let mut server = Connection::local(b);
    client.send(&handshake()).unwrap();
    let (id, payload) = server.recv().unwrap();
    let mut actual = vec![id as u8];
    actual.extend_from_slice(&payload);
    assert_eq!(actual, handshake());
    vfp::configure(vfp::Config::default());
}
#[test]
fn unsupported_release_is_rejected_before_handshake_bytes_are_written() {
    let _lock = SERIAL.lock().unwrap();
    configuration("1.6.4", true);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (mut server, _) = listener.accept().unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        use std::io::Read;
        let mut byte = [0];
        assert_eq!(server.read(&mut byte).unwrap(), 0);
    });
    let mut client = Connection::connect("127.0.0.1", addr.port(), Duration::from_secs(3)).unwrap();
    assert!(
        client
            .send(&handshake())
            .unwrap_err()
            .to_string()
            .contains("not implemented")
    );
    drop(client);
    worker.join().unwrap();
    vfp::configure(vfp::Config::default());
}
#[test]
fn changing_target_during_a_connection_cannot_rewrite_later_packets() {
    let _lock = SERIAL.lock().unwrap();
    configuration("native", true);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (mut server, _) = listener.accept().unwrap();
        assert_eq!(vfp::read_frame(&mut server).unwrap(), handshake());
        // Deliberately looks exactly like a handshake, but is the second frame.
        assert_eq!(vfp::read_frame(&mut server).unwrap(), handshake());
    });
    let mut client = Connection::connect("127.0.0.1", addr.port(), Duration::from_secs(3)).unwrap();
    client.send(&handshake()).unwrap();
    configuration("26.1", true);
    client.send(&handshake()).unwrap();
    drop(client);
    worker.join().unwrap();
    vfp::configure(vfp::Config::default());
}
