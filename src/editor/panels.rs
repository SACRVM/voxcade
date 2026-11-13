//! The editor's panels, drawn with egui as the IDE is (`crate::gui`), so
//! the two are of a piece: on the right what is shown or held and the
//! tools, rows like the IDE's list with each one's key at the end; along
//! the bottom the level's blocks, the tileset's tiles, the kinds of things
//! or the colours, as pictures. What is clicked comes back as `Action`s,
//! and the keys the panels name do the same.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui::{Color32, RichText, TextStyle};

use super::stage;
use super::model::room::Item;
use super::model::Editor;
use super::view::{Action, Figures, View};
use crate::gui::{ACCENT, QUIET, TEXT, WARNING};

/// How wide the panel on the right is, how high the bar along the bottom.
pub const COLUMN: f32 = 250.0;
pub const BAR: f32 = 76.0;

/// What a frame of the panels gave: the clicks, the keys named on them
/// (a letter or a colour's digit, and what it does), and where they lie.
#[derive(Default)]
pub struct Shown {
    pub clicked: Vec<Action>,
    pub keys: Vec<(char, Action)>,
    /// The keys the panels name with the words of their buttons, in the
    /// order drawn (F1's overview, `keys.rs`).
    pub named: Vec<(String, String)>,
    pub rects: Vec<egui::Rect>,
    /// A name typed for an item and Enter pressed (`Editor::rename`).
    pub renamed: Option<(Item, String)>,
    /// A name typed for a figure's part and Enter pressed
    /// (`Editor::rename_part`).
    pub renamed_part: Option<(usize, String)>,
    /// The room the panels leave the 3D view (egui's points), where it is
    /// not the room right of nothing and above the bottom bar.
    pub free: Option<egui::Rect>,
    /// Where each number field lies (its letter or word), as laid out.
    pub fields: Vec<(String, egui::Rect)>,
    /// Where each row of a figure's parts lies (its part's name; the
    /// figure whole's: empty), as laid out.
    pub rows: Vec<(String, egui::Rect)>,
}

/// The pictures on the panels as textures, made again only when what they
/// show changed.
#[derive(Default)]
pub struct Thumbs(HashMap<String, (u64, egui::TextureHandle)>, HashMap<String, Option<String>>);

impl Thumbs {
    /// The picture `key`, made by `make` if new or changed (`of`: what it
    /// shows, hashed).
    pub fn get(&mut self, ctx: &egui::Context, key: String, of: impl Hash, make: impl FnOnce() -> (i32, i32, Vec<Option<[u8; 3]>>)) -> (egui::TextureId, egui::Vec2) {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        of.hash(&mut h);
        let hash = h.finish();
        if self.0.get(&key).is_none_or(|(old, _)| *old != hash) {
            let (w, h, pixels) = make();
            let rgba: Vec<u8> = pixels.iter().flat_map(|p| p.map_or([0; 4], |[r, g, b]| [r, g, b, 255])).collect();
            let image = egui::ColorImage::from_rgba_unmultiplied([w.max(1) as usize, h.max(1) as usize], if rgba.is_empty() { &[0; 4] } else { &rgba });
            self.0.insert(key.clone(), (hash, ctx.load_texture(&key, image, egui::TextureOptions::NEAREST)));
        }
        let t = &self.0[&key].1;
        (t.id(), t.size_vec2())
    }

    /// The picture of the model `file` (its path in the game folder `dir`)
    /// seen from the front (`front_of`), made anew when the file
    /// changed, and why it does not read, if it does not (the editors'
    /// lists mark it and say why on hover, not an empty tile).
    pub fn model(&mut self, ctx: &egui::Context, dir: &std::path::Path, file: &str, fit: i32) -> (egui::TextureId, egui::Vec2, Option<String>) {
        let path = dir.join(file);
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let key = format!("model {file}");
        let mut made = None;
        let (tex, px) = self.get(ctx, key.clone(), (file, modified), || {
            let (picture, problem) = match crate::vox::load_model(&path) {
                Ok(m) => (front_of(&m, fit), None),
                Err(e) => ((1, 1, vec![None]), Some(format!("{e:#}"))),
            };
            made = Some(problem);
            picture
        });
        if let Some(problem) = made {
            self.1.insert(key.clone(), problem);
        }
        (tex, px, self.1.get(&key).cloned().flatten())
    }
}

/// The panels of a frame.
pub fn ui(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs) -> Shown {
    let mut shown = Shown::default();
    // The marks over the view as the BACKGROUND asks (`stage::paint`).
    super::zones::set_marks(ui.ctx(), view.marks(ed.scope));
    if let Some(names) = &ed.question {
        question(ui, &ed.file, names, &mut shown);
        return shown;
    }
    // Among the pieces: the three zones of `stage.rs`; else the voxel
    // editor and the level of blocks: `shaping.rs`.
    if ed.among_pieces() || ed.among_land() {
        stage::ui(ui, ed, view, figures, thumbs, &mut shown);
    } else {
        super::shaping::ui(ui, ed, view, figures, thumbs, &mut shown);
    }
    // F1: the keys of what is open, over the view.
    if view.keys_shown {
        let groups = super::keys::overview(ed, &shown.named);
        let free = shown.free.unwrap_or_else(|| ui.ctx().content_rect());
        super::keys::page(ui, free, &groups, &mut shown);
    }
    shown
}

/// The question before a level turned off the grid opens, over the whole
/// window in the panels' look: what is wrong in a few words, which things
/// (the first few), ALIGN ALL and DON'T OPEN.
fn question(ui: &mut egui::Ui, file: &str, names: &[String], shown: &mut Shown) {
    let frame = egui::Frame::new().fill(crate::gui::PANEL).inner_margin(egui::Margin::same(40));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.25).max(0.0));
            ui.label(RichText::new(file).color(QUIET));
            ui.add_space(12.0);
            // The words short; what they mean on hover.
            let count = if names.len() == 1 { "1 THING".to_string() } else { format!("{} THINGS", names.len()) };
            ui.label(RichText::new(format!("{count} OFF THE GRID")).color(TEXT).heading()).on_hover_text("Turned freely (rotation = 30 and the like); this game sets things on the grid, in quarter turns");
            ui.add_space(12.0);
            let more = if names.len() > 4 { format!(" and {} more", names.len() - 4) } else { String::new() };
            ui.label(RichText::new(format!("{}{more}", names.iter().take(4).cloned().collect::<Vec<_>>().join(", "))).color(WARNING));
            ui.add_space(8.0);
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                let width = 2.0 * 180.0 + ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                let tips = ["Turns each to its nearest way (a step back undoes it)", "Leaves the level as it is; or allow free rotation in conf.lua: c.camera.rotates = \"free\""];
                for ((label, action), tip) in [("ALIGN ALL", Action::AlignAll), ("DON'T OPEN", Action::DontOpen)].into_iter().zip(tips) {
                    if ui.add_sized([180.0, 36.0], egui::Button::new(RichText::new(label).color(if action == Action::AlignAll { ACCENT } else { TEXT }))).on_hover_text(tip).clicked() {
                        shown.clicked.push(action);
                    }
                }
            });
        });
    });
}

/// FIGURE's joints over the 3D view (in the room the panels at `rects`
/// leave): a ring where each part shown turns, the chosen part's bigger
/// with a line to where its parent turns, a part with a warning in the
/// warning's colour. Drawn over the voxels, so a joint inside the figure
/// shows.
pub fn joints(ui: &egui::Ui, ed: &Editor, view: &View, free: egui::Rect) {
    let screen = ui.ctx().content_rect();
    let marks = super::view::joint_marks(ed, view, screen.width() / screen.height().max(1.0), view.inset);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("joints"))).with_clip_rect(free);
    let at = |share: glam::Vec2| egui::pos2(screen.left() + share.x * screen.width(), screen.top() + share.y * screen.height());
    let chosen = marks.iter().any(|m| m.chosen);
    let gold = super::background::Marks::colour(view.marks(ed.scope).gold());
    // The chosen part's joint, a line to the part's middle: whose it is.
    let vp_line = view.view_proj(&view.block, screen.width() / screen.height().max(1.0));
    let middle = ed.shape().filter(|_| ed.part_mode().is_some()).and_then(|b| super::handles::project(vp_line, glam::Vec2::new(screen.width(), screen.height()), b.size.as_vec3() / 2.0)).map(|(p, _)| screen.min + egui::vec2(p.x, p.y));
    for m in marks.iter().filter(|m| m.chosen) {
        if let Some(mid) = middle {
            painter.line_segment([at(m.at), mid], egui::Stroke::new(2.0, gold));
        }
    }
    // The chosen one last, over the rest.
    for m in marks.iter().filter(|m| !m.chosen).chain(marks.iter().filter(|m| m.chosen)) {
        let (radius, colour) = match (m.chosen, m.warned) {
            (true, _) => (6.0, gold),
            (false, true) => (4.0, WARNING),
            (false, false) if chosen => (3.0, QUIET),
            (false, false) => (4.0, TEXT),
        };
        painter.circle(at(m.at), radius, crate::gui::GROUND, egui::Stroke::new(if m.chosen { 2.5 } else { 1.5 }, colour));
    }
    // The chosen part's arrows; its joint's ring lit under the mouse (or
    // dragged), its word beside it.
    let vp = view.view_proj(&view.block, screen.width() / screen.height().max(1.0));
    let ring_lit = view.ring_hover || ed.joint_drag().is_some_and(|d| !d.part);
    if ring_lit && let Some(m) = marks.iter().find(|m| m.chosen) {
        let p = at(m.at);
        painter.circle(p, 8.0, crate::gui::GROUND, egui::Stroke::new(3.0, gold));
        painter.text(p + egui::vec2(12.0, -12.0), egui::Align2::LEFT_BOTTOM, "JOINT", TextStyle::Small.resolve(ui.style()), gold);
    }
    // SELECT's rectangle: a thin outline from the press to the pointer.
    if let Some(m) = view.marquee {
        let at = |s: [f32; 2]| egui::pos2(screen.left() + s[0] * screen.width(), screen.top() + s[1] * screen.height());
        let gold = super::background::Marks::colour(view.marks(ed.scope).gold());
        painter.rect_stroke(egui::Rect::from_two_pos(at(m.from), at(m.to)), 0.0, egui::Stroke::new(1.0, gold), egui::StrokeKind::Middle);
    }
    // (GAME VIEW: the figure alone, as the game draws it.)
    if let Some(h) = ed.part_handles(vp).filter(|_| view.before_game.is_none()) {
        let lit = ed.joint_drag().and_then(|d| d.drag.as_ref().map(|(g, _)| g.grip)).or(view.grip_hover);
        stage::paint(&painter, &h, vp, screen, lit);
    }
    // SLICE's arrow, lit under the mouse or dragged.
    if let Some(h) = ed.slice_handles(vp) {
        let lit = (view.slice_hover || ed.slicing()).then(|| ed.slice.as_ref().map(|s| super::handles::Grip::Arrow(s.axis))).flatten();
        stage::paint(&painter, &h, vp, screen, lit);
    }
}

/// A model's first frame as a set-down piece shows it to the camera
/// (facing south: its front), at most `fit` pixels each way.
fn front_of(m: &crate::vox::VoxModel, fit: i32) -> (i32, i32, Vec<Option<[u8; 3]>>) {
    let voxels: Vec<(glam::IVec3, [u8; 3])> = m.frames.first().map_or(Vec::new(), |f| f.iter().map(|&(p, c)| (glam::IVec3::new(-p.x, -p.y, p.z), [c[0], c[1], c[2]])).collect());
    super::view::front_picture(&voxels, fit)
}

/// The marks among the pieces over the 3D view (in the room the panels at
/// `rects` leave): the chosen one's box, the lights in their colours,
/// places and areas with their names; and a quiet word on what the mouse
/// does.
pub fn marks(ui: &egui::Ui, ed: &Editor, view: &View, figures: &Figures, free: egui::Rect) {
    if ed.question.is_none() && view.before_game.is_none() {
        axis_marker(ui, ed, view, free);
    }
    if ed.among_land() && ed.question.is_none() {
        return land_marks(ui, ed, view, free);
    }
    if !ed.among_pieces() || ed.question.is_some() {
        return;
    }
    let screen = ui.ctx().content_rect();
    let marks = super::view::marks(ed, view, figures, screen.width() / screen.height().max(1.0), view.inset);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("pieces marks"))).with_clip_rect(free);
    let at = |share: glam::Vec2| egui::pos2(screen.left() + share.x * screen.width(), screen.top() + share.y * screen.height());
    let colour = |tone: super::view::Tone| match tone {
        super::view::Tone::Chosen => ACCENT,
        super::view::Tone::Plain => Color32::from_rgb(150, 205, 235),
        super::view::Tone::Quiet => Color32::from_rgba_unmultiplied(132, 126, 118, 110),
    };
    for (quad, tone) in &marks.faces {
        let fill = if *tone == super::view::Tone::Chosen { Color32::from_rgba_unmultiplied(255, 214, 90, 14) } else { Color32::from_rgba_unmultiplied(150, 205, 235, 10) };
        painter.add(egui::Shape::convex_polygon(quad.iter().map(|p| at(*p)).collect(), fill, egui::Stroke::NONE));
    }
    // The chosen one's last, over the rest.
    let order = |tone: &super::view::Tone| *tone == super::view::Tone::Chosen;
    for (a, b, tone) in marks.lines.iter().filter(|l| !order(&l.2)).chain(marks.lines.iter().filter(|l| order(&l.2))) {
        let width = if *tone == super::view::Tone::Chosen { 2.0 } else { 1.0 };
        painter.line_segment([at(*a), at(*b)], egui::Stroke::new(width, colour(*tone)));
    }
    for (p, radius, light, tone) in &marks.rings {
        let fill = light.map_or(crate::gui::GROUND, |c| Color32::from_rgb(c[0], c[1], c[2]));
        painter.circle(at(*p), *radius, fill, egui::Stroke::new(if *tone == super::view::Tone::Chosen { 2.5 } else { 1.5 }, if *tone == super::view::Tone::Chosen { ACCENT } else { crate::gui::GROUND }));
    }
    for (p, name, tone) in &marks.names {
        let font = TextStyle::Small.resolve(ui.style());
        let galley = painter.layout_no_wrap(name.clone(), font, if *tone == super::view::Tone::Chosen { ACCENT } else { TEXT });
        let rect = egui::Align2::CENTER_BOTTOM.anchor_size(at(*p), galley.size()).expand2(egui::vec2(4.0, 1.0));
        painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(24, 23, 30, 190));
        painter.galley(rect.min + egui::vec2(4.0, 1.0), galley, TEXT);
    }
    // The handles on the chosen one, over the rest.
    stage::paint_handles(&painter, ed, view, screen);
}

/// How far the axis marker's lines reach (points), and how far its
/// middle stands in from the view's bottom left corner.
const AXIS_REACH: f32 = 22.0;
const AXIS_IN: f32 = 38.0;

/// A quiet axis marker in the view's bottom left corner: X red, Y green,
/// Z blue as the handles have them, each line the way its axis runs on
/// the screen (an axis toward the eye short), its letter at the tip.
fn axis_marker(ui: &egui::Ui, ed: &Editor, view: &View, free: egui::Rect) {
    let screen = ui.ctx().content_rect();
    let aspect = screen.width() / screen.height().max(1.0);
    let orbit = match ed.scope {
        super::model::Scope::Level => &view.level,
        _ => &view.block,
    };
    let vp = orbit.view_proj(aspect);
    let size = glam::Vec2::new(screen.width(), screen.height());
    let Some((middle, _)) = super::handles::project(vp, size, orbit.target) else { return };
    let step = orbit.zoom * 0.05;
    let ways: Vec<glam::Vec2> = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z].iter().map(|&a| super::handles::project(vp, size, orbit.target + a * step).map_or(glam::Vec2::ZERO, |(s, _)| s - middle)).collect();
    let longest = ways.iter().map(|w| w.length()).fold(0.0, f32::max);
    if longest <= 1e-3 {
        return;
    }
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("axis marker"))).with_clip_rect(free);
    let o = egui::pos2(free.left() + AXIS_IN, free.bottom() - AXIS_IN);
    let font = TextStyle::Small.resolve(ui.style());
    // The axis furthest from the eye first (its line under the others).
    let mut order = [0, 1, 2];
    order.sort_by(|&a, &b| ways[a].length().total_cmp(&ways[b].length()));
    for a in order {
        let w = ways[a] / longest * AXIS_REACH;
        let [r, g, b, _] = super::zones::AXIS[a].to_array();
        let colour = Color32::from_rgba_unmultiplied(r, g, b, 170);
        let tip = o + egui::vec2(w.x, w.y);
        painter.line_segment([o, tip], egui::Stroke::new(1.5, colour));
        // (An axis toward the eye: its letter beside the middle, not on
        // the others' lines.)
        let away = if w.length() > 0.3 * AXIS_REACH { w.normalize() * 8.0 } else { glam::Vec2::new(-9.0, 9.0) - w };
        painter.text(tip + egui::vec2(away.x, away.y), egui::Align2::CENTER_CENTER, ["X", "Y", "Z"][a], font.clone(), colour);
    }
}

/// The terrain's points over the view, on its surface: a small square
/// each, the one under the mouse and the chosen ones lit; SELECT's
/// rectangle.
/// How far apart on the screen (points, on average) the terrain's points
/// must stand for those away from the mouse to show faintly.
const FAINT_POINTS_APART: f32 = 14.0;

fn land_marks(ui: &egui::Ui, ed: &Editor, view: &View, free: egui::Rect) {
    let screen = ui.ctx().content_rect();
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("land marks"))).with_clip_rect(free);
    let vp = view.view_proj(&view.level, screen.width() / screen.height().max(1.0));
    let size = glam::Vec2::new(screen.width(), screen.height());
    let gold = ACCENT;
    let plain = Color32::from_rgb(150, 205, 235);
    let points = ed.points_on_screen(vp, size);
    // The other points faint, a dot each, where they stand apart enough
    // on the screen not to cover the land.
    let inside = points.iter().filter(|(_, s)| free.contains(egui::pos2(screen.left() + s.x, screen.top() + s.y))).count();
    let faint = inside > 0 && free.area() / inside as f32 >= FAINT_POINTS_APART * FAINT_POINTS_APART;
    for &(p, s) in &points {
        let at = egui::pos2(screen.left() + s.x, screen.top() + s.y);
        if !free.contains(at) {
            continue;
        }
        if !ed.point_shown(p) {
            if faint {
                painter.rect(egui::Rect::from_center_size(at, egui::vec2(3.0, 3.0)), 0.5, Color32::from_rgba_unmultiplied(24, 23, 30, 120), egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(150, 205, 235, 110)), egui::StrokeKind::Middle);
            }
            continue;
        }
        let chosen = ed.land.selected.contains(&p) || (ed.land.selected.is_empty() && ed.land.chosen == Some(p));
        let lit = ed.land.hover == Some(p);
        let r = if lit || chosen { 4.0 } else { 2.5 };
        let fill = if chosen { gold } else if lit { plain } else { Color32::from_rgba_unmultiplied(24, 23, 30, 200) };
        painter.rect(egui::Rect::from_center_size(at, egui::vec2(2.0 * r, 2.0 * r)), 1.0, fill, egui::Stroke::new(1.0, if chosen || lit { Color32::from_rgb(24, 23, 30) } else { plain }), egui::StrokeKind::Middle);
    }
    if let Some(m) = view.marquee {
        let at = |s: [f32; 2]| egui::pos2(screen.left() + s[0] * screen.width(), screen.top() + s[1] * screen.height());
        painter.rect_stroke(egui::Rect::from_two_pos(at(m.from), at(m.to)), 0.0, egui::Stroke::new(1.0, gold), egui::StrokeKind::Middle);
    }
}

/// What a key does on the panels as they were last drawn.
pub fn key_action(keys: &[(char, Action)], key: char) -> Option<Action> {
    let key = key.to_ascii_uppercase();
    keys.iter().find(|(k, _)| *k == key).map(|&(_, a)| a)
}

/// The panels laid out at 1600 by 900 as a window lays them (a few frames
/// for egui to settle), and what egui painted of them (for tests).
#[cfg(test)]
pub(crate) fn laid_out_at(ed: &Editor, view: &View) -> (Shown, Vec<egui::epaint::ClippedShape>) {
    let ctx = egui::Context::default();
    crate::gui::style(&ctx);
    let cfg = crate::session::config::<crate::Voxcade>(&ed.dir).unwrap_or_default();
    let (figures, mut thumbs) = (Figures::load(&ed.dir, &cfg).with_pieces(&ed.dir, &ed.level), Thumbs::default());
    let mut out = None;
    let mut shapes = Vec::new();
    for _ in 0..3 {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))), ..Default::default() };
        shapes = ctx.run_ui(raw, |u| out = Some(self::ui(u, ed, view, &figures, &mut thumbs))).shapes;
    }
    (out.expect("drawn"), shapes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GameConfig;
    use crate::editor::model::tests::folder;
    use crate::blocks::Turn;
    use crate::editor::model::{Layer, Scope, Tool};
    use crate::editor::view::act;
    use crate::grid::Kind;
    use glam::IVec3;

    /// The panels as they are drawn: what they offer and its keys.
    fn shown(ed: &Editor, view: &View) -> Shown {
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let (figures, mut thumbs) = (Figures::load(&ed.dir, &GameConfig::default()), Thumbs::default());
        let mut out = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| out = Some(ui_inner(ui, ed, view, &figures, &mut thumbs)));
        out.expect("drawn")
    }

    fn ui_inner(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs) -> Shown {
        super::ui(ui, ed, view, figures, thumbs)
    }

    fn press(ed: &mut Editor, view: &mut View, key: char) {
        let keys = shown(ed, view).keys;
        act(ed, view, key_action(&keys, key).unwrap_or_else(|| panic!("nothing on {key}")));
    }

    /// A model that does not read shows in ADD MODEL marked, with why on
    /// hover (`Thumbs::model`), not as an empty tile; one that reads has
    /// nothing to say, and a file mended says nothing more.
    #[test]
    fn a_model_that_does_not_read_says_why() {
        let dir = crate::scratch::Scratch::new("thumbs-broken");
        std::fs::write(dir.join("broken.vvx"), "format = \"voxcade voxel 1\"\nsize = [2, 2]\n").unwrap();
        std::fs::write(dir.join("good.vvx"), "format = \"voxcade voxel 1\"\nsize = [1, 1, 1]\n\n[palette]\na = \"#ff0000\"\n\n[[frame]]\nlayers = \"\"\"\na\n\"\"\"\n").unwrap();
        let ctx = egui::Context::default();
        let mut thumbs = Thumbs::default();
        let (_, _, why) = thumbs.model(&ctx, &dir, "broken.vvx", 64);
        let why = why.expect("a reason");
        assert!(why.contains("size"), "{why}");
        let (_, _, again) = thumbs.model(&ctx, &dir, "broken.vvx", 64);
        assert_eq!(again.as_deref(), Some(why.as_str()), "kept while the file stays");
        let (_, px, fine) = thumbs.model(&ctx, &dir, "good.vvx", 64);
        assert_eq!((fine, px), (None, egui::vec2(1.0, 1.0)));
        // A level of blocks without its cells says how a level without
        // blocks is written.
        std::fs::write(dir.join("room.toml"), "format = \"voxcade level 1\"\nblock = 10\nsize = [4, 4, 2]\n").unwrap();
        let e = format!("{:#}", Editor::open(&dir, "room.toml").err().expect("refused"));
        assert!(e.contains("leaves `block` out") && e.contains("`size` in voxels"), "{e}");
    }

    /// The tools' rows name their keys, the same on both levels, and do
    /// what clicking them does; the number keys take the block's first ten
    /// colours.
    #[test]
    fn keys_do_what_their_words_do() {
        let dir = folder("keys");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        let mut view = View::new(&ed, &GameConfig::default());
        press(&mut ed, &mut view, 'e');
        assert_eq!((ed.scope, ed.tool), (Scope::Level, Tool::Remove), "E: REMOVE in the level");
        press(&mut ed, &mut view, 'T');
        assert_eq!(ed.held.turn, Turn::TURN);
        act(&mut ed, &mut view, Action::Hold(0));
        assert_eq!(ed.scope, Scope::Block(0), "the held block, chosen again, opens");
        press(&mut ed, &mut view, 'p');
        assert_eq!(ed.tool, Tool::Paint, "P: PAINT in a block");
        press(&mut ed, &mut view, 'n');
        assert_eq!(ed.tool, Tool::Deep, "N: DEEP PAINT in a block");
        for i in 0..11u8 {
            ed.blocks[0].set(IVec3::new(i as i32 % 4, 0, i as i32 / 4), Some([10 * i, 0, 0])).unwrap();
        }
        let own = ed.colours().0;
        let keys = shown(&ed, &view).keys;
        assert_eq!(key_action(&keys, '1'), Some(Action::Colour(own[0])));
        assert_eq!(key_action(&keys, '0'), Some(Action::Colour(own[9])));
        assert!(key_action(&keys, 'J').is_none());
        press(&mut ed, &mut view, 'q');
        assert_eq!(ed.tool, Tool::Box, "Q: BOX in a block");
        press(&mut ed, &mut view, 'f');
        assert!(ed.forms.bevel, "F: BEVEL, with BOX");
        press(&mut ed, &mut view, 's');
        assert_eq!(ed.tool, Tool::Sphere, "S: SPHERE");
        press(&mut ed, &mut view, 'y');
        assert!(ed.symmetry, "Y: SYMMETRY");
        press(&mut ed, &mut view, 'b');
        assert_eq!(ed.scope, Scope::Level, "B: BACK");
        act(&mut ed, &mut view, Action::KindTo(Kind::Ladder));
        assert_eq!(ed.kind(), Some(Kind::Ladder), "the kind chosen");
        act(&mut ed, &mut view, Action::Hold(0));
        act(&mut ed, &mut view, Action::Hold(0));
        press(&mut ed, &mut view, 'd');
        assert_eq!((ed.scope, ed.level.blocks.len()), (Scope::Level, 0), "D: DELETE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A right click opens the menu (the window does it as the button
    /// comes up) and the next frame of the panels keeps it open, whether
    /// the press and the release came apart or in one batch (a click made
    /// by a program: the panels see both in one frame).
    #[test]
    fn a_right_click_keeps_its_menu_open() {
        use crate::editor::view::{Menu, MenuOn};
        let dir = folder("rclick");
        for together in [false, true] {
            let mut ed = Editor::open(&dir, "level.toml").unwrap();
            let mut view = View::new(&ed, &GameConfig::default());
            act(&mut ed, &mut view, Action::Hold(0));
            act(&mut ed, &mut view, Action::Hold(0));
            let ctx = egui::Context::default();
            crate::gui::style(&ctx);
            let (figures, mut thumbs) = (Figures::load(&ed.dir, &GameConfig::default()), Thumbs::default());
            let p = egui::pos2(700.0, 450.0);
            let mut t = 0.0;
            let mut frame = |ed: &mut Editor, view: &mut View, events: Vec<egui::Event>| {
                t += 0.05;
                let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))), time: Some(t), events, ..Default::default() };
                let mut out = None;
                let _ = ctx.run_ui(raw, |ui| out = Some(super::ui(ui, ed, view, &figures, &mut thumbs)));
                for a in out.unwrap().clicked {
                    act(ed, view, a);
                }
            };
            let button = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: Default::default() };
            frame(&mut ed, &mut view, vec![egui::Event::PointerMoved(p)]);
            let menu = Some(Menu { on: MenuOn::Voxel(IVec3::ZERO, None), at: [p.x, p.y] });
            if together {
                view.menu = menu;
                frame(&mut ed, &mut view, vec![button(true), button(false)]);
            } else {
                frame(&mut ed, &mut view, vec![button(true)]);
                view.menu = menu;
                frame(&mut ed, &mut view, vec![button(false)]);
            }
            frame(&mut ed, &mut view, vec![]);
            assert!(view.menu.is_some(), "together {together}: the menu closed at once");
            // A left click elsewhere (on the inspector) closes it.
            let far = egui::pos2(1500.0, 800.0);
            let left = |pressed| egui::Event::PointerButton { pos: far, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
            frame(&mut ed, &mut view, vec![egui::Event::PointerMoved(far), left(true), left(false)]);
            assert!(view.menu.is_none(), "together {together}: a left click elsewhere closes it");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A number field is typed into as a person does: a click on it (no
    /// drag) makes it a text field, the whole number chosen; digits and
    /// Enter set it (one action, clamped to its range); Escape leaves it
    /// as it was; a drag sideways still changes it.
    #[test]
    fn a_number_field_takes_typing() {
        use crate::editor::model::room::{Field, Item};
        let dir = crate::scratch::Scratch::new("type-number");
        std::fs::write(dir.join("room.toml"), crate::grid::tests::PIECES).unwrap();
        let mut ed = Editor::open(&dir, "room.toml").unwrap();
        let mut view = View::new(&ed, &GameConfig::default());
        act(&mut ed, &mut view, Action::ChooseItem(Some(Item::Thing(1))));
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let (figures, mut thumbs) = (Figures::load(&ed.dir, &GameConfig::default()), Thumbs::default());
        let mut t = 0.0;
        let mut frame = |ed: &mut Editor, view: &mut View, events: Vec<egui::Event>| -> Shown {
            t += 0.05;
            let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))), time: Some(t), events, ..Default::default() };
            let mut out = None;
            let _ = ctx.run_ui(raw, |ui| out = Some(super::ui(ui, ed, view, &figures, &mut thumbs)));
            let shown = out.unwrap();
            for &a in &shown.clicked {
                act(ed, view, a);
            }
            shown
        };
        let reach = |s: &Shown| s.fields.iter().find(|(w, _)| w == "RADIUS").expect("a RADIUS field").1.center();
        let at = reach(&frame(&mut ed, &mut view, vec![]));
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let key = |k| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() };
        frame(&mut ed, &mut view, vec![egui::Event::PointerMoved(at)]);
        // Press and release in one frame (a quick click, a program's).
        frame(&mut ed, &mut view, vec![button(true), button(false)]);
        assert!(ctx.egui_wants_keyboard_input(), "the field takes the keys");
        frame(&mut ed, &mut view, vec![egui::Event::Text("120".into())]);
        frame(&mut ed, &mut view, vec![key(egui::Key::Enter)]);
        frame(&mut ed, &mut view, vec![]);
        assert_eq!(ed.level.things[1].lights[0].radius, 120.0, "typed");
        assert!(!ctx.egui_wants_keyboard_input(), "Enter gives the keys back");
        // Escape: as it was.
        frame(&mut ed, &mut view, vec![button(true)]);
        frame(&mut ed, &mut view, vec![button(false)]);
        frame(&mut ed, &mut view, vec![egui::Event::Text("5".into())]);
        frame(&mut ed, &mut view, vec![key(egui::Key::Escape)]);
        frame(&mut ed, &mut view, vec![]);
        assert_eq!(ed.level.things[1].lights[0].radius, 120.0, "Escape leaves it");
        // Out of range: clamped.
        frame(&mut ed, &mut view, vec![button(true)]);
        frame(&mut ed, &mut view, vec![button(false)]);
        frame(&mut ed, &mut view, vec![egui::Event::Text("99999".into()), key(egui::Key::Enter)]);
        frame(&mut ed, &mut view, vec![]);
        assert_eq!(ed.level.things[1].lights[0].radius, 2000.0, "clamped");
        // A drag sideways still changes it.
        let before = ed.level.things[1].lights[0].radius;
        frame(&mut ed, &mut view, vec![button(true)]);
        for k in 1..=10 {
            frame(&mut ed, &mut view, vec![egui::Event::PointerMoved(at - egui::vec2(k as f32 * 6.0, 0.0))]);
        }
        frame(&mut ed, &mut view, vec![egui::Event::PointerButton { pos: at - egui::vec2(60.0, 0.0), button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        assert!(ed.level.things[1].lights[0].radius < before, "dragged left: less");
        let _ = Field::Radius(0);
    }

    /// The same through the window's own glue (SDL's events as egui's,
    /// `gui::input`): a click, the text, Return.
    #[test]
    fn a_number_field_takes_typing_through_the_glue() {
        use crate::editor::model::room::Item;
        use sdl3::event::Event as E;
        let dir = crate::scratch::Scratch::new("type-glue");
        std::fs::write(dir.join("room.toml"), crate::grid::tests::PIECES).unwrap();
        let mut ed = Editor::open(&dir, "room.toml").unwrap();
        let mut view = View::new(&ed, &GameConfig::default());
        act(&mut ed, &mut view, Action::ChooseItem(Some(Item::Thing(1))));
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let (figures, mut thumbs) = (Figures::load(&ed.dir, &GameConfig::default()), Thumbs::default());
        let mut input = crate::gui::input::Input::default();
        let mut frame = |ed: &mut Editor, view: &mut View, input: &mut crate::gui::input::Input, events: Vec<E>| -> Shown {
            for e in &events {
                input.handle(e, || None);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            let raw = input.take([1600, 900], 1.0, 1.0);
            let mut out = None;
            let _ = ctx.run_ui(raw, |ui| out = Some(super::ui(ui, ed, view, &figures, &mut thumbs)));
            let shown = out.unwrap();
            for &a in &shown.clicked {
                act(ed, view, a);
            }
            shown
        };
        let at = frame(&mut ed, &mut view, &mut input, vec![]).fields.iter().find(|(w, _)| w == "RADIUS").unwrap().1.center();
        let mouse = sdl3::mouse::MouseState::from_sdl_state(0);
        let button = |down: bool| if down { E::MouseButtonDown { timestamp: 0, window_id: 1, which: 0, mouse_btn: sdl3::mouse::MouseButton::Left, clicks: 1, x: at.x, y: at.y } } else { E::MouseButtonUp { timestamp: 0, window_id: 1, which: 0, mouse_btn: sdl3::mouse::MouseButton::Left, clicks: 1, x: at.x, y: at.y } };
        frame(&mut ed, &mut view, &mut input, vec![E::MouseMotion { timestamp: 0, window_id: 1, which: 0, mousestate: mouse, x: at.x, y: at.y, xrel: 0.0, yrel: 0.0 }]);
        frame(&mut ed, &mut view, &mut input, vec![button(true)]);
        frame(&mut ed, &mut view, &mut input, vec![button(false)]);
        assert!(ctx.egui_wants_keyboard_input(), "a text field now");
        let key = |down: bool| if down { E::KeyDown { timestamp: 0, window_id: 1, keycode: Some(sdl3::keyboard::Keycode::Return), scancode: Some(sdl3::keyboard::Scancode::Return), keymod: sdl3::keyboard::Mod::empty(), repeat: false, which: 0, raw: 0 } } else { E::KeyUp { timestamp: 0, window_id: 1, keycode: Some(sdl3::keyboard::Keycode::Return), scancode: Some(sdl3::keyboard::Scancode::Return), keymod: sdl3::keyboard::Mod::empty(), repeat: false, which: 0, raw: 0 } };
        frame(&mut ed, &mut view, &mut input, vec![E::TextInput { timestamp: 0, window_id: 1, text: "120".into() }]);
        frame(&mut ed, &mut view, &mut input, vec![key(true), key(false)]);
        frame(&mut ed, &mut view, &mut input, vec![]);
        assert_eq!(ed.level.things[1].lights[0].radius, 120.0);
    }

    /// The panels laid out at the window's size, as a window lays them.
    fn laid_out(ctx: &egui::Context, ed: &Editor, view: &View) -> Shown {
        let (figures, mut thumbs) = (Figures::load(&ed.dir, &GameConfig::default()), Thumbs::default());
        let mut out = None;
        for _ in 0..3 {
            let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))), ..Default::default() };
            let _ = ctx.run_ui(raw, |ui| out = Some(super::ui(ui, ed, view, &figures, &mut thumbs)));
        }
        out.expect("drawn")
    }

    /// A wide number never moves the inspector: the panels and every field
    /// stand where they stood with 0 in them, whether a field holds
    /// -180.5, 1000 or -1000; in the level editor (a piece's POSITION and
    /// ROTATE) and the voxel editor (a part's POSITION, ROTATE, JOINT).
    #[test]
    fn a_wide_number_never_moves_the_inspector() {
        use crate::editor::model::room::Item;
        let same = |what: &str, a: &Shown, b: &Shown| {
            // (The three zones; a bar at the chosen one goes with it.)
            assert_eq!(a.rects[..3], b.rects[..3], "{what}: the panels moved");
            assert_eq!(a.free, b.free, "{what}: the view's room changed");
            assert_eq!(a.fields.len(), b.fields.len(), "{what}: fields came or went");
            for ((w, r), (_, s)) in a.fields.iter().zip(&b.fields) {
                assert_eq!(r, s, "{what}: the field {w} moved or grew");
            }
        };
        // The level editor: the chair, every number in it wide.
        let dir = crate::scratch::Scratch::new("wide-level");
        std::fs::write(dir.join("room.toml"), crate::grid::tests::PIECES).unwrap();
        let mut ed = Editor::open(&dir, "room.toml").unwrap();
        let mut view = View::new(&ed, &GameConfig::default());
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        act(&mut ed, &mut view, Action::ChooseItem(Some(Item::Thing(3))));
        let set = |ed: &mut Editor, v: f64| {
            let t = &mut ed.level.things[3];
            t.at = [v as i32; 3];
            t.facing = Some(crate::grid::Facing::Turn(v));
            t.pitch = v;
            t.roll = v;
        };
        set(&mut ed, 0.0);
        let plain = laid_out(&ctx, &ed, &view);
        assert!(plain.fields.len() >= 6, "the chair's fields: {:?}", plain.fields);
        for v in [-180.5, 1000.0, -1000.0] {
            set(&mut ed, v);
            same(&format!("level, {v}"), &plain, &laid_out(&ctx, &ed, &view));
        }
        // The voxel editor: the arm, its place, rotation and joint wide.
        let (mut ed, mut view, _dir) = crate::editor::model::parts::tests::figure("wide");
        let arm = crate::editor::model::parts::tests::part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let set = |ed: &mut Editor, v: f32| {
            let model = ed.model.as_mut().unwrap();
            let o = model.parts.iter_mut().find(|o| o.name == "arm").unwrap();
            o.rotation = [v; 3];
            o.joint = Some(glam::Vec3::splat(v));
            ed.follow_figure();
        };
        set(&mut ed, 0.0);
        let plain = laid_out(&ctx, &ed, &view);
        assert!(plain.fields.len() >= 9, "the arm's fields: {:?}", plain.fields);
        for v in [-180.5, 1000.0, -1000.0] {
            set(&mut ed, v);
            same(&format!("model, {v}"), &plain, &laid_out(&ctx, &ed, &view));
        }
    }

    /// The bar at the chosen one stays where it is while MOVE and ROTATE
    /// switch, in the level editor (a piece) and the voxel editor (a part).
    #[test]
    fn the_bar_stays_while_move_and_rotate_switch() {
        use crate::editor::model::room::Item;
        let dir = crate::scratch::Scratch::new("bar-level");
        std::fs::write(dir.join("room.toml"), crate::grid::tests::PIECES).unwrap();
        let mut ed = Editor::open(&dir, "room.toml").unwrap();
        let mut view = View::new(&ed, &GameConfig::default());
        act(&mut ed, &mut view, Action::ChooseItem(Some(Item::Thing(3))));
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let moving = laid_out(&ctx, &ed, &view);
        view.turning = ed.pieces.chosen;
        assert!(view.turn_mode(&ed));
        let rotating = laid_out(&ctx, &ed, &view);
        assert!(moving.rects.len() > 3, "a bar at the chair");
        // (Its place; at a piece turned off the axes MOVE has LOCAL and
        // GLOBAL besides, so the bar is wider in MOVE.)
        assert_eq!(moving.rects[3].min, rotating.rects[3].min, "level: the bar moved");
        let (mut ed, mut view, _dir) = crate::editor::model::parts::tests::figure("bar");
        let arm = crate::editor::model::parts::tests::part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let moving = laid_out(&ctx, &ed, &view);
        act(&mut ed, &mut view, Action::PartRotating(true));
        let rotating = laid_out(&ctx, &ed, &view);
        assert!(moving.rects.len() > 3, "a bar at the arm");
        assert_eq!(moving.rects[3..], rotating.rects[3..], "model: the bar moved");
    }

    /// A bar that comes under the pointer takes no click until the pointer
    /// has moved a few pixels or 300 ms have passed (a double click's
    /// second click never lands on what the first put there); then it does.
    #[test]
    fn a_new_bar_takes_no_click_at_once() {
        use crate::editor::model::room::Item;
        let dir = crate::scratch::Scratch::new("bar-arm");
        std::fs::write(dir.join("room.toml"), crate::grid::tests::PIECES).unwrap();
        let mut ed = Editor::open(&dir, "room.toml").unwrap();
        let mut view = View::new(&ed, &GameConfig::default());
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let (figures, mut thumbs) = (Figures::load(&ed.dir, &GameConfig::default()), Thumbs::default());
        let frame = |ed: &Editor, view: &View, thumbs: &mut Thumbs, t: f64, events: Vec<egui::Event>| -> Shown {
            let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))), time: Some(t), events, ..Default::default() };
            let mut out = None;
            let _ = ctx.run_ui(raw, |ui| out = Some(super::ui(ui, ed, view, &figures, thumbs)));
            out.unwrap()
        };
        // Where the chair's bar will come: found once, then the bar gone.
        act(&mut ed, &mut view, Action::ChooseItem(Some(Item::Thing(3))));
        let bar = frame(&ed, &view, &mut thumbs, 0.0, vec![]).rects[3];
        let at = bar.left_center() + egui::vec2(24.0, 0.0);
        act(&mut ed, &mut view, Action::ChooseItem(None));
        frame(&ed, &view, &mut thumbs, 1.0, vec![egui::Event::PointerMoved(at)]);
        // Chosen with the pointer resting there: the bar comes under it.
        act(&mut ed, &mut view, Action::ChooseItem(Some(Item::Thing(3))));
        let click = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(&ed, &view, &mut thumbs, 1.02, vec![]);
        frame(&ed, &view, &mut thumbs, 1.05, vec![click(true)]);
        let early = frame(&ed, &view, &mut thumbs, 1.08, vec![click(false)]);
        assert!(early.clicked.is_empty(), "a click at once: nothing ({:?})", early.clicked);
        frame(&ed, &view, &mut thumbs, 1.5, vec![click(true)]);
        let late = frame(&ed, &view, &mut thumbs, 1.55, vec![click(false)]);
        assert!(!late.clicked.is_empty(), "after 300 ms: the button's action");
    }

    /// H and K switch the level's tools between its things and its blocks;
    /// M is MOVE among the things, MIRROR among the blocks; nothing of the
    /// blocks is offered among the things.
    #[test]
    fn the_things_have_their_own_tools_and_keys() {
        let dir = folder("layers");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        ed.kinds = vec!["fox".into(), "gem".into()];
        let mut view = View::new(&ed, &GameConfig::default());
        press(&mut ed, &mut view, 'h');
        assert_eq!(ed.layer, Layer::Things);
        let keys = shown(&ed, &view).keys;
        assert!(!keys.iter().any(|(_, a)| matches!(a, Action::Turn(_) | Action::Choose | Action::Clone)), "nothing of the blocks: {:?}", keys.iter().map(|k| k.0).collect::<String>());
        press(&mut ed, &mut view, 'm');
        assert_eq!((ed.tool, ed.held.turn), (Tool::Move, Turn::NONE), "M: MOVE");
        press(&mut ed, &mut view, 'k');
        assert_eq!((ed.layer, ed.tool), (Layer::Blocks, Tool::Add));
        press(&mut ed, &mut view, 'm');
        assert_eq!(ed.held.turn, Turn::MIRROR, "M: MIRROR");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

