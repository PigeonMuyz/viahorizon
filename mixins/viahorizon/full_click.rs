// SPDX-License-Identifier: GPL-3.0-or-later
// Internal game -> translator message, consumed before network framing.
// It is selected only for a live old-protocol connection and never sent to a
// server. This preserves component values that native Hashed::of discards.
use crate::{
    Error, Result,
    buf::{Codec, Reader, Writer},
    item::{Slot, Stack},
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
    vfp_wire::{self as wire, Packet},
};
pub const INTERNAL_ID: i32 = 0x7fff_ff00;
pub fn encode(
    container: i32,
    state: i32,
    slot: i16,
    button: i8,
    input: i32,
    changed: &[(i16, Option<Stack>)],
    carried: &Option<Stack>,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(container);
    w.varint(state);
    w.i16(slot);
    w.i8(button);
    w.varint(input);
    let changed: std::collections::BTreeMap<_, _> =
        changed.iter().map(|(slot, stack)| (*slot, stack)).collect();
    w.varint(changed.len() as i32);
    for (slot, stack) in changed {
        w.i16(slot);
        Slot::write(stack, &mut w);
    }
    Slot::write(carried, &mut w);
    w.data
}
fn stack<M: Maps>(r: &mut Reader<'_>, w: &mut Writer, registry: &Registries) -> Result<()> {
    let start = r.position();
    let value = Slot::read(r)?;
    if value
        .as_ref()
        .is_some_and(|stack| M::ITEMS.inverse(stack.item).is_err())
    {
        // A native creative item absent from this server requests a normal
        // authoritative inventory resynchronization, as hashed clicks do.
        if M::NATIVE_ITEMS {
            w.bool(false);
        } else {
            w.varint(0);
        }
        return Ok(());
    }
    crate::vfp_items::stack_using::<M>(&mut Reader::new(r.since(start)), w, false, false, registry)
}
pub fn serverbound<M: Maps>(data: &[u8], registry: &Registries) -> Result<Vec<Packet>> {
    if M::PROTOCOL >= 770 {
        return Err(Error::new(
            "Full-stack predictions require a pre-1.21.5 server",
        ));
    }
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    w.varint(r.varint()?);
    w.varint(r.varint()?);
    w.i16(r.i16()?);
    w.i8(r.i8()?);
    let input = r.varint()?;
    if !(0..=6).contains(&input) {
        return Err(Error::new("Invalid inventory click type"));
    }
    w.varint(input);
    let count = r.count(128)?;
    w.varint(count as i32);
    let mut slots = std::collections::BTreeSet::new();
    for _ in 0..count {
        let slot = r.i16()?;
        if !slots.insert(slot) {
            return Err(Error::new("Duplicate changed inventory slot"));
        }
        w.i16(slot);
        stack::<M>(&mut r, &mut w, registry)?;
    }
    stack::<M>(&mut r, &mut w, registry)?;
    if r.remaining() != 0 {
        return Err(Error::new("Trailing full inventory prediction"));
    }
    Ok(vec![(
        wire::packet_id(crate::vfp_tables::OLD_SERVERBOUND, "CONTAINER_CLICK")
            .map_err(Error::new)?,
        w.data,
    )])
}
