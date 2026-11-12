-- Pulsar: a space shooter that scrolls up the screen, in Lua alone (no
-- kit): the ship flies on by itself, you steer it across, push ahead or
-- fall back, and shoot; the enemies come in waves on the music's bars and
-- fire on its beats (main.lua). Models, the stars and the asteroids from
-- tools/gen_pulsar.py, the song from music/pulsar_drive.compose.toml.
-- Lengths are in voxels, speeds in voxels a second; the ship is 33 across.
-- Two can fly: a second player joins with START on a pad (or Return at
-- the keyboard), its ship in gold and green.

function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)
  c.title = "Pulsar"
  c.languages = { "en", "de", "fr" }   -- the menus speak them; the keys shown at the start too
  c.scored = true
  c.high_scores = {
    { "ACE", 20000 }, { "VOX", 15000 }, { "SKY", 12000 }, { "ORB", 10000 }, { "ZAP", 8000 },
    { "NEO", 6000 }, { "ION", 4000 }, { "RAY", 3000 }, { "SOL", 2000 }, { "DOT", 1000 },
  }
  c.state.player = { lives = 3, bombs = 3, power = 1 }

  -- The level: a map of stars far below, asteroids, a station to fly over
  -- (tools/gen_pulsar.py); the ship takes off from the launch pad's deck.
  c.world.file = "map.toml"
  c.physics.gravity = 0             -- space: what flies keeps its height
  c.physics.body_width = 6
  c.player.kind = { "ship", "ship2" }
  c.player.spawn = { 240, 60 }
  c.players = 2
  -- Things go as fast as main.lua says at once: the ship's own easing is
  -- the script's.
  c.player.accel = 100000
  c.player.air_accel = 100000

  c.actions = { "fire", "bomb" }
  c.keys = { fire = { "Z", "Space" }, bomb = { "X", "Left Shift" } }
  c.pad = { fire = { "south" }, bomb = { "east" } }

  -- The ship (drawn by main.lua: it banks), shots, enemies. reach: the
  -- radius things are hit within (the ship's small, as in the arcades).
  c.models = { "ship.vvx", "ship2.vvx" }
  c.things.ship = { reach = 5 }
  c.things.ship2 = { like = "ship" }
  c.things.shot = {}
  c.things.bolt = {}
  c.things.fighter = { model = "fighter.vvx", hp = 1, reach = 11, effect = "boom_small.vvx",
                       family = "enemies", props = { worth = 100, fires = 4 } }
  c.things.swooper = { model = "swooper.vvx", hp = 2, reach = 14, effect = "boom_small.vvx",
                       family = "enemies", props = { worth = 150, fires = 0 } }
  c.things.heavy = { model = "heavy.vvx", hp = 8, reach = 17, effect = "boom.vvx",
                     family = "enemies", props = { worth = 500, fires = 2 } }
  c.things.boss = { model = "boss.vvx", hp = 120, reach = 40, effect = "boom_boss.vvx",
                    family = "enemies", props = { worth = 10000, fires = 1 } }
  -- A mine stands where it was laid; it goes off when the ship comes near
  -- (main.lua: ARM), or shot down.
  c.things.mine = { model = "mine.vvx", hp = 3, reach = 9, effect = "boom_small.vvx",
                    family = "enemies", props = { worth = 50, fires = 0 } }
  c.things.power = { model = "power.vvx", reach = 14 }
  -- The fighters and mantas face the way they fly, a diagonal too.
  c.things.fighter.turns = "free"
  c.things.swooper.turns = "free"

  c.camera.pitch = 62               -- from high above, a little ahead
  c.camera.fov = 40
  c.camera.view_height = 340        -- the field from the bottom of the screen to the top, in any window; the panels to the sides take what is left (main.lua: PANEL)
  c.camera.snap = false             -- the stars glide by
  c.camera.follow = 1

  c.light.sun = { 0.4, 0.3, -1.0 }
  c.light.ambient = 0.5
  c.light.shadow = 0.6
  c.light.sky = { 0.01, 0.01, 0.03 }
  c.light.shadow_style = "voxel"
  c.light.model_shading = "faces"
  c.effects.tilt_shift = 0.0
  c.effects.glow = 1.2
  c.effects.haze = 0.0

  c.audio.music = { "music/pulsar_drive.mml" }
  c.audio.music_volume = 0.45
  -- the sounds are files, sounds/NAME.toml, played by their names (level:sound)

  c.title_screen.at = { 240, 700 }   -- out among the asteroids, the ship not yet in sight
  c.title_screen.view_width = 180
  c.title_screen.light_menu = true
  c.title_screen.logo = { "PULSAR" }
  c.title_screen.logo_colors = { "#7fd4ff" }

  -- The HUD's pictures (main.lua draws the HUD): a letter a colour.
  c.icons.life = {
    rows = { "...W...", "..WCW..", ".WWWWW.", "WWW.WWW", "W.....W" },
    colors = { W = { 226, 232, 240 }, C = { 60, 180, 230 } },
  }
  c.icons.bomb = {
    rows = { ".YYY.", "YROY.", "YOOOY", "YOOOY", ".YYY." },
    colors = { Y = { 250, 200, 60 }, O = { 250, 130, 40 }, R = { 255, 255, 220 } },
  }
end
