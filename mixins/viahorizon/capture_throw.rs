// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    game: &mut crate::game::Game<'_>,
    right: bool,
    input: horizon_client::menu::Input,
) -> ::core::ops::ControlFlow<()> {
    use horizon_client::menu::Input;
    if !game
        .remote
        .as_ref()
        .is_some_and(|remote| horizon_client::net::vfp_capture_session::full_items(&remote.session))
    {
        return ::core::ops::ControlFlow::Continue(());
    }
    let container = game
        .remote
        .as_ref()
        .and_then(|remote| remote.menu.as_ref())
        .map_or(0, |menu| menu.0);
    if input == Input::Pickup
        && let Some(carried) = game.carried.as_mut()
    {
        if right && carried.count > 1 {
            carried.count -= 1;
        } else {
            game.carried = None;
        }
        game.swing_arm();
    }
    let carried = game.to_network(&game.carried);
    let state = game.remote.as_ref().map_or(0, |remote| remote.state_id);
    let data = horizon_protocol::vfp_full_click::encode(
        container,
        state,
        -999,
        i8::from(right),
        if input == Input::Throw { 4 } else { 0 },
        &[],
        &carried,
    );
    game.send(horizon_protocol::packets::play::Serverbound::Unknown(
        horizon_protocol::vfp_full_click::INTERNAL_ID,
        data,
    ));
    ::core::ops::ControlFlow::Break(())
}
