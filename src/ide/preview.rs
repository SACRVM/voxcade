//! A picture of what an editor opens, in the main area: a model (its
//! frames playing; PAUSE, PREV and NEXT above it), a block or the level, in the game's own light, turned
//! with the mouse. It is the editor's own view of it, without the panels
//! and the floor, drawn into a texture of its own on the IDE's GPU, and
//! only when it changed (turned, resized, the next frame).

use std::path::Path;

use anyhow::Result;

use crate::config::GameConfig;
use crate::editor::model::{Editor, Scope};
use crate::gui::QUIET;
use crate::editor::view::{self, Figures, Shown, View};
use crate::render::Renderer;

/// The format it is drawn in: the games' own, as the headless canvas's.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A smaller shadow map than the editor's: the picture is small.
const SHADOW_MAP: u32 = 2048;

/// What a preview draws into, and its name to egui.
struct Target {
    view: wgpu::TextureView,
    size: [u32; 2],
    id: egui::TextureId,
}

pub struct Preview {
    ed: Editor,
    view: View,
    shown: Shown,
    figures: Figures,
    cfg: GameConfig,
    renderer: Option<Renderer>,
    target: Option<Target>,
    /// The size in pixels the main area has room for.
    wanted: Option<[u32; 2]>,
    dirty: bool,
    /// The renderer has this one's world (not another's it took over).
    meshed: bool,
    /// The camera sees all of it at the target's size.
    fitted: bool,
    /// A model's frames held at the one shown (PAUSE, PREV, NEXT).
    paused: bool,
}

impl Preview {
    /// The model (or block) `file` of the game in `dir`.
    pub fn model(dir: &Path, file: &str, cfg: &GameConfig) -> Result<Self> {
        Self::of(Editor::open_model(dir, file)?, dir, cfg)
    }

    /// The level of blocks of the game in `dir`, as the game's camera sees
    /// it.
    pub fn level(dir: &Path, file: &str, cfg: &GameConfig) -> Result<Self> {
        Self::of(Editor::open(dir, file)?, dir, cfg)
    }

    fn of(mut ed: Editor, dir: &Path, cfg: &GameConfig) -> Result<Self> {
        // A figure of parts as before the editor's FIGURE: its frames.
        if ed.scope == Scope::Figure {
            (ed.scope, ed.figure) = (Scope::Frame(0), None);
        }
        let mut view = View::new(&ed, cfg);
        view.floor = false;
        let shown = Shown::new(&ed)?;
        let figures = if ed.model.is_some() { Figures::load(dir, &GameConfig::default()) } else { Figures::load(dir, cfg) };
        Ok(Self { ed, view, shown, figures, cfg: cfg.clone(), renderer: None, target: None, wanted: None, dirty: true, meshed: false, fitted: false, paused: false })
    }

    /// The picture, filling what is left of `ui`; a drag turns it. A
    /// model's frames play when `animate` (not in a shot), held with
    /// PAUSE, stepped with PREV and NEXT.
    pub fn ui(&mut self, ui: &mut egui::Ui, animate: bool) {
        let count = self.ed.frames().len();
        if count > 1 {
            let playing = animate && !self.paused;
            let mut at = match self.ed.scope {
                Scope::Frame(f) => f.min(count - 1),
                _ => 0,
            };
            if playing {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(1.0 / view::FRAMES_PER_SECOND));
                at = (ui.input(|i| i.time) * view::FRAMES_PER_SECOND) as usize % count;
            }
            ui.horizontal(|ui| {
                if ui.button(if playing { "PAUSE" } else { "PLAY" }).clicked() {
                    self.paused = playing;
                }
                // A step holds the frames.
                if ui.button("PREV").clicked() {
                    (self.paused, at) = (true, (at + count - 1) % count);
                }
                if ui.button("NEXT").clicked() {
                    (self.paused, at) = (true, (at + 1) % count);
                }
                ui.label(egui::RichText::new(format!("FRAME {} OF {count}", at + 1)).color(QUIET));
            });
            ui.add_space(8.0);
            if self.ed.scope != Scope::Frame(at) {
                self.ed.scope = Scope::Frame(at);
                self.dirty = true;
            }
        }
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
        let pixels = ui.ctx().pixels_per_point();
        self.wanted = Some([(rect.width() * pixels).round().max(1.0) as u32, (rect.height() * pixels).round().max(1.0) as u32]);
        if response.dragged() {
            let d = response.drag_delta();
            self.view.orbit(self.ed.scope).turn(d.x / rect.width().max(1.0), d.y / rect.height().max(1.0));
            self.dirty = true;
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(if response.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        if let Some(t) = &self.target {
            ui.painter().image(t.id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }

    /// Draws the picture if it changed or the room did; true if it did.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, egui: &mut egui_wgpu::Renderer) -> bool {
        let Some(size) = self.wanted else { return false };
        if !self.dirty && self.target.as_ref().is_some_and(|t| t.size == size) {
            return false;
        }
        let resized = self.target.as_ref().is_none_or(|t| t.size != size);
        if resized || !self.fitted {
            self.fit(size[0] as f32 / size[1] as f32);
            self.fitted = true;
        }
        let renderer = self.renderer.get_or_insert_with(|| Renderer::new(device, FORMAT, size[0], size[1]));
        // Another frame of the model: its world built anew.
        if let Ok(true) = self.shown.update(&self.ed) {
            self.meshed = false;
        }
        if !self.meshed {
            renderer.rebuild_all(device, queue, &mut self.shown.world, &crate::editor::ALL_FACES);
            self.meshed = true;
        }
        if resized {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("preview"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            renderer.resize(device, size[0], size[1]);
            let id = match &self.target {
                Some(t) => {
                    egui.update_egui_texture_from_wgpu_texture(device, &view, wgpu::FilterMode::Linear, t.id);
                    t.id
                }
                None => egui.register_native_texture(device, &view, wgpu::FilterMode::Linear),
            };
            self.target = Some(Target { view, size, id });
        }
        let target = self.target.as_ref().expect("made above");
        let mut frame = view::frame(&self.ed, &self.view, &self.shown, &self.figures, &self.cfg.light, size[0] as f32 / size[1] as f32);
        frame.shadow_map = SHADOW_MAP;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("preview") });
        renderer.render(device, queue, &mut encoder, &target.view, &frame);
        queue.submit([encoder.finish()]);
        self.dirty = false;
        true
    }

    /// The camera brought to see all of it at the picture's `aspect`: the
    /// model's box round about, the level from the front, whole.
    fn fit(&mut self, aspect: f32) {
        use crate::editor::model::Scope;
        let scope = self.ed.scope;
        let n = self.ed.level.n as f32;
        let (target, wide, high) = match scope {
            Scope::Level => {
                let size = self.ed.level.size.as_vec3() * n;
                (glam::Vec3::new(size.x / 2.0, n / 2.0, size.z / 2.0), size.x, size.z)
            }
            _ => {
                let size = self.ed.grid().as_vec3();
                let round = size.length();
                (size / 2.0, round, round)
            }
        };
        let orbit = self.view.orbit(scope);
        orbit.target = target;
        orbit.zoom = (wide.max(high * aspect) * 1.1).max(8.0);
    }

    /// Takes over what `old` drew with (its renderer, its texture): a new
    /// one costs a moment (the shaders), and the name egui knows stays.
    pub fn adopt(&mut self, old: Preview) {
        (self.renderer, self.target) = (old.renderer, old.target);
    }

    /// Its name to egui, to be freed when it goes.
    pub fn texture(&self) -> Option<egui::TextureId> {
        self.target.as_ref().map(|t| t.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model's frames play; NEXT and PREV hold them and step, PLAY lets
    /// them go on.
    #[test]
    fn a_models_frames_are_held_and_stepped() {
        let dir = Path::new("games/voxkeep");
        let cfg = crate::session::config::<crate::Voxcade>(dir).unwrap();
        let mut preview = Preview::model(dir, "bat.vox", &cfg).unwrap();
        let ctx = egui::Context::default();
        let mut time = 0.0;
        fn frame(ctx: &egui::Context, time: &mut f64, events: Vec<egui::Event>, preview: &mut Preview) {
            *time += 1.0 / 60.0;
            let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 400.0))), events, time: Some(*time), ..Default::default() };
            let _ = ctx.run_ui(raw, |ui| preview.ui(ui, true));
        }
        let shown = |p: &Preview| match p.ed.scope {
            Scope::Frame(f) => f,
            _ => usize::MAX,
        };
        frame(&ctx, &mut time, Vec::new(), &mut preview);
        // The buttons from the left: PAUSE, PREV, NEXT.
        let buttons: Vec<egui::Rect> = ctx.viewport(|v| {
            let w = &v.prev_pass.widgets;
            let mut rects: Vec<_> = w.layer_ids().flat_map(|l| w.get_layer(l).map(|r| r.rect).collect::<Vec<_>>()).filter(|r| r.top() < 40.0 && r.width() < 120.0).collect();
            rects.sort_by(|a, b| a.left().total_cmp(&b.left()));
            rects
        });
        let click = |at: egui::Rect, time: &mut f64, preview: &mut Preview| {
            let p = at.center();
            let button = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(&ctx, time, vec![egui::Event::PointerMoved(p)], preview);
            frame(&ctx, time, vec![button(true), button(false)], preview);
        };
        let before = shown(&preview);
        click(buttons[2], &mut time, &mut preview);
        let next = shown(&preview);
        assert!(preview.paused, "NEXT holds the frames");
        assert_eq!(next, (before + 1) % 2, "one on");
        for _ in 0..30 {
            frame(&ctx, &mut time, Vec::new(), &mut preview);
        }
        assert_eq!(shown(&preview), next, "held");
        click(buttons[1], &mut time, &mut preview);
        assert_eq!(shown(&preview), before, "PREV one back");
        click(buttons[0], &mut time, &mut preview);
        assert!(!preview.paused, "PLAY lets them go on");
    }
}
