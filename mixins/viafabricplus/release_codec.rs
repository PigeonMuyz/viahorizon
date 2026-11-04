// SPDX-License-Identifier: GPL-3.0-or-later
// Primitive formats used by the pre-1.19.3 Release adapters. These operate on
// packet bodies; Horizon still owns transport encryption and compression.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
};
use horizon_world::nbt::{self, Tag};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
};

pub fn end(r: &Reader<'_>) -> Result<()> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(Error::new("Trailing legacy Release packet bytes"))
    }
}
pub fn boolean(r: &mut Reader<'_>) -> Result<bool> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::new("Invalid legacy boolean")),
    }
}
pub fn optional_text(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let present = boolean(r)?;
    w.bool(present);
    if present {
        w.string(&r.string(262144)?);
    }
    Ok(())
}
pub fn properties(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let count = r.count(1024)?;
    w.varint(count as i32);
    for _ in 0..count {
        w.string(&r.string(64)?);
        w.string(&r.string(32767)?);
        let signed = boolean(r)?;
        w.bool(signed);
        if signed {
            w.string(&r.string(4096)?);
        }
    }
    Ok(())
}
pub fn skip_profile_key(r: &mut Reader<'_>) -> Result<()> {
    if boolean(r)? {
        r.i64()?;
        r.byte_array(4096)?;
        r.byte_array(4096)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}
impl Position {
    pub fn unpack(value: i64, protocol: i32) -> Self {
        let x = (value >> 38) as i32;
        let (y, z) = if protocol < 477 {
            (((value << 26) >> 52) as i32, ((value << 38) >> 38) as i32)
        } else {
            (((value << 52) >> 52) as i32, ((value << 26) >> 38) as i32)
        };
        Self { x, y, z }
    }
    pub fn pack(self, protocol: i32) -> Result<i64> {
        if !(-33554432..33554432).contains(&self.x)
            || !(-33554432..33554432).contains(&self.z)
            || !(-2048..2048).contains(&self.y)
        {
            return Err(Error::new("Legacy block position outside packed range"));
        }
        let x = (i64::from(self.x) & 0x3ffffff) << 38;
        let z = i64::from(self.z) & 0x3ffffff;
        let y = i64::from(self.y) & 0xfff;
        Ok(if protocol < 477 {
            x | (y << 26) | z
        } else {
            x | (z << 12) | y
        })
    }
    pub fn read(r: &mut Reader<'_>, protocol: i32, short_y: bool) -> Result<Self> {
        if protocol >= 47 {
            return Ok(Self::unpack(r.i64()?, protocol));
        }
        Ok(Self {
            x: r.i32()?,
            y: if short_y {
                i32::from(r.i16()?)
            } else {
                i32::from(r.u8()?)
            },
            z: r.i32()?,
        })
    }
    pub fn write(self, w: &mut Writer, protocol: i32, short_y: bool) -> Result<()> {
        if protocol >= 47 {
            w.i64(self.pack(protocol)?);
        } else {
            w.i32(self.x);
            if short_y {
                w.i16(i16::try_from(self.y).map_err(|_| Error::new("Legacy Y exceeds short"))?);
            } else {
                w.u8(u8::try_from(self.y).map_err(|_| Error::new("Legacy Y exceeds byte"))?);
            }
            w.i32(self.z);
        }
        Ok(())
    }
}

fn limited_read(mut input: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    input
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|_| Error::new("Invalid compressed legacy payload"))?;
    if out.len() > limit {
        return Err(Error::new("Legacy decompression exceeds bound"));
    }
    Ok(out)
}
pub fn inflate_zlib(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    if expected > crate::vfp_core::MAX_PACKET {
        return Err(Error::new("Legacy chunk exceeds bound"));
    }
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let out = limited_read(&mut decoder, expected)?;
    if out.len() != expected || decoder.total_in() != data.len() as u64 {
        return Err(Error::new("Legacy chunk compressed length mismatch"));
    }
    Ok(out)
}
pub fn read_compressed_nbt(r: &mut Reader<'_>) -> Result<Tag> {
    let length = r.i16()?;
    if length == -1 {
        return Ok(Tag::End);
    }
    if length < 0 {
        return Err(Error::new("Invalid compressed item NBT length"));
    }
    let data = r.bytes(length as usize)?;
    let mut decoder = flate2::bufread::GzDecoder::new(data);
    let bytes = limited_read(&mut decoder, crate::buf::NBT_QUOTA)?;
    if !decoder.into_inner().is_empty() {
        return Err(Error::new("Trailing compressed legacy item NBT bytes"));
    }
    let (root, used) = nbt::read_named(
        &bytes,
        nbt::Limits {
            max_bytes: crate::buf::NBT_QUOTA,
            max_nodes: 100000,
            max_depth: 64,
        },
    )
    .map_err(|_| Error::new("Invalid compressed item NBT"))?;
    if used != bytes.len() || !matches!(root.tag, Tag::Compound(_)) {
        return Err(Error::new("Invalid compressed item root"));
    }
    Ok(root.tag)
}
fn write_compressed_nbt(w: &mut Writer, tag: &Tag) -> Result<()> {
    if matches!(tag, Tag::End) {
        w.i16(-1);
        return Ok(());
    }
    let bytes = nbt::write_named(
        &nbt::NamedTag {
            name: Default::default(),
            tag: tag.clone(),
        },
        nbt::Limits {
            max_bytes: crate::buf::NBT_QUOTA,
            max_nodes: 100000,
            max_depth: 64,
        },
    )
    .map_err(|_| Error::new("Legacy item NBT exceeds bound"))?;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(&bytes)
        .map_err(|_| Error::new("Legacy item compression failed"))?;
    let data = encoder
        .finish()
        .map_err(|_| Error::new("Legacy item compression failed"))?;
    w.i16(
        i16::try_from(data.len()).map_err(|_| Error::new("Legacy compressed NBT exceeds short"))?,
    );
    w.bytes(&data);
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    pub id: i32,
    pub count: i8,
    pub damage: i16,
    pub tag: Tag,
}
impl Stack {
    pub fn read(r: &mut Reader<'_>, protocol: i32) -> Result<Option<Self>> {
        let id = if protocol >= 404 {
            if !boolean(r)? {
                return Ok(None);
            }
            r.varint()?
        } else {
            let id = r.i16()?;
            if id == -1 {
                return Ok(None);
            }
            i32::from(id)
        };
        if id < 0 {
            return Err(Error::new("Negative legacy item ID"));
        }
        let count = r.i8()?;
        let damage = if protocol < 393 { r.i16()? } else { 0 };
        let tag = if protocol < 47 {
            read_compressed_nbt(r)?
        } else {
            r.vfp_named_nbt()?
        };
        if !matches!(tag, Tag::End | Tag::Compound(_)) {
            return Err(Error::new("Legacy item NBT is not a compound"));
        }
        Ok(Some(Self {
            id,
            count,
            damage,
            tag,
        }))
    }
    pub fn write(stack: Option<&Self>, w: &mut Writer, protocol: i32) -> Result<()> {
        let Some(stack) = stack else {
            if protocol >= 404 {
                w.bool(false);
            } else {
                w.i16(-1);
            }
            return Ok(());
        };
        if stack.id < 0 {
            return Err(Error::new("Negative outgoing legacy item ID"));
        }
        if protocol >= 404 {
            w.bool(true);
            w.varint(stack.id);
        } else {
            w.i16(i16::try_from(stack.id).map_err(|_| Error::new("Legacy item ID exceeds short"))?);
        }
        w.i8(stack.count);
        if protocol < 393 {
            w.i16(stack.damage);
        }
        if protocol < 47 {
            write_compressed_nbt(w, &stack.tag)?;
        } else {
            w.vfp_named_nbt(&stack.tag)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Packing {
    Spanning,
    Padded,
}
fn word_count(entries: usize, bits: u8, packing: Packing) -> usize {
    match packing {
        Packing::Spanning => (entries * usize::from(bits)).div_ceil(64),
        Packing::Padded => entries.div_ceil(64 / usize::from(bits)),
    }
}
pub fn unpack(words: &[i64], entries: usize, bits: u8, packing: Packing) -> Result<Vec<i32>> {
    if entries > 4096 || !(1..=31).contains(&bits) {
        return Err(Error::new("Invalid legacy packed-array dimensions"));
    }
    if words.len() != word_count(entries, bits, packing) {
        return Err(Error::new("Legacy packed-array length mismatch"));
    }
    let mask = (1u64 << bits) - 1;
    let per = 64 / usize::from(bits);
    let mut values = Vec::with_capacity(entries);
    for index in 0..entries {
        let (at, offset) = match packing {
            Packing::Spanning => (
                index * usize::from(bits) / 64,
                index * usize::from(bits) % 64,
            ),
            Packing::Padded => (index / per, index % per * usize::from(bits)),
        };
        let mut value = (words[at] as u64) >> offset;
        if packing == Packing::Spanning && offset + usize::from(bits) > 64 {
            value |= (words[at + 1] as u64) << (64 - offset);
        }
        values.push((value & mask) as i32);
    }
    Ok(values)
}
pub fn pack(values: &[i32], bits: u8, packing: Packing) -> Result<Vec<i64>> {
    if values.len() > 4096 || !(1..=31).contains(&bits) {
        return Err(Error::new("Invalid outgoing legacy packed array"));
    }
    let mut words = vec![0u64; word_count(values.len(), bits, packing)];
    let mask = (1u64 << bits) - 1;
    let per = 64 / usize::from(bits);
    for (index, &value) in values.iter().enumerate() {
        if value < 0 || value as u64 > mask {
            return Err(Error::new("Legacy palette value exceeds width"));
        }
        let (at, offset) = match packing {
            Packing::Spanning => (
                index * usize::from(bits) / 64,
                index * usize::from(bits) % 64,
            ),
            Packing::Padded => (index / per, index % per * usize::from(bits)),
        };
        words[at] |= (value as u64) << offset;
        if packing == Packing::Spanning && offset + usize::from(bits) > 64 {
            words[at + 1] |= (value as u64) >> (64 - offset);
        }
    }
    Ok(words.into_iter().map(|value| value as i64).collect())
}

#[derive(Clone, Copy)]
pub struct PaletteFormat {
    pub entries: usize,
    pub min_bits: u8,
    pub local_max: u8,
    pub global_bits: u8,
    pub single: bool,
    pub global_length: bool,
    pub packing: Packing,
}
impl PaletteFormat {
    pub fn blocks(protocol: i32, global_bits: u8) -> Self {
        Self {
            entries: 4096,
            min_bits: 4,
            local_max: 8,
            global_bits,
            single: protocol >= 757,
            global_length: protocol < 393,
            packing: if protocol < 735 {
                Packing::Spanning
            } else {
                Packing::Padded
            },
        }
    }
    pub fn biomes(global_bits: u8) -> Self {
        Self {
            entries: 64,
            min_bits: 1,
            local_max: 3,
            global_bits,
            single: true,
            global_length: false,
            packing: Packing::Padded,
        }
    }
    pub fn read(self, r: &mut Reader<'_>) -> Result<Vec<i32>> {
        let wire_bits = r.u8()?;
        if self.single && wire_bits == 0 {
            let value = r.varint()?;
            if value < 0 || r.varint()? != 0 {
                return Err(Error::new("Invalid single-value legacy palette"));
            }
            return Ok(vec![value; self.entries]);
        }
        let bits = if wire_bits <= self.local_max {
            wire_bits.max(self.min_bits)
        } else {
            self.global_bits
        };
        if !(1..=31).contains(&bits) {
            return Err(Error::new("Invalid legacy palette width"));
        }
        let local = bits <= self.local_max;
        let palette = if local || self.global_length {
            let count = r.count(1usize << self.local_max)?;
            if local && count == 0 {
                return Err(Error::new("Empty legacy local palette"));
            }
            let values = (0..count).map(|_| r.varint()).collect::<Result<Vec<_>>>()?;
            if values.iter().any(|&value| value < 0) {
                return Err(Error::new("Negative legacy palette value"));
            }
            values
        } else {
            Vec::new()
        };
        let words = r.long_array()?;
        let mut values = unpack(&words, self.entries, bits, self.packing)?;
        if local {
            for value in &mut values {
                *value = *palette
                    .get(*value as usize)
                    .ok_or_else(|| Error::new("Legacy palette index outside table"))?;
            }
        }
        Ok(values)
    }
    pub fn write(self, w: &mut Writer, values: &[i32]) -> Result<()> {
        if values.len() != self.entries || values.iter().any(|&value| value < 0) {
            return Err(Error::new("Invalid outgoing legacy palette values"));
        }
        let mut lookup = BTreeMap::new();
        let mut palette = Vec::new();
        let mut indices = Vec::with_capacity(values.len());
        for &value in values {
            let index = *lookup.entry(value).or_insert_with(|| {
                let at = palette.len() as i32;
                palette.push(value);
                at
            });
            indices.push(index);
        }
        if self.single && palette.len() == 1 {
            w.u8(0);
            w.varint(palette[0]);
            w.varint(0);
            return Ok(());
        }
        let local_bits =
            ((usize::BITS - (palette.len() - 1).leading_zeros()) as u8).max(self.min_bits);
        let local = local_bits <= self.local_max;
        let bits = if local { local_bits } else { self.global_bits };
        w.u8(bits);
        if local {
            w.varint(palette.len() as i32);
            for value in palette {
                w.varint(value);
            }
        } else if self.global_length {
            w.varint(0);
        }
        let words = pack(if local { &indices } else { values }, bits, self.packing)?;
        w.long_array(&words);
        Ok(())
    }
}

#[derive(Clone)]
pub struct OldSection {
    pub blocks: Vec<i32>,
    pub block_light: Vec<u8>,
    pub sky_light: Option<Vec<u8>>,
}
pub struct OldColumn {
    pub x: i32,
    pub z: i32,
    pub full: bool,
    pub sections: BTreeMap<i32, OldSection>,
    pub biomes: Option<Vec<i32>>,
}
impl OldColumn {
    pub fn read_17(r: &mut Reader<'_>, sky: bool) -> Result<Self> {
        let x = r.i32()?;
        let z = r.i32()?;
        let full = boolean(r)?;
        let mask = r.u16()?;
        let add = r.u16()?;
        if add & !mask != 0 {
            return Err(Error::new("Legacy add mask contains absent sections"));
        }
        let length = usize::try_from(r.i32()?)
            .map_err(|_| Error::new("Negative compressed chunk length"))?;
        if length > crate::vfp_core::MAX_PACKET {
            return Err(Error::new("Compressed legacy chunk exceeds bound"));
        }
        let compressed = r.bytes(length)?;
        let count = mask.count_ones() as usize;
        let expected = count * (4096 + 2048 + 2048 + if sky { 2048 } else { 0 })
            + add.count_ones() as usize * 2048
            + if full { 256 } else { 0 };
        let data = inflate_zlib(compressed, expected)?;
        Self::arrays_17(x, z, full, mask, add, sky, &data)
    }
    pub fn arrays_17(
        x: i32,
        z: i32,
        full: bool,
        mask: u16,
        add: u16,
        sky: bool,
        data: &[u8],
    ) -> Result<Self> {
        let mut r = Reader::new(data);
        let mut sections = BTreeMap::new();
        for y in 0..16 {
            if mask & (1 << y) == 0 {
                continue;
            }
            sections.insert(
                y,
                OldSection {
                    blocks: r
                        .bytes(4096)?
                        .iter()
                        .map(|&id| i32::from(id) << 4)
                        .collect(),
                    block_light: Vec::new(),
                    sky_light: None,
                },
            );
        }
        for section in sections.values_mut() {
            let data = r.bytes(2048)?;
            for (index, state) in section.blocks.iter_mut().enumerate() {
                *state |= i32::from((data[index / 2] >> ((index % 2) * 4)) & 15);
            }
        }
        for section in sections.values_mut() {
            section.block_light = r.bytes(2048)?.to_vec();
        }
        if sky {
            for section in sections.values_mut() {
                section.sky_light = Some(r.bytes(2048)?.to_vec());
            }
        }
        for y in 0..16 {
            if add & (1 << y) == 0 {
                continue;
            }
            let data = r.bytes(2048)?;
            let section = sections
                .get_mut(&y)
                .ok_or_else(|| Error::new("Add array for absent legacy section"))?;
            for (index, state) in section.blocks.iter_mut().enumerate() {
                *state |= i32::from((data[index / 2] >> ((index % 2) * 4)) & 15) << 12;
            }
        }
        let biomes = if full {
            Some(r.bytes(256)?.iter().map(|&id| i32::from(id)).collect())
        } else {
            None
        };
        end(&r)?;
        Ok(Self {
            x,
            z,
            full,
            sections,
            biomes,
        })
    }
    pub fn write_18(&self, w: &mut Writer, sky: bool) -> Result<()> {
        let mut mask = 0u16;
        let mut body = Writer::new();
        for (&y, section) in &self.sections {
            if !(0..16).contains(&y) || section.blocks.len() != 4096 {
                return Err(Error::new("Invalid legacy section coordinates"));
            }
            mask |= 1 << y;
            for &state in &section.blocks {
                let state = u16::try_from(state)
                    .map_err(|_| Error::new("Pre-flattening block state exceeds short"))?;
                body.bytes(&state.to_le_bytes());
            }
        }
        for section in self.sections.values() {
            if section.block_light.len() != 2048 {
                return Err(Error::new("Invalid legacy block light layer"));
            }
            body.bytes(&section.block_light);
        }
        if sky {
            for section in self.sections.values() {
                let data = section
                    .sky_light
                    .as_ref()
                    .ok_or_else(|| Error::new("Missing legacy sky light layer"))?;
                if data.len() != 2048 {
                    return Err(Error::new("Invalid legacy sky light layer"));
                }
                body.bytes(data);
            }
        }
        if self.full {
            let biomes = self
                .biomes
                .as_ref()
                .ok_or_else(|| Error::new("Missing full legacy column biomes"))?;
            if biomes.len() != 256 {
                return Err(Error::new("Invalid legacy biome count"));
            }
            for &biome in biomes {
                body.u8(u8::try_from(biome).map_err(|_| Error::new("Legacy biome exceeds byte"))?);
            }
        }
        w.i32(self.x);
        w.i32(self.z);
        w.bool(self.full);
        w.u16(mask);
        w.byte_array(&body.data);
        Ok(())
    }
}

// Server-authoritative inventory state for the transaction protocol predating
// container state IDs. No client-wide counters or optimistic success replies.
#[derive(Default)]
pub struct Transactions {
    windows: BTreeMap<u8, Window>,
    pub carried: Option<Stack>,
}
#[derive(Default)]
struct Window {
    next: i16,
    pending: BTreeMap<i16, BTreeMap<i16, Option<Stack>>>,
    order: std::collections::VecDeque<i16>,
    slots: BTreeMap<i16, Option<Stack>>,
    blocked: bool,
}
impl Transactions {
    pub fn close(&mut self, window: u8) {
        self.windows.remove(&window);
        self.carried = None;
    }
    pub fn content(&mut self, window: u8, slots: Vec<Option<Stack>>) -> Result<()> {
        if slots.len() > 4096 {
            return Err(Error::new("Legacy inventory exceeds slot bound"));
        }
        let state = self.windows.entry(window).or_default();
        state.slots = slots
            .into_iter()
            .enumerate()
            .map(|(i, stack)| (i as i16, stack))
            .collect();
        state.pending.clear();
        state.order.clear();
        state.blocked = false;
        Ok(())
    }
    pub fn set_slot(&mut self, window: u8, slot: i16, stack: Option<Stack>) {
        if window == 255 && slot == -1 {
            self.carried = stack;
        } else {
            self.windows
                .entry(window)
                .or_default()
                .slots
                .insert(slot, stack);
        }
    }
    pub fn clicked(&self, window: u8, slot: i16) -> Option<Stack> {
        self.projected_slot(window, slot).flatten()
    }
    pub fn projected_slot(&self, window: u8, slot: i16) -> Option<Option<Stack>> {
        let state = self.windows.get(&window)?;
        for action in state.order.iter().rev() {
            if let Some(value) = state
                .pending
                .get(action)
                .and_then(|changes| changes.get(&slot))
            {
                return Some(value.clone());
            }
        }
        state.slots.get(&slot).cloned()
    }
    pub fn authoritative_slot(&self, window: u8, slot: i16) -> Option<Option<Stack>> {
        self.windows.get(&window)?.slots.get(&slot).cloned()
    }
    pub fn pending(&self, window: u8, action: i16) -> bool {
        self.windows
            .get(&window)
            .is_some_and(|state| state.pending.contains_key(&action))
    }
    pub fn waiting(&self, window: u8) -> bool {
        self.windows
            .get(&window)
            .is_some_and(|state| state.blocked || state.pending.len() >= 128)
    }
    pub fn begin(&mut self, window: u8, changes: BTreeMap<i16, Option<Stack>>) -> Result<i16> {
        let state = self.windows.entry(window).or_default();
        if state.blocked || state.pending.len() >= 128 {
            return Err(Error::new(
                "Legacy container is waiting for server resynchronization",
            ));
        }
        let action = state.next;
        if state.pending.contains_key(&action) {
            return Err(Error::new(
                "Legacy transaction counter wrapped while pending",
            ));
        }
        state.next = state.next.wrapping_add(1);
        state.pending.insert(action, changes);
        state.order.push_back(action);
        Ok(action)
    }
    pub fn acknowledge(&mut self, window: u8, action: i16, accepted: bool) -> Result<bool> {
        let state = self
            .windows
            .get_mut(&window)
            .ok_or_else(|| Error::new("Transaction for unknown legacy window"))?;
        let changes = state
            .pending
            .remove(&action)
            .ok_or_else(|| Error::new("Unknown legacy transaction acknowledgement"))?;
        state.order.retain(|&value| value != action);
        if accepted {
            state.slots.extend(changes);
        } else {
            state.pending.clear();
            state.order.clear();
            state.blocked = true;
        }
        Ok(!accepted)
    }
}
