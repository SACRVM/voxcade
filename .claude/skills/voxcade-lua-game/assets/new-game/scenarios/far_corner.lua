-- A scenario: a run begun later than the start, to try what happens there
-- without playing to it (docs/game-model.md, "Scenarios and the console").
-- PLAY SCENARIO above the file plays the game from here; voxcade shot,
-- inspect and run take --scenario far_corner. setup runs once, before the
-- first tick: set the states and things as late in the game.
return {
  about = "The player in the floor's far corner",
  setup = function()
    player:place({ x = level.size.x - 8, y = level.size.y - 8, z = 1 })
  end,
}
