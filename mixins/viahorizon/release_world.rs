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
        let key = (r.varint()?, r.varint()?);
        let incoming = decode_light(r.rest())?;
        let old = self.columns.get(&key);
        let mut merged = old
            .map(|v| decode_light(v))
            .transpose()?
            .unwrap_or_default();
        merged.0 = incoming.0;
        for (previous, update) in merged.1.iter_mut().zip(incoming.1) {
            previous.extend(update);
        }
        let bytes = encode_light(&merged);
        let previous = old.map_or(0, Vec::len);
        let new_size = self.bytes - previous + bytes.len();
        if new_size > 64 * 1024 * 1024 || (self.columns.len() >= 4096 && old.is_none()) {
            return Err(Error::new("Legacy light cache exceeds connection bound"));
        }
        self.bytes = new_size;
        self.columns.insert(key, bytes);
        Ok(self.loaded.contains(&key))
    }
    fn append(&mut self, x: i32, z: i32, sections: usize, w: &mut Writer) -> Result<()> {
        if self.loaded.len() >= 65536 && !self.loaded.contains(&(x, z)) {
            return Err(Error::new("Too many loaded legacy columns"));
        }
        self.loaded.insert((x, z));
        if let Some(data) = self.columns.get(&(x, z)) {
            w.bytes(data);
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
type LightSnapshot = (bool, [BTreeMap<usize, Option<Vec<u8>>>; 2]);
fn decode_light(data: &[u8]) -> Result<LightSnapshot> {
    let mut r = Reader::new(data);
    let trust = codec::boolean(&mut r)?;
    let mut masks = Vec::new();
    for _ in 0..4 {
        let mask = r.long_array()?;
        if mask.len() > 5 {
            return Err(Error::new("Legacy light mask exceeds dimension bound"));
        }
        masks.push(mask);
    }
    let bits = |mask: &[i64]| -> Vec<usize> {
        mask.iter()
            .enumerate()
            .flat_map(|(word, value)| {
                (0..64)
                    .filter(move |bit| (*value as u64) & (1u64 << bit) != 0)
                    .map(move |bit| word * 64 + bit)
            })
            .collect()
    };
    let mut layers: [BTreeMap<usize, Option<Vec<u8>>>; 2] = Default::default();
    for kind in 0..2 {
        for index in bits(&masks[kind + 2]) {
            layers[kind].insert(index, None);
        }
        let present = bits(&masks[kind]);
        if r.count(258)? != present.len() {
            return Err(Error::new("Legacy light arrays disagree with mask"));
        }
        for index in present {
            if layers[kind].contains_key(&index) {
                return Err(Error::new("Overlapping legacy light masks"));
            }
            let value = r.byte_array(2048)?;
            if value.len() != 2048 {
                return Err(Error::new("Invalid legacy light layer size"));
            }
            layers[kind].insert(index, Some(value));
        }
    }
    codec::end(&r)?;
    Ok((trust, layers))
}
fn encode_light(snapshot: &LightSnapshot) -> Vec<u8> {
    let mut w = Writer::new();
    w.bool(snapshot.0);
    let mut masks = [vec![0i64; 5], vec![0; 5], vec![0; 5], vec![0; 5]];
    for (kind, layers) in snapshot.1.iter().enumerate() {
        for (index, value) in layers {
            masks[if value.is_some() { kind } else { kind + 2 }][index / 64] |=
                (1u64 << (index % 64)) as i64;
        }
    }
    for mask in &mut masks {
        while mask.last() == Some(&0) {
            mask.pop();
        }
        w.long_array(mask);
    }
    for layers in &snapshot.1 {
        w.varint(layers.values().filter(|v| v.is_some()).count() as i32);
        for value in layers.values().flatten() {
            w.byte_array(value);
        }
    }
    w.data
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

#[cfg(test)]
mod light_regressions {
    use super::*;
    fn update(cache: &mut LightCache, snapshot: &LightSnapshot) -> bool {
        let mut w = Writer::new();
        w.varint(-2);
        w.varint(3);
        w.bytes(&encode_light(snapshot));
        cache.update(&w.data).unwrap()
    }
    fn appended(cache: &mut LightCache) -> LightSnapshot {
        let mut w = Writer::new();
        cache.append(-2, 3, 16, &mut w).unwrap();
        decode_light(&w.data).unwrap()
    }
    #[test]
    fn partial_chunk_replacement_preserves_latest_sky_and_block_light() {
        let mut cache = LightCache::default();
        let initial = (
            true,
            [
                BTreeMap::from([(1, Some(vec![255; 2048])), (2, Some(vec![238; 2048]))]),
                BTreeMap::from([(1, Some(vec![17; 2048]))]),
            ],
        );
        assert!(!update(&mut cache, &initial));
        assert_eq!(appended(&mut cache), initial);
        let partial = (
            false,
            [
                BTreeMap::from([(2, None)]),
                BTreeMap::from([(1, Some(vec![34; 2048]))]),
            ],
        );
        assert!(update(&mut cache, &partial));
        let expected = (
            false,
            [
                BTreeMap::from([(1, Some(vec![255; 2048])), (2, None)]),
                BTreeMap::from([(1, Some(vec![34; 2048]))]),
            ],
        );
        assert_eq!(appended(&mut cache), expected);
        assert_eq!(appended(&mut cache), expected);
        cache.forget(-2, 3);
        assert_eq!(cache.bytes, 0);
        assert!(
            appended(&mut cache)
                .1
                .iter()
                .all(|layer| layer.values().all(Option::is_none))
        );
        cache.clear();
        assert!(cache.loaded.is_empty());
    }
    #[test]
    fn malformed_light_does_not_replace_retained_snapshot() {
        let snapshot = (
            true,
            [
                BTreeMap::from([(1, Some(vec![255; 2048]))]),
                BTreeMap::new(),
            ],
        );
        let mut cache = LightCache::default();
        update(&mut cache, &snapshot);
        let mut w = Writer::new();
        w.varint(-2);
        w.varint(3);
        w.bytes(&encode_light(&snapshot));
        w.data.pop();
        assert!(cache.update(&w.data).is_err());
        assert_eq!(appended(&mut cache), snapshot);
    }
}
