//! A game's settings (`conf.lua`, `function conf(c)`): every field has a
//! default, so a game only states what differs.
//!
//! Every length is in voxels and every speed in voxels per second: there is
//! no unit conversion. A game drawn on a finer grid states bigger numbers
//! (a jump of 8 voxels with 10-voxel figures is a jump of 16 with
//! 20-voxel figures). The defaults fit figures about 10 voxels tall.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::audio::GameSound;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct GameConfig {
    /// The Voxcade the game's files are for (`c.voxcade = "0.6.0"`): a game
    /// for an older one is brought up to date by `voxcade upgrade`, one for
    /// a newer one does not open (`crate::upgrade`, which reads the line
    /// from `conf.lua` itself). In neither the fingerprint nor the hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voxcade: Option<String>,
    /// The game's name: the window's title, the title screen's logo
    /// unless `title_screen.logo` says otherwise.
    pub title: String,
    pub world: WorldConfig,
    pub physics: PhysicsConfig,
    pub player: PlayerConfig,
    pub title_screen: TitleScreenConfig,
    pub camera: CameraConfig,
    pub light: LightConfig,
    pub debris: DebrisConfig,
    pub effects: EffectsConfig,
    pub transition: TransitionConfig,
    pub audio: AudioConfig,
    pub ui: UiConfig,
    /// The game's own sounds by name: its files in `sounds/` (`NAME.toml`;
    /// `audio::sound`), read with the config, never written in
    /// it (`c.sounds` is refused). One of a kit's names replaces the kit's.
    #[serde(skip)]
    pub sounds: BTreeMap<String, GameSound>,
    pub markers: MarkersConfig,
    /// The kinds of things the game has (`c.things.NAME = { ... }`): each
    /// may name a behavior of its ready-made rules, may build on another
    /// (`like = "other"`) and says what differs; a level places them by
    /// name (`sim::things::defs`).
    pub things: BTreeMap<String, toml::Table>,
    /// The building kit a Lua game plays with (`"shooter"`), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kit: Option<String>,
    /// A Lua game's actions besides the four directions (a kit brings its
    /// own), and the keys and gamepad buttons bound to them at first.
    pub actions: Vec<String>,
    /// The keys bound to each action at first, by their US names
    /// (`c.keys.jump = { "Z" }`); the player may change them.
    pub keys: crate::controls::Bindings,
    /// The gamepad's buttons bound to each action at first
    /// (`c.pad.jump = { "A" }`).
    pub pad: crate::controls::Bindings,
    /// No more: `frame:model` draws any model file of the game by its
    /// path, loaded when it is first drawn. Kept to say so to a `conf.lua`
    /// that sets it (`voxcade upgrade` takes it out).
    #[serde(skip_serializing)]
    pub models: Option<toml::Value>,
    /// The game keeps a score (`player.state.score`), with ready-made rules
    /// or without: the title shows the best, and runs go into the high
    /// score table.
    pub scored: bool,
    /// What can be carried (`c.items.key = { model = "key.vvx", stack = 1 }`):
    /// an inventory (`player.state.inventory`) counts these by name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub items: BTreeMap<String, ItemConfig>,
    /// What a run starts with (`c.state.world`, `c.state.player`): the
    /// states' first keys, the engine's own among them (`hp`, `weapon`,
    /// `inventory`) and the game's (`lives`).
    #[serde(skip_serializing_if = "StateConfig::is_empty")]
    pub state: StateConfig,
    /// Small pictures for the HUD (`frame:icon`, `screen:burst`):
    /// `c.icons.heart = { rows = { ".RR.RR.", ... }, colors = { R = { 214, 36, 44 } } }`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub icons: BTreeMap<String, IconConfig>,
    /// Save games (`c.saves = { slots = 3, version = 1 }`): none without
    /// slots.
    #[serde(skip_serializing_if = "SavesConfig::is_off")]
    pub saves: SavesConfig,
    /// The high score table to beat before anyone played: up to ten lines
    /// of initials and points (`{ { "VOX", 50000 }, ... }`); without them
    /// the engine's.
    pub high_scores: Vec<(String, u32)>,
    /// The languages the game speaks (`{ "de", "en", "fr" }`, two letters
    /// each): SETTINGS offers them, `world.language` says which the player
    /// chose, the engine's menus speak it. None: English.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// The seats the game has, how many play at once (`c.players = 4`;
    /// `docs/game-model.md`, "Several players"): 1, a game of one, unless
    /// set.
    #[serde(skip_serializing_if = "is_one")]
    pub players: u32,
    /// The high score table takes the run as one line, every seat's score
    /// added, instead of a line for each player.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub team_score: bool,
    /// The sections the game adds (the shooter's `[weapons]`): it reads
    /// them itself, and a section nobody reads is an error there.
    #[serde(flatten)]
    pub game: toml::Table,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            voxcade: None,
            title: "Untitled".into(),
            world: WorldConfig::default(),
            physics: PhysicsConfig::default(),
            player: PlayerConfig::default(),
            title_screen: TitleScreenConfig::default(),
            camera: CameraConfig::default(),
            light: LightConfig::default(),
            debris: DebrisConfig::default(),
            effects: EffectsConfig::default(),
            transition: TransitionConfig::default(),
            audio: AudioConfig::default(),
            ui: UiConfig::default(),
            sounds: BTreeMap::new(),
            game: toml::Table::new(),
            markers: MarkersConfig::default(),
            things: BTreeMap::new(),
            kit: None,
            actions: Vec::new(),
            keys: Default::default(),
            pad: Default::default(),
            models: None,
            items: BTreeMap::new(),
            icons: BTreeMap::new(),
            state: StateConfig::default(),
            saves: SavesConfig::default(),
            scored: false,
            high_scores: Vec::new(),
            languages: Vec::new(),
            players: 1,
            team_score: false,
        }
    }
}

fn is_one(n: &u32) -> bool {
    *n == 1
}

/// The most seats a game may have (`c.players`).
pub const MOST_PLAYERS: u32 = 16;

/// The kind of the players' figures (`c.player.kind`): one for every seat,
/// or one for each seat in turn, the last for the seats after it. Ready-
/// made rules bring their own player in a game of one; in a game of
/// several the jump & run's heroes are of these kinds, and the shooter's
/// soldiers (kinds without behavior: their keys move them).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum SeatKinds {
    One(String),
    Seats(Vec<String>),
}

impl SeatKinds {
    /// Seat `seat`'s (from 1).
    pub fn of(&self, seat: usize) -> &str {
        match self {
            SeatKinds::One(name) => name,
            SeatKinds::Seats(names) => names.get(seat.saturating_sub(1)).or(names.last()).map_or("", String::as_str),
        }
    }

    /// Every name it gives, seat 1's first.
    pub fn names(&self) -> Vec<&str> {
        match self {
            SeatKinds::One(name) => vec![name],
            SeatKinds::Seats(names) => names.iter().map(String::as_str).collect(),
        }
    }
}

/// Save games: how many slots the player has (the pause menu's SAVE and
/// LOAD, `world:save`; at most 9), and the game's own version of what a save holds
/// (an older save is brought up to date by `on_save_loaded(version)`).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct SavesConfig {
    /// How many save slots the player has (0: no saving).
    pub slots: u32,
    /// The game's own version of what a save holds: count it up when that
    /// changes, `on_save_loaded(version)` brings an older save up to date.
    pub version: u32,
}

impl Default for SavesConfig {
    fn default() -> Self {
        Self { slots: 0, version: 1 }
    }
}

impl SavesConfig {
    pub fn is_off(&self) -> bool {
        *self == Self::default()
    }
}

/// The states a run starts with (`c.state`).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct StateConfig {
    /// `world.state` at the start.
    pub world: toml::Table,
    /// `player.state` at the start.
    pub player: toml::Table,
}

impl StateConfig {
    pub fn is_empty(&self) -> bool {
        self.world.is_empty() && self.player.is_empty()
    }

    /// The player's start number `key` (`hp`), if the game gives one.
    pub fn player_number(&self, key: &str) -> Option<i64> {
        match self.player.get(key)? {
            toml::Value::Integer(n) => Some(*n),
            toml::Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }
}

/// A small picture for the HUD (`c.icons.NAME`): its rows from the top,
/// each letter a colour of `colors`, `.` empty.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct IconConfig {
    /// Its rows from the top, a letter a colour of `colors`, `.` empty.
    pub rows: Vec<String>,
    /// A letter's colour: red, green, blue (and alpha), 0 to 255.
    pub colors: BTreeMap<String, Vec<u8>>,
}

impl IconConfig {
    /// Its voxels: (column, row from the top, colour).
    pub fn voxels(&self) -> Vec<(usize, usize, [u8; 4])> {
        let mut out = Vec::new();
        for (r, row) in self.rows.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                if let Some(rgb) = self.colors.get(ch.to_string().as_str()) {
                    out.push((c, r, [rgb[0], rgb[1], rgb[2], rgb.get(3).copied().unwrap_or(255)]));
                }
            }
        }
        out
    }

    /// A mistake in it, named by `what` (`c.icons.heart`).
    pub fn check(&self, what: &str) -> Result<(), String> {
        if self.rows.is_empty() {
            return Err(format!("{what}.rows: the picture's rows from the top, one string each"));
        }
        for (letter, rgb) in &self.colors {
            if letter.chars().count() != 1 || letter == "." {
                return Err(format!("{what}.colors.{letter}: one letter names a color (\".\" is empty)"));
            }
            if !(3..=4).contains(&rgb.len()) {
                return Err(format!("{what}.colors.{letter}: red, green, blue (and alpha), 0 to 255"));
            }
        }
        for (r, row) in self.rows.iter().enumerate() {
            if let Some(ch) = row.chars().find(|&ch| ch != '.' && !self.colors.contains_key(ch.to_string().as_str())) {
                return Err(format!("{what}.rows[{}]: the letter '{ch}' has no color in {what}.colors", r + 1));
            }
        }
        Ok(())
    }
}

/// A kind of item (`c.items.NAME`).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ItemConfig {
    /// Its model (.vvx), for the script to draw (`frame:model`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// How many one inventory holds at most.
    pub stack: u32,
    /// Where it comes in `world.items` (then by name).
    pub order: i32,
}

impl Default for ItemConfig {
    fn default() -> Self {
        Self { model: None, stack: 1, order: 0 }
    }
}

impl GameConfig {
    /// The items' names in their order (`order`, then the name).
    pub fn item_names(&self) -> Vec<&str> {
        let mut names: Vec<(&i32, &str)> = self.items.iter().map(|(n, i)| (&i.order, n.as_str())).collect();
        names.sort();
        names.into_iter().map(|(_, n)| n).collect()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct WorldConfig {
    /// The level's file, relative to the game folder: a world (`.vvx`,
    /// `crate::scene`) or a map (`.toml`, `crate::map`).
    pub file: String,
    /// The lowest voxel layer cannot be destroyed.
    pub bedrock: bool,
    /// A world made in code: empty, this big (x, y, z); the game's script
    /// builds it (`level:fill`), `file` is not read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[i32; 3]>,
    /// A level of voxel blocks in a grid (`grid`): `file` is not read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<BlocksConfig>,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self { file: "world.vvx".into(), bedrock: true, size: None, blocks: None }
    }
}

/// A level of voxel blocks in a grid (`grid`, `docs/tiles.md`): its file,
/// and what lies around it.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct BlocksConfig {
    /// The level, relative to the game folder; its blocks in `blocks/`.
    pub file: String,
    /// Space left and right of the grid, in voxels, for the backdrops.
    pub margin: i32,
    /// Open space under the grid, in voxels: pits deep enough to fall out of.
    pub below: i32,
    /// Pictures far behind the grid (sky, hills): parallax through the
    /// perspective.
    pub backdrops: Vec<Backdrop>,
}

impl Default for BlocksConfig {
    fn default() -> Self {
        Self { file: "level.toml".into(), margin: 0, below: 0, backdrops: Vec::new() }
    }
}

/// A picture far behind a level (`BlocksConfig::backdrops`): a wall one
/// voxel thick, repeated across the world.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Backdrop {
    /// The picture, relative to the game folder.
    pub image: String,
    /// How far behind the level's last row, in voxels.
    pub behind: i32,
    /// Voxels a pixel.
    pub scale: i32,
    /// The height of its bottom edge, in voxels.
    pub z: i32,
}

impl Default for Backdrop {
    fn default() -> Self {
        Self { image: String::new(), behind: 200, scale: 1, z: 0 }
    }
}

/// How everything falls and walks.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsConfig {
    /// Downward acceleration, voxels per second².
    pub gravity: f32,
    /// Fastest fall, voxels per second.
    pub max_fall: f32,
    /// Ledges up to this many voxels are walked up without jumping.
    pub step: i32,
    /// Width of the collision box of everything that walks. Effects (sparks,
    /// debris, gibs, screen shake) are sized relative to it.
    pub body_width: f32,
    /// How far under the level a falling body is lost (`on_fall_out`), in
    /// voxels. (At its default it is left out of replays' fingerprint:
    /// those from before it play on.)
    #[serde(skip_serializing_if = "PhysicsConfig::lost_below_is_default")]
    pub lost_below: f32,
}

impl PhysicsConfig {
    fn lost_below_is_default(lost_below: &f32) -> bool {
        *lost_below == Self::default().lost_below
    }
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self { gravity: 260.0, max_fall: 160.0, step: 2, body_width: 3.0, lost_below: 48.0 }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct PlayerConfig {
    /// Player model (.vvx), relative to the game folder. Forward is +y.
    pub model: String,
    /// Start position on the ground (x, y). Negative means world centre.
    pub spawn: [f32; 2],
    /// Running speed.
    pub speed: f32,
    /// How quickly it gets to running speed on the ground, and in the air.
    pub accel: f32,
    /// How quickly it gets to running speed in the air.
    pub air_accel: f32,
    /// Height of a full jump (holding the button); letting go early cuts it.
    pub jump: f32,
    /// A game without ready-made rules: the kind of thing (`c.things`) the
    /// player's figure is (`player`), placed where each level starts and
    /// going along from level to level; with several seats one for every
    /// seat or a list by seat (`{ "hero", "hero_blue" }`). Without it the
    /// player has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<SeatKinds>,
    /// What the game adds to its player (the shooter: knife, muzzle ...);
    /// it reads them itself.
    #[serde(flatten)]
    pub game: toml::Table,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            model: "player.vvx".into(),
            spawn: [-1.0, -1.0],
            speed: 46.0,
            accel: 900.0,
            air_accel: 350.0,
            jump: 8.4,
            kind: None,
            game: toml::Table::new(),
        }
    }
}

/// The title screen: where its camera looks and how close, what lies
/// there, what flies over, the logo and the song.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct TitleScreenConfig {
    /// What plays on the title (a song of `c.audio.music`; none: the first).
    pub music: Option<String>,
    /// The ground point the camera looks at (default: where the player starts).
    pub at: Option<[f32; 2]>,
    /// Voxels across the screen there (default: `c.camera.view_width`).
    pub view_width: Option<f32>,
    /// The logo, a line each (default: the game's title), and their
    /// colours as "#rrggbb".
    pub logo: Vec<String>,
    /// Each logo line's colour, "#rrggbb".
    pub logo_colors: Vec<String>,
    /// How high each logo line's capitals are, in UI voxels.
    pub logo_heights: Vec<u32>,
    /// Which logo line drips (blood).
    pub drip: Option<usize>,
    /// Cracks per letter in each logo line, on average.
    pub logo_cracks: Vec<f32>,
    /// A model of the game's (`.vvx`) as the logo, in place of its lines
    /// of letters: a UI voxel each of its voxels, its front (+y) to the
    /// player and its z up, centred under the top of the screen. A figure
    /// of parts (its objects named, a letter each) drops in part by part
    /// in the file's order, else whole. Lit as the big letters are (the
    /// sides a little darker, what lies further back too): its colours are
    /// its materials'.
    pub logo_model: Option<String>,
    /// How far the logo's letters (or its model's parts) bob up and down
    /// once they have landed, in UI voxels: a slow, smooth wave running
    /// through them, coming in over a second; 0 (unless set) they stand
    /// still.
    pub logo_bob: f32,
    /// Models lying in the scene.
    pub props: Vec<PropConfig>,
    /// A model that flies over the scene now and then (a helicopter),
    /// every `flyby_every` seconds.
    pub flyby: Option<String>,
    /// Seconds between two flybys (0: none).
    pub flyby_every: f32,
    /// How high the flyby crosses over the ground.
    pub flyby_altitude: f32,
    /// Seconds on the title without a key pressed before it plays the
    /// game's replays (`replays/`), as the arcade machines did; 0: never.
    pub attract_after: f32,
    /// The scene behind the menu is dark: its rows and hint in light
    /// letters (default: dark ones, over bright ground like More Blood's
    /// sand).
    pub light_menu: bool,
}

impl Default for TitleScreenConfig {
    fn default() -> Self {
        Self {
            music: None,
            at: None,
            view_width: None,
            logo: Vec::new(),
            logo_colors: Vec::new(),
            logo_heights: Vec::new(),
            drip: None,
            logo_cracks: Vec::new(),
            logo_model: None,
            logo_bob: 0.0,
            props: Vec::new(),
            flyby: None,
            flyby_every: 0.0,
            flyby_altitude: 40.0,
            attract_after: 30.0,
            light_menu: false,
        }
    }
}

/// A model placed in the title scene, on the ground at `at`, rotated
/// `rotation` degrees clockwise seen from above (as a level's thing's), in
/// quarter turns.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PropConfig {
    /// Its model (.vvx).
    pub model: String,
    /// Where it lies on the ground, x and y in voxels.
    pub at: [f32; 2],
    /// Degrees clockwise seen from above, 0, 90, 180 or 270.
    #[serde(default, deserialize_with = "quarter_degrees")]
    pub rotation: i32,
}

impl PropConfig {
    /// The way its model looks: its rotation as a quarter turn.
    pub fn look(&self) -> glam::IVec2 {
        [glam::IVec2::Y, glam::IVec2::X, glam::IVec2::NEG_Y, glam::IVec2::NEG_X][(self.rotation / 90).rem_euclid(4) as usize]
    }
}

/// The range a number of `conf.lua` must lie in (`RANGES`), from `low` to
/// `high`; an end that is `open` is not in it (`above 0`).
pub struct Bound {
    /// The setting's path under `c` (`camera.view_width`): `[]` each of a
    /// list (`light.sky[]`, `world.blocks.backdrops[].scale`), `[n]` its
    /// n-th from 1, `*` each entry of a table by name (`items.*.stack`).
    pub key: &'static str,
    pub low: f64,
    pub high: f64,
    pub low_open: bool,
    pub high_open: bool,
}

const fn within(key: &'static str, low: f64, high: f64) -> Bound {
    Bound { key, low, high, low_open: false, high_open: false }
}

/// Above `low` (not it), up to `high`.
const fn above(key: &'static str, low: f64, high: f64) -> Bound {
    Bound { key, low, high, low_open: true, high_open: false }
}

impl Bound {
    /// The range in words: "from 0 to 1", "above 0, up to 65536".
    pub fn words(&self) -> String {
        match (self.low_open, self.high_open) {
            (false, false) => format!("from {} to {}", self.low, self.high),
            (true, false) => format!("above {}, up to {}", self.low, self.high),
            (false, true) => format!("from {} to below {}", self.low, self.high),
            (true, true) => format!("above {} and below {}", self.low, self.high),
        }
    }

    pub fn holds(&self, v: f64) -> bool {
        let low = if self.low_open { v > self.low } else { v >= self.low };
        let high = if self.high_open { v < self.high } else { v <= self.high };
        low && high
    }
}

/// The longest a length of the world may be, in voxels (`world::MOST_SIDE`).
const SIDE: f64 = crate::world::MOST_SIDE as f64;

/// Every number of `conf.lua` that has a range, and the range: checked as
/// the settings are read (`check_ranges`), shown by `voxcade api` and
/// HELP's reference beside each setting. A number left out is one of
/// `FREE` (test `every_number_of_conf_lua_has_its_range`).
pub const RANGES: &[Bound] = &[
    within("world.size[]", 1.0, SIDE),
    within("world.blocks.margin", 0.0, 4096.0),
    within("world.blocks.below", 0.0, 4096.0),
    within("world.blocks.backdrops[].behind", 0.0, SIDE),
    within("world.blocks.backdrops[].scale", 1.0, 64.0),
    within("world.blocks.backdrops[].z", -4096.0, SIDE),
    within("physics.gravity", 0.0, 100_000.0),
    within("physics.max_fall", 0.0, 100_000.0),
    within("physics.step", 0.0, 256.0),
    above("physics.body_width", 0.0, 256.0),
    within("physics.lost_below", 0.0, SIDE),
    within("player.speed", 0.0, 100_000.0),
    within("player.accel", 0.0, 1_000_000.0),
    within("player.air_accel", 0.0, 1_000_000.0),
    within("player.jump", 0.0, 10_000.0),
    above("title_screen.view_width", 0.0, SIDE),
    within("title_screen.logo_heights[]", 1.0, 1000.0),
    within("title_screen.drip", 0.0, 99.0),
    within("title_screen.logo_cracks[]", 0.0, 100.0),
    within("title_screen.logo_bob", 0.0, 100.0),
    within("title_screen.flyby_every", 0.0, 3600.0),
    within("title_screen.flyby_altitude", 0.0, SIDE),
    within("title_screen.attract_after", 0.0, 3600.0),
    within("camera.pitch", -89.0, 90.0),
    within("camera.yaw", -360.0, 360.0),
    within("camera.fov", 10.0, 100.0),
    above("camera.view_width", 0.0, SIDE),
    above("camera.view_height", 0.0, SIDE),
    above("camera.view_at_least[]", 0.0, SIDE),
    above("camera.follow", 0.0, 1.0),
    within("camera.follow_height.below", 0.0, 4096.0),
    within("camera.follow_height.above", 0.0, 4096.0),
    above("camera.hold[]", 0.0, SIDE),
    within("camera.aspect", 0.25, 4.0),
    within("light.sun[]", -1000.0, 1000.0),
    Bound { key: "light.sun[3]", low: -1000.0, high: 0.0, low_open: false, high_open: true },
    within("light.sun_color[]", 0.0, 1.0),
    within("light.ambient", 0.0, 1.0),
    within("light.shadow", 0.0, 1.0),
    within("light.sky[]", 0.0, 1.0),
    within("light.reflection_horizon[]", 0.0, 1.0),
    within("light.reflection[]", 0.0, 1.0),
    within("light.metal_sweep", 0.0, 10.0),
    within("light.edge_tolerance", 0.0, 16.0),
    within("debris.settle", 0.0, 1.0),
    within("debris.max_particles", 0.0, 1_000_000.0),
    within("debris.gore", 0.0, 1.0),
    within("transition.fade", 0.0, 10.0),
    within("transition.go_to", 0.0, 10.0),
    within("transition.restart", 0.0, 10.0),
    within("transition.won", 0.0, 10.0),
    within("transition.lost", 0.0, 10.0),
    within("effects.glow", 0.0, 10.0),
    within("effects.lights", 0.0, 10.0),
    within("effects.haze", 0.0, 1.0),
    within("effects.haze_color[]", 0.0, 1.0),
    within("effects.tilt_shift", 0.0, 1.0),
    within("effects.screen_flash", 0.0, 1.0),
    within("effects.hitstop", 0.0, 60.0),
    within("effects.outline", 0.0, 8.0),
    within("effects.outline_color[]", 0.0, 1.0),
    above("ui.height", 0.0, 4096.0),
    within("ui.depth", 1.0, 16.0),
    within("ui.slant", 0.0, 4.0),
    within("ui.round", 0.0, 1.0),
    within("ui.grain", 0.0, 1.0),
    within("audio.volume", 0.0, 1.0),
    within("audio.music_volume", 0.0, 1.0),
    within("audio.drive", 0.0, 12.0),
    within("audio.crossfade", 0.0, 60.0),
    within("items.*.stack", 1.0, 1_000_000.0),
    within("saves.slots", 0.0, 9.0),
    within("players", 1.0, MOST_PLAYERS as f64),
];

/// The numbers of `conf.lua` that may be any number of their type: a
/// place on the ground (`player.spawn`: below 0 the world's middle), an
/// order, a version, points, a quarter turn checked as it is read.
pub const FREE: &[&str] = &["player.spawn[]", "title_screen.at[]", "title_screen.props[].at[]", "title_screen.props[].rotation", "items.*.order", "saves.version", "high_scores[][]", "icons.*.colors.*[]"];

/// The range of the number at `path` (as `RANGES` writes it, `light.sun[3]`
/// before `light.sun[]`), if it has one.
pub fn bound(path: &str) -> Option<&'static Bound> {
    RANGES.iter().find(|b| b.key == path)
}

/// A number of `c` (the settings as `conf(c)` gave them back) out of its
/// range: the mistake naming it, its value and the range. A world made in
/// code that is too big to make is one too (`world::too_big`).
pub fn check_ranges(c: &toml::Value) -> std::result::Result<(), String> {
    fn number(v: &toml::Value) -> Option<f64> {
        v.as_float().or_else(|| v.as_integer().map(|i| i as f64))
    }
    // Every number at `key`'s path below `at`, with its path as written.
    fn walk<'a>(at: &'a toml::Value, parts: &[&str], said: String, out: &mut Vec<(String, &'a toml::Value)>) {
        let Some((part, rest)) = parts.split_first() else {
            out.push((said, at));
            return;
        };
        let (name, index) = match part.find('[') {
            Some(i) => (&part[..i], Some(&part[i..])),
            None => (*part, None),
        };
        let mut here = vec![];
        if name == "*" {
            for (k, v) in at.as_table().into_iter().flatten() {
                here.push((format!("{said}.{k}"), v));
            }
        } else if let Some(v) = at.get(name) {
            here.push((format!("{said}.{name}"), v));
        }
        for (said, v) in here {
            match index {
                None => walk(v, rest, said, out),
                Some("[]") => {
                    for (i, item) in v.as_array().into_iter().flatten().enumerate() {
                        walk(item, rest, format!("{said}[{}]", i + 1), out);
                    }
                }
                Some(n) => {
                    let n: usize = n.trim_matches(['[', ']']).parse().unwrap_or(0);
                    if let Some(item) = v.as_array().and_then(|a| a.get(n.wrapping_sub(1))) {
                        walk(item, rest, format!("{said}[{n}]"), out);
                    }
                }
            }
        }
    }
    for b in RANGES {
        let mut found = Vec::new();
        walk(c, &b.key.split('.').collect::<Vec<_>>(), "c".into(), &mut found);
        for (said, v) in found {
            let Some(n) = number(v) else { continue };
            if !b.holds(n) {
                let shown = if v.is_integer() { n.to_string() } else { v.to_string() };
                return Err(format!("{said} = {shown}: {}", b.words()));
            }
        }
    }
    if let Some(size) = c.get("world").and_then(|w| w.get("size")).and_then(toml::Value::as_array)
        && let [x, y, z] = size.as_slice()
        && let (Some(x), Some(y), Some(z)) = (number(x), number(y), number(z))
        && let Some(why) = crate::world::too_big(glam::I64Vec3::new(x as i64, y as i64, z as i64))
    {
        return Err(format!("c.world.size: {why}"));
    }
    Ok(())
}

/// The settings that are a colour (`color`), section and name.
pub const COLORS: [(&str, &str); 6] = [("light", "sun_color"), ("light", "sky"), ("light", "reflection_horizon"), ("light", "reflection"), ("effects", "haze_color"), ("effects", "outline_color")];

/// What a colour must be, said when it is not.
pub const COLOR_IS: &str = "a color is three numbers from 0 to 1, { 0.4, 0.6, 0.8 }, or \"#rrggbb\"";

/// A colour as three numbers from 0 to 1 (`{ 0.4, 0.6, 0.8 }`) or as
/// text, `"#rrggbb"`, as a level's file writes it (`COLORS`).
fn color<'de, D: serde::Deserializer<'de>>(d: D) -> Result<[f32; 3], D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        Numbers([f32; 3]),
        Text(String),
    }
    let wrong = || serde::de::Error::custom(COLOR_IS);
    match Written::deserialize(d).map_err(|_: D::Error| wrong())? {
        Written::Numbers(c) => Ok(c),
        Written::Text(t) => crate::grid::hex(&t).map(|c| c.map(|v| f32::from(v) / 255.0)).ok_or_else(wrong),
    }
}

/// Degrees in quarter turns: 0, 90, 180, 270 (or another multiple of 90).
fn quarter_degrees<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
    let degrees = i32::deserialize(d)?;
    if degrees % 90 != 0 {
        return Err(serde::de::Error::custom(format!("rotation = {degrees}: a title's prop stands in quarter turns, 0, 90, 180 or 270 degrees")));
    }
    Ok(degrees)
}

/// Palette indices that placed things instead of voxels in a world or a
/// prefab of a game before Voxcade 0.6.0 (`c.markers.rifleman = 240`): read
/// only for `voxcade upgrade`, which names each marker in the files
/// (`crate::scene`); a game that still sets one does not load.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(transparent)]
pub struct MarkersConfig(BTreeMap<String, u8>);

impl MarkersConfig {
    /// Whether it names no marker (since Voxcade 0.6.0 every game's: markers
    /// are palette entries of the worlds and prefabs, `crate::scene`).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The markers by their palette index: the kind's name each places
    /// (what `voxcade upgrade` moves into the files).
    pub fn by_index(&self) -> std::collections::BTreeMap<u8, String> {
        self.0.iter().map(|(name, &i)| (i, name.clone())).collect()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct CameraConfig {
    /// Degrees below the horizon. 90 is straight top-down.
    pub pitch: f32,
    /// Degrees around the vertical axis. 0 looks north (+y).
    pub yaw: f32,
    /// Vertical field of view in degrees. Higher means stronger perspective.
    pub fov: f32,
    /// Voxels visible across the screen at the player's distance,
    /// independent of resolution.
    pub view_width: f32,
    /// Voxels visible from the bottom of the screen to the top there
    /// instead: the width then follows the window's shape and `view_width`
    /// counts no more (a vertical shooter whose field must show its whole
    /// height in any window). Unset, the width is fixed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_height: Option<f32>,
    /// At least this many voxels across and high there, `{ 320, 200 }` as
    /// a screen of old had pixels: the window's shape decides which way it
    /// shows more (a wide window more across, a narrow one more high).
    /// Takes the place of `view_width` and `view_height` for the picture
    /// (the shooter's reach, part of its rules, keeps `view_width`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_at_least: Option<[f32; 2]>,
    /// Fraction of the distance to the player the camera closes per tick
    /// (only without `snap`; snapping keeps the camera on the player).
    pub follow: f32,
    /// Everything moving travels in whole voxels and the camera scrolls with
    /// the player in whole voxels (the 3D-pixel look). Off: moving things and
    /// the camera glide between voxels.
    pub snap: bool,
    /// A side view's camera follows the player up and down too, in whole
    /// voxels: its height moves only when the feet leave the band from
    /// `below` voxels under it to `above` over it, so jumps keep the view
    /// still. Unset, the height never changes (a view from above).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_height: Option<FollowHeight>,
    /// The view stops at the edges of the stage (`World::stage`; the world
    /// without one) instead of showing the dark beyond: a side view's level,
    /// or the ground a view from above shows (BunnyFarm's meadow).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stop_at_edges: bool,
    /// How far apart the players in play may be, across and deep, in
    /// voxels (`{ 240, 150 }`; `{ 240 }` across only, a side view): a move
    /// that would take the two furthest apart further is cut short, as the
    /// edge of an arcade machine's screen held them (`docs/game-model.md`,
    /// "Several players"). The simulation's, never the window's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<Vec<f32>>,
    /// How the game sets things down: on the grid in quarter turns (the
    /// default), or rotated freely (`rotates = "free"`: a level's pieces at
    /// any bearing, `rotation = 30`). The game's choice for every level, and
    /// what the level editor offers; a level turned off the quarter turns
    /// in a game on the grid does not load.
    #[serde(skip_serializing_if = "Rotates::is_quarter")]
    pub rotates: Rotates,
    /// The picture's shape, its width over its height (2.39 a film's):
    /// the scene is drawn into the largest rectangle of that shape the
    /// window holds, in its middle, black bars on the other two sides; the
    /// HUD and the menus lie inside it too. Unset: the window's shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect: Option<f32>,
}

/// How a game sets things down (`c.camera.rotates`).
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Rotates {
    /// On the grid: the four ways, in quarter turns.
    #[default]
    Quarter,
    /// Any bearing.
    Free,
}

impl Rotates {
    fn is_quarter(&self) -> bool {
        *self == Rotates::Quarter
    }
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self { pitch: 35.0, yaw: 0.0, fov: 40.0, view_width: 200.0, view_height: None, view_at_least: None, follow: 0.2, snap: true, follow_height: None, stop_at_edges: false, hold: None, rotates: Rotates::Quarter, aspect: None }
    }
}

impl CameraConfig {
    /// The hold across and deep (`hold`), each `f32::INFINITY` where unset.
    pub fn hold_span(&self) -> Option<[f32; 2]> {
        let h = self.hold.as_ref()?;
        Some([h.first().copied().unwrap_or(f32::INFINITY), h.get(1).copied().unwrap_or(f32::INFINITY)])
    }

    /// Shows `width` voxels across, whatever the game asked for (the
    /// title's own width, a model's sheet, the glide between them).
    pub fn show_width(&mut self, width: f32) {
        (self.view_width, self.view_height, self.view_at_least) = (width, None, None);
    }
}

/// How far the feet may be under and over the camera's height before it
/// follows them (`CameraConfig::follow_height`), in voxels.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FollowHeight {
    /// How far the feet may go under the camera's height, in voxels.
    pub below: f32,
    /// How far over it.
    pub above: f32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct LightConfig {
    /// Direction the sunlight travels (towards the ground).
    pub sun: [f32; 3],
    /// The sunlight's colour and strength, 0..1 per channel: white unless
    /// set (warm at dusk, a pale blue for the moon). A game's
    /// `frame:daylight` sets it for one picture.
    #[serde(deserialize_with = "color")]
    pub sun_color: [f32; 3],
    /// Light that reaches every face, 0..1.
    pub ambient: f32,
    /// How much a shadow darkens sunlit faces, 0..1.
    pub shadow: f32,
    /// Background colour, 0..1 per channel.
    #[serde(deserialize_with = "color")]
    pub sky: [f32; 3],
    /// The sky metal mirrors: its colour at the horizon and overhead, 0..1
    /// per channel (below the horizon it mirrors the ground, in the haze's
    /// colour).
    #[serde(deserialize_with = "color")]
    pub reflection_horizon: [f32; 3],
    /// The sky metal mirrors overhead, 0..1 per channel.
    #[serde(deserialize_with = "color")]
    pub reflection: [f32; 3],
    /// A streak of light sweeping over metal now and then, this bright
    /// (0: none): metal reads as metal even where the sun cannot glint in it.
    pub metal_sweep: f32,
    /// How shadow edges look: "smooth" (soft edges across the voxels),
    /// "voxel" (every voxel face is either lit or in shadow, edges step
    /// along the grid) or "voxel_soft" (as voxel, with half-shadowed faces
    /// in between).
    pub shadow_style: ShadowStyle,
    /// How models (actors, props) are lit: "faces" (every face of a voxel
    /// by its own direction), "smooth" (a whole voxel by the direction of
    /// the surface around it, so round shapes shade softly like pixel art)
    /// or "edges" (smooth, but where two flat surfaces meet the faces keep
    /// their own light, so hard edges stay hard).
    pub model_shading: ModelShading,
    /// Models shaded otherwise, by file (relative to the game folder), e.g.
    /// `{ ["huey.vvx"] = "faces" }` for a machine among smooth figures.
    pub model_shading_for: BTreeMap<String, ModelShading>,
    /// With "edges": how far, in voxels, a voxel's surroundings may stray
    /// from one smooth surface before it counts as an edge.
    pub edge_tolerance: f32,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelShading {
    #[default]
    Faces,
    Smooth,
    Edges,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShadowStyle {
    #[default]
    Smooth,
    Voxel,
    VoxelSoft,
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            sun: [0.45, 0.6, -1.0],
            sun_color: [1.0, 1.0, 1.0],
            ambient: 0.45,
            shadow: 0.55,
            sky: [0.06, 0.07, 0.1],
            reflection_horizon: [1.0, 0.95, 0.86],
            reflection: [0.48, 0.56, 0.7],
            metal_sweep: 0.0,
            shadow_style: ShadowStyle::Smooth,
            model_shading: ModelShading::Faces,
            model_shading_for: BTreeMap::new(),
            edge_tolerance: 0.5,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct DebrisConfig {
    /// Chance that a landed debris voxel becomes part of the world again.
    pub settle: f32,
    /// Upper bound for flying debris and sparks.
    pub max_particles: usize,
    /// Share of a body's voxels that burst blood-red when it dies, 0..1
    /// (0 keeps their own colours, say for robots).
    pub gore: f32,
}

impl Default for DebrisConfig {
    fn default() -> Self {
        Self { settle: 0.85, max_particles: 30_000, gore: 0.0 }
    }
}

/// What makes the picture livelier than flat voxels: glow, light from
/// flashes and explosions, haze, blur at the screen edges, a white flash on
/// big blasts, and the game holding still for a moment when an enemy dies.
/// How the picture goes from one level to the next (`docs/game-model.md`,
/// "Leaving a level"): it fades out to a colour and in again, the
/// picture's alone (no ticks run meanwhile, so replays do not see it; tools
/// without a window skip it).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct TransitionConfig {
    /// Seconds out and seconds in; 0: a cut.
    pub fade: f32,
    /// The colour faded through, "#rrggbb".
    pub color: String,
    /// A length of its own for a reason the level is left (a puzzle
    /// game's quick restart: `restart = 0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go_to: Option<f32>,
    /// Seconds of the fade when the level is entered afresh.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<f32>,
    /// Seconds of the fade when the run is won.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub won: Option<f32>,
    /// Seconds of the fade when the run is lost.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lost: Option<f32>,
}

impl Default for TransitionConfig {
    fn default() -> Self {
        Self { fade: 0.3, color: "#000000".into(), go_to: None, restart: None, won: None, lost: None }
    }
}

impl TransitionConfig {
    /// The fade's seconds out (and in) when the level is left for `why`.
    pub fn length(&self, why: &str) -> f32 {
        let own = match why {
            "go_to" => self.go_to,
            "restart" => self.restart,
            "won" => self.won,
            "lost" => self.lost,
            _ => None,
        };
        own.unwrap_or(self.fade).max(0.0)
    }

    /// The colour as 0..1 (black when it is no "#rrggbb").
    pub fn rgb(&self) -> [f32; 3] {
        let hex = self.color.trim_start_matches('#');
        let c = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0) as f32 / 255.0;
        if hex.len() == 6 { [c(0), c(2), c(4)] } else { [0.0; 3] }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct EffectsConfig {
    /// How strongly bright things (muzzle flashes, fire, tracers, sparks)
    /// glow into their surroundings; 0 turns it off.
    pub glow: f32,
    /// How strongly flashes and explosions light up what is around them.
    pub lights: f32,
    /// Haze towards the far edge of the view, 0..1, and its colour (0..1).
    pub haze: f32,
    /// The haze's colour, 0..1 per channel.
    #[serde(deserialize_with = "color")]
    pub haze_color: [f32; 3],
    /// Blur at the top and bottom of the screen, a miniature look, 0..1.
    pub tilt_shift: f32,
    /// How bright the white flash over the screen on big explosions gets, 0..1.
    pub screen_flash: f32,
    /// Ticks the game holds still when an enemy dies.
    pub hitstop: u32,
    /// A dark line round the models (figures, props), as round a sprite:
    /// how many voxels thick (0: none; 1 a voxel, as a sprite's pixel),
    /// and its colour (0..1). Hidden behind what stands in front, drawn
    /// where one part of a model stands before another too.
    pub outline: f32,
    /// The outline's colour, 0..1 per channel.
    #[serde(deserialize_with = "color")]
    pub outline_color: [f32; 3],
}

impl Default for EffectsConfig {
    fn default() -> Self {
        Self {
            glow: 1.0,
            lights: 1.0,
            haze: 0.0,
            haze_color: [0.8, 0.75, 0.65],
            tilt_shift: 0.0,
            screen_flash: 1.0,
            hitstop: 2,
            outline: 0.0,
            outline_color: [0.07, 0.05, 0.09],
        }
    }
}

/// The HUD, built from voxels in its own scene over the world.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    /// UI voxels from the bottom of the screen to the top, whatever the
    /// resolution (180: a 5x7 letter is a 25th of the screen).
    pub height: f32,
    /// How many voxels deep letters and icons are.
    pub depth: i32,
    /// How far the voxels' sides show at the top and bottom edges of the
    /// screen, in voxels per voxel of depth (the UI is seen in perspective
    /// from in front of the middle, so the sides facing the middle show).
    pub slant: f32,
    /// A glyph atlas (PNG) for big letters: the logo, headings, messages.
    pub display_font: Option<String>,
    /// Big letters' front edges are rounded off with this radius, a share
    /// of their capital height (0: square).
    pub round: f32,
    /// How much the colour of big letters varies voxel to voxel, 0..1.
    pub grain: f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { height: 180.0, depth: 2, slant: 0.45, display_font: None, round: 0.0, grain: 0.0 }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct AudioConfig {
    /// Loudness of all sounds, 0..1 (more is held under the ceiling by the
    /// limiter, `audio::limiter`).
    pub volume: f32,
    /// Music as MML text (see `audio::music`), relative to the game folder:
    /// one file, or a list the keys 1..9 switch between while playing.
    #[serde(skip_serializing_if = "Songs::is_none")]
    pub music: Songs,
    /// Loudness of the music, 0..1, times the player's music volume.
    pub music_volume: f32,
    /// How hard the game's sound is pushed into the limiter, in decibels:
    /// 0 as it is, more makes it denser and louder; music you listen to for
    /// long tires sooner when dense. 0 to 12. The limiter's drive (the JUST
    /// mastering stage's own input gain), on sounds and music together,
    /// after the player's volumes; the game's choice, not the player's.
    pub drive: f32,
    /// Seconds one song crossfades into the next when the game's code
    /// changes the song (`music:play`; one call may say otherwise): the
    /// old fading out as the new fades in, equal in power. 0: the old ends
    /// and the new begins, a level's fade between them silent.
    pub crossfade: f32,
    /// No more: which song plays when is the game's code (`music:play`),
    /// never a kit's. Kept to say so to a `conf.lua` that sets it.
    #[serde(skip_serializing)]
    pub boss_music: Option<String>,
}

/// One song or several.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
#[serde(untagged)]
pub enum Songs {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl Songs {
    fn is_none(&self) -> bool {
        *self == Songs::None
    }

    pub fn files(&self) -> Vec<&str> {
        match self {
            Songs::None => Vec::new(),
            Songs::One(file) => vec![file.as_str()],
            Songs::Many(files) => files.iter().map(String::as_str).collect(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { volume: 0.8, music: Songs::None, music_volume: 0.5, drive: 0.0, crossfade: 0.0, boss_music: None }
    }
}

/// A line of `conf.lua` (`voxcade kinds`): `c.SECTION.NAME = { key = value,
/// ... }`, the values as Lua writes them.
pub fn conf_line(section: &str, name: &str, keys: &[(&str, String)]) -> String {
    let keys: Vec<String> = keys.iter().map(|(k, v)| format!("{k} = {v}")).collect();
    format!("c.{section}.{name} = {{ {} }}", keys.join(", "))
}

/// A list of numbers as Lua writes it: `{ 0, 12, 1.5 }`.
pub fn lua_list(values: &[f32]) -> String {
    format!("{{ {} }}", values.iter().map(f32::to_string).collect::<Vec<_>>().join(", "))
}

/// Tables that may build on one another (`like = "other"`: the other's
/// keys, then its own), each with what it is like filled in and `like`
/// gone. `section` names them in errors (`things`, `weapons`).
pub fn resolve_like(section: &str, tables: &BTreeMap<String, toml::Table>) -> Result<BTreeMap<String, toml::Table>> {
    fn fill(section: &str, name: &str, tables: &BTreeMap<String, toml::Table>, path: &mut Vec<String>) -> Result<toml::Table> {
        let own = &tables[name];
        let Some(like) = own.get("like") else { return Ok(own.clone()) };
        let like = like.as_str().with_context(|| format!("c.{section}.{name}.like: the name of another"))?;
        if path.iter().any(|p| p == like) || like == name {
            anyhow::bail!("c.{section}.{name}.like = \"{like}\": goes round in a circle ({} -> {like})", path.join(" -> "));
        }
        if !tables.contains_key(like) {
            anyhow::bail!("c.{section}.{name}.like = \"{like}\": there is no c.{section}.{like}");
        }
        path.push(like.to_string());
        let mut table = fill(section, like, tables, path)?;
        for (key, value) in own {
            if key != "like" {
                table.insert(key.clone(), value.clone());
            }
        }
        Ok(table)
    }
    tables.keys().map(|name| Ok((name.clone(), fill(section, name, tables, &mut vec![name.clone()])?))).collect()
}

impl GameConfig {
    /// The settings in `new` that only take effect when the game starts
    /// (the kit, the actions and their keys, the level and its markers,
    /// where the player starts) and differ from these, by name.
    pub fn restart_only(&self, new: &GameConfig) -> Vec<&'static str> {
        let same = |a: &dyn erased::Ser, b: &dyn erased::Ser| a.text() == b.text();
        let mut changed = Vec::new();
        for (name, differs) in [
            ("kit", !same(&self.kit, &new.kit)),
            ("actions", !same(&self.actions, &new.actions)),
            ("keys", self.keys != new.keys),
            ("pad", self.pad != new.pad),
            ("world.file", self.world.file != new.world.file),
            ("world.size", self.world.size != new.world.size),
            ("world.blocks", !same(&self.world.blocks, &new.world.blocks)),
            ("markers", !same(&self.markers, &new.markers)),
            ("player.spawn", self.player.spawn != new.player.spawn),
            ("player.kind", self.player.kind != new.player.kind),
            ("players", self.players != new.players),
            ("camera.rotates", self.camera.rotates != new.camera.rotates),
        ] {
            if differs {
                changed.push(name);
            }
        }
        changed
    }

    /// Keeps this config's start-only settings in `new` (they wait for a
    /// restart, `restart_only`).
    pub fn keep_for_restart(&self, new: &mut GameConfig) {
        new.kit = self.kit.clone();
        new.actions = self.actions.clone();
        new.keys = self.keys.clone();
        new.pad = self.pad.clone();
        new.world.file = self.world.file.clone();
        new.world.size = self.world.size;
        new.world.blocks = self.world.blocks.clone();
        new.markers = self.markers.clone();
        new.player.spawn = self.player.spawn;
        new.player.kind = self.player.kind.clone();
        new.players = self.players;
        new.camera.rotates = self.camera.rotates;
    }
}

/// Settings compared by what they write out (`GameConfig::restart_only`).
mod erased {
    pub trait Ser {
        fn text(&self) -> String;
    }

    impl<T: serde::Serialize> Ser for T {
        fn text(&self) -> String {
            serde_json::to_string(self).unwrap_or_default()
        }
    }
}
