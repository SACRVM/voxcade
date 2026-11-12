//! The kinds of things and the weapons as the shooter's behaviors read
//! them (`Rules`): every number that makes a rifleman a rifleman, a hut a
//! hut, a gunship a gunship and a rifle a rifle. A thing's `kind` is its
//! place in `Rules::kinds`; a weapon's its place in `Rules::weapons`.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::Sfx;
use super::config::ShooterConfig;
use crate::config::{GameConfig, resolve_like};
use crate::sim::things::{self, Frames};

#[derive(Clone, Debug)]
pub struct Rules {
    pub kinds: Vec<Kind>,
    pub weapons: Vec<WeaponRules>,
    /// The weapons' names (`c.weapons.NAME`), in the order of `weapons`.
    pub weapon_names: Vec<String>,
    /// How the player's actions attack and move (`c.player.attacks`,
    /// `moves`).
    pub binds: Binds,
    /// Shots find a target near the line, the player's and the enemies'
    /// (`combat::aim`).
    pub aim_assist: bool,
    /// The player's model's frames for what it does (`c.player.frames`),
    /// and its parts swinging (`c.player.swing`).
    pub player_frames: BTreeMap<String, Frames>,
    pub player_swing: BTreeMap<String, f32>,
}

/// What an action of the player's attacks with, in order: the first that
/// can attacks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    Weapon(usize),
    /// The gun in hand (`player.state.weapon`).
    Held,
}

/// The player's actions as the rules use them: the kit's actions in order
/// (its attacks by name, then its moves), what each attack uses, which
/// action jumps and which strafes.
#[derive(Clone, Debug, Default)]
pub struct Binds {
    pub actions: Vec<&'static str>,
    /// The action's place in `actions` and its weapons in order.
    pub attacks: Vec<(usize, Vec<Choice>)>,
    pub jump: Option<usize>,
    pub strafe: Option<usize>,
}

/// The moves the shooter's player may have.
pub const MOVES: [&str; 2] = ["jump", "strafe"];

impl Binds {
    /// The actions `c.player.attacks` and `moves` name: the attacks in the
    /// order of their names, then the moves.
    pub fn actions_of(own: &super::config::PlayerConfig) -> Vec<&'static str> {
        let mut actions: Vec<&'static str> = own.attacks.keys().map(|a| crate::names::intern(a)).collect();
        for m in &own.moves {
            let m = crate::names::intern(m);
            if !actions.contains(&m) {
                actions.push(m);
            }
        }
        actions
    }
}

#[derive(Clone, Debug)]
pub struct Kind {
    pub name: String,
    /// Its model's file (None: drawn as nothing, a block for the simulation).
    pub model: Option<String>,
    /// Hits it takes.
    pub hp: i32,
    /// Touched within this on the ground, not by its box (`reach`).
    pub reach: Option<f32>,
    /// Whose side it is on: rounds and blows hurt other teams and none.
    pub team: Option<&'static str>,
    /// What does not hurt it.
    pub immune: Immune,
    /// Drawn facing exactly its way (`rotates = "free"`).
    pub free_turn: bool,
    /// How fast its model turns round as it is drawn, degrees a second.
    pub spin: f32,
    /// Its model's frames for what it does: a soldier's or a hostage's
    /// idle, walk, jump; a gunship's idle (parked) and fly.
    pub frames: BTreeMap<String, Frames>,
    /// A figure of parts: the parts that swing as it walks (degrees).
    pub swing: BTreeMap<String, f32>,
    pub rules: KindRules,
}

/// What does not hurt a kind (`immune = { "round", "melee", "blast" }`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Immune {
    pub round: bool,
    pub melee: bool,
    pub blast: bool,
}

impl Immune {
    fn of(list: &[String]) -> Self {
        let has = |h: &str| list.iter().any(|l| l == h);
        Self { round: has("round"), melee: has("melee"), blast: has("blast") }
    }
}

/// Whether what a team (`by`) does hurts one of team `of`: unless both are
/// of the same team (who has none is hurt by all and hurts all).
pub fn hurts(by: Option<&str>, of: Option<&str>) -> bool {
    !matches!((by, of), (Some(a), Some(b)) if a == b)
}

/// How tall the figures were that the soldier's and the hostage's defaults
/// were tuned on (`kit.toml`'s scale): a kind of another height that
/// leaves a length or speed unset gets it scaled to its own height.
pub const TUNED_ON: f32 = 10.0;

/// Scales the lengths and speeds a kind leaves unset (not in `params`) by
/// `f`, its height over `TUNED_ON`.
fn scale(params: &toml::Table, f: f32, keys: &mut [(&str, &mut f32)]) {
    for (key, value) in keys.iter_mut() {
        if !params.contains_key(*key) {
            **value *= f;
        }
    }
}

/// The behavior's name as `c.things.NAME.behavior` writes it (none for
/// the script's kinds).
pub fn behavior_name(rules: &KindRules) -> &'static str {
    match rules {
        KindRules::Script => "",
        KindRules::Soldier(_) => "soldier",
        KindRules::Spawner(_) => "spawner",
        KindRules::Captive(_) => "captive",
        KindRules::Gunship(_) => "gunship",
        KindRules::Pickup(_) => "pickup",
    }
}

/// What a kind does: the behavior and its numbers.
#[derive(Clone, Debug)]
pub enum KindRules {
    /// No behavior: a Lua game's script moves it (`lua::host::Kit`).
    Script,
    Soldier(SoldierRules),
    Spawner(SpawnerRules),
    Captive(CaptiveRules),
    Gunship(GunshipRules),
    Pickup(PickupRules),
}

/// A soldier walks about until he sees or hears the player, then keeps his
/// distance and fires (or throws) along the 8 ways.
#[derive(Clone, Debug)]
pub struct SoldierRules {
    pub weapon: usize,
    /// Running speed when alert, walking speed otherwise, and how quickly
    /// he gets to either.
    pub speed: f32,
    pub idle_speed: f32,
    pub accel: f32,
    /// How far he sees the player, hears gunfire (rounds hitting near him:
    /// half that) and explosions; a comrade falling he notices as far as
    /// he sees.
    pub sight: f32,
    pub hearing: f32,
    pub blast_hearing: f32,
    /// Further from the player he waits where he is.
    pub active: f32,
    /// How far he shoots.
    pub range: f32,
    /// The distance he keeps from the player.
    pub distance: f32,
    /// Where his shots leave the gun (ahead, up; see `muzzle_at`).
    pub muzzle: Option<[f32; 2]>,
    /// The dust his steps and landings raise.
    pub steps: Steps,
}

/// The dust a figure raises (a soldier's, the player's): a puff of
/// `step_dust` specks every `stride` voxels walked (half its height
/// unset), and one of `landing_dust` as it lands from higher than
/// `landing` voxels (the player's landing is heard too).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steps {
    pub stride: Option<f32>,
    pub step_dust: u32,
    pub landing: f32,
    pub landing_dust: u32,
}

impl Default for Steps {
    fn default() -> Self {
        Self { stride: None, step_dust: 3, landing: 2.0, landing_dust: 10 }
    }
}

/// A hut sends `count` men of kind `spawns`, the first `first` ticks after
/// the player comes within `range`, then one every `every` ticks.
#[derive(Clone, Debug)]
pub struct SpawnerRules {
    pub spawns: usize,
    pub count: u32,
    pub first: u32,
    pub every: u32,
    pub range: f32,
    /// Only while the hut lies this way of the player (x east, y north),
    /// by more than `ahead_by` (unset: his height); none: on any side.
    pub ahead: Option<glam::Vec2>,
    pub ahead_by: Option<f32>,
    /// Nobody comes out while the player is nearer than this.
    pub not_near: f32,
}

/// A hostage in a cage: touched within `reach` of the player, freed by
/// the game's code (`thing.state.freed`: the cage bursts within 0.6 of
/// its reach), then it runs off as `flee` says.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CaptiveRules {
    pub speed: f32,
    pub accel: f32,
    pub reach: f32,
    pub flee: Flee,
}

/// Where a hostage freed runs: a way (`flee = { 0, -1 }`, x east, y
/// north), `"away"` from the player, or to a place of the level by its
/// name (in a level without it: away).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "FleeSpec")]
pub enum Flee {
    Way(glam::Vec3),
    Away,
    Place(String),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FleeSpec {
    Way([f32; 2]),
    Name(String),
}

impl TryFrom<FleeSpec> for Flee {
    type Error = String;

    fn try_from(spec: FleeSpec) -> Result<Self, String> {
        match spec {
            FleeSpec::Way([x, y]) if x != 0.0 || y != 0.0 => Ok(Flee::Way(glam::Vec3::new(x, y, 0.0).normalize())),
            FleeSpec::Way(_) => Err("flee: a way such as { 0, -1 } (south), \"away\" or a place's name".into()),
            FleeSpec::Name(name) if name == "away" => Ok(Flee::Away),
            FleeSpec::Name(name) => Ok(Flee::Place(name)),
        }
    }
}

/// A gunship parked until the player comes within `wake`, then flying at
/// `altitude` over its pad (at least `clearance` over what stands below),
/// holding at `offset` from him (x east, y north), firing `weapon` from the guns at
/// `gun` and `rockets` from the pods at `pods` (across, along, up in its
/// model; mirrored for the other side). `size` is its hit box (across,
/// along, high).
#[derive(Clone, Debug)]
pub struct GunshipRules {
    pub weapon: usize,
    pub rockets: usize,
    pub speed: f32,
    pub altitude: f32,
    pub clearance: f32,
    pub offset: [f32; 2],
    pub wake: f32,
    pub size: [f32; 3],
    pub gun: [f32; 3],
    pub pods: [f32; 3],
    /// It turns its side to the player and the gun on that side fires
    /// (door gunners); else it faces him and its guns take turns.
    pub side_on: bool,
    pub crash_blast: f32,
    /// What its crash does to those near (as a weapon's `damage`).
    pub crash_damage: i32,
}

/// A pickup: it lies where it is placed, touched by the player's hips
/// within `reach` of its middle; what it gives is the game's code
/// (`props`, `on_touch`).
#[derive(Clone, Debug)]
pub struct PickupRules {
    /// Its model's frame.
    pub frame: usize,
    pub reach: f32,
    /// It bobs up this many voxels and down again (0: it lies still),
    /// `bob_ticks` up, as many down.
    pub bob: u32,
    pub bob_ticks: u32,
}

/// A weapon, the player's or anyone's.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WeaponRules {
    /// What the HUD and the status line call it.
    pub label: String,
    /// Ticks between shots (a gunship's gun: between the rounds of a burst).
    pub cooldown: u32,
    /// A random number of ticks below this on top of `cooldown` (0: none).
    pub jitter: u32,
    /// Rounds in a burst, and the ticks after one (a gunship's gun).
    pub burst: u32,
    pub pause: u32,
    /// How fast a round flies, and for how many seconds.
    pub speed: f32,
    pub life: f32,
    /// Hits a round takes off.
    pub damage: i32,
    /// The crater where a round hits the world (0: sparks); a rocket's or a
    /// grenade's blast.
    pub blast: f32,
    /// How far rounds stray at random, with aim assist.
    pub spread: f32,
    /// Blows up where it hits, hurting everyone near.
    pub rocket: bool,
    /// A full throw, for thrown weapons (grenades); 0 for guns.
    pub throw: f32,
    /// How many of its rounds an inventory holds: the player's weapon's rounds
    /// are an item of its name (0: it never runs out).
    pub ammo: u32,
    /// The sound a shot makes: the kit's or one of the game's files in
    /// `sounds/`.
    pub sound: crate::names::Name,
    /// A blow, not a shot: it reaches this far (voxels, along the ground)
    /// to one in front, within `cone` degrees, at the attacker's height.
    pub reach: f32,
    pub cone: f32,
    /// It attacks again while its action is held (false: once a press).
    pub auto: bool,
    /// How far it is heard (none: as far as a listener hears shots; 0:
    /// silent).
    pub noise: Option<f32>,
    /// Its blast hurts its own team too (false: it spares them).
    pub friendly_fire: bool,
    /// Its blast does less towards the edge (true), or its whole damage.
    pub falloff: bool,
}

impl Default for WeaponRules {
    fn default() -> Self {
        Self {
            label: String::new(),
            cooldown: 0,
            jitter: 0,
            burst: 1,
            pause: 0,
            speed: 0.0,
            life: 1.0,
            damage: 1,
            blast: 0.0,
            spread: 0.0,
            rocket: false,
            throw: 0.0,
            ammo: 0,
            sound: crate::names::Name(Sfx::Shot.name()),
            reach: 0.0,
            cone: 90.0,
            auto: true,
            noise: None,
            friendly_fire: true,
            falloff: false,
        }
    }
}

impl Rules {
    /// The item a weapon of the player's spends (its own name, when it has
    /// `ammo`); none: it never runs out.
    pub fn rounds_of(&self, weapon: usize) -> Option<&str> {
        (self.weapons[weapon].ammo > 0).then(|| self.weapon_names[weapon].as_str())
    }
}

impl WeaponRules {
    /// Thrown, not fired.
    pub fn thrown(&self) -> bool {
        self.throw > 0.0
    }

    /// A blow from close by, not a shot.
    pub fn melee(&self) -> bool {
        self.reach > 0.0
    }
}

macro_rules! rules_of {
    ($name:ident, $variant:ident, $rules:ty) => {
        pub fn $name(&self, kind: usize) -> &$rules {
            match &self.kinds[kind].rules {
                KindRules::$variant(r) => r,
                other => panic!("{} is no {}: {other:?}", self.kinds[kind].name, stringify!($variant)),
            }
        }
    };
}

impl Rules {
    rules_of!(soldier, Soldier, SoldierRules);
    rules_of!(spawner, Spawner, SpawnerRules);
    rules_of!(captive, Captive, CaptiveRules);
    rules_of!(gunship, Gunship, GunshipRules);
    rules_of!(pickup, Pickup, PickupRules);


    /// The kind called `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.kinds.iter().position(|k| k.name == name)
    }

    /// Every weapon and kind with the numbers in effect, a line each as
    /// `conf.lua` would write it (`voxcade kinds`): nothing hidden.
    pub fn numbers(&self) -> Vec<String> {
        use crate::config::{conf_line, lua_list};
        let text = |s: &str| format!("{s:?}");
        let mut lines = vec![format!("c.weapons.aim_assist = {}", self.aim_assist)];
        for (name, w) in self.weapon_names.iter().zip(&self.weapons) {
            let mut keys = vec![
                ("label", text(&w.label)), ("cooldown", w.cooldown.to_string()), ("jitter", w.jitter.to_string()), ("burst", w.burst.to_string()),
                ("pause", w.pause.to_string()), ("speed", w.speed.to_string()), ("life", w.life.to_string()), ("damage", w.damage.to_string()),
                ("blast", w.blast.to_string()), ("spread", w.spread.to_string()), ("rocket", w.rocket.to_string()), ("throw", w.throw.to_string()),
                ("ammo", w.ammo.to_string()), ("reach", w.reach.to_string()), ("cone", w.cone.to_string()), ("auto", w.auto.to_string()),
            ];
            if let Some(n) = w.noise {
                keys.push(("noise", n.to_string()));
            }
            keys.extend([("friendly_fire", w.friendly_fire.to_string()), ("falloff", w.falloff.to_string()), ("sound", text(w.sound.0))]);
            lines.push(conf_line("weapons", name, &keys));
        }
        for k in self.kinds.iter().filter(|k| k.name != crate::grid::PIECE) {
            let mut keys = vec![("behavior", text(behavior_name(&k.rules)))];
            if let Some(m) = &k.model {
                keys.push(("model", text(m)));
            }
            keys.push(("hp", k.hp.to_string()));
            if let Some(t) = k.team {
                keys.push(("team", text(t)));
            }
            let immune: Vec<&str> = [("round", k.immune.round), ("melee", k.immune.melee), ("blast", k.immune.blast)].into_iter().filter(|(_, on)| *on).map(|(h, _)| h).collect();
            if !immune.is_empty() {
                keys.push(("immune", format!("{{ {} }}", immune.iter().map(|h| text(h)).collect::<Vec<_>>().join(", "))));
            }
            if let Some(r) = k.reach {
                keys.push(("reach", r.to_string()));
            }
            let weapon = |w: usize| text(&self.weapon_names[w]);
            match &k.rules {
                KindRules::Script => keys[0].1 = "nil".into(),
                KindRules::Soldier(r) => {
                    keys.extend([
                        ("weapon", weapon(r.weapon)), ("speed", r.speed.to_string()), ("idle_speed", r.idle_speed.to_string()), ("accel", r.accel.to_string()),
                        ("sight", r.sight.to_string()), ("hearing", r.hearing.to_string()), ("blast_hearing", r.blast_hearing.to_string()),
                        ("active", r.active.to_string()), ("range", r.range.to_string()), ("distance", r.distance.to_string()),
                    ]);
                    if let Some(m) = r.muzzle {
                        keys.push(("muzzle", lua_list(&m)));
                    }
                    if let Some(stride) = r.steps.stride {
                        keys.push(("stride", stride.to_string()));
                    }
                    keys.extend([
                        ("step_dust", r.steps.step_dust.to_string()), ("landing", r.steps.landing.to_string()), ("landing_dust", r.steps.landing_dust.to_string()),
                    ]);
                }
                KindRules::Spawner(r) => {
                    keys.extend([("spawns", text(&self.kinds[r.spawns].name)), ("count", r.count.to_string()), ("first", r.first.to_string()), ("every", r.every.to_string()), ("range", r.range.to_string())]);
                    if let Some(a) = r.ahead {
                        keys.push(("ahead", lua_list(&[a.x, a.y])));
                    }
                    if let Some(by) = r.ahead_by {
                        keys.push(("ahead_by", by.to_string()));
                    }
                    keys.push(("not_near", r.not_near.to_string()));
                }
                KindRules::Captive(r) => {
                    let flee = match &r.flee {
                        Flee::Way(w) => lua_list(&[w.x, w.y]),
                        Flee::Away => text("away"),
                        Flee::Place(p) => text(p),
                    };
                    keys.extend([("speed", r.speed.to_string()), ("accel", r.accel.to_string()), ("reach", r.reach.to_string()), ("flee", flee)]);
                }
                KindRules::Gunship(r) => keys.extend([
                    // why: the key's name, c.things.NAME.rockets, not a weapon
                    ("weapon", weapon(r.weapon)), ("rockets", weapon(r.rockets)), ("speed", r.speed.to_string()), ("altitude", r.altitude.to_string()),
                    ("clearance", r.clearance.to_string()), ("offset", lua_list(&r.offset)), ("wake", r.wake.to_string()), ("size", lua_list(&r.size)),
                    ("gun", lua_list(&r.gun)), ("pods", lua_list(&r.pods)), ("side_on", r.side_on.to_string()), ("crash_blast", r.crash_blast.to_string()),
                    ("crash_damage", r.crash_damage.to_string()),
                ]),
                KindRules::Pickup(r) => keys.extend([("frame", r.frame.to_string()), ("reach", r.reach.to_string()), ("bob", r.bob.to_string()), ("bob_ticks", r.bob_ticks.to_string())]),
            }
            lines.push(conf_line("things", &k.name, &keys));
        }
        lines
    }

    /// The rules a game's config gives: its weapons (`c.weapons.NAME`, the
    /// game's alone: `kit.toml` has the kit's to copy) and its kinds of
    /// things (`c.things.NAME`, the shooter's behaviors).
    /// A kind that leaves a length or speed unset gets the default scaled
    /// to its height (`tall`: a model file's, if known) from the height it
    /// was tuned on (`TUNED_ON`).
    pub fn from_game(cfg: &GameConfig, own: &ShooterConfig, tall: &dyn Fn(&str) -> Option<f32>) -> Result<Self> {
        let mut tables: BTreeMap<String, toml::Table> = BTreeMap::new();
        let mut aim_assist = true;
        for (key, value) in &own.weapons {
            match (key.as_str(), value) {
                ("aim_assist", toml::Value::Boolean(on)) => aim_assist = *on,
                ("aim_assist", _) => bail!("c.weapons.aim_assist: true or false"),
                (name, toml::Value::Table(table)) => tables.entry(name.to_string()).or_default().extend(table.clone()),
                (name, _) => bail!("c.weapons.{name}: a weapon is a table of its own, c.weapons.{name} = {{ ... }}"),
            }
        }
        // A mistake shows in the table that makes it, not in one like it;
        // a shot's sound is one the game has, the kit's or its own.
        let known = |s: &str| Sfx::ALL.iter().any(|f| f.name() == s) || cfg.sounds.contains_key(s);
        for (name, table) in &tables {
            written::<WeaponRules>(table, &format!("c.weapons.{name}"))?;
            if let Some(toml::Value::String(sound)) = table.get("sound")
                && !known(sound)
            {
                let mut all: Vec<&str> = Sfx::ALL.iter().map(|f| f.name()).chain(cfg.sounds.keys().map(String::as_str)).collect();
                all.sort();
                all.dedup();
                bail!("c.weapons.{name}.sound = \"{sound}\": no sound called that (a sound is a file, sounds/{sound}.toml; the game's and the ready-made rules': {})", all.join(", "));
            }
        }
        let tables = resolve_like("weapons", &tables)?;
        let names: Vec<String> = tables.keys().cloned().collect();
        let weapon_names: Vec<&String> = tables.keys().collect();
        let weapon = |name: &str, what: &str| {
            if name.is_empty() {
                bail!("{what}: the weapon it attacks with, by its name in c.weapons ({})", if weapon_names.is_empty() { "none yet: c.weapons.NAME = { ... }, src/shooter/kit.toml has the kit's to copy".to_string() } else { weapon_names.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ") });
            }
            weapon_names.iter().position(|n| *n == name).with_context(|| format!("{what} = \"{name}\": there is no c.weapons.{name}"))
        };
        let weapons = tables
            .iter()
            .map(|(name, table)| toml::Value::Table(table.clone()).try_into().with_context(|| format!("c.weapons.{name}")))
            .collect::<Result<Vec<WeaponRules>>>()?;
        let actions = Binds::actions_of(&own.player);
        let mut binds = Binds { actions: actions.clone(), ..Binds::default() };
        for (action, list) in &own.player.attacks {
            if list.is_empty() {
                bail!("c.player.attacks.{action}: the weapons it attacks with, in order (the first that can)");
            }
            let choices = list
                .iter()
                .map(|w| match w.as_str() {
                    "held" => Ok(Choice::Held),
                    name => weapon(name, &format!("c.player.attacks.{action}")).map(Choice::Weapon),
                })
                .collect::<Result<Vec<_>>>()?;
            let at = actions.iter().position(|a| *a == action.as_str()).expect("an attack's action");
            binds.attacks.push((at, choices));
        }
        for m in &own.player.moves {
            let at = actions.iter().position(|a| *a == m.as_str()).expect("a move's action");
            match m.as_str() {
                "jump" => binds.jump = Some(at),
                "strafe" => binds.strafe = Some(at),
                other => bail!("c.player.moves: \"{other}\" is none of the shooter's moves ({})", MOVES.join(", ")),
            }
        }
        if names.iter().any(|n| n == "held") {
            bail!("c.weapons.held: held is the word for the gun in hand, no weapon's name");
        }

        let defs = things::defs(cfg, &super::BEHAVIORS)?;
        let kind_index = |name: &str, what: &str| {
            defs.iter().position(|d| d.name == name).with_context(|| format!("{what} = \"{name}\": there is no c.things.{name}"))
        };
        let kinds = defs
            .iter()
            .map(|def| -> Result<Kind> {
                let name = &def.name;
                let read = || -> Result<toml::Value> { Ok(toml::Value::Table(def.params.clone())) };
                // (The engine's own kind of pieces has no table of the game's.)
                let none = toml::Table::new();
                let own = cfg.things.get(name).unwrap_or(&none);
                let here_written = format!("c.things.{name}");
                let parts = |hp: bool, model: bool| -> Result<()> {
                    if !hp && def.hp.is_some() {
                        bail!("c.things.{name}.hp: a {} takes no hits", def.behavior.as_deref().unwrap_or("thing"));
                    }
                    if !model && def.model.is_some() {
                        bail!("c.things.{name}.model: a {} is not drawn", def.behavior.as_deref().unwrap_or("thing"));
                    }
                    Ok(())
                };
                let here = |key: &str| format!("c.things.{name}.{key}");
                // Its height over the one the defaults were tuned on (none
                // known: as tuned).
                let factor = || def.model.as_deref().and_then(tall).map_or(1.0, |h| h.max(1.0) / TUNED_ON);
                // Numbers the kit no longer reads: what a thing is worth and
                // what it gives are the game's code.
                let moved: &[(&str, &str)] = match def.behavior.as_deref() {
                    Some("pickup") => &[
                        ("weapon", "what a pickup gives is your code's: props = { weapon = \"machine_gun\", ammo = 150 }, and player.state.weapon, player.state.ammo in on_touch"),
                        ("grenades", "what a pickup gives is your code's: props = { grenades = 5 }, and player.state.grenades in on_touch"),
                        ("heal", "what a pickup gives is your code's: props = { heal = 2 }, and player.state.hp in on_touch"),
                        ("item", "what a pickup gives is your code's: props = { item = \"key\" }, and player:take(b.props.item) in on_touch"),
                        ("score", "what a thing is worth is your code's: props = { worth = 50 }, added in on_touch"),
                    ],
                    Some("soldier") => &[
                        ("keep", "how far a soldier keeps from the player is distance = 64 now (keep is the word for a kept kind)"),
                        ("score", "what a soldier is worth is your code's: props = { worth = 100, knife_worth = 150 }, added in on_death (its weapon says knife)"),
                        ("knife_score", "what a soldier is worth is your code's: props = { worth = 100, knife_worth = 150 }, added in on_death (its weapon says knife)"),
                    ],
                    Some("gunship") => &[
                        ("keep", "where a gunship holds from the player is offset = { 0, 64 } now (x east, y north; keep is the word for a kept kind)"),
                        ("score", "what a gunship is worth is your code's: props = { worth = 5000 }, added in on_death"),
                    ],
                    Some("captive") => &[("score", "freeing a hostage is your code's: thing.state.freed = true where the player touches it, and props = { worth = 500 } added there")],
                    _ => &[],
                };
                if let Some((key, instead)) = moved.iter().find(|(k, _)| own.contains_key(*k)) {
                    bail!("c.things.{name}.{key} is no more: {instead} (docs/game-model.md)");
                }
                let (team, immune) = (def.team.as_deref().map(crate::names::intern), Immune::of(&def.immune));
                let Some(behavior) = def.behavior.as_deref() else {
                    if let Some(key) = def.params.keys().next() {
                        bail!("c.things.{name}.{key}: a kind without a behavior (the script's) has a model and hp, nothing more");
                    }
                    return Ok(Kind { name: name.clone(), model: def.model.clone(), hp: def.hp.unwrap_or(0), reach: def.reach, team, immune, free_turn: def.free_turn, spin: def.spin, frames: def.frames.clone(), swing: def.swing.clone(), rules: KindRules::Script });
                };
                let rules = match behavior {
                    "soldier" => {
                        parts(true, true)?;
                        written::<SoldierSpec>(own, &here_written)?;
                        let mut spec: SoldierSpec = read()?.try_into().with_context(|| format!("c.things.{name}"))?;
                        scale(&def.params, factor(), &mut [
                            ("speed", &mut spec.speed), ("idle_speed", &mut spec.idle_speed), ("accel", &mut spec.accel),
                            ("sight", &mut spec.sight), ("hearing", &mut spec.hearing), ("blast_hearing", &mut spec.blast_hearing),
                            ("active", &mut spec.active), ("range", &mut spec.range), ("distance", &mut spec.distance),
                        ]);
                        let gun = weapon(&spec.weapon, &here("weapon"))?;
                        KindRules::Soldier(spec.rules(gun))
                    }
                    "spawner" => {
                        parts(false, false)?;
                        written::<SpawnerSpec>(own, &here_written)?;
                        let spec: SpawnerSpec = read()?.try_into().with_context(|| format!("c.things.{name}"))?;
                        if spec.spawns.is_empty() {
                            bail!("c.things.{name}.spawns: the kind of soldier it sends");
                        }
                        let spawns = kind_index(&spec.spawns, &here("spawns"))?;
                        if defs[spawns].behavior.as_deref() != Some("soldier") {
                            bail!("c.things.{name}.spawns = \"{}\": a hut sends soldiers", spec.spawns);
                        }
                        let ahead = match spec.ahead.map(glam::Vec2::from) {
                            Some(way) if way == glam::Vec2::ZERO => bail!("c.things.{name}.ahead: a way such as {{ 0, 1 }} (north), or none for any side"),
                            way => way.map(glam::Vec2::normalize),
                        };
                        KindRules::Spawner(SpawnerRules { spawns, count: spec.count, first: spec.first, every: spec.every, range: spec.range, ahead, ahead_by: spec.ahead_by, not_near: spec.not_near })
                    }
                    "captive" => {
                        parts(true, true)?;
                        written::<CaptiveRules>(own, &here_written)?;
                        let mut rules: CaptiveRules = read()?.try_into().with_context(|| format!("c.things.{name}"))?;
                        scale(&def.params, factor(), &mut [("speed", &mut rules.speed), ("accel", &mut rules.accel), ("reach", &mut rules.reach)]);
                        KindRules::Captive(CaptiveRules { reach: def.reach.unwrap_or(rules.reach), ..rules })
                    }
                    "gunship" => {
                        parts(true, true)?;
                        written::<GunshipSpec>(own, &here_written)?;
                        let spec: GunshipSpec = read()?.try_into().with_context(|| format!("c.things.{name}"))?;
                        // why: the key's name, c.things.NAME.rockets, not a weapon
                        let (gun, rockets) = (weapon(&spec.weapon, &here("weapon"))?, weapon(&spec.rockets, &here("rockets"))?);
                        KindRules::Gunship(spec.rules(gun, rockets))
                    }
                    "pickup" => {
                        parts(false, true)?;
                        written::<PickupSpec>(own, &here_written)?;
                        let spec: PickupSpec = read()?.try_into().with_context(|| format!("c.things.{name}"))?;
                        // Unset, it bobs a fifth of its height.
                        let bob = spec.bob.unwrap_or_else(|| def.model.as_deref().and_then(tall).map_or(1, |h| (h as u32 / 5).max(1)));
                        KindRules::Pickup(PickupRules { frame: spec.frame, reach: def.reach.unwrap_or(PickupRules::default().reach), bob, bob_ticks: spec.bob_ticks })
                    }
                    other => bail!("c.things.{name}.behavior = \"{other}\": not the shooter's"),
                };
                let hp = def.hp.unwrap_or(match rules {
                    KindRules::Soldier(_) | KindRules::Captive(_) => 1,
                    KindRules::Gunship(_) => 30,
                    _ => 0,
                });
                Ok(Kind { name: name.clone(), model: def.model.clone(), hp, reach: def.reach, team, immune, free_turn: def.free_turn, spin: def.spin, frames: def.frames.clone(), swing: def.swing.clone(), rules })
            })
            .collect::<Result<Vec<_>>>()?;
        let fps = own.player.fps.map_or(things::FPS, |f| f.max(0.1));
        let player_frames = things::frame_sets("c.player.frames", own.player.frames.clone(), fps)?;
        Ok(Self { kinds, weapons, weapon_names: names, binds, aim_assist, player_frames, player_swing: own.player.swing.clone() })
    }
}

/// Reads a table as it is written, without what it is like or the keys the
/// engine reads (`like`, `behavior`, `model`, `hp`): an unknown key or a
/// wrong value is reported where it stands.
fn written<T: serde::de::DeserializeOwned>(table: &toml::Table, what: &str) -> Result<()> {
    let mut table = table.clone();
    for key in ["like", "behavior", "model", "hp"] {
        table.remove(key);
    }
    // A kind's keys every kind has (`sim::things::Def`).
    if what.starts_with("c.things.") {
        for key in things::COMMON {
            table.remove(key);
        }
    }
    toml::Value::Table(table).try_into::<T>().with_context(|| what.to_string())?;
    Ok(())
}

/// The kit: its weapons, and kinds of things to copy into a game.
pub const KIT: &str = include_str!("kit.toml");

/// A soldier as `[things.NAME]` writes it (`SoldierRules`, the weapon by
/// name); unset numbers are the kit's rifleman's.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SoldierSpec {
    weapon: String,
    speed: f32,
    idle_speed: f32,
    accel: f32,
    sight: f32,
    hearing: f32,
    blast_hearing: f32,
    active: f32,
    range: f32,
    distance: f32,
    muzzle: Option<[f32; 2]>,
    stride: Option<f32>,
    step_dust: u32,
    landing: f32,
    landing_dust: u32,
}

impl Default for SoldierSpec {
    fn default() -> Self {
        Self {
            weapon: String::new(),
            speed: 24.0,
            idle_speed: 10.0,
            accel: 300.0,
            sight: 60.0,
            hearing: 70.0,
            blast_hearing: 130.0,
            active: 150.0,
            range: 95.0,
            distance: 32.0,
            muzzle: None,
            stride: Steps::default().stride,
            step_dust: Steps::default().step_dust,
            landing: Steps::default().landing,
            landing_dust: Steps::default().landing_dust,
        }
    }
}

impl SoldierSpec {
    fn rules(self, weapon: usize) -> SoldierRules {
        SoldierRules {
            weapon,
            speed: self.speed,
            idle_speed: self.idle_speed,
            accel: self.accel,
            sight: self.sight,
            hearing: self.hearing,
            blast_hearing: self.blast_hearing,
            active: self.active,
            range: self.range,
            distance: self.distance,
            muzzle: self.muzzle,
            steps: Steps { stride: self.stride, step_dust: self.step_dust, landing: self.landing, landing_dust: self.landing_dust },
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SpawnerSpec {
    spawns: String,
    count: u32,
    first: u32,
    every: u32,
    range: f32,
    ahead: Option<[f32; 2]>,
    ahead_by: Option<f32>,
    not_near: f32,
}

impl Default for SpawnerSpec {
    fn default() -> Self {
        Self { spawns: String::new(), count: 2, first: 60, every: 420, range: 120.0, ahead: None, ahead_by: None, not_near: 0.0 }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GunshipSpec {
    weapon: String,
    rockets: String,
    speed: f32,
    altitude: f32,
    clearance: f32,
    offset: [f32; 2],
    wake: f32,
    size: [f32; 3],
    gun: [f32; 3],
    pods: [f32; 3],
    side_on: bool,
    crash_blast: f32,
    crash_damage: i32,
}

impl Default for GunshipSpec {
    fn default() -> Self {
        Self {
            weapon: String::new(),
            rockets: String::new(),
            speed: 35.0,
            altitude: 17.0,
            clearance: 4.0,
            offset: [0.0, 65.0],
            wake: 130.0,
            size: [12.0, 24.0, 8.0],
            gun: [0.0, 12.0, 1.5],
            pods: [5.0, 2.5, 2.5],
            side_on: false,
            crash_blast: 9.0,
            crash_damage: 6,
        }
    }
}

impl GunshipSpec {
    fn rules(self, weapon: usize, rockets: usize) -> GunshipRules {
        GunshipRules {
            weapon,
            rockets,
            speed: self.speed,
            altitude: self.altitude,
            clearance: self.clearance,
            offset: self.offset,
            wake: self.wake,
            size: self.size,
            gun: self.gun,
            pods: self.pods,
            side_on: self.side_on,
            crash_blast: self.crash_blast,
            crash_damage: self.crash_damage,
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PickupSpec {
    frame: usize,
    bob: Option<u32>,
    bob_ticks: u32,
}

impl Default for PickupSpec {
    fn default() -> Self {
        let r = PickupRules::default();
        Self { frame: r.frame, bob: None, bob_ticks: r.bob_ticks }
    }
}

impl Default for PickupRules {
    fn default() -> Self {
        Self { frame: 0, reach: 5.0, bob: 1, bob_ticks: 20 }
    }
}

impl Default for CaptiveRules {
    fn default() -> Self {
        Self { speed: 30.0, accel: 399.0, reach: 8.0, flee: Flee::Away }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shooter::tests::kit;

    fn rules(cfg: &GameConfig) -> Result<Rules> {
        Rules::from_game(cfg, &ShooterConfig::from_game(cfg)?, &|_| None)
    }

    /// A kind that says only its behavior gets the numbers of the kit's
    /// kind of that behavior (kit.toml writes the defaults out).
    #[test]
    fn the_kits_kinds_are_the_behaviors_defaults() {
        let mut cfg = kit();
        for (name, table) in [
            ("bare_soldier", "behavior = \"soldier\"\nweapon = \"enemy_rifle\""),
            ("bare_hut", "behavior = \"spawner\"\nspawns = \"rifleman\""),
            ("bare_hostage", "behavior = \"captive\""),
            ("bare_gunship", "behavior = \"gunship\"\nweapon = \"door_gun\"\nrockets = \"rocket_pods\""),
            ("bare_pickup", "behavior = \"pickup\""),
        ] {
            cfg.things.insert(name.into(), toml::from_str(table).unwrap());
        }
        let r = rules(&cfg).unwrap();
        let same = |bare: &str, kit: &str| {
            let (a, b) = (&r.kinds[r.find(bare).unwrap()], &r.kinds[r.find(kit).unwrap()]);
            assert_eq!((a.hp, format!("{:?}", a.rules)), (b.hp, format!("{:?}", b.rules)), "{bare} is not the kit's {kit}");
        };
        same("bare_soldier", "rifleman");
        same("bare_hut", "hut");
        same("bare_hostage", "hostage");
        same("bare_gunship", "gunship");
        same("bare_pickup", "machine_gun");
    }

    /// A kind's frames, the player's (`c.player.frames`), a soldier's dust
    /// and how a pickup bobs are the game's to set; mistakes say where.
    #[test]
    fn frames_and_bobbing_are_the_games() {
        let mut cfg = kit();
        cfg.things.insert("walker".into(), toml::from_str("behavior = \"soldier\"\nweapon = \"enemy_rifle\"\nstride = 7\nstep_dust = 0\nfps = 4\nframes = { idle = { first = 0, last = 1 }, walk = { first = 2, last = 5, stride = 6 } }").unwrap());
        cfg.things.insert("crate".into(), toml::from_str("behavior = \"pickup\"\nmodel = \"crate.vox\"").unwrap());
        cfg.things.insert("still".into(), toml::from_str("behavior = \"pickup\"\nmodel = \"crate.vox\"\nbob = 0\nbob_ticks = 8").unwrap());
        cfg.player.game.insert("frames".into(), toml::from_str::<toml::Table>("jump = { first = 6, last = 7, loop = false }").unwrap().into());
        let r = Rules::from_game(&cfg, &ShooterConfig::from_game(&cfg).unwrap(), &|file| (file == "crate.vox").then_some(15.0)).unwrap();
        let walker = &r.kinds[r.find("walker").unwrap()].frames;
        assert_eq!((walker["idle"].fps, walker["walk"].stride), (4.0, Some(6.0)), "the kind's speed, its own stride");
        assert_eq!(r.soldier(r.find("walker").unwrap()).steps, Steps { stride: Some(7.0), step_dust: 0, ..Steps::default() }, "its dust");
        assert_eq!(r.player_frames["jump"], Frames { first: 6, last: 7, fps: things::FPS, stride: None, looped: false });
        let bob = |name: &str| (r.pickup(r.find(name).unwrap()).bob, r.pickup(r.find(name).unwrap()).bob_ticks);
        assert_eq!((bob("crate"), bob("still")), ((3, 20), (0, 8)), "a fifth of its height unset");
        cfg.player.game.insert("frames".into(), toml::from_str::<toml::Table>("walk = [1]").unwrap().into());
        let err = format!("{:#}", rules(&cfg).unwrap_err());
        assert!(err.contains("c.player.frames.walk"), "{err}");
    }

    /// A soldier or a hostage of another height than the kit's 10 voxels
    /// gets the lengths and speeds it leaves unset scaled to its height;
    /// `voxcade kinds` prints the numbers in effect.
    #[test]
    fn unset_lengths_and_speeds_follow_the_height() {
        let mut cfg = kit();
        cfg.things.insert("giant".into(), toml::from_str("behavior = \"soldier\"\nmodel = \"giant.vox\"\nweapon = \"enemy_rifle\"\nsight = 100").unwrap());
        let r = Rules::from_game(&cfg, &ShooterConfig::from_game(&cfg).unwrap(), &|file| (file == "giant.vox").then_some(22.0)).unwrap();
        let giant = r.soldier(r.find("giant").unwrap());
        let rifleman = r.soldier(r.find("rifleman").unwrap());
        assert_eq!((giant.speed, giant.sight, giant.distance), (24.0 * 2.2, 100.0, 32.0 * 2.2), "2.2 times as tall, its own sight");
        assert_eq!(rifleman.speed, 24.0, "no model known: as tuned");
        assert!(r.numbers().iter().any(|l| l.starts_with("c.things.giant = { behavior = \"soldier\"") && l.contains("sight = 100,")), "{:?}", r.numbers());
    }

    /// A game's weapons are its own (`kit.toml` has the kit's to copy): one
    /// like another takes its keys and changes some; unset keys are the
    /// weapons' defaults.
    #[test]
    fn a_games_weapons_are_its_own() {
        let cfg: GameConfig = toml::from_str("[weapons]\naim_assist = false\n[weapons.rifle]\nlabel = \"Rifle\"\ncooldown = 9\nspeed = 300\n[weapons.flamer]\nlike = \"rifle\"\nlife = 0.3\n").unwrap();
        let r = rules(&cfg).unwrap();
        assert_eq!(r.weapon_names, ["flamer", "rifle"], "the game's, no others");
        let weapon = |name: &str| &r.weapons[r.weapon_names.iter().position(|n| n == name).unwrap()];
        assert_eq!((weapon("rifle").speed, weapon("rifle").label.as_str(), weapon("rifle").cooldown, weapon("rifle").life), (300.0, "Rifle", 9, 1.0));
        assert_eq!((weapon("flamer").speed, weapon("flamer").life, weapon("flamer").cooldown), (300.0, 0.3, 9), "the flamer is like the rifle");
        assert!(!r.aim_assist);
    }

    /// Every mistake in a kind or a weapon says where it is.
    #[test]
    fn mistakes_in_kinds_and_weapons_are_errors() {
        let error = |text: &str| {
            let mut cfg = kit();
            let more: GameConfig = toml::from_str(text).unwrap();
            cfg.things.extend(more.things);
            for (key, value) in more.game {
                match (cfg.game.get_mut(&key), value) {
                    (Some(toml::Value::Table(t)), toml::Value::Table(more)) => t.extend(more),
                    (_, value) => {
                        cfg.game.insert(key, value);
                    }
                }
            }
            cfg.player.game = more.player.game;
            rules(&cfg).map(|_| ()).expect_err(text).to_string()
        };
        assert!(error("[things.x]\nbehavior = \"soldier\"\nsped = 3\n").contains("c.things.x"));
        assert!(error("[things.x]\nbehavior = \"soldier\"\nweapon = \"bow\"\n").contains("no c.weapons.bow"));
        assert!(error("[things.x]\nbehavior = \"soldier\"\n").contains("c.things.x.weapon: the weapon it attacks with"));
        assert!(error("[things.x]\nbehavior = \"spawner\"\n").contains("spawns"));
        assert!(error("[things.x]\nbehavior = \"spawner\"\nspawns = \"hostage\"\n").contains("sends soldiers"));
        assert!(error("[things.x]\nbehavior = \"spawner\"\nspawns = \"rifleman\"\nhp = 3\n").contains("takes no hits"));
        assert!(error("[things.x]\nbehavior = \"spawner\"\nspawns = \"rifleman\"\nmodel = \"hut.vox\"\n").contains("not drawn"));
        assert!(error("[things.x]\nbehavior = \"checkpoint\"\n").contains("is no more"));
        assert!(error("[things.x]\nbehavior = \"pickup\"\nheal = 2\n").contains("c.things.x.heal is no more"));
        assert!(error("[things.x]\nbehavior = \"pickup\"\nweapon = \"door_gun\"\n").contains("c.things.x.weapon is no more"));
        assert!(error("[things.x]\nbehavior = \"gunship\"\nkeep = 35\n").contains("offset = { 0, 64 }"));
        assert!(error("[things.x]\nbehavior = \"spawner\"\nspawns = \"rifleman\"\nahead = [0, 0]\n").contains("c.things.x.ahead: a way"));
        let mut cfg = kit();
        cfg.things.extend(toml::from_str::<GameConfig>("[things.x]\nbehavior = \"captive\"\nflee = [0, 0]\n").unwrap().things);
        let e = format!("{:#}", rules(&cfg).map(|_| ()).unwrap_err());
        assert!(e.contains("c.things.x") && e.contains("flee: a way"), "{e}");
        assert!(error("[weapons]\nbullet_speed = 3\n").contains("c.weapons.bullet_speed"));
        let e = error("[weapons.rifle]\nsped = 3\n");
        assert!(e.contains("c.weapons.rifle"), "{e}");
        let e = error("[weapons.rifle]\nsound = \"pew\"\n");
        assert!(e.contains("c.weapons.rifle"), "{e}");
        assert!(error("[player.attacks]\nfire = [\"bow\"]\n").contains("no c.weapons.bow"));
        assert!(error("[player]\nmoves = [\"fly\"]\n").contains("none of the shooter's moves"));
    }
}
