-- Pattern A cleared and the hats chosen: an empty machine to fill step
-- by step with the mouse (it plays silence until a step is set).
return {
  about = "Pattern A with no step set, the hats chosen",
  setup = function()
    for _, row in ipairs(world.state.patterns[1]) do
      for i = 1, 16 do row[i] = false end
    end
    world.state.row = 3
    lay_pattern()
  end,
}
