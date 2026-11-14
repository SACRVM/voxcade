//! SDL3's events as egui's input, and egui's wishes (the mouse cursor, the
//! clipboard, where typed text goes) passed back to SDL: the little glue a
//! ready-made integration would bring, kept small.
//!
//! SDL gives positions in the window's points; egui counts its own points
//! (`pixels_per_point` pixels each). With the display scale as egui's
//! pixels per point the two are the same on most systems.

use std::time::Instant;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use sdl3::event::{Event as SdlEvent, WindowEvent};
use sdl3::keyboard::{Keycode, Mod};
use sdl3::mouse::{MouseButton, MouseWheelDirection};

/// The input gathered since the last frame.
pub struct Input {
    start: Instant,
    events: Vec<Event>,
    modifiers: Modifiers,
    /// egui's points for one of the window's (SDL's) points.
    scale: f32,
    focused: bool,
    /// Text being composed with an input method (SDL's text editing).
    composing: bool,
    /// Where the pointer was last (egui's points).
    last: Option<Pos2>,
}

/// Whether windows keep the keyboard for egui while the system's focus is
/// elsewhere (`keep_focus`).
static KEEP_FOCUS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// From now on a window that lost the system's focus still counts as
/// focused for its panels: for an IDE another session uses (`ide --remote`),
/// whose keys come as events while the person works in another window.
/// Without it egui lets go of every text field and the keys go nowhere.
pub fn keep_focus(on: bool) {
    KEEP_FOCUS.store(on, std::sync::atomic::Ordering::Relaxed);
}

impl Default for Input {
    fn default() -> Self {
        Self { start: Instant::now(), events: Vec::new(), modifiers: Modifiers::default(), scale: 1.0, focused: true, composing: false, last: None }
    }
}

impl Input {
    /// Takes in an SDL event; `paste`: the clipboard's text, asked for
    /// only when something is pasted.
    pub fn handle(&mut self, event: &SdlEvent, paste: impl FnOnce() -> Option<String>) {
        let at = |x: f32, y: f32| Pos2::new(x * self.scale, y * self.scale);
        match event {
            SdlEvent::KeyDown { keycode: Some(k), keymod, repeat, .. } | SdlEvent::KeyUp { keycode: Some(k), keymod, repeat, .. } => {
                let pressed = matches!(event, SdlEvent::KeyDown { .. });
                self.modifiers = modifiers(*keymod);
                let m = self.modifiers;
                // Copy, cut and paste are events of their own to egui;
                // Ctrl+Shift+V pastes too (as in terminals). With nothing
                // to paste the key goes on (the console sends it).
                if pressed && m.command && !m.alt {
                    match k {
                        Keycode::C if !m.shift => return self.events.push(Event::Copy),
                        Keycode::X if !m.shift => return self.events.push(Event::Cut),
                        Keycode::V => {
                            if let Some(text) = paste() {
                                return self.events.push(Event::Paste(text));
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(key) = key(*k) {
                    self.events.push(Event::Key { key, physical_key: None, pressed, repeat: *repeat, modifiers: m });
                }
            }
            SdlEvent::TextInput { text, .. } => {
                if self.composing {
                    self.composing = false;
                    self.events.push(Event::Ime(egui::ImeEvent::Commit(text.clone())));
                } else if !text.chars().any(char::is_control) && !(self.modifiers.ctrl && !self.modifiers.alt) && !self.modifiers.mac_cmd {
                    // (Ctrl with Alt is AltGr on Windows: its letters, @
                    // and braces on a German keyboard, are typed text.)
                    self.events.push(Event::Text(text.clone()));
                }
            }
            SdlEvent::TextEditing { text, .. } => {
                self.composing = !text.is_empty();
                self.events.push(Event::Ime(egui::ImeEvent::Preedit { text: text.clone(), active_range_chars: None }));
            }
            SdlEvent::MouseMotion { x, y, .. } => {
                self.last = Some(at(*x, *y));
                self.events.push(Event::PointerMoved(at(*x, *y)));
            }
            SdlEvent::MouseButtonDown { mouse_btn, x, y, .. } | SdlEvent::MouseButtonUp { mouse_btn, x, y, .. } => {
                let button = match mouse_btn {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    MouseButton::X1 => PointerButton::Extra1,
                    MouseButton::X2 => PointerButton::Extra2,
                    MouseButton::Unknown => return,
                };
                let pressed = matches!(event, SdlEvent::MouseButtonDown { .. });
                self.events.push(Event::PointerButton { pos: at(*x, *y), button, pressed, modifiers: self.modifiers });
            }
            SdlEvent::MouseWheel { x, y, direction, .. } => {
                let flip = if matches!(direction, MouseWheelDirection::Flipped) { -1.0 } else { 1.0 };
                self.events.push(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(*x, *y) * flip,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: self.modifiers,
                });
            }
            SdlEvent::Window { win_event, .. } => match win_event {
                WindowEvent::MouseLeave => self.events.push(Event::PointerGone),
                WindowEvent::FocusGained | WindowEvent::FocusLost => {
                    // (Used by another session, `keep_focus`, a window
                    // elsewhere still takes its keys.)
                    self.focused = matches!(win_event, WindowEvent::FocusGained) || KEEP_FOCUS.load(std::sync::atomic::Ordering::Relaxed);
                    self.events.push(Event::WindowFocused(self.focused));
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// A point of the window in SDL's points as egui's.
    /// Every button let go where the pointer was last (a release the
    /// window never heard: egui would hold it down for good, and the next
    /// click would be no click).
    pub fn release_buttons(&mut self) {
        let pos = self.events.iter().rev().find_map(|e| match e {
            Event::PointerMoved(p) | Event::PointerButton { pos: p, .. } => Some(*p),
            _ => None,
        });
        let pos = pos.or(self.last).unwrap_or_default();
        for button in [PointerButton::Primary, PointerButton::Secondary, PointerButton::Middle] {
            self.events.push(Event::PointerButton { pos, button, pressed: false, modifiers: self.modifiers });
        }
    }

    pub fn point(&self, x: f32, y: f32) -> egui::Pos2 {
        Pos2::new(x * self.scale, y * self.scale)
    }

    /// The clock egui is given moved on by `by` (tests: a click past a
    /// double click's time without waiting for it).
    #[cfg(test)]
    pub fn later(&mut self, by: std::time::Duration) {
        self.start -= by;
    }

    /// The input for a frame of a window `size` pixels big with
    /// `pixels_per_point` pixels to egui's point and `density` pixels to
    /// one of SDL's points.
    pub fn take(&mut self, size: [u32; 2], pixels_per_point: f32, density: f32) -> RawInput {
        self.scale = density / pixels_per_point;
        let mut raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(size[0] as f32, size[1] as f32) / pixels_per_point)),
            time: Some(self.start.elapsed().as_secs_f64()),
            focused: self.focused,
            events: std::mem::take(&mut self.events),
            ..RawInput::default()
        };
        raw.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(pixels_per_point);
        raw
    }
}

/// SDL's modifier keys as egui's (`command` is Ctrl, or Cmd on a Mac).
pub fn modifiers(m: Mod) -> Modifiers {
    let ctrl = m.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
    let gui = m.intersects(Mod::LGUIMOD | Mod::RGUIMOD);
    Modifiers {
        alt: m.intersects(Mod::LALTMOD | Mod::RALTMOD),
        ctrl,
        shift: m.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD),
        mac_cmd: cfg!(target_os = "macos") && gui,
        command: if cfg!(target_os = "macos") { gui } else { ctrl },
    }
}

/// The egui key an SDL key is, if egui has it.
pub fn key(k: Keycode) -> Option<Key> {
    use Keycode as K;
    Some(match k {
        K::Up => Key::ArrowUp,
        K::Down => Key::ArrowDown,
        K::Left => Key::ArrowLeft,
        K::Right => Key::ArrowRight,
        K::Escape => Key::Escape,
        K::Tab => Key::Tab,
        K::Backspace => Key::Backspace,
        K::Return | K::KpEnter => Key::Enter,
        K::Space => Key::Space,
        K::Insert => Key::Insert,
        K::Delete => Key::Delete,
        K::Home => Key::Home,
        K::End => Key::End,
        K::PageUp => Key::PageUp,
        K::PageDown => Key::PageDown,
        K::Comma => Key::Comma,
        K::Period => Key::Period,
        K::Minus => Key::Minus,
        K::Plus => Key::Plus,
        K::Equals => Key::Equals,
        K::Semicolon => Key::Semicolon,
        K::Slash => Key::Slash,
        K::Backslash => Key::Backslash,
        K::LeftBracket => Key::OpenBracket,
        K::RightBracket => Key::CloseBracket,
        K::Grave => Key::Backtick,
        K::Apostrophe => Key::Quote,
        K::_0 => Key::Num0,
        K::_1 => Key::Num1,
        K::_2 => Key::Num2,
        K::_3 => Key::Num3,
        K::_4 => Key::Num4,
        K::_5 => Key::Num5,
        K::_6 => Key::Num6,
        K::_7 => Key::Num7,
        K::_8 => Key::Num8,
        K::_9 => Key::Num9,
        K::A => Key::A,
        K::B => Key::B,
        K::C => Key::C,
        K::D => Key::D,
        K::E => Key::E,
        K::F => Key::F,
        K::G => Key::G,
        K::H => Key::H,
        K::I => Key::I,
        K::J => Key::J,
        K::K => Key::K,
        K::L => Key::L,
        K::M => Key::M,
        K::N => Key::N,
        K::O => Key::O,
        K::P => Key::P,
        K::Q => Key::Q,
        K::R => Key::R,
        K::S => Key::S,
        K::T => Key::T,
        K::U => Key::U,
        K::V => Key::V,
        K::W => Key::W,
        K::X => Key::X,
        K::Y => Key::Y,
        K::Z => Key::Z,
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
        _ => return None,
    })
}

/// The mouse cursor egui wants, as one of SDL's.
pub fn cursor(icon: egui::CursorIcon) -> sdl3::mouse::SystemCursor {
    use egui::CursorIcon as C;
    use sdl3::mouse::SystemCursor as S;
    match icon {
        C::Text | C::VerticalText => S::IBeam,
        C::PointingHand => S::Hand,
        C::Wait => S::Wait,
        C::Progress => S::WaitArrow,
        C::Crosshair => S::Crosshair,
        C::NotAllowed | C::NoDrop => S::No,
        C::Move | C::AllScroll | C::Grab | C::Grabbing => S::SizeAll,
        C::ResizeHorizontal | C::ResizeEast | C::ResizeWest | C::ResizeColumn => S::SizeWE,
        C::ResizeVertical | C::ResizeNorth | C::ResizeSouth | C::ResizeRow => S::SizeNS,
        C::ResizeNeSw | C::ResizeNorthEast | C::ResizeSouthWest => S::SizeNESW,
        C::ResizeNwSe | C::ResizeNorthWest | C::ResizeSouthEast => S::SizeNWSE,
        _ => S::Arrow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(keycode: Keycode, keymod: Mod) -> SdlEvent {
        SdlEvent::KeyDown { timestamp: 0, window_id: 0, keycode: Some(keycode), scancode: None, keymod, repeat: false, which: 0, raw: 0 }
    }

    /// Keys come as egui's with their modifiers, Ctrl+C, X and V as copy,
    /// cut and paste, typed text as text (not while Ctrl is down), the
    /// mouse in egui's points.
    #[test]
    fn sdl_keys_become_egui_events() {
        let mut input = Input::default();
        input.handle(&down(Keycode::A, Mod::LSHIFTMOD), || None);
        input.handle(&SdlEvent::TextInput { timestamp: 0, window_id: 0, text: "A".into() }, || None);
        input.handle(&down(Keycode::C, Mod::LCTRLMOD), || None);
        input.handle(&down(Keycode::V, Mod::RCTRLMOD), || Some("pasted".into()));
        input.handle(&SdlEvent::TextInput { timestamp: 0, window_id: 0, text: "v".into() }, || None);
        let raw = input.take([200, 100], 2.0, 2.0);
        let shift = Modifiers { shift: true, ..Modifiers::default() };
        assert_eq!(
            raw.events,
            [
                Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: shift },
                Event::Text("A".into()),
                Event::Copy,
                Event::Paste("pasted".into()),
            ]
        );
        assert_eq!(raw.screen_rect.unwrap().size(), Vec2::new(100.0, 50.0), "pixels in egui's points");
        // Positions: SDL's points times its density over egui's scale.
        input.take([200, 100], 1.0, 2.0);
        input.handle(&SdlEvent::MouseMotion { timestamp: 0, window_id: 0, which: 0, mousestate: sdl3::mouse::MouseState::from_sdl_state(0), x: 10.0, y: 5.0, xrel: 0.0, yrel: 0.0 }, || None);
        assert_eq!(input.take([200, 100], 1.0, 2.0).events, [Event::PointerMoved(Pos2::new(20.0, 10.0))]);
    }
}
