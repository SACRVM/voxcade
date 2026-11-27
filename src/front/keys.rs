//! What keys and buttons mean to the menus and to a game being played
//! before the game hears them: steering the menus, pausing (which gives
//! way to the game's actions), leaving a run that ended. The window's and
//! the IDE's PLAY alike.

use sdl3::gamepad::Button;
use sdl3::keyboard::Scancode;

use crate::controls::{Nav, PAUSE, PadStyle};
use crate::settings::Settings;

/// Menu steering from a key: the arrows and WASD, Enter or Space to choose,
/// Escape or Backspace to go back, the pause keys.
pub fn key_nav(code: Scancode, settings: &Settings) -> Option<Nav> {
    Some(match code {
        Scancode::Up | Scancode::W => Nav::Up,
        Scancode::Down | Scancode::S => Nav::Down,
        Scancode::Left | Scancode::A => Nav::Left,
        Scancode::Right | Scancode::D => Nav::Right,
        Scancode::Return | Scancode::KpEnter | Scancode::Space => Nav::Confirm,
        Scancode::Escape | Scancode::Backspace => Nav::Back,
        _ if settings.keys.action_of(code.name()) == Some(PAUSE) => Nav::Pause,
        _ => return None,
    })
}

pub fn is_direction(nav: Nav) -> bool {
    matches!(nav, Nav::Up | Nav::Down | Nav::Left | Nav::Right)
}

/// Which letters a kind of gamepad prints on its buttons.
pub fn pad_style(kind: sdl3::gamepad::GamepadType) -> PadStyle {
    use sdl3::gamepad::GamepadType::*;
    match kind {
        NintendoSwitchPro | NintendoSwitchJoyconLeft | NintendoSwitchJoyconRight | NintendoSwitchJoyconPair => PadStyle::Nintendo,
        PS3 | PS4 | PS5 => PadStyle::PlayStation,
        _ => PadStyle::Xbox,
    }
}

/// Menu steering from a gamepad button: d-pad, the bottom button to
/// choose, the right one or Back to go back, Start.
pub fn pad_nav(button: Button) -> Option<Nav> {
    Some(match button {
        Button::DPadUp => Nav::Up,
        Button::DPadDown => Nav::Down,
        Button::DPadLeft => Nav::Left,
        Button::DPadRight => Nav::Right,
        Button::South => Nav::Confirm,
        Button::East | Button::Back => Nav::Back,
        Button::Start => Nav::Pause,
        _ => return None,
    })
}

/// What a key or button does to a game being played (no menu open).
#[derive(Debug, PartialEq, Eq)]
pub enum PlayKey {
    /// The run is over: back to the title (its pause key, or Enter or the
    /// bottom button).
    ToTitle,
    /// The pause page opens (a level change's fade is seen out first).
    Pause,
    /// The game's own: it hears it with the keys held.
    Game,
}

/// What the key `code` does to a game being played: the pause keys are
/// those the settings give PAUSE, and they give way to the game's actions
/// (`Bindings::pause_gives_way`).
pub fn play_key(code: Scancode, settings: &Settings, phase_over: bool, fading: bool) -> PlayKey {
    let pause = settings.keys.action_of(code.name()) == Some(PAUSE);
    if phase_over && (pause || matches!(code, Scancode::Return | Scancode::KpEnter)) {
        PlayKey::ToTitle
    } else if pause && !fading {
        PlayKey::Pause
    } else {
        PlayKey::Game
    }
}

/// What the gamepad button `button` (its binding's `name`) does to a game
/// being played, as `play_key`.
pub fn play_button(button: Button, name: &str, settings: &Settings, phase_over: bool, fading: bool) -> PlayKey {
    let pause = settings.pad.action_of(name) == Some(PAUSE);
    if phase_over && (pause || button == Button::South) {
        PlayKey::ToTitle
    } else if pause && !fading {
        PlayKey::Pause
    } else {
        PlayKey::Game
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::Bindings;

    /// The settings of a game whose action `previous` has P, and whose
    /// pad's `previous` has the north button.
    fn settings_of_a_game_with_p() -> Settings {
        let mut settings = Settings::default();
        settings.adopt(&["previous"], &Bindings::new(&[("previous", &["P"])]), &Bindings::new(&[("previous", &["north"])]), false);
        settings
    }

    /// Escape pauses a game being played, P does too unless one of the
    /// game's actions has it (Sokovox's previous level): pausing gives way
    /// to the game. A fade is seen out before the pause page; once the run
    /// is over, the pause key and Enter go back to the title.
    #[test]
    fn pausing_gives_way_to_the_games_keys() {
        let plain = {
            let mut settings = Settings::default();
            settings.adopt(&["fire"], &Bindings::new(&[("fire", &["Z"])]), &Bindings::new(&[("fire", &["south"])]), false);
            settings
        };
        assert_eq!(play_key(Scancode::P, &plain, false, false), PlayKey::Pause, "P pauses a game that leaves it");
        let game = settings_of_a_game_with_p();
        assert_eq!(play_key(Scancode::Escape, &game, false, false), PlayKey::Pause);
        assert_eq!(play_key(Scancode::P, &game, false, false), PlayKey::Game, "the game's P does not pause");
        assert_eq!(play_key(Scancode::Escape, &game, false, true), PlayKey::Game, "a fade is seen out first");
        assert_eq!(play_key(Scancode::Escape, &game, true, false), PlayKey::ToTitle);
        assert_eq!(play_key(Scancode::Return, &game, true, false), PlayKey::ToTitle);
        assert_eq!(play_key(Scancode::Return, &game, false, false), PlayKey::Game);
        assert_eq!(play_button(Button::Start, "start", &game, false, false), PlayKey::Pause);
        assert_eq!(play_button(Button::North, "north", &game, false, false), PlayKey::Game);
        assert_eq!(play_button(Button::South, "south", &game, true, false), PlayKey::ToTitle);
        assert_eq!(play_button(Button::South, "south", &game, false, false), PlayKey::Game);
    }

    /// The menus are steered by the arrows and WASD, Enter and Escape, and
    /// a pad's d-pad, bottom and right buttons.
    #[test]
    fn the_menus_are_steered_by_keys_and_buttons() {
        let settings = settings_of_a_game_with_p();
        assert_eq!(key_nav(Scancode::W, &settings), Some(Nav::Up));
        assert_eq!(key_nav(Scancode::Return, &settings), Some(Nav::Confirm));
        assert_eq!(key_nav(Scancode::Escape, &settings), Some(Nav::Back));
        assert_eq!(key_nav(Scancode::P, &settings), None, "the game's P is no pause");
        assert_eq!(pad_nav(Button::East), Some(Nav::Back));
        assert_eq!(pad_nav(Button::Start), Some(Nav::Pause));
        assert!(is_direction(Nav::Left) && !is_direction(Nav::Confirm));
    }
}
