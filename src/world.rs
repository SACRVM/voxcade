//! The voxel world: a sparse map of 32³ chunks holding palette indices.
//! Index 0 is empty, 1..=255 index into the world palette.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

use glam::{IVec3, Vec3};

/// A fast hash for grid cells (FxHash's mix): the standard one resists
/// attacks the engine does not face and costs most of a lookup, and its
/// order changes from run to run. With this one a map of cells is quick
/// and walks in the same order every run.
#[derive(Default, Clone, Copy)]
pub struct CellHasher(u64);

impl CellHasher {
    #[inline]
    fn add(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for CellHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.add(b as u64);
        }
    }

    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.add(i as u32 as u64);
    }
}

/// Maps and sets of grid cells (`CellHasher`).
pub type CellMap<V> = HashMap<IVec3, V, BuildHasherDefault<CellHasher>>;
pub type CellSet = HashSet<IVec3, BuildHasherDefault<CellHasher>>;

/// A number in [0, 1) for a cell and a seed, the same on every machine: a
/// map's speckle, the grain of big letters.
pub fn cell_hash(p: IVec3, seed: u32) -> f32 {
    let mut h = (p.x as u32).wrapping_mul(0x8da6_b343) ^ (p.y as u32).wrapping_mul(0xd816_3841) ^ (p.z as u32).wrapping_mul(0xcb1a_b31f) ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

pub const CHUNK: i32 = 32;
pub const CHUNK_VOLUME: usize = (CHUNK * CHUNK * CHUNK) as usize;

/// The most voxels a world made from a file may be each way (65536).
pub const MOST_SIDE: i64 = 1 << 16;
/// The most columns (across times deep) a world made from a file may
/// have: 4096 by 4096. More Blood's map is 640 by 2368, 1.5 million.
pub const MOST_COLUMNS: i64 = 1 << 24;
/// The most voxels a world made from a file may hold, empty ones counted:
/// about a billion. More Blood's map is 121 million.
pub const MOST_WORLD_VOXELS: i64 = 1 << 30;

/// The mistake for a world `size` voxels big that a file asks for (a map,
/// a level, a world or a prefab), told before anything that big is made:
/// None when it is within `MOST_SIDE`, `MOST_COLUMNS` and
/// `MOST_WORLD_VOXELS`. In whole numbers that cannot overflow.
pub fn too_big(size: glam::I64Vec3) -> Option<String> {
    let (columns, voxels) = (size.x.saturating_mul(size.y), size.x.saturating_mul(size.y).saturating_mul(size.z));
    let said = format!("{} by {} by {} voxels", size.x, size.y, size.z);
    if size.max_element() > MOST_SIDE {
        Some(format!("{said} is too big: {MOST_SIDE} voxels each way at most"))
    } else if columns > MOST_COLUMNS {
        Some(format!("{said} is too big: {columns} columns, {MOST_COLUMNS} at most (4096 by 4096)"))
    } else if voxels > MOST_WORLD_VOXELS {
        Some(format!("{said} is too big: {voxels} voxels, {MOST_WORLD_VOXELS} at most"))
    } else {
        None
    }
}

#[derive(Clone)]
pub struct Chunk {
    pub voxels: [u8; CHUNK_VOLUME],
    filled: u32,
}

impl Chunk {
    fn new() -> Arc<Self> {
        Arc::new(Self { voxels: [0; CHUNK_VOLUME], filled: 0 })
    }
}

/// Index of a chunk-local position (each axis 0..CHUNK).
#[inline]
pub fn local_index(l: IVec3) -> usize {
    (l.x + l.y * CHUNK + l.z * CHUNK * CHUNK) as usize
}

/// A marker in a level: a thing of a kind (`c.things`) at a cell, which
/// the game places there. `kind` is the kind's place in the game's list of
/// kind names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    pub kind: usize,
    pub cell: IVec3,
}

/// What a cell is to a body moving through it (`World::hold`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Free,
    Solid,
    /// A one-way voxel: it holds a body coming down onto it from above.
    Above,
}

/// The solid pieces a level sets down (`solid = true`): the cells their
/// models' voxels fill, which hold bodies up and stop them, bullets and
/// debris too, though the world's voxels there stay empty (the model draws
/// them, and the world's palette stays the game's). By each piece's place
/// among the level's pieces; a piece the game takes away (`thing:remove`)
/// is lifted at the tick's end, its cells free again. A blast does not
/// blow them away, and they hold up no voxels of the world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Solids {
    /// Every cell a piece fills, and how many pieces fill it.
    cells: CellMap<u16>,
    /// Each solid piece's cells, by its place among the level's pieces.
    of: std::collections::BTreeMap<usize, Arc<Vec<IVec3>>>,
    /// The pieces taken away, by their places.
    lifted: std::collections::BTreeSet<usize>,
}

impl Solids {
    /// The pieces' cells, each by its place among the level's pieces.
    pub fn new(pieces: impl IntoIterator<Item = (usize, Arc<Vec<IVec3>>)>) -> Self {
        let mut out = Self::default();
        for (i, cells) in pieces {
            for c in cells.iter() {
                *out.cells.entry(*c).or_insert(0) += 1;
            }
            out.of.insert(i, cells);
        }
        out
    }

    /// Whether a solid piece fills the cell.
    #[inline]
    pub fn holds(&self, p: IVec3) -> bool {
        self.cells.contains_key(&p)
    }

    /// The pieces taken away so far, in order.
    pub fn lifted(&self) -> impl Iterator<Item = usize> + '_ {
        self.lifted.iter().copied()
    }

    /// How many cells the pieces fill.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Takes the piece at place `i` away: its cells are free where no other
    /// piece fills them. Whether it was there.
    fn lift(&mut self, i: usize) -> bool {
        let Some(cells) = self.of.get(&i).cloned().filter(|_| !self.lifted.contains(&i)) else { return false };
        for c in cells.iter() {
            if let Some(n) = self.cells.get_mut(c) {
                *n -= 1;
                if *n == 0 {
                    self.cells.remove(c);
                }
            }
        }
        self.lifted.insert(i);
        true
    }
}

/// A copy of a world is cheap: the copies share their chunks until one of
/// them writes to a chunk, which then becomes its own (`Arc::make_mut`).
/// So a game's whole state can be kept and brought back (`Session::snapshot`).
#[derive(Clone)]
pub struct World {
    /// Extent in voxels; valid positions are 0..size on each axis.
    pub size: IVec3,
    /// RGBA per palette index. Entry 0 is unused (empty).
    pub palette: [[u8; 4]; 256],
    /// Palette indices that are soft: grass and shrubs you see and can blow
    /// away, but that bodies, bullets, grenades and debris pass through.
    pub soft: [bool; 256],
    /// Palette indices that hold only from above: planks a body jumps
    /// through from below and stands on (`sim::move_body_in`); solid for
    /// everything else.
    pub oneway: [bool; 256],
    /// Where the game is played, from corner to corner (one excluded): a
    /// side view's level without what lies far behind it (a tile map's
    /// lane). The camera stays within it (`c.camera.stop_at_edges`) and
    /// fits its shadows to it. None: the whole world.
    pub stage: Option<(IVec3, IVec3)>,
    /// A level's cells by what they are, for the game's rules (`grid`).
    pub cells: Option<Arc<crate::grid::CellKinds>>,
    /// The cells its solid pieces fill (`Solids`); None without any, the
    /// cost of a lookup only where a level has them.
    pub pieces: Option<Arc<Solids>>,
    chunks: CellMap<Arc<Chunk>>,
    dirty: CellSet,
    /// Every voxel written since the world was made, as one number: each
    /// write takes out a hash of the cell with its old value and puts in one
    /// with the new, so the order of the writes does not matter
    /// (`Game::state_hash`; the world as loaded is the game's data).
    changes: u64,
}

impl World {
    pub fn new(size: IVec3, palette: [[u8; 4]; 256]) -> Self {
        Self { size, palette, soft: [false; 256], oneway: [false; 256], stage: None, cells: None, pieces: None, chunks: CellMap::default(), dirty: CellSet::default(), changes: 0 }
    }

    /// The palette index of a colour (opaque) that is solid, neither soft
    /// nor one-way (a wall set by colour must hold), added to the palette
    /// if no such entry is there yet; None when the palette is full. The
    /// numbers already in the palette stay as they are.
    pub fn colour(&mut self, rgb: [u8; 3]) -> Option<u8> {
        self.colour_as(rgb, false, false)
    }

    /// The palette index of a colour that is also soft or one-way as asked
    /// (a colour may be in the palette once for each), added if it is not
    /// there yet; None when the palette is full.
    pub fn colour_as(&mut self, rgb: [u8; 3], soft: bool, oneway: bool) -> Option<u8> {
        let want = [rgb[0], rgb[1], rgb[2], 255];
        if let Some(i) = (1..256).find(|&i| self.palette[i] == want && self.soft[i] == soft && self.oneway[i] == oneway) {
            return Some(i as u8);
        }
        let free = (1..256).find(|&i| self.palette[i] == [0; 4])?;
        self.palette[free] = want;
        self.soft[free] = soft;
        self.oneway[free] = oneway;
        Some(free as u8)
    }

    /// Every voxel written since the world was made (`changes`).
    pub fn changes(&self) -> u64 {
        self.changes
    }

    pub fn chunk_of(p: IVec3) -> IVec3 {
        p.div_euclid(IVec3::splat(CHUNK))
    }

    /// Reads voxels one after another, looking their chunk up only when the
    /// next one lies in another (neighbours mostly share one).
    pub fn reader(&self) -> Reader<'_> {
        Reader { world: self, key: IVec3::MAX, chunk: None }
    }

    pub fn in_bounds(&self, p: IVec3) -> bool {
        p.cmpge(IVec3::ZERO).all() && p.cmplt(self.size).all()
    }

    pub fn get(&self, p: IVec3) -> u8 {
        if !self.in_bounds(p) {
            return 0;
        }
        match self.chunks.get(&Self::chunk_of(p)) {
            Some(chunk) => chunk.voxels[local_index(p.rem_euclid(IVec3::splat(CHUNK)))],
            None => 0,
        }
    }

    /// Whether a voxel inside the world is there and not soft, or a solid
    /// piece fills the cell.
    pub fn solid(&self, p: IVec3) -> bool {
        self.voxel_solid(p) || self.piece_at(p)
    }

    /// Whether the world's own voxel there is there and not soft (what a
    /// blast can take; a solid piece's cell is not one).
    pub fn voxel_solid(&self, p: IVec3) -> bool {
        let v = self.get(p);
        v != 0 && !self.soft[v as usize]
    }

    /// Whether a solid piece fills the cell (`Solids`).
    #[inline]
    pub fn piece_at(&self, p: IVec3) -> bool {
        self.pieces.as_ref().is_some_and(|s| s.holds(p))
    }

    /// Takes the solid piece at place `i` among the level's pieces away
    /// (the game removed it): its cells free. Whether it was there.
    pub fn lift_piece(&mut self, i: usize) -> bool {
        match &mut self.pieces {
            Some(s) if s.of.contains_key(&i) && !s.lifted.contains(&i) => Arc::make_mut(s).lift(i),
            _ => false,
        }
    }

    /// Solid for anything that moves: the ground below the world and its
    /// sides block, the sky above is open, soft voxels let everything through.
    pub fn blocks(&self, p: IVec3) -> bool {
        if p.z < 0 || p.x < 0 || p.y < 0 || p.x >= self.size.x || p.y >= self.size.y {
            return true;
        }
        self.solid(p)
    }

    /// The stage's corners (`stage`), the whole world without one.
    pub fn stage_box(&self) -> (Vec3, Vec3) {
        match self.stage {
            Some((lo, hi)) => (lo.as_vec3(), hi.as_vec3()),
            None => (Vec3::ZERO, self.size.as_vec3()),
        }
    }

    /// What a cell is to a body moving through it: free, solid, or a
    /// one-way voxel, held only from above. The ground below the world and
    /// its sides are solid, the sky above is open (as `blocks`).
    pub fn hold(&self, p: IVec3) -> Hold {
        if p.z < 0 || p.x < 0 || p.y < 0 || p.x >= self.size.x || p.y >= self.size.y {
            return Hold::Solid;
        }
        let v = self.get(p) as usize;
        if self.piece_at(p) {
            Hold::Solid
        } else if v == 0 || self.soft[v] {
            Hold::Free
        } else if self.oneway[v] {
            Hold::Above
        } else {
            Hold::Solid
        }
    }

    /// Height of the first free voxel above the highest solid one in a
    /// column (soft voxels do not count: you stand in the grass).
    pub fn column_top(&self, x: i32, y: i32) -> i32 {
        (0..self.size.z)
            .rev()
            .find(|&z| self.solid(IVec3::new(x, y, z)))
            .map_or(0, |z| z + 1)
    }

    /// Changes a voxel and marks every chunk whose mesh depends on it.
    pub fn set(&mut self, p: IVec3, v: u8) {
        if !self.set_raw(p, v) {
            return;
        }
        // Faces and ambient occlusion reach one voxel into neighbouring
        // chunks, so a voxel on a chunk border dirties those as well.
        let l = p.rem_euclid(IVec3::splat(CHUNK));
        let span = |c: i32| match c {
            0 => -1..=0,
            c if c == CHUNK - 1 => 0..=1,
            _ => 0..=0,
        };
        let cc = Self::chunk_of(p);
        for dz in span(l.z) {
            for dy in span(l.y) {
                for dx in span(l.x) {
                    self.dirty.insert(cc + IVec3::new(dx, dy, dz));
                }
            }
        }
    }

    /// Changes a voxel without marking anything dirty (bulk loading).
    /// Returns whether the voxel changed.
    pub fn set_raw(&mut self, p: IVec3, v: u8) -> bool {
        if !self.in_bounds(p) {
            return false;
        }
        let cc = Self::chunk_of(p);
        let shared = match self.chunks.entry(cc) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(_) if v == 0 => return false,
            Entry::Vacant(e) => e.insert(Chunk::new()),
        };
        let li = local_index(p.rem_euclid(IVec3::splat(CHUNK)));
        let old = shared.voxels[li];
        if old == v {
            return false;
        }
        // A chunk a copy of the world still shares becomes this world's own.
        let chunk = Arc::make_mut(shared);
        chunk.voxels[li] = v;
        let cell = |v: u8| crate::hash::Stable::default().i32(p.x).i32(p.y).i32(p.z).u64(v as u64).finish();
        self.changes ^= cell(old) ^ cell(v);
        match (old == 0, v == 0) {
            (true, false) => chunk.filled += 1,
            (false, true) => chunk.filled -= 1,
            _ => {}
        }
        if chunk.filled == 0 {
            self.chunks.remove(&cc);
        }
        true
    }

    /// Sets every voxel of chunk `cc` to `f(world position)`, at once and
    /// without marking anything dirty (bulk loading). Positions outside the
    /// world stay empty.
    pub fn fill_chunk(&mut self, cc: IVec3, mut f: impl FnMut(IVec3) -> u8) {
        let base = cc * CHUNK;
        let size = self.size;
        let chunk = Arc::make_mut(self.chunks.entry(cc).or_insert_with(Chunk::new));
        let mut filled = 0;
        for z in 0..CHUNK {
            for y in 0..CHUNK {
                for x in 0..CHUNK {
                    let l = IVec3::new(x, y, z);
                    let p = base + l;
                    let inside = p.cmpge(IVec3::ZERO).all() && p.cmplt(size).all();
                    let v = if inside { f(p) } else { 0 };
                    chunk.voxels[local_index(l)] = v;
                    filled += (v != 0) as u32;
                }
            }
        }
        chunk.filled = filled;
        if filled == 0 {
            self.chunks.remove(&cc);
        }
    }

    pub fn chunk(&self, cc: IVec3) -> Option<&Chunk> {
        self.chunks.get(&cc).map(|c| &**c)
    }

    /// The whole world as one number: its size, palette, soft and one-way
    /// colours, stage and every chunk's voxels, the chunks in the order of
    /// their sorted keys (the map's own order is not the same from run to
    /// run). Read-only; how a level begins, for `Session::start_mark`.
    pub fn content_hash(&self) -> u64 {
        let mut h = crate::hash::Stable::default();
        h.i32(self.size.x).i32(self.size.y).i32(self.size.z);
        for c in &self.palette {
            h.bytes(c);
        }
        for i in 0..256 {
            h.u64(self.soft[i] as u64 | (self.oneway[i] as u64) << 1);
        }
        match self.stage {
            Some((a, b)) => h.u64(1).i32(a.x).i32(a.y).i32(a.z).i32(b.x).i32(b.y).i32(b.z),
            None => h.u64(0),
        };
        let mut keys = self.chunk_keys();
        keys.sort_by_key(|k| (k.z, k.y, k.x));
        for cc in keys {
            h.i32(cc.x).i32(cc.y).i32(cc.z).bytes(&self.chunks[&cc].voxels);
        }
        // Solid pieces' cells, where a level has them (others as before).
        if let Some(p) = &self.pieces {
            for (i, cells) in &p.of {
                h.u64(*i as u64).u64(p.lifted.contains(i) as u64);
                for c in cells.iter() {
                    h.i32(c.x).i32(c.y).i32(c.z);
                }
            }
        }
        h.finish()
    }

    pub fn chunk_keys(&self) -> Vec<IVec3> {
        self.chunks.keys().copied().collect()
    }

    pub fn mark_all_dirty(&mut self) {
        let keys = self.chunk_keys();
        self.dirty.extend(keys);
    }

    pub fn take_dirty(&mut self) -> Vec<IVec3> {
        self.dirty.drain().collect()
    }

    /// The voxels that differ from `base` (the world as its level's file
    /// made it), each cell with what it holds now, in the order z, y, x:
    /// what a save keeps of a level beside its file. Only chunks no longer
    /// shared with `base` are looked at.
    pub fn changed_from(&self, base: &World) -> Vec<(IVec3, u8)> {
        let mut chunks: Vec<IVec3> = self.chunks.iter().filter(|(cc, c)| base.chunks.get(*cc).is_none_or(|b| !Arc::ptr_eq(c, b))).map(|(cc, _)| *cc).collect();
        chunks.extend(base.chunks.keys().filter(|cc| !self.chunks.contains_key(*cc)));
        chunks.sort_unstable_by_key(|c| (c.z, c.y, c.x));
        chunks.dedup();
        let mut changed = Vec::new();
        for cc in chunks {
            for z in 0..CHUNK {
                for y in 0..CHUNK {
                    for x in 0..CHUNK {
                        let p = cc * CHUNK + IVec3::new(x, y, z);
                        let now = self.get(p);
                        if now != base.get(p) {
                            changed.push((p, now));
                        }
                    }
                }
            }
        }
        changed.sort_unstable_by_key(|(p, _)| (p.z, p.y, p.x));
        changed
    }

    /// After this world was brought back in place of `shown` (the one whose
    /// chunks the renderer has meshed): marks dirty what `shown` still had
    /// to mesh and every chunk that differs between the two, with its
    /// neighbours (faces and shading reach one voxel into them). Chunks the
    /// two still share are the same and stay as they are.
    pub fn mark_changed_since(&mut self, shown: &World) {
        // What waited to be drawn anew still waits (its neighbours were
        // marked with it); only the chunks changed since take theirs.
        self.dirty.extend(shown.dirty.iter().copied());
        let mut changed: Vec<IVec3> = Vec::new();
        for (cc, chunk) in &self.chunks {
            if shown.chunks.get(cc).is_none_or(|other| !Arc::ptr_eq(chunk, other)) {
                changed.push(*cc);
            }
        }
        changed.extend(shown.chunks.keys().filter(|cc| !self.chunks.contains_key(*cc)));
        changed.sort_unstable_by_key(|c| (c.z, c.y, c.x));
        for cc in changed {
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        self.dirty.insert(cc + IVec3::new(dx, dy, dz));
                    }
                }
            }
        }
    }
}

/// See `World::reader`.
pub struct Reader<'a> {
    world: &'a World,
    key: IVec3,
    chunk: Option<&'a Chunk>,
}

impl Reader<'_> {
    /// As `World::get`.
    #[inline]
    pub fn get(&mut self, p: IVec3) -> u8 {
        if !self.world.in_bounds(p) {
            return 0;
        }
        let cc = World::chunk_of(p);
        if cc != self.key {
            self.key = cc;
            self.chunk = self.world.chunks.get(&cc).map(|c| &**c);
        }
        self.chunk.map_or(0, |c| c.voxels[local_index(p.rem_euclid(IVec3::splat(CHUNK)))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A colour set from a script is solid even where the palette holds the
    /// same colour soft or one-way: a wall set by colour must hold.
    #[test]
    fn a_colour_set_from_lua_is_solid() {
        let mut palette = [[0; 4]; 256];
        palette[1] = [10, 20, 30, 255];
        palette[2] = [10, 20, 30, 255];
        palette[3] = [10, 20, 30, 255];
        let mut world = World::new(IVec3::new(8, 8, 8), palette);
        world.soft[1] = true;
        world.oneway[2] = true;
        let i = world.colour([10, 20, 30]).unwrap();
        assert_eq!(i, 3, "the solid entry, not the soft or the one-way one");
        world.set(IVec3::new(1, 1, 1), i);
        assert!(world.solid(IVec3::new(1, 1, 1)));
        // With only soft ones there, a new solid entry is made.
        let mut only_soft = [[0; 4]; 256];
        only_soft[1] = [1, 2, 3, 255];
        let mut world = World::new(IVec3::new(8, 8, 8), only_soft);
        world.soft[1] = true;
        let i = world.colour([1, 2, 3]).unwrap();
        assert_eq!(i, 2);
        assert!(!world.soft[i as usize]);
    }

    /// A copy shares every chunk until one side writes; brought back in
    /// place of the other, exactly the written chunk and its neighbours are
    /// meshed anew.
    #[test]
    fn a_snapshot_shares_unchanged_chunks() {
        let mut world = World::new(IVec3::new(128, 128, 64), [[0; 4]; 256]);
        for y in 0..128 {
            for x in 0..128 {
                world.set_raw(IVec3::new(x, y, 0), 1);
            }
        }
        let kept = world.clone();
        assert!(world.chunks.iter().all(|(cc, c)| Arc::ptr_eq(c, &kept.chunks[cc])));
        world.set(IVec3::new(40, 40, 0), 0);
        world.take_dirty();
        let unshared: Vec<IVec3> = world.chunks.iter().filter(|(cc, c)| !Arc::ptr_eq(c, &kept.chunks[*cc])).map(|(cc, _)| *cc).collect();
        assert_eq!(unshared, [IVec3::new(1, 1, 0)], "only the chunk written to is copied");
        assert_eq!(kept.get(IVec3::new(40, 40, 0)), 1, "the copy changed with the world");

        let mut back = kept.clone();
        back.mark_changed_since(&world);
        let mut dirty = back.take_dirty();
        dirty.sort_unstable_by_key(|c| (c.z, c.y, c.x));
        let mut around = Vec::new();
        for z in -1..=1 {
            for y in 0..=2 {
                for x in 0..=2 {
                    around.push(IVec3::new(x, y, z));
                }
            }
        }
        assert_eq!(dirty, around);
    }

    /// What replays, saves and the network rely on: the world's number and
    /// its count of changes do not depend on the order the voxels were set
    /// in; a voxel set back is no change; what differs from the level's
    /// world is listed in the order z, y, x.
    #[test]
    fn the_worlds_number_and_changes_ignore_the_order() {
        let cells: Vec<(IVec3, u8)> = (0..200).map(|i| (IVec3::new(i * 7 % 70, i * 13 % 50, i % 40), (i % 5 + 1) as u8)).collect();
        let fresh = || World::new(IVec3::new(70, 50, 40), [[9; 4]; 256]);
        let (mut a, mut b) = (fresh(), fresh());
        let mut seen = crate::world::CellSet::default();
        let cells: Vec<_> = cells.into_iter().filter(|(p, _)| seen.insert(*p)).collect();
        for &(p, v) in &cells {
            a.set(p, v);
        }
        for &(p, v) in cells.iter().rev() {
            b.set_raw(p, v);
        }
        assert_eq!(a.content_hash(), b.content_hash());
        assert_eq!(a.changes(), b.changes());
        let base = a.clone();
        let (p, v) = cells[3];
        a.set(p, 0);
        assert_ne!(a.changes(), base.changes());
        assert_eq!(a.changed_from(&base), [(p, 0)]);
        a.set(p, v);
        assert_eq!(a.changes(), base.changes(), "set back, no change");
        assert_eq!(a.content_hash(), base.content_hash());
        assert!(a.changed_from(&base).is_empty());
        // Outside the world nothing is written, and nothing counts.
        assert!(!a.set_raw(IVec3::new(-1, 0, 0), 3) && !a.set_raw(IVec3::new(0, 0, 40), 3));
        assert_eq!(a.changes(), base.changes());
    }

    /// A voxel on a chunk's border marks the neighbours its faces and
    /// shading reach; the ground below and the sides block, the sky is
    /// open; soft voxels let through, one-way ones hold from above.
    #[test]
    fn borders_mark_their_neighbours_and_the_edges_hold() {
        let mut world = World::new(IVec3::new(64, 64, 64), [[0; 4]; 256]);
        world.set(IVec3::new(CHUNK - 1, 5, 0), 1);
        let mut dirty = world.take_dirty();
        dirty.sort_unstable_by_key(|c| (c.z, c.y, c.x));
        assert_eq!(dirty, [IVec3::new(0, 0, -1), IVec3::new(1, 0, -1), IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]);
        world.set(IVec3::new(10, 10, 10), 0);
        assert!(world.take_dirty().is_empty(), "nothing changed, nothing to mesh");
        assert!(world.blocks(IVec3::new(5, 5, -1)) && world.blocks(IVec3::new(-1, 5, 5)) && world.blocks(IVec3::new(5, 64, 5)));
        assert!(!world.blocks(IVec3::new(5, 5, 64)), "the sky is open");
        let soft = world.colour_as([1, 2, 3], true, false).unwrap();
        let oneway = world.colour_as([1, 2, 3], false, true).unwrap();
        world.set(IVec3::new(3, 3, 3), soft);
        world.set(IVec3::new(4, 4, 4), oneway);
        assert!(!world.blocks(IVec3::new(3, 3, 3)) && world.get(IVec3::new(3, 3, 3)) == soft);
        assert_eq!((world.hold(IVec3::new(3, 3, 3)), world.hold(IVec3::new(4, 4, 4)), world.hold(IVec3::new(5, 5, 5))), (Hold::Free, Hold::Above, Hold::Free));
        assert_eq!(world.column_top(3, 3), 0, "one stands in the grass");
        assert_eq!(world.column_top(4, 4), 5);
    }
}
