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
        if (l.edge.y > 0.5 && ((of_level && dot(n, l.pos_radius.xyz - world) <= 0.0) || stopped(cell, off, l.pos_radius.xyz, ids, vec2<u32>(l.edge.zw)))) {
            continue;
        }
        // } WALLS
        sum += lit;
    }
    return vec4<f32>(unpack4x8unorm(s.x).rgb * sum, 0.0);
}

// WALLS {
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
            // Asked carefully (`off` 2: the fog's beams), leaving through an
            // edge or a corner past the lamp's housing, the cells across
            // each face it meets there count too, so a wall of voxels that
            // touch only at their edges lets no light through. (The faces
            // walk as they always did: a face beside a lamp in its wall
            // would go dark.)
            let tied = (ts - vec3<f32>(out)) < vec3<f32>(1e-3 * size);
            if (off == 2u && t > begin + 1.0 && u32(tied.x) + u32(tied.y) + u32(tied.z) > 1u) {
                let ahead = select(vec3<i32>(-1), vec3<i32>(1), dir > vec3<f32>(0.0));
                let beside = (tied.x && solid(c + vec3<i32>(ahead.x, 0, 0), level)) || (tied.y && solid(c + vec3<i32>(0, ahead.y, 0), level)) || (tied.z && solid(c + vec3<i32>(0, 0, ahead.z), level));
                if (beside) {
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
