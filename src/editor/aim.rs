//! What a ray from the view meets among the level's things and pieces,
//! and where one would be set down.

use super::*;

/// Where a ray (in voxels) meets the level's things: the spot a thing
/// would stand on (on the voxels of `world`) and the nearest thing in
/// front of them along it.
pub fn things_along(ed: &Editor, world: &World, figures: &Figures, origin: Vec3, dir: Vec3) -> (Option<IVec3>, Option<usize>) {
    let hit = super::super::model::pick(origin, dir, world.size, |c| c.cmpge(IVec3::ZERO).all() && world.get(c) != 0);
    let spot = hit.and_then(|h| ed.thing_spot(h.cell, h.normal));
    let wall = hit.and_then(|h| ray_box(origin, dir, h.cell.as_vec3(), h.cell.as_vec3() + 1.0)).unwrap_or(f32::INFINITY);
    let under = ed
        .level
        .things
        .iter()
        .enumerate()
        .filter(|&(i, _)| Some(i) != ed.moving())
        .filter_map(|(i, t)| {
            let (lo, hi) = figures.bounds(&figures.key_in(t));
            let at = IVec3::from(t.at);
            ray_box(origin, dir, (at + lo).as_vec3(), (at + hi).as_vec3()).map(|d| (i, d))
        })
        .filter(|&(_, d)| d <= wall + 0.5)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i);
    (spot, under)
}

/// How far along a ray (from `origin` along `dir`, a unit) it enters the
/// box from `lo` to `hi` (0 if it starts inside), if it meets it ahead.
pub fn ray_box(origin: Vec3, dir: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
    let (mut enter, mut leave) = (0.0f32, f32::INFINITY);
    for a in 0..3 {
        if dir[a].abs() < 1e-9 {
            if origin[a] < lo[a] || origin[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((lo[a] - origin[a]) / dir[a], (hi[a] - origin[a]) / dir[a]);
        enter = enter.max(t0.min(t1));
        leave = leave.min(t0.max(t1));
    }
    (enter <= leave).then_some(enter)
}

/// The item a ray (in voxels) meets first: a piece or a thing of a kind
/// by the first of its voxels met (`pick_along`), a light alone or a place
/// by a small box round it; an area only where nothing else is met. What
/// is hidden is not met.
pub fn item_along(ed: &Editor, figures: &Figures, origin: Vec3, dir: Vec3) -> Option<Item> {
    pick_along(ed, figures, origin, dir).map(|(item, _)| item)
}

/// What a ray (in voxels) meets first among the pieces and things, as
/// `hit_along` finds it: the item, a piece's own light when the ray meets
/// that first (a small box round where it shines from: its place among the
/// lights).
pub fn pick_along(ed: &Editor, figures: &Figures, origin: Vec3, dir: Vec3) -> Option<(Item, Option<usize>)> {
    hit_along(ed, figures, origin, dir, true).map(|h| (h.item, h.light))
}

/// What a ray meets: the item, its light, how far along the ray, and the
/// way out of the face it met (in the level; zero for a box).
#[derive(Clone, Copy, Debug)]
pub struct Met {
    pub item: Item,
    pub light: Option<usize>,
    pub at: f32,
    pub normal: Vec3,
}

/// The first item a ray meets: a piece by the first of its model's
/// voxels the ray meets, walked in the piece's own turned frame (glass
/// counts), never by its box (a small piece in the hollow of a big one is
/// chosen where the picture shows it); a thing of a kind by its figure's
/// voxels; a light alone, a piece's light and a place by a small box; an
/// area only where nothing else is met (with `marks`: lights, places and
/// areas count). Hidden ones are left out.
pub fn hit_along(ed: &Editor, figures: &Figures, origin: Vec3, dir: Vec3, marks: bool) -> Option<Met> {
    let mut best: Option<Met> = None;
    fn take(best: &mut Option<Met>, d: Option<(f32, Vec3)>, item: Item, light: Option<usize>) {
        if let Some((d, normal)) = d
            && best.is_none_or(|b| d < b.at)
        {
            *best = Some(Met { item, light, at: d, normal });
        }
    }
    let boxed = |lo: Vec3, hi: Vec3| ray_box(origin, dir, lo, hi).map(|d| (d, Vec3::ZERO));
    for (i, t) in ed.level.things.iter().enumerate() {
        if ed.is_hidden(Item::Thing(i)) {
            continue;
        }
        let at = IVec3::from(t.at).as_vec3();
        if !t.is_piece() {
            // Its figure as it is drawn (`Figures::key_in`), unturned there.
            let key = figures.key_in(t);
            let met = match figures.figure_cells(&key) {
                Some((cells, lo)) => cells.hit(origin - at - lo.as_vec3(), dir).map(|(d, n)| (d, n.as_vec3())),
                None => {
                    let (lo, hi) = figures.bounds(&key);
                    boxed(at + lo.as_vec3(), at + hi.as_vec3())
                }
            };
            take(&mut best, met, Item::Thing(i), None);
            continue;
        }
        let size = ed.size_of(t);
        if t.model.is_none() {
            if marks {
                let spot = room::light_spots(t, size).next().map_or(at, |(spot, _)| spot);
                take(&mut best, boxed(spot - 3.0, spot + 3.0), Item::Thing(i), None);
            }
            continue;
        }
        // Into the piece's own unturned room (from its pivot cell's low
        // corner): about where it turns; then its model's box from its
        // low corner (the pivot `size / 2` across and deep, 0 up).
        let about = room::centre_in(size);
        let turn = room::rotation_of(t);
        let back = turn.inverse();
        let local = about + back * (origin - at - about);
        let met = match (t.model.as_deref().and_then(|m| figures.model_cells(m)), size) {
            (Some(cells), Some(s)) => {
                let pivot = IVec3::new(s.x / 2, s.y / 2, 0).as_vec3();
                cells.hit(local + pivot, back * dir).map(|(d, n)| (d, turn * n.as_vec3()))
            }
            _ => {
                let (lo, hi) = figures.piece_box(t);
                ray_box(local, back * dir, lo, hi).map(|d| (d, Vec3::ZERO))
            }
        };
        take(&mut best, met, Item::Thing(i), None);
        // Its lights before it, a little nearer (they sit on or in it).
        if marks {
            for (k, (spot, _)) in room::light_spots(t, size).enumerate() {
                take(&mut best, ray_box(origin, dir, spot - 1.5, spot + 1.5).map(|d| (d - 2.0, Vec3::ZERO)), Item::Thing(i), Some(k));
            }
        }
    }
    if !marks {
        return best;
    }
    for (i, p) in ed.level.places.iter().enumerate() {
        if ed.is_hidden(Item::Place(i)) {
            continue;
        }
        let at = IVec3::from(p.at).as_vec3() + Vec3::new(0.5, 0.5, 0.0);
        take(&mut best, boxed(at - Vec3::new(4.0, 4.0, 0.0), at + Vec3::new(4.0, 4.0, 10.0)), Item::Place(i), None);
    }
    if best.is_none() {
        for (i, a) in ed.level.areas.iter().enumerate() {
            if ed.is_hidden(Item::Area(i)) {
                continue;
            }
            let at = IVec3::from(a.at).as_vec3();
            take(&mut best, boxed(at, at + IVec3::from(a.size).as_vec3()), Item::Area(i), None);
        }
    }
    best
}

/// The spot a ray points at to set something down (ADD): before the face
/// of the first voxel it meets, the level's or a piece's or a thing's
/// (`hit_along`), else on the floor.
pub fn set_down_spot(ed: &Editor, figures: &Figures, world: &World, origin: Vec3, dir: Vec3) -> Option<IVec3> {
    let floor = super::super::model::pick(origin, dir, world.size, |c| c.cmpge(IVec3::ZERO).all() && world.get(c) != 0);
    let floor_at = floor.and_then(|h| ray_box(origin, dir, h.cell.as_vec3(), h.cell.as_vec3() + 1.0)).unwrap_or(f32::INFINITY);
    match hit_along(ed, figures, origin, dir, false) {
        Some(m) if m.at < floor_at && m.normal != Vec3::ZERO => {
            let p = origin + dir * m.at + m.normal * 0.5;
            Some(p.floor().as_ivec3().max(IVec3::ZERO))
        }
        _ => floor_spot(world, origin, dir),
    }
}

/// The spot a ray (in voxels) points at to set something down: before the
/// face of the level's voxels it meets, or on the floor.
pub fn floor_spot(world: &World, origin: Vec3, dir: Vec3) -> Option<IVec3> {
    let hit = super::super::model::pick(origin, dir, world.size, |c| c.cmpge(IVec3::ZERO).all() && world.get(c) != 0)?;
    Some(hit.cell + if hit.normal == IVec3::ZERO { IVec3::Z } else { hit.normal })
}
