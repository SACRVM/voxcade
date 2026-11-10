#!/usr/bin/env python3
"""Voxcade's icon: the logo's V in the pixel font, a voxel deep, on the
title screens' dark (the website's favicon, bigger). Writes src/icon.png
(256 by 256: the windows' icon, and the desktop entry's on Linux) and
dist/voxcade.ico (Windows: the program's icon, build.rs).

    python3 tools/make_icon.py
"""
from PIL import Image, ImageDraw

BG, FRONT, SIDE = "#050509", "#e8a040", "#7a5420"
V = ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."]
CELL = 28   # 9 cells across: the V's 5, two either side

icon = Image.new("RGBA", (9 * CELL, 9 * CELL), (0, 0, 0, 0))
d = ImageDraw.Draw(icon)
d.rounded_rectangle((0, 0, icon.width - 1, icon.height - 1), CELL, fill=BG)
cells = [(x, y) for y, row in enumerate(V) for x, c in enumerate(row) if c == "#"]
for colour, shift in ((SIDE, CELL // 2), (FRONT, 0)):
    for x, y in cells:
        x0, y0 = (x + 2) * CELL + shift - CELL // 4, (y + 1) * CELL + shift - CELL // 4
        d.rectangle((x0, y0, x0 + CELL - 1, y0 + CELL - 1), fill=colour)
icon = icon.resize((256, 256), Image.LANCZOS)
icon.save("src/icon.png", optimize=True)
icon.save("dist/voxcade.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("src/icon.png, dist/voxcade.ico")
