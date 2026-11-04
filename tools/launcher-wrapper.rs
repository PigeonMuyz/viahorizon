// SPDX-License-Identifier: GPL-3.0-or-later
// Horizon's instance wrapper receives: original-game.exe, original arguments.
// Use the prebuilt modded client beside this wrapper, retaining the launcher's
// session-file handoff, environment, working directory and argument boundaries.
use std::{env, ffi::OsStr, path::Path, process::Command};

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let original = args
        .next()
        .ok_or("Run this through the Horizon launcher instance")?;
    if Path::new(&original).file_name() != Some(OsStr::new("horizon-client.exe")) {
        return Err("Expected Horizon's client command".into());
    }
    let executable = env::current_exe()?.with_file_name("horizon-client.exe");
    let mut command = Command::new(executable);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // No extra console; the game owns its window.
    }
    Ok(command.status()?.code().unwrap_or(1))
}

fn main() {
    // Arguments can contain private session-file paths; never print them.
    let code = run().unwrap_or_else(|_| {
        eprintln!(
            "Could not start the prebuilt ViaFabricPlus client. Check the instance wrapper path."
        );
        1
    });
    std::process::exit(code);
}
