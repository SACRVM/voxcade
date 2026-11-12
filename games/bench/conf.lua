-- Load test at 16-bit sprite scale (tools/gen_bench.py): More Blood's
-- figures at 2x the resolution, 40 of them and the Huey. Run
--   voxcade shot games/bench --screen title
-- and read the "frame:" line. Blasts at this scale: grenades at the towers
-- and the hut east of them,
--   voxcade shot games/bench --at 880,140 --frames 430 --input "up:2;grenade:1;none:45;..."
-- and read the "ticks:" line. Lengths and speeds are More Blood's, doubled.

function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)
  c.title = "Bench"
  c.kit = "shooter"

  c.world.file = "map.toml"

  c.physics.gravity = 1040
  c.physics.max_fall = 640
  c.physics.step = 8
  c.physics.body_width = 12

  c.player.model = "player.vvx"
  c.player.spawn = { 260, 40 }
  c.player.speed = 184
  c.player.accel = 3600
  c.player.air_accel = 1400
  c.player.jump = 34
  c.player.muzzle = { 15.0, 25.0 }

  -- Its weapons (src/shooter/kit.toml's, lengths and speeds doubled); the
  -- player fires the rifle and throws grenades.
  c.weapons.rifle = {
    label = "Rifle",
    cooldown = 9,
    speed = 680,
    life = 1.0,
    damage = 1,
    blast = 5.6,
    sound = "shot",
  }

  c.weapons.rockets = {
    label = "Rockets",
    cooldown = 32,
    speed = 440,
    life = 1.6,
    damage = 6,
    blast = 24,
    rocket = true,
    friendly_fire = false,
    ammo = 8,
    sound = "rocket",
  }

  c.weapons.grenade = {
    label = "Grenade",
    throw = 168,
    blast = 28,
    damage = 6,
    friendly_fire = false,
    sound = "throw",
    ammo = 99,
    auto = false,
  }

  c.state.player = { hp = 5, weapon = "rifle", inventory = { grenade = 99 } }
  c.player.attacks = { fire = { "held" }, grenade = { "grenade" } }
  c.player.moves = { "jump", "strafe" }

  c.debris.settle = 0.7
  c.debris.max_particles = 240000
  c.debris.gore = 0.8

  c.camera.pitch = 35
  c.camera.fov = 40
  c.camera.view_width = 720

  c.light.sun = { 0.35, 0.5, -1.0 }
  c.light.ambient = 0.5
  c.light.shadow = 0.5
  c.light.sky = { 0.09, 0.07, 0.06 }
  c.light.shadow_style = "voxel"
  c.light.model_shading = "edges"
  c.light.model_shading_for = { ["huey.vvx"] = "faces" }

  c.title_screen.at = { 260, 160 }
  c.title_screen.view_width = 480
  c.title_screen.logo = { " " }
  c.title_screen.props = {
    { model = "player.vvx", at = { 80, 120 }, rotation = 180 },
    { model = "enemy.vvx", at = { 116, 120 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 152, 120 }, rotation = 180 },
    { model = "hostage.vvx", at = { 188, 120 }, rotation = 180 },
    { model = "player.vvx", at = { 224, 120 }, rotation = 180 },
    { model = "enemy.vvx", at = { 260, 120 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 296, 120 }, rotation = 180 },
    { model = "hostage.vvx", at = { 332, 120 }, rotation = 180 },
    { model = "player.vvx", at = { 368, 120 }, rotation = 180 },
    { model = "enemy.vvx", at = { 404, 120 }, rotation = 180 },
    { model = "enemy.vvx", at = { 80, 152 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 116, 152 }, rotation = 180 },
    { model = "hostage.vvx", at = { 152, 152 }, rotation = 180 },
    { model = "player.vvx", at = { 188, 152 }, rotation = 180 },
    { model = "enemy.vvx", at = { 224, 152 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 260, 152 }, rotation = 180 },
    { model = "hostage.vvx", at = { 296, 152 }, rotation = 180 },
    { model = "player.vvx", at = { 332, 152 }, rotation = 180 },
    { model = "enemy.vvx", at = { 368, 152 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 404, 152 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 80, 184 }, rotation = 180 },
    { model = "hostage.vvx", at = { 116, 184 }, rotation = 180 },
    { model = "player.vvx", at = { 152, 184 }, rotation = 180 },
    { model = "enemy.vvx", at = { 188, 184 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 224, 184 }, rotation = 180 },
    { model = "hostage.vvx", at = { 260, 184 }, rotation = 180 },
    { model = "player.vvx", at = { 296, 184 }, rotation = 180 },
    { model = "enemy.vvx", at = { 332, 184 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 368, 184 }, rotation = 180 },
    { model = "hostage.vvx", at = { 404, 184 }, rotation = 180 },
    { model = "hostage.vvx", at = { 80, 216 }, rotation = 180 },
    { model = "player.vvx", at = { 116, 216 }, rotation = 180 },
    { model = "enemy.vvx", at = { 152, 216 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 188, 216 }, rotation = 180 },
    { model = "hostage.vvx", at = { 224, 216 }, rotation = 180 },
    { model = "player.vvx", at = { 260, 216 }, rotation = 180 },
    { model = "enemy.vvx", at = { 296, 216 }, rotation = 180 },
    { model = "grenadier.vvx", at = { 332, 216 }, rotation = 180 },
    { model = "hostage.vvx", at = { 368, 216 }, rotation = 180 },
    { model = "player.vvx", at = { 404, 216 }, rotation = 180 },
    { model = "huey.vvx", at = { 260, 300 }, rotation = 270 },
  }
end
