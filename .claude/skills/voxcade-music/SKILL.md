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

Outside a game folder, `voxcade mcp --music` is an MCP server of four
tools for exactly this: `music_words` (the language, every instrument
word, the voices, an example), `music_check` (the parser's mistake with
its line, or the song's facts), `music_render` (a WAV; tell the user its
path) and `music_picture` (a spectrogram PNG you can look at).

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
  they do not fit before the next vowel; a plosive after s or sh is
  unaspirated; a v (z) is Klatt's voiced fricative: a voice bar and a
  weak hiss, not a vowel; b, d, g release a short hiss before their
  voicing, a g's longest and compact (so grau is not heard as blau), a
  voice bar only after a voiced sound; `singer`'s r is the German r
  (uvular, its F3 high, a light rasp before a vowel, never more open
  than the vowel after it, its voice alone in a coda, the vowel before
  it turning straight towards it), `voice`'s the
  English one; a b, d, g after a nasal of its place closes briefly; nothing steps (the voicing set at
  a pulse's start, hisses ramped over 5 ms, the nasal zero gliding).
  Checked by `the_singer_sings_every_consonant_of_a_cluster` and
  `the_singer_tells_g_from_b_and_r_from_l`; sources in `clear.rs`'s
  comments (Klatt 1980, 1987 and the like).
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
megaphone's band still takes about 12 dB off an s; `drive_end` slides
it there on every note, evenly in dB over `drive_time`, attack + decay
unless set, the clipper then working the whole note, 0 dB a gentle
rounding: `drive=18 drive_end=3` a kick hard on the strike, clean in its
tail) and the bit crusher `crush_rate` (Hz the
sound is held at, 6300 the speech of old games) and `crush_bits` (1 to
16, after the envelope; 4 grainy). `unison` (1 to 7, a square, saw,
triangle or sine only) plays so many oscillators together, spread evenly
from `detune` semitones (0 to 1, 0.1) below the note to as far above, as
loud as one; the envelope, filters and drive stay one for them all.
An LFO on every line, `+` and `=` too: `lfo_cutoff` (semitones up and
down, 0 to 60, the line needs a filter), `lfo_volume` (down only, 0 to
1), `lfo_speed` (Hz from each note's start, 5) or `lfo_beats` (a turn in
so many of the song's beats, 1/16 to 64, on the beat and with the
tempo; not both), `lfo_wave` sine, triangle, square, saw (falling) or
noise. A wobble bass: `saw filter=lowpass cutoff=400 resonance=0.5
lfo_beats=0.5 lfo_cutoff=24`. Checking a voice: render a copy of
the song with only its channel, then its formants (LPC on the vowels),
its noise bands, the levels of its syllables and its steps at the joins;
the voice peaks near 1 before its volume, a driven one at about 0.4 of
its peak in RMS.

Drums from layers: lines beginning with `+` right under an instrument
are more layers of it (four with its own line at most; any instrument
word but `sidechain`, `sidechain_release`, `duck`, which its own line
says for the whole note), every note sounding them all, each with its
own envelope, over when the last has rung out; one line beginning with
`=` after them is what their sum goes through: `drive` (with
`drive_end`, `drive_time`), `filter`,
`cutoff`, `resonance`, the crusher, `volume` (1 unless set). No
layers on a voice. `pitch_start` (Hz) begins each note there, falling
exponentially in Hz to the note's pitch, `pitch_decay` its time
constant (0.02); not with `sweep`. `curve=exp` makes `decay` and
`release` fall exponentially, their time the fall to -60 dB (so
-20 dB at a third of it; a release equal to the decay goes on as the
decay went). A bass drum so: a sub, a body, a click, `= drive=3`.
Check a kick with the owner's three meters (KNALL, BALLERN, BRUSTKORB;
`ide::instrument::kick_meters`) against their good zone.

## Samples

Since 2026-11-23 (the owner: "wir werden samples erlauben") a song's
instrument may play a recording: the wave `sample`, a WAV in the game's
`samples/` (only WAV: PCM 16 or 24 bit, mono or stereo, 22050 to 48000
Hz; 10 s a sample at most, 64 MB all of a game's; mixed to one channel
and converted to 44100 Hz once as the game loads; anything else a plain
mistake naming the file). Their rights are their maker's, the engine
asks nothing ("lizenzierung - nicht unser thema"). Sound effects stay
synthetic: a `.wav` in `sounds/` is still a mistake.

```
@break sample name=amen root=c4 start=0 end=1.92 loop=yes
@stab  sample name=vox_yeah attack=0.002 release=0.05 crush_rate=12000 crush_bits=12
@kick  sine freq=50 pitch_start=100 decay=0.6 sustain=0 curve=exp
  + sample name=thump volume=0.5
```

`name` its file's name without `.wav` (a name not there: a mistake with
its line and the nearest name, as the game loads for a studio, when the
song plays otherwise); `root` the note it plays at its own speed (`c4`;
higher is faster and shorter, no time stretch, cubic between frames);
`start`, `end` the seconds of it that play; `loop=yes` that stretch round
while the note is held (its last 5 ms faded into its first at the seam);
`reverse=yes`. The envelope, filters, `drive`, the crusher and the volume
follow as on any wave; `sweep`, `pitch_start`, `vibrato`, `arp` and
`freq` move its speed; `duty`, `pwm`, `pwm_speed` are refused. An
unlooped note ends with its envelope or when its stretch has played out
(its last 3 ms faded). `music:mix` turns `root` (a note, `"g3"`),
`start` and `["end"]` from the next note (cutting a break up in play);
`name`, `loop`, `reverse` do not turn. A sample saved anew is heard at
once (the playing song made anew from it); samples count for neither
replays nor the state hash. `voxcade music FILE.mml` alone finds the
`samples/` beside the `music/` folder it lies in, else beside the file.
`games/drums/samples/thump.wav` is the engine's own render of a
layered bass drum, the studio's `@thump`.

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
c.audio.music = { "music/sunny_hills.mml" }   -- the game's songs; its code plays them (music:play)
c.audio.music_volume = 0.4
c.audio.drive = 0          -- how hard the game's sound is pushed into the limiter, in decibels: 0 as it is, more makes it denser and louder; music you listen to for long tires sooner when dense (0 to 12)
c.audio.ceiling = -1       -- the loudest it may get, in dB true peak, after the drive (-6 to 0; -1 unless set)
c.title_screen.music = "music/storm.mml"      -- the engine's title's song (unset: the title is silent)
c.title_screen.music_at = 33                  -- the bar the engine title's song begins at (1 unless set): past an intro
```

A song begun at a bar: `music:play("storm", { at = 33 })` (counted from
1) begins it there, its beat 0 there, going round its `#loop` as
written; a bar past its end or after its loop is a mistake.

Music in a game comes from its code alone (the owner, 2026-11-24):
what plays is the song the code asked last with `music:play`, until it
asks another or `music:stop`; nothing plays before it asks (ask in
`on_level_start`), a level that asks nothing leaves what plays, and
there is no song of the player's to fall back to. Never a kit's: More
Blood's `main.lua` plays `move_on` in `on_level_start` and `end_boss`
while its gunship flies. A title level of the game's own asks its song
in its `on_level_start` too (`c.title_screen.music` is only the engine's
title's); a run asking the same song lets it play on, unbroken. A radio
(HELP's A CAR RADIO): `music:play` of the next song at once or
crossfaded; `music:pause()` and `music:resume()` hold it and go on.

Songs live in the game's own folder (copy them; a recipe's `out` can
write the copy).

The game's code picks a song of its own (`music:play("storm", "bar")`:
from the next bar, `"beat"`, `"now"`) and can go with its beats
(`on_beat`, `music.beat`, `music.bar`; `#tempo` and `#meter` in the song
say when they fall, so they count for the game's replays). The song then
keeps time with the game, held while it pauses. A level change's fade
holds no song: the clock goes on through it (beats in it are told at the
first tick after), so a song asked for again in the next level plays on
without a gap, at its level (a fade of the picture is no fade of the
music). Another song: the old ends as the new begins, at once, unless it
crossfades:
`music:play("shop", { from = "now", crossfade = 3 })` or
`c.audio.crossfade = 3` for every change fade the old out as the new
comes in, equal in power; the clock is the new song's from then.
Silence: `music:stop()` (at once, `"beat"`, `"bar"`, or `{ from = "bar",
fade = 2 }`): no song of the game's, no beats; a play after it begins its
song from beat 0; a play asked while a stop is pending replaces it.

Music the game writes itself (a drum machine, a rhythm game's patterns,
a seeded score): a file of `c.audio.music` with only `@instruments` and
`#settings` and no channel is a studio, no song of its own (the engine's
title and `voxcade music GAME` skip it; one that does not
read, or has a `#loop`, is a mistake as the game loads). The game lays
channels over it: `music:play("kit", { channels = { K = "@kick o4 l16 c
r r r c r r r c r r r c r r r", H = "..." }, from = "bar" })`, the whole
set, a letter and its MML each; every channel fills as many whole bars
(counted exactly), no `t`, no singing voice (stage 1). Asked again while
the studio plays, only the channels change, the clock going on: "now"
for a step edited (it sounds when the music comes to it), "bar" for the
next pattern; a channel the same plays on untouched, a changed one's
sounding note rings on until its next note, exactly on the sample (the
audio gets the change ahead with its beat). `music.position` is the
beats with their fraction (`math.floor(music.position * 4) % 16` the
sixteenth sounding). Ask when a step changes, not every tick. The
studio's whole text counts for replays; saved while channels are laid,
its instruments are read anew. HELP's "A DRUM MACHINE" and
`games/drums` are the examples; `voxcade music` cannot write a laid song
(it has no game): film a replay instead (`voxcade film`) and look at the
sound's spectrogram around the bar line.

How it sounds where the player is: `music:volume(level, seconds)` (0..1
of the song's own loudness, under the player's volume) and
`music:muffle(amount, seconds)` (a low-pass of 12 dB an octave over the
whole song, as through a wall: 0 open, 0.5 about 1.3 kHz, 1 300 Hz; the
game's sounds stay clear) glide there over the seconds (0.3 unless said;
0 at once, over 5 and 20 ms, so nothing clicks) and hold until changed,
from level to level, the song going on. One song through three scenes:
`music:volume(0.6) music:muffle(0.7)` outside the house, `(1)` and `(0)`
in the room, the muffle back up in the hall behind the door. Only heard
(no replay changes with it); a film of a replay carries it. Each channel
has a strip of its own: `music:mix("K", { volume = 0.6, mute = false,
pan = -0.5 }, 0.05)` (the fader 0..1, the mute, the pan a balance; a
key left out stays, `false` gives it back, `music:mix("K", false)` the
whole strip, `music:mix("K")` reads it back), held by letter through
every song and stop until given back: a level knob, a mute button. A
strip turns its instrument's words too, over the instrument's own for
the channel's notes: `music:mix("B", { cutoff = 900, resonance = 0.8 },
0.05)` (an acid bass's filter knob: the note's cutoff sweep goes as deep
from it). `cutoff`, `resonance`, `duty`, `pwm`, `vibrato`, `sustain` and
`duck` glide under a sounding note; `attack`, `decay`, `release`,
`freq` and the other times and rates, `lowpass`, `drive`, the crusher
and `sidechain`, `pitch_start` and `pitch_decay` count from the
channel's next note, a sample's `root`, `start` and `["end"]` too;
`wave`, `filter`, `arp`, `curve`, `consonants` and a sample's `name`,
`loop` and `reverse` do not turn (another sound). Of an instrument of layers a strip turns
the first layer, but `drive`, `cutoff` and `resonance` the sum where
its `=` line states them. A word a note's
instrument lacks does nothing on it: give the instrument a filter
(`filter=lowpass cutoff=8000`) for a cutoff knob to have something to
turn.

A tempo of the game's: `music.tempo = 140` plays its song at 140 beats a
minute from the next tick (1 to 999, kept to a thousandth; read back,
the song's own `#tempo` until set; `nil` gives each song its own again).
It holds for every song of the game's until set nil. Only the timing
changes: each sound keeps its length (a kick's decay is as long at 160
as at 120), pitch and a singer's vowels stay, sung consonants go with
the tempo (listen to a sung line at another tempo). `music.beat`,
`on_beat` and `"bar"` go with it, exactly (the clock counts in integers
once a tempo is set; unset it is the old formula, so replays stay). A
song played much faster may cut a release that rang out at its own
tempo, as the same song written faster would. Checked by
measurement in `lua::tests::the_music_sounds_as_the_game_asks`.

A loop of its own: `#loop 33 80` in a song plays it from its start once,
then bars 33 to 80 (counted from 1, both in, `#meter` beats each) round
and round, what lies after bar 80 never reached: a song made with 32
bars of intro and outro for mixing plays its middle in the game.
`#loop 33` goes round from bar 33 to the end, `#loop no` plays it once.
A note still held as bar 80 ends is let go there (as at a song's end),
its release ringing on into bar 33 beside the loop's first notes. The
game's beats count
on through the loop (`music.beat`, `on_beat`), as through a whole song
going round. `voxcade music FILE.mml` writes such a song to the loop's
end and round into its first four bars again, and says where the seam
is: look at the seam there (`spectro.py`), and let the user listen.

Layers by tension: `#layer D C 0.35` in a song puts channels in a layer
that sounds while the game's `music.intensity` (0..1, 1 unless set) is at
least 0.35, fading in and out over a bar. `compose.py` writes them with
`adaptive = true` (every section has every part: drums from 0.3, broken
chords from 0.55, the melody from 0.8; pads and bass always); Pulsar's
song is one (style `space`). Check a song's layers from a game's replay:
`voxcade film GAME REPLAY --size 640x360 --out x.mp4`, its sound bar by
bar. The game's tempo is `music.tempo`. Still planned
(`shots/reviews/music-plan.md`): stingers, sounds in the song's chord.

## Checking a song

```bash
cargo build --release        # first, after any change to the MML parser or synth
./target/release/voxcade music games/NAME --out shots/song.wav          # the game's first song
./target/release/voxcade music games/NAME/music/x.mml --out shots/x.wav --seconds 30
.claude/skills/voxcade-music/scripts/spectro.py shots/song.wav shots/song.png --section 15
```

`voxcade music` prints the length, the peak and warnings about channels
of different lengths (a bar too many on one line). A game's song it
writes as the game plays it (its music volume, drive and ceiling); a
song file alone at the engine's defaults, and it says so: the game may
play it louder and limited. `spectro.py` adds the
loudness per section (pass the length of 8 bars: 8 × 4 × 60 / tempo
seconds), the share above 4 kHz (harshness: the library's songs stay well
under 1 %), the DC offset, left against right, and the loop's seam. Then
read the spectrogram PNG: sections should differ in texture, the melody
should come and go, nothing should sit as a hard band high up.

Report what you measured and what you could not judge, and give the user
the command to listen (`voxcade music ... --out` writes a WAV; in the game
the keys 1..9 switch songs).
