// SPDX-License-Identifier: GPL-3.0-or-later
fn prepare_chat(
    auth: Option<&crate::auth::AuthSession>,
    local: &Option<horizon_protocol::connection::Pipe>,
) {
    use horizon_protocol::{vfp_chat_signing, vfp_core};
    vfp_chat_signing::prepare(None);
    let config = vfp_core::configuration();
    if local.is_some() || !config.enabled || config.login_preview {
        return;
    }
    if let Some(auth) = auth {
        match crate::auth::vfp_certificates::get(auth) {
            Ok(identity) => vfp_chat_signing::prepare(Some(identity)),
            Err(error) => eprintln!("ViaHorizon chat: {error}"),
        }
    }
}
