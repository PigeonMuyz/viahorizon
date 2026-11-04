// SPDX-License-Identifier: GPL-3.0-or-later
// The pinned Horizon FilterMask codec still used the pre-26.3 long array.
fn inject(
    reader: &mut crate::buf::Reader<'_>,
) -> ::core::ops::ControlFlow<Result<(i32, Vec<i64>)>> {
    ::core::ops::ControlFlow::Break((|| {
        let kind = reader.varint()?;
        if !(0..=2).contains(&kind) {
            return Err(crate::Error::new("Invalid chat filter mask type"));
        }
        let mut longs = Vec::new();
        if kind == 2 {
            let bytes = reader.byte_array(1 << 20)?;
            for chunk in bytes.chunks(8) {
                let mut word = [0; 8];
                word[..chunk.len()].copy_from_slice(chunk);
                longs.push(i64::from_le_bytes(word));
            }
        }
        Ok((kind, longs))
    })())
}
