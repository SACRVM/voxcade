//! The thin things among what stops lamps, as the fog's beams need them
//! (`render::fog`): a beam takes its points on planes every 4 voxels across
//! the camera's way (8, 16 for a wider lamp), so a shadow thinner than
//! that along a ray may fall between two points that both see the lamp
//! (a post's, seen edge on) and not show. A post, a bar, a rail, a beam of
//! a frame casts such a shadow from wherever the lamp is; a long wall, a
//! floor or a pane a voxel thin only from a lamp in its own plane. So a
//! voxel is thin where its run of solid voxels is shorter than the points'
//! spacing one way (`RUNS`: 4, 8, 16) and shorter than `SECOND` another
//! way, and the fog's beams take
//! their points a voxel apart where a lamp's ray can pass the shadow of a
//! block of `BLOCK` voxels holding one (`fog_beams.wgsl`'s `fine`), and
//! every 4 voxels elsewhere as before.
//!
//! Read from the lamps' volume (`Volume`: the level's opaque voxels and its
//! pieces'), whole rows of voxels a word at a time; made the first time a
//! picture of fog asks, brought up to date where the volume changes. Only
//! the picture.

use std::collections::BTreeMap;

use glam::{IVec3, Vec3};

use super::volume::{Region, Volume};

/// The points' spacings along a beam's ray a voxel's runs are told
/// against: bit k of a block's word, runs shorter than `RUNS[k]`.
pub const RUNS: [i32; 3] = [4, 8, 16];

/// A block of thin voxels, voxels each way.
pub const BLOCK: i32 = 8;

/// A thin voxel's run along a second way is shorter than this too: a post
/// 2 by 4 voxels casts a shadow 2 thin from a lamp beside its long side, a
/// wall 40 long only from a lamp in its plane (left to the planes).
const SECOND: i32 = 16;

/// How far round what is read the runs are looked along: the longest of
/// `RUNS` and `SECOND`.
const APRON: i32 = 16;

/// The slabs `Thin::of` reads on the cores, voxels thick.
const SLAB: i32 = 32;

/// The blocks holding thin voxels: each block's low corner in the level's
/// voxels (y, x, z: a row of blocks along x a range, `near`) and which of
/// `RUNS` its voxels are thin by.
#[derive(Default)]
pub struct Thin {
    blocks: BTreeMap<(i32, i32, i32), u8>,
}

impl Thin {
    /// The thin blocks of the whole volume, on every core (in slabs along
    /// y: levels are wide more than high).
    pub fn of(v: &Volume) -> Self {
        let size = v.reach.texels * 2;
        let slabs: Vec<i32> = (0..size.y).step_by(SLAB as usize).collect();
        let found = crate::par::in_order(&slabs, 1, |part| part.iter().flat_map(|&y| scan(v, IVec3::new(0, y, 0), IVec3::new(size.x, (y + SLAB).min(size.y), size.z))).collect());
        Self { blocks: found.into_iter().collect() }
    }

    /// The blocks of the volume's regions `changed` (copy 0's texels, as
    /// `Volume::update` gives them) found anew.
    pub fn update(&mut self, v: &Volume, changed: &[Region]) {
        for &(lo, size) in changed {
            let (lo, hi) = (lo * 2, ((lo + size) * 2).min(v.reach.texels * 2));
            let corner = v.reach.origin;
            self.blocks.retain(|&(y, x, z), _| {
                let b = IVec3::new(x, y, z) - corner;
                !(b.cmpge(lo).all() && b.cmplt(hi).all())
            });
            self.blocks.extend(scan(v, lo, hi));
        }
    }

    /// The blocks thin by the runs shorter than `run` voxels (one of
    /// `RUNS`, or past them the longest) that lie within the box from `lo`
    /// to `hi` (level voxels), low and high corner, row by row.
    pub fn near(&self, lo: Vec3, hi: Vec3, run: i32) -> Vec<(Vec3, Vec3)> {
        let bit = 1 << RUNS.iter().position(|&r| r >= run).unwrap_or(RUNS.len() - 1);
        let (a, b) = (lo.floor().as_ivec3().div_euclid(IVec3::splat(BLOCK)) * BLOCK, hi.floor().as_ivec3());
        let mut out = Vec::new();
        for y in (a.y..=b.y).step_by(BLOCK as usize) {
            for (&(_, x, z), w) in self.blocks.range((y, a.x, i32::MIN)..=(y, b.x, i32::MAX)) {
                if *w & bit != 0 && z >= a.z && z <= b.z {
                    let lo = Vec3::new(x as f32, y as f32, z as f32);
                    out.push((lo, lo + BLOCK as f32));
                }
            }
        }
        out
    }
}

/// The thin blocks within the volume's voxels from `lo` up to `hi` (from
/// its low corner, whole blocks): their low corners in the level's voxels,
/// y, x, z, and which of `RUNS` they are thin by.
fn scan(v: &Volume, lo: IVec3, hi: IVec3) -> Vec<((i32, i32, i32), u8)> {
    let size = v.reach.texels * 2;
    // What is read: the region and APRON round it, within the volume.
    let (rlo, rhi) = ((lo - APRON).max(IVec3::ZERO), (hi + APRON).min(size));
    let n = rhi - rlo;
    if n.cmple(IVec3::ZERO).any() {
        return Vec::new();
    }
    let words = (n.x as usize).div_ceil(64);
    let rows = (n.y * n.z) as usize;
    let row = |y: i32, z: i32| (y + n.y * z) as usize * words;
    // The solid voxels, a row along x a few words.
    let mut solid = vec![0u64; rows * words];
    let t = v.reach.texels;
    for z in 0..n.z {
        for y in 0..n.y {
            let (gy, gz) = (rlo.y + y, rlo.z + z);
            let shift = ((gy & 1) << 1 | (gz & 1) << 2) as u32;
            let base = (t.x * ((gy >> 1) + t.y * (gz >> 1))) as usize;
            let out = row(y, z);
            for x in 0..n.x {
                let gx = rlo.x + x;
                let byte = v.copies[0][base + (gx >> 1) as usize];
                if (byte >> (shift + (gx & 1) as u32)) & 1 == 1 {
                    solid[out + (x / 64) as usize] |= 1 << (x % 64);
                }
            }
        }
    }
    if solid.iter().all(|w| *w == 0) {
        return Vec::new();
    }
    let mut found: BTreeMap<(i32, i32, i32), u8> = BTreeMap::new();
    // Runs along x, y, z at least each of RUNS long: the longest's first.
    let along = |run| [covered_x(&solid, words, run), covered_rows(&solid, run, words, n.y), covered_rows(&solid, run, words * n.y as usize, n.z)];
    let long = along(SECOND);
    for (k, &run) in RUNS.iter().enumerate() {
        let short = if run == SECOND { long.clone() } else { along(run) };
        for z in (lo.z - rlo.z)..(hi.z - rlo.z) {
            for y in (lo.y - rlo.y)..(hi.y - rlo.y) {
                let at = row(y, z);
                for w in 0..words {
                    let i = at + w;
                    let (nx, ny, nz) = (!short[0][i], !short[1][i], !short[2][i]);
                    let (lx, ly, lz) = (!long[0][i], !long[1][i], !long[2][i]);
                    let mut thin = solid[i] & ((nx & (ly | lz)) | (ny & (lx | lz)) | (nz & (lx | ly)));
                    while thin != 0 {
                        let x = w as i32 * 64 + thin.trailing_zeros() as i32;
                        thin &= thin - 1;
                        if x < lo.x - rlo.x || x >= hi.x - rlo.x {
                            continue;
                        }
                        let b = (IVec3::new(x, y, z) + rlo).div_euclid(IVec3::splat(BLOCK)) * BLOCK + v.reach.origin;
                        *found.entry((b.y, b.x, b.z)).or_default() |= 1 << k;
                    }
                }
            }
        }
    }
    found.into_iter().collect()
}

/// Of the rows `solid` (`words` a row), the voxels lying in a run along x
/// of at least `run` (a power of two): the runs' starts by halvings, then
/// spread over each run.
fn covered_x(solid: &[u64], words: usize, run: i32) -> Vec<u64> {
    let mut starts = solid.to_vec();
    let mut s = 1;
    while s < run {
        starts = starts.chunks(words).flat_map(|r| (0..words).map(move |w| r[w] & shr(r, w, s))).collect::<Vec<u64>>();
        s *= 2;
    }
    let mut cover = starts;
    let mut s = 1;
    while s < run {
        cover = cover.chunks(words).flat_map(|r| (0..words).map(move |w| r[w] | shl(r, w, s))).collect::<Vec<u64>>();
        s *= 2;
    }
    cover
}

/// Bit x of the row `r` shifted to x - `s` (word `w`).
fn shr(r: &[u64], w: usize, s: i32) -> u64 {
    let next = r.get(w + 1).copied().unwrap_or(0);
    (r[w] >> s) | if s == 0 { 0 } else { next << (64 - s) }
}

/// Bit x of the row `r` shifted to x + `s` (word `w`).
fn shl(r: &[u64], w: usize, s: i32) -> u64 {
    let before = if w == 0 { 0 } else { r[w - 1] };
    (r[w] << s) | if s == 0 { 0 } else { before >> (64 - s) }
}

/// Of the words `solid`, the voxels lying in a run of at least `run` (a
/// power of two) along y or z: the words `stride` apart are neighbours
/// that way, `n` of them in a line.
fn covered_rows(solid: &[u64], run: i32, stride: usize, n: i32) -> Vec<u64> {
    let place = |i: usize| (i / stride) as i32 % n;
    let mut starts = solid.to_vec();
    let mut s = 1;
    while s < run {
        let next: Vec<u64> = (0..solid.len()).map(|i| if place(i) + s < n { starts[i] & starts[i + s as usize * stride] } else { 0 }).collect();
        starts = next;
        s *= 2;
    }
    let mut cover = starts;
    let mut s = 1;
    while s < run {
        let next: Vec<u64> = (0..solid.len()).map(|i| if place(i) >= s { cover[i] | cover[i - s as usize * stride] } else { cover[i] }).collect();
        cover = next;
        s *= 2;
    }
    cover
}

#[cfg(test)]
mod tests {
    use super::super::ModelBank;
    use super::super::volume::Reach;
    use super::*;
    use crate::world::World;

    /// A level 128 by 96 by 40 with a floor 4 thick and the boxes `solid`
    /// (low corner, size) on it; its volume whole.
    fn volume_of(solid: &[(IVec3, IVec3)]) -> (World, Volume) {
        let mut palette = [[0u8; 4]; 256];
        palette[1] = [200, 200, 200, 255];
        let mut w = World::new(IVec3::new(128, 96, 40), palette);
        for &(lo, size) in solid.iter().chain(&[(IVec3::ZERO, IVec3::new(128, 96, 4))]) {
            for z in lo.z..lo.z + size.z {
                for y in lo.y..lo.y + size.y {
                    for x in lo.x..lo.x + size.x {
                        w.set(IVec3::new(x, y, z), 1);
                    }
                }
            }
        }
        let reach = Reach::of(&[(Vec3::new(64.0, 48.0, 20.0), 200.0)], w.size);
        let v = Volume::new(&w, reach, &ModelBank::default(), &[], 0);
        (w, v)
    }

    /// The blocks thin by `run`, low corners, z, y, x in order.
    fn thin_at(t: &Thin, run: i32) -> Vec<IVec3> {
        let mut all: Vec<IVec3> = t.near(Vec3::splat(-1000.0), Vec3::splat(1000.0), run).into_iter().map(|(lo, _)| lo.as_ivec3()).collect();
        all.sort_by_key(|b| (b.z, b.y, b.x));
        all
    }

    /// Thin are posts and rails, thinner than a beam's points are apart
    /// two ways of the three: a post a voxel thin and one of 2 by 4 voxels,
    /// a rail; not a wall a voxel thin (its shadow is thin only from a lamp
    /// in its plane), not the floor, not a pillar 6 by 6 but for a beam
    /// whose points lie 8 apart. A post set later is found where the volume
    /// changed, one taken away goes.
    #[test]
    fn posts_and_rails_are_thin_walls_and_floors_are_not() {
        let up = |x, y, w, d| (IVec3::new(x, y, 4), IVec3::new(w, d, 20));
        let (mut w, mut v) = volume_of(&[up(10, 10, 1, 1), up(30, 10, 2, 4), up(50, 10, 6, 6), up(70, 10, 1, 40), (IVec3::new(90, 20, 12), IVec3::new(1, 30, 1))]);
        let mut t = Thin::of(&v);
        assert_eq!(thin_at(&t, 4), [IVec3::new(8, 8, 0), IVec3::new(24, 8, 0), IVec3::new(8, 8, 8), IVec3::new(24, 8, 8), IVec3::new(88, 16, 8), IVec3::new(88, 24, 8), IVec3::new(88, 32, 8), IVec3::new(88, 40, 8), IVec3::new(88, 48, 8), IVec3::new(8, 8, 16), IVec3::new(24, 8, 16)], "the posts, three blocks high each, and the rail");
        let wider = thin_at(&t, 8);
        assert!(wider.contains(&IVec3::new(48, 8, 8)) && !thin_at(&t, 4).contains(&IVec3::new(48, 8, 8)), "the pillar for points 8 apart only: {wider:?}");
        assert!(!wider.iter().any(|b| b.x == 64 || b.x == 72), "never the wall");
        assert!(!wider.iter().any(|b| b.z == 0 && b.x > 96), "never the floor");
        // A post at (100, 70) set later, the thin one at (10, 10) taken away.
        for z in 4..24 {
            w.set(IVec3::new(100, 70, z), 1);
            w.set(IVec3::new(10, 10, z), 0);
        }
        let changed = v.update(&w);
        t.update(&v, &changed);
        let now = thin_at(&t, 4);
        assert!(now.contains(&IVec3::new(96, 64, 8)), "the new post: {now:?}");
        assert!(!now.contains(&IVec3::new(8, 8, 8)), "the old one gone");
        assert_eq!(now, thin_at(&Thin::of(&v), 4), "as found anew");
    }

    /// Runs told a word at a time: along x across a word's end, along y.
    #[test]
    fn runs_are_told_across_words() {
        // Two words a row: a run of 5 from x 61, one of 2 from x 100.
        let mut row = [0u64; 2];
        for x in (61..66).chain(100..102) {
            row[x / 64] |= 1 << (x % 64);
        }
        let c = covered_x(&row, 2, 4);
        let set: Vec<usize> = (0..128).filter(|x| c[x / 64] >> (x % 64) & 1 == 1).collect();
        assert_eq!(set, (61..66).collect::<Vec<_>>(), "the run of 5 covered, the run of 2 not");
        // Six rows of one word, a run along y of 4 at bit 3 from row 1.
        let rows: Vec<u64> = (0..6).map(|y| if (1..5).contains(&y) { 1 << 3 } else { 0 }).collect();
        let c = covered_rows(&rows, 4, 1, 6);
        assert_eq!(c, [0, 8, 8, 8, 8, 0]);
        assert_eq!(covered_rows(&rows, 8, 1, 6), [0; 6], "shorter than 8");
    }
}
