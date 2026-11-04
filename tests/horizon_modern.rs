// SPDX-License-Identifier: GPL-3.0-or-later
use horizon_protocol::{
    buf::{Codec, Reader, Writer},
    item::Slot,
    packets::{configuration as cfg, play},
    vfp_core as core, vfp_full_click as full,
    vfp_mapping::{Maps, V120, V1193, V1194, V1202, V1203},
    vfp_registry_access::Registries,
    vfp_session::Session,
    vfp_tables as native,
    vfp_wire::{self as wire, Packet},
};
use horizon_world::nbt::{self, Tag};

fn named(w: &mut Writer, tag: &Tag) {
    w.bytes(
        &nbt::write_named(
            &nbt::NamedTag {
                name: Default::default(),
                tag: tag.clone(),
            },
            nbt::Limits::default(),
        )
        .unwrap(),
    );
}
fn tables(protocol: i32) -> (&'static [&'static str], &'static [&'static str]) {
    match protocol {
        761 => (
            horizon_protocol::vfp_tables_1193::CLIENTBOUND,
            horizon_protocol::vfp_tables_1193::SERVERBOUND,
        ),
        762 => (
            horizon_protocol::vfp_tables_1194::CLIENTBOUND,
            horizon_protocol::vfp_tables_1194::SERVERBOUND,
        ),
        763 => (
            horizon_protocol::vfp_tables_120::CLIENTBOUND,
            horizon_protocol::vfp_tables_120::SERVERBOUND,
        ),
        764 => (
            horizon_protocol::vfp_tables_1202::CLIENTBOUND,
            horizon_protocol::vfp_tables_1202::SERVERBOUND,
        ),
        765 => (
            horizon_protocol::vfp_tables_1203::CLIENTBOUND,
            horizon_protocol::vfp_tables_1203::SERVERBOUND,
        ),
        766 => (
            horizon_protocol::vfp_tables_1205::CLIENTBOUND,
            horizon_protocol::vfp_tables_1205::SERVERBOUND,
        ),
        767 => (
            horizon_protocol::vfp_tables_121::CLIENTBOUND,
            horizon_protocol::vfp_tables_121::SERVERBOUND,
        ),
        768 => (
            horizon_protocol::vfp_tables_1212::CLIENTBOUND,
            horizon_protocol::vfp_tables_1212::SERVERBOUND,
        ),
        769 => (
            horizon_protocol::vfp_tables_1214::CLIENTBOUND,
            horizon_protocol::vfp_tables_1214::SERVERBOUND,
        ),
        770 => (
            horizon_protocol::vfp_tables_1215::CLIENTBOUND,
            horizon_protocol::vfp_tables_1215::SERVERBOUND,
        ),
        771 => (
            horizon_protocol::vfp_tables_1216::CLIENTBOUND,
            horizon_protocol::vfp_tables_1216::SERVERBOUND,
        ),
        772 => (
            horizon_protocol::vfp_tables_1217::CLIENTBOUND,
            horizon_protocol::vfp_tables_1217::SERVERBOUND,
        ),
        773 => (
            horizon_protocol::vfp_tables_1219::CLIENTBOUND,
            horizon_protocol::vfp_tables_12111::SERVERBOUND,
        ),
        774 => (
            horizon_protocol::vfp_tables_12111::CLIENTBOUND,
            horizon_protocol::vfp_tables_12111::SERVERBOUND,
        ),
        775 => (native::OLD_CLIENTBOUND, native::OLD_SERVERBOUND),
        776 => (native::OLD_CLIENTBOUND, native::OLD_SERVERBOUND),
        _ => panic!("unexpected protocol {protocol}"),
    }
}
fn combined_registry() -> Tag {
    Tag::compound([
        (
            "minecraft:dimension_type",
            Tag::compound([
                ("type", Tag::String("minecraft:dimension_type".into())),
                (
                    "value",
                    Tag::List(vec![Tag::compound([
                        ("id", Tag::Int(0)),
                        ("name", Tag::String("minecraft:overworld".into())),
                        (
                            "element",
                            Tag::compound([
                                ("min_y", Tag::Int(-64)),
                                ("height", Tag::Int(384)),
                                ("logical_height", Tag::Int(384)),
                                ("coordinate_scale", Tag::Double(1f64.to_bits())),
                                ("effects", Tag::String("minecraft:overworld".into())),
                                (
                                    "infiniburn",
                                    Tag::String("#minecraft:infiniburn_overworld".into()),
                                ),
                                ("has_skylight", Tag::Byte(1)),
                                ("ambient_light", Tag::Float(0)),
                                ("monster_spawn_light_level", Tag::Int(7)),
                                ("monster_spawn_block_light_limit", Tag::Int(0)),
                            ]),
                        ),
                    ])]),
                ),
            ]),
        ),
        (
            "minecraft:worldgen/biome",
            Tag::compound([
                ("type", Tag::String("minecraft:worldgen/biome".into())),
                (
                    "value",
                    Tag::List(vec![Tag::compound([
                        ("id", Tag::Int(0)),
                        ("name", Tag::String("minecraft:plains".into())),
                        (
                            "element",
                            Tag::compound([
                                ("precipitation", Tag::String("rain".into())),
                                ("has_precipitation", Tag::Byte(1)),
                                ("temperature", Tag::Float(0.8f32.to_bits())),
                                ("downfall", Tag::Float(0.4f32.to_bits())),
                                ("effects", Tag::compound([("sky_color", Tag::Int(7907327))])),
                            ]),
                        ),
                    ])]),
                ),
            ]),
        ),
    ])
}
fn old_join(protocol: i32, registry: &Tag) -> Vec<u8> {
    let mut w = Writer::new();
    w.i32(7);
    w.bool(false);
    w.u8(0);
    w.i8(-1);
    w.varint(1);
    w.string("minecraft:overworld");
    named(&mut w, registry);
    w.string("minecraft:overworld");
    w.string("minecraft:overworld");
    w.i64(0);
    for n in [10, 8, 8] {
        w.varint(n);
    }
    for b in [false, true, false, false, false] {
        w.bool(b);
    }
    if protocol >= 763 {
        w.varint(0);
    }
    w.data
}
fn drain(session: &mut Session, first: Option<Packet>) -> Vec<Packet> {
    let mut packets: Vec<_> = first.into_iter().collect();
    while let Some(packet) = session.poll() {
        packets.push(packet.unwrap());
    }
    packets
}
fn fixture(version: &str) -> Session {
    let protocol = core::release(version).unwrap().protocol;
    let mut session = Session::for_release_fixture(version).unwrap();
    let mut w = Writer::new();
    w.uuid(42);
    w.string("Fixture");
    w.varint(0);
    if matches!(protocol, 766..=767) {
        w.bool(true);
    }
    if protocol == 776 {
        w.uuid(43);
    }
    let profile = session.ingest(2, &w.data).unwrap().unwrap();
    horizon_protocol::packets::login::Clientbound::decode(profile.0, &profile.1).unwrap();
    let ack = session.serverbound(&[3]).unwrap();
    assert_eq!(
        ack,
        if protocol < 764 {
            vec![]
        } else {
            vec![vec![3]]
        }
    );
    let mut configuration = Vec::new();
    if protocol < 764 {
        let first = session
            .ingest(
                wire::packet_id(tables(protocol).0, "LOGIN").unwrap(),
                &old_join(protocol, &combined_registry()),
            )
            .unwrap();
        configuration.extend(drain(&mut session, first));
    } else {
        let first = session
            .ingest(if protocol < 766 { 2 } else { 3 }, &[])
            .unwrap();
        configuration.extend(drain(&mut session, first));
    }
    assert_eq!(configuration.last(), Some(&(3, Vec::new())), "{version}");
    for (id, bytes) in configuration {
        assert!(
            !matches!(
                cfg::Clientbound::decode(id, &bytes).unwrap(),
                cfg::Clientbound::Unknown(_, _)
            ),
            "{version}"
        );
    }
    let reply = session.serverbound(&[3]).unwrap();
    if protocol < 764 {
        assert!(reply.is_empty());
    } else {
        assert_eq!(reply, vec![vec![if protocol < 766 { 2 } else { 3 }]]);
    }
    for (id, data) in drain(&mut session, None) {
        assert!(
            !matches!(
                play::Clientbound::decode(id, &data).unwrap(),
                play::Clientbound::Unknown(_, _)
            ),
            "{version}"
        );
    }
    session
}

#[test]
fn implemented_families_enter_fixture_play_and_route_keepalives_in_both_directions() {
    for version in [
        "1.19.3", "1.19.4", "1.20", "1.20.2", "1.20.3", "1.20.5", "1.21", "1.21.2", "1.21.4",
        "1.21.5", "1.21.6", "1.21.7", "1.21.9", "1.21.11", "26.1", "26.2",
    ] {
        let mut session = fixture(version);
        let protocol = core::release(version).unwrap().protocol;
        let bytes = 9123456789i64.to_be_bytes();
        let packet = session
            .ingest(
                wire::packet_id(tables(protocol).0, "KEEP_ALIVE").unwrap(),
                &bytes,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            packet,
            (
                wire::packet_id(native::NEW_CLIENTBOUND, "KEEP_ALIVE").unwrap(),
                bytes.to_vec()
            ),
            "{version}"
        );
        let mut body = Writer::new();
        body.varint(wire::packet_id(native::NEW_SERVERBOUND, "KEEP_ALIVE").unwrap());
        body.bytes(&bytes);
        let reply = session.serverbound(&body.data).unwrap();
        assert_eq!(reply.len(), 1, "{version}");
        let mut r = Reader::new(&reply[0]);
        assert_eq!(
            r.varint().unwrap(),
            wire::packet_id(tables(protocol).1, "KEEP_ALIVE").unwrap(),
            "{version}"
        );
        assert_eq!(r.rest(), bytes, "{version}");
    }
}
fn item_tag(label: &str) -> Tag {
    Tag::compound([
        ("Damage", Tag::Int(12)),
        (
            "display",
            Tag::compound([
                (
                    "Name",
                    Tag::String(format!("{{\"text\":\"{label}\"}}").as_str().into()),
                ),
                (
                    "Lore",
                    Tag::List(vec![Tag::String("{\"text\":\"Lore\"}".into())]),
                ),
            ]),
        ),
        (
            "Enchantments",
            Tag::List(vec![Tag::compound([
                ("id", Tag::String("minecraft:sharpness".into())),
                ("lvl", Tag::Short(3)),
            ])]),
        ),
        ("test:opaque", Tag::ByteArray(vec![0, 1, 255])),
    ])
}
fn legacy_stack<M: Maps>(tag: &Tag) -> Vec<u8> {
    let mut w = Writer::new();
    w.bool(true);
    w.varint(
        M::ITEM_IDENTIFIERS
            .iter()
            .position(|name| *name == "diamond_sword")
            .unwrap() as i32,
    );
    w.i8(3);
    if M::PROTOCOL < 764 {
        named(&mut w, tag);
    } else {
        w.nbt(tag);
    }
    w.data
}
fn mapped_stack<M: Maps>(tag: &Tag) -> horizon_protocol::item::Stack {
    let original = legacy_stack::<M>(tag);
    let mut r = Reader::new(&original);
    let mut w = Writer::new();
    horizon_protocol::vfp_items_1203::forward::<M>(&mut r, &mut w, &Registries::default()).unwrap();
    assert_eq!(r.remaining(), 0);
    Slot::read(&mut Reader::new(&w.data)).unwrap().unwrap()
}
#[test]
fn legacy_stacks_keep_names_lore_enchantments_and_opaque_nbt_through_native_slot_writes() {
    fn check<M: Maps>() {
        let original = legacy_stack::<M>(&item_tag("Original"));
        let mapped = mapped_stack::<M>(&item_tag("Original"));
        assert!(mapped.component(6).is_some());
        assert!(mapped.component(13).is_some());
        let mut bytes = Writer::new();
        Slot::write(&Some(mapped), &mut bytes);
        let mut restored = Writer::new();
        let mut r = Reader::new(&bytes.data);
        horizon_protocol::vfp_items_1203::reverse::<M>(
            &mut r,
            &mut restored,
            false,
            &Registries::default(),
        )
        .unwrap();
        assert_eq!(r.remaining(), 0);
        assert_eq!(restored.data, original, "{}", M::PROTOCOL);
    }
    check::<V1193>();
    check::<V1194>();
    check::<V120>();
    check::<V1202>();
    check::<V1203>();
}
#[test]
fn full_predictions_retain_distinct_nbt_in_the_same_item_type() {
    let mut session = fixture("1.20.3");
    assert!(session.full_item_predictions());
    let first = mapped_stack::<V1203>(&item_tag("First"));
    let second = mapped_stack::<V1203>(&item_tag("Second"));
    let data = full::encode(
        200,
        3,
        4,
        0,
        0,
        &[(5, Some(first.clone())), (6, Some(second))],
        &Some(first),
    );
    let mut w = Writer::new();
    w.varint(full::INTERNAL_ID);
    w.bytes(&data);
    let outgoing = session.serverbound(&w.data).unwrap();
    assert_eq!(outgoing.len(), 1);
    let mut r = Reader::new(&outgoing[0]);
    assert_eq!(
        r.varint().unwrap(),
        wire::packet_id(
            horizon_protocol::vfp_tables_1203::SERVERBOUND,
            "CONTAINER_CLICK"
        )
        .unwrap()
    );
    assert_eq!(r.u8().unwrap(), 200);
    assert_eq!(r.varint().unwrap(), 3);
    assert_eq!(r.i16().unwrap(), 4);
    assert_eq!(r.i8().unwrap(), 0);
    assert_eq!(r.varint().unwrap(), 0);
    assert_eq!(r.varint().unwrap(), 2);
    for (slot, label) in [(5, "First"), (6, "Second")] {
        assert_eq!(r.i16().unwrap(), slot);
        assert!(r.bool().unwrap());
        r.varint().unwrap();
        assert_eq!(r.i8().unwrap(), 3);
        assert_eq!(r.nbt().unwrap(), item_tag(label));
    }
    assert!(r.bool().unwrap());
    r.varint().unwrap();
    assert_eq!(r.i8().unwrap(), 3);
    assert_eq!(r.nbt().unwrap(), item_tag("First"));
    assert_eq!(r.remaining(), 0);
    assert!(!fixture("1.21.5").full_item_predictions());
}
#[test]
fn full_prediction_deduplicates_slots_and_rejects_trailing_bytes() {
    let first = mapped_stack::<V1203>(&item_tag("First"));
    let second = mapped_stack::<V1203>(&item_tag("Second"));
    let mut data = full::encode(1, 2, 3, 0, 0, &[(5, Some(first)), (5, Some(second))], &None);
    let packets = full::serverbound::<V1203>(&data, &Registries::default()).unwrap();
    let mut r = Reader::new(&packets[0].1);
    r.varint().unwrap();
    r.varint().unwrap();
    r.i16().unwrap();
    r.i8().unwrap();
    r.varint().unwrap();
    assert_eq!(r.varint().unwrap(), 1);
    data.push(0);
    assert!(full::serverbound::<V1203>(&data, &Registries::default()).is_err());
}
#[test]
fn old_sign_text_is_fixed_once_through_the_complete_packet_chain() {
    let mut session = fixture("1.19.4");
    let mut w = Writer::new();
    w.i64(42);
    w.varint(
        V1194::BLOCK_ENTITY_IDENTIFIERS
            .iter()
            .position(|name| *name == "sign")
            .unwrap() as i32,
    );
    named(
        &mut w,
        &Tag::compound([
            ("Text1", Tag::String("{\"text\":\"First\"}".into())),
            ("Text2", Tag::String("\"Second\"".into())),
            ("Text3", Tag::String("\"\"".into())),
            ("Text4", Tag::String("\"\"".into())),
            ("Color", Tag::String("black".into())),
            ("GlowingText", Tag::Byte(0)),
        ]),
    );
    let packet = session
        .ingest(
            wire::packet_id(tables(762).0, "BLOCK_ENTITY_DATA").unwrap(),
            &w.data,
        )
        .unwrap()
        .unwrap();
    let play::Clientbound::BlockEntityData(value) =
        play::Clientbound::decode(packet.0, &packet.1).unwrap()
    else {
        panic!()
    };
    let Some(Tag::List(messages)) = value
        .tag
        .get("front_text")
        .and_then(|text| text.get("messages"))
    else {
        panic!("{:?}", value.tag)
    };
    assert!(
        messages[0] == Tag::String("First".into())
            || messages[0].get("text") == Some(&Tag::String("First".into())),
        "{:?}",
        messages[0]
    );
}
#[test]
fn sparse_combined_registry_ids_survive_expansion() {
    let registry = Tag::compound([(
        "minecraft:dimension_type",
        Tag::compound([
            ("type", Tag::String("minecraft:dimension_type".into())),
            (
                "value",
                Tag::List(vec![
                    Tag::compound([
                        ("id", Tag::Int(2)),
                        ("name", Tag::String("test:two".into())),
                        ("element", Tag::compound([("height", Tag::Int(256))])),
                    ]),
                    Tag::compound([
                        ("id", Tag::Int(5)),
                        ("name", Tag::String("test:five".into())),
                        ("element", Tag::compound([("height", Tag::Int(384))])),
                    ]),
                ]),
            ),
        ]),
    )]);
    let mut w = Writer::new();
    w.nbt(&registry);
    let packets = horizon_protocol::vfp_configuration_1203::clientbound(5, &w.data).unwrap();
    let data = packets
        .into_iter()
        .find_map(
            |(id, data)| match cfg::Clientbound::decode(id, &data).unwrap() {
                cfg::Clientbound::RegistryData(data)
                    if data.registry == "minecraft:dimension_type" =>
                {
                    Some(data)
                }
                _ => None,
            },
        )
        .unwrap();
    assert_eq!(data.entries.len(), 6);
    assert_eq!(data.entries[2].id, "test:two");
    assert_eq!(data.entries[5].id, "test:five");
    assert_eq!(
        data.entries
            .iter()
            .map(|entry| &entry.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        6
    );
}
#[test]
fn old_dismount_is_bundled_and_synthetic_self_teleports_are_acknowledged_locally() {
    use horizon_protocol::{vfp_tables_1193 as old, vfp_tables_1194 as next};
    let mut stage = horizon_protocol::vfp_transition_1193::Transition::default();
    stage
        .clientbound(
            wire::packet_id(old::CLIENTBOUND, "LOGIN").unwrap(),
            &7i32.to_be_bytes(),
        )
        .unwrap();
    let mut w = Writer::new();
    w.varint(40);
    w.varint(1);
    w.varint(7);
    stage
        .clientbound(
            wire::packet_id(old::CLIENTBOUND, "SET_PASSENGERS").unwrap(),
            &w.data,
        )
        .unwrap();
    let mut w = Writer::new();
    for _ in 0..3 {
        w.f64(1.0);
    }
    w.f32(2.0);
    w.f32(3.0);
    w.u8(0);
    w.varint(55);
    w.bool(true);
    let packets = stage
        .clientbound(
            wire::packet_id(old::CLIENTBOUND, "PLAYER_POSITION").unwrap(),
            &w.data,
        )
        .unwrap();
    assert_eq!(
        packets
            .iter()
            .map(|(id, _)| next::CLIENTBOUND[*id as usize])
            .collect::<Vec<_>>(),
        [
            "BUNDLE_DELIMITER",
            "SET_PASSENGERS",
            "PLAYER_POSITION",
            "BUNDLE_DELIMITER"
        ]
    );
    let mut w = Writer::new();
    w.varint(7);
    for _ in 0..3 {
        w.f64(1.0);
    }
    w.i8(-128);
    w.i8(32);
    w.bool(true);
    let packet = stage
        .clientbound(
            wire::packet_id(old::CLIENTBOUND, "TELEPORT_ENTITY").unwrap(),
            &w.data,
        )
        .unwrap()
        .remove(0);
    let mut r = Reader::new(&packet.1);
    r.bytes(24).unwrap();
    assert_eq!(r.f32().unwrap(), -180.0);
    assert_eq!(r.f32().unwrap(), 45.0);
    r.u8().unwrap();
    let id = r.varint().unwrap();
    assert!(id < 0);
    let mut w = Writer::new();
    w.varint(id);
    assert!(
        stage
            .serverbound(vec![(
                wire::packet_id(next::SERVERBOUND, "ACCEPT_TELEPORTATION").unwrap(),
                w.data
            )])
            .unwrap()
            .is_empty()
    );
}
#[test]
fn pre_1205_login_requests_gain_the_authentication_flag() {
    let mut session = Session::for_release_fixture("1.20.3").unwrap();
    let mut w = Writer::new();
    w.string("");
    w.byte_array(&[1, 2, 3]);
    w.byte_array(&[4, 5]);
    let packet = session.ingest(1, &w.data).unwrap().unwrap();
    let horizon_protocol::packets::login::Clientbound::Hello(hello) =
        horizon_protocol::packets::login::Clientbound::decode(packet.0, &packet.1).unwrap()
    else {
        panic!()
    };
    assert!(hello.should_authenticate);
    assert_eq!(hello.public_key, [1, 2, 3]);
    assert_eq!(hello.challenge, [4, 5]);
}

#[test]
fn decorated_pot_special_recipes_consume_their_category_in_both_legacy_formats() {
    fn check<M: Maps>() {
        let mut w = Writer::new();
        w.varint(2);
        for (name, kind) in [
            ("test:pot", "crafting_decorated_pot"),
            ("test:dye", "crafting_special_armordye"),
        ] {
            if M::NATIVE_ITEMS {
                w.string(&format!("minecraft:{kind}"));
            }
            w.string(name);
            if !M::NATIVE_ITEMS {
                w.varint(
                    M::RECIPE_IDENTIFIERS
                        .iter()
                        .position(|value| *value == kind)
                        .unwrap() as i32,
                );
            }
            w.varint(2);
        }
        let mut recipes = horizon_protocol::vfp_recipes_121::Recipes::default();
        let packet = recipes
            .update::<M>(&w.data, &Registries::default())
            .unwrap();
        assert_eq!(
            packet.0,
            wire::packet_id(native::NEW_CLIENTBOUND, "UPDATE_RECIPES").unwrap()
        );
        let mut malformed = w.data.clone();
        malformed.push(0);
        assert!(
            recipes
                .update::<M>(&malformed, &Registries::default())
                .is_err()
        );
        w.data.pop();
        assert!(
            recipes
                .update::<M>(&w.data, &Registries::default())
                .is_err()
        );
    }
    check::<V120>();
    check::<V1202>();
    check::<V1203>();
    check::<horizon_protocol::vfp_mapping::V1205>();
    check::<horizon_protocol::vfp_mapping::V121>();
}

#[test]
fn inline_chat_holder_selects_chat_and_narration_independently() {
    use horizon_protocol::{packets::play::ChatTypeBound, vfp_chat};
    use horizon_world::nbt::Tag;
    let bound = ChatTypeBound {
        chat_type: vfp_chat::DIRECT,
        name: Tag::compound([(
            vfp_chat::ENVELOPE,
            Tag::compound([
                ("sender", Tag::String("Alice".into())),
                ("type", Tag::compound([
                    ("chat", Tag::String("visible".into())),
                    ("narration", Tag::String("spoken".into())),
                ])),
            ]),
        )]),
        target: None,
    };
    let (sender, chat) = vfp_chat::direct(&bound).unwrap();
    let (narrated_sender, narration) = vfp_chat::direct_decoration(&bound, true).unwrap();
    assert_eq!(sender, narrated_sender);
    assert_eq!(chat, &Tag::String("visible".into()));
    assert_eq!(narration, &Tag::String("spoken".into()));
    let native = ChatTypeBound { chat_type: 0, ..bound };
    assert!(vfp_chat::direct_decoration(&native, false).is_none());
    assert!(vfp_chat::direct_decoration(&native, true).is_none());
}
