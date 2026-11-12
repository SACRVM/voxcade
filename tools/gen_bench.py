#!/usr/bin/env python3
"""Writes games/bench: a load test at 16-bit sprite scale.

More Blood's figures (22 voxels tall, 8-bit scale) blown up to twice the
resolution in every direction (44 tall, the size of a 16-bit sprite, each
with 8 times the voxels), 40 of them and the Huey standing on flat sand,
seen on the title screen. `voxcade shot games/bench --screen title` then
says what a frame costs (`frame: cpu ... | gpu ...`).

East of them, out of the title's view, a field to blow up: two of More
Blood's watchtowers and a hut at the same resolution, with More Blood's
lengths and speeds doubled (weapons, blasts, debris). Throwing grenades
there (`shot --at` in front of them) shows what blasts cost at this scale
(`ticks: ... slowest ...`).

    python3 tools/gen_bench.py [scale]      # scale 2 by default
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vvx  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "games" / "moreblood"
OUT = ROOT / "games" / "bench"
SCALE = int(sys.argv[1]) if len(sys.argv) > 1 else 2
FIGURES = ["player.vvx", "enemy.vvx", "grenadier.vvx", "hostage.vvx"]


OUT.mkdir(exist_ok=True)
# The figures are Voxcade's models (.vvx, tools/vvx.py): each voxel a cube
# of SCALE each way, the signs kept.
k = SCALE
for name in FIGURES + ["huey.vvx"]:
    m = vvx.read(SOURCE / name)
    frames = [{(x * k + a, y * k + b, z * k + c): s for (x, y, z), s in cells.items() for a in range(k) for b in range(k) for c in range(k)} for cells in m["frames"]]
    vvx.write_model(OUT / name, {**m, "size": [n * k for n in m["size"]], "frames": frames}, kept=m["palette"])
    print(f"wrote {OUT / name} ({len(frames[0])} voxels a frame)")

# Prefabs to blow up, without their markers.
(OUT / "prefabs").mkdir(exist_ok=True)
for name in ["tower.vvx", "hut.vvx"]:
    m = vvx.read(SOURCE / "prefabs" / name)
    number = {sign: n + 1 for n, sign in enumerate(m["colours"])}
    voxels = [(x * k + a, y * k + b, z * k + c, number[s]) for (x, y, z), s in m["frames"][0].items() if s in number for a in range(k) for b in range(k) for c in range(k)]
    vvx.write_scene(OUT / "prefabs" / name, [n * k for n in m["size"]], voxels, {number[s]: rgb for s, rgb in m["colours"].items()})
    print(f"wrote {OUT / 'prefabs' / name}")

# Flat sand, wide enough for the view and the field east of it.
w, d = 560 * SCALE, 200 * SCALE
(OUT / "ground").mkdir(exist_ok=True)
Image.fromarray(np.full((d, w), 2, np.uint8), "L").save(OUT / "ground" / "height.png")
vvx.write_map(OUT / "map.toml", (w, d, 48 * SCALE),
              {"height": "ground/height.png", "materials": [{"surface": ["#d6b880", "#dec189", "#cdaf78"], "pattern": 14, "grain": 0.45}]},
              {"tower": "prefabs/tower.vvx", "hut": "prefabs/hut.vvx"},
              [{"prefab": "tower", "at": [440 * SCALE, 110 * SCALE]},
               {"prefab": "hut", "at": [500 * SCALE, 110 * SCALE]},
               {"prefab": "tower", "at": [470 * SCALE, 160 * SCALE]}])

# 40 figures in rows across the view, the Huey behind them.
props = []
for row in range(4):
    for col in range(10):
        x = (40 + col * 18) * SCALE
        y = (60 + row * 16) * SCALE
        props.append(f'    {{ model = "{FIGURES[(row + col) % 4]}", at = {{ {x}, {y} }}, rotation = 180 }},')
props.append(f'    {{ model = "huey.vvx", at = {{ {130 * SCALE}, {150 * SCALE} }}, rotation = 270 }},')
S = SCALE
# The Voxcade these files are for: the engine's own version.
VERSION = next(line.split('"')[1] for line in (ROOT / "Cargo.toml").read_text().splitlines() if line.startswith("version"))
(OUT / "conf.lua").write_text(f"""-- Load test at 16-bit sprite scale (tools/gen_bench.py): More Blood's
-- figures at {SCALE}x the resolution, 40 of them and the Huey. Run
--   voxcade shot games/bench --screen title
-- and read the "frame:" line. Blasts at this scale: grenades at the towers
-- and the hut east of them,
--   voxcade shot games/bench --at {440 * S},{70 * S} --frames 430 --input "up:2;grenade:1;none:45;..."
-- and read the "ticks:" line. Lengths and speeds are More Blood's, doubled.

function conf(c)
  c.voxcade = "{VERSION}"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)
  c.title = "Bench"
  c.kit = "shooter"

  c.world.file = "map.toml"

  c.physics.gravity = {520 * S}
  c.physics.max_fall = {320 * S}
  c.physics.step = {4 * S}
  c.physics.body_width = {6 * S}

  c.player.model = "player.vvx"
  c.player.spawn = {{ {130 * S}, {20 * S} }}
  c.player.speed = {92 * S}
  c.player.accel = {1800 * S}
  c.player.air_accel = {700 * S}
  c.player.jump = {17 * S}
  c.player.muzzle = {{ {7.5 * S}, {12.5 * S} }}

  -- Its weapons (src/shooter/kit.toml's, lengths and speeds doubled); the
  -- player fires the rifle and throws grenades.
  c.weapons.rifle = {{
    label = "Rifle",
    cooldown = 9,
    speed = {340 * S},
    life = 1.0,
    damage = 1,
    blast = {2.8 * S},
    sound = "shot",
  }}

  c.weapons.rockets = {{
    label = "Rockets",
    cooldown = 32,
    speed = {220 * S},
    life = 1.6,
    damage = 6,
    blast = {12 * S},
    rocket = true,
    friendly_fire = false,
    ammo = 8,
    sound = "rocket",
  }}

  c.weapons.grenade = {{
    label = "Grenade",
    throw = {84 * S},
    blast = {14 * S},
    damage = 6,
    friendly_fire = false,
    sound = "throw",
    ammo = 99,
    auto = false,
  }}

  c.state.player = {{ hp = 5, weapon = "rifle", inventory = {{ grenade = 99 }} }}
  c.player.attacks = {{ fire = {{ "held" }}, grenade = {{ "grenade" }} }}
  c.player.moves = {{ "jump", "strafe" }}

  c.debris.settle = 0.7
  c.debris.max_particles = {30000 * S ** 3}
  c.debris.gore = 0.8

  c.camera.pitch = 35
  c.camera.fov = 40
  c.camera.view_width = {360 * SCALE}

  c.light.sun = {{ 0.35, 0.5, -1.0 }}
  c.light.ambient = 0.5
  c.light.shadow = 0.5
  c.light.sky = {{ 0.09, 0.07, 0.06 }}
  c.light.shadow_style = "voxel"
  c.light.model_shading = "edges"
  c.light.model_shading_for = {{ ["huey.vvx"] = "faces" }}

  c.title_screen.at = {{ {130 * SCALE}, {80 * SCALE} }}
  c.title_screen.view_width = {240 * SCALE}
  c.title_screen.logo = {{ " " }}
  c.title_screen.props = {{
{chr(10).join(props)}
  }}
end
""")
print(f"wrote {OUT / 'conf.lua'}")
