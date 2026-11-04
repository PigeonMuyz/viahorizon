// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(game: &mut crate::game::Game<'_>, pos: [i32; 3]) {
    if let Some(remote) = game.remote.as_mut()
        && horizon_client::net::vfp_capture_session::unsequenced_blocks(&remote.session)
    {
        // Old servers have no sequence ack to release a retained server state.
        // Apply their correction immediately, including refused placements.
        remote.predictions.remove(&pos);
    }
}
