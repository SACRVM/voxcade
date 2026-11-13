//! A guest's side of a network game: it sends its players' keys at once,
//! every reading numbered, and plays the host's bundles in order as they
//! come, keeping a few in hand against the network's jitter
//! (`docs/multiplayer.md`, stage 4).

use std::collections::{BTreeMap, VecDeque};

use super::wire::{self, Apart, Ask, Bundle, Down, Hello, Kind, Member, PieceAck, Readings, SeatEvent, Up, Welcome};
use super::{HEARTBEAT, MAX_CATCH_UP, Machine, MachineId, SILENT, TICK_US};
use crate::input::Input;

/// Why a guest's network game ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gone {
    /// Nobody answered its HELLO.
    NoAnswer,
    /// The host would not have it, and said why.
    Refused(String),
    /// The host said farewell, or went silent.
    HostLeft,
    /// It could not play as fast as the host (more than 2 s in hand).
    CannotKeepUp,
    /// It left (its players' QUIT TO TITLE).
    Left,
    /// Its game ran apart from the host's there, the host said.
    Apart(Apart),
}

#[derive(Clone, Debug, PartialEq)]
enum Stage {
    Joining { since: u64, last: Option<u64> },
    Playing,
    Gone(Gone),
}

/// Something coming in pieces.
struct Assembly {
    count: u32,
    pieces: Vec<Option<Vec<u8>>>,
    got: u32,
}

/// A guest's side, talking to the host at an address of type `A`.
pub struct Guest<A> {
    host: A,
    hello: Hello,
    stage: Stage,
    key: u64,
    welcome: Option<Welcome>,
    /// Bundles come and not yet given out (from `next`), and the first not
    /// come with all before it (`seen`: its seat events taken in).
    hand: BTreeMap<u64, Bundle>,
    next: u64,
    seen: u64,
    /// Its seats (generation, the ask that took it), and the asks answered
    /// since asked (a seat, or none free).
    mine: BTreeMap<u8, (u32, u32)>,
    answers: Vec<(u32, Option<u8>)>,
    /// Each seat's next reading's number and those not yet acknowledged.
    readings: BTreeMap<u8, (u32, VecDeque<(u32, Input)>)>,
    asks: VecDeque<(u32, Ask)>,
    next_ask: u32,
    last_heard: u64,
    /// The host's last stamp and when it came.
    stamp: Option<(u32, u64)>,
    last_up: Option<u64>,
    told: (u64, u64),
    /// Playing at the host's pace: time owed in µs, the last frame, the
    /// ticks it aims to keep in hand, how often it ran dry in this window
    /// and for how many windows it has not, since when it holds too many.
    owed: u64,
    last_due: Option<u64>,
    /// What was left in hand as the last tick was played.
    left: u32,
    target: u32,
    window: u64,
    dry: u32,
    calm: u32,
    too_many: Option<u64>,
    assembling: BTreeMap<u32, Assembly>,
    /// Things come whole: their pieces, and the bytes until taken.
    assembled: BTreeMap<u32, (u32, Option<Vec<u8>>)>,
    pieces_due: bool,
    paused: Option<Machine>,
    roster: Vec<Member>,
    /// Its state hash by parts at ticks the host compares, each sent a few
    /// times (one may go astray), and the host's lines for every screen.
    checks: VecDeque<(u64, Vec<u64>, u8)>,
    said: Vec<String>,
    last_said: Option<String>,
    /// The biggest datagram it sends: the transport's.
    limit: usize,
}

/// A check is sent this many times.
const CHECK_TIMES: u8 = 3;

/// A HELLO is said again this often (µs), and given up after 10 s.
const HELLO_AGAIN: u64 = 250_000;
const HELLO_FOR: u64 = 10_000_000;
/// The hand's target lies between these (ticks).
const MIN_HAND: u32 = 1;
const MAX_HAND: u32 = 6;
/// It is weighed every 2 s; dry twice in one raises it, four calm in a
/// row lower it.
const WINDOW: u64 = 2_000_000;
/// More in hand than this for this long: it cannot keep up.
const TOO_MANY: usize = 120;
const TOO_LONG: u64 = 2_000_000;
/// Bundles further ahead than this are not kept.
const AHEAD: u64 = 100_000;
/// Things in pieces at once, at most.
const ASSEMBLING: usize = 4;
/// Datagrams its asks fill at most in one go.
const ASK_DATAGRAMS: usize = 4;

impl<A: Copy + PartialEq + std::fmt::Debug> Guest<A> {
    /// Joins the game at `host` as the machine `id`, `nonce` a random
    /// number of this joining's own: its HELLO goes out at once.
    pub fn new(host: A, id: MachineId, nonce: u64, name: &str, commit: &str, now: u64) -> Self {
        let cut = |s: &str| s.chars().take(32).collect::<String>();
        Self {
            host,
            hello: Hello { id, nonce, name: cut(name), commit: cut(commit) },
            stage: Stage::Joining { since: now, last: None },
            key: 0,
            welcome: None,
            hand: BTreeMap::new(),
            next: 0,
            seen: 0,
            mine: BTreeMap::new(),
            answers: Vec::new(),
            readings: BTreeMap::new(),
            asks: VecDeque::new(),
            next_ask: 0,
            last_heard: now,
            stamp: None,
            last_up: None,
            told: (u64::MAX, 0),
            owed: 0,
            last_due: None,
            left: 0,
            target: MIN_HAND,
            window: now,
            dry: 0,
            calm: 0,
            too_many: None,
            assembling: BTreeMap::new(),
            assembled: BTreeMap::new(),
            pieces_due: false,
            paused: None,
            roster: Vec::new(),
            checks: VecDeque::new(),
            said: Vec::new(),
            last_said: None,
            limit: wire::MAX_DATAGRAM,
        }
    }

    /// Its state hash by parts after `tick` (a tick the host compares): sent
    /// with what goes to the host.
    pub fn check(&mut self, tick: u64, parts: Vec<u64>) {
        self.checks.push_back((tick, parts, 0));
        while self.checks.len() > wire::MAX_CHECKS {
            self.checks.pop_front();
        }
    }

    /// The host's lines for every screen since asked.
    pub fn said(&mut self) -> Vec<String> {
        std::mem::take(&mut self.said)
    }

    /// What the host said letting it in, once it did.
    pub fn welcome(&self) -> Option<&Welcome> {
        self.welcome.as_ref()
    }

    /// Why it ended, once it did.
    pub fn gone(&self) -> Option<&Gone> {
        match &self.stage {
            Stage::Gone(why) => Some(why),
            _ => None,
        }
    }

    pub fn paused(&self) -> Option<Machine> {
        self.paused
    }

    /// How long the host has said nothing (µs).
    pub fn silent_for(&self, now: u64) -> u64 {
        now.saturating_sub(self.last_heard)
    }

    /// Who plays, as the host said last.
    pub fn roster(&self) -> &[Member] {
        &self.roster
    }

    /// Its seats (from 1) as the bundles come say: each with the ask that
    /// took it.
    pub fn mine(&self) -> Vec<(u8, u32)> {
        self.mine.iter().map(|(&seat, &(_, ask))| (seat, ask)).collect()
    }

    /// The asks for a seat answered since asked: the seat it got, or none
    /// (all taken).
    pub fn answers(&mut self) -> Vec<(u32, Option<u8>)> {
        std::mem::take(&mut self.answers)
    }

    /// Bundles come and not given out, with none missing before them.
    pub fn in_hand(&self) -> usize {
        (self.seen - self.next) as usize
    }

    /// The ticks it aims to keep in hand.
    pub fn target(&self) -> u32 {
        self.target
    }

    /// The next tick it gives out.
    pub fn next_tick(&self) -> u64 {
        self.next
    }

    /// Asks for a seat for one more player here: the ask's number (its
    /// answer comes in `answers`).
    pub fn ask_seat(&mut self) -> u32 {
        self.push_ask(Ask::Take)
    }

    pub fn leave(&mut self, seat: u8) {
        self.push_ask(Ask::Leave(seat));
    }

    /// What the player on a seat plays with now (the session's bytes); more
    /// than the host reads is never sent (the seat keeps the controls it
    /// had, on every machine alike).
    pub fn controls(&mut self, seat: u8, bytes: Vec<u8>) {
        if bytes.len() > wire::MAX_CONTROLS {
            eprintln!("seat {seat}'s controls take {} bytes, more than the network carries ({}): kept as they were", bytes.len(), wire::MAX_CONTROLS);
            return;
        }
        self.push_ask(Ask::Controls(seat, bytes));
    }

    /// The biggest datagram its transport carries (iroh's QUIC carries
    /// less than a LAN's UDP): what it sends fits.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.min(wire::MAX_DATAGRAM);
    }

    fn push_ask(&mut self, ask: Ask) -> u32 {
        let n = self.next_ask;
        self.next_ask += 1;
        self.asks.push_back((n, ask));
        n
    }

    /// A player's keys on one of its seats, read this frame: sent at once.
    pub fn read(&mut self, seat: u8, input: Input) {
        if !self.mine.contains_key(&seat) {
            return;
        }
        let (next, list) = self.readings.entry(seat).or_insert((0, VecDeque::new()));
        list.push_back((*next, input));
        *next += 1;
        while list.len() > wire::MAX_READINGS {
            list.pop_front();
        }
    }

    /// Leaves the game: its farewell (sent a few times: it may go astray).
    pub fn leave_game(&mut self) -> Vec<(A, Vec<u8>)> {
        let was = matches!(self.stage, Stage::Playing);
        self.stage = Stage::Gone(Gone::Left);
        if was { vec![(self.host, wire::bye(self.key)); 10] } else { Vec::new() }
    }

    /// A datagram from `from`.
    pub fn receive(&mut self, now: u64, from: A, datagram: &[u8]) {
        if from != self.host || matches!(self.stage, Stage::Gone(_)) {
            return;
        }
        let Ok((kind, key, mut r)) = wire::open(datagram) else { return };
        match kind {
            Kind::Refused => {
                if let (Stage::Joining { .. }, Ok(why)) = (&self.stage, wire::read_refused(&mut r)) {
                    self.stage = Stage::Gone(Gone::Refused(why));
                }
            }
            Kind::Welcome => {
                if let (Stage::Joining { .. }, Ok(welcome)) = (&self.stage, Welcome::read(&mut r)) {
                    self.key = key;
                    (self.next, self.seen) = (welcome.first, welcome.first);
                    if welcome.blob > 0 {
                        let count = (welcome.blob as usize).div_ceil(wire::PIECE) as u32;
                        self.assembling.insert(1, Assembly { count, pieces: vec![None; count as usize], got: 0 });
                    }
                    self.welcome = Some(welcome);
                    self.stage = Stage::Playing;
                    self.last_heard = now;
                    self.window = now;
                }
            }
            _ if key != self.key || self.stage != Stage::Playing => {}
            Kind::Down => {
                if let Ok(down) = Down::read(&mut r) {
                    self.down(now, down);
                }
            }
            Kind::Piece => {
                if let Ok(piece) = wire::Piece::read(&mut r) {
                    self.last_heard = now;
                    self.piece(piece);
                }
            }
            Kind::Bye => self.stage = Stage::Gone(Gone::HostLeft),
            _ => {}
        }
    }

    fn down(&mut self, now: u64, down: Down) {
        self.last_heard = now;
        self.stamp = Some((down.stamp, now));
        while self.asks.front().is_some_and(|(n, _)| *n < down.asks_done) {
            self.asks.pop_front();
        }
        for (seat, seq) in down.acked {
            if let Some((_, list)) = self.readings.get_mut(&seat) {
                while list.front().is_some_and(|(n, _)| *n <= seq) {
                    list.pop_front();
                }
            }
        }
        self.paused = down.paused;
        if let Some(roster) = down.roster {
            self.roster = roster;
        }
        if down.said != self.last_said {
            self.said.extend(down.said.clone());
            self.last_said = down.said;
        }
        if let Some(apart) = down.apart {
            self.stage = Stage::Gone(Gone::Apart(apart));
            return;
        }
        for b in down.bundles {
            if b.tick >= self.next && b.tick < self.next + AHEAD {
                self.hand.entry(b.tick).or_insert(b);
            }
        }
        // The seats taken and left, in the ticks' order, as they come.
        let index = self.welcome.as_ref().map_or(0, |w| w.index);
        while let Some(b) = self.hand.get(&self.seen) {
            for e in &b.seats {
                match *e {
                    SeatEvent::Taken { seat, owner, generation, ask } if owner == index => {
                        self.mine.insert(seat, (generation, ask));
                        self.readings.remove(&seat);
                        self.answers.push((ask, Some(seat)));
                    }
                    SeatEvent::Taken { seat, .. } | SeatEvent::Left { seat } => {
                        self.mine.remove(&seat);
                        self.readings.remove(&seat);
                    }
                    SeatEvent::Refused { owner, ask } if owner == index => self.answers.push((ask, None)),
                    SeatEvent::Refused { .. } => {}
                }
            }
            self.seen += 1;
        }
    }

    fn piece(&mut self, p: wire::Piece) {
        self.pieces_due = true;
        if self.assembled.contains_key(&p.blob) {
            return;
        }
        if !self.assembling.contains_key(&p.blob) {
            if self.assembling.len() >= ASSEMBLING {
                return;
            }
            self.assembling.insert(p.blob, Assembly { count: p.count, pieces: vec![None; p.count as usize], got: 0 });
        }
        let Some(a) = self.assembling.get_mut(&p.blob) else { return };
        if p.count != a.count || (p.index + 1 < a.count && p.data.len() != wire::PIECE) {
            return;
        }
        a.pieces[p.index as usize].get_or_insert(p.data);
        while a.pieces.get(a.got as usize).is_some_and(Option::is_some) {
            a.got += 1;
        }
        if a.got == a.count {
            let whole: Vec<u8> = a.pieces.iter_mut().flat_map(|p| p.take().unwrap_or_default()).collect();
            self.assembling.remove(&p.blob);
            self.assembled.insert(p.blob, (p.count, Some(whole)));
        }
    }

    /// Something sent in pieces (1: the run so far), once all of it came.
    pub fn take(&mut self, id: u32) -> Option<Vec<u8>> {
        self.assembled.get_mut(&id)?.1.take()
    }

    /// How far something in pieces has come: pieces come of how many.
    pub fn progress(&self, id: u32) -> Option<(u32, u32)> {
        self.assembling.get(&id).map(|a| (a.got, a.count))
    }

    /// The next bundle at once, without keeping pace (catching up).
    pub fn next_bundle(&mut self) -> Option<Bundle> {
        let b = self.hand.remove(&self.next)?;
        self.next += 1;
        Some(b)
    }

    /// The bundles to play this frame, at the host's pace: a little faster
    /// when more was left in hand as the last tick was played than it aims
    /// at, a little slower with less, up to `MAX_CATCH_UP` after a hitch;
    /// none in hand, the picture waits.
    pub fn due(&mut self, now: u64) -> Vec<Bundle> {
        let dt = self.last_due.map_or(0, |t| now.saturating_sub(t)).min(250_000);
        self.last_due = Some(now);
        let per_mille = match self.left.cmp(&self.target) {
            std::cmp::Ordering::Greater => 1020,
            std::cmp::Ordering::Less => 980,
            std::cmp::Ordering::Equal => 1000,
        };
        self.owed += dt * per_mille / 1000;
        let mut out = Vec::new();
        while self.owed >= TICK_US && out.len() < MAX_CATCH_UP {
            match self.next_bundle() {
                Some(b) => {
                    self.owed -= TICK_US;
                    self.left = self.in_hand() as u32;
                    out.push(b);
                }
                None => {
                    // Dry though it held as many as it aims at: the
                    // network is later than its hand allows.
                    if self.paused.is_none() && self.left >= self.target {
                        self.dry += 1;
                    }
                    break;
                }
            }
        }
        // A bundle a little late is played as it comes, the next ones on
        // time again; waiting longer (or paused) the pace starts anew.
        let wait = if self.paused.is_some() { 0 } else { MAX_HAND as u64 };
        self.owed = self.owed.min((self.in_hand() as u64 + 1 + wait) * TICK_US);
        if now.saturating_sub(self.window) >= WINDOW {
            self.window = now;
            if self.dry >= 2 {
                (self.target, self.calm) = ((self.target + 1).min(MAX_HAND), 0);
            } else {
                self.calm += 1;
                if self.calm >= 4 && self.target > MIN_HAND {
                    (self.target, self.calm) = (self.target - 1, 0);
                }
            }
            self.dry = 0;
        }
        if self.in_hand() > TOO_MANY {
            let since = *self.too_many.get_or_insert(now);
            if now.saturating_sub(since) >= TOO_LONG {
                self.stage = Stage::Gone(Gone::CannotKeepUp);
            }
        } else {
            self.too_many = None;
        }
        out
    }

    /// The datagrams to send now: HELLO while joining; then its readings
    /// and asks not yet acknowledged with the bundles it has, every frame
    /// it has seats or something to say, else now and then.
    pub fn outgoing(&mut self, now: u64) -> Vec<(A, Vec<u8>)> {
        match self.stage {
            Stage::Gone(_) => return Vec::new(),
            Stage::Joining { since, last } => {
                if now.saturating_sub(since) >= HELLO_FOR {
                    self.stage = Stage::Gone(Gone::NoAnswer);
                    return Vec::new();
                }
                if last.is_some_and(|t| now.saturating_sub(t) < HELLO_AGAIN) {
                    return Vec::new();
                }
                self.stage = Stage::Joining { since, last: Some(now) };
                return vec![(self.host, self.hello.write())];
            }
            Stage::Playing => {}
        }
        if now.saturating_sub(self.last_heard) >= SILENT {
            self.stage = Stage::Gone(Gone::HostLeft);
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut mask = 0u64;
        for i in 0..64 {
            if self.hand.contains_key(&(self.seen + 1 + i)) {
                mask |= 1 << i;
            }
        }
        let says = !self.mine.is_empty() || !self.asks.is_empty() || self.told != (self.seen, mask) || self.last_up.is_none_or(|t| now.saturating_sub(t) >= HEARTBEAT);
        if says {
            let (echo, held) = self.stamp.map_or((0, 0), |(s, at)| (s, (now.saturating_sub(at) / 1000).min(u16::MAX as u64) as u16));
            let readings = self
                .readings
                .iter()
                .filter_map(|(&seat, (_, list))| {
                    let first = list.front()?.0;
                    Some(Readings { seat, generation: self.mine.get(&seat)?.0, first, inputs: list.iter().map(|(_, i)| *i).collect() })
                })
                .collect();
            let asks_from = self.asks.front().map_or(self.next_ask, |(n, _)| *n);
            let mut up = Up { echo, held, got: self.seen, mask, hand: self.target as u8, readings, asks_from, asks: Vec::new(), checks: Vec::new() };
            // Every datagram fits the transport's (`limit`): the readings
            // with room left for the checks, the oldest of the longest left
            // out while they do not (they went before; the newest of every
            // seat always goes); then the checks, and the asks that fit, the
            // asks after them in more datagrams.
            let limit = self.limit;
            let fits = |up: &Up| up.write(0).len() <= limit;
            let checks: usize = 1 + self.checks.iter().map(|(_, parts, _)| 10 + 1 + 8 * parts.len()).sum::<usize>();
            while up.write(0).len() + checks > limit {
                let Some(longest) = up.readings.iter_mut().filter(|r| r.inputs.len() > 1).max_by_key(|r| r.inputs.len()) else { break };
                longest.inputs.remove(0);
                longest.first = longest.first.wrapping_add(1);
            }
            for c in &mut self.checks {
                up.checks.push((c.0, c.1.clone()));
                if !fits(&up) {
                    up.checks.pop();
                    break;
                }
                c.2 += 1;
            }
            self.checks.retain(|c| c.2 < CHECK_TIMES);
            let mut asks = self.asks.iter().map(|(_, a)| a.clone()).peekable();
            for _ in 0..ASK_DATAGRAMS {
                while up.asks.len() < wire::MAX_ASKS
                    && let Some(ask) = asks.peek()
                {
                    up.asks.push(ask.clone());
                    if !fits(&up) {
                        up.asks.pop();
                        break;
                    }
                    asks.next();
                }
                let sent = up.asks.len() as u32;
                out.push((self.host, up.write(self.key)));
                let alone = up.readings.is_empty() && up.checks.is_empty();
                if asks.peek().is_none() || (sent == 0 && alone) {
                    break;
                }
                // The asks after them, alone.
                up = Up { asks_from: up.asks_from.wrapping_add(sent), asks: Vec::new(), readings: Vec::new(), checks: Vec::new(), ..up };
            }
            self.last_up = Some(now);
            self.told = (self.seen, mask);
        }
        if std::mem::take(&mut self.pieces_due) {
            for (&blob, a) in &self.assembling {
                let mut mask = 0u64;
                for i in 0..64 {
                    if a.pieces.get((a.got + 1 + i) as usize).is_some_and(Option::is_some) {
                        mask |= 1 << i;
                    }
                }
                out.push((self.host, PieceAck { blob, got: a.got, mask }.write(self.key)));
            }
            for (&blob, &(count, _)) in &self.assembled {
                out.push((self.host, PieceAck { blob, got: count, mask: 0 }.write(self.key)));
            }
        }
        out
    }
}
