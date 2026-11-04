// SPDX-License-Identifier: GPL-3.0-or-later
fn vfp_send(
    writer: &mut crate::connection::ConnWriter,
    body: &[u8],
) -> core::ops::ControlFlow<Result<()>> {
    match crate::connection::vfp_bridge::send(writer, body) {
        Some(result) => core::ops::ControlFlow::Break(result),
        None => core::ops::ControlFlow::Continue(()),
    }
}
