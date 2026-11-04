// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    game: &mut crate::game::Game<'_>,
    ops: &[(Option<usize>, i32, horizon_client::menu::Input)],
) -> ::core::ops::ControlFlow<bool> {
    use horizon_client::menu::Input;
    use horizon_client::net::vfp_capture_session::legacy_protocol;

    let Some(protocol @ (4 | 5 | 47)) = game
        .remote
        .as_ref()
        .and_then(|remote| legacy_protocol(&remote.session))
    else {
        return ::core::ops::ControlFlow::Continue(());
    };
    let (_, menu) = game.remote_menu();
    let virtual_slot = match menu.as_str() {
        "inventory" => Some(45),
        "minecraft:brewing_stand" => Some(4),
        "minecraft:enchantment" if protocol <= 5 => Some(1),
        _ => None,
    };
    let allowed = |&(slot, button, input): &(Option<usize>, i32, Input)| {
        if input == Input::Swap && button == 40 {
            return false;
        }
        match virtual_slot {
            Some(virtual_slot) => {
                slot.and_then(|index| crate::game::Game::menu_slot_index(&menu, index))
                    != Some(virtual_slot)
            }
            None => true,
        }
    };
    if ops.iter().all(allowed) {
        return ::core::ops::ControlFlow::Continue(());
    }
    let filtered: Vec<_> = ops.iter().filter(|op| allowed(op)).copied().collect();
    ::core::ops::ControlFlow::Break(filtered.is_empty() || game.remote_menu_ops(&filtered))
}
