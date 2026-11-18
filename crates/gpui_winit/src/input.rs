//! Translation of winit input into GPUI input: keystrokes, modifiers, mouse buttons and click counting.

use std::time::{Duration, Instant};

use gpui::{Capslock, DevicePixels, Keystroke, Modifiers, MouseButton, NavigationDirection, Point};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        GetDoubleClickTime, GetKeyState, VIRTUAL_KEY, VK_CAPITAL, VK_CONTROL, VK_LWIN, VK_MENU,
        VK_RWIN, VK_SHIFT,
    },
    WindowsAndMessaging::{GetSystemMetrics, SM_CXDOUBLECLK, SM_CYDOUBLECLK},
};
use winit::{
    event::{KeyEvent, MouseButton as WinitMouseButton},
    keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey},
    platform::modifier_supplement::KeyEventExtModifierSupplement as _,
};

pub(crate) fn modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        control: state.control_key(),
        alt: state.alt_key(),
        shift: state.shift_key(),
        platform: state.super_key(),
        function: false,
    }
}

pub(crate) fn current_modifiers() -> Modifiers {
    let pressed = |key: VIRTUAL_KEY| unsafe { GetKeyState(key.0 as i32) } < 0;
    Modifiers {
        control: pressed(VK_CONTROL),
        alt: pressed(VK_MENU),
        shift: pressed(VK_SHIFT),
        platform: pressed(VK_LWIN) || pressed(VK_RWIN),
        function: false,
    }
}

pub(crate) fn capslock() -> Capslock {
    Capslock {
        on: unsafe { GetKeyState(VK_CAPITAL.0 as i32) & 1 } > 0,
    }
}

pub(crate) fn mouse_button(button: WinitMouseButton) -> Option<MouseButton> {
    match button {
        WinitMouseButton::Left => Some(MouseButton::Left),
        WinitMouseButton::Right => Some(MouseButton::Right),
        WinitMouseButton::Middle => Some(MouseButton::Middle),
        WinitMouseButton::Back => Some(MouseButton::Navigate(NavigationDirection::Back)),
        WinitMouseButton::Forward => Some(MouseButton::Navigate(NavigationDirection::Forward)),
        WinitMouseButton::Other(_) => None,
    }
}

/// A keystroke and whether character input takes precedence over key bindings for it.
#[derive(Debug, PartialEq)]
pub(crate) struct TranslatedKey {
    pub(crate) keystroke: Keystroke,
    pub(crate) prefer_character_input: bool,
}

/// Translates a key event, or returns `None` for keys GPUI does not model, such as bare modifiers.
pub(crate) fn translate_key(event: &KeyEvent, modifiers: Modifiers) -> Option<TranslatedKey> {
    let key = key_name(&event.key_without_modifiers(), &event.physical_key)?;
    let key_char = event
        .text_with_all_modifiers()
        .and_then(printable)
        .map(str::to_owned);
    Some(build_keystroke(key, key_char, modifiers))
}

fn build_keystroke(key: String, key_char: Option<String>, modifiers: Modifiers) -> TranslatedKey {
    let prefer_character_input =
        key_char.is_some() && modifiers.control && modifiers.alt && !modifiers.platform;
    TranslatedKey {
        keystroke: Keystroke {
            modifiers,
            key,
            key_char,
        },
        prefer_character_input,
    }
}

/// Whether a keystroke's character should be typed when the keystroke itself was not handled.
pub(crate) fn types_character(translated: &TranslatedKey) -> bool {
    let modifiers = translated.keystroke.modifiers;
    translated.keystroke.key_char.is_some()
        && (translated.prefer_character_input
            || !(modifiers.control || modifiers.alt || modifiers.platform))
}

fn printable(text: &str) -> Option<&str> {
    (!text.is_empty() && !text.chars().any(char::is_control)).then_some(text)
}

fn key_name(key: &Key, physical: &PhysicalKey) -> Option<String> {
    match key {
        Key::Named(named) => named_key(*named).map(str::to_owned),
        Key::Character(text) => Some(text.to_lowercase()),
        Key::Dead(Some(character)) => Some(character.to_lowercase().collect()),
        Key::Dead(None) | Key::Unidentified(_) => physical_key(physical).map(str::to_owned),
    }
}

fn named_key(key: NamedKey) -> Option<&'static str> {
    Some(match key {
        NamedKey::Space => "space",
        NamedKey::Backspace => "backspace",
        NamedKey::Enter => "enter",
        NamedKey::Tab => "tab",
        NamedKey::ArrowUp => "up",
        NamedKey::ArrowDown => "down",
        NamedKey::ArrowRight => "right",
        NamedKey::ArrowLeft => "left",
        NamedKey::Home => "home",
        NamedKey::End => "end",
        NamedKey::PageUp => "pageup",
        NamedKey::PageDown => "pagedown",
        NamedKey::BrowserBack => "back",
        NamedKey::BrowserForward => "forward",
        NamedKey::Escape => "escape",
        NamedKey::Insert => "insert",
        NamedKey::Delete => "delete",
        NamedKey::ContextMenu => "menu",
        NamedKey::F1 => "f1",
        NamedKey::F2 => "f2",
        NamedKey::F3 => "f3",
        NamedKey::F4 => "f4",
        NamedKey::F5 => "f5",
        NamedKey::F6 => "f6",
        NamedKey::F7 => "f7",
        NamedKey::F8 => "f8",
        NamedKey::F9 => "f9",
        NamedKey::F10 => "f10",
        NamedKey::F11 => "f11",
        NamedKey::F12 => "f12",
        NamedKey::F13 => "f13",
        NamedKey::F14 => "f14",
        NamedKey::F15 => "f15",
        NamedKey::F16 => "f16",
        NamedKey::F17 => "f17",
        NamedKey::F18 => "f18",
        NamedKey::F19 => "f19",
        NamedKey::F20 => "f20",
        NamedKey::F21 => "f21",
        NamedKey::F22 => "f22",
        NamedKey::F23 => "f23",
        NamedKey::F24 => "f24",
        _ => return None,
    })
}

fn physical_key(key: &PhysicalKey) -> Option<&'static str> {
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    Some(match code {
        KeyCode::KeyA => "a",
        KeyCode::KeyB => "b",
        KeyCode::KeyC => "c",
        KeyCode::KeyD => "d",
        KeyCode::KeyE => "e",
        KeyCode::KeyF => "f",
        KeyCode::KeyG => "g",
        KeyCode::KeyH => "h",
        KeyCode::KeyI => "i",
        KeyCode::KeyJ => "j",
        KeyCode::KeyK => "k",
        KeyCode::KeyL => "l",
        KeyCode::KeyM => "m",
        KeyCode::KeyN => "n",
        KeyCode::KeyO => "o",
        KeyCode::KeyP => "p",
        KeyCode::KeyQ => "q",
        KeyCode::KeyR => "r",
        KeyCode::KeyS => "s",
        KeyCode::KeyT => "t",
        KeyCode::KeyU => "u",
        KeyCode::KeyV => "v",
        KeyCode::KeyW => "w",
        KeyCode::KeyX => "x",
        KeyCode::KeyY => "y",
        KeyCode::KeyZ => "z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        _ => return None,
    })
}

/// The system's double-click thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DoubleClick {
    pub(crate) interval: Duration,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

impl DoubleClick {
    pub(crate) fn system() -> Self {
        Self {
            interval: Duration::from_millis(u64::from(unsafe { GetDoubleClickTime() })),
            width: unsafe { GetSystemMetrics(SM_CXDOUBLECLK) },
            height: unsafe { GetSystemMetrics(SM_CYDOUBLECLK) },
        }
    }
}

/// Counts consecutive clicks of one button on nearby positions.
#[derive(Debug, Default)]
pub(crate) struct ClickState {
    last: Option<LastClick>,
}

#[derive(Debug)]
struct LastClick {
    button: MouseButton,
    at: Instant,
    position: Point<DevicePixels>,
    count: usize,
}

impl ClickState {
    pub(crate) fn update(
        &mut self,
        button: MouseButton,
        position: Point<DevicePixels>,
        now: Instant,
        threshold: DoubleClick,
    ) -> usize {
        let count = match &self.last {
            Some(last)
                if last.button == button
                    && now.saturating_duration_since(last.at) < threshold.interval
                    && (last.position.x.0 - position.x.0).abs() <= threshold.width
                    && (last.position.y.0 - position.y.0).abs() <= threshold.height =>
            {
                last.count + 1
            }
            _ => 1,
        };
        self.last = Some(LastClick {
            button,
            at: now,
            position,
            count,
        });
        count
    }

    pub(crate) fn current_count(&self) -> usize {
        self.last.as_ref().map_or(0, |last| last.count)
    }
}

#[cfg(test)]
mod tests {
    use gpui::point;

    use super::*;

    const THRESHOLD: DoubleClick = DoubleClick {
        interval: Duration::from_millis(500),
        width: 4,
        height: 4,
    };

    fn at(x: i32, y: i32) -> Point<DevicePixels> {
        point(DevicePixels(x), DevicePixels(y))
    }

    #[test]
    fn rapid_clicks_in_place_count_up() {
        let mut state = ClickState::default();
        let start = Instant::now();
        let step = Duration::from_millis(100);
        let counts: Vec<_> = (0..4u32)
            .map(|i| state.update(MouseButton::Left, at(10, 10), start + step * i, THRESHOLD))
            .collect();
        assert_eq!(counts, [1, 2, 3, 4]);
    }

    #[test]
    fn slow_clicks_restart_the_count() {
        let mut state = ClickState::default();
        let start = Instant::now();
        state.update(MouseButton::Left, at(10, 10), start, THRESHOLD);
        let count = state.update(
            MouseButton::Left,
            at(10, 10),
            start + THRESHOLD.interval,
            THRESHOLD,
        );
        assert_eq!(count, 1);
    }

    #[test]
    fn moving_past_the_tolerance_restarts_the_count() {
        let mut state = ClickState::default();
        let start = Instant::now();
        state.update(MouseButton::Left, at(10, 10), start, THRESHOLD);
        let near = state.update(MouseButton::Left, at(14, 6), start, THRESHOLD);
        let far = state.update(MouseButton::Left, at(19, 6), start, THRESHOLD);
        assert_eq!((near, far), (2, 1));
    }

    #[test]
    fn a_different_button_restarts_the_count() {
        let mut state = ClickState::default();
        let start = Instant::now();
        state.update(MouseButton::Left, at(0, 0), start, THRESHOLD);
        assert_eq!(
            state.update(MouseButton::Right, at(0, 0), start, THRESHOLD),
            1
        );
        assert_eq!(
            state.update(MouseButton::Right, at(0, 0), start, THRESHOLD),
            2
        );
    }

    #[test]
    fn named_keys_use_gpui_names() {
        let key = |named| key_name(&Key::Named(named), &PhysicalKey::Code(KeyCode::KeyA));
        assert_eq!(key(NamedKey::ArrowLeft).as_deref(), Some("left"));
        assert_eq!(key(NamedKey::PageDown).as_deref(), Some("pagedown"));
        assert_eq!(key(NamedKey::Space).as_deref(), Some("space"));
        assert_eq!(key(NamedKey::F13).as_deref(), Some("f13"));
        assert_eq!(key(NamedKey::ContextMenu).as_deref(), Some("menu"));
    }

    #[test]
    fn modifier_and_unmapped_keys_have_no_name() {
        let physical = PhysicalKey::Code(KeyCode::ShiftLeft);
        for named in [
            NamedKey::Shift,
            NamedKey::Control,
            NamedKey::Alt,
            NamedKey::Super,
        ] {
            assert_eq!(key_name(&Key::Named(named), &physical), None);
        }
    }

    #[test]
    fn character_keys_are_lowercased() {
        let physical = PhysicalKey::Code(KeyCode::KeyA);
        assert_eq!(
            key_name(&Key::Character("A".into()), &physical).as_deref(),
            Some("a")
        );
        assert_eq!(
            key_name(&Key::Character("É".into()), &physical).as_deref(),
            Some("é")
        );
    }

    #[test]
    fn unidentified_keys_fall_back_to_the_physical_key() {
        let key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
        assert_eq!(
            key_name(&key, &PhysicalKey::Code(KeyCode::Digit7)).as_deref(),
            Some("7")
        );
        assert_eq!(key_name(&key, &PhysicalKey::Code(KeyCode::F20)), None);
    }

    #[test]
    fn control_characters_are_not_typed() {
        assert_eq!(printable("a"), Some("a"));
        assert_eq!(printable(" "), Some(" "));
        assert_eq!(printable("\u{3}"), None);
        assert_eq!(printable("\r"), None);
        assert_eq!(printable(""), None);
    }

    #[test]
    fn plain_and_shifted_characters_are_typed() {
        let shifted = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let key = build_keystroke("a".into(), Some("A".into()), shifted);
        assert!(types_character(&key));
        assert!(!key.prefer_character_input);
    }

    #[test]
    fn command_chords_are_not_typed() {
        let control = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        assert!(!types_character(&build_keystroke(
            "c".into(),
            Some("c".into()),
            control
        )));
        assert!(!types_character(&build_keystroke(
            "enter".into(),
            None,
            Modifiers::default()
        )));
    }

    #[test]
    fn alt_gr_characters_take_precedence_over_bindings() {
        let alt_gr = Modifiers {
            control: true,
            alt: true,
            ..Modifiers::default()
        };
        let key = build_keystroke("q".into(), Some("@".into()), alt_gr);
        assert!(key.prefer_character_input);
        assert!(types_character(&key));

        let chord = build_keystroke("t".into(), None, alt_gr);
        assert!(!chord.prefer_character_input);
        assert!(!types_character(&chord));
    }
}
