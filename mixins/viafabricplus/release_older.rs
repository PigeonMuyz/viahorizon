// SPDX-License-Identifier: GPL-3.0-or-later
// Stateful Release edges before 1.19.3.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::{self as chain, Step},
    vfp_release_codec::{self as codec, Position, Stack},
    vfp_release_rewrite as rewrite,
    vfp_wire::Packet,
};
use horizon_world::nbt::{self, Tag};
use std::{collections::BTreeMap, sync::Arc};

pub struct Adapter {
    step: &'static Step,
    server_protocol: i32,
    chat: crate::vfp_release_chat::Chat,
    entities: BTreeMap<i32, String>,
    dimensions: BTreeMap<String, Tag>,
    active: Option<String>,
    sections: usize,
    biome_bits: u8,
    chat_types: BTreeMap<i32, Tag>,
    health: (f32, i32, f32),
    loom: Option<i32>,
    local: Vec<Packet>,
    light: crate::vfp_release_world::LightCache,
    earlier: crate::vfp_release_pre18::State,
    inventory: crate::vfp_release_inventory::Inventory,
    books: [bool; 8],
    ability_speeds: (f32, f32),
}
impl Adapter {
    pub fn step(&self) -> &'static Step {
        self.step
    }
    pub fn held(&self) -> Option<Option<Stack>> {
        self.inventory.held()
    }
    pub fn username(&mut self, name: &str) {
        self.earlier.username = name.to_owned();
    }
    pub fn new(
        step: &'static Step,
        server_protocol: i32,
        identity: Option<Arc<crate::vfp_chat_signing::Identity>>,
    ) -> Self {
        let mut earlier = crate::vfp_release_pre18::State::default();
        earlier.source(server_protocol);
        Self {
            step,
            server_protocol,
            chat: crate::vfp_release_chat::Chat::new(identity),
            entities: BTreeMap::new(),
            dimensions: BTreeMap::new(),
            active: None,
            sections: 0,
            biome_bits: 0,
            chat_types: BTreeMap::new(),
            health: (20.0, 20, 5.0),
            loom: None,
            local: Vec::new(),
            light: Default::default(),
            earlier,
            inventory: Default::default(),
            books: [false; 8],
            ability_speeds: (0.05, 0.1),
        }
    }
    pub fn take_local(&mut self) -> Vec<Packet> {
        std::mem::take(&mut self.local)
    }
    fn registry(&mut self, registry: &Tag) -> Result<()> {
        self.dimensions.clear();
        self.chat_types.clear();
        if let Some(Tag::List(values)) = registry
            .get("minecraft:dimension_type")
            .and_then(|value| value.get("value"))
        {
            for value in values {
                if let (Some(Tag::String(name)), Some(element)) =
                    (value.get("name"), value.get("element"))
                {
                    self.dimensions
                        .insert(name.to_string_lossy(), element.clone());
                }
            }
        }
        if let Some(Tag::List(values)) = registry
            .get("minecraft:chat_type")
            .and_then(|value| value.get("value"))
        {
            for value in values {
                if let (Some(Tag::Int(id)), Some(element)) = (value.get("id"), value.get("element"))
                {
                    self.chat_types.insert(*id, element.clone());
                }
            }
        }
        let biomes = registry
            .get("minecraft:worldgen/biome")
            .and_then(|value| value.get("value"));
        let Some(Tag::List(values)) = biomes else {
            return Err(Error::new("Legacy join has no biome registry"));
        };
        if values.is_empty() || values.len() > 65536 {
            return Err(Error::new("Invalid legacy biome registry size"));
        }
        self.biome_bits = ((usize::BITS - (values.len() - 1).leading_zeros()) as u8).max(1);
        Ok(())
    }
    fn activate(&mut self, name: String) -> Result<()> {
        let dimension = self
            .dimensions
            .get(&name)
            .ok_or_else(|| Error::new("Legacy join references an unknown dimension type"))?;
        let Some(Tag::Int(height)) = dimension.get("height") else {
            return Err(Error::new("Legacy dimension has no height"));
        };
        if *height <= 0 || *height > 4096 || *height % 16 != 0 {
            return Err(Error::new("Invalid legacy dimension height"));
        }
        self.sections = *height as usize / 16;
        self.active = Some(name);
        Ok(())
    }
    fn dimension_key(&self, element: &Tag) -> Result<String> {
        if let Some((name, _)) = self.dimensions.iter().find(|(_, value)| *value == element) {
            return Ok(name.clone());
        }
        // The world name is deliberately not used as the dimension type key.
        // Plugin dimensions may use a different type with the same height.
        Err(Error::new(
            "Respawn dimension was not declared by the old server",
        ))
    }
    fn join(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let step = self.step;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let player = r.i32()?;
        w.i32(player);
        self.entities.clear();
        self.entities.insert(player, "player".into());
        self.loom = None;
        self.light.clear();
        self.inventory.reset();
        self.earlier.reset_world();
        w.bytes(r.bytes(3)?); // Hardcore, current and previous game modes.
        let worlds = r.count(1024)?;
        w.varint(worlds as i32);
        for _ in 0..worlds {
            w.string(&r.string(32767)?);
        }
        let mut registry = r.vfp_named_nbt()?;
        if step.source == 757 {
            add_infiniburn(&mut registry, true);
        }
        if step.source == 754 {
            add_world_height(&mut registry, true);
        }
        if step.source == 758 {
            add_monster_spawn(&mut registry, true);
            insert_registry(
                &mut registry,
                "minecraft:chat_type",
                bundled(crate::vfp_release_chain_resources::CHAT_REGISTRY_1_19)?,
            )?;
        }
        self.registry(&registry)?;
        // Save 1.19's old chat decoration before replacing the wire registry.
        if step.source == 759 {
            insert_registry(
                &mut registry,
                "minecraft:chat_type",
                bundled(crate::vfp_release_chain_resources::CHAT_REGISTRY_1_19_1)?,
            )?;
        }
        w.vfp_named_nbt(&registry)?;
        if step.source < 759 {
            let mut element = r.vfp_named_nbt()?;
            if step.source == 754 {
                add_world_height(&mut element, false);
            }
            if step.source == 757 {
                add_infiniburn(&mut element, false);
            }
            if step.source == 758 {
                add_monster_spawn(&mut element, false);
            }
            let name = self.dimension_key(&element)?;
            self.activate(name.clone())?;
            if step.target >= 759 {
                w.string(&name);
            } else {
                w.vfp_named_nbt(&element)?;
            }
        } else {
            let name = r.string(32767)?;
            self.activate(name.clone())?;
            w.string(&name);
        }
        if step.source == 756 {
            w.string(&r.string(32767)?);
            w.i64(r.i64()?);
            w.varint(r.varint()?);
            let view = r.varint()?;
            w.varint(view);
            w.varint(view);
            w.bytes(r.rest());
        } else {
            w.bytes(r.rest());
        }
        if step.source == 758 {
            w.bool(false);
        } // No last death location before 1.19.
        Ok(w.data)
    }
    fn respawn(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let step = self.step;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        if step.source < 759 {
            let mut element = r.vfp_named_nbt()?;
            if step.source == 754 {
                add_world_height(&mut element, false);
            }
            if step.source == 757 {
                add_infiniburn(&mut element, false);
            }
            if step.source == 758 {
                add_monster_spawn(&mut element, false);
            }
            let name = self.dimension_key(&element)?;
            self.activate(name.clone())?;
            if step.target >= 759 {
                w.string(&name);
            } else {
                w.vfp_named_nbt(&element)?;
            }
        } else {
            let name = r.string(32767)?;
            self.activate(name.clone())?;
            w.string(&name);
        }
        w.string(&r.string(32767)?);
        w.bytes(r.bytes(8 + 1 + 1 + 1 + 1)?);
        self.light.clear();
        self.earlier.reset_world();
        let keep = codec::boolean(&mut r)?;
        if step.target == 761 {
            w.u8(if keep { 3 } else { 2 });
        } else {
            w.bool(keep);
        }
        w.bytes(r.rest());
        if step.source == 758 {
            w.bool(false);
        }
        self.loom = None;
        Ok(w.data)
    }
    fn join_1161(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let player = r.i32()?;
        w.i32(player);
        self.entities.clear();
        self.entities.insert(player, "player".into());
        self.inventory.reset();
        self.earlier.reset_world();
        self.books = [false; 8];
        let gamemode = r.u8()?;
        w.bool(gamemode & 8 != 0);
        w.u8(gamemode & !8);
        w.i8(r.i8()?);
        let worlds = r.count(1024)?;
        w.varint(worlds as i32);
        for _ in 0..worlds {
            w.string(&r.string(32767)?);
        }
        r.vfp_named_nbt()?;
        let registry = bundled(crate::vfp_release_chain_resources::DIMENSION_REGISTRY_1_16_2)?;
        self.registry(&registry)?;
        w.vfp_named_nbt(&registry)?;
        let kind = r.string(32767)?;
        let element = self
            .dimensions
            .get(&kind)
            .ok_or_else(|| Error::new("Unknown 1.16.1 dimension type"))?;
        w.vfp_named_nbt(element)?;
        w.string(&r.string(32767)?);
        w.i64(r.i64()?);
        w.varint(i32::from(r.u8()?));
        w.bytes(r.rest());
        Ok(w.data)
    }
    fn respawn_1161(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let kind = r.string(32767)?;
        let element = self
            .dimensions
            .get(&kind)
            .ok_or_else(|| Error::new("Undeclared 1.16.1 respawn dimension"))?;
        w.vfp_named_nbt(element)?;
        w.bytes(r.rest());
        self.earlier.reset_world();
        Ok(w.data)
    }
    fn join_1152(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let player = r.i32()?;
        w.i32(player);
        self.entities.clear();
        self.entities.insert(player, "player".into());
        self.inventory.reset();
        self.earlier.reset_world();
        self.books = [false; 8];
        w.u8(r.u8()?);
        w.i8(-1);
        let dimension = r.i32()?;
        w.varint(3);
        for name in [
            "minecraft:overworld",
            "minecraft:the_nether",
            "minecraft:the_end",
        ] {
            w.string(name);
        }
        w.vfp_named_nbt(&dimensions_116()?)?;
        let name = dimension_name(dimension)?;
        w.string(name);
        w.string(name);
        w.i64(r.i64()?);
        w.u8(r.u8()?);
        let kind = r.string(16)?;
        w.varint(r.varint()?);
        w.bool(codec::boolean(&mut r)?);
        w.bool(codec::boolean(&mut r)?);
        w.bool(false);
        w.bool(kind == "flat");
        codec::end(&r)?;
        Ok(w.data)
    }
    fn respawn_1152(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let name = dimension_name(r.i32()?)?;
        w.string(name);
        w.string(name);
        w.i64(r.i64()?);
        w.u8(r.u8()?);
        w.i8(-1);
        let kind = r.string(16)?;
        w.bool(false);
        w.bool(kind == "flat");
        w.bool(true);
        codec::end(&r)?;
        self.earlier.reset_world();
        Ok(w.data)
    }
    fn join_1144(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let player = r.i32()?;
        self.entities.clear();
        self.entities.insert(player, "player".into());
        w.i32(player);
        w.u8(r.u8()?);
        w.i32(r.i32()?);
        w.i64(0);
        w.u8(r.u8()?);
        w.string(&r.string(16)?);
        w.varint(r.varint()?);
        w.bool(codec::boolean(&mut r)?);
        w.bool(true);
        codec::end(&r)?;
        self.inventory.reset();
        self.earlier.reset_world();
        Ok(w.data)
    }
    pub fn clientbound(&mut self, id: i32, data: &[u8]) -> Result<Vec<Packet>> {
        if self.step.source < 754 && !matches!(self.step.source, 498 | 578 | 736) {
            return self.earlier_clientbound(id, data);
        }
        let step = self.step;
        let name = step.incoming_name(id)?;
        let mut target = name;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut prefix = Vec::new();
        match name {
            "LOGIN" => {
                if step.source == 498 {
                    w.bytes(&self.join_1144(data)?);
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                if step.source == 578 {
                    w.bytes(&self.join_1152(data)?);
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                if step.source == 736 {
                    w.bytes(&self.join_1161(data)?);
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                w.bytes(&self.join(data)?);
                if step.source == 760 {
                    let mut features = Writer::new();
                    features.varint(1);
                    features.string("minecraft:vanilla");
                    // Before LOGIN so the pre-1.20.2 bridge includes features
                    // in configuration instead of forwarding them as play.
                    prefix.push(step.incoming("UPDATE_ENABLED_FEATURES", features.data)?);
                }
            }
            "RESPAWN" => {
                if step.source == 498 {
                    w.i32(r.i32()?);
                    w.i64(0);
                    w.u8(r.u8()?);
                    w.string(&r.string(16)?);
                    codec::end(&r)?;
                    self.earlier.reset_world();
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                if step.source == 578 {
                    w.bytes(&self.respawn_1152(data)?);
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                if step.source == 736 {
                    w.bytes(&self.respawn_1161(data)?);
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                if step.source == 760 && self.health.0 <= 0.0 {
                    let mut health = Writer::new();
                    health.f32(20.0);
                    health.varint(self.health.1);
                    health.f32(self.health.2);
                    prefix.push(step.incoming("SET_HEALTH", health.data)?);
                    self.health.0 = 20.0;
                }
                w.bytes(&self.respawn(data)?);
            }
            "PLAYER_CHAT" if step.source == 760 => {
                let Some(chat) = self.chat.player_1191(data)? else {
                    return Ok(Vec::new());
                };
                target = "DISGUISED_CHAT";
                w.bytes(&chat);
            }
            "PLAYER_CHAT" if step.source == 759 => {
                target = "SYSTEM_CHAT";
                w.bytes(&self.player_1190(data)?);
            }
            "SYSTEM_CHAT" if step.source == 759 => {
                w.string(&r.string(262144)?);
                w.bool(r.varint()? == 2);
                codec::end(&r)?;
            }
            "CHAT" if step.source == 758 => {
                target = "SYSTEM_CHAT";
                w.string(&r.string(262144)?);
                let kind = r.i8()?;
                w.varint(if kind == 0 { 1 } else { i32::from(kind) });
                r.uuid()?;
                codec::end(&r)?;
            }
            "BLOCK_BREAK_ACK" | "ADD_VIBRATION_SIGNAL" if step.source == 758 => {
                return Ok(Vec::new());
            }
            "DELETE_CHAT" | "PLAYER_CHAT_HEADER" | "CHAT_PREVIEW" | "SET_DISPLAY_CHAT_PREVIEW"
                if step.source == 760 =>
            {
                return Ok(Vec::new());
            }
            "PLAYER_INFO" if step.source >= 758 => {
                return self.player_info(data);
            }
            "SERVER_DATA" => {
                codec::optional_text(&mut r, &mut w)?;
                codec::optional_text(&mut r, &mut w)?;
                let previews = codec::boolean(&mut r)?;
                if step.source == 759 {
                    w.bool(previews);
                    w.bool(false);
                } else if step.source == 760 {
                    w.bool(codec::boolean(&mut r)?);
                } else {
                    w.bool(previews);
                }
                codec::end(&r)?;
            }
            "LEVEL_CHUNK_WITH_LIGHT" => {
                w.bytes(&rewrite::chunk_118(
                    step,
                    data,
                    self.sections,
                    self.biome_bits,
                )?);
            }
            "LEVEL_CHUNK" if matches!(step.source, 498 | 578 | 736 | 754) => {
                return self.earlier_clientbound(id, data);
            }
            "LEVEL_CHUNK" if step.source == 756 => {
                target = "LEVEL_CHUNK_WITH_LIGHT";
                w.bytes(&crate::vfp_release_world::chunk_117(
                    step,
                    data,
                    self.sections,
                    self.biome_bits,
                    &mut self.light,
                )?);
            }
            "LIGHT_UPDATE" if step.source == 756 => {
                if !self.light.update(data)? {
                    return Ok(Vec::new());
                }
                w.bytes(data);
            }
            "FORGET_LEVEL_CHUNK" if step.source == 756 => {
                let x = r.i32()?;
                let z = r.i32()?;
                codec::end(&r)?;
                self.light.forget(x, z);
                w.bytes(data);
            }
            "REMOVE_ENTITY" if step.source == 755 => {
                target = "REMOVE_ENTITIES";
                w.varint(1);
                w.varint(r.varint()?);
                codec::end(&r)?;
            }
            "SET_ENTITY_DATA" => {
                let mut peek = Reader::new(data);
                let entity = peek.varint()?;
                let entity = self.entities.get(&entity).map_or("", String::as_str);
                w.bytes(&rewrite::metadata_119(step, data, entity)?);
            }
            "ADD_ENTITY" | "ADD_MOB" => {
                if step.source == 758 && name == "ADD_MOB" {
                    target = "ADD_ENTITY";
                }
                let entity = r.varint()?;
                w.varint(entity);
                w.bytes(r.bytes(16)?);
                let kind = r.varint()?;
                let entity_name = chain::identifier(step.source, "entities", kind)?;
                self.entities.insert(entity, entity_name.into());
                w.varint(step.same_or_map("entities", kind, false)?);
                w.bytes(r.bytes(24)?);
                if step.source == 758 {
                    if name == "ADD_MOB" {
                        let yaw = r.i8()?;
                        let pitch = r.i8()?;
                        w.i8(pitch);
                        w.i8(yaw);
                        w.i8(r.i8()?);
                        w.varint(0);
                    } else {
                        w.i8(r.i8()?);
                        let yaw = r.i8()?;
                        w.i8(yaw);
                        w.i8(yaw);
                        let value = r.i32()?;
                        w.varint(if entity_name == "falling_block" {
                            step.same_or_map("blockstates", value, false)?
                        } else {
                            value
                        });
                    }
                    w.bytes(r.bytes(6)?);
                    codec::end(&r)?;
                } else if step.source < 758 {
                    if name == "ADD_MOB" {
                        if step.source == 498 {
                            w.bytes(r.bytes(3 + 6)?);
                            let mut metadata = Writer::new();
                            metadata.varint(entity);
                            metadata.bytes(r.rest());
                            return Ok(vec![
                                step.incoming(target, w.data)?,
                                step.incoming(
                                    "SET_ENTITY_DATA",
                                    rewrite::metadata_119(step, &metadata.data, entity_name)?,
                                )?,
                            ]);
                        } else {
                            w.bytes(r.rest());
                        }
                    } else {
                        w.bytes(r.bytes(2)?);
                        let value = r.i32()?;
                        w.i32(if entity_name == "falling_block" {
                            step.same_or_map("blockstates", value, false)?
                        } else {
                            value
                        });
                        w.bytes(r.rest());
                    }
                } else {
                    w.bytes(r.bytes(3)?);
                    let value = r.varint()?;
                    w.varint(if entity_name == "falling_block" {
                        step.same_or_map("blockstates", value, false)?
                    } else {
                        value
                    });
                    w.bytes(r.rest());
                }
            }
            "ADD_PAINTING" if step.source == 758 => {
                return self.painting(data);
            }
            "ADD_GLOBAL_ENTITY" if step.source == 578 => {
                target = "ADD_ENTITY";
                let entity = r.varint()?;
                w.varint(entity);
                let kind = r.u8()?;
                if kind != 1 {
                    return Err(Error::new("Unknown old global entity type"));
                }
                self.entities.insert(entity, "lightning_bolt".into());
                let mut uuid = [0; 16];
                crate::crypto::random_bytes(&mut uuid)?;
                uuid[6] = (uuid[6] & 15) | 64;
                uuid[8] = (uuid[8] & 63) | 128;
                w.bytes(&uuid);
                w.varint(chain::named_id(step.target, "entities", "lightning_bolt")?);
                w.bytes(r.bytes(24)?);
                w.bytes(&[0; 2]);
                w.i32(0);
                w.bytes(&[0; 6]);
                codec::end(&r)?;
            }
            "ADD_PLAYER" => {
                let entity = r.varint()?;
                self.entities.insert(entity, "player".into());
                if step.source == 498 {
                    w.varint(entity);
                    w.bytes(r.bytes(16 + 24 + 2)?);
                    let mut metadata = Writer::new();
                    metadata.varint(entity);
                    metadata.bytes(r.rest());
                    return Ok(vec![
                        step.incoming(name, w.data)?,
                        step.incoming(
                            "SET_ENTITY_DATA",
                            rewrite::metadata_119(step, &metadata.data, "player")?,
                        )?,
                    ]);
                } else {
                    w.bytes(data);
                }
            }
            "REMOVE_ENTITIES" => {
                let count = r.count(65536)?;
                let mut removed = Vec::new();
                for _ in 0..count {
                    let id = r.varint()?;
                    self.entities.remove(&id);
                    removed.push(id);
                }
                codec::end(&r)?;
                if step.source == 754 {
                    let mut result = Vec::new();
                    for id in removed {
                        let mut w = Writer::new();
                        w.varint(id);
                        result.push(step.incoming("REMOVE_ENTITY", w.data)?);
                    }
                    return Ok(result);
                }
                w.bytes(data);
            }
            "SOUND" | "SOUND_ENTITY" => {
                let sound = step.same_or_map("sounds", r.varint()?, false)?;
                w.varint(if step.target == 761 { sound + 1 } else { sound });
                w.bytes(r.rest());
                if step.source == 758 {
                    random_seed(&mut w)?;
                }
            }
            "CUSTOM_SOUND" => {
                if step.target == 761 {
                    target = "SOUND";
                    w.varint(0);
                    w.string(&r.string(32767)?);
                    w.bool(false);
                } else {
                    w.string(&r.string(32767)?);
                }
                w.bytes(r.rest());
                if step.source == 758 {
                    random_seed(&mut w)?;
                }
            }
            "LEVEL_EVENT" => {
                let event = r.i32()?;
                let position = Position::unpack(r.i64()?, step.source);
                let value = r.i32()?;
                let global = codec::boolean(&mut r)?;
                codec::end(&r)?;
                if step.target == 761 {
                    if let Some(sound) = door_sound(event) {
                        target = "SOUND";
                        w.varint(sound + 1);
                        w.varint(4);
                        for coordinate in [position.x, position.y, position.z] {
                            w.i32(
                                coordinate
                                    .checked_mul(8)
                                    .and_then(|x| x.checked_add(4))
                                    .ok_or_else(|| Error::new("Old sound coordinate overflow"))?,
                            );
                        }
                        w.f32(1.0);
                        w.f32(0.95);
                        random_seed(&mut w)?;
                    } else {
                        level_event(step, &mut w, event, position, value, global)?;
                    }
                } else {
                    level_event(step, &mut w, event, position, value, global)?;
                }
            }
            "BLOCK_UPDATE" => {
                w.i64(r.i64()?);
                w.varint(step.same_or_map("blockstates", r.varint()?, false)?);
                codec::end(&r)?;
            }
            "BLOCK_EVENT" => {
                w.bytes(r.bytes(10)?);
                w.varint(step.same_or_map("blocks", r.varint()?, false)?);
                codec::end(&r)?;
            }
            "CHUNK_BLOCKS_UPDATE" if step.source == 736 => {
                let x = i64::from(r.i32()?) & 0x3f_ffff;
                let z = i64::from(r.i32()?) & 0x3f_ffff;
                let mut sections: [Vec<i64>; 16] = std::array::from_fn(|_| Vec::new());
                for _ in 0..r.count(65536)? {
                    let position = r.u16()?;
                    let state = i64::from(step.map("blockstates", r.varint()?)?);
                    let section = usize::from((position >> 4) & 15);
                    let local = i64::from(((position >> 8) << 4) | (position & 15));
                    sections[section].push((state << 12) | local);
                }
                codec::end(&r)?;
                for (y, entries) in sections.into_iter().enumerate() {
                    if entries.is_empty() {
                        continue;
                    }
                    let mut section = Writer::new();
                    section.i64((x << 42) | (z << 20) | y as i64);
                    section.bool(false);
                    section.varint(entries.len() as i32);
                    for entry in entries {
                        section.varlong(entry);
                    }
                    prefix.push(step.incoming("SECTION_BLOCKS_UPDATE", section.data)?);
                }
                return Ok(prefix);
            }
            "CHUNK_BLOCKS_UPDATE" => {
                rewrite::chunk_blocks(step, &mut r, &mut w)?;
                codec::end(&r)?;
            }
            "SECTION_BLOCKS_UPDATE" => {
                w.i64(r.i64()?);
                w.bool(codec::boolean(&mut r)?);
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    let entry = r.varlong()?;
                    if entry < 0 {
                        return Err(Error::new("Negative old section update"));
                    }
                    let state = i32::try_from(entry >> 12)
                        .map_err(|_| Error::new("Old section update state overflow"))?;
                    w.varlong(
                        (i64::from(step.same_or_map("blockstates", state, false)?) << 12)
                            | (entry & 4095),
                    );
                }
                codec::end(&r)?;
            }
            "BLOCK_ENTITY_DATA" => {
                if step.source <= 755 {
                    w.bytes(data);
                } else if step.source == 756 {
                    w.bytes(&crate::vfp_release_world::block_entity_117(data)?);
                } else {
                    w.i64(r.i64()?);
                    w.varint(step.same_or_map("blockentities", r.varint()?, false)?);
                    w.vfp_named_nbt(&r.vfp_named_nbt()?)?;
                    codec::end(&r)?;
                }
            }
            "CONTAINER_SET_SLOT" => {
                if matches!(step.source, 498 | 578 | 736) {
                    w.u8(r.u8()?);
                    w.i16(r.i16()?);
                    rewrite::item(step, false, &mut r, &mut w)?;
                    codec::end(&r)?;
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                if step.source == 754 {
                    w.bytes(&self.inventory.slot(step, data)?);
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                w.u8(r.u8()?);
                w.varint(if step.source == 755 { 0 } else { r.varint()? });
                w.i16(r.i16()?);
                rewrite::item(step, false, &mut r, &mut w)?;
                codec::end(&r)?;
            }
            "CONTAINER_SET_CONTENT" => {
                if matches!(step.source, 498 | 578 | 736) {
                    w.u8(r.u8()?);
                    let count = r.i16()?;
                    if !(0..=4096).contains(&count) {
                        return Err(Error::new("Invalid 1.16.1 container length"));
                    }
                    w.i16(count);
                    for _ in 0..count {
                        rewrite::item(step, false, &mut r, &mut w)?;
                    }
                    codec::end(&r)?;
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                if step.source == 754 {
                    w.bytes(&self.inventory.content(step, data)?);
                    return Ok(vec![step.incoming(name, w.data)?]);
                }
                w.u8(r.u8()?);
                w.varint(if step.source == 755 { 0 } else { r.varint()? });
                let count = if step.source == 755 {
                    usize::try_from(r.i16()?)
                        .map_err(|_| Error::new("Negative old container size"))?
                } else {
                    r.count(4096)?
                };
                if count > 4096 {
                    return Err(Error::new("Old container exceeds slot bound"));
                }
                w.varint(count as i32);
                for _ in 0..count {
                    rewrite::item(step, false, &mut r, &mut w)?;
                }
                if step.source == 755 {
                    codec::Stack::write(None, &mut w, step.target)?;
                } else {
                    rewrite::item(step, false, &mut r, &mut w)?;
                }
                codec::end(&r)?;
            }
            "SET_EQUIPMENT" => {
                w.varint(r.varint()?);
                let mut count = 0;
                loop {
                    let slot = r.u8()?;
                    w.u8(slot);
                    count += 1;
                    if count > 16 {
                        return Err(Error::new("Too many old equipment entries"));
                    }
                    rewrite::item(step, false, &mut r, &mut w)?;
                    if slot & 128 == 0 {
                        break;
                    }
                }
                codec::end(&r)?;
            }
            "SET_EQUIPPED_ITEM" if step.source == 498 || step.source == 578 => {
                if step.source == 578 {
                    target = "SET_EQUIPMENT";
                }
                w.varint(r.varint()?);
                let slot = r.varint()?;
                if !(0..=5).contains(&slot) {
                    return Err(Error::new("Invalid old equipment slot"));
                }
                if step.source == 578 {
                    w.u8(slot as u8);
                } else {
                    w.varint(slot);
                }
                rewrite::item(step, false, &mut r, &mut w)?;
                codec::end(&r)?;
            }
            "MERCHANT_OFFERS" => {
                w.varint(r.varint()?);
                let count = if step.source < 759 {
                    usize::from(r.u8()?)
                } else {
                    r.count(1024)?
                };
                if step.target >= 759 {
                    w.varint(count as i32);
                } else {
                    w.u8(count as u8);
                }
                for _ in 0..count {
                    rewrite::item(step, false, &mut r, &mut w)?;
                    rewrite::item(step, false, &mut r, &mut w)?;
                    if step.source < 759 {
                        let second = codec::boolean(&mut r)?;
                        if step.target < 759 {
                            w.bool(second);
                        }
                        if second {
                            rewrite::item(step, false, &mut r, &mut w)?;
                        } else if step.target >= 759 {
                            codec::Stack::write(None, &mut w, step.target)?;
                        }
                    } else {
                        rewrite::item(step, false, &mut r, &mut w)?;
                    }
                    w.bytes(r.bytes(1 + 4 * 6)?);
                }
                w.bytes(r.rest());
            }
            "UPDATE_RECIPES" => {
                w.bytes(&rewrite::recipes(step, data, step.target == 761)?);
            }
            "UPDATE_TAGS" => {
                w.bytes(&if matches!(step.source, 498 | 578 | 736 | 754) {
                    rewrite::tags_1164(step, data)?
                } else {
                    rewrite::tags(step, data)?
                });
            }
            "LEVEL_PARTICLES" => {
                rewrite::particle(step, &mut r, &mut w, true)?;
                codec::end(&r)?;
            }
            "COMMANDS" => {
                w.bytes(&rewrite::commands(step, data)?);
            }
            "COOLDOWN" => {
                w.varint(step.same_or_map("items", r.varint()?, false)?);
                w.varint(r.varint()?);
                codec::end(&r)?;
            }
            "RECIPE" if step.source == 736 => {
                w.varint(r.varint()?);
                for index in 0..4 {
                    self.books[index] = codec::boolean(&mut r)?;
                    w.bool(self.books[index]);
                }
                for index in 4..8 {
                    self.books[index] = false;
                    w.bool(false);
                }
                w.bytes(r.rest());
            }
            "UPDATE_ADVANCEMENTS" => {
                w.bytes(&rewrite::advancements(step, data)?);
            }
            "EXPLODE" if step.target == 761 => {
                for _ in 0..3 {
                    w.f64(f64::from(r.f32()?));
                }
                w.bytes(r.rest());
            }
            "EXPLODE" if step.source == 754 => {
                w.bytes(r.bytes(16)?);
                let count = r.i32()?;
                if !(0..=65536).contains(&count) {
                    return Err(Error::new("Invalid old explosion block count"));
                }
                w.varint(count);
                w.bytes(r.rest());
            }
            "UPDATE_MOB_EFFECT" if step.source == 758 => {
                w.bytes(data);
                w.bool(false);
            }
            "UPDATE_MOB_EFFECT" | "REMOVE_MOB_EFFECT" if step.source == 757 => {
                w.varint(r.varint()?);
                w.varint(i32::from(r.i8()?));
                w.bytes(r.rest());
            }
            "SET_HEALTH" => {
                self.health = (r.f32()?, r.varint()?, r.f32()?);
                codec::end(&r)?;
                w.bytes(data);
            }
            "OPEN_SCREEN" => {
                let window = r.varint()?;
                let kind = r.varint()?;
                if step.source == 754 {
                    if let Ok(window) = u8::try_from(window) {
                        self.inventory.open(window, Some(kind));
                    }
                }
                self.loom = (kind == 17).then_some(window);
                if step.source == 578 {
                    w.varint(window);
                    w.varint(if kind >= 20 {
                        kind.checked_add(1)
                            .ok_or_else(|| Error::new("Old menu ID overflow"))?
                    } else {
                        kind
                    });
                    w.bytes(r.rest());
                } else {
                    w.bytes(data);
                }
            }
            "HORSE_SCREEN_OPEN" if step.source == 754 => {
                self.inventory.open(r.u8()?, None);
                w.bytes(data);
            }
            "SET_CARRIED_ITEM" if step.source == 754 => {
                self.inventory.select(i16::from(r.u8()?));
                codec::end(&r)?;
                w.bytes(data);
            }
            "CONTAINER_CLOSE" => {
                self.loom = None;
                if step.source == 754 {
                    self.inventory.close(r.u8()?);
                }
                w.bytes(data);
            }
            "CONTAINER_ACK" if step.source == 754 => {
                return self.inventory.acknowledge(step, data);
            }
            "SET_BORDER" if step.source == 754 => {
                target = match r.varint()? {
                    0 => "SET_BORDER_SIZE",
                    1 => "SET_BORDER_LERP_SIZE",
                    2 => "SET_BORDER_CENTER",
                    3 => "INITIALIZE_BORDER",
                    4 => "SET_BORDER_WARNING_DELAY",
                    5 => "SET_BORDER_WARNING_DISTANCE",
                    _ => return Err(Error::new("Unknown old border action")),
                };
                w.bytes(r.rest());
            }
            "SET_TITLES" if step.source == 754 => {
                let action = r.varint()?;
                target = match action {
                    0 => "SET_TITLE_TEXT",
                    1 => "SET_SUBTITLE_TEXT",
                    2 => "SET_ACTION_BAR_TEXT",
                    3 => "SET_TITLES_ANIMATION",
                    4 | 5 => "CLEAR_TITLES",
                    _ => return Err(Error::new("Unknown old title action")),
                };
                if action >= 4 {
                    w.bool(action == 5);
                    codec::end(&r)?;
                } else {
                    w.bytes(r.rest());
                }
            }
            "PLAYER_COMBAT" if step.source == 754 => {
                target = match r.varint()? {
                    0 => "PLAYER_COMBAT_ENTER",
                    1 => "PLAYER_COMBAT_END",
                    2 => "PLAYER_COMBAT_KILL",
                    _ => return Err(Error::new("Unknown old combat action")),
                };
                w.bytes(r.rest());
            }
            "PLAYER_POSITION" if step.source == 754 => {
                w.bytes(data);
                w.bool(false);
            }
            "SET_DEFAULT_SPAWN_POSITION" if step.source == 754 => {
                w.bytes(data);
                w.f32(0.0);
            }
            "UPDATE_ATTRIBUTES" if step.source == 754 => {
                w.varint(r.varint()?);
                let count = r.i32()?;
                if !(0..=1024).contains(&count) {
                    return Err(Error::new("Invalid old attribute collection size"));
                }
                w.varint(count);
                w.bytes(r.rest());
            }
            "UPDATE_ATTRIBUTES" if step.source == 578 => {
                w.varint(r.varint()?);
                let count = r.i32()?;
                if !(0..=1024).contains(&count) {
                    return Err(Error::new("Invalid legacy attribute count"));
                }
                w.i32(count);
                for _ in 0..count {
                    let key = r.string(64)?;
                    w.string(&attribute_name(&key));
                    w.f64(r.f64()?);
                    let modifiers = r.count(1024)?;
                    w.varint(modifiers as i32);
                    w.bytes(r.bytes(modifiers * 25)?);
                }
                codec::end(&r)?;
            }
            "PLAYER_ABILITIES" if step.source == 578 => {
                w.u8(r.u8()?);
                self.ability_speeds = (r.f32()?, r.f32()?);
                w.f32(self.ability_speeds.0);
                w.f32(self.ability_speeds.1);
                codec::end(&r)?;
            }
            "CHAT" if step.source == 578 => {
                w.bytes(data);
                w.bytes(&[0; 16]);
            }
            "CONTAINER_SET_DATA" if step.source == 578 => {
                w.u8(r.u8()?);
                let property = r.i16()?;
                w.i16(property);
                let value = r.i16()?;
                w.i16(if (4..=6).contains(&property) && value >= 11 {
                    value
                        .checked_add(1)
                        .ok_or_else(|| Error::new("Old enchantment ID overflow"))?
                } else {
                    value
                });
                codec::end(&r)?;
            }
            "RESOURCE_PACK" if step.source == 754 => {
                w.string(&r.string(32767)?);
                w.string(&r.string(40)?);
                codec::end(&r)?;
                w.bool(false);
                w.bool(false);
            }
            "MAP_ITEM_DATA" if step.source == 754 => {
                w.varint(r.varint()?);
                w.i8(r.i8()?);
                codec::boolean(&mut r)?;
                w.bool(codec::boolean(&mut r)?);
                let icons = r.count(65536)?;
                w.bool(icons != 0);
                if icons != 0 {
                    w.varint(icons as i32);
                }
                w.bytes(r.rest());
            }
            "LIGHT_UPDATE" if step.source == 754 => {
                w.bytes(&crate::vfp_release_world::light_1164(data)?);
            }
            "LIGHT_UPDATE" if step.source == 578 => {
                w.varint(r.varint()?);
                w.varint(r.varint()?);
                w.bool(true);
                w.bytes(r.rest());
            }
            "FORGET_LEVEL_CHUNK" if step.source == 754 => {
                self.earlier.forget(data)?;
                w.bytes(data);
            }
            "CONTAINER_SET_DATA" if step.source == 758 => {
                let window = r.u8()?;
                let property = r.i16()?;
                let value = r.i16()?;
                codec::end(&r)?;
                w.u8(window);
                w.i16(property);
                w.i16(if self.loom == Some(i32::from(window)) && property == 0 {
                    if value == 0 {
                        -1
                    } else if value <= 34 {
                        value - 1
                    } else {
                        0
                    }
                } else {
                    value
                });
            }
            "AWARD_STATS" => {
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    let category = r.varint()?;
                    w.varint(category);
                    let value = r.varint()?;
                    let registry = match category {
                        0 => Some("blocks"),
                        1..=5 => Some("items"),
                        6..=7 => Some("entities"),
                        8 => None,
                        _ => return Err(Error::new("Invalid old statistic category")),
                    };
                    w.varint(if let Some(registry) = registry {
                        step.same_or_map(registry, value, false)?
                    } else {
                        value
                    });
                    w.varint(r.varint()?);
                }
                codec::end(&r)?;
            }
            // The remaining packets have the same field layout and contain no
            // static registry IDs at these four specific protocol edges.
            _ => {
                w.bytes(data);
            }
        }
        prefix.push(step.incoming(target, w.data)?);
        Ok(prefix)
    }
    fn player_info(&mut self, data: &[u8]) -> Result<Vec<Packet>> {
        let step = self.step;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let action = r.varint()?;
        if !(0..=4).contains(&action) {
            return Err(Error::new("Invalid old player info action"));
        }
        let count = r.count(65536)?;
        let target = if step.target == 761 {
            if action == 4 {
                "PLAYER_INFO_REMOVE"
            } else {
                w.u8(if action == 0 {
                    63
                } else {
                    1 << if action == 1 { 2 } else { action + 2 }
                });
                "PLAYER_INFO_UPDATE"
            }
        } else {
            w.varint(action);
            "PLAYER_INFO"
        };
        w.varint(count as i32);
        for _ in 0..count {
            w.bytes(r.bytes(16)?);
            match action {
                0 => {
                    w.string(&r.string(16)?);
                    codec::properties(&mut r, &mut w)?;
                    let mode = r.varint()?;
                    let ping = r.varint()?;
                    let display = if codec::boolean(&mut r)? {
                        Some(r.string(262144)?)
                    } else {
                        None
                    };
                    if step.target == 761 {
                        codec::skip_profile_key(&mut r)?;
                        w.bool(false);
                    }
                    w.varint(mode);
                    if step.target == 761 {
                        w.bool(true);
                    }
                    w.varint(ping);
                    w.bool(display.is_some());
                    if let Some(display) = display {
                        w.string(&display);
                    }
                    if step.source == 758 {
                        w.bool(false);
                    } else if step.target < 761 {
                        let start = r.position();
                        codec::skip_profile_key(&mut r)?;
                        w.bytes(r.since(start));
                    }
                }
                1 | 2 => {
                    w.varint(r.varint()?);
                }
                3 => {
                    codec::optional_text(&mut r, &mut w)?;
                }
                _ => (),
            }
        }
        codec::end(&r)?;
        Ok(vec![step.incoming(target, w.data)?])
    }
    fn player_1190(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let signed: serde_json::Value = serde_json::from_str(&r.string(262144)?)
            .map_err(|_| Error::new("Invalid old chat component"))?;
        let content = if codec::boolean(&mut r)? {
            serde_json::from_str(&r.string(262144)?)
                .map_err(|_| Error::new("Invalid old unsigned chat component"))?
        } else {
            signed
        };
        let kind = r.varint()?;
        r.uuid()?;
        let sender: serde_json::Value = serde_json::from_str(&r.string(262144)?)
            .map_err(|_| Error::new("Invalid old chat sender"))?;
        let target: Option<serde_json::Value> = if codec::boolean(&mut r)? {
            Some(
                serde_json::from_str(&r.string(262144)?)
                    .map_err(|_| Error::new("Invalid old chat target"))?,
            )
        } else {
            None
        };
        r.i64()?;
        r.i64()?;
        r.byte_array(4096)?;
        codec::end(&r)?;
        let kind = self
            .chat_types
            .get(&kind)
            .ok_or_else(|| Error::new("Undeclared old chat type"))?;
        let (decoration, overlay) = if let Some(chat) = kind.get("chat") {
            (chat, false)
        } else if let Some(overlay) = kind.get("overlay") {
            (overlay, true)
        } else {
            return Err(Error::new("Unsupported old chat decoration"));
        };
        let decoration = decoration.get("decoration").unwrap_or(decoration);
        let Some(Tag::String(key)) = decoration.get("translation_key") else {
            w.string(&content.to_string());
            w.bool(overlay);
            return Ok(w.data);
        };
        let mut args = Vec::new();
        if let Some(Tag::List(parameters)) = decoration.get("parameters") {
            for parameter in parameters {
                let Tag::String(parameter) = parameter else {
                    return Err(Error::new("Invalid old chat decoration parameter"));
                };
                args.push(match parameter.to_string_lossy().as_str() {
                    "sender" => sender.clone(),
                    "content" => content.clone(),
                    "target" | "team_name" => target.clone().unwrap_or(serde_json::Value::Null),
                    _ => return Err(Error::new("Unknown old chat decoration parameter")),
                });
            }
        }
        let mut result = serde_json::json!({"translate":key.to_string_lossy(),"with":args});
        if let Some(Tag::Compound(style)) = decoration.get("style") {
            for (name, value) in style {
                match value {
                    Tag::String(value) => {
                        result[name.to_string_lossy()] = value.to_string_lossy().into();
                    }
                    Tag::Byte(value) => {
                        result[name.to_string_lossy()] = (*value != 0).into();
                    }
                    _ => (),
                }
            }
        }
        w.string(&result.to_string());
        w.bool(overlay);
        Ok(w.data)
    }
    fn painting(&mut self, data: &[u8]) -> Result<Vec<Packet>> {
        let step = self.step;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let entity = r.varint()?;
        w.varint(entity);
        w.bytes(r.bytes(16)?);
        let motive = r.varint()?;
        let position = Position::unpack(r.i64()?, step.source);
        let direction = r.i8()?;
        codec::end(&r)?;
        self.entities.insert(entity, "painting".into());
        w.varint(chain::named_id(step.target, "entities", "painting")?);
        for coordinate in [position.x, position.y, position.z] {
            w.f64(f64::from(coordinate) + 0.5);
        }
        w.bytes(&[0; 3]);
        w.varint(match direction {
            -1 => 1,
            2 => 2,
            0 => 3,
            1 => 4,
            3 => 5,
            _ => return Err(Error::new("Invalid old painting direction")),
        });
        w.bytes(&[0; 6]);
        let mut metadata = Writer::new();
        metadata.varint(entity);
        metadata.u8(8);
        metadata.varint(22);
        metadata.varint(step.same_or_map("paintings", motive, false)?);
        metadata.u8(255);
        Ok(vec![
            step.incoming("ADD_ENTITY", w.data)?,
            step.incoming("SET_ENTITY_DATA", metadata.data)?,
        ])
    }
    pub fn serverbound(
        &mut self,
        packets: Vec<Packet>,
        held: Option<&Option<Stack>>,
    ) -> Result<Vec<Packet>> {
        if self.step.source < 754 && !matches!(self.step.source, 498 | 578 | 736) {
            return self.earlier.serverbound(self.step, packets, held);
        }
        let step = self.step;
        let mut result = Vec::new();
        if step.source == 760 {
            if let Some(ack) = self.chat.acknowledgement() {
                result.push(step.outgoing("CHAT_ACK", ack)?);
            }
        }
        for (id, data) in packets {
            let name = step.outgoing_name(id)?;
            let mut target = name;
            let mut r = Reader::new(&data);
            let mut w = Writer::new();
            match name {
                "CHAT_SESSION_UPDATE" | "CHAT_ACK" if step.source == 760 => {
                    continue;
                }
                "CHAT_ACK" if step.source == 759 => {
                    continue;
                }
                "CHAT" | "CHAT_COMMAND" if step.source == 760 => {
                    w.bytes(&self.chat.outgoing_1193(&data, name == "CHAT_COMMAND")?);
                }
                "CHAT" | "CHAT_COMMAND" if step.source == 759 => {
                    w.bytes(&self.chat.outgoing_1191(&data, name == "CHAT_COMMAND")?);
                }
                "CHAT" | "CHAT_COMMAND" if step.source == 758 => {
                    target = "CHAT";
                    let text = r.string(32767)?;
                    w.string(&if name == "CHAT_COMMAND" {
                        format!("/{text}")
                    } else {
                        text
                    });
                    r.i64()?;
                    r.i64()?;
                    if name == "CHAT_COMMAND" {
                        let args = r.count(8)?;
                        for _ in 0..args {
                            r.string(32767)?;
                            r.byte_array(4096)?;
                        }
                    } else {
                        r.byte_array(4096)?;
                    }
                    codec::boolean(&mut r)?;
                    codec::end(&r)?;
                }
                "CHAT_PREVIEW" if step.source == 758 => {
                    continue;
                }
                "SET_BEACON" if step.source == 758 => {
                    for _ in 0..2 {
                        w.varint(if codec::boolean(&mut r)? {
                            r.varint()?
                        } else {
                            -1
                        });
                    }
                    codec::end(&r)?;
                }
                "SET_CREATIVE_MODE_SLOT" => {
                    let Some(data) = rewrite::creative(step, &data)? else {
                        continue;
                    };
                    w.bytes(&data);
                }
                "CONTAINER_CLICK" => {
                    if matches!(step.source, 498 | 578 | 736) {
                        w.u8(r.u8()?);
                        w.i16(r.i16()?);
                        w.i8(r.i8()?);
                        w.i16(r.i16()?);
                        w.varint(r.varint()?);
                        rewrite::item(step, true, &mut r, &mut w)?;
                        codec::end(&r)?;
                        result.push(step.outgoing(name, w.data)?);
                        continue;
                    }
                    if step.source == 754 {
                        let Some(data) = self.inventory.click(step, &data, self.server_protocol)?
                        else {
                            continue;
                        };
                        w.bytes(&data);
                        result.push(step.outgoing(name, w.data)?);
                        continue;
                    }
                    w.u8(r.u8()?);
                    let state = r.varint()?;
                    if step.source != 755 {
                        w.varint(state);
                    }
                    w.i16(r.i16()?);
                    w.i8(r.i8()?);
                    w.varint(r.varint()?);
                    let changed = r.count(4096)?;
                    w.varint(changed as i32);
                    for _ in 0..changed {
                        w.i16(r.i16()?);
                        rewrite::item(step, true, &mut r, &mut w)?;
                    }
                    rewrite::item(step, true, &mut r, &mut w)?;
                    codec::end(&r)?;
                }
                "CLIENT_INFORMATION" if step.source == 756 => {
                    w.string(&r.string(16)?);
                    w.i8(r.i8()?);
                    w.varint(r.varint()?);
                    w.bool(codec::boolean(&mut r)?);
                    w.u8(r.u8()?);
                    w.varint(r.varint()?);
                    w.bool(codec::boolean(&mut r)?);
                    codec::boolean(&mut r)?;
                    codec::end(&r)?;
                }
                "CLIENT_INFORMATION" if step.source == 754 => {
                    w.string(&r.string(16)?);
                    w.i8(r.i8()?);
                    w.varint(r.varint()?);
                    w.bool(codec::boolean(&mut r)?);
                    w.u8(r.u8()?);
                    w.varint(r.varint()?);
                    codec::boolean(&mut r)?;
                    codec::end(&r)?;
                }
                "PONG" if step.source == 754 => {
                    if let Some(packet) = self.inventory.pong(step, &data)? {
                        result.push(packet);
                    }
                    continue;
                }
                "EDIT_BOOK" if matches!(step.source, 498 | 578 | 736 | 754) => {
                    rewrite::item(step, true, &mut r, &mut w)?;
                    w.bytes(r.rest());
                }
                "RECIPE_BOOK_CHANGE_SETTINGS" if step.source == 736 => {
                    let book = r.count(3)?;
                    self.books[book * 2] = codec::boolean(&mut r)?;
                    self.books[book * 2 + 1] = codec::boolean(&mut r)?;
                    codec::end(&r)?;
                    target = "RECIPE_BOOK_UPDATE";
                    w.varint(1);
                    for value in self.books {
                        w.bool(value);
                    }
                }
                "RECIPE_BOOK_SEEN_RECIPE" if step.source == 736 => {
                    target = "RECIPE_BOOK_UPDATE";
                    w.varint(0);
                    w.string(&r.string(32767)?);
                    codec::end(&r)?;
                }
                "INTERACT" if step.source == 578 => {
                    w.varint(r.varint()?);
                    let action = r.varint()?;
                    w.varint(action);
                    if action == 2 {
                        w.bytes(r.bytes(12)?);
                    }
                    if action == 0 || action == 2 {
                        w.varint(r.varint()?);
                    }
                    codec::boolean(&mut r)?;
                    codec::end(&r)?;
                }
                "PLAYER_ABILITIES" if step.source == 578 => {
                    w.u8(r.u8()?);
                    w.f32(self.ability_speeds.0);
                    w.f32(self.ability_speeds.1);
                    codec::end(&r)?;
                }
                "JIGSAW_GENERATE" | "SET_JIGSAW_BLOCK" if step.source == 578 => {
                    continue;
                }
                "EDIT_BOOK" if step.source == 755 => {
                    let slot = r.varint()?;
                    let count = r.count(200)?;
                    let mut pages = Vec::new();
                    for _ in 0..count {
                        pages.push(Tag::String(r.string(8192)?.as_str().into()));
                    }
                    if pages.is_empty() {
                        pages.push(Tag::String("".into()));
                    }
                    let signed = codec::boolean(&mut r)?;
                    let mut tag = BTreeMap::new();
                    tag.insert("pages".into(), Tag::List(pages));
                    if signed {
                        tag.insert("title".into(), Tag::String(r.string(128)?.as_str().into()));
                        tag.insert(
                            "author".into(),
                            Tag::String(self.earlier.username.as_str().into()),
                        );
                    }
                    codec::end(&r)?;
                    codec::Stack::write(
                        Some(&codec::Stack {
                            id: chain::named_id(step.source, "items", "writable_book")?,
                            count: 1,
                            damage: 0,
                            tag: Tag::Compound(tag),
                        }),
                        &mut w,
                        step.source,
                    )?;
                    w.bool(signed);
                    w.varint(slot);
                }
                "PLAYER_ACTION" | "USE_ITEM_ON" | "USE_ITEM" if step.source == 758 => {
                    match name {
                        "PLAYER_ACTION" => {
                            w.varint(r.varint()?);
                            w.i64(r.i64()?);
                            w.u8(r.u8()?);
                        }
                        "USE_ITEM_ON" => {
                            w.varint(r.varint()?);
                            w.i64(r.i64()?);
                            w.varint(r.varint()?);
                            w.bytes(r.bytes(12)?);
                            w.bool(codec::boolean(&mut r)?);
                        }
                        _ => {
                            w.varint(r.varint()?);
                        }
                    }
                    let sequence = r.varint()?;
                    if sequence < 0 {
                        return Err(Error::new("Negative old block prediction sequence"));
                    }
                    codec::end(&r)?;
                    let mut ack = Writer::new();
                    ack.varint(sequence);
                    self.local
                        .push(step.incoming("BLOCK_CHANGED_ACK", ack.data)?);
                }
                "SET_CARRIED_ITEM" if step.source == 754 => {
                    self.inventory.select(r.i16()?);
                    codec::end(&r)?;
                    w.bytes(&data);
                }
                "CONTAINER_CLOSE" => {
                    self.loom = None;
                    if step.source == 754 {
                        self.inventory.close(r.u8()?);
                    }
                    w.bytes(&data);
                }
                "CONTAINER_BUTTON_CLICK" if step.source == 758 => {
                    let window = r.u8()?;
                    let button = r.i8()?;
                    codec::end(&r)?;
                    w.u8(window);
                    w.i8(if self.loom == Some(i32::from(window)) {
                        button
                            .checked_add(1)
                            .ok_or_else(|| Error::new("Old loom pattern overflow"))?
                    } else {
                        button
                    });
                }
                _ => {
                    w.bytes(&data);
                }
            }
            result.push(step.outgoing(target, w.data)?);
        }
        Ok(result)
    }
    fn earlier_clientbound(&mut self, id: i32, data: &[u8]) -> Result<Vec<Packet>> {
        self.earlier.clientbound(self.step, id, data)
    }
}
fn random_seed(w: &mut Writer) -> Result<()> {
    let mut bytes = [0; 8];
    crate::crypto::random_bytes(&mut bytes)?;
    w.bytes(&bytes);
    Ok(())
}
fn bundled(data: &[u8]) -> Result<Tag> {
    let (root, used) = nbt::read_named(
        data,
        nbt::Limits {
            max_bytes: data.len(),
            max_nodes: 100000,
            max_depth: 64,
        },
    )
    .map_err(|_| Error::new("Invalid bundled legacy registry"))?;
    if used != data.len() {
        return Err(Error::new("Trailing bundled legacy registry bytes"));
    }
    Ok(root.tag)
}
fn insert_registry(registry: &mut Tag, name: &str, value: Tag) -> Result<()> {
    let Tag::Compound(registry) = registry else {
        return Err(Error::new("Old registry root is not a compound"));
    };
    registry.insert(name.into(), value);
    Ok(())
}
fn dimensions_mut(registry: &mut Tag, mut apply: impl FnMut(&mut Tag)) {
    if let Some(Tag::List(values)) = registry
        .get_mut("minecraft:dimension_type")
        .and_then(|value| value.get_mut("value"))
    {
        for value in values {
            if let Some(element) = value.get_mut("element") {
                apply(element);
            }
        }
    }
}
fn add_infiniburn(tag: &mut Tag, registry: bool) {
    if registry {
        dimensions_mut(tag, |tag| add_infiniburn(tag, false));
        return;
    }
    if let Some(Tag::String(name)) = tag.get_mut("infiniburn") {
        let value = name.to_string_lossy();
        if !value.starts_with('#') {
            *name = format!("#{value}").as_str().into();
        }
    }
}
fn add_monster_spawn(tag: &mut Tag, registry: bool) {
    if registry {
        dimensions_mut(tag, |tag| add_monster_spawn(tag, false));
        return;
    }
    if let Tag::Compound(fields) = tag {
        fields.insert("monster_spawn_block_light_limit".into(), Tag::Int(0));
        fields.insert("monster_spawn_light_level".into(), Tag::Int(11));
    }
}
fn add_world_height(tag: &mut Tag, registry: bool) {
    if registry {
        dimensions_mut(tag, |tag| add_world_height(tag, false));
        return;
    }
    if let Tag::Compound(fields) = tag {
        fields.insert("height".into(), Tag::Int(256));
        fields.insert("min_y".into(), Tag::Int(0));
    }
}
fn dimension_name(dimension: i32) -> Result<&'static str> {
    match dimension {
        -1 => Ok("minecraft:the_nether"),
        0 => Ok("minecraft:overworld"),
        1 => Ok("minecraft:the_end"),
        _ => Err(Error::new("Unknown pre-1.16 vanilla dimension")),
    }
}
fn dimensions_116() -> Result<Tag> {
    let registry = bundled(crate::vfp_release_chain_resources::DIMENSION_REGISTRY_1_16_2)?;
    let Some(Tag::List(values)) = registry
        .get("minecraft:dimension_type")
        .and_then(|value| value.get("value"))
    else {
        return Err(Error::new("Bundled dimension registry has no entries"));
    };
    let mut dimensions = Vec::new();
    for value in values {
        let (Some(Tag::String(name)), Some(Tag::Compound(element))) =
            (value.get("name"), value.get("element"))
        else {
            return Err(Error::new("Malformed bundled dimension entry"));
        };
        let mut fields = element.clone();
        for key in ["effects", "coordinate_scale", "min_y", "height"] {
            fields.remove(&key.into());
        }
        fields.insert("name".into(), Tag::String(name.clone()));
        fields.insert(
            "shrunk".into(),
            Tag::Byte(if name.to_string_lossy() == "minecraft:the_nether" {
                1
            } else {
                0
            }),
        );
        dimensions.push(Tag::Compound(fields));
    }
    Ok(Tag::compound([("dimension", Tag::List(dimensions))]))
}
fn attribute_name(value: &str) -> String {
    if value.contains(':') {
        return value.to_owned();
    }
    let mut result = String::from("minecraft:");
    for character in value.chars() {
        if character.is_ascii_uppercase() {
            result.push('_');
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}
fn door_sound(event: i32) -> Option<i32> {
    Some(match event {
        1005 => 615,
        1006 => 1346,
        1007 => 1348,
        1008 => 111,
        1011 => 614,
        1012 => 1345,
        1013 => 1347,
        1014 => 110,
        1036 => 622,
        1037 => 623,
        _ => return None,
    })
}
fn level_event(
    step: &Step,
    w: &mut Writer,
    event: i32,
    position: Position,
    value: i32,
    global: bool,
) -> Result<()> {
    w.i32(event);
    w.i64(position.pack(step.target)?);
    w.i32(match event {
        2001 => step.same_or_map("blockstates", value, false)?,
        1010 => step.same_or_map("items", value, false)?,
        _ => value,
    });
    w.bool(global);
    Ok(())
}
