// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    session: &crate::auth::AuthSession,
    server_hash: &str,
) -> ::core::ops::ControlFlow<Result<(), JoinError>> {
    if !horizon_protocol::vfp_core::configuration().enabled {
        return ::core::ops::ControlFlow::Continue(());
    }
    let mut result = Err(crate::auth::JoinError::Unavailable);
    for delay in [0, 500, 1500] {
        if delay != 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        result = crate::auth::join_server_once(session, server_hash);
        if !matches!(result, Err(crate::auth::JoinError::Unavailable)) {
            break;
        }
    }
    ::core::ops::ControlFlow::Break(result)
}
