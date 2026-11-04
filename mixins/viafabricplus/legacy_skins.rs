// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(game: &mut crate::game::Game<'_>) {
    use horizon_client::skins::{self, NameOrId, Resolve};
    let Some(remote) = game.remote.as_mut() else {
        return;
    };
    if !matches!(
        horizon_client::net::vfp_capture_session::legacy_protocol(&remote.session),
        Some(4 | 5)
    ) {
        return;
    }
    let mut profiles = game.skin_profiles.borrow_mut();
    for (&uuid, entry) in &remote.tab_players {
        if uuid == remote.profile || remote.skin_textures.contains_key(&uuid) {
            continue;
        }
        let online_id = (uuid >> 76) & 15 == 4;
        let key = if online_id {
            NameOrId::Id(uuid)
        } else {
            if !(1..=16).contains(&entry.name.len())
                || !entry
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                continue;
            }
            NameOrId::Name(entry.name.clone())
        };
        if let Resolve::Done(Some(profile)) = profiles.state(&key) {
            if let Some((value, signature)) = profile.textures() {
                remote
                    .skin_textures
                    .insert(uuid, skins::unpack(&value, signature, &skins::TEXTURE_URIS));
            }
        }
    }
}
