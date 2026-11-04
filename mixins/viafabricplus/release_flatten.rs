// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::{BTreeMap, VecDeque};

use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::{self as chain, Step},
    vfp_release_chain_data::{
        BLOCK_ALIASES_112, CHANNELS_112, ENTITIES_112, ENTITY_NAMES_111, PAINTINGS_112,
        PARTICLES_112, STATS_112,
    },
    vfp_release_chain_resources::{RECIPES_113, TAGS_113},
    vfp_release_codec::{self as codec, Position, Stack},
    vfp_release_columns::Column,
    vfp_release_connections::Store,
    vfp_release_metadata::{self as meta, Entry, Value},
    vfp_release_rewrite as rewrite,
    vfp_wire::Packet,
};
use horizon_world::nbt::Tag;

const RECIPE_PREFIX: &str = "viaversion:legacy/";

#[derive(Default)]
pub struct State {
    entities: BTreeMap<i32, &'static str>,
    suggestions: VecDeque<(i32, String)>,
    connections: Store,
    block_entities: BTreeMap<(i32, i32), BTreeMap<u16, (i32, i32)>>,
}

pub fn state_id(step: &Step, state: i32) -> Result<i32> {
    let state = state.max(0);
    step.map("blockstates", state)
        .or_else(|_| step.map("blockstates", state & !15))
        .or_else(|_| step.map("blockstates", 0))
}

pub fn combined_state(state: i32) -> i32 {
    ((state & 4095) << 4) | ((state >> 12) & 15)
}

fn particle_id(old: i32) -> Option<i32> {
    PARTICLES_112
        .get(usize::try_from(old).ok()?)
        .copied()
        .filter(|id| *id >= 0)
}

fn particle_arguments(
    step: &Step,
    w: &mut Writer,
    old: i32,
    args: [i32; 2],
    dust: [f32; 3],
) -> Result<()> {
    match old {
        30 => {
            for value in dust {
                w.f32(value);
            }
            w.f32(1.0);
        }
        36 => {
            let mut item = Some(Stack {
                id: args[0],
                count: 1,
                damage: args[1] as i16,
                tag: Tag::End,
            });
            rewrite::stack(step, false, &mut item)?;
            Stack::write(item.as_ref(), w, step.target)?;
        }
        37 | 38 | 46 => {
            let mut state = combined_state(args[0]);
            if old == 46 && state >> 4 == 160 {
                state = (95 << 4) | (state & 15);
            }
            w.varint(state_id(step, state)?);
        }
        _ => (),
    }
    Ok(())
}

fn metadata(step: &Step, r: &mut Reader<'_>, w: &mut Writer, kind: &str) -> Result<()> {
    let mut entries = meta::read(r, step.source)?;
    let cloud_particle = if kind == "area_effect_cloud" {
        let get = |index, fallback| {
            entries
                .iter()
                .find(|entry| entry.index == index)
                .map(Entry::integer)
                .transpose()
                .map(|value| value.unwrap_or(fallback))
        };
        if entries.iter().any(|entry| entry.index == 9) {
            let old = get(9, -1)?;
            if let Some(id) = particle_id(old) {
                let mut particle = Writer::new();
                particle.varint(id);
                particle_arguments(
                    step,
                    &mut particle,
                    old,
                    [get(10, 0)?, get(11, 0)?],
                    [1.0, 0.0, 0.0],
                )?;
                Some(Entry {
                    index: 9,
                    kind: 15,
                    value: Value::Particle(particle.data),
                })
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    if kind == "area_effect_cloud" {
        entries.retain(|entry| entry.index < 9);
    }
    for entry in &mut entries {
        entry.remap(step)?;
        match entry.index {
            0 => entry.value = Value::Int(entry.integer()? & !0x10),
            2 => {
                if let Value::Text(name) = &entry.value {
                    entry.kind = 5;
                    entry.value = Value::OptionalText(
                        (!name.is_empty()).then(|| crate::vfp_release_text::json(name)),
                    );
                }
            }
            9 if kind.ends_with("minecart") => {
                entry.value = Value::Int(state_id(step, combined_state(entry.integer()?))?);
            }
            17 if kind == "wolf" => entry.value = Value::Int(15 - entry.integer()?.clamp(0, 15)),
            _ => (),
        }
        if matches!(
            kind,
            "zombie" | "husk" | "zombie_villager" | "zombie_pigman"
        ) && entry.index >= 15
        {
            entry.index = entry
                .index
                .checked_add(1)
                .ok_or_else(|| Error::new("Legacy zombie metadata index overflow"))?;
        }
    }
    entries.extend(cloud_particle);
    meta::write(w, &entries, step.target)
}

fn commands() -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(2);
    w.u8(0);
    w.varint(1);
    w.varint(1);
    w.u8(0x16);
    w.varint(0);
    w.string("args");
    w.string("brigadier:string");
    w.varint(2);
    w.string("minecraft:ask_server");
    w.varint(0);
    w.data
}

fn channel(name: &str, inverse: bool) -> Option<String> {
    const BUILTIN: &[(&str, &str)] = &[
        ("MC|TrList", "minecraft:trader_list"),
        ("MC|Brand", "minecraft:brand"),
        ("MC|BOpen", "minecraft:book_open"),
        ("REGISTER", "minecraft:register"),
        ("UNREGISTER", "minecraft:unregister"),
        ("BungeeCord", "bungeecord:main"),
        ("MC|DebugPath", "minecraft:debug/paths"),
        (
            "MC|DebugNeighborsUpdate",
            "minecraft:debug/neighbors_update",
        ),
    ];
    let full_name = if inverse && !name.contains(':') {
        format!("minecraft:{name}")
    } else {
        name.to_owned()
    };
    if let Some(mapped) = BUILTIN.iter().chain(CHANNELS_112).find_map(|&(old, new)| {
        let (before, after) = if inverse { (new, old) } else { (old, new) };
        (full_name == before).then_some(after)
    }) {
        return Some(mapped.to_owned());
    }
    let (namespace, path) = full_name
        .split_once(':')
        .unwrap_or(("minecraft", &full_name));
    let valid = !namespace.is_empty()
        && !path.is_empty()
        && namespace
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'))
        && path
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'));
    if !valid {
        return None;
    }
    if inverse {
        (full_name.len() <= 20).then_some(full_name)
    } else {
        Some(format!("{namespace}:{path}"))
    }
}

fn registrations(payload: &[u8], inverse: bool) -> Result<Vec<u8>> {
    let names = std::str::from_utf8(payload)
        .map_err(|_| Error::new("Invalid legacy plugin registration"))?;
    Ok(names
        .split('\0')
        .filter_map(|name| channel(name, inverse))
        .collect::<Vec<_>>()
        .join("\0")
        .into_bytes())
}

fn named_sound(step: &Step, name: &str) -> Option<String> {
    let bare = name.strip_prefix("minecraft:").unwrap_or(name);
    let old = chain::named_id(step.source, "sounds", bare).ok();
    if let Some(old) = old {
        let new = step.map("sounds", old).ok()?;
        let identifier = chain::identifier(step.target, "sounds", new).ok()?;
        Some(format!("minecraft:{identifier}"))
    } else {
        Some(name.to_owned())
    }
}

fn inventory_trade(step: &Step, r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.i32(r.i32()?);
    let trades = r.u8()?;
    w.u8(trades);
    for _ in 0..trades {
        rewrite::item(step, false, r, w)?;
        rewrite::item(step, false, r, w)?;
        let second = codec::boolean(r)?;
        w.bool(second);
        if second {
            rewrite::item(step, false, r, w)?;
        }
        w.bytes(r.bytes(9)?);
    }
    Ok(())
}

fn tag_number(tag: &Tag, key: &str) -> Option<i32> {
    match tag.get(key)? {
        Tag::Byte(value) => Some(i32::from(*value)),
        Tag::Short(value) => Some(i32::from(*value)),
        Tag::Int(value) => Some(*value),
        _ => None,
    }
}

fn tag_string(tag: &Tag, key: &str) -> Option<String> {
    if let Tag::String(value) = tag.get(key)? {
        Some(value.to_string_lossy())
    } else {
        None
    }
}

fn potted(tag: &Tag) -> i32 {
    let id = tag_number(tag, "Item").unwrap_or_else(|| {
        match tag_string(tag, "Item")
            .as_deref()
            .map(|name| name.strip_prefix("minecraft:").unwrap_or(name))
        {
            Some("sapling") => 6,
            Some("tallgrass") => 31,
            Some("deadbush") => 32,
            Some("yellow_flower") => 37,
            Some("red_flower") => 38,
            Some("brown_mushroom") => 39,
            Some("red_mushroom") => 40,
            Some("cactus") => 81,
            _ => 0,
        }
    });
    let damage = tag_number(tag, "Data").unwrap_or(0);
    match id {
        6 => 5266 + if (0..=5).contains(&damage) { damage } else { 0 },
        31 if damage == 2 => 5272,
        32 => 5285,
        37 => 5273,
        38 => 5274 + if (0..=8).contains(&damage) { damage } else { 0 },
        39 => 5284,
        40 => 5283,
        81 => 5286,
        _ => 5265,
    }
}

fn entity_name(name: &str) -> &str {
    match name.strip_prefix("minecraft:").unwrap_or(name) {
        "commandblock_minecart" => "command_block_minecart",
        "ender_crystal" => "end_crystal",
        "evocation_fangs" => "evoker_fangs",
        "evocation_illager" => "evoker",
        "eye_of_ender_signal" => "eye_of_ender",
        "fireworks_rocket" => "firework_rocket",
        "illusion_illager" => "illusioner",
        "snowman" => "snow_golem",
        "villager_golem" => "iron_golem",
        "vindication_illager" => "vindicator",
        "xp_bottle" => "experience_bottle",
        "xp_orb" => "experience_orb",
        name => name,
    }
}

fn spawner_name(tag: &mut Tag) {
    let Some(Tag::String(id)) = tag.get_mut("id") else {
        return;
    };
    let name = id.to_string_lossy();
    let mapped = entity_name(&name);
    if mapped != name.strip_prefix("minecraft:").unwrap_or(&name) {
        *id = format!("minecraft:{mapped}").as_str().into();
    }
}

fn statistic(step: &Step, name: &str) -> Option<(i32, i32)> {
    if let Some(id) = STATS_112
        .iter()
        .find_map(|&(old, id)| (old == name).then_some(id))
    {
        return Some((8, id));
    }
    let (category, key) = name.strip_prefix("stat.")?.split_once('.')?;
    let (category, registry) = match category {
        "mineBlock" => (0, "blocks"),
        "craftItem" => (1, "items"),
        "useItem" => (2, "items"),
        "breakItem" => (3, "items"),
        "pickup" => (4, "items"),
        "drop" => (5, "items"),
        "killEntity" => (6, "entities"),
        "entityKilledBy" => (7, "entities"),
        _ => return None,
    };
    let key = key
        .strip_prefix("minecraft.")
        .or_else(|| key.strip_prefix("minecraft:"))
        .unwrap_or(key);
    let lookup = |key: &str| chain::named_id(step.target, registry, key).ok();
    let id = if registry == "entities" {
        let entity = ENTITY_NAMES_111
            .iter()
            .find_map(|&(old, new)| (key == old).then_some(new));
        let bare = key.to_ascii_lowercase();
        lookup(entity_name(entity.unwrap_or(&bare)))
    } else {
        // Old counters were for the shared numeric ID. Use its first
        // available flattened variant when it no longer has a single ID.
        let legacy = if registry == "items" {
            match key {
                "boat" => "oak_boat",
                "bed" => "red_bed",
                "banner" => "white_banner",
                "fish" => "cod",
                "cooked_fish" => "cooked_cod",
                "dye" => "ink_sac",
                "wooden_door" => "oak_door",
                "reeds" => "sugar_cane",
                "speckled_melon" => "glistering_melon_slice",
                "totem" => "totem_of_undying",
                "record_13" => "music_disc_13",
                "record_cat" => "music_disc_cat",
                name => name,
            }
        } else {
            key
        };
        BLOCK_ALIASES_112
            .iter()
            .find_map(|&(old, choices)| {
                (legacy == old)
                    .then(|| choices.iter().find_map(|&choice| lookup(choice)))
                    .flatten()
            })
            .or_else(|| lookup(legacy))
            .or_else(|| {
                let disc = legacy.strip_prefix("record_")?;
                lookup(&format!("music_disc_{disc}"))
            })
    }?;
    Some((category, id))
}

fn block_entity_kind(tag: &Tag) -> String {
    let name = tag_string(tag, "id").unwrap_or_default();
    name.strip_prefix("minecraft:").unwrap_or(&name).to_owned()
}

fn flatten_block_entity(tag: &mut Tag, base: Option<i32>) -> Option<i32> {
    let kind = block_entity_kind(tag);
    match kind.as_str() {
        "bed" => base
            .filter(|id| (972..=987).contains(id))
            .map(|id| id - 972 + 748 + tag_number(tag, "color").unwrap_or(14).clamp(0, 15) * 16),
        "banner" => {
            let color = 15 - tag_number(tag, "Base").unwrap_or(0).clamp(0, 15);
            if let Some(Tag::List(patterns)) = tag.get_mut("Patterns") {
                for pattern in patterns {
                    if let Some(color) = tag_number(pattern, "Color") {
                        if let Some(value) = pattern.get_mut("Color") {
                            *value = Tag::Int(15 - color.clamp(0, 15));
                        }
                    }
                }
            }
            if let Some(Tag::String(name)) = tag.get_mut("CustomName") {
                *name = crate::vfp_release_text::json(&name.to_string_lossy())
                    .as_str()
                    .into();
            }
            match base {
                Some(id @ 6854..=6869) => Some(id + color * 16),
                Some(id @ 7110..=7113) => Some(id + color * 4),
                _ => None,
            }
        }
        "skull" => base.filter(|id| (5447..=5451).contains(id)).map(|id| {
            id + (tag_number(tag, "Rot").unwrap_or(0) & 15)
                + tag_number(tag, "SkullType").unwrap_or(0).clamp(0, 5) * 20
        }),
        "flower_pot" => base
            .filter(|id| (5265..=5286).contains(id))
            .map(|_| potted(tag)),
        "mob_spawner" => {
            if let Some(entity) = tag.get_mut("SpawnData") {
                spawner_name(entity);
            }
            if let Some(Tag::List(potentials)) = tag.get_mut("SpawnPotentials") {
                for potential in potentials {
                    if let Some(entity) = potential.get_mut("Entity") {
                        spawner_name(entity);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn block_key(pos: Position) -> Option<u16> {
    if !(0..256).contains(&pos.y) {
        return None;
    }
    Some(((pos.y << 8) | ((pos.z & 15) << 4) | (pos.x & 15)) as u16)
}

impl State {
    pub fn source(&mut self, protocol: i32) {
        self.connections.source(protocol);
    }

    pub fn reset(&mut self) {
        self.entities.clear();
        self.suggestions.clear();
        self.block_entities.clear();
        self.connections.clear();
    }

    fn block(&mut self, pos: Position, state: i32) -> i32 {
        let Some(key) = block_key(pos) else {
            return state;
        };
        let coord = (pos.x >> 4, pos.z >> 4);
        if matches!(state, 972..=987 | 5265..=5286 | 5447..=5451 | 6854..=6869 | 7110..=7113) {
            let column = self.block_entities.entry(coord).or_default();
            let (before, after) = column.entry(key).or_insert((state, state));
            if *before != state {
                (*before, *after) = (state, state);
            }
            *after
        } else {
            if let Some(column) = self.block_entities.get_mut(&coord) {
                column.remove(&key);
            }
            state
        }
    }

    pub fn column(&mut self, step: &Step, column: &mut Column) -> Result<Vec<Packet>> {
        if let Some(biomes) = &mut column.biomes {
            for id in biomes {
                if !matches!(id, 0..=49 | 127 | 129..=134 | 140 | 149 | 151 | 155..=158 | 160..=167)
                {
                    *id = 1;
                }
            }
        }
        let coord = (column.x, column.z);
        if column.full {
            self.block_entities.remove(&coord);
        }
        for (&section, data) in &mut column.sections {
            for (index, state) in data.blocks.iter_mut().enumerate() {
                let pos = Position {
                    x: (column.x << 4) | (index & 15) as i32,
                    y: (section * 16 + (index >> 8)) as i32,
                    z: (column.z << 4) | ((index >> 4) & 15) as i32,
                };
                *state = self.block(pos, *state);
            }
        }
        for tag in &mut column.entities {
            let pos = (|| {
                Some(Position {
                    x: tag_number(tag, "x")?,
                    y: tag_number(tag, "y")?,
                    z: tag_number(tag, "z")?,
                })
            })();
            let key = pos
                .filter(|pos| (pos.x >> 4, pos.z >> 4) == coord)
                .and_then(block_key);
            let base = key
                .and_then(|key| {
                    self.block_entities
                        .get(&coord)
                        .and_then(|column| column.get(&key))
                })
                .map(|&(base, _)| base);
            if let (Some(key), Some(mapped)) = (key, flatten_block_entity(tag, base)) {
                if let Some(data) = column.sections.get_mut(&(usize::from(key) >> 12)) {
                    data.blocks[usize::from(key) & 4095] = mapped;
                }
                if let Some(state) = self
                    .block_entities
                    .get_mut(&coord)
                    .and_then(|column| column.get_mut(&key))
                {
                    state.1 = mapped;
                }
            }
        }
        column
            .entities
            .retain(|tag| !matches!(block_entity_kind(tag).as_str(), "noteblock" | "flower_pot"));
        self.connections.column(step, column)
    }

    pub fn clientbound(
        &mut self,
        step: &Step,
        name: &str,
        data: &[u8],
    ) -> Result<Option<Vec<Packet>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut before = Vec::new();
        let mut target = name;
        match name {
            "LOGIN" | "RESPAWN" => {
                self.reset();
                return Ok(Some(vec![
                    step.incoming(name, data.to_vec())?,
                    step.incoming("COMMANDS", commands())?,
                    step.incoming("UPDATE_TAGS", TAGS_113.to_vec())?,
                ]));
            }
            "ADD_ENTITY" => {
                let id = r.varint()?;
                r.bytes(16)?;
                let object = i32::from(r.u8()?);
                r.bytes(26)?;
                w.bytes(r.since(0));
                let data = r.i32()?;
                let kind = crate::vfp_release_113::object_kind(object, data)
                    .and_then(|kind| chain::identifier(step.target, "entities", kind).ok());
                if let Some(kind) = kind {
                    self.entities.insert(id, kind);
                } else {
                    self.entities.remove(&id);
                }
                w.i32(match object {
                    70 => state_id(step, combined_state(data))?,
                    71 => match data {
                        0 => 3,
                        1 => 4,
                        3 => 5,
                        _ => data,
                    },
                    _ => data,
                });
                w.bytes(r.rest());
            }
            "ADD_MOB" | "ADD_PLAYER" => {
                let id = r.varint()?;
                w.varint(id);
                w.bytes(r.bytes(16)?);
                let kind = if name == "ADD_MOB" {
                    let old = r.varint()?;
                    let Some(new) = ENTITIES_112
                        .iter()
                        .find_map(|&(from, to)| (from == old).then_some(to))
                    else {
                        return Ok(Some(Vec::new()));
                    };
                    w.varint(new);
                    w.bytes(r.bytes(33)?);
                    chain::identifier(step.target, "entities", new)?
                } else {
                    w.bytes(r.bytes(26)?);
                    "player"
                };
                self.entities.insert(id, kind);
                metadata(step, &mut r, &mut w, kind)?;
            }
            "SET_ENTITY_DATA" => {
                let id = r.varint()?;
                w.varint(id);
                metadata(
                    step,
                    &mut r,
                    &mut w,
                    self.entities.get(&id).copied().unwrap_or(""),
                )?;
            }
            "REMOVE_ENTITIES" => {
                let count = r.count(65536)?;
                for _ in 0..count {
                    self.entities.remove(&r.varint()?);
                }
                codec::end(&r)?;
                return Ok(None);
            }
            "ADD_PAINTING" => {
                w.varint(r.varint()?);
                w.bytes(r.bytes(16)?);
                let title = r.string(32767)?.to_ascii_lowercase();
                let title = title.strip_prefix("minecraft:").unwrap_or(&title);
                w.varint(
                    PAINTINGS_112
                        .iter()
                        .position(|&name| name == title)
                        .unwrap_or(0) as i32,
                );
                w.bytes(r.bytes(9)?);
            }
            "FORGET_LEVEL_CHUNK" => {
                let (x, z) = (r.i32()?, r.i32()?);
                self.block_entities.remove(&(x, z));
                self.connections.forget(x, z);
                codec::end(&r)?;
                return Ok(None);
            }
            "BLOCK_ENTITY_DATA" => {
                let packed = r.i64()?;
                let pos = Position::unpack(packed, step.source);
                let action = r.u8()?;
                let mut tag = r.vfp_named_nbt()?;
                let cached = self.block_entities.get_mut(&(pos.x >> 4, pos.z >> 4));
                let entry = cached.and_then(|column| column.get_mut(&block_key(pos)?));
                let base = entry.as_ref().map(|entry| entry.0);
                if let Some(mapped) = flatten_block_entity(&mut tag, base) {
                    if let Some(entry) = entry {
                        entry.1 = mapped;
                    }
                    before.extend(self.connections.update(step, vec![(pos, mapped)])?);
                }
                if action == 5 {
                    codec::end(&r)?;
                    return Ok(Some(before));
                }
                w.i64(packed);
                w.u8(action);
                w.vfp_named_nbt(&tag)?;
            }
            "BLOCK_UPDATE" => {
                let pos = Position::unpack(r.i64()?, step.source);
                let state = self.block(pos, state_id(step, r.varint()?)?);
                codec::end(&r)?;
                return Ok(Some(self.connections.update(step, vec![(pos, state)])?));
            }
            "CHUNK_BLOCKS_UPDATE" => {
                let chunk_x = r.i32()?;
                let chunk_z = r.i32()?;
                let count = r.count(65536)?;
                let mut changes = Vec::with_capacity(count);
                for _ in 0..count {
                    let offset = r.u16()?;
                    let pos = Position {
                        x: (chunk_x << 4) | i32::from(offset >> 12),
                        y: i32::from(offset & 255),
                        z: (chunk_z << 4) | i32::from((offset >> 8) & 15),
                    };
                    changes.push((pos, self.block(pos, state_id(step, r.varint()?)?)));
                }
                codec::end(&r)?;
                return Ok(Some(self.connections.update(step, changes)?));
            }
            "EXPLODE" => {
                let origin = [r.f32()? as i32, r.f32()? as i32, r.f32()? as i32];
                r.f32()?;
                let count = usize::try_from(r.i32()?)
                    .ok()
                    .filter(|&count| count <= 65536)
                    .ok_or_else(|| Error::new("Invalid legacy explosion record count"))?;
                let mut changes = Vec::with_capacity(count);
                for _ in 0..count {
                    let pos = Position {
                        x: origin[0].wrapping_add(i32::from(r.i8()?)),
                        y: origin[1].wrapping_add(i32::from(r.i8()?)),
                        z: origin[2].wrapping_add(i32::from(r.i8()?)),
                    };
                    if (0..256).contains(&pos.y)
                        && (-(1 << 25)..(1 << 25)).contains(&pos.x)
                        && (-(1 << 25)..(1 << 25)).contains(&pos.z)
                    {
                        changes.push((pos, self.block(pos, 0)));
                    }
                }
                r.bytes(12)?;
                codec::end(&r)?;
                let mut packets = vec![step.incoming(name, data.to_vec())?];
                packets.extend(self.connections.update(step, changes)?);
                return Ok(Some(packets));
            }
            "CONTAINER_SET_DATA" => {
                w.u8(r.u8()?);
                let property = r.i16()?;
                w.i16(property);
                let value = r.i16()?;
                let value = if (4..=6).contains(&property) && value >= 0 {
                    step.map("enchantments", i32::from(value))
                        .ok()
                        .and_then(|mapped| i16::try_from(mapped).ok())
                        .unwrap_or(-1)
                } else {
                    value
                };
                w.i16(value);
            }
            "BLOCK_EVENT" => {
                let pos = r.i64()?;
                let action = r.u8()?;
                let param = r.u8()?;
                let old = r.varint()?;
                let block = match old {
                    25 => 73,
                    33 => 99,
                    29 => 92,
                    54 => 142,
                    146 => 305,
                    130 => 249,
                    138 => 257,
                    52 => 140,
                    209 => 472,
                    219..=234 => old - 219 + 483,
                    _ => return Ok(Some(Vec::new())),
                };
                if block == 73 && param < 25 {
                    let mut note = Writer::new();
                    note.i64(pos);
                    note.varint(249 + i32::from(action) * 48 + i32::from(param) * 2);
                    before.push(step.incoming("BLOCK_UPDATE", note.data)?);
                }
                w.i64(pos);
                w.u8(action);
                w.u8(param);
                w.varint(block);
            }
            "LEVEL_EVENT" => {
                let event = r.i32()?;
                w.i32(event);
                w.i64(r.i64()?);
                let value = r.i32()?;
                w.i32(match event {
                    1010 if value != 0 => step.map("items", value.saturating_mul(16))?,
                    2001 => state_id(step, combined_state(value))?,
                    _ => value,
                });
                w.bool(codec::boolean(&mut r)?);
            }
            "SOUND" | "COOLDOWN" => {
                let id = r.varint()?;
                let mapped = if name == "SOUND" {
                    step.map("sounds", id)
                } else {
                    step.map("items", id.saturating_mul(16))
                };
                let Ok(mapped) = mapped else {
                    return Ok(Some(Vec::new()));
                };
                w.varint(mapped);
                w.bytes(r.rest());
            }
            "CUSTOM_SOUND" => {
                let sound = r.string(32767)?;
                let Some(sound) = named_sound(step, &sound) else {
                    return Ok(Some(Vec::new()));
                };
                w.string(&sound);
                w.bytes(r.rest());
            }
            "LEVEL_PARTICLES" => {
                let old = r.i32()?;
                let Some(id) = particle_id(old) else {
                    return Ok(Some(Vec::new()));
                };
                w.i32(id);
                w.bool(codec::boolean(&mut r)?);
                w.bytes(r.bytes(12)?);
                let offsets = [r.f32()?, r.f32()?, r.f32()?];
                let speed = r.f32()?;
                let count = r.i32()?;
                let dust = if old == 30 && count == 0 {
                    for _ in 0..4 {
                        w.f32(0.0);
                    }
                    w.i32(1);
                    let mut dust = offsets.map(|value| (value * speed).clamp(0.0, 1.0));
                    if dust[0] == 0.0 {
                        dust[0] = 1.0;
                    }
                    dust
                } else {
                    for value in offsets {
                        w.f32(value);
                    }
                    w.f32(speed);
                    w.i32(count);
                    [1.0, 0.0, 0.0]
                };
                let args = match old {
                    36 => [r.varint()?, r.varint()?],
                    37 | 38 | 46 => [r.varint()?, 0],
                    _ => [0, 0],
                };
                particle_arguments(step, &mut w, old, args, dust)?;
            }
            "MAP_ITEM_DATA" => {
                w.varint(r.varint()?);
                w.u8(r.u8()?);
                w.bool(codec::boolean(&mut r)?);
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    let icon = r.u8()?;
                    w.varint(i32::from(icon >> 4));
                    w.bytes(r.bytes(2)?);
                    w.u8(icon & 15);
                    w.bool(false);
                }
                w.bytes(r.rest());
            }
            "CUSTOM_PAYLOAD" => {
                let old = r.string(32767)?;
                if old == "MC|StopSound" {
                    target = "STOP_SOUND";
                    let category = r.string(32767)?;
                    let sound = r.string(32767)?;
                    let flags = u8::from(!category.is_empty()) | (u8::from(!sound.is_empty()) << 1);
                    w.u8(flags);
                    if flags & 1 != 0 {
                        let categories = [
                            "master", "music", "record", "weather", "block", "hostile", "neutral",
                            "player", "ambient", "voice",
                        ];
                        w.varint(
                            categories
                                .iter()
                                .position(|&value| value == category)
                                .unwrap_or(0) as i32,
                        );
                    }
                    if flags & 2 != 0 {
                        w.string(&named_sound(step, &sound).unwrap_or(sound));
                    }
                } else {
                    let Some(mapped) = channel(&old, false) else {
                        return Ok(Some(Vec::new()));
                    };
                    w.string(&mapped);
                    match old.as_str() {
                        "MC|TrList" => inventory_trade(step, &mut r, &mut w)?,
                        "REGISTER" | "UNREGISTER" => w.bytes(&registrations(r.rest(), false)?),
                        _ => w.bytes(r.rest()),
                    }
                }
            }
            "COMMAND_SUGGESTIONS" => {
                let Some((id, input)) = self.suggestions.pop_front() else {
                    return Ok(Some(Vec::new()));
                };
                let start = input.rfind(' ').map_or(0, |position| position + 1);
                w.varint(id);
                w.varint(input[..start].encode_utf16().count() as i32);
                w.varint(input[start..].encode_utf16().count() as i32);
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    let value = r.string(32767)?;
                    w.string(if start == 0 {
                        value.strip_prefix('/').unwrap_or(&value)
                    } else {
                        &value
                    });
                    w.bool(false);
                }
            }
            "PLACE_GHOST_RECIPE" => {
                w.u8(r.u8()?);
                w.string(&format!("{RECIPE_PREFIX}{}", r.varint()?));
            }
            "RECIPE" => {
                let action = r.count(2)?;
                if action == 0 {
                    before.push(step.incoming("UPDATE_RECIPES", RECIPES_113.to_vec())?);
                }
                w.varint(action as i32);
                w.bool(codec::boolean(&mut r)?);
                w.bool(codec::boolean(&mut r)?);
                w.bool(false);
                w.bool(false);
                for _ in 0..if action == 0 { 2 } else { 1 } {
                    let count = r.count(65536)?;
                    w.varint(count as i32);
                    for _ in 0..count {
                        w.string(&format!("{RECIPE_PREFIX}{}", r.varint()?));
                    }
                }
            }
            "SET_OBJECTIVE" => {
                w.string(&r.string(32767)?);
                let action = r.u8()?;
                w.u8(action);
                if matches!(action, 0 | 2) {
                    w.string(&crate::vfp_release_text::json(&r.string(32767)?));
                    w.varint(i32::from(r.string(32767)? == "hearts"));
                }
            }
            "UPDATE_ADVANCEMENTS" => {
                return Ok(Some(vec![
                    step.incoming(name, rewrite::advancements(step, data)?)?,
                ]));
            }
            "AWARD_STATS" => {
                let count = r.count(65536)?;
                let mut stats = BTreeMap::new();
                for _ in 0..count {
                    let name = r.string(32767)?;
                    let value = r.varint()?;
                    if let Some(key) = statistic(step, &name) {
                        let count = stats.entry(key).or_insert(0_i32);
                        *count = (*count).saturating_add(value);
                    }
                }
                w.varint(stats.len() as i32);
                for ((category, id), value) in stats {
                    w.varint(category);
                    w.varint(id);
                    w.varint(value);
                }
            }
            "SET_PLAYER_TEAM" => {
                w.string(&r.string(32767)?);
                let action = r.u8()?;
                w.u8(action);
                if matches!(action, 0 | 2) {
                    let display = r.string(32767)?;
                    let prefix = r.string(32767)?;
                    let suffix = r.string(32767)?;
                    w.string(&crate::vfp_release_text::json(&display));
                    w.u8(r.u8()?);
                    w.string(&r.string(32767)?);
                    w.string(&r.string(32767)?);
                    let color = r.i8()?;
                    w.varint(if color == -1 { 21 } else { i32::from(color) });
                    w.string(&crate::vfp_release_text::json(&prefix));
                    w.string(&crate::vfp_release_text::json(&suffix));
                }
                w.bytes(r.rest());
            }
            _ => return Ok(None),
        }
        codec::end(&r)?;
        before.push(step.incoming(target, w.data)?);
        Ok(Some(before))
    }

    pub fn serverbound(
        &mut self,
        step: &Step,
        name: &str,
        data: &[u8],
    ) -> Result<Option<Vec<Packet>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut target = name;
        match name {
            "COMMAND_SUGGESTION" => {
                let id = r.varint()?;
                let input = r.string(32767)?;
                if self.suggestions.len() >= 128 {
                    return Ok(Some(Vec::new()));
                }
                w.string(&format!("/{input}"));
                w.bool(false);
                w.bool(false);
                self.suggestions.push_back((id, input));
            }
            "CUSTOM_PAYLOAD" => {
                let identifier = r.string(32767)?;
                let Some(old) = channel(&identifier, true) else {
                    return Ok(Some(Vec::new()));
                };
                w.string(&old);
                if matches!(old.as_str(), "REGISTER" | "UNREGISTER") {
                    w.bytes(&registrations(r.rest(), true)?);
                } else {
                    w.bytes(r.rest());
                }
            }
            "EDIT_BOOK" => {
                target = "CUSTOM_PAYLOAD";
                let mut item = Stack::read(&mut r, step.target)?;
                rewrite::stack(step, true, &mut item)?;
                w.string(if codec::boolean(&mut r)? {
                    "MC|BSign"
                } else {
                    "MC|BEdit"
                });
                Stack::write(item.as_ref(), &mut w, step.source)?;
            }
            "PICK_ITEM" | "RENAME_ITEM" | "SELECT_TRADE" | "SET_BEACON" => {
                target = "CUSTOM_PAYLOAD";
                match name {
                    "PICK_ITEM" => w.string("MC|PickItem"),
                    "RENAME_ITEM" => {
                        w.string("MC|ItemName");
                        w.string(&r.string(32767)?);
                    }
                    "SELECT_TRADE" => {
                        w.string("MC|TrSel");
                        w.i32(r.varint()?);
                    }
                    "SET_BEACON" => {
                        w.string("MC|Beacon");
                        w.i32(r.varint()?);
                        w.i32(r.varint()?);
                    }
                    _ => unreachable!(),
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
            "PLACE_RECIPE" => {
                w.u8(r.u8()?);
                let name = r.string(32767)?;
                let Some(id) = name
                    .strip_prefix(RECIPE_PREFIX)
                    .and_then(|id| id.parse::<i32>().ok())
                    .filter(|id| *id >= 0)
                else {
                    return Ok(Some(Vec::new()));
                };
                w.varint(id);
                w.bool(codec::boolean(&mut r)?);
            }
            "RECIPE_BOOK_UPDATE" => {
                let action = r.count(1)?;
                w.varint(action as i32);
                if action == 0 {
                    let name = r.string(32767)?;
                    let Some(id) = name
                        .strip_prefix(RECIPE_PREFIX)
                        .and_then(|id| id.parse::<i32>().ok())
                        .filter(|id| *id >= 0)
                    else {
                        return Ok(Some(Vec::new()));
                    };
                    w.i32(id);
                } else {
                    w.bool(codec::boolean(&mut r)?);
                    w.bool(codec::boolean(&mut r)?);
                    codec::boolean(&mut r)?;
                    codec::boolean(&mut r)?;
                }
            }
            "SET_COMMAND_BLOCK" | "SET_COMMAND_MINECART" | "SET_STRUCTURE_BLOCK" => {
                target = "CUSTOM_PAYLOAD";
                if name == "SET_COMMAND_MINECART" {
                    w.string("MC|AdvCmd");
                    w.u8(1);
                    w.i32(r.varint()?);
                    w.string(&r.string(32767)?);
                    w.bool(codec::boolean(&mut r)?);
                } else {
                    w.string(if name == "SET_COMMAND_BLOCK" {
                        "MC|AutoCmd"
                    } else {
                        "MC|Struct"
                    });
                    let pos = Position::unpack(r.i64()?, step.target);
                    w.i32(pos.x);
                    w.i32(pos.y);
                    w.i32(pos.z);
                    if name == "SET_COMMAND_BLOCK" {
                        w.string(&r.string(32767)?);
                        let mode = ["SEQUENCE", "AUTO", "REDSTONE"][r.count(2)?];
                        let flags = r.u8()?;
                        w.bool(flags & 1 != 0);
                        w.string(mode);
                        w.bool(flags & 2 != 0);
                        w.bool(flags & 4 != 0);
                    } else {
                        w.u8(r.count(3)? as u8 + 1);
                        w.string(["SAVE", "LOAD", "CORNER", "DATA"][r.count(3)?]);
                        w.string(&r.string(32767)?);
                        for _ in 0..6 {
                            w.i32(i32::from(r.i8()?));
                        }
                        w.string(["NONE", "LEFT_RIGHT", "FRONT_BACK"][r.count(2)?]);
                        w.string(
                            [
                                "NONE",
                                "CLOCKWISE_90",
                                "CLOCKWISE_180",
                                "COUNTERCLOCKWISE_90",
                            ][r.count(3)?],
                        );
                        w.string(&r.string(32767)?);
                        let integrity = r.f32()?;
                        let seed = r.varlong()?;
                        let flags = r.u8()?;
                        w.bool(flags & 1 != 0);
                        w.bool(flags & 2 != 0);
                        w.bool(flags & 4 != 0);
                        w.f32(integrity);
                        w.varlong(seed);
                    }
                }
            }
            "BLOCK_ENTITY_TAG_QUERY" | "ENTITY_TAG_QUERY" => return Ok(Some(Vec::new())),
            _ => return Ok(None),
        }
        codec::end(&r)?;
        Ok(Some(vec![step.outgoing(target, w.data)?]))
    }
}
