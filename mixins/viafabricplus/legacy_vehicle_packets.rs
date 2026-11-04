// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(game: &crate::game::Game<'_>) -> ::core::ops::ControlFlow<Vec<S>> {
    if game.remote.as_ref().is_some_and(|remote| {
        matches!(
            horizon_client::net::vfp_capture_session::legacy_protocol(&remote.session),
            Some(4 | 5 | 47)
        )
    }) {
        // The existing input bridge still sends the steer flags. The old
        // server owns the boat's actual movement.
        ::core::ops::ControlFlow::Break(Vec::new())
    } else {
        ::core::ops::ControlFlow::Continue(())
    }
}
