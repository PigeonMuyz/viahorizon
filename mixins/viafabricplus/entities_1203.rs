// SPDX-License-Identifier: GPL-3.0-or-later
// Item bodies become native here. Particle/entity/block ids remain original
// until the later typed codecs, as declared by Maps::NATIVE_ITEMS.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
    vfp_tables_261_native as native, vfp_tables_1205 as next,
    vfp_wire::{self as wire, Packet},
};
fn particle_id<M: Maps>(name: &str) -> Result<i32> {
    let native = native::PARTICLE_NAMES
        .iter()
        .position(|value| *value == name)
        .ok_or_else(|| Error::new("Missing native particle name"))?;
    M::PARTICLES.inverse(native as i32).map_err(Error::new)
}
fn item_template<M: Maps>(r: &mut Reader<'_>, w: &mut Writer, registry: &Registries) -> Result<()> {
    let mut item = Writer::new();
    crate::vfp_items_1203::forward::<M>(r, &mut item, registry)?;
    let mut item = Reader::new(&item.data);
    let count = item.varint()?;
    if count == 0 {
        w.varint(M::ITEMS.map(1).map_err(Error::new)?);
        w.varint(1);
        w.varint(0);
        w.varint(0);
    } else {
        let id = item.varint()?;
        w.varint(id);
        w.varint(count);
        w.bytes(item.rest());
    }
    Ok(())
}
pub fn particle<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    color: Option<i32>,
    registry: &Registries,
) -> Result<()> {
    let id = r.varint()?;
    let mapped = M::PARTICLES.map(id).map_err(Error::new)?;
    w.varint(id);
    let name = native::PARTICLE_NAMES
        .get(mapped as usize)
        .copied()
        .ok_or_else(|| Error::new("Invalid mapped legacy particle"))?;
    match name {
        "block" | "block_marker" | "falling_dust" => w.varint(r.varint()?),
        "dust" => w.bytes(r.bytes(16)?),
        "dust_color_transition" => {
            let first = r.bytes(12)?.to_vec();
            let scale = r.f32()?;
            let second = r.bytes(12)?.to_vec();
            w.bytes(&first);
            w.bytes(&second);
            w.f32(scale);
        }
        "entity_effect" => {
            w.i32(color.map_or(0, |color| 0xff000000u32 as i32 | (color & 0xffffff)))
        }
        "item" => item_template::<M>(r, w, registry)?,
        "vibration" => {
            let source = if M::PROTOCOL < 765 {
                let kind = r.string(32767)?;
                match kind.strip_prefix("minecraft:").unwrap_or(&kind) {
                    "block" => 0,
                    "entity" => 1,
                    _ => return Err(Error::new("Unknown legacy vibration source name")),
                }
            } else {
                r.varint()?
            };
            w.varint(source);
            match source {
                0 => w.bytes(r.bytes(8)?),
                1 => {
                    w.varint(r.varint()?);
                    w.f32(r.f32()?);
                }
                _ => return Err(Error::new("Unknown legacy vibration source")),
            }
            w.varint(r.varint()?);
        }
        "sculk_charge" => w.f32(r.f32()?),
        "shriek" => w.varint(r.varint()?),
        _ => (),
    }
    Ok(())
}
pub fn level_particle<M: Maps>(data: &[u8], registry: &Registries) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let id = r.varint()?;
    let head_start = r.position();
    r.bool()?;
    r.bytes(24)?;
    let offsets = [r.f32()?, r.f32()?, r.f32()?];
    let speed = r.f32()?;
    let count = r.i32()?;
    let head = r.since(head_start).to_vec();
    let mapped = M::PARTICLES.map(id).map_err(Error::new)?;
    let color = if native::PARTICLE_NAMES.get(mapped as usize) == Some(&"entity_effect") {
        Some(if speed == 0.0 {
            0
        } else if count == 0 {
            let rgb = offsets.map(|n| (n * 255.0).round() as i32);
            (rgb[0] << 16) | (rgb[1] << 8) | rgb[2]
        } else {
            crate::vfp_transition_1214::random_color()
        })
    } else {
        None
    };
    let mut source = Writer::new();
    source.varint(id);
    source.bytes(r.rest());
    let mut w = Writer::new();
    w.bytes(&head);
    let mut particle = Reader::new(&source.data);
    self::particle::<M>(&mut particle, &mut w, color, registry)?;
    if particle.remaining() != 0 {
        return Err(Error::new("Trailing old level particle"));
    }
    Ok(w.data)
}
pub fn metadata<M: Maps>(
    data: &[u8],
    families: u8,
    previous_cloud_color: Option<i32>,
    first: bool,
    registry: &Registries,
) -> Result<(Vec<Packet>, Option<i32>)> {
    let mut r = Reader::new(data);
    let entity = r.varint()?;
    let mut fields = Vec::new();
    let mut seen = [false; 255];
    let mut cloud_color = None;
    for _ in 0..256 {
        let index = r.u8()?;
        if index == 255 {
            if r.remaining() != 0 {
                return Err(Error::new("Trailing legacy metadata"));
            }
            break;
        }
        if seen[index as usize] {
            return Err(Error::new("Duplicate legacy metadata index"));
        }
        seen[index as usize] = true;
        let source = r.varint()?;
        if !(0..=if M::PROTOCOL < 762 { 23 } else { 27 }).contains(&source) {
            return Err(Error::new("Unknown 1.20.3 metadata serializer"));
        }
        let source = if M::PROTOCOL < 762 && source >= 14 {
            source + 1
        } else {
            source
        };
        let mut target = source;
        if target >= 18 {
            target += 1;
        }
        if target >= 23 {
            target += 1;
        }
        if target >= 28 {
            target += 1;
        }
        let start = r.position();
        let value = if M::PROTOCOL < 764 && source == 16 {
            let mut tag = Writer::new();
            tag.nbt(&r.vfp_named_nbt()?);
            tag.data
        } else if M::PROTOCOL < 765 && matches!(source, 5 | 6) {
            let mut text = Writer::new();
            let present = source == 5 || r.bool()?;
            if source == 6 {
                text.bool(present);
            }
            if present {
                text.nbt(&crate::vfp_text_1214::json_text(&r.string(262144)?, 0)?);
            }
            text.data
        } else if source == 7 {
            let mut item = Writer::new();
            crate::vfp_items_1203::forward::<M>(&mut r, &mut item, registry)?;
            item.data
        } else if source == 17 {
            let mut particle = Writer::new();
            self::particle::<M>(&mut r, &mut particle, None, registry)?;
            particle.data
        } else {
            crate::entity_data::vfp_entities::skip_121_value::<M>(target, &mut r, registry)?;
            r.since(start).to_vec()
        };
        if families & 32 != 0 && index == 9 {
            if source != 1 {
                return Err(Error::new("Invalid cloud color metadata"));
            }
            cloud_color = Some(Reader::new(&value).varint()?);
        }
        fields.push((index, target, value));
    }
    let has_cloud_particle = fields
        .iter()
        .any(|(index, kind, _)| *index == 11 && *kind == 17);
    if M::PROTOCOL < 763 && families & 128 != 0 {
        if first && !fields.iter().any(|(index, _, _)| *index == 12) {
            let mut value = Writer::new();
            for n in [0.0, 1.0, 0.0, 0.0] {
                value.f32(n);
            }
            fields.push((12, 30, value.data));
        } else {
            for (index, kind, value) in &mut fields {
                if *index == 12 {
                    if *kind != 30 {
                        return Err(Error::new("Invalid old item-display rotation"));
                    }
                    let mut r = Reader::new(value);
                    let [x, y, z, w] = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
                    let mut rotated = Writer::new();
                    for n in [-z, w, x, -y] {
                        rotated.f32(n);
                    }
                    *value = rotated.data;
                }
            }
        }
    }
    let color = cloud_color.or(previous_cloud_color);
    let mut w = Writer::new();
    w.varint(entity);
    let mut extra = Vec::new();
    for (mut index, mut kind, mut value) in fields {
        if M::PROTOCOL < 762 && families & 2 != 0 {
            if index == 18 {
                continue;
            }
            if index > 18 {
                index -= 1;
            }
        }
        if M::PROTOCOL < 764 && families & 64 != 0 && index >= 10 {
            index = index
                .checked_add(1)
                .filter(|index| *index < 255)
                .ok_or_else(|| Error::new("Display metadata index overflows"))?;
        }
        if families & 1 != 0 && index == 10 {
            if kind != 1 {
                return Err(Error::new("Invalid living effect color"));
            }
            let color = Reader::new(&value).varint()?;
            let mut particles = Writer::new();
            particles.varint(i32::from(color != 0));
            if color != 0 {
                particles.varint(particle_id::<M>("entity_effect")?);
                particles.i32(0xff000000u32 as i32 | (color & 0xffffff));
            }
            kind = 18;
            value = particles.data;
        }
        if families & 4 != 0 {
            if index == 20 {
                if kind != 1 {
                    return Err(Error::new("Invalid llama carpet metadata"));
                }
                let color = Reader::new(&value).varint()?;
                let colors = [
                    "white",
                    "orange",
                    "magenta",
                    "light_blue",
                    "yellow",
                    "lime",
                    "pink",
                    "gray",
                    "light_gray",
                    "cyan",
                    "purple",
                    "blue",
                    "brown",
                    "green",
                    "red",
                    "black",
                ];
                let mut equipment = Writer::new();
                equipment.varint(entity);
                equipment.u8(6);
                if let Some(color) = usize::try_from(color)
                    .ok()
                    .and_then(|index| colors.get(index))
                {
                    let name = format!("{color}_carpet");
                    let id = M::ITEM_IDENTIFIERS
                        .iter()
                        .position(|value| *value == name)
                        .ok_or_else(|| Error::new("Missing legacy carpet item"))?;
                    equipment.varint(1);
                    equipment.varint(M::ITEMS.map(id as i32).map_err(Error::new)?);
                    equipment.varint(0);
                    equipment.varint(0);
                } else {
                    equipment.varint(0);
                }
                extra.push((
                    wire::packet_id(next::CLIENTBOUND, "SET_EQUIPMENT").map_err(Error::new)?,
                    equipment.data,
                ));
                continue;
            } else if index > 20 {
                index -= 1;
            }
        }
        if families & 32 != 0 {
            if index == 9 {
                continue;
            }
            if index == 11 && kind == 17 && color.is_some() {
                let mut p = Reader::new(&value);
                if p.varint()? == particle_id::<M>("entity_effect")? {
                    let mut particle = Writer::new();
                    particle.varint(particle_id::<M>("entity_effect")?);
                    particle.i32(0xff000000u32 as i32 | (color.unwrap_or(0) & 0xffffff));
                    value = particle.data;
                }
            }
            if index > 9 {
                index -= 1;
            }
        }
        if families & 16 != 0 && index == 10 {
            if kind != 1 {
                return Err(Error::new("Invalid arrow color metadata"));
            }
            let color = Reader::new(&value).varint()?;
            if color != -1 {
                let mut v = Writer::new();
                v.varint(0xff000000u32 as i32 | (color & 0xffffff));
                value = v.data;
            }
        }
        if families & 8 != 0 && index == 8 && kind == 7 && Reader::new(&value).varint()? == 0 {
            continue;
        }
        w.u8(index);
        w.varint(kind);
        w.bytes(&value);
    }
    if families & 32 != 0 && !has_cloud_particle && cloud_color.is_some_and(|color| color != 0) {
        w.u8(10);
        w.varint(17);
        w.varint(particle_id::<M>("entity_effect")?);
        w.i32(0xff000000u32 as i32 | (cloud_color.unwrap_or(0) & 0xffffff));
    }
    w.u8(255);
    let mut packets = vec![(
        wire::packet_id(next::CLIENTBOUND, "SET_ENTITY_DATA").map_err(Error::new)?,
        w.data,
    )];
    packets.extend(extra);
    Ok((packets, cloud_color))
}
