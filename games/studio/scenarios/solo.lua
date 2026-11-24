-- SOLO on with the selector on the bass drum: the pattern plays on, only
-- the bass drum is heard (every other voice muted), its lamp lit.
return {
  about = "SOLO on, the selector on the bass drum: only the bass drum sounds",
  setup = function()
    world.state.row = 2
    world.state.solo = true
    apply_solo()
  end,
}
