//! Install and uninstall the Claude Code integration.
//! Validates both settings and instructions before touching either file.

mod hooks;
mod prompt;

use crate::{commands::fail, storage, subprocess};
use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub fn run_init(uninstall: bool, with_repomix: bool) {
    configure(uninstall).unwrap_or_else(|error| fail(1, &error));
    if uninstall {
        println!("Claude Code integration uninstalled.");
    } else {
        println!("Claude Code integration installed.");
        check_repomix(with_repomix);
    }
}

fn configure(uninstall: bool) -> Result<(), String> {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or("HOME or USERPROFILE must be set")?;
    let root = PathBuf::from(home).join(".claude");
    if uninstall && !root.exists() {
        return Ok(());
    }
    let _lock = storage::lock(&root.join(".ai-summary.lock"))
        .map_err(|error| format!("cannot lock Claude configuration: {error}"))?;
    let settings = hooks::path_for_update(root.join("settings.json"))?;
    let instructions = hooks::path_for_update(root.join("CLAUDE.md"))?;
    let hook_dir = root.join("hooks");
    let planned_instructions = prompt::plan(&instructions, uninstall)?;
    let planned_settings = hooks::plan(&settings, &hook_dir, uninstall)?;
    if !uninstall {
        for (name, script, _) in hooks::SCRIPTS {
            write(&hook_dir.join(name), script.as_bytes(), 0o700)?;
        }
    }
    if let Some(bytes) = planned_settings {
        write(&settings, &bytes, 0o600)?;
    }
    if let Some(text) = planned_instructions {
        write(&instructions, text.as_bytes(), 0o600)?;
    }
    if uninstall {
        for (name, _, _) in hooks::SCRIPTS {
            match fs::remove_file(hook_dir.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(format!("cannot remove owned hook: {error}")),
            }
        }
    }
    Ok(())
}

fn write(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    storage::write_atomic(path, bytes, mode)
        .map_err(|error| format!("cannot save {}: {error}", path.display()))
}

fn check_repomix(install: bool) {
    if subprocess::available(Command::new("repomix").arg("--version")) {
        println!("Optional repomix dependency is installed.");
    } else if install {
        let output = subprocess::capture(
            Command::new("npm").args(["install", "-g", "repomix"]),
            None,
            Duration::from_secs(120),
            subprocess::CONTENT_LIMIT,
        );
        if !output.is_ok_and(|output| output.status.success()) {
            fail(
                1,
                "repomix installation failed; Claude integration is installed",
            );
        }
        println!("Installed optional repomix dependency.");
    }
}
