/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! D-Bus activation: a `Toggle` call to the launcher's bus name (the Plasma launcher widget, a
//! compositor key binding) starts a hidden launcher when none runs, which the call then shows.

use std::path::PathBuf;

use crate::instance::LAUNCHER_ID;

const MARK: &str = "# Managed by Task Manager";

/// D-Bus `.service` Exec lines are split like a shell command line.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn service_file(executable: &str) -> String {
    format!(
        "{MARK}\n[D-BUS Service]\nName={LAUNCHER_ID}\nExec={} --hidden\n",
        quote(executable)
    )
}

fn service_path() -> Result<PathBuf, String> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|data| {
            data.join("dbus-1/services")
                .join(format!("{LAUNCHER_ID}.service"))
        })
        .ok_or_else(|| "The home directory is unavailable.".into())
}

/// Registers this executable for the user's session bus.
pub fn install() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let executable = executable
        .to_str()
        .ok_or("The executable path is not UTF-8.")?;
    if executable.contains(['\n', '\r']) {
        return Err("The executable path contains a newline.".into());
    }
    let service = service_path()?;
    if let Ok(old) = std::fs::read_to_string(&service)
        && !old.starts_with(MARK)
    {
        return Err(format!("{} belongs to another program.", service.display()));
    }
    std::fs::create_dir_all(service.parent().unwrap_or(&service)).map_err(|e| e.to_string())?;
    std::fs::write(&service, service_file(executable)).map_err(|e| e.to_string())
}

pub fn uninstall() -> Result<(), String> {
    let service = service_path()?;
    if std::fs::read_to_string(&service).is_ok_and(|old| old.starts_with(MARK)) {
        std::fs::remove_file(&service).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_starts_hidden_with_a_quoted_path() {
        let file = service_file("/opt/Task Manager/it's/taskmanager-launcher");
        assert!(file.starts_with(MARK));
        assert!(file.contains("Name=io.github.taskmanager.TaskManager.Launcher\n"));
        assert!(file.contains("Exec='/opt/Task Manager/it'\\''s/taskmanager-launcher' --hidden\n"));
    }
}
