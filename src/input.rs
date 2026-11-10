//! What the player holds during a tick: the four directions and the game's
//! actions. Each game names its actions (More Blood: jump, fire, grenade,
//! strafe); keys and buttons are bound to them by name, and replays and
//! `shot` scripts write them by name.

/// The directions, by their names in scripts.
pub const DIRECTIONS: [&str; 4] = ["left", "right", "up", "down"];

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    /// Bit n: the game's n-th action is held.
    pub actions: u32,
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
}
