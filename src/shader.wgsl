// Voxcade shading: sun with soft shadow map, ambient, per-vertex ambient
// occlusion, light from flashes and explosions, haze. Colours stay in
// palette (gamma) space for the flat retro look. The scene goes to two
// targets: its colour, and what glows (blurred and added on top later).

const MAX_LIGHTS: u32 = 16u;

struct Light {
    // xyz: position, w: radius
    pos_radius: vec4<f32>,
    // rgb: colour times brightness
    color: vec4<f32>,
};

struct Globals {
    view_proj: mat4x4<f32>,
    light_vp: mat4x4<f32>,
    // xyz: direction the sunlight travels, w: shadow depth bias
    sun_dir: vec4<f32>,
    // x: ambient, y: shadow strength, z: shadow texel size, w: light count
    params: vec4<f32>,
    // xyz: eye position, w: haze strength
    eye: vec4<f32>,
    // rgb: haze colour, w: distance where the haze starts
    haze: vec4<f32>,
    // x: 1 / distance over which the haze builds up, y: shadow style
    // (0 smooth, 1 voxel, 2 voxel with half shadow), z: seconds, w: how
    // bright the streak sweeping over metal is
    misc: vec4<f32>,
    // rgb: the sky metal mirrors, overhead and at the horizon
    reflection: vec4<f32>,
    horizon: vec4<f32>,
    // rgb: the models' outline colour, w: how many voxels thick (0: none)
    outline: vec4<f32>,
    // rgb: the sunlight's colour and strength (white unless the game says)
    sun_color: vec4<f32>,
    // x: the picture's number, y: 1 when the faces remember their shadow
    // (a game moving its sun)
    memory: vec4<u32>,
    lights: array<Light, MAX_LIGHTS>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
// The faces' memory of their shadow: FACES entries of three words (a key,
// a check, the picture it was last seen in and whether it was lit).
@group(0) @binding(3) var<storage, read_write> faces: array<atomic<u32>>;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) ao: f32,
    // x: how much it glows, y: how opaque it is
    @location(4) fx: vec2<f32>,
    // x: how metal, y: how smooth
    @location(5) metal: vec2<f32>,
    // xyz: the direction this face is lit by when w is 1 (a model shaded
    // smooth, vox::VoxModel::shade); w 0: by its own.
    @location(6) shade: vec4<f32>,
    // xyz: the middle of the voxel when w is 1 (a cube turned freely, off
    // the grid); w 0: on the grid, found from the face.
    @location(7) centre: vec4<f32>,
};

// A vector turned by a rotation (a unit quaternion: xyz, w). All zeros or
// 0 0 0 1 leave it exactly as it is (nothing but zeros is added).
fn rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

// Whether a rotation turns at all.
fn turns(q: vec4<f32>) -> bool {
    return any(q.xyz != vec3<f32>(0.0));
}

// An instance's free turn (render::snorm_turn) made whole again.
fn instance_turn(turn: vec4<f32>) -> vec4<f32> {
    if (turns(turn)) {
        return normalize(turn);
    }
    return vec4<f32>(0.0);
}

@vertex
fn vs_chunk(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) color: vec4<f32>,
) -> VOut {
    var out: VOut;
    out.clip = g.view_proj * vec4<f32>(pos, 1.0);
    out.world = pos;
    out.normal = normal.xyz;
    out.color = color.rgb;
    out.ao = color.a;
    out.fx = vec2<f32>(0.0, 1.0);
    out.metal = vec2<f32>(0.0, 0.0);
    out.shade = vec4<f32>(0.0);
    out.centre = vec4<f32>(0.0);
    return out;
}

@vertex
fn vs_inst(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) pos_size: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) fx: vec4<f32>,
    @location(5) shade: vec4<f32>,
    @location(6) turn: vec4<f32>,
) -> VOut {
    let q = instance_turn(turn);
    let lit = vec4<f32>(rotate(q, shade.xyz), shade.w);
    return cube(pos, normal.xyz, normal.xyz, pos_size.xyz, pos_size.w, color, fx, lit, q);
}

// A model drawn from the bank (render::ModelDraw): where, which way, and
// whether it flashes.
struct Actor {
    // xyz: where the pivot's cell has its minimum corner, w: quarter turns
    origin: vec4<f32>,
    // rgb: the flash colour, w: 1 while it flashes
    tint: vec4<f32>,
    // x: how many steps nearer the eye its depth is taken (a posed
    // figure's part, render::ModelDraw::nearer)
    extra: vec4<f32>,
    // Turned freely after the quarter turns (0 0 0 1: not at all), then
    // shifted by pivot.xyz (render::turned_centre); pivot.w 1: off the grid.
    rot: vec4<f32>,
    pivot: vec4<f32>,
};

@group(1) @binding(0) var<uniform> actor: Actor;

// A position or direction turned by `k` quarter turns (render::quarter).
fn quarter(p: vec2<f32>, k: u32) -> vec2<f32> {
    switch (k % 4u) {
        case 1u: { return vec2<f32>(p.y, -p.x); }
        case 2u: { return -p; }
        case 3u: { return vec2<f32>(-p.y, p.x); }
        default: { return p; }
    }
}

// A voxel of a model in the bank: pos_size holds its place relative to the
// model's pivot, before the turn.
fn model_centre(local: vec3<f32>) -> vec3<f32> {
    let k = u32(actor.origin.w);
    let corner = vec3<f32>(quarter(local.xy, k), local.z);
    if (actor.pivot.w > 0.5) {
        // render::turned_centre
        return actor.origin.xyz + actor.pivot.xyz + rotate(actor.rot, corner + 0.5);
    }
    return actor.origin.xyz + corner + 0.5;
}

@vertex
fn vs_model(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) pos_size: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) fx: vec4<f32>,
    @location(5) shade: vec4<f32>,
) -> VOut {
    let k = u32(actor.origin.w);
    // Which faces the shade direction lights is counted in the model's own
    // directions: ask with the face turned back.
    let own = vec3<f32>(quarter(normal.xy, 4u - k), normal.z);
    var c = color;
    if (actor.tint.w > 0.5) {
        c = vec4<f32>(actor.tint.rgb, color.a);
    }
    let lit = vec4<f32>(rotate(actor.rot, vec3<f32>(quarter(shade.xy, k), shade.z)), shade.w);
    var out = cube(pos, normal.xyz, own, model_centre(pos_size.xyz), pos_size.w, c, fx, lit, actor.rot);
    // Off the grid (a part shifted, if not turned): its shadow asks at the
    // voxel's middle.
    out.centre.w = max(out.centre.w, actor.pivot.w);
    // A part of a posed figure: its depth taken a little nearer the eye,
    // the further out it hangs the more, its place in the picture kept.
    // Where a bent limb's faces lie in one plane with its parent's, one of
    // them is plainly in front.
    if (actor.extra.x > 0.0) {
        let toward = normalize(g.eye.xyz - out.world);
        let near = g.view_proj * vec4<f32>(out.world + toward * actor.extra.x * PART_NEARER, 1.0);
        out.clip.z = near.z / near.w * out.clip.w;
    }
    return out;
}

// How much nearer a figure's part is taken a step, in voxels: more than the
// depth buffer's grain at the game's distances, far less than can be seen.
const PART_NEARER: f32 = 0.03;

// How far behind a model its outline lies, in voxels: relief steps less
// deep than this hide it, a part standing further before another shows it.
const OUTLINE_BEHIND: f32 = 3.0;

// A model's outline (c.effects.outline): its voxels again, each grown by
// the outline's thickness all round, only their far faces drawn (the
// pipeline culls the near ones), in the outline's colour, and pushed
// OUTLINE_BEHIND voxels further along the rays from the eye (the same
// place on the screen, only deeper). What sticks out past the model shows,
// and past a part of it standing well before another; the rest lies
// behind the model's own faces. Grown in voxels, it is as thick wherever
// the model stands, as a sprite's line is a pixel.
@vertex
fn vs_model_outline(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) pos_size: vec4<f32>,
) -> VOut {
    let grown = pos_size.w + 2.0 * g.outline.w;
    let flat = vec4<f32>(g.outline.rgb, 1.0);
    var out = cube(pos, normal.xyz, normal.xyz, model_centre(pos_size.xyz), grown, flat, vec4<f32>(0.0, 1.0, 0.0, 0.0), vec4<f32>(0.0), actor.rot);
    out.world += normalize(out.world - g.eye.xyz) * OUTLINE_BEHIND;
    out.clip = g.view_proj * vec4<f32>(out.world, 1.0);
    return out;
}

// A cube: `normal` is the face's direction, `own` the same in the model's
// directions (which bit of shade.w lights it), both before the free turn
// `q` (zeros or 0 0 0 1: square to the grid).
fn cube(
    pos: vec3<f32>,
    normal: vec3<f32>,
    own: vec3<f32>,
    centre: vec3<f32>,
    size: f32,
    color: vec4<f32>,
    fx: vec4<f32>,
    shade: vec4<f32>,
    q: vec4<f32>,
) -> VOut {
    let world = centre + rotate(q, pos * size);
    var out: VOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    // fx.z: the faces a neighbouring cube covers (bits in mesh::DIRS order:
    // +x -x +y -y +z -z). They are not drawn: where they meet the
    // neighbour's surface at the same depth they would show as seams.
    let hidden = u32(round(fx.z * 255.0));
    let face = face_index(normal);
    if (((hidden >> face) & 1u) == 1u) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    out.world = world;
    out.normal = rotate(q, normal);
    out.centre = vec4<f32>(centre, select(0.0, 1.0, turns(q)));
    out.color = color.rgb;
    out.ao = 1.0;
    out.fx = fx.xy;
    // fx.w: metal, packed as vox::metal::pack.
    let m = u32(round(fx.w * 255.0));
    out.metal = vec2<f32>(f32(m >> 4u) / 15.0, f32(m & 15u) / 15.0);
    // shade.w: the faces lit by shade.xyz instead of their own (bits as
    // above); with none, xyz is the way smooth glass's facet faces (or
    // nothing).
    let lit = u32(round(shade.w * 127.0));
    out.shade = vec4<f32>(shade.xyz, f32((lit >> face_index(own)) & 1u));
    return out;
}

// A face's index in mesh::DIRS order (+x -x +y -y +z -z).
fn face_index(n: vec3<f32>) -> u32 {
    if (n.x > 0.5) {
        return 0u;
    } else if (n.x < -0.5) {
        return 1u;
    } else if (n.y > 0.5) {
        return 2u;
    } else if (n.y < -0.5) {
        return 3u;
    } else if (n.z > 0.5) {
        return 4u;
    }
    return 5u;
}

@vertex
fn vs_chunk_shadow(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return g.light_vp * vec4<f32>(pos, 1.0);
}

@vertex
fn vs_model_shadow(
    @location(0) pos: vec3<f32>,
    @location(2) pos_size: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    return g.light_vp * vec4<f32>(model_centre(pos_size.xyz) + rotate(actor.rot, pos * pos_size.w), 1.0);
}

@vertex
fn vs_inst_shadow(
    @location(0) pos: vec3<f32>,
    @location(2) pos_size: vec4<f32>,
    @location(6) turn: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    return g.light_vp * vec4<f32>(pos_size.xyz + rotate(instance_turn(turn), pos * pos_size.w), 1.0);
}

// 1 = fully sunlit, 0 = fully in shadow. 3x3 PCF for soft edges; in the
// voxel styles the whole face asks once, at its middle, and the answer is
// rounded to lit or shadowed (or half). `face` is the face's own direction,
// `n` the one it is lit by: a voxel lit as a whole (smooth) asks once for
// all its faces, where its surface faces. `centre` is the voxel's middle
// when it is turned off the grid (w 1).
fn sun_visibility(world: vec3<f32>, face: vec3<f32>, n: vec3<f32>, centre: vec4<f32>, sunlit: f32) -> f32 {
    let style = g.misc.y;
    var at = world;
    if (style > 0.5) {
        if (centre.w > 0.5) {
            at = centre.xyz + n * 0.5;
        } else {
            let cell = floor(world - face * 0.5);
            at = cell + vec3<f32>(0.5) + n * 0.5;
        }
    }
    let raw = shadow_lookup(at, n);
    if (style > 1.5) {
        return round(raw * 2.0) * 0.5;
    }
    if (style > 0.5) {
        // only one the sun lights enough for its shadow to show (`sunlit`,
        // how squarely) needs a memory
        if (g.memory.y == 1u && sunlit > 0.05) {
            return remembered(at, n, raw);
        }
        return step(0.5, raw);
    }
    return raw;
}

// A face lit or shaded as it was until its shadow is clearly the other
// way. A sun the game moves turns the shadow map every picture, and where
// its texels fall shifts by up to half a texel: at a shadow's edge, and
// where a shadow's corner falls close by, a face decided at its middle
// alone went from lit to shaded and back from picture to picture (the
// user saw it in BunnyFarm). Each face remembers how much in the sun it
// has been of late (KEEP of each picture's look goes in) and whether it is
// lit, and changes only when that is MARGIN past the middle: the
// rounding's jitter stays inside, a passing edge goes through, so a face
// changes once, a few pictures after the edge passes, and the shadows
// move as smoothly as the sun. The memory is the face's (by where it is
// and the way it looks), not the pixel's, so every pixel of a face
// agrees, and it changes once a picture whichever pixel comes first. Only
// a face at an edge (some taps in the shadow, some not) makes one: a face
// wholly lit or wholly shaded only looks whether it has one and keeps
// using it, and once it has agreed with it for STALE pictures it is
// forgotten (a memory of every face cost the small GPU 7 ms; a face
// leaving it at once flickered as rounding took it back to the edge).
// Each entry is three words: a key, a check, and lit (bit 0), how much in
// the sun (10 bits) and the picture it was last seen in (21 bits).
const FACES: u32 = 65536u;
const PROBES: u32 = 8u;
const STALE: u32 = 30u;
const MARGIN: f32 = 0.25;
const KEEP: f32 = 0.25;
const PICTURES: u32 = 0x1fffffu;

fn mix_hash(seed: u32, c: vec3<i32>) -> u32 {
    var h = seed ^ (bitcast<u32>(c.x) * 0x27d4eb2du);
    h = ((h ^ (h >> 15u)) * 0x2c1b3c6du) ^ bitcast<u32>(c.y);
    h = ((h ^ (h >> 12u)) * 0x297a2d39u) ^ bitcast<u32>(c.z);
    h = (h ^ (h >> 15u)) * 0x27d4eb2du;
    return h ^ (h >> 16u);
}

fn face_word(lit: bool, sun: f32, now: u32) -> u32 {
    return select(0u, 1u, lit) | (u32(round(clamp(sun, 0.0, 1.0) * 1023.0)) << 1u) | ((now & PICTURES) << 11u);
}

fn face_lit(word: u32) -> f32 {
    return select(0.0, 1.0, (word & 1u) == 1u);
}

// The face's memory after this picture's look.
fn face_next(word: u32, raw: f32, now: u32) -> u32 {
    let sun = mix(f32((word >> 1u) & 0x3ffu) / 1023.0, raw, KEEP);
    var lit = (word & 1u) == 1u;
    if (lit && sun < 0.5 - MARGIN) {
        lit = false;
    } else if (!lit && sun > 0.5 + MARGIN) {
        lit = true;
    }
    return face_word(lit, sun, now);
}

fn remembered(at: vec3<f32>, n: vec3<f32>, raw: f32) -> f32 {
    // `at` lies on the half-voxel grid; the way the face looks tells two
    // faces at one point apart.
    let c = vec3<i32>(round((at + n * 0.25) * 4.0));
    let key = mix_hash(0x9e3779b9u, c) | 1u;
    let check = mix_hash(0x85ebca6bu, c);
    let now = g.memory.x & PICTURES;
    let clear = raw <= 0.0 || raw >= 1.0;
    var free = FACES;
    var slot = key % FACES;
    for (var i = 0u; i < PROBES; i += 1u) {
        let k = slot * 3u;
        let found = atomicLoad(&faces[k]);
        var word = atomicLoad(&faces[k + 2u]);
        if (found == key && atomicLoad(&faces[k + 1u]) == check) {
            // once a picture: the first pixel of the face to come
            for (var tries = 0u; tries < 4u; tries += 1u) {
                if ((word >> 11u) == now) {
                    return face_lit(word);
                }
                var next = face_next(word, raw, now);
                if (clear && face_lit(next) == raw) {
                    // agreeing with a clear look: not kept fresh, so it fades
                    next = (next & 0x7ffu) | (word & ~0x7ffu);
                }
                let swap = atomicCompareExchangeWeak(&faces[k + 2u], word, next);
                if (swap.exchanged) {
                    return face_lit(next);
                }
                word = swap.old_value;
            }
            return face_lit(word);
        }
        if (found == 0u || ((now - (word >> 11u)) & PICTURES) > STALE) {
            if (free == FACES) {
                free = slot;
            }
            if (found == 0u) {
                break;                  // never used: the face has none further on
            }
        }
        slot = (slot + 1u) % FACES;
    }
    if (clear || free == FACES) {
        return step(0.5, raw);
    }
    let k = free * 3u;
    let found = atomicLoad(&faces[k]);
    if (atomicCompareExchangeWeak(&faces[k], found, key).exchanged) {
        atomicStore(&faces[k + 1u], check);
        let next = face_word(raw >= 0.5, raw, now);
        atomicStore(&faces[k + 2u], next);
        return face_lit(next);
    }
    return step(0.5, raw);
}

fn shadow_lookup(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let p = g.light_vp * vec4<f32>(world + n * 0.4, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z > 1.0) {
        return 1.0;
    }
    let texel = g.params.z;
    let depth = p.z - g.sun_dir.w;
    var sum = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, depth);
        }
    }
    return sum / 9.0;
}

// Light from flashes and explosions near this point.
fn flare_light(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = u32(g.params.w);
    for (var i = 0u; i < count; i += 1u) {
        let l = g.lights[i];
        let to = l.pos_radius.xyz - world;
        let d = length(to);
        let r = l.pos_radius.w;
        if (d < r) {
            let fall = 1.0 - d / r;
            // Faces turned away still catch a little: it is a burst, not a lamp.
            let facing = max(dot(n, to / max(d, 0.001)), 0.0) * 0.75 + 0.25;
            sum += l.color.rgb * fall * fall * facing;
        }
    }
    return sum;
}

// What a mirror shows the way `r` points: the sky - a bright band low
// over the horizon, deeper overhead - and the ground below the horizon.
fn mirrored(r: vec3<f32>) -> vec3<f32> {
    let up = r.z;
    var sky = mix(g.horizon.rgb, g.reflection.rgb, smoothstep(0.05, 0.95, up));
    // A bright band where the sky meets the haze, about 40 degrees up:
    // flat plates seen from the game's camera mirror right through it.
    sky += vec3<f32>(0.36) * exp(-((up - 0.64) * (up - 0.64)) / 0.0035);
    return mix(g.haze.rgb * 0.45, sky, smoothstep(-0.04, 0.04, up));
}

// The lights about (a torch, a burst) glinting off a smooth face seen
// from `v`, as sharp as `power`.
fn flare_glint(world: vec3<f32>, n: vec3<f32>, v: vec3<f32>, power: f32) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = u32(g.params.w);
    for (var i = 0u; i < count; i += 1u) {
        let l = g.lights[i];
        let to = l.pos_radius.xyz - world;
        let d = length(to);
        let r = l.pos_radius.w;
        if (d < r) {
            let h = normalize(to / max(d, 0.001) + v);
            sum += l.color.rgb * (1.0 - d / r) * pow(max(dot(n, h), 0.0), power) * (power + 8.0) / 24.0;
        }
    }
    return sum;
}

struct FOut {
    @location(0) color: vec4<f32>,
    @location(1) glow: vec4<f32>,
};

// The outline's flat colour, unlit, glowing not at all.
@fragment
fn fs_outline(in: VOut) -> FOut {
    var out: FOut;
    out.color = vec4<f32>(in.color, 1.0);
    out.glow = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VOut) -> FOut {
    let face = normalize(in.normal);
    // A voxel shaded smooth is lit as a whole, by its surface's direction.
    var n = face;
    if (in.shade.w > 0.5) {
        n = normalize(in.shade.xyz);
    }
    let to_sun = -normalize(g.sun_dir.xyz);
    let diffuse = max(dot(n, to_sun), 0.0);
    let vis = sun_visibility(in.world, face, n, in.centre, diffuse);
    let ambient = g.params.x;
    let shadow = g.params.y;
    // The sun's own light in its colour (white unless the game says), over
    // the ambient light every face gets.
    let sun = vec3<f32>(ambient) + (1.0 - ambient) * diffuse * mix(1.0 - shadow, 1.0, vis) * g.sun_color.rgb;
    let ao = mix(0.5, 1.0, in.ao);
    let lit = in.color * (sun * ao + flare_light(in.world, n));
    // Glowing things shine in their own colour, whatever the light.
    let glow = in.fx.x;
    var color = mix(lit, in.color * 1.15, glow);

    // Metal is a mirror more than a colour: its colour is how much it
    // mirrors (more at a glancing look), and what it mirrors is the sky -
    // a bright band low over the horizon, deeper overhead - and the ground
    // below the horizon. A soft sheen comes from the bright sky opposite
    // the sun, a hard highlight from the sun itself (gone in shadow). It all
    // shifts over a surface as the view does, which is what makes a flat
    // plate read as metal.
    var shine = vec3<f32>(0.0);
    var opacity = in.fx.y;
    if (in.metal.x > 0.0) {
        let v = normalize(g.eye.xyz - in.world);
        let r = reflect(-v, n);
        let seen = mirrored(r);
        let cosv = max(dot(n, v), 0.0);
        let reflectance = in.color + (vec3<f32>(1.0) - in.color) * pow(1.0 - cosv, 5.0);
        let gloss = in.metal.y;
        let away = normalize(vec3<f32>(-to_sun.x, -to_sun.y, 0.0) * 0.77 + vec3<f32>(0.0, 0.0, 0.64));
        let sheen = pow(max(dot(r, away), 0.0), mix(3.0, 18.0, gloss)) * mix(0.25, 0.6, gloss);
        let power = mix(6.0, 160.0, gloss * gloss);
        let h = normalize(to_sun + v);
        let highlight = pow(max(dot(n, h), 0.0), power) * (power + 8.0) / 24.0 * mix(0.15, 1.0, vis);
        let m = in.metal.x;
        // Shadows still darken it a little, so they read on metal too.
        let metal = (seen * 1.08 + vec3<f32>(sheen)) * reflectance * mix(0.72, 1.0, vis);
        color = mix(color, metal, m) + highlight * m * g.sun_color.rgb;
        // The sweep: a narrow streak running diagonally over the ground
        // plane every few seconds, brighter on smoother metal.
        let along = dot(in.world.xy, vec2<f32>(0.8, -0.6)) + in.world.z * 0.5 - g.misc.z * 110.0;
        let period = 480.0;
        let p = along - period * floor(along / period) - period * 0.5;
        let streak = exp(-(p * p) / 40.0) + 0.35 * exp(-((p + 9.0) * (p + 9.0)) / 10.0);
        shine = vec3<f32>(streak * g.misc.w * m * (0.35 + 0.65 * gloss));
        color += shine;
    } else if (in.metal.y > 0.0) {
        // Smooth glass mirrors uncoloured, little head on and much at a
        // glancing look (Fresnel's 4 % for glass), and the sun and the
        // lights about glint off its faces, more sharply the smoother it is.
        // Where it mirrors it lets less through.
        // A cut stone mirrors by its facets' slant, not its voxels' faces;
        // a facet turned away from the eye shows the face it is seen by.
        let v = normalize(g.eye.xyz - in.world);
        var facet = n;
        if (in.shade.w < 0.5 && dot(in.shade.xyz, in.shade.xyz) > 0.25 && dot(in.shade.xyz, v) > 0.0) {
            facet = normalize(in.shade.xyz);
        }
        let gloss = in.metal.y;
        let fresnel = 0.04 + 0.96 * pow(1.0 - max(dot(facet, v), 0.0), 5.0);
        let power = mix(6.0, 160.0, gloss * gloss);
        let h = normalize(to_sun + v);
        let sun_glint = pow(max(dot(facet, h), 0.0), power) * (power + 8.0) / 24.0 * vis;
        let glint = sun_glint * g.sun_color.rgb + flare_glint(in.world, facet, v, power);
        color = mix(color, mirrored(reflect(-v, facet)), fresnel) + glint;
        opacity = mix(opacity, 1.0, clamp(fresnel + max(glint.r, max(glint.g, glint.b)), 0.0, 1.0));
        // The glints glow a little too, so they sparkle.
        shine = glint;
    }

    let haze = clamp((distance(in.world, g.eye.xyz) - g.haze.w) * g.misc.x, 0.0, 1.0) * g.eye.w;
    color = mix(color, g.haze.rgb, haze * (1.0 - glow));

    var out: FOut;
    out.color = vec4<f32>(color, opacity);
    // The sweep glows a little too, so it sparkles.
    out.glow = vec4<f32>(in.color * glow + shine * 0.3, opacity);
    return out;
}
