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
        m.add_method_mut("model", |_, o, (file, thing, frame, rotation, pose): (String, Value, Option<usize>, Option<Table>, Option<Table>)| {
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
        m.add_method_mut("cube", |_, o, (x, y, z, rgba, glow): (f32, f32, f32, Option<Table>, Option<f32>)| {
            o.cmds.push(Cmd::Cube { at: Vec3::new(x, y, z), color: color(rgba, [255, 255, 255, 255])?, glow: glow.unwrap_or(0.0) });
            Ok(())
        });
        m.add_method_mut("light", |lua, o, (at, radius, rgb): (Value, f32, Option<Table>)| {
            let at = to_point(lua, &at, "frame:light")?;
            let c = color(rgb, [255, 255, 255, 255])?;
            o.cmds.push(Cmd::Light { at, radius, color: [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0] });
            Ok(())
        });
        m.add_method_mut("menu", |lua, o, t: Table| {
            o.cmds.push(Cmd::Menu(menu(lua, &t)?));
            Ok(())
        });
        m.add_method_mut("grid", |lua, o, t: Table| {
            o.cmds.push(Cmd::Grid(grid(lua, &t)?));
            Ok(())
        });
        m.add_method_mut("daylight", |_, o, t: Table| {
            o.cmds.push(Cmd::Daylight(daylight(&t)?));
            Ok(())
        });
        m.add_method_mut("camera", |lua, o, t: Value| {
            let Value::Table(t) = t else { return Err(mlua::Error::runtime("frame:camera: a table of what it sets, frame:camera({ look_at = { x, y, z }, width = 120 })")) };
            let cam = picture_camera(lua, &t, o.alpha)?;
            // Where points show from here on: the picture's camera.
            if let Some(viewing) = &o.viewing {
                o.view_proj = viewing.view_proj(Some(&cam));
            }
            o.cmds.push(Cmd::Camera(cam));
            Ok(())
        });
        m.add_method_mut("text", |lua, o, (text, x, y, rgba, size): (String, f32, f32, Option<Table>, Option<i32>)| {
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text))?;
            let size = text_size(size, "frame:text")?;
            o.cmds.push(Cmd::Text { text, x, y, color: color(rgba, crate::ui::TEXT)?, size });
            Ok(())
        });
        // Where a point of the level shows, in the HUD's measure: a thing
        // where its feet are drawn this picture, as its model is.
        m.add_method("to_screen", |lua, o, (at, up): (Value, Option<f32>)| {
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
        m.add_method_mut("box", |_, o, (x, y, width, height, rgba): (f32, f32, f32, f32, Option<Table>)| {
            o.cmds.push(Cmd::Box { x, y, width, height, color: color(rgba, crate::ui::TEXT)? });
            Ok(())
        });
        m.add_method_mut("bar", |_, o, (x, y, width, height, fraction, rgba, back): (f32, f32, f32, f32, f32, Option<Table>, Option<Table>)| {
            // The filled part, then the rest beside it (a voxel drawn first
            // keeps its cell).
            let filled = (fraction.clamp(0.0, 1.0) * width).ceil().min(width);
            if filled > 0.0 {
                o.cmds.push(Cmd::Box { x, y, width: filled, height, color: color(rgba, crate::ui::RED)? });
            }
            if filled < width {
                o.cmds.push(Cmd::Box { x: x + filled, y, width: width - filled, height, color: color(back, BAR_BACK)? });
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
        m.add_method_mut("drop_in", |lua, o, (text, x, y, ticks, rgba, size): (String, f32, f32, f32, Option<Table>, Option<i32>)| {
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text))?;
            let size = text_size(size, "frame:drop_in")?;
            o.cmds.push(Cmd::Drop { text, x, y, ticks, color: color(rgba, crate::ui::TEXT)?, size });
            Ok(())
        });
    }
}
