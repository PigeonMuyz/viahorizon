// SPDX-License-Identifier: GPL-3.0-or-later
use horizon_protocol::{
    buf::{Codec, Reader, Writer},
    packets::{configuration as cfg, play},
    vfp_core as core, vfp_items as items, vfp_tables as t, vfp_wire as wire,
};
use horizon_world::nbt::Tag;
fn old(name: &str) -> i32 {
    wire::packet_id(t::OLD_CLIENTBOUND, name).unwrap()
}
fn var(body: &mut Vec<u8>, v: i32) {
    core::put_varint(body, v);
}

#[test]
fn synthesized_registries_have_the_expected_stable_ids() {
    let packets = horizon_protocol::vfp_configuration::missing_registries_262().unwrap();
    assert_eq!(packets.len(), 2);
    let cfg::Clientbound::RegistryData(pots) =
        cfg::Clientbound::decode(packets[0].0, &packets[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(pots.entries.len(), 23);
    assert_eq!(pots.entries[0].id, "minecraft:angler");
    assert_eq!(pots.entries[22].id, "minecraft:snort");
    let cfg::Clientbound::RegistryData(transformers) =
        cfg::Clientbound::decode(packets[1].0, &packets[1].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        transformers
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["minecraft:axe", "minecraft:hoe", "minecraft:shovel"]
    );
    assert!(matches!(transformers.entries[0].data,Some(Tag::List(ref list)) if !list.is_empty()));
}
#[test]
fn enchantment_requirements_convert_integer_providers_without_changing_effects() {
    let requirement = Tag::compound([
        ("condition", Tag::String("minecraft:value_check".into())),
        ("value", Tag::Float(1.5f32.to_bits())),
        (
            "range",
            Tag::compound([
                ("min", Tag::Float((-0.5f32).to_bits())),
                ("max", Tag::Float(3.4f32.to_bits())),
            ]),
        ),
    ]);
    let mut enchantment = Tag::compound([(
        "effects",
        Tag::compound([(
            "minecraft:damage",
            Tag::List(vec![Tag::compound([
                ("requirements", requirement),
                (
                    "effect",
                    Tag::compound([
                        ("type", Tag::String("minecraft:add".into())),
                        ("value", Tag::Int(3)),
                    ]),
                ),
            ])]),
        )]),
    )]);
    horizon_protocol::vfp_registry::upgrade_registry("minecraft:enchantment", &mut enchantment)
        .unwrap();
    let Tag::List(effects) = enchantment
        .get("effects")
        .unwrap()
        .get("minecraft:damage")
        .unwrap()
    else {
        panic!()
    };
    let req = effects[0].get("requirements").unwrap();
    assert_eq!(
        req.get("type"),
        Some(&Tag::String("minecraft:int_value_check".into()))
    );
    assert_eq!(req.get("value"), Some(&Tag::Int(2)));
    assert_eq!(req.get("test").unwrap().get("min"), Some(&Tag::Int(0)));
    assert_eq!(req.get("test").unwrap().get("max"), Some(&Tag::Int(3)));
    assert!(req.get("range").is_none());
    assert_eq!(
        effects[0].get("effect").unwrap().get("value"),
        Some(&Tag::Int(3))
    );
}
#[test]
fn number_provider_rounding_matches_java_float_rounding_and_promotes_scores() {
    use horizon_protocol::vfp_registry as r;
    for (value, expected) in [
        (-1.5, -1),
        (-0.5, 0),
        (0.5, 1),
        (f32::NAN, 0),
        (f32::INFINITY, i32::MAX),
        (f32::NEG_INFINITY, i32::MIN),
    ] {
        assert_eq!(
            r::int_provider(Tag::Float(value.to_bits())).unwrap(),
            Tag::Int(expected)
        );
    }
    let score = r::float_provider(Tag::compound([
        ("type", Tag::String("minecraft:score".into())),
        ("scale", Tag::Float(2.0f32.to_bits())),
    ]))
    .unwrap();
    assert_eq!(
        score.get("type"),
        Some(&Tag::String("minecraft:mul".into()))
    );
    assert!(matches!(score.get("inputs"),Some(Tag::List(values)) if values.len()==2));
}
#[test]
fn block_state_providers_rewrite_fields_and_preserve_weighted_entries() {
    let mut provider = Tag::compound([
        (
            "type",
            Tag::String("minecraft:weighted_state_provider".into()),
        ),
        (
            "entries",
            Tag::List(vec![Tag::compound([
                ("weight", Tag::Int(3)),
                (
                    "data",
                    Tag::compound([
                        ("Name", Tag::String("minecraft:oak_log".into())),
                        (
                            "Properties",
                            Tag::compound([("axis", Tag::String("y".into()))]),
                        ),
                    ]),
                ),
            ])]),
        ),
    ]);
    horizon_protocol::vfp_registry::block_provider(&mut provider).unwrap();
    assert_eq!(
        provider.get("type"),
        Some(&Tag::String("minecraft:weighted".into()))
    );
    let Tag::List(entries) = provider.get("entries").unwrap() else {
        panic!()
    };
    assert_eq!(entries[0].get("weight"), Some(&Tag::Int(3)));
    let data = entries[0].get("data").unwrap();
    assert_eq!(
        data.get("id"),
        Some(&Tag::String("minecraft:oak_log".into()))
    );
    assert!(data.get("properties").is_some());
    assert!(data.get("Name").is_none());
}
#[test]
fn nested_bundle_templates_and_pot_decorations_round_trip_registry_ids() {
    for native in [52, 76] {
        let mut old = Writer::new();
        old.varint(1);
        old.varint(72);
        old.varint(1);
        old.varint(0);
        old.varint(t::DATA_COMPONENT_TYPE.inverse(native).unwrap());
        if native == 52 {
            old.varint(2);
            for id in [72, 85] {
                old.varint(id);
                old.varint(4);
                old.varint(0);
                old.varint(0);
            }
        } else {
            old.varint(4);
            for id in [72, 85, 170, 184] {
                old.varint(id);
            }
        }
        let mut mapped = Writer::new();
        items::stack_262(&mut Reader::new(&old.data), &mut mapped, true, false).unwrap();
        let stack = horizon_protocol::item::Slot::read(&mut Reader::new(&mapped.data))
            .unwrap()
            .unwrap();
        assert_eq!(stack.item, 73);
        if native == 52 {
            assert!(
                matches!(stack.component(52),Some(horizon_protocol::item::Component::Stacks(stacks)) if stacks.len()==2&&stacks[0].item==73&&stacks[1].item==87)
            );
        }
        let mut restored = Writer::new();
        items::stack_262(&mut Reader::new(&mapped.data), &mut restored, false, false).unwrap();
        assert_eq!(restored.data, old.data);
    }
}
#[test]
fn metadata_maps_items_block_states_and_particle_payloads() {
    let mut payload = Writer::new();
    payload.varint(99);
    payload.u8(8);
    payload.varint(7);
    payload.varint(1);
    payload.varint(72);
    payload.varint(0);
    payload.varint(0);
    payload.u8(9);
    payload.varint(14);
    payload.varint(27);
    payload.u8(10);
    payload.varint(15);
    payload.varint(28);
    payload.u8(11);
    payload.varint(16);
    payload.varint(1);
    payload.varint(27);
    payload.u8(255);
    let converted = items::clientbound_262(old("SET_ENTITY_DATA"), &payload.data).unwrap();
    let play::Clientbound::SetEntityData(p) =
        play::Clientbound::decode(converted[0].0, &converted[0].1).unwrap()
    else {
        panic!()
    };
    use horizon_protocol::entity_data::{ParticleData, Value};
    assert!(matches!(p.values[0].2,Value::Item(Some(ref stack)) if stack.item==73));
    assert_eq!(p.values[1].2, Value::Id(28));
    assert_eq!(p.values[2].2, Value::Id(29));
    assert!(
        matches!(p.values[3].2,Value::Particle(ref particle) if particle.data==Some(ParticleData::State(28)))
    );
    let mut bad = vec![1, 0, 43, 0, 255];
    assert!(items::clientbound_262(old("SET_ENTITY_DATA"), &bad).is_err());
    bad[2] = 99;
    assert!(items::clientbound_262(old("SET_ENTITY_DATA"), &bad).is_err());
}
#[test]
fn old_particle_layout_moves_the_particle_and_expands_speed_axes() {
    let mut payload = Writer::new();
    payload.bool(true);
    payload.bool(false);
    for v in [1.0, 2.0, 3.0] {
        payload.f64(v);
    }
    for v in [0.1, 0.2, 0.3] {
        payload.f32(v);
    }
    payload.f32(0.25);
    payload.i32(3);
    payload.varint(1);
    payload.varint(27);
    let converted = items::clientbound_262(old("LEVEL_PARTICLES"), &payload.data).unwrap();
    let play::Clientbound::LevelParticles(p) =
        play::Clientbound::decode(converted[0].0, &converted[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(p.count, 3);
    assert_eq!(p.max_speed, [0.25; 3]);
    assert_eq!(p.randomization, 0);
    assert_eq!(p.pos, [1.0, 2.0, 3.0]);
}
fn old_spawn(writer: &mut Writer) {
    writer.varint(0);
    writer.string("minecraft:overworld");
    writer.i64(123);
    writer.i8(1);
    writer.i8(-1);
    writer.bool(false);
    writer.bool(true);
    writer.bool(false);
    writer.varint(0);
    writer.varint(63);
}
#[test]
fn login_respawn_and_arm_animation_decode_as_native_packets() {
    let mut payload = Writer::new();
    payload.i32(-3);
    payload.bool(false);
    payload.varint(1);
    payload.string("minecraft:overworld");
    for v in [10, 8, 6] {
        payload.varint(v);
    }
    for b in [false, true, false] {
        payload.bool(b);
    }
    old_spawn(&mut payload);
    payload.bool(false);
    payload.bool(false);
    let packets = items::clientbound_262(old("LOGIN"), &payload.data).unwrap();
    let play::Clientbound::Login(login) =
        play::Clientbound::decode(packets[0].0, &packets[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(login.spawn.game_type, 1);
    assert_eq!(login.spawn.previous_game_type, None);
    assert_eq!(login.player_id, -3);
    let mut respawn = Writer::new();
    old_spawn(&mut respawn);
    respawn.i8(2);
    let p = items::clientbound_262(old("RESPAWN"), &respawn.data).unwrap();
    assert!(matches!(
        play::Clientbound::decode(p[0].0, &p[0].1).unwrap(),
        play::Clientbound::Respawn(_)
    ));
    for action in [0, 3] {
        let p = items::clientbound_262(old("ANIMATE"), &[1, action]).unwrap();
        let play::Clientbound::SwingAnimation(s) =
            play::Clientbound::decode(p[0].0, &p[0].1).unwrap()
        else {
            panic!()
        };
        assert_eq!(s.hand, if action == 0 { 0 } else { 1 });
        assert_eq!(s.kind, 1);
        assert_eq!(s.duration, 6);
    }
}

#[test]
fn commands_map_parser_ids_and_preserve_properties_and_references() {
    let mut w = Writer::new();
    w.varint(3);
    w.u8(0);
    w.varint(2);
    w.varint(1);
    w.varint(2);
    for (name, parser) in [("dialog", 55), ("uuid", 56)] {
        w.u8(2 | 4 | 16);
        w.varint(0);
        w.string(name);
        w.varint(parser);
        w.string("minecraft:ask_server");
    }
    w.varint(0);
    let p = items::clientbound_262(old("COMMANDS"), &w.data).unwrap();
    let play::Clientbound::Commands(tree) = play::Clientbound::decode(p[0].0, &p[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(tree.nodes[0].children, vec![1, 2]);
    assert_eq!(tree.nodes[1].parser, Some((58, Vec::new())));
    assert_eq!(tree.nodes[2].parser, Some((61, Vec::new())));
    let end = w.data.len() - 1;
    w.data[end] = 3;
    assert!(items::clientbound_262(old("COMMANDS"), &w.data).is_err());
}

#[test]
fn recipe_displays_map_nested_tags_items_and_ingredient_sets() {
    use horizon_protocol::packets::recipes::{HolderSet, RecipeDisplay, SlotDisplay};
    let mut w = Writer::new();
    w.varint(1); // Entry count.
    w.varint(4); // Display id.
    w.varint(0); // Shapeless.
    w.varint(1); // One ingredient.
    w.varint(6); // Old tag slot has only a name.
    w.string("minecraft:planks");
    w.varint(4);
    w.varint(72); // Result item.
    w.varint(0); // Empty station.
    w.varint(0);
    w.varint(0); // Group/category.
    w.bool(true);
    w.varint(1);
    w.varint(2);
    w.varint(72); // Holder set of one item.
    w.u8(3);
    w.bool(true);
    let p = items::clientbound_262(old("RECIPE_BOOK_ADD"), &w.data).unwrap();
    let play::Clientbound::RecipeBookAdd(book) =
        play::Clientbound::decode(p[0].0, &p[0].1).unwrap()
    else {
        panic!()
    };
    let RecipeDisplay::Shapeless {
        ingredients,
        result,
        ..
    } = &book.entries[0].display
    else {
        panic!()
    };
    assert_eq!(
        ingredients,
        &vec![SlotDisplay::Tag(HolderSet::Tag("minecraft:planks".into()))]
    );
    assert_eq!(result, &SlotDisplay::Item(73));
    assert_eq!(
        book.entries[0].requirements,
        Some(vec![HolderSet::Items(vec![73])])
    );
    assert!(book.replace);
    w.data.push(0);
    assert!(items::clientbound_262(old("RECIPE_BOOK_ADD"), &w.data).is_err());
}

#[test]
fn advancements_move_coordinates_outside_display_and_map_the_icon() {
    let mut w = Writer::new();
    w.bool(true);
    w.varint(2);
    w.string("test:visible");
    w.bool(false);
    w.bool(true);
    w.nbt(&Tag::String("title".into()));
    w.nbt(&Tag::String("description".into()));
    w.varint(72);
    w.varint(1);
    w.varint(0);
    w.varint(0);
    w.varint(0);
    w.i32(2);
    w.f32(3.5);
    w.f32(-2.0);
    w.varint(1);
    w.varint(1);
    w.string("criterion");
    w.bool(false);
    w.string("test:hidden");
    w.bool(true);
    w.string("test:visible");
    w.bool(false);
    w.varint(0);
    w.bool(false);
    w.varint(0);
    w.varint(0);
    w.bool(true);
    let p = items::clientbound_262(old("UPDATE_ADVANCEMENTS"), &w.data).unwrap();
    let play::Clientbound::UpdateAdvancements(a) =
        play::Clientbound::decode(p[0].0, &p[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!((a.added[0].x, a.added[0].y), (3.5, -2.0));
    assert_eq!(a.added[0].display.as_ref().unwrap().icon.item, 73);
    assert_eq!((a.added[1].x, a.added[1].y), (0.0, 0.0));
    assert_eq!(a.added[1].parent.as_deref(), Some("test:visible"));
}

fn component_stack(native_kind: i32, value: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(1);
    w.varint(72);
    w.varint(1);
    w.varint(0);
    w.varint(t::DATA_COMPONENT_TYPE.inverse(native_kind).unwrap());
    w.bytes(value);
    w.data
}
fn convert_stack(bytes: &[u8], forward: bool) -> Vec<u8> {
    let mut r = Reader::new(bytes);
    let mut w = Writer::new();
    items::stack_262(&mut r, &mut w, forward, false).unwrap();
    assert_eq!(r.remaining(), 0);
    w.data
}
#[test]
fn composite_item_values_round_trip_sounds_effects_tools_and_trim_holders() {
    let sound_id = (0..t::SOUNDS.size)
        .find(|&id| t::SOUNDS.map(id).unwrap() != id)
        .unwrap();
    let mut consumable = Writer::new();
    consumable.f32(1.6);
    consumable.varint(1);
    consumable.varint(sound_id + 1);
    consumable.bool(true);
    consumable.varint(2);
    consumable.varint(3);
    consumable.f32(16.0);
    consumable.varint(4);
    consumable.varint(sound_id + 1);
    let mut tool = Writer::new();
    tool.varint(1);
    tool.varint(2);
    tool.varint(10);
    tool.bool(true);
    tool.f32(8.0);
    tool.bool(true);
    tool.bool(true);
    tool.f32(1.0);
    tool.varint(1);
    tool.bool(false);
    let mut instrument = Writer::new();
    instrument.varint(0);
    instrument.varint(sound_id + 1);
    instrument.f32(7.0);
    instrument.f32(256.0);
    instrument.nbt(&Tag::String("horn".into()));
    let mut trim = Writer::new();
    trim.varint(0);
    trim.string("gold");
    trim.varint(0);
    trim.nbt(&Tag::String("Gold".into()));
    trim.varint(1);
    for (kind, value) in [
        (24, consumable.data),
        (28, tool.data),
        (63, instrument.data),
        (58, trim.data),
    ] {
        let original = component_stack(kind, &value);
        let converted = convert_stack(&original, true);
        let mut r = Reader::new(&converted);
        horizon_protocol::item::Slot::read(&mut r).unwrap();
        assert_eq!(r.remaining(), 0);
        assert_eq!(
            convert_stack(&converted, false),
            original,
            "component {kind}"
        );
        for len in 0..original.len() {
            assert!(
                items::stack_262(
                    &mut Reader::new(&original[..len]),
                    &mut Writer::new(),
                    true,
                    false
                )
                .is_err(),
                "component {kind} prefix {len}"
            );
        }
    }
}

#[test]
fn swing_components_expand_merge_and_preserve_explicit_removals() {
    let original = vec![1, 72, 1, 0, 40, 1, 6];
    let converted = convert_stack(&original, true);
    assert_eq!(converted, vec![1, 73, 2, 0, 40, 1, 6, 41, 1, 6]);
    assert_eq!(convert_stack(&converted, false), original);
    let removed = vec![1, 72, 0, 1, 40];
    assert_eq!(
        convert_stack(&convert_stack(&removed, true), false),
        removed
    );
    let mut incompatible = converted;
    *incompatible.last_mut().unwrap() = 7;
    assert!(
        items::stack_262(
            &mut Reader::new(&incompatible),
            &mut Writer::new(),
            false,
            false
        )
        .is_err()
    );
}

#[test]
fn stable_book_values_keep_filtered_text_bytes_in_both_directions() {
    let mut value = Writer::new();
    value.varint(1);
    value.string("raw page");
    value.bool(true);
    value.string("filtered page");
    let original = component_stack(56, &value.data);
    assert_eq!(
        convert_stack(&convert_stack(&original, true), false),
        original
    );
}

#[test]
fn repairable_holder_sets_map_item_ids_in_both_directions() {
    let original = component_stack(33, &[2, 72]);
    assert_eq!(convert_stack(&original, true), vec![1, 73, 1, 0, 33, 2, 73]);
    assert_eq!(
        convert_stack(&convert_stack(&original, true), false),
        original
    );
}

#[test]
fn brewing_menu_emits_default_timing_and_fuel_data() {
    let mut w = Writer::new();
    w.varint(4);
    w.varint(11);
    w.nbt(&Tag::String("Brewing".into()));
    let packets = items::clientbound_262(old("OPEN_SCREEN"), &w.data).unwrap();
    assert_eq!(packets.len(), 3);
    for (packet, slot, value) in [(&packets[1], 2, 400), (&packets[2], 3, 20)] {
        let play::Clientbound::ContainerSetData(p) =
            play::Clientbound::decode(packet.0, &packet.1).unwrap()
        else {
            panic!()
        };
        assert_eq!((p.container, p.id, p.value), (4, slot, value));
    }
    let packets = items::clientbound_262(old("UPDATE_RECIPES"), &[0, 0]).unwrap();
    let mut r = Reader::new(&packets[0].1);
    assert_eq!(r.varint().unwrap(), 2);
    for (name, expected) in [
        ("minecraft:brewing_input", t::BREWING_INPUTS),
        ("minecraft:brewing_reagent", t::BREWING_REAGENTS),
    ] {
        assert_eq!(r.string(32767).unwrap(), name);
        assert_eq!(r.varint().unwrap() as usize, expected.len());
        for &id in expected {
            assert_eq!(r.varint().unwrap(), id);
        }
    }
    assert_eq!(r.varint().unwrap(), 0);
    assert_eq!(r.remaining(), 0);
}

#[test]
fn fixture_session_delivers_player_position_and_abilities_after_world_entry() {
    let mut session = fixture_session();
    let expected = play::Clientbound::PlayerPosition(play::PlayerPosition {
        teleport: 3,
        position: [10.0, 64.0, -4.0],
        delta: [0.0; 3],
        yaw: 90.0,
        pitch: 0.0,
        relatives: 0,
    });
    let encoded = expected.encode();
    let mut r = Reader::new(&encoded);
    r.varint().unwrap();
    let (id, data) = session
        .clientbound(old("PLAYER_POSITION"), r.rest())
        .unwrap();
    assert_eq!(play::Clientbound::decode(id, &data).unwrap(), expected);
    assert!(session.clientbound(old("PLAYER_POSITION"), &[0]).is_err());
}

#[test]
fn text_hover_items_drop_removed_component_schemas_and_keep_other_fields() {
    let mut tag = Tag::compound([
        ("text", Tag::String("hover".into())),
        (
            "hover_event",
            Tag::compound([
                ("action", Tag::String("show_item".into())),
                ("id", Tag::String("minecraft:map".into())),
                (
                    "components",
                    Tag::compound([
                        ("minecraft:map_color", Tag::Int(123)),
                        ("minecraft:damage", Tag::Int(2)),
                    ]),
                ),
            ]),
        ),
    ]);
    horizon_protocol::vfp_play::text(&mut tag).unwrap();
    let c = tag.get("hover_event").unwrap().get("components").unwrap();
    assert!(c.get("minecraft:map_color").is_none());
    assert_eq!(c.get("minecraft:damage"), Some(&Tag::Int(2)));
}

#[test]
fn chat_filters_use_263_byte_bitsets_and_translate_old_long_masks() {
    let mut w = Writer::new();
    play::FilterMask::write(&(2, vec![0x0102, 0x80]), &mut w);
    assert_eq!(w.data, vec![2, 9, 2, 1, 0, 0, 0, 0, 0, 0, 128]);
    assert_eq!(
        play::FilterMask::read(&mut Reader::new(&w.data)).unwrap(),
        (2, vec![0x0102, 0x80])
    );
    assert!(play::FilterMask::read(&mut Reader::new(&[3])).is_err());
    let mut old_chat = Writer::new();
    old_chat.varint(1);
    old_chat.uuid(2);
    old_chat.varint(3);
    old_chat.bool(false);
    old_chat.string("test message");
    old_chat.i64(4);
    old_chat.i64(5);
    old_chat.varint(0);
    old_chat.bool(false);
    old_chat.varint(2);
    old_chat.long_array(&[0x0102, 0x80]);
    old_chat.varint(1);
    old_chat.nbt(&Tag::String("Sender".into()));
    old_chat.bool(false);
    let packets = items::clientbound_262(old("PLAYER_CHAT"), &old_chat.data).unwrap();
    let play::Clientbound::PlayerChat(chat) =
        play::Clientbound::decode(packets[0].0, &packets[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(chat.filter, (2, vec![0x0102, 0x80]));
    assert_eq!(chat.content, "test message");
    assert_eq!(chat.sender, 2);
}

#[test]
fn merchant_costs_and_results_translate_independent_component_layouts() {
    let mut w = Writer::new();
    w.varint(4);
    w.varint(1);
    w.varint(72);
    w.varint(2);
    w.varint(1);
    w.varint(3);
    w.varint(7);
    w.bytes(&[1, 72, 1, 0, 40, 1, 6]);
    w.bool(false);
    w.bool(false);
    for value in [0, 12, 5, 0] {
        w.i32(value);
    }
    w.f32(0.05);
    w.i32(0);
    w.varint(2);
    w.varint(4);
    w.bool(true);
    w.bool(true);
    let packets = items::clientbound_262(old("MERCHANT_OFFERS"), &w.data).unwrap();
    let play::Clientbound::MerchantOffers(p) =
        play::Clientbound::decode(packets[0].0, &packets[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(p.offers[0].cost_a.item, 73);
    assert_eq!(p.offers[0].cost_a.count, 2);
    assert_eq!(p.offers[0].result.item, 73);
    assert!(p.offers[0].result.component(40).is_some());
    assert!(p.offers[0].result.component(41).is_some());
}

#[test]
fn explosion_maps_particle_states_and_adds_the_sound_flag() {
    let mut w = Writer::new();
    for v in [1.0, 2.0, 3.0] {
        w.f64(v);
    }
    w.f32(4.0);
    w.i32(5);
    w.bool(false);
    w.varint(1);
    w.varint(27); // Block particle and its old state.
    w.varint(1); // Sound holder.
    w.varint(0); // No debris particles.
    let packets = items::clientbound_262(old("EXPLODE"), &w.data).unwrap();
    let play::Clientbound::Explode(e) =
        play::Clientbound::decode(packets[0].0, &packets[0].1).unwrap()
    else {
        panic!()
    };
    assert!(e.play_sound);
    assert_eq!(e.block_count, 5);
    assert_eq!(e.particle.id, 1);
}

#[test]
fn incompatible_item_fields_survive_clientbound_and_serverbound_round_trips() {
    let map = component_stack(48, &[1]); // Also exercise an ordinary component first.
    assert_eq!(convert_stack(&convert_stack(&map, true), false), map);
    let mut color = Writer::new();
    color.varint(1);
    color.varint(72);
    color.varint(1);
    color.varint(0);
    color.varint(45);
    color.i32(0x123456);
    assert_eq!(
        convert_stack(&convert_stack(&color.data, true), false),
        color.data
    );
    let mut material = Writer::new();
    material.varint(0);
    material.string("gold");
    material.varint(1);
    material.string("gold");
    material.string("darker_gold");
    material.nbt(&Tag::String("Gold".into()));
    let original = component_stack(64, &material.data);
    assert_eq!(
        convert_stack(&convert_stack(&original, true), false),
        original
    );
}

#[test]
fn pot_block_entity_downgrade_preserves_sides_and_rejects_patched_templates() {
    let original = Tag::compound([(
        "sherds",
        Tag::List(
            (0..4)
                .map(|_| Tag::String("minecraft:brick".into()))
                .collect(),
        ),
    )]);
    let mut tag = original.clone();
    horizon_protocol::vfp_world::upgrade_block_entity(&mut tag).unwrap();
    horizon_protocol::vfp_world::downgrade_block_entity(&mut tag).unwrap();
    assert_eq!(tag, original);
    let mut invalid = Tag::compound([(
        "sherds",
        Tag::compound([(
            "back",
            Tag::compound([
                ("id", Tag::String("minecraft:brick".into())),
                ("count", Tag::Int(2)),
            ]),
        )]),
    )]);
    let saved = invalid.clone();
    assert!(horizon_protocol::vfp_world::downgrade_block_entity(&mut invalid).is_err());
    assert_eq!(invalid, saved);
}
fn fixture_session() -> horizon_protocol::vfp_session::Session {
    let mut session = horizon_protocol::vfp_session::Session::for_protocol_fixture().unwrap();
    let mut success = vec![0; 16];
    core::put_string(&mut success, "Horizon");
    success.push(0);
    success.extend([0; 16]);
    session.clientbound(2, &success).unwrap();
    assert_eq!(session.serverbound(&[3]).unwrap(), vec![vec![3]]);
    let first = session.clientbound(3, &[]).unwrap();
    assert_eq!(first.0, 7);
    assert!(session.serverbound(&[3]).is_err());
    assert_eq!(session.poll().unwrap().unwrap().0, 7);
    assert_eq!(session.poll().unwrap().unwrap(), (3, Vec::new()));
    assert_eq!(session.serverbound(&[3]).unwrap(), vec![vec![3]]);
    session
}

fn fixture_body(name: &str, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    var(
        &mut body,
        wire::packet_id(t::NEW_SERVERBOUND, name).unwrap(),
    );
    body.extend_from_slice(payload);
    body
}
fn mining_action(kind: i32, direction: u8) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(kind);
    w.i64(42);
    w.u8(direction);
    w.varint(0);
    w.data
}
#[test]
fn mining_effects_are_queued_only_while_destroying_and_reset_on_reconfiguration() {
    let mut session = fixture_session();
    session
        .serverbound(&fixture_body("PLAYER_ACTION", &mining_action(0, 2)))
        .unwrap();
    for tick in 0..=4 {
        session.serverbound(&fixture_body("PUNCH", &[])).unwrap();
        session
            .serverbound(&fixture_body("CLIENT_TICK_END", &[]))
            .unwrap();
        if tick == 0 {
            assert!(session.poll().is_none());
            continue;
        }
        let (id, data) = session.poll().unwrap().unwrap();
        let play::Clientbound::LevelEvent(event) = play::Clientbound::decode(id, &data).unwrap()
        else {
            panic!()
        };
        assert_eq!(event.kind, if tick == 4 { 2020 } else { 2019 });
    }
    session
        .clientbound(old("START_CONFIGURATION"), &[])
        .unwrap();
    session.serverbound(&fixture_body("PUNCH", &[])).unwrap();
    session
        .serverbound(&fixture_body("CLIENT_TICK_END", &[]))
        .unwrap();
    assert!(session.poll().is_none());
}

#[test]
fn cancelling_mining_discards_stale_progress_without_dropping_other_queued_events() {
    let mut session = fixture_session();
    let mut splash = Writer::new();
    splash.i32(2002);
    splash.i64(42);
    splash.i32(0);
    splash.bool(false);
    session
        .clientbound(old("LEVEL_EVENT"), &splash.data)
        .unwrap();
    session
        .serverbound(&fixture_body("PLAYER_ACTION", &mining_action(0, 2)))
        .unwrap();
    for _ in 0..2 {
        session.serverbound(&fixture_body("PUNCH", &[])).unwrap();
        session
            .serverbound(&fixture_body("CLIENT_TICK_END", &[]))
            .unwrap();
    }
    session
        .serverbound(&fixture_body("PLAYER_ACTION", &mining_action(2, 2)))
        .unwrap();
    let (id, data) = session.poll().unwrap().unwrap();
    let play::Clientbound::LevelEvent(p) = play::Clientbound::decode(id, &data).unwrap() else {
        panic!()
    };
    assert_eq!(p.kind, 1053);
    assert!(session.poll().is_none());
}

#[test]
fn auxiliary_command_and_report_packets_preserve_fields_and_reject_trailing_data() {
    let mut suggestions = Writer::new();
    suggestions.varint(4);
    suggestions.varint(1);
    suggestions.varint(2);
    suggestions.varint(1);
    suggestions.string("test");
    suggestions.bool(true);
    suggestions.nbt(&Tag::String("tooltip".into()));
    let mut report = Writer::new();
    report.varint(1);
    report.string("Server");
    report.string("fixture");
    let mut completions = Writer::new();
    completions.varint(2);
    completions.varint(1);
    completions.string("Horizon");
    for (name, mut data) in [
        ("COMMAND_SUGGESTIONS", suggestions.data),
        ("CUSTOM_REPORT_DETAILS", report.data),
        ("CUSTOM_CHAT_COMPLETIONS", completions.data),
    ] {
        let packets = items::clientbound_262(old(name), &data).unwrap();
        assert_eq!(packets[0].1, data);
        assert_eq!(
            packets[0].0,
            wire::packet_id(t::NEW_CLIENTBOUND, name).unwrap()
        );
        data.push(0);
        assert!(items::clientbound_262(old(name), &data).is_err());
    }
}

#[test]
fn auxiliary_query_and_suggestion_requests_validate_their_original_layouts() {
    let mut query = Writer::new();
    query.varint(7);
    query.i64(42);
    let mut suggestion = Writer::new();
    suggestion.varint(3);
    suggestion.string("/give @s sto");
    let mut seen = Writer::new();
    seen.varint(0);
    seen.string("minecraft:story/root");
    for (name, mut data) in [
        ("BLOCK_ENTITY_TAG_QUERY", query.data),
        ("COMMAND_SUGGESTION", suggestion.data),
        ("SEEN_ADVANCEMENTS", seen.data),
    ] {
        let id = wire::packet_id(t::NEW_SERVERBOUND, name).unwrap();
        let packets = items::serverbound_262(id, &data).unwrap();
        assert_eq!(packets[0].1, data);
        assert_eq!(
            packets[0].0,
            wire::packet_id(t::OLD_SERVERBOUND, name).unwrap()
        );
        data.push(0);
        assert!(items::serverbound_262(id, &data).is_err());
    }
}

#[test]
fn profiles_with_skin_overrides_decode_and_restore_the_complete_original_value() {
    let mut profile = Writer::new();
    profile.bool(true);
    profile.uuid(12);
    profile.string("Horizon");
    profile.varint(1);
    profile.string("textures");
    profile.string("skin-property");
    profile.bool(false);
    profile.bool(true);
    profile.string("minecraft:test_skin");
    profile.bool(false);
    profile.bool(false);
    profile.bool(true);
    profile.bool(true);
    let original = component_stack(72, &profile.data);
    let converted = convert_stack(&original, true);
    let stack = horizon_protocol::item::Slot::read(&mut Reader::new(&converted))
        .unwrap()
        .unwrap();
    let Some(horizon_protocol::item::Component::Profile(p)) = stack.component(72) else {
        panic!()
    };
    assert_eq!(p.name.as_deref(), Some("Horizon"));
    assert_eq!(p.id, Some(12));
    assert_eq!(p.properties[0].1, "skin-property");
    assert_eq!(convert_stack(&converted, false), original);
}

#[test]
fn generated_events_wake_a_split_reader_without_corrupting_partial_encrypted_frames() {
    use horizon_protocol::{
        connection::{Connection, vfp_bridge},
        crypto::Cfb8,
        frame,
    };
    use std::{
        io::Write,
        net::TcpListener,
        sync::{atomic::Ordering, mpsc},
        time::{Duration, Instant},
    };
    for encrypted in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (release, wait) = mpsc::channel();
        let (ready, ready_wait) = mpsc::channel();
        let mut body = Vec::new();
        var(&mut body, old("KEEP_ALIVE"));
        body.extend_from_slice(&42i64.to_be_bytes());
        let mut frame = frame::prefixed(
            &frame::contents(&body, if encrypted { Some(0) } else { None }).unwrap(),
        );
        let key = [0x5a; 16];
        if encrypted {
            Cfb8::new(&key, &key).encrypt(&mut frame);
        }
        let network_size = frame.len();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_nodelay(true).unwrap();
            socket.write_all(&frame[..2]).unwrap();
            ready.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            socket.write_all(&frame[2..]).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let mut client =
            Connection::connect("127.0.0.1", addr.port(), Duration::from_secs(2)).unwrap();
        client
            .reader
            .set_read_timeout(Some(Duration::from_millis(400)))
            .unwrap();
        if encrypted {
            client.set_compression(0);
            client.enable_encryption(&key);
        }
        vfp_bridge::install_protocol_fixture(&client.reader, fixture_session());
        let counters = client.reader.counters.clone();
        let (mut reader, mut writer) = client.split();
        let (events, results) = mpsc::channel();
        let reader_thread = std::thread::spawn(move || {
            events.send(reader.recv()).unwrap();
            events.send(reader.recv()).unwrap();
            let start = Instant::now();
            assert!(reader.recv().is_err());
            start.elapsed()
        });
        ready_wait.recv_timeout(Duration::from_secs(2)).unwrap();
        std::thread::sleep(Duration::from_millis(60)); // Enter the receive loop with an incomplete frame.
        writer
            .send(&fixture_body("PLAYER_ACTION", &mining_action(0, 2)))
            .unwrap();
        for tick in 0..2 {
            if tick == 1 {
                writer
                    .send(&fixture_body("PLAYER_ACTION", &mining_action(1, 5)))
                    .unwrap();
            }
            writer.send(&fixture_body("PUNCH", &[])).unwrap();
            writer.send(&fixture_body("CLIENT_TICK_END", &[])).unwrap();
        }
        let (id, data) = results
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        let play::Clientbound::LevelEvent(event) = play::Clientbound::decode(id, &data).unwrap()
        else {
            panic!()
        };
        assert_eq!(event.kind, 2019);
        assert_eq!(event.data, 5);
        release.send(()).unwrap();
        let (id, data) = results
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(
            id,
            wire::packet_id(t::NEW_CLIENTBOUND, "KEEP_ALIVE").unwrap()
        );
        assert_eq!(data, 42i64.to_be_bytes());
        let elapsed = reader_thread.join().unwrap();
        assert!(
            elapsed >= Duration::from_millis(200),
            "original socket timeout was not restored: {elapsed:?}"
        );
        assert_eq!(counters.received.load(Ordering::Relaxed), 1);
        assert_eq!(
            counters.bytes_in.load(Ordering::Relaxed),
            network_size as i64
        );
        release.send(()).unwrap();
        server.join().unwrap();
    }
}
#[test]
fn fixture_session_connects_configuration_gameplay_and_multi_packet_replies() {
    let mut session = fixture_session();
    let mut block = vec![0; 8];
    var(&mut block, 27);
    let translated = session.clientbound(old("BLOCK_UPDATE"), &block).unwrap();
    let play::Clientbound::BlockUpdate(p) =
        play::Clientbound::decode(translated.0, &translated.1).unwrap()
    else {
        panic!()
    };
    assert_eq!(p.state, 28);
    let mut teleport = Vec::new();
    var(
        &mut teleport,
        wire::packet_id(t::NEW_SERVERBOUND, "ACCEPT_TELEPORTATION").unwrap(),
    );
    var(&mut teleport, 5);
    teleport.extend([0; 32]);
    let replies = session.serverbound(&teleport).unwrap();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0], vec![0, 5]);
    let request = session
        .clientbound(old("START_CONFIGURATION"), &[])
        .unwrap();
    assert_eq!(
        request.0,
        wire::packet_id(t::NEW_CLIENTBOUND, "START_CONFIGURATION").unwrap()
    );
    let mut ack = Vec::new();
    var(
        &mut ack,
        wire::packet_id(t::NEW_SERVERBOUND, "CONFIGURATION_ACKNOWLEDGED").unwrap(),
    );
    assert_eq!(session.serverbound(&ack).unwrap().len(), 1);
    assert_eq!(session.clientbound(3, &[]).unwrap().0, 7);
}
#[test]
fn hashless_inventory_predictions_map_items_and_resynchronize_unverified_hashes() {
    use horizon_protocol::item::Hashed;
    let click = play::ContainerClick {
        container: 0,
        state: 1,
        slot: 2,
        button: 0,
        input: 0,
        changed: Vec::new(),
        carried: Some(Hashed {
            item: 73,
            count: 2,
            added: Vec::new(),
            removed: Vec::new(),
        }),
    };
    let encoded = play::Serverbound::ContainerClick(click.clone()).encode();
    let mut r = Reader::new(&encoded);
    let id = r.varint().unwrap();
    let converted = items::serverbound_262(id, r.rest()).unwrap();
    let play::Serverbound::ContainerClick(restored) =
        play::Serverbound::decode(id, &converted[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(restored.carried.unwrap().item, 72);
    let mut primitive = click.clone();
    primitive.carried.as_mut().unwrap().added.push((3, 123));
    let encoded = play::Serverbound::ContainerClick(primitive).encode();
    let mut r = Reader::new(&encoded);
    let id = r.varint().unwrap();
    let converted = items::serverbound_262(id, r.rest()).unwrap();
    let play::Serverbound::ContainerClick(p) =
        play::Serverbound::decode(id, &converted[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(p.carried.unwrap().added, vec![(3, 123)]);
    let mut hashed = click;
    hashed.carried.as_mut().unwrap().added.push((24, 123));
    let bad = play::Serverbound::ContainerClick(hashed).encode();
    let mut r = Reader::new(&bad);
    let id = r.varint().unwrap();
    let converted = items::serverbound_262(id, r.rest()).unwrap();
    assert_eq!(converted.len(), 1);
    let play::Serverbound::ContainerClick(restored) =
        play::Serverbound::decode(converted[0].0, &converted[0].1).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        (
            restored.container,
            restored.state,
            restored.slot,
            restored.button,
            restored.input
        ),
        (0, 1, 2, 0, 0)
    );
    assert!(restored.changed.is_empty());
    assert_eq!(
        restored.carried,
        Some(Hashed {
            item: 72,
            count: 2,
            added: Vec::new(),
            removed: Vec::new()
        })
    );
}

#[test]
fn item_backups_preserve_existing_custom_data_and_reject_forged_payloads() {
    let original_tag = Tag::compound([("test:original", Tag::Int(7))]);
    let mut w = Writer::new();
    w.varint(1);
    w.varint(72);
    w.varint(2);
    w.varint(0);
    w.varint(0);
    w.nbt(&original_tag);
    w.varint(45);
    w.i32(123);
    let converted = convert_stack(&w.data, true);
    assert_eq!(convert_stack(&converted, false), w.data);
    let mut r = Reader::new(&converted);
    let mut stack = horizon_protocol::item::Slot::read(&mut r).unwrap().unwrap();
    let horizon_protocol::item::Component::Nbt(tag) =
        &mut stack.added.iter_mut().find(|(id, _)| *id == 0).unwrap().1
    else {
        panic!()
    };
    let Tag::List(values) = tag
        .get_mut("viafabricplus:26_2_backup")
        .unwrap()
        .get_mut("values")
        .unwrap()
    else {
        panic!()
    };
    *values[0].get_mut("source").unwrap() = Tag::Int(3);
    stack.wire = Default::default();
    let mut forged = Writer::new();
    horizon_protocol::item::Slot::write(&Some(stack), &mut forged);
    assert!(
        items::stack_262(
            &mut Reader::new(&forged.data),
            &mut Writer::new(),
            false,
            false
        )
        .is_err()
    );
}

#[test]
fn malformed_registry_expressions_fail_without_mutating_input() {
    let score = Tag::compound([
        ("type", Tag::String("minecraft:score".into())),
        ("scale", Tag::String("bad".into())),
    ]);
    assert!(horizon_protocol::vfp_registry::int_provider(score).is_err());
    let mut predicate = Tag::compound([("condition", Tag::Int(1))]);
    let original = predicate.clone();
    assert!(horizon_protocol::vfp_registry::predicate(&mut predicate).is_err());
    assert_eq!(predicate, original);
}

#[test]
fn serverbound_menu_actions_keep_their_payload_and_validate_boundaries() {
    for packet in [
        play::Serverbound::ContainerClose(play::ContainerClose { container: 4 }),
        play::Serverbound::SelectTrade(play::SelectTrade { item: 2 }),
    ] {
        let encoded = packet.encode();
        let mut r = Reader::new(&encoded);
        let id = r.varint().unwrap();
        let payload = r.rest();
        let converted = items::serverbound_262(id, payload).unwrap();
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].1, payload);
        let mut bad = payload.to_vec();
        bad.push(0);
        assert!(items::serverbound_262(id, &bad).is_err());
    }
}
