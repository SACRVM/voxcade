//! Network play in lockstep on a LAN (`docs/multiplayer.md`, stage 4).
//! One machine hosts, the others are guests, in a star: every machine
//! plays the whole game, only what the players hold travels. The host
//! gathers every seat's input into each tick's bundle and sends it to
//! every guest; the guests play the bundles in order as they come. Nobody
//! waits for anyone: a seat whose keys are late repeats its last for a
//! moment, then holds nothing.
//!
//! The protocol is two state machines without any I/O (`Host`, `Guest`):
//! time and datagrams are handed in, datagrams and bundles come out. A
//! game's window drives them over a UDP socket, the tests over a network
//! simulated in one process (`SimNet`), as deterministic as a replay.
//! What a bundle means for the game (its events, the controls) is the
//! session's: the protocol carries it as bytes.

mod guest;
mod host;
pub mod link;
pub mod machine;
pub mod play;
pub mod quic;
mod simnet;
pub mod soak;
pub mod trial;
pub mod udp;
pub mod wire;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod session_tests;

pub use guest::{Gone, Guest};
pub use host::{Host, Notice};
pub use simnet::{Link, SimNet};
pub use wire::{Apart, Ask, Bundle, MAX_DATAGRAM, MAX_SEATS, Member, SeatEvent};

/// A tick's length in microseconds, as the simulation's 60 a second.
pub const TICK_US: u64 = 1_000_000 / 60;

/// The most ticks a guest plays in one frame after a hitch (as the window
/// does in a game of one).
pub const MAX_CATCH_UP: usize = 5;

/// The ticks a seat repeats its last input when none came (100 ms), then
/// it holds nothing.
pub const REPEAT: u32 = 6;

/// After this long without a word from a machine (µs) the others hear of
/// it (1 s), and after this long it is gone (10 s).
pub const QUIET: u64 = 1_000_000;
pub const SILENT: u64 = 10_000_000;

/// A machine with nothing else to send says it is there this often (µs),
/// so a Wi-Fi radio does not doze.
pub const HEARTBEAT: u64 = 50_000;

/// A machine's number in a network game: the host 0, the guests from 1 in
/// the order they came.
pub type Machine = u8;

/// A machine's own id, a random number kept in its config folder: a guest
/// coming back is known by it, whatever its address now.
pub type MachineId = [u8; 16];
