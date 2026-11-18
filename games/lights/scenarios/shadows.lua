-- Three bays side by side, the same lamp, low wall and figure, only the
-- word differing: "none" shines through the wall, "static" is stopped
-- by it, "all" by the figure too.
return {
  about = "Shadows none, static and all side by side",
  setup = function()
    world.state.subject = "shadows"
  end,
}
