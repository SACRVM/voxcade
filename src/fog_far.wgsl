// The fog behind glass and water (render::fog), laid over the scene before
// its see-through cubes are drawn onto it, so they lie over it and tint
// it: from the nearer see-through cube (`glass_depth`) to the opaque scene,
// what the fog took (the scene times what it left, blended), its own
// colour for the rest, and the beams there (`far`, worked out from the
// see-through cube on). Where no see-through cube lies before the scene
// nothing changes. The composite lays the fog before them.

@group(0) @binding(0) var<uniform> fog: Fog;
@group(0) @binding(1) var scene_depth: texture_depth_2d;
@group(0) @binding(2) var glass_depth: texture_depth_2d;
@group(0) @binding(3) var far: texture_2d<f32>;

struct FarOut {
    @builtin(position) clip: vec4<f32>,
};

// One triangle covering the screen.
@vertex
fn vs_far(@builtin(vertex_index) i: u32) -> FarOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: FarOut;
    out.clip = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return out;
}

@fragment
fn fs_fog_far(in: FarOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    // No see-through cube here: nothing to change.
    let glass_at = textureLoad(glass_depth, px, 0);
    if (glass_at >= 1.0) {
        discard;
    }
    let size = vec2<f32>(textureDimensions(scene_depth));
    let ray = view_ray(vec2<f32>(px) + vec2<f32>(0.5), size, textureLoad(scene_depth, px, 0));
    let glass = view_ray(vec2<f32>(px) + vec2<f32>(0.5), size, glass_at).w;
    let air = air_span(ray.xyz, ray.w);
    let begins = max(air.x, glass);
    if (begins >= air.y) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let kept = exp(-fog.eye.w * (air.y - begins));
    let beam = beams_at(far, px, min(ray.w, FOG_FAR));
    return vec4<f32>(beam + fog.color.rgb * (1.0 - kept), kept);
}
