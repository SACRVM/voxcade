//! GRID: faint lines along the voxels' edges on the faces the eye sees, in
//! the voxel editor (a block, a model's frame, a figure's part, a figure
//! whole whose parts stand unturned), so voxels can be counted on a flat
//! face, as MagicaVoxel's grid. Only where a voxel is big enough on the
//! screen to count (`MIN_VOXEL`), never in GAME VIEW or a level. Drawn
//! over the 3D view by the panels' painter: each edge of a face turned to
//! the eye whose middle the eye sees (nothing of the shape between).

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use glam::{IVec3, Mat4, Vec2, Vec3};

use super::model::{Editor, Scope};
use super::view::View;
use crate::world::CellSet;

/// How many points a voxel spans on the screen at least for its lines to
/// show (fewer: they would cover the faces).
pub const MIN_VOXEL: f32 = 7.0;

/// More faces than this turned to the eye: no lines (they would take too
/// long to find, and stand too close to read).
const MOST_FACES: usize = 60_000;

/// The lines' colour: dark and faint over any voxel.
pub const LINE: egui::Color32 = egui::Color32::from_rgba_premultiplied(0, 0, 0, 56);

/// The lines last worked out and what they were worked out for.
#[derive(Default)]
pub struct Kept {
    key: Option<u64>,
    lines: Vec<[Vec2; 2]>,
}

/// Whether GRID's lines are drawn now: lit, in the voxel editor, not in
/// GAME VIEW, not over a figure whose parts are turned or posed (their
/// cells are no grid of the view's).
fn shows(ed: &Editor, view: &View) -> bool {
    if !view.grid || view.before_game.is_some() {
        return false;
    }
    match ed.scope {
        Scope::Level => false,
        Scope::Block(_) | Scope::Frame(_) => ed.shape().is_some(),
        Scope::Figure => ed.figure.as_ref().is_some_and(|f| !crate::session::rests_turned(&f.model) && !f.pose.iter().any(|q| *q != crate::session::PartPose::REST)),
    }
}

/// The cells shown in the grid the view draws (`View::view_proj` of the
/// block: a turned part's own grid), those SLICE cuts away left out.
fn cells(ed: &Editor) -> CellSet {
    let mut set = CellSet::default();
    match ed.scope {
        Scope::Level => {}
        Scope::Block(_) | Scope::Frame(_) => {
            let Some(b) = ed.shape() else { return set };
            for z in 0..b.size.z {
                for y in 0..b.size.y {
                    for x in 0..b.size.x {
                        let c = IVec3::new(x, y, z);
                        if b.get(c) != 0 && !ed.cut(c) {
                            set.insert(c);
                        }
                    }
                }
            }
        }
        Scope::Figure => {
            let (Some(figure), Some((shift, _))) = (ed.figure.as_ref(), ed.figure_box()) else { return set };
            let Some(frame) = figure.model.frames.first() else { return set };
            for (p, part) in figure.model.parts.iter().enumerate() {
                if figure.hidden[p] {
                    continue;
                }
                for &k in &part.voxels {
                    let c = frame[k as usize].0 + shift;
                    if !ed.cut(c) {
                        set.insert(c);
                    }
                }
            }
        }
    }
    set
}

/// What the lines depend on: the view's matrix, the screen, the voxels
/// shown and the slice.
fn key_of(ed: &Editor, vp: Mat4, screen: Vec2) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in vp.to_cols_array() {
        v.to_bits().hash(&mut h);
    }
    screen.x.to_bits().hash(&mut h);
    screen.y.to_bits().hash(&mut h);
    format!("{:?}", ed.scope).hash(&mut h);
    ed.slice.as_ref().map(|s| (s.axis, s.at, s.side)).hash(&mut h);
    match ed.scope {
        Scope::Figure => {
            if let Some(f) = &ed.figure {
                f.hidden.hash(&mut h);
                ed.figure_box().map(|(lo, _)| lo).hash(&mut h);
                for v in f.model.frames.first().into_iter().flatten() {
                    v.hash(&mut h);
                }
            }
        }
        _ => {
            if let Some(b) = ed.shape() {
                b.size.hash(&mut h);
                b.voxels.hash(&mut h);
            }
        }
    }
    h.finish()
}

/// The lines to draw over the view (points from the screen's top left),
/// worked out anew only when the view or the voxels changed.
pub fn lines(ed: &Editor, view: &View, screen: Vec2) -> Vec<[Vec2; 2]> {
    if !shows(ed, view) {
        return Vec::new();
    }
    let aspect = screen.x / screen.y.max(1.0);
    let vp = view.view_proj(&view.block, aspect);
    let key = key_of(ed, vp, screen);
    let mut kept = view.grid_lines.borrow_mut();
    if kept.key != Some(key) {
        let cells = cells(ed);
        // The eye in the grid shown (a turned part's own).
        let eye = view.rest.inverse().transform_point3(view.block_eye(aspect));
        kept.lines = if cells.is_empty() { Vec::new() } else { work_out(&cells, vp, screen, eye) };
        kept.key = Some(key);
    }
    kept.lines.clone()
}

/// The six ways out of a cell.
const DIRS: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

/// The edges of the faces of `cells` turned to `eye` whose middles the eye
/// sees, projected (`vp`, `screen`); none where a voxel spans fewer than
/// `MIN_VOXEL` points.
pub fn work_out(cells: &CellSet, vp: Mat4, screen: Vec2, eye: Vec3) -> Vec<[Vec2; 2]> {
    let lo = cells.iter().fold(IVec3::MAX, |a, c| a.min(*c));
    let hi = cells.iter().fold(IVec3::MIN, |a, c| a.max(*c + 1));
    // How big a voxel is on the screen, at the cells' middle.
    let mid = (lo + hi).as_vec3() / 2.0;
    let Some((m, _)) = super::handles::project(vp, screen, mid) else { return Vec::new() };
    let span = [Vec3::X, Vec3::Y, Vec3::Z].iter().filter_map(|&a| super::handles::project(vp, screen, mid + a)).map(|(p, _)| p.distance(m)).fold(0.0, f32::max);
    if span < MIN_VOXEL {
        return Vec::new();
    }
    // The faces turned to the eye, open to the air.
    let mut faces = Vec::new();
    for &c in cells {
        for (d, n) in DIRS.iter().enumerate() {
            if cells.contains(&(c + *n)) {
                continue;
            }
            let centre = c.as_vec3() + 0.5 + n.as_vec3() * 0.5;
            if n.as_vec3().dot(eye - centre) > 0.0 {
                faces.push((c, d));
            }
        }
        if faces.len() > MOST_FACES {
            return Vec::new();
        }
    }
    // Each face's four edges (a corner and an axis), drawn once, where the
    // eye sees the edge's middle (moved a little onto the face, a little
    // out of it).
    let mut seen: HashSet<(IVec3, u8)> = HashSet::new();
    let mut out = Vec::new();
    for (c, d) in faces {
        let n = DIRS[d];
        let a = d / 2;
        let (b, e) = ((a + 1) % 3, (a + 2) % 3);
        let mut corner = c;
        if n[a] > 0 {
            corner[a] += 1;
        }
        let (ub, ue) = (unit(b), unit(e));
        let centre = corner.as_vec3() + (ub + ue).as_vec3() * 0.5;
        for (from, axis) in [(corner, b), (corner + ue, b), (corner, e), (corner + ub, e)] {
            let key = (from, axis as u8);
            if seen.contains(&key) {
                continue;
            }
            let to = from + unit(axis);
            let middle = (from + to).as_vec3() / 2.0;
            let probe = middle + (centre - middle) * 0.2 + n.as_vec3() * 0.02;
            if !clear(cells, probe, eye, lo, hi) {
                continue;
            }
            seen.insert(key);
            if let (Some((p, _)), Some((q, _))) = (super::handles::project(vp, screen, from.as_vec3()), super::handles::project(vp, screen, to.as_vec3())) {
                out.push([p, q]);
            }
        }
    }
    out
}

fn unit(axis: usize) -> IVec3 {
    let mut u = IVec3::ZERO;
    u[axis] = 1;
    u
}

/// Whether nothing of `cells` (within `lo`..`hi`) lies between `from` and
/// `eye`: the cells the line crosses, one by one.
fn clear(cells: &CellSet, from: Vec3, eye: Vec3, lo: IVec3, hi: IVec3) -> bool {
    let line = eye - from;
    let length = line.length();
    if length < 1e-6 {
        return true;
    }
    let dir = line / length;
    let mut cell = from.floor().as_ivec3();
    let step = dir.signum().as_ivec3();
    let mut next = Vec3::ZERO;
    let mut delta = Vec3::ZERO;
    for a in 0..3 {
        if dir[a].abs() < 1e-9 {
            next[a] = f32::INFINITY;
            delta[a] = f32::INFINITY;
        } else {
            let edge = if dir[a] > 0.0 { cell[a] + 1 } else { cell[a] } as f32;
            next[a] = (edge - from[a]) / dir[a];
            delta[a] = 1.0 / dir[a].abs();
        }
    }
    loop {
        if cells.contains(&cell) {
            return false;
        }
        let a = if next.x < next.y && next.x < next.z { 0 } else if next.y < next.z { 1 } else { 2 };
        if next[a] > length {
            return true;
        }
        cell[a] += step[a];
        next[a] += delta[a];
        // Out of the cells' box on the way to the eye: nothing more.
        if (step[a] > 0 && cell[a] >= hi[a]) || (step[a] < 0 && cell[a] < lo[a]) {
            return true;
        }
    }
}

/// GRID's lines over the view (`free`, the room the panels leave), faint.
pub fn paint(ui: &egui::Ui, ed: &Editor, view: &View, free: egui::Rect) {
    let screen = ui.ctx().content_rect();
    let lines = lines(ed, view, Vec2::new(screen.width(), screen.height()));
    if lines.is_empty() {
        return;
    }
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("grid lines"))).with_clip_rect(free);
    let stroke = egui::Stroke::new(1.0, LINE);
    for [p, q] in lines {
        painter.line_segment([screen.min + egui::vec2(p.x, p.y), screen.min + egui::vec2(q.x, q.y)], stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A camera looking at a 4 by 4 by 4 cube from in front, above and to
    /// the side (`near`: how far away).
    fn camera(near: f32) -> (Mat4, Vec3) {
        let eye = Vec3::new(2.0, 2.0, 2.0) + Vec3::new(-0.6, -1.0, 0.8).normalize() * near;
        let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::splat(2.0), Vec3::Z);
        let proj = glam::camera::rh::proj::directx::perspective(0.6, 16.0 / 9.0, 0.1, 500.0);
        (proj * view, eye)
    }

    fn cube(n: i32) -> CellSet {
        let mut set = CellSet::default();
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    set.insert(IVec3::new(x, y, z));
                }
            }
        }
        set
    }

    /// Near, a cube's three faces turned to the eye are ruled voxel by
    /// voxel: 4 by 4 cells each, 3 faces, their edges once each (five
    /// lines of four each way on a face, those shared counted once); none
    /// of the faces turned away; far off, none.
    #[test]
    fn a_cube_seen_near_shows_its_cells_and_far_none() {
        let (vp, eye) = camera(14.0);
        let screen = Vec2::new(1600.0, 900.0);
        let lines = work_out(&cube(4), vp, screen, eye);
        // Three faces of 4 by 4: each 2 * 5 * 4 = 40 edges, the three
        // edges where two of them meet (4 each) counted once.
        assert_eq!(lines.len(), 3 * 40 - 3 * 4, "{}", lines.len());
        let (vp, eye) = camera(2000.0);
        assert!(work_out(&cube(4), vp, screen, eye).is_empty(), "far off: none");
    }

    /// GRID is lit when a model opens and its lines are drawn there (a
    /// wall near enough); never in GAME VIEW, never in a level of blocks;
    /// GRID pressed: none.
    #[test]
    fn grid_is_lit_in_the_voxel_editor_only() {
        use crate::editor::view::{Action, act};
        let (mut win, _dir) = crate::editor::big::wall_window("grid-lit", [8, 3, 6]);
        assert!(win.view.grid && shows(&win.ed, &win.view));
        assert!(!lines(&win.ed, &win.view, Vec2::new(1600.0, 900.0)).is_empty(), "the wall ruled");
        act(&mut win.ed, &mut win.view, Action::Grid);
        assert!(lines(&win.ed, &win.view, Vec2::new(1600.0, 900.0)).is_empty(), "GRID off");
        act(&mut win.ed, &mut win.view, Action::Grid);
        win.view.before_game = Some(win.view.block);
        assert!(!shows(&win.ed, &win.view), "GAME VIEW");
        let level = crate::editor::model::tests::folder("grid-level");
        let ed = Editor::open(&level, "level.toml").unwrap();
        let view = View::new(&ed, &Default::default());
        assert!(!shows(&ed, &view), "a level of blocks");
    }

    /// A face hidden behind another part of the shape shows no lines: a
    /// wall in front of a cube hides the cube's front face.
    #[test]
    fn faces_behind_the_shape_show_none() {
        let (vp, eye) = camera(14.0);
        let screen = Vec2::new(1600.0, 900.0);
        let alone = work_out(&cube(4), vp, screen, eye).len();
        // A tall thin wall right before the cube, the way the eye looks.
        let mut set = cube(4);
        for z in 0..12 {
            for x in -6..10 {
                set.insert(IVec3::new(x, -3, z));
            }
        }
        let lines = work_out(&set, vp, screen, eye);
        // Of the cube's front, nothing; the lines are the wall's and what
        // shows of the cube's top over it.
        let front = |p: Vec2| {
            let (q, _) = crate::editor::handles::project(vp, screen, Vec3::new(2.0, 0.0, 2.0)).unwrap();
            p.distance(q) < 2.0
        };
        assert!(!lines.iter().any(|[p, q]| front((*p + *q) / 2.0)), "the cube's front is hidden");
        assert!(lines.len() > alone / 3, "the wall's own lines");
    }
}
