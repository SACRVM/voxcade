// Fog (render::fog): air within the level's box that holds light. What
// the beams' pass (fog_beams.wgsl) and the composite laying the fog over
// the picture (fog_composite.wgsl) share: the fog's numbers, the view ray,
// the stretch of it in the air. Each of them binds `fog` itself.

struct Fog {
    inv_view_proj: mat4x4<f32>,
    // xyz: the eye, w: the density (what a voxel of air takes from the
    // light passing through it)
    eye: vec4<f32>,
    // rgb: the fog's own colour, where no lamp lights it; w: how much it
    // sends the light on ahead (Henyey-Greenstein's g, -0.9 to 0.9)
    color: vec4<f32>,
    // xyz: the air's low corner (the level's), w: the density the beams
    // are lit by (what the fog sends on of a lamp's light per voxel)
    lo: vec4<f32>,
    // xyz: the air's high corner, w: unused
    hi: vec4<f32>,
    // xyz: the way the camera looks, w: unused
    forward: vec4<f32>,
    // xy: the beams' target's size, z: how many of the picture's pixels a
    // pixel of it is each way, w: unused
    size: vec4<u32>,
};

// The farthest the beams' target says the eye sees (its alpha, a half
// float: the sky's 1e9 would be infinite).
const FOG_FAR: f32 = 60000.0;

// The view ray through the point `px` of a picture `full` pixels big, to
// the depth `depth` there: its way (unit, xyz) and how far it goes to what
// it meets (w; past the level for the sky).
fn view_ray(px: vec2<f32>, full: vec2<f32>, depth: f32) -> vec4<f32> {
    let uv = px / full;
    let at = fog.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let d = at.xyz / at.w - fog.eye.xyz;
    let len = length(d);
    return vec4<f32>(d / len, select(len, 1e9, depth >= 1.0));
}

// Where the ray from the eye the way `dir` is in the air (the level's
// box), up to `far`: from (x) and to (y); to not past from: nowhere.
fn air_span(dir: vec3<f32>, far: f32) -> vec2<f32> {
    let safe = select(dir, vec3<f32>(1e-7), abs(dir) < vec3<f32>(1e-7));
    let a = (fog.lo.xyz - fog.eye.xyz) / safe;
    let b = (fog.hi.xyz - fog.eye.xyz) / safe;
    let t0 = max(max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z)), 0.0);
    let t1 = min(min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z)), far);
    return vec2<f32>(t0, t1);
}

// The beams at the picture's pixel `px` that sees `seen` voxels far, from
// their smaller target `beams`: of its four pixels round this one, each
// weighed by how near it is and how alike the distance it was worked out
// at (its alpha) is to `seen`, so a beam behind a pillar does not bleed
// over the pillar's edge.
fn beams_at(beams: texture_2d<f32>, px: vec2<i32>, seen: f32) -> vec3<f32> {
    let low = (vec2<f32>(px) + vec2<f32>(0.5)) / f32(fog.size.z) - vec2<f32>(0.5);
    let base = floor(low);
    let f = low - base;
    let last = vec2<i32>(fog.size.xy) - vec2<i32>(1);
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    var best = vec3<f32>(0.0);
    var best_gap = 1e30;
    for (var k = 0; k < 4; k += 1) {
        let d = vec2<f32>(f32(k & 1), f32(k >> 1u));
        let beam = textureLoad(beams, clamp(vec2<i32>(base + d), vec2<i32>(0), last), 0);
        // How far the eye saw where that pixel was worked out (0: no
        // light reaches there, nothing to weigh).
        let gap = select(abs(beam.a - seen), 0.0, beam.a <= 0.0);
        let near = (1.0 - abs(d.x - f.x)) * (1.0 - abs(d.y - f.y));
        let alike = 1.0 / (1.0 + gap / (0.01 * seen + 1.0));
        let w = near * alike * alike * alike * alike;
        sum += beam.rgb * w;
        weight += w;
        if (gap < best_gap) {
            best_gap = gap;
            best = beam.rgb;
        }
    }
    return select(best, sum / max(weight, 1e-6), weight > 1e-3);
}
