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
    let layout = Layout::of(cfg, aspect);
    let camera = camera(&layout, cfg.slant, cfg.depth as f32);
    let mut out = Vec::new();
    let mut letters = Vec::new();
    let menu = view.menu;
    let Some(page) = menu.page() else { return (out, letters, camera) };
    // The engine's words in the player's language (`c.languages`).
    let lang = view.settings.language();
    let pen = Pen {
        view,
        layout,
        lang: lang.as_str(),
        depth: cfg.depth,
        t: (menu.tick - menu.opened) as f32 + alpha,
        now: menu.tick as f32 + alpha,
        finish: Finish::of(cfg),
    };
    heading(&mut out, &mut letters, &pen, page);
    match page {
        Page::Title => title_page(&mut out, &pen),
        Page::Pause | Page::Network => pause_page(&mut out, &pen, page),
        Page::Find => find_page(&mut out, &pen),
        Page::Join => join_page(&mut out, &pen),
        Page::Code => code_page(&mut out, &pen),
        Page::Joining => joining_page(&mut out, &pen),
        Page::Settings => settings_page(&mut out, &pen),
        Page::Audio | Page::Video => audio_video_page(&mut out, &pen, page),
        Page::Controls => controls_page(&mut out, &pen),
        Page::Players => players_page(&mut out, &pen),
        Page::PlayerControls(seat) => player_controls_page(&mut out, &pen, seat),
        Page::Save | Page::Load => slots_page(&mut out, &pen, page),
        Page::Initials => initials_page(&mut out, &pen),
        Page::HighScores => high_scores_page(&mut out, &pen),
    }
    hint_lines(&mut out, &pen, page);
    crate::render::hide_inner_faces(&mut out);
    (out, letters, camera)
}

/// What the menu's pages are drawn with: the view, the UI grid, the
/// player's language, the letters' depth, the ticks since the page opened
/// (`t`) and since the menu began (`now`), the big letters' finish.
struct Pen<'a> {
    view: &'a MenuView<'a>,
    layout: Layout,
    lang: &'a str,
    depth: i32,
    t: f32,
    now: f32,
    finish: Finish,
}

impl Pen<'_> {
    fn say(&self, en: &'static str) -> &'static str {
        crate::lang::say(self.lang, en)
    }

    fn top(&self) -> f32 {
        self.layout.height
    }

    fn text(&self, out: &mut Vec<Instance>, s: &str, x: f32, y: f32, color: [u8; 4]) {
        text(out, s, x, y, 1, color, self.depth);
    }

    fn centred(&self, out: &mut Vec<Instance>, s: &str, y: f32, scale: i32, color: [u8; 4]) {
        centred(out, &self.layout, s, y, scale, color, self.depth);
    }

    fn row(&self, out: &mut Vec<Instance>, i: usize, label: &str, x: f32, y: f32, scale: i32) {
        row(out, self.view, i, label, x, y, scale, self.depth, self.now);
    }
}

/// The title's logo and high score, or another page's dimming and its
/// heading dropping in.
fn heading(out: &mut Vec<Instance>, letters: &mut Vec<ModelDraw>, pen: &Pen, page: Page) {
    let (view, layout, top) = (pen.view, &pen.layout, pen.top());
    let menu = view.menu;
    if page == Page::Title {
        logo(out, letters, view, layout, pen.now, pen.finish);
        if view.scored {
            let score = crate::lang::say_with(pen.lang, "HI-SCORE {}", &[&format!("{:06}", view.settings.best())]);
            text(out, &score, layout.width - 3.0 - text_width(&score, 1), top - 10.0, 1, NUMBERS, pen.depth);
        }
    } else {
        dim(out, layout, 0.8);
        // A guest's own menu stops nothing: no PAUSED over it.
        let guest = page == Page::Pause && menu.net.as_ref().is_some_and(|n| !n.hosting);
        let heading = if guest { "MENU" } else { page.heading() };
        dropping(out, pen.say(heading), layout, None, top - 38.0, 3, TEXT, pen.depth, pen.t, view.font, pen.finish);
    }
}

/// Big items low on the left, clear of what lies in the scene: the last
/// always where it was, a fourth (CONTINUE) above.
fn title_page(out: &mut Vec<Instance>, pen: &Pen) {
    let items = pen.view.menu.items(Page::Title, pen.view.settings);
    let first = 28.0 + (items.len() as f32 - 1.0) * 17.0;
    for (i, item) in items.iter().enumerate() {
        pen.row(out, i, &item.label_in(pen.lang), 24.0, first - i as f32 * 17.0, 2);
    }
}

/// The pause page (a network game's with who plays and who knocks) or
/// NETWORK's.
fn pause_page(out: &mut Vec<Instance>, pen: &Pen, page: Page) {
    let (menu, layout, top, lang) = (pen.view.menu, &pen.layout, pen.top(), pen.lang);
    let items = menu.items(page, pen.view.settings);
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
        pen.row(out, i, &label, x, top - 70.0 - i as f32 * apart, scale);
    }
    // Where the game is found, small at the top left (one line above the
    // heading's), who plays at the top right, how to let others in under
    // the items.
    if page == Page::Pause
        && let Some(net) = &menu.net
    {
        for (i, line) in net.lines.iter().enumerate() {
            pen.text(out, line, 6.0, top - 10.0 - i as f32 * 9.0, MUTED_LIGHT);
        }
        for (i, line) in net.who.iter().enumerate() {
            pen.text(out, line, layout.width - 6.0 - text_width(line, 1), top - 10.0 - i as f32 * 9.0, MUTED_LIGHT);
        }
        for (i, line) in net.help.iter().rev().enumerate() {
            pen.centred(out, line, 14.0 + i as f32 * 9.0, 1, MUTED_LIGHT);
        }
    }
}

/// The games found on the LAN, another game's greyed; typing an address
/// instead.
fn find_page(out: &mut Vec<Instance>, pen: &Pen) {
    let (menu, top) = (pen.view.menu, pen.top());
    let items = menu.items(Page::Find, pen.view.settings);
    let x0 = (pen.layout.width / 2.0 - 120.0).round();
    for (i, &item) in items.iter().enumerate() {
        let y = top - 66.0 - i as f32 * 13.0;
        match item {
            Item::Found(n) if !menu.found[n].same => pen.text(out, &menu.found[n].line, x0, y, MUTED_LIGHT),
            Item::Found(n) => pen.row(out, i, &menu.found[n].line, x0, y, 1),
            other => pen.row(out, i, &other.label_in(pen.lang), x0, y, 1),
        }
    }
    if menu.found.is_empty() {
        pen.centred(out, pen.say("SEARCHING THE NETWORK ..."), top - 52.0, 1, MUTED_LIGHT);
        pen.centred(out, pen.say("NONE FOUND? TYPE THE HOST'S ADDRESS"), 27.0, 1, MUTED_LIGHT);
        pen.centred(out, pen.say("(A FIREWALL MAY HIDE IT)"), 18.0, 1, MUTED_LIGHT);
    }
}

/// The host's address being typed, big, the cursor blinking.
fn join_page(out: &mut Vec<Instance>, pen: &Pen) {
    let (menu, top) = (pen.view.menu, pen.top());
    pen.centred(out, pen.say("THE HOST'S ADDRESS (ITS PAUSE PAGE SAYS IT)"), top - 62.0, 1, MUTED_LIGHT);
    let shown = format!("{}{}", menu.address, if (menu.tick / 20).is_multiple_of(2) { "_" } else { " " });
    let scale = if text_width(&shown, 3) > pen.layout.width - 8.0 { 2 } else { 3 };
    pen.centred(out, &shown, top - 96.0, scale, NUMBERS);
}

/// A game's code being typed: six places, the cursor blinking in the next.
fn code_page(out: &mut Vec<Instance>, pen: &Pen) {
    let (menu, top) = (pen.view.menu, pen.top());
    pen.centred(out, pen.say("THE GAME'S CODE (THE HOST SENDS IT)"), top - 62.0, 1, MUTED_LIGHT);
    let blink = (menu.tick / 20).is_multiple_of(2);
    let shown: String = (0..matchmaker_protocol::LENGTH)
        .map(|i| match menu.code.chars().nth(i) {
            Some(c) => c,
            None if i == menu.code.len() && blink => '_',
            None => '.',
        })
        .collect();
    pen.centred(out, &shown, top - 96.0, 3, NUMBERS);
}

fn joining_page(out: &mut Vec<Instance>, pen: &Pen) {
    let lines = pen.view.menu.net.as_ref().map_or(&[][..], |n| &n.lines[..]);
    for (i, line) in lines.iter().enumerate() {
        pen.centred(out, line, pen.top() - 70.0 - i as f32 * 12.0, 1, if i == 0 { NUMBERS } else { TEXT });
    }
}

/// The settings' own page as the pause page: big rows in the middle,
/// LANGUAGE with its value (small where it would not fit).
fn settings_page(out: &mut Vec<Instance>, pen: &Pen) {
    let width = pen.layout.width;
    for (i, &item) in pen.view.menu.items(Page::Settings, pen.view.settings).iter().enumerate() {
        let mut label = item.label_in(pen.lang);
        if item == Item::Language {
            label = format!("{label} < {} >", crate::lang::name(pen.lang));
        }
        let scale = if text_width(&label, 2) > width - 8.0 { 1 } else { 2 };
        let x = ((width - text_width(&label, scale)) / 2.0).round();
        pen.row(out, i, &label, x, pen.top() - 70.0 - i as f32 * 17.0, scale);
    }
}

/// A few rows each, roomy under the heading.
fn audio_video_page(out: &mut Vec<Instance>, pen: &Pen, page: Page) {
    let menu = pen.view.menu;
    let x0 = (pen.layout.width / 2.0 - 100.0).round();
    for (i, &item) in menu.items(page, pen.view.settings).iter().enumerate() {
        let y = pen.top() - 58.0 - i as f32 * 13.0;
        pen.row(out, i, &item.label_in(pen.lang), x0, y, 1);
        setting(out, pen.view, item, x0 + 120.0, y, menu.cursor() == i, pen.depth);
    }
}

/// The KEYBOARD and GAMEPAD heads of a page of bindings from `x0`, and
/// where their columns are.
fn binding_heads(out: &mut Vec<Instance>, pen: &Pen, x0: f32) -> (f32, f32) {
    let (kx, px) = (x0 + 70.0, x0 + 160.0);
    pen.text(out, pen.say("KEYBOARD"), kx, pen.top() - 56.0, MUTED_LIGHT);
    pen.text(out, pen.say("GAMEPAD"), px, pen.top() - 56.0, MUTED_LIGHT);
    (kx, px)
}

/// Whether `action` is being bound: PRESS A KEY OR BUTTON blinking in its
/// row then.
fn rebinding(out: &mut Vec<Instance>, pen: &Pen, action: &str, kx: f32, y: f32) -> bool {
    let menu = pen.view.menu;
    if menu.rebinding != Some(action) {
        return false;
    }
    if (menu.tick / 15).is_multiple_of(2) {
        pen.text(out, pen.say("PRESS A KEY OR BUTTON"), kx, y, NUMBERS);
    }
    true
}

/// A game of one's keys and buttons.
fn controls_page(out: &mut Vec<Instance>, pen: &Pen) {
    let (view, top) = (pen.view, pen.top());
    let menu = view.menu;
    let x0 = (pen.layout.width / 2.0 - 110.0).round();
    let (kx, px) = binding_heads(out, pen, x0);
    let items = menu.items(Page::Controls, view.settings);
    // Down to above the hint at the bottom, closer where there are many.
    let apart = ((top - 70.0 - 30.0) / (items.len().max(2) - 1) as f32).floor().min(12.0);
    for (i, &item) in items.iter().enumerate() {
        let y = top - 70.0 - i as f32 * apart;
        pen.row(out, i, &item.label_in(pen.lang), x0, y, 1);
        setting(out, view, item, kx, y, menu.cursor() == i, pen.depth);
        let Item::Bind(action) = item else { continue };
        if rebinding(out, pen, action, kx, y) {
            continue;
        }
        let keys = view.settings.keys.get(action).first().map(|k| view.settings.key_label(k)).unwrap_or_default();
        let pads = view.settings.pad.get(action).first().map(|b| pad_label(b, menu.pad_style)).unwrap_or_default();
        pen.text(out, &keys, kx, y, TEXT);
        pen.text(out, &pads, px, y, TEXT);
    }
    let y = top - 70.0 - items.len() as f32 * apart - 4.0;
    pen.text(out, pen.say("MOVE: ARROWS OR WASD, LEFT STICK OR D-PAD"), x0, y, MUTED_LIGHT);
}

/// A heading for each seat with its three rows under it, then SWAP, RUMBLE
/// and BACK; scrolled so the chosen row stays in view.
fn players_page(out: &mut Vec<Instance>, pen: &Pen) {
    let (view, top, lang) = (pen.view, pen.top(), pen.lang);
    let menu = view.menu;
    let x0 = (pen.layout.width / 2.0 - 110.0).round();
    let vx = x0 + 100.0;
    let items = menu.items(Page::Players, view.settings);
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
        pen.text(out, &head, x0, y, NUMBERS);
        // A seat nobody sits on: START takes it.
        if !menu.seats.get(n - 1).is_some_and(|s| s.taken) {
            pen.text(out, pen.say("PRESS START"), vx, y, MUTED_LIGHT);
        }
    }
    for (i, (&item, &y)) in items.iter().zip(&rows).enumerate() {
        let y = y + shift;
        if !shown(y) {
            continue;
        }
        let indent = if matches!(item, Item::SeatKeys(_) | Item::SeatPad(_) | Item::SeatControls(_)) { 10.0 } else { 0.0 };
        pen.row(out, i, &item.label_in(lang), x0 + indent, y, 1);
        setting(out, view, item, vx, y, menu.cursor() == i, pen.depth);
        let value = match item {
            Item::SeatKeys(n) => pen.say(menu.seats.get(n - 1).map_or(crate::seating::Keys::Nothing, |s| s.keyboard).label()).to_string(),
            Item::SeatPad(n) => match menu.seats.get(n - 1).map(|s| s.pad.as_str()) {
                Some(name) if !name.is_empty() => name.to_uppercase().chars().take(24).collect(),
                _ => pen.say("NONE").to_string(),
            },
            _ => continue,
        };
        let selected = menu.cursor() == i;
        pen.text(out, &chosen(&value, selected), vx, y, if selected { NUMBERS } else { TEXT });
    }
}

/// A seat's own keys and buttons.
fn player_controls_page(out: &mut Vec<Instance>, pen: &Pen, seat: usize) {
    let (view, top) = (pen.view, pen.top());
    let menu = view.menu;
    let part = menu.seats.get(seat - 1).map_or(crate::seating::Keys::Nothing, |s| s.keyboard);
    let head = crate::lang::say_with(pen.lang, "PLAYER {}", &[&seat.to_string()]);
    pen.centred(out, &head, top - 46.0, 1, NUMBERS);
    let x0 = (pen.layout.width / 2.0 - 110.0).round();
    let (kx, px) = binding_heads(out, pen, x0);
    let items = menu.items(Page::PlayerControls(seat), view.settings);
    // Down to above the hint at the bottom, closer where there are many.
    let apart = ((top - 66.0 - 24.0) / (items.len().max(2) - 1) as f32).floor().min(12.0);
    let keys = view.settings.keys_of(part);
    let pad = view.settings.pad_of(seat);
    for (i, &item) in items.iter().enumerate() {
        let y = top - 66.0 - i as f32 * apart;
        pen.row(out, i, &item.label_in(pen.lang), x0, y, 1);
        let Item::Bind(action) = item else { continue };
        if rebinding(out, pen, action, kx, y) {
            continue;
        }
        let key = keys.map_or_else(|| "-".to_string(), |b| b.get(action).first().map(|k| view.settings.key_label(k)).unwrap_or_default());
        let button = if crate::seating::DIRECTIONS.contains(&action) { String::new() } else { pad.get(action).first().map(|b| pad_label(b, menu.pad_style)).unwrap_or_default() };
        pen.text(out, &key, kx, y, TEXT);
        pen.text(out, &button, px, y, TEXT);
    }
    // How it moves where its directions are no rows of their own.
    if !matches!(part, crate::seating::Keys::Left | crate::seating::Keys::Right) {
        let y = top - 66.0 - items.len() as f32 * apart - 4.0;
        let moving = if part == crate::seating::Keys::Whole { "MOVE: ARROWS OR WASD, LEFT STICK OR D-PAD" } else { "MOVE: LEFT STICK OR D-PAD" };
        pen.text(out, pen.say(moving), x0, y, MUTED_LIGHT);
    }
}

/// The run's save slots, to save into or load from.
fn slots_page(out: &mut Vec<Instance>, pen: &Pen, page: Page) {
    let menu = pen.view.menu;
    let x0 = (pen.layout.width / 2.0 - 100.0).round();
    for (i, &item) in menu.items(page, pen.view.settings).iter().enumerate() {
        let label = match item {
            Item::Slot(n) => menu.slots[n].clone().unwrap_or_else(|| format!("{}  {}", n + 1, pen.say("EMPTY"))),
            other => other.label_in(pen.lang),
        };
        pen.row(out, i, &label, x0, pen.top() - 70.0 - i as f32 * 14.0, 1);
    }
}

/// A run's initials being picked: three big letters, the one being picked
/// blinking and hopping when it changes, those to come waiting as lines.
fn initials_page(out: &mut Vec<Instance>, pen: &Pen) {
    let (view, top, lang, depth) = (pen.view, pen.top(), pen.lang, pen.depth);
    let menu = view.menu;
    let e = &menu.entry;
    let line = crate::lang::say_with(lang, "{} PLACE   SCORE {}", &[&crate::lang::ordinal(lang, e.rank + 1), &format!("{:06}", e.score)]);
    pen.centred(out, &line, top - 58.0, 1, NUMBERS);
    pen.centred(out, pen.say("ENTER YOUR INITIALS"), top - 74.0, 1, MUTED_LIGHT);
    // Whose, where several played.
    if let Some(seat) = e.seat {
        pen.centred(out, &crate::lang::say_with(lang, "PLAYER {}", &[&seat.to_string()]), top - 48.0, 1, TEXT);
    }
    let (scale, cell) = (5, 40.0);
    let x0 = (pen.layout.width / 2.0 - cell * 1.5).round();
    for (k, &c) in e.letters.iter().enumerate() {
        let x = x0 + k as f32 * cell;
        if k > e.at {
            for bx in 6..cell as i32 - 6 {
                for by in 36..39 {
                    for z in 0..depth {
                        voxel(out, x + bx as f32, by as f32, z as f32, MUTED_LIGHT);
                    }
                }
            }
            continue;
        }
        let color = if k < e.at || (menu.tick / 10).is_multiple_of(2) { TEXT } else { RED };
        let since = pen.now - menu.moved as f32;
        let hop = if k == e.at && since < 10.0 { ((std::f32::consts::PI * since / 10.0).sin() * 3.0).round() } else { 0.0 };
        big_letter(out, c, x, cell, 36.0 + hop, scale, color, depth, view.font, pen.finish);
    }
}

/// The lines drop in one after another, the new one blinking.
fn high_scores_page(out: &mut Vec<Instance>, pen: &Pen) {
    let menu = pen.view.menu;
    for (i, h) in pen.view.settings.high_scores.iter().enumerate() {
        let Some(lift) = drop_lift(pen.t - 15.0 - i as f32 * 4.0, 60.0, 0.25) else { continue };
        let line = format!("{:>4}  {:<3}  {:06}", crate::lang::ordinal(pen.lang, i + 1), h.initials, h.score);
        let color = match (menu.highlight == Some(i), i) {
            (true, _) if (menu.tick / 8).is_multiple_of(2) => RED,
            (true, _) | (false, 0) => NUMBERS,
            _ => TEXT,
        };
        pen.centred(out, &line, pen.top() - 60.0 - i as f32 * 11.0 + lift, 1, color);
    }
}

/// How to steer, for the device in hand; a line too long for the screen
/// goes on over two or more.
fn hint_lines(out: &mut Vec<Instance>, pen: &Pen, page: Page) {
    let layout = &pen.layout;
    let hint = hint_text(pen.view.menu, page, pen.lang);
    let room = layout.width - if page == Page::Title { 16.0 } else { 8.0 };
    let lines = wrap_hint(&hint, (room / 6.0).floor() as usize);
    for (i, line) in lines.iter().rev().enumerate() {
        let (x, color) = if page == Page::Title {
            (8.0, if pen.view.light_menu { MUTED_LIGHT } else { MUTED_DARK })
        } else {
            (((layout.width - text_width(line, 1)) / 2.0).round(), MUTED_LIGHT)
        };
        pen.text(out, line, x, 4.0 + i as f32 * 11.0, color);
    }
}

/// A chosen setting's value between arrows (`< ON >`), another's as it is.
fn chosen(value: &str, selected: bool) -> String {
    if selected { format!("< {value} >") } else { value.to_string() }
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
    let flag = |on: bool| crate::lang::say(&lang, if on { "ON" } else { "OFF" });
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
            text(out, &chosen(&name.to_uppercase(), selected), x, y, 1, color, depth);
        }
        Item::Fullscreen => text(out, flag(s.fullscreen), x, y, 1, color, depth),
        Item::Shake => text(out, flag(s.shake), x, y, 1, color, depth),
        Item::Flashes => text(out, flag(s.flashes), x, y, 1, color, depth),
        Item::Blur => text(out, flag(s.blur), x, y, 1, color, depth),
        Item::Rumble => text(out, flag(s.rumble), x, y, 1, color, depth),
        Item::Shadows => {
            text(out, &chosen(crate::lang::say(&lang, s.shadows.label()), selected), x, y, 1, color, depth);
        }
        Item::FrameRate => {
            text(out, &chosen(crate::lang::say(&lang, s.frame_rate.label()), selected), x, y, 1, color, depth);
        }
        Item::Language => {
            text(out, &chosen(&crate::lang::name(&lang), selected), x, y, 1, color, depth);
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
    let layout = Layout::of(cfg, aspect);
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
    let layout = Layout::of(cfg, aspect);
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
    let layout = Layout::of(cfg, aspect);
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
