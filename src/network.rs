//! A network game in the window (`docs/multiplayer.md`, stages 4 and 6):
//! how the machines reach each other (a LAN, or the internet with a
//! ticket, a code or a list from our matchmaker), the host's side or a
//! guest's, this machine's players and the seats they sit on, what the
//! screen says about the network (who went quiet, who paused, how far
//! joining is, who asks in) and what the pause page shows (where the game
//! is found, who plays).

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use anyhow::Result;
use matchmaker_protocol as mm;

use crate::game::Game;
use crate::menu::NetView;
use crate::net::link::{Link, Where};
use crate::net::play::{Hosting, Joined, Joining, Locals};
use crate::net::udp::{self, Udp};
use crate::net::{Machine, Notice};
use crate::session::Session;

/// A machine quiet this long more (silent 5 s in all) may be dropped from
/// the host's pause page.
const DROP_AFTER: Duration = Duration::from_secs(4);
/// A line about the network stays this long.
const SAID_FOR: Duration = Duration::from_secs(5);
/// What a guest asking for any game asks (its words in `lang`).
const ANY_GAME: &str = "ANY GAME";

pub enum Side<G: Game> {
    Host(Box<Hosting<G, Where>>),
    Guest(Box<Joining<G, Where>>),
}

pub struct Network<G: Game> {
    pub side: Side<G>,
    link: Link,
    start: Instant,
    /// Where the game is found (the host's addresses with its port); the
    /// host joined (as typed).
    found: Vec<String>,
    address: String,
    /// This machine's players and their seats; a guest's first player
    /// watching (LEAVE on its menu).
    pub locals: Locals,
    pub watching: bool,
    /// Bundles a guest took but could not play yet (a level change's fade
    /// came between).
    pub held: std::collections::VecDeque<crate::net::Bundle>,
    /// Lines said for a while, and since when each machine is quiet.
    said: Vec<(String, Instant)>,
    quiet: BTreeMap<Machine, (String, Instant)>,
    /// Why a guest across the internet got nowhere (no game with its
    /// code, no answer from our server).
    failed: Option<String>,
    /// The seats taken and seats the host's room at our matchmaker shows.
    room: Option<(u8, u8)>,
}

impl<G: Game> Network<G> {
    /// Hosts `game` (a run about to begin) on the first free port from
    /// 26500, or `port`.
    pub fn host(game: &Session<G>, port: Option<u16>) -> Result<Self> {
        let (link, port) = Link::lan_host(port)?;
        let mut hosting = Hosting::new(game, udp::random(), &udp::machine_name())?;
        hosting.reports_to = Some(crate::net::play::reports());
        // What a search for games on the LAN finds.
        hosting.host.set_here(crate::net::wire::Here {
            game: crate::settings::game_name(&game.dir),
            title: game.cfg.title.clone(),
            fingerprint: game.fingerprint()?,
            commit: udp::COMMIT.to_string(),
            name: udp::machine_name(),
            seats: 0,
            taken: 0,
            port,
        });
        let players = game.cfg.players.max(1) as usize;
        let mut found: Vec<String> = udp::addresses().iter().map(|ip| format!("{ip}:{port}")).collect();
        if found.is_empty() {
            found.push(format!("PORT {port}"));
        }
        Ok(Self { side: Side::Host(Box::new(hosting)), link, start: Instant::now(), found, address: String::new(), locals: Locals::new(players, true), watching: false, held: Default::default(), said: Vec::new(), quiet: BTreeMap::new(), failed: None, room: None })
    }

    /// Hosts `game` across the internet: who has the ticket may ask in,
    /// and the host's player lets each in (`let_in`). Where our
    /// matchmaker is known the game opens a room there: its code, and a
    /// line in the list of open games.
    pub fn host_net(game: &Session<G>) -> Result<Self> {
        let link = Link::net_host(crate::net::quic::reach(), Some(room_of(game)?))?;
        let mut hosting = Hosting::new(game, udp::random(), &udp::machine_name())?;
        hosting.reports_to = Some(crate::net::play::reports());
        hosting.host.set_limit(link.limit());
        let players = game.cfg.players.max(1) as usize;
        println!("hosting across the internet; the ticket:\n{}", link.ticket().unwrap_or_default());
        Ok(Self { side: Side::Host(Box::new(hosting)), link, start: Instant::now(), found: Vec::new(), address: String::new(), locals: Locals::new(players, true), watching: false, held: Default::default(), said: Vec::new(), quiet: BTreeMap::new(), failed: None, room: None })
    }

    /// Joins the game hosted at `address` (`host[:port]` on the LAN), or
    /// across the internet by its ticket or its code (`game`: the app and
    /// version played here, `app_of`, so a code of another is refused as
    /// such).
    pub fn join(dir: &std::path::Path, address: &str, players: u32, game: Option<(String, String)>) -> Result<Self> {
        let (link, host) = if Link::is_net(address) {
            Link::net_guest(address, crate::net::quic::reach(), game)?
        } else {
            (Link::lan_guest()?, Where::Lan(udp::address(address)?))
        };
        let guest = crate::net::Guest::new(host, udp::program_id(), udp::random(), &udp::machine_name(), udp::COMMIT, 0);
        let mut joining = Joining::new(dir, guest);
        joining.reports_to = Some(crate::net::play::reports());
        let shown = match matchmaker_protocol::tidy(address) {
            Some(code) => code,
            None if Link::is_ticket(address) => "THE TICKET'S HOST".to_string(),
            None => address.to_string(),
        };
        Ok(Self { side: Side::Guest(Box::new(joining)), link, start: Instant::now(), found: Vec::new(), address: shown, locals: Locals::new(players.max(1) as usize, false), watching: false, held: Default::default(), said: Vec::new(), quiet: BTreeMap::new(), failed: None, room: None })
    }

    /// Joins any game of `app` and `version` across the internet with a
    /// free seat (our matchmaker's quick join).
    pub fn join_any(dir: &std::path::Path, players: u32, app: String, version: String) -> Result<Self> {
        let (link, host) = Link::net_guest_any(crate::net::quic::reach(), mm::Search { app, version, ..mm::Search::default() })?;
        let guest = crate::net::Guest::new(host, udp::program_id(), udp::random(), &udp::machine_name(), udp::COMMIT, 0);
        let mut joining = Joining::new(dir, guest);
        joining.reports_to = Some(crate::net::play::reports());
        Ok(Self { side: Side::Guest(Box::new(joining)), link, start: Instant::now(), found: Vec::new(), address: ANY_GAME.to_string(), locals: Locals::new(players.max(1) as usize, false), watching: false, held: Default::default(), said: Vec::new(), quiet: BTreeMap::new(), failed: None, room: None })
    }

    /// The ticket of a game hosted across the internet.
    pub fn ticket(&self) -> Option<&str> {
        self.link.ticket()
    }

    /// The code it goes by, once our server gave one.
    pub fn game_code(&self) -> Option<&str> {
        self.link.game_code()
    }

    /// Lets a machine asking in across the internet into the game.
    pub fn let_in(&mut self, peer: u32) {
        self.link.let_in(peer);
    }

    /// Its clock (µs since it began).
    pub fn now(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    /// Where the game is found (the host's).
    pub fn found(&self) -> &[String] {
        &self.found
    }

    pub fn hosting(&self) -> bool {
        matches!(self.side, Side::Host(_))
    }

    /// A line on the screen for a while (and in the terminal).
    pub fn say(&mut self, line: String) {
        println!("{line}");
        self.said.push((line, Instant::now()));
    }

    /// A guest keeps the network game's run among the window's runs (the
    /// host's own recorder keeps the host's).
    pub fn keep_run(&self, game: &Session<G>, runs: &crate::replay::Runs) {
        if let Side::Guest(j) = &self.side
            && matches!(j.state(), Joined::Playing | Joined::Apart(_) | Joined::Gone(_))
            && let Ok(replay) = j.recording(game)
        {
            runs.keep_also(&replay, "guest");
        }
    }

    /// What came since the last frame; the host lets in who asks to join
    /// and hears who went quiet or left.
    pub fn receive(&mut self, game: &Session<G>, lang: &str) -> Result<()> {
        let now = self.now();
        let came = self.link.drain();
        // A guest that got nowhere says why where it waits: the
        // matchmaker's refusal in the game's words, or what went wrong.
        let waiting = matches!(&self.side, Side::Guest(j) if matches!(j.state(), Joined::Waiting(None)));
        if let Some(refusal) = self.link.refused() {
            self.failed = Some(refusal_line(lang, refusal, &self.address));
        }
        for trouble in self.link.trouble() {
            println!("the network: {trouble}");
            if waiting && self.failed.is_none() {
                self.failed = Some(trouble.to_uppercase());
            }
        }
        // The host's room shows the seats as they are.
        if let Side::Host(h) = &self.side
            && self.link.game_code().is_some()
        {
            let seats = h.host.seats_taken();
            if self.room != Some(seats) {
                self.room = Some(seats);
                self.link.update_room(mm::Update { taken: Some(seats.0 as u32), seats: Some(seats.1 as u32), ..mm::Update::default() });
            }
        }
        match &mut self.side {
            Side::Host(h) => {
                for (from, bytes) in came {
                    h.host.receive(now, from, &bytes);
                }
                let names: BTreeMap<Machine, String> = h.host.roster().into_iter().map(|m| (m.index, m.name)).collect();
                let name = |m: Machine| names.get(&m).cloned().unwrap_or_else(|| format!("#{m}")).to_uppercase();
                let notices = h.notices(game)?;
                for notice in &notices {
                    println!("the network: {notice:?}");
                }
                for notice in notices {
                    match notice {
                        Notice::Joining { name, .. } => self.said.push((crate::lang::say_with(lang, "{} PLAYS", &[&name.to_uppercase()]), Instant::now())),
                        Notice::Quiet { index } => {
                            self.quiet.insert(index, (name(index), Instant::now()));
                        }
                        Notice::Back { index } => {
                            self.quiet.remove(&index);
                        }
                        Notice::Left { index, dropped } => {
                            let who = self.quiet.remove(&index).map_or_else(|| name(index), |(n, _)| n);
                            let line = if dropped { "{} WAS DROPPED" } else { "{} LEFT" };
                            self.said.push((crate::lang::say_with(lang, line, &[&who]), Instant::now()));
                        }
                        Notice::Apart { index, apart } => {
                            let commit = h.host.commit(index).unwrap_or("?").to_string();
                            self.said.push((desync_line(&apart, &name(index), &commit), Instant::now()));
                            for at in h.reports.drain(..) {
                                println!("the desync's report: {}", at.display());
                            }
                        }
                    }
                }
            }
            Side::Guest(j) => {
                for (from, bytes) in came {
                    j.guest.receive(now, from, &bytes);
                }
                let lines = j.guest.said();
                for line in &lines {
                    println!("the host says: {line}");
                }
                self.said.extend(lines.into_iter().map(|l| (l, Instant::now())));
            }
        }
        Ok(())
    }

    /// The seats this machine's players sit on: asked for those who sit
    /// (`sits`), given up for those who left; the players here whom no
    /// seat was free for.
    pub fn settle(&mut self, sits: impl Fn(usize) -> bool) -> Vec<usize> {
        let watching = self.watching;
        let sits = |local: usize| sits(local) && !(local == 1 && watching);
        let was = self.locals.seats();
        let refused = match &mut self.side {
            Side::Host(h) => self.locals.settle(h.as_mut(), sits),
            Side::Guest(j) => self.locals.settle(j.as_mut(), sits),
        };
        let now = self.locals.seats();
        if now != was {
            println!("this machine's players sit on seats {now:?}");
        }
        refused
    }

    /// A player's keys here, read this frame, and what it plays with.
    pub fn read(&mut self, seat: u8, input: crate::input::Input, controls: crate::controls::Controls) {
        match &mut self.side {
            Side::Host(h) => {
                h.read(seat, input);
                h.set_controls(seat, controls);
            }
            Side::Guest(j) => {
                j.read(seat, input);
                j.set_controls(seat, controls);
            }
        }
    }

    /// What is to be sent now.
    pub fn send(&mut self) {
        let now = self.now();
        let out = match &mut self.side {
            Side::Host(h) => h.host.outgoing(now),
            Side::Guest(j) => j.guest.outgoing(now),
        };
        for (to, bytes) in out {
            self.link.send(to, &bytes);
        }
    }

    /// Leaves the network game: the farewell, a few times (it may go
    /// astray).
    pub fn leave(&mut self) {
        let out = match &mut self.side {
            Side::Host(h) => (0..10).flat_map(|_| h.host.farewell()).collect(),
            Side::Guest(j) => j.end("LEFT"),
        };
        for (to, bytes) in out {
            self.link.send(to, &bytes);
        }
    }

    /// The lines at the top of the screen: who went quiet, who paused,
    /// what happened lately.
    pub fn notices(&mut self, lang: &str) -> Vec<String> {
        self.said.retain(|(_, at)| at.elapsed() < SAID_FOR);
        let mut lines: Vec<String> = self.quiet.values().map(|(name, _)| format!("{name} ...")).collect();
        // Across the internet: who asks in, with the code to compare.
        for knock in self.link.knocking() {
            lines.push(crate::lang::say_with(lang, "{} ASKS IN, SAYS {}", &[&knock_name(&knock), &knock.code]));
        }
        // A guest hears who went quiet from the host, and sees the host go
        // quiet itself.
        if let Side::Guest(j) = &self.side {
            lines.extend(j.guest.roster().iter().filter(|m| m.quiet).map(|m| format!("{} ...", m.name.to_uppercase())));
            if matches!(j.state(), Joined::Playing) && j.guest.silent_for(self.now()) >= crate::net::QUIET {
                let host = j.guest.roster().first().map_or_else(|| "HOST".to_string(), |m| m.name.to_uppercase());
                lines.push(format!("{host} ..."));
            }
        }
        if let Side::Guest(j) = &self.side
            && let Some(who) = j.guest.paused()
        {
            let name = j.guest.roster().iter().find(|m| m.index == who).map_or_else(|| "HOST".to_string(), |m| m.name.to_uppercase());
            lines.push(crate::lang::say_with(lang, "PAUSED BY {}", &[&name]));
        }
        lines.extend(self.said.iter().map(|(l, _)| l.clone()));
        lines
    }

    /// What the menus show of it: the host's addresses and who plays,
    /// machines to drop; a guest joining, how far.
    pub fn view(&self, lang: &str) -> NetView {
        match &self.side {
            Side::Host(h) => {
                let mut lines: Vec<String> = self.found.iter().map(|at| crate::lang::say_with(lang, "HOSTED AT {}", &[at])).collect();
                let knocks: Vec<(u32, String)> = self.link.knocking().iter().map(|k| (k.peer, format!("{} {}", knock_name(k), k.code))).collect();
                if self.link.ticket().is_some() {
                    match self.link.game_code() {
                        Some(code) => lines.push(crate::lang::say_with(lang, "ON THE INTERNET: CODE {}", &[code])),
                        None => lines.push(crate::lang::say(lang, "HOSTED ON THE INTERNET").to_string()),
                    }
                    if h.host.roster().len() == 1 && knocks.is_empty() {
                        let send = if self.link.game_code().is_some() { "SEND THE CODE (COPY THE CODE)" } else { "SEND THE TICKET (COPY THE TICKET)" };
                        lines.push(crate::lang::say(lang, send).to_string());
                    }
                }
                let who = h.host.roster().iter().skip(1).map(|m| format!("{} {} MS{}", m.name.to_uppercase(), m.rtt_ms, if m.quiet { " ..." } else { "" })).collect();
                // Nobody came yet: how to let them in on this system.
                let mut help = Vec::new();
                if h.host.roster().len() == 1 && self.link.ticket().is_none() {
                    let port = self.found.first().and_then(|f| f.rsplit(':').next()).unwrap_or("26500").to_string();
                    let (first, second) = match std::env::consts::OS {
                        "windows" => ("NOBODY COMES? LET VOXCADE IN (FIREWALL)", "AND MAKE THE NETWORK PRIVATE"),
                        "macos" => ("NOBODY COMES? START VOXCADE", "FROM THE TERMINAL"),
                        _ => ("NOBODY COMES? LET UDP {} IN:", "SUDO UFW ALLOW {}/UDP"),
                    };
                    help = vec![crate::lang::say_with(lang, first, &[&port]), crate::lang::say_with(lang, second, &[&port])];
                }
                let drops = self.quiet.iter().filter(|(_, (_, since))| since.elapsed() >= DROP_AFTER).map(|(&m, (name, _))| (m, name.clone())).collect();
                NetView { hosting: true, lines, who, help, drops, seated: true, ticket: self.link.ticket().is_some(), code: self.link.game_code().is_some(), knocks }
            }
            Side::Guest(j) => {
                let lines = match j.state() {
                    Joined::Waiting(None) => match (self.link.code(), &self.failed) {
                        (Some(code), _) => vec![crate::lang::say_with(lang, "THE HOST LETS YOU IN: SAY {}", &[code])],
                        (None, Some(failed)) => vec![failed.clone()],
                        (None, None) => {
                            let asked = match self.link.found_code() {
                                Some(code) => code.to_string(),
                                None if self.address == ANY_GAME => crate::lang::say(lang, ANY_GAME).to_string(),
                                None => self.address.to_uppercase(),
                            };
                            vec![crate::lang::say_with(lang, "ASKING {}", &[&asked])]
                        }
                    },
                    Joined::Waiting(Some((got, of))) => vec![crate::lang::say_with(lang, "RECEIVING THE GAME {}%", &[&(got * 100 / of.max(1)).to_string()])],
                    Joined::CatchingUp(at, of) => vec![crate::lang::say_with(lang, "CATCHING UP {}%", &[&(at * 100 / of.max(1)).to_string()])],
                    Joined::Playing => Vec::new(),
                    Joined::Apart(apart) => vec![desync_line(&apart, "", &j.host_commit)],
                    Joined::Gone(why) => vec![why],
                };
                let who = j.guest.roster().iter().map(|m| format!("{} {} MS", m.name.to_uppercase(), m.rtt_ms)).collect();
                NetView { hosting: false, lines, who, help: Vec::new(), drops: Vec::new(), seated: !self.locals.seats().is_empty() && !self.watching, ticket: false, code: false, knocks: Vec::new() }
            }
        }
    }
}

/// What our matchmaker knows a game by: `voxcade/` and its folder's name,
/// and its data's fingerprint (only the same meet, as on a LAN).
pub fn app_of<G: Game>(game: &Session<G>) -> Result<(String, String)> {
    Ok((format!("voxcade/{}", crate::settings::game_name(&game.dir)), format!("{:016x}", game.fingerprint()?)))
}

/// The room a game hosted here opens at our matchmaker: its title, seats,
/// and the host's name and build for the list.
pub fn room_of<G: Game>(game: &Session<G>) -> Result<mm::Room> {
    let (app, version) = app_of(game)?;
    Ok(mm::Room {
        app,
        version,
        title: game.cfg.title.clone(),
        seats: game.cfg.players.max(1),
        taken: 1,
        attributes: [("name".to_string(), udp::machine_name()), ("build".to_string(), udp::COMMIT.to_string())].into(),
        ..mm::Room::default()
    })
}

/// Why our matchmaker found no game, in the game's words (`asked`: the
/// code, as shown).
pub fn refusal_line(lang: &str, refusal: mm::Refusal, asked: &str) -> String {
    match refusal {
        mm::Refusal::UnknownCode => crate::lang::say_with(lang, "NO GAME WITH THE CODE {}", &[asked]),
        mm::Refusal::OtherApp => crate::lang::say(lang, "THE CODE IS ANOTHER GAME'S").to_string(),
        mm::Refusal::OtherVersion => crate::lang::say(lang, "THE CODE IS FOR ANOTHER VERSION").to_string(),
        mm::Refusal::NoRoomFree => crate::lang::say(lang, "NO GAME WITH A FREE SEAT").to_string(),
        mm::Refusal::TooFast => crate::lang::say(lang, "TOO MANY ASKS: WAIT A MOMENT").to_string(),
        other => other.text().to_uppercase(),
    }
}

/// A machine asking in, by the name it gave (a number until its HELLO came).
fn knock_name(knock: &crate::net::link::Knock) -> String {
    if knock.name.is_empty() { format!("#{}", knock.peer) } else { knock.name.to_uppercase() }
}

/// Where two machines' games ran apart, as every screen says it: the tick,
/// the part of the state, the machine and its build.
pub fn desync_line(apart: &crate::net::Apart, who: &str, commit: &str) -> String {
    let whose = if who.is_empty() { String::new() } else { format!(", {who}") };
    format!("DESYNC AT {} IN {}{whose}, {commit}", apart.tick, apart.part.to_uppercase())
}

/// The search for games on the LAN while JOIN shows: a broadcast every
/// second, the answers of the last three seconds.
pub struct Finder {
    socket: Udp,
    last: Option<Instant>,
    found: BTreeMap<SocketAddr, (crate::net::wire::Here, Instant)>,
    /// This game's fingerprint: another game's is greyed.
    fingerprint: u64,
}

impl Finder {
    pub fn new(fingerprint: u64) -> Result<Self> {
        Ok(Self { socket: Udp::broadcast()?, last: None, found: BTreeMap::new(), fingerprint })
    }

    /// Searches (once a second) and takes in the answers: the games found,
    /// as JOIN lists them.
    pub fn found(&mut self, lang: &str) -> Vec<crate::menu::FoundLine> {
        if self.last.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
            self.socket.search();
            self.last = Some(Instant::now());
        }
        for (from, bytes) in self.socket.drain() {
            if let Ok((crate::net::wire::Kind::Here, _, mut r)) = crate::net::wire::open(&bytes)
                && let Ok(here) = crate::net::wire::Here::read(&mut r)
            {
                self.found.insert(SocketAddr::new(from.ip(), here.port), (here, Instant::now()));
            }
        }
        self.found.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(3));
        self.found
            .iter()
            .map(|(at, (h, _))| {
                let build = if h.commit != udp::COMMIT { format!(" {}", crate::lang::say(lang, "(OTHER BUILD)")) } else { String::new() };
                let line = format!("{}  {}  {}/{}  {at}{build}", h.title.to_uppercase(), h.name.to_uppercase(), h.taken, h.seats);
                crate::menu::FoundLine { line, address: at.to_string(), same: h.fingerprint == self.fingerprint }
            })
            .collect()
    }
}

/// The open games of this game and version across the internet, from our
/// matchmaker, while JOIN shows: its list asked every three seconds. Its
/// endpoint starts on a thread of its own (reaching the relay takes a
/// moment), and the page never waits for it.
pub struct NetFinder {
    starting: Option<std::sync::mpsc::Receiver<Result<crate::net::quic::Quic>>>,
    quic: Option<crate::net::quic::Quic>,
    search: mm::Search,
    last: Option<Instant>,
    rooms: Vec<mm::Listed>,
}

impl NetFinder {
    pub fn new(app: String, version: String) -> Self {
        let (started, starting) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = started.send(crate::net::quic::Quic::start(crate::net::quic::reach()));
        });
        Self { starting: Some(starting), quic: None, search: mm::Search { app, version, ..mm::Search::default() }, last: None, rooms: Vec::new() }
    }

    /// Asks (every three seconds) and takes in the answers: the games
    /// found, as JOIN lists them (their code where a LAN game has its
    /// address).
    pub fn found(&mut self) -> Vec<crate::menu::FoundLine> {
        if let Some(starting) = &self.starting
            && let Ok(started) = starting.try_recv()
        {
            self.starting = None;
            match started {
                Ok(quic) => self.quic = Some(quic),
                Err(e) => eprintln!("the list of games on the internet: {e:#}"),
            }
        }
        if let Some(quic) = &self.quic {
            if self.last.is_none_or(|t| t.elapsed() >= Duration::from_secs(3)) {
                quic.list(self.search.clone());
                self.last = Some(Instant::now());
            }
            for event in quic.drain() {
                match event {
                    crate::net::quic::Event::Rooms(rooms) => self.rooms = rooms.rooms,
                    crate::net::quic::Event::Trouble(t) => eprintln!("the list of games on the internet: {t}"),
                    _ => {}
                }
            }
        }
        self.rooms
            .iter()
            .map(|room| {
                let name = room.attributes.get("name").map_or(String::new(), |n| format!("  {}", n.to_uppercase()));
                let build = if room.attributes.get("build").is_some_and(|b| b != udp::COMMIT) { " (OTHER BUILD)" } else { "" };
                let line = format!("{}{name}  {}/{}  {}{build}", room.title.to_uppercase(), room.taken, room.seats, room.code);
                crate::menu::FoundLine { line, address: room.code.clone(), same: true }
            })
            .collect()
    }
}
