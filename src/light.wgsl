// A light as both the scene's shader and the lamps' pass take it
// (render::shader::LightData, render::lamps): what it gives a point, walls
// aside. Written ahead of both (render::shader::SHADER, LAMPS), so the two
// light alike.

struct Light {
    // xyz: position, w: radius
    pos_radius: vec4<f32>,
    // rgb: colour times brightness, w: the falloff's power ((1 - distance
    // / radius) to it; 2 as lights always were)
    color: vec4<f32>,
    // xyz: the way a spot shines, w: the cosine of half its cone (below
    // -1: it shines every way)
    spot: vec4<f32>,
    // x: the cosine within which the cone's edge is no longer soft; y: 1
    // when walls stop it; z, w: two 24-bit hashes of its exact place (the
    // lamps' memory's key, lamps.wgsl)
    edge: vec4<f32>,
};

// The light `l` gives a point at `world` lit by the way `n`, as though no
// wall stood between them.
fn light_on(l: Light, world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let to = l.pos_radius.xyz - world;
    let d = length(to);
    let r = l.pos_radius.w;
    if (d >= r) {
        return vec3<f32>(0.0);
    }
    let fall = 1.0 - d / r;
    // Faces turned away still catch a little: a burst lights round it.
    let facing = max(dot(n, to / max(d, 0.001)), 0.0) * 0.75 + 0.25;
    var lit = l.color.rgb * fall * fall * facing;
    if (l.color.w != 2.0) {
        lit = l.color.rgb * pow(fall, l.color.w) * facing;
    }
    if (l.spot.w > -1.5) {
        lit *= in_cone(l, -to / max(d, 0.001));
    }
    return lit;
}

// How much of a spot's light goes the way `way` (unit, from the light):
// all of it inside its cone's hard part, none outside the cone, softly
// between.
fn in_cone(l: Light, way: vec3<f32>) -> f32 {
    let c = dot(way, l.spot.xyz);
    if (l.edge.x <= l.spot.w) {
        return select(0.0, 1.0, c >= l.spot.w);
    }
    return smoothstep(l.spot.w, l.edge.x, c);
}
