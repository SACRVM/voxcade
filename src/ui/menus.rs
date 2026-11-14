//! The engine's menus (title, settings, pause, network...), notices, the
//! frame rate, PRESS START.

use super::*;

// --- menus ---------------------------------------------------------------------------

pub(super) const DIM: [u8; 4] = [8, 6, 6, 255];
pub(super) const SLIDER_EMPTY: [u8; 4] = [70, 62, 54, 255];
/// Quiet text: hints and column heads, dark over the sand of the title,
/// light over the darkened pages.
pub(super) const MUTED_DARK: [u8; 4] = [70, 62, 54, 255];
pub(super) const MUTED_LIGHT: [u8; 4] = [150, 140, 118, 255];
/// The title's rows: idle, chosen, the marker; over a dark scene light.
pub(super) const TITLE_ROWS: ([u8; 4], [u8; 4], [u8; 4]) = ([58, 48, 38, 255], [214, 34, 32, 255], [58, 48, 38, 255]);
pub(super) const TITLE_ROWS_LIGHT: ([u8; 4], [u8; 4], [u8; 4]) = (TEXT, [214, 34, 32, 255], TEXT);
pub(super) const DRIP: [u8; 4] = [150, 14, 16, 255];

/// What the menus show besides their pages: the logo (lines and colours,
/// the one that drips), the songs' titles, the high score.
pub struct MenuView<'a> {
    pub menu: &'a Menu,
    pub settings: &'a Settings,
    pub logo: &'a [(String, [u8; 4])],
    pub drip: Option<usize>,
    pub songs: &'a [String],
    /// The game keeps a score (and so a high score).
    pub scored: bool,
    /// The big letters' font, and the logo lines' heights in voxels.
    pub font: Option<&'a DisplayFont>,
    pub logo_heights: &'a [u32],
    /// Cracks per letter in each logo line.
    pub logo_cracks: &'a [f32],
    /// Where each logo line's letters lie in the model bank, built once
    /// (`logo_letters`); empty: they are built here every frame.
    pub logo_bank: &'a [Vec<std::ops::Range<u32>>],
    /// The game's own logo, a model (`c.title_screen.logo_model`), in
    /// place of the lines when it has one.
    pub logo_model: Option<&'a LogoModel>,
    /// How far the logo's letters bob once landed (`c.title_screen.logo_bob`).
    pub logo_bob: f32,
    /// The title's scene is dark: its rows and hint in light letters.
    pub light_menu: bool,
}

/// The menu's voxels for this frame, the logo's letters drawn from the
/// model bank, and the camera that shows them: on the
/// title the logo and the main items over the scene, the other pages over
/// a darkened screen.
pub fn menu(view: &MenuView, cfg: &UiConfig, alpha: f32, aspect: f32) -> (Vec<Instance>, Vec<ModelDraw>, Mat4) {
    let layout = Layout { width: (cfg.height * aspect).round(), height: cfg.height };
    let camera = camera(&layout, cfg.slant, cfg.depth as f32);
    let finish = Finish::of(cfg);
    let mut out = Vec::new();
    let mut letters = Vec::new();
    let menu = view.menu;
    let Some(page) = menu.page() else { return (out, letters, camera) };
    let depth = cfg.depth;
    let t = (menu.tick - menu.opened) as f32 + alpha;
    let now = menu.tick as f32 + alpha;
    let top = layout.height;
    // The engine's words in the player's language (`c.languages`).
    let lang = view.settings.language();
    let say = |en: &'static str| crate::lang::say(lang, en);

    if page == Page::Title {
        logo(&mut out, &mut letters, view, &layout, now, finish);
        if view.scored {
            let score = crate::lang::say_with(lang, "HI-SCORE {}", &[&format!("{:06}", view.settings.best())]);
            text(&mut out, &score, layout.width - 3.0 - text_width(&score, 1), top - 10.0, 1, NUMBERS, depth);
        }
    } else {
        dim(&mut out, &layout, 0.8);
        // A guest's own menu stops nothing: no PAUSED over it.
        let guest = page == Page::Pause && menu.net.as_ref().is_some_and(|n| !n.hosting);
        let heading = if guest { "MENU" } else { page.heading() };
        dropping(&mut out, say(heading), &layout, None, top - 38.0, 3, TEXT, depth, t, view.font, finish);
    }

    match page {
        Page::Title => {
            // Big items low on the left, clear of what lies in the scene:
            // the last always where it was, a fourth (CONTINUE) above.
            let items = menu.items(page, view.settings);
            let first = 28.0 + (items.len() as f32 - 1.0) * 17.0;
            for (i, item) in items.iter().enumerate() {
                row(&mut out, view, i, &item.label_in(lang), 24.0, first - i as f32 * 17.0, 2, depth, now);
            }
        }
        Page::Pause | Page::Network => {
            let items = menu.items(page, view.settings);
            // Closer where a network game's pause page has many.
            let apart = if items.len() > 6 { 13.0 } else { 17.0 };
            for (i, item) in items.iter().enumerate() {
                let label = match item {
                    Item::Drop(m) => {
                        let name = menu.net.iter().flat_map(|n| &n.drops).find(|(d, _)| d == m).map_or(String::new(), |(_, n)| n.to_uppercase());
                        crate::lang::say_with(lang, "DROP {}", &[&name])
                    }
                    Item::LetIn(p) => {
                        let line = menu.net.iter().flat_map(|n| &n.knocks).find(|(k, _)| k == p).map_or(String::new(), |(_, l)| l.clone());
                        crate::lang::say_with(lang, "LET IN {}", &[&line])
                    }
                    other => other.label_in(lang),
                };
                let scale = if text_width(&label, 2) > layout.width - 8.0 { 1 } else { 2 };
                let x = ((layout.width - text_width(&label, scale)) / 2.0).round();
                row(&mut out, view, i, &label, x, top - 70.0 - i as f32 * apart, scale, depth, now);
            }
            // Where the game is found, small at the top left (one line above
            // the heading's), who plays at the top right, how to let others
            // in under the items.
            if page == Page::Pause
                && let Some(net) = &menu.net
            {
                for (i, line) in net.lines.iter().enumerate() {
                    text(&mut out, line, 6.0, top - 10.0 - i as f32 * 9.0, 1, MUTED_LIGHT, depth);
                }
                for (i, line) in net.who.iter().enumerate() {
                    text(&mut out, line, layout.width - 6.0 - text_width(line, 1), top - 10.0 - i as f32 * 9.0, 1, MUTED_LIGHT, depth);
                }
                for (i, line) in net.help.iter().rev().enumerate() {
                    centred(&mut out, &layout, line, 14.0 + i as f32 * 9.0, 1, MUTED_LIGHT, depth);
                }
            }
        }
        // The games found on the LAN, another game's greyed; typing an
        // address instead.
        Page::Find => {
            let items = menu.items(page, view.settings);
            let x0 = (layout.width / 2.0 - 120.0).round();
            for (i, &item) in items.iter().enumerate() {
                let y = top - 66.0 - i as f32 * 13.0;
                match item {
                    Item::Found(n) if !menu.found[n].same => text(&mut out, &menu.found[n].line, x0, y, 1, MUTED_LIGHT, depth),
                    Item::Found(n) => row(&mut out, view, i, &menu.found[n].line, x0, y, 1, depth, now),
                    other => row(&mut out, view, i, &other.label_in(lang), x0, y, 1, depth, now),
                }
            }
            if menu.found.is_empty() {
                centred(&mut out, &layout, say("SEARCHING THE NETWORK ..."), top - 52.0, 1, MUTED_LIGHT, depth);
                centred(&mut out, &layout, say("NONE FOUND? TYPE THE HOST'S ADDRESS"), 27.0, 1, MUTED_LIGHT, depth);
                centred(&mut out, &layout, say("(A FIREWALL MAY HIDE IT)"), 18.0, 1, MUTED_LIGHT, depth);
            }
        }
        // The host's address being typed, big, the cursor blinking.
        Page::Join => {
            centred(&mut out, &layout, say("THE HOST'S ADDRESS (ITS PAUSE PAGE SAYS IT)"), top - 62.0, 1, MUTED_LIGHT, depth);
            let shown = format!("{}{}", menu.address, if (menu.tick / 20).is_multiple_of(2) { "_" } else { " " });
            let scale = if text_width(&shown, 3) > layout.width - 8.0 { 2 } else { 3 };
            centred(&mut out, &layout, &shown, top - 96.0, scale, NUMBERS, depth);
        }
        // A game's code being typed: six places, the cursor blinking in the
        // next.
        Page::Code => {
            centred(&mut out, &layout, say("THE GAME'S CODE (THE HOST SENDS IT)"), top - 62.0, 1, MUTED_LIGHT, depth);
            let blink = (menu.tick / 20).is_multiple_of(2);
            let shown: String = (0..matchmaker_protocol::LENGTH)
                .map(|i| match menu.code.chars().nth(i) {
                    Some(c) => c,
                    None if i == menu.code.len() && blink => '_',
                    None => '.',
                })
                .collect();
            centred(&mut out, &layout, &shown, top - 96.0, 3, NUMBERS, depth);
        }
        Page::Joining => {
            let lines = menu.net.as_ref().map_or(&[][..], |n| &n.lines[..]);
            for (i, line) in lines.iter().enumerate() {
                centred(&mut out, &layout, line, top - 70.0 - i as f32 * 12.0, 1, if i == 0 { NUMBERS } else { TEXT }, depth);
            }
        }
        // The settings' own page as the pause page: big rows in the middle,
        // LANGUAGE with its value (small where it would not fit).
        Page::Settings => {
            for (i, &item) in menu.items(page, view.settings).iter().enumerate() {
                let mut label = item.label_in(lang);
                if item == Item::Language {
                    label = format!("{label} < {} >", crate::lang::name(lang));
                }
                let scale = if text_width(&label, 2) > layout.width - 8.0 { 1 } else { 2 };
                let x = ((layout.width - text_width(&label, scale)) / 2.0).round();
                row(&mut out, view, i, &label, x, top - 70.0 - i as f32 * 17.0, scale, depth, now);
            }
        }
        Page::Audio | Page::Video => {
            let x0 = (layout.width / 2.0 - 100.0).round();
            let items = menu.items(page, view.settings);
            // A few rows each, roomy under the heading.
            for (i, &item) in items.iter().enumerate() {
                let y = top - 58.0 - i as f32 * 13.0;
                row(&mut out, view, i, &item.label_in(lang), x0, y, 1, depth, now);
                let selected = menu.cursor() == i;
                setting(&mut out, view, item, x0 + 120.0, y, selected, depth);
            }
        }
        Page::Controls => {
            let x0 = (layout.width / 2.0 - 110.0).round();
            let (kx, px) = (x0 + 70.0, x0 + 160.0);
            text(&mut out, say("KEYBOARD"), kx, top - 56.0, 1, MUTED_LIGHT, depth);
            text(&mut out, say("GAMEPAD"), px, top - 56.0, 1, MUTED_LIGHT, depth);
            let items = menu.items(page, view.settings);
            // Down to above the hint at the bottom, closer where there are many.
            let apart = ((top - 70.0 - 30.0) / (items.len().max(2) - 1) as f32).floor().min(12.0);
            for (i, &item) in items.iter().enumerate() {
                let y = top - 70.0 - i as f32 * apart;
                row(&mut out, view, i, &item.label_in(lang), x0, y, 1, depth, now);
                setting(&mut out, view, item, kx, y, menu.cursor() == i, depth);
                let Item::Bind(action) = item else { continue };
                if menu.rebinding == Some(action) {
                    if (menu.tick / 15).is_multiple_of(2) {
                        text(&mut out, say("PRESS A KEY OR BUTTON"), kx, y, 1, NUMBERS, depth);
                    }
                    continue;
                }
                let keys = view.settings.keys.get(action).first().map(|k| view.settings.key_label(k)).unwrap_or_default();
                let pads = view.settings.pad.get(action).first().map(|b| pad_label(b, menu.pad_style)).unwrap_or_default();
                text(&mut out, &keys, kx, y, 1, TEXT, depth);
                text(&mut out, &pads, px, y, 1, TEXT, depth);
            }
            let y = top - 70.0 - items.len() as f32 * apart - 4.0;
            text(&mut out, say("MOVE: ARROWS OR WASD, LEFT STICK OR D-PAD"), x0, y, 1, MUTED_LIGHT, depth);
        }
        Page::Players => {
            // A heading for each seat with its three rows under it, then
            // SWAP, RUMBLE and BACK; scrolled so the chosen row stays in view.
            let x0 = (layout.width / 2.0 - 110.0).round();
            let vx = x0 + 100.0;
            let items = menu.items(page, view.settings);
            let mut rows = Vec::with_capacity(items.len());
            let mut heads = Vec::new();
            let mut y = top - 54.0;
            for &item in &items {
                match item {
                    Item::SeatKeys(n) => {
                        y -= 3.0;
                        heads.push((n, y));
                        y -= 10.0;
                    }
                    Item::Swap => y -= 4.0,
                    _ => {}
                }
                rows.push(y);
                y -= 10.0;
            }
            let lowest = 16.0;
            let shift = (lowest - rows.get(menu.cursor()).copied().unwrap_or(lowest)).max(0.0);
            let shown = |y: f32| (lowest..=top - 50.0).contains(&y);
            for (n, y) in heads {
                let y = y + shift;
                if !shown(y) {
                    continue;
                }
                let head = crate::lang::say_with(lang, "PLAYER {}", &[&n.to_string()]);
                text(&mut out, &head, x0, y, 1, NUMBERS, depth);
                // A seat nobody sits on: START takes it.
                if !view.menu.seats.get(n - 1).is_some_and(|s| s.taken) {
                    text(&mut out, say("PRESS START"), vx, y, 1, MUTED_LIGHT, depth);
                }
            }
            for (i, (&item, &y)) in items.iter().zip(&rows).enumerate() {
                let y = y + shift;
                if !shown(y) {
                    continue;
                }
                let indent = if matches!(item, Item::SeatKeys(_) | Item::SeatPad(_) | Item::SeatControls(_)) { 10.0 } else { 0.0 };
                row(&mut out, view, i, &item.label_in(lang), x0 + indent, y, 1, depth, now);
                setting(&mut out, view, item, vx, y, menu.cursor() == i, depth);
                let value = match item {
                    Item::SeatKeys(n) => say(menu.seats.get(n - 1).map_or(crate::seating::Keys::Nothing, |s| s.keyboard).label()).to_string(),
                    Item::SeatPad(n) => match menu.seats.get(n - 1).map(|s| s.pad.as_str()) {
                        Some(name) if !name.is_empty() => name.to_uppercase().chars().take(24).collect(),
                        _ => say("NONE").to_string(),
                    },
                    _ => continue,
                };
                let selected = menu.cursor() == i;
                let shown = if selected { format!("< {value} >") } else { value };
                text(&mut out, &shown, vx, y, 1, if selected { NUMBERS } else { TEXT }, depth);
            }
        }
        Page::PlayerControls(seat) => {
            let part = menu.seats.get(seat - 1).map_or(crate::seating::Keys::Nothing, |s| s.keyboard);
            let head = crate::lang::say_with(lang, "PLAYER {}", &[&seat.to_string()]);
            centred(&mut out, &layout, &head, top - 46.0, 1, NUMBERS, depth);
            let x0 = (layout.width / 2.0 - 110.0).round();
            let (kx, px) = (x0 + 70.0, x0 + 160.0);
            text(&mut out, say("KEYBOARD"), kx, top - 56.0, 1, MUTED_LIGHT, depth);
            text(&mut out, say("GAMEPAD"), px, top - 56.0, 1, MUTED_LIGHT, depth);
            let items = menu.items(page, view.settings);
            // Down to above the hint at the bottom, closer where there are many.
            let apart = ((top - 66.0 - 24.0) / (items.len().max(2) - 1) as f32).floor().min(12.0);
            let keys = view.settings.keys_of(part);
            let pad = view.settings.pad_of(seat);
            for (i, &item) in items.iter().enumerate() {
                let y = top - 66.0 - i as f32 * apart;
                row(&mut out, view, i, &item.label_in(lang), x0, y, 1, depth, now);
                let Item::Bind(action) = item else { continue };
                if menu.rebinding == Some(action) {
                    if (menu.tick / 15).is_multiple_of(2) {
                        text(&mut out, say("PRESS A KEY OR BUTTON"), kx, y, 1, NUMBERS, depth);
                    }
                    continue;
                }
                let key = keys.map_or_else(|| "-".to_string(), |b| b.get(action).first().map(|k| view.settings.key_label(k)).unwrap_or_default());
                let button = if crate::input::DIRECTIONS.contains(&action) { String::new() } else { pad.get(action).first().map(|b| pad_label(b, menu.pad_style)).unwrap_or_default() };
                text(&mut out, &key, kx, y, 1, TEXT, depth);
                text(&mut out, &button, px, y, 1, TEXT, depth);
            }
            // How it moves where its directions are no rows of their own.
            if !matches!(part, crate::seating::Keys::Left | crate::seating::Keys::Right) {
                let y = top - 66.0 - items.len() as f32 * apart - 4.0;
                let moving = if part == crate::seating::Keys::Whole { "MOVE: ARROWS OR WASD, LEFT STICK OR D-PAD" } else { "MOVE: LEFT STICK OR D-PAD" };
                text(&mut out, say(moving), x0, y, 1, MUTED_LIGHT, depth);
            }
        }
        Page::Save | Page::Load => {
            let x0 = (layout.width / 2.0 - 100.0).round();
            for (i, &item) in menu.items(page, view.settings).iter().enumerate() {
                let label = match item {
                    Item::Slot(n) => menu.slots[n].clone().unwrap_or_else(|| format!("{}  {}", n + 1, say("EMPTY"))),
                    other => other.label_in(lang),
                };
                row(&mut out, view, i, &label, x0, top - 70.0 - i as f32 * 14.0, 1, depth, now);
            }
        }
        Page::Initials => {
            let e = &menu.entry;
            let line = crate::lang::say_with(lang, "{} PLACE   SCORE {}", &[&crate::lang::ordinal(lang, e.rank + 1), &format!("{:06}", e.score)]);
            centred(&mut out, &layout, &line, top - 58.0, 1, NUMBERS, depth);
            centred(&mut out, &layout, say("ENTER YOUR INITIALS"), top - 74.0, 1, MUTED_LIGHT, depth);
            // Whose, where several played.
            if let Some(seat) = e.seat {
                centred(&mut out, &layout, &crate::lang::say_with(lang, "PLAYER {}", &[&seat.to_string()]), top - 48.0, 1, TEXT, depth);
            }
            // Three big letters; the one being picked blinks and hops when
            // it changes, those to come wait as lines.
            let (scale, cell) = (5, 40.0);
            let x0 = (layout.width / 2.0 - cell * 1.5).round();
            for (k, &c) in e.letters.iter().enumerate() {
                let x = x0 + k as f32 * cell;
                if k > e.at {
                    for bx in 6..cell as i32 - 6 {
                        for by in 36..39 {
                            for z in 0..depth {
                                voxel(&mut out, x + bx as f32, by as f32, z as f32, MUTED_LIGHT);
                            }
                        }
                    }
                    continue;
                }
                let color = if k < e.at || (menu.tick / 10).is_multiple_of(2) { TEXT } else { RED };
                let since = now - menu.moved as f32;
                let hop = if k == e.at && since < 10.0 { ((std::f32::consts::PI * since / 10.0).sin() * 3.0).round() } else { 0.0 };
                big_letter(&mut out, c, x, cell, 36.0 + hop, scale, color, depth, view.font, finish);
            }
        }
        Page::HighScores => {
            // The lines drop in one after another, the new one blinking.
            for (i, h) in view.settings.high_scores.iter().enumerate() {
                let Some(lift) = drop_lift(t - 15.0 - i as f32 * 4.0, 60.0, 0.25) else { continue };
                let line = format!("{:>4}  {:<3}  {:06}", crate::lang::ordinal(lang, i + 1), h.initials, h.score);
                let color = match (menu.highlight == Some(i), i) {
                    (true, _) if (menu.tick / 8).is_multiple_of(2) => RED,
                    (true, _) | (false, 0) => NUMBERS,
                    _ => TEXT,
                };
                centred(&mut out, &layout, &line, top - 60.0 - i as f32 * 11.0 + lift, 1, color, depth);
            }
        }
    }

    // How to steer, for the device in hand; a line too long for the screen
    // goes on over two or more.
    let hint = hint_text(menu, page, lang);
    let room = layout.width - if page == Page::Title { 16.0 } else { 8.0 };
    let lines = wrap_hint(&hint, (room / 6.0).floor() as usize);
    for (i, line) in lines.iter().rev().enumerate() {
        let (x, color) = if page == Page::Title {
            (8.0, if view.light_menu { MUTED_LIGHT } else { MUTED_DARK })
        } else {
            (((layout.width - text_width(line, 1)) / 2.0).round(), MUTED_LIGHT)
        };
        text(&mut out, line, x, 4.0 + i as f32 * 11.0, 1, color, depth);
    }
    crate::render::hide_inner_faces(&mut out);
    (out, letters, camera)
}

/// The hint line of a page: how to steer, for the device in hand.
pub(super) fn hint_text(menu: &Menu, page: Page, lang: &str) -> String {
    let say = |en: &'static str| crate::lang::say(lang, en);
    let style = menu.pad_style;
    let with = |en: &'static str, parts: &[&str]| crate::lang::say_with(lang, en, parts);
    let (south, east) = (pad_label("south", style), pad_label("east", style));
    match (menu.device, menu.rebinding.is_some(), page) {
        (_, true, _) => with("ESC OR {}: CANCEL", &[&pad_label("back", style)]),
        (Device::Gamepad, _, Page::Initials) => with("UP/DOWN: LETTER   {}: NEXT   {}: BACK", &[&south, &east]),
        (Device::Keyboard, _, Page::Initials) => say("TYPE OR UP/DOWN   ENTER: NEXT   ESC: BACK").to_string(),
        (Device::Gamepad, _, Page::HighScores) => with("{}: CONTINUE", &[&south]),
        (Device::Keyboard, _, Page::HighScores) => say("ENTER: CONTINUE").to_string(),
        (Device::Gamepad, _, Page::Join) => with("UP/DOWN: SIGN   RIGHT: ONE MORE   {}: JOIN   {}: BACK", &[&south, &east]),
        (Device::Keyboard, _, Page::Join) => say("TYPE   ENTER: JOIN   ESC: BACK").to_string(),
        (Device::Gamepad, _, Page::Code) => with("UP/DOWN: SIGN   RIGHT: ONE MORE   {}: JOIN   {}: BACK", &[&south, &east]),
        (Device::Keyboard, _, Page::Code) => say("TYPE   ENTER: JOIN   ESC: BACK").to_string(),
        (Device::Gamepad, _, Page::Joining) => with("{}: CANCEL", &[&east]),
        (Device::Keyboard, _, Page::Joining) => say("ESC: CANCEL").to_string(),
        (Device::Gamepad, _, _) => with("{}: SELECT   {}: BACK", &[&south, &east]),
        (Device::Keyboard, _, _) => say("ENTER: SELECT   ESC: BACK").to_string(),
    }
}

/// A hint broken into lines of at most `cols` letters: at the gaps of two
/// spaces or more between its parts first, else at a single space; a word
/// longer than a line stays whole.
pub(super) fn wrap_hint(hint: &str, cols: usize) -> Vec<String> {
    let len = |s: &str| s.chars().count();
    if len(hint) <= cols {
        return vec![hint.to_string()];
    }
    let mut pieces: Vec<String> = Vec::new();
    for part in hint.split("  ").map(str::trim).filter(|p| !p.is_empty()) {
        if len(part) <= cols {
            pieces.push(part.to_string());
        } else {
            let mut words: Vec<String> = Vec::new();
            for word in part.split(' ') {
                match words.last_mut() {
                    Some(l) if len(l) + 1 + len(word) <= cols => {
                        l.push(' ');
                        l.push_str(word);
                    }
                    _ => words.push(word.to_string()),
                }
            }
            pieces.extend(words);
        }
    }
    let mut lines: Vec<String> = Vec::new();
    for piece in pieces {
        let joined = lines.last().map(|l| format!("{l}   {piece}"));
        match joined {
            Some(j) if len(&j) <= cols => *lines.last_mut().unwrap() = j,
            _ => lines.push(piece),
        }
    }
    lines
}

/// One big letter in the middle of a cell `cell` wide from `x`: the
/// display font's when the game has one, else the pixel font's at `scale`.
#[allow(clippy::too_many_arguments)]
pub(super) fn big_letter(out: &mut Vec<Instance>, c: char, x: f32, cell: f32, y: f32, scale: i32, color: [u8; 4], depth: i32, font: Option<&DisplayFont>, finish: Finish) {
    match font {
        Some(font) => {
            let g = font.glyph(c, 8 * scale as u32);
            let x = x + ((cell - g.width as f32) / 2.0).round();
            extrude(out, &g, x, y, color, depth * 2, Finish { seed: c as u32 * 7, ..finish });
        }
        None => text(out, &c.to_string(), x + ((cell - text_width("A", scale)) / 2.0).round(), y, scale, color, depth),
    }
}

/// Text in the middle of the screen's width.
#[allow(clippy::too_many_arguments)]
pub(super) fn centred(out: &mut Vec<Instance>, layout: &Layout, s: &str, y: f32, scale: i32, color: [u8; 4], depth: i32) {
    text(out, s, ((layout.width - text_width(s, scale)) / 2.0).round(), y, scale, color, depth);
}

/// A menu row: yellow and marked when selected, hopping a little.
#[allow(clippy::too_many_arguments)]
pub(super) fn row(out: &mut Vec<Instance>, view: &MenuView, i: usize, label: &str, x: f32, y: f32, scale: i32, depth: i32, now: f32) {
    // Over the bright sand of the title dark letters read, over a dark
    // title and the darkened pages light ones.
    let menu = view.menu;
    let (idle, active, mark) = match menu.page() {
        Some(Page::Title) if view.light_menu => TITLE_ROWS_LIGHT,
        Some(Page::Title) => TITLE_ROWS,
        _ => (TEXT, NUMBERS, RED),
    };
    if menu.cursor() != i {
        text(out, label, x, y, scale, idle, depth);
        return;
    }
    let since = now - menu.moved as f32;
    let hop = if since < 10.0 { (std::f32::consts::PI * since / 10.0).sin() * 2.0 } else { 0.0 };
    let y = y + hop.round();
    text(out, label, x, y, scale, active, depth);
    // The marker nudges towards the item.
    let nudge = ((now / 8.0).sin() * 1.5).round();
    text(out, ">", x - 9.0 * scale as f32 + nudge, y, scale, mark, depth);
}

/// A setting's value next to its row.
pub(super) fn setting(out: &mut Vec<Instance>, view: &MenuView, item: Item, x: f32, y: f32, selected: bool, depth: i32) {
    let s = view.settings;
    let lang = s.language();
    let flag = |on: bool| crate::lang::say(lang, if on { "ON" } else { "OFF" });
    let color = if selected { NUMBERS } else { TEXT };
    match item {
        Item::Music | Item::Sound => {
            let v = if item == Item::Music { s.music } else { s.sound };
            for i in 0..VOLUME_STEPS {
                let c = if i < v { color } else { SLIDER_EMPTY };
                for dy in 0..7 {
                    for dx in 0..3 {
                        for z in 0..depth {
                            voxel(out, x + i as f32 * 5.0 + dx as f32, y + dy as f32, z as f32, c);
                        }
                    }
                }
            }
        }
        Item::Song => {
            let name = view.songs.get(s.song).map(String::as_str).unwrap_or("-");
            let shown = if selected { format!("< {} >", name.to_uppercase()) } else { name.to_uppercase() };
            text(out, &shown, x, y, 1, color, depth);
        }
        Item::Fullscreen => text(out, flag(s.fullscreen), x, y, 1, color, depth),
        Item::Shake => text(out, flag(s.shake), x, y, 1, color, depth),
        Item::Flashes => text(out, flag(s.flashes), x, y, 1, color, depth),
        Item::Blur => text(out, flag(s.blur), x, y, 1, color, depth),
        Item::Rumble => text(out, flag(s.rumble), x, y, 1, color, depth),
        Item::Shadows => {
            let label = crate::lang::say(lang, s.shadows.label());
            let shown = if selected { format!("< {label} >") } else { label.to_string() };
            text(out, &shown, x, y, 1, color, depth);
        }
        Item::FrameRate => {
            let label = crate::lang::say(lang, s.frame_rate.label());
            let shown = if selected { format!("< {label} >") } else { label.to_string() };
            text(out, &shown, x, y, 1, color, depth);
        }
        Item::Language => {
            let name = crate::lang::name(lang);
            let shown = if selected { format!("< {name} >") } else { name };
            text(out, &shown, x, y, 1, color, depth);
        }
        Item::ShowFps => text(out, flag(s.show_fps), x, y, 1, color, depth),
        _ => {}
    }
}

/// What the network says, small in the middle, a line each (who went
/// quiet, who paused, how far joining is): at the top over the game, low
/// over a menu (its heading and the host's lines are at the top), from
/// `low` up.
pub fn notice(out: &mut Vec<Instance>, lines: &[String], cfg: &UiConfig, aspect: f32, low: Option<f32>) {
    let layout = Layout { width: (cfg.height * aspect).round(), height: cfg.height };
    let mut voxels = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let y = match low {
            Some(from) => from + (lines.len() - 1 - i) as f32 * 9.0,
            None => layout.height - 10.0 - i as f32 * 9.0,
        };
        centred(&mut voxels, &layout, line, y, 1, NUMBERS, cfg.depth);
    }
    crate::render::hide_inner_faces(&mut voxels);
    out.extend(voxels);
}

/// The frames a second, small in the bottom right corner, over the game
/// and the menus alike (`Settings::show_fps`).
pub fn fps(out: &mut Vec<Instance>, fps: u32, cfg: &UiConfig, aspect: f32) {
    let layout = Layout { width: (cfg.height * aspect).round(), height: cfg.height };
    let label = format!("{fps} FPS");
    let mut voxels = Vec::new();
    text(&mut voxels, &label, layout.width - 3.0 - text_width(&label, 1), 3.0, 1, TEXT, cfg.depth);
    crate::render::hide_inner_faces(&mut voxels);
    out.extend(voxels);
}

/// "PRESS START", blinking over a replay the title plays (attract mode),
/// `seconds` into it.
pub fn press_start(out: &mut Vec<Instance>, seconds: f32, cfg: &UiConfig, aspect: f32, lang: &str) {
    if (seconds * 2.0) as u32 % 2 == 1 {
        return;
    }
    let layout = Layout { width: (cfg.height * aspect).round(), height: cfg.height };
    let label = crate::lang::say(lang, "PRESS START");
    let mut voxels = Vec::new();
    text(&mut voxels, label, ((layout.width - text_width(label, 2)) / 2.0).round(), (layout.height * 0.22).round(), 2, NUMBERS, cfg.depth);
    crate::render::hide_inner_faces(&mut voxels);
    out.extend(voxels);
}

/// A dark see-through layer over the whole screen, behind the menu.
pub(super) fn dim(out: &mut Vec<Instance>, layout: &Layout, opacity: f32) {
    const SIZE: f32 = 24.0;
    let fx = [0, (opacity.clamp(0.0, 1.0) * 255.0) as u8, 0, 0];
    // Behind the letters (z below 0), reaching past the edges, as the
    // perspective shrinks what is further back.
    let z = -SIZE / 2.0 - 2.0;
    let mut y = -SIZE * 2.0;
    while y < layout.height + SIZE * 2.0 {
        let mut x = -SIZE * 2.0;
        while x < layout.width + SIZE * 2.0 {
            out.push(Instance { pos_size: [x, y, z, SIZE], color: DIM, fx, shade: [0; 4], turn: [0; 4] });
            x += SIZE;
        }
        y += SIZE;
    }
}
