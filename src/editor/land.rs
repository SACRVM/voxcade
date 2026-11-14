//! The level editor's TERRAIN (`crate::terrain`): the land's points drawn
//! on its surface, raised and lowered by dragging, a brush round the one
//! grabbed so a drag makes a hill rather than a spike; FLATTEN, SMOOTH and
//! PAINT (a shape) brushed over the points; several selected by a
//! rectangle on the screen and moved together; the grid's spacing and
//! size, the materials, UNDER and SEED in the inspector. Every change is a
//! step of the level, written as the level's text writes the terrain (only
//! its lines); a drag is one step, the land built again as it goes.

use glam::{IVec2, Mat4, Vec2, Vec3};

use super::selection::SelectMode;
use std::sync::Arc;

use super::{Begun, Editor, Layer, Scope, Step};
use crate::terrain::{HIGHEST, MATERIALS, Material, Shape, Terrain};

/// What a press among the points does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LandTool {
    /// A point chosen, and dragged up or down (the brush round it, or the
    /// selection with it).
    #[default]
    Pointer,
    /// A rectangle on the screen selects the points in it.
    Select,
    /// The points brushed take the height of the one pressed first.
    Flatten,
    /// The points brushed go toward their neighbours' mean.
    Smooth,
    /// The points brushed take the shape held.
    Paint,
    /// The points brushed get rougher (their share of `rough`), or calmer.
    Roughen,
    Calm,
}

impl LandTool {
    pub const ALL: [LandTool; 7] = [LandTool::Pointer, LandTool::Select, LandTool::Flatten, LandTool::Smooth, LandTool::Paint, LandTool::Roughen, LandTool::Calm];

    pub fn label(self) -> &'static str {
        match self {
            LandTool::Pointer => "POINTER",
            LandTool::Select => "SELECT",
            LandTool::Flatten => "FLATTEN",
            LandTool::Smooth => "SMOOTH",
            LandTool::Paint => "PAINT",
            LandTool::Roughen => "ROUGHEN",
            LandTool::Calm => "CALM",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            LandTool::Pointer => "Choose a point; drag it up or down, the brush's RADIUS round it (or the selection with it)",
            LandTool::Select => "A rectangle over the points selects them (NEW, ADD, REMOVE)",
            LandTool::Flatten => "Drag over the points: they take the height of the first one",
            LandTool::Smooth => "Drag over the points: rough land evened out",
            LandTool::Paint => "Drag over the points: they take the SHAPE held",
            LandTool::Roughen => "Drag over the points: the land gets lumpy there (ROUGH says how much at most)",
            LandTool::Calm => "Drag over the points: the land gets even there",
        }
    }
}

/// A number of the terrain set in the inspector: the brush's radius, the
/// chosen points' height, the grid's spacing and size, the seed; a
/// material's (by its place) conditions, depth and grain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandField {
    Radius,
    Height,
    Spacing,
    Cols,
    Rows,
    Terrace,
    Rough,
    Seed,
    Thick(u8),
    Hard(u8),
    Patch(u8),
    Blend(u8),
    /// A scatter rule's numbers: how many on 100 by 100 voxels, how far
    /// apart, how deep sunk; its conditions (as a material's) and blend.
    Dip,
    DipTo,
    Fold,
    Fault,
    Density(u8),
    Apart(u8),
    Sink(u8),
    ScAbove(u8),
    ScBelow(u8),
    ScSteeper(u8),
    ScFlatter(u8),
    ScBlend(u8),
    Above(u8),
    Below(u8),
    Steeper(u8),
    Flatter(u8),
    Depth(u8),
    Grain(u8),
}

/// A row of TERRAIN's outliner: a material, a layer, a scatter rule (its
/// place in its list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandRow {
    Material(usize),
    Layer(usize),
    Scatter(usize),
}

/// The editor's state among the points.
#[derive(Clone, Debug)]
pub struct Land {
    /// The row chosen in the outliner (its fields in the inspector).
    pub row: Option<LandRow>,
    pub tool: LandTool,
    /// The brush's radius, in points (1: the point alone).
    pub radius: i32,
    /// The shape PAINT lays and the SHAPE buttons show.
    pub shape: Shape,
    /// The point chosen last, and the points selected.
    pub chosen: Option<IVec2>,
    pub selected: Vec<IVec2>,
    pub mode: SelectMode,
    /// The point under the mouse, and the nearest to it on the screen
    /// (however far): the points round it are drawn, not the rest.
    pub hover: Option<IVec2>,
    pub near: Option<IVec2>,
    drag: Option<PointDrag>,
}

impl Default for Land {
    fn default() -> Self {
        Self { row: None, tool: LandTool::Pointer, radius: 3, shape: Shape::Smooth, chosen: None, selected: Vec::new(), mode: SelectMode::New, hover: None, near: None, drag: None }
    }
}

/// A press among the points held: the point grabbed, the heights as they
/// were, where on the vertical through the point the pointer was then
/// (POINTER's), FLATTEN's height, the point brushed last.
#[derive(Clone, Debug)]
struct PointDrag {
    grabbed: IVec2,
    from: Vec<i32>,
    start: Option<f32>,
    target: i32,
    last: Option<IVec2>,
}

/// How high on the vertical through `at` (its x and y) a ray from the eye
/// passes nearest: where the pointer is along that line, as a piece's Z
/// arrow follows it. None looking straight along it.
pub fn height_along(at: Vec3, origin: Vec3, dir: Vec3) -> Option<f32> {
    let w = Vec3::new(at.x, at.y, 0.0) - origin;
    let (b, c) = (dir.z, dir.dot(dir));
    let (d, e) = (w.z, dir.dot(w));
    let denom = c - b * b;
    (denom.abs() > 1e-6 * c.max(1e-6)).then(|| (b * e - c * d) / denom)
}

/// How much of a brush's change reaches a point `d` points from its middle
/// (`radius` 1: the middle alone): a smooth fall to nothing at the radius.
pub fn falloff(d: f32, radius: i32) -> f32 {
    let r = radius.max(1) as f32;
    if d >= r {
        return 0.0;
    }
    let t = d / r;
    (1.0 - t * t) * (1.0 - t * t)
}

/// `v` to the nearest `1 / per` (tenths, hundredths), kept within `lo`
/// and `hi`: how the terrain's numbers are written.
fn stepped(v: f64, per: f64, lo: f64, hi: f64) -> f64 {
    ((v * per).round() / per).clamp(lo, hi)
}

impl Editor {
    /// Whether the level's tools work on its terrain.
    pub fn among_land(&self) -> bool {
        self.scope == Scope::Level && self.layer == Layer::Terrain
    }

    /// The level's terrain.
    pub fn terrain(&self) -> Option<&Terrain> {
        self.level.terrain.as_ref()
    }

    /// The tallest a point may be: the level's room over the terrain's base.
    fn highest(&self) -> i32 {
        let base = self.terrain().map_or(0, |t| t.at[2]);
        (self.level.voxels().z - base).clamp(0, HIGHEST)
    }

    /// TERRAIN (ADD's menu): a flat land over the whole level, grass, its
    /// points 16 voxels apart (fewer for a small level); TERRAIN shown.
    pub fn add_terrain(&mut self) {
        if self.level.terrain.is_none() {
            let v = self.level.voxels();
            let spacing = (v.x.min(v.y) / 4).clamp(1, 16);
            let height = (v.z / 8).clamp(1, 8);
            let t = Terrain::flat(spacing, (v.x / spacing).max(1), (v.y / spacing).max(1), height);
            self.change(None, |l| l.terrain = Some(t));
        }
        self.land = Land::default();
        self.switch(Layer::Terrain);
    }

    /// REMOVE TERRAIN: the land taken out (a step); the pieces shown.
    pub fn remove_terrain(&mut self) {
        self.change(None, |l| l.terrain = None);
        self.land = Land::default();
        self.switch(Layer::Pieces);
    }

    /// Where a point stands in the level: on its column's middle, at its
    /// height.
    pub fn point_spot(&self, p: IVec2) -> Option<Vec3> {
        let t = self.terrain()?;
        let i = t.index(p)?;
        let c = t.column_of(p);
        Some(Vec3::new((t.at[0] + c.x) as f32 + 0.5, (t.at[1] + c.y) as f32 + 0.5, (t.at[2] + t.heights[i]) as f32))
    }

    /// Every point and where it is on the screen (`view_proj` over a
    /// screen `screen` big), those in front of the eye.
    pub fn points_on_screen(&self, view_proj: Mat4, screen: Vec2) -> Vec<(IVec2, Vec2)> {
        let Some(t) = self.terrain() else { return Vec::new() };
        let mut out = Vec::with_capacity(t.heights.len());
        for y in 0..t.rows {
            for x in 0..t.cols {
                let p = IVec2::new(x, y);
                if let Some((s, _)) = self.point_spot(p).and_then(|at| super::super::handles::project(view_proj, screen, at)) {
                    out.push((p, s));
                }
            }
        }
        out
    }

    /// The point nearest the mouse on the screen, within a few pixels.
    pub fn point_at(&self, view_proj: Mat4, screen: Vec2, mouse: Vec2) -> Option<IVec2> {
        const NEAR: f32 = 10.0;
        self.points_on_screen(view_proj, screen).into_iter().map(|(p, s)| (p, s.distance(mouse))).filter(|&(_, d)| d <= NEAR).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(p, _)| p)
    }

    /// The point nearest the mouse on the screen, however far.
    pub fn nearest_point(&self, view_proj: Mat4, screen: Vec2, mouse: Vec2) -> Option<IVec2> {
        self.points_on_screen(view_proj, screen).into_iter().min_by(|a, b| a.1.distance_squared(mouse).total_cmp(&b.1.distance_squared(mouse))).map(|(p, _)| p)
    }

    /// Whether a point is drawn: the chosen and selected ones, the one
    /// under the mouse, and those within the brush's radius and two more of
    /// the point nearest the mouse (a dense grid would cover the land).
    pub fn point_shown(&self, p: IVec2) -> bool {
        let l = &self.land;
        l.chosen == Some(p) || l.hover == Some(p) || l.selected.contains(&p) || l.near.is_some_and(|n| (n - p).abs().max_element() <= l.radius + 2)
    }

    /// The points a change goes to: the selection (with the chosen one in
    /// it), else the chosen one.
    fn targets(&self) -> Vec<IVec2> {
        match self.land.chosen {
            Some(c) if self.land.selected.is_empty() || self.land.selected.contains(&c) => {
                if self.land.selected.is_empty() {
                    vec![c]
                } else {
                    self.land.selected.clone()
                }
            }
            _ => self.land.selected.clone(),
        }
    }

    /// A press among the points (`p` under the mouse, `ray` from the eye
    /// through it): POINTER chooses it and grabs it; FLATTEN, SMOOTH and
    /// PAINT brush there. Nothing under it: none chosen, none selected.
    pub fn land_press(&mut self, p: Option<IVec2>, ray: (Vec3, Vec3)) {
        if self.drag_open() {
            self.end_drag(false);
        }
        let Some(t) = self.terrain() else { return };
        let from = t.heights.clone();
        let at = p.and_then(|p| t.index(p));
        let (shape, height) = at.map_or((self.land.shape, 0), |i| (t.shapes[i], t.heights[i]));
        match (self.land.tool, p) {
            (LandTool::Select, _) => {}
            (LandTool::Pointer, None) => {
                self.land.chosen = None;
                self.land.selected.clear();
            }
            (LandTool::Pointer, Some(p)) => {
                if !self.land.selected.contains(&p) {
                    self.land.selected.clear();
                }
                self.land.chosen = Some(p);
                self.land.shape = shape;
                self.history.stroke = Some(Begun::Level(self.level_state()));
                let start = self.point_spot(p).and_then(|at| height_along(at, ray.0, ray.1));
                self.land.drag = Some(PointDrag { grabbed: p, from, start, target: 0, last: None });
            }
            (_, p) => {
                let target = height;
                self.history.stroke = Some(Begun::Level(self.level_state()));
                self.land.drag = Some(PointDrag { grabbed: p.unwrap_or_default(), from, start: None, target, last: None });
                if let Some(p) = p {
                    self.brush(p);
                }
            }
        }
    }

    /// The mouse dragged on (`p` under it, `ray` through it): POINTER's
    /// point up or down as far as the pointer went along the vertical
    /// through it (the point stays under the pointer; `fine`, Shift: a
    /// third of that), in whole voxels; a brush over the point under it.
    pub fn land_drag(&mut self, p: Option<IVec2>, ray: (Vec3, Vec3), fine: bool) {
        let Some(d) = self.land.drag.clone() else { return };
        if self.land.tool == LandTool::Pointer {
            let (Some(start), Some(at)) = (d.start, d.from.get(self.terrain().and_then(|t| t.index(d.grabbed)).unwrap_or(usize::MAX)).copied()) else { return };
            let Some(spot) = self.point_spot(d.grabbed) else { return };
            let Some(now) = height_along(Vec3::new(spot.x, spot.y, at as f32), ray.0, ray.1) else { return };
            let by = (now - start) / if fine { 3.0 } else { 1.0 };
            self.raise_from(&d.from, d.grabbed, by.round() as i32);
            return;
        }
        if let Some(p) = p
            && d.last != Some(p)
        {
            self.brush(p);
        }
    }

    /// Whether a press among the points is held.
    pub fn land_dragging(&self) -> bool {
        self.land.drag.is_some()
    }

    pub(super) fn let_go_of_land(&mut self) {
        self.land.drag = None;
    }

    /// The heights `from` with the point `at` raised by `by` voxels: the
    /// selection with it alike, else the brush round it.
    pub fn raise_from(&mut self, from: &[i32], at: IVec2, by: i32) {
        let highest = self.highest();
        let radius = self.land.radius;
        let together = self.land.selected.contains(&at);
        let selected = self.land.selected.clone();
        let Some(t) = self.level.terrain.as_mut() else { return };
        for y in 0..t.rows {
            for x in 0..t.cols {
                let q = IVec2::new(x, y);
                let i = t.index(q).expect("a point");
                let w = if together {
                    if selected.contains(&q) { 1.0 } else { 0.0 }
                } else {
                    falloff(q.as_vec2().distance(at.as_vec2()), radius)
                };
                t.heights[i] = (from[i] + (by as f32 * w).round() as i32).clamp(0, highest);
            }
        }
    }

    /// The level changed as one step (`--shot`'s changes).
    pub fn change_level(&mut self, f: impl FnOnce(&mut crate::grid::GridLevel)) {
        self.change(None, f);
    }

    /// FLATTEN, SMOOTH or PAINT once round the point `at`.
    fn brush(&mut self, at: IVec2) {
        let Some(d) = self.land.drag.as_mut() else { return };
        d.last = Some(at);
        let target = d.target;
        let (radius, shape, tool) = (self.land.radius, self.land.shape, self.land.tool);
        let Some(t) = self.level.terrain.as_mut() else { return };
        // ROUGHEN on even land: the land made able to be rough (ROUGH a
        // half), every point calm but where it is brushed.
        if matches!(tool, LandTool::Roughen | LandTool::Calm) && t.roughness.is_none() {
            let rough = t.rough.unwrap_or(0.0) > 0.0;
            if tool == LandTool::Calm && !rough {
                return;
            }
            t.roughness = Some(vec![if rough { 9 } else { 0 }; t.heights.len()]);
            if !rough {
                t.rough = Some(0.5);
            }
        }
        let was = t.heights.clone();
        let reach = radius.max(1);
        for y in (at.y - reach)..=(at.y + reach) {
            for x in (at.x - reach)..=(at.x + reach) {
                let q = IVec2::new(x, y);
                let Some(i) = t.index(q) else { continue };
                let w = falloff(q.as_vec2().distance(at.as_vec2()), radius);
                if w <= 0.0 {
                    continue;
                }
                match tool {
                    LandTool::Flatten => t.heights[i] += ((target - was[i]) as f32 * w).round() as i32,
                    LandTool::Smooth => {
                        let mut sum = 0;
                        let mut n = 0;
                        for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                            if let Some(k) = t.index(q + IVec2::new(dx, dy)) {
                                sum += was[k];
                                n += 1;
                            }
                        }
                        let mean = sum as f32 / n as f32;
                        t.heights[i] += ((mean - was[i] as f32) * w).round() as i32;
                    }
                    LandTool::Paint => t.shapes[i] = shape,
                    LandTool::Roughen | LandTool::Calm => {
                        let r = t.roughness.as_mut().expect("made above");
                        let by = (4.0 * w).round().max(1.0) as i32 * if tool == LandTool::Roughen { 1 } else { -1 };
                        r[i] = (r[i] as i32 + by).clamp(0, 9) as u8;
                    }
                    _ => {}
                }
            }
        }
    }

    /// SELECT's rectangle (pixels `a` to `b` on a screen `screen` big)
    /// let go: the points in it selected as `mode` says.
    pub fn select_points(&mut self, view_proj: Mat4, screen: Vec2, a: Vec2, b: Vec2, mode: SelectMode) {
        let (lo, hi) = (a.min(b), a.max(b));
        let inside: Vec<IVec2> = self.points_on_screen(view_proj, screen).into_iter().filter(|(_, s)| s.cmpge(lo).all() && s.cmple(hi).all()).map(|(p, _)| p).collect();
        let sel = &mut self.land.selected;
        match mode {
            SelectMode::New => *sel = inside,
            SelectMode::Add => {
                for p in inside {
                    if !sel.contains(&p) {
                        sel.push(p);
                    }
                }
            }
            SelectMode::Remove => sel.retain(|p| !inside.contains(p)),
        }
        sel.sort_by_key(|p| (p.y, p.x));
        self.land.chosen = self.land.selected.first().copied();
    }

    /// The SHAPE buttons: the shape held for PAINT, and given to the
    /// chosen or selected points (a step).
    pub fn land_shape(&mut self, shape: Shape) {
        self.land.shape = shape;
        let targets = self.targets();
        if targets.is_empty() {
            return;
        }
        self.change(None, |l| {
            if let Some(t) = l.terrain.as_mut() {
                for p in targets {
                    if let Some(i) = t.index(p) {
                        t.shapes[i] = shape;
                    }
                }
            }
        });
    }

    /// A number of the terrain set (a step; the same number set again
    /// joins it).
    pub fn land_set(&mut self, field: LandField, v: f64) {
        let whole = v.round() as i32;
        let highest = self.highest();
        let room = self.level.voxels();
        match field {
            LandField::Radius => {
                self.land.radius = whole.clamp(1, 64);
                return;
            }
            LandField::Height => {
                let targets = self.targets();
                self.change(Some(super::room::Field::Land(field)), |l| {
                    if let Some(t) = l.terrain.as_mut() {
                        for p in targets {
                            if let Some(i) = t.index(p) {
                                t.heights[i] = whole.clamp(0, highest);
                            }
                        }
                    }
                });
                return;
            }
            _ => {}
        }
        self.change(Some(super::room::Field::Land(field)), |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            let m = |k: u8, t: &mut Terrain| -> Option<usize> { ((k as usize) < t.materials.len()).then_some(k as usize) };
            match field {
                LandField::Spacing => {
                    // Kept within the level: fewer points where it would
                    // reach past it.
                    t.spacing = whole.clamp(1, 1024);
                    let cols = ((room.x - t.at[0]) / t.spacing).clamp(1, t.cols);
                    let rows = ((room.y - t.at[1]) / t.spacing).clamp(1, t.rows);
                    resize(t, cols, rows);
                }
                LandField::Cols => {
                    let most = ((room.x - t.at[0]) / t.spacing).max(1);
                    resize(t, whole.clamp(1, most), t.rows);
                }
                LandField::Rows => {
                    let most = ((room.y - t.at[1]) / t.spacing).max(1);
                    resize(t, t.cols, whole.clamp(1, most));
                }
                LandField::Seed => t.seed = Some(whole.max(0) as u64),
                LandField::Thick(k) => {
                    if let Some(l) = t.layers.get_mut(k as usize) {
                        l.thick = whole.clamp(1, HIGHEST);
                    }
                }
                LandField::Hard(k) => {
                    if let Some(l) = t.layers.get_mut(k as usize) {
                        l.hard = stepped(v, 100.0, 0.0, 1.0);
                    }
                }
                LandField::Patch(k) => {
                    if let Some(m) = t.materials.get_mut(k as usize) {
                        m.patch = Some(whole.clamp(1, 1000));
                    }
                }
                LandField::Blend(k) => {
                    if let Some(m) = t.materials.get_mut(k as usize) {
                        let b = stepped(v, 10.0, 0.0, 1000.0);
                        m.blend = (b > 0.0).then_some(b);
                    }
                }
                LandField::Density(k) | LandField::Apart(k) | LandField::Sink(k) | LandField::ScAbove(k) | LandField::ScBelow(k) | LandField::ScSteeper(k) | LandField::ScFlatter(k) | LandField::ScBlend(k) => {
                    if let Some(r) = t.scatter.get_mut(k as usize) {
                        let deg = stepped(v, 10.0, 0.0, 89.9);
                        match field {
                            LandField::Density(_) => r.density = stepped(v, 100.0, 0.0, 10_000.0),
                            LandField::Apart(_) => r.apart = Some(whole.clamp(0, HIGHEST)),
                            LandField::Sink(_) => r.sink = Some(whole.clamp(0, HIGHEST)),
                            LandField::ScAbove(_) => r.above = Some(whole.clamp(-HIGHEST, HIGHEST)),
                            LandField::ScBelow(_) => r.below = Some(whole.clamp(-HIGHEST, HIGHEST)),
                            LandField::ScSteeper(_) => r.steeper = Some(deg),
                            LandField::ScFlatter(_) => r.flatter = Some(deg),
                            _ => {
                                let b = stepped(v, 10.0, 0.0, 1000.0);
                                r.blend = (b > 0.0).then_some(b);
                            }
                        }
                    }
                }
                LandField::Terrace => t.terrace = Some(whole.clamp(1, HIGHEST)),
                LandField::Dip => t.dip = Some(stepped(v, 10.0, 0.0, 45.0)),
                LandField::DipTo => t.dip_to = Some(v.round().rem_euclid(360.0)),
                LandField::Fold => t.fold = Some(stepped(v, 100.0, 0.0, 1.0)),
                LandField::Fault => t.fault = Some(stepped(v, 100.0, 0.0, 1.0)),
                LandField::Rough => t.rough = Some(stepped(v, 100.0, 0.0, 1.0)),
                LandField::Above(k) | LandField::Below(k) | LandField::Depth(k) => {
                    if let Some(k) = m(k, t) {
                        let mat = &mut t.materials[k];
                        match field {
                            LandField::Above(_) => mat.above = Some(whole.clamp(-HIGHEST, HIGHEST)),
                            LandField::Below(_) => mat.below = Some(whole.clamp(-HIGHEST, HIGHEST)),
                            _ => mat.depth = Some(whole.clamp(1, HIGHEST)),
                        }
                    }
                }
                LandField::Steeper(k) | LandField::Flatter(k) => {
                    if let Some(k) = m(k, t) {
                        let deg = stepped(v, 10.0, 0.0, 89.9);
                        match field {
                            LandField::Steeper(_) => t.materials[k].steeper = Some(deg),
                            _ => t.materials[k].flatter = Some(deg),
                        }
                    }
                }
                LandField::Grain(k) => {
                    if let Some(k) = m(k, t) {
                        let g = stepped(v, 100.0, 0.0, 1.0);
                        t.materials[k].grain = (g > 0.0).then_some(g);
                    }
                }
                LandField::Radius | LandField::Height => {}
            }
        });
    }

    /// A material's condition (0 ABOVE, 1 BELOW, 2 STEEPER, 3 FLATTER) on
    /// or off; the last material has none (it paints the rest).
    pub fn land_condition(&mut self, k: u8, which: u8, on: bool) {
        self.change(None, |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            let last = t.materials.len().saturating_sub(1);
            let Some(m) = t.materials.get_mut(k as usize) else { return };
            if on && k as usize == last {
                return;
            }
            match (which, on) {
                (0, true) => m.above = Some(m.above.unwrap_or(40)),
                (1, true) => m.below = Some(m.below.unwrap_or(10)),
                (2, true) => m.steeper = Some(m.steeper.unwrap_or(45.0)),
                (3, true) => m.flatter = Some(m.flatter.unwrap_or(20.0)),
                (0, false) => m.above = None,
                (1, false) => m.below = None,
                (2, false) => m.steeper = None,
                _ => m.flatter = None,
            }
        });
    }

    /// A material's color (`k` the materials' count: UNDER's).
    pub fn land_colour(&mut self, k: u8, c: [u8; 3]) {
        let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        self.change(None, |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            match t.materials.get_mut(k as usize) {
                Some(m) => m.color = hex,
                None => t.under = Some(hex),
            }
        });
    }

    /// The pieces scatter rule `k` set down (marked `scatter = k + 1`),
    /// wherever they stand now.
    pub fn scattered(&self, k: usize) -> usize {
        self.level.things.iter().filter(|t| t.scatter == Some(k as u32 + 1)).count()
    }

    /// SCATTER: scatter rule `k`'s pieces set into the level as the level's
    /// own pieces (named after their model, on from the numbers there are,
    /// standing on the land, sunk as it says, turned as it says: freely
    /// only where the game turns things freely, else in quarters), marked
    /// as the rule's; one step. Nothing while pieces of it are there
    /// (CLEAR first): the reason said.
    pub fn scatter_pieces(&mut self, k: usize) {
        let Some(t) = self.terrain() else { return };
        let Some(rule) = t.scatter.get(k).cloned() else { return };
        if rule.makes() != crate::terrain::ScatterAs::Pieces {
            return;
        }
        if self.scattered(k) > 0 {
            self.hint = Some(format!("SCATTER {}: its pieces are there; CLEAR first", k + 1));
            return;
        }
        let placed = t.scatter(k, &t.surface());
        let (at, sink) = (t.at, rule.sink.unwrap_or(0));
        let mut names: Vec<String> = self.level.things.iter().filter_map(|t| t.name.clone()).collect();
        let mut things = Vec::new();
        for p in placed {
            let model = rule.models[p.model].clone();
            let stem = std::path::Path::new(&model).file_stem().map_or("piece".into(), |s| s.to_string_lossy().into_owned());
            let stem = stem.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>();
            let name = (1..).map(|n| format!("{stem}_{n}")).find(|n| !names.contains(n)).expect("a free name");
            names.push(name.clone());
            let facing = match rule.rotate() {
                crate::terrain::Rotate::None => None,
                crate::terrain::Rotate::Any if self.pieces.free_turns => Some(crate::grid::Facing::Turn(p.rotation)),
                _ => Some(crate::grid::Facing::Way(crate::grid::Way::nearest(p.rotation))),
            };
            let spot = [at[0] + p.column.x, at[1] + p.column.y, at[2] + p.ground - sink];
            things.push(crate::grid::Thing { model: Some(model), name: Some(name), facing, scatter: Some(k as u32 + 1), ..crate::grid::Thing::new(crate::grid::PIECE, spot) });
        }
        if things.is_empty() {
            self.hint = Some(format!("SCATTER {}: no ground fits it", k + 1));
            return;
        }
        self.change(None, |l| l.things.extend(things));
    }

    /// CLEAR: the pieces scatter rule `k` set down taken away, moved ones
    /// too (the mark stays on them); one step.
    pub fn clear_scatter(&mut self, k: usize) {
        let mark = Some(k as u32 + 1);
        self.change(None, |l| l.things.retain(|t| t.scatter != mark));
        self.keep_choice();
    }

    /// SCATTER's plus: a rule more, the game's first model, firs' numbers.
    pub fn add_scatter(&mut self) {
        let model = self.model_choices().into_iter().next().unwrap_or_else(|| "tree.vvx".into());
        self.change(None, |l| {
            if let Some(t) = l.terrain.as_mut() {
                t.scatter.push(crate::terrain::Scatter { apart: Some(12), flatter: Some(30.0), ..crate::terrain::Scatter::new(&model) });
            }
        });
        if let Some(t) = self.terrain() {
            self.land.row = Some(LandRow::Scatter(t.scatter.len() - 1));
        }
    }

    /// × of a rule: its pieces stay, no longer marked; the others' marks
    /// follow their rules' new numbers.
    pub fn remove_scatter(&mut self, k: usize) {
        self.change(None, |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            if k >= t.scatter.len() {
                return;
            }
            t.scatter.remove(k);
            for th in &mut l.things {
                th.scatter = match th.scatter {
                    Some(n) if n as usize == k + 1 => None,
                    Some(n) if n as usize > k + 1 => Some(n - 1),
                    other => other,
                };
            }
        });
        self.land.row = None;
    }

    /// A rule's row dragged onto another's place; the marks follow.
    pub fn move_scatter(&mut self, from: usize, to: usize) {
        self.change(None, |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            let n = t.scatter.len();
            if from >= n || to >= n || from == to {
                return;
            }
            let mut order: Vec<usize> = (0..n).collect();
            let r = order.remove(from);
            order.insert(to, r);
            let rule = t.scatter.remove(from);
            t.scatter.insert(to, rule);
            // (Rule old k now stands at place `order.position(k)`.)
            for th in &mut l.things {
                if let Some(m) = th.scatter {
                    let old = m as usize - 1;
                    if let Some(new) = order.iter().position(|&o| o == old) {
                        th.scatter = Some(new as u32 + 1);
                    }
                }
            }
        });
    }

    /// A rule's model (the game's model file), how it turns, what it makes.
    pub fn scatter_model(&mut self, k: usize, model: String) {
        self.change(None, |l| {
            if let Some(r) = l.terrain.as_mut().and_then(|t| t.scatter.get_mut(k)) {
                r.models = vec![model];
            }
        });
    }

    pub fn scatter_rotate(&mut self, k: usize, rotate: crate::terrain::Rotate) {
        self.change(None, |l| {
            if let Some(r) = l.terrain.as_mut().and_then(|t| t.scatter.get_mut(k)) {
                r.rotate = Some(rotate);
            }
        });
    }

    pub fn scatter_as(&mut self, k: usize, makes: crate::terrain::ScatterAs) {
        self.change(None, |l| {
            if let Some(r) = l.terrain.as_mut().and_then(|t| t.scatter.get_mut(k)) {
                r.as_ = Some(makes);
            }
        });
    }

    /// A rule's condition (0 ABOVE, 1 BELOW, 2 STEEPER, 3 FLATTER) on or
    /// off.
    pub fn scatter_condition(&mut self, k: usize, which: u8, on: bool) {
        self.change(None, |l| {
            let Some(r) = l.terrain.as_mut().and_then(|t| t.scatter.get_mut(k)) else { return };
            match (which, on) {
                (0, true) => r.above = Some(r.above.unwrap_or(40)),
                (1, true) => r.below = Some(r.below.unwrap_or(60)),
                (2, true) => r.steeper = Some(r.steeper.unwrap_or(30.0)),
                (3, true) => r.flatter = Some(r.flatter.unwrap_or(30.0)),
                (0, false) => r.above = None,
                (1, false) => r.below = None,
                (2, false) => r.steeper = None,
                _ => r.flatter = None,
            }
        });
    }

    /// A step that changed the land's surface: the things and places that
    /// stood on it (their feet on the land as it was, within a voxel)
    /// moved with it, in that step; what is set in the air or sunk on
    /// purpose stays.
    pub(super) fn follow_land(&mut self, step: &mut Step) {
        let Step::Level(before, after) = step else { return };
        // (Its own still: the step is being taken.)
        let after = Arc::make_mut(after);
        let (Some(b), Some(a)) = (before.terrain.as_deref(), after.terrain.as_deref()) else { return };
        let same_surface = (&b.heights, &b.shapes, b.spacing, b.at, b.cols, b.rows, b.terrace, b.rough, &b.roughness, &b.layers, b.seed) == (&a.heights, &a.shapes, a.spacing, a.at, a.cols, a.rows, a.terrace, a.rough, &a.roughness, &a.layers, a.seed);
        if same_surface {
            return;
        }
        let ground = |t: &Terrain, at: [i32; 3]| {
            let c = IVec2::new(at[0] - t.at[0], at[1] - t.at[1]);
            let size = t.size();
            (c.x >= 0 && c.y >= 0 && c.x < size.x && c.y < size.y).then(|| t.at[2] + t.height_at(c))
        };
        let mut moved = false;
        let mut follow = |at: &mut [i32; 3]| {
            if let (Some(was), Some(now)) = (ground(b, *at), ground(a, *at))
                && was != now
                && (at[2] - was).abs() <= 1
            {
                at[2] += now - was;
                moved = true;
            }
        };
        for t in &mut after.things {
            follow(&mut t.at);
        }
        for p in &mut after.places {
            follow(&mut p.at);
        }
        if moved {
            self.level.things = after.things.clone();
            self.level.places = after.places.clone();
        }
    }

    /// REROLL: another seed (the next of the old one's), the land's
    /// unevenness, patches and grain anew; a step.
    pub fn reroll(&mut self) {
        self.change(None, |l| {
            if let Some(t) = l.terrain.as_mut() {
                let mut z = t.seed.unwrap_or(0).wrapping_add(0x9E37_79B9_7F4A_7C15);
                z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                t.seed = Some((z ^ (z >> 31)) % 100_000);
            }
        });
    }

    /// LAYERS' plus: a band of rock more on top (hard, a wall), or a
    /// first one where there were none.
    pub fn add_layer(&mut self) {
        self.change(None, |l| {
            if let Some(t) = l.terrain.as_mut() {
                let hard = t.layers.last().is_none_or(|l| l.hard < 0.5);
                t.layers.push(crate::terrain::Layer { thick: 8, hard: if hard { 0.9 } else { 0.2 }, color: if hard { "#b8603a".into() } else { "#d98a5a".into() } });
            }
        });
    }

    /// × of a layer.
    pub fn remove_layer(&mut self, k: u8) {
        self.change(None, |l| {
            if let Some(t) = l.terrain.as_mut()
                && (k as usize) < t.layers.len()
            {
                t.layers.remove(k as usize);
            }
        });
    }

    /// A layer's row dragged onto another's place.
    pub fn move_layer(&mut self, from: u8, to: u8) {
        self.change(None, |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            let (from, to) = (from as usize, to as usize);
            if from < t.layers.len() && to < t.layers.len() && from != to {
                let m = t.layers.remove(from);
                t.layers.insert(to, m);
            }
        });
    }

    /// A layer's color.
    pub fn layer_colour(&mut self, k: u8, c: [u8; 3]) {
        let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        self.change(None, |l| {
            if let Some(m) = l.terrain.as_mut().and_then(|t| t.layers.get_mut(k as usize)) {
                m.color = hex;
            }
        });
    }

    /// A material's tone (0 its color, then the others) set, one more
    /// (up to three: a little darker than its color) or the last taken
    /// out.
    pub fn tone(&mut self, k: u8, tone: u8, c: [u8; 3]) {
        let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        self.change(None, |l| {
            if let Some(m) = l.terrain.as_mut().and_then(|t| t.materials.get_mut(k as usize)) {
                match tone {
                    0 => m.color = hex,
                    n => {
                        if let Some(t) = m.tones.get_mut(n as usize - 1) {
                            *t = hex;
                        }
                    }
                }
            }
        });
    }

    pub fn add_tone(&mut self, k: u8) {
        self.change(None, |l| {
            if let Some(m) = l.terrain.as_mut().and_then(|t| t.materials.get_mut(k as usize))
                && m.tones.len() < 2
            {
                // (Close to its color: patches read as variation.)
                let [r, g, b] = m.rgb().map(|v| (v as f64 * 0.93).round() as u8);
                m.tones.push(format!("#{r:02x}{g:02x}{b:02x}"));
            }
        });
    }

    pub fn remove_tone(&mut self, k: u8) {
        self.change(None, |l| {
            if let Some(m) = l.terrain.as_mut().and_then(|t| t.materials.get_mut(k as usize)) {
                m.tones.pop();
            }
        });
    }

    /// MATERIALS' plus: a new material first (rock where steep), so the
    /// last stays the one for the rest.
    pub fn add_material(&mut self) {
        self.change(None, |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            if t.materials.len() < MATERIALS {
                t.materials.insert(0, Material { steeper: Some(45.0), ..Material::new("#7d7468") });
            }
        });
        self.land.row = Some(LandRow::Material(0));
    }

    /// × of a material (not the last one left).
    pub fn remove_material(&mut self, k: u8) {
        self.change(None, |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            if t.materials.len() > 1 && (k as usize) < t.materials.len() {
                t.materials.remove(k as usize);
                clear_last(t);
            }
        });
    }

    /// A material's row dragged onto another's place.
    pub fn move_material(&mut self, from: u8, to: u8) {
        self.change(None, |l| {
            let Some(t) = l.terrain.as_mut() else { return };
            let (from, to) = (from as usize, to as usize);
            if from < t.materials.len() && to < t.materials.len() && from != to {
                let m = t.materials.remove(from);
                t.materials.insert(to, m);
                clear_last(t);
            }
        });
    }
}

/// The last material paints whatever no other does: its conditions go.
fn clear_last(t: &mut Terrain) {
    if let Some(m) = t.materials.last_mut() {
        (m.above, m.below, m.steeper, m.flatter) = (None, None, None, None);
    }
}

/// The grid made `cols` by `rows` points: rows and columns added or taken
/// at the far and the right edge, the heights kept; a new point as high as
/// the one before it on the edge.
pub fn resize(t: &mut Terrain, cols: i32, rows: i32) {
    if (cols, rows) == (t.cols, t.rows) {
        return;
    }
    let mut heights = Vec::with_capacity((cols * rows) as usize);
    let mut shapes = Vec::with_capacity((cols * rows) as usize);
    let mut rough = Vec::with_capacity((cols * rows) as usize);
    for y in 0..rows {
        for x in 0..cols {
            let i = (x.min(t.cols - 1) + y.min(t.rows - 1) * t.cols) as usize;
            heights.push(t.heights[i]);
            shapes.push(if x < t.cols && y < t.rows { t.shapes[i] } else { Shape::Smooth });
            rough.push(t.roughness.as_ref().map_or(9, |r| r[i]));
        }
    }
    (t.cols, t.rows, t.heights, t.shapes) = (cols, rows, heights, shapes);
    if t.roughness.is_some() {
        t.roughness = Some(rough);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::tests::folder;

    /// A ray from the front, level, at height `z`: the pointer that high
    /// on any point's vertical.
    fn eye(z: f32) -> (Vec3, Vec3) {
        (Vec3::new(0.0, -500.0, z), Vec3::Y)
    }

    /// A level of a flat land opened in the editor.
    fn land(name: &str) -> (Editor, crate::scratch::Scratch) {
        let dir = folder(name);
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        let text = "format = \"voxcade level 1\"\nsize = [160, 96, 80]\n\n# the land\n[terrain]\nspacing = 16\nheights = \"\"\"\n4 4 4 4 4 4 4 4 4 4\n4 4 4 4 4 4 4 4 4 4\n4 4 4 4 4 4 4 4 4 4\n4 4 4 4 4 4 4 4 4 4\n4 4 4 4 4 4 4 4 4 4\n4 4 4 4 4 4 4 4 4 4\n\"\"\"\n\n[[terrain.material]]\ncolor = \"#5c8a3a\"\n";
        std::fs::write(dir.join("levels/land.toml"), text).unwrap();
        let mut ed = Editor::open(&dir, "levels/land.toml").unwrap();
        ed.switch(Layer::Terrain);
        (ed, dir)
    }

    fn file(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("levels/land.toml")).unwrap()
    }

    /// The view's marks as a window paints them over the land, at 1600 by
    /// 900: the strokes' and fills' colors.
    fn painted(ed: &Editor, view: &crate::editor::view::View) -> Vec<egui::Color32> {
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0));
        let figures = crate::editor::view::Figures::load(&ed.dir, &crate::config::GameConfig::default());
        let raw = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
        let out = ctx.run_ui(raw, |ui| crate::editor::panels::marks(ui, ed, view, &figures, screen));
        out.shapes
            .iter()
            .flat_map(|c| match &c.shape {
                egui::Shape::LineSegment { stroke, .. } => vec![stroke.color],
                egui::Shape::Rect(r) => vec![r.fill],
                _ => Vec::new(),
            })
            .collect()
    }

    /// TERRAIN shows every point of the land faintly, not only those near
    /// the mouse, and the view has a quiet axis marker (X red, Y green, Z
    /// blue, the handles' colors); GAME VIEW shows what the game shows,
    /// no marker.
    #[test]
    fn the_lands_points_show_faintly_and_the_axes_in_the_corner() {
        let (ed, _dir) = land("land-faint");
        let mut view = crate::editor::view::View::new(&ed, &crate::config::GameConfig::default());
        let colors = painted(&ed, &view);
        let t = ed.terrain().unwrap();
        let faint = colors.iter().filter(|c| **c == egui::Color32::from_rgba_unmultiplied(24, 23, 30, 120)).count();
        assert_eq!(faint, (t.cols * t.rows) as usize, "every point, faintly");
        let axes = |colors: &[egui::Color32]| crate::editor::zones::AXIS.iter().filter(|a| colors.contains(&egui::Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 170))).count();
        assert_eq!(axes(&colors), 3, "X, Y and Z in the corner");
        view.before_game = Some(view.level);
        assert_eq!(axes(&painted(&ed, &view)), 0, "none in GAME VIEW");
    }

    /// A point raised with the brush: a hill, not a spike, one step and
    /// one UNDO; only the heights' lines change, the comment kept.
    #[test]
    fn a_point_raised_with_the_brush_makes_a_hill() {
        let (mut ed, dir) = land("land-raise");
        let before = file(&dir);
        ed.land.radius = 3;
        ed.land_press(Some(IVec2::new(4, 3)), eye(4.0));
        ed.land_drag(Some(IVec2::new(4, 3)), eye(24.0), false);
        ed.land_drag(Some(IVec2::new(4, 3)), eye(34.0), false);
        assert_eq!(file(&dir), before, "nothing written while dragging");
        ed.release();
        let t = ed.terrain().unwrap();
        let h = |x: i32, y: i32| t.heights[t.index(IVec2::new(x, y)).unwrap()];
        assert_eq!(h(4, 3), 34, "the grabbed point up 30");
        assert!(h(5, 3) > 4 && h(5, 3) < 34 && h(6, 3) < h(5, 3), "the brush round it falls off: {} {}", h(5, 3), h(6, 3));
        assert_eq!(h(0, 0), 4, "far away untouched");
        assert_eq!(ed.steps(), 1);
        let after = file(&dir);
        assert!(after.contains("# the land\n[terrain]\nspacing = 16\n"), "{after}");
        assert_eq!(crate::grid::GridLevel::from_text(&after).unwrap().terrain.as_ref(), ed.terrain());
        ed.undo();
        assert_eq!(file(&dir), before);
    }

    /// FLATTEN takes the first point's height, PAINT the shape held, the
    /// selection moves together and takes a typed HEIGHT; the grid grows
    /// at its far and right edge, the heights kept; materials come, go and
    /// change order, the last one never with conditions.
    #[test]
    fn the_terrains_tools_and_fields() {
        let (mut ed, _dir) = land("land-tools");
        ed.land_set(LandField::Height, 0.0);
        ed.land.chosen = Some(IVec2::new(0, 0));
        ed.land_set(LandField::Height, 20.0);
        assert_eq!(ed.terrain().unwrap().heights[0], 20);
        // Drawn: the chosen one and those near the pointer, not the rest.
        (ed.land.near, ed.land.radius) = (Some(IVec2::new(5, 3)), 1);
        assert!(ed.point_shown(IVec2::new(0, 0)) && ed.point_shown(IVec2::new(7, 5)) && !ed.point_shown(IVec2::new(9, 0)));
        ed.land.tool = LandTool::Flatten;
        ed.land.radius = 2;
        ed.land_press(Some(IVec2::new(0, 0)), eye(0.0));
        ed.land_drag(Some(IVec2::new(1, 0)), eye(0.0), false);
        ed.release();
        let t = ed.terrain().unwrap();
        assert!(t.heights[1] > 4 && t.heights[2] > 4, "flattened toward 20: {:?}", &t.heights[..4]);
        ed.land.tool = LandTool::Paint;
        ed.land.shape = Shape::Step;
        ed.land.radius = 1;
        ed.land_press(Some(IVec2::new(3, 3)), eye(0.0));
        ed.release();
        let t = ed.terrain().unwrap();
        assert_eq!(t.shapes[t.index(IVec2::new(3, 3)).unwrap()], Shape::Step);
        assert_eq!(t.shapes.iter().filter(|s| **s == Shape::Step).count(), 1, "radius 1: the point alone");
        // A selection raised together, then a HEIGHT typed for it.
        ed.land.tool = LandTool::Pointer;
        ed.land.selected = vec![IVec2::new(7, 1), IVec2::new(8, 1)];
        ed.land_press(Some(IVec2::new(7, 1)), eye(4.0));
        ed.land_drag(Some(IVec2::new(7, 1)), eye(14.0), false);
        ed.release();
        let t = ed.terrain().unwrap();
        assert_eq!((t.heights[t.index(IVec2::new(7, 1)).unwrap()], t.heights[t.index(IVec2::new(8, 1)).unwrap()], t.heights[t.index(IVec2::new(6, 1)).unwrap()]), (14, 14, 4));
        ed.land_set(LandField::Height, 9.0);
        let t = ed.terrain().unwrap();
        assert_eq!(t.heights[t.index(IVec2::new(8, 1)).unwrap()], 9);
        // The grid: within the level (160 / 16 = 10 columns at most).
        ed.land_set(LandField::Rows, 5.0);
        ed.land_set(LandField::Cols, 12.0);
        let t = ed.terrain().unwrap();
        assert_eq!((t.cols, t.rows), (10, 5));
        assert_eq!(t.heights[0], 20);
        // Materials.
        ed.add_material();
        ed.land_condition(1, 0, true);
        assert!(ed.terrain().unwrap().materials[1].unconditional(), "the last takes no condition");
        ed.move_material(0, 1);
        let t = ed.terrain().unwrap();
        assert_eq!(t.materials[0].color, "#5c8a3a");
        assert!(t.materials[1].unconditional(), "moved last: its conditions gone");
        ed.land_colour(0, [10, 20, 30]);
        ed.land_colour(2, [1, 2, 3]);
        let t = ed.terrain().unwrap();
        assert_eq!((t.materials[0].color.as_str(), t.under.as_deref()), ("#0a141e", Some("#010203")));
        ed.remove_material(0);
        assert_eq!(ed.terrain().unwrap().materials.len(), 1);
    }

    /// ROUGHEN on even land makes it able to be rough where brushed, CALM
    /// takes it back; REROLL a new seed in one step; layers and tones come,
    /// change and go, written as the file says them.
    #[test]
    fn roughen_reroll_layers_and_tones() {
        let (mut ed, dir) = land("land-rough");
        ed.land.tool = LandTool::Roughen;
        ed.land.radius = 2;
        ed.land_press(Some(IVec2::new(5, 3)), eye(0.0));
        ed.land_drag(Some(IVec2::new(6, 3)), eye(0.0), false);
        ed.release();
        let t = ed.terrain().unwrap();
        let r = t.roughness.as_ref().unwrap();
        assert_eq!(t.rough, Some(0.5));
        assert!(r[t.index(IVec2::new(5, 3)).unwrap()] >= 4 && r[0] == 0, "rough where brushed, calm elsewhere");
        ed.land.tool = LandTool::Calm;
        for _ in 0..3 {
            ed.land_press(Some(IVec2::new(5, 3)), eye(0.0));
            ed.release();
        }
        let t = ed.terrain().unwrap();
        assert_eq!(t.roughness.as_ref().unwrap()[t.index(IVec2::new(5, 3)).unwrap()], 0);
        let steps = ed.steps();
        ed.reroll();
        let seed = ed.terrain().unwrap().seed;
        assert!(seed.is_some() && ed.steps() == steps + 1);
        ed.reroll();
        assert_ne!(ed.terrain().unwrap().seed, seed);
        ed.add_layer();
        ed.add_layer();
        ed.land_set(LandField::Thick(1), 14.0);
        ed.land_set(LandField::Hard(0), 0.75);
        ed.layer_colour(1, [1, 2, 3]);
        ed.move_layer(1, 0);
        ed.add_tone(0);
        ed.tone(0, 1, [9, 9, 9]);
        ed.land_set(LandField::Patch(0), 4.0);
        ed.land_condition(0, 0, false);
        let text = file(&dir);
        let back = crate::grid::GridLevel::from_text(&text).unwrap().terrain.unwrap();
        assert_eq!(Some(&back), ed.terrain(), "{text}");
        assert_eq!((back.layers[0].thick, back.layers[0].color.as_str(), back.layers[1].hard), (14, "#010203", 0.75));
        assert_eq!(back.materials[0].tones, ["#090909"]);
        assert!(text.contains("roughness = \"\"\"\n"), "{text}");
        ed.remove_tone(0);
        ed.remove_layer(0);
        assert!(ed.terrain().unwrap().materials[0].tones.is_empty() && ed.terrain().unwrap().layers.len() == 1);
    }

    /// SCATTER sets a rule's pieces into the level once, marked as its, one
    /// step; quiet while they are there; CLEAR takes them away (a moved one
    /// too) and SCATTER can go again; a rule taken out leaves its pieces,
    /// unmarked, and the later rules' marks follow their numbers.
    #[test]
    fn scatter_sets_pieces_once_and_clear_takes_them() {
        let (mut ed, dir) = land("land-scatter");
        std::fs::write(dir.join("fir.vvx"), "format = \"voxcade voxel 1\"\nsize = [3, 3, 5]\n\n[palette]\na = { color = \"#2f5d3a\" }\n\n[[frame]]\nlayers = \"\"\"\naaa *3\n*5\n\"\"\"\n").unwrap();
        ed.add_scatter();
        ed.add_scatter();
        ed.scatter_model(0, "fir.vvx".into());
        ed.land_set(LandField::Density(0), 6.0);
        ed.land_set(LandField::Sink(0), 1.0);
        let steps = ed.steps();
        ed.scatter_pieces(0);
        let n = ed.scattered(0);
        assert!(n > 3 && ed.steps() == steps + 1, "{n} pieces, one step");
        let t = ed.level.things.iter().find(|t| t.scatter == Some(1)).unwrap().clone();
        assert_eq!((t.at[2], t.model.as_deref(), t.name.as_deref()), (3, Some("fir.vvx"), Some("fir_1")), "on the land, sunk a voxel, named on");
        let text = file(&dir);
        assert!(text.contains("model = \"fir.vvx\"\nat = ") && text.contains("scatter = 1\n") && text.contains("[[terrain.scatter]]"), "{text}");
        ed.scatter_pieces(0);
        assert_eq!((ed.scattered(0), ed.steps()), (n, steps + 1), "nothing while its pieces are there");
        assert!(ed.hint.take().is_some_and(|h| h.contains("CLEAR first")));
        // A moved one is still the rule's.
        let i = ed.level.things.iter().position(|t| t.scatter == Some(1)).unwrap();
        ed.change_level(|l| l.things[i].at[0] += 7);
        ed.clear_scatter(0);
        assert_eq!(ed.scattered(0), 0);
        ed.scatter_pieces(0);
        assert_eq!(ed.scattered(0), n, "again the same");
        // Rule 1 out: its pieces stay, plain.
        ed.move_scatter(0, 1);
        assert_eq!(ed.scattered(1), n, "the marks follow the rule");
        ed.remove_scatter(1);
        assert!(ed.level.things.iter().all(|t| t.scatter.is_none()) && ed.level.things.len() >= n);
    }

    /// Pieces standing on the land move with it when it is raised, in the
    /// same step; one set in the air stays; UNDO puts both back.
    #[test]
    fn pieces_on_the_land_follow_it() {
        let (mut ed, _dir) = land("land-follow");
        let spot = ed.point_spot(IVec2::new(4, 3)).unwrap();
        let (x, y) = (spot.x as i32, spot.y as i32);
        ed.change_level(|l| {
            l.things.push(crate::grid::Thing { model: Some("fir.vvx".into()), name: Some("on".into()), ..crate::grid::Thing::new(crate::grid::PIECE, [x, y, 4]) });
            l.things.push(crate::grid::Thing { model: Some("fir.vvx".into()), name: Some("air".into()), ..crate::grid::Thing::new(crate::grid::PIECE, [x + 1, y, 30]) });
        });
        let steps = ed.steps();
        ed.land.chosen = Some(IVec2::new(4, 3));
        ed.land_set(LandField::Height, 20.0);
        let z = |ed: &Editor, name: &str| ed.level.things.iter().find(|t| t.name.as_deref() == Some(name)).unwrap().at[2];
        assert_eq!((z(&ed, "on"), z(&ed, "air"), ed.steps()), (20, 30, steps + 1), "on the land up with it, in the air stays, one step");
        // A brush stroke too.
        ed.land.radius = 1;
        ed.land_press(Some(IVec2::new(4, 3)), eye(20.0));
        ed.land_drag(Some(IVec2::new(4, 3)), eye(25.0), false);
        ed.release();
        assert_eq!(z(&ed, "on"), 25);
        ed.undo();
        ed.undo();
        assert_eq!((z(&ed, "on"), z(&ed, "air")), (4, 30));
    }

    /// The land shown in the view, built again only where the points
    /// changed, is the land built whole: raised, lowered, a shape.
    #[test]
    fn the_land_built_again_in_part_is_the_land_built_whole() {
        let (mut ed, _dir) = land("land-part");
        let mut shown = crate::editor::view::Shown::new(&ed).unwrap();
        for (p, by) in [(IVec2::new(4, 3), 30), (IVec2::new(4, 3), -2), (IVec2::new(9, 0), 12)] {
            let from = ed.terrain().unwrap().heights.clone();
            ed.raise_from(&from, p, by);
            assert!(!shown.update(&ed).unwrap(), "built again in part");
            assert_eq!(shown.world.content_hash(), crate::editor::view::Shown::new(&ed).unwrap().world.content_hash(), "after {p} by {by}");
        }
        ed.level.terrain.as_mut().unwrap().shapes[5] = Shape::Step;
        assert!(!shown.update(&ed).unwrap());
        assert_eq!(shown.world.content_hash(), crate::editor::view::Shown::new(&ed).unwrap().world.content_hash(), "a shape");
        // A material changed: built whole.
        ed.level.terrain.as_mut().unwrap().materials[0].color = "#112233".into();
        assert!(shown.update(&ed).unwrap());
    }

    /// A model added where a ray from above meets the land stands on it;
    /// set down, it stays on the mouse, the next of its name, until let
    /// go of (a stamp: firs by the dozen).
    #[test]
    fn a_model_added_stands_on_the_land_and_stays_on_the_mouse() {
        let (mut ed, dir) = land("land-add-model");
        std::fs::write(dir.join("fir.vvx"), "format = \"voxcade voxel 1\"\nsize = [3, 3, 5]\n\n[palette]\na = { color = \"#2f5d3a\" }\n\n[[frame]]\nlayers = \"\"\"\naaa *3\n*5\n\"\"\"\n").unwrap();
        ed.switch(Layer::Pieces);
        let from = ed.terrain().unwrap().heights.clone();
        ed.raise_from(&from, IVec2::new(2, 2), 20);
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        let figures = crate::editor::view::Figures::load(&dir, &crate::config::GameConfig::default()).with_pieces(&dir, &ed.level);
        let spot = |x: f32, y: f32| crate::editor::view::set_down_spot(&ed, &figures, &shown.world, Vec3::new(x, y, 75.0), Vec3::new(0.0, 0.0, -1.0)).unwrap();
        let top = |x: i32, y: i32| ed.terrain().unwrap().surface().at(IVec2::new(x, y)).unwrap();
        let (a, b) = (spot(40.5, 40.5), spot(100.5, 20.5));
        assert_eq!((a.z, b.z), (top(40, 40), top(100, 20)), "on the land where the ray meets it");
        assert!(a.z > 4, "on the hill");
        ed.add_piece("fir.vvx");
        ed.hang_at(Some(a));
        assert!(ed.stamp());
        ed.hang_at(Some(b));
        assert!(ed.stamp());
        let names: Vec<_> = ed.level.things.iter().map(|t| (t.name.clone().unwrap(), t.at)).collect();
        assert_eq!(names, [("fir".to_string(), a.to_array()), ("fir_2".to_string(), b.to_array())]);
        assert!(matches!(&ed.pieces.hanging, Some(super::super::room::Hanging::Thing(t)) if t.name.as_deref() == Some("fir_3")), "the next on the mouse");
        assert_eq!(ed.steps(), 2, "a step a fir");
        assert!(ed.drop_hanging());
    }

    /// TERRAIN on a level of pieces without one: a flat land as big as
    /// the level; REMOVE TERRAIN takes it out again.
    #[test]
    fn terrain_is_added_and_removed() {
        let dir = folder("land-add");
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        std::fs::write(dir.join("levels/room.toml"), "format = \"voxcade level 1\"\nsize = [100, 60, 40]\n").unwrap();
        let mut ed = Editor::open(&dir, "levels/room.toml").unwrap();
        ed.add_terrain();
        assert!(ed.among_land());
        let t = ed.terrain().unwrap();
        assert_eq!((t.spacing, t.cols, t.rows, t.heights[0]), (15, 6, 4, 5));
        let text = std::fs::read_to_string(dir.join("levels/room.toml")).unwrap();
        assert!(text.contains("[terrain]") && text.contains("[[terrain.material]]"), "{text}");
        ed.remove_terrain();
        assert!(ed.terrain().is_none() && !ed.among_land());
    }
}
