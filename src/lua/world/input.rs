//! `input`: the keys held this tick; the stand-in for what is not there.

use super::*;

/// `input`: the keys held this tick, by the game's action names.
pub struct InputView {
    pub now: Input,
    pub before: Input,
    pub actions: Arc<Vec<&'static str>>,
    /// How many ticks each direction (the first four) and action has been
    /// held (`input:repeated`).
    pub held_for: [u16; KEYS],
    /// The pointer as the tick takes it (`input.pointer`; checked), and
    /// the text typed while its player types (`input.typed`).
    pub pointer: Option<crate::input::Pointer>,
    pub typed: Option<crate::input::Typed>,
}

/// What a seat's `input` shows besides its keys: whether the game has a
/// pointer and the level's picture now (`input::picture_mark`), whether
/// its player types.
#[derive(Clone, Copy, Debug)]
pub struct Gate {
    pub picture: Option<u16>,
    pub typing: bool,
}

impl Stage {
    /// What seat `seat`'s `input` shows besides its keys now.
    pub fn gate(&self, seat: usize) -> Gate {
        let mark = (self.level as u64) << 32 | self.visits.get(self.level).copied().unwrap_or(0) as u64;
        Gate { picture: self.pointer.then(|| crate::input::picture_mark(mark)), typing: self.seats.get(seat - 1).is_some_and(|s| s.typing) }
    }
}

impl InputView {
    /// A seat's `input`: its keys now and before, its pointer and text as
    /// `gate` lets them through.
    pub fn new(now: Input, before: Input, actions: Arc<Vec<&'static str>>, held_for: [u16; KEYS], gate: Gate) -> Self {
        let pointer = gate.picture.and_then(|picture| now.pointer.as_ref()?.checked(picture));
        let typed = gate.typing.then(|| now.typed.as_ref().map(|t| t.checked()).unwrap_or_default());
        Self { now, before, actions, held_for, pointer, typed }
    }
}

/// A stick's value as the script reads it: `{ x, y }`, each -1 to 1.
fn stick_table(lua: &Lua, x: f64, y: f64) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("x", x)?;
    t.raw_set("y", y)?;
    Ok(t)
}

/// `input.move`: the left stick, else the directions held (a diagonal
/// about 0.71 each way, never longer than 1).
fn moved(i: &Input) -> (f64, f64) {
    if i.moves != [0, 0] {
        let [x, y] = crate::input::stick_checked(i.moves);
        return (x as f64 / 127.0, y as f64 / 127.0);
    }
    let x = i.right as i32 - i.left as i32;
    let y = i.up as i32 - i.down as i32;
    if x != 0 && y != 0 {
        let d = std::f64::consts::FRAC_1_SQRT_2;
        return (x as f64 * d, y as f64 * d);
    }
    (x as f64, y as f64)
}

/// The four directions and the game's actions, each with its count of
/// ticks held.
pub const KEYS: usize = 4 + 32;
/// `input:repeated`: true as a key goes down, again after this many ticks
/// held, then every `REPEAT_EVERY`, as a menu moves on a held key.
pub const REPEAT_AFTER: u16 = 20;
pub const REPEAT_EVERY: u16 = 6;

/// Whether a key held `n` ticks repeats this tick.
pub fn repeats(n: u16) -> bool {
    n == 1 || (n > REPEAT_AFTER && (n - REPEAT_AFTER - 1).is_multiple_of(REPEAT_EVERY))
}

impl InputView {
    /// An action or a direction by its name: held now, and a tick before.
    fn key(&self, name: &str) -> mlua::Result<(bool, bool)> {
        let i = self.index(name)?;
        let held = |input: &Input| if i < 4 { input.directions()[i] } else { input.held(i - 4) };
        Ok((held(&self.now), held(&self.before)))
    }

    /// An action or a direction by its name, its place among `KEYS`.
    fn index(&self, name: &str) -> mlua::Result<usize> {
        if let Some(d) = crate::input::DIRECTIONS.iter().position(|d| *d == name) {
            return Ok(d);
        }
        match self.actions.iter().position(|a| *a == name) {
            Some(a) => Ok(4 + a),
            None if self.actions.is_empty() => Err(mlua::Error::runtime(format!("no action called '{name}': the game names no actions (c.actions = {{ \"{name}\" }} in conf.lua); the directions are left, right, up, down"))),
            None => Err(mlua::Error::runtime(format!("no action called '{name}' (c.actions: {}; or a direction: left, right, up, down)", self.actions.join(", ")))),
        }
    }
}

impl UserData for InputView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field(MetaMethod::Type, "input");
        f.add_field_method_get("left", |_, i| Ok(i.now.left));
        f.add_field_method_get("right", |_, i| Ok(i.now.right));
        f.add_field_method_get("up", |_, i| Ok(i.now.up));
        f.add_field_method_get("down", |_, i| Ok(i.now.down));
        f.add_field_method_get("dir", |lua, i| {
            let n = &i.now;
            let x = n.right as i32 - n.left as i32;
            let y = n.up as i32 - n.down as i32;
            let t = lua.create_table()?;
            t.raw_set("x", x)?;
            t.raw_set("y", y)?;
            Ok(t)
        });
        f.add_field_method_get("move", |lua, i| {
            let (x, y) = moved(&i.now);
            stick_table(lua, x, y)
        });
        f.add_field_method_get("aim", |lua, i| {
            let [x, y] = crate::input::stick_checked(i.now.aim);
            stick_table(lua, x as f64 / 127.0, y as f64 / 127.0)
        });
        f.add_field_method_get("pointer", |lua, i| {
            let Some(p) = &i.pointer else { return Ok(Value::Nil) };
            let t = lua.create_table()?;
            if let Some(panel) = p.panel.as_ref().filter(|panel| !panel.name.is_empty()) {
                let steps = crate::input::PANEL_STEPS as f64;
                t.raw_set("hud", panel.name.as_str())?;
                t.raw_set("index", (panel.index > 0).then_some(panel.index))?;
                t.raw_set("u", panel.u as f64 / steps)?;
                t.raw_set("v", panel.v as f64 / steps)?;
            }
            Ok(Value::Table(t))
        });
        f.add_field_method_get("typed", |lua, i| {
            let Some(typed) = &i.typed else { return Ok(Value::Nil) };
            let t = lua.create_table()?;
            t.raw_set("text", typed.text.as_str())?;
            t.raw_set("erase", typed.erase)?;
            t.raw_set("done", typed.done)?;
            t.raw_set("cancel", typed.cancel)?;
            Ok(Value::Table(t))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("input"));
        // `input.jump`: an action is asked by its name, never read as a field.
        m.add_meta_method(MetaMethod::Index, |_, i, key: Value| -> mlua::Result<Value> {
            let Value::String(s) = &key else { return Err(no_such("input", &key, None)) };
            let a = s.to_string_lossy();
            let ask = format!("an action is asked by its name: input:held(\"{a}\"), input:pressed(\"{a}\"), input:released(\"{a}\")");
            if i.actions.iter().any(|x| *x == a) {
                return Err(mlua::Error::runtime(format!("input has no `{a}`: {ask}")));
            }
            let items: Vec<&crate::lua::api::Item> = crate::lua::api::items_of("input").collect();
            if near_item(&a, &items).is_some() {
                return Err(no_such("input", &key, None));
            }
            let actions = if i.actions.is_empty() { "the game names none, c.actions = { ... } in conf.lua".to_string() } else { format!("c.actions: {}", i.actions.join(", ")) };
            Err(mlua::Error::runtime(format!("input has no `{a}`: {ask} ({actions}); the directions are input.left, input.right, input.up, input.down and input.dir")))
        });
        m.add_meta_function(MetaMethod::NewIndex, |_, (_, key, _): (AnyUserData, Value, Value)| -> mlua::Result<()> { Err(not_settable("input", &key, None)) });
        m.add_method("held", |_, i, name: String| Ok(i.key(&name)?.0));
        m.add_method("pressed", |_, i, name: String| {
            let (now, before) = i.key(&name)?;
            Ok(now && !before)
        });
        m.add_method("released", |_, i, name: String| {
            let (now, before) = i.key(&name)?;
            Ok(!now && before)
        });
        m.add_method("repeated", |_, i, name: String| Ok(repeats(i.held_for[i.index(&name)?])));
    }
}

/// `input` outside `on_tick`, `screen` outside `on_draw`: using it says
/// where it is.
pub struct Absent(pub &'static str);

impl UserData for Absent {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |_, a, _: Value| -> mlua::Result<Value> { Err(mlua::Error::runtime(a.0)) });
        m.add_meta_method(MetaMethod::ToString, |_, a, ()| Ok(a.0.split(':').next().unwrap_or("").to_string()));
    }
}
