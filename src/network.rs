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
/// Who a guest asks when it was given a ticket (the menus' word).
const TICKETS_HOST: &str = "THE TICKET'S HOST";

pub enum Side<G: Game> {
    Host(Box<Hosting<G, Where>>),
    Guest(Box<Joining<G, Where>>),
}

/// How the machines reach each other, as it comes: across the internet it
/// starts on a thread of its own (reaching the relay takes up to 15 s), and
/// the window never waits for it: the game plays, JOINING shows and BACK
/// leaves meanwhile.
enum Linking {
    Up(Link),
    Starting(std::sync::mpsc::Receiver<Result<Link>>),
    /// It would not start.
    Down,
}

impl Linking {
    fn start(make: impl FnOnce() -> Result<Link> + Send + 'static) -> Self {
        Self::Starting(on_a_thread(make))
    }
}

/// What `make` makes, on a thread of its own: taken from the channel when
/// it is there.
fn on_a_thread<T: Send + 'static>(make: impl FnOnce() -> T + Send + 'static) -> std::sync::mpsc::Receiver<T> {
    let (made, making) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = made.send(make());
    });
    making
}

pub struct Network<G: Game> {
    pub side: Side<G>,
    link: Linking,
    /// Why the host's link would not start (said once), and whether its
    /// ticket came since asked.
    broken: Option<String>,
    new_ticket: bool,
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
    fn with(side: Side<G>, link: Linking, found: Vec<String>, address: String, locals: Locals) -> Self {
        Self { side, link, broken: None, new_ticket: false, start: Instant::now(), found, address, locals, watching: false, held: Default::default(), said: Vec::new(), quiet: BTreeMap::new(), failed: None, room: None }
    }

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
        Ok(Self::with(Side::Host(Box::new(hosting)), Linking::Up(link), found, String::new(), Locals::new(players, true)))
    }

    /// Hosts `game` across the internet: who has the ticket may ask in,
    /// and the host's player lets each in (`let_in`). Where our
    /// matchmaker is known the game opens a room there: its code, and a
    /// line in the list of open games. The game begins at once, its link
    /// on the way (`new_ticket` once it is there; `broken` if it would
    /// not start).
    pub fn host_net(game: &Session<G>) -> Result<Self> {
        let room = room_of(game)?;
        let mut hosting = Hosting::new(game, udp::random(), &udp::machine_name())?;
        hosting.reports_to = Some(crate::net::play::reports());
        hosting.host.set_limit(crate::net::link::NET_LIMIT);
        let players = game.cfg.players.max(1) as usize;
        let reach = crate::net::quic::reach();
        let link = Linking::start(move || Link::net_host(reach, Some(room)));
        Ok(Self::with(Side::Host(Box::new(hosting)), link, Vec::new(), String::new(), Locals::new(players, true)))
    }

    /// Joins the game hosted at `address` (`host[:port]` on the LAN), or
    /// across the internet by its ticket or its code (`game`: the app and
    /// version played here, `app_of`, so a code of another is refused as
    /// such; the link on the way, JOINING shows meanwhile).
    pub fn join(dir: &std::path::Path, address: &str, players: u32, game: Option<(String, String)>) -> Result<Self> {
        let (link, host) = if Link::is_net(address) {
            let (text, reach) = (address.to_string(), crate::net::quic::reach());
            (Linking::start(move || Link::net_guest(&text, reach, game).map(|(link, _)| link)), Where::Net(0))
        } else {
            (Linking::Up(Link::lan_guest()?), Where::Lan(udp::address(address)?))
        };
        let shown = match matchmaker_protocol::tidy(address) {
            Some(code) => code,
            None if Link::is_ticket(address) => TICKETS_HOST.to_string(),
            None => address.to_string(),
        };
        Ok(Self::with(Side::Guest(Box::new(Self::joining(dir, host))), link, Vec::new(), shown, Locals::new(players.max(1) as usize, false)))
    }

    /// Joins any game of `app` and `version` across the internet with a
    /// free seat (our matchmaker's quick join).
    pub fn join_any(dir: &std::path::Path, players: u32, app: String, version: String) -> Result<Self> {
        let reach = crate::net::quic::reach();
        let link = Linking::start(move || Link::net_guest_any(reach, mm::Search { app, version, ..mm::Search::default() }).map(|(link, _)| link));
        Ok(Self::with(Side::Guest(Box::new(Self::joining(dir, Where::Net(0)))), link, Vec::new(), ANY_GAME.to_string(), Locals::new(players.max(1) as usize, false)))
    }

    /// A guest of the host at `host`: across the internet it sends no
    /// bigger datagrams than iroh carries.
    fn joining(dir: &std::path::Path, host: Where) -> Joining<G, Where> {
        let mut guest = crate::net::Guest::new(host, udp::program_id(), udp::random(), &udp::machine_name(), udp::COMMIT, 0);
        if matches!(host, Where::Net(_)) {
            guest.set_limit(crate::net::link::NET_LIMIT);
        }
        let mut joining = Joining::new(dir, guest);
        joining.reports_to = Some(crate::net::play::reports());
        joining
    }

    /// The link, once it is there.
    fn link(&self) -> Option<&Link> {
        match &self.link {
            Linking::Up(link) => Some(link),
            _ => None,
        }
    }

    /// Takes the link in once it started; one that would not start ends a
    /// guest's joining, or the host's network game (`broken`), saying why.
    fn settle_link(&mut self) {
        let Linking::Starting(starting) = &self.link else { return };
        let why = match starting.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Ok(Ok(link)) => {
                if let Some(ticket) = link.ticket() {
                    println!("hosting across the internet; the ticket:\n{ticket}");
                    self.new_ticket = true;
                }
                self.link = Linking::Up(link);
                // Its clock begins now: a guest asks the host for as long as
                // it would have, the time reaching the relay not counted.
                self.start = Instant::now();
                return;
            }
            Ok(Err(e)) => format!("{e:#}"),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => "the network's thread ended".to_string(),
        };
        eprintln!("the network: {why}");
        self.link = Linking::Down;
        match &mut self.side {
            Side::Guest(j) => {
                j.end(&why.to_uppercase());
            }
            Side::Host(_) => self.broken = Some(why),
        }
    }

    /// Why the host's link across the internet would not start, once: its
    /// network game is over (the game plays on alone).
    pub fn broken(&mut self) -> Option<String> {
        self.broken.take()
    }

    /// The ticket of a game hosted across the internet, once, as it came.
    pub fn new_ticket(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.new_ticket) {
            return None;
        }
        self.ticket().map(str::to_string)
    }

    /// Whether the console line `line` may run now (`Hosting::takes_line`:
    /// it must travel whole with the next tick); a guest has no console.
    pub fn takes_line(&self, game: &Session<G>, line: &str) -> Result<()> {
        match &self.side {
            Side::Host(h) => h.takes_line(game, line),
            Side::Guest(_) => Ok(()),
        }
    }

    /// The ticket of a game hosted across the internet.
    pub fn ticket(&self) -> Option<&str> {
        self.link()?.ticket()
    }

    /// The code it goes by, once our server gave one.
    pub fn game_code(&self) -> Option<&str> {
        self.link()?.game_code()
    }

    /// Lets a machine asking in across the internet into the game.
    pub fn let_in(&mut self, peer: u32) {
        if let Linking::Up(link) = &mut self.link {
            link.let_in(peer);
        }
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
        // Nothing comes before the link is there.
        self.settle_link();
        let Linking::Up(link) = &mut self.link else { return Ok(()) };
        let now = self.start.elapsed().as_micros() as u64;
        let came = link.drain();
        // A guest that got nowhere says why where it waits: the
        // matchmaker's refusal in the game's words, or what went wrong.
        let waiting = matches!(&self.side, Side::Guest(j) if matches!(j.state(), Joined::Waiting(None)));
        if let Some(refusal) = link.refused() {
            self.failed = Some(refusal_line(lang, refusal, &self.address));
        }
        for trouble in link.trouble() {
            println!("the network: {trouble}");
            if waiting && self.failed.is_none() {
                self.failed = Some(trouble.to_uppercase());
            }
        }
        // The host's room shows the seats as they are.
        if let Side::Host(h) = &self.side
            && link.game_code().is_some()
        {
            let seats = h.host.seats_taken();
            if self.room != Some(seats) {
                self.room = Some(seats);
                link.update_room(mm::Update { taken: Some(seats.0 as u32), seats: Some(seats.1 as u32), ..mm::Update::default() });
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
        // Nothing goes before the link is there (a guest's HELLO waits).
        let Linking::Up(link) = &mut self.link else { return };
        let out = match &mut self.side {
            Side::Host(h) => h.host.outgoing(now),
            Side::Guest(j) => j.guest.outgoing(now),
        };
        for (to, bytes) in out {
            link.send(to, bytes);
        }
        // The host closes who was refused or is gone (across the internet).
        if let Side::Host(h) = &mut self.side {
            for at in h.host.to_close(now) {
                link.close(at);
            }
        }
    }

    /// Leaves the network game: the farewell, a few times (it may go
    /// astray).
    pub fn leave(&mut self) {
        let out = match &mut self.side {
            Side::Host(h) => (0..10).flat_map(|_| h.host.farewell()).collect(),
            Side::Guest(j) => j.end("YOU LEFT"),
        };
        if let Linking::Up(link) = &self.link {
            for (to, bytes) in out {
                link.send(to, bytes);
            }
        }
    }

    /// The lines at the top of the screen: who went quiet, who paused,
    /// what happened lately.
    pub fn notices(&mut self, lang: &str) -> Vec<String> {
        self.said.retain(|(_, at)| at.elapsed() < SAID_FOR);
        let mut lines: Vec<String> = self.quiet.values().map(|(name, _)| format!("{name} ...")).collect();
        // Across the internet: who asks in, with the code to compare.
        for knock in self.link().map(Link::knocking).unwrap_or_default() {
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
                let knocks: Vec<(u32, String)> = self.link().map(Link::knocking).unwrap_or_default().iter().map(|k| (k.peer, format!("{} {}", knock_name(k), k.code))).collect();
                if self.ticket().is_some() {
                    match self.game_code() {
                        Some(code) => lines.push(crate::lang::say_with(lang, "ON THE INTERNET: CODE {}", &[code])),
                        None => lines.push(crate::lang::say(lang, "HOSTED ON THE INTERNET").to_string()),
                    }
                    if h.host.roster().len() == 1 && knocks.is_empty() {
                        let send = if self.game_code().is_some() { "SEND THE CODE (COPY THE CODE)" } else { "SEND THE TICKET (COPY THE TICKET)" };
                        lines.push(crate::lang::say(lang, send).to_string());
                    }
                }
                let who = h.host.roster().iter().skip(1).map(|m| format!("{} {} MS{}", m.name.to_uppercase(), m.rtt_ms, if m.quiet { " ..." } else { "" })).collect();
                // Nobody came yet: how to let them in on this system (a
                // LAN's host: its addresses are found).
                let mut help = Vec::new();
                if h.host.roster().len() == 1 && !self.found.is_empty() {
                    let port = self.found.first().and_then(|f| f.rsplit(':').next()).unwrap_or("26500").to_string();
                    let (first, second) = match std::env::consts::OS {
                        "windows" => ("NOBODY COMES? LET VOXCADE IN (FIREWALL)", "AND MAKE THE NETWORK PRIVATE"),
                        "macos" => ("NOBODY COMES? START VOXCADE", "FROM THE TERMINAL"),
                        _ => ("NOBODY COMES? LET UDP {} IN:", "SUDO UFW ALLOW {}/UDP"),
                    };
                    help = vec![crate::lang::say_with(lang, first, &[&port]), crate::lang::say_with(lang, second, &[&port])];
                }
                let drops = self.quiet.iter().filter(|(_, (_, since))| since.elapsed() >= DROP_AFTER).map(|(&m, (name, _))| (m, name.clone())).collect();
                NetView { hosting: true, lines, who, help, drops, seated: true, ticket: self.ticket().is_some(), code: self.game_code().is_some(), knocks }
            }
            Side::Guest(j) => {
                let lines = match j.state() {
                    Joined::Waiting(None) => match (self.link().and_then(Link::code), &self.failed) {
                        (Some(code), _) => vec![crate::lang::say_with(lang, "THE HOST LETS YOU IN: SAY {}", &[code])],
                        (None, Some(failed)) => vec![failed.clone()],
                        (None, None) => {
                            let asked = match self.link().and_then(Link::found_code) {
                                Some(code) => code.to_string(),
                                None if self.address == ANY_GAME => crate::lang::say(lang, ANY_GAME).to_string(),
                                None if self.address == TICKETS_HOST => crate::lang::say(lang, TICKETS_HOST).to_string(),
                                None => self.address.to_uppercase(),
                            };
                            vec![crate::lang::say_with(lang, "ASKING {}", &[&asked])]
                        }
                    },
                    Joined::Waiting(Some((got, of))) => vec![crate::lang::say_with(lang, "RECEIVING THE GAME {}%", &[&(got * 100 / of.max(1)).to_string()])],
                    Joined::CatchingUp(at, of) => vec![crate::lang::say_with(lang, "CATCHING UP {}%", &[&(at * 100 / of.max(1)).to_string()])],
                    Joined::Playing => Vec::new(),
                    Joined::Apart(apart) => vec![desync_line(&apart, "", &j.host_commit)],
                    Joined::Gone(why) => vec![crate::lang::say_line(lang, &why)],
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
        let starting = on_a_thread(|| crate::net::quic::Quic::start(crate::net::quic::reach()));
        Self { starting: Some(starting), quic: None, search: mm::Search { app, version, ..mm::Search::default() }, last: None, rooms: Vec::new() }
    }

    /// Asks (every three seconds) and takes in the answers: the games
    /// found, as JOIN lists them (their code where a LAN game has its
    /// address).
    pub fn found(&mut self, lang: &str) -> Vec<crate::menu::FoundLine> {
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
                let build = if room.attributes.get("build").is_some_and(|b| b != udp::COMMIT) { format!(" {}", crate::lang::say(lang, "(OTHER BUILD)")) } else { String::new() };
                let line = format!("{}{name}  {}/{}  {}{build}", room.title.to_uppercase(), room.taken, room.seats, room.code);
                crate::menu::FoundLine { line, address: room.code.clone(), same: true }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Lua = crate::Voxcade;

    fn pulsar() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/pulsar")
    }

    /// A link across the internet that takes long to start (up to 15 s for
    /// the relays): it would not start once let go.
    fn slow() -> (std::sync::mpsc::Sender<()>, Linking) {
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let link = Linking::start(move || {
            let _ = wait.recv();
            anyhow::bail!("no relay answered within 15 s")
        });
        (go, link)
    }

    /// The frames of a window until `done` holds (5 s at most).
    fn frames(net: &mut Network<Lua>, game: &Session<Lua>, mut done: impl FnMut(&mut Network<Lua>) -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            net.receive(game, "en").unwrap();
            net.send();
            if done(net) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    /// Joining or hosting across the internet holds no frame of the window
    /// while the link starts: JOINING shows what it asks, the host's game
    /// plays. A link that would not start ends the guest's joining, or the
    /// host's network game (the game plays on alone), saying why.
    #[test]
    fn a_link_starting_holds_no_frame() {
        let game = Session::<Lua>::load(&pulsar()).unwrap();
        let guest = crate::net::Guest::new(Where::Net(0), [4; 16], 1, "guest", "x", 0);
        let (go, link) = slow();
        let mut net = Network::with(Side::Guest(Box::new(Joining::new(&pulsar(), guest))), link, Vec::new(), "AAX7QK".into(), Locals::new(1, false));
        let begun = Instant::now();
        for _ in 0..10 {
            net.receive(&game, "en").unwrap();
            net.send();
        }
        assert!(begun.elapsed() < Duration::from_millis(500), "ten frames took {:?}", begun.elapsed());
        assert!(matches!(&net.side, Side::Guest(j) if j.state() == Joined::Waiting(None)));
        assert_eq!(net.view("en").lines, ["ASKING AAX7QK"]);
        go.send(()).unwrap();
        assert!(frames(&mut net, &game, |n| matches!(&n.side, Side::Guest(j) if matches!(j.state(), Joined::Gone(_)))), "it ends");
        let Side::Guest(j) = &net.side else { unreachable!() };
        assert_eq!(j.state(), Joined::Gone("NO RELAY ANSWERED WITHIN 15 S".into()));

        let hosting = Hosting::new(&game, 1, "host").unwrap();
        let (go, link) = slow();
        let mut net = Network::with(Side::Host(Box::new(hosting)), link, Vec::new(), String::new(), Locals::new(2, true));
        net.receive(&game, "en").unwrap();
        assert_eq!((net.broken(), net.new_ticket(), net.ticket()), (None, None, None), "on the way");
        assert!(net.view("en").help.is_empty(), "no word about a LAN's firewall");
        go.send(()).unwrap();
        let mut why = None;
        assert!(frames(&mut net, &game, |n| {
            why = n.broken();
            why.is_some()
        }));
        assert!(why.unwrap().contains("no relay"));
    }
}
