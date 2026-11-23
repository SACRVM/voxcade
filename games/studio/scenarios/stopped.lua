-- The machine stopped on pattern B, its levels and the volume turned
-- down: START (the red button) begins it at the first step.
return {
  about = "Stopped on pattern B, the levels and the volume turned down",
  setup = function()
    local knobs = world.state.knobs
    world.state.shown, world.state.playing = 2, 2
    knobs.bd_level, knobs.sd_level, knobs.ch_level = 0.6, 0.4, 0.3
    knobs.volume = 0.7
    apply_knobs(0)
    start_stop()
  end,
}
