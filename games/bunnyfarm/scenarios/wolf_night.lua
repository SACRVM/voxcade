-- An evening with a gap in a fence: to bed, and the shy wolf comes.
local set = require("setup")
return {
  about = "Day 12, 21:30: hens and sheep, a gap in the sheep's fence, the bunny at the burrow (to bed: the wolf comes)",
  setup = function()
    set.day(12, "21:30")
    set.level(6)
    set.coins(800)
    set.animal("chicken", "f", "white")
    set.animal("chicken", "m", "brown")
    set.animal("sheep", "f", "white")
    set.animal("sheep", "m", "black")
    set.gap("sheep")
    set.bunny_at("burrow")
  end,
}
