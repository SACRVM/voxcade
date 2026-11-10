-- The yard with the player's last hit point: what the next round does.
return {
  about = "The player with one hit point left",
  setup = function()
    player.state.hp = 1
  end,
}
