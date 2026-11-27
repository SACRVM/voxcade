//! What the player presses: the keyboard keys and gamepad buttons for each
//! action (changeable in the settings), and how the menus are steered.
//! Plain names, so the bindings can be saved and shown; reading them from
//! SDL (`read_input`) serves the game's window and the IDE's alike.

use std::collections::BTreeMap;

use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::keyboard::{KeyboardState, Keycode, Scancode};
use serde::{Deserialize, Serialize};

use crate::config::Stick;

/// Which kind of controller something came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Device {
    #[default]
    Keyboard,
    Gamepad,
}

impl Device {
    /// Its name for the scripts (`controls.device`).
    pub fn name(self) -> &'static str {
        match self {
            Device::Keyboard => "keyboard",
            Device::Gamepad => "gamepad",
        }
    }
}

/// What the player plays with as a game sees it (`controls` in Lua): the
/// device used last and each action's key as that device labels it. It
/// comes from the player's machine (the keyboard's layout, their bindings,
/// the pad in hand), not from the game, so replays carry it
/// (`Session::set_controls`, `replay::Recorder`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Controls {
    /// None where nobody plays (tests, `check`, a replay recorded before
    /// replays carried the controls).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<Device>,
    /// Every action of the game and pause, with its first key or button as
    /// the device labels it ("" where none is bound or nobody plays).
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    /// The language the player chose of the game's (`c.languages`;
    /// `world.language`); None where nobody plays: the game's first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl Controls {
    /// Nobody plays: the actions, without keys.
    pub fn nobody(actions: &[&str]) -> Self {
        Self { device: None, keys: actions.iter().chain([&PAUSE]).map(|a| (a.to_string(), String::new())).collect(), language: None }
    }

    /// The player's: `device` (a pad printing its buttons as `pad_style`
    /// does), the keys as `settings` bind and the keyboard labels them.
    pub fn of(actions: &[&str], device: Device, pad_style: PadStyle, s: &crate::settings::Settings) -> Self {
        Self::labelled(actions, device, s, |action| match device {
            Device::Keyboard => s.keys.get(action).first().map(|k| s.key_label(k)),
            Device::Gamepad => s.pad.get(action).first().map(|b| pad_label(b, pad_style)),
        })
    }

    /// A player's on one half of the shared keyboard: the half's keys
    /// (`Settings::left_keys`, `right_keys`), as the keyboard labels them.
    pub fn of_half(actions: &[&str], half: crate::seating::Keys, s: &crate::settings::Settings) -> Self {
        let bound = s.keys_of(half);
        Self::labelled(actions, Device::Keyboard, s, |action| bound.and_then(|b| b.get(action).first()).map(|k| s.key_label(k)))
    }

    /// A player's on a pad of its own buttons (`Settings::pad_of`).
    pub fn of_pad(actions: &[&str], pad_style: PadStyle, pad: &Bindings, s: &crate::settings::Settings) -> Self {
        Self::labelled(actions, Device::Gamepad, s, |action| pad.get(action).first().map(|b| pad_label(b, pad_style)))
    }

    /// A player's on `device`: each action and pause with its `label`, the
    /// language chosen in `s`.
    fn labelled(actions: &[&str], device: Device, s: &crate::settings::Settings, label: impl Fn(&str) -> Option<String>) -> Self {
        let keys = actions.iter().chain([&PAUSE]).map(|a| (a.to_string(), label(a).unwrap_or_default())).collect();
        Self { device: Some(device), keys, language: Some(s.language().to_string()) }
    }

    /// These controls with a key for each of `actions` and pause: a replay
    /// recorded before an action came has none for it, and shows it
    /// unbound, as where nobody plays (a text naming it would stop the game).
    pub fn with_actions(mut self, actions: &[&str]) -> Self {
        for action in actions.iter().chain([&PAUSE]) {
            self.keys.entry(action.to_string()).or_default();
        }
        self
    }
}

/// The engine's own action besides the game's: pausing.
pub const PAUSE: &str = "pause";

/// For each action (the game's, by name, and `PAUSE`) the keys (SDL
/// scancode names, like "Left Shift") or the gamepad buttons (see
/// `pad_label`) that do it; the first is the one shown. Saved as a table
/// of the actions' names. Moving is always the arrows and WASD, the left
/// stick and the d-pad.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(transparent)]
pub struct Bindings(BTreeMap<String, Vec<String>>);

impl Bindings {
    /// From (action, keys) pairs.
    pub fn new(pairs: &[(&str, &[&str])]) -> Self {
        Self(pairs.iter().map(|(a, keys)| (a.to_string(), keys.iter().map(|k| k.to_string()).collect())).collect())
    }

    /// Pausing on the keyboard: Escape or P.
    pub fn pause_keys() -> Self {
        Self::new(&[(PAUSE, &["Escape", "P"])])
    }

    /// Pausing on a gamepad: start.
    pub fn pause_pad() -> Self {
        Self::new(&[(PAUSE, &["start"])])
    }

    pub fn get(&self, action: &str) -> &[String] {
        self.0.get(action).map_or(&[], Vec::as_slice)
    }

    /// Makes `name` the first binding of `action` and takes it off every
    /// other action, so one key never does two things.
    pub fn bind(&mut self, action: &str, name: &str) {
        for keys in self.0.values_mut() {
            keys.retain(|n| !n.eq_ignore_ascii_case(name));
        }
        self.0.entry(action.to_string()).or_default().insert(0, name.to_string());
    }

    /// The action `name` is bound to, if any.
    pub fn action_of(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(_, keys)| keys.iter().any(|n| n.eq_ignore_ascii_case(name))).map(|(a, _)| a.as_str())
    }

    /// Takes the defaults for the actions it has nothing for (a new game, or
    /// an action a game added).
    pub fn fill(&mut self, defaults: &Bindings) {
        for (action, keys) in &defaults.0 {
            self.0.entry(action.clone()).or_insert_with(|| keys.clone());
        }
    }

    /// Pausing gives way to the game: a key or button one of its actions
    /// has pauses no more (Sokovox's P, or one bound before the game
    /// took it), so one key never does two things.
    pub fn pause_gives_way(&mut self) {
        let taken: Vec<String> = self.0.iter().filter(|(a, _)| a.as_str() != PAUSE).flat_map(|(_, keys)| keys.iter().map(|n| n.to_lowercase())).collect();
        if let Some(pause) = self.0.get_mut(PAUSE) {
            pause.retain(|n| !taken.contains(&n.to_lowercase()));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every key or button named in it.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.values().flatten().map(String::as_str)
    }

    /// Every action with each key or button it has, in the actions' order.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().flat_map(|(a, keys)| keys.iter().map(move |k| (a.as_str(), k.as_str())))
    }
}

/// Which letters a gamepad prints on its buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PadStyle {
    #[default]
    Xbox,
    /// Switch-style: B at the bottom, A on the right.
    Nintendo,
    PlayStation,
}

impl PadStyle {
    /// By name: xbox, nintendo or playstation (`shot --pad`).
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "xbox" => PadStyle::Xbox,
            "nintendo" => PadStyle::Nintendo,
            "playstation" => PadStyle::PlayStation,
            _ => return None,
        })
    }
}

/// How a gamepad button is shown: as the pad in hand prints it.
pub fn pad_label(name: &str, style: PadStyle) -> String {
    use PadStyle::*;
    match (name, style) {
        ("south", Xbox) => "A",
        ("east", Xbox) => "B",
        ("west", Xbox) => "X",
        ("north", Xbox) => "Y",
        ("south", Nintendo) => "B",
        ("east", Nintendo) => "A",
        ("west", Nintendo) => "Y",
        ("north", Nintendo) => "X",
        ("south", PlayStation) => "CROSS",
        ("east", PlayStation) => "CIRCLE",
        ("west", PlayStation) => "SQUARE",
        ("north", PlayStation) => "TRIANGLE",
        ("leftshoulder", Xbox) => "LB",
        ("rightshoulder", Xbox) => "RB",
        ("lefttrigger", Xbox) => "LT",
        ("righttrigger", Xbox) => "RT",
        ("leftshoulder", Nintendo) => "L",
        ("rightshoulder", Nintendo) => "R",
        ("lefttrigger", Nintendo) => "ZL",
        ("righttrigger", Nintendo) => "ZR",
        ("leftshoulder", PlayStation) => "L1",
        ("rightshoulder", PlayStation) => "R1",
        ("lefttrigger", PlayStation) => "L2",
        ("righttrigger", PlayStation) => "R2",
        ("start", Nintendo) => "+",
        ("back", Nintendo) => "-",
        ("start", PlayStation) => "OPTIONS",
        ("back", PlayStation) => "SHARE",
        ("start", _) => "START",
        ("back", _) => "BACK",
        ("leftstick", PlayStation) => "L3",
        ("rightstick", PlayStation) => "R3",
        ("leftstick", _) => "L3",
        ("rightstick", _) => "R3",
        ("dpup", _) => "UP",
        ("dpdown", _) => "DOWN",
        ("dpleft", _) => "LEFT",
        ("dpright", _) => "RIGHT",
        (other, _) => return other.to_uppercase(),
    }
    .to_string()
}

/// How a key is shown: its SDL name in capitals; the mouse's as words
/// ("MOUSE LEFT", "WHEEL DOWN").
pub fn key_label(name: &str) -> String {
    if crate::input::MOUSE_KEYS.contains(&name.to_lowercase().as_str()) {
        return name.replace('_', " ").to_uppercase();
    }
    name.to_uppercase()
}

/// Steering a menu, from any device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    /// The pause key or the start button.
    Pause,
}

// Reading the keyboard and gamepads (the game's window and the IDE's).

/// The gamepad's buttons by their names in the bindings (positional, like
/// SDL's).
const BUTTONS: [(Button, &str); 14] = [
    (Button::South, "south"),
    (Button::East, "east"),
    (Button::West, "west"),
    (Button::North, "north"),
    (Button::LeftShoulder, "leftshoulder"),
    (Button::RightShoulder, "rightshoulder"),
    (Button::LeftStick, "leftstick"),
    (Button::RightStick, "rightstick"),
    (Button::Start, "start"),
    (Button::Back, "back"),
    (Button::DPadUp, "dpup"),
    (Button::DPadDown, "dpdown"),
    (Button::DPadLeft, "dpleft"),
    (Button::DPadRight, "dpright"),
];

/// A gamepad button's name in the bindings.
pub fn button_name(button: Button) -> Option<&'static str> {
    BUTTONS.iter().find(|(b, _)| *b == button).map(|(_, name)| *name)
}

/// The gamepad's triggers, bound as its buttons are.
const TRIGGERS: [(Axis, &str); 2] = [(Axis::TriggerLeft, "lefttrigger"), (Axis::TriggerRight, "righttrigger")];

/// Every name a binding gives a pad's button or trigger (`c.pad`), by
/// where it lies: the names `pad_held` reads.
pub fn pad_names() -> impl Iterator<Item = &'static str> {
    BUTTONS.iter().map(|(_, n)| *n).chain(TRIGGERS.iter().map(|(_, n)| *n))
}

/// Whether the button (or trigger) a binding names is held on `pad`.
pub fn pad_held(pad: &Gamepad, name: &str) -> bool {
    if let Some((axis, _)) = TRIGGERS.iter().find(|(_, n)| *n == name) {
        return pad.axis(*axis) > 12_000;
    }
    BUTTONS.iter().find(|(_, n)| *n == name).is_some_and(|(b, _)| pad.button(*b))
}

/// The keys that walk each way (`input::DIRECTIONS`' order) on the whole
/// keyboard (`read_seat`) and on its halves as they begin
/// (`seating::LEFT_HALF`, `RIGHT_HALF`: WASD and the arrows), and the
/// d-pad's buttons.
fn walks(table: &str, way: usize, name: &str) -> bool {
    use crate::seating::{LEFT_HALF, RIGHT_HALF};
    if table == "pad" {
        return ["dpleft", "dpright", "dpup", "dpdown"][way] == name;
    }
    let key = Scancode::from_name(name);
    key.is_some() && [LEFT_HALF.0[way], RIGHT_HALF.0[way]].iter().any(|k| Scancode::from_name(k) == key)
}

/// Why a direction of `conf.lua` bound to a key or button that does not
/// walk that way would do nothing, and what to do instead.
fn not_walking(table: &str, way: usize, name: &str) -> String {
    use crate::seating::{LEFT_HALF, RIGHT_HALF};
    let dir = crate::input::DIRECTIONS[way];
    let walk = if table == "pad" {
        format!("on a pad the d-pad (\"{}\") and a stick walk {dir}", ["dpleft", "dpright", "dpup", "dpdown"][way])
    } else {
        format!("on the keyboard \"{}\" and \"{}\" walk {dir}", RIGHT_HALF.0[way], LEFT_HALF.0[way])
    };
    let players = if table == "pad" { "" } else { ". In a game of several a player sharing the keyboard may change the keys of their half's directions (CONTROLS, KEYS)" };
    format!("\"{name}\" does not walk {dir}: {walk}, and a game's conf.lua cannot give the four directions other keys or buttons (stating those is fine); to have \"{name}\" do something, bind it to an action of your own (c.actions){players}")
}

/// The letters on an Xbox pad's face buttons and where they lie (a
/// Nintendo pad has them the other way round).
const XBOX_LETTERS: [(&str, &str); 4] = [("a", "south"), ("b", "east"), ("x", "west"), ("y", "north")];

/// A binding a game's `conf.lua` gives (`c.keys`, `c.pad`) that would do
/// nothing: its table (`keys`, `pad`), the action, the key or button (none:
/// the action is the mistake) and the mistake in a game author's words.
pub struct DeadBinding {
    pub table: &'static str,
    pub action: String,
    pub name: Option<String>,
    pub why: String,
}

impl DeadBinding {
    /// The mistake as `conf.lua`'s others read: `c.pad.act: ...`.
    pub fn said(&self) -> String {
        format!("c.{}.{}: {}", self.table, self.action, self.why)
    }
}

/// The bindings of a game's own `conf.lua` that would do nothing, in the
/// tables' order: an action the game lacks (`actions`: a kit's and
/// `c.actions`; `pause` besides), a direction bound to a key or button
/// that does not walk it anyway (`walks`: the arrows and WASD, the d-pad;
/// stating those is fine), a key no key's name (SDL's, as on a US
/// keyboard; the mouse's, `input::MOUSE_KEYS`, with a `pointer` only), a
/// button none of `pad_names`. A player's settings are never asked: a
/// stale name there does nothing, as before.
pub fn dead_bindings(keys: &Bindings, pad: &Bindings, actions: &[&str], pointer: bool) -> Vec<DeadBinding> {
    use crate::audio::words::near;
    let mut out = Vec::new();
    let known: Vec<&str> = actions.iter().copied().chain([PAUSE]).collect();
    let pads: Vec<&str> = pad_names().collect();
    let mouse = crate::input::MOUSE_KEYS;
    // SDL's names of the keys, small, to find the one meant.
    let sdl: Vec<String> = (1..Scancode::Count as i32).filter_map(Scancode::from_i32).map(|c| c.name().to_lowercase()).filter(|n| !n.is_empty()).collect();
    for (table, bindings) in [("keys", keys), ("pad", pad)] {
        for (action, names) in &bindings.0 {
            let dead = |name: Option<&String>, why: String| DeadBinding { table, action: action.clone(), name: name.cloned(), why };
            let way = crate::input::DIRECTIONS.iter().position(|d| d == action);
            if way.is_none() && !known.contains(&action.as_str()) {
                let meant = near(action, known.iter().copied()).map_or(String::new(), |n| format!(" (did you mean {n}?)"));
                out.push(dead(None, format!("the game has no action {action}{meant}; its actions: {} (c.actions names its own)", known.join(", "))));
                continue;
            }
            for name in names {
                // A direction may state what walks it anyway, nothing else.
                if let Some(way) = way {
                    let small = name.to_lowercase();
                    let named = if table == "pad" { pads.contains(&name.as_str()) } else { Scancode::from_name(name).is_some() || (pointer && mouse.contains(&small.as_str())) };
                    if named {
                        if !walks(table, way, name) {
                            out.push(dead(Some(name), not_walking(table, way, name)));
                        }
                        continue;
                    }
                }
                let why = if table == "pad" {
                    if pads.contains(&name.as_str()) {
                        continue;
                    }
                    let list = pads.join(", ");
                    match XBOX_LETTERS.iter().find(|(l, _)| name.eq_ignore_ascii_case(l)) {
                        Some((letter, at)) => format!(
                            "\"{name}\" is no button's name: a pad's buttons go by where they lie, as the letters on them differ (an Xbox pad's A is \"south\", B \"east\", X \"west\", Y \"north\"; a Nintendo pad has its letters the other way round), so the Xbox pad's {} is \"{at}\"; the names: {list}",
                            letter.to_uppercase()
                        ),
                        None => {
                            let meant = near(name, pads.iter().copied()).map_or(String::new(), |n| format!(" (did you mean \"{n}\"?)"));
                            format!("\"{name}\" is no button's name{meant}; a pad's buttons go by where they lie: {list}")
                        }
                    }
                } else {
                    let small = name.to_lowercase();
                    if mouse.contains(&small.as_str()) {
                        if pointer {
                            continue;
                        }
                        format!("\"{name}\" is the mouse's, and the game has no pointer: c.pointer = \"system\" (the system's arrow) or \"own\" (the game draws its own) in conf.lua")
                    } else if Scancode::from_name(name).is_some() {
                        continue;
                    } else {
                        let among = sdl.iter().map(String::as_str).chain(mouse.iter().copied().filter(|_| pointer));
                        let meant = near(&small, among).map(|n| Scancode::from_name(n).map_or(n.to_string(), |c| c.name().to_string()));
                        let meant = meant.map_or(String::new(), |n| format!(" (did you mean \"{n}\"?)"));
                        let theirs = if pointer { format!(", or the mouse's: {}", mouse.join(", ")) } else { String::new() };
                        format!("\"{name}\" is no key's name{meant}: keys go by their names on a US keyboard, as SDL writes them (\"Z\", \"Space\", \"Left Shift\", \"Return\"){theirs}")
                    }
                };
                out.push(dead(Some(name), why));
            }
        }
    }
    out
}

/// Which keys are down: the keyboard, or the keyboard with keys held
/// besides by another session driving the IDE (`ide::remote`).
pub trait KeyState {
    fn pressed(&self, key: Scancode) -> bool;

    /// Whether the mouse's button or wheel `name` (`input::MOUSE_KEYS`)
    /// holds this tick: only where the keyboard has the mouse.
    fn mouse(&self, _name: &str) -> bool {
        false
    }

    /// Whether the player types (`player.state.typing`): the keys that type
    /// hold no action then.
    fn typing(&self) -> bool {
        false
    }
}

/// The keyboard with the mouse's buttons and wheel as keys, and whether its
/// player types (the window's).
pub struct WithMouse<'a> {
    pub keys: &'a dyn KeyState,
    /// The mouse's key names held this tick.
    pub mouse: &'a [&'static str],
    pub typing: bool,
}

impl KeyState for WithMouse<'_> {
    fn pressed(&self, key: Scancode) -> bool {
        self.keys.pressed(key)
    }

    fn mouse(&self, name: &str) -> bool {
        self.mouse.iter().any(|m| m.eq_ignore_ascii_case(name))
    }

    fn typing(&self) -> bool {
        self.typing
    }
}

/// Whether a key types (a letter, a digit, a sign, space, Backspace, Enter,
/// Escape): while its player types it holds no action.
pub fn types(key: Scancode) -> bool {
    use Scancode as S;
    let name = key.name();
    (name.chars().count() == 1 && !name.starts_with(|c: char| c.is_whitespace()))
        || matches!(key, S::Space | S::Backspace | S::Return | S::KpEnter | S::Escape | S::Tab | S::Delete)
        || name.starts_with("Keypad")
}

impl KeyState for KeyboardState<'_> {
    fn pressed(&self, key: Scancode) -> bool {
        self.is_scancode_pressed(key)
    }
}

/// The keyboard's keys and these besides.
pub struct WithHeld<'a>(pub &'a KeyboardState<'a>, pub &'a [Scancode]);

impl KeyState for WithHeld<'_> {
    fn pressed(&self, key: Scancode) -> bool {
        self.0.is_scancode_pressed(key) || self.1.contains(&key)
    }
}

/// A stick's dead zone: leaning less is no lean.
pub const DEAD: f32 = 0.3;

/// What the player holds: moving on the arrows and WASD, the d-pad and the
/// left stick (the right one where `pointer` gives the left to the
/// pointer); the rest as the settings bind it. With it the pointer's
/// stick's lean (`sticks`).
pub fn read_input(keys: Option<&dyn KeyState>, pads: &[Gamepad], settings: &crate::settings::Settings, actions: &[&str], pointer: Option<Stick>) -> (crate::input::Input, [f32; 2]) {
    read_seat(keys, crate::seating::Keys::Whole, &pads.iter().collect::<Vec<_>>(), &settings.pad, settings, actions, pointer)
}

/// What the sticks hold, `left` and `right` as SDL gives them (-1 to 1, y
/// down): the four directions (`DIRECTIONS`' order), `input.move` and
/// `input.aim`, and the lean of the stick `pointer` gives the pointer
/// (`c.pointer_stick`; 0, 0 without), which then holds no directions: the
/// other walks, and the aim is the pointer's (`Session::mouse_aim`).
pub fn sticks(left: [f32; 2], right: [f32; 2], pointer: Option<Stick>) -> ([bool; 4], [i8; 2], [i8; 2], [f32; 2]) {
    let (walk, aim, lean) = match pointer {
        None => (left, right, [0.0; 2]),
        Some(Stick::Left) => (right, [0.0; 2], left),
        Some(Stick::Right) => (left, [0.0; 2], right),
    };
    // Eight ways from the stick: past a dead zone, and a direction only when
    // the stick leans that way enough (not every diagonal wobble).
    let leans = |a: f32, b: f32| a.abs() > DEAD && a.abs() > b.abs() * 0.4;
    let [x, y] = walk;
    let dirs = [x < 0.0 && leans(x, y), x > 0.0 && leans(x, y), y < 0.0 && leans(y, x), y > 0.0 && leans(y, x)];
    // The sticks as values, past the same dead zone.
    (dirs, crate::input::stick(x, y, DEAD), crate::input::stick(aim[0], aim[1], DEAD), lean)
}

/// What one seat holds (`docs/game-model.md`, "Several players"): its part
/// of the keyboard (`half`: all of it as the settings bind it, moving on
/// the arrows and WASD; a half by its own keys, its directions too) and
/// its pads by their buttons (`pad`, `Settings::pad_of`); with it the lean
/// of the stick `pointer` gives the pointer (`sticks`).
pub fn read_seat(keys: Option<&dyn KeyState>, half: crate::seating::Keys, pads: &[&Gamepad], pad: &Bindings, settings: &crate::settings::Settings, actions: &[&str], pointer: Option<Stick>) -> (crate::input::Input, [f32; 2]) {
    use crate::seating::Keys;
    // (No keyboard: the pads alone, while the keys are elsewhere.)
    let keys = keys.filter(|_| half != Keys::Nothing);
    let held = |c: Scancode| keys.is_some_and(|k| k.pressed(c) && !(k.typing() && types(c)));
    let own = settings.keys_of(half);
    let named = |names: &[String]| {
        names.iter().any(|n| match Scancode::from_name(n) {
            Some(c) => held(c),
            None => keys.is_some_and(|k| k.mouse(n)),
        })
    };
    let any = |codes: &[Scancode], way: &str| match half {
        // A half: its own keys for the way.
        Keys::Left | Keys::Right => own.is_some_and(|b| named(b.get(way))),
        _ => codes.iter().any(|&c| held(c)),
    };
    let bound = |action: &str| own.is_some_and(|b| named(b.get(action))) || pads.iter().any(|p| pad.get(action).iter().any(|n| pad_held(p, n)));
    let stick = |axis: Axis| pads.iter().map(|p| p.axis(axis) as f32 / 32767.0).fold(0.0f32, |a, v| if v.abs() > a.abs() { v } else { a });
    let dpad = |button: Button| pads.iter().any(|p| p.button(button));
    let ([sl, sr, su, sd], moves, aim, lean) = sticks([stick(Axis::LeftX), stick(Axis::LeftY)], [stick(Axis::RightX), stick(Axis::RightY)], pointer);
    let mut input = crate::input::Input {
        left: any(&[Scancode::Left, Scancode::A], "left") || dpad(Button::DPadLeft) || sl,
        right: any(&[Scancode::Right, Scancode::D], "right") || dpad(Button::DPadRight) || sr,
        up: any(&[Scancode::Up, Scancode::W], "up") || dpad(Button::DPadUp) || su,
        down: any(&[Scancode::Down, Scancode::S], "down") || dpad(Button::DPadDown) || sd,
        moves,
        aim,
        ..Default::default()
    };
    for (n, action) in actions.iter().enumerate() {
        input.hold(n, bound(action));
    }
    (input, lean)
}

/// The keys' names as this keyboard prints them (a German keyboard's Y
/// for the bindings' Z), for the menus and hints.
pub fn label_keys(settings: &mut crate::settings::Settings) {
    settings.key_names = settings
        .keys
        .names()
        .chain(settings.left_keys.names())
        .chain(settings.right_keys.names())
        .filter_map(|name| {
            let key = Keycode::from_scancode(Scancode::from_name(name)?, sdl3::sys::keycode::SDL_KMOD_NONE, false)?;
            let label = key.name();
            (!label.is_empty() && !label.eq_ignore_ascii_case(name)).then(|| (name.to_string(), label))
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Binding a key to an action takes it off the action that had it.
    #[test]
    fn a_key_does_one_thing() {
        let mut keys = Bindings::new(&[("fire", &["X", "J"]), ("jump", &["Space", "Z"])]);
        assert_eq!(keys.action_of("Space"), Some("jump"));
        keys.bind("fire", "Space");
        assert_eq!(keys.action_of("space"), Some("fire"));
        assert_eq!(keys.get("fire")[0], "Space");
        assert_eq!(keys.get("jump"), ["Z"]);
        keys.fill(&Bindings::pause_keys());
        keys.fill(&Bindings::new(&[("jump", &["Up"])]));
        assert_eq!(keys.get(PAUSE), ["Escape", "P"]);
        assert_eq!(keys.get("jump"), ["Z"], "a default replaced a binding");
    }

    /// A key a game's action has pauses no more: bound by the game, or
    /// saved for it before (P for the previous level and for pause).
    #[test]
    fn pausing_gives_way_to_the_game() {
        let mut keys = Bindings::new(&[("previous", &["P"])]);
        keys.fill(&Bindings::pause_keys());
        keys.pause_gives_way();
        assert_eq!(keys.get(PAUSE), ["Escape"]);
        assert_eq!(keys.action_of("P"), Some("previous"));
        let mut saved = Bindings::new(&[("previous", &["p"]), (PAUSE, &["Escape", "P"])]);
        saved.pause_gives_way();
        assert_eq!(saved.get(PAUSE), ["Escape"]);
    }

    /// The stick a game gives the pointer (`c.pointer_stick`) holds no
    /// directions and no `input.move`: the other walks, and the aim is
    /// left to the pointer; without one, the left walks and the right aims.
    #[test]
    fn a_stick_given_to_the_pointer_holds_no_directions() {
        let (left, right) = ([-1.0, 0.0], [0.0, -1.0]);
        let (dirs, moves, aim, lean) = sticks(left, right, None);
        assert_eq!((dirs, moves, aim, lean), ([true, false, false, false], [-127, 0], [0, 127], [0.0, 0.0]));
        let (dirs, moves, aim, lean) = sticks(left, right, Some(Stick::Left));
        assert_eq!((dirs, moves, aim, lean), ([false, false, true, false], [0, 127], [0, 0], left), "the right stick walks");
        let (dirs, moves, aim, lean) = sticks(left, right, Some(Stick::Right));
        assert_eq!((dirs, moves, aim, lean), ([true, false, false, false], [-127, 0], [0, 0], right), "the left stick walks");
        let (dirs, moves, ..) = sticks([0.2, 0.1], [0.0; 2], Some(Stick::Right));
        assert_eq!((dirs, moves), ([false; 4], [0, 0]), "a wobble in the dead zone is none");
    }

    /// Every default gamepad button has a short label on every kind of
    /// pad, and a Switch-style pad's bottom button reads B.
    #[test]
    fn pad_buttons_have_labels() {
        let mut pad = crate::shooter::pad();
        pad.fill(&Bindings::pause_pad());
        for style in [PadStyle::Xbox, PadStyle::Nintendo, PadStyle::PlayStation] {
            for name in pad.names() {
                assert!(pad_label(name, style).len() <= 8, "{name} shows as {}", pad_label(name, style));
            }
        }
        assert_eq!(pad_label("south", PadStyle::Nintendo), "B");
        assert_eq!(pad_label("south", PadStyle::Xbox), "A");
    }
}
