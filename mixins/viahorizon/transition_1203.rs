// SPDX-License-Identifier: GPL-3.0-or-later
// 1.20.3/1.20.4 packet bodies enter the 1.20.5 structural chain here.
// Item bodies are already native; all other static ids stay in M's domain.
use crate::{
    Error, Result,
    buf::{Codec, Reader, Writer},
    item::Slot,
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
    vfp_tables_1203 as old, vfp_tables_1205 as next,
    vfp_transition_121::Prepared,
    vfp_wire::{self as wire, Packet},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Transition {
    families: BTreeMap<i32, u8>,
    cloud_colors: BTreeMap<i32, i32>,
    metadata_seen: BTreeSet<i32>,
    teams: BTreeMap<String, BTreeSet<String>>,
    player: i32,
    secure_chat: Option<bool>,
    chat_session: Option<Vec<u8>>,
    replies: Vec<Packet>,
    delayed_acks: i32,
    last_seen: [u8; 3],
}
fn source(name: &str, data: Vec<u8>) -> Result<Prepared> {
    Ok(Prepared::Source((
        wire::packet_id(next::CLIENTBOUND, name).map_err(Error::new)?,
        data,
    )))
}
fn end(r: &Reader<'_>) -> Result<()> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(Error::new("Trailing 1.20.3 packet bytes"))
    }
}
fn string(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.string(&r.string(32767)?);
    Ok(())
}
fn nonempty_item<M: Maps>(r: &mut Reader<'_>, registry: &Registries) -> Result<Vec<u8>> {
    let mut w = Writer::new();
    crate::vfp_items_1203::forward::<M>(r, &mut w, registry)?;
    if Reader::new(&w.data).varint()? == 0 {
        // As in ViaVersion, invalid required stacks become one stone.
        w = Writer::new();
        w.varint(1);
        w.varint(M::ITEMS.map(1).map_err(Error::new)?);
        w.varint(0);
        w.varint(0);
    }
    Ok(w.data)
}
fn item_template<M: Maps>(r: &mut Reader<'_>, w: &mut Writer, registry: &Registries) -> Result<()> {
    let bytes = nonempty_item::<M>(r, registry)?;
    let mut item = Reader::new(&bytes);
    let count = item.varint()?;
    w.varint(item.varint()?);
    w.varint(count);
    w.bytes(item.rest());
    Ok(())
}
fn item_cost(bytes: &[u8], w: &mut Writer) -> Result<bool> {
    let mut r = Reader::new(bytes);
    let Some(stack) = Slot::read(&mut r)? else {
        return Ok(false);
    };
    end(&r)?;
    w.varint(stack.item);
    w.varint(stack.count);
    w.varint(stack.added.len() as i32);
    for (kind, value) in stack.added {
        w.varint(kind);
        value.write(kind, w);
    }
    Ok(true)
}
fn spawn_info(r: &mut Reader<'_>, w: &mut Writer, registry: &Registries) -> Result<()> {
    let dimension = r.string(32767)?;
    w.varint(
        registry
            .id("dimension_type", &dimension)
            .map_err(Error::new)?,
    );
    string(r, w)?;
    w.bytes(r.bytes(12)?); // seed, current/previous game mode, debug and flat
    let death = r.bool()?;
    w.bool(death);
    if death {
        string(r, w)?;
        w.bytes(r.bytes(8)?);
    }
    w.varint(r.varint()?);
    Ok(())
}
fn range_attributes(player: i32, creative: bool) -> Result<Prepared> {
    let mut w = Writer::new();
    w.varint(player);
    w.varint(2);
    for (source, base, bonus, name) in [
        (6, 4.5, 0.5, "minecraft:creative_mode_block_range"),
        (7, 3.0, 2.0, "minecraft:creative_mode_entity_range"),
    ] {
        w.varint(next::ATTRIBUTES.map(source).map_err(Error::new)?);
        w.f64(base);
        w.varint(i32::from(creative));
        if creative {
            w.string(name);
            w.f64(bonus);
            w.u8(0);
        }
    }
    Ok(Prepared::Native((
        wire::packet_id(crate::vfp_tables::NEW_CLIENTBOUND, "UPDATE_ATTRIBUTES")
            .map_err(Error::new)?,
        w.data,
    )))
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
            .and_then(|id| old::CLIENTBOUND.get(id))
            .copied()
            .ok_or_else(|| Error::new("Unknown 1.20.3 play packet"))?;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut after = Vec::new();
        match name {
            // The recipe engine reads the original string serializer and NBT stacks.
            "UPDATE_RECIPES" => w.bytes(data),
            "LOGIN" => {
                self.families.clear();
                self.cloud_colors.clear();
                self.metadata_seen.clear();
                self.teams.clear();
                self.delayed_acks = 0;
                self.last_seen = [0; 3];
                self.player = r.i32()?;
                w.i32(self.player);
                w.bool(r.bool()?);
                let worlds = r.count(65536)?;
                w.varint(worlds as i32);
                for _ in 0..worlds {
                    string(&mut r, &mut w)?;
                }
                for _ in 0..3 {
                    w.varint(r.varint()?);
                }
                w.bytes(r.bytes(3)?);
                let start = r.position();
                r.string(32767)?;
                r.string(32767)?;
                r.i64()?;
                let creative = r.u8()? == 1;
                let mut spawn = Reader::new(&data[start..]);
                spawn_info(&mut spawn, &mut w, registry)?;
                end(&spawn)?;
                w.bool(self.secure_chat.unwrap_or(false));
                after.push(range_attributes(self.player, creative)?);
            }
            "RESPAWN" => {
                self.families.clear();
                self.cloud_colors.clear();
                self.metadata_seen.clear();
                self.teams.clear();
                let mut mode = Reader::new(data);
                mode.string(32767)?;
                mode.string(32767)?;
                mode.i64()?;
                let creative = mode.u8()? == 1;
                spawn_info(&mut r, &mut w, registry)?;
                w.u8(r.u8()?);
                end(&r)?;
                after.push(range_attributes(self.player, creative)?);
            }
            "SERVER_DATA" => {
                w.nbt(&r.nbt()?);
                let icon = r.bool()?;
                w.bool(icon);
                if icon {
                    w.byte_array(&r.byte_array(2_097_152)?);
                }
                let secure = r.bool()?;
                end(&r)?;
                self.secure_chat = Some(secure);
                if secure {
                    if let Some(data) = self.chat_session.take() {
                        self.replies.push((
                            wire::packet_id(old::SERVERBOUND, "CHAT_SESSION_UPDATE")
                                .map_err(Error::new)?,
                            data,
                        ));
                    }
                } else {
                    self.chat_session = None;
                }
            }
            "START_CONFIGURATION" => {
                end(&r)?;
                self.delayed_acks = 0;
                self.last_seen = [0; 3];
                self.secure_chat = None;
                self.chat_session = None;
            }
            "GAME_EVENT" => {
                let event = r.u8()?;
                let value = r.f32()?;
                end(&r)?;
                w.u8(event);
                w.f32(value);
                if event == 3 {
                    after.push(range_attributes(
                        self.player,
                        (value + 0.5).floor() as i32 == 1,
                    )?);
                }
            }
            "ADD_ENTITY" => {
                let entity = r.varint()?;
                r.uuid()?;
                let kind = usize::try_from(r.varint()?)
                    .map_err(|_| Error::new("Negative old entity kind"))?;
                let family = M::ENTITY_FAMILIES
                    .get(kind)
                    .copied()
                    .ok_or_else(|| Error::new("Unknown old entity kind"))?;
                if self.families.len() >= 65536 && !self.families.contains_key(&entity) {
                    return Err(Error::new("Too many old entities"));
                }
                self.families.insert(entity, family);
                self.cloud_colors.remove(&entity);
                self.metadata_seen.remove(&entity);
                w.bytes(data);
            }
            "REMOVE_ENTITIES" => {
                let count = r.count(65536)?;
                for _ in 0..count {
                    let entity = r.varint()?;
                    self.families.remove(&entity);
                    self.cloud_colors.remove(&entity);
                    self.metadata_seen.remove(&entity);
                }
                end(&r)?;
                w.bytes(data);
            }
            "SET_ENTITY_DATA" => {
                let entity = r.varint()?;
                let family = if entity == self.player {
                    1
                } else {
                    self.families.get(&entity).copied().unwrap_or(0)
                };
                let (packets, color) = crate::vfp_entities_1203::metadata::<M>(
                    data,
                    family,
                    self.cloud_colors.get(&entity).copied(),
                    !self.metadata_seen.contains(&entity),
                    registry,
                )?;
                self.metadata_seen.insert(entity);
                if let Some(color) = color {
                    self.cloud_colors.insert(entity, color);
                }
                return Ok(packets.into_iter().map(Prepared::Source).collect());
            }
            "UPDATE_MOB_EFFECT" => {
                w.varint(r.varint()?);
                w.varint(r.varint()?);
                w.varint(i32::from(r.i8()?));
                w.varint(r.varint()?);
                w.u8(r.u8()?);
                if r.bool()? {
                    if M::PROTOCOL < 764 {
                        r.vfp_named_nbt()?;
                    } else {
                        r.nbt()?;
                    }
                }
                end(&r)?;
            }
            "UPDATE_ATTRIBUTES" => {
                w.varint(r.varint()?);
                let count = r.count(1024)?;
                let mut entries = Vec::new();
                for _ in 0..count {
                    let name = r.string(32767)?;
                    let name = name.strip_prefix("minecraft:").unwrap_or(&name);
                    let start = r.position();
                    r.f64()?;
                    let modifiers = r.count(1024)?;
                    r.bytes(
                        modifiers
                            .checked_mul(25)
                            .ok_or_else(|| Error::new("Attribute size overflow"))?,
                    )?;
                    // Unknown registry names are ignored rather than assigned to armor.
                    if let Some(id) = M::ATTRIBUTE_IDENTIFIERS
                        .iter()
                        .position(|value| *value == name)
                    {
                        entries.push((id, r.since(start).to_vec()));
                    }
                }
                end(&r)?;
                w.varint(entries.len() as i32);
                for (id, value) in entries {
                    w.varint(id as i32);
                    w.bytes(&value);
                }
            }
            "CONTAINER_SET_SLOT" => {
                w.i8(r.i8()?);
                w.varint(r.varint()?);
                w.i16(r.i16()?);
                crate::vfp_items_1203::forward::<M>(&mut r, &mut w, registry)?;
                end(&r)?;
            }
            "CONTAINER_SET_CONTENT" => {
                w.u8(r.u8()?);
                w.varint(r.varint()?);
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..=count {
                    crate::vfp_items_1203::forward::<M>(&mut r, &mut w, registry)?;
                }
                end(&r)?;
            }
            "SET_EQUIPMENT" => {
                let entity = r.varint()?;
                w.varint(entity);
                for _ in 0..128 {
                    let slot = r.u8()?;
                    let mapped = if self
                        .families
                        .get(&entity)
                        .is_some_and(|family| family & 2 != 0)
                        && slot & 127 == 4
                    {
                        (slot & 128) | 6
                    } else {
                        slot
                    };
                    w.u8(mapped);
                    crate::vfp_items_1203::forward::<M>(&mut r, &mut w, registry)?;
                    if slot & 128 == 0 {
                        break;
                    }
                }
                end(&r)?;
            }
            "MERCHANT_OFFERS" => {
                w.varint(r.varint()?);
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    item_cost(&nonempty_item::<M>(&mut r, registry)?, &mut w)?;
                    w.bytes(&nonempty_item::<M>(&mut r, registry)?);
                    let mut item = Writer::new();
                    crate::vfp_items_1203::forward::<M>(&mut r, &mut item, registry)?;
                    let mut cost = Writer::new();
                    let present = item_cost(&item.data, &mut cost)?;
                    w.bool(present);
                    w.bytes(&cost.data);
                    w.bool(r.bool()?);
                    w.bytes(r.bytes(24)?);
                }
                w.bytes(r.rest());
            }
            "UPDATE_ADVANCEMENTS" => {
                w.bool(r.bool()?);
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    string(&mut r, &mut w)?;
                    let parent = r.bool()?;
                    w.bool(parent);
                    if parent {
                        string(&mut r, &mut w)?;
                    }
                    let display = r.bool()?;
                    w.bool(display);
                    if display {
                        w.nbt(&r.nbt()?);
                        w.nbt(&r.nbt()?);
                        item_template::<M>(&mut r, &mut w, registry)?;
                        w.varint(r.varint()?);
                        let flags = r.i32()?;
                        w.i32(flags);
                        if flags & 1 != 0 {
                            string(&mut r, &mut w)?;
                        }
                        w.bytes(r.bytes(8)?);
                    }
                    let requirements = r.count(65536)?;
                    w.varint(requirements as i32);
                    for _ in 0..requirements {
                        let names = r.count(65536)?;
                        w.varint(names as i32);
                        for _ in 0..names {
                            string(&mut r, &mut w)?;
                        }
                    }
                    w.bool(r.bool()?);
                }
                w.bytes(r.rest());
            }
            "LEVEL_PARTICLES" => w.bytes(&crate::vfp_entities_1203::level_particle::<M>(
                data, registry,
            )?),
            "EXPLODE" => {
                w.bytes(r.bytes(28)?);
                let blocks = r.count(1_048_576)?;
                w.varint(blocks as i32);
                w.bytes(r.bytes(blocks * 3 + 12)?);
                w.varint(r.varint()?);
                for _ in 0..2 {
                    crate::vfp_entities_1203::particle::<M>(&mut r, &mut w, None, registry)?;
                }
                w.varint(0);
                string(&mut r, &mut w)?;
                let range = r.bool()?;
                w.bool(range);
                if range {
                    w.f32(r.f32()?);
                }
                end(&r)?;
            }
            "SET_PLAYER_TEAM" => {
                let team = r.string(32767)?;
                let action = r.u8()?;
                w.string(&team);
                w.u8(action);
                if action == 0 || action == 2 {
                    w.nbt(&r.nbt()?);
                    w.u8(r.u8()?);
                    string(&mut r, &mut w)?;
                    string(&mut r, &mut w)?;
                    w.varint(r.varint()?);
                    w.nbt(&r.nbt()?);
                    w.nbt(&r.nbt()?);
                }
                if action == 0 {
                    self.teams.insert(team.clone(), BTreeSet::new());
                }
                if action == 1 {
                    self.teams.remove(&team);
                }
                if matches!(action, 0 | 3 | 4) {
                    let count = r.count(65536)?;
                    let mut names = Vec::new();
                    for _ in 0..count {
                        let player = r.string(32767)?;
                        if action != 4 {
                            for members in self.teams.values_mut() {
                                members.remove(&player);
                            }
                            self.teams
                                .entry(team.clone())
                                .or_default()
                                .insert(player.clone());
                            names.push(player);
                        } else if self
                            .teams
                            .get_mut(&team)
                            .is_some_and(|members| members.remove(&player))
                        {
                            names.push(player);
                        }
                    }
                    if action == 4 && names.is_empty() {
                        end(&r)?;
                        return Ok(Vec::new());
                    }
                    w.varint(names.len() as i32);
                    for player in names {
                        w.string(&player);
                    }
                }
                end(&r)?;
            }
            _ => w.bytes(data),
        }
        let mut result = vec![source(name, w.data)?];
        result.extend(after);
        Ok(result)
    }
    fn ack(&mut self, r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
        let offset = r.varint()?;
        if offset < 0 {
            return Err(Error::new("Negative chat acknowledgement offset"));
        }
        let total = offset
            .checked_add(self.delayed_acks)
            .ok_or_else(|| Error::new("Chat acknowledgement overflow"))?;
        self.delayed_acks = 0;
        self.last_seen.copy_from_slice(r.bytes(3)?);
        w.varint(total);
        w.bytes(&self.last_seen);
        end(r)
    }
    pub fn serverbound(&mut self, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let mut result = std::mem::take(&mut self.replies);
        for (id, data) in packets {
            let name = usize::try_from(id)
                .ok()
                .and_then(|id| next::SERVERBOUND.get(id))
                .copied()
                .ok_or_else(|| Error::new("Unknown 1.20.5 reply"))?;
            if matches!(name, "COOKIE_RESPONSE" | "DEBUG_SUBSCRIPTION_REQUEST") {
                continue;
            }
            let mut target = name;
            let mut r = Reader::new(&data);
            let mut w = Writer::new();
            match name {
                "CONTAINER_BUTTON_CLICK" => {
                    w.i8(r.varint()? as i8);
                    w.i8(r.varint()? as i8);
                    end(&r)?;
                }
                "CHAT_ACK" => {
                    let offset = r.varint()?;
                    end(&r)?;
                    if offset < 0 {
                        return Err(Error::new("Negative chat acknowledgement offset"));
                    }
                    self.delayed_acks = self
                        .delayed_acks
                        .checked_add(offset)
                        .ok_or_else(|| Error::new("Chat acknowledgement overflow"))?;
                    let forward = self.delayed_acks - 20;
                    if forward < 20 {
                        continue;
                    }
                    self.delayed_acks = 20;
                    self.last_seen = [0; 3];
                    w.varint(forward);
                }
                "CHAT" => {
                    string(&mut r, &mut w)?;
                    w.i64(r.i64()?);
                    let salt = r.i64()?;
                    let signed = r.bool()?;
                    let signature = if signed {
                        Some(r.bytes(256)?.to_vec())
                    } else {
                        None
                    };
                    let secure = self.secure_chat.unwrap_or(true);
                    w.i64(if secure { salt } else { 0 });
                    w.bool(secure && signed);
                    if secure {
                        if let Some(signature) = signature {
                            w.bytes(&signature);
                        }
                    }
                    self.ack(&mut r, &mut w)?;
                }
                "CHAT_COMMAND_SIGNED" => {
                    target = "CHAT_COMMAND";
                    string(&mut r, &mut w)?;
                    w.i64(r.i64()?);
                    let salt = r.i64()?;
                    let count = r.count(8)?;
                    let secure = self.secure_chat.unwrap_or(true);
                    w.i64(if secure { salt } else { 0 });
                    w.varint(if secure { count as i32 } else { 0 });
                    for _ in 0..count {
                        let name = r.string(32767)?;
                        let signature = r.bytes(256)?;
                        if secure {
                            w.string(&name);
                            w.bytes(signature);
                        }
                    }
                    self.ack(&mut r, &mut w)?;
                }
                "CHAT_COMMAND" => {
                    string(&mut r, &mut w)?;
                    end(&r)?;
                    let millis = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|_| Error::new("System clock predates epoch"))?
                        .as_millis();
                    w.i64(millis.min(i64::MAX as u128) as i64);
                    w.i64(0);
                    w.varint(0);
                    w.varint(0);
                    w.bytes(&self.last_seen);
                }
                "CHAT_SESSION_UPDATE" if self.secure_chat != Some(true) => {
                    if self.secure_chat.is_none() {
                        self.chat_session = Some(data);
                    }
                    continue;
                }
                _ => w.bytes(&data),
            }
            result.push((
                wire::packet_id(old::SERVERBOUND, target).map_err(Error::new)?,
                w.data,
            ));
        }
        Ok(result)
    }
}
