//! The world the view shows (`Shown`): the level, the block, the model's
//! frame or the figure's part chosen, kept in step with the editor voxel
//! by voxel; the room round a model shaped in place; see-through cubes far
//! to near.

use super::*;

/// The world the 3D view draws: the level as built, or the open block;
/// brought up to the editor by building again only the cells that changed.
pub struct Shown {
    pub world: World,
    scope: Scope,
    size: IVec3,
    cells: Vec<Cell>,
    blocks: Vec<(Block, Kind)>,
    /// The block or frame shown, as built, and the slice it was cut by
    /// (its axis, layer and side).
    shape: Option<Block>,
    slice: Option<(usize, i32, i32)>,
    /// The part shown is turned at rest (drawn as cubes, the world empty).
    turned: bool,
    /// The level's terrain as built, and its columns' heights.
    terrain: Option<crate::terrain::Terrain>,
    surface: Option<crate::terrain::Surface>,
    /// The level round a model shaped in place, and the selection's
    /// outline, kept from frame to frame (`frame`): made again only when
    /// what they are made of changes. A room of big pieces joined on every
    /// frame took a fifth of a second (the owner's back wall: "ruckelig
    /// wie sau").
    room: std::cell::RefCell<RoomCache>,
    pub(super) outline: std::cell::RefCell<crate::editor::model::selection::Outlines>,
}

/// The room round a model shaped in place as `frame` draws it: what it was
/// made of, its cubes and the body they make, joined as the game joins
/// glass (`render::join_see_through`) for the side the eye was last on,
/// and sorted far to near for the eye it was last drawn for.
#[derive(Default)]
pub(super) struct RoomCache {
    made_of: Option<RoomOf>,
    cubes: Vec<Instance>,
    body: crate::render::SeeThroughBody,
    joined_for: Option<([usize; 3], [bool; 3])>,
    joined: Vec<Instance>,
    sorted_for: Option<Vec3>,
    sorted: Vec<Instance>,
    /// How often it was made, and joined (for tests).
    made: usize,
    joins: usize,
}

/// What the room's cubes depend on: the level round the model (when
/// shown), the models loaded (they only ever come, never change), the
/// figure's box and the grid they are drawn in.
#[derive(Clone, PartialEq)]
pub(super) struct RoomOf {
    context: Option<crate::editor::context::Context>,
    models: usize,
    figure_box: Option<(IVec3, IVec3)>,
    seen: [f32; 16],
}

/// Sorts cubes far from `eye` to near, as a stable `sort_by` on their
/// distance would (`f32::total_cmp`'s order, equal ones in the order
/// given), each distance worked out once and the cubes sorted by it in
/// three passes of a radix sort: a big room sorted by comparing its cubes
/// took more than a frame while the view turned.
pub(crate) fn far_to_near(cubes: &mut [Instance], eye: Vec3) {
    let order = far_to_near_order(cubes, eye);
    let sorted: Vec<Instance> = order.iter().map(|&k| cubes[k as usize]).collect();
    cubes.copy_from_slice(&sorted);
}

/// The order `far_to_near` puts `cubes` in (their places).
pub(super) fn far_to_near_order(cubes: &[Instance], eye: Vec3) -> Vec<u32> {
    let keys: Vec<u32> = cubes
        .iter()
        .map(|i| {
            // `f32::total_cmp`'s order as a signed number, then unsigned.
            let bits = (-(Vec3::from_slice(&i.pos_size[..3]) - eye).length_squared()).to_bits() as i32;
            ((bits ^ (((bits >> 31) as u32) >> 1) as i32) as u32) ^ 0x8000_0000
        })
        .collect();
    let mut order: Vec<u32> = (0..cubes.len() as u32).collect();
    let mut next = vec![0; cubes.len()];
    // Least significant digits first, each pass stable: 11, 11, 10 bits.
    for shift in [0, 11, 22] {
        let digit = |k: u32| ((keys[k as usize] >> shift) & 0x7ff) as usize;
        let mut at = [0usize; 2048];
        for &k in &order {
            at[digit(k)] += 1;
        }
        let mut sum = 0;
        for slot in &mut at {
            (*slot, sum) = (sum, sum + *slot);
        }
        for &k in &order {
            let d = digit(k);
            next[at[d]] = k;
            at[d] += 1;
        }
        std::mem::swap(&mut order, &mut next);
    }
    order
}

/// What of the slice the world shown depends on.
pub(super) fn slice_key(ed: &Editor) -> Option<(usize, i32, i32)> {
    ed.slice.as_ref().filter(|_| ed.can_slice()).map(|s| (s.axis, s.at, s.side))
}

impl Shown {
    pub fn new(ed: &Editor) -> Result<Self> {
        let mut shown = Self { world: World::new(IVec3::ONE, [[0; 4]; 256]), scope: ed.scope, size: IVec3::ZERO, cells: Vec::new(), blocks: Vec::new(), shape: None, slice: None, turned: false, terrain: None, surface: None, room: Default::default(), outline: Default::default() };
        shown.build(ed)?;
        Ok(shown)
    }

    pub(super) fn build(&mut self, ed: &Editor) -> Result<()> {
        let listed = ed.level.blocks.len();
        self.world = match ed.scope {
            Scope::Level => {
                let models = |name: &str| crate::vox::load_model(&ed.dir.join(name));
                crate::grid::build(&ed.level, &ed.blocks[..listed], 0, 0, &[], &models)?
            }
            // A figure's part turned at rest is drawn as cubes where it
            // stands (`frame`), not as the world (which cannot turn).
            Scope::Frame(_) if ed.rest_view() != Mat4::IDENTITY => World::new(IVec3::ONE, [[0; 4]; 256]),
            Scope::Block(_) | Scope::Frame(_) => {
                let block = ed.shape().expect("a block or frame shown");
                let mut world = World::new(block.size, [[0; 4]; 256]);
                for (k, &v) in block.voxels.iter().enumerate() {
                    // (What the slice cuts away is not drawn.)
                    if v != 0 && !ed.cut(block.at(k)) {
                        let colour = world.colour(block.palette[v as usize - 1]).unwrap_or(1);
                        world.set_raw(block.at(k), colour);
                    }
                }
                world.mark_all_dirty();
                world
            }
            // The figure whole is drawn as cubes (`figure.rs`): its parts
            // dimmed or left out without building anything again.
            Scope::Figure => World::new(ed.grid(), [[0; 4]; 256]),
        };
        self.scope = ed.scope;
        self.shape = ed.shape().cloned();
        self.slice = slice_key(ed);
        self.turned = ed.rest_view() != Mat4::IDENTITY;
        self.size = ed.level.size;
        self.cells = ed.level.cells.clone();
        self.blocks = ed.level.blocks.iter().zip(&ed.blocks).map(|(e, b)| (b.clone(), e.kind)).collect();
        self.terrain = ed.level.terrain.clone().filter(|_| ed.scope == Scope::Level);
        self.surface = self.terrain.as_ref().map(crate::terrain::Terrain::surface);
        Ok(())
    }

    /// The terrain's points changed (their heights or shapes, nothing
    /// else, in a level without blocks): only the columns they reach built
    /// again, their chunks to mesh anew. False where it cannot be so.
    pub(super) fn update_land(&mut self, ed: &Editor) -> bool {
        let (Some(was), Some(now), Some(surface)) = (&self.terrain, &ed.level.terrain, &mut self.surface) else { return false };
        // (Baked scatter voxels are built whole.)
        if ed.level.has_grid()
            || now.scatter.iter().any(|r| r.makes() == crate::terrain::ScatterAs::Voxels)
            || was.scatter != now.scatter
            || (was.spacing, was.at, was.cols, was.rows, was.terrace, was.rough, was.seed) != (now.spacing, now.at, now.cols, now.rows, now.terrace, now.rough, now.seed)
            || (&was.under, &was.materials, &was.layers, was.roughness.is_some()) != (&now.under, &now.materials, &now.layers, now.roughness.is_some())
            || (was.dip, was.dip_to, was.fold, was.fault) != (now.dip, now.dip_to, now.fold, now.fault)
        {
            return false;
        }
        let rough = |t: &crate::terrain::Terrain, i: usize| t.roughness.as_ref().map_or(9, |r| r[i]);
        let changed: Vec<IVec2> = (0..now.heights.len()).filter(|&i| was.heights[i] != now.heights[i] || was.shapes[i] != now.shapes[i] || rough(was, i) != rough(now, i)).map(|i| IVec2::new(i as i32 % now.cols, i as i32 / now.cols)).collect();
        let Some(&first) = changed.first() else { return true };
        let (from, to) = changed.iter().fold((first, first), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        let old = surface.clone();
        let (lo, hi) = now.resurface(surface, from, to);
        // (A column's colours hang on its neighbours': one more round.)
        let (lo, hi) = ((lo - 1).max(IVec2::ZERO), (hi + 1).min(surface.size));
        let Ok(painter) = now.painter(&mut self.world, IVec3::ZERO, surface.clone()) else { return false };
        let corner = IVec3::from(now.at);
        for y in lo.y..hi.y {
            for x in lo.x..hi.x {
                let c = IVec2::new(x, y);
                let top = old.at(c).unwrap_or(0).max(surface.at(c).unwrap_or(0));
                for z in 0..corner.z + top + 1 {
                    let p = corner + IVec3::new(x, y, z - corner.z);
                    let v = painter.voxel(p);
                    if self.world.get(p) != v {
                        self.world.set(p, v);
                    }
                }
            }
        }
        self.terrain = Some(now.clone());
        true
    }

    /// The level round a model shaped in place (`Editor::context_cubes`),
    /// joined and sorted far to near from `eye` as `frame` draws it: made
    /// again only when what it is made of changes, sorted again only when
    /// the eye moves.
    pub(super) fn room(&self, ed: &Editor, figures: &Figures, seen: Mat4, eye: Vec3, grid: IVec3) -> Vec<Instance> {
        let toward = eye - grid.as_vec3() / 2.0;
        let context = ed.context.as_ref().filter(|c| c.shown);
        let (models, figure_box, seen_at, side) = (figures.models.len(), ed.figure_box(), seen.to_cols_array(), crate::render::see_through_side(toward));
        let mut cache = self.room.borrow_mut();
        let same = cache.made_of.as_ref().is_some_and(|m| m.context.as_ref() == context && m.models == models && m.figure_box == figure_box && m.seen == seen_at);
        if !same {
            let mut cubes = Vec::new();
            ed.context_cubes(figures, seen, &mut cubes);
            let body = crate::render::see_through_body(&cubes);
            let (made, joins) = (cache.made + 1, cache.joins);
            *cache = RoomCache { made_of: Some(RoomOf { context: context.cloned(), models, figure_box, seen: seen_at }), cubes, body, made, joins, ..RoomCache::default() };
        }
        if cache.joined_for != Some(side) {
            let mut joined = cache.cubes.clone();
            crate::render::join_by_body(&mut joined, &cache.body, toward);
            (cache.joined, cache.joined_for, cache.sorted_for) = (joined, Some(side), None);
            cache.joins += 1;
        }
        if cache.sorted_for != Some(eye) {
            let sorted = far_to_near_order(&cache.joined, eye).into_iter().map(|k| cache.joined[k as usize]).collect();
            (cache.sorted, cache.sorted_for) = (sorted, Some(eye));
        }
        cache.sorted.clone()
    }

    /// How often the room round a model was made, and joined (for tests).
    #[cfg(test)]
    pub(crate) fn rooms_made(&self) -> usize {
        self.room.borrow().made
    }

    #[cfg(test)]
    pub(crate) fn room_joins(&self) -> usize {
        self.room.borrow().joins
    }

    /// The block or frame shown changed but kept its size: the voxels whose
    /// colour changed set in the world as `build` would set them (what the
    /// slice cuts away left out), the rest untouched. False where it cannot
    /// be so (another size, a palette full of colours no longer used): the
    /// world is then built anew.
    pub(super) fn retouch(&mut self, ed: &Editor) -> bool {
        let (Some(was), Some(now)) = (&self.shape, ed.shape()) else { return false };
        if !matches!(ed.scope, Scope::Block(_) | Scope::Frame(_)) || was.size != now.size || self.world.size != now.size || was.voxels.len() != now.voxels.len() {
            return false;
        }
        let colour = |b: &Block, v: u8| (v != 0).then(|| b.palette[v as usize - 1]);
        let same_palette = was.palette == now.palette;
        for (k, (&a, &b)) in was.voxels.iter().zip(&now.voxels).enumerate() {
            if a == b && same_palette {
                continue;
            }
            let c = colour(now, b);
            if colour(was, a) == c {
                continue;
            }
            let at = now.at(k);
            if ed.cut(at) {
                continue;
            }
            let v = match c {
                Some(rgb) => match self.world.colour(rgb) {
                    Some(v) => v,
                    None => return false,
                },
                None => 0,
            };
            self.world.set(at, v);
        }
        self.shape = Some(now.clone());
        true
    }

    /// Brings the world up to the editor. True: it was built anew (every
    /// chunk to mesh again); else the changed chunks are marked dirty.
    pub fn update(&mut self, ed: &Editor) -> Result<bool> {
        let listed = ed.level.blocks.len();
        let same_blocks = |shown: &Self| shown.blocks.len() == listed && shown.blocks.iter().zip(ed.level.blocks.iter().zip(&ed.blocks)).all(|((b, k), (e, m))| *k == e.kind && b == m);
        match ed.scope {
            Scope::Block(_) | Scope::Frame(_) | Scope::Figure => {
                let same = self.scope == ed.scope && self.slice == slice_key(ed) && self.turned == (ed.rest_view() != Mat4::IDENTITY);
                if same && self.shape.as_ref() == ed.shape() && (ed.scope != Scope::Figure || self.world.size == ed.grid()) {
                    return Ok(false);
                }
                // A stroke (an undo, a step) on a block or a frame as big
                // as before: only the voxels it changed set again, their
                // chunks to mesh anew (a big model rebuilt whole on every
                // voxel of a stroke stuttered: the owner's back wall).
                if same && !self.turned && self.retouch(ed) {
                    return Ok(false);
                }
            }
            Scope::Level => {
                let same_land = self.terrain == ed.level.terrain;
                if self.scope == Scope::Level && self.size == ed.level.size && self.cells == ed.level.cells && same_blocks(self) {
                    if same_land || self.update_land(ed) {
                        return Ok(false);
                    }
                    self.build(ed)?;
                    return Ok(true);
                }
                if !same_land {
                    self.build(ed)?;
                    return Ok(true);
                }
                if self.scope == Scope::Level && self.size == ed.level.size && self.blocks.len() <= listed {
                    // Only the cells whose block, its kind or its voxels
                    // changed, stamped again.
                    let changed: Vec<bool> = (0..listed).map(|i| self.blocks.get(i).is_none_or(|(b, k)| *k != ed.level.blocks[i].kind || *b != ed.blocks[i])).collect();
                    let n = ed.level.n;
                    let mut stamped: BTreeMap<Cell, Vec<u8>> = BTreeMap::new();
                    for (i, (&now, &was)) in ed.level.cells.iter().zip(&self.cells).enumerate() {
                        if now == was && (now.block == 0 || !changed[now.block as usize - 1]) {
                            continue;
                        }
                        let voxels = match now.block {
                            0 => None,
                            b => {
                                if let std::collections::btree_map::Entry::Vacant(e) = stamped.entry(now) {
                                    let block = ed.blocks[b as usize - 1].turned(now.turn);
                                    let Some(v) = crate::grid::stamp(&mut self.world, &block, ed.level.blocks[b as usize - 1].kind) else {
                                        anyhow::bail!("the level has more than 255 colors");
                                    };
                                    e.insert(v);
                                }
                                stamped.get(&now)
                            }
                        };
                        let s = ed.level.size;
                        let corner = IVec3::new(i as i32 % s.x, i as i32 / s.x % s.y, i as i32 / (s.x * s.y)) * n;
                        for k in 0..n * n * n {
                            let v = voxels.map_or(0, |v| v[k as usize]);
                            self.world.set(corner + IVec3::new(k % n, k / n % n, k / (n * n)), v);
                        }
                    }
                    self.cells = ed.level.cells.clone();
                    self.blocks = ed.level.blocks.iter().zip(&ed.blocks).map(|(e, b)| (b.clone(), e.kind)).collect();
                    return Ok(false);
                }
            }
        }
        self.build(ed)?;
        Ok(true)
    }
}
