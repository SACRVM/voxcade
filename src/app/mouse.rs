//! The mouse and the keyboard's typing in a window (`docs/game-model.md`,
//! "input"): where the pointer is and what it is over, its buttons and
//! wheel as keys (`input::MOUSE_KEYS`), the text typed while a player types
//! (`p.state.typing`). What a tick hears of them is gathered between two
//! ticks and taken once (`Mouse::add_to`): a click carries the answers
//! of the moment its button went down, a wheel's notch is one press,
//! several one a tick with a tick between, none lost.

use std::collections::VecDeque;

use sdl3::mouse::MouseButton;

use crate::controls::Bindings;
use crate::input::{Input, Pointer, Typed, TypedQueue};

/// The mouse's buttons by their key names, in `Mouse::held`'s order.
const BUTTONS: [(MouseButton, &str); 3] = [(MouseButton::Left, "mouse_left"), (MouseButton::Right, "mouse_right"), (MouseButton::Middle, "mouse_middle")];

/// The most wheel notches waiting (more are a spin, not steps).
const MOST_NOTCHES: usize = 32;

#[derive(Default)]
pub(super) struct Mouse {
    /// Where it is in the window (its points, from the top left); None
    /// before it came over it.
    pub at: Option<[f32; 2]>,
    held: [bool; 3],
    /// Went down since the last tick took them (a click shorter than a
    /// tick is a press).
    downs: [bool; 3],
    /// The answers of the first press since the last tick.
    press: Option<Pointer>,
    /// The wheel's notches to come, up (true) or down, and whether the tick
    /// after a notch's holds none (so the next is a press of its own).
    wheel: VecDeque<bool>,
    gap: bool,
    /// A touchpad's part of a notch, until it makes one.
    rest: f32,
    /// The text typed waiting for the ticks, and whether the window's text
    /// input is on for it.
    pub typed: TypedQueue,
    pub typing: bool,
}

impl Mouse {
    /// Which of the mouse's key names a button is.
    pub fn index(button: MouseButton) -> Option<usize> {
        BUTTONS.iter().position(|(b, _)| *b == button)
    }

    /// A button went down where `answer` says the pointer is.
    pub fn down(&mut self, i: usize, answer: Option<Pointer>) {
        self.held[i] = true;
        self.downs[i] = true;
        if self.press.is_none() {
            self.press = answer.map(|a| Pointer { press: true, ..a });
        }
    }

    pub fn up(&mut self, i: usize) {
        self.held[i] = false;
    }

    /// The wheel turned `turned` notches (above 0 away from the player:
    /// up), a part of one too (a touchpad).
    pub fn turned(&mut self, turned: f32) {
        if !turned.is_finite() {
            return;
        }
        self.rest += turned;
        let whole = self.rest.trunc();
        self.rest -= whole;
        self.wheel(whole as i32);
    }

    /// The wheel turned `notches` (above 0 away from the player: up).
    pub fn wheel(&mut self, notches: i32) {
        for _ in 0..notches.unsigned_abs() {
            if self.wheel.len() < MOST_NOTCHES {
                self.wheel.push_back(notches > 0);
            }
        }
    }

    /// Text typed (`TextInput`), Backspace, Enter or Escape.
    pub fn type_text(&mut self, typed: Typed) {
        self.typed.push(&typed);
    }

    /// Nothing held or waiting any more (the window lost the focus, a menu
    /// opened).
    pub fn let_go(&mut self) {
        (self.held, self.downs, self.press) = ([false; 3], [false; 3], None);
        self.wheel.clear();
        self.gap = false;
    }

    /// What the mouse gives the next tick (or the next reading over the
    /// network) on top of the keys in `input`: the actions `keys` binds to
    /// its buttons held or pressed since and to this tick's notch, the
    /// pointer (`now`: where it is, answered; a press's answers before
    /// them), the text typed, a sort a tick. Taken: the next gets the next.
    pub fn add_to(&mut self, input: &mut Input, keys: &Bindings, actions: &[&str], now: Option<Pointer>) {
        let notch = if std::mem::take(&mut self.gap) {
            None
        } else {
            let n = self.wheel.pop_front();
            self.gap = n.is_some();
            n
        };
        let on = |name: &str| match name.to_lowercase().as_str() {
            "wheel_up" => notch == Some(true),
            "wheel_down" => notch == Some(false),
            other => BUTTONS.iter().position(|(_, n)| *n == other).is_some_and(|i| self.held[i] || self.downs[i]),
        };
        for (a, action) in actions.iter().enumerate() {
            if keys.get(action).iter().any(|k| on(k)) {
                input.hold(a, true);
            }
        }
        self.downs = [false; 3];
        let press = self.press.take();
        input.pointer = press.or(now).map(Box::new);
        input.typed = self.typed.take().map(Box::new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A notch is one press: two notches are held a tick each with a tick
    /// between; a click shorter than a tick still holds its tick, with the
    /// answers of its moment.
    #[test]
    fn notches_and_clicks_come_one_a_tick_none_lost() {
        let keys = Bindings::new(&[("next", &["wheel_down"]), ("click", &["mouse_left"])]);
        let actions = ["next", "click"];
        let mut m = Mouse::default();
        m.wheel(-2);
        let mut held = Vec::new();
        for _ in 0..5 {
            let mut i = Input::default();
            m.add_to(&mut i, &keys, &actions, None);
            held.push(i.held(0));
        }
        assert_eq!(held, [true, false, true, false, false]);
        let at = |x| Pointer { x, picture: Some(1), ..Pointer::default() };
        m.down(0, Some(at(5)));
        m.up(0);
        let mut i = Input::default();
        m.add_to(&mut i, &keys, &actions, Some(at(40)));
        assert!(i.held(1), "a click shorter than a tick");
        assert_eq!(i.pointer.as_ref().map(|p| (p.x, p.press)), Some((5, true)), "with the answers of its moment");
        let mut i = Input::default();
        m.add_to(&mut i, &keys, &actions, Some(at(40)));
        assert!(!i.held(1));
        assert_eq!(i.pointer.map(|p| p.x), Some(40));
    }
}
