// The picture put together with fog (render::fog): post.wgsl's composite,
// the fog laid over the scene before the UI. At each pixel the scene times
// what the fog left of it on the way there, the fog's own colour for the
// rest, and the beams from their smaller target: of its four pixels round
// this one, each weighed by how near it is and how alike the depth it was
// worked out at (its alpha) is to this pixel's, so a beam behind a pillar
// does not bleed over the pillar's edge.

@group(2) @binding(0) var<uniform> fog: Fog;
@group(2) @binding(1) var scene_depth: texture_depth_2d;
@group(2) @binding(2) var beams: texture_2d<f32>;

fn fogged(in: PostOut, scene: vec3<f32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(scene_depth));
    let px = min(vec2<i32>(in.uv * vec2<f32>(size)), size - vec2<i32>(1));
    let ray = view_ray(vec2<f32>(px) + vec2<f32>(0.5), vec2<f32>(size), textureLoad(scene_depth, px, 0));
    let air = air_span(ray.xyz, ray.w);
    let kept = exp(-fog.eye.w * max(air.y - air.x, 0.0));
    let seen = min(ray.w, FOG_FAR);
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
    let beam = select(best, sum / max(weight, 1e-6), weight > 1e-3);
    return scene * kept + fog.color.rgb * (1.0 - kept) + beam;
}

@fragment
fn fs_composite_fog(in: PostOut) -> @location(0) vec4<f32> {
    return vec4<f32>(finish(in, fogged(in, scene_color(in))), 1.0);
}

@fragment
fn fs_composite_fog_srgb(in: PostOut) -> @location(0) vec4<f32> {
    return vec4<f32>(decoded(finish(in, fogged(in, scene_color(in)))), 1.0);
}
