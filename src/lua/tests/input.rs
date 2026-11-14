//! The keys: `controls`, `input`, the actions and what replays carry of
//! them.

use super::*;

/// `controls` is what the player plays with, there everywhere: the device
/// used last and how it labels an action's key (the keyboard by its
/// layout, the pad by its letters), in a text as the action in braces too;
/// nobody plays until a window or a replay says who does. An action the
/// game lacks is named, and so is a brace left open.
#[test]
fn controls_name_the_keys_in_hand() {
    use crate::controls::{Bindings, Device, PadStyle};
    let main = "function on_tick() world.state.seen = controls.device or 'nobody' end\n\
                function on_draw(frame) frame:text((controls.device or 'nobody') .. ':' .. controls:key('fire') .. ':{pause}:{{', 0, 0) end\n";
    let mut g = game("keys-in-hand", main);
    let shown = |g: &Session<LuaGame>| {
        g.compose(0.0, 1.6, Scene::Play, None);
        first_text(g)
    };
    assert_eq!(shown(&g), "nobody:::{");
    let mut settings = crate::settings::Settings::default();
    settings.keys = Bindings::new(&[("fire", &["Z"]), ("pause", &["Escape"])]);
    settings.pad = Bindings::new(&[("fire", &["east"]), ("pause", &["start"])]);
    settings.key_names.insert("Z".into(), "Y".into());
    g.set_controls(g.player_controls(Device::Keyboard, PadStyle::Xbox, &settings));
    assert_eq!(shown(&g), "keyboard:Y:ESCAPE:{", "a German keyboard's Y where the binding says Z");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "seen"), format!("{:?}", crate::lua::Value::Str(b"keyboard".to_vec())), "on_tick sees it too");
    g.set_controls(g.player_controls(Device::Gamepad, PadStyle::PlayStation, &settings));
    assert_eq!(shown(&g), "gamepad:CIRCLE:OPTIONS:{");
    for (name, line, says) in [
        ("keys-unknown", "frame:text(controls:key('jumpp'), 0, 0)", "controls:key: no action 'jumpp'"),
        ("keys-unknown-text", "frame:text('PRESS {jumpp}', 0, 0)", "frame:text: no action 'jumpp'"),
        ("keys-open-brace", "frame:text('PRESS {fire', 0, 0)", "frame:text: a '{' without its '}'"),
    ] {
        let g = game(name, &format!("function on_draw(frame) {line} end\n"));
        g.compose(0.0, 1.6, Scene::Play, None);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
}

/// A replay carries what the player played with, from the step it
/// changed on: a game that goes by `controls` plays back the same (the
/// guard's second played twice included), and would not without them.
#[test]
fn a_replay_carries_the_controls() {
    use crate::controls::{Device, PadStyle};
    use crate::replay::{Recorder, Replay, Verdict, play_through};
    let main = "function on_world_start() world.state.pad = 0 end\n\
                function on_tick() if controls.device == 'gamepad' then world.state.pad = world.state.pad + 1 end end\n";
    let dir = folder("controls-replay", main);
    let mut g = game_in(&dir);
    let mut settings = crate::settings::Settings::default();
    settings.adopt(g.actions(), &g.default_keys(), &g.default_pad());
    let mut recorder = Recorder::default();
    for tick in 0..360 {
        let device = if (120..320).contains(&tick) { Device::Gamepad } else { Device::Keyboard };
        g.set_controls(g.player_controls(device, PadStyle::Xbox, &settings));
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(state(&mut g, "pad"), format!("{:?}", crate::lua::Value::Int(200)));
    let replay = recorder.replay(&g).unwrap();
    assert_eq!(replay.controls.iter().map(|c| c.at).collect::<Vec<_>>(), [0, 120, 320]);
    let path = dir.join("controls.toml");
    replay.save(&path).unwrap();
    let back = Replay::load(&path).unwrap();
    assert_eq!(back, replay, "written and read back");
    assert_eq!(play_through::<LuaGame>(&dir, &back).unwrap(), Verdict::Same);
    let without = Replay { controls: Vec::new(), ..back };
    assert!(matches!(play_through::<LuaGame>(&dir, &without).unwrap(), Verdict::RunsApart(_)), "nobody played the pad");
}

/// `input:repeated` is true as a key goes down and again while it is held
/// (after 20 ticks, every 6); `frame:menu` and `frame:grid` are read and
/// checked (rows as texts or tables, keys in braces filled, the chosen row
/// from 1) and drawn, a long list scrolled; a mistake names what is wanted.
#[test]
fn a_game_draws_menus_and_repeats_keys() {
    let main = r##"
function on_world_start() world.state.down = 0 world.state.act = 0 end
function on_tick()
  if input:repeated("down") then world.state.down = world.state.down + 1 end
  if input:repeated("fire") then world.state.act = world.state.act + 1 end
end
function on_draw(frame)
  local rows = {}
  for i = 1, 10 do rows[i] = { text = "row " .. i, value = i * 3, dim = i == 2 } end
  rows[1] = "Plain"
  frame:menu{ title = "Market", tabs = { "Buy", "Sell" }, tab = 2, rows = rows, at = 7, shown = 4, hint = "{fire} buys" }
  frame:grid{ cells = { { count = 3 }, false, { dim = true } }, at = 2, columns = 2 }
end
"##;
    let mut g = game("menus", main);
    let down = Input { down: true, ..Input::default() };
    for _ in 0..40 {
        g.step(&down);
    }
    let mut fire = Input::default();
    fire.hold(1, true);
    g.step(&fire);
    assert_eq!((state(&mut g, "down"), state(&mut g, "act")), ("Int(5)".into(), "Int(1)".into()), "at 1, 21, 27, 33 and 39 ticks held; a press");
    g.compose(0.0, 16.0 / 9.0, crate::session::Scene::Play, None);
    assert_eq!(halted(&g), "");
    let overlay = g.game.drawn.borrow();
    let menu = overlay.iter().find_map(|c| match c { crate::lua::world::Cmd::Menu(m) => Some(m.clone()), _ => None }).expect("a menu");
    assert_eq!((menu.rows.len(), menu.at, menu.tab, menu.shown), (10, Some(6), 1, 4));
    assert_eq!((menu.rows[0].text.as_str(), menu.rows[1].dim, menu.rows[2].value.as_deref()), ("Plain", true, Some("9")));
    assert!(menu.hint.as_deref().is_some_and(|h| h.ends_with(" buys") && !h.contains('{')), "{:?}", menu.hint);
    let grid = overlay.iter().find_map(|c| match c { crate::lua::world::Cmd::Grid(g) => Some(g.clone()), _ => None }).expect("a grid");
    assert_eq!((grid.cells.len(), grid.at, grid.cells[0].count, grid.cells[1].icon.is_none(), grid.cells[2].dim), (3, Some(1), Some(3), true, true));
    drop(overlay);
    let ui = crate::config::UiConfig::default();
    assert!(g.game.hud(&ui, 0.0, 16.0 / 9.0, None).len() > 500, "drawn");
    for (bad, want) in [
        ("frame:menu{ rows = { \"a\" }, at = 2 }", "frame:menu: at = 2, but there are 1 (from 1)"),
        ("frame:menu{ rows = { \"a\" }, colour = 1 }", "frame:menu: no 'colour'"),
        ("frame:menu{ rows = { \"a\", 7 } }", "rows[2] is a text or"),
        ("frame:grid{ cells = { 1 } }", "cells[1] is { icon =, count =, dim = }"),
    ] {
        let g = game("menus-bad", &format!("function on_draw(frame) {bad} end\n"));
        g.compose(0.0, 16.0 / 9.0, crate::session::Scene::Play, None);
        assert!(halted(&g).contains(want), "{bad}: {}", halted(&g));
    }
}

/// The directions are pressed and released like actions: one step a press.
#[test]
fn directions_are_pressed_like_actions() {
    let main = "function on_tick()\n  if input:pressed('right') then world.state.steps = (world.state.steps or 0) + 1 end\n  if input:released('right') then world.state.ups = (world.state.ups or 0) + 1 end\n  world.state.held = input:held('right')\nend\n";
    let mut g = game("directions", main);
    let right = Input { right: true, ..Input::default() };
    for input in [right, right, right, Input::default(), right, Input::default()] {
        g.step(&input);
    }
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "steps"), format!("{:?}", crate::lua::Value::Int(2)), "two presses, however long held");
    assert_eq!(state(&mut g, "ups"), format!("{:?}", crate::lua::Value::Int(2)));
}

/// A replay recorded before an action came carries no key for it: played
/// back, the game's texts show it unbound (as where nobody plays) and the
/// game draws on (Voxkeep's "{restart} STARTS THE ROOM AGAIN" stopped it
/// on an older replay, in every picture drawn of it).
#[test]
fn an_older_replay_shows_an_action_it_lacks_unbound() {
    use crate::replay::{Playback, Replay, replay_session};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/voxkeep");
    let replay = Replay::load(&dir.join("replays/tests/climb_to_the_door.toml")).unwrap();
    assert!(replay.controls.iter().all(|c| !c.controls.keys.contains_key("restart")), "the replay is from before the restart key");
    let mut g = replay_session::<LuaGame>(&dir, &replay).unwrap();
    let mut p = Playback::new(&replay, &mut g).unwrap();
    for _ in 0..60 {
        let input = p.next_input(&mut g);
        g.step(&input);
        let _ = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    }
    assert_eq!(halted(&g), "");
}

/// Every key a game or a template binds to start with is one SDL knows by
/// that name (Pulsar's "LShift" was none: Shift never bombed, and only the
/// window's start said so).
#[test]
fn every_games_keys_are_keys() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut seen = 0;
    for base in ["games", "templates"] {
        for entry in std::fs::read_dir(root.join(base)).unwrap().flatten() {
            let dir = entry.path();
            if !dir.join("conf.lua").is_file() {
                continue;
            }
            let cfg = crate::session::config::<LuaGame>(&dir).unwrap();
            for name in LuaGame::default_keys(&cfg).names() {
                assert!(sdl3::keyboard::Scancode::from_name(name).is_some(), "{}: no key named '{name}' (SDL's names: \"Left Shift\", \"Space\", \"Z\")", dir.display());
                seen += 1;
            }
        }
    }
    assert!(seen > 20, "{seen} keys looked at");
}
