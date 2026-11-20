-- Lights: a test hall for the lamps, the fog and what stops them, as the
-- Bench is a load test. Not a game to show: it has no cover, so the
-- launcher does not list it, and the packages leave it out. Its pictures
-- are tests (tests/pictures.rs), its scenarios each subject alone.

function conf(c)
  c.voxcade = "0.7.0"
  c.title = "Lights"

  -- The hall: a level of blocks 4 voxels each way (levels/hall.toml), its
  -- lamps that stand still as the level's lights, its thin walls, glass
  -- and water as pieces (pieces/).
  c.world.blocks = { file = "levels/hall.toml" }
  c.player.spawn = { 60, 30 }

  -- The figures (copies of others', figures/): the hero walks with the
  -- arrows; the rest stand, walk or dance in front of the lamps.
  c.things.hero = { model = "figures/hero.vvx" }
  c.things.dancer = { model = "figures/dancer.vvx" }
  c.things.guard = { model = "figures/guard.vvx" }
  c.things.posed = { model = "figures/posed.vvx" }
  c.player.kind = "hero"

  -- Q and E (or the pad's shoulders) step through the subjects.
  c.actions = { "prev", "next" }
  c.keys = { prev = { "Q" }, next = { "E" } }
  c.pad = { prev = { "leftshoulder" }, next = { "rightshoulder" } }

  c.camera.pitch = 50
  c.camera.view_width = 160

  -- A dim hall, the lamps the light that counts, but the room itself
  -- readable round them.
  c.light.ambient = 0.3
  c.light.shadow = 0.35
  c.light.sun_color = { 76.5, 81.6, 107.1 }
  c.light.sky = { 5.1, 5.1, 10.2 }

  c.title_screen.logo = { "LIGHTS" }
  c.title_screen.attract_after = 0
end
