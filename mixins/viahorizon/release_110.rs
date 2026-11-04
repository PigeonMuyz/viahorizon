// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;

use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::Step,
    vfp_release_chain_data::{BLOCK_ENTITY_NAMES_111, ENTITY_NAMES_111, POTION_COLORS_111},
    vfp_release_codec::{self as codec, Stack},
    vfp_release_metadata::{self as meta, Entry, Value},
    vfp_wire::Packet,
};
use horizon_world::nbt::Tag;

#[derive(Default)]
pub struct State {
    mobs: BTreeMap<i32, i32>,
    fishing_hooks: std::collections::BTreeSet<i32>,
}

fn rename(tag: &mut Tag, names: &[(&str, &str)], inverse: bool) {
    let Some(Tag::String(id)) = tag.get_mut("id") else {
        return;
    };
    let value = id.to_string_lossy();
    if let Some(mapped) = names.iter().find_map(|&(old, new)| {
        let (before, after) = if inverse { (new, old) } else { (old, new) };
        (before == value).then_some(after)
    }) {
        *id = mapped.into();
    }
}

pub fn block_entity(tag: &mut Tag) {
    rename(tag, BLOCK_ENTITY_NAMES_111, false);
    if let Some(data) = tag.get_mut("SpawnData") {
        rename(data, ENTITY_NAMES_111, false);
    }
    if let Some(Tag::List(entries)) = tag.get_mut("SpawnPotentials") {
        for entry in entries {
            if let Some(entity) = entry.get_mut("Entity") {
                rename(entity, ENTITY_NAMES_111, false);
            }
        }
    }
}

pub fn stack(stack: &mut Stack, inverse: bool) {
    const COUNT: &str = "viafabricplus:1_10_count";
    const GLINT: &str = "viafabricplus:1_10_glint";
    if stack.id == 383 {
        if let Some(entity) = stack.tag.get_mut("EntityTag") {
            rename(entity, ENTITY_NAMES_111, inverse);
        }
    }
    if inverse {
        if let Tag::Compound(fields) = &mut stack.tag {
            if let Some(Tag::Byte(count)) = fields.remove(&COUNT.into()) {
                stack.count = count;
            }
            if fields.remove(&GLINT.into()).is_some() {
                fields.insert("ench".into(), Tag::List(Vec::new()));
            }
        }
    } else {
        if stack.count <= 0 {
            if matches!(stack.tag, Tag::End) {
                stack.tag = Tag::Compound(Default::default());
            }
            if let Tag::Compound(fields) = &mut stack.tag {
                fields.insert(COUNT.into(), Tag::Byte(stack.count));
                stack.count = 1;
            }
        }
        if let Tag::Compound(fields) = &mut stack.tag {
            if let Some(Tag::List(ench)) = fields.get_mut(&"ench".into()) {
                if ench.is_empty() {
                    ench.push(Tag::Compound(Default::default()));
                    fields.insert(GLINT.into(), Tag::Byte(1));
                }
            }
        }
    }
}

fn mob_type(kind: i32, entries: &mut Vec<Entry>) -> Result<i32> {
    let value = |index| {
        entries
            .iter()
            .find(|entry| entry.index == index)
            .map(Entry::integer)
            .transpose()
            .map(|value| value.unwrap_or(0))
    };
    Ok(match kind {
        68 if value(12)? & 4 != 0 => 4,
        51 => match value(12)? {
            1 => 5,
            2 => 6,
            _ => 51,
        },
        54 => match value(13)? {
            6 => 23,
            profession @ 1..=5 => {
                entries.push(Entry::int(16, 1, profession - 1));
                27
            }
            _ => 54,
        },
        100 => match value(14)? {
            1 => 31,
            2 => 32,
            3 => 29,
            4 => 28,
            _ => 100,
        },
        _ => kind,
    })
}

fn metadata(kind: i32, entries: Vec<Entry>) -> Result<Vec<Entry>> {
    let mut out = BTreeMap::new();
    for mut entry in entries {
        if let Value::Item(Some(item)) = &mut entry.value {
            stack(item, false);
        }
        match kind {
            4 | 68 if entry.index == 12 => {
                entry.value = Value::Bool(entry.integer()? & 2 != 0);
                entry.kind = 6;
            }
            5 | 6 | 51 => {
                if entry.index == 12 {
                    continue;
                }
                if entry.index > 12 {
                    entry.index -= 1;
                }
            }
            23 | 27 | 54 => {
                if entry.index == 14 && kind != 27 {
                    continue;
                }
                if entry.index == 15 {
                    entry.index = 14;
                }
            }
            28 | 29 | 31 | 32 | 100 => match entry.index {
                13 if matches!(kind, 31 | 32) => {
                    out.insert(
                        15,
                        Entry {
                            index: 15,
                            kind: 6,
                            value: Value::Bool(entry.integer()? & 8 != 0),
                        },
                    );
                }
                14 | 17 => continue,
                15 if kind != 100 => continue,
                16 => entry.index = 14,
                _ => (),
            },
            _ => (),
        }
        out.insert(entry.index, entry);
    }
    Ok(out.into_values().collect())
}

fn velocity(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    for scale in [1.33_f64, 1.2, 1.33] {
        w.i16((f64::from(r.i16()?) * scale) as i16);
    }
    Ok(())
}

impl State {
    pub fn reset(&mut self) {
        self.mobs.clear();
        self.fishing_hooks.clear();
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
        match name {
            "LOGIN" | "RESPAWN" => {
                self.reset();
                return Ok(None);
            }
            "ADD_ENTITY" => {
                let id = r.varint()?;
                r.bytes(16)?;
                let object = r.u8()?;
                self.mobs.remove(&id);
                self.fishing_hooks.remove(&id);
                match object {
                    90 => {
                        self.fishing_hooks.insert(id);
                        r.bytes(30)?;
                        w.bytes(r.since(0));
                        velocity(&mut r, &mut w)?;
                    }
                    2 => {
                        w.bytes(r.since(0));
                        w.bytes(r.rest());
                        let mut item = Writer::new();
                        item.varint(id);
                        meta::write(
                            &mut item,
                            &[Entry {
                                index: 6,
                                kind: 5,
                                value: Value::Item(Some(Stack {
                                    id: 1,
                                    count: 1,
                                    damage: 0,
                                    tag: Tag::End,
                                })),
                            }],
                            step.target,
                        )?;
                        after.push(step.incoming("SET_ENTITY_DATA", item.data)?);
                    }
                    _ => return Ok(None),
                }
            }
            "ADD_MOB" => {
                let id = r.varint()?;
                let uuid = r.bytes(16)?;
                let kind = i32::from(r.u8()?);
                let spawn = r.bytes(33)?;
                let mut entries = meta::read(&mut r, step.source)?;
                let kind = mob_type(kind, &mut entries)?;
                self.mobs.insert(id, kind);
                self.fishing_hooks.remove(&id);
                w.varint(id);
                w.bytes(uuid);
                w.varint(kind);
                w.bytes(spawn);
                meta::write(&mut w, &metadata(kind, entries)?, step.target)?;
            }
            "ADD_PLAYER" | "SET_ENTITY_DATA" => {
                let id = r.varint()?;
                if name == "ADD_PLAYER" {
                    self.mobs.remove(&id);
                    self.fishing_hooks.remove(&id);
                    r.bytes(42)?;
                }
                w.bytes(r.since(0));
                let kind = self.mobs.get(&id).copied().unwrap_or(-1);
                let entries = meta::read(&mut r, step.source)?;
                meta::write(&mut w, &metadata(kind, entries)?, step.target)?;
            }
            "REMOVE_ENTITIES" => {
                let count = r.count(65536)?;
                for _ in 0..count {
                    let id = r.varint()?;
                    self.mobs.remove(&id);
                    self.fishing_hooks.remove(&id);
                }
                return Ok(None);
            }
            "SET_ENTITY_MOTION" => {
                let id = r.varint()?;
                if !self.fishing_hooks.contains(&id) {
                    return Ok(None);
                }
                w.varint(id);
                velocity(&mut r, &mut w)?;
            }
            "TAKE_ITEM_ENTITY" => {
                w.varint(r.varint()?);
                w.varint(r.varint()?);
                w.varint(1);
            }
            "BLOCK_ENTITY_DATA" => {
                w.bytes(r.bytes(9)?);
                let mut tag = r.vfp_named_nbt()?;
                block_entity(&mut tag);
                w.vfp_named_nbt(&tag)?;
            }
            "SOUND" => {
                let mut id = r.varint()?;
                if !(0..=4096).contains(&id) {
                    return Err(Error::new("Invalid 1.10 sound ID"));
                }
                if id == 196 {
                    return Ok(Some(Vec::new()));
                }
                for (minimum, shift) in [
                    (85, 2),
                    (176, 1),
                    (197, 8),
                    (207, -1),
                    (279, 9),
                    (296, 1),
                    (390, 4),
                    (400, 3),
                    (450, 1),
                    (455, 1),
                    (470, 1),
                ] {
                    if id >= minimum {
                        id += shift;
                    }
                }
                w.varint(id);
                w.bytes(r.rest());
            }
            "SET_TITLES" => {
                let action = r.count(4)? as i32;
                w.varint(action + i32::from(action >= 2));
                w.bytes(r.rest());
            }
            "LEVEL_EVENT" => {
                let effect = r.i32()?;
                if effect != 2002 {
                    return Ok(None);
                }
                let pos = r.i64()?;
                let value = r.i32()?;
                let (_, color, instant) = POTION_COLORS_111
                    .iter()
                    .find(|&&(old, _, _)| old == value)
                    .copied()
                    .unwrap_or((value, 0, false));
                w.i32(if instant { 2007 } else { effect });
                w.i64(pos);
                w.i32(color);
                w.bool(codec::boolean(&mut r)?);
            }
            _ => return Ok(None),
        }
        codec::end(&r)?;
        let mut output = vec![step.incoming(name, w.data)?];
        output.extend(after);
        Ok(Some(output))
    }
}

pub fn serverbound(step: &Step, name: &str, data: &[u8]) -> Result<Option<Vec<Packet>>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    match name {
        "USE_ITEM_ON" => {
            w.i64(r.i64()?);
            w.varint(r.varint()?);
            w.varint(r.varint()?);
            for _ in 0..3 {
                let coordinate = r.f32()?;
                if !coordinate.is_finite() {
                    return Err(Error::new("Invalid block interaction coordinate"));
                }
                w.u8((coordinate * 16.0) as u8);
            }
        }
        "CHAT" => {
            let chat = r.string(32767)?;
            let mut length = 0;
            let end = chat.char_indices().find_map(|(offset, c)| {
                length += c.len_utf16();
                (length > 100).then_some(offset)
            });
            w.string(end.map_or(chat.as_str(), |end| chat[..end].trim()));
        }
        "SET_CREATIVE_MODE_SLOT" | "CONTAINER_CLICK" => {
            if name == "SET_CREATIVE_MODE_SLOT" {
                w.i16(r.i16()?);
            } else {
                w.bytes(r.bytes(6)?);
                w.varint(r.varint()?);
            }
            let mut item = Stack::read(&mut r, step.target)?;
            if let Some(item) = &mut item {
                if matches!(item.id, 218..=234 | 449 | 450) {
                    return Ok(Some(Vec::new()));
                }
                stack(item, true);
            }
            Stack::write(item.as_ref(), &mut w, step.source)?;
        }
        _ => return Ok(None),
    }
    codec::end(&r)?;
    Ok(Some(vec![step.outgoing(name, w.data)?]))
}
