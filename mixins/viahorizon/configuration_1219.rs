// SPDX-License-Identifier: GPL-3.0-or-later
// Environmental attributes and timeline registries introduced in 1.21.11.
// Output goes through configuration_12111, then the 26.1/26.2 translators.
use crate::{
    Error, Result,
    buf::Reader,
    packets::configuration as cfg,
    vfp_configuration_12111,
    vfp_registry_access::Registries,
    vfp_tables as t, vfp_tables_1219 as old, vfp_tables_12111 as middle,
    vfp_wire::{self as wire, Packet},
};
use horizon_world::nbt::{Compound, JavaString, Tag};
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
fn number(value: &Tag) -> Option<f32> {
    Some(match value {
        Tag::Byte(v) => *v as f32,
        Tag::Short(v) => *v as f32,
        Tag::Int(v) => *v as f32,
        Tag::Long(v) => *v as f32,
        Tag::Float(v) => f32::from_bits(*v),
        Tag::Double(v) => f64::from_bits(*v) as f32,
        _ => return None,
    })
}
fn truth(map: &Compound, name: &str) -> bool {
    map.get(&key(name))
        .and_then(number)
        .is_some_and(|value| value != 0.0)
}
fn attribute(
    base: &Compound,
    attributes: &mut Compound,
    old: &str,
    new: &str,
    default: Option<Tag>,
) {
    if let Some(value) = base.get(&key(old)).cloned().or(default) {
        attributes.insert(key(new), value);
    }
}
fn music(sound: &str, replace: bool, delay: i32) -> Tag {
    let mut map = Compound::new();
    map.insert(key("sound"), text(sound));
    map.insert(key("max_delay"), Tag::Int(24000));
    map.insert(key("min_delay"), Tag::Int(delay));
    if replace {
        map.insert(key("replace_current_music"), Tag::Byte(1));
    }
    Tag::Compound(map)
}
fn cave(attributes: &mut Compound) {
    attributes.insert(
        key("audio/ambient_sounds"),
        Tag::compound([(
            "mood",
            Tag::compound([
                ("tick_delay", Tag::Int(6000)),
                ("offset", Tag::Float(2f32.to_bits())),
                ("sound", text("minecraft:ambient.cave")),
                ("block_search_extent", Tag::Int(8)),
            ]),
        )]),
    );
}
fn dimension(id: &str, base: &mut Compound) {
    let mut attributes = Compound::new();
    match local(id) {
        "the_nether" => {
            base.insert(
                key("timelines"),
                Tag::List(vec![text("minecraft:villager_schedule")]),
            );
            base.insert(key("skybox"), text("none"));
            base.insert(key("cardinal_light"), text("nether"));
            attributes.insert(key("visual/sky_light_color"), text("#7a7aff"));
            attributes.insert(
                key("visual/fog_start_distance"),
                Tag::Float(10f32.to_bits()),
            );
            attributes.insert(key("visual/fog_end_distance"), Tag::Float(96f32.to_bits()));
            attributes.insert(key("gameplay/sky_light_level"), Tag::Float(4f32.to_bits()));
        }
        "the_end" => {
            base.insert(
                key("timelines"),
                Tag::List(vec![text("minecraft:villager_schedule")]),
            );
            base.insert(key("skybox"), text("end"));
            attributes.insert(key("visual/fog_color"), text("#181318"));
            attributes.insert(key("visual/sky_color"), text("#000000"));
            attributes.insert(key("visual/sky_light_color"), text("#ac60cd"));
            cave(&mut attributes);
            attributes.insert(
                key("audio/background_music"),
                Tag::compound([("default", music("minecraft:music.end", true, 6000))]),
            );
        }
        _ => {
            base.insert(
                key("timelines"),
                Tag::List(vec![
                    text("minecraft:day"),
                    text("minecraft:moon"),
                    text("minecraft:early_game"),
                ]),
            );
            attributes.insert(key("visual/fog_color"), text("#c0d8ff"));
            attributes.insert(key("visual/sky_color"), text("#78a7ff"));
            cave(&mut attributes);
            attributes.insert(
                key("audio/background_music"),
                Tag::compound([
                    ("default", music("minecraft:music.game", false, 12000)),
                    ("creative", music("minecraft:music.creative", false, 12000)),
                ]),
            );
        }
    }
    if base.remove(&key("fixed_time")).is_some() {
        base.insert(key("has_fixed_time"), Tag::Byte(1));
    }
    if !truth(base, "natural") {
        attributes.insert(key("visual/sky_light_factor"), Tag::Float(0));
    }
    if truth(base, "ultrawarm") {
        attributes.insert(
            key("visual/default_dripstone_particle"),
            Tag::compound([("type", text("minecraft:dripping_dripstone_lava"))]),
        );
    }
    if let Some(height) = base.get(&key("cloud_height")).and_then(number) {
        attributes.insert(key("visual/cloud_color"), text("#ccffffff"));
        attributes.insert(key("visual/cloud_height"), Tag::Float(height.to_bits()));
    }
    attribute(
        base,
        &mut attributes,
        "has_raids",
        "gameplay/can_start_raid",
        Some(Tag::Byte(1)),
    );
    let zombify = base
        .get(&key("piglin_safe"))
        .map_or(0, |_| i8::from(!truth(base, "piglin_safe")));
    attributes.insert(key("gameplay/piglins_zombify"), Tag::Byte(zombify));
    attribute(
        base,
        &mut attributes,
        "respawn_anchor_works",
        "gameplay/respawn_anchor_works",
        Some(Tag::Byte(1)),
    );
    attribute(
        base,
        &mut attributes,
        "ultrawarm",
        "gameplay/fast_lava",
        Some(Tag::Byte(0)),
    );
    attribute(
        base,
        &mut attributes,
        "ultrawarm",
        "gameplay/water_evaporates",
        Some(Tag::Byte(0)),
    );
    base.insert(key("attributes"), Tag::Compound(attributes));
}
fn biome(id: &str, base: &mut Compound) -> Result<()> {
    let Some(Tag::Compound(effects)) = base.get(&key("effects")) else {
        return Err(Error::new("Old biome has no effects compound"));
    };
    let mut attributes = Compound::new();
    if ![
        "warped_forest",
        "basalt_deltas",
        "nether_wastes",
        "soul_sand_valley",
        "crimson_forest",
    ]
    .contains(&local(id))
    {
        attribute(
            effects,
            &mut attributes,
            "sky_color",
            "visual/sky_color",
            Some(Tag::Int(0)),
        );
    }
    if local(id) != "the_end" {
        attribute(
            effects,
            &mut attributes,
            "water_fog_color",
            "visual/water_fog_color",
            Some(Tag::Int(-16448205)),
        );
        attribute(
            effects,
            &mut attributes,
            "fog_color",
            "visual/fog_color",
            Some(Tag::Int(0)),
        );
    }
    if let Some(Tag::List(music)) = effects.get(&key("music")) {
        if let Some(Tag::Compound(data)) = music.first().and_then(|entry| entry.get("data")) {
            let mut default = Compound::new();
            for field in ["max_delay", "min_delay", "sound"] {
                if let Some(value) = data.get(&key(field)) {
                    default.insert(key(field), value.clone());
                }
            }
            attributes.insert(
                key("audio/background_music"),
                Tag::compound([("default", Tag::Compound(default))]),
            );
        }
    }
    let mut sounds = Compound::new();
    for (old, name) in [
        ("mood_sound", "mood"),
        ("additions_sound", "additions"),
        ("ambient_sound", "loop"),
    ] {
        if let Some(value) = effects.get(&key(old)) {
            sounds.insert(key(name), value.clone());
        }
    }
    if !sounds.is_empty() {
        attributes.insert(key("audio/ambient_sounds"), Tag::Compound(sounds));
    }
    if let Some(Tag::Compound(particle)) = effects.get(&key("particle")) {
        let probability = particle
            .get(&key("probability"))
            .ok_or_else(|| Error::new("Missing biome particle probability"))?;
        let options = particle
            .get(&key("options"))
            .ok_or_else(|| Error::new("Missing biome particle options"))?;
        attributes.insert(
            key("visual/ambient_particles"),
            Tag::List(vec![Tag::compound([
                ("probability", probability.clone()),
                ("particle", options.clone()),
            ])]),
        );
    }
    base.insert(key("attributes"), Tag::Compound(attributes));
    Ok(())
}
pub fn upgrade(registry: &str, id: &str, tag: &mut Tag) -> Result<()> {
    if !matches!(local(registry), "dimension_type" | "worldgen/biome") {
        return Ok(());
    }
    let mut converted = tag.clone();
    let Tag::Compound(base) = &mut converted else {
        return Err(Error::new("Invalid old environmental registry record"));
    };
    if local(registry) == "dimension_type" {
        dimension(id, base);
    } else {
        biome(id, base)?;
    }
    *tag = converted;
    Ok(())
}
pub fn play_tags(payload: &[u8]) -> Result<Vec<u8>> {
    let id =
        wire::packet_id(middle::CONFIGURATION_CLIENTBOUND, "UPDATE_TAGS").map_err(Error::new)?;
    let (_, data) =
        Configuration::default().clientbound(id, payload, &mut Registries::default())?;
    Ok(data)
}
#[derive(Default)]
pub struct Configuration {
    next: vfp_configuration_12111::Configuration,
    seen: BTreeSet<String>,
}
impl Configuration {
    fn convert(
        &mut self,
        mut packet: cfg::Clientbound,
        registry: &mut Registries,
    ) -> Result<Packet> {
        match &mut packet {
            cfg::Clientbound::RegistryData(data) => {
                self.seen.insert(local(&data.registry).to_owned());
                for entry in &mut data.entries {
                    if let Some(tag) = &mut entry.data {
                        upgrade(&data.registry, &entry.id, tag)?;
                    }
                }
            }
            cfg::Clientbound::UpdateTags(data) => {
                for registry in &mut data.registries {
                    let table = match local(&registry.registry) {
                        "block" => Some(old::STEP_BLOCKS),
                        "item" => Some(old::STEP_ITEMS),
                        "entity_type" => Some(old::STEP_ENTITIES),
                        _ => None,
                    };
                    if let Some(table) = table {
                        for tag in &mut registry.tags {
                            for id in &mut tag.ids {
                                *id = table.map(*id).map_err(Error::new)?;
                            }
                        }
                    }
                }
            }
            _ => (),
        }
        let bytes = packet.encode();
        let mut r = Reader::new(&bytes);
        let id = r.varint()?;
        let name = t::NEW_CONFIGURATION
            .get(id as usize)
            .ok_or_else(|| Error::new("Unknown intermediate configuration packet"))?;
        let old_id =
            wire::packet_id(middle::CONFIGURATION_CLIENTBOUND, name).map_err(Error::new)?;
        self.next.clientbound(old_id, r.rest(), registry)
    }
    pub fn clientbound(
        &mut self,
        id: i32,
        payload: &[u8],
        registry: &mut Registries,
    ) -> Result<Packet> {
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| middle::CONFIGURATION_CLIENTBOUND.get(id))
            .ok_or_else(|| Error::new("Unknown 1.21.9 configuration packet"))?;
        self.convert(
            cfg::Clientbound::decode(
                wire::packet_id(t::NEW_CONFIGURATION, name).map_err(Error::new)?,
                payload,
            )?,
            registry,
        )
    }
    pub fn finish(&mut self, registry: &mut Registries) -> Result<Vec<Packet>> {
        let mut packets = Vec::new();
        if !self.seen.contains("zombie_nautilus_variant") {
            packets.push(self.convert(
                cfg::Clientbound::RegistryData(cfg::RegistryData {
                    registry: "minecraft:zombie_nautilus_variant".into(),
                    entries: vec![cfg::RegistryEntry {
                        id: "minecraft:pale".into(),
                        data: Some(Tag::compound([
                            (
                                "asset_id",
                                text("minecraft:entity/zombie_nautilus/temperate"),
                            ),
                            ("spawn_conditions", Tag::List(Vec::new())),
                        ])),
                    }],
                }),
                registry,
            )?);
        }
        if !self.seen.contains("timeline") {
            let (root, consumed) = horizon_world::nbt::read_named(
                old::TIMELINE,
                horizon_world::nbt::Limits {
                    max_bytes: old::TIMELINE.len(),
                    max_nodes: 100000,
                    max_depth: 64,
                },
            )
            .map_err(|e| Error::new(e.to_string()))?;
            if consumed != old::TIMELINE.len() {
                return Err(Error::new("Trailing timeline registry data"));
            }
            let entries = old::TIMELINE_ENTRY_ORDER
                .iter()
                .map(|name| {
                    root.tag
                        .get(name)
                        .cloned()
                        .map(|tag| cfg::RegistryEntry {
                            id: (*name).into(),
                            data: Some(tag),
                        })
                        .ok_or_else(|| Error::new("Missing ordered timeline entry"))
                })
                .collect::<Result<Vec<_>>>()?;
            packets.push(self.convert(
                cfg::Clientbound::RegistryData(cfg::RegistryData {
                    registry: "minecraft:timeline".into(),
                    entries,
                }),
                registry,
            )?);
        }
        packets.extend(self.next.finish(registry)?);
        Ok(packets)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
