//! The editors' one surface: what the level editor (`stage.rs`) and the
//! voxel editor (`shaping.rs`) are both made of, so that they look alike,
//! use the same words in the same places and are worked the same way.
//!
//! - Three zones: a bar along the top (what is open, + something, UNDO,
//!   REDO; on the right how the view looks), the outliner on the left (what
//!   there is, one row each, one scroll), the inspector on the right (the
//!   chosen one as short rows of a word and its controls, one scroll for
//!   the rare case it does not fit).
//! - In the view: a bar of buttons at the chosen thing, a menu at the mouse
//!   (a right click), the view's own buttons in its corner, a mistake as a
//!   short red line whose whole words are its tooltip.
//! - X red, Y green, Z blue, wherever an axis is: the handles and the
//!   letters of the number fields.
//!
//! Nothing here explains itself in sentences: a control's word, its value,
//! its key; what it does is its tooltip.

use egui::{Color32, Pos2, Rect, RichText, TextStyle, Vec2};

use super::panels::Shown;
use super::view::{Action, key_of};
use crate::gui::{ACCENT, ERROR, QUIET, TEXT, WARNING};

/// How tall the top bar is, how wide the outliner and the inspector: fixed,
/// so the view does not move when the choice changes.
pub const TOP: f32 = 46.0;
pub const OUTLINER: f32 = 220.0;
pub const INSPECTOR: f32 = 300.0;
/// The inspector's words before a row's controls.
pub const LABEL: f32 = 64.0;
/// A number field one of a row's three equal columns wide (`col`).
pub const FIELD: f32 = -1.0;
/// A field as wide as two columns and the gap between (RADIUS).
pub const FIELD2: f32 = -2.0;
/// The room between a row's controls.
const GAP: f32 = 4.0;

/// The width of `n` of a row's three equal columns (and the gaps between
/// them): from the left edge every row's controls begin at (after its
/// label) to the panel's right edge, so that every row's X, Y and Z, and
/// whatever else sits on the columns, line up down the inspector. Called
/// inside a `row`.
pub fn col(ui: &egui::Ui, n: usize) -> f32 {
    let one = ((ui.max_rect().width() - LABEL - 3.0 * GAP) / 3.0).floor();
    one * n as f32 + GAP * (n as f32 - 1.0)
}

/// The axes' colours, as the handles and the fields' letters have them: X
/// red, Y green, Z blue; and the same lit (under the mouse, dragged).
pub const AXIS: [Color32; 3] = [Color32::from_rgb(232, 92, 84), Color32::from_rgb(118, 204, 96), Color32::from_rgb(92, 148, 240)];
pub const AXIS_LIT: [Color32; 3] = [Color32::from_rgb(255, 170, 160), Color32::from_rgb(196, 250, 170), Color32::from_rgb(170, 205, 255)];

/// The three zones: the top bar, the outliner and the inspector (its
/// contents in one scroll), laid out with the contents given; where the
/// view's room is (`shown.free`, and returned).
pub fn layout(ui: &mut egui::Ui, shown: &mut Shown, top: impl FnOnce(&mut egui::Ui, &mut Shown), left: impl FnOnce(&mut egui::Ui, &mut Shown), right: impl FnOnce(&mut egui::Ui, &mut Shown)) -> Rect {
    let bar = egui::Frame::new().fill(crate::gui::PANEL).inner_margin(egui::Margin::symmetric(12, 6));
    // The side panels' scroll bars flush with their right edge, the rows
    // using the width up to them (no margin on the right: the bar's).
    let side = egui::Frame::new().fill(crate::gui::PANEL).inner_margin(egui::Margin { left: 10, right: 0, top: 10, bottom: 10 });
    let t = egui::Panel::top("zones top").exact_size(TOP).resizable(false).frame(bar).show(ui, |ui| ui.horizontal_centered(|ui| top(ui, shown)));
    let l = egui::Panel::left("zones outliner").exact_size(OUTLINER).resizable(false).frame(side).show(ui, |ui| {
        flush(ui);
        left(ui, shown)
    });
    let r = egui::Panel::right("zones inspector").exact_size(INSPECTOR).resizable(false).frame(side).show(ui, |ui| {
        flush(ui);
        egui::ScrollArea::vertical().id_salt("inspector").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            right(ui, shown)
        });
    });
    let screen = ui.ctx().content_rect();
    let free = Rect::from_min_max(Pos2::new(l.response.rect.right(), t.response.rect.bottom()), Pos2::new(r.response.rect.left(), screen.bottom()));
    shown.rects.extend([r.response.rect, t.response.rect, l.response.rect]);
    shown.free = Some(free);
    free
}

/// Scroll bars in `ui` at its right edge, the content up to them (a
/// little room between); what is not in a scroll keeps the same room.
pub fn flush(ui: &mut egui::Ui) {
    let scroll = &mut ui.spacing_mut().scroll;
    // Laid over the content's right margin (which is as wide as the bar
    // and a little room): the rows end at the same place whether a bar is
    // needed or not, the bar at the edge.
    scroll.floating = true;
    (scroll.floating_width, scroll.floating_allocated_width) = (scroll.bar_width, 0.0);
    (scroll.bar_outer_margin, scroll.bar_inner_margin) = (0.0, 0.0);
    scroll.content_margin = egui::Margin { right: (scroll.bar_width + 4.0) as i8, ..egui::Margin::ZERO };
    // Always seen while there is something to scroll, as a solid one.
    (scroll.dormant_background_opacity, scroll.dormant_handle_opacity) = (0.4, 0.6);
}

/// What does not scroll in a side panel (a field over its list): as wide
/// as the rows under it, clear of their scroll bar.
pub fn beside_bar(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let room = ui.available_width() - ui.spacing().scroll.content_margin.right as f32;
    ui.allocate_ui(egui::vec2(room, 0.0), add);
}

/// A mistake over the view's top left: short, red, the whole words its
/// tooltip.
pub fn mistake(ui: &egui::Ui, free: Rect, text: &str, shown: &mut Shown) {
    say(ui, free, text, ERROR, shown);
}

/// What the editor did or would not, over the view's top left: short, in
/// `colour`, the whole words its tooltip.
pub fn say(ui: &egui::Ui, free: Rect, text: &str, colour: Color32, shown: &mut Shown) {
    // The words up to the first stop, cut.
    let short: String = text.split([':', '.', '(']).next().unwrap_or(text).trim().chars().take(40).collect();
    let short = if short.len() < text.trim().len() { format!("{short}…") } else { short };
    let area = egui::Area::new(egui::Id::unique("zones mistake")).order(egui::Order::Foreground).fixed_pos(free.min + Vec2::new(12.0, 10.0)).show(ui.ctx(), |ui| {
        egui::Frame::new().fill(Color32::from_rgba_unmultiplied(24, 23, 30, 230)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.label(RichText::new(short).color(colour)).on_hover_text(text);
        });
    });
    shown.rects.push(area.response.rect);
}

/// Over the view's top left: what came from the disk (`disk.rs`), unless
/// a mistake is said there; where a file changed while a change of the
/// editor's could not be written (a mistake said why), whose to keep,
/// under it.
pub fn from_disk(ui: &egui::Ui, free: Rect, ed: &super::model::Editor, shown: &mut Shown) {
    if let Some(path) = &ed.disk.clash {
        let name = path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let area = egui::Area::new(egui::Id::unique("zones clash")).order(egui::Order::Foreground).fixed_pos(free.min + Vec2::new(12.0, 44.0)).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(Color32::from_rgba_unmultiplied(24, 23, 30, 230)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CHANGED ON DISK").color(WARNING)).on_hover_text(format!("Another program changed {name} while a change made here could not be written."));
                    small(ui, shown, "KEEP MINE", Action::KeepMine, None, &format!("Write what the editor holds over {name}"));
                    small(ui, shown, "TAKE THE FILE'S", Action::TakeFile, None, &format!("Read {name} anew; what the editor held of it goes"));
                });
            });
        });
        shown.rects.push(area.response.rect);
    } else if ed.message.is_none()
        && let Some(n) = &ed.disk.notice
    {
        say(ui, free, n, WARNING, shown);
    }
}

/// The mark at a row's left that hides it from the picture (in the
/// editor only): a filled square while shown, an empty one while hidden;
/// the figure's parts and the level's things alike.
pub fn hide_mark(ui: &mut egui::Ui, hidden: bool, action: Action, shown: &mut Shown) {
    let (rect, mark) = ui.allocate_exact_size(egui::vec2(18.0, ui.text_style_height(&TextStyle::Body) + 8.0), egui::Sense::click());
    let square = Rect::from_center_size(rect.center(), egui::vec2(9.0, 9.0));
    let colour = if mark.hovered() { TEXT } else { QUIET };
    if hidden {
        ui.painter().rect_stroke(square, 1.0, egui::Stroke::new(1.5, colour), egui::StrokeKind::Inside);
    } else {
        ui.painter().rect_filled(square, 1.0, colour);
    }
    if mark.on_hover_text(if hidden { "Show it" } else { "Hide it (from the picture only)" }).clicked() {
        shown.clicked.push(action);
    }
}

/// A button whose key follows its word, quiet; `on`: a toggle, lit while
/// on (it still looks as a button does: what is clicked has a frame). Its
/// key works while the panels show it.
pub fn keyed(ui: &mut egui::Ui, shown: &mut Shown, label: &str, action: Action, on: Option<bool>, tip: &str) -> egui::Response {
    let key = key_of(action);
    if let Some(k) = key {
        shown.keys.push((k, action));
    }
    let mut job = egui::text::LayoutJob::default();
    let colour = if on == Some(true) { ACCENT } else { TEXT };
    job.append(label, 0.0, egui::TextFormat { font_id: TextStyle::Button.resolve(ui.style()), color: colour, ..Default::default() });
    if let Some(k) = key {
        job.append(&format!(" {k}"), 0.0, egui::TextFormat { font_id: TextStyle::Small.resolve(ui.style()), color: QUIET, valign: egui::Align::Center, ..Default::default() });
    }
    let response = ui.add(egui::Button::new(job).selected(on == Some(true)));
    let response = if tip.is_empty() { response } else { response.on_hover_text(tip) };
    if response.clicked() {
        shown.clicked.push(action);
    }
    response
}

/// A small button of a word (`on`: lit, a toggle), its tooltip naming
/// what it does and its key; the key works while it is shown.
pub fn small(ui: &mut egui::Ui, shown: &mut Shown, word: &str, action: Action, on: Option<bool>, tip: &str) -> egui::Response {
    if let Some(k) = key_of(action) {
        shown.keys.push((k, action));
    }
    let mut job = egui::text::LayoutJob::default();
    job.append(word, 0.0, egui::TextFormat { font_id: TextStyle::Small.resolve(ui.style()), color: if on == Some(true) { ACCENT } else { TEXT }, ..Default::default() });
    let response = ui.add(egui::Button::new(job).selected(on == Some(true))).on_hover_text(tip);
    if response.clicked() {
        shown.clicked.push(action);
    }
    response
}

/// A row of the inspector: its word (what it is on hover), then what is
/// laid in it.
pub fn row(ui: &mut egui::Ui, word: &str, tip: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(LABEL, ui.text_style_height(&TextStyle::Body) + 6.0), egui::Sense::hover());
        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, word, TextStyle::Small.resolve(ui.style()), QUIET);
        if !tip.is_empty() {
            response.on_hover_text(tip);
        }
        add(ui);
    });
}

/// A row's word standing on one of its columns (quiet, as the rows'
/// labels), to name the field after it.
pub fn cell_word(ui: &mut egui::Ui, word: &str, tip: &str) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(col(ui, 1), ui.text_style_height(&TextStyle::Body) + 6.0), egui::Sense::hover());
    ui.painter().text(rect.left_center() + Vec2::new(2.0, 0.0), egui::Align2::LEFT_CENTER, word, TextStyle::Small.resolve(ui.style()), QUIET);
    if !tip.is_empty() {
        response.on_hover_text(tip);
    }
}

/// A number field: its letter before the number, in its axis's colour
/// (`axis` 0..2; else quiet), dragged sideways or typed, to `decimals`
/// places at most; a change gives the action `set` makes of the new value.
#[allow(clippy::too_many_arguments)]
pub fn number(ui: &mut egui::Ui, shown: &mut Shown, letter: &str, axis: Option<usize>, value: f64, decimals: usize, speed: f64, range: std::ops::RangeInclusive<f64>, tip: &str, width: f32, set: impl FnOnce(f64) -> Action) {
    let colour = axis.map_or(QUIET, |a| AXIS[a]);
    let mut v = value;
    // On the columns (`FIELD`: one, `FIELD2`: two), else as wide as asked.
    let width = if width < 0.0 { col(ui, (-width) as usize) } else { width };
    // The number right-aligned: padded to as many signs as the field
    // holds (one typeface, every sign as wide), so that the letters stand
    // at the same place in every field and 0, 120 and -18 end together.
    let font = ui.style().drag_value_text_style.resolve(ui.style());
    let small = TextStyle::Small.resolve(ui.style());
    let (sign, letters) = ui.ctx().fonts_mut(|f| (f.glyph_width(&font, '0'), f.glyph_width(&small, '0') * (letter.chars().count() + 1) as f32));
    let room = width - 2.0 * ui.spacing().button_padding.x - letters - 2.0;
    let signs = (room / sign.max(1.0)).floor().max(1.0) as usize;
    // Typed, it takes effect on Enter or a click elsewhere (one step),
    // Escape leaving it as it was.
    let drag = egui::DragValue::new(&mut v)
        .speed(speed)
        .range(range)
        .min_decimals(0)
        .max_decimals(decimals)
        .update_while_editing(false)
        .custom_formatter(move |v, r| format!("{:>signs$}", egui::emath::format_with_decimals_in_range(v, r)))
        .prefix(RichText::new(format!("{letter} ")).color(colour).small());
    // Escape while typing: the number as it was. (egui keeps the text
    // and sets it the frame after, when Escape is no longer pressed: the
    // field remembers it was left so and drops that.)
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let response = ui.add_sized([width, ui.text_style_height(&TextStyle::Body) + 8.0], drag);
    shown.fields.push((letter.to_string(), response.rect));
    let id = response.id;
    let escaped = escape && (response.lost_focus() || response.has_focus());
    let left_by_escape = ui.data_mut(|d| d.remove_temp::<bool>(id.with("escaped")).is_some());
    if escaped {
        ui.data_mut(|d| d.insert_temp(id.with("escaped"), true));
    }
    if escaped || left_by_escape {
        return;
    }
    let response = if tip.is_empty() { response } else { response.on_hover_text(tip) };
    if response.changed() && v != value {
        shown.clicked.push(set(v));
    }
}

/// What sort of thing a row of an outliner is, for its sign.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    /// A model set down, a light, a thing of a kind, a place, an area (the
    /// level's); a block, a frame of a model, a part of a figure.
    Piece,
    Light,
    Kind,
    Place,
    Area,
    Block,
    Frame,
    Part,
}

/// A small sign for what a row is, drawn in `colour` round `at`.
pub fn sign(painter: &egui::Painter, at: Pos2, sort: Sort, colour: Color32) {
    let s = egui::Stroke::new(1.5, colour);
    let r = 5.0;
    match sort {
        Sort::Piece | Sort::Block => {
            // A box seen from a corner.
            painter.rect_stroke(Rect::from_center_size(at + Vec2::new(-1.0, 1.0), Vec2::splat(r * 1.6)), 1.0, s, egui::StrokeKind::Middle);
            painter.line_segment([at + Vec2::new(-r * 0.8 - 1.0, -r * 0.8 + 1.0), at + Vec2::new(-r * 0.8 + 2.0, -r * 0.8 - 2.0)], s);
            painter.line_segment([at + Vec2::new(-r * 0.8 + 2.0, -r * 0.8 - 2.0), at + Vec2::new(r * 0.8 + 2.0, -r * 0.8 - 2.0)], s);
            painter.line_segment([at + Vec2::new(r * 0.8 + 2.0, -r * 0.8 - 2.0), at + Vec2::new(r * 0.8 + 2.0, r * 0.8 - 2.0)], s);
        }
        Sort::Light => {
            painter.circle_stroke(at, r * 0.7, s);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let d = Vec2::new(a.cos(), a.sin());
                painter.line_segment([at + d * r * 1.0, at + d * r * 1.4], s);
            }
        }
        Sort::Kind => {
            painter.circle_stroke(at + Vec2::new(0.0, -r * 0.7), r * 0.45, s);
            painter.line_segment([at + Vec2::new(0.0, -r * 0.2), at + Vec2::new(0.0, r * 0.6)], s);
            painter.line_segment([at + Vec2::new(-r * 0.6, r * 1.1), at + Vec2::new(0.0, r * 0.6)], s);
            painter.line_segment([at + Vec2::new(r * 0.6, r * 1.1), at + Vec2::new(0.0, r * 0.6)], s);
        }
        Sort::Place => {
            painter.circle_stroke(at + Vec2::new(0.0, r * 0.4), r * 0.8, s);
            painter.line_segment([at + Vec2::new(0.0, r * 0.4), at + Vec2::new(0.0, -r * 1.2)], s);
            painter.line_segment([at + Vec2::new(0.0, -r * 1.2), at + Vec2::new(-r * 0.5, -r * 0.6)], s);
            painter.line_segment([at + Vec2::new(0.0, -r * 1.2), at + Vec2::new(r * 0.5, -r * 0.6)], s);
        }
        Sort::Area => {
            let b = Rect::from_center_size(at, Vec2::splat(r * 2.2));
            for (a, c) in [(b.left_top(), Vec2::X), (b.left_top(), Vec2::Y), (b.right_top(), -Vec2::X), (b.right_top(), Vec2::Y), (b.left_bottom(), Vec2::X), (b.left_bottom(), -Vec2::Y), (b.right_bottom(), -Vec2::X), (b.right_bottom(), -Vec2::Y)] {
                painter.line_segment([a, a + c * r * 0.8], s);
            }
        }
        Sort::Frame => {
            // A film's frame.
            let b = Rect::from_center_size(at, Vec2::new(r * 2.0, r * 1.6));
            painter.rect_stroke(b, 1.0, s, egui::StrokeKind::Middle);
            painter.line_segment([b.left_top() + Vec2::new(0.0, 3.0), b.right_top() + Vec2::new(0.0, 3.0)], s);
        }
        Sort::Part => {
            // A limb between two joints.
            let (a, b) = (at + Vec2::new(-r * 0.8, r * 0.8), at + Vec2::new(r * 0.8, -r * 0.8));
            painter.line_segment([a, b], egui::Stroke::new(2.5, colour));
            painter.circle_filled(a, 2.5, colour);
            painter.circle_filled(b, 2.5, colour);
        }
    }
}

/// One row of an outliner, the whole row clicked: indented `indent`, its
/// sign (or a small picture, `pic`), its words cut at the edge, quiet when
/// `quiet`; a dot of colour at its end, and a sign of warning with a short
/// word before it; chosen, marked. Its tooltip is `tip`.
#[allow(clippy::too_many_arguments)]
pub fn list_row(ui: &mut egui::Ui, sort: Sort, pic: Option<(egui::TextureId, Vec2)>, label: &str, indent: f32, chosen: bool, quiet: bool, dot: Option<Color32>, warn: Option<&str>, tip: &str) -> egui::Response {
    let height = if pic.is_some() { 40.0 } else { ui.text_style_height(&TextStyle::Body) + 8.0 };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    if chosen || response.hovered() {
        painter.rect_filled(rect, 3.0, if chosen { crate::gui::ROW_OPEN } else { crate::gui::ROW_HOVERED });
    }
    if chosen {
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())), 1.0, ACCENT);
    }
    let colour = if chosen { ACCENT } else if quiet { QUIET } else { TEXT };
    let left = rect.left() + indent;
    let text_at = match pic {
        Some((tex, px)) => {
            let scale = (32.0 / px.x.max(px.y).max(1.0)).floor().clamp(1.0, 4.0);
            let r = Rect::from_center_size(Pos2::new(left + 22.0, rect.center().y), px * scale);
            painter.image(tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            left + 44.0
        }
        None => {
            sign(&painter, Pos2::new(left + 14.0, rect.center().y), sort, if chosen { ACCENT } else { QUIET });
            left + 28.0
        }
    };
    // The end: a warning's sign and word, a dot of colour.
    let font = TextStyle::Body.resolve(ui.style());
    let mut right = rect.right() - 6.0;
    if let Some(c) = dot {
        painter.circle_filled(Pos2::new(right - 4.0, rect.center().y), 4.0, c);
        right -= 14.0;
    }
    if let Some(word) = warn {
        let g = painter.layout_no_wrap(word.to_string(), TextStyle::Small.resolve(ui.style()), WARNING);
        let at = Pos2::new(right - g.size().x, rect.center().y - g.size().y / 2.0);
        painter.galley(at, g.clone(), WARNING);
        let t = Pos2::new(at.x - 9.0, rect.center().y);
        painter.add(egui::Shape::convex_polygon(vec![t + Vec2::new(0.0, -5.0), t + Vec2::new(5.0, 4.0), t + Vec2::new(-5.0, 4.0)], Color32::TRANSPARENT, egui::Stroke::new(1.3, WARNING)));
        right = at.x - 18.0;
    }
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_string(), font, colour);
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - text_at).max(10.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    painter.galley(Pos2::new(text_at, rect.center().y - galley.size().y / 2.0), galley, colour);
    if tip.is_empty() { response } else { response.on_hover_text(tip) }
}

/// A heading of an outliner's group that folds with a click (open at
/// first; remembered while the editor is open), its count after it.
pub fn group(ui: &mut egui::Ui, title: &str, count: Option<usize>, tip: &str) -> bool {
    let open_id = egui::Id::unique(("zones group", title));
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(true);
    let (rect, heading) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ui.text_style_height(&TextStyle::Small) + 12.0), egui::Sense::click());
    if heading.hovered() {
        ui.painter().rect_filled(rect, 3.0, crate::gui::ROW_HOVERED);
    }
    crate::gui::arrow(ui, Pos2::new(rect.left() + 8.0, rect.center().y), if open { 1.0 } else { 0.0 });
    let words = count.map_or(title.to_string(), |n| format!("{title} {n}"));
    ui.painter().text(Pos2::new(rect.left() + 20.0, rect.center().y), egui::Align2::LEFT_CENTER, words, TextStyle::Small.resolve(ui.style()), QUIET);
    if heading.on_hover_text(tip).clicked() {
        ui.data_mut(|d| d.insert_temp(open_id, !open));
    }
    open
}

/// A menu over the view at `at` (kept inside `free`): its entries, each
/// with its key on the right; a click on one gives its action. Whether a
/// left click fell elsewhere (it closes then).
pub fn menu(ui: &egui::Ui, at: Pos2, free: Rect, entries: Vec<(String, Action, Option<&'static str>)>, shown: &mut Shown) -> bool {
    let area = egui::Area::new(egui::Id::unique("zones menu")).order(egui::Order::Foreground).fixed_pos(at).constrain_to(free).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(200.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            for (word, action, key) in entries {
                if crate::gui::row(ui, &word, TextStyle::Body, TEXT, false, key).clicked() {
                    shown.clicked.push(action);
                }
            }
        });
    });
    shown.rects.push(area.response.rect);
    // Only a left click elsewhere closes it: the right click that opened
    // it comes up in its first frame (in the same frame as the press, for
    // a click a program makes), and a right click elsewhere opens another.
    area.response.clicked_elsewhere() && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary))
}

/// A bar of buttons at the chosen thing in the view: over the box its
/// marks take on the screen (`bounds`), from `middle` (its middle's x) to
/// the right; slid left only as far as the view's edge asks (it never
/// jumps to the other side), under the box where the top is near. `id` keeps
/// its width from the last frame (`guess` before).
#[allow(clippy::too_many_arguments)]
pub fn bar_at(ui: &egui::Ui, id: &str, middle: f32, bounds: Rect, free: Rect, guess: f32, shown: &mut Shown, add: impl FnOnce(&mut egui::Ui, &mut Shown)) -> Rect {
    let width_id = egui::Id::unique((id, "width"));
    let width: f32 = ui.data(|d| d.get_temp(width_id)).unwrap_or(guess).max(guess * 0.5);
    let x = (middle - 16.0).min(free.right() - 8.0 - width).max(free.left() + 8.0);
    let y = if bounds.top() - 38.0 >= free.top() + 8.0 { bounds.top() - 38.0 } else { (bounds.bottom() + 8.0).min(free.bottom() - 40.0) };
    let area = egui::Area::new(egui::Id::unique(id)).order(egui::Order::Foreground).fixed_pos(Pos2::new(x, y)).show(ui.ctx(), |ui| {
        egui::Frame::new().fill(Color32::from_rgba_unmultiplied(31, 29, 38, 235)).corner_radius(4.0).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
                add(ui, shown)
            });
        });
    });
    let rect = area.response.rect;
    ui.data_mut(|d| d.insert_temp(width_id, rect.width()));
    shown.rects.push(rect);
    rect
}

/// The view's own buttons in its lower right corner: turn it, further and
/// nearer; the mouse's ways to do the same on hover.
pub fn view_buttons(ui: &egui::Ui, free: Rect, shown: &mut Shown) {
    let at = free.right_bottom() + Vec2::new(-12.0, -12.0);
    let area = egui::Area::new(egui::Id::unique("zones view buttons")).order(egui::Order::Foreground).pivot(egui::Align2::RIGHT_BOTTOM).fixed_pos(at).show(ui.ctx(), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
            for (word, action, tip) in [
                ("<", Action::TurnView(-1), "Turn the view (or drag with the right mouse button)"),
                (">", Action::TurnView(1), "Turn the view (or drag with the right mouse button)"),
                ("-", Action::ZoomView(-1), "Further away (or the mouse wheel; the middle button drags the view along)"),
                ("+", Action::ZoomView(1), "Nearer (or the mouse wheel; the middle button drags the view along)"),
            ] {
                if ui.button(RichText::new(word).color(QUIET)).on_hover_text(tip).clicked() {
                    shown.clicked.push(action);
                }
            }
        });
    });
    shown.rects.push(area.response.rect);
}

/// A colour of three bytes as egui's.
pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// The words of a part's warnings, short: the first's (`EMPTY`, `LOOSE`,
/// `UNSURE`); the whole sentences are the row's tooltip.
pub fn warning_word(warnings: &[String]) -> Option<&'static str> {
    let w = warnings.first()?;
    Some(if w.starts_with("has no voxels") {
        "EMPTY"
    } else if w.starts_with("touches nothing") {
        "LOOSE"
    } else {
        "UNSURE"
    })
}
