Pulsar
======

A space shooter that scrolls up the screen, made for Voxcade in Lua alone
(no kit): the ship flies on by itself; the arrows steer it across, push it
ahead or let it fall back; fire shoots (Z or Space), bomb clears the
enemies' shots and hurts all on screen (X). The enemies come in waves on
the music's bars, one after another on its beats, and fire on its beats.

Keys: arrows, Z / Space fire, X / Left Shift bomb (a pad: the stick or the
pad's cross, south fire, east bomb).

Made with:
- tools/gen_pulsar.py: the ships, the boss, the explosion, the power-up,
  the stars, the asteroids and the stations, all generated.
- music/pulsar_drive.compose.toml (tools/compose.py): the song, from chord
  progressions of free-midi-chords by Ludovic Drolez (MIT).
