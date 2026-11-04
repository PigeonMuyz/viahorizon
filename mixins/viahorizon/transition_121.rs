// SPDX-License-Identifier: GPL-3.0-or-later
// 1.21/1.21.1 structural layer. Static ids stay in the server's domain;
// recipes are emitted in native format because their definitions own stacks.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
    vfp_tables_121 as old, vfp_tables_261_native as native, vfp_tables_1212 as next,
    vfp_wire::{self as wire, Packet},
};
use std::collections::{BTreeMap, BTreeSet};
pub enum Prepared {
    Source(Packet),
    Native(Packet),
}
#[derive(Clone)]
struct Boat {
    kind: i32,
    uuid: u128,
    position: [f64; 3],
    rotation: [u8; 3],
    data: i32,
    chest: bool,
    variant: i32,
    metadata: BTreeMap<u8, (i32, Vec<u8>)>,
    passengers: Vec<i32>,
}
pub struct Transition {
    recipes: crate::vfp_recipes_121::Recipes,
    entities: BTreeMap<i32, &'static str>,
    boats: BTreeMap<i32, Boat>,
    player: i32,
    vehicle: Option<i32>,
    input: (f32, f32, u8),
    ground: bool,
    collision: bool,
    bundling: bool,
    max_health: f64,
    world: Option<String>,
    fake_teleports: BTreeSet<i32>,
    fake_counter: i32,
    skip_position: bool,
    replies: Vec<Packet>,
    equipment: crate::vfp_attributes_1205::Equipment,
}
impl Default for Transition {
    fn default() -> Self {
        Self {
            recipes: Default::default(),
            entities: BTreeMap::new(),
            boats: BTreeMap::new(),
            player: 0,
            vehicle: None,
            input: (0.0, 0.0, 0),
            ground: false,
            collision: false,
            bundling: false,
            max_health: 20.0,
            world: None,
            fake_teleports: BTreeSet::new(),
            fake_counter: i32::MIN,
            skip_position: false,
            replies: Vec::new(),
            equipment: Default::default(),
        }
    }
}
fn source(name: &str, data: Vec<u8>) -> Result<Prepared> {
    Ok(Prepared::Source((
        wire::packet_id(next::CLIENTBOUND, name).map_err(Error::new)?,
        data,
    )))
}
fn metadata(entity: i32, fields: impl IntoIterator<Item = (u8, i32, Vec<u8>)>) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(entity);
    for (index, kind, data) in fields {
        w.u8(index);
        w.varint(kind);
        w.bytes(&data);
    }
    w.u8(255);
    w.data
}
fn boat_kind(variant: i32, chest: bool) -> Result<i32> {
    let name = match (variant, chest) {
        (1, false) => "spruce_boat",
        (2, false) => "birch_boat",
        (3, false) => "jungle_boat",
        (4, false) => "acacia_boat",
        (5, false) => "cherry_boat",
        (6, false) => "dark_oak_boat",
        (7, false) => "mangrove_boat",
        (8, false) => "bamboo_raft",
        (1, true) => "spruce_chest_boat",
        (2, true) => "birch_chest_boat",
        (3, true) => "jungle_chest_boat",
        (4, true) => "acacia_chest_boat",
        (5, true) => "cherry_chest_boat",
        (6, true) => "dark_oak_chest_boat",
        (7, true) => "mangrove_chest_boat",
        (8, true) => "bamboo_chest_raft",
        (_, true) => "oak_chest_boat",
        _ => "oak_boat",
    };
    native::ENTITY_NAMES
        .iter()
        .position(|value| *value == name)
        .map(|id| id as i32)
        .ok_or_else(|| Error::new("Missing native boat type"))
}
fn spawn_info(r: &mut Reader<'_>) -> Result<String> {
    r.varint()?;
    let world = r.string(32767)?;
    r.bytes(12)?;
    if r.bool()? {
        r.string(32767)?;
        r.bytes(8)?;
    }
    r.varint()?;
    Ok(world)
}
pub fn client_information(data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    r.string(16)?;
    r.i8()?;
    r.varint()?;
    r.bool()?;
    r.u8()?;
    r.varint()?;
    r.bool()?;
    r.bool()?;
    let end = r.position();
    r.varint()?;
    if r.remaining() != 0 {
        return Err(Error::new("Trailing old client information"));
    }
    Ok(data[..end].to_vec())
}
fn pos(x: i32, y: i32, z: i32) -> i64 {
    ((i64::from(x) & 0x3ffffff) << 38) | ((i64::from(z) & 0x3ffffff) << 12) | (i64::from(y) & 0xfff)
}
impl Transition {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn clientbound<M: Maps>(
        &mut self,
        id: i32,
        data: &[u8],
        registry: &mut Registries,
    ) -> Result<Vec<Prepared>> {
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| {
                if M::PROTOCOL < 767 {
                    crate::vfp_tables_1205::CLIENTBOUND.get(id)
                } else {
                    old::CLIENTBOUND.get(id)
                }
            })
            .copied()
            .ok_or_else(|| Error::new("Unknown 1.21 play packet"))?;
        match name {
            "UPDATE_RECIPES" => {
                return Ok(vec![Prepared::Native(
                    self.recipes.update::<M>(data, registry)?,
                )]);
            }
            "RECIPE" => {
                return Ok(self
                    .recipes
                    .book(data)?
                    .into_iter()
                    .map(Prepared::Native)
                    .collect());
            }
            "PLACE_GHOST_RECIPE" => {
                return Ok(self
                    .recipes
                    .ghost(data)?
                    .into_iter()
                    .map(Prepared::Native)
                    .collect());
            }
            _ => (),
        }
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut target = name;
        let mut before = Vec::new();
        let mut after = Vec::new();
        match name {
            "BUNDLE_DELIMITER" => {
                self.bundling = !self.bundling;
                w.bytes(data);
            }
            "LOGIN" => {
                registry.clear_entity_overrides();
                self.boats.clear();
                self.entities.clear();
                self.player = r.i32()?;
                self.entities.insert(self.player, "player");
                self.vehicle = None;
                self.max_health = 20.0;
                r.bool()?;
                for _ in 0..r.count(1024)? {
                    r.string(32767)?;
                }
                for _ in 0..3 {
                    r.varint()?;
                }
                for _ in 0..3 {
                    r.bool()?;
                }
                self.world = Some(spawn_info(&mut r)?);
                w.bytes(&data[..r.position()]);
                w.varint(64);
                w.bytes(r.rest());
            }
            "RESPAWN" => {
                registry.clear_entity_overrides();
                let world = spawn_info(&mut r)?;
                let end = r.position();
                let keep = r.u8()?;
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old respawn"));
                }
                self.world = Some(world);
                self.boats.clear();
                self.entities.clear();
                self.entities.insert(self.player, "player");
                self.vehicle = None;
                self.ground = false;
                self.collision = false;
                if keep & 1 == 0 {
                    self.max_health = 20.0;
                }
                w.bytes(&data[..end]);
                w.varint(64);
                w.u8(keep);
                if keep & 2 != 0 {
                    if !self.bundling {
                        before.push(source("BUNDLE_DELIMITER", Vec::new())?);
                    }
                    let mut health = Writer::new();
                    health.f32(self.max_health as f32);
                    let mut pose = Writer::new();
                    pose.varint(0);
                    after.push(source(
                        "SET_ENTITY_DATA",
                        metadata(self.player, [(6, 21, pose.data), (9, 3, health.data)]),
                    )?);
                    let teleport = self.fake_counter;
                    self.fake_counter = self.fake_counter.wrapping_add(1);
                    self.fake_teleports.insert(teleport);
                    let mut position = Writer::new();
                    position.varint(teleport);
                    for _ in 0..6 {
                        position.f64(0.0);
                    }
                    position.f32(-180.0);
                    position.f32(0.0);
                    position.i32(7);
                    after.push(source("PLAYER_POSITION", position.data)?);
                    if !self.bundling {
                        after.push(source("BUNDLE_DELIMITER", Vec::new())?);
                    }
                }
            }
            "UPDATE_ATTRIBUTES" => {
                let entity = r.varint()?;
                w.varint(entity);
                let count = r.count(1024)?;
                w.varint(count as i32);
                for _ in 0..count {
                    let attribute = r.varint()?;
                    let base = r.f64()?;
                    w.varint(attribute);
                    w.f64(base);
                    let mut mods: [BTreeMap<String, f64>; 3] = Default::default();
                    let mut soul = false;
                    let count = r.count(1024)?;
                    w.varint(count as i32);
                    for _ in 0..count {
                        let id = if M::PROTOCOL < 767 {
                            crate::vfp_modifiers_1205::name(r.uuid()?, None)
                        } else {
                            r.string(32767)?
                        };
                        let value = r.f64()?;
                        let operation = r.u8()?;
                        soul |= matches!(
                            id.strip_prefix("minecraft:").unwrap_or(&id),
                            "enchantment.soul_speed"
                        );
                        w.string(&id);
                        w.f64(value);
                        w.u8(operation);
                        if operation > 2 {
                            return Err(Error::new("Invalid old attribute operation"));
                        }
                        mods[operation as usize].insert(id, value);
                    }
                    if M::PROTOCOL < 767
                        && entity == self.player
                        && M::ATTRIBUTE_IDENTIFIERS.get(attribute as usize)
                            == Some(&"generic.movement_speed")
                    {
                        self.equipment.soul(soul);
                    }
                    if entity == self.player
                        && M::ATTRIBUTES.map(attribute).map_err(Error::new)?
                            == crate::vfp_tables_121::ATTRIBUTES
                                .map(18)
                                .map_err(Error::new)?
                    {
                        let v = base + mods[0].values().sum::<f64>();
                        let mut v = v + v * mods[1].values().sum::<f64>();
                        for factor in mods[2].values() {
                            v *= 1.0 + factor;
                        }
                        self.max_health = v.clamp(1.0, 1024.0);
                    }
                }
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old attributes"));
                }
            }
            "DISGUISED_CHAT" | "PLAYER_CHAT" if M::PROTOCOL < 767 => {
                if name == "DISGUISED_CHAT" {
                    r.nbt()?;
                } else {
                    r.uuid()?;
                    r.varint()?;
                    if r.bool()? {
                        r.bytes(256)?;
                    }
                    r.string(256)?;
                    r.bytes(16)?;
                    for _ in 0..r.count(20)? {
                        if r.varint()? == 0 {
                            r.bytes(256)?;
                        }
                    }
                    if r.bool()? {
                        r.nbt()?;
                    }
                    let mask = r.varint()?;
                    if mask == 2 {
                        let n = r.count(4096)?;
                        r.bytes(n * 8)?;
                    } else if !(0..=1).contains(&mask) {
                        return Err(Error::new("Invalid legacy chat mask"));
                    }
                }
                w.bytes(&data[..r.position()]);
                let chat = r.varint()?;
                if chat < 0 || chat == i32::MAX {
                    return Err(Error::new("Invalid legacy chat type"));
                }
                w.varint(chat + 1);
                w.bytes(r.rest());
            }
            "PROJECTILE_POWER" if M::PROTOCOL < 767 => {
                w.varint(r.varint()?);
                let x = r.f64()?;
                let y = r.f64()?;
                let z = r.f64()?;
                w.f64((x * x + y * y + z * z).sqrt());
            }
            "GAME_EVENT" if M::PROTOCOL < 767 => {
                let event = r.u8()?;
                let value = r.f32()?;
                if event == 4 && value < 0.5 {
                    let mut reply = Writer::new();
                    reply.varint(0);
                    self.replies.push((
                        wire::packet_id(old::SERVERBOUND, "CLIENT_COMMAND").map_err(Error::new)?,
                        reply.data,
                    ));
                    return Ok(Vec::new());
                }
                w.bytes(data);
            }
            "SET_TIME" => {
                w.i64(r.i64()?);
                let day = r.i64()?;
                w.i64(day.wrapping_abs());
                w.bool(day >= 0);
            }
            "PLAYER_INFO_UPDATE" => {
                if r.u8()? & !63 != 0 {
                    return Err(Error::new("Invalid old player-info actions"));
                }
                w.bytes(data);
            }
            "PLAYER_POSITION" => {
                let position = r.bytes(24)?.to_vec();
                let rotation = r.bytes(8)?.to_vec();
                let flags = r.u8()? & 31;
                let teleport = r.varint()?;
                w.varint(teleport);
                w.bytes(&position);
                w.bytes(&[0; 24]);
                w.bytes(&rotation);
                w.i32(i32::from(flags) | ((i32::from(flags) & 7) << 5));
            }
            "TELEPORT_ENTITY" => {
                target = "ENTITY_POSITION_SYNC";
                let entity = r.varint()?;
                w.varint(entity);
                let position = [r.f64()?, r.f64()?, r.f64()?];
                for p in position {
                    w.f64(p);
                }
                w.bytes(&[0; 24]);
                let yaw = r.u8()?;
                let pitch = r.u8()?;
                w.f32(f32::from(yaw) * 360.0 / 256.0);
                w.f32(f32::from(pitch) * 360.0 / 256.0);
                w.bool(r.bool()?);
                if let Some(boat) = self.boats.get_mut(&entity) {
                    boat.position = position;
                    boat.rotation[0] = pitch;
                    boat.rotation[1] = yaw;
                }
            }
            "HORSE_SCREEN_OPEN" if M::PROTOCOL < 767 => {
                w.varint(i32::from(r.u8()?));
                w.varint((r.varint()? - 1).max(0) / 3);
                w.bytes(r.rest());
            }
            "LEVEL_EVENT" if M::PROTOCOL < 767 => {
                let event = r.i32()?;
                w.i32(event);
                w.bytes(r.bytes(8)?);
                let data = r.i32()?;
                if event == 1010 {
                    let item = usize::try_from(data)
                        .ok()
                        .and_then(|id| M::ITEM_IDENTIFIERS.get(id))
                        .copied()
                        .unwrap_or("");
                    let Some(name) = item.strip_prefix("music_disc_") else {
                        return Ok(Vec::new());
                    };
                    let id = crate::vfp_registry_1205::JUKEBOX_ORDER
                        .iter()
                        .position(|key| key.strip_prefix("minecraft:").unwrap_or(key) == name)
                        .ok_or_else(|| Error::new("Missing legacy music disc song"))?;
                    w.i32(id as i32);
                } else {
                    w.i32(data);
                }
                w.bytes(r.rest());
            }
            "CONTAINER_SET_CONTENT"
            | "CONTAINER_CLOSE"
            | "CONTAINER_SET_DATA"
            | "HORSE_SCREEN_OPEN" => {
                w.varint(i32::from(r.u8()?));
                w.bytes(r.rest());
            }
            "CONTAINER_SET_SLOT" => {
                let container = i32::from(r.i8()?);
                let state = r.varint()?;
                let slot = r.i16()?;
                match container {
                    -1 => target = "SET_CURSOR_ITEM",
                    -2 => {
                        target = "SET_PLAYER_INVENTORY";
                        w.varint(i32::from(slot));
                    }
                    _ => {
                        w.varint(container);
                        w.varint(state);
                        w.i16(slot);
                    }
                }
                w.bytes(r.rest());
            }
            "SET_CARRIED_ITEM" => {
                target = "SET_HELD_SLOT";
                w.bytes(data);
            }
            "COOLDOWN" => {
                let id = r.varint()?;
                let name = usize::try_from(id)
                    .ok()
                    .and_then(|id| M::ITEM_IDENTIFIERS.get(id))
                    .ok_or_else(|| Error::new("Invalid old cooldown item"))?;
                w.string(&format!("minecraft:{name}"));
                w.varint(r.varint()?);
            }
            "EXPLODE" => {
                let center = [r.f64()?, r.f64()?, r.f64()?];
                for p in center {
                    w.f64(p);
                }
                let power = r.f32()?;
                let mut blocks = Vec::new();
                for _ in 0..r.count(1_048_576)? {
                    blocks.push([r.i8()?, r.i8()?, r.i8()?]);
                }
                registry.set_explosion(power, blocks.len() as i32);
                let knockback = [r.f32()?, r.f32()?, r.f32()?];
                let present = knockback.iter().any(|v| *v != 0.0);
                w.bool(present);
                if present {
                    for v in knockback {
                        w.f64(f64::from(v));
                    }
                }
                let interaction = r.varint()?;
                if matches!(interaction, 1 | 2) {
                    for block in blocks {
                        let mut update = Writer::new();
                        update.i64(pos(
                            (center[0].floor() as i32).wrapping_add(i32::from(block[0])),
                            (center[1].floor() as i32).wrapping_add(i32::from(block[1])),
                            (center[2].floor() as i32).wrapping_add(i32::from(block[2])),
                        ));
                        update.varint(0);
                        before.push(source("BLOCK_UPDATE", update.data)?);
                    }
                }
                let start = r.position();
                crate::entity_data::vfp_entities::particle_using::<M>(
                    &mut r,
                    &mut Writer::new(),
                    registry,
                )?;
                let small = r.since(start).to_vec();
                let start = r.position();
                crate::entity_data::vfp_entities::particle_using::<M>(
                    &mut r,
                    &mut Writer::new(),
                    registry,
                )?;
                let large = r.since(start).to_vec();
                w.bytes(if power >= 2.0 && interaction != 0 {
                    &large
                } else {
                    &small
                });
                w.bytes(r.rest());
            }
            "ADD_ENTITY" => {
                let entity = r.varint()?;
                let uuid = r.uuid()?;
                let kind = r.varint()?;
                let name = usize::try_from(kind)
                    .ok()
                    .and_then(|kind| M::ENTITY_IDENTIFIERS.get(kind))
                    .copied()
                    .ok_or_else(|| Error::new("Invalid old entity kind"))?;
                if self.entities.len() >= 65536 && !self.entities.contains_key(&entity) {
                    return Err(Error::new("Too many tracked legacy entities"));
                }
                self.entities.insert(entity, name);
                self.boats.remove(&entity);
                if matches!(name, "boat" | "chest_boat") {
                    let position = [r.f64()?, r.f64()?, r.f64()?];
                    let rotation = [r.u8()?, r.u8()?, r.u8()?];
                    let extra = r.varint()?;
                    registry.set_entity_override(entity, Some(boat_kind(0, name == "chest_boat")?));
                    self.boats.insert(
                        entity,
                        Boat {
                            kind,
                            uuid,
                            position,
                            rotation,
                            data: extra,
                            chest: name == "chest_boat",
                            variant: 0,
                            metadata: BTreeMap::new(),
                            passengers: Vec::new(),
                        },
                    );
                } else {
                    registry.set_entity_override(entity, None);
                }
                w.bytes(data);
            }
            "SET_ENTITY_DATA" => {
                let (entity, fields) =
                    crate::entity_data::vfp_entities::fields_121::<M>(data, registry)?;
                let name = self.entities.get(&entity).copied().unwrap_or("");
                let boat = self.boats.get_mut(&entity);
                let mut values = Vec::new();
                let mut changed = false;
                let mut boat = boat;
                for (mut index, kind, value) in fields {
                    if let Some(boat) = &mut boat {
                        if index == 11 {
                            if kind != 1 {
                                return Err(Error::new("Invalid old boat variant"));
                            }
                            let mut variant = Reader::new(&value).varint()?;
                            if M::PROTOCOL < 762 && variant > 4 {
                                variant = variant
                                    .checked_add(1)
                                    .ok_or_else(|| Error::new("Old boat variant overflows"))?;
                            }
                            changed |= variant != boat.variant;
                            boat.variant = variant;
                            continue;
                        }
                        if index > 11 {
                            index -= 1;
                        }
                        boat.metadata.insert(index, (kind, value.clone()));
                    } else {
                        if matches!(name, "arrow" | "spectral_arrow" | "trident") && index >= 10 {
                            index = index
                                .checked_add(1)
                                .filter(|index| *index < 255)
                                .ok_or_else(|| Error::new("Arrow metadata index overflows"))?;
                        }
                        if matches!(name, "dolphin" | "squid" | "glow_squid") && index >= 16 {
                            index = index
                                .checked_add(1)
                                .filter(|index| *index < 255)
                                .ok_or_else(|| {
                                    Error::new("Water creature metadata index overflows")
                                })?;
                        }
                    }
                    values.push((index, kind, value));
                }
                if changed {
                    let boat = boat.expect("changed boat variant");
                    registry
                        .set_entity_override(entity, Some(boat_kind(boat.variant, boat.chest)?));
                    if !self.bundling {
                        before.push(source("BUNDLE_DELIMITER", Vec::new())?);
                    }
                    let mut remove = Writer::new();
                    remove.varint(1);
                    remove.varint(entity);
                    before.push(source("REMOVE_ENTITIES", remove.data)?);
                    let mut spawn = Writer::new();
                    spawn.varint(entity);
                    spawn.uuid(boat.uuid);
                    spawn.varint(boat.kind);
                    for p in boat.position {
                        spawn.f64(p);
                    }
                    spawn.bytes(&boat.rotation);
                    spawn.varint(boat.data);
                    spawn.bytes(&[0; 6]);
                    before.push(source("ADD_ENTITY", spawn.data)?);
                    values = boat
                        .metadata
                        .iter()
                        .map(|(index, (kind, value))| (*index, *kind, value.clone()))
                        .collect();
                    if !boat.passengers.is_empty() {
                        let mut passengers = Writer::new();
                        passengers.varint(entity);
                        passengers.varint(boat.passengers.len() as i32);
                        for id in &boat.passengers {
                            passengers.varint(*id);
                        }
                        after.push(source("SET_PASSENGERS", passengers.data)?);
                    }
                    if !self.bundling {
                        after.push(source("BUNDLE_DELIMITER", Vec::new())?);
                    }
                }
                w.bytes(&metadata(entity, values));
            }
            "SET_PASSENGERS" => {
                let entity = r.varint()?;
                let mut passengers = Vec::new();
                for _ in 0..r.count(65536)? {
                    passengers.push(r.varint()?);
                }
                if self.vehicle == Some(entity) {
                    self.vehicle = None;
                }
                if passengers.contains(&self.player) {
                    self.vehicle = Some(entity);
                }
                if let Some(boat) = self.boats.get_mut(&entity) {
                    boat.passengers = passengers;
                }
                w.bytes(data);
            }
            "REMOVE_ENTITIES" => {
                for _ in 0..r.count(65536)? {
                    let entity = r.varint()?;
                    self.entities.remove(&entity);
                    self.boats.remove(&entity);
                    registry.set_entity_override(entity, None);
                    if self.vehicle == Some(entity) {
                        self.vehicle = None;
                    }
                }
                w.bytes(data);
            }
            "MOVE_ENTITY_POS" | "MOVE_ENTITY_POS_ROT" | "MOVE_ENTITY_ROT" => {
                let entity = r.varint()?;
                if let Some(boat) = self.boats.get_mut(&entity) {
                    if name != "MOVE_ENTITY_ROT" {
                        for p in &mut boat.position {
                            *p += f64::from(r.i16()?) / 4096.0;
                        }
                    }
                    if name != "MOVE_ENTITY_POS" {
                        boat.rotation[1] = r.u8()?;
                        boat.rotation[0] = r.u8()?;
                    }
                }
                w.bytes(data);
            }
            _ => w.bytes(data),
        }
        if M::PROTOCOL < 767 {
            if let Some(packet) = self.equipment.observe::<M>(name, data, registry)? {
                after.push(Prepared::Native(packet));
            }
        }
        before.push(source(target, w.data)?);
        before.extend(after);
        Ok(before)
    }
    pub fn serverbound(&mut self, protocol: i32, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        // Native tick-end frames reach this method even when they have no
        // legacy equivalent, flushing automatic replies on the next tick.
        let mut result = std::mem::take(&mut self.replies);
        for (id, data) in packets {
            let name = usize::try_from(id)
                .ok()
                .and_then(|id| next::SERVERBOUND.get(id))
                .copied()
                .ok_or_else(|| Error::new("Unknown 1.21.2 reply"))?;
            if matches!(name, "BUNDLE_ITEM_SELECTED" | "CLIENT_TICK_END") {
                continue;
            }
            let mut r = Reader::new(&data);
            let mut w = Writer::new();
            match name {
                "USE_ITEM" if protocol < 767 => {
                    w.varint(r.varint()?);
                    w.varint(r.varint()?);
                    let yaw = r.f32()?;
                    let pitch = r.f32()?;
                    if r.remaining() != 0 {
                        return Err(Error::new("Trailing legacy use-item rotations"));
                    }
                    let mut rotation = Writer::new();
                    rotation.f32(yaw);
                    rotation.f32(pitch);
                    rotation.bool(self.ground);
                    result.push((
                        wire::packet_id(old::SERVERBOUND, "MOVE_PLAYER_ROT").map_err(Error::new)?,
                        rotation.data,
                    ));
                }
                "CLIENT_INFORMATION" => w.bytes(&client_information(&data)?),
                "PLACE_RECIPE" | "RECIPE_BOOK_SEEN_RECIPE" => {
                    let Some(value) = self.recipes.serverbound(name, &data)? else {
                        continue;
                    };
                    w.bytes(&value);
                }
                "CONTAINER_CLICK" | "CONTAINER_CLOSE" => {
                    let id = r.varint()?;
                    if !(0..=255).contains(&id) {
                        return Err(Error::new("Legacy container id out of range"));
                    }
                    w.u8(id as u8);
                    w.bytes(r.rest());
                }
                "USE_ITEM_ON" => {
                    r.varint()?;
                    r.bytes(8)?;
                    r.varint()?;
                    r.bytes(12)?;
                    r.bool()?;
                    let end = r.position();
                    r.bool()?;
                    w.bytes(&data[..end]);
                    w.bytes(r.rest());
                }
                "PLAYER_INPUT" => {
                    let flags = r.u8()?;
                    if r.remaining() != 0 || flags & 128 != 0 {
                        return Err(Error::new("Invalid native player input"));
                    }
                    self.input = (
                        if flags & 4 != 0 {
                            0.98
                        } else if flags & 8 != 0 {
                            -0.98
                        } else {
                            0.0
                        },
                        if flags & 1 != 0 {
                            0.98
                        } else if flags & 2 != 0 {
                            -0.98
                        } else {
                            0.0
                        },
                        ((flags >> 4) & 1) | ((flags >> 4) & 2),
                    );
                    continue;
                }
                "ACCEPT_TELEPORTATION" => {
                    if self.fake_teleports.remove(&r.varint()?) {
                        self.skip_position = true;
                        continue;
                    }
                    w.bytes(&data);
                }
                "MOVE_PLAYER_POS"
                | "MOVE_PLAYER_POS_ROT"
                | "MOVE_PLAYER_ROT"
                | "MOVE_PLAYER_STATUS_ONLY" => {
                    let fields = match name {
                        "MOVE_PLAYER_POS" => 24,
                        "MOVE_PLAYER_POS_ROT" => 32,
                        "MOVE_PLAYER_ROT" => 8,
                        _ => 0,
                    };
                    w.bytes(r.bytes(fields)?);
                    let flags = r.u8()?;
                    if flags & !3 != 0 || r.remaining() != 0 {
                        return Err(Error::new("Invalid movement flags"));
                    }
                    let ground = flags & 1 != 0;
                    let collision = flags & 2 != 0;
                    let cancel = name == "MOVE_PLAYER_STATUS_ONLY"
                        && self.ground == ground
                        && self.collision != collision;
                    self.ground = ground;
                    self.collision = collision;
                    w.bool(ground);
                    if cancel {
                        continue;
                    }
                    if name == "MOVE_PLAYER_POS_ROT" && self.skip_position {
                        self.skip_position = false;
                        continue;
                    }
                }
                _ => w.bytes(&data),
            }
            result.push((
                wire::packet_id(old::SERVERBOUND, name).map_err(Error::new)?,
                w.data,
            ));
            if name == "MOVE_PLAYER_ROT" && self.vehicle.is_some() {
                let mut input = Writer::new();
                input.f32(self.input.0);
                input.f32(self.input.1);
                input.u8(self.input.2);
                result.push((
                    wire::packet_id(old::SERVERBOUND, "PLAYER_INPUT").map_err(Error::new)?,
                    input.data,
                ));
            }
        }
        if protocol < 767 {
            for (id, _) in &mut result {
                let name = old::SERVERBOUND
                    .get(*id as usize)
                    .ok_or_else(|| Error::new("Unknown 1.21 reply id"))?;
                *id = wire::packet_id(crate::vfp_tables_1205::SERVERBOUND, name)
                    .map_err(Error::new)?;
            }
        }
        Ok(result)
    }
}
