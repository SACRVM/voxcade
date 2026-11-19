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
}

/// A text size: 1 the pixel font, 2 and up the big letters.
/// The words of `frame:light`'s table: where, how far, its colour, and how
/// it shines (`render::LightShape`, `render::LIGHT_WORDS`).
const LIGHT_KEYS: [&str; 10] = ["at", "radius", "color", "falloff", "cone", "blend", "rotation", "pitch", "shadows", "fog"];

/// What each word of `shadows` means, for a mistake in one.
const SHADOW_MEANS: &str = "\"none\": it shines through walls, \"static\": the level's voxels and pieces stop it, \"all\": moving things too";

/// A light as `frame:light{ ... }` writes it: `at` and `radius` needed, the
/// rest as a level's light takes them, each checked (a mistake names the
/// word and what it wants).
fn light_of(lua: &Lua, t: &Table) -> mlua::Result<crate::render::Light> {
    let fail = |what: String| mlua::Error::runtime(format!("frame:light: {what}"));
    let mut shape = crate::render::LightShape::default();
    let (mut at, mut radius, mut rgb) = (None, None, None);
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
            "at" => at = Some(to_point(lua, &v, "frame:light")?),
            "radius" => radius = Some(number(&v).filter(|r| *r > 0.0).ok_or_else(|| fail("radius is a number above 0 (how far it reaches, voxels)".into()))? as f32),
            "color" => {
                let Value::Table(c) = v else { return Err(fail("color is a color, { r, g, b } with each a whole number from 0 to 255".into())) };
                rgb = Some(color_of(&c, "color").map_err(fail)?);
            }
            "falloff" => shape.falloff = ranged(&v)?,
            "cone" => shape.cone = Some(ranged(&v)?),
            "blend" => shape.blend = ranged(&v)?,
            "rotation" => shape.rotation = ranged(&v)?,
            "pitch" => shape.pitch = ranged(&v)?,
            "shadows" => {
                shape.shadows = Some(match &v {
                    Value::String(w) => crate::render::LampShadows::read(&w.to_str()?).map_err(|e| fail(format!("shadows: {e} ({SHADOW_MEANS})")))?,
                    Value::Boolean(on) => return Err(fail(crate::render::LampShadows::boolean(*on))),
                    _ => return Err(fail(format!("shadows is one of {} ({SHADOW_MEANS})", crate::render::LampShadows::list()))),
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
    Ok(crate::render::Light::lamp(at, radius, rgb.unwrap_or([1.0; 3]), &shape, |w| w))
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

/// `text` with every `{action}` its key, `{{` a brace.
pub(super) fn fill_keys(controls: &Controls, text: &str) -> mlua::Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('{') {
            out.push('{');
            rest = after;
            continue;
        }
        let Some(end) = rest.find('}') else {
            return Err(mlua::Error::runtime("frame:text: a '{' without its '}' (an action's key is {jump}, a brace {{)"));
        };
        // A key's name in capitals, as on the keys themselves.
        out.push_str(&crate::ui::engine_case(key_of(controls, &rest[..end], "frame:text")?));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
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
        m.add_method_mut("cube", |_, o, (x, y, z, rgba, glow): (f32, f32, f32, Option<Given>, Option<f32>)| {
            o.cmds.push(Cmd::Cube { at: Vec3::new(x, y, z), color: color(rgba.as_deref(), [255, 255, 255, 255])?, glow: glow.unwrap_or(0.0) });
            Ok(())
        });
        m.add_method_mut("light", |lua, o, (GivenValue(at), radius, rgb, more): (GivenValue, Option<f32>, Option<Given>, Option<GivenValue>)| {
            if more.is_some() {
                return Err(mlua::Error::runtime("frame:light: a fourth argument is not taken; the rest of a light's words go in a table: frame:light{ at = player, radius = 30, color = { 255, 200, 120 }, cone = 30 }"));
            }
            let light = match (&at, radius) {
                // frame:light{ at =, radius =, color =, falloff =, ... }
                (Value::Table(t), None) if rgb.is_none() => light_of(lua, t)?,
                (_, Some(radius)) => {
                    let at = to_point(lua, &at, "frame:light")?;
                    let c = match rgb.as_deref() {
                        Some(t) => color_of(t, "color").map_err(|e| mlua::Error::runtime(format!("frame:light: {e}")))?,
                        None => [1.0; 3],
                    };
                    crate::render::Light::lamp(at, radius, c, &crate::render::LightShape::default(), |w| w)
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
            o.cmds.push(Cmd::Grid(grid(lua, &t)?));
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
        m.add_method_mut("text", |lua, o, (text, x, y, rgba, size): (String, f32, f32, Option<Given>, Option<i32>)| {
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text))?;
            let size = text_size(size, "frame:text")?;
            o.cmds.push(Cmd::Text { text, x, y, color: color(rgba.as_deref(), crate::ui::TEXT)?, size });
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
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text))?;
            Ok(crate::ui::line_width(&text, text_size(size, "frame:text_width")?, o.font.as_deref()))
        });
        m.add_method_mut("box", |_, o, (x, y, width, height, rgba): (f32, f32, f32, f32, Option<Given>)| {
            o.cmds.push(Cmd::Box { x, y, width, height, color: color(rgba.as_deref(), crate::ui::TEXT)? });
            Ok(())
        });
        m.add_method_mut("bar", |_, o, (x, y, width, height, fraction, rgba, back): (f32, f32, f32, f32, f32, Option<Given>, Option<Given>)| {
            // The filled part, then the rest beside it (a voxel drawn first
            // keeps its cell).
            let filled = (fraction.clamp(0.0, 1.0) * width).ceil().min(width);
            if filled > 0.0 {
                o.cmds.push(Cmd::Box { x, y, width: filled, height, color: color(rgba.as_deref(), crate::ui::RED)? });
            }
            if filled < width {
                o.cmds.push(Cmd::Box { x: x + filled, y, width: width - filled, height, color: color(back.as_deref(), BAR_BACK)? });
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
        m.add_method_mut("drop_in", |lua, o, (text, x, y, ticks, rgba, size): (String, f32, f32, f32, Option<Given>, Option<i32>)| {
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text))?;
            let size = text_size(size, "frame:drop_in")?;
            o.cmds.push(Cmd::Drop { text, x, y, ticks, color: color(rgba.as_deref(), crate::ui::TEXT)?, size });
            Ok(())
        });
    }
}
