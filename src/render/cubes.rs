//! Passes over a frame's cubes on the CPU: faces a neighbour covers left
//! out, cubes nothing of which shows dropped, see-through bodies joined.

use glam::Vec3;

use super::{FX_COVERED, FX_OPACITY, Instance};

/// Up to this many cells around the cubes `hide_inner_faces` keeps a bit per
/// cell (2 MB); more, and it looks them up in a hash set.
const DENSE_CELLS: i64 = 1 << 24;

/// Marks besides the faces of opaque unit cubes that lie against a cell
/// `taken` says is filled (a figure's part drawn as cubes against the
/// part being shaped, which is the world: one covered face otherwise won
/// single pixels at the seam).
pub fn hide_faces_against(instances: &mut [Instance], taken: impl Fn(glam::IVec3) -> bool) {
    for i in instances.iter_mut().filter(|i| i.pos_size[3] == 1.0 && i.fx[FX_OPACITY] == 255 && i.turn == [0; 4]) {
        let c = glam::Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
        for (bit, d) in crate::mesh::DIRS.iter().enumerate() {
            if taken(c + *d) {
                i.fx[FX_COVERED] |= 1 << bit;
            }
        }
    }
}

/// Marks the faces of opaque unit cubes that touch another opaque unit
/// cube, so they are not drawn: they are never seen, and where such a face
/// meets the neighbour's surface at the same depth it shows as a seam.
/// Runs every frame over the UI's tens of thousands of voxels, so which
/// cells are taken is a bit per cell of the box around them.
pub fn hide_inner_faces(instances: &mut [Instance]) {
    // (A cube turned freely lies on no cell: its own model marks its faces.)
    let solid = |i: &Instance| i.pos_size[3] == 1.0 && i.fx[FX_OPACITY] == 255 && i.turn == [0; 4];
    let cell = |i: &Instance| glam::Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
    let (mut lo, mut hi) = (glam::IVec3::MAX, glam::IVec3::MIN);
    for i in instances.iter().filter(|i| solid(i)) {
        lo = lo.min(cell(i));
        hi = hi.max(cell(i));
    }
    if lo.x > hi.x {
        return;
    }
    let size = hi.as_i64vec3() - lo.as_i64vec3() + 1;
    let volume = size.x.saturating_mul(size.y).saturating_mul(size.z);
    fn mark(instances: &mut [Instance], solid: impl Fn(&Instance) -> bool, cell: impl Fn(&Instance) -> glam::IVec3, taken: impl Fn(glam::IVec3) -> bool) {
        for i in instances.iter_mut().filter(|i| solid(i)) {
            let c = cell(i);
            let mut hidden = 0u8;
            for (bit, d) in crate::mesh::DIRS.iter().enumerate() {
                if taken(c + *d) {
                    hidden |= 1 << bit;
                }
            }
            i.fx[FX_COVERED] = hidden;
        }
    }
    if volume > DENSE_CELLS {
        let cells: crate::world::CellSet = instances.iter().filter(|i| solid(i)).map(cell).collect();
        mark(instances, solid, cell, |c| cells.contains(&c));
        return;
    }
    let (sx, sy) = (size.x as usize, size.y as usize);
    let index = |c: glam::IVec3| {
        let d = c - lo;
        (d.z as usize * sy + d.y as usize) * sx + d.x as usize
    };
    let mut bits = vec![0u64; (volume as usize).div_ceil(64)];
    for i in instances.iter().filter(|i| solid(i)) {
        let n = index(cell(i));
        bits[n / 64] |= 1 << (n % 64);
    }
    mark(instances, solid, cell, |c| {
        c.cmpge(lo).all() && c.cmple(hi).all() && {
            let n = index(c);
            bits[n / 64] >> (n % 64) & 1 == 1
        }
    });
}

/// Leaves out the opaque unit cubes of which nothing can show from `eye`
/// (the UI's camera): every face either covered (`hide_inner_faces`, run
/// first) or turned clearly away from the eye, which the pipeline culls. A
/// plate `[ui] depth` deep has its whole back layer so. The picture is the
/// same: what is left out made no fragment, and the rest keeps its order.
pub fn leave_out_unseen(instances: &mut Vec<Instance>, eye: Vec3) {
    instances.retain(|i| {
        let solid = i.pos_size[3] == 1.0 && i.fx[FX_OPACITY] == 255 && i.turn == [0; 4];
        if !solid {
            return true;
        }
        let centre = Vec3::from_slice(&i.pos_size[..3]);
        crate::mesh::DIRS.iter().enumerate().any(|(bit, d)| {
            let n = d.as_vec3();
            // (Half a voxel of room: a face edge-on, or nearly, is kept.)
            i.fx[FX_COVERED] >> bit & 1 == 0 && n.dot(eye - (centre + n * 0.5)) > -0.5
        })
    });
}

/// All of the eye's way (`toward`) that `join_see_through` goes by: the
/// three ways by how much they face it, the most first, and on which side
/// of each it is (true: the low one). Two eyes alike in this join the same
/// cubes the same (the editor keeps a joined room until it changes).
pub fn see_through_side(toward: Vec3) -> ([usize; 3], [bool; 3]) {
    let a = toward.abs();
    let first = if a.z >= 0.8 * a.max_element() { 2 } else if a.y >= a.x { 1 } else { 0 };
    let mut ways = [first, (first + 1) % 3, (first + 2) % 3];
    if a[ways[2]] > a[ways[1]] {
        ways.swap(1, 2);
    }
    (ways, [toward.x < 0.0, toward.y < 0.0, toward.z < 0.0])
}

/// Marks the faces two see-through unit cubes share, so water, glass and
/// the like are one joined body and not a grid of darker stripes where the
/// faces between cubes blend on top of each other. Only cubes in
/// neighbouring cells that lie alike within their cells (a column raised by
/// a wave does not, and keeps its faces) and are not turned freely join;
/// colours may differ. Opaque cubes are left alone, and so are bits already
/// set. Needs every see-through cube of the frame at once.
///
/// One sort of shared face stays (the user's idea): a skin for every layer
/// of the body, each a whole plane without a grid, so it shows how thick it
/// is by itself: three voxels of water are as dense as three sheets were.
/// The layers lie across the body where it is thin at that cube (water
/// lies flat, a pane of glass may stand, the user: the shortest run of
/// see-through cubes through it, along x, y or z, where that is half the
/// next or less); where it is about as thick every way, a block or a gem,
/// across the way turned most towards the eye (`toward`, from the cubes to
/// the eye; the upright one in a near tie, as a view from above at 45
/// degrees is). Of the two faces shared along that
/// way the one facing the eye stays.
pub fn join_see_through(instances: &mut [Instance], toward: Vec3) {
    let body = see_through_body(instances);
    join_by_body(instances, &body, toward);
}

/// What `join_see_through` finds of the cubes before it looks at the eye:
/// for each see-through cube that joins (its place among them), how many
/// cubes the body is through there along x, y and z, and which of its six
/// neighbours (`mesh::DIRS`) it joins. The editor keeps it for a big room
/// and joins it again only when the eye goes round to another side.
#[derive(Default)]
pub struct SeeThroughBody(Vec<(u32, [u8; 3], u8)>);

/// The body the see-through cubes of `instances` make (`SeeThroughBody`).
pub fn see_through_body(instances: &[Instance]) -> SeeThroughBody {
    let joins = |i: &Instance| i.pos_size[3] == 1.0 && i.fx[FX_OPACITY] < 255 && i.turn == [0; 4];
    let cell = |i: &Instance| glam::Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
    let mut at: crate::world::CellMap<glam::Vec3> = crate::world::CellMap::default();
    for i in instances.iter().filter(|i| joins(i)) {
        at.insert(cell(i), glam::Vec3::from_slice(&i.pos_size[..3]) - cell(i).as_vec3());
    }
    if at.len() < 2 {
        return SeeThroughBody(Vec::new());
    }
    // No run is counted further than this each way: thick is thick.
    const FAR: i32 = 16;
    let mut body = Vec::new();
    for (k, i) in instances.iter().enumerate().filter(|(_, i)| joins(i)) {
        let c = cell(i);
        let within = glam::Vec3::from_slice(&i.pos_size[..3]) - c.as_vec3();
        let joined = |to: glam::IVec3| at.get(&to).is_some_and(|w| (*w - within).abs().max_element() <= 0.01);
        // How many cubes the body is through here, along each way (at
        // most 1 + 2 FAR: a byte holds it).
        let run = |axis: usize| {
            let step = crate::mesh::DIRS[axis * 2];
            let count = |d: glam::IVec3| (1..=FAR).take_while(|n| joined(c + d * *n)).count();
            (1 + count(step) + count(-step)) as u8
        };
        let near = crate::mesh::DIRS.iter().enumerate().filter(|(_, d)| joined(c + **d)).fold(0u8, |bits, (bit, _)| bits | 1 << bit);
        body.push((k as u32, [run(0), run(1), run(2)], near));
    }
    SeeThroughBody(body)
}

/// Marks the shared faces of `body`'s cubes in `instances` (the cubes it
/// was found in, as they were) for the eye's way `toward`: what
/// `join_see_through` does after finding the body.
pub fn join_by_body(instances: &mut [Instance], body: &SeeThroughBody, toward: Vec3) {
    let (ways, _) = see_through_side(toward);
    for &(k, runs, near) in &body.0 {
        let runs = runs.map(usize::from);
        // Thin here: half as far through as any other way, or less. A
        // body about as thick every way (a gem's middle) goes by the view,
        // or its cubes would each choose their own and the skins lie in
        // patches.
        let mut sorted = runs;
        sorted.sort_unstable();
        let across = if sorted[0] * 2 <= sorted[1] { *ways.iter().find(|w| runs[**w] == sorted[0]).expect("one is the shortest") } else { ways[0] };
        let kept = across * 2 + (toward[across] < 0.0) as usize;
        let i = &mut instances[k as usize];
        for bit in 0..6 {
            if bit != kept && near >> bit & 1 == 1 {
                i.fx[FX_COVERED] |= 1 << bit;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Touching opaque unit cubes hide the faces between them, whether they
    /// sit close together (a bit per cell) or far apart (a hash set); a
    /// see-through cube neither hides nor gets hidden.
    /// A model draw only casting a shadow goes into the sun's map and not
    /// into the scene (nor its outline, nor the UI's); any other into
    /// both; one without voxels into neither.
    #[test]
    fn a_draw_only_casting_is_in_the_shadow_pass_alone() {
        let plain = crate::render::ModelDraw { voxels: 0..8, origin: glam::Vec3::ZERO, turn: 0, rot: glam::Quat::IDENTITY, shift: glam::Vec3::ZERO, tint: None, nearer: 0.0, shadow_only: false };
        let casting = crate::render::ModelDraw { shadow_only: true, ..plain.clone() };
        assert!(plain.in_pass(true) && plain.in_pass(false));
        assert!(casting.in_pass(true) && !casting.in_pass(false));
        let empty = crate::render::ModelDraw { voxels: 3..3, ..casting };
        assert!(!empty.in_pass(true) && !empty.in_pass(false));
    }

    #[test]
    fn touching_cubes_hide_the_faces_between_them() {
        use super::{Instance, hide_inner_faces};
        let cube = |x: f32, y: f32, z: f32| Instance { pos_size: [x + 0.5, y + 0.5, z + 0.5, 1.0], color: [255; 4], fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] };
        for far in [5.0, 1.0e6] {
            let mut v = vec![cube(0.0, 0.0, 0.0), cube(1.0, 0.0, 0.0), cube(0.0, 0.0, 1.0), cube(far, far, 0.0)];
            v.push(Instance { fx: [0, 128, 0, 0], ..cube(0.0, 1.0, 0.0) });
            hide_inner_faces(&mut v);
            // Bits in mesh::DIRS order: +x, -x, +y, -y, +z, -z.
            assert_eq!(v[0].fx[2], 0b01_0001, "the first cube's +x and +z faces (far cube at {far})");
            assert_eq!(v[1].fx[2], 0b00_0010, "-x");
            assert_eq!(v[2].fx[2], 0b10_0000, "-z");
            assert_eq!(v[3].fx[2], 0, "a cube on its own");
            assert_eq!(v[4].fx[2], 0, "a see-through cube");
        }
    }

    /// See-through cubes join where they lie alike in neighbouring cells:
    /// the faces they share go, a raised, turned or opaque neighbour keeps
    /// them, and a patch keeps only its outside.
    #[test]
    fn see_through_cubes_join_where_they_touch() {
        use super::{Instance, join_see_through};
        let cube = |x: f32, y: f32, z: f32| Instance { pos_size: [x + 0.5, y + 0.5, z + 0.5, 1.0], color: [50, 130, 200, 140], fx: [0, 140, 0, 0], shade: [0; 4], turn: [0; 4] };
        // Bits in mesh::DIRS order: +x, -x, +y, -y, +z, -z.
        let mut v = vec![cube(0.0, 0.0, 0.0), Instance { color: [255, 255, 255, 140], ..cube(1.0, 0.0, 0.0) }];
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!((v[0].fx[2], v[1].fx[2]), (0b01, 0b10), "the shared faces, different colors");
        let mut v = vec![cube(0.0, 0.0, 0.0), Instance { fx: [0, 255, 0, 0], ..cube(1.0, 0.0, 0.0) }];
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!((v[0].fx[2], v[1].fx[2]), (0, 0), "beside an opaque cube");
        let mut v = vec![cube(0.0, 0.0, 0.0), cube(1.0, 0.0, 0.3)];
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!((v[0].fx[2], v[1].fx[2]), (0, 0), "a neighbour raised by 0.3");
        let mut v = vec![cube(0.0, 0.0, 0.0), Instance { turn: [10, 0, 0, 100], ..cube(1.0, 0.0, 0.0) }];
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!((v[0].fx[2], v[1].fx[2]), (0, 0), "a freely turned neighbour");
        let mut v = vec![cube(0.0, 0.0, 0.0), cube(1.0, 0.0, 0.0)];
        v[0].fx[2] = 0b10_0000;
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!(v[0].fx[2], 0b10_0001, "bits set before stay");
        // A skin a layer, across the body where it is thinnest. Water two
        // deep and three wide each way, from above: the top between its
        // layers stays, every side between its cubes goes.
        let up = glam::Vec3::new(0.0, -0.7, 0.7);
        let slab = |v: &mut Vec<Instance>| {
            for z in 0..2 {
                for y in 0..3 {
                    for x in 0..3 {
                        v.push(cube(x as f32, y as f32, z as f32));
                    }
                }
            }
        };
        let mut v = Vec::new();
        slab(&mut v);
        join_see_through(&mut v, up);
        assert_eq!(v[4].fx[2], 0b00_1111, "the lower layer's middle: its sides go, its top stays (nothing below it)");
        assert_eq!(v[9 + 4].fx[2], 0b10_1111, "the upper layer's middle: its sides and its bottom go");
        // A pane of glass standing, one cube thick: one skin, no layers,
        // though the view is from above.
        let mut v: Vec<Instance> = (0..3).flat_map(|z| (0..3).map(move |x| cube(x as f32, 0.0, z as f32))).collect();
        join_see_through(&mut v, up);
        assert_eq!(v[4].fx[2], 0b11_0011, "a standing pane's middle: every face shared goes");
        // A pane two thick, six wide and high: layers across it, the far
        // one's near side stays.
        let mut v: Vec<Instance> = (0..2).flat_map(|y| (0..6).flat_map(move |z| (0..6).map(move |x| cube(x as f32, y as f32, z as f32)))).collect();
        join_see_through(&mut v, up);
        assert_eq!((v[7].fx[2], v[36 + 7].fx[2]), (0b11_0111, 0b11_0011), "two thick: the near pane loses its far side, the far one keeps its near side");
        // A block as wide as it is deep and high: across the view. From
        // above the tops between its layers stay, from the south the near sides.
        let block = || -> Vec<Instance> { (0..8).map(|n| cube((n & 1) as f32, (n >> 1 & 1) as f32, (n >> 2) as f32)).collect() };
        let mut v = block();
        join_see_through(&mut v, up);
        assert_eq!((v[0].fx[2], v[4].fx[2]), (0b00_0101, 0b10_0101), "a block from above");
        let mut v = block();
        join_see_through(&mut v, glam::Vec3::new(0.0, -1.0, 0.2));
        assert_eq!((v[0].fx[2], v[2].fx[2]), (0b01_0101, 0b01_0001), "a block from the south");
        // A sheet of glass one cube thick, standing or lying flat, seen
        // from any pitch: no face between its cubes is left, only its two
        // sides toward and away from the eye (the editor's pieces once
        // showed every row's top as a line, not joined at all).
        for pitch in [5.0f32, 25.0, 40.0, 60.0, 85.0] {
            let (s, c) = pitch.to_radians().sin_cos();
            let toward = glam::Vec3::new(0.1, -c, s);
            let standing: Vec<Instance> = (0..5).flat_map(|z| (0..6).map(move |x| cube(x as f32, 0.0, z as f32))).collect();
            let lying: Vec<Instance> = (0..5).flat_map(|y| (0..6).map(move |x| cube(x as f32, y as f32, 0.0))).collect();
            for (mut v, sides, name) in [(standing, 0b00_1100u8, "standing"), (lying, 0b11_0000, "lying")] {
                join_see_through(&mut v, toward);
                for i in &v {
                    let at = glam::Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
                    let shared = crate::mesh::DIRS.iter().enumerate().filter(|(_, d)| v.iter().any(|o| glam::Vec3::from_slice(&o.pos_size[..3]).floor().as_ivec3() == at + **d)).fold(0u8, |b, (k, _)| b | 1 << k);
                    assert_eq!(i.fx[2], shared, "a {name} sheet at {pitch} degrees, cube {at}: every face it shares goes");
                    assert_eq!(i.fx[2] & sides, 0, "a {name} sheet at {pitch} degrees, cube {at}: its two sides stay");
                }
            }
        }
        let mut v: Vec<Instance> = (0..3).flat_map(|y| (0..3).map(move |x| cube(x as f32, y as f32, 0.0))).collect();
        join_see_through(&mut v, glam::Vec3::Z);
        for (n, i) in v.iter().enumerate() {
            let (x, y) = (n % 3, n / 3);
            let side = (x < 2) as u8 | ((x > 0) as u8) << 1 | ((y < 2) as u8) << 2 | ((y > 0) as u8) << 3;
            assert_eq!(i.fx[2], side, "cube {x},{y} of a patch: its inner sides only, tops and bottoms stay");
        }
    }

    /// A plate of the HUD two voxels deep, left of and below the UI's eye:
    /// its front layer stays whole; of its back layer only the cubes whose
    /// sides face the eye (the right column, the top row), the rest left
    /// out (BunnyFarm's HUD went from 59,248 cubes to 29,241, the picture
    /// the same to the byte).
    #[test]
    fn cubes_nothing_of_which_shows_are_left_out() {
        let cube = |x: i32, y: i32, z: i32| crate::render::Instance { pos_size: [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5, 1.0], color: [9, 9, 9, 255], fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] };
        let mut plate: Vec<_> = (0..2).flat_map(|z| (0..4).flat_map(move |y| (0..6).map(move |x| cube(x, y, z)))).collect();
        let glass = crate::render::Instance { color: [9, 9, 9, 100], fx: [0, 128, 0, 0], ..cube(2, 2, -1) };
        plate.push(glass);
        crate::render::hide_inner_faces(&mut plate);
        crate::render::leave_out_unseen(&mut plate, glam::Vec3::new(50.0, 50.0, 200.0));
        let at = |i: &crate::render::Instance| (i.pos_size[0] as i32, i.pos_size[1] as i32, i.pos_size[2] as i32);
        let back: Vec<(i32, i32, i32)> = plate.iter().filter(|i| i.pos_size[2] < 1.0 && i.fx[1] == 255).map(at).collect();
        assert_eq!(plate.iter().filter(|i| i.pos_size[2] > 1.0).count(), 24, "the front layer whole");
        assert_eq!(back.len(), 9, "{back:?}");
        assert!(back.iter().all(|&(x, y, _)| x == 5 || y == 3), "{back:?}");
        assert!(plate.iter().any(|i| i.fx[1] != 255), "what is see-through stays");
        let order: Vec<_> = plate.iter().filter(|i| i.fx[1] == 255).map(at).map(|(x, y, z)| (z, y, x)).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "the cubes keep their order: {order:?}");
    }

    /// A cube lying against a filled cell (a figure's part against the
    /// part shaped) leaves out the face between them, and only that.
    #[test]
    fn a_cube_hides_the_face_against_what_is_filled() {
        let cube = |p: glam::Vec3| crate::render::Instance { pos_size: [p.x + 0.5, p.y + 0.5, p.z + 0.5, 1.0], color: [9, 9, 9, 255], fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] };
        let mut cubes = vec![cube(glam::Vec3::ZERO)];
        crate::render::hide_faces_against(&mut cubes, |c| c == glam::IVec3::X);
        let bit = crate::mesh::DIRS.iter().position(|d| *d == glam::IVec3::X).unwrap();
        assert_eq!(cubes[0].fx[2], 1 << bit);
    }
}
