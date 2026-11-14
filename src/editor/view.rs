//! The editor's view: the level, the open block, the model or the figure
//! in the engine's own light and shadows, seen from any side (an orbit
//! around what is shown). `View` holds the cameras, the pointer's state
//! and what the panels have open; beside it what the mouse aims at
//! (`aim.rs`), the world shown (`shown.rs`), how things and pieces look
//! (`figures.rs`), what every click and key does (`actions.rs`), the
//! picture (`picture.rs`) and the marks among the pieces (`marks.rs`).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use glam::{IVec2, IVec3, Mat4, Quat, Vec3, Vec4};

use super::model::room::{self, Hanging, Item};
use super::model::{Editor, Hit, Layer, Scope, Tool};
use crate::blocks::{Block, Turn};
use crate::config::{CameraConfig, GameConfig, LightConfig, ShadowStyle};
use crate::grid::{Cell, Kind};
use crate::render::{FrameData, Instance};
use crate::world::World;

#[path = "aim.rs"]
mod aim;
pub use aim::*;
#[path = "shown.rs"]
mod shown;
pub use shown::*;
#[path = "figures.rs"]
mod figures;
pub use figures::*;
#[path = "actions.rs"]
mod actions;
pub use actions::*;
#[path = "picture.rs"]
mod picture;
pub use picture::*;
#[path = "marks.rs"]
mod marks;
pub use marks::*;

/// How the camera looks at what is shown: turned about it (degrees), how
/// wide the view is there (voxels), where it looks, and how wide it opens
/// (the field of view, degrees: the editors' 40, or the game's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub target: Vec3,
    pub fov: f32,
}

/// FRONT, SIDE and TOP in the voxel editor's corner: the yaw and pitch
/// each looks from, as Blender's front, right and top views: at a model's
/// front (its +y, `Face`) from the north, at its right side (+x) from the
/// east, straight down (+y up the screen). A level's block's front is its
/// y 0 side, the level's (`Face::Block`; the logo's alike): from the south.
pub const STRAIGHT: [(&str, f32, f32); 3] = [("FRONT", 180.0, 0.0), ("SIDE", 270.0, 0.0), ("TOP", 0.0, 90.0)];

/// `STRAIGHT`'s yaw and pitch for a model shown as `face` says.
pub fn straight_of(face: Face, k: usize) -> Option<(f32, f32)> {
    let &(_, yaw, pitch) = STRAIGHT.get(k)?;
    Some(if k == 0 && matches!(face, Face::Block | Face::Logo) { (0.0, pitch) } else { (yaw, pitch) })
}

/// The editors' field of view.
pub const FOV: f32 = 40.0;

impl Orbit {
    pub fn camera(&self) -> CameraConfig {
        CameraConfig { yaw: self.yaw, pitch: self.pitch, fov: self.fov, view_width: self.zoom, ..CameraConfig::default() }
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

/// The first look at a level of blocks `size` voxels big (its start at
/// `x`), from the front at the game's tilt, in the room the panels leave
/// (as a 1600 by 900 window has it): the whole level, a little room
/// round it; or, for a level far wider than it is high (a long
/// side-scroller, which shown whole would be a strip), all of its height,
/// from its start (or its left end) on. GAME VIEW is the game's camera.
pub fn frame_level(size: Vec3, x: f32, pitch: f32) -> Orbit {
    use super::zones::{INSPECTOR, OUTLINER, TOP};
    let (w, h) = (1600.0, 900.0);
    let (room_w, room_h) = (w - OUTLINER - INSPECTOR, h - TOP);
    // How wide the view must be to show the level's width, and its height
    // (the view is as wide as the window; the room a share of it).
    // (The near edge looks wider at a tilt: more room across.)
    let for_width = size.x * 1.25 * w / room_w;
    // (Seen at the tilt: its height and some of its depth up the screen.)
    let p = pitch.to_radians();
    let tall = size.z * p.cos().abs() + size.y * p.sin().abs();
    let for_height = tall * 1.15 * (room_w / room_h) * w / room_w;
    let zoom = if for_width <= for_height * 2.5 { for_width.max(for_height) } else { for_height };
    let seen = zoom * room_w / w;
    let middle_x = if zoom >= for_width { size.x / 2.0 } else { x.max(seen / 2.0 - size.x * 0.02).min(size.x - seen / 2.0) };
    Orbit { yaw: 0.0, pitch, zoom: zoom.max(16.0), target: Vec3::new(middle_x, size.y / 2.0, size.z / 2.0), fov: FOV }
}

/// Where a run of the game begins in a level, as far as the editor knows
/// it without running the game's code: the level's place `start`, else
/// the platformer's hero where the level sets one, else `c.player.spawn`
/// on the ground (`lua::world::start`'s order), and how tall the player's
/// figure is there (the camera looks at its middle as a run starts).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameStart {
    /// Where the level's voxel 0, 0, 0 lies in the game's world (a level
    /// of blocks' `margin` and `below`).
    pub origin: IVec3,
    pub spawn: [f32; 2],
    /// The kinds the ready-made rules stand the player as (behavior
    /// "hero"), and how tall the player's figure is: a hero as its kind
    /// says, the player's own kind as tall as its model (a Lua game's
    /// figures are, `LuaGame::kinds`).
    pub heroes: Vec<String>,
    pub heights: BTreeMap<String, f32>,
    /// The player's own kind, if the game names one (`c.player.kind`).
    pub player: Option<String>,
}

impl GameStart {
    pub fn of(cfg: &GameConfig, dir: &Path) -> Self {
        let number = |v: &toml::Value| v.as_float().or(v.as_integer().map(|i| i as f64)).map(|f| f as f32);
        let heroes: Vec<String> = cfg.things.iter().filter(|(_, t)| t.get("behavior").and_then(|b| b.as_str()) == Some("hero")).map(|(k, _)| k.clone()).collect();
        let player = cfg.player.kind.as_ref().map(|k| k.of(1).to_string()).filter(|k| !k.is_empty());
        let mut heights: BTreeMap<String, f32> = heroes.iter().filter_map(|k| Some((k.clone(), cfg.things.get(k)?.get("height").and_then(number)?))).collect();
        if let Some(k) = player.as_ref().filter(|k| !heights.contains_key(*k)) {
            let model = cfg.things.get(k).and_then(|t| t.get("model")).and_then(|m| m.as_str()).and_then(|m| crate::vox::load_model(&dir.join(m)).ok());
            heights.insert(k.clone(), model.map_or(1.0, |m| m.size.z.max(1) as f32));
        }
        let origin = cfg.world.blocks.as_ref().map_or(IVec3::ZERO, |b| IVec3::new(b.margin, 0, b.below));
        Self { origin, spawn: cfg.player.spawn, heroes, heights, player }
    }

    /// The point the game's camera looks at as a run starts in the level
    /// `ed` holds (its voxels): the player's feet where the run begins,
    /// lifted by half its height, in whole voxels when the camera `snap`s.
    pub fn target(&self, ed: &Editor, snap: bool) -> Vec3 {
        let level = &ed.level;
        let size = level.voxels();
        let hero = level.things.iter().find(|t| self.heroes.contains(&t.kind));
        let (feet, kind) = if let Some(p) = level.places.iter().find(|p| p.name == "start") {
            (IVec3::from(p.at).as_vec3() + Vec3::new(0.5, 0.5, 0.0), self.player.clone().or(hero.map(|t| t.kind.clone())))
        } else if let Some(t) = hero {
            (IVec3::from(t.at).as_vec3() + Vec3::new(0.5, 0.5, 0.0), Some(t.kind.clone()))
        } else {
            // `c.player.spawn` (a negative one the middle) on the ground
            // there: the top of the level's blocks in that column.
            let at = |v: f32, origin: i32, whole: i32| if v < 0.0 { whole as f32 * 0.5 } else { v - origin as f32 };
            let (x, y) = (at(self.spawn[0], self.origin.x, size.x), at(self.spawn[1], self.origin.y, size.y));
            let n = level.n.max(1);
            let cell = IVec3::new((x as i32).div_euclid(n), (y as i32).div_euclid(n), 0);
            let top = if level.has_grid() { (0..level.size.z).rev().find(|&z| level.cell(IVec3::new(cell.x, cell.y, z)).block != 0).map_or(0, |z| (z + 1) * n) } else { 0 };
            (Vec3::new(x, y, top as f32), self.player.clone())
        };
        let height = kind.and_then(|k| self.heights.get(&k).copied()).unwrap_or(0.0);
        // (Drawn feet on whole voxels, as `Session::view_target` snaps them.)
        if snap { (feet.floor() + Vec3::Z * (height * 0.5)).round() } else { feet + Vec3::Z * (height * 0.5) }
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
    /// The pieces only casting a shadow (`shadow_only`) show as ghosts,
    /// to choose and move; off (GAME VIEW, the IDE's picture) they only
    /// cast, as in the game.
    pub ghosts: bool,
    /// Ctrl is down: BOX and SPHERE cut away (their shape shown red, in
    /// the voxels).
    pub cut: bool,
    /// Alt is down: BOX and SPHERE fill only where it is empty.
    pub only_empty: bool,
    /// Where voxel 0, 0, 0 of the grid shown stood in a figure's scene
    /// when last drawn (`Editor::origin`): the view goes with the figure
    /// when another part is chosen or a part's box grows on its low side.
    pub origin: Option<IVec3>,
    /// Among the pieces: the game's camera (GAME VIEW looks as it does),
    /// the window's shape, the models listed to choose one to set down
    /// (ADD MODEL), the lights shining in the view or not (the view only),
    /// and the item the mouse is over.
    pub game: CameraConfig,
    /// Where the game's run begins in a level, as far as the editor can
    /// know it without the game's code (GAME VIEW looks from there).
    pub start: GameStart,
    pub aspect: f32,
    pub picker: bool,
    pub lights: bool,
    pub over: Option<Item>,
    /// The item whose name is being typed in the list, and why the last
    /// name typed was refused.
    pub renaming: Option<Item>,
    pub rename_problem: Option<String>,
    /// A figure's part whose name is being typed in its row.
    pub renaming_part: Option<usize>,
    /// Among the pieces: the handle under the mouse (lit), the menu open
    /// (a right click's, or ADD's on the top bar), the camera before GAME
    /// VIEW (to go back to; None: GAME VIEW is off), the level's own
    /// settings shown in the inspector (LEVEL), the spot a model picked
    /// from ADD MODEL goes to (a right click's on the floor), where the
    /// right button went down (a click without dragging opens the menu).
    pub grip_hover: Option<super::handles::Grip>,
    /// ROTATE was pressed for this item (its rings shown, not its arrows);
    /// any other choice is in MOVE.
    pub turning: Option<Item>,
    /// In a figure, the chosen part's joint's ring under the mouse (lit).
    pub ring_hover: bool,
    /// SLICE's arrow under the mouse (lit).
    pub slice_hover: bool,
    /// The chosen part's place at rest (`Editor::rest_view`), which the
    /// view looks through; ROTATE (its rings) rather than MOVE at it.
    pub rest: Mat4,
    pub part_rotate: bool,
    pub menu: Option<Menu>,
    pub before_game: Option<Orbit>,
    /// The game's outline round its models (`c.effects.outline`: rgb, how
    /// many voxels thick), for a figure's GAME VIEW.
    pub outline: [f32; 4],
    /// BACKGROUND: the level editor's and the voxel editor's (None: the
    /// game's sky, as always), and whether one was chosen since the window
    /// last kept them (`background::Backgrounds`).
    pub backgrounds: [Option<[u8; 3]>; 2],
    pub backgrounds_changed: bool,
    /// The game's sky (what BACKGROUND's first choice shows).
    pub sky: [u8; 3],
    pub level_settings: bool,
    pub picker_spot: Option<IVec3>,
    pub right_press: Option<(f32, f32)>,
    /// Where the left button went down, until the pointer has gone
    /// `handles::TRAVEL` from it (None: no press, or a drag now), and
    /// whether a program's pointer pressed (`ide::remote`).
    pub press_at: Option<(f32, f32)>,
    pub synthetic_press: bool,
    /// The buttons that went down in the view and are not let go since
    /// (`HELD_LEFT`, `HELD_RIGHT`, `HELD_MIDDLE`): a motion drags or turns
    /// only with these, whatever button state it carries.
    pub held: u8,
    /// Since when the left button is up while a drag is still open (its
    /// let-go never came: `EditorWindow::draw` ends it).
    pub up_since: Option<std::time::Instant>,
    /// SELECT's rectangle being dragged on the screen (shares of the
    /// window, from the press to the pointer) and what it will do.
    pub marquee: Option<Marquee>,
    /// F1's page of the keys shown over the view (`keys.rs`).
    pub keys_shown: bool,
    /// How much of the window the panels leave the view: shares of it on
    /// the left, at the top, on the right and at the bottom.
    pub room: [f32; 4],
    /// GRID: faint lines along the voxels' edges on the faces seen, in the
    /// voxel editor (`cell_lines.rs`); the lines worked out last, kept
    /// while neither the view nor the voxels change.
    pub grid: bool,
    /// FRONT, SIDE or TOP pressed in the voxel editor: the free view's yaw
    /// and pitch before it, to go back to (`View::straight_view`).
    pub free: Option<(f32, f32)>,
    pub grid_lines: std::cell::RefCell<super::cell_lines::Kept>,
}

/// SELECT's rectangle on the screen: where it began and where the pointer
/// is (shares of the window), and its mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marquee {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub mode: super::model::selection::SelectMode,
}

/// A menu over the view: on what (a thing, empty floor at a spot, or the
/// top bar's ADD) and where its corner is (egui's points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Menu {
    pub on: MenuOn,
    pub at: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuOn {
    Item(Item),
    Floor(IVec3),
    Add,
    /// The voxel editor's: a voxel (its cell in the grid shown, and the
    /// figure's part there), a frame, a part, a level's block.
    Voxel(IVec3, Option<u16>),
    Frame(usize),
    Part(usize),
    Block(usize),
    /// MERGE WITH…: the parts a part may be merged into.
    MergeWith(usize),
    /// The selection's (SELECT), and its MOVE TO PART…: the parts.
    Selection,
    ToPart,
}

/// Cubes placed in a part's own grid moved to where the part stands
/// (`rest`: its grid to the figure's), turned with it; placed from one
/// cube exactly by the turn the shader draws them with, so neighbours
/// meet corner to corner.
fn place_turned(cubes: &mut [Instance], rest: Mat4) {
    let (_, turn, _) = rest.to_scale_rotation_translation();
    let (snorm, drawn) = (crate::render::snorm_turn(turn), crate::render::turn_as_drawn(turn));
    let Some(anchor) = cubes.first().map(|c| Vec3::from_slice(&c.pos_size[..3])) else { return };
    let to = rest.transform_point3(anchor);
    for c in cubes {
        let at = Vec3::from_slice(&c.pos_size[..3]);
        let p = to + drawn * (at - anchor);
        (c.pos_size[0], c.pos_size[1], c.pos_size[2], c.turn) = (p.x, p.y, p.z, snorm);
    }
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
    /// The title's logo (`c.title_screen.logo_model`): the game lays its
    /// x across and its y towards the player (`ui::logo_model_parts`),
    /// which a view of the world shows the right way round, to read it,
    /// only from its y 0 side (from +y its letters read mirrored).
    Logo,
}

impl Face {
    /// The way the game has the model look (`session::model_drawn`'s
    /// `look`; its forward is +y): a figure to its side, anything else
    /// turned to the camera; a block as it lies and the title's logo as
    /// the title lays it, unturned.
    pub fn look(self) -> IVec2 {
        match self {
            Face::Right => IVec2::X,
            Face::Left => IVec2::NEG_X,
            Face::Front => IVec2::NEG_Y,
            Face::Block | Face::Logo => IVec2::Y,
        }
    }
}

/// The side the game shows of the model `file` (its path in the game
/// folder) as a run starts: of a block its front as it lies; of a figure
/// (a kind with it) the side it looks to (`start_way`); of the title's
/// logo the side that reads the right way round; else its front.
pub fn face_of(file: &str, cfg: &GameConfig) -> Face {
    if file.starts_with(&format!("{}/", crate::blocks::FOLDER)) {
        return Face::Block;
    }
    if cfg.title_screen.logo_model.as_deref() == Some(file) {
        return Face::Logo;
    }
    let kind = cfg.things.values().find(|t| t.get("model").and_then(|m| m.as_str()) == Some(file));
    match kind.map(start_way) {
        Some(crate::grid::Way::East) => Face::Right,
        Some(crate::grid::Way::West) => Face::Left,
        // (Facing away there is no side for: its front.)
        _ => Face::Front,
    }
}

/// The way a kind of thing (`c.things.NAME`) faces as a run starts, as
/// the game turns it: the way its `facing` says where it sets one (a bat
/// to the front, wings spread, whether or not it turns to the hero later),
/// else as its behavior goes: a figure (it walks or flies) seen from the
/// side, the hero right, the others left; anything else to the camera.
fn start_way(kind: &toml::Table) -> crate::grid::Way {
    use crate::grid::Way;
    let number = |v: &toml::Value| v.as_float().or(v.as_integer().map(|i| i as f64));
    let facing = kind.get("facing").and_then(|f| f.as_array()).and_then(|f| Some((number(f.first()?)?, number(f.get(1)?)?)));
    if let Some((x, y)) = facing
        && (x, y) != (0.0, 0.0)
    {
        // As the game turns a body to it (`sim::quarter_turn`, from facing
        // the camera).
        let look = crate::sim::quarter_turn(Vec3::new(x as f32, y as f32, 0.0), glam::IVec2::NEG_Y);
        return Way::of_pair(look.to_array()).unwrap_or(Way::South);
    }
    match kind.get("behavior").and_then(|b| b.as_str()) {
        Some("hero") => Way::East,
        Some(b) if SIDEWAYS.contains(&b) => Way::West,
        _ => Way::South,
    }
}

/// A model's frame seen from `face` as the game shows it, the nearest
/// voxel of each column: its width, height and pixels (rows from the
/// top). A figure's front is on the side it looks to.
pub fn seen(frame: &Block, face: Face) -> (i32, i32, Vec<Option<[u8; 3]>>) {
    let s = frame.size;
    let w = match face {
        Face::Right | Face::Left => s.y,
        Face::Front | Face::Block | Face::Logo => s.x,
    };
    let pixels = (0..w * s.z)
        .map(|i| {
            let (c, z) = (i % w, s.z - 1 - i / w);
            // The voxels along the line of sight, the nearest first.
            match face {
                Face::Right => (0..s.x).rev().find_map(|x| frame.colour_at(IVec3::new(x, c, z))),
                Face::Left => (0..s.x).find_map(|x| frame.colour_at(IVec3::new(x, s.y - 1 - c, z))),
                Face::Front => (0..s.y).rev().find_map(|y| frame.colour_at(IVec3::new(s.x - 1 - c, y, z))),
                Face::Block | Face::Logo => (0..s.y).find_map(|y| frame.colour_at(IVec3::new(c, y, z))),
            }
        })
        .collect();
    (w, s.z, pixels)
}

impl View {
    /// A first look: the level from the front at the game's tilt, whole
    /// (`frame_level`); a block from the front left.
    pub fn new(ed: &Editor, cfg: &GameConfig) -> Self {
        let n = ed.level.n.max(1) as f32;
        let size = ed.level.size.as_vec3() * n;
        let margin = cfg.world.blocks.as_ref().map_or(0, |b| b.margin) as f32;
        let x = (cfg.player.spawn[0] - margin).clamp(0.0, size.x);
        let column = (x / n) as i32;
        let top = (0..ed.level.size.z).rev().find(|&z| (0..ed.level.size.y).any(|y| ed.level.cell(IVec3::new(column, y, z)).block != 0));
        let z = top.map_or(size.z / 2.0, |t| (t + 1) as f32 * n);
        let level = if ed.level.has_grid() { frame_level(size, x, cfg.camera.pitch) } else { Orbit { yaw: 0.0, pitch: cfg.camera.pitch, zoom: cfg.camera.view_width, target: Vec3::new(x, n / 2.0, z), fov: FOV } };
        let face = face_of(&ed.file, cfg);
        let block = match ed.scope {
            // A model as the game shows it as a run starts (`face`), a
            // little turned so that some of its front shows, all of it in
            // view.
            Scope::Frame(_) | Scope::Figure => {
                // A figure framed by its voxels, not by its stated box (a
                // box left far too big by a part that flew off and came
                // back would show the figure as a speck).
                let (lo, size) = match (ed.scope, &ed.figure) {
                    (Scope::Figure, Some(f)) => match f.model.frames[0].iter().map(|v| v.0).fold(None, |b: Option<(IVec3, IVec3)>, c| Some(b.map_or((c, c), |(lo, hi)| (lo.min(c), hi.max(c))))) {
                        Some((lo, hi)) => (lo.as_vec3(), (hi - lo + 1).as_vec3()),
                        None => (Vec3::ZERO, ed.grid().as_vec3()),
                    },
                    _ => (Vec3::ZERO, ed.grid().as_vec3()),
                };
                let yaw = match face {
                    Face::Right => 250.0,
                    Face::Left => 110.0,
                    Face::Front => 160.0,
                    Face::Block => -30.0,
                    Face::Logo => -20.0,
                };
                Orbit { yaw, pitch: 20.0, zoom: (size.max_element() * 2.8).max(16.0), target: lo + size / 2.0, fov: FOV }
            }
            _ => Orbit { yaw: -30.0, pitch: 25.0, zoom: n * 4.5, target: Vec3::splat(n / 2.0), fov: FOV },
        };
        let mut view = Self {
            level,
            block,
            hover: None,
            spot: None,
            under: None,
            choosing: false,
            inset: glam::Vec2::ZERO,
            face,
            playing: true,
            floor: true,
            ghosts: true,
            cut: false,
            only_empty: false,
            origin: ed.origin(),
            game: cfg.camera.clone(),
            start: GameStart::of(cfg, &ed.dir),
            aspect: 16.0 / 9.0,
            picker: false,
            lights: true,
            over: None,
            renaming: None,
            rename_problem: None,
            renaming_part: None,
            grip_hover: None,
            turning: None,
            ring_hover: false,
            slice_hover: false,
            rest: Mat4::IDENTITY,
            part_rotate: false,
            menu: None,
            before_game: None,
            outline: {
                let [r, g, b] = cfg.effects.outline_color;
                [r, g, b, cfg.effects.outline.max(0.0)]
            },
            backgrounds: [None; 2],
            backgrounds_changed: false,
            sky: cfg.light.sky.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
            level_settings: false,
            picker_spot: None,
            right_press: None,
            press_at: None,
            synthetic_press: false,
            held: 0,
            up_since: None,
            marquee: None,
            keys_shown: false,
            room: [0.0; 4],
            grid: true,
            free: None,
            grid_lines: Default::default(),
        };
        // A level without blocks: the whole room as the game looks.
        if !ed.level.has_grid() {
            view.game_view(ed);
            let room = ed.level.voxels().as_vec3();
            view.level.zoom = view.level.zoom.max(room.x * 1.6).max(room.y * 2.0);
            // Its walls up to the top in view too.
            view.level.target.z = room.z * 0.3;
        }
        view
    }

    /// Whether the chosen one shows its rings (ROTATE), else its arrows.
    pub fn turn_mode(&self, ed: &Editor) -> bool {
        self.turning.is_some() && self.turning == ed.pieces.chosen && ed.can_turn()
    }

    /// The room the panels leave the view (shares of the window: left,
    /// top, right, bottom): the view's middle is its middle.
    pub fn set_room(&mut self, room: [f32; 4]) {
        let [l, t, r, b] = room;
        self.room = room;
        self.inset = glam::Vec2::new(r - l, b - t);
    }

    /// The chosen item brought into the view if it lies outside the room
    /// the panels leave (chosen in the list): the view moved to have it in
    /// the middle, not turned.
    pub fn bring_into_view(&mut self, ed: &Editor) {
        let Some(at) = ed.pieces.chosen.and_then(|i| ed.at_of(i)) else { return };
        let p = IVec3::from(at).as_vec3() + Vec3::new(0.5, 0.5, 0.0);
        let c = self.view_proj(&self.level, self.aspect) * p.extend(1.0);
        let (x, y) = (c.x / c.w, c.y / c.w);
        let [l, t, r, b] = self.room;
        let inside = c.w > 0.0 && x > -0.9 + 2.0 * l && x < 0.9 - 2.0 * r && y < 0.9 - 2.0 * t && y > -0.9 + 2.0 * b;
        if !inside {
            self.level.target = p;
        }
    }

    /// GAME VIEW: the level as the game's camera sees it as a run starts
    /// (its turn, tilt, field of view and width), on the player where the
    /// run begins (`GameStart::target`), stopped at the level's edges as
    /// the game stops it (`c.camera.stop_at_edges`, `session::within`).
    pub fn game_view(&mut self, ed: &Editor) {
        let g = &self.game;
        let width = crate::camera::shown_width(g, self.aspect);
        let target = self.start.target(ed, g.snap);
        let stage = (Vec3::ZERO, ed.level.voxels().as_vec3());
        let target = crate::session::within(stage, target, g, self.aspect);
        // As wide in the room the panels leave as the game's whole window.
        let width = width / (1.0 - self.room[0] - self.room[2]).max(0.3);
        self.level = Orbit { yaw: g.yaw, pitch: g.pitch, zoom: width.max(4.0), target, fov: g.fov };
    }

    /// The background chosen for the editor `scope` is in (the level's,
    /// or the voxel editor's: a block, a model, a figure); none in GAME
    /// VIEW (the game's own sky).
    pub fn background(&self, scope: Scope) -> Option<[u8; 3]> {
        if self.before_game.is_some() {
            return None;
        }
        self.backgrounds[(scope != Scope::Level) as usize]
    }

    /// The marks over the view, as the background asks.
    pub fn marks(&self, scope: Scope) -> super::background::MarkColors {
        super::background::MarkColors::on(self.background(scope))
    }

    /// A figure's GAME VIEW: the game's camera (its turn, tilt, field of
    /// view and width) on the figure's middle.
    /// GAME VIEW on a figure: the game's angle (yaw, pitch, fov), framed
    /// as the editor frames it (the figure's voxels fill the view), not at
    /// the game's distance: a figure of 40 voxels in a view 500 across is
    /// a speck. The wheel, the corner buttons and the right button go on
    /// from there.
    pub fn figure_game_view(&mut self, ed: &Editor) {
        let g = &self.game;
        // (By its voxels, as a figure opens, not by its stated box.)
        let at = ed.figure_box().map_or(IVec3::ZERO, |(lo, _)| lo);
        let voxels = ed.figure.as_ref().and_then(|f| f.model.frames[0].iter().map(|v| v.0).fold(None, |b: Option<(IVec3, IVec3)>, c| Some(b.map_or((c, c), |(lo, hi)| (lo.min(c), hi.max(c))))));
        let (lo, size) = voxels.map_or((Vec3::ZERO, ed.grid().as_vec3()), |(lo, hi)| ((at + lo).as_vec3(), (hi - lo + 1).as_vec3()));
        // The title's logo is no thing of the world: the title lays it in
        // its own scene, seen straight from the front (`ui::logo_model_parts`).
        let (yaw, pitch) = if self.face == Face::Logo { (0.0, 0.0) } else { (g.yaw, g.pitch) };
        self.block = Orbit { yaw, pitch, zoom: (size.max_element() * 2.8).max(16.0), target: lo + size / 2.0, fov: g.fov };
    }

    /// The view moved by hand: a level's GAME VIEW is the game's camera
    /// and ends; a figure's stays on (its light and outline), only the
    /// angle and the size the user's.
    pub fn leave_game_view(&mut self, scope: Scope) {
        if scope == Scope::Level {
            self.before_game = None;
        }
    }

    /// Which of FRONT, SIDE and TOP the view looks from now, if any.
    pub fn straight(&self, scope: Scope) -> Option<usize> {
        let face = self.face_in(scope);
        (0..STRAIGHT.len()).find(|&k| straight_of(face, k) == Some((self.block.yaw, self.block.pitch)))
    }

    /// The side a model shows the game: a level's block opened its
    /// front as it lies.
    fn face_in(&self, scope: Scope) -> Face {
        if matches!(scope, Scope::Block(_)) { Face::Block } else { self.face }
    }

    /// FRONT, SIDE or TOP (`k`): the view looks from there, at what it
    /// looked at, as near; pressed while it does, the free view's yaw and
    /// pitch as they were before the first of them.
    pub fn straight_view(&mut self, scope: Scope, k: usize) {
        let Some((yaw, pitch)) = straight_of(self.face_in(scope), k) else { return };
        let now = self.straight(scope);
        let orbit = self.orbit(scope);
        if now == Some(k) {
            if let Some((yaw, pitch)) = self.free.take() {
                let orbit = self.orbit(scope);
                (orbit.yaw, orbit.pitch) = (yaw, pitch);
            }
            return;
        }
        let was = (orbit.yaw, orbit.pitch);
        (orbit.yaw, orbit.pitch) = (yaw, pitch);
        if now.is_none() {
            self.free = Some(was);
        }
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
        // (GAME VIEW draws the figure whole as the game does: no part's
        // grid to look through.)
        self.rest = if self.before_game.is_some() && ed.scope != Scope::Level { Mat4::IDENTITY } else { ed.rest_view() };
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
        let m = Mat4::from_translation(Vec3::new(-self.inset.x, self.inset.y, 0.0)) * orbit.view_proj(aspect);
        // A figure's part shaped at rest: the view looks through its turn,
        // so its own grid (what the tools work in) is drawn turned.
        if std::ptr::eq(orbit, &self.block) { m * self.rest } else { m }
    }

    /// Where a point of the screen (shares) points in the grid shown.
    pub fn aim(&self, ed: &Editor, aspect: f32, x: f32, y: f32) -> Option<Hit> {
        let (origin, dir) = self.ray(ed, aspect, x, y);
        ed.aim(origin, dir)
    }

    /// Where the eye is over a block, a model or a figure (the grid's
    /// cells): the side SLICE cuts away is the one it is on.
    pub fn block_eye(&self, aspect: f32) -> Vec3 {
        crate::camera::eye(&self.block.camera(), self.block.target, aspect)
    }

    /// The ray through a point of the screen (shares), in the shown grid's
    /// cells.
    pub fn ray(&self, ed: &Editor, aspect: f32, x: f32, y: f32) -> (Vec3, Vec3) {
        let (orbit, unit) = match ed.scope {
            Scope::Level => (&self.level, ed.level.n.max(1) as f32),
            Scope::Block(_) | Scope::Frame(_) | Scope::Figure => (&self.block, 1.0),
        };
        let (origin, dir) = mouse_ray(self.view_proj(orbit, aspect), x, y);
        (origin / unit, dir)
    }

    /// The ray through a point of the screen (shares) in the level's
    /// voxels.
    pub fn voxel_ray(&self, aspect: f32, x: f32, y: f32) -> (Vec3, Vec3) {
        mouse_ray(self.view_proj(&self.level, aspect), x, y)
    }

    /// Where a point of the screen (shares) points among the level's
    /// things: the spot a thing would stand on (on the voxels shown) and
    /// the nearest thing in front of them there.
    pub fn aim_things(&self, ed: &Editor, shown: &Shown, figures: &Figures, aspect: f32, x: f32, y: f32) -> (Option<IVec3>, Option<usize>) {
        let (origin, dir) = mouse_ray(self.view_proj(&self.level, aspect), x, y);
        things_along(ed, &shown.world, figures, origin, dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::tests::folder;

    /// A kind of thing is drawn facing as the game faces it as a run
    /// starts: Voxkeep's bat (a flyer whose kind says `facing = { 0, -1 }`)
    /// to the front, its wings spread across, not edge on as the other
    /// figures go; its model opens from the front too. A kind facing
    /// right shows its right; the hero and a walker without `facing` from
    /// the side as before, a prop to the camera.
    /// FRONT, SIDE and TOP look straight at a model (its front +y, its
    /// right side +x, from above), at what was looked at; the one lit
    /// pressed again brings the free view back as it was.
    #[test]
    fn front_side_and_top_and_the_free_view_back() {
        let mut ed = Editor::open_model(Path::new("games/voxkeep"), "hero.vvx").unwrap();
        ed.dry = true;
        let mut view = View::new(&ed, &GameConfig::default());
        let (free, target) = ((view.block.yaw, view.block.pitch), view.block.target);
        assert_eq!(view.straight(ed.scope), None);
        for k in [0, 1, 2] {
            act(&mut ed, &mut view, Action::StraightView(k));
            assert_eq!(view.straight(ed.scope), Some(k as usize));
            assert_eq!(view.block.target, target, "what it looks at stays");
        }
        let dir = |v: &View| crate::camera::view_dir(&v.block.camera());
        assert!(dir(&view).z < -0.999, "TOP: straight down");
        view.straight_view(ed.scope, 0);
        assert!((dir(&view) - Vec3::NEG_Y).length() < 1e-4, "FRONT: at the +y side, from the north");
        view.straight_view(ed.scope, 1);
        assert!((dir(&view) - Vec3::NEG_X).length() < 1e-4, "SIDE: at the +x side, from the east");
        view.straight_view(ed.scope, 1);
        assert_eq!(((view.block.yaw, view.block.pitch), view.straight(ed.scope)), (free, None), "again: the free view as it was");
        // A level's block: its front the level's, y 0.
        let dir_of = |face| {
            let (yaw, pitch) = straight_of(face, 0).unwrap();
            crate::camera::view_dir(&Orbit { yaw, pitch, zoom: 10.0, target: Vec3::ZERO, fov: FOV }.camera())
        };
        assert!((dir_of(Face::Block) - Vec3::Y).length() < 1e-4, "FRONT of a block: its y 0 side, from the south");
    }

    #[test]
    fn a_kind_faces_as_its_facing_says() {
        let dir = Path::new("games/voxkeep");
        let cfg = crate::session::config::<crate::Voxcade>(dir).unwrap();
        let figures = Figures::load(dir, &cfg);
        let bat = crate::vox::load_model(&dir.join("bat.vvx")).unwrap();
        assert!(bat.size.x > 3 * bat.size.y, "the bat is wide across, thin deep: {}", bat.size);
        let across = |kind: &str| {
            let (lo, hi) = figures.bounds(kind);
            (hi - lo).truncate()
        };
        assert_eq!(across("bat"), glam::IVec2::new(bat.size.x, bat.size.y), "the bat to the front, its wings across");
        assert_eq!(face_of("bat.vvx", &cfg), Face::Front);
        // The others as before: the hero right, a walker left, a prop to
        // the camera.
        assert_eq!(face_of("hero.vvx", &cfg), Face::Right);
        assert_eq!(face_of("slime.vvx", &cfg), Face::Left);
        assert_eq!(face_of("torch.vvx", &cfg), Face::Front);
        let way = |table: &str| start_way(&toml::from_str::<toml::Table>(table).unwrap());
        assert_eq!(way("behavior = \"walker\"\nfacing = [1, 0]"), crate::grid::Way::East);
        assert_eq!(way("behavior = \"flyer\"\nfacing = [0.2, -1.0]"), crate::grid::Way::South);
        assert_eq!(way("behavior = \"hero\""), crate::grid::Way::East);
        assert_eq!(way("behavior = \"hopper\""), crate::grid::Way::West);
        assert_eq!(way("behavior = \"prop\""), crate::grid::Way::South);
    }

    /// A level's GAME VIEW looks where the game's camera looks as a run
    /// starts: on the player where it begins, stopped at the level's edges
    /// (Voxkeep's hall whole, not its middle at the floor; BunnyFarm's
    /// meadow on the burrow), through the game's own `within`.
    #[test]
    fn game_view_looks_where_the_game_looks_as_a_run_starts() {
        for (game, level) in [("games/voxkeep", "levels/01_hall.toml"), ("games/bunnyfarm", "levels/01_farm.toml")] {
            let dir = Path::new(game);
            let cfg = crate::session::config::<crate::Voxcade>(dir).unwrap();
            let mut ed = Editor::open(dir, level).unwrap();
            let mut view = View::new(&ed, &cfg);
            act(&mut ed, &mut view, Action::GameView);
            let s = crate::session::Session::<crate::Voxcade>::load(dir).unwrap();
            let origin = GameStart::of(&cfg, dir).origin.as_vec3();
            let want = crate::session::within(crate::game::Game::sim(&s.game).world.stage_box(), s.view_target(1.0), &cfg.camera, view.aspect) - origin;
            assert_eq!(view.level.target, want, "{game}");
            assert_eq!((view.level.yaw, view.level.pitch, view.level.fov), (cfg.camera.yaw, cfg.camera.pitch, cfg.camera.fov), "{game}");
        }
    }

    /// The ray through the middle of the screen passes through the point
    /// the camera looks at, from any side.
    #[test]
    fn a_ray_through_the_middle_of_the_screen_meets_the_target() {
        for (yaw, pitch) in [(0.0, 15.0), (200.0, 20.0), (30.0, -50.0), (90.0, 85.0)] {
            let orbit = Orbit { yaw, pitch, zoom: 70.0, target: Vec3::new(8.0, 3.0, 5.0), fov: FOV };
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
        ed.level.things.push(crate::grid::Thing::new("gem", [8, 2, 0]));
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

    /// Among the pieces a click finds the thing or place under the mouse
    /// before an area round it, an area where nothing else is; the lights
    /// shine as the game puts them, the 16 nearest the view's middle.
    #[test]
    fn the_mouse_finds_pieces_and_lights_shine() {
        let dir = crate::scratch::Scratch::new("view-pieces");
        std::fs::write(dir.join("room.toml"), crate::grid::tests::PIECES).unwrap();
        let mut ed = Editor::open(&dir, "room.toml").unwrap();
        let figures = Figures::load(&dir, &GameConfig::default());
        let down = |ed: &Editor, x: f32, y: f32| item_along(ed, &figures, Vec3::new(x, y, 500.0), Vec3::NEG_Z);
        assert_eq!(down(&ed, 40.5, 150.5), Some(Item::Thing(0)), "the stove, in the area by it");
        assert_eq!(down(&ed, 31.0, 13.0), Some(Item::Place(0)), "the door, the first of two places there");
        assert_eq!(down(&ed, 65.0, 125.0), Some(Item::Area(0)), "the area, nothing else there");
        assert_eq!(down(&ed, 200.0, 100.0), None);
        let shining = lights(&ed, Vec3::ZERO);
        assert_eq!(shining.len(), 2);
        assert_eq!(shining[0].pos, Vec3::new(212.5, 40.5, 0.5) + room::rotation_of(&ed.level.things[1]) * Vec3::new(3.0, 3.0, 41.0), "the lamp's, from its model's voxels");
        for k in 0..20 {
            ed.level.things.push(crate::grid::Thing { lights: vec![crate::grid::ThingLight { at: None, radius: 5.0, color: None }], ..crate::grid::Thing::new(crate::grid::PIECE, [k, 0, 0]) });
        }
        assert_eq!(lights(&ed, Vec3::ZERO).len(), crate::render::MAX_LIGHTS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A model is seen as the game shows it as a run starts: the hero
    /// looking right, other figures left, anything else turned to the
    /// camera, a block as it lies; the picture of a frame from there has
    /// the nearest voxels.
    #[test]
    fn a_model_is_seen_as_the_game_shows_it() {
        let mut cfg = GameConfig::default();
        cfg.things.insert("fox".into(), toml::toml! { behavior = "hero"
        model = "fox.vox" });
        cfg.things.insert("sign".into(), toml::toml! { behavior = "prop"
        model = "sign.vox" });
        cfg.things.insert("bat".into(), toml::toml! { behavior = "flyer"
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
