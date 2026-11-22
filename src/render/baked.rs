//! Models baked (`c.effects.bake_models`): each run of the model bank (a
//! model's frame, a figure's part) as the list of the faces of its voxels
//! that are ever drawn, made once and kept with the model (`Baked`). A
//! frame then draws six corners a face (`vs_face`) instead of a cube's 24
//! a voxel, from 12 bytes a face instead of 32 a voxel. The picture is the
//! same: each face is the cube's own, placed by the same sums. On the GPU
//! beside the bank's cubes (`OnGpu`): what is not baked (the logo's
//! letters, a run past what a face packs, a game with the setting off)
//! stays cubes.

use std::ops::Range;
use std::sync::{Arc, OnceLock};

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Quat, Vec3};

use super::{FX_COVERED, FX_GLOW, FX_METAL, FX_OPACITY, Instance, ModelBank, ModelDraw, quarter};
use crate::mesh::DIRS;

/// A face of a model's voxel, as the GPU reads it (`vs_face`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, PartialEq)]
pub struct Face {
    /// Its voxel's cell from the model's pivot cell: x and y in
    /// `CELL_BITS` each (two's complement), z in as many from 0; above
    /// them its own direction (`DIR_AT`, an index into `mesh::DIRS`),
    /// whether the voxel's shade direction lights it (`LIT_AT`) and whether
    /// it is its voxel's first face (`FIRST_AT`: the outline grows each
    /// voxel once).
    pub cell: u32,
    /// The voxel's colour (rgb) and how much it glows (a).
    pub color: [u8; 4],
    /// The direction the voxel is lit by (snorm, `vox::VoxModel::shade`),
    /// and its metal (`vox::metal::pack`).
    pub shade: [i8; 3],
    pub metal: u8,
}

/// How a face's `cell` is packed: a voxel's x, y and z in this many bits
/// each (a model of up to 512 by 512 by 512), then the face's direction,
/// lit, first.
pub const CELL_BITS: u32 = 9;
pub const DIR_AT: u32 = 3 * CELL_BITS;
pub const LIT_AT: u32 = DIR_AT + 3;
pub const FIRST_AT: u32 = LIT_AT + 1;

/// A run's faces, by their own direction, and the box its voxels fill.
#[derive(Debug)]
pub struct Faces {
    pub list: Vec<Face>,
    /// Where each own direction's faces begin in `list` (`mesh::DIRS`
    /// order), and where the last end.
    pub dirs: [u32; 7],
    /// In the run's own grid, from the pivot cell's minimum corner.
    pub lo: IVec3,
    pub hi: IVec3,
}

impl Faces {
    /// The faces of a run's voxels (as the bank holds them, each with the
    /// faces a neighbour covers): none for a run with a voxel a face cannot
    /// carry (past `CELL_BITS`, no whole cell, see-through or covered all
    /// round), and none where its faces would take more memory than its
    /// cubes (a thin run: a voxel alone has six faces, a post four), which
    /// stays cubes.
    pub fn of(voxels: &[Instance]) -> Option<Faces> {
        let half = 1 << (CELL_BITS - 1);
        let mut cells = Vec::with_capacity(voxels.len());
        for v in voxels {
            let [x, y, z, size] = v.pos_size;
            let c = IVec3::new(x as i32, y as i32, z as i32);
            let packs = c.as_vec3() == Vec3::new(x, y, z) && c.x >= -half && c.x < half && c.y >= -half && c.y < half && c.z >= 0 && c.z < 2 * half;
            let whole = size == 1.0 && v.turn == [0; 4] && v.color[3] == 255 && v.fx[FX_OPACITY] == 255 && v.fx[FX_COVERED] & 63 != 63;
            if !packs || !whole {
                return None;
            }
            cells.push(c);
        }
        let bits = (1u32 << CELL_BITS) - 1;
        let mut list = Vec::new();
        let mut dirs = [0; 7];
        for (d, start) in dirs.iter_mut().take(6).enumerate() {
            *start = list.len() as u32;
            for (v, c) in voxels.iter().zip(&cells) {
                let covered = v.fx[FX_COVERED];
                if covered >> d & 1 == 1 {
                    continue;
                }
                let first = (!covered & 63).trailing_zeros() as usize == d;
                let lit = v.shade[3] as u8 >> d & 1;
                let cell = (c.x as u32 & bits) | (c.y as u32 & bits) << CELL_BITS | (c.z as u32) << (2 * CELL_BITS) | (d as u32) << DIR_AT | u32::from(lit) << LIT_AT | u32::from(first) << FIRST_AT;
                let [r, g, b, _] = v.color;
                list.push(Face { cell, color: [r, g, b, v.fx[FX_GLOW]], shade: [v.shade[0], v.shade[1], v.shade[2]], metal: v.fx[FX_METAL] });
            }
        }
        dirs[6] = list.len() as u32;
        if size_of_val(&list[..]) > size_of_val(voxels) {
            return None;
        }
        let lo = cells.iter().copied().fold(IVec3::MAX, IVec3::min);
        let hi = cells.iter().copied().fold(IVec3::MIN, IVec3::max) + 1;
        Some(Faces { list, dirs, lo, hi })
    }
}

/// A run's faces once baked, kept with its model (`VoxModel::baked`,
/// `Part::baked`), so that a bank made anew bakes only the runs of models
/// new to it: empty until `ModelBank::bake` fills it (none in it for a run
/// `Faces::of` leaves as cubes).
#[derive(Clone, Default, Debug)]
pub struct Baked(Arc<OnceLock<Option<Faces>>>);

impl Baked {
    pub(super) fn faces(&self) -> Option<&Faces> {
        self.0.get().and_then(Option::as_ref)
    }

    pub(super) fn is_made(&self) -> bool {
        self.0.get().is_some()
    }

    /// Bakes it from its run's voxels, unless it is baked already.
    pub(super) fn make(&self, voxels: &[Instance]) {
        self.0.get_or_init(|| Faces::of(voxels));
    }
}

/// Where a run of the bank lies on the GPU.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Lies {
    /// Among the cubes, from this one on.
    Cubes(u32),
    /// Among the faces: where its own directions' faces begin and the last
    /// end (`Faces::dirs`, among all the bank's), and its box.
    Faces { dirs: [u32; 7], lo: IVec3, hi: IVec3 },
    /// Nowhere: past what the GPU holds (said).
    Nowhere,
}

/// How a bank goes onto the GPU: where each run lies (by its voxels in the
/// bank), how many cubes and faces, and what is to be said.
#[derive(Debug)]
pub(super) struct Plan {
    pub(super) runs: Vec<(Range<u32>, Lies)>,
    pub(super) cubes: u32,
    pub(super) faces: u32,
    pub(super) said: Vec<String>,
}

/// Each run of `bank` where it goes, in the bank's order: its faces where
/// it is baked, else its cubes; within `most` bytes each (the GPU's
/// largest buffer): faces past it go as cubes, cubes past it nowhere, and
/// either is said.
pub(super) fn plan(bank: &ModelBank, most: u64) -> Plan {
    let (face, cube) = (size_of::<Face>() as u64, size_of::<Instance>() as u64);
    let mut plan = Plan { runs: Vec::new(), cubes: 0, faces: 0, said: Vec::new() };
    let fits = |count: u32, more: usize, each: u64| (u64::from(count) + more as u64) * each <= most;
    let (mut as_cubes, mut left) = (0, 0);
    for (voxels, faces) in bank.runs() {
        let lies = match faces {
            Some(f) if fits(plan.faces, f.list.len(), face) => {
                let lies = Lies::Faces { dirs: f.dirs.map(|d| d + plan.faces), lo: f.lo, hi: f.hi };
                plan.faces += f.list.len() as u32;
                lies
            }
            _ if fits(plan.cubes, voxels.len(), cube) => {
                as_cubes += usize::from(faces.is_some());
                plan.cubes += voxels.len() as u32;
                Lies::Cubes(plan.cubes - voxels.len() as u32)
            }
            _ => {
                left += 1;
                Lies::Nowhere
            }
        };
        plan.runs.push((voxels, lies));
    }
    let mb = most / 1_000_000;
    if as_cubes > 0 {
        plan.said.push(format!("models: their faces past the GPU's largest buffer ({mb} MB), {as_cubes} drawn as cubes"));
    }
    if left > 0 {
        plan.said.push(format!("models: past the GPU's largest buffer ({mb} MB), {left} not drawn"));
    }
    plan
}

/// The model bank on the GPU: its runs' faces and cubes, and where each
/// run lies in them.
pub(super) struct OnGpu {
    pub(super) bank: Arc<ModelBank>,
    pub(super) cubes: Option<wgpu::Buffer>,
    pub(super) faces: Option<wgpu::Buffer>,
    runs: Vec<(Range<u32>, Lies)>,
    /// What its buffers take, in bytes.
    pub(super) bytes: u64,
}

impl OnGpu {
    /// `bank` put on the GPU as `plan` says (`plan` of it).
    pub(super) fn new(device: &wgpu::Device, bank: Arc<ModelBank>, plan: Plan) -> Self {
        use wgpu::util::DeviceExt;
        let cubes = (plan.cubes > 0).then(|| {
            // Every run cubes: the bank's voxels as they lie.
            let contents = if plan.cubes as usize == bank.voxels.len() {
                std::borrow::Cow::Borrowed(&bank.voxels[..])
            } else {
                let cubes = plan.runs.iter().filter(|(_, l)| matches!(l, Lies::Cubes(_))).flat_map(|(r, _)| &bank.voxels[r.start as usize..r.end as usize]);
                std::borrow::Cow::Owned(cubes.copied().collect())
            };
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("model bank"), contents: bytemuck::cast_slice(&contents), usage: wgpu::BufferUsages::VERTEX })
        });
        let faces = (plan.faces > 0).then(|| {
            let size = u64::from(plan.faces) * size_of::<Face>() as u64;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("model faces"), size, usage: wgpu::BufferUsages::VERTEX, mapped_at_creation: true });
            {
                let mut bytes = buffer.slice(..).get_mapped_range_mut().expect("a buffer mapped as it is made");
                let mut at = 0;
                for ((_, lies), (_, faces)) in plan.runs.iter().zip(bank.runs()) {
                    if let (Lies::Faces { .. }, Some(f)) = (lies, faces) {
                        let run: &[u8] = bytemuck::cast_slice(&f.list);
                        bytes.slice(at..at + run.len()).copy_from_slice(run);
                        at += run.len();
                    }
                }
            }
            buffer.unmap();
            buffer
        });
        let bytes = u64::from(plan.cubes) * size_of::<Instance>() as u64 + u64::from(plan.faces) * size_of::<Face>() as u64;
        Self { bank, cubes, faces, runs: plan.runs, bytes }
    }

    /// Where the run of `voxels` lies (one not in the bank: nowhere).
    pub(super) fn lies(&self, voxels: &Range<u32>) -> &Lies {
        let from = self.runs.partition_point(|(r, _)| r.start < voxels.start);
        self.runs[from..].iter().take_while(|(r, _)| r.start == voxels.start).find(|(r, _)| r.end == voxels.end).map_or(&Lies::Nowhere, |(_, l)| l)
    }
}

/// The run's own directions (bits in `mesh::DIRS` order) whose faces can
/// face `eye` as `draw` places a run boxed by `lo` and `hi`: on the axes,
/// a face looks the eye's way only where the eye lies before its plane,
/// and every plane of the faces of one way lies within the box; turned
/// freely, all of them.
pub(super) fn facing(draw: &ModelDraw, lo: IVec3, hi: IVec3, eye: Vec3) -> u8 {
    if draw.rot != Quat::IDENTITY {
        return 63;
    }
    // The box's corners where the shader puts them (`model_corner`): the
    // quarter turns about the pivot cell's middle.
    let placed = |p: Vec3| {
        let (x, y) = quarter_f(p.x - 0.5, p.y - 0.5, draw.turn);
        draw.origin + draw.shift + Vec3::new(x + 0.5, y + 0.5, p.z)
    };
    let (a, b) = (placed(lo.as_vec3()), placed(hi.as_vec3()));
    let (lo, hi) = (a.min(b), a.max(b));
    let mut own = 0;
    for (d, n) in DIRS.iter().enumerate() {
        let (x, y) = quarter(n.x, n.y, draw.turn);
        let way = IVec3::new(x, y, n.z);
        let axis = way.abs().max_position();
        let seen = if way[axis] > 0 { eye[axis] > lo[axis] } else { eye[axis] < hi[axis] };
        if seen {
            own |= 1 << d;
        }
    }
    own
}

fn quarter_f(x: f32, y: f32, turn: u32) -> (f32, f32) {
    match turn % 4 {
        0 => (x, y),
        1 => (y, -x),
        2 => (-x, -y),
        _ => (-y, x),
    }
}

/// The ranges of faces to draw of a run whose own directions' faces lie
/// as `dirs` says, of the directions in `own`: those side by side as one.
pub(super) fn ranges(dirs: &[u32; 7], own: u8) -> impl Iterator<Item = Range<u32>> + '_ {
    let mut d = 0;
    std::iter::from_fn(move || {
        while d < 6 && (own >> d & 1 == 0 || dirs[d] == dirs[d + 1]) {
            d += 1;
        }
        if d == 6 {
            return None;
        }
        let start = dirs[d];
        while d < 6 && (own >> d & 1 == 1 || dirs[d] == dirs[d + 1]) {
            d += 1;
        }
        Some(start..dirs[d])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block of one colour, `n` by 3 by 3.
    fn block(n: i32) -> crate::vox::VoxModel {
        let frame = (0..n * 9).map(|k| (IVec3::new(k % n, k / n % 3, k / n / 3), [10, 20, 30, 255])).collect();
        crate::vox::VoxModel::new(IVec3::new(n, 3, 3), vec![frame], vec![Vec::new()])
    }

    /// A voxel of the bank as `ModelBank::add` makes one.
    fn voxel(x: i32, y: i32, z: i32, covered: u8) -> Instance {
        Instance { pos_size: [x as f32, y as f32, z as f32, 1.0], color: [200, 100, 50, 255], fx: [9, 255, covered, 0x7a], shade: [10, -20, 30, 0b100100], turn: [0; 4] }
    }

    /// Each voxel's open faces, by their own direction, carrying the
    /// voxel's cell (negative ones too), colour, glow, metal and shade,
    /// lit where its shade lights that face, the first of each voxel's
    /// marked; the box its voxels fill. A run too thin (faces taking more
    /// than its cubes), or with a voxel a face cannot carry, stays cubes.
    #[test]
    fn a_runs_faces_are_its_voxels_open_faces() {
        // Two voxels, the first's +x face covered, the second's -x; and
        // three open below only, as inside a wall.
        let below = |z: i32| voxel(0, 0, z, 0b011111);
        let run = [voxel(-256, 255, 0, 0b000001), voxel(-255, 255, 511, 0b000010), below(0), below(1), below(2)];
        let f = Faces::of(&run).expect("all pack");
        assert_eq!(f.dirs, [0, 1, 2, 4, 6, 8, 13], "five faces each of the two, by direction, then the three below");
        let cell = |f: &Face| {
            let sign = |v: u32| ((v << (32 - CELL_BITS)) as i32) >> (32 - CELL_BITS);
            let bits = (1 << CELL_BITS) - 1;
            IVec3::new(sign(f.cell & bits), sign(f.cell >> CELL_BITS & bits), (f.cell >> (2 * CELL_BITS) & bits) as i32)
        };
        let dir = |f: &Face| f.cell >> DIR_AT & 7;
        assert_eq!(cell(&f.list[0]), IVec3::new(-255, 255, 511), "+x: the second voxel's");
        assert_eq!(cell(&f.list[1]), IVec3::new(-256, 255, 0), "-x: the first's");
        assert_eq!(cell(&f.list[12]), IVec3::new(0, 0, 2));
        assert!(f.list.iter().enumerate().all(|(i, face)| dir(face) as usize == [0, 1, 2, 2, 3, 3, 4, 4, 5, 5, 5, 5, 5][i]));
        let lit: Vec<u32> = f.list.iter().map(|face| face.cell >> LIT_AT & 1).collect();
        assert_eq!(lit, [0, 0, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1], "the shade lights +y and -z (bits 2 and 5)");
        let first: Vec<u32> = f.list.iter().map(|face| face.cell >> FIRST_AT & 1).collect();
        assert_eq!(first, [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1], "each voxel's lowest open way");
        assert!(f.list.iter().all(|face| face.color == [200, 100, 50, 9] && face.metal == 0x7a && face.shade == [10, -20, 30]));
        assert_eq!((f.lo, f.hi), (IVec3::new(-256, 0, 0), IVec3::new(1, 256, 512)));
        // Thin: ten faces of two voxels take more than their cubes.
        assert!(Faces::of(&run[..2]).is_none());
        // What a face cannot carry.
        assert!(Faces::of(&[below(0), voxel(255, -256, 0, 0b011111)]).is_some());
        for v in [voxel(256, 0, 0, 0b011111), voxel(0, -257, 0, 0b011111), voxel(0, 0, 512, 0b011111), voxel(0, 0, -1, 0b011111), voxel(0, 0, 0, 63)] {
            assert!(Faces::of(&[below(0), v]).is_none(), "{:?} {}", v.pos_size, v.fx[2]);
        }
        let mut half = below(0);
        half.pos_size[0] = 0.5;
        assert!(Faces::of(&[half]).is_none(), "off the cells");
    }

    /// The directions to draw come as ranges, those side by side as one,
    /// empty ones passed over.
    #[test]
    fn the_directions_drawn_come_as_few_ranges() {
        let dirs = [0, 5, 5, 9, 12, 20, 30];
        let all: Vec<_> = ranges(&dirs, 63).map(|r| (r.start, r.end)).collect();
        assert_eq!(all, [(0, 30)]);
        assert_eq!(ranges(&dirs, 0b010101).collect::<Vec<_>>(), [0..9, 12..20], "+x and +y over the empty -x, +z");
        assert_eq!(ranges(&dirs, 0b011001).collect::<Vec<_>>(), [0..5, 9..20], "+x; -y and +z side by side");
        assert_eq!(ranges(&dirs, 0b000010).count(), 0, "-x has none");
        assert_eq!(ranges(&dirs, 0).count(), 0);
    }

    /// Only the faces whose plane the eye can lie before are drawn: a box
    /// seen from beyond its +x side and above shows +x, never -x; seen
    /// from within its span on an axis, both ways of it; turned a quarter,
    /// its own directions as they lie in the world; turned freely, all.
    #[test]
    fn faces_turned_away_from_the_eye_are_left_out() {
        let draw = ModelDraw { voxels: 0..1, origin: Vec3::new(100.0, 100.0, 0.0), turn: 0, rot: Quat::IDENTITY, shift: Vec3::ZERO, tint: None, nearer: 0.0, glow: None, shadow_only: false };
        let (lo, hi) = (IVec3::new(-2, -1, 0), IVec3::new(3, 2, 4));
        // The box from 98 to 103 across, 99 to 102 deep, 0 to 4 high.
        assert_eq!(facing(&draw, lo, hi, Vec3::new(110.0, 100.5, 50.0)), 0b010001 | 0b001100, "+x, both ys, +z");
        assert_eq!(facing(&draw, lo, hi, Vec3::new(98.0, 50.0, 2.0)), 0b111010, "-x (at its plane +x hides), -y, both zs");
        // A quarter turn (facing +x): its own +y looks at the world's +x.
        let turned = ModelDraw { turn: 1, ..draw.clone() };
        let own = facing(&turned, lo, hi, Vec3::new(130.0, 100.0, 50.0));
        assert!(own & 0b000100 != 0 && own & 0b001000 == 0, "own +y, not -y: {own:06b}");
        let free = ModelDraw { rot: Quat::from_rotation_z(0.3), ..draw };
        assert_eq!(facing(&free, lo, hi, Vec3::new(130.0, 100.0, 50.0)), 63);
    }

    /// Faces go to the GPU while they fit its largest buffer, the rest of
    /// the baked as cubes; cubes past it nowhere; each said once.
    #[test]
    fn what_the_gpu_cannot_hold_falls_back_and_is_said() {
        let mut bank = ModelBank::default();
        let a = bank.add_cubes(&[voxel(0, 0, 0, 0), voxel(1, 0, 0, 0)].map(|mut v| {
            v.pos_size = v.pos_size.map(|c| c + 0.5);
            v.pos_size[3] = 1.0;
            v
        }));
        // Blocks n by 3 by 3: 66 voxels seen and 114 faces, 26 and 54.
        let (mut b, mut c) = (block(8), block(3));
        bank.add(&mut b);
        bank.add(&mut c);
        bank.bake();
        let (b, c) = (b.bank[0].clone(), c.bank[0].clone());
        let plenty = plan(&bank, u64::MAX);
        assert!(plenty.said.is_empty());
        assert_eq!((plenty.cubes, plenty.faces), (2, 114 + 54), "the logo's cubes, the models' faces");
        assert_eq!(plenty.runs[0], (a.clone(), Lies::Cubes(0)));
        assert!(matches!(plenty.runs[2].1, Lies::Faces { dirs: [114, ..], .. }), "{:?}", plenty.runs[2]);
        // Room for 125 faces (1500 bytes): the second model as cubes.
        let tight = plan(&bank, 1500);
        assert_eq!((tight.cubes, tight.faces), (2 + 26, 114));
        assert_eq!(tight.runs[2], (c.clone(), Lies::Cubes(2)));
        assert!(tight.said.len() == 1 && tight.said[0].contains("1 drawn as cubes"), "{:?}", tight.said);
        // Room for 25 cubes: the first model nowhere, the second's faces.
        let tighter = plan(&bank, 800);
        assert_eq!(tighter.runs[1], (b, Lies::Nowhere));
        assert_eq!(tighter.runs[2], (c, Lies::Faces { dirs: [0, 9, 18, 27, 36, 45, 54], lo: IVec3::new(-1, -1, 0), hi: IVec3::new(2, 2, 3) }));
        assert!(tighter.said.len() == 1 && tighter.said[0].contains("1 not drawn"), "{:?}", tighter.said);
    }

    /// A bank made anew bakes only what is new: a model's runs baked once
    /// are kept with it and drawn from in the next bank, the same faces;
    /// a bank grown by a model (`Session::bank_fresh`) keeps what it
    /// baked; a model read or shaded anew is baked anew; a bank not baked
    /// draws every run as cubes.
    #[test]
    fn a_bank_made_anew_bakes_only_what_is_new() {
        let model = block;
        let faces = |bank: &ModelBank| bank.runs().map(|(_, f)| f.map(|f| f as *const Faces)).collect::<Vec<_>>();
        let (mut a, mut b) = (model(3), model(5));
        let mut first = ModelBank::default();
        first.add(&mut a);
        first.add(&mut b);
        assert_eq!(faces(&first), [None, None], "not baked: cubes");
        first.bake();
        let was = faces(&first);
        assert!(was.iter().all(Option::is_some));
        let mut again = ModelBank::default();
        again.add(&mut b);
        again.add(&mut a);
        again.bake();
        assert_eq!(faces(&again), [was[1], was[0]], "the same faces, not made again");
        let mut grown = again.clone();
        grown.add(&mut model(4));
        grown.bake();
        assert_eq!(faces(&grown)[..2], faces(&again)[..]);
        assert!(faces(&grown)[2].is_some());
        // Read anew, shaded anew: baked anew.
        let mut read = model(3);
        a.shade(crate::config::ModelShading::Smooth, 0.5);
        let mut anew = ModelBank::default();
        anew.add(&mut read);
        anew.add(&mut a);
        anew.bake();
        assert!(faces(&anew).iter().all(|f| f.is_some() && *f != was[0]));
    }
}
