-- A farm late in its first year: every row, every stable, every animal.
local set = require("setup")
return {
  about = "Day 40, 10:00, level 9: every row and stable, animals of every kind (a rainbow hen, a mother brooding), rich, a full bag",
  setup = function()
    set.day(40, "10:00")
    set.level(9)
    set.coins(50000)
    set.carrots(30)
    set.rows(4)
    set.crops("wheat", 0.5)
    for _, sp in ipairs({ "chicken", "sheep", "cow" }) do
      set.stable(sp)
    end
    set.animal("chicken", "f", "white", { ready = true })
    set.animal("chicken", "m", "brown")
    set.animal("chicken", "r", "white", { ready = true })
    set.animal("chicken", "f", "red", { young = true })
    set.animal("chicken", "f", "blue", { carrying = true, father = "brown" })
    set.animal("sheep", "f", "white", { ready = true })
    set.animal("sheep", "m", "black")
    set.animal("cow", "f", "pink", { ready = true })
    set.animal("cow", "m", "light")
    for item, n in pairs({ wheat = 90, wood = 5, egg = 6, wool = 2, milk = 3, fertilizer = 5, seed_carrot = 10, seed_tomato = 6 }) do
      set.give(item, n, 24)
    end
  end,
}
