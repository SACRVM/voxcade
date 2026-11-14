//! The level's kinds of things and its pieces' models as the editor draws
//! and finds them (`Figures`), and pictures of them seen from the front.

use super::*;

/// The kinds of things as the level shows them (`c.things.NAME`): each
/// one's voxels from where it stands, as games at rest show it, a figure
/// (it walks or flies) seen from the side, anything else turned to the
/// camera; a kind without a model a pale column as wide and tall as its
/// body. And the models the level's pieces are made of (by their files),
/// drawn as the game draws them (`session::model_cubes`).
/// Each figure and model keeps its filled cells too, for a click to find
/// the voxel it meets (`crate::editor::pick`).
pub struct Figures {
    /// What things show as, by key (`key_in`): a kind's name, and `NAME@WAY`
    /// a kind or a piece's model facing one of the four ways.
    looks: BTreeMap<String, KindLook>,
    /// The pieces' models and their filled cells, by their files.
    pub(super) models: BTreeMap<String, crate::vox::VoxModel>,
    cells: BTreeMap<String, crate::editor::pick::Cells>,
}

pub(super) struct KindLook {
    /// Voxels and colours, from the spot it stands on.
    voxels: Vec<(IVec3, [u8; 3])>,
    /// The box around them, from that spot.
    lo: IVec3,
    hi: IVec3,
    /// Its filled cells, from `lo`.
    cells: crate::editor::pick::Cells,
}

impl KindLook {
    pub(super) fn new(voxels: Vec<(IVec3, [u8; 3])>) -> Self {
        let lo = voxels.iter().fold(IVec3::MAX, |a, v| a.min(v.0));
        let hi = voxels.iter().fold(IVec3::MIN, |a, v| a.max(v.0 + 1));
        let (lo, hi) = if voxels.is_empty() { (IVec3::ZERO, IVec3::ONE) } else { (lo, hi) };
        let cells = crate::editor::pick::Cells::of(hi - lo, voxels.iter().map(|v| v.0 - lo));
        Self { voxels, lo, hi, cells }
    }
}

/// The cells a model's seen voxels fill (in its own box).
pub(super) fn model_cells(m: &crate::vox::VoxModel) -> crate::editor::pick::Cells {
    crate::editor::pick::Cells::of(m.size, m.visible.first().into_iter().flatten().map(|&k| m.frames[0][k as usize].0))
}

/// A model's voxel from its pivot turned clockwise, as seen from above,
/// from facing north (its forward) to facing `way`.
pub(super) fn turned_to(way: crate::grid::Way, l: IVec3) -> IVec3 {
    match way {
        crate::grid::Way::North => l,
        crate::grid::Way::East => IVec3::new(l.y, -l.x, l.z),
        crate::grid::Way::South => IVec3::new(-l.x, -l.y, l.z),
        crate::grid::Way::West => IVec3::new(-l.y, l.x, l.z),
    }
}

/// Behaviors whose things go about seen from the side.
pub(super) const SIDEWAYS: [&str; 4] = ["hero", "walker", "hopper", "flyer"];

/// A kind without a model.
pub(super) const MARKER: [u8; 3] = [150, 200, 240];

impl Figures {
    pub fn load(dir: &Path, cfg: &GameConfig) -> Self {
        let mut figures = BTreeMap::new();
        for (kind, table) in &cfg.things {
            let start = start_way(table);
            let number = |key: &str| table.get(key).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
            let model = table.get("model").and_then(|m| m.as_str()).and_then(|file| crate::vox::load_model(&dir.join(file)).ok());
            // Facing each way as a level's file may turn it (its forward
            // north), by `key`.
            for way in crate::grid::Way::ALL {
                let voxels: Vec<(IVec3, [u8; 3])> = match &model {
                    Some(m) => {
                        let pivot = IVec3::new(m.size.x / 2, m.size.y / 2, 0);
                        m.visible[0].iter().map(|&k| m.frames[0][k as usize]).map(|(p, c)| (turned_to(way, p - pivot), [c[0], c[1], c[2]])).collect()
                    }
                    None => Vec::new(),
                };
                if !voxels.is_empty() {
                    figures.insert(format!("{kind}@{}", way.name()), KindLook::new(voxels));
                }
            }
            let voxels: Vec<(IVec3, [u8; 3])> = match model {
                Some(m) => {
                    let pivot = IVec3::new(m.size.x / 2, m.size.y / 2, 0);
                    m.visible[0]
                        .iter()
                        .map(|&k| {
                            let (p, color) = m.frames[0][k as usize];
                            // Turned as the game turns it as a run starts
                            // (`start_way`), about the model's pivot: the
                            // way its kind faces (Voxkeep's bat to the
                            // front, its wings spread), else a figure to
                            // the side, anything else to the camera.
                            (turned_to(start, p - pivot), [color[0], color[1], color[2]])
                        })
                        .collect()
                }
                None => {
                    let w = number("width").unwrap_or(4.0).round().max(1.0) as i32;
                    let h = number("height").unwrap_or(16.0).round().max(1.0) as i32;
                    let mut column = Vec::new();
                    for z in 0..h {
                        for y in -w / 2..w - w / 2 {
                            for x in -w / 2..w - w / 2 {
                                column.push((IVec3::new(x, y, z), MARKER));
                            }
                        }
                    }
                    column
                }
            };
            figures.insert(kind.clone(), KindLook::new(voxels));
        }
        Self { looks: figures, models: BTreeMap::new(), cells: BTreeMap::new() }
    }

    /// The sizes of the models loaded, by their files (`Editor::sizes`).
    pub fn sizes(&self) -> BTreeMap<String, IVec3> {
        self.models.iter().map(|(f, m)| (f.clone(), m.size)).collect()
    }

    /// The model of the file `model` (its path in the game folder), if it
    /// was loaded (`with_pieces`, `load_models`).
    pub fn model(&self, model: &str) -> Option<&crate::vox::VoxModel> {
        self.models.get(model)
    }

    /// Loads the models of `files` not loaded yet (a model that does not
    /// read is left out: the game says why).
    pub fn load_models<'a>(&mut self, dir: &Path, files: impl IntoIterator<Item = &'a str>) {
        for file in files {
            if !self.models.contains_key(file)
                && let Ok(m) = crate::vox::load_model(&dir.join(file))
            {
                self.cells.insert(file.to_string(), model_cells(&m));
                self.models.insert(file.to_string(), m);
            }
        }
    }

    /// The models of the level's pieces and of what hangs on the mouse,
    /// loaded where new.
    pub fn follow(&mut self, dir: &Path, ed: &Editor) {
        let hanging = match &ed.pieces.hanging {
            Some(Hanging::Thing(t)) => t.model.as_deref(),
            _ => None,
        };
        let room = ed.context.iter().flat_map(|c| c.models().map(str::to_string));
        let files: Vec<String> = ed.level.things.iter().filter_map(|t| t.model.clone()).chain(hanging.map(str::to_string)).chain(room).collect();
        if files.iter().any(|f| !self.models.contains_key(f)) {
            self.load_models(dir, files.iter().map(String::as_str));
        }
    }

    /// The box round a model's voxels from its pivot cell's low corner,
    /// unturned (a piece without a model: a small box round its light).
    pub fn piece_box(&self, t: &crate::grid::Thing) -> (Vec3, Vec3) {
        match t.model.as_deref().and_then(|m| self.model(m)) {
            Some(m) => {
                let lo = -IVec3::new(m.size.x / 2, m.size.y / 2, 0).as_vec3();
                (lo, lo + m.size.as_vec3())
            }
            None => (Vec3::new(-2.0, -2.0, 0.0), Vec3::new(3.0, 3.0, 4.0)),
        }
    }

    /// The level's pieces too (`grid::PIECE`): each model it sets down,
    /// turned each of the four ways, by `key` (a piece turned freely shows
    /// at its nearest way: the editor stays on the grid). A model that does
    /// not read is left out (the game says why).
    pub fn with_pieces(mut self, dir: &Path, level: &crate::grid::GridLevel) -> Self {
        for model in level.things.iter().filter_map(|t| t.model.as_ref()) {
            if self.looks.contains_key(&format!("{model}@north")) {
                continue;
            }
            let Ok(m) = crate::vox::load_model(&dir.join(model)) else { continue };
            let pivot = IVec3::new(m.size.x / 2, m.size.y / 2, 0);
            for way in crate::grid::Way::ALL {
                // A model's forward is north; turned clockwise as seen
                // from above to face its way.
                let turn = |l: IVec3| turned_to(way, l);
                let voxels: Vec<(IVec3, [u8; 3])> = m.visible[0]
                    .iter()
                    .map(|&k| {
                        let (p, color) = m.frames[0][k as usize];
                        (turn(p - pivot), [color[0], color[1], color[2]])
                    })
                    .collect();
                self.looks.insert(format!("{model}@{}", way.name()), KindLook::new(voxels));
            }
            self.cells.insert(model.clone(), model_cells(&m));
            self.models.insert(model.clone(), m);
        }
        self
    }

    /// What a thing of the level shows as: its kind (facing its way where
    /// the file turns it and its kind has a model), or a piece's model
    /// facing its way (`with_pieces`).
    pub fn key(t: &crate::grid::Thing) -> String {
        let way = |f: Option<crate::grid::Facing>| crate::grid::Way::nearest(f.map_or(180.0, |f| f.bearing())).name();
        match (&t.model, t.facing) {
            (Some(model), f) => format!("{model}@{}", way(f)),
            (None, Some(f)) => format!("{}@{}", t.kind, way(Some(f))),
            (None, None) => t.kind.clone(),
        }
    }

    /// `key`, where the figures have it, else the kind's own (a kind
    /// without a model looks the same every way).
    pub fn key_in(&self, t: &crate::grid::Thing) -> String {
        let key = Self::key(t);
        if self.looks.contains_key(&key) { key } else { t.kind.clone() }
    }

    /// A figure's filled cells and where its box begins, from the spot
    /// it stands on (`key`).
    pub fn figure_cells(&self, key: &str) -> Option<(&crate::editor::pick::Cells, IVec3)> {
        self.looks.get(key).map(|f| (&f.cells, f.lo))
    }

    /// A model's filled cells, from its box's low corner.
    pub fn model_cells(&self, model: &str) -> Option<&crate::editor::pick::Cells> {
        self.cells.get(model)
    }

    /// The box around a kind's voxels, from the spot it stands on.
    pub fn bounds(&self, kind: &str) -> (IVec3, IVec3) {
        self.looks.get(kind).map_or((IVec3::new(-2, -2, 0), IVec3::new(2, 2, 8)), |f| (f.lo, f.hi))
    }

    /// A kind standing at `at` as cubes: tinted halfway to `tint`, and
    /// see-through (opacity 0..255).
    pub(super) fn cubes(&self, kind: &str, at: IVec3, tint: Option<[u8; 3]>, opacity: u8, out: &mut Vec<Instance>) {
        let Some(f) = self.looks.get(kind) else { return };
        for &(l, c) in &f.voxels {
            let c = tint.map_or(c, |t| std::array::from_fn(|i| ((c[i] as u16 + t[i] as u16) / 2) as u8));
            let p = (at + l).as_vec3() + 0.5;
            out.push(Instance { pos_size: [p.x, p.y, p.z, 1.0], color: [c[0], c[1], c[2], 255], fx: [0, opacity, 0, 0], shade: [0; 4], turn: [0; 4] });
        }
    }

    /// A kind seen from the front, as a picture at most `fit` pixels each
    /// way (rows from the top): its width, height and pixels.
    pub fn picture(&self, kind: &str, fit: i32) -> (i32, i32, Vec<Option<[u8; 3]>>) {
        let Some(f) = self.looks.get(kind) else { return (0, 0, Vec::new()) };
        picture_of(&f.voxels, f.lo, f.hi, fit)
    }
}

/// Voxels seen from the front (the nearest of each column), as a picture
/// at most `fit` pixels each way (rows from the top).
pub fn front_picture(voxels: &[(IVec3, [u8; 3])], fit: i32) -> (i32, i32, Vec<Option<[u8; 3]>>) {
    if voxels.is_empty() {
        return (1, 1, vec![None]);
    }
    let lo = voxels.iter().fold(IVec3::MAX, |a, v| a.min(v.0));
    let hi = voxels.iter().fold(IVec3::MIN, |a, v| a.max(v.0 + 1));
    picture_of(voxels, lo, hi, fit)
}

pub(super) fn picture_of(voxels: &[(IVec3, [u8; 3])], lo: IVec3, hi: IVec3, fit: i32) -> (i32, i32, Vec<Option<[u8; 3]>>) {
    {
        let f = KindLook { voxels: voxels.to_vec(), lo, hi, cells: Default::default() };
        let size = f.hi - f.lo;
        let mut front: Vec<Option<(i32, [u8; 3])>> = vec![None; (size.x * size.z) as usize];
        for &(l, c) in &f.voxels {
            let p = l - f.lo;
            let i = ((size.z - 1 - p.z) * size.x + p.x) as usize;
            if front[i].is_none_or(|(y, _)| p.y < y) {
                front[i] = Some((p.y, c));
            }
        }
        let big = size.x.max(size.z).max(1);
        let scale = |n: i32| if big <= fit { n } else { (n * fit + big - 1) / big };
        let (w, h) = (scale(size.x), scale(size.z));
        let pixels = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let (sx, sy) = if big <= fit { (x, y) } else { ((x * big + big / 2) / fit, (y * big + big / 2) / fit) };
                front.get((sy.min(size.z - 1) * size.x + sx.min(size.x - 1)) as usize).copied().flatten().map(|(_, c)| c)
            })
            .collect();
        (w, h, pixels)
    }
}

/// A block seen from the front: of each column the first voxel from the
/// front (rows from the top).
pub fn front(block: &Block) -> Vec<Option<[u8; 3]>> {
    let s = block.size;
    (0..s.x * s.z).map(|i| (0..s.y).find_map(|y| block.colour_at(IVec3::new(i % s.x, y, s.z - 1 - i / s.x)))).collect()
}

/// How dark a block's front shows beside its top in `above_front`: the
/// two faces told apart, as the level's view tells them.
const FRONT_SHADE: f32 = 0.78;

/// A block seen from above and the front at once, as a level's view sees
/// it (`above`: a level seen from above, its top whole and its front at
/// half; else from the front, its front whole and its top at half),
/// `high` pixels high and as wide as that makes it: blocks that differ
/// only on top (grass, a path, a pond) differ here. Its width, height and
/// pixels (rows from the top), each pixel the voxel its ray meets first,
/// the front's darker.
pub fn above_front(block: &Block, high: i32, above: bool) -> (i32, i32, Vec<Option<[u8; 3]>>) {
    let s = block.size.max(IVec3::ONE).as_vec3();
    // Down the picture a point lies at `a (s.y - y) + b (s.z - z)`: the
    // top's rows `a` a voxel, the front's `b`.
    let (a, b) = if above { (1.0, 0.5) } else { (0.5, 1.0) };
    let tall = a * s.y + b * s.z;
    let wide = ((high as f32 * s.x / tall).round() as i32).max(1);
    let mut pixels = Vec::with_capacity((wide * high) as usize);
    for py in 0..high {
        for px in 0..wide {
            let x = (px as f32 + 0.5) / wide as f32 * s.x;
            let v = (py as f32 + 0.5) / high as f32 * tall;
            pixels.push(ray_above_front(block, s, (a, b), x, v));
        }
    }
    (wide, high, pixels)
}

/// The colour a ray of `above_front` meets: down the picture at `v`
/// (voxels, as `ab` weighs them), across at `x`.
fn ray_above_front(block: &Block, s: glam::Vec3, (a, b): (f32, f32), x: f32, v: f32) -> Option<[u8; 3]> {
    let z_at = |y: f32| s.z - (v - a * (s.y - y)) / b;
    // Into the box: on its front (y 0) where that lies below its top,
    // else on its top.
    let (mut y, mut top) = if z_at(0.0) > s.z { (s.y - v / a, true) } else { (0.0, false) };
    let cx = x.floor() as i32;
    let mut last: Option<IVec3> = None;
    // (Small steps along y: z goes down `a / b` a step.)
    let step = 0.05 * (b / a).min(1.0);
    while y < s.y {
        let z = z_at(y);
        if z < 0.0 {
            break;
        }
        let cell = IVec3::new(cx, y.floor() as i32, (z.floor() as i32).min(s.z as i32 - 1));
        if last != Some(cell) {
            if let Some(was) = last {
                top = cell.z < was.z;
            }
            last = Some(cell);
            if let Some(c) = block.colour_at(cell) {
                return Some(if top { c } else { c.map(|v| (v as f32 * FRONT_SHADE).round() as u8) });
            }
        }
        y += step;
    }
    None
}
