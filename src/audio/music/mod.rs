//! Music written as MML (Music Macro Language): plain text, one line of
//! notes per channel, played by SID-like voices (pulse with width
//! modulation, saw, triangle, sine, noise; ADSR, vibrato, sweeps, chords as
//! fast arpeggios, a low-pass filter). Text, so a person or an agent can
//! write it, and the game plays it again on every save.
//!
//! ```text
//! ; a comment
//! #title  Desert March
//! #tempo  140                      ; beats per minute
//! @lead   square duty=0.25 attack=0.005 decay=0.15 sustain=0.6 release=0.1 vibrato=0.012
//! @chord  square duty=0.5 arp=0,3,7 volume=0.5
//! @kick   triangle freq=110 sweep=-24 decay=0.12 sustain=0
//! $beat = @kick c8 @hat c8 @snare c8 @hat c8
//! A: @lead v12 o4 l8 d4. a a4 g f | [c d e f]2 >c2
//! D: [$beat]8
//! ```
//!
//! Channel commands: notes `c d e f g a b` (`+` or `#` sharp, `-` flat), a
//! length (1 whole, 4 quarter, 8 eighth ...) and dots; `r` rest; `&` ties
//! to the next note; `o4` octave, `<` `>` octave down and up; `l8` default
//! length; `v0`..`v15` volume; `q1`..`q8` how much of a note sounds (8
//! legato); `p-100`..`p100` pan; `t140` tempo; `@name` instrument (until
//! a channel's first one it plays the plain instrument: a square, duty
//! 0.5, every option at its default; no repo song leaves it out, a first
//! song typed by hand may);
//! `[ ... ]3` repeat; `$name` a macro; `|` bar lines, ignored. `#meter 3`
//! (beats in a bar, 4 unless set) is what a game that reads the song's
//! bars counts by (`music.bar`); its beats are `#tempo`'s, a `t` in a
//! channel changing nothing for them. `#layer D C 0.35` puts channels in a
//! layer: they sound while the game's tension (`music.intensity`, 1
//! unless a game sets it) is at least 0.35, fading in and out over a bar;
//! a channel in no layer always sounds.
//!
//! Instrument options: a wave (`square saw triangle sine noise`), `duty`,
//! `pwm` and `pwm_speed` (pulse width wobble), `attack` `decay` `sustain`
//! (a level 0..1) `release`, `vibrato` `vibrato_speed` `vibrato_delay`,
//! `arp=0,4,7` and `arp_speed` (Hz), `sweep` (semitones over
//! `sweep_time`), `freq` (a fixed pitch, for drums), `lowpass` (a gentle
//! fixed one), `volume`.
//!
//! The SID's filter: `filter=lowpass|bandpass|highpass` (12 dB per octave),
//! `cutoff` (Hz), `resonance` (0..1; it overdrives softly instead of
//! spiking), and `cutoff_end` with `cutoff_time` to sweep the cutoff on
//! every note, the way C64 players run their filter tables (acid: open
//! wide, close fast, lots of resonance). Any of these turns the filter on,
//! as a low-pass unless `filter` says otherwise.
//!
//! Sidechain: an instrument with `sidechain=0.6` ducks the other channels
//! by that much whenever one of its notes starts (a kick pumping the bass
//! and pads), and they come back over `sidechain_release` seconds. `duck`
//! (0..1, default 1) is how far an instrument follows; 0 leaves it alone.
//!
//! A game's own say over its song as heard (`music:volume`,
//! `music:muffle`): a part of its loudness and a low-pass over the whole
//! song, as through a wall (`Mix`), each gliding to what the game asked.

mod mix;
mod parse;
mod player;
mod song;
#[cfg(test)]
mod tests;

pub use mix::{Glide, Mix, muffle_cutoff};
pub use parse::parse;
pub use player::Player;
pub use song::{FilterMode, Instrument, Song, Timing};

use super::synth::{SAMPLE_RATE, Wave};
use super::words;
