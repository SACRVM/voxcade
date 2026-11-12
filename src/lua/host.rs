//! What a Lua game's world is made of (`Host`): the simulation's core, the
//! things in one list, the score, how the run ends and what the camera
//! follows. Without a kit it is `Plain`: only things the script moves. A
//! kit (`Kit`, the shooter) brings its own things and rules; its kinds
//! without a behaviour are the script's, moved the same way.

use std::any::Any;
use std::sync::Arc;

use glam::{IVec2, Vec3};

use anyhow::Result;

use crate::config::GameConfig;
use crate::controls::Bindings;
use crate::game::{Game, ModelSpec, Outcome, Setup};
use crate::input::Input;
use crate::session::{Drawing, Models};
use crate::world::{Marker, World};
use crate::sim::things::{Id, Things, Walk};
use crate::sim::{Body, Sim, stand_on_ground};

/// A kind of thing as a Lua game has it: its name, model, height, the hits
/// it takes, and whether the script moves it (no behaviour of a kit's).
#[derive(Clone, Debug)]
pub struct Kind {
    pub name: String,
    pub model: Option<String>,
    pub height: f32,
    pub hp: i32,
    /// Whether its things have hit points (`thing.state.hp`): the kind
    /// declares `hp`, or the ready-made rules move it.
    pub has_hp: bool,
    pub scripted: bool,
    /// What the game's code reads of its things (`thing.props`).
    pub props: toml::Table,
    /// The families it belongs to.
    pub families: Vec<String>,
    /// What shows and sounds when one of its things goes.
    pub effect: Option<crate::sim::things::Effect>,
    pub sound: Option<&'static str>,
    /// Touched within this on the ground, not by its box.
    pub reach: Option<f32>,
    /// Whose side it is on (`team`).
    pub team: Option<String>,
    /// Drawn facing exactly its way (`turns = "free"`).
    pub free_turn: bool,
    /// How fast its model turns round as it is drawn, degrees a second.
    pub spin: f32,
    /// Stays where it is put: no gravity (`fixed`).
    pub fixed: bool,
    /// Its model's frames for what it does (idle, walk, jump: `frame:model`
    /// without a frame).
    pub frames: std::collections::BTreeMap<String, crate::sim::things::Frames>,
    /// A figure of parts: the parts that swing as it walks (degrees).
    pub swing: std::collections::BTreeMap<String, f32>,
}

/// A thing as the script sees it.
pub struct Seen<'a> {
    pub kind: usize,
    pub body: &'a Body,
    pub hp: i32,
    /// Ticks it still shows white after a hit.
    pub flash: u32,
    /// Half its width (and depth): its box for touches.
    pub half: f32,
    /// Touched by whatever stands this close to it on the ground (the
    /// shooter's goal and pickups), not by its box.
    pub reach: Option<f32>,
}

/// A thing as the script may change it: its hits always, its body and walk
/// only when it is the script's.
pub struct Touched<'a> {
    pub hp: &'a mut i32,
    pub moved: Option<(&'a mut Body, &'a mut Walk)>,
}

/// A value the ready-made rules keep in a thing's state (a prop's
/// `thing.state.pose`, a hostage's `thing.state.freed`).
#[derive(Clone, Debug, PartialEq)]
pub enum Kept {
    Bool(bool),
    Text(String),
}

/// The player's handle (`player`): no thing's id. The figure behind it is
/// the host's (`Host::player_seen`), whatever its id in a level.
pub const PLAYER: u32 = u32::MAX;

/// The handle of seat `seat`'s player (1: `player`), below `PLAYER`, no
/// thing's id either.
pub fn seat_handle(seat: usize) -> u32 {
    PLAYER - (seat.max(1) as u32 - 1)
}

/// The seat a handle is a player's of, if it is one.
pub fn handle_seat(id: u32) -> Option<usize> {
    (id > PLAYER - crate::config::MOST_PLAYERS).then(|| (PLAYER - id) as usize + 1)
}

/// A seat as the run has it (`docs/game-model.md`, "Several players"):
/// who sits there and the engine's keys of its player's state; for the
/// seats after the first also what they hold (seat 1's is the game's).
#[derive(Clone, Debug)]
pub struct SeatState {
    /// Someone sits there (seat 1 always), and someone did in this run:
    /// its state is kept from then on, and counts for the hash.
    pub taken: bool,
    pub been: bool,
    /// The engine's keys (`p.state.out`, `team`, `kind`).
    pub out: bool,
    pub team: Option<String>,
    pub kind: Option<String>,
    /// The score of a seat after the first (seat 1's is the host's), and
    /// its hit points while its figure does not stand (its figure's while
    /// it does).
    pub score: u32,
    pub hp: Option<i32>,
    /// Where its figure comes in, placed before it stood (`p:place`).
    pub coming: Option<(Vec3, Option<IVec2>)>,
    /// What it holds this tick and held the last, and how long each key
    /// (`p.input`); what its player plays with (`p.controls`).
    pub input: Input,
    pub before: Input,
    pub held_for: [u16; super::world::KEYS],
    pub controls: crate::controls::Controls,
    /// What the machines say for the next tick: what it holds, or nobody.
    pub next: Option<Input>,
    /// Taken (`"new"`, `"back"`) or left this tick: told first among the
    /// events.
    pub joined: Option<&'static str>,
    pub left: bool,
}

impl Default for SeatState {
    fn default() -> Self {
        Self {
            taken: false,
            been: false,
            out: false,
            team: None,
            kind: None,
            score: 0,
            hp: None,
            coming: None,
            input: Input::default(),
            before: Input::default(),
            held_for: [0; super::world::KEYS],
            controls: Default::default(),
            next: None,
            joined: None,
            left: false,
        }
    }
}

/// A figure of no kind of `c.things` (the shooter's player, `c.player`).
pub const NO_KIND: usize = usize::MAX;

pub trait Host: Any {
    fn sim(&self) -> &Sim;
    fn sim_mut(&mut self) -> &mut Sim;
    /// Every thing, in the order they came.
    fn ids(&self) -> Vec<Id>;
    fn get(&self, id: Id) -> Option<Seen<'_>>;
    fn get_mut(&mut self, id: Id) -> Option<Touched<'_>>;
    /// Places a thing of kind `kind` at (x, y): on the ground there unless
    /// `z` is given.
    fn spawn(&mut self, kind: usize, x: f32, y: f32, z: Option<f32>) -> Result<Id, String>;
    fn remove(&mut self, id: Id) -> bool;
    fn score(&self) -> u32;
    fn set_score(&mut self, score: u32);

    /// The player's numbers the engine keeps (`player.state`'s engine keys:
    /// the score, and lives and hit points where the ready-made rules keep
    /// them).
    fn player_keys(&self) -> &'static [&'static str] {
        &["score"]
    }

    /// One of them (None: not kept here).
    fn player_number(&self, key: &str) -> Option<i64> {
        (key == "score").then(|| self.score() as i64)
    }

    /// One of them that is no number (the shooter's weapon by its name).
    fn player_kept(&self, _key: &str) -> Option<Kept> {
        None
    }

    /// Sets one of them that is no number.
    fn set_player_kept(&mut self, key: &str, _value: Kept) -> Result<(), String> {
        Err(format!("{key} is not kept by the rules"))
    }

    /// Sets one of them: whether it is kept here.
    fn set_player_number(&mut self, key: &str, value: i64) -> bool {
        if key == "score" {
            self.set_score(value.max(0) as u32);
        }
        key == "score"
    }

    /// The player's figure where it is one of the things (the platformer's
    /// hero, a thing of `c.player.kind`).
    fn player_id(&self) -> Option<Id> {
        None
    }

    /// The player's figure: None where it has none (a game without
    /// `c.player.kind`, a level without a hero).
    fn player_seen(&self) -> Option<Seen<'_>> {
        self.player_id().and_then(|id| self.get(id))
    }

    /// The figure of a seat after the first, a thing of `c.player.kind`
    /// (only a game without ready-made rules has them yet).
    fn seat_figure(&self, _seat: usize) -> Option<Id> {
        None
    }

    /// A ready-made rules' figure for a seat after the first: of `kind`
    /// at `at` (facing, hit points), or taken away (None). Theirs that
    /// play one player say so.
    fn put_seat(&mut self, _seat: usize, _figure: Option<(usize, Vec3, Option<IVec2>, i32)>) -> Result<Option<Id>, String> {
        Err("the ready-made rules play one player so far".into())
    }

    /// A seat's figure of the ready-made rules stood at `at`, still and
    /// afresh (`p:place`).
    fn put_figure(&mut self, _id: Id, _at: Vec3) -> Result<(), String> {
        Err("the ready-made rules play one player so far".into())
    }

    /// The kind the ready-made rules' figures for the seats after the
    /// first are, unless a player's `state.kind` says.
    fn hero_kind(&self) -> Option<usize> {
        None
    }

    /// Whether the camera follows the players (not a thing it was told to
    /// follow, nor a point).
    fn follows_players(&self) -> bool {
        false
    }

    /// One of its figures held back to `at` (`c.camera.hold`): where it
    /// came from this tick, its speed that way gone.
    fn hold_back(&mut self, _id: Id, _at: Vec3) {}

    /// A number the ready-made rules keep for one of their figures that is
    /// a player's (`p.state.invulnerable`), and setting it.
    fn figure_number(&self, _id: Id, _key: &str) -> Option<i64> {
        None
    }

    fn set_figure_number(&mut self, _id: Id, _key: &str, _value: i64) -> bool {
        false
    }

    fn end(&mut self, outcome: Outcome);
    /// The thing the camera follows from now on.
    fn follow(&mut self, id: Id) -> Result<(), String>;
    /// The camera rests over a point from now on, following nothing.
    fn look_at(&mut self, x: f32, y: f32) -> Result<(), String>;
    /// Where its rules stand the player in a level of this world and these
    /// markers, if they place one.
    fn start_at(&self, _world: &World, _markers: &[Marker]) -> Option<Vec3> {
        None
    }

    /// The script hurts one of its rules' own (`thing:hurt`): taken as their
    /// hits are (the shooter's soldier falls, the platformer's hero loses a
    /// heart), their events reported as theirs. A plain host has none.
    fn hurt(&mut self, _who: crate::sim::Who, _damage: i32, _by: Option<crate::sim::Who>, _weapon: Option<String>) -> Result<(), String> {
        Err("it takes no hits".into())
    }

    /// Takes the rounds in flight away, a team's only if given
    /// (`level:clear_rounds`). Rules that fire none have none to take.
    fn clear_rounds(&mut self, _team: Option<&str>) -> Result<(), String> {
        Ok(())
    }

    /// The keys its rules keep in thing `id`'s state besides `hp`: a
    /// behaviour's switches, a prop's `pose`.
    fn thing_keys(&self, _id: Id) -> &'static [&'static str] {
        &[]
    }

    /// One of them.
    fn thing_value(&self, _id: Id, _key: &str) -> Option<Kept> {
        None
    }

    /// Sets one of them (a mistake: why not).
    fn set_thing_value(&mut self, _id: Id, key: &str, _value: Kept) -> Result<(), String> {
        Err(format!("{key} is not kept by its rules"))
    }

    /// Whether two of its bodies touch, where its rules have their own
    /// shape for it (the shooter's pickups, touched by the player's hips
    /// within their reach); None: the engine's (`touch`).
    fn touches(&self, _a: &Touching, _b: &Touching) -> Option<bool> {
        None
    }

    /// Stands the player's figure at `at` (`player:place`, a respawn), as
    /// its rules stand it: still, on its feet. A plain host's player is a
    /// thing the script moves (`thing:place`).
    fn put_player(&mut self, _at: Vec3) -> Result<(), String> {
        Err("the player moves by its ready-made rules".into())
    }

    /// The game's code attacks with a thing (`thing:attack`): the weapon by
    /// its name, at a point (and the thing there) if given. It starts at
    /// the tick's end (`start_attacks`); rules without weapons refuse.
    fn attack(&mut self, _who: crate::sim::Who, _weapon: &str, _at: Option<(Vec3, Option<Id>)>) -> Result<(), String> {
        Err("no weapons without ready-made rules that have them (c.kit = \"shooter\"); thing:hurt hurts a thing".into())
    }

    /// The attacks the code asked for this tick start (the tick's end).
    fn start_attacks(&mut self) {}

    /// Keys of `player.state` its rules kept before and keep elsewhere now,
    /// with the new place (the shooter's `grenades`: the inventory's).
    fn moved_player_keys(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// The items of the player's inventory its rules spend (the shooter's
    /// rounds and grenades) and how many it holds: kept in step with
    /// `player.state.inventory` around the rules (`set_stock`).
    fn stock(&self) -> Vec<(String, u32)> {
        Vec::new()
    }

    fn set_stock(&mut self, _item: &str, _count: u32) {}

    /// A seat's part of the inventory the rules spend (`stock` of seat 1),
    /// and setting it: a seat's player's in a game of several.
    fn seat_stock(&self, _seat: usize) -> Vec<(String, u32)> {
        Vec::new()
    }

    fn set_seat_stock(&mut self, _seat: usize, _item: &str, _count: u32) {}

    /// A key the rules keep for a seat's player that is no number (the
    /// shooter's `weapon`; seat 1's `player_kept`), and setting it.
    fn seat_kept(&self, _seat: usize, _key: &str) -> Option<Kept> {
        None
    }

    fn set_seat_kept(&mut self, _seat: usize, key: &str, _value: Kept) -> Result<(), String> {
        Err(format!("{key} is not kept by the rules"))
    }

    /// Turns a thing to `way` (`thing:face`): the script's things, and the
    /// player's figure of ready-made rules (it turns again as it moves).
    fn face(&mut self, id: Id, way: Vec3) -> Result<(), String> {
        match self.get_mut(id).and_then(|t| t.moved) {
            Some((body, _)) => {
                body.face(way);
                Ok(())
            }
            None => Err("it turns by its ready-made rules".into()),
        }
    }

    /// Sets a thing moving at `vel` (voxels a second), standing or not
    /// (`thing:launch`): the script's things, those whose rules are off,
    /// and the figures of rules that let it (the hero, a soldier, the
    /// shooter's player). Its rules steer on from there.
    fn launch(&mut self, id: Id, vel: Vec3) -> Result<(), String> {
        match self.get_mut(id).and_then(|t| t.moved) {
            Some((body, walk)) => {
                body.vel = vel;
                body.on_ground &= vel.z <= 0.0;
                *walk = Walk::default();
                Ok(())
            }
            None => Err("it moves by its ready-made rules".into()),
        }
    }

    /// Switches a thing's ready-made behaviour off or on (`thing:enable`):
    /// off, physics and the script move it as one of its own.
    fn enable(&mut self, _id: Id, _behaviour: &str, _on: bool) -> Result<(), String> {
        Err("it has no ready-made behaviour (they come with c.kit)".into())
    }

    /// Sends a thing up at `speed` (voxels a second), standing or not
    /// (`thing:bounce`): the script's things, and the figures of rules that
    /// let it (the platformer's hero).
    fn bounce(&mut self, id: Id, speed: f32) -> Result<(), String> {
        match self.get_mut(id).and_then(|t| t.moved) {
            Some((body, _)) => {
                body.vel.z = speed;
                body.on_ground = false;
                Ok(())
            }
            None => Err("it moves by its ready-made rules".into()),
        }
    }
    fn box_clone(&self) -> Box<dyn Host>;
}

/// A kit a Lua game may play with (`c.kit`): a game of its own whose
/// behaviours run by themselves, the script's `on_tick` after them; what
/// they do to things it reports (`Sim::events`) and the script hears.
pub trait Kit: Game + Host + Clone {
    /// Whether `c.kit = name` asks for it.
    fn named(name: &str) -> bool;

    /// The names `c.kit` may give it (for mistakes to list).
    fn names() -> Vec<&'static str>;

    /// One tick of its rules.
    fn rules(&mut self, input: &Input);
    /// What the seats after the first hold this tick (seat 2 first; None:
    /// nobody), before `rules`: for a kit that plays several.
    fn set_seat_inputs(&mut self, _inputs: &[Option<Input>]) {}
    /// Each seat's team this tick, seat 1 first (`p.state.team`; none: the
    /// players' team), before `rules`: whom its rounds and blasts hurt.
    fn set_seat_teams(&mut self, _teams: &[Option<&'static str>]) {}
    /// Whether its rules play several players (`c.players`).
    fn plays_several(_cfg: &GameConfig) -> bool {
        false
    }

    /// A tick while the level is held (`level.state.held`): time goes on,
    /// nothing of its rules moves; whether a tick ran (not a hit stop).
    fn held(&mut self) -> bool;

    /// Moves the things the script moves, after its `on_tick`; those of a
    /// kind `fixed` says stay where they are.
    fn move_scripted(&mut self, fixed: &dyn Fn(usize) -> bool);

    /// The script built the world (`c.world.size`, in `on_level_start`) after the
    /// kit placed its own: they are put on what was built (the shooter's
    /// player; the platformer's hero comes from the script, on it already).
    fn built(&mut self) {}

    /// Another level is entered (`world:go_to`, `world:restart`): its world,
    /// the things its markers place and its places; the kit's run kept
    /// (score, lives, weapon); the player with its feet at `at`, facing its
    /// way if given, or else where the rules start it (`Host::start_at`),
    /// and back on its feet if it would arrive dead (a death in the tick of
    /// a win does not carry over).
    fn enter_level(&mut self, world: World, markers: &[Marker], places: &[crate::levels::Place], at: Option<(Vec3, Option<IVec2>)>);

    /// Their own part of the run a save keeps (the shooter's weapon and
    /// ammo, the platformer's hearts), by name.
    fn run(&self) -> toml::Table {
        toml::Table::new()
    }

    /// That part back from a save; a name it no longer knows is left out.
    fn set_run(&mut self, _run: &toml::Table) {}
}

/// The world a script works on (`world` in Lua): the host, and the kinds of
/// things and sound events by name.
pub struct Stage {
    pub host: Box<dyn Host>,
    pub kinds: Arc<Vec<Kind>>,
    pub sounds: Arc<Vec<&'static str>>,
    /// Things the script removed, gone at the tick's end (`thing:remove`).
    pub gone: Vec<u32>,
    /// The game's levels, the one being played (its place among them) and
    /// how often each was entered in this run.
    pub levels: Arc<crate::levels::Levels>,
    pub level: usize,
    pub visits: Vec<u32>,
    /// What the script asked to happen at the tick's end: the one ask that
    /// counts so far (`Ask::outranks`).
    pub ask: Option<Ask>,
    /// Where the player arrived in the level being played
    /// (`level.arrived`).
    pub arrived: crate::levels::Place,
    /// Whether the run has ended and the script was told
    /// (`on_world_end`): asks count no more.
    pub ended: bool,
    /// The save slots `world:save` asked for, saved at the tick's end (or
    /// after the `on_level_start` that asked).
    pub saving: Vec<u32>,
    /// What lies in each save slot (`world.saves`), from 1, and the saves.
    pub slots: Vec<Option<crate::saves::SlotInfo>>,
    pub slot_texts: std::collections::BTreeMap<u32, Arc<str>>,
    /// A replay's loads, read in turn instead of the slots.
    pub replayed: std::collections::VecDeque<Arc<str>>,
    /// The saves loaded in this run, in order (a replay carries them).
    pub loaded: Vec<Arc<str>>,
    /// The pairs of things touching, each once: the player first, else the
    /// one that came first (`on_touch` when a pair begins, `on_touch_end`
    /// when it ends). Made from the bodies, so not in the state hash.
    pub touching: std::collections::BTreeSet<(u32, u32)>,
    /// Things whose death was told and that have not come back (no
    /// touches, not listed or counted).
    pub dead: std::collections::BTreeSet<u32>,
    /// Things the ready-made rules took away this tick as they died: read
    /// as they were until the tick's end (`on_death` names them).
    pub graveyard: Vec<(u32, usize, Body, f32)>,
    /// Things put somewhere this tick (`thing:place`): what they touch
    /// there they touch without `on_touch`.
    pub placed: Vec<u32>,
    /// What the script did to things this tick (`thing:hurt`,
    /// `thing:take`): told in the next tick's events, before the rules'.
    pub later: Vec<crate::sim::Event>,
    /// Who brought the script's own things' hit points to 0 by
    /// `thing:hurt`, and with what: `on_death`'s `by` and `weapon`.
    pub killers: std::collections::BTreeMap<u32, (Option<u32>, Option<String>)>,
    /// What the player plays with (`controls`).
    pub controls: crate::controls::Controls,
    /// Every seat, seat 1 first (`c.players`; one in a game of one).
    pub seats: Vec<SeatState>,
    /// What the machine drawing has of the game (`Game::set_local`): its
    /// players in `on_draw`, their keys and language.
    pub local: crate::game::Local,
    /// Whether `on_tick` runs (a player's keys are only there then).
    pub ticking: bool,
    /// The game's actions (`p.input`).
    pub actions: Arc<Vec<&'static str>>,
    /// The languages the game speaks (`c.languages`): `world.language` is
    /// the player's of them, else the first.
    pub languages: Arc<Vec<String>>,
    /// The things' names in this level (`thing.name`, `level:thing(name)`):
    /// a placed thing's is its kind's and its number (`door_1`), a spawned
    /// one's what `level:spawn` named it.
    pub names: std::collections::BTreeMap<u32, String>,
    /// Where the things stood when touches were last looked for: how a
    /// touch began (`on_touch`'s `where`) is judged from there.
    pub seen_at: std::collections::BTreeMap<u32, Vec3>,
    /// Pictures played once where something happened (`level:effect`, a
    /// thing that went): only shown, not part of the game.
    pub effects: Vec<Shown>,
    /// What `level:spawn` gave a thing besides its kind's props.
    pub props: std::collections::BTreeMap<u32, super::value::Value>,
    /// Things told to have fallen out of the level (once each).
    pub fallen: std::collections::BTreeSet<u32>,
    /// How many frames each of the game's models has (`level:effect`).
    pub frames: Arc<std::collections::BTreeMap<String, usize>>,
    /// How tall each of the game's models is (a piece's body).
    pub heights: Arc<std::collections::BTreeMap<String, f32>>,
    /// The voxels each kind that bursts as it goes bursts into (`effect =
    /// "burst"`), by its place among the kinds.
    pub bursts: Arc<std::collections::BTreeMap<usize, crate::sim::Shape>>,
    /// The parts of its figures of parts (`frame:model`'s pose).
    pub parts: Arc<std::collections::BTreeMap<String, Vec<String>>>,
    /// What the HUD's means need between frames (`frame:icon`,
    /// `screen:burst`).
    pub hud: HudState,
    /// The songs of `c.audio.music`, their names and beats, and the song
    /// the game plays of its own choice with its clock (`music`).
    pub songs: Arc<Vec<crate::audio::music::Timing>>,
    pub music: Music,
    /// The pieces the level set down (`grid::PIECE`), by their things'
    /// ids: the model each shows, the way it faces, the light it brings.
    pub pieces: std::collections::BTreeMap<u32, crate::levels::Piece>,
    /// How bright each piece's light is, 0 (off) to 1 (`thing.state.light`):
    /// the game's to change, so in the snapshot, the hash and a save.
    pub lights: std::collections::BTreeMap<u32, f32>,
}

/// When a song the game asks for begins (`music:play`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Now,
    Beat,
    Bar,
}

/// The song the game plays of its own choice (`music:play`; none: the
/// player's) and its clock: the steps since it began, a step every tick
/// the game goes (a hit stop's too, so the music never waits), from which
/// its beats follow without adding up errors; the song asked for next and
/// from when; the last beat told (`on_beat`).
#[derive(Clone, Debug, PartialEq)]
pub struct Music {
    pub song: Option<usize>,
    pub steps: u64,
    pub next: Option<(Option<usize>, When)>,
    pub told: Option<u64>,
    /// The game's tension, 0..1 (`music.intensity`): the song's layers
    /// follow it.
    pub intensity: f32,
}

impl Default for Music {
    fn default() -> Self {
        Self { song: None, steps: 0, next: None, told: None, intensity: 1.0 }
    }
}

impl Music {
    /// The beats since its song began (60 steps a second); none without
    /// a song of the game's own.
    pub fn beats(&self, songs: &[crate::audio::music::Timing]) -> Option<f64> {
        let t = songs.get(self.song?)?;
        Some(self.steps as f64 * t.tempo as f64 / 3600.0)
    }

    /// A step on: the clock goes on, and the song asked for begins when
    /// its moment comes (a beat or a bar of the one playing begins; at
    /// once when none plays), at its beat 0.
    pub fn step(&mut self, songs: &[crate::audio::music::Timing]) {
        let before = self.beats(songs);
        if self.song.is_some() {
            self.steps += 1;
        }
        let Some((next, when)) = self.next else { return };
        let meter = self.song.and_then(|s| songs.get(s)).map_or(4, |t| t.meter) as f64;
        let due = match (before, self.beats(songs), when) {
            (Some(a), Some(b), When::Beat) => b.floor() > a.floor(),
            (Some(a), Some(b), When::Bar) => (b / meter).floor() > (a / meter).floor(),
            _ => true,
        };
        if due {
            (self.song, self.steps, self.told, self.next) = (next, 0, None, None);
        }
    }

    /// The beats begun since the last told, to tell now (`on_beat`).
    pub fn untold(&mut self, songs: &[crate::audio::music::Timing]) -> std::ops::Range<u64> {
        let Some(now) = self.beats(songs).map(|b| b.floor() as u64) else { return 0..0 };
        let from = self.told.map_or(0, |t| t + 1);
        self.told = Some(now);
        from..now + 1
    }
}

/// The HUD's means between frames: the game's icons (`c.icons`), the UI
/// grid's height and depth (`c.ui`), and the voxels `screen:burst` threw
/// (only shown, never part of the game: no hash, no replay sees them).
#[derive(Clone, Debug)]
pub struct HudState {
    pub icons: Arc<std::collections::BTreeMap<String, crate::config::IconConfig>>,
    pub height: f32,
    pub depth: i32,
    pub bits: Vec<Bit>,
    seed: u32,
}

/// A voxel thrown off an icon that burst, in the UI's voxels (up is up).
#[derive(Clone, Debug)]
pub struct Bit {
    pub pos: Vec3,
    pub prev: Vec3,
    pub vel: Vec3,
    pub color: [u8; 4],
    pub life: f32,
    pub span: f32,
}

impl HudState {
    pub fn new(cfg: &crate::config::GameConfig) -> Self {
        Self { icons: Arc::new(cfg.icons.clone()), height: cfg.ui.height, depth: cfg.ui.depth, bits: Vec::new(), seed: 0x1D_EA }
    }

    /// The config's icons and grid anew (a reload).
    pub fn apply(&mut self, cfg: &crate::config::GameConfig) {
        (self.icons, self.height, self.depth) = (Arc::new(cfg.icons.clone()), cfg.ui.height, cfg.ui.depth);
    }

    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }

    /// The icon `name` at `x`, `y` (its top left, from the HUD's top left)
    /// bursts: its voxels fly off and fall.
    pub fn burst(&mut self, name: &str, x: f32, y: f32) -> Result<(), String> {
        let Some(icon) = self.icons.get(name).cloned() else {
            let names: Vec<&str> = self.icons.keys().map(String::as_str).collect();
            return Err(format!("no icon called '{name}' (c.icons: {})", names.join(", ")));
        };
        let rows = icon.rows.len() as f32;
        let cols = icon.rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f32;
        let (cx, cy) = ((cols - 1.0) / 2.0, (rows - 1.0) / 2.0);
        for (c, r, color) in icon.voxels() {
            for z in 0..self.depth {
                let pos = Vec3::new(x + c as f32, self.height - y - 1.0 - r as f32, z as f32);
                let out = Vec3::new(c as f32 - cx, cy - r as f32, 0.0).normalize_or_zero();
                let vel = out * (25.0 + 35.0 * self.random()) + Vec3::new(0.0, 40.0 + 50.0 * self.random(), 20.0 * self.random());
                let span = 0.5 + 0.5 * self.random();
                self.bits.push(Bit { pos, prev: pos, vel, color, life: span, span });
            }
        }
        Ok(())
    }

    /// A tick of the thrown voxels: they fly, fall and fade.
    pub fn tick(&mut self) {
        self.bits.retain_mut(|b| {
            b.prev = b.pos;
            b.life -= crate::sim::DT;
            b.vel.y -= 260.0 * crate::sim::DT;
            b.pos += b.vel * crate::sim::DT;
            b.life > 0.0
        });
    }
}

/// A model played once at a point (`level:effect`).
#[derive(Clone, Debug)]
pub struct Shown {
    pub model: String,
    pub at: Vec3,
    pub age: u32,
    /// Frames a second.
    pub fps: f32,
}

/// How a touch began (`on_touch`'s `where`): which face of the other one
/// met it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Met {
    /// Its top: came down onto it (landed on it).
    Top,
    /// Its bottom: came up under it, or it came down onto this one.
    Bottom,
    Side,
}

impl Met {
    pub fn word(self) -> &'static str {
        match self {
            Met::Top => "top",
            Met::Bottom => "bottom",
            Met::Side => "side",
        }
    }
}

/// A body as touches see it: where it stands, how high, half its width,
/// the reach it is touched within (on the ground) if it has one.
#[derive(Clone, Copy, Debug)]
pub struct Touching {
    pub id: u32,
    pub at: Vec3,
    pub height: f32,
    pub half: f32,
    pub reach: Option<f32>,
}

impl Clone for Stage {
    fn clone(&self) -> Self {
        Self {
            host: self.host.box_clone(),
            kinds: self.kinds.clone(),
            sounds: self.sounds.clone(),
            gone: self.gone.clone(),
            levels: self.levels.clone(),
            level: self.level,
            visits: self.visits.clone(),
            ask: self.ask.clone(),
            arrived: self.arrived.clone(),
            ended: self.ended,
            saving: self.saving.clone(),
            slots: self.slots.clone(),
            slot_texts: self.slot_texts.clone(),
            replayed: self.replayed.clone(),
            loaded: self.loaded.clone(),
            touching: self.touching.clone(),
            dead: self.dead.clone(),
            graveyard: self.graveyard.clone(),
            placed: self.placed.clone(),
            later: self.later.clone(),
            killers: self.killers.clone(),
            controls: self.controls.clone(),
            seats: self.seats.clone(),
            local: self.local.clone(),
            ticking: false,
            actions: self.actions.clone(),
            languages: self.languages.clone(),
            hud: self.hud.clone(),
            names: self.names.clone(),
            seen_at: self.seen_at.clone(),
            effects: self.effects.clone(),
            props: self.props.clone(),
            fallen: self.fallen.clone(),
            frames: self.frames.clone(),
            heights: self.heights.clone(),
            bursts: self.bursts.clone(),
            parts: self.parts.clone(),
            songs: self.songs.clone(),
            music: self.music.clone(),
            pieces: self.pieces.clone(),
            lights: self.lights.clone(),
        }
    }
}

/// A change of the game's course the script asked for, done at the tick's
/// end (`docs/game-model.md`, "Leaving a level").
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// Another level (by its name: the levels may be read anew meanwhile),
    /// arriving at a place.
    GoTo { level: String, place: Option<String> },
    /// This level afresh, arriving at a place (else the checkpoint, else
    /// its start).
    Restart { place: Option<String> },
    /// A save: from a slot (`world:load`), or the one the run goes on from
    /// at once (`Game::start_from`).
    Load { slot: Option<u32> },
    Won,
    Lost,
}

impl Ask {
    /// Why the level is left, as `on_level_end` and `world.leaving` say.
    pub fn why(&self) -> &'static str {
        match self {
            Ask::GoTo { .. } => "go_to",
            Ask::Restart { .. } => "restart",
            Ask::Load { .. } => "load",
            Ask::Won => "won",
            Ask::Lost => "lost",
        }
    }

    /// Whether it goes on (to another level, to the win): going on
    /// outranks starting over (a restart, a loss), so a death in the tick
    /// the goal is reached does not undo it.
    pub fn goes_on(&self) -> bool {
        matches!(self, Ask::GoTo { .. } | Ask::Won)
    }

    /// Whether it takes the place of `held`, the ask that counted so far:
    /// only by going on where `held` starts over; among equals the first
    /// counts.
    pub fn outranks(&self, held: &Ask) -> bool {
        self.goes_on() && !held.goes_on()
    }
}

impl Stage {
    /// Seat `seat`'s figure where it is one of the things (seat 1's: the
    /// player's).
    pub fn figure(&self, seat: usize) -> Option<Id> {
        if seat == 1 { self.host.player_id() } else { self.host.seat_figure(seat) }
    }

    /// The handle of the player whose figure `id` is, if it is a player's.
    pub fn figure_handle(&self, id: Id) -> Option<u32> {
        (1..=self.seats.len()).find(|&n| self.figure(n) == Some(id)).map(seat_handle)
    }

    /// The kind of seat `seat`'s figure: its `state.kind`, else
    /// `c.player.kind`'s for the seat.
    pub fn seat_kind(&self, seat: usize) -> Option<usize> {
        let named = self.seats.get(seat.wrapping_sub(1)).and_then(|s| s.kind.clone());
        // The ready-made rules' hero kind where the game names none.
        let Some(name) = named.or_else(|| self.host.sim().cfg.player.kind.as_ref().map(|k| k.of(seat).to_string())) else { return self.host.hero_kind() };
        self.kinds.iter().position(|k| k.name == name)
    }

    /// Whether seat `seat`'s player is in play: someone sits there and it
    /// is not out.
    pub fn in_play(&self, seat: usize) -> bool {
        self.seats.get(seat.wrapping_sub(1)).is_some_and(|s| s.taken && !s.out)
    }

    /// Whether seat `seat`'s player plays in this level: in play, and its
    /// figure stands (a figure coming in at the tick's end does not yet; a
    /// game without figures for its players plays them all the same).
    pub fn playing(&self, seat: usize) -> bool {
        self.in_play(seat) && (self.seen(seat_handle(seat)).is_some() || self.seat_kind(seat).is_none() && self.host.player_seen().is_none())
    }

    /// The thing behind a handle's id (`PLAYER`: the player's figure; a
    /// seat's handle: its figure).
    pub fn seen(&self, id: u32) -> Option<Seen<'_>> {
        let living = match handle_seat(id) {
            Some(1) => self.host.player_seen(),
            Some(seat) => self.host.seat_figure(seat).and_then(|f| self.host.get(f)),
            None => self.host.get(Id::from_raw(id)),
        };
        living.or_else(|| {
            let (_, kind, body, half) = self.graveyard.iter().find(|(gone, ..)| *gone == id)?;
            Some(Seen { kind: *kind, body, hp: 0, flash: 0, half: *half, reach: None })
        })
    }

    /// The bodies touches are found among: the player's figure, then the
    /// level's things in the order they came; not the dead, nor what the
    /// script took away this tick.
    pub fn bodies(&self) -> Vec<Touching> {
        let host = &self.host;
        let mut bodies = Vec::new();
        if let Some(p) = host.player_seen().filter(|_| !self.dead.contains(&PLAYER) && self.in_play(1)) {
            bodies.push(Touching { id: PLAYER, at: p.body.pos, height: p.body.height, half: p.half, reach: p.reach });
        }
        // The seats after the first, in play, by their handles.
        for seat in 2..=self.seats.len() {
            let handle = seat_handle(seat);
            if let Some(p) = self.seen(handle).filter(|_| self.in_play(seat) && !self.dead.contains(&handle)) {
                bodies.push(Touching { id: handle, at: p.body.pos, height: p.body.height, half: p.half, reach: p.reach });
            }
        }
        let figures: Vec<Id> = (1..=self.seats.len()).filter_map(|n| self.figure(n)).collect();
        for id in host.ids() {
            if figures.contains(&id) || self.dead.contains(&id.raw()) || self.gone.contains(&id.raw()) {
                continue;
            }
            if let Some(t) = host.get(id) {
                bodies.push(Touching { id: id.raw(), at: t.body.pos, height: t.body.height, half: t.half, reach: t.reach });
            }
        }
        bodies
    }

    /// The pairs of things touching now, each once: the player first, else
    /// the one that came first. Touching: their boxes overlap, or one
    /// stands within the other's reach on the ground.
    pub fn contacts(&self) -> std::collections::BTreeSet<(u32, u32)> {
        let bodies = self.bodies();
        let mut pairs = std::collections::BTreeSet::new();
        for (i, a) in bodies.iter().enumerate() {
            for b in &bodies[i + 1..] {
                if self.host.touches(a, b).unwrap_or_else(|| touch(a, b)) {
                    // A player first, of two the lower seat; else the one
                    // that came first.
                    pairs.insert(match (handle_seat(a.id), handle_seat(b.id)) {
                        (Some(sa), Some(sb)) if sb < sa => (b.id, a.id),
                        (Some(_), _) => (a.id, b.id),
                        (None, Some(_)) => (b.id, a.id),
                        (None, None) => (a.id.min(b.id), a.id.max(b.id)),
                    });
                }
            }
        }
        pairs
    }

    /// How `a`'s touch of `b` began: from where both stood when touches
    /// were last looked for (`seen_at`). `a` met `b`'s top when its feet
    /// were at or above `b`'s top then (within a step's height) and it came
    /// down onto `b` (moving down against it, however fast); its bottom the
    /// other way round; its side otherwise, and always within a reach.
    pub fn met(&self, a: u32, b: u32) -> Met {
        let (Some(ta), Some(tb)) = (self.seen(a), self.seen(b)) else { return Met::Side };
        if ta.reach.is_some() || tb.reach.is_some() {
            return Met::Side;
        }
        let step = self.host.sim().cfg.physics.step as f32;
        let (na, nb) = (ta.body.pos, tb.body.pos);
        let (pa, pb) = (self.seen_at.get(&a).copied().unwrap_or(na), self.seen_at.get(&b).copied().unwrap_or(nb));
        let (fall_a, fall_b) = (na.z - pa.z, nb.z - pb.z);
        if pa.z >= pb.z + tb.body.height - step && fall_a < fall_b {
            Met::Top
        } else if pb.z >= pa.z + ta.body.height - step && fall_b < fall_a {
            Met::Bottom
        } else {
            Met::Side
        }
    }

    /// Names the level's things as they stand when it is entered: the name
    /// the level's file gives one (a piece's, a thing's of a kind in the
    /// order its kind's markers come), else its kind's name and its number
    /// among its kind (`door_1`), in the order they came; what the script
    /// gave or the level kept goes with them.
    pub fn name_things(&mut self) {
        (self.names, self.props, self.fallen, self.seen_at) = Default::default();
        let figures: Vec<Id> = (1..=self.seats.len()).filter_map(|n| self.figure(n)).collect();
        let level = self.levels.list[self.level].clone();
        // The names the file gives its things of kinds, by kind, in order.
        let mut given: std::collections::BTreeMap<usize, std::collections::VecDeque<Option<String>>> = Default::default();
        for (i, m) in level.markers.iter().enumerate() {
            given.entry(m.kind).or_default().push_back(level.marker_names.get(i).cloned().flatten());
        }
        let mut counts: std::collections::BTreeMap<usize, u32> = Default::default();
        for id in self.host.ids() {
            if figures.contains(&id) {
                continue;
            }
            let Some(kind) = self.host.get(id).map(|t| t.kind) else { continue };
            let Some(k) = self.kinds.get(kind) else { continue };
            let n = counts.entry(kind).or_insert(0);
            *n += 1;
            let own = match self.pieces.get(&id.raw()) {
                Some(p) => p.name.clone(),
                None => given.get_mut(&kind).and_then(|q| q.pop_front()).flatten(),
            };
            self.names.insert(id.raw(), own.unwrap_or_else(|| format!("{}_{n}", k.name)));
        }
    }

    /// The thing called `name` in this level, if it is still there.
    pub fn named(&self, name: &str) -> Option<u32> {
        self.names.iter().find(|(id, n)| n.as_str() == name && self.seen(**id).is_some() && !self.dead.contains(id)).map(|(id, _)| *id)
    }

    /// Remembers where every body stands now, for the next touches' `where`.
    pub fn note_positions(&mut self) {
        self.seen_at = self.bodies().iter().map(|b| (b.id, b.at)).collect();
    }

    /// What shows and sounds as a thing of `kind` goes at `at`, facing
    /// `look`: its kind's effect (a model played once, or the thing
    /// bursting into its voxels) and sound.
    pub fn goes(&mut self, kind: usize, at: Vec3, look: IVec2) {
        let Some(k) = self.kinds.get(kind) else { return };
        if let Some(shape) = self.bursts.get(&kind) {
            burst(self.host.sim_mut(), shape, at, look);
        } else if let Some(e) = k.effect.clone().filter(|e| self.frames.contains_key(&e.model)) {
            self.effects.push(Shown { model: e.model, at, age: 0, fps: e.fps });
        }
        if let Some(sound) = k.sound {
            self.host.sim_mut().sound(sound, at, 0.8);
        }
    }

    /// Takes the things the script removed out of the game (the tick's
    /// end), each with its kind's effect and sound.
    pub fn settle(&mut self) {
        for id in std::mem::take(&mut self.gone) {
            let going = self.host.get(Id::from_raw(id)).map(|t| (t.kind, t.body.pos, t.body.look));
            // (Not for what fell out of the level: nobody sees it go.)
            if let Some((kind, at, look)) = going.filter(|_| !self.dead.contains(&id) && !self.fallen.contains(&id)) {
                self.goes(kind, at, look);
            }
            self.host.remove(Id::from_raw(id));
        }
        // What died and went is read no more; the dead that went are no
        // longer kept as dead.
        self.graveyard.clear();
        let (host, player) = (&self.host, self.host.player_seen().is_some());
        self.dead.retain(|&id| if id == PLAYER { player } else { host.get(Id::from_raw(id)).is_some() });
    }
}

/// A thing bursting into its voxels where it stood, its model turned to
/// `look` as it was drawn (made facing +y): the simulation's pieces, as a
/// shooter's men burst (`Sim::gib`; no blood without `c.debris.gore`).
fn burst(sim: &mut Sim, shape: &crate::sim::Shape, at: Vec3, look: IVec2) {
    let look = if look == IVec2::ZERO { IVec2::Y } else { look };
    let size = if look.x != 0 { glam::IVec3::new(shape.size.y, shape.size.x, shape.size.z) } else { shape.size };
    let (pivot, turned_pivot) = (glam::IVec3::new(shape.size.x / 2, shape.size.y / 2, 0), glam::IVec3::new(size.x / 2, size.y / 2, 0));
    // Its right (+x) goes to the right of `look`, its front (+y) to `look`.
    let voxels: Vec<_> = shape
        .voxels
        .iter()
        .map(|&(v, color)| {
            let d = v - pivot;
            (glam::IVec3::new(d.x * look.y + d.y * look.x, d.y * look.y - d.x * look.x, d.z) + turned_pivot, color)
        })
        .collect();
    sim.gib(&voxels, size, at, Vec3::ZERO);
}

/// Whether two bodies touch: their boxes overlap, or one stands within the
/// other's reach on the ground (both with a reach: the larger, each is
/// touched within its own).
pub fn touch(a: &Touching, b: &Touching) -> bool {
    let d = a.at - b.at;
    let reach = match (a.reach, b.reach) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    };
    match reach {
        Some(reach) => d.truncate().length() < reach,
        None => d.x.abs() < a.half + b.half && d.y.abs() < a.half + b.half && a.at.z < b.at.z + b.height && b.at.z < a.at.z + a.height,
    }
}

/// Steers and moves the script's things the way they were told to walk
/// this tick (`thing:walk`), then forgets the walk. A thing of a kind
/// that is `fixed` has no gravity and does not land: it stays where it was
/// put.
pub fn move_things<B>(things: &mut Things<B>, sim: &Sim, fixed: &dyn Fn(usize) -> bool, walk: impl Fn(&mut B) -> Option<&mut Walk>) {
    let cfg = &sim.cfg;
    for t in things.iter_mut() {
        // Its own, or one whose ready-made rules are off (`thing:enable`).
        let Some(w) = t.off.as_mut().or_else(|| walk(&mut t.brain)) else { continue };
        if fixed(t.kind) {
            *w = Walk::default();
            t.flash = t.flash.saturating_sub(1);
            continue;
        }
        let accel = if t.body.on_ground { cfg.player.accel } else { cfg.player.air_accel };
        t.body.steer(w.wish, w.speed, accel);
        *w = Walk::default();
        t.body.advance(&sim.world, &cfg.physics);
        t.flash = t.flash.saturating_sub(1);
    }
}

/// A Lua game's world without a kit: the core and the script's things.
#[derive(Clone)]
pub struct Plain {
    pub sim: Sim,
    pub things: Things<Walk>,
    pub kinds: Arc<Vec<Kind>>,
    /// The thing the camera follows (the first spawned, or `level.camera:follow`).
    pub focus: Option<Id>,
    /// The game pointed the camera at a place (`level.camera:look_at`): no
    /// thing spawned takes it until `follow` or the next level.
    pub looking: bool,
    pub score: u32,
    pub outcome: Option<Outcome>,
    /// The player's figure, a thing of `c.player.kind` going from level to
    /// level (`player`), and those of the seats after the first (seat 2
    /// first; None while it has none standing).
    pub player: Option<Id>,
    pub seats: Vec<Option<Id>>,
}

impl Plain {
    /// Whether the player's figure keeps hit points (its kind has `hp`):
    /// `player.state.hp` is then its.
    fn player_hp(&self) -> bool {
        self.player.and_then(|id| self.things.get(id)).is_some_and(|t| self.kinds[t.kind].has_hp)
    }
}

impl Host for Plain {
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
        self.things.get(id).map(|t| Seen { kind: t.kind, body: &t.body, hp: t.hp, flash: t.flash, half: self.sim.half(), reach: self.kinds[t.kind].reach })
    }

    fn get_mut(&mut self, id: Id) -> Option<Touched<'_>> {
        self.things.get_mut(id).map(|t| Touched { hp: &mut t.hp, moved: Some((&mut t.body, &mut t.brain)) })
    }

    fn spawn(&mut self, kind: usize, x: f32, y: f32, z: Option<f32>) -> Result<Id, String> {
        let k = &self.kinds[kind];
        let at = match z {
            Some(z) => Vec3::new(x, y, z),
            None => stand_on_ground(&self.sim.world, Vec3::new(x, y, 0.0), k.height, self.sim.half()),
        };
        let id = self.things.spawn(kind, Body::new(at, k.height), k.hp, Walk::default());
        if !self.looking {
            self.focus.get_or_insert(id);
        }
        Ok(id)
    }

    /// Takes a thing out; the camera's thing taken, the next spawned is
    /// followed.
    fn remove(&mut self, id: Id) -> bool {
        if self.focus == Some(id) {
            self.focus = None;
        }
        self.things.remove(id).is_some()
    }

    fn score(&self) -> u32 {
        self.score
    }

    fn set_score(&mut self, score: u32) {
        self.score = score;
    }

    fn player_keys(&self) -> &'static [&'static str] {
        if self.player_hp() { &["score", "hp"] } else { &["score"] }
    }

    fn player_number(&self, key: &str) -> Option<i64> {
        match key {
            "score" => Some(self.score as i64),
            "hp" if self.player_hp() => self.player.and_then(|id| self.things.get(id)).map(|t| t.hp as i64),
            _ => None,
        }
    }

    fn set_player_number(&mut self, key: &str, value: i64) -> bool {
        match key {
            "score" => self.score = value.max(0) as u32,
            "hp" if self.player_hp() => {
                if let Some(t) = self.player.and_then(|id| self.things.get_mut(id)) {
                    t.hp = value as i32;
                }
            }
            _ => return false,
        }
        true
    }

    fn player_id(&self) -> Option<Id> {
        self.player
    }

    fn seat_figure(&self, seat: usize) -> Option<Id> {
        seat.checked_sub(2).and_then(|i| self.seats.get(i).copied().flatten())
    }

    fn put_player(&mut self, at: Vec3) -> Result<(), String> {
        let half = self.sim.half();
        let Some(t) = self.player.and_then(|id| self.things.get_mut(id)) else { return Err("the player has no figure".into()) };
        let at = crate::sim::stand_at(&self.sim.world, at, t.body.height, half);
        (t.body.pos, t.body.prev, t.body.vel) = (at, at, Vec3::ZERO);
        Ok(())
    }

    fn end(&mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
    }

    fn follow(&mut self, id: Id) -> Result<(), String> {
        (self.focus, self.looking) = (Some(id), false);
        Ok(())
    }

    fn look_at(&mut self, x: f32, y: f32) -> Result<(), String> {
        (self.focus, self.looking) = (None, true);
        (self.sim.cam_target.x, self.sim.cam_target.y) = (x, y);
        Ok(())
    }

    fn box_clone(&self) -> Box<dyn Host> {
        Box::new(self.clone())
    }
}

/// One of two kits, as `c.kit` names it: a program offers several kits as
/// `Either<A, Either<B, C>>` (the Voxcade program: the shooter and the
/// platformer). Everything goes to the kit it is; what a kit is asked
/// before it exists, by its config, goes to the one the config names.
#[derive(Clone)]
pub enum Either<A, B> {
    A(A),
    B(B),
}

/// The kept state of either kit.
pub enum EitherSnapshot<A: Game, B: Game> {
    A(A::Snapshot),
    B(B::Snapshot),
}

macro_rules! each {
    ($value:expr, $kit:ident => $then:expr) => {
        match $value {
            Either::A($kit) => $then,
            Either::B($kit) => $then,
        }
    };
}

impl<A: Kit, B: Kit> Either<A, B> {
    /// Whether the config's kit is the first.
    fn first(cfg: &GameConfig) -> bool {
        cfg.kit.as_deref().is_some_and(A::named)
    }
}

impl<A: Kit, B: Kit> Game for Either<A, B> {
    type Snapshot = EitherSnapshot<A, B>;

    fn sections(cfg: &GameConfig) -> Vec<&'static str> {
        if Self::first(cfg) { A::sections(cfg) } else { B::sections(cfg) }
    }

    fn behaviours(cfg: &GameConfig) -> Vec<&'static str> {
        if Self::first(cfg) { A::behaviours(cfg) } else { B::behaviours(cfg) }
    }

    fn actions(cfg: &GameConfig) -> Vec<&'static str> {
        if Self::first(cfg) { A::actions(cfg) } else { B::actions(cfg) }
    }

    fn default_keys(cfg: &GameConfig) -> Bindings {
        if Self::first(cfg) { A::default_keys(cfg) } else { B::default_keys(cfg) }
    }

    fn default_pad(cfg: &GameConfig) -> Bindings {
        if Self::first(cfg) { A::default_pad(cfg) } else { B::default_pad(cfg) }
    }

    fn sounds(cfg: &GameConfig) -> Vec<(&'static str, crate::audio::SoundDef)> {
        if Self::first(cfg) { A::sounds(cfg) } else { B::sounds(cfg) }
    }

    fn models(cfg: &GameConfig) -> Result<Vec<ModelSpec>> {
        if Self::first(cfg) { A::models(cfg) } else { B::models(cfg) }
    }

    fn level(cfg: &GameConfig) -> Result<Option<World>> {
        if Self::first(cfg) { A::level(cfg) } else { B::level(cfg) }
    }

    fn load(setup: Setup) -> Result<Self> {
        if Self::first(setup.cfg) { A::load(setup).map(Either::A) } else { B::load(setup).map(Either::B) }
    }

    fn snapshot(&self) -> Result<Self::Snapshot> {
        Ok(match self {
            Either::A(k) => EitherSnapshot::A(k.snapshot()?),
            Either::B(k) => EitherSnapshot::B(k.snapshot()?),
        })
    }

    fn restore(&mut self, kept: &Self::Snapshot) {
        match (self, kept) {
            (Either::A(k), EitherSnapshot::A(s)) => k.restore(s),
            (Either::B(k), EitherSnapshot::B(s)) => k.restore(s),
            _ => panic!("a kit's state brought back into the other kit"),
        }
    }

    fn sim(&self) -> &Sim {
        each!(self, k => Game::sim(k))
    }

    fn sim_mut(&mut self) -> &mut Sim {
        each!(self, k => Game::sim_mut(k))
    }

    fn step(&mut self, input: &Input) {
        each!(self, k => k.step(input))
    }

    fn state_hash(&self) -> u64 {
        each!(self, k => k.state_hash())
    }

    fn draw(&self, out: &mut Drawing) {
        each!(self, k => k.draw(out))
    }

    fn focus(&self) -> Option<&Body> {
        each!(self, k => k.focus())
    }

    fn outcome(&self) -> Option<Outcome> {
        each!(self, k => k.outcome())
    }

    fn scored(&self) -> bool {
        each!(self, k => k.scored())
    }

    fn score(&self) -> u32 {
        each!(self, k => Game::score(k))
    }

    fn rumble(&self) -> Option<(f32, u32)> {
        each!(self, k => k.rumble())
    }

    fn summary(&self) -> Option<String> {
        each!(self, k => k.summary())
    }

    fn numbers(&self) -> Vec<String> {
        each!(self, k => k.numbers())
    }

    fn report(&self) -> Vec<String> {
        each!(self, k => k.report())
    }

    fn place_player(&mut self, x: f32, y: f32) {
        each!(self, k => k.place_player(x, y))
    }

    fn apply_config(&mut self, cfg: &GameConfig) -> Result<()> {
        each!(self, k => k.apply_config(cfg))
    }

    fn replace_world(&mut self, world: World, markers: &[Marker]) {
        each!(self, k => k.replace_world(world, markers))
    }

    fn reshape(&mut self, models: &Models) {
        each!(self, k => k.reshape(models))
    }
}

impl<A: Kit, B: Kit> Host for Either<A, B> {
    fn sim(&self) -> &Sim {
        each!(self, k => Host::sim(k))
    }

    fn sim_mut(&mut self) -> &mut Sim {
        each!(self, k => Host::sim_mut(k))
    }

    fn ids(&self) -> Vec<Id> {
        each!(self, k => k.ids())
    }

    fn get(&self, id: Id) -> Option<Seen<'_>> {
        each!(self, k => k.get(id))
    }

    fn get_mut(&mut self, id: Id) -> Option<Touched<'_>> {
        each!(self, k => k.get_mut(id))
    }

    fn spawn(&mut self, kind: usize, x: f32, y: f32, z: Option<f32>) -> Result<Id, String> {
        each!(self, k => k.spawn(kind, x, y, z))
    }

    fn remove(&mut self, id: Id) -> bool {
        each!(self, k => k.remove(id))
    }

    fn score(&self) -> u32 {
        each!(self, k => Host::score(k))
    }

    fn set_score(&mut self, score: u32) {
        each!(self, k => k.set_score(score))
    }

    fn player_keys(&self) -> &'static [&'static str] {
        each!(self, k => k.player_keys())
    }

    fn player_number(&self, key: &str) -> Option<i64> {
        each!(self, k => k.player_number(key))
    }

    fn set_player_number(&mut self, key: &str, value: i64) -> bool {
        each!(self, k => k.set_player_number(key, value))
    }

    fn end(&mut self, outcome: Outcome) {
        each!(self, k => k.end(outcome))
    }

    fn follow(&mut self, id: Id) -> Result<(), String> {
        each!(self, k => k.follow(id))
    }

    fn look_at(&mut self, x: f32, y: f32) -> Result<(), String> {
        each!(self, k => k.look_at(x, y))
    }

    fn start_at(&self, world: &World, markers: &[Marker]) -> Option<Vec3> {
        each!(self, k => k.start_at(world, markers))
    }

    fn hurt(&mut self, who: crate::sim::Who, damage: i32, by: Option<crate::sim::Who>, weapon: Option<String>) -> Result<(), String> {
        each!(self, k => k.hurt(who, damage, by, weapon))
    }

    fn clear_rounds(&mut self, team: Option<&str>) -> Result<(), String> {
        each!(self, k => k.clear_rounds(team))
    }

    fn launch(&mut self, id: Id, vel: Vec3) -> Result<(), String> {
        each!(self, k => k.launch(id, vel))
    }

    fn enable(&mut self, id: Id, behaviour: &str, on: bool) -> Result<(), String> {
        each!(self, k => k.enable(id, behaviour, on))
    }

    fn thing_keys(&self, id: Id) -> &'static [&'static str] {
        each!(self, k => k.thing_keys(id))
    }

    fn thing_value(&self, id: Id, key: &str) -> Option<Kept> {
        each!(self, k => k.thing_value(id, key))
    }

    fn set_thing_value(&mut self, id: Id, key: &str, value: Kept) -> Result<(), String> {
        each!(self, k => k.set_thing_value(id, key, value))
    }

    fn player_kept(&self, key: &str) -> Option<Kept> {
        each!(self, k => k.player_kept(key))
    }

    fn set_player_kept(&mut self, key: &str, value: Kept) -> Result<(), String> {
        each!(self, k => k.set_player_kept(key, value))
    }

    fn touches(&self, a: &Touching, b: &Touching) -> Option<bool> {
        each!(self, k => k.touches(a, b))
    }

    fn face(&mut self, id: Id, way: Vec3) -> Result<(), String> {
        each!(self, k => k.face(id, way))
    }

    fn stock(&self) -> Vec<(String, u32)> {
        each!(self, k => k.stock())
    }

    fn moved_player_keys(&self) -> &'static [(&'static str, &'static str)] {
        each!(self, k => k.moved_player_keys())
    }

    fn attack(&mut self, who: crate::sim::Who, weapon: &str, at: Option<(Vec3, Option<Id>)>) -> Result<(), String> {
        each!(self, k => k.attack(who, weapon, at))
    }

    fn start_attacks(&mut self) {
        each!(self, k => k.start_attacks())
    }

    fn set_stock(&mut self, item: &str, count: u32) {
        each!(self, k => k.set_stock(item, count))
    }

    fn seat_stock(&self, seat: usize) -> Vec<(String, u32)> {
        each!(self, k => k.seat_stock(seat))
    }

    fn set_seat_stock(&mut self, seat: usize, item: &str, count: u32) {
        each!(self, k => k.set_seat_stock(seat, item, count))
    }

    fn seat_kept(&self, seat: usize, key: &str) -> Option<Kept> {
        each!(self, k => k.seat_kept(seat, key))
    }

    fn set_seat_kept(&mut self, seat: usize, key: &str, value: Kept) -> Result<(), String> {
        each!(self, k => k.set_seat_kept(seat, key, value))
    }

    fn put_player(&mut self, at: Vec3) -> Result<(), String> {
        each!(self, k => k.put_player(at))
    }

    fn bounce(&mut self, id: Id, speed: f32) -> Result<(), String> {
        each!(self, k => k.bounce(id, speed))
    }

    fn player_id(&self) -> Option<Id> {
        each!(self, k => k.player_id())
    }

    fn seat_figure(&self, seat: usize) -> Option<Id> {
        each!(self, k => k.seat_figure(seat))
    }

    fn put_seat(&mut self, seat: usize, figure: Option<(usize, Vec3, Option<IVec2>, i32)>) -> Result<Option<Id>, String> {
        each!(self, k => k.put_seat(seat, figure))
    }

    fn put_figure(&mut self, id: Id, at: Vec3) -> Result<(), String> {
        each!(self, k => k.put_figure(id, at))
    }

    fn hero_kind(&self) -> Option<usize> {
        each!(self, k => k.hero_kind())
    }

    fn figure_number(&self, id: Id, key: &str) -> Option<i64> {
        each!(self, k => k.figure_number(id, key))
    }

    fn follows_players(&self) -> bool {
        each!(self, k => k.follows_players())
    }

    fn hold_back(&mut self, id: Id, at: Vec3) {
        each!(self, k => k.hold_back(id, at))
    }

    fn set_figure_number(&mut self, id: Id, key: &str, value: i64) -> bool {
        each!(self, k => k.set_figure_number(id, key, value))
    }

    fn player_seen(&self) -> Option<Seen<'_>> {
        each!(self, k => k.player_seen())
    }

    fn box_clone(&self) -> Box<dyn Host> {
        Box::new(self.clone())
    }
}

impl<A: Kit, B: Kit> Kit for Either<A, B> {
    fn named(name: &str) -> bool {
        A::named(name) || B::named(name)
    }

    fn names() -> Vec<&'static str> {
        let mut names = A::names();
        names.extend(B::names());
        names
    }

    fn rules(&mut self, input: &Input) {
        each!(self, k => k.rules(input))
    }

    fn set_seat_inputs(&mut self, inputs: &[Option<Input>]) {
        each!(self, k => k.set_seat_inputs(inputs))
    }

    fn set_seat_teams(&mut self, teams: &[Option<&'static str>]) {
        each!(self, k => k.set_seat_teams(teams))
    }

    fn plays_several(cfg: &GameConfig) -> bool {
        if Self::first(cfg) { A::plays_several(cfg) } else { B::plays_several(cfg) }
    }

    fn held(&mut self) -> bool {
        each!(self, k => k.held())
    }

    fn move_scripted(&mut self, fixed: &dyn Fn(usize) -> bool) {
        each!(self, k => k.move_scripted(fixed))
    }

    fn built(&mut self) {
        each!(self, k => k.built())
    }

    fn enter_level(&mut self, world: World, markers: &[Marker], places: &[crate::levels::Place], at: Option<(Vec3, Option<IVec2>)>) {
        each!(self, k => k.enter_level(world, markers, places, at))
    }

    fn run(&self) -> toml::Table {
        each!(self, k => k.run())
    }

    fn set_run(&mut self, run: &toml::Table) {
        each!(self, k => k.set_run(run))
    }
}
