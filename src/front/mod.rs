//! The front end of a game played as a state of its own: the title, the
//! glide from it into the game, the game, a replay the title plays
//! (attract mode), the high score table in the title's round, and the
//! engine's menu over them, steered by keys, pads and the pointer, and
//! the pages it builds. The window's (`crate::app`) and the IDE's PLAY's
//! (`crate::ide`, always playing, its menu over the game alone). Its moves
//! need neither a window nor a GPU, so they are tested here.

pub mod keys;

use anyhow::Result;
use sdl3::gamepad::{Axis, Button};
use sdl3::keyboard::Scancode;

use crate::controls::{Device, Nav, PadStyle};
use crate::game::{Game, Request};
use crate::menu::{Command, Menu, Page};
use crate::seating::{Keys, Seating};
use crate::session::{INTRO_TIME, Scene, Session};
use crate::settings::Settings;

/// Ticks the high score table shows before the title (or the replay of the
/// title's round) comes.
pub const HIGH_SCORES_TICKS: u64 = 8 * 60;

/// What the title does after a while without a key (`Front::idle`).
#[derive(Debug, PartialEq, Eq)]
pub enum Idle {
    /// Nothing yet.
    Wait,
    /// The high score table shows (the first of the title's round).
    Table,
    /// One of the game's replays plays, if it has one.
    Replay,
}

/// What a window's music is (`Front::wanted`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wanted {
    /// The engine's title: its song (`c.title_screen.music`, else the
    /// first a player hears) from `c.title_screen.music_at`'s bar, its
    /// round's table and replays under it.
    Title,
    /// The game's, as its code asked last (`Session::new_music`): its
    /// song, its silence, or, where it asked nothing yet, what plays going
    /// on.
    Game,
}

pub struct Front {
    /// The menu open over the title or the game (`Menu::default()`: none).
    pub menu: Menu,
    /// What the player pressed last, kept whatever menu opens: the menus'
    /// hints and the game's name its keys or buttons.
    pub played_with: Device,
    pub pad_kind: PadStyle,
    /// The left stick steering a menu: the way it points, once past the
    /// middle, and back to the middle before it counts again.
    pub stick: Option<Nav>,
    /// The lines of a run of several still to enter, whose and how many.
    pub entering: Vec<(Option<usize>, u32)>,
    /// The game plays (else the title shows).
    pub playing: bool,
    /// The title's scene's clock.
    pub title_time: f32,
    pub title_tick: u64,
    /// The title's music plays, not the game's.
    pub title_music: bool,
    /// What the window last had the mixer play (`App::music`): handed
    /// again only when what is wanted changes.
    pub heard: Option<Wanted>,
    /// Seconds into the glide from the title into the game, while it runs.
    pub intro: Option<f32>,
    /// The title's attract mode: a replay playing under the title's music,
    /// how long it played, and whether a key asked it to end.
    pub attracting: bool,
    pub attract_time: f32,
    pub leave_attract: bool,
    /// The window has lost the focus: a game of one holds (no ticks, its
    /// song held as a pause holds it) and goes on by itself when the
    /// window has it again, no menu opened (the owner, 2026-11-24: landing
    /// in the pause menu on every switch of windows annoyed). A network
    /// game goes on: nobody waits.
    pub away: bool,
    /// The title's round as at an arcade machine: the title, the high
    /// score table, a replay. Whether the table is showing in it, and
    /// whether it was shown since the last replay.
    pub table_in_round: bool,
    pub table_shown: bool,
    /// The host's address (or a game's code) is typed as text.
    pub typing_address: bool,
}

impl Front {
    /// On the title, `menu` open.
    pub fn new(menu: Menu) -> Self {
        Front {
            menu,
            played_with: Device::Keyboard,
            pad_kind: PadStyle::default(),
            stick: None,
            entering: Vec::new(),
            playing: false,
            title_time: 0.0,
            title_tick: 0,
            title_music: true,
            heard: None,
            intro: None,
            attracting: false,
            attract_time: 0.0,
            leave_attract: false,
            away: false,
            table_in_round: false,
            table_shown: false,
            typing_address: false,
        }
    }

    /// Into the game at once (a replay watched, a scenario, a run loaded or
    /// begun again): no menu, the title's music no more.
    pub fn straight_in(&mut self) {
        self.playing = true;
        self.menu = Menu::default();
        self.title_music = false;
    }

    /// From the title into the game by the glide (START, HOST).
    pub fn glide_in(&mut self) {
        self.intro = Some(0.0);
        self.menu = Menu::default();
        self.title_music = false;
    }

    /// The glide goes on by `elapsed` seconds; at its end the game plays.
    pub fn glide(&mut self, elapsed: f32) {
        if let Some(t) = &mut self.intro {
            *t += elapsed;
            if *t >= INTRO_TIME {
                self.intro = None;
                self.playing = true;
            }
        }
    }

    /// Back on the title (a run's end, QUIT TO TITLE), `menu` open: the
    /// title's clock and music from the start.
    pub fn back_to_title(&mut self, menu: Menu) {
        self.playing = false;
        self.menu = menu;
        self.table_in_round = false;
        self.title_time = 0.0;
        self.title_music = true;
    }

    /// What the window's music is to be now, from the front end alone:
    /// the one decision (`App::music` hands it, and only when it changes).
    pub fn wanted(&self) -> Wanted {
        if self.title_music { Wanted::Title } else { Wanted::Game }
    }

    /// What the game asked of the window in a tick (`Session::requests`):
    /// SETTINGS opened over it, the game held while it shows; the pause
    /// page (a command: the menu needs the game's slots); whether to quit.
    pub fn asked(&mut self, requests: &[Request]) -> Option<Command> {
        let mut then = None;
        for request in requests {
            match request {
                Request::Open("settings") => self.menu = Menu::settings(),
                Request::Open("pause") => then = then.or(Some(Command::Pause)),
                Request::Open(_) => {}
                Request::Quit => then = Some(Command::Quit),
                // Quitting wins over going to the title.
                Request::ToTitle => then = then.or(Some(Command::ToTitle)),
            }
        }
        then
    }

    /// A key or button while the title plays a replay: it asks the replay to
    /// end and is taken (true).
    pub fn leaves_attract(&mut self) -> bool {
        if self.attracting {
            self.leave_attract = true;
        }
        self.attracting
    }

    /// The window lost the focus: a game of one holds, whatever menu is
    /// open staying so; a network game (`networked`) goes on.
    pub fn look_away(&mut self, networked: bool) {
        self.away = !networked;
    }

    /// The window has the focus again: what held goes on.
    pub fn look_back(&mut self) {
        self.away = false;
    }

    /// Whether the game holds now: a menu over it or the window elsewhere
    /// (no ticks; the game's song held).
    pub fn holds(&self) -> bool {
        self.menu.is_open() || self.away
    }

    /// The title alone shows: no game, no glide, no page but the title's.
    pub fn on_title(&self) -> bool {
        !self.playing && self.intro.is_none() && self.menu.stack.len() == 1 && self.menu.page() == Some(Page::Title)
    }

    /// The title waited long enough without a key (`waited`): first the
    /// high score table (a game that scores, not shown since the last
    /// replay), then a replay.
    pub fn idle(&mut self, waited: bool, scored: bool) -> Idle {
        if waited && scored && !self.table_shown {
            self.menu = Menu::high_scores(None);
            (self.table_in_round, self.table_shown) = (true, true);
            Idle::Table
        } else if waited {
            self.table_shown = false;
            Idle::Replay
        } else {
            Idle::Wait
        }
    }

    /// The table goes after a while: on to the replay in the title's round,
    /// else to the title (`title` makes its menu). True when the round
    /// begins anew (the title waits from now).
    pub fn table_over(&mut self, title: impl FnOnce() -> Menu) -> bool {
        if self.menu.page() == Some(Page::HighScores) && self.menu.tick - self.menu.opened >= HIGH_SCORES_TICKS {
            self.menu = title();
            self.menu.tick = HIGH_SCORES_TICKS;
            if !std::mem::take(&mut self.table_in_round) {
                self.table_shown = false;
                return true;
            }
        }
        false
    }

    /// What the picture shows behind the menu.
    pub fn scene(&self) -> Scene {
        match self.intro {
            Some(t) => Scene::Intro { time: self.title_time, t: t / INTRO_TIME },
            None if self.playing => Scene::Play,
            None => Scene::Title { time: self.title_time },
        }
    }

    /// A key while CONTROLS waits for one: bound (Escape keeps what was
    /// bound), what the menu then asks in `commands`. Whether one was
    /// waited for: the key is then taken, whatever it is.
    pub fn rebind_key(&mut self, code: Scancode, repeat: bool, settings: &mut Settings, commands: &mut Vec<Command>) -> bool {
        if self.menu.rebinding.is_none() {
            return false;
        }
        if !repeat {
            commands.extend(self.menu.bind(Device::Keyboard, code.name(), code == Scancode::Escape, settings));
        }
        true
    }

    /// A key steering the menu that is open (`keys::key_nav`; held down,
    /// only a direction steps on): the keyboard used.
    pub fn menu_key(&mut self, code: Scancode, repeat: bool, settings: &mut Settings, commands: &mut Vec<Command>) {
        self.played_with = Device::Keyboard;
        if let Some(nav) = keys::key_nav(code, settings).filter(|n| !repeat || keys::is_direction(*n)) {
            commands.extend(self.menu.nav(nav, settings));
        }
    }

    /// A pad's button (its binding's `name`) for the menus: bound while
    /// CONTROLS waits for one (Back keeps what was bound), else steering
    /// the menu that is open, the pad used. Whether it was the menus'.
    pub fn menu_button(&mut self, button: Button, name: &str, settings: &mut Settings, commands: &mut Vec<Command>) -> bool {
        if self.menu.rebinding.is_some() {
            commands.extend(self.menu.bind(Device::Gamepad, name, button == Button::Back, settings));
            return true;
        }
        if !self.menu.is_open() {
            return false;
        }
        self.played_with = Device::Gamepad;
        if let Some(nav) = keys::pad_nav(button) {
            commands.extend(self.menu.nav(nav, settings));
        }
        true
    }

    /// A pad's stick or trigger leaning to `value` for the menus: a
    /// trigger bound while CONTROLS waits for a button, the left stick
    /// steering the menu that is open (once past the middle, back to it
    /// before it counts again). Whether it was the menus'.
    pub fn menu_axis(&mut self, axis: Axis, value: i16, settings: &mut Settings, commands: &mut Vec<Command>) -> bool {
        let v = value as f32 / 32767.0;
        if self.menu.rebinding.is_some() {
            let trigger = match axis {
                Axis::TriggerLeft => Some("lefttrigger"),
                Axis::TriggerRight => Some("righttrigger"),
                _ => None,
            };
            if let Some(name) = trigger.filter(|_| v > 0.6) {
                commands.extend(self.menu.bind(Device::Gamepad, name, false, settings));
            }
            return true;
        }
        if !self.menu.is_open() {
            return false;
        }
        if !matches!(axis, Axis::LeftX | Axis::LeftY) {
            return true;
        }
        let way = match (axis, v) {
            (Axis::LeftX, v) if v < -0.6 => Some(Nav::Left),
            (Axis::LeftX, v) if v > 0.6 => Some(Nav::Right),
            (Axis::LeftY, v) if v < -0.6 => Some(Nav::Up),
            (Axis::LeftY, v) if v > 0.6 => Some(Nav::Down),
            _ => None,
        };
        let along = |n: Nav| matches!((axis, n), (Axis::LeftX, Nav::Left | Nav::Right) | (Axis::LeftY, Nav::Up | Nav::Down));
        match way {
            Some(nav) if self.stick != Some(nav) => {
                self.stick = Some(nav);
                self.played_with = Device::Gamepad;
                commands.extend(self.menu.nav(nav, settings));
            }
            None if v.abs() < 0.3 && self.stick.is_some_and(along) => self.stick = None,
            _ => {}
        }
        true
    }

    /// A mouse button down for the menus, `row` the row of the page under
    /// the pointer (`row_at`): bound while CONTROLS waits for a key (a game
    /// of one's, the keyboard's bindings), else over a page the left one
    /// confirming the row under it, the right going back. Whether it was
    /// the menus'.
    pub fn menu_click(&mut self, button: sdl3::mouse::MouseButton, row: Option<usize>, settings: &mut Settings, commands: &mut Vec<Command>) -> bool {
        if self.menu.rebinding.is_some() {
            if self.menu.page() == Some(Page::Controls)
                && let Some(i) = crate::mouse::Mouse::index(button)
            {
                commands.extend(self.menu.bind(Device::Keyboard, crate::input::MOUSE_KEYS[i], false, settings));
            }
            return true;
        }
        if !self.menu.is_open() {
            return false;
        }
        match button {
            sdl3::mouse::MouseButton::Left => {
                if let Some(row) = row {
                    self.menu.choose(row);
                    commands.extend(self.menu.nav(Nav::Confirm, settings));
                }
            }
            sdl3::mouse::MouseButton::Right => commands.extend(self.menu.nav(Nav::Back, settings)),
            _ => {}
        }
        true
    }

    /// The wheel turned `y` notches (above 0: up) while CONTROLS waits for
    /// a key: bound as one. Whether it was.
    pub fn wheel_bind(&mut self, y: f32, settings: &mut Settings, commands: &mut Vec<Command>) -> bool {
        if self.menu.rebinding.is_none() || self.menu.page() != Some(Page::Controls) || y == 0.0 {
            return false;
        }
        let name = if y > 0.0 { "wheel_up" } else { "wheel_down" };
        commands.extend(self.menu.bind(Device::Keyboard, name, false, settings));
        true
    }

    /// The row of the open page under the pointer at `at` (the HUD's
    /// measure; None off the picture), among the rows as drawn (`rows`,
    /// `Session::menu_rows`); none while CONTROLS waits for a key.
    pub fn row_at(&self, at: Option<[f32; 2]>, rows: impl FnOnce() -> Vec<crate::ui::RowBox>) -> Option<usize> {
        if !self.menu.is_open() || self.menu.rebinding.is_some() {
            return None;
        }
        let [x, y] = at?;
        rows().into_iter().find(|&(_, rx, ry, w, h)| x >= rx && y >= ry && x < rx + w && y < ry + h).map(|(i, ..)| i)
    }
}

/// The game's own title (`c.title_screen.level`) begun in place of the
/// engine's, if it has one: straight in, no menu. Whether it has one.
pub fn own_title<G: Game>(game: &mut Session<G>, front: &mut Front) -> Result<bool> {
    if game.cfg.title_screen.level.is_none() {
        return Ok(false);
    }
    front.straight_in();
    game.begin_title()?;
    Ok(true)
}

/// The pause page; SAVE and LOAD where the game has save slots and someone
/// plays (not while a replay is watched).
pub fn pause_menu<G: Game>(game: &Session<G>, playing: bool) -> Menu {
    if !playing {
        return Menu::pause();
    }
    let now = crate::saves::now();
    Menu::pause().with_slots((1..=game.slots.count()).map(|n| game.slots.info(n).map(|i| crate::menu::slot_line(n, Some(&i), now, &game.language()))).collect())
}

/// The game paused (`pause_menu`), LEAVE offered to the player on
/// `leaving`.
pub fn pause_for<G: Game>(front: &mut Front, game: &Session<G>, playing: bool, leaving: Option<usize>) {
    front.menu = pause_menu(game, playing);
    front.menu.leaving = leaving;
}

/// The seats as the plan in the settings has them, with the pads plugged
/// in (the players page before a run, and where its runs begin).
pub fn planned(settings: &Settings, named: &[(u32, String)], seats: u32) -> Seating {
    let mut s = Seating::new(seats);
    s.begin(None, &crate::seating::resolve(&settings.players, named));
    s
}

/// The seat after the first the plan gives the keyboard (its start key
/// takes it).
pub fn planned_keyboard(settings: &Settings) -> Option<usize> {
    settings.players.iter().enumerate().skip(1).find(|(_, p)| p.keyboard != Keys::Nothing).map(|(i, _)| i + 1)
}

/// The seat the plan gives a pad (its START takes it).
pub fn planned_pad(settings: &Settings, named: &[(u32, String)], pad: u32) -> Option<usize> {
    let name = named.iter().find(|(id, _)| *id == pad).map(|(_, n)| n)?;
    settings.players.iter().position(|p| p.pad.as_ref() == Some(name)).map(|i| i + 1)
}

/// The players page's `command` (a seat's part of the keyboard or its pad
/// a step on, SWAP) done on `seats`, the pads plugged in `named`, and the
/// plan in the settings made of them (the settings keep it): the seats as
/// they are now.
pub fn reseat(command: Command, mut seats: Seating, settings: &mut Settings, named: &[(u32, String)]) -> Seating {
    let mut chosen = None;
    match command {
        Command::SeatKeys(seat, step) => seats.set_keyboard(seat, seats.keys(seat).step(step)),
        Command::SeatPad(seat, step) => {
            let choices: Vec<Option<u32>> = std::iter::once(None).chain(named.iter().map(|(id, _)| Some(*id))).collect();
            seats.set_pad(seat, crate::settings::cycle(&choices, seats.pad(seat), step));
            chosen = Some(seat);
        }
        _ => seats.swap(1, 2),
    }
    settings.players = crate::seating::plan_of(&seats, named, &settings.players, chosen);
    seats
}

/// The player's settings that the game itself follows, and the game's
/// languages they choose from.
pub fn apply_settings<G: Game>(game: &mut Session<G>, settings: &mut Settings) {
    settings.offer_languages(&game.cfg.languages);
    game.options = settings.options();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Losing the focus opens no menu: a game of one holds, and with the
    /// focus back it goes on; a menu open stays open; a network game goes
    /// on throughout.
    #[test]
    fn losing_the_focus_holds_and_opens_no_menu() {
        let mut front = Front::new(Menu::title());
        front.straight_in();
        front.look_away(false);
        assert!(front.holds() && !front.menu.is_open() && front.playing);
        front.look_back();
        assert!(!front.holds());
        front.menu = Menu::pause();
        front.look_away(false);
        front.look_back();
        assert!(front.menu.is_open() && front.holds(), "the pause menu stays");
        front.menu = Menu::default();
        front.look_away(true);
        assert!(!front.holds(), "a network game goes on");
    }

    /// The table's time runs out.
    fn table_shown_long(front: &mut Front) {
        front.menu.tick = front.menu.opened + HIGH_SCORES_TICKS;
    }

    /// Long enough on the title without a key: a game that scores shows
    /// its high score table first, then a replay plays; the round begins
    /// again after the replay. A game without scores plays a replay at
    /// once. Nothing of it while the game plays, glides in, or another page
    /// is open.
    #[test]
    fn the_title_plays_its_round_after_a_while() {
        let mut front = Front::new(Menu::title());
        assert!(front.on_title());
        assert_eq!(front.idle(false, true), Idle::Wait);
        assert_eq!(front.idle(true, true), Idle::Table);
        assert_eq!(front.menu.page(), Some(Page::HighScores));
        assert!(!front.on_title(), "the table is no title");
        assert!(!front.table_over(Menu::title), "not yet");
        table_shown_long(&mut front);
        assert!(!front.table_over(Menu::title), "in the round: on to the replay");
        assert!(front.on_title());
        assert_eq!(front.idle(true, true), Idle::Replay, "the table was shown: a replay");
        assert_eq!(front.idle(true, true), Idle::Table, "after the replay the round begins again");

        // The table opened from the title (not by waiting) goes back to it
        // and the title waits anew.
        let mut front = Front::new(Menu::high_scores(None));
        table_shown_long(&mut front);
        assert!(front.table_over(Menu::title));

        let mut front = Front::new(Menu::title());
        assert_eq!(front.idle(true, false), Idle::Replay, "no scores: a replay at once");

        for change in [
            |f: &mut Front| f.playing = true,
            |f: &mut Front| f.intro = Some(0.0),
            |f: &mut Front| f.menu.stack.push((Page::Settings, 0)),
        ] {
            let mut front = Front::new(Menu::title());
            change(&mut front);
            assert!(!front.on_title());
        }
    }

    /// A key while the title plays a replay ends the replay and does
    /// nothing else; otherwise it is not taken.
    #[test]
    fn a_key_ends_the_titles_replay() {
        let mut front = Front::new(Menu::title());
        assert!(!front.leaves_attract() && !front.leave_attract);
        front.attracting = true;
        assert!(front.leaves_attract() && front.leave_attract);
    }

    /// START glides into the game, which plays at the glide's end; back on
    /// the title its clock and music begin again.
    #[test]
    fn start_glides_into_the_game_and_back_to_the_title() {
        let mut front = Front::new(Menu::title());
        front.title_time = 12.0;
        front.glide_in();
        assert!(!front.playing && !front.title_music && !front.menu.is_open());
        assert!(matches!(front.scene(), Scene::Intro { .. }));
        front.glide(INTRO_TIME / 2.0);
        assert!(!front.playing);
        front.glide(INTRO_TIME);
        assert!(front.playing && front.intro.is_none());
        assert!(matches!(front.scene(), Scene::Play));
        front.back_to_title(Menu::title());
        assert!(!front.playing && front.title_music && front.title_time == 0.0 && front.on_title());
        assert!(matches!(front.scene(), Scene::Title { .. }));
    }
}
