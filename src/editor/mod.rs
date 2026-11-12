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
pub mod model;
pub mod panels;
pub mod view;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::game::Game;
use model::{Editor, Layer, Scope};
use view::{Figures, Shown, View};

/// How `voxcade edit --shot` draws the editor without a window.
pub struct ShotOptions {
    /// The level to open (its file in the game folder: `levels/02_crypt.json`),
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
    /// drawn again.
    pub stamp: bool,
    /// The level's things, not its blocks.
    pub things: bool,
    /// The tool chosen (its word: box, sphere, add ...), and for BOX and
    /// SPHERE the size of the shape held at the mouse.
    pub tool: Option<String>,
    pub held: Option<glam::IVec3>,
    /// The camera turned (yaw) and tilted (pitch), in degrees.
    pub view: Option<(f32, f32)>,
    pub width: u32,
    pub height: u32,
    pub out: PathBuf,
}

/// Opens the editor on the game in `dir`, on its `level` (a file in the
/// game folder; else the one its config names), at `block` if given, else
/// on the level's blocks or its `things`.
fn open<G: Game>(dir: &Path, level: Option<&str>, block: Option<&str>, things: bool) -> Result<(Editor, View, crate::config::GameConfig)> {
    let cfg = crate::session::config::<G>(dir)?;
    let file = match level {
        Some(file) => file.to_string(),
        None => model::level_file(&cfg)?,
    };
    let mut ed = Editor::open(dir, &file)?;
    ed.kinds = cfg.things.keys().cloned().collect();
    ed.held_kind = ed.kinds.first().cloned();
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
        if ed.forms() {
            ed.held_form = Some(model::Held { size: opts.held.unwrap_or(glam::IVec3::new(5, 5, 3)).max(glam::IVec3::ONE), at: None });
        }
    }
    let aspect = opts.width as f32 / opts.height as f32;
    if let Some((yaw, pitch)) = opts.view {
        let orbit = view.orbit(ed.scope);
        orbit.yaw = yaw;
        orbit.pitch = pitch.clamp(-85.0, 85.0);
    }
    let mut shown = Shown::new(&ed)?;
    let figures = Figures::load(dir, &cfg);
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
    // With --stamp twice: the shape set, then all drawn again.
    for pass in 0..=opts.stamp as usize {
        for step in 0..120 {
            let mut raw = input.take([opts.width, opts.height], 1.0, 1.0);
            raw.time = Some((pass * 120 + step) as f64 / 60.0);
            let out = ctx.run_ui(raw, |ui| {
                drawn = panels::ui(ui, &ed, &view, &figures, &mut thumbs);
                panels::joints(ui, &ed, &view, &drawn.rects);
            });
            let settled = step >= 2 && out.viewport_output.get(&egui::ViewportId::ROOT).is_none_or(|v| v.repaint_delay == Duration::MAX);
            if settled {
                last = Some(out);
                break;
            }
            let (device, queue, _) = canvas.gpu();
            painter.skip(device, queue, out);
        }
        view.inset = inset(&drawn.rects, ctx.content_rect());
        // The mouse in the middle of the view.
        let (mx, my) = (0.5 - view.inset.x / 2.0, 0.5 - view.inset.y / 2.0);
        if among_things(&ed) {
            (view.spot, view.under) = view.aim_things(&ed, &shown, &figures, aspect, mx, my);
        } else {
            view.hover = view.aim(&ed, aspect, mx, my);
        }
        if opts.stamp && pass == 0 {
            if !ed.forms() {
                bail!("--stamp sets BOX or SPHERE: --tool box or --tool sphere");
            }
            ed.place_form(view.hover, false, false);
            ed.drop_form();
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

/// How much of the window (`screen`, egui's points) the panels at `rects`
/// take: the right one across, the bottom one up.
pub(crate) fn inset(rects: &[egui::Rect], screen: egui::Rect) -> glam::Vec2 {
    match rects {
        [right, bottom, ..] => glam::Vec2::new(right.width() / screen.width().max(1.0), bottom.height() / screen.height().max(1.0)),
        _ => glam::Vec2::ZERO,
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
    let mut window = match model {
        Some(file) => EditorWindow::open_model::<G>(&video, dir, file, None)?,
        None => EditorWindow::open::<G>(&video, dir, level, block, things, None)?,
    };
    let mut events = sdl.event_pump().context("SDL events")?;
    loop {
        window.draw()?;
        // Drawn only after something happened (or when its panels ask):
        // the editor sits still otherwise.
        let first = match window.wakes_at() {
            None => Some(events.wait_event()),
            Some(at) => events.wait_event_timeout(at.saturating_duration_since(Instant::now()).max(Duration::from_millis(1))),
        };
        for event in first.into_iter().chain(events.poll_iter().collect::<Vec<_>>()) {
            let shift = shift_held(&events);
            if !window.handle(&event, shift) {
                return Ok(());
            }
        }
    }
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
    /// BOX and SPHERE: where the mouse was when the shape was drawn (it
    /// stays there until the mouse moves on).
    drawn_at: Option<model::Hit>,
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
        let figures = Figures::load(dir, &cfg);
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
            drawn_at: None,
            dirty: true,
            repaint_at: None,
            marker: None,
            capture: None,
            picture: None,
        })
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

        self.input.handle(event, || None);
        let Self { screen, renderer, ed, view, shown, figures, keys, rects, gui, input, steps_at_click, drawn_at, .. } = self;
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
            Event::Window { win_event: WindowEvent::FocusLost, .. } => view.cut = false,
            Event::KeyDown { keycode: Some(key), keymod, .. } if !gui.egui_wants_keyboard_input() => {
                let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD);
                let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                match key {
                    // BOX and SPHERE: the shape a voxel deeper or shallower.
                    Keycode::Plus | Keycode::Equals | Keycode::KpPlus if ed.forms() => ed.deepen(1, view.hover),
                    Keycode::Minus | Keycode::KpMinus if ed.forms() => ed.deepen(-1, view.hover),
                    Keycode::Escape if ed.forms() && ed.drop_form() => *drawn_at = None,
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
                } else if mousestate.right() {
                    view.orbit(ed.scope).turn(dx, dy);
                }
                let over = over(x, y) && !mousestate.left();
                if among_things(ed) {
                    (view.spot, view.under) = if over { (None, None) } else { view.aim_things(ed, shown, figures, aspect, sx, sy) };
                    if mousestate.left() {
                        ed.drag_thing(view.spot);
                    }
                } else if ed.forms() {
                    // BOX and SPHERE: the footprint dragged out on its face's
                    // plane; a shape drawn follows the mouse once it moves on.
                    view.hover = if over { None } else { view.aim(ed, aspect, sx, sy) };
                    if mousestate.left() && ed.drawing.is_some() {
                        let (origin, dir) = view.ray(ed, aspect, sx, sy);
                        ed.draw_form(origin, dir);
                    } else if drawn_at.is_some() && view.hover != *drawn_at {
                        *drawn_at = None;
                        ed.let_form_follow();
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
                if among_things(ed) {
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
            // BOX and SPHERE: a click without a drag sets the shape held
            // (Shift: only where empty; Ctrl: cut away), a drag holds the
            // footprint as the shape, where it was drawn.
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } if ed.forms() => {
                if let Some(d) = ed.drawing {
                    if d.end == d.start && ed.held_form.is_some() {
                        ed.drawing = None;
                        ed.place_form(view.hover, view.cut, shift);
                    } else {
                        ed.hold_drawn();
                        *drawn_at = view.hover;
                    }
                }
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } => ed.release(),
            // Over the panels the wheel scrolls them (egui), else it zooms.
            Event::MouseWheel { y, mouse_x, mouse_y, .. } if !over(mouse_x, mouse_y) => view.orbit(ed.scope).zoom_by(y),
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
        // the view kept on it.
        ed.follow_figure();
        view.follow_origin(ed);
        let out = gui.run_ui(raw, |ui| {
            drawn = panels::ui(ui, ed, view, figures, thumbs);
            panels::joints(ui, ed, view, &drawn.rects);
            if let Some(tip) = marker {
                crate::gui::marker::paint(ui.ctx(), tip);
            }
        });
        let acted = !drawn.clicked.is_empty();
        for action in drawn.clicked {
            view::act(ed, view, action);
        }
        view.inset = inset(&drawn.rects, gui.content_rect());
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
