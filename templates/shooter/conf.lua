-- A shooter with the shooter kit: run and fire through a yard that
-- main.lua builds, riflemen up north; one plays, or two (a second player
-- joins with START on a pad, or Return at the keyboard). Saved while the
-- game runs, a setting takes effect at once; the kit, the players and
-- where they start after a restart (F5). Lengths are in voxels, speeds in
-- voxels a second; the figures are 22 voxels tall.

function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)
  c.title = "My Game"
  c.kit = "shooter"
  c.players = 2                     -- seats: a second player may join
  c.player.kind = { "hero", "hero2" }  -- each player's figure (c.things below)
  c.scored = true                 -- it keeps a score: the high score table
  c.state.player = {                -- what a run starts with
    hp = 5,                         -- hit points (the kit keeps them: player.state.hp; main.lua's MAX_HP is the same number)
    weapon = "rifle",               -- the gun in hand (player.state.weapon)
    lives = 3,                      -- the game's own: main.lua counts them
    inventory = { grenade = 5 },
  }
  c.player.invulnerable = 75        -- ticks nothing hurts after a hit
  c.player.attacks = {               -- what each action attacks with: the first of its weapons that can
    fire = { "knife", "held", "rifle" },   -- the knife on a man close in front, else the gun in hand, else the rifle
    grenade = { "grenade" },
  }
  c.player.moves = { "jump", "strafe" }

  -- The weapons (src/shooter/kit.toml has more to copy): a weapon with ammo
  -- keeps its rounds in the inventory under its name.
  c.weapons.rifle = {
    label = "Rifle",
    cooldown = 9,                   -- ticks between shots
    speed = 170,                    -- voxels a second
    life = 1.0,                     -- seconds a round flies
    damage = 1,
    blast = 3.4,                    -- crater where it hits the world (0: sparks)
    sound = "shot",
  }
  c.weapons.knife = {
    label = "Knife",
    reach = 6.5,                    -- a blow: this far, along the ground, to one in front ...
    cone = 145.08,                  -- ... within this many degrees of the way it faces
    cooldown = 16,
    damage = 4,
    noise = 0,                      -- silent: nobody hears it, nobody sees the man fall
    sound = "knife",
  }
  c.weapons.grenade = {
    label = "Grenade",
    throw = 42,                     -- a full throw; it goes off just after landing
    blast = 4.5,                    -- its crater, a ball this far around where it lies; one about as far as
                                    -- the figure is tall (22 here) opens a way through a wall
    damage = 6,                     -- to everyone in the blast
    friendly_fire = false,          -- it spares its thrower
    sound = "throw",
    ammo = 99,                      -- an inventory holds this many
    auto = false,                   -- one a press
  }
  c.weapons.enemy_rifle = {
    label = "Rifle",
    cooldown = 70,
    jitter = 50,                    -- and a random number of ticks below this on top
    speed = 80,
    life = 2.0,
    spread = 0.08,                  -- how far rounds stray, with aim assist
    sound = "enemy_shot",
  }

  -- A world made in code: this big, built by main.lua's on_level_start.
  c.world.size = { 256, 512, 48 }
  c.world.bedrock = true

  c.physics.gravity = 520
  c.physics.max_fall = 320
  c.physics.step = 4                -- ledges this high are walked up without jumping
  c.physics.body_width = 6

  c.player.model = "player.vvx"     -- the player's figure in a game of one (c.players = 1)
  c.player.spawn = { 128, 40 }      -- x, y on the ground
  c.player.speed = 92
  c.player.jump = 17
  c.player.muzzle = { 7.5, 12.5 }   -- where shots leave the gun: ahead of the feet, above them

  -- The players' figures: no behavior, their keys move them.
  c.things.hero = { model = "player.vvx" }
  c.things.hero2 = { like = "hero", model = "player2.vvx" }   -- the second, in blue

  c.things.rifleman = {
    behavior = "soldier",          -- walks about, wakes on sight or noise, fires along the 8 ways
    model = "enemy.vvx",
    hp = 2,
    team = "enemies",               -- their rounds hurt other teams, not their own
    weapon = "enemy_rifle",
    speed = 48,
    sight = 200,
    muzzle = { 7.5, 12.5 },
    family = "soldiers",
    props = { worth = 100, knife_worth = 150 },  -- what main.lua adds for him
  }

  c.camera.pitch = 45
  c.camera.view_width = 260
  c.camera.hold = { 220, 100 }      -- how far apart two players may be, across and deep

  c.title_screen.logo = { "MY GAME" }
  c.audio.music = { "music/hot_pursuit.mml" }

  -- the HUD's pictures (main.lua draws the HUD): a letter a colour, . empty
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
