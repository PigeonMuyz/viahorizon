use crate::vfp_mapping::{Maps, V262};
// SPDX-License-Identifier: GPL-3.0-or-later
// Native 26.2 -> 26.3 wire conversions. Unsupported layouts return an error;
// this module does not promote a release to playable until the full gate passes.
use crate::{
    vfp_core::{Cursor, MAX_PACKET, Result, put_varint},
    vfp_tables as t,
};

#[derive(Clone, Copy, Debug)]
pub struct IdMap {
    pub size: i32,
    pub mapped_size: i32,
    pub runs: &'static [(i32, i32)],
}
impl IdMap {
    pub fn map(&self, id: i32) -> Result<i32> {
        if id < 0 || id >= self.size {
            return Err(format!("Registry ID {id} outside 0..{}", self.size));
        }
        let n = self.runs.partition_point(|&(at, _)| at <= id);
        let &(at, to) = self
            .runs
            .get(n.wrapping_sub(1))
            .ok_or("Missing mapping run")?;
        if to < 0 {
            return Err(format!("Registry ID {id} requires a structural conversion"));
        }
        Ok(to + id - at)
    }
    pub fn inverse(&self, id: i32) -> Result<i32> {
        if id < 0 || id >= self.mapped_size {
            return Err("Mapped registry ID out of range".into());
        }
        for (index, &(at, to)) in self.runs.iter().enumerate() {
            let end = self.runs.get(index + 1).map_or(self.size, |r| r.0);
            if to >= 0 && id >= to && id - to < end - at {
                return Ok(at + id - to);
            }
        }
        Err(format!(
            "New registry ID {id} has no exact old representation"
        ))
    }
}
pub type Packet = (i32, Vec<u8>);
pub fn packet_id(names: &[&str], name: &str) -> Result<i32> {
    names
        .iter()
        .position(|&n| n == name)
        .map(|n| n as i32)
        .ok_or_else(|| format!("No packet {name}"))
}
fn name(names: &'static [&'static str], id: i32) -> Result<&'static str> {
    usize::try_from(id)
        .ok()
        .and_then(|i| names.get(i))
        .copied()
        .ok_or_else(|| format!("Unknown packet ID {id}"))
}
fn count(r: &mut Cursor<'_>, max: usize) -> Result<usize> {
    let n = usize::try_from(r.varint()?).map_err(|_| "Negative array size")?;
    if n > max {
        return Err("Array exceeds protocol bound".into());
    }
    Ok(n)
}
fn remaining<'a>(r: &mut Cursor<'a>, bytes: &'a [u8]) -> &'a [u8] {
    &bytes[r.position()..]
}
fn varlong(r: &mut Cursor<'_>) -> Result<u64> {
    let mut out = 0;
    for i in 0..10 {
        let b = r.take(1)?[0];
        if i == 9 && b > 1 {
            return Err("VarLong overflow".into());
        }
        out |= u64::from(b & 127) << (7 * i);
        if b < 128 {
            return Ok(out);
        }
    }
    Err("VarLong too long".into())
}
fn put_varlong(out: &mut Vec<u8>, mut v: u64) {
    while v > 127 {
        out.push((v as u8 & 127) | 128);
        v >>= 7;
    }
    out.push(v as u8);
}
// In 26.3 java.util.BitSet uses a byte array, not a big-endian long array.
pub fn light_masks(r: &mut Cursor<'_>, out: &mut Vec<u8>) -> Result<()> {
    for _ in 0..4 {
        let n = count(r, 4096)?;
        let mut bytes = Vec::with_capacity(n * 8);
        for _ in 0..n {
            bytes.extend_from_slice(
                &u64::from_be_bytes(r.take(8)?.try_into().unwrap()).to_le_bytes(),
            );
        }
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        put_varint(out, bytes.len() as i32);
        out.extend_from_slice(&bytes);
    }
    Ok(())
}
fn light_layers(r: &mut Cursor<'_>, out: &mut Vec<u8>) -> Result<()> {
    for _ in 0..2 {
        let n = count(r, 4096)?;
        put_varint(out, n as i32);
        for _ in 0..n {
            let size = count(r, 2048)?;
            put_varint(out, size as i32);
            out.extend_from_slice(r.take(size)?);
        }
    }
    Ok(())
}
fn palette<M: Maps>(r: &mut Cursor<'_>, out: &mut Vec<u8>, blocks: bool) -> Result<()> {
    let bits = r.take(1)?[0];
    let entries = if blocks { 4096usize } else { 64 };
    let local_max = if blocks { 8 } else { 3 };
    if bits == 0 {
        out.push(0);
        let id = r.varint()?;
        put_varint(out, if blocks { M::BLOCKSTATES.map(id)? } else { id });
        return Ok(());
    }
    if bits > 32 || (blocks && bits < 4) {
        return Err("Invalid palette bit count".into());
    }
    if blocks
        && bits > local_max
        && bits != (i32::BITS - (M::BLOCKSTATES.size - 1).leading_zeros()) as u8
    {
        return Err("Global block palette has the wrong registry bit width".into());
    }
    let mut palette_size = 0;
    let output_bits = if blocks && bits > local_max {
        (i32::BITS - (M::BLOCKSTATES.mapped_size - 1).leading_zeros()) as u8
    } else {
        bits
    };
    out.push(output_bits);
    if bits <= local_max {
        palette_size = count(r, 1usize << bits)?;
        if palette_size == 0 {
            return Err("Empty indirect palette".into());
        }
        put_varint(out, palette_size as i32);
        for _ in 0..palette_size {
            let id = r.varint()?;
            if id < 0 {
                return Err("Negative palette ID".into());
            }
            put_varint(out, if blocks { M::BLOCKSTATES.map(id)? } else { id });
        }
    }
    let per = 64 / usize::from(bits);
    let words = entries.div_ceil(per);
    let input = r.take(words * 8)?;
    let get = |i: usize| {
        let word = u64::from_be_bytes(input[i / per * 8..i / per * 8 + 8].try_into().unwrap());
        ((word >> ((i % per) * usize::from(bits))) & ((1u64 << bits) - 1)) as i32
    };
    if blocks && bits > local_max {
        let target_per = 64 / usize::from(output_bits);
        for start in (0..entries).step_by(target_per) {
            let mut word = 0u64;
            for i in start..(start + target_per).min(entries) {
                word |= (M::BLOCKSTATES.map(get(i))? as u64)
                    << ((i - start) * usize::from(output_bits));
            }
            out.extend_from_slice(&word.to_be_bytes());
        }
    } else {
        if palette_size > 0 && (0..entries).any(|i| get(i) as usize >= palette_size) {
            return Err("Palette index outside table".into());
        }
        out.extend_from_slice(input);
    }
    Ok(())
}
pub fn light_data(input: &[u8]) -> Result<Vec<u8>> {
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    light_masks(&mut reader, &mut output)?;
    light_layers(&mut reader, &mut output)?;
    reader.end()?;
    Ok(output)
}
// Pre-1.21.5 palettes include a VarInt long-array length, even for a
// single-value palette. Keep this primitive pass usable by the standalone core.
fn palette_1214(
    r: &mut Cursor<'_>,
    out: &mut Vec<u8>,
    blocks: bool,
    global_bits: u8,
) -> Result<()> {
    let wire = r.take(1)?[0];
    let bits = match wire {
        0 => 0,
        1..=8 if blocks => wire.max(4),
        1..=3 if !blocks => wire,
        _ if global_bits != 0 => global_bits,
        _ => wire,
    };
    if bits > 32 {
        return Err("Old palette width exceeds 32 bits".into());
    }
    out.push(bits);
    if bits == 0 {
        let id = r.varint()?;
        if id < 0 {
            return Err("Negative old single palette id".into());
        }
        put_varint(out, id);
        if r.varint()? != 0 {
            return Err("Single palette has nonempty storage".into());
        }
        return Ok(());
    }
    let local = bits <= if blocks { 8 } else { 3 };
    let mut size = 0;
    if local {
        size = count(r, 1usize << bits)?;
        if size == 0 {
            return Err("Empty old local palette".into());
        }
        put_varint(out, size as i32);
        for _ in 0..size {
            let id = r.varint()?;
            if id < 0 {
                return Err("Negative old palette id".into());
            }
            put_varint(out, id);
        }
    }
    let entries: usize = if blocks { 4096 } else { 64 };
    let per = 64 / usize::from(bits);
    let expected = entries.div_ceil(per);
    let count = count(r, expected)?;
    if count == 0 {
        out.resize(out.len() + expected * 8, 0);
        return Ok(());
    }
    if count != expected {
        return Err("Incorrect old palette storage length".into());
    }
    let bytes = r.take(expected * 8)?;
    if local {
        for i in 0..entries {
            let word = u64::from_be_bytes(bytes[i / per * 8..i / per * 8 + 8].try_into().unwrap());
            let index = (word >> (i % per * usize::from(bits))) & ((1u64 << bits) - 1);
            if index >= size as u64 {
                return Err("Old palette index outside table".into());
            }
        }
    }
    out.extend_from_slice(bytes);
    Ok(())
}
fn registry_bits(count: usize) -> u8 {
    (usize::BITS - count.saturating_sub(1).leading_zeros()) as u8
}
fn section_pending(size: usize, position: usize, expected: Option<usize>, seen: usize) -> bool {
    expected.map_or(position < size, |count| seen < count)
}
pub(crate) fn section_padding(
    input: &[u8],
    position: usize,
    expected: Option<usize>,
) -> Result<()> {
    // Vanilla's preallocated buffer may exceed the bytes actually written.
    // Only a dimension-bounded parse may discard that padding, and it must be
    // all zero. Without a dimension, retain the strict end-of-buffer behavior.
    if expected.is_some() && input[position..].iter().any(|byte| *byte != 0) {
        return Err("Nonzero chunk section padding".into());
    }
    Ok(())
}
fn sections_1214<M: Maps>(
    input: &[u8],
    biome_count: usize,
    expected: Option<usize>,
) -> Result<Vec<u8>> {
    if input.len() > MAX_PACKET {
        return Err("Old section buffer too large".into());
    }
    let mut r = Cursor::new(input);
    let mut out = Vec::new();
    let mut sections = 0;
    while section_pending(input.len(), r.position(), expected, sections) {
        sections += 1;
        if sections > 256 {
            return Err("Too many old chunk sections".into());
        }
        let non_air = r.take(2)?;
        // This is an advisory signed short, not a buffer length. Preserve it
        // like ViaVersion's ChunkSectionType1_18 and Horizon's Section codec;
        // server/plugin counters need not lie in 0..=4096. Palettes below
        // still validate their own lengths, identifiers and storage indices.
        out.extend_from_slice(non_air);
        palette_1214(
            &mut r,
            &mut out,
            true,
            registry_bits(M::BLOCKSTATES.size as usize),
        )?;
        palette_1214(&mut r, &mut out, false, registry_bits(biome_count))?;
    }
    section_padding(input, r.position(), expected)?;
    Ok(out)
}
pub fn biome_palettes_1214(input: &[u8]) -> Result<Vec<u8>> {
    biome_palettes_1214_using(input, 0)
}
pub fn biome_palettes_1214_using(input: &[u8], biome_count: usize) -> Result<Vec<u8>> {
    if input.len() > MAX_PACKET {
        return Err("Old biome buffer too large".into());
    }
    let mut r = Cursor::new(input);
    let mut out = Vec::new();
    let mut sections = 0;
    while r.position() < input.len() {
        sections += 1;
        if sections > 256 {
            return Err("Too many old biome sections".into());
        }
        palette_1214(&mut r, &mut out, false, registry_bits(biome_count))?;
    }
    Ok(out)
}
pub fn chunk_sections_with<M: Maps>(input: &[u8]) -> Result<Vec<u8>> {
    chunk_sections_using::<M>(input, 0)
}
pub fn chunk_sections_using<M: Maps>(input: &[u8], biome_count: usize) -> Result<Vec<u8>> {
    chunk_sections_in_dimension::<M>(input, biome_count, None)
}
pub fn chunk_sections_in_dimension<M: Maps>(
    input: &[u8],
    biome_count: usize,
    expected: Option<usize>,
) -> Result<Vec<u8>> {
    if expected.is_some_and(|count| count == 0 || count > 256) {
        return Err("Invalid dimension section count".into());
    }
    let old_lengths = if M::PROTOCOL < 770 {
        Some(sections_1214::<M>(input, biome_count, expected)?)
    } else {
        None
    };
    let input = old_lengths.as_deref().unwrap_or(input);
    let expanded = if M::PROTOCOL < 775 {
        Some(sections_12111::<M>(input, expected)?)
    } else {
        None
    };
    let input = expanded.as_deref().unwrap_or(input);
    if input.len() > MAX_PACKET {
        return Err("Section buffer too large".into());
    }
    let mut r = Cursor::new(input);
    let mut out = Vec::new();
    let mut sections = 0;
    while section_pending(input.len(), r.position(), expected, sections) {
        sections += 1;
        if sections > 256 {
            return Err("Too many chunk sections".into());
        }
        let counts = r.take(4)?;
        // Both counters are signed shorts on the wire. They do not size the
        // palettes; retaining them matches the native and ViaVersion codecs.
        out.extend_from_slice(counts);
        palette::<M>(&mut r, &mut out, true)?;
        palette::<M>(&mut r, &mut out, false)?;
    }
    section_padding(input, r.position(), expected)?;
    Ok(out)
}
fn fluid_12111<M: Maps>(id: i32) -> Result<bool> {
    use std::sync::OnceLock;
    static RANGES: OnceLock<Result<Vec<(i32, i32)>>> = OnceLock::new();
    let ranges = RANGES
        .get_or_init(|| {
            let bytes = crate::vfp_tables_12111::FLUID_RANGES_261;
            let mut r = Cursor::new(bytes);
            let mut end = 0i32;
            let mut ranges = Vec::new();
            while r.position() < bytes.len() {
                let offset = r.varint()?;
                let length = r.varint()?;
                if offset < 0 || length < 0 {
                    return Err("Invalid fluid-id range".into());
                }
                let start = end.checked_add(offset).ok_or("Fluid-id overflow")?;
                let last = start.checked_add(length).ok_or("Fluid-id overflow")?;
                end = last.checked_add(1).ok_or("Fluid-id overflow")?;
                ranges.push((start, last));
            }
            Ok(ranges)
        })
        .as_ref()
        .map_err(Clone::clone)?;
    // All old section palettes still contain ids from the original server.
    // Classify through the composed map, then the bundled 26.1 fluid domain;
    // do not assume the source ids already belong to the 1.21.2 registry.
    // This lookup does not rewrite the palette; the final codec maps it once.
    let native = M::BLOCKSTATES.map(id)?;
    let id = crate::vfp_mapping::V261::BLOCKSTATES.inverse(native)?;
    let index = ranges.partition_point(|&(start, _)| start <= id);
    Ok(index != 0 && id <= ranges[index - 1].1)
}
fn palette_12111<M: Maps>(r: &mut Cursor<'_>, blocks: bool) -> Result<i16> {
    let entries = if blocks { 4096usize } else { 64 };
    let local_max = if blocks { 8 } else { 3 };
    let bits = r.take(1)?[0];
    if bits == 0 {
        let id = r.varint()?;
        if id < 0 {
            return Err("Negative palette id".into());
        }
        return Ok(if blocks && fluid_12111::<M>(id)? {
            4096
        } else {
            0
        });
    }
    if bits > 32 || (blocks && bits < 4) {
        return Err("Invalid old palette width".into());
    }
    let mut ids = Vec::new();
    if bits <= local_max {
        let n = count(r, 1usize << bits)?;
        if n == 0 {
            return Err("Empty old indirect palette".into());
        }
        for _ in 0..n {
            let id = r.varint()?;
            if id < 0 {
                return Err("Negative palette id".into());
            }
            ids.push(id);
        }
    }
    let per = 64 / usize::from(bits);
    let words = r.take(entries.div_ceil(per) * 8)?;
    let mut fluid = 0;
    for i in 0..entries {
        let word = u64::from_be_bytes(words[i / per * 8..i / per * 8 + 8].try_into().unwrap());
        let value = ((word >> ((i % per) * usize::from(bits))) & ((1u64 << bits) - 1)) as usize;
        let id = if ids.is_empty() {
            i32::try_from(value).map_err(|_| "Palette id overflow")?
        } else {
            *ids.get(value).ok_or("Old palette index outside table")?
        };
        if blocks && fluid_12111::<M>(id)? {
            fluid += 1;
        }
    }
    Ok(fluid)
}
fn sections_12111<M: Maps>(input: &[u8], expected: Option<usize>) -> Result<Vec<u8>> {
    if input.len() > MAX_PACKET {
        return Err("Section buffer too large".into());
    }
    let mut r = Cursor::new(input);
    let mut out = Vec::new();
    let mut sections = 0;
    while section_pending(input.len(), r.position(), expected, sections) {
        sections += 1;
        if sections > 256 {
            return Err("Too many old chunk sections".into());
        }
        let non_air = r.take(2)?;
        let start = r.position();
        let fluid = palette_12111::<M>(&mut r, true)?;
        palette_12111::<M>(&mut r, false)?;
        out.extend_from_slice(non_air);
        out.extend_from_slice(&fluid.to_be_bytes());
        out.extend_from_slice(&input[start..r.position()]);
    }
    section_padding(input, r.position(), expected)?;
    Ok(out)
}
pub fn chunk_with_light_with<M: Maps>(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Cursor::new(payload);
    r.take(8)?;
    let heightmaps = count(&mut r, 16)?;
    for _ in 0..heightmaps {
        r.varint()?;
        let n = count(&mut r, 4096)?;
        r.take(n * 8)?;
    }
    let prefix = r.position();
    let size = count(&mut r, MAX_PACKET)?;
    let sections = chunk_sections_with::<M>(r.take(size)?)?;
    let mut out = payload[..prefix].to_vec();
    put_varint(&mut out, sections.len() as i32);
    out.extend_from_slice(&sections);
    // Block entities also contain NBT data with versioned schemas. Do not pass
    // them unchanged under the pretense of full world compatibility.
    if count(&mut r, 65536)? != 0 {
        return Err("Chunk block-entity NBT conversion is required".into());
    }
    put_varint(&mut out, 0);
    light_masks(&mut r, &mut out)?;
    light_layers(&mut r, &mut out)?;
    r.end()?;
    Ok(out)
}
pub fn clientbound_with<M: Maps>(id: i32, payload: &[u8]) -> Result<Vec<Packet>> {
    let packet = name(t::OLD_CLIENTBOUND, id)?;
    let new_id = packet_id(t::NEW_CLIENTBOUND, packet)?;
    let mut r = Cursor::new(payload);
    let mut out = Vec::new();
    let mut extra = None;
    match packet {
        "BLOCK_UPDATE" => {
            out.extend_from_slice(r.take(8)?);
            put_varint(&mut out, M::BLOCKSTATES.map(r.varint()?)?);
            r.end()?;
        }
        "BLOCK_EVENT" => {
            out.extend_from_slice(r.take(10)?);
            put_varint(&mut out, M::BLOCKS.map(r.varint()?)?);
            r.end()?;
        }
        "SECTION_BLOCKS_UPDATE" => {
            out.extend_from_slice(r.take(8)?);
            let n = count(&mut r, 4096)?;
            put_varint(&mut out, n as i32);
            for _ in 0..n {
                let v = varlong(&mut r)?;
                let state = i32::try_from(v >> 12).map_err(|_| "Block state overflow")?;
                put_varlong(
                    &mut out,
                    ((M::BLOCKSTATES.map(state)? as u64) << 12) | (v & 4095),
                );
            }
            r.end()?;
        }
        "LEVEL_CHUNK_WITH_LIGHT" => {
            out = chunk_with_light_with::<M>(payload)?;
        }
        "LIGHT_UPDATE" => {
            let x = r.varint()?;
            let z = r.varint()?;
            put_varint(&mut out, x);
            put_varint(&mut out, z);
            light_masks(&mut r, &mut out)?;
            light_layers(&mut r, &mut out)?;
            r.end()?;
        }
        "MOVE_ENTITY_POS" | "MOVE_ENTITY_POS_ROT" => {
            put_varint(&mut out, r.varint()?);
            let delta = r.take(6)?;
            let rot = if packet == "MOVE_ENTITY_POS_ROT" {
                r.take(2)?
            } else {
                &[]
            };
            let on_ground = r.take(1)?[0];
            if on_ground > 1 {
                return Err("Invalid on-ground flag".into());
            }
            r.end()?;
            put_varint(&mut out, 2 | i32::from(on_ground));
            put_varint(&mut out, 3);
            out.extend_from_slice(delta);
            out.extend_from_slice(rot);
        }
        "MOVE_ENTITY_ROT" => {
            put_varint(&mut out, r.varint()?);
            let rot = r.take(2)?;
            out.extend_from_slice(r.take(1)?);
            out.extend_from_slice(rot);
            r.end()?;
        }
        "ENTITY_POSITION_SYNC" => {
            put_varint(&mut out, r.varint()?);
            put_varint(&mut out, 1);
            put_varint(&mut out, 1);
            out.extend_from_slice(r.take(24)?);
            put_varint(&mut out, 3);
            r.take(24)?;
            out.extend_from_slice(r.take(9)?);
            r.end()?;
        }
        "ADD_ENTITY" => {
            put_varint(&mut out, r.varint()?);
            out.extend_from_slice(r.take(16)?);
            put_varint(&mut out, M::ENTITIES.map(r.varint()?)?);
            out.extend_from_slice(remaining(&mut r, payload));
        }
        "OPEN_SIGN_EDITOR" => {
            out.extend_from_slice(r.take(8)?);
            let front = r.take(1)?[0];
            if front > 1 {
                return Err("Invalid sign side".into());
            }
            put_varint(&mut out, front as i32);
            r.end()?;
        }
        "SOUND" | "SOUND_ENTITY" => {
            let holder = r.varint()?;
            if holder < 0 {
                return Err("Invalid sound holder".into());
            }
            put_varint(
                &mut out,
                if holder == 0 {
                    0
                } else {
                    M::SOUNDS.map(holder - 1)? + 1
                },
            );
            out.extend_from_slice(remaining(&mut r, payload));
        }
        "AWARD_STATS" => {
            let n = count(&mut r, 65536)?;
            put_varint(&mut out, n as i32);
            for _ in 0..n {
                let category = r.varint()?;
                put_varint(&mut out, category);
                let id = r.varint()?;
                let map = match category {
                    0 => M::BLOCKS,
                    1..=5 => M::ITEMS,
                    6 | 7 => M::ENTITIES,
                    8 => M::STATISTICS,
                    _ => return Err("Unknown statistics category".into()),
                };
                put_varint(&mut out, map.map(id)?);
                put_varint(&mut out, r.varint()?);
            }
            r.end()?;
        }
        "LEVEL_EVENT" => {
            let kind = i32::from_be_bytes(r.take(4)?.try_into().unwrap());
            let pos = r.take(8)?;
            let data = i32::from_be_bytes(r.take(4)?.try_into().unwrap());
            let global = r.take(1)?;
            r.end()?;
            out.extend_from_slice(&kind.to_be_bytes());
            out.extend_from_slice(pos);
            out.extend_from_slice(
                &if kind == 2001 {
                    M::BLOCKSTATES.map(data)?
                } else {
                    data
                }
                .to_be_bytes(),
            );
            out.extend_from_slice(global);
            if kind == 2002 || kind == 2007 {
                let mut sound = Vec::new();
                sound.extend_from_slice(&if kind == 2002 { 1053i32 } else { 1054 }.to_be_bytes());
                sound.extend_from_slice(pos);
                sound.extend_from_slice(&0i32.to_be_bytes());
                sound.extend_from_slice(global);
                extra = Some((new_id, sound));
            }
        }
        "BUNDLE_DELIMITER"
        | "KEEP_ALIVE"
        | "PING"
        | "BLOCK_CHANGED_ACK"
        | "BLOCK_DESTRUCTION"
        | "CHUNK_BATCH_FINISHED"
        | "CHUNK_BATCH_START"
        | "CLEAR_TITLES"
        | "CONTAINER_CLOSE"
        | "CONTAINER_SET_DATA"
        | "CUSTOM_PAYLOAD"
        | "FORGET_LEVEL_CHUNK"
        | "GAME_EVENT"
        | "HURT_ANIMATION"
        | "INITIALIZE_BORDER"
        | "PING_RESPONSE"
        | "PLAYER_COMBAT_END"
        | "PLAYER_COMBAT_ENTER"
        | "PLAYER_ROTATION"
        | "REMOVE_ENTITIES"
        | "REMOVE_MOB_EFFECT"
        | "ROTATE_HEAD"
        | "SET_BORDER_CENTER"
        | "SET_BORDER_LERP_SIZE"
        | "SET_BORDER_SIZE"
        | "SET_BORDER_WARNING_DELAY"
        | "SET_BORDER_WARNING_DISTANCE"
        | "SET_CAMERA"
        | "SET_CARRIED_ITEM"
        | "SET_CHUNK_CACHE_CENTER"
        | "SET_CHUNK_CACHE_RADIUS"
        | "SET_DEFAULT_SPAWN_POSITION"
        | "SET_EXPERIENCE"
        | "SET_HEALTH"
        | "SET_RENDER_DISTANCE"
        | "STOP_SOUND"
        | "TICKING_STATE"
        | "TICKING_STEP" => {
            out = payload.to_vec();
        }
        _ => return Err(format!("26.2 {packet} still requires a payload converter")),
    }
    let mut packets = vec![(new_id, out)];
    if let Some(extra) = extra {
        packets.push(extra);
    }
    Ok(packets)
}
pub fn serverbound_262(id: i32, payload: &[u8]) -> Result<Vec<Packet>> {
    let packet = name(t::NEW_SERVERBOUND, id)?;
    let mut r = Cursor::new(payload);
    let mut out = Vec::new();
    match packet {
        "ACCEPT_TELEPORTATION" => {
            put_varint(&mut out, r.varint()?);
            let movement = r.take(32)?.to_vec();
            r.end()?;
            let mut movement = movement;
            movement.push(0);
            return Ok(vec![
                (packet_id(t::OLD_SERVERBOUND, packet)?, out),
                (
                    packet_id(t::OLD_SERVERBOUND, "MOVE_PLAYER_POS_ROT")?,
                    movement,
                ),
            ]);
        }
        "SIGN_UPDATE" => {
            let pos = r.take(8)?;
            let start = r.position();
            for _ in 0..4 {
                r.string(384)?;
            }
            let end = r.position();
            let slot = r.varint()?;
            r.end()?;
            if ![0, 1].contains(&slot) {
                return Err("Unknown sign text slot".into());
            }
            out.extend_from_slice(pos);
            out.push(slot as u8);
            out.extend_from_slice(&payload[start..end]);
        }
        "PUNCH" => {
            r.end()?;
            return Ok(vec![(packet_id(t::OLD_SERVERBOUND, "SWING")?, vec![0])]);
        }
        "PLAYER_ACTION" => {
            let action = r.varint()?;
            let pos = r.take(8)?;
            let face = r.take(1)?;
            let seq = r.varint()?;
            r.end()?;
            if !(0..=8).contains(&action) {
                return Err("Unknown player action".into());
            }
            if action == 1 {
                return Ok(Vec::new());
            }
            put_varint(&mut out, if action > 1 { action - 1 } else { action });
            out.extend_from_slice(pos);
            out.extend_from_slice(face);
            put_varint(&mut out, seq);
        }
        "KEEP_ALIVE"
        | "PONG"
        | "MOVE_PLAYER_POS"
        | "MOVE_PLAYER_POS_ROT"
        | "MOVE_PLAYER_ROT"
        | "MOVE_PLAYER_STATUS_ONLY"
        | "CLIENT_TICK_END"
        | "CHAT_ACK"
        | "CHAT"
        | "CHAT_COMMAND"
        | "CLIENT_COMMAND"
        | "CLIENT_INFORMATION"
        | "CHUNK_BATCH_RECEIVED"
        | "PLAYER_INPUT"
        | "PLAYER_LOADED"
        | "SET_CARRIED_ITEM"
        | "USE_ITEM"
        | "USE_ITEM_ON"
        | "ATTACK"
        | "INTERACT"
        | "CUSTOM_PAYLOAD"
        | "RESOURCE_PACK"
        | "COOKIE_RESPONSE"
        | "ACCEPT_CODE_OF_CONDUCT" => {
            out = payload.to_vec();
        }
        _ => {
            return Err(format!(
                "26.3 {packet} still requires a serverbound payload converter"
            ));
        }
    }
    Ok(vec![(packet_id(t::OLD_SERVERBOUND, packet)?, out)])
}

pub fn chunk_sections(input: &[u8]) -> Result<Vec<u8>> {
    chunk_sections_with::<V262>(input)
}
pub fn chunk_with_light(payload: &[u8]) -> Result<Vec<u8>> {
    chunk_with_light_with::<V262>(payload)
}
pub fn clientbound_262(id: i32, payload: &[u8]) -> Result<Vec<Packet>> {
    clientbound_with::<V262>(id, payload)
}
