// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::{self as chain, Step},
    vfp_release_chain_data::{FAMILIES_114, OBJECTS_113},
    vfp_release_codec::{self as codec, Position, Stack},
    vfp_release_metadata::{self as meta, Entry, Value},
    vfp_release_rewrite as rewrite,
    vfp_wire::Packet,
};

const VIEW_DISTANCE: i32 = 64;
const LIVING: u8 = 1;
const MOB: u8 = 2;
const ZOMBIE: u8 = 4;
const SKELETON: u8 = 8;
const ILLAGER: u8 = 16;
const RAIDER: u8 = 32;
const ARROW: u8 = 64;
const MINECART: u8 = 128;

#[derive(Default)]
pub struct State {
    center: Option<(i32, i32)>,
    player: Option<i32>,
    entities: BTreeMap<i32, Entity>,
    slash_requests: BTreeSet<i32>,
}

#[derive(Default)]
struct Entity {
    kind: &'static str,
    mob_flags: i32,
    player_flags: i32,
    sleeping: bool,
    riptide: bool,
}

impl Entity {
    fn pose(&self) -> Entry {
        let pose = if self.player_flags & 128 != 0 {
            1
        } else if self.sleeping {
            2
        } else if self.player_flags & 16 != 0 {
            3
        } else if self.riptide {
            4
        } else if self.player_flags & 2 != 0 {
            5
        } else {
            0
        };
        Entry::int(6, 18, pose)
    }

    fn attacking(&mut self, value: bool) {
        self.mob_flags = (self.mob_flags & !4) | if value { 4 } else { 0 };
    }
}

pub fn recipes(step: &Step, data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    let count = r.count(65536)?;
    let mut written = 0;
    for _ in 0..count {
        let id = r.string(32767)?;
        let kind = r.string(32767)?;
        let serializer = kind.strip_prefix("minecraft:").unwrap_or(&kind);
        if step.target >= 477 {
            if matches!(
                serializer,
                "crafting_special_banneraddpattern" | "crafting_special_repairitem"
            ) {
                continue;
            }
            w.string(&kind);
            w.string(&id);
        } else {
            w.string(&id);
            w.string(&kind);
        }
        written += 1;
        match serializer {
            "crafting_shaped" => {
                let width = r.varint()?;
                let height = r.varint()?;
                if !(1..=3).contains(&width) || !(1..=3).contains(&height) {
                    return Err(Error::new("Invalid 1.13 recipe dimensions"));
                }
                w.varint(width);
                w.varint(height);
                w.string(&r.string(32767)?);
                for _ in 0..width * height {
                    rewrite::ingredient(step, false, &mut r, &mut w)?;
                }
                rewrite::item(step, false, &mut r, &mut w)?;
            }
            "crafting_shapeless" => {
                w.string(&r.string(32767)?);
                let count = r.count(9)?;
                w.varint(count as i32);
                for _ in 0..count {
                    rewrite::ingredient(step, false, &mut r, &mut w)?;
                }
                rewrite::item(step, false, &mut r, &mut w)?;
            }
            "smelting" => {
                w.string(&r.string(32767)?);
                rewrite::ingredient(step, false, &mut r, &mut w)?;
                rewrite::item(step, false, &mut r, &mut w)?;
                w.f32(r.f32()?);
                w.varint(r.varint()?);
            }
            kind if kind.starts_with("crafting_special_") => (),
            _ => return Err(Error::new("Unknown 1.13 recipe kind")),
        }
    }
    codec::end(&r)?;
    let mut packet = Writer::new();
    packet.varint(written);
    packet.bytes(&w.data);
    Ok(packet.data)
}

fn position(step: &Step, inverse: bool, r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let (source, target) = if inverse {
        (step.target, step.source)
    } else {
        (step.source, step.target)
    };
    w.i64(Position::unpack(r.i64()?, source).pack(target)?);
    Ok(())
}

fn families(kind: &str) -> u8 {
    FAMILIES_114
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or(0, |(_, flags)| *flags)
}

pub fn object_kind(object: i32, data: i32) -> Option<i32> {
    let lookup = |variant| {
        OBJECTS_113
            .iter()
            .find(|&&(kind, key, _)| kind == object && key == variant)
            .map(|&(_, _, kind)| kind)
    };
    lookup(data).or_else(|| lookup(-1))
}

fn metadata_packet(step: &Step, entity: i32, entries: &[Entry]) -> Result<Packet> {
    let mut w = Writer::new();
    w.varint(entity);
    meta::write(&mut w, entries, step.target)?;
    step.incoming("SET_ENTITY_DATA", w.data)
}
impl State {
    pub fn reset(&mut self) {
        self.center = None;
        self.entities.clear();
        self.slash_requests.clear();
    }

    pub fn chunk_center(&mut self, step: &Step, x: i32, z: i32) -> Result<Option<Packet>> {
        if self.center.is_some_and(|(cx, cz)| {
            x.abs_diff(cx) < VIEW_DISTANCE as u32 && z.abs_diff(cz) < VIEW_DISTANCE as u32
        }) {
            return Ok(None);
        }
        self.center = Some((x, z));
        let mut w = Writer::new();
        w.varint(x);
        w.varint(z);
        Ok(Some(step.incoming("SET_CHUNK_CACHE_CENTER", w.data)?))
    }

    fn metadata(
        &mut self,
        step: &Step,
        id: i32,
        r: &mut Reader<'_>,
        w: &mut Writer,
    ) -> Result<Vec<Packet>> {
        let mut extra = Vec::new();
        let entity = self.entities.entry(id).or_default();
        let kind = entity.kind;
        let family = families(kind);
        let mut output = BTreeMap::new();
        let mut pose_changed = false;
        let mut flags_changed = false;
        for mut entry in meta::read(r, step.source)? {
            if entry.index > 250 {
                continue;
            }
            entry.remap(step)?;
            if step.source == 393 {
                if family & MINECART != 0 && entry.index == 9 {
                    entry.value =
                        Value::Int(step.same_or_map("blockstates", entry.integer()?, false)?);
                }
                if family & ARROW != 0 && entry.index >= 7 {
                    entry.index += 1;
                }
            } else {
                if entry.index >= 6 {
                    entry.index += 1;
                }
                if family & MINECART != 0 && entry.index == 10 {
                    entry.value =
                        Value::Int(step.same_or_map("blockstates", entry.integer()?, false)?);
                }
                if family & LIVING != 0 && entry.index >= 12 {
                    entry.index += 1;
                }
                if family & LIVING != 0
                    && entry.index == 8
                    && matches!(&entry.value, Value::Float(value) if value.is_nan())
                {
                    entry.value = Value::Float(1.0);
                }
                if family & MOB != 0 && entry.index == 13 {
                    entity.mob_flags = (entry.integer()? & !4) | (entity.mob_flags & 4);
                    flags_changed = true;
                    continue;
                }
                if kind == "player" && self.player != Some(id) {
                    match entry.index {
                        0 => {
                            entity.player_flags = entry.integer()?;
                            pose_changed = true;
                        }
                        7 => {
                            entity.riptide = entry.integer()? & 4 != 0;
                            pose_changed = true;
                        }
                        _ => (),
                    }
                }
                if family & ZOMBIE != 0 {
                    if entry.index == 16 {
                        entity.attacking(entry.integer()? != 0);
                        flags_changed = true;
                        continue;
                    }
                    if entry.index > 16 {
                        entry.index -= 1;
                    }
                }
                if kind == "horse" && entry.index == 18 {
                    let armor = entry.integer()?;
                    let stack = if (1..=3).contains(&armor) {
                        Some(Stack {
                            id: step.same_or_map("items", 726 + armor, false)?,
                            count: 1,
                            damage: 0,
                            tag: horizon_world::nbt::Tag::End,
                        })
                    } else {
                        None
                    };
                    let mut equip = Writer::new();
                    equip.varint(id);
                    equip.varint(4);
                    Stack::write(stack.as_ref(), &mut equip, step.target)?;
                    extra.push(step.incoming("SET_EQUIPPED_ITEM", equip.data)?);
                    continue;
                }
                if (kind == "villager" && entry.index == 15)
                    || (kind == "zombie_villager" && entry.index == 18)
                {
                    let profession = usize::try_from(entry.integer()?)
                        .ok()
                        .and_then(|i| [5, 9, 4, 1, 2, 11].get(i).copied())
                        .unwrap_or(0);
                    entry.kind = 16;
                    entry.value = Value::Villager([2, profession, 0]);
                }
                if family & ARROW != 0 && entry.index >= 9 {
                    entry.index += 1;
                }
                if kind == "firework_rocket" && entry.index == 8 {
                    let attached = entry.integer()?;
                    entry = Entry::int(
                        8,
                        17,
                        if attached == 0 {
                            0
                        } else {
                            attached
                                .checked_add(1)
                                .ok_or_else(|| Error::new("Invalid firework entity ID"))?
                        },
                    );
                }
                if (family & SKELETON != 0 || family & ILLAGER != 0) && entry.index == 14 {
                    entity.attacking(entry.integer()? != 0);
                    flags_changed = true;
                    continue;
                }
                if family & ILLAGER != 0 && entry.index > 14 {
                    entry.index -= 1;
                }
                if kind == "ocelot" {
                    if (15..=17).contains(&entry.index) {
                        continue;
                    }
                    if entry.index > 17 {
                        entry.index -= 3;
                    }
                }
                if family & RAIDER != 0 && entry.index >= 14 {
                    entry.index += 1;
                }
            }
            output.insert(entry.index, entry);
        }
        if pose_changed {
            output.insert(6, entity.pose());
        }
        if flags_changed {
            output.insert(13, Entry::int(13, 0, entity.mob_flags));
        }
        meta::write(w, &output.into_values().collect::<Vec<_>>(), step.target)?;
        Ok(extra)
    }

    fn sleep(&mut self, step: &Step, id: i32, position: Option<Position>) -> Result<Packet> {
        let entity = self.entities.entry(id).or_default();
        entity.sleeping = position.is_some();
        let mut entries = vec![Entry {
            index: 12,
            kind: 10,
            value: Value::OptionalPosition(position),
        }];
        if self.player != Some(id) {
            entries.push(entity.pose());
        }
        metadata_packet(step, id, &entries)
    }

    pub fn clientbound(
        &mut self,
        step: &Step,
        name: &str,
        data: &[u8],
    ) -> Result<Option<Vec<Packet>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut extra = Vec::new();
        let mut target_name = name;
        let village = step.source == 404;
        match name {
            "LOGIN" | "RESPAWN" => {
                self.reset();
                if name == "LOGIN" {
                    self.player = Some(r.i32()?);
                }
                if let Some(id) = self.player {
                    self.entities.insert(
                        id,
                        Entity {
                            kind: "player",
                            ..Entity::default()
                        },
                    );
                }
                if !village {
                    return Ok(None);
                }
                if name == "LOGIN" {
                    w.i32(self.player.unwrap_or_default());
                    w.u8(r.u8()?);
                }
                w.i32(r.i32()?);
                let difficulty = r.u8()?;
                w.u8(r.u8()?);
                w.string(&r.string(32767)?);
                if name == "LOGIN" {
                    w.varint(VIEW_DISTANCE);
                    w.bool(codec::boolean(&mut r)?);
                }
                extra.push(step.incoming("CHANGE_DIFFICULTY", vec![difficulty, 0])?);
                let mut radius = Writer::new();
                radius.varint(VIEW_DISTANCE);
                extra.push(step.incoming("SET_CHUNK_CACHE_RADIUS", radius.data)?);
            }
            "ADD_ENTITY" => {
                let id = r.varint()?;
                let uuid = r.bytes(16)?;
                let object = r.u8()?;
                let coordinates = r.bytes(26)?;
                let mut state = r.i32()?;
                let velocity = r.bytes(6)?;
                let Some(old_kind) = object_kind(i32::from(object), state) else {
                    return Ok(Some(Vec::new()));
                };
                let new_kind = step.same_or_map("entities", old_kind, false)?;
                let kind = chain::identifier(step.target, "entities", new_kind)?;
                self.entities.insert(
                    id,
                    Entity {
                        kind,
                        ..Entity::default()
                    },
                );
                w.varint(id);
                w.bytes(uuid);
                if village {
                    w.varint(new_kind);
                } else {
                    w.u8(object);
                }
                w.bytes(coordinates);
                if kind == "falling_block" {
                    state = step.same_or_map("blockstates", state, false)?;
                } else if village && (families(kind) & ARROW != 0 || (kind == "item" && state > 0))
                {
                    if families(kind) & ARROW != 0 {
                        state = state.saturating_sub(1);
                    }
                    let mut motion = Writer::new();
                    motion.varint(id);
                    motion.bytes(velocity);
                    extra.push(step.incoming("SET_ENTITY_MOTION", motion.data)?);
                }
                w.i32(state);
                w.bytes(velocity);
            }
            "ADD_MOB" | "ADD_PLAYER" => {
                let id = r.varint()?;
                w.varint(id);
                w.bytes(r.bytes(16)?);
                let kind = if name == "ADD_MOB" {
                    let kind = r.varint()?;
                    if chain::identifier(step.source, "entities", kind).is_err() {
                        return Ok(Some(Vec::new()));
                    }
                    let mapped = step.same_or_map("entities", kind, false)?;
                    w.varint(mapped);
                    chain::identifier(step.target, "entities", mapped)?
                } else {
                    "player"
                };
                self.entities.insert(
                    id,
                    Entity {
                        kind,
                        ..Entity::default()
                    },
                );
                w.bytes(r.bytes(if name == "ADD_MOB" { 33 } else { 26 })?);
                extra.extend(self.metadata(step, id, &mut r, &mut w)?);
            }
            "SET_ENTITY_DATA" => {
                let id = r.varint()?;
                w.varint(id);
                extra.extend(self.metadata(step, id, &mut r, &mut w)?);
            }
            "REMOVE_ENTITIES" => {
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    let id = r.varint()?;
                    self.entities.remove(&id);
                    w.varint(id);
                }
            }
            "ADD_PAINTING" if village => {
                let id = r.varint()?;
                self.entities.insert(
                    id,
                    Entity {
                        kind: "painting",
                        ..Entity::default()
                    },
                );
                w.varint(id);
                w.bytes(r.bytes(16)?);
                w.varint(r.varint()?);
                position(step, false, &mut r, &mut w)?;
                w.u8(r.u8()?);
            }
            "PLAYER_SLEEP" if village => {
                let id = r.varint()?;
                let pos = Position::unpack(r.i64()?, step.source);
                codec::end(&r)?;
                return Ok(Some(vec![self.sleep(step, id, Some(pos))?]));
            }
            "ANIMATE" if village => {
                let id = r.varint()?;
                let action = r.u8()?;
                w.varint(id);
                w.u8(action);
                if action == 2 {
                    extra.push(self.sleep(step, id, None)?);
                }
            }
            "COMMAND_SUGGESTIONS" if !village => {
                let id = r.varint()?;
                let start = r.varint()?;
                w.varint(id);
                w.varint(if self.slash_requests.remove(&id) {
                    start
                        .checked_add(1)
                        .ok_or_else(|| Error::new("Command suggestion offset overflow"))?
                } else {
                    start
                });
                w.bytes(r.rest());
            }
            "BOSS_EVENT" if !village => {
                w.bytes(r.bytes(16)?);
                let action = r.varint()?;
                w.varint(action);
                if action == 0 {
                    w.string(&r.string(262144)?);
                    w.f32(r.f32()?);
                    w.varint(r.varint()?);
                    w.varint(r.varint()?);
                    let flags = r.u8()?;
                    w.u8(flags | if flags & 2 != 0 { 4 } else { 0 });
                } else {
                    w.bytes(r.rest());
                }
            }
            "CHANGE_DIFFICULTY" if village => {
                w.u8(r.u8()?);
                w.bool(false);
            }
            "BLOCK_DESTRUCTION"
            | "BLOCK_ENTITY_DATA"
            | "SET_DEFAULT_SPAWN_POSITION"
            | "OPEN_SIGN_EDITOR"
                if village =>
            {
                if name == "BLOCK_DESTRUCTION" {
                    w.varint(r.varint()?);
                }
                position(step, false, &mut r, &mut w)?;
                w.bytes(r.rest());
            }
            "BLOCK_UPDATE" => {
                position(step, false, &mut r, &mut w)?;
                w.varint(step.same_or_map("blockstates", r.varint()?, false)?);
            }
            "BLOCK_EVENT" => {
                position(step, false, &mut r, &mut w)?;
                w.bytes(r.bytes(2)?);
                w.varint(step.same_or_map("blocks", r.varint()?, false)?);
            }
            "CHUNK_BLOCKS_UPDATE" => {
                rewrite::chunk_blocks(step, &mut r, &mut w)?;
            }
            "LEVEL_EVENT" => {
                let event = r.i32()?;
                w.i32(event);
                position(step, false, &mut r, &mut w)?;
                let data = r.i32()?;
                w.i32(match event {
                    2001 => step.same_or_map("blockstates", data, false)?,
                    1010 if data != 0 => step.same_or_map("items", data, false)?,
                    _ => data,
                });
                w.bool(codec::boolean(&mut r)?);
            }
            "EXPLODE" if village => {
                for _ in 0..3 {
                    let value = r.f32()?;
                    w.f32(if value < 0.0 { value.trunc() } else { value });
                }
                w.bytes(r.rest());
            }
            "MAP_ITEM_DATA" if village => {
                w.varint(r.varint()?);
                w.u8(r.u8()?);
                w.bool(codec::boolean(&mut r)?);
                w.bool(false);
                w.bytes(r.rest());
            }
            "SOUND" | "COOLDOWN" => {
                w.varint(step.same_or_map(
                    if name == "SOUND" { "sounds" } else { "items" },
                    r.varint()?,
                    false,
                )?);
                w.bytes(r.rest());
            }
            "COMMANDS" if village => {
                return Ok(Some(vec![
                    step.incoming(name, rewrite::commands(step, data)?)?,
                ]));
            }
            "UPDATE_ADVANCEMENTS" => {
                return Ok(Some(vec![
                    step.incoming(name, rewrite::advancements(step, data)?)?,
                ]));
            }
            "UPDATE_TAGS" => {
                for registry in ["blocks", "items", "fluids"] {
                    let count = r.count(65536)?;
                    w.varint(count as i32);
                    for _ in 0..count {
                        w.string(&r.string(32767)?);
                        let values = r.count(65536)?;
                        w.varint(values as i32);
                        for _ in 0..values {
                            let id = r.varint()?;
                            w.varint(if registry == "fluids" {
                                id
                            } else {
                                step.same_or_map(registry, id, false)?
                            });
                        }
                    }
                }
                if village {
                    w.varint(0);
                }
            }
            "AWARD_STATS" => {
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    let category = r.varint()?;
                    let id = r.varint()?;
                    let registry = match category {
                        0 => Some("blocks"),
                        1..=5 => Some("items"),
                        6 | 7 => Some("entities"),
                        8 if step.registry("statistics").is_some() => Some("statistics"),
                        _ => None,
                    };
                    w.varint(category);
                    w.varint(if let Some(registry) = registry {
                        step.same_or_map(registry, id, false)?
                    } else {
                        id
                    });
                    w.varint(r.varint()?);
                }
            }
            "OPEN_SCREEN" if village => {
                let window = r.u8()?;
                let kind = r.string(32767)?;
                let title = r.string(262144)?;
                let slots = r.u8()?;
                if kind == "EntityHorse" {
                    target_name = "HORSE_SCREEN_OPEN";
                    w.u8(window);
                    w.varint(i32::from(slots));
                    w.i32(r.i32()?);
                } else {
                    let menu = match kind.as_str() {
                        "minecraft:crafting_table" => 11,
                        "minecraft:furnace" => 13,
                        "minecraft:dropper" | "minecraft:dispenser" => 6,
                        "minecraft:enchanting_table" => 12,
                        "minecraft:brewing_stand" => 10,
                        "minecraft:villager" => 18,
                        "minecraft:beacon" => 8,
                        "minecraft:anvil" => 7,
                        "minecraft:hopper" => 15,
                        "minecraft:shulker_box" => 19,
                        _ if (9..=54).contains(&slots) && slots % 9 == 0 => {
                            i32::from(slots / 9 - 1)
                        }
                        _ => return Ok(Some(Vec::new())),
                    };
                    w.varint(i32::from(window));
                    w.varint(menu);
                    w.string(&menu_title(&title));
                }
            }
            "CUSTOM_PAYLOAD" if village => {
                let channel = r.string(32767)?;
                match channel.strip_prefix("minecraft:").unwrap_or(&channel) {
                    "trader_list" => {
                        target_name = "MERCHANT_OFFERS";
                        w.varint(r.i32()?);
                        let count = r.u8()?;
                        w.u8(count);
                        for _ in 0..count {
                            rewrite::item(step, false, &mut r, &mut w)?;
                            rewrite::item(step, false, &mut r, &mut w)?;
                            let second = codec::boolean(&mut r)?;
                            w.bool(second);
                            if second {
                                rewrite::item(step, false, &mut r, &mut w)?;
                            }
                            w.bytes(r.bytes(9)?);
                            w.i32(0);
                            w.i32(0);
                            w.f32(0.0);
                        }
                        w.varint(0);
                        w.varint(0);
                        w.bool(false);
                    }
                    "book_open" => {
                        target_name = "OPEN_BOOK";
                        w.varint(r.varint()?);
                    }
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        }
        codec::end(&r)?;
        let mut output = vec![step.incoming(target_name, w.data)?];
        output.extend(extra);
        Ok(Some(output))
    }

    pub fn serverbound(
        &mut self,
        step: &Step,
        name: &str,
        data: &[u8],
    ) -> Result<Option<Vec<Packet>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let village = step.source == 404;
        match name {
            "COMMAND_SUGGESTION" if !village => {
                let id = r.varint()?;
                let command = r.string(32767)?;
                w.varint(id);
                if let Some(command) = command.strip_prefix('/') {
                    if self.slash_requests.len() >= 4096 {
                        self.slash_requests.clear();
                    }
                    self.slash_requests.insert(id);
                    w.string(command);
                } else {
                    self.slash_requests.remove(&id);
                    w.string(&command);
                }
            }
            "CONTAINER_CLICK" => {
                w.bytes(r.bytes(6)?);
                w.varint(r.varint()?);
                rewrite::item(step, true, &mut r, &mut w)?;
            }
            "SET_CREATIVE_MODE_SLOT" => {
                return Ok(Some(
                    rewrite::creative(step, data)?
                        .map(|data| step.outgoing(name, data))
                        .transpose()?
                        .into_iter()
                        .collect(),
                ));
            }
            "EDIT_BOOK" => {
                let mut stack = Stack::read(&mut r, step.target)?;
                rewrite::stack(step, true, &mut stack)?;
                if village {
                    use horizon_world::nbt::Tag;
                    if let Some(Stack {
                        tag: Tag::Compound(fields),
                        ..
                    }) = stack.as_mut()
                    {
                        let pages = fields
                            .entry("pages".into())
                            .or_insert_with(|| Tag::List(vec![Tag::String("".into())]));
                        if let Tag::List(pages) = pages {
                            pages.truncate(50);
                        }
                    }
                }
                Stack::write(stack.as_ref(), &mut w, step.source)?;
                w.bool(codec::boolean(&mut r)?);
                let hand = r.varint()?;
                if village {
                    w.varint(hand);
                } else if hand != 0 {
                    return Ok(Some(Vec::new()));
                }
            }
            "BLOCK_ENTITY_TAG_QUERY"
            | "PLAYER_ACTION"
            | "SET_COMMAND_BLOCK"
            | "SET_STRUCTURE_BLOCK"
            | "SIGN_UPDATE"
                if village =>
            {
                if matches!(name, "BLOCK_ENTITY_TAG_QUERY" | "PLAYER_ACTION") {
                    w.varint(r.varint()?);
                }
                position(step, true, &mut r, &mut w)?;
                w.bytes(r.rest());
            }
            "USE_ITEM_ON" if village => {
                let hand = r.varint()?;
                position(step, true, &mut r, &mut w)?;
                w.varint(r.varint()?);
                w.varint(hand);
                w.bytes(r.bytes(12)?);
                codec::boolean(&mut r)?;
            }
            "RECIPE_BOOK_UPDATE" if village => {
                let action = r.varint()?;
                w.varint(action);
                match action {
                    0 => w.string(&r.string(32767)?),
                    1 => {
                        for index in 0..8 {
                            let value = codec::boolean(&mut r)?;
                            if index < 4 {
                                w.bool(value);
                            }
                        }
                    }
                    _ => return Err(Error::new("Invalid old recipe book action")),
                }
            }
            "CHANGE_DIFFICULTY" | "LOCK_DIFFICULTY" | "SET_JIGSAW_BLOCK" if village => {
                return Ok(Some(Vec::new()));
            }
            _ => return Ok(None),
        }
        codec::end(&r)?;
        Ok(Some(vec![step.outgoing(name, w.data)?]))
    }
}

fn menu_title(title: &str) -> String {
    fn rewrite(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                if let Some(serde_json::Value::String(key)) = fields.get_mut("translate") {
                    if key.starts_with("block.") && key.ends_with(".name") {
                        key.truncate(key.len() - 5);
                    }
                }
                for value in fields.values_mut() {
                    rewrite(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    rewrite(value);
                }
            }
            _ => (),
        }
    }
    let Ok(mut component) = serde_json::from_str(title) else {
        return title.to_owned();
    };
    rewrite(&mut component);
    component.to_string()
}
