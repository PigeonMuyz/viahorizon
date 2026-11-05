// SPDX-License-Identifier: GPL-3.0-or-later
// Older protocol patch boundaries. Major schema transitions remain explicit;
// packet names alone never authorize forwarding an unknown play layout.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::Step,
    vfp_release_codec as codec, vfp_release_rewrite as rewrite,
    vfp_wire::Packet,
};

#[derive(Default)]
pub struct State {
    pub username: String,
    keepalive: Option<i64>,
    pub legacy17: crate::vfp_release_17::Legacy,
    dimension: i32,
    columns: crate::vfp_release_columns::Columns,
    pack_hash: Option<String>,
    illagers_111: std::collections::BTreeSet<i32>,
    potions_19: std::collections::BTreeSet<i32>,
    eighteen: crate::vfp_release_18::State,
    ten: crate::vfp_release_110::State,
    flatten: crate::vfp_release_flatten::State,
    flat13: crate::vfp_release_113::State,
    patch_entities: std::collections::BTreeMap<i32, String>,
}
impl State {
    pub fn source(&mut self, protocol: i32) {
        self.flatten.source(protocol);
    }

    pub fn reset_world(&mut self) {
        self.columns.clear();
        self.patch_entities.clear();
        self.flat13.reset();
        self.flatten.reset();
        self.ten.reset();
        self.eighteen.reset_world();
        self.potions_19.clear();
        self.illagers_111.clear();
    }
    pub fn forget(&mut self, data: &[u8]) -> Result<()> {
        let mut r = Reader::new(data);
        self.columns.forget(r.i32()?, r.i32()?);
        codec::end(&r)
    }
    pub fn clientbound(
        &mut self,
        step: &'static Step,
        id: i32,
        data: &[u8],
    ) -> Result<Vec<Packet>> {
        if step.source <= 5 {
            return self.legacy17.clientbound(step, id, data);
        }
        let name = step.incoming_name(id)?;
        if name == "LOGIN" && step.source < 735 {
            let mut join = Reader::new(data);
            join.i32()?;
            join.u8()?;
            self.dimension = if step.source <= 107 {
                i32::from(join.i8()?)
            } else {
                join.i32()?
            };
            self.columns.clear();
        } else if name == "RESPAWN" && step.source < 735 {
            self.dimension = Reader::new(data).i32()?;
            self.columns.clear();
        }
        if name == "FORGET_LEVEL_CHUNK" {
            let mut r = Reader::new(data);
            self.columns.forget(r.i32()?, r.i32()?);
        }
        // A later partial column must include edits received since its full snapshot.
        if step.target == 755 {
            let mut updates = Vec::new();
            let mut r = Reader::new(data);
            match name {
                "BLOCK_UPDATE" => {
                    let position =
                        crate::vfp_release_codec::Position::unpack(r.i64()?, step.source);
                    let state = step.same_or_map("blockstates", r.varint()?, false)?;
                    codec::end(&r)?;
                    updates.push((position, state));
                }
                "SECTION_BLOCKS_UPDATE" => {
                    let key = r.i64()?;
                    codec::boolean(&mut r)?;
                    let sx = (key >> 42) as i32;
                    let sz = ((key << 22) >> 42) as i32;
                    let sy = ((key << 44) >> 44) as i32;
                    for _ in 0..r.count(65536)? {
                        let value = r.varlong()?;
                        if value < 0 {
                            return Err(Error::new("Negative cached section update"));
                        }
                        let state = step.same_or_map(
                            "blockstates",
                            i32::try_from(value >> 12)
                                .map_err(|_| Error::new("Cached state overflow"))?,
                            false,
                        )?;
                        updates.push((
                            crate::vfp_release_codec::Position {
                                x: sx.wrapping_mul(16).wrapping_add(((value >> 8) & 15) as i32),
                                y: sy.wrapping_mul(16).wrapping_add((value & 15) as i32),
                                z: sz.wrapping_mul(16).wrapping_add(((value >> 4) & 15) as i32),
                            },
                            state,
                        ));
                    }
                    codec::end(&r)?;
                }
                "EXPLODE" => {
                    let origin = [r.f32()? as i32, r.f32()? as i32, r.f32()? as i32];
                    r.f32()?;
                    let count = r.i32()?;
                    if !(0..=65536).contains(&count) {
                        return Err(Error::new("Invalid cached explosion size"));
                    }
                    for _ in 0..count {
                        updates.push((
                            crate::vfp_release_codec::Position {
                                x: origin[0].wrapping_add(i32::from(r.i8()?)),
                                y: origin[1].wrapping_add(i32::from(r.i8()?)),
                                z: origin[2].wrapping_add(i32::from(r.i8()?)),
                            },
                            0,
                        ));
                    }
                    r.bytes(12)?;
                    codec::end(&r)?;
                }
                _ => (),
            }
            for (position, state) in updates {
                self.columns.set_block(position, state);
            }
        }
        if name == "LEVEL_CHUNK" {
            let mut column =
                crate::vfp_release_columns::Column::read(step, data, self.dimension == 0)?;
            if step.source == 47 && column.full && column.sections.is_empty() {
                let mut w = Writer::new();
                w.i32(column.x);
                w.i32(column.z);
                return Ok(vec![step.incoming("FORGET_LEVEL_CHUNK", w.data)?]);
            }
            column.remap(step)?;
            let after = if step.source == 340 {
                self.flatten.column(step, &mut column)?
            } else {
                Vec::new()
            };
            if step.source == 109 {
                column.legacy_block_entities();
            }
            if step.source == 210 {
                for entity in &mut column.entities {
                    crate::vfp_release_110::block_entity(entity);
                }
            }
            if step.source == 316 {
                use horizon_world::nbt::Tag;
                for (&section, data) in &column.sections {
                    for (index, state) in data.blocks.iter().enumerate() {
                        if state >> 4 != 26 {
                            continue;
                        }
                        let x = column.x.wrapping_mul(16).wrapping_add((index & 15) as i32);
                        let y = (section * 16 + (index >> 8)) as i32;
                        let z = column
                            .z
                            .wrapping_mul(16)
                            .wrapping_add(((index >> 4) & 15) as i32);
                        column.entities.push(Tag::compound([
                            ("id", Tag::String("minecraft:bed".into())),
                            ("x", Tag::Int(x)),
                            ("y", Tag::Int(y)),
                            ("z", Tag::Int(z)),
                            ("color", Tag::Int(14)),
                        ]));
                    }
                }
            }
            let mut before = Vec::new();
            if step.target == 477 {
                let motion = crate::vfp_release_chain_resources::MOTION_BLOCKING_114
                    .iter()
                    .copied()
                    .collect();
                column.make_heightmap(&motion, 477)?;
                before.extend(self.flat13.chunk_center(step, column.x, column.z)?);
                before.push(column.light(step, self.dimension == 0)?);
            }
            if step.target == 755 {
                column = self.columns.merge(column)?;
            }
            before.push(step.incoming(name, column.write(step, self.dimension == 0)?)?);
            before.extend(after);
            return Ok(before);
        }
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        match step.source {
            // No payload changes in these adjacent Release protocol families.
            108 | 480 | 573 | 575 | 735 | 751 | 753 => {
                w.bytes(data);
            }
            47 => {
                if let Some(packets) = self.eighteen.clientbound(step, name, data)? {
                    return Ok(packets);
                }
                w.bytes(data);
            }
            477 => match name {
                "LOGIN" | "RESPAWN" => {
                    self.patch_entities.clear();
                    w.bytes(data);
                }
                "ADD_MOB" => {
                    let entity = r.varint()?;
                    w.varint(entity);
                    w.bytes(r.bytes(16)?);
                    let kind = r.varint()?;
                    w.varint(kind);
                    let kind = crate::vfp_release_chain::identifier(step.source, "entities", kind)?;
                    self.patch_entities.insert(entity, kind.into());
                    w.bytes(r.bytes(24 + 3 + 6)?);
                    let mut metadata = Writer::new();
                    metadata.varint(entity);
                    metadata.bytes(r.rest());
                    let converted = rewrite::metadata_119(step, &metadata.data, kind)?;
                    let mut converted = Reader::new(&converted);
                    converted.varint()?;
                    w.bytes(converted.rest());
                }
                "SET_ENTITY_DATA" => {
                    let entity = Reader::new(data).varint()?;
                    let kind = self.patch_entities.get(&entity).map_or("", String::as_str);
                    w.bytes(&rewrite::metadata_119(step, data, kind)?);
                }
                "REMOVE_ENTITIES" => {
                    let count = r.count(65536)?;
                    for _ in 0..count {
                        self.patch_entities.remove(&r.varint()?);
                    }
                    codec::end(&r)?;
                    w.bytes(data);
                }
                _ => {
                    w.bytes(data);
                }
            },
            107 => match name {
                "LOGIN" => {
                    w.i32(r.i32()?);
                    w.u8(r.u8()?);
                    w.i32(i32::from(r.i8()?));
                    w.bytes(r.rest());
                }
                "SOUND" => {
                    let id = r.varint()?;
                    w.varint(if id >= 415 {
                        id.checked_add(1)
                            .ok_or_else(|| Error::new("Old sound ID overflow"))?
                    } else {
                        id
                    });
                    w.bytes(r.rest());
                }
                _ => {
                    w.bytes(data);
                }
            },
            109 => match name {
                "UPDATE_SIGN" => {
                    let position = codec::Position::unpack(r.i64()?, step.source);
                    w.i64(position.pack(step.target)?);
                    w.u8(9);
                    let mut fields = std::collections::BTreeMap::new();
                    fields.insert("id".into(), horizon_world::nbt::Tag::String("Sign".into()));
                    for (key, value) in [("x", position.x), ("y", position.y), ("z", position.z)] {
                        fields.insert(key.into(), horizon_world::nbt::Tag::Int(value));
                    }
                    for line in 1..=4 {
                        fields.insert(
                            format!("Text{line}").as_str().into(),
                            horizon_world::nbt::Tag::String(r.string(262144)?.as_str().into()),
                        );
                    }
                    w.vfp_named_nbt(&horizon_world::nbt::Tag::Compound(fields))?;
                    codec::end(&r)?;
                    return Ok(vec![step.incoming("BLOCK_ENTITY_DATA", w.data)?]);
                }
                "SOUND" => {
                    w.varint(r.varint()?);
                    w.varint(r.varint()?);
                    w.bytes(r.bytes(16)?);
                    w.u8((f32::from(r.u8()?) / 63.5 * 63.0).round() as u8);
                    codec::end(&r)?;
                }
                _ => {
                    w.bytes(data);
                }
            },
            110 => match name {
                "LOGIN" | "RESPAWN" => {
                    self.potions_19.clear();
                    w.bytes(data);
                }
                "SOUND" | "CUSTOM_SOUND" => {
                    if name == "SOUND" {
                        let sound = r.varint()?;
                        if !(0..=4096).contains(&sound) {
                            return Err(Error::new("Invalid 1.9 sound ID"));
                        }
                        let shifts = [(24, 1), (248, 4), (296, 6), (354, 4), (372, 4)];
                        w.varint(
                            sound
                                + shifts
                                    .into_iter()
                                    .filter(|&(start, _)| sound >= start)
                                    .map(|(_, offset)| offset)
                                    .sum::<i32>(),
                        );
                    } else {
                        w.string(&r.string(32767)?);
                    }
                    w.varint(r.varint()?);
                    w.bytes(r.bytes(16)?);
                    w.f32(f32::from(r.u8()?) / 63.0);
                    codec::end(&r)?;
                }
                "RESOURCE_PACK" => {
                    r.string(32767)?;
                    self.pack_hash = Some(r.string(40)?);
                    codec::end(&r)?;
                    w.bytes(data);
                }
                "ADD_ENTITY" => {
                    let id = r.varint()?;
                    r.bytes(16)?;
                    if r.u8()? == 73 {
                        self.potions_19.insert(id);
                    } else {
                        self.potions_19.remove(&id);
                    }
                    w.bytes(data);
                }
                "ADD_MOB" | "ADD_PLAYER" | "SET_ENTITY_DATA" => {
                    let start = r.position();
                    let id = r.varint()?;
                    if name != "SET_ENTITY_DATA" {
                        self.potions_19.remove(&id);
                        r.bytes(16 + if name == "ADD_MOB" { 34 } else { 26 })?;
                    }
                    w.bytes(r.since(start));
                    let mut entries = crate::vfp_release_metadata::read(&mut r, step.source)?;
                    let potion = self.potions_19.contains(&id);
                    entries.retain_mut(|entry| {
                        if entry.index == 5 && potion {
                            return false;
                        }
                        if entry.index >= 5 && !potion {
                            entry.index += 1;
                        }
                        true
                    });
                    crate::vfp_release_metadata::write(&mut w, &entries, step.target)?;
                    codec::end(&r)?;
                }
                "REMOVE_ENTITIES" => {
                    let count = r.count(65536)?;
                    for _ in 0..count {
                        self.potions_19.remove(&r.varint()?);
                    }
                    codec::end(&r)?;
                    w.bytes(data);
                }
                _ => w.bytes(data),
            },
            210 => {
                if let Some(packets) = self.ten.clientbound(step, name, data)? {
                    return Ok(packets);
                }
                return self.inventory_and_items(step, name, data);
            }
            315 => match name {
                "AWARD_STATS" => {
                    let count = r.count(65536)?;
                    let mut values = Vec::new();
                    for _ in 0..count {
                        let name = r.string(32767)?;
                        let value = r.varint()?;
                        if name != "stat.treasureFished" && name != "stat.junkFished" {
                            values.push((name, value));
                        }
                    }
                    codec::end(&r)?;
                    w.varint(values.len() as i32);
                    for (name, value) in values {
                        w.string(&name);
                        w.varint(value);
                    }
                }
                _ => {
                    w.bytes(data);
                }
            },
            316 => match name {
                "LOGIN" | "RESPAWN" => {
                    self.illagers_111.clear();
                    w.bytes(data);
                }
                "SOUND" => {
                    let id = r.varint()?;
                    if !(0..=4096).contains(&id) {
                        return Err(Error::new("Invalid 1.11 sound ID"));
                    }
                    let shifts = [
                        (26, 2),
                        (70, 4),
                        (74, 1),
                        (143, 3),
                        (185, 1),
                        (263, 7),
                        (301, 33),
                        (317, 2),
                        (491, 3),
                    ];
                    w.varint(
                        id + shifts
                            .into_iter()
                            .filter(|&(start, _)| id >= start)
                            .map(|(_, offset)| offset)
                            .sum::<i32>(),
                    );
                    w.bytes(r.rest());
                }
                "ADD_MOB" | "ADD_PLAYER" | "SET_ENTITY_DATA" => {
                    use crate::vfp_release_metadata::{self as meta, Value};
                    let id = r.varint()?;
                    if name != "SET_ENTITY_DATA" {
                        self.illagers_111.remove(&id);
                        r.bytes(16)?;
                        if name == "ADD_MOB" {
                            if matches!(r.varint()?, 34 | 36 | 37) {
                                self.illagers_111.insert(id);
                            }
                            r.bytes(33)?;
                        } else {
                            r.bytes(26)?;
                        }
                    }
                    w.bytes(r.since(0));
                    let mut entries = meta::read(&mut r, step.source)?;
                    let illager = self.illagers_111.contains(&id);
                    for entry in &mut entries {
                        if illager && entry.index >= 12 {
                            entry.index += 1;
                        }
                        if let Value::Item(Some(stack)) = &mut entry.value {
                            if stack.id == 355 {
                                stack.damage = 14;
                            }
                        }
                    }
                    meta::write(&mut w, &entries, step.target)?;
                    codec::end(&r)?;
                }
                "REMOVE_ENTITIES" => {
                    let count = r.count(65536)?;
                    for _ in 0..count {
                        self.illagers_111.remove(&r.varint()?);
                    }
                    codec::end(&r)?;
                    w.bytes(data);
                }
                _ => return self.inventory_and_items(step, name, data),
            },
            335 => {
                w.bytes(data);
            }
            338 => {
                if name == "KEEP_ALIVE" {
                    let value = r.varint()?;
                    codec::end(&r)?;
                    self.keepalive = Some(i64::from(value));
                    w.i64(i64::from(value));
                } else {
                    w.bytes(data);
                }
            }
            340 => {
                if let Some(packets) = self.flatten.clientbound(step, name, data)? {
                    return Ok(packets);
                }
                return self.inventory_and_items(step, name, data);
            }
            393 | 404 => {
                if let Some(packets) = self.flat13.clientbound(step, name, data)? {
                    return Ok(packets);
                }
                return self.inventory_and_items(step, name, data);
            }
            401 => return self.inventory_and_items(step, name, data),
            485 | 490 => {
                if name == "MERCHANT_OFFERS" {
                    w.varint(r.varint()?);
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
                        w.bytes(r.bytes(1 + 4 * 5)?);
                        if step.source == 490 {
                            w.i32(0);
                        }
                    }
                    w.varint(r.varint()?);
                    w.varint(r.varint()?);
                    let villager = codec::boolean(&mut r)?;
                    w.bool(villager);
                    if step.source == 485 {
                        w.bool(villager);
                    } else {
                        w.bool(codec::boolean(&mut r)?);
                    }
                    codec::end(&r)?;
                } else {
                    w.bytes(data);
                }
            }
            // These edges require the major world/entity/inventory adapters.
            // Keep their admission closed instead of interpreting source IDs
            // using a later protocol's tables.
            _ => {
                return Err(Error::new(format!(
                    "Source-only {} -> {} adapter still needs {name}",
                    step.source, step.target
                )));
            }
        }
        Ok(vec![step.incoming(name, w.data)?])
    }
    fn inventory_and_items(
        &mut self,
        step: &'static Step,
        name: &str,
        data: &[u8],
    ) -> Result<Vec<Packet>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        match name {
            "CONTAINER_SET_SLOT" => {
                w.u8(r.u8()?);
                w.i16(r.i16()?);
                rewrite::item(step, false, &mut r, &mut w)?;
                codec::end(&r)?;
            }
            "CONTAINER_SET_CONTENT" => {
                w.u8(r.u8()?);
                let count = r.i16()?;
                if !(0..=4096).contains(&count) {
                    return Err(Error::new("Invalid old inventory length"));
                }
                w.i16(count);
                for _ in 0..count {
                    rewrite::item(step, false, &mut r, &mut w)?;
                }
                codec::end(&r)?;
            }
            "SET_EQUIPPED_ITEM" => {
                w.varint(r.varint()?);
                w.varint(r.varint()?);
                rewrite::item(step, false, &mut r, &mut w)?;
                codec::end(&r)?;
            }
            "SET_ENTITY_DATA" => {
                w.bytes(&rewrite::metadata_119(step, data, "")?);
            }
            "ADD_MOB" | "ADD_PLAYER" => {
                let entity = r.varint()?;
                w.varint(entity);
                w.bytes(r.bytes(16)?);
                if name == "ADD_MOB" {
                    w.varint(r.varint()?);
                }
                w.bytes(r.bytes(if name == "ADD_MOB" {
                    24 + 3 + 6
                } else {
                    24 + 2
                })?);
                let mut metadata = Writer::new();
                metadata.varint(entity);
                metadata.bytes(r.rest());
                let converted = rewrite::metadata_119(step, &metadata.data, "")?;
                let mut converted = Reader::new(&converted);
                converted.varint()?;
                w.bytes(converted.rest());
            }
            "LEVEL_PARTICLES" if step.source >= 393 => {
                rewrite::particle(step, &mut r, &mut w, true)?;
                codec::end(&r)?;
            }
            "CUSTOM_PAYLOAD" => {
                let channel = r.string(32767)?;
                w.string(&channel);
                if channel == "minecraft:trader_list"
                    || (matches!(step.source, 210 | 316) && channel == "MC|TrList")
                {
                    w.i32(r.i32()?);
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
                    }
                    codec::end(&r)?;
                } else {
                    w.bytes(r.rest());
                }
            }
            "UPDATE_ADVANCEMENTS" if step.source == 401 => {
                w.bytes(&rewrite::advancements(step, data)?);
            }
            "UPDATE_RECIPES" => {
                w.bytes(&crate::vfp_release_113::recipes(step, data)?);
            }
            _ => {
                w.bytes(data);
            }
        }
        Ok(vec![step.incoming(name, w.data)?])
    }
    pub fn serverbound(
        &mut self,
        step: &'static Step,
        packets: Vec<Packet>,
        held: Option<&Option<codec::Stack>>,
    ) -> Result<Vec<Packet>> {
        if step.source <= 5 {
            return self.legacy17.serverbound(step, packets);
        }
        let mut result = Vec::new();
        for (id, data) in packets {
            let name = step.outgoing_name(id)?;
            let mut r = Reader::new(&data);
            let mut w = Writer::new();
            match step.source {
                107 | 108 | 109 | 315 | 477 | 480 | 485 | 490 | 573 | 575 | 735 | 753 => {
                    w.bytes(&data);
                }
                47 => {
                    if let Some(packets) = self.eighteen.serverbound(step, name, &data, held)? {
                        result.extend(packets);
                        continue;
                    }
                    w.bytes(&data);
                }
                110 => match name {
                    "RESOURCE_PACK" => {
                        let action = r.varint()?;
                        codec::end(&r)?;
                        let Some(hash) = &self.pack_hash else {
                            continue;
                        };
                        w.string(hash);
                        w.varint(action);
                    }
                    "SET_CREATIVE_MODE_SLOT" => {
                        r.i16()?;
                        let stack = codec::Stack::read(&mut r, step.target)?;
                        codec::end(&r)?;
                        if stack.is_some_and(|stack| (213..=217).contains(&stack.id)) {
                            continue;
                        }
                        w.bytes(&data);
                    }
                    _ => w.bytes(&data),
                },
                210 => {
                    if let Some(packets) = crate::vfp_release_110::serverbound(step, name, &data)? {
                        result.extend(packets);
                        continue;
                    }
                    w.bytes(&data);
                }
                316 => match name {
                    "CRAFTING_RECIPE_PLACEMENT" | "RECIPE_BOOK_UPDATE" | "SEEN_ADVANCEMENTS" => {
                        continue;
                    }
                    "CLIENT_INFORMATION" => {
                        let locale = r.string(16)?;
                        let mut length = 0;
                        let end = locale
                            .char_indices()
                            .find_map(|(offset, ch)| {
                                length += ch.len_utf16();
                                (length > 7).then_some(offset)
                            })
                            .unwrap_or(locale.len());
                        w.string(&locale[..end]);
                        w.bytes(r.rest());
                    }
                    "SET_CREATIVE_MODE_SLOT" | "CONTAINER_CLICK" => {
                        if name == "SET_CREATIVE_MODE_SLOT" {
                            w.i16(r.i16()?);
                        } else {
                            w.bytes(r.bytes(6)?);
                            w.varint(r.varint()?);
                        }
                        let mut stack = codec::Stack::read(&mut r, step.target)?;
                        codec::end(&r)?;
                        if let Some(item) = &mut stack {
                            if matches!(item.id, 235..=252 | 453) {
                                continue;
                            }
                            if item.id == 355 {
                                item.damage = 0;
                            }
                        }
                        codec::Stack::write(stack.as_ref(), &mut w, step.source)?;
                    }
                    _ => w.bytes(&data),
                },
                335 => {
                    if name == "PLACE_RECIPE" {
                        continue;
                    }
                    w.bytes(&data);
                }
                338 => {
                    if name == "KEEP_ALIVE" {
                        let value = r.i64()?;
                        codec::end(&r)?;
                        if self.keepalive != Some(value) {
                            return Err(Error::new("Mismatched legacy keepalive reply"));
                        }
                        self.keepalive = None;
                        w.varint(
                            i32::try_from(value)
                                .map_err(|_| Error::new("Old keepalive key exceeds VarInt"))?,
                        );
                    } else {
                        w.bytes(&data);
                    }
                }
                340 => {
                    if let Some(packets) = self.flatten.serverbound(step, name, &data)? {
                        result.extend(packets);
                        continue;
                    }
                    w.bytes(&data);
                }
                393 | 404 => {
                    if let Some(packets) = self.flat13.serverbound(step, name, &data)? {
                        result.extend(packets);
                        continue;
                    }
                    w.bytes(&data);
                }
                401 => match name {
                    "CONTAINER_CLICK" => {
                        w.u8(r.u8()?);
                        w.i16(r.i16()?);
                        w.i8(r.i8()?);
                        w.i16(r.i16()?);
                        w.varint(r.varint()?);
                        rewrite::item(step, true, &mut r, &mut w)?;
                        codec::end(&r)?;
                    }
                    "SET_CREATIVE_MODE_SLOT" => {
                        w.i16(r.i16()?);
                        rewrite::item(step, true, &mut r, &mut w)?;
                        codec::end(&r)?;
                    }
                    "EDIT_BOOK" => {
                        rewrite::item(step, true, &mut r, &mut w)?;
                        w.bytes(r.rest());
                    }
                    _ => {
                        w.bytes(&data);
                    }
                },
                751 => {
                    if name == "EDIT_BOOK" {
                        let item = codec::Stack::read(&mut r, step.target)?;
                        codec::Stack::write(item.as_ref(), &mut w, step.source)?;
                        w.bool(codec::boolean(&mut r)?);
                        let slot = r.varint()?;
                        w.varint(if slot == 40 { 1 } else { 0 });
                        codec::end(&r)?;
                    } else {
                        w.bytes(&data);
                    }
                }
                _ => {
                    return Err(Error::new(format!(
                        "Source-only {} -> {} adapter still needs outgoing {name}",
                        step.source, step.target
                    )));
                }
            }
            result.push(step.outgoing(name, w.data)?);
        }
        Ok(result)
    }
}
