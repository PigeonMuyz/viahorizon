// SPDX-License-Identifier: GPL-3.0-or-later
// 1.16.4 transaction IDs <-> 1.17 predicted slot changes. Inventories and ping
// acknowledgements belong to this connection; rejected clicks await resync.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::Step,
    vfp_release_codec::{self as codec, Stack, Transactions},
    vfp_wire::Packet,
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Inventory {
    transactions: Transactions,
    pings: BTreeMap<i32, (u8, i16)>,
    next_ping: i32,
    active: Option<(u8, Option<i32>)>,
    slots: BTreeMap<u8, i16>,
    selected: i16,
}
fn mapped(step: &Step, stack: &Option<Stack>, inverse: bool) -> Result<Option<Stack>> {
    let mut stack = stack.clone();
    if let Some(stack) = &mut stack {
        stack.id = step.same_or_map("items", stack.id, inverse)?;
    }
    Ok(stack)
}
impl Inventory {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn close(&mut self, window: u8) {
        self.transactions.close(window);
        self.slots.remove(&window);
        if self.active.is_some_and(|(id, _)| id == window) {
            self.active = None;
        }
    }
    pub fn open(&mut self, window: u8, kind: Option<i32>) {
        if let Some((previous, _)) = self.active {
            self.close(previous);
        }
        self.close(window);
        self.active = Some((window, kind));
    }
    pub fn select(&mut self, slot: i16) {
        if (0..9).contains(&slot) {
            self.selected = slot;
        }
    }
    pub fn held(&self) -> Option<Option<Stack>> {
        self.active
            .and_then(|(window, _)| {
                let count = *self.slots.get(&window)?;
                if count < 36 {
                    return None;
                }
                self.transactions
                    .projected_slot(window, count - 9 + self.selected)
            })
            .or_else(|| self.transactions.projected_slot(0, 36 + self.selected))
    }
    fn mirror_hotbar(&mut self, window: u8) {
        if !self.active.is_some_and(|(id, _)| id == window) {
            return;
        }
        let Some(count) = self
            .slots
            .get(&window)
            .copied()
            .filter(|count| *count >= 36)
        else {
            return;
        };
        for index in 0..9 {
            if let Some(value) = self
                .transactions
                .authoritative_slot(window, count - 9 + index)
            {
                self.transactions.set_slot(0, 36 + index, value);
            }
        }
    }
    pub fn content(&mut self, step: &Step, data: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let window = r.u8()?;
        w.u8(window);
        let count = r.i16()?;
        if !(0..=4096).contains(&count) {
            return Err(Error::new("Invalid transaction inventory length"));
        }
        w.i16(count);
        let mut slots = Vec::new();
        for _ in 0..count {
            let stack = Stack::read(&mut r, step.source)?;
            Stack::write(mapped(step, &stack, false)?.as_ref(), &mut w, step.target)?;
            slots.push(stack);
        }
        codec::end(&r)?;
        self.transactions.content(window, slots)?;
        self.slots.insert(window, count);
        self.mirror_hotbar(window);
        Ok(w.data)
    }
    pub fn slot(&mut self, step: &Step, data: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let window = r.u8()?;
        let slot = r.i16()?;
        w.u8(window);
        w.i16(slot);
        let stack = Stack::read(&mut r, step.source)?;
        codec::end(&r)?;
        Stack::write(mapped(step, &stack, false)?.as_ref(), &mut w, step.target)?;
        self.transactions
            .set_slot(if window == 254 { 0 } else { window }, slot, stack);
        if self.active.is_some_and(|(id, _)| id == window) {
            if let Some(count) = self
                .slots
                .get(&window)
                .copied()
                .filter(|count| *count >= 36)
            {
                if (count - 9..count).contains(&slot) {
                    if let Some(value) = self.transactions.authoritative_slot(window, slot) {
                        self.transactions
                            .set_slot(0, 36 + slot - (count - 9), value);
                    }
                }
            }
        }
        Ok(w.data)
    }
    pub fn click(&mut self, step: &Step, data: &[u8], server: i32) -> Result<Option<Vec<u8>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let window = r.u8()?;
        let slot = r.i16()?;
        let button = r.i8()?;
        let mode = r.varint()?;
        if !(0..=6).contains(&mode) {
            return Err(Error::new("Unknown old container click mode"));
        }
        let active_kind = self
            .active
            .filter(|(id, _)| *id == window)
            .and_then(|(_, kind)| kind);
        // Do not allocate a legacy action ID for a click the older edge
        // would suppress. No server confirmation can arrive for these slots.
        if server <= 47
            && ((window == 0 && slot == 45)
                || (mode == 2 && button == 40)
                || (active_kind == Some(10) && slot == 4)
                || (server <= 5 && active_kind == Some(12) && slot == 1))
        {
            return Ok(None);
        }
        let count = r.count(128)?;
        let mut changes = BTreeMap::new();
        for _ in 0..count {
            let slot = r.i16()?;
            let stack = Stack::read(&mut r, step.target)?;
            changes.insert(slot, mapped(step, &stack, true)?);
        }
        Stack::read(&mut r, step.target)?;
        codec::end(&r)?;
        if self.transactions.waiting(window) {
            return Ok(None);
        }
        let clicked = if mode == 0 || (mode == 1 && !changes.is_empty()) {
            self.transactions.clicked(window, slot)
        } else {
            None
        };
        let action = self.transactions.begin(window, changes)?;
        w.u8(window);
        w.i16(slot);
        w.i8(button);
        w.i16(action);
        w.varint(mode);
        Stack::write(clicked.as_ref(), &mut w, step.source)?;
        Ok(Some(w.data))
    }
    pub fn acknowledge(&mut self, step: &Step, data: &[u8]) -> Result<Vec<Packet>> {
        let mut r = Reader::new(data);
        let window = r.u8()?;
        let action = r.i16()?;
        let accepted = codec::boolean(&mut r)?;
        codec::end(&r)?;
        if self.transactions.pending(window, action) {
            self.transactions.acknowledge(window, action, accepted)?;
        }
        if accepted {
            self.mirror_hotbar(window);
            return Ok(Vec::new());
        }
        if self.pings.len() >= 1024 {
            return Err(Error::new(
                "Too many pending legacy transaction acknowledgements",
            ));
        }
        self.next_ping = self
            .next_ping
            .checked_sub(1)
            .ok_or_else(|| Error::new("Legacy ping counter exhausted"))?;
        self.pings.insert(self.next_ping, (window, action));
        let mut w = Writer::new();
        w.i32(self.next_ping);
        Ok(vec![step.incoming("PING", w.data)?])
    }
    pub fn pong(&mut self, step: &Step, data: &[u8]) -> Result<Option<Packet>> {
        let mut r = Reader::new(data);
        let id = r.i32()?;
        codec::end(&r)?;
        let Some((window, action)) = self.pings.remove(&id) else {
            return Ok(None);
        };
        let mut w = Writer::new();
        w.u8(window);
        w.i16(action);
        w.bool(true);
        Ok(Some(step.outgoing("CONTAINER_ACK", w.data)?))
    }
}
