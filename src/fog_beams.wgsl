// The beams (render::fog): the lamps' light the fog sends to the eye,
// into a target a fraction of the picture's size. Drawn as the lamps' pass
// draws (lamps.wgsl's tiles, `vs_lamp`, and its walls): each tile with the
// lights whose box reaches it. Along each pixel's view ray, through the
// stretch of it where a light can reach (its ball, a spot's cone too), the
// light it gets at a point times what the fog sends on towards the eye,
// less what the fog took before it got there. A lamp walls stop lights a
// point only where no solid voxel lies between it and the middle of the
// point's voxel (the faces' memory asked, under keys of its own: a still
// scene walks each way once), so a beam ends at a wall, however thin, and
// a pillar cuts a dark slot into it; the slot's edges step voxel by voxel,
// as the faces' light does, fixed in the level.
//
// No noise: the stretch is taken at the planes across the camera's way
// every FOG_STEP voxels (fixed in the level, so a scrolling view and a
// turning spot take the same points), the light between them added up as
// a straight line, and where a wall's shadow begins or ends between two
// points it is sought out (FOG_EDGE_STEPS halvings) rather than guessed.
// A shadow thinner along the ray than the planes' spacing that lies
// between two points that both see the lamp (a thin post's, seen edge on)
// does not show in the fog.

@group(1) @binding(0) var<uniform> fog: Fog;
// The picture's lights for the fog, as `lights` (their colour times how
// strongly each shows in fog).
@group(1) @binding(1) var fog_lights: texture_2d<f32>;
// The see-through cubes' depth, with glass or water in the picture
// (`fs_beam_glass`).
@group(1) @binding(2) var glass_depth: texture_depth_2d;

// The planes' spacing, voxels along the camera's way, for a light whose
// ball is at most FOG_MOST_PLANES of them across; twice (four times ...)
// as far for a wider one.
const FOG_STEP: f32 = 4.0;
const FOG_MOST_PLANES: f32 = 48.0;
// Halvings seeking where a wall's shadow begins between two points.
const FOG_EDGE_STEPS: i32 = 4;

// How much of the light travelling the way `cos_angle` from the eye's way
// back (1: straight towards the eye) the fog sends on to the eye
// (Henyey-Greenstein's, 1 for fog that sends every way alike).
fn phase(cos_angle: f32) -> f32 {
    let g = fog.color.w;
    let d = 1.0 + g * g - 2.0 * g * cos_angle;
    return (1.0 - g * g) / (d * sqrt(d));
}

// Where along the ray from the eye the way `dir` it lies within the
// spot's cone (`l`, its front half) whose half's cosine is `c`: from (x)
// to (y), to not past from: nowhere. A cone of 180 degrees or more:
// everywhere.
fn cone_span(l: Light, dir: vec3<f32>, c: f32) -> vec2<f32> {
    let none = vec2<f32>(1.0, 0.0);
    if (c <= 0.0) {
        return vec2<f32>(-1e9, 1e9);
    }
    let axis = l.spot.xyz;
    let co = fog.eye.xyz - l.pos_radius.xyz;
    let da = dot(dir, axis);
    let ca = dot(co, axis);
    let c2 = c * c;
    // Ahead of the apex: (co + t dir) . axis >= 0.
    var lo = -1e9;
    var hi = 1e9;
    if (abs(da) < 1e-7) {
        if (ca < 0.0) {
            return none;
        }
    } else if (da > 0.0) {
        lo = -ca / da;
    } else {
        hi = -ca / da;
    }
    // Within the double cone: a t^2 + b t + k >= 0.
    let a = da * da - c2;
    let b = 2.0 * (da * ca - c2 * dot(dir, co));
    let k = ca * ca - c2 * dot(co, co);
    if (abs(a) < 1e-7) {
        if (abs(b) < 1e-9) {
            return select(vec2<f32>(lo, hi), none, k < 0.0);
        }
        if (b > 0.0) {
            return vec2<f32>(max(lo, -k / b), hi);
        }
        return vec2<f32>(lo, min(hi, -k / b));
    }
    let disc = b * b - 4.0 * a * k;
    if (disc < 0.0) {
        return select(vec2<f32>(lo, hi), none, a < 0.0);
    }
    let s = sqrt(disc);
    let r = vec2<f32>((-b - s) / (2.0 * a), (-b + s) / (2.0 * a));
    let r1 = min(r.x, r.y);
    let r2 = max(r.x, r.y);
    if (a < 0.0) {
        return vec2<f32>(max(lo, r1), min(hi, r2));
    }
    // The ray goes through both halves: the front one is its far part
    // going the cone's way, its near part going against it.
    if (da > 0.0) {
        return vec2<f32>(max(lo, r2), hi);
    }
    return vec2<f32>(lo, min(hi, r1));
}

// The light `l` sends towards the eye from the point `p` on the view ray
// the way `dir`, per voxel of fog, walls aside.
fn scatter(l: Light, p: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let to = l.pos_radius.xyz - p;
    let d = max(length(to), 0.001);
    let fall = 1.0 - d / l.pos_radius.w;
    if (fall <= 0.0) {
        return vec3<f32>(0.0);
    }
    var lit = l.color.rgb * fall * fall;
    if (l.color.w != 2.0) {
        lit = l.color.rgb * pow(fall, l.color.w);
    }
    if (l.spot.w > -1.5) {
        lit *= in_cone(l, -to / d);
    }
    return lit * phase(dot(to, dir) / d);
}

// Whether the lamp `l` reaches the middle of the voxel the point `p` is
// in, walls stopping it: the faces' memory asked as for the face in front
// of that voxel (the same walk, the same answer).
fn reaches(l: Light, p: vec3<f32>, movers: u32) -> bool {
    // WALLS {
    if (l.edge.y > 0.5) {
        let cell = vec3<i32>(floor(p));
        if (solid(cell - lamps.origin.xyz, 0)) {
            return false;
        }
        let ids = vec2<u32>(mix_hash(0x9e3779b9u, cell), mix_hash(0x85ebca6bu, cell));
        // A lamp whose shadows are "all": what moves too, asked afresh,
        // only the boxes whose shadows the ray meets (`movers`, `shading`).
        return !stopped(cell, 0u, l.pos_radius.xyz, ids, vec2<u32>(l.edge.zw)) && !(movers != 0u && movers_stop(vec3<f32>(cell) + vec3<f32>(0.5), l.pos_radius.xyz, 0.0, u32(l.edge.y - 2.0), movers));
    }
    // } WALLS
    return true;
}

// The light `l` sends to the eye along the ray the way `dir` from `a` to
// `b`, the fog taking its share from `air` on: points at the planes
// between, and where it enters and leaves a spot's hard core (`core`: the
// light bends there), each the light it gets (unstopped, `scatter`) and
// whether it gets it; where that changes between two points, where it
// changes. The fog takes its share by the density alone; the glow
// (`fog.hi.w`) scales what it sends towards the eye: above 1 up to the
// brightest light the lamp gives along the ray (`most`), so raising it
// never makes a beam fainter anywhere, nor brighter than its lamp.
fn beam(l: Light, dir: vec3<f32>, a: f32, b: f32, core: vec2<f32>, air: f32, movers: u32) -> vec3<f32> {
    let density = fog.eye.w;
    let eye = fog.eye.xyz;
    let ahead = max(dot(dir, fog.forward.xyz), 0.05);
    let step = FOG_STEP * exp2(max(ceil(log2(2.0 * l.pos_radius.w / (FOG_STEP * FOG_MOST_PLANES))), 0.0));
    let start = dot(eye, fog.forward.xyz);
    let stopping = l.edge.y > 0.5;
    var t0 = a;
    let s0 = scatter(l, eye + dir * a, dir);
    var f0 = s0 * exp(-density * (a - air));
    var v0 = !stopping || reaches(l, eye + dir * a, movers);
    var sum = vec3<f32>(0.0);
    var most = select(vec3<f32>(0.0), s0, v0);
    for (var k = 0; k < i32(FOG_MOST_PLANES) + 4; k += 1) {
        // The next plane, or the core's edge if it comes first.
        var t1 = ((floor((start + t0 * ahead) / step + 1e-4) + 1.0) * step - start) / ahead;
        if (core.x > t0 + 1e-3) {
            t1 = min(t1, core.x);
        }
        if (core.y > t0 + 1e-3) {
            t1 = min(t1, core.y);
        }
        t1 = min(t1, b);
        let p1 = eye + dir * t1;
        let s1 = scatter(l, p1, dir);
        let f1 = s1 * exp(-density * (t1 - air));
        let v1 = !stopping || reaches(l, p1, movers);
        if (v1) {
            most = max(most, s1);
        }
        if (v0 == v1) {
            if (v0) {
                sum += (f0 + f1) * 0.5 * (t1 - t0);
            }
        } else {
            // Where the lamp's light begins or ends between them.
            var lo = t0;
            var hi = t1;
            for (var i = 0; i < FOG_EDGE_STEPS; i += 1) {
                let mid = (lo + hi) * 0.5;
                if (reaches(l, eye + dir * mid, movers) == v0) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let edge = (lo + hi) * 0.5;
            let between = mix(f0, f1, (edge - t0) / max(t1 - t0, 1e-6));
            if (v0) {
                sum += (f0 + between) * 0.5 * (edge - t0);
            } else {
                sum += (between + f1) * 0.5 * (t1 - edge);
            }
        }
        if (t1 >= b) {
            break;
        }
        t0 = t1;
        f0 = f1;
        v0 = v1;
    }
    let glow = fog.hi.w;
    if (glow > 1.0) {
        return min(sum * density * glow, most);
    }
    return sum * density * glow;
}

// Whether the box round `middle`, `half` each way, meets the triangle
// `a`, `b`, `c` (Akenine-Möller's separating axes: the box's three, the
// triangle's plane, the nine across the edges).
fn box_meets_triangle(middle: vec3<f32>, half: vec3<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> bool {
    var v = array<vec3<f32>, 3>(a - middle, b - middle, c - middle);
    if (any(min(min(v[0], v[1]), v[2]) > half) || any(max(max(v[0], v[1]), v[2]) < -half)) {
        return false;
    }
    var e = array<vec3<f32>, 3>(v[1] - v[0], v[2] - v[1], v[0] - v[2]);
    let n = cross(e[0], e[1]);
    if (abs(dot(n, v[0])) > dot(half, abs(n))) {
        return false;
    }
    for (var i = 0; i < 3; i += 1) {
        let ed = e[i];
        var axes = array<vec3<f32>, 3>(vec3<f32>(0.0, -ed.z, ed.y), vec3<f32>(ed.z, 0.0, -ed.x), vec3<f32>(-ed.y, ed.x, 0.0));
        for (var j = 0; j < 3; j += 1) {
            let ax = axes[j];
            let p = vec3<f32>(dot(ax, v[0]), dot(ax, v[1]), dot(ax, v[2]));
            let r = dot(half, abs(ax));
            if (min(p.x, min(p.y, p.z)) > r || max(p.x, max(p.y, p.z)) < -r) {
                return false;
            }
        }
    }
    return true;
}

// Which of the boxes of a lamp whose shadows are "all" (`l`, its list in
// `edge.y`) can shade the stretch of a ray from `near` to `far`: those
// that meet the triangle of the lamp and the stretch (the ways from the
// lamp to every point of it; grown by a voxel, as a point is asked at its
// voxel's middle), a bit each of the first 32
// (past 32 all of them). Once a ray, so its points walk only those: most
// rays through the fog pass no moving thing's shadow.
fn shading(l: Light, near: vec3<f32>, far: vec3<f32>) -> u32 {
    var mask = 0u;
    // WALLS {
    let list = u32(l.edge.y - 2.0);
    let n = list & 255u;
    if (lamps.movers_dims.w == 0) {
        return 0u;
    }
    if (n > 32u) {
        return 0xffffffffu;
    }
    let start = list >> 8u;
    let corner = vec3<f32>(lamps.movers_origin.xyz);
    for (var i = 0u; i < n; i += 1u) {
        let k = i32(mover_lists[start + i]);
        let lo = corner + vec3<f32>(mover_boxes[2 * k].xyz);
        let hi = corner + vec3<f32>(mover_boxes[2 * k + 1].xyz);
        if (box_meets_triangle((lo + hi) * 0.5, (hi - lo) * 0.5 + vec3<f32>(1.0), l.pos_radius.xyz, near, far)) {
            mask |= 1u << i;
        }
    }
    // } WALLS
    return mask;
}

// The light the tile's lights send to the eye along the ray the way `dir`
// from `begin` to `end`, the fog taking its share from `air` on.
fn beams_over(in: TileOut, dir: vec3<f32>, begin: f32, end: f32, air: f32) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    for (var k = 0u; k < in.count.x; k += 1u) {
        let row = light_row(in, k);
        let l = Light(textureLoad(fog_lights, vec2<i32>(0, row), 0), textureLoad(fog_lights, vec2<i32>(1, row), 0), textureLoad(fog_lights, vec2<i32>(2, row), 0), textureLoad(fog_lights, vec2<i32>(3, row), 0));
        // Where the ray is within its ball, its cone, the stretch.
        let oc = fog.eye.xyz - l.pos_radius.xyz;
        let h = dot(dir, oc);
        let disc = h * h - (dot(oc, oc) - l.pos_radius.w * l.pos_radius.w);
        if (disc <= 0.0) {
            continue;
        }
        let root = sqrt(disc);
        var a = max(-h - root, begin);
        var b = min(-h + root, end);
        var core = vec2<f32>(-1.0);
        if (l.spot.w > -1.5) {
            let cone = cone_span(l, dir, l.spot.w);
            a = max(a, cone.x);
            b = min(b, cone.y);
            if (l.edge.x > l.spot.w) {
                core = cone_span(l, dir, l.edge.x);
            }
        }
        if (b > a) {
            let movers = select(0u, shading(l, fog.eye.xyz + dir * a, fog.eye.xyz + dir * b), l.edge.y > 1.5);
            sum += beam(l, dir, a, b, core, air, movers);
        }
    }
    return sum;
}

// The picture's pixel in the middle of this one's (of the beams' target).
fn beams_pixel(own: vec2<f32>) -> vec2<i32> {
    let scale = f32(fog.size.z);
    let full = vec2<i32>(textureDimensions(scene_depth));
    return min(vec2<i32>(floor(own) * scale + vec2<f32>(scale * 0.5)), full - vec2<i32>(1));
}

@fragment
fn fs_beam(in: TileOut) -> @location(0) vec4<f32> {
    let px = beams_pixel(in.clip.xy);
    let full = vec2<f32>(textureDimensions(scene_depth));
    let ray = view_ray(vec2<f32>(px) + vec2<f32>(0.5), full, textureLoad(scene_depth, px, 0));
    // (Its alpha how far the eye sees there, for fog_composite.wgsl.)
    let seen = min(ray.w, FOG_FAR);
    let air = air_span(ray.xyz, ray.w);
    if (air.y <= air.x) {
        return vec4<f32>(0.0, 0.0, 0.0, seen);
    }
    return vec4<f32>(beams_over(in, ray.xyz, air.x, air.y, air.x), seen);
}

// With glass or water in the picture (`glass_depth`, the see-through
// cubes' depth): the beams before the nearer see-through cube (`near`, for
// the composite) and behind it (`far`, fog_far.wgsl, their light taken by
// the fog from the cube on), each its alpha how far it reaches.
struct BeamsOut {
    @location(0) near: vec4<f32>,
    @location(1) far: vec4<f32>,
};

@fragment
fn fs_beam_glass(in: TileOut) -> BeamsOut {
    let px = beams_pixel(in.clip.xy);
    let full = vec2<f32>(textureDimensions(scene_depth));
    let ray = view_ray(vec2<f32>(px) + vec2<f32>(0.5), full, textureLoad(scene_depth, px, 0));
    let glass = min(view_ray(vec2<f32>(px) + vec2<f32>(0.5), full, textureLoad(glass_depth, px, 0)).w, ray.w);
    let air = air_span(ray.xyz, ray.w);
    var out: BeamsOut;
    out.near = vec4<f32>(0.0, 0.0, 0.0, min(glass, FOG_FAR));
    out.far = vec4<f32>(0.0, 0.0, 0.0, min(ray.w, FOG_FAR));
    if (air.y <= air.x) {
        return out;
    }
    let split = clamp(glass, air.x, air.y);
    out.near = vec4<f32>(beams_over(in, ray.xyz, air.x, split, air.x), out.near.a);
    if (split < air.y) {
        out.far = vec4<f32>(beams_over(in, ray.xyz, split, air.y, split), out.far.a);
    }
    return out;
}
