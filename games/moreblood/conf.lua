-- More Blood (working title): run north through the desert, free the
-- hostages, reach the helipad. Saved while the game runs, a setting takes
-- effect at once; the kit, the level, the markers and where the player
-- starts after a restart (F5).
--
-- Drawn at 8-bit sprite scale: figures are 22 voxels tall. Every length is
-- in voxels and every speed in voxels per second.

function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)
  c.title = "More Blood"
  c.languages = { "en", "de", "fr" }   -- the menus speak them; the keys shown at the start too
  c.kit = "shooter"               -- soldiers, huts, hostages, pickups, the gunship (src/shooter)
  c.scored = true                 -- it keeps a score: the high score table
  c.state.player = {                -- what a run starts with
    hp = 5,                         -- hit points (the kit keeps them: player.state.hp)
    weapon = "rifle",               -- the gun in hand (player.state.weapon)
    lives = 3,                      -- the game's own: main.lua counts them
    inventory = { grenade = 5 },
  }
  c.player.invulnerable = 75        -- ticks nothing hurts after a hit
  c.high_scores = {               -- the table to beat before anyone played
    { "NIK", 25000 }, { "HUY", 20000 }, { "GUN", 17500 }, { "BLD", 15000 }, { "RAM", 12500 },
    { "DOG", 10000 }, { "TAG", 7500 }, { "SAR", 5000 }, { "POW", 2500 }, { "MIA", 1000 },
  }

  c.world.file = "map.toml"       -- ground images and prefabs, put together at start (tools/gen_moreblood.py)
  c.world.bedrock = true

  c.physics.gravity = 520
  c.physics.max_fall = 320
  c.physics.step = 4                -- ledges this high are walked up without jumping
  c.physics.body_width = 6          -- collision box of everyone who walks

  c.player.model = "player.vvx"
  c.player.spawn = { 320, 180 }
  c.player.speed = 92
  c.player.accel = 1800            -- on the ground
  c.player.air_accel = 700
  c.player.jump = 17               -- height of a full jump
  c.player.muzzle = { 7.5, 12.5 }  -- where shots leave the gun: ahead of the feet, above them
  c.player.attacks = {               -- what each action attacks with: the first of its weapons that can
    fire = { "knife", "held", "rifle" },   -- the knife on a man close in front, else the gun in hand, else the rifle
    grenade = { "grenade" },
  }
  c.player.moves = { "jump", "strafe" }

  c.weapons.aim_assist = true       -- shots find a target near the line, the player's and the enemies'; false: only the 8 ways

  c.weapons.rifle = {
    label = "Rifle",
    cooldown = 9,           -- ticks between shots
    speed = 340,
    life = 1.0,             -- seconds a round flies
    damage = 1,
    blast = 2.8,            -- a rifle shot chips the wall; grenades and rockets blow holes
    sound = "shot",
  }

  c.weapons.machine_gun = {
    like = "rifle",
    label = "MG",
    cooldown = 4,
    life = 0.9,
    spread = 0.07,          -- rounds stray this far (with aim assist)
    ammo = 150,             -- rounds an inventory holds (player.state.inventory.machine_gun)
  }

  c.weapons.rockets = {
    label = "Rockets",
    cooldown = 32,
    speed = 220,
    life = 1.6,
    damage = 6,
    blast = 12,
    rocket = true,          -- blows up where it hits, hurting everyone near
    friendly_fire = false,  -- but not the player who fired it
    ammo = 8,
    sound = "rocket",
  }

  c.weapons.knife = {
    label = "Knife",
    reach = 13,             -- a blow: this far to a man in front ...
    cone = 145.08,          -- ... within this many degrees of the way the player faces
    cooldown = 16,
    damage = 4,             -- every man of More Blood falls to it
    noise = 0,              -- silent: nobody hears it, nobody sees him fall
    sound = "knife",
  }

  c.weapons.grenade = {
    label = "Grenade",
    throw = 84,             -- a full throw; it goes off just after landing
    blast = 14,
    damage = 6,             -- to everyone in the blast: every man of More Blood falls
    friendly_fire = false,  -- it spares the player who threw it
    sound = "throw",
    ammo = 99,              -- an inventory holds this many (player.state.inventory.grenade)
    auto = false,           -- one a press
  }

  c.weapons.enemy_rifle = {
    label = "Rifle",
    cooldown = 70,
    jitter = 50,            -- and a random number of ticks below this on top
    speed = 160,
    life = 2.0,
    spread = 0.08,
    sound = "enemy_shot",
  }

  c.weapons.enemy_grenade = {
    like = "grenade",
    cooldown = 150,
    jitter = 70,
    ammo = 0,               -- the enemies carry no inventory
    damage = 2,             -- the player loses two hit points in its blast
    friendly_fire = true,   -- their own men in it are hurt too
  }

  c.weapons.door_gun = {  -- the door gunners' M60s
    label = "Door gun",
    cooldown = 6,           -- between the rounds of a burst
    burst = 5,
    pause = 110,            -- after a burst
    speed = 190,
    life = 2.5,
    spread = 0.04,
    sound = "enemy_shot",
  }

  c.weapons.rocket_pods = {
    label = "Rocket pods",
    cooldown = 260,         -- the first after half of it
    speed = 150,
    life = 3.0,
    damage = 2,
    blast = 12,
    rocket = true,
    sound = "rocket",
  }

  c.things.rifleman = {
    behaviour = "soldier",  -- walks about, wakes on sight or noise, keeps his distance, fires along the 8 ways
    model = "enemy.vvx",
    hp = 2,                 -- rifle hits he takes (the player's shots find him in a narrow cone)
    team = "enemies",       -- their rounds and blows hurt other teams, not their own
    weapon = "enemy_rifle",
    speed = 48,
    idle_speed = 20,
    accel = 600,
    sight = 240,            -- about what is on screen ahead: they see you when you see them
    hearing = 300,          -- gunfire (rounds hitting near him: half that around the hit)
    blast_hearing = 420,    -- explosions
    active = 440,           -- further away he waits
    range = 260,            -- shooting range
    distance = 64,          -- the distance he keeps from the player
    muzzle = { 7.5, 12.5 },
    family = "soldiers",
    props = { worth = 100, knife_worth = 150 },  -- what main.lua adds for him, knifed or not
  }

  c.things.guard = {  -- a rifleman up on a watchtower
    like = "rifleman",
    hp = 4,
  }

  c.things.grenadier = {
    like = "rifleman",
    model = "grenadier.vvx",
    hp = 3,
    weapon = "enemy_grenade",
    distance = 88,
  }

  c.things.hut = {
    behaviour = "spawner",  -- sends its men while the player is this close
    spawns = "rifleman",
    count = 2,
    first = 60,             -- ticks
    every = 420,
    range = 240,
    ahead = { 0, 1 },       -- and only while the hut lies north of him: nobody comes out behind his back
  }

  c.things.hostage = {
    behaviour = "captive",
    model = "hostage.vvx",
    speed = 60,             -- running off, a little faster than the enemies chase
    accel = 798,
    reach = 14,             -- this close (through the bars) touches it: main.lua frees it
    flee = { 0, -1 },       -- freed, it runs south, back the way the player came
    hp = 1,                 -- a blast kills it (main.lua: the run is lost)
    immune = { "round", "melee" },  -- rounds fly past it, no blow is meant for it
    props = { worth = 500 },
  }

  c.things.gunship = {  -- the gunship parked on the helipad: the goal opens once it is down
    behaviour = "gunship",
    model = "huey.vvx",     -- frames turn the rotor (hind.vvx: the attack helicopter)
    hp = 60,                -- rifle rounds (a rocket or a blast counts 6)
    team = "enemies",
    immune = { "melee" },   -- no knife reaches the machine
    weapon = "door_gun",
    rockets = "rocket_pods",
    speed = 70,
    altitude = 46,          -- over the pad it took off from (clears the palms)
    clearance = 4,          -- at least this over what stands below
    offset = { 0, 35 },     -- where it holds from the player: north of him, up the screen
    wake = 260,             -- it lifts off when the player comes this close
    size = { 32, 72, 31 },  -- hit box: across, along, high (the model's skids to rotor)
    gun = { 16, 4, 15 },    -- the door gunners' M60 muzzles (across, mirrored; along; up)
    pods = { 13, 15, 7 },   -- the rocket pods' fronts
    side_on = true,         -- it turns its side to the player: the door gunner on that side fires
    crash_blast = 24,
    crash_damage = 2,       -- to those near where it crashes
    props = { worth = 5000 },
    -- The Hind instead: model = "hind.vvx", size = { 36, 76, 28 }, gun = { 0, 38, 5 }
    -- (its chin gun), pods = { 14, 7, 7 }, side_on = false.
  }

  -- Pickups lie there; what each gives is main.lua's (props).
  c.things.machine_gun = {
    behaviour = "pickup",
    model = "pickups.vvx",  -- models: machine gun, rockets, grenades, health
    frame = 0,
    reach = 9,              -- this close it is touched
    family = "pickups",
    props = { weapon = "machine_gun", ammo = 150, worth = 50 },  -- the gun handed over, its rounds
  }

  c.things.rockets = {
    like = "machine_gun",
    frame = 1,
    props = { weapon = "rockets", ammo = 8, worth = 50 },
  }

  c.things.grenades = {
    behaviour = "pickup",
    model = "pickups.vvx",
    frame = 2,
    reach = 9,
    family = "pickups",
    props = { grenades = 5, worth = 50 },
  }

  c.things.health = {
    behaviour = "pickup",
    model = "pickups.vvx",
    frame = 3,
    reach = 9,
    family = "pickups",
    props = { heal = 2, worth = 50 },
  }

  c.things.checkpoint = {}    -- a line across the level: past it, main.lua moves the checkpoint

  c.things.goal = {           -- the helipad: main.lua wins there once a gunship is down
    reach = 18,               -- this close to the goal marker: touched
    props = { worth = 2000 },
  }

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
  c.icons.head = {            -- Nick Pain, for his lives
    rows = { ".HHH.", "HHHHH", "BBBBB", "TTTTT", "TETET", "TTTTT", ".TTT." },
    colors = { H = { 34, 28, 26 }, B = { 196, 40, 36 }, T = { 214, 168, 126 }, E = { 24, 22, 20 } },
  }

  c.ui.display_font = "ui/display_font.png"  -- big letters (logo, headings, messages): Fira Sans Compressed Heavy, SIL OFL
  c.ui.round = 0.1             -- big letters' front edges rounded, radius a tenth of their height
  c.ui.grain = 0.14            -- and their colour a little uneven

  -- the start: a camera over the clearing at the start where the dog tags lie
  c.title_screen.music = "music/storm.mml"
  c.title_screen.at = { 282, 204 }       -- the tags lie low and right of the middle
  c.title_screen.view_width = 370        -- the whole clearing, tags and all
  c.title_screen.logo = { "MORE", "BLOOD" }
  c.title_screen.logo_colors = { "#f0e8d0", "#c8181a" }
  c.title_screen.logo_heights = { 24, 46 } -- capitals, in UI voxels
  c.title_screen.drip = 1                -- BLOOD bleeds
  c.title_screen.logo_cracks = { 0.6, 2.2 } -- cracks per letter: a few in MORE, BLOOD split up
  c.title_screen.props = { { model = "dogtags.vvx", at = { 312, 140 } } }
  c.title_screen.flyby = "huey.vvx"      -- passes over now and then
  c.title_screen.flyby_every = 26
  c.title_screen.flyby_altitude = 46     -- the gunship's height over its pad (c.things.gunship.altitude)

  c.camera.pitch = 35
  c.camera.yaw = 0
  c.camera.fov = 40
  c.camera.view_width = 360        -- voxels across the screen at the player
  c.camera.follow = 0.2
  c.camera.snap = true

  c.light.sun = { 0.35, 0.5, -1.0 }
  c.light.ambient = 0.5
  c.light.shadow = 0.5
  c.light.sky = { 0.09, 0.07, 0.06 }
  c.light.shadow_style = "voxel"  -- "smooth": soft edges; "voxel": every voxel lit or shaded; "voxel_soft": with a half step
  c.light.metal_sweep = 0.45      -- a streak of light running over metal (the title's dog tags) now and then
  c.light.model_shading = "edges" -- figures shade softly like pixel art, their hard edges stay hard
  c.light.model_shading_for = { ["dogtags.vvx"] = "faces", ["huey.vvx"] = "faces" } -- stamped letters and machinery stay crisp

  c.effects.glow = 1.0              -- how strongly flashes, fire and tracers glow
  c.effects.lights = 1.0            -- how strongly flashes and explosions light up their surroundings
  c.effects.haze = 0.35             -- heat haze towards the top of the screen
  c.effects.haze_color = { 0.86, 0.78, 0.62 }
  c.effects.tilt_shift = 0.45       -- blur at the very top and bottom of the screen
  c.effects.screen_flash = 1.0      -- white flash on big explosions
  c.effects.hitstop = 2             -- ticks the game holds still when an enemy dies

  c.audio.volume = 0.8
  -- MML text: edit and save while the game runs. Keys 1 to 4 switch songs.
  -- Earlier songs, out of the list for now: music/jungle_pulse.mml,
  -- music/main_theme.mml, music/theme.mml.
  c.audio.music = { "music/move_on.mml", "music/blood_rush.mml", "music/storm.mml", "music/end_boss.mml" }
  c.audio.music_volume = 0.45

  -- A file in sounds/ replaces the engine's sound of its name (list them:
  -- voxcade sounds games/moreblood --list): sounds/shot.toml with
  -- wave = "noise", freq = 9000, sweep = -31, hold = 0.015, decay = 0.14.

  c.debris.settle = 0.7
  c.debris.max_particles = 30000
  c.debris.gore = 0.8              -- share of a body's voxels that burst blood-red
end
