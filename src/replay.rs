//! Replays: a run of a game, written down as the keys held at every tick
//! from a freshly loaded game. The simulation is deterministic, so that is
//! the whole run, a few kilobytes a minute. A replay also carries the
//! fingerprint of the game's data (`Session::fingerprint`) and the
//! simulation's state every second (`Game::state_hash`): one recorded for
//! other data is not played, and one that plays out differently (the
//! engine or game changed since) is caught at the second it runs apart.
//!
//! The input is written in `shot`'s script format, `keys:ticks;...`, so a
//! script is a replay without checks and every replay can go to `shot`.
//! What the player played with (`Controls`: the device, the keys as their
//! keyboard names them) is input too: written down whenever it changed.
//!
//! A run on sealed files (`crate::files`: a JAM's) is version 2: it names
//! the files it began on and those applied while it ran by their hashes, so
//! it plays on exactly those, whatever the folder holds since; the bytes
//! lie in the store (`files::Store`), or in the replay itself once kept
//! with F9 (`carried`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::controls::Controls;
use crate::files::{Changes, File, Files, Set, Store};
use crate::game::{Game, Outcome};
use crate::input::{DIRECTIONS, Input};
use crate::saves::Slots;
use crate::session::{Event, Session};

/// The simulation's state is checked this often: once a second.
pub const CHECK_EVERY: u64 = 60;
/// The newest version written: 1 for a run on the game's folder as it is,
/// 2 for one that carries its files (`files`).
const VERSION: u32 = 2;

/// A run as written to a file (TOML).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Replay {
    pub version: u32,
    /// The game's title, for people reading the file.
    pub game: String,
    /// `Session::fingerprint` of the data it was recorded on (hex).
    pub fingerprint: String,
    /// How many times the game stepped (hit stop included).
    pub ticks: u64,
    /// "won", "game over" or "quit".
    pub outcome: String,
    pub score: u32,
    /// The keys held at every step, in `shot`'s script format.
    pub input: String,
    /// `Game::state_hash` after every `CHECK_EVERY` steps (hex).
    pub checks: Vec<String>,
    /// The save the run began from (LOAD in the pause menu).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// What lay in the save slots as it began: each filled slot's first
    /// lines, by its number (`world.saves` reads the same played back).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slots: BTreeMap<String, String>,
    /// The saves it loaded (`world:load`), in order: played back, a load
    /// reads these, never the slots as they are now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loads: Vec<String>,
    /// Saves made from the pause menu, between two steps: after how many
    /// steps, into which slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu_saves: Vec<[u64; 2]>,
    /// What the player played with, from the step it changed on (the
    /// first at 0); none: nobody played (a replay from before it was kept).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<ControlsAt>,
    /// The scenario the run began from (`scenarios/NAME.lua`), by name:
    /// played back from it as it is now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// The console lines run during it, each after how many steps: played
    /// back there again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evals: Vec<EvalAt>,
    /// The seats after the first someone sat on (`c.players`): what was
    /// held there at every step, `away` while nobody sat there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seats: Vec<SeatRun>,
    /// The game's files it began on, by name and hash (a sealed set,
    /// version 2); none: the folder as it is.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: Set,
    /// Files applied while it ran (APPLY), in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies: Vec<ApplyAt>,
    /// The bytes of its files by hash (base64), for a run kept to be played
    /// on another machine (F9); else they are in the store.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub carried: BTreeMap<String, String>,
}

/// Files applied during a run: after how many steps, and how many of its
/// console lines had run before them then; each file's new hash, "" where
/// a file went.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ApplyAt {
    pub at: u64,
    #[serde(default)]
    pub after: usize,
    pub files: BTreeMap<String, String>,
}

/// What one seat after the first held through a run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SeatRun {
    /// Its number (2 and on).
    pub seat: usize,
    /// Its keys at every step in `shot`'s script format, `away` where the
    /// seat was free.
    pub input: String,
    /// What its player played with, from the step it changed on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<ControlsAt>,
}

/// A console line, after how many steps it was run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EvalAt {
    pub at: u64,
    pub code: String,
}

/// The controls from a step on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ControlsAt {
    /// After how many steps.
    pub at: u64,
    #[serde(flatten)]
    pub controls: Controls,
}

impl Replay {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let replay: Replay = toml::from_str(&text).with_context(|| format!("{} is no replay", path.display()))?;
        if !(1..=VERSION).contains(&replay.version) {
            bail!("{}: replay version {}, this engine reads 1 to {VERSION}", path.display(), replay.version);
        }
        Ok(replay)
    }

    /// A file of its by its hash: carried in it, else from `store`.
    fn file(&self, hash: &str, store: &Store) -> Result<File> {
        use base64::Engine;
        match self.carried.get(hash) {
            Some(text) => {
                let file = File::of(base64::engine::general_purpose::STANDARD.decode(text).with_context(|| format!("the carried file {hash}"))?);
                anyhow::ensure!(file.hash == hash, "the carried file {hash} is damaged");
                Ok(file)
            }
            None => store.get(hash),
        }
    }

    /// The files it began on, sealed (None: the folder as it is).
    pub fn files_in(&self, dir: &Path, store: &Store) -> Result<Option<Files>> {
        if self.files.is_empty() {
            return Ok(None);
        }
        let files = self.files.iter().map(|(name, hash)| Ok((name.clone(), self.file(hash, store).with_context(|| format!("the replay's {name}"))?))).collect::<Result<_>>()?;
        Ok(Some(Files::sealed(dir, files)))
    }

    /// Its APPLYs with their files: after how many steps and console lines.
    fn applies_in(&self, store: &Store) -> Result<Vec<(u64, usize, Arc<Changes>)>> {
        self.applies
            .iter()
            .map(|a| {
                let changes = a.files.iter().map(|(name, hash)| Ok((name.clone(), if hash.is_empty() { None } else { Some(self.file(hash, store).with_context(|| format!("the replay's {name}"))?) }))).collect::<Result<Changes>>()?;
                Ok((a.at, a.after, Arc::new(changes)))
            })
            .collect()
    }

    /// Puts the bytes of every file it names into it (from `store`), so it
    /// plays on a machine that has none of them.
    pub fn carry(&mut self, store: &Store) -> Result<()> {
        use base64::Engine;
        let hashes: Vec<String> = self.files.values().chain(self.applies.iter().flat_map(|a| a.files.values())).filter(|h| !h.is_empty()).cloned().collect();
        for hash in hashes {
            if !self.carried.contains_key(&hash) {
                let file = self.file(&hash, store)?;
                self.carried.insert(hash, base64::engine::general_purpose::STANDARD.encode(&file.bytes));
            }
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let text = format!("# Voxcade replay: {}, {} ticks, {}, score {}\n{}", self.game, self.ticks, self.outcome, self.score, toml::to_string(self)?);
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Whether it was recorded on this data.
    pub fn fits(&self, fingerprint: u64) -> bool {
        self.fingerprint == hex(fingerprint)
    }

    /// The save slots as the run began, and the saves it loaded, for this
    /// run only: a replay never reads or writes the player's.
    pub fn save_slots(&self) -> Slots {
        Slots::holding(self.slots.iter().filter_map(|(n, text)| Some((n.parse().ok()?, text.clone()))).collect()).replaying(&self.loads)
    }
}

/// The game in `dir` loaded for playing `replay` back: from its start the
/// save slots as the run had them, and the saves it loaded
/// (`Playback::new` then goes on from the save it began from).
pub fn replay_session<G: Game>(dir: &Path, replay: &Replay) -> Result<Session<G>> {
    replay_session_in(dir, replay, &Store::local())
}

/// `replay_session`, the files of a run that carries its own from `store`.
pub fn replay_session_in<G: Game>(dir: &Path, replay: &Replay, store: &Store) -> Result<Session<G>> {
    match replay.files_in(dir, store)? {
        Some(files) => Session::<G>::load_from(files, replay.save_slots()),
        None => Session::<G>::load_with(dir, replay.save_slots()),
    }
}

pub fn hex(v: u64) -> String {
    format!("{v:016x}")
}

/// Reads a script: `keys:ticks` segments joined by `;`, keys joined by `+`:
/// the directions (`left right up down`), the game's `actions`, or `none`.
pub fn parse_script(text: &str, actions: &[&str]) -> Result<Vec<(Input, u32)>> {
    parse_seat_script(text, actions)?
        .into_iter()
        .map(|(input, ticks)| input.map(|i| (i, ticks)).context("away is for the seats after the first (--input2 and on)"))
        .collect()
}

/// Reads a seat's script: as `parse_script`, and `away:ticks` while nobody
/// sits there (None).
pub fn parse_seat_script(text: &str, actions: &[&str]) -> Result<SeatSegments> {
    let mut script = Vec::new();
    for segment in text.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (keys, ticks) = segment.split_once(':').with_context(|| format!("input segment '{segment}' must be 'keys:ticks'"))?;
        let ticks = ticks.trim().parse().with_context(|| format!("ticks in '{segment}'"))?;
        if keys.trim() == AWAY {
            script.push((None, ticks));
            continue;
        }
        let mut input = Input::default();
        for key in keys.split('+').map(str::trim).filter(|k| !k.is_empty() && *k != "none") {
            if let Some(d) = DIRECTIONS.iter().position(|d| *d == key) {
                input.set_direction(d, true);
            } else if let Some(a) = actions.iter().position(|a| *a == key) {
                input.hold(a, true);
            } else {
                bail!("unknown key '{key}' (use {} {} none)", DIRECTIONS.join(" "), actions.join(" "));
            }
        }
        script.push((Some(input), ticks));
    }
    Ok(script)
}

/// A seat's script while nobody sits there.
const AWAY: &str = "away";

/// Writes runs of held keys as a script (`parse_script` reads it back),
/// runs of the same keys as one: the directions, then the game's `actions`.
pub fn write_script(script: &[(Input, u32)], actions: &[&str]) -> String {
    let seated: Vec<(Option<Input>, u32)> = script.iter().map(|&(input, ticks)| (Some(input), ticks)).collect();
    write_seat_script(&seated, actions)
}

/// Writes a seat's runs as a script, `away` where nobody sat there
/// (`parse_seat_script` reads it back).
pub fn write_seat_script(script: &[(Option<Input>, u32)], actions: &[&str]) -> String {
    let mut runs: Vec<(Option<Input>, u32)> = Vec::new();
    for &(input, ticks) in script.iter().filter(|(_, ticks)| *ticks > 0) {
        match runs.last_mut() {
            Some((last, n)) if *last == input => *n += ticks,
            _ => runs.push((input, ticks)),
        }
    }
    let segments: Vec<String> = runs
        .iter()
        .map(|&(input, ticks)| {
            let Some(input) = input else { return format!("{AWAY}:{ticks}") };
            let directions = DIRECTIONS.iter().zip(input.directions()).filter(|(_, on)| *on).map(|(d, _)| *d);
            let held = actions.iter().enumerate().filter(|(n, _)| input.held(*n)).map(|(_, a)| *a);
            let keys: Vec<&str> = directions.chain(held).collect();
            format!("{}:{ticks}", if keys.is_empty() { "none".to_string() } else { keys.join("+") })
        })
        .collect();
    segments.join(";")
}

/// The seats' scripts (`--input2` and on, a replay's seats), each played
/// into its seat tick by tick: after its end its seat stays as it was,
/// holding nothing.
#[derive(Clone, Default)]
pub struct SeatScripts {
    /// Each seat, its segments, the segment at and the ticks of it used.
    seats: Vec<(usize, SeatSegments, usize, u32)>,
}

/// A seat's input in runs: what it held (None: nobody sat there), how many
/// ticks.
type SeatSegments = Vec<(Option<Input>, u32)>;

impl SeatScripts {
    /// Reads `(seat, script)` pairs for a game with `seats` seats.
    pub fn parse(scripts: &[(usize, String)], actions: &[&str], seats: u32) -> Result<Self> {
        let mut read = Vec::new();
        for (seat, text) in scripts {
            if *seat < 2 || *seat > seats as usize {
                bail!("--input{seat}: the game has {seats} seat{} (c.players)", if seats == 1 { "" } else { "s" });
            }
            read.push((*seat, parse_seat_script(text, actions).with_context(|| format!("--input{seat}"))?, 0, 0));
        }
        Ok(Self { seats: read })
    }

    /// Sets each seat's input for the next step on `game`.
    pub fn next<G: Game>(&mut self, game: &mut Session<G>) {
        for (seat, segments, at, used) in &mut self.seats {
            let mut input = None;
            loop {
                match segments.get(*at) {
                    Some(&(held, ticks)) if *used < ticks => {
                        *used += 1;
                        input = Some(held);
                        break;
                    }
                    Some(_) => (*at, *used) = (*at + 1, 0),
                    None => break,
                }
            }
            // After the end: as the last segment left it, nothing held.
            let held = input.unwrap_or_else(|| segments.last().and_then(|(held, _)| held.map(|_| Input::default())));
            game.set_seat(*seat, held);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.seats.is_empty()
    }
}

/// A script's input tick by tick, nothing held after its end.
#[derive(Clone)]
pub struct Script {
    segments: Vec<(Input, u32)>,
    at: usize,
    used: u32,
}

impl Script {
    pub fn new(segments: Vec<(Input, u32)>) -> Self {
        Self { segments, at: 0, used: 0 }
    }

    /// The next tick's input.
    pub fn next_input(&mut self) -> Input {
        while let Some(&(input, ticks)) = self.segments.get(self.at) {
            if self.used < ticks {
                self.used += 1;
                return input;
            }
            self.at += 1;
            self.used = 0;
        }
        Input::default()
    }
}

/// Writes down a run while it is played: every tick as it was played
/// (`Session::played`: its saves and console lines, the keys every seat
/// held, the controls), and the simulation's state every second.
#[derive(Default)]
pub struct Recorder {
    script: Vec<(Input, u32)>,
    checks: Vec<String>,
    steps: u64,
    /// How the run began (the save it went on from, the slots), taken at
    /// the first step; the saves loaded and made from the menu since.
    began: bool,
    from: Option<String>,
    slots: BTreeMap<String, String>,
    loads: Vec<String>,
    menu_saves: Vec<[u64; 2]>,
    controls: Vec<ControlsAt>,
    scenario: Option<String>,
    evals: Vec<EvalAt>,
    /// The sealed files it began on and their fingerprint (None: the
    /// folder), and the files applied since.
    files: Option<(Set, u64)>,
    applies: Vec<ApplyAt>,
    /// A check due at a tick whose level change waited for its fade out:
    /// taken once the change is done, as a replay (which cuts) sees it.
    check_due: bool,
    /// The seats after the first, seat 2 first: what each held at every
    /// step (None: nobody sat there) and its player's controls.
    seats: Vec<(SeatSegments, Vec<ControlsAt>)>,
}

impl Recorder {
    /// After each `Session::step`.
    pub fn record<G: Game>(&mut self, game: &Session<G>) {
        if !self.began {
            self.began = true;
            self.from = game.started_from.clone();
            self.scenario = game.scenario.clone();
            self.slots = game.slots_at_start.iter().map(|(n, t)| (n.to_string(), t.clone())).collect();
            self.files = game.files.set().map(|set| (set, game.fingerprint().unwrap_or_default()));
        }
        if let Some(new) = game.game.loaded().get(self.loads.len()..) {
            self.loads.extend(new.iter().map(|t| t.to_string()));
        }
        if self.check_due && !game.changing() {
            self.check_due = false;
            self.checks.push(hex(game.game.state_hash()));
        }
        // A fade's frames are no ticks: the replay plays without them.
        if !game.ticked {
            return;
        }
        let tick = game.played();
        // The saves and console lines before it: after this many steps.
        self.events(&tick.events);
        if self.controls.last().is_none_or(|c| c.controls != tick.controls) {
            self.controls.push(ControlsAt { at: self.steps, controls: tick.controls.clone() });
        }
        match self.script.last_mut() {
            Some((last, ticks)) if *last == tick.input => *ticks += 1,
            _ => self.script.push((tick.input, 1)),
        }
        self.seats.resize_with(tick.seats.len(), Default::default);
        for ((script, controls), seat) in self.seats.iter_mut().zip(&tick.seats) {
            match script.last_mut() {
                Some((last, ticks)) if *last == seat.input => *ticks += 1,
                _ => script.push((seat.input, 1)),
            }
            if seat.input.is_some() && controls.last().is_none_or(|c| c.controls != seat.controls) {
                controls.push(ControlsAt { at: self.steps, controls: seat.controls.clone() });
            }
        }
        self.steps += 1;
        if self.steps.is_multiple_of(CHECK_EVERY) {
            if game.changing() {
                self.check_due = true;
            } else {
                self.checks.push(hex(game.game.state_hash()));
            }
        }
    }

    /// Writes down the saves and console lines before the next step.
    fn events(&mut self, events: &[Event]) {
        for event in events {
            match event {
                Event::Save { slot, .. } => self.menu_saves.push([self.steps, *slot as u64]),
                Event::Eval(code) => self.evals.push(EvalAt { at: self.steps, code: code.clone() }),
                Event::Apply(changes) => self.applies.push(ApplyAt {
                    at: self.steps,
                    after: self.evals.len(),
                    files: changes.iter().map(|(name, file)| (name.clone(), file.as_ref().map(|f| f.hash.clone()).unwrap_or_default())).collect(),
                }),
            }
        }
    }

    /// The run so far as a replay of `game` (the one it was recorded on).
    pub fn replay<G: Game>(&self, game: &Session<G>) -> Result<Replay> {
        // The saves and console lines since the last step too.
        let mut done = Self { menu_saves: self.menu_saves.clone(), evals: self.evals.clone(), applies: self.applies.clone(), steps: self.steps, ..Self::default() };
        done.events(game.pending());
        let outcome = match game.game.outcome() {
            Some(Outcome::Won) => "won",
            Some(Outcome::Lost) => "game over",
            None => "quit",
        };
        // A run on sealed files began on them, whatever was applied since.
        let (version, fingerprint, files) = match &self.files {
            Some((set, fingerprint)) => (2, *fingerprint, set.clone()),
            None => (1, game.fingerprint()?, Set::new()),
        };
        Ok(Replay {
            version,
            game: game.cfg.title.clone(),
            fingerprint: hex(fingerprint),
            ticks: self.steps,
            outcome: outcome.to_string(),
            score: game.game.score(),
            input: write_script(&self.script, game.actions()),
            checks: self.checks.clone(),
            from: self.from.clone(),
            slots: self.slots.clone(),
            loads: self.loads.clone(),
            menu_saves: done.menu_saves,
            controls: self.controls.clone(),
            scenario: if self.began { self.scenario.clone() } else { game.scenario.clone() },
            evals: done.evals,
            // Only the seats someone sat on.
            seats: self
                .seats
                .iter()
                .enumerate()
                .filter(|(_, (script, _))| script.iter().any(|(input, _)| input.is_some()))
                .map(|(i, (script, controls))| SeatRun { seat: i + 2, input: write_seat_script(script, game.actions()), controls: controls.clone() })
                .collect(),
            files,
            applies: done.applies,
            carried: BTreeMap::new(),
        })
    }
}

/// Plays a replay into a freshly loaded game and checks it as it goes.
#[derive(Clone)]
pub struct Playback {
    script: Script,
    checks: Vec<String>,
    ticks: u64,
    steps: u64,
    /// The saves made from the pause menu, by the step after which.
    menu_saves: Vec<[u64; 2]>,
    controls: Vec<ControlsAt>,
    /// The console lines, and how many of them ran.
    evals: Vec<EvalAt>,
    evaluated: usize,
    /// The files applied (after how many steps and console lines), and how
    /// many of them were.
    applies: Vec<(u64, usize, Arc<Changes>)>,
    applied: usize,
    /// What the seats after the first held, and played with.
    seats: SeatScripts,
    seat_controls: Vec<(usize, Vec<ControlsAt>)>,
    /// Where it ran apart, once it did.
    apart: Option<Apart>,
}

/// Where a replay stopped matching its run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Apart {
    /// Seconds into the run (checks are a second apart).
    pub second: u64,
}

impl Playback {
    /// Plays `replay` into `game`, a game loaded for it (`replay_session`:
    /// the slots and loads as the run had them, none of the player's), from
    /// the save it began from.
    pub fn new<G: Game>(replay: &Replay, game: &mut Session<G>) -> Result<Self> {
        Self::new_in(replay, game, &Store::local())
    }

    /// `new`, the files it applies from `store` (where the replay does not
    /// carry them).
    pub fn new_in<G: Game>(replay: &Replay, game: &mut Session<G>, store: &Store) -> Result<Self> {
        anyhow::ensure!(!game.slots.on_disk_now(), "a replay plays on a game loaded for it (replay_session), never on the player's save slots");
        // Nobody plays until the first step, as when it was recorded.
        let nobody = Controls::nobody(game.actions());
        game.set_controls(nobody);
        if let Some(from) = &replay.from {
            game.start_from(from)?;
        }
        if let Some(scenario) = &replay.scenario {
            game.start_scenario(scenario)?;
        }
        for &[at, slot] in &replay.menu_saves {
            if at == 0 {
                game.save_to(slot as u32)?;
            }
        }
        let script = Script::new(parse_script(&replay.input, game.actions())?);
        // Keys for the actions the game has now, those it gained since too.
        let fit = |list: &[ControlsAt]| list.iter().map(|c| ControlsAt { at: c.at, controls: c.controls.clone().with_actions(game.actions()) }).collect::<Vec<_>>();
        let controls = fit(&replay.controls);
        let scripts: Vec<(usize, String)> = replay.seats.iter().map(|s| (s.seat, s.input.clone())).collect();
        let seats = SeatScripts::parse(&scripts, game.actions(), game.cfg.players)?;
        let seat_controls = replay.seats.iter().map(|s| (s.seat, fit(&s.controls))).collect();
        let applies = replay.applies_in(store)?;
        Ok(Self { script, checks: replay.checks.clone(), ticks: replay.ticks, steps: 0, menu_saves: replay.menu_saves.clone(), controls, evals: replay.evals.clone(), evaluated: 0, applies, applied: 0, seats, seat_controls, apart: None })
    }

    pub fn done(&self) -> bool {
        self.steps >= self.ticks
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// How many steps the replay has.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// The input of the next step, and what the player played with then
    /// (set on `game`).
    pub fn next_input<G: Game>(&mut self, game: &mut Session<G>) -> Input {
        if let Some(c) = self.controls_at(self.steps) {
            game.set_controls(c.clone());
        }
        for (seat, list) in &self.seat_controls {
            if let Some(c) = list.iter().rfind(|c| c.at == self.steps) {
                game.set_seat_controls(*seat, c.controls.clone());
            }
        }
        self.seats.next(game);
        // The console lines run after these steps, run again (a mistake
        // made in one then is made again), and the files applied among
        // them, in the order they came.
        loop {
            if let Some((_, _, changes)) = self.applies.get(self.applied).filter(|(at, after, _)| *at <= self.steps && *after <= self.evaluated) {
                let changes = changes.clone();
                self.applied += 1;
                let _ = game.apply(changes);
            } else if let Some(line) = self.evals.get(self.evaluated).filter(|e| e.at <= self.steps) {
                let code = line.code.clone();
                self.evaluated += 1;
                let _ = game.eval(&code);
            } else {
                break;
            }
        }
        self.script.next_input()
    }

    /// The controls that came at `step`, if they changed there.
    fn controls_at(&self, step: u64) -> Option<&Controls> {
        self.controls.iter().rfind(|c| c.at == step).map(|c| &c.controls)
    }

    /// After each `Game::step`: whether the run still matches the replay
    /// (it says so once, where it ran apart).
    pub fn check<G: Game>(&mut self, game: &mut Session<G>) -> Result<(), Apart> {
        self.steps += 1;
        // A save from the pause menu after this step, as the run made it.
        for &[at, slot] in &self.menu_saves {
            if at == self.steps
                && let Err(e) = game.save_to(slot as u32)
            {
                eprintln!("the replay's save into slot {slot}: {e:#}");
            }
        }
        if self.apart.is_some() || !self.steps.is_multiple_of(CHECK_EVERY) {
            return Ok(());
        }
        let n = (self.steps / CHECK_EVERY) as usize;
        match self.checks.get(n - 1) {
            Some(expected) if *expected != hex(game.game.state_hash()) => {
                let apart = Apart { second: n as u64 };
                self.apart = Some(apart);
                Err(apart)
            }
            _ => Ok(()),
        }
    }

    /// One step of the game with the replay's input, checked.
    pub fn step<G: Game>(&mut self, game: &mut Session<G>) -> Result<(), Apart> {
        let input = self.next_input(game);
        game.step(&input);
        self.check(game)
    }
}

/// What playing a replay through showed.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Same,
    /// Recorded on other game data.
    OtherData,
    RunsApart(Apart),
}

/// The game's replays (`replays/` and the folders in it, by name): the
/// regression runs in `replays/tests/` and the ones the title shows.
pub fn game_replays(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "toml") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&dir.join("replays"), &mut out);
    out
}

/// Plays a replay through a freshly loaded game without a window.
pub fn play_through<G: Game>(dir: &Path, replay: &Replay) -> Result<Verdict> {
    play_going_back::<G>(dir, replay, None, &mut Costs::default())
}

/// `play_through`, going back every `every` ticks if given (`check
/// --rollback`), what keeping and bringing back the game cost added up in
/// `costs`.
fn play_going_back<G: Game>(dir: &Path, replay: &Replay, every: Option<u64>, costs: &mut Costs) -> Result<Verdict> {
    let mut game = replay_session::<G>(dir, replay)?;
    // Begun as the run began (a save, a scenario, whose file counts):
    // then the data it was recorded on can be told.
    let playback = Playback::new(replay, &mut game)?;
    if !replay.fits(game.fingerprint()?) {
        return Ok(Verdict::OtherData);
    }
    play::<G>(&mut game, playback, every, costs)
}

/// Every this many seconds of a replay `check` keeps the game, plays the
/// next second, brings the game back and plays the second again (`guard`).
const GUARD_EVERY: u64 = 5;

/// What keeping and bringing back a game cost while going back (`check
/// --rollback`): how often, and the time each took in all.
#[derive(Default)]
pub struct Costs {
    pub times: u32,
    pub keep: std::time::Duration,
    pub restore: std::time::Duration,
}

impl Costs {
    /// A line of the average costs.
    fn said(&self) -> String {
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0 / self.times.max(1) as f64;
        format!("kept and brought back {} times: {:.3} ms to keep, {:.3} ms to bring back", self.times, ms(self.keep), ms(self.restore))
    }
}

/// Plays a replay through `game`, whatever data it was recorded on, the
/// game drawn after every tick as in a window (`Session::look`). Every
/// 5 s it plays a second twice from a snapshot (`guard`); going back
/// `every` ticks, it plays every stretch of that many twice instead, as
/// rollback over a network would.
fn play<G: Game>(game: &mut Session<G>, mut playback: Playback, every: Option<u64>, costs: &mut Costs) -> Result<Verdict> {
    let (often, span) = every.map_or((GUARD_EVERY * CHECK_EVERY, CHECK_EVERY), |k| (k, k));
    while !playback.done() {
        if playback.steps.is_multiple_of(often) && playback.steps + span <= playback.ticks {
            guard(game, &playback, span, costs)?;
        }
        let step = playback.step(game);
        // Drawn as the window draws it (without the picture): a mistake in
        // the game's drawing stops the replay where it would stop the game.
        game.look();
        game.stopped()?;
        if let Err(apart) = step {
            return Ok(Verdict::RunsApart(apart));
        }
    }
    Ok(Verdict::Same)
}

/// The hidden-state guard: from a snapshot, the next `span` steps of the
/// replay twice (its input, controls and console lines); the same input
/// must give the same states. A game that runs differently the second time
/// keeps state its snapshots miss (a Lua game: a local of `main.lua` its
/// functions change, named from `Game::hidden`), and its replays could not
/// be trusted, nor could going back over a network. What keeping and
/// bringing back cost goes into `costs`.
fn guard<G: Game>(game: &mut Session<G>, playback: &Playback, span: u64, costs: &mut Costs) -> Result<()> {
    let started = std::time::Instant::now();
    let kept = game.snapshot()?;
    costs.keep += started.elapsed();
    costs.times += 1;
    let before = game.game.hidden();
    let states = |game: &mut Session<G>| -> Result<Vec<u64>> {
        let mut ahead = playback.clone();
        let mut out = Vec::new();
        for _ in 0..span {
            let input = ahead.next_input(game);
            game.step(&input);
            ahead.steps += 1;
            game.stopped()?;
            out.push(game.game.state_hash());
        }
        Ok(out)
    };
    let first = states(game)?;
    let after = game.game.hidden();
    let started = std::time::Instant::now();
    game.restore(&kept);
    costs.restore += started.elapsed();
    let second = states(game)?;
    game.restore(&kept);
    let Some(apart) = first.iter().zip(&second).position(|(a, b)| a != b) else {
        return Ok(());
    };
    if span != CHECK_EVERY {
        bail!("going back to tick {} and playing on, tick {} came out differently", playback.steps, playback.steps + apart as u64 + 1);
    }
    let at = playback.steps / CHECK_EVERY;
    let mut changed: Vec<&str> = after.iter().filter(|v| !before.contains(v)).map(|(name, _)| name.as_str()).collect();
    changed.sort_unstable();
    changed.dedup();
    match changed.as_slice() {
        [] => bail!("the game runs differently from a snapshot (after {at} s): it keeps state its snapshots miss"),
        names => bail!(
            "{} changed while the game ran (after {at} s): game state belongs in a state (world.state, player.state, thing.state), where snapshots and replays see it",
            names.iter().map(|n| format!("'{n}' (main.lua)")).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Plays every replay of a game and says how each went; with `update`,
/// records their checks and fingerprint anew from their input (after a
/// change to the game or engine that was meant). Whether all played the
/// same.
pub fn check<G: Game>(dir: &Path, update: bool) -> Result<bool> {
    let replays = game_replays(dir);
    if replays.is_empty() {
        println!("{}: no replays", dir.display());
        return Ok(true);
    }
    let mut all = true;
    for path in &replays {
        let replay = Replay::load(path)?;
        let name = path.strip_prefix(dir).unwrap_or(path).display().to_string();
        if update {
            let fresh = rerecord::<G>(dir, &replay)?;
            fresh.save(path)?;
            println!("{name}: recorded anew ({} ticks, {}, score {})", fresh.ticks, fresh.outcome, fresh.score);
            continue;
        }
        let verdict = play_through::<G>(dir, &replay)?;
        match verdict {
            Verdict::Same => println!("{name}: same ({} s)", replay.ticks / CHECK_EVERY),
            Verdict::OtherData => println!("{name}: recorded on other game data (`voxcade check --update` records it anew)"),
            Verdict::RunsApart(Apart { second }) => println!("{name}: runs apart after {second} s"),
        }
        all &= verdict == Verdict::Same;
    }
    Ok(all)
}

/// Plays every replay of a game as `check` does, going back every `every`
/// ticks and playing on from there (rollback over a network): each must
/// come out as recorded. Says what keeping and bringing back the game cost.
pub fn check_going_back<G: Game>(dir: &Path, every: u64) -> Result<bool> {
    anyhow::ensure!(every > 0, "--rollback: going back a number of ticks, 1 or more");
    let replays = game_replays(dir);
    if replays.is_empty() {
        println!("{}: no replays", dir.display());
        return Ok(true);
    }
    let mut all = true;
    for path in &replays {
        let replay = Replay::load(path)?;
        let name = path.strip_prefix(dir).unwrap_or(path).display().to_string();
        let mut costs = Costs::default();
        let verdict = play_going_back::<G>(dir, &replay, Some(every), &mut costs)?;
        match verdict {
            Verdict::Same => println!("{name}: same ({} s), going back every {every} ticks; {}", replay.ticks / CHECK_EVERY, costs.said()),
            Verdict::OtherData => println!("{name}: recorded on other game data (`voxcade check --update` records it anew)"),
            Verdict::RunsApart(Apart { second }) => println!("{name}: runs apart after {second} s"),
        }
        all &= verdict == Verdict::Same;
    }
    Ok(all)
}

/// Plays every replay of a game whatever data it was recorded on and, only
/// if every one runs exactly as recorded (every second's check), writes the
/// data's fingerprint into them, changing nothing else in the files: after
/// a change to the game's files that was meant to leave the game as it was
/// (a new layout of `conf.lua`). If any runs apart or has no checks to
/// tell, nothing is written. Whether all were refitted.
pub fn refit<G: Game>(dir: &Path) -> Result<bool> {
    refit_saying::<G>(dir, &mut |line| println!("{line}"))
}

/// `refit`, each line of what it does said to `say` (the IDE's upgrade
/// page shows them).
pub fn refit_saying<G: Game>(dir: &Path, say: &mut dyn FnMut(String)) -> Result<bool> {
    let replays = game_replays(dir);
    if replays.is_empty() {
        say(format!("{}: no replays", dir.display()));
        return Ok(true);
    }
    let mut fitted = Vec::new();
    let mut all = true;
    for path in &replays {
        let replay = Replay::load(path)?;
        let name = path.strip_prefix(dir).unwrap_or(path).display().to_string();
        let mut game = replay_session::<G>(dir, &replay)?;
        let playback = Playback::new(&replay, &mut game)?;
        let fingerprint = hex(game.fingerprint()?);
        if replay.checks.is_empty() {
            say(format!("{name}: no checks, nothing to tell it by"));
            all = false;
            continue;
        }
        match play::<G>(&mut game, playback, None, &mut Costs::default())? {
            Verdict::RunsApart(Apart { second }) => {
                say(format!("{name}: runs apart after {second} s"));
                all = false;
            }
            _ if replay.fingerprint == fingerprint => say(format!("{name}: same ({} s), fits already", replay.ticks / CHECK_EVERY)),
            _ => {
                say(format!("{name}: same ({} s), fingerprint {} -> {fingerprint}", replay.ticks / CHECK_EVERY, replay.fingerprint));
                fitted.push((path, replay.fingerprint, fingerprint));
            }
        }
    }
    if !all {
        say("nothing written: a replay that does not run the same keeps its fingerprint".into());
        return Ok(false);
    }
    for (path, old, new) in fitted {
        let text = std::fs::read_to_string(path)?;
        let line = format!("fingerprint = \"{old}\"");
        if text.matches(&line).count() != 1 {
            bail!("{}: no single line {line}", path.display());
        }
        std::fs::write(path, text.replace(&line, &format!("fingerprint = \"{new}\""))).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(true)
}

/// The replay's input played into a fresh game and recorded again.
fn rerecord<G: Game>(dir: &Path, replay: &Replay) -> Result<Replay> {
    let mut game = replay_session::<G>(dir, replay)?;
    let mut playback = Playback::new(replay, &mut game)?;
    let mut recorder = Recorder::default();
    while !playback.done() {
        let input = playback.next_input(&mut game);
        game.step(&input);
        game.look();
        game.stopped()?;
        recorder.record(&game);
        // Only for the menu's saves: the old checks are what is renewed.
        let _ = playback.check(&mut game);
    }
    recorder.replay(&game)
}

/// A run is kept in the config folder from this many steps on (10 s).
const KEEP_FROM: u64 = 600;
/// How many kept runs stay there (the oldest go).
const KEEP: usize = 20;
/// Steps a run goes on after it is won or lost (the end plays out).
const END_AFTER: u64 = 180;

/// The runs played in the window: the one being recorded and the last one
/// finished. Finished runs of 10 s or more are kept in the user's config
/// folder (`settings::replays_for`), the newest 20; `keep` puts a run in
/// the game's own `replays/` (F9), where the title shows it.
pub struct Runs {
    game_dir: PathBuf,
    kept: PathBuf,
    recording: Option<Recorder>,
    /// Steps since the run was won or lost.
    over: u64,
    last: Option<Replay>,
}

impl Runs {
    pub fn new(game_dir: &Path) -> Self {
        Self { game_dir: game_dir.to_path_buf(), kept: crate::settings::replays_for(game_dir), recording: None, over: 0, last: None }
    }

    /// A run starts on a freshly loaded game.
    pub fn start<G: Game>(&mut self, game: &Session<G>) {
        self.end(game);
        self.recording = (!game.touched).then(Recorder::default);
        self.over = 0;
    }

    /// After each `Session::step`.
    pub fn step<G: Game>(&mut self, game: &Session<G>) {
        let Some(recorder) = &mut self.recording else { return };
        recorder.record(game);
        if game.game.outcome().is_some() {
            self.over += 1;
            if self.over >= END_AFTER {
                self.end(game);
            }
        }
    }

    /// The run is over (or left): kept if long enough, and the last run.
    pub fn end<G: Game>(&mut self, game: &Session<G>) {
        let Some(recorder) = self.recording.take() else { return };
        if game.touched {
            println!("run not kept: the game's files changed during it");
            return;
        }
        if game.game.halted().is_some() {
            println!("run not kept: the game stopped with an error");
            return;
        }
        match recorder.replay(game) {
            Ok(replay) => {
                if replay.ticks >= KEEP_FROM
                    && let Err(e) = self.keep_in_config(&replay)
                {
                    eprintln!("keeping the run: {e:#}");
                }
                self.last = Some(replay);
            }
            Err(e) => eprintln!("recording the run: {e:#}"),
        }
    }

    fn keep_in_config(&self, replay: &Replay) -> Result<()> {
        self.keep_named(replay, &stamp())
    }

    /// Keeps a run played elsewhere than by this recorder (a network
    /// game's guest: its recording, the host's) among the runs in the
    /// config folder, if it is long enough.
    pub fn keep_also(&self, replay: &Replay, tag: &str) {
        if replay.ticks >= KEEP_FROM
            && let Err(e) = self.keep_named(replay, &format!("{}-{tag}", stamp()))
        {
            eprintln!("keeping the run: {e:#}");
        }
    }

    fn keep_named(&self, replay: &Replay, name: &str) -> Result<()> {
        replay.save(&self.kept.join(format!("{name}.toml")))?;
        let mut kept: Vec<PathBuf> = std::fs::read_dir(&self.kept)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        kept.sort();
        let gone = kept.len().saturating_sub(KEEP);
        for old in &kept[..gone] {
            std::fs::remove_file(old)?;
        }
        Ok(())
    }

    /// Puts the run being played, or else the last one, in the game's
    /// `replays/` folder (F9); where it went.
    pub fn keep<G: Game>(&self, game: &Session<G>) -> Result<PathBuf> {
        let mut replay = match &self.recording {
            Some(recorder) if !game.touched => recorder.replay(game)?,
            Some(_) => bail!("the game's files changed during this run"),
            None => self.last.clone().context("no run to keep yet")?,
        };
        // A run on sealed files takes its bytes along.
        replay.carry(&Store::local())?;
        let path = self.game_dir.join("replays").join(format!("{}-{}.toml", stamp(), replay.score));
        replay.save(&path)?;
        Ok(path)
    }
}

/// The title's attract mode: after some seconds on the title without a key
/// pressed, the game's replays play one after another, as the arcade
/// machines showed theirs. Only those in `replays/` itself (put there with
/// F9), not the regression runs in its folders; read afresh each time, so
/// a run just kept is among them.
pub struct Attract {
    game_dir: PathBuf,
    idle: f32,
    next: usize,
}

impl Attract {
    pub fn new(game_dir: &Path) -> Self {
        Self { game_dir: game_dir.to_path_buf(), idle: 0.0, next: 0 }
    }

    /// A key, button or stick was touched: the wait starts over.
    pub fn input(&mut self) {
        self.idle = 0.0;
    }

    /// `seconds` more on the title without input; whether `after` seconds
    /// have passed (0: never).
    pub fn wait(&mut self, seconds: f32, after: f32) -> bool {
        self.idle += seconds;
        after > 0.0 && self.idle >= after
    }

    /// The next replay recorded on this data (`fingerprint`), round the
    /// list; the wait starts over.
    pub fn next(&mut self, fingerprint: u64) -> Option<Replay> {
        self.idle = 0.0;
        let mut paths: Vec<PathBuf> = std::fs::read_dir(self.game_dir.join("replays"))
            .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "toml")).collect())
            .unwrap_or_default();
        paths.sort();
        for _ in 0..paths.len() {
            let path = &paths[self.next % paths.len()];
            self.next += 1;
            match Replay::load(path) {
                Ok(replay) if replay.fits(fingerprint) => return Some(replay),
                Ok(_) => {}
                Err(e) => eprintln!("{e:#}"),
            }
        }
        None
    }
}

/// The time now (UTC) as a file name that sorts by time: `20261015-140312`.
fn stamp() -> String {
    stamp_of(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64)
}

/// Seconds since 1970 (UTC) as `stamp` writes them.
fn stamp_of(secs: i64) -> String {
    let (days, day) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970 to a date (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", day / 3600, day / 60 % 60, day % 60)
}

/// Lists the runs kept for a game in the config folder.
pub fn list<G: Game>(game_dir: &Path) -> Result<()> {
    let kept = crate::settings::replays_for(game_dir);
    let fingerprint = Session::<G>::load(game_dir)?.fingerprint()?;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&kept)
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "toml")).collect())
        .unwrap_or_default();
    paths.sort();
    if paths.is_empty() {
        println!("no runs kept yet in {}", kept.display());
    }
    for path in paths {
        match Replay::load(&path) {
            Ok(r) => {
                let data = match &r.scenario {
                    // Its scenario's file counts for its data: told by playing it.
                    Some(name) => format!("  (from the scenario {name})"),
                    None if r.fits(fingerprint) => String::new(),
                    None => "  (other game data)".into(),
                };
                println!("{}  {:>4} s  {:<9}  score {:>6}{data}", path.display(), r.ticks / CHECK_EVERY, r.outcome, r.score);
            }
            Err(e) => println!("{}: {e:#}", path.display()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shooter::Shooter;
    use crate::shooter::ACTIONS;
    /// The shooter's keys, to build its input with.
    use crate::shooter::Controls as Keys;

    /// Copies a game folder for a test to change.
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                copy(&path, &to.join(entry.file_name()));
            } else {
                std::fs::copy(&path, to.join(entry.file_name())).unwrap();
            }
        }
    }

    /// Steps `ticks` with the player walking right, recorded.
    fn walk(game: &mut Session<crate::Voxcade>, recorder: &mut Recorder, ticks: u32) {
        let right = Input { right: true, ..Input::default() };
        for _ in 0..ticks {
            game.step(&right);
            game.look();
            recorder.record(game);
        }
    }

    /// A run on sealed files (a JAM's) plays on those files whatever the
    /// folder holds since, the files applied while it ran too: its replay,
    /// version 2, comes out the same from the store, and from itself once
    /// it carries its bytes on a machine whose store has none of them. The
    /// files applied took effect with the game's state kept, and the run
    /// counts as helped.
    #[test]
    fn a_run_on_sealed_files_replays_with_its_applies() {
        let tmp = std::env::temp_dir().join(format!("voxcade-sealed-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let dir = tmp.join("game");
        copy(Path::new("templates/empty"), &dir);
        let store = Store::at(&tmp.join("store"));
        let (_, files) = Files::seal(&dir, &store).unwrap();
        let mut game = Session::<crate::Voxcade>::load_from(files, Slots::default()).unwrap();
        let x = |game: &Session<crate::Voxcade>| game.game.focus().unwrap().pos.x;
        let mut recorder = Recorder::default();
        let start = x(&game);
        // The world is 96 voxels across, the player starts in its middle.
        walk(&mut game, &mut recorder, 30);
        let slow = x(&game) - start;
        assert!(slow > 10.0, "the player walks: {slow}");
        // The folder changes on disk: nothing of it reaches the game ...
        let main = std::fs::read_to_string(dir.join("main.lua")).unwrap();
        std::fs::write(dir.join("main.lua"), main.replace("p:walk(p.input.dir, 40)", "p:walk(p.input.dir, 80)")).unwrap();
        assert_eq!(game.reload(&dir.join("main.lua")).unwrap(), crate::session::Reloaded::Nothing, "a sealed game is not touched by the folder");
        // ... until it is applied, between two ticks; a console line too.
        let _ = game.eval("player.state.score = 5");
        let changes = game.files.changes_since(&store).unwrap();
        assert_eq!(changes.keys().collect::<Vec<_>>(), ["main.lua"]);
        game.apply(Arc::new(changes)).unwrap();
        assert_eq!(game.trouble(), None);
        let before = x(&game);
        walk(&mut game, &mut recorder, 15);
        let fast = x(&game) - before;
        assert!((fast - slow).abs() < 1.0, "the new main.lua walks twice as fast, as far in half the time: {slow} then {fast}");
        walk(&mut game, &mut recorder, 60);
        assert!(!game.touched && game.helped(), "an applied run is kept, and counts as helped");
        let end = game.game.state_hash();
        let replay = recorder.replay(&game).unwrap();
        assert_eq!(replay.version, 2);
        assert_eq!(replay.applies.len(), 1);
        assert_eq!(replay.applies[0].after, 1, "applied after the console line");
        assert!(replay.files.contains_key("main.lua") && replay.files.contains_key("conf.lua"));

        // The folder moves on: the replay plays on its own files.
        std::fs::write(dir.join("main.lua"), "error('not these files')").unwrap();
        let path = tmp.join("run.toml");
        replay.save(&path).unwrap();
        let replay = Replay::load(&path).unwrap();
        let play_back = |replay: &Replay, store: &Store, every: Option<u64>| {
            let mut again = replay_session_in::<crate::Voxcade>(&dir, replay, store).unwrap();
            let playback = Playback::new_in(replay, &mut again, store).unwrap();
            assert!(replay.fits(again.fingerprint().unwrap()), "begun on the files it was recorded on");
            let verdict = play::<crate::Voxcade>(&mut again, playback, every, &mut Costs::default()).unwrap();
            (verdict, again.game.state_hash())
        };
        assert_eq!(play_back(&replay, &store, None), (Verdict::Same, end));
        // Going back again and again across the APPLY (rollback): the state
        // kept before it comes back with the files and code before it.
        assert_eq!(play_back(&replay, &store, Some(8)), (Verdict::Same, end));
        // On a machine with none of its files, carrying them.
        let empty = Store::at(&tmp.join("empty"));
        assert!(replay_session_in::<crate::Voxcade>(&dir, &replay, &empty).is_err(), "without its files it cannot play");
        let mut carrying = replay.clone();
        carrying.carry(&store).unwrap();
        carrying.save(&path).unwrap();
        assert_eq!(play_back(&Replay::load(&path).unwrap(), &empty, None), (Verdict::Same, end));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A run of 10 s or more is kept in the config folder, the newest 20
    /// stay; a shorter one is only the last run; F9 puts a run in the game's
    /// folder, and it plays back; a run during which files reloaded is not
    /// kept.
    #[test]
    fn runs_are_kept_and_the_oldest_go() {
        let tmp = std::env::temp_dir().join(format!("voxcade-runs-{}", std::process::id()));
        let kept = tmp.join("kept");
        std::fs::create_dir_all(&kept).unwrap();
        for n in 0..21 {
            std::fs::write(kept.join(format!("1999{n:04}-000000.toml")), "").unwrap();
        }
        let mut runs = Runs { game_dir: tmp.join("game"), kept: kept.clone(), recording: None, over: 0, last: None };
        let files = || std::fs::read_dir(&kept).unwrap().count();
        let play = |runs: &mut Runs, steps: u32| {
            let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
            runs.start(&game);
            for t in 0..steps {
                let input: Input = Keys { up: true, fire: t % 40 < 10, ..Keys::default() }.into();
                game.step(&input);
                runs.step(&game);
            }
            game
        };
        let game = play(&mut runs, 650);
        runs.end(&game);
        assert_eq!(files(), KEEP, "the oldest were not taken away");
        assert!(!kept.join("19990000-000000.toml").exists() && !kept.join("19990001-000000.toml").exists());
        let long = runs.last.clone().unwrap();
        assert_eq!(long.ticks, 650);

        let game = play(&mut runs, 100);
        runs.end(&game);
        assert_eq!(runs.last.as_ref().unwrap().ticks, 100);
        assert_eq!(files(), KEEP, "a short run was kept");

        let kept_by_hand = runs.keep(&game).unwrap();
        assert!(kept_by_hand.starts_with(tmp.join("game").join("replays")));
        let back = Replay::load(&kept_by_hand).unwrap();
        assert_eq!(play_through::<Shooter>(Path::new("games/moreblood"), &back).unwrap(), Verdict::Same);

        let mut game = play(&mut runs, 700);
        game.touched = true;
        let before = runs.last.clone();
        runs.end(&game);
        assert_eq!(runs.last, before, "a run with reloaded files counted");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// The title waits its time without input, then plays the game's own
    /// replays in turn, those for other data left out, the regression runs
    /// too; a key starts the wait over.
    #[test]
    fn the_title_plays_the_games_own_replays_in_turn() {
        let tmp = std::env::temp_dir().join(format!("voxcade-attract-{}", std::process::id()));
        let replays = tmp.join("replays");
        std::fs::create_dir_all(replays.join("tests")).unwrap();
        let replay = |fingerprint: u64, score: u32| Replay {
            version: 1,
            game: "Test".into(),
            fingerprint: hex(fingerprint),
            ticks: 60,
            outcome: "quit".into(),
            score,
            input: "up:60".into(),
            checks: Vec::new(),
            from: None,
            slots: BTreeMap::new(),
            loads: Vec::new(),
            menu_saves: Vec::new(),
            controls: Vec::new(),
            scenario: None,
            evals: Vec::new(),
            seats: Vec::new(),
            files: Set::new(),
            applies: Vec::new(),
            carried: BTreeMap::new(),
        };
        replay(42, 1).save(&replays.join("a.toml")).unwrap();
        replay(7, 2).save(&replays.join("b.toml")).unwrap();
        replay(42, 3).save(&replays.join("c.toml")).unwrap();
        replay(42, 4).save(&replays.join("tests").join("t.toml")).unwrap();

        let mut attract = Attract::new(&tmp);
        assert!(!attract.wait(20.0, 30.0));
        attract.input();
        assert!(!attract.wait(20.0, 30.0), "a key did not start the wait over");
        assert!(attract.wait(10.0, 30.0));
        assert!(!attract.wait(100.0, 0.0), "0 means never");
        let scores: Vec<u32> = (0..3).map(|_| attract.next(42).unwrap().score).collect();
        assert_eq!(scores, [1, 3, 1]);
        assert!(!attract.wait(1.0, 30.0), "playing did not start the wait over");
        assert!(attract.next(99).is_none());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// Kept runs are named by the time, so they sort by it.
    #[test]
    fn a_stamp_is_a_sortable_date() {
        assert_eq!(stamp_of(0), "19700101-000000");
        assert_eq!(stamp_of(951_868_798), "20000229-235958");
        assert_eq!(stamp_of(1_792_072_992), "20261015-140312");
        assert_eq!(stamp().len(), 15);
    }

    /// A script reads back as written, and runs of the same keys merge.
    #[test]
    fn a_script_reads_back_as_written() {
        let text = "up:30;up+fire:12;none:5;left+grenade+strafe:1";
        let script = parse_script(text, &ACTIONS).unwrap();
        assert_eq!(write_script(&script, &ACTIONS), text);
        assert_eq!(write_script(&parse_script("up:3;up:4;none:2", &ACTIONS).unwrap(), &ACTIONS), "up:7;none:2");
        assert!(parse_script("up+shoot:3", &ACTIONS).is_err(), "a key the game does not have");
        let mut s = Script::new(script);
        let inputs: Vec<Input> = (0..50).map(|_| s.next_input()).collect();
        let (fire, strafe) = (0, 3);
        assert!(inputs[29].up && !inputs[29].held(fire) && inputs[30].held(fire) && inputs[47].held(strafe));
        assert_eq!(inputs[48], Input::default(), "keys held after the end");
    }

    /// A recorded run of More Blood plays back the same, and a replay with
    /// one key changed is caught within the second it makes a difference.
    #[test]
    fn a_recorded_run_plays_back_and_a_changed_one_is_caught() {
        let dir = Path::new("games/moreblood");
        let mut game = Session::<Shooter>::load(dir).unwrap();
        let mut recorder = Recorder::default();
        for t in 0..400 {
            let input: Input = Keys { up: true, fire: t % 30 < 12, left: (120..170).contains(&t), grenade: t == 200, ..Keys::default() }.into();
            game.step(&input);
            recorder.record(&game);
        }
        let replay = recorder.replay(&game).unwrap();
        assert_eq!(replay.checks.len(), 6);
        let back: Replay = toml::from_str(&toml::to_string(&replay).unwrap()).unwrap();
        assert_eq!(back, replay, "the file does not read back");
        assert_eq!(play_through::<Shooter>(dir, &replay).unwrap(), Verdict::Same);

        // Fire held a second longer at tick 250: apart in the fifth second.
        let mut script = parse_script(&replay.input, &ACTIONS).unwrap();
        let mut ticks = Vec::new();
        for (input, n) in script.drain(..) {
            ticks.extend(std::iter::repeat_n(input, n as usize));
        }
        for input in &mut ticks[250..310] {
            input.hold(0, true);
        }
        let changed = Replay { input: write_script(&ticks.iter().map(|&i| (i, 1)).collect::<Vec<_>>(), &ACTIONS), ..replay.clone() };
        assert!(matches!(play_through::<Shooter>(dir, &changed).unwrap(), Verdict::RunsApart(Apart { second: 5 | 6 })));

        let other = Replay { fingerprint: hex(1), ..replay };
        assert_eq!(play_through::<Shooter>(dir, &other).unwrap(), Verdict::OtherData);
    }

    /// A replay recorded before `conf.lua` changed in a way that leaves the
    /// game as it was gets the new fingerprint and nothing else; after a
    /// change that alters the game nothing is written.
    #[test]
    fn refit_takes_a_new_layout_and_refuses_a_new_game() {
        let tmp = std::env::temp_dir().join(format!("voxcade-refit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let copy = |from: &Path, to: &Path| {
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(from, to).unwrap();
        };
        let arena = Path::new("games/arena");
        for file in ["conf.lua", "main.lua", "world.vvx", "player.vvx", "replays/tests/walkabout.toml"] {
            copy(&arena.join(file), &tmp.join(file));
        }
        let conf = std::fs::read_to_string(arena.join("conf.lua")).unwrap();
        let replay = tmp.join("replays/tests/walkabout.toml");
        let before = std::fs::read_to_string(&replay).unwrap();
        assert!(check::<crate::Voxcade>(&tmp, false).unwrap());

        // Room for more debris than the run ever throws: the same game, new data.
        std::fs::write(tmp.join("conf.lua"), conf.replace("max_particles = 30000", "max_particles = 40000")).unwrap();
        assert!(!check::<crate::Voxcade>(&tmp, false).unwrap(), "the fingerprint should no longer fit");
        assert!(refit::<crate::Voxcade>(&tmp).unwrap());
        let after = std::fs::read_to_string(&replay).unwrap();
        let changed: Vec<_> = before.lines().zip(after.lines()).filter(|(a, b)| a != b).collect();
        assert_eq!(changed.len(), 1, "only the fingerprint may change: {changed:?}");
        assert!(changed[0].0.starts_with("fingerprint = "));
        assert_eq!(before.lines().count(), after.lines().count());
        assert!(check::<crate::Voxcade>(&tmp, false).unwrap());

        // Heavier gravity changes the game: nothing is written.
        std::fs::write(tmp.join("conf.lua"), conf.replace("\nend", "\n  c.physics.gravity = 300\nend")).unwrap();
        assert!(!refit::<crate::Voxcade>(&tmp).unwrap());
        assert_eq!(std::fs::read_to_string(&replay).unwrap(), after);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// The replays in the games' folders play out as recorded: the
    /// regression net for every change to the engine (after a change that
    /// was meant, `voxcade check <game> --update`).
    #[test]
    fn the_games_replays_play_out_the_same() {
        for game in ["games/moreblood", "games/arena", "games/sokovox"] {
            assert!(check::<crate::Voxcade>(Path::new(game), false).unwrap(), "a replay of {game} plays out differently");
        }
    }
}
