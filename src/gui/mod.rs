//! The look the IDE and the editors share, so they are of a piece: egui
//! on the engine's SDL3 windows and wgpu (`input`: the glue; `Painter`:
//! egui's frames drawn), one monospace typeface in two sizes (`fonts`),
//! calm colours, and the list row both use. Calm on purpose: a dark
//! ground, big readable letters, few words.

pub mod fonts;
pub mod input;

use egui::{Color32, FontId, TextStyle};

/// The colours: a dark ground, light warm letters, what is open or chosen
/// in the engine's yellow.
pub const GROUND: Color32 = Color32::from_rgb(24, 23, 30);
pub const PANEL: Color32 = Color32::from_rgb(31, 29, 38);
pub const TEXT: Color32 = Color32::from_rgb(226, 218, 198);
pub const QUIET: Color32 = Color32::from_rgb(132, 126, 118);
pub const ACCENT: Color32 = Color32::from_rgb(255, 214, 90);
/// Something gone wrong.
pub const ERROR: Color32 = Color32::from_rgb(214, 104, 92);
/// A row under the mouse, and the one open or chosen.
pub const ROW_HOVERED: Color32 = Color32::from_rgb(40, 38, 49);
pub const ROW_OPEN: Color32 = Color32::from_rgb(52, 48, 64);
/// Room before a row's words, and after a heading's arrow.
pub const INSET: f32 = 8.0;

/// The look: calm colours and big letters.
pub fn style(ctx: &egui::Context) {
    // One monospace typeface throughout, for the retro feel (egui's own
    // Hack), in two sizes only.
    ctx.set_fonts(fonts::definitions());
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = GROUND;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(70, 64, 86);
    visuals.widgets.noninteractive.bg_stroke.color = Color32::from_rgb(44, 41, 52);
    ctx.set_visuals(visuals);
    // A notch of the mouse wheel: about four lines of code (egui's own is
    // two).
    ctx.options_mut(|o| o.input_options.line_scroll_speed = 80.0);
    ctx.all_styles_mut(|style| {
        let (normal, small) = (FontId::monospace(16.0), FontId::monospace(13.0));
        style.text_styles = [
            (TextStyle::Heading, normal.clone()),
            (TextStyle::Body, normal.clone()),
            (TextStyle::Button, normal.clone()),
            (TextStyle::Small, small),
            (TextStyle::Monospace, normal),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
        // Scroll bars always there when needed, wide enough to grab, near
        // the panel's edge.
        style.spacing.scroll = egui::style::ScrollStyle { bar_width: 10.0, bar_inner_margin: 8.0, bar_outer_margin: 4.0, handle_min_length: 32.0, ..egui::style::ScrollStyle::solid() };
    });
}

/// The arrow of a heading that folds, at `at`: down when open, to the left
/// when closed (`openness` between).
pub fn arrow(ui: &egui::Ui, at: egui::Pos2, openness: f32) {
    let turn = egui::emath::remap(openness, 0.0..=1.0, std::f32::consts::FRAC_PI_2..=0.0);
    ui.painter().add(egui::Shape::rotated_triangle(egui::Rect::from_center_size(at, egui::vec2(8.0, 8.0)), turn, QUIET));
}

/// A row of a list: `label` in `colour`, `hint` (a key, in the quiet
/// colour) at its right end. The whole row is clicked and lit (`open`: lit
/// always), not only its words.
pub fn row(ui: &mut egui::Ui, label: &str, style: TextStyle, colour: Color32, open: bool, hint: Option<&str>) -> egui::Response {
    let font = style.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), colour);
    let (id, rect) = ui.allocate_space(egui::vec2(ui.available_width(), galley.size().y + 10.0));
    // Over the gaps between rows too, so that no click falls between two.
    let band = rect.expand2(egui::vec2(0.0, ui.spacing().item_spacing.y / 2.0));
    let response = ui.interact(band, id, egui::Sense::click());
    if open || response.hovered() {
        ui.painter().rect_filled(band, 3.0, if open { ROW_OPEN } else { ROW_HOVERED });
    }
    // A long name is cut at the edge.
    let text = egui::pos2(rect.left() + INSET, rect.center().y - galley.size().y / 2.0);
    let painter = ui.painter().with_clip_rect(band.intersect(ui.clip_rect()));
    painter.galley(text, galley, TEXT);
    if let Some(hint) = hint {
        painter.text(egui::pos2(rect.right() - INSET, rect.center().y), egui::Align2::RIGHT_CENTER, hint, TextStyle::Small.resolve(ui.style()), QUIET);
    }
    response
}

/// egui's frames drawn with the GPU: on the ground (the IDE), or over
/// what is there (an editor's 3D view, `over`).
pub struct Painter {
    pub renderer: egui_wgpu::Renderer,
    over: bool,
}

impl Painter {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, over: bool) -> Self {
        Self { renderer: egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default()), over }
    }

    /// Takes a frame's new and freed textures without drawing it (a shot
    /// lays its first frames out unseen).
    pub fn skip(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, out: egui::FullOutput) {
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        for id in &out.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }

    /// Draws a frame's output into `view` (`size` pixels big).
    pub fn paint(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView, ctx: &egui::Context, out: egui::FullOutput, size: [u32; 2]) {
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point: out.pixels_per_point };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gui") });
        let extra = self.renderer.update_buffers(device, queue, &mut encoder, &jobs, &screen);
        {
            let ground = wgpu::Color { r: GROUND.r() as f64 / 255.0, g: GROUND.g() as f64 / 255.0, b: GROUND.b() as f64 / 255.0, a: 1.0 };
            let load = if self.over { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(ground) };
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &jobs, &screen);
        }
        queue.submit(extra.into_iter().chain([encoder.finish()]));
        for id in &out.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}
