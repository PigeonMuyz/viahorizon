// SPDX-License-Identifier: GPL-3.0-or-later
// Expand the combined NBT registry packet into individual 1.20.5 packets.
// Sparse server ids are retained, with harmless named copies for missing ids.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    packets::configuration as cfg,
    vfp_registry_1203 as data, vfp_tables_1203 as old, vfp_tables_1205 as next,
    vfp_wire::{self as wire, Packet},
};
use horizon_world::nbt::{JavaString, Tag};
use std::collections::BTreeMap;
fn local(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}
fn string(tag: &Tag, key: &str) -> Option<String> {
    if let Some(Tag::String(value)) = tag.get(key) {
        Some(value.to_string_lossy())
    } else {
        None
    }
}
fn sound(tag: &mut Tag, key: &str) {
    if let Some(Tag::String(name)) = tag.get(key) {
        if !data::SOUND_NAMES.contains(&local(&name.to_string_lossy())) {
            let name = name.clone();
            if let Tag::Compound(values) = tag {
                values.insert(key.into(), Tag::compound([("sound_id", Tag::String(name))]));
            }
        }
    }
}
fn upgrade(name: &str, tag: &mut Tag) -> Result<()> {
    match local(name) {
        "dimension_type" => {
            if let Some(Tag::Compound(provider)) = tag.get_mut("monster_spawn_light_level") {
                if let Some(Tag::Compound(mut value)) = provider.remove(&JavaString::from("value"))
                {
                    for field in ["min_inclusive", "max_inclusive"] {
                        if let Some(value) = value.remove(&JavaString::from(field)) {
                            provider.insert(field.into(), value);
                        }
                    }
                }
            }
        }
        "worldgen/biome" => {
            if let Some(effects) = tag.get_mut("effects") {
                sound(effects, "ambient_sound");
                for key in ["mood_sound", "additions_sound", "music"] {
                    if let Some(value) = effects.get_mut(key) {
                        sound(value, "sound");
                    }
                }
                if let Some(Tag::Compound(options)) = effects
                    .get_mut("particle")
                    .and_then(|value| value.get_mut("options"))
                {
                    let kind = options
                        .get(&JavaString::from("type"))
                        .and_then(|tag| {
                            if let Tag::String(value) = tag {
                                Some(value.to_string_lossy())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    let fields: &[(&str, &str)] = match local(&kind) {
                        "block" | "block_marker" | "falling_dust" | "dust_pillar" => {
                            &[("value", "block_state")]
                        }
                        "item" => &[("value", "item")],
                        "dust_color_transition" => {
                            &[("fromColor", "from_color"), ("toColor", "to_color")]
                        }
                        "entity_effect" => &[("value", "color")],
                        _ => &[],
                    };
                    for (before, after) in fields {
                        if let Some(value) = options.remove(&JavaString::from(*before)) {
                            options.insert((*after).into(), value);
                        }
                    }
                }
            }
        }
        _ => (),
    }
    Ok(())
}
fn registry(name: &str, entries: Vec<cfg::RegistryEntry>) -> Result<Packet> {
    let bytes = cfg::Clientbound::RegistryData(cfg::RegistryData {
        registry: name.to_owned(),
        entries,
    })
    .encode();
    let mut r = Reader::new(&bytes);
    r.varint()?;
    Ok((
        wire::packet_id(next::CONFIGURATION_CLIENTBOUND, "REGISTRY_DATA").map_err(Error::new)?,
        r.rest().to_vec(),
    ))
}
fn damage_defaults() -> Result<Tag> {
    let bytes = data::DAMAGE_TYPES;
    let (root, used) = horizon_world::nbt::read_named(
        bytes,
        horizon_world::nbt::Limits {
            max_bytes: bytes.len(),
            max_nodes: 100000,
            max_depth: 64,
        },
    )
    .map_err(|error| Error::new(error.to_string()))?;
    if used != bytes.len() {
        return Err(Error::new("Trailing legacy damage registry"));
    }
    Ok(root.tag)
}
pub fn clientbound(id: i32, payload: &[u8]) -> Result<Vec<Packet>> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|id| old::CONFIGURATION_CLIENTBOUND.get(id))
        .copied()
        .ok_or_else(|| Error::new("Unknown 1.20.3 configuration packet"))?;
    if name != "REGISTRY_DATA" {
        return Ok(vec![(
            wire::packet_id(next::CONFIGURATION_CLIENTBOUND, name).map_err(Error::new)?,
            payload.to_vec(),
        )]);
    }
    let mut r = Reader::new(payload);
    let root = r.nbt()?;
    if r.remaining() != 0 {
        return Err(Error::new("Trailing combined registry NBT"));
    }
    let Tag::Compound(registries) = root else {
        return Err(Error::new("Combined registry NBT is not a compound"));
    };
    let mut packets = vec![(
        wire::packet_id(next::CONFIGURATION_CLIENTBOUND, "SELECT_KNOWN_PACKS")
            .map_err(Error::new)?,
        vec![0],
    )];
    for (key, value) in registries {
        let name = string(&value, "type").unwrap_or_else(|| key.to_string_lossy());
        let Some(Tag::List(values)) = value.get("value") else {
            return Err(Error::new("Combined registry has no values"));
        };
        let mut ordered = BTreeMap::new();
        for entry in values {
            let id = match entry.get("id") {
                Some(Tag::Int(id)) if (0..65536).contains(id) => *id,
                _ => return Err(Error::new("Invalid combined registry id")),
            };
            let key = string(entry, "name")
                .ok_or_else(|| Error::new("Combined registry has no entry name"))?;
            let mut element = entry
                .get("element")
                .cloned()
                .ok_or_else(|| Error::new("Combined registry has no entry data"))?;
            upgrade(&name, &mut element)?;
            ordered.insert(
                id,
                cfg::RegistryEntry {
                    id: key,
                    data: Some(element),
                },
            );
        }
        let mut entries = Vec::new();
        if let Some((highest, _)) = ordered.last_key_value() {
            let prototype = ordered
                .first_key_value()
                .expect("nonempty registry")
                .1
                .clone();
            for id in 0..=*highest {
                entries.push(
                    ordered
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| cfg::RegistryEntry {
                            id: format!(
                                "viafabricplus:missing_{}_{}",
                                local(&name).replace('/', "_"),
                                id
                            ),
                            data: prototype.data.clone(),
                        }),
                );
            }
        }
        if local(&name) == "damage_type" {
            if !entries.iter().any(|entry| local(&entry.id) == "spit") {
                entries.push(cfg::RegistryEntry {
                    id: "minecraft:spit".into(),
                    data: Some(Tag::compound([
                        (
                            "scaling",
                            Tag::String("when_caused_by_living_non_player".into()),
                        ),
                        ("message_id", Tag::String("mob".into())),
                        ("exhaustion", Tag::Float(0.1f32.to_bits())),
                    ])),
                });
            }
            let Tag::Compound(defaults) = damage_defaults()? else {
                return Err(Error::new("Invalid bundled damage types"));
            };
            for (name, tag) in defaults {
                let name = name.to_string_lossy();
                if !entries.iter().any(|entry| local(&entry.id) == local(&name)) {
                    entries.push(cfg::RegistryEntry {
                        id: if name.contains(':') {
                            name
                        } else {
                            format!("minecraft:{name}")
                        },
                        data: Some(tag),
                    });
                }
            }
        }
        packets.push(registry(&name, entries)?);
    }
    packets.push(registry(
        "minecraft:wolf_variant",
        vec![cfg::RegistryEntry {
            id: "minecraft:pale".into(),
            data: Some(Tag::compound([
                (
                    "wild_texture",
                    Tag::String("minecraft:entity/wolf/wolf".into()),
                ),
                (
                    "tame_texture",
                    Tag::String("minecraft:entity/wolf/wolf_tame".into()),
                ),
                (
                    "angry_texture",
                    Tag::String("minecraft:entity/wolf/wolf_angry".into()),
                ),
                ("biomes", Tag::List(Vec::new())),
            ])),
        }],
    )?);
    packets.push(registry(
        "minecraft:banner_pattern",
        data::PATTERNS
            .iter()
            .map(|name| cfg::RegistryEntry {
                id: format!("minecraft:{name}"),
                data: Some(Tag::compound([
                    ("asset_id", Tag::String((*name).into())),
                    (
                        "translation_key",
                        Tag::String(format!("block.minecraft.banner.{name}").as_str().into()),
                    ),
                ])),
            })
            .collect(),
    )?);
    Ok(packets)
}
pub fn serverbound(body: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut r = Reader::new(body);
    let id = r.varint()?;
    let name = usize::try_from(id)
        .ok()
        .and_then(|id| next::CONFIGURATION_SERVERBOUND.get(id))
        .copied()
        .ok_or_else(|| Error::new("Unknown 1.20.5 configuration reply"))?;
    if matches!(name, "COOKIE_RESPONSE" | "SELECT_KNOWN_PACKS") {
        return Ok(Vec::new());
    }
    let mut w = Writer::new();
    w.varint(wire::packet_id(old::CONFIGURATION_SERVERBOUND, name).map_err(Error::new)?);
    w.bytes(r.rest());
    Ok(vec![w.data])
}
