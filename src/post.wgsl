// Voxcade post-processing: shrink and blur the scene and what glows, then
// put the picture together: glow on top, blur towards the screen's top and
// bottom, a white flash after big blasts, and a level change's fade.

struct Post {
    // x: glow strength, y: tilt-shift strength, z: screen flash, w: unused
    params: vec4<f32>,
    // xy: one texel of the source, zw: blur direction; in the composite:
    // the fade's colour and how far it has gone
    step: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> post: Post;

struct PostOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle covering the screen.
@vertex
fn vs_post(@builtin(vertex_index) i: u32) -> PostOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: PostOut;
    out.clip = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

// Four bilinear taps: a 4x4 box, for shrinking to a quarter.
@fragment
fn fs_down(in: PostOut) -> @location(0) vec4<f32> {
    let t = post.step.xy;
    var sum = textureSample(src, src_sampler, in.uv + vec2<f32>(-t.x, -t.y));
    sum += textureSample(src, src_sampler, in.uv + vec2<f32>(t.x, -t.y));
    sum += textureSample(src, src_sampler, in.uv + vec2<f32>(-t.x, t.y));
    sum += textureSample(src, src_sampler, in.uv + vec2<f32>(t.x, t.y));
    return sum * 0.25;
}

// Nine-tap gaussian along `post.step.zw`.
@fragment
fn fs_blur(in: PostOut) -> @location(0) vec4<f32> {
    let d = post.step.zw * post.step.xy;
    var w = array<f32, 5>(0.227, 0.194, 0.121, 0.054, 0.016);
    var sum = textureSample(src, src_sampler, in.uv) * w[0];
    for (var i = 1; i < 5; i += 1) {
        let o = d * f32(i) * 1.5;
        sum += textureSample(src, src_sampler, in.uv + o) * w[i];
        sum += textureSample(src, src_sampler, in.uv - o) * w[i];
    }
    return sum;
}

@group(1) @binding(0) var scene_blur: texture_2d<f32>;
@group(1) @binding(1) var glow_blur: texture_2d<f32>;
@group(1) @binding(2) var ui_layer: texture_2d<f32>;

@fragment
fn fs_composite(in: PostOut) -> @location(0) vec4<f32> {
    let sharp = textureSample(src, src_sampler, in.uv).rgb;
    let soft = textureSample(scene_blur, src_sampler, in.uv).rgb;
    let glow = textureSample(glow_blur, src_sampler, in.uv).rgb;
    // Sharp over most of the screen, softening only near the top and bottom.
    let edge = smoothstep(0.6, 1.0, abs(in.uv.y - 0.5) * 2.0) * post.params.y;
    var color = mix(sharp, soft, edge);
    // The UI lies over the world, sharp; glow shines over both.
    let ui = textureSample(ui_layer, src_sampler, in.uv);
    color = mix(color, ui.rgb, ui.a);
    color += glow * post.params.x;
    color = mix(color, vec3<f32>(1.0), post.params.z);
    color = mix(color, post.step.rgb, post.step.w);
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
