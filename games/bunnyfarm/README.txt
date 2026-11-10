BunnyFarm
=========

A farming game with a tamagotchi at its heart, made for Voxcade in Lua
alone (no kit; docs/bunnyfarm.md has the concept and the stages). You are
the bunny: sow and harvest the field, sell at the market stall, eat a
carrot when hungry, sleep in the burrow. A day runs from 6:00 to 2:00 in
about 14 minutes; sleeping skips to the morning and saves the game,
CONTINUE on the title goes on. A poor condition only slows the bunny down.

Keys: arrows walk, Space / Z acts on the bed in front or opens the place
you stand at (the stall, the burrow), Q / E choose the seed, Tab / I the
bag, X / Backspace back out of a menu (a pad: the stick or the cross,
south act, east back, the shoulders the seed, north the bag).

Made with:
- tools/gen_bunnyfarm.py: the blocks (grass, beds, paths, the pond and the
  fence in pieces that join up, flowers and tufts), the farm, the farmer,
  the crops in their four stages, the burrow, the stall, the trees and
  what stands about: the user's to shape in the editors from now on.
- music/sunny_hills.compose.toml and music/quiet_warehouse.compose.toml
  (tools/compose.py): the songs, from chord progressions of
  free-midi-chords by Ludovic Drolez (MIT).
- The rules, the numbers and the timers come from the browser prototype
  "My Farm" (its own code and data), carried into game time.
