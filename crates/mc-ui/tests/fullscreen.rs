/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Fullscreen launcher mode: tabs and content only, Esc hides.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use mc_core::config::Config;
use mc_ui::{AlwaysOnTopMode, App, AppOptions, DataSource};

fn harness(page: &str) -> Harness<'static, Option<App>> {
    let page = page.to_owned();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1920., 1080.))
        .build_ui_state(
            move |ui, app: &mut Option<App>| {
                let Some(app) = app else {
                    *app = Some(App::new(
                        ui.ctx(),
                        AppOptions {
                            source: DataSource::Fixture(
                                mc_magpie::synthetic_series(5, 20),
                                Default::default(),
                            ),
                            config_path: None,
                            perf_overlay: false,
                            fonts: Default::default(),
                            initial_config: Some(Config {
                                window_selected_page: page.clone(),
                                ..Default::default()
                            }),
                            always_on_top_mode: AlwaysOnTopMode::Viewport,
                            fullscreen: true,
                        },
                    ));
                    // Fonts installed by `App::new` are bound from the next pass on
                    ui.ctx().request_repaint();
                    return;
                };
                app.logic(&ui.ctx().clone());
                app.ui(ui);
            },
            None,
        );
    harness.run_steps(3);
    harness
}

fn page(harness: &Harness<'_, Option<App>>) -> String {
    harness
        .state()
        .as_ref()
        .unwrap()
        .config()
        .window_selected_page
        .clone()
}

#[test]
fn escape_asks_to_hide_once() {
    let mut harness = harness("apps-page");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    let app = harness.state_mut().as_mut().unwrap();
    assert!(app.take_hide_request());
    assert!(!app.take_hide_request());
}

#[test]
fn escape_closes_an_open_dialog_instead_of_hiding() {
    let mut harness = harness("apps-page");
    harness
        .state_mut()
        .as_mut()
        .unwrap()
        .open_dialog_for_test(None);
    harness.run_steps(2);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state_mut().as_mut().unwrap().take_hide_request());
}

/// Clicks at `pos`; returns the viewport commands the click produced.
fn click(harness: &mut Harness<'_, Option<App>>, pos: egui::Pos2) -> Vec<egui::ViewportCommand> {
    let mut commands = vec![];
    let mut step = |harness: &mut Harness<'_, Option<App>>| {
        harness.step();
        commands.extend(
            harness
                .output()
                .viewport_output
                .values()
                .flat_map(|viewport| viewport.commands.clone()),
        );
    };
    harness.hover_at(pos);
    step(harness);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        step(harness);
    }
    step(harness);
    commands
}

#[test]
fn tab_strip_replaces_title_and_menu_bars() {
    let mut harness = harness("apps-page");
    // Second tab ("Performance"), where the windowed mode has its title bar
    click(&mut harness, egui::pos2(100., 15.));
    assert_eq!(page(&harness), "performance-page");
}

#[test]
fn no_window_caption_buttons() {
    let mut harness = harness("apps-page");
    let commands = click(&mut harness, egui::pos2(1910., 10.));
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Close))
    );
}
