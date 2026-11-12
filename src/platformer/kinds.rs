//! The kinds of things as the platformer's behaviours read them (`Rules`):
//! every number that makes a hero a hero, an opossum a walker, a lift a
//! platform. A thing's `kind` is its place in `Rules::kinds`. The defaults
//! are the kit's (`kit.toml`), at the scale of Sunny Land's pictures.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use glam::IVec3;
use serde::Deserialize;

use crate::config::GameConfig;
use crate::sim::things::{self, Frames};

#[derive(Clone, Debug)]
pub struct Rules {
    pub kinds: Vec<Kind>,
}

#[derive(Clone, Debug)]
pub struct Kind {
    pub name: String,
    /// Its model's file (None: drawn as nothing).
    pub model: Option<String>,
    /// Hits it takes.
    pub hp: i32,
    /// Its body: across and deep, and high.
    pub width: f32,
    pub height: f32,
    /// Its model's size (1 each way without one).
    pub size: IVec3,
    /// Its model's frames for what it does and its poses, by name.
    pub frames: BTreeMap<String, Frames>,
    /// How many a second where a set of them does not say.
    pub fps: f32,
    /// A figure of parts: the parts that swing as it walks (degrees).
    pub swing: BTreeMap<String, f32>,
    /// How fast its model turns round as it is drawn, degrees a second.
    pub spin: f32,
    /// Touched within this on the ground, not by its box.
    pub reach: Option<f32>,
    /// The way a figure faces as it comes in (x across, y into the level:
    /// `{ 0, -1 }` the camera); None: towards the level's middle.
    pub facing: Option<[f32; 2]>,
    pub rules: KindRules,
}

/// What a kind does: its behaviour and the numbers it reads. How things
/// move, never what a touch means: that is the game's code (`on_touch`).
#[derive(Clone, Debug)]
pub enum KindRules {
    /// No behaviour: a Lua game's script moves it.
    Script,
    Hero(HeroRules),
    Walker(WalkerRules),
    Hopper(HopperRules),
    Flyer(FlyerRules),
    /// Spins where it is placed (a gem): what it gives is the game's code.
    Pickup,
    Platform(PlatformRules),
    /// Stands where it is placed, showing its pose (`thing.state.pose`): a
    /// door, spikes, a sign.
    Prop(PropRules),
    /// A box the hero pushes along by walking into it: it falls over
    /// edges, is stood on, holds down what it lies on.
    Crate(CrateRules),
}

impl KindRules {
    /// A figure that takes hits (`thing:hurt`): it goes at 0.
    pub fn takes_hits(&self) -> bool {
        matches!(self, KindRules::Walker(_) | KindRules::Hopper(_) | KindRules::Flyer(_))
    }
}

/// The behaviour's name as `c.things.NAME.behaviour` writes it (none for
/// the script's kinds).
pub fn behaviour_name(rules: &KindRules) -> &'static str {
    match rules {
        KindRules::Script => "",
        KindRules::Hero(_) => "hero",
        KindRules::Walker(_) => "walker",
        KindRules::Hopper(_) => "hopper",
        KindRules::Flyer(_) => "flyer",
        KindRules::Pickup => "pickup",
        KindRules::Platform(_) => "platform",
        KindRules::Prop(_) => "prop",
        KindRules::Crate(_) => "crate",
    }
}

/// The player's figure: runs, jumps (higher held longer), climbs ladders,
/// lands on enemies, is hurt by the rest.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct HeroRules {
    pub speed: f32,
    pub accel: f32,
    pub air_accel: f32,
    pub jump: f32,
    pub jump_cut: f32,
    pub coyote: u32,
    pub buffer: u32,
    pub climb: f32,
    pub invulnerable: u32,
    pub knockback: [f32; 2],
    pub fall_gravity: f32,
    pub hang: f32,
    pub hang_speed: f32,
    pub corner: u32,
    pub look_ahead: f32,
    pub look_ahead_speed: f32,
    /// How far above its head its hands reach for a ledge's edge, voxels
    /// (0: it grabs none).
    pub grab: f32,
    /// How far ahead of its body it finds that edge, voxels.
    pub grab_ahead: f32,
    /// How far under the edge its head hangs, voxels; an edge lower than
    /// that over its head it pulls itself onto at once.
    pub hang_below: f32,
    /// Ticks pulling itself up onto the ledge it hangs from.
    pub pull: u32,
    /// Climbing, it turns to the ladder (its back to the camera).
    pub face_ladder: bool,
    /// While nothing hurts it, it blinks: this many ticks shown, as many
    /// not (0: it does not).
    pub blink: u32,
    /// Jumps it may make again in the air before it lands (a double jump:
    /// 1), and how high such a jump goes, voxels: from where it is then.
    pub air_jumps: u32,
    pub air_jump: f32,
}

impl Default for HeroRules {
    fn default() -> Self {
        Self {
            speed: 150.0,
            accel: 1200.0,
            air_accel: 800.0,
            jump: 29.0,
            jump_cut: 0.5,
            coyote: 6,
            buffer: 6,
            climb: 60.0,
            invulnerable: 60,
            knockback: [100.0, 100.0],
            fall_gravity: 1.5,
            hang: 0.5,
            hang_speed: 30.0,
            corner: 4,
            look_ahead: 40.0,
            look_ahead_speed: 90.0,
            grab: 0.0,
            grab_ahead: 3.0,
            hang_below: 2.0,
            pull: 24,
            face_ladder: true,
            blink: 4,
            air_jumps: 0,
            air_jump: 20.0,
        }
    }
}

/// Walks to and fro, turning at walls (and ledges).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct WalkerRules {
    pub speed: f32,
    pub ledges: bool,
}

impl Default for WalkerRules {
    fn default() -> Self {
        Self { speed: 60.0, ledges: true }
    }
}

/// Hops now and then, left and right in turn.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct HopperRules {
    pub every: u32,
    pub hop: [f32; 2],
}

impl Default for HopperRules {
    fn default() -> Self {
        Self { every: 120, hop: [100.0, 200.0] }
    }
}

/// Hovers up and down where it is placed.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FlyerRules {
    pub range: f32,
    pub period: u32,
    /// It turns to face the hero; off, it keeps looking the way it was
    /// placed.
    pub face_hero: bool,
}

impl Default for FlyerRules {
    fn default() -> Self {
        Self { range: 50.0, period: 120, face_hero: true }
    }
}

/// Stands where it is placed: in the air (a torch on a wall), or with
/// `stands` on the floor below (a door, spikes).
#[derive(Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PropRules {
    pub stands: bool,
}

/// Pushed along by the hero walking into it, this fast at most (voxels a
/// second): a crate.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct CrateRules {
    pub push: f32,
}

impl Default for CrateRules {
    fn default() -> Self {
        Self { push: 30.0 }
    }
}

/// Goes from where it is placed by `travel` and back, waiting at each end,
/// carrying who stands on it (held from above only); or, told to
/// (`thing.state.goal`), to one end, and stays there.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PlatformRules {
    pub travel: [f32; 2],
    pub speed: f32,
    pub pause: u32,
    /// Coming down onto the hero it hurts it and goes on (a trap); else it
    /// waits above it (above a crate, a walker, a hopper too).
    pub crushes: bool,
}

impl Default for PlatformRules {
    fn default() -> Self {
        Self { travel: [64.0, 0.0], speed: 30.0, pause: 30, crushes: false }
    }
}

/// How tall the figures were that a behaviour's defaults were tuned on
/// (Sunny Land's fox, opossum, frog and eagle): a kind of another height
/// that leaves a length or speed unset gets it scaled to its own height.
pub const TUNED_ON: [(&str, f32); 4] = [("hero", 20.0), ("walker", 16.0), ("hopper", 18.0), ("flyer", 24.0)];

/// Scales the lengths and speeds a kind leaves unset (not in `own`) by
/// `f`, its height over the height they were tuned on.
fn scale(own: &toml::Table, f: f32, keys: &mut [(&str, &mut f32)]) {
    for (key, value) in keys.iter_mut() {
        if !own.contains_key(*key) {
            **value *= f;
        }
    }
}

/// The keys every kind may have besides its behaviour's (and the
/// engine's, `things::COMMON`).
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Common {
    width: Option<f32>,
    height: Option<f32>,
    facing: Option<[f32; 2]>,
}

const COMMON: [&str; 3] = ["width", "height", "facing"];

/// Numbers the kit no longer reads: what a touch means is the game's code.
const MOVED: [(&str, &str); 7] = [
    ("hearts", "the hero's hit points are the game's: c.state.player.hp = 3 (player.state.hp)"),
    ("lives", "lives are the game's: c.state.player.lives = 3, counted down in its code"),
    ("score", "what a thing is worth is your code's: props = { worth = 100 }, added in on_touch or on_death"),
    ("item", "what a pickup gives is your code's: props = { item = \"key\" }, and player:take(b.props.item) in on_touch"),
    ("bounce", "the bounce off an enemy is your code's: player:bounce(height) in on_touch"),
    ("after", "a goal is your code's: on_touch, and world:win() when it is time"),
    ("wins", "a goal is your code's: on_touch, and world:win() when it is time"),
];

/// The kit (`kit.toml`): kinds of things to copy into a game.
pub const KIT: &str = include_str!("kit.toml");

impl Rules {
    /// The kinds from the config; `size` gives a model's size (None: no
    /// such model).
    pub fn from_game(cfg: &GameConfig, size: impl Fn(&str) -> Option<IVec3>) -> Result<Self> {
        let defs = things::defs(cfg, &super::BEHAVIOURS)?;
        let kinds = defs
            .iter()
            .map(|def| -> Result<Kind> {
                let name = &def.name;
                let mut own = def.params.clone();
                if let Some((key, instead)) = MOVED.iter().find(|(k, _)| own.contains_key(*k)) {
                    bail!("c.things.{name}.{key} is no more: {instead} (docs/game-model.md)");
                }
                let mut common = toml::Table::new();
                for key in COMMON {
                    if let Some(v) = own.remove(key) {
                        common.insert(key.to_string(), v);
                    }
                }
                let common: Common = toml::Value::Table(common).try_into().with_context(|| format!("c.things.{name}"))?;
                let read = |what: &str| -> Result<toml::Value> {
                    let _ = what;
                    Ok(toml::Value::Table(own.clone()))
                };
                let size = match &def.model {
                    Some(file) => size(file).with_context(|| format!("c.things.{name}.model = \"{file}\": no such model"))?,
                    None => IVec3::ONE,
                };
                // Its height over the one its behaviour's defaults were
                // tuned on (none known: as tuned).
                let tall = common.height.or(def.model.as_ref().map(|_| size.z as f32));
                let f = |behaviour: &str| match (tall, TUNED_ON.iter().find(|(b, _)| *b == behaviour)) {
                    (Some(h), Some((_, on))) => h.max(1.0) / on,
                    _ => 1.0,
                };
                let rules = match def.behaviour.as_deref() {
                    None => {
                        if let Some(key) = own.keys().next() {
                            bail!("c.things.{name}.{key}: a kind without a behaviour (the script's) has a model and hp, nothing more");
                        }
                        KindRules::Script
                    }
                    Some("hero") => {
                        let mut r: HeroRules = read("hero")?.try_into().with_context(|| format!("c.things.{name}"))?;
                        let (f, [up, back]) = (f("hero"), &mut r.knockback);
                        scale(&own, f, &mut [
                            ("speed", &mut r.speed), ("accel", &mut r.accel), ("air_accel", &mut r.air_accel), ("jump", &mut r.jump),
                            ("climb", &mut r.climb), ("hang_speed", &mut r.hang_speed), ("look_ahead", &mut r.look_ahead),
                            ("look_ahead_speed", &mut r.look_ahead_speed), ("grab", &mut r.grab), ("grab_ahead", &mut r.grab_ahead),
                            ("hang_below", &mut r.hang_below), ("air_jump", &mut r.air_jump),
                        ]);
                        scale(&own, f, &mut [("knockback", up), ("knockback", back)]);
                        if !own.contains_key("corner") {
                            r.corner = (r.corner as f32 * f).round() as u32;
                        }
                        KindRules::Hero(r)
                    }
                    Some("walker") => {
                        let mut r: WalkerRules = read("walker")?.try_into().with_context(|| format!("c.things.{name}"))?;
                        scale(&own, f("walker"), &mut [("speed", &mut r.speed)]);
                        KindRules::Walker(r)
                    }
                    Some("hopper") => {
                        let mut r: HopperRules = read("hopper")?.try_into().with_context(|| format!("c.things.{name}"))?;
                        let [across, up] = &mut r.hop;
                        scale(&own, f("hopper"), &mut [("hop", across), ("hop", up)]);
                        KindRules::Hopper(r)
                    }
                    Some("flyer") => {
                        let mut r: FlyerRules = read("flyer")?.try_into().with_context(|| format!("c.things.{name}"))?;
                        scale(&own, f("flyer"), &mut [("range", &mut r.range)]);
                        KindRules::Flyer(r)
                    }
                    Some("pickup") => {
                        if let Some(key) = own.keys().next() {
                            bail!("c.things.{name}.{key}: a pickup has no numbers of its own (what it gives: props, and your code)");
                        }
                        KindRules::Pickup
                    }
                    Some("platform") => KindRules::Platform(read("platform")?.try_into().with_context(|| format!("c.things.{name}"))?),
                    Some("prop") => KindRules::Prop(read("prop")?.try_into().with_context(|| format!("c.things.{name}"))?),
                    Some("crate") => KindRules::Crate(read("crate")?.try_into().with_context(|| format!("c.things.{name}"))?),
                    Some(other) => bail!("c.things.{name}.behaviour = \"{other}\": no such behaviour"),
                };
                Ok(Kind {
                    name: name.clone(),
                    model: def.model.clone(),
                    hp: def.hp.unwrap_or(1),
                    width: common.width.unwrap_or(cfg.physics.body_width).max(1.0),
                    height: common.height.unwrap_or(size.z as f32).max(1.0),
                    size,
                    // Names its behaviour does not play are poses the game
                    // sets (`thing.state.pose`).
                    frames: def.frames.clone(),
                    fps: def.fps,
                    swing: def.swing.clone(),
                    spin: def.spin,
                    facing: common.facing,
                    reach: def.reach,
                    rules,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { kinds })
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.kinds.iter().position(|k| k.name == name)
    }

    /// Every kind with the numbers in effect, a line each as `conf.lua`
    /// would write it (`voxcade kinds`): nothing hidden.
    pub fn numbers(&self) -> Vec<String> {
        use crate::config::{conf_line, lua_list};
        let text = |s: &str| format!("{s:?}");
        self.kinds
            .iter()
            .filter(|k| k.name != crate::grid::PIECE)
            .map(|k| {
                let behaviour = behaviour_name(&k.rules);
                let mut keys = vec![("behaviour", if behaviour.is_empty() { "nil".into() } else { text(behaviour) })];
                if let Some(m) = &k.model {
                    keys.push(("model", text(m)));
                }
                keys.extend([("hp", k.hp.to_string()), ("width", k.width.to_string()), ("height", k.height.to_string()), ("fps", k.fps.to_string())]);
                if let Some(r) = k.reach {
                    keys.push(("reach", r.to_string()));
                }
                if let Some(f) = k.facing {
                    keys.push(("facing", lua_list(&f)));
                }
                match &k.rules {
                    KindRules::Script | KindRules::Pickup => {}
                    KindRules::Hero(r) => keys.extend([
                        ("speed", r.speed.to_string()), ("accel", r.accel.to_string()), ("air_accel", r.air_accel.to_string()), ("jump", r.jump.to_string()),
                        ("jump_cut", r.jump_cut.to_string()), ("coyote", r.coyote.to_string()), ("buffer", r.buffer.to_string()), ("climb", r.climb.to_string()),
                        ("invulnerable", r.invulnerable.to_string()), ("knockback", lua_list(&r.knockback)), ("fall_gravity", r.fall_gravity.to_string()),
                        ("hang", r.hang.to_string()), ("hang_speed", r.hang_speed.to_string()), ("corner", r.corner.to_string()),
                        ("look_ahead", r.look_ahead.to_string()), ("look_ahead_speed", r.look_ahead_speed.to_string()), ("grab", r.grab.to_string()),
                        ("grab_ahead", r.grab_ahead.to_string()), ("hang_below", r.hang_below.to_string()),
                        ("pull", r.pull.to_string()), ("face_ladder", r.face_ladder.to_string()), ("blink", r.blink.to_string()),
                        ("air_jumps", r.air_jumps.to_string()), ("air_jump", r.air_jump.to_string()),
                    ]),
                    KindRules::Walker(r) => keys.extend([("speed", r.speed.to_string()), ("ledges", r.ledges.to_string())]),
                    KindRules::Hopper(r) => keys.extend([("every", r.every.to_string()), ("hop", lua_list(&r.hop))]),
                    KindRules::Flyer(r) => keys.extend([("range", r.range.to_string()), ("period", r.period.to_string()), ("face_hero", r.face_hero.to_string())]),
                    KindRules::Platform(r) => keys.extend([("travel", lua_list(&r.travel)), ("speed", r.speed.to_string()), ("pause", r.pause.to_string()), ("crushes", r.crushes.to_string())]),
                    KindRules::Prop(r) => keys.push(("stands", r.stands.to_string())),
                    KindRules::Crate(r) => keys.push(("push", r.push.to_string())),
                }
                conf_line("things", &k.name, &keys)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kit's kinds, as a game's things.
    fn kit() -> GameConfig {
        let table: toml::Table = toml::from_str(KIT).unwrap();
        let mut cfg = GameConfig::default();
        for (name, t) in table["things"].as_table().unwrap() {
            cfg.things.insert(name.clone(), t.as_table().unwrap().clone());
        }
        cfg.kit = Some("platformer".into());
        cfg
    }

    /// A figure of another height than the one its behaviour was tuned on
    /// gets the lengths and speeds it leaves unset scaled to its height;
    /// what it sets, and the counts and times, stay; `voxcade kinds` prints
    /// them all.
    #[test]
    fn unset_lengths_and_speeds_follow_the_height() {
        let mut cfg = GameConfig { kit: Some("platformer".into()), ..GameConfig::default() };
        cfg.things.insert("mouse".into(), toml::from_str("behaviour = \"hero\"\nheight = 10\njump = 9").unwrap());
        cfg.things.insert("fox".into(), toml::from_str("behaviour = \"hero\"\nheight = 20").unwrap());
        cfg.things.insert("bug".into(), toml::from_str("behaviour = \"walker\"\nheight = 8").unwrap());
        cfg.things.insert("blob".into(), toml::from_str("behaviour = \"walker\"").unwrap());
        let r = Rules::from_game(&cfg, |_| None).unwrap();
        let hero = |name: &str| match &r.kinds[r.find(name).unwrap()].rules {
            KindRules::Hero(h) => h.clone(),
            _ => unreachable!(),
        };
        let (mouse, fox) = (hero("mouse"), hero("fox"));
        assert_eq!(fox, HeroRules::default(), "the fox's height: as tuned");
        assert_eq!((mouse.speed, mouse.knockback, mouse.corner, mouse.jump), (75.0, [50.0, 50.0], 2, 9.0), "half as tall, its own jump");
        assert_eq!((mouse.coyote, mouse.invulnerable, mouse.jump_cut), (fox.coyote, fox.invulnerable, fox.jump_cut), "counts and times stay");
        let walker = |name: &str| match &r.kinds[r.find(name).unwrap()].rules {
            KindRules::Walker(w) => w.speed,
            _ => unreachable!(),
        };
        assert_eq!((walker("bug"), walker("blob")), (30.0, 60.0), "no height known: as tuned");
        assert!(r.numbers().iter().any(|l| l.starts_with("c.things.mouse = { behaviour = \"hero\"") && l.contains("speed = 75,")), "{:?}", r.numbers());
    }

    /// A kind that names only its behaviour is the kit's of that behaviour:
    /// the numbers in `kit.toml` are the behaviours' defaults.
    #[test]
    fn the_kits_kinds_are_the_behaviours_defaults() {
        let mut cfg = kit();
        for b in super::super::BEHAVIOURS {
            cfg.things.insert(format!("bare_{b}"), toml::from_str(&format!("behaviour = \"{b}\"")).unwrap());
        }
        let r = Rules::from_game(&cfg, |_| None).unwrap();
        for b in super::super::BEHAVIOURS {
            let (bare, kit) = (&r.kinds[r.find(&format!("bare_{b}")).unwrap()], &r.kinds[r.find(b).unwrap()]);
            assert_eq!(format!("{:?}", bare.rules), format!("{:?}", kit.rules), "a bare {b} is not the kit's");
        }
    }

    /// A misspelt number or one of the wrong kind says which kind it is in.
    #[test]
    fn mistakes_in_kinds_are_named() {
        let mut cfg = kit();
        cfg.things.insert("fox".into(), toml::from_str("behaviour = \"hero\"\njumpp = 3").unwrap());
        let e = format!("{:#}", Rules::from_game(&cfg, |_| None).unwrap_err());
        assert!(e.contains("c.things.fox") && e.contains("jumpp"), "{e}");
        let mut cfg = kit();
        cfg.things.insert("gem".into(), toml::from_str("behaviour = \"pickup\"\nmodel = \"gem.vox\"").unwrap());
        let e = format!("{:#}", Rules::from_game(&cfg, |_| None).unwrap_err());
        assert!(e.contains("no such model"), "{e}");
    }
}
