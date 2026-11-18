//! The moving things that stop the lamps whose shadows are "all"
//! (`LampShadows::All`): every picture, the cells the models gathered for
//! them fill (in view or not, within such a lamp's reach; not the level's
//! pieces: they are the level's volume, `volume`) and the opaque cubes, as
//! a bit a voxel with coarser copies, as the level's volume is laid out,
//! in a box round them alone; and a box round each thing (`boxes`), so a
//! way of light that meets none of them walks nothing. The lamps' pass and
//! the fog's beams walk it after the level's volume, for those lamps only,
//! and ask it afresh every picture (the faces' memory keeps only what the
//! level's walls do: a moving thing changes its answers every picture).
//!
//! A cell is filled when its middle lies inside a voxel (`cells`): on the
//! grid the voxel's own cell, turned freely the cells whose middles the
//! turned cube holds. Nothing grows, so a shadow is the shape of what casts
//! it, and a thing moving steadily changes a cell once as it passes; a
//! turned surface a voxel thick leaves cells touching at least at their
//! edges, which the walks close (`lamps.wgsl`). A turned voxel can hold no
//! cell's middle at all (an eighth round, its middle at a cell's corner):
//! where nothing else of its thing lies round it (a cube alone, a rod a
//! voxel thin with a gap) it fills its middle's cell (`fill`), so the
//! smallest thing still casts; a voxel of a body is left to the body's
//! cells round it, which grows nothing. A face of a moving thing
//! looks past what moves in the cells right in front of it
//! (`lamps.wgsl`'s RIM), so a turned figure does not shade itself into
//! specks.
//!
//! Only the picture: nothing here is the game's.

use glam::{IVec3, Quat, Vec3};

use super::volume::{LEVELS, Reach, coarsen};
use super::{FX_GLOW, FX_OPACITY, Instance, Light, ModelBank, ModelDraw, off_grid, quarter, turned_centre};

/// The box is whole blocks of this many voxels each way (its coarsest
/// copy's texel).
const BLOCK_VOXELS: i32 = 2 << (LEVELS - 1);

/// The most bytes the moving things' volume may take (every copy): moving
/// things further apart stop no lamp, said once.
pub const MOST_BYTES: u64 = 32 << 20;

/// The most boxes the walks test (`boxes`): more things are boxed by the
/// blocks they lie in, and past that the walks take the whole volume.
pub const MOST_BOXES: usize = 256;

/// The cubes' boxes are blocks of this many voxels each way.
const CUBE_BLOCK: i32 = 8;

/// The moving things' volume of a picture.
pub struct Movers {
    pub reach: Reach,
    /// Every copy's bytes, x fastest, then y, then z.
    pub copies: Vec<Vec<u8>>,
    /// The boxes round what fills it, low corner and high (past the last
    /// cell), from the volume's low corner; empty: the walks take all of it.
    pub boxes: Vec<[i32; 8]>,
    /// What it holds, to send it to the GPU only when it changed.
    pub key: u64,
}

/// The cells a voxel whose middle is `middle`, turned `rot`, fills: those
/// whose middles it holds. On the grid (`rot` none, `middle` a cell's
/// middle) its own cell. How many.
fn cells(middle: Vec3, rot: Option<Quat>, mut put: impl FnMut(IVec3)) -> usize {
    let Some(rot) = rot else {
        put(middle.floor().as_ivec3());
        return 1;
    };
    let mut n = 0;
    let back = rot.inverse();
    // A middle within a turned cube lies within half its diagonal of its
    // own middle.
    let (lo, hi) = ((middle - 0.5 - 0.867).ceil().as_ivec3(), (middle - 0.5 + 0.867).floor().as_ivec3());
    for z in lo.z..=hi.z {
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let c = IVec3::new(x, y, z);
                let q = back * (c.as_vec3() + 0.5 - middle);
                if q.abs().max_element() <= 0.5 {
                    put(c);
                    n += 1;
                }
            }
        }
    }
    n
}

/// The cells a thing fills: those its voxels' middles fill (`cells`, given
/// whole by `each`, which tells a voxel that held no cell's middle by its
/// middle), and of such a voxel its middle's cell where none of the
/// others lies on it or round it (faces, edges or corners), so a cube
/// alone or a thin rod's lost voxel still stops a lamp; a body's surface
/// grows nothing, its cells round such a voxel already there.
fn fill(each: impl FnOnce(&mut dyn FnMut(IVec3), &mut dyn FnMut(Vec3))) -> Vec<IVec3> {
    let (mut filled, mut lost) = (Vec::new(), Vec::new());
    each(&mut |c| filled.push(c), &mut |m| lost.push(m.floor().as_ivec3()));
    if lost.is_empty() {
        return filled;
    }
    let set: crate::world::CellSet = filled.iter().copied().collect();
    let mut added = crate::world::CellSet::default();
    for c in lost {
        let alone = (-1..=1).all(|z| (-1..=1).all(|y| (-1..=1).all(|x| !set.contains(&(c + IVec3::new(x, y, z))))));
        if alone && added.insert(c) {
            filled.push(c);
        }
    }
    filled
}

/// The cells a draw's opaque voxels fill (`cells`), placed as the shader
/// places them (`vs_model`); `lost` the middle of a voxel that filled none
/// (`fill`). A voxel that glows is left out: a lamp's bulb does not stop
/// its own light.
fn draw_cells(bank: &ModelBank, draw: &ModelDraw, put: &mut dyn FnMut(IVec3), lost: &mut dyn FnMut(Vec3)) {
    let free = off_grid(draw.rot, draw.shift).then_some(draw.rot);
    let Some(voxels) = bank.voxels.get(draw.voxels.start as usize..draw.voxels.end as usize) else { return };
    for v in voxels {
        if v.fx[FX_GLOW] > 0 {
            continue;
        }
        let [lx, ly, lz, _] = v.pos_size;
        let (x, y) = quarter(lx as i32, ly as i32, draw.turn);
        let local = Vec3::new(x as f32, y as f32, lz);
        match free {
            Some(rot) => {
                let middle = turned_centre(draw.origin, local, draw.shift, rot);
                if cells(middle, Some(rot), &mut *put) == 0 {
                    lost(middle);
                }
            }
            None => put((draw.origin + local + 0.5).floor().as_ivec3()),
        }
    }
}

/// The cells a cube fills (a frame:cube, a spark, debris), as a model's
/// voxel (`lost` as `draw_cells`'). Glowing ones and see-through ones stop
/// nothing.
fn cube_cells(c: &Instance, put: &mut dyn FnMut(IVec3), lost: &mut dyn FnMut(Vec3)) {
    if c.fx[FX_GLOW] > 0 || c.fx[FX_OPACITY] < 255 {
        return;
    }
    let [x, y, z, _] = c.pos_size;
    let rot = (c.turn != [0; 4]).then(|| Quat::from_array(c.turn.map(|v| v as f32 / 127.0)).normalize());
    if cells(Vec3::new(x, y, z), rot, put) == 0 {
        lost(Vec3::new(x, y, z));
    }
}

/// A box growing round the cells put into it.
#[derive(Clone, Copy)]
struct Bounds {
    lo: IVec3,
    hi: IVec3,
}

impl Bounds {
    const NONE: Self = Self { lo: IVec3::MAX, hi: IVec3::MIN };

    fn put(&mut self, c: IVec3) {
        (self.lo, self.hi) = (self.lo.min(c), self.hi.max(c));
    }

    fn join(self, o: Self) -> Self {
        Self { lo: self.lo.min(o.lo), hi: self.hi.max(o.hi) }
    }

    fn is_empty(&self) -> bool {
        self.lo.x > self.hi.x
    }
}

/// `boxes` joined by the blocks of `block` voxels their low corners lie
/// in, in their order, until at most `MOST_BOXES` are left; none (the
/// whole volume) when even blocks as big as `most` do not bring them down.
fn fewer(boxes: Vec<Bounds>, most: i32) -> Vec<Bounds> {
    let mut boxes = boxes;
    let mut block = CUBE_BLOCK * 2;
    while boxes.len() > MOST_BOXES {
        if block > most {
            return Vec::new();
        }
        let mut joined: Vec<(IVec3, Bounds)> = Vec::new();
        for b in boxes {
            let key = b.lo.div_euclid(IVec3::splat(block));
            match joined.iter_mut().find(|(k, _)| *k == key) {
                Some((_, j)) => *j = j.join(b),
                None => joined.push((key, b)),
            }
        }
        boxes = joined.into_iter().map(|(_, b)| b).collect();
        block *= 2;
    }
    boxes
}

impl Movers {
    /// The volume of the things `things` (each one's draws from `bank`)
    /// and the cubes `cubes`: None when they fill no cell; a mistake, in
    /// words, when they lie too far apart for `MOST_BYTES` or the GPU's 3D
    /// textures (`most` texels each way).
    pub fn of(bank: &ModelBank, things: &[Vec<ModelDraw>], cubes: &[Instance], most: u32) -> Option<Result<Self, String>> {
        let mut filled = Vec::new();
        let mut boxes = Vec::new();
        for draws in things {
            let mut b = Bounds::NONE;
            for c in fill(|put, lost| {
                for d in draws {
                    draw_cells(bank, d, put, lost);
                }
            }) {
                b.put(c);
                filled.push(c);
            }
            if !b.is_empty() {
                boxes.push(b);
            }
        }
        // The cubes boxed by the blocks they lie in.
        let mut blocks: Vec<(IVec3, Bounds)> = Vec::new();
        for c in fill(|put, lost| {
            for c in cubes {
                cube_cells(c, put, lost);
            }
        }) {
            filled.push(c);
            let key = c.div_euclid(IVec3::splat(CUBE_BLOCK));
            match blocks.last_mut().filter(|(k, _)| *k == key) {
                Some((_, b)) => b.put(c),
                None => match blocks.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, b)) => b.put(c),
                    None => {
                        let mut b = Bounds::NONE;
                        b.put(c);
                        blocks.push((key, b));
                    }
                },
            }
        }
        boxes.extend(blocks.into_iter().map(|(_, b)| b));
        if filled.is_empty() {
            return None;
        }
        let all = boxes.iter().fold(Bounds::NONE, |a, b| a.join(*b));
        let origin = all.lo.div_euclid(IVec3::splat(BLOCK_VOXELS)) * BLOCK_VOXELS;
        let end = (all.hi + 1 + BLOCK_VOXELS - 1).div_euclid(IVec3::splat(BLOCK_VOXELS)) * BLOCK_VOXELS;
        let reach = Reach { origin, texels: (end - origin) / 2 };
        let (t, v) = (reach.texels, reach.texels * 2);
        if reach.bytes() > MOST_BYTES || t.max_element() as u32 > most {
            return Some(Err(format!("moving things stop no lamp: they lie {} by {} by {} voxels apart, past what their volume holds ({} MB, {} voxels each way)", v.x, v.y, v.z, MOST_BYTES >> 20, most * 2)));
        }
        let mut copies = vec![vec![0u8; (t.x * t.y * t.z) as usize]];
        for c in filled {
            let tc: IVec3 = (c - origin) >> 1;
            copies[0][(tc.x + t.x * (tc.y + t.y * tc.z)) as usize] |= 1 << ((c.x & 1) | (c.y & 1) << 1 | (c.z & 1) << 2);
        }
        for m in 1..LEVELS as i32 {
            copies.push(vec![0; ((t.x >> m) * (t.y >> m) * (t.z >> m)) as usize]);
        }
        coarsen(&mut copies, t, IVec3::ZERO, t);
        let boxes: Vec<[i32; 8]> = fewer(boxes, v.max_element()).iter().map(|b| {
            let (lo, hi) = (b.lo - origin, b.hi + 1 - origin);
            [lo.x, lo.y, lo.z, 0, hi.x, hi.y, hi.z, 0]
        }).collect();
        let mut h = crate::hash::Stable::default();
        h.u64(origin.x as u64).u64(origin.y as u64).u64(origin.z as u64).u64(t.x as u64).u64(t.y as u64).u64(t.z as u64).bytes(&copies[0]);
        h.bytes(bytemuck::cast_slice(&boxes));
        Some(Ok(Self { reach, copies, boxes, key: h.finish() }))
    }
}

/// The most entries the lights' lists of boxes take (`Movers::lists`): a
/// light past them walks the whole volume.
pub const MOST_LISTED: usize = 1 << 16;

/// A light's word for the boxes it walks (`lists`) meaning all of the
/// volume.
const EVERY_BOX: u32 = 255;

impl Movers {
    /// Each of `lights`' boxes, those it can light (within its reach and,
    /// a spot, its cone), one light's after another in the list given
    /// back, and each light's word for the shaders (`LightData`'s `edge.y`
    /// of a lamp whose shadows are "all"): 2, plus where its boxes begin
    /// times 256, plus how many (255: every one, the whole volume walked);
    /// None for the others. A box a light cannot reach casts nothing for
    /// it, so the walks test a few boxes a light, not all of them.
    pub fn lists(&self, lights: &[Light]) -> (Vec<u32>, Vec<Option<f32>>) {
        let mut list = Vec::new();
        let corner = self.reach.origin.as_vec3();
        let words = lights
            .iter()
            .map(|l| {
                if !l.stopped_by_movers() {
                    return None;
                }
                let start = list.len();
                if !self.boxes.is_empty() {
                    for (k, b) in self.boxes.iter().enumerate() {
                        let (lo, hi) = (corner + Vec3::new(b[0] as f32, b[1] as f32, b[2] as f32), corner + Vec3::new(b[4] as f32, b[5] as f32, b[6] as f32));
                        if lit(l, lo, hi) {
                            list.push(k as u32);
                        }
                    }
                }
                let count = list.len() - start;
                let code = if self.boxes.is_empty() || count >= EVERY_BOX as usize || list.len() > MOST_LISTED {
                    list.truncate(start);
                    EVERY_BOX
                } else {
                    (start as u32) << 8 | count as u32
                };
                Some(2.0 + code as f32)
            })
            .collect();
        (list, words)
    }
}

/// Whether the light `l` can reach into the box from `lo` to `hi`: within
/// its radius and, a spot, its cone (the box taken as the ball round it).
pub(super) fn lit(l: &Light, lo: Vec3, hi: Vec3) -> bool {
    let (middle, r) = ((lo + hi) * 0.5, (hi - lo).length() * 0.5);
    let v = middle - l.pos;
    let d = v.length();
    if d <= r {
        return true;
    }
    if d - r > l.radius {
        return false;
    }
    l.spot.is_none_or(|s| {
        let off = (v.dot(s.way) / d).clamp(-1.0, 1.0).acos();
        off - (r / d).min(1.0).asin() <= s.outer.clamp(-1.0, 1.0).acos()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(at: Vec3, turn: [i8; 4], opacity: u8, glow: u8) -> Instance {
        let mut fx = [0u8; 4];
        (fx[FX_OPACITY], fx[FX_GLOW]) = (opacity, glow);
        Instance { pos_size: at.extend(1.0).to_array(), color: [255; 4], fx, shade: [0; 4], turn }
    }

    fn filled(m: &Movers, c: IVec3) -> bool {
        let tc: IVec3 = (c - m.reach.origin) >> 1;
        let t = m.reach.texels;
        m.copies[0][(tc.x + t.x * (tc.y + t.y * tc.z)) as usize] >> ((c.x & 1) | (c.y & 1) << 1 | (c.z & 1) << 2) & 1 == 1
    }

    /// A cube a voxel big fills its cell; turned, the cells whose middles
    /// it holds; glowing or see-through, none. The box is whole blocks
    /// round what fills it, its coarse copies holding what lies below.
    #[test]
    fn the_moving_things_fill_their_cells() {
        let bank = ModelBank::default();
        let m = Movers::of(&bank, &[], &[cube(Vec3::new(70.5, 3.5, 9.5), [0; 4], 255, 0), cube(Vec3::new(5.5, 5.5, 5.5), [0; 4], 100, 0), cube(Vec3::new(9.5, 9.5, 9.5), [0; 4], 255, 9)], 2048).unwrap().unwrap();
        assert_eq!((m.reach.origin, m.reach.texels * 2), (IVec3::new(64, 0, 0), IVec3::splat(64)), "the one opaque cube's block");
        assert!(filled(&m, IVec3::new(70, 3, 9)) && !filled(&m, IVec3::new(71, 3, 9)));
        assert_eq!(m.boxes, vec![[6, 3, 9, 0, 7, 4, 10, 0]], "its box, from the volume's corner");
        assert_eq!(m.copies.len(), LEVELS as usize);
        assert!(m.copies[LEVELS as usize - 1].iter().any(|b| *b != 0), "the coarsest copy holds it");
        // Turned an eighth round its middle: at a cell's middle only its
        // own cell; 0.4 off it, the middles of that cell and the next (its
        // corner reaches 0.71 along the axes); none grown round it.
        let count = |at: Vec3, turn| {
            let m = Movers::of(&bank, &[], &[cube(at, turn, 255, 0)], 2048).unwrap().unwrap();
            m.copies[0].iter().map(|b| b.count_ones()).sum::<u32>()
        };
        let eighth = crate::render::snorm_turn(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4));
        assert_eq!(count(Vec3::new(70.5, 3.5, 9.5), eighth), 1);
        assert_eq!(count(Vec3::new(70.9, 3.5, 9.5), eighth), 2);
        assert_eq!(count(Vec3::new(70.9, 3.5, 9.5), [0; 4]), 1, "on the grid: its middle's cell");
        assert!(Movers::of(&bank, &[], &[cube(Vec3::ZERO, [0; 4], 0, 0)], 2048).is_none(), "nothing opaque: no volume");
        let far = Movers::of(&bank, &[], &[cube(Vec3::new(0.5, 0.5, 0.5), [0; 4], 255, 0), cube(Vec3::new(4000.5, 4000.5, 100.5), [0; 4], 255, 0)], 2048).unwrap();
        assert!(far.is_err_and(|e| e.contains("moving things stop no lamp")));
    }

    /// A cube turned so that it holds no cell's middle (an eighth round,
    /// its middle at a cell's corner) still fills its middle's cell: alone,
    /// or in a rod of such cubes along x a voxel apart; a turned body's
    /// cells are those its voxels' middles fill, nothing grown round them
    /// (its lost voxels lie by its cells).
    #[test]
    fn the_smallest_turned_thing_still_fills_a_cell() {
        let bank = ModelBank::default();
        let eighth = crate::render::snorm_turn(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4));
        let mut none = 0;
        cells(Vec3::new(71.0, 4.0, 9.5), Some(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)), |_| none += 1);
        assert_eq!(none, 0, "no cell's middle in it");
        let m = Movers::of(&bank, &[], &[cube(Vec3::new(71.0, 4.0, 9.5), eighth, 255, 0)], 2048).expect("it fills a cell").unwrap();
        assert!(filled(&m, IVec3::new(71, 4, 9)), "its middle's cell");
        assert_eq!(m.copies[0].iter().map(|b| b.count_ones()).sum::<u32>(), 1);
        let rod: Vec<Instance> = (0..6).map(|i| cube(Vec3::new(71.0 + 2.0 * i as f32, 4.0, 9.5), eighth, 255, 0)).collect();
        let m = Movers::of(&bank, &[], &rod, 2048).unwrap().unwrap();
        assert!((0..6).all(|i| filled(&m, IVec3::new(71 + 2 * i, 4, 9))), "every cube of the rod");
        // A body 6 by 6 by 6 turned an eighth: the cells of its voxels'
        // middles, no more.
        let rot = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
        let body: Vec<Instance> = (0..216).map(|i| {
            let at = Vec3::new(40.0, 40.0, 10.0) + rot * Vec3::new((i % 6) as f32, (i / 6 % 6) as f32, (i / 36) as f32);
            cube(at, crate::render::snorm_turn(rot), 255, 0)
        }).collect();
        let mut own = crate::world::CellSet::default();
        for c in &body {
            cells(Vec3::from_slice(&c.pos_size[..3]), Some(Quat::from_array(c.turn.map(|v| v as f32 / 127.0)).normalize()), |c| {
                own.insert(c);
            });
        }
        let m = Movers::of(&bank, &[], &body, 2048).unwrap().unwrap();
        assert_eq!(m.copies[0].iter().map(|b| b.count_ones()).sum::<u32>() as usize, own.len(), "nothing grown round the body");
    }

    /// A lamp whose shadows are "all" lists the boxes it can light: a spot
    /// straight down those under it, not one beside its cone or past its
    /// radius; a round lamp every box in reach; a lamp of another word
    /// none (its word unchanged).
    #[test]
    fn a_lamp_lists_the_boxes_it_can_light() {
        use crate::render::{LampShadows, LightShape};
        let bank = ModelBank::default();
        let m = Movers::of(&bank, &[], &[cube(Vec3::new(10.5, 10.5, 1.5), [0; 4], 255, 0), cube(Vec3::new(40.5, 10.5, 1.5), [0; 4], 255, 0), cube(Vec3::new(200.5, 10.5, 1.5), [0; 4], 255, 0)], 2048).unwrap().unwrap();
        assert_eq!(m.boxes.len(), 3);
        let spot = Light::lamp(Vec3::new(10.5, 10.5, 20.0), 60.0, [1.0; 3], &LightShape { cone: Some(30.0), pitch: -90.0, ..Default::default() }, |w| w).stated(LampShadows::All);
        let round = Light { spot: None, ..spot };
        let other = spot.stated(LampShadows::Static);
        let (list, words) = m.lists(&[spot, round, Light { shadows: Some(LampShadows::Static), ..other }]);
        assert_eq!(list, vec![0, 0, 1], "the spot the box under it; the round lamp both in reach, not the one 190 voxels off");
        assert_eq!(words, vec![Some(2.0 + 1.0), Some(2.0 + (1 << 8 | 2) as f32), None]);
    }

    /// A slab a voxel thick, turned any way, leaves no way through it but
    /// along the edges and corners its cells touch at (which the walks
    /// close): every line across it meets a filled cell or such a touch.
    /// Checked here as every cell of the slab's middle plane having a
    /// filled cell within one step, faces, edges or corners.
    #[test]
    fn a_turned_slab_a_voxel_thick_leaves_no_gap() {
        for (yaw, pitch) in [(0.3f32, 0.0f32), (0.785, 0.0), (0.6, 0.5), (0.785, 0.615), (1.2, 0.2)] {
            let rot = Quat::from_rotation_z(yaw) * Quat::from_rotation_x(pitch);
            let mut set = crate::world::CellSet::default();
            for v in -8..=8 {
                for u in -8..=8 {
                    cells(Vec3::new(40.0, 40.0, 40.0) + rot * Vec3::new(u as f32, v as f32, 0.0), Some(rot), |c| {
                        set.insert(c);
                    });
                }
            }
            for v in -5..=5 {
                for u in -5..=5 {
                    let p = (Vec3::new(40.0, 40.0, 40.0) + rot * Vec3::new(u as f32 + 0.5, v as f32 + 0.5, 0.0)).floor().as_ivec3();
                    let near = (-1..=1).any(|z| (-1..=1).any(|y| (-1..=1).any(|x| set.contains(&(p + IVec3::new(x, y, z))))));
                    assert!(near, "yaw {yaw} pitch {pitch}: nothing filled round {p}");
                }
            }
        }
    }
}
