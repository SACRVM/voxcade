// The picture put together with fog (render::fog): post.wgsl's composite,
// the fog laid over the scene before the UI. At each pixel the scene times
// what the fog left of it on the way there, the fog's own colour for the
// rest, and the beams from their smaller target (`beams_at`). With glass
// or water in the picture the fog behind it was laid over before it
// (fog_far.wgsl): here only what lies before it, up to the nearer of the
// opaque scene and the see-through cubes (`glass_depth`).

@group(2) @binding(0) var<uniform> fog: Fog;
@group(2) @binding(1) var scene_depth: texture_depth_2d;
@group(2) @binding(2) var beams: texture_2d<f32>;
@group(2) @binding(3) var glass_depth: texture_depth_2d;

// The scene with the fog over it (rgb) and what the fog left of what lies
// there (a: the glow is dimmed alike), up to the opaque scene or, `glass`,
// the nearer see-through cube.
fn fogged(in: PostOut, scene: vec3<f32>, glass: bool) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(scene_depth));
    let px = min(vec2<i32>(in.uv * vec2<f32>(size)), size - vec2<i32>(1));
    var ray = view_ray(vec2<f32>(px) + vec2<f32>(0.5), vec2<f32>(size), textureLoad(scene_depth, px, 0));
    if (glass) {
        let glass_at = textureLoad(glass_depth, px, 0);
        if (glass_at < 1.0) {
            ray.w = min(ray.w, view_ray(vec2<f32>(px) + vec2<f32>(0.5), vec2<f32>(size), glass_at).w);
        }
    }
    let air = air_span(ray.xyz, ray.w);
    let kept = exp(-fog.eye.w * max(air.y - air.x, 0.0));
    let beam = beams_at(beams, px, min(ray.w, FOG_FAR));
    return vec4<f32>(scene * kept + fog.color.rgb * (1.0 - kept) + beam, kept);
}

@fragment
fn fs_composite_fog(in: PostOut) -> @location(0) vec4<f32> {
    let f = fogged(in, scene_color(in), false);
    return vec4<f32>(finish_seen(in, f.rgb, f.a), 1.0);
}

@fragment
fn fs_composite_fog_srgb(in: PostOut) -> @location(0) vec4<f32> {
    let f = fogged(in, scene_color(in), false);
    return vec4<f32>(decoded(finish_seen(in, f.rgb, f.a)), 1.0);
}

@fragment
fn fs_composite_fog_glass(in: PostOut) -> @location(0) vec4<f32> {
    let f = fogged(in, scene_color(in), true);
    return vec4<f32>(finish_seen(in, f.rgb, f.a), 1.0);
}

@fragment
fn fs_composite_fog_glass_srgb(in: PostOut) -> @location(0) vec4<f32> {
    let f = fogged(in, scene_color(in), true);
    return vec4<f32>(decoded(finish_seen(in, f.rgb, f.a)), 1.0);
}
