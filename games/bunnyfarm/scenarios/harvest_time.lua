-- Every open bed ripe, the scarecrow to harvest them all.
local set = require("setup")
return {
  about = "Day 8, 9:00: four rows of ripe carrots, coins for the fees, the bunny at the scarecrow",
  setup = function()
    set.day(8, "9:00")
    set.level(4)
    set.coins(500)
    set.rows(4)
    set.crops("carrot")
    set.bunny_at("scarecrow", 0, -20)
  end,
}
