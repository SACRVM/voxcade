//! `level`: the level being played, its voxels and things; its areas and its camera.

use super::*;

/// `level`: the level being played.
pub struct LevelView;

/// `level:spawn`'s options: `{ z =, facing =, state =, name =, props = }`.
struct SpawnOptions {
    z: Option<f32>,
    facing: Option<Vec3>,
    state: Option<Table>,
    name: Option<String>,
    props: Option<Table>,
}

fn spawn_options(v: Value) -> mlua::Result<SpawnOptions> {
    let mut o = SpawnOptions { z: None, facing: None, state: None, name: None, props: None };
    match v {
        Value::Nil => {}
        Value::Table(t) => {
            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair?;
                match k.to_string()?.as_str() {
                    "z" => {
                        o.z = Some(match v {
                            Value::Integer(i) => i as f32,
                            Value::Number(n) => n as f32,
                            _ => return Err(mlua::Error::runtime("level:spawn: z is a number")),
                        })
                    }
                    "facing" => o.facing = Some(to_dir(&v, "level:spawn: facing")?),
                    "state" => match v {
                        Value::Table(s) => o.state = Some(s),
                        _ => return Err(mlua::Error::runtime("level:spawn: state is a table: the thing's first state")),
                    },
                    "name" => match v {
                        Value::String(n) => o.name = Some(n.to_str()?.to_string()),
                        _ => return Err(mlua::Error::runtime("level:spawn: name is a string: what level:thing(name) finds it by")),
                    },
                    "props" => match v {
                        Value::Table(p) => o.props = Some(p),
                        _ => return Err(mlua::Error::runtime("level:spawn: props is a table: what the game's code reads of this one thing, over its kind's")),
                    },
                    other => return Err(mlua::Error::runtime(format!("level:spawn: no option called '{other}' (z, facing, state, name, props)"))),
                }
            }
        }
        Value::Integer(_) | Value::Number(_) => return Err(mlua::Error::runtime("level:spawn: the fourth is a table of options: level:spawn(kind, x, y, { z = 3 })")),
        other => return Err(mlua::Error::runtime(format!("level:spawn: the options are a table, not a {}", other.type_name()))),
    }
    Ok(o)
}

/// A thing's own props are those its kind declares (`c.things.NAME.props`),
/// each of the kind its default is.
fn declared_props(kind: &str, declared: &toml::Table, given: &Table) -> mlua::Result<()> {
    for pair in given.pairs::<Value, Value>() {
        let (key, v) = pair?;
        let key = match key {
            Value::String(s) => s.to_str()?.to_string(),
            other => return Err(mlua::Error::runtime(format!("level:spawn: props are named, not a {} key", other.type_name()))),
        };
        let Some(default) = declared.get(&key) else {
            let names: Vec<&str> = declared.keys().map(String::as_str).collect();
            let has = if names.is_empty() { " declares none".to_string() } else { format!(": {}", names.join(", ")) };
            return Err(mlua::Error::runtime(format!("level:spawn: props.{key}: a {kind} has no such prop (c.things.{kind}.props{has})")));
        };
        let same = matches!(
            (default, &v),
            (toml::Value::Integer(_) | toml::Value::Float(_), Value::Integer(_) | Value::Number(_))
                | (toml::Value::String(_), Value::String(_))
                | (toml::Value::Boolean(_), Value::Boolean(_))
                | (toml::Value::Array(_) | toml::Value::Table(_), Value::Table(_))
        );
        if !same {
            let is = match default {
                toml::Value::Integer(_) | toml::Value::Float(_) => "number",
                toml::Value::String(_) => "string",
                toml::Value::Boolean(_) => "boolean",
                _ => "table",
            };
            return Err(mlua::Error::runtime(format!("level:spawn: props.{key} is a {is} by its kind (c.things.{kind}.props.{key} = {default}), not a {}", v.type_name())));
        }
    }
    Ok(())
}

impl UserData for LevelView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_function_get("state", |lua, _| {
            level_here(lua, "level.state")?;
            plain_state(lua, "voxcade.level")
        });
        f.add_field_function_get("name", |lua, _| {
            level_here(lua, "level.name")?;
            look(lua, |h| Ok(h.levels.list[h.level].name.clone()))
        });
        f.add_field_function_get("number", |lua, _| {
            level_here(lua, "level.number")?;
            look(lua, |h| Ok(h.level + 1))
        });
        f.add_field_function_get("next", |lua, _| {
            level_here(lua, "level.next")?;
            look(lua, |h| Ok(h.levels.list.get(h.level + 1).map(|l| l.name.clone())))
        });
        f.add_field_function_get("visits", |lua, _| {
            level_here(lua, "level.visits")?;
            look(lua, |h| Ok(h.visits[h.level]))
        });
        f.add_field_function_get("kept", |lua, _| {
            level_here(lua, "level.kept")?;
            let name = look(lua, |h| Ok(h.levels.list[h.level].name.clone()))?;
            let all: Table = lua.named_registry_value("voxcade.kept")?;
            let kept = match all.raw_get::<Option<Table>>(name.as_str())? {
                Some(t) => t,
                None => {
                    let t = lua.create_table()?;
                    all.raw_set(name.as_str(), &t)?;
                    t
                }
            };
            if looking(lua) { lua.named_registry_value::<Function>("voxcade.readonly")?.call(kept) } else { Ok(Value::Table(kept)) }
        });
        f.add_field_function_get("arrived", |lua, _| {
            level_here(lua, "level.arrived")?;
            let arrived = look(lua, |h| Ok(h.arrived.clone()))?;
            let t = place_table(lua, &arrived)?;
            t.raw_set("name", arrived.name.as_str())?;
            Ok(t)
        });
        f.add_field_function_get("places", |lua, _| {
            level_here(lua, "level.places")?;
            let (places, start) = look(lua, |h| Ok((h.levels.list[h.level].places.clone(), h.start(h.level))))?;
            let t = lua.create_table()?;
            for p in places.iter().chain([&start]) {
                t.raw_set(p.name.as_str(), place_table(lua, p)?)?;
            }
            Ok(t)
        });
        f.add_field_function_get("areas", |lua, _| {
            level_here(lua, "level.areas")?;
            let areas = look(lua, |h| Ok(h.levels.list[h.level].areas.clone()))?;
            let t = lua.create_table()?;
            for (i, a) in areas.iter().enumerate() {
                t.raw_set(a.name.as_str(), AreaView(i))?;
            }
            readonly(lua, t, "level.areas")
        });
        f.add_field_function_get("size", |lua, _| {
            level_here(lua, "level.size")?;
            let size = look(lua, |h| Ok(h.host.sim().world.size.as_vec3()))?;
            point(lua, size)
        });
        f.add_field_function_get("camera", |lua, _| {
            level_here(lua, "level.camera")?;
            if let Some(camera) = lua.named_registry_value::<Option<AnyUserData>>("voxcade.camera")? {
                return Ok(camera);
            }
            let camera = lua.create_userdata(CameraView)?;
            lua.set_named_registry_value("voxcade.camera", &camera)?;
            Ok(camera)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("level"));
        m.add_function("spawn", |lua, (_, kind, x, y, options): (AnyUserData, String, f32, f32, Value)| {
            level_here(lua, "level:spawn")?;
            let o = spawn_options(options)?;
            // The options are checked before the thing comes.
            let first = match &o.state {
                Some(first) => crate::lua::value::capture(&Value::Table(first.clone()), "level:spawn's state", &mut Default::default(), true).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?,
                None => None,
            };
            let props = match &o.props {
                Some(p) => {
                    let declared = look(lua, |h| Ok(h.kinds[h.kind(&kind)?].props.clone()))?;
                    declared_props(&kind, &declared, p)?;
                    crate::lua::value::capture(&Value::Table(p.clone()), "level:spawn's props", &mut Default::default(), false).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?
                }
                None => None,
            };
            if let Some(name) = &o.name
                && look(lua, |h| Ok(h.named(name).is_some()))?
            {
                return Err(mlua::Error::runtime(format!("level:spawn: a thing called '{name}' is in the level already (a name is one thing's)")));
            }
            if kind == crate::grid::PIECE {
                return Err(mlua::Error::runtime("level:spawn: \"piece\" is the kind of the models and lights a level's file sets down; spawn a kind of c.things"));
            }
            let id = change(lua, "level:spawn", |h| {
                let k = h.kind(&kind)?;
                if o.facing.is_some() && !h.kinds[k].scripted {
                    return Err(mlua::Error::runtime(format!("level:spawn: facing: a {kind} turns by its ready-made rules (its behavior); facing is for kinds the script moves")));
                }
                let id = h.host.spawn(k, x, y, o.z).map_err(|e| mlua::Error::runtime(format!("level:spawn: {e}")))?;
                if let Some(way) = o.facing {
                    h.moved(id.raw(), "level:spawn: facing")?.0.face(way);
                }
                if let Some(name) = o.name {
                    h.names.insert(id.raw(), name);
                }
                if let Some(props) = props {
                    h.props.insert(id.raw(), props);
                }
                Ok(id.raw())
            })?;
            // The ready-made rules may take it as the player (a hero where
            // none stood): its first state is then the player's.
            let id = folded(lua, id);
            if let Some(first) = first
                && let Value::Table(fresh) = crate::lua::value::restore(lua, &first).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?
                && let Value::Table(state) = thing_state(lua, id)?
            {
                for pair in fresh.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    state.set(k, v)?;
                }
            }
            handle(lua, id)
        });
        m.add_function("thing", |lua, (_, v): (AnyUserData, Value)| {
            level_here(lua, "level:thing")?;
            let id = match &v {
                Value::String(name) => {
                    let name = name.to_str()?.to_string();
                    match look(lua, |h| Ok(h.named(&name)))? {
                        Some(id) => id,
                        None => return Ok(None),
                    }
                }
                _ => thing_id(&v, "level:thing")?,
            };
            if look(lua, |h| Ok(h.seen(id).is_some() && !h.gone.contains(&id)))? { Ok(Some(handle(lua, id)?)) } else { Ok(None) }
        });
        m.add_function("things", |lua, (_, kind): (AnyUserData, Option<String>)| {
            level_here(lua, "level:things")?;
            let ids = look(lua, |h| {
                let pick = kind.map(|k| h.pick(&k, "level:things")).transpose()?;
                Ok(h.ids_of(pick.as_ref()))
            })?;
            let list = lua.create_table()?;
            for (n, id) in ids.into_iter().enumerate() {
                list.raw_set(n + 1, handle(lua, id.raw())?)?;
            }
            Ok(list)
        });
        // The players playing in this level, in seat order.
        m.add_function("players", |lua, _: AnyUserData| {
            level_here(lua, "level:players")?;
            let seats = look(lua, |h| Ok((1..=h.seats.len()).filter(|&n| h.playing(n)).collect::<Vec<_>>()))?;
            let list = lua.create_table()?;
            for (n, seat) in seats.into_iter().enumerate() {
                list.raw_set(n + 1, handle(lua, seat_handle(seat))?)?;
            }
            Ok(list)
        });
        m.add_function("count", |lua, (_, kind): (AnyUserData, Option<String>)| {
            level_here(lua, "level:count")?;
            look(lua, |h| {
                let pick = kind.map(|k| h.pick(&k, "level:count")).transpose()?;
                Ok(h.ids_of(pick.as_ref()).len())
            })
        });
        m.add_function("explode", |lua, (_, at, radius): (AnyUserData, Value, f32)| {
            level_here(lua, "level:explode")?;
            let at = to_point(lua, &at, "level:explode")?;
            change(lua, "level:explode", |h| {
                h.host.sim_mut().explode(at, radius);
                Ok(())
            })
        });
        m.add_function("clear_rounds", |lua, (_, team): (AnyUserData, Option<String>)| {
            level_here(lua, "level:clear_rounds")?;
            change(lua, "level:clear_rounds", |h| {
                if let Some(t) = &team
                    && t != crate::sim::things::PLAYER_TEAM
                    && !h.kinds.iter().any(|k| k.team.as_deref() == Some(t.as_str()))
                    && !h.seats.iter().any(|s| s.team.as_deref() == Some(t.as_str()))
                {
                    let seats = h.seats.iter().filter_map(|s| s.team.as_deref());
                    let mut teams: Vec<&str> = h.kinds.iter().filter_map(|k| k.team.as_deref()).chain(seats).chain([crate::sim::things::PLAYER_TEAM]).collect();
                    teams.sort_unstable();
                    teams.dedup();
                    return Err(mlua::Error::runtime(format!("level:clear_rounds: no team called '{t}' (the kinds' teams and the players': {})", teams.join(", "))));
                }
                h.host.clear_rounds(team.as_deref()).map_err(mlua::Error::runtime)
            })
        });
        m.add_function("sound", |lua, (_, name, at, volume): (AnyUserData, String, Value, Option<f32>)| {
            level_here(lua, "level:sound")?;
            let at = to_point(lua, &at, "level:sound")?;
            change(lua, "level:sound", |h| {
                let Some(&event) = h.sounds.iter().find(|s| **s == name) else { return Err(mlua::Error::runtime(format!("no sound called '{name}': a sound is a file, sounds/{name}.toml (the game's: {})", h.sounds.join(", ")))) };
                h.host.sim_mut().sound(event, at, volume.unwrap_or(1.0));
                Ok(())
            })
        });
        m.add_function("effect", |lua, (_, model, at, fps): (AnyUserData, String, Value, Option<f32>)| {
            level_here(lua, "level:effect")?;
            // Where a thing stands, or a point: the model's feet there.
            let at = match &at {
                Value::UserData(ud) if ud.is::<ThingRef>() => {
                    let id = ud.borrow::<ThingRef>()?.0;
                    look(lua, |h| Ok(h.thing(id)?.body.pos))?
                }
                _ => to_point(lua, &at, "level:effect")?,
            };
            change(lua, "level:effect", |h| {
                if !h.frames.contains_key(&model) {
                    return Err(mlua::Error::runtime(format!("level:effect: no model called '{model}' (a kind's model, or one in c.models)")));
                }
                h.effects.push(crate::lua::host::Shown { model, at, age: 0, fps: fps.unwrap_or(crate::sim::things::EFFECT_FPS).max(0.1) });
                Ok(())
            })
        });
        m.add_function("block", |lua, (_, x, y, z): (AnyUserData, f32, f32, f32)| {
            level_here(lua, "level:block")?;
            look(lua, |h| {
                let Some(cells) = h.host.sim().world.cells.as_ref() else { return Ok(None) };
                let bits = cells.at(Vec3::new(x, y, z) + 0.5);
                Ok(BLOCK_KINDS.iter().find(|(_, f)| bits & f != 0).map(|(n, _)| *n))
            })
        });
        m.add_function("voxel", |lua, (_, x, y, z): (AnyUserData, i32, i32, i32)| {
            level_here(lua, "level:voxel")?;
            look(lua, |h| Ok(h.host.sim().world.get(IVec3::new(x, y, z))))
        });
        m.add_function("solid", |lua, (_, x, y, z): (AnyUserData, i32, i32, i32)| {
            level_here(lua, "level:solid")?;
            look(lua, |h| Ok(h.host.sim().world.solid(IVec3::new(x, y, z))))
        });
        m.add_function("set_voxel", |lua, (_, x, y, z, v): (AnyUserData, i32, i32, i32, Value)| {
            level_here(lua, "level:set_voxel")?;
            change(lua, "level:set_voxel", |h| {
                let size = h.host.sim().world.size;
                if !h.host.sim().world.in_bounds(IVec3::new(x, y, z)) {
                    return Err(mlua::Error::runtime(format!("level:set_voxel: {x}, {y}, {z} is outside the level (x 0 to {}, y 0 to {}, z 0 to {}: level.size)", size.x - 1, size.y - 1, size.z - 1)));
                }
                let v = palette_index(&mut h.host.sim_mut().world, &v, "level:set_voxel")?;
                h.host.sim_mut().world.set(IVec3::new(x, y, z), v);
                Ok(())
            })
        });
        m.add_function("fill", |lua, (_, x0, y0, z0, x1, y1, z1, v): (AnyUserData, i32, i32, i32, i32, i32, i32, Value)| {
            level_here(lua, "level:fill")?;
            change(lua, "level:fill", |h| {
                let world = &mut h.host.sim_mut().world;
                let v = palette_index(world, &v, "level:fill")?;
                let (lo, hi) = (IVec3::new(x0, y0, z0).min(IVec3::new(x1, y1, z1)).max(IVec3::ZERO), IVec3::new(x0, y0, z0).max(IVec3::new(x1, y1, z1)).min(world.size - 1));
                for z in lo.z..=hi.z {
                    for y in lo.y..=hi.y {
                        for x in lo.x..=hi.x {
                            world.set(IVec3::new(x, y, z), v);
                        }
                    }
                }
                Ok(())
            })
        });
        m.add_function("ground", |lua, (_, x, y): (AnyUserData, i32, i32)| {
            level_here(lua, "level:ground")?;
            look(lua, |h| Ok(h.host.sim().world.column_top(x, y)))
        });
    }
}

/// One of the level's areas (`level.areas.NAME`), by its place in the
/// level's list: a named box, only to read.
#[derive(Clone, Copy)]
pub struct AreaView(pub usize);

impl AreaView {
    fn area(lua: &Lua, i: usize) -> mlua::Result<crate::levels::Area> {
        look(lua, |h| h.levels.list[h.level].areas.get(i).cloned().ok_or_else(|| mlua::Error::runtime("the area is gone: its level was left")))
    }
}

impl UserData for AreaView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_method_get("name", |lua, a| Ok(Self::area(lua, a.0)?.name));
        f.add_field_method_get("x", |lua, a| Ok(Self::area(lua, a.0)?.at.x));
        f.add_field_method_get("y", |lua, a| Ok(Self::area(lua, a.0)?.at.y));
        f.add_field_method_get("z", |lua, a| Ok(Self::area(lua, a.0)?.at.z));
        f.add_field_method_get("size", |lua, a| point(lua, Self::area(lua, a.0)?.size.as_vec3()));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |lua, a, ()| Ok(format!("area {}", Self::area(lua, a.0)?.name)));
        // A thing's place (where its feet stand, its pivot), or a point:
        // inside the box or not; a point without z, at any height.
        m.add_method("holds", |lua, a, v: Value| {
            let area = Self::area(lua, a.0)?;
            let (x, y, z) = match &v {
                Value::UserData(ud) if ud.is::<ThingRef>() => {
                    let id = ud.borrow::<ThingRef>()?.0;
                    let at = look(lua, |h| Ok(h.thing(id)?.body.pos))?;
                    (at.x, at.y, Some(at.z))
                }
                Value::Table(t) => {
                    let n = |k: &str| -> mlua::Result<Option<f32>> { t.get::<Option<f32>>(k) };
                    match (n("x")?, n("y")?) {
                        (Some(x), Some(y)) => (x, y, n("z")?),
                        _ => return Err(mlua::Error::runtime("area:holds: a thing, or a point { x =, y =, z = } (z left out: any height)")),
                    }
                }
                other => return Err(mlua::Error::runtime(format!("area:holds: a thing, or a point {{ x =, y =, z = }}, not a {}", other.type_name()))),
            };
            Ok(area.holds(x, y, z))
        });
    }
}

/// `level.camera`: where the camera looks.
pub struct CameraView;

impl UserData for CameraView {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("camera"));
        m.add_function("follow", |lua, (_, t): (AnyUserData, Value)| {
            level_here(lua, "level.camera:follow")?;
            // Nothing: the players again (the first playing; several, their middle).
            let id = match t {
                Value::Nil => look(lua, |h| Ok((1..=h.seats.len()).filter(|&n| h.playing(n)).find_map(|n| h.target(seat_handle(n)))))?,
                t => Some(thing_id(&t, "level.camera:follow")?),
            };
            let Some(id) = id else { return Ok(()) };
            change(lua, "level.camera:follow", |h| {
                h.thing(id)?;
                let id = h.target(id).unwrap_or(id);
                h.host.follow(Id::from_raw(id)).map_err(|e| mlua::Error::runtime(format!("level.camera:follow: {e}")))
            })
        });
        m.add_function("look_at", |lua, (_, x, y): (AnyUserData, f32, f32)| {
            level_here(lua, "level.camera:look_at")?;
            change(lua, "level.camera:look_at", |h| h.host.look_at(x, y).map_err(|e| mlua::Error::runtime(format!("level.camera:look_at: {e}"))))
        });
    }
}
