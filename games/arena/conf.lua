-- Voxcade demo arena. Every value here can be changed while the game runs:
-- save the file and the game picks it up.

function conf(c)
  c.title = "Arena"
  c.languages = { "en", "de", "fr" }   -- the menus speak them; the keys shown at the start too
  c.kit = "shooter"
  c.scored = true                 -- it keeps a score: the high score table
  c.state.player = { hp = 5, weapon = "rifle", inventory = { grenade = 5 } }   -- what a run starts with
  c.player.invulnerable = 75        -- ticks nothing hurts after a hit
  c.player.attacks = {               -- what each action attacks with: the first of its weapons that can
    fire = { "knife", "held", "rifle" },   -- the knife on a man close in front, else the gun in hand, else the rifle
    grenade = { "grenade" },
  }
  c.player.moves = { "jump", "strafe" }

  -- The weapons it has (src/shooter/kit.toml has the kit's to copy).
  c.weapons.rifle = {
    label = "Rifle",
    cooldown = 9,                    -- ticks between shots
    speed = 170,                     -- voxels a second
    life = 1.0,                      -- seconds a round flies
    damage = 1,
    blast = 3.4,                     -- crater where it hits the world (0: sparks)
    sound = "shot",
  }
  c.weapons.knife = {
    label = "Knife",
    reach = 6.5,                     -- a blow: this far, along the ground, to one in front ...
    cone = 145.08,                   -- ... within this many degrees of the way it faces
    cooldown = 16,
    damage = 4,
    noise = 0,                       -- silent: nobody hears it
    sound = "knife",
  }
  c.weapons.grenade = {
    label = "Grenade",
    throw = 42,                      -- a full throw; it goes off just after landing
    blast = 4.5,
    damage = 6,                      -- to everyone in the blast
    friendly_fire = false,           -- it spares its thrower
    sound = "throw",
    ammo = 99,                       -- an inventory holds this many
    auto = false,                    -- one a press
  }

  c.world.file = "world.vox"
  c.world.bedrock = true          -- the lowest layer cannot be blown away

  c.player.model = "player.vox"    -- forward is +y
  c.player.spawn = { 256, 150 }    -- x, y on the ground

  c.camera.pitch = 35              -- degrees below the horizon, 90 = straight down
  c.camera.yaw = 0                 -- 0 looks north (+y); keep 0, actors do not turn with it yet
  c.camera.fov = 40                -- vertical field of view in degrees: the strength of the perspective
  c.camera.view_width = 200        -- voxels across the screen at the player, at any resolution
  c.camera.follow = 0.2            -- how quickly the camera catches up per tick (only with snap = false)
  c.camera.snap = true             -- move and scroll in whole voxels (3D-pixel look); false glides

  c.light.sun = { 0.45, 0.6, -1.0 } -- direction the sunlight travels
  c.light.ambient = 0.45
  c.light.shadow = 0.55
  c.light.sky = { 0.06, 0.07, 0.10 }

  c.debris.settle = 0.85           -- chance a landed debris voxel rejoins the world
  c.debris.max_particles = 30000
end
