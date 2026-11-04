use crate::vfp_mapping::{Maps, V262};
// SPDX-License-Identifier: GPL-3.0-or-later
// Versioned command, recipe-display and advancement layouts.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    packets::play,
    vfp_tables as t, vfp_wire as wire,
};
fn var(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.varint(r.varint()?);
    Ok(())
}
fn string(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.string(&r.string(32767)?);
    Ok(())
}
fn text<M: Maps>(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let mut value = r.nbt()?;
    crate::vfp_play::text_version(&mut value, M::PROTOCOL)?;
    w.nbt(&value);
    Ok(())
}
fn count(r: &mut Reader<'_>, w: &mut Writer) -> Result<usize> {
    let n = r.count(r.remaining())?;
    w.varint(n as i32);
    Ok(n)
}
fn commands<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let n = count(r, w)?;
    let index = |r: &mut Reader<'_>, w: &mut Writer| -> Result<()> {
        let id = r.varint()?;
        if id < 0 || id as usize >= n {
            return Err(Error::new("Command node reference out of range"));
        }
        w.varint(id);
        Ok(())
    };
    for _ in 0..n {
        let flags = r.u8()?;
        let kind = flags & 3;
        if kind == 3 || flags & !0x3f != 0 || (kind != 2 && flags & 16 != 0) {
            return Err(Error::new("Invalid command node flags"));
        }
        w.u8(flags);
        for _ in 0..count(r, w)? {
            index(r, w)?;
        }
        if flags & 8 != 0 {
            index(r, w)?;
        }
        if kind != 0 {
            string(r, w)?;
        }
        if kind == 2 {
            let id = M::ARGUMENTTYPES.map(r.varint()?).map_err(Error::new)?;
            w.varint(id);
            let start = r.position();
            if M::PROTOCOL < 762 && play::ARGUMENT_TYPES.get(id as usize) == Some(&"minecraft:time")
            {
                w.i32(0);
            } else {
                play::ArgumentProperties::read(r, id)?;
                w.bytes(r.since(start));
            }
            if flags & 16 != 0 {
                string(r, w)?;
            }
        }
    }
    index(r, w)
}
fn holders<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let n = r.varint()?;
    if n < 0 || n as usize > r.remaining() + 1 {
        return Err(Error::new("Invalid item holder set size"));
    }
    w.varint(n);
    if n == 0 {
        string(r, w)?;
    } else {
        for _ in 1..n {
            w.varint(M::ITEMS.map(r.varint()?).map_err(Error::new)?);
        }
    }
    Ok(())
}
fn slot<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    depth: usize,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    if depth > 32 {
        return Err(Error::new("Recipe slot displays nest too deeply"));
    }
    let source = r.varint()?;
    let kind = M::SLOT_DISPLAYS.map(source).map_err(Error::new)?;
    w.varint(kind);
    match kind {
        0 | 1 => (),
        2 => slot::<M>(r, w, depth + 1, _registry)?,
        3 => {
            slot::<M>(r, w, depth + 1, _registry)?;
            w.varint(
                M::DATA_COMPONENT_TYPE
                    .map(r.varint()?)
                    .map_err(Error::new)?,
            );
        }
        4 => w.varint(M::ITEMS.map(r.varint()?).map_err(Error::new)?),
        5 => crate::vfp_items::template_using::<M>(r, w, true, _registry)?,
        6 => {
            w.varint(0);
            string(r, w)?;
        }
        7 | 9 => {
            slot::<M>(r, w, depth + 1, _registry)?;
            slot::<M>(r, w, depth + 1, _registry)?;
        }
        8 => {
            slot::<M>(r, w, depth + 1, _registry)?;
            slot::<M>(r, w, depth + 1, _registry)?;
            let holder = r.varint()?;
            if holder < 0 {
                return Err(Error::new("Negative trim-pattern holder"));
            }
            w.varint(holder);
            if holder == 0 {
                string(r, w)?;
                w.nbt(&r.nbt()?);
                w.bool(r.bool()?);
            }
        }
        10 => {
            for _ in 0..count(r, w)? {
                slot::<M>(r, w, depth + 1, _registry)?;
            }
        }
        _ => return Err(Error::new("Unknown recipe slot display")),
    }
    Ok(())
}
fn display<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let kind = r.varint()?;
    w.varint(kind);
    match kind {
        0 | 1 => {
            if kind == 1 {
                var(r, w)?;
                var(r, w)?;
            }
            for _ in 0..count(r, w)? {
                slot::<M>(r, w, 0, _registry)?;
            }
            slot::<M>(r, w, 0, _registry)?;
            slot::<M>(r, w, 0, _registry)?;
        }
        2 => {
            for _ in 0..4 {
                slot::<M>(r, w, 0, _registry)?;
            }
            var(r, w)?;
            w.f32(r.f32()?);
        }
        3 | 4 => {
            for _ in 0..if kind == 3 { 3 } else { 5 } {
                slot::<M>(r, w, 0, _registry)?;
            }
        }
        _ => return Err(Error::new("Unknown recipe display")),
    }
    Ok(())
}
fn advancements<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    w.bool(r.bool()?);
    for _ in 0..count(r, w)? {
        string(r, w)?;
        let parent = r.bool()?;
        w.bool(parent);
        if parent {
            string(r, w)?;
        }
        let present = r.bool()?;
        w.bool(present);
        let (mut x, mut y) = (0.0, 0.0);
        if present {
            text::<M>(r, w)?;
            text::<M>(r, w)?;
            crate::vfp_items::template_using::<M>(r, w, true, _registry)?;
            var(r, w)?;
            let flags = r.i32()?;
            w.i32(flags);
            if flags & 1 != 0 {
                if M::PROTOCOL < 770 {
                    let asset = r.string(32767)?;
                    let (namespace, path) = asset
                        .split_once(':')
                        .unwrap_or(("minecraft", asset.as_str()));
                    let path = path
                        .strip_prefix("textures/")
                        .and_then(|path| path.strip_suffix(".png"))
                        .unwrap_or(path);
                    w.string(&format!("{namespace}:{path}"));
                } else {
                    string(r, w)?;
                }
            }
            x = r.f32()?;
            y = r.f32()?;
        }
        for _ in 0..count(r, w)? {
            for _ in 0..count(r, w)? {
                string(r, w)?;
            }
        }
        w.bool(r.bool()?);
        w.f32(x);
        w.f32(y);
    }
    // Removed identifiers and criterion progress retain their layout. Validate
    // the complete packet with the native decoder below.
    w.bytes(r.rest());
    if M::PROTOCOL < 770 {
        w.bool(true);
    }
    r.bytes(r.remaining())?;
    Ok(())
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
        .ok_or_else(|| Error::new("Invalid UI packet ID"))?;
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    match name {
        "COMMANDS" => commands::<M>(&mut r, &mut w, _registry)?,
        "UPDATE_ADVANCEMENTS" => advancements::<M>(&mut r, &mut w, _registry)?,
        "UPDATE_RECIPES" => {
            let groups = r.count(r.remaining())?;
            w.varint(groups as i32 + 2);
            for (name, ids) in [
                ("minecraft:brewing_input", t::BREWING_INPUTS),
                ("minecraft:brewing_reagent", t::BREWING_REAGENTS),
            ] {
                w.string(name);
                w.varint(ids.len() as i32);
                for &id in ids {
                    w.varint(id);
                }
            }
            for _ in 0..groups {
                string(&mut r, &mut w)?;
                for _ in 0..count(&mut r, &mut w)? {
                    w.varint(M::ITEMS.map(r.varint()?).map_err(Error::new)?);
                }
            }
            for _ in 0..count(&mut r, &mut w)? {
                holders::<M>(&mut r, &mut w, _registry)?;
                slot::<M>(&mut r, &mut w, 0, _registry)?;
            }
        }
        "RECIPE_BOOK_ADD" => {
            for _ in 0..count(&mut r, &mut w)? {
                var(&mut r, &mut w)?;
                display::<M>(&mut r, &mut w, _registry)?;
                var(&mut r, &mut w)?;
                var(&mut r, &mut w)?;
                let requirements = r.bool()?;
                w.bool(requirements);
                if requirements {
                    for _ in 0..count(&mut r, &mut w)? {
                        holders::<M>(&mut r, &mut w, _registry)?;
                    }
                }
                w.i8(r.i8()?);
            }
            w.bool(r.bool()?);
        }
        "PLACE_GHOST_RECIPE" => {
            var(&mut r, &mut w)?;
            display::<M>(&mut r, &mut w, _registry)?;
        }
        _ => return crate::vfp_play::clientbound_using::<M>(id, payload, _registry),
    }
    if r.remaining() != 0 {
        return Err(Error::new("Trailing UI packet bytes"));
    }
    let target = wire::packet_id(t::NEW_CLIENTBOUND, name).map_err(Error::new)?;
    play::Clientbound::decode(target, &w.data)?;
    Ok(vec![(target, w.data)])
}

pub fn clientbound_262(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<V262>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}

pub fn clientbound_with<M: Maps>(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<M>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}
