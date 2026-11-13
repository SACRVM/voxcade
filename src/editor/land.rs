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
use super::{Editor, Layer, Scope, Step};
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
    Above(u8),
    Below(u8),
    Steeper(u8),
    Flatter(u8),
    Depth(u8),
    Grain(u8),
}

/// The editor's state among the points.
#[derive(Clone, Debug)]
pub struct Land {
    pub tool: LandTool,
    /// The brush's radius, in points (1: the point alone).
    pub radius: i32,
    /// The shape PAINT lays and the SHAPE buttons show.
    pub shape: Shape,
    /// The point chosen last, and the points selected.
    pub chosen: Option<IVec2>,
    pub selected: Vec<IVec2>,
    pub mode: SelectMode,
    /// The point under the mouse.
    pub hover: Option<IVec2>,
    drag: Option<Drag>,
}

impl Default for Land {
    fn default() -> Self {
        Self { tool: LandTool::Pointer, radius: 3, shape: Shape::Smooth, chosen: None, selected: Vec::new(), mode: SelectMode::New, hover: None, drag: None }
    }
}

/// A press among the points held: the point grabbed, the heights as they
/// were, where on the screen it began and how far a voxel up is there
/// (pixels), FLATTEN's height, the point brushed last.
#[derive(Clone, Debug)]
struct Drag {
    grabbed: IVec2,
    from: Vec<i32>,
    press: Vec2,
    up: Vec2,
    target: i32,
    last: Option<IVec2>,
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

    /// A press among the points (`p` under the mouse): POINTER chooses it
    /// and grabs it (`up`: a voxel up on the screen there, in pixels);
    /// FLATTEN, SMOOTH and PAINT brush there. Nothing under it: none
    /// chosen, none selected.
    pub fn land_press(&mut self, p: Option<IVec2>, mouse: Vec2, up: Vec2) {
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
                self.stroke = Some(Step::Level(self.level_state(), self.level_state()));
                self.land.drag = Some(Drag { grabbed: p, from, press: mouse, up, target: 0, last: None });
            }
            (_, p) => {
                let target = height;
                self.stroke = Some(Step::Level(self.level_state(), self.level_state()));
                self.land.drag = Some(Drag { grabbed: p.unwrap_or_default(), from, press: mouse, up, target, last: None });
                if let Some(p) = p {
                    self.brush(p);
                }
            }
        }
    }

    /// The mouse dragged on (`p` under it): POINTER's point up or down by
    /// how far the mouse went along a voxel up, in whole voxels; a brush
    /// over the point under it.
    pub fn land_drag(&mut self, p: Option<IVec2>, mouse: Vec2) {
        let Some(d) = self.land.drag.clone() else { return };
        if self.land.tool == LandTool::Pointer {
            let len = d.up.length_squared();
            let by = if len < 1e-6 { 0 } else { ((mouse - d.press).dot(d.up) / len).round() as i32 };
            self.raise_from(&d.from, d.grabbed, by);
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
                        l.hard = ((v * 100.0).round() / 100.0).clamp(0.0, 1.0);
                    }
                }
                LandField::Patch(k) => {
                    if let Some(m) = t.materials.get_mut(k as usize) {
                        m.patch = Some(whole.clamp(1, 1000));
                    }
                }
                LandField::Blend(k) => {
                    if let Some(m) = t.materials.get_mut(k as usize) {
                        let b = ((v * 10.0).round() / 10.0).clamp(0.0, 1000.0);
                        m.blend = (b > 0.0).then_some(b);
                    }
                }
                LandField::Terrace => t.terrace = Some(whole.clamp(1, HIGHEST)),
                LandField::Rough => t.rough = Some(((v * 100.0).round() / 100.0).clamp(0.0, 1.0)),
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
                        let deg = (v * 10.0).round() / 10.0;
                        let deg = deg.clamp(0.0, 89.9);
                        match field {
                            LandField::Steeper(_) => t.materials[k].steeper = Some(deg),
                            _ => t.materials[k].flatter = Some(deg),
                        }
                    }
                }
                LandField::Grain(k) => {
                    if let Some(k) = m(k, t) {
                        let g = ((v * 100.0).round() / 100.0).clamp(0.0, 1.0);
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

/// Where a point's arrow up goes on the screen: pixels for a voxel up at
/// `at`.
pub fn up_on_screen(view_proj: Mat4, screen: Vec2, at: Vec3) -> Vec2 {
    let p = super::super::handles::project(view_proj, screen, at);
    let q = super::super::handles::project(view_proj, screen, at + Vec3::Z);
    match (p, q) {
        (Some((p, _)), Some((q, _))) => q - p,
        _ => Vec2::new(0.0, -1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::tests::folder;

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

    /// A point raised with the brush: a hill, not a spike, one step and
    /// one UNDO; only the heights' lines change, the comment kept.
    #[test]
    fn a_point_raised_with_the_brush_makes_a_hill() {
        let (mut ed, dir) = land("land-raise");
        let before = file(&dir);
        ed.land.radius = 3;
        ed.land_press(Some(IVec2::new(4, 3)), Vec2::ZERO, Vec2::new(0.0, -2.0));
        ed.land_drag(Some(IVec2::new(4, 3)), Vec2::new(0.0, -40.0));
        ed.land_drag(Some(IVec2::new(4, 3)), Vec2::new(0.0, -60.0));
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
        ed.land.tool = LandTool::Flatten;
        ed.land.radius = 2;
        ed.land_press(Some(IVec2::new(0, 0)), Vec2::ZERO, Vec2::Y);
        ed.land_drag(Some(IVec2::new(1, 0)), Vec2::ZERO);
        ed.release();
        let t = ed.terrain().unwrap();
        assert!(t.heights[1] > 4 && t.heights[2] > 4, "flattened toward 20: {:?}", &t.heights[..4]);
        ed.land.tool = LandTool::Paint;
        ed.land.shape = Shape::Step;
        ed.land.radius = 1;
        ed.land_press(Some(IVec2::new(3, 3)), Vec2::ZERO, Vec2::Y);
        ed.release();
        let t = ed.terrain().unwrap();
        assert_eq!(t.shapes[t.index(IVec2::new(3, 3)).unwrap()], Shape::Step);
        assert_eq!(t.shapes.iter().filter(|s| **s == Shape::Step).count(), 1, "radius 1: the point alone");
        // A selection raised together, then a HEIGHT typed for it.
        ed.land.tool = LandTool::Pointer;
        ed.land.selected = vec![IVec2::new(7, 1), IVec2::new(8, 1)];
        ed.land_press(Some(IVec2::new(7, 1)), Vec2::ZERO, Vec2::new(0.0, -1.0));
        ed.land_drag(Some(IVec2::new(7, 1)), Vec2::new(0.0, -10.0));
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
        ed.land_press(Some(IVec2::new(5, 3)), Vec2::ZERO, Vec2::Y);
        ed.land_drag(Some(IVec2::new(6, 3)), Vec2::ZERO);
        ed.release();
        let t = ed.terrain().unwrap();
        let r = t.roughness.as_ref().unwrap();
        assert_eq!(t.rough, Some(0.5));
        assert!(r[t.index(IVec2::new(5, 3)).unwrap()] >= 4 && r[0] == 0, "rough where brushed, calm elsewhere");
        ed.land.tool = LandTool::Calm;
        for _ in 0..3 {
            ed.land_press(Some(IVec2::new(5, 3)), Vec2::ZERO, Vec2::Y);
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
        assert!(matches!(&ed.hanging, Some(super::super::room::Hanging::Thing(t)) if t.name.as_deref() == Some("fir_3")), "the next on the mouse");
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
