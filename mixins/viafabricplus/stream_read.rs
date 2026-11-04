// SPDX-License-Identifier: GPL-3.0-or-later
fn vfp_stream_read(
    stream: &mut crate::connection::Stream,
    output: &mut [u8],
) -> core::ops::ControlFlow<std::io::Result<usize>> {
    match crate::connection::vfp_bridge::read_with_events(stream, output) {
        Some(result) => core::ops::ControlFlow::Break(result),
        None => core::ops::ControlFlow::Continue(()),
    }
}
