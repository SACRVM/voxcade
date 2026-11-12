//! The editor's picture: the level or the open block in the engine's own
//! light and shadows, seen from any side (an orbit around what is shown),
//! and a few calm panels in the voxel UI: the tools on the right, the
//! level's blocks (its kinds of things, or the block's colours) along the
//! bottom. Words and pictures there are what the mouse clicks (`Spot`).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use glam::{IVec3, Mat4, Vec3, Vec4};

use super::model::{Editor, Hit, Layer, Scope, Tool};
use crate::blocks::{Block, Turn};
use crate::config::{CameraConfig, GameConfig, LightConfig, ShadowStyle};
use crate::grid::{Cell, Kind};
use crate::render::{FrameData, Instance};
use crate::world::World;

/// How the camera looks at what is shown: turned about it (degrees), how
/// wide the view is there (voxels), and where it looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub target: Vec3,
}

impl Orbit {
    pub fn camera(&self) -> CameraConfig {
        CameraConfig { yaw: self.yaw, pitch: self.pitch, fov: 40.0, view_width: self.zoom, ..CameraConfig::default() }
    }

    /// Turned by a drag across `dx` and down `dy` (shares of the screen):
    /// what is shown turns with the mouse, as if held.
    pub fn turn(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw + dx * 300.0).rem_euclid(360.0);
        self.pitch = (self.pitch + dy * 200.0).clamp(-85.0, 85.0);
    }

    /// Moved by a drag (shares of the screen): what is shown goes with the
    /// mouse.
    pub fn pan(&mut self, dx: f32, dy: f32, aspect: f32) {
        let yaw = self.yaw.to_radians();
        let right = Vec3::new(yaw.cos(), -yaw.sin(), 0.0);
        let up = right.cross(crate::camera::view_dir(&self.camera()));
        self.target -= right * dx * self.zoom - up * dy * self.zoom / aspect;
    }

    /// Nearer (steps above 0) or further.
    pub fn zoom_by(&mut self, steps: f32) {
        self.zoom = (self.zoom * 1.15f32.powf(-steps)).clamp(4.0, 4000.0);
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        crate::camera::view_proj(&self.camera(), self.target, aspect)
    }
}

/// The ray through a point of the screen (shares, from the top left):
/// where it starts and which way it goes.
pub fn mouse_ray(view_proj: Mat4, x: f32, y: f32) -> (Vec3, Vec3) {
    let inv = view_proj.inverse();
    let at = |depth: f32| {
        let p = inv * Vec4::new(2.0 * x - 1.0, 1.0 - 2.0 * y, depth, 1.0);
        p.truncate() / p.w
    };
    let (near, far) = (at(0.0), at(1.0));
    (near, (far - near).normalize())
}

/// What the editor shows besides the grid, and how it is seen.
pub struct View {
    pub level: Orbit,
    pub block: Orbit,
    /// Where the mouse points in the grid shown.
    pub hover: Option<Hit>,
    /// Among the things: where one put down would stand, and the thing the
    /// mouse is over (its place in the level's list).
    pub spot: Option<IVec3>,
    pub under: Option<usize>,
    /// The bottom bar shows the tileset to draw a new block out of.
    pub choosing: bool,
    /// How much of the window the panels take (shares: on the right, at
    /// the bottom): the view's middle is the middle of the rest.
    pub inset: glam::Vec2,
    /// The side the game shows a model from, and whether its frames play
    /// on the panel.
    pub face: Face,
    pub playing: bool,
    /// The floor under the grid shows (the editor's, to build on).
    pub floor: bool,
    /// Ctrl is down: BOX and SPHERE cut away (their shape shown red, in
    /// the voxels).
    pub cut: bool,
    /// Alt is down: BOX and SPHERE fill only where it is empty.
    pub only_empty: bool,
    /// Where voxel 0, 0, 0 of the grid shown stood in a figure's scene
    /// when last drawn (`Editor::origin`): the view goes with the figure
    /// when another part is chosen or a part's box grows on its low side.
    pub origin: Option<IVec3>,
}

/// How fast a model's frames play (on the editor's panel, in the IDE's
/// picture).
pub const FRAMES_PER_SECOND: f64 = 8.0;

/// The side of a model the game shows the camera. Models face +y in their
/// files, and the game turns a thing to where it looks (`Session::model`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// A figure looking right: its +x side (the hero, as a run starts).
    Right,
    /// A figure looking left: its −x side (the other figures, looking at
    /// the hero, who comes from the left).
    Left,
    /// Turned to the camera: its front, +y (props, pickups, doors).
    Front,
    /// A block as it lies in the level: its y 0 side, the level's front.
    Block,
}

/// The side the game shows of the model `file` (its path in the game
/// folder) as a run starts: of a block its front as it lies; of a figure
/// (a kind with it whose behaviour walks or flies) the side it looks to,
/// the hero right, the others left; else its front.
pub fn face_of(file: &str, cfg: &GameConfig) -> Face {
    if file.starts_with(&format!("{}/", crate::blocks::FOLDER)) {
        return Face::Block;
    }
    let behaviour = cfg.things.values().filter(|t| t.get("model").and_then(|m| m.as_str()) == Some(file)).find_map(|t| t.get("behaviour").and_then(|b| b.as_str()));
    match behaviour {
        Some("hero") => Face::Right,
        Some(b) if SIDEWAYS.contains(&b) => Face::Left,
        _ => Face::Front,
    }
}

/// A model's frame seen from `face` as the game shows it, the nearest
/// voxel of each column: its width, height and pixels (rows from the
/// top). A figure's front is on the side it looks to.
pub fn seen(frame: &Block, face: Face) -> (i32, i32, Vec<Option<[u8; 3]>>) {
    let s = frame.size;
    let w = match face {
        Face::Right | Face::Left => s.y,
        Face::Front | Face::Block => s.x,
    };
    let pixels = (0..w * s.z)
        .map(|i| {
            let (c, z) = (i % w, s.z - 1 - i / w);
            // The voxels along the line of sight, the nearest first.
            match face {
                Face::Right => (0..s.x).rev().find_map(|x| frame.colour_at(IVec3::new(x, c, z))),
                Face::Left => (0..s.x).find_map(|x| frame.colour_at(IVec3::new(x, s.y - 1 - c, z))),
                Face::Front => (0..s.y).rev().find_map(|y| frame.colour_at(IVec3::new(s.x - 1 - c, y, z))),
                Face::Block => (0..s.y).find_map(|y| frame.colour_at(IVec3::new(c, y, z))),
            }
        })
        .collect();
    (w, s.z, pixels)
}

impl View {
    /// A first look: the level from the front, a little from above, where
    /// the game's player starts; a block from the front left.
    pub fn new(ed: &Editor, cfg: &GameConfig) -> Self {
        let n = ed.level.n as f32;
        let size = ed.level.size.as_vec3() * n;
        let margin = cfg.world.blocks.as_ref().map_or(0, |b| b.margin) as f32;
        let x = (cfg.player.spawn[0] - margin).clamp(0.0, size.x);
        let column = (x / n) as i32;
        let top = (0..ed.level.size.z).rev().find(|&z| (0..ed.level.size.y).any(|y| ed.level.cell(IVec3::new(column, y, z)).block != 0));
        let z = top.map_or(size.z / 2.0, |t| (t + 1) as f32 * n);
        let level = Orbit { yaw: 0.0, pitch: cfg.camera.pitch, zoom: cfg.camera.view_width, target: Vec3::new(x, n / 2.0, z) };
        let face = face_of(&ed.file, cfg);
        let block = match ed.scope {
            // A model as the game shows it as a run starts (`face`), a
            // little turned so that some of its front shows, all of it in
            // view.
            Scope::Frame(_) | Scope::Figure => {
                let size = ed.grid().as_vec3();
                let yaw = match face {
                    Face::Right => 250.0,
                    Face::Left => 110.0,
                    Face::Front => 160.0,
                    Face::Block => -30.0,
                };
                Orbit { yaw, pitch: 20.0, zoom: (size.max_element() * 2.8).max(16.0), target: size / 2.0 }
            }
            _ => Orbit { yaw: -30.0, pitch: 25.0, zoom: n * 4.5, target: Vec3::splat(n / 2.0) },
        };
        Self { level, block, hover: None, spot: None, under: None, choosing: false, inset: glam::Vec2::ZERO, face, playing: true, floor: true, cut: false, only_empty: false, origin: ed.origin() }
    }

    pub fn orbit(&mut self, scope: Scope) -> &mut Orbit {
        match scope {
            Scope::Level => &mut self.level,
            Scope::Block(_) | Scope::Frame(_) | Scope::Figure => &mut self.block,
        }
    }

    /// The view kept on the figure as the grid under it moves (another
    /// part chosen, the figure whole again, a part's box grown or trimmed
    /// on its low side, a step back): the figure stays still on the screen.
    pub fn follow_origin(&mut self, ed: &Editor) {
        let now = ed.origin();
        if let (Some(was), Some(now)) = (self.origin, now)
            && was != now
        {
            self.block.target += (was - now).as_vec3();
            self.hover = None;
        }
        self.origin = now;
    }

    /// The camera's matrix for `orbit`, its middle moved into the room the
    /// panels leave.
    pub fn view_proj(&self, orbit: &Orbit, aspect: f32) -> Mat4 {
        Mat4::from_translation(Vec3::new(-self.inset.x, self.inset.y, 0.0)) * orbit.view_proj(aspect)
    }

    /// Where a point of the screen (shares) points in the grid shown.
    pub fn aim(&self, ed: &Editor, aspect: f32, x: f32, y: f32) -> Option<Hit> {
        let (origin, dir) = self.ray(ed, aspect, x, y);
        ed.aim(origin, dir)
    }

    /// The ray through a point of the screen (shares), in the shown grid's
    /// cells.
    pub fn ray(&self, ed: &Editor, aspect: f32, x: f32, y: f32) -> (Vec3, Vec3) {
        let (orbit, unit) = match ed.scope {
            Scope::Level => (&self.level, ed.level.n as f32),
            Scope::Block(_) | Scope::Frame(_) | Scope::Figure => (&self.block, 1.0),
        };
        let (origin, dir) = mouse_ray(self.view_proj(orbit, aspect), x, y);
        (origin / unit, dir)
    }

    /// Where a point of the screen (shares) points among the level's
    /// things: the spot a thing would stand on (on the voxels shown) and
    /// the nearest thing in front of them there.
    pub fn aim_things(&self, ed: &Editor, shown: &Shown, figures: &Figures, aspect: f32, x: f32, y: f32) -> (Option<IVec3>, Option<usize>) {
        let (origin, dir) = mouse_ray(self.view_proj(&self.level, aspect), x, y);
        things_along(ed, &shown.world, figures, origin, dir)
    }
}

/// Where a ray (in voxels) meets the level's things: the spot a thing
/// would stand on (on the voxels of `world`) and the nearest thing in
/// front of them along it.
pub fn things_along(ed: &Editor, world: &World, figures: &Figures, origin: Vec3, dir: Vec3) -> (Option<IVec3>, Option<usize>) {
    let hit = super::model::pick(origin, dir, world.size, |c| c.cmpge(IVec3::ZERO).all() && world.get(c) != 0);
    let spot = hit.and_then(|h| ed.thing_spot(h.cell, h.normal));
    let wall = hit.and_then(|h| ray_box(origin, dir, h.cell.as_vec3(), h.cell.as_vec3() + 1.0)).unwrap_or(f32::INFINITY);
    let under = ed
        .level
        .things
        .iter()
        .enumerate()
        .filter(|&(i, _)| Some(i) != ed.moving())
        .filter_map(|(i, t)| {
            let (lo, hi) = figures.bounds(&t.kind);
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

/// The world the 3D view draws: the level as built, or the open block;
/// brought up to the editor by building again only the cells that changed.
pub struct Shown {
    pub world: World,
    scope: Scope,
    size: IVec3,
    cells: Vec<Cell>,
    blocks: Vec<(Block, Kind)>,
    /// The block or frame shown, as built.
    shape: Option<Block>,
}

impl Shown {
    pub fn new(ed: &Editor) -> Result<Self> {
        let mut shown = Self { world: World::new(IVec3::ONE, [[0; 4]; 256]), scope: ed.scope, size: IVec3::ZERO, cells: Vec::new(), blocks: Vec::new(), shape: None };
        shown.build(ed)?;
        Ok(shown)
    }

    fn build(&mut self, ed: &Editor) -> Result<()> {
        let listed = ed.level.blocks.len();
        self.world = match ed.scope {
            Scope::Level => crate::grid::build(&ed.level, &ed.blocks[..listed], 0, 0, &[])?,
            Scope::Block(_) | Scope::Frame(_) => {
                let block = ed.shape().expect("a block or frame shown");
                let mut world = World::new(block.size, [[0; 4]; 256]);
                for (k, &v) in block.voxels.iter().enumerate() {
                    if v != 0 {
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
        self.size = ed.level.size;
        self.cells = ed.level.cells.clone();
        self.blocks = ed.level.blocks.iter().zip(&ed.blocks).map(|(e, b)| (b.clone(), e.kind)).collect();
        Ok(())
    }

    /// Brings the world up to the editor. True: it was built anew (every
    /// chunk to mesh again); else the changed chunks are marked dirty.
    pub fn update(&mut self, ed: &Editor) -> Result<bool> {
        let listed = ed.level.blocks.len();
        let same_blocks = |shown: &Self| shown.blocks.len() == listed && shown.blocks.iter().zip(ed.level.blocks.iter().zip(&ed.blocks)).all(|((b, k), (e, m))| *k == e.kind && b == m);
        match ed.scope {
            Scope::Block(_) | Scope::Frame(_) | Scope::Figure => {
                if self.scope == ed.scope && self.shape.as_ref() == ed.shape() && (ed.scope != Scope::Figure || self.world.size == ed.grid()) {
                    return Ok(false);
                }
            }
            Scope::Level => {
                if self.scope == Scope::Level && self.size == ed.level.size && self.cells == ed.level.cells && same_blocks(self) {
                    return Ok(false);
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
                                        anyhow::bail!("the level has more than 255 colours");
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

/// The kinds of things as the level shows them (`c.things.NAME`): each
/// one's voxels from where it stands, as games at rest show it, a figure
/// (it walks or flies) seen from the side, anything else turned to the
/// camera; a kind without a model a pale column as wide and tall as its
/// body.
pub struct Figures(BTreeMap<String, Figure>);

struct Figure {
    /// Voxels and colours, from the spot it stands on.
    voxels: Vec<(IVec3, [u8; 3])>,
    /// The box around them, from that spot.
    lo: IVec3,
    hi: IVec3,
}

/// Behaviours whose things go about seen from the side.
const SIDEWAYS: [&str; 4] = ["hero", "walker", "hopper", "flyer"];
/// A kind without a model.
const MARKER: [u8; 3] = [150, 200, 240];

impl Figures {
    pub fn load(dir: &Path, cfg: &GameConfig) -> Self {
        let mut figures = BTreeMap::new();
        for (kind, table) in &cfg.things {
            let behaviour = table.get("behaviour").and_then(|b| b.as_str());
            let sideways = behaviour.is_some_and(|b| SIDEWAYS.contains(&b));
            let number = |key: &str| table.get(key).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
            let model = table.get("model").and_then(|m| m.as_str()).and_then(|file| crate::vox::load_model(&dir.join(file)).ok());
            let voxels: Vec<(IVec3, [u8; 3])> = match model {
                Some(m) => {
                    let pivot = IVec3::new(m.size.x / 2, m.size.y / 2, 0);
                    m.visible[0]
                        .iter()
                        .map(|&k| {
                            let (p, color) = m.frames[0][k as usize];
                            // Turned as the game turns it as a run starts
                            // (`face_of`), about the model's pivot: the hero
                            // to the right, the other figures to the left,
                            // anything else to the camera.
                            let l = p - pivot;
                            let l = match (sideways, behaviour) {
                                (true, Some("hero")) => IVec3::new(l.y, -l.x, l.z),
                                (true, _) => IVec3::new(-l.y, l.x, l.z),
                                _ => IVec3::new(-l.x, -l.y, l.z),
                            };
                            (l, [color[0], color[1], color[2]])
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
            let lo = voxels.iter().fold(IVec3::MAX, |a, v| a.min(v.0));
            let hi = voxels.iter().fold(IVec3::MIN, |a, v| a.max(v.0 + 1));
            let (lo, hi) = if voxels.is_empty() { (IVec3::ZERO, IVec3::ONE) } else { (lo, hi) };
            figures.insert(kind.clone(), Figure { voxels, lo, hi });
        }
        Self(figures)
    }

    /// The box around a kind's voxels, from the spot it stands on.
    pub fn bounds(&self, kind: &str) -> (IVec3, IVec3) {
        self.0.get(kind).map_or((IVec3::new(-2, -2, 0), IVec3::new(2, 2, 8)), |f| (f.lo, f.hi))
    }

    /// A kind standing at `at` as cubes: tinted halfway to `tint`, and
    /// see-through (opacity 0..255).
    fn cubes(&self, kind: &str, at: IVec3, tint: Option<[u8; 3]>, opacity: u8, out: &mut Vec<Instance>) {
        let Some(f) = self.0.get(kind) else { return };
        for &(l, c) in &f.voxels {
            let c = tint.map_or(c, |t| std::array::from_fn(|i| ((c[i] as u16 + t[i] as u16) / 2) as u8));
            let p = (at + l).as_vec3() + 0.5;
            out.push(Instance { pos_size: [p.x, p.y, p.z, 1.0], color: [c[0], c[1], c[2], 255], fx: [0, opacity, 0, 0], shade: [0; 4], turn: [0; 4] });
        }
    }

    /// A kind seen from the front, as a picture at most `fit` pixels each
    /// way (rows from the top): its width, height and pixels.
    pub fn picture(&self, kind: &str, fit: i32) -> (i32, i32, Vec<Option<[u8; 3]>>) {
        let Some(f) = self.0.get(kind) else { return (0, 0, Vec::new()) };
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

/// What a click on the panels (`panels.rs`) or a key does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Tool(Tool),
    Undo,
    Redo,
    /// From a block back to the level.
    Back,
    /// What the block shown or held is to the game.
    KindTo(Kind),
    /// In the level the held block turned, in a block the block itself.
    Turn(Turn),
    /// The block shown or held copied, or taken out of the level.
    Clone,
    Delete,
    /// The level's block (its place) held; twice: opened.
    Hold(usize),
    /// The tileset shown to draw a new block out of, or hidden again.
    Choose,
    Tile(i32),
    Empty,
    Colour([u8; 3]),
    /// The level's tools to its blocks or its things.
    Layer(Layer),
    /// A kind of thing (its place in the game's list) held to put down.
    HoldKind(usize),
    /// A model's next frame (1) or the one before (-1).
    Frame(i32),
    /// A copy of the model's frame after it; the frame taken out.
    CopyFrame,
    DeleteFrame,
    /// The model's box cut down to its voxels.
    Trim,
    /// The model's frames playing on the panel, or held.
    Play,
    /// BOX's edges cut back, or not; every tool mirrored left to right, or
    /// not.
    Bevel,
    Symmetry,
    /// FIGURE: a part chosen in the list to shape and rig (its place among
    /// the parts; chosen again, none: `Back` too), left out of the picture
    /// or shown again, every part shown again. The chosen part's parent:
    /// the next part clicked (CHOOSE), or as the rules find it; its joint
    /// moved half a voxel along an axis (0 x, 1 y, 2 z; the sign the way),
    /// or where the rules find it.
    Part(usize),
    Hide(usize),
    ShowAll,
    PickParent,
    ParentWorkedOut,
    Joint(u8, i8),
    JointWorkedOut,
    /// The held colour's material: one of its words (`frames::WORDS`) set.
    Material(u8, f32),
    /// A colour no voxel has taken out of the model's palette.
    Forget([u8; 3]),
}

/// The key an action answers to (the panels show it beside its word): a
/// tool the same on both levels. N is NEW in the level, DEEP PAINT in a
/// block; M is MIRROR among the level's blocks, MOVE among its things,
/// TRIM in a model (none is where another is).
pub fn key_of(action: Action) -> Option<char> {
    Some(match action {
        Action::Tool(Tool::Add) => 'A',
        Action::Tool(Tool::Remove) => 'E',
        Action::Tool(Tool::Paint) => 'P',
        Action::Tool(Tool::Deep) => 'N',
        Action::Tool(Tool::Move) => 'M',
        Action::Tool(Tool::Pick) => 'I',
        Action::Tool(Tool::Box) => 'Q',
        Action::Tool(Tool::Sphere) => 'S',
        Action::Bevel => 'F',
        Action::Symmetry => 'Y',
        Action::Layer(Layer::Blocks) => 'K',
        Action::Layer(Layer::Things) => 'H',
        Action::Undo => 'U',
        Action::Redo => 'R',
        Action::Back => 'B',
        Action::Turn(t) if t == Turn::TURN => 'T',
        Action::Turn(t) if t == Turn::ROLL => 'O',
        Action::Turn(t) if t == Turn::MIRROR => 'M',
        Action::Clone => 'C',
        Action::Delete => 'D',
        Action::Choose => 'N',
        Action::Frame(-1) => 'V',
        Action::Frame(_) => 'X',
        Action::CopyFrame => 'C',
        Action::DeleteFrame => 'D',
        Action::Trim => 'M',
        _ => return None,
    })
}

/// A block seen from the front: of each column the first voxel from the
/// front (rows from the top).
pub fn front(block: &Block) -> Vec<Option<[u8; 3]>> {
    let s = block.size;
    (0..s.x * s.z).map(|i| (0..s.y).find_map(|y| block.colour_at(IVec3::new(i % s.x, y, s.z - 1 - i / s.x)))).collect()
}

/// Does what a click on the panels or a key says.
pub fn act(ed: &mut Editor, view: &mut View, action: Action) {
    let scope = ed.scope;
    match action {
        Action::Tool(tool) => ed.choose(tool),
        Action::Undo => ed.undo(),
        Action::Redo => ed.redo(),
        Action::Back => ed.back_to_level(),
        Action::KindTo(kind) => ed.set_kind(kind),
        Action::Turn(turn) => match ed.scope {
            Scope::Level => ed.turn(turn),
            Scope::Block(_) | Scope::Frame(_) => ed.turn_block(turn),
            Scope::Figure => {}
        },
        Action::Clone => {
            if let Err(e) = ed.clone_block() {
                ed.message = Some(format!("{e:#}"));
            }
        }
        Action::Delete => ed.delete_block(),
        Action::Hold(i) => {
            if ed.held.block as usize == i + 1 {
                ed.open_block(i);
            } else {
                ed.hold(i);
            }
        }
        Action::Choose => view.choosing = !view.choosing,
        Action::Tile(id) => {
            if let Err(e) = ed.new_block(Some(id)) {
                ed.message = Some(format!("{e:#}"));
            }
            view.choosing = false;
        }
        Action::Empty => {
            if let Err(e) = ed.new_block(None) {
                ed.message = Some(format!("{e:#}"));
            }
            view.choosing = false;
        }
        Action::Colour(c) => ed.colour = c,
        Action::Layer(layer) => {
            ed.switch(layer);
            view.choosing = false;
            (view.hover, view.spot, view.under) = (None, None, None);
        }
        Action::HoldKind(i) => ed.hold_kind(i),
        Action::Frame(by) => ed.step_frame(by),
        Action::CopyFrame => ed.copy_frame(),
        Action::DeleteFrame => ed.delete_frame(),
        Action::Trim => ed.trim(),
        Action::Play => view.playing = !view.playing,
        Action::Bevel => ed.bevel = !ed.bevel,
        Action::Symmetry => ed.symmetry = !ed.symmetry,
        Action::Part(p) => ed.choose_part(p),
        Action::Hide(p) => ed.hide_part(p),
        Action::ShowAll => ed.show_all_parts(),
        Action::PickParent => ed.picking_parent = !ed.picking_parent,
        Action::ParentWorkedOut => ed.set_parent(None),
        Action::Joint(axis, way) => {
            let mut by = Vec3::ZERO;
            by[axis.min(2) as usize] = 0.5 * way.signum() as f32;
            ed.move_joint(by);
        }
        Action::JointWorkedOut => ed.set_joint(None),
        Action::Material(which, v) => {
            if let Some(m) = ed.material() {
                ed.set_material(super::frames::with_word(m, which as usize, v));
            }
        }
        Action::Forget(c) => ed.forget_colour(c),
    }
    // The figure made again at once (a part chosen, its rig set), and the
    // view kept on it.
    ed.follow_figure();
    if scope != ed.scope {
        view.hover = None;
    }
    view.follow_origin(ed);
}

/// A frame of the editor: what is shown in the game's light, the floor
/// under it, the things (in the level), where the tool would work (the
/// panels are egui's, `panels.rs`, drawn over it). Without the mouse's
/// marks it is the IDE's preview too.
pub fn frame(ed: &Editor, view: &View, shown: &Shown, figures: &Figures, light: &LightConfig, aspect: f32) -> FrameData {
    let n = ed.level.n;
    let (orbit, unit) = match ed.scope {
        Scope::Level => (&view.level, n),
        Scope::Block(_) | Scope::Frame(_) | Scope::Figure => (&view.block, 1),
    };
    let grid = ed.grid();
    let view_proj = view.view_proj(orbit, aspect);
    let eye = crate::camera::eye(&orbit.camera(), orbit.target, aspect);
    let sun = Vec3::from(light.sun).normalize_or(Vec3::NEG_Z);
    // A figure's part is shaped in the figure: the room is the figure's
    // (with the part's box), the floor under it all.
    let (room_lo, room_hi) = match ed.figure_box() {
        Some((lo, size)) => (lo.min(IVec3::ZERO), (lo + size).max(shown.world.size)),
        None => (IVec3::ZERO, shown.world.size),
    };
    let (light_vp, depth_range) = crate::camera::light_around(sun, room_lo.as_vec3() * unit as f32 - Vec3::new(0.0, 0.0, unit as f32), room_hi.as_vec3().max(Vec3::ONE) * unit as f32);

    // The floor: a cell deep under the grid, in two shades (the editor's;
    // not in the IDE's pictures).
    let mut instances = Vec::new();
    let u = unit as f32;
    let (floor_lo, floor_size) = if ed.figure_box().is_some() { (room_lo, room_hi - room_lo) } else { (IVec3::ZERO, grid) };
    for y in floor_lo.y..floor_lo.y + floor_size.y * view.floor as i32 {
        for x in floor_lo.x..floor_lo.x + floor_size.x {
            let shade = if (x + y).rem_euclid(2) == 0 { [58, 56, 66, 255] } else { [66, 64, 74, 255] };
            instances.push(Instance { pos_size: [(x as f32 + 0.5) * u, (y as f32 + 0.5) * u, (floor_lo.z as f32 - 0.5) * u, u], color: shade, fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] });
        }
    }
    let things = ed.scope == Scope::Level && ed.layer == Layer::Things;
    if ed.scope == Scope::Level {
        // The thing the tool would work on tinted by what it would do.
        let marked = if things { ed.moving().or(view.under) } else { None };
        let tint = match ed.tool {
            Tool::Remove => [230, 70, 60],
            Tool::Move => [255, 214, 90],
            _ => [255, 255, 255],
        };
        for (i, t) in ed.level.things.iter().enumerate() {
            let tint = (Some(i) == marked && ed.tool != Tool::Add).then_some(tint);
            figures.cubes(&t.kind, IVec3::from(t.at), tint, 255, &mut instances);
        }
    }
    // Where the tool would work: a see-through cube there, or the held kind
    // of thing see-through where it would stand.
    let mut see_through = 0;
    let far = |i: &Instance| -(Vec3::from_slice(&i.pos_size[..3]) - eye).length_squared();
    // A figure: its parts as cubes round the one chosen (that one is the
    // world shown, being shaped), see-through while one is chosen, flush
    // ones one body; drawn far to near with the tool's marks after the
    // opaque ones.
    let mut through = Vec::new();
    if let (Some(figure), Some((lo, _))) = (&ed.figure, ed.figure_box()) {
        through = figure.cubes(lo, &mut instances);
        crate::render::join_see_through(&mut through, eye - grid.as_vec3() / 2.0);
    }
    if things {
        if let (Tool::Add, Some(at), Some(kind)) = (ed.tool, view.spot, &ed.held_kind) {
            let start = instances.len();
            figures.cubes(kind, at, None, 120, &mut instances);
            instances[start..].sort_by(|a, b| far(a).total_cmp(&far(b)));
            see_through = instances.len() - start;
        }
    } else if ed.forms() {
        // The shape BOX or SPHERE would make, see-through: its surface,
        // in the colour held (red to cut), far to near.
        let cells = ed.form_cells_now(view.hover, view.cut);
        let all: crate::world::CellSet = cells.iter().copied().collect();
        let color = if view.cut || ed.drawing.is_some_and(|d| d.cut) { [230, 70, 60, 255] } else { [ed.colour[0], ed.colour[1], ed.colour[2], 255] };
        let start = instances.len();
        for c in cells.iter().filter(|c| crate::mesh::DIRS.iter().any(|d| !all.contains(&(**c + *d)))) {
            let p = (c.as_vec3() + 0.5) * u;
            instances.push(Instance { pos_size: [p.x, p.y, p.z, u * 1.02], color, fx: [0, 110, 0, 0], shade: [0; 4], turn: [0; 4] });
        }
        instances[start..].sort_by(|a, b| far(a).total_cmp(&far(b)));
        see_through = instances.len() - start;
    } else if let Some(c) = view.hover.and_then(|hit| ed.target(hit)) {
        let color = match ed.tool {
            Tool::Remove => [230, 70, 60, 255],
            Tool::Pick => [255, 255, 255, 255],
            _ => match ed.scope {
                Scope::Level => [255, 214, 90, 255],
                Scope::Block(_) | Scope::Frame(_) | Scope::Figure => [ed.colour[0], ed.colour[1], ed.colour[2], 255],
            },
        };
        let filled = match ed.scope {
            Scope::Level => ed.level.cell(c).block != 0,
            Scope::Block(_) | Scope::Frame(_) | Scope::Figure => ed.shape().is_some_and(|b| b.get(c) != 0),
        };
        let size = if filled { u * 1.04 } else { u };
        let p = (c.as_vec3() + 0.5) * u;
        instances.push(Instance { pos_size: [p.x, p.y, p.z, size], color, fx: [0, 110, 0, 0], shade: [0; 4], turn: [0; 4] });
        see_through = 1;
    }
    if !through.is_empty() {
        through.extend(instances.drain(instances.len() - see_through..));
        through.sort_by(|a, b| far(a).total_cmp(&far(b)));
        see_through = through.len();
        instances.extend(through);
    }

    FrameData {
        view_proj,
        light_vp,
        remember: false,
        sun_dir: sun,
        sun_color: light.sun_color,
        depth_bias: 0.3 / depth_range,
        ambient: light.ambient,
        shadow: light.shadow,
        shadow_style: match light.shadow_style {
            ShadowStyle::Smooth => 0.0,
            ShadowStyle::Voxel => 1.0,
            ShadowStyle::VoxelSoft => 2.0,
        },
        sky: light.sky,
        reflection: light.reflection,
        reflection_horizon: light.reflection_horizon,
        metal_sweep: 0.0,
        time: 0.0,
        eye,
        haze: 0.0,
        haze_color: [0.0; 3],
        haze_start: 1.0,
        haze_depth: 1.0,
        lights: Vec::new(),
        glow: 0.0,
        shadow_map: SHADOW_MAP,
        tilt_shift: 0.0,
        screen_flash: 0.0,
        outline: [0.0; 4],
        fade: [0.0; 4],
        models: Vec::new(),
        ui_models: Vec::new(),
        bank: Arc::default(),
        instances,
        see_through,
        ui: Vec::new(),
        ui_view_proj: Mat4::IDENTITY,
    }
}

/// A joint of a figure as FIGURE marks it over the 3D view: where (shares
/// of the screen, from the top left), where its parent's joint is (the
/// chosen part's only), and whether it is the chosen part's or has a
/// warning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointMark {
    pub part: usize,
    pub at: glam::Vec2,
    pub parent: Option<glam::Vec2>,
    pub chosen: bool,
    pub warned: bool,
}

/// FIGURE's joints on the screen (`aspect`, the panels taking `inset`):
/// one for each part shown that lies before the eye.
pub fn joint_marks(ed: &Editor, view: &View, aspect: f32, inset: glam::Vec2) -> Vec<JointMark> {
    let (Some(figure), Some((lo, _))) = (&ed.figure, ed.figure_box()) else { return Vec::new() };
    let shift = lo.as_vec3();
    let view_proj = Mat4::from_translation(Vec3::new(-inset.x, inset.y, 0.0)) * view.block.view_proj(aspect);
    let on_screen = |p: Vec3| {
        let c = view_proj * (p + shift).extend(1.0);
        (c.w > 1e-4).then(|| glam::Vec2::new((c.x / c.w + 1.0) / 2.0, (1.0 - c.y / c.w) / 2.0))
    };
    let parts = figure.parts();
    (0..parts.len())
        .filter(|&p| !figure.hidden[p])
        .filter_map(|p| {
            let chosen = figure.selected == Some(p);
            let parent = parts[p].parent.filter(|_| chosen).and_then(|q| on_screen(figure.joint(q)));
            Some(JointMark { part: p, at: on_screen(figure.joint(p))?, parent, chosen, warned: !figure.warnings(p).is_empty() })
        })
        .collect()
}

/// The editor's shadow map: the size the game's "high" setting has.
pub const SHADOW_MAP: u32 = 4096;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::tests::folder;

    /// The ray through the middle of the screen passes through the point
    /// the camera looks at, from any side.
    #[test]
    fn a_ray_through_the_middle_of_the_screen_meets_the_target() {
        for (yaw, pitch) in [(0.0, 15.0), (200.0, 20.0), (30.0, -50.0), (90.0, 85.0)] {
            let orbit = Orbit { yaw, pitch, zoom: 70.0, target: Vec3::new(8.0, 3.0, 5.0) };
            let (origin, dir) = mouse_ray(orbit.view_proj(16.0 / 9.0), 0.5, 0.5);
            let off = (orbit.target - origin).reject_from(dir).length();
            assert!(off < 0.01, "{yaw}, {pitch}: misses by {off}");
        }
    }

    /// A ray meets a box where it enters it, from inside at once, beside
    /// it never.
    #[test]
    fn a_ray_enters_a_box() {
        let (lo, hi) = (Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 2.0));
        assert_eq!(ray_box(Vec3::new(0.0, 0.0, 10.0), Vec3::NEG_Z, lo, hi), Some(8.0));
        assert_eq!(ray_box(Vec3::new(0.0, 0.0, 1.0), Vec3::NEG_Z, lo, hi), Some(0.0));
        assert_eq!(ray_box(Vec3::new(5.0, 0.0, 10.0), Vec3::NEG_Z, lo, hi), None);
        assert_eq!(ray_box(Vec3::new(0.0, 0.0, 10.0), Vec3::Z, lo, hi), None, "behind the ray");
    }

    /// The mouse over a thing points at it (its body, a kind without a
    /// model as wide and tall as the kind says), else at the spot a new
    /// one would stand on.
    #[test]
    fn the_mouse_finds_things_and_spots() {
        let dir = folder("along");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        ed.level.things.push(crate::grid::Thing { kind: "gem".into(), at: [8, 2, 0] });
        let mut cfg = GameConfig::default();
        cfg.things.insert("gem".into(), toml::toml! { width = 2
        height = 4 });
        let figures = Figures::load(&dir, &cfg);
        assert_eq!(figures.bounds("gem"), (IVec3::new(-1, -1, 0), IVec3::new(1, 1, 4)));
        let shown = Shown::new(&ed).unwrap();
        let down = |x: f32, y: f32| things_along(&ed, &shown.world, &figures, Vec3::new(x, y, 50.0), Vec3::NEG_Z);
        assert_eq!(down(8.5, 2.5), (Some(IVec3::new(8, 2, 0)), Some(0)), "over the gem, on the floor");
        assert_eq!(down(2.5, 1.5), (Some(IVec3::new(2, 2, 4)), None), "on the block's top");
        assert_eq!(down(8.5, 6.5), (Some(IVec3::new(8, 6, 0)), None), "the floor of the back row");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A model is seen as the game shows it as a run starts: the hero
    /// looking right, other figures left, anything else turned to the
    /// camera, a block as it lies; the picture of a frame from there has
    /// the nearest voxels.
    #[test]
    fn a_model_is_seen_as_the_game_shows_it() {
        let mut cfg = GameConfig::default();
        cfg.things.insert("fox".into(), toml::toml! { behaviour = "hero"
        model = "fox.vox" });
        cfg.things.insert("sign".into(), toml::toml! { behaviour = "prop"
        model = "sign.vox" });
        cfg.things.insert("bat".into(), toml::toml! { behaviour = "flyer"
        model = "bat.vox" });
        let faces = ["fox.vox", "bat.vox", "sign.vox", "other.vox", "blocks/27.vox"].map(|f| face_of(f, &cfg));
        assert_eq!(faces, [Face::Right, Face::Left, Face::Front, Face::Front, Face::Block]);
        // 2 across, 3 deep, 1 high: red at the back left, blue at the
        // front right.
        let mut b = Block::boxed(IVec3::new(2, 3, 1));
        b.set(IVec3::new(0, 0, 0), Some([255, 0, 0])).unwrap();
        b.set(IVec3::new(1, 2, 0), Some([0, 0, 255])).unwrap();
        let (red, blue) = (Some([255, 0, 0]), Some([0, 0, 255]));
        assert_eq!(seen(&b, Face::Front), (2, 1, vec![blue, red]), "turned to the camera: its +x on the left");
        assert_eq!(seen(&b, Face::Right), (3, 1, vec![red, None, blue]), "looking right: its front on the right");
        assert_eq!(seen(&b, Face::Left), (3, 1, vec![blue, None, red]), "looking left: its front on the left");
        assert_eq!(seen(&b, Face::Block), (2, 1, vec![red, blue]), "a block from the level's front");
    }
}
