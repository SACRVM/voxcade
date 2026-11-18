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
    // number
    info: vec4<u32>,
    // The moving things' volume (render::movers, for lamps whose shadows
    // are "all"): xyz its texels each way, w its copies (0: none); and its
    // low corner in the level's voxels.
    movers_dims: vec4<i32>,
    movers_origin: vec4<i32>,
};

@group(0) @binding(0) var<uniform> lamps: Lamps;
@group(0) @binding(1) var scene_depth: texture_depth_2d;
@group(0) @binding(2) var surface: texture_2d<u32>;
// The picture's lights, a row each, its four parts (light.wgsl's Light)
// across.
@group(0) @binding(3) var lights: texture_2d<f32>;
// WALLS {
@group(0) @binding(4) var volume: texture_3d<u32>;
@group(0) @binding(5) var<storage, read_write> seen: array<u32>;
@group(0) @binding(6) var movers: texture_3d<u32>;
// The boxes round the moving things, low corner and high, from the
// volume's low corner (render::movers::Movers::boxes).
@group(0) @binding(7) var<storage, read> mover_boxes: array<vec4<i32>>;
// Each lamp's boxes, the ones it can light, one lamp's after another
// (render::movers::Movers::lists; its light's `edge.y` says where).
@group(0) @binding(8) var<storage, read> mover_lists: array<u32>;
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

@fragment
fn fs_lamp(in: TileOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let s = textureLoad(surface, px, 0);
    if ((s.x & (1u << 24u)) == 0u) {
        return vec4<f32>(0.0);
    }
    let depth = textureLoad(scene_depth, px, 0);
    let uv = in.clip.xy / vec2<f32>(textureDimensions(scene_depth));
    let at = lamps.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world = at.xyz / at.w;
    let n = lit_way(s.w);
    let off = (s.x >> 25u) & 1u;
    // WALLS {
    // The cell in front of the face, and its part of each lamp's key in
    // the memory.
    let cell = vec3<i32>(bitcast<i32>(s.y), bitcast<i32>(s.z), (bitcast<i32>(s.w) << 16u) >> 16u);
    // A face of the level itself (its voxel in the volume) gets nothing of
    // a lamp behind it: a way to the lamp would only go through a gap.
    let way_index = (s.x >> 26u) & 7u;
    let of_level = way_index < 6u && solid(cell - vec3<i32>(FACE_DIRS[way_index]) - lamps.origin.xyz, 0);
    let ids = vec2<u32>(mix_hash(0x9e3779b9u + off, cell), mix_hash(0x85ebca6bu + off, cell));
    // } WALLS
    var sum = vec3<f32>(0.0);
    for (var k = 0u; k < in.count.x; k += 1u) {
        let row = light_row(in, k);
        let l = Light(textureLoad(lights, vec2<i32>(0, row), 0), textureLoad(lights, vec2<i32>(1, row), 0), textureLoad(lights, vec2<i32>(2, row), 0), textureLoad(lights, vec2<i32>(3, row), 0));
        let lit = light_on(l, world, n);
        if (all(lit <= vec3<f32>(0.0))) {
            continue;
        }
        // WALLS {
        if (l.edge.y > 0.5 && ((of_level && dot(n, l.pos_radius.xyz - world) <= 0.0) || stopped(cell, off, l.pos_radius.xyz, ids, vec2<u32>(l.edge.zw)) || (l.edge.y > 1.5 && movers_stop(movers_from(cell, n, of_level), l.pos_radius.xyz, select(RIM, 0.0, of_level), u32(l.edge.y - 2.0), 0xffffffffu)))) {
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
    if ((s.x & (1u << 24u)) == 0u) {
        return vec4<f32>(0.0);
    }
    let depth = textureLoad(scene_depth, px, 0);
    let uv = (vec2<f32>(px) + vec2<f32>(0.5)) / vec2<f32>(size);
    let at = lamps.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let world = at.xyz / at.w;
    let n = lit_way(s.w);
    let off = (s.x >> 25u) & 1u;
    let cell = vec3<i32>(bitcast<i32>(s.y), bitcast<i32>(s.z), (bitcast<i32>(s.w) << 16u) >> 16u);
    let way_index = (s.x >> 26u) & 7u;
    let of_level = way_index < 6u && solid(cell - vec3<i32>(FACE_DIRS[way_index]) - lamps.origin.xyz, 0);
    let ids = vec2<u32>(mix_hash(0x9e3779b9u + off, cell), mix_hash(0x85ebca6bu + off, cell));
    for (var k = 0u; k < in.count.x; k += 1u) {
        let row = light_row(in, k);
        let l = Light(textureLoad(lights, vec2<i32>(0, row), 0), textureLoad(lights, vec2<i32>(1, row), 0), textureLoad(lights, vec2<i32>(2, row), 0), textureLoad(lights, vec2<i32>(3, row), 0));
        if (all(light_on(l, world, n) <= vec3<f32>(0.0)) || (of_level && dot(n, l.pos_radius.xyz - world) <= 0.0)) {
            continue;
        }
        _ = stopped(cell, off, l.pos_radius.xyz, ids, vec2<u32>(l.edge.zw));
    }
    return vec4<f32>(0.0);
}

// Whether the cell `c` (from the moving things' volume's low corner) of
// its copy `level` holds anything.
fn mover_solid(c: vec3<i32>, level: i32) -> bool {
    let t = c >> vec3<u32>(1u);
    let dims = lamps.movers_dims.xyz >> vec3<u32>(u32(level));
    if (any(t < vec3<i32>(0)) || any(t >= dims)) {
        return false;
    }
    let bits = textureLoad(movers, t, level).r;
    let b = u32((c.x & 1) | ((c.y & 1) << 1u) | ((c.z & 1) << 2u));
    return ((bits >> b) & 1u) == 1u;
}

// How far along the way `dir` from `origin` (the moving things' volume's)
// it first leaves what moves, looking at most `most` voxels on: 0 when it
// starts in the open, -1 when it is still inside there.
fn leave_mover(origin: vec3<f32>, dir: vec3<f32>, most: f32) -> f32 {
    let inv = 1.0 / dir;
    var t = 0.0;
    for (var i = 0; i < 64; i += 1) {
        let c = vec3<i32>(floor(origin + dir * t));
        if (!mover_solid(c, 0)) {
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

// Where a face's way to a lamp whose shadows are "all" begins: the middle
// of the cell in front of it (`cell`), as for the walls; a moving thing's
// face (not `of_level`) a voxel further out along its way `n`, so a way
// grazing a turned figure's own surface does not catch its cells, which
// change as a part moves a little (a face flicking dark for a picture).
fn movers_from(cell: vec3<i32>, n: vec3<f32>, of_level: bool) -> vec3<f32> {
    return vec3<f32>(cell) + vec3<f32>(0.5) + select(n, vec3<f32>(0.0), of_level);
}

// A lamp's word for its boxes (`list`, render::movers::Movers::lists)
// meaning all of the volume.
const EVERY_BOX: u32 = 255u;

// The span of the way from `lamp` along `dir` (in the moving things'
// volume, its inverse `inv`), up to `len`, within the boxes `list` names
// (where they begin in `mover_lists`, times 256, and how many; of the
// first 32 only those `mask` has a bit for): from where it enters the
// first it meets to where it leaves the last; none (x past y) when it
// meets none.
fn in_boxes(lamp: vec3<f32>, inv: vec3<f32>, len: f32, list: u32, mask: u32) -> vec2<f32> {
    var span = vec2<f32>(1e30, -1.0);
    let n = list & 255u;
    let start = list >> 8u;
    for (var i = 0u; i < select(n, 0u, n == EVERY_BOX); i += 1u) {
        if (i < 32u && ((mask >> i) & 1u) == 0u) {
            continue;
        }
        let k = i32(mover_lists[start + i]);
        let a = (vec3<f32>(mover_boxes[2 * k].xyz) - lamp) * inv;
        let b = (vec3<f32>(mover_boxes[2 * k + 1].xyz) - lamp) * inv;
        let t = max(max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z)), 0.0);
        let end = min(min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z)), len);
        if (t < end) {
            span = vec2<f32>(min(span.x, t), max(span.y, end));
        }
    }
    if (n == EVERY_BOX) {
        // Every box: the whole volume.
        let a = -lamp * inv;
        let b = (vec3<f32>(lamps.movers_dims.xyz * 2) - lamp) * inv;
        span = vec2<f32>(max(max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z)), 0.0), min(min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z)), len));
    }
    return span;
}

// Whether a moving thing (render::movers) stands between the lamp at
// `lamp_at` and the point `to_at` (a face's, `movers_from`; a point of a
// beam), for a lamp whose shadows are "all": the moving things' volume
// walked as `walk` walks the level's, only where the way meets the boxes
// the lamp can light (`list`, `mask`). The lamp in what moves (a car's
// headlight) leaves it first (HOUSING); the point, in what moves, looks
// past it for `rim` voxels (a moving thing's face where another part of it
// lies right in front).
fn movers_stop(to_at: vec3<f32>, lamp_at: vec3<f32>, rim: f32, list: u32, mask: u32) -> bool {
    if (lamps.movers_dims.w == 0) {
        return false;
    }
    let corner = vec3<f32>(lamps.movers_origin.xyz);
    let lamp = lamp_at - corner;
    let to = to_at - corner;
    let d = to - lamp;
    let len = length(d);
    if (len < 0.5) {
        return false;
    }
    let dir = select(d / len, vec3<f32>(1e-7), abs(d / len) < vec3<f32>(1e-7));
    let inv = 1.0 / dir;
    let span = in_boxes(lamp, inv, len, list, mask);
    var t = span.x;
    var end = span.y;
    if (end <= t) {
        return false;
    }
    if (t <= 0.0) {
        let begin = leave_mover(lamp, dir, min(len, HOUSING));
        if (begin < 0.0) {
            return true;
        }
        t = begin;
    }
    if (rim > 0.0 && end >= len) {
        end = len - max(leave_mover(to, -dir, rim), 0.0);
    }
    // The boxes are a thing each, a few voxels to a few dozen: the walk
    // begins at cells of 4 voxels and goes no coarser than 8.
    let top = min(lamps.movers_dims.w - 1, 3);
    var level = min(top, 2);
    for (var i = 0; i < MOST_STEPS; i += 1) {
        if (t >= end) {
            return false;
        }
        let p = lamp + dir * t;
        let size = f32(1 << u32(level));
        let c = vec3<i32>(floor(p / size));
        if (mover_solid(c, level)) {
            if (level == 0) {
                return true;
            }
            level -= 1;
        } else {
            let lo = vec3<f32>(c) * size;
            let ts = (select(lo, lo + vec3<f32>(size), dir > vec3<f32>(0.0)) - lamp) * inv;
            let out = min(ts.x, min(ts.y, ts.z));
            // Between two cells touching only at an edge: closed, as `walk`.
            let tied = (ts - vec3<f32>(out)) < vec3<f32>(1e-3 * size);
            if (u32(tied.x) + u32(tied.y) + u32(tied.z) > 1u) {
                let ahead = select(vec3<i32>(-1), vec3<i32>(1), dir > vec3<f32>(0.0));
                let beside = u32(tied.x && mover_solid(c + vec3<i32>(ahead.x, 0, 0), level)) + u32(tied.y && mover_solid(c + vec3<i32>(0, ahead.y, 0), level)) + u32(tied.z && mover_solid(c + vec3<i32>(0, 0, ahead.z), level));
                if (beside > 1u) {
                    if (level == 0) {
                        return true;
                    }
                    level -= 1;
                    continue;
                }
            }
            t = max(out, t) + 0.001;
            level = min(level + 1, top);
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
// it is still in solid voxels there.
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

// How many cells a way looks at most: one longer is taken as clear.
const MOST_STEPS: i32 = 256;
// The lamp sitting in its own housing (a lantern's frame, the bulb's
// socket): its way begins where it first leaves the solid voxels it sits
// in, at most this many voxels on (deeper in, it is buried and lights
// nothing).
const HOUSING: f32 = 8.0;
// A face off the grid looks past the solid its own voxels fill round it
// (render::volume grows a turned piece by half a voxel), this far.
const RIM: f32 = 2.0;

// Whether a solid voxel lies on the way from the lamp at `lamp` to the
// middle of the cell `cell` (in front of a face): walked through the cells,
// past empty space by the coarser copies (up a copy after each empty
// cell, down one into a cell holding anything).
fn walk(cell: vec3<i32>, off: u32, lamp_at: vec3<f32>) -> bool {
    // From the volume's low corner, as its cells are.
    let to = vec3<f32>(cell - lamps.origin.xyz) + vec3<f32>(0.5);
    let lamp = lamp_at - vec3<f32>(lamps.origin.xyz);
    let d = to - lamp;
    let len = length(d);
    if (len < 0.5) {
        return false;
    }
    // No way exactly along an axis: its zero a tiny step on, so the walk's
    // crossings all lie ahead.
    let dir = select(d / len, vec3<f32>(1e-7), abs(d / len) < vec3<f32>(1e-7));
    let begin = leave_solid(lamp, dir, min(len, HOUSING));
    if (begin < 0.0) {
        return true;
    }
    var end = len;
    if (off == 1u) {
        let rim = leave_solid(to, -dir, RIM);
        end = len - max(rim, 0.0);
    }
    let inv = 1.0 / dir;
    let top = lamps.dims.w - 1;
    var t = begin;
    var level = top;
    for (var i = 0; i < MOST_STEPS; i += 1) {
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
            level = min(level + 1, top);
        }
    }
    return false;
}

// The faces' memory of what stops lamps: entries of two words (as many as
// the buffer holds, render::lamps::faces_for the picture's size): a key of
// the cell in front of a face and the lamp's exact place; and whether
// stopped (bit 0), the volume's state (9 bits), the picture last asked (6
// bits) and a second key (16 bits, never 0); then the count of faces that
// found no room (render::lamps says it once). An entry not asked for STALE
// pictures makes room (a lamp moving asks anew each picture) and is walked
// again. Plain loads and stores, no atomics: pixels of a face read through
// the caches; an entry another pixel is writing reads as no answer by its
// second key (a word of another face's matches one time in 65535), so every
// answer is the walk's for its key and the picture does not depend on what
// the memory holds.
const PROBES: u32 = 16u;
const STALE: u32 = 8u;

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

// Whether walls stop the lamp at `lamp` lighting the face in front of
// `cell`: `ids` the cell's part of the key, `lamp_ids` the lamp's (a hash of
// its exact place, render::shader::LightData::of), so a key costs a few
// operations a lamp.
fn stopped(cell: vec3<i32>, off: u32, lamp: vec3<f32>, ids: vec2<u32>, lamp_ids: vec2<u32>) -> bool {
    let key = (ids.x ^ (lamp_ids.x * 0x9e3779b1u) ^ (lamp_ids.y << 8u)) | 1u;
    let tag = (((ids.y ^ (lamp_ids.y * 0x85ebca77u) ^ lamp_ids.x) % 65535u) + 1u) << 16u;
    let state = (lamps.info.x & 0x1ffu) << 1u;
    let now = lamps.info.y & 0x3fu;
    let faces = (arrayLength(&seen) - 1u) / 2u;
    var free = faces;
    var slot = key % faces;
    for (var i = 0u; i < PROBES; i += 1u) {
        let k = slot * 2u;
        let found = seen[k];
        let word = seen[k + 1u];
        if (found == key) {
            if ((word & 0xffff0000u) != tag) {
                // being written by another pixel of the face: walked here
                return walk(cell, off, lamp);
            }
            if ((word & 0x3feu) == state && fresh(word, now)) {
                if (((word >> 10u) & 0x3fu) != now) {
                    seen[k + 1u] = (word & 0xffff03ffu) | (now << 10u);
                }
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
            }
            if (found == 0u) {
                break;
            }
        }
        slot = (slot + 1u) % faces;
    }
    let b = walk(cell, off, lamp);
    if (free == faces) {
        seen[faces * 2u] += 1u;
        return b;
    }
    seen[free * 2u] = key;
    seen[free * 2u + 1u] = tag | (now << 10u) | state | select(0u, 1u, b);
    return b;
}
// } WALLS
