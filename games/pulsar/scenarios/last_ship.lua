-- The last ship, no bombs: the next hit ends the run.
return {
  about = "The last ship, no bombs left",
  setup = function()
    player.state.lives = 1
    player.state.bombs = 0
  end,
}
