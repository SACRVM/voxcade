//! The mouse and the keyboard's typing where a game plays, a window of its
//! own (`app`) or the IDE's PLAY (`ide::play`) alike (`docs/game-model.md`,
//! "input"): where the pointer is and what it is over, its buttons and
//! wheel as keys (`input::MOUSE_KEYS`), the text typed while a player types
//! (`p.state.typing`), a pad's stick moving the pointer
//! (`c.pointer_stick`). What a tick hears of them is gathered between two
//! ticks and taken once (`Mouse::add_to`): a click carries the answers
//! of the moment its button went down, a wheel's notch is one press,
//! several one a tick with a tick between, none lost.

use std::collections::VecDeque;

use sdl3::mouse::MouseButton;

use crate::controls::{Bindings, Device};
use crate::input::{Input, Pointer, Typed, TypedQueue};

/// Where a place `at` of a picture `size` big (the same measure, from its
/// top left: a window's points, the IDE's play area's pixels) is in the
/// HUD's measure (`hud`, its width and height), the picture drawn there in
/// its own shape where the game has one (`shape`, `c.camera.aspect`: bars
/// round it); None off the picture, on a bar too.
pub fn hud_at(at: [f32; 2], size: (u32, u32), shape: Option<f32>, hud: [f32; 2]) -> Option<[f32; 2]> {
    let (w, h) = size;
    let (x0, y0, across, high) = match shape {
        Some(shape) => crate::render::letterbox(w, h, shape),
        None => (0, 0, w, h),
    };
    let (u, v) = ((at[0] - x0 as f32) / across.max(1) as f32, (at[1] - y0 as f32) / high.max(1) as f32);
    ((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)).then_some([u * hud[0], v * hud[1]])
}

/// Where seat `seat`'s pointer is in the HUD's measure: the mouse seat's
/// (`mouse_seat`) the mouse's (`mouse`, None off the picture) while the
/// keyboard was used last (`with`), its stick's (`stick`) while the pad
/// was; another seat's its stick's where it has a pad (`pads`).
pub fn seat_place(seat: usize, mouse_seat: usize, with: Device, mouse: Option<[f32; 2]>, stick: Option<[f32; 2]>, pads: bool) -> Option<[f32; 2]> {
    if seat != mouse_seat {
        return stick.filter(|_| pads);
    }
    match with {
        Device::Keyboard => mouse,
        Device::Gamepad => stick,
    }
}

/// Every frame in a game whose pad's stick moves the pointer
/// (`c.pointer_stick`): each seat's glides by the lean read last over
/// `seconds` (a tenth of a second at most) at `speed` while `playing`, on a
/// HUD `hud` big. One begins in the picture's middle, the mouse seat's
/// following the mouse (`mouse`, where the keyboard is used) so the pad
/// takes it on where the mouse left it; always on the picture. Whether
/// seat 1 leans its stick (its pad is then what it plays with).
pub fn glide_sticks(sticks: &mut [StickPointer], mouse: Option<[f32; 2]>, mouse_seat: usize, hud: [f32; 2], speed: f32, seconds: f32, playing: bool) -> bool {
    let [w, h] = hud;
    let mut leaned = false;
    for (i, s) in sticks.iter_mut().enumerate() {
        let lean = if playing { s.lean } else { [0.0; 2] };
        let at = mouse.filter(|_| i + 1 == mouse_seat).or(s.at).unwrap_or([w / 2.0, h / 2.0]);
        s.at = Some(crate::input::glide(at, lean, seconds.min(0.1), speed, w, h));
        if i == 0 && crate::input::stick(lean[0], lean[1], crate::controls::DEAD) != [0, 0] {
            leaned = true;
        }
    }
    leaned
}

/// The mouse's buttons by their key names, in `Mouse::held`'s order.
const BUTTONS: [(MouseButton, &str); 3] = [(MouseButton::Left, "mouse_left"), (MouseButton::Right, "mouse_right"), (MouseButton::Middle, "mouse_middle")];

/// The most wheel notches waiting (more are a spin, not steps).
const MOST_NOTCHES: usize = 32;

#[derive(Default)]
pub struct Mouse {
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

    /// The mouse came to `at`: whether it moved there, more than 2 of its
    /// measure from where it was (a hand on it, not a tremble).
    pub fn move_to(&mut self, at: [f32; 2]) -> bool {
        let moved = self.at.is_some_and(|was| (was[0] - at[0]).abs() + (was[1] - at[1]).abs() > 2.0);
        self.at = Some(at);
        moved
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

    /// Text the system typed (its `TextInput`): the signs the pixel font
    /// writes.
    pub fn type_in(&mut self, text: &str) {
        self.type_text(Typed { text: text.chars().filter(|&c| crate::ui::writes(c)).collect(), ..Typed::default() });
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

/// A seat's pointer that a pad's stick moves (`c.pointer_stick`), as the
/// mouse's: where it is, the lean read last, and the answers of a pad
/// button pressed since the last tick took them.
#[derive(Default)]
pub struct StickPointer {
    /// Where it is in the HUD's measure; None before the game had one.
    pub at: Option<[f32; 2]>,
    /// The stick's lean as read last (`controls::sticks`).
    pub lean: [f32; 2],
    press: Option<Pointer>,
}

impl StickPointer {
    /// A button bound to one of the game's actions went down where
    /// `answer` says the pointer is: the first since the last tick is kept.
    pub fn down(&mut self, answer: Option<Pointer>) {
        if self.press.is_none() {
            self.press = answer.map(|a| Pointer { press: true, ..a });
        }
    }

    /// Nothing waiting any more (the window lost the focus).
    pub fn let_go(&mut self) {
        self.press = None;
    }

    /// The press's answers into the next tick's (or reading's) `input`,
    /// unless the mouse's press came first. Taken: the next gets the next.
    pub fn add_to(&mut self, input: &mut Input) {
        if let Some(p) = self.press.take()
            && !input.pointer.as_ref().is_some_and(|p| p.press)
        {
            input.pointer = Some(Box::new(p));
        }
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

    /// A place on the picture is the HUD's from corner to corner; a picture
    /// of its own shape lies between bars that are no place; off it, none.
    #[test]
    fn a_place_on_the_picture_is_the_huds_and_a_bar_is_none() {
        let hud = [320.0, 180.0];
        assert_eq!(hud_at([0.0, 0.0], (1600, 900), None, hud), Some([0.0, 0.0]));
        assert_eq!(hud_at([1600.0, 900.0], (1600, 900), None, hud), Some(hud));
        assert_eq!(hud_at([400.0, 450.0], (1600, 900), None, hud), Some([80.0, 90.0]));
        assert_eq!(hud_at([1601.0, 10.0], (1600, 900), None, hud), None);
        assert_eq!(hud_at([-1.0, 10.0], (1600, 900), None, hud), None);
        // 4:3 in 16:9: 1200 across, bars of 200 left and right.
        let square = [240.0, 180.0];
        assert_eq!(hud_at([100.0, 450.0], (1600, 900), Some(4.0 / 3.0), square), None, "a bar");
        assert_eq!(hud_at([200.0, 0.0], (1600, 900), Some(4.0 / 3.0), square), Some([0.0, 0.0]));
        assert_eq!(hud_at([800.0, 450.0], (1600, 900), Some(4.0 / 3.0), square), Some([120.0, 90.0]));
        assert_eq!(hud_at([1400.0, 900.0], (1600, 900), Some(4.0 / 3.0), square), Some(square));
    }

    /// A pad's button pressed while its stick moves the pointer carries
    /// the answers of its moment, as a click does, however far the stick
    /// moved it before the tick; the tick after has the pointer where it is.
    #[test]
    fn a_pad_button_carries_the_pointers_answer_of_its_moment() {
        let at = |x| Pointer { x, picture: Some(1), ..Pointer::default() };
        let mut s = StickPointer::default();
        s.down(Some(at(5)));
        s.down(Some(at(9)));
        let mut i = Input { pointer: Some(Box::new(at(40))), ..Input::default() };
        s.add_to(&mut i);
        assert_eq!(i.pointer.as_ref().map(|p| (p.x, p.press)), Some((5, true)), "the first press's answers");
        let mut i = Input { pointer: Some(Box::new(at(40))), ..Input::default() };
        s.add_to(&mut i);
        assert_eq!(i.pointer.map(|p| (p.x, p.press)), Some((40, false)), "taken once");
        // The mouse's click came first: it is kept.
        let mut m = Mouse::default();
        m.down(0, Some(at(3)));
        s.down(Some(at(7)));
        let mut i = Input::default();
        m.add_to(&mut i, &Bindings::default(), &[], Some(at(40)));
        s.add_to(&mut i);
        assert_eq!(i.pointer.map(|p| p.x), Some(3));
    }
}
