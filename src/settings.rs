//! The player's settings: volumes, which song plays, the screen, effects,
//! controls and the high score table. Kept per game in the user's config folder
//! (`voxcade/<key>.toml`, `Identity`), not in the game, so a game's folder
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
        let at = Self::ALL.iter().position(|&s| s == self).unwrap_or(0) as i32;
        Self::ALL[(at + step).rem_euclid(Self::ALL.len() as i32) as usize]
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
        let at = Self::ALL.iter().position(|&s| s == self).unwrap_or(0) as i32;
        Self::ALL[(at + step).rem_euclid(Self::ALL.len() as i32) as usize]
    }
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

/// Where a game's settings live: `<config>/<key>.toml` (`config_dir`,
/// `Identity::key`).
pub fn path_for(game_dir: &Path) -> PathBuf {
    kept_for(game_dir, Kept::Settings)
}

/// Where the runs played in the window are kept: beside the settings,
/// `voxcade/replays/<key>/`.
pub fn replays_for(game_dir: &Path) -> PathBuf {
    kept_for(game_dir, Kept::Runs)
}

/// The player's config folder, `voxcade` in the system's place for it:
/// `$XDG_CONFIG_HOME`, else `~/.config` (Linux, and macOS too for now: no
/// macOS release yet, Application Support would be its place), on Windows
/// `%APPDATA%` (a `voxcade` folder an older Voxcade made under `HOME`'s
/// `.config`, while there is none in `%APPDATA%`, stays in use).
pub(crate) fn config_dir() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    config_from(var("XDG_CONFIG_HOME"), var("HOME"), var("APPDATA"), cfg!(windows), &|p: &Path| p.is_dir())
}

/// `config_dir` from the variables it reads (`is_dir` asks the disk).
fn config_from(xdg: Option<PathBuf>, home: Option<PathBuf>, appdata: Option<PathBuf>, windows: bool, is_dir: &dyn Fn(&Path) -> bool) -> PathBuf {
    let under_home = home.map(|h| h.join(".config").join("voxcade"));
    let appdata = appdata.map(|a| a.join("voxcade"));
    let chosen = match (xdg, windows) {
        (Some(x), _) => Some(x.join("voxcade")),
        (None, true) => match (&appdata, &under_home) {
            (Some(a), Some(h)) if !is_dir(a) && is_dir(h) => under_home,
            _ => appdata.or(under_home),
        },
        (None, false) => under_home.or(appdata),
    };
    chosen.unwrap_or_else(|| PathBuf::from("voxcade"))
}

/// The games that come with Voxcade (the source's `games/`; the package
/// has them but the Arena and the Bench): their folders' bare names are
/// theirs alone in the config folder (`settle`; test
/// `the_games_of_voxcade_are_the_sources_games`).
pub(crate) const GAMES_OF_VOXCADE: [&str; 7] = ["arena", "bench", "bunnyfarm", "moreblood", "pulsar", "sokovox", "voxkeep"];

/// Who a game folder is for what is kept of it outside the folder: its
/// settings and high scores, saves, kept runs, and the IDE's memory,
/// OUTPUT, deleted files, journals and lock.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Identity {
    /// The folder's name.
    pub name: String,
    /// The name and the first 8 signs of the BLAKE3 of the folder's
    /// canonical path (`voxkeep-1a2b3c4d`): this folder and no other.
    pub folder: String,
    /// What its kept things are named by: the bare name for a game that
    /// comes with Voxcade (`comes_with_voxcade`: its high scores survive an
    /// update and a package unpacked elsewhere), else `folder`.
    pub key: String,
}

impl Identity {
    pub fn of(game_dir: &Path) -> Self {
        let path = game_dir.canonicalize().or_else(|_| std::path::absolute(game_dir)).unwrap_or_else(|_| game_dir.to_path_buf());
        let name = path.file_name().map_or_else(|| "game".to_string(), |n| n.to_string_lossy().into_owned());
        let hash = blake3::hash(path.to_string_lossy().as_bytes()).to_hex();
        let folder = format!("{name}-{}", &hash[..8]);
        let key = if comes_with_voxcade(&path) { name.clone() } else { folder.clone() };
        Self { name, folder, key }
    }
}

/// Whether the game folder `dir` (canonical) is one that comes with
/// Voxcade: it lies in a `games` folder beside a Voxcade program (a
/// package) or in a Voxcade source tree's. Told by where it lies, not by
/// which program asks, so every program names it alike.
fn comes_with_voxcade(dir: &Path) -> bool {
    let Some(games) = dir.parent().filter(|g| g.file_name().is_some_and(|n| n == "games")) else { return false };
    let Some(root) = games.parent() else { return false };
    root.join("voxcade").is_file() || root.join("voxcade.exe").is_file() || std::fs::read_to_string(root.join("Cargo.toml")).is_ok_and(|t| t.lines().any(|l| l.trim() == "name = \"voxcade\""))
}

/// What is kept of a game in the config folder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kept {
    /// The player's settings and high scores, `<key>.toml`.
    Settings,
    /// A settings file that would not read, kept aside (`Settings::load_for`).
    Broken,
    /// The save games, `saves/<key>/`.
    Saves,
    /// The runs kept from the window, `replays/<key>/`.
    Runs,
    /// What the IDE remembers, `ide/<key>.toml`.
    IdeMemory,
    /// OUTPUT's lines, `ide/output/<key>.toml`.
    IdeOutput,
    /// Files deleted where there was no trash, `ide/deleted/<key>/`.
    IdeDeleted,
    /// The open texts' journals, `ide/<folder>.journal/`: this folder's
    /// files only, never adopted.
    Journals,
    /// The IDE's MCP lock, `ide/<folder>.lock`: this folder's IDE only.
    Lock,
}

impl Kept {
    /// What a game adopts of what an older Voxcade kept under its bare name.
    const ADOPTED: [Kept; 7] = [Kept::Settings, Kept::Broken, Kept::Saves, Kept::Runs, Kept::IdeMemory, Kept::IdeOutput, Kept::IdeDeleted];

    /// Where it lies in the config folder `config` for the name `key`.
    fn at(self, config: &Path, key: &str) -> PathBuf {
        let ide = config.join("ide");
        match self {
            Kept::Settings => config.join(format!("{key}.toml")),
            Kept::Broken => config.join(format!("{key}.toml.broken")),
            Kept::Saves => config.join("saves").join(key),
            Kept::Runs => config.join("replays").join(key),
            Kept::IdeMemory => ide.join(format!("{key}.toml")),
            Kept::IdeOutput => ide.join("output").join(format!("{key}.toml")),
            Kept::IdeDeleted => ide.join("deleted").join(key),
            Kept::Journals => ide.join(format!("{key}.journal")),
            Kept::Lock => ide.join(format!("{key}.lock")),
        }
    }

    /// The name of `id` this is kept under.
    fn name_of(self, id: &Identity) -> &str {
        match self {
            Kept::Journals | Kept::Lock => &id.folder,
            _ => &id.key,
        }
    }
}

/// Where `what` of the game in `game_dir` is kept.
pub(crate) fn kept_for(game_dir: &Path, what: Kept) -> PathBuf {
    what.at(&config_dir(), what.name_of(&Identity::of(game_dir)))
}

/// The list of the game folders the config folder's names stand for
/// (`folders/index.toml`): written by `settle`, read by people.
const FOLDERS: &str = "voxcade folders 1";

#[derive(Default, Deserialize, Serialize)]
struct Folders {
    format: String,
    /// Each key settled (`Identity::key`), its folder's path.
    #[serde(default)]
    folders: BTreeMap<String, String>,
}

/// Settles who the game in `game_dir` is in the config folder, before a
/// window plays it or the IDE opens it (`settle_in`).
pub fn settle(game_dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let beside = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("games")));
    let claimed = |name: &str| GAMES_OF_VOXCADE.contains(&name) || beside.as_ref().is_some_and(|b| b.join(name).join("conf.lua").is_file());
    let moved = settle_in(&config_dir(), game_dir, &claimed);
    for (from, to) in &moved {
        eprintln!("kept for {}: {} is now {}", game_dir.display(), from.display(), to.display());
    }
    moved
}

/// The adoption rule: a game that comes with Voxcade keeps its bare name
/// and nothing moves. Another game, the first time it is settled (its key not yet in `folders/index.toml`),
/// adopts what an older Voxcade kept under its folder's bare name, unless
/// that name is `claimed` by a game that comes with Voxcade (a copy of
/// Voxkeep never takes Voxkeep's saves): each such thing is moved to its
/// key where the key has none yet, so a second folder of the same name
/// starts clean. Then the key goes into the index with the folder's path,
/// and nothing is adopted for it again. A folder moved or renamed is
/// another folder: it starts clean, what was kept under its old key stays
/// there. What moved, from and to.
fn settle_in(config: &Path, game_dir: &Path, claimed: &dyn Fn(&str) -> bool) -> Vec<(PathBuf, PathBuf)> {
    let id = Identity::of(game_dir);
    let mut moved = Vec::new();
    if id.key == id.name {
        return moved;
    }
    let index = config.join("folders").join("index.toml");
    let mut folders: Folders = std::fs::read_to_string(&index).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default();
    if folders.folders.contains_key(&id.key) {
        return moved;
    }
    if !claimed(&id.name) {
        for what in Kept::ADOPTED {
            let (from, to) = (what.at(config, &id.name), what.at(config, &id.key));
            // `ide/claude.toml` was the IDE's own once (`Prefs::load`).
            if what == Kept::IdeMemory && id.name == "claude" {
                continue;
            }
            if from.exists() && !to.exists() {
                if let Some(parent) = to.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::rename(&from, &to) {
                    Ok(()) => moved.push((from, to)),
                    Err(e) => eprintln!("kept for {}: {} stays ({e})", game_dir.display(), from.display()),
                }
            }
        }
    }
    folders.format = FOLDERS.into();
    let path = game_dir.canonicalize().unwrap_or_else(|_| game_dir.to_path_buf());
    folders.folders.insert(id.key, path.to_string_lossy().into_owned());
    match toml::to_string(&folders) {
        Ok(text) => {
            if let Err(e) = crate::blocks::save_at_once(&index, text.as_bytes()) {
                eprintln!("{}: {e:#}", index.display());
            }
        }
        Err(e) => eprintln!("{}: {e}", index.display()),
    }
    moved
}

/// The game folder's name (what a network game is known by,
/// `network::app_of`, never `Identity`'s key).
pub(crate) fn game_name(game_dir: &Path) -> String {
    game_dir
        .canonicalize()
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "game".to_string())
}

impl Settings {
    /// What of the player's settings the game itself follows.
    pub fn options(&self) -> crate::session::Options {
        crate::session::Options { shake: self.shake, flashes: self.flashes, blur: self.blur, shadow_map: self.shadows.map_size() }
    }
}

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

    /// A game folder with a `conf.lua` at `dir`.
    fn game_at(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c) end\n").unwrap();
        dir.canonicalize().unwrap()
    }

    /// Everything an older Voxcade kept for the name `key` in `config`,
    /// each saying `said`.
    fn keep_all(config: &Path, key: &str, said: &str) {
        for what in Kept::ADOPTED {
            let at = what.at(config, key);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            if matches!(what, Kept::Saves | Kept::Runs | Kept::IdeDeleted) {
                std::fs::create_dir_all(&at).unwrap();
                std::fs::write(at.join("1.toml"), said).unwrap();
            } else {
                std::fs::write(&at, said).unwrap();
            }
        }
    }

    /// What `config` holds for the name `key`, thing by thing (None: not there).
    fn held(config: &Path, key: &str) -> Vec<Option<String>> {
        Kept::ADOPTED
            .iter()
            .map(|what| {
                let at = what.at(config, key);
                let at = if at.is_dir() { at.join("1.toml") } else { at };
                std::fs::read_to_string(at).ok()
            })
            .collect()
    }

    fn all(said: &str) -> Vec<Option<String>> {
        vec![Some(said.to_string()); Kept::ADOPTED.len()]
    }

    fn none() -> Vec<Option<String>> {
        vec![None; Kept::ADOPTED.len()]
    }

    /// A game folder is known by its name and a short hash of its whole
    /// path, two folders of one name apart; a game that comes with Voxcade
    /// (in the `games` folder of a package or of Voxcade's source) keeps
    /// its bare name, whichever program asks. The journals and the lock
    /// are always the folder's own.
    #[test]
    fn a_game_is_known_by_its_folders_path() {
        let root = crate::scratch::Scratch::new("identity");
        let package = root.join("package");
        game_at(&package.join("games").join("voxkeep"));
        std::fs::write(package.join("voxcade"), "the program").unwrap();
        let source = root.join("source");
        game_at(&source.join("games").join("voxkeep"));
        std::fs::write(source.join("Cargo.toml"), "[package]\nname = \"voxcade\"\n").unwrap();
        let other_source = root.join("other");
        game_at(&other_source.join("games").join("voxkeep"));
        std::fs::write(other_source.join("Cargo.toml"), "[package]\nname = \"another\"\n").unwrap();
        let own = game_at(&root.join("projects").join("voxkeep"));
        let own_too = game_at(&root.join("more").join("voxkeep"));
        for dir in [package.join("games/voxkeep"), source.join("games/voxkeep")] {
            let id = Identity::of(&dir);
            assert_eq!((id.name.as_str(), id.key.as_str()), ("voxkeep", "voxkeep"), "{}", dir.display());
            assert_ne!(id.folder, id.key, "its journals are its folder's own");
        }
        let (a, b, c) = (Identity::of(&own), Identity::of(&own_too), Identity::of(&other_source.join("games/voxkeep")));
        for id in [&a, &b, &c] {
            assert_eq!(id.key, id.folder);
            assert!(id.key.starts_with("voxkeep-") && id.key.len() == "voxkeep-".len() + 8 && id.key[8..].chars().all(|c| c.is_ascii_hexdigit()), "{}", id.key);
        }
        assert!(a.key != b.key && b.key != c.key && a.key != c.key, "two folders of one name");
        assert_eq!(Identity::of(&root.join("projects").join(".").join("voxkeep")), a, "the same folder however written");
        let config = root.join("config");
        assert_eq!(Kept::Journals.at(&config, Kept::Journals.name_of(&a)), config.join("ide").join(format!("{}.journal", a.folder)));
        assert_eq!(Kept::Settings.at(&config, Kept::Settings.name_of(&Identity::of(&source.join("games/voxkeep")))), config.join("voxkeep.toml"));
    }

    /// The adoption rule, every branch: a game of its own adopts what an
    /// older Voxcade kept under its bare name, moved, once; a second folder
    /// of that name starts clean; nothing is adopted again once settled; a
    /// thing its key already has is not written over; a game that comes
    /// with Voxcade moves nothing and keeps its name; a copy of one never
    /// takes its things; a folder moved starts clean and what was kept
    /// stays; the IDE's own `claude.toml` is no game's.
    #[test]
    fn a_game_adopts_what_its_bare_name_kept_once() {
        let root = crate::scratch::Scratch::new("adoption");
        let config = root.join("config");
        let claimed = |name: &str| name == "voxkeep";

        // A game of its own adopts, moved.
        keep_all(&config, "test", "old");
        let first = game_at(&root.join("a").join("test"));
        let moved = settle_in(&config, &first, &claimed);
        assert_eq!(moved.len(), Kept::ADOPTED.len());
        let key = Identity::of(&first).key;
        assert_eq!(held(&config, &key), all("old"));
        assert_eq!(held(&config, "test"), none(), "moved, not copied");
        let index = std::fs::read_to_string(config.join("folders/index.toml")).unwrap();
        assert!(index.starts_with("format = \"voxcade folders 1\"") && index.contains(&key) && index.contains(&*first.to_string_lossy()), "{index}");

        // A second folder of the name starts clean.
        let second = game_at(&root.join("b").join("test"));
        assert!(settle_in(&config, &second, &claimed).is_empty());
        assert_eq!(held(&config, &Identity::of(&second).key), none());

        // Settled once: what an older Voxcade keeps under the name later is not taken.
        keep_all(&config, "test", "later");
        assert!(settle_in(&config, &first, &claimed).is_empty() && settle_in(&config, &second, &claimed).is_empty());
        assert_eq!((held(&config, &key), held(&config, "test")), (all("old"), all("later")));

        // What the key has already is not written over.
        let third = game_at(&root.join("c").join("test"));
        let third_key = Identity::of(&third).key;
        std::fs::write(Kept::Settings.at(&config, &third_key), "mine").unwrap();
        let moved = settle_in(&config, &third, &claimed);
        assert_eq!(moved.len(), Kept::ADOPTED.len() - 1);
        assert_eq!(std::fs::read_to_string(Kept::Settings.at(&config, &third_key)).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(Kept::Settings.at(&config, "test")).unwrap(), "later", "left where it was");

        // A game that comes with Voxcade keeps its bare name; a copy of one takes nothing.
        keep_all(&config, "voxkeep", "voxkeep's");
        let source = root.join("source");
        std::fs::write({ std::fs::create_dir_all(&source).unwrap(); source.join("Cargo.toml") }, "[package]\nname = \"voxcade\"\n").unwrap();
        let bundled = game_at(&source.join("games").join("voxkeep"));
        assert!(settle_in(&config, &bundled, &claimed).is_empty());
        let copy = game_at(&root.join("projects").join("voxkeep"));
        assert!(settle_in(&config, &copy, &claimed).is_empty(), "a copy took the bundled game's things");
        assert_eq!(held(&config, "voxkeep"), all("voxkeep's"));
        assert_eq!(held(&config, &Identity::of(&copy).key), none());
        assert!(!std::fs::read_to_string(config.join("folders/index.toml")).unwrap().contains("\"voxkeep\""), "a bundled game is in no index");

        // A folder moved is another: it starts clean, its old things stay.
        std::fs::remove_file(Kept::Settings.at(&config, "test")).unwrap();
        let away = root.join("moved").join("test");
        std::fs::create_dir_all(away.parent().unwrap()).unwrap();
        std::fs::rename(&first, &away).unwrap();
        assert!(settle_in(&config, &away, &claimed).is_empty());
        assert_eq!(held(&config, &Identity::of(&away).key), none());
        assert_eq!(held(&config, &key), all("old"), "kept under the old key");

        // `ide/claude.toml` is the IDE's own.
        std::fs::create_dir_all(config.join("ide")).unwrap();
        std::fs::write(config.join("ide/claude.toml"), "resume = true\n").unwrap();
        let claude = game_at(&root.join("d").join("claude"));
        settle_in(&config, &claude, &claimed);
        assert!(config.join("ide/claude.toml").is_file());
    }

    /// The config folder: `XDG_CONFIG_HOME`'s, else `~/.config`'s; on
    /// Windows `%APPDATA%`'s, unless only an older Voxcade's under `HOME`
    /// is there, which stays in use.
    #[test]
    fn the_config_folder_is_the_systems_place_for_it() {
        let p = |s: &str| Some(PathBuf::from(s));
        let none = |_: &Path| false;
        let home_has = |d: &Path| d == Path::new("/h/.config/voxcade");
        assert_eq!(config_from(p("/x"), p("/h"), p("/a"), true, &none), PathBuf::from("/x/voxcade"));
        assert_eq!(config_from(None, p("/h"), p("/a"), false, &none), PathBuf::from("/h/.config/voxcade"));
        assert_eq!(config_from(None, p("/h"), p("/a"), true, &none), PathBuf::from("/a/voxcade"), "Windows: APPDATA before HOME");
        assert_eq!(config_from(None, p("/h"), p("/a"), true, &home_has), PathBuf::from("/h/.config/voxcade"), "an older Voxcade's stays in use");
        let both = |d: &Path| d == Path::new("/h/.config/voxcade") || d == Path::new("/a/voxcade");
        assert_eq!(config_from(None, p("/h"), p("/a"), true, &both), PathBuf::from("/a/voxcade"));
        assert_eq!(config_from(None, None, p("/a"), false, &none), PathBuf::from("/a/voxcade"));
        assert_eq!(config_from(None, p("/h"), None, true, &none), PathBuf::from("/h/.config/voxcade"));
    }

    /// `GAMES_OF_VOXCADE` names the source's games, no more and no fewer.
    #[test]
    fn the_games_of_voxcade_are_the_sources_games() {
        let mut games: Vec<String> = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("games")).unwrap().flatten().filter(|e| e.path().join("conf.lua").is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        games.sort();
        assert_eq!(games, GAMES_OF_VOXCADE);
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
