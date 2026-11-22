//! What the player holds during a tick: the four directions and the game's
//! actions, the sticks as values, where the pointer is and what it is over,
//! and the text typed. Each game names its actions (More Blood: jump,
//! fire, grenade, strafe); keys and buttons are bound to them by name, and
//! replays and `shot` scripts write them by name.
//!
//! Whatever comes from the player's machine is input: a replay, the network
//! and `shot --input` carry all of it (`docs/game-model.md`, "input"). The
//! pointer never comes as pixels: the machine answers what it is over from
//! the picture it last drew (`Pointer`), and the tick checks the answer
//! before the game sees it (`Pointer::checked`).

/// The directions, by their names in scripts.
pub const DIRECTIONS: [&str; 4] = ["left", "right", "up", "down"];

/// The mouse's buttons and wheel as key names in `c.keys` (a game with a
/// pointer, `c.pointer`): a wheel's notch is one press.
pub const MOUSE_KEYS: [&str; 5] = ["mouse_left", "mouse_right", "mouse_middle", "wheel_up", "wheel_down"];

/// A stick's value at its fullest (`Input::moves`, `aim`): 1.0 in 127ths.
pub const STICK: i32 = 127;

/// The most bytes of text a tick carries (more goes into the ticks after).
pub const MOST_TYPED: usize = 8;

/// The most backspaces a tick carries.
pub const MOST_ERASE: u16 = 64;

/// A panel's name at most (`frame:hud`, a menu's or grid's `name`).
pub const MOST_PANEL_NAME: usize = 16;

/// `u` and `v` of a panel in these steps: 1000 is its far edge.
pub const PANEL_STEPS: u16 = 1000;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    /// Bit n: the game's n-th action is held.
    pub actions: u32,
    /// The left stick (`input.move`): x and y in 127ths, y up, never
    /// longer than 127; 0, 0 untouched (the directions then).
    pub moves: [i8; 2],
    /// The right stick (`input.aim`), alike; in a game with a pointer the
    /// way from the player's figure to the pointer where no stick aims.
    pub aim: [i8; 2],
    /// Where the pointer is and what it is over, in a game with a pointer
    /// (None: no pointer, or the pad was used last).
    pub pointer: Option<Box<Pointer>>,
    /// What was typed this tick, while the player types
    /// (`player.state.typing`).
    pub typed: Option<Box<Typed>>,
}

/// The pointer as a machine answered it from the picture it last drew.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Pointer {
    /// Where it is in the HUD's measure (`frame.width` by `frame.height`),
    /// whole UI voxels from the top left, as the machine's picture had it
    /// (`input.pointer.x`, `frame.pointer` played back).
    pub x: i32,
    pub y: i32,
    /// The level's picture that answered (`picture_mark` of
    /// `Game::level_mark`): an answer from another is nil whole. None: not
    /// answered yet (a script's `pointer=X,Y`, answered before its step).
    pub picture: Option<u16>,
    /// The HUD panel under it, the topmost.
    pub panel: Option<Panel>,
    /// The level under it (none under a panel): what its ray meets.
    pub level: Option<Met>,
    /// These are the answers of the moment a mouse button went down (a
    /// reading's, merged so it is kept); never in a tick played.
    pub press: bool,
}

/// A HUD panel the pointer is over (`frame:hud`, `frame:menu`,
/// `frame:grid`).
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Panel {
    /// Its name; "" for a menu or grid without one (it covers, unnamed).
    pub name: String,
    /// The menu's row or the grid's cell under it, from 1; 0: none.
    pub index: u16,
    /// Where in it from its top left, in `PANEL_STEPS`.
    pub u: u16,
    pub v: u16,
}

/// What the pointer's ray meets in the level, as the picture drew it.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Met {
    /// The thing whose drawn voxels it meets first, by the key it was
    /// drawn under (`Drawing::of_thing`: its id, or a player's handle).
    pub thing: Option<u32>,
    /// That figure's part, by its name (the model drawn is the picture's
    /// alone: a `frame:model` file the tick does not know), at most
    /// `MOST_PART_NAME` bytes: a model with a longer one is a mistake as
    /// the game reads it (`session::models`), so none is dropped here.
    pub part: Option<String>,
    /// The level's area it enters first before it meets the level, by its
    /// place in the level's list (from 1); 0: none.
    pub area: u16,
    /// Where it meets the level's voxels and solid pieces, in sixteenths
    /// of a voxel, and the face it meets (`mesh::DIRS`' order).
    pub at: Option<([i32; 3], u8)>,
}

/// A part's name at most, in bytes (UTF-8: a letter past ASCII takes two
/// to four), as a pointer's answer carries it and a model the game reads
/// may have it.
pub const MOST_PART_NAME: usize = 32;

/// Why a part may not be called `name` as the pointer names it: longer
/// than `MOST_PART_NAME` bytes (None: it may).
pub fn part_name_too_long(name: &str) -> Option<String> {
    let n = name.len();
    (n > MOST_PART_NAME).then(|| {
        let letters = name.chars().count();
        let counted = if letters == n { String::new() } else { format!(" ({letters} letters: one past ASCII takes two to four bytes)") };
        format!("its name is {n} bytes long{counted}, past the {MOST_PART_NAME} a part's name may have (the pointer names a part by it)")
    })
}

/// Text typed in a tick: one sort a tick, so its order is kept whatever a
/// game's code reads first (`TypedQueue`).
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Typed {
    pub text: String,
    pub erase: u16,
    /// Enter, and Escape.
    pub done: bool,
    pub cancel: bool,
}

impl Typed {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.erase == 0 && !self.done && !self.cancel
    }

    /// Text alone, of the signs the pixel font writes, at most
    /// `MOST_TYPED` bytes, never cut inside a character.
    pub fn text(text: &str) -> Self {
        Self { text: written(text), ..Self::default() }
    }

    /// As the tick takes it from another machine: only the signs the font
    /// writes, the counts in their bounds.
    pub fn checked(&self) -> Self {
        Self { text: written(&self.text), erase: self.erase.min(MOST_ERASE), done: self.done, cancel: self.cancel }
    }
}

/// The signs of `text` the pixel font writes (a space too), at most
/// `MOST_TYPED` bytes.
fn written(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().filter(|&c| crate::ui::writes(c)) {
        if out.len() + c.len_utf8() > MOST_TYPED {
            break;
        }
        out.push(c);
    }
    out
}

/// The text typed, waiting for the ticks: pieces in the order they came,
/// each tick taking one sort (text, backspaces, Enter or Escape), so a
/// tick never holds text after a backspace it would be read before.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct TypedQueue {
    pieces: std::collections::VecDeque<Typed>,
}

impl TypedQueue {
    /// What was typed, after what came before.
    pub fn push(&mut self, typed: &Typed) {
        let mut rest = typed.text.as_str();
        while !rest.is_empty() {
            let at = rest.char_indices().map(|(i, c)| i + c.len_utf8()).take_while(|&end| end <= MOST_TYPED).last().unwrap_or(rest.len());
            self.pieces.push_back(Typed { text: rest[..at].to_string(), ..Typed::default() });
            rest = &rest[at..];
        }
        if typed.erase > 0 {
            self.pieces.push_back(Typed { erase: typed.erase, ..Typed::default() });
        }
        if typed.done {
            self.pieces.push_back(Typed { done: true, ..Typed::default() });
        }
        if typed.cancel {
            self.pieces.push_back(Typed { cancel: true, ..Typed::default() });
        }
    }

    /// The next tick's: the first piece and those of its sort after it, as
    /// many as one tick carries.
    pub fn take(&mut self) -> Option<Typed> {
        let mut out = self.pieces.pop_front()?;
        while let Some(next) = self.pieces.front() {
            let text = !out.text.is_empty() && !next.text.is_empty() && out.text.len() + next.text.len() <= MOST_TYPED;
            let erase = out.erase > 0 && next.erase > 0 && out.erase + next.erase <= MOST_ERASE;
            if !(text || erase) {
                break;
            }
            out.text.push_str(&next.text);
            out.erase += next.erase;
            self.pieces.pop_front();
        }
        Some(out)
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    pub fn clear(&mut self) {
        self.pieces.clear();
    }
}

/// A level's picture as one small number (`Pointer::picture`), from
/// `Game::level_mark`: an answer a few ticks old is from another level's
/// picture when it differs.
pub fn picture_mark(level_mark: u64) -> u16 {
    let h = crate::hash::mix(level_mark);
    (h ^ h >> 16 ^ h >> 32 ^ h >> 48) as u16
}

/// Whether `name` may name a panel: a word of letters, digits and `_`, at
/// most `MOST_PANEL_NAME` bytes ("" too: a menu or grid without a name).
pub fn panel_name(name: &str) -> bool {
    name.len() <= MOST_PANEL_NAME && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Pointer {
    /// The pointer as the tick takes it, now that the level's picture is
    /// `picture`: None whole when it is another level's; a panel whose
    /// answer cannot be one (a name no panel has, `u` or `v` past the edge)
    /// is dropped, and its covering with it.
    pub fn checked(&self, picture: u16) -> Option<Pointer> {
        if self.picture != Some(picture) {
            return None;
        }
        let panel = self.panel.clone().filter(|p| panel_name(&p.name) && p.u <= PANEL_STEPS && p.v <= PANEL_STEPS);
        let level = if panel.is_some() { None } else { self.level.clone() };
        Some(Pointer { panel, level, press: false, ..self.clone() })
    }
}

/// A stick's value (-1 to 1 each way, y down as SDL gives it) as the
/// engine carries it: past the dead zone `dead` (radially, then from 0
/// again), y up, in 127ths, never longer than 127.
pub fn stick(x: f32, y: f32, dead: f32) -> [i8; 2] {
    let len = (x * x + y * y).sqrt();
    if len <= dead || !len.is_finite() {
        return [0, 0];
    }
    let scale = ((len - dead) / (1.0 - dead)).min(1.0) / len;
    let [sx, sy] = stick_checked_wide((x * scale * 127.0).round() as i32, (-y * scale * 127.0).round() as i32);
    [sx as i8, sy as i8]
}

/// A stick's value as the tick takes it (another machine may send any
/// bytes): -128 is -127, and one longer than 127 is shortened along its
/// way, in whole numbers alike everywhere.
pub fn stick_checked(v: [i8; 2]) -> [i32; 2] {
    let [x, y] = v.map(|n| (n as i32).max(-STICK));
    stick_checked_wide(x, y)
}

/// A way in 127ths no longer than 127: one longer scaled down along it,
/// towards 0 (a square root is exact in IEEE floats on every system).
fn stick_checked_wide(x: i32, y: i32) -> [i32; 2] {
    let len2 = x * x + y * y;
    if len2 <= STICK * STICK {
        return [x, y];
    }
    let len = (len2 as f64).sqrt();
    let along = |v: i32| (v as f64 * STICK as f64 / len).trunc() as i32;
    [along(x), along(y)]
}

impl Input {
    pub fn held(&self, action: usize) -> bool {
        self.actions >> action & 1 == 1
    }

    pub fn hold(&mut self, action: usize, on: bool) {
        if on {
            self.actions |= 1 << action;
        } else {
            self.actions &= !(1 << action);
        }
    }

    /// The directions held, in `DIRECTIONS` order.
    pub fn directions(&self) -> [bool; 4] {
        [self.left, self.right, self.up, self.down]
    }

    pub fn set_direction(&mut self, which: usize, on: bool) {
        match which {
            0 => self.left = on,
            1 => self.right = on,
            2 => self.up = on,
            _ => self.down = on,
        }
    }

    /// The keys and sticks alone: what a tick goes on holding when no new
    /// reading came (the pointer stays, the text typed went once).
    pub fn held_on(&self) -> Self {
        Self { typed: None, pointer: self.pointer.clone().map(|p| Box::new(Pointer { press: false, ..*p })), ..self.clone() }
    }

    /// Whether its pointer was moved since the tick that held `before`
    /// (`input.pointer.moved`): it is elsewhere, or that tick had none (the
    /// run's first, the mouse back on the picture or used after the pad).
    /// What it is over changing alone is no move.
    pub fn pointer_moved(&self, before: &Input) -> bool {
        self.pointer.as_ref().is_some_and(|p| before.pointer.as_ref().is_none_or(|b| (b.x, b.y) != (p.x, p.y)))
    }

    /// Whether anything but the directions and actions is in it (what
    /// replays, hashes and the wire wrote before these came).
    pub fn has_more(&self) -> bool {
        self.moves != [0, 0] || self.aim != [0, 0] || self.pointer.is_some() || self.typed.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stick past its dead zone is carried in 127ths, never longer than
    /// 1; one inside it is 0; y up.
    #[test]
    fn a_stick_is_carried_in_127ths_never_longer_than_one() {
        assert_eq!(stick(0.1, 0.2, 0.3), [0, 0]);
        assert_eq!(stick(1.0, 0.0, 0.3), [127, 0]);
        assert_eq!(stick(0.0, -1.0, 0.3), [0, 127], "SDL's up is negative: the engine's up is +");
        for (x, y) in [(1.0, 1.0), (-1.0, 1.0), (0.9, -0.95), (0.71, 0.71)] {
            let [sx, sy] = stick(x, y, 0.3).map(|n| n as i32);
            assert!(sx * sx + sy * sy <= 127 * 127, "{x},{y}: {sx},{sy}");
        }
        assert_eq!(stick_checked([-128, 0]), [-127, 0]);
        let [x, y] = stick_checked([127, 127]);
        assert!(x * x + y * y <= 127 * 127 && x == y, "{x},{y}");
    }

    /// Typed text is taken a sort a tick, in order, none lost, none twice.
    #[test]
    fn typed_text_is_taken_a_sort_a_tick_in_order() {
        let mut q = TypedQueue::default();
        q.push(&Typed { text: "ab".into(), ..Typed::default() });
        q.push(&Typed { text: "c".into(), erase: 1, ..Typed::default() });
        q.push(&Typed { erase: 2, done: true, ..Typed::default() });
        assert_eq!(q.take(), Some(Typed { text: "abc".into(), ..Typed::default() }));
        assert_eq!(q.take(), Some(Typed { erase: 3, ..Typed::default() }));
        assert_eq!(q.take(), Some(Typed { done: true, ..Typed::default() }));
        assert_eq!(q.take(), None);
        q.push(&Typed { text: "abcdefghijklm".into(), ..Typed::default() });
        assert_eq!(q.take().unwrap().text, "abcdefgh");
        assert_eq!(q.take().unwrap().text, "ijklm");
        assert_eq!(Typed::text("a€b\u{1}ü").text, "abü", "only the signs the font writes");
    }

    /// The tick takes a pointer from this level's picture alone, a panel
    /// only as one can be.
    #[test]
    fn a_pointer_from_another_pictures_is_nil() {
        let p = Pointer { x: 3, y: 4, picture: Some(7), panel: Some(Panel { name: "bag".into(), index: 2, u: 500, v: 1000 }), press: true, ..Pointer::default() };
        assert_eq!(p.checked(8), None);
        let ok = p.checked(7).unwrap();
        assert!(!ok.press && ok.panel.is_some());
        let bad = Pointer { panel: Some(Panel { name: "no such!".into(), ..Panel::default() }), ..p.clone() };
        assert_eq!(bad.checked(7).unwrap().panel, None);
        let far = Pointer { panel: Some(Panel { u: 1001, ..Panel::default() }), ..p };
        assert_eq!(far.checked(7).unwrap().panel, None);
    }
}
