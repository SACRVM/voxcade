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
use super::zones::{self, FIELD, LABEL, Sort, keyed, rgb, row, small};
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
    let free = zones::layout(ui, shown, |ui, shown| top_bar(ui, ed, view, shown), |ui, shown| outliner(ui, ed, view, thumbs, shown), |ui, shown| inspector(ui, ed, view, figures, shown));
    if let Some(m) = &ed.message {
        zones::mistake(ui, free, m, shown);
    }
    zones::from_disk(ui, free, ed, shown);
    if ed.among_pieces() {
        beside(ui, ed, view, free, shown);
    }
    zones::view_buttons(ui, free, None, None, shown);
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
            keyed(ui, shown, "THINGS", Action::Layer(Layer::Things), Some(false), zones::THINGS_TIP);
        }
        if ed.level.has_grid() || ed.level.terrain.is_some() {
            keyed(ui, shown, "PIECES", Action::Layer(Layer::Pieces), Some(ed.layer == Layer::Pieces), zones::PIECES_TIP);
        }
        if ed.level.terrain.is_some() {
            keyed(ui, shown, "TERRAIN", Action::Layer(Layer::Terrain), Some(ed.layer == Layer::Terrain), "The land: its points, their heights and shapes, its materials");
        }
        if ed.level.has_grid() || ed.level.terrain.is_some() {
            ui.add_space(14.0);
        }
        let add = keyed(ui, shown, "+ ADD", Action::AddMenu, Some(view.menu.is_some_and(|m| m.on == MenuOn::Add)), "Add a model, a light, a place or an area");
        // The menu hangs under it (`menu`).
        ui.data_mut(|d| d.insert_temp(egui::Id::unique("stage add at"), add.rect.left_bottom()));
        // Its entries' keys work without it open.
        for (a, word) in [(Action::AddModel, "ADD MODEL"), (Action::AddLight, "ADD LIGHT"), (Action::AddPlace, "ADD PLACE")] {
            if let Some(k) = key_of(a) {
                zones::key_named(shown, k, a, word);
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
            keyed(ui, shown, "LEVEL", Action::LevelSettings, Some(view.level_settings || (ed.pieces.chosen.is_none() && !ed.among_land())), "The level's size and daylight");
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
        // Its place, quiet at the end: two gems are told apart.
        let at = ed.at_of(item).map(|[x, y, z]| format!("{x} {y} {z}"));
        zones::list_row_aside(ui, sort_of(ed, item), None, &label, 0.0, chosen, !named || hidden, light, None, at.as_deref(), &what_of(ed, item), egui::Sense::click())
    })
    .inner
}

/// The outliner: the three groups with their counts, each folding at its
/// heading; a field to find by name when there are many; one scroll.
fn outliner(ui: &mut egui::Ui, ed: &Editor, view: &View, thumbs: &mut Thumbs, shown: &mut Shown) {
    if ed.among_land() {
        return land_outliner(ui, ed, thumbs, shown);
    }
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
    let scroll = last != Some(ed.pieces.chosen);
    ui.data_mut(|d| d.insert_temp(seen_id, ed.pieces.chosen));
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
                let chosen = ed.pieces.chosen == Some(item);
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
    if ed.among_land() && !view.level_settings {
        return land(ui, ed, shown);
    }
    match ed.pieces.chosen.filter(|_| !view.level_settings) {
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
        None if !t.is_piece() => ed.pieces.kind_models.get(&t.kind).map(|m| (t.kind.clone(), format!("{} (c.things.{}): its model {m}", t.kind, t.kind))),
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
                let free = ed.pieces.free_kinds.contains(&t.kind);
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

/// A piece's rows: ROTATE, SOLID (and LIGHT + without one), its lights.
fn piece_rows(ui: &mut egui::Ui, ed: &Editor, figures: &Figures, t: &crate::grid::Thing, shown: &mut Shown) {
    // On the grid (or solid) the turns step by quarters.
    let quarters = !ed.pieces.free_turns || t.solid;
    let speed = if quarters { 2.0 } else { 0.5 };
    let tip = |what: &str| if quarters { format!("{what}, in quarter turns (this game sets things on the grid{})", if t.solid && ed.pieces.free_turns { "; a solid piece too" } else { "" }) } else { what.to_string() };
    // X, Y, Z as POSITION and the rings' colors: about X its pitch, about
    // Y its roll, about Z its rotation (the file's words).
    row(ui, "ROTATE", "Degrees about X, Y and Z (pitch, roll, rotation in the level's file). Or drag its rings.", |ui| {
        field(ui, shown, "X", Some(0), t.pitch, Field::Pitch, speed, -360.0..=720.0, &tip("pitch: its front up (90 lays it on its back)"), FIELD);
        field(ui, shown, "Y", Some(1), t.roll, Field::Roll, speed, -360.0..=720.0, &tip("roll: its right side down (90 lays it on its right side)"), FIELD);
        field(ui, shown, "Z", Some(2), room::bearing_of(t).rem_euclid(360.0), Field::Turn, speed, -360.0..=720.0, &tip("rotation: round, clockwise seen from above; 0 away from the camera, 180 towards it (a new piece faces the camera)"), FIELD);
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
        let chosen = ed.pieces.chosen_light == Some(k);
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
        row(ui, "SIZE", "How big the grid is, in blocks: across, in depth, up (it grows or is cut on the side ANCHOR leaves free)", |ui| {
            for a in 0..3 {
                field(ui, shown, ["X", "Y", "Z"][a], Some(a), s[a] as f64, Field::LevelSize(a), 0.1, 1.0..=crate::editor::model::MOST_CELLS as f64, "", FIELD);
            }
        });
        row(ui, "ANCHOR", "The side that stays when SIZE changes", |ui| {
            for (a, sides) in [["LEFT", "RIGHT"], ["FRONT", "BACK"], ["BOTTOM", "TOP"]].into_iter().enumerate() {
                let width = zones::col(ui, 1);
                let word = sides[!ed.anchor[a] as usize];
                if ui.add_sized([width, ui.text_style_height(&TextStyle::Body) + 8.0], egui::Button::new(RichText::new(word).small())).on_hover_text(format!("{word} stays; a click: {}", sides[ed.anchor[a] as usize])).clicked() {
                    shown.clicked.push(Action::Anchor(a as u8));
                }
            }
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

/// A material's or a layer's row dragged (its place in the list).
#[derive(Clone, Copy)]
struct MaterialRow(usize);
#[derive(Clone, Copy)]
struct LayerRow(usize);
#[derive(Clone, Copy)]
struct ScatterRow(usize);

/// A scatter rule in short: how many on 100 by 100 voxels, its conditions
/// (as a material's).
pub fn scatter_text(r: &crate::terrain::Scatter) -> String {
    let mut out = vec![format!("{}", r.density)];
    out.extend(r.above.map(|v| format!(">{v}")));
    out.extend(r.below.map(|v| format!("<{v}")));
    out.extend(r.steeper.map(|v| format!(">{v}°")));
    out.extend(r.flatter.map(|v| format!("<{v}°")));
    out.join(" ")
}

/// A condition of a material or a scatter rule: its word (a click switches
/// it on or off: `switch`), its number where on (`set`), on half a row.
#[allow(clippy::too_many_arguments)]
fn condition(ui: &mut egui::Ui, shown: &mut Shown, word: &str, value: Option<f64>, decimals: usize, tip: &str, switch: Action, set: impl FnOnce(f64) -> Action) {
    let on = value.is_some();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(44.0, 22.0), egui::Sense::click());
    let colour = if on { ACCENT } else { QUIET };
    ui.painter().text(rect.left_center() + Vec2::new(2.0, 0.0), egui::Align2::LEFT_CENTER, word, TextStyle::Small.resolve(ui.style()), colour);
    if response.on_hover_text(tip).clicked() {
        shown.clicked.push(switch);
    }
    match value {
        Some(v) => zones::number(ui, shown, "", None, v, decimals, 1.0, -4096.0..=4096.0, tip, 44.0, set),
        None => {
            ui.allocate_exact_size(Vec2::new(44.0, 22.0), egui::Sense::hover());
        }
    }
}

/// A material's conditions in short: `>82` above, `<40` below, `>42°`
/// steeper, `<10°` flatter, `~8` its blend; `else` for the last.
pub fn conditions_text(m: &crate::terrain::Material, last: bool) -> String {
    if last {
        return "else".into();
    }
    let mut out: Vec<String> = Vec::new();
    out.extend(m.above.map(|v| format!(">{v}")));
    out.extend(m.below.map(|v| format!("<{v}")));
    out.extend(m.steeper.map(|v| format!(">{v}°")));
    out.extend(m.flatter.map(|v| format!("<{v}°")));
    out.extend(m.blend.map(|v| format!("~{v}")));
    if out.is_empty() { "always".into() } else { out.join(" ") }
}

/// Small squares of colors, as a row's picture: room for three, so the
/// rows' words line up.
fn swatches(ui: &mut egui::Ui, colors: &[[u8; 3]]) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(3.0 * 12.0 + 2.0 * 3.0, 12.0), egui::Sense::hover());
    for (k, c) in colors.iter().take(3).enumerate() {
        let at = rect.left_top() + Vec2::new(k as f32 * 15.0, 0.0);
        ui.painter().rect_filled(Rect::from_min_size(at, Vec2::splat(12.0)), 2.0, rgb(*c));
    }
}

/// TERRAIN's outliner: its lists, a row each, the chosen one lit and its
/// fields in the inspector: MATERIALS (the first that fits paints, the last
/// paints the rest) and LAYERS (from the top down); + at a heading adds,
/// × takes out, a row dragged onto another moves it. The level's things
/// are not listed here (they stay drawn).
fn land_outliner(ui: &mut egui::Ui, ed: &Editor, thumbs: &mut Thumbs, shown: &mut Shown) {
    use super::model::land::{LandField, LandRow};
    let Some(t) = ed.terrain() else { return };
    egui::ScrollArea::vertical().id_salt("land outliner").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        let extras = [("+", Action::AddMaterial, "A material more (rock where steep), first", true)];
        if zones::group_with(ui, "MATERIALS", Some(t.materials.len()), "The land's colors, top to bottom: the first whose conditions all hold paints a voxel, the last paints the rest", &extras, shown) {
            let last = t.materials.len() - 1;
            for (k, m) in t.materials.iter().enumerate() {
                let chosen = ed.land.row == Some(LandRow::Material(k));
                let response = ui
                    .dnd_drag_source(egui::Id::unique(("land material", k)), MaterialRow(k), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.add_space(4.0);
                            swatches(ui, &m.all_tones());
                            let width = (ui.available_width() - 34.0).max(20.0);
                            let text = RichText::new(conditions_text(m, k == last)).small().color(if chosen { ACCENT } else { TEXT });
                            if ui.add_sized([width, 20.0], egui::Button::selectable(chosen, text)).on_hover_text("Its fields in the inspector").clicked() {
                                shown.clicked.push(Action::LandRow(LandRow::Material(k)));
                            }
                            if ui.add_enabled(last > 0, egui::Button::new(RichText::new("×").small())).on_hover_text("Take it out").on_disabled_hover_text("The last one left").clicked() {
                                shown.clicked.push(Action::RemoveMaterial(k as u8));
                            }
                        });
                    })
                    .response;
                if let Some(from) = response.dnd_release_payload::<MaterialRow>()
                    && from.0 != k
                {
                    shown.clicked.push(Action::MoveMaterial(from.0 as u8, k as u8));
                }
            }
            ui.add_space(6.0);
        }
        let extras = [("+", Action::AddLayer, "A band of rock more on top", true)];
        if zones::group_with(ui, "LAYERS", Some(t.layers.len()), "Bands of rock STEP's walls are cut in, from the top down here, again and again upward: THICK voxels, HARD 1 a wall with a ledge on top, 0 a slope of scree", &extras, shown) {
            for (k, l) in t.layers.iter().enumerate().rev() {
                let kk = k as u8;
                let response = ui
                    .dnd_drag_source(egui::Id::unique(("land layer", k)), LayerRow(k), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.add_space(4.0);
                            let mut c = l.rgb();
                            if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Its color in the walls").changed() {
                                shown.clicked.push(Action::LayerColour(kk, c));
                            }
                            zones::number(ui, shown, "", None, l.thick as f64, 0, 0.1, 1.0..=4096.0, "THICK: voxels", 52.0, move |v| Action::LandSet(LandField::Thick(kk), v));
                            zones::number(ui, shown, "", None, l.hard, 2, 0.01, 0.0..=1.0, "HARD: 1 a wall with a ledge on top, 0 a slope of scree", 58.0, move |v| Action::LandSet(LandField::Hard(kk), v));
                            if ui.add(egui::Button::new(RichText::new("×").small())).on_hover_text("Take it out").clicked() {
                                shown.clicked.push(Action::RemoveLayer(kk));
                            }
                        });
                    })
                    .response;
                if let Some(from) = response.dnd_release_payload::<LayerRow>()
                    && from.0 != k
                {
                    shown.clicked.push(Action::MoveLayer(from.0 as u8, kk));
                }
            }
            ui.add_space(6.0);
        }
        let extras = [("+", Action::AddScatter, "A scatter rule more", true)];
        if zones::group_with(ui, "SCATTER", Some(t.scatter.len()), "Models scattered over the land by rule: as pieces (SCATTER sets them into the level) or as voxels baked into it", &extras, shown) {
            for (k, r) in t.scatter.iter().enumerate() {
                let chosen = ed.land.row == Some(LandRow::Scatter(k));
                let response = ui
                    .dnd_drag_source(egui::Id::unique(("land scatter", k)), ScatterRow(k), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.add_space(4.0);
                            let file = r.models.first().cloned().unwrap_or_default();
                            let (tex, px, _) = thumbs.model(ui.ctx(), &ed.dir, &file, 64);
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 18.0), egui::Sense::hover());
                            let scale = 18.0 / px.x.max(px.y).max(1.0);
                            ui.painter().image(tex, Rect::from_center_size(rect.left_center() + Vec2::new(9.0, 0.0), px * scale), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            let width = (ui.available_width() - 40.0).max(20.0);
                            let what = if r.makes() == crate::terrain::ScatterAs::Voxels { "V" } else { "P" };
                            let text = RichText::new(format!("{} {what}", scatter_text(r))).small().color(if chosen { ACCENT } else { TEXT });
                            if ui.add_sized([width, 20.0], egui::Button::selectable(chosen, text)).on_hover_text(format!("{}: P pieces, V voxels; its fields in the inspector", r.models.join(", "))).clicked() {
                                shown.clicked.push(Action::LandRow(LandRow::Scatter(k)));
                            }
                            if ui.add(egui::Button::new(RichText::new("×").small())).on_hover_text("Take the rule out (its pieces stay)").clicked() {
                                shown.clicked.push(Action::RemoveScatter(k as u8));
                            }
                        });
                    })
                    .response;
                if let Some(from) = response.dnd_release_payload::<ScatterRow>()
                    && from.0 != k
                {
                    shown.clicked.push(Action::MoveScatter(from.0 as u8, k as u8));
                }
            }
        }
    });
}

/// TERRAIN's inspector: the tool and its brush, the chosen points' height
/// and shape, the terrain's numbers, and the fields of the material chosen
/// in the outliner.
fn land(ui: &mut egui::Ui, ed: &Editor, shown: &mut Shown) {
    use super::model::land::{LandField, LandRow, LandTool};
    use super::model::selection::SelectMode;
    use crate::terrain::Shape;
    let Some(t) = ed.terrain() else { return };
    let l = &ed.land;
    ui.label(RichText::new("TERRAIN").color(TEXT)).on_hover_text(&ed.file);
    row(ui, "TOOL", "What the mouse does among the points", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            ui.spacing_mut().button_padding = egui::vec2(7.0, 3.0);
            for tool in LandTool::ALL {
                small(ui, shown, tool.label(), Action::LandTool(tool), Some(l.tool == tool), tool.tip());
            }
        });
    });
    if l.tool == LandTool::Select {
        row(ui, "MODE", "What SELECT's rectangle does", |ui| {
            for (word, mode, tip) in [("NEW", SelectMode::New, "The rectangle's points instead"), ("ADD", SelectMode::Add, "The rectangle's points besides"), ("REMOVE", SelectMode::Remove, "The rectangle's points out")] {
                small(ui, shown, word, Action::LandMode(mode), Some(l.mode == mode), tip);
            }
        });
    } else {
        row(ui, "RADIUS", "How far a drag reaches round the point, in points (1: the point alone)", |ui| {
            zones::number(ui, shown, "", None, l.radius as f64, 0, 0.1, 1.0..=64.0, "", FIELD, |v| Action::LandSet(LandField::Radius, v));
        });
    }
    // The chosen points: which, their height, their shape.
    let chosen: Vec<glam::IVec2> = if l.selected.is_empty() { l.chosen.into_iter().collect() } else { l.selected.clone() };
    let first = chosen.first().and_then(|p| t.index(*p));
    row(ui, "POINT", "The point chosen (x, y from the near left), or how many are selected; HEIGHT over the land's base, voxels", |ui| {
        let word = match (chosen.len(), l.chosen) {
            (0, _) => "NONE".to_string(),
            (1, Some(p)) => format!("{} {}", p.x, p.y),
            (n, _) => format!("{n} SEL"),
        };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(zones::col(ui, 1), 20.0), egui::Sense::hover());
        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, word, TextStyle::Body.resolve(ui.style()), if chosen.is_empty() { QUIET } else { TEXT });
        if let Some(i) = first {
            zones::number(ui, shown, "HEIGHT", None, t.heights[i] as f64, 0, 0.3, 0.0..=crate::terrain::HIGHEST as f64, "The chosen points' height over the land's base, voxels", zones::FIELD2, |v| Action::LandSet(LandField::Height, v));
        }
    });
    let shape_now = first.map_or(l.shape, |i| t.shapes[i]);
    row(ui, "SHAPE", "How the land goes from the chosen points to the next (PAINT lays the one lit)", |ui| {
        for (shape, tip) in [(Shape::Smooth, "Round hills: a smooth curve through the points"), (Shape::Linear, "Ridges and peaks: straight slopes"), (Shape::Step, "Terraces and cliffs along the land's contours")] {
            small(ui, shown, shape.name(), Action::LandShape(shape), Some(shape_now == shape), tip);
        }
    });
    ui.add_space(6.0);
    row(ui, "GRID", "Voxels between the points, and how many points across and deep (added or taken at the right and the far edge)", |ui| {
        zones::number(ui, shown, "", None, t.spacing as f64, 0, 0.1, 1.0..=1024.0, "Voxels between the points", FIELD, |v| Action::LandSet(LandField::Spacing, v));
        zones::number(ui, shown, "X", Some(0), t.cols as f64, 0, 0.1, 1.0..=4096.0, "Points across", FIELD, |v| Action::LandSet(LandField::Cols, v));
        zones::number(ui, shown, "Y", Some(1), t.rows as f64, 0, 0.1, 1.0..=4096.0, "Points deep", FIELD, |v| Action::LandSet(LandField::Rows, v));
    });
    row(ui, "ROUGH", "How uneven the land is, 0 to 1 (where ROUGHEN painted it); TERRACE: STEP's terraces' height without layers", |ui| {
        zones::number(ui, shown, "", None, t.rough.unwrap_or(crate::terrain::ROUGH), 2, 0.01, 0.0..=1.0, "How uneven the land is, 0 to 1", FIELD, |v| Action::LandSet(LandField::Rough, v));
        // (With layers their bands are the terraces.)
        if t.layers.is_empty() {
            zones::number(ui, shown, "TERRACE", None, t.terrace.unwrap_or(crate::terrain::TERRACE) as f64, 0, 0.1, 1.0..=4096.0, "STEP's terraces: how high each is, voxels", zones::FIELD2, |v| Action::LandSet(LandField::Terrace, v));
        }
    });
    row(ui, "SEED", "The pattern of the grain, the patches and the unevenness", |ui| {
        zones::number(ui, shown, "", None, t.seed.unwrap_or(0) as f64, 0, 0.2, 0.0..=1e9, "", FIELD, |v| Action::LandSet(LandField::Seed, v));
        crate::gui::dice(ui);
        small(ui, shown, "REROLL", Action::Reroll, None, "Another seed: the unevenness, the patches and the grain anew");
    });
    // How the layers' bands lie.
    if !t.layers.is_empty() {
        row(ui, "DIP", "How many degrees the bands tilt (0 level), and TO which way they tilt down (degrees clockwise from north; from the seed until set)", |ui| {
            zones::number(ui, shown, "", None, t.dip.unwrap_or(crate::terrain::DIP), 1, 0.1, 0.0..=45.0, "Degrees the bands tilt", FIELD, |v| Action::LandSet(LandField::Dip, v));
            let to = t.dip_to.unwrap_or(f64::NAN);
            zones::number(ui, shown, "TO", None, if to.is_nan() { 0.0 } else { to }, 0, 1.0, 0.0..=359.0, "The way they tilt down, degrees clockwise from north", zones::FIELD2, |v| Action::LandSet(LandField::DipTo, v));
        });
        row(ui, "FOLD", "Broad folds, layers swelling, thinning and pinching out, 0 to 1 (0 with DIP 0: flat as a cake); FAULT: the bands broken along one line, 0 to 1", |ui| {
            zones::number(ui, shown, "", None, t.fold.unwrap_or(crate::terrain::FOLD), 2, 0.01, 0.0..=1.0, "Broad folds, layers swelling and thinning, 0 to 1", FIELD, |v| Action::LandSet(LandField::Fold, v));
            zones::number(ui, shown, "FAULT", None, t.fault.unwrap_or(0.0), 2, 0.01, 0.0..=1.0, "The bands broken along one line, 0 to 1", zones::FIELD2, |v| Action::LandSet(LandField::Fault, v));
        });
    }
    let last = t.materials.len() - 1;
    row(ui, "UNDER", "What lies under the materials", |ui| {
        let mut c = t.under.as_deref().and_then(crate::grid::hex).unwrap_or_else(|| t.materials[last].rgb());
        if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Its color").changed() {
            shown.clicked.push(Action::LandColour(t.materials.len() as u8, c));
        }
    });
    // The material chosen in the outliner.
    if let Some(LandRow::Material(k)) = l.row
        && let Some(m) = t.materials.get(k)
    {
        let kk = k as u8;
        ui.add_space(8.0);
        ui.label(RichText::new(format!("MATERIAL {}", k + 1)).color(TEXT));
        row(ui, "COLORS", "Its color, and tones mixed in patches", |ui| {
            for (n, mut c) in m.all_tones().into_iter().enumerate() {
                if egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text(if n == 0 { "Its color" } else { "A tone mixed in, in patches" }).changed() {
                    shown.clicked.push(Action::Tone(kk, n as u8, c));
                }
            }
            if m.tones.len() < 2 && ui.add(egui::Button::new(RichText::new("+").small())).on_hover_text("A tone more, mixed in in patches").clicked() {
                shown.clicked.push(Action::AddTone(kk));
            }
            if !m.tones.is_empty() && ui.add(egui::Button::new(RichText::new("−").small())).on_hover_text("The last tone out").clicked() {
                shown.clicked.push(Action::RemoveTone(kk));
            }
        });
        row(ui, "GRAIN", "A slight shade voxel by voxel, 0 to 1; PATCH: how big the patches of its tones are, voxels", |ui| {
            zones::number(ui, shown, "", None, m.grain.unwrap_or(0.0), 2, 0.01, 0.0..=1.0, "A slight shade voxel by voxel, 0 to 1", FIELD, move |v| Action::LandSet(LandField::Grain(kk), v));
            if !m.tones.is_empty() {
                zones::number(ui, shown, "PATCH", None, m.patch.unwrap_or(crate::terrain::PATCH) as f64, 0, 0.1, 1.0..=1000.0, "How big the patches of its tones are, voxels across", zones::FIELD2, move |v| Action::LandSet(LandField::Patch(kk), v));
            }
        });
        if k < last {
            row(ui, "HEIGHT", "Where it paints by height over the land's base: ABOVE, BELOW (a click switches one on or off)", |ui| {
                condition(ui, shown, "ABOVE", m.above.map(f64::from), 0, "Voxels higher than this", Action::LandCondition(kk, 0, m.above.is_none()), move |v| Action::LandSet(LandField::Above(kk), v));
                condition(ui, shown, "BELOW", m.below.map(f64::from), 0, "Voxels lower than this", Action::LandCondition(kk, 1, m.below.is_none()), move |v| Action::LandSet(LandField::Below(kk), v));
            });
            row(ui, "SLOPE", "Where it paints by slope: STEEPER, FLATTER, degrees (a click switches one on or off)", |ui| {
                condition(ui, shown, "STEEP", m.steeper, 1, "Slopes steeper than this, degrees", Action::LandCondition(kk, 2, m.steeper.is_none()), move |v| Action::LandSet(LandField::Steeper(kk), v));
                condition(ui, shown, "FLAT", m.flatter, 1, "Slopes flatter than this, degrees", Action::LandCondition(kk, 3, m.flatter.is_none()), move |v| Action::LandSet(LandField::Flatter(kk), v));
            });
            row(ui, "BLEND", "How far its borders wander: voxels of height (lower in hollows), degrees of slope", |ui| {
                zones::number(ui, shown, "", None, m.blend.unwrap_or(0.0), 1, 0.1, 0.0..=1000.0, "", FIELD, move |v| Action::LandSet(LandField::Blend(kk), v));
            });
        }
    }
    // The scatter rule chosen in the outliner.
    if let Some(LandRow::Scatter(k)) = l.row
        && let Some(r) = t.scatter.get(k)
    {
        use crate::terrain::{Rotate, ScatterAs};
        let kk = k as u8;
        ui.add_space(8.0);
        ui.label(RichText::new(format!("SCATTER {}", k + 1)).color(TEXT));
        row(ui, "MODEL", "What it sets down (a game's model)", |ui| {
            let choices = ed.model_choices();
            let now = r.models.join(", ");
            egui::ComboBox::from_id_salt(("scatter model", k)).selected_text(RichText::new(&now).small()).width(zones::col(ui, 3)).show_ui(ui, |ui| {
                for (n, c) in choices.iter().enumerate() {
                    if ui.selectable_label(r.models.len() == 1 && r.models[0] == *c, c).clicked() {
                        shown.clicked.push(Action::ScatterModel(kk, n as u16));
                    }
                }
            });
        });
        row(ui, "DENSITY", "How many on 100 by 100 voxels of ground that fits; APART: the least distance between two, voxels", |ui| {
            zones::number(ui, shown, "", None, r.density, 2, 0.05, 0.0..=10_000.0, "How many on 100 by 100 voxels of ground that fits", FIELD, move |v| Action::LandSet(LandField::Density(kk), v));
            zones::number(ui, shown, "APART", None, r.apart.unwrap_or(0) as f64, 0, 0.2, 0.0..=4096.0, "The least distance between two, voxels", zones::FIELD2, move |v| Action::LandSet(LandField::Apart(kk), v));
        });
        row(ui, "SINK", "How deep into the land, voxels (a fir on a slope does not hover)", |ui| {
            zones::number(ui, shown, "", None, r.sink.unwrap_or(0) as f64, 0, 0.1, 0.0..=4096.0, "", FIELD, move |v| Action::LandSet(LandField::Sink(kk), v));
        });
        row(ui, "ROTATE", "How each is turned: any way (a game that turns things freely; else quarters), in quarter turns, not at all", |ui| {
            for (word, rot) in [("ANY", Rotate::Any), ("QUARTERS", Rotate::Quarters), ("NONE", Rotate::None)] {
                small(ui, shown, word, Action::ScatterRotate(kk, rot), Some(r.rotate() == rot), "");
            }
        });
        row(ui, "HEIGHT", "Where it sets down by the land's height over its base: ABOVE, BELOW (a click switches one on or off)", |ui| {
            condition(ui, shown, "ABOVE", r.above.map(f64::from), 0, "Ground higher than this", Action::ScatterCondition(kk, 0, r.above.is_none()), move |v| Action::LandSet(LandField::ScAbove(kk), v));
            condition(ui, shown, "BELOW", r.below.map(f64::from), 0, "Ground lower than this", Action::ScatterCondition(kk, 1, r.below.is_none()), move |v| Action::LandSet(LandField::ScBelow(kk), v));
        });
        row(ui, "SLOPE", "Where it sets down by slope: STEEPER, FLATTER, degrees (a click switches one on or off)", |ui| {
            condition(ui, shown, "STEEP", r.steeper, 1, "Slopes steeper than this, degrees", Action::ScatterCondition(kk, 2, r.steeper.is_none()), move |v| Action::LandSet(LandField::ScSteeper(kk), v));
            condition(ui, shown, "FLAT", r.flatter, 1, "Slopes flatter than this, degrees", Action::ScatterCondition(kk, 3, r.flatter.is_none()), move |v| Action::LandSet(LandField::ScFlatter(kk), v));
        });
        row(ui, "BLEND", "How far its borders wander: voxels of height, degrees of slope", |ui| {
            zones::number(ui, shown, "", None, r.blend.unwrap_or(0.0), 1, 0.1, 0.0..=1000.0, "", FIELD, move |v| Action::LandSet(LandField::ScBlend(kk), v));
        });
        row(ui, "AS", "PIECES: set into the level by SCATTER, the level's own from then on; VOXELS: baked into the land as the level is built", |ui| {
            small(ui, shown, "PIECES", Action::ScatterAs(kk, ScatterAs::Pieces), Some(r.makes() == ScatterAs::Pieces), "Set into the level by SCATTER, each a piece of its own");
            small(ui, shown, "VOXELS", Action::ScatterAs(kk, ScatterAs::Voxels), Some(r.makes() == ScatterAs::Voxels), "Baked into the land's voxels as the level is built (boulders, stumps)");
        });
        if r.makes() == ScatterAs::Pieces {
            let set = ed.scattered(k);
            row(ui, "", "", |ui| {
                let tip = if set > 0 { "Its pieces are there: CLEAR first" } else { "Set its pieces into the level, each a piece of its own from then on" };
                if ui.add_enabled(set == 0, egui::Button::new(RichText::new("SCATTER").small())).on_hover_text(tip).on_disabled_hover_text(tip).clicked() {
                    shown.clicked.push(Action::Scatter(kk));
                }
                if ui.add_enabled(set > 0, egui::Button::new(RichText::new(format!("CLEAR {set}")).small())).on_hover_text("Take away the pieces it set down, moved ones too").on_disabled_hover_text("None of its pieces are there").clicked() {
                    shown.clicked.push(Action::ClearScatter(kk));
                }
            });
        }
    }
    ui.add_space(10.0);
    if ui.add(egui::Button::new(RichText::new("REMOVE TERRAIN").small())).on_hover_text("The land taken out of the level (UNDO brings it back)").clicked() {
        shown.clicked.push(Action::RemoveTerrain);
    }
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
/// MOVE or ROTATE, and what is done to it most, a word each.
fn beside(ui: &mut egui::Ui, ed: &Editor, view: &View, free: Rect, shown: &mut Shown) {
    let Some(item) = ed.pieces.chosen.filter(|_| ed.pieces.hanging.is_none() && ed.gripped().is_none() && ed.dragging().is_none() && !view.picker) else { return };
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
    let kind_model = matches!(item, Item::Thing(i) if !ed.level.things[i].is_piece() && ed.pieces.kind_models.contains_key(&ed.level.things[i].kind));
    let middle = handles::project(vp, glam::Vec2::new(screen.width(), screen.height()), h.centre).map_or(bounds.center().x, |(p, _)| screen.left() + p.x);
    let turning = view.turn_mode(ed);
    let guess = if piece { 345.0 } else if turns { 234.0 } else { 122.0 };
    let rect = zones::bar_at(ui, &format!("stage beside {item:?} {turns}"), middle, bounds, free, guess, shown, |ui, shown| {
        // MOVE (its arrows) or ROTATE (its rings), one lit.
        if turns {
            zones::small(ui, shown, "MOVE", Action::HandleMode(false), Some(!turning), "Arrows: move it along X, Y, Z (or drag it over the floor)");
            zones::small(ui, shown, "ROTATE", Action::HandleMode(true), Some(turning), "Rings: rotate it about its sides and the upright");
            // Turned off the level's axes: its arrows along its own or the
            // level's.
            if !turning && ed.piece_turned_off_axes() {
                ui.add_space(6.0);
                zones::small(ui, shown, "LOCAL", Action::MoveGlobal(false), Some(!ed.rigging.move_global), "Arrows along its own axes, as it stands (it lands on whole voxels)");
                zones::small(ui, shown, "GLOBAL", Action::MoveGlobal(true), Some(ed.rigging.move_global), "Arrows along the level's axes");
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
            if ed.level.terrain.is_none() {
                entries.push(("TERRAIN".into(), Action::AddTerrain, None));
            }
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
                    if ed.pieces.kind_models.contains_key(&t.kind) {
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
                            let (tex, px, problem) = thumbs.model(ui.ctx(), &ed.dir, file, 64);
                            let scale = ((tile - 16.0) / px.x.max(px.y).max(1.0)).floor().max(1.0);
                            let pic = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + tile / 2.0), px * scale);
                            ui.painter().image(tex, pic, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            // A model that does not read: a mark where its
                            // picture would be, why on hover; not to set down.
                            if problem.is_some() {
                                let font = egui::FontId::monospace(tile * 0.4);
                                ui.painter().text(pic.center(), egui::Align2::CENTER_CENTER, "!", font, crate::gui::WARNING);
                            }
                            let stem = std::path::Path::new(file).file_stem().map_or(file.clone(), |s| s.to_string_lossy().into_owned());
                            let color = if problem.is_some() { QUIET } else { TEXT };
                            let mut job = egui::text::LayoutJob::simple_singleline(stem, TextStyle::Small.resolve(ui.style()), color);
                            job.wrap = egui::text::TextWrapping::truncate_at_width(tile - 4.0);
                            let galley = ui.fonts_mut(|f| f.layout_job(job));
                            ui.painter().galley(Pos2::new(rect.center().x - galley.size().x / 2.0, rect.bottom() - 18.0), galley, color);
                            match problem {
                                Some(why) => {
                                    response.on_hover_text(format!("{file}: {why}"));
                                }
                                None => {
                                    if response.on_hover_text(file).clicked() {
                                        shown.clicked.push(Action::Piece(i));
                                    }
                                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::keys;
    use crate::editor::panels::laid_out_at;

    /// A level of pieces with one chosen, laid out as a window lays it.
    fn room(name: &str) -> (Editor, View, crate::scratch::Scratch) {
        let dir = crate::editor::big::room_game(name, 3);
        let mut ed = Editor::open(&dir, "levels/room.toml").unwrap();
        assert!(ed.among_pieces(), "a level without blocks opens among its pieces");
        ed.choose_item(Some(Item::Thing(6)));
        let cfg = crate::session::config::<crate::Voxcade>(&dir).unwrap();
        let view = View::new(&ed, &cfg);
        (ed, view, dir)
    }

    /// The three zones leave the view the room between them: right of the
    /// outliner, under the top bar, left of the inspector, down to the
    /// window's foot.
    #[test]
    fn the_zones_leave_the_view_its_room() {
        let (ed, view, _dir) = room("stage-zones");
        let (shown, _) = laid_out_at(&ed, &view);
        let free = shown.free.expect("the view's room");
        let want = Rect::from_min_max(Pos2::new(zones::OUTLINER, zones::TOP), Pos2::new(1600.0 - zones::INSPECTOR, 900.0));
        for (a, b) in [(free.left(), want.left()), (free.top(), want.top()), (free.right(), want.right()), (free.bottom(), want.bottom())] {
            assert!((a - b).abs() <= 5.0, "{free:?} for {want:?}");
        }
    }

    /// Every key a menu names among the pieces (on a piece, on the floor,
    /// ADD's) is on F1's page and works there: the menus only save a way.
    /// The bar at the chosen piece offers what its menu offers most, its
    /// keys among the panels'.
    #[test]
    fn the_menus_keys_are_on_the_page() {
        let (ed, view, _dir) = room("stage-menus");
        let (shown, _) = laid_out_at(&ed, &view);
        let page = keys::overview(&ed, &shown.named);
        let mut named = 0;
        for on in [MenuOn::Item(Item::Thing(6)), MenuOn::Floor(IVec3::new(50, 50, 2)), MenuOn::Add] {
            let entries = menu_entries(&ed, on);
            assert!(!entries.is_empty(), "{on:?} offers something");
            for (word, action, key) in entries {
                if let Some(k) = key {
                    named += 1;
                    assert!(keys::lists(&page, k), "{word} {k} on the page");
                    // (T among the pieces is the window's own, `pieces_key`.)
                    if let Some(c) = k.chars().next().filter(|_| k.len() == 1)
                        && let Some(a) = crate::editor::panels::key_action(&shown.keys, c)
                    {
                        assert_eq!(a, action, "{word} {k} does what the entry does");
                    }
                }
            }
        }
        assert!(named >= 7, "the menus name keys: {named}");
    }
}
