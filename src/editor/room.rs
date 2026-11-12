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
/// (x, y, z), its light's place in its model and its reach, its turn, an
/// area's size, the level's size; the arrow keys' nudges; a light's
/// colour being picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    At(usize),
    LightAt(usize),
    Radius,
    Colour,
    Turn,
    Size(usize),
    LevelSize(usize),
    Nudge,
}

/// Colours for a light to choose from at once, warm to cool, each with
/// what it is like.
pub const LIGHT_COLOURS: [(&str, &str); 6] = [("#ffd9a0", "a lamp"), ("#ffb060", "a candle"), ("#ffffff", "white"), ("#a0b8ff", "the moon"), ("#a0ffc0", "green"), ("#ff8070", "red")];

/// A new light's reach (a piece's, a light alone's) and colour, a new
/// area's size.
const PIECE_RADIUS: f64 = 60.0;
const LIGHT_RADIUS: f64 = 80.0;
const NEW_COLOUR: &str = "#ffd9a0";
const NEW_AREA: [i32; 3] = [40, 30, 40];

/// How close to a quarter turn an angle is taken as it (degrees).
pub const SNAP: f64 = 3.0;

/// The bearing a thing faces (degrees clockwise seen from above, north 0;
/// south when the file says none).
pub fn bearing_of(t: &Thing) -> f64 {
    t.facing.map_or(180.0, Facing::bearing)
}

/// How a game draws a thing facing `facing`: the quarter way it looks
/// and the free turn after it (none on the four ways), as a level's piece
/// is drawn (`LuaGame::draw`, `session::facing`).
pub fn turn_of(facing: Option<Facing>) -> (IVec2, Quat) {
    match facing.unwrap_or(Facing::Way(Way::South)) {
        Facing::Way(w) => (IVec2::from(w.pair()), Quat::IDENTITY),
        Facing::Turn(b) => {
            let look = Way::nearest(b);
            let way = (b - look.bearing()).to_radians() as f32;
            let way = (way + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            (IVec2::from(look.pair()), if way.abs() < 1e-4 { Quat::IDENTITY } else { Quat::from_rotation_z(-way) })
        }
    }
}

/// A thing's turn as a whole: clockwise seen from above by its bearing.
pub fn rotation_of(t: &Thing) -> Quat {
    Quat::from_rotation_z(-(bearing_of(t) as f32).to_radians())
}

/// Where a thing's light shines from in the level's voxels: its offset in
/// the model's own voxels turned with it, from the middle of the pivot's
/// cell half a voxel up, as the game puts it.
pub fn light_spot(t: &Thing) -> Option<Vec3> {
    let l = t.light.as_ref()?;
    let offset = IVec3::from(l.at.unwrap_or_default()).as_vec3();
    Some(IVec3::from(t.at).as_vec3() + Vec3::splat(0.5) + rotation_of(t) * offset)
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
        self.tuned = None;
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
            && let Some(Step::Level(_, last)) = self.past.last_mut()
        {
            *last = after;
            if !self.dry {
                self.message = self.save_level().err().map(|e| format!("{e:#}"));
            }
            return;
        }
        let step = Step::Level(before, after);
        self.save(&step);
        self.past.push(step);
        self.future.clear();
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
        self.hang(Hanging::Thing(Thing { name: Some(name), light: Some(light), ..Thing::new(PIECE, [0; 3]) }));
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
        let to = self.inside(IVec3::from(self.at_of(item).expect("chosen")) + by);
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
        let Some(item) = self.chosen else { return };
        let free = self.free_turns;
        self.change(Some(field), |l| match (field, item) {
            (Field::At(a), _) => {
                let a = a.min(2);
                let v = whole.clamp(0, (voxels[a] - 1).max(0));
                match item {
                    Item::Thing(i) => l.things[i].at[a] = v,
                    Item::Place(i) => l.places[i].at[a] = v,
                    Item::Area(i) => l.areas[i].at[a] = v,
                }
            }
            (Field::LightAt(a), Item::Thing(i)) => {
                if let Some(light) = &mut l.things[i].light {
                    let mut at = light.at.unwrap_or_default();
                    at[a.min(2)] = whole;
                    light.at = Some(at);
                }
            }
            (Field::Radius, Item::Thing(i)) => {
                if let Some(light) = &mut l.things[i].light {
                    light.radius = v.round().clamp(1.0, 2000.0);
                }
            }
            (Field::Turn, Item::Thing(i)) if l.things[i].is_piece() => l.things[i].facing = Some(facing_at(v, free)),
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
        let quarter = |t: &mut Thing| {
            if t.is_piece() {
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

    /// LIGHT: the chosen piece's light on (at `top`, its model's top, from
    /// its pivot, warm) or off. A light alone is its light, always on.
    pub fn set_light(&mut self, on: bool, top: i32) {
        let Some(Item::Thing(i)) = self.chosen else { return };
        if self.level.things[i].model.is_none() {
            return;
        }
        let light = on.then(|| ThingLight { at: Some([0, 0, top]), radius: PIECE_RADIUS, color: Some(NEW_COLOUR.into()) });
        self.change(None, |l| l.things[i].light = light);
    }

    /// The chosen one's light's colour (`#rrggbb`).
    pub fn set_colour(&mut self, hex: &str) {
        let Some(Item::Thing(i)) = self.chosen else { return };
        let hex = hex.to_string();
        self.change(Some(Field::Colour), |l| {
            if let Some(light) = &mut l.things[i].light {
                light.color = Some(hex);
            }
        });
    }

    /// EDIT MODEL: the chosen piece's model asked to open in the model
    /// editor.
    pub fn edit_model(&mut self) {
        self.wants_model = self.chosen_thing().and_then(|t| t.model.clone());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::tests::PIECES;

    /// The sketch's room (`grid::tests::PIECES`) open in the editor, the
    /// game turning things freely or not; the folder and the room's path.
    fn room(name: &str, free: bool) -> (Editor, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("voxcade-room-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
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
        assert!(saved(&dir).contains("at = [40, 150, 0]\nturn = 100\n"), "{}", saved(&dir));
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

    /// A piece's light switched on at its model's top, tuned (where it
    /// shines from, its reach, its colour) and off; a light alone's reach.
    /// Where it shines from turns with the piece.
    #[test]
    fn a_light_is_added_and_tuned() {
        let (mut ed, dir) = room("light", false);
        ed.choose_item(Some(Item::Thing(0)));
        ed.set_light(true, 24);
        assert_eq!(read(&dir).things[0].light, Some(ThingLight { at: Some([0, 0, 24]), radius: PIECE_RADIUS, color: Some(NEW_COLOUR.into()) }));
        ed.set_field(Field::LightAt(1), 3.0);
        ed.set_field(Field::Radius, 90.4);
        ed.set_colour("#a0b8ff");
        let l = read(&dir).things[0].light.clone().unwrap();
        assert_eq!((l.at, l.radius, l.color.as_deref()), (Some([0, 3, 24]), 90.0, Some("#a0b8ff")));
        // Facing south, a light 3 voxels to the model's front is 3 towards
        // the camera.
        assert_eq!(light_spot(&ed.level.things[0]), Some(Vec3::new(40.5, 147.5, 24.5)));
        ed.set_light(false, 24);
        assert!(read(&dir).things[0].light.is_none());
        assert_eq!(saved(&dir), PIECES, "on, tuned and off again: as it was");
        ed.choose_item(Some(Item::Thing(2)));
        ed.set_light(false, 0);
        assert!(ed.level.things[2].light.is_some(), "a light alone is its light");
        ed.set_field(Field::Radius, 200.0);
        assert!(saved(&dir).contains("light = { radius = 200, color = \"#a0b8ff\" }"));
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
