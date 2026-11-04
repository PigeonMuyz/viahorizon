// SPDX-License-Identifier: GPL-3.0-or-later
// 1.21.11 registry records are upgraded before the existing 26.1 -> 26.3 chain.
use crate::{
    Error, Result,
    buf::Reader,
    packets::configuration as cfg,
    vfp_registry_access::Registries,
    vfp_tables as t, vfp_tables_12111 as old,
    vfp_wire::{self as wire, Packet},
};
use horizon_world::nbt::{JavaString, Tag};
use std::collections::BTreeSet;
fn key(s: &str) -> JavaString {
    s.into()
}
fn text(s: &str) -> Tag {
    Tag::String(s.into())
}
fn local(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}
fn compound() -> Tag {
    Tag::compound([] as [(&str, Tag); 0])
}
fn asset(tag: &mut Tag, entity: &str, swap: bool) -> Result<()> {
    let Some(Tag::String(id)) = tag.get_mut("asset_id") else {
        return Ok(());
    };
    let mut name = id.to_string_lossy();
    if swap {
        if let Some(prefix) = name.strip_suffix(&format!("_{entity}")) {
            name = prefix.replace(&format!("{entity}/"), &format!("{entity}/{entity}_"));
        }
    } else {
        name = name.replace(&format!("{entity}/"), &format!("{entity}/{entity}_"));
    }
    *id = key(&name);
    if entity != "frog" {
        let Tag::Compound(map) = tag else {
            unreachable!()
        };
        map.insert(key("baby_asset_id"), text(&format!("{name}_baby")));
    }
    Ok(())
}
fn enchantments(tag: &mut Tag, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Enchantment expression nesting exceeds 64"));
    }
    match tag {
        Tag::List(values) => {
            for value in values {
                enchantments(value, depth + 1)?;
            }
        }
        Tag::Compound(map) => {
            if matches!(map.get(&key("condition")),Some(Tag::String(s)) if local(&s.to_string_lossy()) == "time_check")
            {
                map.insert(key("clock"), text("minecraft:overworld"));
            }
            for value in map.values_mut() {
                enchantments(value, depth + 1)?;
            }
        }
        _ => (),
    }
    Ok(())
}
pub fn upgrade(registry: &str, id: &str, value: &mut Tag) -> Result<()> {
    let mut converted = value.clone();
    let Tag::Compound(map) = &mut converted else {
        return Err(Error::new("Registry entry is not a compound"));
    };
    match local(registry) {
        "dimension_type" => {
            let dimension = local(id);
            map.insert(
                key("has_ender_dragon_fight"),
                Tag::Byte(i8::from(dimension == "the_end")),
            );
            match dimension {
                "overworld" | "overworld_caves" => {
                    map.insert(key("default_clock"), text("minecraft:overworld"));
                }
                "the_end" => {
                    map.insert(key("default_clock"), text("minecraft:the_end"));
                }
                _ => (),
            }
            let color = match dimension {
                "the_end" => -12630209,
                "the_nether" => -13621215,
                "overworld" => -16119286,
                _ => {
                    let ambient = match map.get(&key("ambient_light")) {
                        Some(Tag::Float(v)) => f32::from_bits(*v),
                        Some(Tag::Double(v)) => f64::from_bits(*v) as f32,
                        _ => 0.0,
                    };
                    let value = ((10.0f32 + ambient * 245.0) + 0.5).floor() as i32;
                    let v = value.clamp(10, 255);
                    (0xff000000u32 as i32) | (v << 16) | (v << 8) | v
                }
            };
            let attributes = map.entry(key("attributes")).or_insert_with(compound);
            let Tag::Compound(attributes) = attributes else {
                return Err(Error::new("Invalid dimension attributes"));
            };
            attributes.insert(key("visual/ambient_light_color"), Tag::Int(color));
        }
        "timeline" => {
            map.insert(key("clock"), text("minecraft:overworld"));
        }
        "wolf_sound_variant" => {
            let mut sounds = std::mem::take(map);
            sounds.insert(key("step_sound"), text("minecraft:entity.wolf.step"));
            map.insert(key("adult_sounds"), Tag::Compound(sounds.clone()));
            map.insert(key("baby_sounds"), Tag::Compound(sounds));
        }
        "wolf_variant" => {
            if let Some(Tag::Compound(assets)) = map.get(&key("assets")) {
                let mut baby = assets.clone();
                for value in baby.values_mut() {
                    let Tag::String(id) = value else {
                        return Err(Error::new("Invalid wolf texture identifier"));
                    };
                    *id = key(&format!("{}_baby", id.to_string_lossy()));
                }
                map.insert(key("baby_assets"), Tag::Compound(baby));
            }
        }
        "frog_variant" => asset(&mut converted, "frog", true)?,
        "chicken_variant" => asset(&mut converted, "chicken", true)?,
        "cow_variant" => asset(&mut converted, "cow", true)?,
        "pig_variant" => asset(&mut converted, "pig", true)?,
        "cat_variant" => asset(&mut converted, "cat", false)?,
        "enchantment" => enchantments(&mut converted, 0)?,
        _ => (),
    }
    *value = converted;
    Ok(())
}
fn entry(id: &str, data: Tag) -> cfg::RegistryEntry {
    cfg::RegistryEntry {
        id: if id.contains(':') {
            id.into()
        } else {
            format!("minecraft:{id}")
        },
        data: Some(data),
    }
}
fn defaults(registry: &str) -> Vec<cfg::RegistryEntry> {
    match local(registry) {
        "world_clock" => vec![entry("overworld", compound()), entry("the_end", compound())],
        "damage_type" => vec![entry(
            "spear",
            Tag::compound([
                ("exhaustion", Tag::Float(0.1f32.to_bits())),
                ("message_id", text("spear")),
                ("scaling", text("when_caused_by_living_non_player")),
            ]),
        )],
        "chicken_variant" => ["temperate", "cold", "warm"]
            .into_iter()
            .map(|name| {
                let mut tag = Tag::compound([
                    (
                        "asset_id",
                        text(&format!("minecraft:entity/chicken/chicken_{name}")),
                    ),
                    (
                        "baby_asset_id",
                        text(&format!("minecraft:entity/chicken/chicken_{name}_baby")),
                    ),
                ]);
                if name == "cold" {
                    let Tag::Compound(map) = &mut tag else {
                        unreachable!()
                    };
                    map.insert(key("model"), text("cold"));
                }
                entry(name, tag)
            })
            .collect(),
        "instrument" => vec![entry(
            "ponder_goat_horn",
            Tag::compound([
                (
                    "description",
                    Tag::compound([("translate", text("instrument.minecraft.ponder_goat_horn"))]),
                ),
                ("sound_event", text("minecraft:item.goat_horn.sound.0")),
                ("use_duration", Tag::Float(7f32.to_bits())),
                ("range", Tag::Float(256f32.to_bits())),
            ]),
        )],
        "trim_material" => [
            "quartz",
            "iron",
            "netherite",
            "redstone",
            "copper",
            "gold",
            "emerald",
            "diamond",
            "lapis",
            "amethyst",
            "resin",
        ]
        .into_iter()
        .map(|name| {
            entry(
                name,
                Tag::compound([
                    (
                        "description",
                        Tag::compound([(
                            "translate",
                            text(&format!("trim_material.minecraft.{name}")),
                        )]),
                    ),
                    ("asset_name", text(name)),
                ]),
            )
        })
        .collect(),
        "jukebox_song" => [
            "11",
            "13",
            "5",
            "blocks",
            "cat",
            "chirp",
            "far",
            "mall",
            "mellohi",
            "otherside",
            "pigstep",
            "relic",
            "stal",
            "strad",
            "wait",
            "ward",
            "creator",
            "creator_music_box",
            "lava_chicken",
            "tears",
            "precipice",
        ]
        .into_iter()
        .map(|name| {
            entry(
                name,
                Tag::compound([
                    (
                        "description",
                        Tag::compound([(
                            "translate",
                            text(&format!("jukebox_song.minecraft.{name}")),
                        )]),
                    ),
                    ("sound_event", text(&format!("minecraft:music_disc.{name}"))),
                    ("length_in_seconds", Tag::Float(175f32.to_bits())),
                    ("comparator_output", Tag::Int(10)),
                ]),
            )
        })
        .collect(),
        _ => Vec::new(),
    }
}
fn required_tags(update: &mut cfg::UpdateTags) {
    let damage = [
        "damages_helmet",
        "bypasses_armor",
        "bypasses_shield",
        "bypasses_invulnerability",
        "bypasses_cooldown",
        "bypasses_effects",
        "bypasses_resistance",
        "bypasses_enchantments",
        "is_fire",
        "is_projectile",
        "witch_resistant_to",
        "is_explosion",
        "is_fall",
        "is_drowning",
        "is_freezing",
        "is_lightning",
        "no_anger",
        "no_impact",
        "always_most_significant_fall",
        "wither_immune_to",
        "ignites_armor_stands",
        "burns_armor_stands",
        "avoids_guardian_thorns",
        "always_triggers_silverfish",
        "always_hurts_ender_dragons",
        "no_knockback",
        "always_kills_armor_stands",
        "can_break_armor_stand",
        "bypasses_wolf_armor",
        "is_player_attack",
        "burn_from_stepping",
        "panic_causes",
        "panic_environmental_causes",
        "mace_smash",
    ];
    let banners = [
        "no_item_required",
        "pattern_item/flower",
        "pattern_item/creeper",
        "pattern_item/skull",
        "pattern_item/mojang",
        "pattern_item/globe",
        "pattern_item/piglin",
        "pattern_item/flow",
        "pattern_item/guster",
        "pattern_item/field_masoned",
        "pattern_item/bordure_indented",
    ];
    for (name, missing) in [
        ("damage_type", damage.as_slice()),
        ("banner_pattern", banners.as_slice()),
    ] {
        let index = match update
            .registries
            .iter()
            .position(|r| local(&r.registry) == name)
        {
            Some(index) => index,
            None => {
                update.registries.push(cfg::RegistryTags {
                    registry: format!("minecraft:{name}"),
                    tags: Vec::new(),
                });
                update.registries.len() - 1
            }
        };
        let registry = &mut update.registries[index];
        for name in missing {
            if !registry.tags.iter().any(|tag| local(&tag.name) == *name) {
                registry.tags.push(cfg::TagEntry {
                    name: format!("minecraft:{name}"),
                    ids: Vec::new(),
                });
            }
        }
    }
}
pub fn play_tags(payload: &[u8]) -> Result<Vec<u8>> {
    let id = wire::packet_id(old::CONFIGURATION_CLIENTBOUND, "UPDATE_TAGS").map_err(Error::new)?;
    let (_, data) =
        Configuration::default().clientbound(id, payload, &mut Registries::default())?;
    Ok(data)
}
#[derive(Default)]
pub struct Configuration {
    seen: BTreeSet<String>,
    tags_sent: bool,
}
impl Configuration {
    fn convert(
        &mut self,
        mut packet: cfg::Clientbound,
        registry: &mut Registries,
    ) -> Result<Packet> {
        match &mut packet {
            cfg::Clientbound::RegistryData(data) => {
                let name = local(&data.registry);
                for value in &mut data.entries {
                    if let Some(tag) = &mut value.data {
                        upgrade(name, &value.id, tag)?;
                    }
                }
                for value in defaults(name) {
                    if !data
                        .entries
                        .iter()
                        .any(|e| local(&e.id) == local(&value.id))
                    {
                        data.entries.push(value);
                    }
                }
                registry
                    .replace(name, data.entries.iter().map(|e| e.id.clone()))
                    .map_err(Error::new)?;
                self.seen.insert(name.to_owned());
            }
            cfg::Clientbound::UpdateTags(update) => {
                for registry in &mut update.registries {
                    let mapping = match local(&registry.registry) {
                        "block" => Some(old::STEP_BLOCKS),
                        "item" => Some(old::STEP_ITEMS),
                        "entity_type" => Some(old::STEP_ENTITIES),
                        _ => None,
                    };
                    if let Some(map) = mapping {
                        for tag in &mut registry.tags {
                            for id in &mut tag.ids {
                                *id = map.map(*id).map_err(Error::new)?;
                            }
                        }
                    }
                }
                required_tags(update);
                self.tags_sent = true;
            }
            _ => (),
        }
        let bytes = packet.encode();
        let mut r = Reader::new(&bytes);
        let id = r.varint()?;
        let name = t::NEW_CONFIGURATION
            .get(id as usize)
            .ok_or_else(|| Error::new("Unknown generated configuration packet"))?;
        let id = wire::packet_id(t::OLD_CONFIGURATION, name).map_err(Error::new)?;
        crate::vfp_configuration_261::clientbound(id, r.rest())
    }
    pub fn clientbound(
        &mut self,
        id: i32,
        payload: &[u8],
        registry: &mut Registries,
    ) -> Result<Packet> {
        let name = usize::try_from(id)
            .ok()
            .and_then(|i| old::CONFIGURATION_CLIENTBOUND.get(i))
            .ok_or_else(|| Error::new("Unknown 1.21.11 configuration packet"))?;
        let id = wire::packet_id(t::NEW_CONFIGURATION, name).map_err(Error::new)?;
        self.convert(cfg::Clientbound::decode(id, payload)?, registry)
    }
    pub fn finish(&mut self, registry: &mut Registries) -> Result<Vec<Packet>> {
        let mut packets = Vec::new();
        for name in [
            "damage_type",
            "chicken_variant",
            "instrument",
            "jukebox_song",
            "trim_material",
            "world_clock",
        ] {
            if !self.seen.contains(name) {
                packets.push(self.convert(
                    cfg::Clientbound::RegistryData(cfg::RegistryData {
                        registry: format!("minecraft:{name}"),
                        entries: defaults(name),
                    }),
                    registry,
                )?);
            }
        }
        let blob = old::SOUND_VARIANTS_261;
        let (root, consumed) = horizon_world::nbt::read_named(
            blob,
            horizon_world::nbt::Limits {
                max_bytes: blob.len(),
                max_nodes: 100000,
                max_depth: 64,
            },
        )
        .map_err(|e| Error::new(e.to_string()))?;
        if consumed != blob.len() {
            return Err(Error::new("Trailing sound variant registry data"));
        }
        for name in [
            "cat_sound_variant",
            "cow_sound_variant",
            "pig_sound_variant",
            "chicken_sound_variant",
        ] {
            if self.seen.contains(name) {
                continue;
            }
            let Some(Tag::Compound(entries)) = root.tag.get(name) else {
                return Err(Error::new("Missing bundled sound variant registry"));
            };
            packets.push(
                self.convert(
                    cfg::Clientbound::RegistryData(cfg::RegistryData {
                        registry: format!("minecraft:{name}"),
                        entries: old::SOUND_VARIANT_ENTRY_ORDER
                            .iter()
                            .find(|(registry, _)| *registry == name)
                            .ok_or_else(|| Error::new("Missing sound variant registry order"))?
                            .1
                            .iter()
                            .map(|id| {
                                entries
                                    .get(&key(id))
                                    .cloned()
                                    .map(|tag| entry(id, tag))
                                    .ok_or_else(|| Error::new("Missing ordered sound variant"))
                            })
                            .collect::<Result<Vec<_>>>()?,
                    }),
                    registry,
                )?,
            );
        }
        if !self.tags_sent {
            packets.push(self.convert(
                cfg::Clientbound::UpdateTags(cfg::UpdateTags {
                    registries: Vec::new(),
                }),
                registry,
            )?);
        }
        Ok(packets)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
