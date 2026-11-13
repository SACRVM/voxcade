//! `controls`: what the player plays with.

use super::*;

/// `controls`: what the player plays with, the device and the keys as the
/// player sees them. It comes from their machine, so replays carry it
/// (`Session::set_controls`) and the game may go by it anywhere. Seat 0 is
/// the global `controls`: seat 1's, in `on_draw` the machine drawing's
/// first player's.
pub struct ControlsView(pub usize);

impl ControlsView {
    fn of<'a>(&self, lua: &Lua, h: &'a Stage) -> &'a Controls {
        match self.0 {
            0 if looking(lua) => h.drawn_controls(),
            seat => h.controls_of(seat),
        }
    }
}

impl Stage {
    /// What seat `seat`'s player plays with (seat 1's: `controls`).
    fn controls_of(&self, seat: usize) -> &Controls {
        match seat {
            0 | 1 => &self.controls,
            _ => &self.seats[seat - 1].controls,
        }
    }

    /// The players at the machine drawing (`frame.players`): its seats
    /// taken, every taken one at one machine.
    pub fn drawn_seats(&self) -> Vec<usize> {
        (1..=self.seats.len()).filter(|&n| self.seats[n - 1].taken && self.local.has(n)).collect()
    }

    /// What the first of them plays with (seat 1's where none sits at the
    /// machine drawing): the keys in braces in `on_draw`'s texts.
    pub(super) fn drawn_controls(&self) -> &Controls {
        self.controls_of(self.drawn_seats().first().copied().unwrap_or(1))
    }
}

impl UserData for ControlsView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field(MetaMethod::Type, "controls");
        f.add_field_method_get("device", |lua, c| look(lua, |h| Ok(c.of(lua, h).device.map(|d| d.name()))));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("controls"));
        m.add_meta_function(MetaMethod::Index, |_, (_, key): (AnyUserData, Value)| -> mlua::Result<Value> { Err(no_such("controls", &key, None)) });
        m.add_meta_function(MetaMethod::NewIndex, |_, (_, key, _): (AnyUserData, Value, Value)| -> mlua::Result<()> { Err(not_settable("controls", &key, None)) });
        m.add_method("key", |lua, c, action: String| look(lua, |h| key_of(c.of(lua, h), &action, "controls:key").map(str::to_owned)));
    }
}
