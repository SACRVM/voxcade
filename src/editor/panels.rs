//! The editor's panels, drawn with egui as the IDE is (`crate::gui`), so
//! the two are of a piece: on the right what is shown or held and the
//! tools, rows like the IDE's list with each one's key at the end; along
//! the bottom the level's blocks, the tileset's tiles, the kinds of things
//! or the colours, as pictures. What is clicked comes back as `Action`s,
//! and the keys the panels name do the same.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui::{Color32, RichText, TextStyle};

use super::figure::Figure;
use super::model::room::{self, Field, Item};
use super::model::{Editor, Layer, Scope, Tool};
use super::view::{Action, FRAMES_PER_SECOND, Figures, View, front, key_of, seen};
use crate::blocks::Turn;
use crate::grid::Kind;
use crate::gui::{ACCENT, ERROR, QUIET, TEXT, WARNING};

/// How wide the panel on the right is, how high the bar along the bottom.
pub const COLUMN: f32 = 250.0;
pub const BAR: f32 = 76.0;

/// What a frame of the panels gave: the clicks, the keys named on them
/// (a letter or a colour's digit, and what it does), and where they lie.
#[derive(Default)]
pub struct Shown {
    pub clicked: Vec<Action>,
    pub keys: Vec<(char, Action)>,
    pub rects: Vec<egui::Rect>,
    /// A name typed for an item and Enter pressed (`Editor::rename`).
    pub renamed: Option<(Item, String)>,
}

/// The pictures on the panels as textures, made again only when what they
/// show changed.
#[derive(Default)]
pub struct Thumbs(HashMap<String, (u64, egui::TextureHandle)>);

impl Thumbs {
    /// The picture `key`, made by `make` if new or changed (`of`: what it
    /// shows, hashed).
    fn get(&mut self, ctx: &egui::Context, key: String, of: impl Hash, make: impl FnOnce() -> (i32, i32, Vec<Option<[u8; 3]>>)) -> (egui::TextureId, egui::Vec2) {
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
}

/// The panels of a frame.
pub fn ui(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs) -> Shown {
    let mut shown = Shown::default();
    if let Some(names) = &ed.question {
        question(ui, &ed.file, names, &mut shown);
        return shown;
    }
    let frame = egui::Frame::new().fill(crate::gui::PANEL).inner_margin(egui::Margin::symmetric(14, 12));
    let right = egui::Panel::right("tools").exact_size(COLUMN).resizable(false).frame(frame).show(ui, |ui| {
        egui::ScrollArea::vertical().content_margin(egui::Margin { right: 6, ..egui::Margin::ZERO }).show(ui, |ui| column(ui, ed, view, figures, thumbs, &mut shown));
    });
    shown.rects.push(right.response.rect);
    let bottom = egui::Panel::bottom("bar").exact_size(BAR).resizable(false).frame(frame).show(ui, |ui| {
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal_centered(|ui| bar(ui, ed, view, figures, thumbs, &mut shown));
        });
    });
    shown.rects.push(bottom.response.rect);
    shown
}

/// The question before a level turned off the grid opens, over the whole
/// window in the panels' look: what is wrong in a sentence, which things
/// (the first few), ALIGN ALL and DON'T OPEN.
fn question(ui: &mut egui::Ui, file: &str, names: &[String], shown: &mut Shown) {
    let frame = egui::Frame::new().fill(crate::gui::PANEL).inner_margin(egui::Margin::same(40));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.25).max(0.0));
            ui.label(RichText::new(file).color(QUIET));
            ui.add_space(12.0);
            let (count, are) = if names.len() == 1 { ("One thing".to_string(), "is") } else { (format!("{} things", names.len()), "are") };
            ui.label(RichText::new(format!("{count} of this level {are} turned freely, and this game sets things on the grid in quarter turns.")).color(TEXT).heading());
            ui.add_space(12.0);
            let more = if names.len() > 4 { format!(" and {} more", names.len() - 4) } else { String::new() };
            ui.label(RichText::new(format!("{}{more}", names.iter().take(4).cloned().collect::<Vec<_>>().join(", "))).color(WARNING));
            ui.add_space(8.0);
            ui.label(RichText::new("ALIGN ALL turns each to its nearest way (a step back undoes it). Or allow free turns in conf.lua: c.camera.turns = \"free\".").color(QUIET));
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                let width = 2.0 * 180.0 + ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                for (label, action) in [("ALIGN ALL", Action::AlignAll), ("DON'T OPEN", Action::DontOpen)] {
                    if ui.add_sized([180.0, 36.0], egui::Button::new(RichText::new(label).color(if action == Action::AlignAll { ACCENT } else { TEXT }))).clicked() {
                        shown.clicked.push(action);
                    }
                }
            });
        });
    });
}

/// A small quiet heading, as the IDE's list has.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text.to_uppercase()).small().color(QUIET));
}

/// A row that does `action` (its key at the end), lit when `on`.
fn row(ui: &mut egui::Ui, shown: &mut Shown, label: &str, action: Action, on: bool) {
    let key = key_of(action);
    if let Some(k) = key {
        shown.keys.push((k, action));
    }
    let hint = key.map(String::from);
    if crate::gui::row(ui, label, TextStyle::Body, if on { ACCENT } else { TEXT }, on, hint.as_deref()).clicked() {
        shown.clicked.push(action);
    }
}

/// Buttons side by side, each with its key after its word.
fn buttons(ui: &mut egui::Ui, shown: &mut Shown, words: &[(&str, Action)]) {
    ui.horizontal_wrapped(|ui| {
        for &(label, action) in words {
            let key = key_of(action);
            if let Some(k) = key {
                shown.keys.push((k, action));
            }
            let mut job = egui::text::LayoutJob::default();
            let font = TextStyle::Button.resolve(ui.style());
            job.append(label, 0.0, egui::TextFormat { font_id: font, color: TEXT, ..Default::default() });
            if let Some(k) = key {
                job.append(&format!(" {k}"), 0.0, egui::TextFormat { font_id: TextStyle::Small.resolve(ui.style()), color: QUIET, valign: egui::Align::Center, ..Default::default() });
            }
            if ui.button(job).clicked() {
                shown.clicked.push(action);
            }
        }
    });
}

/// A picture `size` points big at most, its pixels whole.
fn picture(ui: &mut egui::Ui, (id, px): (egui::TextureId, egui::Vec2), size: f32) {
    let scale = (size / px.x.max(px.y).max(1.0)).floor().max(1.0);
    ui.image((id, px * scale));
}

/// The panel on the right: what is shown or held, its words, the tools,
/// the colour, UNDO and REDO.
fn column(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    if let Some(message) = &ed.message {
        ui.label(RichText::new(message).color(ERROR));
        ui.add_space(6.0);
    }
    // A figure of parts: its tree, and the part chosen shaped and rigged
    // under it.
    if let (Some(figure), Scope::Figure | Scope::Frame(_)) = (&ed.figure, ed.scope) {
        return figure_column(ui, ed, figure, shown);
    }
    if ed.among_pieces() {
        return pieces_column(ui, ed, view, figures, thumbs, shown);
    }
    let things = ed.scope == Scope::Level && ed.layer == Layer::Things;
    match ed.scope {
        Scope::Level => {
            heading(ui, "Level");
            row(ui, shown, "BLOCKS", Action::Layer(Layer::Blocks), !things);
            row(ui, shown, "THINGS", Action::Layer(Layer::Things), things);
            // (Offered where the level holds pieces, places or areas.)
            if has_pieces(ed) {
                row(ui, shown, "PIECES", Action::Layer(Layer::Pieces), false);
            }
            if things {
                if let Some(kind) = &ed.held_kind {
                    heading(ui, "Held");
                    let pic = thumbs.get(ui.ctx(), format!("kind {kind}"), kind, || figures.picture(kind, 64));
                    picture(ui, pic, 96.0);
                    ui.label(kind.to_uppercase());
                }
            } else if let Some(i) = ed.block_index() {
                heading(ui, "Held");
                let block = &ed.blocks[i];
                let turn = ed.held.turn;
                let pic = thumbs.get(ui.ctx(), "held".into(), (&block.voxels, &block.palette, turn), || {
                    let turned = block.turned(turn);
                    (turned.size.x, turned.size.z, front(&turned))
                });
                picture(ui, pic, 96.0);
                kind(ui, ed, shown);
                buttons(ui, shown, &[("TURN", Action::Turn(Turn::TURN)), ("ROLL", Action::Turn(Turn::ROLL)), ("MIRROR", Action::Turn(Turn::MIRROR))]);
                buttons(ui, shown, &[("CLONE", Action::Clone), ("DELETE", Action::Delete)]);
            }
        }
        Scope::Block(i) => {
            heading(ui, &format!("Block {}", ed.level.blocks[i].name));
            kind(ui, ed, shown);
            buttons(ui, shown, &[("TURN", Action::Turn(Turn::TURN)), ("CLONE", Action::Clone), ("DELETE", Action::Delete)]);
        }
        Scope::Figure => {}
        Scope::Frame(f) => {
            let name = std::path::Path::new(&ed.file).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
            let s = ed.grid();
            heading(ui, &format!("Model {name}"));
            ui.label(RichText::new(format!("{} × {} × {} voxels", s.x, s.y, s.z)).color(QUIET));
            // Its frames one after another as the game shows it, or the
            // one being shaped while they are held.
            let frames = ed.frames();
            let at = if view.playing && frames.len() > 1 {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(1.0 / FRAMES_PER_SECOND));
                (ui.input(|i| i.time) * FRAMES_PER_SECOND) as usize % frames.len()
            } else {
                f
            };
            let frame = &frames[at];
            let pic = thumbs.get(ui.ctx(), format!("frame {at}"), (&frame.voxels, &frame.palette, frame.size.to_array(), view.face as u8), || seen(frame, view.face));
            ui.add_space(4.0);
            picture(ui, pic, 160.0);
            // (A figure whose rig cannot be made, shaped part by part: its
            // frames are its parts, not to play, copy or take out.)
            let parted = ed.parted();
            if frames.len() > 1 && !parted {
                buttons(ui, shown, &[(if view.playing { "PAUSE" } else { "PLAY" }, Action::Play)]);
            }
            ui.add_space(4.0);
            ui.label(format!("{} {} OF {}", if parted { "PART" } else { "FRAME" }, f + 1, frames.len()));
            buttons(ui, shown, &[("PREV", Action::Frame(-1)), ("NEXT", Action::Frame(1))]);
            if !parted {
                buttons(ui, shown, &[("COPY", Action::CopyFrame), ("DELETE", Action::DeleteFrame)]);
            }
            buttons(ui, shown, &[("TURN", Action::Turn(Turn::TURN)), ("TRIM", Action::Trim)]);
        }
    }
    if !Tool::of(ed.scope, ed.layer).is_empty() {
        heading(ui, "Tools");
    }
    for &tool in Tool::of(ed.scope, ed.layer) {
        row(ui, shown, tool.label(ed.scope), Action::Tool(tool), ed.tool == tool);
    }
    if ed.shape().is_some() {
        if ed.resting_tool() == Tool::Box {
            row(ui, shown, "BEVEL", Action::Bevel, ed.bevel);
        }
        row(ui, shown, "SYMMETRY", Action::Symmetry, ed.symmetry);
        // (Kept while Shift makes the tool PICK, so the column does not jump.)
        if matches!(ed.resting_tool(), Tool::Box | Tool::Sphere) && ed.scope != Scope::Level {
            ui.label(RichText::new("Drag on a face, + and - its depth, click sets it. Alt: only where empty. Ctrl: cuts. Escape lets go.").small().color(QUIET));
        }
    }
    if ed.shape().is_some() {
        heading(ui, "Colour");
        let mut c = ed.colour;
        ui.horizontal(|ui| {
            if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Click for every colour.").changed() {
                shown.clicked.push(Action::Colour(c));
            }
            ui.label(RichText::new(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).color(QUIET));
        });
        material(ui, ed, shown);
    }
    ui.add_space(10.0);
    buttons(ui, shown, &[("UNDO", Action::Undo), ("REDO", Action::Redo)]);
    if let Scope::Block(_) = ed.scope {
        buttons(ui, shown, &[("BACK", Action::Back)]);
    }
}

/// FIGURE's panel: the figure's parts by what they hang on, the root
/// first, each with a mark that hides it (the picture only) and, quiet
/// under it, what is plainly wrong with how it hangs; the part chosen to
/// shape.
fn parts(ui: &mut egui::Ui, ed: &Editor, figure: &Figure, shown: &mut Shown) {
    let name = std::path::Path::new(&ed.file).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let s = figure.model.size;
    heading(ui, &format!("Figure {name}"));
    ui.label(RichText::new(format!("{} × {} × {} voxels, {} parts", s.x, s.y, s.z, figure.parts().len())).color(QUIET));
    heading(ui, "Parts");
    for &(p, depth) in &figure.tree {
        let (hidden, chosen) = (figure.hidden[p], figure.selected == Some(p));
        let indent = depth as f32 * 14.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(indent);
            // Shown: a filled square; hidden: an empty one.
            let (rect, mark) = ui.allocate_exact_size(egui::vec2(20.0, ui.text_style_height(&TextStyle::Body) + 10.0), egui::Sense::click());
            let square = egui::Rect::from_center_size(rect.center(), egui::vec2(9.0, 9.0));
            let colour = if mark.hovered() { TEXT } else { QUIET };
            if hidden {
                ui.painter().rect_stroke(square, 1.0, egui::Stroke::new(1.5, colour), egui::StrokeKind::Inside);
            } else {
                ui.painter().rect_filled(square, 1.0, colour);
            }
            if mark.on_hover_text(if hidden { "Show it" } else { "Hide it (the picture only)" }).clicked() {
                shown.clicked.push(Action::Hide(p));
            }
            // While a parent is being chosen: the parts that would make a
            // ring quiet, saying why.
            let ring = if ed.picking_parent { figure.selected.and_then(|s| ed.ring_with(s, p)) } else { None };
            let colour = if ring.is_some() { QUIET } else if chosen { ACCENT } else if hidden { QUIET } else { TEXT };
            let row = crate::gui::row(ui, &figure.parts()[p].name, TextStyle::Body, colour, chosen, None);
            let row = match ring {
                Some(why) => row.on_hover_text(why),
                None => row,
            };
            if row.clicked() {
                shown.clicked.push(Action::Part(p));
            }
        });
        // Under it, quiet: what is set by hand; what is plainly wrong.
        let part = &figure.parts()[p];
        let by_hand = match (part.hung == crate::vox::Hung::ByHand, part.joint_by_hand) {
            (true, true) => Some("parent and joint set by hand"),
            (true, false) => Some("parent set by hand"),
            (false, true) => Some("joint set by hand"),
            (false, false) => None,
        };
        let left = (indent + 30.0).min(120.0) as i8;
        let said = by_hand.map(|w| (w.to_string(), QUIET)).into_iter().chain(figure.warnings(p).into_iter().map(|w| (w, WARNING)));
        for (words, colour) in said {
            egui::Frame::new().inner_margin(egui::Margin { left, bottom: 4, ..egui::Margin::ZERO }).show(ui, |ui| ui.label(RichText::new(words).small().color(colour)));
        }
    }
    ui.add_space(6.0);
    if figure.hidden.iter().any(|&h| h) {
        buttons(ui, shown, &[("SHOW ALL", Action::ShowAll)]);
    }
    if figure.selected.is_none() {
        ui.label(RichText::new("Choose a part to shape it and set what it hangs on. The rings in the view: where each part turns.").small().color(QUIET));
    }
}

/// A figure's column: its parts, and the chosen part's parent and joint;
/// UNDO and REDO. Its tools and colours are on the bar (`figure_bar`), so
/// that the parts stay in view.
fn figure_column(ui: &mut egui::Ui, ed: &Editor, figure: &Figure, shown: &mut Shown) {
    parts(ui, ed, figure, shown);
    if let Some(p) = figure.selected {
        rig(ui, ed, figure, p, shown);
    }
    ui.add_space(10.0);
    buttons(ui, shown, &[("UNDO", Action::Undo), ("REDO", Action::Redo)]);
}

/// The bar while a figure's part is shaped: the tools side by side, BEVEL
/// and SYMMETRY, then the colours.
fn figure_bar(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let pick = |ui: &mut egui::Ui, shown: &mut Shown, label: &str, action: Action, on: bool| {
        let key = key_of(action);
        if let Some(k) = key {
            shown.keys.push((k, action));
        }
        let mut job = egui::text::LayoutJob::default();
        job.append(label, 0.0, egui::TextFormat { font_id: TextStyle::Button.resolve(ui.style()), color: if on { ACCENT } else { TEXT }, ..Default::default() });
        if let Some(k) = key {
            job.append(&format!(" {k}"), 0.0, egui::TextFormat { font_id: TextStyle::Small.resolve(ui.style()), color: QUIET, valign: egui::Align::Center, ..Default::default() });
        }
        if ui.selectable_label(on, job).clicked() {
            shown.clicked.push(action);
        }
    };
    for &tool in Tool::of(ed.scope, ed.layer) {
        pick(ui, shown, tool.label(ed.scope), Action::Tool(tool), ed.tool == tool);
    }
    ui.add_space(10.0);
    if ed.resting_tool() == Tool::Box {
        pick(ui, shown, "BEVEL", Action::Bevel, ed.bevel);
    }
    pick(ui, shown, "SYMMETRY", Action::Symmetry, ed.symmetry);
    ui.add_space(14.0);
    colours(ui, ed, shown);
    let mut c = ed.colour;
    if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Every colour.").changed() {
        shown.clicked.push(Action::Colour(c));
    }
    ui.add_space(8.0);
    material(ui, ed, shown);
}

/// The held colour's material in a model, quiet: a word of what it is
/// (PLAIN, or its words), opening on a click to the four sliders.
fn material(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let Some(m) = ed.material() else { return };
    let now = super::frames::words(m);
    let set: Vec<String> = super::frames::WORDS.iter().zip(now).enumerate().filter(|&(i, (_, v))| v > 0.0 && (i != 2 || m.metal != 0)).map(|(_, (w, v))| format!("{w} {}", (v * 100.0).round() / 100.0)).collect();
    let label = if set.is_empty() { "PLAIN".to_string() } else { set.join(" ") };
    ui.menu_button(RichText::new(label).color(QUIET), |ui| {
        ui.label(RichText::new("What this colour is made of, every voxel of it.").small().color(QUIET));
        for (i, word) in super::frames::WORDS.iter().enumerate() {
            let mut v = now[i];
            // ROUGH says how dull a mirror is: of metal or glass only.
            let mirrors = m.metal != 0 || m.rgba[3] < 255;
            ui.horizontal(|ui| {
                ui.add_sized(egui::vec2(56.0, 0.0), egui::Label::new(*word));
                let slider = ui.add_enabled(i != 2 || mirrors, egui::Slider::new(&mut v, 0.0..=1.0).step_by(0.05).fixed_decimals(2));
                if slider.changed() {
                    shown.clicked.push(Action::Material(i as u8, v));
                }
            });
        }
    })
    .response
    .on_hover_text("What the held colour is made of: glass, metal, how rough, how much it glows.");
}

/// The chosen part's parent and joint: what they are, whether set by hand,
/// and the buttons that set them or give them back to the rules.
fn rig(ui: &mut egui::Ui, ed: &Editor, figure: &Figure, p: usize, shown: &mut Shown) {
    let part = &figure.parts()[p];
    let quiet = |ui: &mut egui::Ui, text: &str| ui.label(RichText::new(text).small().color(QUIET));
    heading(ui, &format!("Part {}", part.name));
    let by_hand = part.hung == crate::vox::Hung::ByHand;
    ui.label(format!("PARENT {}", part.parent.map_or("none: the root", |q| figure.parts()[q].name.as_str())));
    quiet(ui, if by_hand { "set by hand" } else { "found by the rules" });
    if ed.picking_parent {
        ui.label(RichText::new("Click the part it hangs on in the list.").small().color(ACCENT));
        buttons(ui, shown, &[("CANCEL", Action::PickParent)]);
    } else if by_hand {
        buttons(ui, shown, &[("CHOOSE", Action::PickParent), ("BY THE RULES", Action::ParentWorkedOut)]);
    } else {
        buttons(ui, shown, &[("CHOOSE", Action::PickParent)]);
    }
    // Each way its own line: where it is, half a voxel less or more.
    let j = figure.joint(p);
    ui.add_space(4.0);
    ui.label("JOINT");
    quiet(ui, if part.joint_by_hand { "set by hand" } else { "found by the rules" });
    for (axis, name) in [(0u8, "X"), (1, "Y"), (2, "Z")] {
        ui.horizontal(|ui| {
            ui.label(RichText::new(name).color(QUIET));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(52.0, ui.text_style_height(&TextStyle::Body)), egui::Sense::hover());
            ui.painter().text(rect.right_center(), egui::Align2::RIGHT_CENTER, {
                // Halves as they are, a joint the rules found between them short.
                let v = j[axis as usize];
                if (v * 2.0).fract() == 0.0 { v.to_string() } else { format!("{v:.2}") }
            }, TextStyle::Body.resolve(ui.style()), TEXT);
            ui.add_space(8.0);
            for (way, sign) in [(-1i8, "-"), (1, "+")] {
                if ui.add(egui::Button::new(sign).min_size(egui::vec2(24.0, 0.0))).on_hover_text(format!("{name} half a voxel {}", if way < 0 { "less" } else { "more" })).clicked() {
                    shown.clicked.push(Action::Joint(axis, way));
                }
            }
        });
    }
    if part.joint_by_hand {
        buttons(ui, shown, &[("BY THE RULES", Action::JointWorkedOut)]);
    }
}

/// FIGURE's joints over the 3D view (in the room the panels at `rects`
/// leave): a ring where each part shown turns, the chosen part's bigger
/// with a line to where its parent turns, a part with a warning in the
/// warning's colour. Drawn over the voxels, so a joint inside the figure
/// shows.
pub fn joints(ui: &egui::Ui, ed: &Editor, view: &View, rects: &[egui::Rect]) {
    let screen = ui.ctx().content_rect();
    let marks = super::view::joint_marks(ed, view, screen.width() / screen.height().max(1.0), super::inset(rects, screen));
    let free = match rects {
        [right, bottom, ..] => egui::Rect::from_min_max(screen.min, egui::pos2(right.left(), bottom.top())),
        _ => screen,
    };
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("joints"))).with_clip_rect(free);
    // The editor's quiet word, at the foot of the view.
    // (BOX and SPHERE in a figure say how they go there: the column has
    // no room for it.)
    let forms = (ed.part_mode().is_some() && ed.forms()).then_some("Drag on a face, + and - its depth, click sets it. Alt: only where empty. Ctrl: cuts. Escape lets go.");
    if let Some(hint) = ed.hint.as_deref().or(forms) {
        painter.text(free.left_bottom() + egui::vec2(16.0, -14.0), egui::Align2::LEFT_BOTTOM, hint, TextStyle::Body.resolve(ui.style()), QUIET);
    }
    let at = |share: glam::Vec2| egui::pos2(screen.left() + share.x * screen.width(), screen.top() + share.y * screen.height());
    let chosen = marks.iter().any(|m| m.chosen);
    for m in marks.iter().filter(|m| m.chosen) {
        if let Some(parent) = m.parent {
            painter.line_segment([at(m.at), at(parent)], egui::Stroke::new(2.0, ACCENT));
        }
    }
    // The chosen one last, over the rest.
    for m in marks.iter().filter(|m| !m.chosen).chain(marks.iter().filter(|m| m.chosen)) {
        let (radius, colour) = match (m.chosen, m.warned) {
            (true, _) => (6.0, ACCENT),
            (false, true) => (4.0, WARNING),
            (false, false) if chosen => (3.0, QUIET),
            (false, false) => (4.0, TEXT),
        };
        painter.circle(at(m.at), radius, crate::gui::GROUND, egui::Stroke::new(if m.chosen { 2.5 } else { 1.5 }, colour));
    }
}

/// What the block shown or held is to the game, to choose.
fn kind(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let Some(now) = ed.kind() else { return };
    egui::ComboBox::from_id_salt("kind").selected_text(now.name().to_uppercase()).show_ui(ui, |ui| {
        for k in Kind::ALL {
            if ui.selectable_label(k == now, k.name().to_uppercase()).clicked() && k != now {
                shown.clicked.push(Action::KindTo(k));
            }
        }
    });
}

/// A picture on the bar that is clicked, framed in yellow when `chosen`.
fn thumb(ui: &mut egui::Ui, pic: (egui::TextureId, egui::Vec2), chosen: bool, tip: Option<&str>) -> bool {
    let room = BAR - 28.0;
    let scale = (room / pic.1.x.max(pic.1.y).max(1.0)).min(4.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(room, room), egui::Sense::click());
    if response.hovered() || chosen {
        ui.painter().rect_filled(rect.expand(3.0), 3.0, if chosen { crate::gui::ROW_OPEN } else { crate::gui::ROW_HOVERED });
    }
    let size = pic.1 * scale;
    ui.painter().image(pic.0, egui::Rect::from_center_size(rect.center(), size), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    if chosen {
        ui.painter().rect_stroke(rect.expand(3.0), 3.0, egui::Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
    }
    let response = match tip {
        Some(tip) => response.on_hover_text(tip),
        None => response,
    };
    response.clicked()
}

/// A colour on the bar, its digit on it (a key) if it has one.
fn swatch(ui: &mut egui::Ui, c: [u8; 3], size: f32, chosen: bool, digit: Option<char>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
    if chosen || response.hovered() {
        ui.painter().rect_stroke(rect.expand(2.0), 4.0, egui::Stroke::new(2.0, if chosen { ACCENT } else { QUIET }), egui::StrokeKind::Outside);
    }
    if let Some(d) = digit {
        let light = c[0] as u32 * 3 + c[1] as u32 * 6 + c[2] as u32 > 1300;
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, d, TextStyle::Small.resolve(ui.style()), if light { Color32::from_rgb(30, 28, 36) } else { TEXT });
    }
    response
}

/// The bar along the bottom: the level's blocks (NEW to draw one out of the
/// tileset), the tiles, the kinds of things, or the colours of the block
/// or model (its own first, numbered, then the others').
fn bar(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    let things = ed.scope == Scope::Level && ed.layer == Layer::Things;
    if ed.among_pieces() {
        return pieces_bar(ui, ed, view, shown);
    }
    match ed.scope {
        Scope::Level if things => {
            for (i, kind) in ed.kinds.iter().enumerate() {
                let pic = thumbs.get(ui.ctx(), format!("kind {kind}"), kind, || figures.picture(kind, 64));
                if thumb(ui, pic, ed.held_kind.as_ref() == Some(kind), Some(&kind.to_uppercase())) {
                    shown.clicked.push(Action::HoldKind(i));
                }
            }
        }
        Scope::Level if view.choosing => {
            buttons(ui, shown, &[("EMPTY", Action::Empty)]);
            if let Some(sheet) = &ed.sheet {
                for id in 1..=sheet.count {
                    let Some(pixels) = sheet.tile(id) else { continue };
                    let pic = thumbs.get(ui.ctx(), format!("tile {id}"), id, || (sheet.n, sheet.n, pixels.iter().map(|p| (p[3] >= 128).then_some([p[0], p[1], p[2]])).collect()));
                    if thumb(ui, pic, false, Some(&format!("tile {id}"))) {
                        shown.clicked.push(Action::Tile(id));
                    }
                }
            }
        }
        Scope::Level => {
            buttons(ui, shown, &[("NEW", Action::Choose)]);
            for (i, block) in ed.blocks[..ed.level.blocks.len()].iter().enumerate() {
                let pic = thumbs.get(ui.ctx(), format!("block {i}"), (&block.voxels, &block.palette), || (block.size.x, block.size.z, front(block)));
                let name = format!("{} ({})", ed.level.blocks[i].name, ed.level.blocks[i].kind.name());
                if thumb(ui, pic, ed.held.block as usize == i + 1, Some(&name)) {
                    shown.clicked.push(Action::Hold(i));
                }
            }
        }
        Scope::Figure => {
            ui.label(RichText::new("Choose a part on the right to shape it. Drag with the right button to turn the figure, with the middle one to move it; the wheel brings it nearer.").color(QUIET));
        }
        Scope::Frame(_) if ed.figure.is_some() => figure_bar(ui, ed, shown),
        Scope::Block(_) | Scope::Frame(_) => colours(ui, ed, shown),
    }
}

/// The colours of the block or model on the bar: its own first, big and
/// numbered, then the others'.
fn colours(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let (own, all) = ed.colours();
    // The block's own colours, big: the first ten on 1 to 9 and 0.
    for (i, &c) in own.iter().enumerate() {
        let digit = (i < 10).then(|| char::from_digit((i as u32 + 1) % 10, 10).expect("a digit"));
        if let Some(d) = digit {
            shown.keys.push((d, Action::Colour(c)));
        }
        let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        if swatch(ui, c, 40.0, c == ed.colour, digit).on_hover_text(hex).clicked() {
            shown.clicked.push(Action::Colour(c));
        }
    }
    ui.add_space(12.0);
    for &c in all.iter().filter(|c| !own.contains(c)) {
        let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        // A model's colour no voxel has any more: it keeps its place (and
        // its sign in the file) until taken out.
        let unused = ed.model.as_ref().is_some_and(|m| !m.uses(c));
        let response = swatch(ui, c, 24.0, c == ed.colour, None);
        let response = if unused { response.on_hover_text(format!("{hex}: no voxel has it. A right click takes it out of the model.")) } else { response.on_hover_text(hex) };
        if response.secondary_clicked() && unused {
            shown.clicked.push(Action::Forget(c));
        } else if response.clicked() {
            shown.clicked.push(Action::Colour(c));
        }
    }
}

/// Whether a level of blocks holds pieces, places or areas (the PIECES row
/// is offered then).
fn has_pieces(ed: &Editor) -> bool {
    ed.level.things.iter().any(|t| t.is_piece()) || !ed.level.places.is_empty() || !ed.level.areas.is_empty()
}

/// A colour of three bytes as egui's.
fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// `#rrggbb` as three bytes (white if it is none).
fn bytes_of(hex: Option<&str>) -> [u8; 3] {
    let h = hex.and_then(|h| h.strip_prefix('#')).filter(|h| h.len() == 6);
    h.map_or([255; 3], |h| [0, 2, 4].map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(255)))
}

/// The panel among the pieces: the level and its size, the list of what
/// is set down in it, and the chosen one's words and numbers; or the
/// models to choose one from (ADD MODEL).
fn pieces_column(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    if view.picker {
        return picker(ui, ed, thumbs, shown);
    }
    if ed.level.has_grid() {
        heading(ui, "Level");
        row(ui, shown, "BLOCKS", Action::Layer(Layer::Blocks), false);
        row(ui, shown, "THINGS", Action::Layer(Layer::Things), false);
        row(ui, shown, "PIECES", Action::Layer(Layer::Pieces), true);
    } else {
        heading(ui, "Room");
        ui.label(RichText::new(&ed.file).small().color(QUIET));
        let s = ed.level.size;
        // Its size, changed in a small window of its own (rarely wanted).
        ui.menu_button(RichText::new(format!("SIZE {}×{}×{}", s.x, s.y, s.z)).color(QUIET), |ui| {
            ui.label(RichText::new("How big the room is: across, deep and up.").small().color(QUIET));
            for (axis, word) in ["ACROSS", "DEEP", "UP"].iter().enumerate() {
                number(ui, shown, word, s[axis] as f64, Field::LevelSize(axis), 10.0, 1.0..=4096.0);
            }
        })
        .response
        .on_hover_text("The room's size in voxels. Click to change it.");
    }
    list(ui, ed, view, shown);
    match ed.chosen {
        Some(item) => chosen(ui, ed, view, figures, item, shown),
        None => {
            ui.add_space(8.0);
            ui.label(RichText::new("Click something in the view or the list to choose it. ADD below sets a new one down.").small().color(QUIET));
        }
    }
    let lights = ed.level.things.iter().filter(|t| t.light.is_some()).count();
    if lights > crate::render::MAX_LIGHTS {
        ui.add_space(8.0);
        ui.label(RichText::new(format!("{lights} lights: the game shows the {} nearest the middle of its view.", crate::render::MAX_LIGHTS)).small().color(WARNING));
    }
}

/// The list of what is set down: the things (pieces, lights alone, things
/// of a kind) by name, then the places, then the areas; the chosen one
/// lit and scrolled into view; an item being renamed as a field.
fn list(ui: &mut egui::Ui, ed: &Editor, view: &View, shown: &mut Shown) {
    // The list takes at most half the column while one is chosen.
    let room = ui.ctx().content_rect().height() * if ed.chosen.is_some() { 0.38 } else { 0.62 };
    // Scrolled to the chosen one when it changed (by a click in the view).
    let seen_id = egui::Id::unique("pieces list chosen");
    let last: Option<Option<Item>> = ui.data(|d| d.get_temp(seen_id));
    let scroll = last != Some(ed.chosen);
    ui.data_mut(|d| d.insert_temp(seen_id, ed.chosen));
    let groups: [(&str, Vec<Item>, &str); 3] = [
        ("In the level", (0..ed.level.things.len()).map(Item::Thing).collect(), "Nothing set down yet."),
        ("Places", (0..ed.level.places.len()).map(Item::Place).collect(), "None: a place is where a player arrives."),
        ("Areas", (0..ed.level.areas.len()).map(Item::Area).collect(), "None: an area is a box your code asks about."),
    ];
    egui::ScrollArea::vertical().id_salt("pieces list").max_height(room).auto_shrink([false, true]).show(ui, |ui| {
        for (title, items, none) in groups {
            heading(ui, title);
            if items.is_empty() {
                ui.label(RichText::new(none).small().color(QUIET));
            }
            for item in items {
                let chosen = ed.chosen == Some(item);
                if view.renaming == Some(item) {
                    rename_field(ui, ed, view, item, shown);
                    continue;
                }
                let (label, named) = ed.label_of(item);
                let light = match item {
                    Item::Thing(i) => ed.level.things[i].light.as_ref().map(|l| bytes_of(l.color.as_deref())),
                    _ => None,
                };
                let colour = if chosen { ACCENT } else if named { TEXT } else { QUIET };
                let response = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        // A light's colour in a small square before its name.
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, ui.text_style_height(&TextStyle::Body) + 10.0), egui::Sense::hover());
                        if let Some(c) = light {
                            ui.painter().rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(8.0, 8.0)), 2.0, rgb(c));
                        }
                        crate::gui::row(ui, &label, TextStyle::Body, colour, chosen, None)
                    })
                    .inner;
                let tip = match item {
                    Item::Thing(i) => {
                        let t = &ed.level.things[i];
                        match (&t.model, t.is_piece(), named) {
                            (Some(m), _, true) => m.clone(),
                            (Some(m), _, false) => format!("{m}: no name yet, so the game's code cannot ask for it by name"),
                            (None, true, _) => "a light alone".into(),
                            (None, false, _) => format!("a thing of the kind {}", t.kind),
                        }
                    }
                    Item::Place(_) => "a place a player arrives at".into(),
                    Item::Area(_) => "an area your code asks about".into(),
                };
                let response = response.on_hover_text(tip);
                if response.double_clicked() {
                    shown.clicked.push(Action::Rename(Some(item)));
                } else if response.clicked() {
                    shown.clicked.push(Action::ChooseItem(if chosen { None } else { Some(item) }));
                }
                if chosen && scroll {
                    response.scroll_to_me(None);
                }
            }
        }
    });
}

/// An item's name typed in the list: Enter keeps it (or says why not),
/// Escape leaves it as it was.
fn rename_field(ui: &mut egui::Ui, ed: &Editor, view: &View, item: Item, shown: &mut Shown) {
    let id = egui::Id::unique(("renaming", format!("{item:?}")));
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| ed.name_of(item).unwrap_or_default().to_string());
    let edit = ui.add(egui::TextEdit::singleline(&mut text).font(TextStyle::Body).hint_text("a name, then Enter").desired_width(f32::INFINITY));
    if !edit.has_focus() && !edit.lost_focus() && ui.data(|d| d.get_temp::<bool>(id.with("focused"))).is_none() {
        edit.request_focus();
        ui.data_mut(|d| d.insert_temp(id.with("focused"), true));
    }
    ui.data_mut(|d| d.insert_temp(id, text.clone()));
    if edit.lost_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            shown.renamed = Some((item, text));
        } else {
            shown.clicked.push(Action::Rename(None));
        }
        ui.data_mut(|d| {
            d.remove::<String>(id);
            d.remove::<bool>(id.with("focused"));
        });
    }
    if let Some(problem) = &view.rename_problem {
        ui.label(RichText::new(problem).small().color(ERROR));
    }
}

/// A number of the chosen one on a line of its own: its word, the number
/// (dragged, or clicked and typed), and - and + by `step`.
fn number(ui: &mut egui::Ui, shown: &mut Shown, word: &str, value: f64, field: Field, step: f64, range: std::ops::RangeInclusive<f64>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(50.0, ui.text_style_height(&TextStyle::Body)), egui::Sense::hover());
        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, word, TextStyle::Small.resolve(ui.style()), QUIET);
        let mut v = value;
        let drag = ui.add_sized([58.0, 24.0], egui::DragValue::new(&mut v).speed(0.25).range(range.clone()).fixed_decimals(0));
        if drag.on_hover_text("Drag, or click and type a number").changed() && v != value {
            shown.clicked.push(Action::Set(field, v));
        }
        for (sign, by) in [("-", -step), ("+", step)] {
            let said = if by < 0.0 { "less" } else { "more" };
            if ui.add(egui::Button::new(sign).min_size(egui::vec2(24.0, 0.0))).on_hover_text(format!("{} {said}", step)).clicked() {
                shown.clicked.push(Action::Set(field, (value + by).clamp(*range.start(), *range.end())));
            }
        }
    });
}

/// The chosen one: its name (RENAME) and what the game's code finds it
/// by, its model (EDIT MODEL), where it stands, its turn, its light; an
/// area's size; COPY and REMOVE.
fn chosen(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, item: Item, shown: &mut Shown) {
    let quiet = |ui: &mut egui::Ui, text: &str| ui.label(RichText::new(text).small().color(QUIET));
    let thing = match item {
        Item::Thing(i) => Some(&ed.level.things[i]),
        _ => None,
    };
    let what = match (item, thing) {
        (Item::Thing(_), Some(t)) if t.model.is_some() => "Piece",
        (Item::Thing(_), Some(t)) if t.is_piece() => "Light",
        (Item::Thing(_), _) => "Thing",
        (Item::Place(_), _) => "Place",
        (Item::Area(_), _) => "Area",
    };
    heading(ui, &format!("Chosen {what}"));
    let (label, named) = ed.label_of(item);
    if view.renaming != Some(item) {
        ui.label(RichText::new(&label).color(if named { ACCENT } else { QUIET }).heading());
    }
    // What the game's code asks for it by: said once, quietly.
    let asked = match (item, named) {
        (_, false) => "No name yet: RENAME gives it one, so the game's code can find it.".to_string(),
        (Item::Thing(_), true) => format!("The game's code finds it by its name: level:thing(\"{label}\")"),
        (Item::Place(_), true) => format!("The game's code sends a player here: world:go_to(level, \"{label}\")"),
        (Item::Area(_), true) => format!("The game's code asks about it: level.areas.{label}"),
    };
    quiet(ui, &asked);
    buttons(ui, shown, &[("RENAME", Action::Rename(Some(item)))]);
    if let Some(model) = thing.and_then(|t| t.model.as_ref()) {
        ui.add_space(4.0);
        quiet(ui, model);
        buttons(ui, shown, &[("EDIT MODEL", Action::EditModel)]);
    }
    let at = ed.at_of(item).unwrap_or_default();
    let voxels = ed.level.voxels();
    heading(ui, if item == Item::Area(0) || matches!(item, Item::Area(_)) { "Low corner" } else { "Stands at" });
    for (axis, word) in ["X", "Y", "Z"].iter().enumerate() {
        let word = match axis {
            2 => "Z UP",
            _ => word,
        };
        number(ui, shown, word, at[axis] as f64, Field::At(axis), 1.0, 0.0..=(voxels[axis] - 1).max(0) as f64);
    }
    // Which way it faces: the four ways on the grid; any angle where the
    // game turns things freely.
    let faces = match item {
        Item::Thing(_) => thing.is_some_and(|t| t.is_piece() && t.model.is_some()),
        Item::Place(_) => true,
        Item::Area(_) => false,
    };
    if faces {
        heading(ui, "Faces");
        let facing = match item {
            Item::Thing(_) => {
                let t = thing.expect("a thing");
                let b = room::bearing_of(t);
                match t.facing {
                    Some(crate::grid::Facing::Turn(_)) if t.facing.is_some_and(|f| f.off_grid()) => format!("{b}°"),
                    _ => crate::grid::Way::nearest(b).name().to_uppercase(),
                }
            }
            Item::Place(i) => {
                let p = &ed.level.places[i];
                let pair = p.facing.unwrap_or([0, -1]);
                crate::grid::Way::of_pair(pair).map_or(format!("[{}, {}]", pair[0], pair[1]), |w| w.name().to_uppercase())
            }
            Item::Area(_) => String::new(),
        };
        match thing.filter(|_| ed.free_turns) {
            // Turned freely: its angle, a quarter at a time too.
            Some(t) => {
                number(ui, shown, "ANGLE", room::bearing_of(t).rem_euclid(360.0), Field::Turn, 15.0, 0.0..=360.0);
                buttons(ui, shown, &[("TURN A QUARTER", Action::TurnItem(1))]);
                quiet(ui, "Degrees clockwise seen from above: 0 away from the camera, 180 towards it.");
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label(facing);
                    buttons(ui, shown, &[("TURN", Action::TurnItem(1))]);
                });
            }
        }
    }
    if let Some(t) = thing {
        light(ui, ed, figures, t, shown);
    }
    if let Item::Area(i) = item {
        heading(ui, "Size");
        let size = ed.level.areas[i].size;
        for (axis, word) in ["ACROSS", "DEEP", "UP"].iter().enumerate() {
            number(ui, shown, word, size[axis] as f64, Field::Size(axis), 1.0, 1.0..=4096.0);
        }
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        buttons(ui, shown, &[("COPY", Action::CopyItem)]);
        let mut job = egui::text::LayoutJob::default();
        job.append("REMOVE", 0.0, egui::TextFormat { font_id: TextStyle::Button.resolve(ui.style()), color: TEXT, ..Default::default() });
        job.append(" Del", 0.0, egui::TextFormat { font_id: TextStyle::Small.resolve(ui.style()), color: QUIET, valign: egui::Align::Center, ..Default::default() });
        if ui.button(job).on_hover_text("UNDO brings it back").clicked() {
            shown.clicked.push(Action::RemoveItem);
        }
    });
}

/// A thing's light: a piece's switched on or off; where it shines from in
/// the model, how far it reaches, its colour (a few at once, or any).
fn light(ui: &mut egui::Ui, ed: &Editor, figures: &Figures, t: &crate::grid::Thing, shown: &mut Shown) {
    let alone = t.model.is_none();
    if !t.is_piece() {
        return;
    }
    heading(ui, "Light");
    if !alone {
        let on = t.light.is_some();
        let top = t.model.as_deref().and_then(|m| figures.model(m)).map_or(10, |m| m.size.z);
        let mut job = egui::text::LayoutJob::default();
        job.append(if on { "ON" } else { "OFF" }, 0.0, egui::TextFormat { font_id: TextStyle::Button.resolve(ui.style()), color: if on { ACCENT } else { TEXT }, ..Default::default() });
        if ui.selectable_label(on, job).on_hover_text(if on { "The piece gives no light" } else { "The piece gives light, from its top at first" }).clicked() {
            shown.clicked.push(Action::Light(!on, top));
        }
    }
    let Some(l) = &t.light else { return };
    if !alone {
        let at = l.at.unwrap_or_default();
        ui.label(RichText::new("Shines from (the model's voxels, from where it stands):").small().color(QUIET));
        for (axis, word) in ["X", "Y", "Z UP"].iter().enumerate() {
            number(ui, shown, word, at[axis] as f64, Field::LightAt(axis), 1.0, -1000.0..=1000.0);
        }
    }
    number(ui, shown, "REACH", l.radius, Field::Radius, 10.0, 1.0..=2000.0);
    let now = bytes_of(l.color.as_deref());
    ui.horizontal_wrapped(|ui| {
        let mut c = now;
        if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Any colour").changed() {
            shown.clicked.push(Action::LightColour(c));
        }
        ui.add_space(6.0);
        for (hex, like) in room::LIGHT_COLOURS {
            let c = bytes_of(Some(hex));
            if swatch(ui, c, 20.0, c == now, None).on_hover_text(format!("Like {like}")).clicked() {
                shown.clicked.push(Action::LightColour(c));
            }
        }
    });
    let _ = ed;
}

/// ADD MODEL's list: the game's models by folder, each with a small
/// picture; a click hangs one on the mouse.
fn picker(ui: &mut egui::Ui, ed: &Editor, thumbs: &mut Thumbs, shown: &mut Shown) {
    heading(ui, "Add a model");
    ui.label(RichText::new("Click one, then click in the view where it is to stand.").small().color(QUIET));
    buttons(ui, shown, &[("CANCEL", Action::AddModel)]);
    let choices = ed.model_choices();
    if choices.is_empty() {
        ui.label(RichText::new("The game has no models (.vvx) yet.").color(QUIET));
    }
    let mut folder = None;
    for (i, file) in choices.iter().enumerate() {
        let (dir, name) = file.rsplit_once('/').map_or(("", file.as_str()), |(d, n)| (d, n));
        if folder != Some(dir) {
            heading(ui, if dir.is_empty() { "The game's folder" } else { dir });
            folder = Some(dir);
        }
        let stem = name.strip_suffix(".vvx").unwrap_or(name);
        let response = ui
            .horizontal(|ui| {
                let path = ed.dir.join(file);
                let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                let pic = thumbs.get(ui.ctx(), format!("model {file}"), (file, modified), || model_picture(&path, 32));
                let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                let scale = (32.0 / pic.1.x.max(pic.1.y).max(1.0)).min(4.0);
                ui.painter().image(pic.0, egui::Rect::from_center_size(rect.center(), pic.1 * scale), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                crate::gui::row(ui, stem, TextStyle::Body, TEXT, false, None)
            })
            .inner;
        if response.on_hover_text(file.as_str()).clicked() {
            shown.clicked.push(Action::Piece(i));
        }
    }
}

/// A model's first frame as a set-down piece shows it to the camera
/// (facing south: its front), at most `fit` pixels each way.
fn model_picture(path: &std::path::Path, fit: i32) -> (i32, i32, Vec<Option<[u8; 3]>>) {
    let Ok(m) = crate::vox::load_model(path) else { return (1, 1, vec![None]) };
    let voxels: Vec<(glam::IVec3, [u8; 3])> = m.frames.first().map_or(Vec::new(), |f| f.iter().map(|&(p, c)| (glam::IVec3::new(-p.x, -p.y, p.z), [c[0], c[1], c[2]])).collect());
    super::view::front_picture(&voxels, fit)
}

/// The bar among the pieces: what can be added, how the view looks,
/// UNDO and REDO.
fn pieces_bar(ui: &mut egui::Ui, ed: &Editor, view: &View, shown: &mut Shown) {
    let _ = ed;
    buttons_row(ui, shown, &[("ADD MODEL", Action::AddModel), ("ADD LIGHT", Action::AddLight), ("ADD PLACE", Action::AddPlace), ("ADD AREA", Action::AddArea)]);
    ui.add_space(24.0);
    buttons_row(ui, shown, &[("GAME VIEW", Action::GameView)]);
    let mut job = egui::text::LayoutJob::default();
    job.append(if view.lights { "LIGHTS ON" } else { "LIGHTS OFF" }, 0.0, egui::TextFormat { font_id: TextStyle::Button.resolve(ui.style()), color: TEXT, ..Default::default() });
    if ui.selectable_label(view.lights, job).on_hover_text("The pieces' lights in this view (the game's own are as the file says)").clicked() {
        shown.clicked.push(Action::Lights);
    }
    ui.add_space(24.0);
    buttons_row(ui, shown, &[("UNDO", Action::Undo), ("REDO", Action::Redo)]);
}

/// Buttons side by side in a row that is laid out already (the bar).
fn buttons_row(ui: &mut egui::Ui, shown: &mut Shown, words: &[(&str, Action)]) {
    for &(label, action) in words {
        let key = key_of(action);
        if let Some(k) = key {
            shown.keys.push((k, action));
        }
        let mut job = egui::text::LayoutJob::default();
        job.append(label, 0.0, egui::TextFormat { font_id: TextStyle::Button.resolve(ui.style()), color: TEXT, ..Default::default() });
        if let Some(k) = key {
            job.append(&format!(" {k}"), 0.0, egui::TextFormat { font_id: TextStyle::Small.resolve(ui.style()), color: QUIET, valign: egui::Align::Center, ..Default::default() });
        }
        if ui.button(job).clicked() {
            shown.clicked.push(action);
        }
    }
}

/// The marks among the pieces over the 3D view (in the room the panels at
/// `rects` leave): the chosen one's box, the lights in their colours,
/// places and areas with their names; and a quiet word on what the mouse
/// does.
pub fn marks(ui: &egui::Ui, ed: &Editor, view: &View, figures: &Figures, rects: &[egui::Rect]) {
    if !ed.among_pieces() || ed.question.is_some() {
        return;
    }
    let screen = ui.ctx().content_rect();
    let marks = super::view::marks(ed, view, figures, screen.width() / screen.height().max(1.0), super::inset(rects, screen));
    let free = match rects {
        [right, bottom, ..] => egui::Rect::from_min_max(screen.min, egui::pos2(right.left(), bottom.top())),
        _ => screen,
    };
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
        let fill = light.map_or(crate::gui::GROUND, rgb);
        painter.circle(at(*p), *radius, fill, egui::Stroke::new(if *tone == super::view::Tone::Chosen { 2.5 } else { 1.5 }, if *tone == super::view::Tone::Chosen { ACCENT } else { crate::gui::GROUND }));
    }
    for (p, name, tone) in &marks.names {
        let font = TextStyle::Small.resolve(ui.style());
        let galley = painter.layout_no_wrap(name.clone(), font, if *tone == super::view::Tone::Chosen { ACCENT } else { TEXT });
        let rect = egui::Align2::CENTER_BOTTOM.anchor_size(at(*p), galley.size()).expand2(egui::vec2(4.0, 1.0));
        painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(24, 23, 30, 190));
        painter.galley(rect.min + egui::vec2(4.0, 1.0), galley, TEXT);
    }
    // What the mouse does now, at the foot of the view.
    if ed.hint.is_none() {
        let word = if view.picker {
            "Choose a model on the right; then click where it is to stand."
        } else if ed.hanging.is_some() {
            "Click where it is to stand. T turns it, Escape lets go."
        } else if ed.chosen.is_some() {
            "Drag it to move it. Arrows nudge it (Shift: 10), Page Up and Page Down raise and lower it, T turns it. Right drag turns the view."
        } else {
            "Click a piece to choose it. Right drag turns the view, middle drag moves it, the wheel zooms."
        };
        painter.text(free.left_bottom() + egui::vec2(16.0, -14.0), egui::Align2::LEFT_BOTTOM, word, TextStyle::Body.resolve(ui.style()), QUIET);
    }
}

/// What a key does on the panels as they were last drawn.
pub fn key_action(keys: &[(char, Action)], key: char) -> Option<Action> {
    let key = key.to_ascii_uppercase();
    keys.iter().find(|(k, _)| *k == key).map(|&(_, a)| a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GameConfig;
    use crate::editor::model::tests::folder;
    use crate::editor::view::act;
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
        assert!(ed.bevel, "F: BEVEL, with BOX");
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
