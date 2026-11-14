//! Voxcade's two editors on one `Editor` (`model.rs`; `docs/tiles.md`):
//! the level editor (a level's blocks in their cells, its things, its
//! pieces, lights, places and areas, its terrain) and the voxel editor (a
//! block opened from the level, a model of frames, a figure of parts),
//! each in a window of its own (`window.rs`) or drawn into a PNG without
//! one (`shot.rs`). Every step is saved at once, never over a newer file
//! (`disk.rs`), and a game running beside it shows it straight away. The
//! panels are the IDE's look in three zones (`zones.rs`, `panels.rs`:
//! top bar, outliner, inspector; egui), the 3D view between them in the
//! game's own light (`view.rs`, `picture.rs`). Calm on purpose: few tools
//! in view, large and readable.

pub mod background;
pub mod cell_lines;
#[cfg(test)]
pub(crate) mod big;
pub mod figure;
pub mod context;
pub mod frames;
pub mod handles;
pub mod keys;
pub mod model;
pub mod panels;
pub mod pick;
pub mod shaping;
mod shot;
pub mod slice;
pub mod stage;
pub mod zones;
pub mod view;
mod window;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::game::Game;
use model::{Editor, Item, Layer, Scope, Tool};
use view::{Figures, Shown, View};
pub use shot::*;
pub use window::*;

/// Opens the editor on the game in `dir`, on its `level` (a file in the
/// game folder; else the one its config names), at `block` if given, else
/// on the level's blocks or its `things`.
fn open<G: Game>(dir: &Path, level: Option<&str>, block: Option<&str>, things: bool) -> Result<(Editor, View, crate::config::GameConfig)> {
    let cfg = crate::session::config::<G>(dir)?;
    let file = match level {
        Some(file) => file.to_string(),
        None => model::level_file(dir, &cfg)?,
    };
    let mut ed = Editor::open(dir, &file)?;
    // Things turned off the quarter turns in a game on the grid: asked
    // before the level opens, nothing changed until answered.
    let off = ed.level.off_grid();
    if cfg.camera.rotates == crate::config::Rotates::Quarter && !off.is_empty() {
        ed.question = Some(off.iter().map(|t| t.called()).collect());
    }
    ed.kinds = cfg.things.keys().cloned().collect();
    ed.held_kind = ed.kinds.first().cloned();
    for (kind, table) in &cfg.things {
        if let Some(m) = table.get("model").and_then(|m| m.as_str()) {
            ed.pieces.kind_models.insert(kind.clone(), m.to_string());
        }
        if table.get("behavior").is_none() {
            ed.pieces.still_kinds.insert(kind.clone());
        }
        if table.get("rotates").and_then(|t| t.as_str()) == Some("free") {
            ed.pieces.free_kinds.insert(kind.clone());
        }
    }
    ed.pieces.free_turns = cfg.camera.rotates == crate::config::Rotates::Free;
    ed.game_light = cfg.light.clone();
    ed.pieces.scenes = crate::levels::scene_files(dir, &cfg);
    if things {
        ed.switch(Layer::Things);
    }
    let mut view = View::new(&ed, &cfg);
    if let Some(name) = block {
        let i = ed.level.blocks.iter().position(|b| b.name == name).with_context(|| format!("the level has no block '{name}'"))?;
        ed.open_block(i);
        view.block = View::new(&ed, &cfg).block;
    }
    Ok((ed, view, cfg))
}

/// Opens the editor on the model `file` of the game in `dir`.
fn open_model<G: Game>(dir: &Path, file: &str) -> Result<(Editor, View, crate::config::GameConfig)> {
    let cfg = crate::session::config::<G>(dir)?;
    let ed = Editor::open_model(dir, file)?;
    let view = View::new(&ed, &cfg);
    Ok((ed, view, cfg))
}

/// The room the panels a frame drew (`drawn`) leave the 3D view in the
/// window (`screen`, egui's points), and as shares of it (left, top,
/// right, bottom: `View::set_room`): between the outliner, the top bar
/// and the inspector; the whole window under the question a level asks
/// before it opens.
pub(crate) fn room_of(drawn: &panels::PanelsDrawn, screen: egui::Rect) -> (egui::Rect, [f32; 4]) {
    let Some(free) = drawn.free else { return (screen, [0.0; 4]) };
    let (w, h) = (screen.width().max(1.0), screen.height().max(1.0));
    (free, [(free.left() - screen.left()) / w, (free.top() - screen.top()) / h, (screen.right() - free.right()) / w, (screen.bottom() - free.bottom()) / h])
}

/// The window as the handles and the marks over the view see it, in
/// egui's points: its size, the shape of the view drawn into it, a point
/// of the mouse from its top left corner, a camera's matrix for it.
#[derive(Clone, Copy)]
pub(crate) struct ViewRoom(pub egui::Rect);

impl ViewRoom {
    pub fn size(self) -> glam::Vec2 {
        glam::Vec2::new(self.0.width(), self.0.height())
    }

    pub fn aspect(self) -> f32 {
        self.0.width() / self.0.height().max(1.0)
    }

    pub fn local(self, mouse: egui::Pos2) -> glam::Vec2 {
        glam::Vec2::new(mouse.x - self.0.left(), mouse.y - self.0.top())
    }

    pub fn view_proj(self, view: &View, orbit: &view::Orbit) -> glam::Mat4 {
        view.view_proj(orbit, self.aspect())
    }
}

/// How near the mouse must be to the joint's ring to take it (points).
pub(crate) const RING_GRAB: f32 = 9.0;

/// Whether the mouse (egui's point) is on the chosen part's joint's ring
/// (only the ring itself takes the mouse: around it the tools work).
pub(crate) fn on_ring(ed: &Editor, view: &View, room: ViewRoom, mouse: egui::Pos2) -> bool {
    let Some(spot) = ed.joint_spot() else { return false };
    handles::project(room.view_proj(view, &view.block), room.size(), spot).is_some_and(|(p, _)| p.distance(room.local(mouse)) <= RING_GRAB)
}

/// A handle under the mouse (egui's point): the grip, the handles, the
/// view's matrix, the mouse in the room's points.
type Under = (handles::Grip, handles::Handles, glam::Mat4, glam::Vec2);

/// SLICE's arrow under the mouse.
fn slice_under(ed: &Editor, view: &View, room: ViewRoom, mouse: egui::Pos2) -> Option<Under> {
    let (vp, at) = (room.view_proj(view, &view.block), room.local(mouse));
    let (grip, h) = ed.slice_grip(vp, room.size(), at)?;
    Some((grip, h, vp, at))
}

/// The arrow of the chosen part under the mouse.
fn joint_under(ed: &Editor, view: &View, room: ViewRoom, mouse: egui::Pos2) -> Option<Under> {
    let (vp, at) = (room.view_proj(view, &view.block), room.local(mouse));
    let h = ed.part_handles(vp)?;
    let grip = h.grip_at(vp, room.size(), at)?;
    Some((grip, h, vp, at))
}

/// The menu a right click at a point of the screen (shares) opens in the
/// voxel editor or the level of blocks: of the voxel there (and the part
/// it is of), of the part there in the figure whole, of the block there in
/// the level.
fn voxel_menu(ed: &Editor, view: &View, aspect: f32, sx: f32, sy: f32) -> Option<view::MenuOn> {
    let (origin, dir) = view.ray(ed, aspect, sx, sy);
    // (The ray in the grid shown, drawn through a part's rest: as the figure
    // stands.)
    let (origin, dir) = (view.rest.transform_point3(origin), view.rest.transform_vector3(dir));
    let part = || ed.figure.as_ref().zip(ed.figure_box()).and_then(|(f, (lo, _))| f.part_at(lo, origin, dir));
    // On a selected voxel: the selection's menu.
    if ed.selection().is_some() {
        let on = match ed.scope {
            Scope::Figure => ed.aim_whole(view.ray(ed, aspect, sx, sy).0, view.ray(ed, aspect, sx, sy).1).map(|(h, p)| (h.cell, Some(p))),
            _ => view.aim(ed, aspect, sx, sy).map(|h| (h.cell, None)),
        };
        if on.is_some_and(|(c, p)| ed.is_selected(c, p)) {
            return Some(view::MenuOn::Selection);
        }
    }
    match ed.scope {
        Scope::Figure => part().map(view::MenuOn::Part),
        Scope::Level if ed.layer == Layer::Blocks => {
            let hit = view.aim(ed, aspect, sx, sy)?;
            let block = ed.level.cell(hit.cell).block;
            (block != 0).then(|| view::MenuOn::Block(block as usize - 1))
        }
        Scope::Level => None,
        Scope::Block(_) | Scope::Frame(_) => {
            let filled = view.aim(ed, aspect, sx, sy).map(|h| h.cell).filter(|&c| ed.shape().is_some_and(|b| b.get(c) != 0));
            match (filled, part()) {
                (Some(c), p) => Some(view::MenuOn::Voxel(c, p.map(|p| p as u16))),
                (None, Some(p)) => Some(view::MenuOn::Part(p)),
                (None, None) => None,
            }
        }
    }
}

/// Whether the tools work on the level's things.
fn among_things(ed: &Editor) -> bool {
    ed.scope == Scope::Level && ed.layer == Layer::Things
}

/// Every face of the world's voxels: the editor's camera goes all round.
pub(crate) const ALL_FACES: crate::mesh::FaceMask = [true; 6];

#[cfg(test)]
mod tests {
    use super::*;

    /// The editor's test level with the sketch's things, places and areas
    /// after its cells (`grid::tests::PIECES`), in a game folder whose
    /// `conf.lua` adds `conf`.
    fn game_with_pieces(name: &str, conf: &str, pieces: &str) -> crate::scratch::Scratch {
        let dir = model::tests::folder(name);
        let level = std::fs::read_to_string(dir.join("level.toml")).unwrap();
        let set_down = pieces.split_once("\n\n").unwrap().1;
        std::fs::write(dir.join("level.toml"), format!("{level}\n{set_down}")).unwrap();
        std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.blocks = {{ file = \"level.toml\" }}\n  c.things.walker = {{}}\n{conf}end\n")).unwrap();
        dir
    }

    /// A level with pieces, lights, places and areas the editor does not
    /// shape yet keeps every one of them, to the character, through a
    /// step and its step back (each written as it is taken).
    #[test]
    fn the_editor_keeps_what_it_does_not_shape() {
        let dir = game_with_pieces("pieces-kept", "  c.camera.rotates = \"free\"\n", crate::grid::tests::PIECES);
        let text = std::fs::read_to_string(dir.join("level.toml")).unwrap();
        let (mut ed, ..) = open::<crate::Voxcade>(&dir, Some("level.toml"), None, false).unwrap();
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
        assert!(ed.question.is_none(), "a game that turns things freely asks nothing");
        ed.kinds = vec!["walker".into()];
        ed.hold_kind(0);
        ed.switch(Layer::Things);
        ed.press_thing(Some(glam::IVec3::new(2, 2, 8)), None);
        ed.release();
        assert_ne!(std::fs::read_to_string(dir.join("level.toml")).unwrap(), text);
        ed.undo();
        assert_eq!(std::fs::read_to_string(dir.join("level.toml")).unwrap(), text);
        // PICK on a piece holds no kind (there is none to put down).
        let at = ed.level.things.iter().position(|t| t.is_piece()).unwrap();
        ed.choose(model::Tool::Pick);
        ed.press_thing(None, Some(at));
        assert_eq!(ed.held_kind.as_deref(), Some("walker"));
        // A level without blocks opens among its pieces, nothing written.
        std::fs::write(dir.join("room.toml"), crate::grid::tests::PIECES).unwrap();
        let (room, view, _) = open::<crate::Voxcade>(&dir, Some("room.toml"), None, false).unwrap();
        assert!(room.among_pieces() && room.pieces.free_turns && view.level.zoom >= 360.0);
        assert_eq!(std::fs::read_to_string(dir.join("room.toml")).unwrap(), crate::grid::tests::PIECES);
        // Its level is the first in levels/ where the config names none.
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        std::fs::write(dir.join("levels/02_b.toml"), crate::grid::tests::PIECES).unwrap();
        std::fs::write(dir.join("levels/10_a.toml"), crate::grid::tests::PIECES).unwrap();
        std::fs::write(dir.join("levels/notes.toml"), "a = 1\n").unwrap();
        assert_eq!(model::level_file(&dir, &crate::config::GameConfig::default()).unwrap(), "levels/02_b.toml");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A level turned off the grid in a game on the grid asks before it
    /// opens: DON'T OPEN leaves the file as it is; ALIGN ALL faces each
    /// thing its nearest way, one step back, written as every step is.
    #[test]
    fn a_level_off_the_grid_asks_before_it_opens() {
        let dir = game_with_pieces("pieces-asked", "", crate::grid::tests::PIECES);
        let text = std::fs::read_to_string(dir.join("level.toml")).unwrap();
        let (mut ed, mut view, _) = open::<crate::Voxcade>(&dir, Some("level.toml"), None, false).unwrap();
        assert_eq!(ed.question.as_deref(), Some(&["'chair' (line 39)".to_string()][..]));
        view::act(&mut ed, &mut view, view::Action::DontOpen);
        assert_eq!(std::fs::read_to_string(dir.join("level.toml")).unwrap(), text, "DON'T OPEN touches nothing");
        view::act(&mut ed, &mut view, view::Action::AlignAll);
        assert!(ed.question.is_none() && ed.level.off_grid().is_empty());
        let aligned = std::fs::read_to_string(dir.join("level.toml")).unwrap();
        assert_eq!(aligned, text.replace("rotation = 45", "facing = \"east\""), "written at once, nothing else changed");
        assert_eq!(ed.steps(), 1);
        ed.undo();
        assert_eq!(std::fs::read_to_string(dir.join("level.toml")).unwrap(), text, "a step back");
    }
}
