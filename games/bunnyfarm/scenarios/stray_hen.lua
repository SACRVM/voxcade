-- The morning after the wolf: a hen waits in the meadow behind her pen.
local set = require("setup")
return {
  about = "Day 9, 9:00: a hen ran off behind her pen; fetch her round the fence through the farm's gate",
  setup = function()
    set.day(9, "9:00")
    set.level(4)
    set.animal("chicken", "f", "white")
    set.animal("chicken", "f", "brown", { out = { x = 392, y = 528 } })
    set.gap("chicken", 24)
    set.bunny_at("farm_gate", 0, 12)
  end,
}
