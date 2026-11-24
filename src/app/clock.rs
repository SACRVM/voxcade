//! The time a window owes its game's simulation, tick by tick: the
//! window's and the IDE's PLAY alike, so the game's own song keeps with the
//! time it is heard in.

use crate::sim::DT;

/// Ticks a frame may play to catch up: past them the rest waits for the
/// next frames (a game whose own song keeps time with it), or is dropped.
pub const MAX_CATCH_UP: u32 = 5;

/// The most of a frame's time slides, the title's clock and the like take:
/// a long frame is a quarter of a second to them.
const LONGEST_FRAME: f32 = 0.25;

/// The most time owed while the game's own song keeps time with the
/// game's clock (`Session::steers_music`): the song plays on through a
/// frame the window was held up in (a model read, the first pictures'
/// shaders, the system), and the clock catches up with it over the next
/// frames, `MAX_CATCH_UP` ticks a frame, instead of the song jumping back
/// to a clock that dropped the time. Past it the time is dropped and the
/// song jumps (a window held for seconds).
pub const MOST_OWED: f32 = 2.0;

/// The simulation's time owed: what is left of the frames' time after the
/// ticks it paid for.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickClock {
    owed: f32,
}

impl TickClock {
    /// A frame's time added, `elapsed` seconds since the last: whole up to
    /// `MOST_OWED` where the game's own song keeps time with it (`keep`),
    /// else at most a quarter of a second. What the frame's slides and
    /// clocks take: at most a quarter of a second.
    pub fn frame(&mut self, elapsed: f32, keep: bool) -> f32 {
        let elapsed = elapsed.max(0.0);
        self.owed = if keep { (self.owed + elapsed).min(MOST_OWED) } else { self.owed + elapsed.min(LONGEST_FRAME) };
        elapsed.min(LONGEST_FRAME)
    }

    /// The ticks due now, taken off what is owed: at most `MAX_CATCH_UP`;
    /// where they did not pay for it all, the rest waits for the next frame
    /// (`keep`) or is dropped.
    pub fn due(&mut self, keep: bool) -> u32 {
        let mut ticks = 0;
        while self.owed >= DT && ticks < MAX_CATCH_UP {
            self.owed -= DT;
            ticks += 1;
        }
        if ticks == MAX_CATCH_UP && !keep {
            self.owed = 0.0;
        }
        ticks
    }

    /// How far the simulation's clock lies behind the time heard,
    /// `since` seconds after the last frame's time was added: what the
    /// game's own song is to be at, past the clock (`Session::hand_music`).
    pub fn behind(&self, since: f32) -> f32 {
        (self.owed + since.max(0.0)).min(MOST_OWED)
    }

    /// How far the picture is between the last tick and the next (0 to 1).
    pub fn fraction(&self) -> f32 {
        (self.owed / DT).min(1.0)
    }

    /// Nothing owed: a song begun now at the clock as it stands, the time
    /// before it (a game read, a level built) not the song's.
    pub fn settle(&mut self) {
        self.owed = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame held up is caught up over the next frames where the game's
    /// own song keeps time, `MAX_CATCH_UP` ticks a frame, and dropped
    /// otherwise, as before.
    #[test]
    fn a_long_frame_is_caught_up_where_the_song_keeps_time() {
        for keep in [false, true] {
            let mut c = TickClock::default();
            let mut ticks = 0;
            assert_eq!(c.frame(0.5, keep), 0.25, "slides take a quarter of a second at most");
            ticks += c.due(keep);
            for _ in 0..40 {
                c.frame(DT, keep);
                ticks += c.due(keep);
            }
            let seconds = ticks as f32 * DT;
            if keep {
                assert!((seconds - (0.5 + 40.0 * DT)).abs() < DT, "{seconds}: every moment played");
            } else {
                assert!(seconds < 0.5 + 40.0 * DT - 0.3, "{seconds}: the time past the catch-up dropped");
            }
        }
        let mut c = TickClock::default();
        c.frame(5.0, true);
        assert_eq!(c.behind(0.0), MOST_OWED, "seconds held up: the rest dropped");
    }
}
