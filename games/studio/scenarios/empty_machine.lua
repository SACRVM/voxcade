-- Pattern A cleared and the closed hat chosen: an empty machine to fill
-- step by step with the mouse (it plays silence until a step is set).
return {
  about = "Pattern A with no step set, the closed hat chosen",
  setup = function()
    for _, row in ipairs(world.state.patterns[1]) do
      for i = 1, 16 do row[i] = false end
    end
    world.state.row = #world.state.patterns[1]
    lay_pattern()
  end,
}
