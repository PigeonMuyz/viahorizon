// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;

use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_18_data::{EFFECTS, EGGS, ENTITIES, METADATA, OBJECTS, POTIONS, SOUNDS},
    vfp_release_chain::Step,
    vfp_release_codec::{self as codec, Position, Stack},
    vfp_release_columns::Column,
    vfp_release_metadata::{self as meta, Entry, Value},
    vfp_wire::Packet,
};
use horizon_world::nbt::Tag;

#[derive(Default)]
pub struct State {
    player: Option<i32>,
    entities: BTreeMap<i32, &'static str>,
    vehicles: BTreeMap<i32, i32>,
    selected: usize,
    hotbar: [Option<Stack>; 9],
    windows: BTreeMap<u8, Menu>,
    hotbar_offsets: BTreeMap<u8, i16>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Brewing,
    Enchanting,
}

fn potion_index(damage: i32) -> usize {
    let damage = if damage >= 16384 {
        damage - 8192
    } else {
        damage
    };
    POTIONS
        .iter()
        .position(|&(id, _)| id == damage)
        .or_else(|| {
            POTIONS
                .iter()
                .position(|&(id, _)| id >= 8192 && (id & 0x7f) == (damage & 0x7f))
        })
        .unwrap_or(1)
}

fn stack(item: &mut Stack, inverse: bool) {
    const LIT: &str = "viafabricplus:1_8_lit_furnace";
    if matches!(item.tag, Tag::End) {
        item.tag = Tag::Compound(Default::default());
    }
    let Tag::Compound(tag) = &mut item.tag else {
        return;
    };
    if inverse {
        if item.id == 61 && tag.remove(&LIT.into()).is_some() {
            item.id = 62;
        }
        if item.id == 383 {
            if let Some(Tag::Compound(entity)) = tag.get_mut(&"EntityTag".into()) {
                if let Some(Tag::String(name)) = entity.remove(&"id".into()) {
                    if let Some((id, _)) = EGGS
                        .iter()
                        .find(|&&(_, candidate)| candidate == name.to_string_lossy())
                    {
                        item.damage = *id as i16;
                    }
                }
                if entity.is_empty() {
                    tag.remove(&"EntityTag".into());
                }
            }
        }
        if matches!(item.id, 373 | 438) {
            let offset = if item.id == 438 { 8192 } else { 0 };
            item.id = 373;
            if let Some(Tag::String(name)) = tag.remove(&"Potion".into()) {
                let name = name.to_string_lossy();
                let name = name.strip_prefix("minecraft:").unwrap_or(&name);
                if let Some((id, _)) = POTIONS.iter().find(|&&(_, candidate)| candidate == name) {
                    item.damage = (id + offset) as i16;
                }
            }
        }
    } else {
        if item.id == 62 {
            item.id = 61;
            tag.insert(LIT.into(), Tag::Byte(1));
        }
        if item.id == 383 && item.damage != 0 {
            if let Some(&(_, name)) = EGGS.iter().find(|&&(id, _)| id == i32::from(item.damage)) {
                let entity = tag
                    .entry("EntityTag".into())
                    .or_insert_with(|| Tag::Compound(Default::default()));
                if let Tag::Compound(entity) = entity {
                    entity.insert("id".into(), Tag::String(name.into()));
                }
                item.damage = 0;
            }
        }
        if item.id == 373 {
            if item.damage >= 16384 {
                item.id = 438;
            }
            let name = POTIONS
                .get(potion_index(i32::from(item.damage)))
                .map_or("water", |&(_, name)| name);
            tag.insert(
                "Potion".into(),
                Tag::String(format!("minecraft:{name}").as_str().into()),
            );
            item.damage = 0;
        }
    }
    if tag.is_empty() {
        item.tag = Tag::End;
    }
}

fn item(step: &Step, r: &mut Reader<'_>, w: &mut Writer, inverse: bool) -> Result<Option<Stack>> {
    let (source, target) = if inverse {
        (step.target, step.source)
    } else {
        (step.source, step.target)
    };
    let mut value = Stack::read(r, source)?;
    if let Some(value) = &mut value {
        stack(value, inverse);
    }
    Stack::write(value.as_ref(), w, target)?;
    Ok(value)
}

fn old_uuid() -> Result<u128> {
    let mut bytes = [0; 16];
    crate::crypto::random_bytes(&mut bytes)?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    Ok(u128::from_be_bytes(bytes))
}

fn fixed_position(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    for _ in 0..3 {
        w.f64(f64::from(r.i32()?) / 32.0);
    }
    Ok(())
}

fn metadata(
    step: &Step,
    r: &mut Reader<'_>,
    w: &mut Writer,
    kind: &str,
    spawn: bool,
) -> Result<()> {
    let mut output = BTreeMap::new();
    for mut entry in meta::read(r, step.source)? {
        let mut family = kind;
        let mapping = loop {
            if let Some(&row) = METADATA
                .iter()
                .find(|&&(name, index, _, _, _)| name == family && index == entry.index)
            {
                break Some(row);
            }
            if family.is_empty() {
                break None;
            }
            family = ENTITIES
                .iter()
                .find_map(|&(name, parent, _)| (name == family).then_some(parent))
                .unwrap_or("");
        };
        let Some((family, old_index, old_kind, index, new_kind)) = mapping else {
            continue;
        };
        if new_kind < 0 || i32::from(old_kind) != entry.kind {
            continue;
        }
        if kind == "PLAYER" && old_index == 0 {
            let flags = entry.integer()?;
            output.insert(5, Entry::int(5, 0, i32::from((flags & 16) != 0)));
            entry.value = Value::Int(flags & !16);
        }
        entry.index = index;
        entry.kind = i32::from(new_kind);
        match new_kind {
            0 | 1 | 10 | 12 => entry.value = Value::Int(entry.integer()?),
            4 => {
                if let Value::Text(value) = &mut entry.value {
                    if serde_json::from_str::<serde_json::Value>(value).is_err() {
                        *value = crate::vfp_release_text::json(value);
                    }
                }
            }
            5 => {
                if let Value::Item(Some(item)) = &mut entry.value {
                    stack(item, false);
                }
            }
            6 => {
                let value = entry.integer()?;
                entry.value = Value::Bool(if family == "ABSTRACT_AGEABLE" && old_index == 12 {
                    value < 0
                } else {
                    value != 0
                });
            }
            9 => {
                if let Value::Position(position) = entry.value {
                    entry.value = Value::OptionalPosition(Some(position));
                }
            }
            11 => {
                let uuid = if let Value::Text(owner) = &entry.value {
                    let value = owner.replace('-', "");
                    (value.len() == 32)
                        .then(|| u128::from_str_radix(&value, 16).ok())
                        .flatten()
                } else {
                    None
                };
                entry.value = Value::Uuid(uuid);
            }
            _ => (),
        }
        output.insert(entry.index, entry);
    }
    if spawn && kind == "ENDER_DRAGON" {
        output.insert(11, Entry::int(11, 1, 0));
    }
    meta::write(w, &output.into_values().collect::<Vec<_>>(), step.target)
}

fn bulk(step: &Step, data: &[u8]) -> Result<Vec<Packet>> {
    let mut r = Reader::new(data);
    let sky = codec::boolean(&mut r)?;
    let count = r.count(1024)?;
    let mut columns = Vec::with_capacity(count);
    for _ in 0..count {
        columns.push((r.i32()?, r.i32()?, r.u16()?));
    }
    let mut output = Vec::with_capacity(count);
    for (x, z, mask) in columns {
        let size = mask.count_ones() as usize * (8192 + 2048 + if sky { 2048 } else { 0 }) + 256;
        let mut source = Writer::new();
        source.i32(x);
        source.i32(z);
        source.bool(true);
        source.u16(mask);
        source.varint(size as i32);
        source.bytes(r.bytes(size)?);
        let mut column = Column::read(step, &source.data, sky)?;
        column.remap(step)?;
        output.push(step.incoming("LEVEL_CHUNK", column.write(step, sky)?)?);
    }
    codec::end(&r)?;
    Ok(output)
}

fn attack_speed(w: &mut Writer) {
    w.string("generic.attackSpeed");
    w.f64(20.0);
    // Use the standard weapon modifier identifier so the client's equipped
    // item does not apply the attack-speed penalty introduced in 1.9.
    w.varint(3);
    for (id, operation) in [
        (0xfa233e1c_4180_4865_b01b_bcce9785aca3, 0),
        (0xaf8b6e3f_3328_4c0a_aa36_5ba2bb9dbef3, 2),
        (0x55fced67_e92a_486e_9800_b47f202c4386, 2),
    ] {
        w.uuid(id);
        w.f64(0.0);
        w.u8(operation);
    }
}

impl State {
    pub fn reset_world(&mut self) {
        self.entities.clear();
        self.vehicles.clear();
        self.windows.clear();
        self.hotbar_offsets.clear();
        if let Some(id) = self.player {
            self.entities.insert(id, "PLAYER");
        }
    }

    fn equipment(step: &Step, entity: i32, slot: i32, stack: Option<&Stack>) -> Result<Packet> {
        let mut w = Writer::new();
        w.varint(entity);
        w.varint(slot);
        Stack::write(stack, &mut w, step.target)?;
        step.incoming("SET_EQUIPPED_ITEM", w.data)
    }

    fn remember(&mut self, window: u8, slot: i16, mut item: Option<Stack>) {
        let offset = if matches!(window, 0 | 254) {
            36
        } else if let Some(&offset) = self.hotbar_offsets.get(&window) {
            offset
        } else {
            return;
        };
        if (offset..offset + 9).contains(&slot) {
            if let Some(value) = &mut item {
                stack(value, true);
            }
            self.hotbar[(slot - offset) as usize] = item;
        }
    }

    pub fn clientbound(
        &mut self,
        step: &Step,
        name: &str,
        data: &[u8],
    ) -> Result<Option<Vec<Packet>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut after = Vec::new();
        let mut target = name;
        match name {
            "LOGIN" | "RESPAWN" => {
                if name == "LOGIN" {
                    self.player = Some(r.i32()?);
                    self.hotbar.fill(None);
                    self.selected = 0;
                }
                self.reset_world();
                if let Some(entity) = self.player {
                    let mut attrs = Writer::new();
                    attrs.varint(entity);
                    attrs.i32(1);
                    attack_speed(&mut attrs);
                    return Ok(Some(vec![
                        step.incoming(name, data.to_vec())?,
                        step.incoming("UPDATE_ATTRIBUTES", attrs.data)?,
                    ]));
                }
                return Ok(None);
            }
            "UPDATE_ATTRIBUTES" => {
                let entity = r.varint()?;
                if self.player != Some(entity) {
                    return Ok(None);
                }
                let count = r.i32()?;
                if !(0..=4096).contains(&count) {
                    return Err(Error::new("Invalid 1.8 attribute count"));
                }
                let mut fields = Writer::new();
                let mut written = 0;
                for _ in 0..count {
                    let start = r.position();
                    let name = r.string(32767)?;
                    r.f64()?;
                    let modifiers = r.count(4096)?;
                    r.bytes(modifiers * 25)?;
                    if name != "generic.attackSpeed" {
                        fields.bytes(r.since(start));
                        written += 1;
                    }
                }
                w.varint(entity);
                w.i32(written + 1);
                w.bytes(&fields.data);
                attack_speed(&mut w);
            }
            "MAP_BULK_CHUNK" => return Ok(Some(bulk(step, data)?)),
            "ADD_ENTITY" | "ADD_MOB" | "ADD_PLAYER" | "ADD_PAINTING" => {
                let id = r.varint()?;
                w.varint(id);
                w.uuid(if name == "ADD_PLAYER" {
                    r.uuid()?
                } else {
                    old_uuid()?
                });
                let kind = match name {
                    "ADD_ENTITY" => {
                        let object = r.u8()?;
                        w.u8(object);
                        fixed_position(&mut r, &mut w)?;
                        w.bytes(r.bytes(2)?);
                        let data = r.i32()?;
                        w.i32(data);
                        if data > 0 {
                            w.bytes(r.bytes(6)?);
                        } else {
                            w.bytes(&[0; 6]);
                        }
                        if object == 73 {
                            let mut potion = Stack {
                                id: 373,
                                count: 1,
                                damage: data as i16,
                                tag: Tag::End,
                            };
                            stack(&mut potion, false);
                            let mut metadata = Writer::new();
                            metadata.varint(id);
                            meta::write(
                                &mut metadata,
                                &[Entry {
                                    index: 6,
                                    kind: 5,
                                    value: Value::Item(Some(potion)),
                                }],
                                step.target,
                            )?;
                            after.push(step.incoming("SET_ENTITY_DATA", metadata.data)?);
                        }
                        let lookup = |key| {
                            OBJECTS.iter().find_map(|&(ty, variant, kind)| {
                                (ty == i32::from(object) && variant == key).then_some(kind)
                            })
                        };
                        lookup(data).or_else(|| lookup(-1)).unwrap_or("ENTITY")
                    }
                    "ADD_MOB" => {
                        let ty = r.u8()?;
                        w.u8(ty);
                        fixed_position(&mut r, &mut w)?;
                        w.bytes(r.bytes(9)?);
                        let kind = ENTITIES
                            .iter()
                            .find_map(|&(name, _, id)| (id == i32::from(ty)).then_some(name))
                            .unwrap_or("ENTITY");
                        metadata(step, &mut r, &mut w, kind, true)?;
                        kind
                    }
                    "ADD_PLAYER" => {
                        fixed_position(&mut r, &mut w)?;
                        w.bytes(r.bytes(2)?);
                        let held = r.i16()?;
                        metadata(step, &mut r, &mut w, "PLAYER", true)?;
                        if held > 0 {
                            let mut held = Stack {
                                id: i32::from(held),
                                count: 1,
                                damage: 0,
                                tag: Tag::End,
                            };
                            stack(&mut held, false);
                            after.push(Self::equipment(step, id, 0, Some(&held))?);
                        }
                        "PLAYER"
                    }
                    "ADD_PAINTING" => {
                        w.bytes(r.rest());
                        "PAINTING"
                    }
                    _ => unreachable!(),
                };
                self.entities.insert(id, kind);
            }
            "ADD_EXPERIENCE_ORB" | "ADD_GLOBAL_ENTITY" | "TELEPORT_ENTITY" => {
                w.varint(r.varint()?);
                if name == "ADD_GLOBAL_ENTITY" {
                    w.u8(r.u8()?);
                }
                fixed_position(&mut r, &mut w)?;
                w.bytes(r.rest());
            }
            "SET_ENTITY_DATA" => {
                let id = r.varint()?;
                w.varint(id);
                let kind = self.entities.get(&id).copied().unwrap_or("ENTITY");
                metadata(step, &mut r, &mut w, kind, false)?;
            }
            "MOVE_ENTITY_POS" | "MOVE_ENTITY_POS_ROT" => {
                w.varint(r.varint()?);
                for _ in 0..3 {
                    w.i16(i16::from(r.i8()?) * 128);
                }
                w.bytes(r.rest());
            }
            "SET_ENTITY_LINK" => {
                let passenger = r.i32()?;
                let vehicle = r.i32()?;
                if codec::boolean(&mut r)? {
                    w.i32(passenger);
                    w.i32(vehicle);
                } else {
                    target = "SET_PASSENGERS";
                    if vehicle == -1 {
                        let Some(vehicle) = self.vehicles.remove(&passenger) else {
                            return Ok(Some(Vec::new()));
                        };
                        w.varint(vehicle);
                        w.varint(0);
                    } else {
                        self.vehicles.insert(passenger, vehicle);
                        w.varint(vehicle);
                        w.varint(1);
                        w.varint(passenger);
                    }
                }
            }
            "REMOVE_ENTITIES" => {
                let count = r.count(65536)?;
                for _ in 0..count {
                    let id = r.varint()?;
                    self.entities.remove(&id);
                    self.vehicles.remove(&id);
                }
                codec::end(&r)?;
                return Ok(None);
            }
            "SET_EQUIPPED_ITEM" => {
                let entity = r.varint()?;
                let slot = r.i16()?;
                let local = self.player == Some(entity);
                if !(0..=4).contains(&slot) || (local && slot > 3) {
                    return Ok(Some(Vec::new()));
                }
                w.varint(entity);
                w.varint(i32::from(if local {
                    slot + 2
                } else if slot > 0 {
                    slot + 1
                } else {
                    slot
                }));
                item(step, &mut r, &mut w, false)?;
            }
            "PLAYER_POSITION" => {
                w.bytes(r.bytes(33)?);
                w.varint(0);
            }
            "SET_CARRIED_ITEM" => {
                let slot = r.u8()?;
                if slot < 9 {
                    self.selected = usize::from(slot);
                }
                w.u8(slot);
            }
            "SET_PLAYER_TEAM" => {
                w.string(&r.string(32767)?);
                let mode = r.u8()?;
                w.u8(mode);
                if matches!(mode, 0 | 2) {
                    for _ in 0..3 {
                        w.string(&r.string(32767)?);
                    }
                    w.u8(r.u8()?);
                    w.string(&r.string(32767)?);
                    w.string("always");
                    w.i8(r.i8()?);
                }
                w.bytes(r.rest());
            }
            "MAP_ITEM_DATA" => {
                w.varint(r.varint()?);
                w.u8(r.u8()?);
                w.bool(true);
                w.bytes(r.rest());
            }
            "CHAT" | "DISCONNECT" | "TAB_LIST" => {
                let value = r.string(262144)?;
                w.string(&crate::vfp_release_text::component(&value));
                match name {
                    "CHAT" => w.u8(r.u8()?),
                    "TAB_LIST" => {
                        let footer = r.string(262144)?;
                        w.string(&crate::vfp_release_text::component(&footer));
                    }
                    _ => (),
                }
            }
            "SET_TITLES" => {
                let action = r.varint()?;
                w.varint(action);
                if matches!(action, 0 | 1) {
                    let title = r.string(262144)?;
                    w.string(&crate::vfp_release_text::component(&title));
                } else {
                    w.bytes(r.rest());
                }
            }
            "CUSTOM_SOUND" => {
                let sound = r.string(32767)?;
                let name = sound.strip_prefix("minecraft:").unwrap_or(&sound);
                let (name, category) = SOUNDS
                    .iter()
                    .find_map(|&(old, new, category)| (old == name).then_some((new, category)))
                    .unwrap_or((name, 0));
                w.string(name);
                w.varint(category);
                w.bytes(r.rest());
            }
            "LEVEL_EVENT" => {
                let event = r.i32()?;
                let event = EFFECTS
                    .iter()
                    .find_map(|&(old, new)| (old == event).then_some(new))
                    .unwrap_or(event);
                if event < 0 {
                    return Ok(Some(Vec::new()));
                }
                w.i32(event);
                w.i64(r.i64()?);
                let value = r.i32()?;
                w.i32(if event == 2002 {
                    potion_index(value) as i32
                } else {
                    value
                });
                w.bool(codec::boolean(&mut r)?);
            }
            "BLOCK_ENTITY_DATA" => {
                w.i64(r.i64()?);
                let action = r.u8()?;
                w.u8(action);
                let mut tag = r.vfp_named_nbt()?;
                if action == 1 {
                    if let Tag::Compound(fields) = &mut tag {
                        let entity = fields
                            .get(&"EntityId".into())
                            .cloned()
                            .unwrap_or(Tag::String("AreaEffectCloud".into()));
                        fields.insert("SpawnData".into(), Tag::compound([("id", entity)]));
                    }
                }
                w.vfp_named_nbt(&tag)?;
            }
            "OPEN_SCREEN" => {
                let window = r.u8()?;
                let kind = r.string(32767)?;
                self.hotbar_offsets.remove(&window);
                let menu = match kind.as_str() {
                    "minecraft:brewing_stand" => Some(Menu::Brewing),
                    "minecraft:enchanting_table" => Some(Menu::Enchanting),
                    _ => None,
                };
                if let Some(menu) = menu {
                    self.windows.insert(window, menu);
                } else {
                    self.windows.remove(&window);
                }
                w.u8(window);
                w.string(&kind);
                w.string(&r.string(262144)?);
                let slots = r.u8()?;
                w.u8(if menu == Some(Menu::Brewing) {
                    slots.saturating_add(1)
                } else {
                    slots
                });
                w.bytes(r.rest());
            }
            "CONTAINER_CLOSE" => {
                let window = r.u8()?;
                self.windows.remove(&window);
                self.hotbar_offsets.remove(&window);
                codec::end(&r)?;
                w.u8(window);
            }
            "CONTAINER_SET_DATA" => {
                let window = r.u8()?;
                let property = r.i16()?;
                let value = r.i16()?;
                w.u8(window);
                w.i16(property);
                if self.windows.get(&window) == Some(&Menu::Enchanting)
                    && (4..7).contains(&property)
                    && value >= 0
                {
                    w.i16(value & 255);
                    let mut level = Writer::new();
                    level.u8(window);
                    level.i16(property + 3);
                    level.i16(value >> 8);
                    after.push(step.incoming(name, level.data)?);
                } else {
                    w.i16(value);
                }
            }
            "CONTAINER_SET_SLOT" => {
                let window = r.u8()?;
                let slot = r.i16()?;
                w.u8(window);
                w.i16(
                    if slot >= 4 && self.windows.get(&window) == Some(&Menu::Brewing) {
                        slot.checked_add(1)
                            .ok_or_else(|| Error::new("Brewing slot overflow"))?
                    } else {
                        slot
                    },
                );
                let value = item(step, &mut r, &mut w, false)?;
                self.remember(window, slot, value);
            }
            "CONTAINER_SET_CONTENT" => {
                let window = r.u8()?;
                let count = r.i16()?;
                if !(0..=4096).contains(&count) {
                    return Err(Error::new("Invalid 1.8 window size"));
                }
                if (1..=253).contains(&window) && count >= 36 {
                    self.hotbar_offsets.insert(window, count - 9);
                }
                let brewing = self.windows.get(&window) == Some(&Menu::Brewing) && count >= 4;
                w.u8(window);
                w.i16(count + i16::from(brewing || (window == 0 && count == 45)));
                for slot in 0..count {
                    if brewing && slot == 4 {
                        Stack::write(None, &mut w, step.target)?;
                    }
                    let value = item(step, &mut r, &mut w, false)?;
                    self.remember(window, slot, value);
                }
                if (brewing && count == 4) || (window == 0 && count == 45) {
                    Stack::write(None, &mut w, step.target)?;
                }
            }
            "UPDATE_MOB_EFFECT" => {
                w.varint(r.varint()?);
                w.bytes(r.bytes(2)?);
                w.varint(r.varint()?);
                w.u8(if codec::boolean(&mut r)? { 2 } else { 0 });
            }
            "CUSTOM_PAYLOAD" => {
                let channel = r.string(32767)?;
                w.string(&channel);
                if channel == "MC|TrList" {
                    w.i32(r.i32()?);
                    let count = r.u8()?;
                    w.u8(count);
                    for _ in 0..count {
                        item(step, &mut r, &mut w, false)?;
                        item(step, &mut r, &mut w, false)?;
                        let second = codec::boolean(&mut r)?;
                        w.bool(second);
                        if second {
                            item(step, &mut r, &mut w, false)?;
                        }
                        w.bytes(r.bytes(9)?);
                    }
                } else {
                    w.bytes(r.rest());
                }
            }
            "UPDATE_ENTITY_NBT" | "SET_COMPRESSION" => return Ok(Some(Vec::new())),
            _ => return Ok(None),
        }
        codec::end(&r)?;
        let mut output = vec![step.incoming(target, w.data)?];
        output.extend(after);
        Ok(Some(output))
    }

    pub fn serverbound(
        &mut self,
        step: &Step,
        name: &str,
        data: &[u8],
        held: Option<&Option<Stack>>,
    ) -> Result<Option<Vec<Packet>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut target = name;
        match name {
            "ACCEPT_TELEPORTATION" | "MOVE_VEHICLE" | "PADDLE_BOAT" => return Ok(Some(Vec::new())),
            "SWING" => {
                if r.varint()? != 0 {
                    return Ok(Some(Vec::new()));
                }
            }
            "INTERACT" => {
                w.varint(r.varint()?);
                let action = r.count(2)?;
                w.varint(action as i32);
                if action == 2 {
                    w.bytes(r.bytes(12)?);
                }
                if action != 1 && r.varint()? != 0 {
                    return Ok(Some(Vec::new()));
                }
            }
            "PLAYER_COMMAND" => {
                w.varint(r.varint()?);
                let action = r.varint()?;
                if matches!(action, 6 | 8) {
                    return Ok(Some(Vec::new()));
                }
                w.varint(if action == 7 { 6 } else { action });
                w.varint(r.varint()?);
            }
            "PLAYER_ACTION" => {
                let action = r.varint()?;
                if action == 6 {
                    return Ok(Some(Vec::new()));
                }
                w.varint(action);
                w.bytes(r.bytes(9)?);
            }
            "USE_ITEM" | "USE_ITEM_ON" => {
                target = "USE_ITEM_ON";
                let cursor = if name == "USE_ITEM" {
                    if r.varint()? != 0 {
                        return Ok(Some(Vec::new()));
                    }
                    w.i64(
                        Position {
                            x: -1,
                            y: -1,
                            z: -1,
                        }
                        .pack(step.source)?,
                    );
                    w.u8(255);
                    [0, 0, 0]
                } else {
                    w.i64(r.i64()?);
                    w.u8(r.count(5)? as u8);
                    if r.varint()? != 0 {
                        return Ok(Some(Vec::new()));
                    }
                    [r.u8()?, r.u8()?, r.u8()?]
                };
                let mut predicted = held.cloned();
                if let Some(Some(item)) = &mut predicted {
                    stack(item, true);
                }
                let current = predicted.as_ref().unwrap_or(&self.hotbar[self.selected]);
                Stack::write(current.as_ref(), &mut w, step.source)?;
                w.bytes(&cursor);
            }
            "COMMAND_SUGGESTION" => {
                w.string(&r.string(32767)?);
                codec::boolean(&mut r)?;
                w.bytes(r.rest());
            }
            "CLIENT_INFORMATION" => {
                w.string(&r.string(32767)?);
                w.i8(r.i8()?);
                w.u8(r.count(2)? as u8);
                w.bool(codec::boolean(&mut r)?);
                let skin = r.u8()?;
                let hand = r.varint()?;
                w.u8(if hand == 0 { skin | 128 } else { skin });
            }
            "SET_CARRIED_ITEM" => {
                let slot = r.i16()?;
                if !(0..9).contains(&slot) {
                    return Ok(Some(Vec::new()));
                }
                self.selected = slot as usize;
                w.i16(slot);
            }
            "CONTAINER_CLOSE" => {
                let window = r.u8()?;
                self.windows.remove(&window);
                self.hotbar_offsets.remove(&window);
                w.u8(window);
            }
            "CONTAINER_CLICK" | "SET_CREATIVE_MODE_SLOT" => {
                let slot = if name == "CONTAINER_CLICK" {
                    let window = r.u8()?;
                    let mut slot = r.i16()?;
                    if (window == 0 && slot == 45)
                        || (slot == 4 && self.windows.get(&window) == Some(&Menu::Brewing))
                    {
                        return Ok(Some(Vec::new()));
                    }
                    if slot > 4 && self.windows.get(&window) == Some(&Menu::Brewing) {
                        slot -= 1;
                    }
                    w.u8(window);
                    w.i16(slot);
                    w.u8(r.u8()?);
                    w.i16(r.i16()?);
                    w.u8(r.count(6)? as u8);
                    slot
                } else {
                    let slot = r.i16()?;
                    if slot == 45 {
                        return Ok(Some(Vec::new()));
                    }
                    w.i16(slot);
                    slot
                };
                let mut value = Stack::read(&mut r, step.target)?;
                if let Some(value) = &mut value {
                    if matches!(value.id, 198..=212 | 432..=437 | 439..=448) {
                        return Ok(Some(Vec::new()));
                    }
                    stack(value, true);
                }
                Stack::write(value.as_ref(), &mut w, step.source)?;
                codec::end(&r)?;
                if name == "SET_CREATIVE_MODE_SLOT" && (36..45).contains(&slot) {
                    self.hotbar[(slot - 36) as usize] = value;
                }
            }
            "SIGN_UPDATE" => {
                w.i64(r.i64()?);
                for _ in 0..4 {
                    let text = r.string(262144)?;
                    w.string(&serde_json::json!({"text":text}).to_string());
                }
            }
            "CUSTOM_PAYLOAD" => {
                let channel = r.string(32767)?;
                match channel.as_str() {
                    "MC|BSign" | "MC|BEdit" => {
                        w.string(&channel);
                        let mut book = Stack::read(&mut r, step.target)?;
                        if channel == "MC|BSign" {
                            if let Some(book) = &mut book {
                                book.id = 387;
                                if let Some(Tag::List(pages)) = book.tag.get_mut("pages") {
                                    for page in pages {
                                        if let Tag::String(page) = page {
                                            *page =
                                                serde_json::json!({"text":page.to_string_lossy()})
                                                    .to_string()
                                                    .as_str()
                                                    .into();
                                        }
                                    }
                                }
                            }
                        }
                        Stack::write(book.as_ref(), &mut w, step.source)?;
                    }
                    "MC|AutoCmd" => {
                        w.string("MC|AdvCdm");
                        w.u8(0);
                        w.bytes(r.bytes(12)?);
                        w.string(&r.string(32767)?);
                        w.bool(codec::boolean(&mut r)?);
                        r.rest();
                    }
                    "MC|AdvCmd" => {
                        w.string("MC|AdvCdm");
                        w.bytes(r.rest());
                    }
                    _ => {
                        w.string(&channel);
                        w.bytes(r.rest());
                    }
                }
            }
            _ => return Ok(None),
        }
        codec::end(&r)?;
        Ok(Some(vec![step.outgoing(target, w.data)?]))
    }
}
