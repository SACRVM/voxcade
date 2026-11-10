-- A scenario: a run begun later than the start, to try what happens there
-- without playing to it (docs/game-model.md, "Scenarios and the console").
-- PLAY SCENARIO above the file plays the game from here; voxcade shot,
-- inspect and run take --scenario last_life.
return {
  about = "The last life, one hit point left",
  setup = function()
    player.state.lives = 1
    player.state.hp = 1
  end,
}
