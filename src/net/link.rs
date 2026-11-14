//! How a network game's machines reach each other (`docs/multiplayer.md`
//! stage 6): on a LAN by the game's own UDP (stage 4, as it was), across
//! the internet by iroh (`quic`) with a ticket. Over the internet a
//! machine that connects only asks in: nothing it sends reaches the game
//! until the host lets it in, after both screens showed the same four
//! signs and the players read them out to each other (a ticket pasted in
//! a chat is a key anyone who sees it holds). Where our matchmaker is
//! known (`../matchmaker`), a host's game opens a room there: it goes by a
//! code of six signs a guest types instead of the ticket, and stands in the
//! list of open games (`network::NetFinder`) and for any game.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;

use anyhow::Result;

use super::quic::{Event, Quic, Reach};
use super::udp::Udp;
use super::wire;
use matchmaker_protocol as mm;

/// The biggest datagram a bundle's datagram fills over iroh: QUIC carries
/// 1,162 bytes until it has measured the path (`voxcade nettest`).
pub const NET_LIMIT: usize = 1100;

/// Where a machine is: an address on the LAN, or a connection across the
/// internet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Where {
    Lan(SocketAddr),
    Net(u32),
}

/// A machine asking in over the internet: the code both screens show, and
/// the name it gave (once its HELLO came).
#[derive(Clone, Debug, PartialEq)]
pub struct Knock {
    pub peer: u32,
    pub code: String,
    pub name: String,
}

/// The four signs both machines show before one is let in: from both
/// keys, so neither has to send it (no 0 and O, no 1 and I).
pub fn code(host: &[u8; 32], guest: &[u8; 32]) -> String {
    const SIGNS: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut both = [0u8; 64];
    both[..32].copy_from_slice(host);
    both[32..].copy_from_slice(guest);
    let hash = blake3::hash(&both);
    hash.as_bytes()[..4].iter().map(|b| SIGNS[(b % 32) as usize] as char).collect()
}

pub struct Link {
    how: How,
}

enum How {
    Lan(Udp),
    Net(Box<Net>),
}

struct Net {
    quic: Quic,
    hosting: bool,
    /// Machines connected and asking in (the host's), and those let in.
    knocking: BTreeMap<u32, Knock>,
    let_in: BTreeSet<u32>,
    /// A guest's: the host's key, and the code to say once connected.
    host_key: Option<[u8; 32]>,
    code: Option<String>,
    /// The host's: the code its room goes by. A guest's: asking the
    /// matchmaker for a room (by its code or any), the code of the room
    /// found, why none was.
    game_code: Option<String>,
    looking: bool,
    found_code: Option<String>,
    refused: Option<mm::Refusal>,
    trouble: Vec<String>,
}

impl Link {
    /// The host's on a LAN, on the first free port from 26500 or `port`.
    pub fn lan_host(port: Option<u16>) -> Result<(Self, u16)> {
        let (udp, port) = Udp::host(port)?;
        Ok((Self { how: How::Lan(udp) }, port))
    }

    pub fn lan_guest() -> Result<Self> {
        Ok(Self { how: How::Lan(Udp::guest()?) })
    }

    /// The host's across the internet (its ticket: `ticket`), its room
    /// opened at our matchmaker where it is known (its code, once given:
    /// `game_code`).
    pub fn net_host(reach: Reach, room: Option<mm::Room>) -> Result<Self> {
        let quic = Quic::start(reach)?;
        if let Some(room) = room.filter(|_| super::quic::matchmaker_known()) {
            quic.open(room);
        }
        Ok(Self { how: How::Net(Box::new(Net::new(quic, true))) })
    }

    /// A guest's across the internet, connecting to the host of a ticket,
    /// or of a code once the matchmaker found its room (`game`: the app
    /// and version played here, so the code of another is refused as
    /// such); where the host is.
    pub fn net_guest(text: &str, reach: Reach, game: Option<(String, String)>) -> Result<(Self, Where)> {
        let quic = Quic::start(reach)?;
        let mut net = Net::new(quic, false);
        match mm::tidy(text) {
            Some(code) => {
                let (app, version) = game.unzip();
                net.quic.join(mm::Join { code, app, version });
                net.looking = true;
            }
            None => net.host_key = Some(net.quic.connect(text)?),
        }
        Ok((Self { how: How::Net(Box::new(net)) }, Where::Net(0)))
    }

    /// A guest's across the internet, in any room of the search with a
    /// free seat (the matchmaker's quick join).
    pub fn net_guest_any(reach: Reach, search: mm::Search) -> Result<(Self, Where)> {
        let quic = Quic::start(reach)?;
        let mut net = Net::new(quic, false);
        net.quic.quick(search);
        net.looking = true;
        Ok((Self { how: How::Net(Box::new(net)) }, Where::Net(0)))
    }

    /// The host's room changes (seats taken).
    pub fn update_room(&self, update: mm::Update) {
        if let How::Net(n) = &self.how
            && n.hosting
            && n.game_code.is_some()
        {
            n.quic.update(update);
        }
    }

    /// The code of the room a guest joins, once the matchmaker found it.
    pub fn found_code(&self) -> Option<&str> {
        match &self.how {
            How::Net(n) => n.found_code.as_deref(),
            How::Lan(_) => None,
        }
    }

    /// Why the matchmaker found no room for a guest, once (said in the
    /// game's words).
    pub fn refused(&mut self) -> Option<mm::Refusal> {
        match &mut self.how {
            How::Net(n) => n.refused.take(),
            How::Lan(_) => None,
        }
    }

    /// Whether `text` is a ticket.
    pub fn is_ticket(text: &str) -> bool {
        text.trim().starts_with("vxc1")
    }

    /// Whether `text` is a game's code (six signs, as typed).
    pub fn is_code(text: &str) -> bool {
        mm::tidy(text).is_some()
    }

    /// Whether `text` names a game across the internet (a ticket or a
    /// code), else an address on the LAN.
    pub fn is_net(text: &str) -> bool {
        Self::is_ticket(text) || Self::is_code(text)
    }

    /// The biggest datagram the host fills.
    pub fn limit(&self) -> usize {
        match self.how {
            How::Lan(_) => wire::MAX_DATAGRAM,
            How::Net(_) => NET_LIMIT,
        }
    }

    /// The host's ticket across the internet.
    pub fn ticket(&self) -> Option<&str> {
        match &self.how {
            How::Net(n) if n.hosting => Some(n.quic.ticket()),
            _ => None,
        }
    }

    /// The code the host's game goes by, once our server gave it.
    pub fn game_code(&self) -> Option<&str> {
        match &self.how {
            How::Net(n) if n.hosting => n.game_code.as_deref(),
            _ => None,
        }
    }

    /// The machines asking in (the host's).
    pub fn knocking(&self) -> Vec<Knock> {
        match &self.how {
            How::Net(n) => n.knocking.values().cloned().collect(),
            How::Lan(_) => Vec::new(),
        }
    }

    /// Lets a machine asking in into the game.
    pub fn let_in(&mut self, peer: u32) {
        if let How::Net(n) = &mut self.how
            && n.knocking.remove(&peer).is_some()
        {
            n.let_in.insert(peer);
        }
    }

    /// Ends the connection with a machine refused or gone (across the
    /// internet; a LAN has nothing to close).
    pub fn close(&mut self, at: Where) {
        if let (How::Net(n), Where::Net(peer)) = (&mut self.how, at) {
            n.quic.close(peer);
            n.knocking.remove(&peer);
            n.let_in.remove(&peer);
        }
    }

    /// Lets every machine asking in into the game (a host without a
    /// window: `inspect`).
    pub fn let_all_in(&mut self) -> Vec<Knock> {
        let knocks = self.knocking();
        for k in &knocks {
            self.let_in(k.peer);
        }
        knocks
    }

    /// The code a guest says to the host's player once connected.
    pub fn code(&self) -> Option<&str> {
        match &self.how {
            How::Net(n) => n.code.as_deref(),
            How::Lan(_) => None,
        }
    }

    /// What went wrong apart from any machine, since asked last.
    pub fn trouble(&mut self) -> Vec<String> {
        match &mut self.how {
            How::Net(n) => std::mem::take(&mut n.trouble),
            How::Lan(_) => Vec::new(),
        }
    }

    pub fn send(&self, to: Where, bytes: Vec<u8>) {
        match (&self.how, to) {
            (How::Lan(udp), Where::Lan(addr)) => udp.send(addr, &bytes),
            (How::Net(n), Where::Net(peer)) if !n.hosting || n.let_in.contains(&peer) => n.quic.send(peer, bytes),
            _ => {}
        }
    }

    /// The datagrams that came: over the internet only from machines let
    /// in (the host's) or from the host (a guest's).
    pub fn drain(&mut self) -> Vec<(Where, Vec<u8>)> {
        match &mut self.how {
            How::Lan(udp) => udp.drain().into_iter().map(|(from, bytes)| (Where::Lan(from), bytes)).collect(),
            How::Net(n) => n.drain(),
        }
    }
}

impl Net {
    fn new(quic: Quic, hosting: bool) -> Self {
        Self { quic, hosting, knocking: BTreeMap::new(), let_in: BTreeSet::new(), host_key: None, code: None, game_code: None, looking: false, found_code: None, refused: None, trouble: Vec::new() }
    }

    fn drain(&mut self) -> Vec<(Where, Vec<u8>)> {
        let mut came = Vec::new();
        for event in self.quic.drain() {
            match event {
                Event::Joined { peer, key } if self.hosting => {
                    let code = code(&self.quic.key(), &key);
                    eprintln!("a machine asks in over the internet, saying {code}");
                    self.knocking.insert(peer, Knock { peer, code, name: String::new() });
                }
                Event::Joined { key, .. } => {
                    if Some(key) == self.host_key {
                        let code = code(&key, &self.quic.key());
                        eprintln!("connected to the host, say {code}");
                        self.code = Some(code);
                    }
                }
                Event::Datagram { peer, bytes, .. } => {
                    if !self.hosting || self.let_in.contains(&peer) {
                        came.push((Where::Net(peer), bytes));
                    } else if let Some(knock) = self.knocking.get_mut(&peer)
                        && knock.name.is_empty()
                        && let Ok((wire::Kind::Hello, _, mut r)) = wire::open(&bytes)
                        && let Ok(hello) = wire::Hello::read(&mut r)
                    {
                        knock.name = hello.name;
                    }
                }
                Event::Gone { peer, why } => {
                    self.knocking.remove(&peer);
                    if !self.hosting {
                        self.trouble.push(format!("the connection to the host ended: {why}"));
                    }
                }
                Event::Path { peer, relay, rtt, .. } => eprintln!("machine {peer}: {} ({} ms)", if relay { "through the relay" } else { "directly" }, rtt.as_millis()),
                Event::Code(code) => {
                    eprintln!("the game's code: {code}");
                    self.game_code = Some(code);
                }
                Event::Found(found) if self.looking => {
                    self.looking = false;
                    eprintln!("the matchmaker found {} ({})", found.code, found.title);
                    self.found_code = Some(found.code);
                    match self.quic.connect(&found.ticket) {
                        Ok(key) => self.host_key = Some(key),
                        Err(e) => self.trouble.push(format!("{e:#}")),
                    }
                }
                Event::Refused(refusal) if self.looking => {
                    self.looking = false;
                    self.refused = Some(refusal);
                    self.trouble.push(refusal.text().to_string());
                }
                Event::Found(_) | Event::Refused(_) | Event::Rooms(_) => {}
                Event::Trouble(t) => self.trouble.push(t),
            }
        }
        came
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both machines work out the same code from the two keys, and another
    /// pair of keys another.
    #[test]
    fn both_screens_show_the_same_code() {
        let (host, guest, other) = ([1u8; 32], [2u8; 32], [3u8; 32]);
        let c = code(&host, &guest);
        assert_eq!(c.len(), 4);
        assert!(c.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit()) && !c.contains(['0', 'O', '1', 'I']));
        assert_eq!(c, code(&host, &guest));
        assert_ne!(c, code(&host, &other));
        assert_ne!(c, code(&guest, &host), "who hosts counts");
    }

    /// A LAN link sends to an address and drains what came, saying from
    /// where; a datagram for a machine across the internet goes nowhere
    /// on it; it has no ticket, code or trouble.
    #[test]
    fn a_lan_link_carries_datagrams_between_addresses() {
        let (a, b) = (Udp::guest().unwrap(), Udp::guest().unwrap());
        let (a_at, b_at) = (a.here(), b.here());
        let (sender, mut receiver) = (Link { how: How::Lan(a) }, Link { how: How::Lan(b) });
        sender.send(Where::Net(3), b"lost".to_vec());
        sender.send(Where::Lan(b_at), b"hello".to_vec());
        let started = std::time::Instant::now();
        let mut came = Vec::new();
        while came.is_empty() && started.elapsed() < std::time::Duration::from_secs(5) {
            came = receiver.drain();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(came, [(Where::Lan(a_at), b"hello".to_vec())]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(receiver.drain().is_empty(), "nothing else came");
        assert!(sender.ticket().is_none() && sender.code().is_none() && sender.game_code().is_none());
        assert!(receiver.trouble().is_empty() && receiver.knocking().is_empty());
        assert_eq!(sender.limit(), crate::net::MAX_DATAGRAM);
    }
}
