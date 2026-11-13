//! The host's side of a network game: it keeps one slot for each seat
//! (the newest reading, never a queue), builds every tick's bundle from
//! them and from its own players' keys held back by the guests' delay,
//! and sends each guest the bundles it lacks. It waits for nobody
//! (`docs/multiplayer.md`, stage 4, "The host builds every tick, and
//! nobody waits").

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use super::wire::{self, Apart, Ask, Bundle, Down, Hello, Kind, Member, PIECE, PieceAck, SeatEvent, Up, Welcome};
use super::{HEARTBEAT, Machine, MachineId, QUIET, REPEAT, SILENT, TICK_US};
use crate::input::Input;

/// What the session hears of the network from the host.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// A machine asks to join: the session lets it in (`Host::admit`, with
    /// the run so far) or not (`Host::refuse`).
    Joining { index: Machine, name: String, commit: String },
    /// No word from it for a second (every screen names it), or again.
    Quiet { index: Machine },
    Back { index: Machine },
    /// It left: by its farewell, or dropped (10 s of silence, DROP).
    Left { index: Machine, dropped: bool },
    /// Its game ran apart from the host's (`net::play` compares).
    Apart { index: Machine, apart: Apart },
}

/// A seat as the host keeps it.
#[derive(Clone, Debug, Default)]
struct Slot {
    owner: Option<Machine>,
    generation: u32,
    /// The newest reading come and its number, the number of the one the
    /// last tick took, the actions held in any reading since, and the
    /// ticks it was repeated for want of a new one.
    newest: Input,
    newest_seq: Option<u32>,
    used: Option<u32>,
    held: u32,
    repeats: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Asked to join; the session has not answered.
    Waiting,
    /// Let in; no word from it since.
    Welcomed,
    Playing,
    Gone,
}

/// Something sent in pieces to one guest.
struct Sending {
    id: u32,
    data: Arc<[u8]>,
    count: u32,
    got: u32,
    mask: u64,
    /// When each piece in flight was last sent.
    sent: BTreeMap<u32, u64>,
}

struct Remote<A> {
    addr: A,
    id: MachineId,
    nonce: u64,
    name: String,
    commit: String,
    index: Machine,
    stage: Stage,
    welcome: Option<Welcome>,
    last_heard: u64,
    last_welcome: Option<u64>,
    last_down: Option<u64>,
    quiet: bool,
    rtt_us: Option<u64>,
    hand: u8,
    /// Every bundle before `got` it has, and which of the 64 after.
    got: u64,
    mask: u64,
    asks_done: u32,
    sending: VecDeque<Sending>,
    /// Where its game ran apart, told it since when (it is dropped after
    /// a while).
    apart: Option<(Apart, u64)>,
}

/// How long the host tells a guest where its game ran apart before it
/// drops it, and a line for every screen goes.
const TELLING: u64 = 2_000_000;

/// The host's side, sending datagrams to addresses of type `A`.
pub struct Host<A> {
    key: u64,
    name: String,
    seats: Vec<Slot>,
    remotes: Vec<Remote<A>>,
    joined: u8,
    /// Every bundle of the run, as sent.
    history: Vec<Vec<u8>>,
    /// The host's own players' readings, each with the tick it was read
    /// before, held back by `delay`.
    own: VecDeque<(u64, u8, Input)>,
    own_last: Vec<Input>,
    delay: u32,
    /// Ticks the measured delay has differed from `delay`, and which way.
    drift: (i32, u32),
    asks: VecDeque<(Machine, u32, Ask)>,
    own_asks: u32,
    paused: Option<Machine>,
    notices: Vec<Notice>,
    us_per_tick: u32,
    last_roster: Option<u64>,
    /// Refusals sent to an address for a reason this minute, and those
    /// still to send.
    refusals: BTreeMap<(A, String), (u64, u32)>,
    refused: Vec<(A, Vec<u8>)>,
    /// Addresses refused or gone, for the transport to close a moment
    /// later (since when, once asked).
    closing: Vec<(A, Option<u64>)>,
    /// Ticks a guest's seat held nothing for want of its keys.
    stopped: u64,
    /// The guests' state hashes by parts as they reported them (machine,
    /// tick, parts), and a line for every screen (since when).
    checks: Vec<(Machine, u64, Vec<u64>)>,
    said: Option<(String, Option<u64>)>,
    /// What it answers a search for games with (its seats filled in), and
    /// when it last answered each address (once in 200 ms at most).
    here: Option<wire::Here>,
    answered: BTreeMap<A, u64>,
    /// The biggest datagram it fills with bundles: the transport's
    /// (`wire::MAX_DATAGRAM` on a LAN, less over iroh's QUIC).
    limit: usize,
}

/// How often the host tells the guests who plays.
const ROSTER_EVERY: u64 = 1_000_000;
/// The host's own players' keys are held back between these (ticks).
const MIN_DELAY: u32 = 2;
const MAX_DELAY: u32 = 8;
/// The measured delay must differ this many ticks (2 s) before it moves.
const DELAY_PATIENCE: u32 = 120;
/// Datagrams a guest is sent at most in one go (bundles, pieces).
const BUDGET: usize = 8;
const PIECE_BUDGET: usize = 16;
/// How often a HELLO is answered again, and a refusal at most a minute.
const WELCOME_AGAIN: u64 = 200_000;
const REFUSALS: u32 = 5;
/// The most addresses refused this minute it keeps count of (more are not
/// answered), and the most addresses waiting to be closed.
const REFUSED_KEPT: usize = 1024;
const CLOSING_KEPT: usize = 256;
/// An address refused or gone is closed this long after (its last words
/// on the way first).
const CLOSE_AFTER: u64 = 1_000_000;
/// The most a DOWN's head takes beside its bundles: the datagram's head,
/// the stamp and the asks taken, the newest reading of every seat, who
/// paused, where the guest's game ran apart and a line for every screen
/// (who plays goes in a datagram of its own). A bundle may take the rest
/// of a datagram (`Host::max_bundle`): it always fits, whatever the head.
const MAX_HEAD: usize = wire::HEADER + 5 + 5 + (1 + wire::MAX_SEATS * 6) + 1 + 1 + (1 + 10 + 1 + 32 + 1) + (1 + 2 + wire::MAX_TEXT) + 1;

/// An input as wide as one is on the wire: every direction, every action.
const WIDEST: Input = Input { left: true, right: true, up: true, down: true, actions: u32::MAX };

/// A bundle too big for a datagram (the session's bytes were too many).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooBig;

impl<A: Copy + Ord + std::fmt::Debug> Host<A> {
    /// Hosts a game of `seats` seats under `key` (64 random bits, never
    /// from the game's own generator), seat 1 its own.
    pub fn new(key: u64, name: &str, seats: usize) -> Self {
        let seats = seats.clamp(1, wire::MAX_SEATS);
        let mut slots = vec![Slot::default(); seats];
        (slots[0].owner, slots[0].generation) = (Some(0), 1);
        Self {
            key,
            name: name.to_string(),
            seats: slots,
            remotes: Vec::new(),
            joined: 0,
            history: Vec::new(),
            own: VecDeque::new(),
            own_last: vec![Input::default(); seats],
            delay: 0,
            drift: (0, 0),
            asks: VecDeque::new(),
            own_asks: 0,
            paused: None,
            notices: Vec::new(),
            us_per_tick: 0,
            last_roster: None,
            refusals: BTreeMap::new(),
            refused: Vec::new(),
            closing: Vec::new(),
            stopped: 0,
            checks: Vec::new(),
            said: None,
            here: None,
            answered: BTreeMap::new(),
            limit: wire::MAX_DATAGRAM,
        }
    }

    /// The biggest datagram its transport carries (iroh's QUIC carries
    /// less than a LAN's UDP).
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.min(wire::MAX_DATAGRAM);
    }

    pub fn key(&self) -> u64 {
        self.key
    }

    /// The next tick it builds.
    pub fn next_tick(&self) -> u64 {
        self.history.len() as u64
    }

    /// The ticks a guest's seat held nothing because its keys did not
    /// come for too long (its figure stopped).
    pub fn stopped(&self) -> u64 {
        self.stopped
    }

    /// How many ticks its own players' keys are held back.
    pub fn delay(&self) -> u32 {
        self.delay
    }

    /// The machine sitting on a seat (from 1), if any.
    pub fn owner(&self, seat: u8) -> Option<Machine> {
        self.seats.get(seat as usize - 1).and_then(|s| s.owner)
    }

    /// What a tick of the simulation costs it (µs): a guest coming late
    /// guesses from it how long catching up takes.
    pub fn set_cost(&mut self, us_per_tick: u32) {
        self.us_per_tick = us_per_tick;
    }

    /// Who paused the game (its loop standing still), if anyone.
    pub fn set_paused(&mut self, who: Option<Machine>) {
        self.paused = who;
    }

    /// One of its own players' keys, read before the next tick.
    pub fn read(&mut self, seat: u8, input: Input) {
        if self.owner(seat) == Some(0) {
            self.own.push_back((self.next_tick(), seat, input));
        }
    }

    /// An ask of its own (a seat for one more player here, leaving one,
    /// controls changed), stamped into the next bundle: its number.
    pub fn ask(&mut self, ask: Ask) -> u32 {
        let n = self.own_asks;
        self.own_asks += 1;
        self.asks.push_back((0, n, ask));
        n
    }

    /// Lets the machine that asked to join in: it gets the bundles from the
    /// next tick on, and before them `blob` (the run so far) in pieces and
    /// `info` (the session's own: how the run began).
    pub fn admit(&mut self, index: Machine, blob: Vec<u8>, info: Vec<u8>) {
        let first = self.next_tick();
        let seats = self.seats.len() as u8;
        let us_per_tick = self.us_per_tick;
        let Some(r) = self.remotes.iter_mut().find(|r| r.index == index && r.stage == Stage::Waiting) else { return };
        r.welcome = Some(Welcome { index, first, seats, blob: blob.len() as u32, us_per_tick, info });
        (r.stage, r.got, r.last_welcome) = (Stage::Welcomed, first, None);
        if !blob.is_empty() {
            r.sending.push_back(sending(1, blob.into()));
        }
    }

    /// Turns the machine that asked to join away, saying why.
    pub fn refuse(&mut self, index: Machine, reason: &str) -> Option<(A, Vec<u8>)> {
        let r = self.remotes.iter_mut().find(|r| r.index == index && r.stage == Stage::Waiting)?;
        r.stage = Stage::Gone;
        let addr = r.addr;
        self.close_later(addr);
        Some((addr, wire::refused(reason)))
    }

    /// The transport closes an address a moment after now (its last words
    /// on the way first).
    fn close_later(&mut self, addr: A) {
        if self.closing.iter().all(|(a, _)| *a != addr) {
            if self.closing.len() >= CLOSING_KEPT {
                self.closing.remove(0);
            }
            self.closing.push((addr, None));
        }
    }

    /// How many refusals it keeps count of (a test's).
    #[cfg(test)]
    pub(crate) fn refusals_kept(&self) -> usize {
        self.refusals.len()
    }

    /// The addresses to close now, refused or gone a moment ago (iroh's
    /// connections: a machine not let in, dropped or left; on a LAN there
    /// is nothing to close). Not one a machine plays from again.
    pub fn to_close(&mut self, now: u64) -> Vec<A> {
        let mut out = Vec::new();
        let remotes = &self.remotes;
        self.closing.retain_mut(|(addr, since)| {
            if now.saturating_sub(*since.get_or_insert(now)) < CLOSE_AFTER {
                return true;
            }
            if !remotes.iter().any(|r| r.addr == *addr && r.stage != Stage::Gone) {
                out.push(*addr);
            }
            false
        });
        out
    }

    /// Sends a guest something too big for a bundle (a save's text for
    /// LOAD), as pieces numbered `id` (2 and on; 1 is the run so far).
    pub fn send(&mut self, index: Machine, id: u32, data: Arc<[u8]>) {
        if let Some(r) = self.remotes.iter_mut().find(|r| r.index == index && r.stage != Stage::Gone) {
            r.sending.push_back(sending(id, data));
        }
    }

    /// Whether a guest has all of what was sent it as `id`.
    pub fn delivered(&self, index: Machine, id: u32) -> bool {
        self.remotes.iter().find(|r| r.index == index).is_none_or(|r| r.stage == Stage::Gone || r.sending.iter().all(|s| s.id != id))
    }

    /// The commit a machine's program was built from, as it said (people
    /// read it beside a difference).
    pub fn commit(&self, index: Machine) -> Option<&str> {
        self.remotes.iter().rev().find(|r| r.index == index).map(|r| r.commit.as_str())
    }

    /// The guests let in and not gone.
    pub fn guests(&self) -> Vec<Machine> {
        self.remotes.iter().filter(|r| matches!(r.stage, Stage::Welcomed | Stage::Playing)).map(|r| r.index).collect()
    }

    /// Whether every guest has every bundle built so far.
    pub fn all_delivered(&self) -> bool {
        let next = self.next_tick();
        self.remotes.iter().filter(|r| matches!(r.stage, Stage::Welcomed | Stage::Playing)).all(|r| r.got >= next)
    }

    /// The machines playing (not asking, not gone), the host first.
    pub fn roster(&self) -> Vec<Member> {
        let mut list = vec![Member { index: 0, name: self.name.clone(), rtt_ms: 0, quiet: false }];
        for r in self.remotes.iter().filter(|r| matches!(r.stage, Stage::Welcomed | Stage::Playing)) {
            list.push(Member { index: r.index, name: r.name.clone(), rtt_ms: r.rtt_us.map_or(0, |t| (t / 1000).min(u16::MAX as u64) as u16), quiet: r.quiet });
        }
        list
    }

    /// Drops a guest (DROP on the pause page): its seats leave.
    pub fn drop_machine(&mut self, index: Machine) {
        self.gone(index, true);
    }

    fn gone(&mut self, index: Machine, dropped: bool) {
        let Some(r) = self.remotes.iter_mut().find(|r| r.index == index && r.stage != Stage::Gone) else { return };
        let was = r.stage;
        r.stage = Stage::Gone;
        r.sending.clear();
        let addr = r.addr;
        self.close_later(addr);
        if was == Stage::Waiting {
            return;
        }
        for (n, seat) in self.seats.iter().enumerate() {
            if seat.owner == Some(index) {
                self.asks.push_back((index, u32::MAX, Ask::Leave(n as u8 + 1)));
            }
        }
        self.notices.push(Notice::Left { index, dropped });
    }

    /// The state hashes by parts the guests reported since asked: whose,
    /// at which tick.
    pub fn checks(&mut self) -> Vec<(Machine, u64, Vec<u64>)> {
        std::mem::take(&mut self.checks)
    }

    /// Tells a guest where its game ran apart from the host's; it leaves
    /// (or is dropped after a while).
    pub fn tell_apart(&mut self, index: Machine, apart: Apart) {
        if let Some(r) = self.remotes.iter_mut().find(|r| r.index == index && r.apart.is_none()) {
            r.apart = Some((apart.clone(), u64::MAX));
            self.notices.push(Notice::Apart { index, apart });
        }
    }

    /// What it answers a search for games on the LAN with (`Here`: the
    /// seats are filled in as they are).
    pub fn set_here(&mut self, here: wire::Here) {
        self.here = Some(here);
    }

    /// Seats taken, and seats (a matchmaker's room shows them).
    pub fn seats_taken(&self) -> (u8, u8) {
        (self.seats.iter().filter(|s| s.owner.is_some()).count() as u8, self.seats.len() as u8)
    }

    /// A guest's name as it said it.
    pub fn name(&self, index: Machine) -> Option<&str> {
        self.remotes.iter().rev().find(|r| r.index == index).map(|r| r.name.as_str())
    }

    /// A line every screen shows for a while.
    pub fn say(&mut self, line: &str) {
        self.said = Some((line.to_string(), None));
    }

    /// What happened since asked.
    pub fn notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// The most bytes a bundle has: with the biggest head a DOWN has, it
    /// still fits a datagram of the transport.
    fn max_bundle(&self) -> usize {
        self.limit.saturating_sub(MAX_HEAD)
    }

    /// The most bytes of the session's own (`extra`) a bundle carries: with
    /// every seat's keys it still fits a datagram (seats taken and controls
    /// changed wait for the next bundle where they do not fit).
    pub fn extra_room(&self) -> usize {
        // The extra's length takes two bytes.
        let fixed = Bundle { tick: u64::MAX, inputs: vec![Some(WIDEST); self.seats.len()], ..Bundle::default() }.size() + 2;
        self.max_bundle().saturating_sub(fixed).min(wire::MAX_EXTRA)
    }

    /// Builds the next tick's bundle with the session's bytes `extra`: the
    /// asks first (seats taken and left, controls), as many as fit a
    /// datagram (the rest, in order, go into the next), then what every
    /// seat holds. The host plays it, and every guest gets it. Too big:
    /// `extra` is longer than `extra_room`.
    pub fn tick(&mut self, extra: Vec<u8>) -> Result<Bundle, TooBig> {
        if extra.len() > self.extra_room() {
            return Err(TooBig);
        }
        let tick = self.next_tick();
        let most = self.max_bundle();
        // The bundle as it grows, every seat's keys as wide as they come.
        let mut probe = Bundle { tick, inputs: vec![Some(WIDEST); self.seats.len()], seats: Vec::new(), controls: Vec::new(), extra };
        while let Some((owner, ask_no, ask)) = self.asks.pop_front() {
            let (mut events, mut controls) = (probe.seats.clone(), probe.controls.clone());
            match &ask {
                Ask::Take => events.push(match self.seats.iter().position(|s| s.owner.is_none()) {
                    Some(n) => SeatEvent::Taken { seat: n as u8 + 1, owner, generation: self.seats[n].generation + 1, ask: ask_no },
                    None => SeatEvent::Refused { owner, ask: ask_no },
                }),
                Ask::Leave(seat) => {
                    if self.seats.get((*seat as usize).wrapping_sub(1)).is_some_and(|s| s.owner == Some(owner)) {
                        events.push(SeatEvent::Left { seat: *seat });
                        controls.retain(|(c, _)| c != seat);
                    }
                }
                Ask::Controls(seat, bytes) => {
                    // More than a guest reads is never sent: the seat keeps
                    // the controls it had, on every machine alike.
                    if bytes.len() > wire::MAX_CONTROLS {
                        eprintln!("seat {seat}'s controls take {} bytes, more than the network carries ({}): kept as they were", bytes.len(), wire::MAX_CONTROLS);
                        continue;
                    }
                    if self.owner((*seat).max(1)) == Some(owner) && *seat >= 1 {
                        controls.retain(|(c, _)| c != seat);
                        controls.push((*seat, bytes.clone()));
                    }
                }
            }
            let grown = Bundle { seats: events, controls, ..probe.clone() };
            if grown.seats.len() > wire::MAX_SEAT_EVENTS || grown.size() > most {
                // It waits for the next bundle, and every ask after it.
                self.asks.push_front((owner, ask_no, ask));
                break;
            }
            // Taken in: the seats as it leaves them.
            match grown.seats.last() {
                Some(&SeatEvent::Taken { seat, generation, .. }) if grown.seats.len() > probe.seats.len() => {
                    let n = seat as usize - 1;
                    self.seats[n] = Slot { owner: Some(owner), generation, ..Slot::default() };
                    self.own_last[n] = Input::default();
                }
                Some(&SeatEvent::Left { seat }) if grown.seats.len() > probe.seats.len() => self.seats[seat as usize - 1].owner = None,
                _ => {}
            }
            probe = grown;
        }
        let Bundle { seats: events, mut controls, extra, .. } = probe;
        controls.sort_by_key(|(seat, _)| *seat);
        let own = self.own_inputs(tick);
        let mut stopped = 0;
        let inputs = self
            .seats
            .iter_mut()
            .enumerate()
            .map(|(n, s)| match s.owner {
                None => None,
                Some(0) => Some(own[n]),
                Some(_) => {
                    let (input, late) = take(s);
                    stopped += late as u64;
                    Some(input)
                }
            })
            .collect();
        self.stopped += stopped;
        let bundle = Bundle { tick, inputs, seats: events, controls, extra };
        let bytes = wire::bundle_bytes(&bundle);
        debug_assert!(bytes.len() <= most, "a bundle of {} bytes", bytes.len());
        self.history.push(bytes);
        self.measure_delay();
        Ok(bundle)
    }

    /// The host's own seats' keys for `tick`: the readings held back long
    /// enough, merged (the newest, every action held in any of them); a
    /// seat without one goes on with its last.
    fn own_inputs(&mut self, tick: u64) -> Vec<Input> {
        let mut merged: Vec<Option<Input>> = vec![None; self.seats.len()];
        while let Some(&(read, seat, input)) = self.own.front() {
            if read + self.delay as u64 > tick {
                break;
            }
            self.own.pop_front();
            let n = seat as usize - 1;
            if self.seats[n].owner != Some(0) {
                continue;
            }
            self.own_last[n] = input;
            let m = merged[n].get_or_insert(input);
            m.actions |= input.actions;
            (m.left, m.right, m.up, m.down) = (input.left, input.right, input.up, input.down);
        }
        merged.iter().enumerate().map(|(n, m)| m.unwrap_or(self.own_last[n])).collect()
    }

    /// The delay a guest's keys have (its round trip in ticks and its
    /// hand), the longest of those that sit: the host's own keys are held
    /// back as long, moving a tick at a time after 2 s.
    fn measure_delay(&mut self) {
        let seated = |r: &Remote<A>| self.seats.iter().any(|s| s.owner == Some(r.index));
        let wanted = self
            .remotes
            .iter()
            .filter(|r| r.stage == Stage::Playing && seated(r))
            .map(|r| (r.rtt_us.unwrap_or(0).div_ceil(TICK_US) as u32 + r.hand as u32).clamp(MIN_DELAY, MAX_DELAY))
            .max()
            .unwrap_or(0);
        let way = (wanted as i32 - self.delay as i32).signum();
        if way == 0 {
            self.drift = (0, 0);
            return;
        }
        self.drift = if self.drift.0 == way { (way, self.drift.1 + 1) } else { (way, 1) };
        if self.drift.1 >= DELAY_PATIENCE {
            self.delay = (self.delay as i32 + way) as u32;
            self.drift = (0, 0);
        }
    }

    /// A datagram from `from`.
    pub fn receive(&mut self, now: u64, from: A, datagram: &[u8]) {
        let Ok((kind, key, mut r)) = wire::open(datagram) else { return };
        // A search for games: answered to where it came from.
        if kind == Kind::Find {
            let last = self.answered.insert(from, now);
            if last.is_some_and(|t| now.saturating_sub(t) < 200_000) {
                return;
            }
            if self.answered.len() > 256 {
                self.answered.retain(|_, t| now.saturating_sub(*t) < 1_000_000);
            }
            if let Some(here) = &self.here {
                let taken = self.seats.iter().filter(|s| s.owner.is_some()).count() as u8;
                let answer = wire::Here { seats: self.seats.len() as u8, taken, ..here.clone() };
                self.refused.push((from, answer.write()));
            }
            return;
        }
        if kind == Kind::Hello {
            if wire::version_of(datagram) != Some(wire::VERSION) {
                let why = format!("this game speaks the network's version {}, that program another: the same build on every machine", wire::VERSION);
                self.refuse_to(now, from, &why);
                return;
            }
            if let Ok(hello) = Hello::read(&mut r) {
                self.hello(now, from, hello);
            }
            return;
        }
        if key != self.key {
            return;
        }
        let Some(at) = self.remotes.iter().position(|r| r.addr == from && r.stage != Stage::Gone && r.stage != Stage::Waiting) else { return };
        match kind {
            Kind::Up => {
                if let Ok(up) = Up::read(&mut r) {
                    self.up(now, at, up);
                }
            }
            Kind::PieceAck => {
                if let Ok(ack) = PieceAck::read(&mut r) {
                    let rem = &mut self.remotes[at];
                    rem.last_heard = now;
                    if let Some(s) = rem.sending.iter_mut().find(|s| s.id == ack.blob) {
                        if ack.got > s.got {
                            s.got = ack.got.min(s.count);
                            let got = s.got;
                            s.sent.retain(|&p, _| p >= got);
                        }
                        s.mask = ack.mask;
                    }
                    rem.sending.retain(|s| s.got < s.count);
                }
            }
            Kind::Bye => {
                let index = self.remotes[at].index;
                self.gone(index, false);
            }
            _ => {}
        }
    }

    fn hello(&mut self, now: u64, from: A, hello: Hello) {
        if let Some(r) = self.remotes.iter_mut().find(|r| r.id == hello.id && r.stage != Stage::Gone) {
            if r.nonce == hello.nonce {
                // Its HELLO again: the WELCOME went astray (or a copy came
                // late, while it plays).
                if r.stage != Stage::Playing {
                    r.addr = from;
                    r.last_welcome = None;
                }
                return;
            }
            // Started anew: what it had is gone.
            let index = r.index;
            self.gone(index, false);
        }
        let playing = self.remotes.iter().filter(|r| r.stage != Stage::Gone).count();
        if playing + 1 >= wire::MAX_MACHINES || self.joined == u8::MAX {
            self.refuse_to(now, from, "the game is full");
            return;
        }
        self.joined += 1;
        let index = self.joined;
        self.remotes.push(Remote {
            addr: from,
            id: hello.id,
            nonce: hello.nonce,
            name: hello.name.clone(),
            commit: hello.commit.clone(),
            index,
            stage: Stage::Waiting,
            welcome: None,
            last_heard: now,
            last_welcome: None,
            last_down: None,
            quiet: false,
            rtt_us: None,
            hand: 1,
            got: 0,
            mask: 0,
            asks_done: 0,
            sending: VecDeque::new(),
            apart: None,
        });
        self.notices.push(Notice::Joining { index, name: hello.name, commit: hello.commit });
    }

    /// A refusal to an address, five a minute for one reason at most; the
    /// address is closed a moment later. The counts of minutes gone are
    /// forgotten, and at most `REFUSED_KEPT` addresses are answered in one.
    fn refuse_to(&mut self, now: u64, to: A, why: &str) {
        let minute = now / 60_000_000;
        self.refusals.retain(|_, (m, _)| *m == minute);
        let key = (to, why.to_string());
        if self.refusals.len() >= REFUSED_KEPT && !self.refusals.contains_key(&key) {
            return;
        }
        let count = self.refusals.entry(key).or_insert((minute, 0));
        count.1 += 1;
        if count.1 <= REFUSALS {
            self.refused.push((to, wire::refused(why)));
            self.close_later(to);
        }
    }

    fn up(&mut self, now: u64, at: usize, up: Up) {
        let r = &mut self.remotes[at];
        r.last_heard = now;
        if r.stage == Stage::Welcomed {
            r.stage = Stage::Playing;
        }
        if r.quiet {
            r.quiet = false;
            self.notices.push(Notice::Back { index: r.index });
        }
        if up.echo != 0 {
            let sample = ((now / 1000) as u32).wrapping_sub(up.echo).saturating_sub(up.held as u32) as u64 * 1000;
            if sample < SILENT {
                r.rtt_us = Some(r.rtt_us.map_or(sample, |rtt| (rtt * 7 + sample) / 8));
            }
        }
        if up.got >= r.got {
            r.got = up.got;
            r.mask = up.mask;
        }
        r.hand = up.hand.min(MAX_HAND);
        let index = r.index;
        for (tick, parts) in up.checks {
            if !self.checks.iter().any(|(m, t, _)| *m == index && *t == tick) {
                self.checks.push((index, tick, parts));
            }
        }
        for readings in &up.readings {
            let Some(s) = self.seats.get_mut((readings.seat as usize).wrapping_sub(1)) else { continue };
            if s.owner != Some(index) || s.generation != readings.generation {
                continue;
            }
            for (n, &input) in readings.inputs.iter().enumerate() {
                let seq = readings.first.wrapping_add(n as u32);
                if s.newest_seq.is_none_or(|newest| seq > newest) {
                    s.held |= input.actions;
                    (s.newest, s.newest_seq) = (input, Some(seq));
                }
            }
        }
        let r = &mut self.remotes[at];
        for (n, ask) in up.asks.into_iter().enumerate() {
            let no = up.asks_from.wrapping_add(n as u32);
            if no == r.asks_done {
                r.asks_done += 1;
                self.asks.push_back((index, no, ask));
            }
        }
    }

    /// The datagrams to send now: WELCOMEs, refusals, every guest the
    /// bundles it lacks and the pieces it waits for, a word where nothing
    /// else went for a while; and who went quiet or silent.
    pub fn outgoing(&mut self, now: u64) -> Vec<(A, Vec<u8>)> {
        let mut out = std::mem::take(&mut self.refused);
        let quiet: Vec<(Machine, bool)> = self
            .remotes
            .iter()
            .filter(|r| r.stage != Stage::Gone)
            .filter_map(|r| {
                let since = now.saturating_sub(r.last_heard);
                (since >= SILENT).then_some((r.index, true)).or((since >= QUIET && !r.quiet && r.stage == Stage::Playing).then_some((r.index, false)))
            })
            .collect();
        for (index, silent) in quiet {
            if silent {
                self.gone(index, true);
            } else if let Some(r) = self.remotes.iter_mut().find(|r| r.index == index) {
                r.quiet = true;
                self.notices.push(Notice::Quiet { index });
            }
        }
        // A guest told long enough where its game ran apart is dropped.
        let told: Vec<Machine> = self.remotes.iter().filter(|r| r.stage != Stage::Gone && r.apart.as_ref().is_some_and(|(_, since)| *since != u64::MAX && now.saturating_sub(*since) >= TELLING)).map(|r| r.index).collect();
        for index in told {
            self.gone(index, true);
        }
        if let Some((_, since)) = &mut self.said {
            let at = *since.get_or_insert(now);
            if now.saturating_sub(at) >= TELLING {
                self.said = None;
            }
        }
        let said = self.said.as_ref().map(|(line, _)| line.clone());
        let roster = self.last_roster.is_none_or(|t| now.saturating_sub(t) >= ROSTER_EVERY).then(|| self.roster());
        if roster.is_some() {
            self.last_roster = Some(now);
        }
        let stamp = ((now / 1000) as u32).max(1);
        let next = self.next_tick();
        for r in &mut self.remotes {
            match r.stage {
                Stage::Waiting | Stage::Gone => continue,
                Stage::Welcomed => {
                    if r.last_welcome.is_none_or(|t| now.saturating_sub(t) >= WELCOME_AGAIN)
                        && let Some(w) = &r.welcome
                    {
                        r.last_welcome = Some(now);
                        out.push((r.addr, w.write(self.key)));
                    }
                    continue;
                }
                Stage::Playing => {}
            }
            let acked: Vec<(u8, u32)> = self.seats.iter().enumerate().filter(|(_, s)| s.owner == Some(r.index)).filter_map(|(n, s)| Some((n as u8 + 1, s.newest_seq?))).collect();
            if let Some((_, since)) = &mut r.apart
                && *since == u64::MAX
            {
                *since = now;
            }
            let apart = r.apart.as_ref().map(|(a, _)| a.clone());
            let mut head = Down { stamp, asks_done: r.asks_done, acked, paused: self.paused, roster: roster.clone(), apart, said: said.clone(), bundles: Vec::new() };
            // Who plays, in a datagram of its own (sixteen names fill one).
            if head.roster.is_some() {
                out.push((r.addr, wire::down(&head, self.key, &[], self.limit).0));
                head.roster = None;
                r.last_down = Some(now);
            }
            // Every bundle it lacks, oldest first, in every datagram until it
            // says it has it: one lost costs nothing while the next comes.
            let lacks: Vec<u64> = (r.got..next).filter(|&t| t == r.got || t - r.got > 64 || r.mask >> (t - r.got - 1) & 1 == 0).collect();
            let (mut from, mut sent) = (0, 0);
            loop {
                let bundles: Vec<&[u8]> = lacks[from..].iter().map(|&t| self.history[t as usize].as_slice()).collect();
                let (datagram, n) = wire::down(&head, self.key, &bundles, self.limit);
                if n == 0 {
                    // Nothing to send: a word now and then all the same.
                    if sent == 0 && r.last_down.is_none_or(|t| now.saturating_sub(t) >= HEARTBEAT) {
                        out.push((r.addr, datagram));
                        r.last_down = Some(now);
                    }
                    break;
                }
                out.push((r.addr, datagram));
                r.last_down = Some(now);
                (from, sent) = (from + n, sent + 1);
                if sent == BUDGET || from == lacks.len() {
                    break;
                }
            }
            // The pieces it waits for, the oldest thing first, again when
            // they may be lost.
            let rto = r.rtt_us.map_or(50_000, |t| (t * 2).max(30_000));
            let mut budget = PIECE_BUDGET;
            for s in &mut r.sending {
                for p in s.got..s.count.min(s.got + 65) {
                    if budget == 0 {
                        break;
                    }
                    let had = p > s.got && s.mask >> (p - s.got - 1) & 1 == 1;
                    if had || s.sent.get(&p).is_some_and(|&when| now.saturating_sub(when) < rto) {
                        continue;
                    }
                    let start = p as usize * PIECE;
                    let data = s.data[start..(start + PIECE).min(s.data.len())].to_vec();
                    out.push((r.addr, wire::Piece { blob: s.id, index: p, count: s.count, data }.write(self.key)));
                    s.sent.insert(p, now);
                    budget -= 1;
                }
            }
        }
        out
    }

    /// The farewell to every guest (sent a few times: it may go astray).
    pub fn farewell(&self) -> Vec<(A, Vec<u8>)> {
        self.remotes.iter().filter(|r| r.stage != Stage::Gone).map(|r| (r.addr, wire::bye(self.key))).collect()
    }
}

/// The most a guest's hand counts for the delay.
const MAX_HAND: u8 = 6;

/// A seat's input for the tick being built: the newest reading if one
/// came since the last tick, with every action held in any that came;
/// else the last again for a while, then nothing held.
fn take(s: &mut Slot) -> (Input, bool) {
    match s.newest_seq {
        Some(seq) if s.used.is_none_or(|used| seq != used) => {
            let mut input = s.newest;
            input.actions |= s.held;
            (s.used, s.held, s.repeats) = (Some(seq), 0, 0);
            (input, false)
        }
        Some(_) if s.repeats < REPEAT => {
            s.repeats += 1;
            (s.newest, false)
        }
        // Nothing came for a while: its figure stops.
        Some(_) => (Input::default(), true),
        None => (Input::default(), false),
    }
}

fn sending(id: u32, data: Arc<[u8]>) -> Sending {
    let count = (data.len().div_ceil(PIECE) as u32).max(1);
    Sending { id, data, count, got: 0, mask: 0, sent: BTreeMap::new() }
}
