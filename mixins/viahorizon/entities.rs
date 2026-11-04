use crate::vfp_mapping::{Maps, V262};
// SPDX-License-Identifier: GPL-3.0-or-later
// This module is installed under entity_data, so the original value codecs can
// be reused without making Horizon's internal readers public.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_tables as t, vfp_wire as wire,
};
fn copy(r: &mut Reader<'_>, w: &mut Writer, n: usize) -> Result<()> {
    w.bytes(r.bytes(n)?);
    Ok(())
}
// Decode the metadata profile layout and preserve its skin overrides in the
// item representation. The item adapter retains the complete original value
// for lossless serverbound restoration.
pub fn item_profile_262(r: &mut Reader<'_>, w: &mut Writer, forward: bool) -> Result<()> {
    use crate::item::{Component, Profile, SkinPatch, component};
    let value = if forward {
        let p = super::read_profile(r)?;
        Component::Profile(Profile {
            name: p.name,
            id: p.uuid,
            skin: SkinPatch {
                body: p.body,
                cape: p.cape,
                elytra: p.elytra,
                slim: p.slim,
            },
            properties: p
                .properties
                .into_iter()
                .map(|p| (p.name, p.value, p.signature))
                .collect(),
        })
    } else {
        let start = r.position();
        super::read_profile(r)?;
        w.bytes(r.since(start));
        return Ok(());
    };
    value.write(component::PROFILE, w);
    Ok(())
}
fn var(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.varint(r.varint()?);
    Ok(())
}
pub fn item_profile_legacy(r: &mut Reader<'_>, w: &mut Writer, forward: bool) -> Result<()> {
    use crate::buf::Codec;
    if forward {
        let start = r.position();
        if r.bool()? {
            r.string(16)?;
        }
        if r.bool()? {
            r.uuid()?;
        }
        for _ in 0..r.count(1024)? {
            crate::packets::login::Property::read(r)?;
        }
        w.bool(false);
        w.bytes(r.since(start));
        w.bytes(&[0; 4]);
    } else {
        let profile = super::read_profile(r)?;
        w.bool(profile.name.is_some());
        if let Some(name) = profile.name {
            w.string(&name);
        }
        w.bool(profile.uuid.is_some());
        if let Some(uuid) = profile.uuid {
            w.uuid(uuid);
        }
        w.varint(profile.properties.len() as i32);
        for property in profile.properties {
            crate::packets::login::Property::write(&property, w);
        }
    }
    Ok(())
}
pub fn particle_using<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let id = M::PARTICLES.map(r.varint()?).map_err(Error::new)?;
    w.varint(id);
    if M::PROTOCOL < 768 {
        let name = crate::vfp_tables_261_native::PARTICLE_NAMES
            .get(id as usize)
            .copied();
        if matches!(name, Some("dust" | "dust_color_transition")) {
            let mut color = || -> Result<i32> {
                let red = (r.f32()? * 255.0) as i32;
                let green = (r.f32()? * 255.0) as i32;
                let blue = (r.f32()? * 255.0) as i32;
                Ok((255i32 << 24) | (red << 16) | (green << 8) | blue)
            };
            w.i32(color()?);
            if name == Some("dust_color_transition") {
                w.i32(color()?);
            }
            w.f32(r.f32()?);
            return Ok(());
        }
    }
    if M::PROTOCOL < 773 {
        match crate::vfp_tables_261_native::PARTICLE_NAMES
            .get(id as usize)
            .copied()
        {
            Some("dragon_breath") => {
                w.f32(1.0);
                return Ok(());
            }
            Some("flash") => {
                w.i32(-1);
                return Ok(());
            }
            Some("effect" | "instant_effect") => {
                w.i32(-1);
                w.f32(1.0);
                return Ok(());
            }
            _ => (),
        }
    }
    match id {
        1 | 2 | 36 | 121 | 125 => {
            w.varint(M::BLOCKSTATES.map(r.varint()?).map_err(Error::new)?);
        }
        7 | 10 | 15 | 28 | 46 | 48 | 52 => copy(r, w, 4)?,
        8 | 9 | 21 | 23 | 56 => copy(r, w, 8)?,
        22 => copy(r, w, 12)?,
        115 => var(r, w)?,
        59 => {
            copy(r, w, 28)?;
            if M::PROTOCOL < 769 {
                w.varint(crate::vfp_transition_1214::trail_duration());
            } else {
                var(r, w)?;
            }
        }
        57 => crate::vfp_items::template_using::<M>(r, w, true, _registry)?,
        58 => {
            let source = r.varint()?;
            w.varint(source);
            match source {
                0 => copy(r, w, 8)?,
                1 => {
                    var(r, w)?;
                    copy(r, w, 4)?;
                }
                _ => return Err(Error::new("Unknown vibration source")),
            };
            var(r, w)?;
        }
        _ => (),
    }
    Ok(())
}
fn metadata<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let mut seen = [false; 255];
    for _ in 0..256 {
        let index = r.u8()?;
        w.u8(index);
        if index == 255 {
            return Ok(());
        }
        if seen[index as usize] {
            return Err(Error::new("Duplicate entity metadata index"));
        }
        seen[index as usize] = true;
        let kind = r.varint()?;
        if !(0..=42).contains(&kind) {
            return Err(Error::new("Unknown 26.2 metadata serializer"));
        }
        w.varint(kind);
        match kind {
            5 | 6 => {
                let present = kind == 5 || r.bool()?;
                if kind == 6 {
                    w.bool(present);
                }
                if present {
                    let mut tag = r.nbt()?;
                    crate::vfp_play::text_version(&mut tag, M::PROTOCOL)?;
                    w.nbt(&tag);
                }
            }
            7 => crate::vfp_items::stack_using::<M>(r, w, true, false, _registry)?,
            14 => w.varint(M::BLOCKSTATES.map(r.varint()?).map_err(Error::new)?),
            15 => {
                let old = r.varint()?;
                if old < 0 {
                    return Err(Error::new("Negative optional block state"));
                }
                w.varint(if old == 0 {
                    0
                } else {
                    M::BLOCKSTATES.map(old).map_err(Error::new)?
                });
            }
            16 => particle_using::<M>(r, w, _registry)?,
            17 => {
                let n = r.count(65536)?;
                w.varint(n as i32);
                for _ in 0..n {
                    particle_using::<M>(r, w, _registry)?;
                }
            }
            33 => {
                let present = r.bool()?;
                w.bool(present);
                if present {
                    w.string(&r.string(32767)?);
                    copy(r, w, 8)?;
                }
            }
            34 => {
                let id = r.varint()?;
                if id < 0 {
                    return Err(Error::new("Negative painting holder"));
                }
                w.varint(id);
                if id == 0 {
                    var(r, w)?;
                    var(r, w)?;
                    w.string(&r.string(32767)?);
                    for _ in 0..2 {
                        let present = r.bool()?;
                        w.bool(present);
                        if present {
                            let mut text = r.nbt()?;
                            crate::vfp_play::text_version(&mut text, M::PROTOCOL)?;
                            w.nbt(&text);
                        }
                    }
                }
            }
            _ => {
                // Validate unchanged values without serializing them again. A
                // partial profile may contain both a name and UUID; the native
                // writer would turn it into a full (already resolved) profile.
                let start = r.position();
                super::read_value(kind, r)?
                    .ok_or_else(|| Error::new("Missing metadata value codec"))?;
                w.bytes(r.since(start));
            }
        }
    }
    Err(Error::new("Entity metadata has no terminator"))
}
fn spawn(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    var(r, w)?;
    w.string(&r.string(32767)?);
    w.i64(r.i64()?);
    let mode = r.i8()?;
    let previous = r.i8()?;
    if !(0..=3).contains(&mode) || !(-1..=3).contains(&previous) {
        return Err(Error::new("Invalid spawn game mode"));
    }
    w.varint(mode as i32);
    w.varint(previous as i32 + 1);
    w.bool(r.bool()?);
    w.bool(r.bool()?);
    let death = r.bool()?;
    w.bool(death);
    if death {
        w.string(&r.string(32767)?);
        copy(r, w, 8)?;
    }
    var(r, w)?;
    var(r, w)?;
    Ok(())
}
pub fn metadata_261(
    entity: i32,
    payload: &[u8],
    cube: bool,
    target_index: Option<u8>,
    fake: i32,
) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.varint(entity);
    for _ in 0..256 {
        let index = r.u8()?;
        if index == 255 {
            w.u8(255);
            if r.remaining() != 0 {
                return Err(Error::new("Trailing metadata"));
            }
            return Ok(w.data);
        }
        let mapped = if cube && index >= 16 {
            index
                .checked_add(2)
                .filter(|&v| v < 255)
                .ok_or_else(|| Error::new("Shifted metadata index overflow"))?
        } else {
            index
        };
        w.u8(mapped);
        let kind = r.varint()?;
        w.varint(kind);
        if target_index == Some(index) && kind == 19 {
            let value = r.varint()?;
            w.varint(if value == 1 { fake + 1 } else { value });
            continue;
        }
        let start = r.position();
        match kind {
            33 => {
                if r.bool()? {
                    r.string(32767)?;
                    r.bytes(8)?;
                }
            }
            34 => {
                if r.varint()? == 0 {
                    r.varint()?;
                    r.varint()?;
                    r.string(32767)?;
                    for _ in 0..2 {
                        if r.bool()? {
                            r.nbt()?;
                        }
                    }
                }
            }
            _ => {
                super::read_value(kind, &mut r)?
                    .ok_or_else(|| Error::new("Missing metadata codec"))?;
            }
        }
        w.bytes(r.since(start));
    }
    Err(Error::new("Metadata has no terminator"))
}
pub fn prepare_metadata_12111_using<M: Maps>(
    payload: &[u8],
    flags: u8,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.varint(r.varint()?);
    let mut seen = [false; 255];
    for _ in 0..256 {
        let old = r.u8()?;
        if old == 255 {
            if r.remaining() != 0 {
                return Err(Error::new("Trailing old metadata"));
            }
            w.u8(255);
            return Ok(w.data);
        }
        if seen[old as usize] {
            return Err(Error::new("Duplicate old metadata index"));
        }
        seen[old as usize] = true;
        let mut index = u16::from(old);
        if flags & 2 != 0 && index >= 19 {
            index += 1;
        } // villager finalization
        if flags & 4 != 0 && index >= 21 {
            index += 1;
        } // zombie villager
        if flags & 1 != 0 && index >= 17 {
            index += 1;
        } // age locked
        if index >= 255 {
            return Err(Error::new("Metadata index overflow"));
        }
        w.u8(index as u8);
        let source = r.varint()?;
        let kind = serializer_12111(source)?;
        w.varint(kind);
        let start = r.position();
        skip_source_metadata::<M>(kind, &mut r, registry)?;
        w.bytes(r.since(start));
    }
    Err(Error::new("Old metadata has no terminator"))
}
pub fn clientbound_using<M: Maps>(
    id: i32,
    payload: &[u8],
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<wire::Packet>> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|i| t::OLD_CLIENTBOUND.get(i))
        .copied()
        .ok_or_else(|| Error::new("Invalid entity packet ID"))?;
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    let mut target = name;
    match name {
        "ADD_ENTITY" => {
            use crate::buf::Codec;
            let mut packet = crate::packets::play::AddEntity::read(&mut r)?;
            packet.kind = if let Some(kind) = _registry.entity_override(packet.id) {
                kind
            } else {
                M::ENTITIES.map(packet.kind).map_err(Error::new)?
            };
            if crate::vfp_tables_261_native::ENTITY_NAMES.get(packet.kind as usize)
                == Some(&"falling_block")
            {
                packet.data = M::BLOCKSTATES.map(packet.data).map_err(Error::new)?;
            }
            crate::packets::play::AddEntity::write(&packet, &mut w);
        }
        "SET_ENTITY_DATA" => {
            var(&mut r, &mut w)?;
            metadata::<M>(&mut r, &mut w, _registry)?;
        }
        "LEVEL_PARTICLES" => {
            let flags = r.bytes(2)?.to_vec();
            let position = r.bytes(24)?.to_vec();
            let offsets = r.bytes(12)?.to_vec();
            let speed = r.bytes(4)?.to_vec();
            let count = r.i32()?;
            if count < 0 {
                return Err(Error::new("Negative particle count"));
            }
            particle_using::<M>(&mut r, &mut w, _registry)?;
            w.bytes(&flags);
            w.bytes(&position);
            w.bytes(&offsets);
            for _ in 0..3 {
                w.bytes(&speed);
            }
            w.varint(count);
            w.varint(0);
        }
        "LOGIN" => {
            w.i32(r.i32()?);
            w.bool(r.bool()?);
            let n = r.count(1024)?;
            w.varint(n as i32);
            for _ in 0..n {
                w.string(&r.string(32767)?);
            }
            for _ in 0..3 {
                var(&mut r, &mut w)?;
            }
            for _ in 0..3 {
                w.bool(r.bool()?);
            }
            spawn(&mut r, &mut w)?;
            w.bool(r.bool()?);
            w.bool(r.bool()?);
        }
        "RESPAWN" => {
            spawn(&mut r, &mut w)?;
            w.i8(r.i8()?);
        }
        "ANIMATE" => {
            var(&mut r, &mut w)?;
            match r.u8()? {
                action @ (0 | 3) => {
                    target = "SWING_ANIMATION";
                    w.varint(if action == 3 { 1 } else { 0 });
                    w.varint(1);
                    w.varint(6);
                }
                2 => w.u8(0),
                4 => w.u8(1),
                5 => w.u8(2),
                _ => return Err(Error::new("Unknown legacy entity animation")),
            }
        }
        _ => return crate::vfp_ui::clientbound_using::<M>(id, payload, _registry),
    }
    if r.remaining() != 0 {
        return Err(Error::new("Trailing entity packet bytes"));
    }
    Ok(vec![(
        wire::packet_id(t::NEW_CLIENTBOUND, target).map_err(Error::new)?,
        w.data,
    )])
}

pub fn clientbound_262(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<V262>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}
pub fn particle_262(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    particle_using::<V262>(r, w, &crate::vfp_registry_access::Registries::default())
}

pub fn particle_with<M: Maps>(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    particle_using::<M>(r, w, &crate::vfp_registry_access::Registries::default())
}

pub fn clientbound_with<M: Maps>(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<M>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}

fn serializer_12111(source: i32) -> Result<i32> {
    let serializers = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 23, 25, 26,
        27, 28, 30, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
    ];
    usize::try_from(source)
        .ok()
        .and_then(|id| serializers.get(id))
        .copied()
        .ok_or_else(|| Error::new("Unknown 1.21.11 metadata serializer"))
}
pub(crate) fn skip_source_metadata<M: Maps>(
    kind: i32,
    r: &mut Reader<'_>,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let mut ignored = Writer::new();
    match kind {
        7 => crate::vfp_items::stack_using::<M>(r, &mut ignored, true, false, registry)?,
        16 => particle_using::<M>(r, &mut ignored, registry)?,
        17 => {
            for _ in 0..r.count(65536)? {
                particle_using::<M>(r, &mut ignored, registry)?;
            }
        }
        33 => {
            if r.bool()? {
                r.string(32767)?;
                r.bytes(8)?;
            }
        }
        34 => {
            let id = r.varint()?;
            if id < 0 {
                return Err(Error::new("Negative old painting holder"));
            }
            if id == 0 {
                r.varint()?;
                r.varint()?;
                r.string(32767)?;
                for _ in 0..2 {
                    if r.bool()? {
                        r.nbt()?;
                    }
                }
            }
        }
        _ => {
            super::read_value(kind, r)?.ok_or_else(|| Error::new("Missing old metadata codec"))?;
        }
    }
    Ok(())
}

pub fn prepare_metadata_12111(
    payload: &[u8],
    flags: u8,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<u8>> {
    prepare_metadata_12111_using::<crate::vfp_mapping::V12111>(payload, flags, registry)
}
pub fn prepare_metadata_1219(
    payload: &[u8],
    flags: u8,
    game_time: i64,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<u8>> {
    prepare_metadata_1219_using::<crate::vfp_mapping::V1219>(payload, flags, game_time, registry)
}
pub fn prepare_metadata_1219_using<M: Maps>(
    payload: &[u8],
    flags: u8,
    game_time: i64,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.varint(r.varint()?);
    let mut seen = [false; 255];
    for _ in 0..256 {
        let index = r.u8()?;
        w.u8(index);
        if index == 255 {
            if r.remaining() != 0 {
                return Err(Error::new("Trailing 1.21.9 metadata"));
            }
            return Ok(w.data);
        }
        if seen[index as usize] {
            return Err(Error::new("Duplicate 1.21.9 metadata index"));
        }
        seen[index as usize] = true;
        let source = r.varint()?;
        if !(0..=36).contains(&source) {
            return Err(Error::new("Unknown 1.21.9 metadata serializer"));
        }
        if (flags & 1 != 0 && index == 21) || (flags & 2 != 0 && index == 18) {
            if source != 1 {
                return Err(Error::new("Invalid relative anger-time metadata"));
            }
            w.varint(2);
            w.varlong(game_time.wrapping_add(i64::from(r.varint()?)));
        } else if flags & 4 != 0 && index == 15 {
            if source != 0 {
                return Err(Error::new("Invalid old main-arm metadata"));
            }
            w.varint(38);
            w.varint(i32::from(r.i8()?));
        } else {
            let middle = if source >= 28 { source + 1 } else { source };
            w.varint(middle);
            let native = serializer_12111(middle)?;
            let start = r.position();
            skip_source_metadata::<M>(native, &mut r, registry)?;
            w.bytes(r.since(start));
        }
    }
    Err(Error::new("1.21.9 metadata has no terminator"))
}
pub fn fields_121<M: Maps>(
    payload: &[u8],
    registry: &crate::vfp_registry_access::Registries,
) -> Result<(i32, Vec<(u8, i32, Vec<u8>)>)> {
    let mut r = Reader::new(payload);
    let entity = r.varint()?;
    let mut fields = Vec::new();
    let mut seen = [false; 255];
    for _ in 0..256 {
        let index = r.u8()?;
        if index == 255 {
            if r.remaining() != 0 {
                return Err(Error::new("Trailing old metadata"));
            }
            return Ok((entity, fields));
        }
        if seen[index as usize] {
            return Err(Error::new("Duplicate old metadata index"));
        }
        seen[index as usize] = true;
        let source = r.varint()?;
        let start = r.position();
        if !(0..=30).contains(&source) {
            return Err(Error::new("Unknown old metadata serializer"));
        }
        if M::PROTOCOL < 767 && matches!(source, 23 | 26) {
            let id = r.varint()?;
            if id < 0 || id == i32::MAX {
                return Err(Error::new("Invalid old variant id"));
            }
            let mut value = Writer::new();
            value.varint(id + 1);
            fields.push((index, source, value.data));
            continue;
        }
        if source == 16 {
            r.nbt()?;
        } else if source == 23 {
            let id = r.varint()?;
            if id < 0 {
                return Err(Error::new("Negative wolf variant holder"));
            }
            if id == 0 {
                for _ in 0..3 {
                    r.string(32767)?;
                }
                let n = r.count(65537)?;
                if n == 0 {
                    r.string(32767)?;
                } else {
                    for _ in 1..n {
                        r.varint()?;
                    }
                }
            }
        } else {
            let target = match source {
                0..=22 => source,
                23 => 24,
                24 => 26,
                _ => source + 4,
            };
            let middle = if target <= 15 {
                target
            } else if target <= 32 {
                target - 1
            } else {
                target + 1
            };
            let native = serializer_12111(if middle >= 28 { middle + 1 } else { middle })?;
            skip_source_metadata::<M>(native, &mut r, registry)?;
        }
        fields.push((index, source, r.since(start).to_vec()));
    }
    Err(Error::new("Old metadata has no terminator"))
}
pub fn skip_121_value<M: Maps>(
    source: i32,
    r: &mut Reader<'_>,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    if !(0..=30).contains(&source) {
        return Err(Error::new("Unknown 1.21 metadata serializer"));
    }
    if source == 16 {
        r.nbt()?;
        return Ok(());
    }
    if M::PROTOCOL < 767 && matches!(source, 23 | 26) {
        r.varint()?;
        return Ok(());
    }
    if source == 23 {
        let id = r.varint()?;
        if id < 0 {
            return Err(Error::new("Negative wolf holder"));
        }
        if id == 0 {
            for _ in 0..3 {
                r.string(32767)?;
            }
            let count = r.count(65537)?;
            if count == 0 {
                r.string(32767)?;
            } else {
                for _ in 1..count {
                    r.varint()?;
                }
            }
        }
        return Ok(());
    }
    let target = match source {
        0..=22 => source,
        23 => 24,
        24 => 26,
        _ => source + 4,
    };
    let middle = if target <= 15 {
        target
    } else if target <= 32 {
        target - 1
    } else {
        target + 1
    };
    skip_source_metadata::<M>(
        serializer_12111(if middle >= 28 { middle + 1 } else { middle })?,
        r,
        registry,
    )
}
pub fn prepare_metadata_1214<M: Maps>(
    payload: &[u8],
    entity: &str,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<(Vec<u8>, Option<bool>)> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.varint(r.varint()?);
    let mut seen = [false; 255];
    let mut saddle = None;
    for _ in 0..256 {
        let index = r.u8()?;
        if index == 255 {
            w.u8(255);
            if r.remaining() != 0 {
                return Err(Error::new("Trailing 1.21.4 metadata"));
            }
            return Ok((w.data, saddle));
        }
        let index = if M::PROTOCOL < 769 && entity == "creaking" && index >= 18 {
            if index == 254 {
                return Err(Error::new("Creaking metadata index overflows"));
            }
            index + 1
        } else {
            index
        };
        if seen[index as usize] {
            return Err(Error::new("Duplicate 1.21.4 metadata index"));
        }
        seen[index as usize] = true;
        let source = r.varint()?;
        if !(0..=30).contains(&source) {
            return Err(Error::new("Unknown 1.21.4 metadata serializer"));
        }
        let target = match source {
            0..=22 => source,
            23 => 24,
            24 => 26,
            _ => source + 4,
        };
        let mut value = Writer::new();
        let mut target = target;
        if source == 16 {
            value.nbt(&r.nbt()?);
        } else if source == 23 {
            let holder = r.varint()?;
            if holder < 0 {
                return Err(Error::new("Negative old wolf holder"));
            }
            if holder == 0 {
                for _ in 0..3 {
                    r.string(32767)?;
                }
                let biomes = r.count(65537)?;
                if biomes == 0 {
                    r.string(32767)?;
                } else {
                    for _ in 1..biomes {
                        r.varint()?;
                    }
                }
            }
            value.varint(if holder == 0 { 0 } else { holder - 1 });
        } else if entity.ends_with("minecart") && index == 11 {
            if source != 1 {
                return Err(Error::new("Invalid old minecart display block"));
            }
            let state = r.varint()?;
            if M::BLOCKSTATES.map(state).map_err(Error::new)? == 0 {
                continue;
            }
            target = 15;
            value.varint(state);
        } else if M::PROTOCOL < 769 && entity == "salmon" && index == 17 {
            if source != 4 {
                return Err(Error::new("Invalid old salmon variant"));
            }
            target = 1;
            value.varint(match r.string(32767)?.as_str() {
                "small" => 0,
                "large" => 2,
                _ => 1,
            });
        } else if entity == "mooshroom" && index == 17 {
            if source != 4 {
                return Err(Error::new("Invalid mooshroom variant"));
            }
            target = 1;
            value.varint(if r.string(32767)? == "red" { 0 } else { 1 });
        } else {
            let middle = if target <= 15 {
                target
            } else if target <= 32 {
                target - 1
            } else {
                target + 1
            };
            let native = serializer_12111(if middle >= 28 { middle + 1 } else { middle })?;
            let start = r.position();
            skip_source_metadata::<M>(native, &mut r, registry)?;
            value.bytes(r.since(start));
        }
        if (entity == "pig" && index == 17) || (entity == "strider" && index == 19) {
            if source != 8 {
                return Err(Error::new("Invalid old saddle metadata"));
            }
            saddle = Some(Reader::new(&value.data).bool()?);
        } else if matches!(
            entity,
            "horse"
                | "donkey"
                | "mule"
                | "llama"
                | "trader_llama"
                | "skeleton_horse"
                | "zombie_horse"
                | "camel"
        ) && index == 17
        {
            if source != 0 {
                return Err(Error::new("Invalid horse flags"));
            }
            saddle = Some(Reader::new(&value.data).u8()? & 4 != 0);
        }
        let removed = if entity.ends_with("minecart") {
            Some(13)
        } else if matches!(entity, "pig" | "dolphin" | "turtle") {
            Some(17)
        } else {
            None
        };
        if removed == Some(index)
            || (entity == "strider" && index == 19)
            || (entity == "turtle" && matches!(index, 20..=22))
        {
            continue;
        }
        w.u8(if removed.is_some_and(|removed| index > removed) {
            index - 1
        } else {
            index
        });
        w.varint(target);
        w.bytes(&value.data);
    }
    Err(Error::new("1.21.4 metadata has no terminator"))
}
pub fn prepare_metadata_1217<M: Maps>(
    payload: &[u8],
    player: bool,
    hanging: bool,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<u8>> {
    use horizon_world::nbt::Tag;
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.varint(r.varint()?);
    let mut seen = [false; 255];
    for _ in 0..256 {
        let index = r.u8()?;
        if index == 255 {
            w.u8(255);
            if r.remaining() != 0 {
                return Err(Error::new("Trailing pre-1.21.9 metadata"));
            }
            return Ok(w.data);
        }
        if seen[index as usize] {
            return Err(Error::new("Duplicate old metadata index"));
        }
        seen[index as usize] = true;
        let source = r.varint()?;
        if !(0..=34).contains(&source) {
            return Err(Error::new("Unknown pre-1.21.9 metadata serializer"));
        }
        let target_index = if hanging && index >= 8 {
            if index == 254 {
                return Err(Error::new("Hanging metadata index overflows"));
            }
            index + 1
        } else if player {
            match index {
                17 => 16,
                18 => 15,
                15 | 16 => index + 2,
                _ => index,
            }
        } else {
            index
        };
        if source == 16 {
            let tag = r.nbt()?;
            if player && matches!(index, 19 | 20) {
                let variant = match tag.get("Variant") {
                    Some(Tag::Int(value)) => *value,
                    Some(Tag::Byte(value)) => i32::from(*value),
                    Some(Tag::Short(value)) => i32::from(*value),
                    _ => -1,
                };
                w.u8(target_index);
                w.varint(19);
                w.varint(if variant == -1 {
                    0
                } else {
                    variant.wrapping_add(1)
                });
            }
            continue; // The compound serializer was removed in 1.21.9.
        }
        let middle = if source <= 15 {
            source
        } else if source <= 32 {
            source - 1
        } else {
            source + 1
        };
        let native = serializer_12111(if middle >= 28 { middle + 1 } else { middle })?;
        w.u8(target_index);
        w.varint(middle);
        let start = r.position();
        skip_source_metadata::<M>(native, &mut r, registry)?;
        w.bytes(r.since(start));
    }
    Err(Error::new("Pre-1.21.9 metadata has no terminator"))
}
