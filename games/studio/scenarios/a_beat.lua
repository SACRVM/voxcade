-- A full pattern over six voices, a few knobs turned away from their
-- middle (a long kick, a bright snare, a conga tuned down, a short open
-- hat, a strong accent), playing: what the knobs and the switches do,
-- heard at once.
local function steps(picture)
  local row = {}
  for i = 1, 16 do row[i] = picture:sub(i, i) == "x" end
  return row
end

return {
  about = "Six voices playing a full pattern, a few knobs turned, the low tom a conga",
  setup = function()
    -- The rows in the selector's order: AC BD SD LT MT HT RS CP CB CY OH CH.
    local rows = { "x...x.....x.x...", "x.....x...x..x..", "....x.......x...", "..x.....x.....x.",
      "................", "................", "................", "....x.......x..x",
      "................", "................", "......x.......x.", "x.x.x...x.x.x..." }
    for r, picture in ipairs(rows) do world.state.patterns[1][r] = steps(picture) end
    world.state.other.lt = true
    local knobs = world.state.knobs
    knobs.bd_decay, knobs.bd_tone = 0.85, 0.5
    knobs.sd_tone, knobs.sd_snappy = 0.8, 0.9
    knobs.lt_tuning, knobs.lt_level = 0.25, 0.7
    knobs.oh_decay, knobs.ac_level = 0.2, 0.9
    world.state.row = 4
    apply_knobs(0)
    lay_pattern()
  end,
}
