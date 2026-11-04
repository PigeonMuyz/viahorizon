// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    session: &crate::net::Session,
    packet: &horizon_protocol::packets::play::Serverbound,
) -> ::core::ops::ControlFlow<()> {
    if crate::net::vfp_capture_session::enqueue_legacy(session, packet) {
        ::core::ops::ControlFlow::Break(())
    } else {
        ::core::ops::ControlFlow::Continue(())
    }
}
