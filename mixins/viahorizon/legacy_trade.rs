// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(game: &mut crate::game::Game<'_>, index: usize) -> ::core::ops::ControlFlow<()> {
    use crate::game::stations::INPUT;
    use horizon_client::menu::Input;
    use horizon_protocol::packets::play::{SelectTrade, Serverbound};

    if !game.remote.as_ref().is_some_and(|remote| {
        horizon_client::net::vfp_capture_session::legacy_protocol(&remote.session).is_some()
    }) || game.remote_menu().1 != "minecraft:merchant"
    {
        return ::core::ops::ControlFlow::Continue(());
    }

    let Some(merchant) = game
        .station
        .as_mut()
        .and_then(|station| station.merchant.as_mut())
    else {
        return ::core::ops::ControlFlow::Continue(());
    };
    let Some(offer) = merchant.session.offers.get(index).cloned() else {
        return ::core::ops::ControlFlow::Break(());
    };
    merchant.session.selection_hint = index;
    game.send(Serverbound::SelectTrade(SelectTrade { item: index as i32 }));
    if game.carried.is_some() || offer.out_of_stock() {
        return ::core::ops::ControlFlow::Break(());
    }

    // These servers accept the selection hint but leave payment slots to
    // ordinary clicks. Use the same predicted click path as manual trades.
    for slot in [INPUT, INPUT + 1] {
        if game.stack_in(slot).is_some() {
            game.remote_menu_ops(&[(Some(slot), 0, Input::QuickMove)]);
            if game.stack_in(slot).is_some() {
                return ::core::ops::ControlFlow::Break(());
            }
        }
    }
    let count_a = offer.cost_a_count(game.items());
    let costs = [
        Some((&offer.base_cost_a, count_a)),
        offer.cost_b.as_ref().map(|cost| (cost, cost.stack.count)),
    ];
    for (payment, cost) in costs.into_iter().enumerate() {
        let Some((cost, mut remaining)) = cost else {
            continue;
        };
        let payment = INPUT + payment;
        for slot in 0..36 {
            if remaining == 0 {
                break;
            }
            let Some(stack) = game.stack_in(slot).filter(|stack| cost.test(stack)) else {
                continue;
            };
            if game
                .stack_in(payment)
                .is_some_and(|current| !current.same_item_same_components(stack))
            {
                continue;
            }
            let amount = remaining.min(stack.count);
            let mut clicks = vec![(Some(slot), 0, Input::Pickup)];
            if amount == stack.count {
                clicks.push((Some(payment), 0, Input::Pickup));
            } else {
                clicks.extend((0..amount).map(|_| (Some(payment), 1, Input::Pickup)));
                clicks.push((Some(slot), 0, Input::Pickup));
            }
            if !game.remote_menu_ops(&clicks) || game.carried.is_some() {
                return ::core::ops::ControlFlow::Break(());
            }
            remaining -= amount;
        }
    }
    ::core::ops::ControlFlow::Break(())
}
