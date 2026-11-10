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
import json
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "games" / "moreblood"
OUT = ROOT / "games" / "bench"
SCALE = int(sys.argv[1]) if len(sys.argv) > 1 else 2
FIGURES = ["player.vox", "enemy.vox", "grenadier.vox", "hostage.vox"]


def chunks(data, at, end):
    while at < end:
        cid, content, children = struct.unpack_from("<4sii", data, at)
        yield cid, data[at + 12:at + 12 + content], at + 12 + content, at + 12 + content + children
        at += 12 + content + children


def read_vox(path):
    """Frames as (size, [(x, y, z, index)]) and the 256-colour palette."""
    data = path.read_bytes()
    assert data[:4] == b"VOX ", path
    _, main, start, end = next(chunks(data, 8, len(data)))
    frames, palette, size = [], None, None
    for cid, content, _, _ in chunks(data, start, end):
        if cid == b"SIZE":
            size = struct.unpack("<3i", content)
        elif cid == b"XYZI":
            n = struct.unpack_from("<i", content)[0]
            frames.append((size, [struct.unpack_from("<4B", content, 4 + 4 * k) for k in range(n)]))
        elif cid == b"RGBA":
            palette = content
    return frames, palette


def write_vox(path, frames, palette):
    def chunk(cid, content, children=b""):
        return cid + struct.pack("<ii", len(content), len(children)) + content + children
    body = b""
    for size, voxels in frames:
        body += chunk(b"SIZE", struct.pack("<3i", *size))
        body += chunk(b"XYZI", struct.pack("<i", len(voxels)) + b"".join(struct.pack("<4B", *v) for v in voxels))
    body += chunk(b"RGBA", palette)
    path.write_bytes(b"VOX " + struct.pack("<i", 150) + chunk(b"MAIN", b"", body))


def blow_up(frames, k):
    out = []
    for (sx, sy, sz), voxels in frames:
        assert max(sx, sy, sz) * k <= 256, "too big for a .vox model"
        out.append(((sx * k, sy * k, sz * k), [(x * k + a, y * k + b, z * k + c, i)
                                              for x, y, z, i in voxels for a in range(k) for b in range(k) for c in range(k)]))
    return out


OUT.mkdir(exist_ok=True)
for name in FIGURES + ["huey.vox"]:
    frames, palette = read_vox(SOURCE / name)
    write_vox(OUT / name, blow_up(frames, SCALE), palette)
    print(f"wrote {OUT / name} ({len(frames[0][1]) * SCALE ** 3} voxels a frame)")

# Prefabs to blow up, without their markers (palette slots 240 and up).
(OUT / "prefabs").mkdir(exist_ok=True)
for name in ["tower.vox", "hut.vox"]:
    frames, palette = read_vox(SOURCE / "prefabs" / name)
    frames = [(size, [v for v in voxels if v[3] < 240]) for size, voxels in frames]
    write_vox(OUT / "prefabs" / name, blow_up(frames, SCALE), palette)
    print(f"wrote {OUT / 'prefabs' / name}")

# Flat sand, wide enough for the view and the field east of it.
w, d = 560 * SCALE, 200 * SCALE
(OUT / "ground").mkdir(exist_ok=True)
Image.fromarray(np.full((d, w), 2, np.uint8), "L").save(OUT / "ground" / "height.png")
json.dump({"size": [w, d, 48 * SCALE],
           "ground": {"height": "ground/height.png",
                      "materials": [{"surface": ["#d6b880", "#dec189", "#cdaf78"], "pattern": 14, "grain": 0.45}]},
           "prefabs": {"tower": "prefabs/tower.vox", "hut": "prefabs/hut.vox"},
           "place": [{"prefab": "tower", "at": [440 * SCALE, 110 * SCALE]},
                     {"prefab": "hut", "at": [500 * SCALE, 110 * SCALE]},
                     {"prefab": "tower", "at": [470 * SCALE, 160 * SCALE]}]},
          open(OUT / "map.json", "w"))

# 40 figures in rows across the view, the Huey behind them.
props = []
for row in range(4):
    for col in range(10):
        x = (40 + col * 18) * SCALE
        y = (60 + row * 16) * SCALE
        props.append(f'    {{ model = "{FIGURES[(row + col) % 4]}", at = {{ {x}, {y} }}, turn = 2 }},')
props.append(f'    {{ model = "huey.vox", at = {{ {130 * SCALE}, {150 * SCALE} }}, turn = 1 }},')
S = SCALE
(OUT / "conf.lua").write_text(f"""-- Load test at 16-bit sprite scale (tools/gen_bench.py): More Blood's
-- figures at {SCALE}x the resolution, 40 of them and the Huey. Run
--   voxcade shot games/bench --screen title
-- and read the "frame:" line. Blasts at this scale: grenades at the towers
-- and the hut east of them,
--   voxcade shot games/bench --at {440 * S},{70 * S} --frames 430 --input "up:2;grenade:1;none:45;..."
-- and read the "ticks:" line. Lengths and speeds are More Blood's, doubled.

function conf(c)
  c.title = "Bench"
  c.kit = "shooter"

  c.world.file = "map.json"

  c.physics.gravity = {520 * S}
  c.physics.max_fall = {320 * S}
  c.physics.step = {4 * S}
  c.physics.body_width = {6 * S}

  c.player.model = "player.vox"
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
  c.light.model_shading_for = {{ ["huey.vox"] = "faces" }}

  c.title_screen.at = {{ {130 * SCALE}, {80 * SCALE} }}
  c.title_screen.view_width = {240 * SCALE}
  c.title_screen.logo = {{ " " }}
  c.title_screen.props = {{
{chr(10).join(props)}
  }}
end
""")
print(f"wrote {OUT / 'conf.lua'}")
