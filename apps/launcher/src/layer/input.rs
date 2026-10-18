/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Wayland input → egui events for the launcher surface.

use egui::{CursorIcon, Key, Modifiers, PointerButton, Vec2, vec2};
use smithay_client_toolkit::seat::keyboard::{self, Keysym};
use smithay_client_toolkit::seat::pointer::{AxisScroll, CursorIcon as WlCursor};

/// Linux input event codes (linux/input-event-codes.h)
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const BTN_SIDE: u32 = 0x113;
const BTN_EXTRA: u32 = 0x114;

pub fn modifiers(m: keyboard::Modifiers) -> Modifiers {
    Modifiers {
        alt: m.alt,
        ctrl: m.ctrl,
        shift: m.shift,
        mac_cmd: false,
        command: m.ctrl,
    }
}

pub fn button(code: u32) -> Option<PointerButton> {
    Some(match code {
        BTN_LEFT => PointerButton::Primary,
        BTN_RIGHT => PointerButton::Secondary,
        BTN_MIDDLE => PointerButton::Middle,
        BTN_SIDE => PointerButton::Extra1,
        BTN_EXTRA => PointerButton::Extra2,
        _ => return None,
    })
}

/// Wheel steps (high-resolution when available) or pixels; egui scrolls content up for a positive
/// delta, Wayland reports scrolling down as positive.
pub fn scroll(horizontal: AxisScroll, vertical: AxisScroll) -> (egui::MouseWheelUnit, Vec2) {
    let steps = |axis: AxisScroll| {
        if axis.value120 != 0 {
            axis.value120 as f32 / 120.
        } else {
            axis.discrete as f32
        }
    };
    if steps(horizontal) != 0. || steps(vertical) != 0. {
        (
            egui::MouseWheelUnit::Line,
            -vec2(steps(horizontal), steps(vertical)),
        )
    } else {
        (
            egui::MouseWheelUnit::Point,
            -vec2(horizontal.absolute as f32, vertical.absolute as f32),
        )
    }
}

/// Text typed by a key press; shortcuts and control characters produce none.
pub fn text(utf8: Option<&str>, modifiers: Modifiers) -> Option<&str> {
    let text = utf8?;
    (!modifiers.ctrl
        && !modifiers.command
        && !modifiers.alt
        && !text.is_empty()
        && !text.chars().any(char::is_control))
    .then_some(text)
}

pub fn key(keysym: Keysym) -> Option<Key> {
    use Keysym as K;
    Some(match keysym {
        K::Escape => Key::Escape,
        K::Tab | K::ISO_Left_Tab => Key::Tab,
        K::BackSpace => Key::Backspace,
        K::Return | K::KP_Enter => Key::Enter,
        K::space | K::KP_Space => Key::Space,
        K::Insert | K::KP_Insert => Key::Insert,
        K::Delete | K::KP_Delete => Key::Delete,
        K::Home | K::KP_Home => Key::Home,
        K::End | K::KP_End => Key::End,
        K::Page_Up | K::KP_Page_Up => Key::PageUp,
        K::Page_Down | K::KP_Page_Down => Key::PageDown,
        K::Left | K::KP_Left => Key::ArrowLeft,
        K::Right | K::KP_Right => Key::ArrowRight,
        K::Up | K::KP_Up => Key::ArrowUp,
        K::Down | K::KP_Down => Key::ArrowDown,
        K::minus | K::KP_Subtract => Key::Minus,
        K::plus | K::KP_Add => Key::Plus,
        K::equal | K::KP_Equal => Key::Equals,
        K::slash | K::KP_Divide => Key::Slash,
        K::question => Key::Questionmark,
        K::comma => Key::Comma,
        K::period => Key::Period,
        K::F1 => Key::F1,
        K::F2 => Key::F2,
        K::F3 => Key::F3,
        K::F4 => Key::F4,
        K::F5 => Key::F5,
        K::F6 => Key::F6,
        K::F7 => Key::F7,
        K::F8 => Key::F8,
        K::F9 => Key::F9,
        K::F10 => Key::F10,
        K::F11 => Key::F11,
        K::F12 => Key::F12,
        _ => {
            // Letters and digits by their (Latin) name; other layouts send Latin keysyms with
            // Ctrl only when the layout has them, as other toolkits do
            let name = keysym.key_char()?;
            let name = name.to_ascii_uppercase().to_string();
            if name.len() != 1 || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
                return None;
            }
            Key::from_name(&name)?
        }
    })
}

pub fn cursor(icon: CursorIcon) -> Option<WlCursor> {
    use CursorIcon as C;
    Some(match icon {
        C::None => return None,
        C::Default => WlCursor::Default,
        C::ContextMenu => WlCursor::ContextMenu,
        C::Help => WlCursor::Help,
        C::PointingHand => WlCursor::Pointer,
        C::Progress => WlCursor::Progress,
        C::Wait => WlCursor::Wait,
        C::Cell => WlCursor::Cell,
        C::Crosshair => WlCursor::Crosshair,
        C::Text => WlCursor::Text,
        C::VerticalText => WlCursor::VerticalText,
        C::Alias => WlCursor::Alias,
        C::Copy => WlCursor::Copy,
        C::Move => WlCursor::Move,
        C::NoDrop => WlCursor::NoDrop,
        C::NotAllowed => WlCursor::NotAllowed,
        C::Grab => WlCursor::Grab,
        C::Grabbing => WlCursor::Grabbing,
        C::AllScroll => WlCursor::AllScroll,
        C::ResizeHorizontal => WlCursor::EwResize,
        C::ResizeNeSw => WlCursor::NeswResize,
        C::ResizeNwSe => WlCursor::NwseResize,
        C::ResizeVertical => WlCursor::NsResize,
        C::ResizeEast => WlCursor::EResize,
        C::ResizeSouthEast => WlCursor::SeResize,
        C::ResizeSouth => WlCursor::SResize,
        C::ResizeSouthWest => WlCursor::SwResize,
        C::ResizeWest => WlCursor::WResize,
        C::ResizeNorthWest => WlCursor::NwResize,
        C::ResizeNorth => WlCursor::NResize,
        C::ResizeNorthEast => WlCursor::NeResize,
        C::ResizeColumn => WlCursor::ColResize,
        C::ResizeRow => WlCursor::RowResize,
        C::ZoomIn => WlCursor::ZoomIn,
        C::ZoomOut => WlCursor::ZoomOut,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_keys_are_mapped() {
        assert_eq!(key(Keysym::Escape), Some(Key::Escape));
        assert_eq!(key(Keysym::Tab), Some(Key::Tab));
        // Shift+Tab arrives as ISO_Left_Tab
        assert_eq!(key(Keysym::ISO_Left_Tab), Some(Key::Tab));
        assert_eq!(key(Keysym::Delete), Some(Key::Delete));
    }

    #[test]
    fn letters_and_digits_map_regardless_of_case() {
        assert_eq!(key(Keysym::q), Some(Key::Q));
        assert_eq!(key(Keysym::Q), Some(Key::Q));
        assert_eq!(key(Keysym::_1), Some(Key::Num1));
        assert_eq!(key(Keysym::Cyrillic_a), None);
    }

    #[test]
    fn text_skips_shortcuts_and_control_characters() {
        assert_eq!(text(Some("a"), Modifiers::NONE), Some("a"));
        assert_eq!(text(Some("а"), Modifiers::SHIFT), Some("а"));
        assert_eq!(text(Some("c"), Modifiers::COMMAND), None);
        assert_eq!(text(Some("\u{1b}"), Modifiers::NONE), None);
        assert_eq!(text(Some("\t"), Modifiers::NONE), None);
    }

    #[test]
    fn wheel_down_scrolls_content_up() {
        let none = AxisScroll::default();
        let down = AxisScroll {
            value120: 120,
            discrete: 1,
            absolute: 15.,
            ..Default::default()
        };
        assert_eq!(
            scroll(none, down),
            (egui::MouseWheelUnit::Line, vec2(0., -1.))
        );
        let touchpad = AxisScroll {
            absolute: 7.5,
            ..Default::default()
        };
        assert_eq!(
            scroll(none, touchpad),
            (egui::MouseWheelUnit::Point, vec2(0., -7.5))
        );
    }

    #[test]
    fn buttons() {
        assert_eq!(button(BTN_LEFT), Some(PointerButton::Primary));
        assert_eq!(button(BTN_RIGHT), Some(PointerButton::Secondary));
        assert_eq!(button(0x200), None);
    }
}
