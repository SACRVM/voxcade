#!/usr/bin/env python3
"""A toolkit for voxel figures and props written by a script: clean voxel
forms (boxes with their edges cut back, limbs of square section, a tidy
pass), metal colours, a face, and the writers for frames and figures of
parts. One colour a material: the engine does light and shade.

The templates' and Voxkeep's figures are small and plain, built from
boxes by tools/gen_voxkeep.py (the user, 2026-10-23: after a try at
16-bit size, "the 8-bit variants had more charm", and they are easy to
make); this is for new figures made the same way, by hand in the editor
or with a script. A game's model files are the user's once there:
`KEEP_EXISTING` keeps them. Plain Python, no numpy; imported, not run."""

import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gen_sokovox import write_parts, write_vox  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# A face's colours (`face`).
EYE, GLINT = (30, 24, 34), (250, 248, 240)
BLUSH, MOUTH = (246, 150, 136), (150, 58, 58)


# ------------------------------------------------------------- the toolkit

def norm(v):
    length = math.sqrt(sum(c * c for c in v)) or 1.0
    return tuple(c / length for c in v)


class Fig:
    """Voxels by place, each a colour; later shapes go over earlier ones
    unless `under`."""

    def __init__(self, size):
        self.size = size
        self.v = {}

    def put(self, p, c, under=False):
        x, y, z = p
        if 0 <= x < self.size[0] and 0 <= y < self.size[1] and 0 <= z < self.size[2]:
            if under and p in self.v:
                return
            self.v[p] = c

    def shape(self, inside, lo, hi, paint, under=False):
        """Fills every voxel whose middle lies inside: `inside(p)` gives a
        normal there (or None outside), `paint(p, n)` the colour."""
        for x in range(max(0, math.floor(lo[0])), min(self.size[0], math.ceil(hi[0]) + 1)):
            for y in range(max(0, math.floor(lo[1])), min(self.size[1], math.ceil(hi[1]) + 1)):
                for z in range(max(0, math.floor(lo[2])), min(self.size[2], math.ceil(hi[2]) + 1)):
                    p = (x + 0.5, y + 0.5, z + 0.5)
                    n = inside(p)
                    if n is not None:
                        c = paint((x, y, z), n) if callable(paint) else paint
                        if c is not None:
                            self.put((x, y, z), c, under)

    def ellipsoid(self, c, r, paint, under=False, cut=None):
        def inside(p):
            d = [(p[i] - c[i]) / r[i] for i in range(3)]
            if sum(k * k for k in d) > 1.0 or (cut and cut(p)):
                return None
            return norm(tuple(d[i] / r[i] for i in range(3)))
        self.shape(inside, [c[i] - r[i] for i in range(3)], [c[i] + r[i] for i in range(3)], paint, under)

    def capsule(self, a, b, ra, rb=None, paint=None, under=False):
        """A limb from a to b, its radius going from ra to rb."""
        rb = ra if rb is None else rb
        ab = [b[i] - a[i] for i in range(3)]
        length2 = sum(k * k for k in ab) or 1e-9

        def inside(p):
            ap = [p[i] - a[i] for i in range(3)]
            t = max(0.0, min(1.0, sum(ap[i] * ab[i] for i in range(3)) / length2))
            q = [a[i] + ab[i] * t for i in range(3)]
            d = [p[i] - q[i] for i in range(3)]
            r = ra + (rb - ra) * t
            if sum(k * k for k in d) > r * r:
                return None
            return norm(d)
        r = max(ra, rb)
        lo = [min(a[i], b[i]) - r for i in range(3)]
        hi = [max(a[i], b[i]) + r for i in range(3)]
        self.shape(inside, lo, hi, paint, under)

    def cylinder(self, cx, cy, z0, z1, r0, r1, paint, under=False):
        """Upright, flat at both ends, elliptical: radii r0 (x, y) at z0
        going to r1 at z1."""
        def inside(p):
            if not z0 <= p[2] <= z1:
                return None
            t = (p[2] - z0) / max(z1 - z0, 1e-9)
            rx, ry = r0[0] + (r1[0] - r0[0]) * t, r0[1] + (r1[1] - r0[1]) * t
            dx, dy = (p[0] - cx) / rx, (p[1] - cy) / ry
            if dx * dx + dy * dy > 1.0:
                return None
            # The rims at top and bottom face up and down.
            if p[2] > z1 - 1.0:
                return (0, 0, 1)
            if p[2] < z0 + 1.0:
                return (0, 0, -1)
            return norm((dx, dy, 0.0))
        r = max(r0 + r1)
        self.shape(inside, (cx - r, cy - r, z0), (cx + r, cy + r, z1), paint, under)

    def box(self, lo, hi, paint, under=False):
        """Voxels lo..hi (inclusive) on every axis."""
        for x in range(lo[0], hi[0] + 1):
            for y in range(lo[1], hi[1] + 1):
                for z in range(lo[2], hi[2] + 1):
                    c = paint((x, y, z), (0, 0, 1)) if callable(paint) else paint
                    self.put((x, y, z), c, under)

    def recolour(self, test, paint):
        for p, c in list(self.v.items()):
            if test(p):
                self.v[p] = paint(p, c) if callable(paint) else paint

    def rbox(self, lo, hi, paint, bevel=1, under=False, axes=3):
        """A box of whole voxels lo..hi (inclusive) with its edges and
        corners cut back `bevel` voxels: a clean, deliberate voxel form
        (a head, a chest), where an ellipsoid would leave ragged steps.
        `paint` may leave a voxel out (None). `axes` 2: only the upright
        edges cut (a belt, a hem: round seen from above, square in height)."""
        for x in range(lo[0], hi[0] + 1):
            for y in range(lo[1], hi[1] + 1):
                for z in range(lo[2], hi[2] + 1):
                    inset = [min(p - a, b - p) for p, a, b in zip((x, y, z), lo, hi)][:axes]
                    if sum(max(0, bevel - d) for d in inset) <= bevel:
                        c = paint((x, y, z), (0, 0, 1)) if callable(paint) else paint
                        if c is not None:
                            self.put((x, y, z), c, under)

    def limb(self, xs, a, b, width, paint, under=False, across=0):
        """A limb of square section: the voxels `xs` (a range) across, and
        in the plane across them (y, z) every voxel within `width` / 2 of
        the line from `a` to `b` (y, z) and between its ends. It swings in
        that plane only, so it steps as a clean line does at any angle.
        `across` 1: `xs` are y columns and the line lies in x, z (a wing
        spread to the sides)."""
        (ay, az), (by, bz) = a, b
        dy, dz = by - ay, bz - az
        length2 = dy * dy + dz * dz or 1e-9
        r = width / 2
        for y in range(math.floor(min(ay, by) - r), math.ceil(max(ay, by) + r) + 1):
            for z in range(math.floor(min(az, bz) - r), math.ceil(max(az, bz) + r) + 1):
                py, pz = y + 0.5 - ay, z + 0.5 - az
                t = (py * dy + pz * dz) / length2
                if not -0.01 <= t <= 1.01:
                    continue
                if abs(py * dz - pz * dy) / math.sqrt(length2) <= r:
                    for x in xs:
                        p = (x, y, z) if across == 0 else (y, x, z)
                        c = paint(p, (0, 0, 1)) if callable(paint) else paint
                        self.put(p, c, under)

    def beam(self, a, b, width, paint, under=False):
        """A limb of square section along any line from `a` to `b` (x, y,
        z): every voxel whose middle lies within `width` / 2 of the line
        across both ways (one way level, the other square to it) and
        between its ends. For a limb that leaves the y-z plane (an arm
        reaching across to a rifle)."""
        d = [b[i] - a[i] for i in range(3)]
        length = math.sqrt(sum(k * k for k in d)) or 1e-9
        u = [k / length for k in d]
        side = (u[1], -u[0], 0.0)
        n = math.hypot(side[0], side[1])
        side = (1.0, 0.0, 0.0) if n < 1e-6 else (side[0] / n, side[1] / n, 0.0)
        up = (side[1] * u[2] - side[2] * u[1], side[2] * u[0] - side[0] * u[2], side[0] * u[1] - side[1] * u[0])
        r = width / 2
        lo = [math.floor(min(a[i], b[i]) - r - 1) for i in range(3)]
        hi = [math.ceil(max(a[i], b[i]) + r + 1) for i in range(3)]
        for x in range(lo[0], hi[0] + 1):
            for y in range(lo[1], hi[1] + 1):
                for z in range(lo[2], hi[2] + 1):
                    q = (x + 0.5 - a[0], y + 0.5 - a[1], z + 0.5 - a[2])
                    t = sum(q[i] * u[i] for i in range(3)) / length
                    if -0.01 <= t <= 1.01 and abs(sum(q[i] * side[i] for i in range(3))) <= r and abs(sum(q[i] * up[i] for i in range(3))) <= r:
                        c = paint((x, y, z), (0, 0, 1)) if callable(paint) else paint
                        self.put((x, y, z), c, under)

    def tidy(self):
        """Cleans the voxelling: a hole a voxel wide that five sides close
        round is filled (with the colour most of them have), a voxel with
        no neighbour at all goes."""
        dirs = ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))
        holes = {}
        for (x, y, z) in self.v:
            for dx, dy, dz in dirs:
                q = (x + dx, y + dy, z + dz)
                if q not in self.v:
                    holes[q] = holes.get(q, 0) + 1
        for q, n in holes.items():
            if n >= 5:
                around = [self.v[(q[0] + dx, q[1] + dy, q[2] + dz)] for dx, dy, dz in dirs if (q[0] + dx, q[1] + dy, q[2] + dz) in self.v]
                self.put(q, max(set(around), key=around.count))
        for p in [p for p in self.v if not any((p[0] + dx, p[1] + dy, p[2] + dz) in self.v for dx, dy, dz in dirs)]:
            del self.v[p]

    def surface(self, p):
        """Whether a voxel shows: an empty neighbour."""
        x, y, z = p
        return any((x + dx, y + dy, z + dz) not in self.v for dx, dy, dz in
                   ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)))


# Colours that shine as metal (`metal`): MagicaVoxel's metal material in
# the file, the engine's sky reflection and highlight on them.
METALS = {}


def metal(rgb, shine=0.8, rough=0.3):
    """`rgb` as metal: buckles, rivets, a blade."""
    METALS[rgb] = {"_type": "_metal", "_metal": str(shine), "_rough": str(rough)}
    return rgb


# Files already there are kept (`gen_voxkeep.py`: a game's figures are the
# user's once made, edited in the voxel editor).
KEEP_EXISTING = False


def kept(path):
    """Whether `path` stays as it is (KEEP_EXISTING and there)."""
    if KEEP_EXISTING and path.exists():
        print(f"{path}: kept (there already: yours, edited in the editor)")
        return True
    return False


def save(path, size, figs):
    """Frames into one model file; colours by value, metal ones with their
    material."""
    if kept(path):
        return
    index, palette, frames = {}, {}, []
    for f in figs:
        frame = []
        for (x, y, z), c in sorted(f.v.items()):
            i = index.setdefault(c, len(index) + 1)
            palette[i] = c
            frame.append((x, y, z, i))
        frames.append(frame)
    assert len(index) <= 255, f"{path}: {len(index)} colours"
    write_vox(path, size, frames, palette, {i: METALS[c] for c, i in index.items() if c in METALS})
    shown = path.relative_to(ROOT) if path.is_relative_to(ROOT) else path
    print(f"{shown}: {len(frames)} frames, {len(index)} colours")


def save_parts(path, parts):
    """A figure of parts (the engine turns each about its joint: c.things.
    NAME.swing, frame:model's pose): `parts` as (name, Fig) in the order
    they are drawn, a cell drawn by a later part taken from the earlier
    ones, as in one figure. The figure's size is its parts' box."""
    if kept(path):
        return
    owner = {}
    for name, f in parts:
        for p, c in f.v.items():
            owner[p] = (name, c)
    index, palette, out = {}, {}, []
    for name, _ in parts:
        cells = {p: c for p, (n, c) in owner.items() if n == name}
        if not cells:
            continue
        low = tuple(min(p[a] for p in cells) for a in range(3))
        high = tuple(max(p[a] for p in cells) for a in range(3))
        size = tuple(high[a] - low[a] + 1 for a in range(3))
        voxels = []
        for p, c in sorted(cells.items()):
            i = index.setdefault(c, len(index) + 1)
            palette[i] = c
            voxels.append((p[0] - low[0], p[1] - low[1], p[2] - low[2], i))
        out.append((name, size, voxels, low))
    assert len(index) <= 255, f"{path}: {len(index)} colours"
    write_parts(path, out, palette, {i: METALS[c] for c, i in index.items() if c in METALS})
    print(f"{path}: {len(out)} parts, {len(index)} colours")


def joint(start, length, angle, x=None):
    """The point `length` from `start` at `angle` degrees from straight down,
    forward (+y) positive, in the plane x = start's."""
    a = math.radians(angle)
    return (start[0] if x is None else x, start[1] + length * math.sin(a), start[2] - length * math.cos(a))


def joint2(start, length, angle):
    """The point (y, z) `length` from `start` at `angle` degrees from
    straight down, forward (+y) positive."""
    a = math.radians(angle)
    return (start[0] + length * math.sin(a), start[1] - length * math.cos(a))


def face(f, skin, eyes, mouth, nose, brow=None, blush=BLUSH):
    """A face on the head's front (+y), as the user shaped Voxkeep's hero
    in the editor (`eyes`: the eyes' lower row and their two columns):
    each eye a dark pupil two voxels tall with its white a single voxel
    on its outer side, the upper row; nothing round the corner (from the
    side only the white shows at the face's edge: a pupil there or more
    white read as four eyes from the front, which the user rejected);
    brows over the pupils (`brow`); a nose standing out a voxel in the
    middle at `nose`; a mouth at `mouth`; a blush under each eye. `skin`
    is the head's colour."""
    low, left, right = eyes
    middle = (left + right) // 2

    def front(z, x):
        ys = [y for (xx, y, w), c in f.v.items() if xx == x and w == z and c == skin]
        return max(ys) if ys else None

    for pupil, out in ((left, -1), (right, 1)):
        for z in (low, low + 1):
            y = front(z, pupil)
            if y is not None:
                f.put((pupil, y, z), EYE)
        y = front(low + 1, pupil + out)
        if y is not None:
            f.put((pupil + out, y, low + 1), GLINT)
        if brow:
            y = front(low + 2, pupil)
            if y is not None:
                f.put((pupil, y, low + 2), brow)
        y = front(low - 1, pupil)
        if blush and y is not None:
            f.put((pupil, y, low - 1), blush)
    y = front(nose, middle)
    if y is not None:
        f.put((middle, y + 1, nose), skin)
    y = front(mouth, middle)
    if y is not None:
        f.put((middle, y, mouth), MOUTH)


def ik(s, h, l1, l2, pole):
    """Where the elbow (or knee) goes between s and h for bones l1 and l2,
    bending towards `pole`."""
    d = [h[i] - s[i] for i in range(3)]
    dist = min(math.sqrt(sum(k * k for k in d)), (l1 + l2) * 0.999)
    u = norm(d)
    a = (l1 * l1 + dist * dist - l2 * l2) / (2 * l1 * dist)
    a = max(-1.0, min(1.0, a))
    along = sum(pole[i] * u[i] for i in range(3))
    n = norm(tuple(pole[i] - along * u[i] for i in range(3)))
    return tuple(s[i] + u[i] * l1 * a + n[i] * l1 * math.sqrt(1 - a * a) for i in range(3))
