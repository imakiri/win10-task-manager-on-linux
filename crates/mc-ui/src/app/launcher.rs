/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Fullscreen launcher mode: only the tab strip and the pages, on the whole screen, with no title
//! bar, menu bar, "Fewer details" toggle or resize border. Pages switch as in the window
//! (mouse, Ctrl/Alt+1…4); Esc asks the native shell to hide the launcher.

use egui::{Context, Key, KeyboardShortcut, Modifiers, Rect, vec2};

use super::App;
use crate::win10::metrics;

#[derive(Default)]
pub(super) struct Launcher {
    hide_requested: bool,
}

impl App {
    /// Launcher mode: Esc asked to hide the launcher since the last call.
    pub fn take_hide_request(&mut self) -> bool {
        self.launcher
            .as_mut()
            .is_some_and(|launcher| std::mem::take(&mut launcher.hide_requested))
    }

    pub(super) fn launcher_shortcuts(&mut self, ctx: &Context) {
        // Dialogs keep Esc for themselves
        if self.launcher.is_none()
            || self.run_task.is_some()
            || self.hardware_notice
            || !self.dialogs.is_empty()
        {
            return;
        }
        let pressed = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if pressed(KeyboardShortcut::new(Modifiers::NONE, Key::Escape))
            && let Some(launcher) = &mut self.launcher
        {
            launcher.hide_requested = true;
        }
    }

    /// The rect the window chrome is laid out against: the launcher moves it up so the tab strip
    /// takes the place of the missing title and menu bars.
    pub(super) fn chrome_origin(&self, window: Rect) -> Rect {
        if self.launcher.is_some() {
            window.translate(vec2(0., -metrics::LAUNCHER_CHROME_SHIFT))
        } else {
            window
        }
    }
}
