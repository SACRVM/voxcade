//! Things: what acts in a game's world besides its voxels (enemies,
//! hostages, gunships, pickups, huts ...). One list, in the order they came
//! into the world; each thing has an id that stays with it, its kind (which
//! of the game's definitions it is), the parts the engine knows (a body to
//! stand and move with, health, a white flash after a hit) and its brain:
//! the game's own state for it, a type the game chooses (the shooter's
//! `Brain`). A game runs its behaviours as systems, each going through the
//! things it moves in this order, so every run goes the same.
//!
//! Plain data throughout: a copy of the list is a copy of every thing
//! (`Session::snapshot`). New things go to the end, removing one keeps the
//! order of the others, so the list stays sorted by id.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{Body, Motion};
use crate::config::{GameConfig, resolve_like};

/// A kind of thing as the config defines it (`things.NAME`), with what it
/// is like filled in: its behaviour (one of the game's; none: the game's
/// script moves it), the parts the engine knows (its model's file, the hits
/// it takes, the numbers the game's code reads, its families) and the
/// rest, for its behaviour to read.
#[derive(Clone, Debug)]
pub struct Def {
    pub name: String,
    pub behaviour: Option<String>,
    pub model: Option<String>,
    pub hp: Option<i32>,
    /// What the game's code reads of its things (`thing.props`), by name:
    /// fixed, never a state (`c.things.gem.props = { worth = 100 }`).
    pub props: toml::Table,
    /// The families it belongs to (`level:things("enemies")`).
    pub families: Vec<String>,
    /// What shows and sounds when one of its things goes (dies, or is
    /// taken away): a model played once, or the thing bursting into its
    /// voxels (`BURST`); a sound event.
    pub effect: Option<Effect>,
    pub sound: Option<String>,
    /// Touched by what stands this close to it on the ground, not by its
    /// box (a pickup, a helipad).
    pub reach: Option<f32>,
    /// Whose side it is on (`team = "enemies"`): rounds and blows hurt
    /// those of other teams (and those of none), not their own.
    pub team: Option<String>,
    /// What does not hurt it: `round`, `melee`, `blast` (a hostage the
    /// rounds fly past).
    pub immune: Vec<String>,
    /// Drawn facing exactly its way, off the grid (`turns = "free"`), not
    /// in quarter turns (`"quarter"`, the default): the game's choice.
    pub free_turn: bool,
    /// How fast its model turns round its upright axis as it is drawn,
    /// degrees a second (`spin`; 0: not at all): a gem, a coin. Only the
    /// picture turns.
    pub spin: f32,
    /// Stays where it is put (`fixed = true`): no gravity, no landing on
    /// voxels, whatever the script asks of it (a sign, a switch, a thing
    /// hung in the air). Only for kinds the script moves.
    pub fixed: bool,
    /// Its model's frames for what it does and its poses, by name
    /// (`frames`), and how many a second where a set does not say (`fps`).
    pub frames: BTreeMap<String, Frames>,
    pub fps: f32,
    /// A figure of parts: the parts that swing as it walks, by name, and
    /// how far, degrees forward and back (`session::swinging`; less than
    /// 0: back first).
    pub swing: BTreeMap<String, f32>,
    pub params: toml::Table,
}

/// How many frames a second a set goes where neither it nor its kind says.
pub const FPS: f32 = 10.0;

/// A set of a model's frames, played for one thing a figure does or one
/// pose: `first` to `last`, `fps` a second or a frame every `stride`
/// voxels walked (the steps match the speed), round and round or once
/// (its last frame held then: a jump, a pull).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frames {
    pub first: usize,
    pub last: usize,
    pub fps: f32,
    pub stride: Option<f32>,
    pub looped: bool,
}

impl Frames {
    /// Always the one frame.
    pub fn held(frame: usize) -> Self {
        Self { first: frame, last: frame, fps: FPS, stride: None, looped: true }
    }

    /// The frame shown `ticks` after it began, `walked` voxels walked since.
    pub fn at(&self, ticks: u64, walked: f32) -> usize {
        let step = match self.stride {
            Some(stride) => (walked / stride) as usize,
            None => (ticks as f32 * self.fps / 60.0) as usize,
        };
        if self.looped { self.first + step % (self.last - self.first + 1) } else { self.first + step.min(self.last - self.first) }
    }
}

/// A set of frames as `frames.NAME` writes it in full.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FramesTable {
    first: usize,
    last: usize,
    fps: Option<f32>,
    stride: Option<f32>,
    #[serde(rename = "loop", default = "round")]
    looped: bool,
}

fn round() -> bool {
    true
}

/// The sets of frames a table names (`what`: where it is written, as
/// `c.things.hero.frames`): each `{ first, last }` at `fps`, round and
/// round, or `{ first = 9, last = 11, fps = 12, stride = 6, loop = false }`.
pub fn frame_sets(what: &str, sets: toml::Table, fps: f32) -> Result<BTreeMap<String, Frames>> {
    sets.into_iter()
        .map(|(name, v)| {
            let what = format!("{what}.{name}");
            let full = "the first and the last frame ({ 2, 5 }), or { first = 2, last = 5, fps = 8, stride = 6, loop = false }";
            let (first, last, own, stride, looped) = match v {
                toml::Value::Array(pair) => match pair.as_slice() {
                    [a, b] => match (a.as_integer(), b.as_integer()) {
                        (Some(a), Some(b)) if a >= 0 && b >= 0 => (a as usize, b as usize, None, None, true),
                        _ => bail!("{what}: the first and the last frame, from 0 ({{ 2, 5 }})"),
                    },
                    _ => bail!("{what}: {full}"),
                },
                toml::Value::Table(_) => {
                    let t: FramesTable = v.try_into().with_context(|| format!("{what} (first, last, fps, stride, loop)"))?;
                    (t.first, t.last, t.fps, t.stride, t.looped)
                }
                _ => bail!("{what}: {full}"),
            };
            let frames = Frames { first: first.min(last), last: first.max(last), fps: own.map_or(fps, |f| f.max(0.1)), stride: stride.map(|s| s.max(0.1)), looped };
            Ok((name, frames))
        })
        .collect()
}

/// The frame a figure shows for what its body does: its `sets` named
/// `idle` (standing), `walk` and `jump` (in the air), each from when it
/// began. A set it leaves out is the model's own layout (`count` frames,
/// `tall` voxels): frame 0 standing, 1 in the air, 1 onwards walking, a
/// frame every half its height walked (a stride grows with the legs); one
/// frame alone shows always.
pub fn figure_frame(sets: &BTreeMap<String, Frames>, body: &Body, count: usize, tall: f32) -> usize {
    if count < 2 {
        return 0;
    }
    let (motion, ticks) = body.motion();
    let (name, own) = match motion {
        Motion::Still => ("idle", Frames::held(0)),
        Motion::Walking => ("walk", Frames { first: 1, last: count - 1, fps: FPS, stride: Some((tall * 0.5).max(1.0)), looped: true }),
        Motion::Air => ("jump", Frames::held(1)),
    };
    sets.get(name).unwrap_or(&own).at(ticks as u64, body.walked).min(count - 1)
}

/// A model played once where one of a kind's things goes, its frames
/// `fps` a second (`effect = "poof.vvx"`, or `{ model = "poof.vvx", fps =
/// 20 }`).
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub model: String,
    pub fps: f32,
}

/// How fast an effect's frames go unless it says.
pub const EFFECT_FPS: f32 = 12.0;

/// The effect that is no model: the thing bursts into its own voxels,
/// which fall, lie a moment and are gone (`effect = "burst"`; the
/// simulation's, as a shooter's men burst).
pub const BURST: &str = "burst";

impl Effect {
    /// Whether the thing bursts into its voxels (`BURST`) rather than
    /// showing a model.
    pub fn bursts(&self) -> bool {
        self.model == BURST
    }
}

/// The keys every kind has besides `like`, `behaviour`, `model` and `hp`
/// (`Def`): a behaviour's own numbers are the rest.
pub const COMMON: [&str; 12] = ["props", "family", "effect", "sound", "reach", "team", "immune", "turns", "spin", "frames", "fps", "swing"];

/// The player's team (a kind's `team` names others).
pub const PLAYER_TEAM: &str = "player";

/// What can hurt a thing, for `immune`.
pub const HARMS: [&str; 3] = ["round", "melee", "blast"];

/// Behaviours that were and are no more: what to write instead.
const RETIRED: [(&str, &str); 3] = [
    ("goal", "a goal is a thing your code answers: a kind of its own (the platformer's behaviour = \"prop\", stands = true; or none, touched within its reach), and on_touch: world:win(), world:go_to"),
    ("checkpoint", "a checkpoint is a thing your code answers: a kind without a behaviour (or the platformer's prop), and player.state.checkpoint = b.name in on_touch or on_tick"),
    ("hazard", "spikes are a thing your code answers: a kind with a family (the platformer's behaviour = \"prop\"), and in on_tick: if #player:touching(\"hazards\") > 0 then player:hurt(1) end"),
];

/// The kinds of things a game defines, in the order of their names: a
/// thing's and a marker's `kind` is the place here. `behaviours` are the
/// game's; any other is a mistake in `conf.lua`.
pub fn defs(cfg: &GameConfig, behaviours: &[&str]) -> Result<Vec<Def>> {
    resolve_like("things", &cfg.things)?
        .into_iter()
        .map(|(name, mut params)| {
            let what = |key: &str| format!("c.things.{name}.{key}");
            let behaviour = params.remove("behaviour").map(|b| b.as_str().map(str::to_string).with_context(|| what("behaviour"))).transpose()?;
            if let Some(b) = &behaviour
                && !behaviours.contains(&b.as_str())
                && let Some((_, instead)) = RETIRED.iter().find(|(r, _)| r == b)
            {
                bail!("c.things.{name}.behaviour = \"{b}\" is no more: {instead} (docs/game-model.md)");
            }
            if let Some(b) = &behaviour
                && !behaviours.contains(&b.as_str())
            {
                if behaviours.is_empty() {
                    bail!("c.things.{name}.behaviour = \"{b}\": no behaviours without a kit (c.kit)");
                }
                bail!("c.things.{name}.behaviour = \"{b}\": no such behaviour (behaviours: {})", behaviours.join(", "));
            }
            let model = params.remove("model").map(|m| m.as_str().map(str::to_string).with_context(|| what("model"))).transpose()?;
            let hp = params.remove("hp").map(|h| h.as_integer().map(|h| h as i32).with_context(|| what("hp"))).transpose()?;
            let props = match params.remove("props") {
                None => toml::Table::new(),
                Some(toml::Value::Table(t)) => t,
                Some(_) => bail!("{}: a table of what the game's code reads (props = {{ worth = 100 }})", what("props")),
            };
            let families = match params.remove("family") {
                None => Vec::new(),
                Some(toml::Value::String(f)) => vec![f],
                Some(toml::Value::Array(list)) => list.iter().map(|f| f.as_str().map(str::to_string)).collect::<Option<Vec<_>>>().with_context(|| format!("{}: names of families", what("family")))?,
                Some(_) => bail!("{}: a family's name or a list of them (family = {{ \"enemies\" }})", what("family")),
            };
            let text = |key: &str, params: &mut toml::Table| params.remove(key).map(|v| v.as_str().map(str::to_string).with_context(|| what(key))).transpose();
            let effect_is = || format!("{}: a model, {{ model = \"poof.vvx\", fps = 20 }} (its frames a second, {EFFECT_FPS} without), or \"{BURST}\" (it bursts into its voxels)", what("effect"));
            let effect = match params.remove("effect") {
                None => None,
                Some(toml::Value::String(model)) => Some(Effect { model, fps: EFFECT_FPS }),
                Some(toml::Value::Table(mut t)) => {
                    let model = t.remove("model").and_then(|m| m.as_str().map(str::to_string));
                    let fps = t.remove("fps").map(|f| f.as_float().or_else(|| f.as_integer().map(|i| i as f64)));
                    match (model, fps) {
                        (Some(model), None) if t.is_empty() => Some(Effect { model, fps: EFFECT_FPS }),
                        (Some(model), Some(Some(fps))) if t.is_empty() => Some(Effect { model, fps: (fps as f32).max(0.1) }),
                        _ => bail!(effect_is()),
                    }
                }
                Some(_) => bail!(effect_is()),
            };
            let sound = text("sound", &mut params)?;
            let reach = params.remove("reach").map(|r| r.as_float().or_else(|| r.as_integer().map(|i| i as f64)).map(|r| r as f32).with_context(|| what("reach"))).transpose()?;
            let team = text("team", &mut params)?;
            let immune = match params.remove("immune") {
                None => Vec::new(),
                Some(toml::Value::Array(list)) => list.iter().map(|f| f.as_str().map(str::to_string)).collect::<Option<Vec<_>>>().with_context(|| format!("{}: what does not hurt it ({})", what("immune"), HARMS.join(", ")))?,
                Some(_) => bail!("{}: a list of what does not hurt it (immune = {{ \"round\" }}: {})", what("immune"), HARMS.join(", ")),
            };
            if let Some(harm) = immune.iter().find(|h| !HARMS.contains(&h.as_str())) {
                bail!("{}: \"{harm}\" is none of {}", what("immune"), HARMS.join(", "));
            }
            let free_turn = match text("turns", &mut params)?.as_deref() {
                None | Some("quarter") => false,
                Some("free") => true,
                Some(other) => bail!("{}: \"{other}\" is neither \"quarter\" (on the grid, the default) nor \"free\" (facing exactly its way)", what("turns")),
            };
            let spin = match params.remove("spin") {
                None => 0.0,
                Some(v) => v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).with_context(|| format!("{}: how fast it turns round, degrees a second", what("spin")))? as f32,
            };
            let fixed = match params.remove("fixed") {
                None => false,
                Some(toml::Value::Boolean(b)) => b,
                Some(_) => bail!("{}: true or false (a thing that stays where it is put)", what("fixed")),
            };
            if fixed && behaviour.is_some() {
                bail!("{}: a kind with a behaviour is moved by its ready-made rules; fixed is for kinds the script moves (no behaviour)", what("fixed"));
            }
            let fps = match params.remove("fps") {
                None => FPS,
                Some(f) => f.as_float().or_else(|| f.as_integer().map(|i| i as f64)).with_context(|| what("fps"))?.max(0.1) as f32,
            };
            let frames = match params.remove("frames") {
                None => BTreeMap::new(),
                Some(toml::Value::Table(sets)) => frame_sets(&what("frames"), sets, fps)?,
                Some(_) => bail!("{}: its model's frames by name (frames = {{ walk = {{ 1, 4 }} }})", what("frames")),
            };
            let swing = match params.remove("swing") {
                None => BTreeMap::new(),
                Some(toml::Value::Table(parts)) => parts
                    .into_iter()
                    .map(|(part, v)| match v.as_float().or_else(|| v.as_integer().map(|i| i as f64)) {
                        Some(degrees) => Ok((part, degrees as f32)),
                        None => bail!("{}.{part}: how far it swings, degrees", what("swing")),
                    })
                    .collect::<Result<_>>()?,
                Some(_) => bail!("{}: parts by name and how far they swing (swing = {{ leg_left = 30, leg_right = -30 }})", what("swing")),
            };
            Ok(Def { name, behaviour, model, hp, props, families, effect, sound, reach, team, immune, free_turn, spin, fixed, frames, fps, swing, params })
        })
        .collect()
}

/// The names of the kinds of things a game defines, in order (what its
/// markers may name).
pub fn names(cfg: &GameConfig) -> Vec<&str> {
    cfg.things.keys().map(String::as_str).collect()
}

/// A thing's name in its world: never given to another thing in the same
/// run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(u32);

impl Id {
    /// As a number (for scripts and saved state), and back.
    pub fn raw(self) -> u32 {
        self.0
    }

    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// What a game's script set a thing to do this tick: the way to walk (on
/// the ground, length 1 or 0) and how fast.
#[derive(Clone, Copy, Debug, Default)]
pub struct Walk {
    pub wish: glam::Vec3,
    pub speed: f32,
}

#[derive(Clone)]
pub struct Thing<B> {
    pub id: Id,
    /// Which of the game's kinds it is.
    pub kind: usize,
    /// Where it is and how it moves (only walkers advance it).
    pub body: Body,
    /// Hits it takes yet (0 for things that take none).
    pub hp: i32,
    /// Ticks left of the white flash after a hit.
    pub flash: u32,
    /// What it shows instead of its rules' own animation
    /// (`thing.state.pose`), and from which tick.
    pub pose: Option<(String, u64)>,
    /// Its ready-made rules switched off (`thing:enable`): physics and the
    /// script move it as one of its own, this its walk.
    pub off: Option<Walk>,
    pub brain: B,
}

#[derive(Clone)]
pub struct Things<B> {
    list: Vec<Thing<B>>,
    next: u32,
}

impl<B> Default for Things<B> {
    fn default() -> Self {
        Self { list: Vec::new(), next: 0 }
    }
}

impl<B> Things<B> {
    /// A new thing at the end of the list.
    pub fn spawn(&mut self, kind: usize, body: Body, hp: i32, brain: B) -> Id {
        let id = Id(self.next);
        self.next += 1;
        self.list.push(Thing { id, kind, body, hp, flash: 0, pose: None, off: None, brain });
        id
    }

    fn index(&self, id: Id) -> Option<usize> {
        self.list.binary_search_by_key(&id, |t| t.id).ok()
    }

    pub fn get(&self, id: Id) -> Option<&Thing<B>> {
        self.index(id).map(|i| &self.list[i])
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Thing<B>> {
        self.index(id).map(|i| &mut self.list[i])
    }

    /// Takes a thing out; the others keep their order.
    pub fn remove(&mut self, id: Id) -> Option<Thing<B>> {
        self.index(id).map(|i| self.list.remove(i))
    }

    pub fn retain(&mut self, keep: impl FnMut(&Thing<B>) -> bool) {
        self.list.retain(keep);
    }

    /// Empties the list; ids go on counting.
    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Thing<B>> {
        self.list.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Thing<B>> {
        self.list.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The ids of the things `pick` chooses, in order.
    pub fn ids(&self, mut pick: impl FnMut(&Thing<B>) -> bool) -> Vec<Id> {
        self.list.iter().filter(|t| pick(t)).map(|t| t.id).collect()
    }

    /// The `n`th of the things `pick` chooses, counting from 0 in order.
    pub fn nth(&self, n: usize, mut pick: impl FnMut(&Thing<B>) -> bool) -> Option<Id> {
        self.list.iter().filter(|t| pick(t)).nth(n).map(|t| t.id)
    }
}

/// A pickup's `item` (`c.things.NAME.item`) is one of `c.items`.
pub fn check_item(cfg: &crate::config::GameConfig, kind: &str, item: Option<&str>) -> anyhow::Result<()> {
    let Some(item) = item else { return Ok(()) };
    if cfg.items.contains_key(item) {
        return Ok(());
    }
    match cfg.item_names().as_slice() {
        [] => anyhow::bail!("c.things.{kind}.item = \"{item}\": there are no items (c.items names them)"),
        names => anyhow::bail!("c.things.{kind}.item = \"{item}\": no such item (c.items: {})", names.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    /// A kind like another has the other's keys and its own on top; a
    /// circle, an unknown one and a missing behaviour are mistakes.
    #[test]
    fn kinds_build_on_one_another() {
        let defs_of = |text: &str| defs(&toml::from_str(text).unwrap(), &["walker", "door"]);
        let found = defs_of("[things.b]\nlike = \"a\"\nhp = 4\n[things.a]\nbehaviour = \"walker\"\nmodel = \"a.vox\"\nhp = 2\nspeed = 3\n").unwrap();
        assert_eq!(found.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["a", "b"], "kinds go by name");
        assert_eq!((found[1].behaviour.as_deref(), found[1].model.as_deref(), found[1].hp), (Some("walker"), Some("a.vox"), Some(4)));
        assert_eq!(found[1].params.get("speed").and_then(|v| v.as_integer()), Some(3));
        assert!(!found[1].params.contains_key("like"));
        assert!(defs_of("[things.a]\nlike = \"b\"\n[things.b]\nlike = \"a\"\n").unwrap_err().to_string().contains("circle"));
        assert!(defs_of("[things.a]\nlike = \"c\"\n").unwrap_err().to_string().contains("no c.things.c"));
        assert_eq!(defs_of("[things.a]\nhp = 2\n").unwrap()[0].behaviour, None, "without one the game's script moves it");
        assert!(defs_of("[things.a]\nbehaviour = \"flyer\"\n").unwrap_err().to_string().contains("no such behaviour"));
    }

    /// Things stay in the order they came; taking one out keeps the
    /// others' order and ids, and an id is never given twice.
    #[test]
    fn things_keep_their_order_and_their_ids() {
        let mut things = Things::default();
        let body = || Body::new(Vec3::ZERO, 1.0);
        let ids: Vec<Id> = (0..5).map(|n| things.spawn(n, body(), 1, n % 2 == 0)).collect();
        assert_eq!(things.remove(ids[1]).map(|t| t.kind), Some(1));
        assert!(things.get(ids[1]).is_none());
        assert_eq!(things.iter().map(|t| t.kind).collect::<Vec<_>>(), [0, 2, 3, 4]);
        assert_eq!(things.nth(1, |t| t.brain), Some(ids[2]));
        assert_eq!(things.nth(3, |t| t.brain), None);
        let late = things.spawn(9, body(), 1, true);
        assert!(late > ids[4], "an id came twice");
        assert_eq!(things.get(ids[3]).map(|t| t.kind), Some(3));
        assert_eq!(things.ids(|t| t.brain), [ids[0], ids[2], ids[4], late]);
        things.clear();
        assert!(things.spawn(0, body(), 1, true) > late);
    }
}
