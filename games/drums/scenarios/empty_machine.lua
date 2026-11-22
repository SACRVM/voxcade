-- Pattern A cleared: an empty machine to fill step by step (it plays
-- silence until a step is set).
return {
  about = "Pattern A with no step set",
  setup = function()
    for _, row in ipairs(world.state.patterns[1]) do
      for i = 1, 16 do row[i] = false end
    end
    lay_pattern()
  end,
}
