//! The engine's menus as drawn: pages of bindings whatever the number of
//! actions.

use super::*;
use crate::controls::{Bindings, Nav, PadStyle};
use crate::menu::SeatLine;

/// Settings for a game of `actions`, each bound to a key and a button.
fn settings_for(actions: &[&'static str]) -> Settings {
    let mut s = Settings::default();
    let keys: Vec<(&str, &[&str])> = actions.iter().map(|&a| (a, &["Z"][..])).collect();
    let pad: Vec<(&str, &[&str])> = actions.iter().map(|&a| (a, &["south"][..])).collect();
    s.adopt(actions, &Bindings::new(&keys), &Bindings::new(&pad));
    s
}

/// `n` actions by name (`a01`, `a02` ...).
fn actions(n: usize) -> Vec<&'static str> {
    (1..=n).map(|i| crate::names::intern(&format!("a{i:02}"))).collect()
}

fn view<'a>(menu: &'a Menu, settings: &'a Settings, pointer_stick: Option<crate::config::Stick>) -> MenuView<'a> {
    MenuView { menu, settings, logo: &[], drip: None, scored: false, font: None, logo_heights: &[], logo_cracks: &[], logo_bank: &[], logo_model: None, logo_bob: 0.0, light_menu: false, pointer_stick }
}

/// The rows drawn, from the top: each item's index and box (from the
/// HUD's top left).
fn rows(menu: &Menu, settings: &Settings) -> Vec<RowBox> {
    let mut rows = menu_rows(&view(menu, settings, Some(crate::config::Stick::Left)), &UiConfig::default(), 16.0 / 9.0);
    rows.sort_by(|a, b| a.2.total_cmp(&b.2));
    rows
}

/// A digest of the voxels a page draws.
fn digest(m: &Menu, settings: &Settings, pointer_stick: Option<crate::config::Stick>) -> u64 {
    let (cubes, ..) = menu(&view(m, settings, pointer_stick), &UiConfig::default(), 0.0, 16.0 / 9.0);
    let mut h = crate::hash::Stable::default();
    h.bytes(bytemuck::cast_slice(&cubes));
    h.finish()
}

/// A page of bindings with few actions draws the voxels it drew before
/// pages of many scrolled (pinned from the code before): More Blood's
/// four, Voxkeep's three, BunnyFarm's five (rows closer, 10 apart), a pad
/// of other letters, a second player's half and the players page of two.
#[test]
fn a_page_of_few_bindings_draws_as_before() {
    let mut drawn = Vec::new();
    for (names, style) in [(&["fire", "grenade", "jump", "strafe"][..], PadStyle::Xbox), (&["jump", "restart", "interact"][..], PadStyle::Xbox), (&["act", "back", "prev", "next", "bag"][..], PadStyle::Xbox), (&["fire", "grenade", "jump", "strafe"][..], PadStyle::Nintendo)] {
        let settings = settings_for(names);
        let menu = Menu { stack: vec![(Page::Controls, 2)], pad_style: style, ..Menu::default() };
        drawn.push(digest(&menu, &settings, None));
    }
    let settings = settings_for(&["fire", "bomb"]);
    let seats = vec![SeatLine { keyboard: crate::seating::Keys::Left, pad: String::new(), taken: true }, SeatLine { keyboard: crate::seating::Keys::Right, pad: "XBOX CONTROLLER".into(), taken: false }];
    let menu = Menu { stack: vec![(Page::Players, 4)], seats: seats.clone(), ..Menu::default() };
    drawn.push(digest(&menu, &settings, None));
    let menu = Menu { stack: vec![(Page::PlayerControls(2), 5)], seats, ..Menu::default() };
    drawn.push(digest(&menu, &settings, Some(crate::config::Stick::Right)));
    assert_eq!(drawn, [15925080607135474730, 5918019879425428388, 13802446346191162255, 13936269840824952929, 4838706849178853975, 9705921337843865268]);
}

/// The CONTROLS page of a game of one with `n` actions, its cursor at
/// `at`.
fn controls(n: usize, at: usize) -> (Menu, Settings) {
    (Menu { stack: vec![(Page::Controls, at)], ..Menu::default() }, settings_for(&actions(n)))
}

/// With 15 or 32 actions (and every row a page of bindings has) the rows
/// shown are `LEAST_APART` apart, a letter and 3 clear, never on each
/// other, all on the screen between the columns' heads and the lines
/// under them, as far from the last row shown as the heads from the first;
/// the marks say which way more lie.
#[test]
fn many_bindings_scroll_and_never_overlap() {
    let top = UiConfig::default().height;
    for n in [15, 32] {
        for at in [0, n / 2, n + 3] {
            let (menu, settings) = controls(n, at);
            let drawn = rows(&menu, &settings);
            assert_eq!(drawn.len(), 8, "{n} actions: the rows that fit over MOVE and POINTER");
            for pair in drawn.windows(2) {
                assert_eq!(pair[1].2 - pair[0].2, LEAST_APART, "{n} actions at {at}: {drawn:?}");
                assert_eq!(pair[1].0, pair[0].0 + 1, "the rows in order");
            }
            assert!(drawn.iter().all(|r| r.2 >= 0.0 && r.2 + r.4 <= top), "{drawn:?}");
            let pen = Pen { view: &view(&menu, &settings, Some(crate::config::Stick::Left)), layout: Layout::of(&UiConfig::default(), 16.0 / 9.0), lang: "en", depth: 2, t: 0.0, now: 0.0, finish: Finish::default() };
            let bound = bound_rows(&pen, Page::Controls, n + 4, top - 70.0, 40.0, top - 56.0);
            let shown: Vec<f32> = bound.ys.iter().flatten().copied().collect();
            assert_eq!((shown[0], shown[7]), (top - 70.0, 40.0));
            assert_eq!((top - 56.0) - shown[0], shown[7] - bound.foot, "the heads as far over the first row as the lines under the last");
            assert!(bound.foot - 10.0 >= 4.0 + 9.0, "POINTER over the hint");
            let first = drawn[0].0;
            assert_eq!(bound.above.is_some(), first > 0, "{n} at {at}");
            assert_eq!(bound.below.is_some(), first + 8 < n + 4, "{n} at {at}");
        }
    }
}

/// The cursor walks down through every row of a long page and round from
/// the last to the first, and up again round the other way: the row chosen
/// is always shown, the rows stay put while it moves among those shown
/// (a step moves them one row at most), and at the first row the page is
/// as it opened.
#[test]
fn the_cursor_walks_through_every_binding_in_view() {
    let (mut menu, mut settings) = controls(32, 0);
    let n = 36;
    let opened = rows(&menu, &settings);
    let mut before = opened.clone();
    for nav in [Nav::Down, Nav::Up] {
        for _ in 0..2 * n {
            menu.nav(nav, &mut settings);
            let drawn = rows(&menu, &settings);
            assert!(drawn.iter().any(|r| r.0 == menu.cursor()), "row {} chosen and not shown: {drawn:?}", menu.cursor());
            let (was, now) = (before[0].0, drawn[0].0);
            assert!(was.abs_diff(now) <= 1 || menu.cursor() == 0 || menu.cursor() == n - 1, "a step scrolls one row at most: {was} to {now}");
            before = drawn;
        }
        menu.stack[0].1 = 0;
        assert_eq!(rows(&menu, &settings), opened);
    }
    // While a binding is made its row is shown.
    let (mut menu, mut settings) = controls(32, 30);
    menu.nav(Nav::Confirm, &mut settings);
    assert_eq!(menu.rebinding, Some(actions(32)[30]));
    assert!(rows(&menu, &settings).iter().any(|r| r.0 == 30));
}

/// The pointer over a row chosen after scrolling is that row, and choosing
/// it moves no row: the rows under a still pointer stay.
#[test]
fn the_pointer_picks_the_binding_drawn_where_it_is() {
    let (mut menu, settings) = controls(15, 17);
    let drawn = rows(&menu, &settings);
    assert!(drawn[0].0 > 0, "scrolled");
    for &(i, x, y, w, h) in &drawn {
        let (px, py) = (x + w / 2.0, y + h / 2.0);
        let under = drawn.iter().find(|&&(_, rx, ry, rw, rh)| px >= rx && py >= ry && px < rx + rw && py < ry + rh).map(|r| r.0);
        assert_eq!(under, Some(i));
        menu.choose(i);
        assert_eq!(rows(&menu, &settings), drawn, "choosing {i} under the pointer moved the rows");
    }
}

/// The players page of four seats scrolls as the CONTROLS page does: the
/// row chosen in view, the rows still under the pointer, at the first row
/// as it opened.
#[test]
fn the_players_page_of_four_scrolls_with_the_cursor() {
    let mut settings = settings_for(&["fire"]);
    let seats = vec![SeatLine { keyboard: crate::seating::Keys::Left, pad: String::new(), taken: true }; 4];
    let mut menu = Menu { stack: vec![(Page::Players, 0)], seats, ..Menu::default() };
    let opened = rows(&menu, &settings);
    let n = menu.items(Page::Players, &settings).len();
    assert!(opened.len() < n, "four seats do not fit");
    for _ in 0..n {
        menu.nav(Nav::Down, &mut settings);
        let drawn = rows(&menu, &settings);
        assert!(drawn.iter().any(|r| r.0 == menu.cursor()), "{}", menu.cursor());
        let at = menu.cursor();
        menu.choose(drawn[drawn.len() / 2].0);
        assert_eq!(rows(&menu, &settings), drawn, "the pointer's row stays");
        menu.choose(at);
    }
    menu.stack[0].1 = 0;
    assert_eq!(rows(&menu, &settings), opened);
}
