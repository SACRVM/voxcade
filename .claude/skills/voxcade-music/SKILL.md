---
name: voxcade-music
description: >
  Music for Voxcade games: the MML text format the engine plays on its
  SID-like voices, making a background song from a recipe with
  tools/compose.py (styles calm, mysterious, dark, action, bright, space), the
  library in music/, bringing Suno songs in as MIDI or audio stems
  (tools/midi2mml.py, tools/audio2midi.py), wiring songs into a game
  (c.audio.music, the title's, songs the game's code picks and goes
  with: music:play, on_beat), and checking a song you cannot
  hear (WAV, spectrogram, DC offset, loop seam). Use this whenever a
  song, Musik, soundtrack, theme, a mood for a level, MML, a recipe,
  Suno, MIDI or stems come up, or the user says the music is too much,
  too loud, annoying or does not fit.
---

# Music in Voxcade

Music is text: MML in `.mml` files, which a person or an agent can write
and edit, and which the running game plays anew on every save. The
engine's voices are SID-like (pulse with width wobble, saw, triangle,
sine, noise; envelopes, vibrato, sweeps, chords as fast arpeggios, the
SID's filter, sidechain ducking). The format is described at the top of
`src/audio/music/mod.rs`.

You cannot listen. Everything you say about how a song sounds comes from
its structure, its numbers and its spectrogram; say so, and let the user
judge by ear.

## What the user wants from music

- Background that can run for half an hour without wearing on the ears.
  Pure C64 sound for that long "makes you want to run amok" (the user),
  and the songs made in Suno were much work. So simple songs come from
  chord progressions (`tools/compose.py`) with rules against fatigue: the
  melody in some sections only ("teils teils"), chords as soft pads (the
  trilling C64 arpeggio only a quiet accent in dark and action), every
  voice low-passed, pulse waves at duty 0.5, the texture changing every 8
  bars over two to three minutes before the song starts over.
- More Blood's mood, in the user's words: dark, menacing, atmospheric,
  C64 style; its four songs (Move On, Blood Rush, Storm Is Coming, End
  Boss) belong together. Older songs stay in the repo, out of the list.
- Pulse waves off duty 0.5 play round zero, in MML as in a sound file
  (no DC offset since 2026-11-13: no thumps, no lost headroom); a thin
  sound is still a choice of timbre. MML refuses a value out of its range
  (tempo 1 to 999, lengths 1 to 192, octaves 0 to 9) and names the line;
  HELP has the topic "A SONG IN MML".
- An instrument's words are one register with a sound file's
  (`src/audio/words.rs`): a word both know means the same, in the same
  unit (`sweep` in semitones, `attack` and `decay` in seconds). MML's
  `sustain` is a level; a sound's time at full volume is `hold`.

## A singing voice

An instrument sings with the wave `singer` (`src/audio/music/clear.rs`,
the clear voice, for words to be understood: a glottal pulse, five
formants, formants gliding from each consonant's locus, voice onset
times, a parallel branch for the hiss) or `voice` (`voice.rs`, the first,
rougher one, kept to the sample: good for short shouts). No samples. A
quoted text before a note
is the syllable that note sings, phonemes split by spaces, in small
letters: onset consonants, one vowel or diphthong, coda consonants.

```
@vox singer attack=0.005 decay=0 sustain=1 release=0.06 vibrato=0.006 vibrato_speed=5.5 vibrato_delay=0.1 volume=0.5
@shout voice attack=0.005 decay=0 sustain=1 release=0.05 drive=19 filter=bandpass cutoff=1150 volume=0.6
V: @vox o4 "l a" e8 "l a" g8 "s ae ng" a4 b4 q8 "h ei" e2 q7
```

- The vowel starts on the note's beat, the onset just before it (never
  before the note before), the coda ends where the note is let go (`q`:
  `q7`, the default, ends a syllable 26 ms before the next eighth at
  144; `q8` holds it to the end). A later consonant cuts into the vowel
  before it.
- A voice note without a text holds the vowel (a melisma): tied notes
  share it, the coda and a diphthong's glide on the last; after a rest
  it sings the vowel alone. A voice's first note needs a text.
- The phonemes (HELP's A SONG IN MML lists them, the mistake of an unknown
  one too): vowels a ae e eh i ih o u ax er oe ue, diphthongs aw ow ai oi
  ei, consonants m n ng l r w j f v s z sh ch x h rr t k p d g b (x the
  German ach, rr a trilled r, er the unstressed German -er: "sh v eh s"
  "t er"). Written in English or German sounds: "f
  ae n", "z ai n", "ih ch", "a x".
- `consonants` (dB, -24 to 12, 0 unless set) moves a voice's hiss,
  bursts and breath against its vowels, both voices. Unset they lie as
  in speech (sh about 13 dB under the vowel, s 15, f 20).
- Clusters (`singer`): a coda ends where the next syllable's onset
  begins, both compressed together down to each consonant's least where
  they do not fit before the next vowel; consonants in a cluster are a
  quarter shorter; a plosive after s or sh is unaspirated, one after a
  nasal of its place has a short closure; a v (z) is Klatt's voiced
  fricative: a voice bar and a weak hiss, not a vowel; a burst is a
  transient falling into the breath; nothing steps (the voicing set at
  a pulse's start, hisses ramped over 5 ms, the nasal zero gliding).
  Checked by `the_singer_sings_every_consonant_of_a_cluster` and
  `the_singers_bursts_fall_away_into_their_breath`; sources in
  `clear.rs`'s comments (Klatt 1980, 1987 and the like).
- `duty`, `pwm`, `pwm_speed` are refused on a voice; every other word
  applies. The attack starts from where the voice was, so a line does
  not drop to silence between syllables.
- An echo is a second voice channel with the same notes, a rest of the
  delay before (`r8.` a dotted eighth), quieter (`v4` against `v15`) and
  panned (`p31`); MML has no echo.

For every wave: `drive` (0 to 36 dB into tanh, before the filters, its
DC taken off again; 12 rough, 19 the shout through a megaphone with
`filter=bandpass cutoff=1150`; a voice's consonants join past the
clipper, so they stay as far under the vowels as without it: the
megaphone's band still takes about 12 dB off an s) and the bit crusher `crush_rate` (Hz the
sound is held at, 6300 the speech of old games) and `crush_bits` (1 to
16, after the envelope; 4 grainy). Checking a voice: render a copy of
the song with only its channel, then its formants (LPC on the vowels),
its noise bands, the levels of its syllables and its steps at the joins;
the voice peaks near 1 before its volume, a driven one at about 0.4 of
its peak in RMS.

## A song from a recipe

```bash
python3 tools/compose.py --find Joyful Playful        # progressions tagged with these moods
python3 tools/compose.py music/sunny_hills.compose.toml   # writes the song (and copies)
```

A recipe (`NAME.compose.toml`, see the library's for examples):

```toml
title = "Sunny Hills"
out = ["sunny_hills.mml", "../games/NAME/music/sunny_hills.mml"]   # the library and the game
style = "bright"          # calm, mysterious, dark, action, bright, space
key = "g"
tempo = 128
seed = 1                  # another seed: another song in the same mood
moods = ["Joyful", "Playful"]
scale = "major"           # major, minor, modal
melody = "some"           # none, some, most
variation = "some"        # none (unset), some, most: bars vary within a section
```

The same recipe always makes the same song. `variation` (Strudel's and
TidalCycles' lesson against fatigue: a rule that runs) varies bars within
a section over the same song: every fourth bar the bass walks to the next
chord and the drums play a soft pickup, broken chords start a beat later
each bar (action's stabs syncopate), later themes answer their first
phrase backwards; `most` adds seeded ghost notes on the hi-hat. It draws
its own random numbers, so the song under it stays the seed's, and a
recipe without it is unchanged. A new style is a new entry
in `STYLES` in `tools/compose.py` (instruments, bass patterns, drums,
melody range, rhythms of two bars = 32 sixteenths), keeping the fatigue
rules. The progressions are Ludovic Drolez's free-midi-chords (MIT,
`tools/progressions.toml` and its licence): keep the credit in
`music/README.md` and in each song's header.

The library `music/` holds the recipes and their songs, for the editor's
package later; add a new song's line to the table in `music/README.md`
(style, key, tempo, length, chords).

## A song from Suno

The user makes songs in Suno (Pro) and exports MIDI or audio stems.

- **Stems to MIDI first:** `python3 tools/audio2midi.py --bpm 120 --out
  games/NAME/music/SONG STEMS/*.mp3`. The tempo must be known (the zip's
  name often says it). Check the result by drawing its notes over the
  stem's spectrogram, not only by counting notes.
- **Leave out stems that only echo others** (doubles and bleed are typical
  of Suno; the user agrees): compare their onsets, pitch and rhythm, with
  a shifted copy as the chance level.
- **MIDI to MML:** an import file (`games/moreblood/music/end_boss.import.toml`
  is one) says which part goes to which channel, instrument, octave,
  volume; `python3 tools/midi2mml.py FILE.import.toml`. The melodies get
  a C64 echo instead of a doubled stem (`delay` on a second, quieter part):
  a third of the line's volume (two thirds sounded doubled to the user).

## Into a game

```lua
c.audio.music = { "music/sunny_hills.mml" }   -- one or more: keys 1..9 switch while playing
c.audio.music_volume = 0.4
c.audio.drive = 0          -- how hard the game's sound is pushed into the limiter, in decibels: 0 as it is, more makes it denser and louder; music you listen to for long tires sooner when dense (0 to 12)
c.title_screen.music = "music/storm.mml"      -- the title's own (default: the first song)
```

Which song plays when is the game's code, never a kit's: More Blood's
`main.lua` plays `end_boss` while its gunship flies (`music:play`) and
gives the player's song back after (`music:play()`).

Songs live in the game's own folder (copy them; a recipe's `out` can
write the copy). The title's settings page plays the chosen song.

The game's code picks a song of its own (`music:play("storm", "bar")`:
from the next bar, `"beat"`, `"now"`) and can go with its beats
(`on_beat`, `music.beat`, `music.bar`; `#tempo` and `#meter` in the song
say when they fall, so they count for the game's replays). The song then
keeps time with the game, held while it pauses. A level change's fade
holds no song: the clock goes on through it (beats in it are told at the
first tick after), so a song asked for again in the next level plays on
without a gap. Another song: the old ends as the new begins (the rest of
a level's fade silent, a short pause), unless it crossfades:
`music:play("shop", { from = "now", crossfade = 3 })` or
`c.audio.crossfade = 3` for every change fade the old out as the new
comes in, equal in power; the clock is the new song's from then. No
`music:stop` yet.

How it sounds where the player is: `music:volume(level, seconds)` (0..1
of the song's own loudness, under the player's volume) and
`music:muffle(amount, seconds)` (a low-pass of 12 dB an octave over the
whole song, as through a wall: 0 open, 0.5 about 1.3 kHz, 1 300 Hz; the
game's sounds stay clear) glide there over the seconds (0.3 unless said;
0 at once, over 5 and 20 ms, so nothing clicks) and hold until changed,
from level to level, the song going on. One song through three scenes:
`music:volume(0.6) music:muffle(0.7)` outside the house, `(1)` and `(0)`
in the room, the muffle back up in the hall behind the door. Only heard
(no replay changes with it); a film of a replay carries it. Checked by
measurement in `lua::tests::the_music_sounds_as_the_game_asks`.

Layers by tension: `#layer D C 0.35` in a song puts channels in a layer
that sounds while the game's `music.intensity` (0..1, 1 unless set) is at
least 0.35, fading in and out over a bar. `compose.py` writes them with
`adaptive = true` (every section has every part: drums from 0.3, broken
chords from 0.55, the melody from 0.8; pads and bass always); Pulsar's
song is one (style `space`). Check a song's layers from a game's replay:
`voxcade film GAME REPLAY --size 640x360 --out x.mp4`, its sound bar by
bar. Still planned (`shots/reviews/music-plan.md`): a tempo from the
game, stingers, sounds in the song's chord.

## Checking a song

```bash
cargo build --release        # first, after any change to the MML parser or synth
./target/release/voxcade music games/NAME --out shots/song.wav          # the game's first song
./target/release/voxcade music games/NAME/music/x.mml --out shots/x.wav --seconds 30
.claude/skills/voxcade-music/scripts/spectro.py shots/song.wav shots/song.png --section 15
```

`voxcade music` prints the length, the peak and warnings about channels
of different lengths (a bar too many on one line). `spectro.py` adds the
loudness per section (pass the length of 8 bars: 8 × 4 × 60 / tempo
seconds), the share above 4 kHz (harshness: the library's songs stay well
under 1 %), the DC offset, left against right, and the loop's seam. Then
read the spectrogram PNG: sections should differ in texture, the melody
should come and go, nothing should sit as a hard band high up.

Report what you measured and what you could not judge, and give the user
the command to listen (`voxcade music ... --out` writes a WAV; in the game
the keys 1..9 switch songs).
