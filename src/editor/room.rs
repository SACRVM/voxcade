//! The level's pieces (`grid::text`): models and lights set down freely in
//! voxels, places a player arrives at and named areas; all a level
//! without blocks has, and what a level of blocks may hold besides. One is
//! chosen (in the list or the view) and changed: dragged over the floor,
//! nudged a voxel, raised, turned, copied, removed, renamed, its light
//! switched on and tuned. A new one hangs on the mouse until a click sets
//! it down. Every change is a step back and is saved at once, as the
//! blocks' are.

use glam::{IVec2, IVec3, Quat, Vec3};

use super::{Editor, Layer, Scope, Step};
use crate::editor::handles::{self, Drag, Grip, Handles};
use crate::grid::{Area, Facing, GridLevel, PIECE, Place, Thing, ThingLight, Way};

/// Something set down in the level, by its place in its list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Thing(usize),
    Place(usize),
    Area(usize),
}

/// What hangs on the mouse to be set down: a thing (a piece, a light
/// alone), a place or an area.
#[derive(Clone, Debug, PartialEq)]
pub enum Hanging {
    Thing(Thing),
    Place(Place),
    Area(Area),
}

/// A number set from the panels or the keys: where the chosen one stands
/// (x, y, z), its light's place in its model and its radius, its turn, an
/// area's size, the level's size; the arrow keys' nudges; a light's
/// colour being picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    At(usize),
    /// A light of the chosen piece (by its place among its lights): where
    /// it shines from (an axis), its radius, its colour.
    LightAt(usize, usize),
    Radius(usize),
    Colour(usize),
    /// The level's daylight: where the sun stands (degrees round from
    /// north, clockwise seen from above) and how high (degrees over the
    /// horizon), the light every face gets and how dark shadows are (in
    /// hundredths), the sun's colour and the sky's.
    SunFrom,
    SunHeight,
    Ambient,
    Shadow,
    DayColour,
    /// A piece's turn round (its bearing), its pitch and its roll.
    Turn,
    Pitch,
    Roll,
    /// How high the chosen one's lowest voxel stands (its `at`'s z, but
    /// for a piece laid down, whose `at` lies under it).
    Base,
    Size(usize),
    LevelSize(usize),
    Nudge,
}

/// Colours for a light to choose from at once, warm to cool, each with
/// what it is like.
pub const LIGHT_COLOURS: [(&str, &str); 6] = [("#ffd9a0", "a lamp"), ("#ffb060", "a candle"), ("#ffffff", "white"), ("#a0b8ff", "the moon"), ("#a0ffc0", "green"), ("#ff8070", "red")];

/// A new light's radius (a piece's, a light alone's) and colour, a new
/// area's size.
const PIECE_RADIUS: f64 = 60.0;
const LIGHT_RADIUS: f64 = 80.0;
const NEW_COLOUR: &str = "#ffd9a0";
const NEW_AREA: [i32; 3] = [40, 30, 40];

/// How close to a quarter turn an angle is taken as it (degrees): typed,
/// and dragged on a ring.
pub const SNAP: f64 = 3.0;
pub const RING_SNAP: f64 = 5.0;

/// How big the handles look on the chosen one: a share of the view's
/// height, whatever its size.
pub const HANDLE_SHARE: f32 = 0.14;

/// The bearing a thing faces (degrees clockwise seen from above, north 0;
/// south when the file says none).
pub fn bearing_of(t: &Thing) -> f64 {
    t.facing.map_or(180.0, Facing::bearing)
}

/// How a game draws a thing: the quarter way it looks and the free turn
/// after it (none on the four ways, untipped), as a level's piece is drawn
/// (`LuaGame::draw`, `session::facing`, then its pitch and roll).
pub fn turn_of(t: &Thing) -> (IVec2, Quat) {
    let tip = crate::session::turning(0.0, t.pitch as f32, t.roll as f32);
    let (look, round) = match t.facing.unwrap_or(Facing::Way(Way::South)) {
        Facing::Way(w) => (IVec2::from(w.pair()), Quat::IDENTITY),
        Facing::Turn(b) => {
            let look = Way::nearest(b);
            let way = (b - look.bearing()).to_radians() as f32;
            let way = (way + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            (IVec2::from(look.pair()), if way.abs() < 1e-4 { Quat::IDENTITY } else { Quat::from_rotation_z(-way) })
        }
    };
    (look, if tip == Quat::IDENTITY { round } else { round * tip })
}

/// A thing's turn as a whole: clockwise seen from above by its bearing,
/// tipped by its pitch and roll (`session::turning`).
pub fn rotation_of(t: &Thing) -> Quat {
    turning(bearing_of(t), t.pitch, t.roll)
}

/// `session::turning` of degrees as the file has them.
pub fn turning(bearing: f64, pitch: f64, roll: f64) -> Quat {
    let b = Quat::from_rotation_z(-(bearing as f32).to_radians());
    if pitch == 0.0 && roll == 0.0 { b } else { b * crate::session::turning(0.0, pitch as f32, roll as f32) }
}

/// Where a thing turns (`render::turn_centre`), from its pivot cell's low
/// corner: the middle of the voxel at half its model's height (`size`;
/// unknown or none, its feet).
pub fn centre_in(size: Option<IVec3>) -> Vec3 {
    size.map_or(Vec3::new(0.5, 0.5, 0.0), crate::render::turn_centre)
}

/// Where a point of a thing's model (from its pivot cell's low corner, in
/// the model's own voxels) lies in the level, the thing turned and tipped.
pub fn model_point(t: &Thing, size: Option<IVec3>, p: Vec3) -> Vec3 {
    let about = centre_in(size);
    IVec3::from(t.at).as_vec3() + about + rotation_of(t) * (p - about)
}

/// Where a thing's light `l` shines from in the level's voxels: the middle
/// of its voxel in the model (its offset from the pivot's cell), turned
/// and tipped with the piece (its model `size`), as the game puts it.
pub fn light_spot(t: &Thing, size: Option<IVec3>, l: &ThingLight) -> Vec3 {
    let offset = IVec3::from(l.at.unwrap_or_default()).as_vec3();
    if t.pitch == 0.0 && t.roll == 0.0 {
        return IVec3::from(t.at).as_vec3() + Vec3::splat(0.5) + rotation_of(t) * offset;
    }
    model_point(t, size, offset + Vec3::splat(0.5))
}

/// Where each of a thing's lights shines from, with the light.
pub fn light_spots(t: &Thing, size: Option<IVec3>) -> impl Iterator<Item = (Vec3, &ThingLight)> {
    t.lights.iter().map(move |l| (light_spot(t, size, l), l))
}

/// How far over its `at` a model of `size` turned by `rot` reaches down
/// (its box's lowest corner; 0 standing): what keeps a piece laid down on
/// the floor it stood on.
pub fn low_of(size: IVec3, rot: Quat) -> f32 {
    let about = crate::render::turn_centre(size);
    let lo = -IVec3::new(size.x / 2, size.y / 2, 0).as_vec3();
    let hi = lo + size.as_vec3();
    (0..8)
        .map(|k| {
            let c = Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
            about.z + (rot * (c - about)).z
        })
        .fold(f32::MAX, f32::min)
}

/// A handle being dragged on the chosen one (or its light): the handles as
/// they were at the press, what the drag measured, and the item as it was.
#[derive(Clone, Debug)]
pub struct Gripped {
    pub item: Item,
    pub light: Option<usize>,
    pub handles: Handles,
    pub drag: Drag,
    was: Was,
}

/// An item as it was when a handle was pressed.
#[derive(Clone, Debug)]
enum Was {
    Thing(Thing),
    Place(Place),
    Area(Area),
}

/// The way the sunlight travels for a sun standing `from` degrees round
/// (clockwise seen from above, north 0) and `height` degrees over the
/// horizon, as a level's `[light]` writes it (three places after the point).
pub fn sun_way(from: f64, height: f64) -> [f64; 3] {
    let (b, h) = (from.to_radians(), height.clamp(1.0, 90.0).to_radians());
    let round = |v: f64| (v * 1000.0).round() / 1000.0 + 0.0;
    [round(-h.cos() * b.sin()), round(-h.cos() * b.cos()), round(-h.sin())]
}

/// Where a sun travelling `way` stands: degrees round from north and over
/// the horizon (`sun_way` back).
pub fn sun_stands(way: [f64; 3]) -> (f64, f64) {
    let [x, y, z] = way;
    let flat = (x * x + y * y).sqrt();
    let from = if flat < 1e-9 { 0.0 } else { (-x).atan2(-y).to_degrees().rem_euclid(360.0) };
    (((from * 10.0).round() / 10.0) % 360.0, ((-z).atan2(flat).to_degrees() * 10.0).round() / 10.0)
}

/// Where a ray (from `origin` along `dir`, in voxels) meets the level at
/// height `z`: the voxel there, None if it looks away from it.
pub fn plane_spot(origin: Vec3, dir: Vec3, z: i32) -> Option<IVec3> {
    if dir.z.abs() < 1e-6 {
        return None;
    }
    let t = (z as f32 - origin.z) / dir.z;
    // (Behind the eye, or not a number: nowhere.)
    if t.is_nan() || t <= 0.0 {
        return None;
    }
    let p = origin + dir * t;
    Some(IVec3::new(p.x.floor() as i32, p.y.floor() as i32, z))
}

/// A name of letters, digits and `_` made of a file's or a word's: what a
/// new piece is called after its model.
fn plain(word: &str) -> String {
    let out: String = word.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let out = out.trim_matches('_').to_string();
    if out.is_empty() { "piece".into() } else { out }
}

impl Editor {
    /// Whether the tools work on the level's pieces.
    pub fn among_pieces(&self) -> bool {
        self.scope == Scope::Level && self.layer == Layer::Pieces
    }

    /// Whether an item is in the level.
    pub fn exists(&self, item: Item) -> bool {
        match item {
            Item::Thing(i) => i < self.level.things.len(),
            Item::Place(i) => i < self.level.places.len(),
            Item::Area(i) => i < self.level.areas.len(),
        }
    }

    /// The name an item has in the file, if it has one.
    pub fn name_of(&self, item: Item) -> Option<&str> {
        match item {
            Item::Thing(i) => self.level.things.get(i)?.name.as_deref(),
            Item::Place(i) => Some(&self.level.places.get(i)?.name),
            Item::Area(i) => Some(&self.level.areas.get(i)?.name),
        }
    }

    /// How the list calls an item: its name, or (unnamed, `false`) what it
    /// is: its kind, its model's file, a light.
    pub fn label_of(&self, item: Item) -> (String, bool) {
        if let Some(name) = self.name_of(item) {
            return (name.to_string(), true);
        }
        let Item::Thing(i) = item else { return (String::new(), false) };
        let t = &self.level.things[i];
        let what = match (&t.model, t.is_piece()) {
            (Some(model), _) => std::path::Path::new(model).file_stem().map_or(model.clone(), |s| s.to_string_lossy().into_owned()),
            (None, true) => "a light".into(),
            (None, false) => t.kind.clone(),
        };
        (what, false)
    }

    /// Where an item stands (an area: its low corner), in voxels from the
    /// level's low corner.
    pub fn at_of(&self, item: Item) -> Option<[i32; 3]> {
        match item {
            Item::Thing(i) => self.level.things.get(i).map(|t| t.at),
            Item::Place(i) => self.level.places.get(i).map(|p| p.at),
            Item::Area(i) => self.level.areas.get(i).map(|a| a.at),
        }
    }

    /// The item called `name`: a thing, else a place, else an area.
    pub fn find(&self, name: &str) -> Option<Item> {
        let thing = self.level.things.iter().position(|t| t.name.as_deref() == Some(name)).map(Item::Thing);
        thing.or_else(|| self.level.places.iter().position(|p| p.name == name).map(Item::Place)).or_else(|| self.level.areas.iter().position(|a| a.name == name).map(Item::Area))
    }

    /// The chosen thing, if a thing is chosen.
    pub fn chosen_thing(&self) -> Option<&Thing> {
        match self.chosen {
            Some(Item::Thing(i)) => self.level.things.get(i),
            _ => None,
        }
    }

    /// Chooses an item (None: nothing).
    pub fn choose_item(&mut self, item: Option<Item>) {
        self.release();
        self.chosen = item.filter(|&i| self.exists(i));
        self.chosen_light = None;
        self.tuned = None;
    }

    /// Chooses a light of the chosen piece (by its place among its
    /// lights; None: the piece again): its handles then move it in the
    /// model.
    pub fn choose_light(&mut self, k: Option<usize>) {
        self.chosen_light = k.filter(|&k| self.chosen_thing().is_some_and(|t| t.model.is_some() && k < t.lights.len()));
    }

    /// The size of a thing's model, if it is known (`sizes`).
    pub fn size_of(&self, t: &Thing) -> Option<IVec3> {
        t.model.as_ref().and_then(|m| self.sizes.get(m)).copied()
    }

    /// The lowest `at` an item may have in z: its lowest voxel on the
    /// level's floor (a piece laid down has its pivot under it).
    fn lowest(&self, item: Item) -> i32 {
        let Item::Thing(i) = item else { return 0 };
        let t = &self.level.things[i];
        match self.size_of(t) {
            Some(size) if t.pitch != 0.0 || t.roll != 0.0 => -(low_of(size, rotation_of(t)).round() as i32),
            _ => 0,
        }
    }

    /// How high an item's lowest voxel stands: its `at`'s z, less what a
    /// piece laid down reaches under it (`lowest`).
    pub fn base_of(&self, item: Item) -> Option<i32> {
        Some(self.at_of(item)?[2] - self.lowest(item))
    }

    /// A spot for an item kept inside the level's voxels.
    fn inside_for(&self, item: Item, at: IVec3) -> IVec3 {
        let hi = (self.level.voxels() - 1).max(IVec3::ZERO);
        let lo = IVec3::new(0, 0, self.lowest(item).min(hi.z));
        at.clamp(lo, hi)
    }

    /// What is chosen dropped if a step took it away.
    pub(super) fn keep_choice(&mut self) {
        if self.chosen.is_some_and(|i| !self.exists(i)) {
            self.chosen = None;
        }
    }

    /// A change to the level as one step, saved at once; `tune`: a number
    /// set again (dragged, typed, a key held) joins the step that set it
    /// last for the same item.
    fn change(&mut self, tune: Option<Field>, f: impl FnOnce(&mut GridLevel)) {
        self.release();
        let before = self.level_state();
        f(&mut self.level);
        let after = self.level_state();
        if after == before {
            return;
        }
        let key = tune.map(|t| (self.chosen, t));
        if key.is_some()
            && key == self.tuned
            && let Some(Step::Level(..)) = self.past.last()
            && let Some(Step::Level(first, _)) = self.past.pop()
        {
            // The step that set it last, now ending here.
            self.commit(Step::Level(first, after));
            return;
        }
        let step = Step::Level(before, after);
        self.commit(step);
        self.tuned = key;
    }

    /// A spot kept inside the level's voxels.
    fn inside(&self, at: IVec3) -> IVec3 {
        at.clamp(IVec3::ZERO, (self.level.voxels() - 1).max(IVec3::ZERO))
    }

    /// Moves an item to `at` as it is (no step: a drag's).
    fn put(&mut self, item: Item, at: [i32; 3]) {
        match item {
            Item::Thing(i) => self.level.things[i].at = at,
            Item::Place(i) => self.level.places[i].at = at,
            Item::Area(i) => self.level.areas[i].at = at,
        }
    }

    /// Names taken among an item's sort (things, places, areas), but the
    /// one at `except`.
    fn taken(&self, sort: Item, name: &str, except: Option<usize>) -> bool {
        match sort {
            Item::Thing(_) => self.level.things.iter().enumerate().any(|(i, t)| Some(i) != except && t.name.as_deref() == Some(name)),
            Item::Place(_) => self.level.places.iter().enumerate().any(|(i, p)| Some(i) != except && p.name == name),
            Item::Area(_) => self.level.areas.iter().enumerate().any(|(i, a)| Some(i) != except && a.name == name),
        }
    }

    /// A free name of the sort after `base`: `stove`, else `stove_2` and
    /// on (a copy of `stove_2` is `stove_3`).
    fn name_for(&self, sort: Item, base: &str) -> String {
        let base = plain(base);
        if !self.taken(sort, &base, None) {
            return base;
        }
        let stem = match base.rsplit_once('_') {
            Some((stem, n)) if !stem.is_empty() && n.parse::<u32>().is_ok() => stem.to_string(),
            _ => base,
        };
        (2..).map(|n| format!("{stem}_{n}")).find(|name| !self.taken(sort, name, None)).expect("a free name")
    }

    /// Hangs something on the mouse to be set down.
    pub fn hang(&mut self, hanging: Hanging) {
        self.release();
        self.hanging = Some(hanging);
        self.hang_spot = None;
    }

    /// Lets go of what hangs on the mouse; whether something did.
    pub fn drop_hanging(&mut self) -> bool {
        self.hang_spot = None;
        self.hanging.take().is_some()
    }

    /// The spot under the mouse (on the floor or a block; None: off the
    /// level), where what hangs there would stand: a light alone some
    /// way up, an area round it.
    pub fn hang_at(&mut self, spot: Option<IVec3>) {
        self.hang_spot = spot.map(|s| self.inside(s));
        let Some(s) = self.hang_spot else { return };
        let up = (self.level.voxels().z / 2).clamp(0, 40);
        match &mut self.hanging {
            Some(Hanging::Thing(t)) => t.at = (s + IVec3::Z * if t.model.is_none() { up } else { 0 }).to_array(),
            Some(Hanging::Place(p)) => p.at = s.to_array(),
            Some(Hanging::Area(a)) => a.at = [s.x - a.size[0] / 2, s.y - a.size[1] / 2, s.z],
            None => {}
        }
    }

    /// What hangs on the mouse set down where it is, chosen; a step.
    /// Nothing without a spot under the mouse.
    pub fn set_down(&mut self) -> bool {
        let (Some(hanging), Some(_)) = (self.hanging.clone(), self.hang_spot) else { return false };
        self.drop_hanging();
        let item = match hanging {
            Hanging::Thing(t) => {
                self.change(None, |l| l.things.push(t));
                Item::Thing(self.level.things.len() - 1)
            }
            Hanging::Place(p) => {
                self.change(None, |l| l.places.push(p));
                Item::Place(self.level.places.len() - 1)
            }
            Hanging::Area(a) => {
                self.change(None, |l| l.areas.push(a));
                Item::Area(self.level.areas.len() - 1)
            }
        };
        self.chosen = Some(item);
        true
    }

    /// ADD MODEL: a piece of the model `file` (its path in the game
    /// folder) on the mouse, named after it.
    pub fn add_piece(&mut self, file: &str) {
        let stem = std::path::Path::new(file).file_stem().map_or("piece".into(), |s| s.to_string_lossy().into_owned());
        let name = self.name_for(Item::Thing(0), &stem);
        self.hang(Hanging::Thing(Thing { model: Some(file.to_string()), name: Some(name), ..Thing::new(PIECE, [0; 3]) }));
    }

    /// ADD LIGHT: a light alone on the mouse.
    pub fn add_light(&mut self) {
        let name = self.name_for(Item::Thing(0), "light");
        let light = ThingLight { at: None, radius: LIGHT_RADIUS, color: Some(NEW_COLOUR.into()) };
        self.hang(Hanging::Thing(Thing { name: Some(name), lights: vec![light], ..Thing::new(PIECE, [0; 3]) }));
    }

    /// ADD PLACE: a place on the mouse, facing the camera.
    pub fn add_place(&mut self) {
        let name = self.name_for(Item::Place(0), "place");
        self.hang(Hanging::Place(Place { name, at: [0; 3], facing: Some(Way::South.pair()), named: true, line: 0 }));
    }

    /// ADD AREA: an area on the mouse.
    pub fn add_area(&mut self) {
        let name = self.name_for(Item::Area(0), "area");
        self.hang(Hanging::Area(Area { name, at: [0; 3], size: NEW_AREA, line: 0 }));
    }

    /// COPY: the chosen one again, under a new name, on the mouse.
    pub fn copy_item(&mut self) {
        let Some(item) = self.chosen else { return };
        let hanging = match item {
            Item::Thing(i) => {
                let mut t = self.level.things[i].clone();
                t.line = 0;
                if let Some(name) = &t.name {
                    t.name = Some(self.name_for(item, name));
                }
                Hanging::Thing(t)
            }
            Item::Place(i) => {
                let p = &self.level.places[i];
                Hanging::Place(Place { name: self.name_for(item, &p.name), line: 0, ..p.clone() })
            }
            Item::Area(i) => {
                let a = &self.level.areas[i];
                Hanging::Area(Area { name: self.name_for(item, &a.name), line: 0, ..a.clone() })
            }
        };
        self.hang(hanging);
    }

    /// REMOVE: the chosen one taken out of the level; a step.
    pub fn remove_item(&mut self) {
        let Some(item) = self.chosen else { return };
        // (Lights put away are kept by the places in the list.)
        self.put_away.clear();
        self.chosen_light = None;
        self.change(None, |l| match item {
            Item::Thing(i) => drop(l.things.remove(i)),
            Item::Place(i) => drop(l.places.remove(i)),
            Item::Area(i) => drop(l.areas.remove(i)),
        });
        self.chosen = None;
    }

    /// RENAME: the item called `typed` (trimmed); refused, with why, when
    /// it is empty or another of its sort has it.
    pub fn rename(&mut self, item: Item, typed: &str) -> Result<(), String> {
        let name = typed.trim();
        if name.is_empty() {
            return Err("A name needs a letter at least.".into());
        }
        if name.contains(['"', '\\']) || name.chars().any(char::is_control) {
            return Err("A name takes no quotes or backslashes.".into());
        }
        // An area is asked for by its name in the code, level.areas.NAME:
        // a word of letters, digits and _ (not a digit first). A thing or
        // a place is asked for by a string, any name.
        let word = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.starts_with(|c: char| c.is_ascii_digit());
        if matches!(item, Item::Area(_)) && !word {
            return Err("An area's name: letters, digits and _ only (level.areas.NAME).".into());
        }
        let (i, what) = match item {
            Item::Thing(i) => (i, "thing"),
            Item::Place(i) => (i, "place"),
            Item::Area(i) => (i, "area"),
        };
        if self.taken(item, name, Some(i)) {
            return Err(format!("Another {what} is called {name}: a name is one {what}'s."));
        }
        let name = name.to_string();
        self.change(None, |l| match item {
            Item::Thing(i) => l.things[i].name = Some(name),
            Item::Place(i) => l.places[i].name = name,
            Item::Area(i) => l.areas[i].name = name,
        });
        Ok(())
    }

    /// The press of the mouse on an item (None: on nothing, which chooses
    /// nothing), the ray through it: the item chosen, and dragged from
    /// there over the level at its own height while the mouse is held.
    pub fn press_item(&mut self, under: Option<Item>, origin: Vec3, dir: Vec3) {
        self.choose_item(under);
        let Some(item) = self.chosen else { return };
        let at = IVec3::from(self.at_of(item).expect("chosen"));
        let Some(spot) = plane_spot(origin, dir, at.z) else { return };
        self.stroke = Some(Step::Level(self.level_state(), self.level_state()));
        self.dragging = Some((item, at - spot));
    }

    /// The mouse dragged on with an item: it goes along, in whole voxels,
    /// at its height (let go: one step, `release`).
    pub fn drag_item(&mut self, origin: Vec3, dir: Vec3) {
        let Some((item, grab)) = self.dragging else { return };
        let Some(at) = self.at_of(item) else { return };
        let Some(spot) = plane_spot(origin, dir, at[2]) else { return };
        let to = self.inside(spot + grab);
        self.put(item, [to.x, to.y, at[2]]);
    }

    /// The item being dragged, if one is.
    pub fn dragging(&self) -> Option<Item> {
        self.dragging.map(|(i, _)| i)
    }

    /// The chosen one moved by `by` voxels (an arrow key, a page key);
    /// nudges one after another are one step.
    pub fn nudge(&mut self, by: IVec3) {
        if let Some(Hanging::Thing(t)) = &mut self.hanging {
            // A light on the mouse goes up and down so.
            t.at[2] = (t.at[2] + by.z).max(0);
            return;
        }
        let Some(item) = self.chosen else { return };
        let to = self.inside_for(item, IVec3::from(self.at_of(item).expect("chosen")) + by);
        self.change(Some(Field::Nudge), |l| match item {
            Item::Thing(i) => l.things[i].at = to.to_array(),
            Item::Place(i) => l.places[i].at = to.to_array(),
            Item::Area(i) => l.areas[i].at = to.to_array(),
        });
    }

    /// A number of the chosen one (or of the level) set; the same number
    /// set again joins the step.
    pub fn set_field(&mut self, field: Field, v: f64) {
        let whole = v.round() as i32;
        let voxels = self.level.voxels();
        if let Field::LevelSize(axis) = field {
            // A level of blocks is as big as its grid.
            if !self.level.has_grid() {
                return self.change(Some(field), |l| l.size[axis.min(2)] = whole.clamp(1, 4096));
            }
            return;
        }
        if let Some(day) = self.day_field(field, v) {
            return self.change(Some(field), |l| l.daylight = Some(day));
        }
        let Some(item) = self.chosen else { return };
        // A piece turned round, tipped: as its ring would, kept on its floor.
        if let (Field::Turn | Field::Pitch | Field::Roll, Item::Thing(i)) = (field, item) {
            let t = &self.level.things[i];
            if field == Field::Turn && self.kind_turns(t) {
                let free = self.free_kinds.contains(&t.kind);
                let now = Some(facing_at(if free { v } else { (v / 90.0).round() * 90.0 }, free));
                return self.change(Some(field), |l| l.things[i].facing = now);
            }
            if t.model.is_none() {
                return;
            }
            let (mut b, mut p, mut r) = (bearing_of(t), t.pitch, t.roll);
            match field {
                Field::Turn => b = v,
                Field::Pitch => p = v,
                _ => r = v,
            }
            let now = self.turned(t, b, p, r, SNAP);
            return self.change(Some(field), |l| l.things[i] = now);
        }
        let low = self.lowest(item);
        if field == Field::Base {
            return self.set_field(Field::At(2), v + low as f64);
        }
        self.change(Some(field), |l| match (field, item) {
            (Field::At(a), _) => {
                let a = a.min(2);
                let v = whole.clamp(if a == 2 { low.min(voxels[2] - 1) } else { 0 }, (voxels[a] - 1).max(0));
                match item {
                    Item::Thing(i) => l.things[i].at[a] = v,
                    Item::Place(i) => l.places[i].at[a] = v,
                    Item::Area(i) => l.areas[i].at[a] = v,
                }
            }
            (Field::LightAt(k, a), Item::Thing(i)) => {
                if let Some(light) = l.things[i].lights.get_mut(k) {
                    let mut at = light.at.unwrap_or_default();
                    at[a.min(2)] = whole;
                    light.at = Some(at);
                }
            }
            (Field::Radius(k), Item::Thing(i)) => {
                if let Some(light) = l.things[i].lights.get_mut(k) {
                    light.radius = v.round().clamp(1.0, 2000.0);
                }
            }
            (Field::Size(a), Item::Area(i)) => l.areas[i].size[a.min(2)] = whole.clamp(1, 4096),
            _ => {}
        });
    }

    /// TURN: the chosen one (or what hangs on the mouse) a quarter turn
    /// clockwise seen from above (`by` -1: the other way); a piece turned
    /// off the quarter turns keeps its angle off them. A thing of a kind
    /// or an area has no way to face.
    pub fn turn_item(&mut self, by: i32) {
        let free = self.free_turns;
        let still = self.still_kinds.clone();
        let quarter = |t: &mut Thing| {
            if t.is_piece() || still.contains(&t.kind) {
                t.facing = Some(facing_at(bearing_of(t) + 90.0 * by as f64, free));
            }
        };
        let place = |p: &mut Place| {
            let [x, y] = p.facing.unwrap_or(Way::South.pair());
            p.named = p.named || p.facing.is_none();
            p.facing = Some(if by > 0 { [y, -x] } else { [-y, x] });
        };
        match &mut self.hanging {
            Some(Hanging::Thing(t)) => return quarter(t),
            Some(Hanging::Place(p)) => return place(p),
            Some(Hanging::Area(_)) => return,
            None => {}
        }
        match self.chosen {
            Some(Item::Thing(i)) => self.change(None, |l| quarter(&mut l.things[i])),
            Some(Item::Place(i)) => self.change(None, |l| place(&mut l.places[i])),
            _ => {}
        }
    }

    /// A place facing one of the four ways (FACING), named so in the file.
    pub fn face_place(&mut self, way: Way) {
        let Some(Item::Place(i)) = self.chosen else { return };
        self.change(None, |l| {
            l.places[i].facing = Some(way.pair());
            l.places[i].named = true;
        });
    }

    /// The thing `was` turned round to `bearing` and tipped to `pitch` and
    /// `roll` (degrees): on the grid (or solid) each the nearest quarter
    /// turn, else to a tenth, a quarter within `snap` taken; its lowest
    /// voxel kept at the height it had (laid down, a piece stays on its
    /// floor; its `at` goes under it).
    fn turned(&self, was: &Thing, bearing: f64, pitch: f64, roll: f64, snap: f64) -> Thing {
        let free = self.free_turns && !was.solid;
        let angle = |d: f64| {
            let d = d.rem_euclid(360.0);
            let quarter = (d / 90.0).round() * 90.0;
            let out = if !free || (d - quarter).abs() <= snap { quarter } else { (d * 10.0).round() / 10.0 };
            out.rem_euclid(360.0)
        };
        let mut t = was.clone();
        let b = angle(bearing);
        if (b - bearing_of(was)).rem_euclid(360.0) != 0.0 {
            t.facing = Some(facing_at(b, true));
        }
        (t.pitch, t.roll) = (angle(pitch), angle(roll));
        if let Some(size) = self.size_of(was) {
            let (before, after) = (low_of(size, rotation_of(was)), low_of(size, rotation_of(&t)));
            t.at[2] = was.at[2] + (before - after).round() as i32;
        }
        t
    }

    /// The chosen piece turned on by `degrees` about one of its axes as its
    /// rings are (0 pitch, its nose up; 1 roll, its right side down; 2
    /// round, clockwise seen from above): one step, kept on its floor.
    /// False (nothing done) for a turn off the quarters where the game
    /// turns on the grid, or what is chosen is no piece of a model.
    pub fn tip(&mut self, axis: usize, degrees: f64) -> bool {
        let Some(Item::Thing(i)) = self.chosen else { return false };
        let t = &self.level.things[i];
        if t.model.is_none() || ((!self.free_turns || t.solid) && degrees.rem_euclid(90.0) != 0.0) {
            return false;
        }
        let (mut b, mut p, mut r) = (bearing_of(t), t.pitch, t.roll);
        match axis {
            0 => p += degrees,
            1 => r += degrees,
            _ => b += degrees,
        }
        let now = self.turned(t, b, p, r, 0.0);
        self.change(None, |l| l.things[i] = now);
        true
    }

    /// The handles on the chosen one, sized for the view `view_proj`: on a
    /// piece arrows along the world's axes and rings about its own (pitch
    /// about its right side, roll about its forward, round about the
    /// upright: each ring changes one of its three numbers), from where it
    /// turns; on its chosen light arrows along the model's axes; a light
    /// alone and a thing of a kind arrows; a place arrows and the ring
    /// round; an area arrows and its faces. None while something hangs on
    /// the mouse.
    pub fn handles(&self, view_proj: glam::Mat4, turn: bool) -> Option<Handles> {
        let mut h = self.all_handles(view_proj)?;
        // MOVE: the arrows (and an area's faces); TURN: the rings. Never
        // both: the piece stays in view.
        if turn && self.can_turn() {
            (h.arrows, h.faces) = ([None; 3], None);
        } else {
            h.rings = [None; 3];
        }
        Some(h)
    }

    /// Whether the chosen one turns by its rings (TURN): a piece of a model
    /// (not while its light is chosen), a place.
    pub fn can_turn(&self) -> bool {
        match self.chosen {
            Some(Item::Thing(i)) => (self.level.things[i].model.is_some() && self.chosen_light.is_none()) || self.kind_turns(&self.level.things[i]),
            Some(Item::Place(_)) => true,
            _ => false,
        }
    }

    /// Every handle the chosen one has, arrows and rings alike.
    fn all_handles(&self, view_proj: glam::Mat4) -> Option<Handles> {
        let item = self.chosen.filter(|_| self.hanging.is_none())?;
        let at = IVec3::from(self.at_of(item)?).as_vec3();
        let make = |c: Vec3| Handles::at(c, handles::world_size(view_proj, c, HANDLE_SHARE));
        let world = [Some(Vec3::X), Some(Vec3::Y), Some(Vec3::Z)];
        let mut h;
        match item {
            Item::Thing(i) => {
                let t = &self.level.things[i];
                let size = self.size_of(t);
                match (self.chosen_light.and_then(|k| t.lights.get(k)), &t.model) {
                    (Some(l), Some(_)) => {
                        h = make(light_spot(t, size, l));
                        let r = rotation_of(t);
                        h.arrows = [Some(r * Vec3::X), Some(r * Vec3::Y), Some(r * Vec3::Z)];
                    }
                    (_, Some(_)) => {
                        h = make(model_point(t, size, centre_in(size)));
                        h.arrows = world;
                        // Its box as it lies: the arrows' heads beyond it,
                        // the rings round it (as far as the view allows).
                        if let Some(size) = size {
                            let lo = -IVec3::new(size.x / 2, size.y / 2, 0).as_vec3();
                            let hi = lo + size.as_vec3();
                            h.body = (0..8).map(|k| model_point(t, Some(size), Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z }))).collect();
                            let round = (size.as_vec3() * 0.5).length() + 1.0;
                            h.ring = h.ring.max(round.min(handles::world_size(view_proj, h.centre, 0.42)));
                        }
                        let round = Quat::from_rotation_z(-(bearing_of(t) as f32).to_radians());
                        let pitch = Quat::from_rotation_x((t.pitch as f32).to_radians());
                        h.rings = [Some(round * Vec3::X), Some(round * pitch * Vec3::Y), Some(Vec3::Z)];
                    }
                    (_, None) if t.is_piece() => {
                        h = make(light_spots(t, None).next().map_or(at + 0.5, |(spot, _)| spot));
                        h.arrows = world;
                    }
                    _ => {
                        h = make(at + Vec3::new(0.5, 0.5, 0.0));
                        h.arrows = world;
                        // A thing of a kind without a behavior turns round.
                        if self.kind_turns(t) {
                            h.rings[2] = Some(Vec3::Z);
                        }
                    }
                }
            }
            Item::Place(_) => {
                h = make(at + Vec3::new(0.5, 0.5, 0.0));
                h.arrows = world;
                h.rings[2] = Some(Vec3::Z);
            }
            Item::Area(i) => {
                let size = IVec3::from(self.level.areas[i].size).as_vec3();
                h = make(at + size * 0.5);
                h.arrows = world;
                h.faces = Some((at, at + size));
            }
        }
        Some(h)
    }

    /// The press on a handle `grip` of the chosen one (`h` as `handles`
    /// gave them), the mouse at `mouse` (pixels of a `screen` big) with its
    /// ray: dragged from there until let go (one step, `release`) or
    /// Escape (`cancel_drag`). Whether it took.
    #[allow(clippy::too_many_arguments)]
    pub fn grab(&mut self, grip: Grip, h: Handles, view_proj: glam::Mat4, screen: glam::Vec2, mouse: glam::Vec2, ray: (Vec3, Vec3)) -> bool {
        let Some(item) = self.chosen else { return false };
        let Some(drag) = Drag::begin(&h, grip, view_proj, screen, mouse, ray) else { return false };
        self.release();
        let was = match item {
            Item::Thing(i) => Was::Thing(self.level.things[i].clone()),
            Item::Place(i) => Was::Place(self.level.places[i].clone()),
            Item::Area(i) => Was::Area(self.level.areas[i].clone()),
        };
        let light = self.chosen_light.filter(|_| matches!(&was, Was::Thing(t) if t.model.is_some()));
        self.stroke = Some(Step::Level(self.level_state(), self.level_state()));
        self.gripped = Some(Gripped { item, light, handles: h, drag, was });
        true
    }

    /// The handle being dragged, if one is.
    pub fn gripped(&self) -> Option<&Gripped> {
        self.gripped.as_ref()
    }

    /// The mouse dragged on with a handle: the chosen one moved along its
    /// axis by whole voxels, turned (in quarters on the grid or for a
    /// solid piece, else freely, the quarters near taken), an area sized.
    pub fn drag_grip(&mut self, view_proj: glam::Mat4, screen: glam::Vec2, mouse: glam::Vec2, ray: (Vec3, Vec3)) {
        let Some(mut g) = self.gripped.take() else { return };
        let amount = g.drag.amount(&g.handles, view_proj, screen, mouse, ray) as f64;
        self.follow_grip(&g, amount);
        self.gripped = Some(g);
    }

    /// The item as the grip has it `amount` on (voxels or degrees) from
    /// where it was pressed.
    fn follow_grip(&mut self, g: &Gripped, amount: f64) {
        let whole = amount.round() as i32;
        let along = |a: usize| IVec3::from_array(std::array::from_fn(|k| if k == a { whole } else { 0 }));
        match (&g.was, g.drag.grip, g.item) {
            (Was::Thing(t), Grip::Arrow(a), Item::Thing(i)) => match g.light {
                Some(k) => {
                    let Some(l) = t.lights.get(k) else { return };
                    let to = IVec3::from(l.at.unwrap_or_default()) + along(a);
                    self.level.things[i].lights[k].at = Some(to.to_array());
                }
                None => {
                    let to = self.inside_for(g.item, IVec3::from(t.at) + along(a));
                    self.level.things[i].at = to.to_array();
                }
            },
            (Was::Thing(t), Grip::Ring(a), Item::Thing(i)) if t.model.is_some() => {
                let quarters = !self.free_turns || t.solid;
                let (mut b, mut p, mut r) = (bearing_of(t), t.pitch, t.roll);
                // The right hand's way about the upright is anticlockwise
                // seen from above: the bearing goes back.
                match a {
                    0 => p = handles::step(p, amount, quarters, RING_SNAP),
                    1 => r = handles::step(r, amount, quarters, RING_SNAP),
                    _ => b = handles::step(b, -amount, quarters, RING_SNAP),
                }
                let now = self.turned(t, b, p, r, RING_SNAP);
                self.level.things[i] = now;
            }
            (Was::Place(p), Grip::Arrow(a), Item::Place(i)) => self.level.places[i].at = self.inside_for(g.item, IVec3::from(p.at) + along(a)).to_array(),
            (Was::Thing(t), Grip::Ring(_), Item::Thing(i)) if !t.is_piece() => {
                // A thing of a kind: round only, in quarters unless its
                // kind turns freely.
                let free = self.free_kinds.contains(&t.kind);
                let b = handles::step(bearing_of(t), -amount, !free, RING_SNAP);
                self.level.things[i].facing = Some(facing_at(b, free));
            }
            (Was::Place(p), Grip::Ring(_), Item::Place(i)) => {
                // Quarter turns clockwise seen from above.
                let quarters = (-amount / 90.0).round() as i32;
                let mut now = p.clone();
                for _ in 0..quarters.rem_euclid(4) {
                    let [x, y] = now.facing.unwrap_or(Way::South.pair());
                    now.facing = Some([y, -x]);
                    now.named = now.named || p.facing.is_none();
                }
                self.level.places[i] = now;
            }
            (Was::Area(ar), Grip::Arrow(a), Item::Area(i)) => self.level.areas[i].at = self.inside_for(g.item, IVec3::from(ar.at) + along(a)).to_array(),
            (Was::Area(ar), Grip::Face(a, high), Item::Area(i)) => {
                let now = &mut self.level.areas[i];
                if high {
                    now.size[a] = (ar.size[a] + whole).clamp(1, 4096);
                } else {
                    let by = whole.min(ar.size[a] - 1);
                    (now.at[a], now.size[a]) = (ar.at[a] + by, ar.size[a] - by);
                }
            }
            _ => {}
        }
    }

    /// Escape while dragging (a handle, or the chosen one over the floor):
    /// all as it was before the press, no step. Whether a drag was going on.
    pub fn cancel_drag(&mut self) -> bool {
        if self.gripped.is_none() && self.dragging.is_none() {
            return false;
        }
        (self.gripped, self.dragging) = (None, None);
        if let Some(Step::Level(before, _)) = self.stroke.take() {
            self.set_level(&before);
        }
        true
    }

    /// LIGHT: the chosen piece's lights switched off (taken out, kept to
    /// switch on again while the editor is open) or on (as they were, else
    /// a new one at its model's top); one step each.
    pub fn switch_light(&mut self) {
        let Some(Item::Thing(i)) = self.chosen else { return };
        let t = &self.level.things[i];
        if t.model.is_none() {
            return;
        }
        if !t.lights.is_empty() {
            let lights = t.lights.clone();
            self.put_away.retain(|(j, _)| *j != i);
            self.put_away.push((i, lights));
            self.chosen_light = None;
            self.change(None, |l| l.things[i].lights.clear());
        } else if let Some(k) = self.put_away.iter().position(|(j, _)| *j == i) {
            let (_, lights) = self.put_away.remove(k);
            self.change(None, |l| l.things[i].lights = lights);
        } else {
            let top = self.size_of(t).map_or(10, |s| s.z);
            self.add_piece_light(top);
        }
    }

    /// The right-click menu's ADD on empty floor: a light, a place or an
    /// area set down at `spot` at once (a light some way up), chosen.
    pub fn add_at(&mut self, what: Hanging, spot: IVec3) {
        self.hang(what);
        self.hang_at(Some(spot));
        self.set_down();
    }

    /// ADD LIGHT on the chosen piece: one more light (the first at `top`,
    /// its model's top, from its pivot; another beside the last, wider),
    /// warm. A light alone stays one light.
    pub fn add_piece_light(&mut self, top: i32) {
        let Some(Item::Thing(i)) = self.chosen else { return };
        if self.level.things[i].model.is_none() {
            return;
        }
        let light = match self.level.things[i].lights.last() {
            None => ThingLight { at: Some([0, 0, top]), radius: PIECE_RADIUS, color: Some(NEW_COLOUR.into()) },
            Some(last) => ThingLight { radius: (last.radius * 2.0).min(2000.0), ..last.clone() },
        };
        self.change(None, |l| l.things[i].lights.push(light));
    }

    /// REMOVE beside a light of the chosen piece: that light taken away. A
    /// light alone keeps its one light.
    pub fn remove_piece_light(&mut self, k: usize) {
        let Some(Item::Thing(i)) = self.chosen else { return };
        if self.level.things[i].model.is_none() || k >= self.level.things[i].lights.len() {
            return;
        }
        self.change(None, |l| drop(l.things[i].lights.remove(k)));
        self.chosen_light = None;
    }

    /// The colour (`#rrggbb`) of the chosen one's light `k`.
    pub fn set_colour(&mut self, k: usize, hex: &str) {
        let Some(Item::Thing(i)) = self.chosen else { return };
        let hex = hex.to_string();
        self.change(Some(Field::Colour(k)), |l| {
            if let Some(light) = l.things[i].lights.get_mut(k) {
                light.color = Some(hex);
            }
        });
    }

    /// SOLID: the chosen piece's voxels hold bodies up and stop them (or no
    /// more). Refused, with why, for a piece turned off the quarter turns:
    /// its voxels would lie off the grid.
    pub fn set_solid(&mut self, on: bool) {
        let Some(Item::Thing(i)) = self.chosen else { return };
        let t = &self.level.things[i];
        if t.model.is_none() {
            return;
        }
        if on && t.off_grid() {
            self.message = Some(format!("{} is turned off the quarter turns: only a piece in quarter turns can be solid (its voxels must lie on the grid). Rotate it to a quarter first.", self.label_of(Item::Thing(i)).0));
            return;
        }
        self.message = None;
        self.change(None, |l| l.things[i].solid = on);
    }

    /// The level's daylight with a number of DAYLIGHT set (None: not one of
    /// its fields); unset words taken from the game's `c.light`.
    fn day_field(&self, field: Field, v: f64) -> Option<crate::grid::Daylight> {
        let mut day = self.level.daylight.clone().unwrap_or_default();
        let game = &self.game_light;
        let sun = day.sun.unwrap_or(game.sun.map(|c| c as f64));
        let (from, height) = sun_stands(sun);
        let hundredths = |v: f64| (v.round().clamp(0.0, 100.0)) / 100.0;
        match field {
            Field::SunFrom => day.sun = Some(sun_way(v.rem_euclid(360.0), height)),
            Field::SunHeight => day.sun = Some(sun_way(from, v.clamp(1.0, 90.0))),
            Field::Ambient => day.ambient = Some(hundredths(v)),
            Field::Shadow => day.shadow = Some(hundredths(v)),
            _ => return None,
        }
        Some(day)
    }

    /// The level's daylight's sun colour (`sun`) or sky's (`#rrggbb`).
    pub fn set_day_colour(&mut self, sun: bool, hex: &str) {
        let mut day = self.level.daylight.clone().unwrap_or_default();
        if sun {
            day.sun_color = Some(hex.to_string());
        } else {
            day.sky = Some(hex.to_string());
        }
        self.change(Some(Field::DayColour), |l| l.daylight = Some(day));
    }

    /// OWN DAYLIGHT: the level gets a daylight of its own, as the game's
    /// is now (`c.light`), to change from there.
    pub fn day_own(&mut self) {
        let g = &self.game_light;
        let hex = |c: [f32; 3]| {
            let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let (from, height) = sun_stands(g.sun.map(|v| v as f64));
        let unit = |v: f32| (v as f64 * 100.0).round() / 100.0;
        let day = crate::grid::Daylight { sun: Some(sun_way(from, height)), sun_color: Some(hex(g.sun_color)), ambient: Some(unit(g.ambient)), shadow: Some(unit(g.shadow)), sky: Some(hex(g.sky)) };
        self.change(None, |l| l.daylight = Some(day));
    }

    /// AS THE GAME: the level's own daylight taken out (the game's
    /// `c.light` again).
    pub fn day_as_game(&mut self) {
        self.change(None, |l| l.daylight = None);
    }

    /// EDIT MODEL: the chosen piece's model asked to open in the model
    /// editor.
    pub fn edit_model(&mut self) {
        self.wants_model = self.chosen_thing().and_then(|t| t.model.clone().or_else(|| self.kind_models.get(&t.kind).cloned()));
    }

    /// Whether a thing of a kind faces as the level's file turns it: a
    /// kind without a behavior (one with turns by its ready-made rules).
    pub fn kind_turns(&self, t: &Thing) -> bool {
        !t.is_piece() && self.still_kinds.contains(&t.kind)
    }

    /// The game's models a piece can be made of, by their paths in the
    /// game folder (folders first, then names, in natural order): every
    /// `.vvx` but the blocks, the levels, the builds and the game's worlds
    /// and prefabs.
    pub fn model_choices(&self) -> Vec<String> {
        fn walk(dir: &std::path::Path, from: &str, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let path = format!("{from}{name}");
                if e.path().is_dir() {
                    if from.is_empty() && matches!(name.as_str(), "blocks" | "levels" | "builds" | "replays" | "shots" | "target") {
                        continue;
                    }
                    walk(&e.path(), &format!("{path}/"), out);
                } else if name.ends_with(&format!(".{}", crate::vvx::EXTENSION)) {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.dir, "", &mut out);
        out.retain(|f| !self.scenes.contains(f));
        out.sort_by(|a, b| {
            let folder = |p: &str| p.rsplit_once('/').map_or(String::new(), |(f, _)| f.to_string());
            folder(a).cmp(&folder(b)).then_with(|| crate::levels::natural(a, b))
        });
        out
    }
}

/// The facing an angle gives: within `SNAP` of a quarter turn (or always
/// in a game on the grid) the way there, else the angle (to a tenth).
pub fn facing_at(degrees: f64, free: bool) -> Facing {
    let d = degrees.rem_euclid(360.0);
    let quarter = (d / 90.0).round() * 90.0;
    if !free || (d - quarter).abs() <= SNAP {
        Facing::Way(Way::nearest(quarter))
    } else {
        Facing::Turn((d * 10.0).round() / 10.0)
    }
}

/// What is hidden in the editor alone (its eye in the outliner, HIDE in
/// a menu): not drawn, not met by a click, nothing of it written to the
/// level's file; kept while the window is open. By name; an unnamed thing
/// as it is.
#[derive(Clone, Debug, Default)]
pub struct Hidden {
    names: std::collections::BTreeSet<(u8, String)>,
    things: Vec<Thing>,
}

impl Editor {
    /// How `item` is known to `Hidden`: its sort and name, else the thing.
    fn hidden_key(&self, item: Item) -> Option<Result<(u8, String), Thing>> {
        let sort = match item {
            Item::Thing(_) => 0,
            Item::Place(_) => 1,
            Item::Area(_) => 2,
        };
        match (self.name_of(item), item) {
            (Some(n), _) => Some(Ok((sort, n.to_string()))),
            (None, Item::Thing(i)) => self.level.things.get(i).cloned().map(Err),
            _ => None,
        }
    }

    /// Whether `item` is hidden in the editor.
    pub fn is_hidden(&self, item: Item) -> bool {
        match self.hidden_key(item) {
            Some(Ok(k)) => self.hidden.names.contains(&k),
            Some(Err(t)) => self.hidden.things.contains(&t),
            None => false,
        }
    }

    /// Whether anything is hidden.
    pub fn any_hidden(&self) -> bool {
        (0..self.level.things.len()).map(Item::Thing).chain((0..self.level.places.len()).map(Item::Place)).chain((0..self.level.areas.len()).map(Item::Area)).any(|i| self.is_hidden(i))
    }

    /// `item` hidden or shown again (no step: the file is not touched); a
    /// hidden one chosen is chosen no more.
    pub fn set_hidden(&mut self, item: Item, hide: bool) {
        let Some(key) = self.hidden_key(item) else { return };
        match (key, hide) {
            (Ok(k), true) => {
                self.hidden.names.insert(k);
            }
            (Ok(k), false) => {
                self.hidden.names.remove(&k);
            }
            (Err(t), true) => self.hidden.things.push(t),
            (Err(t), false) => self.hidden.things.retain(|o| *o != t),
        }
        if hide && self.chosen == Some(item) {
            self.choose_item(None);
        }
    }

    /// HIDE / SHOW (the eye).
    pub fn toggle_hidden(&mut self, item: Item) {
        let hidden = self.is_hidden(item);
        self.set_hidden(item, !hidden);
    }

    /// ONLY THIS: everything else hidden, it shown.
    pub fn only(&mut self, item: Item) {
        let all: Vec<Item> = (0..self.level.things.len()).map(Item::Thing).chain((0..self.level.places.len()).map(Item::Place)).chain((0..self.level.areas.len()).map(Item::Area)).collect();
        for other in all {
            self.set_hidden(other, other != item);
        }
    }

    /// SHOW ALL.
    pub fn show_all(&mut self) {
        self.hidden = Hidden::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::tests::PIECES;

    /// The sketch's room (`grid::tests::PIECES`) open in the editor, the
    /// game turning things freely or not; the folder and the room's path.
    fn room(name: &str, free: bool) -> (Editor, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("room-{name}"));
        std::fs::write(dir.join("room.toml"), PIECES).unwrap();
        let mut ed = Editor::open(&dir, "room.toml").unwrap();
        ed.free_turns = free;
        (ed, dir)
    }

    fn saved(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("room.toml")).unwrap()
    }

    fn read(dir: &std::path::Path) -> GridLevel {
        GridLevel::read(&dir.join("room.toml")).unwrap()
    }

    /// A ray straight down onto (x, y) from high above.
    fn down(x: f32, y: f32) -> (Vec3, Vec3) {
        (Vec3::new(x, y, 500.0), Vec3::NEG_Z)
    }

    /// A level without blocks opens among its pieces; choosing and letting
    /// go writes nothing; a model added hangs on the mouse, named after
    /// its file, and a click sets it down, chosen, appended to the file.
    #[test]
    fn a_piece_is_added_where_the_mouse_is() {
        let (mut ed, dir) = room("add", false);
        assert!(ed.among_pieces() && ed.level.things.len() == 5);
        ed.choose_item(Some(Item::Thing(1)));
        ed.release();
        assert_eq!(saved(&dir), PIECES, "nothing changed, nothing written");
        ed.add_piece("room/stove.vvx");
        assert!(!ed.set_down(), "nowhere to set it down yet");
        ed.hang_at(Some(IVec3::new(70, 30, 0)));
        assert!(ed.set_down() && ed.hanging.is_none());
        assert_eq!(ed.chosen, Some(Item::Thing(5)));
        let t = &read(&dir).things[5];
        assert_eq!((t.name.as_deref(), t.model.as_deref(), t.at), (Some("stove_2"), Some("room/stove.vvx"), [70, 30, 0]));
        assert!(saved(&dir).starts_with(PIECES.split("[[place]]").next().unwrap().trim_end()), "the file's things as they were, the new one after them");
        // Off the level the spot is kept inside it; a light alone hangs some
        // way up.
        ed.add_light();
        ed.hang_at(Some(IVec3::new(900, -5, 0)));
        assert_eq!(ed.hang_spot, Some(IVec3::new(359, 0, 0)));
        assert!(matches!(&ed.hanging, Some(Hanging::Thing(t)) if t.at == [359, 0, 40] && t.name.as_deref() == Some("light")));
        assert!(ed.drop_hanging() && ed.hanging.is_none(), "Escape lets go");
        ed.undo();
        assert_eq!(saved(&dir), PIECES, "a step back: the file as it was");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A piece pressed is chosen and dragged over the level at its own
    /// height, one step; nudged and raised by keys (one step for a run of
    /// them), set by its numbers; only its `at` line changes.
    #[test]
    fn a_piece_is_moved_and_raised() {
        let (mut ed, dir) = room("move", false);
        let (o, d) = down(41.5, 150.5);
        ed.press_item(Some(Item::Thing(0)), o, d);
        assert_eq!(ed.chosen, Some(Item::Thing(0)));
        let (o, d) = down(61.5, 140.5);
        ed.drag_item(o, d);
        ed.release();
        assert_eq!((ed.steps(), read(&dir).things[0].at), (1, [60, 140, 0]));
        assert_eq!(saved(&dir), PIECES.replace("at = [40, 150, 0]", "at = [60, 140, 0]"), "only its at");
        ed.nudge(IVec3::Z * 10);
        ed.nudge(IVec3::X);
        assert_eq!((ed.steps(), read(&dir).things[0].at), (2, [61, 140, 10]), "the nudges one step");
        ed.set_field(Field::At(2), 30.0);
        ed.set_field(Field::At(2), 31.0);
        assert_eq!((ed.steps(), read(&dir).things[0].at), (3, [61, 140, 31]), "a number set again joins its step");
        ed.set_field(Field::At(2), 5000.0);
        assert_eq!(ed.level.things[0].at[2], 119, "inside the level");
        ed.undo();
        ed.undo();
        assert_eq!(read(&dir).things[0].at, [60, 140, 0]);
        ed.redo();
        assert_eq!(read(&dir).things[0].at, [61, 140, 10]);
        // Nothing under the mouse: nothing chosen.
        let (o, d) = down(5.0, 5.0);
        ed.press_item(None, o, d);
        assert_eq!(ed.chosen, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// TURN turns a quarter clockwise seen from above (back with -1); on
    /// the grid only the four ways; turning freely any angle, a few degrees
    /// from a quarter snapping to it; a place turns its arrow, a thing of a
    /// kind and an area do not turn.
    #[test]
    fn pieces_turn_on_the_grid_and_freely() {
        let (mut ed, dir) = room("turn", false);
        ed.choose_item(Some(Item::Thing(0)));
        ed.turn_item(1);
        assert_eq!(ed.level.things[0].facing, Some(Facing::Way(Way::West)), "south, a quarter clockwise: west");
        ed.turn_item(-1);
        ed.turn_item(-1);
        assert_eq!(ed.level.things[0].facing, Some(Facing::Way(Way::East)));
        ed.set_field(Field::Turn, 100.0);
        assert_eq!(ed.level.things[0].facing, Some(Facing::Way(Way::East)), "on the grid an angle is its nearest way");
        assert!(saved(&dir).contains("at = [40, 150, 0]\nfacing = \"east\"\n"));
        ed.free_turns = true;
        ed.set_field(Field::Turn, 100.0);
        assert_eq!(ed.level.things[0].facing, Some(Facing::Turn(100.0)));
        assert!(saved(&dir).contains("at = [40, 150, 0]\nrotation = 100\n"), "{}", saved(&dir));
        ed.turn_item(1);
        assert_eq!(ed.level.things[0].facing, Some(Facing::Turn(190.0)), "a quarter on, off the grid still");
        ed.set_field(Field::Turn, 268.0);
        assert_eq!(ed.level.things[0].facing, Some(Facing::Way(Way::West)), "near a quarter: the quarter");
        ed.choose_item(Some(Item::Place(0)));
        ed.turn_item(1);
        assert_eq!((ed.level.places[0].facing, ed.level.places[0].named), (Some([0, -1]), true), "east, a quarter clockwise: south");
        let steps = ed.steps();
        for item in [Item::Thing(4), Item::Area(0)] {
            ed.choose_item(Some(item));
            ed.turn_item(1);
        }
        assert_eq!(ed.steps(), steps, "a walker and an area do not turn");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// COPY hangs the chosen one on the mouse under a new name; REMOVE
    /// takes it away and UNDO brings it back; RENAME refuses an empty name
    /// and one its sort has, and gives a thing of a kind its first name.
    #[test]
    fn copying_removing_and_renaming() {
        let (mut ed, dir) = room("copy", false);
        ed.choose_item(Some(Item::Thing(0)));
        ed.copy_item();
        ed.hang_at(Some(IVec3::new(10, 10, 0)));
        ed.set_down();
        ed.choose_item(Some(Item::Thing(5)));
        ed.copy_item();
        ed.hang_at(Some(IVec3::new(20, 10, 0)));
        ed.set_down();
        let names: Vec<_> = ed.level.things.iter().map(|t| t.name.clone().unwrap_or_default()).collect();
        assert_eq!(names[5..], ["stove_2", "stove_3"]);
        assert_eq!((ed.level.things[6].facing, ed.level.things[6].line), (Some(Facing::Way(Way::South)), 0));
        ed.remove_item();
        ed.choose_item(Some(Item::Thing(1)));
        ed.remove_item();
        assert!(!saved(&dir).contains("\"lamp\""));
        ed.undo();
        assert!(read(&dir).things.iter().any(|t| t.name.as_deref() == Some("lamp")), "a step back brings it back");
        assert_eq!(ed.rename(Item::Thing(0), "  "), Err("A name needs a letter at least.".into()));
        assert_eq!(ed.rename(Item::Thing(0), "lamp"), Err("Another thing is called lamp: a name is one thing's.".into()));
        assert_eq!(ed.rename(Item::Thing(0), "stove"), Ok(()), "its own name");
        assert_eq!(ed.rename(Item::Place(0), "window"), Err("Another place is called window: a name is one place's.".into()));
        assert_eq!(ed.rename(Item::Area(0), "lamp"), Ok(()), "an area's name is among areas");
        assert_eq!(ed.rename(Item::Thing(4), " guard "), Ok(()));
        let level = read(&dir);
        assert_eq!((level.things[4].name.as_deref(), level.areas[0].name.as_str()), (Some("guard"), "lamp"));
        assert!(saved(&dir).contains("[[thing]]\nname = \"guard\"\nkind = \"walker\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A piece's lights: ADD LIGHT puts the first at its model's top and
    /// another beside it, wider; each tuned (where it shines from, its
    /// radius, its colour) and removed on its own; a light alone keeps its
    /// one light. Where a light shines from turns with the piece.
    #[test]
    fn a_pieces_lights_are_added_and_tuned() {
        let (mut ed, dir) = room("light", false);
        ed.choose_item(Some(Item::Thing(0)));
        ed.add_piece_light(24);
        assert_eq!(read(&dir).things[0].lights, [ThingLight { at: Some([0, 0, 24]), radius: PIECE_RADIUS, color: Some(NEW_COLOUR.into()) }]);
        ed.set_field(Field::LightAt(0, 1), 3.0);
        ed.set_field(Field::Radius(0), 90.4);
        ed.set_colour(0, "#a0b8ff");
        let l = read(&dir).things[0].lights[0].clone();
        assert_eq!((l.at, l.radius, l.color.as_deref()), (Some([0, 3, 24]), 90.0, Some("#a0b8ff")));
        // Facing south, a light 3 voxels to the model's front is 3 towards
        // the camera.
        assert_eq!(light_spot(&ed.level.things[0], None, &l), Vec3::new(40.5, 147.5, 24.5));
        // A second: beside the first, twice as far; tuned on its own; the
        // file says them as a list.
        ed.add_piece_light(24);
        ed.set_colour(1, "#ffb060");
        let lights = read(&dir).things[0].lights.clone();
        assert_eq!((lights.len(), lights[1].at, lights[1].radius, lights[0].color.as_deref()), (2, Some([0, 3, 24]), 180.0, Some("#a0b8ff")));
        assert!(saved(&dir).contains("lights = [\n  { at = [0, 3, 24], radius = 90, color = \"#a0b8ff\" },\n  { at = [0, 3, 24], radius = 180, color = \"#ffb060\" },\n]\n"), "{}", saved(&dir));
        ed.remove_piece_light(0);
        assert_eq!(read(&dir).things[0].lights.iter().map(|l| l.radius).collect::<Vec<_>>(), [180.0], "the other one stays");
        assert!(saved(&dir).contains("light = { at = [0, 3, 24], radius = 180"), "one light: light = again");
        ed.remove_piece_light(0);
        assert!(read(&dir).things[0].lights.is_empty());
        assert_eq!(saved(&dir), PIECES, "on, tuned and off again: as it was");
        ed.choose_item(Some(Item::Thing(2)));
        ed.add_piece_light(0);
        ed.remove_piece_light(0);
        assert_eq!(ed.level.things[2].lights.len(), 1, "a light alone is its one light");
        ed.set_field(Field::Radius(0), 200.0);
        assert!(saved(&dir).contains("light = { radius = 200, color = \"#a0b8ff\" }"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// SOLID on a piece in quarter turns writes its line; on a piece turned
    /// freely it is refused with why, nothing written; a solid piece keeps
    /// to the quarter turns when its angle is set.
    #[test]
    fn a_piece_is_made_solid_on_the_grid_only() {
        let (mut ed, dir) = room("solid", true);
        ed.choose_item(Some(Item::Thing(0)));
        ed.set_solid(true);
        assert!(read(&dir).things[0].solid && saved(&dir).contains("facing = \"south\"\nsolid = true\n"));
        ed.set_field(Field::Turn, 100.0);
        assert_eq!(ed.level.things[0].facing, Some(Facing::Way(Way::East)), "a solid piece turns in quarters");
        ed.set_solid(false);
        assert!(!read(&dir).things[0].solid);
        // The chair stands at 45 degrees.
        ed.choose_item(Some(Item::Thing(3)));
        let before = saved(&dir);
        ed.set_solid(true);
        assert!(ed.message.as_deref().is_some_and(|m| m.contains("only a piece in quarter turns can be solid")), "{:?}", ed.message);
        assert_eq!(saved(&dir), before, "nothing written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DAYLIGHT: the level's own (from the game's), the sun moved round
    /// and up, its colour, the light every face gets; AS THE GAME takes
    /// it out. The sun's way and where it stands read back the same.
    #[test]
    fn a_level_gets_its_own_daylight() {
        let (mut ed, dir) = room("daylight", false);
        ed.game_light.sun = [0.0, -1.0, -1.0];
        ed.day_own();
        let day = read(&dir).daylight.unwrap();
        assert_eq!((day.sun, day.ambient), (Some([0.0, -0.707, -0.707]), Some(ed.game_light.ambient as f64 * 100.0).map(|v| v.round() / 100.0)));
        assert_eq!(sun_stands(day.sun.unwrap()), (0.0, 45.0), "a sun from the north, halfway up");
        ed.set_field(Field::SunFrom, 90.0);
        ed.set_field(Field::SunHeight, 30.0);
        ed.set_field(Field::Ambient, 25.0);
        ed.set_day_colour(false, "#203040");
        let day = read(&dir).daylight.unwrap();
        assert_eq!((sun_stands(day.sun.unwrap()), day.ambient, day.sky.as_deref()), ((90.0, 30.0), Some(0.25), Some("#203040")));
        assert_eq!(day.sun, Some([-0.866, -0.0, -0.5]).map(|s: [f64; 3]| s.map(|v| v + 0.0)));
        ed.day_as_game();
        assert_eq!(saved(&dir), PIECES, "as the game: the file as it was");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A place and an area are added like a piece (an area round the
    /// spot), named on, moved, an area sized; the room's size changes; all
    /// steps back to the file as it was.
    #[test]
    fn places_and_areas() {
        let (mut ed, dir) = room("places", false);
        ed.add_place();
        ed.hang_at(Some(IVec3::new(100, 50, 0)));
        ed.set_down();
        ed.add_area();
        ed.hang_at(Some(IVec3::new(100, 50, 0)));
        ed.set_down();
        let level = read(&dir);
        assert_eq!(level.places[2], Place { name: "place".into(), at: [100, 50, 0], facing: Some([0, -1]), named: true, line: 0 });
        assert_eq!((level.areas[1].name.as_str(), level.areas[1].at, level.areas[1].size), ("area", [80, 35, 0], NEW_AREA));
        ed.set_field(Field::Size(2), 12.0);
        ed.set_field(Field::Size(0), 0.0);
        assert_eq!(read(&dir).areas[1].size, [1, 30, 12]);
        ed.choose_item(None);
        ed.set_field(Field::LevelSize(0), 400.0);
        assert_eq!(read(&dir).size, IVec3::new(400, 200, 120));
        while ed.steps() > 0 {
            ed.undo();
        }
        assert_eq!(saved(&dir), PIECES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A view of the room for the handles, its matrix and the screen.
    fn seen(target: Vec3, yaw: f32, pitch: f32) -> (glam::Mat4, glam::Vec2) {
        let orbit = crate::editor::view::Orbit { yaw, pitch, zoom: 300.0, target, fov: crate::editor::view::FOV };
        (orbit.view_proj(16.0 / 9.0), glam::Vec2::new(1600.0, 900.0))
    }

    /// A point of the room on the screen, and the mouse's ray through it.
    fn at_screen(vp: glam::Mat4, screen: glam::Vec2, p: Vec3) -> (glam::Vec2, (Vec3, Vec3)) {
        let (px, _) = handles::project(vp, screen, p).unwrap();
        (px, crate::editor::view::mouse_ray(vp, px.x / screen.x, px.y / screen.y))
    }

    /// The stove's model is 20 across, 14 deep and 30 tall.
    fn with_sizes(ed: &mut Editor) {
        ed.sizes.insert("room/stove.vvx".into(), IVec3::new(20, 14, 30));
        ed.sizes.insert("room/chair.vvx".into(), IVec3::new(8, 8, 12));
    }

    /// An arrow dragged moves the chosen one along its axis only, by whole
    /// voxels, the numbers following, one step back; Escape while dragging
    /// puts it back, no step.
    #[test]
    fn an_arrow_moves_along_its_axis_by_whole_voxels() {
        let (mut ed, dir) = room("arrow", false);
        with_sizes(&mut ed);
        ed.choose_item(Some(Item::Thing(0)));
        for (yaw, pitch) in [(0.0, 30.0), (40.0, 50.0)] {
            let (vp, screen) = seen(Vec3::new(40.0, 150.0, 10.0), yaw, pitch);
            let h = ed.handles(vp, false).unwrap();
            let from = ed.level.things[0].at;
            assert_eq!(h.centre, IVec3::from(from).as_vec3() + Vec3::new(0.5, 0.5, 15.5), "where it turns: the voxel at half its height");
            let (press, ray) = at_screen(vp, screen, h.centre + Vec3::X * h.length * 0.9);
            assert!(ed.grab(Grip::Arrow(0), h.clone(), vp, screen, press, ray));
            let (to, ray) = at_screen(vp, screen, h.centre + Vec3::X * (h.length * 0.9 + 7.4));
            ed.drag_grip(vp, screen, to, ray);
            assert_eq!(ed.level.things[0].at, [from[0] + 7, from[1], from[2]], "{yaw}: along x only, whole voxels");
            ed.release();
        }
        assert_eq!(ed.steps(), 2, "a drag a step");
        assert_eq!(saved(&dir), PIECES.replace("at = [40, 150, 0]", "at = [54, 150, 0]"), "only its at");
        ed.undo();
        ed.undo();
        assert_eq!(saved(&dir), PIECES);
        // Escape while dragging: as it was, no step.
        let (vp, screen) = seen(Vec3::new(40.0, 150.0, 10.0), 0.0, 30.0);
        let h = ed.handles(vp, false).unwrap();
        let (press, ray) = at_screen(vp, screen, h.centre + Vec3::Z * h.length * 0.9);
        ed.grab(Grip::Arrow(2), h.clone(), vp, screen, press, ray);
        let (to, ray) = at_screen(vp, screen, h.centre + Vec3::Z * (h.length * 0.9 + 5.0));
        ed.drag_grip(vp, screen, to, ray);
        assert_eq!(ed.level.things[0].at[2], 5);
        assert!(ed.cancel_drag() && ed.level.things[0].at[2] == 0 && ed.gripped().is_none());
        ed.release();
        assert_eq!((ed.steps(), saved(&dir)), (0, PIECES.to_string()));
        assert!(!ed.cancel_drag(), "nothing dragged: Escape goes on to what it did before");
    }

    /// A ring turns the piece about its axis the way it is dragged: on the
    /// grid in quarter turns, freely otherwise (a quarter within a few
    /// degrees taken); a solid piece in quarters always; one step.
    #[test]
    fn a_ring_turns_in_quarters_on_the_grid_and_freely_otherwise() {
        for (free, solid, swept, want) in [(false, false, 50.0, 90.0), (false, false, 40.0, 180.0), (true, false, 50.0, 130.0), (true, false, 87.0, 90.0), (true, true, 50.0, 90.0)] {
            let (mut ed, dir) = room("ring", free);
            with_sizes(&mut ed);
            ed.choose_item(Some(Item::Thing(0)));
            ed.level.things[0].solid = solid;
            // From above: the upright ring round the middle, swept
            // anticlockwise as seen (the right hand's way about +z).
            let (vp, screen) = seen(Vec3::new(40.0, 150.0, 10.0), 0.0, 85.0);
            let h = ed.handles(vp, true).unwrap();
            let middle = handles::project(vp, screen, h.centre).unwrap().0;
            let point = |deg: f32| {
                let p = middle + glam::Vec2::new(deg.to_radians().cos(), -deg.to_radians().sin()) * 100.0;
                (p, crate::editor::view::mouse_ray(vp, p.x / screen.x, p.y / screen.y))
            };
            let (press, ray) = point(0.0);
            assert!(ed.grab(Grip::Ring(2), h.clone(), vp, screen, press, ray));
            for k in 1..=10 {
                let (p, ray) = point(swept * k as f32 / 10.0);
                ed.drag_grip(vp, screen, p, ray);
            }
            ed.release();
            let b = bearing_of(&ed.level.things[0]);
            assert_eq!(b, want, "free {free}, solid {solid}, swept {swept}: south turned anticlockwise");
            assert_eq!(ed.steps(), (want != 180.0) as usize);
            let _ = dir;
        }
    }

    /// Tipped about its side (pitch) a piece lies down and stays on the
    /// floor it stood on: its `at` goes under it by what it now reaches
    /// down; laid back up it stands where it stood. On the grid only
    /// quarters; freely any angle; the file says pitch and roll.
    #[test]
    fn a_piece_laid_down_stays_on_its_floor() {
        let (mut ed, dir) = room("lay", false);
        with_sizes(&mut ed);
        ed.choose_item(Some(Item::Thing(0)));
        assert!(ed.tip(0, 90.0));
        let t = ed.level.things[0].clone();
        assert_eq!((t.pitch, t.at), (90.0, [40, 150, -8]), "20 by 14 by 30, on its back: its 14 deep now up, about the voxel 15.5 up");
        assert!((low_of(IVec3::new(20, 14, 30), rotation_of(&t)) + t.at[2] as f32).abs() < 1e-3, "its lowest voxel on the floor");
        assert!(saved(&dir).contains("at = [40, 150, -8]\nfacing = \"south\"\npitch = 90\n"), "{}", saved(&dir));
        // Its handles turn with it: the roll ring about its forward, now up.
        let h = ed.handles(seen(Vec3::ZERO, 0.0, 30.0).0, true).unwrap();
        assert!((h.rings[1].unwrap() - Vec3::Z).length() < 1e-5, "{:?}", h.rings[1]);
        assert!(ed.tip(1, 90.0));
        assert!(!ed.tip(0, 30.0), "on the grid only quarters");
        ed.tip(1, -90.0);
        ed.tip(0, -90.0);
        assert_eq!(saved(&dir), PIECES, "laid down and stood up again: as it was");
        ed.free_turns = true;
        assert!(ed.tip(0, 30.0));
        assert_eq!(ed.level.things[0].pitch, 30.0);
        // A number typed keeps it on its floor the same way.
        ed.set_field(Field::Pitch, 0.0);
        assert_eq!(ed.level.things[0].at, [40, 150, 0]);
        // Solid: quarters only, and solid refused while tipped off them.
        ed.set_field(Field::Roll, 20.0);
        ed.set_solid(true);
        assert!(!ed.level.things[0].solid && ed.message.is_some());
    }

    /// An area is sized by the squares on its faces: the high side grows
    /// it, the low side moves its corner and keeps the high side where it
    /// was.
    #[test]
    fn an_area_is_sized_by_its_faces() {
        let (mut ed, _dir) = room("faces", false);
        ed.choose_item(Some(Item::Area(0)));
        let (vp, screen) = seen(Vec3::new(50.0, 135.0, 20.0), 20.0, 30.0);
        let h = ed.handles(vp, false).unwrap();
        assert_eq!(h.faces, Some((Vec3::new(30.0, 120.0, 0.0), Vec3::new(70.0, 150.0, 40.0))));
        let (press, ray) = at_screen(vp, screen, Vec3::new(70.0, 135.0, 20.0));
        ed.grab(Grip::Face(0, true), h.clone(), vp, screen, press, ray);
        let (to, ray) = at_screen(vp, screen, Vec3::new(76.0, 135.0, 20.0));
        ed.drag_grip(vp, screen, to, ray);
        ed.release();
        assert_eq!((ed.level.areas[0].at, ed.level.areas[0].size), ([30, 120, 0], [46, 30, 40]));
        let h = ed.handles(vp, false).unwrap();
        let (press, ray) = at_screen(vp, screen, Vec3::new(30.0, 135.0, 20.0));
        ed.grab(Grip::Face(0, false), h.clone(), vp, screen, press, ray);
        let (to, ray) = at_screen(vp, screen, Vec3::new(35.0, 135.0, 20.0));
        ed.drag_grip(vp, screen, to, ray);
        ed.release();
        assert_eq!((ed.level.areas[0].at, ed.level.areas[0].size), ([35, 120, 0], [41, 30, 40]));
    }

    /// MOVE shows the arrows only, TURN the rings only (a light alone, a
    /// thing of a kind, an area do not turn); a press on the piece's body
    /// grabs no handle (the piece is dragged), its arrows' heads lie clear
    /// of it and grab; an arrow seen end on is dimmed and grabs nothing.
    #[test]
    fn move_and_turn_and_the_body_is_the_pieces() {
        let (mut ed, _dir) = room("modes", false);
        with_sizes(&mut ed);
        ed.choose_item(Some(Item::Thing(0)));
        let (vp, screen) = seen(Vec3::new(40.0, 150.0, 10.0), 0.0, 30.0);
        let moving = ed.handles(vp, false).unwrap();
        assert!(moving.arrows.iter().all(Option::is_some) && moving.rings.iter().all(Option::is_none));
        let turning = ed.handles(vp, true).unwrap();
        assert!(turning.arrows.iter().all(Option::is_none) && turning.rings.iter().all(Option::is_some));
        assert!(turning.ring >= (Vec3::new(20.0, 14.0, 30.0) * 0.5).length(), "the rings round its box");
        // The body: no handle there in either; the X arrow's head beyond it.
        let body = moving.body_on_screen(vp, screen).unwrap();
        let middle = handles::project(vp, screen, moving.centre).unwrap().0;
        assert_eq!((moving.grip_at(vp, screen, middle), turning.grip_at(vp, screen, middle)), (None, None));
        let shapes = moving.shapes(vp, screen);
        let head = shapes.iter().find(|s| s.grip == Grip::Arrow(0) && s.what == handles::What::Head).unwrap();
        assert!(head.points[0].x > body.1.x + 10.0, "the head clear of the stove: {:?} past {:?}", head.points[0], body.1);
        assert_eq!(moving.grip_at(vp, screen, head.points[0] - glam::Vec2::new(4.0, 0.0)), Some(Grip::Arrow(0)));
        // Seen from straight in front, Y points at the eye: dimmed, no grip.
        let (front, _) = seen(Vec3::new(40.0, 150.0, 10.0), 0.0, 0.0);
        let y = ed.handles(front, false).unwrap().shapes(front, screen).into_iter().find(|s| s.grip == Grip::Arrow(1)).unwrap();
        assert!(y.dim);
        // What does not turn has no TURN: a light alone, a thing of a kind.
        for (item, turns) in [(Item::Thing(0), true), (Item::Thing(2), false), (Item::Thing(4), false), (Item::Place(0), true), (Item::Area(0), false)] {
            ed.choose_item(Some(item));
            assert_eq!(ed.can_turn(), turns, "{item:?}");
        }
    }

    /// Z in the inspector is how high a piece's lowest voxel stands: 0 for
    /// one laid on the floor, though its `at` lies under it; set, it lifts
    /// the piece. An area's name is a word (the code says
    /// level.areas.NAME); a thing's may have spaces.
    #[test]
    fn z_is_the_lowest_voxel_and_an_areas_name_is_a_word() {
        let (mut ed, dir) = room("base", false);
        with_sizes(&mut ed);
        ed.choose_item(Some(Item::Thing(0)));
        ed.tip(0, 90.0);
        assert_eq!((ed.level.things[0].at[2], ed.base_of(Item::Thing(0))), (-8, Some(0)));
        ed.set_field(Field::Base, 5.0);
        assert_eq!((ed.level.things[0].at[2], ed.base_of(Item::Thing(0))), (-3, Some(5)));
        assert!(ed.rename(Item::Area(0), "by the stove").unwrap_err().contains("letters, digits and _"));
        assert!(ed.rename(Item::Area(0), "2nd").is_err());
        assert_eq!(ed.rename(Item::Thing(0), "floor lamp"), Ok(()));
        assert!(saved(&dir).contains("name = \"floor lamp\""));
    }

    /// ALIGN ALL takes every turn to the quarters: round, pitch and roll.
    #[test]
    fn align_all_takes_all_three_turns_to_the_quarters() {
        let dir = crate::scratch::Scratch::new("room-align3");
        std::fs::write(dir.join("room.toml"), PIECES.replace("facing = \"south\"\n", "facing = \"south\"\npitch = 30\nroll = 140\n")).unwrap();
        let mut ed = Editor::open(&dir, "room.toml").unwrap();
        assert_eq!(ed.level.off_grid().len(), 2);
        ed.align_all();
        assert!(ed.level.off_grid().is_empty());
        let t = &ed.level.things[0];
        assert_eq!((t.pitch, t.roll), (0.0, 180.0));
        assert!(saved(&dir).contains("facing = \"south\"\nroll = 180\n") && !saved(&dir).contains("pitch"), "{}", saved(&dir));
        assert_eq!(ed.steps(), 1);
    }

    /// The right-click menus: a piece of a model has the most (its model,
    /// its light, turning, laying down, solid), a light alone, a thing of a
    /// kind and an area what every item has, a place its turn too; the
    /// floor what can be added there; ADD the four.
    #[test]
    fn the_menu_offers_what_each_thing_can_do() {
        use crate::editor::view::{Action, MenuOn};
        let (mut ed, _dir) = room("menu", false);
        let words = |ed: &Editor, on| crate::editor::stage::menu_entries(ed, on).into_iter().map(|(w, ..)| w).collect::<Vec<_>>();
        assert_eq!(words(&ed, MenuOn::Item(Item::Thing(0))), ["COPY", "REMOVE", "RENAME", "HIDE", "ONLY THIS", "EDIT MODEL", "LIGHT ON", "ROTATE 90°", "LAY DOWN", "SOLID"]);
        assert_eq!(words(&ed, MenuOn::Item(Item::Thing(1)))[6], "LIGHT OFF", "the lamp has its light");
        assert!(!words(&ed, MenuOn::Item(Item::Thing(3))).contains(&"SOLID".to_string()), "the chair at 45 degrees cannot be solid");
        for item in [Item::Thing(2), Item::Thing(4), Item::Area(0)] {
            assert_eq!(words(&ed, MenuOn::Item(item)), ["COPY", "REMOVE", "RENAME", "HIDE", "ONLY THIS"], "{item:?}");
        }
        assert_eq!(words(&ed, MenuOn::Item(Item::Place(0))), ["COPY", "REMOVE", "RENAME", "HIDE", "ONLY THIS", "ROTATE 90°"]);
        assert_eq!(words(&ed, MenuOn::Floor(IVec3::new(5, 5, 0))), ["ADD MODEL…", "ADD LIGHT", "ADD PLACE", "ADD AREA"]);
        assert_eq!(words(&ed, MenuOn::Add), ["MODEL…", "LIGHT", "PLACE", "AREA"]);
        // Its entries do what they say: a light set down where the floor
        // was clicked, chosen; the stove's light switched on and off again.
        let mut view = crate::editor::view::View::new(&ed, &crate::config::GameConfig::default());
        crate::editor::view::act(&mut ed, &mut view, Action::AddAt(0, IVec3::new(70, 30, 0)));
        assert_eq!((ed.chosen, ed.level.things[5].at), (Some(Item::Thing(5)), [70, 30, 40]));
        crate::editor::view::act(&mut ed, &mut view, Action::OpenMenu(Item::Thing(0), [10.0, 10.0]));
        assert_eq!((ed.chosen, view.menu.map(|m| m.on)), (Some(Item::Thing(0)), Some(MenuOn::Item(Item::Thing(0)))));
        crate::editor::view::act(&mut ed, &mut view, Action::SwitchLight);
        assert!(view.menu.is_none() && ed.level.things[0].lights.len() == 1, "a light on, the menu closed");
        ed.set_colour(0, "#a0b8ff");
        ed.switch_light();
        assert!(ed.level.things[0].lights.is_empty());
        ed.switch_light();
        assert_eq!(ed.level.things[0].lights[0].color.as_deref(), Some("#a0b8ff"), "on again as it was");
    }

    /// The game's models to set down: folders after the game's own, the
    /// blocks, levels and worlds left out.
    #[test]
    fn the_models_to_choose_from() {
        let (mut ed, dir) = room("models", false);
        for f in ["hero.vvx", "room/stove.vvx", "room/lamp.vvx", "blocks/stone.vvx", "levels/old.vvx", "world.vvx", "notes.txt"] {
            std::fs::create_dir_all(dir.join(f).parent().unwrap()).unwrap();
            std::fs::write(dir.join(f), "").unwrap();
        }
        ed.scenes = vec!["world.vvx".into()];
        assert_eq!(ed.model_choices(), ["hero.vvx", "room/lamp.vvx", "room/stove.vvx"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
