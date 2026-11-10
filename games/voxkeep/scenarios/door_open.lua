-- Every gem of the hall taken: the door opens and leads on.
return {
  about = "The hall with every gem taken: its door open",
  setup = function()
    for _, gem in ipairs(level:things("gem")) do
      player.state.score = player.state.score + gem.props.worth
      player.state.taken = player.state.taken + 1
      gem:remove()
    end
  end,
}
