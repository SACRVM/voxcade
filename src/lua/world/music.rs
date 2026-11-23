//! `music`: the game's song, its clock, its volume and muffle, its channels' strips.

use super::*;
use crate::audio::music::{GLIDING, NEXT_NOTE, NO_KNOB};

/// `music`: the song the game plays of its own choice, and its beats and
/// bars, so the game can go with it (waves on the beat). Its clock is the
/// game's (a step every tick): replays play it the same.
pub struct MusicView;

impl UserData for MusicView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field(MetaMethod::Type, "music");
        f.add_field_function_get("song", |lua, _| look(lua, |h| Ok(h.music.song.and_then(|s| h.songs.get(s)).map(|t| t.name.clone()))));
        f.add_field_function_get("beat", |lua, _| look(lua, |h| Ok(h.music.clock(&h.songs).map(|c| c.beat as i64))));
        f.add_field_function_get("position", |lua, _| look(lua, |h| Ok(h.music.clock(&h.songs).map(|c| c.position))));
        // The tempo held, else the song's own, both kept to a thousandth.
        f.add_field_function_get("tempo", |lua, _| look(lua, |h| Ok(h.music.tempo.or_else(|| h.music.milli_tempo(&h.songs)).map(|m| f64::from(m) / 1000.0))));
        f.add_field_function_set("tempo", |lua, _, v: Value| {
            let tempo = match v {
                Value::Nil => None,
                Value::Integer(i) if (1..=999).contains(&i) => Some(crate::lua::host::milli(i as f64)),
                Value::Number(n) if (1.0..=999.0).contains(&n) => Some(crate::lua::host::milli(n)),
                other => return Err(mlua::Error::runtime(format!("music.tempo: beats a minute from 1 to 999, or nil for each song's own #tempo, not {}", shown(&other)))),
            };
            change(lua, "music.tempo", |h| {
                h.music.tempo = tempo;
                Ok(())
            })
        });
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
        f.add_field_function_get("bar", |lua, _| look(lua, |h| Ok(h.music.clock(&h.songs).map(|c| c.bar as i64))));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("music"));
        m.add_meta_function(MetaMethod::Index, |_, (_, key): (AnyUserData, Value)| -> mlua::Result<Value> { Err(no_such("music", &key, None)) });
        m.add_meta_function(MetaMethod::NewIndex, |_, (_, key, _): (AnyUserData, Value, Value)| -> mlua::Result<()> { Err(not_settable("music", &key, None)) });
        m.add_function("play", |lua, (_, song, GivenValue(how)): (AnyUserData, Option<String>, GivenValue)| {
            // From when as a word, or { from = ..., crossfade = seconds, at = bar, channels = { K = mml } }.
            let (when, cross, at, channels) = match &how {
                Value::Nil => (None, None, None, None),
                Value::String(s) => (Some(s.to_str()?.to_string()), None, None, None),
                Value::Table(t) => {
                    known_keys(t, &["from", "crossfade", "at", "channels"], "music:play")?;
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
                    let at = match t.raw_get::<Value>("at")? {
                        Value::Nil => None,
                        Value::Integer(i) if (1..=i64::from(u32::MAX)).contains(&i) => Some(i as u32),
                        Value::Number(n) if n.fract() == 0.0 && (1.0..=f64::from(u32::MAX)).contains(&n) => Some(n as u32),
                        other => return Err(mlua::Error::runtime(format!("music:play: at is the bar the song begins at, a whole number from 1 (its first bar), not {}", shown(&other)))),
                    };
                    let channels = match t.raw_get::<GivenValue>("channels")?.0 {
                        Value::Nil => None,
                        Value::Table(c) => Some(laid(&c)?),
                        other => return Err(mlua::Error::runtime(format!("music:play: channels is a table of a letter and its notes, like {{ K = \"@kick c4 c4 c4 c4\" }}, not {}", shown(&other)))),
                    };
                    (from, cross, at, channels)
                }
                other => return Err(mlua::Error::runtime(format!("music:play: after the song, \"bar\", \"beat\" or \"now\", or {{ from = ..., crossfade = seconds, at = bar, channels = ... }}, not {}", shown(other)))),
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
                let timing = song.and_then(|s| h.songs.get(s));
                let studio = timing.and_then(|t| t.studio.clone());
                // Channels are laid over a studio; a studio plays only laid
                // ones (those that play stay where none are given).
                let laid = match (channels, timing, &studio) {
                    (Some(_), None, _) => return Err(mlua::Error::runtime("music:play: channels are laid over a file of instruments of c.audio.music, named first: music:play(\"drums\", { channels = ... })")),
                    (Some(_), Some(t), None) => return Err(mlua::Error::runtime(format!("music:play: {} has channels of its own; channels are laid over a file of instruments only (@instruments and #settings, no channel)", t.name))),
                    (Some(c), Some(_), Some(_)) => Some(Arc::new(c)),
                    (None, Some(_), Some(_)) if song == h.music.song && h.music.laid.is_some() => h.music.laid.clone(),
                    (None, Some(t), Some(_)) => return Err(mlua::Error::runtime(format!("music:play: {} is a file of instruments: lay channels over it, like music:play(\"{}\", {{ channels = {{ K = \"@kick c4 c4 c4 c4\" }} }})", t.name, t.name))),
                    (None, ..) => None,
                };
                // A bar within the song: the player's own begins at its
                // start; laid channels' bars are counted as they are read.
                let bar_mistake = |at: u32, bars: u32, t: &crate::audio::music::Timing| match (crate::audio::music::Timing { bars, ..t.clone() }).bar_mistake(at) {
                    Some(wrong) => Err(mlua::Error::runtime(format!("music:play: at = {at} {wrong}"))),
                    None => Ok(()),
                };
                if let Some(at) = at {
                    let Some(t) = timing else {
                        return Err(mlua::Error::runtime("music:play: at needs a song of yours (the player's own begins at its start)"));
                    };
                    if laid.is_none() {
                        bar_mistake(at, t.bars, t)?;
                    }
                }
                let ask = crate::lua::host::SongAsk { song, stop: false, when, crossfade: cross, at: at.unwrap_or(1), laid };
                // Asked for what is to come or what plays already, the same
                // channels too: nothing new to read (what plays goes on,
                // nothing else asked, a stop to come forgotten). Stopped,
                // nothing plays already.
                if h.music.next.as_ref() == Some(&ask) {
                    return Ok(());
                }
                if !h.music.silent && song == h.music.song && ask.laid == h.music.laid {
                    h.music.next = None;
                    return Ok(());
                }
                if song.is_some() && song == h.music.song && (cross.is_some() || at.is_some()) {
                    let word = if at.is_some() { "at" } else { "crossfade" };
                    return Err(mlua::Error::runtime(format!("music:play: {word} is for a song that begins; {} plays, and only its channels change", timing.map_or("", |t| t.name.as_str()))));
                }
                // Laid channels read against the studio now, so a mistake
                // halts at this line.
                if let (Some(laid), Some(studio), Some(t)) = (&ask.laid, &studio, timing) {
                    let song = crate::audio::music::lay(studio, laid).map_err(|e| mlua::Error::runtime(format!("music:play: {e:#}")))?;
                    if let Some(at) = at {
                        bar_mistake(at, song.bars, t)?;
                    }
                }
                h.music.next = Some(ask);
                Ok(())
            })
        });
        m.add_function("stop", |lua, (_, GivenValue(how)): (AnyUserData, GivenValue)| {
            // From when as a word, or { from = ..., fade = seconds }.
            let (when, fade) = match &how {
                Value::Nil => (None, None),
                Value::String(s) => (Some(s.to_str()?.to_string()), None),
                Value::Table(t) => {
                    known_keys(t, &["from", "fade"], "music:stop")?;
                    let fade = match t.raw_get::<Value>("fade")? {
                        Value::Nil => None,
                        Value::Integer(i) if (0..=60).contains(&i) => Some(i as f32),
                        Value::Number(n) if (0.0..=60.0).contains(&n) => Some(n as f32),
                        other => return Err(mlua::Error::runtime(format!("music:stop: fade is seconds from 0 (at once) to 60, not {}", shown(&other)))),
                    };
                    let from = match t.raw_get::<Value>("from")? {
                        Value::Nil => None,
                        Value::String(s) => Some(s.to_str()?.to_string()),
                        other => return Err(mlua::Error::runtime(format!("music:stop: from is \"now\", \"beat\" or \"bar\", not {}", shown(&other)))),
                    };
                    (from, fade)
                }
                other => return Err(mlua::Error::runtime(format!("music:stop: \"now\", \"beat\" or \"bar\", or {{ from = ..., fade = seconds }}, not {}", shown(other)))),
            };
            change(lua, "music:stop", |h| {
                let when = match when.as_deref() {
                    None | Some("now") => crate::lua::host::When::Now,
                    Some("beat") => crate::lua::host::When::Beat,
                    Some("bar") => crate::lua::host::When::Bar,
                    Some(other) => return Err(mlua::Error::runtime(format!("music:stop: from \"now\", \"beat\" or \"bar\", not \"{other}\""))),
                };
                // Stopped already: silence goes on, a song asked since
                // forgotten.
                if h.music.silent {
                    h.music.next = None;
                    return Ok(());
                }
                // At once unless faded: the audio's shortest fade keeps it
                // from clicking.
                h.music.next = Some(crate::lua::host::SongAsk { song: None, stop: true, when, crossfade: Some(fade.unwrap_or(0.0)), at: 1, laid: None });
                Ok(())
            })
        });
        m.add_function("mix", |lua, (_, channel, GivenValue(settings), seconds): (AnyUserData, Value, GivenValue, Value)| {
            let letter = match &channel {
                Value::String(s) => {
                    let s = s.to_string_lossy();
                    let mut chars = s.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) if c.is_ascii_alphabetic() => c,
                        _ => return Err(mlua::Error::runtime(format!("music:mix: a channel is a letter, A to Z or a to z, not \"{s}\""))),
                    }
                }
                other => return Err(mlua::Error::runtime(format!("music:mix: a channel is a letter, A to Z or a to z, not {}", shown(other)))),
            };
            // Without settings: the strip as asked last.
            if settings.is_nil() {
                if !seconds.is_nil() {
                    return Err(mlua::Error::runtime("music:mix: the settings first ({ volume, mute, pan } or false), then the seconds"));
                }
                let strip = look(lua, |h| Ok(h.music.mix.strip(letter)))?;
                let t = lua.create_table()?;
                t.raw_set("volume", strip.volume)?;
                t.raw_set("mute", strip.mute)?;
                t.raw_set("pan", strip.pan.to)?;
                for (word, v) in strip.turned() {
                    t.raw_set(word, v)?;
                }
                return Ok(Value::Table(t));
            }
            let over = glide_seconds("music:mix", &seconds)?;
            let number = |key: &str, v: Value, lo: f64, hi: f64, range: &str| -> mlua::Result<Option<Option<f32>>> {
                match v {
                    Value::Nil => Ok(None),
                    Value::Boolean(false) => Ok(Some(None)),
                    Value::Integer(i) if (lo..=hi).contains(&(i as f64)) => Ok(Some(Some(i as f32))),
                    Value::Number(n) if (lo..=hi).contains(&n) => Ok(Some(Some(n as f32))),
                    other => Err(mlua::Error::runtime(format!("music:mix: {key} is {range}, or false for {}, not {}", if key == "pan" { "the middle" } else { "1" }, shown(&other)))),
                }
            };
            // The keys asked (None: kept); false gives a key, or the whole
            // strip, back. The instrument's words turned (None: given back).
            let mut words: Vec<(&'static str, Option<f32>)> = Vec::new();
            let (volume, mute, pan) = match &settings {
                Value::Boolean(false) => {
                    words.extend(GLIDING.iter().chain(&NEXT_NOTE).map(|w| (*w, None)));
                    (Some(None), Some(false), Some(None))
                }
                Value::Table(t) => {
                    words = knob_words(t)?;
                    let volume = number("volume", t.raw_get("volume")?, 0.0, 1.0, "a number from 0 (silent) to 1 (as written)")?;
                    let pan = number("pan", t.raw_get("pan")?, -1.0, 1.0, "a number from -1 (left) to 1 (right)")?;
                    let mute = match t.raw_get::<Value>("mute")? {
                        Value::Nil => None,
                        Value::Boolean(b) => Some(b),
                        other => return Err(mlua::Error::runtime(format!("music:mix: mute is true or false, not {}", shown(&other)))),
                    };
                    (volume, mute, pan)
                }
                other => return Err(mlua::Error::runtime(format!("music:mix: after the channel, {{ volume = 0..1, mute = true|false, pan = -1..1, cutoff = 900, ... }} or false (as written), not {}", shown(other)))),
            };
            change(lua, "music:mix", |h| {
                // A song file's channels are its own; a studio's are laid
                // by the game, and with none of its songs playing or asked
                // any letter waits for the song to come.
                let songs: Vec<&crate::audio::music::Timing> = [h.music.song, h.music.next.as_ref().filter(|a| !a.stop).and_then(|a| a.song)].into_iter().flatten().filter_map(|s| h.songs.get(s)).collect();
                if !songs.is_empty() && songs.iter().all(|t| t.studio.is_none()) && !songs.iter().any(|t| t.letters.contains(&letter)) {
                    let has: Vec<String> = songs.iter().map(|t| format!("{} has {}", t.name, t.letters.iter().map(char::to_string).collect::<Vec<_>>().join(", "))).collect();
                    return Err(mlua::Error::runtime(format!("music:mix: no channel {letter} in your song ({})", has.join("; "))));
                }
                let strips = Arc::make_mut(&mut h.music.mix.strips);
                let strip = strips.entry(letter).or_default();
                if let Some(v) = volume {
                    strip.volume = v.unwrap_or(1.0);
                }
                if let Some(m) = mute {
                    strip.mute = m;
                }
                if volume.is_some() || mute.is_some() {
                    let to = if strip.mute { 0.0 } else { strip.volume };
                    strip.level.ask(to, over);
                }
                if let Some(p) = pan {
                    strip.pan.ask(p.unwrap_or(0.0), over);
                }
                for (word, v) in &words {
                    strip.turn(word, *v, over);
                }
                Ok(Value::Nil)
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

/// The instrument's words a `music:mix` table turns, each a number in its
/// range (`audio::words`) or false (given back to the instrument), in the
/// order of the tables; any other key but the strip's own is a mistake
/// naming why, or the word meant.
fn knob_words(t: &mlua::Table) -> mlua::Result<Vec<(&'static str, Option<f32>)>> {
    use crate::audio::words;
    const STRIP: [&str; 3] = ["volume", "mute", "pan"];
    for pair in t.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let name = match &key {
            Value::String(s) => s.to_str()?.to_string(),
            other => return Err(mlua::Error::runtime(format!("music:mix: a key is a word, like volume or cutoff, not {}", shown(other)))),
        };
        let name = name.as_str();
        if STRIP.contains(&name) || GLIDING.contains(&name) || NEXT_NOTE.contains(&name) {
            continue;
        }
        if let Some((_, why)) = NO_KNOB.iter().find(|(w, _)| *w == name) {
            return Err(mlua::Error::runtime(format!("music:mix: {name} does not turn: {why}")));
        }
        if words::word(name).is_some_and(|w| w.mml.is_none()) {
            return Err(mlua::Error::runtime(format!("music:mix: {name} is a word of sound files, not of an instrument")));
        }
        let all = || STRIP.iter().chain(&GLIDING).chain(&NEXT_NOTE).copied();
        let near = words::near(name, all()).map_or(String::new(), |w| format!(" (did you mean {w}?)"));
        return Err(mlua::Error::runtime(format!("music:mix: no word {name}{near}; it takes {}", all().collect::<Vec<_>>().join(", "))));
    }
    let mut turned = Vec::new();
    for word in GLIDING.iter().chain(&NEXT_NOTE) {
        let usage = words::word(word).and_then(|w| w.mml.as_ref()).expect("a knob is an instrument's word");
        let (lo, hi) = usage.range.expect("a knob has a range");
        let whole = usage.unit == words::Unit::Bits;
        let value = match t.raw_get::<Value>(*word)? {
            Value::Nil => continue,
            Value::Boolean(false) => None,
            Value::Integer(i) if (lo..=hi).contains(&(i as f64)) => Some(i as f32),
            Value::Number(n) if (lo..=hi).contains(&n) && (!whole || n.fract() == 0.0) => Some(n as f32),
            other => {
                let kind = if whole { "a whole number" } else { "a number" };
                return Err(mlua::Error::runtime(format!("music:mix: {word} is {kind} {}, or false for the instrument's own, not {}", words::range_words(usage), shown(&other))));
            }
        };
        turned.push((*word, value));
    }
    Ok(turned)
}

/// `music:play`'s `channels`: each a letter and its MML (a table the
/// script may only read is read as it sees it).
fn laid(t: &mlua::Table) -> mlua::Result<crate::audio::music::Laid> {
    let mut channels = Vec::new();
    for pair in t.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let letter = match &key {
            Value::String(s) => {
                let s = s.to_string_lossy();
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if c.is_ascii_alphabetic() => c,
                    _ => return Err(mlua::Error::runtime(format!("music:play: a channel is a letter, A to Z or a to z, not \"{s}\""))),
                }
            }
            other => return Err(mlua::Error::runtime(format!("music:play: a channel is a letter, A to Z or a to z, not {}", shown(other)))),
        };
        match value {
            Value::String(text) => channels.push((letter, text.to_str()?.to_string())),
            other => return Err(mlua::Error::runtime(format!("music:play: channel {letter} is its notes as a text of MML, not {}", shown(&other)))),
        }
    }
    if channels.is_empty() {
        return Err(mlua::Error::runtime("music:play: channels lays one channel at least, like { K = \"@kick c4 c4 c4 c4\" }"));
    }
    Ok(crate::audio::music::Laid::new(channels))
}

/// A value as a mistake shows it: a text in quotes, a number as it is,
/// else its type.
pub(super) fn shown(v: &Value) -> String {
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
        let mut mix = look(lua, |h| Ok(h.music.mix.clone()))?;
        return Ok(Some(f64::from(glide(&mut mix).to)));
    }
    let to = match number(&value) {
        Some(v) if (0.0..=1.0).contains(&v) => v as f32,
        Some(v) => return Err(mlua::Error::runtime(format!("{what}: {name} from {range}, not {v}"))),
        None => return Err(mlua::Error::runtime(format!("{what}: {name} is a number from {range}, not {}", shown(&value)))),
    };
    let over = glide_seconds(what, &seconds)?;
    change(lua, what, |h| {
        glide(&mut h.music.mix).ask(to, over);
        Ok(None)
    })
}

/// The seconds a glide of the music's sound takes (`music:volume`,
/// `music:muffle`, `music:mix`): 0.3 unless said, 0 at once.
fn glide_seconds(what: &str, seconds: &Value) -> mlua::Result<f32> {
    match seconds {
        Value::Nil => Ok(0.3),
        Value::Integer(i) if *i >= 0 => Ok(*i as f32),
        Value::Number(s) if *s >= 0.0 && s.is_finite() => Ok(*s as f32),
        Value::Integer(_) | Value::Number(_) => Err(mlua::Error::runtime(format!("{what}: seconds is 0 (at once) or more, not {}", shown(seconds)))),
        other => Err(mlua::Error::runtime(format!("{what}: seconds is a number, 0 (at once) or more, not {}", shown(other)))),
    }
}
