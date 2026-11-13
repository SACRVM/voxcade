//! `voxcade edit --shot`: the editor drawn into a PNG without a window,
//! the changes it tries first, by its flags; and the changes `voxcade
//! edit` writes without a window (`--clear`, `--fill`, `--scatter`).

use super::*;

/// How `voxcade edit --shot` draws the editor without a window.
#[derive(Default)]
pub struct ShotOptions {
    /// The level to open (its file in the game folder: `levels/02_crypt.toml`),
    /// else the one `c.world.blocks.file` names.
    pub level: Option<String>,
    /// The block to open (its name), else the level.
    pub block: Option<String>,
    /// A model to open instead (its path in the game folder); of a figure
    /// of parts the part chosen, shaped in the figure, and its parent and
    /// joint set by hand (the joint as the file, the JOINT row and the
    /// printed tree have it, from the pivot); nothing written.
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
    /// The chosen one's handles in ROTATE (its rings), else MOVE.
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
        // Only a rule that sets pieces: else a mistake naming the rules.
        let rules: Vec<String> = ed.terrain().map(|t| t.scatter.iter().enumerate().map(|(i, r)| format!("{} ({}: {})", i + 1, r.makes().word(), r.models.join(", "))).collect()).unwrap_or_default();
        let fits = ed.terrain().and_then(|t| t.scatter.get(k.wrapping_sub(1))).is_some_and(|r| r.makes() == crate::terrain::ScatterAs::Pieces);
        if !fits {
            let there = if rules.is_empty() { "the level has no scatter rules".to_string() } else { format!("its scatter rules are {}", rules.join("; ")) };
            bail!("--scatter {k}: no scatter rule {k} that sets pieces: {there}");
        }
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
    let find = |ed: &Editor, name: &str| ed.find_listed(name).with_context(|| format!("the level has nothing called '{name}'"));
    // What hangs on the mouse set down exactly at a spot.
    let set_at = |ed: &mut Editor, at: glam::IVec3| {
        match &mut ed.pieces.hanging {
            Some(Hanging::Thing(t)) => t.at = at.to_array(),
            Some(Hanging::Place(p)) => p.at = at.to_array(),
            Some(Hanging::Area(a)) => a.at = at.to_array(),
            None => {}
        }
        ed.pieces.hang_spot = Some(at);
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
                anyhow::ensure!(ed.pieces.free_turns, "--rotate {name} --by {by}: this game turns things on the grid, in quarter turns (c.camera.rotates = \"free\" for any angle)");
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

/// The part of a figure `name` names (`--part`, `--parent` ...).
fn part_of(ed: &model::Editor, name: &str) -> Result<usize> {
    ed.figure.as_ref().context("--part: the model is no figure of parts (its objects have no names)")?.parts().iter().position(|p| p.name == name).with_context(|| format!("the figure has no part '{name}'"))
}

/// `--part`, `--parent`, `--joint`: the part chosen, its parent and its
/// joint set by hand (the joint from the pivot, as `--help`, the file,
/// the JOINT row and the printed tree have it).
pub(super) fn choose_part(ed: &mut Editor, view: &mut View, opts: &ShotOptions) -> Result<()> {
    if let Some(name) = &opts.part {
        let p = part_of(ed, name)?;
        view::act(ed, view, view::Action::Part(p));
        if let Some(parent) = &opts.parent {
            let q = part_of(ed, parent)?;
            ed.set_parent(Some(q));
        }
        if let Some(at) = opts.joint {
            ed.set_joint(Some(at));
        }
        ed.follow_figure();
        view.follow_origin(ed);
    } else if opts.parent.is_some() || opts.joint.is_some() {
        bail!("--parent and --joint set the rig of the part --part names");
    }
    Ok(())
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
    choose_part(&mut ed, &mut view, opts)?;
    shape_parts(&mut ed, &mut view, opts)?;
    tool_and_selection(&mut ed, &mut view, opts)?;
    let aspect = opts.width as f32 / opts.height as f32;
    in_context(dir, &mut ed, opts)?;
    let mut figures = Figures::load(dir, &cfg).with_pieces(dir, &ed.level);
    figures.follow(dir, &ed);
    try_pieces(dir, &mut ed, &mut figures, opts)?;
    show_terrain(&mut ed, opts)?;
    set_view(&mut ed, &mut view, opts, aspect)?;
    cells_and_slice(&mut ed, &mut view, opts, aspect)?;
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
        ed.pieces.sizes = figures.sizes();
        let out = settle(&ctx, &mut input, [opts.width, opts.height], pass, |ui| {
            drawn = panels::ui(ui, &ed, &view, &figures, &mut thumbs);
            let free = room_of(&drawn, ui.ctx().content_rect()).0;
            panels::joints(ui, &ed, &view, free);
            panels::marks(ui, &ed, &view, &figures, free);
        }, |out| {
            let (device, queue, _) = canvas.gpu();
            painter.skip(device, queue, out);
        });
        last = Some(out);
        view.set_room(room_of(&drawn, ctx.content_rect()).1);
        // The mouse in the middle of the view.
        let (mx, my) = (0.5 - view.inset.x / 2.0, 0.5 - view.inset.y / 2.0);
        // Among the pieces after the panels are laid out: a menu open, a
        // handle under the mouse (the room is known now).
        if pass == 0 {
            pointer_flags(&mut ed, &mut view, &figures, &shown, opts, ctx.content_rect(), (mx, my), aspect)?;
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
    print_facts(&ed, &view, &ctx, opts, frame.instances.len(), keys);
    Ok(())
}

/// The panels laid out over frames a sixtieth of a second apart (pass
/// `pass` of the shot) until egui is settled, the frames before handed to
/// `skip`: the last one's output. Two seconds at most: what never settles
/// (a text field in focus, its cursor blinking: ADD MODEL's find) is drawn
/// as it is then, never left out.
fn settle(ctx: &egui::Context, input: &mut crate::gui::input::Input, size: [u32; 2], pass: usize, mut ui: impl FnMut(&mut egui::Ui), mut skip: impl FnMut(egui::FullOutput)) -> egui::FullOutput {
    const FRAMES: usize = 120;
    let mut step = 0;
    loop {
        let mut raw = input.take(size, 1.0, 1.0);
        raw.time = Some((pass * FRAMES + step) as f64 / 60.0);
        let out = ctx.run_ui(raw, &mut ui);
        let settled = step >= 2 && out.viewport_output.get(&egui::ViewportId::ROOT).is_none_or(|v| v.repaint_delay == Duration::MAX);
        if settled || step + 1 == FRAMES {
            return out;
        }
        skip(out);
        step += 1;
    }
}

/// `--add-part`, `--mirror`, `--merge-into`, `--rotate-part`, `--pose-part`,
/// `--move-part` and `--move-joint`: a figure's parts changed, in turn.
fn shape_parts(ed: &mut Editor, view: &mut View, opts: &ShotOptions) -> Result<()> {
    if opts.add_part {
        ed.add_part();
    }
    if opts.mirror {
        let p = ed.figure.as_ref().and_then(|f| f.selected).context("--mirror mirrors the part --part chose")?;
        if let Some(why) = ed.cannot_mirror(p) {
            bail!("--mirror: {why}");
        }
        ed.mirror_part(p);
        view.follow_origin(ed);
    }
    if let Some(into) = &opts.merge_into {
        let src = ed.figure.as_ref().and_then(|f| f.selected).context("--merge-into NAME merges the part --part chose")?;
        let into = part_of(ed, into)?;
        if let Some(why) = ed.cannot_merge(src, into) {
            bail!("--merge-into: {why}");
        }
        ed.merge_part(src, into);
    }
    for (name, by, about) in &opts.part_turns {
        let p = part_of(ed, name)?;
        if ed.figure.as_ref().and_then(|f| f.selected) != Some(p) {
            view::act(ed, view, view::Action::Part(p));
        }
        let now = ed.part_rotation().context("--rotate-part: no figure's part chosen")?;
        ed.set_part_rotation(*about, now[*about] + by);
        ed.follow_figure();
        view.follow_origin(ed);
    }
    for (name, by, about) in &opts.part_poses {
        let p = part_of(ed, name)?;
        if !ed.rigging.posing {
            ed.set_posing(true);
        }
        if ed.figure.as_ref().and_then(|f| f.selected) != Some(p) {
            view::act(ed, view, view::Action::Part(p));
        }
        let now = ed.chosen_pose().context("--pose-part: no figure's part chosen")?;
        ed.set_pose(*about, now[*about] + by);
        view.follow_origin(ed);
    }
    // Rotated or posed without --part: the figure whole after.
    if (!opts.part_turns.is_empty() || !opts.part_poses.is_empty()) && opts.part.is_none() {
        view::act(ed, view, view::Action::WholeFigure);
        ed.follow_figure();
        view.follow_origin(ed);
    }
    for (name, by, joint) in &opts.part_moves {
        let p = part_of(ed, name)?;
        if ed.figure.as_ref().and_then(|f| f.selected) != Some(p) {
            view::act(ed, view, view::Action::Part(p));
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
        view.follow_origin(ed);
    }
    Ok(())
}

/// `--tool`, `--background`, `--select` and `--selection`, `--shift`: the
/// tool chosen, the view's background, the selection and what is done to it.
fn tool_and_selection(ed: &mut Editor, view: &mut View, opts: &ShotOptions) -> Result<()> {
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
                let p = part_of(ed, &rest[3..])?;
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
        view.follow_origin(ed);
    }
    if opts.shift {
        ed.shift_pick(true);
    }
    Ok(())
}

/// `--context LEVEL:NAME` (`--context-off`): a model shaped in place.
fn in_context(dir: &Path, ed: &mut Editor, opts: &ShotOptions) -> Result<()> {
    if let Some((level, name)) = &opts.context {
        anyhow::ensure!(ed.model.is_some(), "--context shapes a model in place: --model FILE");
        let lv = crate::grid::GridLevel::read(&dir.join(level))?;
        let piece = model::room::listed(&lv.things, name).with_context(|| format!("--context {level}:{name}: the level has no piece called so"))?;
        ed.set_context(context::Seed { level: level.clone(), piece, hidden: Default::default() })?;
        if opts.context_off {
            ed.toggle_context();
        }
    }
    Ok(())
}

/// Among the pieces: the changes tried, the one chosen, those hidden.
fn try_pieces(dir: &Path, ed: &mut Editor, figures: &mut Figures, opts: &ShotOptions) -> Result<()> {
    // The changes tried among the pieces, then the one chosen, the view.
    // (ADD MODEL's pictures are the pieces' too.)
    if !opts.ops.is_empty() || opts.choose.is_some() || opts.picker {
        if ed.level.has_grid() {
            ed.switch(Layer::Pieces);
        }
        anyhow::ensure!(ed.question.is_none(), "the level asks first whether to align its things (--align)");
    }
    for op in &opts.ops {
        figures.follow(dir, ed);
        ed.pieces.sizes = figures.sizes();
        try_op(ed, figures, op)?;
        figures.follow(dir, ed);
    }
    if let Some(name) = &opts.choose {
        let item = ed.find_listed(name).with_context(|| format!("--choose {name}: the level has nothing called so"))?;
        ed.choose_item(Some(item));
    }
    for name in &opts.hide {
        let item = ed.find_listed(name).with_context(|| format!("--hide {name}: the level has nothing called so"))?;
        ed.set_hidden(item, true);
    }
    Ok(())
}

/// `--terrain`, `--raise`, `--shape`, `--material`, `--scatter`.
fn show_terrain(ed: &mut Editor, opts: &ShotOptions) -> Result<()> {
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
    Ok(())
}

/// The view as the flags set it: GAME VIEW, the chosen one in view, its
/// light, LEVEL, the handles, ADD's menu, the camera, ADD MODEL, the lights.
fn set_view(ed: &mut Editor, view: &mut View, opts: &ShotOptions, aspect: f32) -> Result<()> {
    view.aspect = aspect;
    // (Where the panels will lie, for the view's middle: as in a window.)
    let (w, h) = (opts.width as f32, opts.height as f32);
    view.set_room([zones::OUTLINER / w, zones::TOP / h, zones::INSPECTOR / w, 0.0]);
    if opts.game_view {
        view::act(ed, view, view::Action::GameView);
    }
    if opts.choose.is_some() {
        view.bring_into_view(ed);
    }
    if let Some(k) = opts.choose_light {
        ed.choose_light(Some(k));
        anyhow::ensure!(ed.pieces.chosen_light == Some(k), "--choose-light {k}: the chosen piece has no light {k} (they count from 0)");
    }
    view.level_settings = opts.level_settings;
    if opts.turn_handles && ed.part_mode().is_some() {
        // A figure's part: its rings.
        ed.rigging.part_rotating = true;
    } else if opts.turn_handles {
        view::act(ed, view, view::Action::HandleMode(true));
        anyhow::ensure!(view.turn_mode(ed), "--handles rotate: what is chosen does not turn (a piece of a model, a place or a figure's part does)");
    }
    if opts.add_menu {
        view::act(ed, view, view::Action::AddMenu);
    }
    if let Some((yaw, pitch)) = opts.view {
        let orbit = view.orbit(ed.scope);
        orbit.yaw = yaw;
        orbit.pitch = pitch.clamp(-90.0, 90.0);
    }
    (view.picker, view.lights) = (opts.picker, !opts.lights_off);
    Ok(())
}

/// `--clear`, `--fill` (shown, not written) and `--slice`.
fn cells_and_slice(ed: &mut Editor, view: &mut View, opts: &ShotOptions, aspect: f32) -> Result<()> {
    // Cells changed, to look at (nothing written).
    if !opts.cells.is_empty() {
        let model = ed.model.as_mut().context("--clear and --fill change a model: --model FILE")?;
        let changed = model.change_cells(&cells_of(opts), &opts.cells)?;
        println!("{}: {changed} (shown, not written)", opts.model.as_deref().unwrap_or_default());
        ed.follow_figure();
        view.follow_origin(ed);
    }
    if let Some((axis, at)) = opts.slice {
        anyhow::ensure!(ed.can_slice(), "--slice cuts a block, a model or a figure");
        ed.set_slice(Some(axis));
        ed.set_slice_at(at);
        ed.slice_toward(view.block_eye(aspect));
    }
    Ok(())
}

/// In the first pass, the panels laid out: `--menu`, `--hover`, `--click`
/// and `--marquee` at the mouse in the middle of the view (`mx`, `my`).
#[allow(clippy::too_many_arguments)]
fn pointer_flags(ed: &mut Editor, view: &mut View, figures: &Figures, shown: &Shown, opts: &ShotOptions, screen: egui::Rect, (mx, my): (f32, f32), aspect: f32) -> Result<()> {
    let (w, h) = (opts.width as f32, opts.height as f32);
    if let Some(name) = &opts.menu {
        let at = egui::pos2(screen.left() + mx * screen.width(), screen.top() + my * screen.height());
        if name == "keys" {
            // F1's page of the keys.
            view.keys_shown = true;
        } else if let Some(part) = name.strip_prefix("part:") {
            // A figure's part's menu (its row's, a right click).
            let p = part_of(ed, part)?;
            view.menu = Some(view::Menu { on: view::MenuOn::Part(p), at: [zones::OUTLINER - 40.0, 120.0] });
        } else if name == "selection" {
            // The selection's (a right click on it), at the middle.
            anyhow::ensure!(ed.selection().is_some(), "--menu selection: nothing selected (--select)");
            view.menu = Some(view::Menu { on: view::MenuOn::Selection, at: [at.x, at.y] });
        } else if !ed.among_pieces() {
            // The voxel editor's, of what is under the middle.
            anyhow::ensure!(name == "voxel", "--menu {name}: not in a level of blocks or the voxel editor, which have --menu voxel (the voxel, the part or the block under the middle of the view), part:NAME, selection and keys (floor and a thing's name are among the pieces)");
            let on = voxel_menu(ed, view, aspect, mx, my).context("--menu voxel: nothing under the middle of the view")?;
            view.menu = Some(view::Menu { on, at: [at.x, at.y] });
        } else if name == "floor" {
            let (origin, dir) = view.voxel_ray(aspect, mx, my);
            let spot = view::floor_spot(&shown.world, origin, dir).context("--menu floor: no floor under the middle of the view")?;
            view.menu = Some(view::Menu { on: view::MenuOn::Floor(spot), at: [at.x, at.y] });
        } else {
            let item = ed.find_listed(name).with_context(|| format!("--menu {name}: the level has nothing called so"))?;
            // At the thing as it stands on the screen.
            let at = stage::handles_on_screen(ed, view, screen).map_or(at, |(_, _, b)| b.center());
            view::act(ed, view, view::Action::OpenMenu(item, [at.x, at.y]));
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
        let picked = view::pick_along(ed, figures, origin, dir);
        if stage::grip_under(ed, view, screen, at).is_some() {
            println!("click {x},{y}: a handle of the chosen one");
        } else {
            ed.choose_item(picked.map(|(item, _)| item));
            println!("click {x},{y}: {}", picked.map_or("nothing".to_string(), |(item, _)| ed.label_of(item).0));
        }
    }
    // SELECT's rectangle dragged there (points of the shot): what
    // it takes selected, its outline drawn.
    if let Some([x0, y0, x1, y1]) = opts.marquee {
        let vp = view.view_proj(&view.block, aspect);
        ed.select_rect(vp, glam::Vec2::new(w, h), glam::Vec2::new(x0, y0), glam::Vec2::new(x1, y1), ed.selecting.select_mode, ed.selecting.select_through);
        view.marquee = Some(view::Marquee { from: [x0 / w, y0 / h], to: [x1 / w, y1 / h], mode: ed.selecting.select_mode });
        println!("marquee: {} voxels selected", ed.selection().map_or(0, |s| s.count()));
    }
    Ok(())
}

/// What a shot prints: the level and the cubes drawn, the aim; among the
/// pieces where the handles and the buttons lie and what is set down; a
/// figure's parts.
fn print_facts(ed: &Editor, view: &View, ctx: &egui::Context, opts: &ShotOptions, cubes: usize, keys: usize) {
    let s = ed.level.size;
    let aim = if among_things(ed) { format!("spot {:?}, over {:?}", view.spot, view.under.map(|i| &ed.level.things[i].kind)) } else { format!("{:?}", view.hover) };
    println!(
        "{}: {} by {} by {} cells, {} blocks, {} things | {} cubes, {} keys on the panels | aim {}",
        opts.out.display(),
        s.x,
        s.y,
        s.z,
        ed.level.blocks.len(),
        ed.level.things.len(),
        cubes,
        keys,
        aim
    );
    // Among the pieces: what is set down, the chosen one first; where the
    // handles and the buttons beside the chosen one lie (to count how far
    // the mouse goes).
    if ed.among_pieces() {
        let screen = ctx.content_rect();
        if let Some((_, _, b)) = stage::handles_on_screen(ed, view, screen) {
            println!("handles: {:.0},{:.0} to {:.0},{:.0}", b.left(), b.top(), b.right(), b.bottom());
        }
        if let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("stage beside rect"))).filter(|_| ed.pieces.chosen.is_some()) {
            println!("buttons: {:.0},{:.0} to {:.0},{:.0}", r.left(), r.top(), r.right(), r.bottom());
        }
        let lights: usize = ed.level.things.iter().map(|t| t.lights.len()).sum();
        println!("pieces: {} things ({lights} lights), {} places, {} areas, the level {:?} voxels", ed.level.things.len(), ed.level.places.len(), ed.level.areas.len(), ed.level.voxels().to_array());
        if let Some(item) = ed.pieces.chosen {
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
            let light = if t.is_some_and(|t| t.shadow_only) { format!("{light}, shadow only") } else { light };
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field in focus never lets egui settle (its cursor blinks): the
    /// shot draws the panels as they are after two seconds, not nothing
    /// (ADD MODEL's find over a game of many models drew no panel at all).
    #[test]
    fn panels_that_never_settle_are_drawn_all_the_same() {
        let ctx = egui::Context::default();
        let mut input = crate::gui::input::Input::default();
        let mut text = String::new();
        let mut skipped = 0;
        let out = settle(&ctx, &mut input, [640, 360], 0, |ui| {
            ui.label("ADD MODEL");
            ui.add(egui::TextEdit::singleline(&mut text)).request_focus();
        }, |_| skipped += 1);
        assert_eq!(skipped, 119, "it never settled");
        assert!(!out.shapes.is_empty(), "the last frame is drawn");
    }

    /// An unnamed thing is found by what the list shows for it (`--choose
    /// torch`, `torch#3` the third), a name before that.
    #[test]
    fn things_are_found_by_the_name_the_list_shows() {
        let ed = Editor::open(Path::new("games/voxkeep"), "levels/01_hall.toml").unwrap();
        let torches: Vec<usize> = ed.level.things.iter().enumerate().filter(|(_, t)| t.name.is_none() && t.kind == "torch").map(|(i, _)| i).collect();
        assert!(torches.len() >= 3);
        assert_eq!(ed.find_listed("torch"), Some(Item::Thing(torches[0])));
        assert_eq!(ed.find_listed("torch#3"), Some(Item::Thing(torches[2])));
        assert_eq!((ed.find("torch"), ed.find_listed("torch#99"), ed.find_listed("torch#0")), (None, None, None), "its own name only, no such one");
    }

    /// `--scatter N` without such a rule is a mistake naming the rules
    /// there are (it said "0 pieces set down").
    #[test]
    fn scatter_without_such_a_rule_names_the_rules() {
        let opts = ShotOptions { level: Some("levels/01_hall.toml".into()), scatter: vec![1], dry: true, ..Default::default() };
        let e = scatter_level::<crate::Voxcade>(Path::new("games/voxkeep"), &opts).unwrap_err();
        assert!(format!("{e:#}").contains("--scatter 1: no scatter rule 1 that sets pieces: the level has no scatter rules"), "{e:#}");
    }
}
