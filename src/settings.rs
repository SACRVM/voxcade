//! The player's settings: volumes, which song plays, the screen, effects,
//! controls and the high score table. Kept per game in the user's config folder
//! (`voxcade/<key>.toml`, `kept::Identity`), not in the game, so a game's folder
//! stays what its maker made.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::controls::Bindings;

/// Volume steps: 0 (off) to this.
pub const VOLUME_STEPS: u8 = 10;

/// How fine the sun's shadows are: the size of the shadow map, the biggest
/// pass on small graphics (on the CPU's own Radeon 610M 3.8 ms of an 8.8 ms
/// frame at the finest). Coarser maps show their texels at the shadows'
/// edges.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Shadows {
    #[default]
    High,
    Medium,
    Low,
}

impl Shadows {
    pub const ALL: [Shadows; 3] = [Shadows::High, Shadows::Medium, Shadows::Low];

    /// The shadow map's width and height in texels.
    pub fn map_size(self) -> u32 {
        match self {
            Shadows::High => 4096,
            Shadows::Medium => 2048,
            Shadows::Low => 1024,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Shadows::High => "HIGH",
            Shadows::Medium => "MEDIUM",
            Shadows::Low => "LOW",
        }
    }

    /// `step` levels coarser (negative: finer), round the list.
    pub fn step(self, step: i32) -> Self {
        cycle(&Self::ALL, self, step)
    }
}

/// How often the picture is drawn: what the window presents with
/// (`Screen::set_present`). The simulation ticks on its own clock, so none
/// of these changes how the game plays.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FrameRate {
    /// As fast as the machine draws (the window's first way).
    #[default]
    Unlimited,
    /// Once for each refresh of the screen.
    Vsync,
    /// At most 60 and 30 a second (the machine rests in between).
    Fps60,
    Fps30,
}

impl FrameRate {
    pub const ALL: [FrameRate; 4] = [FrameRate::Unlimited, FrameRate::Vsync, FrameRate::Fps60, FrameRate::Fps30];

    pub fn label(self) -> &'static str {
        match self {
            FrameRate::Unlimited => "UNLIMITED",
            FrameRate::Vsync => "VSYNC",
            FrameRate::Fps60 => "60",
            FrameRate::Fps30 => "30",
        }
    }

    /// The most pictures a second the window draws; none for no limit
    /// (VSYNC waits for the screen, not for the clock).
    pub fn limit(self) -> Option<u32> {
        match self {
            FrameRate::Fps60 => Some(60),
            FrameRate::Fps30 => Some(30),
            _ => None,
        }
    }

    /// Whether the picture waits for the screen's refresh (a capped rate
    /// shows whole pictures too).
    pub fn waits_for_the_screen(self) -> bool {
        self != FrameRate::Unlimited
    }

    /// `step` places on, round the list.
    pub fn step(self, step: i32) -> Self {
        cycle(&Self::ALL, self, step)
    }
}

/// The one `step` places on from `now` in `all`, round the list (from the
/// first where `now` is none of them).
pub fn cycle<T: Copy + PartialEq>(all: &[T], now: T, step: i32) -> T {
    let at = all.iter().position(|&x| x == now).unwrap_or(0) as i32;
    all[(at + step).rem_euclid(all.len() as i32) as usize]
}

/// Lines in the high score table.
pub const HIGH_SCORES: usize = 10;

/// A line of the high score table: who (three letters) and how many points.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HighScore {
    pub initials: String,
    pub score: u32,
}

/// The engine's table to beat, for a game that brings none of its own
/// (`c.high_scores`).
const PRESETS: [(&str, u32); HIGH_SCORES] =
    [("VOX", 10000), ("CAD", 9000), ("ACE", 8000), ("SKY", 7000), ("ZAP", 6000), ("MAX", 5000), ("JOY", 4000), ("KIM", 3000), ("ROY", 2000), ("EVA", 1000)];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Music and sound volume, 0..=VOLUME_STEPS.
    pub music: u8,
    pub sound: u8,
    /// Which of the game's songs plays while playing.
    pub song: usize,
    pub fullscreen: bool,
    /// The view shakes when things blow up.
    pub shake: bool,
    /// The screen flashes white on big explosions.
    pub flashes: bool,
    /// The tilt-shift blur at the top and bottom of the screen.
    pub blur: bool,
    pub shadows: Shadows,
    /// How often the picture is drawn (FRAME RATE).
    pub frame_rate: FrameRate,
    /// Gamepads rumble on hits and explosions.
    pub rumble: bool,
    /// The frames a second, small in a corner over the game.
    pub show_fps: bool,
    pub keys: Bindings,
    pub pad: Bindings,
    /// Who plays with what (the PLAYERS page), seat 1 first: what the page
    /// shows plays from a run's start; none: the engine's way (seat 1 with
    /// the keyboard and the pad that began the run).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub players: Vec<crate::seating::SeatPlan>,
    /// The host's address typed last on JOIN (a network game).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub address: String,
    /// The keyboard's halves once two play at it: the four directions
    /// (`input::DIRECTIONS`) and each action, the halves' own
    /// (`seating::LEFT_HALF`, `RIGHT_HALF` to start with).
    #[serde(default, skip_serializing_if = "Bindings::is_empty")]
    pub left_keys: Bindings,
    #[serde(default, skip_serializing_if = "Bindings::is_empty")]
    pub right_keys: Bindings,
    /// The pad buttons of a player after the first who bound their own,
    /// by seat ("2"); the others' are `pad`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub seat_pads: BTreeMap<String, Bindings>,
    /// The best runs, the best first, at most `HIGH_SCORES` (`rank`,
    /// `enter`); an empty table takes the game's own to beat
    /// (`adopt_scores`).
    pub high_scores: Vec<HighScore>,
    /// The one high score older versions kept: it goes into the table once.
    #[serde(skip_serializing)]
    high_score: u32,
    /// What the keys bound say on this keyboard, where it differs from
    /// their names in the bindings: bindings are places on a US keyboard
    /// (Z and X side by side for More Blood on any keyboard), a German one
    /// says Y where the bindings say Z (`key_label`; the window fills it).
    #[serde(skip)]
    pub key_names: BTreeMap<String, String>,
    /// The game's actions and pause, in the order the controls page shows
    /// them, and the bindings RESET goes back to (`adopt`).
    #[serde(skip)]
    pub actions: Vec<&'static str>,
    #[serde(skip)]
    default_keys: Bindings,
    #[serde(skip)]
    default_pad: Bindings,
    #[serde(skip)]
    default_halves: (Bindings, Bindings),
    /// The language the player chose (LANGUAGE), if one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The game's languages (`c.languages`, `offer_languages`) and the
    /// system's, best first (the window fills them; elsewhere none, so
    /// `shot` speaks the game's first on every machine).
    #[serde(skip)]
    pub languages: Vec<String>,
    #[serde(skip)]
    pub system_languages: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            music: 8,
            sound: 8,
            song: 0,
            fullscreen: false,
            shake: true,
            flashes: true,
            blur: true,
            shadows: Shadows::High,
            frame_rate: FrameRate::Unlimited,
            rumble: true,
            show_fps: false,
            keys: Bindings::default(),
            pad: Bindings::default(),
            players: Vec::new(),
            address: String::new(),
            left_keys: Bindings::default(),
            right_keys: Bindings::default(),
            seat_pads: BTreeMap::new(),
            high_scores: Vec::new(),
            high_score: 0,
            key_names: BTreeMap::new(),
            actions: Vec::new(),
            default_keys: Bindings::default(),
            default_pad: Bindings::default(),
            default_halves: Default::default(),
            language: None,
            languages: Vec::new(),
            system_languages: Vec::new(),
        }
    }
}

impl Settings {
    /// Takes on the game's actions (besides pause) and their keys and
    /// buttons to start with; what the player bound stays, but pausing
    /// gives way to the game's actions.
    pub fn adopt(&mut self, actions: &[&'static str], keys: &Bindings, pad: &Bindings) {
        self.actions = actions.iter().copied().chain([crate::controls::PAUSE]).collect();
        self.default_keys = keys.clone();
        self.default_keys.fill(&Bindings::pause_keys());
        self.default_pad = pad.clone();
        self.default_pad.fill(&Bindings::pause_pad());
        self.keys.fill(&self.default_keys);
        self.pad.fill(&self.default_pad);
        for b in [&mut self.default_keys, &mut self.default_pad, &mut self.keys, &mut self.pad] {
            b.pause_gives_way();
        }
        // The keyboard's halves: their directions, the game's actions in
        // order, their pause.
        use crate::input::DIRECTIONS;
        use crate::seating::{KEYBOARD_START, LEFT_HALF, LEFT_PAUSE, RIGHT_HALF};
        let half = |(dirs, keys): ([&str; 4], [&str; 6]), pause: &str| {
            let mut b = Bindings::default();
            for (name, key) in DIRECTIONS.iter().zip(dirs) {
                b.bind(name, key);
            }
            for (action, key) in actions.iter().zip(keys) {
                b.bind(action, key);
            }
            b.bind(crate::controls::PAUSE, pause);
            b
        };
        self.default_halves = (half(LEFT_HALF, LEFT_PAUSE), half(RIGHT_HALF, KEYBOARD_START));
        self.left_keys.fill(&self.default_halves.0);
        self.right_keys.fill(&self.default_halves.1);
    }

    /// The keys a part of the keyboard plays with: all of it the game's
    /// (`keys`), a half its own; none: nothing.
    pub fn keys_of(&self, part: crate::seating::Keys) -> Option<&Bindings> {
        use crate::seating::Keys;
        match part {
            Keys::Whole => Some(&self.keys),
            Keys::Left => Some(&self.left_keys),
            Keys::Right => Some(&self.right_keys),
            Keys::Nothing => None,
        }
    }

    pub fn keys_of_mut(&mut self, part: crate::seating::Keys) -> Option<&mut Bindings> {
        use crate::seating::Keys;
        match part {
            Keys::Whole => Some(&mut self.keys),
            Keys::Left => Some(&mut self.left_keys),
            Keys::Right => Some(&mut self.right_keys),
            Keys::Nothing => None,
        }
    }

    /// A seat's pad buttons: its own where its player bound them, else
    /// the game's (`pad`, seat 1's always).
    pub fn pad_of(&self, seat: usize) -> &Bindings {
        self.seat_pads.get(&seat.to_string()).filter(|_| seat > 1).unwrap_or(&self.pad)
    }

    /// A seat's pad buttons to bind (a seat after the first's own from
    /// then on, begun as the game's).
    pub fn pad_of_mut(&mut self, seat: usize) -> &mut Bindings {
        if seat <= 1 {
            return &mut self.pad;
        }
        self.seat_pads.entry(seat.to_string()).or_insert_with(|| self.pad.clone())
    }

    /// Back to the start's keys and buttons for one seat's player (its
    /// CONTROLS page): its part of the keyboard's and its pad's.
    pub fn reset_seat(&mut self, seat: usize, part: crate::seating::Keys) {
        use crate::seating::Keys;
        match part {
            Keys::Whole => self.keys = self.default_keys.clone(),
            Keys::Left => self.left_keys = self.default_halves.0.clone(),
            Keys::Right => self.right_keys = self.default_halves.1.clone(),
            Keys::Nothing => {}
        }
        match seat {
            1 => self.pad = self.default_pad.clone(),
            n => {
                self.seat_pads.remove(&n.to_string());
            }
        }
    }

    /// The languages the game speaks (`c.languages`).
    pub fn offer_languages(&mut self, languages: &[String]) {
        self.languages = languages.to_vec();
    }

    /// The language the game speaks to this player: the one chosen if the
    /// game speaks it, else the first of the system's it speaks, else its
    /// first; English for a game that names none.
    pub fn language(&self) -> String {
        let speaks = |l: &String| self.languages.contains(l);
        self.language.iter().chain(&self.system_languages).find(|l| speaks(l)).or(self.languages.first()).cloned().unwrap_or_else(|| "en".into())
    }

    /// The next (or, `back`, the last) of the game's languages
    /// (LANGUAGE).
    pub fn step_language(&mut self, back: bool) {
        let n = self.languages.len();
        if n == 0 {
            return;
        }
        let at = self.languages.iter().position(|l| *l == self.language()).unwrap_or(0);
        self.language = Some(self.languages[if back { (at + n - 1) % n } else { (at + 1) % n }].clone());
    }

    /// An empty table takes the game's lines to beat (`c.high_scores`; the
    /// engine's without them), and a high score kept by an older version
    /// goes in once; whether anything changed (then save).
    pub fn adopt_scores(&mut self, presets: &[(String, u32)]) -> bool {
        let mut changed = false;
        if self.high_scores.is_empty() {
            let presets: Vec<(String, u32)> = match presets {
                [] => PRESETS.iter().map(|&(i, s)| (i.to_string(), s)).collect(),
                own => own.to_vec(),
            };
            self.high_scores = presets.into_iter().take(HIGH_SCORES).map(|(initials, score)| HighScore { initials: initials.to_uppercase().chars().take(3).collect(), score }).collect();
            self.high_scores.sort_by_key(|h| std::cmp::Reverse(h.score));
            changed = true;
        }
        let old = std::mem::take(&mut self.high_score);
        changed |= old > 0 && self.enter("???", old).is_some();
        changed
    }

    /// Where `score` would stand in the table (0: the best), if it gets in:
    /// it must beat a line, and one as good as another goes below it.
    pub fn rank(&self, score: u32) -> Option<usize> {
        if score == 0 {
            return None;
        }
        let at = self.high_scores.iter().position(|h| score > h.score).unwrap_or(self.high_scores.len());
        (at < HIGH_SCORES).then_some(at)
    }

    /// Puts a run in the table; where it stands, if it got in.
    pub fn enter(&mut self, initials: &str, score: u32) -> Option<usize> {
        let at = self.rank(score)?;
        self.high_scores.insert(at, HighScore { initials: initials.to_string(), score });
        self.high_scores.truncate(HIGH_SCORES);
        Some(at)
    }

    /// A bound key as this keyboard labels it (`key_names`), for the menus.
    pub fn key_label(&self, name: &str) -> String {
        crate::controls::key_label(self.key_names.get(name).map_or(name, String::as_str))
    }

    /// The best score (the title's HI-SCORE).
    pub fn best(&self) -> u32 {
        self.high_scores.first().map_or(0, |h| h.score)
    }

    /// Back to the game's keys and buttons (RESET).
    pub fn reset_controls(&mut self) {
        self.keys = self.default_keys.clone();
        self.pad = self.default_pad.clone();
    }

    /// Music volume as a factor.
    pub fn music_gain(&self) -> f32 {
        self.music.min(VOLUME_STEPS) as f32 / VOLUME_STEPS as f32
    }

    pub fn sound_gain(&self) -> f32 {
        self.sound.min(VOLUME_STEPS) as f32 / VOLUME_STEPS as f32
    }

    /// The settings at `path`; the defaults when there are none yet or they
    /// cannot be read (`load_for` says why).
    pub fn load(path: &Path) -> Self {
        Self::load_for(path, false).0
    }

    /// The settings at `path` and, if the file was there but would not
    /// read, what happened: the file is copied to `NAME.toml.broken` first,
    /// so the defaults saved after it never wipe what the player had.
    /// `integrated`: the picture is drawn by the processor's own graphics,
    /// where the shadows start at MEDIUM; a file that names the shadows
    /// (the player chose) wins.
    pub fn load_for(path: &Path, integrated: bool) -> (Self, Option<String>) {
        let mut problem = None;
        let mut named_shadows = false;
        let mut settings = match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Settings>(&text) {
                Ok(settings) => {
                    named_shadows = toml::from_str::<toml::Table>(&text).is_ok_and(|t| t.contains_key("shadows"));
                    settings
                }
                Err(e) => {
                    eprintln!("settings: {}: {e}", path.display());
                    let mut kept = path.as_os_str().to_os_string();
                    kept.push(".broken");
                    let kept = PathBuf::from(kept);
                    problem = Some(match std::fs::copy(path, &kept) {
                        Ok(_) => format!("{}; a copy is kept as {}", e.message(), kept.display()),
                        Err(c) => format!("{} ({c})", e.message()),
                    });
                    Settings::default()
                }
            },
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("settings: {}: {e}", path.display());
                    problem = Some(e.to_string());
                }
                Settings::default()
            }
        };
        if integrated && !named_shadows {
            settings.shadows = Shadows::Medium;
        }
        (settings, problem)
    }

    /// Written beside and moved over (`blocks::save_at_once`): a crash or a
    /// full disk leaves the old file, never half of one.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).context("writing settings")?;
        crate::blocks::save_at_once(path, text.as_bytes())
    }

    /// What of the player's settings the game itself follows.
    pub fn options(&self) -> crate::session::Options {
        crate::session::Options { shake: self.shake, flashes: self.flashes, blur: self.blur, shadow_map: self.shadows.map_size() }
    }
}

/// The languages the player's system prefers, best first, two small
/// letters each (`de`), as SDL tells them.
pub fn system_languages() -> Vec<String> {
    let mut out = Vec::new();
    // Safe: SDL gives a list of `count` locales and wants it freed after.
    unsafe {
        let mut count = 0;
        let list = sdl3::sys::locale::SDL_GetPreferredLocales(&mut count);
        if list.is_null() {
            return out;
        }
        for i in 0..count.max(0) as usize {
            let locale = *list.add(i);
            if !locale.is_null() && !(*locale).language.is_null()
                && let Ok(lang) = std::ffi::CStr::from_ptr((*locale).language).to_str()
                && !out.iter().any(|l: &String| l == &lang.to_ascii_lowercase())
            {
                out.push(lang.to_ascii_lowercase());
            }
        }
        sdl3::sys::stdinc::SDL_free(list.cast());
    }
    out
}

/// Where a game's settings live: `<config>/<key>.toml` (`kept::config_dir`,
/// `kept::Identity`).
pub fn path_for(game_dir: &Path) -> PathBuf {
    crate::kept::kept_for(game_dir, crate::kept::Kept::Settings)
}

// Where other parts still name them here.
pub(crate) use crate::kept::{Kept, config_dir, game_name, kept_for, replays_for, settle};

#[cfg(test)]
mod tests {
    use super::*;

    /// The game speaks to the player in the language they chose, else in
    /// the first of the system's it speaks, else in its first; English for
    /// a game that names none; LANGUAGE steps round the game's.
    #[test]
    fn a_player_speaks_one_of_the_games_languages() {
        let mut s = Settings::default();
        assert_eq!(s.language(), "en");
        s.offer_languages(&["de".into(), "en".into(), "fr".into()]);
        assert_eq!(s.language(), "de", "the game's first");
        s.system_languages = vec!["es".into(), "fr".into()];
        assert_eq!(s.language(), "fr", "the system's first the game speaks");
        s.language = Some("en".into());
        assert_eq!(s.language(), "en", "the player's choice");
        s.step_language(false);
        assert_eq!(s.language(), "fr");
        s.step_language(false);
        assert_eq!(s.language(), "de", "round again");
        s.step_language(true);
        assert_eq!(s.language(), "fr");
        s.offer_languages(&["en".into()]);
        assert_eq!(s.language(), "en", "a choice the game does not speak gives way");
    }

    /// Saved settings come back as they were; a file from an older version
    /// with fewer fields fills in the defaults.
    #[test]
    fn settings_survive_a_save() {
        let dir = crate::scratch::Scratch::new("settings");
        let path = dir.join("settings.toml");
        let mut s = Settings { music: 3, song: 2, blur: false, ..Settings::default() };
        s.adopt_scores(&[]);
        s.enter("NIC", 12_345);
        s.keys.bind("fire", "Return");
        s.players = vec![crate::seating::SeatPlan { pad: Some("Pro Controller".into()), ..Default::default() }, crate::seating::SeatPlan { keyboard: crate::seating::Keys::Whole, pad: None }];
        s.right_keys.bind("fire", "M");
        s.pad_of_mut(2).bind("fire", "east");
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);

        std::fs::write(&path, "music = 5\n").unwrap();
        let mut old = Settings::load(&path);
        assert_eq!(old.music, 5);
        old.adopt(&crate::shooter::ACTIONS, &crate::shooter::keys(), &crate::shooter::pad());
        assert_eq!(old.keys.get("jump"), ["Space", "Z"], "the game's keys did not come in");
        assert_eq!(old.keys.get(crate::controls::PAUSE), ["Escape", "P"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A settings file that does not read is never silently written over:
    /// it is copied to `NAME.toml.broken`, the reason comes back to be said,
    /// the defaults play; a missing file is no trouble.
    #[test]
    fn an_unreadable_settings_file_is_kept_aside() {
        let dir = crate::scratch::Scratch::new("settings-broken");
        let path = dir.join("game.toml");
        let (fresh, problem) = Settings::load_for(&path, false);
        assert_eq!((fresh, problem), (Settings::default(), None), "no file, no trouble");
        std::fs::write(&path, "music = \"loud\"\nhigh_scores = [").unwrap();
        let (s, problem) = Settings::load_for(&path, false);
        assert_eq!(s, Settings::default());
        let why = problem.expect("the trouble is not reported");
        assert!(why.contains("game.toml.broken"), "{why}");
        assert_eq!(std::fs::read_to_string(dir.join("game.toml.broken")).unwrap(), "music = \"loud\"\nhigh_scores = [");
        // The defaults saved after it do not wipe the player's file.
        s.save(&path).unwrap();
        assert!(std::fs::read_to_string(dir.join("game.toml.broken")).unwrap().contains("loud"));
        assert_eq!(Settings::load_for(&path, false).1, None);
    }

    /// A save goes beside and is moved over: no half-written or leftover
    /// file, the folder made if need be.
    #[test]
    fn settings_are_written_whole_or_not_at_all() {
        let dir = crate::scratch::Scratch::new("settings-atomic");
        let path = dir.join("deeper").join("game.toml");
        Settings::default().save(&path).unwrap();
        Settings { music: 2, ..Settings::default() }.save(&path).unwrap();
        assert_eq!(Settings::load(&path).music, 2);
        let names: Vec<String> = std::fs::read_dir(path.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["game.toml"], "a temporary file is left");
        // A folder in the file's place cannot be written over: said, and nothing half-written.
        let blocked = dir.join("blocked.toml");
        std::fs::create_dir(&blocked).unwrap();
        assert!(Settings::default().save(&blocked).is_err());
    }

    /// On the processor's own graphics the shadows start at MEDIUM, a
    /// choice the player saved wins, other graphics start at HIGH.
    #[test]
    fn integrated_graphics_start_with_medium_shadows() {
        let dir = crate::scratch::Scratch::new("settings-shadows");
        let path = dir.join("game.toml");
        assert_eq!(Settings::load_for(&path, true).0.shadows, Shadows::Medium, "no file yet");
        assert_eq!(Settings::load_for(&path, false).0.shadows, Shadows::High);
        std::fs::write(&path, "music = 3\n").unwrap();
        assert_eq!(Settings::load_for(&path, true).0.shadows, Shadows::Medium, "a file that never chose");
        std::fs::write(&path, "shadows = \"high\"\n").unwrap();
        assert_eq!(Settings::load_for(&path, true).0.shadows, Shadows::High, "the player's choice");
        std::fs::write(&path, "shadows = \"low\"\n").unwrap();
        assert_eq!(Settings::load_for(&path, true).0.shadows, Shadows::Low);
        assert_eq!(Settings::default().shadows, Shadows::High, "shot's pictures stay as they were");
    }

    /// A run gets into the table by beating a line; as good as another, it
    /// goes below it; the table keeps ten. An empty table takes the game's
    /// lines, and the one high score of an older version goes in once.
    #[test]
    fn the_table_keeps_the_best_ten() {
        let mut s = Settings::default();
        assert!(s.adopt_scores(&[("ABC".into(), 500), ("XY".into(), 900)]));
        assert_eq!(s.high_scores.iter().map(|h| (h.initials.as_str(), h.score)).collect::<Vec<_>>(), [("XY", 900), ("ABC", 500)]);
        assert!(!s.adopt_scores(&[("ZZZ".into(), 1)]), "a table once filled stays the player's");
        assert_eq!(s.rank(500), Some(2), "as good as a line: below it");
        assert_eq!(s.rank(0), None);
        for n in 0..12 {
            s.enter("AAA", 100 + n);
        }
        assert_eq!(s.high_scores.len(), HIGH_SCORES);
        assert_eq!(s.rank(102), None, "worse than the tenth");
        assert_eq!(s.enter("WIN", 1000), Some(0));
        assert_eq!(s.best(), 1000);

        let old: Settings = toml::from_str("high_score = 7777\n").unwrap();
        let mut old = old;
        assert!(old.adopt_scores(&[]));
        assert_eq!(old.high_scores.iter().find(|h| h.score == 7777).map(|h| h.initials.as_str()), Some("???"));
        assert!(!toml::to_string(&old).unwrap().contains("high_score ="), "the old field is not written again");
    }
}
