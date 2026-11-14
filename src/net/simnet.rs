//! A network simulated in one process, for tests of network play without
//! a window (`docs/multiplayer.md`, stage 4, "Tests without a window"):
//! datagrams wait in a queue by when they arrive and in what order they
//! were sent; every link has the same delay, jitter, loss (also in
//! bursts), duplicates and reordering, and each may be cut. Its chances come from its
//! own seed (never the game's generator), time is whole microseconds, so a
//! test plays the same every time, on every system.

use std::collections::{BTreeMap, BTreeSet};

use super::wire::MAX_DATAGRAM;

/// How a link between two machines behaves. Chances are in thousandths.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Link {
    pub delay_us: u64,
    pub jitter_us: u64,
    pub loss: u32,
    /// A loss starts a burst this often, losing this many more after it.
    pub burst: u32,
    pub burst_len: u32,
    pub duplicate: u32,
    /// Held back by up to a few times the jitter.
    pub reorder: u32,
}

impl Link {
    /// A home network on a cable or good Wi-Fi: 1 to 5 ms, one in a
    /// hundred lost.
    pub fn lan() -> Self {
        Self { delay_us: 1_000, jitter_us: 4_000, loss: 10, ..Self::default() }
    }

    /// A bad one: 50 ms and 30 ms of jitter, a tenth lost (some in
    /// bursts), a tenth doubled, some out of order.
    pub fn bad() -> Self {
        Self { delay_us: 50_000, jitter_us: 30_000, loss: 100, burst: 20, burst_len: 8, duplicate: 100, reorder: 50 }
    }
}

/// The network between machines numbered from 0.
pub struct SimNet {
    seed: u64,
    now: u64,
    order: u64,
    queue: BTreeMap<(u64, u64), (usize, usize, Vec<u8>)>,
    /// How every link behaves.
    all: Link,
    cut: BTreeSet<(usize, usize)>,
    bursting: BTreeMap<(usize, usize), u32>,
    /// The biggest datagram sent, and how many were too big.
    pub largest: usize,
    pub too_big: u64,
    pub sent: u64,
}

impl SimNet {
    pub fn new(seed: u64, all: Link) -> Self {
        Self { seed, now: 0, order: 0, queue: BTreeMap::new(), all, cut: BTreeSet::new(), bursting: BTreeMap::new(), largest: 0, too_big: 0, sent: 0 }
    }

    /// splitmix64: a number from the network's own sequence.
    fn next(&mut self) -> u64 {
        self.seed = self.seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        crate::hash::mix(self.seed)
    }

    fn chance(&mut self, per_mille: u32) -> bool {
        per_mille > 0 && self.next() % 1000 < per_mille as u64
    }

    fn upto(&mut self, max: u64) -> u64 {
        if max == 0 { 0 } else { self.next() % (max + 1) }
    }

    /// Cuts the link both ways (or mends it): nothing gets through.
    pub fn cut(&mut self, a: usize, b: usize, cut: bool) {
        for pair in [(a, b), (b, a)] {
            if cut {
                self.cut.insert(pair);
            } else {
                self.cut.remove(&pair);
            }
        }
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    /// Time goes on to `now` (µs).
    pub fn advance(&mut self, now: u64) {
        self.now = self.now.max(now);
    }

    /// A datagram leaves `from` for `to` now.
    pub fn send(&mut self, from: usize, to: usize, bytes: Vec<u8>) {
        self.sent += 1;
        self.largest = self.largest.max(bytes.len());
        if bytes.len() > MAX_DATAGRAM {
            self.too_big += 1;
            return;
        }
        if self.cut.contains(&(from, to)) {
            return;
        }
        let link = self.all;
        if let Some(left) = self.bursting.get_mut(&(from, to)) {
            *left -= 1;
            if *left == 0 {
                self.bursting.remove(&(from, to));
            }
            return;
        }
        if self.chance(link.loss) {
            if self.chance(link.burst * 1000 / link.loss.max(1)) && link.burst_len > 0 {
                self.bursting.insert((from, to), link.burst_len);
            }
            return;
        }
        let copies = if self.chance(link.duplicate) { 2 } else { 1 };
        for _ in 0..copies {
            let mut delay = link.delay_us + self.upto(link.jitter_us);
            if self.chance(link.reorder) {
                delay += self.upto(link.jitter_us * 3 + 5_000);
            }
            self.order += 1;
            self.queue.insert((self.now + delay, self.order), (from, to, bytes.clone()));
        }
    }

    /// The datagrams arrived by now, in the order they arrive: (from, to,
    /// bytes).
    pub fn deliver(&mut self) -> Vec<(usize, usize, Vec<u8>)> {
        let later = self.queue.split_off(&(self.now + 1, 0));
        std::mem::replace(&mut self.queue, later).into_values().collect()
    }
}
