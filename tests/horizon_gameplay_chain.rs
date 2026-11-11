// SPDX-License-Identifier: GPL-3.0-or-later
use horizon_protocol::{
    buf::{Codec, Reader, Writer},
    chunk::{BlockEntityInfo, ChunkData},
    entity_data::Value,
    item::{Component, Slot},
    packets::{login, play},
    vfp_core as core, vfp_items as items,
    vfp_mapping::{Maps, V261, V262},
    vfp_session::Session,
    vfp_tables as t, vfp_tables_261_native as types,
    vfp_transition_261::{self as transition, Entities},
    vfp_wire as wire,
};
use horizon_world::nbt::Tag;

#[test]
fn early_release_air_use_preserves_the_unsigned_y_sentinel() {
    use horizon_protocol::{vfp_release_chain::Chain, vfp_tables_1193};
    for protocol in [4, 5, 47] {
        let mut chain = Chain::new(protocol, None).unwrap();
        let mut use_item = Writer::new();
        use_item.varint(0); // Main hand.
        use_item.varint(1); // Native prediction sequence.
        let packets = chain
            .serverbound(vec![(
                wire::packet_id(vfp_tables_1193::SERVERBOUND, "USE_ITEM").unwrap(),
                use_item.data,
            )])
            .unwrap();
        assert_eq!(packets.len(), 1);
        let mut r = Reader::new(&packets[0].1);
        if protocol == 47 {
            assert_eq!(r.i64().unwrap(), -1);
        } else {
            assert_eq!(r.i32().unwrap(), -1);
            assert_eq!(r.u8().unwrap(), 255);
            assert_eq!(r.i32().unwrap(), -1);
        }
        assert_eq!(r.u8().unwrap(), 255); // Air-use face.
        assert_eq!(r.i16().unwrap(), -1); // Empty hand.
        assert_eq!(r.bytes(3).unwrap(), &[0, 0, 0]);
        assert_eq!(r.remaining(), 0);
    }
}

#[test]
fn early_release_block_use_keeps_height_bounds_and_held_item() {
    use horizon_protocol::{
        vfp_release_17::Legacy,
        vfp_release_chain_data::STEPS,
        vfp_release_codec::{Position, Stack},
    };
    let step = STEPS.iter().find(|step| step.source == 5).unwrap();
    let stack = Stack {
        id: 1,
        count: 12,
        damage: 0,
        tag: Tag::End,
    };
    for (position, face, valid) in [
        (
            Position {
                x: -20,
                y: 0,
                z: 30,
            },
            1,
            true,
        ),
        (
            Position {
                x: 20,
                y: 255,
                z: -30,
            },
            5,
            true,
        ),
        (
            Position {
                x: -1,
                y: -1,
                z: -1,
            },
            255,
            true,
        ),
        (
            Position {
                x: -1,
                y: -1,
                z: -1,
            },
            1,
            false,
        ),
        (Position { x: 0, y: -2, z: 0 }, 255, false),
        (Position { x: 0, y: 256, z: 0 }, 1, false),
    ] {
        let mut w = Writer::new();
        w.i64(position.pack(47).unwrap());
        w.u8(face);
        Stack::write(Some(&stack), &mut w, 47).unwrap();
        w.bytes(&[8, 16, 4]);
        let result = Legacy::default().serverbound(
            step,
            vec![(
                wire::packet_id(step.next_serverbound, "USE_ITEM_ON").unwrap(),
                w.data,
            )],
        );
        if !valid {
            assert!(result.is_err(), "{position:?}");
            continue;
        }
        let packets = result.unwrap();
        let mut r = Reader::new(&packets[0].1);
        assert_eq!(r.i32().unwrap(), position.x);
        assert_eq!(r.u8().unwrap(), position.y as u8);
        assert_eq!(r.i32().unwrap(), position.z);
        assert_eq!(r.u8().unwrap(), face);
        assert_eq!(Stack::read(&mut r, 5).unwrap(), Some(stack.clone()));
        assert_eq!(r.bytes(3).unwrap(), &[8, 16, 4]);
        assert_eq!(r.remaining(), 0);
    }
}
fn old(name: &str) -> i32 {
    wire::packet_id(t::OLD_CLIENTBOUND, name).unwrap()
}
fn native(name: &str) -> i32 {
    wire::packet_id(t::NEW_CLIENTBOUND, name).unwrap()
}
fn kind(name: &str) -> i32 {
    types::ENTITY_NAMES.iter().position(|&n| n == name).unwrap() as i32
}
fn decode(p: &wire::Packet) -> play::Clientbound {
    play::Clientbound::decode(p.0, &p.1).unwrap()
}
fn payload(packet: play::Clientbound) -> Vec<u8> {
    let bytes = packet.encode();
    let mut r = Reader::new(&bytes);
    r.varint().unwrap();
    r.rest().to_vec()
}
fn spawn(id: i32, kind: i32, data: i32) -> Vec<u8> {
    payload(play::Clientbound::AddEntity(play::AddEntity {
        id,
        uuid: 7,
        kind,
        position: [1.0, 64.0, 2.0],
        velocity: [0.0; 3],
        pitch: 0,
        yaw: 0,
        head_yaw: 0,
        data,
    }))
}
fn first_changed_item() -> i32 {
    (0..V261::ITEMS.size)
        .find(|&id| V261::ITEMS.map(id).unwrap() != V262::ITEMS.map(id).unwrap())
        .unwrap()
}
#[test]
fn source_maps_reach_nested_items_and_shifted_component_types() {
    let id = first_changed_item();
    let expected = V261::ITEMS.map(id).unwrap();
    let mut w = Writer::new();
    w.varint(1);
    w.varint(id);
    w.varint(2);
    w.varint(0);
    w.varint(V261::DATA_COMPONENT_TYPE.inverse(52).unwrap());
    w.varint(1);
    w.varint(id);
    w.varint(3);
    w.varint(0);
    w.varint(0);
    w.varint(V261::DATA_COMPONENT_TYPE.inverse(81).unwrap());
    w.nbt(&Tag::compound([("test", Tag::Int(2))]));
    let mut out = Writer::new();
    items::stack_with::<V261>(&mut Reader::new(&w.data), &mut out, true, false).unwrap();
    let p = Slot::read(&mut Reader::new(&out.data)).unwrap().unwrap();
    assert_eq!(p.item, expected);
    let Some(Component::Stacks(stacks)) = p.component(52) else {
        panic!()
    };
    assert_eq!(stacks[0].item, expected);
    assert_eq!(stacks[0].count, 3);
    assert!(p.component(81).is_some());
    let mut restored = Writer::new();
    items::stack_with::<V261>(&mut Reader::new(&out.data), &mut restored, false, false).unwrap();
    assert_eq!(restored.data, w.data);
}
#[test]
fn attribute_updates_and_item_modifiers_use_the_261_attribute_registry() {
    let packet = play::Clientbound::UpdateAttributes(play::UpdateAttributes {
        id: 3,
        values: vec![play::AttributeSnapshot {
            attribute: 0,
            base: 2.0,
            modifiers: Vec::new(),
        }],
    });
    let packets =
        items::clientbound_with::<V261>(old("UPDATE_ATTRIBUTES"), &payload(packet)).unwrap();
    let play::Clientbound::UpdateAttributes(p) = decode(&packets[0]) else {
        panic!()
    };
    assert_eq!(p.values[0].attribute, V261::ATTRIBUTES.map(0).unwrap());
    let mut w = Writer::new();
    w.varint(1);
    w.varint(1);
    w.varint(1);
    w.varint(0);
    w.varint(16);
    w.varint(1);
    w.varint(0);
    w.string("test:modifier");
    w.f64(1.5);
    w.varint(0);
    w.varint(0);
    w.varint(0);
    let mut out = Writer::new();
    items::stack_with::<V261>(&mut Reader::new(&w.data), &mut out, true, false).unwrap();
    let stack = Slot::read(&mut Reader::new(&out.data)).unwrap().unwrap();
    let Some(Component::Raw(data)) = stack.component(16) else {
        panic!()
    };
    let mut r = Reader::new(data);
    assert_eq!(r.varint().unwrap(), 1);
    assert_eq!(r.varint().unwrap(), V261::ATTRIBUTES.map(0).unwrap());
    let mut restored = Writer::new();
    items::stack_with::<V261>(&mut Reader::new(&out.data), &mut restored, false, false).unwrap();
    assert_eq!(restored.data, w.data);
}
#[test]
fn falling_block_spawn_data_is_remapped_for_both_source_versions() {
    fn check<M: Maps>() {
        let entity = M::ENTITIES.inverse(kind("falling_block")).unwrap();
        let state = (0..M::BLOCKSTATES.size)
            .find(|&id| M::BLOCKSTATES.map(id).unwrap() != id)
            .unwrap();
        let packets =
            items::clientbound_with::<M>(old("ADD_ENTITY"), &spawn(3, entity, state)).unwrap();
        let play::Clientbound::AddEntity(p) = decode(&packets[0]) else {
            panic!()
        };
        assert_eq!(p.data, M::BLOCKSTATES.map(state).unwrap());
        assert_eq!(p.kind, kind("falling_block"));
    }
    check::<V261>();
    check::<V262>();
}
#[test]
fn entity_zero_is_aliased_consistently_in_projectiles_actions_and_spectating() {
    let mut ids = Entities::with_alias(-42).unwrap();
    let mut packets = vec![(native("ADD_ENTITY"), spawn(0, kind("snowball"), 0))];
    ids.clientbound(&mut packets).unwrap();
    let play::Clientbound::AddEntity(p) = decode(&packets[0]) else {
        panic!()
    };
    assert_eq!((p.id, p.data), (-42, -42));
    let mut attack = Writer::new();
    attack.varint(-42);
    let attack_id = wire::packet_id(t::OLD_SERVERBOUND, "ATTACK").unwrap();
    let mut packets = vec![(attack_id, attack.data)];
    ids.serverbound(&mut packets).unwrap();
    assert_eq!(packets[0].1, vec![0]);
    let spectate = wire::packet_id(t::OLD_SERVERBOUND, "SPECTATE_ENTITY").unwrap();
    let mut none = vec![(spectate, vec![0])];
    ids.serverbound(&mut none).unwrap();
    assert_eq!(
        none,
        vec![(
            wire::packet_id(t::OLD_SERVERBOUND, "SWING").unwrap(),
            vec![0]
        )]
    );
    let mut some = Writer::new();
    some.varint(-41);
    let mut packets = vec![(spectate, some.data)];
    ids.serverbound(&mut packets).unwrap();
    assert_eq!(packets, vec![(spectate, vec![0])]);
    let mut link = Writer::new();
    link.i32(0);
    link.i32(0);
    let mut packets = vec![(native("SET_ENTITY_LINK"), link.data)];
    ids.clientbound(&mut packets).unwrap();
    assert_eq!(&packets[0].1[..4], &(-42i32).to_be_bytes());
    assert_eq!(&packets[0].1[4..], &0i32.to_be_bytes());
}
fn metadata(id: i32, index: u8, value: i32, serializer: i32) -> wire::Packet {
    let mut w = Writer::new();
    w.varint(id);
    w.u8(index);
    w.varint(serializer);
    w.varint(value);
    w.u8(255);
    (native("SET_ENTITY_DATA"), w.data)
}
#[test]
fn metadata_12111_keeps_the_last_value_for_a_repeated_index() {
    use horizon_protocol::{
        vfp_mapping::V12111, vfp_registry_access::Registries, vfp_tables_12111,
        vfp_transition_12111::Transition,
    };

    let packet_id = wire::packet_id(vfp_tables_12111::CLIENTBOUND, "SET_ENTITY_DATA").unwrap();
    let registries = Registries::default();
    let mut transition = Transition::default();
    // Entity 7, index 1 = 10, index 0 = 3, then index 1 = 20.
    let payload = [7, 1, 1, 10, 0, 0, 3, 1, 1, 20, 255];
    let (id, data) = transition
        .clientbound(packet_id, &payload, &registries)
        .unwrap();
    let packets = items::clientbound_with::<V12111>(id, &data).unwrap();
    assert_eq!(packets.len(), 1);
    let play::Clientbound::SetEntityData(packet) = decode(&packets[0]) else {
        panic!("Expected a translated entity metadata packet");
    };
    assert_eq!(packet.id, 7);
    assert_eq!(
        packet.values,
        vec![(0, 0, Value::Byte(3)), (1, 1, Value::Int(20))]
    );

    let conflicting_serializer = [7, 1, 1, 10, 1, 0, 20, 255];
    assert!(
        transition
            .clientbound(packet_id, &conflicting_serializer, &registries)
            .is_err()
    );
    let truncated_value = [7, 1, 1, 10, 1, 1, 0x80];
    assert!(
        transition
            .clientbound(packet_id, &truncated_value, &registries)
            .is_err()
    );
}
#[test]
fn cube_metadata_and_entity_references_change_without_touching_default_zero_sentinels() {
    let mut ids = Entities::with_alias(-42).unwrap();
    let mut spawn_cube = vec![(native("ADD_ENTITY"), spawn(5, kind("slime"), 0))];
    ids.clientbound(&mut spawn_cube).unwrap();
    let mut packets = vec![metadata(5, 16, 4, 1)];
    ids.clientbound(&mut packets).unwrap();
    let play::Clientbound::SetEntityData(p) = decode(&packets[0]) else {
        panic!()
    };
    assert_eq!(p.values[0].0, 18);
    let mut spawn_frog = vec![(native("ADD_ENTITY"), spawn(6, kind("frog"), 0))];
    ids.clientbound(&mut spawn_frog).unwrap();
    let mut packets = vec![metadata(6, 19, 1, 19)];
    ids.clientbound(&mut packets).unwrap();
    let play::Clientbound::SetEntityData(p) = decode(&packets[0]) else {
        panic!()
    };
    assert_eq!(p.values[0].2, Value::Id(-41));
    let mut packets = vec![metadata(7, 17, 0, 1)];
    ids.clientbound(&mut packets).unwrap();
    let play::Clientbound::SetEntityData(p) = decode(&packets[0]) else {
        panic!()
    };
    assert_eq!(p.values[0].2, Value::Int(0));
    let mut removed = vec![(
        native("REMOVE_ENTITIES"),
        payload(play::Clientbound::RemoveEntities(play::RemoveEntities {
            ids: vec![5],
        })),
    )];
    ids.clientbound(&mut removed).unwrap();
    let mut packets = vec![metadata(5, 16, 4, 1)];
    ids.clientbound(&mut packets).unwrap();
    let play::Clientbound::SetEntityData(p) = decode(&packets[0]) else {
        panic!()
    };
    assert_eq!(p.values[0].0, 16);
}
#[test]
fn team_layout_moves_flags_and_affixes_and_handles_reset_color() {
    let mut w = Writer::new();
    w.string("fixture");
    w.i8(0);
    w.nbt(&Tag::String("Display".into()));
    w.i8(3);
    w.varint(2);
    w.varint(1);
    w.varint(21);
    w.nbt(&Tag::String("[".into()));
    w.nbt(&Tag::String("]".into()));
    w.varint(1);
    w.string("Player");
    let prepared = transition::prepare("SET_PLAYER_TEAM", &w.data, false).unwrap();
    let p = items::clientbound_with::<V261>(old("SET_PLAYER_TEAM"), &prepared).unwrap();
    let play::Clientbound::SetPlayerTeam(p) = decode(&p[0]) else {
        panic!()
    };
    let fields = p.parameters.unwrap();
    assert_eq!(fields.options, 3);
    assert_eq!(fields.color, None);
    assert_eq!(fields.name_tag_visibility, 2);
    assert_eq!(fields.prefix, Tag::String("[".into()));
    assert_eq!(p.players, vec!["Player"]);
}
#[test]
fn obsolete_bed_block_entities_are_removed_and_other_types_are_remapped() {
    let mut w = Writer::new();
    w.i32(0);
    w.i32(0);
    ChunkData::write(
        &ChunkData {
            heightmaps: Vec::new(),
            buffer: vec![0, 1, 0, 0, 0, 27, 0, 0],
            block_entities: vec![
                BlockEntityInfo {
                    packed_xz: 0,
                    y: 64,
                    kind: 25,
                    tag: Tag::compound([] as [(&str, Tag); 0]),
                },
                BlockEntityInfo {
                    packed_xz: 1,
                    y: 64,
                    kind: 26,
                    tag: Tag::compound([] as [(&str, Tag); 0]),
                },
            ],
        },
        &mut w,
    );
    w.bytes(&[0; 6]);
    let p = items::clientbound_with::<V261>(old("LEVEL_CHUNK_WITH_LIGHT"), &w.data).unwrap();
    let play::Clientbound::LevelChunkWithLight(p) = decode(&p[0]) else {
        panic!()
    };
    assert_eq!(p.chunk.block_entities.len(), 1);
    assert_eq!(p.chunk.block_entities[0].kind, 25);
    let mut w = Writer::new();
    play::BlockEntityData::write(
        &play::BlockEntityData {
            pos: [0, 64, 0],
            kind: 25,
            tag: Tag::compound([] as [(&str, Tag); 0]),
        },
        &mut w,
    );
    assert!(
        items::clientbound_with::<V261>(old("BLOCK_ENTITY_DATA"), &w.data)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn obsolete_bed_item_data_has_a_lossless_backup_instead_of_an_invalid_type() {
    let mut w = Writer::new();
    w.varint(1);
    w.varint(72);
    w.varint(1);
    w.varint(0);
    w.varint(V261::DATA_COMPONENT_TYPE.inverse(62).unwrap());
    w.varint(25);
    w.nbt(&Tag::compound([("test", Tag::Int(3))]));
    let mut out = Writer::new();
    items::stack_with::<V261>(&mut Reader::new(&w.data), &mut out, true, false).unwrap();
    let p = Slot::read(&mut Reader::new(&out.data)).unwrap().unwrap();
    assert!(p.component(62).is_none());
    assert!(p.component(0).is_some());
    let mut restored = Writer::new();
    items::stack_with::<V261>(&mut Reader::new(&out.data), &mut restored, false, false).unwrap();
    assert_eq!(restored.data, w.data);
}
fn fixture(encrypted: bool) -> Session {
    let mut session = Session::for_release_fixture("26.1").unwrap();
    if encrypted {
        session
            .serverbound(
                &login::Serverbound::Key(login::Key {
                    key: vec![1],
                    challenge: vec![2],
                })
                .encode(),
            )
            .unwrap();
    }
    let mut success = vec![0; 16];
    core::put_string(&mut success, "Horizon");
    success.push(0);
    session.clientbound(2, &success).unwrap();
    session.serverbound(&[3]).unwrap();
    session.clientbound(3, &[]).unwrap();
    session.poll().unwrap().unwrap();
    session.poll().unwrap().unwrap();
    session.serverbound(&[3]).unwrap();
    session
}
fn old_login() -> Vec<u8> {
    let mut w = Writer::new();
    w.i32(0);
    w.bool(false);
    w.varint(1);
    w.string("minecraft:overworld");
    w.varint(20);
    w.varint(8);
    w.varint(8);
    w.bool(false);
    w.bool(true);
    w.bool(false);
    w.varint(0);
    w.string("minecraft:overworld");
    w.i64(0);
    w.i8(1);
    w.i8(-1);
    w.bool(false);
    w.bool(false);
    w.bool(false);
    w.varint(0);
    w.varint(63);
    w.bool(false);
    w.data
}
#[test]
fn full_261_fixture_preserves_online_mode_order_and_zero_entity_identity() {
    for encrypted in [false, true] {
        let mut session = fixture(encrypted);
        let p = session.clientbound(old("LOGIN"), &old_login()).unwrap();
        let play::Clientbound::Login(login) = decode(&p) else {
            panic!()
        };
        assert_eq!(login.online_mode, encrypted);
        assert!(!login.enforces_secure_chat);
        assert!(login.player_id < -1);
        let packets = session.clientbound(old("SET_CAMERA"), &[0]).unwrap();
        let mut r = Reader::new(&packets.1);
        assert_eq!(r.varint().unwrap(), login.player_id);
        let mut attack = Writer::new();
        attack.varint(wire::packet_id(t::NEW_SERVERBOUND, "ATTACK").unwrap());
        attack.varint(login.player_id);
        let packets = session.serverbound(&attack.data).unwrap();
        let mut r = Reader::new(&packets[0]);
        assert_eq!(
            r.varint().unwrap(),
            wire::packet_id(t::OLD_SERVERBOUND, "ATTACK").unwrap()
        );
        assert_eq!(r.varint().unwrap(), 0);
        let mut bed = Writer::new();
        play::BlockEntityData::write(
            &play::BlockEntityData {
                pos: [0, 64, 0],
                kind: 25,
                tag: Tag::compound([] as [(&str, Tag); 0]),
            },
            &mut bed,
        );
        assert!(
            session
                .ingest(old("BLOCK_ENTITY_DATA"), &bed.data)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn metadata_keeps_partial_profile_resolution_and_texture_overrides_byte_exact() {
    let mut w = Writer::new();
    w.varint(19);
    w.u8(12);
    w.varint(41);
    w.bool(false); // Partial, even with both a name and UUID.
    w.bool(true);
    w.string("Horizon");
    w.bool(true);
    w.uuid(123);
    w.varint(0);
    w.bool(true);
    w.string("test:skin");
    w.bool(false);
    w.bool(false);
    w.bool(true);
    w.bool(true);
    w.u8(255);
    for converted in [
        items::clientbound_with::<V261>(old("SET_ENTITY_DATA"), &w.data).unwrap(),
        items::clientbound_with::<V262>(old("SET_ENTITY_DATA"), &w.data).unwrap(),
    ] {
        assert_eq!(converted[0], (native("SET_ENTITY_DATA"), w.data.clone()));
        let play::Clientbound::SetEntityData(p) = decode(&converted[0]) else {
            panic!()
        };
        let Value::Profile(p) = &p.values[0].2 else {
            panic!()
        };
        assert_eq!(p.body.as_deref(), Some("test:skin"));
        assert_eq!(p.uuid, Some(123));
    }
}

#[test]
fn suppressed_bed_packet_preserves_the_following_encrypted_compressed_frame() {
    use horizon_protocol::{connection::Connection, connection::vfp_bridge};
    use std::{net::TcpListener, sync::atomic::Ordering, time::Duration};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut server = Connection::from_stream(stream).unwrap();
        server.enable_encryption(&[0x72; 16]);
        server.set_compression(1);
        let mut bed = Writer::new();
        bed.varint(old("BLOCK_ENTITY_DATA"));
        play::BlockEntityData::write(
            &play::BlockEntityData {
                pos: [1, 64, 2],
                kind: 25,
                tag: Tag::compound([("old", Tag::Int(7))]),
            },
            &mut bed,
        );
        server.send(&bed.data).unwrap();
        let mut keepalive = Writer::new();
        keepalive.varint(old("KEEP_ALIVE"));
        keepalive.i64(918273);
        server.send(&keepalive.data).unwrap();
    });
    let mut client = Connection::connect("127.0.0.1", addr.port(), Duration::from_secs(3)).unwrap();
    client
        .reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    client.enable_encryption(&[0x72; 16]);
    client.set_compression(1);
    vfp_bridge::install_protocol_fixture(&client.reader, fixture(false));
    let (mut reader, _writer) = client.split();
    let received = std::thread::spawn(move || {
        let packet = reader.recv().unwrap();
        assert_eq!(
            packet,
            (native("KEEP_ALIVE"), 918273i64.to_be_bytes().to_vec())
        );
        assert_eq!(reader.counters.received.load(Ordering::Relaxed), 2);
    });
    worker.join().unwrap();
    received.join().unwrap();
}

fn adventure_item<M: Maps>(partial: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(1);
    w.varint(1);
    w.varint(1);
    w.varint(0);
    w.varint(M::DATA_COMPONENT_TYPE.inverse(15).unwrap());
    w.varint(1); // One block predicate.
    w.bool(true);
    w.varint(2); // Explicit holder set of one block.
    w.varint(23);
    w.bool(false); // No state properties.
    w.bool(false); // No block entity NBT.
    w.varint(0); // No exact component values.
    partial(&mut w);
    w.data
}

#[test]
fn partial_predicates_map_component_ids_and_preserve_registry_aware_nbt() {
    fn check<M: Maps>() {
        let component = M::DATA_COMPONENT_TYPE.inverse(81).unwrap();
        let original = adventure_item::<M>(|w| {
            w.varint(2);
            w.bool(true);
            w.varint(0);
            w.nbt(&Tag::compound([("test", Tag::Int(5))]));
            w.bool(false);
            w.varint(component);
            w.nbt(&Tag::compound([(
                "id",
                Tag::String("test:unchanged".into()),
            )]));
        });
        let mut out = Writer::new();
        items::stack_with::<M>(&mut Reader::new(&original), &mut out, true, false).unwrap();
        let stack = Slot::read(&mut Reader::new(&out.data)).unwrap().unwrap();
        let Some(Component::Raw(data)) = stack.component(15) else {
            panic!()
        };
        let mut r = Reader::new(data);
        assert_eq!(r.varint().unwrap(), 1);
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), 2);
        assert_eq!(r.varint().unwrap(), M::BLOCKS.map(23).unwrap());
        assert!(!r.bool().unwrap());
        assert!(!r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), 0);
        assert_eq!(r.varint().unwrap(), 2);
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), 0);
        assert_eq!(r.nbt().unwrap(), Tag::compound([("test", Tag::Int(5))]));
        assert!(!r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), 81);
        assert_eq!(
            r.nbt().unwrap(),
            Tag::compound([("id", Tag::String("test:unchanged".into()))])
        );
        assert_eq!(r.remaining(), 0);
        let mut restored = Writer::new();
        items::stack_with::<M>(&mut Reader::new(&out.data), &mut restored, false, false).unwrap();
        assert_eq!(restored.data, original);
    }
    check::<V261>();
    check::<V262>();
}

#[test]
fn partial_predicates_reject_invalid_types_oversized_lists_and_changed_schemas() {
    for source in [
        adventure_item::<V262>(|w| w.varint(65)),
        adventure_item::<V262>(|w| {
            w.varint(1);
            w.bool(true);
            w.varint(-1);
        }),
        adventure_item::<V262>(|w| {
            w.varint(1);
            w.bool(false);
            w.varint(45);
            w.nbt(&Tag::Int(3));
        }),
        adventure_item::<V262>(|w| {
            w.varint(1);
            w.bool(false);
            w.varint(56);
            w.nbt(&Tag::compound([] as [(&str, Tag); 0]));
        }),
    ] {
        assert!(
            items::stack_with::<V262>(&mut Reader::new(&source), &mut Writer::new(), true, false)
                .is_err()
        );
    }
}

fn hover() -> Tag {
    Tag::compound([
        ("text", Tag::String("Hover".into())),
        (
            "hover_event",
            Tag::compound([
                ("action", Tag::String("show_item".into())),
                ("id", Tag::String("minecraft:stone".into())),
                (
                    "components",
                    Tag::compound([("minecraft:map_color", Tag::Int(123))]),
                ),
            ]),
        ),
    ])
}
fn hover_components(tag: &Tag) -> &Tag {
    tag.get("hover_event").unwrap().get("components").unwrap()
}
#[test]
fn metadata_text_converts_hover_values_in_required_and_optional_components() {
    let mut w = Writer::new();
    w.varint(19);
    for (index, kind) in [(2, 5), (3, 6)] {
        w.u8(index);
        w.varint(kind);
        if kind == 6 {
            w.bool(true);
        }
        w.nbt(&hover());
    }
    w.u8(4);
    w.varint(6);
    w.bool(false);
    w.u8(255);
    let packets = items::clientbound_with::<V261>(old("SET_ENTITY_DATA"), &w.data).unwrap();
    let play::Clientbound::SetEntityData(p) = decode(&packets[0]) else {
        panic!()
    };
    assert!(
        matches!(&p.values[0].2, Value::Text(t) if hover_components(t).get("minecraft:map_color").is_none())
    );
    assert!(
        matches!(&p.values[1].2, Value::OptText(Some(t)) if hover_components(t).get("minecraft:map_color").is_none())
    );
    assert_eq!(p.values[2].2, Value::OptText(None));
}

#[test]
fn nested_hover_items_are_converted_without_rewriting_arbitrary_custom_data() {
    let custom = hover(); // Deliberately text-shaped arbitrary data.
    let item = Tag::compound([
        ("id", Tag::String("minecraft:stone".into())),
        (
            "components",
            Tag::compound([
                ("map_color", Tag::Int(3)),
                ("custom_data", custom.clone()),
                ("custom_name", hover()),
            ]),
        ),
    ]);
    let mut tag = Tag::compound([
        ("text", Tag::String("Container".into())),
        (
            "hover_event",
            Tag::compound([
                ("action", Tag::String("show_item".into())),
                ("id", Tag::String("minecraft:bundle".into())),
                (
                    "components",
                    Tag::compound([
                        ("minecraft:bundle_contents", Tag::List(vec![item])),
                        ("minecraft:custom_data", custom.clone()),
                    ]),
                ),
            ]),
        ),
    ]);
    horizon_protocol::vfp_play::text(&mut tag).unwrap();
    let c = hover_components(&tag);
    assert_eq!(c.get("minecraft:custom_data"), Some(&custom));
    let Some(Tag::List(items)) = c.get("minecraft:bundle_contents") else {
        panic!()
    };
    let c = items[0].get("components").unwrap();
    assert!(c.get("map_color").is_none());
    assert_eq!(c.get("custom_data"), Some(&custom));
    assert!(
        hover_components(c.get("custom_name").unwrap())
            .get("minecraft:map_color")
            .is_none()
    );
    let mut deep = hover();
    for _ in 0..66 {
        deep = Tag::compound([("extra", Tag::List(vec![deep]))]);
    }
    let original = deep.clone();
    assert!(horizon_protocol::vfp_play::text(&mut deep).is_err());
    assert_eq!(deep, original);
}
fn text_item<M: Maps>(kind: i32, bytes: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    for value in [1, 1, 1, 0, M::DATA_COMPONENT_TYPE.inverse(kind).unwrap()] {
        w.varint(value);
    }
    w.bytes(bytes);
    w.data
}
fn round_text<M: Maps>(
    original: &[u8],
    edit: impl FnOnce(&mut Option<horizon_protocol::item::Stack>),
) -> Vec<u8> {
    let mut mapped = Writer::new();
    items::stack_with::<M>(&mut Reader::new(original), &mut mapped, true, false).unwrap();
    let mut slot = Slot::read(&mut Reader::new(&mapped.data)).unwrap();
    edit(&mut slot);
    let mut native = Writer::new();
    Slot::write(&slot, &mut native);
    let mut restored = Writer::new();
    items::stack_with::<M>(&mut Reader::new(&native.data), &mut restored, false, false).unwrap();
    restored.data
}
#[test]
fn item_name_and_lore_hover_changes_restore_the_original_server_values() {
    for kind in [6, 9, 11] {
        let mut w = Writer::new();
        if kind == 11 {
            w.varint(1);
        }
        w.nbt(&hover());
        let original = text_item::<V261>(kind, &w.data);
        let restored = round_text::<V261>(&original, |slot| {
            let value = slot.as_ref().unwrap().component(kind).unwrap();
            let tag = match value {
                Component::Text(t) => t,
                Component::Lore(l) => &l[0],
                _ => panic!(),
            };
            assert!(hover_components(tag).get("minecraft:map_color").is_none());
        });
        assert_eq!(restored, original);
    }
}
#[test]
fn written_books_keep_filtered_variants_through_the_actual_native_slot_writer() {
    fn check<M: Maps>() {
        let mut w = Writer::new();
        w.string("Title");
        w.bool(true);
        w.string("Filtered title");
        w.string("Author");
        w.varint(2);
        w.varint(1);
        w.nbt(&hover());
        w.bool(true);
        w.nbt(&Tag::String("Filtered page".into()));
        w.bool(true);
        let original = text_item::<M>(57, &w.data);
        let restored = round_text::<M>(&original, |slot| {
            let Some(Component::WrittenBook(book)) = slot.as_ref().unwrap().component(57) else {
                panic!()
            };
            assert!(
                hover_components(&book.pages[0])
                    .get("minecraft:map_color")
                    .is_none()
            );
        });
        assert_eq!(restored, original);
        let edited = round_text::<M>(&original, |slot| {
            let stack = slot.as_mut().unwrap();
            let (_, Component::WrittenBook(book)) =
                stack.added.iter_mut().find(|(id, _)| *id == 57).unwrap()
            else {
                panic!()
            };
            book.pages[0] = Tag::String("Edited".into());
        });
        assert_ne!(edited, original);
        let mut mapped = Writer::new();
        items::stack_with::<M>(&mut Reader::new(&edited), &mut mapped, true, false).unwrap();
        let stack = Slot::read(&mut Reader::new(&mapped.data)).unwrap().unwrap();
        let Some(Component::WrittenBook(book)) = stack.component(57) else {
            panic!()
        };
        assert_eq!(book.pages, vec![Tag::String("Edited".into())]);
    }
    check::<V261>();
    check::<V262>();
}

#[test]
fn spectator_optional_entity_id_uses_java_integer_wraparound() {
    let ids = Entities::with_alias(-42).unwrap();
    let mut w = Writer::new();
    w.varint(i32::MIN);
    let mut packets = vec![(
        wire::packet_id(t::OLD_SERVERBOUND, "SPECTATE_ENTITY").unwrap(),
        w.data,
    )];
    ids.serverbound(&mut packets).unwrap();
    assert_eq!(Reader::new(&packets[0].1).varint().unwrap(), i32::MAX);
}
