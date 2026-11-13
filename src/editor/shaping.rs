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
    if ed.hint.is_none() || ed.disk.clash.is_some() {
        zones::from_disk(ui, free, ed, shown);
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
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // BACKGROUND, rightmost (not in GAME VIEW: the game's own sky).
        if view.before_game.is_none() {
            zones::background_button(ui, view.background(ed.scope), view.sky, shown);
        }
        // Shaped in place: the level round it.
        if let Some(c) = &ed.context {
            keyed(ui, shown, "CONTEXT", Action::Context, Some(c.shown), &format!("The level round it, faint ({}): shown or not", c.file));
        }
        match ed.scope {
            Scope::Level => {
                keyed(ui, shown, "GAME VIEW", Action::GameView, Some(view.before_game.is_some()), "Look as the game's camera does; again: back");
            }
            Scope::Frame(_) if ed.figure.is_none() && !ed.parted() && ed.frames().len() > 1 => {
                keyed(ui, shown, "PLAY", Action::Play, Some(view.playing), "Its frames one after another, as the game shows them (the outliner's picture)");
            }
            // A figure: as the game draws it; posed by its rings.
            Scope::Frame(_) | Scope::Figure if ed.figure.is_some() => {
                keyed(ui, shown, "GAME VIEW", Action::GameView, Some(view.before_game.is_some()), "As the game lights and shades it");
                keyed(ui, shown, "POSE", Action::Pose, Some(ed.rigging.posing), "Turn the parts to try a pose; nothing is saved. COPY gives the Lua table");
            }
            _ => {}
        }
    });
}

/// The bar at a figure's chosen part, over its arrows: what is done to a
/// part there (HIDE).
fn part_bar(ui: &mut egui::Ui, ed: &Editor, free: Rect, view: &View, shown: &mut Shown) {
    // A selection: its arrows and outline only; its actions in the
    // inspector (`selection_rows`).
    if ed.selection().is_some() {
        return;
    }
    let Some(p) = ed.figure.as_ref().and_then(|f| f.selected).filter(|_| ed.part_mode().is_some() && ed.joint_drag().is_none() && ed.forms.drawing.is_none() && !ed.rigging.posing) else { return };
    let screen = ui.ctx().content_rect();
    let vp = view.view_proj(&view.block, screen.width() / screen.height().max(1.0));
    let Some(h) = ed.part_handles(vp) else { return };
    let size = glam::Vec2::new(screen.width(), screen.height());
    // Clear of the arrows (MOVE) and the rings (ROTATE) alike, and of the
    // part's box: the bar stays where it is while MOVE and ROTATE switch.
    let mut bounds = Rect::NOTHING;
    for h in [ed.part_handles_as(vp, false), ed.part_handles_as(vp, true)].into_iter().flatten() {
        for s in h.shapes(vp, size) {
            for q in &s.points {
                bounds.extend_with(screen.min + Vec2::new(q.x, q.y));
            }
        }
        for c in &h.body {
            if let Some((q, _)) = super::handles::project(vp, size, *c) {
                bounds.extend_with(screen.min + Vec2::new(q.x, q.y));
            }
        }
    }
    if !bounds.is_positive() {
        return;
    }
    // (From the part's middle, its arrows' place, in either.)
    let centre = ed.part_handles_as(vp, false).map_or(h.centre, |h| h.centre);
    let middle = super::handles::project(vp, size, centre).map_or(bounds.center().x, |(q, _)| screen.left() + q.x);
    let rotating = ed.rigging.part_rotating;
    let removable = ed.cannot_remove(p);
    zones::bar_at(ui, &format!("part bar {p}"), middle, bounds, free, 400.0, shown, |ui, shown| {
        // MOVE (its arrows) or ROTATE (its rings about its joint), one lit.
        small(ui, shown, "MOVE", Action::PartRotating(false), Some(!rotating), "Arrows: move it by whole voxels (Shift held: by tenths; what hangs on it goes along)");
        small(ui, shown, "ROTATE", Action::PartRotating(true), Some(rotating), "Rings: rotate it about its joint (by 5°; Shift held: by 1°)");
        // A part turned as it stands: its arrows along its own axes or
        // the figure's.
        if ed.part_turned() && !rotating {
            ui.add_space(6.0);
            small(ui, shown, "LOCAL", Action::MoveGlobal(false), Some(!ed.rigging.move_global), "Arrows along the part's own axes, as it stands");
            small(ui, shown, "GLOBAL", Action::MoveGlobal(true), Some(ed.rigging.move_global), "Arrows along the figure's axes");
        }
        ui.add_space(6.0);
        small(ui, shown, "HIDE", Action::Hide(p), None, "Hide it from the picture (the outliner's mark shows it again)  H");
        small(ui, shown, "DUPLICATE", Action::DuplicatePart(p), None, "A copy of it where it stands, chosen to be moved away");
        let refused = ed.cannot_mirror(p);
        let tip = refused.clone().unwrap_or_else(|| "Its partner of the pair (_left, _right) made or made anew, mirrored across the figure's middle".into());
        if ui.add_enabled(refused.is_none(), egui::Button::new(RichText::new("MIRROR").small())).on_hover_text(&tip).on_disabled_hover_text(&tip).clicked() {
            shown.clicked.push(Action::MirrorPart(p));
        }
        let tip = removable.unwrap_or("Remove it with its voxels; what hangs on it hangs on its parent (UNDO brings it back)  Del");
        let word = RichText::new("REMOVE").small();
        if ui.add_enabled(removable.is_none(), egui::Button::new(word)).on_hover_text(tip).on_disabled_hover_text(tip).clicked() {
            shown.clicked.push(Action::RemovePart(p));
        }
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
                Some(figure) => parts(ui, ed, view, figure, &stem(&ed.file), shown),
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
    for (a, word) in [(Action::Frame(-1), "PREVIOUS FRAME"), (Action::Frame(1), "NEXT FRAME")] {
        if let Some(k) = key_of(a) {
            zones::key_named(shown, k, a, word);
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
fn parts(ui: &mut egui::Ui, ed: &Editor, view: &View, figure: &Figure, name: &str, shown: &mut Shown) {
    // The heading's own buttons: SHOW ALL (quiet while none is hidden), +
    // a new part (each one click, always in view).
    let hidden = figure.hidden.iter().any(|&h| h);
    let extras = [("SHOW ALL", Action::ShowAll, "Every part shown again", hidden), ("+", Action::AddPart(None), "Add a part: one voxel of the color held, on top of the part chosen, hanging on it (named to type)", true)];
    if !zones::group_with(ui, "PARTS", Some(figure.parts().len()), "The figure's parts by what they hang on: a click shapes one, again the whole figure", &extras, shown) {
        return;
    }
    // The figure whole first, the parts under it (the editor opens on it;
    // Escape or a click on empty space in the view comes back to it; a
    // stroke from it on a part keeps it chosen).
    let whole_chosen = figure.selected.is_none() || ed.rigging.painting_whole;
    let whole = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(18.0);
        list_row(ui, Sort::Piece, None, name, 0.0, whole_chosen, false, None, None, "The figure whole: every part as it is; the tools work on the part they meet (Esc)")
    });
    shown.rows.push((String::new(), whole.inner.rect));
    if whole.inner.clicked() {
        shown.clicked.push(Action::WholeFigure);
    }
    // A part's row dropped on the figure whole's: refused, said.
    if let Some(src) = whole.inner.dnd_release_payload::<PartDrag>() {
        shown.clicked.push(Action::Reparent(src.0, usize::MAX));
    }
    for &(p, depth) in &figure.tree {
        let depth = depth + 1;
        let part = &figure.parts()[p];
        let (hidden, chosen) = (figure.hidden[p], figure.selected == Some(p) && !ed.rigging.painting_whole);
        let warnings = figure.warnings(p);
        let by_hand = match (part.hung == crate::vox::Hung::ByHand, part.joint_by_hand) {
            (true, true) => "parent and joint set by hand",
            (true, false) => "parent set by hand",
            (false, true) => "joint set by hand",
            (false, false) => "parent and joint by the rules",
        };
        // (Its name as the game's code uses it: `thing.state.parts.NAME`.)
        let tip = std::iter::once(format!("{} (in code: parts.{}): {by_hand}", part.name, part.name)).chain(warnings.iter().cloned()).collect::<Vec<_>>().join("\n");
        if view.renaming_part == Some(p) {
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0 + 18.0);
                let id = egui::Id::unique(("part name", p));
                let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| part.name.clone());
                let field = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
                if !field.has_focus() && ui.data(|d| d.get_temp::<bool>(id.with("asked"))).is_none() {
                    field.request_focus();
                    ui.data_mut(|d| d.insert_temp(id.with("asked"), true));
                }
                ui.data_mut(|d| d.insert_temp(id, text.clone()));
                let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                if field.lost_focus() || enter || escape {
                    ui.data_mut(|d| {
                        d.remove::<String>(id);
                        d.remove::<bool>(id.with("asked"));
                    });
                    if escape {
                        shown.clicked.push(Action::RenamePart(usize::MAX));
                    } else {
                        shown.renamed_part = Some((p, text));
                    }
                }
            });
            if let Some(problem) = &view.rename_problem {
                ui.label(RichText::new(problem).small().color(crate::gui::ERROR));
            }
            continue;
        }
        let r = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(depth as f32 * 12.0);
                zones::hide_mark(ui, hidden, Action::Hide(p), shown);
                zones::list_row_sensed(ui, Sort::Part, None, &part.name, 0.0, chosen, hidden, None, zones::warning_word(&warnings), &tip, egui::Sense::click_and_drag())
            })
            .inner;
        shown.rows.push((part.name.clone(), r.rect));
        // Dragged onto another part's row: hung there (the row under the
        // pointer lit, a line under it); nothing else a drag does.
        r.dnd_set_drag_payload(PartDrag(p));
        if let Some(src) = r.dnd_hover_payload::<PartDrag>()
            && src.0 != p
        {
            ui.painter().rect_stroke(r.rect, 3.0, egui::Stroke::new(1.5, crate::gui::ACCENT), egui::StrokeKind::Inside);
            ui.painter().line_segment([r.rect.left_bottom(), r.rect.right_bottom()], egui::Stroke::new(2.0, crate::gui::ACCENT));
        }
        if let Some(src) = r.dnd_release_payload::<PartDrag>()
            && src.0 != p
        {
            shown.clicked.push(Action::Reparent(src.0, p));
        }
        if r.secondary_clicked() {
            let at = r.interact_pointer_pos().unwrap_or(r.rect.right_top());
            shown.clicked.push(Action::PartMenu(p, [at.x, at.y]));
        } else if r.double_clicked() {
            shown.clicked.push(Action::RenamePart(p));
        } else if r.clicked() {
            shown.clicked.push(Action::Part(p));
        }
    }
}

/// The inspector.
fn inspector(ui: &mut egui::Ui, ed: &Editor, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    if ed.rigging.posing && ed.figure.is_some() {
        pose(ui, ed, shown);
    }
    // (A stroke from the figure whole: still the figure's rows.)
    match if ed.rigging.painting_whole { Scope::Figure } else { ed.scope } {
        Scope::Level => level(ui, ed, figures, thumbs, shown),
        Scope::Block(i) => {
            ui.label(RichText::new(format!("BLOCK {}", ed.level.blocks[i].name.to_uppercase())).color(TEXT));
            row(ui, "KIND", "What it is to the game", |ui| kind(ui, ed, shown));
            row(ui, "BLOCK", "", |ui| {
                small(ui, shown, "ROTATE", Action::Turn(Turn::TURN), None, "Rotate it 90°, round the upright  T");
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
                    trim_button(ui, ed, shown);
                });
                row(ui, "PARTS", "Its parts; a click on one (here or in the view) shapes it", |ui| {
                    ui.label(format!("{}", figure.parts().len()));
                });
                // The tools as for a part: a press on a part chooses it
                // and the tool works there at once.
                shaping(ui, ed, shown);
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
                        // (Four words: a little less padding, so they keep
                        // to the inspector's room.)
                        ui.spacing_mut().button_padding = egui::vec2(4.0, 3.0);
                        small(ui, shown, "ROTATE", Action::Turn(Turn::TURN), None, "Rotate the model 90°, round the upright  T");
                        trim_button(ui, ed, shown);
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
    // Below every other row (nothing above it moves when it comes).
    if ed.selection().is_some() {
        selection_rows(ui, ed, shown);
    }
}

/// A part's row being dragged (to hang it on the row it is dropped on).
#[derive(Clone, Copy, Debug)]
struct PartDrag(usize);

/// TRIM: the box cut down to what the voxels need (the figure whole's,
/// a part's own, a model of frames'); quiet when there is nothing to cut.
fn trim_button(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let can = ed.can_trim();
    let r = ui.add_enabled(can, egui::Button::new(RichText::new("TRIM").small()));
    if r.on_hover_text("Its box cut down to its voxels  M").on_disabled_hover_text("ALREADY TIGHT").clicked() {
        shown.clicked.push(Action::Trim);
    }
}

/// SELECTION, while voxels are selected: what is done at them, on the
/// column grid (each also in the menu a right click on them opens).
fn selection_rows(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let count = ed.selection().map_or(0, |s| s.count());
    ui.separator();
    ui.label(RichText::new(format!("SELECTION {count}")).color(TEXT)).on_hover_text("Voxels selected: its arrows move them (Alt: what they land on stays); Escape: none");
    // A button a column wide (or two), on the grid.
    let cell = |ui: &mut egui::Ui, shown: &mut Shown, word: &str, colour: egui::Color32, cols: usize, action: Action, refused: Option<&str>, tip: &str| {
        let b = egui::Button::new(RichText::new(word).small().color(if refused.is_some() { QUIET } else { colour }));
        let r = ui
            .add_enabled_ui(refused.is_none(), |ui| {
                // (Its word within its column: DUPLICATE too.)
                ui.spacing_mut().button_padding.x = 2.0;
                ui.add_sized([zones::col(ui, cols), ui.text_style_height(&egui::TextStyle::Body) + 6.0], b)
            })
            .inner;
        let r = match refused {
            Some(why) => r.on_disabled_hover_text(why),
            None => r.on_hover_text(tip),
        };
        if r.clicked() {
            shown.clicked.push(action);
        }
    };
    // What one does most first (the owner's words): a part of them,
    // away, another color.
    let refused = ed.cannot_part();
    row(ui, "", "", |ui| {
        cell(ui, shown, "NEW PART", TEXT, 1, Action::SelNewPart, refused, "A part of their own, hanging on the part most came from (named to type)");
        cell(ui, shown, "REMOVE", TEXT, 1, Action::SelRemove, None, "Take them away  Del");
        cell(ui, shown, "PAINT", TEXT, 1, Action::SelPaint, None, "The color held onto them");
    });
    row(ui, "", "", |ui| {
        // (DUPLICATE is wider than a column: two.)
        cell(ui, shown, "DUPLICATE", TEXT, 2, Action::SelDuplicate, None, "A copy in place: the next move leaves these where they are");
    });
    for (word, flip) in [("FLIP", true), ("ROTATE", false)] {
        row(ui, word, if flip { "Mirrored across the selection's box" } else { "A quarter turn about its middle" }, |ui| {
            for a in 0..3 {
                let action = if flip { Action::SelFlip(a as u8) } else { Action::SelRotate(a as u8) };
                cell(ui, shown, ["X", "Y", "Z"][a], zones::AXIS[a], 1, action, None, if flip { "Mirrored along this axis" } else { "90° about this axis" });
            }
        });
    }
    if ed.figure.is_some() {
        row(ui, "", "", |ui| {
            // MOVE TO PART…: the parts to choose from.
            let parts: Vec<String> = ed.figure.as_ref().map_or_else(Vec::new, |f| f.parts().iter().map(|p| p.name.clone()).collect());
            ui.add_enabled_ui(refused.is_none(), |ui| {
                ui.set_width(zones::col(ui, 2));
                ui.menu_button(RichText::new("MOVE TO PART…").small().color(TEXT), |ui| {
                    for (q, name) in parts.iter().enumerate() {
                        if ui.button(name.to_uppercase()).clicked() {
                            shown.clicked.push(Action::SelToPart(q));
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Into another part, where they lie");
            });
        });
    }
    row(ui, "", "", |ui| {
        cell(ui, shown, "COPY", TEXT, 1, Action::SelCopy, None, "Ctrl+C");
        let none = ed.selecting.clipboard.is_none().then_some("Nothing copied yet");
        cell(ui, shown, "PASTE", TEXT, 1, Action::SelPaste, none, "Where it lay, selected, to move  Ctrl+V");
    });
}

/// What a block or a frame (a part) is shaped with: the tools, BEVEL and
/// SYMMETRY, a fresh box's DEPTH, the colours and the held one's material.
fn shaping(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    slice_rows(ui, ed, shown);
    tools(ui, ed, shown);
    // SELECT: what its rectangle does (Ctrl while dragging: ADD; Alt:
    // REMOVE), whether it reaches through, none selected.
    if ed.resting_tool() == Tool::Select {
        use super::model::selection::SelectMode;
        row(ui, "", "", |ui| {
            for (word, mode, tip) in [("NEW", SelectMode::New, "The rectangle's voxels instead"), ("ADD", SelectMode::Add, "The rectangle's voxels besides  Ctrl"), ("REMOVE", SelectMode::Remove, "The rectangle's voxels out  Alt")] {
                small(ui, shown, word, Action::SelectMode(mode), Some(ed.selecting.select_mode == mode), tip);
            }
        });
        row(ui, "", "", |ui| {
            small(ui, shown, "THROUGH", Action::SelectThrough, Some(ed.selecting.select_through), "Every voxel behind the rectangle (off: only those seen)");
            let r = ui.add_enabled(ed.selection().is_some(), egui::Button::new(RichText::new("CLEAR").small()));
            if r.on_hover_text("None selected  Esc").clicked() {
                shown.clicked.push(Action::SelClear);
            }
        });
    }
    let fresh = ed.forms.fresh.filter(|_| ed.forms.drawing.is_none());
    row(ui, "", "", |ui| {
        if ed.resting_tool() == Tool::Box || fresh.is_some() {
            small(ui, shown, "BEVEL", Action::Bevel, Some(ed.forms.bevel), "Its edges cut back  F");
        }
        small(ui, shown, "SYMMETRY", Action::Symmetry, Some(ed.symmetry), "Every tool mirrored left to right  Y");
    });
    if ed.forms() {
        let depth = fresh.map_or(ed.forms.depth, |f| {
            let a = (0..3).max_by_key(|&a| f.way[a].abs()).unwrap_or(2);
            f.size[a]
        });
        row(ui, "DEPTH", "How deep a box or sphere goes into its face (the fresh one, outlined, at once): + and -", |ui| {
            zones::number(ui, shown, "D", None, depth as f64, 0, 0.1, 1.0..=256.0, "", FIELD, |v| Action::Depth(v.round() as i32));
        });
    }
    colours(ui, ed, shown);
}

/// SLICE: X, Y, Z on the columns (lit the one cutting; again: off), and
/// while it cuts its LAYER and OFF.
fn slice_rows(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    if !ed.can_slice() {
        return;
    }
    let on = ed.slice.as_ref().map(|s| s.axis);
    row(ui, "SLICE", "Cut the model across an axis: what lies before the plane is neither drawn nor touched; the file stays as it is", |ui| {
        let width = zones::col(ui, 1);
        for (a, letter) in ["X", "Y", "Z"].into_iter().enumerate() {
            let word = RichText::new(letter).color(if on == Some(a) { zones::AXIS_LIT[a] } else { zones::AXIS[a] });
            let tip = if on == Some(a) { format!("Slicing across {letter}: click for off") } else { format!("Slice across {letter}") };
            if ui.add_sized([width, ui.text_style_height(&TextStyle::Body) + 8.0], egui::Button::new(word).selected(on == Some(a))).on_hover_text(tip).clicked() {
                shown.clicked.push(Action::Slice(Some(a as u8)));
            }
        }
    });
    if let Some(s) = &ed.slice {
        let room = ed.slice_room()[s.axis];
        row(ui, "LAYER", "Where the plane lies, in voxels from the box's low side (or drag its arrow)", |ui| {
            zones::number(ui, shown, ["X", "Y", "Z"][s.axis], Some(s.axis), s.at as f64, 0, 0.1, 0.0..=(room - 1).max(0) as f64, "Page Up, Page Down", FIELD, |v| Action::SliceAt(v.round() as i32));
            let width = zones::col(ui, 1);
            if ui.add_sized([width, ui.text_style_height(&TextStyle::Body) + 8.0], egui::Button::new("OFF")).on_hover_text("The whole model again").clicked() {
                shown.clicked.push(Action::Slice(None));
            }
        });
    }
}

/// A tool's tooltip: what it does, its key at the end (POINTER's Esc).
fn tool_tip(tool: Tool, key: Option<char>) -> String {
    let tip = match tool {
        Tool::Pointer => "Changes nothing: a click chooses a part (in a level, holds the block), the handles work  Esc",
        Tool::Add => "Set voxels (a block in the level)",
        Tool::Remove => "Take them away",
        Tool::Paint => "Recolor (in the level: replace the block)",
        Tool::Deep => "Recolor through, every voxel behind",
        Tool::Box => "Drag a footprint on a face: set where it was drawn. Alt: only where empty, Ctrl: cut",
        Tool::Sphere => "The same, round",
        Tool::Pick => "Take up a color (a block) from what is clicked; Shift held does the same",
        Tool::Move => "Drag a thing to another place",
        Tool::Select => "Drag a rectangle on the screen: the voxels of the active parts in it. Double click: the color connected",
        Tool::Fill => "The connected voxels of the color clicked take the held one. Ctrl: every voxel of that color",
    };
    zones::tip_with_key(tip, key)
}

/// The tools in a row: their words, lit the one in hand, each its key.
fn tools(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let list = Tool::of(ed.scope, ed.layer);
    if list.is_empty() {
        return;
    }
    // On the grid: from the fields' left edge, as every row.
    row(ui, "TOOL", "What the mouse does in the view", |ui| {
        ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().button_padding = egui::vec2(7.0, 3.0);
        for &tool in list {
            let tip = tool_tip(tool, key_of(Action::Tool(tool)));
            small(ui, shown, tool.label(ed.scope, ed.layer), Action::Tool(tool), Some(ed.tool == tool), &tip);
        }
        });
    });
}

/// The colours: the block's or model's own first, bigger, the first ten
/// on 1 to 9 and 0, then the others' (a model's colour no voxel has any
/// more: a right click takes it out); every colour, the held one's code;
/// its material as one row.
fn colours(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let (own, all) = ed.colours();
    row(ui, "COLOR", "The colors: a click holds one", |ui| {
    ui.vertical(|ui| {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (i, &c) in own.iter().enumerate() {
            let digit = (i < 10).then(|| char::from_digit((i as u32 + 1) % 10, 10).expect("a digit"));
            if let Some(d) = digit {
                shown.keys.push((d, Action::Colour(c)));
                if i == 0 {
                    shown.named.push(("1 … 0".into(), "COLOR".into()));
                }
            }
            let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
            let tip = digit.map_or(hex.clone(), |d| format!("{hex}  {d}"));
            let r = swatch(ui, c, 26.0, c == ed.colour, digit).on_hover_text(tip);
            swatch_menu(&r, ed, c, shown);
            if r.clicked() {
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
                let response = swatch(ui, c, 16.0, c == ed.colour, None);
                let response = response.on_hover_text(hex);
                swatch_menu(&response, ed, c, shown);
                if response.clicked() {
                    shown.clicked.push(Action::Colour(c));
                }
            }
        });
    }
    });
    });
    row(ui, "HELD", "The color held: a click for every color", |ui| {
        let mut c = ed.colour;
        // On the first column, its code on the next.
        ui.spacing_mut().interact_size.x = zones::col(ui, 1);
        if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Every color").changed() {
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
    // On the grid: GLASS's number on the first column, METAL's word on
    // the second and its number on the third; ROUGH and GLOW so below.
    for [a, b] in [[0, 1], [2, 3]] {
        let words = super::frames::WORDS;
        // (The width taken in the row: inside `add_enabled_ui` the room
        // left is the child's.)
        let field = |ui: &mut egui::Ui, shown: &mut Shown, i: usize, width: f32| {
            ui.add_enabled_ui(i != 2 || mirrors, |ui| {
                zones::number(ui, shown, "", None, (now[i] as f64 * 100.0).round() / 100.0, 2, 0.01, 0.0..=1.0, &format!("{}: {} (of the color held, every voxel of it)", words[i], tips[i]), width, |v| Action::Material(i as u8, v as f32));
            });
        };
        row(ui, words[a], tips[a], |ui| {
            let width = zones::col(ui, 1);
            field(ui, shown, a, width);
            zones::cell_word(ui, words[b], tips[b]);
            field(ui, shown, b, width);
        });
    }
}

/// A colour to click, its digit on it (a key) if it has one.
/// A color's menu (a right click on its swatch): REPLACE WITH HELD, and
/// REMOVE for a model's color no voxel has.
fn swatch_menu(r: &egui::Response, ed: &Editor, c: [u8; 3], shown: &mut Shown) {
    r.context_menu(|ui| {
        let held = c == ed.colour;
        let b = ui.add_enabled(!held, egui::Button::new("REPLACE WITH HELD"));
        if b.on_disabled_hover_text("It is the color held").clicked() {
            shown.clicked.push(Action::ReplaceColour(c));
            ui.close();
        }
        if ed.model.is_some() {
            let unused = ed.model.as_ref().is_some_and(|m| !m.uses(c));
            let b = ui.add_enabled(unused, egui::Button::new("REMOVE"));
            if b.on_disabled_hover_text("Voxels have it").clicked() {
                shown.clicked.push(Action::Forget(c));
                ui.close();
            }
        }
    });
}

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

/// POSE: the pose as Lua (`frame:model`'s pose table), COPY and RESET.
fn pose(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    let lua = ed.pose_lua();
    ui.label(RichText::new("POSE").color(TEXT));
    ui.add(egui::Label::new(RichText::new(&lua).monospace().small().color(if ed.rigging.pose.is_empty() { QUIET } else { TEXT })).wrap()).on_hover_text("frame:model's pose: the parts by name, their rotation, pitch and roll in degrees");
    row(ui, "", "", |ui| {
        if ui.add(egui::Button::new(RichText::new("COPY").small())).on_hover_text("The pose as Lua to the clipboard").clicked() {
            ui.ctx().copy_text(lua.clone());
        }
        let r = ui.add_enabled(!ed.rigging.pose.is_empty(), egui::Button::new(RichText::new("RESET").small()));
        if r.on_hover_text("No pose: every part at rest").on_disabled_hover_text("No pose: every part at rest").clicked() {
            shown.clicked.push(Action::PoseReset);
        }
    });
    ui.separator();
}

/// The chosen part: its name, PARENT (a menu of the parts, or as the rules
/// find it) and JOINT (where it turns, by halves of a voxel; or as the
/// rules find it).
fn rig(ui: &mut egui::Ui, ed: &Editor, figure: &Figure, p: usize, shown: &mut Shown) {
    let part = &figure.parts()[p];
    ui.label(RichText::new(format!("PART {}", part.name.to_uppercase())).color(TEXT));
    let by_hand = part.hung == crate::vox::Hung::ByHand;
    // From the pivot: its `at` and `offset` (a tenth shown only where it
    // has one); in POSE with the pose's offset, which typing sets.
    if let Some(at) = ed.part_place() {
        let tip = if ed.rigging.posing { "Where it stands posed, from the figure's pivot: typing sets the pose's offset (nothing saved)" } else { "Where it stands from the figure's pivot, in voxels, tenths too (those hanging on it go along). Or drag its arrows (MOVE; Shift: tenths)" };
        row(ui, "POSITION", tip, |ui| {
            for a in 0..3 {
                zones::number(ui, shown, ["X", "Y", "Z"][a], Some(a), ((at[a] * 10.0).round() / 10.0) as f64, 1, 0.1, -1000.0..=1000.0, "", FIELD, |v| Action::PartAt(a as u8, v as f32));
            }
        });
    }
    // Its own box, and TRIM when it holds more than its voxels.
    if let Some(b) = ed.shape() {
        let s = b.size;
        row(ui, "SIZE", "Its own box: voxels across, deep and up", |ui| {
            ui.label(format!("{} × {} × {}", s.x, s.y, s.z));
            trim_button(ui, ed, shown);
        });
    }
    // ROTATE: its rest rotation about its joint, as a level's piece's row
    // (Z round, X its nose up, Y rolled), in the file's degrees.
    if let Some(r) = ed.part_rotation().filter(|_| !ed.rigging.posing) {
        row(ui, "ROTATE", "Degrees about its joint (what hangs on it turns along). Or drag its rings (ROTATE).", |ui| {
            for (axis, letter, colour, tip) in [(0, "Z", 2, "Round, clockwise seen from above"), (1, "X", 0, "Its front up"), (2, "Y", 1, "Its right side down")] {
                zones::number(ui, shown, letter, Some(colour), r[axis] as f64, 1, 0.5, -360.0..=360.0, tip, FIELD, |v| Action::PartRotation(axis as u8, v as f32));
            }
        });
    }
    // POSE: its pose on top of the rest (nothing saved), as the rings.
    if let Some(r) = ed.chosen_pose().filter(|_| ed.rigging.posing && !ed.root_chosen()) {
        row(ui, "POSE", "Degrees on top of its rest, as frame:model's pose (nothing saved). Or drag its rings.", |ui| {
            for (axis, letter, colour, tip) in [(0, "Z", 2, "rotation: round, clockwise seen from above"), (1, "X", 0, "pitch: its front up"), (2, "Y", 1, "roll: its right side down")] {
                zones::number(ui, shown, letter, Some(colour), r[axis] as f64, 1, 0.5, -360.0..=360.0, tip, FIELD, |v| Action::PoseValue(axis as u8, v as f32));
            }
        });
    }
    // The root hangs on nothing and turns about no joint: neither row.
    let root = ed.root_chosen();
    if !root {
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
    // From the pivot, halves of a voxel, as the file says it.
    let j = figure.file_joint(p);
    row(ui, "JOINT", if part.joint_by_hand { "Where it turns: set by hand (or drag its arrows)" } else { "Where it turns: found by the rules (or drag its arrows)" }, |ui| {
        for a in 0..3 {
            // (A joint the rules found may lie at quarters: shown as it is.)
            zones::number(ui, shown, ["X", "Y", "Z"][a], Some(a), j[a] as f64, 3, 0.05, -1000.0..=1000.0, "", FIELD, |v| Action::JointTo(a as u8, v as f32));
        }
    });
    // (Only while there is something to give back to the rules: no empty
    // row.)
    if part.joint_by_hand {
        row(ui, "", "", |ui| {
            {
                small(ui, shown, "JOINT BY THE RULES", Action::JointWorkedOut, None, "Where the rules find it: the middle of where it meets its parent");
            }
        });
    }
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
        row(ui, "ROTATE", "How it lies when placed", |ui| {
            small(ui, shown, "ROTATE", Action::Turn(Turn::TURN), None, "90° round the upright  T");
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
pub fn menu_entries(ed: &Editor, on: MenuOn) -> Vec<zones::Entry> {
    let mut out: Vec<zones::Entry> = Vec::new();
    match on {
        MenuOn::Voxel(cell, part) => {
            out.push(("PICK COLOR".into(), Action::PickAt(cell), Some("Shift"), None));
            if matches!(ed.scope, Scope::Block(_) | Scope::Frame(_)) && ed.shape().is_some_and(|b| b.get(cell) != 0) {
                out.push(("ERASE".into(), Action::EraseAt(cell), None, None));
            }
            if let Some(p) = part.filter(|&p| ed.figure.as_ref().is_some_and(|f| f.selected != Some(p as usize))) {
                let name = ed.figure.as_ref().map_or(String::new(), |f| f.parts()[p as usize].name.to_uppercase());
                out.push((format!("SELECT {name}"), Action::Part(p as usize), None, None));
            }
        }
        MenuOn::Frame(f) => {
            out.push(("SHAPE".into(), Action::GoFrame(f), None, None));
            out.push(("COPY".into(), Action::CopyFrameOf(f), Some("C"), None));
            if ed.frames().len() > 1 {
                out.push(("REMOVE".into(), Action::DeleteFrameOf(f), Some("D"), None));
            }
        }
        MenuOn::Part(p) => {
            let Some(figure) = &ed.figure else { return out };
            let name = figure.parts()[p].name.to_uppercase();
            let whole = figure.selected == Some(p);
            out.push((if whole { "THE WHOLE FIGURE".into() } else { format!("SELECT {name}") }, if whole { Action::WholeFigure } else { Action::Part(p) }, None, None));
            out.push((if figure.hidden[p] { "SHOW".into() } else { "HIDE".into() }, Action::Hide(p), None, None));
            out.push(("HIDE OTHERS".into(), Action::HideOthers(p), None, None));
            if figure.hidden.iter().any(|&h| h) {
                out.push(("SHOW ALL".into(), Action::ShowAll, None, None));
            }
            out.push(("ADD PART".into(), Action::AddPart(Some(p)), None, None));
            out.push(("DUPLICATE".into(), Action::DuplicatePart(p), None, None));
            out.push(("MIRROR".into(), Action::MirrorPart(p), None, ed.cannot_mirror(p)));
            out.push(("RENAME".into(), Action::RenamePart(p), Some("F2"), None));
            // (Refused ones too, quiet, why on hover: every action is seen.)
            if let Some(q) = figure.parts()[p].parent {
                out.push((format!("MERGE INTO {}", figure.parts()[q].name.to_uppercase()), Action::MergeInto(p, q), None, ed.cannot_merge(p, q).map(str::to_string)));
            }
            let any = (0..figure.parts().len()).any(|q| ed.cannot_merge(p, q).is_none());
            out.push(("MERGE WITH…".into(), Action::MergeMenu(p, [0.0, 0.0]), None, (!any).then(|| "No part it can merge with as it stands".to_string())));
            out.push(("REMOVE".into(), Action::RemovePart(p), Some("Del"), ed.cannot_remove(p).map(str::to_string)));
        }
        MenuOn::Selection => {
            let part = ed.cannot_part().map(str::to_string);
            out.push(("NEW PART".into(), Action::SelNewPart, None, part.clone()));
            out.push(("REMOVE".into(), Action::SelRemove, Some("Del"), None));
            out.push(("PAINT".into(), Action::SelPaint, None, None));
            out.push(("DUPLICATE".into(), Action::SelDuplicate, None, None));
            for (a, w) in ["X", "Y", "Z"].iter().enumerate() {
                out.push((format!("FLIP {w}"), Action::SelFlip(a as u8), None, None));
            }
            for (a, w) in ["X", "Y", "Z"].iter().enumerate() {
                out.push((format!("ROTATE {w}"), Action::SelRotate(a as u8), None, None));
            }
            out.push(("MOVE TO PART…".into(), Action::SelToPartMenu([0.0, 0.0]), None, part));
            out.push(("COPY".into(), Action::SelCopy, Some("Ctrl+C"), None));
            out.push(("PASTE".into(), Action::SelPaste, Some("Ctrl+V"), ed.selecting.clipboard.is_none().then(|| "Nothing copied yet".to_string())));
        }
        MenuOn::ToPart => {
            if let Some(f) = &ed.figure {
                for (q, o) in f.parts().iter().enumerate() {
                    out.push((o.name.to_uppercase(), Action::SelToPart(q), None, None));
                }
            }
        }
        MenuOn::MergeWith(p) => {
            let Some(figure) = &ed.figure else { return out };
            for q in (0..figure.parts().len()).filter(|&q| q != p) {
                out.push((format!("INTO {}", figure.parts()[q].name.to_uppercase()), Action::MergeInto(p, q), None, ed.cannot_merge(p, q).map(str::to_string)));
            }
        }
        MenuOn::Block(i) => {
            out.push(("OPEN".into(), Action::OpenBlock(i), None, None));
            out.push(("COPY".into(), Action::CloneOf(i), Some("C"), None));
            out.push(("REMOVE".into(), Action::DeleteOf(i), Some("D"), None));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::keys;
    use crate::editor::panels::{key_action, laid_out_at};
    use std::path::Path;

    /// Every key a menu of the voxel editor names (a frame's, a voxel's,
    /// the selection's, a part's, a block's in the level) is on F1's page,
    /// and a letter the panels take does what the entry does.
    #[test]
    fn the_menus_keys_are_on_the_page() {
        let bunny = Editor::open_model(Path::new("games/bunnyfarm"), "bunny.vvx").unwrap();
        let mut selected = Editor::open_model(Path::new("games/bunnyfarm"), "bunny.vvx").unwrap();
        selected.select_box_cells(IVec3::new(2, 2, 2), IVec3::new(8, 8, 8), false);
        let logo = Editor::open_model(Path::new("games/bunnyfarm"), "logo.vvx").unwrap();
        let level = crate::editor::model::tests::folder("shaping-menus");
        let blocks = Editor::open(&level, "level.toml").unwrap();
        let cases: [(&Editor, Vec<MenuOn>); 4] = [
            (&bunny, vec![MenuOn::Frame(0), MenuOn::Voxel(IVec3::new(6, 6, 6), None)]),
            (&selected, vec![MenuOn::Selection]),
            (&logo, vec![MenuOn::Part(1)]),
            (&blocks, vec![MenuOn::Block(0)]),
        ];
        let mut named = 0;
        for (ed, menus) in cases {
            let view = View::new(ed, &Default::default());
            let (shown, _) = laid_out_at(ed, &view);
            let page = keys::overview(ed, &shown.named);
            for on in menus {
                let entries = menu_entries(ed, on);
                assert!(!entries.is_empty(), "{on:?} offers something");
                for (word, action, key, _) in entries {
                    let Some(k) = key else { continue };
                    named += 1;
                    assert!(keys::lists(&page, k), "{word} {k} on the page of {:?}", ed.scope);
                    if let Some(c) = k.chars().next().filter(|_| k.len() == 1)
                        && let Some(a) = key_action(&shown.keys, c)
                    {
                        // (The menu's entry for a frame or a block of the
                        // list, the key for the one open: the same deed.)
                        let alike = |a: Action| match a {
                            Action::CopyFrameOf(_) => Action::CopyFrame,
                            Action::DeleteFrameOf(_) => Action::DeleteFrame,
                            Action::CloneOf(_) => Action::Clone,
                            Action::DeleteOf(_) => Action::Delete,
                            a => a,
                        };
                        assert_eq!(alike(a), alike(action), "{word} {k}");
                    }
                }
            }
        }
        assert!(named >= 10, "the menus name keys: {named}");
    }

    /// The voxel editor lays out the same three zones as the level of
    /// pieces: the view's room the same in both (one way of working in
    /// both editors), and each tool's key is named in its tooltip and on
    /// F1's page.
    #[test]
    fn both_editors_leave_the_view_the_same_room() {
        let bunny = Editor::open_model(Path::new("games/bunnyfarm"), "bunny.vvx").unwrap();
        let view = View::new(&bunny, &Default::default());
        let (shown, _) = laid_out_at(&bunny, &view);
        let dir = crate::editor::big::room_game("shaping-zones", 3);
        let room = Editor::open(&dir, "levels/room.toml").unwrap();
        let (pieces, _) = laid_out_at(&room, &View::new(&room, &Default::default()));
        let (a, b) = (shown.free.unwrap(), pieces.free.unwrap());
        assert!((a.min - b.min).length() < 1.0 && (a.max - b.max).length() < 1.0, "{a:?} and {b:?}");
        for &tool in Tool::of(bunny.scope, bunny.layer) {
            if let Some(k) = key_of(Action::Tool(tool)) {
                assert!(tool_tip(tool, Some(k)).ends_with(&format!("  {k}")), "{tool:?}");
                assert!(shown.named.contains(&(k.to_string(), tool.label(bunny.scope, bunny.layer).to_string())), "{tool:?} on the page");
            }
        }
        assert!(tool_tip(Tool::Pointer, None).ends_with("  Esc"));
    }
}
