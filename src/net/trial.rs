//! The iroh trial (`docs/multiplayer.md` stage 6, step 2): whether iroh
//! carries lockstep play at 60 ticks a second, pass or fail by numbers
//! before anything is built on it. `voxcade nettest` plays a host and its
//! guests in one process, each an endpoint on a thread of its own as on a
//! machine of its own, sharing one clock: so a datagram's way is measured
//! one way, not as half a round trip. `nettest host` and `nettest join`
//! measure between two machines by round trips.
//!
//! The bounds are our own reasoning, not a source's: a guest's hand
//! (stage 4's jitter buffer) holds up to 6 ticks, 100 ms; stage 4 counts a
//! machine gone after 10 s of silence; a bundle of stage 4 is up to 1,000
//! bytes.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use super::quic::{Event, Quic, Reach};

/// The trial's bounds.
const MOST_LOSS: f64 = 0.01;
const MOST_P99_RELAYED: Duration = Duration::from_millis(100);
const MOST_P99_DIRECT: Duration = Duration::from_millis(20);
const DATAGRAM_NEEDED: usize = 1000;
const NOTICED_WITHIN: Duration = Duration::from_secs(15);
const MOST_FRAME_P99_ADDED: Duration = Duration::from_millis(2);
const LONGEST_FRAME: Duration = Duration::from_millis(50);

/// A tick: 60 a second.
const TICK: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// What a datagram of the trial is.
const UP: u8 = 0;
const DOWN: u8 = 1;
const PROBE: u8 = 2;
const ECHO: u8 = 3;

/// A datagram of `size` bytes: its kind, who sent it, its number and when
/// (nanoseconds on the shared clock).
fn datagram(kind: u8, from: u8, seq: u32, t: u64, size: usize) -> Vec<u8> {
    let mut d = vec![0u8; size.max(14)];
    d[0] = kind;
    d[1] = from;
    d[2..6].copy_from_slice(&seq.to_le_bytes());
    d[6..14].copy_from_slice(&t.to_le_bytes());
    d
}

fn read(d: &[u8]) -> Option<(u8, u8, u32, u64)> {
    (d.len() >= 14).then(|| (d[0], d[1], u32::from_le_bytes(d[2..6].try_into().unwrap()), u64::from_le_bytes(d[6..14].try_into().unwrap())))
}

/// Delays as they came, and how many were sent.
#[derive(Default)]
struct Way {
    sent: u64,
    delays: Vec<Duration>,
}

impl Way {
    fn loss(&self) -> f64 {
        if self.sent == 0 { 0.0 } else { 1.0 - self.delays.len() as f64 / self.sent as f64 }
    }

    fn at(&self, share: f64) -> Duration {
        let mut d = self.delays.clone();
        d.sort();
        d.get(((d.len() as f64 * share) as usize).min(d.len().saturating_sub(1))).copied().unwrap_or_default()
    }
}

/// The frames of a loop, by how long each took.
fn frames_p99(frames: &[Duration]) -> (Duration, Duration) {
    let mut f = frames.to_vec();
    f.sort();
    let p99 = f.get(((f.len() as f64 * 0.99) as usize).min(f.len().saturating_sub(1))).copied().unwrap_or_default();
    (p99, f.last().copied().unwrap_or_default())
}

/// Some work a frame does on the main thread, the same with the network
/// as without: about a millisecond.
fn frame_work(seed: &mut u64) {
    let until = Instant::now() + Duration::from_micros(1000);
    while Instant::now() < until {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    }
}

/// One line of the verdict.
fn line(ok: bool, what: String) -> bool {
    println!("  {} {what}", if ok { "PASS" } else { "FAIL" });
    ok
}

/// Waits for events until `done` says so or `limit` passes.
fn wait(quics: &[&Quic], limit: Duration, mut seen: impl FnMut(usize, Event) -> bool) -> bool {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        for (i, q) in quics.iter().enumerate() {
            for e in q.drain() {
                if let Event::Trouble(t) = &e {
                    eprintln!("  (machine {i}: {t})");
                }
                if seen(i, e) {
                    return true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

/// A host and `guests` guests in one process, reaching each other as
/// `reach` says, playing `seconds` of ticks of `size` bytes each way.
/// Whether every bound held.
pub fn run(reach: Reach, guests: usize, seconds: u32, size: usize) -> Result<bool> {
    anyhow::ensure!((1..=7).contains(&guests), "1 to 7 guests");
    let clock = Instant::now();
    let now = || clock.elapsed().as_nanos() as u64;

    // The main loop alone first, for its frame times without the network.
    let mut seed = 1u64;
    let mut alone = Vec::new();
    let mut next = Instant::now();
    for _ in 0..180 {
        let began = Instant::now();
        frame_work(&mut seed);
        next += TICK;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        alone.push(began.elapsed());
    }

    println!("starting a host and {guests} guests ({reach:?}) ...");
    let started = Instant::now();
    let host = Quic::start(reach)?;
    let joining: Vec<Quic> = (0..guests).map(|_| Quic::start(reach)).collect::<Result<_>>()?;
    for g in &joining {
        g.connect(host.ticket())?;
    }
    let mut all: Vec<&Quic> = vec![&host];
    all.extend(joining.iter());
    let mut joined = 0;
    let mut paths: Vec<(usize, u32, bool, Duration, Option<usize>)> = Vec::new();
    let connected = wait(&all, Duration::from_secs(30), |i, e| {
        match e {
            Event::Joined { .. } if i == 0 => joined += 1,
            Event::Path { peer, relay, rtt, max_datagram } => paths.push((i, peer, relay, rtt, max_datagram)),
            _ => {}
        }
        joined == guests
    });
    if !connected {
        bail!("only {joined} of {guests} guests connected within 30 s");
    }
    println!("connected in {:.1} s", started.elapsed().as_secs_f32());

    // Lockstep's traffic: every tick each guest's reading up, the host's
    // bundle down to each guest.
    let mut up: Vec<Way> = (0..guests).map(|_| Way::default()).collect();
    let mut down: Vec<Way> = (0..guests).map(|_| Way::default()).collect();
    let mut frames = Vec::new();
    let ticks = seconds * 60;
    let mut next = Instant::now();
    for seq in 0..ticks {
        let began = Instant::now();
        let t = now();
        for (g, quic) in joining.iter().enumerate() {
            quic.send(0, datagram(UP, g as u8, seq, t, size));
            up[g].sent += 1;
            // The host's peers are numbered as they joined; any order.
            host.send(g as u32, datagram(DOWN, g as u8, seq, t, size));
        }
        for (i, quic) in all.iter().enumerate() {
            for e in quic.drain() {
                match e {
                    Event::Datagram { bytes, at, .. } => {
                        let Some((kind, from, _, sent)) = read(&bytes) else { continue };
                        let delay = Duration::from_nanos((at.duration_since(clock).as_nanos() as u64).saturating_sub(sent));
                        match kind {
                            UP if i == 0 => up[from as usize].delays.push(delay),
                            DOWN if i > 0 => down[i - 1].delays.push(delay),
                            _ => {}
                        }
                    }
                    Event::Path { peer, relay, rtt, max_datagram } => paths.push((i, peer, relay, rtt, max_datagram)),
                    Event::Trouble(t) => eprintln!("  (machine {i}: {t})"),
                    _ => {}
                }
            }
        }
        frame_work(&mut seed);
        next += TICK;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        frames.push(began.elapsed());
    }
    // What is still on its way.
    wait(&all, Duration::from_millis(500), |i, e| {
        if let Event::Datagram { bytes, at, .. } = e
            && let Some((kind, from, _, sent)) = read(&bytes)
        {
            let delay = Duration::from_nanos((at.duration_since(clock).as_nanos() as u64).saturating_sub(sent));
            match kind {
                UP if i == 0 => up[from as usize].delays.push(delay),
                DOWN if i > 0 => down[i - 1].delays.push(delay),
                _ => {}
            }
        }
        false
    });
    for way in &mut down {
        way.sent = ticks as u64;
    }

    // The biggest datagram that passes, the first guest to the host (a
    // guest's only peer is the host; the host numbers its guests as they
    // joined).
    let sizes = [1400, 1300, 1200, 1150, 1100, 1050, 1000, 900, 800];
    for &s in &sizes {
        joining[0].send(0, datagram(PROBE, 0, s as u32, now(), s));
    }
    let mut passed = Vec::new();
    wait(&all, Duration::from_millis(1500), |i, e| {
        if let (0, Event::Datagram { bytes, .. }) = (i, e)
            && let Some((PROBE, _, s, _)) = read(&bytes)
        {
            passed.push(s as usize);
        }
        false
    });
    let biggest = passed.iter().copied().max().unwrap_or(0);

    // A guest vanishes without a word: how soon the host knows.
    let vanished = Instant::now();
    joining[guests - 1].vanish();
    let noticed = wait(&[&host], Duration::from_secs(40), |_, e| matches!(e, Event::Gone { .. })).then(|| vanished.elapsed());

    // The verdict.
    let relayed = paths.iter().filter(|p| p.0 > 0).all(|p| p.2) && !paths.is_empty();
    let direct = paths.iter().filter(|p| p.0 > 0).all(|p| !p.2) && !paths.is_empty();
    println!("paths (machine, peer, relayed, round trip, biggest datagram):");
    for (i, peer, relay, rtt, max) in &paths {
        println!("  {i} -> {peer}: {}, {:.1} ms, {}", if *relay { "relay" } else { "direct" }, rtt.as_secs_f64() * 1000.0, max.map_or("?".into(), |m| m.to_string()));
    }
    let most = if direct { MOST_P99_DIRECT } else { MOST_P99_RELAYED };
    let ms = |d: Duration| format!("{:.1} ms", d.as_secs_f64() * 1000.0);
    println!("{} s at 60 Hz, {size} bytes each way, {guests} guests, the paths {}:", seconds, if relayed { "relayed" } else if direct { "direct" } else { "mixed" });
    let mut ok = true;
    for (name, ways) in [("up", &up), ("down", &down)] {
        for (g, w) in ways.iter().enumerate() {
            let what = format!("{name} {g}: {} of {} ({:.2} % lost), p50 {}, p99 {}, most {}", w.delays.len(), w.sent, w.loss() * 100.0, ms(w.at(0.5)), ms(w.at(0.99)), ms(w.at(1.0)));
            ok &= line(w.loss() <= MOST_LOSS && w.at(0.99) <= most, what);
        }
    }
    ok &= line(biggest >= DATAGRAM_NEEDED, format!("the biggest datagram that passes: {biggest} bytes (stage 4 needs {DATAGRAM_NEEDED})"));
    ok &= line(noticed.is_some_and(|n| n <= NOTICED_WITHIN), format!("a machine that vanished noticed after {}", noticed.map_or("never (40 s)".into(), |n| format!("{:.1} s", n.as_secs_f32()))));
    let (alone_p99, _) = frames_p99(&alone);
    let (p99, longest) = frames_p99(&frames);
    ok &= line(p99 <= alone_p99 + MOST_FRAME_P99_ADDED && longest <= LONGEST_FRAME, format!("the main loop's frames: p99 {} (alone {}), longest {}", ms(p99), ms(alone_p99), ms(longest)));
    println!("RESULT: {}", if ok { "pass" } else { "fail" });
    Ok(ok)
}

/// `nettest host`: an endpoint that answers every datagram, until the
/// program is ended.
pub fn host(reach: Reach) -> Result<()> {
    let quic = Quic::start(reach)?;
    println!("hosting ({reach:?}); on the other machine:\n  voxcade nettest join {}", quic.ticket());
    loop {
        for e in quic.drain() {
            match e {
                Event::Datagram { peer, mut bytes, .. } => {
                    if bytes.first() == Some(&UP) {
                        bytes[0] = ECHO;
                    }
                    quic.send(peer, bytes);
                }
                Event::Joined { peer, .. } => println!("machine {peer} joined"),
                Event::Path { peer, relay, rtt, max_datagram } => println!("machine {peer}: {} ({:.1} ms, datagrams up to {max_datagram:?})", if relay { "through the relay" } else { "directly" }, rtt.as_secs_f64() * 1000.0),
                Event::Gone { peer, why } => println!("machine {peer} gone: {why}"),
                Event::Trouble(t) => eprintln!("{t}"),
                Event::Code(_) | Event::Found(_) | Event::Rooms(_) | Event::Refused(_) => {}
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// `nettest join TICKET`: `seconds` of datagrams at 60 Hz to a host and
/// back; the way one way is half the round trip.
pub fn join(ticket: &str, reach: Reach, seconds: u32, size: usize) -> Result<bool> {
    let quic = Quic::start(reach)?;
    quic.connect(ticket)?;
    let mut path = None;
    if !wait(&[&quic], Duration::from_secs(30), |_, e| matches!(e, Event::Joined { .. })) {
        bail!("the host did not answer within 30 s");
    }
    let clock = Instant::now();
    let mut back = Way::default();
    let mut next = Instant::now();
    for seq in 0..seconds * 60 {
        quic.send(0, datagram(UP, 0, seq, clock.elapsed().as_nanos() as u64, size));
        back.sent += 1;
        for e in quic.drain() {
            match e {
                Event::Datagram { bytes, at, .. } => {
                    if let Some((ECHO, _, _, sent)) = read(&bytes) {
                        back.delays.push(Duration::from_nanos((at.duration_since(clock).as_nanos() as u64).saturating_sub(sent)) / 2);
                    }
                }
                Event::Path { relay, rtt, max_datagram, .. } => path = Some((relay, rtt, max_datagram)),
                Event::Gone { why, .. } => bail!("the host went: {why}"),
                _ => {}
            }
        }
        next += TICK;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
    std::thread::sleep(Duration::from_millis(500));
    for e in quic.drain() {
        if let Event::Datagram { bytes, at, .. } = e
            && let Some((ECHO, _, _, sent)) = read(&bytes)
        {
            back.delays.push(Duration::from_nanos((at.duration_since(clock).as_nanos() as u64).saturating_sub(sent)) / 2);
        }
    }
    let relay = path.is_none_or(|p: (bool, Duration, Option<usize>)| p.0);
    let ms = |d: Duration| format!("{:.1} ms", d.as_secs_f64() * 1000.0);
    if let Some((relay, rtt, max)) = path {
        println!("path: {} ({}, datagrams up to {max:?})", if relay { "through the relay" } else { "directly" }, ms(rtt));
    }
    let most = if relay { MOST_P99_RELAYED } else { MOST_P99_DIRECT };
    let ok = line(back.loss() <= MOST_LOSS && back.at(0.99) <= most, format!("one way (half the round trip): {} of {} back ({:.2} % lost), p50 {}, p99 {}, most {}", back.delays.len(), back.sent, back.loss() * 100.0, ms(back.at(0.5)), ms(back.at(0.99)), ms(back.at(1.0))));
    println!("RESULT: {}", if ok { "pass" } else { "fail" });
    Ok(ok)
}

/// `nettest`'s word for a reach.
pub fn reach_named(name: &str) -> Result<Reach> {
    Ok(match name {
        "internet" => Reach::Internet,
        "relayed" => Reach::Relayed,
        "direct" => Reach::Direct,
        other => bail!("{other}: internet, relayed or direct"),
    })
}
