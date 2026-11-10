#!/usr/bin/env python3
"""Puts shots side by side to look at them together (before/after, several
angles), optionally cropped to a region; with --diff also says how many
pixels differ between the first two and writes where (red on grey).

usage: tile.py OUT.png A.png B.png [...] [--crop X0,Y0,X1,Y1] [--width W] [--diff]
       (--crop in the shots' own pixels; --width scales each tile to that width)
"""
import sys
from PIL import Image, ImageChops


def main(args):
    crop, width, diff, files = None, None, False, []
    it = iter(args)
    for a in it:
        if a == "--crop":
            crop = tuple(int(v) for v in next(it).split(","))
        elif a == "--width":
            width = int(next(it))
        elif a == "--diff":
            diff = True
        else:
            files.append(a)
    if len(files) < 2:
        raise SystemExit(__doc__)
    out, shots = files[0], [Image.open(f).convert("RGB") for f in files[1:]]
    if diff:
        if shots[0].size != shots[1].size:
            raise SystemExit("--diff needs two shots of one size")
        d = ImageChops.difference(shots[0], shots[1]).convert("L").point(lambda v: 255 if v else 0)
        n = d.histogram()[255]
        total = d.width * d.height
        print(f"{n} of {total} pixels differ ({100 * n / total:.3f} %)")
        if n:
            box = d.getbbox()
            print(f"where: x {box[0]}..{box[2]}, y {box[1]}..{box[3]}")
            grey = shots[0].convert("L").convert("RGB")
            red = Image.new("RGB", d.size, (255, 0, 0))
            shots.append(Image.composite(red, grey, d))
    if crop:
        shots = [s.crop(crop) for s in shots]
    if width:
        shots = [s.resize((width, round(s.height * width / s.width)), Image.NEAREST) for s in shots]
    w, h = sum(s.width for s in shots), max(s.height for s in shots)
    sheet = Image.new("RGB", (w, h), (20, 20, 24))
    x = 0
    for s in shots:
        sheet.paste(s, (x, 0))
        x += s.width
    sheet.save(out)
    print(f"wrote {out} ({w} by {h})")


if __name__ == "__main__":
    main(sys.argv[1:])
