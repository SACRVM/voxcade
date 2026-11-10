//! The bytes of the protocol (`docs/multiplayer.md`, stage 4, "What
//! travels"). Every datagram begins with `VXC`, the protocol's version, its
//! kind and the session's key (0 for HELLO, before there is one); numbers
//! are LEB128 varints, lists a count first. Reading trusts no count: each
//! is checked against its limit and what is left before anything is kept,
//! and a broken datagram is dropped whole, never a panic.

use crate::input::Input;

/// The most bytes a datagram may have; more is dropped as it is read. Kept
/// under the Internet's smallest MTU less its headers.
pub const MAX_DATAGRAM: usize = 1200;
/// The protocol's version: machines that speak another do not play.
pub const VERSION: u8 = 1;
const MAGIC: &[u8; 3] = b"VXC";
/// Bytes before a message's body: `VXC`, version, kind, key.
pub const HEADER: usize = 3 + 1 + 1 + 8;
/// The most seats a game has (`c.players`).
pub const MAX_SEATS: usize = 8;
/// The most machines in a network game.
pub const MAX_MACHINES: usize = 16;
/// A bundle's events (seats taken and left) and the session's bytes, at
/// most: one bundle always fits a datagram.
const MAX_SEAT_EVENTS: usize = 24;
pub const MAX_EXTRA: usize = 900;
const MAX_CONTROLS: usize = 256;
/// The most readings of one seat a guest sends at once (the newest).
pub const MAX_READINGS: usize = 32;
const MAX_ASKS: usize = 16;
/// Hash checks a guest reports at once, and the parts of each.
pub const MAX_CHECKS: usize = 4;
const MAX_PARTS: usize = 16;
const MAX_NAME: usize = 32;
const MAX_TEXT: usize = 200;
/// A piece's bytes, and the most pieces a thing sent in pieces has (the
/// run so far for a guest coming late: 1100 × 60,000, 66 MB).
pub const PIECE: usize = 1100;
pub const MAX_PIECES: u32 = 60_000;

/// A datagram that does not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Broken;

type Read<T> = Result<T, Broken>;

/// The kinds of datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Hello = 1,
    Welcome = 2,
    Refused = 3,
    Up = 4,
    Down = 5,
    Piece = 6,
    PieceAck = 7,
    Bye = 8,
    /// A search for games on the LAN, broadcast (key 0), and a host's
    /// answer.
    Find = 9,
    Here = 10,
}

impl Kind {
    fn of(n: u8) -> Read<Self> {
        Ok(match n {
            1 => Self::Hello,
            2 => Self::Welcome,
            3 => Self::Refused,
            4 => Self::Up,
            5 => Self::Down,
            6 => Self::Piece,
            7 => Self::PieceAck,
            8 => Self::Bye,
            9 => Self::Find,
            10 => Self::Here,
            _ => return Err(Broken),
        })
    }
}

pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new(kind: Kind, key: u64) -> Self {
        let mut buf = Vec::with_capacity(MAX_DATAGRAM);
        buf.extend_from_slice(MAGIC);
        buf.push(VERSION);
        buf.push(kind as u8);
        buf.extend_from_slice(&key.to_le_bytes());
        Self { buf }
    }

    /// Bytes without a datagram's head: the session's, inside one.
    pub fn bare() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn varint(&mut self, mut v: u64) -> &mut Self {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(byte);
                return self;
            }
            self.buf.push(byte | 0x80);
        }
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.varint(b.len() as u64);
        self.buf.extend_from_slice(b);
        self
    }

    pub fn str(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes())
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// Reads bytes without a datagram's head (`Writer::bare`).
    pub fn bare(buf: &'a [u8]) -> Self {
        Self { buf, at: 0 }
    }

    pub fn u8(&mut self) -> Read<u8> {
        let v = *self.buf.get(self.at).ok_or(Broken)?;
        self.at += 1;
        Ok(v)
    }

    pub fn u64(&mut self) -> Read<u64> {
        let b = self.buf.get(self.at..self.at + 8).ok_or(Broken)?;
        self.at += 8;
        Ok(u64::from_le_bytes(b.try_into().map_err(|_| Broken)?))
    }

    pub fn varint(&mut self) -> Read<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            v |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(Broken)
    }

    pub fn u32(&mut self) -> Read<u32> {
        u32::try_from(self.varint()?).map_err(|_| Broken)
    }

    /// A count no larger than `max`.
    pub fn count(&mut self, max: usize) -> Read<usize> {
        let n = self.varint()?;
        if n > max as u64 {
            return Err(Broken);
        }
        Ok(n as usize)
    }

    pub fn bytes(&mut self, max: usize) -> Read<&'a [u8]> {
        let n = self.count(max)?;
        let b = self.buf.get(self.at..self.at + n).ok_or(Broken)?;
        self.at += n;
        Ok(b)
    }

    pub fn str(&mut self, max: usize) -> Read<String> {
        String::from_utf8(self.bytes(max)?.to_vec()).map_err(|_| Broken)
    }

    /// Whether every byte was read.
    pub fn done(&self) -> Read<()> {
        if self.at == self.buf.len() { Ok(()) } else { Err(Broken) }
    }
}

/// A datagram's kind and key, and a reader of its body; anything not
/// ours, too big or of another version is broken (`Refused` alone may come
/// from another version: what it says is for people).
pub fn open(datagram: &[u8]) -> Read<(Kind, u64, Reader<'_>)> {
    if datagram.len() > MAX_DATAGRAM || datagram.len() < HEADER || &datagram[..3] != MAGIC {
        return Err(Broken);
    }
    let kind = Kind::of(datagram[4])?;
    if datagram[3] != VERSION && kind != Kind::Hello && kind != Kind::Refused {
        return Err(Broken);
    }
    let key = u64::from_le_bytes(datagram[5..HEADER].try_into().map_err(|_| Broken)?);
    Ok((kind, key, Reader { buf: datagram, at: HEADER }))
}

/// The version a datagram says it speaks (a HELLO from another program).
pub fn version_of(datagram: &[u8]) -> Option<u8> {
    datagram.get(3).copied()
}

fn input(w: &mut Writer, i: Input) {
    w.varint(i.left as u64 | (i.right as u64) << 1 | (i.up as u64) << 2 | (i.down as u64) << 3 | (i.actions as u64) << 4);
}

fn read_input(r: &mut Reader) -> Read<Input> {
    let v = r.varint()?;
    let actions = u32::try_from(v >> 4).map_err(|_| Broken)?;
    Ok(Input { left: v & 1 != 0, right: v & 2 != 0, up: v & 4 != 0, down: v & 8 != 0, actions })
}

/// A guest's first word: who it is (`MachineId`), this joining's own
/// random number (a HELLO late on the network is not taken for the
/// machine starting anew), a name and the engine's commit for people.
#[derive(Clone, Debug, PartialEq)]
pub struct Hello {
    pub id: super::MachineId,
    pub nonce: u64,
    pub name: String,
    pub commit: String,
}

impl Hello {
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new(Kind::Hello, 0);
        w.buf.extend_from_slice(&self.id);
        w.u64(self.nonce).str(&self.name).str(&self.commit);
        w.buf
    }

    pub fn read(r: &mut Reader) -> Read<Self> {
        let mut id = [0u8; 16];
        for b in &mut id {
            *b = r.u8()?;
        }
        let hello = Self { id, nonce: r.u64()?, name: r.str(MAX_NAME)?, commit: r.str(MAX_NAME)? };
        r.done()?;
        Ok(hello)
    }
}

/// The host's answer to a HELLO it takes: the guest's number, the first
/// tick it gets bundles of, the game's seats, the run so far sent in
/// pieces (its size), the host's cost of a tick, and the session's own
/// bytes (how the run began, what is compared).
#[derive(Clone, Debug, PartialEq)]
pub struct Welcome {
    pub index: super::Machine,
    pub first: u64,
    pub seats: u8,
    pub blob: u32,
    pub us_per_tick: u32,
    pub info: Vec<u8>,
}

impl Welcome {
    pub fn write(&self, key: u64) -> Vec<u8> {
        let mut w = Writer::new(Kind::Welcome, key);
        w.u8(self.index).varint(self.first).u8(self.seats).varint(self.blob as u64).varint(self.us_per_tick as u64).bytes(&self.info);
        w.buf
    }

    pub fn read(r: &mut Reader) -> Read<Self> {
        let welcome = Self { index: r.u8()?, first: r.varint()?, seats: r.u8()?, blob: r.u32()?, us_per_tick: r.u32()?, info: r.bytes(MAX_EXTRA)?.to_vec() };
        if welcome.seats as usize > MAX_SEATS || welcome.seats == 0 || welcome.blob > MAX_PIECES * PIECE as u32 {
            return Err(Broken);
        }
        r.done()?;
        Ok(welcome)
    }
}

/// A search for games on the LAN (any game: those of another are shown
/// greyed).
pub fn find() -> Vec<u8> {
    Writer::new(Kind::Find, 0).buf
}

/// A host's answer to a search: its game (the folder's name and its
/// title), the game's fingerprint (another game is greyed), the build,
/// the machine's name, its seats and how many are taken, its port.
#[derive(Clone, Debug, PartialEq)]
pub struct Here {
    pub game: String,
    pub title: String,
    pub fingerprint: u64,
    pub commit: String,
    pub name: String,
    pub seats: u8,
    pub taken: u8,
    pub port: u16,
}

impl Here {
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new(Kind::Here, 0);
        w.str(&self.game).str(&self.title).u64(self.fingerprint).str(&self.commit).str(&self.name).u8(self.seats).u8(self.taken).varint(self.port as u64);
        w.buf
    }

    pub fn read(r: &mut Reader) -> Read<Self> {
        let here = Self { game: r.str(MAX_NAME * 2)?, title: r.str(MAX_NAME * 2)?, fingerprint: r.u64()?, commit: r.str(MAX_NAME)?, name: r.str(MAX_NAME)?, seats: r.u8()?, taken: r.u8()?, port: u16::try_from(r.varint()?).map_err(|_| Broken)? };
        r.done()?;
        Ok(here)
    }
}

/// Why a HELLO was not taken, for people.
pub fn refused(reason: &str) -> Vec<u8> {
    let mut w = Writer::new(Kind::Refused, 0);
    w.str(&reason[..reason.len().min(MAX_TEXT)]);
    w.buf
}

pub fn read_refused(r: &mut Reader) -> Read<String> {
    r.str(MAX_TEXT)
}

pub fn bye(key: u64) -> Vec<u8> {
    Writer::new(Kind::Bye, key).buf
}

/// What a guest asks of the host, stamped into a bundle when the host
/// builds the next.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// A seat for one more player at this machine.
    Take,
    Leave(u8),
    /// What the player on a seat plays with now (the session's bytes).
    Controls(u8, Vec<u8>),
}

/// A seat's readings: its number, its generation, the first reading's
/// number and the readings after it.
#[derive(Clone, Debug, PartialEq)]
pub struct Readings {
    pub seat: u8,
    pub generation: u32,
    pub first: u32,
    pub inputs: Vec<Input>,
}

/// A guest's datagram: the host's stamp it heard last and how long it
/// held it (the round trip), the bundles it has (all before `got`, and
/// which of the 64 after), its hand, its seats' readings not yet
/// acknowledged, its asks from `asks_from` on, and its state hash by parts
/// at ticks the host compares (every 10th).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Up {
    pub echo: u32,
    pub held: u16,
    pub got: u64,
    pub mask: u64,
    pub hand: u8,
    pub readings: Vec<Readings>,
    pub asks_from: u32,
    pub asks: Vec<Ask>,
    pub checks: Vec<(u64, Vec<u64>)>,
}

impl Up {
    pub fn write(&self, key: u64) -> Vec<u8> {
        let mut w = Writer::new(Kind::Up, key);
        w.varint(self.echo as u64).varint(self.held as u64).varint(self.got).u64(self.mask).u8(self.hand);
        w.varint(self.readings.len() as u64);
        for r in &self.readings {
            w.u8(r.seat).varint(r.generation as u64).varint(r.first as u64).varint(r.inputs.len() as u64);
            for &i in &r.inputs {
                input(&mut w, i);
            }
        }
        w.varint(self.asks_from as u64).varint(self.asks.len() as u64);
        for ask in &self.asks {
            match ask {
                Ask::Take => {
                    w.u8(1);
                }
                Ask::Leave(seat) => {
                    w.u8(2).u8(*seat);
                }
                Ask::Controls(seat, bytes) => {
                    w.u8(3).u8(*seat).bytes(bytes);
                }
            }
        }
        w.varint(self.checks.len() as u64);
        for (tick, parts) in &self.checks {
            w.varint(*tick).varint(parts.len() as u64);
            for &p in parts {
                w.u64(p);
            }
        }
        w.buf
    }

    pub fn read(r: &mut Reader) -> Read<Self> {
        let mut up = Self { echo: r.u32()?, held: u16::try_from(r.varint()?).map_err(|_| Broken)?, got: r.varint()?, mask: r.u64()?, hand: r.u8()?, ..Self::default() };
        for _ in 0..r.count(MAX_SEATS)? {
            let (seat, generation, first) = (r.u8()?, r.u32()?, r.u32()?);
            let n = r.count(MAX_READINGS)?;
            let inputs = (0..n).map(|_| read_input(r)).collect::<Read<Vec<_>>>()?;
            up.readings.push(Readings { seat, generation, first, inputs });
        }
        up.asks_from = r.u32()?;
        for _ in 0..r.count(MAX_ASKS)? {
            up.asks.push(match r.u8()? {
                1 => Ask::Take,
                2 => Ask::Leave(r.u8()?),
                3 => Ask::Controls(r.u8()?, r.bytes(MAX_CONTROLS)?.to_vec()),
                _ => return Err(Broken),
            });
        }
        for _ in 0..r.count(MAX_CHECKS)? {
            let tick = r.varint()?;
            let parts = (0..r.count(MAX_PARTS)?).map(|_| r.u64()).collect::<Read<Vec<_>>>()?;
            up.checks.push((tick, parts));
        }
        r.done()?;
        Ok(up)
    }
}

/// A change of seats, stamped into a bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeatEvent {
    /// Taken by a machine's player (its ask), the seat's new generation.
    Taken { seat: u8, owner: super::Machine, generation: u32, ask: u32 },
    Left { seat: u8 },
    /// No seat was free for that ask.
    Refused { owner: super::Machine, ask: u32 },
}

/// One tick for every machine: what each seat holds (None: nobody sits
/// there), the seats taken and left before it, what players' controls
/// changed, and the session's own bytes (its events).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bundle {
    pub tick: u64,
    pub inputs: Vec<Option<Input>>,
    pub seats: Vec<SeatEvent>,
    pub controls: Vec<(u8, Vec<u8>)>,
    pub extra: Vec<u8>,
}

impl Bundle {
    pub fn write(&self, w: &mut Writer) {
        w.varint(self.tick).u8(self.inputs.len() as u8);
        let present = self.inputs.iter().enumerate().filter(|(_, i)| i.is_some()).fold(0u8, |m, (n, _)| m | 1 << n);
        w.u8(present);
        for i in self.inputs.iter().flatten() {
            input(w, *i);
        }
        w.u8(self.seats.len() as u8);
        for e in &self.seats {
            match *e {
                SeatEvent::Taken { seat, owner, generation, ask } => {
                    w.u8(1).u8(seat).u8(owner).varint(generation as u64).varint(ask as u64);
                }
                SeatEvent::Left { seat } => {
                    w.u8(2).u8(seat);
                }
                SeatEvent::Refused { owner, ask } => {
                    w.u8(3).u8(owner).varint(ask as u64);
                }
            }
        }
        w.u8(self.controls.len() as u8);
        for (seat, bytes) in &self.controls {
            w.u8(*seat).bytes(bytes);
        }
        w.bytes(&self.extra);
    }

    pub fn read(r: &mut Reader) -> Read<Self> {
        let tick = r.varint()?;
        let n = r.u8()? as usize;
        if n == 0 || n > MAX_SEATS {
            return Err(Broken);
        }
        let present = r.u8()?;
        let mut inputs = Vec::with_capacity(n);
        for seat in 0..n {
            inputs.push(if present >> seat & 1 == 1 { Some(read_input(r)?) } else { None });
        }
        if present >> n != 0 {
            return Err(Broken);
        }
        let mut seats = Vec::new();
        let n = r.u8()? as usize;
        if n > MAX_SEAT_EVENTS {
            return Err(Broken);
        }
        for _ in 0..n {
            seats.push(match r.u8()? {
                1 => SeatEvent::Taken { seat: r.u8()?, owner: r.u8()?, generation: r.u32()?, ask: r.u32()? },
                2 => SeatEvent::Left { seat: r.u8()? },
                3 => SeatEvent::Refused { owner: r.u8()?, ask: r.u32()? },
                _ => return Err(Broken),
            });
        }
        let mut controls = Vec::new();
        let n = r.u8()? as usize;
        if n > MAX_SEATS {
            return Err(Broken);
        }
        for _ in 0..n {
            controls.push((r.u8()?, r.bytes(MAX_CONTROLS)?.to_vec()));
        }
        let extra = r.bytes(MAX_EXTRA)?.to_vec();
        Ok(Self { tick, inputs, seats, controls, extra })
    }

    /// Its bytes on the wire.
    pub fn size(&self) -> usize {
        let mut w = Writer { buf: Vec::new() };
        self.write(&mut w);
        w.buf.len()
    }
}

/// Who plays in a network game, for people: a machine's number, name,
/// round trip and whether it went quiet.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub index: super::Machine,
    pub name: String,
    pub rtt_ms: u16,
    pub quiet: bool,
}

/// Where a guest's game ran apart from the host's: the tick checked, the
/// first part of the state hash that differs, and whether what was
/// played differs (the network's fault) or only the state (the game's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Apart {
    pub tick: u64,
    pub part: String,
    pub inputs: bool,
}

/// The host's datagram to a guest: its clock's stamp (ms), the asks it
/// took of that guest (all before `asks_done`), the newest reading it
/// has of each of the guest's seats, who paused, who plays (now and
/// then), where its game ran apart, a line for every screen, and bundles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Down {
    pub stamp: u32,
    pub asks_done: u32,
    pub acked: Vec<(u8, u32)>,
    pub paused: Option<super::Machine>,
    pub roster: Option<Vec<Member>>,
    pub apart: Option<Apart>,
    pub said: Option<String>,
    pub bundles: Vec<Bundle>,
}

impl Down {
    /// Everything but the bundles; `bundle` adds them while they fit.
    pub fn start(&self, key: u64) -> Writer {
        let mut w = Writer::new(Kind::Down, key);
        w.varint(self.stamp as u64).varint(self.asks_done as u64).varint(self.acked.len() as u64);
        for &(seat, seq) in &self.acked {
            w.u8(seat).varint(seq as u64);
        }
        w.u8(self.paused.map_or(0, |m| m + 1));
        match &self.roster {
            None => {
                w.u8(0);
            }
            Some(list) => {
                w.u8(1).varint(list.len() as u64);
                for m in list {
                    w.u8(m.index).str(&m.name).varint(m.rtt_ms as u64).u8(m.quiet as u8);
                }
            }
        }
        match &self.apart {
            None => w.u8(0),
            Some(a) => w.u8(1).varint(a.tick).str(&a.part).u8(a.inputs as u8),
        };
        match &self.said {
            None => w.u8(0),
            Some(line) => w.u8(1).str(&line[..line.len().min(MAX_TEXT)]),
        };
        w
    }

    pub fn read(r: &mut Reader) -> Read<Self> {
        let mut down = Self { stamp: r.u32()?, asks_done: r.u32()?, ..Self::default() };
        for _ in 0..r.count(MAX_SEATS)? {
            down.acked.push((r.u8()?, r.u32()?));
        }
        down.paused = r.u8()?.checked_sub(1);
        if r.u8()? == 1 {
            let mut list = Vec::new();
            for _ in 0..r.count(MAX_MACHINES)? {
                list.push(Member { index: r.u8()?, name: r.str(MAX_NAME)?, rtt_ms: u16::try_from(r.varint()?).map_err(|_| Broken)?, quiet: r.u8()? != 0 });
            }
            down.roster = Some(list);
        }
        if r.u8()? == 1 {
            down.apart = Some(Apart { tick: r.varint()?, part: r.str(MAX_NAME)?, inputs: r.u8()? != 0 });
        }
        if r.u8()? == 1 {
            down.said = Some(r.str(MAX_TEXT)?);
        }
        let n = r.count(MAX_DATAGRAM)?;
        for _ in 0..n {
            down.bundles.push(Bundle::read(r)?);
        }
        r.done()?;
        Ok(down)
    }
}

/// Writes a DOWN with as many of `bundles` (in order) as fit in `limit`
/// bytes (at most `MAX_DATAGRAM`); how many.
pub fn down(head: &Down, key: u64, bundles: &[&[u8]], limit: usize) -> (Vec<u8>, usize) {
    let mut w = head.start(key);
    // The count is a varint below 128: a byte.
    let at = w.len();
    w.u8(0);
    let mut n = 0;
    for b in bundles {
        if w.len() + b.len() > limit.min(MAX_DATAGRAM) || n == 127 {
            break;
        }
        w.buf.extend_from_slice(b);
        n += 1;
    }
    w.buf[at] = n as u8;
    (w.buf, n)
}

/// A piece of something too big for a datagram (the run so far, a save):
/// which thing, which piece of how many, its bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub blob: u32,
    pub index: u32,
    pub count: u32,
    pub data: Vec<u8>,
}

impl Piece {
    pub fn write(&self, key: u64) -> Vec<u8> {
        let mut w = Writer::new(Kind::Piece, key);
        w.varint(self.blob as u64).varint(self.index as u64).varint(self.count as u64).bytes(&self.data);
        w.buf
    }

    pub fn read(r: &mut Reader) -> Read<Self> {
        let piece = Self { blob: r.u32()?, index: r.u32()?, count: r.u32()?, data: r.bytes(PIECE)?.to_vec() };
        if piece.count == 0 || piece.count > MAX_PIECES || piece.index >= piece.count {
            return Err(Broken);
        }
        r.done()?;
        Ok(piece)
    }
}

/// The pieces of a thing a guest has: all before `got`, and which of the
/// 64 after.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PieceAck {
    pub blob: u32,
    pub got: u32,
    pub mask: u64,
}

impl PieceAck {
    pub fn write(&self, key: u64) -> Vec<u8> {
        let mut w = Writer::new(Kind::PieceAck, key);
        w.varint(self.blob as u64).varint(self.got as u64).u64(self.mask);
        w.buf
    }

    pub fn read(r: &mut Reader) -> Read<Self> {
        let ack = Self { blob: r.u32()?, got: r.u32()?, mask: r.u64()? };
        r.done()?;
        Ok(ack)
    }
}

/// A bundle written alone, as the host keeps it and DOWN carries it.
pub fn bundle_bytes(b: &Bundle) -> Vec<u8> {
    let mut w = Writer { buf: Vec::new() };
    b.write(&mut w);
    w.buf
}
