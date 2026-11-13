//! A loaded game folder, the engine's side of a game: its config, level,
//! models and their bank on the GPU, sounds, music, the title scene and
//! its logo, the options, hot reload, the replays' fingerprint, and turning
//! the game into a frame for the renderer. The game itself (`game::Game`:
//! its rules, things and drawing) is `Session::game`.

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

/// The point `t` of the way from `a` to `b`. Exactly `a` when `a == b`,
/// unlike `Vec3::lerp` (which computes `a * (1 - t) + b * t`): a resting
/// player at height 7 would otherwise land on 6.9999998 for some `t` and be
/// drawn a voxel lower in every few frames.
pub fn between(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// The models a game draws, by the names it gave them (`Game::models`),
/// and the title scene's, by file.
pub struct Models {
    actors: Vec<(ModelSpec, VoxModel)>,
    pub props: HashMap<String, VoxModel>,
}

/// The game's display font, if it names one (a broken one is reported and
/// left out: the pixel font stands in).
fn display_font(files: &Files, ui: &crate::config::UiConfig) -> Option<Arc<DisplayFont>> {
    let file = ui.display_font.as_ref()?;
    DisplayFont::load_in(files, &files.path(file)).map_err(|e| eprintln!("display font: {e:#}")).ok().map(Arc::new)
}

/// The titles of the game's songs: each file's `#title` line, else its name.
fn song_titles(files: &Files, cfg: &GameConfig) -> Vec<String> {
    cfg.audio
        .music
        .files()
        .iter()
        .map(|f| {
            files
                .read_to_string(f)
                .ok()
                .and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("#title").map(|v| v.trim().to_string())))
                .unwrap_or_else(|| Path::new(f).file_stem().map_or(f.to_string(), |n| n.to_string_lossy().into_owned()))
        })
        .collect()
}

/// The beats of the songs of `c.audio.music`, in order: a song that does
/// not read keeps its place, at 120 beats a minute (its mistake shows when
/// it is played).
fn song_timings(files: &Files, cfg: &GameConfig) -> Vec<audio::music::Timing> {
    cfg.audio
        .music
        .files()
        .iter()
        .map(|f| {
            let song = files.read_to_string(f).ok().and_then(|t| audio::music::parse(&t).ok());
            match song {
                Some(song) => song.timing(f),
                None => audio::music::Song::empty().timing(f),
            }
        })
        .collect()
}

/// The files of the title scene's models.
fn title_models(cfg: &GameConfig) -> impl Iterator<Item = &str> {
    let t = &cfg.title_screen;
    t.props.iter().map(|p| p.model.as_str()).chain(t.flyby.as_deref()).chain(t.logo_model.as_deref())
}

impl Models {
    /// The game's models, and the parts its kinds swing checked against
    /// them (`c.things.NAME.swing`: a figure of parts, each part there).
    /// The game's models, `pieces` too (the models its levels set down,
    /// which `conf.lua` need not name).
    fn load_for<G: Game>(files: &Files, cfg: &GameConfig, pieces: &[String]) -> Result<Self> {
        let mut specs = G::models(cfg)?;
        for file in pieces {
            if !specs.iter().any(|s| &s.file == file) {
                specs.push(ModelSpec { name: file.clone(), file: file.clone(), shaped: false });
            }
        }
        let models = Self::load(files, cfg, specs)?;
        for d in crate::sim::things::defs(cfg, &G::behaviors(cfg))? {
            if d.swing.is_empty() {
                continue;
            }
            let Some(m) = d.model.as_deref().and_then(|file| models.named(file)) else {
                anyhow::bail!("c.things.{}.swing: a kind with a model of parts swings them (it has none)", d.name);
            };
            if m.parts.is_empty() {
                anyhow::bail!("c.things.{}.swing: {} is no figure of parts (its objects named in the file: head, body ...)", d.name, d.model.as_deref().unwrap_or_default());
            }
            if let Some(missing) = d.swing.keys().find(|k| !m.parts.iter().any(|p| &p.name == *k)) {
                let names: Vec<&str> = m.parts.iter().map(|p| p.name.as_str()).collect();
                anyhow::bail!("c.things.{}.swing.{missing}: no such part (its parts: {})", d.name, names.join(", "));
            }
        }
        Ok(models)
    }

    fn load(files: &Files, cfg: &GameConfig, specs: Vec<ModelSpec>) -> Result<Self> {
        let load = |file: &str| -> Result<VoxModel> {
            let mut model = vox::load_model_in(files, &files.path(file))?;
            let light = &cfg.light;
            model.shade(light.model_shading_for.get(file).copied().unwrap_or(light.model_shading), light.edge_tolerance);
            Ok(model)
        };
        Ok(Self {
            actors: specs
                .into_iter()
                .map(|spec| -> Result<(ModelSpec, VoxModel)> {
                    let model = load(&spec.file)?;
                    Ok((spec, model))
                })
                .collect::<Result<_>>()?,
            props: title_models(cfg).map(|f| Ok((f.to_string(), load(f)?))).collect::<Result<_>>()?,
        })
    }

    /// How many frames each of the game's models has, by name.
    pub fn frame_counts(&self) -> std::collections::BTreeMap<String, usize> {
        self.actors.iter().map(|(spec, m)| (spec.name.clone(), m.frames.len())).collect()
    }

    /// How tall each of the game's models is, by name (a piece's body).
    pub fn heights(&self) -> std::collections::BTreeMap<String, f32> {
        self.actors.iter().map(|(spec, m)| (spec.name.clone(), m.size.z as f32)).collect()
    }

    /// The parts of the game's figures of parts, by the model's name and
    /// by its file.
    pub fn part_names(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        self.actors
            .iter()
            .filter(|(_, m)| !m.parts.is_empty())
            .flat_map(|(spec, m)| {
                let names: Vec<String> = m.parts.iter().map(|p| p.name.clone()).collect();
                [(spec.name.clone(), names.clone()), (spec.file.clone(), names)]
            })
            .collect()
    }

    /// The game's model called `name`.
    pub fn get(&self, name: &str) -> Option<&VoxModel> {
        self.actors.iter().find(|(spec, _)| spec.name == name).map(|(_, m)| m)
    }

    /// The game's model called `name`, or kept in that file.
    pub fn named(&self, name: &str) -> Option<&VoxModel> {
        self.get(name).or_else(|| self.actors.iter().find(|(spec, _)| spec.file == name).map(|(_, m)| m))
    }

    /// A model as the simulation sees it: its size, and its voxels to burst
    /// into.
    pub fn shape(&self, name: &str) -> Option<Shape> {
        self.get(name).map(|m| Shape { size: m.size, voxels: m.frames[0].clone() })
    }

    /// Every model, in a fixed order: the game's as it named them, then the
    /// title's by file.
    fn all_mut(&mut self) -> Vec<&mut VoxModel> {
        let mut props: Vec<_> = self.props.iter_mut().collect();
        props.sort_unstable_by(|a, b| a.0.cmp(b.0));
        self.actors.iter_mut().map(|(_, m)| m).chain(props.into_iter().map(|(_, m)| m)).collect()
    }

    /// The files of the game's models; `shaped`: only those the simulation
    /// reads.
    fn files(&self, shaped: bool) -> impl Iterator<Item = &str> {
        self.actors.iter().filter(move |(spec, _)| spec.shaped || !shaped).map(|(spec, _)| spec.file.as_str())
    }
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
    /// The opaque voxels of every model and the title logo's letters, put
    /// on the GPU once (`rebuild_bank`).
    pub bank: std::sync::Arc<ModelBank>,
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

impl<G: Game> Session<G> {
    /// Whether a level change is being shown (no ticks run; the pause menu
    /// waits for it).
    pub fn fading(&self) -> bool {
        self.fade.is_some()
    }

    /// Whether a change the game asked for waits for its fade out: the
    /// game still stands as it was before it (a recorder's check waits).
    pub fn changing(&self) -> bool {
        self.fade.as_ref().is_some_and(|f| f.out)
    }

    /// Does the change the game asked for; every chunk the level before
    /// showed is drawn anew (the next level's chunks are others). A load
    /// reads its slot now (a replay's: the save it loaded then).
    fn change(&mut self) {
        let shown = self.game.sim().world.clone();
        self.game.change();
        self.game.sim_mut().world.mark_changed_since(&shown);
        // A save asked for in on_level_start.
        self.saves_due();
    }

    /// Writes the saves the game made (`world:save`) into their slots.
    fn saves_due(&mut self) {
        let due = self.game.saving();
        if due.is_empty() {
            return;
        }
        for (slot, text) in due {
            if let Err(e) = self.slots.write(slot, &text, self.clock) {
                eprintln!("{e:#}");
                self.problem = Some(format!("{e:#}"));
            }
        }
        self.game.set_slots(self.slots.listing(), self.slots.texts());
    }

    /// Saves the game into `slot` now, between two ticks (SAVE in the
    /// pause menu); not a game stopped by a mistake, half way through.
    pub fn save_to(&mut self, slot: u32) -> Result<()> {
        if let Some(e) = self.game.halted() {
            anyhow::bail!("the game stopped with a mistake, nothing to save: {e}");
        }
        let text = self.game.save()?;
        self.slots.write(slot, &text, self.clock)?;
        self.game.set_slots(self.slots.listing(), self.slots.texts());
        self.pending.push(Event::Save { slot, when: self.clock });
        Ok(())
    }

    /// The run goes on from a save at once, between two ticks: LOAD in the
    /// pause menu (on a game loaded afresh, as its replay plays it), a
    /// replay that began from a save.
    pub fn start_from(&mut self, text: &str) -> Result<()> {
        let shown = self.game.sim().world.clone();
        self.game.start_from(text)?;
        self.game.sim_mut().world.mark_changed_since(&shown);
        self.started_from = Some(text.to_string());
        self.saves_due();
        Ok(())
    }

    /// The run goes on from the scenario `name` at once, before its first
    /// tick (`docs/game-model.md`, "Scenarios and the console").
    pub fn start_scenario(&mut self, name: &str) -> Result<()> {
        let shown = self.game.sim().world.clone();
        let started = self.game.start_scenario(name);
        self.game.sim_mut().world.mark_changed_since(&shown);
        started?;
        self.scenario = Some(name.to_string());
        self.saves_due();
        Ok(())
    }

    /// A console line run between two ticks: what it gave back and
    /// printed. It is part of the run (`evals`: a replay runs it again
    /// after the same step), a mistake in it too.
    pub fn eval(&mut self, code: &str) -> Result<String> {
        let shown = self.game.sim().world.clone();
        let said = self.game.eval(code);
        self.game.sim_mut().world.mark_changed_since(&shown);
        self.evals.push(code.to_string());
        self.pending.push(Event::Eval(code.to_string()));
        self.saves_due();
        said
    }

    /// Whether the run was helped: begun from a scenario, a console line
    /// run in it, files changed in it (APPLY). Its high score goes in as
    /// `HELPED` (no initials asked).
    pub fn helped(&self) -> bool {
        self.scenario.is_some() || !self.evals.is_empty() || self.applied
    }

    /// Files changed while the game runs (APPLY): laid over its files and
    /// taken in one after another (`conf.lua`, the levels, the models, the
    /// code, the rest), the game's state kept as a reload keeps it. Part of
    /// the run, as a console line is (`Event::Apply`: a replay applies them
    /// again after the same step). Only a game of sealed files takes them,
    /// files a replay can carry (`Files::seal`). What was reloaded, the
    /// most far-reaching; a file that would not load is the game's trouble
    /// (`trouble`) as on a reload.
    pub fn apply(&mut self, changes: Arc<Changes>) -> Result<Reloaded> {
        anyhow::ensure!(self.files.is_sealed(), "files are applied to a game of sealed files only (crate::files::Files::seal)");
        self.files = self.files.changed(&changes);
        self.pending.push(Event::Apply(changes.clone()));
        self.applied = true;
        // A run applied to still starts from its files: it is kept.
        let touched = self.touched;
        let mut names: Vec<&String> = changes.keys().collect();
        names.sort_by_key(|name| self.apply_order(name));
        let mut most = Reloaded::Nothing;
        let mut failed = Ok(());
        for name in names {
            let path = self.files.path(name);
            match self.take_in(&path) {
                Ok(r) => most = most.max(r),
                Err(e) => failed = Err(e),
            }
        }
        self.touched = touched;
        failed.map(|()| most)
    }

    /// Where a changed file comes in an APPLY: the config first (it reloads
    /// the models and sounds too), the levels, the models, the code last.
    fn apply_order(&self, name: &str) -> u8 {
        let path = self.files.path(name);
        if G::config_file() == Some(name) {
            0
        } else if name.starts_with("levels/") || self.level_files.contains(&path) || is_block_file(&self.dir, &path) {
            1
        } else if name.ends_with(".vvx") {
            2
        } else if name.ends_with(".lua") {
            3
        } else {
            4
        }
    }

    /// The scenarios a run of this game may begin from: each one's name
    /// and what it is about.
    pub fn scenarios(&mut self) -> Result<Vec<(String, String)>> {
        self.game.scenarios()
    }



    /// Loads the game folder `dir` as a game of kind `G`, its save slots
    /// for this run only.
    pub fn load(dir: &Path) -> Result<Self> {
        Self::load_with(dir, Slots::default())
    }

    /// Loads the game folder `dir` with these save slots (the player's on
    /// disk, `Slots::on_disk`, or a replay's).
    pub fn load_with(dir: &Path, slots: Slots) -> Result<Self> {
        Self::load_from(Files::folder(dir), slots)
    }

    /// Loads the game from its files (`crate::files`: the folder, or a set
    /// sealed) with these save slots.
    pub fn load_from(files: Files, mut slots: Slots) -> Result<Self> {
        let dir = files.dir().to_path_buf();
        let cfg = config_in::<G>(&files)?;
        slots.set_count(cfg.saves.slots);
        let levels = std::sync::Arc::new(crate::levels::load::<G>(&files, &cfg)?);
        let level_files = levels.files.clone();
        let piece_models = levels.piece_models();
        let models = Models::load_for::<G>(&files, &cfg, &piece_models)?;
        let first = &levels.list[0];
        let songs = song_timings(&files, &cfg);
        let game = G::load(Setup { files: &files, cfg: &cfg, world: first.world.clone(), markers: first.markers.clone(), models: &models, levels: levels.clone(), slots: slots.listing(), slot_texts: slots.texts(), replayed: slots.replayed().to_vec(), songs: songs.clone() })?;
        let cue = game.music().map(|c| c.song);
        let sounds = SoundBank::load(&files, &cfg.sounds, &G::sounds(&cfg))?;
        let actions = G::actions(&cfg);
        let song_titles = song_titles(&files, &cfg);
        let cfg_ui = cfg.ui.clone();
        let seats = session_seats(&cfg);
        let mut session = Self {
            dir,
            cfg,
            game,
            models,
            level_files,
            piece_models,
            sounds,
            song_titles,
            songs,
            cue,
            music_changed: true,
            fades: false,
            fade: None,
            flyby_from: 0.0,
            ticked: false,
            song: 0,
            options: Options::default(),
            font: display_font(&files, &cfg_ui),
            bank: Default::default(),
            logo: Vec::new(),
            logo_model: None,
            props_ground: Vec::new(),
            touched: false,
            applied: false,
            controls: Controls::nobody(&actions),
            seats: vec![crate::game::Seat { input: None, controls: Controls::nobody(&actions) }; seats],
            actions,
            pending: Vec::new(),
            played: Tick::default(),
            inputs: Default::default(),
            clock: crate::saves::now(),
            local: Default::default(),
            problem: None,
            drawn_sun: Default::default(),
            slots_at_start: slots.headers(),
            slots,
            started_from: None,
            scenario: None,
            evals: Vec::new(),
            files,
        };
        session.rebuild_bank();
        session.place_props();
        // A save asked for in the first on_level_start.
        session.saves_due();
        Ok(session)
    }

    /// Finds the ground under each of the title's props: its usual height
    /// under them, so a cactus at the edge does not lift them. The title's
    /// world never changes (back to the title the game loads afresh); after
    /// the level or `conf.lua` reloads they are placed again.
    fn place_props(&mut self) {
        let world = &self.game.sim().world;
        self.props_ground = self
            .cfg
            .title_screen
            .props
            .iter()
            .map(|prop| {
                let Some(m) = self.models.props.get(&prop.model) else { return 0 };
                let reach = if prop.rotation % 180 != 0 { IVec2::new(m.size.y, m.size.x) } else { m.size.truncate() } / 2;
                let at = IVec2::new(prop.at[0] as i32, prop.at[1] as i32);
                let mut tops = Vec::new();
                for y in (at.y - reach.y..=at.y + reach.y).step_by(3) {
                    for x in (at.x - reach.x..=at.x + reach.x).step_by(3) {
                        tops.push(world.column_top(x, y));
                    }
                }
                tops.sort_unstable();
                tops.get(tops.len() / 2).copied().unwrap_or(0)
            })
            .collect();
    }

    /// What a run of the game depends on, as one number: the simulation's
    /// part of the config, the game's actions and code, the level's files
    /// and the figures' models. A replay carries it and is not played on
    /// other data; changes to the look, the sound or the title leave it
    /// alone.
    pub fn fingerprint(&self) -> Result<u64> {
        let files = self.fingerprinted()?;
        fingerprint_of(&self.cfg, &G::sections(&self.cfg), &self.actions, &files)
    }

    /// The fingerprint's parts by name, each as one number: two machines
    /// whose games differ learn which file does ("levels/3.toml").
    pub fn fingerprint_files(&self) -> Result<Vec<(String, u64)>> {
        let files = self.fingerprinted()?;
        let parts = fingerprint_parts(&self.cfg, &G::sections(&self.cfg), &self.actions, &files)?;
        Ok(parts.into_iter().map(|(name, bytes)| (name, crate::hash::Stable::default().bytes(&bytes).finish())).collect())
    }

    /// The files the fingerprint reads, by their names.
    fn fingerprinted(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let mut paths: Vec<PathBuf> = self.level_files.clone();
        paths.extend(self.models.files(true).map(|f| self.dir.join(f)));
        paths.sort();
        paths.dedup();
        let mut files = Vec::new();
        for path in paths {
            // Named the same on every system: relative, with forward slashes.
            let name = path.strip_prefix(&self.dir).unwrap_or(&path);
            let name: Vec<String> = name.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            let bytes = self.files.read(&path).with_context(|| format!("reading {}", path.display()))?;
            files.push((name.join("/"), bytes));
        }
        files.extend(self.game.code());
        // A game that reads or picks its music: the songs' beats.
        if self.game.uses_music() {
            let beats: String = self.songs.iter().map(|t| format!("{} {} {}\n", t.name, t.tempo, t.meter)).collect();
            files.push(("music".into(), beats.into_bytes()));
        }
        Ok(files)
    }

    /// Puts the models and the title logo's letters in a new model bank
    /// (after any of them changed: a model, the font, the logo in
    /// `conf.lua`); the renderer uploads it with the next frame.
    fn rebuild_bank(&mut self) {
        let t = &self.cfg.title_screen;
        let letters = crate::ui::logo_letters(&self.logo_lines(), &t.logo_heights, &t.logo_cracks, self.font.as_deref(), &self.cfg.ui);
        let own = t.logo_model.as_ref().and_then(|f| self.models.props.get(f)).map(crate::ui::logo_model_parts);
        let mut bank = ModelBank::default();
        for model in self.models.all_mut() {
            bank.add(model);
        }
        self.logo = letters.iter().map(|line| line.iter().map(|cubes| bank.add_cubes(cubes)).collect()).collect();
        self.logo_model = own.map(|(size, parts)| crate::ui::LogoModel { size, parts: parts.iter().map(|(cubes, at)| (bank.add_cubes(cubes), *at)).collect() });
        self.bank = std::sync::Arc::new(bank);
    }

    /// The logo's lines and their colours: `[title_screen] logo`, else the
    /// game's title.
    fn logo_lines(&self) -> Vec<(String, [u8; 4])> {
        let t = &self.cfg.title_screen;
        let lines: Vec<String> = if t.logo.is_empty() { vec![self.cfg.title.to_uppercase()] } else { t.logo.clone() };
        lines
            .into_iter()
            .enumerate()
            .map(|(i, l)| (l, t.logo_colors.get(i).map_or([240, 230, 204, 255], |c| crate::ui::hex_color(c))))
            .collect()
    }

    /// Applies a file that changed in the game folder, keeping the game running.
    /// A failed reload is kept to show over the game (`problem`) until the
    /// next one works. A game of sealed files is not touched by the folder
    /// (its changes come by `apply`).
    pub fn reload(&mut self, changed: &Path) -> Result<Reloaded> {
        if self.files.is_sealed() {
            return Ok(Reloaded::Nothing);
        }
        // A file gone counts in the sounds' folder only (a sound taken
        // away); elsewhere the game keeps what it read.
        if !changed.exists() && !audio::sound::in_folder(&self.dir, changed) {
            return Ok(Reloaded::Nothing);
        }
        self.take_in(changed)
    }

    /// Takes in a file of the game's files that changed.
    fn take_in(&mut self, changed: &Path) -> Result<Reloaded> {
        let reloaded = self.reload_file(changed);
        match &reloaded {
            Ok(Reloaded::Nothing) => {}
            Ok(_) => self.problem = None,
            Err(e) => self.problem = Some(format!("{e:#}")),
        }
        reloaded
    }

    fn reload_file(&mut self, changed: &Path) -> Result<Reloaded> {
        let name = changed.file_name();
        let is = |file: &str| name == Path::new(file).file_name();
        if let Some(file) = G::config_file().filter(|f| is(f)) {
            let before = self.cfg.audio.music.clone();
            let mut cfg = config_in::<G>(&self.files)?;
            let waiting = self.cfg.restart_only(&cfg);
            if !waiting.is_empty() {
                self.cfg.keep_for_restart(&mut cfg);
                eprintln!("{file}: {} take effect after a restart (F5)", waiting.join(", "));
            }
            self.cfg = cfg;
            self.song_titles = song_titles(&self.files, &self.cfg);
            self.font = display_font(&self.files, &self.cfg.ui);
            self.retime_songs();
            self.music_changed |= self.cfg.audio.music != before;
            self.game.apply_config(&self.cfg)?;
            // As many save slots as the config says now.
            self.slots.set_count(self.cfg.saves.slots);
            self.game.set_slots(self.slots.listing(), self.slots.texts());
            if let Some(world) = G::level(&self.cfg)? {
                self.game.replace_world(world, &[]);
            }
            // Kinds of things may have come, gone or changed their models.
            self.models = Models::load_for::<G>(&self.files, &self.cfg, &self.piece_models)?;
            self.game.reshape(&self.models);
            self.touched = true;
            self.sounds = SoundBank::load(&self.files, &self.cfg.sounds, &G::sounds(&self.cfg))?;
            self.rebuild_bank();
            self.place_props();
            // The camera or sun may have moved, which changes the face set.
            return Ok(Reloaded::World);
        }
        let mask = self.face_mask();
        if self.game.reload_code(&self.files, changed)? {
            if self.game.start_failed() && self.game.halted().is_none() {
                // The start was stopped half way: begin afresh from the
                // files, as F5 does (it may stop again, mended or not).
                let slots = self.slots.clone();
                *self = Self::load_from(self.files.clone(), slots)?;
                return Ok(Reloaded::World);
            }
            self.touched = true;
            // A sun the game moves now (or no more) wants other faces.
            if self.face_mask() != mask {
                return Ok(Reloaded::World);
            }
            return Ok(Reloaded::Code);
        }
        if self.cfg.audio.music.files().iter().chain(self.song_file().as_slice()).any(|f| same_file(&self.dir.join(f), changed)) {
            self.retime_songs();
            self.music_changed = true;
            return Ok(Reloaded::Music);
        }
        if self.cfg.ui.display_font.as_deref().is_some_and(is) {
            self.font = display_font(&self.files, &self.cfg.ui);
            self.rebuild_bank();
            return Ok(Reloaded::Models);
        }
        if audio::sound::in_folder(&self.dir, changed) {
            return self.reload_sounds();
        }
        let block = self.cfg.world.blocks.is_some() && is_block_file(&self.dir, changed);
        // A level file new in `levels/` (not for a world the script builds).
        let ending = crate::levels::ending(&self.cfg);
        let new_level = self.cfg.world.size.is_none() && changed.parent().is_some_and(|d| same_file(d, &self.dir.join("levels"))) && (changed.extension().is_some_and(|e| e == ending) || crate::levels::says_level(&self.files, changed));
        if block || new_level || self.level_files.iter().any(|f| same_file(f, changed)) {
            let levels = std::sync::Arc::new(crate::levels::load::<G>(&self.files, &self.cfg)?);
            self.game.check_levels(&levels)?;
            self.level_files = levels.files.clone();
            // The pieces' models may have come, gone or changed.
            self.piece_models = levels.piece_models();
            self.models = Models::load_for::<G>(&self.files, &self.cfg, &self.piece_models)?;
            self.game.reshape(&self.models);
            self.rebuild_bank();
            self.game.replace_levels(levels);
            self.touched = true;
            self.place_props();
            return Ok(Reloaded::World);
        }
        if self.models.files(false).any(is) || title_models(&self.cfg).any(is) {
            self.models = Models::load_for::<G>(&self.files, &self.cfg, &self.piece_models)?;
            self.game.reshape(&self.models);
            self.touched = true;
            self.rebuild_bank();
            self.place_props();
            return Ok(Reloaded::Models);
        }
        Ok(Reloaded::Nothing)
    }

    /// A file in `sounds/` came, changed or went: the sounds are read
    /// again (a mistake keeps them as they were), and only those that
    /// differ are made anew. A name come or gone is the game's to know
    /// (`Game::sounds_changed`: `level:sound` finds it).
    fn reload_sounds(&mut self) -> Result<Reloaded> {
        let own = audio::sound::read_folder(&self.files)?;
        let names = |own: &BTreeMap<String, audio::GameSound>| own.keys().cloned().collect::<Vec<_>>();
        if names(&own) != names(&self.cfg.sounds) {
            let mut cfg = self.cfg.clone();
            cfg.sounds = own.clone();
            self.game.sounds_changed(&cfg)?;
        }
        let brought = G::sounds(&self.cfg);
        let before = audio::sound::all(&brought, &self.cfg.sounds);
        let now = audio::sound::all(&brought, &own);
        for (name, def, file) in &now {
            if !before.iter().any(|(n, d, _)| n == name && d == def) {
                self.sounds.put(&self.files, name, def).with_context(|| file.clone().unwrap_or_else(|| format!("{name} (built in)")))?;
            }
        }
        for (name, ..) in before.iter().filter(|(n, ..)| !now.iter().any(|(m, ..)| m == n)) {
            self.sounds.put(&self.files, name, &audio::SoundDef::Silent)?;
        }
        self.cfg.sounds = own;
        Ok(Reloaded::Sounds)
    }

    /// The actions the game's input carries besides the directions.
    /// What the game's script printed since last asked.
    pub fn printed(&mut self) -> Vec<String> {
        self.game.printed()
    }

    /// What stops the game: a script's error (its file and line in it),
    /// or a file that would not load on a reload.
    pub fn trouble(&self) -> Option<String> {
        self.game.halted().or_else(|| self.problem.clone())
    }

    pub fn actions(&self) -> &[&'static str] {
        &self.actions
    }

    /// What the player plays with as the game sees it: nobody as it loads,
    /// then the window's player (`player_controls`) or a replay's.
    pub fn controls(&self) -> &Controls {
        &self.controls
    }

    /// What the player plays with from now on: the game sees it in the
    /// next tick (`controls` in Lua) and a recording notes the change. A
    /// window sets its player's before every tick it plays; a replay sets
    /// what it recorded.
    pub fn set_controls(&mut self, controls: Controls) {
        if controls != self.controls {
            self.game.set_controls(&controls);
            self.controls = controls;
        }
    }

    /// The seats after the first (seat 2 first) as the next tick finds them.
    pub fn seats(&self) -> &[crate::game::Seat] {
        &self.seats
    }

    /// Who sits on seat `seat` (2 and on) for the next tick and what they
    /// hold (None: nobody, the seat free); a seat the game does not have
    /// is no mistake, nobody sits there.
    pub fn set_seat(&mut self, seat: usize, input: Option<crate::input::Input>) {
        if let Some(s) = seat.checked_sub(2).and_then(|i| self.seats.get_mut(i)) {
            s.input = input;
        }
    }

    /// What the player on seat `seat` (2 and on) plays with.
    pub fn set_seat_controls(&mut self, seat: usize, controls: Controls) {
        if let Some(s) = seat.checked_sub(2).and_then(|i| self.seats.get_mut(i)) {
            s.controls = controls;
        }
    }

    /// The language the game speaks to the player: theirs of the game's
    /// (`c.languages`), else its first, else English.
    pub fn language(&self) -> String {
        let speaks = &self.cfg.languages;
        self.controls.language.as_ref().filter(|l| speaks.contains(l)).or(speaks.first()).cloned().unwrap_or_else(|| "en".into())
    }

    /// The player's controls: `device`, a pad printing its buttons as
    /// `pad_style` does, the keys as `settings` bind and label them.
    pub fn player_controls(&self, device: crate::controls::Device, pad_style: crate::controls::PadStyle, settings: &Settings) -> Controls {
        Controls::of(&self.actions, device, pad_style, settings)
    }

    /// The keys and gamepad buttons for its actions to start with.
    pub fn default_keys(&self) -> crate::controls::Bindings {
        G::default_keys(&self.cfg)
    }

    pub fn default_pad(&self) -> crate::controls::Bindings {
        G::default_pad(&self.cfg)
    }

    /// Keeps the game's whole state as it stands, to bring it back later
    /// (`restore`): going back in a replay, rollback when playing over a
    /// network, saving. Cheap: the copy shares the world's chunks until one
    /// side writes to them.
    pub fn snapshot(&self) -> Result<Snapshot<G>> {
        Ok(Snapshot {
            kept: self.game.snapshot()?,
            slots: self.slots.clone(),
            controls: self.controls.clone(),
            seats: self.seats.clone(),
            pending: self.pending.clone(),
            played: self.played.clone(),
            inputs: self.inputs,
            clock: self.clock,
            files: self.files.clone(),
        })
    }

    /// Brings a kept state back (as often as wanted); what differs from the
    /// world drawn now is meshed anew. The sounds of the tick it was kept at
    /// are not played again.
    pub fn restore(&mut self, snapshot: &Snapshot<G>) {
        // Files applied since: those of then taken in again first (the
        // code, the levels), then the state on them.
        let mut differing = self.files.differing(&snapshot.files);
        if !differing.is_empty() {
            self.files = snapshot.files.clone();
            differing.sort_by_key(|name| self.apply_order(name));
            let touched = self.touched;
            for name in differing {
                let path = self.files.path(&name);
                if let Err(e) = self.take_in(&path) {
                    eprintln!("restoring {name}: {e:#}");
                }
            }
            self.touched = touched;
        }
        let shown = self.game.sim().world.clone();
        self.game.restore(&snapshot.kept);
        self.game.sim_mut().world.mark_changed_since(&shown);
        self.slots = snapshot.slots.clone();
        self.controls = snapshot.controls.clone();
        self.game.set_controls(&self.controls);
        self.seats = snapshot.seats.clone();
        self.pending = snapshot.pending.clone();
        self.played = snapshot.played.clone();
        (self.inputs, self.clock) = (snapshot.inputs, snapshot.clock);
        self.game.set_local(&self.local);
    }

    /// The world, to build its meshes from.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.game.sim_mut().world
    }

    /// An error when the game stopped (a script's error): tools without a
    /// window end with it.
    pub fn stopped(&self) -> Result<()> {
        match self.game.halted() {
            Some(e) => anyhow::bail!("the game stopped at tick {}: {e}", self.game.sim().tick),
            None => Ok(()),
        }
    }

    /// One tick of the game, and its own song when it picks another. A
    /// change the game asked for at the tick's end (another level, the
    /// run's end) is done right after it, or, where changes fade, between
    /// the fade out and the fade in, none of whose frames is a tick.
    pub fn step(&mut self, input: &crate::input::Input) {
        if let Some(fade) = &mut self.fade {
            self.ticked = false;
            fade.t += crate::sim::DT;
            if fade.t >= fade.length {
                if fade.out {
                    (fade.out, fade.t) = (false, 0.0);
                    self.change();
                } else {
                    self.fade = None;
                }
            }
            return;
        }
        self.ticked = true;
        if !self.seats.is_empty() {
            self.game.set_seats(&self.seats);
        }
        self.played = Tick { events: std::mem::take(&mut self.pending), when: self.clock, controls: self.controls.clone(), input: *input, seats: self.seats.clone() };
        self.played.hash_into(&mut self.inputs);
        self.game.step(input);
        self.saves_due();
        if let Some(why) = self.game.leaving() {
            let length = self.cfg.transition.length(why);
            if self.fades && length > 0.0 {
                self.fade = Some(Fade { out: true, t: 0.0, length, color: self.cfg.transition.rgb() });
            } else {
                self.change();
            }
        }
        let cue = self.game.music().map(|c| c.song);
        if cue != self.cue {
            self.cue = cue;
            self.music_changed = true;
        }
    }

    /// Plays a tick brought whole (`Tick`: over the network, a test): its
    /// events, then the tick with its seats, controls and time. A level
    /// change's fade takes no tick, so a session that fades lets it pass
    /// (`step` while `fading`) before it plays the next.
    pub fn play(&mut self, tick: &Tick) {
        for event in &tick.events {
            match event {
                Event::Save { slot, when } => {
                    self.clock = *when;
                    if let Err(e) = self.save_to(*slot) {
                        eprintln!("saving into slot {slot}: {e:#}");
                    }
                }
                Event::Eval(code) => {
                    let _ = self.eval(code);
                }
                Event::Apply(changes) => {
                    if let Err(e) = self.apply(changes.clone()) {
                        eprintln!("applying files: {e:#}");
                    }
                }
            }
        }
        self.clock = tick.when;
        self.set_controls(tick.controls.clone());
        for (n, seat) in tick.seats.iter().enumerate() {
            self.set_seat(n + 2, seat.input);
            self.set_seat_controls(n + 2, seat.controls.clone());
        }
        self.step(&tick.input);
    }

    /// The last tick played, with what happened before it.
    pub fn played(&self) -> &Tick {
        &self.played
    }

    /// The saves and console lines since the last tick: the next one's
    /// events.
    pub fn pending(&self) -> &[Event] {
        &self.pending
    }

    /// The time saves are stamped with now.
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// The slots hold these saves from now on, for this run only (a guest
    /// of a network game keeps the host's, `docs/multiplayer.md`).
    pub fn adopt_slots(&mut self, texts: BTreeMap<u32, String>) {
        let count = self.slots.count();
        self.slots = Slots::holding(texts);
        self.slots.set_count(count);
        self.game.set_slots(self.slots.listing(), self.slots.texts());
    }

    /// The time saves are stamped with from now on (seconds since 1970):
    /// the window's clock where someone plays, the host's over the network,
    /// so every machine writes the same save.
    pub fn set_clock(&mut self, when: u64) {
        self.clock = when;
    }

    /// The state hash by its parts (`Game::state_sections`) and, last, the
    /// ticks played (`inputs`): where two machines ran apart, and whether
    /// they played the same.
    pub fn sections(&self) -> Vec<(&'static str, u64)> {
        let mut parts = self.game.state_sections();
        parts.push(("inputs", self.inputs.finish()));
        parts
    }

    /// How a run of the game begins, as one number: the world's voxels and
    /// the state's parts (`sections` but the ticks played), taken before
    /// any tick (read-only; the IDE's play compares it after a reload).
    pub fn start_mark(&self) -> u64 {
        let mut h = crate::hash::Stable::default();
        h.u64(self.game.sim().world.content_hash());
        for (name, part) in self.game.state_sections() {
            h.bytes(name.as_bytes()).u64(part);
        }
        h.finish()
    }

    /// What this machine has of the game: its seats and its player's
    /// language (`game::Local`; at one machine all of them, the default).
    pub fn set_local(&mut self, local: crate::game::Local) {
        if local != self.local {
            self.game.set_local(&local);
            self.local = local;
        }
    }

    pub fn local(&self) -> &crate::game::Local {
        &self.local
    }

    /// The lines this machine's players give its high score table: its own
    /// seats', or the team's line (`c.team_score`) at every machine.
    pub fn local_scores(&self) -> Vec<(usize, u32)> {
        let team = self.cfg.team_score;
        self.game.scores().into_iter().filter(|&(seat, _)| team || self.local.has(seat)).collect()
    }

    /// The file of the song that plays.
    pub fn song_file(&self) -> Option<&str> {
        let files = self.cfg.audio.music.files();
        files.get(self.song.min(files.len().saturating_sub(1))).copied()
    }

    /// Switches to the game's `index`th song (from 0); false if there is none.
    pub fn select_song(&mut self, index: usize) -> bool {
        if index >= self.cfg.audio.music.files().len() || index == self.song {
            return false;
        }
        self.song = index;
        self.music_changed = true;
        true
    }

    /// The songs' beats anew (a song or `c.audio.music` changed), told to
    /// the game.
    fn retime_songs(&mut self) {
        let songs = song_timings(&self.files, &self.cfg);
        if songs != self.songs {
            self.game.set_songs(&songs);
            self.songs = songs;
        }
    }

    /// The game's music, ready to play: its own song where it is in it,
    /// else the player's; None without music or when its file has a
    /// mistake (reported).
    pub fn music(&self) -> Option<audio::music::Player> {
        if let Some(cue) = self.game.music() {
            let mut player = self.song(self.cfg.audio.music.files().get(cue.song)?)?;
            player.seek(player.at_beat(cue.beats));
            player.start_at(cue.intensity);
            return Some(player);
        }
        self.song(self.song_file()?)
    }

    /// Whether the game plays a song of its own choice (`music:play`):
    /// the music then keeps time with the game, and holds while it does.
    pub fn steers_music(&self) -> bool {
        self.game.music().is_some()
    }

    /// Hands the music to the mixer: the song anew when it changed, else
    /// the game's own song kept in time with its clock (held while the
    /// game is, `held`: a pause, a fade). Whether a song began.
    pub fn feed_music(&mut self, mixer: &mut Mixer, held: bool) -> bool {
        let began = std::mem::take(&mut self.music_changed);
        if began {
            mixer.set_music(self.music());
        } else if let Some(cue) = self.game.music()
            && let Some(t) = self.songs.get(cue.song)
        {
            mixer.follow_music((cue.beats * 60.0 / t.tempo as f64 * audio::SAMPLE_RATE as f64) as u64);
            mixer.music_intensity(cue.intensity);
        }
        mixer.hold_music(held && self.steers_music());
        began
    }

    /// The game's `index`th song (from 0), ready to play.
    pub fn song_at(&self, index: usize) -> Option<audio::music::Player> {
        self.song(self.cfg.audio.music.files().get(index)?)
    }

    /// The title's song: `[title_screen] music`, else the first song.
    pub fn title_music(&self) -> Option<audio::music::Player> {
        let file = self.cfg.title_screen.music.as_deref().or_else(|| self.cfg.audio.music.files().first().copied())?;
        self.song(file)
    }

    fn song(&self, file: &str) -> Option<audio::music::Player> {
        let path = self.files.path(file);
        let text = self.files.read_to_string(&path).map_err(|e| eprintln!("music: {}: {e}", path.display())).ok()?;
        match audio::music::parse(&text) {
            Ok(song) => Some(audio::music::Player::new(song, self.cfg.audio.music_volume)),
            Err(e) => {
                eprintln!("music: {}: {e:#}", path.display());
                None
            }
        }
    }

    /// Hands the sounds of the last tick to the mixer, placed by where they
    /// happened in the view.
    /// Hands the last tick's sounds to the mixer, `gain` the player's volume.
    pub fn play_sounds(&self, mixer: &mut Mixer, gain: f32) {
        let sim = self.game.sim();
        let listener = sim.cam_target;
        let volume = self.cfg.audio.volume.max(0.0) * gain;
        for event in &sim.sounds {
            if let Some(samples) = self.sounds.get(event.name) {
                // (A fixed height: as wide as a 16:9 window shows.)
                let (gain, pan) = audio::place(event.at, listener, camera::shown_width(&self.cfg.camera, 16.0 / 9.0));
                mixer.play(event.name, samples, gain * event.volume * volume, pan);
            }
        }
    }

    /// The sun of the level being played: its file's (`[light]`), else
    /// `c.light`'s.
    pub fn sun(&self) -> Vec3 {
        self.game.daylight().sun.unwrap_or_else(|| Vec3::from(self.cfg.light.sun)).normalize_or(Vec3::NEG_Z)
    }

    /// The faces the meshes hold: those a view ray or the sun can hit, the
    /// suns of every level (`[light]`) with `c.light`'s, so a level entered
    /// finds its faces there; for a game that moves its sun
    /// (`frame:daylight`), every face a sun above the horizon can.
    pub fn face_mask(&self) -> FaceMask {
        let rays = camera::corner_rays(&self.cfg.camera);
        if self.game.moves_sun() {
            return mesh::face_mask_any_sun(&rays);
        }
        let mut mask = mesh::face_mask(&rays, Vec3::from(self.cfg.light.sun).normalize_or(Vec3::NEG_Z));
        for sun in self.game.suns() {
            let more = mesh::face_mask(&rays, sun.normalize_or(Vec3::NEG_Z));
            for (m, o) in mask.iter_mut().zip(more) {
                *m |= o;
            }
        }
        mask
    }

    /// Where a body's feet are drawn, `alpha` of the way into the tick.
    fn body_origin(&self, body: &Body, alpha: f32) -> Vec3 {
        let feet = between(body.prev, body.pos, alpha);
        if self.cfg.camera.snap { feet.floor() } else { feet }
    }

    /// Where the feet of the body the camera keeps on screen are drawn,
    /// `alpha` of the way into the tick (`Game::focus`; the camera's target
    /// without one).
    pub fn player_origin(&self, alpha: f32) -> Vec3 {
        let sim = self.game.sim();
        let group = self.game.focus_group();
        let Some(first) = group.first() else { return between(sim.cam_prev, sim.cam_target, alpha) };
        let anchor = self.body_origin(first, alpha);
        if group.len() == 1 {
            return anchor;
        }
        // Several players: the middle of their span, the way there from the
        // first's drawn voxel in whole voxels when snapping (so none of them
        // flickers against the view).
        let at: Vec<Vec3> = group.iter().map(|b| between(b.prev, b.pos, alpha)).collect();
        let (lo, hi) = at.iter().fold((at[0], at[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let middle = (lo + hi) * 0.5;
        let way = (middle - between(first.prev, first.pos, alpha)) * Vec3::new(1.0, 1.0, 0.0);
        anchor + if self.cfg.camera.snap { way.round() } else { way }
    }

    /// The point the camera looks at, `alpha` of the way into the tick. The
    /// same everywhere, edges included: the camera never tilts or zooms,
    /// and beyond the edge lies the dark (unless it stops at the stage's
    /// edges, `within_stage`).
    pub fn view_target(&self, alpha: f32) -> Vec3 {
        let sim = self.game.sim();
        let snap = self.cfg.camera.snap;
        let mut target = between(sim.cam_prev, sim.cam_target, alpha);
        if snap {
            // On the ground plane the camera sits on the drawn player's voxel,
            // so the world moves exactly when the player steps. Rounding the
            // two separately makes the player flicker; letting the camera lag
            // behind makes it catch up in hitching steps after every stop.
            // Height stays the camera's own, since it ignores jumps; a side
            // view's camera keeps its height to the drawn feet within its
            // band, in whole voxels: inside the band the view stands still,
            // at its edge it moves with the feet, never rounded apart.
            let feet = self.player_origin(alpha);
            let z = match self.cfg.camera.follow_height {
                Some(band) => feet.z + (target.z - feet.z).floor().max(-band.above).min(band.below),
                None => target.z,
            };
            target = Vec3::new(feet.x, feet.y, z);
        }
        // Looking ahead (a side view leading where the hero runs), in whole
        // voxels when snapping, as the target is.
        let lead = between(sim.lead_prev, sim.lead, alpha);
        target += if snap { lead.round() } else { lead };
        if sim.shake > 0.0 && self.options.shake {
            // At most a body width each way.
            let t = sim.tick as f32;
            let amount = sim.shake * sim.shake * self.cfg.physics.body_width;
            target += Vec3::new((t * 1.7).sin(), (t * 2.3).cos(), 0.0) * amount;
        }
        if snap { target.round() } else { target }
    }

    /// `target` moved so the view shows nothing beyond the stage's sides,
    /// top and bottom (`c.camera.stop_at_edges`): at a level's end the
    /// camera stops and the player walks on towards the screen's edge. A
    /// stage smaller than the view is centred. Seen from above, the ground
    /// the screen shows stays on the stage across and in depth (BunnyFarm's
    /// meadow: the bunny goes out through the farm's gate to its edges).
    fn within_stage(&self, target: Vec3, cam: &crate::config::CameraConfig, aspect: f32) -> Vec3 {
        if !cam.stop_at_edges {
            return target;
        }
        let (lo, hi) = self.game.sim().world.stage_box();
        let fit = |v: f32, lo: f32, hi: f32| if hi - lo < 0.0 { (lo + hi) * 0.5 } else { v.max(lo).min(hi) };
        let mut t = target;
        if let Some((near, far)) = camera::ground_extent(cam, aspect) {
            t.x = fit(t.x, lo.x - near.x, hi.x - far.x);
            t.y = fit(t.y, lo.y - near.y, hi.y - far.y);
        } else {
            let (half, down, up) = camera::view_extent(cam, aspect);
            t.x = fit(t.x, lo.x + half, hi.x - half);
            t.z = fit(t.z, lo.z + down, hi.z - up);
        }
        if cam.snap { t.round() } else { t }
    }

    /// Builds the frame `alpha` of the way from the previous tick to the current one.
    #[cfg(test)]
    pub fn frame(&self, alpha: f32, aspect: f32) -> FrameData {
        self.compose(alpha, aspect, Scene::Play, None)
    }

    /// The title scene's camera: the game's, closer.
    fn title_camera(&self) -> crate::config::CameraConfig {
        let mut cam = self.cfg.camera.clone();
        if let Some(w) = self.cfg.title_screen.view_width {
            cam.show_width(w);
        }
        cam
    }

    /// What the title's camera looks at: `[title_screen] at` on the
    /// ground, at the camera's own height.
    fn title_target(&self) -> Vec3 {
        let at = self.cfg.title_screen.at.unwrap_or(self.cfg.player.spawn);
        Vec3::new(at[0], at[1], self.game.sim().cam_target.z).floor()
    }

    /// Where the title's flyby is `time` seconds in (its skids), if it is
    /// crossing the view now: from the left edge to the right, a little up
    /// the screen, at the gunship's height over the ground.
    pub fn flyby_at(&self, time: f32) -> Option<Vec3> {
        let time = time - self.flyby_from;
        let t = &self.cfg.title_screen;
        t.flyby.as_ref()?;
        let every = t.flyby_every.max(FLYBY_TIME + 1.0);
        let into = (time - 3.0).rem_euclid(every);
        if time < 3.0 || into > FLYBY_TIME {
            return None;
        }
        let target = self.title_target();
        let span = self.title_camera().view_width * 0.9;
        let x = target.x - span + 2.0 * span * into / FLYBY_TIME;
        let y = target.y + span * 0.12;
        let ground = self.game.sim().world.column_top(target.x as i32, y as i32) as f32;
        Some(Vec3::new(x, y, ground + t.flyby_altitude).floor())
    }

    /// The title's sounds for tick `tick`, `time` seconds in: the flyby's
    /// rotor chopping as it crosses, placed where it is.
    pub fn title_sounds(&self, mixer: &mut Mixer, tick: u64, time: f32, gain: f32) {
        let Some(at) = self.flyby_at(time) else { return };
        if !tick.is_multiple_of(5) {
            return;
        }
        if let Some(samples) = self.sounds.get("rotor") {
            let (g, pan) = audio::place(at, self.title_target(), self.title_camera().view_width);
            mixer.play("rotor", samples, g * 0.8 * gain * self.cfg.audio.volume.max(0.0), pan);
        }
    }

    /// A frame of `scene` with `menu` over it when it is open (instead of
    /// the HUD).
    pub fn menu_frame(&self, menu: &Menu, settings: &Settings, alpha: f32, aspect: f32, scene: Scene) -> FrameData {
        let ui = menu.is_open().then(|| {
            let t = &self.cfg.title_screen;
            let logo = self.logo_lines();
            let view = crate::ui::MenuView {
                menu,
                settings,
                logo: &logo,
                drip: t.drip,
                songs: &self.song_titles,
                scored: self.game.scored(),
                font: self.font.as_deref(),
                logo_heights: &t.logo_heights,
                logo_cracks: &t.logo_cracks,
                logo_bank: &self.logo,
                logo_model: self.logo_model.as_ref(),
                logo_bob: self.cfg.title_screen.logo_bob,
                light_menu: self.cfg.title_screen.light_menu,
            };
            crate::ui::menu(&view, &self.cfg.ui, alpha, aspect)
        });
        self.compose(alpha, aspect, scene, ui)
    }

    /// What the game draws into a frame seen as `view` (its projection,
    /// the sun's, the eye), leaving out what lies where `world` is filled;
    /// room for `cubes` to begin with.
    fn drawing<'a>(&'a self, world: &'a crate::world::World, alpha: f32, view: (Mat4, Mat4, Vec3), aspect: f32, cubes: usize) -> Drawing<'a> {
        let (view_proj, light_vp, eye) = view;
        Drawing {
            alpha,
            title: false,
            snap: self.cfg.camera.snap,
            // Grenades, bullets, fire and smoke are a third of a body width
            // thick: a cube of voxels, the way an 8-bit shot is a couple of
            // pixels.
            chunky: (self.cfg.physics.body_width / 3.0).round().max(1.0) as i32,
            view_proj,
            light_vp,
            models: &self.models,
            voxels: Voxels {
                world,
                eye,
                taken: crate::world::CellSet::with_capacity_and_hasher(1 << 16, Default::default()),
                opaque: Vec::with_capacity(cubes),
                see_through: Vec::new(),
            },
            fade: 1.0,
            draws: Vec::new(),
            lights: Vec::new(),
            daylight: None,
            hud: crate::ui::Layout::of(&self.cfg.ui, aspect),
            font: self.font.clone(),
        }
    }

    /// One of the game's models alone (`name`: its name or its file), as
    /// the game draws it, in its light and shading, from its camera's
    /// angle, on a floor of two greys: the model sheet (`voxcade sheet`).
    /// Frame `frame`, facing `look` (quarter turns) turned on by `turn`
    /// (`turning`), a figure of parts posed by `pose`, `view_width` voxels
    /// across the view. None: no such model.
    #[allow(clippy::too_many_arguments)]
    pub fn still(&self, name: &str, frame: usize, look: IVec2, turn: Quat, pose: &dyn Fn(&str) -> PartPose, view_width: f32, aspect: f32) -> Option<FrameData> {
        let m = self.models.named(name)?;
        let size = m.size.as_vec3();
        // An empty world of its own round it: the level is not drawn.
        let across = (size.x.max(size.y) * 2.0).ceil() as i32 + 8;
        let world = crate::world::World::new(IVec3::new(across, across, m.size.z + 8), self.game.sim().world.palette);
        let room = world.size.as_vec3();
        let origin = Vec3::new((across / 2) as f32, (across / 2) as f32, 1.0);
        let target = origin + Vec3::Z * (size.z * 0.5);
        let mut cam = self.cfg.camera.clone();
        cam.show_width(view_width);
        let view_proj = camera::view_proj(&cam, target, aspect);
        let (light_vp, depth_range) = camera::light_view_proj(&cam, target, aspect, self.sun(), room, (Vec3::ZERO, room), self.options.shadow_map);
        let eye = camera::eye(&cam, target, aspect);
        let mut out = self.drawing(&world, 1.0, (view_proj, light_vp, eye), aspect, 512);
        let turns: Vec<PartPose> = m.parts.iter().map(|p| pose(&p.name)).collect();
        out.put_posed(m, frame.min(m.frames.len() - 1), origin, look, turn, &turns, None);
        let floor = (size.x.max(size.y) * 0.5).ceil() as i32 + 3;
        for y in -floor..floor {
            for x in -floor..floor {
                let shade = if (x + y).rem_euclid(2) == 0 { [58, 56, 66, 255] } else { [66, 64, 74, 255] };
                out.cube(origin + Vec3::new(x as f32, y as f32, -1.0), shade, 0.0, 1.0);
            }
        }
        let Drawing { voxels, draws, .. } = out;
        let see_through = voxels.see_through.len();
        let instances = voxels.finish();
        // The game's frame for everything else (its light, sky, glow),
        // without its scene, HUD, lights and blur.
        let mut f = self.compose(1.0, aspect, Scene::Play, Some((Vec::new(), Vec::new(), Mat4::IDENTITY)));
        let haze_start = eye.distance(target);
        (f.view_proj, f.light_vp, f.remember, f.depth_bias, f.eye) = (view_proj, light_vp, false, 0.3 / depth_range, eye);
        (f.haze_start, f.haze_depth) = (haze_start, haze_start * 2.0);
        (f.models, f.instances, f.see_through, f.lights, f.ui) = (draws, instances, see_through, Vec::new(), Vec::new());
        (f.tilt_shift, f.screen_flash, f.fade) = (0.0, 0.0, [0.0; 4]);
        Some(f)
    }

    /// The game's own drawing of the play, as a picture in a window of the
    /// usual shape calls it, without the picture: its things and the
    /// script's `on_draw` (only looking; a mistake there halts the game as
    /// in a window). `check` looks after every tick, so a replay stops
    /// where drawing would stop the game in the window, at a fraction of a
    /// frame's cost (no light, particles or HUD voxels).
    pub fn look(&self) {
        let sim = self.game.sim();
        let (cam, aspect) = (&self.cfg.camera, 16.0 / 9.0);
        let target = self.within_stage(self.view_target(1.0), cam, aspect);
        let sun = self.drawn_sun.get().unwrap_or_else(|| self.sun());
        let (light_vp, _) = camera::light_view_proj(cam, target, aspect, sun, sim.world.size.as_vec3(), sim.world.stage_box(), self.options.shadow_map);
        let view = (camera::view_proj(cam, target, aspect), light_vp, camera::eye(cam, target, aspect));
        let mut out = self.drawing(&sim.world, 1.0, view, aspect, 512);
        self.game.draw(&mut out);
    }

    /// A frame of `scene`; `ui` (the menus) instead of the HUD when given;
    pub fn compose(&self, alpha: f32, aspect: f32, scene: Scene, ui: Option<(Vec<Instance>, Vec<ModelDraw>, Mat4)>) -> FrameData {
        let sim = self.game.sim();
        let world_size = sim.world.size.as_vec3();
        let (cam, target) = match scene {
            Scene::Play => (self.cfg.camera.clone(), self.within_stage(self.view_target(alpha), &self.cfg.camera, aspect)),
            Scene::Title { .. } => (self.title_camera(), self.title_target()),
            Scene::Intro { t, .. } => {
                // Eased, and not on the grid: a glide, not steps.
                let e = t.clamp(0.0, 1.0);
                let e = e * e * (3.0 - 2.0 * e);
                let mut cam = self.title_camera();
                let (from, to) = (camera::shown_width(&cam, aspect), camera::shown_width(&self.cfg.camera, aspect));
                cam.show_width(from + (to - from) * e);
                let end = self.within_stage(self.view_target(alpha), &self.cfg.camera, aspect);
                (cam, between(self.title_target(), end, e))
            }
        };
        let view_proj = camera::view_proj(&cam, target, aspect);
        // The sun the game set last picture, if it sets one: what is drawn
        // is chosen by where its shadows fall.
        let sun = self.drawn_sun.get().unwrap_or_else(|| self.sun());
        let (light_vp, depth_range) =
            camera::light_view_proj(&cam, target, aspect, sun, world_size, sim.world.stage_box(), self.options.shadow_map);

        // A voxel is a 3D pixel: everything moving fills one voxel, on the
        // world's grid when snapping. The first claim on a cell wins, and
        // cells the world fills stay the world's: among the cubes built here
        // by a set of taken cells; for the models on the GPU by drawing
        // order (world, models, cubes: at equal depth the first stays).
        let eye = camera::eye(&cam, target, aspect);
        let mut out = self.drawing(&sim.world, alpha, (view_proj, light_vp, eye), aspect, 512 + sim.particles.len());
        out.title = matches!(scene, Scene::Title { .. });

        // The title's models lie on the ground (`place_props`), and its flyby
        // crosses, rotor turning.
        let title = match scene {
            Scene::Play => None,
            Scene::Title { time } => Some((time, 1.0)),
            Scene::Intro { time, t } => Some((time, 1.0 - t.clamp(0.0, 1.0))),
        };
        if let Some((time, opacity)) = title {
            out.fade = opacity;
            for (prop, &ground) in self.cfg.title_screen.props.iter().zip(&self.props_ground) {
                let Some(m) = self.models.props.get(&prop.model) else { continue };
                let look = prop.look();
                let origin = IVec3::new(prop.at[0] as i32, prop.at[1] as i32, ground).as_vec3();
                out.put_model(m, 0, origin, look, Quat::IDENTITY, None);
            }
            if let (Some(at), Some(m)) = (self.flyby_at(time), self.cfg.title_screen.flyby.as_ref().and_then(|f| self.models.props.get(f))) {
                out.fade = 1.0;
                out.put_model(m, (time * 30.0) as usize % m.frames.len(), at, IVec2::X, Quat::IDENTITY, None);
            }
            out.fade = 1.0;
        }

        // The game's things, then the debris, fire, smoke and sparks.
        self.game.draw(&mut out);
        for q in &sim.particles {
            let at = between(q.prev, q.pos, alpha);
            let (color, glow, opacity) = q.look();
            if q.chunky() {
                out.blob(at, color, glow, opacity);
            } else {
                out.cube(out.corner(at), color, glow, opacity);
            }
        }
        let Drawing { voxels, draws: models, lights: game_lights, daylight, .. } = out;
        // The daylight the game set for this picture, else its level's
        // (`[light]`), else c.light's; the shadows fitted to its sun.
        let level = self.game.daylight();
        let day = daylight.unwrap_or_default();
        let day = Daylight {
            sun: day.sun.or(level.sun),
            sun_color: day.sun_color.or(level.sun_color),
            ambient: day.ambient.or(level.ambient),
            shadow: day.shadow.or(level.shadow),
            sky: day.sky.or(level.sky),
        };
        let light = &self.cfg.light;
        let drawn = day.sun.map(|s| s.normalize_or(Vec3::NEG_Z));
        self.drawn_sun.set(drawn);
        let sun_now = drawn.unwrap_or_else(|| self.sun());
        let (light_vp, depth_range) = if sun_now == sun {
            (light_vp, depth_range)
        } else {
            camera::light_view_proj(&cam, target, aspect, sun_now, world_size, sim.world.stage_box(), self.options.shadow_map)
        };
        let see_through = voxels.see_through.len();
        let instances = voxels.finish();

        // Light from flashes and explosions, and what the game lights (rockets
        // in flight); the ones nearest the middle of the view when there are
        // too many.
        let fx = &self.cfg.effects;
        let mut lights: Vec<Light> = sim
            .flares
            .iter()
            .map(|f| Light { pos: f.pos, radius: f.radius, color: f.color.map(|c| c * f.strength() * fx.lights) })
            .chain(game_lights)
            .collect();
        lights.sort_by(|a, b| a.pos.distance_squared(target).total_cmp(&b.pos.distance_squared(target)));
        lights.truncate(crate::render::MAX_LIGHTS);

        // What stopped the game or a reload, over everything until mended
        // (in place of the HUD).
        let problem = self.game.halted().or_else(|| self.problem.clone());
        let (mut ui, ui_models, ui_view_proj) = ui.unwrap_or_else(|| {
            let ui = &self.cfg.ui;
            let camera = crate::ui::camera(&crate::ui::Layout::of(ui, aspect), ui.slant, ui.depth as f32);
            let mut hud = if problem.is_some() { Vec::new() } else { self.game.hud(ui, alpha, aspect, self.font.as_deref()) };
            // Every game's HUD: faces a neighbour covers would show as
            // seams (thin dark lines through the hearts at some sizes).
            crate::render::hide_inner_faces(&mut hud);
            (hud, Vec::new(), camera)
        });
        if let Some(problem) = problem {
            ui.extend(crate::ui::error_panel(&problem, &self.cfg.ui, aspect));
        }

        let haze_start = eye.distance(target);
        FrameData {
            view_proj,
            light_vp,
            remember: self.game.moves_sun(),
            sun_dir: sun_now,
            sun_color: day.sun_color.unwrap_or(light.sun_color),
            depth_bias: 0.3 / depth_range,
            ambient: day.ambient.unwrap_or(light.ambient),
            shadow: day.shadow.unwrap_or(light.shadow),
            shadow_style: match light.shadow_style {
                ShadowStyle::Smooth => 0.0,
                ShadowStyle::Voxel => 1.0,
                ShadowStyle::VoxelSoft => 2.0,
            },
            sky: day.sky.unwrap_or(light.sky),
            reflection: light.reflection,
            reflection_horizon: light.reflection_horizon,
            metal_sweep: light.metal_sweep,
            time: match scene {
                Scene::Title { time } | Scene::Intro { time, .. } => time,
                Scene::Play => sim.tick as f32 * crate::sim::DT,
            },
            eye,
            haze: fx.haze.clamp(0.0, 1.0),
            haze_color: fx.haze_color,
            haze_start,
            haze_depth: haze_start * 2.0,
            lights,
            glow: fx.glow.max(0.0),
            shadow_map: self.options.shadow_map,
            tilt_shift: if self.options.blur { fx.tilt_shift.clamp(0.0, 1.0) } else { 0.0 },
            screen_flash: if self.options.flashes { sim.screen_flash } else { 0.0 },
            outline: {
                let [r, g, b] = fx.outline_color;
                [r, g, b, fx.outline.max(0.0)]
            },
            fade: self.fade.as_ref().map_or([0.0; 4], Fade::look),
            models,
            ui_models,
            bank: self.bank.clone(),
            instances,
            see_through,
            behind: 0,
            ui,
            ui_view_proj,
        }
    }
}

/// What a game draws into a frame (`Game::draw`): its models from the bank
/// on the GPU, cubes (bullets, grenades, anything small), and lights, on
/// the voxel grid. Models outside the view and the shadow box are left out;
/// where two things share a cell, the first drawn keeps it.
pub struct Drawing<'a> {
    alpha: f32,
    title: bool,
    snap: bool,
    chunky: i32,
    view_proj: Mat4,
    light_vp: Mat4,
    models: &'a Models,
    voxels: Voxels<'a>,
    /// What is drawn becomes see-through on top (the title's props leaving).
    fade: f32,
    draws: Vec<ModelDraw>,
    lights: Vec<Light>,
    /// The daylight the game set for this picture (`frame:daylight`).
    daylight: Option<Daylight>,
    /// The HUD's grid this frame, in UI voxels across and up, and the big
    /// letters' font.
    hud: crate::ui::Layout,
    font: Option<Arc<DisplayFont>>,
}

/// A picture's daylight as the game sets it (`frame:daylight`); what it
/// leaves out is `c.light`'s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Daylight {
    pub sun: Option<Vec3>,
    pub sun_color: Option<[f32; 3]>,
    pub ambient: Option<f32>,
    pub shadow: Option<f32>,
    pub sky: Option<[f32; 3]>,
}

impl Drawing<'_> {
    /// This picture's daylight, as the game sets it.
    pub fn daylight(&mut self, day: Daylight) {
        self.daylight = Some(day);
    }

    /// The HUD's grid this frame: UI voxels across (by the screen's shape)
    /// and up (`c.ui.height`).
    pub fn hud_size(&self) -> (f32, f32) {
        (self.hud.width, self.hud.height)
    }

    /// The game's big letters (`c.ui.display_font`), if it has them.
    pub fn font(&self) -> Option<Arc<DisplayFont>> {
        self.font.clone()
    }

    /// How far into the tick the frame is drawn (0..1).
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// The camera's projection this picture (where a point shows).
    pub fn view_proj(&self) -> Mat4 {
        self.view_proj
    }

    /// Whether moving things are drawn on the voxel grid.
    pub fn snaps(&self) -> bool {
        self.snap
    }

    /// On the title scene (the game's things in it, the player not).
    pub fn title(&self) -> bool {
        self.title
    }

    /// Between where something was a tick ago and where it is now.
    pub fn between(&self, prev: Vec3, pos: Vec3) -> Vec3 {
        between(prev, pos, self.alpha)
    }

    /// On the grid when the camera snaps (`[camera] snap`).
    pub fn snapped(&self, at: Vec3) -> Vec3 {
        if self.snap { at.floor() } else { at }
    }

    /// Where a body's feet are drawn.
    pub fn body_origin(&self, body: &Body) -> Vec3 {
        self.snapped(between(body.prev, body.pos, self.alpha))
    }

    /// Where something moving at `point` is drawn: the minimum corner of the
    /// voxel it fills. On the grid when snapping, gliding otherwise.
    pub fn corner(&self, point: Vec3) -> Vec3 {
        if self.snap { point.floor() } else { point - 0.5 }
    }

    /// How many voxels across the chunky things are (a third of a body).
    pub fn chunky(&self) -> i32 {
        self.chunky
    }

    /// The game's model called `name`, if it has one.
    pub fn model_of(&self, name: &str) -> Option<&VoxModel> {
        self.models.get(name)
    }

    /// The frame of the model `name` for what a figure's body does: its
    /// kind's `sets` (idle, walk, jump), else the model's own layout
    /// (`things::figure_frame`).
    pub fn figure_frame(&self, name: &str, sets: &BTreeMap<String, Frames>, body: &Body) -> usize {
        self.models.get(name).map_or(0, |m| crate::sim::things::figure_frame(sets, body, m.frames.len(), m.size.z as f32))
    }

    /// One frame of the game's model `name` with its feet at `origin`,
    /// turned to `look` (quarter turns; the model's forward is +y),
    /// whitened by `tint`.
    pub fn model(&mut self, name: &str, frame: usize, origin: Vec3, look: IVec2, tint: Option<[u8; 4]>) {
        self.model_turned(name, frame, origin, look, Quat::IDENTITY, tint);
    }

    /// `model`, turned freely besides (the game's choice: a ship banking, a
    /// figure facing a diagonal): `turn` in the model's own directions
    /// (`turning`, `facing`), about `render::turn_centre` (the middle of
    /// the pivot cell, the voxel at half the model's height). Its voxels
    /// leave the grid then, but for quarter turns.
    pub fn model_turned(&mut self, name: &str, frame: usize, origin: Vec3, look: IVec2, turn: Quat, tint: Option<[u8; 4]>) {
        let models = self.models;
        if let Some(m) = models.get(name) {
            self.put_model(m, frame, origin, look, turn, tint);
        }
    }

    /// `model_turned`, a figure of parts posed besides: `pose` turns each
    /// part by its name about its joint (`turning`, in the model's own
    /// directions; identity: as it stands), moving what hangs on it along.
    #[allow(clippy::too_many_arguments)]
    pub fn model_posed(&mut self, name: &str, frame: usize, origin: Vec3, look: IVec2, turn: Quat, pose: &dyn Fn(&str) -> PartPose, tint: Option<[u8; 4]>) {
        let models = self.models;
        if let Some(m) = models.get(name) {
            let turns: Vec<PartPose> = m.parts.iter().map(|p| pose(&p.name)).collect();
            self.put_posed(m, frame, origin, look, turn, &turns, tint);
        }
    }

    /// Frame `frame` of a model on the grid, a figure of parts swinging its
    /// parts with its steps (`swing`, `swinging`; its height `tall`).
    #[allow(clippy::too_many_arguments)]
    pub fn model_swung(&mut self, name: &str, frame: usize, origin: Vec3, look: IVec2, swing: &BTreeMap<String, f32>, body: &Body, tall: f32, tint: Option<[u8; 4]>) {
        let swung = swinging(swing, body, tall);
        self.model_posed(name, frame, origin, look, Quat::IDENTITY, &swung, tint);
    }

    /// A figure as its kind animates it: the frame for what its body does
    /// (`sets`, `things::figure_frame`), its parts swinging with its steps
    /// (`swing`), turned `turn`.
    #[allow(clippy::too_many_arguments)]
    pub fn figure(&mut self, name: &str, sets: &BTreeMap<String, Frames>, swing: &BTreeMap<String, f32>, body: &Body, origin: Vec3, look: IVec2, turn: Quat, tint: Option<[u8; 4]>) {
        let models = self.models;
        let Some(m) = models.get(name) else { return };
        let frame = crate::sim::things::figure_frame(sets, body, m.frames.len(), m.size.z as f32);
        let swung = swinging(swing, body, m.size.z as f32);
        let turns: Vec<PartPose> = m.parts.iter().map(|p| swung(&p.name)).collect();
        self.put_posed(m, frame, origin, look, turn, &turns, tint);
    }

    fn put_model(&mut self, m: &VoxModel, frame: usize, origin: Vec3, look: IVec2, turn: Quat, tint: Option<[u8; 4]>) {
        self.put_posed(m, frame, origin, look, turn, &[], tint);
    }

    #[allow(clippy::too_many_arguments)]
    fn put_posed(&mut self, m: &VoxModel, frame: usize, origin: Vec3, look: IVec2, turn: Quat, pose: &[PartPose], tint: Option<[u8; 4]>) {
        let posed = pose.iter().any(|q| *q != PartPose::REST) || rests_turned(m);
        let (lo, hi) = if turn == Quat::IDENTITY && !posed {
            let reach = (m.size.x.max(m.size.y) / 2 + 1) as f32;
            (origin - Vec3::new(reach, reach, 0.0), origin + Vec3::new(reach, reach, m.size.z as f32))
        } else {
            let (middle, reach) = (origin + Vec3::new(0.5, 0.5, m.size.z as f32 * 0.5), m.size.as_vec3().length() * 0.5 + 1.0);
            (middle - reach, middle + reach)
        };
        if !crate::render::on_screen(&self.view_proj, lo, hi) && !crate::render::on_screen(&self.light_vp, lo, hi) {
            return;
        }
        // A model colour's alpha is how opaque it is (glass); `fade` makes
        // what is drawn see-through on top. Fading, a model is all cubes.
        let fade = self.fade;
        let voxels = &mut self.voxels;
        // A model's glow is a byte (0 to 255); half a step over it keeps
        // the cube's rounding down from losing one.
        let mut put = |corner: Vec3, color: [u8; 4], metal: u8, glow: u8, shade: [i8; 4], turn: [i8; 4], covered: u8| {
            let glow = if glow == 0 { 0.0 } else { (glow as f32 + 0.5) / 255.0 };
            voxels.add(corner, color, glow, color[3] as f32 / 255.0 * fade, metal, shade, turn, covered)
        };
        draw(&mut put, (fade >= 1.0).then_some(&mut self.draws), m, frame, origin, look, turn, pose, tint);
    }

    /// A cube with its minimum corner at `corner` (see `corner`): `glow`
    /// and `opacity` 0..1.
    pub fn cube(&mut self, corner: Vec3, color: [u8; 4], glow: f32, opacity: f32) {
        self.voxels.add(corner, color, glow, opacity, 0, [0; 4], [0; 4], 0);
    }

    /// A chunky cube of voxels (`chunky` across) around `centre`.
    pub fn blob(&mut self, centre: Vec3, color: [u8; 4], glow: f32, opacity: f32) {
        let chunky = self.chunky;
        let corner = self.corner(centre - (chunky - 1) as f32 * 0.5);
        for z in 0..chunky {
            for y in 0..chunky {
                for x in 0..chunky {
                    self.voxels.add(corner + IVec3::new(x, y, z).as_vec3(), color, glow, opacity, 0, [0; 4], [0; 4], 0);
                }
            }
        }
    }

    /// A light lighting what is around it this frame.
    pub fn light(&mut self, light: Light) {
        self.lights.push(light);
    }
}

/// The cubes of one frame: opaque ones, and see-through ones that are drawn
/// after them, far to near.
struct Voxels<'a> {
    world: &'a crate::world::World,
    eye: Vec3,
    taken: crate::world::CellSet,
    opaque: Vec<Instance>,
    see_through: Vec<(f32, Instance)>,
}

impl Voxels<'_> {
    /// Claims the voxel with its minimum corner at `corner`, unless the
    /// world or something drawn earlier fills it; one turned freely
    /// (`turn`, `render::snorm_turn`) lies on no cell and claims none.
    /// An opaque cube leaves out the faces `covered` names (a model's own
    /// voxels lying against them, `vox::covered`); a see-through one keeps
    /// them, for `render::join_see_through`'s skins.
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, corner: Vec3, color: [u8; 4], glow: f32, opacity: f32, metal: u8, shade: [i8; 4], turn: [i8; 4], covered: u8) {
        if turn == [0; 4] {
            let cell = (corner + 0.5).floor().as_ivec3();
            if self.world.get(cell) != 0 || !self.taken.insert(cell) {
                return;
            }
        }
        let centre = corner + 0.5;
        let opaque = opacity >= 0.99;
        let fx = [(glow.clamp(0.0, 1.0) * 255.0) as u8, (opacity.clamp(0.0, 1.0) * 255.0) as u8, if opaque { covered } else { 0 }, metal];
        let instance = Instance { pos_size: centre.extend(1.0).to_array(), color, fx, shade, turn };
        if !opaque {
            self.see_through.push((centre.distance_squared(self.eye), instance));
        } else {
            self.opaque.push(instance);
        }
    }

    fn finish(mut self) -> Vec<Instance> {
        self.see_through.sort_by(|a, b| b.0.total_cmp(&a.0));
        let from = self.opaque.len();
        self.opaque.extend(self.see_through.into_iter().map(|(_, i)| i));
        // Water and glass are one body: the faces between their cubes go,
        // but for the one towards the eye, a skin a layer.
        let cubes = &mut self.opaque[from..];
        if !cubes.is_empty() {
            let middle = cubes.iter().map(|i| Vec3::from_slice(&i.pos_size[..3])).sum::<Vec3>() / cubes.len() as f32;
            crate::render::join_see_through(cubes, self.eye - middle);
        }
        self.opaque
    }
}

/// The quarter turn that faces a model's forward (+y) along `look`, as a
/// rotation (clockwise seen from above, as `render::quarter` turns).
fn look_turn(look: IVec2) -> Quat {
    Quat::from_rotation_z(-(look.x as f32).atan2(look.y as f32))
}

/// A free turn in a model's own directions (forward +y, right +x, up +z),
/// in degrees: `turn` round clockwise seen from above (90 faces +x the way
/// a quarter turn does), `pitch` the nose up, `roll` the right side down (banking right:
/// its top leans right). They are taken roll first, then pitch, then the
/// turn round: the rotation `Rz(-turn) · Rx(pitch) · Ry(roll)`, so the turn
/// round stays the bearing seen from above whatever the other two (a
/// level's piece and `frame:model` write it `rotation`, `pitch`, `roll`,
/// `grid::Thing`).
/// All 0 is no turn at all.
pub fn turning(turn: f32, pitch: f32, roll: f32) -> Quat {
    if turn == 0.0 && pitch == 0.0 && roll == 0.0 {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_z(-turn.to_radians()) * Quat::from_rotation_x(pitch.to_radians()) * Quat::from_rotation_y(roll.to_radians())
}

/// A kind's spin (`spin`, degrees a second round its upright axis) after
/// `ticks` of the game (between two ticks too): only the picture turns,
/// every thing of the kind alike.
pub fn spinning(spin: f32, ticks: f32) -> Quat {
    if spin == 0.0 { Quat::IDENTITY } else { turning((spin * ticks * crate::sim::DT).rem_euclid(360.0), 0.0, 0.0) }
}

/// The turn that shows a body's model facing exactly its way on the ground
/// while its quarter turn is its `look` (a figure drawn at its diagonal),
/// for a kind that turns freely (`free`, `rotates = "free"`): none when it
/// does not, or faces straight along `look`.
pub fn facing(free: bool, body: &Body) -> Quat {
    let (facing, look) = (body.facing, body.look);
    if !free {
        return Quat::IDENTITY;
    }
    let way = facing.x.atan2(facing.y) - (look.x as f32).atan2(look.y as f32);
    let way = (way + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    if facing.truncate() == Vec2::ZERO || way.abs() < 1e-4 { Quat::IDENTITY } else { Quat::from_rotation_z(-way) }
}

/// A figure's parts turned as they swing with its steps: each part named
/// in `swing` pitched forward and back by its degrees (forward first; less
/// than 0: back first), a stride every half its height (`tall`) walked;
/// standing, as it stands.
pub fn swinging<'a>(swing: &'a BTreeMap<String, f32>, body: &Body, tall: f32) -> impl Fn(&str) -> PartPose + 'a {
    let phase = crate::sim::math::sin(std::f32::consts::PI * body.walked / (tall * 0.5).max(1.0));
    move |part| swing.get(part).map_or(PartPose::REST, |&degrees| PartPose::turned(turning(0.0, degrees * phase, 0.0)))
}

/// How a figure's part is posed: turned about its joint (on its rest
/// rotation), and moved by less than a voxel (`offset`, on its own), with
/// what hangs on it. Only the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartPose {
    pub turn: Quat,
    pub offset: Vec3,
}

impl PartPose {
    pub const REST: Self = Self { turn: Quat::IDENTITY, offset: Vec3::ZERO };

    pub fn turned(turn: Quat) -> Self {
        Self { turn, ..Self::REST }
    }
}

/// A script's pose entry: a part by its name, degrees round, nose up,
/// rolled (`turning`), and its offset in voxels.
pub type PoseEntry = (String, [f32; 3], [f32; 3]);

/// A script's pose (each part by its name: degrees round, nose up, rolled,
/// `turning`, an offset) over `under` (the kind's swing).
pub fn posing<'a>(pose: &'a [PoseEntry], under: impl Fn(&str) -> PartPose + 'a) -> impl Fn(&str) -> PartPose + 'a {
    move |part| {
        let under = under(part);
        match pose.iter().find(|(p, ..)| p == part) {
            Some((_, [round, pitch, roll], offset)) => PartPose { turn: turning(*round, *pitch, *roll) * under.turn, offset: Vec3::from(*offset) + under.offset },
            None => under,
        }
    }
}

/// A script's pose alone (a level's piece, `thing.state.parts`), no swing
/// under it.
pub fn posing_alone(pose: &[PoseEntry]) -> impl Fn(&str) -> PartPose + '_ {
    posing(pose, |_| PartPose::REST)
}

/// What `draw` hands over for each voxel it draws as a cube: its corner,
/// colour, metal, glow, the way it is lit and its free turn.
type Put<'a> = dyn FnMut(Vec3, [u8; 4], u8, u8, [i8; 4], [i8; 4], u8) + 'a;

/// What `draw` puts down at once: its voxels in the bank (if there), the
/// see-through ones and all the visible ones (indices into the frame), its
/// turn and its shift (`render::turned_centre`).
type Piece<'a> = (Option<std::ops::Range<u32>>, &'a [u32], &'a [u32], &'a [u8], Quat, Vec3, f32);

/// Draws one frame of a model with its feet at `origin`, turned in quarter
/// turns (model forward is +y) around a whole-voxel pivot, and freely by
/// `free_turn` in its own directions (identity: on the grid) about
/// `render::turn_centre` (that pivot's middle, the voxel at half its
/// height); a figure of parts each part posed by `pose`
/// about its joint (in its own directions, carrying what hangs on it;
/// empty: as it stands). Its opaque voxels come from the model bank on the
/// GPU, one entry (a part) in `on_gpu`; the see-through ones are cubes
/// (sorted with the frame's other see-through ones), and all of them
/// without `on_gpu` or a bank (a model fading out).
#[allow(clippy::too_many_arguments)]
fn draw(put: &mut Put, on_gpu: Option<&mut Vec<ModelDraw>>, model: &VoxModel, frame: usize, origin: Vec3, look: IVec2, free_turn: Quat, pose: &[PartPose], tint: Option<[u8; 4]>) {
    draw_placed(put, on_gpu, model, frame, origin, look, free_turn, pose, tint, false);
}

/// `draw`; `as_drawn` places the cubes of what is turned freely by the
/// turn the shader turns them by (`render::turn_as_drawn`), from the
/// voxel it turns about, so that neighbours meet corner to corner: a model
/// drawn all as cubes (the editors' pieces). Placed by the exact turn, as
/// the bank's voxels are, each cube was turned by the packed turn about a
/// middle the exact one put it at, and neighbours were apart or overlapped
/// by a hundredth of a voxel: single pixels of what lies behind showed on
/// its faces. Without it the cubes lie where the bank's voxels lie (the
/// glass beside them).
#[allow(clippy::too_many_arguments)]
fn draw_placed(
    put: &mut Put,
    mut on_gpu: Option<&mut Vec<ModelDraw>>,
    model: &VoxModel,
    frame: usize,
    origin: Vec3,
    look: IVec2,
    free_turn: Quat,
    pose: &[PartPose],
    tint: Option<[u8; 4]>,
    as_drawn: bool,
) {
    // The free turn as the world sees it, after the quarter turns.
    let quarter_turn = look_turn(look);
    let rot = if free_turn == Quat::IDENTITY { free_turn } else { quarter_turn * free_turn * quarter_turn.inverse() };
    let pivot = IVec3::new(model.size.x / 2, model.size.y / 2, 0);
    let frame = if model.parts.is_empty() { frame.min(model.frames.len() - 1) } else { 0 };
    let metal = model.metal.get(frame);
    let shade = model.shade.get(frame);
    let facets = model.facets.get(frame);
    let glow = model.glow.get(frame);
    let turn = match (look.x, look.y) {
        (0, 1) => 0,
        (1, 0) => 1,
        (0, -1) => 2,
        _ => 3,
    };
    let about = crate::render::turn_centre(model.size);
    // What is drawn: the frame whole, or each part with its own turn and
    // shift (its chain of turns about the joints, in the model's own
    // directions, seen after the quarter turns and the free turn).
    let mut pieces: Vec<Piece> = Vec::new();
    if model.parts.is_empty() {
        pieces.push((model.bank.get(frame).cloned(), &model.glass[frame], &model.visible[frame], model.covered.get(frame).map_or(&[][..], |c| &c[..]), rot, crate::render::turn_about(rot, about), 0.0));
    } else {
        let mut chain: Vec<Option<(Quat, Vec3)>> = vec![None; model.parts.len()];
        let centre = Vec3::new(0.5, 0.5, 0.0);
        // Posed (or a part turned at rest), a part's depth is taken nearer
        // by how far down the figure it hangs (`ModelDraw::nearer`); at
        // rest nothing overlaps.
        let posed = pose.iter().any(|q| *q != PartPose::REST) || rests_turned(model);
        let steps = |mut i: usize| {
            let mut n = 0.0;
            while let Some(p) = model.parts[i].parent.filter(|_| n < 16.0) {
                (i, n) = (p, n + 1.0);
            }
            n
        };
        for i in 0..model.parts.len() {
            let (own, b) = part_chain(model, pose, i, &mut chain);
            let seen = quarter_turn * own * quarter_turn.inverse();
            let shift = about + rot * (-(seen * centre) + quarter_turn * (own * centre) + quarter_turn * (b - centre) + centre - about);
            let p = &model.parts[i];
            let bank = (!model.bank.is_empty()).then(|| p.bank.clone());
            pieces.push((bank, &p.glass, &p.visible, &p.covered, rot * seen, shift, if posed { steps(i) } else { 0.0 }));
        }
    }
    // What each set of lit faces (bits in mesh::DIRS order) becomes after
    // the turn, once for all the voxels.
    let dirs = crate::mesh::DIRS;
    let to: [usize; 6] = std::array::from_fn(|f| {
        let (x, y) = crate::render::quarter(dirs[f].x, dirs[f].y, turn);
        dirs.iter().position(|e| *e == IVec3::new(x, y, dirs[f].z)).unwrap_or(f)
    });
    let turned_faces: [i8; 64] = std::array::from_fn(|mask| (0..6).filter(|f| mask >> f & 1 == 1).fold(0, |bits, f| bits | 1 << to[f]));
    let voxels = &model.frames[frame];
    for (bank, glass, visible, covered, rot, shift, nearer) in pieces {
        // As cubes the visible voxels carry the faces their own neighbours
        // cover, as the bank's do (`vox::covered`); glass carries none.
        let (as_cubes, covered) = match (on_gpu.as_deref_mut(), bank) {
            (Some(on_gpu), Some(voxels)) => {
                on_gpu.push(ModelDraw { voxels, origin, turn, rot, shift, tint: tint.map(|[r, g, b, _]| [r, g, b]), nearer });
                (glass, &[][..])
            }
            _ => (visible, covered),
        };
        let free = crate::render::off_grid(rot, shift);
        let snorm = crate::render::snorm_turn(rot);
        // As drawn: from the voxel it turns about, at its exact place, by the
        // packed turn (`draw_placed`).
        let middle = about - 0.5;
        let placed = (as_drawn && snorm != [0; 4]).then(|| (crate::render::turned_centre(origin, middle, shift, rot), crate::render::turn_as_drawn(rot)));
        for (i, &k) in as_cubes.iter().enumerate() {
            // in the world's directions after the quarter turns, as a cube's faces are counted
            let hidden = covered.get(i).map_or(0, |&c| turned_faces[(c & 63) as usize] as u8);
            let k = k as usize;
            let (v, color) = &voxels[k];
            let l = *v - pivot;
            let (x, y) = crate::render::quarter(l.x, l.y, turn);
            let mut lit = shade.and_then(|s| s.get(k)).map_or([0; 4], |&[nx, ny, nz, faces]| {
                let (nx, ny) = crate::render::quarter(nx as i32, ny as i32, turn);
                [nx as i8, ny as i8, nz, turned_faces[(faces & 63) as usize]]
            });
            // Smooth glass mirrors and glints by its facet's way (lit by its
            // faces' own, as glass is).
            if let Some(&[nx, ny, nz]) = facets.and_then(|f| f.get(k)).filter(|f| **f != [0; 3]) {
                let (nx, ny) = crate::render::quarter(nx as i32, ny as i32, turn);
                lit = [nx as i8, ny as i8, nz, 0];
            }
            // A flash whitens, glass stays glass.
            let [r, g, b, _] = tint.unwrap_or(*color);
            let local = IVec3::new(x, y, l.z).as_vec3();
            let corner = if let Some((at, drawn)) = placed {
                at + drawn * (local - middle) - 0.5
            } else if free {
                crate::render::turned_centre(origin, local, shift, rot) - 0.5
            } else {
                origin + IVec3::new(x, y, l.z).as_vec3()
            };
            put(corner, [r, g, b, color[3]], metal.and_then(|m| m.get(k)).copied().unwrap_or(0), glow.and_then(|g| g.get(k)).copied().unwrap_or(0), lit, snorm, hidden);
        }
    }
}

/// The cells a model's first frame fills (every voxel, glass too: a
/// figure's parts as they stand), facing `look` in quarter turns, from its
/// pivot's cell: exactly where `draw` puts its voxels on the grid. What a
/// solid piece holds up (`world::Solids`).
pub fn quarter_cells(model: &VoxModel, look: IVec2) -> Vec<IVec3> {
    let bearing = match (look.x, look.y) {
        (0, 1) => 0,
        (1, 0) => 90,
        (0, -1) => 180,
        _ => 270,
    };
    piece_cells(model, [bearing, 0, 0]).expect("quarter turns")
}

/// A turn of whole quarter turns as a matrix of whole numbers: `turning`'s
/// `Rz(-turn) · Rx(pitch) · Ry(roll)` for `[turn, pitch, roll]` in degrees,
/// each a multiple of 90 (None if one is not).
pub fn quarter_matrix([turn, pitch, roll]: [i32; 3]) -> Option<[[i32; 3]; 3]> {
    if [turn, pitch, roll].iter().any(|d| d.rem_euclid(90) != 0) {
        return None;
    }
    // The cosine and sine of a quarter turn by its count.
    let cs = |d: i32| [(1, 0), (0, 1), (-1, 0), (0, -1)][(d.rem_euclid(360) / 90) as usize];
    let mul = |a: [[i32; 3]; 3], b: [[i32; 3]; 3]| std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()));
    let (zc, zs) = cs(-turn);
    let (xc, xs) = cs(pitch);
    let (yc, ys) = cs(roll);
    let z = [[zc, -zs, 0], [zs, zc, 0], [0, 0, 1]];
    let x = [[1, 0, 0], [0, xc, -xs], [0, xs, xc]];
    let y = [[yc, 0, ys], [0, 1, 0], [-ys, 0, yc]];
    Some(mul(mul(z, x), y))
}

/// The cells a model's first frame fills (every voxel, glass too: a
/// figure's parts as they stand), turned by `[turn, pitch, roll]` in
/// whole quarter turns (`turning`'s words and order; turn a bearing,
/// clockwise seen from above, 0 the model's own forward), from its pivot's
/// cell: exactly where `draw` puts its voxels, which lie on the grid as
/// they turn about a voxel's middle (`render::turn_centre`). What a solid
/// piece holds up (`world::Solids`). None if a turn is off the quarters.
pub fn piece_cells(model: &VoxModel, turns: [i32; 3]) -> Option<Vec<IVec3>> {
    let m = quarter_matrix(turns)?;
    let pivot = IVec3::new(model.size.x / 2, model.size.y / 2, 0);
    let h = model.size.z / 2;
    let mut cells: Vec<IVec3> = model.frames.first().map_or(Vec::new(), |f| {
        f.iter()
            .map(|(v, _)| {
                // The voxel's middle from the turn's centre is whole: x and
                // y from the pivot cell's middle, z from the voxel at half
                // the height.
                let l = *v - pivot;
                let d = [l.x, l.y, l.z - h];
                let r: [i32; 3] = std::array::from_fn(|i| (0..3).map(|k| m[i][k] * d[k]).sum());
                IVec3::new(r[0], r[1], r[2] + h)
            })
            .collect()
    });
    cells.sort_unstable_by_key(|c| (c.z, c.y, c.x));
    cells.dedup();
    Some(cells)
}

/// A model's frame as cubes, every voxel that shows, as a game draws it
/// with its feet at `origin` facing `look` and turned `turn` freely after
/// (`Drawing::model_turned`): the opaque ones into `opaque`, the glass into
/// `glass`. The level editor's pieces, so they look as in the game; what
/// is turned freely placed as drawn (`draw_placed`), neighbours meeting.
pub fn model_cubes(model: &VoxModel, frame: usize, origin: Vec3, look: IVec2, turn: Quat, opaque: &mut Vec<Instance>, glass: &mut Vec<Instance>) {
    let mut put = |corner: Vec3, color: [u8; 4], metal: u8, glow: u8, shade: [i8; 4], turn: [i8; 4], covered: u8| {
        let covered = if color[3] < 255 { 0 } else { covered };
        let instance = Instance { pos_size: (corner + 0.5).extend(1.0).to_array(), color, fx: [glow, color[3], covered, metal], shade, turn };
        if color[3] < 255 { glass.push(instance) } else { opaque.push(instance) }
    };
    draw_placed(&mut put, None, model, frame, origin, look, turn, &[], None, true);
}

/// A model as a game draws it alone (the editors' GAME VIEW): with its
/// feet at `origin` facing `look`, a figure posed by `pose` (a turn per
/// part, on its rest), its opaque voxels from the model bank its
/// `VoxModel::bank` was filled into (`draws`: outlined and shaded as the
/// game's), its glass as cubes.
pub fn model_drawn(model: &VoxModel, origin: Vec3, look: IVec2, pose: &[PartPose], draws: &mut Vec<ModelDraw>, glass: &mut Vec<Instance>) {
    let mut put = |corner: Vec3, color: [u8; 4], metal: u8, glow: u8, shade: [i8; 4], turn: [i8; 4], covered: u8| {
        let covered = if color[3] < 255 { 0 } else { covered };
        glass.push(Instance { pos_size: (corner + 0.5).extend(1.0).to_array(), color, fx: [glow, color[3], covered, metal], shade, turn });
    };
    draw(&mut put, Some(draws), model, 0, origin, look, Quat::IDENTITY, pose, None);
}

/// Where part `i` of a figure stands posed by `pose` (a turn per part, on
/// its rest rotation): its turn and shift, as `part_at_rest` gives them.
pub fn part_posed(model: &VoxModel, pose: &[PartPose], i: usize) -> (Quat, Vec3) {
    let mut chain = vec![None; model.parts.len()];
    part_chain(model, pose, i, &mut chain)
}

/// Where part `i` of a figure stands, posed: the turn and the shift that
/// take a voxel's middle (from the pivot cell's minimum corner, in the
/// model's own directions) to where it is, each part turned about its
/// joint and carried by the part it hangs on.
fn part_chain(model: &VoxModel, pose: &[PartPose], i: usize, chain: &mut [Option<(Quat, Vec3)>]) -> (Quat, Vec3) {
    if let Some(done) = chain[i] {
        return done;
    }
    let part = &model.parts[i];
    // The pose turns on from the part's rest rotation, about the same
    // joint: `pose · rest` (the rest first).
    let posed = pose.get(i).copied().unwrap_or(PartPose::REST);
    let own = match rest_turn(part) {
        Some(rest) => posed.turn * rest,
        None => posed.turn,
    };
    // Turned about its joint, then moved by its offset (the file's and
    // the pose's), in its parent's directions: the joint goes along, and
    // what hangs on it.
    let offset = part.offset + posed.offset;
    let joint = part.joint;
    let done = match part.parent {
        None => (own, joint - own * joint + offset),
        Some(p) => {
            let (turn, shift) = part_chain(model, pose, p, chain);
            let turned = turn * own;
            (turned, turn * (joint + offset) + shift - turned * joint)
        }
    };
    chain[i] = Some(done);
    done
}

/// A part's rest rotation (`vox::Part::rest`) as a turn in the model's own
/// directions about its joint (`turning`), none where it has none.
pub fn rest_turn(part: &crate::vox::Part) -> Option<Quat> {
    let [round, pitch, roll] = part.rest;
    (part.rest != [0.0; 3]).then(|| turning(round, pitch, roll))
}

/// Where part `i` of a figure stands at rest (no pose): its turn and its
/// shift in the model's own directions, its parents' rest rotations and
/// its own about their joints: a point `p` of it, measured from the
/// pivot's low corner as `Part::joint` is (a voxel's middle half a voxel
/// past its corner), lies at `turn * p + shift`. What `draw` turns a
/// figure's part by, for the editors' own drawing of a figure.
pub fn part_at_rest(model: &VoxModel, i: usize) -> (Quat, Vec3) {
    let mut chain = vec![None; model.parts.len()];
    part_chain(model, &[], i, &mut chain)
}

/// Whether a figure has a part turned at rest (`vox::Part::rest`).
pub fn rests_turned(model: &VoxModel) -> bool {
    model.parts.iter().any(|p| p.rest != [0.0; 3] || p.offset != Vec3::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shooter::Controls as Input;
    use crate::shooter::Shooter;
    use std::collections::HashSet;

    /// A game on open flat ground, without model files.
    fn flat_game(size: i32) -> Session<Shooter> {
        game_on(crate::sim::tests::flat_world(IVec3::new(size, size, 16)), GameConfig::default())
    }

    /// A game in `world` with `cfg`, without model files.
    fn game_on(world: World, cfg: GameConfig) -> Session<Shooter> {
        let player = VoxModel::new(IVec3::new(5, 4, 10), vec![Vec::new()], vec![Vec::new()]);
        let models = Models { actors: vec![(ModelSpec { name: "player".into(), file: String::new(), shaped: true }, player)], props: HashMap::new() };
        let game = Shooter::new(world, &cfg, crate::shooter::tests::shapes(), &[]);
        Session {
            dir: PathBuf::new(),
            files: Files::disk(),
            cfg,
            game,
            models,
            level_files: Vec::new(),
            piece_models: Vec::new(),
            fades: false,
            fade: None,
            flyby_from: 0.0,
            ticked: false,
            sounds: SoundBank::default(),
            song_titles: Vec::new(),
            songs: Vec::new(),
            cue: None,
            music_changed: false,
            song: 0,
            options: Options::default(),
            font: None,
            bank: Default::default(),
            logo: Vec::new(),
            logo_model: None,
            props_ground: Vec::new(),
            actions: crate::shooter::ACTIONS.to_vec(),
            controls: Controls::nobody(&crate::shooter::ACTIONS),
            seats: Vec::new(),
            pending: Vec::new(),
            played: Tick::default(),
            inputs: Default::default(),
            clock: 0,
            local: Default::default(),
            problem: None,
            drawn_sun: Default::default(),
            touched: false,
            applied: false,
            slots: Slots::default(),
            slots_at_start: BTreeMap::new(),
            started_from: None,
            scenario: None,
            evals: Vec::new(),
        }
    }

    /// While running steadily the player must keep its place on screen: the
    /// drawn player and the camera move in the same whole-voxel steps. If they
    /// are rounded separately, the player flickers between two spots.
    #[test]
    fn running_player_keeps_still_on_screen() {
        for run in [Input { left: true, ..Input::default() }, Input { up: true, left: true, ..Input::default() }] {
            let mut game = flat_game(512);
            let mut offsets = Vec::new();
            for tick in 0..150 {
                game.game.step(&run);
                if tick < 60 {
                    continue; // let speed and camera lag settle
                }
                for alpha in [0.0, 0.3, 0.6, 0.9] {
                    let offset = game.player_origin(alpha) - game.view_target(alpha);
                    offsets.push((offset * 1000.0).round().as_ivec3());
                }
            }
            let changes = offsets.windows(2).filter(|pair| pair[0] != pair[1]).count();
            println!("{run:?}: player jumps against the camera {changes} times in {} frames", offsets.len());
            assert!(changes <= 2, "player shivers against the camera {changes} times while running");
        }
    }

    /// Every game's HUD is drawn without the faces a neighbour covers:
    /// they showed as thin dark lines through the platformer's hearts at
    /// some window sizes (1440 by 810 among them).
    #[test]
    fn a_huds_inner_faces_are_hidden() {
        let game = Session::<crate::Voxcade>::load(Path::new("games/voxkeep")).unwrap();
        let frame = game.compose(0.0, 1440.0 / 810.0, Scene::Play, None);
        let cell = |i: &Instance| Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
        let taken: HashSet<IVec3> = frame.ui.iter().map(cell).collect();
        let next_to = frame.ui.iter().filter(|i| taken.contains(&(cell(i) + IVec3::X))).collect::<Vec<_>>();
        assert!(!next_to.is_empty(), "the hearts have voxels side by side");
        assert!(next_to.iter().all(|i| i.fx[2] & 1 == 1), "their +x faces hidden");
    }

    /// The line round the models is the game's: none unless it sets
    /// `c.effects.outline` (voxels thick) and, if it likes, its colour.
    #[test]
    fn the_outline_round_models_is_the_games() {
        let mut game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
        assert_eq!(game.frame(1.0, 16.0 / 9.0).outline[3], 0.0, "none by default");
        (game.cfg.effects.outline, game.cfg.effects.outline_color) = (1.0, [0.1, 0.2, 0.3]);
        assert_eq!(game.frame(1.0, 16.0 / 9.0).outline, [0.1, 0.2, 0.3, 1.0]);
    }

    /// A still (the model sheet) draws the one model alone, by its name or
    /// its file, from the side asked, on its floor; no other model.
    #[test]
    fn a_still_draws_one_model_alone() {
        let game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
        let front = game.still("player", 0, IVec2::NEG_Y, Quat::IDENTITY, &|_| PartPose::REST, 40.0, 0.75).unwrap();
        assert_eq!(front.models.len(), 1, "the player alone");
        assert!(front.instances.len() >= 100, "on a floor: {}", front.instances.len());
        let by_file = game.still(&game.cfg.player.model, 0, IVec2::NEG_Y, Quat::IDENTITY, &|_| PartPose::REST, 40.0, 0.75).unwrap();
        assert_eq!(by_file.models[0].voxels, front.models[0].voxels);
        let turned = game.still("player", 0, IVec2::NEG_Y, turning(-45.0, 0.0, 0.0), &|_| PartPose::REST, 40.0, 0.75).unwrap();
        assert!(turned.models[0].rot != Quat::IDENTITY);
        assert!(game.still("nothing.vox", 0, IVec2::Y, Quat::IDENTITY, &|_| PartPose::REST, 40.0, 0.75).is_none());
    }

    /// Standing still on raised ground must draw the same frame every time,
    /// whatever the frame rate. Rounding in the interpolation used to drop the
    /// player a voxel in some frames (only above height 4, so only off the floor).
    #[test]
    fn standing_on_the_plaza_draws_the_same_frame() {
        let mut game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
        for _ in 0..90 {
            game.game.step(&Input { up: true, ..Input::default() });
        }
        // Long enough for the dust of the last steps to settle.
        for _ in 0..60 {
            game.game.step(&Input::default());
        }
        assert!(game.game.player.body.pos.z > 4.0, "the player should stand on the plaza");
        assert!(game.game.particles.is_empty(), "dust still in the air");

        // The window's loop at 200 frames per second, 60 ticks per second.
        let mut acc = 0.0f32;
        let mut last: Option<Vec<[f32; 4]>> = None;
        let mut changes = 0;
        for _ in 0..400 {
            acc += 1.0 / 200.0;
            while acc >= crate::sim::DT {
                game.game.step(&Input::default());
                acc -= crate::sim::DT;
            }
            let drawn: Vec<_> = game.frame(acc / crate::sim::DT, 16.0 / 9.0).cubes().iter().map(|i| i.pos_size).collect();
            if last.as_ref().is_some_and(|l| *l != drawn) {
                changes += 1;
            }
            last = Some(drawn);
        }
        assert_eq!(changes, 0, "the standing player flickers");
    }

    /// At the edge of the world the camera behaves as everywhere else: it
    /// stays on the player and never changes height (no tilt, no zoom).
    #[test]
    fn the_edge_changes_nothing() {
        let mut game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
        let height = game.view_target(0.0).z;
        for _ in 0..240 {
            game.game.step(&Input { down: true, ..Input::default() });
            for alpha in [0.0, 0.3, 0.6, 0.9] {
                let view = game.view_target(alpha);
                let feet = game.player_origin(alpha);
                assert_eq!(view.z, height, "the camera changed height");
                assert_eq!((view.x, view.y), (feet.x, feet.y), "the camera left the player");
            }
        }
        assert!(game.game.player.body.pos.y < 10.0, "the player should have reached the south wall");
    }

    /// When the player stops, the view must stop with it: no catching-up
    /// steps afterwards, which read as hitches.
    #[test]
    fn stopping_stops_the_view() {
        let mut game = flat_game(512);
        for _ in 0..60 {
            game.game.step(&Input { left: true, ..Input::default() });
        }

        let mut late_moves = 0;
        let mut last = (game.player_origin(1.0), game.view_target(1.0));
        let mut player_still_for = 0;
        for _ in 0..60 {
            game.game.step(&Input::default());
            for alpha in [0.0, 0.3, 0.6, 0.9] {
                let now = (game.player_origin(alpha), game.view_target(alpha));
                if now.0 == last.0 {
                    player_still_for += 1;
                    if now.1 != last.1 {
                        late_moves += 1;
                    }
                } else {
                    player_still_for = 0;
                }
                last = now;
            }
        }
        println!("view moves {late_moves} times after the player stood still");
        assert!(player_still_for > 100, "the player should have come to rest");
        assert_eq!(late_moves, 0, "the view keeps moving after the player stopped");
    }

    /// Jumping in place must leave the view where it is: the camera height
    /// ignores jumps, so nothing may round it up and down while airborne.
    #[test]
    fn jumping_keeps_the_view_still() {
        let mut game = flat_game(256);
        for _ in 0..60 {
            game.game.step(&Input::default());
        }

        let mut views = Vec::new();
        for tick in 0..60 {
            game.game.step(&Input { jump: tick < 20, ..Input::default() });
            for alpha in [0.0, 0.3, 0.6, 0.9] {
                views.push((game.view_target(alpha) * 1000.0).round().as_ivec3());
            }
        }
        let changes = views.windows(2).filter(|pair| pair[0] != pair[1]).count();
        println!("view moves {changes} times during a jump");
        assert_eq!(changes, 0, "the view shakes while jumping");
    }

    /// Stairs rising eastwards from x 130, a voxel every 4, seen from the
    /// side by a camera that follows the height within `band`.
    fn stairs_game(band: crate::config::FollowHeight) -> Session<Shooter> {
        let mut world = crate::sim::tests::flat_world(IVec3::new(512, 64, 128));
        for x in 130..450 {
            for z in 1..=(x - 130) / 4 {
                for y in 0..64 {
                    world.set_raw(IVec3::new(x, y, z), 1);
                }
            }
        }
        let mut cfg = GameConfig::default();
        cfg.player.spawn = [100.0, 32.0];
        cfg.camera.pitch = 15.0;
        cfg.camera.follow_height = Some(band);
        game_on(world, cfg)
    }

    /// Climbing, a side view's camera follows the feet up in whole voxels,
    /// keeping them within its band and never moving back and forth.
    #[test]
    fn the_side_view_follows_a_climb_without_shaking() {
        let band = crate::config::FollowHeight { below: 6.0, above: 2.0 };
        let mut game = stairs_game(band);
        let run = Input { right: true, ..Input::default() };
        let mut heights = Vec::new();
        for _ in 0..400 {
            game.game.step(&run);
            for alpha in [0.0, 0.3, 0.6, 0.9] {
                let view = game.view_target(alpha);
                let feet = game.player_origin(alpha);
                assert!(view.z == view.z.round(), "the view's height {} is not a whole voxel", view.z);
                let offset = view.z - feet.z;
                assert!((-band.above..=band.below).contains(&offset), "the feet left the band: view {} feet {}", view.z, feet.z);
                heights.push(view.z);
            }
        }
        assert!(heights.last().unwrap() - heights[0] > 40.0, "the view did not climb: {} to {}", heights[0], heights.last().unwrap());
        assert!(heights.windows(2).all(|w| w[1] >= w[0]), "the view went down while climbing");
    }

    /// A jump that stays within the band keeps a side view still.
    #[test]
    fn a_jump_inside_the_zone_keeps_the_view_still() {
        let mut game = stairs_game(crate::config::FollowHeight { below: 30.0, above: 60.0 });
        for _ in 0..60 {
            game.game.step(&Input::default());
        }
        let mut views = Vec::new();
        for tick in 0..60 {
            game.game.step(&Input { jump: tick < 20, ..Input::default() });
            for alpha in [0.0, 0.3, 0.6, 0.9] {
                views.push((game.view_target(alpha) * 1000.0).round().as_ivec3());
            }
        }
        assert!(views.windows(2).all(|pair| pair[0] == pair[1]), "the view moves during a jump inside its band");
    }

    /// With `stop_at_edges` the view stops at the stage's end: the player
    /// walks on, the view stays; away from the end it follows again.
    #[test]
    fn the_view_stops_at_the_level_edges() {
        let mut cfg = GameConfig::default();
        cfg.camera.pitch = 15.0;
        cfg.camera.view_width = 160.0;
        cfg.camera.stop_at_edges = true;
        let mut game = game_on(crate::sim::tests::flat_world(IVec3::new(512, 64, 200)), cfg);
        let cam = game.cfg.camera.clone();
        let (half, down, _) = camera::view_extent(&cam, 16.0 / 9.0);
        let at = |game: &mut Session<Shooter>, x: f32| {
            game.game.player.body.pos.x = x;
            game.game.player.body.prev.x = x;
            game.within_stage(game.view_target(1.0), &cam, 16.0 / 9.0)
        };
        let (near, nearer) = (at(&mut game, 30.0), at(&mut game, 10.0));
        assert_eq!(near, nearer, "the view moved past the left edge");
        assert!((near.x - half).abs() <= 1.0, "the view stops at {} instead of {half}", near.x);
        assert!(near.z >= down - 1.0, "the view shows below the floor");
        let middle = at(&mut game, 256.0);
        assert_eq!(middle.x, 256.0, "away from the edges the view follows");
        let far = at(&mut game, 505.0);
        assert!((far.x - (512.0 - half)).abs() <= 1.0, "the view passed the right edge: {}", far.x);
    }

    /// Seen from above, `stop_at_edges` keeps the ground the screen shows on
    /// the stage across and in depth: at a corner the view stops, the
    /// corner rays meet the ground inside; in the middle it follows.
    #[test]
    fn a_view_from_above_stops_at_the_level_edges() {
        let mut cfg = GameConfig::default();
        cfg.camera.pitch = 50.0;
        cfg.camera.view_width = 320.0;
        cfg.camera.stop_at_edges = true;
        let mut game = game_on(crate::sim::tests::flat_world(IVec3::new(1088, 672, 32)), cfg);
        let cam = game.cfg.camera.clone();
        let (near, far) = camera::ground_extent(&cam, 16.0 / 9.0).expect("a view from above");
        let mut at = |x: f32, y: f32| {
            game.game.player.body.pos = Vec3::new(x, y, 16.0);
            game.game.player.body.prev = game.game.player.body.pos;
            game.game.sim.cam_target = Vec3::new(x, y, 16.0);
            game.game.sim.cam_prev = game.game.sim.cam_target;
            game.within_stage(game.view_target(1.0), &cam, 16.0 / 9.0)
        };
        for (x, y) in [(5.0, 5.0), (1080.0, 668.0), (5.0, 668.0), (1080.0, 5.0)] {
            let t = at(x, y);
            assert!(t.x + near.x >= -1.0 && t.x + far.x <= 1089.0, "the view shows beyond the sides at {t}");
            assert!(t.y + near.y >= -1.0 && t.y + far.y <= 673.0, "the view shows beyond the ends at {t}");
        }
        let middle = at(544.0, 336.0);
        assert_eq!((middle.x, middle.y), (544.0, 336.0), "away from the edges the view follows");
    }

    /// Seen from above the shadow box is what it always was, whatever the
    /// stage; seen from the side it is fitted to the stage and stays small.
    #[test]
    fn shadows_fit_the_view_from_above_and_from_the_side() {
        let world = Vec3::new(512.0, 512.0, 64.0);
        let sun = Vec3::new(0.45, 0.6, -1.0);
        for pitch in [35.0, 58.0, 72.0] {
            let cam = crate::config::CameraConfig { pitch, ..Default::default() };
            let a = camera::light_view_proj(&cam, Vec3::new(200.0, 200.0, 8.0), 16.0 / 9.0, sun, world, (Vec3::ZERO, world), 2048);
            let b = camera::light_view_proj(&cam, Vec3::new(200.0, 200.0, 8.0), 16.0 / 9.0, sun, world, (Vec3::ONE, Vec3::splat(40.0)), 2048);
            assert_eq!(a, b, "the stage changed the shadows of a view from above at pitch {pitch}");
        }
        let side = crate::config::CameraConfig { pitch: 15.0, view_width: 400.0, ..Default::default() };
        let stage = (Vec3::new(0.0, 100.0, 0.0), Vec3::new(2000.0, 164.0, 800.0));
        let big = Vec3::new(2000.0, 1000.0, 800.0);
        let (_, depth) = camera::light_view_proj(&side, Vec3::new(600.0, 132.0, 300.0), 16.0 / 9.0, sun, big, stage, 2048);
        assert!(depth < 1500.0, "the side view's shadow box is {depth} voxels deep");
    }

    /// A game that moves its sun (BunnyFarm's day) has its faces remember
    /// their shadow, and its shadows follow the sun as it is, picture by
    /// picture (the shader's `remembered`: a face at a shadow's edge
    /// changes once, as the edge passes); a game whose sun stands still
    /// draws as it always did.
    #[test]
    fn a_moving_sun_has_the_faces_remember_their_shadow() {
        let farm = Session::<crate::Voxcade>::load(Path::new("games/bunnyfarm")).unwrap();
        assert!(farm.frame(1.0, 16.0 / 9.0).remember, "BunnyFarm moves its sun");
        let keep = Session::<crate::Voxcade>::load(Path::new("games/voxkeep")).unwrap();
        assert!(!keep.frame(1.0, 16.0 / 9.0).remember, "Voxkeep's sun stands still");
        let cam = crate::config::CameraConfig::default();
        let world = Vec3::new(512.0, 512.0, 64.0);
        let at = |hour: f32| {
            let a = (hour - 6.0) / 14.0 * std::f32::consts::PI;
            let sun = Vec3::new(-a.cos() * 0.9, 0.55, -(0.25 + a.sin() * 0.85));
            camera::light_view_proj(&cam, Vec3::new(200.0, 200.0, 8.0), 16.0 / 9.0, sun, world, (Vec3::ZERO, world), 2048).0
        };
        assert_ne!(at(7.0), at(7.0 + 1.0 / 600.0), "the shadows follow the sun picture by picture");
    }

    /// Saving any file a map level is built from, a prefab for one, rebuilds
    /// the level; other files leave it alone.
    #[test]
    fn saving_a_prefab_rebuilds_the_level() {
        let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let prefab = game.reload(Path::new("games/moreblood/prefabs/palm_0.vvx")).unwrap();
        assert!(matches!(prefab, Reloaded::World), "a changed prefab must rebuild the level");
        let other = game.reload(Path::new("games/moreblood/notes.txt")).unwrap();
        assert!(matches!(other, Reloaded::Nothing));
    }

    /// More Blood starts calm: standing still at the start for three
    /// seconds, nobody notices the player and nothing hits.
    #[test]
    fn more_blood_starts_out_of_sight() {
        let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        for _ in 0..180 {
            game.step(&Default::default());
        }
        assert_eq!(game.game.alerted_enemies(), 0, "enemies notice the player at the start");
        assert_eq!(game.game.status.hp, 5, "the player is hit at the start");
    }

    /// The number keys switch between the songs `c.audio.music` lists; each plays.
    #[test]
    fn songs_can_be_switched() {
        let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let files: Vec<String> = game.cfg.audio.music.files().iter().map(|f| f.to_string()).collect();
        assert!(files.len() >= 2, "More Blood should list several songs");
        for (i, file) in files.iter().enumerate() {
            // The first song already plays; every other one is a change.
            assert_eq!(game.select_song(i), i != 0);
            assert_eq!(game.song_file(), Some(file.as_str()));
            assert!(game.music().is_some(), "{file} does not play");
        }
        assert!(!game.select_song(files.len()), "there is no song past the list");
    }

    /// Only models that can be seen or cast a shadow into the view are built:
    /// at More Blood's start not the enemies far up the level (the view
    /// itself reaches some 800 voxels ahead, up to the haze).
    #[test]
    fn models_out_of_view_are_not_built() {
        let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let frame = game.frame(1.0, 16.0 / 9.0);
        let far = game.view_target(1.0).y + 1200.0;
        assert!(game.game.enemies().any(|(e, _)| e.body.pos.y > far + 200.0), "More Blood should have enemies far ahead");
        let cubes = frame.cubes();
        assert!(cubes.iter().all(|i| i.pos_size[1] < far), "a cube far up the level was built");
        let all = game.game.enemies().count() * game.models.get("enemy.vvx").unwrap().visible[0].len();
        assert!(cubes.len() < all / 2, "{} cubes built, the enemies alone have {all}", cubes.len());
    }

    /// A model turned freely and drawn all as cubes (the level editor's
    /// pieces, `model_cubes`) has its neighbours meet corner to corner as
    /// the shader turns them (by the packed turn, `render::turn_as_drawn`),
    /// and stands where the exact turn puts it. Placed by the exact turn,
    /// neighbours were a hundredth of a voxel apart or over each other,
    /// and single pixels of what lay behind showed on its faces.
    #[test]
    fn a_model_turned_freely_as_cubes_meets_corner_to_corner() {
        let mut frame: Vec<(IVec3, [u8; 4])> = Vec::new();
        for z in 0..4 {
            for y in 0..8 {
                for x in 0..24 {
                    frame.push((IVec3::new(x, y, z), [110, 75, 50, 255]));
                }
            }
        }
        let metal = vec![0; frame.len()];
        let m = VoxModel::new(IVec3::new(24, 8, 4), vec![frame], vec![metal]);
        let origin = Vec3::new(100.0, 80.0, 3.0);
        for (look, rot) in [(IVec2::Y, turning(30.0, 0.0, 0.0)), (IVec2::X, turning(30.0, 20.0, 0.0)), (IVec2::NEG_Y, turning(-75.0, 12.0, 15.0))] {
            let (mut cubes, mut glass) = (Vec::new(), Vec::new());
            model_cubes(&m, 0, origin, look, rot, &mut cubes, &mut glass);
            let mut exact = Vec::new();
            draw(&mut |corner: Vec3, _, _, _, _, _, _| exact.push(corner + 0.5), None, &m, 0, origin, look, rot, &[], None);
            assert_eq!(cubes.len(), exact.len());
            let centre = |i: &Instance| Vec3::from_slice(&i.pos_size[..3]);
            let off = cubes.iter().zip(&exact).map(|(i, e)| centre(i).distance(*e)).fold(0.0, f32::max);
            assert!(off < 0.25, "{rot}: a cube {off} voxels from where the exact turn puts it");
            // As the shader turns each cube (`instance_turn`: the packed turn, normalized).
            let drawn = Quat::from_array(cubes[0].turn.map(|c| c as f32 / 127.0)).normalize();
            assert!(cubes.iter().all(|i| i.turn == cubes[0].turn));
            let (mut pairs, mut worst) = (0, 0.0f32);
            for a in &cubes {
                for b in &cubes {
                    // Neighbours in the model's grid: one step along an axis, seen turned.
                    let d = drawn.inverse() * (centre(b) - centre(a));
                    let step = d.round();
                    if step.abs().element_sum() == 1.0 {
                        pairs += 1;
                        worst = worst.max((d - step).abs().max_element());
                    }
                }
            }
            assert!(pairs > 1000, "{rot}: {pairs} pairs of neighbours");
            assert!(worst < 1e-4, "{rot}: neighbours' shared corners {worst} voxels apart as drawn");
        }
    }

    /// A model drawn as cubes (the level editor's pieces, `model_cubes`)
    /// leaves out the faces between its own opaque voxels, as the bank
    /// does, counted in the world's directions whichever way it faces:
    /// drawn, they won single bright pixels along the edges of a wall in
    /// shade. Its glass keeps them, for the join's skins.
    #[test]
    fn a_model_as_cubes_leaves_out_the_faces_between_its_voxels() {
        // A wall 4 wide, 2 deep, 3 high, a pane of glass on its top.
        let mut frame: Vec<(IVec3, [u8; 4])> = Vec::new();
        for z in 0..3 {
            for y in 0..2 {
                for x in 0..4 {
                    frame.push((IVec3::new(x, y, z), [200, 190, 150, 255]));
                }
            }
        }
        frame.extend((0..4).map(|x| (IVec3::new(x, 0, 3), [150, 200, 220, 120])));
        let metal = vec![0; frame.len()];
        let m = VoxModel::new(IVec3::new(4, 2, 4), vec![frame], vec![metal]);
        for look in [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X] {
            let (mut opaque, mut glass) = (Vec::new(), Vec::new());
            model_cubes(&m, 0, Vec3::new(20.0, 30.0, 4.0), look, Quat::IDENTITY, &mut opaque, &mut glass);
            assert_eq!((opaque.len(), glass.len()), (24, 4), "{look}: the wall's voxels are all on its surface");
            let cell = |i: &Instance| Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
            let taken: crate::world::CellSet = opaque.iter().map(cell).collect();
            for i in &opaque {
                let against = crate::mesh::DIRS.iter().enumerate().filter(|(_, d)| taken.contains(&(cell(i) + **d))).fold(0u8, |b, (k, _)| b | 1 << k);
                assert_eq!(i.fx[2], against, "{look}: the cube at {} leaves out exactly the faces its neighbours cover", cell(i));
            }
            assert!(glass.iter().all(|i| i.fx[2] == 0), "{look}: glass keeps its faces");
        }
    }

    /// A model drawn from the bank on the GPU puts the same cubes in the
    /// world as drawn cube by cube, every way it turns, quarter turns and
    /// free ones, flashing or not; its glass stays cubes
    /// (`render::ModelBank::cubes` does what the shader's `vs_model` does).
    #[test]
    fn models_from_the_bank_match_their_cubes() {
        let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        type Cube = ([u32; 3], [u8; 4], u8, [i8; 4], [i8; 4], u8);
        let key = |centre: Vec3, color, metal, shade, turn, covered| -> Cube { (centre.to_array().map(f32::to_bits), color, metal, shade, turn, covered) };
        let huey = game.models.get("huey.vvx").unwrap();
        assert!(huey.glass.iter().any(|g| !g.is_empty()), "the Huey has no glass to test with");
        for m in [game.models.get("player").unwrap(), huey] {
            for look in [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X] {
                for rot in [Quat::IDENTITY, turning(30.0, 10.0, -25.0)] {
                    for tint in [None, Some([255, 255, 255, 255])] {
                        let origin = Vec3::new(100.0, 200.0, 7.0);
                        let mut old: Vec<Cube> = Vec::new();
                        draw(&mut |corner: Vec3, color, metal, _, shade, turn, covered| old.push(key(corner + 0.5, color, metal, shade, turn, covered)), None, m, 1, origin, look, rot, &[], tint);
                        let mut new: Vec<Cube> = Vec::new();
                        let mut on_gpu = Vec::new();
                        draw(&mut |corner: Vec3, color, metal, _, shade, turn, covered| new.push(key(corner + 0.5, color, metal, shade, turn, covered)), Some(&mut on_gpu), m, 1, origin, look, rot, &[], tint);
                        assert_eq!(on_gpu.len(), 1);
                        let glass = new.len();
                        let bank = &game.bank;
                        new.extend(on_gpu.iter().flat_map(|d| bank.cubes(d)).map(|i| key(Vec3::from_slice(&i.pos_size[..3]), i.color, i.fx[3], i.shade, i.turn, i.fx[2])));
                        assert!(new.len() > glass * 10, "most of the model should come from the bank");
                        old.sort_unstable();
                        new.sort_unstable();
                        assert!(old == new, "{look} {rot} {tint:?}: the bank draws other cubes");
                    }
                }
            }
        }
    }

    /// A figure of parts: posed, a part turns about its joint (an arm held
    /// out to the side, turned a quarter to point ahead: its hand ahead at
    /// the shoulder's height) and what hangs on it goes along; the rest
    /// stays; turned and quartered alike, the bank draws each part where
    /// the cubes do.
    #[test]
    fn a_figures_parts_turn_about_their_joints() {
        let dir = crate::scratch::Scratch::new("parts");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("figure.vox");
        // A body, a head on it, an arm held out to its side, a hand at its end.
        crate::vox::write_figure(&file, &[("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)), ("arm", IVec3::new(3, 2, 1), IVec3::new(4, 1, 6)), ("hand", IVec3::new(1, 2, 1), IVec3::new(7, 1, 6))]);
        let mut m = crate::vox::load_model(&file).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let mut bank = ModelBank::default();
        bank.add(&mut m);
        assert_eq!((m.parts[2].parent, m.parts[3].parent), (Some(0), Some(2)), "the arm on the body, the hand on the arm");
        let origin = Vec3::new(100.0, 200.0, 7.0);
        type Cube = ([u32; 3], [u8; 4], u8, [i8; 4], [i8; 4], u8);
        let key = |centre: Vec3, color, metal, shade, turn, covered| -> Cube { (centre.to_array().map(f32::to_bits), color, metal, shade, turn, covered) };
        let cubes = |look: IVec2, rot: Quat, pose: &[Quat]| {
            let pose: Vec<PartPose> = pose.iter().map(|q| PartPose::turned(*q)).collect();
            let pose = &pose[..];
            let mut all: Vec<Cube> = Vec::new();
            draw(&mut |corner: Vec3, color, metal, _, shade, turn, covered| all.push(key(corner + 0.5, color, metal, shade, turn, covered)), None, &m, 0, origin, look, rot, pose, None);
            let mut gpu: Vec<Cube> = Vec::new();
            let mut on_gpu = Vec::new();
            draw(&mut |corner: Vec3, color, metal, _, shade, turn, covered| gpu.push(key(corner + 0.5, color, metal, shade, turn, covered)), Some(&mut on_gpu), &m, 0, origin, look, rot, pose, None);
            assert_eq!(on_gpu.len(), 4, "a draw a part");
            gpu.extend(on_gpu.iter().flat_map(|d| bank.cubes(d)).map(|i| key(Vec3::from_slice(&i.pos_size[..3]), i.color, i.fx[3], i.shade, i.turn, i.fx[2])));
            all.sort_unstable();
            gpu.sort_unstable();
            assert!(all == gpu, "{look} {rot}: the bank draws other cubes");
            all
        };
        let colour = |part: usize| m.frames[0][m.parts[part].voxels[0] as usize].1;
        let centres = |cubes: &[Cube], part: usize| -> Vec<Vec3> { cubes.iter().filter(|c| c.1 == colour(part)).map(|c| Vec3::from_array(c.0.map(f32::from_bits))).collect() };
        let still = cubes(IVec2::Y, Quat::IDENTITY, &[]);
        let ahead = cubes(IVec2::Y, Quat::IDENTITY, &[Quat::IDENTITY, Quat::IDENTITY, turning(-90.0, 0.0, 0.0), Quat::IDENTITY]);
        let (before, after) = (centres(&still, 3), centres(&ahead, 3));
        assert!(after.iter().all(|h| h.y > before[0].y + 2.0), "the hand ahead: {after:?} from {before:?}");
        assert!(after.iter().all(|h| (h.z - before[0].z).abs() < 1e-4), "at the shoulder's height: {after:?}");
        assert_eq!(centres(&ahead, 1), centres(&still, 1), "the head stays");
        // Quartered and turned freely, the bank still draws what the cubes do.
        for look in [IVec2::X, IVec2::NEG_Y, IVec2::NEG_X] {
            cubes(look, turning(20.0, 0.0, 10.0), &[turning(0.0, 0.0, 5.0), turning(30.0, 0.0, 0.0), turning(0.0, -40.0, 0.0), turning(0.0, 60.0, 0.0)]);
        }
        // Posed, a part's depth is taken nearer the further out it hangs, so
        // faces of two parts in one plane do not flicker (a bent elbow); at
        // rest every part lies as it is.
        let nearer = |pose: &[Quat]| {
            let pose: Vec<PartPose> = pose.iter().map(|q| PartPose::turned(*q)).collect();
            let pose = &pose[..];
            let mut on_gpu = Vec::new();
            draw(&mut |_, _, _, _, _, _, _| {}, Some(&mut on_gpu), &m, 0, origin, IVec2::Y, Quat::IDENTITY, pose, None);
            on_gpu.iter().map(|d| d.nearer).collect::<Vec<f32>>()
        };
        assert_eq!(nearer(&[]), [0.0; 4], "at rest");
        assert_eq!(nearer(&[Quat::IDENTITY, Quat::IDENTITY, turning(-90.0, 0.0, 0.0), Quat::IDENTITY]), [0.0, 1.0, 1.0, 2.0], "body, head, arm, hand");
    }

    /// A part turned at rest in its file (`rotation`, `pitch`, `roll`) is
    /// drawn exactly as the unturned figure posed so: from the bank and as
    /// cubes alike, what hangs on it carried along; a pose turns on from
    /// the rest (`pose · rest`); `part_at_rest` is where `draw` puts it.
    #[test]
    fn a_part_turned_at_rest_draws_as_the_same_pose() {
        let text = |rest: &str| format!("format = \"voxcade voxel 1\"\nsize = [9, 3, 12]\n\n[palette]\np = \"#6a4a30\"\nl = \"#a07040\"\nh = \"#d0b040\"\n\n[[part]]\nname = \"frame\"\nat = [-4, 0, 0]\nlayers = \"\"\"\np.......p\n*11\n\nppppppppp\n\"\"\"\n\n[[part]]\nname = \"leaf\"\nat = [-3, 0, 0]\nparent = \"frame\"\njoint = [-3, 0.5, 5.5]\n{rest}layers = \"\"\"\nlllllll\n*11\n\"\"\"\n\n[[part]]\nname = \"handle\"\nat = [2, -1, 5]\nparent = \"leaf\"\nlayers = \"\"\"\nh\n\"\"\"\n");
        let model = |rest: &str| {
            let mut m = crate::vvx::read(&text(rest)).unwrap();
            ModelBank::default().add(&mut m);
            m
        };
        let (plain, rested) = (model(""), model("rotation = -70\npitch = 15\n"));
        assert!(!rests_turned(&plain) && rests_turned(&rested));
        let origin = Vec3::new(40.0, 50.0, 3.0);
        // The cubes (every voxel, as a fading model draws) and the bank's draws.
        let drawn = |m: &VoxModel, look: IVec2, rot: Quat, pose: &[Quat]| {
            let pose: Vec<PartPose> = pose.iter().map(|q| PartPose::turned(*q)).collect();
            let pose = &pose[..];
            let mut cubes: Vec<[u32; 3]> = Vec::new();
            draw(&mut |corner: Vec3, _, _, _, _, _, _| cubes.push(corner.to_array().map(f32::to_bits)), None, m, 0, origin, look, rot, pose, None);
            let mut on_gpu = Vec::new();
            draw(&mut |_, _, _, _, _, _, _| {}, Some(&mut on_gpu), m, 0, origin, look, rot, pose, None);
            (cubes, on_gpu)
        };
        let leaf = turning(-70.0, 15.0, 0.0);
        for (look, rot) in [(IVec2::Y, Quat::IDENTITY), (IVec2::X, turning(20.0, 0.0, 10.0)), (IVec2::NEG_Y, Quat::IDENTITY)] {
            assert_eq!(drawn(&rested, look, rot, &[]), drawn(&plain, look, rot, &[Quat::IDENTITY, leaf, Quat::IDENTITY]), "{look}: the rest is that pose");
            // A pose on top: round further, after the rest.
            let more = turning(30.0, 0.0, 0.0);
            assert_eq!(drawn(&rested, look, rot, &[Quat::IDENTITY, more, Quat::IDENTITY]), drawn(&plain, look, rot, &[Quat::IDENTITY, more * leaf, Quat::IDENTITY]), "{look}: pose · rest");
        }
        let (_, at_rest) = drawn(&plain, IVec2::Y, Quat::IDENTITY, &[]);
        assert!(at_rest.iter().all(|d| d.rot == Quat::IDENTITY && d.nearer == 0.0), "none at rest: as before");
        let (_, turned) = drawn(&rested, IVec2::Y, Quat::IDENTITY, &[]);
        assert_eq!(turned.iter().map(|d| d.nearer).collect::<Vec<_>>(), [0.0, 1.0, 2.0], "a part turned at rest is taken nearer as a posed one");
        assert_eq!(part_at_rest(&rested, 2).0, leaf, "the handle carried by the leaf");
        assert_eq!(part_at_rest(&plain, 2), (Quat::IDENTITY, Vec3::ZERO));
        // Only the picture: the simulation's cells are the unturned ones.
        assert_eq!(quarter_cells(&rested, IVec2::Y), quarter_cells(&plain, IVec2::Y));
    }

    /// MagicaVoxel's emission material on a colour makes a model's voxels
    /// of it glow, as `frame:cube`'s glow does: from the bank (`fx[0]`
    /// of its cubes) and as cubes (what a fading model draws); a model
    /// without one carries no glow at all, as before.
    #[test]
    fn an_emissive_colour_makes_a_models_voxels_glow() {
        let dir = crate::scratch::Scratch::new("emit");
        std::fs::create_dir_all(&dir).unwrap();
        let model = |emit: bool| {
            let voxels = (0..3).map(|x| dot_vox::Voxel { x, y: 0, z: 0, i: x }).collect();
            let mut palette = vec![dot_vox::Color { r: 0, g: 0, b: 0, a: 255 }; 256];
            palette[0] = dot_vox::Color { r: 200, g: 40, b: 40, a: 255 };
            palette[1] = dot_vox::Color { r: 255, g: 220, b: 80, a: 255 };
            palette[2] = dot_vox::Color { r: 40, g: 40, b: 200, a: 255 };
            let lamp = dot_vox::Material { id: 2, properties: [("_type".to_string(), "_emit".to_string()), ("_emit".to_string(), "0.6".to_string())].into() };
            let data = dot_vox::DotVoxData {
                version: 150,
                index_map: Vec::new(),
                models: vec![dot_vox::Model { size: dot_vox::Size { x: 3, y: 1, z: 1 }, voxels }],
                palette,
                materials: if emit { vec![lamp] } else { Vec::new() },
                scenes: Vec::new(),
                layers: Vec::new(),
            };
            let file = dir.join(if emit { "lamp.vox" } else { "plain.vox" });
            let mut bytes = Vec::new();
            data.write_vox(&mut bytes).unwrap();
            std::fs::write(&file, bytes).unwrap();
            crate::vox::load_model(&file).unwrap()
        };
        let want = (0.6f32 * 255.0).round() as u8;
        // The glow by voxel and the colours' own.
        let mut lamp = model(true);
        assert_eq!(lamp.glow, vec![vec![0, want, 0]]);
        let mut plain = model(false);
        assert!(plain.glow.is_empty(), "no glow where no color has one");
        let origin = Vec3::new(10.0, 10.0, 0.0);
        for m in [&mut lamp, &mut plain] {
            let mut bank = crate::render::ModelBank::default();
            bank.add(m);
            let glowing = |m: &VoxModel| m.glow.first().map_or(0, |g| g.iter().filter(|&&g| g != 0).count());
            // From the bank.
            let mut on_gpu = Vec::new();
            draw(&mut |_, _, _, _, _, _, _| {}, Some(&mut on_gpu), m, 0, origin, IVec2::Y, Quat::IDENTITY, &[], None);
            let from_bank: Vec<u8> = on_gpu.iter().flat_map(|d| bank.cubes(d)).map(|i| i.fx[0]).collect();
            assert_eq!(from_bank.len(), 3);
            assert_eq!(from_bank.iter().filter(|&&g| g == want).count(), glowing(m), "the bank's cubes");
            assert!(from_bank.iter().all(|&g| g == 0 || g == want));
            // As cubes, through a Voxels as the game's frame does: no byte lost.
            let world = crate::world::World::new(IVec3::new(32, 32, 8), [[0; 4]; 256]);
            let mut voxels = Voxels { world: &world, eye: Vec3::new(0.0, 0.0, 100.0), taken: Default::default(), opaque: Vec::new(), see_through: Vec::new() };
            draw(
                &mut |corner: Vec3, color: [u8; 4], metal: u8, glow: u8, shade: [i8; 4], turn: [i8; 4], covered: u8| {
                    let glow = if glow == 0 { 0.0 } else { (glow as f32 + 0.5) / 255.0 };
                    voxels.add(corner, color, glow, 1.0, metal, shade, turn, covered)
                },
                None,
                m,
                0,
                origin,
                IVec2::Y,
                Quat::IDENTITY,
                &[],
                None,
            );
            let cubes = voxels.finish();
            assert_eq!(cubes.len(), 3);
            assert_eq!(cubes.iter().filter(|i| i.fx[0] == want).count(), glowing(m), "as cubes");
            assert!(cubes.iter().all(|i| i.fx[0] == 0 || i.fx[0] == want));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A part's joint set by hand in the file is where it turns: an arm
    /// hanging beside the body, its joint put at its top, swung half round
    /// rises above it; by the rules (the middle of where it meets the
    /// body) it stays about where it hung.
    #[test]
    fn a_part_turns_about_the_joint_set_in_its_file() {
        let dir = crate::scratch::Scratch::new("rigged");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("figure.vox");
        let parts = [("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("arm", IVec3::new(1, 1, 6), IVec3::new(4, 1, 1))];
        let arm_z = |rig: &[(&str, &str, &str)]| {
            crate::vox::write_figure_with(&file, &parts, rig);
            let m = crate::vox::load_model(&file).unwrap();
            let mut zs = Vec::new();
            let colour = m.frames[0][m.parts[1].voxels[0] as usize].1;
            draw(&mut |corner: Vec3, c, _, _, _, _, _| if c == colour { zs.push(corner.z + 0.5) }, None, &m, 0, Vec3::ZERO, IVec2::Y, Quat::IDENTITY, &[PartPose::REST, PartPose::turned(turning(0.0, 180.0, 0.0))], None);
            (m.parts[1].joint.z, zs.iter().copied().fold(f32::MAX, f32::min), zs.iter().copied().fold(f32::MIN, f32::max))
        };
        // Its top: the arm's voxels end at z 7 (from 1 to 7).
        let (joint, low, high) = arm_z(&[("arm", "_joint", "4.5 1.5 7")]);
        assert_eq!(joint, 7.0);
        assert!(low > 7.0 && high < 13.5, "swung up about its top: {low}..{high}");
        let (joint, low, high) = arm_z(&[]);
        assert_eq!(joint, 4.0, "by the rules: the middle of where it meets the body");
        assert!(low < 2.0 && high > 6.0, "swung about its middle: {low}..{high}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A free turn of a quarter puts every voxel where the quarter turn
    /// does (it turns about the same pivot), and none turns nothing:
    /// models stay exactly as they were unless a game turns them.
    #[test]
    fn a_free_quarter_turn_is_the_quarter_turn() {
        let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let m = game.models.get("player").unwrap();
        let origin = Vec3::new(100.0, 200.0, 7.0);
        let cubes = |look: IVec2, rot: Quat| {
            let mut all: Vec<(Vec3, [i8; 4])> = Vec::new();
            draw(&mut |corner: Vec3, _, _, _, _, turn, _| all.push((corner + 0.5, turn)), None, m, 1, origin, look, rot, &[], None);
            all
        };
        assert_eq!(turning(0.0, 0.0, 0.0), Quat::IDENTITY);
        assert!(cubes(IVec2::Y, Quat::IDENTITY).iter().all(|(_, turn)| *turn == [0; 4]), "an unturned model is square to the grid");
        for (degrees, look) in [(90.0, IVec2::X), (180.0, IVec2::NEG_Y), (-90.0, IVec2::NEG_X)] {
            let free = cubes(IVec2::Y, turning(degrees, 0.0, 0.0));
            let quarter = cubes(look, Quat::IDENTITY);
            assert_eq!(free.len(), quarter.len());
            for ((a, _), (b, _)) in free.iter().zip(&quarter) {
                assert!(a.distance(*b) < 1e-3, "{degrees}°: a voxel at {a} instead of {b}");
            }
        }
        // Rolled right by a quarter it lies on its right side, its top
        // to the right (+x) by half its height from the middle: about the
        // voxel at half its height (22 tall: the 12th, 11.5 up), so its
        // top voxel's middle lies 10 to the right, on the grid.
        let rolled = cubes(IVec2::Y, turning(0.0, 0.0, 90.0));
        let right = rolled.iter().map(|(c, _)| c.x).fold(f32::MIN, f32::max);
        let centre = crate::render::turn_centre(m.size).z;
        assert!((right - (origin.x + 0.5 + (m.size.z as f32 - 0.5 - centre))).abs() < 1e-3, "rolled right, its top went to x {right}");
        assert!(rolled.iter().all(|(c, _)| (c - c.floor() - Vec3::splat(0.5)).abs().max_element() < 1e-3), "a quarter roll keeps the voxels on the grid");
        // The turn is in the model's own directions: facing +x, its right
        // is -y, and rolled right its top goes there.
        let rolled_east = cubes(IVec2::X, turning(0.0, 0.0, 90.0));
        let top = rolled_east.iter().map(|(c, _)| c.y).fold(f32::MAX, f32::min);
        assert!(top < origin.y - m.size.z as f32 * 0.25, "facing +x and rolled right, its top should lie towards -y ({top})");
    }

    /// Where a model's pivot lies and what a turn does to a model of even
    /// size (pinned as it is, so nobody changes how games draw by
    /// accident): the pivot is the cell `size / 2` (whole voxels, rounded
    /// down) of each side, and a quarter turn turns the cells about the
    /// middle of that cell. An odd size has the pivot in its middle and
    /// fills the same cells at every turn. An even size has one more cell
    /// on the low side of the pivot than on the high side; an axis that
    /// a turn flips (a half turn flips both, a quarter turn one) then
    /// fills the cells one voxel higher than unturned. Free turns (`rotates
    /// = "free"`, `frame:model`'s turn) land on the same cells as the
    /// quarter turns.
    #[test]
    fn an_even_sized_model_shifts_a_voxel_when_it_turns() {
        let dir = crate::scratch::Scratch::new("pivot");
        std::fs::create_dir_all(&dir).unwrap();
        // The cells a model fills from the pivot's cell, per look: the
        // lowest and highest of x and y.
        let cells = |size: IVec3, look: IVec2, free: Quat| {
            let file = dir.join("box.vox");
            crate::vox::write_figure(&file, &[("box", size, IVec3::ZERO)]);
            let m = crate::vox::load_model(&file).unwrap();
            assert_eq!(m.size, size);
            let origin = Vec3::new(100.0, 200.0, 0.0);
            let (mut lo, mut hi) = (IVec2::MAX, IVec2::MIN);
            draw(
                &mut |corner: Vec3, _, _, _, _, _, _| {
                    let c = IVec2::new(corner.x.round() as i32 - 100, corner.y.round() as i32 - 200);
                    (lo, hi) = (lo.min(c), hi.max(c));
                },
                None,
                &m,
                0,
                origin,
                look,
                free,
                &[],
                None,
            );
            (lo.to_array(), hi.to_array())
        };
        // Looking +y, +x, -y, -x: the quarter turns.
        let looks = [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X];
        // Odd across both ways: always -2 to 2.
        for look in looks {
            assert_eq!(cells(IVec3::new(5, 5, 2), look, Quat::IDENTITY), ([-2, -2], [2, 2]), "odd, {look}");
        }
        // Even across both ways (4 by 4): -2 to 1 as it stands; the flipped
        // axis -1 to 2: +x flips y, -y flips both, -x flips x.
        let want = [([-2, -2], [1, 1]), ([-2, -1], [1, 2]), ([-1, -1], [2, 2]), ([-1, -2], [2, 1])];
        for (look, want) in looks.iter().zip(want) {
            assert_eq!(cells(IVec3::new(4, 4, 2), *look, Quat::IDENTITY), want, "even, {look}");
        }
        // Four by five: the five never moves, the four does when flipped.
        let want = [([-2, -2], [1, 2]), ([-2, -1], [2, 2]), ([-1, -2], [2, 2]), ([-2, -2], [2, 1])];
        for (look, want) in looks.iter().zip(want) {
            assert_eq!(cells(IVec3::new(4, 5, 2), *look, Quat::IDENTITY), want, "four by five, {look}");
        }
        // A free half turn fills the cells of the quarter turns' second.
        assert_eq!(cells(IVec3::new(4, 4, 2), IVec2::Y, turning(180.0, 0.0, 0.0)), ([-1, -1], [2, 2]), "a free half turn");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The cells a solid piece fills (`quarter_cells`) are exactly where
    /// its model's voxels are drawn, every quarter turn, an even size too;
    /// the hidden ones inside count as well.
    #[test]
    fn a_solid_pieces_cells_are_where_it_is_drawn() {
        let dir = crate::scratch::Scratch::new("cells");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("box.vox");
        crate::vox::write_figure(&file, &[("box", IVec3::new(4, 5, 3), IVec3::ZERO)]);
        let m = crate::vox::load_model(&file).unwrap();
        for look in [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X] {
            let cells = quarter_cells(&m, look);
            assert_eq!(cells.len(), 4 * 5 * 3, "every voxel, {look}");
            let mut drawn = Vec::new();
            let origin = Vec3::new(100.0, 200.0, 7.0);
            draw(&mut |corner: Vec3, _, _, _, _, _, _| drawn.push((corner - origin).round().as_ivec3()), None, &m, 0, origin, look, Quat::IDENTITY, &[], None);
            assert!(!drawn.is_empty() && drawn.iter().all(|c| cells.contains(c)), "drawn where it is solid, {look}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A piece turned by quarter turns about any axis (`turn`, `pitch`,
    /// `roll`: all 64 of them, the 24 ways a box can lie) fills exactly the
    /// cells its model is drawn in, as `frame:model` draws it with the same
    /// three numbers (the quarter way it faces, then the rest freely): an
    /// even height, an odd one, an even width.
    #[test]
    fn a_solid_piece_turned_any_way_is_where_it_is_drawn() {
        let dir = crate::scratch::Scratch::new("cells-24");
        for size in [IVec3::new(4, 5, 6), IVec3::new(3, 2, 7), IVec3::new(6, 4, 9)] {
            let file = dir.join(format!("box{}.vox", size.z));
            crate::vox::write_figure(&file, &[("box", size, IVec3::ZERO)]);
            let m = crate::vox::load_model(&file).unwrap();
            for k in 0..64 {
                let turns = [k % 4 * 90, k / 4 % 4 * 90, k / 16 * 90];
                let cells = piece_cells(&m, turns).unwrap();
                assert_eq!(cells.len(), (size.x * size.y * size.z) as usize, "every voxel, {turns:?}");
                let look = IVec2::from(crate::grid::Way::nearest(turns[0] as f64).pair());
                let free = turning(0.0, turns[1] as f32, turns[2] as f32);
                let origin = Vec3::new(100.0, 200.0, 7.0);
                let mut drawn = Vec::new();
                draw(
                    &mut |corner: Vec3, _, _, _, _, _, _| {
                        let off = (corner - corner.round()).abs().max_element();
                        assert!(off < 1e-3, "a voxel off the grid by {off}, {turns:?}");
                        drawn.push((corner - origin).round().as_ivec3());
                    },
                    None,
                    &m,
                    0,
                    origin,
                    look,
                    free,
                    &[],
                    None,
                );
                assert!(drawn.iter().all(|c| cells.contains(c)), "drawn where it is solid, {turns:?}");
                let bounds = |cs: &[IVec3]| (cs.iter().fold(IVec3::MAX, |a, c| a.min(*c)), cs.iter().fold(IVec3::MIN, |a, c| a.max(*c)));
                assert_eq!(bounds(&drawn), bounds(&cells), "the same box, {turns:?}");
            }
        }
        let ways: std::collections::BTreeSet<_> = (0..64).map(|k| quarter_matrix([k % 4 * 90, k / 4 % 4 * 90, k / 16 * 90]).unwrap()).collect();
        assert_eq!(ways.len(), 24, "the 24 ways a box lies");
        assert_eq!(piece_cells(&crate::vox::load_model(&dir.join("box6.vox")).unwrap(), [0, 30, 0]), None, "off the quarters: none");
    }

    /// A kind's spin turns its model round its upright axis with the
    /// game's time, every thing of the kind alike: at 120 degrees a second
    /// a quarter turn after three quarters of a second; none without.
    #[test]
    fn a_spinning_kind_turns_round() {
        assert_eq!(spinning(0.0, 1000.0), Quat::IDENTITY);
        let same = |a: Quat, b: Quat| a.dot(b).abs() > 0.99999;
        let quarter = spinning(120.0, 45.0);
        assert!(same(quarter, turning(90.0, 0.0, 0.0)), "{quarter}");
        assert!(same(spinning(120.0, 225.0), turning(90.0, 0.0, 0.0)), "round again");
    }

    /// The title logo's letters drawn from the bank put the same cubes on
    /// screen as built every frame: while they fall, as they land and
    /// while BLOOD drips (`ui::logo_letters`).
    #[test]
    fn the_logo_from_the_bank_matches_its_cubes() {
        let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let settings = Settings::default();
        type Cube = ([u32; 3], [u8; 4], [u8; 2], [i8; 4]);
        let cubes = |f: &FrameData| -> Vec<Cube> {
            let mut all: Vec<Cube> = f.ui_cubes().iter().map(|i| ([0, 1, 2].map(|k| i.pos_size[k].to_bits()), i.color, [i.fx[0], i.fx[1]], i.shade)).collect();
            all.sort_unstable();
            all
        };
        for ticks in [20, 45, 80, 240] {
            let mut menu = Menu::title();
            for _ in 0..ticks {
                menu.update();
            }
            let new = game.menu_frame(&menu, &settings, 0.5, 16.0 / 9.0, Scene::Title { time: 1.0 });
            assert!(!new.ui_models.is_empty(), "the logo is not drawn from the bank");
            let logo = std::mem::take(&mut game.logo);
            let old = game.menu_frame(&menu, &settings, 0.5, 16.0 / 9.0, Scene::Title { time: 1.0 });
            game.logo = logo;
            assert!(old.ui_models.is_empty());
            assert!(cubes(&old) == cubes(&new), "{ticks} ticks in: the logo from the bank differs");
        }
    }

    /// The fingerprint follows what the simulation reads: the light, the
    /// music or Windows line ends leave it, the physics, a camera turn, the
    /// hit stop or a level file change it (a replay of the old data would
    /// run apart). And it is the same every time the game loads.
    #[test]
    fn the_fingerprint_follows_what_the_simulation_reads() {
        let dir = Path::new("games/moreblood");
        let cfg = config::<Shooter>(dir).unwrap();
        let level = |json: &[u8]| vec![("map.json".to_string(), json.to_vec()), ("ground/height.png".to_string(), vec![137, 80, 0, 13, 10])];
        let print = |c: &GameConfig, json: &[u8]| {
            fingerprint_of(c, &<Shooter as Game>::sections(c), &<Shooter as Game>::actions(c), &level(json)).unwrap()
        };
        let base = print(&cfg, b"{}\n");
        let with = |change: &dyn Fn(&mut GameConfig)| {
            let mut c = cfg.clone();
            change(&mut c);
            print(&c, b"{}\n")
        };
        assert_eq!(with(&|c| (c.light.ambient, c.audio.music_volume, c.camera.view_width) = (0.6, 0.5, 300.0)), base, "the look or the sound changed it");
        assert_eq!(with(&|c| c.audio.drive = 6.0), base, "the drive into the limiter changed it");
        assert_eq!(print(&cfg, b"{}\r\n"), base, "a level file with Windows line ends changes it");
        assert_ne!(with(&|c| c.physics.gravity = 530.0), base);
        assert_ne!(with(&|c| c.effects.hitstop = 3), base);
        assert_ne!(with(&|c| c.camera.yaw = 90.0), base);
        assert_ne!(print(&cfg, b"{ }\n"), base);
        // A default written out is the same game.
        let arena = Path::new("games/arena");
        let mut written = config::<Shooter>(arena).unwrap();
        written.physics.gravity = crate::config::PhysicsConfig::default().gravity;
        let plain = config::<Shooter>(arena).unwrap();
        assert_eq!(print(&written, b"{}\n"), print(&plain, b"{}\n"));
        let game = Session::<Shooter>::load(dir).unwrap();
        assert_eq!(game.fingerprint().unwrap(), Session::<Shooter>::load(dir).unwrap().fingerprint().unwrap());
    }

    /// Every game's config, written out and read back, is the same config.
    #[test]
    fn the_config_reads_back_as_written() {
        for game in ["games/moreblood", "games/arena", "games/bench"] {
            let cfg = config::<Shooter>(Path::new(game)).unwrap();
            let text = toml::to_string(&cfg).unwrap();
            let back: GameConfig = toml::from_str(&text).unwrap();
            assert_eq!(toml::to_string(&back).unwrap(), text, "{game} reads back otherwise");
        }
    }

    /// The same input gives the same state every second, run after run (the
    /// check replays carry); other input soon gives another.
    #[test]
    fn the_same_run_gives_the_same_state() {
        let run = |fire: bool| {
            let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
            let mut states = Vec::new();
            for t in 0..300 {
                let input = Input { up: true, left: t % 90 < 30, fire: fire && t % 20 < 10, grenade: t == 120, ..Input::default() };
                game.step(&input.into());
                if t % 60 == 59 {
                    states.push(game.game.state_hash());
                }
            }
            states
        };
        let shooting = run(true);
        assert_eq!(shooting, run(true), "the same run gave another state");
        assert_ne!(shooting.last(), run(false).last(), "shooting changed nothing");
    }

    /// A game kept at one tick and brought back after playing on stands
    /// exactly as it was kept, and plays on exactly as it did the first
    /// time: the copy shares nothing the game went on to change.
    #[test]
    fn a_restored_game_plays_on_the_same() {
        use crate::replay::{Replay, Script, parse_script};
        let dir = Path::new("games/moreblood");
        let replay = Replay::load(&dir.join("replays/tests/run_and_gun.toml")).unwrap();
        let mut game = Session::<Shooter>::load(dir).unwrap();
        let mut script = Script::new(parse_script(&replay.input, game.actions()).unwrap());
        let inputs: Vec<_> = (0..1200).map(|_| script.next_input()).collect();
        for input in &inputs[..600] {
            game.step(input);
        }
        let kept_at = game.game.state_hash();
        let start = std::time::Instant::now();
        let snapshot = game.snapshot().unwrap();
        let took = start.elapsed();
        let mut first = Vec::new();
        for input in &inputs[600..] {
            game.step(input);
            first.push(game.game.state_hash());
        }
        assert_ne!(game.game.state_hash(), kept_at, "the run should have gone on");
        let start = std::time::Instant::now();
        game.restore(&snapshot);
        println!("snapshot {:?}, restore {:?}", took, start.elapsed());
        assert_eq!(game.game.state_hash(), kept_at, "the kept state was changed by the game playing on");
        assert!(!game.world_mut().take_dirty().is_empty(), "the blasts since should be meshed anew");
        for (n, input) in inputs[600..].iter().enumerate() {
            game.step(input);
            assert_eq!(game.game.state_hash(), first[n], "tick {} played out differently", 600 + n + 1);
        }
    }

    /// More Blood's figures are shaded smooth with hard edges, the dog tags
    /// and the Huey face by face (`[light] model_shading_for`).
    #[test]
    fn more_blood_shades_its_figures_smooth() {
        let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let smooth = |m: &VoxModel| m.shade.iter().flatten().filter(|s| s[3] != 0).count();
        assert!(smooth(game.models.get("player").unwrap()) > 100, "Nick is not shaded smooth");
        assert!(smooth(game.models.get("enemy.vvx").unwrap()) > 100);
        assert!(game.models.props["dogtags.vvx"].shade.is_empty(), "the dog tags lost their crisp letters");
        assert!(game.models.get("huey.vvx").unwrap().shade.is_empty(), "the Huey is shaded smooth");
    }

    /// The title shows its props (More Blood's dog tags) and no player; the
    /// game shows the player and no props.
    #[test]
    fn the_title_shows_its_props_and_no_player() {
        let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let steel = [168, 172, 176];
        let has = |frame: &FrameData, rgb: [u8; 3]| frame.cubes().iter().filter(|i| i.color[..3] == rgb).count();
        let title = game.compose(1.0, 16.0 / 9.0, Scene::Title { time: 1.0 }, None);
        let play = game.compose(1.0, 16.0 / 9.0, Scene::Play, None);
        assert!(has(&title, steel) > 500, "no dog tags on the title");
        assert_eq!(has(&play, steel), 0, "the dog tags lie in the game");
        // Where the player stands, the game draws the player's colours and
        // the title none (the tags may lie there).
        let feet = game.game.player.body.pos;
        let size = game.models.get("player").unwrap().size.as_vec3();
        let colors: HashSet<[u8; 4]> = game.models.get("player").unwrap().frames[0].iter().map(|(_, c)| *c).collect();
        let near = |f: &FrameData| {
            f.cubes()
                .iter()
                .filter(|i| colors.contains(&i.color))
                .filter(|i| {
                    let p = Vec3::new(i.pos_size[0], i.pos_size[1], i.pos_size[2]) - feet;
                    p.x.abs() < size.x / 2.0 && p.y.abs() < size.y / 2.0 && p.z > 0.0 && p.z < size.z
                })
                .count()
        };
        assert!(near(&play) > 50, "no player in the game");
        assert_eq!(near(&title), 0, "the player stands on the title");
    }

    /// The flyby crosses now and then, and is gone in between.
    #[test]
    fn the_title_flyby_comes_and_goes() {
        let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let every = game.cfg.title_screen.flyby_every;
        assert!(game.flyby_at(1.0).is_none(), "it is there from the start");
        let a = game.flyby_at(4.0).expect("no flyby after 4 s");
        let b = game.flyby_at(5.0).expect("the flyby vanished halfway");
        assert!(b.x > a.x, "it does not cross");
        assert!(game.flyby_at(3.0 + every * 0.5 + 3.0).is_none());
        assert!(game.flyby_at(4.0 + every).is_some(), "it does not come back");
    }

    /// The player's choices turn off the screen flash and the blur.
    #[test]
    fn options_turn_off_flashes_and_blur() {
        let mut game = flat_game(64);
        game.cfg.effects.tilt_shift = 0.5;
        game.game.screen_flash = 1.0;
        let f = game.compose(1.0, 1.0, Scene::Play, None);
        assert!(f.screen_flash > 0.0 && f.tilt_shift > 0.0);
        game.options = Options { shake: false, flashes: false, blur: false, ..Options::default() };
        let f = game.compose(1.0, 1.0, Scene::Play, None);
        assert_eq!((f.screen_flash, f.tilt_shift), (0.0, 0.0));
    }

    /// Stepping over flowers and rubble must not move the camera up or down.
    #[test]
    fn walking_over_bumps_keeps_the_camera_calm() {
        let mut game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
        // Single voxels in the path, like flowers and settled rubble.
        let start = game.game.player.body.pos.floor().as_ivec3();
        for i in 1..12 {
            game.game.world.set(start + IVec3::new(-6 * i, 0, 0), 1);
        }
        let walk = Input { left: true, ..Input::default() };
        let (mut steps, mut max_cam) = (0, 0.0f32);
        for _ in 0..240 {
            let cam_before = game.game.cam_target.z;
            let z_before = game.game.player.body.pos.z;
            game.game.step(&walk);
            if game.game.player.body.pos.z > z_before {
                steps += 1;
            }
            max_cam = max_cam.max((game.game.cam_target.z - cam_before).abs());
        }
        println!("step-ups {steps}, max camera rise per tick {max_cam:.3}");
        assert!(steps > 0, "the path should cross something to step over");
        assert_eq!(max_cam, 0.0, "camera height moves {max_cam} voxels in one tick");
    }
}



