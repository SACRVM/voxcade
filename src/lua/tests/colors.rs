//! A color is read one way in every call (`color::read`): "#rrggbb" or
//! { r, g, b } with numbers from 0 to 255, a fourth value where it can be
//! see-through, and a mistake naming the call and its parameter.

use super::*;
use crate::lua::world::Cmd;

/// What `on_draw` drew with these lines, and the game (halted or not).
fn drawn(name: &str, lines: &str) -> (Session<LuaGame>, Vec<Cmd>) {
    let g = game(name, &format!("function on_draw(frame)\n{lines}\nend\n"));
    let _ = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    let cmds = g.game.drawn.borrow().clone();
    (g, cmds)
}

/// The byte colors of the HUD's and the cubes' commands, in order.
fn bytes(cmds: &[Cmd]) -> Vec<[u8; 4]> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Cube { color, .. } | Cmd::Text { color, .. } | Cmd::Box { color, .. } | Cmd::Drop { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

/// A fourth value on the HUD reaches the picture: the cubes of a box, a
/// text and a bar carry it as their opacity (they were drawn opaque
/// whatever it said), and a color without one stays opaque.
#[test]
fn a_fourth_value_on_the_hud_is_its_cubes_opacity() {
    let (g, _) = drawn(
        "hud-opacity",
        "  frame:box(1, 1, 2, 2, { 10, 20, 30, 40 })\n  frame:box(9, 1, 2, 2, { 40, 50, 60 })\n  frame:text(\"A\", 1, 9, \"#70809064\")\n  frame:bar(1, 20, 8, 2, 0.5, { 200, 0, 0, 128 }, { 0, 0, 200 })",
    );
    assert_eq!(halted(&g), "");
    let (cubes, _) = g.game.hud(&crate::config::UiConfig::default(), 0.0, 16.0 / 9.0, None);
    let opacity = |rgb: [u8; 3]| {
        let of: std::collections::BTreeSet<u8> = cubes.iter().filter(|i| i.color[..3] == rgb).map(|i| i.fx[crate::render::FX_OPACITY]).collect();
        of.into_iter().collect::<Vec<u8>>()
    };
    assert_eq!(opacity([10, 20, 30]), [40], "a box's fourth value");
    assert_eq!(opacity([40, 50, 60]), [255], "a box without one");
    assert_eq!(opacity([112, 128, 144]), [100], "a text's");
    assert_eq!(opacity([200, 0, 0]), [128], "a bar's filled part");
    assert_eq!(opacity([0, 0, 200]), [255], "a bar's back without one");
}

/// Every call that draws in bytes takes hex and numbers alike, a fourth
/// value as alpha, a fraction rounded; nothing given is its default.
#[test]
fn the_hud_and_cubes_take_hex_or_numbers_and_a_fourth_value() {
    let (g, cmds) = drawn(
        "hud",
        "  frame:cube(1, 1, 1, \"#ff8000\")\n  frame:cube(1, 1, 1, { 255, 128, 0 })\n  frame:cube(1, 1, 1, { 255, 127.5, 0, 100 })\n  frame:cube(1, 1, 1, \"#ff800064\")\n  frame:text(\"A\", 1, 1, \"#102030\")\n  frame:text(\"A\", 1, 1, { 16, 32, 48 })\n  frame:box(1, 1, 2, 2, \"#10203080\")\n  frame:bar(1, 1, 8, 2, 0.5, \"#ff0000\", { 0, 0, 255 })\n  frame:drop_in(\"A\", 1, 1, 3, { 1.4, 200, 9 })\n  frame:text(\"A\", 1, 1)",
    );
    assert_eq!(halted(&g), "");
    assert_eq!(
        bytes(&cmds),
        [[255, 128, 0, 255], [255, 128, 0, 255], [255, 128, 0, 100], [255, 128, 0, 100], [16, 32, 48, 255], [16, 32, 48, 255], [16, 32, 48, 128], [255, 0, 0, 255], [0, 0, 255, 255], [1, 200, 9, 255], crate::ui::TEXT]
    );
}

/// A light, the daylight and the fog take hex and numbers alike, a
/// fraction kept; the same color either way to the bit.
#[test]
fn lights_daylight_and_fog_take_hex_or_numbers() {
    let (g, cmds) = drawn(
        "lights",
        "  frame:light({ x = 1, y = 2, z = 3 }, 20, \"#ff8000\")\n  frame:light({ x = 1, y = 2, z = 3 }, 20, { 255, 128, 0 })\n  frame:light{ at = { x = 1, y = 2, z = 3 }, radius = 20, color = { 255, 127.5, 0 } }\n  frame:daylight{ sun_color = \"#ffe0c0\", sky = { 16, 32, 48 } }\n  frame:fog{ density = 0.01, color = \"#0a141e\" }",
    );
    assert_eq!(halted(&g), "");
    let lights: Vec<[f32; 3]> = cmds.iter().filter_map(|c| if let Cmd::Light(l) = c { Some(l.color) } else { None }).collect();
    assert_eq!(lights, [[1.0, 128.0 / 255.0, 0.0], [1.0, 128.0 / 255.0, 0.0], [1.0, (127.5f64 / 255.0) as f32, 0.0]]);
    let day = cmds.iter().find_map(|c| if let Cmd::Daylight(d) = c { Some(*d) } else { None }).unwrap();
    assert_eq!((day.sun_color, day.sky), (Some([1.0, 224.0 / 255.0, 192.0 / 255.0]), Some([16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0])));
    let fog = cmds.iter().find_map(|c| if let Cmd::Fog(f) = c { Some(*f) } else { None }).unwrap();
    assert_eq!(fog.color, Some([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]));
}

/// Each wrong color is a mistake naming the call and its parameter, with
/// the rule; a fourth value where a color cannot be see-through says why.
#[test]
fn a_wrong_color_names_the_call_and_the_rule() {
    let rule = crate::color::RULE;
    let see = crate::color::RULE_SEE_THROUGH;
    for (call, says) in [
        ("frame:text(\"A\", 1, 1, \"orange\")", format!("frame:text: color: \"orange\" is no color: {see}")),
        ("frame:text(\"A\", 1, 1, { 255, 0 })", format!("frame:text: color: {{ 255, 0 }} has 2 numbers: {see}")),
        ("frame:text(\"A\", 1, 1, {})", format!("frame:text: color: {{}} has 0 numbers: {see}")),
        ("frame:cube(1, 1, 1, { 300, 0, 0 })", "frame:cube: color: { 300, 0, 0 }: each number is from 0 to 255, not 300".into()),
        ("frame:box(1, 1, 2, 2, true)", format!("frame:box: color: {see}, not a boolean")),
        ("frame:bar(1, 1, 8, 2, 0.5, nil, { r = 1 })", format!("frame:bar: back: {see}, not a table with named keys")),
        ("frame:drop_in(\"A\", 1, 1, 3, { 1, 0.5, 0 })", "frame:drop_in: color: { 1, 0.5, 0 } reads as numbers from 0 to 1, but a color's are from 0 to 255: write { 255, 127.5, 0 } or \"#ff8000\"".into()),
        ("frame:text(\"A\", 1, 1, { 1, 1, 1 })", "frame:text: color: { 1, 1, 1 } reads as numbers from 0 to 1, but a color's are from 0 to 255: write { 255, 255, 255 } or \"#ffffff\"".into()),
        ("frame:light({ x = 1, y = 2, z = 3 }, 20, { 255, 0, 0, 128 })", format!("frame:light: color: {{ 255, 0, 0, 128 }}: a light has no transparency, so no fourth number: {rule}")),
        ("frame:light{ at = { x = 1, y = 2, z = 3 }, radius = 20, color = \"#ff000080\" }", format!("frame:light: color: \"#ff000080\": a light has no transparency, so no \"#rrggbbaa\": {rule}")),
        ("frame:daylight{ sky = { 10, 20, 30, 40 } }", format!("frame:daylight: sky: {{ 10, 20, 30, 40 }}: the daylight has no transparency, so no fourth number: {rule}")),
        ("frame:fog{ color = \"#0a141e80\" }", format!("frame:fog: color: \"#0a141e80\": the fog's color has no transparency, so no \"#rrggbbaa\": {rule}")),
        ("frame:fog{ color = 5 }", format!("frame:fog: color: {rule}, not an integer")),
    ] {
        let (g, _) = drawn("wrong", &format!("  {call}"));
        assert!(halted(&g).contains(&says) && halted(&g).contains("main.lua:2"), "{call}:\n{}\nwanted: {says}", halted(&g));
    }
}

/// `level:set_voxel` and `level:fill` take a color either way into the
/// level's palette (rounded), no fourth value: a level's voxels are
/// opaque.
#[test]
fn a_voxel_takes_hex_or_numbers_but_no_fourth_value() {
    let mut g = game("voxels", "function on_level_start()\n  level:set_voxel(1, 1, 1, \"#5a6e4a\")\n  level:set_voxel(2, 1, 1, { 90, 110, 74 })\n  level:fill(3, 1, 1, 4, 1, 1, { 90, 109.6, 74 })\nend\nfunction on_tick()\n  world.state.a = level:voxel(1, 1, 1)\n  world.state.b = level:voxel(2, 1, 1)\n  world.state.c = level:voxel(4, 1, 1)\nend\n");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    let (a, b, c) = (state(&mut g, "a"), state(&mut g, "b"), state(&mut g, "c"));
    assert!(a == b && b == c && a != "Int(0)", "one palette entry: {a} {b} {c}");
    let g = game("voxel-alpha", "function on_level_start()\n  level:set_voxel(1, 1, 1, { 90, 110, 74, 128 })\nend\n");
    assert!(halted(&g).contains("level:set_voxel: { 90, 110, 74, 128 }: a level's voxels are opaque (a model's glass is see-through), so no fourth number"), "{}", halted(&g));
    let g = game("voxel-word", "function on_level_start()\n  level:fill(0, 0, 0, 1, 1, 1, \"green\")\nend\n");
    assert!(halted(&g).contains("level:fill: \"green\" is no color: a color is"), "{}", halted(&g));
}
