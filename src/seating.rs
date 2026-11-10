//! Who plays on which seat at one machine (`docs/game-model.md`, "Several
//! players"): each seat has a part of the keyboard (all of it, a half, or
//! none) and a pad, or nothing. The PLAYERS page sets them (`set_keyboard`,
//! `set_pad`, `swap`) and the settings keep them as a plan (`SeatPlan`):
//! what the page shows plays from the run's start. START is the quick way
//! in: a pad no seat has takes its planned seat or the first free one, the
//! keyboard its start key (Return) the same, all of it while seat 1 plays
//! on a pad (the user's wish: one on the pad, the other on WASD and the
//! game's X and Z), its right half while seat 1 plays on the keyboard. The
//! halves' keys are the player's (`Settings::left_keys`, `right_keys`),
//! `LEFT_HALF` and `RIGHT_HALF` to start with. A game of one seat plays
//! every device on seat 1, as always.

use serde::{Deserialize, Serialize};

/// A press of START (a pad's, or the keyboard's start key): where it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Someone took this seat (2 and on).
    Join(usize),
    /// The player on this seat pauses the game.
    Pause(usize),
    /// Every seat is taken: nothing.
    Full,
}

/// The keys of the keyboard's halves once a second player plays at it, to
/// start with: the four directions (left, right, up, down), then the
/// game's actions in order (the first two are what most games have: fire
/// and jump).
pub const LEFT_HALF: ([&str; 4], [&str; 6]) = (["A", "D", "W", "S"], ["F", "G", "R", "T", "V", "B"]);
pub const RIGHT_HALF: ([&str; 4], [&str; 6]) = (["Left", "Right", "Up", "Down"], ["K", "L", "I", "O", "Comma", "Period"]);
/// The left half's pause to start with.
pub const LEFT_PAUSE: &str = "Escape";
/// The keyboard's start key: takes a seat, then pauses (the right half's
/// pause to start with).
pub const KEYBOARD_START: &str = "Return";
/// The four directions by name, as a half's bindings have them, in the
/// order of `LEFT_HALF`'s.
pub const DIRECTIONS: [&str; 4] = ["left", "right", "up", "down"];

/// Which keys of the keyboard a seat plays with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keys {
    /// All of it, as the player's settings bind them.
    Whole,
    Left,
    Right,
    #[default]
    Nothing,
}

impl Keys {
    /// The PLAYERS page's choices in turn.
    const ALL: [Keys; 4] = [Keys::Nothing, Keys::Whole, Keys::Left, Keys::Right];

    /// `step` choices on, round the list.
    pub fn step(self, step: i32) -> Self {
        let at = Self::ALL.iter().position(|&k| k == self).unwrap_or(0) as i32;
        Self::ALL[(at + step).rem_euclid(Self::ALL.len() as i32) as usize]
    }

    /// Its word on the PLAYERS page.
    pub fn label(self) -> &'static str {
        match self {
            Keys::Whole => "ALL",
            Keys::Left => "LEFT HALF",
            Keys::Right => "RIGHT HALF",
            Keys::Nothing => "NONE",
        }
    }

    fn is_nothing(&self) -> bool {
        *self == Keys::Nothing
    }
}

/// A seat as the settings keep it (the PLAYERS page): its part of the
/// keyboard and its pad by name (a second pad of the same name "NAME#2").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeatPlan {
    #[serde(default, skip_serializing_if = "Keys::is_nothing")]
    pub keyboard: Keys,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad: Option<String>,
}

/// The settings' plan with the pads plugged in (`named`: each by its id
/// and the name the plan keeps it by): each seat's keyboard and pad,
/// `None` for a pad not there.
pub fn resolve(plan: &[SeatPlan], named: &[(u32, String)]) -> Vec<(Keys, Option<u32>)> {
    plan.iter().map(|s| (s.keyboard, s.pad.as_ref().and_then(|name| named.iter().find(|(_, n)| n == name)).map(|(id, _)| *id))).collect()
}

/// What the settings keep of a seating (the PLAYERS page changed it):
/// each seat's keyboard and its pad by name; a pad the plan had that is
/// not plugged in stays planned, but where its seat's pad was just
/// chosen (`chosen`).
pub fn plan_of(seating: &Seating, named: &[(u32, String)], was: &[SeatPlan], chosen: Option<usize>) -> Vec<SeatPlan> {
    seating
        .plan()
        .into_iter()
        .enumerate()
        .map(|(i, (keyboard, pad))| {
            let name = pad.and_then(|id| named.iter().find(|(p, _)| *p == id)).map(|(_, n)| n.clone());
            let absent = was.get(i).and_then(|s| s.pad.clone()).filter(|n| !named.iter().any(|(_, m)| m == n) && chosen != Some(i + 1));
            SeatPlan { keyboard, pad: name.or(absent) }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct Seating {
    /// Each seat's part of the keyboard and its pad (by its id), seat 1
    /// first.
    keyboard: Vec<Keys>,
    pads: Vec<Option<u32>>,
    /// Whether someone sits there (seat 1 always).
    taken: Vec<bool>,
}

impl Seating {
    /// Seat 1 with the keyboard, the others free.
    pub fn new(seats: u32) -> Self {
        let seats = seats.max(1) as usize;
        let mut keyboard = vec![Keys::Nothing; seats];
        keyboard[0] = Keys::Whole;
        Self { keyboard, pads: vec![None; seats], taken: (0..seats).map(|n| n == 0).collect() }
    }

    /// A new run as `plan` says (each seat's keyboard and pad, the pads
    /// found among those plugged in: `None` where its pad is not there;
    /// none: seat 1 with the keyboard); the pad that began it (`pad`) seat
    /// 1's where no seat has it and seat 1 has none; seat 1 with nothing
    /// of its own there plays on the keyboard.
    pub fn begin(&mut self, pad: Option<u32>, plan: &[(Keys, Option<u32>)]) {
        *self = Self::new(self.seats() as u32);
        if !plan.is_empty() {
            self.keyboard[0] = Keys::Nothing;
            for (n, &(keys, pad)) in plan.iter().enumerate().take(self.seats()) {
                if keys != Keys::Nothing {
                    self.set_keyboard(n + 1, keys);
                }
                if pad.is_some() {
                    self.set_pad(n + 1, pad);
                }
            }
        }
        if let Some(pad) = pad.filter(|&p| self.pad_seat(p).is_none() && self.pads[0].is_none()) {
            self.pads[0] = Some(pad);
        }
        if self.keyboard[0] == Keys::Nothing && self.pads[0].is_none() {
            self.set_keyboard(1, Keys::Whole);
        }
        self.taken[0] = true;
    }

    /// A game of one seat: every device is seat 1's.
    pub fn alone(&self) -> bool {
        self.seats() == 1
    }

    pub fn seats(&self) -> usize {
        self.taken.len()
    }

    /// The seat a pad plays on (a game of one: seat 1, every pad).
    pub fn pad_seat(&self, pad: u32) -> Option<usize> {
        if self.alone() {
            return Some(1);
        }
        self.pads.iter().position(|&p| p == Some(pad)).map(|i| i + 1)
    }

    /// The pad of a seat.
    pub fn pad(&self, seat: usize) -> Option<u32> {
        self.pads.get(seat.wrapping_sub(1)).copied().flatten()
    }

    /// A pad's START: its seat's pause, or a free seat taken: `planned`
    /// (the seat the PLAYERS page gave it), else one whose pad is gone
    /// (its player picked up another), else the first.
    pub fn pad_start(&mut self, pad: u32, plugged: &[u32], planned: Option<usize>) -> Start {
        if let Some(seat) = self.pad_seat(pad) {
            return Start::Pause(seat);
        }
        let orphan = (2..=self.seats()).find(|&s| self.taken[s - 1] && self.keyboard[s - 1] == Keys::Nothing && self.pads[s - 1].is_none_or(|p| !plugged.contains(&p)));
        match planned.filter(|&s| s > 1 && s <= self.seats() && !self.taken[s - 1]).or(orphan).or_else(|| self.free()) {
            Some(seat) => {
                self.pads[seat - 1] = Some(pad);
                self.taken[seat - 1] = true;
                Start::Join(seat)
            }
            None => Start::Full,
        }
    }

    /// The keyboard's start key: the pause of the seat after the first
    /// that has it (all or its right half), or a free seat taken
    /// (`planned`, else the first): all of it while seat 1 plays on a pad,
    /// else its right half.
    pub fn keyboard_start(&mut self, planned: Option<usize>) -> Start {
        if self.alone() {
            return Start::Pause(1);
        }
        if let Some(seat) = (2..=self.seats()).find(|&s| matches!(self.keyboard[s - 1], Keys::Whole | Keys::Right)) {
            return Start::Pause(seat);
        }
        let Some(seat) = planned.filter(|&s| s > 1 && s <= self.seats() && !self.taken[s - 1]).or_else(|| self.free()) else { return Start::Full };
        let keys = match self.keyboard[0] {
            Keys::Whole if self.pads[0].is_some() => Keys::Whole,
            Keys::Whole | Keys::Left => Keys::Right,
            Keys::Right => Keys::Left,
            Keys::Nothing => Keys::Whole,
        };
        self.set_keyboard(seat, keys);
        Start::Join(seat)
    }

    /// The seat after the first that has all of the keyboard: Escape
    /// pauses for it (its pause offers LEAVE).
    pub fn keyboard_seat(&self) -> Option<usize> {
        (2..=self.seats()).find(|&s| self.keyboard[s - 1] == Keys::Whole)
    }

    /// The player on `seat` (2 and on) leaves it: its devices free again
    /// (the keyboard whole for whoever has the other half, seat 1's where
    /// nobody has any of it).
    pub fn leave(&mut self, seat: usize) {
        if seat < 2 || seat > self.seats() {
            return;
        }
        self.taken[seat - 1] = false;
        self.pads[seat - 1] = None;
        if std::mem::take(&mut self.keyboard[seat - 1]) != Keys::Nothing {
            match self.keyboard.iter().position(|&k| matches!(k, Keys::Left | Keys::Right)) {
                Some(other) => self.keyboard[other] = Keys::Whole,
                None if self.keyboard.iter().all(|&k| k == Keys::Nothing) => self.keyboard[0] = Keys::Whole,
                None => {}
            }
        }
    }

    pub fn taken(&self, seat: usize) -> bool {
        self.taken.get(seat.wrapping_sub(1)).copied().unwrap_or(false)
    }

    /// The keys seat `seat` plays with.
    pub fn keys(&self, seat: usize) -> Keys {
        self.keyboard.get(seat.wrapping_sub(1)).copied().unwrap_or(Keys::Nothing)
    }

    /// Seat `seat` plays with `keys` (the PLAYERS page): all of the
    /// keyboard taken from the others, a half from whoever has it (from
    /// one with all of it: it keeps the other half). A seat after the
    /// first given a device takes the seat, one left with none leaves it.
    pub fn set_keyboard(&mut self, seat: usize, keys: Keys) {
        let Some(i) = seat.checked_sub(1).filter(|&i| i < self.seats()) else { return };
        for (o, k) in self.keyboard.iter_mut().enumerate() {
            if o == i {
                continue;
            }
            *k = match (keys, *k) {
                (Keys::Whole, _) => Keys::Nothing,
                (Keys::Left, Keys::Whole) => Keys::Right,
                (Keys::Right, Keys::Whole) => Keys::Left,
                (Keys::Left | Keys::Right, k) if k == keys => Keys::Nothing,
                (_, k) => k,
            };
        }
        self.keyboard[i] = keys;
        self.settle();
    }

    /// Seat `seat` plays with this pad (None: with none), taken from any
    /// other seat; taking and leaving seats as `set_keyboard`.
    pub fn set_pad(&mut self, seat: usize, pad: Option<u32>) {
        let Some(i) = seat.checked_sub(1).filter(|&i| i < self.seats()) else { return };
        if pad.is_some() {
            for p in self.pads.iter_mut().filter(|p| **p == pad) {
                *p = None;
            }
        }
        self.pads[i] = pad;
        self.settle();
    }

    /// Two seats change devices (their players stay where they are in the
    /// game: the people swap).
    pub fn swap(&mut self, a: usize, b: usize) {
        if a == b || a == 0 || b == 0 || a > self.seats() || b > self.seats() {
            return;
        }
        self.keyboard.swap(a - 1, b - 1);
        self.pads.swap(a - 1, b - 1);
        self.settle();
    }

    /// What it keeps as a plan: each seat's keyboard and pad.
    pub fn plan(&self) -> Vec<(Keys, Option<u32>)> {
        self.keyboard.iter().copied().zip(self.pads.iter().copied()).collect()
    }

    /// Seats after the first taken while they have a device, left without
    /// one; seat 1 stays.
    fn settle(&mut self) {
        for i in 1..self.seats() {
            self.taken[i] = self.keyboard[i] != Keys::Nothing || self.pads[i].is_some();
        }
    }

    fn free(&self) -> Option<usize> {
        (2..=self.seats()).find(|&s| !self.taken[s - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pad's START takes its planned seat or the first free one, then
    /// pauses it; the keyboard goes whole while seat 1 plays on a pad,
    /// else its right half; leaving frees a seat (the keyboard whole again
    /// for the other half); a game of one seat plays everything on seat 1.
    #[test]
    fn devices_take_seats_and_leave_them() {
        let mut s = Seating::new(3);
        s.begin(Some(7), &[]);
        assert_eq!((s.pad_seat(7), s.keys(1)), (Some(1), Keys::Whole), "the pad that began the run, and the keyboard");
        assert_eq!(s.pad_seat(8), None, "a pad no seat has plays nothing");
        assert_eq!(s.pad_start(7, &[7, 8], None), Start::Pause(1));
        assert_eq!(s.pad_start(8, &[7, 8], Some(3)), Start::Join(3), "its planned seat");
        assert_eq!(s.pad_start(8, &[7, 8], None), Start::Pause(3));
        assert_eq!(s.keyboard_start(None), Start::Join(2));
        assert_eq!((s.keys(1), s.keys(2)), (Keys::Nothing, Keys::Whole), "seat 1 on its pad, the keyboard seat 2's");
        assert_eq!((s.keyboard_start(None), s.keyboard_seat()), (Start::Pause(2), Some(2)));
        assert_eq!(s.pad_start(9, &[7, 8, 9], None), Start::Full);
        s.leave(2);
        assert_eq!((s.taken(2), s.keys(1)), (false, Keys::Whole), "the keyboard seat 1's again");
        // Seat 3's pad unplugged: another pad picks the seat up again.
        assert_eq!(s.pad_start(10, &[7, 10], None), Start::Join(3));

        let mut s = Seating::new(2);
        s.begin(None, &[]);
        assert_eq!(s.keyboard_start(None), Start::Join(2));
        assert_eq!((s.keys(1), s.keys(2)), (Keys::Left, Keys::Right), "seat 1 on the keyboard: shared");
        s.leave(2);
        assert_eq!(s.keys(1), Keys::Whole, "the keyboard whole again");

        let mut one = Seating::new(1);
        one.begin(None, &[]);
        assert_eq!((one.pad_seat(5), one.pad_start(5, &[5], None), one.keyboard_start(None)), (Some(1), Start::Pause(1), Start::Pause(1)));
    }

    /// The PLAYERS page: a device given to a seat is taken from the others
    /// (half of the keyboard from whoever has all of it), a seat with a
    /// device plays, one left with none leaves; two seats swap devices;
    /// the plan begins the next run as it was, the pad that began it seat
    /// 1's only where nobody has it.
    #[test]
    fn the_players_page_gives_each_seat_its_devices() {
        let mut s = Seating::new(3);
        s.begin(Some(7), &[]);
        s.set_keyboard(2, Keys::Left);
        assert_eq!((s.keys(1), s.keys(2), s.taken(2)), (Keys::Right, Keys::Left, true), "half of it from seat 1");
        s.set_pad(3, Some(7));
        assert_eq!((s.pad(1), s.pad(3), s.taken(3)), (None, Some(7), true), "the pad from seat 1 to seat 3");
        s.set_keyboard(1, Keys::Whole);
        assert_eq!((s.keys(1), s.keys(2), s.taken(2)), (Keys::Whole, Keys::Nothing, false), "all of it: seat 2 left with nothing leaves");
        s.swap(1, 3);
        assert_eq!((s.keys(1), s.pad(1), s.keys(3), s.pad(3)), (Keys::Nothing, Some(7), Keys::Whole, None));
        let plan = s.plan();
        let mut next = Seating::new(3);
        next.begin(Some(9), &plan);
        assert_eq!((next.pad(1), next.keys(3), next.taken(3), next.pad_seat(9)), (Some(7), Keys::Whole, true, None), "the plan, the beginning pad free");
        let mut fresh = Seating::new(3);
        fresh.begin(Some(9), &[(Keys::Nothing, None), (Keys::Nothing, Some(4))]);
        assert_eq!((fresh.keys(1), fresh.pad(1), fresh.pad(2), fresh.taken(2)), (Keys::Nothing, Some(9), Some(4), true), "nothing of seat 1's planned: the beginning pad");
        fresh.begin(None, &[(Keys::Nothing, None), (Keys::Whole, None)]);
        assert_eq!((fresh.keys(1), fresh.keys(2)), (Keys::Whole, Keys::Nothing), "seat 1's pad not there, nor one beginning: the keyboard");
    }
}
