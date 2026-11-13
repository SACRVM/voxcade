//! The level editor among its pieces, laid out as the 3D tools people know:
//! three zones, each with one job.
//!
//! - A bar along the top: what is open, ADD, UNDO and REDO; on the right
//!   how the view looks (GAME VIEW, LIGHTS) and the level's own settings
//!   (LEVEL).
//! - On the left the outliner: what is set down, by name, in three groups
//!   that fold. One scroll for all of it.
//! - On the right the inspector: the chosen one only (or the level), as a
//!   grid of short rows: its name to type over, where it stands, how it is
//!   turned, its lights. Fits without scrolling in the usual cases.
//!
//! The view between them holds what is done every session: the thing is
//! chosen, dragged, lifted and turned by its handles (`handles.rs`), and a
//! row of buttons beside it copies, removes, opens its model and switches
//! its light; a right click anywhere offers the same and what can be added
//! there. Nothing is said in sentences: labels, values and controls; what
//! a control does is its tooltip, with its key.

use egui::{Color32, Pos2, Rect, RichText, TextStyle, Vec2};
use glam::IVec3;

use super::background::Marks;
use super::handles::{self, Grip, What};
use super::model::room::{self, Field, Item};
use super::model::{Editor, Layer};
use super::panels::{Shown, Thumbs};
use super::view::{Action, Figures, MenuOn, View, key_of};
use super::zones::{self, FIELD, LABEL, Sort, keyed, list_row, rgb, row, small};
use crate::grid::Way;
use crate::gui::{ACCENT, ERROR, QUIET, TEXT, WARNING};

/// The width of a light's × at the end of its row.
const REMOVE_WIDTH: f32 = 24.0;

/// More rows than this in the outliner: a field to find one by its name.
const FILTER_FROM: usize = 15;

/// The pieces' panels: the top bar, the outliner, the inspector, and over
/// the view the buttons at the chosen one, a menu if one is open and ADD
/// MODEL's pictures. Where the view's room is (`shown.free`).
pub fn ui(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, thumbs: &mut Thumbs, shown: &mut Shown) {
    let free = zones::layout(ui, shown, |ui, shown| top_bar(ui, ed, view, shown), |ui, shown| outliner(ui, ed, view, shown), |ui, shown| inspector(ui, ed, view, figures, shown));
    if let Some(m) = &ed.message {
        zones::mistake(ui, free, m, shown);
    }
    zones::from_disk(ui, free, ed, shown);
    beside(ui, ed, view, free, shown);
    zones::view_buttons(ui, free, shown);
    menu(ui, ed, view, free, shown);
    if view.picker {
        picker(ui, ed, free, thumbs, shown);
    }
}

/// The top bar: the level open (and in a level of blocks the three layers),
/// ADD, UNDO, REDO; on the right GAME VIEW, LIGHTS, LEVEL.
fn top_bar(ui: &mut egui::Ui, ed: &Editor, view: &View, shown: &mut Shown) {
    {
        let name = std::path::Path::new(&ed.file).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
        ui.label(RichText::new(name.to_uppercase()).color(TEXT)).on_hover_text(&ed.file);
        ui.add_space(14.0);
        if ed.level.has_grid() {
            keyed(ui, shown, "BLOCKS", Action::Layer(Layer::Blocks), Some(false), "The level's blocks");
            keyed(ui, shown, "THINGS", Action::Layer(Layer::Things), Some(false), "Things of the game's kinds, on the blocks");
            keyed(ui, shown, "PIECES", Action::Layer(Layer::Pieces), Some(true), "Models, lights, places and areas set down freely");
            ui.add_space(14.0);
        }
        let add = keyed(ui, shown, "+ ADD", Action::AddMenu, Some(view.menu.is_some_and(|m| m.on == MenuOn::Add)), "Add a model, a light, a place or an area");
        // The menu hangs under it (`menu`).
        ui.data_mut(|d| d.insert_temp(egui::Id::unique("stage add at"), add.rect.left_bottom()));
        // Its entries' keys work without it open.
        for a in [Action::AddModel, Action::AddLight, Action::AddPlace] {
            if let Some(k) = key_of(a) {
                shown.keys.push((k, a));
            }
        }
        ui.add_space(8.0);
        keyed(ui, shown, "UNDO", Action::Undo, None, "A step back: any change, a removal too");
        keyed(ui, shown, "REDO", Action::Redo, None, "The step taken back again");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // BACKGROUND, rightmost (not in GAME VIEW: the game's own sky).
            if view.before_game.is_none() {
                zones::background_button(ui, view.background(ed.scope), view.sky, shown);
            }
            keyed(ui, shown, "LEVEL", Action::LevelSettings, Some(view.level_settings || ed.chosen.is_none()), "The level's size and daylight");
            let lights: usize = ed.level.things.iter().map(|t| t.lights.len()).sum();
            let many = lights > crate::render::MAX_LIGHTS;
            let tip = if many { format!("The pieces' lights in this view. {lights} lights: the game shows the {} nearest the middle of its view", crate::render::MAX_LIGHTS) } else { "The pieces' lights in this view".to_string() };
            let mut job = egui::text::LayoutJob::default();
            job.append("LIGHTS", 0.0, egui::TextFormat { font_id: TextStyle::Button.resolve(ui.style()), color: if many { WARNING } else if view.lights { ACCENT } else { TEXT }, ..Default::default() });
            if ui.add(egui::Button::new(job).selected(view.lights)).on_hover_text(tip).clicked() {
                shown.clicked.push(Action::Lights);
            }
            keyed(ui, shown, "GAME VIEW", Action::GameView, Some(view.before_game.is_some()), "Look as the game's camera does; again: back");
        });
    }
}

fn sort_of(ed: &Editor, item: Item) -> Sort {
    match item {
        Item::Thing(i) => {
            let t = &ed.level.things[i];
            match (t.is_piece(), t.model.is_some()) {
                (true, true) => Sort::Piece,
                (true, false) => Sort::Light,
                _ => Sort::Kind,
            }
        }
        Item::Place(_) => Sort::Place,
        Item::Area(_) => Sort::Area,
    }
}

/// `#rrggbb` as three bytes (white if it is none).
pub fn bytes_of(hex: Option<&str>) -> [u8; 3] {
    let h = hex.and_then(|h| h.strip_prefix('#')).filter(|h| h.len() == 6);
    h.map_or([255; 3], |h| [0, 2, 4].map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(255)))
}

/// What an item is, for a tooltip: its model's file, a light, its kind, a
/// place, an area; and how the game's code finds it by its name.
fn what_of(ed: &Editor, item: Item) -> String {
    let (label, named) = ed.label_of(item);
    let what = match item {
        Item::Thing(i) => {
            let t = &ed.level.things[i];
            match (&t.model, t.is_piece()) {
                (Some(m), _) => m.clone(),
                (None, true) => "a light".into(),
                (None, false) => format!("a {}", t.kind),
            }
        }
        Item::Place(_) => "a place a player arrives at".into(),
        Item::Area(_) => "an area".into(),
    };
    let code = match (item, named) {
        (_, false) => "no name yet".to_string(),
        (Item::Thing(_), true) => format!("level:thing(\"{label}\")"),
        (Item::Place(_), true) => format!("world:go_to(level, \"{label}\")"),
        (Item::Area(_), true) => format!("level.areas.{label}"),
    };
    format!("{label}\n{what}\n{code}")
}

/// One row of the outliner: the item's sign, its name, its light's colour
/// at the end; chosen, marked.
fn outline_row(ui: &mut egui::Ui, ed: &Editor, item: Item, chosen: bool, shown: &mut Shown) -> egui::Response {
    let (label, named) = ed.label_of(item);
    let light = match item {
        Item::Thing(i) => ed.level.things[i].lights.first().map(|l| rgb(bytes_of(l.color.as_deref()))),
        _ => None,
    };
    let hidden = ed.is_hidden(item);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // The hide mark, as the figure's parts have it.
        zones::hide_mark(ui, hidden, Action::HideItem(item), shown);
        list_row(ui, sort_of(ed, item), None, &label, 0.0, chosen, !named || hidden, light, None, &what_of(ed, item))
    })
    .inner
}

/// The outliner: the three groups with their counts, each folding at its
/// heading; a field to find by name when there are many; one scroll.
fn outliner(ui: &mut egui::Ui, ed: &Editor, view: &View, shown: &mut Shown) {
    let filter_id = egui::Id::unique("stage filter");
    let rows = ed.level.things.len() + ed.level.places.len() + ed.level.areas.len();
    let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
    if rows > FILTER_FROM {
        zones::beside_bar(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut filter).hint_text("find").desired_width(f32::INFINITY)).on_hover_text("Only the names with these letters");
        });
        ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        ui.add_space(4.0);
    } else {
        filter.clear();
    }
    // Scrolled to the chosen one when it changed (by a click in the view).
    let seen_id = egui::Id::unique("stage outliner chosen");
    let last: Option<Option<Item>> = ui.data(|d| d.get_temp(seen_id));
    let scroll = last != Some(ed.chosen);
    ui.data_mut(|d| d.insert_temp(seen_id, ed.chosen));
    let groups: [(&str, Vec<Item>); 3] = [
        ("THINGS", (0..ed.level.things.len()).map(Item::Thing).collect()),
        ("PLACES", (0..ed.level.places.len()).map(Item::Place).collect()),
        ("AREAS", (0..ed.level.areas.len()).map(Item::Area).collect()),
    ];
    let tips = ["Models, lights and things set down", "Where a player arrives (world:go_to)", "Boxes the game's code asks about (level.areas)"];
    let wanted = filter.trim().to_lowercase();
    egui::ScrollArea::vertical().id_salt("outliner").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for ((title, items), tip) in groups.into_iter().zip(tips) {
            // SHOW ALL at the heading, always, quiet while nothing of the
            // group is hidden: one click, no row moving.
            let extras = [("SHOW ALL", Action::ShowAllItems, "Everything shown again", items.iter().any(|&i| ed.is_hidden(i)))];
            if !zones::group_with(ui, title, Some(items.len()), tip, &extras, shown) {
                continue;
            }
            for item in items {
                if !wanted.is_empty() && !ed.label_of(item).0.to_lowercase().contains(&wanted) {
                    continue;
                }
                let chosen = ed.chosen == Some(item);
                let response = outline_row(ui, ed, item, chosen, shown);
                if response.secondary_clicked() {
                    let at = response.interact_pointer_pos().unwrap_or(response.rect.right_top());
                    shown.clicked.push(Action::OpenMenu(item, [at.x, at.y]));
                } else if response.double_clicked() && matches!(sort_of(ed, item), Sort::Piece) {
                    shown.clicked.push(Action::EditModel);
                } else if response.clicked() {
                    shown.clicked.push(Action::ChooseItem(Some(item)));
                }
                if chosen && scroll {
                    response.scroll_to_me(Some(egui::Align::Center));
                }
            }
            ui.add_space(6.0);
        }
    });
    let _ = view;
}

/// A number field of the level (`zones::number`): its value set as `set`
/// (degrees to a tenth, voxels and hundredths whole).
#[allow(clippy::too_many_arguments)]
fn field(ui: &mut egui::Ui, shown: &mut Shown, letter: &str, axis: Option<usize>, value: f64, set: Field, speed: f64, range: std::ops::RangeInclusive<f64>, tip: &str, width: f32) {
    let decimals = if matches!(set, Field::Turn | Field::Pitch | Field::Roll | Field::SunFrom | Field::SunHeight) { 1 } else { 0 };
    zones::number(ui, shown, letter, axis, value, decimals, speed, range, tip, width, |v| Action::Set(set, v));
}

/// The inspector: the chosen one (or the level, with LEVEL or nothing
/// chosen).
fn inspector(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, shown: &mut Shown) {
    match ed.chosen.filter(|_| !view.level_settings) {
        Some(item) => chosen(ui, ed, view, figures, item, shown),
        None => level(ui, ed, shown),
    }
}

/// The head: the name to type over (Enter keeps it, Escape leaves it; a
/// name refused says why under it), and quiet under it what it is, a
/// model's file with EDIT.
fn head(ui: &mut egui::Ui, ed: &Editor, view: &View, item: Item, shown: &mut Shown) {
    let id = egui::Id::unique(("stage name", format!("{item:?}")));
    let (label, named) = ed.label_of(item);
    let now = if named { label.clone() } else { String::new() };
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| now.clone());
    let edit = egui::TextEdit::singleline(&mut text).font(TextStyle::Body).hint_text(RichText::new(&label).color(QUIET)).desired_width(f32::INFINITY);
    let response = ui.add(edit).on_hover_text(what_of(ed, item));
    if view.renaming == Some(item) && !response.has_focus() && ui.data(|d| d.get_temp::<bool>(id.with("asked"))).is_none() {
        response.request_focus();
        ui.data_mut(|d| d.insert_temp(id.with("asked"), true));
    }
    // Enter and Escape end the typing: the keys are the view's again.
    if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape)) {
        response.surrender_focus();
    }
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    } else if response.lost_focus() {
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if !escape && text.trim() != now {
            shown.renamed = Some((item, text.clone()));
        }
        ui.data_mut(|d| {
            d.remove::<String>(id);
            d.remove::<bool>(id.with("asked"));
        });
        if escape || text.trim() == now {
            shown.clicked.push(Action::Rename(None));
        }
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    if let Some(problem) = &view.rename_problem {
        ui.label(RichText::new(problem).small().color(ERROR));
    }
    let thing = match item {
        Item::Thing(i) => Some(&ed.level.things[i]),
        _ => None,
    };
    // A piece's model file; a thing of a kind its kind (EDIT: its kind's
    // model, if it has one).
    let shown_as = thing.and_then(|t| match &t.model {
        Some(m) => Some((m.clone(), m.clone())),
        None if !t.is_piece() => ed.kind_models.get(&t.kind).map(|m| (t.kind.clone(), format!("{} (c.things.{}): its model {m}", t.kind, t.kind))),
        None => None,
    });
    match shown_as {
        Some((word, tip)) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                let room = ui.available_width() - 60.0;
                let mut job = egui::text::LayoutJob::simple_singleline(word, TextStyle::Small.resolve(ui.style()), QUIET);
                job.wrap = egui::text::TextWrapping::truncate_at_width(room);
                ui.label(job).on_hover_text(tip);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    keyed(ui, shown, "EDIT", Action::EditModel, None, "Open its model in the voxel editor (or double click it)");
                });
            });
        }
        None => {
            let what = match (item, thing) {
                (Item::Thing(_), Some(t)) if t.is_piece() => "light".to_string(),
                (Item::Thing(_), Some(t)) => t.kind.clone(),
                (Item::Place(_), _) => "place".into(),
                _ => "area".into(),
            };
            ui.label(RichText::new(what).small().color(QUIET));
        }
    }
}

/// POSITION: three fields, X, Y, Z (an area's low corner).
fn position(ui: &mut egui::Ui, ed: &Editor, item: Item, shown: &mut Shown) {
    let at = ed.at_of(item).unwrap_or_default();
    let v = ed.level.voxels();
    row(ui, "POSITION", "Where it stands, in voxels: X across, Y in depth, Z up. Or drag its arrows.", |ui| {
        for a in 0..2 {
            field(ui, shown, ["X", "Y"][a], Some(a), at[a] as f64, Field::At(a), 0.25, 0.0..=(v[a] - 1).max(0) as f64, "", FIELD);
        }
        // Z: how high its lowest voxel stands (0 on the floor), laid down
        // as standing.
        let base = ed.base_of(item).unwrap_or(at[2]);
        field(ui, shown, "Z", Some(2), base as f64, Field::Base, 0.25, 0.0..=(v[2] - 1).max(0) as f64, "How high it stands: its lowest voxel over the floor", FIELD);
    });
}

/// The chosen one's rows.
fn chosen(ui: &mut egui::Ui, ed: &Editor, view: &View, figures: &Figures, item: Item, shown: &mut Shown) {
    head(ui, ed, view, item, shown);
    ui.add_space(2.0);
    position(ui, ed, item, shown);
    match item {
        Item::Thing(i) => {
            let t = &ed.level.things[i];
            if t.model.is_some() {
                piece_rows(ui, ed, figures, t, shown);
            } else if ed.kind_turns(t) {
                // A thing of a kind without a behavior: round only.
                let free = ed.free_kinds.contains(&t.kind);
                let tip = if free { "Round, clockwise seen from above: 0 away from the camera, 180 towards it" } else { "Round, clockwise seen from above, in quarter turns (its kind turns on the grid)" };
                row(ui, "ROTATE", "Degrees. Or drag its ring.", |ui| {
                    field(ui, shown, "Z", Some(2), room::bearing_of(t).rem_euclid(360.0), Field::Turn, if free { 0.5 } else { 2.0 }, -360.0..=720.0, tip, FIELD);
                });
            } else if t.is_piece() {
                // A light alone: its colour and radius.
                if let Some(l) = t.lights.first() {
                    row(ui, "LIGHT", "Its color and how far it shines (its radius)", |ui| light_controls(ui, l, 0, zones::FIELD2, shown));
                }
            }
        }
        Item::Place(i) => {
            let p = &ed.level.places[i];
            row(ui, "FACING", "The way a player arriving here looks", |ui| {
                ui.spacing_mut().button_padding = egui::vec2(9.0, 3.0);
                for (way, letter, tip) in [(Way::North, "N", "north: away from the camera"), (Way::East, "E", "east: right"), (Way::South, "S", "south: towards the camera"), (Way::West, "W", "west: left")] {
                    let on = p.facing.unwrap_or(Way::South.pair()) == way.pair();
                    if ui.add(egui::Button::new(letter).selected(on)).on_hover_text(tip).clicked() {
                        shown.clicked.push(Action::FacePlace(way));
                    }
                }
            });
        }
        Item::Area(i) => {
            let size = ed.level.areas[i].size;
            row(ui, "SIZE", "How big it is, in voxels. Or drag the squares on its faces.", |ui| {
                for a in 0..3 {
                    field(ui, shown, ["X", "Y", "Z"][a], Some(a), size[a] as f64, Field::Size(a), 0.25, 1.0..=4096.0, "", FIELD);
                }
            });
        }
    }
}

/// A light's colour (the picker on a click), its radius.
fn light_controls(ui: &mut egui::Ui, l: &crate::grid::ThingLight, k: usize, radius: f32, shown: &mut Shown) {
    let mut c = bytes_of(l.color.as_deref());
    // The colour on the first column, RADIUS on the other two.
    ui.spacing_mut().interact_size.x = zones::col(ui, 1);
    if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Its color").changed() {
        shown.clicked.push(Action::LightColour(k as u8, c));
    }
    field(ui, shown, "RADIUS", None, l.radius, Field::Radius(k), 1.0, 1.0..=2000.0, "How far it shines, in voxels (radius in the level's file)", radius);
}

/// A piece's rows: TURN, SOLID (and LIGHT + without one), its lights.
fn piece_rows(ui: &mut egui::Ui, ed: &Editor, figures: &Figures, t: &crate::grid::Thing, shown: &mut Shown) {
    // On the grid (or solid) the turns step by quarters.
    let quarters = !ed.free_turns || t.solid;
    let speed = if quarters { 2.0 } else { 0.5 };
    let tip = |what: &str| if quarters { format!("{what}, in quarter turns (this game sets things on the grid{})", if t.solid && ed.free_turns { "; a solid piece too" } else { "" }) } else { what.to_string() };
    row(ui, "ROTATE", "Degrees. Or drag its rings.", |ui| {
        field(ui, shown, "Z", Some(2), room::bearing_of(t).rem_euclid(360.0), Field::Turn, speed, -360.0..=720.0, &tip("Round, clockwise seen from above: 0 away from the camera, 180 towards it"), FIELD);
        field(ui, shown, "X", Some(0), t.pitch, Field::Pitch, speed, -360.0..=720.0, &tip("Tipped: its front up (90 lays it on its back)"), FIELD);
        field(ui, shown, "Y", Some(1), t.roll, Field::Roll, speed, -360.0..=720.0, &tip("Rolled: its right side down (90 lays it on its right side)"), FIELD);
    });
    let top = t.model.as_deref().and_then(|m| figures.model(m)).map_or(10, |m| m.size.z);
    row(ui, "", "", |ui| {
        let off = t.off_grid() && !t.solid;
        let mut job = egui::text::LayoutJob::default();
        job.append("SOLID", 0.0, egui::TextFormat { font_id: TextStyle::Button.resolve(ui.style()), color: if t.solid { ACCENT } else { TEXT }, ..Default::default() });
        let tip = if off { "Only a piece in quarter turns can be solid: rotate it to 0, 90, 180 or 270 first" } else if t.solid { "Bodies stand on it and are stopped by it, exactly where it is drawn. Click: they pass through" } else { "Click: bodies stand on it and are stopped by it (a floor, a wall, a table)" };
        let response = ui.add_enabled(!off, egui::Button::new(job).selected(t.solid)).on_hover_text(tip).on_disabled_hover_text(tip);
        if response.clicked() {
            shown.clicked.push(Action::Solid(!t.solid));
        }
        if t.lights.is_empty() && ui.button("+ LIGHT").on_hover_text("It gives light, from its top at first").clicked() {
            shown.clicked.push(Action::AddPieceLight(top));
        }
    });
    if t.lights.is_empty() {
        return;
    }
    // LIGHTS, + at its end; a light a row; the chosen one's place in the
    // model under it.
    ui.horizontal(|ui| {
        ui.label(RichText::new("LIGHTS").small().color(QUIET));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 1.0);
            if ui.small_button("+").on_hover_text("One more light (a flame and its wider glow)").clicked() {
                shown.clicked.push(Action::AddPieceLight(top));
            }
        });
    });
    for (k, l) in t.lights.iter().enumerate() {
        let chosen = ed.chosen_light == Some(k);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
            let (rect, response) = ui.allocate_exact_size(Vec2::new(LABEL, ui.text_style_height(&TextStyle::Body) + 8.0), egui::Sense::click());
            if chosen || response.hovered() {
                ui.painter().rect_filled(rect, 3.0, if chosen { crate::gui::ROW_OPEN } else { crate::gui::ROW_HOVERED });
            }
            ui.painter().text(rect.left_center() + Vec2::new(4.0, 0.0), egui::Align2::LEFT_CENTER, format!("{}", k + 1), TextStyle::Body.resolve(ui.style()), if chosen { ACCENT } else { TEXT });
            if response.on_hover_text("Choose it: its handles move it in the model").clicked() {
                shown.clicked.push(Action::ChooseLight(if chosen { None } else { Some(k as u8) }));
            }
            // RADIUS and × on the last two columns.
            let radius = zones::col(ui, 2) - REMOVE_WIDTH - 4.0;
            light_controls(ui, l, k, radius, shown);
            if ui.add_sized([REMOVE_WIDTH, ui.text_style_height(&TextStyle::Body) + 8.0], egui::Button::new("×")).on_hover_text("Remove this light (UNDO brings it back)").clicked() {
                shown.clicked.push(Action::RemoveLight(k as u8));
            }
        });
        if chosen {
            let at = l.at.unwrap_or_default();
            row(ui, "IN MODEL", "Where it shines from in the model's own voxels, from where the piece stands; it turns with the piece", |ui| {
                for a in 0..3 {
                    field(ui, shown, ["X", "Y", "Z"][a], Some(a), at[a] as f64, Field::LightAt(k, a), 0.25, -1000.0..=1000.0, "", FIELD);
                }
            });
        }
    }
}

/// LEVEL: its size, its daylight (as the game's, or its own: the sun,
/// its colour and the sky's, the light everywhere and the shadows).
fn level(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    ui.label(RichText::new("LEVEL").color(TEXT)).on_hover_text(&ed.file);
    let s = ed.level.size;
    if ed.level.has_grid() {
        row(ui, "SIZE", "Its size in blocks (BLOCKS changes it)", |ui| {
            ui.label(format!("{} × {} × {}", s.x, s.y, s.z));
        });
    } else {
        row(ui, "SIZE", "How big the room is, in voxels: across, in depth, up", |ui| {
            for a in 0..3 {
                field(ui, shown, ["X", "Y", "Z"][a], Some(a), s[a] as f64, Field::LevelSize(a), 1.0, 1.0..=4096.0, "", FIELD);
            }
        });
    }
    let own = ed.level.daylight.as_ref();
    row(ui, "DAYLIGHT", "The light of this level", |ui| {
        if ui.add(egui::Button::new("AS THE GAME").selected(own.is_none())).on_hover_text("The game's daylight (c.light in conf.lua)").clicked() && own.is_some() {
            shown.clicked.push(Action::DayAsGame);
        }
        if ui.add(egui::Button::new("OWN").selected(own.is_some())).on_hover_text("A daylight of its own, starting from the game's").clicked() && own.is_none() {
            shown.clicked.push(Action::DayOwn);
        }
    });
    let Some(day) = own else { return };
    let g = &ed.game_light;
    let (from, height) = room::sun_stands(day.sun.unwrap_or(g.sun.map(|v| v as f64)));
    row(ui, "SUN", "Where the sun stands", |ui| {
        field(ui, shown, "ROUND", None, from, Field::SunFrom, 1.0, -360.0..=720.0, "Degrees round, clockwise seen from above: 0 away from the camera", 100.0);
        field(ui, shown, "HIGH", None, height, Field::SunHeight, 0.5, 1.0..=90.0, "Degrees over the horizon: 90 straight above", 100.0);
    });
    let hex = |c: [f32; 3]| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let sun_now = day.sun_color.as_deref().map_or(hex(g.sun_color), |c| bytes_of(Some(c)));
    let sky_now = day.sky.as_deref().map_or(hex(g.sky), |c| bytes_of(Some(c)));
    row(ui, "COLORS", "The sunlight's color and the sky's", |ui| {
        for (word, sun, now) in [("SUN", true, sun_now), ("SKY", false, sky_now)] {
            ui.label(RichText::new(word).small().color(QUIET));
            let mut c = now;
            if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text(if sun { "The sunlight's color" } else { "The sky's color" }).changed() {
                shown.clicked.push(Action::DayColour(sun, c));
            }
            ui.add_space(8.0);
        }
    });
    let percent = |v: Option<f64>, game: f32| (v.unwrap_or(game as f64) * 100.0).round();
    row(ui, "LIGHT", "", |ui| {
        field(ui, shown, "AMBIENT", None, percent(day.ambient, g.ambient), Field::Ambient, 0.5, 0.0..=100.0, "The light every face gets, 0 to 100", 100.0);
        field(ui, shown, "SHADOW", None, percent(day.shadow, g.shadow), Field::Shadow, 0.5, 0.0..=100.0, "How dark shadows are, 0 to 100", 100.0);
    });
}

/// Where the handles lie on the screen (egui's points): the box round
/// them, and the matrix and size they were made for.
pub fn handles_on_screen(ed: &Editor, view: &View, screen: Rect) -> Option<(handles::Handles, glam::Mat4, Rect)> {
    let vp = view.view_proj(&view.level, screen.width() / screen.height().max(1.0));
    let h = ed.handles(vp, view.turn_mode(ed))?;
    let size = glam::Vec2::new(screen.width(), screen.height());
    let shapes = h.shapes(vp, size);
    let mut bounds = Rect::NOTHING;
    for s in &shapes {
        for p in &s.points {
            bounds.extend_with(screen.min + Vec2::new(p.x, p.y));
        }
    }
    Some((h, vp, bounds))
}

/// The handles on the chosen one, over the view (on `painter`, the room
/// `screen` the window is): the rings first, quieter behind, then the
/// faces, then the arrows; the one under the mouse or dragged lit.
pub fn paint_handles(painter: &egui::Painter, ed: &Editor, view: &View, screen: Rect) {
    let Some((h, vp, _)) = handles_on_screen(ed, view, screen) else { return };
    let lit = ed.gripped().map(|g| g.drag.grip).or(view.grip_hover);
    paint(painter, &h, vp, screen, lit);
}

/// Handles over the view (`painter`, the window's room `screen`), the one
/// `lit` lit: the rings first, quieter behind, the faces, then the arrows.
pub fn paint(painter: &egui::Painter, h: &handles::Handles, vp: glam::Mat4, screen: Rect, lit: Option<Grip>) {
    let size = glam::Vec2::new(screen.width(), screen.height());
    let at = |p: glam::Vec2| screen.min + Vec2::new(p.x, p.y);
    for s in h.shapes(vp, size) {
        let a = s.grip.axis();
        let on = lit == Some(s.grip) && !s.dim;
        // Seen end on: quiet, it does not grab.
        // (Deeper on a light BACKGROUND: `background::Marks`.)
        let marks = super::zones::marks_of(painter.ctx());
        let (plain, lit_c) = (Marks::colour(marks.axis(a, false)), Marks::colour(marks.axis(a, true)));
        let colour = if on { lit_c } else if s.dim { plain.gamma_multiply(0.3) } else { plain };
        match s.what {
            What::Ring => {
                for (k, w) in s.points.windows(2).enumerate() {
                    let behind = s.behind.get(k).copied().unwrap_or(false);
                    let c = if behind { colour.gamma_multiply(0.35) } else { colour };
                    painter.line_segment([at(w[0]), at(w[1])], egui::Stroke::new(if on { 3.5 } else { 2.0 }, c));
                }
            }
            What::Shaft => {
                painter.line_segment([at(s.points[0]), at(s.points[1])], egui::Stroke::new(if on { 5.0 } else { 3.0 }, colour));
            }
            What::Head | What::Face => {
                let points: Vec<Pos2> = s.points.iter().map(|p| at(*p)).collect();
                painter.add(egui::Shape::convex_polygon(points, colour, egui::Stroke::new(1.0, Color32::from_rgb(24, 23, 30))));
            }
        }
    }
}

/// The buttons at the chosen one, over its handles (`zones::bar_at`):
/// MOVE or TURN, and what is done to it most, a word each.
fn beside(ui: &mut egui::Ui, ed: &Editor, view: &View, free: Rect, shown: &mut Shown) {
    let Some(item) = ed.chosen.filter(|_| ed.hanging.is_none() && ed.gripped().is_none() && ed.dragging().is_none() && !view.picker) else { return };
    let screen = ui.ctx().content_rect();
    let Some((h, vp, _)) = handles_on_screen(ed, view, screen) else { return };
    // Clear of the arrows (MOVE) and the rings (ROTATE) alike: the bar
    // stays where it is while they switch.
    let size = glam::Vec2::new(screen.width(), screen.height());
    let mut bounds = Rect::NOTHING;
    for both in [ed.handles(vp, false), ed.handles(vp, true)].into_iter().flatten() {
        for s in both.shapes(vp, size) {
            for p in &s.points {
                bounds.extend_with(screen.min + Vec2::new(p.x, p.y));
            }
        }
    }
    if !bounds.is_positive() {
        return;
    }
    let piece = matches!(sort_of(ed, item), Sort::Piece);
    let lights = match item {
        Item::Thing(i) => !ed.level.things[i].lights.is_empty(),
        _ => false,
    };
    let turns = ed.can_turn();
    // A thing of a kind with a model: EDIT opens that.
    let kind_model = matches!(item, Item::Thing(i) if !ed.level.things[i].is_piece() && ed.kind_models.contains_key(&ed.level.things[i].kind));
    let middle = handles::project(vp, glam::Vec2::new(screen.width(), screen.height()), h.centre).map_or(bounds.center().x, |(p, _)| screen.left() + p.x);
    let turning = view.turn_mode(ed);
    let guess = if piece { 345.0 } else if turns { 234.0 } else { 122.0 };
    let rect = zones::bar_at(ui, &format!("stage beside {item:?} {turns}"), middle, bounds, free, guess, shown, |ui, shown| {
        // MOVE (its arrows) or TURN (its rings), one lit.
        if turns {
            zones::small(ui, shown, "MOVE", Action::HandleMode(false), Some(!turning), "Arrows: move it along X, Y, Z (or drag it over the floor)");
            zones::small(ui, shown, "ROTATE", Action::HandleMode(true), Some(turning), "Rings: rotate it about its sides and the upright");
            // Turned off the level's axes: its arrows along its own or the
            // level's.
            if !turning && ed.piece_turned_off_axes() {
                ui.add_space(6.0);
                zones::small(ui, shown, "LOCAL", Action::MoveGlobal(false), Some(!ed.move_global), "Arrows along its own axes, as it stands (it lands on whole voxels)");
                zones::small(ui, shown, "GLOBAL", Action::MoveGlobal(true), Some(ed.move_global), "Arrows along the level's axes");
            }
            ui.add_space(6.0);
        }
        small(ui, shown, "COPY", Action::CopyItem, None, "Copy it: the copy hangs on the mouse, a click sets it down  C");
        small(ui, shown, "REMOVE", Action::RemoveItem, None, "Remove it (UNDO brings it back)  Del");
        if kind_model {
            small(ui, shown, "EDIT", Action::EditModel, None, "Open its kind's model in the voxel editor  E");
        }
        if piece {
            small(ui, shown, "EDIT", Action::EditModel, None, "Open its model in the voxel editor  E (or double click it)");
            let tip = if lights { "Its light: on. Click: off (again: on as it was)" } else { "Its light: off. Click: on" };
            small(ui, shown, "LIGHT", Action::SwitchLight, Some(lights), tip);
        }
    });
    ui.data_mut(|d| d.insert_temp(egui::Id::unique("stage beside rect"), rect));
}

/// What a menu offers: on a thing what is done to it (a piece of a model
/// more than the others), on empty floor what can be added there, ADD's
/// the four things to add; each entry with its key, if it has one. Every
/// one of them is also a button somewhere (the top bar, the buttons at
/// the chosen one, the inspector): the menu only saves the way.
pub fn menu_entries(ed: &Editor, on: MenuOn) -> Vec<(String, Action, Option<&'static str>)> {
    let mut entries: Vec<(String, Action, Option<&'static str>)> = Vec::new();
    match on {
        MenuOn::Add => {
            entries.extend([("MODEL…".into(), Action::AddModel, Some("A")), ("LIGHT".into(), Action::AddLight, Some("L")), ("PLACE".into(), Action::AddPlace, Some("P")), ("AREA".into(), Action::AddArea, None)]);
        }
        MenuOn::Floor(spot) => {
            entries.extend([("ADD MODEL…".into(), Action::AddModelAt(spot), None), ("ADD LIGHT".into(), Action::AddAt(0, spot), None), ("ADD PLACE".into(), Action::AddAt(1, spot), None), ("ADD AREA".into(), Action::AddAt(2, spot), None)]);
        }
        MenuOn::Item(item) => {
            if !ed.exists(item) {
                return entries;
            }
            entries.extend([("COPY".into(), Action::CopyItem, Some("C")), ("REMOVE".into(), Action::RemoveItem, Some("Del")), ("RENAME".into(), Action::Rename(Some(item)), Some("F2"))]);
            entries.push((if ed.is_hidden(item) { "SHOW".into() } else { "HIDE".into() }, Action::HideItem(item), None));
            entries.push(("HIDE OTHERS".into(), Action::OnlyItem(item), None));
            if ed.any_hidden() {
                entries.push(("SHOW ALL".into(), Action::ShowAllItems, None));
            }
            match item {
                Item::Thing(i) if ed.level.things[i].model.is_some() => {
                    let t = &ed.level.things[i];
                    entries.push(("EDIT MODEL".into(), Action::EditModel, Some("E")));
                    entries.push((if t.lights.is_empty() { "LIGHT ON" } else { "LIGHT OFF" }.into(), Action::SwitchLight, None));
                    entries.push(("ROTATE 90°".into(), Action::TurnItem(1), Some("T")));
                    entries.push(("LAY DOWN".into(), Action::Tip(0, 1), None));
                    if !t.off_grid() || t.solid {
                        entries.push((if t.solid { "NOT SOLID" } else { "SOLID" }.into(), Action::Solid(!t.solid), None));
                    }
                }
                Item::Place(_) => entries.push(("ROTATE 90°".into(), Action::TurnItem(1), Some("T"))),
                Item::Thing(i) if !ed.level.things[i].is_piece() => {
                    let t = &ed.level.things[i];
                    if ed.kind_models.contains_key(&t.kind) {
                        entries.push(("EDIT MODEL".into(), Action::EditModel, Some("E")));
                    }
                    if ed.kind_turns(t) {
                        entries.push(("ROTATE 90°".into(), Action::TurnItem(1), Some("T")));
                    }
                }
                _ => {}
            }
        }
        // (The voxel editor's: `shaping::menu_entries`.)
        MenuOn::Voxel(..) | MenuOn::Frame(_) | MenuOn::Part(_) | MenuOn::Block(_) | MenuOn::MergeWith(_) | MenuOn::Selection | MenuOn::ToPart => {}
    }
    entries
}

/// The open menu: a thing's (a right click on it, in the view or the
/// outliner), empty floor's (what can be added there) or ADD's; each entry
/// its key on the right.
fn menu(ui: &mut egui::Ui, ed: &Editor, view: &View, free: Rect, shown: &mut Shown) {
    let Some(m) = view.menu else { return };
    let at = match m.on {
        MenuOn::Add => ui.data(|d| d.get_temp(egui::Id::unique("stage add at"))).unwrap_or(free.min),
        _ => Pos2::new(m.at[0], m.at[1]),
    };
    let entries = menu_entries(ed, m.on);
    if entries.is_empty() {
        return;
    }
    let entries = entries.into_iter().map(|(w, a, k)| (w, a, k, None)).collect();
    if zones::menu(ui, at, free, entries, shown) && !matches!(m.on, MenuOn::Add) {
        shown.clicked.push(Action::CloseMenu);
    }
}

/// ADD MODEL: the game's models as pictures with their names, over the
/// view; a field to find one by name when there are many; a click hangs
/// one on the mouse (or sets it down at the spot a right click chose).
fn picker(ui: &mut egui::Ui, ed: &Editor, free: Rect, thumbs: &mut Thumbs, shown: &mut Shown) {
    let choices = ed.model_choices();
    let width = (free.width() - 80.0).clamp(300.0, 760.0);
    let at = Pos2::new(free.center().x - width / 2.0, free.top() + 24.0);
    let filter_id = egui::Id::unique("stage picker filter");
    let area = egui::Area::new(egui::Id::unique("stage picker")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).inner_margin(egui::Margin { left: 12, right: 0, top: 12, bottom: 12 }).show(ui, |ui| {
            zones::flush(ui);
            ui.set_width(width);
            let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(RichText::new("ADD MODEL").color(TEXT));
                if choices.len() > 12 {
                    let field = ui.add(egui::TextEdit::singleline(&mut filter).hint_text("find").desired_width(180.0));
                    field.request_focus();
                    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.0);
                    if ui.button("×").on_hover_text("Close  Esc").clicked() {
                        shown.clicked.push(Action::AddModel);
                    }
                });
            });
            if choices.is_empty() {
                ui.label(RichText::new("no models (.vvx) in the game yet").color(QUIET));
            }
            let wanted = filter.trim().to_lowercase();
            let tile = 104.0;
            let per_row = ((width + 8.0) / (tile + 8.0)).floor().max(1.0) as usize;
            let shown_ones: Vec<(usize, &String)> = choices.iter().enumerate().filter(|(_, f)| wanted.is_empty() || f.to_lowercase().contains(&wanted)).collect();
            egui::ScrollArea::vertical().id_salt("picker").max_height((free.height() - 140.0).max(160.0)).show(ui, |ui| {
                for chunk in shown_ones.chunks(per_row) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for &(i, file) in chunk {
                            let (rect, response) = ui.allocate_exact_size(Vec2::new(tile, tile + 20.0), egui::Sense::click());
                            if response.hovered() {
                                ui.painter().rect_filled(rect, 4.0, crate::gui::ROW_HOVERED);
                            }
                            let path = ed.dir.join(file);
                            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                            let (tex, px) = thumbs.get(ui.ctx(), format!("model {file}"), (file, modified), || super::panels::model_picture(&path, 64));
                            let scale = ((tile - 16.0) / px.x.max(px.y).max(1.0)).floor().max(1.0);
                            let pic = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + tile / 2.0), px * scale);
                            ui.painter().image(tex, pic, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            let stem = std::path::Path::new(file).file_stem().map_or(file.clone(), |s| s.to_string_lossy().into_owned());
                            let mut job = egui::text::LayoutJob::simple_singleline(stem, TextStyle::Small.resolve(ui.style()), TEXT);
                            job.wrap = egui::text::TextWrapping::truncate_at_width(tile - 4.0);
                            let galley = ui.fonts_mut(|f| f.layout_job(job));
                            ui.painter().galley(Pos2::new(rect.center().x - galley.size().x / 2.0, rect.bottom() - 18.0), galley, TEXT);
                            if response.on_hover_text(file).clicked() {
                                shown.clicked.push(Action::Piece(i));
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

/// The handle under the mouse at `mouse` (egui's points), if the chosen
/// one has handles there.
pub fn grip_under(ed: &Editor, view: &View, screen: Rect, mouse: Pos2) -> Option<Grip> {
    let (h, vp, _) = handles_on_screen(ed, view, screen)?;
    h.grip_at(vp, glam::Vec2::new(screen.width(), screen.height()), glam::Vec2::new(mouse.x - screen.left(), mouse.y - screen.top()))
}
