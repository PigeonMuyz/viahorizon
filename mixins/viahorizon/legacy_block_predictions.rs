// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(game: &mut crate::game::Game<'_>, sequence: i32) -> ::core::ops::ControlFlow<()> {
    if let Some(remote) = game.remote.as_mut()
        && horizon_client::net::vfp_capture_session::unsequenced_blocks(&remote.session)
    {
        // Before 1.19 the local change stays until an authoritative block
        // update replaces it. A synthetic sequence ack settles bookkeeping,
        // but must not restore the pre-click state before the server replies.
        remote.predictions.retain(|_, prediction| prediction.sequence > sequence);
        return ::core::ops::ControlFlow::Break(());
    }
    ::core::ops::ControlFlow::Continue(())
}
