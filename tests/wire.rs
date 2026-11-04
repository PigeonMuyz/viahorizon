use viahorizon_native::{vfp_core::*, vfp_tables as t, vfp_wire as w};
fn old(name: &str) -> i32 {
    w::packet_id(t::OLD_CLIENTBOUND, name).unwrap()
}
fn new_server(name: &str) -> i32 {
    w::packet_id(t::NEW_SERVERBOUND, name).unwrap()
}
fn var(v: i32) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(&mut out, v);
    out
}

#[test]
fn advisory_section_counts_preserve_the_entire_signed_short_domain() {
    use viahorizon_native::vfp_mapping::*;
    fn check<M: Maps>() {
        for non_air in [i16::MIN, -1, 0, 4096, 4097, 8192, i16::MAX] {
            let mut input = non_air.to_be_bytes().to_vec();
            if M::PROTOCOL >= 775 {
                input.extend_from_slice(&(-1i16).to_be_bytes());
            }
            // Two single-value palettes: air blocks and biome zero.
            input.extend_from_slice(&[0, 0]);
            if M::PROTOCOL < 770 {
                put_varint(&mut input, 0);
            }
            input.extend_from_slice(&[0, 0]);
            if M::PROTOCOL < 770 {
                put_varint(&mut input, 0);
            }
            let output = w::chunk_sections_in_dimension::<M>(&input, 64, Some(1))
                .unwrap_or_else(|e| panic!("{} count {non_air}: {e}", M::PROTOCOL));
            assert_eq!(&output[..2], &non_air.to_be_bytes());
            assert_eq!(
                &output[2..4],
                &if M::PROTOCOL >= 775 { -1i16 } else { 0 }.to_be_bytes()
            );
            assert_eq!(&output[4..], &[0, 0, 0, 0]);
            // Accepting advisory counts must not weaken payload validation.
            assert!(
                w::chunk_sections_in_dimension::<M>(&input[..input.len() - 1], 64, Some(1))
                    .is_err()
            );
            let mut malformed = input.clone();
            malformed.push(1);
            assert!(w::chunk_sections_in_dimension::<M>(&malformed, 64, Some(1)).is_err());
        }
    }
    check::<V1193>();
    check::<V1194>();
    check::<V120>();
    check::<V1202>();
    check::<V1203>();
    check::<V1205>();
    check::<V121>();
    check::<V1212>();
    check::<V1214>();
    check::<V1215>();
    check::<V1216>();
    check::<V1217>();
    check::<V1219>();
    check::<V12111>();
    check::<V261>();
    check::<V262>();
}

#[test]
fn vanilla_section_padding_is_bounded_by_the_current_dimension() {
    use viahorizon_native::vfp_mapping::V1193;
    let mut data = Vec::new();
    for _ in 0..24 {
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 38, 0]);
    }
    data.extend_from_slice(&[0; 23]);
    let converted = w::chunk_sections_in_dimension::<V1193>(&data, 64, Some(24)).unwrap();
    assert_eq!(converted.len(), 24 * 8);
    assert!(
        converted
            .as_chunks::<8>()
            .0
            .iter()
            .all(|section| *section == [0, 0, 0, 0, 0, 0, 0, 38])
    );
    assert!(w::chunk_sections_using::<V1193>(&data, 64).is_err());
    *data.last_mut().unwrap() = 1;
    assert!(w::chunk_sections_in_dimension::<V1193>(&data, 64, Some(24)).is_err());
    assert!(w::chunk_sections_in_dimension::<V1193>(&data[..7], 64, Some(1)).is_err());
    assert!(w::chunk_sections_in_dimension::<V1193>(&data, 64, Some(0)).is_err());
    assert!(w::chunk_sections_in_dimension::<V1193>(&data, 64, Some(257)).is_err());
}

#[test]
fn dimension_height_tracking_changes_with_world_selection_and_resets() {
    let mut registry = viahorizon_native::vfp_registry_access::Registries::default();
    registry
        .dimension_heights(vec![Some(384), Some(256), None])
        .unwrap();
    assert_eq!(registry.active_sections(), None);
    registry.select_dimension(0).unwrap();
    assert_eq!(registry.active_sections(), Some(24));
    assert!(registry.dimension_heights(vec![Some(383)]).is_err());
    assert_eq!(registry.active_sections(), Some(24));
    registry.select_dimension(1).unwrap();
    assert_eq!(registry.active_sections(), Some(16));
    registry.select_dimension(2).unwrap();
    assert_eq!(registry.active_sections(), None);
    assert!(registry.select_dimension(-1).is_err());
    registry.select_dimension(0).unwrap();
    registry.clear();
    assert_eq!(registry.active_sections(), None);
}

#[test]
fn every_pre_261_family_adds_fluid_counts_without_remapping_source_ids_twice() {
    use viahorizon_native::{vfp_mapping::*, vfp_tables_12111};
    fn check<M: Maps>() {
        let mut ranges = Cursor::new(vfp_tables_12111::FLUID_RANGES_261);
        let fluid_261 = ranges.varint().unwrap();
        let native_fluid = V261::BLOCKSTATES.map(fluid_261).unwrap();
        let fluid = M::BLOCKSTATES.inverse(native_fluid).unwrap();
        for (non_air, expected_fluid, indirect) in
            [(0i16, 0i16, false), (4096, 4096, false), (2048, 2048, true)]
        {
            let mut input = non_air.to_be_bytes().to_vec();
            if indirect {
                input.push(4);
                put_varint(&mut input, 2);
                put_varint(&mut input, 0);
                put_varint(&mut input, fluid);
                if M::PROTOCOL < 770 {
                    put_varint(&mut input, 256);
                }
                for _ in 0..256 {
                    input.extend_from_slice(&0x1010_1010_1010_1010u64.to_be_bytes());
                }
            } else {
                input.push(0);
                put_varint(&mut input, if non_air == 0 { 0 } else { fluid });
                if M::PROTOCOL < 770 {
                    put_varint(&mut input, 0);
                }
            }
            input.push(0);
            put_varint(&mut input, 0);
            if M::PROTOCOL < 770 {
                put_varint(&mut input, 0);
            }
            let output = w::chunk_sections_with::<M>(&input)
                .unwrap_or_else(|e| panic!("{}: {e}", M::PROTOCOL));
            let mut reader = Cursor::new(&output);
            assert_eq!(reader.take(2).unwrap(), non_air.to_be_bytes());
            assert_eq!(
                reader.take(2).unwrap(),
                expected_fluid.to_be_bytes(),
                "{}",
                M::PROTOCOL
            );
            assert_eq!(reader.take(1).unwrap(), &[if indirect { 4 } else { 0 }]);
            if indirect {
                assert_eq!(reader.varint().unwrap(), 2);
                assert_eq!(reader.varint().unwrap(), 0);
            }
            assert_eq!(
                reader.varint().unwrap(),
                if non_air == 0 { 0 } else { native_fluid }
            );
            if indirect {
                reader.take(256 * 8).unwrap();
            }
            assert_eq!(reader.take(1).unwrap(), &[0]);
            assert_eq!(reader.varint().unwrap(), 0);
            reader.end().unwrap();
        }
    }
    check::<V1193>();
    check::<V1194>();
    check::<V120>();
    check::<V1202>();
    check::<V1203>();
    check::<V1205>();
    check::<V121>();
    check::<V1212>();
    check::<V1214>();
    check::<V1215>();
    check::<V1216>();
    check::<V1217>();
    check::<V1219>();
    check::<V12111>();
}

#[test]
fn generated_registry_maps_preserve_every_old_identifier() {
    for map in [
        t::BLOCKS,
        t::BLOCKSTATES,
        t::ITEMS,
        t::ENTITIES,
        t::SOUNDS,
        t::PARTICLES,
        t::STATISTICS,
        t::RECIPE_SERIALIZERS,
    ] {
        for id in 0..map.size {
            let mapped = map.map(id).unwrap();
            assert!((0..map.mapped_size).contains(&mapped));
            assert_eq!(map.inverse(mapped).unwrap(), id);
        }
        assert!(map.map(-1).is_err());
        assert!(map.map(map.size).is_err());
    }
    assert_eq!(t::BLOCKSTATES.size, 32366);
    assert_eq!(t::BLOCKSTATES.mapped_size, 35723);
    assert_eq!(t::BLOCKSTATES.map(27).unwrap(), 28);
    assert!(t::DATA_COMPONENT_TYPE.map(40).is_err());
    assert!(t::BLOCKSTATES.inverse(27).is_err());
}
#[test]
fn packet_ids_come_from_enum_order_instead_of_stale_comments() {
    assert_eq!(
        w::packet_id(t::OLD_CONFIGURATION, "STORE_COOKIE").unwrap(),
        10
    );
    assert_eq!(
        w::packet_id(t::NEW_CONFIGURATION, "POST_EFFECTS").unwrap(),
        10
    );
    assert_eq!(
        w::packet_id(t::NEW_CONFIGURATION, "STORE_COOKIE").unwrap(),
        11
    );
    assert!(w::clientbound_262(999, &[]).is_err());
}
#[test]
fn block_updates_translate_states_and_keep_coordinates() {
    let mut payload = vec![0xaa; 8];
    payload.extend(var(27));
    let packets = w::clientbound_262(old("BLOCK_UPDATE"), &payload).unwrap();
    assert_eq!(
        packets[0].0,
        w::packet_id(t::NEW_CLIENTBOUND, "BLOCK_UPDATE").unwrap()
    );
    assert_eq!(&packets[0].1[..8], &[0xaa; 8]);
    assert_eq!(&packets[0].1[8..], var(28));
    payload.push(0);
    assert!(w::clientbound_262(old("BLOCK_UPDATE"), &payload).is_err());
}
#[test]
fn indirect_chunk_palettes_are_mapped_without_touching_local_indices() {
    let mut section = vec![0x10, 0, 0, 0, 4, 2];
    section.extend(var(0));
    section.extend(var(27));
    let mut words = vec![0u8; 2048];
    words[7] = 0x10;
    section.extend_from_slice(&words);
    section.extend([0, 2]); // Single-valued biome 2.
    let mapped = w::chunk_sections(&section).unwrap();
    assert_eq!(&mapped[0..7], &[0x10, 0, 0, 0, 4, 2, 0]);
    assert_eq!(mapped[7], 28);
    assert_eq!(&mapped[8..2056], &words);
    assert_eq!(&mapped[2056..], &[0, 2]);
}
#[test]
fn direct_global_palettes_repack_fifteen_bits_to_sixteen() {
    let values: Vec<i32> = (0..4096)
        .map(|i| if i % 3 == 0 { 32365 } else { i % 1000 })
        .collect();
    let mut section = vec![0x10, 0, 0, 0, 15];
    for group in values.chunks(4) {
        let mut word = 0u64;
        for (i, &v) in group.iter().enumerate() {
            word |= (v as u64) << (15 * i);
        }
        section.extend_from_slice(&word.to_be_bytes());
    }
    section.extend([0, 0]);
    let mapped = w::chunk_sections(&section).unwrap();
    assert_eq!(mapped[4], 16);
    for (index, &old) in values.iter().enumerate() {
        let word = u64::from_be_bytes(
            mapped[5 + index / 4 * 8..13 + index / 4 * 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            ((word >> ((index % 4) * 16)) & 65535) as i32,
            t::BLOCKSTATES.map(old).unwrap()
        );
    }
}
#[test]
fn malformed_palette_indices_and_truncated_storage_fail_closed() {
    let mut section = vec![0, 1, 0, 0, 4, 1, 0];
    section.extend_from_slice(&[0xff; 2048]);
    section.extend([0, 0]);
    assert!(w::chunk_sections(&section).is_err());
    for n in [0, 1, 4, 7, 100, 2047] {
        if n != 0 {
            assert!(w::chunk_sections(&section[..n]).is_err());
        }
    }
}
#[test]
fn light_masks_reverse_each_long_and_trim_zero_bytes() {
    let mut old = vec![2];
    old.extend_from_slice(&0x80u64.to_be_bytes());
    old.extend_from_slice(&1u64.to_be_bytes());
    old.extend([0, 0, 0]);
    let mut out = Vec::new();
    w::light_masks(&mut Cursor::new(&old), &mut out).unwrap();
    assert_eq!(out, vec![9, 0x80, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
}
#[test]
fn entity_movement_preserves_signed_deltas_and_uses_three_tick_steps() {
    let mut old = var(300);
    old.extend_from_slice(
        &[-1i16, 32, -4096]
            .iter()
            .flat_map(|n| n.to_be_bytes())
            .collect::<Vec<_>>(),
    );
    old.extend([0x80, 0x40, 1]);
    let p = w::clientbound_262(self::old("MOVE_ENTITY_POS_ROT"), &old)
        .unwrap()
        .remove(0)
        .1;
    let mut r = Cursor::new(&p);
    assert_eq!(r.varint().unwrap(), 300);
    assert_eq!(r.varint().unwrap(), 3);
    assert_eq!(r.varint().unwrap(), 3);
    assert_eq!(r.take(8).unwrap(), &old[2..10]);
    r.end().unwrap();
}
#[test]
fn teleport_acknowledgement_emits_the_required_position_after_the_ack() {
    let mut input = var(123);
    input.extend_from_slice(&[0x42; 32]);
    let packets = w::serverbound_262(new_server("ACCEPT_TELEPORTATION"), &input).unwrap();
    assert_eq!(packets.len(), 2);
    assert_eq!(packets[0].1, var(123));
    assert_eq!(
        packets[1].0,
        w::packet_id(t::OLD_SERVERBOUND, "MOVE_PLAYER_POS_ROT").unwrap()
    );
    assert_eq!(&packets[1].1[..32], &input[1..]);
    assert_eq!(packets[1].1[32], 0);
}
#[test]
fn sign_slot_moves_before_the_lines_in_old_wire_format() {
    let mut input = vec![0; 8];
    for line in ["front", "🌍", "", "last"] {
        put_string(&mut input, line);
    }
    input.push(0);
    let out = w::serverbound_262(new_server("SIGN_UPDATE"), &input)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(out[8], 0);
    assert_eq!(&out[9..], &input[8..input.len() - 1]);
    *input.last_mut().unwrap() = 2;
    assert!(w::serverbound_262(new_server("SIGN_UPDATE"), &input).is_err());
}
#[test]
fn mining_direction_packets_are_dropped_and_other_actions_shift() {
    let mut p = vec![1];
    p.extend_from_slice(&[0; 9]);
    p.push(10);
    assert!(
        w::serverbound_262(new_server("PLAYER_ACTION"), &p)
            .unwrap()
            .is_empty()
    );
    p[0] = 2;
    assert_eq!(
        w::serverbound_262(new_server("PLAYER_ACTION"), &p).unwrap()[0].1[0],
        1
    );
    assert_eq!(
        w::serverbound_262(new_server("PUNCH"), &[]).unwrap()[0],
        (w::packet_id(t::OLD_SERVERBOUND, "SWING").unwrap(), vec![0])
    );
}
#[test]
fn potion_events_emit_sound_separately_without_reordering_particles() {
    let mut p = 2002i32.to_be_bytes().to_vec();
    p.extend([0; 8]);
    p.extend_from_slice(&42i32.to_be_bytes());
    p.push(0);
    let packets = w::clientbound_262(old("LEVEL_EVENT"), &p).unwrap();
    assert_eq!(packets.len(), 2);
    assert_eq!(packets[0].1, p);
    assert_eq!(&packets[1].1[..4], &1053i32.to_be_bytes());
    assert_eq!(&packets[1].1[12..16], &0i32.to_be_bytes());
}
#[test]
fn incomplete_packet_layouts_remain_explicit_errors() {
    assert!(
        w::clientbound_262(old("CONTAINER_SET_CONTENT"), &[0])
            .unwrap_err()
            .contains("payload converter")
    );
    assert!(
        w::serverbound_262(new_server("CONTAINER_CLICK"), &[0])
            .unwrap_err()
            .contains("payload converter")
    );
}
#[test]
fn previous_release_maps_compose_into_the_native_registry_domains() {
    use viahorizon_native::{vfp_tables as current, vfp_tables_261 as previous};
    for (old, next) in [
        (previous::BLOCKS, current::BLOCKS),
        (previous::BLOCKSTATES, current::BLOCKSTATES),
        (previous::ITEMS, current::ITEMS),
        (previous::ENTITIES, current::ENTITIES),
        (previous::SOUNDS, current::SOUNDS),
        (previous::PARTICLES, current::PARTICLES),
    ] {
        assert_eq!(old.mapped_size, next.size);
        for id in 0..old.size {
            let middle = old.map(id).unwrap();
            let native = next.map(middle).unwrap();
            assert!((0..next.mapped_size).contains(&native));
            assert_eq!(old.inverse(next.inverse(native).unwrap()).unwrap(), id);
        }
    }
}
