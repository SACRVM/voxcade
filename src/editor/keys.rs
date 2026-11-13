//! F1: the keys of what is open, on one page over the view (the owner,
//! 2026-11-13: an overview of the keys for each scope, and each tool's
//! key in its button's tooltip; no sentences in the panels). The keys the
//! panels show come with the words of their buttons (`Shown::named`); the
//! ones the window takes itself are listed here, by scope, in the words
//! the buttons and menus use.

use super::model::{Editor, Scope};

/// A group of the page: its heading, and its keys with their words.
pub type Group = (&'static str, Vec<(String, String)>);

/// The page for what `ed` has open: the keys of the buttons shown
/// (`named`, the panels' this frame), the editor's own for the scope, the
/// ones every scope has, the mouse's in the view.
pub fn overview(ed: &Editor, named: &[(String, String)]) -> Vec<Group> {
    let pairs = |list: &[(&str, &str)]| list.iter().map(|(k, w)| (k.to_string(), w.to_string())).collect::<Vec<_>>();
    let mut groups: Vec<Group> = Vec::new();
    if !named.is_empty() {
        groups.push(("BUTTONS", named.to_vec()));
    }
    let voxels = matches!(ed.scope, Scope::Block(_) | Scope::Frame(_) | Scope::Figure);
    let scope: Vec<(&str, &str)> = if ed.among_pieces() {
        vec![
            ("↑ ↓ ← →", "MOVE"),
            ("Shift+↑ ↓ ← →", "MOVE BY 10"),
            ("Page Up, Page Down, + −", "UP, DOWN"),
            ("T", "ROTATE 90°"),
            ("Shift+T", "ROTATE −90°"),
            ("Del", "REMOVE"),
            ("F2", "RENAME"),
            ("Esc", "DESELECT"),
        ]
    } else if ed.among_land() {
        vec![("Shift+drag", "FINE"), ("Esc", "DESELECT")]
    } else if voxels {
        let mut keys = vec![
            ("Shift (held)", "PICK"),
            ("Ctrl (held)", "CUT (BOX, SPHERE)"),
            ("Alt (held)", "EMPTY ONLY (BOX, SPHERE)"),
            ("+ −", "DEPTH (BOX, SPHERE)"),
            ("Ctrl+drag, Alt+drag", "ADD, REMOVE (SELECT)"),
            ("Ctrl+click", "ALL OF THE COLOR (FILL)"),
            ("Del", "REMOVE (SELECTION)"),
            ("Ctrl+C, Ctrl+V", "COPY, PASTE"),
            ("Shift+drag", "FINE (HANDLES)"),
            ("Page Up, Page Down", "SLICE LAYER"),
        ];
        if ed.figure.is_some() {
            keys.extend([("F2", "RENAME PART"), ("Del", "REMOVE PART")]);
        } else {
            keys.push(("← →", "PREVIOUS, NEXT FRAME"));
        }
        keys.push(("Esc", "POINTER, BACK"));
        keys
    } else {
        vec![("Shift (held)", "PICK"), ("Esc", "POINTER")]
    };
    groups.push((scope_name(ed), pairs(&scope)));
    groups.push(("EVERYWHERE", pairs(&[("Ctrl+Z", "UNDO"), ("Ctrl+Y, Ctrl+Shift+Z", "REDO"), ("F1", "KEYS")])));
    groups.push(("VIEW", pairs(&[("Right drag", "ORBIT"), ("Middle drag, Shift+right drag", "PAN"), ("Wheel", "ZOOM")])));
    groups
}

/// Whether the page lists `key` (as a menu names it: "C", "Del", "F2",
/// "Ctrl+C", "Shift"): a key of its own, or one of a line's keys.
pub fn lists(groups: &[Group], key: &str) -> bool {
    groups.iter().flat_map(|(_, keys)| keys).any(|(k, _)| k.split(", ").any(|one| one == key || one.starts_with(&format!("{key} "))))
}

/// What is open, as the page's heading for its own keys says it.
fn scope_name(ed: &Editor) -> &'static str {
    if ed.among_pieces() {
        "PIECES"
    } else if ed.among_land() {
        "TERRAIN"
    } else {
        match ed.scope {
            Scope::Block(_) => "BLOCK",
            Scope::Figure | Scope::Frame(_) if ed.figure.is_some() => "FIGURE",
            Scope::Frame(_) => "MODEL",
            _ if ed.layer == super::model::Layer::Things => "THINGS",
            _ => "BLOCKS",
        }
    }
}

/// The page over the view (`free`): the groups in columns, each key in the
/// quiet colour and its word; F1 or Esc closes it.
pub fn page(ui: &egui::Ui, free: egui::Rect, groups: &[Group], shown: &mut super::panels::Shown) {
    use egui::{RichText, TextStyle};
    let area = egui::Area::new(egui::Id::unique("editor keys")).order(egui::Order::Foreground).pivot(egui::Align2::CENTER_TOP).fixed_pos(egui::pos2(free.center().x, free.top() + 24.0)).constrain_to(free).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("KEYS").color(crate::gui::TEXT));
                ui.label(RichText::new("F1").small().color(crate::gui::QUIET));
            });
            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 28.0;
                for (heading, keys) in groups {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(*heading).small().color(crate::gui::QUIET));
                        egui::Grid::new(("editor keys", *heading)).num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                            for (key, word) in keys {
                                ui.label(RichText::new(key).text_style(TextStyle::Small).color(crate::gui::ACCENT));
                                ui.label(RichText::new(word).text_style(TextStyle::Small).color(crate::gui::TEXT));
                                ui.end_row();
                            }
                        });
                    });
                }
            });
        });
    });
    shown.rects.push(area.response.rect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::Tool;
    use crate::editor::view::{Action, key_of};

    fn key(win: &mut crate::editor::EditorWindow, code: sdl3::keyboard::Keycode) {
        let e = sdl3::event::Event::KeyDown { timestamp: 0, window_id: 0, keycode: Some(code), scancode: None, keymod: sdl3::keyboard::Mod::NOMOD, repeat: false, which: 0, raw: 0 };
        assert!(win.handle(&e, false));
        win.draw().unwrap();
    }

    /// F1 shows the page of the keys and hides it again; Esc hides it
    /// first, the tool in hand kept (Esc would otherwise go back to
    /// POINTER).
    #[test]
    fn f1_shows_the_keys_and_esc_hides_them_first() {
        let (mut win, _dir) = crate::editor::big::wall_window("keys-f1", [12, 4, 10]);
        crate::editor::view::act(&mut win.ed, &mut win.view, Action::Tool(Tool::Add));
        key(&mut win, sdl3::keyboard::Keycode::F1);
        assert!(win.view.keys_shown);
        key(&mut win, sdl3::keyboard::Keycode::F1);
        assert!(!win.view.keys_shown, "F1 again: hidden");
        key(&mut win, sdl3::keyboard::Keycode::F1);
        key(&mut win, sdl3::keyboard::Keycode::Escape);
        assert!(!win.view.keys_shown, "Esc: hidden");
        assert_eq!(win.ed.tool, Tool::Add, "and nothing else");
        key(&mut win, sdl3::keyboard::Keycode::Escape);
        assert_eq!(win.ed.tool, Tool::Pointer, "Esc once more: POINTER");
    }

    /// The page names every tool's key with the tool's word, in a model and
    /// in a level of blocks; the pieces' own keys among the pieces.
    #[test]
    fn the_page_names_each_tools_key() {
        let (win, _dir) = crate::editor::big::wall_window("keys-tools", [12, 4, 10]);
        let level = crate::editor::model::tests::folder("keys-level");
        let ed = Editor::open(&level, "level.toml").unwrap();
        let view = crate::editor::view::View::new(&ed, &Default::default());
        for (ed, view) in [(&win.ed, &win.view), (&ed, &view)] {
            let (shown, _) = crate::editor::panels::laid_out_at(ed, view);
            let page = overview(ed, &shown.named);
            for &tool in Tool::of(ed.scope, ed.layer) {
                if let Some(k) = key_of(Action::Tool(tool)) {
                    let word = tool.label(ed.scope, ed.layer);
                    assert!(page[0].1.contains(&(k.to_string(), word.to_string())), "{word} {k} on the page of {:?}", ed.scope);
                }
            }
            assert!(lists(&page, "F1") && lists(&page, "Ctrl+Z") && lists(&page, "Esc"));
        }
        let room = crate::editor::big::room_game("keys-pieces", 3);
        let mut ed = Editor::open(&room, "levels/room.toml").unwrap();
        ed.choose_item(Some(crate::editor::model::room::Item::Thing(0)));
        let page = overview(&ed, &[]);
        assert_eq!(page[0].0, "PIECES");
        for k in ["T", "Del", "F2", "Page Up", "Esc"] {
            assert!(lists(&page, k), "{k} among the pieces' keys");
        }
    }
}
