//! `frame`, `on_draw`'s picture: text, cubes, models, lights, menus, the camera and the daylight.

use super::*;

/// `frame` (`on_draw(frame)`): the picture being made; only there, only
/// for this picture.
#[derive(Default)]
pub struct Frame {
    pub cmds: Vec<Cmd>,
    /// The HUD's grid in UI voxels: across (by the screen's shape) and up.
    pub width: f32,
    pub height: f32,
    /// The big letters' font (`c.ui.display_font`), for `text_width`.
    pub font: Option<std::sync::Arc<crate::font::DisplayFont>>,
    /// How far into the tick this picture is (0 to 1), for smooth motion.
    pub alpha: f32,
    /// The parts of the game's figures of parts, by model (a pose's names).
    pub parts: std::sync::Arc<std::collections::BTreeMap<String, Vec<String>>>,
    /// The game's kept models by name, with their frame counts, and the
    /// models loaded when needed: `frame:model` draws a kept one or any
    /// model file of the game (one that is not there is a mistake).
    pub known: std::sync::Arc<std::collections::BTreeMap<String, usize>>,
    pub needed: Option<std::sync::Arc<crate::session::Needed>>,
    /// The camera's projection this picture, and whether moving things are
    /// drawn on the grid (`to_screen`).
    pub view_proj: glam::Mat4,
    pub snap: bool,
    /// How the play is seen unless `frame:camera` says otherwise (None on
    /// the title), so `to_screen` after it goes by the picture's camera.
    pub viewing: Option<crate::session::Viewing>,
    /// This machine's pointer now and what it is over in the picture
    /// before (`frame.pointer`): None with the pad used last, or no
    /// pointer.
    pub pointer: Option<crate::input::Pointer>,
}

/// A text size: 1 the pixel font, 2 and up the big letters.
/// The words of `frame:light`'s table: where, how far, its colour, and how
/// it shines (`render::LightShape`, `render::LIGHT_WORDS`).
const LIGHT_KEYS: [&str; 10] = ["at", "radius", "color", "falloff", "cone", "blend", "rotation", "pitch", "shadows", "fog"];

/// A light's radius as a script gives it: a number above 0 that a float
/// holds (no limit above it: every place that takes a radius takes any).
fn radius_of(n: Option<f64>) -> mlua::Result<f32> {
    n.map(|r| r as f32).filter(|r| r.is_finite() && *r > 0.0).ok_or_else(|| mlua::Error::runtime("frame:light: radius is a number above 0 (how far it reaches, voxels)"))
}

/// A light as `frame:light{ ... }` writes it: `at` and `radius` needed, the
/// rest as a level's light takes them, each checked (a mistake names the
/// word and what it wants).
/// Why a light's color takes no fourth value.
const LIGHT_OPAQUE: &str = "a light has no transparency";

fn light_of(lua: &Lua, t: &Table) -> mlua::Result<crate::render::Light> {
    let fail = |what: String| mlua::Error::runtime(format!("frame:light: {what}"));
    let mut shape = crate::render::LightShape::default();
    let (mut at, mut of, mut radius, mut rgb) = (None, None, None, None);
    for pair in t.pairs::<Value, GivenValue>() {
        let (key, GivenValue(v)) = pair?;
        let Value::String(key) = key else { return Err(fail(format!("its words are {}", LIGHT_KEYS.join(", ")))) };
        let key = key.to_str()?.to_string();
        let number = |v: &Value| match v {
            Value::Integer(n) => Some(*n as f64),
            Value::Number(n) if n.is_finite() => Some(*n),
            _ => None,
        };
        let ranged = |v: &Value| -> mlua::Result<f32> {
            let (_, low, high) = crate::render::LIGHT_WORDS.iter().find(|(w, ..)| *w == key).copied().expect("a word of the light's");
            number(v).filter(|n| (low..=high).contains(n)).map(|n| n as f32).ok_or_else(|| fail(format!("{key} is a number from {low} to {high}")))
        };
        match key.as_str() {
            "at" => (at, of) = (Some(to_point(lua, &v, "frame:light")?), thing_key(lua, &v)?),
            "radius" => radius = Some(radius_of(number(&v))?),
            "color" => rgb = Some(color_of(&v, "color", LIGHT_OPAQUE).map_err(fail)?),
            "falloff" => shape.falloff = ranged(&v)?,
            "cone" => shape.cone = Some(ranged(&v)?),
            "blend" => shape.blend = ranged(&v)?,
            "rotation" => shape.rotation = ranged(&v)?,
            "pitch" => shape.pitch = ranged(&v)?,
            "shadows" => {
                shape.shadows = Some(match &v {
                    Value::String(w) => crate::render::LampShadows::read(&w.to_str()?).map_err(|e| fail(format!("shadows: {e}")))?,
                    Value::Boolean(on) => return Err(fail(crate::render::LampShadows::boolean(*on))),
                    _ => return Err(fail(format!("shadows is one of {} ({})", crate::render::LampShadows::list(), crate::render::LampShadows::MEANS))),
                })
            }
            "fog" => shape.fog = ranged(&v)?,
            other => {
                let near = crate::audio::words::near(other, LIGHT_KEYS.into_iter()).map_or(String::new(), |n| format!(" (did you mean `{n}`?)"));
                return Err(fail(format!("no `{other}`{near}: a light has {}", LIGHT_KEYS.join(", "))));
            }
        }
    }
    let at = at.ok_or_else(|| fail("at is missing: where it shines from, a thing or a point { x, y, z }".into()))?;
    let radius = radius.ok_or_else(|| fail("radius is missing: how far it reaches, voxels".into()))?;
    Ok(crate::render::Light { of, ..crate::render::Light::lamp(at, radius, rgb.unwrap_or([1.0; 3]), &shape, |w| w) })
}

/// Why an outline's color takes no fourth value.
const OUTLINE_OPAQUE: &str = "an outline has no transparency";

/// `frame:draw`'s `outline = { width =, color = }`: how many HUD voxels
/// thick, 1 to 8, and its colour (None: `c.effects.outline_color`).
fn outline_of(v: &Value) -> mlua::Result<(f32, Option<[f32; 3]>)> {
    let fail = |what: String| mlua::Error::runtime(format!("frame:draw: outline: {what}"));
    let Value::Table(t) = v else { return Err(fail("a table, { width = 1, color = \"#101010\" }".into())) };
    super::menus::known_keys(t, &["width", "color"], "frame:draw: outline")?;
    let width = match t.raw_get::<Value>("width")? {
        Value::Integer(n) if (1..=8).contains(&n) => n as f32,
        Value::Number(n) if (1.0..=8.0).contains(&n) && n.fract() == 0.0 => n as f32,
        Value::Nil => return Err(fail("width is missing: how many HUD voxels thick, 1 to 8".into())),
        _ => return Err(fail("width is a whole number of HUD voxels from 1 to 8".into())),
    };
    let color = match t.raw_get::<GivenValue>("color")?.0 {
        Value::Nil => None,
        c => Some(color_of(&c, "color", OUTLINE_OPAQUE).map_err(fail)?),
    };
    Ok((width, color))
}

fn text_size(size: Option<i32>, what: &str) -> mlua::Result<i32> {
    match size.unwrap_or(1) {
        n @ 1..=4 => Ok(n),
        n => Err(mlua::Error::runtime(format!("{what}: size {n}: from 1 (the small letters) to 4"))),
    }
}

/// An action's key as the player sees it (`controls`).
pub(super) fn key_of<'a>(controls: &'a Controls, action: &str, what: &str) -> mlua::Result<&'a str> {
    match controls.keys.get(action) {
        Some(label) => Ok(label),
        None => {
            let known: Vec<&str> = controls.keys.keys().map(String::as_str).collect();
            Err(mlua::Error::runtime(format!("{what}: no action '{action}' (c.actions and pause: {})", known.join(", "))))
        }
    }
}

/// The language the game speaks to the player (`world.language`): the
/// one chosen if the game speaks it, else its first, else English.
pub fn language_of(h: &Stage) -> String {
    h.controls.language.as_ref().filter(|l| h.languages.contains(l)).or(h.languages.first()).cloned().unwrap_or_else(|| "en".into())
}

/// `world.language` as `on_draw` sees it: the machine drawing's (each
/// machine its own over the network), the game's in the simulation.
pub(super) fn drawn_language(h: &Stage) -> String {
    match h.local.language.as_ref().filter(|l| h.languages.contains(l)) {
        Some(own) => own.clone(),
        None => language_of(h),
    }
}

/// `text` with every `{action}` its key, `{{` a brace: the key's sign
/// where `signs` and the engine has one (`ui::signs`), else its name, as
/// `{action:name}` always. The signs' own characters a script's text
/// never carries: they are left out.
pub(super) fn fill_keys(controls: &Controls, text: &str, signs: bool) -> mlua::Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let plain = |out: &mut String, s: &str| out.extend(s.chars().filter(|c| !crate::ui::signs::in_signs(*c)));
    while let Some(at) = rest.find('{') {
        plain(&mut out, &rest[..at]);
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('{') {
            out.push('{');
            rest = after;
            continue;
        }
        let Some(end) = rest.find('}') else {
            return Err(mlua::Error::runtime("frame:text: a '{' without its '}' (an action's key is {jump}, its name {jump:name}, a brace {{)"));
        };
        let (action, named) = match rest[..end].split_once(':') {
            None => (&rest[..end], false),
            Some((action, "name")) => (action, true),
            Some((action, other)) => return Err(mlua::Error::runtime(format!("frame:text: {{{action}:{other}}}: after an action only :name, its key's name ({{{action}:name}})"))),
        };
        let label = key_of(controls, action, "frame:text")?;
        match controls.device.filter(|_| signs && !named).and_then(|device| crate::ui::signs::key_sign(device, label)) {
            Some(sign) => out.push(sign),
            // A key's name in capitals, as on the keys themselves.
            None => out.push_str(&crate::ui::engine_case(label)),
        }
        rest = &rest[end + 1..];
    }
    plain(&mut out, rest);
    Ok(out)
}

/// Whether a text of `size` shows keys as signs: they are drawn in the
/// pixel font, so not in the big letters of a display font.
pub(super) fn signs_at(size: i32, font: Option<&crate::font::DisplayFont>) -> bool {
    size < 2 || font.is_none()
}


impl UserData for Frame {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field(MetaMethod::Type, "frame");
        f.add_field_method_get("width", |_, o| Ok(o.width));
        // The players at the machine this picture is for (every taken seat
        // at one machine), in seat order.
        f.add_field_function_get("players", |lua, _| {
            let seats = look(lua, |h| Ok(h.drawn_seats()))?;
            handles(lua, seats.into_iter().map(seat_handle))
        });
        f.add_field_method_get("height", |_, o| Ok(o.height));
        // This machine's pointer now and the panel under it (the picture
        // before's): nil with the pad used last, or no pointer.
        f.add_field_method_get("pointer", |lua, o| {
            let Some(p) = &o.pointer else { return Ok(Value::Nil) };
            let t = lua.create_table()?;
            t.raw_set("x", p.x)?;
            t.raw_set("y", p.y)?;
            if let Some(panel) = p.panel.as_ref().filter(|panel| !panel.name.is_empty()) {
                t.raw_set("hud", panel.name.as_str())?;
                t.raw_set("index", (panel.index > 0).then_some(panel.index))?;
            }
            if let Some(met) = p.level.as_ref().filter(|_| p.panel.is_none()) {
                super::input::met_answers(lua, met, &t, false)?;
            }
            Ok(Value::Table(t))
        });
        f.add_field_method_get("alpha", |_, o| Ok(o.alpha));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("frame"));
        m.add_meta_function(MetaMethod::Index, |_, (_, key): (AnyUserData, Value)| -> mlua::Result<Value> { Err(no_such("frame", &key, None)) });
        m.add_meta_function(MetaMethod::NewIndex, |_, (_, key, _): (AnyUserData, Value, Value)| -> mlua::Result<()> { Err(not_settable("frame", &key, None)) });
        m.add_method_mut("model", |_, o, (file, thing, frame, rotation, pose): (String, Value, Option<usize>, Option<Given>, Option<Given>)| {
            // A model is drawn where a thing stands (at a point: not yet).
            if let Value::Table(_) = thing {
                return Err(mlua::Error::runtime("frame:model(file, thing, ...): a model is drawn where a thing stands, so the second is a thing, not a point (for a model at a place: a thing of a kind without a behavior, placed there)"));
            }
            let thing = thing_id(&thing, "frame:model")?;
            // A model the game keeps, else any model file of the game,
            // loaded now if this is the first time it is drawn.
            let loaded = match (o.known.contains_key(&file), &o.needed) {
                (true, _) => None,
                (false, Some(needed)) => Some(needed.load(&file).map_err(|e| mlua::Error::runtime(format!("frame:model: {e:#}")))?),
                (false, None) => return Err(mlua::Error::runtime(format!("frame:model: no model \"{file}\""))),
            };
            let turn = match rotation {
                Some(t) => turning(&t, "frame:model")?,
                None => [0.0; 3],
            };
            // A figure of parts posed: each part by its name, a rotation.
            let posed = match pose {
                Some(pose) => {
                    let parts: Option<Vec<String>> = loaded.as_ref().map(|m| m.parts.iter().map(|p| p.name.clone()).collect());
                    let parts = parts.as_ref().filter(|p| !p.is_empty()).or(o.parts.get(&file));
                    pose_of(&pose, &file, parts, "frame:model")?
                }
                None => Vec::new(),
            };
            o.cmds.push(Cmd::Model { file, thing, frame, turn, pose: posed });
            Ok(())
        });
        // A level's piece posed for this picture alone, over its state's
        // pose (laid as the pieces are drawn, `LuaGame::draw_game`).
        m.add_method_mut("pose", |lua, o, (thing, pose): (Value, Given)| {
            let what = "frame:pose";
            let thing = thing_id(&thing, what)?;
            let pose = look(lua, |h| match h.here.pieces.get(&thing) {
                Some(p) => match &p.model {
                    Some(file) => pose_of(&pose, file, h.parts.get(file), what),
                    None => Err(mlua::Error::runtime(format!("{what}: {} has no model to pose (a light alone)", piece_name(h, thing)))),
                },
                None => {
                    let kind = h.kind_name(h.thing(thing)?.kind).unwrap_or("player");
                    Err(mlua::Error::runtime(format!("{what}: only a level's piece (a [[thing]] of its file with a model) is posed for now, not a thing of the kind '{kind}'")))
                }
            })?;
            o.cmds.push(Cmd::Pose { thing, pose });
            Ok(())
        });
        m.add_method_mut("cube", |_, o, (x, y, z, rgba, glow): (f32, f32, f32, Option<GivenValue>, Option<f32>)| {
            o.cmds.push(Cmd::Cube { at: Vec3::new(x, y, z), color: color(rgba.as_ref().map(|g| &g.0), "frame:cube: color", [255, 255, 255, 255])?, glow: glow.unwrap_or(0.0) });
            Ok(())
        });
        m.add_method_mut("light", |lua, o, (GivenValue(at), radius, rgb, more): (GivenValue, Option<f64>, Option<GivenValue>, Option<GivenValue>)| {
            if more.is_some() {
                return Err(mlua::Error::runtime("frame:light: a fourth argument is not taken; the rest of a light's words go in a table: frame:light{ at = player, radius = 30, color = { 255, 200, 120 }, cone = 30 }"));
            }
            let light = match (&at, radius) {
                // frame:light{ at =, radius =, color =, falloff =, ... }
                (Value::Table(t), None) if rgb.is_none() => light_of(lua, t)?,
                // A radius of 0 or less lights nothing (a light faded out),
                // as it always did; one no float holds is a mistake.
                (_, Some(radius)) if radius <= 0.0 => return Ok(()),
                (_, Some(radius)) => {
                    let radius = radius_of(Some(radius))?;
                    let (of, at) = (thing_key(lua, &at)?, to_point(lua, &at, "frame:light")?);
                    let c = match rgb.as_ref().map(|g| &g.0) {
                        None | Some(Value::Nil) => [1.0; 3],
                        Some(v) => color_of(v, "color", LIGHT_OPAQUE).map_err(|e| mlua::Error::runtime(format!("frame:light: {e}")))?,
                    };
                    crate::render::Light { of, ..crate::render::Light::lamp(at, radius, c, &crate::render::LightShape::default(), |w| w) }
                }
                _ => return Err(mlua::Error::runtime("frame:light: a light is frame:light(at, radius, color) or frame:light{ at =, radius =, ... }")),
            };
            o.cmds.push(Cmd::Light(light));
            Ok(())
        });
        m.add_method_mut("menu", |lua, o, t: Given| {
            o.cmds.push(Cmd::Menu(menu(lua, &t)?));
            Ok(())
        });
        m.add_method_mut("grid", |lua, o, t: Given| {
            let mut g = grid(lua, &t)?;
            // A slot's model: any model file of the game, loaded the first
            // time, its front shown small.
            let inside = (g.cell.round() as i32 - 2).max(1);
            for (n, c) in g.cells.iter_mut().enumerate() {
                let Some(file) = &c.model else { continue };
                let Some(needed) = &o.needed else { return Err(mlua::Error::runtime(format!("frame:grid: cells[{}]: no model \"{file}\"", n + 1))) };
                let model = needed.load(file).map_err(|e| mlua::Error::runtime(format!("frame:grid: cells[{}]: {e:#}", n + 1)))?;
                c.thumb = Some(Arc::new(super::menus::thumbnail(&model, inside)));
            }
            o.cmds.push(Cmd::Grid(g));
            Ok(())
        });
        m.add_method_mut("draw", |_, o, (file, x, y, options): (String, f32, f32, Option<Given>)| {
            const WHAT: &str = "frame:draw";
            let (mut frame, mut quarters, mut opacity) = (0, 0, 1.0);
            // A rotation given as a table, a pose or an outline: the model
            // whole.
            let (mut turn, mut pose, mut outline) = (None, None, None);
            if let Some(t) = &options {
                super::menus::known_keys(t, &["frame", "rotation", "pose", "outline", "opacity"], WHAT)?;
                let whole = |v: Value, key: &str| -> mlua::Result<Option<f64>> {
                    match v {
                        Value::Nil => Ok(None),
                        Value::Integer(n) => Ok(Some(n as f64)),
                        Value::Number(n) if n.is_finite() && n.fract() == 0.0 => Ok(Some(n)),
                        _ => Err(mlua::Error::runtime(format!("{WHAT}: {key} is a whole number"))),
                    }
                };
                pose = t.raw_get::<Option<Given>>("pose")?;
                outline = match t.raw_get::<GivenValue>("outline")?.0 {
                    Value::Nil => None,
                    v => Some(outline_of(&v)?),
                };
                match t.raw_get::<GivenValue>("rotation")?.0 {
                    Value::Table(r) => turn = Some(turning(&r, WHAT)?),
                    v => {
                        if let Some(r) = whole(v, "rotation")? {
                            if pose.is_some() || outline.is_some() {
                                let why = if pose.is_some() { "posed" } else { "outlined" };
                                return Err(mlua::Error::runtime(format!("{WHAT}: rotation = {r} turns the flat picture; {why}, the model is drawn whole and its rotation is a table, rotation = {{ rotation = {r} }}")));
                            }
                            if r % 90.0 != 0.0 {
                                return Err(mlua::Error::runtime(format!("{WHAT}: rotation = {r}: quarter turns only (0, 90, 180, 270); to turn it freely, a table: rotation = {{ rotation = {r} }}")));
                            }
                            quarters = (r / 90.0).rem_euclid(4.0) as u8;
                        }
                    }
                }
                opacity = match t.raw_get::<Value>("opacity")? {
                    Value::Nil => 1.0,
                    Value::Integer(n) if (0..=1).contains(&n) => n as f32,
                    Value::Number(n) if (0.0..=1.0).contains(&n) => n as f32,
                    Value::Integer(n) => return Err(mlua::Error::runtime(format!("{WHAT}: opacity = {n}: from 0 (unseen) to 1 (whole)"))),
                    Value::Number(n) => return Err(mlua::Error::runtime(format!("{WHAT}: opacity = {n}: from 0 (unseen) to 1 (whole)"))),
                    _ => return Err(mlua::Error::runtime(format!("{WHAT}: opacity is a number from 0 (unseen) to 1 (whole)"))),
                };
                frame = match whole(t.raw_get("frame")?, "frame")? {
                    Some(n) if n >= 0.0 => n as usize,
                    Some(n) => return Err(mlua::Error::runtime(format!("{WHAT}: frame = {n}: its frames count from 0"))),
                    None => 0,
                };
            }
            // Any model file of the game, loaded the first time it is drawn.
            let Some(needed) = &o.needed else { return Err(mlua::Error::runtime(format!("{WHAT}: no model \"{file}\""))) };
            let model = needed.load(&file).map_err(|e| mlua::Error::runtime(format!("{WHAT}: {e:#}")))?;
            if frame >= model.frames.len() {
                return Err(mlua::Error::runtime(format!("{WHAT}: frame = {frame}, but \"{file}\" has {} (from 0)", model.frames.len())));
            }
            if turn.is_some() || pose.is_some() || outline.is_some() {
                let parts: Vec<String> = model.parts.iter().map(|p| p.name.clone()).collect();
                let pose = match &pose {
                    Some(p) => pose_of(p, &file, Some(&parts).filter(|p| !p.is_empty()), WHAT)?,
                    None => Vec::new(),
                };
                o.cmds.push(Cmd::Whole { file, frame, x, y, turn: turn.unwrap_or_default(), pose, outline, opacity });
                return Ok(());
            }
            // One voxel of the model one of the HUD's.
            let thumb = super::menus::front(&model, frame, quarters, 1);
            o.cmds.push(Cmd::Draw { thumb: Arc::new(thumb), x, y, opacity });
            Ok(())
        });
        m.add_method_mut("hud", |_, o, (name, x, y, width, height): (String, f32, f32, f32, f32)| {
            super::menus::check_panel_name(&name, "frame:hud")?;
            if name.is_empty() {
                return Err(mlua::Error::runtime("frame:hud: a name first, frame:hud(\"bag\", x, y, width, height)"));
            }
            if ![x, y, width, height].iter().all(|v| v.is_finite()) || width < 0.0 || height < 0.0 {
                return Err(mlua::Error::runtime("frame:hud: x, y, width and height are numbers, width and height 0 or more"));
            }
            o.cmds.push(Cmd::Hud { name, x, y, width, height });
            Ok(())
        });
        m.add_method_mut("daylight", |_, o, t: Given| {
            o.cmds.push(Cmd::Daylight(daylight(&t)?));
            Ok(())
        });
        m.add_method_mut("fog", |_, o, GivenValue(t): GivenValue| {
            let Value::Table(t) = t else { return Err(mlua::Error::runtime("frame:fog: a table of what it sets, frame:fog{ density = 0.01, color = { 20, 20, 30 } }")) };
            o.cmds.push(Cmd::Fog(fog(&t)?));
            Ok(())
        });
        m.add_method_mut("camera", |lua, o, GivenValue(t): GivenValue| {
            let Value::Table(t) = t else { return Err(mlua::Error::runtime("frame:camera: a table of what it sets, frame:camera({ look_at = { x, y, z }, width = 120 })")) };
            let cam = picture_camera(lua, &t, o.alpha)?;
            // Where points show from here on: the picture's camera.
            if let Some(viewing) = &o.viewing {
                o.view_proj = viewing.view_proj(Some(&cam));
            }
            o.cmds.push(Cmd::Camera(cam));
            Ok(())
        });
        m.add_method_mut("text", |lua, o, (text, x, y, rgba, size): (String, f32, f32, Option<GivenValue>, Option<i32>)| {
            let size = text_size(size, "frame:text")?;
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text, signs_at(size, o.font.as_deref())))?;
            o.cmds.push(Cmd::Text { text, x, y, color: color(rgba.as_ref().map(|g| &g.0), "frame:text: color", crate::ui::TEXT)?, size });
            Ok(())
        });
        // Where a point of the level shows, in the HUD's measure: a thing
        // where its feet are drawn this picture, as its model is.
        m.add_method("to_screen", |lua, o, (GivenValue(at), up): (GivenValue, Option<f32>)| {
            let at = match &at {
                Value::UserData(ud) if ud.is::<ThingRef>() => {
                    let id = ud.borrow::<ThingRef>()?.0;
                    let feet = look(lua, |h| {
                        let b = &h.thing(id)?.body;
                        Ok(crate::session::between(b.prev, b.pos, o.alpha))
                    })?;
                    if o.snap { feet.floor() } else { feet }
                }
                _ => to_point(lua, &at, "frame:to_screen")?,
            };
            let clip = o.view_proj * (at + Vec3::Z * up.unwrap_or(0.0)).extend(1.0);
            if clip.w <= 1e-6 {
                return Ok((None, None));
            }
            let (x, y) = (clip.x / clip.w, clip.y / clip.w);
            Ok((Some((x + 1.0) * 0.5 * o.width), Some((1.0 - y) * 0.5 * o.height)))
        });
        m.add_method("text_width", |lua, o, (text, size): (String, Option<i32>)| {
            let size = text_size(size, "frame:text_width")?;
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text, signs_at(size, o.font.as_deref())))?;
            Ok(crate::ui::line_width(&text, size, o.font.as_deref()))
        });
        m.add_method_mut("box", |_, o, (x, y, width, height, rgba): (f32, f32, f32, f32, Option<GivenValue>)| {
            o.cmds.push(Cmd::Box { x, y, width, height, color: color(rgba.as_ref().map(|g| &g.0), "frame:box: color", crate::ui::TEXT)? });
            Ok(())
        });
        m.add_method_mut("bar", |_, o, (x, y, width, height, fraction, rgba, back): (f32, f32, f32, f32, f32, Option<GivenValue>, Option<GivenValue>)| {
            // The filled part, then the rest beside it (a voxel drawn first
            // keeps its cell).
            let filled = (fraction.clamp(0.0, 1.0) * width).ceil().min(width);
            if filled > 0.0 {
                o.cmds.push(Cmd::Box { x, y, width: filled, height, color: color(rgba.as_ref().map(|g| &g.0), "frame:bar: color", crate::ui::RED)? });
            }
            if filled < width {
                o.cmds.push(Cmd::Box { x: x + filled, y, width: width - filled, height, color: color(back.as_ref().map(|g| &g.0), "frame:bar: back", BAR_BACK)? });
            }
            Ok(())
        });
        m.add_method_mut("icon", |lua, o, (name, x, y): (String, f32, f32)| {
            look(lua, |h| match h.hud.icons.contains_key(&name) {
                true => Ok(()),
                false => {
                    let names: Vec<&str> = h.hud.icons.keys().map(String::as_str).collect();
                    Err(mlua::Error::runtime(format!("frame:icon: no icon called '{name}' (c.icons: {})", names.join(", "))))
                }
            })?;
            o.cmds.push(Cmd::Icon { name, x, y });
            Ok(())
        });
        m.add_method_mut("drop_in", |lua, o, (text, x, y, ticks, rgba, size): (String, f32, f32, f32, Option<GivenValue>, Option<i32>)| {
            let size = text_size(size, "frame:drop_in")?;
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text, signs_at(size, o.font.as_deref())))?;
            o.cmds.push(Cmd::Drop { text, x, y, ticks, color: color(rgba.as_ref().map(|g| &g.0), "frame:drop_in: color", crate::ui::TEXT)?, size });
            Ok(())
        });
    }
}
