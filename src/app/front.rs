//! The window's front end as a state of its own: the title, the glide from
//! it into the game, the game, a replay the title plays (attract mode), the
//! high score table in the title's round, and the menu over them. Its moves
//! need neither SDL nor a GPU, so they are tested here.

use crate::controls::{Device, Nav, PadStyle};
use crate::menu::{Menu, Page};
use crate::session::{INTRO_TIME, Scene};

/// Ticks the high score table shows before the title (or the replay of the
/// title's round) comes.
pub(super) const HIGH_SCORES_TICKS: u64 = 8 * 60;

/// What the title does after a while without a key (`Front::idle`).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Idle {
    /// Nothing yet.
    Wait,
    /// The high score table shows (the first of the title's round).
    Table,
    /// One of the game's replays plays, if it has one.
    Replay,
}

pub(super) struct Front {
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
    /// The song the title's settings play instead of the title's, if any.
    pub previewing: Option<usize>,
    /// Seconds into the glide from the title into the game, while it runs.
    pub intro: Option<f32>,
    /// The title's attract mode: a replay playing under the title's music,
    /// how long it played, and whether a key asked it to end.
    pub attracting: bool,
    pub attract_time: f32,
    pub leave_attract: bool,
    /// Looking away during a level change's fade pauses once it is over.
    pub pause_after_fade: bool,
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
            previewing: None,
            intro: None,
            attracting: false,
            attract_time: 0.0,
            leave_attract: false,
            pause_after_fade: false,
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
        self.previewing = None;
    }

    /// A key or button while the title plays a replay: it asks the replay to
    /// end and is taken (true).
    pub fn leaves_attract(&mut self) -> bool {
        if self.attracting {
            self.leave_attract = true;
        }
        self.attracting
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
