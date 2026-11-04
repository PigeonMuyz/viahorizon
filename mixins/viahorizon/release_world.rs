// SPDX-License-Identifier: GPL-3.0-or-later
// The 1.17 -> 1.18 chunk/light boundary is connection-local. Light can precede
// a column; later updates remain ordinary light packets for loaded columns.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::{self as chain, Step},
    vfp_release_codec::{self as codec, PaletteFormat},
};
use horizon_world::nbt::Tag;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct LightCache {
    columns: BTreeMap<(i32, i32), Vec<u8>>,
    loaded: BTreeSet<(i32, i32)>,
    bytes: usize,
}
impl LightCache {
    pub fn clear(&mut self) {
        self.columns.clear();
        self.loaded.clear();
        self.bytes = 0;
    }
    pub fn forget(&mut self, x: i32, z: i32) {
        if let Some(data) = self.columns.remove(&(x, z)) {
            self.bytes -= data.len();
        }
        self.loaded.remove(&(x, z));
    }
    pub fn update(&mut self, data: &[u8]) -> Result<bool> {
        let mut r = Reader::new(data);
        let x = r.varint()?;
        let z = r.varint()?;
        let start = r.position();
        codec::boolean(&mut r)?;
        let mut masks = Vec::new();
        for _ in 0..4 {
            let mask = r.long_array()?;
            if mask.len() > 5 {
                return Err(Error::new("Legacy light mask exceeds dimension bound"));
            }
            masks.push(mask);
        }
        for mask in &masks[..2] {
            let expected: usize = mask.iter().map(|word| word.count_ones() as usize).sum();
            let count = r.count(258)?;
            if count != expected {
                return Err(Error::new("Legacy light arrays disagree with mask"));
            }
            for _ in 0..count {
                let layer = r.byte_array(2048)?;
                if layer.len() != 2048 {
                    return Err(Error::new("Invalid legacy light layer size"));
                }
            }
        }
        codec::end(&r)?;
        if self.loaded.contains(&(x, z)) {
            return Ok(true);
        }
        let bytes = data[start..].to_vec();
        let previous = self.columns.get(&(x, z)).map_or(0, Vec::len);
        let new_size = self.bytes - previous + bytes.len();
        if new_size > 64 * 1024 * 1024 || (self.columns.len() >= 4096 && previous == 0) {
            return Err(Error::new("Legacy light cache exceeds connection bound"));
        }
        self.bytes = new_size;
        self.columns.insert((x, z), bytes);
        Ok(false)
    }
    fn append(&mut self, x: i32, z: i32, sections: usize, w: &mut Writer) -> Result<()> {
        if self.loaded.len() >= 65536 && !self.loaded.contains(&(x, z)) {
            return Err(Error::new("Too many loaded legacy columns"));
        }
        self.loaded.insert((x, z));
        if let Some(data) = self.columns.remove(&(x, z)) {
            self.bytes -= data.len();
            w.bytes(&data);
        } else {
            w.bool(false);
            w.varint(0);
            w.varint(0);
            let mut empty = vec![-1i64; (sections + 2).div_ceil(64)];
            if (sections + 2) % 64 != 0 {
                *empty.last_mut().unwrap() = ((1u64 << ((sections + 2) % 64)) - 1) as i64;
            }
            w.long_array(&empty);
            w.long_array(&empty);
            w.varint(0);
            w.varint(0);
        }
        Ok(())
    }
}
fn spawner(kind: i32, tag: &mut Tag) {
    if kind == 8 {
        if let Tag::Compound(fields) = tag {
            if let Some(value) = fields.remove(&"SpawnData".into()) {
                fields.insert("SpawnData".into(), Tag::compound([("entity", value)]));
            }
        }
    }
}
pub fn block_entity_117(data: &[u8]) -> Result<Vec<u8>> {
    const IDS: &[i32] = &[-1, 8, 21, 13, 14, 24, 18, 19, 20, 7, 22, 23, 30, 31];
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    w.i64(r.i64()?);
    let old = r.u8()?;
    let kind = *IDS
        .get(old as usize)
        .filter(|&&id| id >= 0)
        .ok_or_else(|| Error::new("Unknown old block entity action"))?;
    w.varint(kind);
    let mut tag = r.vfp_named_nbt()?;
    spawner(kind, &mut tag);
    w.vfp_named_nbt(&tag)?;
    codec::end(&r)?;
    Ok(w.data)
}
pub fn light_1164(data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    w.varint(r.varint()?);
    w.varint(r.varint()?);
    w.bool(codec::boolean(&mut r)?);
    let mut masks = Vec::new();
    for _ in 0..4 {
        let mask = r.varint()?;
        if mask < 0 || mask >> 18 != 0 {
            return Err(Error::new("Old light mask exceeds 16-section world"));
        }
        w.long_array(&if mask == 0 {
            Vec::new()
        } else {
            vec![i64::from(mask)]
        });
        masks.push(mask);
    }
    for mask in &masks[..2] {
        let count = mask.count_ones();
        w.varint(count as i32);
        for _ in 0..count {
            let layer = r.byte_array(2048)?;
            if layer.len() != 2048 {
                return Err(Error::new("Invalid old light layer"));
            }
            w.byte_array(&layer);
        }
    }
    codec::end(&r)?;
    Ok(w.data)
}
pub fn chunk_117(
    step: &Step,
    data: &[u8],
    sections: usize,
    biome_bits: u8,
    light: &mut LightCache,
) -> Result<Vec<u8>> {
    if !(1..=256).contains(&sections) {
        return Err(Error::new("1.17 column arrived before dimension height"));
    }
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    let x = r.i32()?;
    let z = r.i32()?;
    w.i32(x);
    w.i32(z);
    let mask = r.long_array()?;
    if mask.len() > sections.div_ceil(64)
        || (sections % 64 != 0
            && mask.len() == sections.div_ceil(64)
            && mask
                .last()
                .is_some_and(|word| (*word as u64) >> (sections % 64) != 0))
    {
        return Err(Error::new("1.17 section mask exceeds dimension height"));
    }
    w.vfp_named_nbt(&r.vfp_named_nbt()?)?;
    let count = r.count(sections * 64)?;
    if count != sections * 64 {
        return Err(Error::new(
            "1.17 biome array does not match dimension height",
        ));
    }
    let mut biomes = Vec::with_capacity(count);
    for _ in 0..count {
        biomes.push(r.varint()?.max(0));
    }
    let raw = r.byte_array(crate::vfp_core::MAX_PACKET)?;
    let mut input = Reader::new(&raw);
    let mut body = Writer::new();
    let map = step
        .registry("blockstates")
        .ok_or_else(|| Error::new("Missing 1.17 block state map"))?;
    let source_bits = (32 - (map.size - 1).leading_zeros()) as u8;
    let target_bits = (32 - (map.mapped_size - 1).leading_zeros()) as u8;
    for section in 0..sections {
        let present = mask
            .get(section / 64)
            .is_some_and(|word| (*word as u64) & (1 << (section % 64)) != 0);
        let (count, mut blocks) = if present {
            let count = input.i16()?;
            (
                count,
                PaletteFormat::blocks(step.source, source_bits).read(&mut input)?,
            )
        } else {
            (0, vec![0; 4096])
        };
        for value in &mut blocks {
            *value = map.map(*value).map_err(Error::new)?;
        }
        body.i16(count);
        PaletteFormat::blocks(step.target, target_bits).write(&mut body, &blocks)?;
        PaletteFormat::biomes(biome_bits)
            .write(&mut body, &biomes[section * 64..section * 64 + 64])?;
    }
    codec::end(&input)?;
    w.byte_array(&body.data);
    let entities = r.count(65536)?;
    let mut converted = Vec::new();
    for _ in 0..entities {
        let mut tag = r.vfp_named_nbt()?;
        let (Some(Tag::Int(x)), Some(Tag::Int(y)), Some(Tag::Int(z)), Some(Tag::String(id))) =
            (tag.get("x"), tag.get("y"), tag.get("z"), tag.get("id"))
        else {
            return Err(Error::new(
                "1.17 block entity is missing coordinates or identifier",
            ));
        };
        let packed = (((x & 15) << 4) | (z & 15)) as u8;
        let y = i16::try_from(*y).map_err(|_| Error::new("Old block entity Y exceeds short"))?;
        let name = id.to_string_lossy();
        let kind = chain::named_id(
            step.target,
            "blockentities",
            name.strip_prefix("minecraft:").unwrap_or(&name),
        )?;
        spawner(kind, &mut tag);
        converted.push((packed, y, kind, tag));
    }
    codec::end(&r)?;
    w.varint(converted.len() as i32);
    for (xz, y, kind, tag) in converted {
        w.u8(xz);
        w.i16(y);
        w.varint(kind);
        w.vfp_named_nbt(&tag)?;
    }
    light.append(x, z, sections, &mut w)?;
    Ok(w.data)
}
