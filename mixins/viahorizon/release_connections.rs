// SPDX-License-Identifier: GPL-3.0-or-later
// Neighbor-dependent 1.13 states derived from the old server's block updates.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Result,
    buf::Writer,
    vfp_release_chain::Step,
    vfp_release_codec::Position,
    vfp_release_columns::Column,
    vfp_release_connection_data::{BLOCKS, OCCLUDING, PROFILE_IDS, PROFILES},
    vfp_wire::Packet,
};

pub struct Property {
    pub name: &'static str,
    pub values: &'static [&'static str],
}

pub struct Block {
    pub name: &'static str,
    pub start: u16,
    pub properties: &'static [Property],
}

fn block(state: u16) -> &'static Block {
    let index = BLOCKS.partition_point(|block| block.start <= state);
    &BLOCKS[index.saturating_sub(1)]
}

fn properties(state: u16) -> impl Iterator<Item = (&'static Property, usize, usize)> {
    let block = block(state);
    let mut offset = usize::from(state - block.start);
    let mut stride = 1;
    block.properties.iter().rev().map(move |property| {
        let size = property.values.len();
        let index = offset % size;
        let result = (property, stride, index);
        offset /= size;
        stride *= size;
        result
    })
}

fn value(state: u16, name: &str) -> &'static str {
    properties(state)
        .find_map(|(property, _, index)| (property.name == name).then_some(property.values[index]))
        .unwrap_or("")
}

fn set(state: u16, name: &str, new: &str) -> u16 {
    for (property, stride, old) in properties(state) {
        if property.name == name {
            if let Some(new) = property.values.iter().position(|&value| value == new) {
                return (usize::from(state) - old * stride + new * stride) as u16;
            }
            break;
        }
    }
    state
}

type Pos = (i32, i32, i32);
type Chunk = (i32, i32);
const DIRECTIONS: [&str; 6] = ["north", "south", "east", "west", "up", "down"];
const DELTAS: [Pos; 6] = [
    (0, 0, -1),
    (0, 0, 1),
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
];
const LEFT: [usize; 4] = [3, 2, 0, 1];

fn relative((x, y, z): Pos, face: usize) -> Pos {
    let (dx, dy, dz) = DELTAS[face];
    (x.wrapping_add(dx), y.wrapping_add(dy), z.wrapping_add(dz))
}

fn facing(state: u16) -> Option<usize> {
    DIRECTIONS[..4]
        .iter()
        .position(|&name| name == value(state, "facing"))
}

fn flag(state: u16, key: &str, on: bool) -> u16 {
    set(state, key, if on { "true" } else { "false" })
}

fn fence_type(name: &str) -> Option<usize> {
    if name == "nether_brick_fence" {
        Some(1)
    } else if name.ends_with("_fence") {
        Some(0)
    } else if name.ends_with("glass_pane") || name == "iron_bars" {
        Some(2)
    } else if matches!(name, "cobblestone_wall" | "mossy_cobblestone_wall") {
        Some(3)
    } else {
        None
    }
}

fn flammable(name: &str) -> bool {
    if matches!(
        name,
        "tnt" | "vine" | "bookshelf" | "hay_block" | "dead_bush"
    ) || name.ends_with("_wool")
        || name.ends_with("_carpet")
    {
        return true;
    }
    [
        "dark_oak_",
        "oak_",
        "spruce_",
        "birch_",
        "jungle_",
        "acacia_",
    ]
    .iter()
    .any(|&prefix| {
        matches!(
            name.strip_prefix(prefix),
            Some("slab" | "log" | "planks" | "leaves" | "fence" | "fence_gate" | "stairs")
        )
    })
}

fn plant(name: &str) -> bool {
    matches!(
        name,
        "rose_bush" | "sunflower" | "peony" | "tall_grass" | "large_fern" | "lilac"
    )
}

fn occluding(state: u16) -> bool {
    OCCLUDING.binary_search(&state).is_ok()
}

fn block_index((x, y, z): Pos) -> usize {
    (((y & 15) << 8) | ((z & 15) << 4) | (x & 15)) as usize
}

fn chunk((x, _, z): Pos) -> Chunk {
    (x >> 4, z >> 4)
}

fn section_pos((x, z): Chunk, section: usize, index: usize) -> Pos {
    (
        (x << 4) | (index & 15) as i32,
        (section * 16 + (index >> 8)) as i32,
        (z << 4) | ((index >> 4) & 15) as i32,
    )
}

#[derive(Default)]
pub struct Store {
    columns: BTreeMap<Chunk, BTreeMap<usize, Section>>,
    source: i32,
}

struct Section {
    base: Box<[u16]>,
    connected: BTreeMap<usize, u16>,
}

impl Store {
    pub fn source(&mut self, protocol: i32) {
        self.source = protocol;
    }

    pub fn clear(&mut self) {
        self.columns.clear();
    }

    pub fn forget(&mut self, x: i32, z: i32) {
        self.columns.remove(&(x, z));
    }

    fn section(&self, pos: Pos) -> Option<&Section> {
        self.columns
            .get(&chunk(pos))?
            .get(&usize::try_from(pos.1 >> 4).ok()?)
    }

    fn section_mut(&mut self, pos: Pos) -> Option<&mut Section> {
        self.columns
            .get_mut(&chunk(pos))?
            .get_mut(&usize::try_from(pos.1 >> 4).ok()?)
    }

    fn base(&self, pos: Pos) -> u16 {
        self.section(pos)
            .map_or(0, |section| section.base[block_index(pos)])
    }

    fn stair(&self, pos: Pos, state: u16) -> u16 {
        let Some(face) = facing(state) else {
            return state;
        };
        let half = value(state, "half");
        let neighbor = |direction| {
            let relative = self.base(relative(pos, direction));
            if block(relative).name.ends_with("_stairs") && value(relative, "half") == half {
                facing(relative)
            } else {
                None
            }
        };
        let different = |other: usize| (face < 2) != (other < 2);
        let shape = if let Some(other) =
            neighbor(face).filter(|&other| different(other) && neighbor(other ^ 1) != Some(face))
        {
            if other == LEFT[face] {
                "outer_left"
            } else {
                "outer_right"
            }
        } else if let Some(other) =
            neighbor(face ^ 1).filter(|&other| different(other) && neighbor(other) != Some(face))
        {
            if other == LEFT[face] {
                "inner_left"
            } else {
                "inner_right"
            }
        } else {
            "straight"
        };
        set(state, "shape", shape)
    }

    fn profile(&self, state: u16, face: usize, kind: usize) -> bool {
        let profile = PROFILES[usize::from(PROFILE_IDS[usize::from(state)])];
        if kind < 4 && self.source < 335 && profile[5] != 0 {
            return false;
        }
        profile[kind] & (1 << (face ^ 1)) != 0
    }

    fn connect(&self, pos: Pos, base: u16) -> u16 {
        let name = block(base).name;
        let adjacent = |direction| self.base(relative(pos, direction));
        let neighbor_name = |direction| block(adjacent(direction)).name;
        let mut out = base;
        if let Some(kind) = fence_type(name) {
            let mut mask = 0;
            for face in 0..4 {
                let target = relative(pos, face);
                let neighbor = self.base(target);
                let name_at = block(neighbor).name;
                let neighbor = if name_at.ends_with("_stairs") {
                    self.stair(target, neighbor)
                } else {
                    neighbor
                };
                if name_at == name || self.profile(neighbor, face, kind) {
                    mask |= 1 << face;
                }
            }
            if kind == 2 && self.source <= 47 && mask == 0 {
                mask = 15;
            }
            for (face, key) in DIRECTIONS[..4].iter().enumerate() {
                out = flag(out, key, mask & (1 << face) != 0);
            }
            if kind == 3 {
                out = flag(out, "up", !matches!(mask, 3 | 12) || adjacent(4) != 0);
            }
        } else if name.ends_with("_stairs") {
            out = self.stair(pos, base);
        } else if name.ends_with("_door") {
            let lower = value(base, "half") == "lower";
            let other = adjacent(if lower { 4 } else { 5 });
            if block(other).name == name && value(other, "half") != value(base, "half") {
                let keys = if lower {
                    ["hinge", "powered"]
                } else {
                    ["facing", "open"]
                };
                for key in keys {
                    out = set(out, key, value(other, key));
                }
            }
        } else if matches!(name, "chest" | "trapped_chest") {
            if let Some(face) = facing(base) {
                let left = LEFT[face];
                let right = left ^ 1;
                let other =
                    |side| neighbor_name(side) == name && facing(adjacent(side)) == Some(face);
                out = set(
                    base,
                    "type",
                    if other(left) {
                        "right"
                    } else if other(right) {
                        "left"
                    } else {
                        "single"
                    },
                );
            }
        } else if name == "redstone_wire" {
            for (face, key) in DIRECTIONS[..4].iter().enumerate() {
                let target = relative(pos, face);
                let neighbor = self.base(target);
                let wire = |state| block(state).name == "redstone_wire";
                let side = if wire(neighbor) || self.profile(neighbor, face, 4) {
                    "side"
                } else if wire(self.base(relative(target, 4))) && !occluding(adjacent(4)) {
                    "up"
                } else if wire(self.base(relative(target, 5))) && !occluding(neighbor) {
                    "side"
                } else {
                    "none"
                };
                out = set(out, key, side);
            }
        } else if name == "tripwire" {
            for (face, key) in DIRECTIONS[..4].iter().enumerate() {
                let state = adjacent(face);
                let neighbor = block(state).name;
                out = flag(
                    out,
                    key,
                    neighbor == "tripwire"
                        || (neighbor == "tripwire_hook" && facing(state) == Some(face ^ 1)),
                );
            }
        } else if name == "chorus_plant" {
            for (face, key) in DIRECTIONS.iter().enumerate() {
                let neighbor = neighbor_name(face);
                out = flag(
                    out,
                    key,
                    matches!(neighbor, "chorus_plant" | "chorus_flower")
                        || (face == 5 && neighbor == "end_stone"),
                );
            }
        } else if name == "fire" {
            for (face, key) in DIRECTIONS[..5].iter().enumerate() {
                out = flag(out, key, flammable(neighbor_name(face)));
            }
        } else if matches!(name, "grass_block" | "podzol" | "mycelium") {
            out = flag(
                base,
                "snowy",
                matches!(neighbor_name(4), "snow" | "snow_block"),
            );
        } else if plant(name) && value(base, "half") == "upper" {
            let lower = adjacent(5);
            if plant(block(lower).name) && value(lower, "half") == "lower" {
                out = set(lower, "half", "upper");
            }
        } else if matches!(name, "pumpkin_stem" | "melon_stem") && value(base, "age") == "7" {
            let (fruit, attached) = if name == "pumpkin_stem" {
                ("pumpkin", "attached_pumpkin_stem")
            } else {
                ("melon", "attached_melon_stem")
            };
            if let Some(face) = [2, 0, 1, 3]
                .into_iter()
                .find(|&face| neighbor_name(face) == fruit)
            {
                if let Some(kind) = BLOCKS.iter().find(|kind| kind.name == attached) {
                    out = set(kind.start, "facing", DIRECTIONS[face]);
                }
            }
        }
        out
    }

    fn refresh(&mut self, pos: Pos) -> Option<u16> {
        let index = block_index(pos);
        let section = self.section(pos)?;
        let base = section.base[index];
        let previous = section.connected.get(&index).copied().unwrap_or(base);
        let current = self.connect(pos, base);
        self.record(pos, base, current);
        (previous != current).then_some(current)
    }

    fn record(&mut self, pos: Pos, base: u16, current: u16) {
        let index = block_index(pos);
        if let Some(section) = self.section_mut(pos) {
            if base == current {
                section.connected.remove(&index);
            } else {
                section.connected.insert(index, current);
            }
        }
    }

    pub fn column(&mut self, step: &Step, column: &mut Column) -> Result<Vec<Packet>> {
        let coord = (column.x, column.z);
        if column.full {
            self.forget(coord.0, coord.1);
        }
        let stored = self.columns.entry(coord).or_default();
        for (&y, section) in &column.sections {
            stored.insert(
                y,
                Section {
                    base: section.blocks.iter().map(|&value| value as u16).collect(),
                    connected: BTreeMap::new(),
                },
            );
        }
        let present = column.sections.keys().copied().collect::<BTreeSet<_>>();
        for (&y, section) in &mut column.sections {
            for (index, state) in section.blocks.iter_mut().enumerate() {
                let pos = section_pos(coord, y, index);
                let base = *state as u16;
                let current = self.connect(pos, base);
                self.record(pos, base, current);
                *state = i32::from(current);
            }
        }
        let affected = if column.full {
            (0..16).collect()
        } else {
            present.clone()
        };
        let (x, z) = (coord.0 << 4, coord.1 << 4);
        let mut outside = BTreeSet::new();
        for section in affected {
            for dy in -2..18 {
                let y = (section * 16) as i32 + dy;
                if !(0..256).contains(&y) {
                    continue;
                }
                for dx in -2..18 {
                    for dz in -2..18 {
                        let inside = (0..16).contains(&dx) && (0..16).contains(&dz);
                        if !inside || (!column.full && !present.contains(&((y >> 4) as usize))) {
                            let pos = (x.wrapping_add(dx), y, z.wrapping_add(dz));
                            if self.section(pos).is_some() {
                                outside.insert(pos);
                            }
                        }
                    }
                }
            }
        }
        let changes = outside
            .into_iter()
            .filter_map(|pos| self.refresh(pos).map(|state| (pos, state)))
            .collect();
        packets(step, changes)
    }

    pub fn update(&mut self, step: &Step, incoming: Vec<(Position, i32)>) -> Result<Vec<Packet>> {
        let mut changed = BTreeMap::new();
        let mut affected = BTreeSet::new();
        for (position, state) in incoming {
            let pos = (position.x, position.y, position.z);
            let state = state as u16;
            changed.insert(pos, state);
            if (0..256).contains(&position.y) {
                if let Some(column) = self.columns.get_mut(&chunk(pos)) {
                    let section =
                        column
                            .entry((position.y >> 4) as usize)
                            .or_insert_with(|| Section {
                                base: vec![0; 4096].into_boxed_slice(),
                                connected: BTreeMap::new(),
                            });
                    section.base[block_index(pos)] = state;
                }
            }
            // Redstone and stair support can depend on a diagonal.
            for dx in -2_i32..=2 {
                for dy in -2_i32..=2 {
                    for dz in -2_i32..=2 {
                        if dx.abs() + dy.abs() + dz.abs() <= 2 {
                            affected.insert((
                                pos.0.wrapping_add(dx),
                                pos.1.wrapping_add(dy),
                                pos.2.wrapping_add(dz),
                            ));
                        }
                    }
                }
            }
        }
        let mut result = Vec::new();
        for pos in affected {
            if let Some(&state) = changed.get(&pos) {
                let current = self.connect(pos, state);
                self.record(pos, state, current);
                result.push((pos, current));
            } else if let Some(state) = self.refresh(pos) {
                result.push((pos, state));
            }
        }
        packets(step, result)
    }
}

fn packets(step: &Step, changes: Vec<(Pos, u16)>) -> Result<Vec<Packet>> {
    let mut by_chunk: BTreeMap<Chunk, Vec<(Pos, u16)>> = BTreeMap::new();
    for (pos, state) in changes {
        by_chunk.entry(chunk(pos)).or_default().push((pos, state));
    }
    let mut packets = Vec::new();
    for ((x, z), changes) in by_chunk {
        if changes.len() == 1 || changes.iter().any(|&((_, y, _), _)| !(0..256).contains(&y)) {
            for ((x, y, z), state) in changes {
                let mut w = Writer::new();
                w.i64(Position { x, y, z }.pack(step.target)?);
                w.varint(i32::from(state));
                packets.push(step.incoming("BLOCK_UPDATE", w.data)?);
            }
            continue;
        }
        for batch in changes.chunks(65536) {
            let mut w = Writer::new();
            w.i32(x);
            w.i32(z);
            w.varint(batch.len() as i32);
            for &((x, y, z), state) in batch {
                w.u16((((x & 15) << 12) | ((z & 15) << 8) | y) as u16);
                w.varint(i32::from(state));
            }
            packets.push(step.incoming("CHUNK_BLOCKS_UPDATE", w.data)?);
        }
    }
    Ok(packets)
}
