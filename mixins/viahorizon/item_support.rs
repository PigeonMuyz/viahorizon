// SPDX-License-Identifier: GPL-3.0-or-later
use super::Game;
use horizon_client::items::ItemStack;
use std::{cell::RefCell, collections::BTreeMap};
thread_local! { static ITEMS: RefCell<BTreeMap<(i32, String), bool>> = const { RefCell::new(BTreeMap::new()) }; }
pub(super) fn supported(game: &Game<'_>, protocol: i32, stack: &Option<ItemStack>) -> bool {
    let Some(stack) = stack else { return true };
    let key = (protocol, stack.item.to_string());
    ITEMS.with_borrow_mut(|items| {
        *items.entry(key).or_insert_with(|| {
            game.to_network(&Some(stack.clone())).is_some_and(|s| {
                horizon_protocol::vfp_release_chain::native_item_supported(protocol, s.item)
            })
        })
    })
}
pub(super) fn sanitize(game: &mut Game<'_>) -> bool {
    let Some((protocol, previous)) = game.remote.as_ref().and_then(|r| {
        horizon_client::net::vfp_capture_session::protocol(&r.session)
            .map(|p| (p, r.synced.clone()))
    }) else {
        return false;
    };
    if protocol == 777 {
        return false;
    }
    let invalid: Vec<_> = game
        .inventory
        .iter()
        .enumerate()
        .filter(|(_, s)| !supported(game, protocol, s))
        .map(|(i, _)| i)
        .collect();
    let armor: Vec<_> = game
        .armor
        .iter()
        .enumerate()
        .filter(|(_, s)| !supported(game, protocol, s))
        .map(|(i, _)| i)
        .collect();
    let offhand =
        !supported(game, protocol, &game.offhand) || protocol <= 47 && game.offhand.is_some();
    let carried = !supported(game, protocol, &game.carried);
    let changed = !invalid.is_empty() || !armor.is_empty() || offhand || carried;
    for i in invalid {
        let old = previous.get(i).cloned().flatten();
        game.inventory[i] = if supported(game, protocol, &old) {
            old
        } else {
            None
        };
    }
    for i in armor {
        let old = previous.get(37 + i).cloned().flatten();
        game.armor[i] = if supported(game, protocol, &old) {
            old
        } else {
            None
        };
    }
    if offhand {
        game.offhand = None;
    }
    if carried {
        game.carried = None;
    }
    if changed {
        game.native_chat("[ViaHorizon] That item is unavailable on this server version.");
    }
    changed
}
