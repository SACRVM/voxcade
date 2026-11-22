# Music library

Background songs for games, made from chord progressions by
`tools/compose.py`. Each song has a recipe, `NAME.compose.toml`, and the
MML it makes, `NAME.mml`. A game plays a song from its own folder (copy it
there, or name the copy in the recipe's `out`), with
`c.audio.music = "music/NAME.mml"` in its `conf.lua`.

The songs are made to be heard for long:
- the melody plays in some sections only;
- the chords are soft pads rather than the C64's trilling arpeggio, which
  stays a quiet accent in the dark and action songs;
- every voice is low-passed;
- the texture changes every 8 bars over two to three minutes, then the
  song starts over;
- with `variation` in the recipe, the bars within a section vary too.

| Song | Style | Key | Tempo | Length | Chords (A, B) |
|---|---|---|---|---|---|
| Crates at Dawn | calm | F major | 84 | 3:03 | F Am Dm Fsus4 / F Dm F Bb |
| Quiet Warehouse | calm | A minor | 76 | 3:22 | Am F C G / Fmaj7 G6 Am G6 |
| Under the Dunes | mysterious | E | 92 | 2:47 | E B Am C / Em D#m/E Em Fm/E |
| Night Patrol | dark | D minor (Phrygian) | 108 | 2:22 | Dm Eb F Eb / Bb Cm Dm Eb |
| Hot Pursuit | action | A minor | 144 | 2:13 | F G Em C / Am G C F |
| Pulsar Drive | space, in layers | D minor | 120 | 2:40 | Dm Bb C Am / Am Dm Gm C |
| Last Stand | action | E minor | 150 | 2:34 | Em G A B / E G D E |
| Sunny Hills | bright | G major | 128 | 2:30 | G C D / Gmaj7 D7 Emadd9 Cmaj7 |

Sokovox plays Crates at Dawn and Quiet Warehouse, BunnyFarm Sunny Hills
by day and Quiet Warehouse by night, Voxkeep and the platformer template Under the Dunes, the shooter template
Hot Pursuit, Pulsar Pulsar Drive (each recipe's `out` writes the copies). Every recipe has
`variation = "most"`: the bars within a section vary.

## Drum kits

`kits/` holds three drum kits as studios: files of instruments without a
channel, over which a game lays its own channels (`music:play`'s
`channels`, as `games/drums` does), one a row of steps. Each leans on a
classic drum machine of the 1980s so its sound can be modeled, made with
this synth's own voices:

- `rg80.mml`, after the analog machine of 1980 with the long booming
  bass drum, the cowbell and the hand clap;
- `rg90.mml`, after the machine of 1983 with the hard punchy kick,
  analog drums and sampled hats and cymbals;
- `rg70.mml`, after the all-sampled machine of 1984/85, 8 bits at
  25 kHz.

Every instrument says above its line what its sources say the sound is
and what is our own approximation. There are no samples: the sampled hats
and cymbals are noise, filters and the bit crusher. Where one voice
cannot make a sound, two instruments played on the same steps make it
(a snare's tone and its noise, a cowbell's two squares); the comments
say which.

## Making one

```
python3 tools/compose.py --find Dark            # progressions with these moods
python3 tools/compose.py music/night_patrol.compose.toml
voxcade music music/night_patrol.mml --out shots/music.wav
```

A recipe names the style (calm, mysterious, dark, action, bright, space), the key, the
tempo, the moods to pick progressions by (or the progressions themselves)
and a seed. Another seed gives another song in the same mood, and the same
recipe always makes the same song. `variation = "some"` or `"most"` lets
bars vary within a section (the bass walks to the next chord every fourth
bar, broken chords turn, later themes answer backwards; `most` adds ghost
notes on the hi-hat), over the same song. `tools/compose.py` says more at
its top.

## Credits

The chord progressions and their moods come from
[free-midi-chords](https://github.com/ldrolez/free-midi-chords) by Ludovic
Drolez, under the MIT licence (`tools/progressions.toml`,
`tools/progressions.LICENSE`).
