//! A loaded game folder, the engine's side of a game: its config, level,
//! models and their bank on the GPU, sounds, music, the title scene and
//! its logo, the options, hot reload, the replays' fingerprint, and turning
//! the game into a frame for the renderer. The game itself (`game::Game`:
//! its rules, things and drawing) is `Session::game`.
//!
//! Here the session itself and what a run is made of (`Tick`, `Event`,
//! `Snapshot`, the fingerprint's parts); `load` the loading, fingerprint
//! and hot reload; `play` the ticks, input, seats, snapshots and saves;
//! `sound` the music and sounds fed to the speaker; `view` the sun, the
//! camera and a picture's own camera and daylight; `compose` the frame
//! for the renderer; `drawing` what a game draws into it (`Drawing`);
//! `figures` models turned, posed and as cubes; `models` the game's
//! models.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use glam::{IVec2, IVec3, Mat4, Quat, Vec2, Vec3};

use crate::saves::Slots;
use crate::audio::{self, Mixer, SoundBank};
use crate::camera;
use crate::config::{GameConfig, ShadowStyle};
use crate::mesh::{self, FaceMask};
use crate::render::{FrameData, Instance, Light, ModelBank, ModelDraw};
use crate::game::{Game, ModelSpec, Setup};
use crate::sim::{Body, Shape};
use crate::sim::things::Frames;
use crate::files::{Changes, Files};
use crate::font::DisplayFont;
use crate::menu::Menu;
use crate::controls::Controls;
use crate::settings::Settings;
use crate::vox::{self, VoxModel};
use crate::world::World;

mod compose;
mod drawing;
mod figures;
mod load;
mod models;
mod play;
mod sound;
mod view;
#[cfg(test)]
mod tests;

pub use drawing::*;
pub use figures::*;
pub use models::*;
use sound::*;
pub use view::*;

/// The point `t` of the way from `a` to `b`. Exactly `a` when `a == b`,
/// unlike `Vec3::lerp` (which computes `a * (1 - t) + b * t`): a resting
/// player at height 7 would otherwise land on 6.9999998 for some `t` and be
/// drawn a voxel lower in every few frames.
pub fn between(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// The initials a helped run's high score goes in with (`Session::helped`):
/// the console or a scenario made it, and the table says so.
pub const HELPED: &str = "GOD";

/// How a run begins besides from the game's files (`--scenario`, `--do`):
/// from a scenario, then console lines, before its first tick.
#[derive(Clone, Debug, Default)]
pub struct Begin {
    pub scenario: Option<String>,
    pub lines: Vec<String>,
}

impl Begin {
    /// Takes `--scenario NAME` or `--do LINE` (as often as wanted) among a
    /// command's flags; whether it was one of them.
    pub fn flag(&mut self, flag: &str, value: &str) -> bool {
        match flag {
            "--scenario" => self.scenario = Some(value.to_string()),
            "--do" => self.lines.push(value.to_string()),
            _ => return false,
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        self.scenario.is_none() && self.lines.is_empty()
    }

    /// Begins a fresh run of `game` so: its scenario, then each line, whose
    /// answer `told` hears. A mistake in either ends it.
    pub fn apply<G: Game>(&self, game: &mut Session<G>, mut told: impl FnMut(&str, &str)) -> Result<()> {
        if let Some(name) = &self.scenario {
            game.start_scenario(name).with_context(|| format!("the scenario {name}"))?;
        }
        for line in &self.lines {
            let said = game.eval(line).with_context(|| format!("--do {line}"))?;
            told(line, &said);
        }
        Ok(())
    }
}

/// What happens to a game between two of its ticks that the game hears of:
/// a save from the pause page, a console line. The next tick plays them
/// first, in order (`Tick::events`).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// SAVE into a slot, stamped with that time (seconds since 1970).
    Save { slot: u32, when: u64 },
    /// A line of the console.
    Eval(String),
    /// Files changed while the game runs (APPLY, `docs/multiplayer.md`
    /// stage 6): laid over its files and taken in, its state kept.
    Apply(Arc<Changes>),
}

/// One tick as a game plays it (`docs/multiplayer.md`, stage 4): what
/// happened since the tick before, then what every seat holds and plays
/// with, and the time a save in it is stamped with. The window and a
/// replay set these on the session, the network brings them whole
/// (`Session::play`); every tick played is kept (`Session::played`), and a
/// recording writes it down.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tick {
    pub events: Vec<Event>,
    /// The time of the machine whose clock is the game's, for the saves
    /// made in the tick (seconds since 1970).
    pub when: u64,
    /// Seat 1's controls and keys.
    pub controls: Controls,
    pub input: crate::input::Input,
    /// The seats after the first (seat 2 first).
    pub seats: Vec<crate::game::Seat>,
}

impl Tick {
    /// Takes it into a running hash of every tick played: what two machines
    /// compare to tell a difference in what they played from a difference
    /// in how they played it. The time a save is stamped with is left out
    /// (the game never reads it; a guest coming late stamps the saves it
    /// plays through with its own), and the controls of a seat nobody sits
    /// on.
    fn hash_into(&self, s: &mut crate::hash::Stable) {
        fn input(s: &mut crate::hash::Stable, i: Option<crate::input::Input>) {
            match i {
                Some(i) => s.u64(1 + (i.left as u64 | (i.right as u64) << 1 | (i.up as u64) << 2 | (i.down as u64) << 3 | (i.actions as u64) << 4)),
                None => s.u64(0),
            };
        }
        fn controls(s: &mut crate::hash::Stable, c: &Controls) {
            s.u64(c.device.map_or(0, |d| 1 + d as u64));
            for (action, key) in &c.keys {
                s.bytes(action.as_bytes()).bytes(key.as_bytes());
            }
            s.bytes(c.language.as_deref().unwrap_or("").as_bytes());
        }
        for event in &self.events {
            match event {
                Event::Save { slot, .. } => {
                    s.u64(1).u64(*slot as u64);
                }
                Event::Eval(code) => {
                    s.u64(2).bytes(code.as_bytes());
                }
                Event::Apply(changes) => {
                    s.u64(3);
                    for (name, file) in changes.iter() {
                        s.bytes(name.as_bytes()).bytes(file.as_ref().map_or(&b""[..], |f| f.hash.as_bytes()));
                    }
                }
            }
        }
        controls(s, &self.controls);
        input(s, Some(self.input));
        for seat in &self.seats {
            input(s, seat.input);
            if seat.input.is_some() {
                controls(s, &seat.controls);
            }
        }
    }
}

pub struct Session<G: Game> {
    pub dir: PathBuf,
    /// The game's files as it reads them (`crate::files`): its folder, or
    /// a set sealed, which a change on disk does not touch.
    pub files: Files,
    pub cfg: GameConfig,
    /// What stops the game is drawn over the picture (a window's); the
    /// IDE's PLAY says it in its own red line instead.
    pub problem_shown: bool,
    /// The game itself: its rules, things, drawing.
    pub game: G,
    pub models: Models,
    /// Every file the levels are built from; changing one rebuilds them.
    level_files: Vec<PathBuf>,
    /// The models the levels' pieces show (`Levels::piece_models`).
    piece_models: Vec<String>,
    /// Whether a level change fades (a window where someone plays; tools
    /// without one, replays and attract mode cut), and the fade being
    /// shown: no ticks run meanwhile.
    pub fades: bool,
    fade: Option<Fade>,
    /// A replay of a run whose level changes faded (`Replay::fades`): each
    /// change takes a fade's steps on the music's clock, as the window's
    /// did, without a picture.
    pub fade_clock: bool,
    /// The song changed by the game's code: over how many seconds the
    /// next one crossfades in (0: the old ends, the new begins).
    crossfade: Option<f32>,
    /// A song changed without a crossfade during the fade being shown:
    /// the music is silent until it is over (the pause between songs).
    fade_cut: bool,
    /// The title's time its flybys count from: the window holds it at the
    /// title's time while a run's initials are picked and the high score
    /// table shows, so nothing flies over then (the rotor was loud over the
    /// typing).
    pub flyby_from: f32,
    /// Whether the last `step` played a tick (not a fade's frame): what
    /// recordings count.
    pub ticked: bool,
    pub sounds: SoundBank,
    /// The music changed (its file or which file): start it over.
    pub music_changed: bool,
    /// Which of the game's songs plays.
    pub song: usize,
    /// The songs' titles (their `#title`, else the file name), in order.
    pub song_titles: Vec<String>,
    /// The songs' beats (`song_timings`), as the game was told them.
    songs: Vec<audio::music::Timing>,
    /// The game's own song when the music last changed (`Game::music`).
    cue: Option<usize>,
    /// The save slots: the player's on disk where someone plays, else this
    /// run's only (`crate::saves`).
    pub slots: Slots,
    /// What lay in the slots as the run began (their first lines): what a
    /// replay of it keeps (`world.saves` must read the same).
    pub slots_at_start: BTreeMap<u32, String>,
    /// The save the run began from (LOAD in the pause menu).
    pub started_from: Option<String>,
    /// The scenario the run began from (`--scenario`, the IDE's
    /// SCENARIOS), by name.
    pub scenario: Option<String>,
    /// The console lines run so far, in order (a recording takes each with
    /// the step it came after).
    pub evals: Vec<String>,
    /// The player's choices for the screen (the settings).
    pub options: Options,
    /// The big letters' font (`[ui] display_font`).
    pub font: Option<Arc<DisplayFont>>,
    /// The opaque voxels of every model in memory and the title logo's
    /// letters, put on the GPU once (`rebuild_bank`; a model loaded when
    /// needed goes in as it is first drawn, `bank_fresh`).
    bank: std::cell::RefCell<std::sync::Arc<ModelBank>>,
    /// The level last entered as the game marks it (`Game::level_mark`):
    /// the models loaded when needed follow it (`enter_level`).
    entered: u64,
    /// Where the logo's letters lie in the bank, per line.
    logo: Vec<Vec<std::ops::Range<u32>>>,
    /// The game's own logo model in the model bank, if it has one
    /// (`c.title_screen.logo_model`).
    logo_model: Option<crate::ui::LogoModel>,
    /// The simulation was changed by a reload since the game loaded: a run
    /// of it is no replay (it does not start from the files).
    pub touched: bool,
    /// Whether files were applied while it ran (APPLY): the run plays on
    /// from them, its replay carries them.
    pub applied: bool,
    /// The ground under each of the title's props, found once
    /// (`place_props`): looking for it every frame took most of a title with
    /// many props.
    props_ground: Vec<i32>,
    /// The game's actions as it loaded (`Game::actions`).
    actions: Vec<&'static str>,
    /// What the player plays with as the game sees it (`set_controls`).
    controls: Controls,
    /// The seats after the first (`c.players`; seat 2 first): who sits
    /// there and what they hold, for the next tick (`set_seat`).
    seats: Vec<crate::game::Seat>,
    /// The saves and console lines since the last tick (`Tick::events`).
    pending: Vec<Event>,
    /// The last tick played, its events with it.
    played: Tick,
    /// Every tick played so far as one number (`Tick::hash_into`).
    inputs: crate::hash::Stable,
    /// The time saves are stamped with now (`set_clock`).
    clock: u64,
    /// What this machine has of the game (`set_local`).
    local: crate::game::Local,
    /// What the last reload ran into (shown over the game until a reload
    /// works).
    pub problem: Option<String>,
    /// The sun the game set for the last picture (`frame:daylight`), to fit
    /// the next picture's shadows to before it says again.
    drawn_sun: std::cell::Cell<Option<Vec3>>,
    /// The faces pictures the game set with another pitch or field of view
    /// than its camera's could see (`frame:camera`): the meshes keep them
    /// too from then on (`face_mask`).
    seen_faces: std::cell::Cell<FaceMask>,
    /// The HUD as the last picture had it, worked out once while it stays
    /// the same (`hud_cubes`).
    hud_kept: std::cell::RefCell<compose::HudKept>,
    /// The limits a picture went past, each said once (`warnings`).
    limits: std::cell::RefCell<compose::Limits>,
}

/// What the player may turn off: the view shaking, white flashes, the blur;
/// and how fine the shadows are (the shadow map's size).
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub shake: bool,
    pub flashes: bool,
    pub blur: bool,
    pub shadow_map: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self { shake: true, flashes: true, blur: true, shadow_map: crate::settings::Shadows::High.map_size() }
    }
}

/// What a frame shows: the game being played, or the title scene (its
/// camera, its props, no player) `time` seconds into it.
#[derive(Clone, Copy, Debug)]
pub enum Scene {
    Play,
    Title { time: f32 },
    /// The glide from the title into the game, `t` from 0 to 1: the camera
    /// moves from the title's view to the player's, the props fade.
    Intro { time: f32, t: f32 },
}

/// Seconds the glide into the game takes.
pub const INTRO_TIME: f32 = 1.2;

/// The title's flyby: seconds it takes to cross the view.
const FLYBY_TIME: f32 = 5.0;

/// The game's config: the engine's defaults, then what the game sets
/// (`Game::conf`), checked: sections only the game reads, kinds with
/// behaviors it has.
pub fn config<G: Game>(dir: &Path) -> Result<GameConfig> {
    config_in::<G>(&Files::folder(dir))
}

/// The config of the game's files (`crate::files`).
pub fn config_in<G: Game>(files: &Files) -> Result<GameConfig> {
    let mut cfg = GameConfig::default();
    G::conf(files, &mut cfg)?;
    if cfg.game.contains_key("sounds") {
        anyhow::bail!("c.sounds is no more: a sound is a file now, sounds/NAME.toml, called by its name (wave = \"square\", freq = 880, a line each); `voxcade sounds {} --to-files` writes yours from conf.lua, then take the c.sounds lines out", files.dir().display());
    }
    cfg.sounds = audio::sound::read_folder(files)?;
    let sections = G::sections(&cfg);
    if let Some(unknown) = cfg.game.keys().find(|k| !sections.contains(&k.as_str())) {
        let own = if sections.is_empty() { String::new() } else { format!(" (the game's own: c.{})", sections.join(", c.")) };
        anyhow::bail!("c.{unknown}: no such section{own}");
    }
    crate::sim::things::defs(&cfg, &G::behaviors(&cfg))?;
    for (i, lang) in cfg.languages.iter().enumerate() {
        if lang.len() != 2 || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
            anyhow::bail!("c.languages[{}]: \"{lang}\" is no language: two small letters as \"de\", \"en\", \"fr\"", i + 1);
        }
        if cfg.languages[..i].contains(lang) {
            anyhow::bail!("c.languages: \"{lang}\" twice");
        }
    }
    if !(0.0..=12.0).contains(&cfg.audio.drive) {
        anyhow::bail!("c.audio.drive = {}: from 0 to 12 (decibels the sound is pushed into the limiter; 0 leaves it as it is)", cfg.audio.drive);
    }
    if !(0.0..=60.0).contains(&cfg.audio.crossfade) {
        anyhow::bail!("c.audio.crossfade = {}: seconds from 0 (none: the old song ends, the new begins) to 60", cfg.audio.crossfade);
    }
    if cfg.models.is_some() {
        anyhow::bail!("c.models is no more: frame:model draws any model file of the game by its path (\"sheet.vvx\", \"props/sheet.vvx\"), loaded when it is first drawn; take the c.models lines out (`voxcade upgrade {}` does it)", files.dir().display());
    }
    if cfg.audio.boss_music.is_some() {
        anyhow::bail!("c.audio.boss_music is no more: which song plays when is your game's code, music:play(\"end_boss\") while the boss is up and music:play() after (the song in c.audio.music)");
    }
    Ok(cfg)
}

/// The engine's sections of the config the simulation reads; the others
/// only change how the game looks and sounds. A game adds its own
/// (`Game::sections`).
const SIM_SECTIONS: [&str; 9] = ["world", "physics", "player", "debris", "markers", "things", "items", "saves", "state"];
/// Single keys the simulation reads in the other sections.
const SIM_KEYS: [(&str, &str); 4] = [("camera", "yaw"), ("camera", "follow"), ("camera", "hold"), ("effects", "hitstop")];

/// What a run depends on as one number (`Session::fingerprint`): the
/// simulation's part of the config as it is in effect (written out or left
/// at its default, the same), the game's actions (the input replays write)
/// and the files the simulation reads, each with its name relative to the
/// game folder (the game's code among them). Text files count with Unix
/// line ends, as Git may check them out with Windows ones.
fn fingerprint_of(cfg: &GameConfig, sections: &[&str], actions: &[&str], files: &[(String, Vec<u8>)]) -> Result<u64> {
    let parts = fingerprint_parts(cfg, sections, actions, files)?;
    let mut h = crate::hash::Stable::default();
    h.bytes(&parts[0].1);
    for (name, bytes) in &parts[1..] {
        h.bytes(name.as_bytes()).bytes(bytes);
    }
    Ok(h.finish())
}

/// What the fingerprint is made of: the simulation's part of the settings
/// (as TOML, named "conf.lua"), then each file's bytes, a text with Unix
/// line ends.
fn fingerprint_parts(cfg: &GameConfig, sections: &[&str], actions: &[&str], files: &[(String, Vec<u8>)]) -> Result<Vec<(String, Vec<u8>)>> {
    let toml::Value::Table(all) = toml::Value::try_from(cfg)? else { anyhow::bail!("the config is no table") };
    let mut read = toml::Table::new();
    for &section in SIM_SECTIONS.iter().chain(sections) {
        if let Some(v) = all.get(section) {
            read.insert(section.to_string(), v.clone());
        }
    }
    for (section, key) in SIM_KEYS {
        if let Some(v) = all.get(section).and_then(|t| t.get(key)) {
            read.insert(format!("{section}.{key}"), v.clone());
        }
    }
    // The seats, for a game of several (a game of one counts as before).
    if let Some(v) = all.get("players") {
        read.insert("players".into(), v.clone());
    }
    read.insert("actions".into(), toml::Value::Array(actions.iter().map(|a| toml::Value::String(a.to_string())).collect()));
    let mut parts = vec![("conf.lua".to_string(), toml::to_string(&read)?.into_bytes())];
    for (name, bytes) in files {
        let text = !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok();
        let bytes = if text && bytes.contains(&b'\r') { String::from_utf8_lossy(bytes).replace("\r\n", "\n").into_bytes() } else { bytes.clone() };
        parts.push((name.clone(), bytes));
    }
    Ok(parts)
}

/// Whether `path` is one of the game's voxel blocks (`blocks/NAME.vvx`, a
/// new one too; not the hidden file a block is saved through).
fn is_block_file(dir: &Path, path: &Path) -> bool {
    let shown = path.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.'));
    shown && crate::vvx::is_model(path) && path.parent().is_some_and(|p| same_file(p, &dir.join(crate::blocks::FOLDER)))
}

/// Whether two paths name the same file, however they are spelled.
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// A game's state as it stood at one tick (`Session::snapshot`).
pub struct Snapshot<G: Game> {
    kept: G::Snapshot,
    /// The slots as they stood (a second played twice may save).
    slots: Slots,
    controls: Controls,
    seats: Vec<crate::game::Seat>,
    pending: Vec<Event>,
    played: Tick,
    inputs: crate::hash::Stable,
    clock: u64,
    /// The game's files then: a state kept before an APPLY comes back with
    /// the files and code before it.
    files: Files,
}

/// The seats after the first a game has (`c.players` less one).
fn session_seats(cfg: &GameConfig) -> usize {
    cfg.players.max(1) as usize - 1
}

/// What a reload changed, the least far-reaching first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reloaded {
    Nothing,
    /// The game's code (a Lua game's `main.lua`).
    Code,
    Music,
    Sounds,
    Models,
    /// Geometry or face set changed: rebuild every chunk mesh.
    World,
}

/// A level change being shown: the picture fades out to a colour, the
/// change is done, and it fades in again.
struct Fade {
    out: bool,
    t: f32,
    length: f32,
    color: [f32; 3],
}

impl Fade {
    /// The colour and how far over the picture it lies now.
    fn look(&self) -> [f32; 4] {
        let far = (self.t / self.length).clamp(0.0, 1.0);
        let [r, g, b] = self.color;
        [r, g, b, if self.out { far } else { 1.0 - far }]
    }
}
