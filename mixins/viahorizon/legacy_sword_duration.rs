// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    duration: &mut i32,
) {
    if horizon_client::net::vfp_capture_session::legacy_sword(&stack.item)
        && game.remote.as_ref().is_some_and(|r| {
            matches!(
                horizon_client::net::vfp_capture_session::legacy_protocol(&r.session),
                Some(4 | 5 | 47)
            )
        })
    {
        *duration = 72_000;
    }
}
