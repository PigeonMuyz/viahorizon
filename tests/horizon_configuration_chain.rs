// SPDX-License-Identifier: GPL-3.0-or-later
use horizon_protocol::{
    buf::Reader, packets::configuration as cfg, vfp_configuration_261 as chain, vfp_tables as t,
    vfp_tables_261 as previous, vfp_wire as wire,
};
use horizon_world::nbt::Tag;
fn translate(name: &str, packet: cfg::Clientbound) -> cfg::Clientbound {
    let bytes = packet.encode();
    let mut r = Reader::new(&bytes);
    r.varint().unwrap();
    let (id, data) = chain::clientbound(
        wire::packet_id(t::OLD_CONFIGURATION, name).unwrap(),
        r.rest(),
    )
    .unwrap();
    cfg::Clientbound::decode(id, &data).unwrap()
}
#[test]
fn configuration_tags_compose_both_registry_maps_and_fill_missing_infiniburn_tags() {
    let id = (0..previous::BLOCKS.size)
        .find(|&id| previous::BLOCKS.map(id).unwrap() != id)
        .unwrap();
    let expected = t::BLOCKS.map(previous::BLOCKS.map(id).unwrap()).unwrap();
    let cfg::Clientbound::UpdateTags(update) = translate(
        "UPDATE_TAGS",
        cfg::Clientbound::UpdateTags(cfg::UpdateTags {
            registries: vec![cfg::RegistryTags {
                registry: "minecraft:block".into(),
                tags: vec![cfg::TagEntry {
                    name: "minecraft:infiniburn_nether".into(),
                    ids: vec![id],
                }],
            }],
        }),
    ) else {
        panic!()
    };
    assert_eq!(update.registries[0].tags.len(), 3);
    assert_eq!(update.registries[0].tags[0].ids, vec![expected]);
    assert_eq!(
        update.registries[0].tags[2].name,
        "minecraft:infiniburn_overworld"
    );
}
#[test]
fn enchantment_entity_predicates_are_upgraded_before_263_condition_rewriting() {
    let tag = Tag::compound([(
        "effects",
        Tag::List(vec![Tag::compound([(
            "requirements",
            Tag::compound([
                (
                    "condition",
                    Tag::String("minecraft:entity_properties".into()),
                ),
                (
                    "predicate",
                    Tag::compound([
                        ("type", Tag::String("minecraft:slime".into())),
                        (
                            "type_specific",
                            Tag::compound([
                                ("type", Tag::String("minecraft:slime".into())),
                                ("size", Tag::Int(2)),
                            ]),
                        ),
                        (
                            "vehicle",
                            Tag::compound([("type", Tag::String("minecraft:pig".into()))]),
                        ),
                    ]),
                ),
            ]),
        )])]),
    )]);
    let cfg::Clientbound::RegistryData(registry) = translate(
        "REGISTRY_DATA",
        cfg::Clientbound::RegistryData(cfg::RegistryData {
            registry: "minecraft:enchantment".into(),
            entries: vec![cfg::RegistryEntry {
                id: "test:fixture".into(),
                data: Some(tag),
            }],
        }),
    ) else {
        panic!()
    };
    let Tag::List(effects) = registry.entries[0]
        .data
        .as_ref()
        .unwrap()
        .get("effects")
        .unwrap()
    else {
        panic!()
    };
    let requirements = effects[0].get("requirements").unwrap();
    assert!(requirements.get("condition").is_none());
    assert_eq!(
        requirements.get("type"),
        Some(&Tag::String("minecraft:entity_properties".into()))
    );
    let predicate = requirements.get("predicate").unwrap();
    assert!(predicate.get("type").is_none());
    assert_eq!(
        predicate.get("type_specific/cube_mob").unwrap().get("size"),
        Some(&Tag::Int(2))
    );
    assert_eq!(
        predicate.get("vehicle").unwrap().get("entity_type"),
        Some(&Tag::String("minecraft:pig".into()))
    );
}
#[test]
fn jukebox_additions_preserve_existing_ids_and_are_not_duplicated() {
    let input = cfg::Clientbound::RegistryData(cfg::RegistryData {
        registry: "minecraft:jukebox_song".into(),
        entries: vec![cfg::RegistryEntry {
            id: "test:custom".into(),
            data: Some(Tag::compound([("test", Tag::Int(1))])),
        }],
    });
    let cfg::Clientbound::RegistryData(output) = translate("REGISTRY_DATA", input) else {
        panic!()
    };
    assert_eq!(output.entries.len(), 2);
    assert_eq!(output.entries[0].id, "test:custom");
    assert_eq!(output.entries[1].id, "minecraft:bounce");
    assert_eq!(
        output.entries[1]
            .data
            .as_ref()
            .unwrap()
            .get("comparator_output"),
        Some(&Tag::Int(10))
    );
    let cfg::Clientbound::RegistryData(repeated) =
        translate("REGISTRY_DATA", cfg::Clientbound::RegistryData(output))
    else {
        panic!()
    };
    assert_eq!(repeated.entries.len(), 2);
}
#[test]
fn invalid_entity_predicates_fail_without_partially_rewriting_the_input() {
    let mut tag = Tag::compound([
        (
            "condition",
            Tag::String("minecraft:entity_properties".into()),
        ),
        (
            "predicate",
            Tag::compound([
                ("type", Tag::String("minecraft:slime".into())),
                ("type_specific", Tag::Int(1)),
            ]),
        ),
    ]);
    let original = tag.clone();
    assert!(chain::upgrade_enchantment(&mut tag).is_err());
    assert_eq!(tag, original);
}

#[test]
fn empty_tag_packets_still_supply_the_required_block_tag_registry() {
    let cfg::Clientbound::UpdateTags(p) = translate(
        "UPDATE_TAGS",
        cfg::Clientbound::UpdateTags(cfg::UpdateTags {
            registries: Vec::new(),
        }),
    ) else {
        panic!()
    };
    assert_eq!(p.registries.len(), 1);
    assert_eq!(p.registries[0].registry, "minecraft:block");
    assert_eq!(p.registries[0].tags.len(), 3);
    assert!(p.registries[0].tags.iter().all(|v| v.ids.is_empty()));
}
