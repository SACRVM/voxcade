//! The game API: what a game gives the engine. The engine (`session::Session`)
//! loads a game folder (its `conf.lua`, the level, the models, the sounds,
//! the music, the title), runs the fixed ticks, draws the world, the title
//! and the menus, records and plays replays; a game, written in Rust as a
//! type implementing `Game`, brings its rules: what its input means, what
//! lives in its world besides voxels, how that moves and what it draws.
//! More Blood is `shooter::Shooter`; `examples/rust_digger` is a small one.
//!
//! A game holds the engine's simulation core (`sim::Sim`: the world, bodies,
//! explosions, debris, sounds) and steps it in its own `step`. Everything
//! is deterministic: no clock, randomness only from `Sim::rng`.

use std::path::Path;

use anyhow::Result;

use crate::audio::SoundDef;
use crate::config::{GameConfig, UiConfig};
use crate::controls::Bindings;
use crate::font::DisplayFont;
use crate::input::Input;
use crate::render::Instance;
use crate::session::{Drawing, Models};
use crate::sim::{Body, Sim};
use crate::world::{Marker, World};

/// A model a game draws: its name (what the game calls it by), its file in
/// the game folder, and whether its shape counts for the simulation (bodies
/// collide and burst by it: its file is part of the game data a replay
/// depends on).
pub struct ModelSpec {
    pub name: String,
    pub file: String,
    pub shaped: bool,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Won,
    Lost,
}

/// What a game gets to make itself from its folder.
pub struct Setup<'a> {
    /// The game's files (`crate::files`): its folder, or a set sealed.
    pub files: &'a crate::files::Files,
    pub cfg: &'a GameConfig,
    /// The first level's world, as built (`levels.list[0]`).
    pub world: World,
    /// The level's markers (`[markers]` gives the colours that place each
    /// kind of thing; `kind` is the kind's place in `sim::things::defs`).
    pub markers: Vec<Marker>,
    pub models: &'a Models,
    /// Every level of the game, in order (`crate::levels`).
    pub levels: std::sync::Arc<crate::levels::Levels>,
    /// What lies in its save slots (`c.saves.slots` of them, from 1), and
    /// the saves themselves (a load reads them when it is asked for).
    pub slots: Vec<Option<crate::saves::SlotInfo>>,
    pub slot_texts: std::collections::BTreeMap<u32, std::sync::Arc<str>>,
    /// A replay's loads: what each load reads in turn, not the slots.
    pub replayed: Vec<std::sync::Arc<str>>,
    /// The songs of `c.audio.music`, in order: their names and beats.
    pub songs: Vec<crate::audio::music::Timing>,
}

/// The song a game plays of its own choice (`music:play`): its place
/// among `c.audio.music`, and the beats since it began (on and on through
/// its loops).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MusicCue {
    pub song: usize,
    pub beats: f64,
    /// The game's tension, 0..1: the song's layers follow it (`#layer`).
    pub intensity: f32,
}

/// A change of the game's song as the audio needs it (`music:play`): one
/// asked and not begun yet, with its crossfade (None inside: the game's
/// `c.audio.crossfade`), and the crossfade of the one that began last.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicSwitch {
    pub pending: Option<Option<f32>>,
    pub began: Option<f32>,
}

/// A seat after the first as a tick finds it (`Game::set_seats`): what its
/// player holds, or nobody sitting there, and what they play with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seat {
    pub input: Option<Input>,
    pub controls: crate::controls::Controls,
}

/// What the machine drawing has of the game (`Session::set_local`): the
/// seats played at it and its player's language. Only its pictures follow
/// it (`frame.players`, the keys in braces and `controls` in `on_draw`,
/// `world.language` there), never the game, so machines may differ in it
/// over the network. At one machine every taken seat and seat 1's language
/// (the default).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Local {
    /// The seats played at this machine, in order; None: every taken seat.
    pub seats: Option<Vec<usize>>,
    /// Its player's language; None: seat 1's (`Controls::language`).
    pub language: Option<String>,
}

impl Local {
    /// Whether seat `seat` is played at this machine.
    pub fn has(&self, seat: usize) -> bool {
        self.seats.as_ref().is_none_or(|s| s.contains(&seat))
    }
}

pub trait Game: Sized + 'static {
    /// Its whole state as it stands at a tick (`Game::snapshot`).
    type Snapshot;

    /// Sets its config, starting from the engine's defaults: the
    /// `conf.lua` in its folder (`function conf(c) ... end`), if there is
    /// one; a Rust game may set it in code instead.
    fn conf(files: &crate::files::Files, cfg: &mut GameConfig) -> Result<()> {
        crate::lua::conf::run(files, cfg)
    }

    /// The sections it adds to the config besides the engine's (they are
    /// left to it, `GameConfig::game`, and count for its replays).
    fn sections(_cfg: &GameConfig) -> Vec<&'static str> {
        Vec::new()
    }

    /// The behaviors its kinds of things may have (`[things.NAME]
    /// behavior`; markers place the things by their kind's name).
    fn behaviors(_cfg: &GameConfig) -> Vec<&'static str> {
        Vec::new()
    }

    /// Its actions besides the four directions: the input's bits, the keys
    /// bound to them and the names replays write.
    fn actions(_cfg: &GameConfig) -> Vec<&'static str> {
        Vec::new()
    }

    /// The keys and gamepad buttons for its actions to start with.
    fn default_keys(_cfg: &GameConfig) -> Bindings {
        Bindings::default()
    }

    fn default_pad(_cfg: &GameConfig) -> Bindings {
        Bindings::default()
    }

    /// The sounds its code brings by name (a kit's, the engine's: files
    /// built in, `audio::sound::builtin`). The game's own files in
    /// `sounds/` (`GameConfig::sounds`) replace them by name and add to
    /// them.
    fn sounds(_cfg: &GameConfig) -> Vec<(&'static str, SoundDef)> {
        Vec::new()
    }

    /// The models it draws, from its config.
    fn models(_cfg: &GameConfig) -> Result<Vec<ModelSpec>> {
        Ok(Vec::new())
    }

    /// Its level made in code instead of loaded from `[world] file`
    /// (None: the file). It is made anew when its config changes; replays
    /// know it by the config only, so a change to the code that makes it
    /// needs `check --update`.
    fn level(_cfg: &GameConfig) -> Result<Option<World>> {
        Ok(None)
    }

    /// Makes it from its folder.
    fn load(setup: Setup) -> Result<Self>;

    /// Keeps its whole state as it stands, to bring it back later
    /// (`restore`): going back in a replay, rollback over a network, saving.
    /// A Rust game whose state is a value clones itself (the world shares
    /// its chunks with the copy until one side writes; keep configs and
    /// models behind an `Arc`). Fails when the state cannot be kept.
    fn snapshot(&self) -> Result<Self::Snapshot>;

    /// Brings a kept state back, as often as wanted.
    fn restore(&mut self, kept: &Self::Snapshot);

    /// Its code, if the game folder holds it (a Lua game's scripts): names
    /// and what counts of them for replays (`Session::fingerprint`).
    fn code(&self) -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }

    /// The file in its folder its config comes from, read anew when it
    /// changes (none: the config is the game's Rust code).
    fn config_file() -> Option<&'static str> {
        Some("conf.lua")
    }

    /// Whether the game sets its sun as it runs (`frame:daylight`): its
    /// meshes then hold every face a sun above the horizon can light.
    fn moves_sun(&self) -> bool {
        false
    }

    /// Whether the game sets its camera for a picture (`frame:camera`):
    /// its meshes then hold every face open to the air, whichever way a
    /// camera may look at it, and its models are left out of a picture
    /// only once its camera is known.
    fn moves_camera(&self) -> bool {
        false
    }

    /// The daylight of the level being played, as its file sets it
    /// (`[light]`): what it leaves out is `c.light`'s. Only the picture.
    fn daylight(&self) -> crate::session::Daylight {
        Default::default()
    }

    /// The suns every level of the game sets (`[light]`'s `sun`): the
    /// meshes keep the faces each lights.
    fn suns(&self) -> Vec<glam::Vec3> {
        Vec::new()
    }

    /// A file of its code changed while it runs: takes it in, keeping the
    /// game's state; whether the file was its code.
    fn reload_code(&mut self, _files: &crate::files::Files, _file: &Path) -> Result<bool> {
        Ok(false)
    }

    /// The values its code keeps outside its state (a Lua game: the locals
    /// of `main.lua` its functions keep), by name, each as one number:
    /// `voxcade check` names those that changed when the game runs
    /// differently from a snapshot.
    fn hidden(&self) -> Vec<(String, u64)> {
        Vec::new()
    }

    /// Its things and state as JSON for tools (`voxcade inspect`, the MCP
    /// tool `things`); nothing by default.
    fn inspect(&self) -> serde_json::Value {
        serde_json::Value::Null
    }

    /// What stopped it (a script's error: where and what); it waits until
    /// that is mended (`reload_code`).
    fn halted(&self) -> Option<String> {
        None
    }

    /// Whether it was stopped while it started (a Lua game's
    /// `on_world_start` or first `on_level_start`): mended, it starts
    /// afresh from its files rather than going on from a half-made start.
    fn start_failed(&self) -> bool {
        false
    }

    /// Why the level is about to be left, when the game asked for a change
    /// at the end of the tick just played (`world:go_to` and the like):
    /// the session fades out (in a window) and then calls `change`.
    fn leaving(&self) -> Option<&'static str> {
        None
    }

    /// Does the change `leaving` names: the level left, the next entered
    /// (or the run ended), between two ticks.
    fn change(&mut self) {}

    /// The player left the run (QUIT TO TITLE): a game that wants to know
    /// is told (a Lua game's `on_level_end("quit")`, `on_world_end("quit")`).
    fn quit(&mut self) {}

    /// The saves it made since asked (`world:save`, made where they were
    /// asked for: at a tick's end, after an `on_level_start`), each with
    /// its slot, taken: the session writes them into the slots.
    fn saving(&mut self) -> Vec<(u32, String)> {
        Vec::new()
    }

    /// The game as a save's text, now (`docs/game-model.md`, "Save games").
    fn save(&self) -> Result<String> {
        anyhow::bail!("this game does not save")
    }

    /// The saves it loaded in this run (`world:load`), in order: a replay
    /// carries them.
    fn loaded(&self) -> &[std::sync::Arc<str>] {
        &[]
    }

    /// The run goes on from a save at once, between two ticks (LOAD in the
    /// pause menu on a fresh run, a replay that began from a save).
    fn start_from(&mut self, _text: &str) -> Result<()> {
        anyhow::bail!("this game does not load saves")
    }

    /// The scenarios a run may begin from (`scenarios/NAME.lua`,
    /// `docs/game-model.md`): each one's name and what it is about.
    fn scenarios(&mut self) -> Result<Vec<(String, String)>> {
        Ok(Vec::new())
    }

    /// A fresh run goes on from the scenario `name` at once, before its
    /// first tick: to its level, then its `setup()`.
    fn start_scenario(&mut self, _name: &str) -> Result<()> {
        anyhow::bail!("this game has no scenarios")
    }

    /// A line of the game's own language run between two ticks (the
    /// console): what it gave back and printed. A mistake in it is
    /// returned, the game goes on.
    fn eval(&mut self, _code: &str) -> Result<String> {
        anyhow::bail!("this game has no console")
    }

    /// What the player plays with from the next tick on
    /// (`Session::set_controls`): input like the keys, never game state.
    fn set_controls(&mut self, _controls: &crate::controls::Controls) {}

    /// The seats after the first for the next tick (`Session::set_seat`;
    /// seat 2 first): who sits there, what they hold and play with. Input
    /// like the keys: a game of one never hears of it.
    fn set_seats(&mut self, _seats: &[Seat]) {}

    /// What the machine drawing has of the game (`Local`): its pictures
    /// only, never the game's state.
    fn set_local(&mut self, _local: &Local) {}

    /// What lies in its save slots now, and the saves (after a save).
    fn set_slots(&mut self, _slots: Vec<Option<crate::saves::SlotInfo>>, _texts: std::collections::BTreeMap<u32, std::sync::Arc<str>>) {}

    /// What its script printed since last asked (the IDE's OUTPUT).
    fn printed(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// The engine's core it steps (the world, debris, sounds, the camera).
    fn sim(&self) -> &Sim;
    fn sim_mut(&mut self) -> &mut Sim;

    /// One tick, with the keys held.
    fn step(&mut self, input: &Input);

    /// Its state as one number, the same on every machine for the same run
    /// (replays check it every second): take in everything that carries the
    /// game forward, the core's with it.
    fn state_hash(&self) -> u64;

    /// The state hash by its parts, in the order it takes them in: each
    /// part's name and the hash after it, the last `state_hash` itself.
    /// Two machines whose games ran apart learn where from the first part
    /// that differs (`docs/multiplayer.md`, "Desyncs").
    fn state_sections(&self) -> Vec<(&'static str, u64)> {
        vec![("game", self.state_hash())]
    }

    /// What it draws besides the world and the debris: its models, cubes,
    /// lights (`Drawing`).
    fn draw(&self, _out: &mut Drawing) {}

    /// Its HUD: voxels in the UI's scene (`ui`'s text and icons).
    fn hud(&self, _cfg: &UiConfig, _alpha: f32, _aspect: f32, _font: Option<&DisplayFont>) -> Vec<Instance> {
        Vec::new()
    }

    /// The body the camera keeps on screen: with `[camera] snap` the view
    /// sits on its drawn voxel.
    fn focus(&self) -> Option<&Body> {
        None
    }

    /// The bodies the camera keeps together on one screen, the first its
    /// anchor (several players: the middle of their span, `Session::view_target`);
    /// the focus alone unless said.
    fn focus_group(&self) -> Vec<&Body> {
        self.focus().into_iter().collect()
    }

    /// Won or lost, once it is.
    fn outcome(&self) -> Option<Outcome> {
        None
    }

    /// It keeps a score (the title shows the high score) and what it is.
    fn scored(&self) -> bool {
        false
    }

    fn score(&self) -> u32 {
        0
    }

    /// The lines a run gives the high score table, with whose they are (a
    /// game of several players: each player's own, or the team's in one,
    /// `c.team_score`): seat and score.
    fn scores(&self) -> Vec<(usize, u32)> {
        vec![(1, self.score())]
    }

    /// How loud and how muffled the game asks its song to sound while it
    /// plays (`music:volume`, `music:muffle`), whichever song plays: only
    /// heard, in no hash.
    fn music_mix(&self) -> crate::audio::music::Mix {
        crate::audio::music::Mix::default()
    }

    /// A step of its music's clock alone, while a level change fades (no
    /// tick runs then; the song plays on): as a tick's, the glides too.
    fn music_step(&mut self) {}

    /// The change of song asked and the crossfade of the last
    /// (`MusicSwitch`).
    fn music_switch(&self) -> MusicSwitch {
        MusicSwitch::default()
    }

    /// The song it plays of its own choice and how far in (None: the
    /// player's, as the settings chose it).
    fn music(&self) -> Option<MusicCue> {
        None
    }

    /// The songs of `c.audio.music` changed (a song or the list edited).
    fn set_songs(&mut self, _songs: &[crate::audio::music::Timing]) {}

    /// Whether it reads or picks its music (its code names `music` or
    /// `on_beat`): the songs' beats are then part of its data.
    fn uses_music(&self) -> bool {
        false
    }

    /// How hard and how long (ms) gamepads rumble after this tick.
    fn rumble(&self) -> Option<(f32, u32)> {
        None
    }

    /// One line on how it goes (the window's title, `shot`).
    fn summary(&self) -> Option<String> {
        None
    }

    /// Its kinds and weapons with the numbers in effect, a line each as
    /// `conf.lua` would write them (`voxcade kinds`): defaults included.
    fn numbers(&self) -> Vec<String> {
        Vec::new()
    }

    /// Lines `shot` prints about it.
    fn report(&self) -> Vec<String> {
        Vec::new()
    }

    /// Puts the player at (x, y) as if it started there (`shot --at`).
    fn place_player(&mut self, _x: f32, _y: f32) {}

    /// A sound file came or went while it runs (`cfg.sounds` has the
    /// game's sounds now): a game that checks sounds by name knows the new
    /// names. A mistake (a kind's sound gone) keeps the old.
    fn sounds_changed(&mut self, _cfg: &GameConfig) -> Result<()> {
        Ok(())
    }

    /// Its config changed while it runs.
    fn apply_config(&mut self, cfg: &GameConfig) -> Result<()> {
        self.sim_mut().apply_config(cfg);
        Ok(())
    }

    /// Levels read anew (a level file saved while playing) the game can
    /// take: a mistake keeps the levels it plays (as one at the load would
    /// stop it). By default every one.
    fn check_levels(&self, _levels: &crate::levels::Levels) -> anyhow::Result<()> {
        Ok(())
    }

    /// The levels were read anew (a level file saved while playing): by
    /// default the first one replaces the world.
    fn replace_levels(&mut self, levels: std::sync::Arc<crate::levels::Levels>) {
        let first = &levels.list[0];
        self.replace_world(first.world.clone(), &first.markers);
    }

    /// The level changed while it runs.
    fn replace_world(&mut self, world: World, _markers: &[Marker]) {
        self.sim_mut().replace_world(world);
    }

    /// Its models changed while it runs.
    fn reshape(&mut self, _models: &Models) {}
}
