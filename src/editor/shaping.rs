//! The voxel editor (a level's block, a model and its frames, a figure of
//! parts) and the level of blocks, on the editors' one surface
//! (`zones.rs`), worked as the level's pieces are.
//!
//! - The top bar: what is open (BACK to the level from a block), + FRAME
//!   (or + BLOCK in a level), UNDO, REDO; on the right PLAY for frames,
//!   GAME VIEW in a level.
//! - The outliner: the level's blocks or kinds as small pictures, a model's
//!   frames, a figure's parts as their tree (a part that hangs oddly marked
//!   with a sign and a word, the whole of it its tooltip); a right click on
//!   a row its menu.
//! - The inspector: the tools in a row (lit: the one in hand), BEVEL and
//!   SYMMETRY, a fresh box's DEPTH; the colours as swatches, the held one's
//!   material as one row; what is chosen (a block's kind and turns, a
//!   frame, a part's PARENT and JOINT).
//! - The view: the voxels, a part's joint as a handle, a right click's menu
//!   on a voxel, the view's own buttons.

use egui::{Color32, Pos2, Rect, RichText, TextStyle, Vec2};
use glam::IVec3;

use super::figure::Figure;
use super::model::{Editor, Layer, Scope, Tool};
use super::panels::{Shown, Thumbs};
use super::view::{Action, Figures, MenuOn, View, front, key_of, seen};
use super::zones::{self, FIELD, Sort, keyed, list_row, row, small};
use crate::blocks::Turn;
use crate::grid::Kind;
use crate::gui::{ACCENT, QUIET, TEXT};

/// The panels of the voxel editor and of a level of blocks.
pub fn ui(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    // (The pictures are made in the outliner and the inspector, one after
    // the other.)
    let pics = std::cell::RefCell::new(&mut *thumbs);
    let free = zones::layout(ui, shown, |ui, shown| top_bar(ui, ed, view, shown), |ui, shown| outliner(ui, ed, view, figures, &mut pics.borrow_mut(), shown), |ui, shown| inspector(ui, ed, figures, &mut pics.borrow_mut(), shown));
    if let Some(m) = &ed.message {
        zones::mistake(ui, free, m, shown);
    } else if let Some(h) = ed.hint.as_deref() {
        // What the editor just did or would not: short, its words on hover.
        zones::say(ui, free, h, crate::gui::WARNING, shown);
    }
    zones::view_buttons(ui, free, shown);
    part_bar(ui, ed, free, view, shown);
    if let Some(m) = view.menu {
        let entries = menu_entries(ed, m.on);
        if !entries.is_empty() && zones::menu(ui, Pos2::new(m.at[0], m.at[1]), free, entries, shown) {
            shown.clicked.push(Action::CloseMenu);
        }
    }
    if view.choosing {
        tiles(ui, ed, free, thumbs, shown);
    }
}

/// The name of what is open: the file's stem.
fn stem(file: &str) -> String {
    std::path::Path::new(file).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned())
}

/// The top bar.
fn top_bar(ui: &mut egui::Ui, ed: &Editor, view: &View, shown: &mut Shown) {
    match ed.scope {
        Scope::Level => {
            ui.label(RichText::new(stem(&ed.file).to_uppercase()).color(TEXT)).on_hover_text(&ed.file);
            ui.add_space(14.0);
            let things = ed.layer == Layer::Things;
            keyed(ui, shown, "BLOCKS", Action::Layer(Layer::Blocks), Some(!things), "The level's blocks");
            keyed(ui, shown, "THINGS", Action::Layer(Layer::Things), Some(things), "Things of the game's kinds, on the blocks");
            keyed(ui, shown, "PIECES", Action::Layer(Layer::Pieces), Some(false), "Models, lights, places and areas set down freely");
            ui.add_space(14.0);
            if !things {
                keyed(ui, shown, "+ BLOCK", Action::Choose, Some(view.choosing), "A new block: from a tile of the tileset, or empty");
            }
        }
        Scope::Block(i) => {
            keyed(ui, shown, "BACK", Action::Back, None, "Back to the level");
            ui.label(RichText::new(format!("BLOCK {}", ed.level.blocks[i].name.to_uppercase())).color(TEXT)).on_hover_text(format!("blocks/{}.vvx", ed.level.blocks[i].name));
            ui.add_space(14.0);
        }
        Scope::Frame(_) | Scope::Figure => {
            ui.label(RichText::new(stem(&ed.file).to_uppercase()).color(TEXT)).on_hover_text(&ed.file);
            ui.add_space(14.0);
            if ed.figure.is_none() && !ed.parted() {
                keyed(ui, shown, "+ FRAME", Action::CopyFrame, None, "One more frame: a copy of this one, after it");
            }
        }
    }
    ui.add_space(8.0);
    keyed(ui, shown, "UNDO", Action::Undo, None, "A step back");
    keyed(ui, shown, "REDO", Action::Redo, None, "The step taken back again");
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match ed.scope {
        Scope::Level => {
            keyed(ui, shown, "GAME VIEW", Action::GameView, Some(view.before_game.is_some()), "Look as the game's camera does; again: back");
        }
        Scope::Frame(_) if ed.figure.is_none() && !ed.parted() && ed.frames().len() > 1 => {
            keyed(ui, shown, "PLAY", Action::Play, Some(view.playing), "Its frames one after another, as the game shows them (the outliner's picture)");
        }
        _ => {}
    });
}

/// The bar at a figure's chosen part, over its arrows: what is done to a
/// part there (HIDE).
fn part_bar(ui: &mut egui::Ui, ed: &Editor, free: Rect, view: &View, shown: &mut Shown) {
    let Some(p) = ed.figure.as_ref().and_then(|f| f.selected).filter(|_| ed.part_mode().is_some() && ed.joint_drag().is_none() && ed.drawing.is_none()) else { return };
    let screen = ui.ctx().content_rect();
    let vp = view.view_proj(&view.block, screen.width() / screen.height().max(1.0));
    let Some(h) = ed.part_handles(vp) else { return };
    let size = glam::Vec2::new(screen.width(), screen.height());
    let mut bounds = Rect::NOTHING;
    for s in h.shapes(vp, size) {
        for q in &s.points {
            bounds.extend_with(screen.min + Vec2::new(q.x, q.y));
        }
    }
    // The part's box too: the bar goes above or below it, never over it.
    for c in &h.body {
        if let Some((q, _)) = super::handles::project(vp, size, *c) {
            bounds.extend_with(screen.min + Vec2::new(q.x, q.y));
        }
    }
    if !bounds.is_positive() {
        return;
    }
    let middle = super::handles::project(vp, size, h.centre).map_or(bounds.center().x, |(q, _)| screen.left() + q.x);
    zones::bar_at(ui, &format!("part bar {p}"), middle, bounds, free, 70.0, shown, |ui, shown| {
        small(ui, shown, "HIDE", Action::Hide(p), None, "Hide it from the picture (the outliner's mark shows it again)");
    });
}

/// A small picture of `size` points at most, its pixels whole.
fn picture(ui: &mut egui::Ui, (id, px): (egui::TextureId, egui::Vec2), size: f32) {
    let scale = (size / px.x.max(px.y).max(1.0)).floor().max(1.0);
    ui.image((id, px * scale));
}

/// The outliner.
fn outliner(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    // Playing: the frames one after another, over the list.
    if view.playing && ed.figure.is_none() && !ed.parted() && matches!(ed.scope, Scope::Frame(_)) && ed.frames().len() > 1 {
        let frames = ed.frames();
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(1.0 / super::view::FRAMES_PER_SECOND));
        let at = (ui.input(|i| i.time) * super::view::FRAMES_PER_SECOND) as usize % frames.len();
        let frame = &frames[at];
        let pic = thumbs.get(ui.ctx(), format!("frame {at}"), (&frame.voxels, &frame.palette, frame.size.to_array(), view.face as u8), || seen(frame, view.face));
        ui.vertical_centered(|ui| picture(ui, pic, 120.0));
        ui.add_space(6.0);
    }
    egui::ScrollArea::vertical().id_salt("shaping outliner").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        match ed.scope {
            Scope::Level if ed.layer == Layer::Things => kinds(ui, ed, figures, thumbs, shown),
            Scope::Level | Scope::Block(_) => blocks(ui, ed, thumbs, shown),
            Scope::Frame(_) | Scope::Figure => match &ed.figure {
                Some(figure) => parts(ui, figure, shown),
                None => frames(ui, ed, view, thumbs, shown),
            },
        }
    });
}

/// The level's blocks, each its picture and name: chosen, the one held
/// (or open); a click holds one, a second (or a double click) opens it.
fn blocks(ui: &mut egui::Ui, ed: &Editor, thumbs: &mut Thumbs, shown: &mut Shown) {
    let listed = ed.level.blocks.len();
    if !zones::group(ui, "BLOCKS", Some(listed), "The level's blocks: a click holds one, again opens it") {
        return;
    }
    for i in 0..listed {
        let block = &ed.blocks[i];
        let entry = &ed.level.blocks[i];
        let pic = thumbs.get(ui.ctx(), format!("block {i}"), (&block.voxels, &block.palette), || (block.size.x, block.size.z, front(block)));
        let chosen = match ed.scope {
            Scope::Block(b) => b == i,
            _ => ed.held.block as usize == i + 1,
        };
        let r = list_row(ui, Sort::Block, Some(pic), &entry.name, 0.0, chosen, false, None, None, &format!("{} ({})", entry.name, entry.kind.name()));
        if r.secondary_clicked() {
            let at = r.interact_pointer_pos().unwrap_or(r.rect.right_top());
            shown.clicked.push(Action::BlockMenu(i, [at.x, at.y]));
        } else if r.double_clicked() || (r.clicked() && matches!(ed.scope, Scope::Block(_))) {
            shown.clicked.push(Action::OpenBlock(i));
        } else if r.clicked() {
            shown.clicked.push(Action::Hold(i));
        }
    }
}

/// The game's kinds of things, each its picture: chosen, the one held.
fn kinds(ui: &mut egui::Ui, ed: &Editor, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    if !zones::group(ui, "KINDS", Some(ed.kinds.len()), "The game's kinds of things (c.things): the one held is put down") {
        return;
    }
    for (i, kind) in ed.kinds.iter().enumerate() {
        let pic = thumbs.get(ui.ctx(), format!("kind {kind}"), kind, || figures.picture(kind, 64));
        if list_row(ui, Sort::Kind, Some(pic), kind, 0.0, ed.held_kind.as_ref() == Some(kind), false, None, None, kind).clicked() {
            shown.clicked.push(Action::HoldKind(i));
        }
    }
}

/// A model's frames (a figure that cannot be rigged: its parts), each its
/// picture; the one shaped chosen.
fn frames(ui: &mut egui::Ui, ed: &Editor, view: &View, thumbs: &mut Thumbs, shown: &mut Shown) {
    let parted = ed.parted();
    let frames = ed.frames();
    let now = match ed.scope {
        Scope::Frame(f) => f,
        _ => 0,
    };
    let title = if parted { "PARTS" } else { "FRAMES" };
    if !zones::group(ui, title, Some(frames.len()), "Its frames: a click shapes one (V and X: the one before, the next)") {
        return;
    }
    for a in [Action::Frame(-1), Action::Frame(1)] {
        if let Some(k) = key_of(a) {
            shown.keys.push((k, a));
        }
    }
    for (f, frame) in frames.iter().enumerate() {
        let pic = thumbs.get(ui.ctx(), format!("frame {f}"), (&frame.voxels, &frame.palette, frame.size.to_array(), view.face as u8), || seen(frame, view.face));
        let r = list_row(ui, Sort::Frame, Some(pic), &format!("{}", f + 1), 0.0, f == now, false, None, None, &format!("{} {} of {}", if parted { "part" } else { "frame" }, f + 1, frames.len()));
        if r.secondary_clicked() && !parted {
            let at = r.interact_pointer_pos().unwrap_or(r.rect.right_top());
            shown.clicked.push(Action::FrameMenu(f, [at.x, at.y]));
        } else if r.clicked() {
            shown.clicked.push(Action::GoFrame(f));
        }
    }
}

/// A figure's parts by what they hang on, the root first: each its name
/// (quiet while hidden), a sign and a word where it hangs oddly (the whole
/// of it on hover), a mark at the left that hides it from the picture.
fn parts(ui: &mut egui::Ui, figure: &Figure, shown: &mut Shown) {
    if !zones::group(ui, "PARTS", Some(figure.parts().len()), "The figure's parts by what they hang on: a click shapes one, again the whole figure") {
        return;
    }
    for &(p, depth) in &figure.tree {
        let part = &figure.parts()[p];
        let (hidden, chosen) = (figure.hidden[p], figure.selected == Some(p));
        let warnings = figure.warnings(p);
        let by_hand = match (part.hung == crate::vox::Hung::ByHand, part.joint_by_hand) {
            (true, true) => "parent and joint set by hand",
            (true, false) => "parent set by hand",
            (false, true) => "joint set by hand",
            (false, false) => "parent and joint by the rules",
        };
        let tip = std::iter::once(format!("{}: {by_hand}", part.name)).chain(warnings.iter().cloned()).collect::<Vec<_>>().join("\n");
        let r = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(depth as f32 * 12.0);
                // Shown: a filled square; hidden: an empty one.
                let (rect, mark) = ui.allocate_exact_size(egui::vec2(18.0, ui.text_style_height(&TextStyle::Body) + 8.0), egui::Sense::click());
                let square = Rect::from_center_size(rect.center(), egui::vec2(9.0, 9.0));
                let colour = if mark.hovered() { TEXT } else { QUIET };
                if hidden {
                    ui.painter().rect_stroke(square, 1.0, egui::Stroke::new(1.5, colour), egui::StrokeKind::Inside);
                } else {
                    ui.painter().rect_filled(square, 1.0, colour);
                }
                if mark.on_hover_text(if hidden { "Show it" } else { "Hide it (from the picture only)" }).clicked() {
                    shown.clicked.push(Action::Hide(p));
                }
                list_row(ui, Sort::Part, None, &part.name, 0.0, chosen, hidden, None, zones::warning_word(&warnings), &tip)
            })
            .inner;
        if r.secondary_clicked() {
            let at = r.interact_pointer_pos().unwrap_or(r.rect.right_top());
            shown.clicked.push(Action::PartMenu(p, [at.x, at.y]));
        } else if r.clicked() {
            shown.clicked.push(Action::Part(p));
        }
    }
}

/// The inspector.
fn inspector(ui: &mut egui::Ui, ed: &Editor, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    match ed.scope {
        Scope::Level => level(ui, ed, figures, thumbs, shown),
        Scope::Block(i) => {
            ui.label(RichText::new(format!("BLOCK {}", ed.level.blocks[i].name.to_uppercase())).color(TEXT));
            row(ui, "KIND", "What it is to the game", |ui| kind(ui, ed, shown));
            row(ui, "BLOCK", "", |ui| {
                small(ui, shown, "TURN", Action::Turn(Turn::TURN), None, "Turn it a quarter, round the upright  T");
                small(ui, shown, "COPY", Action::Clone, None, "A copy of it in the level's list  C");
                small(ui, shown, "REMOVE", Action::Delete, None, "Take it out of the level (UNDO brings it back)  D");
            });
            shaping(ui, ed, shown);
        }
        Scope::Figure => {
            if let Some(figure) = &ed.figure {
                let s = figure.model.size;
                ui.label(RichText::new(format!("FIGURE {}", stem(&ed.file).to_uppercase())).color(TEXT));
                row(ui, "SIZE", "Voxels across, deep and up", |ui| {
                    ui.label(format!("{} × {} × {}", s.x, s.y, s.z));
                });
                if figure.hidden.iter().any(|&h| h) {
                    row(ui, "", "", |ui| {
                        small(ui, shown, "SHOW ALL", Action::ShowAll, None, "Every part shown again");
                    });
                }
            }
        }
        Scope::Frame(f) => {
            match (&ed.figure, ed.figure.as_ref().and_then(|fig| fig.selected)) {
                (Some(figure), Some(p)) => rig(ui, ed, figure, p, shown),
                _ => {
                    let frames = ed.frames().len();
                    let parted = ed.parted();
                    let s = ed.grid();
                    ui.label(RichText::new(format!("{} {} OF {frames}", if parted { "PART" } else { "FRAME" }, f + 1)).color(TEXT));
                    row(ui, "SIZE", "Voxels across, deep and up", |ui| {
                        ui.label(format!("{} × {} × {}", s.x, s.y, s.z));
                    });
                    row(ui, "MODEL", "", |ui| {
                        small(ui, shown, "TURN", Action::Turn(Turn::TURN), None, "Turn the model a quarter, round the upright  T");
                        small(ui, shown, "TRIM", Action::Trim, None, "Its box cut down to its voxels  M");
                        if !parted {
                            small(ui, shown, "COPY", Action::CopyFrame, None, "One more frame: a copy of this one, after it  C");
                            small(ui, shown, "REMOVE", Action::DeleteFrame, Some(false), "This frame taken out (UNDO brings it back)  D");
                        }
                    });
                }
            }
            shaping(ui, ed, shown);
        }
    }
}

/// What a block or a frame (a part) is shaped with: the tools, BEVEL and
/// SYMMETRY, a fresh box's DEPTH, the colours and the held one's material.
fn shaping(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    tools(ui, ed, shown);
    let fresh = ed.fresh.filter(|_| ed.drawing.is_none());
    row(ui, "", "", |ui| {
        if ed.resting_tool() == Tool::Box || fresh.is_some() {
            small(ui, shown, "BEVEL", Action::Bevel, Some(ed.bevel), "Its edges cut back  F");
        }
        small(ui, shown, "SYMMETRY", Action::Symmetry, Some(ed.symmetry), "Every tool mirrored left to right  Y");
    });
    if ed.forms() {
        let depth = fresh.map_or(ed.depth, |f| {
            let a = (0..3).max_by_key(|&a| f.way[a].abs()).unwrap_or(2);
            f.size[a]
        });
        row(ui, "DEPTH", "How deep a box or sphere goes into its face (the fresh one, outlined, at once): + and -", |ui| {
            zones::number(ui, shown, "D", None, depth as f64, 0, 0.1, 1.0..=256.0, "", FIELD, |v| Action::Depth(v.round() as i32));
        });
    }
    colours(ui, ed, shown);
}

/// The tools in a row: their words, lit the one in hand, each its key.
fn tools(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let list = Tool::of(ed.scope, ed.layer);
    if list.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().button_padding = egui::vec2(7.0, 3.0);
        for &tool in list {
            let key = key_of(Action::Tool(tool));
            let tip = match tool {
                Tool::Add => "Set voxels (a block in the level)",
                Tool::Remove => "Take them away",
                Tool::Paint => "Recolour (in the level: replace the block)",
                Tool::Deep => "Recolour through, every voxel behind",
                Tool::Box => "Drag a footprint on a face: set where it was drawn. Alt: only where empty, Ctrl: cut",
                Tool::Sphere => "The same, round",
                Tool::Pick => "Take up a colour (a block) from what is clicked; Shift held does the same",
                Tool::Move => "Drag a thing to another place",
            };
            let tip = key.map_or(tip.to_string(), |k| format!("{tip}  {k}"));
            small(ui, shown, tool.label(ed.scope), Action::Tool(tool), Some(ed.tool == tool), &tip);
        }
    });
}

/// The colours: the block's or model's own first, bigger, the first ten
/// on 1 to 9 and 0, then the others' (a model's colour no voxel has any
/// more: a right click takes it out); every colour, the held one's code;
/// its material as one row.
fn colours(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let (own, all) = ed.colours();
    ui.label(RichText::new("COLOUR").small().color(QUIET));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (i, &c) in own.iter().enumerate() {
            let digit = (i < 10).then(|| char::from_digit((i as u32 + 1) % 10, 10).expect("a digit"));
            if let Some(d) = digit {
                shown.keys.push((d, Action::Colour(c)));
            }
            let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
            let tip = digit.map_or(hex.clone(), |d| format!("{hex}  {d}"));
            if swatch(ui, c, 26.0, c == ed.colour, digit).on_hover_text(tip).clicked() {
                shown.clicked.push(Action::Colour(c));
            }
        }
    });
    let others: Vec<[u8; 3]> = all.iter().copied().filter(|c| !own.contains(c)).collect();
    if !others.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
            for c in others {
                let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
                let unused = ed.model.as_ref().is_some_and(|m| !m.uses(c));
                let response = swatch(ui, c, 16.0, c == ed.colour, None);
                let response = if unused { response.on_hover_text(format!("{hex}: no voxel has it; a right click takes it out")) } else { response.on_hover_text(hex) };
                if response.secondary_clicked() && unused {
                    shown.clicked.push(Action::Forget(c));
                } else if response.clicked() {
                    shown.clicked.push(Action::Colour(c));
                }
            }
        });
    }
    row(ui, "HELD", "The colour held: a click for every colour", |ui| {
        let mut c = ed.colour;
        if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Every colour").changed() {
            shown.clicked.push(Action::Colour(c));
        }
        ui.label(RichText::new(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).color(QUIET));
    });
    material(ui, ed, shown);
}

/// The held colour's material in a model, as one row: GLASS, METAL, ROUGH
/// (of metal or glass), GLOW, each 0 to 1.
fn material(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let Some(m) = ed.material() else { return };
    let now = super::frames::words(m);
    let mirrors = m.metal != 0 || m.rgba[3] < 255;
    let tips = ["See-through", "A mirror of the sky", "How dull a mirror is (of metal or glass)", "How much it glows"];
    for pair in [[0, 1], [2, 3]] {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().button_padding = egui::vec2(4.0, 3.0);
            for i in pair {
                let word = super::frames::WORDS[i];
                ui.add_enabled_ui(i != 2 || mirrors, |ui| {
                    zones::number(ui, shown, word, None, (now[i] as f64 * 100.0).round() / 100.0, 2, 0.01, 0.0..=1.0, &format!("{}: {} (of the colour held, every voxel of it)", word, tips[i]), 134.0, |v| Action::Material(i as u8, v as f32));
                });
            }
        });
    }
}

/// A colour to click, its digit on it (a key) if it has one.
fn swatch(ui: &mut egui::Ui, c: [u8; 3], size: f32, chosen: bool, digit: Option<char>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
    // (A dark colour on the dark panel still shows its square.)
    ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(132, 126, 118, 90)), egui::StrokeKind::Inside);
    if chosen || response.hovered() {
        ui.painter().rect_stroke(rect.expand(2.0), 4.0, egui::Stroke::new(2.0, if chosen { ACCENT } else { QUIET }), egui::StrokeKind::Outside);
    }
    if let Some(d) = digit {
        let light = c[0] as u32 * 3 + c[1] as u32 * 6 + c[2] as u32 > 1300;
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, d, TextStyle::Small.resolve(ui.style()), if light { Color32::from_rgb(30, 28, 36) } else { TEXT });
    }
    response
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

/// The chosen part: its name, PARENT (a menu of the parts, or as the rules
/// find it) and JOINT (where it turns, by halves of a voxel; or as the
/// rules find it).
fn rig(ui: &mut egui::Ui, ed: &Editor, figure: &Figure, p: usize, shown: &mut Shown) {
    let part = &figure.parts()[p];
    ui.label(RichText::new(format!("PART {}", part.name.to_uppercase())).color(TEXT));
    let by_hand = part.hung == crate::vox::Hung::ByHand;
    if let Some(at) = ed.part_at() {
        row(ui, "POSITION", "Where it stands from the figure's pivot, in voxels (those hanging on it go along). Or drag its arrows (MOVE).", |ui| {
            for a in 0..3 {
                zones::number(ui, shown, ["X", "Y", "Z"][a], Some(a), at[a] as f64, 0, 0.1, -1000.0..=1000.0, "", FIELD, |v| Action::PartAt(a as u8, v.round() as i32));
            }
        });
    }
    row(ui, "PARENT", if by_hand { "What it hangs on: set by hand" } else { "What it hangs on: found by the rules" }, |ui| {
        let now = part.parent.map_or("none".to_string(), |q| figure.parts()[q].name.clone());
        ui.menu_button(RichText::new(format!("{now} ▾")).color(if by_hand { ACCENT } else { TEXT }), |ui| {
            for (q, other) in figure.parts().iter().enumerate().filter(|&(q, _)| q != p) {
                let refused = ed.ring_with(p, q);
                let r = ui.add_enabled(refused.is_none(), egui::Button::new(&other.name).selected(part.parent == Some(q)));
                let r = match &refused {
                    Some(why) => r.on_disabled_hover_text(why),
                    None => r,
                };
                if r.clicked() {
                    shown.clicked.push(Action::ParentTo(Some(q as u16)));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("BY THE RULES").on_hover_text("What the rules find: the part it touches most").clicked() {
                shown.clicked.push(Action::ParentWorkedOut);
                ui.close();
            }
        });
    });
    let j = figure.joint(p);
    row(ui, "JOINT", if part.joint_by_hand { "Where it turns: set by hand (or drag its arrows)" } else { "Where it turns: found by the rules (or drag its arrows)" }, |ui| {
        for a in 0..3 {
            zones::number(ui, shown, ["X", "Y", "Z"][a], Some(a), j[a] as f64, 1, 0.05, -1000.0..=1000.0, "", FIELD, |v| Action::JointTo(a as u8, v as f32));
        }
    });
    if part.joint_by_hand || by_hand {
        row(ui, "", "", |ui| {
            if part.joint_by_hand {
                small(ui, shown, "JOINT BY THE RULES", Action::JointWorkedOut, None, "Where the rules find it: the middle of where it meets its parent");
            }
        });
    }
}

/// A level of blocks: the block held (its picture, kind, turns, OPEN, COPY,
/// REMOVE) or the kind held, and the tools.
fn level(ui: &mut egui::Ui, ed: &Editor, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    if ed.layer == Layer::Things {
        if let Some(kind) = &ed.held_kind {
            ui.label(RichText::new(format!("HELD {}", kind.to_uppercase())).color(TEXT));
            let pic = thumbs.get(ui.ctx(), format!("kind {kind}"), kind, || figures.picture(kind, 64));
            picture(ui, pic, 72.0);
        }
        tools(ui, ed, shown);
        return;
    }
    if let Some(i) = ed.block_index() {
        let block = &ed.blocks[i];
        let turn = ed.held.turn;
        ui.label(RichText::new(format!("HELD {}", ed.level.blocks[i].name.to_uppercase())).color(TEXT));
        let pic = thumbs.get(ui.ctx(), "held".into(), (&block.voxels, &block.palette, turn), || {
            let turned = block.turned(turn);
            (turned.size.x, turned.size.z, front(&turned))
        });
        ui.horizontal(|ui| {
            picture(ui, pic, 72.0);
            ui.vertical(|ui| {
                kind(ui, ed, shown);
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
                    small(ui, shown, "OPEN", Action::OpenBlock(i), None, "Shape it voxel by voxel (or double click it)");
                });
            });
        });
        row(ui, "TURN", "How it lies when placed", |ui| {
            small(ui, shown, "TURN", Action::Turn(Turn::TURN), None, "A quarter round the upright  T");
            small(ui, shown, "ROLL", Action::Turn(Turn::ROLL), None, "A quarter over its side  O");
            small(ui, shown, "MIRROR", Action::Turn(Turn::MIRROR), None, "Mirrored left to right  M");
        });
        row(ui, "BLOCK", "", |ui| {
            small(ui, shown, "COPY", Action::Clone, None, "A copy of it in the level's list  C");
            small(ui, shown, "REMOVE", Action::Delete, None, "Take it out of the level (UNDO brings it back)  D");
        });
    }
    tools(ui, ed, shown);
}

/// What a menu of the voxel editor offers (each also a control somewhere:
/// the inspector, the outliner, the tools).
pub fn menu_entries(ed: &Editor, on: MenuOn) -> Vec<(String, Action, Option<&'static str>)> {
    let mut out: Vec<(String, Action, Option<&'static str>)> = Vec::new();
    match on {
        MenuOn::Voxel(cell, part) => {
            out.push(("PICK COLOUR".into(), Action::PickAt(cell), Some("Shift")));
            if matches!(ed.scope, Scope::Block(_) | Scope::Frame(_)) && ed.shape().is_some_and(|b| b.get(cell) != 0) {
                out.push(("ERASE".into(), Action::EraseAt(cell), None));
            }
            if let Some(p) = part.filter(|&p| ed.figure.as_ref().is_some_and(|f| f.selected != Some(p as usize))) {
                let name = ed.figure.as_ref().map_or(String::new(), |f| f.parts()[p as usize].name.to_uppercase());
                out.push((format!("SHAPE {name}"), Action::Part(p as usize), None));
            }
        }
        MenuOn::Frame(f) => {
            out.push(("SHAPE".into(), Action::GoFrame(f), None));
            out.push(("COPY".into(), Action::CopyFrameOf(f), Some("C")));
            if ed.frames().len() > 1 {
                out.push(("REMOVE".into(), Action::DeleteFrameOf(f), Some("D")));
            }
        }
        MenuOn::Part(p) => {
            let Some(figure) = &ed.figure else { return out };
            let name = figure.parts()[p].name.to_uppercase();
            out.push((if figure.selected == Some(p) { "THE WHOLE FIGURE".into() } else { format!("SHAPE {name}") }, Action::Part(p), None));
            out.push((if figure.hidden[p] { "SHOW".into() } else { "HIDE".into() }, Action::Hide(p), None));
            if figure.hidden.iter().any(|&h| h) {
                out.push(("SHOW ALL".into(), Action::ShowAll, None));
            }
        }
        MenuOn::Block(i) => {
            out.push(("OPEN".into(), Action::OpenBlock(i), None));
            out.push(("COPY".into(), Action::CloneOf(i), Some("C")));
            out.push(("REMOVE".into(), Action::DeleteOf(i), Some("D")));
        }
        MenuOn::Item(_) | MenuOn::Floor(_) | MenuOn::Add => {}
    }
    out
}

/// The tileset's tiles (+ BLOCK) as pictures over the view, and EMPTY; a
/// click makes a block of one.
fn tiles(ui: &mut egui::Ui, ed: &Editor, free: Rect, thumbs: &mut Thumbs, shown: &mut Shown) {
    let width = (free.width() - 80.0).clamp(300.0, 760.0);
    let at = Pos2::new(free.center().x - width / 2.0, free.top() + 24.0);
    let area = egui::Area::new(egui::Id::unique("shaping tiles")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).inner_margin(egui::Margin { left: 12, right: 0, top: 12, bottom: 12 }).show(ui, |ui| {
            zones::flush(ui);
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(RichText::new("+ BLOCK").color(TEXT));
                small(ui, shown, "EMPTY", Action::Empty, None, "An empty block to shape");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.0);
                    if ui.button("×").on_hover_text("Close  Esc").clicked() {
                        shown.clicked.push(Action::Choose);
                    }
                });
            });
            let Some(sheet) = &ed.sheet else { return };
            let tile = 52.0;
            let per_row = ((width + 6.0) / (tile + 6.0)).floor().max(1.0) as usize;
            let ids: Vec<i32> = (1..=sheet.count).collect();
            egui::ScrollArea::vertical().id_salt("tiles").max_height((free.height() - 140.0).max(160.0)).show(ui, |ui| {
                for chunk in ids.chunks(per_row) {
                    ui.horizontal(|ui| {
                        for &id in chunk {
                            let Some(pixels) = sheet.tile(id) else { continue };
                            let pic = thumbs.get(ui.ctx(), format!("tile {id}"), id, || (sheet.n, sheet.n, pixels.iter().map(|p| (p[3] >= 128).then_some([p[0], p[1], p[2]])).collect()));
                            let (rect, r) = ui.allocate_exact_size(Vec2::splat(tile), egui::Sense::click());
                            if r.hovered() {
                                ui.painter().rect_filled(rect, 3.0, crate::gui::ROW_HOVERED);
                            }
                            let scale = ((tile - 8.0) / pic.1.x.max(pic.1.y).max(1.0)).floor().max(1.0);
                            ui.painter().image(pic.0, Rect::from_center_size(rect.center(), pic.1 * scale), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            if r.on_hover_text(format!("tile {id}")).clicked() {
                                shown.clicked.push(Action::Tile(id));
                            }
                        }
                    });
                }
            });
        });
    });
    shown.rects.push(area.response.rect);
    let _ = IVec3::ZERO;
}
