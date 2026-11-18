-- Lamps at walls: the side room's lamp shines through the doorway and
-- ends at the wall, a lamp beside a pillar casts its shadow.
return {
  about = "A lamp behind a doorway and a pillar's shadow",
  setup = function()
    world.state.subject = "walls"
  end,
}
