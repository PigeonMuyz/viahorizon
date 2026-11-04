// SPDX-License-Identifier: GPL-3.0-or-later
// The first actual chained translation stage: 26.1 -> 26.2 -> native 26.3.
use crate::{
    Error, Result, buf::Reader, packets::configuration as cfg, vfp_tables as tables,
    vfp_tables_261 as previous, vfp_wire as wire,
};
use horizon_world::nbt::{JavaString, Tag};
fn key(v: &str) -> JavaString {
    v.into()
}
fn local(v: &str) -> &str {
    v.strip_prefix("minecraft:").unwrap_or(v)
}
fn entity_predicate(tag: &mut Tag, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Entity predicate nesting exceeds 64"));
    }
    let Tag::Compound(map) = tag else {
        return Err(Error::new("Entity predicate is not a compound"));
    };
    if let Some(kind) = map.remove(&key("type")) {
        map.insert(key("entity_type"), kind);
    }
    if let Some(specific) = map.remove(&key("type_specific")) {
        let Tag::Compound(mut specific) = specific else {
            return Err(Error::new("Invalid type-specific entity predicate"));
        };
        if let Some(kind) = specific.remove(&key("type")) {
            let Tag::String(kind) = kind else {
                return Err(Error::new("Invalid entity subtype discriminator"));
            };
            let kind = kind.to_string_lossy();
            let kind = local(&kind);
            let subtype = match kind {
                "player" | "lightning" | "fishing_hook" | "raider" | "sheep" => kind,
                "slime" => "cube_mob",
                _ => return Err(Error::new("Unsupported 26.1 entity subtype predicate")),
            };
            map.insert(
                key(&format!("type_specific/{subtype}")),
                Tag::Compound(specific),
            );
        }
    }
    for name in ["vehicle", "passenger", "targeted_entity"] {
        if let Some(tag) = map.get_mut(&key(name)) {
            entity_predicate(tag, depth + 1)?;
        }
    }
    Ok(())
}
fn enchantments(tag: &mut Tag, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Enchantment nesting exceeds 64"));
    }
    match tag {
        Tag::List(values) => {
            for v in values {
                enchantments(v, depth + 1)?;
            }
        }
        Tag::Compound(map) => {
            let kind = match map.get(&key("condition")) {
                Some(Tag::String(v)) => v.to_string_lossy(),
                _ => String::new(),
            };
            match local(&kind) {
                "entity_properties" => {
                    if let Some(p) = map.get_mut(&key("predicate")) {
                        entity_predicate(p, depth + 1)?;
                    }
                }
                "damage_source_properties" => {
                    if let Some(Tag::Compound(p)) = map.get_mut(&key("predicate")) {
                        for name in ["source_entity", "direct_entity"] {
                            if let Some(v) = p.get_mut(&key(name)) {
                                entity_predicate(v, depth + 1)?;
                            }
                        }
                    }
                }
                _ => (),
            }
            for v in map.values_mut() {
                enchantments(v, depth + 1)?;
            }
        }
        _ => (),
    }
    Ok(())
}
pub fn upgrade_enchantment(tag: &mut Tag) -> Result<()> {
    let mut next = tag.clone();
    enchantments(&mut next, 0)?;
    *tag = next;
    Ok(())
}
pub fn clientbound(id: i32, payload: &[u8]) -> Result<wire::Packet> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|i| tables::OLD_CONFIGURATION.get(i))
        .copied()
        .ok_or_else(|| Error::new("Unknown 26.1 configuration packet"))?;
    let mapped = wire::packet_id(tables::NEW_CONFIGURATION, name).map_err(Error::new)?;
    let mut packet = cfg::Clientbound::decode(mapped, payload)?;
    match &mut packet {
        cfg::Clientbound::UpdateTags(update) => {
            for registry in &mut update.registries {
                let table = match local(&registry.registry) {
                    "block" => Some(previous::BLOCKS),
                    "item" => Some(previous::ITEMS),
                    "entity_type" => Some(previous::ENTITIES),
                    _ => None,
                };
                if let Some(table) = table {
                    for tag in &mut registry.tags {
                        for id in &mut tag.ids {
                            *id = table.map(*id).map_err(Error::new)?;
                        }
                    }
                }
                if local(&registry.registry) == "block" {
                    for name in [
                        "minecraft:infiniburn_nether",
                        "minecraft:infiniburn_end",
                        "minecraft:infiniburn_overworld",
                    ] {
                        if !registry.tags.iter().any(|tag| tag.name == name) {
                            registry.tags.push(cfg::TagEntry {
                                name: name.into(),
                                ids: Vec::new(),
                            });
                        }
                    }
                }
            }
            if !update
                .registries
                .iter()
                .any(|v| local(&v.registry) == "block")
            {
                update.registries.push(cfg::RegistryTags {
                    registry: "minecraft:block".into(),
                    tags: [
                        "minecraft:infiniburn_nether",
                        "minecraft:infiniburn_end",
                        "minecraft:infiniburn_overworld",
                    ]
                    .into_iter()
                    .map(|name| cfg::TagEntry {
                        name: name.into(),
                        ids: Vec::new(),
                    })
                    .collect(),
                });
            }
        }
        cfg::Clientbound::RegistryData(registry) => {
            if local(&registry.registry) == "enchantment" {
                for entry in &mut registry.entries {
                    if let Some(tag) = &mut entry.data {
                        upgrade_enchantment(tag)?;
                    }
                }
            }
            if local(&registry.registry) == "jukebox_song"
                && !registry.entries.iter().any(|v| local(&v.id) == "bounce")
            {
                registry.entries.push(cfg::RegistryEntry {
                    id: "minecraft:bounce".into(),
                    data: Some(Tag::compound([
                        (
                            "description",
                            Tag::compound([(
                                "translate",
                                Tag::String("jukebox_song.minecraft.bounce".into()),
                            )]),
                        ),
                        (
                            "sound_event",
                            Tag::String("minecraft:music_disc.bounce".into()),
                        ),
                        ("length_in_seconds", Tag::Float(175f32.to_bits())),
                        ("comparator_output", Tag::Int(10)),
                    ])),
                });
            }
        }
        _ => (),
    }
    let encoded = packet.encode();
    let mut r = Reader::new(&encoded);
    r.varint()?;
    crate::vfp_configuration::clientbound_262(id, r.rest())
}
