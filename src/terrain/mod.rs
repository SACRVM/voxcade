//! A landscape from a grid of heights: a level's `[terrain]` (`docs/tiles.md`).
//!
//! Control points stand `spacing` voxels apart, each in the middle of its
//! own square of `spacing` by `spacing` columns (the first square's low
//! corner at `at`); `heights` holds them in voxels, the far row first as a
//! level seen from above reads. Between the points a column's height goes
//! by their shapes: SMOOTH (`.`) a Catmull-Rom curve through the points,
//! bicubic, held within the four points round it so it never overshoots;
//! LINEAR (`^`) straight slopes between the four (bilinear); STEP (`#`)
//! the smooth land cut into terraces `terrace` voxels high: flat ledges
//! and a steep riser with a little foot where the land crosses the next
//! multiple, so the cliffs follow the land's contours (they curve with a
//! valley, stack into banded walls, ring a butte), their lines wandering
//! a few voxels from `seed` (`rough`, 0 clean). Each shape weighs in by
//! the four points round a column, each by its nearness (bilinear
//! weights): SMOOTH and LINEAR land meet without a seam, a meadow runs
//! into a cliff. A column is filled from the world's floor up to its
//! height: the land stands on the ground by making, with no overhangs, so
//! nothing of it falls loose.
//!
//! Its colours are rules (`[[terrain.material]]`): the first whose
//! conditions all hold paints a voxel (`above`, `below`: the voxel's height
//! over `at`; `steeper`, `flatter`: the slope of its column in degrees),
//! `depth` voxels down from where the voxel can be seen, `under` below.
//! `grain` shades a material a little voxel by voxel from `seed`. Every
//! number is whole or a plain sum and product of whole numbers (the same
//! bits on every system); the one tangent is `libm`'s.

use anyhow::{Result, anyhow, bail};
use glam::{IVec2, IVec3};
use toml_edit::Item;

use crate::vvx::{self, Reader};
use crate::world::{CHUNK, World};

mod text;
pub use text::*;

/// How the land goes between a point and its neighbours (the words of
/// glTF's and the animation tools' interpolation).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    /// A smooth curve through the points: round hills.
    #[default]
    Smooth,
    /// Straight slopes from point to point: ridges and peaks.
    Linear,
    /// The point's height over its whole square: terraces and cliffs.
    Step,
}

impl Shape {
    pub const ALL: [Shape; 3] = [Shape::Smooth, Shape::Linear, Shape::Step];

    pub fn sign(self) -> char {
        match self {
            Shape::Smooth => '.',
            Shape::Linear => '^',
            Shape::Step => '#',
        }
    }

    pub fn of_sign(c: char) -> Option<Shape> {
        Shape::ALL.into_iter().find(|s| s.sign() == c)
    }

    pub fn name(self) -> &'static str {
        match self {
            Shape::Smooth => "SMOOTH",
            Shape::Linear => "LINEAR",
            Shape::Step => "STEP",
        }
    }
}

/// One rule of the land's colours: its color where every condition given
/// holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// `#rrggbb`, as the file writes it.
    pub color: String,
    /// The voxel higher than this over the terrain's base.
    pub above: Option<i32>,
    /// The voxel lower than this.
    pub below: Option<i32>,
    /// Its column's slope more than this many degrees.
    pub steeper: Option<f64>,
    /// Its column's slope less than this many degrees.
    pub flatter: Option<f64>,
    /// How many voxels down from where it can be seen it goes (3).
    pub depth: Option<i32>,
    /// A slight shade voxel by voxel, 0 to 1.
    pub grain: Option<f64>,
    /// More tones beside `color` (`colors = [...]` in the file: two or
    /// three), mixed in patches about `patch` voxels across.
    pub tones: Vec<String>,
    pub patch: Option<i32>,
    /// How far its conditions' borders wander: voxels of height for
    /// `above` and `below`, degrees for `steeper` and `flatter`.
    pub blend: Option<f64>,
}

impl Material {
    pub fn new(color: &str) -> Self {
        Self { color: color.to_string(), above: None, below: None, steeper: None, flatter: None, depth: None, grain: None, tones: Vec::new(), patch: None, blend: None }
    }

    /// Its tones: `color` first, then the others.
    pub fn all_tones(&self) -> Vec<[u8; 3]> {
        std::iter::once(&self.color).chain(&self.tones).map(|c| crate::grid::hex(c).unwrap_or([128, 128, 128])).collect()
    }

    /// Whether it holds everywhere (the last rule must).
    pub fn unconditional(&self) -> bool {
        self.above.is_none() && self.below.is_none() && self.steeper.is_none() && self.flatter.is_none()
    }

    pub fn rgb(&self) -> [u8; 3] {
        crate::grid::hex(&self.color).unwrap_or([128, 128, 128])
    }

    pub fn depth(&self) -> i32 {
        self.depth.unwrap_or(DEPTH)
    }
}

/// A band of rock (`[[terrain.layer]]`, from the bottom up): how thick,
/// how hard (a hard one stands as a wall with a ledge on top, a soft one
/// weathers back into a slope of scree), its color in the walls.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub thick: i32,
    pub hard: f64,
    pub color: String,
}

impl Layer {
    pub fn rgb(&self) -> [u8; 3] {
        crate::grid::hex(&self.color).unwrap_or([128, 128, 128])
    }
}

/// How a scatter rule turns what it sets down: any way (a game that turns
/// things freely), in quarter turns, not at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotate {
    Any,
    Quarters,
    None,
}

impl Rotate {
    pub const ALL: [Rotate; 3] = [Rotate::Any, Rotate::Quarters, Rotate::None];

    pub fn word(self) -> &'static str {
        match self {
            Rotate::Any => "any",
            Rotate::Quarters => "quarters",
            Rotate::None => "none",
        }
    }
}

/// What a scatter rule makes: pieces (set into the level by the editor's
/// SCATTER, the owner's from then on) or voxels (baked into the land as
/// the level is built).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterAs {
    Pieces,
    Voxels,
}

impl ScatterAs {
    pub fn word(self) -> &'static str {
        match self {
            ScatterAs::Pieces => "pieces",
            ScatterAs::Voxels => "voxels",
        }
    }
}

/// A rule that scatters models over the land (`[[terrain.scatter]]`):
/// which (one, or several taken in turn by chance), how many on 100 by
/// 100 voxels of ground that fits its conditions (the materials' words),
/// how far apart at least, turned how, sunk how deep, as pieces or voxels.
#[derive(Clone, Debug, PartialEq)]
pub struct Scatter {
    pub models: Vec<String>,
    pub density: f64,
    pub above: Option<i32>,
    pub below: Option<i32>,
    pub steeper: Option<f64>,
    pub flatter: Option<f64>,
    pub blend: Option<f64>,
    pub apart: Option<i32>,
    pub rotate: Option<Rotate>,
    pub sink: Option<i32>,
    pub as_: Option<ScatterAs>,
}

impl Scatter {
    pub fn new(model: &str) -> Self {
        Self { models: vec![model.to_string()], density: 2.0, above: None, below: None, steeper: None, flatter: None, blend: None, apart: None, rotate: None, sink: None, as_: None }
    }

    pub fn rotate(&self) -> Rotate {
        self.rotate.unwrap_or(Rotate::Quarters)
    }

    pub fn makes(&self) -> ScatterAs {
        self.as_.unwrap_or(ScatterAs::Pieces)
    }
}

/// Where a scatter rule sets one down: which of its models, the column
/// (from the terrain's corner), the land's height there, turned how many
/// degrees clockwise seen from above (quarters: 0, 90, 180, 270).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub model: usize,
    pub column: IVec2,
    pub ground: i32,
    pub rotation: f64,
}

/// How deep a material goes when it does not say.
pub const DEPTH: i32 = 3;
/// A material's patches when it does not say, voxels across.
pub const PATCH: i32 = 6;
/// The unevenness at `rough = 1`, voxels up or down: large swells (eight
/// points across) low, most of it in bumps (a point across) and small
/// unevenness (five voxels across), so a little `rough` breaks a meadow's
/// contour rings into irregular ground without making it lumpy from afar.
const SWELLS: f64 = 4.0;
const BUMPS: f64 = 4.0;
const SMALL: f64 = 3.0;
/// The slope a material's `steeper` and `flatter` see: the height across
/// this many voxels each way, so a single contour step is no slope.
const SLOPE_REACH: i32 = 3;
/// A wall the layers' bands show on: at least this many voxels high.
const WALL: i32 = 3;
/// How far the bands of rock wander up and down at `rough = 1`, voxels.
const BANDS: f64 = 2.0;
/// How the bands lie when the file does not say: tilted 3 degrees,
/// folded 0.4 (a level cake: `dip = 0`, `fold = 0`), no fault.
pub const DIP: f64 = 3.0;
pub const FOLD: f64 = 0.4;
/// The most materials a terrain has.
pub const MATERIALS: usize = 32;
/// The highest point, voxels.
pub const HIGHEST: i32 = 4096;
/// A terrace's height and how much its lines wander, when the file does
/// not say.
pub const TERRACE: i32 = 8;
pub const ROUGH: f64 = 0.0;
/// The share of a terrace's rise that is its riser (the wall and its foot).
const RISER: f64 = 0.25;
/// How much one step of grain lightens or darkens at `grain = 1`.
const GRAIN: f64 = 0.12;

/// A level's terrain as its file says it.
#[derive(Clone, Debug)]
pub struct Terrain {
    /// Voxels between points, x and y alike.
    pub spacing: i32,
    /// The first square's low corner, in the level's voxels; its z the
    /// base the heights count from.
    pub at: [i32; 3],
    /// Points across (x) and deep (y).
    pub cols: i32,
    pub rows: i32,
    /// Each point's height, x + y cols, row 0 the nearest (y 0).
    pub heights: Vec<i32>,
    /// Each point's shape, as `heights`.
    pub shapes: Vec<Shape>,
    /// STEP's terraces: their height in voxels, how much their lines
    /// wander (0 to 1).
    pub terrace: Option<i32>,
    /// How uneven the land is (0 to 1), and each point's share of it
    /// (`roughness`: 0 to 9, all 9 when the file has none).
    pub rough: Option<f64>,
    pub roughness: Option<Vec<u8>>,
    /// The bands of rock STEP's walls are cut in, from the bottom up,
    /// again and again upward; none: terraces `terrace` high.
    pub layers: Vec<Layer>,
    /// How the bands lie (with layers): tilted `dip` degrees toward
    /// `dip_to` (degrees clockwise from north, else from the seed), folded
    /// (`fold`, 0 to 1: broad folds, the layers swelling, thinning and a
    /// thin one pinching out), broken along one line (`fault`, 0 to 1).
    pub dip: Option<f64>,
    pub dip_to: Option<f64>,
    pub fold: Option<f64>,
    pub fault: Option<f64>,
    /// Rules that scatter models over it (`[[terrain.scatter]]`).
    pub scatter: Vec<Scatter>,
    pub seed: Option<u64>,
    /// What lies under the materials (else the last material's color).
    pub under: Option<String>,
    pub materials: Vec<Material>,
    /// Where the file says it (0: from no file).
    pub line: usize,
}

/// (Where the file says it is no part of it.)
impl PartialEq for Terrain {
    fn eq(&self, o: &Self) -> bool {
        (self.spacing, self.at, self.cols, self.rows, &self.heights, &self.shapes) == (o.spacing, o.at, o.cols, o.rows, &o.heights, &o.shapes)
            && (self.terrace, self.rough, &self.roughness, &self.layers, self.seed, &self.under, &self.materials) == (o.terrace, o.rough, &o.roughness, &o.layers, o.seed, &o.under, &o.materials)
            && self.scatter == o.scatter
            && (self.dip, self.dip_to, self.fold, self.fault) == (o.dip, o.dip_to, o.fold, o.fault)
    }
}

impl Terrain {
    /// A flat land `cols` by `rows` points, `height` high, grass.
    pub fn flat(spacing: i32, cols: i32, rows: i32, height: i32) -> Self {
        let n = (cols * rows) as usize;
        Self { spacing, at: [0; 3], cols, rows, heights: vec![height; n], shapes: vec![Shape::Smooth; n], terrace: None, rough: None, roughness: None, layers: Vec::new(), scatter: Vec::new(), dip: None, dip_to: None, fold: None, fault: None, seed: None, under: None, materials: vec![Material::new("#5c8a3a")], line: 0 }
    }

    /// Its size in voxels across and deep.
    pub fn size(&self) -> IVec2 {
        IVec2::new(self.cols, self.rows) * self.spacing
    }

    /// Point `p`'s place in `heights` (None outside).
    pub fn index(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.cols && p.y < self.rows).then(|| (p.x + p.y * self.cols) as usize)
    }

    /// The column a point stands on, from the terrain's corner.
    pub fn column_of(&self, p: IVec2) -> IVec2 {
        p * self.spacing + IVec2::splat(self.spacing / 2)
    }

    /// The point whose square holds column `c` (from the corner), clamped.
    pub fn point_of(&self, c: IVec2) -> IVec2 {
        (c.div_euclid(IVec2::splat(self.spacing))).clamp(IVec2::ZERO, IVec2::new(self.cols - 1, self.rows - 1))
    }

    fn h(&self, x: i32, y: i32) -> f64 {
        let x = x.clamp(0, self.cols - 1);
        let y = y.clamp(0, self.rows - 1);
        self.heights[(x + y * self.cols) as usize] as f64
    }

    /// The height of column `c` (from the terrain's corner): whole voxels,
    /// the column filled up to it.
    pub fn height_at(&self, c: IVec2) -> i32 {
        let (i, j, fx, fy) = self.between(c);
        let (a, b, cc, d) = (self.h(i, j), self.h(i + 1, j), self.h(i, j + 1), self.h(i + 1, j + 1));
        // How much of a shape the four points round it have.
        let weight = |shape: Shape| self.nearness((i, j, fx, fy), |k| (self.shapes[k] == shape) as i32 as f64);
        let (linear, step) = (weight(Shape::Linear), weight(Shape::Step));
        let straight = {
            let low = a + (b - a) * fx;
            let high = cc + (d - cc) * fx;
            low + (high - low) * fy
        };
        let h = if linear >= 1.0 {
            straight
        } else {
            let row = |y: i32| catmull_rom(self.h(i - 1, y), self.h(i, y), self.h(i + 1, y), self.h(i + 2, y), fx);
            let v = catmull_rom(row(j - 1), row(j), row(j + 1), row(j + 2), fy);
            // Never past the four points round it.
            let smooth = v.clamp(a.min(b).min(cc).min(d), a.max(b).max(cc).max(d));
            smooth + (straight - smooth) * linear
        };
        // Uneven where it is rough; STEP: then cut into terraces (the
        // bands shifted up and down a little), as much as it weighs in.
        let amount = self.rough_at((i, j, fx, fy));
        let h = if amount > 0.0 { h + amount * self.relief(c) } else { h };
        let h = if step > 0.0 {
            let shift = amount * self.band_shift(c) + self.strata_shift(c);
            h + (self.terraced_at(h + shift, c) - shift - h) * step
        } else {
            h
        };
        ((h + 0.5).floor() as i32).max(0)
    }

    /// Where column `c` lies between the points: the cell of points (i, j)
    /// to (i + 1, j + 1), and how far across it (exact fractions of whole
    /// numbers).
    fn between(&self, c: IVec2) -> (i32, i32, f64, f64) {
        let s = self.spacing;
        let u = c - IVec2::splat(s / 2);
        (u.x.div_euclid(s), u.y.div_euclid(s), u.x.rem_euclid(s) as f64 / s as f64, u.y.rem_euclid(s) as f64 / s as f64)
    }

    /// The four points round a place between them (`between`), each one's
    /// `of` (by its index in the points) weighed by its nearness.
    fn nearness(&self, (i, j, fx, fy): (i32, i32, f64, f64), of: impl Fn(usize) -> f64) -> f64 {
        let at = |x: i32, y: i32| of((x.clamp(0, self.cols - 1) + y.clamp(0, self.rows - 1) * self.cols) as usize);
        (1.0 - fx) * (1.0 - fy) * at(i, j) + fx * (1.0 - fy) * at(i + 1, j) + (1.0 - fx) * fy * at(i, j + 1) + fx * fy * at(i + 1, j + 1)
    }

    /// How much STEP weighs in at column `c` (0 to 1: the four points
    /// round it, each by its nearness).
    pub fn step_at(&self, c: IVec2) -> f64 {
        self.nearness(self.between(c), |k| (self.shapes[k] == Shape::Step) as i32 as f64)
    }

    /// How uneven the land is at a place between the points: `rough`
    /// times the four points' roughness round it (each of 9), by their
    /// nearness.
    fn rough_at(&self, place: (i32, i32, f64, f64)) -> f64 {
        let rough = self.rough.unwrap_or(ROUGH);
        if rough <= 0.0 {
            return 0.0;
        }
        let Some(r) = &self.roughness else { return rough };
        rough * self.nearness(place, |k| r[k] as f64 / 9.0)
    }

    /// How uneven the land is at column `c`, from 0 to 1 (`rough_at`).
    pub fn roughness_at(&self, c: IVec2) -> f64 {
        self.rough_at(self.between(c))
    }

    /// The unevenness at `rough = 1`, voxels up or down: three octaves of
    /// value noise from `seed` (swells, bumps, small unevenness).
    fn relief(&self, c: IVec2) -> f64 {
        let seed = self.seed.unwrap_or(0);
        SWELLS * value_noise(seed ^ 0x5EE1, c, (8 * self.spacing).max(4)) + BUMPS * value_noise(seed ^ 0xB0B5, c, self.spacing.max(4)) + SMALL * value_noise(seed ^ 0x5A11, c, 5)
    }

    /// How far the bands of rock and the terraces' heights wander at
    /// column `c` at `rough = 1`, voxels.
    fn band_shift(&self, c: IVec2) -> f64 {
        BANDS * value_noise(self.seed.unwrap_or(0) ^ 0xBA4D, c, (2 * self.spacing).max(4))
    }

    /// How far the layers' stack lies from level at column `c` (with
    /// layers, else 0), voxels: the dip (a tilt toward `dip_to`, about the
    /// land's middle), broad folds (`fold` times the thickest layer, many
    /// points across), a fine waver (so no band's edge is ruler straight)
    /// and the fault's step on one side of its line.
    pub fn strata_shift(&self, c: IVec2) -> f64 {
        if self.layers.is_empty() {
            return 0.0;
        }
        let seed = self.seed.unwrap_or(0);
        let mid = self.size().as_dvec2() / 2.0;
        let at = glam::DVec2::new(c.x as f64 + 0.5, c.y as f64 + 0.5) - mid;
        let way = |deg: f64| {
            let r = deg * (std::f64::consts::PI / 180.0);
            glam::DVec2::new(libm::sin(r), libm::cos(r))
        };
        let dip = self.dip.unwrap_or(DIP).clamp(0.0, 45.0);
        let to = self.dip_to.unwrap_or((grain_hash(seed ^ 0xD1B, IVec3::ZERO) % 360) as f64);
        let mut shift = libm::tan(dip * (std::f64::consts::PI / 180.0)) * at.dot(way(to));
        let fold = self.fold.unwrap_or(FOLD).clamp(0.0, 1.0);
        let thickest = self.layers.iter().map(|l| l.thick).max().unwrap_or(1).max(1) as f64;
        if fold > 0.0 {
            shift += fold * thickest * value_noise(seed ^ 0xF01D, c, (10 * self.spacing).max(8));
            shift += fold * 2.0 * value_noise(seed ^ 0xFA1E, c, 6);
        }
        let fault = self.fault.unwrap_or(0.0).clamp(0.0, 1.0);
        if fault > 0.0 {
            let across = way((grain_hash(seed ^ 0xFA17, IVec3::ZERO) % 180) as f64 + 90.0);
            if at.dot(across) > 0.0 {
                shift += fault * thickest;
            }
        }
        shift
    }

    /// Each layer's thickness at column `c`: as the file says, swelling and
    /// thinning along the land with `fold` (up to a quarter either way at
    /// 0.4), the thinner layers pinching out here and there.
    pub fn thicknesses(&self, c: IVec2) -> Vec<f64> {
        let fold = self.fold.unwrap_or(FOLD).clamp(0.0, 1.0);
        let (thin, thick) = self.layers.iter().fold((i32::MAX, 0), |(lo, hi), l| (lo.min(l.thick), hi.max(l.thick)));
        let seed = self.seed.unwrap_or(0);
        self.layers
            .iter()
            .enumerate()
            .map(|(k, l)| {
                let t = l.thick.max(1) as f64;
                if fold <= 0.0 {
                    return t;
                }
                let swell = 1.0 + 0.6 * fold * value_noise(seed ^ (0x7C1C_0000 + k as u64), c, (6 * self.spacing).max(8));
                let thinness = if thick > thin { (thick - l.thick) as f64 / (thick - thin) as f64 } else { 0.0 };
                let pinch = (1.0 + fold * thinness * 4.0 * value_noise(seed ^ (0x9F1C_0000 + k as u64), c, (4 * self.spacing).max(8))).clamp(0.0, 1.0);
                t * swell * pinch
            })
            .collect()
    }

    /// The band of rock a height of the layers' stack is in at column `c`
    /// (`thicknesses`): its place among the layers, its bottom and its
    /// thickness, the stack again and again upward. None without layers.
    pub fn layer_at(&self, h: f64, c: IVec2) -> Option<(usize, f64, f64)> {
        band(&self.thicknesses(c), h)
    }

    /// A height cut into terraces: of `terrace` (flat up to the last
    /// quarter of each rise, then a riser to the next, gentle at its foot
    /// and steep at its top), or of the layers at column `c` (each band its
    /// own riser: a hard one a short wall over a little foot, a soft one a
    /// long slope).
    pub fn terraced_at(&self, h: f64, c: IVec2) -> f64 {
        if let Some((k, bottom, thick)) = self.layer_at(h, c) {
            let hard = self.layers[k].hard.clamp(0.0, 1.0);
            let riser = 0.1 + 0.8 * (1.0 - hard);
            let f = ((h - bottom) / thick - (1.0 - riser)) / riser;
            let f = f.clamp(0.0, 1.0);
            // (A hard band's riser bends up at its foot, a soft one's is
            // straight: f to the power 1 + hard, as a sum of the two.)
            return bottom + (f * (1.0 - hard) + f * f * hard) * thick;
        }
        let t = self.terrace.unwrap_or(TERRACE).max(1) as f64;
        let level = (h / t).floor();
        let k = ((h / t - level - (1.0 - RISER)) / RISER).max(0.0);
        (level + k * k) * t
    }

    /// Every column's height, x + y width, row by row (several cores, in
    /// order).
    pub fn surface(&self) -> Surface {
        let size = self.size();
        let rows: Vec<i32> = (0..size.y).collect();
        let top = crate::par::in_order(&rows, 16, |part| part.iter().flat_map(|&y| (0..size.x).map(move |x| self.height_at(IVec2::new(x, y)))).collect());
        Surface { size, top }
    }

    /// The columns of a rectangle (from the corner, `lo` to `hi`
    /// excluded) anew in `surface`: what a change of the points `from` to
    /// `to` (included) can touch is their squares and two more round them.
    pub fn resurface(&self, surface: &mut Surface, from: IVec2, to: IVec2) -> (IVec2, IVec2) {
        let s = self.spacing;
        let lo = ((from - IVec2::splat(2)) * s).max(IVec2::ZERO);
        let hi = ((to + IVec2::splat(3)) * s).min(self.size());
        for y in lo.y..hi.y {
            for x in lo.x..hi.x {
                surface.top[(x + y * surface.size.x) as usize] = self.height_at(IVec2::new(x, y));
            }
        }
        (lo, hi)
    }

    /// Where scatter rule `k` sets its models down on the land `surface`:
    /// one candidate in each square of a grid as fine as its density asks
    /// (100 / sqrt(density) voxels), somewhere in it by chance from `seed`
    /// and the rule's place (so no rows, no clumps), kept where the ground
    /// fits its conditions (the materials' words, `blend` letting the
    /// borders wander) and none of the rule's already kept lies nearer
    /// than `apart`; in rows from the near left. The same on every system.
    pub fn scatter(&self, k: usize, surface: &Surface) -> Vec<Placed> {
        let Some(rule) = self.scatter.get(k) else { return Vec::new() };
        if rule.models.is_empty() || rule.density <= 0.0 {
            return Vec::new();
        }
        let seed = self.seed.unwrap_or(0) ^ (0x5CA7_7E40_0000 + k as u64);
        let cell = 100.0 / rule.density.sqrt();
        let size = surface.size;
        let (nx, ny) = ((size.x as f64 / cell).ceil() as i32, (size.y as f64 / cell).ceil() as i32);
        let blend = rule.blend.unwrap_or(0.0);
        let apart = rule.apart.unwrap_or(0).max(0);
        let mut kept: std::collections::BTreeMap<(i32, i32), Vec<IVec2>> = std::collections::BTreeMap::new();
        let mut out = Vec::new();
        for gy in 0..ny {
            for gx in 0..nx {
                let h = grain_hash(seed, IVec3::new(gx, gy, 0));
                let fx = (h & 0xFFFF) as f64 / 65536.0;
                let fy = ((h >> 16) & 0xFFFF) as f64 / 65536.0;
                let c = IVec2::new(((gx as f64 + fx) * cell).floor() as i32, ((gy as f64 + fy) * cell).floor() as i32);
                let Some(top) = surface.at(c).filter(|&t| t > 0) else { continue };
                let j = if blend > 0.0 { blend / 2.0 * value_noise(seed ^ 0xB1E5, c, ((blend * 2.0).round() as i32).max(4)) } else { 0.0 };
                let g = surface.slope(c);
                let fits = rule.above.is_none_or(|a| top as f64 > a as f64 + j)
                    && rule.below.is_none_or(|b| (top as f64) < b as f64 + j)
                    && rule.steeper.is_none_or(|d| g > tan_degrees(d + j))
                    && rule.flatter.is_none_or(|d| g < tan_degrees(d + j));
                if !fits {
                    continue;
                }
                if apart > 0 {
                    let b = (c.x.div_euclid(apart), c.y.div_euclid(apart));
                    let near = (-1..=1).any(|dy| (-1..=1).any(|dx| kept.get(&(b.0 + dx, b.1 + dy)).is_some_and(|v| v.iter().any(|o| (*o - c).length_squared() < apart * apart))));
                    if near {
                        continue;
                    }
                    kept.entry(b).or_default().push(c);
                }
                let model = ((h >> 32) % rule.models.len() as u64) as usize;
                let rotation = match rule.rotate() {
                    Rotate::Any => ((h >> 40) % 360) as f64,
                    Rotate::Quarters => (((h >> 40) & 3) * 90) as f64,
                    Rotate::None => 0.0,
                };
                out.push(Placed { model, column: c, ground: top, rotation });
            }
        }
        out
    }

    /// Its colours in `world`'s palette, in the order of the materials
    /// (each tone, then its two shades of grain), then `under`, then the
    /// layers; and what paints it there, its corner at `origin` (the
    /// level's) plus `at`. None: the palette is full.
    pub fn painter(&self, world: &mut World, origin: IVec3, surface: Surface) -> Result<Painter> {
        let full = |m: usize| anyhow!("the level has more than 255 colors with the terrain's material {} ({})", m + 1, self.materials[m].color);
        let seed = self.seed.unwrap_or(0);
        let size = surface.size;
        let columns = (size.x * size.y) as usize;
        let column = |k: usize| IVec2::new(k as i32 % size.x, k as i32 / size.x);
        let top = |x: i32, y: i32| if x < 0 || y < 0 || x >= size.x || y >= size.y { i32::MAX } else { surface.top[(x + y * size.x) as usize] };
        let near = |x: i32, y: i32| surface.top[(x.clamp(0, size.x - 1) + y.clamp(0, size.y - 1) * size.x) as usize] as f64;
        let mut mats = Vec::new();
        let mut slope_jitter: Vec<Option<Vec<f64>>> = Vec::new();
        for (m, mat) in self.materials.iter().enumerate() {
            let grain = mat.grain.unwrap_or(0.0);
            let mut entries = Vec::new();
            for rgb in mat.all_tones() {
                let base = world.colour_as(rgb, false, false).ok_or_else(|| full(m))?;
                let mut e = [base; 3];
                if grain > 0.0 {
                    for (k, f) in [(0, 1.0 - GRAIN * grain), (2, 1.0 + GRAIN * grain)] {
                        let shade = rgb.map(|c| (c as f64 * f).round().clamp(0.0, 255.0) as u8);
                        e[k] = world.colour_as(shade, false, false).ok_or_else(|| full(m))?;
                    }
                }
                entries.push(e);
            }
            // Patches: each column's tone from noise `patch` voxels across.
            let tones = (entries.len() > 1).then(|| {
                let cell = mat.patch.unwrap_or(PATCH).max(1);
                let n = entries.len();
                (0..columns)
                    .map(|k| {
                        let v = patch_noise(seed ^ (0x7A7C_0000 + m as u64), column(k), cell as f64);
                        match n {
                            2 => (v >= 0.0) as u8,
                            _ => if v < -0.2 { 0 } else if v > 0.2 { 2 } else { 1 },
                        }
                    })
                    .collect::<Vec<u8>>()
            });
            // Soft borders: each column's conditions moved by up to half
            // the blend either way (noise), the height's lower in a hollow
            // (snow reaches down a gully).
            let blend = mat.blend.unwrap_or(0.0);
            let jitter = (blend > 0.0).then(|| {
                let cell = ((blend * 2.0).round() as i32).max(4);
                (0..columns)
                    .map(|k| {
                        let c = column(k);
                        let n = value_noise(seed ^ (0xB1E4_0000 + m as u64), c, cell);
                        let mean = (near(c.x - 1, c.y) + near(c.x + 1, c.y) + near(c.x, c.y - 1) + near(c.x, c.y + 1)) / 4.0;
                        let hollow = ((mean - near(c.x, c.y)) / 4.0).clamp(0.0, 1.0);
                        (blend / 2.0 * n, blend / 2.0 * (n - hollow))
                    })
                    .collect::<Vec<(f64, f64)>>()
            });
            mats.push(Paint { above: mat.above, below: mat.below, depth: mat.depth(), entries, grain: grain > 0.0, tones, jitter: jitter.as_ref().map(|j| j.iter().map(|v| v.1).collect()) });
            slope_jitter.push(jitter.map(|j| j.into_iter().map(|v| v.0).collect()));
        }
        let under_rgb = self.under.as_deref().and_then(crate::grid::hex).unwrap_or_else(|| self.materials.last().map_or([128; 3], Material::rgb));
        let under = world.colour_as(under_rgb, false, false).ok_or_else(|| anyhow!("the level has more than 255 colors with the terrain's under ({})", self.under.as_deref().unwrap_or("")))?;
        // The layers' colors, and where the land is terraced how far each
        // column's bands are shifted.
        let strata = if self.layers.is_empty() {
            None
        } else {
            let mut entries = Vec::new();
            for l in &self.layers {
                entries.push(world.colour_as(l.rgb(), false, false).ok_or_else(|| anyhow!("the level has more than 255 colors with the terrain's layer {}", l.color))?);
            }
            let mut shift = Vec::with_capacity(columns);
            let mut thick = Vec::with_capacity(columns * self.layers.len());
            for k in 0..columns {
                let c = column(k);
                if self.step_at(c) >= 0.5 {
                    shift.push(self.roughness_at(c) * self.band_shift(c) + self.strata_shift(c));
                    thick.extend(self.thicknesses(c));
                } else {
                    shift.push(f64::NAN);
                    thick.extend(std::iter::repeat_n(0.0, self.layers.len()));
                }
            }
            Some(Strata { entries, shift, thick })
        };
        // Each column: what can be seen of it from the side (the lowest of
        // it and its four neighbours; the land's outer edge, cut, shows
        // what lies under), and which materials its slope allows.
        let tans: Vec<(Option<f64>, Option<f64>)> = self.materials.iter().map(|m| (m.steeper.map(tan_degrees), m.flatter.map(tan_degrees))).collect();
        // Its slope: along each axis the height `SLOPE_REACH` voxels
        // either way; where the land goes on the same way, the gentler
        // side (a tread by a riser is flat, a single contour step no
        // slope), over a peak or a hollow the mean. A wall's face (a voxel
        // under its column's top, open to the side over a drop of `WALL`
        // or more) is as steep as its drop.
        let mut seen = Vec::with_capacity(columns);
        let mut allows = Vec::with_capacity(columns);
        let mut faces = Vec::with_capacity(columns);
        for y in 0..size.y {
            for x in 0..size.x {
                let k = (x + y * size.x) as usize;
                let low = top(x, y).min(top(x - 1, y)).min(top(x + 1, y)).min(top(x, y - 1)).min(top(x, y + 1));
                seen.push(low);
                let g = surface.slope(IVec2::new(x, y));
                let bits_at = |g: f64| {
                    let mut bits = 0u32;
                    for (m, mat) in self.materials.iter().enumerate() {
                        let (steeper, flatter) = match &slope_jitter[m] {
                            None => tans[m],
                            Some(j) => (mat.steeper.map(|d| tan_degrees(d + j[k])), mat.flatter.map(|d| tan_degrees(d + j[k]))),
                        };
                        if steeper.is_none_or(|t| g > t) && flatter.is_none_or(|t| g < t) {
                            bits |= 1 << m;
                        }
                    }
                    bits
                };
                let bits = bits_at(g);
                allows.push(bits);
                let drop = surface.top[k] - low;
                faces.push(if drop >= WALL { bits_at(g.max(drop as f64)) } else { bits });
            }
        }
        let corner = origin + IVec3::from(self.at);
        Ok(Painter { corner, surface, seen, allows, faces, mats, under, seed, strata })
    }
}

/// The band a height is in, the layers `thick` (from the bottom up) again
/// and again upward: its place, its bottom, its thickness (a layer pinched
/// out to nothing is no band). None without a band of any thickness.
fn band(thick: &[f64], h: f64) -> Option<(usize, f64, f64)> {
    let period: f64 = thick.iter().sum();
    if thick.is_empty() || period < 0.5 {
        return None;
    }
    let mut bottom = (h / period).floor() * period;
    let last = thick.iter().rposition(|&t| t > 0.0)?;
    for (k, &t) in thick.iter().enumerate() {
        if t > 0.0 && (h < bottom + t || k == last) {
            return Some((k, bottom, t));
        }
        bottom += t;
    }
    None
}

/// Catmull-Rom through `p1` (t 0) and `p2` (t 1), `p0` and `p3` beyond.
fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
}

/// Smooth noise from -1 to 1 over a lattice `cell` columns wide: a number
/// at each node from `seed`, between them eased (smoothstep).
fn value_noise(seed: u64, c: IVec2, cell: i32) -> f64 {
    let (fx, fy) = (c.x.rem_euclid(cell) as f64 / cell as f64, c.y.rem_euclid(cell) as f64 / cell as f64);
    eased_nodes(seed, c.x.div_euclid(cell), c.y.div_euclid(cell), fx, fy)
}

/// Smooth noise in -1 to 1 over a lattice `cell` wide, at a point of
/// the plane given in fractions (where a warp moved it).
fn value_noise_at(seed: u64, x: f64, y: f64, cell: f64) -> f64 {
    let (u, v) = (x / cell, y / cell);
    let (gx, gy) = (u.floor(), v.floor());
    eased_nodes(seed, gx as i32, gy as i32, u - gx, v - gy)
}

/// Value noise between the lattice's nodes (gx, gy) to (gx + 1, gy + 1),
/// (fx, fy) across, eased: each node a number from `seed`.
fn eased_nodes(seed: u64, gx: i32, gy: i32, fx: f64, fy: f64) -> f64 {
    let ease = |f: f64| f * f * (3.0 - 2.0 * f);
    let (fx, fy) = (ease(fx), ease(fy));
    let node = |x: i32, y: i32| (grain_hash(seed, IVec3::new(x, y, 0)) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
    let low = node(gx, gy) + (node(gx + 1, gy) - node(gx, gy)) * fx;
    let high = node(gx, gy + 1) + (node(gx + 1, gy + 1) - node(gx, gy + 1)) * fx;
    low + (high - low) * fy
}

/// A material's patches: two octaves of value noise, the plane warped by
/// two more (so no border runs along the grid), about `cell` voxels across.
fn patch_noise(seed: u64, c: IVec2, cell: f64) -> f64 {
    let (x, y) = (c.x as f64 + 0.5, c.y as f64 + 0.5);
    let warp = cell * 0.7;
    let wx = x + warp * value_noise_at(seed ^ 0x3A1, x, y, cell * 1.7);
    let wy = y + warp * value_noise_at(seed ^ 0x3A2, x, y, cell * 1.7);
    0.7 * value_noise_at(seed, wx, wy, cell) + 0.3 * value_noise_at(seed ^ 0x3A3, wx, wy, cell * 0.45)
}

/// The tangent of an angle in degrees (`libm`: the same bits everywhere).
fn tan_degrees(d: f64) -> f64 {
    libm::tan(d.clamp(0.0, 89.999) * (std::f64::consts::PI / 180.0))
}

/// Every column's height (from the terrain's base), x + y size.x.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub size: IVec2,
    pub top: Vec<i32>,
}

impl Surface {
    pub fn at(&self, c: IVec2) -> Option<i32> {
        (c.x >= 0 && c.y >= 0 && c.x < self.size.x && c.y < self.size.y).then(|| self.top[(c.x + c.y * self.size.x) as usize])
    }

    /// The slope at column `c` (rise over run, a tangent) as materials and
    /// scatter rules see it: along each axis the height `SLOPE_REACH`
    /// voxels either way; where the land goes on the same way, the
    /// gentler side (a tread by a riser is flat, a single contour step no
    /// slope), over a peak or a hollow the mean.
    pub fn slope(&self, c: IVec2) -> f64 {
        let near = |x: i32, y: i32| self.top[(x.clamp(0, self.size.x - 1) + y.clamp(0, self.size.y - 1) * self.size.x) as usize] as f64;
        let side = |a: f64, b: f64| if a * b > 0.0 { a.abs().min(b.abs()) } else if a == 0.0 || b == 0.0 { 0.0 } else { ((a + b) / 2.0).abs() };
        let r = SLOPE_REACH;
        let h = near(c.x, c.y);
        let gx = side(near(c.x + r, c.y) - h, h - near(c.x - r, c.y)) / r as f64;
        let gy = side(near(c.x, c.y + r) - h, h - near(c.x, c.y - r)) / r as f64;
        (gx * gx + gy * gy).sqrt()
    }
}

/// A material as its palette entries.
struct Paint {
    above: Option<i32>,
    below: Option<i32>,
    depth: i32,
    /// Each tone darker, itself, lighter (all itself without grain).
    entries: Vec<[u8; 3]>,
    grain: bool,
    /// Each column's tone (several tones), and how far its height
    /// conditions are moved (a blend).
    tones: Option<Vec<u8>>,
    jitter: Option<Vec<f64>>,
}

/// The layers' palette entries and, where the land is terraced, how far
/// each column's bands are shifted (NaN elsewhere).
struct Strata {
    entries: Vec<u8>,
    shift: Vec<f64>,
    /// Each column's layers' thicknesses (`Terrain::thicknesses`), the
    /// layers' count a column.
    thick: Vec<f64>,
}

/// What paints a terrain into a world's voxels (`Terrain::painter`).
pub struct Painter {
    /// The terrain's low corner in the world, its z the base.
    pub corner: IVec3,
    pub surface: Surface,
    seen: Vec<i32>,
    /// Which materials each column's slope allows: on its top and the
    /// voxels of a gentle side, and on a wall's face.
    allows: Vec<u32>,
    faces: Vec<u32>,
    mats: Vec<Paint>,
    under: u8,
    seed: u64,
    strata: Option<Strata>,
}

impl Painter {
    /// The voxel at `p` in the world (0 where the land is not).
    pub fn voxel(&self, p: IVec3) -> u8 {
        let q = p - self.corner;
        let size = self.surface.size;
        if q.x < 0 || q.y < 0 || q.x >= size.x || q.y >= size.y {
            return 0;
        }
        let c = (q.x + q.y * size.x) as usize;
        // The voxel's height: the top voxel of a column h high is h.
        let hz = q.z + 1;
        if hz > self.surface.top[c] || p.z < 0 {
            return 0;
        }
        let face = hz < self.surface.top[c] && hz > self.seen[c];
        let bits = if face { self.faces[c] } else { self.allows[c] };
        let last = self.mats.len() - 1;
        for (m, paint) in self.mats.iter().enumerate() {
            if bits & (1 << m) == 0 {
                continue;
            }
            let (above, below) = match &paint.jitter {
                None => (paint.above.is_some_and(|a| hz <= a), paint.below.is_some_and(|b| hz >= b)),
                Some(j) => (paint.above.is_some_and(|a| (hz as f64) <= a as f64 + j[c]), paint.below.is_some_and(|b| (hz as f64) >= b as f64 + j[c])),
            };
            if above || below {
                continue;
            }
            if hz <= self.seen[c] - paint.depth {
                return self.under;
            }
            // Where the land is terraced and no condition picked a
            // material, the bands of rock show on its walls: a voxel open
            // to the side over a drop of a few voxels (a ledge, a tread
            // and a single contour step show the material).
            if m == last
                && let Some(st) = &self.strata
                && !st.shift[c].is_nan()
                && self.surface.top[c] - self.seen[c] >= WALL
                && hz > self.seen[c]
            {
                let n = st.entries.len();
                return st.entries[band(&st.thick[c * n..c * n + n], hz as f64 - 0.5 + st.shift[c]).map_or(0, |b| b.0)];
            }
            let e = &paint.entries[paint.tones.as_ref().map_or(0, |t| t[c] as usize)];
            if !paint.grain {
                return e[1];
            }
            return e[(grain_hash(self.seed, q) % 3) as usize];
        }
        self.under
    }

    /// The chunks the land lies in: its columns from the world's floor to
    /// its highest voxel.
    pub fn chunks(&self) -> Vec<IVec3> {
        let high = self.surface.top.iter().copied().max().unwrap_or(0);
        let lo = IVec3::new(self.corner.x, self.corner.y, 0).div_euclid(IVec3::splat(CHUNK));
        let hi = (IVec3::new(self.corner.x + self.surface.size.x, self.corner.y + self.surface.size.y, self.corner.z + high) - 1).max(IVec3::ZERO).div_euclid(IVec3::splat(CHUNK));
        let mut out = Vec::new();
        for z in lo.z..=hi.z {
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    out.push(IVec3::new(x, y, z));
                }
            }
        }
        out
    }
}

/// A number from a voxel's place and the seed (SplitMix64's mixing).
fn grain_hash(seed: u64, q: IVec3) -> u64 {
    let mut z = seed ^ (q.x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (q.y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (q.z as u32 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

// ---------------------------------------------------------------- the file

#[cfg(test)]
mod tests {
    use super::*;

    /// What `a_terrain_without_the_later_words_builds_as_before` built
    /// before roughness, layers, patches and blend came (pinned again
    /// when the slope came to be measured three voxels each way, the
    /// gentler side: a single contour step no longer turns to rock).
    const BEFORE: u64 = 0xe455_0f3f_fa75_4bac;

    fn read_text(src: &str) -> Result<Terrain> {
        let doc: toml_edit::Document<&str> = toml_edit::Document::parse(src).unwrap();
        read(&Reader { src }, doc.as_table().get("terrain").unwrap())
    }

    fn terrain(heights: &str, shapes: &str, spacing: i32) -> Terrain {
        let shapes = if shapes.is_empty() { String::new() } else { format!("shapes = \"\"\"\n{shapes}\n\"\"\"\n") };
        read_text(&format!("[terrain]\nspacing = {spacing}\nheights = \"\"\"\n{heights}\n\"\"\"\n{shapes}\n[[terrain.material]]\ncolor = \"#5c8a3a\"\n")).unwrap()
    }

    /// At each point's own column its height exactly, whatever its shape;
    /// a STEP point holds its height over its whole square.
    #[test]
    fn each_shape_meets_its_points_exactly() {
        for sign in ['.', '^', '#'] {
            let shapes = format!("{sign} {sign} {sign} {sign}\n{sign} {sign} {sign} {sign}\n{sign} {sign} {sign} {sign}");
            // (STEP's at a height on a terrace's line, its lines clean.)
            let mut t = terrain("8 16 32 0\n0 24 8 40\n16 16 0 8", &shapes, 8);
            t.rough = Some(0.0);
            let s = t.surface();
            for y in 0..t.rows {
                for x in 0..t.cols {
                    let p = IVec2::new(x, y);
                    assert_eq!(s.at(t.column_of(p)), Some(t.heights[t.index(p).unwrap()]), "{sign} at point {p}");
                }
            }
        }
    }

    /// LINEAR goes straight from point to point; SMOOTH never past its four
    /// points; mixed shapes give every column a height between the
    /// lowest and the highest point (no holes, nothing thrown up).
    #[test]
    fn shapes_go_between_their_points_as_they_say() {
        let t = terrain("0 40", "^ ^", 10);
        let s = t.surface();
        // Columns 5 (point 0) to 15 (point 1): 4 voxels a column.
        for x in 5..=15 {
            assert_eq!(s.at(IVec2::new(x, 3)), Some((x - 5) * 4), "linear at {x}");
        }
        let t = terrain("0 40 0 40 0\n40 0 40 0 40", ". . . . .\n. . . . .", 10);
        let s = t.surface();
        assert!(s.top.iter().all(|&h| (0..=40).contains(&h)), "smooth stays within the points");
        let t = terrain("0 40 10 25 3\n40 0 40 0 40\n5 5 30 30 1", ". ^ # . ^\n# . ^ # .\n^ # . ^ #", 7);
        let s = t.surface();
        assert!(s.top.iter().all(|&h| (0..=40).contains(&h)), "mixed stay within the points");
        assert_eq!(s.top.len(), (35 * 21) as usize);
    }

    /// STEP cuts the smooth land into terraces: most columns on a ledge at
    /// a multiple of `terrace`; on a diagonal slope the cliffs run
    /// diagonally (the land's contours, not the grid's lines); a single
    /// high point makes a butte whose outline is round, not a square; the
    /// lines wander as `rough` says, the same every time.
    #[test]
    fn step_cuts_the_land_into_terraces() {
        let n = 8;
        let rows: Vec<String> = (0..n).rev().map(|y| (0..n).map(|x| (5 * (x + y)).to_string()).collect::<Vec<_>>().join(" ")).collect();
        let signs = vec![vec!["#"; n as usize].join(" "); n as usize].join("\n");
        let mut t = terrain(&rows.join("\n"), &signs, 8);
        t.terrace = Some(10);
        t.rough = Some(0.0);
        let s = t.surface();
        let on_ledges = s.top.iter().filter(|&&h| h % 10 == 0).count();
        assert!(on_ledges * 10 > s.top.len() * 6, "most on a ledge: {on_ledges} of {}", s.top.len());
        assert!(s.top.iter().all(|&h| (0..=70).contains(&h)));
        // Where each row first reaches the ledge at 30: further left the
        // further back (a diagonal cliff), never the same column.
        let first = |y: i32| (0..s.size.x).find(|&x| s.at(IVec2::new(x, y)).unwrap() >= 30).unwrap();
        let edges: Vec<i32> = (8..40).step_by(4).map(first).collect();
        assert!(edges.windows(2).all(|w| w[1] < w[0]), "the cliff runs diagonally: {edges:?}");
        // A butte: one high point among low ones.
        let mut b = terrain("0 0 0 0 0\n0 0 0 0 0\n0 0 48 0 0\n0 0 0 0 0\n0 0 0 0 0", "# # # # #\n# # # # #\n# # # # #\n# # # # #\n# # # # #", 16);
        b.terrace = Some(8);
        let s = b.surface();
        let up: Vec<IVec2> = (0..s.size.y).flat_map(|y| (0..s.size.x).map(move |x| IVec2::new(x, y))).filter(|&c| s.at(c).unwrap() >= 16).collect();
        let (lo, hi) = up.iter().fold((IVec2::MAX, IVec2::MIN), |(lo, hi), &c| (lo.min(c), hi.max(c)));
        let area = ((hi - lo + 1).x * (hi - lo + 1).y) as usize;
        assert!(up.len() * 100 < area * 88 && up.len() > 20, "round, not square: {} of {area}", up.len());
        // Rough: the lines wander, the same every time; clean without.
        let rough = Terrain { rough: Some(1.0), seed: Some(7), ..t.clone() };
        assert_ne!(rough.surface(), t.surface());
        assert_eq!(rough.surface(), rough.surface());
        assert_ne!(Terrain { seed: Some(8), ..rough.clone() }.surface(), rough.surface(), "the seed's own");
    }

    #[test]
    fn mistakes_say_their_line_and_word() {
        let bad = |src: &str| read_text(src).unwrap_err().to_string();
        let head = "[terrain]\nspacing = 8\n";
        let mat = "\n[[terrain.material]]\ncolor = \"#5c8a3a\"\n";
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2 3\n1 2\n\"\"\"\n{mat}")).contains("line 5: heights: this row has 2 points, the first has 3"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 x\n\"\"\"\n{mat}")).contains("line 4: heights: 'x' is no height"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\nshapes = \"\"\"\n. %\n\"\"\"\n{mat}")).contains("line 7: shapes: '%' is no shape"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\nshapes = \"\"\"\n. .\n. .\n\"\"\"\n{mat}")).contains("shapes has 2 rows, heights has 1"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\n\n[[terrain.material]]\ncolor = \"#5c8a3a\"\nabove = 3\n")).contains("last material (#5c8a3a) has conditions"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\n\n[[terrain.material]]\ncolor = \"green\"\n")).contains("line 8: color must be a color"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\n\n[[terrain.material]]\ncolour = \"#000000\"\n")).contains("'colour' is no field of a terrain's material"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\n\n[[terrain.material]]\ncolor = \"#5c8a3a\"\nsteeper = 95\n")).contains("steeper must be degrees"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\nroughness = \"\"\"\n3 x\n\"\"\"\n{mat}")).contains("line 7: roughness: 'x' is no roughness"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\n\n[[terrain.layer]]\nthick = 0\nhard = 1\ncolor = \"#000000\"\n{mat}")).contains("line 8: thick must be"));
        assert!(bad(&format!("{head}heights = \"\"\"\n1 2\n\"\"\"\n\n[[terrain.material]]\ncolors = [\"#5c8a3a\"]\n")).contains("colors must be two or three"));
    }

    /// Written and read again, the same terrain; heights and shapes in
    /// lined-up columns, the far row first.
    #[test]
    fn a_terrain_reads_back_as_written() {
        let mut t = terrain("4 4 6 10 14 12\n4 5 8 14 120 16", ". . . ^ ^ .\n. . # # ^ .", 16);
        t.seed = Some(7);
        (t.terrace, t.rough) = (Some(12), Some(0.25));
        t.at = [2, 3, 0];
        t.under = Some("#4a3a2a".into());
        t.materials.insert(0, Material { above: Some(60), grain: Some(0.5), blend: Some(8.0), ..Material::new("#eef2f5") });
        t.materials.insert(1, Material { steeper: Some(45.0), depth: Some(2), tones: vec!["#6d6458".into()], patch: Some(9), ..Material::new("#7d7468") });
        t.roughness = Some((0..t.heights.len()).map(|i| (i % 10) as u8).collect());
        t.layers = vec![Layer { thick: 6, hard: 0.9, color: "#b8603a".into() }, Layer { thick: 10, hard: 0.2, color: "#d98a5a".into() }];
        t.scatter = vec![
            Scatter { below: Some(70), flatter: Some(30.0), apart: Some(14), sink: Some(1), ..Scatter::new("fir.vvx") },
            Scatter { models: vec!["rock.vvx".into(), "rock2.vvx".into()], density: 0.5, steeper: Some(20.5), blend: Some(4.0), rotate: Some(Rotate::Any), as_: Some(ScatterAs::Voxels), ..Scatter::new("rock.vvx") },
        ];
        let text = text(&t);
        assert!(text.contains("  4   4   6  10  14  12\n  4   5   8  14 120  16\n"), "{text}");
        assert!(text.contains("  .   .   .   ^   ^   .\n  .   .   #   #   ^   .\n"), "{text}");
        assert!(text.contains("roughness = \"\"\"\n  6   7   8   9   0   1\n  0   1   2   3   4   5\n"), "{text}");
        assert!(text.contains("[[terrain.layer]]     # from the bottom up, again and again upward\nthick = 6\nhard = 0.9\ncolor = \"#b8603a\"\n"), "{text}");
        assert!(text.contains("colors = [\"#7d7468\", \"#6d6458\"]\nsteeper = 45\ndepth = 2\npatch = 9\n") && text.contains("blend = 8\n"), "{text}");
        assert!(text.contains("[[terrain.scatter]]   # models over the land: as pieces (the editor's SCATTER) or voxels\nmodel = \"fir.vvx\"\ndensity = 2\nbelow = 70\nflatter = 30\napart = 14\nsink = 1\n"), "{text}");
        assert!(text.contains("models = [\"rock.vvx\", \"rock2.vvx\"]\ndensity = 0.5\nsteeper = 20.5\nblend = 4\nrotate = \"any\"\nas = \"voxels\"\n"), "{text}");
        let back = read_text(&text).unwrap();
        assert_eq!(back, t);
    }

    /// A terrain that says nothing of the later words (roughness, layers,
    /// patches, blend) builds the very voxels it built before them.
    #[test]
    fn a_terrain_without_the_later_words_builds_as_before() {
        let mut t = terrain("4 9 30 2 12\n0 17 5 40 22\n12 12 3 8 60\n6 30 44 50 70", ". . ^ ^ #\n. # # ^ #\n# . . ^ #\n# # . . .", 12);
        t.materials = vec![
            Material { above: Some(50), flatter: Some(30.0), ..Material::new("#ffffff") },
            Material { steeper: Some(40.0), grain: Some(0.4), ..Material::new("#777777") },
            Material { grain: Some(0.3), ..Material::new("#50a050") },
        ];
        t.seed = Some(5);
        let mut world = World::new(IVec3::new(64, 48, 80), [[0; 4]; 256]);
        let p = t.painter(&mut world, IVec3::ZERO, t.surface()).unwrap();
        for k in p.chunks() {
            world.fill_chunk(k, |q| p.voxel(q));
        }
        println!("built: {:#x}", world.content_hash());
        assert_eq!(world.content_hash(), BEFORE);
    }

    /// A scatter rule spreads its models evenly (no clumps, no rows), never
    /// nearer than `apart`, only where the ground fits; the same every
    /// time, the seed's own.
    #[test]
    fn scatter_spreads_by_its_rules() {
        let mut t = ramp('.');
        t.scatter = vec![Scatter { models: vec!["a.vvx".into(), "b.vvx".into()], below: Some(60), flatter: Some(60.0), apart: Some(6), ..Scatter::new("a.vvx") }];
        t.scatter[0].density = 400.0;
        t.seed = Some(4);
        let s = t.surface();
        let placed = t.scatter(0, &s);
        assert!(placed.len() > 30, "{}", placed.len());
        assert_eq!(placed, t.scatter(0, &s), "the same every time");
        assert_ne!(placed, Terrain { seed: Some(5), ..t.clone() }.scatter(0, &s));
        for p in &placed {
            assert!(p.ground < 60 && p.ground == s.at(p.column).unwrap(), "below 60 on the land: {p:?}");
        }
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                assert!((a.column - b.column).length_squared() >= 36, "apart: {a:?} {b:?}");
            }
        }
        // Even over the ground that fits (x up to about 52 of 80): the
        // left and right halves of it alike, near and far alike.
        let (left, right) = (placed.iter().filter(|p| p.column.x < 26).count(), placed.iter().filter(|p| p.column.x >= 26).count());
        let (near, far) = (placed.iter().filter(|p| p.column.y < 24).count(), placed.iter().filter(|p| p.column.y >= 24).count());
        assert!(left.abs_diff(right) * 3 < placed.len() && near.abs_diff(far) * 3 < placed.len(), "{left} {right} {near} {far}");
        // No rows: the columns' x and y take many values.
        let xs: std::collections::BTreeSet<i32> = placed.iter().map(|p| p.column.x).collect();
        assert!(xs.len() * 2 > placed.len(), "no rows: {} values of x for {}", xs.len(), placed.len());
        assert!(placed.iter().any(|p| p.model == 1) && placed.iter().any(|p| p.model == 0), "both models");
        assert!(placed.iter().all(|p| p.rotation % 90.0 == 0.0), "quarters");
    }

    /// A ramp from 0 to 90 along x, everywhere SMOOTH unless `sign` says.
    fn ramp(sign: char) -> Terrain {
        let rows = ["0 10 20 30 40 50 60 70 80 90"; 6].join("\n");
        let signs = vec![vec![sign.to_string(); 10].join(" "); 6].join("\n");
        terrain(&rows, &signs, 8)
    }

    /// `rough` makes the land uneven where `roughness` lets it (0 calm, 9
    /// all of it): the same every time, the seed's own; a calm point's
    /// land as without.
    #[test]
    fn roughness_makes_the_land_uneven_where_painted() {
        let even = ramp('.');
        let rough = Terrain { rough: Some(1.0), seed: Some(3), ..even.clone() };
        let (a, b) = (even.surface(), rough.surface());
        let moved = a.top.iter().zip(&b.top).filter(|(x, y)| x != y).count();
        assert!(moved * 2 > a.top.len(), "uneven: {moved} of {}", a.top.len());
        assert!(a.top.iter().zip(&b.top).all(|(x, y)| (x - y).abs() <= 16), "a few voxels, not mountains");
        assert_eq!(b, rough.surface(), "the same every time");
        assert_ne!(b, Terrain { seed: Some(4), ..rough.clone() }.surface());
        // Calm on the left half (0), rough on the right (9).
        let mut painted = rough.clone();
        painted.roughness = Some((0..60).map(|i| if i % 10 < 5 { 0 } else { 9 }).collect());
        let p = painted.surface();
        for y in 0..48 {
            for x in 0..36 {
                let c = IVec2::new(x, y);
                assert_eq!(p.at(c), a.at(c), "calm at {c}");
            }
            assert_eq!(p.at(IVec2::new(76, y)), b.at(IVec2::new(76, y)), "rough at 76, {y}");
        }
    }

    /// Layers cut STEP's terraces: a hard band stands as a wall over a
    /// ledge at its bottom, a soft one as a long slope; the walls show the
    /// bands' colors where no conditional material fits, the ledges the
    /// material that fits them.
    #[test]
    fn layers_cut_hard_walls_and_soft_slopes() {
        let mut t = ramp('#');
        t.layers = vec![Layer { thick: 10, hard: 1.0, color: "#aa0000".into() }, Layer { thick: 10, hard: 0.0, color: "#00aa00".into() }];
        // (Level bands, a cake, to measure each band's riser.)
        (t.dip, t.fold) = (Some(0.0), Some(0.0));
        let s = t.surface();
        let row: Vec<i32> = (0..80).map(|x| s.at(IVec2::new(x, 20)).unwrap()).collect();
        // In a hard band (0 to 10, 20 to 30 ...): the land rises from its
        // bottom to its top within a few columns; in a soft one over many.
        let run = |lo: i32, hi: i32| row.iter().filter(|&&h| h > lo && h < hi).count();
        let (hard, soft) = (run(20, 30), run(30, 40));
        assert!(hard <= 3 && soft >= 5, "hard {hard} columns, soft {soft}: {row:?}");
        assert!(row.iter().filter(|&&h| h == 20).count() >= 3, "a ledge on top of the soft band below: {row:?}");
        t.materials = vec![Material { flatter: Some(20.0), ..Material::new("#ffffff") }, Material::new("#777777")];
        let mut world = World::new(IVec3::new(80, 48, 100), [[0; 4]; 256]);
        let p = t.painter(&mut world, IVec3::ZERO, s.clone()).unwrap();
        let rgb = |v: u8| [world.palette[v as usize][0], world.palette[v as usize][1], world.palette[v as usize][2]];
        let wall = (1..80).find(|&x| row[x] >= 30 && row[x - 1] < 25).unwrap() as i32;
        let lower = row[(wall - 1) as usize];
        assert_eq!(rgb(p.voxel(IVec3::new(wall, 20, lower + 1))), [0xaa, 0, 0], "the hard band's wall in its color");
        let ledge = (0..80).find(|&x| row[x] == 20 && row[x + 1] == 20 && row[x + 2] == 20).unwrap() as i32 + 1;
        assert_eq!(rgb(p.voxel(IVec3::new(ledge, 20, 19))), [255; 3], "the ledge: the material its flatness fits");
    }

    /// The layers lie as rock does: tilted by the dip (the same band
    /// higher on one side of the land than the other), folded, swelling
    /// and thinning, a thin one pinched out somewhere; the same every
    /// time, the seed's own; `dip = 0, fold = 0` flat as a cake; a fault
    /// steps the bands on one side of its line. A terrain without layers
    /// has nothing of it.
    #[test]
    fn bands_dip_fold_and_pinch_out() {
        let mut t = Terrain::flat(16, 40, 30, 60);
        t.layers = vec![Layer { thick: 12, hard: 0.9, color: "#9c4f2e".into() }, Layer { thick: 3, hard: 0.2, color: "#d9a46a".into() }, Layer { thick: 10, hard: 0.8, color: "#b8603a".into() }];
        (t.seed, t.dip_to, t.fold) = (Some(3), Some(90.0), Some(0.6));
        let (west, east) = (IVec2::new(20, 240), IVec2::new(620, 240));
        let tilt = t.strata_shift(east) - t.strata_shift(west);
        let want = libm::tan(3.0 * std::f64::consts::PI / 180.0) * 600.0;
        assert!((tilt - want).abs() < 12.0 + 4.0, "the bands tilt {tilt} across, the dip {want}");
        let along: Vec<f64> = (0..20).map(|k| t.strata_shift(IVec2::new(320, k * 24))).collect();
        assert!(along.iter().fold(0.0f64, |m, v| m.max((v - along[0]).abs())) > 1.0, "folded along the dip's strike too: {along:?}");
        let thick: Vec<Vec<f64>> = (0..40 * 30).map(|k| t.thicknesses(IVec2::new(k % 40 * 16, k / 40 * 16))).collect();
        assert!(thick.iter().any(|t| t[1] == 0.0), "the thin layer pinches out somewhere");
        assert!(thick.iter().any(|t| t[1] > 2.0), "and is there elsewhere");
        let (lo, hi) = thick.iter().fold((f64::MAX, 0.0f64), |(lo, hi), t| (lo.min(t[0]), hi.max(t[0])));
        assert!(lo < 11.0 && hi > 13.0, "the thick one swells and thins: {lo} to {hi}");
        assert_eq!(t.surface(), t.surface(), "the same every time");
        let other = Terrain { seed: Some(4), ..t.clone() };
        assert_ne!(other.strata_shift(IVec2::new(300, 300)), t.strata_shift(IVec2::new(300, 300)));
        // A cake, a fault.
        let cake = Terrain { dip: Some(0.0), fold: Some(0.0), ..t.clone() };
        assert!((0..50).all(|k| cake.strata_shift(IVec2::new(k * 12, k * 9)) == 0.0 && cake.thicknesses(IVec2::new(k * 12, k * 9)) == [12.0, 3.0, 10.0]));
        let faulted = Terrain { fault: Some(1.0), ..cake.clone() };
        let steps: std::collections::BTreeSet<i64> = (0..50).map(|k| faulted.strata_shift(IVec2::new(k * 12, k * 9)) as i64).collect();
        assert_eq!(steps.into_iter().collect::<Vec<_>>(), [0, 12], "the bands one layer up on one side of the fault");
        // Without layers: nothing.
        let bare = Terrain { layers: Vec::new(), ..t.clone() };
        assert_eq!(bare.strata_shift(east), 0.0);
    }

    /// A gently sloping terraced plateau (STEP, uneven, layers): its treads
    /// and the plateau's single contour steps show the material, never a
    /// layer's color; the layers show on walls of a few voxels and more.
    #[test]
    fn treads_show_their_material_and_walls_their_bands() {
        let rows = ["40 41 42 43 44 45 46 47 48 49"; 6].join("\n");
        let signs = ["# # # # # # # # # #"; 7].join("\n");
        let mut t = terrain(&format!("{rows}\n0 0 0 0 0 0 0 0 0 0"), &signs, 8);
        t.rough = Some(0.5);
        t.seed = Some(2);
        t.layers = vec![Layer { thick: 10, hard: 0.9, color: "#aa0000".into() }, Layer { thick: 6, hard: 0.2, color: "#00aa00".into() }];
        t.materials = vec![Material { flatter: Some(30.0), ..Material::new("#ffffff") }, Material::new("#777777")];
        let s = t.surface();
        let mut world = World::new(IVec3::new(80, 56, 100), [[0; 4]; 256]);
        let p = t.painter(&mut world, IVec3::ZERO, s.clone()).unwrap();
        let rgb = |v: u8| [world.palette[v as usize][0], world.palette[v as usize][1], world.palette[v as usize][2]];
        let band = |c: [u8; 3]| c == [0xaa, 0, 0] || c == [0, 0xaa, 0];
        let (mut tops, mut walls) = (0, 0);
        for y in 0..56 {
            for x in 0..80 {
                let c = IVec2::new(x, y);
                let h = s.at(c).unwrap();
                let low = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y].iter().filter_map(|&d| s.at(c + d)).min().unwrap();
                if h - low < WALL {
                    assert!(!band(rgb(p.voxel(IVec3::new(x, y, h - 1)))), "a tread at {c} ({h}, the lowest beside it {low}) shows no band");
                    tops += 1;
                } else if band(rgb(p.voxel(IVec3::new(x, y, low)))) {
                    walls += 1;
                }
            }
        }
        assert!(tops > 1000 && walls > 50, "{tops} treads, {walls} walls in bands");
    }

    /// `colors` mixes a material's tones in patches; `blend` lets a
    /// border wander over that many voxels of height, lower in hollows.
    #[test]
    fn patches_and_soft_borders() {
        let mut t = ramp('.');
        t.materials = vec![Material { above: Some(60), blend: Some(8.0), ..Material::new("#ffffff") }, Material { tones: vec!["#408040".into()], patch: Some(5), ..Material::new("#50a050") }];
        t.seed = Some(9);
        let s = t.surface();
        let mut world = World::new(IVec3::new(80, 48, 100), [[0; 4]; 256]);
        let p = t.painter(&mut world, IVec3::ZERO, s.clone()).unwrap();
        let top = |x: i32, y: i32| p.voxel(IVec3::new(x, y, s.at(IVec2::new(x, y)).unwrap() - 1));
        let snow = world.palette.iter().position(|c| c[..3] == [255, 255, 255]).unwrap() as u8;
        let (mut lowest_snow, mut highest_grass) = (i32::MAX, 0);
        let mut tones = std::collections::BTreeSet::new();
        let mut same = 0;
        for y in 0..48 {
            for x in 0..80 {
                let h = s.at(IVec2::new(x, y)).unwrap();
                if h == 0 {
                    continue;
                }
                if top(x, y) == snow {
                    lowest_snow = lowest_snow.min(h);
                } else {
                    highest_grass = highest_grass.max(h);
                    tones.insert(top(x, y));
                    same += (x > 0 && top(x - 1, y) == top(x, y)) as i32;
                }
            }
        }
        assert!((60 - 4 - 4..60).contains(&lowest_snow), "snow down to {lowest_snow}: the border wanders below 60");
        assert!((60..=64).contains(&highest_grass), "grass up to {highest_grass}");
        assert_eq!(tones.len(), 2, "both tones");
        assert!(same > 30 * 48, "in patches, not voxel by voxel: {same}");
    }

    /// The materials by height and slope: snow above 60, rock where steep,
    /// grass else; under below; grain in a few shades, the same every
    /// time.
    #[test]
    fn materials_go_by_height_and_slope() {
        let mut t = terrain("10 10 10 10\n10 10 80 10\n10 10 10 10", "^ ^ ^ ^\n^ ^ ^ ^\n^ ^ ^ ^", 8);
        t.materials = vec![
            Material { above: Some(60), flatter: Some(30.0), ..Material::new("#ffffff") },
            Material { steeper: Some(45.0), ..Material::new("#777777") },
            Material { grain: Some(1.0), ..Material::new("#50a050") },
        ];
        t.under = Some("#302010".into());
        let mut world = World::new(IVec3::new(64, 64, 128), [[0; 4]; 256]);
        let p = t.painter(&mut world, IVec3::ZERO, t.surface()).unwrap();
        let rgb = |v: u8| [world.palette[v as usize][0], world.palette[v as usize][1], world.palette[v as usize][2]];
        // The peak (point 2, 1): snow on top.
        assert_eq!(rgb(p.voxel(IVec3::new(20, 12, 79))), [255; 3]);
        assert_eq!(p.voxel(IVec3::new(20, 12, 80)), 0, "nothing above the top");
        // Its flank is steep: rock at the top and down the face.
        let top = p.surface.at(IVec2::new(16, 12)).unwrap();
        assert!(top < 60);
        assert_eq!(rgb(p.voxel(IVec3::new(16, 12, top - 1))), [0x77; 3]);
        let below = p.surface.at(IVec2::new(14, 12)).unwrap();
        assert_eq!(rgb(p.voxel(IVec3::new(15, 12, below))), [0x77; 3], "its face over the next column is rock");
        // The flat land round it: grass in shades, the same each time.
        let grass: Vec<u8> = (0..8).map(|x| p.voxel(IVec3::new(x, 0, 9))).collect();
        assert!(grass.iter().all(|&v| rgb(v)[1] > rgb(v)[0]), "green");
        assert!(grass.windows(2).any(|w| w[0] != w[1]), "grain");
        assert_eq!(grass, (0..8).map(|x| p.voxel(IVec3::new(x, 0, 9))).collect::<Vec<_>>());
        // Deep down: under.
        assert_eq!(rgb(p.voxel(IVec3::new(2, 2, 2))), [0x30, 0x20, 0x10]);
        // Palette: the materials in order, grain's shades after its own.
        assert_eq!(rgb(1), [255; 3]);
        assert_eq!(rgb(2), [0x77; 3]);
        assert_eq!(rgb(3), [0x50, 0xa0, 0x50]);
    }
}
