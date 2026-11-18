-- Seven spots in a row on the stages' floor, each one word from the one
-- before it: falloff 4, 2 and 1, soft, narrow, pitched, rotated.
return {
  about = "Seven spots, each one word from the next",
  setup = function()
    world.state.subject = "spots"
  end,
}
