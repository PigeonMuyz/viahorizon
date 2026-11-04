// SPDX-License-Identifier: GPL-3.0-or-later
// Installed under buf so these bounded readers can retain packet positions.
use super::{NBT_QUOTA, Reader, Writer};
use crate::{Error, Result};
use horizon_world::nbt::{self, Tag};
impl Reader<'_> {
    pub(crate) fn vfp_named_nbt(&mut self) -> Result<Tag> {
        let (root, used) = nbt::read_named(
            &self.data[self.pos..],
            nbt::Limits {
                max_bytes: NBT_QUOTA,
                ..nbt::Limits::default()
            },
        )
        .map_err(|error| Error::new(format!("Legacy NBT: {error}")))?;
        self.pos += used;
        Ok(root.tag)
    }
}
impl Writer {
    pub(crate) fn vfp_named_nbt(&mut self, tag: &Tag) -> Result<()> {
        let bytes = nbt::write_named(
            &nbt::NamedTag {
                name: Default::default(),
                tag: tag.clone(),
            },
            nbt::Limits {
                max_bytes: NBT_QUOTA,
                ..nbt::Limits::default()
            },
        )
        .map_err(|error| Error::new(format!("Legacy NBT: {error}")))?;
        self.bytes(&bytes);
        Ok(())
    }
}
