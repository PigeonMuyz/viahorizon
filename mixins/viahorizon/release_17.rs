// SPDX-License-Identifier: GPL-3.0-or-later
// Release 1.7.2/1.7.6 Netty formats. Pre-Netty Release, Beta, Alpha and Classic
// deliberately do not enter this adapter even when protocol numbers overlap.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::Step,
    vfp_release_chain_data::{ENTITIES_17, METADATA_17, OBJECTS_17},
    vfp_release_codec::{self as codec, OldColumn, Position, Stack},
    vfp_release_metadata::{self as meta, Value},
    vfp_wire::Packet,
};
use horizon_world::nbt::Tag;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Legacy {
    player: i32,
    dimension: i32,
    names: BTreeMap<String, u128>,
    latency: BTreeMap<String, i32>,
    entities: BTreeMap<i32, &'static str>,
    windows: BTreeMap<u8, u8>,
    portals: PortalWorld,
    maps: BTreeMap<i32, MapState>,
}
#[derive(Default)]
struct MapState {
    scale: u8,
    icons: Vec<[u8; 3]>,
}
fn uuid(value: &str) -> Result<u128> {
    let compact: String = value
        .chars()
        .filter(|&character| character != '-')
        .collect();
    if compact.len() != 32 || !compact.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::new("Invalid 1.7 UUID"));
    }
    u128::from_str_radix(&compact, 16).map_err(|_| Error::new("Invalid 1.7 UUID"))
}
fn uuid_string(value: u128) -> String {
    let text = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &text[..8],
        &text[8..12],
        &text[12..16],
        &text[16..20],
        &text[20..]
    )
}
fn offline_uuid(name: &str) -> u128 {
    let mut bytes = crate::crypto::md5(format!("OfflinePlayer:{name}").as_bytes());
    bytes[6] = (bytes[6] & 15) | 48;
    bytes[8] = (bytes[8] & 63) | 128;
    u128::from_be_bytes(bytes)
}
const ORIGINAL_ITEM: &str = "viafabricplus:1_7_item";
const ORIGINAL_SKULL: &str = "viafabricplus:1_7_skull_owner";

fn substitute_item(id: i32, damage: i16) -> Option<(i32, i16)> {
    Some(match id {
        8 | 9 => (326, 0),
        10 | 11 => (327, 0),
        51 => (385, 0),
        90 => (399, 0),
        119 => (381, 0),
        127 => (351, 3),
        141 => (391, 0),
        142 => (392, 0),
        43 => (44, damage & 7),
        125 => (126, damage & 7),
        _ => return None,
    })
}

const ENCHANT_LAPIS: Stack = Stack {
    id: 351,
    count: 3,
    damage: 4,
    tag: Tag::End,
};

pub fn unsupported_item(id: i32, damage: i16) -> bool {
    matches!(id, 165..=169 | 176..=197 | 409..=416 | 423..=425 | 427..=431)
        || matches!(
            (id, damage),
            (1, 1..=6) | (3, 1) | (19, 1) | (383, 67 | 68 | 101)
        )
}

fn stack_17(stack: &mut Stack, inverse: bool) {
    if matches!(stack.tag, Tag::End) {
        stack.tag = Tag::Compound(Default::default());
    }
    let Tag::Compound(fields) = &mut stack.tag else {
        return;
    };
    if inverse {
        if let Some(Tag::IntArray(saved)) = fields.remove(&ORIGINAL_ITEM.into()) {
            if let [id, damage] = saved.as_slice() {
                if let Ok(damage) = i16::try_from(*damage) {
                    if substitute_item(*id, damage) == Some((stack.id, stack.damage)) {
                        (stack.id, stack.damage) = (*id, damage);
                    }
                }
            }
        }
        if let Some(Tag::String(name)) = fields.remove(&ORIGINAL_SKULL.into()) {
            fields.insert("SkullOwner".into(), Tag::String(name));
        }
    } else {
        if let Some(replacement) = substitute_item(stack.id, stack.damage) {
            fields.insert(
                ORIGINAL_ITEM.into(),
                Tag::IntArray(vec![stack.id, i32::from(stack.damage)]),
            );
            (stack.id, stack.damage) = replacement;
        }
        if stack.id == 397 && stack.damage == 3 {
            if let Some(Tag::String(name)) = fields.get(&"SkullOwner".into()).cloned() {
                fields.insert(ORIGINAL_SKULL.into(), Tag::String(name.clone()));
                fields.insert(
                    "SkullOwner".into(),
                    Tag::compound([("Name", Tag::String(name))]),
                );
            }
        }
    }
    if stack.id == 387 {
        if let Some(Tag::List(pages)) = fields.get_mut(&"pages".into()) {
            for page in pages {
                if let Tag::String(text) = page {
                    let old = text.to_string_lossy();
                    let new = if inverse {
                        crate::vfp_release_text::legacy(&old)
                    } else {
                        serde_json::json!({"text":old}).to_string()
                    };
                    *text = new.as_str().into();
                }
            }
        }
    }
    if fields.is_empty() {
        stack.tag = Tag::End;
    }
}

fn item(r: &mut Reader<'_>, w: &mut Writer, inverse: bool) -> Result<()> {
    let mut stack = Stack::read(r, if inverse { 47 } else { 5 })?;
    if let Some(stack) = &mut stack {
        stack_17(stack, inverse);
    }
    Stack::write(stack.as_ref(), w, if inverse { 5 } else { 47 })
}

fn read_position(r: &mut Reader<'_>, y: usize) -> Result<Position> {
    let x = r.i32()?;
    let y = match y {
        1 => i32::from(r.u8()?),
        2 => i32::from(r.i16()?),
        4 => r.i32()?,
        _ => return Err(Error::new("Invalid old position Y encoding")),
    };
    Ok(Position { x, y, z: r.i32()? })
}
fn old_position(r: &mut Reader<'_>, w: &mut Writer, y: usize) -> Result<()> {
    w.i64(read_position(r, y)?.pack(47)?);
    Ok(())
}

fn block_17(state: i32, portal_x: bool) -> i32 {
    let block = state >> 4;
    let damage = state & 15;
    match (block, damage) {
        (0, _) | (1, 1..=6) | (175, 9..=15) => block << 4,
        (65, 0..=1 | 6..=15) => 5 << 4,
        (62, 0..=1 | 6..=15) => (62 << 4) | 2,
        (54, 0) => (54 << 4) | 3,
        (90, 0) => (90 << 4) | if portal_x { 1 } else { 2 },
        _ => state,
    }
}

#[derive(Default)]
struct PortalWorld {
    chunks: BTreeMap<(i32, i32), BTreeMap<u16, PortalBlock>>,
}

#[derive(Clone, Copy)]
struct PortalBlock {
    state: i32,
    shown: Option<i32>,
}

fn chunk_key(pos: Position) -> (i32, i32) {
    (pos.x >> 4, pos.z >> 4)
}

fn cell_key(pos: Position) -> Option<u16> {
    if !(0..256).contains(&pos.y) {
        return None;
    }
    Some(((pos.y << 8) | ((pos.z & 15) << 4) | (pos.x & 15)) as u16)
}

fn cell_position((x, z): (i32, i32), cell: u16) -> Position {
    Position {
        x: (x << 4) | i32::from(cell & 15),
        y: i32::from(cell >> 8),
        z: (z << 4) | i32::from((cell >> 4) & 15),
    }
}

impl PortalWorld {
    fn clear(&mut self) {
        self.chunks.clear();
    }

    fn track(&mut self, pos: Position, state: i32) -> bool {
        let Some(cell) = cell_key(pos) else {
            return false;
        };
        let key = chunk_key(pos);
        if matches!(state >> 4, 49 | 90) {
            let chunk = self.chunks.entry(key).or_default();
            if chunk.get(&cell).is_some_and(|block| block.state == state) {
                return false;
            }
            chunk.insert(cell, PortalBlock { state, shown: None });
            return true;
        }
        let Some(chunk) = self.chunks.get_mut(&key) else {
            return false;
        };
        let removed = chunk.remove(&cell).is_some();
        if chunk.is_empty() {
            self.chunks.remove(&key);
        }
        removed
    }

    fn track_column(&mut self, column: &OldColumn) {
        let key = (column.x, column.z);
        if column.full {
            self.chunks.remove(&key);
        } else if let Some(chunk) = self.chunks.get_mut(&key) {
            chunk.retain(|cell, _| !column.sections.contains_key(&i32::from(*cell >> 12)));
        }
        for (&section, blocks) in &column.sections {
            for (index, &state) in blocks.blocks.iter().enumerate() {
                if matches!(state >> 4, 49 | 90) {
                    let cell = ((section << 12) | index as i32) as u16;
                    self.track(cell_position(key, cell), state);
                }
            }
        }
        if self.chunks.get(&key).is_some_and(BTreeMap::is_empty) {
            self.chunks.remove(&key);
        }
    }

    fn get(&self, pos: Position) -> Option<&PortalBlock> {
        self.chunks.get(&chunk_key(pos))?.get(&cell_key(pos)?)
    }

    fn axis_x(&self, pos: Position) -> bool {
        let mut score = [0; 2];
        for (axis, score) in score.iter_mut().enumerate() {
            for direction in [-1, 1] {
                // A maximum-width vanilla portal is 21 interior cells. A
                // frame beyond those cannot belong to this portal.
                for distance in 1..=22 {
                    let shift = direction * distance;
                    let other = if axis == 0 {
                        Position {
                            x: pos.x.wrapping_add(shift),
                            ..pos
                        }
                    } else {
                        Position {
                            z: pos.z.wrapping_add(shift),
                            ..pos
                        }
                    };
                    let Some(block) = self.get(other) else {
                        break;
                    };
                    if block.state >> 4 == 49 {
                        *score += 32;
                        break;
                    }
                    let orientation = block.state & 15;
                    if orientation != 0 && orientation != axis as i32 + 1 {
                        break;
                    }
                    *score += if orientation == 0 { 1 } else { 8 };
                }
            }
        }
        if score[0] == score[1] {
            return self
                .get(pos)
                .and_then(|block| block.shown)
                .is_some_and(|state| state & 15 == 1);
        }
        score[0] > score[1]
    }

    fn render(&mut self, pos: Position, state: i32) -> i32 {
        let converted = block_17(state, state == 90 << 4 && self.axis_x(pos));
        if state == 90 << 4 {
            if let Some(block) =
                cell_key(pos).and_then(|cell| self.chunks.get_mut(&chunk_key(pos))?.get_mut(&cell))
            {
                block.shown = Some(converted);
            }
        }
        converted
    }

    fn render_column(&mut self, column: &mut OldColumn) {
        let key = (column.x, column.z);
        for (&section, blocks) in &mut column.sections {
            for (index, state) in blocks.blocks.iter_mut().enumerate() {
                let cell = ((section << 12) | index as i32) as u16;
                *state = self.render(cell_position(key, cell), *state);
            }
        }
    }

    fn corrections(&mut self, step: &Step, changed: &BTreeSet<(i32, i32)>) -> Result<Vec<Packet>> {
        let mut affected = BTreeSet::new();
        for &(x, z) in changed {
            for delta in -2..=2 {
                affected.insert((x.wrapping_add(delta), z));
                affected.insert((x, z.wrapping_add(delta)));
            }
        }
        let mut output = Vec::new();
        for key in affected {
            let Some(chunk) = self.chunks.get(&key) else {
                continue;
            };
            let candidates = chunk
                .iter()
                .filter_map(|(&cell, block)| {
                    (block.state == 90 << 4).then_some((cell, block.shown))
                })
                .collect::<Vec<_>>();
            let mut changes = Vec::new();
            for (cell, before) in candidates {
                let state = self.render(cell_position(key, cell), 90 << 4);
                if before != Some(state) {
                    let pos = cell_position(key, cell);
                    if pos.pack(step.target).is_ok() {
                        let wire = (((pos.x & 15) << 12) | ((pos.z & 15) << 8) | pos.y) as u16;
                        changes.push((wire, state));
                    }
                }
            }
            if changes.is_empty() {
                continue;
            }
            let mut w = Writer::new();
            w.i32(key.0);
            w.i32(key.1);
            w.varint(changes.len() as i32);
            for (cell, state) in changes {
                w.u16(cell);
                w.varint(state);
            }
            output.push(step.incoming("CHUNK_BLOCKS_UPDATE", w.data)?);
        }
        Ok(output)
    }
}

fn metadata(r: &mut Reader<'_>, w: &mut Writer, kind: &str) -> Result<()> {
    let entries = meta::read(r, 5)?;
    let block_damage = if kind == "ENDERMAN" {
        entries
            .iter()
            .find(|entry| entry.index == 17 && entry.kind == 0)
            .and_then(|entry| entry.integer().ok())
            .unwrap_or(0)
    } else {
        0
    };
    let mut output = Vec::new();
    for mut entry in entries {
        let mut family = kind;
        let mapping = loop {
            if let Some(&row) = METADATA_17
                .iter()
                .find(|&&(name, index, _, _, _)| name == family && index == entry.index)
            {
                break Some(row);
            }
            if family.is_empty() {
                break None;
            }
            family = ENTITIES_17
                .iter()
                .find_map(|&(name, parent, _)| (name == family).then_some(parent))
                .unwrap_or("");
        };
        let Some((family, old_index, old_type, index, new_type)) = mapping else {
            continue;
        };
        if old_type != entry.kind || (family == "ENDERMAN" && old_index == 17) {
            continue;
        }
        entry.index = index;
        entry.kind = new_type;
        match (family, old_index) {
            ("ABSTRACT_AGEABLE", 12) => {
                entry.value = Value::Int(if entry.integer()? < 0 { -1 } else { 0 })
            }
            ("PLAYER", 16) => {
                entry.value = Value::Int(if entry.integer()? & 2 == 0 { 127 } else { 126 })
            }
            ("ITEM_FRAME", 3) => entry.value = Value::Int(entry.integer()? * 2),
            ("ENDERMAN", 16) => {
                entry.value = Value::Int((entry.integer()? & 255) | ((block_damage & 15) << 12))
            }
            _ => (),
        }
        if let Value::Item(Some(stack)) = &mut entry.value {
            stack_17(stack, false);
        }
        output.push(entry);
    }
    meta::write(w, &output, 47)
}

fn realign_y(kind: &str, y: i32) -> i32 {
    let offset = match kind {
        "FALLING_BLOCK" | "TNT" => 16,
        "END_CRYSTAL" => 32,
        "BOAT" => 10,
        "ITEM" => 4,
        "LEASH_KNOT" => 16,
        "EXPERIENCE_ORB" => 8,
        name if name.ends_with("MINECART") => 12,
        _ => 0,
    };
    y.saturating_sub(offset)
}

impl Legacy {
    pub fn clientbound(
        &mut self,
        step: &'static Step,
        id: i32,
        data: &[u8],
    ) -> Result<Vec<Packet>> {
        let name = step.incoming_name(id)?;
        let mut target = name;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut before = Vec::new();
        if step.source == 4 {
            if name == "ADD_PLAYER" {
                w.varint(r.varint()?);
                w.string(&uuid_string(uuid(&r.string(36)?)?));
                w.string(&r.string(16)?);
                w.varint(0);
                w.bytes(r.rest());
            } else {
                w.bytes(data);
            }
            return Ok(vec![step.incoming(name, w.data)?]);
        }
        match name {
            "KEEP_ALIVE" => {
                w.varint(r.i32()?);
                codec::end(&r)?;
            }
            "LOGIN" => {
                self.player = r.i32()?;
                w.i32(self.player);
                w.u8(r.u8()?);
                self.dimension = i32::from(r.i8()?);
                w.i8(self.dimension as i8);
                w.bytes(r.rest());
                w.bool(false);
                self.names.clear();
                self.latency.clear();
                self.entities.clear();
                self.entities.insert(self.player, "PLAYER");
                self.windows.clear();
                self.portals.clear();
                self.maps.clear();
            }
            "RESPAWN" => {
                let dimension = r.i32()?;
                if self.dimension != dimension {
                    self.portals.clear();
                }
                self.dimension = dimension;
                self.entities.clear();
                self.entities.insert(self.player, "PLAYER");
                self.windows.clear();
                w.bytes(data);
            }
            "CHAT" => {
                let text = r.string(262144)?;
                w.string(&crate::vfp_release_text::component(&text));
                codec::end(&r)?;
                w.u8(0);
            }
            "SET_EQUIPPED_ITEM" => {
                w.varint(r.i32()?);
                w.i16(r.i16()?);
                item(&mut r, &mut w, false)?;
                codec::end(&r)?;
            }
            "SET_DEFAULT_SPAWN_POSITION" | "OPEN_SIGN_EDITOR" => {
                old_position(&mut r, &mut w, 4)?;
                codec::end(&r)?;
            }
            "SET_HEALTH" => {
                w.f32(r.f32()?);
                w.varint(i32::from(r.i16()?));
                w.f32(r.f32()?);
                codec::end(&r)?;
            }
            "PLAYER_POSITION" => {
                w.f64(r.f64()?);
                w.f64(r.f64()? - f64::from(1.62f32));
                w.f64(r.f64()?);
                w.f32(r.f32()?);
                w.f32(r.f32()?);
                codec::boolean(&mut r)?;
                w.u8(0);
                codec::end(&r)?;
            }
            "PLAYER_SLEEP" => {
                w.varint(r.i32()?);
                old_position(&mut r, &mut w, 1)?;
                codec::end(&r)?;
            }
            "ADD_PLAYER" => {
                let entity = r.varint()?;
                self.entities.insert(entity, "PLAYER");
                w.varint(entity);
                let profile = uuid(&r.string(36)?)?;
                w.bytes(&profile.to_be_bytes());
                let name = r.string(16)?;
                let count = r.count(1024)?;
                let mut properties = Writer::new();
                properties.varint(count as i32);
                for _ in 0..count {
                    properties.string(&r.string(64)?);
                    properties.string(&r.string(32767)?);
                    let signature = r.string(4096)?;
                    properties.bool(!signature.is_empty());
                    if !signature.is_empty() {
                        properties.string(&signature);
                    }
                }
                if let Some(previous) = self
                    .names
                    .insert(name.clone(), profile)
                    .filter(|&previous| previous != profile)
                {
                    let mut remove = Writer::new();
                    remove.varint(4);
                    remove.varint(1);
                    remove.bytes(&previous.to_be_bytes());
                    before.push(step.incoming("PLAYER_INFO", remove.data)?);
                }
                let mut info = Writer::new();
                info.varint(0);
                info.varint(1);
                info.bytes(&profile.to_be_bytes());
                info.string(&name);
                info.bytes(&properties.data);
                info.varint(0);
                info.varint(self.latency.get(&name).copied().unwrap_or_default());
                info.bool(false);
                before.push(step.incoming("PLAYER_INFO", info.data)?);
                w.bytes(r.bytes(12 + 2 + 2)?);
                metadata(&mut r, &mut w, "PLAYER")?;
                codec::end(&r)?;
            }
            "ADD_MOB" => {
                let entity = r.varint()?;
                let id = r.u8()?;
                let kind = ENTITIES_17
                    .iter()
                    .find_map(|&(name, _, kind)| (kind == i32::from(id)).then_some(name))
                    .unwrap_or("ENTITY");
                self.entities.insert(entity, kind);
                w.varint(entity);
                w.u8(id);
                w.bytes(r.bytes(21)?);
                metadata(&mut r, &mut w, kind)?;
                codec::end(&r)?;
            }
            "ADD_ENTITY" => {
                let entity = r.varint()?;
                let object = r.u8()?;
                let mut x = r.i32()?;
                let y = r.i32()?;
                let mut z = r.i32()?;
                let pitch = r.i8()?;
                let mut yaw = r.i8()?;
                let data = r.i32()?;
                let lookup = |variant| {
                    OBJECTS_17.iter().find_map(|&(id, key, name)| {
                        (id == i32::from(object) && key == variant).then_some(name)
                    })
                };
                let kind = lookup(data).or_else(|| lookup(-1)).unwrap_or("ENTITY");
                self.entities.insert(entity, kind);
                if kind == "ITEM_FRAME" {
                    match data {
                        0 => {
                            z = z.saturating_add(32);
                            yaw = 0;
                        }
                        1 => {
                            x = x.saturating_sub(32);
                            yaw = 64;
                        }
                        2 => {
                            z = z.saturating_sub(32);
                            yaw = -128;
                        }
                        3 => {
                            x = x.saturating_add(32);
                            yaw = -64;
                        }
                        _ => (),
                    }
                }
                w.varint(entity);
                w.u8(object);
                w.i32(x);
                w.i32(realign_y(kind, y));
                w.i32(z);
                w.i8(pitch);
                w.i8(yaw);
                let mapped_data = if kind == "FALLING_BLOCK" {
                    let state = block_17(((data & 4095) << 4) | ((data >> 16) & 15), false);
                    (state >> 4) | ((state & 15) << 12)
                } else {
                    data
                };
                w.i32(mapped_data);
                let velocity = if data > 0 { r.bytes(6)? } else { &[0; 6] };
                if mapped_data > 0 {
                    w.bytes(velocity);
                }
                codec::end(&r)?;
            }
            "ADD_EXPERIENCE_ORB" => {
                let entity = r.varint()?;
                self.entities.insert(entity, "EXPERIENCE_ORB");
                w.varint(entity);
                w.i32(r.i32()?);
                w.i32(realign_y("EXPERIENCE_ORB", r.i32()?));
                w.i32(r.i32()?);
                w.i16(r.i16()?);
                codec::end(&r)?;
            }
            "ADD_PAINTING" => {
                w.varint(r.varint()?);
                w.string(&r.string(32767)?);
                old_position(&mut r, &mut w, 4)?;
                w.i8(i8::try_from(r.i32()?)
                    .map_err(|_| Error::new("Invalid 1.7 painting direction"))?);
                codec::end(&r)?;
            }
            "TAKE_ITEM_ENTITY" => {
                w.varint(r.i32()?);
                w.varint(r.i32()?);
                codec::end(&r)?;
            }
            "SET_ENTITY_MOTION" | "ROTATE_HEAD" | "REMOVE_MOB_EFFECT" => {
                w.varint(r.i32()?);
                w.bytes(r.rest());
            }
            "REMOVE_ENTITIES" => {
                let count = r.u8()?;
                w.varint(i32::from(count));
                for _ in 0..count {
                    let entity = r.i32()?;
                    self.entities.remove(&entity);
                    w.varint(entity);
                }
                codec::end(&r)?;
            }
            "MOVE_ENTITY" => {
                w.varint(r.i32()?);
                codec::end(&r)?;
            }
            "TELEPORT_ENTITY" => {
                let entity = r.i32()?;
                let kind = self.entities.get(&entity).copied().unwrap_or("ENTITY");
                w.varint(entity);
                w.i32(r.i32()?);
                w.i32(realign_y(kind, r.i32()?));
                w.i32(r.i32()?);
                w.bytes(r.bytes(2)?);
                w.bool(true);
                codec::end(&r)?;
            }
            "MOVE_ENTITY_POS" | "MOVE_ENTITY_ROT" | "MOVE_ENTITY_POS_ROT" => {
                w.varint(r.i32()?);
                w.bytes(r.rest());
                w.bool(true);
            }
            "SET_ENTITY_DATA" => {
                let entity = r.i32()?;
                w.varint(entity);
                metadata(
                    &mut r,
                    &mut w,
                    self.entities.get(&entity).copied().unwrap_or("ENTITY"),
                )?;
                codec::end(&r)?;
            }
            "UPDATE_MOB_EFFECT" => {
                w.varint(r.i32()?);
                w.i8(r.i8()?);
                w.i8(r.i8()?);
                w.varint(i32::from(r.i16()?));
                w.bool(false);
                codec::end(&r)?;
            }
            "SET_EXPERIENCE" => {
                w.f32(r.f32()?);
                w.varint(i32::from(r.i16()?));
                w.varint(i32::from(r.i16()?));
                codec::end(&r)?;
            }
            "UPDATE_ATTRIBUTES" => {
                w.varint(r.i32()?);
                let count = r.i32()?;
                if !(0..=1024).contains(&count) {
                    return Err(Error::new("Invalid old attribute count"));
                }
                w.i32(count);
                for _ in 0..count {
                    w.string(&r.string(64)?);
                    w.f64(r.f64()?);
                    let modifiers = r.u16()?;
                    w.varint(i32::from(modifiers));
                    if modifiers > 1024 {
                        return Err(Error::new("Too many old attribute modifiers"));
                    }
                    w.bytes(r.bytes(usize::from(modifiers) * 25)?);
                }
                codec::end(&r)?;
            }
            "LEVEL_CHUNK" => {
                let sky = self.dimension == 0;
                let mut column = OldColumn::read_17(&mut r, sky)?;
                codec::end(&r)?;
                let scope = [(column.x, column.z)].into_iter().collect();
                self.portals.track_column(&column);
                self.portals.render_column(&mut column);
                column.write_18(&mut w, sky)?;
                let mut output = vec![step.incoming(name, w.data)?];
                output.extend(self.portals.corrections(step, &scope)?);
                return Ok(output);
            }
            "MAP_BULK_CHUNK" => return self.bulk(step, data),
            "BLOCK_UPDATE" => {
                let pos = read_position(&mut r, 1)?;
                let block = r.varint()?;
                let damage = r.u8()?;
                if !(0..=4095).contains(&block) || damage > 15 {
                    return Err(Error::new("Invalid 1.7 block update"));
                }
                codec::end(&r)?;
                let state = (block << 4) | i32::from(damage);
                let changed = self.portals.track(pos, state);
                w.i64(pos.pack(step.target)?);
                w.varint(self.portals.render(pos, state));
                let mut output = vec![step.incoming(name, w.data)?];
                if changed {
                    output.extend(
                        self.portals
                            .corrections(step, &[chunk_key(pos)].into_iter().collect())?,
                    );
                }
                return Ok(output);
            }
            "CHUNK_BLOCKS_UPDATE" => {
                let key = (r.i32()?, r.i32()?);
                let count = r.u16()?;
                let length = r.i32()?;
                if length != i32::from(count) * 4 {
                    return Err(Error::new("1.7 block-change array length mismatch"));
                }
                let mut changes = BTreeMap::new();
                for _ in 0..count {
                    changes.insert(r.u16()?, i32::from(r.u16()?));
                }
                codec::end(&r)?;
                for (&offset, &state) in &changes {
                    let cell = ((offset & 255) << 8) | ((offset >> 4) & 240) | (offset >> 12);
                    self.portals.track(cell_position(key, cell), state);
                }
                w.i32(key.0);
                w.i32(key.1);
                w.varint(changes.len() as i32);
                for (offset, state) in changes {
                    let cell = ((offset & 255) << 8) | ((offset >> 4) & 240) | (offset >> 12);
                    w.u16(offset);
                    w.varint(self.portals.render(cell_position(key, cell), state));
                }
                let mut output = vec![step.incoming(name, w.data)?];
                output.extend(
                    self.portals
                        .corrections(step, &[key].into_iter().collect())?,
                );
                return Ok(output);
            }
            "EXPLODE" => {
                let origin = [r.f32()? as i32, r.f32()? as i32, r.f32()? as i32];
                r.f32()?;
                let count = usize::try_from(r.i32()?)
                    .ok()
                    .filter(|&count| count <= 65536)
                    .ok_or_else(|| Error::new("Invalid 1.7 explosion record count"))?;
                let mut changed = BTreeSet::new();
                for _ in 0..count {
                    let pos = Position {
                        x: origin[0].wrapping_add(i32::from(r.i8()?)),
                        y: origin[1].wrapping_add(i32::from(r.i8()?)),
                        z: origin[2].wrapping_add(i32::from(r.i8()?)),
                    };
                    if self.portals.get(pos).is_some() {
                        self.portals.track(pos, 0);
                        changed.insert(chunk_key(pos));
                    }
                }
                r.bytes(12)?;
                codec::end(&r)?;
                let mut output = vec![step.incoming(name, data.to_vec())?];
                output.extend(self.portals.corrections(step, &changed)?);
                return Ok(output);
            }
            "BLOCK_EVENT" => {
                old_position(&mut r, &mut w, 2)?;
                w.bytes(r.rest());
            }
            "BLOCK_DESTRUCTION" => {
                w.varint(r.varint()?);
                old_position(&mut r, &mut w, 4)?;
                w.i8(r.i8()?);
                codec::end(&r)?;
            }
            "LEVEL_EVENT" => {
                let event = r.i32()?;
                w.i32(event);
                old_position(&mut r, &mut w, 1)?;
                let data = r.i32()?;
                w.i32(if event == 2001 {
                    let state = block_17(((data & 4095) << 4) | ((data >> 12) & 15), false);
                    (state >> 4) | ((state & 15) << 12)
                } else {
                    data
                });
                w.bool(codec::boolean(&mut r)?);
                codec::end(&r)?;
            }
            "OPEN_SCREEN" => {
                let window = r.u8()?;
                let kind = r.u8()?;
                let mut title = r.string(32767)?;
                let slots = r.u8()?;
                let mut custom = codec::boolean(&mut r)?;
                let (menu, fallback) = match kind {
                    0 => ("minecraft:chest", "container.chest"),
                    1 => ("minecraft:crafting_table", "container.crafting"),
                    2 => ("minecraft:furnace", "container.furnace"),
                    3 => ("minecraft:dispenser", "container.dispenser"),
                    4 => ("minecraft:enchanting_table", "container.enchant"),
                    5 => ("minecraft:brewing_stand", "container.brewing"),
                    6 => ("minecraft:villager", "entity.Villager.name"),
                    7 => ("minecraft:beacon", "container.beacon"),
                    8 => ("minecraft:anvil", "container.repair"),
                    9 => ("minecraft:hopper", "container.hopper"),
                    10 => ("minecraft:dropper", "container.dropper"),
                    11 => ("EntityHorse", "entity.Horse.name"),
                    _ => return Ok(Vec::new()),
                };
                if matches!(kind, 1 | 8) || (kind == 6 && title.is_empty()) {
                    custom = false;
                }
                if !custom && kind != 0 && kind != 11 {
                    title = fallback.to_owned();
                }
                self.windows.insert(window, kind);
                w.u8(window);
                w.string(menu);
                let title = if custom {
                    crate::vfp_release_text::json(&title)
                } else {
                    serde_json::json!({"translate":title}).to_string()
                };
                w.string(&title);
                w.u8(if matches!(kind, 1 | 4 | 8) { 0 } else { slots });
                if kind == 11 {
                    w.i32(r.i32()?);
                }
                codec::end(&r)?;
            }
            "CONTAINER_CLOSE" => {
                let window = r.u8()?;
                self.windows.remove(&window);
                w.u8(window);
                codec::end(&r)?;
            }
            "CONTAINER_SET_SLOT" => {
                let window = r.u8()?;
                let mut slot = r.i16()?;
                let enchant = self.windows.get(&window) == Some(&4);
                if enchant && slot >= 1 {
                    slot = slot
                        .checked_add(1)
                        .ok_or_else(|| Error::new("1.7 enchantment slot overflow"))?;
                }
                w.u8(window);
                w.i16(slot);
                item(&mut r, &mut w, false)?;
                codec::end(&r)?;
                if enchant && slot == 0 {
                    let mut lapis = Writer::new();
                    lapis.u8(window);
                    lapis.i16(1);
                    Stack::write(Some(&ENCHANT_LAPIS), &mut lapis, 47)?;
                    before.push(step.incoming(name, lapis.data)?);
                }
            }
            "CONTAINER_SET_CONTENT" => {
                let window = r.u8()?;
                w.u8(window);
                let count = r.i16()?;
                if !(0..=4096).contains(&count) {
                    return Err(Error::new("Invalid old window length"));
                }
                let enchanting = self.windows.get(&window) == Some(&4) && count > 0;
                w.i16(count + if enchanting { 1 } else { 0 });
                for slot in 0..count {
                    item(&mut r, &mut w, false)?;
                    if enchanting && slot == 0 {
                        Stack::write(Some(&ENCHANT_LAPIS), &mut w, 47)?;
                    }
                }
                codec::end(&r)?;
            }
            "CONTAINER_SET_DATA" => {
                let window = r.u8()?;
                let property = r.i16()?;
                let value = r.i16()?;
                w.u8(window);
                if self.windows.get(&window) == Some(&2) {
                    w.i16(match property {
                        0 => 2,
                        1 => 0,
                        2 => 1,
                        _ => property,
                    });
                    if property == 0 {
                        let mut total = Writer::new();
                        total.u8(window);
                        total.i16(3);
                        total.i16(200);
                        before.push(step.incoming(name, total.data)?);
                    }
                } else if self.windows.get(&window) == Some(&4) && (0..=2).contains(&property) {
                    // 1.7 provides the cost but no 1.8 enchantment preview.
                    let mut preview = Writer::new();
                    preview.u8(window);
                    preview.i16(property + 4);
                    preview.i16(-1);
                    before.push(step.incoming(name, preview.data)?);
                    w.i16(property);
                } else {
                    w.i16(property);
                }
                w.i16(value);
                codec::end(&r)?;
            }
            "MAP_ITEM_DATA" => {
                let id = r.varint()?;
                let length = usize::from(r.u16()?);
                let mut update = Reader::new(r.bytes(length)?);
                codec::end(&r)?;
                if !self.maps.contains_key(&id) && self.maps.len() >= 1024 {
                    if let Some(id) = self.maps.keys().next().copied() {
                        self.maps.remove(&id);
                    }
                }
                let state = self.maps.entry(id).or_default();
                let mut pixels = None;
                match update.u8()? {
                    0 => {
                        let x = update.u8()?;
                        let z = update.u8()?;
                        let colors = update.rest();
                        if x >= 128
                            || z >= 128
                            || colors.is_empty()
                            || colors.len() > 128 - usize::from(z)
                        {
                            return Err(Error::new("Invalid 1.7 map column"));
                        }
                        pixels = Some((x, z, colors));
                    }
                    1 => {
                        if update.remaining() % 3 != 0 || update.remaining() > 3072 {
                            return Err(Error::new("Invalid 1.7 map icons"));
                        }
                        state.icons = update
                            .rest()
                            .chunks_exact(3)
                            .map(|icon| [icon[0].rotate_left(4), icon[1], icon[2]])
                            .collect();
                    }
                    2 => {
                        state.scale = update.u8()?;
                    }
                    _ => return Ok(Vec::new()),
                }
                codec::end(&update)?;
                w.varint(id);
                w.u8(state.scale);
                w.varint(state.icons.len() as i32);
                for icon in &state.icons {
                    w.bytes(icon);
                }
                if let Some((x, z, colors)) = pixels {
                    w.u8(1);
                    w.u8(colors.len() as u8);
                    w.u8(x);
                    w.u8(z);
                    w.varint(colors.len() as i32);
                    w.bytes(colors);
                } else {
                    w.u8(0);
                }
            }
            "LEVEL_PARTICLES" => {
                const PARTICLES: &[&str] = &[
                    "explode",
                    "largeexplode",
                    "hugeexplosion",
                    "fireworksSpark",
                    "bubble",
                    "splash",
                    "wake",
                    "suspended",
                    "depthsuspend",
                    "crit",
                    "magicCrit",
                    "smoke",
                    "largesmoke",
                    "spell",
                    "instantSpell",
                    "mobSpell",
                    "mobSpellAmbient",
                    "witchMagic",
                    "dripWater",
                    "dripLava",
                    "angryVillager",
                    "happyVillager",
                    "townaura",
                    "note",
                    "portal",
                    "enchantmenttable",
                    "flame",
                    "lava",
                    "footstep",
                    "cloud",
                    "reddust",
                    "snowballpoof",
                    "snowshovel",
                    "slime",
                    "heart",
                    "barrier",
                    "iconcrack",
                    "blockcrack",
                    "blockdust",
                    "droplet",
                    "take",
                    "mobappearance",
                ];
                let name = r.string(32767)?;
                let mut fields = name.splitn(3, '_');
                let key = fields.next().unwrap_or_default();
                let Some(id) = PARTICLES.iter().position(|&name| name == key) else {
                    return Ok(Vec::new());
                };
                w.i32(id as i32);
                w.bool(false);
                w.bytes(r.bytes(32)?);
                let parse = |value: Option<&str>| value.and_then(|value| value.parse::<i32>().ok());
                if (36..=38).contains(&id) {
                    let Some(block) = parse(fields.next()).filter(|id| (0..=4095).contains(id))
                    else {
                        return Ok(Vec::new());
                    };
                    let damage = parse(fields.next()).unwrap_or(0);
                    if id == 36 {
                        w.varint(block);
                        w.varint(damage);
                    } else {
                        w.varint(block | ((damage & 15) << 12));
                    }
                }
                codec::end(&r)?;
            }
            "BLOCK_ENTITY_DATA" => {
                old_position(&mut r, &mut w, 2)?;
                let action = r.u8()?;
                w.u8(action);
                let mut tag = codec::read_compressed_nbt(&mut r)?;
                if action == 4 {
                    if let Tag::Compound(fields) = &mut tag {
                        if matches!(fields.get(&"SkullType".into()), Some(Tag::Byte(3)))
                            && !fields.contains_key(&"Owner".into())
                        {
                            if let Some(Tag::String(name)) = fields.remove(&"ExtraType".into()) {
                                fields.insert(
                                    "Owner".into(),
                                    Tag::compound([("Name", Tag::String(name))]),
                                );
                            }
                        }
                    }
                }
                w.vfp_named_nbt(&tag)?;
                codec::end(&r)?;
            }
            "UPDATE_SIGN" => {
                old_position(&mut r, &mut w, 2)?;
                for _ in 0..4 {
                    let line = r.string(262144)?;
                    w.string(&crate::vfp_release_text::json(&line));
                }
                codec::end(&r)?;
            }
            "PLAYER_INFO" => {
                let name = r.string(16)?;
                let online = codec::boolean(&mut r)?;
                let ping = r.i16()?;
                codec::end(&r)?;
                let existed = self.names.contains_key(&name);
                let id = *self
                    .names
                    .entry(name.clone())
                    .or_insert_with(|| offline_uuid(&name));
                w.varint(if !online {
                    4
                } else if existed {
                    2
                } else {
                    0
                });
                w.varint(1);
                w.bytes(&id.to_be_bytes());
                if online {
                    self.latency.insert(name.clone(), i32::from(ping));
                    if !existed {
                        w.string(&name);
                        w.varint(0);
                        w.varint(0);
                    }
                    w.varint(i32::from(ping));
                    if !existed {
                        w.bool(false);
                    }
                } else {
                    self.names.remove(&name);
                    self.latency.remove(&name);
                }
            }
            "SET_OBJECTIVE" => {
                w.string(&r.string(16)?);
                let value = r.string(32)?;
                let mode = r.u8()?;
                w.u8(mode);
                if mode == 0 || mode == 2 {
                    w.string(&value);
                    w.string("integer");
                }
                codec::end(&r)?;
            }
            "SET_SCORE" => {
                w.string(&r.string(40)?);
                let action = r.u8()?;
                w.varint(i32::from(action));
                if action == 0 {
                    w.string(&r.string(16)?);
                    w.varint(r.i32()?);
                } else {
                    w.string("");
                }
                codec::end(&r)?;
            }
            "SET_PLAYER_TEAM" => {
                w.string(&r.string(16)?);
                let action = r.u8()?;
                w.u8(action);
                if action == 0 || action == 2 {
                    for _ in 0..3 {
                        w.string(&r.string(32)?);
                    }
                    w.u8(r.u8()?);
                    w.string("always");
                    w.u8(0);
                }
                if action == 0 || action == 3 || action == 4 {
                    let count = r.u16()?;
                    w.varint(i32::from(count));
                    for _ in 0..count {
                        w.string(&r.string(40)?);
                    }
                }
                codec::end(&r)?;
            }
            "CUSTOM_PAYLOAD" => {
                let channel = r.string(20)?;
                let length = usize::from(r.u16()?);
                if length != r.remaining() {
                    return Err(Error::new("1.7 custom payload length mismatch"));
                }
                if channel == "MC|Brand" {
                    w.string(&channel);
                    let value = std::str::from_utf8(r.rest())
                        .map_err(|_| Error::new("Invalid 1.7 brand UTF-8"))?;
                    w.string(value);
                } else if channel == "MC|RPack" {
                    target = "RESOURCE_PACK";
                    let value = std::str::from_utf8(r.rest())
                        .map_err(|_| Error::new("Invalid 1.7 resource pack URL"))?;
                    w.string(value);
                    w.string("legacy");
                } else if channel == "MC|TrList" {
                    w.string(&channel);
                    w.i32(r.i32()?);
                    let count = r.u8()?;
                    w.u8(count);
                    for _ in 0..count {
                        item(&mut r, &mut w, false)?;
                        item(&mut r, &mut w, false)?;
                        let second = codec::boolean(&mut r)?;
                        w.bool(second);
                        if second {
                            item(&mut r, &mut w, false)?;
                        }
                        w.bool(codec::boolean(&mut r)?);
                        w.i32(0);
                        w.i32(i32::MAX);
                    }
                    codec::end(&r)?;
                } else {
                    w.string(&channel);
                    w.bytes(r.rest());
                }
            }
            // These 1.7 packets retain their complete 1.8 primitive layout.
            "SET_TIME"
            | "SET_CARRIED_ITEM"
            | "ANIMATE"
            | "ENTITY_EVENT"
            | "SET_ENTITY_LINK"
            | "CUSTOM_SOUND"
            | "GAME_EVENT"
            | "ADD_GLOBAL_ENTITY"
            | "CONTAINER_ACK"
            | "AWARD_STATS"
            | "PLAYER_ABILITIES"
            | "COMMAND_SUGGESTIONS"
            | "SET_DISPLAY_OBJECTIVE"
            | "DISCONNECT" => {
                w.bytes(data);
            }
            _ => return Err(Error::new(format!("Unconverted 1.7 packet {name}"))),
        }
        before.push(step.incoming(target, w.data)?);
        Ok(before)
    }
    fn bulk(&mut self, step: &'static Step, data: &[u8]) -> Result<Vec<Packet>> {
        let mut r = Reader::new(data);
        let count = usize::from(r.u16()?);
        if count > 1024 {
            return Err(Error::new("Too many 1.7 bulk columns"));
        }
        let size =
            usize::try_from(r.i32()?).map_err(|_| Error::new("Negative bulk compressed size"))?;
        if size > crate::vfp_core::MAX_PACKET {
            return Err(Error::new("Bulk compressed chunk exceeds bound"));
        }
        let sky = codec::boolean(&mut r)?;
        let compressed = r.bytes(size)?;
        let mut descriptors = Vec::new();
        let mut expected = 0usize;
        for _ in 0..count {
            let x = r.i32()?;
            let z = r.i32()?;
            let mask = r.u16()?;
            let add = r.u16()?;
            if add & !mask != 0 {
                return Err(Error::new("Invalid bulk add mask"));
            }
            let size = mask.count_ones() as usize
                * (4096 + 2048 + 2048 + if sky { 2048 } else { 0 })
                + add.count_ones() as usize * 2048
                + 256;
            expected = expected
                .checked_add(size)
                .ok_or_else(|| Error::new("Bulk chunk size overflow"))?;
            descriptors.push((x, z, mask, add, size));
        }
        codec::end(&r)?;
        let bytes = codec::inflate_zlib(compressed, expected)?;
        let mut offset = 0;
        let mut columns = Vec::new();
        let mut scope = BTreeSet::new();
        for (x, z, mask, add, size) in descriptors {
            let column =
                OldColumn::arrays_17(x, z, true, mask, add, sky, &bytes[offset..offset + size])?;
            offset += size;
            self.portals.track_column(&column);
            scope.insert((x, z));
            columns.push(column);
        }
        let mut result = Vec::new();
        for mut column in columns {
            self.portals.render_column(&mut column);
            let mut w = Writer::new();
            column.write_18(&mut w, sky)?;
            result.push(step.incoming("LEVEL_CHUNK", w.data)?);
        }
        result.extend(self.portals.corrections(step, &scope)?);
        Ok(result)
    }
    pub fn serverbound(
        &mut self,
        step: &'static Step,
        packets: Vec<Packet>,
    ) -> Result<Vec<Packet>> {
        if step.source == 4 {
            return packets
                .into_iter()
                .map(|(id, data)| step.outgoing(step.outgoing_name(id)?, data))
                .collect();
        }
        let mut result = Vec::new();
        for (id, data) in packets {
            let name = step.outgoing_name(id)?;
            let mut r = Reader::new(&data);
            let mut w = Writer::new();
            match name {
                "KEEP_ALIVE" => {
                    w.i32(r.varint()?);
                    codec::end(&r)?;
                }
                "INTERACT" => {
                    w.i32(r.varint()?);
                    let action = r.varint()?;
                    if action == 2 {
                        r.bytes(12)?;
                        w.u8(0);
                    } else {
                        w.u8(u8::try_from(action)
                            .map_err(|_| Error::new("Invalid old interaction"))?);
                    }
                    codec::end(&r)?;
                }
                "MOVE_PLAYER_POS" | "MOVE_PLAYER_POS_ROT" => {
                    w.f64(r.f64()?);
                    let y = r.f64()?;
                    w.f64(y);
                    w.f64(y + 1.62);
                    w.f64(r.f64()?);
                    w.bytes(r.rest());
                }
                "PLAYER_ACTION" => {
                    w.u8(u8::try_from(r.varint()?)
                        .map_err(|_| Error::new("Old player action exceeds byte"))?);
                    Position::unpack(r.i64()?, 47).write(&mut w, 5, false)?;
                    w.u8(r.u8()?);
                    codec::end(&r)?;
                }
                "USE_ITEM_ON" => {
                    let position = Position::unpack(r.i64()?, 47);
                    let face = r.u8()?;
                    if position
                        == (Position {
                            x: -1,
                            y: -1,
                            z: -1,
                        })
                        && face == 255
                    {
                        // Air use has no block target. 1.8 packs Y=-1, whereas
                        // 1.7 sends the same sentinel as an unsigned byte.
                        w.i32(-1);
                        w.u8(255);
                        w.i32(-1);
                    } else {
                        position.write(&mut w, 5, false)?;
                    }
                    w.u8(face);
                    item(&mut r, &mut w, true)?;
                    w.bytes(r.bytes(3)?);
                    codec::end(&r)?;
                }
                "SWING" => {
                    w.i32(self.player);
                    w.u8(1);
                }
                "PLAYER_COMMAND" => {
                    w.i32(r.varint()?);
                    let action = r
                        .varint()?
                        .checked_add(1)
                        .ok_or_else(|| Error::new("Old player command overflow"))?;
                    w.u8(u8::try_from(action)
                        .map_err(|_| Error::new("Invalid old player command"))?);
                    w.i32(r.varint()?);
                    codec::end(&r)?;
                }
                "PLAYER_INPUT" => {
                    w.f32(r.f32()?);
                    w.f32(r.f32()?);
                    let flags = r.u8()?;
                    w.bool(flags & 1 != 0);
                    w.bool(flags & 2 != 0);
                    codec::end(&r)?;
                }
                "CONTAINER_CLICK" => {
                    let window = r.u8()?;
                    let mut slot = r.i16()?;
                    if self.windows.get(&window) == Some(&4) {
                        if slot == 1 {
                            continue;
                        }
                        if slot >= 2 {
                            slot -= 1;
                        }
                    }
                    w.u8(window);
                    w.i16(slot);
                    w.bytes(r.bytes(4)?);
                    item(&mut r, &mut w, true)?;
                    codec::end(&r)?;
                }
                "CONTAINER_CLOSE" => {
                    let window = r.u8()?;
                    self.windows.remove(&window);
                    w.u8(window);
                    codec::end(&r)?;
                }
                "SET_CREATIVE_MODE_SLOT" => {
                    w.i16(r.i16()?);
                    let mut stack = Stack::read(&mut r, 47)?;
                    codec::end(&r)?;
                    if let Some(stack) = &mut stack {
                        stack_17(stack, true);
                        if unsupported_item(stack.id, stack.damage) {
                            continue;
                        }
                    }
                    Stack::write(stack.as_ref(), &mut w, 5)?;
                }
                "SIGN_UPDATE" => {
                    Position::unpack(r.i64()?, 47).write(&mut w, 5, true)?;
                    for _ in 0..4 {
                        let text: serde_json::Value = serde_json::from_str(&r.string(262144)?)
                            .map_err(|_| Error::new("Invalid old sign component"))?;
                        w.string(&plain(&text));
                    }
                    codec::end(&r)?;
                }
                "COMMAND_SUGGESTION" => {
                    w.string(&r.string(32767)?);
                    r.rest();
                }
                "CLIENT_INFORMATION" => {
                    w.string(&r.string(16)?);
                    w.i8(r.i8()?);
                    w.i8(r.i8()?);
                    w.bool(codec::boolean(&mut r)?);
                    w.u8(2);
                    w.bool(r.u8()? & 1 != 0);
                    codec::end(&r)?;
                }
                "CUSTOM_PAYLOAD" => {
                    let channel = r.string(20)?;
                    let mut body = Writer::new();
                    match channel.as_str() {
                        "MC|BEdit" | "MC|BSign" => {
                            item(&mut r, &mut body, true)?;
                            codec::end(&r)?;
                        }
                        "MC|Brand" | "MC|ItemName" => {
                            body.bytes(r.string(32767)?.as_bytes());
                            codec::end(&r)?;
                        }
                        _ => {
                            body.bytes(r.rest());
                        }
                    }
                    w.string(&channel);
                    w.u16(
                        u16::try_from(body.data.len())
                            .map_err(|_| Error::new("1.7 payload exceeds short"))?,
                    );
                    w.bytes(&body.data);
                }
                "TELEPORT_TO_ENTITY" | "RESOURCE_PACK" => {
                    continue;
                }
                "CHAT"
                | "MOVE_PLAYER_STATUS_ONLY"
                | "MOVE_PLAYER_ROT"
                | "SET_CARRIED_ITEM"
                | "CONTAINER_ACK"
                | "CONTAINER_BUTTON_CLICK"
                | "PLAYER_ABILITIES"
                | "CLIENT_COMMAND" => {
                    w.bytes(&data);
                }
                _ => return Err(Error::new(format!("Unconverted 1.7 reply {name}"))),
            }
            result.push(step.outgoing(name, w.data)?);
        }
        Ok(result)
    }
}
fn plain(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values.iter().map(plain).collect(),
        serde_json::Value::Object(fields) => {
            let mut value = fields
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();
            if let Some(serde_json::Value::Array(extra)) = fields.get("extra") {
                for extra in extra {
                    value.push_str(&plain(extra));
                }
            }
            value
        }
        _ => String::new(),
    }
}
