//! Voxcade's editor for levels of voxel blocks (`docs/tiles.md`): one
//! window, two levels. In the level the cells hold blocks, placed,
//! removed, replaced, turned and mirrored, and its things stand on them,
//! placed, moved and removed; a block opened (a second click on it, or a
//! double click on a cell) is shaped voxel by voxel. Every change is saved
//! at once, and a game running beside it shows it straight away. It is
//! calm on purpose: few tools in view, large and readable. Its panels are
//! the IDE's look (`panels.rs`, egui), the 3D view the game's own light.

pub mod background;
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
pub mod slice;
pub mod stage;
pub mod zones;
pub mod view;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::game::Game;
use model::{Editor, Item, Layer, Scope, Tool};
use view::{Figures, Shown, View};

/// How `voxcade edit --shot` draws the editor without a window.
pub struct ShotOptions {
    /// The level to open (its file in the game folder: `levels/02_crypt.toml`),
    /// else the one `c.world.blocks.file` names.
    pub level: Option<String>,
    /// The block to open (its name), else the level.
    pub block: Option<String>,
    /// A model to open instead (its path in the game folder); of a figure
    /// of parts the part chosen, shaped in the figure, and its parent and
    /// joint set by hand (the joint as the printed tree has it, from the
    /// figure's low corner); nothing written.
    pub model: Option<String>,
    pub part: Option<String>,
    pub parent: Option<String>,
    pub joint: Option<glam::Vec3>,
    /// BOX or SPHERE set where the mouse is (the middle of the view), then
    /// drawn again, no more fresh (`held` alone: fresh, outlined).
    pub stamp: bool,
    /// The level's things, not its blocks.
    pub things: bool,
    /// Shift held: the tool is PICK, the one chosen put aside.
    pub shift: bool,
    /// The question a level turned off the grid asks answered ALIGN ALL
    /// before the shot (without: the question is what is drawn).
    pub align: bool,
    /// The tool chosen (its word: box, sphere, add ...), and for BOX and
    /// SPHERE the size of the shape set under the mouse.
    pub tool: Option<String>,
    pub held: Option<glam::IVec3>,
    /// The camera turned (yaw) and tilted (pitch), in degrees.
    pub view: Option<(f32, f32)>,
    pub width: u32,
    pub height: u32,
    pub out: PathBuf,
    /// Among the pieces: changes tried in turn (nothing written), the item
    /// chosen after them (by its name), the view as the game's camera
    /// sees, ADD MODEL's list shown, the lights off.
    pub ops: Vec<ShotOp>,
    pub choose: Option<String>,
    pub game_view: bool,
    pub picker: bool,
    pub lights_off: bool,
    /// Among the pieces, after the changes: the chosen piece's light (its
    /// place among its lights) chosen; the menu of an item open (by its
    /// name; `floor`: of the floor under the middle of the view), ADD's
    /// menu open, LEVEL pressed; a handle under the mouse (`arrow-x`,
    /// `ring-z`, `face-y`, `face-y-low` ...).
    pub choose_light: Option<usize>,
    pub menu: Option<String>,
    pub add_menu: bool,
    pub level_settings: bool,
    pub hover: Option<String>,
    /// The chosen one's handles in TURN (its rings), else MOVE.
    pub turn_handles: bool,
    /// A figure's parts moved by whole voxels (`--move-part`), joints by
    /// halves (`--move-joint`, true), in turn: by the part's name.
    pub part_moves: Vec<(String, glam::Vec3, bool)>,
    /// Among the pieces: things hidden in the editor (by name); a click
    /// at a pixel of the shot, what it chooses printed and chosen.
    pub hide: Vec<String>,
    pub click: Option<(f32, f32)>,
    /// A block, a model or a figure sliced across an axis (0 x, 1 y, 2 z)
    /// at a layer.
    pub slice: Option<(usize, i32)>,
    /// A model's cells changed (`--clear`, `--fill`) in its frame (from 1)
    /// or its part (`--part`); `dry`: printed, not written.
    pub cells: Vec<frames::CellOp>,
    pub frame: Option<usize>,
    pub dry: bool,
    /// A model shaped in place: the level's file and the piece (by name)
    /// round which it stands; CONTEXT switched off.
    pub context: Option<(String, String)>,
    pub context_off: bool,
    /// A figure's parts rotated at rest about their joints (`--rotate-part
    /// NAME --by DEG [--about rotation|pitch|roll]`), in turn.
    pub part_turns: Vec<(String, f32, usize)>,
    /// `--pose-part NAME --by DEG [--about ...]`: POSE lit, the part posed
    /// so (nothing written); `last_pose`: the last of these flags was one.
    pub part_poses: Vec<(String, f32, usize)>,
    pub last_pose: bool,
    /// A part added to the one `--part` chose; that part merged into
    /// another (by name).
    pub add_part: bool,
    pub merge_into: Option<String>,
    /// `--mirror`: MIRROR on the part `--part` chose.
    pub mirror: bool,
    /// `--background #rrggbb`: the view's background (else the dark one of
    /// always, whatever the person chose).
    pub background: Option<[u8; 3]>,
    /// `--select X0,Y0,Z0 --to X1,Y1,Z1`: the voxels in that box of the
    /// grid shown selected; `--selection OP`: done at the selection, in
    /// order (remove, paint, duplicate, flip-x|y|z, rotate-x|y|z,
    /// new-part, move-to:PART, copy, paste, move:X,Y,Z).
    pub select: Option<(glam::IVec3, glam::IVec3)>,
    /// `--marquee X0,Y0,X1,Y1`: SELECT's rectangle dragged there (points
    /// of the shot), its outline drawn.
    pub marquee: Option<[f32; 4]>,
    pub selection_ops: Vec<String>,
    /// The level's terrain shown (TERRAIN; a flat one where it has none),
    /// a point raised with the brush (`--raise X,Y --by N [--radius R]`),
    /// a point given a shape (`--shape X,Y smooth|linear|step`), in turn.
    pub terrain: bool,
    pub land_ops: Vec<LandOp>,
    /// TERRAIN's material chosen (its place, from 1), its fields shown.
    pub material: Option<usize>,
    /// The terrain's scatter rules (from 1) whose SCATTER is pressed, in
    /// turn, the last chosen; without `--shot` written into the level.
    pub scatter: Vec<usize>,
}

/// `voxcade edit GAME --level FILE --scatter N` without a window: scatter
/// rule N's pieces set into the level and written as the editor writes
/// them (its SCATTER); what it did said.
pub fn scatter_level<G: Game>(dir: &Path, opts: &ShotOptions) -> Result<()> {
    let (mut ed, _view, _cfg) = open::<G>(dir, opts.level.as_deref(), None, false)?;
    ed.dry = opts.dry;
    for &k in &opts.scatter {
        let before = ed.level.things.len();
        ed.scatter_pieces(k.saturating_sub(1));
        if let Some(h) = ed.hint.take() {
            println!("{h}");
        }
        println!("scatter {k}: {} pieces set down{}", ed.level.things.len() - before, if opts.dry { " (dry: nothing written)" } else { "" });
    }
    Ok(())
}

/// A change `--shot` makes to the terrain.
#[derive(Clone, Debug)]
pub enum LandOp {
    Raise(glam::IVec2, i32, i32),
    Shape(glam::IVec2, crate::terrain::Shape),
}

/// Which frame or part `--clear` and `--fill` change: `--part`'s, else
/// `--frame`'s (the first).
fn cells_of(opts: &ShotOptions) -> frames::CellsOf {
    match &opts.part {
        Some(name) => frames::CellsOf::Part(name.clone()),
        None => frames::CellsOf::Frame(opts.frame.unwrap_or(1)),
    }
}

/// `voxcade edit GAME --model FILE --clear ... | --fill ...` without a
/// window: the model's cells changed and written as the editor writes
/// them (`--dry`: only said). Prints what changed.
pub fn change_cells(dir: &Path, opts: &ShotOptions) -> Result<()> {
    let file = opts.model.as_deref().context("--clear and --fill change a model: --model FILE")?;
    let mut model = frames::ModelFile::load(&dir.join(file))?;
    let changed = model.change_cells(&cells_of(opts), &opts.cells)?;
    if changed == frames::Changed::default() {
        println!("{file}: nothing changed");
        return Ok(());
    }
    if !opts.dry {
        model.save()?;
    }
    println!("{file}: {changed}{}", if opts.dry { " (dry: nothing written)" } else { "" });
    Ok(())
}

/// A change `--shot` tries among the pieces, to see what it does: a model
/// (its path in the game folder; or `light`, `place`, `area`) set down at
/// a spot; an item (by its name) moved to a spot, turned by degrees,
/// removed, copied to a spot, renamed; a piece's light set (its radius,
/// its colour `#rrggbb`, where it shines from in the model; `true`: a
/// light more on it, else its first); a piece made solid; the room's
/// size.
#[derive(Clone, Debug, PartialEq)]
pub enum ShotOp {
    Add(String, Option<glam::IVec3>),
    Hang(String, Option<glam::IVec3>),
    Move(String, Option<glam::IVec3>),
    /// Turned by degrees about an axis (0 x: pitch, 1 y: roll, 2 z: round).
    Turn(String, f64, usize),
    Remove(String),
    Copy(String, Option<glam::IVec3>),
    Rename(String, String),
    Light(String, Option<f64>, Option<String>, Option<glam::IVec3>, bool),
    Solid(String),
    Size(glam::IVec3),
}

impl ShotOp {
    /// The spot `--at` gives the change before it.
    pub fn at(&mut self, spot: glam::IVec3) -> bool {
        match self {
            ShotOp::Add(_, at) | ShotOp::Hang(_, at) | ShotOp::Move(_, at) | ShotOp::Copy(_, at) => *at = Some(spot),
            ShotOp::Light(_, _, _, from, _) => *from = Some(spot),
            _ => return false,
        }
        true
    }
}

/// Tries a change among the pieces (`--shot`), as the panels would make it.
fn try_op(ed: &mut Editor, figures: &Figures, op: &ShotOp) -> Result<()> {
    use model::{Field, Hanging, Item};
    let find = |ed: &Editor, name: &str| ed.find(name).with_context(|| format!("the level has nothing called '{name}'"));
    // What hangs on the mouse set down exactly at a spot.
    let set_at = |ed: &mut Editor, at: glam::IVec3| {
        match &mut ed.hanging {
            Some(Hanging::Thing(t)) => t.at = at.to_array(),
            Some(Hanging::Place(p)) => p.at = at.to_array(),
            Some(Hanging::Area(a)) => a.at = at.to_array(),
            None => {}
        }
        ed.hang_spot = Some(at);
        ed.set_down();
    };
    match op {
        ShotOp::Add(what, at) | ShotOp::Hang(what, at) => {
            match what.as_str() {
                "light" => ed.add_light(),
                "place" => ed.add_place(),
                "area" => ed.add_area(),
                model => {
                    anyhow::ensure!(ed.dir.join(model).is_file(), "--add {model}: no such model in the game folder");
                    ed.add_piece(model);
                }
            }
            let at = at.context("--add and --hang need --at X,Y,Z after them")?;
            match op {
                // Left on the mouse, where it would stand.
                ShotOp::Hang(..) => ed.hang_at(Some(at)),
                _ => set_at(ed, at),
            }
        }
        ShotOp::Move(name, at) => {
            ed.choose_item(Some(find(ed, name)?));
            let at = at.context("--move needs --at X,Y,Z after it")?;
            for axis in 0..3 {
                ed.set_field(Field::At(axis), at[axis] as f64);
            }
        }
        ShotOp::Turn(name, by, axis) if *axis < 2 => {
            let item = find(ed, name)?;
            ed.choose_item(Some(item));
            anyhow::ensure!(ed.chosen_thing().is_some_and(|t| t.model.is_some()), "--rotate {name} --about x or y: only a piece of a model tips");
            anyhow::ensure!(ed.tip(*axis, *by), "--rotate {name} --by {by}: this game turns things on the grid, in quarter turns (c.camera.rotates = \"free\" for any angle; a solid piece keeps to them too)");
        }
        ShotOp::Turn(name, by, _) => {
            let item = find(ed, name)?;
            ed.choose_item(Some(item));
            if by.rem_euclid(90.0) == 0.0 {
                for _ in 0..(by / 90.0).rem_euclid(4.0) as i32 {
                    ed.turn_item(1);
                }
            } else if let Some(t) = ed.chosen_thing() {
                anyhow::ensure!(ed.free_turns, "--rotate {name} --by {by}: this game turns things on the grid, in quarter turns (c.camera.rotates = \"free\" for any angle)");
                let b = model::room::bearing_of(t);
                ed.set_field(Field::Turn, b + by);
            }
        }
        ShotOp::Remove(name) => {
            ed.choose_item(Some(find(ed, name)?));
            ed.remove_item();
        }
        ShotOp::Copy(name, at) => {
            ed.choose_item(Some(find(ed, name)?));
            ed.copy_item();
            set_at(ed, at.context("--copy needs --at X,Y,Z after it")?);
        }
        ShotOp::Rename(name, to) => {
            let item = find(ed, name)?;
            ed.rename(item, to).map_err(|e| anyhow::anyhow!("--rename {name}: {e}"))?;
        }
        ShotOp::Light(name, radius, colour, from, more) => {
            let item = find(ed, name)?;
            anyhow::ensure!(matches!(item, Item::Thing(_)), "--light {name}: only a piece has a light");
            ed.choose_item(Some(item));
            if *more || ed.chosen_thing().is_some_and(|t| t.lights.is_empty()) {
                let top = ed.chosen_thing().and_then(|t| t.model.as_deref()).and_then(|m| figures.model(m)).map_or(10, |m| m.size.z);
                ed.add_piece_light(top);
            }
            // The light added last, else the first.
            let k = if *more { ed.chosen_thing().map_or(0, |t| t.lights.len().saturating_sub(1)) } else { 0 };
            if let Some(r) = radius {
                ed.set_field(Field::Radius(k), *r);
            }
            if let Some(c) = colour {
                ed.set_colour(k, c);
            }
            if let Some(at) = from {
                for axis in 0..3 {
                    ed.set_field(Field::LightAt(k, axis), at[axis] as f64);
                }
            }
        }
        ShotOp::Solid(name) => {
            let item = find(ed, name)?;
            ed.choose_item(Some(item));
            ed.set_solid(true);
            if let Some(why) = &ed.message {
                anyhow::bail!("--solid {name}: {why}");
            }
        }
        ShotOp::Size(size) => {
            for axis in 0..3 {
                ed.set_field(Field::LevelSize(axis), size[axis] as f64);
            }
        }
    }
    if let Some(m) = &ed.message {
        bail!("{m}");
    }
    Ok(())
}

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
            ed.kind_models.insert(kind.clone(), m.to_string());
        }
        if table.get("behavior").is_none() {
            ed.still_kinds.insert(kind.clone());
        }
        if table.get("rotates").and_then(|t| t.as_str()) == Some("free") {
            ed.free_kinds.insert(kind.clone());
        }
    }
    ed.free_turns = cfg.camera.rotates == crate::config::Rotates::Free;
    ed.game_light = cfg.light.clone();
    ed.scenes = crate::levels::scene_files(dir, &cfg);
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

/// Draws the editor as it opens into a PNG: the mouse in the middle of the
/// view.
pub fn shot<G: Game>(dir: &Path, opts: &ShotOptions) -> Result<()> {
    let (mut ed, mut view, cfg) = match &opts.model {
        Some(file) => open_model::<G>(dir, file)?,
        None => open::<G>(dir, opts.level.as_deref(), opts.block.as_deref(), opts.things)?,
    };
    // Nothing a shot does is written.
    ed.dry = true;
    if opts.align {
        anyhow::ensure!(ed.question.is_some(), "--align answers the question a level turned off the grid asks: this one asks none");
        ed.align_all();
    }
    let part_of = |ed: &model::Editor, name: &str| -> Result<usize> {
        ed.figure.as_ref().context("--part: the model is no figure of parts (its objects have no names)")?.parts().iter().position(|p| p.name == name).with_context(|| format!("the figure has no part '{name}'"))
    };
    if let Some(name) = &opts.part {
        let p = part_of(&ed, name)?;
        view::act(&mut ed, &mut view, view::Action::Part(p));
        if let Some(parent) = &opts.parent {
            let q = part_of(&ed, parent)?;
            ed.set_parent(Some(q));
        }
        if let Some(at) = opts.joint {
            // As the tree prints it: from the figure's low corner.
            let low = ed.figure.as_ref().map_or(glam::Vec3::ZERO, |f| f.low.as_vec3());
            ed.set_joint(Some(at + low));
        }
        ed.follow_figure();
        view.follow_origin(&ed);
    } else if opts.parent.is_some() || opts.joint.is_some() {
        bail!("--parent and --joint set the rig of the part --part names");
    }
    if opts.add_part {
        ed.add_part();
    }
    if opts.mirror {
        let p = ed.figure.as_ref().and_then(|f| f.selected).context("--mirror mirrors the part --part chose")?;
        if let Some(why) = ed.cannot_mirror(p) {
            bail!("--mirror: {why}");
        }
        ed.mirror_part(p);
        view.follow_origin(&ed);
    }
    if let Some(into) = &opts.merge_into {
        let src = ed.figure.as_ref().and_then(|f| f.selected).context("--merge-into NAME merges the part --part chose")?;
        let into = part_of(&ed, into)?;
        if let Some(why) = ed.cannot_merge(src, into) {
            bail!("--merge-into: {why}");
        }
        ed.merge_part(src, into);
    }
    for (name, by, about) in &opts.part_turns {
        let p = part_of(&ed, name)?;
        if ed.figure.as_ref().and_then(|f| f.selected) != Some(p) {
            view::act(&mut ed, &mut view, view::Action::Part(p));
        }
        let now = ed.part_rotation().context("--rotate-part: no figure's part chosen")?;
        ed.set_part_rotation(*about, now[*about] + by);
        ed.follow_figure();
        view.follow_origin(&ed);
    }
    for (name, by, about) in &opts.part_poses {
        let p = part_of(&ed, name)?;
        if !ed.posing {
            ed.set_posing(true);
        }
        if ed.figure.as_ref().and_then(|f| f.selected) != Some(p) {
            view::act(&mut ed, &mut view, view::Action::Part(p));
        }
        let now = ed.chosen_pose().context("--pose-part: no figure's part chosen")?;
        ed.set_pose(*about, now[*about] + by);
        view.follow_origin(&ed);
    }
    // Rotated or posed without --part: the figure whole after.
    if (!opts.part_turns.is_empty() || !opts.part_poses.is_empty()) && opts.part.is_none() {
        view::act(&mut ed, &mut view, view::Action::WholeFigure);
        ed.follow_figure();
        view.follow_origin(&ed);
    }
    for (name, by, joint) in &opts.part_moves {
        let p = part_of(&ed, name)?;
        if ed.figure.as_ref().and_then(|f| f.selected) != Some(p) {
            view::act(&mut ed, &mut view, view::Action::Part(p));
        }
        if *joint {
            let figure = ed.figure.as_ref().context("--move-joint: no figure")?;
            anyhow::ensure!(!ed.root_chosen(), "--move-joint {name}: the root turns about nothing, it has no joint");
            let at = figure.file_joint(p) + (*by * 10.0).round() / 10.0;
            ed.set_joint(Some(at));
        } else if *by == by.round() {
            ed.move_part(by.round().as_ivec3());
        } else {
            // Tenths too: into its `offset`.
            ed.move_part_by((*by * 10.0).round() / 10.0);
        }
        ed.follow_figure();
        view.follow_origin(&ed);
    }
    if let Some(word) = &opts.tool {
        let tool = model::Tool::of(ed.scope, ed.layer).iter().copied().find(|t| t.label(ed.scope, ed.layer).eq_ignore_ascii_case(word));
        ed.choose(tool.with_context(|| format!("--tool {word}: no such tool here"))?);

    }
    if let Some(c) = opts.background {
        view.backgrounds = [Some(c); 2];
    }
    if let Some((lo, hi)) = opts.select {
        ed.select_from_pivot(lo, hi);
        anyhow::ensure!(ed.selection().is_some(), "--select: no voxel in that box");
    }
    for op in &opts.selection_ops {
        use model::selection::SelOp;
        let axis = |w: &str| ["x", "y", "z"].iter().position(|a| *a == w).with_context(|| format!("--selection {op}: an axis x, y or z"));
        match op.split_once(['-', ':']).unwrap_or((op.as_str(), "")) {
            ("remove", _) => ed.selection_step(SelOp::Remove),
            ("paint", _) => ed.selection_step(SelOp::Paint(ed.colour)),
            ("duplicate", _) => ed.duplicate_selection(),
            ("flip", a) => ed.selection_step(SelOp::Flip(axis(a)?)),
            ("rotate", a) => ed.selection_step(SelOp::Rotate(axis(a)?)),
            ("new", "part") => {
                ed.selection_to_new_part().context("--selection new-part: only in a figure, with voxels selected")?;
            }
            ("move", "") => bail!("--selection move:X,Y,Z or move-to:PART"),
            ("move", rest) if rest.starts_with("to:") => {
                let p = part_of(&ed, &rest[3..])?;
                ed.selection_to_part(p);
            }
            ("move", by) => {
                let n: Vec<i32> = by.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--selection move:X,Y,Z")?;
                let [x, y, z] = n[..] else { bail!("--selection move:X,Y,Z: three numbers") };
                ed.grab_selection();
                ed.move_selection_to(glam::IVec3::new(x, y, z), false, false);
                ed.let_go_of_selection();
            }
            ("copy", _) => ed.copy_selection(),
            ("paste", _) => ed.paste(),
            _ => bail!("--selection {op}: remove, paint, duplicate, flip-x|y|z, rotate-x|y|z, new-part, move-to:PART, copy, paste or move:X,Y,Z"),
        }
        ed.follow_figure();
        view.follow_origin(&ed);
    }
    if opts.shift {
        ed.shift_pick(true);
    }
    let aspect = opts.width as f32 / opts.height as f32;
    if let Some((level, name)) = &opts.context {
        anyhow::ensure!(ed.model.is_some(), "--context shapes a model in place: --model FILE");
        let lv = crate::grid::GridLevel::read(&dir.join(level))?;
        let piece = lv.things.iter().position(|t| t.name.as_deref() == Some(name)).with_context(|| format!("--context {level}:{name}: the level has no piece called so"))?;
        ed.set_context(context::Seed { level: level.clone(), piece, hidden: Default::default() })?;
        if opts.context_off {
            ed.toggle_context();
        }
    }
    let mut figures = Figures::load(dir, &cfg).with_pieces(dir, &ed.level);
    figures.follow(dir, &ed);
    // The changes tried among the pieces, then the one chosen, the view.
    if !opts.ops.is_empty() || opts.choose.is_some() {
        if ed.level.has_grid() {
            ed.switch(Layer::Pieces);
        }
        anyhow::ensure!(ed.question.is_none(), "the level asks first whether to align its things (--align)");
    }
    for op in &opts.ops {
        figures.follow(dir, &ed);
        ed.sizes = figures.sizes();
        try_op(&mut ed, &figures, op)?;
        figures.follow(dir, &ed);
    }
    if let Some(name) = &opts.choose {
        let item = ed.find(name).with_context(|| format!("--choose {name}: the level has nothing called so"))?;
        ed.choose_item(Some(item));
    }
    for name in &opts.hide {
        let item = ed.find(name).with_context(|| format!("--hide {name}: the level has nothing called so"))?;
        ed.set_hidden(item, true);
    }
    // The terrain: shown, its points changed as the brush and SHAPE do.
    if opts.terrain || !opts.land_ops.is_empty() || !opts.scatter.is_empty() || opts.material.is_some() {
        anyhow::ensure!(ed.scope == Scope::Level, "--terrain is a level's: --level FILE");
        ed.add_terrain();
        for op in &opts.land_ops {
            let t = ed.terrain().context("no terrain")?;
            let point = |p: glam::IVec2| t.index(p).map(|_| p).with_context(|| format!("--raise/--shape {},{}: the terrain has {} by {} points", p.x, p.y, t.cols, t.rows));
            match *op {
                LandOp::Raise(p, by, radius) => {
                    let p = point(p)?;
                    let from = t.heights.clone();
                    ed.land.radius = radius;
                    ed.land.chosen = Some(p);
                    let before = ed.terrain().cloned();
                    ed.raise_from(&from, p, by);
                    let after = ed.level.terrain.clone();
                    ed.level.terrain = before;
                    let t2 = after.clone();
                    ed.change_level(move |l| l.terrain = t2);
                }
                LandOp::Shape(p, shape) => {
                    let p = point(p)?;
                    ed.land.chosen = Some(p);
                    ed.land.selected.clear();
                    ed.land_shape(shape);
                }
            }
        }
        if let Some(k) = opts.material {
            ed.land.row = Some(model::land::LandRow::Material(k.saturating_sub(1)));
        }
        for &k in &opts.scatter {
            ed.scatter_pieces(k.saturating_sub(1));
            ed.land.row = Some(model::land::LandRow::Scatter(k.saturating_sub(1)));
        }
        if let Some(t) = ed.terrain() {
            println!("terrain: {} by {} points {} apart, heights {} to {}, {} materials", t.cols, t.rows, t.spacing, t.heights.iter().min().unwrap_or(&0), t.heights.iter().max().unwrap_or(&0), t.materials.len());
        }
    }
    view.aspect = aspect;
    // (Where the panels will lie, for the view's middle: as in a window.)
    let (w, h) = (opts.width as f32, opts.height as f32);
    view.set_room([zones::OUTLINER / w, zones::TOP / h, zones::INSPECTOR / w, 0.0]);
    if opts.game_view {
        view::act(&mut ed, &mut view, view::Action::GameView);
    }
    if opts.choose.is_some() {
        view.bring_into_view(&ed);
    }
    if let Some(k) = opts.choose_light {
        ed.choose_light(Some(k));
        anyhow::ensure!(ed.chosen_light == Some(k), "--choose-light {k}: the chosen piece has no light {k} (they count from 0)");
    }
    view.level_settings = opts.level_settings;
    if opts.turn_handles && ed.part_mode().is_some() {
        // A figure's part: its rings.
        ed.part_rotating = true;
    } else if opts.turn_handles {
        view::act(&mut ed, &mut view, view::Action::HandleMode(true));
        anyhow::ensure!(view.turn_mode(&ed), "--handles rotate: what is chosen does not turn (a piece of a model, a place or a figure's part does)");
    }
    if opts.add_menu {
        view::act(&mut ed, &mut view, view::Action::AddMenu);
    }
    if let Some((yaw, pitch)) = opts.view {
        let orbit = view.orbit(ed.scope);
        orbit.yaw = yaw;
        orbit.pitch = pitch.clamp(-85.0, 85.0);
    }
    (view.picker, view.lights) = (opts.picker, !opts.lights_off);
    // Cells changed, to look at (nothing written).
    if !opts.cells.is_empty() {
        let model = ed.model.as_mut().context("--clear and --fill change a model: --model FILE")?;
        let changed = model.change_cells(&cells_of(opts), &opts.cells)?;
        println!("{}: {changed} (shown, not written)", opts.model.as_deref().unwrap_or_default());
        ed.follow_figure();
        view.follow_origin(&ed);
    }
    if let Some((axis, at)) = opts.slice {
        anyhow::ensure!(ed.can_slice(), "--slice cuts a block, a model or a figure");
        ed.set_slice(Some(axis));
        ed.set_slice_at(at);
        ed.slice_toward(view.block_eye(aspect));
    }
    let mut shown = Shown::new(&ed)?;
    // A shot holds a model's frames still (the one shaped shows).
    view.playing = false;
    let mut canvas = crate::headless::Canvas::new(opts.width, opts.height, false)?;
    // The panels first, laid out over frames a sixtieth of a second apart
    // until egui is settled (as the IDE's shot): where they lie moves the
    // view's middle.
    let ctx = egui::Context::default();
    crate::gui::style(&ctx);
    let mut painter = crate::gui::Painter::new(canvas.gpu().0, wgpu::TextureFormat::Rgba8Unorm, true);
    let (mut input, mut thumbs, mut drawn) = (crate::gui::input::Input::default(), panels::Thumbs::default(), panels::Shown::default());
    let mut last = None;
    // With --held or --stamp twice: the shape set, then all drawn again.
    let again = opts.stamp || opts.held.is_some() || opts.menu.is_some() || opts.hover.is_some() || opts.click.is_some() || opts.marquee.is_some();
    for pass in 0..=again as usize {
        for step in 0..120 {
            let mut raw = input.take([opts.width, opts.height], 1.0, 1.0);
            raw.time = Some((pass * 120 + step) as f64 / 60.0);
            ed.sizes = figures.sizes();
            let out = ctx.run_ui(raw, |ui| {
                drawn = panels::ui(ui, &ed, &view, &figures, &mut thumbs);
                let free = room_of(&drawn, ui.ctx().content_rect()).0;
                panels::joints(ui, &ed, &view, free);
                panels::marks(ui, &ed, &view, &figures, free);
            });
            let settled = step >= 2 && out.viewport_output.get(&egui::ViewportId::ROOT).is_none_or(|v| v.repaint_delay == Duration::MAX);
            if settled {
                last = Some(out);
                break;
            }
            let (device, queue, _) = canvas.gpu();
            painter.skip(device, queue, out);
        }
        view.set_room(room_of(&drawn, ctx.content_rect()).1);
        // The mouse in the middle of the view.
        let (mx, my) = (0.5 - view.inset.x / 2.0, 0.5 - view.inset.y / 2.0);
        // Among the pieces after the panels are laid out: a menu open, a
        // handle under the mouse (the room is known now).
        if pass == 0 {
            let screen = ctx.content_rect();
            if let Some(name) = &opts.menu {
                let at = egui::pos2(screen.left() + mx * screen.width(), screen.top() + my * screen.height());
                if name == "keys" {
                    // F1's page of the keys.
                    view.keys_shown = true;
                } else if let Some(part) = name.strip_prefix("part:") {
                    // A figure's part's menu (its row's, a right click).
                    let p = part_of(&ed, part)?;
                    view.menu = Some(view::Menu { on: view::MenuOn::Part(p), at: [zones::OUTLINER - 40.0, 120.0] });
                } else if name == "selection" {
                    // The selection's (a right click on it), at the middle.
                    anyhow::ensure!(ed.selection().is_some(), "--menu selection: nothing selected (--select)");
                    view.menu = Some(view::Menu { on: view::MenuOn::Selection, at: [at.x, at.y] });
                } else if !ed.among_pieces() {
                    // The voxel editor's, of what is under the middle.
                    anyhow::ensure!(name == "voxel", "--menu {name}: not in a level of blocks or the voxel editor, which have --menu voxel (the voxel, the part or the block under the middle of the view), part:NAME, selection and keys (floor and a thing's name are among the pieces)");
                    let on = voxel_menu(&ed, &view, aspect, mx, my).context("--menu voxel: nothing under the middle of the view")?;
                    view.menu = Some(view::Menu { on, at: [at.x, at.y] });
                } else if name == "floor" {
                    let (origin, dir) = view.voxel_ray(aspect, mx, my);
                    let spot = view::floor_spot(&shown.world, origin, dir).context("--menu floor: no floor under the middle of the view")?;
                    view.menu = Some(view::Menu { on: view::MenuOn::Floor(spot), at: [at.x, at.y] });
                } else {
                    let item = ed.find(name).with_context(|| format!("--menu {name}: the level has nothing called so"))?;
                    // At the thing as it stands on the screen.
                    let at = stage::handles_on_screen(&ed, &view, screen).map_or(at, |(_, _, b)| b.center());
                    view::act(&mut ed, &mut view, view::Action::OpenMenu(item, [at.x, at.y]));
                }
            }
            if let Some(word) = &opts.hover {
                let axis = |c: &str| ["x", "y", "z"].iter().position(|a| *a == c).with_context(|| format!("--hover {word}: arrow-x, ring-z, face-y, face-y-low and the like"));
                let parts: Vec<&str> = word.split('-').collect();
                view.grip_hover = Some(match parts[..] {
                    ["arrow", a] => handles::Grip::Arrow(axis(a)?),
                    ["ring", a] => handles::Grip::Ring(axis(a)?),
                    ["face", a] => handles::Grip::Face(axis(a)?, true),
                    ["face", a, "low"] => handles::Grip::Face(axis(a)?, false),
                    _ => bail!("--hover {word}: arrow-x, ring-z, face-y, face-y-low and the like"),
                });
            }
            if let Some((x, y)) = opts.click {
                // As a click in the window there: the chosen one's handles
                // first, else what the ray meets.
                let (origin, dir) = view.voxel_ray(aspect, x / w, y / h);
                let at = egui::pos2(x, y);
                let picked = view::pick_along(&ed, &figures, origin, dir);
                if stage::grip_under(&ed, &view, screen, at).is_some() {
                    println!("click {x},{y}: a handle of the chosen one");
                } else {
                    ed.choose_item(picked.map(|(item, _)| item));
                    println!("click {x},{y}: {}", picked.map_or("nothing".to_string(), |(item, _)| ed.label_of(item).0));
                }
            }
            // SELECT's rectangle dragged there (points of the shot): what
            // it takes selected, its outline drawn.
            if let Some([x0, y0, x1, y1]) = opts.marquee
                && pass == 0
            {
                let vp = view.view_proj(&view.block, aspect);
                ed.select_rect(vp, glam::Vec2::new(w, h), glam::Vec2::new(x0, y0), glam::Vec2::new(x1, y1), ed.select_mode, ed.select_through);
                view.marquee = Some(view::Marquee { from: [x0 / w, y0 / h], to: [x1 / w, y1 / h], mode: ed.select_mode });
                println!("marquee: {} voxels selected", ed.selection().map_or(0, |s| s.count()));
            }
            if (opts.menu.is_some() || opts.hover.is_some() || opts.click.is_some() || opts.marquee.is_some())
                && let Some(out) = last.take()
            {
                let (device, queue, _) = canvas.gpu();
                painter.skip(device, queue, out);
            }
        }
        if among_things(&ed) {
            (view.spot, view.under) = view.aim_things(&ed, &shown, &figures, aspect, mx, my);
        } else {
            view.hover = view.aim(&ed, aspect, mx, my);
        }
        // BOX or SPHERE: a shape --held big set where the mouse is, fresh
        // (--stamp: and no more fresh, as the next action leaves it).
        if (opts.held.is_some() || opts.stamp) && pass == 0 {
            if !ed.forms() {
                bail!("--held and --stamp set BOX or SPHERE: --tool box or --tool sphere");
            }
            let hit = view.hover.context("--held: nothing under the mouse in the middle of the view")?;
            ed.set_form_at(hit, opts.held.unwrap_or(glam::IVec3::new(5, 5, 3)).max(glam::IVec3::ONE), false);
            if opts.stamp {
                ed.drop_form();
            }
            ed.follow_figure();
            view.follow_origin(&ed);
            shown.update(&ed)?;
            if let Some(out) = last.take() {
                let (device, queue, _) = canvas.gpu();
                painter.skip(device, queue, out);
            }
        }
    }
    canvas.mesh_world(&mut shown.world, &ALL_FACES);
    let frame = view::frame(&ed, &view, &shown, &figures, &cfg.light, aspect);
    canvas.render(&frame);
    if let Some(out) = last {
        let (device, queue, target) = canvas.gpu();
        painter.paint(device, queue, target, &ctx, out, [opts.width, opts.height]);
    }
    let keys = drawn.keys.len();
    let encoder = canvas.gpu().0.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let pixels = canvas.read(encoder)?;
    crate::headless::write_png(&opts.out, opts.width, opts.height, &pixels)?;
    let s = ed.level.size;
    let aim = if among_things(&ed) { format!("spot {:?}, over {:?}", view.spot, view.under.map(|i| &ed.level.things[i].kind)) } else { format!("{:?}", view.hover) };
    println!(
        "{}: {} by {} by {} cells, {} blocks, {} things | {} cubes, {} keys on the panels | aim {}",
        opts.out.display(),
        s.x,
        s.y,
        s.z,
        ed.level.blocks.len(),
        ed.level.things.len(),
        frame.instances.len(),
        keys,
        aim
    );
    // Among the pieces: what is set down, the chosen one first; where the
    // handles and the buttons beside the chosen one lie (to count how far
    // the mouse goes).
    if ed.among_pieces() {
        let screen = ctx.content_rect();
        if let Some((_, _, b)) = stage::handles_on_screen(&ed, &view, screen) {
            println!("handles: {:.0},{:.0} to {:.0},{:.0}", b.left(), b.top(), b.right(), b.bottom());
        }
        if let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("stage beside rect"))).filter(|_| ed.chosen.is_some()) {
            println!("buttons: {:.0},{:.0} to {:.0},{:.0}", r.left(), r.top(), r.right(), r.bottom());
        }
        let lights: usize = ed.level.things.iter().map(|t| t.lights.len()).sum();
        println!("pieces: {} things ({lights} lights), {} places, {} areas, the level {:?} voxels", ed.level.things.len(), ed.level.places.len(), ed.level.areas.len(), ed.level.voxels().to_array());
        if let Some(item) = ed.chosen {
            let t = match item {
                model::Item::Thing(i) => Some(&ed.level.things[i]),
                _ => None,
            };
            let facing = t.and_then(|t| t.facing).map_or(String::new(), |f| format!(", facing {f:?}"));
            let facing = match t {
                Some(t) if t.pitch != 0.0 || t.roll != 0.0 => format!("{facing}, pitch {}, roll {}", t.pitch, t.roll),
                _ => facing,
            };
            let light = t.map_or(String::new(), |t| t.lights.iter().map(|l| format!(", light {l:?}")).collect());
            let light = if t.is_some_and(|t| t.solid) { format!("{light}, solid") } else { light };
            println!("chosen: {} at {:?}{facing}{light}", ed.label_of(item).0, ed.at_of(item).unwrap_or_default());
        }
    }
    // A figure's parts as the list has them, how each hangs and what is
    // wrong with it.
    if let Some(figure) = &ed.figure {
        for line in figure.tree_lines() {
            println!("{line}");
        }
    }
    Ok(())
}

/// The room the panels a frame drew (`drawn`) leave the 3D view in the
/// window (`screen`, egui's points), and as shares of it (left, top,
/// right, bottom: `View::set_room`): among the pieces between the
/// outliner, the top bar and the inspector, else left of the column and
/// above the bar.
pub(crate) fn room_of(drawn: &panels::Shown, screen: egui::Rect) -> (egui::Rect, [f32; 4]) {
    let (w, h) = (screen.width().max(1.0), screen.height().max(1.0));
    match (drawn.free, &drawn.rects[..]) {
        (Some(free), _) => (free, [(free.left() - screen.left()) / w, (free.top() - screen.top()) / h, (screen.right() - free.right()) / w, (screen.bottom() - free.bottom()) / h]),
        // The column on the right and the bar along the bottom, measured
        // by their own sizes as before.
        (None, [right, bottom, ..]) => (egui::Rect::from_min_max(screen.min, egui::pos2(right.left(), bottom.top())), [0.0, 0.0, right.width() / w, bottom.height() / h]),
        (None, _) => (screen, [0.0; 4]),
    }
}

/// How near the mouse must be to the joint's ring to take it (points).
pub(crate) const RING_GRAB: f32 = 9.0;

/// Whether the mouse (egui's point) is on the chosen part's joint's ring
/// (only the ring itself takes the mouse: around it the tools work).
pub(crate) fn on_ring(ed: &Editor, view: &View, room: egui::Rect, mouse: egui::Pos2) -> bool {
    let vp = view.view_proj(&view.block, room.width() / room.height().max(1.0));
    let Some(spot) = ed.joint_spot() else { return false };
    handles::project(vp, glam::Vec2::new(room.width(), room.height()), spot).is_some_and(|(p, _)| p.distance(glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top())) <= RING_GRAB)
}

/// SLICE's arrow under the mouse (egui's point): the grip, the handles,
/// the view's matrix, the mouse in the room's pixels.
fn slice_under(ed: &Editor, view: &View, room: egui::Rect, mouse: egui::Pos2) -> Option<(handles::Grip, handles::Handles, glam::Mat4, glam::Vec2)> {
    let vp = view.view_proj(&view.block, room.width() / room.height().max(1.0));
    let at = glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top());
    let (grip, h) = ed.slice_grip(vp, glam::Vec2::new(room.width(), room.height()), at)?;
    Some((grip, h, vp, at))
}

/// The arrow of the chosen part under the mouse (egui's point): which,
/// the handles, the view's matrix, the mouse in the room's pixels.
fn joint_under(ed: &Editor, view: &View, room: egui::Rect, mouse: egui::Pos2) -> Option<(handles::Grip, handles::Handles, glam::Mat4, glam::Vec2)> {
    let vp = view.view_proj(&view.block, room.width() / room.height().max(1.0));
    let h = ed.part_handles(vp)?;
    let at = glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top());
    let grip = h.grip_at(vp, glam::Vec2::new(room.width(), room.height()), at)?;
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
/// The terrain's points on the screen: the level's camera for the room
/// `room` (egui's points), its size, the mouse `at` within it.
fn land_screen(view: &View, room: egui::Rect, at: egui::Pos2) -> (glam::Mat4, glam::Vec2, glam::Vec2) {
    let vp = view.view_proj(&view.level, room.width() / room.height().max(1.0));
    (vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(at.x - room.left(), at.y - room.top()))
}

fn among_things(ed: &Editor) -> bool {
    ed.scope == Scope::Level && ed.layer == Layer::Things
}

/// The buttons pressed in the view and not yet let go (`View::held`).
pub const HELD_LEFT: u8 = 1;
pub const HELD_RIGHT: u8 = 2;
pub const HELD_MIDDLE: u8 = 4;

/// How long the left button is found up, a drag still open, before the
/// drag ends as let go (its up event lost).
const LOST_RELEASE: Duration = Duration::from_millis(200);

/// Whether the left mouse button is down, as the system has it (not as
/// the events said: one may be lost).
fn left_button_down() -> bool {
    // SAFETY: SDL is running (a window is open); null positions are allowed.
    let buttons = unsafe { sdl3::sys::mouse::SDL_GetGlobalMouseState(std::ptr::null_mut(), std::ptr::null_mut()) };
    buttons.0 & sdl3::sys::mouse::SDL_BUTTON_LMASK.0 != 0
}

/// Opens the editor in a window on the game in `dir` (its `level`, else
/// the one its config names; at `block` if given, or among the level's
/// `things`; or on the `model` file) until it is closed.
pub fn run<G: Game>(dir: &Path, level: Option<&str>, block: Option<&str>, things: bool, model: Option<&str>) -> Result<()> {
    let sdl = sdl3::init().context("initialising SDL")?;
    let video = sdl.video().context("initialising SDL video")?;
    let first = match model {
        Some(file) => EditorWindow::open_model::<G>(&video, dir, file, None)?,
        None => EditorWindow::open::<G>(&video, dir, level, block, things, None)?,
    };
    // The level's window, and a model's for each EDIT MODEL (on its GPU).
    let mut windows = vec![first];
    let mut events = sdl.event_pump().context("SDL events")?;
    loop {
        for w in &mut windows {
            w.draw()?;
        }
        let asked: Vec<(String, Option<context::Seed>)> = windows.iter_mut().filter_map(|w| w.take_model_edit().map(|f| (f, w.take_model_context()))).collect();
        for (file, seed) in asked {
            if let Some(w) = windows.iter_mut().find(|w| w.model_file() == Some(file.as_str())) {
                if let Some(seed) = seed {
                    w.set_context(seed);
                }
                w.raise();
                continue;
            }
            match EditorWindow::open_model::<G>(&video, dir, &file, Some(windows[0].gpu())) {
                Ok(mut w) => {
                    if let Some(seed) = seed {
                        w.set_context(seed);
                    }
                    windows.push(w)
                }
                Err(e) => windows[0].say(format!("{e:#}")),
            }
        }
        // DON'T OPEN: nothing was touched.
        windows.retain(|w| !w.closing());
        if windows.is_empty() {
            return Ok(());
        }
        // Drawn only after something happened (or when its panels ask):
        // the editor sits still otherwise.
        let first = match windows.iter().filter_map(EditorWindow::wakes_at).min() {
            None => Some(events.wait_event()),
            Some(at) => events.wait_event_timeout(at.saturating_duration_since(Instant::now()).max(Duration::from_millis(1))),
        };
        for event in first.into_iter().chain(events.poll_iter().collect::<Vec<_>>()) {
            let shift = shift_held(&events);
            // The left button let go: every other window's drag still open
            // ends too (it may have been begun there and let go here).
            if let sdl3::event::Event::MouseButtonUp { mouse_btn: sdl3::mouse::MouseButton::Left, window_id, .. } = event {
                for w in windows.iter_mut().filter(|w| w.id() != window_id) {
                    w.mouse_up_elsewhere();
                }
            }
            match event.get_window_id() {
                // Each window its own events; one closed goes.
                Some(id) => {
                    if let Some(k) = windows.iter().position(|w| w.id() == id)
                        && !windows[k].handle(&event, shift)
                    {
                        windows.remove(k);
                    }
                }
                None if matches!(event, sdl3::event::Event::Quit { .. }) => return Ok(()),
                None => {}
            }
        }
        if windows.is_empty() {
            return Ok(());
        }
    }
}

/// A key among the pieces (not typed into a field): Escape lets go of what
/// hangs on the mouse, closes ADD MODEL's list or a name being typed, else
/// chooses nothing; the arrows nudge the chosen one across and in depth,
/// as the view looks (Shift: by 10), Page Up and Page Down (and + and -)
/// raise and lower it; T turns it (Shift: the other way), Delete and
/// Backspace remove it, F2 renames it. Whether the key was one of these.
fn pieces_key(ed: &mut Editor, view: &mut View, key: sdl3::keyboard::Keycode, keymod: sdl3::keyboard::Mod) -> bool {
    use sdl3::keyboard::{Keycode, Mod};
    if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD | Mod::LALTMOD | Mod::RALTMOD) {
        return false;
    }
    let by = if keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD) { 10 } else { 1 };
    // Away from the eye and to the right, as the view is turned.
    let yaw = view.level.yaw.to_radians();
    let ahead = if yaw.sin().abs() > yaw.cos().abs() { glam::IVec3::new(yaw.sin().signum() as i32, 0, 0) } else { glam::IVec3::new(0, yaw.cos().signum() as i32, 0) };
    let right = glam::IVec3::new(ahead.y, -ahead.x, 0);
    match key {
        Keycode::Escape => {
            // A menu, a drag (all back as it was), the name typed, what
            // hangs on the mouse, ADD MODEL's pictures, the choice: the
            // nearest goes first.
            if view.menu.is_some() {
                view.menu = None;
            } else if ed.cancel_drag() {
            } else if view.renaming.is_some() {
                view.renaming = None;
            } else if !ed.drop_hanging() {
                if view.picker {
                    view.picker = false;
                } else {
                    ed.choose_item(None);
                }
            }
        }
        Keycode::Up => ed.nudge(ahead * by),
        Keycode::Down => ed.nudge(-ahead * by),
        Keycode::Right => ed.nudge(right * by),
        Keycode::Left => ed.nudge(-right * by),
        Keycode::PageUp | Keycode::Plus | Keycode::Equals | Keycode::KpPlus => ed.nudge(glam::IVec3::Z * by),
        Keycode::PageDown | Keycode::Minus | Keycode::KpMinus => ed.nudge(-glam::IVec3::Z * by),
        Keycode::T => ed.turn_item(if by > 1 { -1 } else { 1 }),
        Keycode::Delete | Keycode::Backspace if ed.hanging.is_none() => ed.remove_item(),
        Keycode::F2 => view.renaming = ed.chosen,
        _ => return false,
    }
    true
}

/// Whether a shift key is down.
pub fn shift_held(events: &sdl3::EventPump) -> bool {
    use sdl3::keyboard::Scancode;
    let keys = events.keyboard_state();
    keys.is_scancode_pressed(Scancode::LShift) || keys.is_scancode_pressed(Scancode::RShift)
}

/// How often an open editor looks whether another program changed its
/// files.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// The editor in a window of its own: `voxcade edit`'s, or one of those the
/// IDE opens (several, on the IDE's GPU). It takes the events of its
/// window and draws when something happened.
pub struct EditorWindow {
    /// Its window, its 3D view's renderer and the panels' painter (none:
    /// a window without a window, for tests that feed its events).
    win: Option<Win>,
    /// The size a window without a window has (its points and pixels).
    headless: [u32; 2],
    ed: Editor,
    view: View,
    shown: Shown,
    figures: Figures,
    cfg: crate::config::GameConfig,
    /// The panels (egui, as the IDE): their input, painter, pictures, the
    /// keys they named and where they lay, last frame.
    gui: egui::Context,
    input: crate::gui::input::Input,
    thumbs: panels::Thumbs,
    keys: Vec<(char, view::Action)>,
    rects: Vec<egui::Rect>,
    /// Where the parts' rows lay, last frame (for tests that click them).
    #[cfg(test)]
    rows: Vec<(String, egui::Rect)>,
    /// What a window without a window would have drawn: how often the
    /// world shown was built anew whole (every chunk meshed again), the
    /// last frame and whether it built the world anew (for tests that
    /// count and time what a window does).
    #[cfg(test)]
    rebuilds: usize,
    #[cfg(test)]
    last: Option<(crate::render::FrameData, bool)>,
    /// Steps there were when the last left click came: a double click
    /// takes back what its first click did.
    steps_at_click: usize,
    /// Something happened since the last frame, and when the panels want
    /// to be drawn again (None: only after an event).
    dirty: bool,
    repaint_at: Option<Instant>,
    /// Another session driving the IDE (`ide::remote`): where its pointer
    /// is (the window's points; an arrow is drawn there), a picture of the
    /// window asked for (`Some(late)`: also if the panels are not settled)
    /// and the one drawn.
    marker: Option<(f32, f32)>,
    capture: Option<bool>,
    picture: Option<Result<Vec<u8>, String>>,
    /// DON'T OPEN was answered: the window closes at its next event.
    closing: bool,
    /// When it last looked whether another program changed its files
    /// (`Editor::look_at_disk`: once a second, as the IDE's list).
    looked: Instant,
}

/// An editor window's own: the window, the 3D view's renderer, the
/// panels' painter.
struct Win {
    screen: crate::window::Screen,
    renderer: crate::render::Renderer,
    painter: crate::gui::Painter,
}

impl EditorWindow {
    /// A window on the level of the game in `dir` (at `block` if given, or
    /// among its `things`), drawn with `gpu` if given.
    pub fn open<G: Game>(video: &sdl3::VideoSubsystem, dir: &Path, level: Option<&str>, block: Option<&str>, things: bool, gpu: Option<&crate::window::Gpu>) -> Result<Self> {
        let (ed, view, cfg) = open::<G>(dir, level, block, things)?;
        // A game of several levels says which.
        let title = match level.and_then(|l| Path::new(l).file_stem()) {
            Some(name) => format!("Level {} - {}", name.to_string_lossy(), cfg.title),
            None => format!("Level - {}", cfg.title),
        };
        Self::with(video, &title, ed, view, cfg, dir, gpu)
    }

    /// A window on the model `file` of the game in `dir`.
    pub fn open_model<G: Game>(video: &sdl3::VideoSubsystem, dir: &Path, file: &str, gpu: Option<&crate::window::Gpu>) -> Result<Self> {
        let (ed, view, cfg) = open_model::<G>(dir, file)?;
        let title = format!("{file} - {}", cfg.title);
        Self::with(video, &title, ed, view, cfg, dir, gpu)
    }

    fn with(video: &sdl3::VideoSubsystem, title: &str, ed: Editor, mut view: View, cfg: crate::config::GameConfig, dir: &Path, gpu: Option<&crate::window::Gpu>) -> Result<Self> {
        // The person's BACKGROUND for each editor (not the game's).
        view.backgrounds = background::Backgrounds::load(&background::Backgrounds::path()).colours();
        let screen = crate::window::Screen::open_on(video, title, 1600, 900, false, gpu)?;
        // SDL sends typed text only to a window that asked for it: without
        // this no field of the editor (a name, a number) takes a key from a
        // real keyboard (the owner: "alle textfelder erlauben keinen input").
        video.text_input().start(&screen.window);
        let mut renderer = crate::render::Renderer::new(&screen.device, screen.config.format, screen.config.width, screen.config.height);
        let mut shown = Shown::new(&ed)?;
        renderer.rebuild_all(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES);
        let figures = Figures::load(dir, &cfg).with_pieces(dir, &ed.level);
        let gui = egui::Context::default();
        crate::gui::style(&gui);
        let painter = crate::gui::Painter::new(&screen.device, screen.config.format, true);
        Ok(Self::of(Some(Win { screen, renderer, painter }), ed, view, cfg, shown, figures, gui))
    }

    /// An editor window without a window (`win` none): its events fed and
    /// its frames run as a window's, nothing drawn; for tests.
    #[cfg(test)]
    pub fn headless(ed: Editor, view: View, cfg: crate::config::GameConfig, dir: &Path) -> Result<Self> {
        let shown = Shown::new(&ed)?;
        let figures = Figures::load(dir, &cfg).with_pieces(dir, &ed.level);
        let gui = egui::Context::default();
        crate::gui::style(&gui);
        Ok(Self::of(None, ed, view, cfg, shown, figures, gui))
    }

    #[allow(clippy::too_many_arguments)]
    fn of(win: Option<Win>, ed: Editor, view: View, cfg: crate::config::GameConfig, shown: Shown, figures: Figures, gui: egui::Context) -> Self {
        Self {
            win,
            headless: [1600, 900],
            ed,
            view,
            shown,
            figures,
            cfg,
            gui,
            input: crate::gui::input::Input::default(),
            thumbs: panels::Thumbs::default(),
            keys: Vec::new(),
            rects: Vec::new(),
            #[cfg(test)]
            rows: Vec::new(),
            #[cfg(test)]
            rebuilds: 0,
            #[cfg(test)]
            last: None,
            steps_at_click: 0,
            dirty: true,
            repaint_at: None,
            marker: None,
            capture: None,
            picture: None,
            closing: false,
            looked: Instant::now(),
        }
    }

    /// Its size in points (as the mouse gives them) and in pixels.
    fn sizes(&self) -> ((f32, f32), [u32; 2]) {
        match &self.win {
            Some(w) => {
                let (pw, ph) = w.screen.window.size();
                ((pw as f32, ph as f32), [w.screen.config.width, w.screen.config.height])
            }
            None => ((self.headless[0] as f32, self.headless[1] as f32), self.headless),
        }
    }

    /// Whether it is to close (DON'T OPEN was answered).
    pub fn closing(&self) -> bool {
        self.closing
    }

    /// The left button let go where this window did not hear it (outside
    /// it, over another window): a drag still open ends there as let go,
    /// one step.
    pub fn mouse_up_elsewhere(&mut self) {
        self.view.up_since = None;
        self.view.press_at = None;
        self.view.held = 0;
        self.view.marquee = None;
        // The panels let go too (egui would hold the button down for good).
        self.input.release_buttons();
        if self.ed.drag_open() {
            self.ed.end_drag(false);
            self.ed.end_whole_stroke();
            self.view.follow_origin(&self.ed);
            self.dirty = true;
        }
    }

    /// When it wants to draw next (None: only after an event of its own).
    pub fn wakes_at(&self) -> Option<Instant> {
        // A drag open with the button found up: soon, to end it.
        if let Some(since) = self.view.up_since {
            return Some(since + LOST_RELEASE);
        }
        let look = self.looked + LOOK_EVERY;
        if self.dirty { Some(Instant::now()) } else { Some(self.repaint_at.map_or(look, |r| r.min(look))) }
    }

    /// Its window's SDL id: the events that are its.
    pub fn id(&self) -> u32 {
        self.win.as_ref().map_or(0, |w| w.screen.window.id())
    }

    /// Its title, size in pixels, pixels to one of SDL's points (the
    /// mouse's) and whether it has the keyboard.
    pub fn info(&self) -> (String, u32, u32, f32, bool) {
        match &self.win {
            Some(w) => (w.screen.window.title().to_string(), w.screen.config.width, w.screen.config.height, w.screen.window.pixel_density(), w.screen.window.has_input_focus()),
            None => (String::new(), self.headless[0], self.headless[1], 1.0, true),
        }
    }

    /// Where a synthetic pointer is (SDL's points), if it is.
    pub fn set_marker(&mut self, at: Option<(f32, f32)>) {
        if self.marker != at {
            self.marker = at;
            self.dirty = true;
        }
    }

    /// A picture of the window is wanted of the next frame drawn (`late`:
    /// even if the panels are still moving).
    pub fn want_picture(&mut self, late: bool) {
        if self.capture != Some(late) {
            self.capture = Some(late);
            self.dirty = true;
        }
    }

    /// The picture asked for, once drawn (PNG bytes), or why not.
    pub fn take_picture(&mut self) -> Option<Result<Vec<u8>, String>> {
        self.picture.take()
    }

    /// The model EDIT MODEL asked to open (its path in the game folder),
    /// once.
    pub fn take_model_edit(&mut self) -> Option<String> {
        self.ed.wants_model.take()
    }

    /// The level round the model EDIT asked to open, for a piece: to shape
    /// it in place (`context.rs`), once.
    pub fn take_model_context(&mut self) -> Option<context::Seed> {
        self.ed.wants_context.take()
    }

    /// Opens the level round its model (EDIT on a piece of it): said over
    /// the panels where it cannot.
    pub fn set_context(&mut self, seed: context::Seed) {
        if let Err(e) = self.ed.set_context(seed) {
            self.ed.message = Some(format!("{e:#}"));
        }
        self.ed.remember_held();
        self.dirty = true;
    }

    /// The GPU it draws with, to open another window on.
    pub fn gpu(&self) -> &crate::window::Gpu {
        self.win.as_ref().expect("a window").screen.gpu()
    }

    /// The model file it shapes, if it shapes one.
    pub fn model_file(&self) -> Option<&str> {
        self.ed.model.as_ref().map(|_| self.ed.file.as_str())
    }

    /// Says a mistake over its panels (one of another window, say).
    pub fn say(&mut self, text: String) {
        self.ed.message = Some(text);
        self.dirty = true;
    }

    /// Brings the window to the front.
    pub fn raise(&mut self) {
        if let Some(w) = &mut self.win {
            w.screen.window.raise();
        }
    }

    /// Opens the level's block `name` to shape it (nothing if the level
    /// has no such block).
    pub fn open_block(&mut self, name: &str) {
        if let Some(i) = self.ed.level.blocks.iter().position(|b| b.name == name) {
            self.ed.hold(i);
            self.ed.open_block(i);
            self.dirty = true;
        }
    }

    /// Takes in an event of its window (`shift`: a shift key is down);
    /// false when the window is to close.
    pub fn handle(&mut self, event: &sdl3::event::Event, shift: bool) -> bool {
        use sdl3::event::{Event, WindowEvent};
        use sdl3::keyboard::{Keycode, Mod};
        use sdl3::mouse::MouseButton;

        if self.closing {
            return false;
        }
        self.input.handle(event, || None);
        // While the question is asked only its buttons answer (the panels
        // take the clicks); the level behind is not touched.
        if self.ed.question.is_some() {
            use sdl3::event::{Event, WindowEvent};
            self.dirty = true;
            match *event {
                Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return false,
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => {
                    if let Some(win) = &mut self.win {
                        win.screen.resize(&mut win.renderer, w, h);
                    }
                }
                _ => {}
            }
            return true;
        }
        let ((pw, ph), [wide, high]) = self.sizes();
        let Self { win, ed, view, shown, figures, cfg, keys, rects, gui, input, steps_at_click, .. } = self;
        self.dirty = true;
        let aspect = wide as f32 / high.max(1) as f32;
        let share = |x: f32, y: f32| (x / pw.max(1.0), y / ph.max(1.0));
        // The mouse over the panels (or a list or picker of theirs open
        // there) is theirs, not the 3D view's.
        // (By where the mouse is now, not where egui last saw it: a click
        // whose motion, press and release come at once is the view's.)
        let over = |x: f32, y: f32| {
            let p = input.point(x, y);
            rects.iter().any(|r| r.contains(p)) || gui.egui_is_using_pointer() || gui.layer_id_at(p).is_some_and(|l| l.order != egui::Order::Background)
        };
        match *event {
            Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return false,
            Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => {
                if let Some(win) = win {
                    win.screen.resize(&mut win.renderer, w, h);
                }
            }
            // Ctrl down or up: BOX and SPHERE cut away while it is held.
            Event::KeyDown { keycode: Some(Keycode::LCtrl | Keycode::RCtrl), .. } => view.cut = true,
            Event::KeyUp { keycode: Some(Keycode::LCtrl | Keycode::RCtrl), .. } => view.cut = false,
            // Alt: they fill only where it is empty.
            Event::KeyDown { keycode: Some(Keycode::LAlt | Keycode::RAlt), .. } => view.only_empty = true,
            Event::KeyUp { keycode: Some(Keycode::LAlt | Keycode::RAlt), .. } => view.only_empty = false,
            // Shift down: the tool is PICK until it is up (`shift` still
            // says so when the other shift key is down). Losing the focus
            // with it held would never see it let go.
            Event::KeyDown { keycode: Some(Keycode::LShift | Keycode::RShift), .. } => ed.shift_pick(true),
            Event::KeyUp { keycode: Some(Keycode::LShift | Keycode::RShift), .. } => ed.shift_pick(shift),
            // The focus lost (another window, the desktop), the window
            // hidden: a drag still open is cancelled, all as before it.
            Event::Window { win_event: WindowEvent::FocusLost | WindowEvent::Hidden | WindowEvent::Minimized, .. } => {
                view.cut = false;
                view.only_empty = false;
                ed.shift_pick(false);
                if ed.drag_open() {
                    ed.end_drag(true);
                    ed.end_whole_stroke();
                }
                view.press_at = None;
                view.held = 0;
                view.marquee = None;
            }
            // Another button while the left one drags: cancelled.
            Event::MouseButtonDown { mouse_btn: MouseButton::Right | MouseButton::Middle, .. } if ed.drag_open() => {
                ed.end_drag(true);
                ed.end_whole_stroke();
                view.press_at = None;
            }
            // Back to the window: its files as they are now (another
            // program's change comes in); at the level, the pieces' models
            // too (one changed in the model editor shows).
            Event::Window { win_event: WindowEvent::FocusGained, .. } => {
                ed.look_at_disk();
                if ed.scope == Scope::Level {
                    *figures = Figures::load(&ed.dir, cfg).with_pieces(&ed.dir, &ed.level);
                }
            }
            // F1: the page of the keys; Esc closes it first.
            Event::KeyDown { keycode: Some(Keycode::F1), .. } if !gui.egui_wants_keyboard_input() => view::act(ed, view, view::Action::Keys),
            Event::KeyDown { keycode: Some(Keycode::Escape), .. } if view.keys_shown => view.keys_shown = false,
            Event::KeyDown { keycode: Some(key), keymod, .. } if ed.among_pieces() && !gui.egui_wants_keyboard_input() && pieces_key(ed, view, key, keymod) => {}
            Event::KeyDown { keycode: Some(key), keymod, .. } if !gui.egui_wants_keyboard_input() => {
                let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD);
                let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                match key {
                    // BOX and SPHERE: the shape a voxel deeper or shallower.
                    Keycode::Plus | Keycode::Equals | Keycode::KpPlus if ed.forms() => ed.deepen(1),
                    Keycode::Minus | Keycode::KpMinus if ed.forms() => ed.deepen(-1),
                    Keycode::Escape if ed.forms() && ed.drop_form() => {}
                    // SELECT's rectangle being dragged: let go of, nothing.
                    Keycode::Escape if view.marquee.is_some() => view.marquee = None,
                    // Among the terrain's points: none chosen (a drag going
                    // on is cancelled first, below).
                    Keycode::Escape if ed.among_land() && !ed.stroke_open() && (ed.land.chosen.is_some() || !ed.land.selected.is_empty()) => {
                        ed.land.chosen = None;
                        ed.land.selected.clear();
                    }
                    // A drag going on: cancelled, all as before it.
                    Keycode::Escape if ed.joint_drag().is_some() || ed.gripped().is_some() || ed.dragging().is_some() || ed.stroke_open() => {
                        ed.end_drag(true);
                        ed.end_whole_stroke();
                    }
                    // The selection: none selected; its voxels gone; copied,
                    // pasted.
                    Keycode::Escape if ed.joint_drag().is_none() && view.menu.is_none() && ed.selection().is_some() => ed.clear_selection(),
                    Keycode::Delete | Keycode::Backspace if ed.selection().is_some() => view::act(ed, view, view::Action::SelRemove),
                    Keycode::C if ctrl && ed.selection().is_some() => view::act(ed, view, view::Action::SelCopy),
                    Keycode::V if ctrl && ed.clipboard.is_some() => view::act(ed, view, view::Action::SelPaste),
                    Keycode::Z if ctrl && shift => ed.redo(),
                    // The chosen part: its name to type, removed.
                    Keycode::F2 if ed.part_mode().is_some() => view::act(ed, view, view::Action::RenamePart(ed.figure.as_ref().and_then(|f| f.selected).unwrap_or(usize::MAX))),
                    Keycode::Delete if ed.part_mode().is_some() => {
                        if let Some(p) = ed.figure.as_ref().and_then(|f| f.selected).filter(|&p| ed.cannot_remove(p).is_none()) {
                            view::act(ed, view, view::Action::RemovePart(p));
                        }
                    }
                    Keycode::Z if ctrl => ed.undo(),
                    Keycode::Y if ctrl => ed.redo(),
                    // A model's frames, one by one.
                    // SLICE's layer, a voxel at a time.
                    Keycode::PageUp if ed.slice.is_some() => {
                        let at = ed.slice.as_ref().map_or(0, |s| s.at);
                        ed.set_slice_at(at + 1);
                    }
                    Keycode::PageDown if ed.slice.is_some() => {
                        let at = ed.slice.as_ref().map_or(0, |s| s.at);
                        ed.set_slice_at(at - 1);
                    }
                    Keycode::Left => ed.step_frame(-1),
                    Keycode::Right => ed.step_frame(1),
                    Keycode::Escape => {
                        if view.menu.is_some() {
                            view.menu = None;
                        } else if ed.cancel_joint() {
                        } else if view.choosing {
                            view.choosing = false;
                        } else if ed.tool != Tool::Pointer && Tool::of(ed.scope, ed.layer).contains(&Tool::Pointer) {
                            // Back to POINTER, which changes nothing.
                            ed.choose(Tool::Pointer);
                        } else {
                            view::act(ed, view, view::Action::Back);
                        }
                    }
                    // A key the panels name (a word's, a colour's digit;
                    // by what the key says).
                    _ if !ctrl => {
                        let name = key.name();
                        let mut chars = name.chars();
                        if let (Some(c), None) = (chars.next(), chars.next())
                            && c.is_ascii_alphanumeric()
                            && let Some(action) = panels::key_action(keys, c)
                        {
                            view::act(ed, view, action);
                        }
                    }
                    _ => {}
                }
            }
            Event::MouseMotion { mousestate, x, y, xrel, yrel, .. } => {
                // A button counts as held only if it went down in this
                // window's view and was not let go since (as the window
                // heard it): a motion carrying a button held from elsewhere
                // (pressed on a panel, in another window, a press whose
                // release was lost) drags nothing and turns nothing.
                let held = |b: u8| view.held & b != 0;
                let left = mousestate.left() && held(HELD_LEFT);
                let right = mousestate.right() && held(HELD_RIGHT);
                let middle = mousestate.middle() && held(HELD_MIDDLE);
                let (sx, sy) = share(x, y);
                let (dx, dy) = share(xrel, yrel);
                if middle || (right && shift) {
                    view.orbit(ed.scope).pan(dx, dy, aspect);
                    view.leave_game_view(ed.scope);
                } else if right {
                    view.orbit(ed.scope).turn(dx, dy);
                    view.leave_game_view(ed.scope);
                }
                // The right button dragged: no click, no menu.
                if let Some((px, py)) = view.right_press
                    && (px - x).abs() + (py - y).abs() > 5.0
                {
                    view.right_press = None;
                }
                let over = over(x, y) && !left;
                // SELECT's rectangle follows the pointer.
                if let Some(m) = &mut view.marquee
                    && left
                {
                    let (sx, sy) = share(x, y);
                    m.to = [sx, sy];
                }
                // Gone a few pixels from the press: a drag now.
                if let Some((px, py)) = view.press_at
                    && (px - x).hypot(py - y) >= handles::TRAVEL
                {
                    view.press_at = None;
                }
                if !ed.among_pieces() {
                    let free = !(over || left);
                    view.slice_hover = free && slice_under(ed, view, gui.content_rect(), input.point(x, y)).is_some();
                    view.ring_hover = free && !view.slice_hover && on_ring(ed, view, gui.content_rect(), input.point(x, y));
                    view.grip_hover = if !free || view.ring_hover || view.slice_hover { None } else { joint_under(ed, view, gui.content_rect(), input.point(x, y)).map(|(g, ..)| g) };
                }
                if ed.among_pieces() {
                    let (origin, dir) = view.voxel_ray(aspect, sx, sy);
                    let room = gui.content_rect();
                    let mouse = input.point(x, y);
                    if left && ed.gripped().is_some() {
                        let vp = view.view_proj(&view.level, room.width() / room.height().max(1.0));
                        ed.drag_grip(vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), (origin, dir));
                    } else if left && ed.dragging().is_some() && view.press_at.is_none() {
                        ed.drag_item(origin, dir);
                    } else if ed.hanging.is_some() {
                        ed.hang_at(if over { None } else { view::set_down_spot(ed, figures, &shown.world, origin, dir) });
                    }
                    let busy = over || ed.hanging.is_some() || left;
                    view.grip_hover = if busy { None } else { stage::grip_under(ed, view, room, mouse) };
                    view.over = if busy || view.grip_hover.is_some() { None } else { view::item_along(ed, figures, origin, dir) };
                } else if ed.among_land() {
                    // The point under the mouse lit; a press held drags it
                    // (POINTER) or brushes over the points (the others).
                    let (vp, size, m) = land_screen(view, gui.content_rect(), input.point(x, y));
                    let p = if over { None } else { ed.point_at(vp, size, m) };
                    ed.land.hover = p;
                    ed.land.near = if over { None } else { ed.nearest_point(vp, size, m) };
                    if left && ed.land_dragging() && view.press_at.is_none() {
                        ed.land_drag(p, view.voxel_ray(aspect, sx, sy), shift);
                    }
                } else if among_things(ed) {
                    (view.spot, view.under) = if over { (None, None) } else { view.aim_things(ed, shown, figures, aspect, sx, sy) };
                    if left {
                        ed.drag_thing(view.spot);
                    }
                } else if ed.slicing() && left {
                    // SLICE's arrow dragged.
                    let room = gui.content_rect();
                    let mouse = input.point(x, y);
                    let vp = view.view_proj(&view.block, room.width() / room.height().max(1.0));
                    let ray = view.ray(ed, aspect, sx, sy);
                    ed.drag_slice(vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), ray);
                } else if ed.joint_drag().is_some() && left {
                    // A joint's arrow dragged.
                    let room = gui.content_rect();
                    let mouse = input.point(x, y);
                    let vp = view.view_proj(&view.block, room.width() / room.height().max(1.0));
                    let ray = view.ray(ed, aspect, sx, sy);
                    // (Shift: finer, a tenth of a voxel, a degree.)
                    ed.set_fine(shift);
                    ed.sel_keep = view.only_empty;
                    ed.drag_joint(vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), ray);
                    // (A part moved moves the grid shown: the view keeps the
                    // figure still at once, for the next motion's ray.)
                    view.follow_origin(ed);
                } else if ed.forms() {
                    // BOX and SPHERE: the footprint dragged out on its face's
                    // plane.
                    view.hover = if over { None } else { view.aim(ed, aspect, sx, sy) };
                    if left && ed.drawing.is_some() {
                        let (origin, dir) = view.ray(ed, aspect, sx, sy);
                        ed.draw_form(origin, dir);
                    }
                } else {
                    view.hover = if over { None } else { view.aim(ed, aspect, sx, sy) };
                    if left
                        && let Some(hit) = view.hover
                    {
                        ed.drag(hit);
                    }
                }
            }
            // A figure in GAME VIEW is only looked at.
            Event::MouseButtonDown { mouse_btn: MouseButton::Left, .. } if view.before_game.is_some() && ed.scope != Scope::Level => {}
            Event::MouseButtonDown { mouse_btn: MouseButton::Left, clicks, x, y, which, .. } if !over(x, y) => {
                view.held |= HELD_LEFT;
                // Where the press was (a drag moves things only once the
                // pointer has gone a few pixels), and whether a program's
                // pointer made it (`ide::remote`: no real button to ask).
                view.press_at = Some((x, y));
                view.synthetic_press = which == crate::ide::remote::SYNTHETIC;
                let (sx, sy) = share(x, y);
                let sliced = if ed.among_pieces() { None } else { slice_under(ed, view, gui.content_rect(), input.point(x, y)) };
                let ring = sliced.is_none() && !ed.among_pieces() && on_ring(ed, view, gui.content_rect(), input.point(x, y));
                let joint = if ring || sliced.is_some() { None } else { joint_under(ed, view, gui.content_rect(), input.point(x, y)) };
                if !ed.among_pieces() {
                    view.menu = None;
                }
                if let Some((grip, h, vp, mouse)) = sliced {
                    // SLICE's arrow: dragged until let go.
                    let room = gui.content_rect();
                    if let Some(drag) = handles::Drag::begin(&h, grip, vp, glam::Vec2::new(room.width(), room.height()), mouse, view.ray(ed, aspect, sx, sy)) {
                        ed.grab_slice(drag, h);
                    }
                } else if ring {
                    // The joint's ring: dragged across the view.
                    let look = crate::camera::view_dir(&view.block.camera());
                    ed.grab_joint(view.ray(ed, aspect, sx, sy), look);
                } else if let Some((grip, h, vp, mouse)) = joint {
                    // The part's arrow: dragged until let go.
                    let room = gui.content_rect();
                    if let Some(drag) = handles::Drag::begin(&h, grip, vp, glam::Vec2::new(room.width(), room.height()), mouse, view.ray(ed, aspect, sx, sy)) {
                        ed.grab_part(drag, h);
                    }
                } else if ed.tool == Tool::Select && ed.scope != Scope::Level {
                    // SELECT: a rectangle on the screen from here (on empty
                    // space too), its mode the lit one or Ctrl (ADD), Alt
                    // (REMOVE); a double click the color connected.
                    if clicks >= 2 {
                        view.marquee = None;
                        let (origin, dir) = view.ray(ed, aspect, sx, sy);
                        let add = ed.select_mode == model::selection::SelectMode::Add || view.cut;
                        match ed.scope {
                            Scope::Figure => {
                                if let Some((hit, p)) = ed.aim_whole(origin, dir) {
                                    ed.select_connected(hit.cell, Some(p), add);
                                }
                            }
                            _ => {
                                if let Some(hit) = view.aim(ed, aspect, sx, sy).filter(|h| h.cell.z >= 0) {
                                    ed.select_connected(hit.cell, None, add);
                                }
                            }
                        }
                    } else {
                        let mode = if view.cut { model::selection::SelectMode::Add } else if view.only_empty { model::selection::SelectMode::Remove } else { ed.select_mode };
                        view.marquee = Some(view::Marquee { from: [sx, sy], to: [sx, sy], mode });
                    }
                } else if ed.scope == Scope::Figure {
                    // The figure whole: the tool works on the part it meets
                    // (the figure stays chosen, nothing dimmed); a double
                    // click chooses that part, the first click's work taken
                    // back. A click never both chooses and paints.
                    let (origin, dir) = view.ray(ed, aspect, sx, sy);
                    if ed.tool == Tool::Pointer {
                        // POINTER: the part clicked chosen, nothing changed.
                        if let (Some(figure), Some((lo, _))) = (&ed.figure, ed.figure_box())
                            && let Some(p) = figure.part_at(lo, origin, dir)
                        {
                            view::act(ed, view, view::Action::Part(p));
                        }
                    } else if let (Some(figure), Some((lo, _))) = (&ed.figure, ed.figure_box())
                        && let Some(p) = figure.part_at(lo, origin, dir)
                    {
                        if clicks >= 2 {
                            if ed.steps() > *steps_at_click {
                                ed.take_back();
                            }
                            view::act(ed, view, view::Action::Part(p));
                        } else {
                            *steps_at_click = ed.steps();
                            ed.begin_whole_stroke(p);
                            // (The grid shown is the part's now: the view
                            // kept still, the ray taken anew.)
                            view.follow_origin(ed);
                            if let Some(hit) = view.aim(ed, aspect, sx, sy) {
                                if ed.tool == Tool::Fill {
                                    ed.fill_at(hit, view.cut);
                                } else if ed.forms() {
                                    ed.begin_form(hit, view.cut);
                                } else {
                                    ed.press(hit);
                                }
                            }
                        }
                    }
                } else if ed.among_land() {
                    // SELECT: a rectangle on the screen; else the point under
                    // the mouse chosen and grabbed, or brushed.
                    if ed.land.tool == model::land::LandTool::Select {
                        let mode = if view.cut { model::selection::SelectMode::Add } else if view.only_empty { model::selection::SelectMode::Remove } else { ed.land.mode };
                        view.marquee = Some(view::Marquee { from: [sx, sy], to: [sx, sy], mode });
                    } else {
                        let (vp, size, m) = land_screen(view, gui.content_rect(), input.point(x, y));
                        let p = ed.point_at(vp, size, m);
                        ed.land_press(p, view.voxel_ray(aspect, sx, sy));
                    }
                } else if ed.among_pieces() {
                    // What hangs on the mouse set down; a handle of the
                    // chosen one grabbed; else the item under it chosen, to
                    // be dragged (nothing: none chosen, the level shown); a
                    // double click opens a piece's model.
                    let (origin, dir) = view.voxel_ray(aspect, sx, sy);
                    let room = gui.content_rect();
                    let mouse = input.point(x, y);
                    view.menu = None;
                    let picked = view::pick_along(ed, figures, origin, dir);
                    if ed.hanging.is_some() {
                        // A model set down stays on the mouse for the next
                        // click (a stamp: trees by the dozen) until Escape;
                        // a double click's second click sets none.
                        if clicks < 2 {
                            ed.hang_at(view::set_down_spot(ed, figures, &shown.world, origin, dir));
                            ed.stamp();
                        }
                    } else if clicks >= 2 {
                        if let Some((item @ Item::Thing(i), _)) = picked
                            && ed.level.things[i].model.is_some()
                        {
                            ed.choose_item(Some(item));
                            ed.edit_model();
                        }
                    } else if let (Some(grip), Some((h, vp, _))) = (stage::grip_under(ed, view, room, mouse), stage::handles_on_screen(ed, view, room)) {
                        ed.grab(grip, h, vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), (origin, dir));
                    } else {
                        ed.press_item(picked.map(|(item, _)| item), origin, dir);
                        ed.choose_light(picked.and_then(|(_, light)| light));
                        (view.renaming, view.level_settings) = (None, false);
                    }
                } else if among_things(ed) {
                    // A second click of a double click would put down or
                    // take away a second thing at once.
                    if clicks < 2 {
                        (view.spot, view.under) = view.aim_things(ed, shown, figures, aspect, sx, sy);
                        ed.press_thing(view.spot, view.under);
                    }
                } else if clicks >= 2 && ed.scope == Scope::Level {
                    // A double click opens the block clicked on.
                    if ed.steps() > *steps_at_click {
                        ed.take_back();
                    }
                    let block = view.aim(ed, aspect, sx, sy).map(|hit| ed.level.cell(hit.cell).block).unwrap_or(0);
                    if block != 0 {
                        ed.hold(block as usize - 1);
                        ed.open_block(block as usize - 1);
                    }
                } else if ed.tool == Tool::Pointer && ed.scope != Scope::Level {
                    // POINTER: in a figure's part, a click on another part
                    // (seen through) chooses it, on empty space the figure
                    // whole; nothing is changed, nothing dragged.
                    if ed.part_mode().is_some() {
                        let (origin, dir) = view.ray(ed, aspect, sx, sy);
                        let (origin, dir) = (view.rest.transform_point3(origin), view.rest.transform_vector3(dir));
                        let met = ed.figure.as_ref().zip(ed.figure_box()).and_then(|(f, (lo, _))| f.part_at(lo, origin, dir));
                        let chosen = ed.figure.as_ref().and_then(|f| f.selected);
                        match met {
                            Some(p) if Some(p) != chosen => view::act(ed, view, view::Action::Part(p)),
                            Some(_) => {}
                            None => view::act(ed, view, view::Action::WholeFigure),
                        }
                    }
                } else if let Some(hit) = view.aim(ed, aspect, sx, sy) {
                    *steps_at_click = ed.steps();
                    if ed.tool == Tool::Fill {
                        ed.fill_at(hit, view.cut);
                    } else if ed.forms() {
                        ed.begin_form(hit, view.cut);
                    } else {
                        ed.press(hit);
                    }
                } else if ed.selection().is_some() {
                    // A click on empty space: none selected.
                    ed.clear_selection();
                } else if ed.part_mode().is_some() {
                    // A part shaped, a click on empty space: the figure
                    // whole again.
                    view::act(ed, view, view::Action::WholeFigure);
                }
            }
            // BOX and SPHERE: the footprint let go is set right where it
            // was drawn (Alt: only where empty; Ctrl, held as it was
            // pressed: cut away), fresh for + and - until the next action.
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } if ed.slicing() => {
                view.held &= !HELD_LEFT;
                ed.let_go_of_slice();
            }
            // SELECT's rectangle let go: what it holds selected.
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, x, y, .. } if view.marquee.is_some() => {
                view.held &= !HELD_LEFT;
                if let Some(mut m) = view.marquee.take() {
                    let (sx, sy) = share(x, y);
                    m.to = [sx, sy];
                    let size = glam::Vec2::new(pw, ph);
                    let at = |s: [f32; 2]| glam::Vec2::new(s[0] * pw, s[1] * ph);
                    if ed.among_land() {
                        ed.select_points(view.view_proj(&view.level, aspect), size, at(m.from), at(m.to), m.mode);
                    } else {
                        let vp = view.view_proj(&view.block, aspect);
                        ed.select_rect(vp, size, at(m.from), at(m.to), m.mode, ed.select_through);
                    }
                }
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } if ed.forms() && ed.joint_drag().is_none() => {
                view.held &= !HELD_LEFT;
                if ed.drawing.is_some() {
                    ed.set_drawn(view.only_empty);
                }
                ed.end_whole_stroke();
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } => {
                view.held &= !HELD_LEFT;
                ed.release();
                ed.let_go_of_selection();
                ed.end_whole_stroke();
                view.press_at = None;
            }
            // A right click (not a drag) among the pieces: the menu of what
            // is under the mouse (chosen), or of the floor there.
            Event::MouseButtonDown { mouse_btn: MouseButton::Right, x, y, .. } => {
                view.right_press = (!over(x, y)).then_some((x, y));
                if !over(x, y) {
                    view.held |= HELD_RIGHT;
                }
            }
            Event::MouseButtonDown { mouse_btn: MouseButton::Middle, x, y, .. } if !over(x, y) => view.held |= HELD_MIDDLE,
            Event::MouseButtonUp { mouse_btn: MouseButton::Middle, .. } => view.held &= !HELD_MIDDLE,
            Event::MouseButtonUp { mouse_btn: MouseButton::Right, .. } if ed.among_land() => {
                view.held &= !HELD_RIGHT;
                view.right_press = None;
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Right, x, y, .. } if !ed.among_pieces() => {
                view.held &= !HELD_RIGHT;
                // The voxel editor and the level of blocks: the menu of the
                // voxel (the part) or the block under the mouse.
                if view.right_press.take().is_some() {
                    let (sx, sy) = share(x, y);
                    let at = input.point(x, y);
                    if let Some(on) = voxel_menu(ed, view, aspect, sx, sy) {
                        if let view::MenuOn::Block(i) = on {
                            ed.hold(i);
                        }
                        view.menu = Some(view::Menu { on, at: [at.x, at.y] });
                    }
                }
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Right, x, y, .. } => {
                view.held &= !HELD_RIGHT;
                if view.right_press.take().is_some() && ed.among_pieces() && ed.hanging.is_none() {
                    let (sx, sy) = share(x, y);
                    let (origin, dir) = view.voxel_ray(aspect, sx, sy);
                    let at = input.point(x, y);
                    let room = gui.content_rect();
                    // On the chosen one's handles (or within them, where
                    // the ray meets nothing): its menu.
                    let on_handles = stage::grip_under(ed, view, room, at).is_some() || stage::handles_on_screen(ed, view, room).is_some_and(|(_, _, b)| b.contains(at));
                    match (view::pick_along(ed, figures, origin, dir), ed.chosen) {
                        (_, Some(item)) if stage::grip_under(ed, view, room, at).is_some() => view::act(ed, view, view::Action::OpenMenu(item, [at.x, at.y])),
                        (Some((item, light)), _) => {
                            let light = if ed.chosen == Some(item) { ed.chosen_light.or(light) } else { light };
                            view::act(ed, view, view::Action::OpenMenu(item, [at.x, at.y]));
                            ed.choose_light(light);
                        }
                        (None, Some(item)) if on_handles => view::act(ed, view, view::Action::OpenMenu(item, [at.x, at.y])),
                        (None, _) => {
                            if let Some(spot) = view::floor_spot(&shown.world, origin, dir) {
                                view.menu = Some(view::Menu { on: view::MenuOn::Floor(spot), at: [at.x, at.y] });
                            }
                        }
                    }
                }
            }
            // Over the panels the wheel scrolls them (egui), else it zooms.
            Event::MouseWheel { y, mouse_x, mouse_y, .. } if !over(mouse_x, mouse_y) => {
                view.orbit(ed.scope).zoom_by(y);
                view.leave_game_view(ed.scope);
            }
            _ => {}
        }
        // The view goes with a model's voxels when its box grew on the
        // low sides or was trimmed.
        let moved = std::mem::take(&mut ed.moved);
        view.block.target += moved.as_vec3();
        true
    }

    /// Draws a frame if something happened since the last, or the panels
    /// asked for one: the 3D view, the panels over it.
    pub fn draw(&mut self) -> Result<()> {
        // A drag whose let-go never came (released outside the windows,
        // over another, events lost): the real button is asked; up for a
        // while (its up event may just be on its way), the drag ends as let
        // go where it is, one step. (Not a program's pointer: no real
        // button to ask.)
        if self.ed.drag_open() && !self.view.synthetic_press && self.win.is_some() && !left_button_down() {
            let since = *self.view.up_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= LOST_RELEASE {
                self.mouse_up_elsewhere();
            }
        } else {
            self.view.up_since = None;
        }
        // A BACKGROUND chosen: kept for the next window, either editor's.
        if self.view.backgrounds_changed {
            self.view.backgrounds_changed = false;
            let path = background::Backgrounds::path();
            // (The other editor's as the file has it now: another window
            // may have chosen it.)
            let mut keep = background::Backgrounds::load(&path).colours();
            let k = (self.ed.scope != Scope::Level) as usize;
            keep[k] = self.view.backgrounds[k];
            if let Err(e) = background::Backgrounds::of(keep).save(&path) {
                eprintln!("background: {e:#}");
            }
            self.dirty = true;
        }
        if self.looked.elapsed() >= LOOK_EVERY {
            self.looked = Instant::now();
            if self.ed.look_at_disk() {
                self.dirty = true;
            }
        }
        if !self.dirty && self.repaint_at.is_none_or(|r| Instant::now() < r) {
            return Ok(());
        }
        self.dirty = false;
        let (_, size) = self.sizes();
        let (scale, density) = self.win.as_ref().map_or((1.0, 1.0), |w| (w.screen.window.display_scale(), w.screen.window.pixel_density()));
        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let Self { win, ed, view, shown, figures, cfg, gui, input, thumbs, keys, rects, .. } = self;
        // The panels first: what was clicked there acts before the 3D view
        // is drawn.
        let raw = input.take(size, scale, density);
        let mut drawn = panels::Shown::default();
        let marker = self.marker.map(|(x, y)| input.point(x, y));
        // A figure's parts as its frames stand now (a stroke, a step back),
        // the view kept on it; the pieces' models, a new one's too.
        ed.follow_figure();
        view.follow_origin(ed);
        ed.slice_toward(view.block_eye(aspect));
        figures.follow(&ed.dir, ed);
        ed.sizes = figures.sizes();
        view.aspect = aspect;
        let out = gui.run_ui(raw, |ui| {
            drawn = panels::ui(ui, ed, view, figures, thumbs);
            let free = room_of(&drawn, ui.ctx().content_rect()).0;
            panels::joints(ui, ed, view, free);
            panels::marks(ui, ed, view, figures, free);
            if let Some(tip) = marker {
                crate::gui::marker::paint(ui.ctx(), tip);
            }
        });
        let acted = !drawn.clicked.is_empty() || drawn.renamed.is_some() || drawn.renamed_part.is_some();
        if let Some((p, typed)) = drawn.renamed_part.take() {
            match ed.rename_part(p, &typed) {
                Ok(()) => (view.renaming_part, view.rename_problem) = (None, None),
                Err(problem) => view.rename_problem = Some(problem),
            }
        }
        if let Some((item, typed)) = drawn.renamed.take() {
            match ed.rename(item, &typed) {
                Ok(()) => (view.renaming, view.rename_problem) = (None, None),
                Err(problem) => view.rename_problem = Some(problem),
            }
        }
        for action in std::mem::take(&mut drawn.clicked) {
            // DON'T OPEN: the window goes, the file untouched.
            if action == view::Action::DontOpen {
                self.closing = true;
                continue;
            }
            view::act(ed, view, action);
        }
        view.set_room(room_of(&drawn, gui.content_rect()).1);
        (*keys, *rects) = (drawn.keys, drawn.rects);
        #[cfg(test)]
        {
            self.rows = drawn.rows;
        }
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        // A picture asked for is taken once the panels have settled.
        let capture = self.capture.is_some_and(|late| late || (!acted && !delay.is_zero()));
        // A click changes what the panels show: once more at once.
        self.repaint_at = if acted { Some(Instant::now()) } else { Instant::now().checked_add(delay) };

        // The world brought up to the editor, the frame drawn.
        let rebuilt = shown.update(ed);
        let frame = view::frame(ed, view, shown, figures, &cfg.light, aspect);
        // (A window without a window draws nothing.)
        let Some(Win { screen, renderer, painter }) = win else {
            #[cfg(test)]
            {
                let whole = matches!(rebuilt, Ok(true));
                self.rebuilds += whole as usize;
                self.last = Some((frame, whole));
            }
            if let Err(e) = rebuilt {
                ed.message = Some(format!("{e:#}"));
            }
            return Ok(());
        };
        match rebuilt {
            Ok(true) => renderer.rebuild_all(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES),
            Ok(false) => {
                renderer.update_chunks(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES);
            }
            Err(e) => ed.message = Some(format!("{e:#}")),
        }
        match screen.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let target = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = screen.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                renderer.render(&screen.device, &screen.queue, &mut encoder, &target, &frame);
                screen.queue.submit([encoder.finish()]);
                painter.paint(&screen.device, &screen.queue, &target, gui, out, size);
                if capture {
                    self.capture = None;
                    self.picture = Some(screen.picture(&texture.texture).map_err(|e| format!("{e:#}")));
                }
                screen.queue.present(texture);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                screen.surface.configure(&screen.device, &screen.config);
                self.dirty = true;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
            wgpu::CurrentSurfaceTexture::Validation => anyhow::bail!("surface validation error"),
        }
        Ok(())
    }
}

/// Every face of the world's voxels: the editor's camera goes all round.
pub(crate) const ALL_FACES: crate::mesh::FaceMask = [true; 6];

#[cfg(test)]
mod tests {
    use super::*;

    /// A figure like the owner's: hips; a head on them turned 45; a nose
    /// on the head turned -45; hair on the head, its box far out in front
    /// and its joint set by hand to many digits; its box stated far too
    /// big (a part flew off once and came back). An editor window without a
    /// window on it.
    fn owners_like(name: &str) -> (EditorWindow, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("window-{name}"));
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "").unwrap();
        let part = |name: &str, at: [i32; 3], size: [i32; 3], sign: char, extra: &str| {
            let rows: Vec<String> = (0..size[2]).map(|_| (0..size[1]).map(|_| sign.to_string().repeat(size[0] as usize)).collect::<Vec<_>>().join("\n")).collect();
            format!("\n[[part]]\nname = \"{name}\"\nat = [{}, {}, {}]\n{extra}layers = \"\"\"\n{}\n\"\"\"\n", at[0], at[1], at[2], rows.join("\n\n"))
        };
        let text = format!(
            "format = \"{}\"\nsize = [13, 2001, 42]\n\n[palette]\na = \"#5a7aa8\"\nb = \"#d0c8b8\"\nc = \"#c85a4a\"\nd = \"#6b4423\"\n{}{}{}{}",
            crate::vvx::FORMAT,
            part("hips", [-3, -2, 0], [6, 4, 8], 'a', ""),
            part("head", [-2, -2, 8], [4, 4, 4], 'b', "parent = \"hips\"\njoint = [0, 0, 8]\nrotation = 45\n"),
            part("hair", [-2, -12, 10], [4, 3, 2], 'd', "parent = \"head\"\njoint = [0.5, -15.24295, 38.045776]\n"),
            part("nose", [0, -4, 9], [1, 2, 1], 'c', "parent = \"head\"\nrotation = -45\n"),
        );
        std::fs::write(dir.join("fig.vvx"), text).unwrap();
        let (ed, view, cfg) = open_model::<crate::Voxcade>(&dir, "fig.vvx").unwrap();
        let mut win = EditorWindow::headless(ed, view, cfg, &dir).unwrap();
        for _ in 0..3 {
            win.draw().unwrap();
        }
        (win, dir)
    }

    fn file(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("fig.vvx")).unwrap()
    }

    /// The camera as it looks: its turn, tilt and distance, and its middle
    /// from the model's pivot (the grid shown moves with the part chosen).
    fn camera(win: &EditorWindow) -> (f32, f32, f32, glam::Vec3) {
        let o = win.view.block;
        (o.yaw, o.pitch, o.zoom, o.target + win.ed.origin().unwrap_or(glam::IVec3::ZERO).as_vec3())
    }

    /// The window's events: the pointer moved there (`held`: the buttons
    /// the motion says are down, SDL's bits), a button down and up.
    fn motion(win: &mut EditorWindow, x: f32, y: f32, held: u32) {
        thread_local!(static LAST: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((800.0, 450.0)) });
        let (px, py) = LAST.with(|l| l.replace((x, y)));
        let e = sdl3::event::Event::MouseMotion { timestamp: 0, window_id: 0, which: 0, mousestate: sdl3::mouse::MouseState::from_sdl_state(held), x, y, xrel: x - px, yrel: y - py };
        assert!(win.handle(&e, false));
        win.draw().unwrap();
    }

    fn button(win: &mut EditorWindow, x: f32, y: f32, button: sdl3::mouse::MouseButton, down: bool) {
        let e = if down {
            sdl3::event::Event::MouseButtonDown { timestamp: 0, window_id: 0, which: 0, mouse_btn: button, clicks: 1, x, y }
        } else {
            sdl3::event::Event::MouseButtonUp { timestamp: 0, window_id: 0, which: 0, mouse_btn: button, clicks: 1, x, y }
        };
        assert!(win.handle(&e, false));
        win.draw().unwrap();
    }

    fn click(win: &mut EditorWindow, at: egui::Pos2) {
        motion(win, at.x, at.y, 0);
        button(win, at.x, at.y, sdl3::mouse::MouseButton::Left, true);
        button(win, at.x, at.y, sdl3::mouse::MouseButton::Left, false);
        // (Past a double click's time: the next click is a click of its own.)
        std::thread::sleep(std::time::Duration::from_millis(350));
        win.draw().unwrap();
    }

    fn row(win: &EditorWindow, name: &str) -> egui::Pos2 {
        win.rows.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no row {name}: {:?}", win.rows)).1.center()
    }

    fn chosen(win: &EditorWindow) -> String {
        let f = win.ed.figure.as_ref().unwrap();
        f.selected.map_or(String::new(), |p| f.parts()[p].name.clone())
    }

    /// The owner's case through the window's own events: clicks on the
    /// parts' rows (the nose, the head, the nose again, the hair, the
    /// figure whole) each choose that row, change nothing of the file and
    /// make no step, and the camera stays as it looks; so do motions that
    /// carry buttons held from elsewhere (a press on a panel, a release
    /// lost). Opening frames the figure by its voxels, not its stated box.
    /// TERRAIN in the window: a point under the mouse lit, pressed and
    /// dragged up through the window's own events: the land up there as
    /// the mouse goes, written at the let-go as one step; Escape lets go
    /// of the choice.
    #[test]
    fn a_terrain_point_dragged_in_the_window_is_one_step() {
        let dir = crate::scratch::Scratch::new("window-land");
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "").unwrap();
        let rows = ["4 4 4 4 4 4 4 4 4 4"; 6].join("\n");
        std::fs::write(dir.join("levels/land.toml"), format!("format = \"voxcade level 1\"\nsize = [160, 96, 80]\n\n[terrain]\nspacing = 16\nheights = \"\"\"\n{rows}\n\"\"\"\n\n[[terrain.material]]\ncolor = \"#5c8a3a\"\n")).unwrap();
        let (ed, view, cfg) = open::<crate::Voxcade>(&dir, Some("levels/land.toml"), None, false).unwrap();
        let mut win = EditorWindow::headless(ed, view, cfg, &dir).unwrap();
        view::act(&mut win.ed, &mut win.view, view::Action::Layer(Layer::Terrain));
        for _ in 0..3 {
            win.draw().unwrap();
        }
        let start = std::fs::read_to_string(dir.join("levels/land.toml")).unwrap();
        let p = glam::IVec2::new(4, 3);
        let vp = win.view.view_proj(&win.view.level, 1600.0 / 900.0);
        let at = win.ed.points_on_screen(vp, glam::Vec2::new(1600.0, 900.0)).into_iter().find(|(q, _)| *q == p).unwrap().1;
        motion(&mut win, at.x, at.y, 0);
        assert_eq!(win.ed.land.hover, Some(p), "lit under the mouse");
        button(&mut win, at.x, at.y, sdl3::mouse::MouseButton::Left, true);
        for k in 1..=6 {
            motion(&mut win, at.x, at.y - 10.0 * k as f32, 1);
        }
        let t = win.ed.terrain().unwrap();
        let raised = t.heights[t.index(p).unwrap()];
        assert!(raised > 10, "up as the mouse goes: {raised}");
        let now = win.ed.points_on_screen(vp, glam::Vec2::new(1600.0, 900.0)).into_iter().find(|(q, _)| *q == p).unwrap().1;
        assert!((now.y - (at.y - 60.0)).abs() < 4.0, "the point stays under the pointer: {now} for {}", at.y - 60.0);
        // Shift held: a third as far.
        let e = sdl3::event::Event::MouseMotion { timestamp: 0, window_id: 0, which: 0, mousestate: sdl3::mouse::MouseState::from_sdl_state(1), x: at.x, y: at.y - 60.0, xrel: 0.0, yrel: 0.0 };
        assert!(win.handle(&e, true));
        let t = win.ed.terrain().unwrap();
        let fine = t.heights[t.index(p).unwrap()];
        assert!((fine - 4 - (raised - 4) / 3).abs() <= 1, "fine: {fine} for {raised}");
        assert_eq!(std::fs::read_to_string(dir.join("levels/land.toml")).unwrap(), start, "nothing written while dragging");
        button(&mut win, at.x, at.y - 60.0, sdl3::mouse::MouseButton::Left, false);
        assert_eq!(win.ed.steps(), 1, "one step");
        assert!(std::fs::read_to_string(dir.join("levels/land.toml")).unwrap().contains(&format!(" {fine} ")));
        assert_eq!(win.ed.land.chosen, Some(p));
        let esc = sdl3::event::Event::KeyDown { timestamp: 0, window_id: 0, keycode: Some(sdl3::keyboard::Keycode::Escape), scancode: None, keymod: sdl3::keyboard::Mod::NOMOD, repeat: false, which: 0, raw: 0 };
        assert!(win.handle(&esc, false));
        assert_eq!(win.ed.land.chosen, None);
    }

    /// GAME VIEW on the owner's figure: the figure fills the view from the
    /// game's angle; the wheel and the right button zoom and turn it and
    /// GAME VIEW stays on; pressed again, the editor's look as it was.
    #[test]
    fn game_view_frames_the_figure_and_the_wheel_and_right_button_work() {
        let (mut win, _dir) = owners_like("game-view");
        let before = win.view.block;
        view::act(&mut win.ed, &mut win.view, view::Action::GameView);
        win.draw().unwrap();
        let game = win.view.game.clone();
        let framed = win.view.block;
        assert_eq!((framed.yaw, framed.pitch, framed.fov), (game.yaw, game.pitch, game.fov));
        assert!(framed.zoom < 100.0 && framed.zoom < game.view_width, "framed by the figure, not the game's width: {}", framed.zoom);
        let wheel = sdl3::event::Event::MouseWheel { timestamp: 0, window_id: 0, which: 0, x: 0.0, y: 1.0, direction: sdl3::mouse::MouseWheelDirection::Normal, mouse_x: 800.0, mouse_y: 450.0, integer_x: 0, integer_y: 1 };
        assert!(win.handle(&wheel, false));
        win.draw().unwrap();
        assert!(win.view.block.zoom < framed.zoom, "the wheel zooms");
        motion(&mut win, 800.0, 450.0, 0);
        button(&mut win, 800.0, 450.0, sdl3::mouse::MouseButton::Right, true);
        motion(&mut win, 860.0, 470.0, 0b100);
        button(&mut win, 860.0, 470.0, sdl3::mouse::MouseButton::Right, false);
        assert!(win.view.block.yaw != framed.yaw || win.view.block.pitch != framed.pitch, "the right button turns");
        assert!(win.view.before_game.is_some(), "and GAME VIEW stays on");
        view::act(&mut win.ed, &mut win.view, view::Action::GameView);
        assert!(win.view.before_game.is_none());
        assert_eq!(win.view.block, before, "off: the editor's look");
    }

    #[test]
    fn clicks_on_the_rows_choose_and_change_nothing() {
        let (mut win, dir) = owners_like("rows");
        let start = file(&dir);
        let cam = camera(&win);
        assert!(win.view.block.zoom < 100.0, "framed by its voxels, not a box 2001 deep: {}", win.view.block.zoom);
        for name in ["nose", "head", "nose", "hair", "", "nose"] {
            let at = row(&win, name);
            click(&mut win, at);
            assert_eq!(chosen(&win), name, "a click on the row {name:?} chooses it");
            assert_eq!((file(&dir), win.ed.steps()), (start.clone(), 0), "after the row {name:?}: the file as it was, no step");
            let now = camera(&win);
            assert_eq!((now.0, now.1, now.2), (cam.0, cam.1, cam.2), "after the row {name:?}: the camera turns nowhere");
            assert!((now.3 - cam.3).length() < 1e-3, "after the row {name:?}: its middle stays: {:?} {:?}", now.3, cam.3);
        }
        // Motions carrying the left and right buttons held from elsewhere,
        // across the view: nothing turns, nothing moves.
        for (x, y) in [(600.0, 300.0), (900.0, 500.0), (700.0, 700.0)] {
            motion(&mut win, x, y, 0b101);
        }
        // A press on a row, the pointer dragged on into the view, let go.
        let at = row(&win, "head");
        motion(&mut win, at.x, at.y, 0);
        button(&mut win, at.x, at.y, sdl3::mouse::MouseButton::Left, true);
        for (x, y) in [(400.0, 400.0), (700.0, 450.0)] {
            motion(&mut win, x, y, 1);
        }
        button(&mut win, 700.0, 450.0, sdl3::mouse::MouseButton::Left, false);
        let now = camera(&win);
        assert_eq!((now.0, now.1, now.2), (cam.0, cam.1, cam.2), "the camera turns nowhere");
        assert_eq!((file(&dir), win.ed.steps()), (start, 0), "nothing changed");
    }

    /// Where a part's voxels show on the window (points), as drawn now.
    fn part_on_screen(win: &EditorWindow, name: &str) -> Vec<glam::Vec2> {
        let f = win.ed.figure.as_ref().unwrap();
        let p = f.parts().iter().position(|o| o.name == name).unwrap();
        let vp = win.view.view_proj(&win.view.block, 1600.0 / 900.0);
        let (lo, _) = win.ed.figure_box().unwrap();
        let map = f.rest_map(p, lo);
        f.model.parts[p]
            .voxels
            .iter()
            .map(|&k| {
                let c = vp * map.transform_point3(f.model.frames[0][k as usize].0.as_vec3() + 0.5).extend(1.0);
                glam::Vec2::new((c.x / c.w + 1.0) / 2.0 * 1600.0, (1.0 - c.y / c.w) / 2.0 * 900.0)
            })
            .collect()
    }

    /// The box about points on the screen, a few points wider.
    fn about(points: &[glam::Vec2]) -> (glam::Vec2, glam::Vec2) {
        let (lo, hi) = points.iter().fold((glam::Vec2::MAX, glam::Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        (lo - 3.0, hi + 3.0)
    }

    /// SELECT's rectangle dragged from `a` to `b`; `held` the modifiers'
    /// keys first (Ctrl: ADD, Alt: REMOVE).
    fn marquee(win: &mut EditorWindow, a: glam::Vec2, b: glam::Vec2) {
        motion(win, a.x, a.y, 0);
        button(win, a.x, a.y, sdl3::mouse::MouseButton::Left, true);
        for k in 1..=4 {
            let p = a + (b - a) * k as f32 / 4.0;
            motion(win, p.x, p.y, 1);
        }
        button(win, b.x, b.y, sdl3::mouse::MouseButton::Left, false);
    }

    /// The parts the selection holds voxels of, and how many.
    fn selected(win: &EditorWindow) -> (Vec<String>, usize) {
        let Some(s) = win.ed.selection() else { return (Vec::new(), 0) };
        let m = win.ed.model.as_ref().unwrap();
        let names = s.cells.keys().filter_map(|o| match o {
            model::selection::Owner::Frame(f) => m.parts.iter().find(|p| p.frame == *f).map(|p| p.name.clone()),
            model::selection::Owner::Block(_) => None,
        });
        let mut names: Vec<String> = names.collect();
        names.sort();
        (names, s.count())
    }

    /// SELECT is a rectangle on the screen: over the head and the nose
    /// with the figure whole chosen it takes from both, with the head
    /// chosen from the head alone, a hidden part never; THROUGH takes the
    /// voxels behind too; ADD and REMOVE grow and shrink it; NEW PART,
    /// REMOVE and PAINT from it, each undone byte for byte.
    #[test]
    fn select_is_a_rectangle_on_the_screen() {
        let (mut win, dir) = owners_like("marquee");
        let start = file(&dir);
        view::act(&mut win.ed, &mut win.view, view::Action::Tool(model::Tool::Select));
        win.draw().unwrap();
        let mut both = part_on_screen(&win, "head");
        both.extend(part_on_screen(&win, "nose"));
        let (a, b) = about(&both);
        marquee(&mut win, a, b);
        let (names, _) = selected(&win);
        assert!(names.contains(&"head".to_string()) && names.contains(&"nose".to_string()), "from both: {names:?}");
        // The nose hidden: never.
        let nose = win.ed.figure.as_ref().unwrap().parts().iter().position(|o| o.name == "nose").unwrap();
        view::act(&mut win.ed, &mut win.view, view::Action::Hide(nose));
        marquee(&mut win, a, b);
        assert!(!selected(&win).0.contains(&"nose".to_string()), "a hidden part never");
        view::act(&mut win.ed, &mut win.view, view::Action::Hide(nose));
        // THROUGH: the head's voxels behind too; off: only those seen.
        let (h0, h1) = about(&part_on_screen(&win, "head"));
        marquee(&mut win, h0, h1);
        let through = selected(&win).1;
        view::act(&mut win.ed, &mut win.view, view::Action::SelectThrough);
        marquee(&mut win, h0, h1);
        let seen = selected(&win).1;
        assert!(seen < through && seen > 0, "seen {seen} of {through}");
        view::act(&mut win.ed, &mut win.view, view::Action::SelectThrough);
        // With the head chosen: the head's alone.
        let head = win.ed.figure.as_ref().unwrap().parts().iter().position(|o| o.name == "head").unwrap();
        win.ed.choose_part(head);
        win.view.follow_origin(&win.ed);
        win.draw().unwrap();
        let (a, b) = about(&both);
        marquee(&mut win, a, b);
        assert_eq!(selected(&win).0, ["head"], "a part chosen: its own alone");
        // ADD and REMOVE: a half, the other half added, the first taken out.
        let mid = (h0 + h1) / 2.0;
        marquee(&mut win, h0, glam::Vec2::new(mid.x, h1.y));
        let left = selected(&win).1;
        view::act(&mut win.ed, &mut win.view, view::Action::SelectMode(model::selection::SelectMode::Add));
        marquee(&mut win, glam::Vec2::new(mid.x, h0.y), h1);
        let all = selected(&win).1;
        assert!(all > left, "ADD: more ({all} after {left})");
        view::act(&mut win.ed, &mut win.view, view::Action::SelectMode(model::selection::SelectMode::Remove));
        marquee(&mut win, h0, glam::Vec2::new(mid.x, h1.y));
        let right = selected(&win).1;
        assert!(right < all && right > 0, "REMOVE: fewer ({right} of {all})");
        view::act(&mut win.ed, &mut win.view, view::Action::SelectMode(model::selection::SelectMode::New));
        assert_eq!((file(&dir), win.ed.steps()), (start.clone(), 0), "selecting changes nothing");
        // NEW PART, REMOVE, PAINT: each a step, undone byte for byte.
        for action in [view::Action::SelNewPart, view::Action::SelRemove, view::Action::SelPaint] {
            marquee(&mut win, h0, h1);
            assert!(win.ed.selection().is_some(), "{action:?}: selected");
            view::act(&mut win.ed, &mut win.view, action);
            assert_ne!(file(&dir), start, "{action:?}: done");
            win.ed.undo();
            assert_eq!(file(&dir), start, "{action:?}: undone byte for byte");
            win.ed.choose_part(win.ed.figure.as_ref().unwrap().parts().iter().position(|o| o.name == "head").unwrap());
            win.view.follow_origin(&win.ed);
            win.draw().unwrap();
        }
    }

    /// Where every voxel of the figure is drawn (from the pivot, to a
    /// thousandth), each with its color.
    fn drawn(win: &EditorWindow) -> std::collections::BTreeSet<([i64; 3], [u8; 4])> {
        let f = win.ed.figure.as_ref().unwrap();
        let shift = f.low;
        let mut out = std::collections::BTreeSet::new();
        for p in 0..f.parts().len() {
            let map = f.rest_map(p, glam::IVec3::ZERO);
            for &k in &f.model.parts[p].voxels {
                let (at, c) = f.model.frames[0][k as usize];
                let v = map.transform_point3(at.as_vec3() + 0.5) + shift.as_vec3();
                out.insert(((v * 1000.0).round().as_i64vec3().to_array(), c));
            }
        }
        out
    }

    /// A part's row dragged onto another's hangs it there where it stands:
    /// every voxel drawn where it was, only its own lines in the file
    /// changed, one step, undone byte for byte; onto its own child or the
    /// figure whole refused; a click on a row still only chooses.
    #[test]
    fn a_row_dragged_onto_another_hangs_the_part_there() {
        let (mut win, dir) = owners_like("reparent");
        let start = file(&dir);
        let before = drawn(&win);
        let (from, to) = (row(&win, "nose"), row(&win, "hips"));
        motion(&mut win, from.x, from.y, 0);
        button(&mut win, from.x, from.y, sdl3::mouse::MouseButton::Left, true);
        for k in 1..=6 {
            let p = from + (to - from) * k as f32 / 6.0;
            motion(&mut win, p.x, p.y, 1);
        }
        button(&mut win, to.x, to.y, sdl3::mouse::MouseButton::Left, false);
        win.draw().unwrap();
        let now = file(&dir);
        assert!(now.contains("name = \"nose\"") && now.split("name = \"nose\"").nth(1).unwrap().split("[[part]]").next().unwrap().contains("parent = \"hips\""), "the nose hangs on the hips: {now}");
        assert_eq!(win.ed.steps(), 1, "one step");
        // (The head of the file aside: the first step makes `size` tight.)
        let others = |t: &str| t.split("[[part]]").skip(1).filter(|p| !p.contains("name = \"nose\"")).map(str::to_string).collect::<Vec<_>>();
        assert_eq!(others(&now), others(&start), "the others' text untouched");
        let after = drawn(&win);
        let near = |a: &std::collections::BTreeSet<([i64; 3], [u8; 4])>, b: &std::collections::BTreeSet<([i64; 3], [u8; 4])>| a.len() == b.len() && a.iter().all(|(p, c)| b.iter().any(|(q, d)| c == d && (0..3).all(|i| (p[i] - q[i]).abs() <= 2)));
        assert!(near(&before, &after), "every voxel drawn where it was");
        win.ed.undo();
        assert_eq!(file(&dir), start, "undone byte for byte");
        // The hair (unturned, on the head turned 45) onto the hips: it
        // takes the head's turn as its own rest, drawn where it was.
        let (from, to) = (row(&win, "hair"), row(&win, "hips"));
        motion(&mut win, from.x, from.y, 0);
        button(&mut win, from.x, from.y, sdl3::mouse::MouseButton::Left, true);
        for k in 1..=6 {
            let p = from + (to - from) * k as f32 / 6.0;
            motion(&mut win, p.x, p.y, 1);
        }
        button(&mut win, to.x, to.y, sdl3::mouse::MouseButton::Left, false);
        win.draw().unwrap();
        let hair = file(&dir).split("name = \"hair\"").nth(1).unwrap().split("[[part]]").next().unwrap().to_string();
        assert!(hair.contains("parent = \"hips\"") && hair.contains("rotation = 45\n"), "{hair}");
        assert!(near(&before, &drawn(&win)), "the hair drawn where it was");
        win.ed.undo();
        assert_eq!(file(&dir), start);
        // Refused: the head onto the nose (its own child), onto the whole.
        let (head, nose) = (row(&win, "head"), row(&win, "nose"));
        for (a, b) in [(head, nose), (nose, row(&win, ""))] {
            motion(&mut win, a.x, a.y, 0);
            button(&mut win, a.x, a.y, sdl3::mouse::MouseButton::Left, true);
            for k in 1..=6 {
                let p = a + (b - a) * k as f32 / 6.0;
                motion(&mut win, p.x, p.y, 1);
            }
            button(&mut win, b.x, b.y, sdl3::mouse::MouseButton::Left, false);
            assert_eq!(file(&dir), start, "refused");
            assert!(win.ed.hint.is_some(), "said why");
        }
        // A click on a row still only chooses.
        let at = row(&win, "nose");
        click(&mut win, at);
        assert_eq!((chosen(&win), file(&dir)), ("nose".to_string(), start));
    }

    /// POINTER's clicks on parts in the view choose them and change
    /// nothing; TRIM changes only `size` (the joint written by hand to its
    /// last digit stays); a LOCAL move of the nose changes only the nose.
    #[test]
    fn pointer_trim_and_a_local_move_change_only_their_own() {
        let (mut win, dir) = owners_like("pointer");
        let start = file(&dir);
        // A click on the head where it shows.
        let aspect = 1600.0 / 900.0;
        let vp = win.view.view_proj(&win.view.block, aspect);
        let head = win.ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == "head").unwrap();
        let f = win.ed.figure.as_ref().unwrap();
        let k = f.model.parts[head].visible[0] as usize;
        let middle = f.rest_map(head, win.ed.figure_box().unwrap().0).transform_point3(f.model.frames[0][k].0.as_vec3() + 0.5);
        let p = vp * middle.extend(1.0);
        let at = egui::pos2((p.x / p.w + 1.0) / 2.0 * 1600.0, (1.0 - p.y / p.w) / 2.0 * 900.0);
        click(&mut win, at);
        assert_eq!((chosen(&win), file(&dir), win.ed.steps()), ("head".to_string(), start.clone(), 0), "POINTER: the head chosen, nothing changed");
        // TRIM: only the size line.
        win.ed.back_to_level();
        win.ed.trim();
        let trimmed = file(&dir);
        let changed: Vec<(&str, &str)> = start.lines().zip(trimmed.lines()).filter(|(a, b)| a != b).collect();
        assert_eq!(changed.len(), 1, "only one line: {changed:?}");
        assert!(changed[0].0.starts_with("size") && trimmed.contains("joint = [0.5, -15.24295, 38.045776]"), "{changed:?}");
        // A LOCAL move of the nose: the nose's lines only.
        let nose = win.ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == "nose").unwrap();
        win.ed.choose_part(nose);
        win.ed.move_part_by(glam::Vec3::new(1.0, 0.0, 0.0));
        let moved = file(&dir);
        let others = |t: &str| t.split("[[part]]").filter(|p| !p.contains("name = \"nose\"")).map(str::to_string).collect::<Vec<_>>();
        assert_eq!(others(&moved), others(&trimmed), "only the nose's text changed");
    }

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
        assert!(room.among_pieces() && room.free_turns && view.level.zoom >= 360.0);
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

