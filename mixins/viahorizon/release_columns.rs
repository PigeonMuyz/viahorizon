// SPDX-License-Identifier: GPL-3.0-or-later
// Chunk-body formats from 1.8 to 1.17. The representation keeps light separate
// from block state IDs and does not reuse a later version's palette widths.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::Step,
    vfp_release_codec::{self as codec, OldSection, Packing, PaletteFormat},
    vfp_wire::Packet,
};
use horizon_world::nbt::Tag;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct Column {
    pub x: i32,
    pub z: i32,
    pub full: bool,
    pub ignore_light: bool,
    pub sections: BTreeMap<usize, OldSection>,
    pub biomes: Option<Vec<i32>>,
    pub heightmap: Tag,
    pub entities: Vec<Tag>,
}
fn width(step: &Step, inverse: bool) -> u8 {
    step.registry("blockstates")
        .map_or(if step.source < 393 { 13 } else { 14 }, |map| {
            (32 - ((if inverse { map.mapped_size } else { map.size }) - 1).leading_zeros()) as u8
        })
}
fn count16(mask: i32) -> Result<u16> {
    u16::try_from(mask).map_err(|_| Error::new("Old column mask exceeds 16 sections"))
}
impl Column {
    pub fn read(step: &Step, data: &[u8], sky: bool) -> Result<Self> {
        let protocol = step.source;
        if !(47..=754).contains(&protocol) {
            return Err(Error::new("Wrong source for 16-section column codec"));
        }
        let mut r = Reader::new(data);
        let x = r.i32()?;
        let z = r.i32()?;
        let full = codec::boolean(&mut r)?;
        let ignore_light = if (735..751).contains(&protocol) {
            codec::boolean(&mut r)?
        } else {
            false
        };
        let mask = if protocol == 47 {
            r.u16()?
        } else {
            count16(r.varint()?)?
        };
        let heightmap = if protocol >= 477 {
            r.vfp_named_nbt()?
        } else {
            Tag::End
        };
        let mut biomes = if full && protocol >= 573 {
            let count = if protocol >= 751 {
                r.count(1024)?
            } else {
                1024
            };
            if count != 1024 {
                return Err(Error::new("Old 3D biome array must have 1024 entries"));
            }
            let values = (0..count)
                .map(|_| if protocol >= 751 { r.varint() } else { r.i32() })
                .collect::<Result<Vec<_>>>()?;
            Some(values)
        } else {
            None
        };
        let bytes = r.byte_array(crate::vfp_core::MAX_PACKET)?;
        let mut body = Reader::new(&bytes);
        let mut sections = BTreeMap::new();
        for y in 0..16 {
            if mask & (1 << y) == 0 {
                continue;
            }
            let blocks = if protocol == 47 {
                body.bytes(8192)?
                    .chunks_exact(2)
                    .map(|bytes| i32::from(u16::from_le_bytes(bytes.try_into().unwrap())))
                    .collect()
            } else {
                if protocol >= 477 {
                    body.i16()?; // Advisory; write() recomputes the count from the palette.
                }
                PaletteFormat::blocks(protocol, width(step, false)).read(&mut body)?
            };
            let mut section = OldSection {
                blocks,
                block_light: Vec::new(),
                sky_light: None,
            };
            if (107..477).contains(&protocol) {
                section.block_light = body.bytes(2048)?.to_vec();
                if sky {
                    section.sky_light = Some(body.bytes(2048)?.to_vec());
                }
            }
            sections.insert(y, section);
        }
        if protocol == 47 {
            for section in sections.values_mut() {
                section.block_light = body.bytes(2048)?.to_vec();
            }
            if sky {
                for section in sections.values_mut() {
                    section.sky_light = Some(body.bytes(2048)?.to_vec());
                }
            }
        }
        if full && protocol < 573 && !(protocol == 47 && mask == 0 && body.remaining() == 0) {
            biomes = Some(
                (0..256)
                    .map(|_| {
                        if protocol >= 393 {
                            body.i32()
                        } else {
                            body.u8().map(i32::from)
                        }
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
        }
        // 1.9 can reserve far more space than its section palettes need.
        crate::vfp_wire::section_padding(&bytes, body.position(), Some(mask.count_ones() as usize))
            .map_err(Error::new)?;
        let mut entities = Vec::new();
        if protocol >= 110 {
            let count = r.count(65536)?;
            for _ in 0..count {
                entities.push(r.vfp_named_nbt()?);
            }
        }
        codec::end(&r)?;
        Ok(Self {
            x,
            z,
            full,
            ignore_light,
            sections,
            biomes,
            heightmap,
            entities,
        })
    }
    pub fn legacy_block_entities(&mut self) {
        for (&section, data) in &self.sections {
            for (index, state) in data.blocks.iter().enumerate() {
                let kind = match state >> 4 {
                    61 | 62 => "Furnace",
                    54 | 146 => "Chest",
                    130 => "EnderChest",
                    84 => "RecordPlayer",
                    23 => "Trap",
                    158 => "Dropper",
                    63 | 68 => "Sign",
                    52 => "MobSpawner",
                    25 => "Music",
                    29 | 33 | 34 | 36 => "Piston",
                    117 => "Cauldron",
                    116 => "EnchantTable",
                    119 | 120 => "Airportal",
                    138 => "Beacon",
                    144 => "Skull",
                    151 | 178 => "DLDetector",
                    154 => "Hopper",
                    149 | 150 => "Comparator",
                    140 => "FlowerPot",
                    176 | 177 => "Banner",
                    209 => "EndGateway",
                    137 => "Control",
                    _ => continue,
                };
                let x = (self.x << 4) | (index & 15) as i32;
                let y = (section * 16 + (index >> 8)) as i32;
                let z = (self.z << 4) | ((index >> 4) & 15) as i32;
                self.entities.push(Tag::compound([
                    ("id", Tag::String(kind.into())),
                    ("x", Tag::Int(x)),
                    ("y", Tag::Int(y)),
                    ("z", Tag::Int(z)),
                ]));
            }
        }
    }
    pub fn remap(&mut self, step: &Step) -> Result<()> {
        for section in self.sections.values_mut() {
            if let Some(map) = step.registry("blockstates") {
                for block in &mut section.blocks {
                    *block = if step.source == 340 {
                        crate::vfp_release_flatten::state_id(step, *block)?
                    } else {
                        map.map(*block).map_err(Error::new)?
                    };
                }
            } else if step.source >= 393 {
                return Err(Error::new("Missing flattened column state map"));
            }
        }
        if step.source < 735 && step.target >= 735 {
            if let Tag::Compound(fields) = &mut self.heightmap {
                for value in fields.values_mut() {
                    if let Tag::LongArray(words) = value {
                        *words = codec::pack(
                            &codec::unpack(words, 256, 9, Packing::Spanning)?,
                            9,
                            Packing::Padded,
                        )?;
                    }
                }
            }
        }
        if step.source < 573 && step.target >= 573 {
            if let Some(old) = &self.biomes {
                if old.len() != 256 {
                    return Err(Error::new("Invalid 2D biome plane"));
                }
                let mut target = Vec::with_capacity(1024);
                for _ in 0..64 {
                    for z in 0..4 {
                        for x in 0..4 {
                            target.push(old[(z * 4 + 2) * 16 + x * 4 + 2]);
                        }
                    }
                }
                self.biomes = Some(target);
            }
        }
        Ok(())
    }
    pub fn write(&self, step: &Step, sky: bool) -> Result<Vec<u8>> {
        let protocol = step.target;
        if !(107..=755).contains(&protocol) {
            return Err(Error::new("Wrong target for old column encoder"));
        }
        let mut w = Writer::new();
        w.i32(self.x);
        w.i32(self.z);
        if protocol < 755 {
            w.bool(self.full);
        }
        if (735..751).contains(&protocol) {
            w.bool(self.ignore_light);
        }
        let mask = self.sections.keys().try_fold(0u16, |mask, &y| {
            if y >= 16 {
                Err(Error::new("Old section Y outside 0..16"))
            } else {
                Ok(mask | (1 << y))
            }
        })?;
        if protocol >= 755 {
            w.long_array(&[i64::from(mask)]);
        } else {
            w.varint(i32::from(mask));
        }
        if protocol >= 477 {
            w.vfp_named_nbt(&self.heightmap)?;
        }
        if self.full && protocol >= 573 {
            let biomes = self
                .biomes
                .as_ref()
                .ok_or_else(|| Error::new("Full old column has no biomes"))?;
            if biomes.len() != 1024 {
                return Err(Error::new("Wrong old 3D biome count"));
            }
            if protocol >= 751 {
                w.varint(1024);
            }
            for &biome in biomes {
                if protocol >= 751 {
                    w.varint(biome);
                } else {
                    w.i32(biome);
                }
            }
        }
        let mut body = Writer::new();
        for section in self.sections.values() {
            if section.blocks.len() != 4096 {
                return Err(Error::new("Wrong old section block count"));
            }
            if protocol >= 477 {
                body.i16(
                    section
                        .blocks
                        .iter()
                        .filter(|&&state| {
                            !crate::vfp_release_chain::air_states(protocol).contains(&state)
                        })
                        .count() as i16,
                );
            }
            PaletteFormat::blocks(protocol, width(step, true)).write(&mut body, &section.blocks)?;
            if protocol < 477 {
                if section.block_light.len() != 2048 {
                    return Err(Error::new("Missing inline old block light"));
                }
                body.bytes(&section.block_light);
                if sky {
                    let layer = section
                        .sky_light
                        .as_ref()
                        .ok_or_else(|| Error::new("Missing inline old sky light"))?;
                    if layer.len() != 2048 {
                        return Err(Error::new("Invalid inline sky layer length"));
                    }
                    body.bytes(layer);
                }
            }
        }
        if self.full && protocol < 573 {
            let biomes = self
                .biomes
                .as_ref()
                .ok_or_else(|| Error::new("Missing old 2D biome plane"))?;
            if biomes.len() != 256 {
                return Err(Error::new("Wrong old 2D biome count"));
            }
            for &biome in biomes {
                if protocol >= 393 {
                    body.i32(biome);
                } else {
                    body.u8(
                        u8::try_from(biome).map_err(|_| Error::new("Old biome ID exceeds byte"))?
                    );
                }
            }
        }
        w.byte_array(&body.data);
        if protocol >= 110 {
            w.varint(self.entities.len() as i32);
            for entity in &self.entities {
                w.vfp_named_nbt(entity)?;
            }
        }
        Ok(w.data)
    }
    pub fn make_heightmap(&mut self, motion_blocking: &BTreeSet<i32>, target: i32) -> Result<()> {
        let mut surface = vec![0; 256];
        let mut motion = vec![0; 256];
        for y in (0..256).rev() {
            let Some(section) = self.sections.get(&(y / 16)) else {
                continue;
            };
            for z in 0..16 {
                for x in 0..16 {
                    let state = section.blocks[((y % 16) << 8) | (z << 4) | x];
                    let index = (z << 4) | x;
                    if !crate::vfp_release_chain::air_states(target).contains(&state)
                        && surface[index] == 0
                    {
                        surface[index] = y as i32 + 1;
                    }
                    if motion_blocking.contains(&state) && motion[index] == 0 {
                        motion[index] = y as i32 + 1;
                    }
                }
            }
        }
        self.heightmap = Tag::compound([
            (
                "MOTION_BLOCKING",
                Tag::LongArray(codec::pack(&motion, 9, Packing::Spanning)?),
            ),
            (
                "WORLD_SURFACE",
                Tag::LongArray(codec::pack(&surface, 9, Packing::Spanning)?),
            ),
        ]);
        Ok(())
    }
    pub fn light(&self, step: &Step, sky: bool) -> Result<Packet> {
        if step.target != 477 {
            return Err(Error::new("Inline light split belongs to 1.13.2 -> 1.14"));
        }
        let mut w = Writer::new();
        w.varint(self.x);
        w.varint(self.z);
        let mut sky_mask = 0u32;
        let mut block_mask = 0u32;
        let mut empty_sky = 0u32;
        let mut empty_block = 0u32;
        let mut sky_layers = Vec::new();
        let mut block_layers = Vec::new();
        for y in 0..16 {
            let bit = 1 << (y + 1);
            if let Some(section) = self.sections.get(&y) {
                if section.block_light.len() != 2048 {
                    return Err(Error::new("Missing old block light during split"));
                }
                block_mask |= bit;
                block_layers.push(section.block_light.clone());
                if sky {
                    let layer = section
                        .sky_light
                        .as_ref()
                        .ok_or_else(|| Error::new("Missing old sky light during split"))?;
                    if layer.len() != 2048 {
                        return Err(Error::new("Invalid old sky light layer"));
                    }
                    sky_mask |= bit;
                    sky_layers.push(layer.clone());
                } else {
                    empty_sky |= bit;
                }
            } else if self.full {
                empty_block |= bit;
                if sky {
                    sky_mask |= bit;
                    sky_layers.push(vec![255; 2048]);
                } else {
                    empty_sky |= bit;
                }
            }
        }
        if self.full {
            empty_block |= 1 | (1 << 17);
            empty_sky |= 1 | (1 << 17);
        }
        for mask in [sky_mask, block_mask, empty_sky, empty_block] {
            w.varint(mask as i32);
        }
        for layer in sky_layers.into_iter().chain(block_layers) {
            w.byte_array(&layer);
        }
        step.incoming("LIGHT_UPDATE", w.data)
    }
}

#[derive(Default)]
pub struct Columns {
    columns: BTreeMap<(i32, i32), Column>,
}
impl Columns {
    pub fn clear(&mut self) {
        self.columns.clear();
    }
    pub fn forget(&mut self, x: i32, z: i32) {
        self.columns.remove(&(x, z));
    }
    pub fn set_block(&mut self, position: codec::Position, state: i32) {
        if !(0..256).contains(&position.y) {
            return;
        }
        let Some(column) = self.columns.get_mut(&(position.x >> 4, position.z >> 4)) else {
            return;
        };
        let section = column
            .sections
            .entry(position.y as usize / 16)
            .or_insert_with(|| OldSection {
                blocks: vec![0; 4096],
                block_light: Vec::new(),
                sky_light: None,
            });
        section.blocks[((position.y as usize & 15) << 8)
            | ((position.z as usize & 15) << 4)
            | (position.x as usize & 15)] = state;
    }
    pub fn merge(&mut self, mut column: Column) -> Result<Column> {
        if column.full {
            if self.columns.len() >= 2048 && !self.columns.contains_key(&(column.x, column.z)) {
                return Err(Error::new("Legacy partial-column cache exceeds bound"));
            }
            self.columns.insert((column.x, column.z), column.clone());
            return Ok(column);
        }
        let old = self
            .columns
            .get_mut(&(column.x, column.z))
            .ok_or_else(|| Error::new("Partial column arrived before full old column"))?;
        for (y, section) in std::mem::take(&mut column.sections) {
            old.sections.insert(y, section);
        }
        if !matches!(column.heightmap, Tag::End) {
            old.heightmap = column.heightmap;
        }
        for entity in column.entities {
            old.entities.retain(|value| {
                !(value.get("x") == entity.get("x")
                    && value.get("y") == entity.get("y")
                    && value.get("z") == entity.get("z"))
            });
            old.entities.push(entity);
        }
        Ok(old.clone())
    }
}

#[cfg(test)]
mod cache_regressions {
    use super::*;
    #[test]
    fn partial_column_preserves_block_edits_and_explosion_air_in_other_sections() {
        let column = |full, sections| Column {
            x: -1,
            z: 2,
            full,
            ignore_light: false,
            sections,
            biomes: None,
            heightmap: Tag::End,
            entities: Vec::new(),
        };
        let section = || OldSection {
            blocks: vec![1; 4096],
            block_light: Vec::new(),
            sky_light: None,
        };
        let mut cache = Columns::default();
        cache
            .merge(column(
                true,
                BTreeMap::from([(0, section()), (1, section())]),
            ))
            .unwrap();
        cache.set_block(codec::Position { x: -1, y: 4, z: 33 }, 0);
        cache.set_block(codec::Position { x: -2, y: 5, z: 34 }, 7);
        let merged = cache
            .merge(column(false, BTreeMap::from([(1, section())])))
            .unwrap();
        assert_eq!(merged.sections[&0].blocks[(4 << 8) | (1 << 4) | 15], 0);
        assert_eq!(merged.sections[&0].blocks[(5 << 8) | (2 << 4) | 14], 7);
        assert_eq!(merged.sections[&0].blocks[0], 1);
        cache.forget(-1, 2);
        assert!(cache.merge(column(false, BTreeMap::new())).is_err());
    }
}
