// SPDX-License-Identifier: GPL-3.0-or-later
fn vfp_recv(
    reader: &mut crate::connection::ConnReader,
) -> core::ops::ControlFlow<Result<(i32, Vec<u8>)>> {
    match crate::connection::vfp_bridge::recv(reader) {
        Some(result) => core::ops::ControlFlow::Break(result),
        None => core::ops::ControlFlow::Continue(()),
    }
}
