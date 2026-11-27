//! The level's solid voxels as the lamps' pass reads them (`lamps`): a bit
//! a voxel, eight a byte (a texel of 2 by 2 by 2 voxels, bit `x&1 | (y&1)<<1
//! | (z&1)<<2`), and above them `LEVELS - 1` coarser copies, a texel of copy
//! m a bit for each of its eight children in copy m - 1 holding anything: a
//! way of light walks past empty space in big steps. The world's opaque
//! voxels (glass lets light through) and the opaque voxels of the level's
//! pieces (a bulb that glows stops nothing) are in it; moving things are
//! not.
//!
//! Only the part of the level its lamps reach (`Reach`: the boxes round
//! them, a block more each way), so a big level with a few lamps takes
//! little. Made when a picture first has a lamp walls stop, from the world
//! as the picture has it, and kept: a chunk the game changed (a blast), or
//! one a piece left or came to (a figure posed anew, `Volume::repose`), is
//! made anew with the blocks of `BLOCK` texels round it, every copy, and
//! only those go to the GPU; a lamp reaching past the box makes it anew
//! round the lamps as they are. Only the picture: nothing here is the
//! game's.

use glam::{IVec3, Vec3};

use super::{ModelBank, ModelDraw, off_grid, quarter, turned_centre};
use crate::world::{CHUNK, CellMap, World};

/// How many copies the volume has: copy 0 the voxels, each further one
/// twice as coarse.
pub const LEVELS: u32 = 6;

/// The top copy's texel in copy 0's texels each way: the volume is a whole
/// number of them, and a change is made anew a block at a time.
const BLOCK: i32 = 1 << (LEVELS - 1);

/// A block's voxels each way.
const BLOCK_VOXELS: i32 = 2 * BLOCK;

/// The most bytes the volume may take on the GPU (every copy): lamps
/// reaching further apart (or past the GPU's 3D textures) shine through
/// walls, said once. All of More Blood's map (640 by 2368 by 80) would take
/// 17 MB.
pub const MOST_BYTES: u64 = 128 << 20;

/// A lamp in solid voxels (a lamp set into a wall, a lantern's housing)
/// walks from the nearest open voxel at most this far (`Volume::start`);
/// buried deeper, it lights nothing. The moving things' walks leave the
/// thing a lamp sits in as far (`lamps.wgsl`'s `solid_stops`).
pub const HOUSING: f32 = 8.0;

/// A box of copy 0's texels from the volume's low corner, low corner and
/// size (whole blocks): what changed, to go to the GPU.
pub type Region = (IVec3, IVec3);

/// The part of the level a volume holds: its low corner in the world's
/// voxels (whole blocks) and copy 0's texels each way (whole blocks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reach {
    pub origin: IVec3,
    pub texels: IVec3,
}

/// The box of the world the lights `lit` (where, how far) reach, voxels:
/// low and high corner, within the world of `size`.
fn lit_box(lit: &[(Vec3, f32)], size: IVec3) -> (IVec3, IVec3) {
    let (mut lo, mut hi) = (IVec3::MAX, IVec3::MIN);
    for &(pos, r) in lit {
        lo = lo.min((pos - r).floor().as_ivec3());
        hi = hi.max((pos + r).ceil().as_ivec3());
    }
    (lo.clamp(IVec3::ZERO, size), hi.clamp(IVec3::ZERO, size))
}

impl Reach {
    /// The part of a world of `size` the lights `lit` reach, in whole
    /// blocks, a block more each way (a lamp moving a little needs no new
    /// volume).
    pub fn of(lit: &[(Vec3, f32)], size: IVec3) -> Self {
        let (lo, hi) = lit_box(lit, size);
        let top = (size + BLOCK_VOXELS - 1) / BLOCK_VOXELS * BLOCK_VOXELS;
        let origin = (lo.div_euclid(IVec3::splat(BLOCK_VOXELS)) - 1).max(IVec3::ZERO) * BLOCK_VOXELS;
        let end = ((hi + BLOCK_VOXELS - 1).div_euclid(IVec3::splat(BLOCK_VOXELS)) + 1).max(IVec3::ONE) * BLOCK_VOXELS;
        let end = end.min(top).max(origin + BLOCK_VOXELS);
        Self { origin, texels: (end - origin) / 2 }
    }

    /// Whether it holds all the lights `lit` reach (of a world of `size`).
    pub fn holds(&self, lit: &[(Vec3, f32)], size: IVec3) -> bool {
        let (lo, hi) = lit_box(lit, size);
        lo.cmpge(self.origin).all() && hi.cmple(self.origin + self.texels * 2).all()
    }

    /// The bytes it takes on the GPU, every copy.
    pub fn bytes(&self) -> u64 {
        let t = self.texels.as_u64vec3();
        (0..LEVELS).map(|m| (t.x >> m) * (t.y >> m) * (t.z >> m)).sum()
    }

    /// Whether a GPU whose 3D textures are at most `most` texels each way
    /// holds it within `MOST_BYTES`; else what is wrong, in words.
    pub fn fits(&self, most: u32) -> Result<(), String> {
        let v = self.texels * 2;
        if self.texels.max_element() as u32 > most {
            Err(format!("its lamps reach {} by {} by {} voxels, more than this GPU's 3D textures hold ({} each way)", v.x, v.y, v.z, most * 2))
        } else if self.bytes() > MOST_BYTES {
            Err(format!("its lamps reach {} by {} by {} voxels, which would take {} MB, past {} MB", v.x, v.y, v.z, self.bytes() >> 20, MOST_BYTES >> 20))
        } else {
            Ok(())
        }
    }

    /// Copy 0's texel of the world's voxel `c`, from the low corner.
    fn texel(&self, c: IVec3) -> IVec3 {
        (c - self.origin) >> 1
    }

    fn inside(&self, t: IVec3) -> bool {
        t.cmpge(IVec3::ZERO).all() && t.cmplt(self.texels).all()
    }
}

pub struct Volume {
    pub reach: Reach,
    /// Every copy's bytes, x fastest, then y, then z.
    pub copies: Vec<Vec<u8>>,
    /// The world as it was made from (its chunks shared, `World::chunks_unlike`).
    world: World,
    /// The pieces' cells, by the chunk they lie in, each with its draw's
    /// place in `draws`.
    pieces: CellMap<Vec<(u32, IVec3)>>,
    /// The pieces' draws as they were and the bank they are drawn from
    /// (`ModelBank::made`).
    draws: Vec<ModelDraw>,
    made: u64,
    /// What the pieces were (`pieces_key`).
    pub pieces_key: u64,
    /// The box the voxels the last `update` or `repose` changed lie in
    /// (the level's voxels, low corner and high, the high one past it):
    /// the chunks the world changed, the cells the pieces that moved left
    /// and came to. None: nothing changed.
    pub moved: Option<(IVec3, IVec3)>,
}

/// A box (low corner, high corner past it) grown to hold the box `lo` to
/// `hi`.
fn grown((a, b): (IVec3, IVec3), lo: IVec3, hi: IVec3) -> (IVec3, IVec3) {
    (a.min(lo), b.max(hi))
}

/// No box yet, for `grown`.
const NO_BOX: (IVec3, IVec3) = (IVec3::MAX, IVec3::MIN);

/// The box of the world's chunks `chunks`.
fn chunks_box(chunks: &[IVec3]) -> (IVec3, IVec3) {
    chunks.iter().fold(NO_BOX, |b, cc| grown(b, *cc * CHUNK, (*cc + 1) * CHUNK))
}

/// `cc` pushed onto `chunks` unless it is the last there already (a
/// draw's cells come chunk after chunk, mostly).
fn once(chunks: &mut Vec<IVec3>, cc: IVec3) {
    if chunks.last() != Some(&cc) {
        chunks.push(cc);
    }
}

/// Whether two draws fill the same cells (`draw_cells`): what
/// `pieces_key` takes of a draw.
fn same_cells(a: &ModelDraw, b: &ModelDraw) -> bool {
    let bits = |d: &ModelDraw| (d.origin.to_array().map(f32::to_bits), d.shift.to_array().map(f32::to_bits), d.rot.to_array().map(f32::to_bits));
    a.voxels == b.voxels && a.turn == b.turn && bits(a) == bits(b)
}

/// A key of the pieces' draws and the bank they are drawn from (which, not
/// how far it has grown: a model put in after, drawn the first time, leaves
/// the pieces' runs as they were): the volume is made anew when it changes.
pub fn pieces_key(bank: &ModelBank, pieces: &[ModelDraw]) -> u64 {
    let mut h = crate::hash::Stable::default();
    h.u64(bank.made);
    for d in pieces {
        h.u64(u64::from(d.voxels.start)).u64(u64::from(d.voxels.end)).vec3(d.origin).u64(u64::from(d.turn)).vec3(d.shift);
        for c in d.rot.to_array() {
            h.f32(c);
        }
    }
    h.finish()
}

/// The cells a draw's opaque voxels fill, as the shader places them
/// (`vs_model`): on the grid exactly; turned freely, each voxel's middle
/// and eight points 0.3 from it along its own axes, so a wall at a slant
/// leaves no gap a way of light slips through (it grows by about half a
/// voxel; a face of it looks past that, `lamps.wgsl`'s RIM). A voxel that
/// glows is left out: a lamp's bulb does not stop its own light.
fn draw_cells(bank: &ModelBank, draw: &ModelDraw, mut put: impl FnMut(IVec3)) {
    let free = off_grid(draw.rot, draw.shift);
    let Some(voxels) = bank.voxels.get(draw.voxels.start as usize..draw.voxels.end as usize) else { return };
    let corners: [Vec3; 8] = std::array::from_fn(|k| Vec3::new(if k & 1 == 0 { -0.3 } else { 0.3 }, if k & 2 == 0 { -0.3 } else { 0.3 }, if k & 4 == 0 { -0.3 } else { 0.3 }));
    for v in voxels {
        if v.fx[super::FX_GLOW] > 0 {
            continue;
        }
        let [lx, ly, lz, _] = v.pos_size;
        let (x, y) = quarter(lx as i32, ly as i32, draw.turn);
        let local = Vec3::new(x as f32, y as f32, lz);
        if free {
            put(turned_centre(draw.origin, local, draw.shift, draw.rot).floor().as_ivec3());
            for o in corners {
                put((draw.origin + draw.shift + draw.rot * (local + 0.5 + o)).floor().as_ivec3());
            }
        } else {
            put((draw.origin + local + 0.5).floor().as_ivec3());
        }
    }
}

impl Volume {
    /// The volume of `world` and the pieces `pieces` draws within `reach`,
    /// every copy.
    pub fn new(world: &World, reach: Reach, bank: &ModelBank, pieces: &[ModelDraw], pieces_key: u64) -> Self {
        let t = reach.texels;
        let mut v = Self { reach, copies: vec![vec![0; (t.x * t.y * t.z) as usize]], world: world.clone(), pieces: CellMap::default(), draws: pieces.to_vec(), made: bank.made, pieces_key, moved: None };
        for (i, d) in pieces.iter().enumerate() {
            v.put_piece(bank, i as u32, d, |_| {});
        }
        let mut chunks: Vec<IVec3> = world.chunk_keys().into_iter().filter(|cc| v.holds_chunk(*cc)).collect();
        chunks.extend(v.pieces.keys().copied().filter(|cc| world.chunk(*cc).is_none()));
        chunks.sort_unstable_by_key(|c| (c.z, c.y, c.x));
        v.fill_chunks(world, &chunks);
        for m in 1..LEVELS as i32 {
            v.copies.push(vec![0; ((t.x >> m) * (t.y >> m) * (t.z >> m)) as usize]);
        }
        v.coarse(IVec3::ZERO, t);
        v
    }

    /// Whether the chunk `cc` lies (partly) within the volume.
    fn holds_chunk(&self, cc: IVec3) -> bool {
        let lo = self.reach.texel(cc * CHUNK);
        (lo + CHUNK / 2).cmpgt(IVec3::ZERO).all() && lo.cmplt(self.reach.texels).all()
    }

    /// The cells within the volume of the draw `d` (the `i`th piece) put
    /// among the pieces', each told to `put`.
    fn put_piece(&mut self, bank: &ModelBank, i: u32, d: &ModelDraw, mut put: impl FnMut(IVec3)) {
        let (reach, pieces) = (self.reach, &mut self.pieces);
        draw_cells(bank, d, |c| {
            if reach.inside(reach.texel(c)) {
                put(c);
                pieces.entry(World::chunk_of(c)).or_default().push((i, c));
            }
        });
    }

    /// Takes in what changed in `world` since it was made or last brought
    /// up to date (its chunks written since): the blocks made anew, to go
    /// to the GPU. Nothing when nothing changed.
    pub fn update(&mut self, world: &World) -> Vec<Region> {
        let chunks = world.chunks_unlike(&self.world);
        let within = chunks_box(&chunks);
        self.remake(world, chunks, within)
    }

    /// Takes in `world` as `update` does and the pieces drawn as `pieces`
    /// (their key `pieces_key`): the chunks the pieces that moved (a
    /// figure's part posed anew) left or came to made anew too, the blocks
    /// to go to the GPU. None where they are not the same pieces moved
    /// (another bank, more or fewer of them): the volume is made anew then.
    /// The same volume as made anew either way.
    pub fn repose(&mut self, world: &World, bank: &ModelBank, pieces: &[ModelDraw], pieces_key: u64) -> Option<Vec<Region>> {
        if bank.made != self.made || pieces.len() != self.draws.len() {
            return None;
        }
        let moved: Vec<u32> = (0..pieces.len()).filter(|&i| !same_cells(&self.draws[i], &pieces[i])).map(|i| i as u32).collect();
        let mut chunks = world.chunks_unlike(&self.world);
        let mut within = chunks_box(&chunks);
        let mut left = Vec::new();
        for &i in &moved {
            draw_cells(bank, &self.draws[i as usize], |c| {
                once(&mut left, World::chunk_of(c));
                within = grown(within, c, c + 1);
            });
        }
        left.sort_unstable_by_key(|c| (c.z, c.y, c.x));
        left.dedup();
        for cc in &left {
            if let Some(cells) = self.pieces.get_mut(cc) {
                cells.retain(|(i, _)| moved.binary_search(i).is_err());
                if cells.is_empty() {
                    self.pieces.remove(cc);
                }
            }
        }
        chunks.extend(left);
        for &i in &moved {
            self.put_piece(bank, i, &pieces[i as usize], |c| {
                once(&mut chunks, World::chunk_of(c));
                within = grown(within, c, c + 1);
            });
            self.draws[i as usize] = pieces[i as usize].clone();
        }
        self.pieces_key = pieces_key;
        Some(self.remake(world, chunks, within))
    }

    /// The chunks `chunks` (any order, each any number of times) of copy 0
    /// made anew from `world` and the pieces, and the blocks they lie in of
    /// every copy: those blocks, to go to the GPU. The world taken as the
    /// one it is made from; what changed lies `within` (`moved`).
    fn remake(&mut self, world: &World, mut chunks: Vec<IVec3>, within: (IVec3, IVec3)) -> Vec<Region> {
        self.world = world.clone();
        chunks.sort_unstable_by_key(|c| (c.z, c.y, c.x));
        chunks.dedup();
        chunks.retain(|cc| self.holds_chunk(*cc));
        self.moved = (!chunks.is_empty()).then_some(within);
        if chunks.is_empty() {
            return Vec::new();
        }
        for &cc in &chunks {
            self.clear_chunk(cc);
        }
        // Each chunk's pieces again too, which share its texels.
        let redo: Vec<IVec3> = chunks.iter().copied().filter(|cc| world.chunk(*cc).is_some() || self.pieces.contains_key(cc)).collect();
        self.fill_chunks(world, &redo);
        // The blocks they lie in, each once, in order.
        let mut blocks: Vec<IVec3> = chunks.iter().map(|cc| self.reach.texel(*cc * CHUNK).div_euclid(IVec3::splat(BLOCK))).collect();
        blocks.sort_unstable_by_key(|b| (b.z, b.y, b.x));
        blocks.dedup();
        let size = IVec3::splat(BLOCK);
        let changed: Vec<Region> = blocks.into_iter().map(|b| (b * BLOCK, size)).filter(|(lo, _)| self.reach.inside(*lo)).collect();
        for &(lo, size) in &changed {
            self.coarse(lo, size);
        }
        changed
    }

    /// Copy 0's texels of the chunk `cc` emptied.
    fn clear_chunk(&mut self, cc: IVec3) {
        let half = CHUNK / 2;
        let (lo, t) = (self.reach.texel(cc * CHUNK), self.reach.texels);
        for z in lo.z.max(0)..(lo.z + half).min(t.z) {
            for y in lo.y.max(0)..(lo.y + half).min(t.y) {
                let row = (t.x * (y + t.y * z)) as usize;
                let (a, b) = (lo.x.clamp(0, t.x) as usize, (lo.x + half).clamp(0, t.x) as usize);
                self.copies[0][row + a..row + b].fill(0);
            }
        }
    }

    /// The bits of `chunks` (empty before) from the world's voxels and the
    /// pieces lying in them: the chunks worked out on every core, written
    /// in the order given.
    fn fill_chunks(&mut self, world: &World, chunks: &[IVec3]) {
        let half = CHUNK / 2;
        let palette = &world.palette;
        let bits: Vec<(IVec3, Vec<u8>)> = crate::par::in_order(chunks, 16, |part| {
            part.iter()
                .map(|&cc| {
                    let mut out = vec![0u8; (half * half * half) as usize];
                    if let Some(chunk) = world.chunk(cc) {
                        for (i, &v) in chunk.voxels.iter().enumerate() {
                            if v != 0 && palette[v as usize][3] == 255 {
                                let i = i as i32;
                                let (x, y, z) = (i % CHUNK, (i / CHUNK) % CHUNK, i / (CHUNK * CHUNK));
                                out[((x >> 1) + half * ((y >> 1) + half * (z >> 1))) as usize] |= 1 << ((x & 1) | (y & 1) << 1 | (z & 1) << 2);
                            }
                        }
                    }
                    (cc, out)
                })
                .collect()
        });
        let (reach, t) = (self.reach, self.reach.texels);
        for (cc, out) in bits {
            let lo = reach.texel(cc * CHUNK);
            for z in 0..half {
                for y in 0..half {
                    let (gy, gz) = (lo.y + y, lo.z + z);
                    if gy < 0 || gz < 0 || gy >= t.y || gz >= t.z {
                        continue;
                    }
                    let row = (t.x * (gy + t.y * gz)) as usize;
                    for x in 0..half {
                        let gx = lo.x + x;
                        let b = out[(x + half * (y + half * z)) as usize];
                        if b != 0 && gx >= 0 && gx < t.x {
                            self.copies[0][row + gx as usize] |= b;
                        }
                    }
                }
            }
            if let Some(cells) = self.pieces.get(&cc) {
                for &(_, c) in cells {
                    let tc = reach.texel(c);
                    if reach.inside(tc) {
                        self.copies[0][(tc.x + t.x * (tc.y + t.y * tc.z)) as usize] |= 1 << ((c.x & 1) | (c.y & 1) << 1 | (c.z & 1) << 2);
                    }
                }
            }
        }
    }

    /// The coarser copies (`copies[1..]`) of the box of copy 0's texels at
    /// `lo`, `size` big (whole blocks), made anew from copy 0.
    fn coarse(&mut self, lo: IVec3, size: IVec3) {
        let (copies, texels) = (&mut self.copies, self.reach.texels);
        for m in 1..LEVELS as i32 {
            let (below, here) = (texels >> (m - 1), texels >> m);
            let (blo, bsize) = (lo >> (m - 1), size >> (m - 1));
            let (src, rest) = copies.split_at_mut(m as usize);
            let (src, out) = (&src[(m - 1) as usize], &mut rest[0]);
            // This copy's texels of the box emptied, then a bit set for each
            // child that holds anything.
            for z in (blo.z >> 1)..((blo.z + bsize.z) >> 1) {
                for y in (blo.y >> 1)..((blo.y + bsize.y) >> 1) {
                    let row = (here.x * (y + here.y * z)) as usize;
                    out[row + (blo.x >> 1) as usize..row + ((blo.x + bsize.x) >> 1) as usize].fill(0);
                }
            }
            for z in blo.z..blo.z + bsize.z {
                for y in blo.y..blo.y + bsize.y {
                    let row = (below.x * (y + below.y * z)) as usize;
                    let parent = (here.x * ((y >> 1) + here.y * (z >> 1))) as usize;
                    let bits = ((y & 1) << 1 | (z & 1) << 2) as u8;
                    for x in blo.x..blo.x + bsize.x {
                        if src[row + x as usize] != 0 {
                            out[parent + (x >> 1) as usize] |= 1 << (bits | (x & 1) as u8);
                        }
                    }
                }
            }
        }
    }

    /// The bytes of copy `m` in the box of copy 0's texels at `lo`, `size`
    /// big: what goes to the GPU for it.
    pub fn bytes_of(&self, m: u32, lo: IVec3, size: IVec3) -> Vec<u8> {
        let (t, lo, size) = (self.reach.texels >> m as i32, lo >> m as i32, size >> m as i32);
        let mut out = Vec::with_capacity((size.x * size.y * size.z) as usize);
        for z in lo.z..lo.z + size.z {
            for y in lo.y..lo.y + size.y {
                let row = (t.x * (y + t.y * z) + lo.x) as usize;
                out.extend_from_slice(&self.copies[m as usize][row..row + size.x as usize]);
            }
        }
        out
    }

    /// Whether the world it was made from has the same size and palette as
    /// `world` (else it is made anew: another level, glass come or gone).
    pub fn same_level(&self, world: &World) -> bool {
        self.world.size == world.size && self.world.palette == world.palette
    }

    /// Where the lamp at `pos` walks from (`lamps.wgsl`'s `walk`), once a
    /// picture: itself in the open; in solid voxels (a lamp set into a wall,
    /// a lantern's housing) the middle of the nearest open voxel within
    /// `HOUSING`, the first of equals by z, y, x, so a lamp flush with a
    /// wall shines on its own side only; None buried deeper (it lights
    /// nothing). One place for every way: each leaving the solid on its own
    /// would go through a wall up to `HOUSING` thick to the room behind it.
    pub fn start(&self, pos: Vec3) -> Option<Vec3> {
        let at = pos.floor().as_ivec3();
        if !self.solid(at, 0) {
            return Some(pos);
        }
        let r = HOUSING as i32;
        let mut best: Option<(f32, Vec3)> = None;
        for z in -r..=r {
            for y in -r..=r {
                for x in -r..=r {
                    let c = at + IVec3::new(x, y, z);
                    let middle = c.as_vec3() + 0.5;
                    let d = middle.distance_squared(pos);
                    if d <= HOUSING * HOUSING && best.is_none_or(|(b, _)| d < b) && !self.solid(c, 0) {
                        best = Some((d, middle));
                    }
                }
            }
        }
        best.map(|(_, m)| m)
    }

    /// Whether the world's voxel `c` is solid in copy `level`'s cell of it
    /// (2^level voxels across), as the shader asks (`lamps.wgsl`'s
    /// `solid`); outside the volume nothing is.
    pub fn solid(&self, c: IVec3, level: i32) -> bool {
        let cell: IVec3 = (c - self.reach.origin) >> level;
        let t: IVec3 = cell >> 1;
        let dims: IVec3 = self.reach.texels >> level;
        if t.cmplt(IVec3::ZERO).any() || t.cmpge(dims).any() {
            return false;
        }
        let bits = self.copies[level as usize][(t.x + dims.x * (t.y + dims.y * t.z)) as usize];
        bits >> ((cell.x & 1) | (cell.y & 1) << 1 | (cell.z & 1) << 2) & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with(cells: &[IVec3]) -> World {
        let mut palette = [[0u8; 4]; 256];
        palette[1] = [200, 200, 200, 255];
        palette[2] = [100, 150, 255, 120];
        let mut w = World::new(IVec3::new(100, 70, 40), palette);
        for &c in cells {
            w.set(c, 1);
        }
        w
    }

    /// The volume holds the world's opaque voxels, not its glass, and every
    /// coarser copy has a bit where a child holds anything; a chunk written
    /// later is made anew with its blocks, the rest as it was, the same as
    /// made anew.
    #[test]
    fn the_volume_holds_the_levels_solid_voxels_and_takes_in_a_change() {
        let mut w = world_with(&[IVec3::new(3, 4, 5), IVec3::new(70, 60, 30)]);
        w.set(IVec3::new(10, 10, 10), 2);
        let bank = ModelBank::default();
        let reach = Reach::of(&[(Vec3::new(50.0, 35.0, 20.0), 200.0)], w.size);
        assert_eq!(reach, Reach { origin: IVec3::ZERO, texels: IVec3::new(64, 64, 32) }, "the world in whole blocks");
        let mut v = Volume::new(&w, reach, &bank, &[], 0);
        assert!(v.solid(IVec3::new(3, 4, 5), 0) && v.solid(IVec3::new(70, 60, 30), 0));
        assert!(!v.solid(IVec3::new(10, 10, 10), 0), "glass lets light through");
        assert!(!v.solid(IVec3::new(4, 4, 5), 0));
        assert!(v.solid(IVec3::new(0, 0, 0), 3) && v.solid(IVec3::new(64, 32, 0), LEVELS as i32 - 1), "the coarse copies hold what is in their cells");
        assert!(!v.solid(IVec3::new(64, 0, 0), LEVELS as i32 - 1));
        // The top copy: one bit for the 32-voxel cell round each voxel.
        let top = &v.copies[LEVELS as usize - 1];
        assert_eq!(top.iter().map(|b| b.count_ones()).sum::<u32>(), 2);
        assert!(v.update(&w).is_empty(), "nothing changed");
        w.set(IVec3::new(70, 60, 30), 0);
        w.set(IVec3::new(71, 61, 31), 1);
        let changed = v.update(&w);
        assert_eq!(changed, vec![(IVec3::new(BLOCK, 0, 0), IVec3::splat(BLOCK))], "the block round the chunk written");
        assert_eq!(v.moved, Some((IVec3::new(64, 32, 0), IVec3::new(96, 64, 32))), "what changed: the chunk written");
        assert!(!v.solid(IVec3::new(70, 60, 30), 0) && v.solid(IVec3::new(71, 61, 31), 0));
        assert!(v.solid(IVec3::new(3, 4, 5), 0), "the rest as it was");
        assert_eq!(v.copies, Volume::new(&w, reach, &bank, &[], 0).copies, "the same as made anew");
    }

    /// A volume holds only what its lamps reach, a block more each way:
    /// a lamp in a corner of a big level takes a corner's room; one
    /// reaching past it needs another.
    #[test]
    fn the_volume_holds_what_its_lamps_reach() {
        let size = IVec3::new(1280, 1200, 288);
        let lamp = [(Vec3::new(300.0, 400.0, 20.0), 60.0)];
        let reach = Reach::of(&lamp, size);
        assert_eq!(reach.origin, IVec3::new(128, 256, 0));
        assert_eq!(reach.texels * 2, IVec3::new(320, 320, 192));
        assert!(reach.holds(&lamp, size) && reach.holds(&[(Vec3::new(310.0, 400.0, 20.0), 60.0)], size));
        assert!(!reach.holds(&[(Vec3::new(600.0, 400.0, 20.0), 60.0)], size));
        assert!(reach.fits(2048).is_ok() && reach.bytes() < 3 << 20);
        let all = Reach::of(&[(Vec3::ZERO, 1.0), (size.as_vec3(), 1.0)], size);
        assert!(Reach::of(&[(Vec3::ZERO, 1.0), (Vec3::new(4000.0, 4000.0, 100.0), 1.0)], IVec3::new(4000, 4000, 100)).fits(4096).unwrap_err().contains("past 128 MB"), "the whole of a very big level is too much");
        assert!(all.fits(2048).is_ok(), "a big level whole fits");
    }

    /// The pieces' key is the bank's (which one, not how far it grew) and
    /// the pieces' draws: a model drawn the first time mid-level, put in
    /// the bank after the pieces' (`Session::bank_fresh`), makes no volume
    /// anew; another bank (a level entered) does.
    #[test]
    fn a_model_drawn_the_first_time_leaves_the_pieces_key() {
        let text = "format = \"voxcade voxel 1\"\nsize = [2, 1, 1]\n\n[palette]\na = \"#b04030\"\n\n[[frame]]\nlayers = \"\"\"\naa\n\"\"\"\n";
        let (mut piece, mut later) = (crate::vvx::read(text).unwrap(), crate::vvx::read(text).unwrap());
        let mut bank = ModelBank::default();
        bank.add(&mut piece);
        let draws = [ModelDraw { voxels: piece.bank[0].clone(), origin: Vec3::new(5.0, 5.0, 1.0), turn: 0, rot: glam::Quat::IDENTITY, shift: Vec3::ZERO, tint: None, nearer: 0.0, glow: None, shadow_only: false, outline: None, opacity: 1.0 }];
        let key = pieces_key(&bank, &draws);
        let mut grown = bank.clone();
        grown.add(&mut later);
        assert_eq!(pieces_key(&grown, &draws), key, "grown by a model drawn later");
        let mut anew = ModelBank::default();
        anew.add(&mut piece);
        assert_ne!(pieces_key(&anew, &draws), key, "another bank");
    }

    /// Pieces that move (a figure's parts posed anew every picture, the
    /// title's smoker lifting his hand) are taken in where they left and
    /// came to, the rest of the volume as it was: the same volume as made
    /// anew, its blocks of what changed only, a part turned freely too, and
    /// the box of the cells it left and came to said (`moved`: the lamps'
    /// answers whose ways pass far from it are kept); a piece moved back
    /// gives the first volume again. Another bank or another number of
    /// pieces is not taken in (made anew).
    #[test]
    fn pieces_that_move_are_taken_in_where_they_moved() {
        let text = "format = \"voxcade voxel 1\"\nsize = [2, 6, 5]\n\n[palette]\na = \"#b04030\"\n\n[[frame]]\nlayers = \"\"\"\naa *6\n*5\n\"\"\"\n";
        let mut piece = crate::vvx::read(text).unwrap();
        let mut bank = ModelBank::default();
        bank.add(&mut piece);
        let draw = |at: Vec3, rot: glam::Quat| ModelDraw { voxels: piece.bank[0].clone(), origin: at, turn: 0, rot, shift: Vec3::ZERO, tint: None, nearer: 0.0, glow: None, shadow_only: false, outline: None, opacity: 1.0 };
        let w = world_with(&[IVec3::new(3, 4, 5), IVec3::new(70, 60, 30)]);
        let reach = Reach::of(&[(Vec3::new(50.0, 35.0, 20.0), 200.0)], w.size);
        let still = draw(Vec3::new(80.0, 10.0, 2.0), glam::Quat::IDENTITY);
        let first = [still.clone(), draw(Vec3::new(10.0, 10.0, 2.0), glam::Quat::IDENTITY)];
        let mut v = Volume::new(&w, reach, &bank, &first, pieces_key(&bank, &first));
        assert!(v.solid(IVec3::new(10, 10, 2), 0) && v.solid(IVec3::new(80, 10, 2), 0));
        let made = v.copies.clone();
        assert_eq!(v.repose(&w, &bank, &first, pieces_key(&bank, &first)), Some(Vec::new()), "nothing moved: nothing changed");
        assert_eq!(v.moved, None);
        for second in [draw(Vec3::new(12.0, 11.0, 3.0), glam::Quat::IDENTITY), draw(Vec3::new(12.0, 11.0, 3.0), glam::Quat::from_rotation_z(0.3) * glam::Quat::from_rotation_x(-0.4))] {
            let moved = [still.clone(), second];
            let changed = v.repose(&w, &bank, &moved, pieces_key(&bank, &moved)).expect("the same pieces moved");
            assert_eq!(changed, vec![(IVec3::ZERO, IVec3::splat(BLOCK))], "the block of the chunks it left and came to");
            assert_eq!(v.copies, Volume::new(&w, reach, &bank, &moved, 0).copies, "the same as made anew");
            assert_eq!(v.pieces_key, pieces_key(&bank, &moved));
            assert!(v.solid(IVec3::new(80, 10, 2), 0), "the piece that stood still as it was");
            let (lo, hi) = v.moved.expect("what moved");
            let (from, to) = (lo.cmple(IVec3::new(11, 8, 3)).all() && hi.cmpge(IVec3::new(13, 14, 8)).all(), (hi - lo).cmple(IVec3::splat(12)).all());
            assert!(from && to, "the cells it left and came to, no more: {lo} to {hi}");
            if moved[1].rot == glam::Quat::IDENTITY {
                assert_eq!((lo, hi), (IVec3::new(9, 7, 2), IVec3::new(13, 14, 8)), "on the grid: the box of both (the pivot in the middle of its bottom)");
            }
        }
        v.repose(&w, &bank, &first, pieces_key(&bank, &first)).expect("moved back");
        assert_eq!(v.copies, made, "moved back: the volume it was");
        assert_eq!(v.repose(&w, &bank, &first[..1], 0), None, "fewer pieces");
        let mut anew = ModelBank::default();
        anew.add(&mut piece);
        assert_eq!(v.repose(&w, &anew, &first, 0), None, "another bank");
    }

    /// A lamp walks from where it is in the open; in solid voxels from the
    /// middle of the nearest open voxel (a lantern in its housing, a lamp
    /// flush with a wall's face on its own side); buried past `HOUSING`,
    /// from nowhere.
    #[test]
    fn a_lamp_in_solid_voxels_walks_from_the_nearest_open_one() {
        let mut w = world_with(&[]);
        for y in 0..70 {
            for z in 0..40 {
                for x in 50..54 {
                    w.set(IVec3::new(x, y, z), 1);
                }
            }
        }
        // A lantern round (20, 20, 20): a solid box of 3.
        for x in 19..22 {
            for y in 19..22 {
                for z in 19..22 {
                    w.set(IVec3::new(x, y, z), 1);
                }
            }
        }
        // A block 20 thick round (80, 35, 20).
        for x in 70..90 {
            for y in 25..45 {
                for z in 10..30 {
                    w.set(IVec3::new(x, y, z), 1);
                }
            }
        }
        let v = Volume::new(&w, Reach::of(&[(Vec3::new(50.0, 35.0, 20.0), 80.0)], w.size), &ModelBank::default(), &[], 0);
        assert_eq!(v.start(Vec3::new(30.2, 20.7, 6.1)), Some(Vec3::new(30.2, 20.7, 6.1)), "in the open: itself");
        assert_eq!(v.start(Vec3::new(20.5, 20.5, 20.5)), Some(Vec3::new(20.5, 20.5, 18.5)), "the lantern: out of its housing, the first of the nearest");
        assert_eq!(v.start(Vec3::new(50.0, 30.5, 12.5)), Some(Vec3::new(49.5, 30.5, 12.5)), "flush with the wall's face: its own side");
        assert_eq!(v.start(Vec3::new(51.8, 30.5, 12.5)), Some(Vec3::new(49.5, 30.5, 12.5)), "in the wall, nearer this side");
        assert_eq!(v.start(Vec3::new(80.5, 35.5, 20.5)), None, "buried");
    }
}
