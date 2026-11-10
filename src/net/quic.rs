//! Network play across the internet: iroh (QUIC between machines known by
//! their keys, hole punching, a relay where that fails, all encrypted) on
//! a thread of its own, `docs/multiplayer.md` stage 6. The window's loop
//! and the tools talk to it through channels and never wait on it: they
//! hand it what to send and take what came (`drain`). Each `Quic` is one
//! machine's endpoint, with a tokio runtime of one thread of its own.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use iroh::endpoint::{Connection, QuicTransportConfig, VarInt, presets};
use iroh::{Endpoint, EndpointAddr, RelayMap, RelayMode, RelayUrl, TransportAddr, Watcher};

use matchmaker_protocol as mm;

/// What the connections speak (QUIC's ALPN): Voxcade's network play.
pub const ALPN: &[u8] = b"voxcade/1";

/// A connection silent this long is gone: as stage 4's `SILENT`.
pub const SILENT: Duration = Duration::from_secs(10);

/// Our own relay (`docs/multiplayer.md` stage 6, step 3): its address and
/// the token every Voxcade shows it (our matchmaker's, `../matchmaker`: its
/// log says it), an access gate rather than a secret. `VOXCADE_RELAY` and
/// `VOXCADE_RELAY_TOKEN` say another.
const OUR_RELAY: Option<(&str, &str)> =
    Some(("https://voxcade.duckdns.org", "cdb4caeceefdf4f912ade3f145ab92081d93202c43991e51"));

/// Our matchmaker on our relay (`../matchmaker`: the public half of its
/// key in hex, its log says it); `VOXCADE_MATCHMAKER` says another.
/// Without it, and without our relay, a game across the internet goes by
/// its ticket only: no codes, no list.
const MATCHMAKER: Option<&str> = Some("81ed743c77a4936626dc4e911e219d370984c03924111c6e61fb4265e83c5fff");

/// The matchmaker: its key, and the relay it is reached through.
fn matchmaker_at() -> Option<EndpointAddr> {
    let key = std::env::var("VOXCADE_MATCHMAKER").ok().or_else(|| MATCHMAKER.map(str::to_string))?;
    let (relay, _) = our_relay().ok()??;
    Some(EndpointAddr::from_parts(key.parse().ok()?, [TransportAddr::Relay(relay)]))
}

/// Whether games meet through a matchmaker here (our relay and our
/// matchmaker are known): by codes, in the list, any game.
pub fn matchmaker_known() -> bool {
    matchmaker_at().is_some()
}
/// The relay to use: ours (or the one `VOXCADE_RELAY` names) with its
/// token, or None for n0's.
fn our_relay() -> Result<Option<(RelayUrl, Option<String>)>> {
    let (url, token) = match std::env::var("VOXCADE_RELAY") {
        Ok(url) => (url, std::env::var("VOXCADE_RELAY_TOKEN").ok()),
        Err(_) => match OUR_RELAY {
            Some((url, token)) => (url.to_string(), Some(token.to_string())),
            None => return Ok(None),
        },
    };
    let url: RelayUrl = url.parse().with_context(|| format!("the relay {url}"))?;
    Ok(Some((url, token)))
}

/// How an endpoint reaches the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Over the internet: directly where the routers let it, else through
    /// a relay (ours).
    Internet,
    /// Only through a relay, never directly: how the worst case plays.
    Relayed,
    /// Only directly, by address, no relay and no lookup: a LAN without
    /// the internet.
    Direct,
}

/// The reach a program's network play uses: across the internet, unless
/// `VOXCADE_REACH` says `relayed` (the worst case) or `direct` (a LAN
/// without the internet) for a test.
pub fn reach() -> Reach {
    match std::env::var("VOXCADE_REACH").as_deref() {
        Ok("relayed") => Reach::Relayed,
        Ok("direct") => Reach::Direct,
        _ => Reach::Internet,
    }
}

/// What came from the network.
#[derive(Debug)]
pub enum Event {
    /// A machine connected, as `peer` from now on, known by its key.
    Joined { peer: u32, key: [u8; 32] },
    Datagram { peer: u32, bytes: Vec<u8>, at: Instant },
    /// The path a connection uses now: through a relay or directly, its
    /// round trip, and the biggest datagram it carries.
    Path { peer: u32, relay: bool, rtt: Duration, max_datagram: Option<usize> },
    /// A connection ended (closed, or silent `SILENT`).
    Gone { peer: u32, why: String },
    /// The matchmaker's answers: the code the room opened here goes by
    /// (again, anew, should the matchmaker have to be reached afresh), a
    /// room found by its code or a quick join, a page of the list, why a
    /// join or a quick join was refused.
    Code(String),
    Found(mm::Found),
    Rooms(mm::Rooms),
    Refused(mm::Refusal),
    /// Something went wrong apart from any connection.
    Trouble(String),
}

enum Command {
    Connect(EndpointAddr),
    Send { peer: u32, bytes: Vec<u8> },
    /// Ends a connection (a machine not let in).
    Close(u32),
    /// Asks the matchmaker (in turn, on one connection kept).
    Ask(mm::Ask),
    /// Freezes the endpoint's thread: nothing is sent or answered any more,
    /// as a machine that lost its network (a test of noticing).
    Vanish,
}

/// One machine's endpoint, running on its own thread.
pub struct Quic {
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    events: mpsc::Receiver<Event>,
    ticket: String,
    key: [u8; 32],
}

impl Quic {
    /// Starts an endpoint and waits until it can be reached (at most 15 s
    /// for the relays); its ticket says how.
    pub fn start(reach: Reach) -> Result<Self> {
        let (commands, take) = tokio::sync::mpsc::unbounded_channel();
        let (tell, events) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        std::thread::Builder::new()
            .name("iroh".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = ready.send(Err(anyhow::anyhow!("the network's runtime: {e}")));
                        return;
                    }
                };
                runtime.block_on(run(reach, take, tell, ready));
            })
            .context("starting the network's thread")?;
        let (ticket, key) = started.recv().context("the network's thread ended")??;
        Ok(Self { commands, events, ticket, key })
    }

    /// This machine's key, as the others know it.
    pub fn key(&self) -> [u8; 32] {
        self.key
    }

    /// What another machine needs to connect to this one: a line of text.
    pub fn ticket(&self) -> &str {
        &self.ticket
    }

    /// Connects to the machine of `ticket`; its key.
    pub fn connect(&self, ticket: &str) -> Result<[u8; 32]> {
        let addr = read_ticket(ticket)?;
        let key = *addr.id.as_bytes();
        self.commands.send(Command::Connect(addr)).ok().context("the network's thread ended")?;
        Ok(key)
    }

    /// Ends the connection with `peer`.
    pub fn close(&self, peer: u32) {
        let _ = self.commands.send(Command::Close(peer));
    }

    /// Opens a room for the game hosted here, its ticket this endpoint's
    /// (`Event::Code`, or trouble).
    pub fn open(&self, room: mm::Room) {
        let _ = self.commands.send(Command::Ask(mm::Ask::Open(mm::Room { ticket: self.ticket.clone(), ..room })));
    }

    /// The room opened here changes (seats taken).
    pub fn update(&self, update: mm::Update) {
        let _ = self.commands.send(Command::Ask(mm::Ask::Update(update)));
    }

    /// The room of a code (`Event::Found`, or `Refused`).
    pub fn join(&self, join: mm::Join) {
        let _ = self.commands.send(Command::Ask(mm::Ask::Join(join)));
    }

    /// The list of open rooms (`Event::Rooms`).
    pub fn list(&self, search: mm::Search) {
        let _ = self.commands.send(Command::Ask(mm::Ask::List(search)));
    }

    /// Any room with a free seat (`Event::Found`, or `Refused`).
    pub fn quick(&self, search: mm::Search) {
        let _ = self.commands.send(Command::Ask(mm::Ask::Quick(search)));
    }

    /// Sends a datagram to `peer` (unreliable, unordered).
    pub fn send(&self, peer: u32, bytes: Vec<u8>) {
        let _ = self.commands.send(Command::Send { peer, bytes });
    }

    /// What came since asked last.
    pub fn drain(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Stops answering, without a word to the others (a test).
    pub fn vanish(&self) {
        let _ = self.commands.send(Command::Vanish);
    }
}

/// A ticket: `vxc1` and the endpoint's address (its key, and its relay or
/// its addresses on a LAN), base64.
fn write_ticket(addr: &EndpointAddr) -> Result<String> {
    use base64::Engine;
    Ok(format!("vxc1{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(addr)?)))
}

fn read_ticket(ticket: &str) -> Result<EndpointAddr> {
    use base64::Engine;
    let Some(text) = ticket.trim().strip_prefix("vxc1") else { bail!("no Voxcade ticket (it begins with vxc1)") };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(text).context("a ticket broken in copying")?;
    serde_json::from_slice(&bytes).context("a ticket broken in copying")
}

/// The endpoint's life: binds it, says its ticket, then serves commands
/// and connections until the program ends.
async fn run(reach: Reach, mut take: tokio::sync::mpsc::UnboundedReceiver<Command>, tell: mpsc::Sender<Event>, ready: mpsc::Sender<Result<(String, [u8; 32])>>) {
    let endpoint = match bind(reach).await {
        Ok(e) => e,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // Over the internet the ticket names the relay alone, not the host's
    // address at home: the machines learn each other's through it and go
    // directly where they can. On a LAN it is the addresses.
    let mut addr = endpoint.addr();
    if reach != Reach::Direct {
        addr.addrs.retain(TransportAddr::is_relay);
    }
    let _ = ready.send(write_ticket(&addr).map(|t| (t, *endpoint.id().as_bytes())));
    let mut peers: Vec<Option<Connection>> = Vec::new();
    let (joined, mut joins) = tokio::sync::mpsc::unbounded_channel::<Connection>();
    // The matchmaker, asked in turn on a task of its own.
    let (asking, asks) = tokio::sync::mpsc::unbounded_channel::<mm::Ask>();
    tokio::spawn(matchmaker(endpoint.clone(), asks, tell.clone()));
    loop {
        tokio::select! {
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break };
                let joined = joined.clone();
                let tell = tell.clone();
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(conn) => { let _ = joined.send(conn); }
                        Err(e) => { let _ = tell.send(Event::Trouble(format!("a machine could not connect: {e}"))); }
                    }
                });
            }
            conn = joins.recv() => {
                let Some(conn) = conn else { break };
                let peer = peers.len() as u32;
                peers.push(Some(conn.clone()));
                serve(peer, conn, tell.clone());
            }
            command = take.recv() => {
                match command {
                    None => break,
                    Some(Command::Connect(addr)) => {
                        let (endpoint, joined, tell) = (endpoint.clone(), joined.clone(), tell.clone());
                        tokio::spawn(async move {
                            match endpoint.connect(addr, ALPN).await {
                                Ok(conn) => { let _ = joined.send(conn); }
                                Err(e) => { let _ = tell.send(Event::Trouble(format!("connecting: {e}"))); }
                            }
                        });
                    }
                    Some(Command::Ask(ask)) => {
                        let _ = asking.send(ask);
                    }
                    Some(Command::Close(peer)) => {
                        if let Some(conn) = peers.get_mut(peer as usize).and_then(Option::take) {
                            conn.close(VarInt::from_u32(1), b"not let in");
                        }
                    }
                    Some(Command::Send { peer, bytes }) => {
                        if let Some(Some(conn)) = peers.get(peer as usize)
                            && let Err(e) = conn.send_datagram(bytes.into())
                        {
                            let _ = tell.send(Event::Trouble(format!("sending to {peer}: {e}")));
                        }
                    }
                    Some(Command::Vanish) => {
                        // The whole runtime stands still: nothing more is
                        // sent, answered or closed.
                        loop {
                            std::thread::sleep(Duration::from_secs(3600));
                        }
                    }
                }
            }
        }
    }
    endpoint.close().await;
}

/// The game's side of the matchmaker (`../matchmaker`): one connection,
/// made at the first ask and kept (a host's room lives as long), asks
/// answered in turn. Should the connection end while a room is open here
/// (the matchmaker started anew), the room is opened again and its new
/// code told.
async fn matchmaker(endpoint: Endpoint, mut asks: tokio::sync::mpsc::UnboundedReceiver<mm::Ask>, tell: mpsc::Sender<Event>) {
    let mut conn: Option<Connection> = None;
    let mut room: Option<mm::Room> = None;
    let mut reopen: Option<tokio::time::Instant> = None;
    loop {
        let watched = conn.clone();
        let closed = async move {
            match watched {
                Some(c) => {
                    c.closed().await;
                }
                None => std::future::pending().await,
            }
        };
        let retry = async move {
            match reopen {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            ask = asks.recv() => {
                let Some(ask) = ask else { break };
                match &ask {
                    mm::Ask::Open(r) => room = Some(r.clone()),
                    mm::Ask::Update(u) => {
                        if let Some(r) = &mut room {
                            changed(r, u);
                        }
                    }
                    _ => {}
                }
                let answer = ask_matchmaker(&endpoint, &mut conn, &ask).await;
                tell_answer(&tell, &ask, answer);
            }
            _ = closed => {
                conn = None;
                if room.is_some() {
                    reopen = Some(tokio::time::Instant::now() + Duration::from_secs(2));
                }
            }
            _ = retry => {
                reopen = None;
                if let Some(r) = room.clone() {
                    match ask_matchmaker(&endpoint, &mut conn, &mm::Ask::Open(r)).await {
                        Ok(mm::Answer::Code(code)) => { let _ = tell.send(Event::Code(code)); }
                        _ => reopen = Some(tokio::time::Instant::now() + Duration::from_secs(10)),
                    }
                }
            }
        }
    }
}

/// A room as an update leaves it.
fn changed(room: &mut mm::Room, update: &mm::Update) {
    if let Some(title) = &update.title {
        room.title = title.clone();
    }
    if let Some(seats) = update.seats {
        room.seats = seats;
    }
    if let Some(taken) = update.taken {
        room.taken = taken;
    }
    if let Some(attributes) = &update.attributes {
        room.attributes = attributes.clone();
    }
    if let Some(listed) = update.listed {
        room.listed = listed;
    }
}

/// Asks the matchmaker once, on the connection kept (made anew if there
/// is none).
async fn ask_matchmaker(endpoint: &Endpoint, conn: &mut Option<Connection>, ask: &mm::Ask) -> Result<mm::Answer> {
    if conn.as_ref().is_none_or(|c| c.close_reason().is_some()) {
        let at = matchmaker_at().context("no matchmaker known")?;
        *conn = Some(tokio::time::timeout(Duration::from_secs(10), endpoint.connect(at, mm::ALPN)).await.context("the matchmaker did not answer")??);
    }
    let c = conn.as_ref().expect("connected");
    let asked = async {
        let (mut send, mut recv) = c.open_bi().await?;
        send.write_all(&serde_json::to_vec(ask)?).await?;
        send.finish()?;
        let bytes = recv.read_to_end(mm::MAX_ANSWER).await?;
        anyhow::Ok(serde_json::from_slice(&bytes)?)
    };
    match tokio::time::timeout(Duration::from_secs(10), asked).await {
        Ok(Ok(answer)) => Ok(answer),
        Ok(Err(e)) => {
            *conn = None;
            Err(e)
        }
        Err(_) => {
            *conn = None;
            bail!("the matchmaker did not answer")
        }
    }
}

/// What the game hears of an answer.
fn tell_answer(tell: &mpsc::Sender<Event>, ask: &mm::Ask, answer: Result<mm::Answer>) {
    let event = match answer {
        Err(e) => Event::Trouble(format!("the matchmaker: {e:#}")),
        Ok(mm::Answer::Code(code)) => Event::Code(code),
        Ok(mm::Answer::Ok {}) => return,
        Ok(mm::Answer::Found(found)) => Event::Found(found),
        Ok(mm::Answer::Rooms(rooms)) => Event::Rooms(rooms),
        Ok(mm::Answer::No(refusal)) if matches!(ask, mm::Ask::Join(_) | mm::Ask::Quick(_)) => Event::Refused(refusal),
        Ok(mm::Answer::No(refusal)) => Event::Trouble(format!("the matchmaker: {refusal}")),
    };
    let _ = tell.send(event);
}

/// The endpoint `reach` asks for: silent connections end after `SILENT`.
async fn bind(reach: Reach) -> Result<Endpoint> {
    let transport = QuicTransportConfig::builder().max_idle_timeout(Some(VarInt::from_u32(SILENT.as_millis() as u32).into())).build();
    // Through our relay (tickets name it, no lookup needed) or n0's.
    let base = match our_relay()? {
        Some((url, token)) => {
            let mut map = RelayMap::from_iter([url]);
            if let Some(token) = token {
                map = map.with_auth_token(token);
            }
            Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Custom(map))
        }
        None => Endpoint::builder(presets::N0),
    };
    let builder = match reach {
        Reach::Internet => base,
        Reach::Relayed => base.clear_ip_transports(),
        Reach::Direct => Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled),
    };
    let endpoint = builder.alpns(vec![ALPN.to_vec()]).transport_config(transport).bind().await.context("opening the network")?;
    if reach != Reach::Direct {
        // Reachable once it has its relay (and knows its addresses).
        if tokio::time::timeout(Duration::from_secs(15), endpoint.online()).await.is_err() {
            // A relay that answered but would not have us says why.
            let statuses = endpoint.home_relay_status().get();
            if let Some(why) = statuses.iter().find_map(|s| s.auth_denied_reason().map(str::to_string)) {
                bail!("the relay would not have us: {why}");
            }
            if let Some(e) = statuses.iter().find_map(|s| s.last_error().map(|e| e.to_string())) {
                bail!("no relay within 15 s: {e}");
            }
            bail!("no relay answered within 15 s (is the internet there?)");
        }
    } else {
        // Its addresses on the networks it is on.
        let mut addr = endpoint.watch_addr();
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            while !addr.get().addrs.iter().any(TransportAddr::is_ip) {
                if addr.updated().await.is_err() {
                    break;
                }
            }
        })
        .await;
    }
    Ok(endpoint)
}

/// Tells of a connection: it joined, its datagrams, its path as it
/// changes, its end.
fn serve(peer: u32, conn: Connection, tell: mpsc::Sender<Event>) {
    let _ = tell.send(Event::Joined { peer, key: *conn.remote_id().as_bytes() });
    let (reading, telling) = (conn.clone(), tell.clone());
    tokio::spawn(async move {
        while let Ok(bytes) = reading.read_datagram().await {
            if telling.send(Event::Datagram { peer, bytes: bytes.to_vec(), at: Instant::now() }).is_err() {
                break;
            }
        }
    });
    let (watching, telling) = (conn.clone(), tell.clone());
    tokio::spawn(async move {
        let mut last: Option<bool> = None;
        loop {
            let selected = watching.paths().iter().find(|p| p.is_selected()).map(|p| (p.is_relay(), p.rtt()));
            if let Some((relay, rtt)) = selected
                && last != Some(relay)
            {
                last = Some(relay);
                let _ = telling.send(Event::Path { peer, relay, rtt, max_datagram: watching.max_datagram_size() });
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                _ = watching.closed() => break,
            }
        }
    });
    tokio::spawn(async move {
        let why = conn.closed().await.to_string();
        let _ = tell.send(Event::Gone { peer, why });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Takes events from both until `done` holds (10 s at most).
    fn until(a: &Quic, b: &Quic, mut done: impl FnMut(bool, Event) -> bool) -> bool {
        let limit = Instant::now() + Duration::from_secs(10);
        while Instant::now() < limit {
            for (first, q) in [(true, a), (false, b)] {
                for e in q.drain() {
                    if done(first, e) {
                        return true;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Two endpoints on this machine, without a relay or the internet,
    /// find each other by a ticket and carry datagrams both ways; a ticket
    /// broken in copying is said to be so.
    #[test]
    fn two_endpoints_carry_datagrams_directly() {
        let host = Quic::start(Reach::Direct).unwrap();
        let guest = Quic::start(Reach::Direct).unwrap();
        assert!(guest.connect("vxc1nonsense").is_err());
        assert!(guest.connect("hello").is_err());
        guest.connect(host.ticket()).unwrap();
        let (mut peer, mut joined) = (None, false);
        assert!(until(&host, &guest, |at_host, e| {
            match e {
                Event::Joined { peer: p, .. } if at_host => peer = Some(p),
                Event::Joined { .. } => joined = true,
                _ => {}
            }
            peer.is_some() && joined
        }));
        guest.send(0, b"up".to_vec());
        host.send(peer.unwrap(), b"down".to_vec());
        let (mut up, mut down) = (false, false);
        assert!(until(&host, &guest, |at_host, e| {
            if let Event::Datagram { bytes, .. } = e {
                if at_host && bytes == b"up" {
                    up = true;
                }
                if !at_host && bytes == b"down" {
                    down = true;
                }
            }
            up && down
        }));
    }
}
