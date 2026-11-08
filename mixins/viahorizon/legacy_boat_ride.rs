// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(game: &mut crate::game::Game<'_>) -> ::core::ops::ControlFlow<Result<bool>> {
    if !game.remote.as_ref().is_some_and(|remote| {
        matches!(
            horizon_client::net::vfp_capture_session::legacy_protocol(&remote.session),
            Some(4 | 5 | 47)
        )
    }) {
        return ::core::ops::ControlFlow::Continue(());
    }
    let Some(boat) = game
        .boats
        .iter()
        .position(|boat| Some(boat.id) == game.riding)
    else {
        return ::core::ops::ControlFlow::Continue(());
    };
    let seat = game.player_boat_seat(boat);
    if let Some(player) = game.player.as_mut() {
        game.previous_position = player.position;
        player.position = seat;
        player.velocity = horizon_core::geometry::Vec3::ZERO;
        player.fall_distance = 0.0;
    }
    ::core::ops::ControlFlow::Break(Ok(true))
}
