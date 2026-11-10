-- The end of the jungle: the helipad ahead, the gunship about.
return {
  about = "Near the end: the helipad ahead, a little way off",
  setup = function()
    local goal = level:things("goal")[1]
    player:place({ x = goal.x, y = goal.y - 160, z = goal.z })
  end,
}
