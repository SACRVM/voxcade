//! Voxcade's editor for levels of voxel blocks (`docs/tiles.md`): one
//! window, two levels. In the level the cells hold blocks, placed,
//! removed, replaced, turned and mirrored, and its things stand on them,
//! placed, moved and removed; a block opened (a second click on it, or a
//! double click on a cell) is shaped voxel by voxel. Every change is saved
//! at once, and a game running beside it shows it straight away. It is
//! calm on purpose: few tools in view, large and readable. Its panels are
//! the IDE's look (`panels.rs`, egui), the 3D view the game's own light.

pub mod figure;
pub mod frames;
pub mod handles;
pub mod model;
pub mod panels;
pub mod stage;
pub mod view;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::game::Game;
use model::{Editor, Item, Layer, Scope};
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
}

/// A change `--shot` tries among the pieces, to see what it does: a model
/// (its path in the game folder; or `light`, `place`, `area`) set down at
/// a spot; an item (by its name) moved to a spot, turned by degrees,
/// removed, copied to a spot, renamed; a piece's light set (its reach,
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
            anyhow::ensure!(ed.chosen_thing().is_some_and(|t| t.model.is_some()), "--turn {name} --about x or y: only a piece of a model tips");
            anyhow::ensure!(ed.tip(*axis, *by), "--turn {name} --by {by}: this game turns things on the grid, in quarter turns (c.camera.turns = \"free\" for any angle; a solid piece keeps to them too)");
        }
        ShotOp::Turn(name, by, _) => {
            let item = find(ed, name)?;
            ed.choose_item(Some(item));
            if by.rem_euclid(90.0) == 0.0 {
                for _ in 0..(by / 90.0).rem_euclid(4.0) as i32 {
                    ed.turn_item(1);
                }
            } else if let Some(t) = ed.chosen_thing() {
                anyhow::ensure!(ed.free_turns, "--turn {name} --by {by}: this game turns things on the grid, in quarter turns (c.camera.turns = \"free\" for any angle)");
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
    if cfg.camera.turns == crate::config::Turns::Quarter && !off.is_empty() {
        ed.question = Some(off.iter().map(|t| t.called()).collect());
    }
    ed.kinds = cfg.things.keys().cloned().collect();
    ed.held_kind = ed.kinds.first().cloned();
    ed.free_turns = cfg.camera.turns == crate::config::Turns::Free;
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
    if let Some(word) = &opts.tool {
        let tool = model::Tool::of(ed.scope, ed.layer).iter().copied().find(|t| t.label(ed.scope).eq_ignore_ascii_case(word));
        ed.choose(tool.with_context(|| format!("--tool {word}: no such tool here"))?);

    }
    if opts.shift {
        ed.shift_pick(true);
    }
    let aspect = opts.width as f32 / opts.height as f32;
    let mut figures = Figures::load(dir, &cfg).with_pieces(dir, &ed.level);
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
    view.aspect = aspect;
    // (Where the panels will lie, for the view's middle: as in a window.)
    let (w, h) = (opts.width as f32, opts.height as f32);
    view.set_room(if ed.among_pieces() { [stage::OUTLINER / w, stage::TOP / h, stage::INSPECTOR / w, 0.0] } else { [0.0, 0.0, panels::COLUMN / w, panels::BAR / h] });
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
    if opts.turn_handles {
        view::act(&mut ed, &mut view, view::Action::HandleMode(true));
        anyhow::ensure!(view.turn_mode(&ed), "--handles turn: what is chosen does not turn (a piece of a model or a place does)");
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
    let again = opts.stamp || opts.held.is_some() || opts.menu.is_some() || opts.hover.is_some();
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
        if pass == 0 && ed.among_pieces() {
            let screen = ctx.content_rect();
            if let Some(name) = &opts.menu {
                let at = egui::pos2(screen.left() + mx * screen.width(), screen.top() + my * screen.height());
                if name == "floor" {
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
            if (opts.menu.is_some() || opts.hover.is_some())
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
        for &(p, depth) in &figure.tree {
            let part = &figure.parts()[p];
            let on = part.parent.map_or("(the root)".to_string(), |q| format!("on {}", figure.parts()[q].name));
            let j = figure.joint(p);
            let by_hand = match (part.hung == crate::vox::Hung::ByHand, part.joint_by_hand) {
                (true, true) => " (parent and joint set by hand)",
                (true, false) => " (parent set by hand)",
                (false, true) => " (joint set by hand)",
                (false, false) => "",
            };
            let chosen = if figure.selected == Some(p) { " (chosen)" } else { "" };
            println!("{}{} {on}, turning at {}, {}, {}{by_hand}{chosen}{}", "  ".repeat(depth + 1), part.name, j.x, j.y, j.z, if figure.hidden[p] { " (hidden)" } else { "" });
            for w in figure.warnings(p) {
                println!("{}  ! {w}", "  ".repeat(depth + 1));
            }
            for &(q, n) in &part.overlaps {
                println!("{}  ({n} in the same place as {}'s)", "  ".repeat(depth + 1), figure.parts()[q].name);
            }
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

/// Whether the tools work on the level's things.
fn among_things(ed: &Editor) -> bool {
    ed.scope == Scope::Level && ed.layer == Layer::Things
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
        let asked: Vec<String> = windows.iter_mut().filter_map(EditorWindow::take_model_edit).collect();
        for file in asked {
            if let Some(w) = windows.iter_mut().find(|w| w.model_file() == Some(file.as_str())) {
                w.raise();
                continue;
            }
            match EditorWindow::open_model::<G>(&video, dir, &file, Some(windows[0].gpu())) {
                Ok(w) => windows.push(w),
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

/// The editor in a window of its own: `voxcade edit`'s, or one of those the
/// IDE opens (several, on the IDE's GPU). It takes the events of its
/// window and draws when something happened.
pub struct EditorWindow {
    screen: crate::window::Screen,
    renderer: crate::render::Renderer,
    ed: Editor,
    view: View,
    shown: Shown,
    figures: Figures,
    cfg: crate::config::GameConfig,
    /// The panels (egui, as the IDE): their input, painter, pictures, the
    /// keys they named and where they lay, last frame.
    gui: egui::Context,
    input: crate::gui::input::Input,
    painter: crate::gui::Painter,
    thumbs: panels::Thumbs,
    keys: Vec<(char, view::Action)>,
    rects: Vec<egui::Rect>,
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

    fn with(video: &sdl3::VideoSubsystem, title: &str, ed: Editor, view: View, cfg: crate::config::GameConfig, dir: &Path, gpu: Option<&crate::window::Gpu>) -> Result<Self> {
        let screen = crate::window::Screen::open_on(video, title, 1600, 900, false, gpu)?;
        let mut renderer = crate::render::Renderer::new(&screen.device, screen.config.format, screen.config.width, screen.config.height);
        let mut shown = Shown::new(&ed)?;
        renderer.rebuild_all(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES);
        let figures = Figures::load(dir, &cfg).with_pieces(dir, &ed.level);
        let gui = egui::Context::default();
        crate::gui::style(&gui);
        let painter = crate::gui::Painter::new(&screen.device, screen.config.format, true);
        Ok(Self {
            screen,
            renderer,
            ed,
            view,
            shown,
            figures,
            cfg,
            gui,
            input: crate::gui::input::Input::default(),
            painter,
            thumbs: panels::Thumbs::default(),
            keys: Vec::new(),
            rects: Vec::new(),
            steps_at_click: 0,
            dirty: true,
            repaint_at: None,
            marker: None,
            capture: None,
            picture: None,
            closing: false,
        })
    }

    /// Whether it is to close (DON'T OPEN was answered).
    pub fn closing(&self) -> bool {
        self.closing
    }

    /// When it wants to draw next (None: only after an event of its own).
    pub fn wakes_at(&self) -> Option<Instant> {
        if self.dirty { Some(Instant::now()) } else { self.repaint_at }
    }

    /// Its window's SDL id: the events that are its.
    pub fn id(&self) -> u32 {
        self.screen.window.id()
    }

    /// Its title, size in pixels, pixels to one of SDL's points (the
    /// mouse's) and whether it has the keyboard.
    pub fn info(&self) -> (String, u32, u32, f32, bool) {
        (self.screen.window.title().to_string(), self.screen.config.width, self.screen.config.height, self.screen.window.pixel_density(), self.screen.window.has_input_focus())
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

    /// The GPU it draws with, to open another window on.
    pub fn gpu(&self) -> &crate::window::Gpu {
        self.screen.gpu()
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
        self.screen.window.raise();
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
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => self.screen.resize(&mut self.renderer, w, h),
                _ => {}
            }
            return true;
        }
        let Self { screen, renderer, ed, view, shown, figures, cfg, keys, rects, gui, input, steps_at_click, .. } = self;
        self.dirty = true;
        let aspect = screen.aspect();
        // The mouse over the panels (or a list or picker of theirs open
        // there) is theirs, not the 3D view's.
        let over = |x: f32, y: f32| rects.iter().any(|r| r.contains(input.point(x, y))) || gui.egui_wants_pointer_input();
        match *event {
            Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return false,
            Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => screen.resize(renderer, w, h),
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
            Event::Window { win_event: WindowEvent::FocusLost, .. } => {
                view.cut = false;
                view.only_empty = false;
                ed.shift_pick(false);
            }
            // Back to the level: the pieces' models as their files are now
            // (one changed in the model editor shows).
            Event::Window { win_event: WindowEvent::FocusGained, .. } if ed.scope == Scope::Level => {
                *figures = Figures::load(&ed.dir, cfg).with_pieces(&ed.dir, &ed.level);
            }
            Event::KeyDown { keycode: Some(key), keymod, .. } if ed.among_pieces() && !gui.egui_wants_keyboard_input() && pieces_key(ed, view, key, keymod) => {}
            Event::KeyDown { keycode: Some(key), keymod, .. } if !gui.egui_wants_keyboard_input() => {
                let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD);
                let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                match key {
                    // BOX and SPHERE: the shape a voxel deeper or shallower.
                    Keycode::Plus | Keycode::Equals | Keycode::KpPlus if ed.forms() => ed.deepen(1),
                    Keycode::Minus | Keycode::KpMinus if ed.forms() => ed.deepen(-1),
                    Keycode::Escape if ed.forms() && ed.drop_form() => {}
                    Keycode::Z if ctrl && shift => ed.redo(),
                    Keycode::Z if ctrl => ed.undo(),
                    Keycode::Y if ctrl => ed.redo(),
                    // A model's frames, one by one.
                    Keycode::Left => ed.step_frame(-1),
                    Keycode::Right => ed.step_frame(1),
                    Keycode::Escape => {
                        if view.choosing {
                            view.choosing = false;
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
                let (sx, sy) = screen.share(x, y);
                let (dx, dy) = screen.share(xrel, yrel);
                if mousestate.middle() || (mousestate.right() && shift) {
                    view.orbit(ed.scope).pan(dx, dy, aspect);
                    view.before_game = None;
                } else if mousestate.right() {
                    view.orbit(ed.scope).turn(dx, dy);
                    view.before_game = None;
                }
                // The right button dragged: no click, no menu.
                if let Some((px, py)) = view.right_press
                    && (px - x).abs() + (py - y).abs() > 5.0
                {
                    view.right_press = None;
                }
                let over = over(x, y) && !mousestate.left();
                if ed.among_pieces() {
                    let (origin, dir) = view.voxel_ray(aspect, sx, sy);
                    let room = gui.content_rect();
                    let mouse = input.point(x, y);
                    if mousestate.left() && ed.gripped().is_some() {
                        let vp = view.view_proj(&view.level, room.width() / room.height().max(1.0));
                        ed.drag_grip(vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), (origin, dir));
                    } else if mousestate.left() && ed.dragging().is_some() {
                        ed.drag_item(origin, dir);
                    } else if ed.hanging.is_some() {
                        ed.hang_at(if over { None } else { view::floor_spot(&shown.world, origin, dir) });
                    }
                    let busy = over || ed.hanging.is_some() || mousestate.left();
                    view.grip_hover = if busy { None } else { stage::grip_under(ed, view, room, mouse) };
                    view.over = if busy || view.grip_hover.is_some() { None } else { view::item_along(ed, figures, origin, dir) };
                } else if among_things(ed) {
                    (view.spot, view.under) = if over { (None, None) } else { view.aim_things(ed, shown, figures, aspect, sx, sy) };
                    if mousestate.left() {
                        ed.drag_thing(view.spot);
                    }
                } else if ed.forms() {
                    // BOX and SPHERE: the footprint dragged out on its face's
                    // plane.
                    view.hover = if over { None } else { view.aim(ed, aspect, sx, sy) };
                    if mousestate.left() && ed.drawing.is_some() {
                        let (origin, dir) = view.ray(ed, aspect, sx, sy);
                        ed.draw_form(origin, dir);
                    }
                } else {
                    view.hover = if over { None } else { view.aim(ed, aspect, sx, sy) };
                    if mousestate.left()
                        && let Some(hit) = view.hover
                    {
                        ed.drag(hit);
                    }
                }
            }
            Event::MouseButtonDown { mouse_btn: MouseButton::Left, clicks, x, y, .. } if !over(x, y) => {
                let (sx, sy) = screen.share(x, y);
                if ed.among_pieces() {
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
                        ed.hang_at(view::floor_spot(&shown.world, origin, dir));
                        ed.set_down();
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
                } else if let Some(hit) = view.aim(ed, aspect, sx, sy) {
                    *steps_at_click = ed.steps();
                    if ed.forms() {
                        ed.begin_form(hit, view.cut);
                    } else {
                        ed.press(hit);
                    }
                }
            }
            // BOX and SPHERE: the footprint let go is set right where it
            // was drawn (Alt: only where empty; Ctrl, held as it was
            // pressed: cut away), fresh for + and - until the next action.
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } if ed.forms() => {
                if ed.drawing.is_some() {
                    ed.set_drawn(view.only_empty);
                }
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } => ed.release(),
            // A right click (not a drag) among the pieces: the menu of what
            // is under the mouse (chosen), or of the floor there.
            Event::MouseButtonDown { mouse_btn: MouseButton::Right, x, y, .. } => view.right_press = (!over(x, y) && ed.among_pieces()).then_some((x, y)),
            Event::MouseButtonUp { mouse_btn: MouseButton::Right, x, y, .. } => {
                if view.right_press.take().is_some() && ed.among_pieces() && ed.hanging.is_none() {
                    let (sx, sy) = screen.share(x, y);
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
                view.before_game = None;
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
        if !self.dirty && self.repaint_at.is_none_or(|r| Instant::now() < r) {
            return Ok(());
        }
        self.dirty = false;
        let Self { screen, renderer, ed, view, shown, figures, cfg, gui, input, painter, thumbs, keys, rects, .. } = self;
        // The panels first: what was clicked there acts before the 3D view
        // is drawn.
        let size = [screen.config.width, screen.config.height];
        let raw = input.take(size, screen.window.display_scale(), screen.window.pixel_density());
        let mut drawn = panels::Shown::default();
        let marker = self.marker.map(|(x, y)| input.point(x, y));
        // A figure's parts as its frames stand now (a stroke, a step back),
        // the view kept on it; the pieces' models, a new one's too.
        ed.follow_figure();
        view.follow_origin(ed);
        figures.follow(&ed.dir, ed);
        ed.sizes = figures.sizes();
        view.aspect = screen.aspect();
        let out = gui.run_ui(raw, |ui| {
            drawn = panels::ui(ui, ed, view, figures, thumbs);
            let free = room_of(&drawn, ui.ctx().content_rect()).0;
            panels::joints(ui, ed, view, free);
            panels::marks(ui, ed, view, figures, free);
            if let Some(tip) = marker {
                crate::gui::marker::paint(ui.ctx(), tip);
            }
        });
        let acted = !drawn.clicked.is_empty() || drawn.renamed.is_some();
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
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        // A picture asked for is taken once the panels have settled.
        let capture = self.capture.is_some_and(|late| late || (!acted && !delay.is_zero()));
        // A click changes what the panels show: once more at once.
        self.repaint_at = if acted { Some(Instant::now()) } else { Instant::now().checked_add(delay) };

        // The world brought up to the editor, the frame drawn.
        match shown.update(ed) {
            Ok(true) => renderer.rebuild_all(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES),
            Ok(false) => {
                renderer.update_chunks(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES);
            }
            Err(e) => ed.message = Some(format!("{e:#}")),
        }
        let frame = view::frame(ed, view, shown, figures, &cfg.light, screen.aspect());
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
        let dir = game_with_pieces("pieces-kept", "  c.camera.turns = \"free\"\n", crate::grid::tests::PIECES);
        let text = std::fs::read_to_string(dir.join("level.toml")).unwrap();
        let (mut ed, ..) = open::<crate::Voxcade>(&dir, Some("level.toml"), None, false).unwrap();
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
        assert_eq!(aligned, text.replace("turn = 45", "facing = \"east\""), "written at once, nothing else changed");
        assert_eq!(ed.steps(), 1);
        ed.undo();
        assert_eq!(std::fs::read_to_string(dir.join("level.toml")).unwrap(), text, "a step back");
    }
}
