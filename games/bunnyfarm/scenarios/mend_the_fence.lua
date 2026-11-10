-- A gap in the hens' fence, wood in the bag, the hammer ready.
local set = require("setup")
return {
  about = "Day 6, 10:00: a gap in the hens' fence, wood in the bag, the bunny at the gap (act mends it)",
  setup = function()
    set.day(6, "10:00")
    set.level(4)
    set.animal("chicken", "f", "white")
    set.animal("chicken", "m", "brown")
    set.give("wood", 2)
    set.gap("chicken")
    set.bunny_at_gap("chicken")
  end,
}
