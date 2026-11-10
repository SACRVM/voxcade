-- The last puzzle: what solving it leads to.
return {
  about = "The last puzzle of Microban, every one before it counted as solved",
  setup = function()
    world.state.puzzle = 155
    for n = 1, 154 do
      world.state.done[n] = true
    end
    world:restart()
  end,
}
