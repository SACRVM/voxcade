//! Rotations and poses (`frame:model`, a piece's `thing.state.parts`), colours, a picture's camera and daylight.

use super::*;

/// `frame:model`'s rotation: degrees round, nose up, rolled (each 0
/// unless given), any other key a mistake (`turn`, its name before, says
/// the new one).
pub(super) fn turning(t: &Table, what: &str) -> mlua::Result<[f32; 3]> {
    turning_with(t, what, false).map(|(r, _)| r)
}

/// A pose entry: `turning`'s rotation, and with `offset` allowed the
/// part's offset in voxels, `offset = { x, y, z }` (each 0 unless given).
fn turning_with(t: &Table, what: &str, offset: bool) -> mlua::Result<([f32; 3], [f32; 3])> {
    const KEYS: [&str; 3] = ["rotation", "pitch", "roll"];
    for pair in t.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let name = match &key {
            Value::String(k) => k.to_string_lossy(),
            other => format!("{other:?}"),
        };
        if name == "turn" {
            return Err(mlua::Error::runtime(format!("{what}: 'turn' is called 'rotation' now: {{ rotation = 45 }}")));
        }
        if !KEYS.contains(&name.as_str()) && !(offset && name == "offset") {
            let words = if offset { "a pose entry has rotation, pitch and roll in degrees and an offset in voxels ({ roll = 20, offset = { 0, 0.5, 0 } })" } else { "a rotation has rotation, pitch and roll in degrees ({ roll = 20 })" };
            return Err(mlua::Error::runtime(format!("{what}: {words}, not '{name}'")));
        }
    }
    let moved = match t.raw_get::<Value>("offset")? {
        Value::Nil => [0.0; 3],
        Value::Table(o) => {
            let n = |k: i64| -> mlua::Result<f32> {
                match o.raw_get::<Value>(k)? {
                    Value::Nil => Ok(0.0),
                    Value::Integer(i) => Ok(i as f32),
                    Value::Number(x) if x.is_finite() => Ok(x as f32),
                    _ => Err(mlua::Error::runtime(format!("{what}: an offset is three numbers of voxels, {{ x, y, z }}"))),
                }
            };
            if o.raw_len() > 3 {
                return Err(mlua::Error::runtime(format!("{what}: an offset is three numbers of voxels, {{ x, y, z }}")));
            }
            [n(1)?, n(2)?, n(3)?]
        }
        _ => return Err(mlua::Error::runtime(format!("{what}: an offset is three numbers of voxels, {{ x, y, z }}"))),
    };
    let get = |key: &str| -> mlua::Result<f32> {
        match t.raw_get::<Value>(key)? {
            Value::Nil => Ok(0.0),
            Value::Integer(i) => Ok(i as f32),
            Value::Number(n) if n.is_finite() => Ok(n as f32),
            _ => Err(mlua::Error::runtime(format!("{what}: the rotation's {key} is a number of degrees"))),
        }
    };
    Ok(([get("rotation")?, get("pitch")?, get("roll")?], moved))
}

/// A pose of the figure of parts `file` (`frame:model`'s, a piece's
/// `thing.state.parts`): each part by its name, a rotation about its joint
/// (`turning`) and an offset in voxels, in the parts' names' order. `parts` are the figure's parts
/// (none: no figure); `what` names it in a mistake.
pub fn pose_of(pose: &Table, file: &str, parts: Option<&Vec<String>>, what: &str) -> mlua::Result<Vec<crate::session::PoseEntry>> {
    let mut posed = Vec::new();
    for pair in pose.pairs::<Value, Value>() {
        let (part, t) = pair?;
        let (Value::String(part), Value::Table(t)) = (part, t) else {
            return Err(mlua::Error::runtime(format!("{what}: a pose is parts by name, each a rotation: {{ head = {{ rotation = 20 }} }}")));
        };
        let part = part.to_str()?.to_string();
        match parts {
            None => return Err(mlua::Error::runtime(format!("{what}: {file} is no figure of parts (its objects named in the file) to pose"))),
            Some(names) if !names.contains(&part) => return Err(mlua::Error::runtime(format!("{what}: {file} has no part '{part}' (its parts: {})", names.join(", ")))),
            Some(_) => posed.push((part, t)),
        }
    }
    // The first mistake the same every run (a table's own walk is not).
    posed.sort_by(|a, b| a.0.cmp(&b.0));
    posed
        .into_iter()
        .map(|(part, t)| {
            let (turn, offset) = turning_with(&t, &format!("{what}, part '{part}'"), true)?;
            Ok((part, turn, offset))
        })
        .collect()
}

/// A piece's pose as its state holds it (`thing.state.parts`, `v`): none
/// for nil; `model` its model (none: a light alone), `name` what a mistake
/// calls it (its name, or "thing N").
pub fn piece_pose(v: &Value, model: Option<&str>, parts: &std::collections::BTreeMap<String, Vec<String>>, name: &str) -> mlua::Result<Vec<crate::session::PoseEntry>> {
    let what = format!("thing.state.parts of {name}");
    match v {
        Value::Nil => Ok(Vec::new()),
        Value::Table(t) => match model {
            Some(file) => pose_of(t, file, parts.get(file), &what),
            None if t.is_empty() => Ok(Vec::new()),
            None => Err(mlua::Error::runtime(format!("{what}: it has no model to pose (a light alone)"))),
        },
        other => Err(mlua::Error::runtime(format!("{what}: a pose is parts by name, each a rotation ({{ leaf = {{ rotation = 90 }} }}), or nil for at rest, not a {}", other.type_name()))),
    }
}

/// The empty part of `frame:bar`.
pub(super) const BAR_BACK: [u8; 4] = [64, 46, 40, 255];

pub(super) fn color(v: Option<Table>, default: [u8; 4]) -> mlua::Result<[u8; 4]> {
    let Some(t) = v else { return Ok(default) };
    let c = |i: i64| -> mlua::Result<u8> { Ok(t.raw_get::<Option<u8>>(i)?.unwrap_or(255)) };
    Ok([c(1)?, c(2)?, c(3)?, c(4)?])
}

/// `frame:camera{ look_at =, width =, height =, pitch =, fov = }`: what it
/// sets of this picture's camera, the rest the game's (`c.camera`). A thing
/// to look at is where it is drawn (at `alpha` into the tick, its middle).
pub(super) fn picture_camera(lua: &Lua, t: &Table, alpha: f32) -> mlua::Result<crate::session::PictureCamera> {
    let fail = |what: String| mlua::Error::runtime(format!("frame:camera: {what}"));
    let mut cam = crate::session::PictureCamera::default();
    let number = |v: &Value, key: &str, says: &str| -> mlua::Result<f32> {
        match v {
            Value::Integer(n) => Ok(*n as f32),
            Value::Number(n) if n.is_finite() => Ok(*n as f32),
            _ => Err(fail(format!("{key} is {says}, not {}", v.type_name()))),
        }
    };
    for pair in t.pairs::<Value, Value>() {
        let (key, v) = pair?;
        let key = match &key {
            Value::String(s) => s.to_str()?.to_string(),
            _ => return Err(fail("its keys are look_at, width, height, pitch and fov".into())),
        };
        match key.as_str() {
            "look_at" => {
                cam.look_at = Some(match &v {
                    Value::UserData(ud) if ud.is::<ThingRef>() => {
                        let id = ud.borrow::<ThingRef>()?.0;
                        let at = look(lua, |h| {
                            let b = &h.thing(id)?.body;
                            Ok(crate::session::between(b.prev, b.pos, alpha) + (b.centre() - b.pos))
                        })?;
                        (at.x, at.y, Some(at.z))
                    }
                    Value::Table(p) => {
                        let get = |name: &str, i: i64| -> mlua::Result<Option<f32>> {
                            let v = match p.get::<Value>(name)? {
                                Value::Nil => p.get::<Value>(i)?,
                                v => v,
                            };
                            match v {
                                Value::Nil => Ok(None),
                                v => number(&v, "look_at", "a point { x, y, z } in voxels (z may be left out: the game's camera's height)").map(Some),
                            }
                        };
                        let need = "look_at is a point { x, y, z } in voxels (z may be left out: the game's camera's height)";
                        let (Some(x), Some(y)) = (get("x", 1)?, get("y", 2)?) else { return Err(fail(need.into())) };
                        (x, y, get("z", 3)?)
                    }
                    _ => return Err(fail("look_at is a point { x, y, z } in voxels or a thing".into())),
                })
            }
            "width" | "height" => {
                let n = number(&v, &key, "how many voxels the picture shows across (width) or high (height) where it looks")?;
                if n <= 0.0 {
                    return Err(fail(format!("{key} = {n}: how many voxels the picture shows, above 0")));
                }
                if key == "width" { cam.width = Some(n) } else { cam.height = Some(n) }
            }
            "pitch" => {
                let n = number(&v, "pitch", "degrees below the horizon")?;
                if !(-89.0..=90.0).contains(&n) {
                    return Err(fail(format!("pitch = {n}: degrees below the horizon, from -89 (looking up) to 90 (straight down)")));
                }
                cam.pitch = Some(n);
            }
            "fov" => {
                let n = number(&v, "fov", "the field of view up and down in degrees")?;
                if !(10.0..=100.0).contains(&n) {
                    return Err(fail(format!("fov = {n}: the field of view up and down, from 10 to 100 degrees")));
                }
                cam.fov = Some(n);
            }
            "yaw" | "rotation" => return Err(fail(format!("{key}: the camera does not turn round yet (it looks the way c.camera says)"))),
            "aspect" => return Err(fail("aspect: the picture's shape is the game's, c.camera.aspect in conf.lua".into())),
            other => return Err(fail(format!("no '{other}': it sets look_at, width, height, pitch and fov"))),
        }
    }
    if cam.width.is_some() && cam.height.is_some() {
        return Err(fail("width or height, not both: the other follows the picture's shape".into()));
    }
    Ok(cam)
}

/// `frame:daylight{ sun =, sun_color =, ambient =, shadow =, sky = }`: what
/// it sets of this picture's daylight, the rest `c.light`'s.
pub(super) fn daylight(t: &Table) -> mlua::Result<crate::session::Daylight> {
    let fail = |what: String| mlua::Error::runtime(format!("frame:daylight: {what}"));
    let mut day = crate::session::Daylight::default();
    let rgb = |v: Value, key: &str| -> mlua::Result<[f32; 3]> {
        let Value::Table(c) = v else { return Err(fail(format!("{key} is a colour, {{ r, g, b }} from 0 to 255"))) };
        let [r, g, b, _] = color(Some(c), [255; 4])?;
        Ok([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
    };
    let unit = |v: Value, key: &str| -> mlua::Result<f32> {
        match v {
            Value::Integer(n) if (0..=1).contains(&n) => Ok(n as f32),
            Value::Number(n) if (0.0..=1.0).contains(&n) => Ok(n as f32),
            _ => Err(fail(format!("{key} is a number from 0 to 1"))),
        }
    };
    for pair in t.pairs::<Value, Value>() {
        let (key, v) = pair?;
        let key = match &key {
            Value::String(s) => s.to_str()?.to_string(),
            _ => return Err(fail("its keys are sun, sun_color, ambient, shadow and sky".into())),
        };
        match key.as_str() {
            "sun" => {
                let Value::Table(w) = v else { return Err(fail("sun is the way its light travels, { x, y, z } with z below 0 (towards the ground)".into())) };
                let at = |i: i64, name: &str| -> mlua::Result<f32> { Ok(w.raw_get::<Option<f32>>(i)?.or(w.raw_get::<Option<f32>>(name)?).unwrap_or(0.0)) };
                let way = Vec3::new(at(1, "x")?, at(2, "y")?, at(3, "z")?);
                if way.z >= 0.0 || way.length() < 1e-4 {
                    return Err(fail("sun is the way its light travels, { x, y, z } with z below 0 (towards the ground)".into()));
                }
                day.sun = Some(way);
            }
            "sun_color" => day.sun_color = Some(rgb(v, "sun_color")?),
            "sky" => day.sky = Some(rgb(v, "sky")?),
            "ambient" => day.ambient = Some(unit(v, "ambient")?),
            "shadow" => day.shadow = Some(unit(v, "shadow")?),
            other => return Err(fail(format!("no '{other}': it sets sun, sun_color, ambient, shadow and sky"))),
        }
    }
    Ok(day)
}
