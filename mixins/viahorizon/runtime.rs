// SPDX-License-Identifier: GPL-3.0-or-later
use super::Game;
use horizon_protocol::vfp_core as core;
use std::{
    cell::RefCell,
    sync::mpsc::{self, Receiver},
    time::Duration,
};

#[derive(Default)]
struct Ui {
    loaded: bool,
    config: core::Config,
    error: Option<String>,
    result: String,
    probe: Option<Receiver<String>>,
    applied: Option<core::Config>,
}
thread_local! { static UI: RefCell<Ui> = RefCell::new(Ui::default()); }
fn path(game: &Game<'_>) -> std::path::PathBuf {
    game.native_data_dir(core::ID).join("config.json")
}
fn load(game: &mut Game<'_>, ui: &mut Ui) {
    let previous = game.native_data_dir(core::LEGACY_ID).join("config.json");
    match core::Config::load_migrating(&path(game), &previous) {
        Ok((config, migrated)) => {
            if migrated
                && game.settings.mods.disabled.contains(core::LEGACY_ID)
                && !game.settings.mods.enabled.contains(core::ID)
            {
                game.settings.mods.disabled.insert(core::ID.into());
                game.settings.save(&Game::options_path());
            }
            ui.config = config;
            ui.error = None;
        }
        Err(e) => {
            ui.config = core::Config::default();
            ui.error = Some(format!("Config error: {e}"));
        }
    }
    ui.loaded = true;
}
fn apply(game: &Game<'_>, ui: &mut Ui) {
    ui.config.enabled = game.native_on(core::ID) && game.native_window();
    if ui.applied.as_ref() != Some(&ui.config) {
        core::configure(ui.config.clone());
        ui.applied = Some(ui.config.clone());
    }
}
fn save(game: &Game<'_>, ui: &mut Ui) -> core::Result<()> {
    let result = ui.config.save(&path(game));
    ui.error = result
        .as_ref()
        .err()
        .map(|e| format!("Could not save config: {e}"));
    apply(game, ui);
    result
}
pub(super) fn sync(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    horizon_client::net::vfp_capture_session::activate(game.remote.as_ref().map(|r| &r.session));
    horizon_client_render::vfp_render_compat::set_legacy(game.remote.as_ref().is_some_and(|r| {
        matches!(
            horizon_client::net::vfp_capture_session::protocol(&r.session),
            Some(4 | 5 | 47)
        )
    }));
    UI.with(|cell| {
        let mut ui = cell.borrow_mut();
        if !ui.loaded {
            load(game, &mut ui);
        }
        apply(game, &mut ui);
        let message = ui.probe.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(message) = message {
            ui.probe = None;
            ui.result = message.clone();
            game.native_chat(&format!("[ViaHorizon] {message}"));
        }
    });
}
pub(super) fn settings(game: &mut Game<'_>, buttons: &mut Vec<String>) {
    sync(game);
    UI.with(|cell| {
        let ui = cell.borrow();
        buttons.push(format!("Server target: {}", ui.config.target.label()));
        buttons.push(format!(
            "Configuration preview (no world entry): {}",
            if ui.config.login_preview { "ON" } else { "OFF" }
        ));
        buttons.push("Reload config.json".into());
        buttons.push(if ui.probe.is_some() {
            "Checking first saved server...".into()
        } else {
            "Check first saved server".into()
        });
        if let Some(error) = &ui.error {
            buttons.push(error.clone());
        } else if !ui.result.is_empty() {
            buttons.push(ui.result.clone());
        }
    });
}
pub(super) fn click(game: &mut Game<'_>, button: usize) {
    sync(game);
    UI.with(|cell| {
        let mut ui = cell.borrow_mut();
        match button {
            0 => {
                ui.config.target = ui.config.target.next();
                let _ = save(game, &mut ui);
            }
            1 => {
                ui.config.login_preview = !ui.config.login_preview;
                let _ = save(game, &mut ui);
            }
            2 => {
                load(game, &mut ui);
                apply(game, &mut ui);
            }
            3 if ui.probe.is_none() => {
                let servers = horizon_protocol::servers::load(
                    &Game::options_path().with_file_name("servers.dat"),
                );
                if let Some(server) = servers.first() {
                    start_probe(&mut ui, server.ip.clone());
                } else {
                    ui.result = "No saved servers".into();
                }
            }
            _ => (),
        }
    });
}
fn start_probe(ui: &mut Ui, address: String) {
    let (tx, rx) = mpsc::channel();
    match std::thread::Builder::new()
        .name("vfp-status".into())
        .spawn(move || {
            let text = match horizon_client::net::ping(&address, Duration::from_secs(3)) {
                Ok((j, _)) => match j["version"]["protocol"]
                    .as_i64()
                    .and_then(|p| i32::try_from(p).ok())
                {
                    Some(protocol) => format!(
                        "{}: {} ({protocol}), {}",
                        address,
                        j["version"]["name"].as_str().unwrap_or("unknown"),
                        core::modern_protocol(protocol)
                            .map_or("not implemented", |r| r.stage.label())
                    ),
                    None => format!("{address}: missing server protocol"),
                },
                Err(e) => format!("{address}: {e}"),
            };
            let _ = tx.send(text);
        }) {
        Ok(_) => ui.probe = Some(rx),
        Err(e) => ui.error = Some(e.to_string()),
    }
}
pub(super) fn command(game: &mut Game<'_>, command: &str) -> bool {
    let mut words = command.split_whitespace();
    if !matches!(
        words.next(),
        Some("vh" | "viahorizon" | "vfp" | "viafabricplus")
    ) || !game.native_on(core::ID)
    {
        return false;
    }
    sync(game);
    let args: Vec<_> = words.collect();
    let result: core::Result<String> = UI.with(|cell| { let mut ui = cell.borrow_mut();
        match args.as_slice() {
            [] | ["status"] => Ok(format!("Server target: {}; configuration preview {}.",ui.config.target.label(),if ui.config.login_preview { "ON (no world entry)" } else { "OFF" })),
            ["target",target] => { ui.config.target = core::Selection::parse(target)?; save(game,&mut ui)?; Ok(format!("Next connection: {}",ui.config.target.label())) },
            ["server",address,target] => { let key = core::server_key(address)?; let target = core::Selection::parse(target)?; ui.config.servers.insert(key.clone(),target); save(game,&mut ui)?; Ok(format!("Saved target for {key}")) },
            ["forget",address] => { ui.config.servers.remove(&core::server_key(address)?); save(game,&mut ui)?; Ok("Server override removed".into()) },
            ["preview",setting] if ["on","off"].contains(setting) => { ui.config.login_preview = *setting == "on"; save(game,&mut ui)?; Ok(if ui.config.login_preview { "Configuration preview enabled; world entry is disabled" } else { "Configuration preview disabled" }.into()) },
            ["reload"] => { load(game,&mut ui); apply(game,&mut ui); if let Some(e) = &ui.error { return Err(e.clone()); } Ok("Configuration reloaded".into()) },
            ["probe",address] => { if ui.probe.is_some() { return Err("A probe is already running".into()); } core::address(address)?; start_probe(&mut ui,(*address).into()); Ok("Checking server...".into()) },
            _ => Ok("/vh status | target native/auto/VERSION | server ADDRESS TARGET | forget ADDRESS | preview on/off | probe ADDRESS | reload".into()),
        }
    });
    game.native_chat(&format!("[ViaHorizon] {}", result.unwrap_or_else(|e| e)));
    true
}
