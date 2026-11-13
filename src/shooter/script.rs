//! The shooter as a Lua game's kit (`c.kit = "shooter"`): its behaviors
//! run by themselves, the script sees its things, counts them and may take
//! their hits; kinds without a behavior are the script's own, moved after
//! its `on_tick`.

use glam::{IVec2, IVec3, Vec3};

use super::kinds::{self, KindRules};
use super::{Brain, Phase, Shooter};
use crate::game::Outcome;
use crate::input::Input;
use crate::lua::host::{Host, Kept, Kit, Seen, Touched, Touching, PLAYER, handle_seat, move_things};
use crate::sim::things::Id;
use crate::sim::{Body, Sim, stand_on_ground};

impl Host for Shooter {
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
            let reach = match t.brain {
                Brain::Pickup => Some(self.rules.pickup(t.kind).reach),
                Brain::Captive(_) => Some(self.rules.captive(t.kind).reach),
                Brain::Player(_) => None,
                _ => self.rules.kinds[t.kind].reach,
            };
            Seen { kind: t.kind, body: &t.body, hp: t.hp, flash: t.flash, half: self.sim.half(), reach }
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

    /// The script's kinds at (x, y) (on the ground unless `z` is given); the
    /// kit's where a marker there would place them.
    fn spawn(&mut self, kind: usize, x: f32, y: f32, z: Option<f32>) -> Result<Id, String> {
        let k = &self.rules.kinds[kind];
        if let KindRules::Script = k.rules {
            let height = self.shapes.kind(k).height();
            let at = match z {
                Some(z) => Vec3::new(x, y, z),
                None => stand_on_ground(&self.sim.world, Vec3::new(x, y, 0.0), height, self.sim.half()),
            };
            return Ok(self.things.spawn(kind, Body::new(at, height), k.hp, Brain::Script(Default::default())));
        }
        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
        let cz = z.map_or_else(|| self.sim.world.column_top(cx, cy), |z| z.floor() as i32);
        Ok(self.place_kind(kind, IVec3::new(cx, cy, cz)))
    }

    fn remove(&mut self, id: Id) -> bool {
        self.things.remove(id).is_some()
    }

    fn score(&self) -> u32 {
        self.status.score
    }

    fn set_score(&mut self, score: u32) {
        self.status.score = score;
    }

    /// Its numbers, and the weapon in hand (by its name in
    /// `c.player.weapons`); what the player holds is the inventory
    /// (`Host::stock`).
    fn player_keys(&self) -> &'static [&'static str] {
        &["score", "hp", "invulnerable", "weapon"]
    }

    fn player_number(&self, key: &str) -> Option<i64> {
        if self.several() && key != "score" {
            // Seat 1's figure (none yet: nothing).
            let t = self.seated(1).and_then(|id| self.things.get(id));
            return match key {
                "hp" => Some(t.map_or(0, |t| t.hp as i64)),
                "invulnerable" => Some(t.and_then(|t| self.mind(t.id)).map_or(0, |s| s.invulnerable as i64)),
                _ => None,
            };
        }
        match key {
            "score" => Some(self.status.score as i64),
            "hp" => Some(self.status.hp as i64),
            "invulnerable" => Some(self.player.invulnerable as i64),
            _ => None,
        }
    }

    /// Hit points back above 0 bring the player down back to life.
    fn set_player_number(&mut self, key: &str, value: i64) -> bool {
        if self.several() && key != "score" {
            // Seat 1's figure, or what it will come with.
            let Some(id) = self.seated(1) else {
                match key {
                    "hp" => {
                        self.arriving(1);
                        if let Some(d) = self.departed.iter_mut().find(|(n, ..)| *n == 1) {
                            d.2 = value as i32;
                        }
                    }
                    "invulnerable" => self.arriving(1).invulnerable = value.clamp(0, u32::MAX as i64) as u32,
                    _ => return false,
                }
                return true;
            };
            match key {
                "hp" => {
                    if let Some(t) = self.things.get_mut(id) {
                        t.hp = value as i32;
                    }
                }
                "invulnerable" => {
                    self.set_figure_number(id, key, value);
                }
                _ => return false,
            }
            return true;
        }
        match key {
            "score" => self.status.score = value.max(0) as u32,
            "hp" => {
                self.status.hp = value as i32;
                if value > 0 {
                    self.back_to_life();
                }
            }
            "invulnerable" => self.player.invulnerable = value.clamp(0, u32::MAX as i64) as u32,
            _ => return false,
        }
        true
    }

    /// The weapon by its name (`c.weapons`), by the player or any thing of
    /// the level.
    fn attack(&mut self, who: crate::sim::Who, weapon: &str, at: Option<(Vec3, Option<Id>)>) -> Result<(), String> {
        let Some(w) = self.rules.weapon_names.iter().position(|n| n == weapon) else {
            return Err(format!("no weapon called '{weapon}' (c.weapons: {})", self.rules.weapon_names.join(", ")));
        };
        if let crate::sim::Who::Thing(id) = who
            && self.things.get(id).is_none()
        {
            return Err("it is gone".into());
        }
        self.queued.push((who, w, at));
        Ok(())
    }

    fn start_attacks(&mut self) {
        Shooter::start_attacks(self);
    }

    fn moved_player_keys(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("ammo", "the rounds are the inventory's, an item of the gun's name: player.state.inventory[player.state.weapon], player:take(\"machine_gun\", 150)"),
            ("grenades", "they are the inventory's: player.state.inventory.grenade, player:take(\"grenade\", 5)"),
        ]
    }

    /// The rounds and grenades the player holds, by item: the inventory's
    /// part the rules spend.
    fn stock(&self) -> Vec<(String, u32)> {
        self.seat_stock(1)
    }

    fn set_stock(&mut self, item: &str, count: u32) {
        self.set_seat_stock(1, item, count);
    }

    /// The gun in hand by its name (`c.weapons`); none: nil.
    fn player_kept(&self, key: &str) -> Option<Kept> {
        self.seat_kept(1, key)
    }

    fn set_player_kept(&mut self, key: &str, value: Kept) -> Result<(), String> {
        self.set_seat_kept(1, key, value)
    }

    /// A seat's rounds and grenades (seat 1's the one player's in a game
    /// of one), by item.
    fn seat_stock(&self, seat: usize) -> Vec<(String, u32)> {
        let Some((_, stock)) = self.arms_of(seat) else { return Vec::new() };
        let items = (0..self.rules.weapons.len()).filter_map(|w| self.rules.rounds_of(w));
        items.map(|item| (item.to_string(), stock.get(item).copied().unwrap_or(0))).collect()
    }

    fn set_seat_stock(&mut self, seat: usize, item: &str, count: u32) {
        if let Some((_, stock)) = self.arms_of_mut(seat) {
            stock.insert(item.to_string(), count);
        }
    }

    fn seat_kept(&self, seat: usize, key: &str) -> Option<Kept> {
        (key == "weapon").then_some(())?;
        let (weapon, _) = self.arms_of(seat)?;
        weapon.map(|w| Kept::Text(self.rules.weapon_names[w].clone()))
    }

    fn set_seat_kept(&mut self, seat: usize, key: &str, value: Kept) -> Result<(), String> {
        let gun = match (key, value) {
            ("weapon", Kept::Text(gun)) => match self.rules.weapon_names.iter().position(|w| *w == gun) {
                Some(w) => Some(w),
                None => return Err(format!("no weapon called '{gun}' (c.weapons: {})", self.rules.weapon_names.join(", "))),
            },
            // nil: no gun in hand.
            ("weapon", Kept::Bool(false)) => None,
            ("weapon", _) => return Err("the name of a weapon (c.weapons), or nil for none".into()),
            _ => return Err(format!("{key} is not kept by its rules")),
        };
        // A seat whose figure stands nowhere yet: its gun when it comes.
        match self.arms_of_mut(seat) {
            Some((weapon, _)) => *weapon = gun,
            None => self.arriving(seat).weapon = gun,
        }
        Ok(())
    }

    /// A pickup is touched by the player's hips within its reach of its
    /// middle (as it always was taken).
    fn touches(&self, a: &Touching, b: &Touching) -> Option<bool> {
        let is_player = |t: &Touching| handle_seat(t.id).is_some();
        let (player, other) = if is_player(a) { (a, b) } else if is_player(b) { (b, a) } else { return None };
        let t = self.things.get(Id::from_raw(other.id))?;
        let Brain::Pickup = t.brain else { return None };
        Some((t.body.pos + 0.5).distance(player.at + Vec3::Z * player.height * 0.2) < self.rules.pickup(t.kind).reach)
    }

    /// A hostage's `freed`: set true by the game's code, its cage bursts.
    /// A gunship's `flying`: up in the air fighting (not parked, not shot
    /// down), only to read.
    fn thing_keys(&self, id: Id) -> &'static [&'static str] {
        match self.things.get(id).map(|t| &t.brain) {
            Some(Brain::Captive(_)) => &["freed"],
            Some(Brain::Gunship(_)) => &["flying"],
            _ => &[],
        }
    }

    fn thing_value(&self, id: Id, key: &str) -> Option<Kept> {
        match (self.things.get(id).map(|t| &t.brain), key) {
            (Some(Brain::Captive(h)), "freed") => Some(Kept::Bool(h.freed)),
            (Some(Brain::Gunship(g)), "flying") => Some(Kept::Bool(g.flying() && g.state != super::helicopter::State::Falling)),
            _ => None,
        }
    }

    fn set_thing_value(&mut self, id: Id, key: &str, value: Kept) -> Result<(), String> {
        let Some(t) = self.things.get(id) else { return Err("it is gone".into()) };
        match (&t.brain, key, value) {
            (Brain::Captive(h), "freed", Kept::Bool(false)) if h.freed => Err("a hostage freed is not caged again".into()),
            (Brain::Captive(_), "freed", Kept::Bool(free)) => {
                if free {
                    self.free_captive(id);
                }
                Ok(())
            }
            (Brain::Captive(_), "freed", _) => Err("true frees it".into()),
            (Brain::Gunship(_), "flying", _) => Err("a gunship flies by its ready-made rules (only to read)".into()),
            _ => Err(format!("{key} is not kept by its rules")),
        }
    }

    /// The player turns (it turns again as it moves); the script's things
    /// as theirs.
    fn face(&mut self, id: Id, way: Vec3) -> Result<(), String> {
        let id = self.own(id);
        if id.raw() == PLAYER {
            self.player.body.face(way);
            return Ok(());
        }
        match self.things.get_mut(id) {
            Some(t) if matches!(t.brain, Brain::Script(_) | Brain::Player(_)) || t.off.is_some() => {
                t.body.face(way);
                Ok(())
            }
            Some(t) => Err(format!("a {} turns by its ready-made rules", self.rules.kinds[t.kind].name)),
            None => Err("it is gone".into()),
        }
    }

    /// The player stood up afresh there (a respawn), facing the way he did.
    fn put_player(&mut self, at: Vec3) -> Result<(), String> {
        if self.several() {
            return match self.seated(1) {
                Some(id) => self.put_figure(id, at),
                None => Ok(()),
            };
        }
        self.stand_afresh(at);
        Ok(())
    }

    fn end(&mut self, outcome: Outcome) {
        self.status.phase = match outcome {
            Outcome::Won => Phase::Won,
            Outcome::Lost => Phase::GameOver,
        };
    }

    /// Its own player is no thing: the camera leads as its rules do.
    fn follow(&mut self, id: Id) -> Result<(), String> {
        self.watch = if id.raw() == PLAYER { crate::sim::Watch::Player } else { crate::sim::Watch::Thing(id) };
        Ok(())
    }

    fn look_at(&mut self, x: f32, y: f32) -> Result<(), String> {
        self.watch = crate::sim::Watch::Still;
        (self.sim.cam_target.x, self.sim.cam_target.y) = (x, y);
        Ok(())
    }

    fn hurt(&mut self, who: crate::sim::Who, damage: i32, by: Option<crate::sim::Who>, weapon: Option<String>) -> Result<(), String> {
        let who = match who {
            crate::sim::Who::Player if self.several() => match self.seated(1) {
                Some(id) => crate::sim::Who::Thing(id),
                None => return Ok(()),
            },
            who => who,
        };
        match who {
            crate::sim::Who::Player => self.hurt_player(damage, by, weapon),
            crate::sim::Who::Thing(id) if matches!(self.things.get(id).map(|t| &t.brain), Some(Brain::Player(_))) => self.hurt_seated(id, damage, by, weapon),
            crate::sim::Who::Thing(id) => match self.things.get(id) {
                Some(t) if matches!(t.brain, Brain::Soldier(_) | Brain::Gunship(_) | Brain::Captive(_)) && t.hp > 0 => {
                    // A silent weapon's blow: nobody hears it.
                    let quiet = weapon.as_ref().and_then(|w| self.rules.weapon_names.iter().position(|n| n == w)).is_some_and(|w| self.rules.weapons[w].noise == Some(0.0));
                    self.damage_thing(id, damage, quiet, by, weapon)
                }
                Some(t) => return Err(format!("a {} takes no hits by its ready-made rules (no hit points: c.things.{}.hp)", self.rules.kinds[t.kind].name, self.rules.kinds[t.kind].name)),
                None => return Err("it is gone".into()),
            },
        }
        Ok(())
    }

    /// Sets a figure moving (`thing:launch`): the player, a soldier or a
    /// hostage (their rules steer on from there), the script's things and
    /// those whose rules are off.
    fn launch(&mut self, id: Id, vel: Vec3) -> Result<(), String> {
        let id = self.own(id);
        let body = if id.raw() == PLAYER {
            &mut self.player.body
        } else {
            let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
            match (&mut t.off, &mut t.brain) {
                (Some(walk), _) | (None, Brain::Script(walk)) => *walk = Default::default(),
                (None, Brain::Soldier(_) | Brain::Captive(_) | Brain::Player(_)) => {}
                _ => {
                    let k = &self.rules.kinds[t.kind];
                    return Err(format!("a {} moves by its ready-made rules (thing:enable(\"{}\", false) first)", k.name, kinds::behavior_name(&k.rules)));
                }
            }
            &mut t.body
        };
        body.vel = vel;
        body.on_ground &= vel.z <= 0.0;
        Ok(())
    }

    /// A thing's behavior off (physics and the script move it) or on
    /// again. The player is no thing of a kind yet: its rules stay.
    fn enable(&mut self, id: Id, behavior: &str, on: bool) -> Result<(), String> {
        let id = self.own(id);
        if id.raw() == PLAYER || matches!(self.things.get(id).map(|t| &t.brain), Some(Brain::Player(_))) {
            return Err("the shooter's players move by their keys: their rules stay on (level.state.held holds them)".into());
        }
        let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
        let k = &self.rules.kinds[t.kind];
        let has = kinds::behavior_name(&k.rules);
        if has.is_empty() {
            return Err(format!("a {} has no ready-made behavior: your script moves it", k.name));
        }
        if has != behavior {
            return Err(format!("a {}'s behavior is \"{has}\", not \"{behavior}\"", k.name));
        }
        match (on, t.off.is_some()) {
            (false, false) => t.off = Some(Default::default()),
            (true, true) => t.off = None,
            _ => {}
        }
        Ok(())
    }

    fn clear_rounds(&mut self, team: Option<&str>) -> Result<(), String> {
        Shooter::clear_rounds(self, team);
        Ok(())
    }

    /// Its own figure, `c.player`: of no kind of `c.things`; in a game of
    /// several, seat 1's figure.
    fn player_seen(&self) -> Option<Seen<'_>> {
        if self.several() {
            return self.seated(1).and_then(|id| self.get(id));
        }
        Some(Seen { kind: crate::lua::host::NO_KIND, body: &self.player.body, hp: self.status.hp, flash: 0, half: self.sim.half(), reach: None })
    }

    /// Seat 1's figure is a thing in a game of several.
    fn player_id(&self) -> Option<Id> {
        self.seated(1).filter(|_| self.several())
    }

    fn seat_figure(&self, seat: usize) -> Option<Id> {
        self.seated(seat)
    }

    /// A seat's figure of `kind` (one without behavior: the players' keys
    /// move it) standing at `at`, its gun and rounds the run's start or
    /// what it had; or taken away (its gun and rounds kept for its return).
    fn put_seat(&mut self, seat: usize, figure: Option<(usize, Vec3, Option<IVec2>, i32)>) -> Result<Option<Id>, String> {
        let was = self.seated(seat);
        if let Some(t) = was.and_then(|id| self.things.remove(id)) {
            if let Brain::Player(s) = t.brain {
                self.departed.retain(|(n, ..)| *n != seat);
                self.departed.push((seat, *s, t.hp));
            }
        }
        let Some((kind, at, facing, hp)) = figure else { return Ok(None) };
        // Seat 1's first figure stands where `--at` put the players.
        let at = if seat == 1 { self.start_at.take().unwrap_or(at) } else { at };
        let k = &self.rules.kinds[kind];
        if !matches!(k.rules, KindRules::Script) {
            return Err(format!("c.player.kind: a player's figure moves by its keys, a {} by its behavior (c.things.{}.behavior)", k.name, k.name));
        }
        let height = self.shapes.kind(k).height();
        let at = crate::sim::stand_at(&self.sim.world, at, height, self.sim.half());
        let mut body = Body::new(at, height);
        if let Some(way) = facing {
            body.face(Vec3::new(way.x as f32, way.y as f32, 0.0));
        }
        // Its hit points as it went (down then: those the game gives it),
        // else the game's, else the run's start.
        let start = super::Status::start(&self.sim.cfg, &self.rules);
        let (mind, kept) = match self.departed.iter().position(|(n, ..)| *n == seat) {
            Some(i) => {
                let (_, mind, kept) = self.departed.remove(i);
                (mind, kept)
            }
            None => (self.fresh(seat, &start), 0),
        };
        let hp = [kept, hp, start.hp].into_iter().find(|&hp| hp > 0).unwrap_or(0);
        let mind = super::Seated { down: false, ..mind };
        Ok(Some(self.things.spawn(kind, body, hp, Brain::Player(Box::new(mind)))))
    }

    /// A seat's figure stood up afresh at `at` (`p:place`), still, facing
    /// the way it did.
    fn put_figure(&mut self, id: Id, at: Vec3) -> Result<(), String> {
        let half = self.sim.half();
        let Some(t) = self.things.get_mut(id) else { return Err("it is gone".into()) };
        let at = crate::sim::stand_at(&self.sim.world, at, t.body.height, half);
        let (facing, look) = (t.body.facing, t.body.look);
        t.body = Body::new(at, t.body.height);
        (t.body.facing, t.body.look) = (facing, look);
        Ok(())
    }

    fn follows_players(&self) -> bool {
        self.watch == crate::sim::Watch::Player
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

    fn figure_number(&self, id: Id, key: &str) -> Option<i64> {
        (key == "invulnerable").then(|| self.mind(id).map(|s| s.invulnerable as i64))?
    }

    fn set_figure_number(&mut self, id: Id, key: &str, value: i64) -> bool {
        match (key, self.things.get_mut(id).map(|t| &mut t.brain)) {
            ("invulnerable", Some(Brain::Player(s))) => {
                s.invulnerable = value.clamp(0, u32::MAX as i64) as u32;
                true
            }
            _ => false,
        }
    }

    fn start_at(&self, world: &crate::world::World, _markers: &[crate::world::Marker]) -> Option<Vec3> {
        Some(crate::sim::spawn_point(world, self.sim.cfg.player.spawn, self.player.body.height, self.sim.half()))
    }

    fn box_clone(&self) -> Box<dyn Host> {
        Box::new(self.clone())
    }
}

impl Kit for Shooter {
    fn named(name: &str) -> bool {
        name == "shooter"
    }

    fn names() -> Vec<&'static str> {
        vec!["shooter"]
    }

    fn rules(&mut self, input: &Input) {
        self.run(input);
    }

    fn set_seat_inputs(&mut self, inputs: &[Option<Input>]) {
        self.seat_inputs = inputs.to_vec();
    }

    fn set_seat_teams(&mut self, teams: &[Option<&'static str>]) {
        Shooter::set_seat_teams(self, teams);
    }

    /// Every seat's player a figure of `c.player.kind`'s.
    fn plays_several(_cfg: &crate::config::GameConfig) -> bool {
        true
    }

    fn held(&mut self) -> bool {
        let ticked = self.sim.begin_tick();
        self.hold();
        for t in self.things.iter_mut() {
            t.body.prev = t.body.pos;
        }
        self.sim.cam_prev = self.sim.cam_target;
        ticked
    }

    fn built(&mut self) {
        let at = self.player.body.pos;
        self.move_player(at.x, at.y);
    }

    fn enter_level(&mut self, world: crate::world::World, markers: &[crate::world::Marker], places: &[crate::levels::Place], at: Option<(Vec3, Option<IVec2>)>) {
        // The run goes along (a player arriving without hit points gets
        // the start's back: the engine's).
        let run = self.status.clone();
        Shooter::replace_world(self, world, markers);
        let s = &mut self.status;
        (s.hp, s.score, s.weapon, s.stock, s.phase) = (run.hp, run.score, run.weapon, run.stock, run.phase);
        self.grenades.clear();
        self.places = places.into();
        match at {
            Some((to, facing)) => {
                self.stand_player(to);
                if let Some(way) = facing {
                    self.player.body.face(Vec3::new(way.x as f32, way.y as f32, 0.0));
                }
            }
            None => {
                let spawn = self.sim.cfg.player.spawn;
                self.move_player(spawn[0], spawn[1]);
                // As a run starts.
                let [x, y] = self.cfg.player.facing.unwrap_or([0.0, -1.0]);
                self.player.body.face(Vec3::new(x, y, 0.0));
            }
        }
    }

    fn run(&self) -> toml::Table {
        let s = &self.status;
        let mut run = toml::Table::new();
        for (key, n) in [("hp", s.hp as i64), ("score", s.score as i64)] {
            run.insert(key.into(), n.into());
        }
        if let Some(w) = s.weapon {
            run.insert("weapon".into(), self.rules.weapon_names[w].clone().into());
        }
        run
    }

    fn set_run(&mut self, run: &toml::Table) {
        let n = |key: &str| run.get(key).and_then(toml::Value::as_integer);
        let s = &mut self.status;
        if let Some(v) = n("hp") {
            s.hp = v as i32;
        }
        if let Some(v) = n("score") {
            s.score = v.max(0) as u32;
        }
        // The gun by its name; one the game no longer has: none.
        s.weapon = run.get("weapon").and_then(toml::Value::as_str).and_then(|gun| self.rules.weapon_names.iter().position(|w| w == gun));
    }

    fn move_scripted(&mut self, fixed: &dyn Fn(usize) -> bool) {
        for t in self.things.iter_mut().filter(|t| matches!(t.brain, Brain::Script(_)) || t.off.is_some()) {
            t.body.prev = t.body.pos;
        }
        move_things(&mut self.things, &self.sim, fixed, |b| match b {
            Brain::Script(walk) => Some(walk),
            _ => None,
        });
    }
}

impl Shooter {
    /// The one player (`PLAYER`) as seat 1's figure in a game of several.
    fn own(&self, id: Id) -> Id {
        match (id.raw() == PLAYER, self.seated(1)) {
            (true, Some(figure)) if self.several() => figure,
            _ => id,
        }
    }

    /// A seat's player's numbers, where `id` is a seat's figure.
    fn mind(&self, id: Id) -> Option<&super::Seated> {
        match &self.things.get(id)?.brain {
            Brain::Player(s) => Some(s),
            _ => None,
        }
    }

    /// A seat's gun in hand and rounds: the one player's (seat 1 of a game
    /// of one), a seat's figure's, else what its figure will come with.
    fn arms_of(&self, seat: usize) -> Option<(Option<usize>, &std::collections::BTreeMap<String, u32>)> {
        if !self.several() {
            return (seat == 1).then_some((self.status.weapon, &self.status.stock));
        }
        match self.seated(seat).and_then(|id| self.mind(id)) {
            Some(s) => Some((s.weapon, &s.stock)),
            None => self.departed.iter().find(|(n, ..)| *n == seat).map(|(_, s, _)| (s.weapon, &s.stock)),
        }
    }

    fn arms_of_mut(&mut self, seat: usize) -> Option<(&mut Option<usize>, &mut std::collections::BTreeMap<String, u32>)> {
        if !self.several() {
            return (seat == 1).then_some((&mut self.status.weapon, &mut self.status.stock));
        }
        match self.seated(seat) {
            Some(id) => match &mut self.things.get_mut(id)?.brain {
                Brain::Player(s) => Some((&mut s.weapon, &mut s.stock)),
                _ => None,
            },
            None => self.departed.iter_mut().find(|(n, ..)| *n == seat).map(|(_, s, _)| (&mut s.weapon, &mut s.stock)),
        }
    }

    /// What seat `seat`'s figure will come with, made ready for it.
    fn arriving(&mut self, seat: usize) -> &mut super::Seated {
        if !self.departed.iter().any(|(n, ..)| *n == seat) {
            let start = super::Status::start(&self.sim.cfg, &self.rules);
            let fresh = self.fresh(seat, &start);
            self.departed.push((seat, fresh, 0));
        }
        &mut self.departed.iter_mut().find(|(n, ..)| *n == seat).expect("made ready").1
    }

    /// A seat's player as a run starts it (`c.state.player`: its gun and
    /// rounds).
    fn fresh(&self, seat: usize, start: &super::Status) -> super::Seated {
        super::Seated {
            seat,
            invulnerable: 0,
            cooldowns: vec![0; self.rules.binds.attacks.len()],
            held: 0,
            busy: 0,
            jump_held: false,
            down: false,
            weapon: start.weapon,
            stock: start.stock.clone(),
        }
    }
}
