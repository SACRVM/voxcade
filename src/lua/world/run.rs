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
            let slots = look(lua, |h| Ok(h.slots.clone()))?;
            let list = lua.create_table()?;
            for (n, info) in slots.into_iter().enumerate() {
                if let Some(info) = info {
                    let t = lua.create_table()?;
                    t.raw_set("level", info.level)?;
                    t.raw_set("seconds", info.ticks / 60)?;
                    list.raw_set(n + 1, t)?;
                }
            }
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
        m.add_function("go_to", |lua, (_, level, place): (AnyUserData, Value, Option<String>)| {
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
                if let Some(p) = &place {
                    check_place(&h.levels.list[index], p, "world:go_to")?;
                }
                Ok(h.levels.list[index].name.clone())
            })?;
            ask(lua, "world:go_to", Ask::GoTo { level: index, place })
        });
        m.add_function("restart", |lua, (_, place): (AnyUserData, Option<String>)| {
            if let Some(p) = &place {
                look(lua, |h| check_place(&h.levels.list[h.level], p, "world:restart"))?;
            }
            ask(lua, "world:restart", Ask::Restart { place })
        });
        m.add_function("save", |lua, (_, n): (AnyUserData, i64)| {
            if phase(lua).is_some() {
                return Err(mlua::Error::runtime("world:save: only while a level is played (on_level_start, on_tick): there is none to save here"));
            }
            change(lua, "world:save", |h| {
                let n = slot(h, n, "world:save")?;
                if !h.saving.contains(&n) {
                    h.saving.push(n);
                }
                Ok(())
            })
        });
        m.add_function("load", |lua, (_, n): (AnyUserData, i64)| {
            let n = look(lua, |h| {
                let n = slot(h, n, "world:load")?;
                match h.slots[n as usize - 1] {
                    Some(_) => Ok(n),
                    None => Err(mlua::Error::runtime(format!("world:load: slot {n} is empty (world.saves[{n}] tells before)"))),
                }
            })?;
            ask(lua, "world:load", Ask::Load { slot: Some(n) })
        });
        m.add_function("hitstop", |lua, (_, ticks): (AnyUserData, u32)| {
            change(lua, "world:hitstop", |h| {
                let sim = h.host.sim_mut();
                sim.freeze = sim.freeze.max(ticks.min(MAX_HITSTOP));
                Ok(())
            })
        });
        m.add_function("read", |lua, (_, name): (AnyUserData, String)| lua.named_registry_value::<Function>("voxcade.read")?.call::<mlua::LuaString>(name));
        m.add_function("win", |lua, _: AnyUserData| ask(lua, "world:win", Ask::Won));
        m.add_function("lose", |lua, _: AnyUserData| ask(lua, "world:lose", Ask::Lost));
    }
}
