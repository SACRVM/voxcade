-- The year's last night: asleep within the minute, the reckoning.
local set = require("setup")
return {
  about = "Day 56, 1:59: the year's last night; the bunny falls asleep within the minute, the reckoning follows",
  setup = function()
    set.day(56, "1:59")
    set.level(7)
    set.coins(4000)
    set.give("wheat", 30)
    set.animal("chicken", "f", "white", { ready = true })
    set.bunny_at("burrow")
  end,
}
