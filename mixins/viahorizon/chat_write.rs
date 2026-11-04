// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    value: &(i32, Vec<i64>),
    writer: &mut crate::buf::Writer,
) -> ::core::ops::ControlFlow<()> {
    writer.varint(value.0);
    if value.0 == 2 {
        let mut bytes: Vec<u8> = value.1.iter().flat_map(|v| v.to_le_bytes()).collect();
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        writer.byte_array(&bytes);
    }
    ::core::ops::ControlFlow::Break(())
}
