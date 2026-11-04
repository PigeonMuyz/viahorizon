// SPDX-License-Identifier: GPL-3.0-or-later
// Common packets the pinned client keeps as Unknown still need an audited
// adapter. Their payloads are validated here rather than rejected for lacking
// a native typed enum variant.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
};
fn var(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.varint(r.varint()?);
    Ok(())
}
fn string(r: &mut Reader<'_>, w: &mut Writer, max: usize) -> Result<()> {
    w.string(&r.string(max)?);
    Ok(())
}
fn count(r: &mut Reader<'_>, w: &mut Writer) -> Result<usize> {
    let n = r.count(r.remaining())?;
    w.varint(n as i32);
    Ok(n)
}
fn finish(r: Reader<'_>, w: Writer) -> Result<Option<Vec<u8>>> {
    if r.remaining() != 0 {
        return Err(Error::new("Trailing auxiliary packet bytes"));
    }
    Ok(Some(w.data))
}
pub fn clientbound(name: &str, payload: &[u8]) -> Result<Option<Vec<u8>>> {
    clientbound_version(name, payload, 776)
}
pub fn clientbound_version(name: &str, payload: &[u8], protocol: i32) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    match name {
        "DEBUG_SAMPLE" => {
            let n = r.count(r.remaining() / 8)?;
            w.varint(n as i32);
            for _ in 0..n {
                w.i64(r.i64()?);
            }
            var(&mut r, &mut w)?;
        }
        "COMMAND_SUGGESTIONS" => {
            var(&mut r, &mut w)?;
            let start = r.varint()?;
            let length = r.varint()?;
            if start < 0 || length < 0 || start.checked_add(length).is_none() {
                return Err(Error::new("Invalid command suggestion range"));
            }
            w.varint(start);
            w.varint(length);
            for _ in 0..count(&mut r, &mut w)? {
                string(&mut r, &mut w, 32767)?;
                let tooltip = r.bool()?;
                w.bool(tooltip);
                if tooltip {
                    let mut tag = r.nbt()?;
                    crate::vfp_play::text_version(&mut tag, protocol)?;
                    w.nbt(&tag);
                }
            }
        }
        "CUSTOM_CHAT_COMPLETIONS" => {
            let action = r.varint()?;
            if !(0..=2).contains(&action) {
                return Err(Error::new("Invalid chat completion action"));
            }
            w.varint(action);
            for _ in 0..count(&mut r, &mut w)? {
                string(&mut r, &mut w, 32767)?;
            }
        }
        "CUSTOM_REPORT_DETAILS" => {
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..count(&mut r, &mut w)? {
                let key = r.string(128)?;
                if !seen.insert(key.clone()) {
                    return Err(Error::new("Duplicate custom report detail"));
                }
                w.string(&key);
                string(&mut r, &mut w, 4096)?;
            }
        }
        _ => return Ok(None),
    }
    finish(r, w)
}
pub fn serverbound(name: &str, payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    match name {
        "DEBUG_SUBSCRIPTION_REQUEST" => {
            let n = r.count(32)?;
            w.varint(n as i32);
            for _ in 0..n {
                let id = r.varint()?;
                if id < 0 {
                    return Err(Error::new("Negative debug subscription id"));
                }
                w.varint(id);
            }
        }
        "COMMAND_SUGGESTION" => {
            var(&mut r, &mut w)?;
            string(&mut r, &mut w, 32767)?;
        }
        "BLOCK_ENTITY_TAG_QUERY" => {
            var(&mut r, &mut w)?;
            w.bytes(r.bytes(8)?);
        }
        "ENTITY_TAG_QUERY" => {
            var(&mut r, &mut w)?;
            var(&mut r, &mut w)?;
        }
        "RECIPE_BOOK_SEEN_RECIPE" => var(&mut r, &mut w)?,
        "SEEN_ADVANCEMENTS" => {
            let action = r.varint()?;
            if !(0..=1).contains(&action) {
                return Err(Error::new("Invalid advancement-tab action"));
            }
            w.varint(action);
            if action == 0 {
                string(&mut r, &mut w, 32767)?;
            }
        }
        "SET_COMMAND_MINECART" => {
            var(&mut r, &mut w)?;
            string(&mut r, &mut w, 32767)?;
            w.bool(r.bool()?);
        }
        "JIGSAW_GENERATE" => {
            w.bytes(r.bytes(8)?);
            var(&mut r, &mut w)?;
            w.bool(r.bool()?);
        }
        _ => return Ok(None),
    }
    finish(r, w)
}
