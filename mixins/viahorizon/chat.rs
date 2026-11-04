// SPDX-License-Identifier: GPL-3.0-or-later
// Horizon's ChatTypeBound stores only an integer registry reference. Keep a
// direct holder inside an internal sender envelope, paired with a reserved
// negative reference. The game formatting hook consumes that envelope; it is
// never sent to a server and needs no global or thread-local holder storage.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    packets::play::ChatTypeBound,
};
use horizon_world::nbt::Tag;
pub const DIRECT: i32 = -2;
pub const ENVELOPE: &str = "viafabricplus:inline_chat_type";
fn decoration(r: &mut Reader<'_>, protocol: i32) -> Result<Tag> {
    let key = r.string(32767)?;
    let n = r.count(1024)?;
    let mut parameters = Vec::with_capacity(n);
    for _ in 0..n {
        let parameter = match r.varint()? {
            0 => "sender",
            1 => "target",
            2 => "content",
            _ => return Err(Error::new("Unknown inline chat parameter")),
        };
        parameters.push(Tag::String(parameter.into()));
    }
    let mut style = r.nbt()?;
    if !matches!(style, Tag::Compound(_)) {
        return Err(Error::new("Inline chat style is not a compound"));
    }
    crate::vfp_play::text_version(&mut style, protocol)?;
    Ok(Tag::compound([
        ("translation_key", Tag::String(key.as_str().into())),
        ("parameters", Tag::List(parameters)),
        ("style", style),
    ]))
}
pub fn bound(r: &mut Reader<'_>, w: &mut Writer, protocol: i32) -> Result<()> {
    let id = r.varint()?;
    if id < 0 {
        return Err(Error::new("Negative server chat holder"));
    }
    let direct = if id == 0 {
        Some(Tag::compound([
            ("chat", decoration(r, protocol)?),
            ("narration", decoration(r, protocol)?),
        ]))
    } else {
        None
    };
    let mut sender = r.nbt()?;
    if let Some(holder) = direct {
        crate::vfp_play::text_version(&mut sender, protocol)?;
        w.varint(DIRECT + 1);
        w.nbt(&Tag::compound([(
            ENVELOPE,
            Tag::compound([("sender", sender), ("type", holder)]),
        )]));
    } else {
        w.varint(id);
        w.nbt(&sender);
    }
    let target = r.bool()?;
    w.bool(target);
    if target {
        let target = r.nbt()?;
        w.nbt(&target);
    }
    if r.remaining() != 0 {
        return Err(Error::new("Trailing chat-bound data"));
    }
    Ok(())
}
pub fn direct(bound: &ChatTypeBound) -> Option<(&Tag, &Tag)> {
    direct_decoration(bound, false)
}

pub fn direct_decoration(bound: &ChatTypeBound, narration: bool) -> Option<(&Tag, &Tag)> {
    if bound.chat_type != DIRECT {
        return None;
    }
    let envelope = bound.name.get(ENVELOPE)?;
    Some((envelope.get("sender")?, envelope.get("type")?.get(if narration { "narration" } else { "chat" })?))
}
