-- The last life with one hit point: the next hit ends the run.
return {
  about = "The last life, one hit point left: the next hit ends the run",
  setup = function()
    player.state.lives = 1
    player.state.hp = 1
  end,
}
