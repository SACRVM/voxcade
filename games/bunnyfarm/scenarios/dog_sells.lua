-- The dog at the farm in the morning, coins to buy with, a pen with room.
local set = require("setup")
return {
  about = "Day 5, 8:30: the dog sells young animals; a hen and a rooster, coins, the bunny by him",
  setup = function()
    set.day(5, "8:30")
    set.level(5)
    set.coins(3000)
    set.give("wheat", 40)
    set.animal("chicken", "f", "white")
    set.animal("chicken", "m", "brown")
    set.bunny_at("dog", 8, -12)
  end,
}
