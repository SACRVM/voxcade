-- The load case: sixteen spots whose shadows are "all" over fifty walking
-- figures, in fog.
return {
  about = "16 lamps, 50 figures",
  setup = function()
    world.state.subject = "load"
  end,
}
