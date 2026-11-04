// SPDX-License-Identifier: GPL-3.0-or-later
// 1.21.2-1.21.4 palettes carry explicit storage lengths; 1.21.5 derives them.
// Static registry ids stay in their source domain for the final Maps codec.
use crate::{
    Error, Result,
    buf::{Codec, Reader, Writer},
    chunk::{BlockEntityInfo, ChunkData},
};
use horizon_world::nbt::Tag;
pub fn chunk(r: &mut Reader<'_>) -> Result<ChunkData> {
    chunk_using::<crate::vfp_mapping::V1214>(r)
}
pub fn chunk_using<M: crate::vfp_mapping::Maps>(r: &mut Reader<'_>) -> Result<ChunkData> {
    let heightmaps = if M::PROTOCOL < 764 {
        r.vfp_named_nbt()?
    } else {
        r.nbt()?
    };
    let Tag::Compound(heightmaps) = heightmaps else {
        return Err(Error::new("Old heightmaps are not a compound"));
    };
    let names = [
        "WORLD_SURFACE_WG",
        "WORLD_SURFACE",
        "OCEAN_FLOOR_WG",
        "OCEAN_FLOOR",
        "MOTION_BLOCKING",
        "MOTION_BLOCKING_NO_LEAVES",
    ];
    let mut converted = Vec::new();
    for (name, value) in heightmaps {
        if let Some(kind) = names
            .iter()
            .position(|known| *known == name.to_string_lossy())
        {
            if let Tag::LongArray(values) = value {
                converted.push((kind as i32, values));
            }
        }
    }
    converted.sort_unstable_by_key(|(kind, _)| *kind);
    let buffer = r.byte_array(2 << 20)?;
    let n = r.count(65536)?;
    let block_entities = (0..n)
        .map(|_| {
            if M::PROTOCOL < 764 {
                Ok(BlockEntityInfo {
                    packed_xz: r.i8()?,
                    y: r.i16()?,
                    kind: r.varint()?,
                    tag: r.vfp_named_nbt()?,
                })
            } else {
                BlockEntityInfo::read(r)
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ChunkData {
        heightmaps: converted,
        buffer,
        block_entities,
    })
}
pub fn biomes(data: &[u8], biome_count: usize) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    let n = r.count(65536)?;
    w.varint(n as i32);
    for _ in 0..n {
        w.bytes(r.bytes(8)?);
        let bytes = r.byte_array(2 << 20)?;
        let converted =
            crate::vfp_wire::biome_palettes_1214_using(&bytes, biome_count).map_err(Error::new)?;
        w.byte_array(&converted);
    }
    if r.remaining() != 0 {
        return Err(Error::new("Trailing old biome update"));
    }
    Ok(w.data)
}
