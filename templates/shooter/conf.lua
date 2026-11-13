-- The game's settings: a shooter made with the shooter kit. Run and
-- fire through a yard that main.lua builds, riflemen further up.
-- One or two can play. A second player joins with START on a gamepad or
-- Return on the keyboard.
-- Save while the game runs and most settings change at once. The kit,
-- the players and where they start change after a restart (F5).
-- Lengths are in voxels (the figures are 22 voxels tall). Speeds are in
-- voxels per second.

function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade version these files are for
  c.title = "My Game"
  c.kit = "shooter"
  c.players = 2                     -- up to two players
  c.player.kind = { "hero", "hero2" }  -- each player's figure (see c.things below)
  c.scored = true                 -- keep a high score table
  c.state.player = {                -- each player at the start:
    hp = 5,                         -- hearts (main.lua's MAX_HP)
    weapon = "rifle",               -- the gun in hand
    lives = 3,                      -- lives (main.lua counts them)
    inventory = { grenade = 5 },
  }
  c.player.invulnerable = 75        -- ticks a player cannot be hurt after a hit
  c.player.attacks = {               -- which weapon each key uses: the first one that can
    fire = { "knife", "held", "rifle" },   -- the knife if an enemy is close, else the gun in hand, else the rifle
    grenade = { "grenade" },
  }
  c.player.moves = { "jump", "strafe" }

  -- The weapons. A weapon with ammo keeps its rounds in the inventory,
  -- under its own name.
  c.weapons.rifle = {
    label = "Rifle",
    cooldown = 9,                   -- ticks between shots
    speed = 170,                    -- voxels per second
    life = 1.0,                     -- seconds a round flies
    damage = 1,
    blast = 3.4,                    -- the hole it makes where it hits (0: only sparks)
    sound = "shot",
  }
  c.weapons.knife = {
    label = "Knife",
    reach = 6.5,                    -- hits someone this close in front ...
    cone = 145.08,                  -- ... within this many degrees of where the player faces
    cooldown = 16,
    damage = 4,
    noise = 0,                      -- silent: no other soldier notices
    sound = "knife",
  }
  c.weapons.grenade = {
    label = "Grenade",
    throw = 42,                     -- how far it flies; it goes off just after landing
    blast = 4.5,                    -- the size of its blast; about the figure's height (22)
                                    -- would blow a way through a wall
    damage = 6,                     -- to everyone in the blast
    friendly_fire = false,          -- it does not hurt the player who threw it
    sound = "throw",
    ammo = 99,                      -- the most a player can carry
    auto = false,                   -- one per press
  }
  c.weapons.enemy_rifle = {
    label = "Rifle",
    cooldown = 70,
    jitter = 50,                    -- plus up to this many ticks more, at random
    speed = 80,
    life = 2.0,
    spread = 0.08,                  -- how much the shots miss
    sound = "enemy_shot",
  }

  -- The world is built in code (main.lua's on_level_start). Its size:
  c.world.size = { 256, 512, 48 }
  c.world.bedrock = true

  c.physics.gravity = 520
  c.physics.max_fall = 320
  c.physics.step = 4                -- steps this high are walked up without jumping
  c.physics.body_width = 6

  c.player.model = "player.vvx"     -- the player's figure when only one can play (c.players = 1)
  c.player.spawn = { 128, 40 }      -- where the players start: x, y on the ground
  c.player.speed = 92
  c.player.jump = 17
  c.player.muzzle = { 7.5, 12.5 }   -- where shots start: in front of the feet, and how high

  -- The players' figures. They have no behavior: the keys move them.
  c.things.hero = { model = "player.vvx" }
  c.things.hero2 = { like = "hero", model = "player2.vvx" }   -- the second, in blue

  c.things.rifleman = {
    behavior = "soldier",          -- walks around, notices what it sees or hears, fires in 8 directions
    model = "enemy.vvx",
    hp = 2,
    team = "enemies",               -- their shots hurt other teams, not their own
    weapon = "enemy_rifle",
    speed = 48,
    sight = 200,
    muzzle = { 7.5, 12.5 },
    family = "soldiers",
    props = { worth = 100, knife_worth = 150 },  -- points main.lua gives for him
  }

  c.camera.pitch = 45
  c.camera.view_width = 260
  c.camera.hold = { 220, 100 }      -- how far apart two players can get: across, deep

  c.title_screen.logo = { "MY GAME" }
  c.audio.music = { "music/hot_pursuit.mml" }

  -- Small pictures for the HUD (main.lua draws them). Each letter is a
  -- color; a dot is empty.
  c.icons.heart = {
    rows = { ".RR.RR.", "RWRRRRR", "RRRRRRR", ".RRRRR.", "..RRR..", "...R..." },
    colors = { R = { 214, 36, 44 }, W = { 255, 176, 170 } },
  }
  c.icons.empty_heart = {
    rows = { ".KK.KK.", "K..K..K", "K.....K", ".K...K.", "..K.K..", "...K..." },
    colors = { K = { 96, 70, 64 } },
  }
  c.icons.grenade = {
    rows = { "..S..", ".SS..", ".GGG.", "GLGGG", "GLGGG", "GGGGG", ".GGG." },
    colors = { S = { 172, 172, 160 }, G = { 84, 104, 56 }, L = { 128, 152, 84 } },
  }
end
