/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! `taskmanager-launcher`: Task Manager as a fullscreen start-menu replacement on Wayland
//! (wlr-layer-shell). The first launch stays resident; later launches show or hide it.
//! Settings are read from the windowed Task Manager's `config.toml` and never written.

#[cfg(target_os = "linux")]
mod instance;
#[cfg(target_os = "linux")]
mod kde;
#[cfg(target_os = "linux")]
mod layer;

/// Same cap as the windowed app (see `apps/taskmanager/src/main.rs`): egui would otherwise size
/// its font atlas to the GPU's largest texture.
#[cfg(target_os = "linux")]
const MAX_TEXTURE_SIDE: usize = 2048;

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("taskmanager-launcher needs a Wayland compositor with wlr-layer-shell");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn main() {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use mc_core::config::Config;
    use mc_ui::{AlwaysOnTopMode, App, AppOptions, DataSource};

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let mut synthetic = None;
    let mut fixture = None;
    let mut no_config = false;
    let mut perf_overlay = false;
    let mut hidden = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--synthetic" => synthetic = args.next().and_then(|n| n.parse().ok()),
            "--fixture" => fixture = args.next().map(PathBuf::from),
            "--no-config" => no_config = true,
            "--perf-overlay" => perf_overlay = true,
            // Started by D-Bus activation: the Toggle call that started it shows it
            "--hidden" => hidden = true,
            "--install-meta" | "--uninstall-meta" => {
                let result = if arg == "--install-meta" {
                    kde::install()
                } else {
                    kde::uninstall()
                };
                if let Err(error) = result {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
                return;
            }
            _ => {
                println!(
                    "taskmanager-launcher [--synthetic PROCESSES] [--fixture DIR] [--no-config] [--perf-overlay]\n\
                     \x20                    [--hidden] [--install-meta | --uninstall-meta]"
                );
                std::process::exit(if matches!(arg.as_str(), "-h" | "--help") {
                    0
                } else {
                    2
                });
            }
        }
    }

    let instance = match instance::Instance::claim() {
        Ok(Some(instance)) => Some(instance),
        Ok(None) => return,
        Err(instance::ClaimError::BusUnavailable(error)) => {
            log::warn!("Launcher activation unavailable: {error}");
            None
        }
        Err(instance::ClaimError::Failed(error)) => {
            eprintln!("Could not activate the launcher: {error}");
            std::process::exit(1);
        }
    };

    let locale_dir = std::env::var_os("MC_LOCALEDIR")
        .map(PathBuf::from)
        .or_else(|| option_env!("MC_LOCALEDIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/usr/share/locale"));
    mc_core::i18n::init(&locale_dir);
    let source = match (synthetic, &fixture) {
        (Some(processes), _) => {
            DataSource::Fixture(mc_magpie::synthetic_series(60, processes), HashMap::new())
        }
        (None, Some(dir)) => match mc_magpie::load_fixture_dir(dir) {
            Ok((readings, icons)) => DataSource::Fixture(readings, icons),
            Err(error) => {
                eprintln!("failed to load fixture {}: {error}", dir.display());
                std::process::exit(1);
            }
        },
        (None, None) => DataSource::Magpie,
    };
    let initial_config = (!no_config)
        .then(Config::default_path)
        .as_deref()
        .and_then(Config::load);
    let fonts = mc_ui::win10::fonts::Fonts::discover();

    let result = layer::run(
        move |ctx| {
            App::new(
                ctx,
                AppOptions {
                    source,
                    config_path: None,
                    perf_overlay,
                    fonts,
                    initial_config,
                    always_on_top_mode: AlwaysOnTopMode::Unavailable,
                    fullscreen: true,
                },
            )
        },
        instance.map(|instance| Box::new(instance) as Box<dyn layer::Activation>),
        MAX_TEXTURE_SIDE,
        hidden,
    );
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
