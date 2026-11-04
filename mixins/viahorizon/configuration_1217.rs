// SPDX-License-Identifier: GPL-3.0-or-later
// 1.21.2-1.21.8 configuration enters the 1.21.9 environmental registry chain.
use crate::{
    Error, Result,
    buf::Reader,
    packets::configuration as cfg,
    vfp_configuration_1219,
    vfp_registry_access::Registries,
    vfp_tables as t, vfp_tables_1217 as old, vfp_tables_12111 as modern,
    vfp_wire::{self as wire, Packet},
};
use horizon_world::nbt::{JavaString, Tag};
fn local(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}
fn particle(tag: &mut Tag) {
    if matches!(tag.get("type"), Some(Tag::String(name)) if local(&name.to_string_lossy()) == "flash")
    {
        if let Tag::Compound(data) = tag {
            data.insert(JavaString::from("color"), Tag::Int(-1));
        }
    }
}
fn effects(tag: &mut Tag, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Enchantment effects nest too deeply"));
    }
    match tag {
        Tag::List(values) => {
            for value in values {
                effects(value, depth + 1)?;
            }
        }
        Tag::Compound(values) => {
            if values.contains_key(&JavaString::from("amount"))
                && values.contains_key(&JavaString::from("operation"))
            {
                if let Some(Tag::String(attribute)) = values.get_mut(&JavaString::from("attribute"))
                {
                    let value = attribute.to_string_lossy();
                    if !value.contains(':') || value.starts_with("minecraft:") {
                        let value = local(&value);
                        let value = ["generic.", "player.", "horse.", "zombie."]
                            .into_iter()
                            .find_map(|prefix| value.strip_prefix(prefix))
                            .unwrap_or(value);
                        *attribute = format!("minecraft:{value}").as_str().into();
                    }
                }
            }
            if matches!(values.get(&JavaString::from("type")),Some(Tag::String(name)) if local(&name.to_string_lossy()) == "spawn_particles")
            {
                if let Some(value) = values.get_mut(&JavaString::from("particle")) {
                    particle(value);
                }
            }
            // Enchantment effect records and their conditional wrappers, not
            // arbitrary item custom data, contain the nested effects.
            for field in ["effect", "effects"] {
                if let Some(value) = values.get_mut(&JavaString::from(field)) {
                    effects(value, depth + 1)?;
                }
            }
        }
        _ => (),
    }
    Ok(())
}
fn upgrade(registry: &str, id: &str, tag: &mut Tag) -> Result<()> {
    match local(registry) {
        "dimension_type" if local(id) == "the_end" => {
            let Tag::Compound(data) = tag else {
                return Err(Error::new("Invalid dimension type"));
            };
            data.insert(
                JavaString::from("ambient_light"),
                Tag::Float(0.25f32.to_bits()),
            );
        }
        "worldgen/biome" => {
            if let Some(options) = tag
                .get_mut("effects")
                .and_then(|value| value.get_mut("particle"))
                .and_then(|value| value.get_mut("options"))
            {
                particle(options);
            }
        }
        "enchantment" => {
            if let Some(Tag::Compound(fields)) = tag.get_mut("effects") {
                for value in fields.values_mut() {
                    effects(value, 0)?;
                }
            }
        }
        _ => (),
    }
    Ok(())
}
pub fn map_tag_id(protocol: i32, registry: &str, id: i32) -> Result<i32> {
    let id = if protocol < 762 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_1193::STEP_BLOCKS,
            "item" => crate::vfp_tables_1193::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1193::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol < 763 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_1194::STEP_BLOCKS,
            "item" => crate::vfp_tables_1194::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1194::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol < 764 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_120::STEP_BLOCKS,
            "item" => crate::vfp_tables_120::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_120::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol < 765 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_1202::STEP_BLOCKS,
            "item" => crate::vfp_tables_1202::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1202::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol < 766 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_1203::STEP_BLOCKS,
            "item" => crate::vfp_tables_1203::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1203::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol < 767 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_1205::STEP_BLOCKS,
            "item" => crate::vfp_tables_1205::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1205::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol <= 767 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_121::STEP_BLOCKS,
            "item" => crate::vfp_tables_121::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_121::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol <= 768 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_1212::STEP_BLOCKS,
            "item" => crate::vfp_tables_1212::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1212::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol <= 769 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_1214::STEP_BLOCKS,
            "item" => crate::vfp_tables_1214::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1214::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let id = if protocol <= 770 {
        let map = match local(registry) {
            "block" => crate::vfp_tables_1215::STEP_BLOCKS,
            "item" => crate::vfp_tables_1215::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1215::STEP_ENTITIES,
            _ => return Ok(id),
        };
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let first = if protocol <= 771 {
        Some(match local(registry) {
            "block" => crate::vfp_tables_1216::STEP_BLOCKS,
            "item" => crate::vfp_tables_1216::STEP_ITEMS,
            "entity_type" => crate::vfp_tables_1216::STEP_ENTITIES,
            _ => return Ok(id),
        })
    } else {
        None
    };
    let id = if let Some(map) = first {
        map.map(id).map_err(Error::new)?
    } else {
        id
    };
    let map = match local(registry) {
        "block" => old::STEP_BLOCKS,
        "item" => old::STEP_ITEMS,
        "entity_type" => old::STEP_ENTITIES,
        _ => return Ok(id),
    };
    map.map(id).map_err(Error::new)
}
pub struct Configuration {
    protocol: i32,
    next: vfp_configuration_1219::Configuration,
}
impl Configuration {
    pub fn new(protocol: i32) -> Self {
        Self {
            protocol,
            next: vfp_configuration_1219::Configuration::default(),
        }
    }
    pub fn clientbound(
        &mut self,
        id: i32,
        payload: &[u8],
        registry: &mut Registries,
    ) -> Result<Packet> {
        let source = if self.protocol <= 770 {
            crate::vfp_tables_1215::CONFIGURATION_CLIENTBOUND
        } else {
            old::CONFIGURATION_CLIENTBOUND
        };
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| source.get(id))
            .ok_or_else(|| Error::new("Unknown 1.21.4-1.21.8 configuration packet"))?;
        let mapped = wire::packet_id(t::NEW_CONFIGURATION, name).map_err(Error::new)?;
        let mut packet = cfg::Clientbound::decode(mapped, payload)?;
        match &mut packet {
            cfg::Clientbound::RegistryData(data) => {
                if self.protocol < 767
                    && local(&data.registry) == "damage_type"
                    && !data
                        .entries
                        .iter()
                        .any(|entry| local(&entry.id) == "campfire")
                {
                    data.entries.push(cfg::RegistryEntry {
                        id: "minecraft:campfire".into(),
                        data: Some(Tag::compound([
                            (
                                "scaling",
                                Tag::String("when_caused_by_living_non_player".into()),
                            ),
                            ("message_id", Tag::String("inFire".into())),
                            ("exhaustion", Tag::Float(0.1f32.to_bits())),
                        ])),
                    });
                }
                if local(&data.registry) == "dimension_type" {
                    let scales = data
                        .entries
                        .iter()
                        .map(|entry| {
                            match entry
                                .data
                                .as_ref()
                                .and_then(|tag| tag.get("coordinate_scale"))
                            {
                                Some(Tag::Double(value)) => f64::from_bits(*value),
                                Some(Tag::Float(value)) => f64::from(f32::from_bits(*value)),
                                Some(Tag::Byte(value)) => f64::from(*value),
                                Some(Tag::Short(value)) => f64::from(*value),
                                Some(Tag::Long(value)) => *value as f64,
                                Some(Tag::Int(value)) => f64::from(*value),
                                _ => 1.0,
                            }
                        })
                        .collect();
                    registry.dimension_scales(scales).map_err(Error::new)?;
                }
                for entry in &mut data.entries {
                    if let Some(tag) = &mut entry.data {
                        if self.protocol <= 770 && local(&data.registry) == "dimension_type" {
                            let cloudy = !matches!(tag.get("effects"), Some(Tag::String(value)) if matches!(local(&value.to_string_lossy()), "the_nether" | "the_end"));
                            if cloudy {
                                if let Tag::Compound(fields) = tag {
                                    fields
                                        .entry(JavaString::from("cloud_height"))
                                        .or_insert(Tag::Int(192));
                                }
                            }
                        }
                        if self.protocol <= 769 && local(&data.registry) == "wolf_variant" {
                            let Tag::Compound(fields) = tag else {
                                return Err(Error::new("Invalid old wolf variant"));
                            };
                            let mut assets = std::collections::BTreeMap::new();
                            for (before, after) in [
                                ("wild_texture", "wild"),
                                ("tame_texture", "tame"),
                                ("angry_texture", "angry"),
                            ] {
                                if let Some(value) = fields.remove(&JavaString::from(before)) {
                                    assets.insert(JavaString::from(after), value);
                                }
                            }
                            fields.remove(&JavaString::from("biomes"));
                            fields.insert(JavaString::from("assets"), Tag::Compound(assets));
                        }
                        if self.protocol < 769 && local(&data.registry) == "worldgen/biome" {
                            if let Some(Tag::Compound(effects)) = tag.get_mut("effects") {
                                if let Some(music) = effects.remove(&JavaString::from("music")) {
                                    let music = if matches!(music, Tag::Compound(_)) {
                                        Tag::List(vec![Tag::compound([
                                            ("data", music),
                                            ("weight", Tag::Int(1)),
                                        ])])
                                    } else {
                                        music
                                    };
                                    effects.insert(JavaString::from("music"), music);
                                }
                            }
                        }
                        upgrade(&data.registry, &entry.id, tag)?;
                    }
                }
            }
            cfg::Clientbound::UpdateTags(data) => {
                for tags in &mut data.registries {
                    if self.protocol < 763 && local(&tags.registry) == "block" {
                        tags.tags
                            .retain(|tag| local(&tag.name) != "replaceable_plants");
                    }
                    if self.protocol < 766 && local(&tags.registry) == "item" {
                        tags.tags.retain(|tag| local(&tag.name) != "tools");
                        for tag in &mut tags.tags {
                            if local(&tag.name) == "axolotl_tempt_items" {
                                tag.name = "minecraft:axolotl_food".into();
                            }
                        }
                    }
                    for tag in &mut tags.tags {
                        if self.protocol <= 769
                            && local(&tags.registry) == "block"
                            && local(&tag.name) == "dead_bush_may_place_on"
                        {
                            tag.name = "minecraft:dry_vegetation_may_place_on".into();
                        }
                        if self.protocol <= 770
                            && local(&tags.registry) == "block"
                            && local(&tag.name) == "plays_ambient_desert_block_sounds"
                        {
                            tag.name =
                                "minecraft:triggers_ambient_desert_dry_vegetation_block_sounds"
                                    .into();
                        }
                        for id in &mut tag.ids {
                            *id = map_tag_id(self.protocol, &tags.registry, *id)?;
                        }
                    }
                }
            }
            _ => (),
        }
        let body = packet.encode();
        let mut r = Reader::new(&body);
        r.varint()?;
        self.next.clientbound(
            wire::packet_id(modern::CONFIGURATION_CLIENTBOUND, name).map_err(Error::new)?,
            r.rest(),
            registry,
        )
    }
    pub fn finish(&mut self, registry: &mut Registries) -> Result<Vec<Packet>> {
        let mut packets = Vec::new();
        if self.protocol < 767 {
            for (name, mut entries) in crate::vfp_configuration_1205::defaults(registry)? {
                for entry in &mut entries {
                    if let Some(tag) = &mut entry.data {
                        upgrade(&name, &entry.id, tag)?;
                    }
                }
                packets.push(self.emit(&name, entries, registry)?);
            }
        }
        if self.protocol < 768 {
            let entries = [
                "ponder", "sing", "seek", "feel", "admire", "call", "yearn", "dream",
            ]
            .into_iter()
            .enumerate()
            .map(|(index, name)| cfg::RegistryEntry {
                id: format!("minecraft:{name}_goat_horn"),
                data: Some(Tag::compound([
                    (
                        "sound_event",
                        Tag::String(
                            format!("minecraft:item.goat_horn.sound.{index}")
                                .as_str()
                                .into(),
                        ),
                    ),
                    ("use_duration", Tag::Float(7.0f32.to_bits())),
                    ("range", Tag::Int(256)),
                    ("description", Tag::String("".into())),
                ])),
            })
            .collect();
            packets.push(self.emit("instrument", entries, registry)?);
        }
        if self.protocol <= 769 {
            for (kind, entity, suffix, names) in [
                (
                    "frog_variant",
                    "frog",
                    true,
                    &["temperate", "warm", "cold"][..],
                ),
                (
                    "cat_variant",
                    "cat",
                    false,
                    &[
                        "tabby",
                        "black",
                        "red",
                        "siamese",
                        "british_shorthair",
                        "calico",
                        "persian",
                        "ragdoll",
                        "white",
                        "jellie",
                        "all_black",
                    ][..],
                ),
                ("pig_variant", "pig", true, &["temperate"][..]),
                ("cow_variant", "cow", true, &["temperate"][..]),
                ("chicken_variant", "chicken", true, &["temperate"][..]),
            ] {
                let entries = names
                    .iter()
                    .map(|name| cfg::RegistryEntry {
                        id: format!("minecraft:{name}"),
                        data: Some(Tag::compound([(
                            "asset_id",
                            Tag::String(
                                format!(
                                    "minecraft:entity/{entity}/{name}{}",
                                    if suffix {
                                        format!("_{entity}")
                                    } else {
                                        String::new()
                                    }
                                )
                                .as_str()
                                .into(),
                            ),
                        )])),
                    })
                    .collect();
                packets.push(self.emit(kind, entries, registry)?);
            }
            let fields = [
                ("ambient_sound", "ambient"),
                ("death_sound", "death"),
                ("growl_sound", "growl"),
                ("hurt_sound", "hurt"),
                ("pant_sound", "pant"),
                ("whine_sound", "whine"),
            ]
            .map(|(key, name)| {
                (
                    key,
                    Tag::String(format!("minecraft:entity.wolf.{name}").as_str().into()),
                )
            });
            packets.push(self.emit(
                "wolf_sound_variant",
                vec![cfg::RegistryEntry {
                    id: "minecraft:classic".into(),
                    data: Some(Tag::compound(fields)),
                }],
                registry,
            )?);
        }
        if self.protocol <= 770 {
            let text = |key: &str| Tag::compound([("translate", Tag::String(key.into()))]);
            let dialog = Tag::compound([
                ("type", Tag::String("minecraft:server_links".into())),
                ("title", text("menu.server_links.title")),
                ("external_title", text("menu.server_links")),
                (
                    "exit_action",
                    Tag::compound([("width", Tag::Int(200)), ("label", text("gui.back"))]),
                ),
                ("columns", Tag::Int(1)),
                ("button_width", Tag::Int(310)),
            ]);
            let body = cfg::Clientbound::RegistryData(cfg::RegistryData {
                registry: "minecraft:dialog".into(),
                entries: vec![cfg::RegistryEntry {
                    id: "minecraft:server_links".into(),
                    data: Some(dialog),
                }],
            })
            .encode();
            let mut r = Reader::new(&body);
            r.varint()?;
            packets.push(
                self.next.clientbound(
                    wire::packet_id(modern::CONFIGURATION_CLIENTBOUND, "REGISTRY_DATA")
                        .map_err(Error::new)?,
                    r.rest(),
                    registry,
                )?,
            );
        }
        packets.extend(self.next.finish(registry)?);
        Ok(packets)
    }
    fn emit(
        &mut self,
        registry_name: &str,
        entries: Vec<cfg::RegistryEntry>,
        registry: &mut Registries,
    ) -> Result<Packet> {
        let body = cfg::Clientbound::RegistryData(cfg::RegistryData {
            registry: format!("minecraft:{registry_name}"),
            entries,
        })
        .encode();
        let mut r = Reader::new(&body);
        r.varint()?;
        self.next.clientbound(
            wire::packet_id(modern::CONFIGURATION_CLIENTBOUND, "REGISTRY_DATA")
                .map_err(Error::new)?,
            r.rest(),
            registry,
        )
    }
    pub fn reset(&mut self) {
        self.next.reset();
    }
    pub fn serverbound(&self, body: &[u8]) -> Result<Vec<Vec<u8>>> {
        let mut r = Reader::new(body);
        let id = r.varint()?;
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| modern::CONFIGURATION_SERVERBOUND.get(id))
            .ok_or_else(|| Error::new("Unknown native configuration reply"))?;
        if *name == "ACCEPT_CODE_OF_CONDUCT" {
            if r.remaining() != 0 {
                return Err(Error::new("Trailing code-of-conduct reply"));
            }
            return Ok(Vec::new());
        }
        if self.protocol <= 770 && *name == "CUSTOM_CLICK_ACTION" {
            return Ok(Vec::new());
        }
        let converted = crate::vfp_configuration::serverbound_262(body)?;
        let mut r = Reader::new(&converted);
        r.varint()?;
        let mut output = Vec::new();
        let source = if self.protocol <= 770 {
            crate::vfp_tables_1215::CONFIGURATION_SERVERBOUND
        } else {
            old::CONFIGURATION_SERVERBOUND
        };
        crate::vfp_core::put_varint(
            &mut output,
            wire::packet_id(source, name).map_err(Error::new)?,
        );
        if self.protocol < 768 && *name == "CLIENT_INFORMATION" {
            output.extend_from_slice(&crate::vfp_transition_121::client_information(r.rest())?);
        } else {
            output.extend_from_slice(r.rest());
        }
        Ok(vec![output])
    }
}
pub fn play_tags(protocol: i32, payload: &[u8]) -> Result<Vec<u8>> {
    let source = if protocol <= 770 {
        crate::vfp_tables_1215::CONFIGURATION_CLIENTBOUND
    } else {
        old::CONFIGURATION_CLIENTBOUND
    };
    let id = wire::packet_id(source, "UPDATE_TAGS").map_err(Error::new)?;
    let (_, data) =
        Configuration::new(protocol).clientbound(id, payload, &mut Registries::default())?;
    Ok(data)
}
