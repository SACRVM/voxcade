-- The machine stopped on pattern B, its knobs turned down: START (the
-- red button) begins it at the first step.
return {
  about = "Stopped on pattern B, the levels and the volume turned down",
  setup = function()
    world.state.shown, world.state.playing = 2, 2
    world.state.levels = { 0.6, 0.4, 0.3 }
    world.state.volume = 0.7
    music:volume(0.7, 0)
    start_stop()
  end,
}
