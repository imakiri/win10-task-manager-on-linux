/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! The Meta key on KDE Plasma: KWin calls `Toggle` on the launcher's bus name directly
//! (`kwinrc` `[ModifierOnlyShortcuts] Meta`), and a D-Bus activation file starts a hidden
//! launcher on the first press. The previous Meta action is kept and restored on uninstall.

use std::path::PathBuf;
use std::process::Command;

use crate::instance::LAUNCHER_ID;

const MARK: &str = "# Managed by Task Manager";

/// `kwinrc` value: service,path,interface,method
fn meta_action() -> String {
    format!("{LAUNCHER_ID},/Launcher,{LAUNCHER_ID},Toggle")
}

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

fn home(var: &str, fallback: &str) -> Result<PathBuf, String> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
        .ok_or_else(|| "The home directory is unavailable.".into())
}

fn service_path() -> Result<PathBuf, String> {
    Ok(home("XDG_DATA_HOME", ".local/share")?
        .join("dbus-1/services")
        .join(format!("{LAUNCHER_ID}.service")))
}

/// Meta action that was set before installing; an empty file means "KWin's default".
fn backup_path() -> Result<PathBuf, String> {
    Ok(home("XDG_CONFIG_HOME", ".config")?.join("taskmanager/launcher-previous-meta"))
}

fn kwin_meta() -> Result<Option<String>, String> {
    let output = Command::new("kreadconfig6")
        .args([
            "--file",
            "kwinrc",
            "--group",
            "ModifierOnlyShortcuts",
            "--key",
            "Meta",
        ])
        .output()
        .map_err(|e| format!("kreadconfig6: {e}"))?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!value.is_empty()).then_some(value))
}

fn set_kwin_meta(value: Option<&str>) -> Result<(), String> {
    let mut command = Command::new("kwriteconfig6");
    command.args([
        "--file",
        "kwinrc",
        "--group",
        "ModifierOnlyShortcuts",
        "--key",
        "Meta",
    ]);
    match value {
        Some(value) => command.arg(value),
        None => command.arg("--delete"),
    };
    let status = command
        .status()
        .map_err(|e| format!("kwriteconfig6: {e}"))?;
    if !status.success() {
        return Err(format!("kwriteconfig6 failed: {status}"));
    }
    // KWin rereads kwinrc; without a running KWin it applies at the next login
    if let Ok(connection) = zbus::blocking::Connection::session()
        && let Err(error) = connection.call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "reconfigure",
            &(),
        )
    {
        log::warn!("Could not reconfigure KWin: {error}");
    }
    Ok(())
}

/// What to restore when uninstalling, from the backup file's contents.
fn previous(backup: &str) -> Option<&str> {
    let value = backup.trim();
    (!value.is_empty()).then_some(value)
}

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
    std::fs::write(&service, service_file(executable)).map_err(|e| e.to_string())?;

    let current = kwin_meta()?;
    let action = meta_action();
    if current.as_deref() != Some(action.as_str()) {
        let backup = backup_path()?;
        std::fs::create_dir_all(backup.parent().unwrap_or(&backup)).map_err(|e| e.to_string())?;
        std::fs::write(&backup, current.unwrap_or_default()).map_err(|e| e.to_string())?;
        set_kwin_meta(Some(&action))?;
    }
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    if kwin_meta()?.as_deref() == Some(meta_action().as_str()) {
        let backup = backup_path()?;
        let saved = std::fs::read_to_string(&backup).unwrap_or_default();
        set_kwin_meta(previous(&saved))?;
        let _ = std::fs::remove_file(backup);
    }
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
    fn meta_calls_toggle_on_the_launcher() {
        assert_eq!(
            meta_action(),
            "io.github.taskmanager.TaskManager.Launcher,/Launcher,\
             io.github.taskmanager.TaskManager.Launcher,Toggle"
        );
    }

    #[test]
    fn activation_starts_hidden_with_a_quoted_path() {
        let file = service_file("/opt/Task Manager/it's/taskmanager-launcher");
        assert!(file.starts_with(MARK));
        assert!(file.contains("Name=io.github.taskmanager.TaskManager.Launcher\n"));
        assert!(file.contains("Exec='/opt/Task Manager/it'\\''s/taskmanager-launcher' --hidden\n"));
    }

    #[test]
    fn empty_backup_restores_kwin_default() {
        assert_eq!(previous(""), None);
        assert_eq!(previous("\n"), None);
        assert_eq!(
            previous("org.kde.plasmashell,/PlasmaShell,org.kde.PlasmaShell,activateLauncherMenu\n"),
            Some("org.kde.plasmashell,/PlasmaShell,org.kde.PlasmaShell,activateLauncherMenu")
        );
    }
}
