//! The platformer as a Lua game's kit (`c.kit = "platformer"`): its
//! behaviours run by themselves, the script sees its things, counts them
//! and may take their hits; kinds without a behaviour are the script's own,
//! moved after its `on_tick`.

use glam::Vec3;

use super::kinds::{self, KindRules};
use super::{Brain, Platformer};
use crate::game::Outcome;
use crate::input::Input;
use crate::lua::host::{Host, Kept, Kit, Seen, Touched, move_things};
use crate::sim::things::Id;
use crate::sim::{Body, Sim, stand_on_ground};

impl Host for Platformer {
    fn sim(&self) -> &Sim {
        &self.sim
    }

    fn sim_mut(&mut self) -> &mut Sim {
        &mut self.sim
    }

    fn ids(&self) -> Vec<Id> {
        self.things.iter().map(|t| t.id).collect()
    }

    fn get(&self, id: Id) -> Option<Seen<'_>> {
        self.things.get(id).map(|t| {
            let k = &self.rules.kinds[t.kind];
            Seen { kind: t.kind, body: &t.body, hp: t.hp, flash: t.flash, half: k.width * 0.5, reach: k.reach }
        })
    }

    fn get_mut(&mut self, id: Id) -> Option<Touched<'_>> {
        self.things.get_mut(id).map(|t| {
            let moved = match (&mut t.off, &mut t.brain) {
                (Some(walk), _) | (None, Brain::Script(walk)) => Some((&mut t.body, walk)),
                _ => None,
            };
            Touched { hp: &mut t.hp, moved }
        })
    }

    /// The script's kinds at (x, y) (on the ground unless `z` is given),
    /// the kit's as a marker there would place them.
    fn spawn(&mut self, kind: usize, x: f32, y: f32, z: Option<f32>) -> Result<Id, String> {
        let k = &self.rules.kinds[kind];
        if let KindRules::Script = k.rules {
            let at = match z {
                Some(z) => Vec3::new(x, y, z),
                None => stand_on_ground(&self.sim.world, Vec3::new(x, y, 0.0), k.height, k.width * 0.5),
            };
            return Ok(self.things.spawn(kind, Body::new(at, k.height), k.hp, Brain::Script(Default::default())));
        }
        let z = z.unwrap_or_else(|| self.sim.world.column_top(x.floor() as i32, y.floor() as i32) as f32);
        Ok(self.place(kind, Vec3::new(x, y, z)))
    }

    fn remove(&mut self, id: Id) -> bool {
        if self.hero == Some(id) {
            self.hero = None;
        }
        for h in self.seat_heroes.iter_mut().filter(|h| **h == Some(id)) {
            *h = None;
        }
        self.things.remove(id).is_some()
    }

    fn score(&self) -> u32 {
        self.status.score
    }

    fn set_score(&mut self, score: u32) {
        self.status.score = score;
    }

    /// The hero's hit points, its safe ticks, its pose, the score.
    fn player_keys(&self) -> &'static [&'static str] {
        &["score", "hp", "invulnerable", "pose"]
    }

    fn player_kept(&self, key: &str) -> Option<Kept> {
        self.hero.and_then(|id| self.thing_value(id, key))
    }

    fn set_player_kept(&mut self, key: &str, value: Kept) -> Result<(), String> {
        match self.hero {
            Some(id) => self.set_thing_value(id, key, value),
            None => Err("there is no hero in this level".into()),
        }
    }

    fn player_number(&self, key: &str) -> Option<i64> {
        match key {
            "score" => Some(self.status.score as i64),
            "hp" => Some(self.status.hp as i64),
            "invulnerable" => self.hero_brain().map(|h| h.hurt as i64),
            _ => None,
        }
    }

    fn set_player_number(&mut self, key: &str, value: i64) -> bool {
        match key {
            "score" => self.status.score = value.max(0) as u32,
            "hp" => self.status.hp = value.max(0) as u32,
            "invulnerable" => {
                let Some(t) = self.hero.and_then(|id| self.things.get_mut(id)) else { return true };
                if let Brain::Hero(h) = &mut t.brain {
                    h.hurt = value.clamp(0, u32::MAX as i64) as u32;
                }
            }
            _ => return false,
        }
        true
    }

    fn end(&mut self, outcome: Outcome) {
        self.status.outcome = Some(outcome);
    }

    fn follow(&mut self, id: Id) -> Result<(), String> {
        self.watch = if self.hero == Some(id) { crate::sim::Watch::Player } else { crate::sim::Watch::Thing(id) };
        Ok(())
    }

    fn look_at(&mut self, x: f32, y: f32) -> Result<(), String> {
        self.watch = crate::sim::Watch::Still;
        (self.sim.cam_target.x, self.sim.cam_target.y) = (x, y);
        Ok(())
    }

    fn player_id(&self) -> Option<Id> {
        self.hero
    }

    fn seat_figure(&self, seat: usize) -> Option<Id> {
        seat.checked_sub(2).and_then(|i| self.seat_heroes.get(i).copied().flatten())
    }

    /// A hero for a seat after the first, or its hero taken away.
    fn put_seat(&mut self, seat: usize, figure: Option<(usize, Vec3, Option<glam::IVec2>, i32)>) -> Result<Option<Id>, String> {
        // Seat 1's hero out of play, or back (its hearts the run's).
        if seat == 1 {
            if let Some(old) = self.hero.take() {
                self.things.remove(old);
            }
            let Some((kind, at, facing, _)) = figure else { return Ok(None) };
            let id = self.place(kind, at);
            if let (Some(t), Some(way)) = (self.things.get_mut(id), facing) {
                t.body.face(Vec3::new(way.x as f32, way.y as f32, 0.0));
            }
            return Ok(Some(id));
        }
        let Some(i) = seat.checked_sub(2).filter(|&i| i < self.seat_heroes.len()) else { return Err(format!("no seat {seat}")) };
        if let Some(old) = self.seat_heroes[i].take() {
            self.things.remove(old);
        }
        let Some((kind, at, facing, hp)) = figure else { return Ok(None) };
        if !matches!(self.rules.kinds.get(kind).map(|k| &k.rules), Some(KindRules::Hero(_))) {
            return Err(format!("a {} is no hero (its behaviour is not \"hero\")", self.rules.kinds.get(kind).map_or("?", |k| k.name.as_str())));
        }
        self.seating = true;
        let id = self.place(kind, at);
        self.seating = false;
        if let Some(t) = self.things.get_mut(id) {
            t.hp = hp;
            if let Some(way) = facing {
                t.body.face(Vec3::new(way.x as f32, way.y as f32, 0.0));
            }
        }
        self.seat_heroes[i] = Some(id);
        Ok(Some(id))
    }

    fn put_figure(&mut self, id: Id, at: Vec3) -> Result<(), String> {
        let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
        (t.body.pos, t.body.prev, t.body.vel) = (at, at, Vec3::ZERO);
        if let Brain::Hero(h) = &mut t.brain {
            *h = super::Hero { hurt: h.hurt, asked: u32::MAX / 2, air: u32::MAX / 2, ..super::Hero::default() };
        }
        Ok(())
    }

    fn hero_kind(&self) -> Option<usize> {
        Platformer::hero_kind(self)
    }

    fn follows_players(&self) -> bool {
        matches!(self.watch, crate::sim::Watch::Player)
    }

    fn hold_back(&mut self, id: Id, at: Vec3) {
        if let Some(t) = self.things.get_mut(id) {
            for axis in 0..2 {
                if at[axis] != t.body.pos[axis] {
                    t.body.vel[axis] = 0.0;
                }
            }
            t.body.pos = at;
        }
    }

    /// A seat's hero's ticks safe from hits (`p.state.invulnerable`).
    fn figure_number(&self, id: Id, key: &str) -> Option<i64> {
        match (&self.things.get(id)?.brain, key) {
            (Brain::Hero(h), "invulnerable") => Some(h.hurt as i64),
            _ => None,
        }
    }

    fn set_figure_number(&mut self, id: Id, key: &str, value: i64) -> bool {
        match (self.things.get_mut(id).map(|t| &mut t.brain), key) {
            (Some(Brain::Hero(h)), "invulnerable") => {
                h.hurt = value.clamp(0, u32::MAX as i64) as u32;
                true
            }
            _ => false,
        }
    }

    /// The hero loses a heart (thrown back from whoever hurt it); a figure
    /// that walks, hops or flies loses hit points and goes at 0 (its
    /// kind's effect and sound are the engine's, what it is worth the
    /// game's code).
    fn hurt(&mut self, who: crate::sim::Who, damage: i32, by: Option<crate::sim::Who>, weapon: Option<String>) -> Result<(), String> {
        let at = |s: &Self, w: Option<crate::sim::Who>| match w {
            Some(crate::sim::Who::Thing(id)) => s.things.get(id).map(|t| t.body.pos.x),
            _ => None,
        };
        match who {
            crate::sim::Who::Player => {
                let (Some(r), Some(hero)) = (self.hero_rules(), self.hero.and_then(|id| self.things.get(id))) else { return Err("the level has no hero".into()) };
                let from = at(self, by).unwrap_or(hero.body.pos.x - hero.body.look.x as f32);
                Platformer::hurt(self, from, &r, damage, by, weapon);
                Ok(())
            }
            // A seat's hero: a heart less, thrown back, as seat 1's.
            crate::sim::Who::Thing(id) if self.seat_heroes.contains(&Some(id)) => {
                let Some(hero) = self.things.get(id) else { return Err("it is gone".into()) };
                let KindRules::Hero(r) = self.rules.kinds[hero.kind].rules.clone() else { return Err("it is no hero".into()) };
                let from = at(self, by).unwrap_or(hero.body.pos.x - hero.body.look.x as f32);
                Platformer::hurt_hero(self, id, from, &r, damage, by, weapon);
                Ok(())
            }
            crate::sim::Who::Thing(id) => {
                let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
                let k = &self.rules.kinds[t.kind];
                if !k.rules.takes_hits() {
                    return Err(format!("a {} takes no hits by its ready-made rules", k.name));
                }
                t.hp -= damage;
                let (dead, kind, body) = (t.hp <= 0, t.kind, t.body.clone());
                self.sim.events.push(crate::sim::Event::Hit { who, by, damage, weapon: weapon.clone() });
                if dead {
                    self.things.remove(id);
                    self.sim.events.push(crate::sim::Event::Death { who, by, kind, body, weapon });
                }
                Ok(())
            }
        }
    }

    /// The hero stood at `at`, still and afresh (not climbing, hanging or
    /// riding), the camera on it.
    fn put_player(&mut self, at: Vec3) -> Result<(), String> {
        let Some(t) = self.hero.and_then(|id| self.things.get_mut(id)) else { return Err("the level has no hero".into()) };
        (t.body.pos, t.body.prev, t.body.vel) = (at, at, Vec3::ZERO);
        if let Brain::Hero(h) = &mut t.brain {
            *h = super::Hero { hurt: h.hurt, asked: u32::MAX / 2, air: u32::MAX / 2, ..super::Hero::default() };
        }
        self.look_at_hero();
        Ok(())
    }

    /// The hero rises at `speed` (its jump not cut by letting go); the
    /// script's things as theirs.
    fn bounce(&mut self, id: Id, speed: f32) -> Result<(), String> {
        let hero = self.hero == Some(id) || (id.raw() == u32::MAX && self.hero.is_some());
        let id = if hero { self.hero.expect("a hero") } else { id };
        let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
        match &mut t.brain {
            Brain::Hero(h) => {
                t.body.vel.z = speed;
                h.jumping = false;
                Ok(())
            }
            Brain::Script(_) => {
                t.body.vel.z = speed;
                t.body.on_ground = false;
                Ok(())
            }
            _ => Err(format!("a {} moves by its ready-made rules", self.rules.kinds[t.kind].name)),
        }
    }

    /// Sets a figure moving (`thing:launch`): the hero (out of a jump, off
    /// a ladder, a ledge or a platform; it steers on as the player says),
    /// a walker or hopper (moving so until it lands), the script's things
    /// and those whose rules are off.
    fn launch(&mut self, id: Id, vel: Vec3) -> Result<(), String> {
        let hero = self.hero == Some(id) || (id.raw() == u32::MAX && self.hero.is_some());
        let id = if hero { self.hero.expect("a hero") } else { id };
        let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
        match (&t.off, &mut t.brain) {
            (None, Brain::Hero(h)) => {
                (h.jumping, h.climbing, h.edge, h.pulling, h.riding) = (false, false, None, 0, None);
            }
            (None, Brain::Walker { .. } | Brain::Hopper { .. }) | (Some(_), _) | (None, Brain::Script(_)) => {}
            _ => return Err(format!("a {} moves by its ready-made rules (thing:enable(\"{}\", false) first)", self.rules.kinds[t.kind].name, kinds::behaviour_name(&self.rules.kinds[t.kind].rules))),
        }
        t.body.vel = vel;
        t.body.on_ground &= vel.z <= 0.0;
        if let Some(walk) = &mut t.off {
            *walk = Default::default();
        }
        Ok(())
    }

    /// A figure's behaviour off (it stands as physics has it, the script
    /// moves it) or on again; the hero's until the next level.
    fn enable(&mut self, id: Id, behaviour: &str, on: bool) -> Result<(), String> {
        let hero = self.hero == Some(id) || (id.raw() == u32::MAX && self.hero.is_some());
        let id = if hero { self.hero.expect("a hero") } else { id };
        let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
        let k = &self.rules.kinds[t.kind];
        let has = kinds::behaviour_name(&k.rules);
        if has.is_empty() {
            return Err(format!("a {} has no ready-made behaviour: your script moves it", k.name));
        }
        if has != behaviour {
            return Err(format!("a {}'s behaviour is \"{has}\", not \"{behaviour}\"", k.name));
        }
        match (on, t.off.is_some()) {
            (false, false) => t.off = Some(Default::default()),
            (true, true) => t.off = None,
            _ => {}
        }
        Ok(())
    }

    /// The hero turns left or right (as the side view has it); the
    /// script's things any way.
    fn face(&mut self, id: Id, way: Vec3) -> Result<(), String> {
        let hero = self.hero == Some(id) || (id.raw() == u32::MAX && self.hero.is_some());
        let id = if hero { self.hero.expect("a hero") } else { id };
        let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
        match &t.brain {
            Brain::Hero(_) if way.x != 0.0 => {
                t.body.face(Vec3::new(way.x.signum(), 0.0, 0.0));
                Ok(())
            }
            Brain::Hero(_) => Ok(()),
            _ if t.off.is_some() => {
                t.body.face(way);
                Ok(())
            }
            Brain::Script(_) => {
                t.body.face(way);
                Ok(())
            }
            _ => Err(format!("a {} turns by its ready-made rules", self.rules.kinds[t.kind].name)),
        }
    }

    fn attack(&mut self, _who: crate::sim::Who, _weapon: &str, _at: Option<(Vec3, Option<Id>)>) -> Result<(), String> {
        Err("the jump & run's ready-made rules have no weapons yet; thing:hurt hurts a thing".into())
    }

    /// A figure's pose (the set of its frames it shows: a prop's always,
    /// another's while the game sets one, over its rules' animation); a
    /// platform's goal (the end it goes to and stays at).
    fn thing_keys(&self, id: Id) -> &'static [&'static str] {
        match self.things.get(id).map(|t| &t.brain) {
            Some(Brain::Script(_)) | None => &[],
            Some(Brain::Platform(_)) => &["pose", "goal"],
            Some(_) => &["pose"],
        }
    }

    fn thing_value(&self, id: Id, key: &str) -> Option<Kept> {
        let t = self.things.get(id)?;
        match (&t.brain, key) {
            (Brain::Prop { pose, .. }, "pose") => Some(Kept::Text(if pose.is_empty() { "idle".into() } else { pose.clone() })),
            (Brain::Script(_), _) => None,
            (Brain::Platform(m), "goal") => m.goal.map(|end| Kept::Text(if end { "end" } else { "start" }.into())),
            (_, "pose") => t.pose.as_ref().map(|(p, _)| Kept::Text(p.clone())),
            _ => None,
        }
    }

    fn set_thing_value(&mut self, id: Id, key: &str, value: Kept) -> Result<(), String> {
        let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
        let k = &self.rules.kinds[t.kind];
        match (&mut t.brain, key, value) {
            (Brain::Prop { pose, since }, "pose", Kept::Text(name)) => {
                if name != "idle" && !k.frames.contains_key(&name) {
                    let names: Vec<&str> = k.frames.keys().map(String::as_str).collect();
                    return Err(format!("a {} has no frames called '{name}' (c.things.{}.frames: {})", k.name, k.name, names.join(", ")));
                }
                // A new pose plays from its first frame, the same again plays on.
                if *pose != name {
                    (*pose, *since) = (name, self.sim.tick);
                }
                Ok(())
            }
            (Brain::Prop { .. }, "pose", _) => Err("a pose is the name of a set of its frames".into()),
            (Brain::Script(_), _, _) => Err(format!("{key} is not kept by its rules")),
            (Brain::Platform(m), "goal", Kept::Text(end)) => {
                m.goal = Some(match end.as_str() {
                    "end" => true,
                    "start" => false,
                    _ => return Err(format!("a platform's goal is \"start\" or \"end\" (of its travel), not \"{end}\"")),
                });
                Ok(())
            }
            (Brain::Platform(m), "goal", Kept::Bool(false)) => {
                m.goal = None;
                Ok(())
            }
            (Brain::Platform(_), "goal", _) => Err("a platform's goal is \"start\" or \"end\" (of its travel), or nil: to and fro".into()),
            (_, "pose", Kept::Text(name)) => {
                if !k.frames.contains_key(&name) {
                    let names: Vec<&str> = k.frames.keys().map(String::as_str).collect();
                    return Err(format!("a {} has no frames called '{name}' (c.things.{}.frames: {})", k.name, k.name, names.join(", ")));
                }
                // The same pose again plays on.
                if t.pose.as_ref().is_none_or(|(p, _)| *p != name) {
                    t.pose = Some((name, self.sim.tick));
                }
                Ok(())
            }
            (_, "pose", Kept::Bool(false)) => {
                t.pose = None;
                Ok(())
            }
            (_, "pose", _) => Err("a pose is the name of a set of its frames, or nil for its rules' own".into()),
            _ => Err(format!("{key} is not kept by its rules")),
        }
    }

    fn start_at(&self, _world: &crate::world::World, markers: &[crate::world::Marker]) -> Option<Vec3> {
        let hero = markers.iter().find(|m| matches!(self.rules.kinds[m.kind].rules, KindRules::Hero(_)))?;
        Some(hero.cell.as_vec3() + Vec3::new(0.5, 0.5, 0.0))
    }

    fn box_clone(&self) -> Box<dyn Host> {
        Box::new(self.clone())
    }
}

impl Kit for Platformer {
    fn named(name: &str) -> bool {
        name == "platformer"
    }

    fn names() -> Vec<&'static str> {
        vec!["platformer"]
    }

    fn rules(&mut self, input: &Input) {
        self.run(input);
    }

    fn set_seat_inputs(&mut self, inputs: &[Option<Input>]) {
        self.seat_inputs = inputs.to_vec();
    }

    fn plays_several(_cfg: &crate::config::GameConfig) -> bool {
        true
    }

    fn held(&mut self) -> bool {
        let ticked = self.sim.begin_tick();
        for t in self.things.iter_mut() {
            t.body.prev = t.body.pos;
        }
        self.sim.cam_prev = self.sim.cam_target;
        ticked
    }

    fn move_scripted(&mut self, fixed: &dyn Fn(usize) -> bool) {
        move_things(&mut self.things, &self.sim, fixed, |b| match b {
            Brain::Script(walk) => Some(walk),
            _ => None,
        });
    }

    fn run(&self) -> toml::Table {
        let s = &self.status;
        let mut run = toml::Table::new();
        for (key, n) in [("score", s.score), ("hp", s.hp)] {
            run.insert(key.into(), (n as i64).into());
        }
        run
    }

    fn set_run(&mut self, run: &toml::Table) {
        let n = |key: &str| run.get(key).and_then(toml::Value::as_integer).map(|v| v.max(0) as u32);
        let s = &mut self.status;
        for (key, field) in [("score", &mut s.score), ("hp", &mut s.hp)] {
            if let Some(v) = n(key) {
                *field = v;
            }
        }
    }

    fn enter_level(&mut self, world: crate::world::World, markers: &[crate::world::Marker], _places: &[crate::levels::Place], at: Option<(Vec3, Option<glam::IVec2>)>) {
        crate::game::Game::replace_world(self, world, markers);
        let Some((to, facing)) = at else { return };
        let Some(kind) = self.hero.and_then(|id| self.things.get(id)).map(|t| t.kind) else { return };
        if let Some(id) = self.hero.take() {
            self.things.remove(id);
        }
        let new = self.place(kind, to);
        if let Some(t) = self.things.get_mut(new)
            && let Some(way) = facing
        {
            t.body.face(Vec3::new(way.x as f32, way.y as f32, 0.0));
        }
        self.look_at_hero();
    }
}
