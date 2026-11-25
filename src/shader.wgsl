// Voxcade shading: sun with soft shadow map, ambient, per-vertex ambient
// occlusion, light from flashes and explosions, haze. Colours stay in
// palette (gamma) space for the flat retro look. The scene goes to two
// targets: its colour, and what glows (blurred and added on top later).
//
// The numbers the Rust side shares with it (MAX_LIGHTS, FACES, the lanes
// of an instance's fx, the faces' order, metal's packing) are not written
// here: render::shader::prelude writes them ahead of this text, from the
// one table both sides read; and `light.wgsl` (a light, what it gives a
// point), which the lamps' pass shares.

struct Globals {
    view_proj: mat4x4<f32>,
    light_vp: mat4x4<f32>,
    // xyz: direction the sunlight travels, w: shadow depth bias
    sun_dir: vec4<f32>,
    // x: ambient, y: shadow strength, z: shadow texel size, w: how many of
    // `lights` this pass lights by (the nearest the middle of the view)
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
// a check, the picture it was last seen in and whether it was lit). What
// lies between the MEMORY marks is only in the pipelines of a game that
// moves its sun (render::plain_shader takes it out of the others: a
// fragment shader that may write storage loses early depth testing on
// many GPUs, and some have no such writes).
// MEMORY {
@group(0) @binding(3) var<storage, read_write> faces: array<atomic<u32>>;
// } MEMORY

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
    // figure's part, render::ModelDraw::nearer); y: how much every voxel
    // glows in place of its own (a part posed with a glow,
    // render::ModelDraw::glow; below 0: its own)
    extra: vec4<f32>,
    // Turned freely after the quarter turns (0 0 0 1: not at all), then
    // shifted by pivot.xyz (render::turned_centre); pivot.w 1: off the grid.
    rot: vec4<f32>,
    pivot: vec4<f32>,
    // A line round this draw alone (render::ModelDraw::outline): rgb, w how
    // thick; w 0: the picture's (g.outline).
    outline: vec4<f32>,
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
    let lit = vec4<f32>(model_shade(shade.xyz), shade.w);
    var drawn = fx;
    drawn[FX_GLOW] = model_glow(fx[FX_GLOW]);
    return model_out(cube_at(model_corner(pos_size.xyz, pos * pos_size.w), normal.xyz, own, model_centre(pos_size.xyz), model_color(color), drawn, lit, actor.rot));
}

// A model's colour as it is drawn: its own, or the flash's while it flashes.
fn model_color(color: vec4<f32>) -> vec4<f32> {
    if (actor.tint.w > 0.5) {
        return vec4<f32>(actor.tint.rgb, color.a);
    }
    return color;
}

// How much a model's voxel glows as it is drawn: its own, or its part's
// pose's in place of it.
fn model_glow(glow: f32) -> f32 {
    if (actor.extra.y >= 0.0) {
        return actor.extra.y;
    }
    return glow;
}

// The way a model's voxel is lit by (vox::VoxModel::shade, its own
// directions), turned as the model is.
fn model_shade(way: vec3<f32>) -> vec3<f32> {
    return rotate(actor.rot, vec3<f32>(quarter(way.xy, u32(actor.origin.w)), way.z));
}

// A corner of a model's face as the draw places it.
fn model_out(face: VOut) -> VOut {
    var out = face;
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

// A model's face baked (render::baked::Face): the same face of its voxel's
// cube as vs_model draws, its four corners the unit cube's (`corner`, 0 to
// 3, by the face's two triangles as the cube's are split): `cell` its
// voxel's cell from the pivot, its own direction, lit, first; `look` its
// colour and glow; `shade` the way it is lit by; `bytes.w` its metal.
@vertex
fn vs_face(
    @builtin(vertex_index) corner: u32,
    @location(0) cell: u32,
    @location(1) look: vec4<f32>,
    @location(2) shade: vec4<f32>,
    @location(3) bytes: vec4<u32>,
) -> VOut {
    let own = FACE_DIRS[extractBits(cell, FACE_DIR_AT, 3u)];
    let normal = vec3<f32>(quarter(own.xy, u32(actor.origin.w)), own.z);
    let local = face_cell(cell);
    let pos = CUBE_CORNERS[face_index(normal) * 4u + corner];
    let lit = f32(extractBits(cell, FACE_LIT_AT, 1u));
    return model_out(voxel_face(model_corner(local, pos), normal, model_centre(local), model_color(look), model_glow(look.a), 1.0, bytes.w, model_shade(shade.xyz), lit, actor.rot));
}

// A baked face's voxel: its cell from the model's pivot cell
// (render::baked::Face, x and y signed).
fn face_cell(cell: u32) -> vec3<f32> {
    let signed = bitcast<i32>(cell);
    let x = extractBits(signed, 0u, FACE_CELL_BITS);
    let y = extractBits(signed, FACE_CELL_BITS, FACE_CELL_BITS);
    return vec3<f32>(f32(x), f32(y), f32(extractBits(cell, 2u * FACE_CELL_BITS, FACE_CELL_BITS)));
}

// How much nearer a figure's part is taken a step, in voxels: more than the
// depth buffer's grain at the game's distances, far less than can be seen.
const PART_NEARER: f32 = 0.03;

// How far behind a model its outline lies, in voxels: relief steps less
// deep than this hide it, a part standing further before another shows it.
const OUTLINE_BEHIND: f32 = 3.0;

// Where a draw's own outline lies in the depth (0 near, 1 far): behind
// everything drawn, before the depth's clear value.
const OWN_OUTLINE_DEPTH: f32 = 0.99999;

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
    return outline_corner(pos_size.xyz, pos, normal.xyz, pos_size.w);
}

// The outline from a model's baked faces: each voxel's first face draws
// the voxel's grown cube whole (`corner` the unit cube's, drawn by its
// triangles), the others nothing.
@vertex
fn vs_face_outline(
    @builtin(vertex_index) corner: u32,
    @location(0) cell: u32,
) -> VOut {
    var out = outline_corner(face_cell(cell), CUBE_CORNERS[corner], CUBE_NORMALS[corner], 1.0);
    if (extractBits(cell, FACE_FIRST_AT, 1u) == 0u) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

// A corner of the outline's grown cube about a model's voxel at `local`
// (`size` voxels big): `pos` the unit cube's corner, `normal` its face's.
fn outline_corner(local: vec3<f32>, pos: vec3<f32>, normal: vec3<f32>, size: f32) -> VOut {
    var fx = vec4<f32>(0.0);
    fx[FX_OPACITY] = 1.0;
    if (actor.outline.w > 0.0) {
        return own_outline_corner(local, pos * size, normal, fx);
    }
    let grown = size + 2.0 * g.outline.w;
    let flat = vec4<f32>(g.outline.rgb, 1.0);
    var out = cube_at(model_corner(local, pos * grown), normal, normal, model_centre(local), flat, fx, vec4<f32>(0.0), actor.rot);
    out.world += normalize(out.world - g.eye.xyz) * OUTLINE_BEHIND;
    out.clip = g.view_proj * vec4<f32>(out.world, 1.0);
    return out;
}

// A line round this draw alone (a model drawn whole on the HUD,
// render::ModelDraw::outline): the voxel's cube, `off` its corner from its
// middle, grown by the line's width across the HUD only (its x and y, the
// way the corner lies from the middle), not towards the eye: grown in
// depth too, its part nearer the eye was magnified and slid away from the
// vanishing point, and away from the screen's middle the ring was nearly
// twice as wide as said, wider still on the side away from the middle.
// At the far end of the depth, so it shows only where nothing else is, the
// silhouette's ring: pushed back only so far, a part standing behind
// another (a chest's lid opened) lay behind the other's line and was
// covered by it whole.
fn own_outline_corner(local: vec3<f32>, off: vec3<f32>, normal: vec3<f32>, fx: vec4<f32>) -> VOut {
    let line = actor.outline;
    let away = rotate(actor.rot, off);
    let across = vec3<f32>(select(-line.w, line.w, away.x >= 0.0), select(-line.w, line.w, away.y >= 0.0), 0.0);
    var out = cube_at(model_corner(local, off) + across, normal, normal, model_centre(local), vec4<f32>(line.rgb, 1.0), fx, vec4<f32>(0.0), actor.rot);
    out.clip.z = out.clip.w * OWN_OUTLINE_DEPTH;
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
    return cube_at(centre + rotate(q, pos * size), normal, own, centre, color, fx, shade, q);
}

// A corner of a model's voxel in the world: `off` is the corner from the
// voxel's middle. A turned model's corner is summed in the model's own
// grid first, exactly, and then turned: two voxels side by side get the
// very same number for the corner they share. Turned each from its own
// middle, they differed in the last bit, and single pixels of what lies
// behind showed between them.
fn model_corner(local: vec3<f32>, off: vec3<f32>) -> vec3<f32> {
    if (actor.pivot.w > 0.5) {
        let k = u32(actor.origin.w);
        let corner = vec3<f32>(quarter(local.xy, k), local.z);
        return actor.origin.xyz + actor.pivot.xyz + rotate(actor.rot, corner + 0.5 + off);
    }
    return model_centre(local) + rotate(actor.rot, off);
}

// A cube's corner at `world`, worked out by the caller.
fn cube_at(
    world: vec3<f32>,
    normal: vec3<f32>,
    own: vec3<f32>,
    centre: vec3<f32>,
    color: vec4<f32>,
    fx: vec4<f32>,
    shade: vec4<f32>,
    q: vec4<f32>,
) -> VOut {
    // counted in the model's own directions, as the bits are
    let face = face_index(own);
    // shade.w: the faces lit by shade.xyz instead of their own (bits as
    // below); with none, xyz is the way smooth glass's facet faces (or
    // nothing). fx[FX_METAL]: metal, packed as vox::metal::pack.
    let lit = u32(round(shade.w * SNORM8));
    var out = voxel_face(world, normal, centre, color, fx[FX_GLOW], fx[FX_OPACITY], u32(round(fx[FX_METAL] * 255.0)), shade.xyz, f32((lit >> face) & 1u), q);
    // fx[FX_COVERED]: the faces a neighbouring cube covers (a bit each, in
    // mesh::DIRS order, face_index). They are not drawn: where they meet
    // the neighbour's surface at the same depth they would show as seams.
    let hidden = u32(round(fx[FX_COVERED] * 255.0));
    if (((hidden >> face) & 1u) == 1u) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

// A corner of a voxel's face at `world`: its colour, how much it glows,
// how opaque, its metal (vox::metal::pack), the way it is lit by and
// whether by that way (`lit`, 1) or its own, turned by `q` (zeros or 0 0 0
// 1: square to the grid).
fn voxel_face(
    world: vec3<f32>,
    normal: vec3<f32>,
    centre: vec3<f32>,
    color: vec4<f32>,
    glow: f32,
    opacity: f32,
    metal: u32,
    shade: vec3<f32>,
    lit: f32,
    q: vec4<f32>,
) -> VOut {
    var out: VOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = rotate(q, normal);
    out.centre = vec4<f32>(centre, select(0.0, 1.0, turns(q)));
    out.color = color.rgb;
    out.ao = 1.0;
    out.fx = vec2<f32>(glow, opacity);
    out.metal = vec2<f32>(f32(metal >> METAL_SHIFT) / f32(METAL_STEPS), f32(metal & METAL_STEPS) / f32(METAL_STEPS));
    out.shade = vec4<f32>(shade, lit);
    return out;
}

// A face's index in mesh::DIRS order (the prelude's FACE_ numbers).
fn face_index(n: vec3<f32>) -> u32 {
    if (n.x > 0.5) {
        return FACE_POS_X;
    } else if (n.x < -0.5) {
        return FACE_NEG_X;
    } else if (n.y > 0.5) {
        return FACE_POS_Y;
    } else if (n.y < -0.5) {
        return FACE_NEG_Y;
    } else if (n.z > 0.5) {
        return FACE_POS_Z;
    }
    return FACE_NEG_Z;
}

@vertex
fn vs_chunk_shadow(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return g.light_vp * vec4<f32>(pos, 1.0);
}

// A model's voxel into the shadow map: the faces another voxel covers are
// left out as in vs_model (inside the model, behind its own faces from
// the sun as from anywhere).
@vertex
fn vs_model_shadow(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) pos_size: vec4<f32>,
    @location(4) fx: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    let own = vec3<f32>(quarter(normal.xy, 4u - u32(actor.origin.w)), normal.z);
    if (((u32(round(fx[FX_COVERED] * 255.0)) >> face_index(own)) & 1u) == 1u) {
        return vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return g.light_vp * vec4<f32>(model_corner(pos_size.xyz, pos * pos_size.w), 1.0);
}

// A model's baked face into the shadow map (vs_face's corners).
@vertex
fn vs_face_shadow(
    @builtin(vertex_index) corner: u32,
    @location(0) cell: u32,
) -> @builtin(position) vec4<f32> {
    let own = FACE_DIRS[extractBits(cell, FACE_DIR_AT, 3u)];
    let normal = vec3<f32>(quarter(own.xy, u32(actor.origin.w)), own.z);
    let pos = CUBE_CORNERS[face_index(normal) * 4u + corner];
    return g.light_vp * vec4<f32>(model_corner(face_cell(cell), pos), 1.0);
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
        // MEMORY {
        if (g.memory.y == 1u && sunlit > 0.05) {
            return remembered(at, n, raw);
        }
        // } MEMORY
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
// the sun (10 bits) and the picture it was last seen in (21 bits). FACES
// (how many) is the prelude's, render::shader::SHADOW_FACES.
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

// MEMORY {
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
        if (!clear) {
            // no room: counted in the word after the entries (render::limits)
            atomicAdd(&faces[FACES * 3u], 1u);
        }
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
// } MEMORY

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

// Light from flashes, explosions and lamps near this point (walls aside:
// the lamps walls stop are the lamps' pass's, render::lamps).
fn flare_light(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = u32(g.params.w);
    for (var i = 0u; i < count; i += 1u) {
        sum += light_on(g.lights[i], world, n);
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
            var glint = l.color.rgb * (1.0 - d / r) * pow(max(dot(n, h), 0.0), power) * (power + 8.0) / 24.0;
            if (l.spot.w > -1.5) {
                glint *= in_cone(l, -to / max(d, 0.001));
            }
            sum += glint;
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

// The outline in the lamps' scene (render::lamps): its pixels no surface
// for the lamps to light.
@fragment
fn fs_outline_lamps(in: VOut) -> LampsOut {
    var out: LampsOut;
    out.color = vec4<f32>(in.color, 1.0);
    out.glow = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out.surface = vec4<u32>(0u);
    return out;
}

@fragment
fn fs_main(in: VOut) -> FOut {
    return shade(in, true).out;
}

// The scene of a picture whose lights the lamps' pass draws
// (render::lamps): each pixel as fs_main without the lights, and what the
// pass needs of its surface (`surface`): x the colour the light falls on (8
// bits each), bit 24 a surface there, bit 25 off the grid, bits 26 to 28
// the face's way (`face_index`); y, z and w's low 16 bits the cell in front
// of its face (lamps.wgsl's ways to a lamp start in its middle: the
// voxel's neighbour across the face; off the grid the cell a voxel out
// from its middle, so a face is lit or not as a whole); w's high bits the
// way it is lit by (`lit_way`). Off the grid the cell's x and y take 20
// bits, and the rest of y and z and x's bits from 26 on where its way to
// the moving things begins, exactly (lamps.wgsl's `surface_cell`,
// `movers_from`).
struct LampsOut {
    @location(0) color: vec4<f32>,
    @location(1) glow: vec4<f32>,
    @location(2) surface: vec4<u32>,
};

@fragment
fn fs_main_lamps(in: VOut) -> LampsOut {
    let s = shade(in, false);
    var out: LampsOut;
    out.color = s.out.color;
    out.glow = s.out.glow;
    let face = normalize(in.normal);
    var n = face;
    if (in.shade.w > 0.5) {
        n = normalize(in.shade.xyz);
    }
    let off = in.centre.w > 0.5;
    var cell = vec3<i32>(floor(in.world - face * 0.5) + face);
    if (off) {
        cell = vec3<i32>(floor(in.centre.xyz + face));
    }
    let keep = pack4x8unorm(vec4<f32>(clamp(s.keep, vec3<f32>(0.0), vec3<f32>(1.0)), 0.0)) & 0xffffffu;
    let way = vec3<u32>(round(clamp(n, vec3<f32>(-1.0), vec3<f32>(1.0)) * 15.0) + 15.0);
    let w = (bitcast<u32>(cell.z) & 0xffffu) | (way.x << 16u) | (way.y << 21u) | (way.z << 26u);
    if (off) {
        // Where its way to the moving things begins (lamps.wgsl's
        // `movers_from`), from the cell's corner.
        let begins = select(in.centre.xyz + face * (0.5 + FACE_OUT), in.centre.xyz + n, in.shade.w > 0.5);
        let q = vec3<u32>(clamp(round((begins - vec3<f32>(cell) + MOVERS_FROM) * 256.0), vec3<f32>(0.0), vec3<f32>(1023.0)));
        out.surface = vec4<u32>(keep | (3u << 24u) | ((q.z >> 4u) << 26u), (bitcast<u32>(cell.x) & 0xfffffu) | (q.x << 20u) | ((q.z & 3u) << 30u), (bitcast<u32>(cell.y) & 0xfffffu) | (q.y << 20u) | (((q.z >> 2u) & 3u) << 30u), w);
        return out;
    }
    out.surface = vec4<u32>(keep | (1u << 24u) | (face_index(face) << 26u), bitcast<u32>(cell.x), bitcast<u32>(cell.y), w);
    return out;
}

// A fragment shaded, and the colour the lights fall on (what multiplies
// their light in it: its own, less what glows, mirrors or the haze hides).
struct Shaded {
    out: FOut,
    keep: vec3<f32>,
};

// The scene's shading of a fragment; with `lights` false without the
// flashes', explosions' and lamps' light (the lamps' pass adds it).
fn shade(in: VOut, lights: bool) -> Shaded {
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
    var lit = in.color * (sun * ao);
    if (lights) {
        lit = in.color * (sun * ao + flare_light(in.world, n));
    }
    // Glowing things shine in their own colour, whatever the light.
    let glow = in.fx.x;
    var color = mix(lit, in.color * 1.15, glow);
    var keep = in.color * (1.0 - glow);

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
        keep *= 1.0 - m;
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
        keep *= 1.0 - fresnel;
        opacity = mix(opacity, 1.0, clamp(fresnel + max(glint.r, max(glint.g, glint.b)), 0.0, 1.0));
        // The glints glow a little too, so they sparkle.
        shine = glint;
    }

    let haze = clamp((distance(in.world, g.eye.xyz) - g.haze.w) * g.misc.x, 0.0, 1.0) * g.eye.w;
    color = mix(color, g.haze.rgb, haze * (1.0 - glow));
    keep *= 1.0 - haze * (1.0 - glow);

    var out: FOut;
    out.color = vec4<f32>(color, opacity);
    // The sweep glows a little too, so it sparkles.
    out.glow = vec4<f32>(in.color * glow + shine * 0.3, opacity);
    return Shaded(out, keep);
}
