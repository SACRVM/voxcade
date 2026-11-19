//! The moving things that stop the lamps whose shadows are "all"
//! (`LampShadows::All`): every picture, each model drawn for them (in view
//! or not, within such a lamp's reach; not the level's pieces: they are the
//! level's volume, `volume`) and each opaque cube as a solid of its own:
//! its voxels as bits in its own grid (`bits`, a model's shared by its
//! draws) and where that grid lies (`Solid`: the draw's turn and place, as
//! the shader draws it), with a box round each thing (`boxes`), so a way of
//! light that meets none of them walks nothing. The lamps' pass and the
//! fog's beams walk a way through each solid whose box it meets, in the
//! solid's own grid (`lamps.wgsl`'s `movers_stop`), after the level's
//! volume, for those lamps only, and afresh every picture (the faces'
//! memory keeps only what the level's walls do).
//!
//! A shadow is the shape of what casts it, turned as it is drawn, nothing
//! grown or lost: a thing turning slowly moves its shadow's edge smoothly,
//! so a face changes once as the edge passes. Until 2026-11-19 the things
//! filled the cells of a volume laid as the level's whose middles their
//! turned voxels held: a turned arm a voxel thin left gaps between its
//! cells that came and went as it turned, faces blinking for a picture.
//!
//! Only the picture: nothing here is the game's.

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat3, Quat, Vec3};

use super::{FX_GLOW, FX_OPACITY, Instance, Light, ModelBank, ModelDraw, quarter};

/// The most boxes the walks test (`boxes`): more things are boxed together
/// by the blocks they lie in, ever larger, until there are no more.
pub const MOST_BOXES: usize = 256;

/// The most bytes the solids and their bits may take: past them moving
/// things stop no lamp, said once.
pub const MOST_BYTES: usize = 32 << 20;

/// The cubes' boxes are blocks of this many voxels each way.
const CUBE_BLOCK: i32 = 8;

/// A solid as the walks take it (`lamps.wgsl`'s `Solid`): `to_grid` the
/// rows of the map from the level's voxels to its grid's (a cell a voxel,
/// from the grid's low corner; a cube's grid one cell its size), `grid`
/// its cells each way and where its bits begin in `Movers::bits` (x
/// fastest, then y, then z, 32 a word), `owner` the thing it is (x its key,
/// y 1; `Drawing::of_thing`), whose own lights it stops not (`Light::of`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, PartialEq)]
pub struct Solid {
    pub to_grid: [[f32; 4]; 3],
    pub grid: [i32; 4],
    pub owner: [u32; 4],
}

/// A thing's key as the walks take it (`Solid::owner`, a light's list's
/// head): the key and 1, or nothing.
fn owner(key: Option<u32>) -> [u32; 2] {
    key.map_or([0, 0], |k| [k, 1])
}

/// The bits of a cube's grid: its one cell, filled (`Movers::bits`' first
/// word).
const FULL: i32 = 0;

/// The moving things of a picture.
pub struct Movers {
    /// The boxes round them, low corner and high (past the last cell) in
    /// the level's voxels, where their solids begin in `solids` and how
    /// many (the low corner's w, the high's).
    pub boxes: Vec<[i32; 8]>,
    pub solids: Vec<Solid>,
    /// The models' voxels, a bit each in their grids; its first word a
    /// cube's (`FULL`).
    pub bits: Vec<u32>,
    /// What `bits` holds, to send it to the GPU only when it changed.
    pub key: u64,
}

/// A box growing round what is put into it.
#[derive(Clone, Copy)]
struct Bounds {
    lo: IVec3,
    hi: IVec3,
}

impl Bounds {
    const NONE: Self = Self { lo: IVec3::MAX, hi: IVec3::MIN };

    /// Round the points `p`.
    fn round(p: impl IntoIterator<Item = Vec3>) -> Self {
        p.into_iter().fold(Self::NONE, |b, p| Self { lo: b.lo.min(p.floor().as_ivec3()), hi: b.hi.max(p.ceil().as_ivec3()) })
    }

    fn join(self, o: Self) -> Self {
        Self { lo: self.lo.min(o.lo), hi: self.hi.max(o.hi) }
    }
}

/// A grid of `size` cells, a point of the level `p` in it at
/// `back · p + shift` (`back` a rotation and a scale), as a
/// solid with its bits at `at`, and the box round it.
fn solid(back: Mat3, shift: Vec3, size: IVec3, at: i32) -> (Solid, Bounds) {
    let rows = back.transpose();
    let row = |i: usize| [rows.col(i).x, rows.col(i).y, rows.col(i).z, shift[i]];
    let to_level = back.inverse();
    let corners = (0..8).map(|k| to_level * (IVec3::new(k & 1, k >> 1 & 1, k >> 2) * size).as_vec3() - to_level * shift);
    (Solid { to_grid: [row(0), row(1), row(2)], grid: [size.x, size.y, size.z, at], owner: [0; 4] }, Bounds::round(corners))
}

/// A model's voxels as a grid in its own directions: its low corner, its
/// cells each way and where its bits begin in `Movers::bits`.
#[derive(Clone, Copy)]
struct Grid {
    lo: IVec3,
    size: IVec3,
    at: i32,
}

/// A box and the solids in it.
type Group = (Bounds, Vec<Solid>);

/// The grid of a model's voxels (`range` in the bank), its bits written
/// into `bits`; none when it holds no voxel that stops a lamp (a voxel that
/// glows does not: a lamp's bulb does not stop its own light).
fn grid_of(bank: &ModelBank, range: std::ops::Range<u32>, bits: &mut Vec<u32>) -> Option<Grid> {
    let voxels = bank.voxels.get(range.start as usize..range.end as usize)?;
    let cells = || voxels.iter().filter(|v| v.fx[FX_GLOW] == 0).map(|v| IVec3::new(v.pos_size[0] as i32, v.pos_size[1] as i32, v.pos_size[2] as i32));
    let (lo, hi) = cells().fold((IVec3::MAX, IVec3::MIN), |(lo, hi), c| (lo.min(c), hi.max(c)));
    if lo.x > hi.x {
        return None;
    }
    let size = hi - lo + 1;
    let at = bits.len();
    bits.resize(at + (size.x as usize * size.y as usize * size.z as usize).div_ceil(32), 0);
    for c in cells() {
        let i = ((c.z - lo.z) * size.y + c.y - lo.y) as usize * size.x as usize + (c.x - lo.x) as usize;
        bits[at + i / 32] |= 1 << (i % 32);
    }
    Some(Grid { lo, size, at: at as i32 })
}

/// A draw of a model (its `grid`) as a solid placed as the shader places
/// its voxels
/// (`vs_model`, `model_corner`): a point `u` of the model's own grid (a
/// voxel `l` from `l` to `l + 1`) lies in the level at
/// `origin + shift + rot · (Q · (u - ½) + ½)`, `Q` its quarter turns.
fn draw_solid(draw: &ModelDraw, grid: Grid) -> (Solid, Bounds) {
    let q = |x, y| {
        let (x, y) = quarter(x, y, draw.turn);
        Vec3::new(x as f32, y as f32, 0.0)
    };
    let turn = Mat3::from_quat(draw.rot) * Mat3::from_cols(q(1, 0), q(0, 1), Vec3::Z);
    let half = Vec3::splat(0.5);
    let place = draw.origin + draw.shift + draw.rot * half - turn * half;
    // grid = turnᵀ · (voxel - place) - lo
    let back = turn.transpose();
    solid(back, -(back * place) - grid.lo.as_vec3(), grid.size, grid.at)
}

/// An opaque cube that does not glow as a solid of one cell its size (its
/// bits `FULL`): a point `g` of its grid lies in the level at
/// `centre + turn · size · (g - ½)`, as the shader draws it (`vs_inst`).
fn cube_solid(c: &Instance) -> Option<(Solid, Bounds)> {
    if c.fx[FX_GLOW] > 0 || c.fx[FX_OPACITY] < 255 {
        return None;
    }
    let [x, y, z, size] = c.pos_size;
    let turn = if c.turn == [0; 4] { Quat::IDENTITY } else { Quat::from_array(c.turn.map(|v| v as f32 / 127.0)).normalize() };
    let back = Mat3::from_quat(turn).transpose() * (1.0 / size);
    Some(solid(back, Vec3::splat(0.5) - back * Vec3::new(x, y, z), IVec3::ONE, FULL))
}

/// `groups` (a box and its solids) joined by the blocks of their low
/// corners, ever larger, in their order, until at most `MOST_BOXES` are
/// left.
fn fewer(mut groups: Vec<Group>) -> Vec<Group> {
    let mut block = CUBE_BLOCK * 2;
    while groups.len() > MOST_BOXES {
        let mut joined: Vec<(IVec3, Group)> = Vec::new();
        for (b, solids) in groups {
            let key = b.lo.div_euclid(IVec3::splat(block));
            match joined.iter_mut().find(|(k, _)| *k == key) {
                Some((_, (j, s))) => {
                    *j = j.join(b);
                    s.extend(solids);
                }
                None => joined.push((key, (b, solids))),
            }
        }
        groups = joined.into_iter().map(|(_, g)| g).collect();
        block *= 2;
    }
    groups
}

impl Movers {
    /// The solids of the things `things` (each one's key and draws from
    /// `bank`) and the cubes `cubes`: None when none stops a lamp; a
    /// mistake, in words, when they take more than `MOST_BYTES`.
    pub fn of(bank: &ModelBank, things: &[super::Mover], cubes: &[Instance]) -> Option<Result<Self, String>> {
        let mut bits = vec![1u32];
        let mut grids: std::collections::HashMap<(u32, u32), Option<Grid>> = Default::default();
        let mut groups: Vec<Group> = Vec::new();
        for (key, draws) in things {
            let (mut b, mut solids) = (Bounds::NONE, Vec::new());
            let [k, owned] = owner(*key);
            for d in draws {
                let grid = *grids.entry((d.voxels.start, d.voxels.end)).or_insert_with(|| grid_of(bank, d.voxels.clone(), &mut bits));
                if let Some(grid) = grid {
                    let (mut s, sb) = draw_solid(d, grid);
                    s.owner = [k, owned, 0, 0];
                    b = b.join(sb);
                    solids.push(s);
                }
            }
            if !solids.is_empty() {
                groups.push((b, solids));
            }
        }
        // The cubes boxed by the blocks their middles lie in.
        let mut blocks: Vec<(IVec3, Group)> = Vec::new();
        for c in cubes {
            let Some((s, sb)) = cube_solid(c) else { continue };
            let key = Vec3::from_slice(&c.pos_size[..3]).floor().as_ivec3().div_euclid(IVec3::splat(CUBE_BLOCK));
            match blocks.iter_mut().rev().find(|(k, _)| *k == key) {
                Some((_, (b, solids))) => {
                    *b = b.join(sb);
                    solids.push(s);
                }
                None => blocks.push((key, (sb, vec![s]))),
            }
        }
        groups.extend(blocks.into_iter().map(|(_, g)| g));
        if groups.is_empty() {
            return None;
        }
        let groups = fewer(groups);
        let count: usize = groups.iter().map(|(_, s)| s.len()).sum();
        if count * size_of::<Solid>() + bits.len() * 4 > MOST_BYTES {
            return Some(Err(format!("moving things stop no lamp: their {count} models and cubes take more than {} MB", MOST_BYTES >> 20)));
        }
        let mut solids = Vec::with_capacity(count);
        let boxes = groups
            .into_iter()
            .map(|(b, s)| {
                let first = solids.len() as i32;
                solids.extend(s);
                [b.lo.x, b.lo.y, b.lo.z, first, b.hi.x, b.hi.y, b.hi.z, solids.len() as i32 - first]
            })
            .collect();
        let key = crate::hash::Stable::default().bytes(bytemuck::cast_slice(&bits)).finish();
        Some(Ok(Self { boxes, solids, bits, key }))
    }
}

/// The most entries the lights' lists of boxes take (`Movers::lists`): a
/// light past them walks every box. Each light's list begins with its
/// thing's key (`HEAD`), kept room for whatever the boxes take.
pub const MOST_LISTED: usize = 1 << 16;

/// The words a light's list begins with: its thing's key and 1, or two 0
/// (`owner`), so the walks pass over that thing's solids (the shaders'
/// MOVERS_HEAD).
pub(super) const HEAD: usize = 2;

/// The low bits of a light's word for its boxes (`lists`) that say how
/// many (the shaders' MOVERS_COUNT_BITS), the rest where its list begins.
pub(super) const COUNT_BITS: u32 = 8;

/// A light's word for the boxes it walks (`lists`) meaning every one (the
/// shaders' EVERY_BOX): the most its count says.
pub(super) const EVERY_BOX: u32 = (1 << COUNT_BITS) - 1;

impl Movers {
    /// Each of `lights`' boxes, those it can light (within its reach and,
    /// a spot, its cone), one light's after another in the list given
    /// back, each light's after its thing's key (`HEAD`), and each light's
    /// word for the shaders (`LightData`'s `edge.y` of a lamp whose shadows
    /// are "all"): 2, plus where its list begins times 2^`COUNT_BITS`, plus
    /// how many boxes (`EVERY_BOX`: every one walked); None for the
    /// others. A box a light
    /// cannot reach casts nothing for it, so the walks test a few boxes a
    /// light, not all of them.
    pub fn lists(&self, lights: &[Light]) -> (Vec<u32>, Vec<Option<f32>>) {
        let mut list = Vec::new();
        // Room kept for every light's head, so the last one's fits.
        let room = MOST_LISTED - HEAD * lights.len().min(super::MOST_LIGHTS);
        let words = lights
            .iter()
            .map(|l| {
                if !l.stopped_by_movers() {
                    return None;
                }
                let start = list.len();
                list.extend(owner(l.of));
                for (k, b) in self.boxes.iter().enumerate() {
                    if lit(l, IVec3::new(b[0], b[1], b[2]).as_vec3(), IVec3::new(b[4], b[5], b[6]).as_vec3()) {
                        list.push(k as u32);
                    }
                }
                let count = list.len() - start - HEAD;
                let count = if count >= EVERY_BOX as usize || list.len() > room {
                    list.truncate(start + HEAD);
                    EVERY_BOX
                } else {
                    count as u32
                };
                Some(2.0 + ((start as u32) << COUNT_BITS | count) as f32)
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

    /// Where the point `p` of the level lies in the solid's grid.
    fn in_grid(s: &Solid, p: Vec3) -> Vec3 {
        Vec3::from_array(s.to_grid.map(|r| r[0] * p.x + r[1] * p.y + r[2] * p.z + r[3]))
    }

    /// Whether the solid holds the point `p` of the level (in a voxel of
    /// its own).
    fn holds(m: &Movers, s: &Solid, p: Vec3) -> bool {
        let g = in_grid(s, p).floor().as_ivec3();
        let size = IVec3::new(s.grid[0], s.grid[1], s.grid[2]);
        if g.min_element() < 0 || (size - g).min_element() <= 0 {
            return false;
        }
        let i = ((g.z * size.y + g.y) * size.x + g.x) as usize;
        m.bits[s.grid[3] as usize + i / 32] >> (i % 32) & 1 == 1
    }

    /// A cube is a solid of one cell its size, turned as it is drawn: it
    /// holds the points within it, no more; glowing or see-through it is
    /// none. Its box is round it; cubes are boxed by the blocks they lie
    /// in.
    #[test]
    fn a_cube_is_a_solid_as_it_is_drawn() {
        let bank = ModelBank::default();
        let m = Movers::of(&bank, &[], &[cube(Vec3::new(70.5, 3.5, 9.5), [0; 4], 255, 0), cube(Vec3::new(5.5, 5.5, 5.5), [0; 4], 100, 0), cube(Vec3::new(9.5, 9.5, 9.5), [0; 4], 255, 9)]).unwrap().unwrap();
        assert_eq!(m.solids.len(), 1, "only the opaque cube that does not glow");
        assert_eq!(m.boxes, vec![[70, 3, 9, 0, 71, 4, 10, 1]], "its box, its solid");
        let s = &m.solids[0];
        assert!(holds(&m, s, Vec3::new(70.1, 3.9, 9.5)) && !holds(&m, s, Vec3::new(71.1, 3.5, 9.5)));
        // Turned an eighth round: its corner reaches 0.71 along the axes,
        // the middle of a side 0.5.
        let eighth = crate::render::snorm_turn(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4));
        let m = Movers::of(&bank, &[], &[cube(Vec3::new(70.5, 3.5, 9.5), eighth, 255, 0)]).unwrap().unwrap();
        let s = &m.solids[0];
        assert!(holds(&m, s, Vec3::new(71.15, 3.5, 9.5)), "its corner, past the cell");
        assert!(!holds(&m, s, Vec3::new(70.95, 3.95, 9.5)), "beside its side, within the cell");
        assert!(Movers::of(&bank, &[], &[cube(Vec3::ZERO, [0; 4], 0, 0)]).is_none(), "nothing opaque: none");
    }

    /// A model drawn is a solid of its voxels in its own grid, placed as
    /// the shader places them: on the grid each voxel's own cell, a
    /// quarter turned the same, turned freely its voxels' middles where
    /// `turned_centre` puts them. A voxel that glows stops nothing.
    #[test]
    fn a_model_is_a_solid_as_it_is_drawn() {
        let text = "format = \"voxcade voxel 1\"\nsize = [3, 2, 2]\n\n[palette]\na = \"#b04030\"\ng = { color = \"#f0d060\", glow = 0.8 }\n\n[[frame]]\nlayers = \"\"\"\naag\na..\n\na..\n...\n\"\"\"\n";
        let mut model = crate::vvx::read(text).unwrap();
        let mut bank = ModelBank::default();
        bank.add(&mut model);
        let range = 0..bank.voxels.len() as u32;
        let voxels: Vec<Vec3> = bank.voxels.iter().filter(|v| v.fx[FX_GLOW] == 0).map(|v| Vec3::new(v.pos_size[0], v.pos_size[1], v.pos_size[2])).collect();
        assert_eq!(voxels.len(), 4, "the model's voxels but the glowing one");
        let rot = Quat::from_rotation_z(0.4) * Quat::from_rotation_x(0.3);
        for (turn, rot, shift) in [(0, Quat::IDENTITY, Vec3::ZERO), (1, Quat::IDENTITY, Vec3::ZERO), (3, Quat::IDENTITY, Vec3::ZERO), (2, rot, Vec3::new(0.3, -0.2, 0.1))] {
            let draw = ModelDraw { voxels: range.clone(), origin: Vec3::new(20.0, 30.0, 4.0), turn, rot, shift, tint: None, nearer: 0.0, shadow_only: false };
            let m = Movers::of(&bank, &[(None, vec![draw.clone()])], &[]).unwrap().unwrap();
            assert_eq!(m.solids.len(), 1);
            let s = &m.solids[0];
            for v in &voxels {
                let (x, y) = quarter(v.x as i32, v.y as i32, turn);
                let middle = crate::render::turned_centre(draw.origin, Vec3::new(x as f32, y as f32, v.z), shift, rot);
                assert!(holds(&m, s, middle), "turn {turn}: the voxel at {v} in its middle {middle}");
                assert!((in_grid(s, middle).fract() - Vec3::splat(0.5)).abs().max_element() < 1e-4, "turn {turn}: a voxel's middle is its cell's");
            }
            let all: usize = m.bits[1..].iter().map(|b| b.count_ones() as usize).sum();
            assert_eq!(all, 4, "turn {turn}: its four voxels, not the glowing one");
            let b = m.boxes[0];
            for v in &voxels {
                let (x, y) = quarter(v.x as i32, v.y as i32, turn);
                let middle = crate::render::turned_centre(draw.origin, Vec3::new(x as f32, y as f32, v.z), shift, rot);
                assert!(middle.cmpge(IVec3::new(b[0], b[1], b[2]).as_vec3()).all() && middle.cmple(IVec3::new(b[4], b[5], b[6]).as_vec3()).all(), "turn {turn}: the box holds {middle}");
            }
        }
    }

    /// The smallest turned thing still stops a lamp: a cube turned an
    /// eighth round with its middle at a cell's corner (it holds no cell's
    /// middle, which once left it out) holds the points round its middle,
    /// alone or in a rod of such cubes a voxel apart; a turned body holds
    /// its own voxels' points and nothing grown round them.
    #[test]
    fn the_smallest_turned_thing_still_fills_a_cell() {
        let bank = ModelBank::default();
        let rot = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
        let eighth = crate::render::snorm_turn(rot);
        let rod: Vec<Instance> = (0..6).map(|i| cube(Vec3::new(71.0 + 2.0 * i as f32, 4.0, 9.5), eighth, 255, 0)).collect();
        let m = Movers::of(&bank, &[], &rod).expect("it stops lamps").unwrap();
        for (i, s) in m.solids.iter().enumerate() {
            let middle = Vec3::new(71.0 + 2.0 * i as f32, 4.0, 9.5);
            assert!(holds(&m, s, middle) && holds(&m, s, middle + Vec3::new(0.6, 0.0, 0.0)), "the rod's cube {i} round its middle");
            assert!(!holds(&m, s, middle + Vec3::new(0.4, 0.4, 0.0)), "the rod's cube {i}: nothing past its side");
        }
        // A body 6 by 6 by 6 turned an eighth: the points within its
        // voxels, none a tenth of a voxel past its faces.
        let body: Vec<Instance> = (0..216).map(|i| cube(Vec3::new(40.0, 40.0, 10.0) + rot * Vec3::new((i % 6) as f32, (i / 6 % 6) as f32, (i / 36) as f32), eighth, 255, 0)).collect();
        let m = Movers::of(&bank, &[], &body).unwrap().unwrap();
        let held = |p: Vec3| m.solids.iter().any(|s| holds(&m, s, p));
        for k in 0..6 {
            let inner = Vec3::new(40.0, 40.0, 10.0) + rot * Vec3::new(k as f32, 2.0, 2.0);
            assert!(held(inner), "inside the body at {inner}");
            assert!(!held(Vec3::new(40.0, 40.0, 10.0) + rot * Vec3::new(k as f32, -0.6, 2.0)), "outside its face");
        }
    }

    /// Many things are boxed together by the blocks they lie in until at
    /// most `MOST_BOXES` boxes are left, every solid in one of them.
    #[test]
    fn many_things_are_boxed_together() {
        let bank = ModelBank::default();
        let cubes: Vec<Instance> = (0..40).flat_map(|x| (0..40).map(move |y| cube(Vec3::new(x as f32 * 9.0 + 0.5, y as f32 * 9.0 + 0.5, 1.5), [0; 4], 255, 0))).collect();
        let m = Movers::of(&bank, &[], &cubes).unwrap().unwrap();
        assert!(m.boxes.len() <= MOST_BOXES, "{} boxes", m.boxes.len());
        assert_eq!(m.boxes.iter().map(|b| b[7]).sum::<i32>() as usize, cubes.len());
        assert_eq!(m.solids.len(), cubes.len());
    }

    /// A lamp whose shadows are "all" lists the boxes it can light: a spot
    /// straight down those under it, not one beside its cone or past its
    /// radius; a round lamp every box in reach; a lamp of another word
    /// none (its word unchanged).
    #[test]
    fn a_lamp_lists_the_boxes_it_can_light() {
        use crate::render::{LampShadows, LightShape};
        let bank = ModelBank::default();
        let m = Movers::of(&bank, &[], &[cube(Vec3::new(10.5, 10.5, 1.5), [0; 4], 255, 0), cube(Vec3::new(40.5, 10.5, 1.5), [0; 4], 255, 0), cube(Vec3::new(200.5, 10.5, 1.5), [0; 4], 255, 0)]).unwrap().unwrap();
        assert_eq!(m.boxes.len(), 3);
        let spot = Light::lamp(Vec3::new(10.5, 10.5, 20.0), 60.0, [1.0; 3], &LightShape { cone: Some(30.0), pitch: -90.0, ..Default::default() }, |w| w).stated(LampShadows::All);
        let round = Light { spot: None, ..spot };
        let other = spot.stated(LampShadows::Static);
        let carried = Light { of: Some(7), ..round };
        let (list, words) = m.lists(&[spot, round, Light { shadows: Some(LampShadows::Static), ..other }, carried]);
        assert_eq!(list, vec![0, 0, 0, 0, 0, 0, 1, 7, 1, 0, 1], "each light's thing (none, none, the thing 7), then its boxes: the spot the box under it; the round lamps both in reach, not the one 190 voxels off");
        assert_eq!(words, vec![Some(2.0 + 1.0), Some(2.0 + (3 << 8 | 2) as f32), None, Some(2.0 + (7 << 8 | 2) as f32)]);
    }
}
