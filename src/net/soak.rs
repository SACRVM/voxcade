//! The protocol played on the simulated network by a stub game, as the
//! tests and the soak use it (`voxcade soak`, the Builds workflow: the
//! same digest on every system): a hash of every bundle played, kept
//! after every tick. Every tick two machines played must have the same
//! hash on both, whatever the network did.

use super::wire::{Bundle, SeatEvent};
use super::*;
use crate::hash::{Stable, mix};
use crate::input::Input;

/// A game reduced to what lockstep needs.
#[derive(Clone)]
pub(crate) struct Stub {
    pub(crate) hash: Stable,
    pub first: u64,
    pub after: Vec<u64>,
    /// The bundles played (the host's, to look at).
    pub log: Vec<Bundle>,
}

pub(crate) fn bits(i: &Input) -> u64 {
    i.left as u64 | (i.right as u64) << 1 | (i.up as u64) << 2 | (i.down as u64) << 3 | (i.actions as u64) << 4
}

impl Stub {
    pub(crate) fn new() -> Self {
        Self { hash: Stable::default(), first: 0, after: Vec::new(), log: Vec::new() }
    }

    pub(crate) fn play(&mut self, b: &Bundle) {
        assert_eq!(b.tick, self.first + self.after.len() as u64, "ticks in order, none twice");
        let s = &mut self.hash;
        s.u64(b.tick);
        for i in &b.inputs {
            s.u64(i.as_ref().map_or(0, |i| 1 + bits(i)));
        }
        for e in &b.seats {
            match *e {
                SeatEvent::Taken { seat, owner, generation, ask } => s.u64(1).u64(seat as u64).u64(owner as u64).u64(generation as u64).u64(ask as u64),
                SeatEvent::Left { seat } => s.u64(2).u64(seat as u64),
                SeatEvent::Refused { owner, ask } => s.u64(3).u64(owner as u64).u64(ask as u64),
            };
        }
        for (seat, c) in &b.controls {
            s.u64(*seat as u64).bytes(c);
        }
        s.bytes(&b.extra);
        self.after.push(s.finish());
        self.log.push(b.clone());
    }

    pub(crate) fn next(&self) -> u64 {
        self.first + self.after.len() as u64
    }

    pub(crate) fn at(&self, tick: u64) -> Option<u64> {
        self.after.get(tick.checked_sub(self.first)? as usize).copied()
    }

    /// The run so far, as a guest coming late gets it: the next tick, the
    /// hash, and `pad` bytes of filling.
    pub(crate) fn blob(&self, pad: usize) -> Vec<u8> {
        let mut out = self.next().to_le_bytes().to_vec();
        out.extend_from_slice(&self.hash.finish().to_le_bytes());
        out.extend((0..pad).map(|n| mix(n as u64) as u8));
        out
    }

    pub(crate) fn from_blob(blob: &[u8], pad: usize) -> Self {
        assert_eq!(blob.len(), 16 + pad, "the run so far came whole");
        assert!(blob[16..].iter().enumerate().all(|(n, &b)| b == mix(n as u64) as u8), "and unchanged");
        let first = u64::from_le_bytes(blob[..8].try_into().unwrap());
        let hash = Stable::resume(u64::from_le_bytes(blob[8..16].try_into().unwrap()));
        Self { hash, first, after: Vec::new(), log: Vec::new() }
    }
}

pub(crate) enum Role {
    Host(Box<Host<usize>>),
    Guest(Box<Guest<usize>>),
}

/// A machine on the simulated network: its clock (an offset and a drift
/// in millionths), its frame rate, its game and its one player's keys.
pub(crate) struct Node {
    pub(crate) role: Role,
    pub(crate) offset: u64,
    pub(crate) drift_ppm: i64,
    pub(crate) frame_us: u64,
    pub(crate) next_frame: u64,
    pub(crate) owed: u64,
    pub(crate) frames: u64,
    pub(crate) game: Option<Stub>,
    /// Joins after this long (µs of its clock), takes a seat or only
    /// watches, taps an action now and then.
    pub(crate) join_at: u64,
    pub(crate) seat: bool,
    pub(crate) taps: bool,
    pub(crate) tapped: u64,
    pub(crate) pad: usize,
    /// When its last frame was; switched off, it does nothing more.
    pub(crate) last: Option<u64>,
    pub(crate) off: bool,
}

impl Node {
    pub(crate) fn local(&self, now: u64) -> u64 {
        (now as i64 + now as i64 * self.drift_ppm / 1_000_000) as u64 + self.offset
    }

    /// Its player's keys this frame: held a while, then others; with taps,
    /// a third action pressed for one frame now and then.
    pub(crate) fn keys(&self, addr: usize) -> Input {
        let h = mix((self.frames / 23) ^ ((addr as u64) << 40));
        let mut input = Input { left: h & 1 != 0, right: h & 2 != 0, up: h & 4 != 0, down: h & 8 != 0, actions: (h >> 4 & 3) as u32 };
        if self.taps && self.frames.is_multiple_of(37) {
            input.actions |= 4;
        }
        input
    }
}

pub(crate) struct Run {
    pub(crate) net: SimNet,
    pub(crate) nodes: Vec<Node>,
    pub(crate) now: u64,
}

impl Run {
    /// A host (machine 0) and `guests` guests on `link`, every machine at
    /// 60 frames a second but the second guest at 144.
    pub(crate) fn new(seed: u64, link: Link, guests: usize) -> Self {
        let mut nodes = vec![node(Role::Host(Box::new(Host::new(mix(seed) | 1, "host", 4))), 16_667)];
        nodes[0].game = Some(Stub::new());
        for g in 0..guests {
            let id = [g as u8 + 1; 16];
            let frame = if g == 1 { 6_944 } else { 16_667 };
            let mut n = node(Role::Guest(Box::new(Guest::new(0, id, mix(seed + g as u64), &format!("guest {}", g + 1), "test", 0))), frame);
            n.offset = mix(seed ^ g as u64) % 1_000_000;
            nodes.push(n);
        }
        Self { net: SimNet::new(seed, link), nodes, now: 0 }
    }

    pub(crate) fn host(&self) -> &Host<usize> {
        match &self.nodes[0].role {
            Role::Host(h) => h,
            Role::Guest(_) => unreachable!("machine 0 hosts"),
        }
    }

    #[cfg(test)]
    pub(crate) fn guest(&self, n: usize) -> &Guest<usize> {
        match &self.nodes[n].role {
            Role::Guest(g) => g,
            Role::Host(_) => unreachable!("machine 0 hosts"),
        }
    }

    /// Plays on until `until` µs of the network's time, in steps of 500 µs,
    /// the machines in order.
    pub(crate) fn to(&mut self, until: u64) {
        while self.now < until {
            self.now += 500;
            self.net.advance(self.now);
            for (from, to, bytes) in self.net.deliver() {
                let node = &mut self.nodes[to];
                let now = node.local(self.now);
                match &mut node.role {
                    Role::Host(h) => h.receive(now, from, &bytes),
                    Role::Guest(g) => g.receive(now, from, &bytes),
                }
            }
            for addr in 0..self.nodes.len() {
                let now = self.nodes[addr].local(self.now);
                if now >= self.nodes[addr].next_frame && !self.nodes[addr].off {
                    let node = &mut self.nodes[addr];
                    let dt = node.last.map_or(0, |t| now - t);
                    node.last = Some(now);
                    node.next_frame += node.frame_us;
                    let out = frame(node, addr, now, dt);
                    node.frames += 1;
                    for (to, bytes) in out {
                        self.net.send(addr, to, bytes);
                    }
                }
            }
        }
    }

    /// Every tick two machines played has the same hash on both; how many
    /// ticks each guest played.
    pub(crate) fn agree(&self) -> Vec<u64> {
        let host = self.nodes[0].game.as_ref().expect("the host plays");
        let mut played = Vec::new();
        for (n, node) in self.nodes.iter().enumerate().skip(1) {
            let Some(game) = &node.game else {
                played.push(0);
                continue;
            };
            for (i, &h) in game.after.iter().enumerate() {
                let tick = game.first + i as u64;
                assert_eq!(Some(h), host.at(tick), "machine {n} at tick {tick}");
            }
            played.push(game.after.len() as u64);
        }
        played
    }
}

pub(crate) fn node(role: Role, frame_us: u64) -> Node {
    Node { role, offset: 0, drift_ppm: 0, frame_us, next_frame: 0, owed: 0, frames: 0, game: None, join_at: 0, seat: true, taps: false, tapped: 0, pad: 0, last: None, off: false }
}

/// One frame of a machine: what came is taken in, its player's keys read,
/// the ticks due played, what is to be sent.
pub(crate) fn frame(node: &mut Node, addr: usize, now: u64, dt: u64) -> Vec<(usize, Vec<u8>)> {
    let keys = node.keys(addr);
    let pad = node.pad;
    match &mut node.role {
        Role::Host(h) => {
            let game = node.game.as_mut().expect("the host plays");
            for notice in h.notices() {
                if let Notice::Joining { index, .. } = notice {
                    h.admit(index, game.blob(pad), Vec::new());
                }
            }
            h.read(1, keys);
            node.owed += dt;
            let mut ticks = 0;
            while node.owed >= TICK_US && ticks < MAX_CATCH_UP {
                let tick = h.next_tick();
                let extra = if tick % 50 == 7 { vec![tick as u8; 3] } else { Vec::new() };
                game.play(&h.tick(extra).expect("a bundle fits"));
                node.owed -= TICK_US;
                ticks += 1;
            }
            if ticks == MAX_CATCH_UP {
                node.owed = 0;
            }
            h.outgoing(now)
        }
        Role::Guest(g) => {
            if now < node.join_at {
                return Vec::new();
            }
            if node.game.is_none()
                && let Some(blob) = g.take(1)
            {
                node.game = Some(Stub::from_blob(&blob, pad));
                if node.seat {
                    g.ask_seat();
                }
            }
            if let Some(game) = &mut node.game {
                // Caught up at once with what came meanwhile.
                while g.in_hand() > g.target() as usize + 1 {
                    let b = g.next_bundle().expect("in hand");
                    game.play(&b);
                }
                for (seat, _) in g.mine() {
                    g.read(seat, keys);
                    node.tapped += (keys.actions & 4 != 0) as u64;
                }
                for b in g.due(now) {
                    game.play(&b);
                }
            }
            g.outgoing(now)
        }
    }
}

/// The soak: `seeds` runs of up to eight machines, every other one on a
/// hard network (1 s, 100 ms of jitter, a quarter lost and a quarter
/// doubled: yojimbo's), the others on a bad one, 30 s each; every guest
/// must play the same ticks as the host. A digest of every hash the hosts
/// played, to compare between systems.
pub fn soak(seeds: u64) -> u64 {
    let hard = Link { delay_us: 1_000_000, jitter_us: 100_000, loss: 250, burst: 0, burst_len: 0, duplicate: 250, reorder: 50 };
    let mut digest = Stable::default();
    for seed in 0..seeds {
        let guests = 1 + seed as usize % 7;
        let mut run = Run::new(10_000 + seed, if seed % 2 == 0 { hard } else { Link::bad() }, guests);
        run.to(30_000_000);
        let played = run.agree();
        assert!(played.iter().all(|&p| p > 600), "seed {seed}: every guest plays on: {played:?} (the host {})", run.host().next_tick());
        for h in &run.nodes[0].game.as_ref().expect("the host plays").after {
            digest.u64(*h);
        }
    }
    digest.finish()
}
