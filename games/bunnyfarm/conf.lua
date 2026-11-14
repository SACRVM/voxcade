-- BunnyFarm: a farming game with a tamagotchi at its heart
-- (docs/bunnyfarm.md). You are the bunny: sow and harvest the field, sell
-- at the market stall, eat a carrot when hungry, sleep in the burrow. A
-- day runs from 6:00 to 2:00 in about 14 minutes; the world goes on while
-- you play and stands still while the game is closed. Made as voxels
-- (tools/gen_bunnyfarm.py first built the blocks, the farm and the
-- figures; they are yours to shape in the editors now).
function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)
  c.title = "BunnyFarm"
  c.languages = { "de", "en", "fr" }
  -- Saved as the bunny goes to sleep; CONTINUE on the title goes on.
  c.saves = { slots = 3, version = 6 }   -- 2: world.state.awake; 3: beds as tables, rows, bag, can, hand; 4: kitchen, recipes, mail; 5: barn; 6: year, collection
  -- The arcade's table scores farm years: what a year added to the
  -- farm's worth (year.lua). A year's end ends the run with that score;
  -- CONTINUE goes on into the next.
  c.scored = true
  c.high_scores = { { "BUN", 40000 }, { "HOP", 30000 }, { "CAT", 22000 }, { "DOG", 16000 }, { "FOX", 12000 },
                    { "EWE", 8000 }, { "HEN", 5000 }, { "COW", 3000 }, { "OWL", 1500 }, { "MOO", 500 } }

  -- The farm: a level of voxel blocks, 16 voxels a cell, seen from above.
  c.world.blocks = { file = "levels/01_farm.toml" }
  c.physics.gravity = 400
  c.physics.body_width = 6

  -- The bunny: steered by main.lua, hopping as it walks.
  c.player.kind = "bunny"
  c.things.bunny = {
    model = "bunny.vvx", height = 20, width = 8,
    frames = { idle = { 0, 0 }, walk = { first = 1, last = 4, stride = 3 } },
  }
  -- A bed's crop (main.lua draws it in its stage), the burrow, the stall
  -- and what stands about the farm, placed in the level (moved in the
  -- level editor). props.solid: half its footprint each way, which the
  -- bunny walks round.
  c.things.crop = { width = 14, height = 12 }
  c.things.burrow = { model = "burrow.vvx", width = 40, height = 20, props = { solid = { 23, 15 } } }
  c.things.stall = { model = "stall.vvx", width = 34, height = 28, props = { solid = { 18, 6 } } }
  c.things.tree = { model = "tree.vvx", width = 8, height = 36, props = { solid = { 4, 4 } } }
  c.things.pine = { model = "pine.vvx", width = 8, height = 34, props = { solid = { 4, 4 } } }
  c.things.bush = { model = "bush.vvx", width = 12, height = 9, props = { solid = { 7, 5 } } }
  c.things.rock = { model = "rock.vvx", width = 12, height = 6, props = { solid = { 6, 4 } } }
  c.things.hay = { model = "hay.vvx", width = 12, height = 9, props = { solid = { 6, 4 } } }
  c.things.barrel = { model = "barrel.vvx", width = 10, height = 12, props = { solid = { 5, 5 } } }
  c.things.crate = { model = "crate.vvx", width = 10, height = 9, props = { solid = { 5, 5 } } }
  c.things.scarecrow = { model = "scarecrow.vvx", width = 4, height = 26, props = { solid = { 2, 2 } } }
  -- the farm's gate (open: the bunny goes through) and the barrier at the
  -- road's end (nobody goes on past it)
  c.things.farm_gate = { model = "farm_gate.vvx", width = 30, height = 26 }
  c.things.barrier = { model = "barrier.vvx", width = 30, height = 14, props = { solid = { 36, 6 } } }
  c.things.kitchen = { model = "kitchen.vvx", width = 30, height = 24, props = { solid = { 16, 6 } } }
  c.things.mailbox = { model = "mailbox.vvx", width = 8, height = 16, props = { solid = { 3, 3 } } }
  c.things.cat = { model = "cat.vvx", width = 8, height = 20, props = { solid = { 4, 4 } } }
  -- The barn (stage 5): the dog who sells animals and the fox who buys
  -- them, each pen's sign (props.pen: which) and the stables built there,
  -- the animals themselves (main.lua draws each by its colour and state;
  -- they walk about their pen, through the stable's footprint never).
  c.things.dog = { model = "dog.vvx", width = 8, height = 20, props = { solid = { 4, 4 } } }
  c.things.fox = { model = "fox.vvx", width = 8, height = 20, props = { solid = { 4, 4 } } }
  for i, sp in ipairs({ "chicken", "sheep", "cow" }) do
    c.things["sign_" .. sp] = { model = "pen_sign.vvx", width = 10, height = 16, props = { solid = { 6, 2 }, picture = i - 1 } }
  end
  c.things.trough_water = { model = "trough.vvx", width = 12, height = 5, props = { solid = { 7, 3 }, picture = 0 } }
  c.things.trough_grain = { model = "trough.vvx", width = 12, height = 5, props = { solid = { 7, 3 }, picture = 1 } }
  c.things.coop = { model = "coop.vvx", width = 20, height = 22, props = { solid = { 11, 9 } } }
  c.things.shed = { model = "shed.vvx", width = 28, height = 22, props = { solid = { 15, 9 } } }
  c.things.barn = { model = "barn.vvx", width = 32, height = 32, props = { solid = { 17, 11 } } }
  c.things.egg = { model = "egg.vvx", width = 4, height = 5 }
  c.things.chicken = { width = 6, height = 10 }
  c.things.sheep = { width = 9, height = 12 }
  c.things.cow = { width = 10, height = 15 }
  -- The crops (data.lua's), each a model of its four stages, dry and wet.
  local CROPS = { "carrot", "wheat", "blueberry", "tomato", "radish", "cauliflower", "turnip", "cabbage", "eggplant", "cucumber", "pumpkin" }
  -- The barn's animals, a model a colour (main.lua draws each as it is:
  -- young or grown, walking, at work, its product showing).
  local COLOURS = { chicken = { "white", "brown", "red", "blue", "green", "rainbow" }, sheep = { "white", "brown", "black" },
                    cow = { "light", "brown", "pink", "purple", "green" } }

  -- What the bag holds: seeds, fertilizer and the harvest (carrots are
  -- money, apart).
  for i, crop in ipairs(CROPS) do
    c.items["seed_" .. crop] = { stack = 99, order = i }
    if crop ~= "carrot" then
      c.items[crop] = { stack = 99, order = 30 + i }
    end
  end
  c.items.fertilizer = { stack = 99, order = 20 }
  c.items.jar = { stack = 99, order = 21 }
  c.items.wood = { stack = 99, order = 22 }   -- mends a gap in a pen's fence (wolf.lua)
  for i, dish in ipairs({ "bread", "jam", "sauce", "soup", "spread", "pickles" }) do
    c.items[dish] = { stack = 99, order = 50 + i }
  end
  c.items.moonberry = { stack = 99, order = 45 }   -- blueberries harvested on a full-moon night
  for i, product in ipairs({ "egg", "rainbow_egg", "wool", "milk" }) do
    c.items[product] = { stack = 99, order = 60 + i }
  end

  -- Keys and buttons: act on what is in front, back out of a menu, choose
  -- the seed, open the bag.
  c.actions = { "act", "back", "prev", "next", "bag" }
  c.keys = { act = { "Space", "Z" }, back = { "X", "Backspace" }, prev = { "Q" }, next = { "E" }, bag = { "Tab", "I" } }
  c.pad = { act = { "south" }, back = { "east" }, prev = { "leftshoulder" }, next = { "rightshoulder" }, bag = { "north" } }

  -- Seen from above at a slant, following the bunny over the farm (the
  -- meadow round it reaches past the picture's edges).
  c.camera.pitch = 50
  c.camera.fov = 40
  c.camera.view_width = 320
  c.camera.view_at_least = { 320, 200 }   -- at least what a 320 by 200 game showed, more as the screen's shape gives
  c.camera.stop_at_edges = true            -- out through the gate, the view stops at the meadow's edges

  -- The day's light is main.lua's (frame:daylight); this is noon.
  c.light.sun = { 0.35, 0.6, -1.0 }
  c.light.ambient = 0.5
  c.light.shadow = 0.45
  c.light.sky = { 0.45, 0.62, 0.85 }
  c.light.shadow_style = "voxel"
  c.effects.lights = 1.5

  -- The title: the farm at dawn, the bunny at its burrow under the logo,
  -- the field on the right, the menu in light letters over the meadow.
  c.title_screen.at = { 504, 278 }
  c.title_screen.view_width = 360
  -- The logo is a model of its own: thick round letters, the Y a carrot,
  -- ears on the B (tools/gen_bunnyfarm.py; shaped in the voxel editor).
  c.title_screen.logo_model = "logo.vvx"
  c.title_screen.logo_bob = 2   -- the letters bob gently, a wave through them
  c.title_screen.light_menu = true

  -- Bright by day, calm by night (main.lua chooses), from the library.
  c.audio.music = { "music/sunny_hills.mml", "music/quiet_warehouse.mml" }
  c.audio.music_volume = 0.4

  -- the sounds are files, sounds/NAME.toml, played by their names (level:sound)

  -- The HUD's pictures, 7 by 7: a letter a colour, . empty.
  local GOLD, GOLD_DARK, GOLD_LIGHT = { 244, 200, 70 }, { 184, 132, 40 }, { 255, 236, 150 }
  local GREEN, ORANGE = { 86, 160, 64 }, { 236, 126, 44 }
  c.icons.coin = { rows = { "..OOO..", ".OYYYO.", "OYYWYYO", "OYYWYYO", "OYYWYYO", ".OYYYO.", "..OOO.." },
                   colors = { O = GOLD_DARK, Y = GOLD, W = GOLD_LIGHT } }
  c.icons.carrot = { rows = { "....G.G", ".....GG", "...OOO.", "..OOOO.", ".OOOO..", ".OO....", "O......" },
                     colors = { G = GREEN, O = ORANGE } }
  c.icons.food = c.icons.carrot
  c.icons.wheat = { rows = { "..Y.Y..", ".YYYYY.", "..YYY..", "Y.YYY.Y", ".YYYYY.", "...S...", "...S..." },
                    colors = { Y = { 232, 190, 84 }, S = { 186, 140, 60 } } }
  c.icons.blueberry = { rows = { "..G.G..", "...G...", ".BBBBB.", "BBBbBBB", "BBBBBBB", ".BBBBB.", "..BBB.." },
                        colors = { G = GREEN, B = { 76, 96, 196 }, b = { 40, 50, 120 } } }
  c.icons.tomato = { rows = { "..G.G..", "...G...", ".RRRRR.", "RRRRRRR", "RRRRRRR", ".RRRRR.", "..RRR.." },
                     colors = { G = GREEN, R = { 222, 62, 50 } } }
  local PINK, CREAM, PURPLE, LEAF, PUMPKIN = { 230, 70, 110 }, { 246, 240, 220 }, { 120, 64, 150 }, { 120, 190, 90 }, { 236, 140, 40 }
  c.icons.radish = { rows = { "..G.G..", "...G...", "..RRR..", ".RRRRR.", ".RRRRR.", "..RRR..", "...W..." },
                     colors = { G = GREEN, R = PINK, W = CREAM } }
  c.icons.cauliflower = { rows = { ".......", ".WWWWW.", "WWWWWWW", "GWWWWWG", "GGWWWGG", ".GGGGG.", "..GGG.." },
                          colors = { G = GREEN, W = CREAM } }
  c.icons.turnip = { rows = { "..G.G..", "...G...", ".PPPPP.", "PPPPPPP", "WWWWWWW", ".WWWWW.", "...W..." },
                     colors = { G = GREEN, P = PURPLE, W = CREAM } }
  c.icons.cabbage = { rows = { ".GGGGG.", "GLLLLLG", "GLLGLLG", "GLGLGLG", "GLLGLLG", "GLLLLLG", ".GGGGG." },
                      colors = { G = GREEN, L = LEAF } }
  c.icons.eggplant = { rows = { ".GGG...", "..GG...", "..PPP..", ".PPPPP.", ".PPPPP.", ".PPPPP.", "..PPP.." },
                       colors = { G = GREEN, P = PURPLE } }
  c.icons.cucumber = { rows = { "......G", ".....GG", "....GLG", "...GLG.", "..GLG..", ".GLG...", "GGG...." },
                       colors = { G = GREEN, L = LEAF } }
  c.icons.pumpkin = { rows = { "...S...", "..SS...", ".OOOOO.", "OOdOdOO", "OOdOdOO", "OOdOdOO", ".OOOOO." },
                      colors = { S = GREEN, O = PUMPKIN, d = { 200, 110, 30 } } }
  c.icons.can = { rows = { ".......", "..CCC..", ".C...CS", "MMMMM.S", "MMMMMSS", "MMMMM..", "MMMMM.." },
                  colors = { M = { 120, 160, 190 }, C = { 90, 120, 150 }, S = { 120, 160, 190 } } }
  c.icons.fertilizer = { rows = { "..TTT..", ".BBBBB.", "BBBBBBB", "BGGBGGB", "BBGGGBB", "BBBGBBB", ".BBBBB." },
                         colors = { B = { 168, 140, 196 }, T = { 120, 96, 150 }, G = GREEN } }
  c.icons.wood = { rows = { ".......", "PPPPPP.", "DDDDDD.", ".PPPPPP", ".DDDDDD", "PPPPPP.", "DDDDDD." },
                   colors = { P = { 200, 150, 96 }, D = { 142, 98, 60 } } }
  c.icons.slot = { rows = { ".......", ".SSSSS.", ".S...S.", ".S.+.S.", ".S...S.", ".SSSSS.", "......." },
                   colors = { S = { 140, 100, 64 }, ["+"] = { 90, 160, 70 } } }
  -- The dishes, and an empty jar.
  local GLASS, LID, CRUST = { 200, 226, 236 }, { 150, 104, 66 }, { 206, 150, 80 }
  local function jar_of(fill) return { rows = { ".LLLLL.", ".GGGGG.", "GFFFFFG", "GFFFFFG", "GFFFFFG", "GFFFFFG", ".GGGGG." }, colors = { L = LID, G = GLASS, F = fill } } end
  c.icons.bread = { rows = { ".......", "..CCC..", ".CBCBC.", "CBBBBBC", "CCCCCCC", ".CCCCC.", "......." },
                    colors = { C = CRUST, B = { 238, 200, 130 } } }
  c.icons.jam = jar_of({ 86, 70, 170 })
  c.icons.sauce = jar_of({ 210, 60, 46 })
  c.icons.soup = jar_of({ 220, 160, 70 })
  c.icons.spread = jar_of({ 150, 90, 160 })
  c.icons.pickles = jar_of({ 90, 150, 70 })
  c.icons.jar = { rows = { ".LLLLL.", ".GGGGG.", "G.....G", "G.....G", "G.....G", "G.....G", ".GGGGG." }, colors = { L = LID, G = GLASS } }
  c.icons.pot = { rows = { ".......", "S.....S", "PPPPPPP", "PPPPPPP", "PPPPPPP", ".PPPPP.", "..P.P.." },
                  colors = { P = { 92, 90, 100 }, S = { 70, 68, 76 } } }
  c.icons.letter = { rows = { ".......", "WWWWWWW", "WRWWWRW", "WWRWRWW", "WWWRWWW", "WWWWWWW", "......." },
                     colors = { W = { 246, 240, 226 }, R = { 204, 70, 56 } } }
  c.icons.check = { rows = { ".......", "......G", ".....GG", "G...GG.", "GG.GG..", ".GGG...", "..G...." },
                    colors = { G = { 86, 160, 64 } } }
  c.icons.field = { rows = { ".......", "DDDDDDD", "SSSSSSS", "DDDDDDD", "SSSSSSS", "DDDDDDD", "......." },
                    colors = { D = { 120, 82, 54 }, S = { 86, 58, 38 } } }
  -- A seed bag a crop, tied at the top, the crop's colour on it.
  for crop, color in pairs({ carrot = ORANGE, wheat = { 232, 190, 84 }, blueberry = { 76, 96, 196 }, tomato = { 222, 62, 50 },
                             radish = PINK, cauliflower = CREAM, turnip = PURPLE, cabbage = LEAF, eggplant = PURPLE,
                             cucumber = GREEN, pumpkin = PUMPKIN }) do
    c.icons["seed_" .. crop] = { rows = { "..S.S..", "...S...", ".BBBBB.", "BBBBBBB", "BBCCCBB", "BBCCCBB", ".BBBBB." },
                                 colors = { B = { 196, 158, 104 }, S = { 120, 84, 52 }, C = color } }
  end
  c.icons.energy = { rows = { "....YY.", "...YY..", "..YY...", ".YYYYY.", "...YY..", "..YY...", ".YY...." },
                     colors = { Y = { 250, 206, 60 } } }
  c.icons.mood = { rows = { ".RR.RR.", "RRRRRRR", "RRRRRRR", ".RRRRR.", "..RRR..", "...R...", "......." },
                   colors = { R = { 232, 96, 120 } } }
  c.icons.sun = { rows = { "Y..Y..Y", ".YYYYY.", ".YYYYY.", "YYYYYYY", ".YYYYY.", ".YYYYY.", "Y..Y..Y" },
                  colors = { Y = { 248, 196, 60 } } }
  c.icons.moon = { rows = { "..MMM..", ".MMM...", "MMM....", "MMM....", "MMM....", ".MMM...", "..MMM.." },
                   colors = { M = { 200, 210, 240 } } }
  -- The week's events (data.lua's EVENTS) and the full moon's berry.
  c.icons.full_moon = { rows = { "..OOO..", ".OMMMO.", "OMMmMMO", "OMMMMmO", "OmMMMMO", ".OMMMO.", "..OOO.." },
                        colors = { O = { 140, 152, 196 }, M = { 234, 238, 252 }, m = { 190, 198, 228 } } }
  c.icons.moonberry = { rows = { "..G.G..", "...G...", ".BBBBB.", "BBBbBBB", "BBBBBBB", ".BBBBB.", "..BBB.." },
                        colors = { G = GREEN, B = { 196, 206, 244 }, b = { 120, 130, 196 } } }
  c.icons.rain = { rows = { ".......", "..CCC..", ".CCCCC.", "CCCCCCC", ".......", ".D.D.D.", "D.D.D.." },
                   colors = { C = { 204, 210, 222 }, D = { 110, 160, 224 } } }
  c.icons.lazy = { rows = { "ZZZZ...", "..Z....", ".Z.....", "ZZZZ...", "....ZZZ", ".....Z.", "....ZZZ" },
                   colors = { Z = { 150, 170, 232 } } }
  c.icons.cake = { rows = { "...R...", "..WWW..", ".WWWWW.", "SSSSSSS", "WWWWWWW", "SSSSSSS", "......." },
                   colors = { R = { 214, 50, 60 }, W = { 250, 244, 236 }, S = { 226, 176, 104 } } }
  -- The barn's: each animal's head in its colour (a darker line round it
  -- on the paper), its products.
  local function darker(c) return { c[1] * 7 // 10, c[2] * 7 // 10, c[3] * 7 // 10 } end
  local INK, COMB_RED, BEAK_YELLOW = { 40, 32, 30 }, { 216, 56, 50 }, { 242, 180, 60 }
  local FEATHERS = { white = { 244, 240, 228 }, brown = { 176, 112, 64 }, red = { 198, 84, 52 }, blue = { 98, 138, 214 }, green = { 102, 174, 94 } }
  for colour, w in pairs(FEATHERS) do
    c.icons["chicken_" .. colour] = { rows = { "...R...", "..RRO..", ".OWWWO.", ".WEWEW.", ".WWBWW.", ".OWRWO.", "..OOO.." },
                                      colors = { W = w, O = darker(w), R = COMB_RED, E = INK, B = BEAK_YELLOW } }
  end
  c.icons.chicken_rainbow = { rows = { "...R...", "..RRO..", ".aaaaa.", ".bEbEb.", ".ccBcc.", ".ddddd.", "..eee.." },
                              colors = { R = COMB_RED, O = { 160, 40, 36 }, E = INK, B = BEAK_YELLOW, a = { 222, 64, 60 }, b = { 242, 150, 50 },
                                         c = { 244, 210, 70 }, d = { 96, 180, 84 }, e = { 84, 128, 220 } } }
  local WOOLS = { white = { { 244, 240, 232 }, { 96, 80, 78 } }, brown = { { 156, 108, 72 }, { 86, 70, 66 } }, black = { { 70, 64, 70 }, { 236, 218, 200 } } }
  for colour, wf in pairs(WOOLS) do
    c.icons["sheep_" .. colour] = { rows = { ".OWWWO.", "OWWWWWO", "FFWWWFF", ".FEFEF.", ".FFFFF.", "..FPF..", "..FFF.." },
                                    colors = { W = wf[1], O = darker(wf[1]), F = wf[2], E = colour == "black" and INK or { 250, 248, 240 }, P = { 240, 160, 160 } } }
  end
  local HIDES = { light = { 238, 230, 212 }, brown = { 152, 98, 62 }, pink = { 238, 156, 178 }, purple = { 156, 118, 206 }, green = { 122, 192, 112 } }
  for colour, h in pairs(HIDES) do
    c.icons["cow_" .. colour] = { rows = { "H.....H", "OBBBBBO", ".BEBEB.", ".BBBBB.", ".MMMMM.", ".MNMNM.", "..MMM.." },
                                  colors = { H = { 236, 226, 198 }, B = h, O = darker(h), E = INK, M = { 240, 192, 180 }, N = { 82, 70, 66 } } }
  end
  local SHELL, SHELL_EDGE = { 250, 246, 236 }, { 196, 176, 150 }
  c.icons.egg = { rows = { "..OOO..", ".OWWWO.", "OWWWWWO", "OWWWWWO", "OWWWWWO", ".OWWWO.", "..OOO.." },
                  colors = { O = SHELL_EDGE, W = SHELL } }
  c.icons.rainbow_egg = { rows = { "..OOO..", ".OaaaO.", "ObbbbbO", "OcccccO", "OdddddO", ".OeeeO.", "..OOO.." },
                          colors = { O = SHELL_EDGE, a = { 222, 64, 60 }, b = { 242, 150, 50 }, c = { 244, 210, 70 }, d = { 96, 180, 84 }, e = { 84, 128, 220 } } }
  c.icons.wool = { rows = { "..O.O..", ".OWOWO.", "OWWWWWO", "OWWWWWO", "OWWWWWO", ".OWWWO.", "..OOO.." },
                   colors = { O = { 176, 166, 156 }, W = { 248, 246, 240 } } }
  c.icons.milk = { rows = { "..CCC..", "..OWO..", ".OWWWO.", ".OWWWO.", ".OBBBO.", ".OWWWO.", ".OOOOO." },
                   colors = { C = { 90, 140, 210 }, O = { 160, 166, 180 }, W = { 250, 250, 248 }, B = { 90, 140, 210 } } }
  c.icons.stable = { rows = { "...R...", "..RRR..", ".RRRRR.", "RRRRRRR", ".WWDWW.", ".WWDWW.", ".WWDWW." },
                     colors = { R = { 184, 64, 52 }, W = { 172, 122, 76 }, D = { 104, 70, 44 } } }
  c.icons.heart = c.icons.mood
  -- a drop of water, rising from a bed watered
  c.icons.drop = { rows = { "...B...", "...B...", "..BBB..", ".BBBBB.", ".BBBBB.", ".BBBBB.", "..BBB.." },
                   colors = { B = { 86, 156, 230 } } }
  c.icons.bag = { rows = { "..S.S..", "...S...", ".BBBBB.", "BBBBBBB", "BBBBBBB", "BBBBBBB", ".BBBBB." },
                  colors = { B = { 196, 158, 104 }, S = { 120, 84, 52 } } }
  c.icons.sprout = { rows = { ".......", ".LL.LL.", "LLL.LLL", ".LLSLL.", "...S...", "...S...", ".BBBBB." },
                     colors = { L = LEAF, S = GREEN, B = { 120, 82, 54 } } }
end
