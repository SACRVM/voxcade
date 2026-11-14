//! What the player presses: the keyboard keys and gamepad buttons for each
//! action (changeable in the settings), and how the menus are steered.
//! Plain names, so the bindings can be saved and shown; reading them from
//! SDL (`read_input`) serves the game's window and the IDE's alike.

use std::collections::BTreeMap;

use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::keyboard::{KeyboardState, Keycode, Scancode};
use serde::{Deserialize, Serialize};

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
        Self { device: Some(device), keys, language: Some(s.language()) }
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

/// How a key is shown: its SDL name in capitals.
pub fn key_label(name: &str) -> String {
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

/// A gamepad button's name in the bindings (positional, like SDL's).
pub fn button_name(button: Button) -> Option<&'static str> {
    Some(match button {
        Button::South => "south",
        Button::East => "east",
        Button::West => "west",
        Button::North => "north",
        Button::LeftShoulder => "leftshoulder",
        Button::RightShoulder => "rightshoulder",
        Button::LeftStick => "leftstick",
        Button::RightStick => "rightstick",
        Button::Start => "start",
        Button::Back => "back",
        Button::DPadUp => "dpup",
        Button::DPadDown => "dpdown",
        Button::DPadLeft => "dpleft",
        Button::DPadRight => "dpright",
        _ => return None,
    })
}

/// Whether the button (or trigger) a binding names is held on `pad`.
pub fn pad_held(pad: &Gamepad, name: &str) -> bool {
    match name {
        "lefttrigger" => pad.axis(Axis::TriggerLeft) > 12_000,
        "righttrigger" => pad.axis(Axis::TriggerRight) > 12_000,
        _ => [
            Button::South,
            Button::East,
            Button::West,
            Button::North,
            Button::LeftShoulder,
            Button::RightShoulder,
            Button::LeftStick,
            Button::RightStick,
            Button::Start,
            Button::Back,
            Button::DPadUp,
            Button::DPadDown,
            Button::DPadLeft,
            Button::DPadRight,
        ]
        .into_iter()
        .find(|&b| button_name(b) == Some(name))
        .is_some_and(|b| pad.button(b)),
    }
}

/// Which keys are down: the keyboard, or the keyboard with keys held
/// besides by another session driving the IDE (`ide::remote`).
pub trait KeyState {
    fn pressed(&self, key: Scancode) -> bool;
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

/// What the player holds: moving on the arrows and WASD, the d-pad and the
/// left stick; the rest as the settings bind it.
pub fn read_input(keys: Option<&dyn KeyState>, pads: &[Gamepad], settings: &crate::settings::Settings, actions: &[&str]) -> crate::input::Input {
    read_seat(keys, crate::seating::Keys::Whole, &pads.iter().collect::<Vec<_>>(), &settings.pad, settings, actions)
}

/// What one seat holds (`docs/game-model.md`, "Several players"): its part
/// of the keyboard (`half`: all of it as the settings bind it, moving on
/// the arrows and WASD; a half by its own keys, its directions too) and
/// its pads by their buttons (`pad`, `Settings::pad_of`).
pub fn read_seat(keys: Option<&dyn KeyState>, half: crate::seating::Keys, pads: &[&Gamepad], pad: &Bindings, settings: &crate::settings::Settings, actions: &[&str]) -> crate::input::Input {
    use crate::seating::Keys;
    // (No keyboard: the pads alone, while the keys are elsewhere.)
    let keys = keys.filter(|_| half != Keys::Nothing);
    let held = |c: Scancode| keys.is_some_and(|k| k.pressed(c));
    let own = settings.keys_of(half);
    let named = |names: &[String]| names.iter().filter_map(|n| Scancode::from_name(n)).any(held);
    let any = |codes: &[Scancode], way: &str| match half {
        // A half: its own keys for the way.
        Keys::Left | Keys::Right => own.is_some_and(|b| named(b.get(way))),
        _ => codes.iter().any(|&c| held(c)),
    };
    let bound = |action: &str| own.is_some_and(|b| named(b.get(action))) || pads.iter().any(|p| pad.get(action).iter().any(|n| pad_held(p, n)));
    let stick = |axis: Axis| pads.iter().map(|p| p.axis(axis) as f32 / 32767.0).fold(0.0f32, |a, v| if v.abs() > a.abs() { v } else { a });
    let dpad = |button: Button| pads.iter().any(|p| p.button(button));
    let (sx, sy) = (stick(Axis::LeftX), stick(Axis::LeftY));
    // Eight ways from the stick: past a dead zone, and a direction only when
    // the stick leans that way enough (not every diagonal wobble).
    const DEAD: f32 = 0.3;
    let lean = |a: f32, b: f32| a.abs() > DEAD && a.abs() > b.abs() * 0.4;
    let mut input = crate::input::Input {
        left: any(&[Scancode::Left, Scancode::A], "left") || dpad(Button::DPadLeft) || (sx < 0.0 && lean(sx, sy)),
        right: any(&[Scancode::Right, Scancode::D], "right") || dpad(Button::DPadRight) || (sx > 0.0 && lean(sx, sy)),
        up: any(&[Scancode::Up, Scancode::W], "up") || dpad(Button::DPadUp) || (sy < 0.0 && lean(sy, sx)),
        down: any(&[Scancode::Down, Scancode::S], "down") || dpad(Button::DPadDown) || (sy > 0.0 && lean(sy, sx)),
        actions: 0,
    };
    for (n, action) in actions.iter().enumerate() {
        input.hold(n, bound(action));
    }
    input
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
