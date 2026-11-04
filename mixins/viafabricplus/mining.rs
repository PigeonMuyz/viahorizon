// SPDX-License-Identifier: GPL-3.0-or-later
// 26.3 moved continuing mining particles/sounds to server level events.
use crate::{
    vfp_core::{Cursor, Result},
    vfp_tables as tables,
    vfp_wire::{Packet, packet_id},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mining {
    position: Option<[u8; 8]>,
    direction: u8,
    tick: u8,
    punched: bool,
}
impl Mining {
    pub fn ongoing(&self) -> bool {
        self.position.is_some()
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn serverbound(&mut self, name: &str, payload: &[u8]) -> Result<Option<Packet>> {
        let mut next = self.clone();
        let event = next.apply(name, payload)?;
        *self = next;
        Ok(event)
    }
    fn apply(&mut self, name: &str, payload: &[u8]) -> Result<Option<Packet>> {
        let mut r = Cursor::new(payload);
        match name {
            "PLAYER_ACTION" => {
                let action = r.varint()?;
                let position: [u8; 8] = r.take(8)?.try_into().unwrap();
                let direction = r.take(1)?[0];
                r.varint()?;
                r.end()?;
                if (0..=3).contains(&action) && direction > 5 {
                    return Err("Invalid mining direction".into());
                }
                match action {
                    0 => {
                        self.position = Some(position);
                        self.direction = direction;
                        self.tick = 0;
                        self.punched = false;
                    }
                    1 => self.direction = direction,
                    2 | 3 => self.reset(),
                    _ => (),
                }
            }
            "PUNCH" => {
                r.end()?;
                self.punched = true;
            }
            "CLIENT_TICK_END" => {
                r.end()?;
                if self.position.is_none() || !self.punched {
                    self.reset();
                    return Ok(None);
                }
                self.punched = false;
                let tick = self.tick;
                self.tick = if tick == 4 { 1 } else { tick + 1 };
                // The initial action already produces its own effect.
                if tick == 0 {
                    return Ok(None);
                }
                let mut data = Vec::with_capacity(17);
                data.extend_from_slice(&if tick == 4 { 2020i32 } else { 2019i32 }.to_be_bytes());
                data.extend_from_slice(&self.position.unwrap());
                data.extend_from_slice(&i32::from(self.direction).to_be_bytes());
                data.push(0);
                return Ok(Some((
                    packet_id(tables::NEW_CLIENTBOUND, "LEVEL_EVENT")?,
                    data,
                )));
            }
            _ => (),
        }
        Ok(None)
    }
}
