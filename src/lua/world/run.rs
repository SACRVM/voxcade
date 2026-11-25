//! `world`: the whole run, its asks and its saves.

use super::*;

/// `world`: the whole run.
pub struct WorldView;

impl UserData for WorldView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field(MetaMethod::Type, "world");
        f.add_field_function_get("state", |lua, _| plain_state(lua, "voxcade.state"));
        f.add_field_function_get("tick", |lua, _| look(lua, |h| Ok(h.host.sim().tick)));
        f.add_field_function_get("language", |lua, _| look(lua, |h| Ok(if looking(lua) { drawn_language(h) } else { language_of(h) })));
        f.add_field_function_get("levels", |lua, _| {
            let names: Vec<String> = look(lua, |h| Ok(h.levels.names().into_iter().map(String::from).collect()))?;
            lua.create_sequence_from(names)
        });
        f.add_field_function_get("leaving", |lua, _| look(lua, |h| Ok(h.ask.as_ref().map(Ask::why))));
        f.add_field_function_get("saves", |lua, _| {
            let (slots, newest) = look(lua, |h| Ok((h.slots.clone(), h.newest_save())))?;
            let list = lua.create_table()?;
            for (n, info) in slots.into_iter().enumerate() {
                if let Some(info) = info {
                    let t = lua.create_table()?;
                    t.raw_set("level", info.level)?;
                    t.raw_set("seconds", info.ticks / 60)?;
                    t.raw_set("label", info.label)?;
                    list.raw_set(n + 1, t)?;
                }
            }
            // `newest` is read through the table's metatable, so `pairs`,
            // `ipairs` and `#` meet the numbered slots only, as ever.
            let more = lua.create_table()?;
            more.raw_set("newest", newest)?;
            let meta = lua.create_table()?;
            meta.raw_set("__index", more)?;
            list.set_metatable(Some(meta))?;
            Ok(list)
        });
        // Every seat: its player, or false while nobody sits there.
        f.add_field_function_get("players", |lua, _| {
            world(lua)?;
            let taken = look(lua, |h| Ok(h.seats.iter().map(|s| s.taken).collect::<Vec<_>>()))?;
            let list = lua.create_table()?;
            for (i, taken) in taken.into_iter().enumerate() {
                match taken {
                    true => list.raw_set(i + 1, handle(lua, seat_handle(i + 1))?)?,
                    false => list.raw_set(i + 1, false)?,
                }
            }
            Ok(list)
        });
        f.add_field_function_get("items", |lua, _| {
            let items = look(lua, |h| {
                let cfg = &h.host.sim().cfg;
                Ok(cfg.item_names().into_iter().map(|n| (n.to_string(), cfg.items[n].clone())).collect::<Vec<_>>())
            })?;
            let list = lua.create_table()?;
            for (n, (name, item)) in items.into_iter().enumerate() {
                let t = lua.create_table()?;
                t.raw_set("name", name)?;
                t.raw_set("model", item.model)?;
                t.raw_set("stack", item.stack)?;
                list.raw_set(n + 1, t)?;
            }
            Ok(list)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("world"));
        m.add_meta_function(MetaMethod::Index, |_, (_, key): (AnyUserData, Value)| -> mlua::Result<Value> { Err(no_such("world", &key, Some("world.state"))) });
        m.add_meta_function(MetaMethod::NewIndex, |_, (_, key, _): (AnyUserData, Value, Value)| -> mlua::Result<()> { Err(not_settable("world", &key, Some("world.state"))) });
        m.add_function("go_to", |lua, (_, level, GivenValue(place), GivenValue(options)): (AnyUserData, Value, GivenValue, GivenValue)| {
            no_run_here(lua, "world:go_to")?;
            let (place, fade, fresh) = place_and_fade(lua, place, options, "world:go_to")?;
            let index = look(lua, |h| {
                let n = h.levels.list.len();
                let index = match &level {
                    Value::String(name) => {
                        let name = name.to_string_lossy();
                        h.levels.find(&name).ok_or_else(|| mlua::Error::runtime(format!("world:go_to: no level called '{name}' (levels: {})", h.levels.names().join(", "))))?
                    }
                    Value::Integer(i) if *i >= 1 && *i as usize <= n => *i as usize - 1,
                    Value::Number(f) if f.fract() == 0.0 && *f >= 1.0 && *f as usize <= n => *f as usize - 1,
                    Value::Integer(_) | Value::Number(_) => return Err(mlua::Error::runtime(format!("world:go_to: no level {}: there are {n}, numbered from 1", level.to_string()?))),
                    Value::Nil => return Err(mlua::Error::runtime("world:go_to: which level? (nil: after the last level, level.next is nil; a game is won with world:win())")),
                    other => return Err(mlua::Error::runtime(format!("world:go_to: a level's name or number, not a {}", other.type_name()))),
                };
                let name = &h.levels.list[index].name;
                if h.levels.title == Some(index) {
                    return Err(mlua::Error::runtime(format!("world:go_to: '{name}' is the game's title (c.title_screen.level), played before a run and never in one: a run ends with world:win() or world:lose()")));
                }
                if let Some(p) = &place {
                    check_place(&h.levels.list[index], p, "world:go_to")?;
                }
                Ok(name.clone())
            })?;
            ask(lua, "world:go_to", Ask::GoTo { level: index, place, fade, fresh })
        });
        m.add_function("restart", |lua, (_, GivenValue(place), GivenValue(options)): (AnyUserData, GivenValue, GivenValue)| {
            let (place, fade, fresh) = place_and_fade(lua, place, options, "world:restart")?;
            if let Some(p) = &place {
                look(lua, |h| check_place(&h.levels.list[h.level], p, "world:restart"))?;
            }
            ask(lua, "world:restart", Ask::Restart { place, fade, fresh })
        });
        m.add_function("save", |lua, (_, n, GivenValue(options)): (AnyUserData, i64, GivenValue)| {
            if phase(lua).is_some() {
                return Err(mlua::Error::runtime("world:save: only while a level is played (on_level_start, on_tick): there is none to save here"));
            }
            if look(lua, |h| Ok(h.on_title()))? {
                return Err(mlua::Error::runtime("world:save: no saving on the title (c.title_screen.level): it is no run, world:new_game() begins one"));
            }
            let label = label_of(options)?;
            change(lua, "world:save", |h| {
                let n = slot(h, n, "world:save")?;
                h.saving.retain(|(s, _)| *s != n);
                h.saving.push((n, SlotAsk::Save(label)));
                Ok(())
            })
        });
        m.add_function("delete_save", |lua, (_, n): (AnyUserData, i64)| {
            change(lua, "world:delete_save", |h| {
                let n = slot(h, n, "world:delete_save")?;
                if h.slots[n as usize - 1].is_none() {
                    return Err(mlua::Error::runtime(format!("world:delete_save: slot {n} is empty (world.saves[{n}] tells before)")));
                }
                h.saving.retain(|(s, _)| *s != n);
                h.saving.push((n, SlotAsk::Delete));
                Ok(())
            })
        });
        m.add_function("load", |lua, (_, n, GivenValue(options)): (AnyUserData, i64, GivenValue)| {
            let fade = fade_of(options, "world:load")?;
            let n = look(lua, |h| {
                let n = slot(h, n, "world:load")?;
                match h.slots[n as usize - 1] {
                    Some(_) => Ok(n),
                    None => Err(mlua::Error::runtime(format!("world:load: slot {n} is empty (world.saves[{n}] tells before)"))),
                }
            })?;
            ask(lua, "world:load", Ask::Load { slot: Some(n), fade })
        });
        m.add_function("hitstop", |lua, (_, ticks): (AnyUserData, u32)| {
            change(lua, "world:hitstop", |h| {
                let sim = h.host.sim_mut();
                sim.freeze = sim.freeze.max(ticks.min(MAX_HITSTOP));
                Ok(())
            })
        });
        m.add_function("read", |lua, (_, name): (AnyUserData, String)| lua.named_registry_value::<Function>("voxcade.read")?.call::<mlua::LuaString>(name));
        m.add_function("win", |lua, (_, GivenValue(options)): (AnyUserData, GivenValue)| {
            no_run_here(lua, "world:win")?;
            ask(lua, "world:win", Ask::Won { fade: fade_of(options, "world:win")? })
        });
        m.add_function("lose", |lua, (_, GivenValue(options)): (AnyUserData, GivenValue)| {
            no_run_here(lua, "world:lose")?;
            ask(lua, "world:lose", Ask::Lost { fade: fade_of(options, "world:lose")? })
        });
        m.add_function("new_game", |lua, (_, GivenValue(options)): (AnyUserData, GivenValue)| {
            if !look(lua, |h| Ok(h.on_title()))? {
                return Err(mlua::Error::runtime("world:new_game: from the game's own title only (c.title_screen.level): in a run world:restart() begins the level again"));
            }
            ask(lua, "world:new_game", Ask::NewGame { fade: fade_of(options, "world:new_game")? })
        });
        // Asked of the machine, not of the run: a window does it.
        m.add_function("open", |lua, (_, page): (AnyUserData, String)| {
            let Some(&page) = crate::game::PAGES.iter().find(|&&p| p == page) else {
                return Err(mlua::Error::runtime(format!("world:open: no page called '{page}' (the engine's: {})", crate::game::PAGES.join(", "))));
            };
            if page == "pause" {
                no_run_here(lua, "world:open(\"pause\")")?;
            }
            request(lua, "world:open", crate::game::Request::Open(page))
        });
        m.add_function("quit", |lua, _: AnyUserData| request(lua, "world:quit", crate::game::Request::Quit));
        m.add_function("to_title", |lua, _: AnyUserData| {
            no_run_here(lua, "world:to_title")?;
            request(lua, "world:to_title", crate::game::Request::ToTitle)
        });
    }
}

/// On the game's own title an ask that would leave a level of a run is a
/// mistake: it is no run.
fn no_run_here(lua: &Lua, what: &str) -> mlua::Result<()> {
    match look(lua, |h| Ok(h.on_title()))? {
        true => Err(mlua::Error::runtime(format!("{what}: the title (c.title_screen.level) is no run: world:new_game() begins one, world:load(slot) one from a save"))),
        false => Ok(()),
    }
}

/// `new` asked of the machine the game is played on, once however often.
fn request(lua: &Lua, what: &str, new: crate::game::Request) -> mlua::Result<()> {
    change(lua, what, |h| {
        if !h.requests.contains(&new) {
            h.requests.push(new);
        }
        Ok(())
    })
}

/// An ask's place and options (`world:go_to`, `world:restart`): `(place)`,
/// `(place, options)` or `(options)` alone (a place is a name, so a table
/// there is the options); the options' fade, and `fresh` (the level from
/// its file, what it kept forgotten: `lua::memory`).
fn place_and_fade(lua: &Lua, place: Value, options: Value, what: &str) -> mlua::Result<(Option<String>, Option<f32>, bool)> {
    let (place, options) = match place {
        Value::Table(_) if options.is_nil() => (None, place),
        Value::Nil => (None, options),
        place => (Some(lua.unpack(place)?), options),
    };
    let fresh = match &options {
        Value::Table(t) => match t.raw_get::<Value>("fresh")? {
            Value::Nil => false,
            Value::Boolean(b) => b,
            other => return Err(mlua::Error::runtime(format!("{what}: fresh is true or false (true: the level from its file, what it kept of earlier visits forgotten), not {}", shown(&other)))),
        },
        _ => false,
    };
    Ok((place, options_fade(options, what, &["fade", "fresh"])?, fresh))
}

/// `world:save`'s options, `{ label = "..." }`: the label, one line of at
/// most `MOST_LABEL` characters.
fn label_of(options: Value) -> mlua::Result<Option<String>> {
    let what = "world:save";
    let t = match options {
        Value::Nil => return Ok(None),
        Value::Table(t) => t,
        other => return Err(mlua::Error::runtime(format!("{what}: the options are a table, {{ label = \"text\" }}, not {}", shown(&other)))),
    };
    known_keys(&t, &["label"], what)?;
    match t.raw_get::<Value>("label")? {
        Value::Nil => Ok(None),
        Value::String(s) => {
            let label = s.to_str()?.to_string();
            match (label.chars().count(), label.chars().any(char::is_control)) {
                (n, _) if n > MOST_LABEL => Err(mlua::Error::runtime(format!("{what}: label is one line of {MOST_LABEL} characters at most, this is {n}"))),
                (_, true) => Err(mlua::Error::runtime(format!("{what}: label is one line: no line breaks or control characters in it"))),
                _ => Ok(Some(label)),
            }
        }
        other => Err(mlua::Error::runtime(format!("{what}: label is a string, not {}", shown(&other)))),
    }
}

/// The most characters a save's label has.
const MOST_LABEL: usize = 80;

/// An ask's options, `{ fade = seconds }`: this change's fade, in the
/// sense and range of `c.transition.fade` (seconds out, as many in; 0: a
/// cut). None: the setting's for the ask's reason.
fn fade_of(options: Value, what: &str) -> mlua::Result<Option<f32>> {
    options_fade(options, what, &["fade"])
}

/// `fade_of` of options that may hold `keys`.
fn options_fade(options: Value, what: &str, keys: &[&str]) -> mlua::Result<Option<f32>> {
    let t = match options {
        Value::Nil => return Ok(None),
        Value::Table(t) => t,
        other => return Err(mlua::Error::runtime(format!("{what}: the options are a table, {{ fade = seconds }}, not {}", shown(&other)))),
    };
    known_keys(&t, keys, what)?;
    let range = crate::config::bound("transition.fade").expect("c.transition.fade has a range");
    match t.raw_get::<Value>("fade")? {
        Value::Nil => Ok(None),
        Value::Integer(i) if range.holds(i as f64) => Ok(Some(i as f32)),
        Value::Number(n) if range.holds(n) => Ok(Some(n as f32)),
        other => Err(mlua::Error::runtime(format!("{what}: fade is seconds {} (0: a cut), not {}", range.words(), shown(&other)))),
    }
}
