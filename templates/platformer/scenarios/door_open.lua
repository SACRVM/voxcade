-- A scenario: a run begun later than the start, to try what happens there
-- without playing to it (docs/game-model.md, "Scenarios and the console").
-- PLAY SCENARIO above the file plays the game from here; voxcade shot,
-- inspect and run take --scenario door_open.
return {
  about = "Every gem taken: the door open",
  setup = function()
    for _, gem in ipairs(level:things("gem")) do
      player.state.score = player.state.score + gem.props.worth
      player.state.taken = player.state.taken + 1
      gem:remove()
    end
  end,
}
