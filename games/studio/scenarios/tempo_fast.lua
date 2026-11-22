-- The machine playing pattern A with the tempo knob near its top (174
-- beats a minute): the knob is turned, the sound follows.
return {
  about = "Playing, the tempo knob near its top",
  setup = function()
    world.state.knobs.knob_tempo = 0.94
    apply_knobs(0)
  end,
}
