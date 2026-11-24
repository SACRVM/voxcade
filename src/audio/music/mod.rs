//! Music written as MML (Music Macro Language): plain text, one line of
//! notes per channel, played by SID-like voices (pulse with width
//! modulation, saw, triangle, sine, noise; ADSR, vibrato, sweeps, chords as
//! fast arpeggios, a low-pass filter) and a voice that sings. Text, so a
//! person or an agent can write it, and the game plays it again on every
//! save.
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
//! A song goes round whole when it ends, unless `#loop` says otherwise:
//! `#loop 33 80` plays it from its start once, then bars 33 to 80 (counted
//! from 1, both in, `#meter` beats of `#tempo` each) round and round, what
//! lies after bar 80 never reached (an outro for mixing); `#loop 33` goes
//! round from bar 33 to the song's end; `#loop no` plays it once. A note
//! still sounding as the loop's last bar ends is cut there, as at a song's
//! end. A game's clock counts on through the loop as through a whole
//! song going round (`music.beat`, `music.bar`).
//!
//! Instrument options: a wave (`square saw triangle sine noise`), `duty`,
//! `pwm` and `pwm_speed` (pulse width wobble), `attack` `decay` `sustain`
//! (a level 0..1) `release`, `vibrato` `vibrato_speed` `vibrato_delay`,
//! `arp=0,4,7` and `arp_speed` (Hz), `sweep` (semitones over
//! `sweep_time`), `freq` (a fixed pitch, for drums), `lowpass` (a gentle
//! fixed one), `volume`; for every wave `drive` (decibels into a soft
//! clipper, before the filters, as an amp before its speaker; 0 clean; a
//! voice's breath and hiss join past it) and
//! a bit crusher, `crush_rate` (Hz the sound is held at) and `crush_bits`
//! (1 to 16, after the envelope).
//!
//! Two voices sing: `@vox singer ...` the clear one, made for words to be
//! understood (`clear.rs`, a cascade/parallel formant synthesizer after
//! Klatt with a glottal pulse, formants gliding by their loci, voice onset
//! times), and `@vox voice ...` the first, rougher one (`voice.rs`, the
//! port of the owner's script, kept to the sample). A quoted text before a note is the syllable it sings,
//! phonemes split by spaces: onset consonants, one vowel or diphthong, coda
//! consonants (`V: @vox "l a" c8 "l ae" d8 "s ae ng" e4`). The vowel
//! starts on the beat, the onset before it, the coda ends where the note is
//! let go (`q`). A voice note without a text holds the vowel before (a
//! melisma: tied notes share it, the coda on the last); after a rest it
//! sings the vowel alone. The phonemes: vowels a ae e eh i ih o u ax er
//! oe ue, diphthongs aw ow ai oi ei, consonants m n ng l r w j f v s z sh ch x
//! h rr t k p d g b (x the German ach, rr a trilled r, er an unstressed
//! German -er, sung softer by `singer`). `consonants` (dB,
//! -24 to 12) moves a voice's hiss, bursts and breath against its vowels. `duty`, `pwm` and `pwm_speed` mean nothing on a voice and
//! are refused; the other words apply (the attack starts from where the
//! voice was, so a line never drops to silence between its syllables).
//!
//! The SID's filter: `filter=lowpass|bandpass|highpass` (12 dB per octave),
//! `cutoff` (Hz), `resonance` (0..1; it overdrives softly instead of
//! spiking), and `cutoff_end` with `cutoff_time` to sweep the cutoff on
//! every note, the way C64 players run their filter tables (acid: open
//! wide, close fast, lots of resonance). Any of these turns the filter on,
//! as a low-pass unless `filter` says otherwise.
//!
//! A pitch that falls from a start: `pitch_start` (Hz) each note begins
//! at, falling exponentially in Hz to the note's pitch with the time
//! constant `pitch_decay` (seconds, 0.02 unless set): f(t) = f + (start -
//! f) e^(-t / pitch_decay), a drum's thump; not with `sweep`. `curve=exp`
//! makes `decay` and `release` fall exponentially, their time the fall to
//! -60 dB (to the sustain level, to silence), the last 2 ms laid down to
//! the target in a straight line so it ends there without a step;
//! `linear` (unless set) the straight line of before.
//!
//! Layers: lines beginning with `+` right under an instrument's line (a
//! comment or a blank line may stand between) are further layers of it,
//! four with its own line at most, each a wave and any instrument word
//! but `sidechain`, `sidechain_release` and `duck` (the note's as a
//! whole, on its own line). Every note sounds all of them, each with its
//! own envelope, added at their volumes; it is over when the last has
//! rung out. One line beginning with `=` after them is what the sum goes
//! through: `drive` (the soft clipper, as an instrument's), `filter`,
//! `cutoff`, `resonance`, the crusher and `volume` (1 unless set), in
//! that order. A voice takes no layers.
//!
//! ```text
//! @kick sine freq=50 pitch_start=180 pitch_decay=0.022 curve=exp decay=0.35 sustain=0
//!   + sine freq=140 pitch_start=320 pitch_decay=0.012 curve=exp decay=0.09 drive=8 volume=0.5
//!   + noise filter=bandpass cutoff=2100 curve=exp decay=0.02 volume=0.6
//!   = drive=4
//! ```
//!
//! Samples: the wave `sample` plays a recording of the game's `samples/`
//! (`sample.rs`: WAV only, read by our own reader, converted once to the
//! engine's rate and one channel): `name` its file's without `.wav`,
//! `root` the note it plays at its own speed at (`c4`; other notes resample
//! it with a cubic, higher faster and shorter), `start` and `end` the
//! seconds of it that play, `loop=yes` that stretch round while the note
//! sounds (its last 5 ms faded into its first as it comes round),
//! `reverse=yes` backwards. An unlooped note ends when its envelope does
//! or its stretch has played out (its last 3 ms faded). The whole chain
//! of every wave follows (envelope, filters, drive, crusher, volume), and
//! `sweep`, `pitch_start`, `vibrato`, `arp` and `freq` move its speed;
//! the pulse's words are refused. A sample may be a layer. A song names
//! its samples as it is read; `Song::bind` finds them (a name not there a
//! mistake naming the line and the nearest), and until then it plays
//! nothing. `@break sample name=amen root=c4 start=0 end=1.92 loop=yes`.
//!
//! Sidechain: an instrument with `sidechain=0.6` ducks the other channels
//! by that much whenever one of its notes starts (a kick pumping the bass
//! and pads), and they come back over `sidechain_release` seconds. `duck`
//! (0..1, default 1) is how far an instrument follows; 0 leaves it alone.
//!
//! A game's own say over its song as heard (`music:volume`,
//! `music:muffle`): a part of its loudness and a low-pass over the whole
//! song, as through a wall (`Mix`), each gliding to what the game asked.

#[cfg(test)]
mod cut_tests;
#[cfg(test)]
mod loop_tests;
#[cfg(test)]
mod laid_tests;
mod mix;
mod parse;
mod player;
mod song;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tempo_tests;
#[cfg(test)]
mod knob_tests;
#[cfg(test)]
mod layer_tests;
mod voice;
pub mod shift;
mod clear;
pub mod sample;
#[cfg(test)]
mod sample_tests;

pub use mix::{GLIDING, Glide, LETTERS, Mix, NEXT_NOTE, NO_KNOB, SAMPLE_KNOBS, SHORTEST, Strip, letter_index, muffle_cutoff};
pub use parse::{Laid, has_channels, lay, note_hz, note_name, parse, read};
pub use player::{Digests, Followed, Player, Swap};
pub use sample::Samples;
pub use song::{FilterMode, Instrument, Sampled, Song, Timing};
pub use voice::phonemes;

use super::synth::{SAMPLE_RATE, Wave};
use super::words;
