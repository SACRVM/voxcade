//! A big model in the voxel editor (a back wall of 169 by 12 by 116, the
//! owner's case: "ruckelig wie sau"): what a frame, a hover and a stroke
//! cost there.

use super::*;
use std::time::Instant;

/// A wall like the owner's back wall: 169 wide, 12 deep, 116 high, solid
/// stone in a few colors with windows cut through.
pub(crate) fn wall_text(w: i32, d: i32, h: i32) -> String {
    let mut layers = Vec::new();
    for z in 0..h {
        let mut rows = Vec::new();
        for y in 0..d {
            let row: String = (0..w)
                .map(|x| {
                    let window = (z % 40) >= 12 && (z % 40) < 32 && (x % 28) >= 8 && (x % 28) < 20;
                    if window {
                        '.'
                    } else if z % 6 == 0 || (x + (z / 6) * 7) % 14 == 0 {
                        'm'
                    } else {
                        ['a', 'b', 'c'][((x * 7 + y * 3 + z * 11) % 3) as usize]
                    }
                })
                .collect();
            rows.push(row);
        }
        layers.push(rows.join("\n"));
    }
    format!("format = \"{}\"\nsize = [{w}, {d}, {h}]\n\n[palette]\na = \"#8c8272\"\nb = \"#7a7062\"\nc = \"#958b7b\"\nm = \"#5a5248\"\n\n[[frame]]\nlayers = \"\"\"\n{}\n\"\"\"\n", crate::vvx::FORMAT, layers.join("\n\n"))
}

/// A game holding the wall, an editor window without a window on it.
pub(crate) fn wall_window(name: &str, size: [i32; 3]) -> (EditorWindow, crate::scratch::Scratch) {
    let dir = crate::scratch::Scratch::new(&format!("big-{name}"));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "").unwrap();
    std::fs::write(dir.join("wall.vvx"), wall_text(size[0], size[1], size[2])).unwrap();
    let (ed, view, cfg) = open_model::<crate::Voxcade>(&dir, "wall.vvx").unwrap();
    let mut win = EditorWindow::headless(ed, view, cfg, &dir).unwrap();
    for _ in 0..3 {
        win.draw().unwrap();
    }
    (win, dir)
}

pub(crate) fn motion(win: &mut EditorWindow, x: f32, y: f32, held: u32, last: &mut (f32, f32)) {
    let (px, py) = std::mem::replace(last, (x, y));
    let e = sdl3::event::Event::MouseMotion { timestamp: 0, window_id: 0, which: 0, mousestate: sdl3::mouse::MouseState::from_sdl_state(held), x, y, xrel: x - px, yrel: y - py };
    assert!(win.handle(&e, false));
}

pub(crate) fn button(win: &mut EditorWindow, x: f32, y: f32, button: sdl3::mouse::MouseButton, down: bool) {
    let e = if down {
        sdl3::event::Event::MouseButtonDown { timestamp: 0, window_id: 0, which: 0, mouse_btn: button, clicks: 1, x, y }
    } else {
        sdl3::event::Event::MouseButtonUp { timestamp: 0, window_id: 0, which: 0, mouse_btn: button, clicks: 1, x, y }
    };
    assert!(win.handle(&e, false));
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// A GPU without a window, drawing what the window would (its meshes
/// kept as a window keeps them): to time the GPU's part.
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: crate::render::Renderer,
    view: wgpu::TextureView,
    /// Time spent meshing and uploading, and drawing (to the GPU's end).
    mesh: f64,
    render: f64,
}

impl Gpu {
    fn new(win: &mut EditorWindow) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })).ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut renderer = crate::render::Renderer::new(&device, format, 1600, 900);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: 1600, height: 900, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        renderer.rebuild_all(&device, &queue, &mut win.shown.world, &ALL_FACES);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        Some(Self { device, queue, renderer, view, mesh: 0.0, render: 0.0 })
    }

    /// What the window does after its frame was made: the meshes brought
    /// up to the world shown, the frame drawn.
    fn present(&mut self, win: &mut EditorWindow) {
        let Some((frame, whole)) = win.last.take() else { return };
        let t = Instant::now();
        if whole {
            self.renderer.rebuild_all(&self.device, &self.queue, &mut win.shown.world, &ALL_FACES);
        } else {
            self.renderer.update_chunks(&self.device, &self.queue, &mut win.shown.world, &ALL_FACES);
        }
        self.mesh += ms(t);
        let t = Instant::now();
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.renderer.render(&self.device, &self.queue, &mut encoder, &self.view, &frame);
        self.queue.submit([encoder.finish()]);
        self.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        self.render += ms(t);
    }

    fn take(&mut self) -> String {
        let (m, r) = (std::mem::take(&mut self.mesh), std::mem::take(&mut self.render));
        format!("[mesh+upload {m:.1} ms, render {r:.1} ms]")
    }
}

/// A solid box of one color as a model's text.
pub(crate) fn box_text(w: i32, d: i32, h: i32) -> String {
    let row = "a".repeat(w as usize);
    let layer = vec![row.as_str(); d as usize].join("\n");
    format!("format = \"{}\"\nsize = [{w}, {d}, {h}]\n\n[palette]\na = \"#a08060\"\n\n[[frame]]\nlayers = \"\"\"\n{}\n\"\"\"\n", crate::vvx::FORMAT, vec![layer.as_str(); h as usize].join("\n\n"))
}

/// A room like the owner's (a level of pieces: two floors, a back wall in
/// two, a left and a right wall, furniture), the back wall's first half
/// (`wall_back_1.vvx`, piece 2) to shape in place.
pub(crate) fn room_game(name: &str, by: i32) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("big-{name}"));
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "").unwrap();
    std::fs::write(dir.join("floor.vvx"), box_text(180 / by, 200 / by, (4 / by).max(1))).unwrap();
    std::fs::write(dir.join("wall_back_1.vvx"), wall_text(169 / by, 12 / by, (116 / by).max(1))).unwrap();
    std::fs::write(dir.join("wall_back_2.vvx"), wall_text(169 / by, 12 / by, (116 / by).max(1))).unwrap();
    std::fs::write(dir.join("wall_side.vvx"), wall_text(11 / by, 200 / by, (116 / by).max(1))).unwrap();
    std::fs::write(dir.join("stove.vvx"), box_text(47 / by, 45 / by, (59 / by).max(1))).unwrap();
    std::fs::write(dir.join("table.vvx"), box_text(43 / by, 51 / by, (22 / by).max(1))).unwrap();
    let pieces = [("floor.vvx", [89, 99, 0]), ("floor.vvx", [269, 99, 0]), ("wall_back_1.vvx", [95, 194, 4]), ("wall_back_2.vvx", [264, 194, 4]), ("wall_side.vvx", [5, 99, 4]), ("wall_side.vvx", [354, 99, 4]), ("stove.vvx", [31, 167, 4]), ("table.vvx", [60, 63, 4])];
    let mut text = String::from("format = \"voxcade level 1\"\nsize = [400, 240, 170]\n");
    for (k, (model, at)) in pieces.iter().enumerate() {
        text += &format!("\n[[thing]]\nname = \"p{k}\"\nmodel = \"{model}\"\nat = [{}, {}, {}]\n", at[0] / by, at[1] / by, at[2] / by);
    }
    std::fs::write(dir.join("levels/room.toml"), text).unwrap();
    dir
}

/// The room's back wall shaped in place (EDIT on its piece, CONTEXT on).
pub(crate) fn wall_in_room(name: &str, by: i32) -> (EditorWindow, crate::scratch::Scratch) {
    let dir = room_game(name, by);
    let (ed, view, cfg) = open_model::<crate::Voxcade>(&dir, "wall_back_1.vvx").unwrap();
    let mut win = EditorWindow::headless(ed, view, cfg, &dir).unwrap();
    win.set_context(context::Seed { level: "levels/room.toml".into(), piece: 2, hidden: Default::default() });
    for _ in 0..3 {
        win.draw().unwrap();
    }
    (win, dir)
}

/// The wall as a figure of parts: the wall and a lamp before it.
pub(crate) fn wall_figure_window(name: &str) -> (EditorWindow, crate::scratch::Scratch) {
    let dir = crate::scratch::Scratch::new(&format!("big-{name}"));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "").unwrap();
    let wall = wall_text(169, 12, 116);
    let layers = &wall[wall.find("layers").unwrap()..];
    let lamp = vec![["ccc"; 3].join("\n"); 3].join("\n\n");
    let text = format!(
        "format = \"{}\"\nsize = [169, 18, 116]\n\n[palette]\na = \"#8c8272\"\nb = \"#7a7062\"\nc = \"#958b7b\"\nm = \"#5a5248\"\n\n[[part]]\nname = \"wall\"\nat = [-84, -6, 0]\n{layers}\n[[part]]\nname = \"lamp\"\nat = [0, -9, 50]\nlayers = \"\"\"\n{lamp}\n\"\"\"\n",
        crate::vvx::FORMAT
    );
    std::fs::write(dir.join("wall.vvx"), text).unwrap();
    let (ed, view, cfg) = open_model::<crate::Voxcade>(&dir, "wall.vvx").unwrap();
    let mut win = EditorWindow::headless(ed, view, cfg, &dir).unwrap();
    for _ in 0..3 {
        win.draw().unwrap();
    }
    (win, dir)
}

/// A row of the screen (y, from x to x) where the model is met all along.
fn run_on(win: &EditorWindow) -> (f32, f32, f32) {
    let mut best = (0.0, 0.0, 0.0);
    for j in 0..90 {
        let y = 100.0 + j as f32 * 8.0;
        let mut start = None;
        for i in 0..=160 {
            let x = 10.0 * i as f32;
            let hit = i < 160 && win.view.aim(&win.ed, 16.0 / 9.0, x / 1600.0, y / 900.0).is_some();
            match (hit, start) {
                (true, None) => start = Some(x),
                (false, Some(s)) => {
                    if x - 10.0 - s > best.2 - best.1 {
                        best = (y, s, x - 10.0);
                    }
                    start = None;
                }
                _ => {}
            }
        }
    }
    best
}

/// Plays hovering, orbiting, strokes, undo and SELECT on the window and
/// prints what each cost.
fn exercise(mut win: EditorWindow) {
    let gpu = std::cell::RefCell::new(Gpu::new(&mut win).expect("a GPU"));
    let draw = |win: &mut EditorWindow| {
        win.draw().unwrap();
        gpu.borrow_mut().present(win);
    };
    let g = || gpu.borrow_mut().take();
    let instances = |win: &EditorWindow| win.last.as_ref().map_or(0, |l| l.0.instances.len());
    let (y, x0, x1) = run_on(&win);
    println!("the model met along y {y} from x {x0} to {x1}");
    let mut last = (800.0, 450.0);
    let t = Instant::now();
    for _ in 0..10 {
        win.dirty = true;
        win.draw().unwrap();
    }
    println!("idle frame: {:.2} ms, instances {}", ms(t) / 10.0, instances(&win));
    gpu.borrow_mut().present(&mut win);
    g();
    view::act(&mut win.ed, &mut win.view, view::Action::Tool(Tool::Add));
    draw(&mut win);
    g();
    let t = Instant::now();
    for k in 0..40 {
        motion(&mut win, x0 + (x1 - x0) * k as f32 / 40.0, y, 0, &mut last);
        draw(&mut win);
    }
    println!("hover frame: {:.2} ms (hover {:?}) {}", ms(t) / 40.0, win.view.hover.map(|h| h.cell), g());
    button(&mut win, 800.0, 450.0, sdl3::mouse::MouseButton::Right, true);
    let t = Instant::now();
    let (mut handled, mut drawing) = (0.0, 0.0);
    for k in 0..20 {
        let h = Instant::now();
        motion(&mut win, 800.0 + k as f32 * 3.0, 450.0, 0b100, &mut last);
        handled += ms(h);
        let h = Instant::now();
        win.draw().unwrap();
        drawing += ms(h);
        gpu.borrow_mut().present(&mut win);
    }
    println!("orbit frame: {:.2} ms (the motion's handling {:.2} ms, draw {:.2} ms) {}", ms(t) / 20.0, handled / 20.0, drawing / 20.0, g());
    button(&mut win, 860.0, 450.0, sdl3::mouse::MouseButton::Right, false);
    draw(&mut win);
    let (y, x0, x1) = run_on(&win);
    for tool in [Tool::Paint, Tool::Add, Tool::Remove] {
        view::act(&mut win.ed, &mut win.view, view::Action::Tool(tool));
        draw(&mut win);
        motion(&mut win, x0, y, 0, &mut last);
        draw(&mut win);
        g();
        let rebuilds = win.rebuilds;
        let t = Instant::now();
        button(&mut win, x0, y, sdl3::mouse::MouseButton::Left, true);
        draw(&mut win);
        let press = ms(t);
        let t = Instant::now();
        let mut worst = 0.0f64;
        for k in 1..=30 {
            let s = Instant::now();
            motion(&mut win, x0 + (x1 - x0) * k as f32 / 30.0, y, 1, &mut last);
            draw(&mut win);
            worst = worst.max(ms(s));
        }
        let drag = ms(t) / 30.0;
        let t = Instant::now();
        button(&mut win, x1, y, sdl3::mouse::MouseButton::Left, false);
        draw(&mut win);
        println!("{tool:?}: press {press:.1} ms, drag frame {drag:.2} ms (worst {worst:.1}), let go (saved) {:.1} ms, steps {}, worlds built whole {} {}", ms(t), win.ed.steps(), win.rebuilds - rebuilds, g());
    }
    let t = Instant::now();
    view::act(&mut win.ed, &mut win.view, view::Action::Undo);
    draw(&mut win);
    println!("undo: {:.1} ms {}", ms(t), g());
    view::act(&mut win.ed, &mut win.view, view::Action::Tool(Tool::Select));
    draw(&mut win);
    let (a, b) = ((x0 + 10.0, y - 100.0), (x1 - 10.0, y + 60.0));
    motion(&mut win, a.0, a.1, 0, &mut last);
    draw(&mut win);
    g();
    let t = Instant::now();
    button(&mut win, a.0, a.1, sdl3::mouse::MouseButton::Left, true);
    draw(&mut win);
    let mut worst = 0.0f64;
    for k in 1..=20 {
        let s = Instant::now();
        motion(&mut win, a.0 + (b.0 - a.0) * k as f32 / 20.0, a.1 + (b.1 - a.1) * k as f32 / 20.0, 1, &mut last);
        draw(&mut win);
        worst = worst.max(ms(s));
    }
    let s = Instant::now();
    button(&mut win, b.0, b.1, sdl3::mouse::MouseButton::Left, false);
    draw(&mut win);
    println!("select: whole {:.1} ms, drag worst {worst:.1}, let go {:.1} ms, selected {} {}", ms(t), ms(s), win.ed.selecting.selection.as_ref().map_or(0, |s| s.cells.len()), g());
    let t = Instant::now();
    for _ in 0..10 {
        win.dirty = true;
        win.draw().unwrap();
    }
    println!("frame with a selection: {:.2} ms, instances {}", ms(t) / 10.0, instances(&win));
    gpu.borrow_mut().present(&mut win);
    println!("  its picture: {}", g());
}

/// Times what the window does on the wall alone and in its room: run by
/// hand with `cargo test --release --lib big_model_timings -- --ignored
/// --nocapture`.
#[test]
#[ignore]
fn big_model_timings() {
    let t = Instant::now();
    let (win, _dir) = wall_window("timings", [169, 12, 116]);
    println!("== the wall alone; open + 3 frames: {:.1} ms", ms(t));
    exercise(win);
    let t = Instant::now();
    let (win, _dir) = wall_in_room("timings-room", 1);
    println!("== the wall in its room (CONTEXT); open + 3 frames: {:.1} ms", ms(t));
    exercise(win);
    let t = Instant::now();
    let (mut win, _dir) = wall_figure_window("timings-figure");
    println!("== the wall as a figure; open + 3 frames: {:.1} ms", ms(t));
    let t = Instant::now();
    for _ in 0..10 {
        win.dirty = true;
        win.draw().unwrap();
    }
    println!("figure whole, idle frame: {:.2} ms, instances {}", ms(t) / 10.0, win.last.as_ref().map_or(0, |l| l.0.instances.len()));
    view::act(&mut win.ed, &mut win.view, view::Action::Part(0));
    win.draw().unwrap();
    println!("chosen: {}", win.ed.figure.as_ref().and_then(|f| f.selected).map_or("none".into(), |p| win.ed.figure.as_ref().unwrap().parts()[p].name.clone()));
    exercise(win);
}

/// The level editor on the room (the pieces).
#[test]
#[ignore]
fn big_level_timings() {
    let dir = room_game("level-timings", 1);
    let t = Instant::now();
    let (ed, view, cfg) = open::<crate::Voxcade>(&dir, Some("levels/room.toml"), None, false).unwrap();
    let mut win = EditorWindow::headless(ed, view, cfg, &dir).unwrap();
    for _ in 0..3 {
        win.draw().unwrap();
    }
    println!("== the room in the level editor; open + 3 frames: {:.1} ms", ms(t));
    let mut gpu = Gpu::new(&mut win).expect("a GPU");
    let t = Instant::now();
    for _ in 0..10 {
        win.dirty = true;
        win.draw().unwrap();
    }
    println!("idle frame: {:.2} ms, instances {}", ms(t) / 10.0, win.last.as_ref().map_or(0, |l| l.0.instances.len()));
    gpu.present(&mut win);
    println!("  its picture: {}", gpu.take());
    let mut last = (800.0, 450.0);
    let t = Instant::now();
    for k in 0..40 {
        motion(&mut win, 500.0 + k as f32 * 15.0, 500.0, 0, &mut last);
        win.draw().unwrap();
        gpu.present(&mut win);
    }
    println!("hover frame: {:.2} ms {}", ms(t) / 40.0, gpu.take());
}

/// Every cell's colour in a world (None: empty), to compare two worlds as
/// they are drawn.
fn colours_of(world: &crate::world::World) -> Vec<Option<[u8; 4]>> {
    let s = world.size;
    let mut out = Vec::new();
    for z in 0..s.z {
        for y in 0..s.y {
            for x in 0..s.x {
                let v = world.get(glam::IVec3::new(x, y, z));
                out.push((v != 0).then(|| world.palette[v as usize]));
            }
        }
    }
    out
}

/// A stroke on a big model, and an undo, set again only the voxels they
/// change: the world shown is never built anew whole (each voxel of a
/// stroke built it anew and every chunk was meshed again: the owner's
/// back wall stuttered), and it stands as a world built afresh would.
#[test]
fn a_stroke_on_a_big_model_sets_only_its_voxels() {
    let (mut win, _dir) = wall_window("stroke", [84, 6, 58]);
    let mut last = (800.0, 450.0);
    let (y, x0, x1) = run_on(&win);
    assert!(x1 - x0 > 100.0, "the wall is met along a row: {x0} to {x1}");
    let rebuilds = win.rebuilds;
    for tool in [Tool::Paint, Tool::Remove] {
        view::act(&mut win.ed, &mut win.view, view::Action::Tool(tool));
        win.ed.colour = [200, 40, 40];
        motion(&mut win, x0, y, 0, &mut last);
        win.draw().unwrap();
        button(&mut win, x0, y, sdl3::mouse::MouseButton::Left, true);
        win.draw().unwrap();
        for k in 1..=12 {
            motion(&mut win, x0 + (x1 - x0) * k as f32 / 12.0, y, 1, &mut last);
            win.draw().unwrap();
        }
        button(&mut win, x1, y, sdl3::mouse::MouseButton::Left, false);
        win.draw().unwrap();
        assert_eq!(colours_of(&win.shown.world), colours_of(&Shown::new(&win.ed).unwrap().world), "{tool:?}: the world as built afresh");
    }
    assert_eq!(win.ed.steps(), 2, "two strokes, two steps");
    view::act(&mut win.ed, &mut win.view, view::Action::Undo);
    win.draw().unwrap();
    assert_eq!(colours_of(&win.shown.world), colours_of(&Shown::new(&win.ed).unwrap().world), "undone: the world as built afresh");
    assert_eq!(win.rebuilds, rebuilds, "no world built anew whole");
}

/// The level round a model shaped in place is made once, not on every
/// frame (made on every frame, a room of big walls took a fifth of a
/// second a frame), joined again only when the eye goes round to another
/// side, and drawn exactly as made afresh: hovering, a stroke, turning the
/// view.
#[test]
fn the_room_round_a_model_is_made_once_and_drawn_the_same() {
    let (mut win, _dir) = wall_in_room("room-once", 3);
    let mut last = (800.0, 450.0);
    let same_as_afresh = |win: &EditorWindow, what: &str| {
        let (frame, _) = win.last.as_ref().expect("a frame drawn");
        let fresh = view::frame(&win.ed, &win.view, &Shown::new(&win.ed).unwrap(), &win.figures, &win.cfg.light, 1600.0 / 900.0);
        assert!(frame.behind > 1000, "{what}: the room is there ({})", frame.behind);
        assert_eq!((frame.behind, frame.see_through), (fresh.behind, fresh.see_through), "{what}");
        assert!(bytemuck::cast_slice::<_, u8>(&frame.instances) == bytemuck::cast_slice::<_, u8>(&fresh.instances), "{what}: drawn as made afresh");
    };
    same_as_afresh(&win, "opened");
    let (y, x0, x1) = run_on(&win);
    view::act(&mut win.ed, &mut win.view, view::Action::Tool(Tool::Paint));
    win.ed.colour = [200, 40, 40];
    for k in 0..6 {
        motion(&mut win, x0 + (x1 - x0) * k as f32 / 6.0, y, 0, &mut last);
        win.draw().unwrap();
    }
    button(&mut win, x0, y, sdl3::mouse::MouseButton::Left, true);
    for k in 1..=6 {
        motion(&mut win, x0 + (x1 - x0) * k as f32 / 6.0, y, 1, &mut last);
        win.draw().unwrap();
    }
    button(&mut win, x1, y, sdl3::mouse::MouseButton::Left, false);
    win.draw().unwrap();
    assert_eq!(win.ed.steps(), 1);
    same_as_afresh(&win, "after a stroke");
    assert_eq!((win.shown.rooms_made(), win.shown.room_joins()), (1, 1), "made and joined once");
    // The view turned a little: the same side, only sorted again.
    win.view.block.yaw += 5.0;
    win.dirty = true;
    win.draw().unwrap();
    same_as_afresh(&win, "turned a little");
    assert_eq!((win.shown.rooms_made(), win.shown.room_joins()), (1, 1));
    // Round to another side: joined again for it, not made again.
    win.view.block.yaw += 90.0;
    win.dirty = true;
    win.draw().unwrap();
    same_as_afresh(&win, "turned round");
    assert_eq!((win.shown.rooms_made(), win.shown.room_joins()), (1, 2));
    // CONTEXT off and on: made again.
    view::act(&mut win.ed, &mut win.view, view::Action::Context);
    win.draw().unwrap();
    view::act(&mut win.ed, &mut win.view, view::Action::Context);
    win.draw().unwrap();
    same_as_afresh(&win, "shown again");
    assert_eq!(win.shown.rooms_made(), 3);
}

/// `join_see_through` as it was before it was split into finding the
/// body and joining it for the eye: the reference the split is held to.
fn join_as_before(instances: &mut [crate::render::Instance], toward: glam::Vec3) {
    use crate::render::Instance;
    let joins = |i: &Instance| i.pos_size[3] == 1.0 && i.fx[1] < 255 && i.turn == [0; 4];
    let cell = |i: &Instance| glam::Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
    let mut at: crate::world::CellMap<glam::Vec3> = crate::world::CellMap::default();
    for i in instances.iter().filter(|i| joins(i)) {
        at.insert(cell(i), glam::Vec3::from_slice(&i.pos_size[..3]) - cell(i).as_vec3());
    }
    if at.len() < 2 {
        return;
    }
    let a = toward.abs();
    let first = if a.z >= 0.8 * a.max_element() { 2 } else if a.y >= a.x { 1 } else { 0 };
    let mut ways = [first, (first + 1) % 3, (first + 2) % 3];
    if a[ways[2]] > a[ways[1]] {
        ways.swap(1, 2);
    }
    const FAR: i32 = 16;
    for i in instances.iter_mut().filter(|i| joins(i)) {
        let c = cell(i);
        let within = glam::Vec3::from_slice(&i.pos_size[..3]) - c.as_vec3();
        let joined = |to: glam::IVec3| at.get(&to).is_some_and(|w| (*w - within).abs().max_element() <= 0.01);
        let run = |axis: usize| {
            let step = crate::mesh::DIRS[axis * 2];
            let count = |d: glam::IVec3| (1..=FAR).take_while(|n| joined(c + d * *n)).count();
            1 + count(step) + count(-step)
        };
        let runs = [run(0), run(1), run(2)];
        let mut sorted = runs;
        sorted.sort_unstable();
        let across = if sorted[0] * 2 <= sorted[1] { *ways.iter().find(|w| runs[**w] == sorted[0]).expect("one is the shortest") } else { ways[0] };
        let kept = across * 2 + (toward[across] < 0.0) as usize;
        for (bit, d) in crate::mesh::DIRS.iter().enumerate() {
            if bit != kept && joined(c + *d) {
                i.fx[2] |= 1 << bit;
            }
        }
    }
}

/// Cubes for the joining and sorting tests: see-through bodies thin and
/// thick, opaque ones, cubes raised within their cells, small ones, a
/// turned one, two in one cell, some faces already marked.
fn some_cubes(seed: u32) -> Vec<crate::render::Instance> {
    let mut r = seed;
    let mut next = move |n: u32| {
        r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (r >> 8) % n
    };
    let mut out = Vec::new();
    for _ in 0..3000 {
        let (x, y, z) = (next(40) as f32, next(20) as f32, next(30) as f32);
        let lift = if next(10) == 0 { 0.25 } else { 0.0 };
        let size = if next(40) == 0 { 0.5 } else { 1.0 };
        let opacity = if next(5) == 0 { 255 } else { 90 + next(100) as u8 };
        let turn = if next(80) == 0 { [10, 0, 0, 120] } else { [0; 4] };
        let marked = if next(20) == 0 { 1 << next(6) } else { 0 };
        out.push(crate::render::Instance { pos_size: [x + 0.5, y + 0.5, z + 0.5 + lift, size], color: [next(255) as u8, 80, 80, 255], fx: [0, opacity, marked, 0], shade: [0; 4], turn });
    }
    out
}

/// Joining split into finding the body and joining it for the eye marks
/// the same faces as joining did whole, from every side.
#[test]
fn joining_by_the_body_is_joining_as_before() {
    for seed in 1..4 {
        let cubes = some_cubes(seed);
        let body = crate::render::see_through_body(&cubes);
        for toward in [glam::Vec3::new(3.0, -40.0, 20.0), glam::Vec3::new(-50.0, 2.0, 1.0), glam::Vec3::new(0.5, 0.4, -9.0), glam::Vec3::new(-7.0, 7.0, 6.0), glam::Vec3::new(10.0, -10.0, 0.0)] {
            let mut before = cubes.clone();
            join_as_before(&mut before, toward);
            let mut whole = cubes.clone();
            crate::render::join_see_through(&mut whole, toward);
            let mut split = cubes.clone();
            crate::render::join_by_body(&mut split, &body, toward);
            let fx = |c: &[crate::render::Instance]| c.iter().map(|i| i.fx).collect::<Vec<_>>();
            assert_eq!(fx(&whole), fx(&before), "seed {seed}, toward {toward}");
            assert_eq!(fx(&split), fx(&before), "seed {seed}, toward {toward}: by the body kept");
            assert!(fx(&before).iter().zip(fx(&cubes)).any(|(a, b)| *a != b), "something joined");
        }
    }
}

/// Far to near by a radix sort is the stable sort by distance it took
/// the place of, cubes at the same distance in the order given.
#[test]
fn far_to_near_sorts_as_sorting_by_distance_did() {
    for seed in 1..4 {
        let mut cubes = some_cubes(seed);
        // (Cubes at the same distance: the same cells again.)
        let more: Vec<_> = cubes.iter().take(500).map(|c| crate::render::Instance { color: [1, 2, 3, 255], ..*c }).collect();
        cubes.extend(more);
        for eye in [glam::Vec3::new(20.0, -60.0, 40.0), glam::Vec3::new(20.0, 10.0, 15.0), glam::Vec3::ZERO] {
            let mut by_sort = cubes.clone();
            let far = |i: &crate::render::Instance| -(glam::Vec3::from_slice(&i.pos_size[..3]) - eye).length_squared();
            by_sort.sort_by(|a, b| far(a).total_cmp(&far(b)));
            let mut by_radix = cubes.clone();
            view::far_to_near(&mut by_radix, eye);
            assert!(bytemuck::cast_slice::<_, u8>(&by_sort) == bytemuck::cast_slice::<_, u8>(&by_radix), "seed {seed}, eye {eye}");
        }
    }
}

/// The selection's outline kept from frame to frame is the outline worked
/// out afresh, as the selection grows and changes.
#[test]
fn a_kept_outline_is_the_outline() {
    let (mut win, _dir) = wall_window("outline", [30, 6, 20]);
    let mut kept = Default::default();
    let mut check = |win: &EditorWindow, what: &str| {
        assert_eq!(win.ed.selection_edges_kept(&mut kept), win.ed.selection_edges(), "{what}");
    };
    win.ed.select_box_cells(glam::IVec3::new(2, 0, 2), glam::IVec3::new(9, 5, 7), false);
    assert!(!win.ed.selection_edges().is_empty());
    check(&win, "a box");
    win.ed.select_box_cells(glam::IVec3::new(8, 1, 6), glam::IVec3::new(14, 3, 12), true);
    check(&win, "another added");
    win.ed.select_box_cells(glam::IVec3::new(0, 0, 0), glam::IVec3::new(3, 2, 3), false);
    check(&win, "a new one");
}

/// A stroke on a figure's part is drawn from the world shown: the figure
/// (its other parts, the rig) is made again once when it is let go, not
/// at every voxel (each voxel made the whole figure again: 22 to 25 ms a
/// frame on the wall as a figure), and what is drawn mid-stroke is what a
/// figure made afresh draws.
#[test]
fn a_stroke_on_a_figure_makes_it_again_when_let_go() {
    let (mut win, _dir) = wall_figure_window("figure-stroke");
    view::act(&mut win.ed, &mut win.view, view::Action::Part(0));
    win.draw().unwrap();
    let mut last = (800.0, 450.0);
    let (y, x0, x1) = run_on(&win);
    assert!(x1 - x0 > 100.0, "the wall is met along a row: {x0} to {x1}");
    view::act(&mut win.ed, &mut win.view, view::Action::Tool(Tool::Paint));
    win.ed.colour = [200, 40, 40];
    motion(&mut win, x0, y, 0, &mut last);
    win.draw().unwrap();
    let before = win.ed.model.as_ref().unwrap().shaped();
    button(&mut win, x0, y, sdl3::mouse::MouseButton::Left, true);
    win.draw().unwrap();
    for k in 1..=8 {
        motion(&mut win, x0 + (x1 - x0) * k as f32 / 12.0, y, 1, &mut last);
        win.draw().unwrap();
    }
    assert!(!win.ed.model.as_ref().unwrap().is(&before), "the stroke changed the wall");
    assert!(win.ed.figure.as_ref().unwrap().made_of(&before), "the figure not made again mid-stroke");
    // Drawn as a figure made afresh draws it.
    let light = win.cfg.light.clone();
    let stale = view::frame(&win.ed, &win.view, &win.shown, &win.figures, &light, 16.0 / 9.0);
    let kept = win.ed.figure.take().unwrap();
    let (mut fresh, _) = super::figure::Figure::follow(None, win.ed.model.as_ref().unwrap(), None);
    let f = fresh.as_mut().unwrap();
    (f.selected, f.hidden) = (kept.selected, kept.hidden.clone());
    win.ed.figure = fresh;
    let afresh = view::frame(&win.ed, &win.view, &win.shown, &win.figures, &light, 16.0 / 9.0);
    assert!(bytemuck::cast_slice::<_, u8>(&stale.instances) == bytemuck::cast_slice::<_, u8>(&afresh.instances), "the same cubes mid-stroke");
    assert_eq!(stale.models, afresh.models);
    win.ed.figure = Some(kept);
    button(&mut win, x1, y, sdl3::mouse::MouseButton::Left, false);
    win.draw().unwrap();
    let now = win.ed.model.as_ref().unwrap().shaped();
    assert!(win.ed.figure.as_ref().unwrap().made_of(&now), "let go: made of the model as it is");
    assert_eq!(win.ed.steps(), 1);
}
