// The lamps' pass (render::lamps): a picture's lights added after its
// opaque scene, the screen in tiles: each tile drawn once (or again for
// every LIGHTS_A_TILE more), with the lights that can reach it, each
// pixel's surface read once (fs_main_lamps wrote what each pixel is: the
// colour the light falls on, the ways it faces) and lit by the same sum
// the scene's own shader takes (light.wgsl). A lamp walls stop (`edge.y`)
// lights a face only where no solid voxel of the level lies between the
// middle of the cell in front of the face and the lamp (render::volume,
// the level's solid voxels and coarser copies of them): voxel faces are
// lit or not as a whole, as by the sun in the voxel styles, and each face
// remembers the answer (`seen`) until the level or the lamp moves, so a
// still scene walks its ways once.
//
// What lies between the WALLS marks needs storage written from a fragment
// shader and a 3D texture: a GPU without them draws the lights through
// walls (render::lamps says so once).

struct Lamps {
    inv_view_proj: mat4x4<f32>,
    // xyz: the volume's texels each way (2 voxels each), w: its copies
    dims: vec4<i32>,
    // xyz: the volume's low corner in the level's voxels (render::volume::Reach)
    origin: vec4<i32>,
    // x: the volume's state (changes when it does), y: the picture's
    // number, z: the state before where the volume changed only within
    // `moved_lo` to `moved_hi` since (0: none, every answer of it walked
    // again; clear_of_moved)
    info: vec4<u32>,
    // The moving things (render::movers, for lamps whose shadows are
    // "all"): x how many boxes (0: none).
    movers: vec4<i32>,
    // The box round what changed since the state before, in the level's
    // voxels, grown by render::lamps::MOVED_MARGIN
    moved_lo: vec4<f32>,
    moved_hi: vec4<f32>,
};

@group(0) @binding(0) var<uniform> lamps: Lamps;
@group(0) @binding(1) var scene_depth: texture_depth_2d;
@group(0) @binding(2) var surface: texture_2d<u32>;
// The picture's lights, a row each (render::lamps::LampRow): its four
// parts (light.wgsl's Light) across, then where a lamp walls stop walks
// from (column LAMP_FROM: xyz, w 1 buried; render::volume::Volume::start).
@group(0) @binding(3) var lights: texture_2d<f32>;
// WALLS {
@group(0) @binding(4) var volume: texture_3d<u32>;
@group(0) @binding(5) var<storage, read_write> seen: array<u32>;
// The moving things' solids (render::movers::Solid): the rows of the map
// from the level's voxels into a solid's grid; its cells each way and
// where its bits begin in `mover_bits` (x fastest, then y, then z, 32 a
// word); the thing it is (x its key, y 1; else both 0).
struct Solid {
    to_grid: array<vec4<f32>, 3>,
    grid: vec4<i32>,
    owner: vec4<u32>,
};
@group(0) @binding(6) var<storage, read> mover_solids: array<Solid>;
// The boxes round the moving things, low corner and high in the level's
// voxels, the low corner's w where their solids begin, the high one's how
// many (render::movers::Movers::boxes).
@group(0) @binding(7) var<storage, read> mover_boxes: array<vec4<i32>>;
// Each lamp's list, one lamp's after another (render::movers::Movers::lists;
// its light's `edge.y` says where): the thing it shines from (its key and
// 1, else two 0), then the boxes it can light.
@group(0) @binding(8) var<storage, read> mover_lists: array<u32>;
@group(0) @binding(9) var<storage, read> mover_bits: array<u32>;
// } WALLS

struct TileOut {
    @builtin(position) clip: vec4<f32>,
    // x: how many of the lights below
    @location(0) @interpolate(flat) count: vec4<u32>,
    // the lights' rows in `lights`, two a word (16 bits each)
    @location(1) @interpolate(flat) a: vec4<u32>,
    @location(2) @interpolate(flat) b: vec4<u32>,
    @location(3) @interpolate(flat) c: vec4<u32>,
    @location(4) @interpolate(flat) d: vec4<u32>,
};

// A tile of the screen (`rect`: its low and high corner, -1 to 1 each
// way), as two triangles, with its lights.
@vertex
fn vs_lamp(
    @builtin(vertex_index) i: u32,
    @location(0) rect: vec4<f32>,
    @location(1) count: vec4<u32>,
    @location(2) a: vec4<u32>,
    @location(3) b: vec4<u32>,
    @location(4) c: vec4<u32>,
    @location(5) d: vec4<u32>,
) -> TileOut {
    let corner = array<vec2<f32>, 6>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let k = corner[i];
    var out: TileOut;
    out.clip = vec4<f32>(mix(rect.x, rect.z, k.x), mix(rect.y, rect.w, k.y), 0.0, 1.0);
    out.count = count;
    out.a = a;
    out.b = b;
    out.c = c;
    out.d = d;
    return out;
}

// The way a surface is lit by, as fs_main_lamps packs it (5 bits each in
// the high bits of `w`).
fn lit_way(w: u32) -> vec3<f32> {
    let n = vec3<f32>(vec3<u32>(w >> 16u, w >> 21u, w >> 26u) & vec3<u32>(31u)) / 15.0 - 1.0;
    return normalize(n);
}

// The row of the tile's `k`th light.
fn light_row(in: TileOut, k: u32) -> i32 {
    var word = in.a;
    switch (k >> 3u) {
        case 1u: { word = in.b; }
        case 2u: { word = in.c; }
        case 3u: { word = in.d; }
        default: {}
    }
    return i32((word[(k >> 1u) & 3u] >> ((k & 1u) * 16u)) & 0xffffu);
}

// The light of the row `row` of `rows` (render::lamps::LampRow, or the
// fog's render::fog::FogRow): its four parts across.
fn light_of(rows: texture_2d<f32>, row: i32) -> Light {
    return Light(textureLoad(rows, vec2<i32>(0, row), 0), textureLoad(rows, vec2<i32>(1, row), 0), textureLoad(rows, vec2<i32>(2, row), 0), textureLoad(rows, vec2<i32>(3, row), 0));
}

// Whether fs_main_lamps wrote a surface for the pixel (`s`, its surface).
fn has_surface(s: vec4<u32>) -> bool {
    return (s.x & (1u << 24u)) != 0u;
}

// Where in the level the pixel `px`, its middle at `at` (the picture's
// pixels), shows its surface.
fn surface_point(px: vec2<i32>, at: vec2<f32>) -> vec3<f32> {
    let depth = textureLoad(scene_depth, px, 0);
    let uv = at / vec2<f32>(textureDimensions(scene_depth));
    let p = lamps.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return p.xyz / p.w;
}

@fragment
fn fs_lamp(in: TileOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let s = textureLoad(surface, px, 0);
    if (!has_surface(s)) {
        return vec4<f32>(0.0);
    }
    let world = surface_point(px, in.clip.xy);
    let n = lit_way(s.w);
    // WALLS {
    let f = face_of(s);
    // } WALLS
    var sum = vec3<f32>(0.0);
    for (var k = 0u; k < in.count.x; k += 1u) {
        let row = light_row(in, k);
        let l = light_of(lights, row);
        let lit = light_on(l, world, n);
        if (all(lit <= vec3<f32>(0.0))) {
            continue;
        }
        // WALLS {
        if (l.edge.y > 0.5 && (turned_from(f, n, l, world) || walls_stop(f, l, row) || (l.edge.y > 1.5 && movers_stop_face(movers_from(s, f, n), n, l.pos_radius.xyz, u32(l.edge.y - 2.0))))) {
            continue;
        }
        // } WALLS
        sum += lit;
    }
    return vec4<f32>(unpack4x8unorm(s.x).rgb * sum, 0.0);
}

// WALLS {
// The lamps' asking (render::lamps): before the lamps' pass, a pixel every
// LAMP_ASK each way asks the lamps new to the faces' memory (moved since
// the last picture, or never seen) for its face, so the pass finds the
// answers there: all the pixels of a face asking at once would each walk.
// Asks only; what it draws is nothing.
@fragment
fn fs_lamp_ask(in: TileOut) -> @location(0) vec4<f32> {
    let size = textureDimensions(scene_depth);
    let px = min(vec2<i32>(floor(in.clip.xy)) * i32(LAMP_ASK) + vec2<i32>(i32(LAMP_ASK / 2u)), vec2<i32>(size) - vec2<i32>(1));
    let s = textureLoad(surface, px, 0);
    if (!has_surface(s)) {
        return vec4<f32>(0.0);
    }
    let world = surface_point(px, vec2<f32>(px) + vec2<f32>(0.5));
    let n = lit_way(s.w);
    let f = face_of(s);
    for (var k = 0u; k < in.count.x; k += 1u) {
        let row = light_row(in, k);
        let l = light_of(lights, row);
        if (all(light_on(l, world, n) <= vec3<f32>(0.0)) || turned_from(f, n, l, world)) {
            continue;
        }
        _ = walls_stop(f, l, row);
    }
    return vec4<f32>(0.0);
}

// A pixel's face as the walls take it (fs_main_lamps wrote its surface):
// whether off the grid (1), the cell in front of it, its way (FACE_DIRS'
// index; 7 off the grid), whether it is a face of the level itself (its
// voxel in the volume), and the cell's part of each lamp's key in the
// memory (`stopped`).
struct Face {
    off: u32,
    cell: vec3<i32>,
    way: u32,
    of_level: bool,
    ids: vec2<u32>,
};

fn face_of(s: vec4<u32>) -> Face {
    let off = (s.x >> 25u) & 1u;
    let cell = surface_cell(s);
    let way = select((s.x >> 26u) & 7u, 7u, off == 1u);
    let of_level = way < 6u && solid(cell - vec3<i32>(FACE_DIRS[way]) - lamps.origin.xyz, 0);
    return Face(off, cell, way, of_level, cell_ids(cell, off));
}

// The cell `cell`'s part of each lamp's key in the memory (`stopped`), a
// face off the grid's (`off` 1) apart from one on it.
fn cell_ids(cell: vec3<i32>, off: u32) -> vec2<u32> {
    return vec2<u32>(mix_hash(0x9e3779b9u + off, cell), mix_hash(0x85ebca6bu + off, cell));
}

// Whether the face `f`, lit by the way `n` at `world`, is a face of the
// level turned away from the lamp `l`: it gets nothing of a lamp behind
// it, a way to the lamp would only go through a gap.
fn turned_from(f: Face, n: vec3<f32>, l: Light, world: vec3<f32>) -> bool {
    return f.of_level && dot(n, l.pos_radius.xyz - world) <= 0.0;
}

// Whether walls stop the lamp `l` of the row `row` lighting the face `f`
// (the memory asked, `stopped`).
fn walls_stop(f: Face, l: Light, row: i32) -> bool {
    return stopped(f.cell, f.off, textureLoad(lights, vec2<i32>(LAMP_FROM, row), 0), f.ids, vec2<u32>(l.edge.zw));
}

// The cell in front of a pixel's face as fs_main_lamps writes it: on the
// grid x and y whole words, off the grid 20 bits each (the rest of them
// where its way to a moving thing begins, `movers_from`); z 16 bits.
fn surface_cell(s: vec4<u32>) -> vec3<i32> {
    let z = (bitcast<i32>(s.w) << 16u) >> 16u;
    if (((s.x >> 25u) & 1u) == 1u) {
        return vec3<i32>((bitcast<i32>(s.y) << 12u) >> 12u, (bitcast<i32>(s.z) << 12u) >> 12u, z);
    }
    return vec3<i32>(bitcast<i32>(s.y), bitcast<i32>(s.z), z);
}

// Where a face's way to a lamp whose shadows are "all" begins: a face of
// the level at the middle of the cell in front of it, as for the walls, so
// a figure's feet shade the floor at their side; any other face just out
// of its own middle (FACE_OUT), a voxel lit as a whole (smooth: lit by `n`,
// not its face's way) a voxel out of the voxel's middle along `n`; exactly,
// off the grid as fs_main_lamps wrote it (`s`, the pixel's surface). A face
// of a part turning slowly begins its way a little further each picture,
// so it changes once as a shadow's edge passes, and no face's way begins
// in a part beside it.
fn movers_from(s: vec4<u32>, f: Face, n: vec3<f32>) -> vec3<f32> {
    let middle = vec3<f32>(f.cell) + vec3<f32>(0.5);
    if (f.of_level) {
        return middle;
    }
    if (f.way == 7u) {
        let q = vec3<u32>((s.y >> 20u) & 1023u, (s.z >> 20u) & 1023u, (s.y >> 30u) | ((s.z >> 30u) << 2u) | ((s.x >> 26u) << 4u));
        return vec3<f32>(f.cell) + vec3<f32>(q) / 256.0 - vec3<f32>(MOVERS_FROM);
    }
    let face = FACE_DIRS[f.way];
    let centre = middle - face;
    return select(centre + n, centre + face * (0.5 + FACE_OUT), all(n == face));
}

// Whether a moving thing stops a lamp whose shadows are "all" (at `lamp`,
// its boxes `list`) lighting a face whose way to it begins at `begins`
// (`movers_from`) and that is lit by the way `n`: a face turned away from
// the lamp is in its own shadow (as a face of the level is from any lamp
// walls stop), but a face of the thing the lamp shines from (`carried`):
// that walks as any other (`movers_stop`), the thing's own solids passed
// over, so its own edges darken none of it.
fn movers_stop_face(begins: vec3<f32>, n: vec3<f32>, lamp: vec3<f32>, list: u32) -> bool {
    return (dot(n, lamp - begins) <= 0.0 && !carried(begins - n * (0.5 + FACE_OUT), list)) || movers_stop(begins, lamp, list, 0xffffffffu);
}

// How many boxes a lamp's list `list` names (render::movers::Movers::lists:
// its word, where its list begins and how many): every box where it says
// EVERY_BOX.
fn listed(list: u32) -> u32 {
    let n = list & EVERY_BOX;
    return select(n, u32(lamps.movers.x), n == EVERY_BOX);
}

// The `i`th box the lamp's list `list` names.
fn listed_box(list: u32, i: u32) -> u32 {
    return select(mover_lists[(list >> MOVERS_COUNT_BITS) + MOVERS_HEAD + i], i, (list & EVERY_BOX) == EVERY_BOX);
}

// The thing the lamp of the list `list` shines from, its list's head: its
// key and 1, else two 0.
fn list_owner(list: u32) -> vec2<u32> {
    let head = list >> MOVERS_COUNT_BITS;
    return vec2<u32>(mover_lists[head], mover_lists[head + 1u]);
}

// The point `p` of the level in the grid of the solid `m`.
fn into_grid(m: Solid, p: vec3<f32>) -> vec3<f32> {
    let r = m.to_grid;
    return vec3<f32>(dot(r[0].xyz, p) + r[0].w, dot(r[1].xyz, p) + r[1].w, dot(r[2].xyz, p) + r[2].w);
}

// Where along a way the box whose planes it crosses at `a` and `b` (each
// axis's two, along the way) holds it, from 0 up to `far`: from (x) to
// (y); to not past from: nowhere.
fn slab_span(a: vec3<f32>, b: vec3<f32>, far: f32) -> vec2<f32> {
    return vec2<f32>(max(max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z)), 0.0), min(min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z)), far));
}

// Whether the point `p` (a face's voxel, `movers_stop_face`) lies in a
// voxel of the thing the lamp of `list` shines from.
fn carried(p: vec3<f32>, list: u32) -> bool {
    if (lamps.movers.x == 0) {
        return false;
    }
    let own = list_owner(list);
    if (own.y != 1u) {
        return false;
    }
    for (var i = 0u; i < listed(list); i += 1u) {
        let k = listed_box(list, i);
        let lo = mover_boxes[2u * k];
        let hi = mover_boxes[2u * k + 1u];
        if (any(p < vec3<f32>(lo.xyz)) || any(p > vec3<f32>(hi.xyz))) {
            continue;
        }
        for (var j = u32(lo.w); j < u32(lo.w + hi.w); j += 1u) {
            let m = mover_solids[j];
            if (all(m.owner.xy == own) && solid_holds(j, vec3<i32>(floor(into_grid(m, p))))) {
                return true;
            }
        }
    }
    return false;
}

// Whether the cell `c` of the solid `k` holds a voxel.
fn solid_holds(k: u32, c: vec3<i32>) -> bool {
    let g = mover_solids[k].grid;
    if (any(c < vec3<i32>(0)) || any(c >= g.xyz)) {
        return false;
    }
    let i = u32((c.z * g.y + c.y) * g.x + c.x);
    return ((mover_bits[u32(g.w) + i / 32u] >> (i % 32u)) & 1u) == 1u;
}

// Whether a voxel of the solid `k` lies on the way from `lamp` along `dir`
// (in the level's voxels) up to `len`: the way taken into the solid's grid
// and walked through its cells a cell a step, closed between two of them
// touching only at an edge as `walk` closes the level's. The lamp in it (a
// car's headlight in its body) leaves it first (HOUSING).
fn solid_stops(k: u32, lamp: vec3<f32>, dir: vec3<f32>, len: f32) -> bool {
    let m = mover_solids[k];
    let r = m.to_grid;
    let a = into_grid(m, lamp);
    let way = vec3<f32>(dot(r[0].xyz, dir), dot(r[1].xyz, dir), dot(r[2].xyz, dir));
    let d = select(way, vec3<f32>(1e-7), abs(way) < vec3<f32>(1e-7));
    let inv = 1.0 / d;
    let size = m.grid.xyz;
    let span = slab_span(-a * inv, (vec3<f32>(size) - a) * inv, len);
    var t = span.x;
    let end = span.y;
    // In its housing while the walk has met nothing but what holds it.
    var housed = t <= 0.0;
    let ahead = select(vec3<i32>(-1), vec3<i32>(1), d > vec3<f32>(0.0));
    // A cell a step: past the grid's cells along every axis, which no way
    // reaches, stopped.
    for (var i = 0; i < size.x + size.y + size.z + 3; i += 1) {
        if (t >= end) {
            return false;
        }
        let c = clamp(vec3<i32>(floor(a + d * t)), vec3<i32>(0), size - vec3<i32>(1));
        if (solid_holds(k, c)) {
            if (!housed || t > HOUSING) {
                return true;
            }
        } else {
            housed = false;
        }
        let ts = (vec3<f32>(select(c, c + vec3<i32>(1), d > vec3<f32>(0.0))) - a) * inv;
        let out = min(ts.x, min(ts.y, ts.z));
        // Leaving through an edge or a corner between two of its voxels
        // touching only there: closed, as `walk`.
        let tied = (ts - vec3<f32>(out)) < vec3<f32>(1e-3);
        if (!housed && u32(tied.x) + u32(tied.y) + u32(tied.z) > 1u) {
            let beside = u32(tied.x && solid_holds(k, c + vec3<i32>(ahead.x, 0, 0))) + u32(tied.y && solid_holds(k, c + vec3<i32>(0, ahead.y, 0))) + u32(tied.z && solid_holds(k, c + vec3<i32>(0, 0, ahead.z)));
            if (beside > 1u) {
                return true;
            }
        }
        t = max(out, t) + 0.001;
    }
    return true;
}

// Whether a moving thing (render::movers) stands between the lamp at
// `lamp` and the point `to` (a face's, `movers_from`; a point of a beam),
// for a lamp whose shadows are "all": each solid of the boxes the lamp can
// light (`list`; of the first 32 only those `mask` has a bit for) whose
// box the way meets, walked (`solid_stops`), but the solids of the thing
// the lamp shines from (a lantern a figure carries).
fn movers_stop(to: vec3<f32>, lamp: vec3<f32>, list: u32, mask: u32) -> bool {
    let d = to - lamp;
    let len = length(d);
    if (lamps.movers.x == 0 || len < 0.5) {
        return false;
    }
    let dir = select(d / len, vec3<f32>(1e-7), abs(d / len) < vec3<f32>(1e-7));
    let inv = 1.0 / dir;
    let own = list_owner(list);
    for (var i = 0u; i < listed(list); i += 1u) {
        if (i < 32u && ((mask >> i) & 1u) == 0u) {
            continue;
        }
        let k = listed_box(list, i);
        let lo = mover_boxes[2u * k];
        let hi = mover_boxes[2u * k + 1u];
        let span = slab_span((vec3<f32>(lo.xyz) - lamp) * inv, (vec3<f32>(hi.xyz) - lamp) * inv, len);
        if (span.x >= span.y) {
            continue;
        }
        for (var j = u32(lo.w); j < u32(lo.w + hi.w); j += 1u) {
            if (own.y == 1u && all(mover_solids[j].owner.xy == own)) {
                continue;
            }
            if (solid_stops(j, lamp, dir, len)) {
                return true;
            }
        }
    }
    return false;
}

// Whether the cell `c` of the copy `level` (2^level voxels across) holds
// anything solid; outside the volume nothing does.
fn solid(c: vec3<i32>, level: i32) -> bool {
    let t = c >> vec3<u32>(1u);
    let dims = lamps.dims.xyz >> vec3<u32>(u32(level));
    if (any(t < vec3<i32>(0)) || any(t >= dims)) {
        return false;
    }
    let bits = textureLoad(volume, t, level).r;
    let b = u32((c.x & 1) | ((c.y & 1) << 1u) | ((c.z & 1) << 2u));
    return ((bits >> b) & 1u) == 1u;
}

// How far along the way `dir` from `origin` it first leaves solid voxels,
// looking at most `most` voxels on: 0 when it starts in the open, -1 when
// it is still in solid voxels there (a face off the grid, RIM).
fn leave_solid(origin: vec3<f32>, dir: vec3<f32>, most: f32) -> f32 {
    let inv = 1.0 / dir;
    var t = 0.0;
    for (var i = 0; i < 64; i += 1) {
        let c = vec3<i32>(floor(origin + dir * t));
        if (!solid(c, 0)) {
            return t;
        }
        let lo = vec3<f32>(c);
        let ts = (select(lo, lo + vec3<f32>(1.0), dir > vec3<f32>(0.0)) - origin) * inv;
        t = min(ts.x, min(ts.y, ts.z)) + 0.001;
        if (t > most) {
            break;
        }
    }
    return -1.0;
}

// A face off the grid looks past the solid its own voxels fill round it
// (render::volume grows a turned piece by half a voxel), this far.
const RIM: f32 = 2.0;

// Whether a solid voxel lies on the way to the middle of the cell `cell`
// (in front of a face) from where the lamp walks from (`start`, its row's
// LAMP_FROM: the lamp itself, or out of the solid voxels it sits in; w 1:
// buried, stopped everywhere): walked through the cells, past empty space
// by the coarser copies, begun at the copy whose cells are about as long
// as the way; down a copy into a cell holding anything, up one only once
// out of the cell above the one just left (not back into a cell found
// solid: a way grazing a floor would go down and up again at every voxel,
// two steps a voxel). Its steps are bounded by its cells, twice those
// along it at copy 0: past them, which no way reaches, stopped.
fn walk(cell: vec3<i32>, off: u32, start: vec4<f32>) -> bool {
    if (start.w > 0.5) {
        return true;
    }
    // From the volume's low corner, as its cells are.
    let to = vec3<f32>(cell - lamps.origin.xyz) + vec3<f32>(0.5);
    let lamp = start.xyz - vec3<f32>(lamps.origin.xyz);
    let d = to - lamp;
    let len = length(d);
    if (len < 0.5) {
        return false;
    }
    // No way exactly along an axis: its zero a tiny step on, so the walk's
    // crossings all lie ahead.
    let dir = select(d / len, vec3<f32>(1e-7), abs(d / len) < vec3<f32>(1e-7));
    var end = len;
    if (off == 1u) {
        let rim = leave_solid(to, -dir, RIM);
        end = len - max(rim, 0.0);
    }
    let inv = 1.0 / dir;
    let top = lamps.dims.w - 1;
    var t = 0.0;
    var level = clamp(i32(floor(log2(max(end, 1.0)))), 0, top);
    let most = 2 * i32(abs(d.x) + abs(d.y) + abs(d.z)) + 4 * top + 8;
    for (var i = 0; i < most; i += 1) {
        if (t >= end) {
            return false;
        }
        let p = lamp + dir * t;
        let size = f32(1 << u32(level));
        let c = vec3<i32>(floor(p / size));
        if (solid(c, level)) {
            if (level == 0) {
                return true;
            }
            level -= 1;
        } else {
            let lo = vec3<f32>(c) * size;
            let ts = (select(lo, lo + vec3<f32>(size), dir > vec3<f32>(0.0)) - lamp) * inv;
            let out = min(ts.x, min(ts.y, ts.z));
            // Leaving through an edge or a corner where two of the cells
            // across the faces it meets there are solid (a wall of voxels
            // touching only at their edges, a diagonal), it passes between
            // them: closed, as the faces they leave between them are. One
            // solid there is a wall it only grazes.
            let tied = (ts - vec3<f32>(out)) < vec3<f32>(1e-3 * size);
            if (u32(tied.x) + u32(tied.y) + u32(tied.z) > 1u) {
                let ahead = select(vec3<i32>(-1), vec3<i32>(1), dir > vec3<f32>(0.0));
                let beside = u32(tied.x && solid(c + vec3<i32>(ahead.x, 0, 0), level)) + u32(tied.y && solid(c + vec3<i32>(0, ahead.y, 0), level)) + u32(tied.z && solid(c + vec3<i32>(0, 0, ahead.z), level));
                if (beside > 1u) {
                    if (level == 0) {
                        return true;
                    }
                    level -= 1;
                    continue;
                }
            }
            t = max(out, t) + 0.001;
            // Out of the cell above through a face it crosses: the far one
            // of its two cells that way (odd going up, even going down).
            if (level < top && any(tied & (((c & vec3<i32>(1)) == vec3<i32>(1)) == (dir > vec3<f32>(0.0))))) {
                level += 1;
            }
        }
    }
    return true;
}

// The faces' memory of what stops lamps: entries of two words (as many as
// the buffer holds, render::lamps::faces_for the picture's size): a key of
// the cell in front of a face and the lamp's exact place; and whether
// stopped (bit 0), the volume's state (9 bits), the picture last asked (6
// bits) and a second key (16 bits, never 0); then the counts of answers
// that found the memory crowded (GROW_AT) and of those that found no room
// (render::lamps makes it larger, or says it). An entry not asked this
// picture or the last (STALE) makes room and is walked again: everything a
// picture shows is asked every picture, and a lamp that moves asks under a
// new key each picture, so its old entries go at once and two headlights
// in fog do not fill the memory. Plain loads and stores, no atomics:
// pixels of a face read through the caches; an entry another pixel is
// writing reads as no answer by its second key (a word of another face's
// matches one time in 65535), so every answer is the walk's for its key
// and the picture does not depend on what the memory holds.
const PROBES: u32 = 32u;
// An answer finding no room within this many slots counts the memory as
// crowded: render::lamps makes it larger while it is small for the
// picture, so its lookups stay short (grown only when one finds no room at
// all, within PROBES, a memory a quarter of the picture's pixels stays
// crowded and the beams of a hall in fog take 10 to 15 % longer than with
// room to spare).
const GROW_AT: u32 = 8u;
const STALE: u32 = 1u;

fn mix_hash(seed: u32, c: vec3<i32>) -> u32 {
    var h = seed ^ (bitcast<u32>(c.x) * 0x27d4eb2du);
    h = ((h ^ (h >> 15u)) * 0x2c1b3c6du) ^ bitcast<u32>(c.y);
    h = ((h ^ (h >> 12u)) * 0x297a2d39u) ^ bitcast<u32>(c.z);
    h = (h ^ (h >> 15u)) * 0x27d4eb2du;
    return h ^ (h >> 16u);
}

// Whether the entry's word was asked within STALE pictures of `now`.
fn fresh(word: u32, now: u32) -> bool {
    return ((now - (word >> 10u)) & 0x3fu) <= STALE;
}

// Whether the way to the middle of `cell` from `start` (a lamp's LAMP_FROM)
// passes no nearer what changed in the volume since the state before than
// a walk reads (the box round it, grown by render::lamps::MOVED_MARGIN):
// every cell the walk reads is as it was, so an answer of the state before
// is the walk's. A lamp buried, or no state before kept: never.
fn clear_of_moved(cell: vec3<i32>, start: vec4<f32>) -> bool {
    if (lamps.info.z == 0u || start.w > 0.5) {
        return false;
    }
    let d = vec3<f32>(cell) + vec3<f32>(0.5) - start.xyz;
    // (Along an axis hardly: the way's place on it decides, as by a slab.)
    let inv = 1.0 / select(d, vec3<f32>(1e-6), abs(d) < vec3<f32>(1e-6));
    let t0 = (lamps.moved_lo.xyz - start.xyz) * inv;
    let t1 = (lamps.moved_hi.xyz - start.xyz) * inv;
    let near = max(max(min(t0.x, t1.x), min(t0.y, t1.y)), max(min(t0.z, t1.z), 0.0));
    let far = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), min(max(t0.z, t1.z), 1.0));
    return near > far;
}

// Whether walls stop the lamp at `lamp` lighting the face in front of
// `cell`: `ids` the cell's part of the key, `lamp_ids` the lamp's (a hash of
// its exact place, render::shader::LightData::of), so a key costs a few
// operations a lamp.
fn stopped(cell: vec3<i32>, off: u32, lamp: vec4<f32>, ids: vec2<u32>, lamp_ids: vec2<u32>) -> bool {
    let key = (ids.x ^ (lamp_ids.x * 0x9e3779b1u) ^ (lamp_ids.y << 8u)) | 1u;
    let tag = (((ids.y ^ (lamp_ids.y * 0x85ebca77u) ^ lamp_ids.x) % 65535u) + 1u) << 16u;
    let state = (lamps.info.x & LAMP_STATES) << 1u;
    let now = lamps.info.y & 0x3fu;
    let faces = (arrayLength(&seen) - LAMP_COUNT_WORDS) / LAMP_ANSWER_WORDS;
    var free = faces;
    var at = 0u;
    // The slot from the key's bits mixed again (the key is odd, `| 1`: the
    // even slots stood empty), the next ones a step apart that the second
    // key gives (odd, so every slot of a memory a power of two long comes
    // in turn; one slot after the other, the entries of a full stretch
    // crowd on, and at 42 in 100 taken a few in a million find no room).
    // (`faces` is a power of two: render::lamps::faces_for, grown twice as
    // large.) The first empty slot ends a lookup; a stale one does not: an
    // answer behind it would be walked again and kept twice, and the beams
    // of a hall in fog would take a quarter longer.
    var slot = (((key ^ (key >> 15u)) * 0x2c1b3c6du) ^ (key >> 13u)) & (faces - 1u);
    let hop = (tag >> 15u) | 1u;
    for (var i = 0u; i < PROBES; i += 1u) {
        let k = slot * LAMP_ANSWER_WORDS;
        let found = seen[k];
        let word = seen[k + 1u];
        if (found == key) {
            if ((word & 0xffff0000u) != tag) {
                // Being written by another pixel of the face, or another
                // face's whose key is the same: walked here; one asked long
                // ago is this one's from now on, or a key written with
                // another's second key (two pixels at once) stood so.
                let b = walk(cell, off, lamp);
                if (!fresh(word, now)) {
                    seen[k + 1u] = tag | (now << 10u) | state | select(0u, 1u, b);
                }
                return b;
            }
            if ((word & (LAMP_STATES << 1u)) == state && fresh(word, now)) {
                if (((word >> 10u) & 0x3fu) != now) {
                    seen[k + 1u] = (word & 0xffff03ffu) | (now << 10u);
                }
                return (word & 1u) == 1u;
            }
            // asked in the state before, its way nowhere near what changed
            // since (a figure posed anew): still the walk's, now this one's
            if ((word & (LAMP_STATES << 1u)) == ((lamps.info.z & LAMP_STATES) << 1u) && fresh(word, now) && clear_of_moved(cell, lamp)) {
                seen[k + 1u] = (word & 0xffff0001u) | (now << 10u) | state;
                return (word & 1u) == 1u;
            }
            // asked before the level changed, or long ago: walked again
            let b = walk(cell, off, lamp);
            seen[k + 1u] = tag | (now << 10u) | state | select(0u, 1u, b);
            return b;
        }
        if (found == 0u || !fresh(word, now)) {
            if (free == faces) {
                free = slot;
                at = i;
            }
            if (found == 0u) {
                break;
            }
        }
        slot = (slot + hop) & (faces - 1u);
    }
    let b = walk(cell, off, lamp);
    if (free == faces || at >= GROW_AT) {
        seen[faces * LAMP_ANSWER_WORDS] += 1u;
    }
    if (free == faces) {
        seen[faces * LAMP_ANSWER_WORDS + 1u] += 1u;
        return b;
    }
    seen[free * LAMP_ANSWER_WORDS] = key;
    seen[free * LAMP_ANSWER_WORDS + 1u] = tag | (now << 10u) | state | select(0u, 1u, b);
    return b;
}
// } WALLS
