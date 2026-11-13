//! `music`: the game's song, its clock, its volume and muffle.

use super::*;

/// `music`: the song the game plays of its own choice, and its beats and
/// bars, so the game can go with it (waves on the beat). Its clock is the
/// game's (a step every tick): replays play it the same.
pub struct MusicView;

impl UserData for MusicView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_function_get("song", |lua, _| look(lua, |h| Ok(h.music.song.and_then(|s| h.songs.get(s)).map(|t| t.name.clone()))));
        f.add_field_function_get("beat", |lua, _| look(lua, |h| Ok(h.music.beats(&h.songs).map(|b| b.floor() as i64))));
        f.add_field_function_get("intensity", |lua, _| look(lua, |h| Ok(h.music.intensity)));
        f.add_field_function_set("intensity", |lua, _, v: f32| {
            if !(0.0..=1.0).contains(&v) {
                return Err(mlua::Error::runtime(format!("music.intensity: from 0 (calm) to 1 (all of the song), not {v}")));
            }
            change(lua, "music.intensity", |h| {
                h.music.intensity = v;
                Ok(())
            })
        });
        f.add_field_function_get("bar", |lua, _| {
            look(lua, |h| {
                let meter = h.music.song.and_then(|s| h.songs.get(s)).map_or(4, |t| t.meter) as f64;
                Ok(h.music.beats(&h.songs).map(|b| (b / meter).floor() as i64))
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("music"));
        m.add_meta_function(MetaMethod::Index, |_, (_, key): (AnyUserData, String)| -> mlua::Result<Value> {
            Err(mlua::Error::runtime(format!("music has no {key} (music:play, music:volume, music:muffle, music.song, music.beat, music.bar, music.intensity)")))
        });
        m.add_function("play", |lua, (_, song, how): (AnyUserData, Option<String>, Value)| {
            // From when as a word, or { from = ..., crossfade = seconds }.
            let (when, cross) = match &how {
                Value::Nil => (None, None),
                Value::String(s) => (Some(s.to_str()?.to_string()), None),
                Value::Table(t) => {
                    known_keys(t, &["from", "crossfade"], "music:play")?;
                    let cross = match t.raw_get::<Value>("crossfade")? {
                        Value::Nil => None,
                        Value::Integer(i) if (0..=60).contains(&i) => Some(i as f32),
                        Value::Number(n) if (0.0..=60.0).contains(&n) => Some(n as f32),
                        other => return Err(mlua::Error::runtime(format!("music:play: crossfade is seconds from 0 (none) to 60, not {}", shown(&other)))),
                    };
                    let from = match t.raw_get::<Value>("from")? {
                        Value::Nil => None,
                        Value::String(s) => Some(s.to_str()?.to_string()),
                        other => return Err(mlua::Error::runtime(format!("music:play: from is \"bar\", \"beat\" or \"now\", not {}", shown(&other)))),
                    };
                    (from, cross)
                }
                other => return Err(mlua::Error::runtime(format!("music:play: after the song, \"bar\", \"beat\" or \"now\", or {{ from = ..., crossfade = seconds }}, not {}", shown(other)))),
            };
            change(lua, "music:play", |h| {
                let when = match when.as_deref() {
                    None | Some("bar") => crate::lua::host::When::Bar,
                    Some("beat") => crate::lua::host::When::Beat,
                    Some("now") => crate::lua::host::When::Now,
                    Some(other) => return Err(mlua::Error::runtime(format!("music:play: from \"bar\", \"beat\" or \"now\", not \"{other}\""))),
                };
                let song = match &song {
                    None => None,
                    Some(name) => match h.songs.iter().position(|t| &t.name == name) {
                        Some(s) => Some(s),
                        None => {
                            let names: Vec<&str> = h.songs.iter().map(|t| t.name.as_str()).collect();
                            let list = if names.is_empty() { "none: c.audio.music = { \"music/NAME.mml\" }".to_string() } else { names.join(", ") };
                            return Err(mlua::Error::runtime(format!("music:play: no song called '{name}' in c.audio.music (its songs: {list})")));
                        }
                    },
                };
                // Asked for what plays already: it goes on, nothing else asked.
                h.music.next = if song == h.music.song { None } else { Some((song, when, cross)) };
                Ok(())
            })
        });
        m.add_function("volume", |lua, (_, level, seconds): (AnyUserData, Value, Value)| {
            mix_ask(lua, "music:volume", "level", "0 (silent) to 1 (as loud as the song is)", level, seconds, |m| &mut m.volume)
        });
        m.add_function("muffle", |lua, (_, amount, seconds): (AnyUserData, Value, Value)| {
            mix_ask(lua, "music:muffle", "amount", "0 (clear) to 1 (very dull, as through a wall)", amount, seconds, |m| &mut m.muffle)
        });
    }
}

/// A value as a mistake shows it: a text in quotes, a number as it is,
/// else its type.
fn shown(v: &Value) -> String {
    match v {
        Value::String(s) => format!("\"{}\"", s.to_string_lossy()),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.type_name().to_string(),
    }
}

/// `music:volume` and `music:muffle`: without a value, the one asked for
/// last; with one (0 to 1), the song glides there over `seconds` (0.3
/// unless said; 0 at once, still without a click). Only heard: in no hash.
fn mix_ask(
    lua: &Lua,
    what: &str,
    name: &str,
    range: &str,
    value: Value,
    seconds: Value,
    glide: fn(&mut crate::audio::music::Mix) -> &mut crate::audio::music::Glide,
) -> mlua::Result<Option<f64>> {
    let number = |v: &Value| match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Number(n) => Some(*n),
        _ => None,
    };
    if value.is_nil() {
        if !seconds.is_nil() {
            return Err(mlua::Error::runtime(format!("{what}: the {name} first, from {range}, then the seconds")));
        }
        let mut mix = look(lua, |h| Ok(h.music.mix))?;
        return Ok(Some(f64::from(glide(&mut mix).to)));
    }
    let to = match number(&value) {
        Some(v) if (0.0..=1.0).contains(&v) => v as f32,
        Some(v) => return Err(mlua::Error::runtime(format!("{what}: {name} from {range}, not {v}"))),
        None => return Err(mlua::Error::runtime(format!("{what}: {name} is a number from {range}, not {}", shown(&value)))),
    };
    let over = match (&seconds, number(&seconds)) {
        (Value::Nil, _) => 0.3,
        (_, Some(s)) if s >= 0.0 && s.is_finite() => s as f32,
        (_, Some(s)) => return Err(mlua::Error::runtime(format!("{what}: seconds is 0 (at once) or more, not {s}"))),
        (other, None) => return Err(mlua::Error::runtime(format!("{what}: seconds is a number, 0 (at once) or more, not {}", shown(other)))),
    };
    change(lua, what, |h| {
        glide(&mut h.music.mix).ask(to, over);
        Ok(None)
    })
}
