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
