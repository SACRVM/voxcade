-- Pattern B playing from the start, the cursor on its hats.
return {
  about = "Pattern B playing, the cursor on the hats",
  setup = function()
    world.state.shown, world.state.playing = 2, 2
    world.state.row, world.state.step = 3, 1
    lay_pattern()
  end,
}
