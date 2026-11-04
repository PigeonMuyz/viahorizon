// SPDX-License-Identifier: GPL-3.0-or-later
// Typed old metadata values, independent of entity-specific index changes.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::{self as chain, Step},
    vfp_release_codec::{self as codec, Position, Stack},
    vfp_release_rewrite as rewrite,
};
use horizon_world::nbt::Tag;

#[derive(Clone, Debug)]
pub struct Entry {
    pub index: u8,
    pub kind: i32,
    pub value: Value,
}
#[derive(Clone, Debug)]
pub enum Value {
    Int(i32),
    Long(i64),
    Float(f32),
    Text(String),
    OptionalText(Option<String>),
    Item(Option<Stack>),
    Bool(bool),
    Rotation([f32; 3]),
    Position(Position),
    OptionalPosition(Option<Position>),
    Uuid(Option<u128>),
    Nbt(Tag),
    Particle(Vec<u8>),
    Villager([i32; 3]),
    GlobalPosition(Option<(String, Position)>),
}
impl Entry {
    pub fn int(index: u8, kind: i32, value: i32) -> Self {
        Self {
            index,
            kind,
            value: Value::Int(value),
        }
    }
    pub fn integer(&self) -> Result<i32> {
        match self.value {
            Value::Int(value) => Ok(value),
            Value::Bool(value) => Ok(i32::from(value)),
            _ => Err(Error::new("Old metadata value is not an integer")),
        }
    }
    pub fn remap(&mut self, step: &Step) -> Result<()> {
        let normalized = normalized(step.source, self.kind)?;
        match &mut self.value {
            Value::Item(stack) => {
                rewrite::stack(step, false, stack)?;
            }
            Value::Int(value) if normalized == 13 && *value != 0 => {
                *value = if step.source == 340 {
                    crate::vfp_release_flatten::state_id(
                        step,
                        crate::vfp_release_flatten::combined_state(*value),
                    )?
                } else {
                    step.same_or_map("blockstates", *value, false)?
                };
            }
            Value::Particle(data) => {
                let mut r = Reader::new(data);
                let mut w = Writer::new();
                rewrite::particle(step, &mut r, &mut w, false)?;
                codec::end(&r)?;
                *data = w.data;
            }
            _ => (),
        }
        if step.source >= 107 && step.source < 393 && step.target >= 393 && self.kind >= 5 {
            self.kind += 1;
        }
        Ok(())
    }
}
fn normalized(protocol: i32, kind: i32) -> Result<i32> {
    if protocol < 107 {
        return [0, 100, 101, 2, 3, 6, 9, 8]
            .get(usize::try_from(kind).unwrap_or(usize::MAX))
            .copied()
            .ok_or_else(|| Error::new("Invalid packed metadata type"));
    }
    if protocol >= 761 {
        if kind == 2 {
            return Ok(102);
        }
        if kind >= 3 {
            return Ok(kind - 1);
        }
    }
    if protocol < 393 && kind >= 5 {
        Ok(kind + 1)
    } else {
        Ok(kind)
    }
}
fn position(r: &mut Reader<'_>, protocol: i32) -> Result<Position> {
    if protocol < 107 {
        Ok(Position {
            x: r.i32()?,
            y: r.i32()?,
            z: r.i32()?,
        })
    } else {
        Ok(Position::unpack(r.i64()?, protocol))
    }
}
fn put_position(w: &mut Writer, value: Position, protocol: i32) -> Result<()> {
    if protocol < 107 {
        w.i32(value.x);
        w.i32(value.y);
        w.i32(value.z);
    } else {
        w.i64(value.pack(protocol)?);
    }
    Ok(())
}
fn particle(r: &mut Reader<'_>, protocol: i32) -> Result<Vec<u8>> {
    let start = r.position();
    let id = r.varint()?;
    match chain::identifier(protocol, "particles", id)? {
        "block" | "block_marker" | "falling_dust" | "shriek" => {
            r.varint()?;
        }
        "dust" => {
            r.bytes(16)?;
        }
        "dust_color_transition" => {
            r.bytes(28)?;
        }
        "item" => {
            Stack::read(r, protocol)?;
        }
        "sculk_charge" => {
            r.f32()?;
        }
        "vibration" => {
            if protocol < 759 {
                r.i64()?;
            }
            let kind = r.string(32767)?;
            match kind.strip_prefix("minecraft:").unwrap_or(&kind) {
                "block" => {
                    r.i64()?;
                }
                "entity" => {
                    r.varint()?;
                    if protocol >= 759 {
                        r.f32()?;
                    }
                }
                _ => return Err(Error::new("Unknown metadata vibration destination")),
            }
            r.varint()?;
        }
        _ => (),
    }
    Ok(r.since(start).to_vec())
}
pub fn read(r: &mut Reader<'_>, protocol: i32) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    loop {
        let header = r.u8()?;
        if (protocol < 107 && header == 127) || (protocol >= 107 && header == 255) {
            break;
        }
        if entries.len() >= 255 {
            return Err(Error::new("Too many old metadata entries"));
        }
        let (index, kind) = if protocol < 107 {
            (header & 31, i32::from(header >> 5))
        } else {
            (header, r.varint()?)
        };
        let max = if protocol < 107 {
            7
        } else if protocol < 335 {
            12
        } else if protocol < 393 {
            13
        } else if protocol < 477 {
            15
        } else if protocol < 759 {
            18
        } else if protocol < 761 {
            22
        } else {
            23
        };
        if !(0..=max).contains(&kind) {
            return Err(Error::new(
                "Invalid metadata serializer for the source version",
            ));
        }
        let value = match normalized(protocol, kind)? {
            0 => Value::Int(i32::from(r.i8()?)),
            100 => Value::Int(i32::from(r.i16()?)),
            101 => Value::Int(r.i32()?),
            1 | 11 | 13 | 17 | 18 | 19 | 20 | 22 => Value::Int(r.varint()?),
            102 => Value::Long(r.varlong()?),
            2 => Value::Float(r.f32()?),
            3 | 4 => Value::Text(r.string(262144)?),
            5 => Value::OptionalText(if codec::boolean(r)? {
                Some(r.string(262144)?)
            } else {
                None
            }),
            6 => Value::Item(Stack::read(r, protocol)?),
            7 => Value::Bool(codec::boolean(r)?),
            8 => Value::Rotation([r.f32()?, r.f32()?, r.f32()?]),
            9 => Value::Position(position(r, protocol)?),
            10 => Value::OptionalPosition(if codec::boolean(r)? {
                Some(position(r, protocol)?)
            } else {
                None
            }),
            12 => Value::Uuid(if codec::boolean(r)? {
                Some(r.uuid()?)
            } else {
                None
            }),
            14 => Value::Nbt(r.vfp_named_nbt()?),
            15 => Value::Particle(particle(r, protocol)?),
            16 => Value::Villager([r.varint()?, r.varint()?, r.varint()?]),
            21 => Value::GlobalPosition(if codec::boolean(r)? {
                Some((r.string(32767)?, position(r, protocol)?))
            } else {
                None
            }),
            _ => return Err(Error::new("Unconverted metadata serializer")),
        };
        entries.push(Entry { index, kind, value });
    }
    Ok(entries)
}
pub fn write(w: &mut Writer, entries: &[Entry], protocol: i32) -> Result<()> {
    if entries.len() > 255 {
        return Err(Error::new("Too many outgoing metadata entries"));
    }
    for entry in entries {
        if protocol < 107 {
            if entry.index > 31 || !(0..=7).contains(&entry.kind) {
                return Err(Error::new("Metadata cannot fit the packed old format"));
            }
            let header = entry.index | (entry.kind as u8) << 5;
            if header == 127 {
                return Err(Error::new("Packed metadata entry collides with terminator"));
            }
            w.u8(header);
        } else {
            if entry.index == 255 {
                return Err(Error::new("Metadata index collides with terminator"));
            }
            w.u8(entry.index);
            w.varint(entry.kind);
        }
        match (normalized(protocol, entry.kind)?, &entry.value) {
            (0, Value::Int(value)) => {
                w.i8(*value as i8);
            }
            (100, Value::Int(value)) => {
                w.i16(*value as i16);
            }
            (101, Value::Int(value)) => {
                w.i32(*value);
            }
            (1 | 11 | 13 | 17 | 18 | 19 | 20 | 22, Value::Int(value)) => {
                w.varint(*value);
            }
            (102, Value::Long(value)) => {
                w.varlong(*value);
            }
            (2, Value::Float(value)) => {
                w.f32(*value);
            }
            (3 | 4, Value::Text(value)) => {
                w.string(value);
            }
            (5, Value::OptionalText(value)) => {
                w.bool(value.is_some());
                if let Some(value) = value {
                    w.string(value);
                }
            }
            (6, Value::Item(value)) => {
                Stack::write(value.as_ref(), w, protocol)?;
            }
            (7, Value::Bool(value)) => {
                w.bool(*value);
            }
            (8, Value::Rotation(value)) => {
                for &value in value {
                    w.f32(value);
                }
            }
            (9, Value::Position(value)) => {
                put_position(w, *value, protocol)?;
            }
            (10, Value::OptionalPosition(value)) => {
                w.bool(value.is_some());
                if let Some(value) = value {
                    put_position(w, *value, protocol)?;
                }
            }
            (12, Value::Uuid(value)) => {
                w.bool(value.is_some());
                if let Some(value) = value {
                    w.bytes(&value.to_be_bytes());
                }
            }
            (14, Value::Nbt(value)) => {
                w.vfp_named_nbt(value)?;
            }
            (15, Value::Particle(value)) => {
                w.bytes(value);
            }
            (16, Value::Villager(value)) => {
                for &value in value {
                    w.varint(value);
                }
            }
            (21, Value::GlobalPosition(value)) => {
                w.bool(value.is_some());
                if let Some((world, value)) = value {
                    w.string(world);
                    put_position(w, *value, protocol)?;
                }
            }
            _ => {
                return Err(Error::new(
                    "Metadata value does not match its target serializer",
                ));
            }
        }
    }
    w.u8(if protocol < 107 { 127 } else { 255 });
    Ok(())
}
