-- The fox at the farm at noon, a hen and a rooster grown to sell her.
local set = require("setup")
return {
  about = "Day 5, 12:30: the fox is here; a grown hen and rooster, the bunny by her",
  setup = function()
    set.day(5, "12:30")
    set.level(5)
    set.coins(3000)
    set.give("wheat", 40)
    set.animal("chicken", "f", "white")
    set.animal("chicken", "m", "brown")
    set.bunny_at("fox", 0, -12)
  end,
}
