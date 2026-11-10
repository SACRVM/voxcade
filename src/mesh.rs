//! Chunk meshing. With a fixed camera and a fixed sun only some face
//! directions can ever be seen or cast a shadow, so only those are built.

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};

use crate::world::{CHUNK, World, local_index};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [i8; 4],
    /// RGB from the palette, A = ambient occlusion (0 enclosed .. 255 open).
    pub color: [u8; 4],
}

/// Face directions; index / 2 is the axis, even indices point positive.
pub const DIRS: [IVec3; 6] =
    [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

pub type FaceMask = [bool; 6];

/// Faces that point towards the camera along any of its view rays, or
/// towards the sun. The rest are never visible and never the first surface a
/// sunbeam hits.
pub fn face_mask(view_rays: &[Vec3], sun_dir: Vec3) -> FaceMask {
    let mut mask = [false; 6];
    for (i, d) in DIRS.iter().enumerate() {
        let n = d.as_vec3();
        mask[i] = view_rays.iter().any(|&r| n.dot(-r) > 1e-3) || n.dot(-sun_dir) > 1e-3;
    }
    mask
}

/// The faces some view ray can hit, and every face a sun above the horizon
/// can light (a game that moves its sun).
pub fn face_mask_any_sun(view_rays: &[Vec3]) -> FaceMask {
    let mut mask = face_mask(view_rays, Vec3::NEG_Z);
    for (i, d) in DIRS.iter().enumerate() {
        mask[i] |= d.z >= 0;
    }
    mask
}

const PAD: i32 = CHUNK + 2;

fn pad_index(l: IVec3) -> usize {
    ((l.x + 1) + (l.y + 1) * PAD + (l.z + 1) * PAD * PAD) as usize
}

pub fn mesh_chunk(
    world: &World,
    cc: IVec3,
    mask: &FaceMask,
    verts: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
) {
    verts.clear();
    indices.clear();
    let Some(chunk) = world.chunk(cc) else { return };
    let base = cc * CHUNK;

    // The chunk plus a one-voxel border, so every lookup below is an array access.
    let mut pad = vec![0u8; (PAD * PAD * PAD) as usize];
    for z in -1..=CHUNK {
        for y in -1..=CHUNK {
            for x in -1..=CHUNK {
                let l = IVec3::new(x, y, z);
                let inside = l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK)).all();
                pad[pad_index(l)] =
                    if inside { chunk.voxels[local_index(l)] } else { world.get(base + l) };
            }
        }
    }
    // Every visible face goes into its slice (by direction and depth), as
    // its colour (high byte) and its corners' openness (two bits each),
    // in one pass over the voxels. Then slice by slice faces of one colour
    // with no corner shaded (open all round) merge into rectangles, the
    // others stay one a voxel. A merged face looks the same: colour and
    // occlusion are even across it, and the shader finds each pixel's
    // voxel from where it is.
    let stride = [1isize, PAD as isize, (PAD * PAD) as isize];
    let offset = |v: IVec3| v.x as isize * stride[0] + v.y as isize * stride[1] + v.z as isize * stride[2];
    let shown: Vec<(usize, isize, [[isize; 3]; 4])> = (0..6)
        .filter(|&d| mask[d])
        .map(|d| {
            let (eu, ev, corners) = face_frame(d);
            let n = DIRS[d];
            let around = corners.map(|(cu, cv)| {
                let su = if cu == 0 { -eu } else { eu };
                let sv = if cv == 0 { -ev } else { ev };
                [offset(n + su), offset(n + sv), offset(n + su + sv)]
            });
            (d, offset(n), around)
        })
        .collect();
    SLICES.with(|slices| {
        let mut slices = slices.borrow_mut();
        let Slices { faces, lists } = &mut *slices;
        for z in 0..CHUNK {
            for y in 0..CHUNK {
                for x in 0..CHUNK {
                    let i = pad_index(IVec3::new(x, y, z)) as isize;
                    let c = pad[i as usize];
                    if c == 0 {
                        continue;
                    }
                    let l = [x as usize, y as usize, z as usize];
                    for &(d, ahead, around) in &shown {
                        if pad[(i + ahead) as usize] != 0 {
                            continue;
                        }
                        let mut ao = 0u16;
                        for (k, [side_u, side_v, corner]) in around.into_iter().enumerate() {
                            let at = |o: isize| pad[(i + o) as usize] != 0;
                            let (side_u, side_v, corner) = (at(side_u), at(side_v), at(corner));
                            let level = if side_u && side_v { 0 } else { 3 - (side_u as u16 + side_v as u16 + corner as u16) };
                            ao |= level << (2 * k);
                        }
                        let axis = d / 2;
                        let (s, u, v) = (l[axis], l[(axis + 1) % 3], l[(axis + 2) % 3]);
                        faces[((d * SIDE + s) * SIDE + v) * SIDE + u] = (c as u16) << 8 | ao;
                        lists[d * SIDE + s].push((v * SIDE + u) as u16);
                    }
                }
            }
        }
        for &(d, ..) in &shown {
            let axis = d / 2;
            for s in 0..SIDE {
                let list = &mut lists[d * SIDE + s];
                if list.is_empty() {
                    continue;
                }
                // Row by row: the voxel order lists the y slices' faces column by column.
                if axis == 1 {
                    list.sort_unstable();
                }
                let plane = &mut faces[(d * SIDE + s) * SIDE * SIDE..][..SIDE * SIDE];
                for &k in list.iter() {
                    let i = k as usize;
                    let face = plane[i];
                    if face == 0 {
                        continue; // merged into one before
                    }
                    let (u, v) = (i % SIDE, i / SIDE);
                    let (mut w, mut h) = (1, 1);
                    if face & 0xFF == OPEN {
                        while u + w < SIDE && plane[i + w] == face {
                            w += 1;
                        }
                        'rows: while v + h < SIDE {
                            for k in 0..w {
                                if plane[u + k + (v + h) * SIDE] != face {
                                    break 'rows;
                                }
                            }
                            h += 1;
                        }
                    }
                    for dv in 0..h {
                        plane[u + (v + dv) * SIDE..u + w + (v + dv) * SIDE].fill(0);
                    }
                    let mut cell = IVec3::ZERO;
                    cell[axis] = s as i32;
                    cell[(axis + 1) % 3] = u as i32;
                    cell[(axis + 2) % 3] = v as i32;
                    let ao = [0, 1, 2, 3].map(|k| (face >> (2 * k)) as u32 & 3);
                    push_quad(base + cell, d, w as i32, h as i32, world.palette[(face >> 8) as usize], ao, verts, indices);
                }
                list.clear();
            }
        }
    });
}

const SIDE: usize = CHUNK as usize;

/// A chunk's visible faces by direction and slice while it is meshed: each
/// meshing thread keeps one, and leaves it empty for the next chunk.
struct Slices {
    faces: Vec<u16>,
    lists: Vec<Vec<u16>>,
}

thread_local! {
    static SLICES: std::cell::RefCell<Slices> =
        std::cell::RefCell::new(Slices { faces: vec![0; 6 * SIDE * SIDE * SIDE], lists: vec![Vec::new(); 6 * SIDE] });
}

/// A face's corners all open (two bits each, `mesh_chunk`): nothing beside
/// it shades them.
const OPEN: u16 = 0xFF;

/// How far a merged face reaches over its edges, in voxels: a hair, less
/// than a hundredth of a pixel from any distance the camera has.
const MERGED_REACH: f32 = 1.0 / 512.0;

/// One unit cube centred on the origin, for instanced drawing.
pub fn unit_cube() -> (Vec<Vertex>, Vec<u32>) {
    let (mut verts, mut indices) = (Vec::new(), Vec::new());
    for d in 0..6 {
        emit_face(IVec3::ZERO, d, [255; 4], |_| false, &mut verts, &mut indices);
    }
    for v in &mut verts {
        for c in &mut v.pos {
            *c -= 0.5;
        }
    }
    (verts, indices)
}

/// Emits the quad of face `d` of the voxel at world position `p`.
/// `solid(offset)` answers for voxels relative to `p` (for ambient occlusion).
fn emit_face(
    p: IVec3,
    d: usize,
    rgb: [u8; 4],
    solid: impl Fn(IVec3) -> bool,
    verts: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
) {
    push_quad(p, d, 1, 1, rgb, occlusion(d, solid), verts, indices);
}

/// The corners of face `d`, eu and ev (`corners`), and which way they wind.
fn face_frame(d: usize) -> (IVec3, IVec3, [(i32, i32); 4]) {
    let axis = d / 2;
    let eu = DIRS[((axis + 1) % 3) * 2];
    let ev = DIRS[((axis + 2) % 3) * 2];
    // eu × ev = +axis, so this order winds counter-clockwise seen from outside.
    let corners = if d.is_multiple_of(2) { [(0, 0), (1, 0), (1, 1), (0, 1)] } else { [(0, 0), (0, 1), (1, 1), (1, 0)] };
    (eu, ev, corners)
}

/// How open each corner of face `d` is, 0 (enclosed) to 3 (open):
/// `solid(offset)` answers for voxels relative to its own.
fn occlusion(d: usize, solid: impl Fn(IVec3) -> bool) -> [u32; 4] {
    let n = DIRS[d];
    let (eu, ev, corners) = face_frame(d);
    let mut ao = [0u32; 4];
    for (k, &(cu, cv)) in corners.iter().enumerate() {
        let su = if cu == 0 { -eu } else { eu };
        let sv = if cv == 0 { -ev } else { ev };
        let side_u = solid(n + su);
        let side_v = solid(n + sv);
        let corner = solid(n + su + sv);
        ao[k] = if side_u && side_v { 0 } else { 3 - (side_u as u32 + side_v as u32 + corner as u32) };
    }
    ao
}

/// The quad of face `d` of the voxel at `p`, stretched over `w` by `h`
/// voxels along its plane (merged faces), its corners shaded by `ao`.
#[allow(clippy::too_many_arguments)]
fn push_quad(p: IVec3, d: usize, w: i32, h: i32, rgb: [u8; 4], ao: [u32; 4], verts: &mut Vec<Vertex>, indices: &mut Vec<u32>) {
    let n = DIRS[d];
    let (eu, ev, corners) = face_frame(d);
    let plane = if d.is_multiple_of(2) { p + n } else { p };
    let normal = [(n.x * 127) as i8, (n.y * 127) as i8, (n.z * 127) as i8, 0];
    // A merged face meets the faces beside it where their corners lie
    // along its edges (T-junctions), and the GPU may leave a pixel's crack
    // there: it reaches a hair over its edges to close them.
    let reach = if w > 1 || h > 1 { MERGED_REACH } else { 0.0 };
    let start = verts.len() as u32;
    for (k, &(cu, cv)) in corners.iter().enumerate() {
        let out = (eu * (2 * cu - 1)).as_vec3() * reach + (ev * (2 * cv - 1)).as_vec3() * reach;
        let pos = ((plane + eu * (cu * w) + ev * (cv * h)).as_vec3() + out).to_array();
        verts.push(Vertex { pos, normal, color: [rgb[0], rgb[1], rgb[2], (ao[k] * 85) as u8] });
    }
    // Split along the brighter diagonal so a dark corner fades out
    // instead of streaking across the face.
    if ao[0] + ao[2] > ao[1] + ao[3] {
        indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
    } else {
        indices.extend_from_slice(&[start + 1, start + 2, start + 3, start + 1, start + 3, start]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Faces of one colour open all round merge into rectangles; a face of
    /// another colour, or with a corner shaded, stays one a voxel. Every
    /// visible face is covered once, in its colour.
    #[test]
    fn open_faces_of_one_colour_merge() {
        let mut world = crate::sim::tests::flat_world(IVec3::new(32, 32, 8));
        world.palette[2] = [200, 40, 40, 255];
        world.set_raw(IVec3::new(5, 5, 0), 2);
        world.set_raw(IVec3::new(10, 10, 1), 1);
        let mut mask = [false; 6];
        mask[4] = true; // +z
        let (mut verts, mut indices) = (Vec::new(), Vec::new());
        mesh_chunk(&world, IVec3::ZERO, &mask, &mut verts, &mut indices);
        let mut covered = vec![0u32; 32 * 32 * 3];
        let quads = verts.chunks(4).count();
        for q in verts.chunks(4) {
            let lo = q.iter().fold(Vec3::splat(f32::MAX), |m, v| m.min(Vec3::from(v.pos))).round().as_ivec3();
            let hi = q.iter().fold(Vec3::splat(f32::MIN), |m, v| m.max(Vec3::from(v.pos))).round().as_ivec3();
            let red = q[0].color[..3] == [200, 40, 40];
            if hi.x - lo.x > 1 || hi.y - lo.y > 1 {
                assert!(q.iter().all(|v| v.color[3] == 255), "a merged face has a shaded corner");
            }
            for y in lo.y..hi.y {
                for x in lo.x..hi.x {
                    covered[((x + y * 32) * 3 + red as i32) as usize] += 1;
                    assert_eq!(lo.z, if (x, y) == (10, 10) { 2 } else { 1 }, "a face at the wrong height");
                }
            }
        }
        for y in 0..32 {
            for x in 0..32 {
                let i = (x + y * 32) * 3;
                let red = (x, y) == (5, 5);
                assert_eq!((covered[i], covered[i + 1]), if red { (0, 1) } else { (1, 0) }, "the face at {x}, {y}");
            }
        }
        assert!(quads < 40, "{quads} faces for a flat floor with two marks");
    }
}
