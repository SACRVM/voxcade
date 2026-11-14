//! A thing as a script holds it (`player` too): its body, its state, what it does.

use super::*;

/// A thing as a script holds it: its id. `player` is one too (`PLAYER`):
/// everything a thing has, its state the run's, its figure the host's
/// from level to level.
#[derive(Clone, Copy, Debug)]
pub struct ThingRef(pub u32);

impl UserData for ThingRef {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field(MetaMethod::Type, "thing");
        f.add_field_method_get("id", |_, t| Ok(handle_seat(t.0).is_none().then_some(t.0)));
        f.add_field_method_get("seat", |_, t| Ok(handle_seat(t.0)));
        // A player's keys (`input` is seat 1's) and what it plays with.
        f.add_field_method_get("input", |lua, t| {
            let Some(seat) = handle_seat(t.0) else { return Err(mlua::Error::runtime("thing.input: only a player has keys (thing.seat)")) };
            let view = look(lua, |h| {
                if !h.ticking {
                    return Err(mlua::Error::runtime("player.input: the keys held are only there in on_tick"));
                }
                let s = &h.seats[seat - 1];
                Ok(InputView { now: s.input, before: s.before, actions: h.actions.clone(), held_for: s.held_for })
            })?;
            lua.create_userdata(view)
        });
        f.add_field_method_get("controls", |lua, t| match handle_seat(t.0) {
            Some(seat) => lua.create_userdata(ControlsView(seat)),
            None => Err(mlua::Error::runtime("thing.controls: only a player plays with something (thing.seat)")),
        });
        f.add_field_method_get("kind", |lua, t| look(lua, |h| Ok(h.kind_name(h.thing(t.0)?.kind).map(str::to_string))));
        f.add_field_method_get("x", |lua, t| look(lua, |h| Ok(h.thing(t.0)?.body.pos.x)));
        f.add_field_method_get("y", |lua, t| look(lua, |h| Ok(h.thing(t.0)?.body.pos.y)));
        f.add_field_method_get("z", |lua, t| look(lua, |h| Ok(h.thing(t.0)?.body.pos.z)));
        f.add_field_method_get("on_ground", |lua, t| look(lua, |h| Ok(h.thing(t.0)?.body.on_ground)));
        // The way to a wall beside it (within half a voxel: bodies move in
        // steps of less), higher than a step (the way it faces first), or
        // nil.
        f.add_field_method_get("wall", |lua, t| {
            let way = look(lua, |h| {
                let seen = h.thing(t.0)?;
                let (b, half) = (seen.body, seen.half);
                let sim = h.host.sim();
                let lift = (sim.cfg.physics.step.max(0) as f32 + 0.01).min(b.height * 0.5);
                let ways = [b.look, glam::IVec2::X, glam::IVec2::NEG_X, glam::IVec2::Y, glam::IVec2::NEG_Y];
                Ok(ways.into_iter().filter(|w| *w != glam::IVec2::ZERO).find(|w| {
                    let at = b.pos + w.as_vec2().extend(0.0) * 0.5 + Vec3::Z * lift;
                    crate::sim::body_hits(&sim.world, at, half, b.height - lift)
                }))
            })?;
            match way {
                Some(w) => {
                    let t = lua.create_table()?;
                    t.raw_set("x", w.x)?;
                    t.raw_set("y", w.y)?;
                    Ok(Value::Table(t))
                }
                None => Ok(Value::Nil),
            }
        });
        f.add_field_method_get("facing", |lua, t| {
            let way = look(lua, |h| Ok(h.thing(t.0)?.body.look))?;
            let w = lua.create_table()?;
            w.raw_set("x", way.x)?;
            w.raw_set("y", way.y)?;
            Ok(w)
        });
        f.add_field_method_get("state", |lua, t| thing_state(lua, t.0));
        f.add_field_method_get("name", |lua, t| look(lua, |h| Ok(h.here.names.get(&t.0).cloned())));
        f.add_field_method_get("families", |lua, t| {
            let families = look(lua, |h| Ok(h.kinds.get(h.thing(t.0)?.kind).map(|k| k.families.clone()).unwrap_or_default()))?;
            let set = lua.create_table()?;
            for f in families {
                set.raw_set(f, true)?;
            }
            readonly(lua, set, "thing.families")
        });
        f.add_field_method_get("props", |lua, t| {
            let cache: Table = lua.named_registry_value("voxcade.props")?;
            if let Some(p) = cache.raw_get::<Option<Value>>(t.0)? {
                return Ok(p);
            }
            let (kind, own) = look(lua, |h| Ok((h.kinds.get(h.thing(t.0)?.kind).map(|k| k.props.clone()).unwrap_or_default(), h.here.props.get(&t.0).cloned())))?;
            let props = lua.create_table()?;
            for (k, v) in &kind {
                props.raw_set(k.as_str(), from_toml(lua, v)?)?;
            }
            if let Some(own) = own
                && let Value::Table(own) = crate::lua::value::restore(lua, &own).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?
            {
                for pair in own.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    props.raw_set(k, v)?;
                }
            }
            let props = readonly(lua, props, "thing.props")?;
            cache.raw_set(t.0, &props)?;
            Ok(props)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, a, b: AnyUserData| Ok(b.borrow::<ThingRef>().is_ok_and(|b| b.0 == a.0)));
        // A thing's fields are read, never assigned: the body is the
        // physics' (`thing:place`), the game's own data goes in
        // `thing.state`.
        m.add_meta_method(MetaMethod::NewIndex, |_, t, (key, _): (Value, Value)| -> mlua::Result<()> {
            let of = if handle_seat(t.0).is_some() { "player" } else { "thing" };
            Err(mlua::Error::runtime(format!("{of}.{} cannot be set: a thing's fields are only to read (thing:place(x, y, z) moves it, the game's own data goes in {of}.state)", key_shown(&key))))
        });
        // A name a thing does not have is a mistake, not a nil.
        m.add_meta_method(MetaMethod::Index, |_, t, key: Value| -> mlua::Result<Value> {
            let of = if handle_seat(t.0).is_some() { "player" } else { "thing" };
            Err(no_such(of, &key, Some(if of == "player" { "player.state" } else { "thing.state" })))
        });
        m.add_meta_method(MetaMethod::ToString, |lua, t, ()| {
            match handle_seat(t.0) {
                Some(1) => return Ok("player".to_string()),
                Some(seat) => return Ok(format!("player {seat}")),
                None => {}
            }
            let kind = look(lua, |h| Ok(h.host.get(Id::from_raw(t.0)).and_then(|x| h.kind_name(x.kind).map(str::to_string)))).ok().flatten();
            Ok(match kind {
                Some(kind) => format!("thing {} ({kind})", t.0),
                None => format!("thing {}", t.0),
            })
        });
        m.add_method("walk", |lua, t, (dir, speed): (Value, f32)| {
            let wish = to_dir(&dir, "thing:walk")?.normalize_or_zero();
            change(lua, "thing:walk", |h| {
                let (body, walk) = h.moved(t.0, "thing:walk")?;
                *walk = Walk { wish, speed };
                body.face(wish);
                Ok(())
            })
        });
        m.add_method("face", |lua, t, dir: Value| {
            let way = to_dir(&dir, "thing:face")?;
            change(lua, "thing:face", |h| {
                let Some(id) = h.target(t.0) else { return Err(h.no_target(t.0, "player:face")) };
                h.thing(t.0)?;
                h.host.face(Id::from_raw(id), way).map_err(|e| mlua::Error::runtime(format!("thing:face: {e}")))
            })
        });
        m.add_method("jump", |lua, t, height: f32| {
            change(lua, "thing:jump", |h| {
                let gravity = h.host.sim().cfg.physics.gravity;
                let b = h.moved(t.0, "thing:jump")?.0;
                if b.on_ground {
                    b.vel.z = (2.0 * gravity * height.max(0.0)).sqrt();
                    b.on_ground = false;
                }
                Ok(())
            })
        });
        m.add_method("touching", |lua, t, what: Option<String>| {
            let ids = look(lua, |h| {
                h.thing(t.0)?;
                let pick = what.map(|w| h.pick(&w, "thing:touching")).transpose()?;
                let others = h.contacts_of(t.0).into_iter().map(|(a, b)| if a == t.0 { b } else { a });
                Ok(others.filter(|&o| pick.as_ref().is_none_or(|p| h.is(o, p))).collect::<Vec<u32>>())
            })?;
            handles(lua, ids)
        });
        m.add_method("in_block", |lua, t, kind: String| {
            let flag = block_flag(&kind, "thing:in_block")?;
            look(lua, |h| h.in_block(t.0, flag))
        });
        m.add_method("launch", |lua, t, (dir, speed, rise): (Value, f32, Option<f32>)| {
            let way = to_dir(&dir, "thing:launch")?.normalize_or_zero();
            change(lua, "thing:launch", |h| {
                let gravity = h.host.sim().cfg.physics.gravity;
                let Some(id) = h.target(t.0) else { return Err(h.no_target(t.0, "player:launch")) };
                h.thing(t.0)?;
                let up = rise.map_or(0.0, |r| (2.0 * gravity * r.max(0.0)).sqrt());
                h.host.launch(Id::from_raw(id), way * speed + Vec3::Z * up).map_err(|e| mlua::Error::runtime(format!("thing:launch: {e}")))
            })
        });
        m.add_method("enable", |lua, t, (behavior, on): (String, bool)| {
            change(lua, "thing:enable", |h| {
                let Some(id) = h.target(t.0) else { return Err(h.no_target(t.0, "player:enable")) };
                h.thing(t.0)?;
                h.host.enable(Id::from_raw(id), &behavior, on).map_err(|e| mlua::Error::runtime(format!("thing:enable: {e}")))
            })
        });
        m.add_method("bounce", |lua, t, height: f32| {
            change(lua, "thing:bounce", |h| {
                let gravity = h.host.sim().cfg.physics.gravity;
                let Some(id) = h.target(t.0) else { return Err(h.no_target(t.0, "player:bounce")) };
                h.thing(t.0)?;
                let speed = (2.0 * gravity * height.max(0.0)).sqrt();
                h.host.bounce(Id::from_raw(id), speed).map_err(|e| mlua::Error::runtime(format!("thing:bounce: {e}")))
            })
        });
        m.add_method("ahead", |lua, t, distance: f32| {
            let p = look(lua, |h| {
                let b = h.thing(t.0)?.body;
                Ok(b.pos + b.facing * distance + Vec3::Z)
            })?;
            point(lua, p)
        });
        m.add_method("near", |lua, t, (other, distance): (Value, f32)| {
            let o = thing_id(&other, "thing:near")?;
            look(lua, |h| Ok(h.thing(t.0)?.body.pos.distance(h.thing(o)?.body.pos) < distance))
        });
        m.add_method("place", |lua, t, (first, y, z): (Value, Option<f32>, Option<f32>)| {
            // A place (`level.places.west`: where, and the way it faces), a
            // thing (where it stands) or x, y, z.
            let (x, y, z, facing) = match &first {
                Value::UserData(ud) if ud.is::<ThingRef>() => {
                    let other = ud.borrow::<ThingRef>()?.0;
                    let at = look(lua, |h| Ok(h.thing(other)?.body.pos))?;
                    (at.x, at.y, Some(at.z), None)
                }
                Value::Table(p) => {
                    let at = to_point(lua, &first, "thing:place")?;
                    let facing = p.raw_get::<Option<Value>>("facing")?.map(|f| to_dir(&f, "thing:place: facing")).transpose()?;
                    (at.x, at.y, Some(at.z), facing)
                }
                Value::Integer(_) | Value::Number(_) => {
                    let x = match first {
                        Value::Integer(i) => i as f32,
                        Value::Number(n) => n as f32,
                        _ => unreachable!("a number"),
                    };
                    let y = y.ok_or_else(|| mlua::Error::runtime("thing:place: x and y (and z), or a place"))?;
                    (x, y, z, None)
                }
                other => return Err(mlua::Error::runtime(format!("thing:place: x and y (and z), or a place, not a {}", other.type_name()))),
            };
            change(lua, "thing:place", |h| {
                // A player whose figure is not standing yet: where it comes in.
                if let Some(seat) = handle_seat(t.0).filter(|_| h.target(t.0).is_none())
                    && let Some(kind) = h.seat_kind(seat)
                    && h.seats[seat - 1].taken
                {
                    let (half, height) = (h.host.sim().half(), h.kinds[kind].height);
                    let at = match z {
                        Some(z) => Vec3::new(x, y, z),
                        None => crate::sim::stand_on_ground(&h.host.sim().world, Vec3::new(x, y, 0.0), height, half),
                    };
                    let way = facing.map(|f| glam::IVec2::new(f.x.round() as i32, f.y.round() as i32)).filter(|w| *w != glam::IVec2::ZERO);
                    h.seats[seat - 1].coming = Some((at, way));
                    return Ok(());
                }
                let (half, height) = (h.host.sim().half(), h.thing(t.0)?.body.height);
                let at = match z {
                    Some(z) => Vec3::new(x, y, z),
                    None => crate::sim::stand_on_ground(&h.host.sim().world, Vec3::new(x, y, 0.0), height, half),
                };
                // A seat's figure of the ready-made rules: stood there by them.
                if let Some(seat) = handle_seat(t.0).filter(|&n| n > 1)
                    && let Some(figure) = h.host.seat_figure(seat).filter(|&f| h.host.get_mut(f).is_some_and(|m| m.moved.is_none()))
                {
                    h.host.put_figure(figure, at).map_err(|e| mlua::Error::runtime(format!("player:place: {e}")))?;
                    h.here.placed.push(t.0);
                    return Ok(());
                }
                // The player of ready-made rules is stood there by them.
                if t.0 == PLAYER && h.host.player_id().is_none_or(|id| h.host.get_mut(id).is_some_and(|m| m.moved.is_none())) {
                    h.host.put_player(at).map_err(|e| mlua::Error::runtime(format!("player:place: {e}")))?;
                    h.here.placed.push(t.0);
                    return Ok(());
                }
                let body = h.moved(t.0, "thing:place")?.0;
                (body.pos, body.vel) = (at, Vec3::ZERO);
                if let Some(way) = facing {
                    body.face(way);
                }
                // What it touches there it touches without on_touch.
                h.here.placed.push(t.0);
                Ok(())
            })
        });
        m.add_method("attack", |lua, t, (weapon, target): (String, Option<Value>)| {
            let at = match target.filter(|v| !v.is_nil()) {
                Some(v) => {
                    let point = to_point(lua, &v, "thing:attack")?;
                    let id = match &v {
                        Value::UserData(ud) if ud.is::<ThingRef>() => Some(ud.borrow::<ThingRef>()?.0),
                        _ => None,
                    };
                    Some((point, id))
                }
                None => None,
            };
            change(lua, "thing:attack", |h| {
                h.thing(t.0)?;
                let who = h.who(t.0);
                let at = at.map(|(p, id)| (p, id.and_then(|id| h.target(id)).filter(|&id| id != PLAYER).map(Id::from_raw)));
                h.host.attack(who, &weapon, at).map_err(|e| mlua::Error::runtime(format!("thing:attack: {e}")))
            })
        });
        m.add_method("hurt", |lua, t, (damage, by, weapon): (i64, Option<Value>, Option<String>)| {
            if damage < 1 {
                return Err(mlua::Error::runtime("thing:hurt: damage is 1 or more (hit points back: set thing.state.hp)"));
            }
            let by = by.filter(|v| !v.is_nil()).map(|v| thing_id(&v, "thing:hurt: by")).transpose()?;
            change(lua, "thing:hurt", |h| {
                let who = |raw: u32| h.who(raw);
                let (whom, by_handle) = (who(t.0), by);
                let by = by.map(who);
                let kind = h.thing(t.0)?.kind;
                match h.kinds.get(kind) {
                    // The script's own kinds: their hit points, and the hit told.
                    Some(k) if k.scripted => {
                        if !k.has_hp {
                            let name = k.name.clone();
                            return Err(mlua::Error::runtime(format!("thing:hurt: a {name} has no hit points (c.things.{name}.hp)")));
                        }
                        let hp = h.thing_mut(t.0)?.hp;
                        *hp -= damage as i32;
                        if *hp <= 0 {
                            h.here.killers.insert(t.0, (by_handle, weapon.clone()));
                        }
                        h.here.later.push(Event::Hit { who: whom, by, damage: damage as i32, weapon: weapon.clone() });
                    }
                    // The ready-made rules' own, taken by their rules.
                    _ => {
                        h.host.hurt(whom, damage as i32, by, weapon.clone()).map_err(|e| mlua::Error::runtime(format!("thing:hurt: {e}")))?;
                        let told = std::mem::take(&mut h.host.sim_mut().events);
                        h.here.later.extend(told);
                    }
                }
                Ok(())
            })
        });
        m.add_method("take", |lua, t, (item, count): (String, Option<i64>)| {
            let count = count.unwrap_or(1);
            if count < 1 {
                return Err(mlua::Error::runtime("thing:take: how many is 1 or more"));
            }
            let stack = look(lua, |h| {
                let cfg = &h.host.sim().cfg;
                match cfg.items.get(&item) {
                    Some(i) => Ok(i.stack as i64),
                    None => Err(mlua::Error::runtime(format!("thing:take: no item called '{item}' (c.items: {})", cfg.item_names().join(", ")))),
                }
            })?;
            let Value::Table(state) = thing_state(lua, t.0)? else { return Err(mlua::Error::runtime("thing:take: no state")) };
            let inventory = match state.get::<Value>("inventory")? {
                Value::Table(inv) => inv,
                Value::Nil => {
                    let inv = lua.create_table()?;
                    state.set("inventory", &inv)?;
                    inv
                }
                other => return Err(mlua::Error::runtime(format!("thing:take: its state.inventory is a {}, not a table", other.type_name()))),
            };
            let have = inventory.get::<Option<i64>>(item.as_str())?.unwrap_or(0);
            let now = (have + count).min(stack).max(have);
            inventory.set(item.as_str(), now)?;
            let taken = now - have;
            if taken > 0 {
                change(lua, "thing:take", |h| {
                    let who = h.who(t.0);
                    h.here.later.push(Event::Pickup { who, item });
                    Ok(())
                })?;
            }
            Ok(taken)
        });
        m.add_method("remove", |lua, t, ()| {
            if handle_seat(t.0).is_some() {
                return Err(mlua::Error::runtime("player:remove: the player belongs to no level; it goes from level to level (out of play: state.out = true)"));
            }
            change(lua, "thing:remove", |h| {
                let there = h.host.get(Id::from_raw(t.0)).is_some() && !h.here.gone.contains(&t.0);
                if there {
                    h.here.gone.push(t.0);
                }
                Ok(there)
            })
        });
    }
}
