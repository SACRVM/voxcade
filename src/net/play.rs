//! A network game of a `Session` (`docs/multiplayer.md`, stage 4): the
//! host's side plays every bundle it builds, a guest's every bundle it
//! gets, each recording the run as any run. What a bundle means for the
//! session travels as its bytes: the saves and console lines before the
//! tick, the host's clock when it changed, a restart (F5) or a load
//! (LOAD, its save sent in pieces first), the players' controls. A guest
//! coming late gets the run so far (its recording, the host's slots now)
//! and plays it through as `inspect` would, then plays on with the
//! others.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use super::wire::{self, Ask, Broken, Bundle, Reader, Writer};
use super::{Gone, Guest, Host, Notice};
use crate::controls::{Controls, Device};
use crate::game::{Game, Seat};
use crate::input::Input;
use crate::replay::{Playback, Recorder, Replay, replay_session};
use crate::session::{Event, Session, Tick};

/// The most bytes of a console line in a network game (a bundle must fit
/// a datagram): a longer one is refused before it runs (`Hosting::takes_line`),
/// never cut.
pub const MAX_LINE: usize = 600;

/// Bytes of a bundle kept for saves (SAVE in the pause menu, about 15
/// bytes each) when console lines fill the rest.
const SAVES_ROOM: usize = 48;

/// What a bundle says to the session.
#[derive(Clone, Debug, Default, PartialEq)]
struct Extra {
    /// The host's clock, when it changed.
    when: Option<u64>,
    events: Vec<Event>,
    /// The run starts anew from the game's files (F5), or from a save
    /// sent in pieces under this number (LOAD).
    restart: bool,
    load: Option<u32>,
}

impl Extra {
    fn write(&self) -> Vec<u8> {
        let mut w = Writer::bare();
        if let Some(when) = self.when {
            w.u8(1).u64(when);
        }
        for e in &self.events {
            match e {
                Event::Save { slot, when } => w.u8(2).varint(*slot as u64).u64(*when),
                // Whole, never cut (the host checks its length first).
                Event::Eval(code) => w.u8(3).str(code),
                // Files go first and only their hash in a bundle, as LOAD
                // goes (docs/multiplayer.md, stage 6, step 7): not yet.
                Event::Apply(_) => &mut w,
            };
        }
        if self.restart {
            w.u8(4);
        }
        if let Some(id) = self.load {
            w.u8(5).varint(id as u64);
        }
        w.buf
    }

    fn read(bytes: &[u8]) -> Result<Self, Broken> {
        let mut r = Reader::bare(bytes);
        let mut e = Self::default();
        while r.done().is_err() {
            match r.u8()? {
                1 => e.when = Some(r.u64()?),
                2 => e.events.push(Event::Save { slot: r.u32()?, when: r.u64()? }),
                3 => e.events.push(Event::Eval(r.str(MAX_LINE)?)),
                4 => e.restart = true,
                5 => e.load = Some(r.u32()?),
                _ => return Err(Broken),
            }
        }
        Ok(e)
    }
}

/// What a player plays with, as a bundle carries it.
pub fn write_controls(c: &Controls) -> Vec<u8> {
    let mut w = Writer::bare();
    w.u8(match c.device {
        None => 0,
        Some(Device::Keyboard) => 1,
        Some(Device::Gamepad) => 2,
    });
    match &c.language {
        Some(l) => w.u8(1).str(l),
        None => w.u8(0),
    };
    w.varint(c.keys.len() as u64);
    for (action, key) in &c.keys {
        w.str(action).str(key);
    }
    w.buf
}

pub fn read_controls(bytes: &[u8]) -> Result<Controls, Broken> {
    let mut r = Reader::bare(bytes);
    let device = match r.u8()? {
        0 => None,
        1 => Some(Device::Keyboard),
        2 => Some(Device::Gamepad),
        _ => return Err(Broken),
    };
    let language = if r.u8()? == 1 { Some(r.str(8)?) } else { None };
    let mut keys = BTreeMap::new();
    for _ in 0..r.count(64)? {
        keys.insert(r.str(32)?, r.str(32)?);
    }
    r.done()?;
    Ok(Controls { device, keys, language })
}

/// The run so far, as a guest coming late gets it in pieces: the host's
/// recording since the run began (how it began, every tick), the clock
/// last told, every seat's controls as the bundles have them, the host's
/// slots now, its game's files by their hashes and its build's commit
/// (a guest whose game differs is told which file).
struct RunSoFar {
    replay: String,
    when: u64,
    known: Vec<Controls>,
    slots: BTreeMap<u32, String>,
    files: Vec<(String, u64)>,
    commit: String,
}

impl RunSoFar {
    fn write(&self) -> Vec<u8> {
        let mut w = Writer::bare();
        w.u8(1).str(&self.replay).u64(self.when).varint(self.known.len() as u64);
        for c in &self.known {
            w.bytes(&write_controls(c));
        }
        w.varint(self.slots.len() as u64);
        for (slot, text) in &self.slots {
            w.varint(*slot as u64).str(text);
        }
        w.varint(self.files.len() as u64);
        for (name, hash) in &self.files {
            w.str(name).u64(*hash);
        }
        w.str(&self.commit);
        w.buf
    }

    fn read(bytes: &[u8]) -> Result<Self, Broken> {
        let mut r = Reader::bare(bytes);
        if r.u8()? != 1 {
            return Err(Broken);
        }
        let replay = r.str(bytes.len())?;
        let when = r.u64()?;
        let known = (0..r.count(wire::MAX_SEATS)?).map(|_| read_controls(r.bytes(1024)?)).collect::<Result<Vec<_>, _>>()?;
        let mut slots = BTreeMap::new();
        for _ in 0..r.count(64)? {
            slots.insert(r.u32()?, r.str(bytes.len())?);
        }
        let mut files = Vec::new();
        for _ in 0..r.count(100_000)? {
            files.push((r.str(1024)?, r.u64()?));
        }
        let commit = r.str(64)?;
        r.done()?;
        Ok(Self { replay, when, known, slots, files, commit })
    }
}

/// The tick a bundle plays, made in `tick` (the one before): its inputs,
/// every seat's controls as the bundles have them (copied only where they
/// differ from the tick before's), the clock, and the events it brings.
fn tick_of<'a>(tick: &'a mut Tick, b: &Bundle, known: &[Controls], when: u64, events: Vec<Event>) -> &'a Tick {
    (tick.events, tick.when) = (events, when);
    tick.input = b.inputs.first().copied().flatten().unwrap_or_default();
    tick.seats.resize_with(known.len().saturating_sub(1), Seat::default);
    for (to, from) in std::iter::once(&mut tick.controls).chain(tick.seats.iter_mut().map(|s| &mut s.controls)).zip(known) {
        if to != from {
            to.clone_from(from);
        }
    }
    for (n, seat) in tick.seats.iter_mut().enumerate() {
        seat.input = b.inputs.get(n + 1).copied().flatten();
    }
    tick
}

/// Takes the controls a bundle changed.
fn take_controls(known: &mut [Controls], b: &Bundle) -> Result<()> {
    for (seat, bytes) in &b.controls {
        let c = read_controls(bytes).map_err(|_| anyhow::anyhow!("tick {}: seat {seat}'s controls do not read", b.tick))?;
        if let Some(k) = known.get_mut(*seat as usize - 1) {
            *k = c;
        }
    }
    Ok(())
}

/// Every seat's controls as a fresh run has them: nobody plays.
fn nobody<G: Game>(game: &Session<G>) -> Vec<Controls> {
    vec![Controls::nobody(game.actions()); game.cfg.players.max(1) as usize]
}

/// The run starts anew on the game's files (`files`: its folder, or the
/// set it plays sealed), with these slots, from a save if given; they must
/// be those the network game began with.
fn fresh<G: Game>(files: &crate::files::Files, slots: crate::saves::Slots, from: Option<&str>, fingerprint: u64) -> Result<Session<G>> {
    let mut game = Session::<G>::load_from(files.clone(), slots)?;
    if game.fingerprint()? != fingerprint {
        bail!("the game's files changed since the network game began");
    }
    if let Some(text) = from {
        game.start_from(text)?;
    }
    Ok(game)
}

/// How often the machines compare their state hashes (ticks), and how
/// many of its own the host keeps to compare with (32: five seconds).
pub const CHECK_EVERY: u64 = 10;
const KEPT_CHECKS: usize = 32;

/// A game's state hash by parts after a tick it compares (`CHECK_EVERY`).
type Check = (u64, Vec<(&'static str, u64)>);

/// Where reports of games run apart go: `desyncs/` in the config folder.
pub fn reports() -> PathBuf {
    crate::kept::config_dir().join("desyncs")
}

/// Writes what is known of a game run apart under `base` (the window's:
/// `reports()`), in `<game>/<when>-<tick>/`: the run's recording (replayed, it
/// brings back any tick), this machine's state hash by parts at the tick
/// and the other's, and, worked out on a thread of its own, the states as
/// `inspect` prints them at the last check that agreed and at the tick
/// that did not. Where it went.
#[allow(clippy::too_many_arguments)]
pub fn report<G: Game>(base: &Path, dir: &Path, recording: &Replay, apart: &wire::Apart, run_start: u64, mine: &[(&'static str, u64)], theirs: &[u64], agreed: Option<u64>, who: &str) -> Result<PathBuf> {
    let at = base.join(crate::kept::game_name(dir)).join(format!("{}-{}", crate::saves::now(), apart.tick));
    std::fs::create_dir_all(&at).with_context(|| format!("creating {}", at.display()))?;
    recording.save(&at.join("recording.toml"))?;
    let mut text = format!("{who}: the state ran apart after tick {} (the run's {}) in {}; {}\n\npart          this machine      the other\n", apart.tick, apart.tick + 1 - run_start, apart.part, if apart.inputs { "what was played differs (the network)" } else { "what was played is the same (the game)" });
    for (n, (name, hash)) in mine.iter().enumerate() {
        let other = theirs.get(n).map_or("-".to_string(), |h| format!("{h:016x}"));
        text += &format!("{name:<13} {hash:016x}  {other}{}\n", if theirs.get(n) != Some(hash) { "  <" } else { "" });
    }
    text += "\nThe states as inspect prints them: agreed.json (the last check that agreed) and apart.json, written by a thread when the recording has been played to them.\n";
    std::fs::write(at.join("sections.txt"), text)?;
    let (dir, replay, out) = (dir.to_path_buf(), recording.clone(), at.clone());
    let ticks: Vec<(u64, &'static str)> = agreed.map(|t| (t + 1 - run_start, "agreed.json")).into_iter().chain([(apart.tick + 1 - run_start, "apart.json")]).collect();
    std::thread::spawn(move || {
        let states = || -> Result<()> {
            let mut game = replay_session::<G>(&dir, &replay)?;
            let mut playback = Playback::new(&replay, &mut game)?;
            for (tick, file) in ticks {
                while playback.steps() < tick && !playback.done() {
                    let _ = playback.step(&mut game);
                }
                std::fs::write(out.join(file), serde_json::to_string_pretty(&game.game.inspect())?)?;
            }
            Ok(())
        };
        if let Err(e) = states() {
            eprintln!("the desync's report: {e:#}");
        }
    });
    Ok(at)
}

/// What the host and a guest alike keep of the run: the game's folder,
/// the run's recording, the fingerprint of the files the network game
/// began with, every seat's controls as the bundles have them, the bundle
/// the run began with (F5 and LOAD begin anew) and its own checks.
struct Run {
    dir: PathBuf,
    recorder: Recorder,
    fingerprint: u64,
    known: Vec<Controls>,
    start: u64,
    checks: std::collections::VecDeque<Check>,
    /// The last tick played, made anew in place for the next (`tick_of`).
    tick: Tick,
}

impl Run {
    fn new(dir: &Path, fingerprint: u64, known: Vec<Controls>) -> Self {
        Self { dir: dir.to_path_buf(), recorder: Recorder::default(), fingerprint, known, start: 0, checks: Default::default(), tick: Tick::default() }
    }

    /// The run begun anew at the bundle of `tick` (F5, LOAD): `game` made
    /// afresh with `slots`, from the save `text` if one, keeping the
    /// player's options and fades; nobody's controls known, nothing
    /// recorded or checked yet.
    fn renew<G: Game>(&mut self, game: &mut Session<G>, slots: crate::saves::Slots, text: Option<&str>, tick: u64) -> Result<()> {
        let mut fresh = fresh::<G>(&game.files, slots, text, self.fingerprint)?;
        (fresh.options, fresh.fades) = (game.options, game.fades);
        *game = fresh;
        self.recorder = Recorder::default();
        self.known = nobody(game);
        (self.start, self.checks) = (tick, Default::default());
        Ok(())
    }

    /// The state hash by parts after `tick`, kept to compare (the newest
    /// `KEPT_CHECKS`).
    fn keep_check(&mut self, tick: u64, sections: Vec<(&'static str, u64)>) {
        self.checks.push_back((tick, sections));
        while self.checks.len() > KEPT_CHECKS {
            self.checks.pop_front();
        }
    }
}

/// The slots as a run keeps them for itself (a guest's: the host's).
fn slots_of<G: Game>(game: &Session<G>) -> crate::saves::Slots {
    let mut slots = crate::saves::Slots::holding(game.slots.texts().into_iter().map(|(n, t)| (n, t.to_string())).collect());
    slots.set_count(game.slots.count());
    slots
}

/// The seats a machine's players take and leave through the network:
/// the host's own asks or a guest's, answered in the bundles.
pub trait Seats {
    /// Asks for a seat for one more player here: the ask's number.
    fn ask_seat(&mut self) -> u32;
    fn leave_seat(&mut self, seat: u8);
    /// The asks answered since asked: the seat, or none free.
    fn answers(&mut self) -> Vec<(u32, Option<u8>)>;
    /// This machine's seats as the bundles say now.
    fn held(&self) -> Vec<u8>;
}

/// This machine's players (its seating's seats, from 1) and the game's
/// seats they sit on, as the bundles gave them (`docs/multiplayer.md`,
/// stage 4: the seating says which of this machine's devices drives which
/// of its players, who sits where is the bundle's).
#[derive(Clone, Debug, Default)]
pub struct Locals {
    /// For each player here: the game's seat it sits on, or the ask
    /// waiting for one; whether no seat was free for it (it asks again
    /// only once it stopped sitting and sits again).
    seat: Vec<Option<u8>>,
    asked: Vec<Option<u32>>,
    refused: Vec<bool>,
}

impl Locals {
    /// `players` players here; the host's first sits on seat 1 from the
    /// start.
    pub fn new(players: usize, host: bool) -> Self {
        let mut seat = vec![None; players.max(1)];
        if host {
            seat[0] = Some(1);
        }
        Self { seat, asked: vec![None; players.max(1)], refused: vec![false; players.max(1)] }
    }

    /// The game's seat of player `local` here (from 1).
    pub fn seat(&self, local: usize) -> Option<u8> {
        self.seat.get(local.wrapping_sub(1)).copied().flatten()
    }

    /// The game's seats played here, in order.
    pub fn seats(&self) -> Vec<usize> {
        let mut seats: Vec<usize> = self.seat.iter().flatten().map(|&s| s as usize).collect();
        seats.sort_unstable();
        seats
    }

    /// Asks for a seat for every player here who sits (`sits`) without
    /// one, gives up the seat of every player who no longer sits, takes
    /// the answers in; the players here whom no seat was free for or
    /// whose seat went (they no longer sit).
    pub fn settle(&mut self, net: &mut impl Seats, sits: impl Fn(usize) -> bool) -> Vec<usize> {
        let mut out = Vec::new();
        for (ask, answer) in net.answers() {
            let Some(i) = self.asked.iter().position(|a| *a == Some(ask)) else { continue };
            self.asked[i] = None;
            match answer {
                Some(seat) => self.seat[i] = Some(seat),
                None => {
                    self.refused[i] = true;
                    out.push(i + 1);
                }
            }
        }
        // A seat the bundles took away (a guest dropped by the host).
        let held = net.held();
        for i in 0..self.seat.len() {
            if self.seat[i].is_some_and(|s| !held.contains(&s)) {
                (self.seat[i], self.refused[i]) = (None, true);
                out.push(i + 1);
            }
        }
        for i in 0..self.seat.len() {
            let local = i + 1;
            if !sits(local) {
                self.refused[i] = false;
            }
            match (sits(local) && !self.refused[i], self.seat[i], self.asked[i]) {
                (true, None, None) => self.asked[i] = Some(net.ask_seat()),
                (false, Some(seat), _) => {
                    net.leave_seat(seat);
                    self.seat[i] = None;
                }
                _ => {}
            }
        }
        out
    }
}

/// The host's side of a network game of a session (the session the
/// caller keeps, as a window does).
pub struct Hosting<G: Game, A> {
    pub host: Host<A>,
    run: Run,
    /// What the host's own players play with (asked for when it changes).
    own: BTreeMap<u8, (Controls, bool)>,
    when: Option<u64>,
    restart: bool,
    /// A LOAD waiting for every guest to have its save: its number, bytes
    /// and text.
    loading: Option<(u32, Arc<[u8]>, String)>,
    blobs: u32,
    answers: Vec<(u32, Option<u8>)>,
    /// The last tick each guest agreed at, and the reports written of
    /// games run apart.
    agreed: BTreeMap<super::Machine, u64>,
    pub reports: Vec<PathBuf>,
    /// Where they go (none: nowhere).
    pub reports_to: Option<PathBuf>,
    _game: std::marker::PhantomData<G>,
}

impl<G: Game, A: Copy + Ord + std::fmt::Debug> Hosting<G, A> {
    /// Hosts `game`, a run begun afresh (from its scenario, its console
    /// lines), under `key` (64 random bits), as `name`.
    pub fn new(game: &Session<G>, key: u64, name: &str) -> Result<Self> {
        let seats = game.cfg.players.max(1) as usize;
        Ok(Self {
            host: Host::new(key, name, seats),
            run: Run::new(&game.dir, game.fingerprint()?, nobody(game)),
            own: BTreeMap::new(),
            when: None,
            restart: false,
            loading: None,
            blobs: 1,
            answers: Vec::new(),
            agreed: BTreeMap::new(),
            reports: Vec::new(),
            reports_to: None,
            _game: std::marker::PhantomData,
        })
    }

    /// One of the host's own players' keys, read before the next tick.
    pub fn read(&mut self, seat: u8, input: Input) {
        self.host.read(seat, input);
    }

    /// What one of the host's own players plays with now.
    pub fn set_controls(&mut self, seat: u8, controls: Controls) {
        let was = self.own.get(&seat).map(|(c, _)| c);
        if was != Some(&controls) {
            self.own.insert(seat, (controls, false));
        }
    }

    /// F5: the run starts anew for everyone at the next tick, on the files
    /// the network game began with (a file changed since: refused).
    pub fn restart(&mut self, game: &Session<G>) -> Result<()> {
        fresh::<G>(&game.files, slots_of(game), None, self.run.fingerprint)?;
        self.restart = true;
        Ok(())
    }

    /// LOAD: the save in `slot` goes to every guest in pieces, then the run
    /// starts anew from it for everyone.
    pub fn load(&mut self, game: &Session<G>, slot: u32) -> Result<()> {
        let text = game.slots.read(slot).with_context(|| format!("slot {slot} is empty"))?.to_string();
        self.blobs += 1;
        let data: Arc<[u8]> = Arc::from(text.as_bytes());
        for guest in self.host.guests() {
            self.host.send(guest, self.blobs, data.clone());
        }
        self.loading = Some((self.blobs, data, text));
        Ok(())
    }

    /// Whether a LOAD waits for its save to reach every guest.
    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }

    /// Whether the console line `line` may run on `game` now: it travels
    /// whole with the next tick's bundle, beside the lines and saves before
    /// it (they ran here already, and run on every guest before that
    /// tick). Too long, or no room left before the next tick (the console
    /// open holds the game): why, in the console's words.
    pub fn takes_line(&self, game: &Session<G>, line: &str) -> Result<()> {
        if line.len() > MAX_LINE {
            bail!("a line of {} bytes is too long for a network game: {MAX_LINE} at most", line.len());
        }
        let mut events = game.pending().to_vec();
        events.push(Event::Eval(line.to_string()));
        let bytes = Extra { when: Some(u64::MAX), events, ..Extra::default() }.write().len();
        if bytes + SAVES_ROOM > self.host.extra_room() {
            bail!("the network takes no more lines before the next tick: close the console a moment, then go on");
        }
        Ok(())
    }

    /// Lets in every machine asking to join, with the run so far; compares
    /// the guests' state hashes with its own (a guest whose game ran apart
    /// is told so, every screen says where, a report is written); what
    /// else the network said.
    pub fn notices(&mut self, game: &Session<G>) -> Result<Vec<Notice>> {
        for (index, tick, theirs) in self.host.checks() {
            let Some((_, mine)) = self.run.checks.iter().find(|(t, _)| *t == tick) else { continue };
            let Some(n) = mine.iter().zip(&theirs).position(|((_, a), b)| a != b).or((mine.len() != theirs.len()).then_some(0)) else {
                self.agreed.insert(index, tick);
                continue;
            };
            let inputs = mine.last().map(|(_, h)| h) != theirs.last();
            let apart = wire::Apart { tick, part: mine[n].0.to_string(), inputs };
            let name = self.host.name(index).unwrap_or("?").to_uppercase();
            let commit = self.host.commit(index).unwrap_or("?").to_string();
            self.host.say(&crate::network::desync_line(&apart, &name, &commit));
            let mine = mine.clone();
            self.host.tell_apart(index, apart.clone());
            if let Some(base) = &self.reports_to {
                match report::<G>(base, &self.run.dir, &self.run.recorder.replay(game)?, &apart, self.run.start, &mine, &theirs, self.agreed.get(&index).copied(), &format!("the host; the other {name} ({commit})")) {
                    Ok(at) => self.reports.push(at),
                    Err(e) => eprintln!("the desync's report: {e:#}"),
                }
            }
        }
        let mut out = Vec::new();
        for notice in self.host.notices() {
            if let Notice::Joining { index, .. } = &notice {
                let blob = self.run_so_far(game)?;
                self.host.admit(*index, blob);
                if let Some((id, data, _)) = &self.loading {
                    self.host.send(*index, *id, data.clone());
                }
            }
            out.push(notice);
        }
        Ok(out)
    }

    fn run_so_far(&self, game: &Session<G>) -> Result<Vec<u8>> {
        let replay = toml::to_string(&self.run.recorder.replay(game)?)?;
        let slots = game.slots.texts().into_iter().map(|(n, t)| (n, t.to_string())).collect();
        let files = game.fingerprint_files()?;
        Ok(RunSoFar { replay, when: self.when.unwrap_or(0), known: self.run.known.clone(), slots, files, commit: super::udp::COMMIT.to_string() }.write())
    }

    /// Builds the next tick's bundle and plays it on `game`; a level
    /// change's fade takes no tick (`Session::ticked`). Whether the run
    /// started anew (F5, LOAD: `game` is another).
    pub fn tick(&mut self, game: &mut Session<G>) -> Result<bool> {
        if game.fading() {
            game.step(&Input::default());
            // A check due at a level change is taken once it is done.
            self.run.recorder.record(game);
            return Ok(false);
        }
        for (seat, (controls, asked)) in &mut self.own {
            if !*asked && self.host.owner(*seat) == Some(0) {
                self.host.ask(Ask::Controls(*seat, write_controls(controls)));
                *asked = true;
            }
        }
        let mut extra = Extra::default();
        let when = game.clock();
        if self.when != Some(when) {
            extra.when = Some(when);
        }
        let load = self.loading.as_ref().filter(|(id, _, _)| self.host.guests().iter().all(|&g| self.host.delivered(g, *id))).map(|(id, _, _)| *id);
        if self.restart {
            extra.restart = true;
        } else if load.is_some() {
            extra.load = load;
        } else {
            extra.events = game.pending().to_vec();
        }
        // Lines run without asking `takes_line` first (`inspect --do`):
        // the network game ends saying why, before anything is sent.
        if let Some(long) = extra.events.iter().find_map(|e| match e {
            Event::Eval(code) if code.len() > MAX_LINE => Some(code.len()),
            _ => None,
        }) {
            bail!("a console line of {long} bytes is too long for a network game: {MAX_LINE} at most");
        }
        let bytes = extra.write();
        let (size, room) = (bytes.len(), self.host.extra_room());
        let bundle = self.host.tick(bytes).map_err(|_| anyhow::anyhow!("the console lines and saves before this tick take {size} bytes, more than a network game carries in one ({room})"))?;
        self.when = Some(when);
        for e in &bundle.seats {
            match *e {
                wire::SeatEvent::Taken { seat, owner: 0, ask, .. } => self.answers.push((ask, Some(seat))),
                wire::SeatEvent::Refused { owner: 0, ask } => self.answers.push((ask, None)),
                _ => {}
            }
        }
        let renewed = extra.restart || extra.load.is_some();
        if renewed {
            let text = self.loading.take().filter(|_| extra.load.is_some()).map(|(_, _, text)| text);
            let slots = game.slots.clone();
            self.run.renew(game, slots, text.as_deref(), bundle.tick)?;
            for (_, asked) in self.own.values_mut() {
                *asked = false;
            }
            self.restart = false;
        }
        take_controls(&mut self.run.known, &bundle)?;
        // The host has made its saves and run its lines already.
        game.play(tick_of(&mut self.run.tick, &bundle, &self.run.known, when, Vec::new()));
        self.run.recorder.record(game);
        if bundle.tick.is_multiple_of(CHECK_EVERY) {
            self.run.keep_check(bundle.tick, game.sections());
        }
        Ok(renewed)
    }

    /// The run so far as a replay.
    pub fn recording(&self, game: &Session<G>) -> Result<Replay> {
        self.run.recorder.replay(game)
    }
}

impl<G: Game, A: Copy + Ord + std::fmt::Debug> Seats for Hosting<G, A> {
    fn ask_seat(&mut self) -> u32 {
        self.host.ask(Ask::Take)
    }

    fn leave_seat(&mut self, seat: u8) {
        self.host.ask(Ask::Leave(seat));
    }

    fn answers(&mut self) -> Vec<(u32, Option<u8>)> {
        std::mem::take(&mut self.answers)
    }

    fn held(&self) -> Vec<u8> {
        (1..=self.run.known.len() as u8).filter(|&s| self.host.owner(s) == Some(0)).collect()
    }
}

/// How far a guest is.
#[derive(Clone, Debug, PartialEq)]
pub enum Joined {
    /// Waiting for the host, or for the run so far (pieces come of how
    /// many).
    Waiting(Option<(u32, u32)>),
    /// Playing the run so far through: ticks played of how many.
    CatchingUp(u64, u64),
    Playing,
    /// Its game ran apart from the host's (the host said where).
    Apart(wire::Apart),
    Gone(String),
}

/// A guest's side of a network game of a session: it plays the run so
/// far through on a session of its own, then hands it over
/// (`catch_up`) and plays the bundles on it (`ticks`, `play`).
pub struct Joining<G: Game, A> {
    pub guest: Guest<A>,
    run: Run,
    catching: Option<(Session<G>, Playback)>,
    caught_up: bool,
    so_far: Option<RunSoFar>,
    when: u64,
    /// Its players' controls (asked for when they change), and why it
    /// ended.
    own: BTreeMap<u8, Controls>,
    ended: Option<String>,
    /// The host's build.
    pub host_commit: String,
    /// Where a report of its game run apart goes (none: nowhere).
    pub reports_to: Option<PathBuf>,
    /// A test's: the next bundle played has seat 1's keys changed, as a
    /// network that broke it would.
    #[cfg(test)]
    pub break_next: bool,
}

impl<G: Game, A: Copy + PartialEq + std::fmt::Debug> Joining<G, A> {
    /// Joins the game in `dir` through `guest`.
    pub fn new(dir: &Path, guest: Guest<A>) -> Self {
        Self {
            guest,
            run: Run::new(dir, 0, Vec::new()),
            catching: None,
            caught_up: false,
            so_far: None,
            when: 0,
            own: BTreeMap::new(),
            ended: None,
            host_commit: String::new(),
            reports_to: None,
            #[cfg(test)]
            break_next: false,
        }
    }

    pub fn state(&self) -> Joined {
        if let Some(why) = &self.ended {
            return Joined::Gone(why.clone());
        }
        match self.guest.gone() {
            Some(Gone::HostLeft) if self.guest.in_hand() == 0 => return Joined::Gone("THE HOST LEFT".into()),
            Some(Gone::HostLeft) | None => {}
            Some(Gone::NoAnswer) => return Joined::Gone("NO ANSWER FROM THE HOST".into()),
            Some(Gone::Refused(why)) => return Joined::Gone(why.to_uppercase()),
            Some(Gone::CannotKeepUp) => return Joined::Gone("THIS MACHINE CANNOT KEEP UP".into()),
            Some(Gone::Left) => return Joined::Gone("YOU LEFT".into()),
            Some(Gone::Apart(apart)) => return Joined::Apart(apart.clone()),
        }
        match (&self.catching, self.caught_up) {
            (_, true) => Joined::Playing,
            (Some((_, p)), false) => Joined::CatchingUp(p.steps(), p.ticks()),
            (None, false) => Joined::Waiting(self.guest.progress(1)),
        }
    }

    /// Ends it, saying why: its farewell, to send.
    pub fn end(&mut self, why: &str) -> Vec<(A, Vec<u8>)> {
        self.ended.get_or_insert_with(|| why.to_string());
        self.guest.leave_game()
    }

    /// One of its players' keys, read this frame.
    pub fn read(&mut self, seat: u8, input: Input) {
        self.guest.read(seat, input);
    }

    /// What the player on one of its seats plays with now.
    pub fn set_controls(&mut self, seat: u8, controls: Controls) {
        if self.own.get(&seat) != Some(&controls) {
            self.guest.controls(seat, write_controls(&controls));
            self.own.insert(seat, controls);
        }
    }

    /// Takes in the run so far once it came and plays it through, for up
    /// to `budget`; the session once it has caught up (handed over: the
    /// bundles play on it from then on).
    pub fn catch_up(&mut self, budget: std::time::Duration) -> Result<Option<Session<G>>> {
        if self.caught_up {
            return Ok(None);
        }
        if self.catching.is_none() {
            let Some(blob) = self.guest.take(1) else { return Ok(None) };
            let run = RunSoFar::read(&blob).map_err(|_| anyhow::anyhow!("the run so far does not read"))?;
            let replay: Replay = toml::from_str(&run.replay).context("the run so far")?;
            let mut game = replay_session::<G>(&self.run.dir, &replay)?;
            let playback = Playback::new(&replay, &mut game)?;
            self.run.fingerprint = game.fingerprint()?;
            if !replay.fits(self.run.fingerprint) {
                // Which files, by name.
                let mine = game.fingerprint_files()?;
                let differ: Vec<&str> = run.files.iter().filter(|f| !mine.contains(f)).map(|(name, _)| name.as_str()).chain(mine.iter().filter(|f| !run.files.iter().any(|(n, _)| *n == f.0)).map(|(name, _)| name.as_str())).collect();
                bail!("{} differs from the host's", if differ.is_empty() { "this machine's game".to_string() } else { differ.join(", ") });
            }
            if run.commit != super::udp::COMMIT {
                eprintln!("the host's program is built from {}, this one from {}: the checks will tell whether they play alike", run.commit, super::udp::COMMIT);
            }
            self.host_commit = run.commit.clone();
            self.run.start = self.guest.welcome().map_or(0, |w| w.first).saturating_sub(replay.ticks);
            (self.catching, self.so_far) = (Some((game, playback)), Some(run));
        }
        let Some((game, playback)) = self.catching.as_mut() else { bail!("catching up lost the run so far") };
        let started = std::time::Instant::now();
        while !playback.done() && started.elapsed() < budget {
            if let Err(apart) = playback.step(game) {
                bail!("the run so far played out differently here after {} s", apart.second);
            }
            game.stopped()?;
            self.run.recorder.record(game);
        }
        if !playback.done() {
            return Ok(None);
        }
        let (Some((mut game, _)), Some(run)) = (self.catching.take(), self.so_far.take()) else { bail!("catching up lost the run so far") };
        game.adopt_slots(run.slots);
        (self.run.known, self.when) = (run.known, run.when);
        if self.run.known.len() != game.cfg.players.max(1) as usize {
            bail!("the host's game has other seats");
        }
        self.caught_up = true;
        Ok(Some(game))
    }

    /// The bundles to play now: what came while it caught up or waited (at
    /// most `most` of them at once), then at the host's pace; the host
    /// gone, the rest.
    pub fn ticks(&mut self, now: u64, most: usize) -> Vec<Bundle> {
        if !self.caught_up {
            return Vec::new();
        }
        let mut out = Vec::new();
        while self.guest.in_hand() > self.guest.target() as usize + 1 && out.len() < most {
            out.extend(self.guest.next_bundle());
        }
        out.extend(self.guest.due(now));
        if self.guest.gone() == Some(&Gone::HostLeft) {
            while let Some(b) = self.guest.next_bundle() {
                out.push(b);
            }
        }
        out
    }

    /// A frame of a level change's fade (a window's: the bundles wait for
    /// it), recorded as such (a check due at the change is taken once it is
    /// done, as a replay sees it).
    pub fn fade(&mut self, game: &mut Session<G>) {
        game.step(&Input::default());
        self.run.recorder.record(game);
    }

    /// Plays a bundle on `game`; whether the run started anew (F5, LOAD:
    /// `game` is another).
    pub fn play(&mut self, game: &mut Session<G>, b: &Bundle) -> Result<bool> {
        let extra = Extra::read(&b.extra).map_err(|_| anyhow::anyhow!("tick {}: the host's bundle does not read", b.tick))?;
        if let Some(when) = extra.when {
            self.when = when;
        }
        let renewed = extra.restart || extra.load.is_some();
        if renewed {
            let text = match extra.load {
                Some(id) => Some(String::from_utf8(self.guest.take(id).with_context(|| format!("tick {}: the save to load has not come", b.tick))?).context("the save to load")?),
                None => None,
            };
            self.run.renew(game, slots_of(game), text.as_deref(), b.tick)?;
        }
        take_controls(&mut self.run.known, b)?;
        tick_of(&mut self.run.tick, b, &self.run.known, self.when, extra.events);
        #[cfg(test)]
        if std::mem::take(&mut self.break_next) {
            self.run.tick.input.up = !self.run.tick.input.up;
        }
        game.play(&self.run.tick);
        game.stopped()?;
        self.run.recorder.record(game);
        if b.tick.is_multiple_of(CHECK_EVERY) {
            let sections = game.sections();
            self.guest.check(b.tick, sections.iter().map(|(_, h)| *h).collect());
            self.run.keep_check(b.tick, sections);
        }
        Ok(renewed)
    }

    /// Writes the report of its game run apart (`report`): where it went.
    pub fn report(&self, game: &Session<G>, apart: &wire::Apart) -> Result<Option<PathBuf>> {
        let Some(base) = &self.reports_to else { return Ok(None) };
        let mine = self.run.checks.iter().find(|(t, _)| *t == apart.tick).map(|(_, m)| m.clone()).unwrap_or_default();
        let agreed = self.run.checks.iter().map(|(t, _)| *t).filter(|&t| t < apart.tick).max();
        report::<G>(base, &self.run.dir, &self.run.recorder.replay(game)?, apart, self.run.start, &mine, &[], agreed, &format!("a guest; the host's build {}", self.host_commit)).map(Some)
    }

    /// The run so far as a replay (equal to the host's, byte for byte).
    pub fn recording(&self, game: &Session<G>) -> Result<Replay> {
        self.run.recorder.replay(game)
    }
}

impl<G: Game, A: Copy + PartialEq + std::fmt::Debug> Seats for Joining<G, A> {
    fn ask_seat(&mut self) -> u32 {
        self.guest.ask_seat()
    }

    fn leave_seat(&mut self, seat: u8) {
        self.guest.leave(seat);
    }

    fn answers(&mut self) -> Vec<(u32, Option<u8>)> {
        self.guest.answers()
    }

    fn held(&self) -> Vec<u8> {
        self.guest.mine().into_iter().map(|(seat, _)| seat).collect()
    }
}
