//! The editor's panels, drawn with egui as the IDE is (`crate::gui`), so
//! the two are of a piece: on the right what is shown or held and the
//! tools, rows like the IDE's list with each one's key at the end; along
//! the bottom the level's blocks, the tileset's tiles, the kinds of things
//! or the colours, as pictures. What is clicked comes back as `Action`s,
//! and the keys the panels name do the same.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use egui::{Color32, RichText, TextStyle};

use super::model::{Editor, Layer, Scope, Tool};
use super::view::{Action, FRAMES_PER_SECOND, Figures, View, front, key_of, seen};
use crate::blocks::Turn;
use crate::grid::Kind;
use crate::gui::{ACCENT, ERROR, QUIET, TEXT};

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
    let things = ed.scope == Scope::Level && ed.layer == Layer::Things;
    match ed.scope {
        Scope::Level => {
            heading(ui, "Level");
            row(ui, shown, "BLOCKS", Action::Layer(Layer::Blocks), !things);
            row(ui, shown, "THINGS", Action::Layer(Layer::Things), things);
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
            if frames.len() > 1 {
                buttons(ui, shown, &[(if view.playing { "PAUSE" } else { "PLAY" }, Action::Play)]);
            }
            ui.add_space(4.0);
            ui.label(format!("FRAME {} OF {}", f + 1, frames.len()));
            buttons(ui, shown, &[("PREV", Action::Frame(-1)), ("NEXT", Action::Frame(1))]);
            buttons(ui, shown, &[("COPY", Action::CopyFrame), ("DELETE", Action::DeleteFrame)]);
            buttons(ui, shown, &[("TURN", Action::Turn(Turn::TURN)), ("TRIM", Action::Trim)]);
        }
    }
    heading(ui, "Tools");
    for &tool in Tool::of(ed.scope, ed.layer) {
        row(ui, shown, tool.label(ed.scope), Action::Tool(tool), ed.tool == tool);
    }
    if ed.shape().is_some() {
        if ed.tool == Tool::Box {
            row(ui, shown, "BEVEL", Action::Bevel, ed.bevel);
        }
        row(ui, shown, "SYMMETRY", Action::Symmetry, ed.symmetry);
        if ed.forms() {
            ui.label(RichText::new("Drag on a face, + and - its depth, click sets it. Shift: only where empty. Ctrl: cuts. Escape lets go.").small().color(QUIET));
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
    }
    ui.add_space(10.0);
    buttons(ui, shown, &[("UNDO", Action::Undo), ("REDO", Action::Redo)]);
    if let Scope::Block(_) = ed.scope {
        buttons(ui, shown, &[("BACK", Action::Back)]);
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
fn swatch(ui: &mut egui::Ui, c: [u8; 3], size: f32, chosen: bool, digit: Option<char>) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
    if chosen || response.hovered() {
        ui.painter().rect_stroke(rect.expand(2.0), 4.0, egui::Stroke::new(2.0, if chosen { ACCENT } else { QUIET }), egui::StrokeKind::Outside);
    }
    if let Some(d) = digit {
        let light = c[0] as u32 * 3 + c[1] as u32 * 6 + c[2] as u32 > 1300;
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, d, TextStyle::Small.resolve(ui.style()), if light { Color32::from_rgb(30, 28, 36) } else { TEXT });
    }
    response.on_hover_text(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).clicked()
}

/// The bar along the bottom: the level's blocks (NEW to draw one out of the
/// tileset), the tiles, the kinds of things, or the colours of the block
/// or model (its own first, numbered, then the others').
fn bar(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    let things = ed.scope == Scope::Level && ed.layer == Layer::Things;
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
        Scope::Block(_) | Scope::Frame(_) => {
            let (own, all) = ed.colours();
            // The block's own colours, big: the first ten on 1 to 9 and 0.
            for (i, &c) in own.iter().enumerate() {
                let digit = (i < 10).then(|| char::from_digit((i as u32 + 1) % 10, 10).expect("a digit"));
                if let Some(d) = digit {
                    shown.keys.push((d, Action::Colour(c)));
                }
                if swatch(ui, c, 40.0, c == ed.colour, digit) {
                    shown.clicked.push(Action::Colour(c));
                }
            }
            ui.add_space(12.0);
            for &c in all.iter().filter(|c| !own.contains(c)) {
                if swatch(ui, c, 24.0, c == ed.colour, None) {
                    shown.clicked.push(Action::Colour(c));
                }
            }
        }
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
        let mut ed = Editor::open(&dir, "level.json").unwrap();
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
        let mut ed = Editor::open(&dir, "level.json").unwrap();
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
