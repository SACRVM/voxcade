//! The protocol on a simulated network with a stub game: a hash of every
//! bundle played, kept after every tick. Every tick two machines played
//! must have the same hash on both, whatever the network did.

use super::soak::*;
use super::wire::{self, Bundle, Down, Hello, Kind, SeatEvent, Up};
use super::*;
use crate::hash::mix;
use crate::input::Input;

/// The host's bundles, seat by seat: how often an action was pressed (held
/// in a tick and not in the one before).
fn presses(log: &[Bundle], seat: usize, action: u32) -> u64 {
    let mut before = false;
    let mut n = 0;
    for b in log {
        let now = b.inputs.get(seat).copied().flatten().is_some_and(|i| i.actions & action != 0);
        n += (now && !before) as u64;
        before = now;
    }
    n
}


/// On a home network two to four machines play the same ticks, every
/// guest's figure goes on (none stops for want of its keys), and nothing
/// sent is too big.
#[test]
fn machines_on_a_lan_play_the_same() {
    for seed in 0..16 {
        let guests = 1 + seed as usize % 3;
        let mut run = Run::new(seed, Link::lan(), guests);
        run.to(20_000_000);
        let played = run.agree();
        let host_ticks = run.host().next_tick();
        assert!(host_ticks >= 1150, "seed {seed}: the host played {host_ticks}");
        for (n, &p) in played.iter().enumerate() {
            assert!(p + 30 >= host_ticks, "seed {seed}: guest {} played {p} of {host_ticks}", n + 1);
            assert_eq!(run.guest(n + 1).mine().len(), 1, "seed {seed}: guest {} has its seat", n + 1);
        }
        assert_eq!(run.host().stopped(), 0, "seed {seed}: no figure stopped");
        assert!(run.net.largest <= MAX_DATAGRAM && run.net.too_big == 0, "seed {seed}: {} bytes", run.net.largest);
        assert!(run.host().delay() >= 2 && run.host().delay() <= 3, "seed {seed}: the host's keys held back {}", run.host().delay());
    }
}

/// On a bad network (50 ms, a tenth lost and a tenth doubled, out of
/// order) the machines still play the same ticks: holes are filled,
/// nothing is played twice.
#[test]
fn machines_on_a_bad_network_play_the_same() {
    for seed in 0..8 {
        let mut run = Run::new(100 + seed, Link::bad(), 2);
        run.to(15_000_000);
        let played = run.agree();
        let host_ticks = run.host().next_tick();
        for (n, &p) in played.iter().enumerate() {
            assert_eq!(run.guest(n + 1).gone(), None, "seed {seed}");
            assert!(p + 120 >= host_ticks, "seed {seed}: guest {} played {p} of {host_ticks}", n + 1);
        }
    }
}

/// A key pressed for one frame, shorter than a tick, reaches a tick every
/// time: no tap is lost, on a guest playing at 144 frames a second.
#[test]
fn no_tap_is_lost() {
    for seed in 0..4 {
        let mut run = Run::new(200 + seed, Link::lan(), 2);
        run.nodes[2].taps = true;
        run.to(12_000_000);
        run.agree();
        let tapped = run.nodes[2].tapped;
        // The taps of the last moments are not in a tick yet.
        let seat = run.guest(2).mine()[0].0 as usize - 1;
        let pressed = presses(&run.nodes[0].game.as_ref().unwrap().log, seat, 4);
        assert!(tapped > 40 && pressed + 1 >= tapped && pressed <= tapped, "seed {seed}: {tapped} tapped, {pressed} pressed");
    }
}

/// A gap of three seconds makes nobody leave: the guest's seat stays, its
/// figure holds nothing meanwhile, and it catches up after.
#[test]
fn a_gap_never_makes_a_seat_leave() {
    let mut run = Run::new(300, Link::lan(), 1);
    run.to(5_000_000);
    run.net.cut(0, 1, true);
    run.to(8_000_000);
    run.net.cut(0, 1, false);
    run.to(14_000_000);
    let played = run.agree();
    let log = &run.nodes[0].game.as_ref().unwrap().log;
    assert!(!log.iter().any(|b| b.seats.iter().any(|e| matches!(e, SeatEvent::Left { .. }))), "nobody left");
    assert!(run.host().stopped() > 100, "its figure stopped meanwhile: {}", run.host().stopped());
    assert!(played[0] + 30 >= run.host().next_tick(), "caught up: {} of {}", played[0], run.host().next_tick());
}

/// Ten seconds of silence and the guest is gone: its seat leaves at a
/// tick, and the guest hears the host no more; nothing hangs.
#[test]
fn ten_seconds_of_silence_end_it() {
    let mut run = Run::new(400, Link::lan(), 1);
    run.to(4_000_000);
    run.net.cut(0, 1, true);
    run.to(16_000_000);
    let log = &run.nodes[0].game.as_ref().unwrap().log;
    assert!(log.iter().any(|b| b.seats.contains(&SeatEvent::Left { seat: 2 })), "its seat left");
    assert_eq!(run.guest(1).gone(), Some(&Gone::HostLeft));
    let played = run.nodes[1].game.as_ref().unwrap().after.len();
    run.to(17_000_000);
    assert_eq!(run.nodes[1].game.as_ref().unwrap().after.len(), played, "a guest gone plays no more");
    run.agree();
}

/// The host's farewell ends the game at once for its guests.
#[test]
fn the_host_leaving_ends_it() {
    let mut run = Run::new(450, Link::lan(), 2);
    run.to(3_000_000);
    let Role::Host(h) = &run.nodes[0].role else { unreachable!() };
    for _ in 0..10 {
        for (to, bytes) in h.farewell() {
            run.net.send(0, to, bytes);
        }
    }
    run.nodes[0].off = true;
    run.to(3_200_000);
    assert_eq!(run.guest(1).gone(), Some(&Gone::HostLeft));
    assert_eq!(run.guest(2).gone(), Some(&Gone::HostLeft));
}

/// A guest coming late gets the run so far in pieces (300 kB over a bad
/// network), then plays on with the others from the tick it was let in.
#[test]
fn a_guest_coming_late_gets_the_run_so_far() {
    let mut run = Run::new(500, Link::bad(), 2);
    run.nodes[0].pad = 300_000;
    run.nodes[1].pad = 300_000;
    run.nodes[2].pad = 300_000;
    run.nodes[2].join_at = 6_000_000;
    run.to(20_000_000);
    let played = run.agree();
    let late = run.nodes[2].game.as_ref().expect("it came in");
    assert!(late.first >= 240, "came in late, at tick {}", late.first);
    assert!(played[1] + 60 + late.first >= run.host().next_tick(), "and plays on: {} from {}", played[1], late.first);
}

/// Guests whose clocks run half a percent fast or slow keep pace: their
/// hands stay small and they never leave.
#[test]
fn drifting_clocks_keep_pace() {
    let mut run = Run::new(600, Link::lan(), 2);
    run.nodes[1].drift_ppm = 5_000;
    run.nodes[2].drift_ppm = -5_000;
    run.nodes[2].frame_us = 16_667;
    run.to(60_000_000);
    let played = run.agree();
    for n in 1..=2 {
        assert_eq!(run.guest(n).gone(), None);
        assert!(run.guest(n).in_hand() <= 6 && run.guest(n).target() <= 3, "guest {n}: {} in hand, aiming at {}", run.guest(n).in_hand(), run.guest(n).target());
        assert!(played[n - 1] + 30 >= run.host().next_tick(), "guest {n} played {} of {}", played[n - 1], run.host().next_tick());
    }
}

/// A reading of a seat's former generation (a player gone, its packets
/// still on the way) steers nobody; the seat's own does.
#[test]
fn a_former_generation_steers_nobody() {
    let mut host: Host<usize> = Host::new(7, "host", 2);
    let hello = Hello { id: [9; 16], nonce: 1, name: "anna".into(), commit: "x".into() };
    host.receive(0, 1, &hello.write());
    let Some(Notice::Joining { index, .. }) = host.notices().pop() else { panic!("asks to join") };
    host.admit(index, Vec::new(), Vec::new());
    let up = |asks: Vec<wire::Ask>, readings: Vec<wire::Readings>, from: u32| Up { got: 0, asks_from: from, asks, readings, ..Up::default() }.write(7);
    host.receive(1, 1, &up(vec![wire::Ask::Take], Vec::new(), 0));
    let b = host.tick(Vec::new()).unwrap();
    let SeatEvent::Taken { generation, .. } = b.seats[0] else { panic!("seat 2 taken: {b:?}") };
    let right = Input { right: true, ..Input::default() };
    let old = wire::Readings { seat: 2, generation: generation - 1, first: 0, inputs: vec![right] };
    host.receive(2, 1, &up(Vec::new(), vec![old], 1));
    assert_eq!(host.tick(Vec::new()).unwrap().inputs[1], Some(Input::default()), "the old generation's keys steer nobody");
    let own = wire::Readings { seat: 2, generation, first: 0, inputs: vec![right] };
    host.receive(3, 1, &up(Vec::new(), vec![own], 1));
    assert_eq!(host.tick(Vec::new()).unwrap().inputs[1], Some(right));
}

/// Every message reads back as written; random bytes and every message
/// cut short or with a byte changed are dropped, never a panic, on either
/// side.
#[test]
fn broken_datagrams_are_dropped() {
    let b = Bundle { tick: 99, inputs: vec![Some(Input { left: true, actions: 5, ..Input::default() }), None, Some(Input::default())], seats: vec![SeatEvent::Taken { seat: 3, owner: 2, generation: 4, ask: 1 }, SeatEvent::Left { seat: 2 }], controls: vec![(1, b"pad".to_vec())], extra: vec![1, 2, 3] };
    let down = Down { stamp: 5, asks_done: 2, acked: vec![(3, 77)], paused: Some(1), roster: Some(vec![wire::Member { index: 1, name: "anna".into(), rtt_ms: 4, quiet: false }]), apart: Some(wire::Apart { tick: 40, part: "level.state".into(), inputs: false }), said: Some("DESYNC".into()), bundles: vec![b.clone(), b.clone()] };
    let (bytes, n) = wire::down(&down, 11, &[&wire::bundle_bytes(&b), &wire::bundle_bytes(&b)], wire::MAX_DATAGRAM);
    assert_eq!(n, 2);
    let (kind, key, mut r) = wire::open(&bytes).unwrap();
    assert_eq!((kind, key), (Kind::Down, 11));
    assert_eq!(Down::read(&mut r).unwrap(), down);
    let up = Up { echo: 3, held: 2, got: 40, mask: 0b101, hand: 2, readings: vec![wire::Readings { seat: 2, generation: 1, first: 9, inputs: vec![Input::default(); 3] }], asks_from: 4, asks: vec![wire::Ask::Take, wire::Ask::Leave(2), wire::Ask::Controls(2, vec![7; 20])], checks: vec![(40, vec![1, 2, 3])] };
    let up_bytes = up.write(11);
    let (_, _, mut r) = wire::open(&up_bytes).unwrap();
    assert_eq!(Up::read(&mut r).unwrap(), up);
    let hello = Hello { id: [1; 16], nonce: 5, name: "anna".into(), commit: "2cb5d76".into() }.write();
    let welcome = wire::Welcome { index: 1, first: 30, seats: 4, blob: 5000, us_per_tick: 80, info: vec![1; 10] }.write(11);
    let piece = wire::Piece { blob: 1, index: 2, count: 3, data: vec![1; 20] }.write(11);
    let samples = [bytes, up_bytes, hello, welcome, piece, wire::refused("no"), wire::bye(11)];
    let mut host: Host<usize> = Host::new(11, "host", 4);
    let mut guest: Guest<usize> = Guest::new(0, [2; 16], 3, "g", "x", 0);
    let mut seed = 1u64;
    for round in 0..20_000u64 {
        seed = mix(seed + round);
        let mut d = samples[(seed % samples.len() as u64) as usize].clone();
        match seed >> 8 & 3 {
            0 => d.truncate((seed >> 16) as usize % d.len()),
            1 => {
                let at = (seed >> 16) as usize % d.len();
                d[at] ^= (seed >> 32) as u8 | 1;
            }
            2 => d = (0..(seed >> 16) % 1300).map(|n| mix(seed ^ n) as u8).collect(),
            _ => d.extend((0..(seed >> 16) % 40).map(|n| n as u8)),
        }
        host.receive(round, (seed >> 40) as usize % 3, &d);
        guest.receive(round, 0, &d);
        let _ = host.outgoing(round);
        let _ = guest.outgoing(round);
    }
}

/// The host's loop standing still (its pause page) pauses every machine:
/// the guests are told who paused, run dry without raising their hands
/// or leaving, and play on when the host does.
#[test]
fn the_hosts_pause_pauses_everyone() {
    let mut run = Run::new(700, Link::lan(), 2);
    run.to(4_000_000);
    let before = run.host().next_tick();
    run.nodes[0].off = true;
    if let Role::Host(h) = &mut run.nodes[0].role {
        h.set_paused(Some(0));
    }
    // Its loop stands still but it still speaks.
    for step in 0..6000u64 {
        let now = 4_000_000 + step * 500;
        run.to(now);
        if step % 100 == 0 {
            let local = run.nodes[0].local(run.now);
            if let Role::Host(h) = &mut run.nodes[0].role {
                for (to, bytes) in h.outgoing(local) {
                    run.net.send(0, to, bytes);
                }
            }
        }
    }
    assert_eq!(run.host().next_tick(), before, "no tick while paused");
    for n in 1..=2 {
        assert_eq!((run.guest(n).paused(), run.guest(n).gone(), run.guest(n).target()), (Some(0), None, 1), "guest {n}");
    }
    run.nodes[0].off = false;
    run.nodes[0].last = None;
    if let Role::Host(h) = &mut run.nodes[0].role {
        h.set_paused(None);
    }
    run.to(10_000_000);
    let played = run.agree();
    assert!(played.iter().all(|&p| p + 30 >= run.host().next_tick()), "on again: {played:?} of {}", run.host().next_tick());
}

/// The soak, two seeds (the Builds workflow runs `voxcade soak 1000` on
/// every system and compares the digests).
#[test]
fn the_soak() {
    assert_eq!(super::soak::soak(2), super::soak::soak(2), "the same every time");
}
