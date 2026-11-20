//! Levels kept built (`docs/engine.md`, "Levels kept built"): while the
//! level played belongs to a location (`Game::location`, "Levels that
//! remember" in `docs/game-model.md`), the levels of that location stay
//! built on the GPU as they are left (their chunks' meshes, the lamps'
//! walls with their thin things and the faces' memory of what stops the
//! lamps), and those not yet built are meshed ahead on a thread of their
//! own and sent a few chunks a picture. A change among them swaps instead
//! of building: what changed since a level was put aside (voxels its
//! record laid, blasts) is built anew as ever. Leaving the location lets
//! it all go; a level of no location is built as it is entered, as
//! before. Only the picture: nothing here is the game's.
//!
//! Measured (2026-11-20, 1280 by 720) on a location of three levels of
//! 512 by 512 by 48 voxels with 300 pieces, a dozen lamps walls stop and
//! fog: a change cost 35 to 40 ms of the CPU on all cores (meshes 9, the
//! lamps' walls 6, the fog's thin things 15) and 60 to 70 on four; a level
//! taken up again 3 to 4. Each such level holds about 50 MB of meshes, 2
//! MB of walls and the lamps' memory on the GPU. The CPU's own small GPU
//! spends some 40 ms more on the first picture after such a change, kept
//! or built alike (not found yet where). A level of Voxkeep's size changes
//! in 3 to 8 ms: a level of no location is built as it is entered, as
//! before. The lamps' walls and thin things need the picture's lights and
//! pieces, which only a picture of the level has: they are made on its
//! first visit and kept from then on; only the meshes are built ahead.

use std::collections::VecDeque;
use std::sync::Arc;

use glam::IVec3;

use super::Renderer;
use super::chunks::{ChunkMesh, upload_into};
use super::lamps::LampsKept;
use crate::game::Location;
use crate::mesh::{FaceMask, Vertex};
use crate::world::{CellMap, World};

/// The most GPU bytes the levels put aside hold (meshes, the lamps' walls
/// and memory): past it the one taken up longest ago is let go, said once.
pub const KEPT_BYTES: u64 = 384 << 20;

/// The chunks of a level built ahead a picture sends: no picture waits for
/// all of a level's.
const AHEAD_CHUNKS: usize = 96;

/// A chunk meshed ahead: where, its vertices and indices.
type Meshed = (IVec3, Vec<Vertex>, Vec<u32>);

/// Which level a picture's world is (`Game::location`): its location, the
/// game's levels as read (a level file read anew makes others) and its
/// place among them.
#[derive(Clone)]
struct Key {
    name: String,
    levels: Arc<crate::levels::Levels>,
    level: usize,
}

impl Key {
    fn same_location(&self, other: &Key) -> bool {
        self.name == other.name && Arc::ptr_eq(&self.levels, &other.levels)
    }
}

/// A level put aside built: the world its meshes show (chunks shared with
/// it, so what changed since is told by `World::mark_changed_since`), its
/// meshes, what the lamps' pass held of it, its bytes on the GPU.
struct Aside {
    level: usize,
    world: World,
    /// The faces its meshes hold (its own camera's yaw).
    mask: FaceMask,
    meshes: CellMap<ChunkMesh>,
    lamps: Option<LampsKept>,
    bytes: u64,
}

/// A level of the location built ahead: its world as its file builds it,
/// the thread meshing it, then its chunks sent a few a picture.
struct Ahead {
    level: usize,
    world: World,
    mask: FaceMask,
    job: Option<std::thread::JoinHandle<Vec<Meshed>>>,
    meshed: VecDeque<Meshed>,
    meshes: CellMap<ChunkMesh>,
}

/// What the renderer keeps of a location's levels (`Renderer::update_level`).
pub(super) struct Kept {
    /// The level the picture shows, if it belongs to a location, and its
    /// world as its meshes stand.
    now: Option<Key>,
    shown: Option<World>,
    /// The levels put aside, the one taken up longest ago first.
    aside: Vec<Aside>,
    ahead: Option<Ahead>,
    /// The budget was found too small in this location (said once; nothing
    /// more is built ahead).
    full: bool,
    /// The most bytes kept (`KEPT_BYTES`; the tests' less).
    most: u64,
    /// The faces the meshes of the level shown hold, once built.
    pub(super) mask: Option<FaceMask>,
}

impl Default for Kept {
    fn default() -> Self {
        Self { now: None, shown: None, aside: Vec::new(), ahead: None, full: false, most: KEPT_BYTES, mask: None }
    }
}

impl Kept {
    /// Everything kept let go (the meshes are built anew with other faces,
    /// `Renderer::rebuild_all`).
    pub(super) fn forget(&mut self) {
        (self.shown, self.ahead) = (None, None);
        self.aside.clear();
    }

    /// The bytes the levels put aside hold.
    fn bytes(&self) -> u64 {
        self.aside.iter().map(|a| a.bytes).sum()
    }
}

/// The bytes a level's meshes hold on the GPU.
fn mesh_bytes(meshes: &CellMap<ChunkMesh>) -> u64 {
    meshes.values().map(|m| m.vertices.size() + m.indices.size()).sum()
}

impl Renderer {
    /// The chunks of `world` that changed built and sent (`update_chunks`),
    /// `world` being the level `location` says (`Game::location`): another
    /// level of the same location than the picture showed takes its meshes
    /// and lamps' walls from those put aside (or built ahead), what changed
    /// since built anew; the one left is put aside. The levels of the
    /// location not built yet are meshed ahead meanwhile. How many chunks
    /// were built for `world`. `mask`: the faces of the level shown (its
    /// own camera's yaw); meshes of other faces are built anew whole, a
    /// level put aside is taken up only with its own.
    pub fn update_level(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &mut World, mask: &FaceMask, location: Option<Location>) -> usize {
        let location_masks = location.as_ref().map(|l| l.masks.clone());
        let key = location.map(|l| Key { name: l.name, levels: l.levels, level: l.level });
        let same = match (&key, &self.kept.now) {
            (Some(a), Some(b)) => a.same_location(b) && a.level == b.level,
            (None, None) => true,
            _ => false,
        };
        let masks = location_masks.unwrap_or_default();
        if !same {
            self.change_level(device, queue, key, world, mask);
        }
        let mut built = 0;
        if self.kept.mask != Some(*mask) {
            // Built for other faces (a level turned another way, a picture
            // at another pitch): this level's meshes anew whole.
            self.meshes.clear();
            world.take_dirty();
            let chunks = world.chunk_keys();
            built = chunks.len();
            self.remesh_all(device, queue, world, &chunks, mask);
            self.kept.mask = Some(*mask);
        }
        built += self.update_chunks(device, queue, world, mask);
        if self.kept.now.is_some() && (built > 0 || self.kept.shown.is_none()) {
            self.kept.shown = Some(world.clone());
        }
        self.build_ahead(device, queue, &masks);
        built
    }

    /// The picture's world became level `key` (None: one of no location).
    fn change_level(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, key: Option<Key>, world: &mut World, mask: &FaceMask) {
        let (old, shown) = (self.kept.now.take(), self.kept.shown.take());
        let same_location = matches!((&old, &key), (Some(a), Some(b)) if a.same_location(b));
        if !same_location {
            // Leaving the location: what was kept of it goes.
            self.kept.forget();
            self.kept.full = false;
        }
        let mut put_aside = false;
        if let (true, Some(old), Some(shown), Some(built)) = (same_location, &old, shown, self.kept.mask) {
            let meshes = std::mem::take(&mut self.meshes);
            let lamps = self.lamps.put_aside();
            let bytes = mesh_bytes(&meshes) + lamps.bytes();
            self.kept.aside.retain(|a| a.level != old.level);
            self.kept.aside.push(Aside { level: old.level, world: shown, mask: built, meshes, lamps: Some(lamps), bytes });
            put_aside = true;
        }
        // Taken up only with the faces it is shown with now (else built
        // anew, as it is entered).
        let taken = key.as_ref().and_then(|k| self.kept.aside.iter().position(|a| a.level == k.level)).map(|i| self.kept.aside.remove(i)).filter(|a| a.mask == *mask);
        if put_aside {
            // (The meshes are empty: `world` has every chunk to build.)
            self.kept.mask = Some(*mask);
        }
        match taken {
            Some(a) => {
                (self.meshes, self.kept.mask) = (a.meshes, Some(a.mask));
                // What changed since it was put aside, its neighbours too.
                world.take_dirty();
                world.mark_changed_since(&a.world);
                self.lamps.take_up(a.lamps);
            }
            // Built as it is entered, its lamps' walls and memory as a fresh
            // renderer makes them (`world` has every chunk to build).
            None if put_aside => self.lamps.take_up(None),
            None => {}
        }
        // The faces' memory of their shadow is by place: another level's
        // never shows here.
        if old.is_some() || key.is_some() {
            self.forget_shadows(device, queue);
        }
        self.kept.now = key;
        self.keep_to_budget();
    }

    /// The levels put aside within `KEPT_BYTES`: the one taken up longest
    /// ago let go first, said once.
    fn keep_to_budget(&mut self) {
        while self.kept.bytes() > self.kept.most && !self.kept.aside.is_empty() {
            self.kept.aside.remove(0);
            if !self.kept.full {
                self.kept.full = true;
                self.said.push(format!("levels kept built: the location's levels need more than the {} MB kept on the GPU; some are built again as they are entered", self.kept.most >> 20));
            }
        }
    }

    /// A level of the location not built yet meshed ahead on a thread of its
    /// own, then sent a few chunks a picture and put aside.
    fn build_ahead(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, masks: &[FaceMask]) {
        let Some(key) = self.kept.now.clone() else { return };
        if let Some(ahead) = &mut self.kept.ahead {
            if ahead.job.as_ref().is_some_and(|j| !j.is_finished()) {
                return;
            }
            if let Some(job) = ahead.job.take() {
                ahead.meshed = job.join().unwrap_or_default().into();
            }
            for _ in 0..AHEAD_CHUNKS {
                let Some((cc, vertices, indices)) = ahead.meshed.pop_front() else { break };
                upload_into(&mut ahead.meshes, device, queue, cc, &vertices, &indices);
            }
            if !ahead.meshed.is_empty() {
                return;
            }
            let Ahead { level, world, mask, meshes, .. } = self.kept.ahead.take().expect("there");
            if level != key.level && !self.kept.aside.iter().any(|a| a.level == level) {
                let bytes = mesh_bytes(&meshes);
                self.kept.aside.insert(0, Aside { level, world, mask, meshes, lamps: None, bytes });
                self.keep_to_budget();
            }
            return;
        }
        if self.kept.full {
            return;
        }
        let levels = &key.levels;
        let next = levels.list.iter().enumerate().find(|(i, l)| *i != key.level && l.location.as_deref() == Some(key.name.as_str()) && !self.kept.aside.iter().any(|a| a.level == *i));
        let Some((level, l)) = next else { return };
        // Its own faces (its own camera's yaw).
        let Some(&mask) = masks.get(level) else { return };
        let mut world = l.world.clone();
        world.take_dirty();
        let shared = world.clone();
        let job = std::thread::Builder::new().name("level ahead".into()).spawn(move || {
            shared
                .chunk_keys()
                .into_iter()
                .map(|cc| {
                    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
                    crate::mesh::mesh_chunk(&shared, cc, &mask, &mut vertices, &mut indices);
                    (cc, vertices, indices)
                })
                .collect()
        });
        // (Without a thread it is built as it is entered.)
        if let Ok(job) = job {
            self.kept.ahead = Some(Ahead { level, world, mask, job: Some(job), meshed: VecDeque::new(), meshes: CellMap::default() });
        }
    }

    /// The faces' memory of their shadow emptied, as a fresh renderer's.
    fn forget_shadows(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if let super::pipelines::Memory::Ready { faces, .. } = &self.memory {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("the faces' memory emptied") });
            encoder.clear_buffer(faces, 0, None);
            queue.submit([encoder.finish()]);
        }
    }

    /// How many levels are put aside built, and how many bytes they hold:
    /// the tests'.
    #[cfg(test)]
    pub(crate) fn kept_levels(&self) -> (usize, u64) {
        (self.kept.aside.len(), self.kept.bytes())
    }

    /// The most bytes the levels put aside may hold: the tests'.
    #[cfg(test)]
    pub(crate) fn keep_at_most(&mut self, bytes: u64) {
        self.kept.most = bytes;
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::session::{Scene, Session};
    use glam::IVec3;

    /// A station of two rooms of blocks (32 by 32 by 12 voxels, a floor
    /// and a wall each, the walls on either side of a lamp at the same
    /// place in both), fog, this `main.lua`; room b's camera turned `yaw`.
    fn station(name: &str, main: &str, yaw: Option<i32>) -> crate::scratch::Scratch {
        use crate::grid::{Cell, GridLevel};
        let dir = crate::lua::tests::levels_folder(&format!("kept-{name}"), main, &[("a", &[]), ("b", &[])]);
        std::fs::write(
            dir.join("conf.lua"),
            "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 16, 16 }\n  c.light.fog_density = 0.03\n  c.light.ambient = 0.05\n  c.light.sun_color = { 12.75, 12.75, 12.75 }\n  c.camera.pitch = 60\n  c.camera.view_width = 40\nend\n",
        )
        .unwrap();
        for (room, wall) in [("a", 2), ("b", 5)] {
            let path = dir.join("levels").join(format!("{room}.toml"));
            let mut level = GridLevel::read(&path).unwrap();
            let rock = level.cells[0];
            (level.size, level.cells) = (IVec3::new(8, 8, 3), vec![Cell::default(); 8 * 8 * 3]);
            for y in 0..8 {
                for x in 0..8 {
                    level.cells[(x + y * 8) as usize] = rock;
                }
                for z in 1..3 {
                    level.cells[(wall + y * 8 + z * 64) as usize] = rock;
                }
            }
            level.location = Some("station".into());
            if room == "b" {
                level.camera = yaw.map(|yaw| crate::grid::LevelCamera { yaw, line: 0 });
            }
            level.save(&path).unwrap();
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, text + "\n[[thing]]\nname = \"lamp\"\nat = [15, 16, 6]\nlight = { radius = 18, shadows = \"static\" }\n").unwrap();
        }
        dir
    }

    const MAIN: &str = r##"
function on_level_start() level.camera:look_at(16, 16) end
function on_draw(frame) frame:daylight({ sun = { -0.6, 0.2, -0.5 }, sun_color = { 200, 200, 200 } }) end
function on_tick()
  local t = world.tick
  if t == 3 then world:go_to("b", { fade = 0 }) end
  if t == 6 then
    level:set_voxel(18, 20, 4, "#ff2000")
    world:go_to("a", { fade = 0 })
  end
  if t == 9 then world:go_to("b", { fade = 0 }) end
end
"##;

    /// `pictures` pictures of the game as it stands, the last one's pixels.
    fn pictures(game: &Session<crate::Voxcade>, canvas: &mut crate::headless::Canvas, pictures: usize) -> Vec<u8> {
        let mut last = Vec::new();
        for _ in 0..pictures {
            let frame = game.compose(1.0, 16.0 / 9.0, Scene::Play, None);
            last = canvas.draw(&frame).unwrap();
        }
        last
    }

    /// On the GPU: a level of a location taken up again from those put
    /// aside draws byte for byte as a fresh renderer draws it: its meshes,
    /// a voxel set as it was left (never drawn before it was put aside),
    /// its lamps' walls and memory (the other room's, its wall on the
    /// other side of a lamp at the same place, never shows), the faces'
    /// memory of a sun the game moves, and its fog. A level file read anew
    /// lets what was kept go.
    #[test]
    fn a_level_taken_up_again_draws_as_a_fresh_renderer_draws_it() {
        let dir = station("swap", MAIN, None);
        let mut game = Session::<crate::Voxcade>::load(&dir).unwrap();
        let mut canvas = crate::headless::Canvas::new(320, 180, false).unwrap();
        canvas.mesh_all(&mut game);
        for _ in 0..12 {
            game.step(&crate::input::Input::default());
            canvas.remesh(&mut game);
            pictures(&game, &mut canvas, 2);
        }
        assert_eq!(game.game.halted(), None);
        assert_eq!(game.game.inspect()["at"]["name"], "b");
        assert_ne!(game.game.sim().world.get(IVec3::new(18, 20, 4)), 0, "the voxel set as b was left");
        assert_eq!(canvas.renderer.kept_levels().0, 1, "a put aside built");
        let swapped = pictures(&game, &mut canvas, 3);
        let mut fresh = crate::headless::Canvas::new(320, 180, false).unwrap();
        fresh.mesh_all(&mut game);
        let first = pictures(&game, &mut fresh, 3);
        assert!(swapped == first, "the picture after the swap is a fresh renderer's");
        // A level file read anew: what was built of the location goes.
        let file = dir.join("levels/a.toml");
        std::fs::write(&file, std::fs::read_to_string(&file).unwrap() + "\n[[place]]\nname = \"door\"\nat = [4, 4, 4]\n").unwrap();
        game.reload(&file).unwrap();
        canvas.remesh(&mut game);
        assert_eq!(canvas.renderer.kept_levels().0, 0, "the levels read anew are built anew");
    }

    /// On the GPU: two rooms of a location seen from either side (room b's
    /// `[camera] yaw = 180`) keep each its own faces: a change between them
    /// swaps without building a chunk, and draws as a fresh renderer does.
    #[test]
    fn rooms_turned_different_ways_keep_their_own_faces() {
        let main = "function on_level_start() level.camera:look_at(16, 16) end\nfunction on_tick()\n  if world.tick % 3 == 0 then world:go_to(level.name == 'a' and 'b' or 'a', { fade = 0 }) end\nend\n";
        let dir = station("yaw", main, Some(180));
        let mut game = Session::<crate::Voxcade>::load(&dir).unwrap();
        let mut canvas = crate::headless::Canvas::new(320, 180, false).unwrap();
        canvas.mesh_all(&mut game);
        let mut built = Vec::new();
        for _ in 0..13 {
            game.step(&crate::input::Input::default());
            let (name, mask) = (game.game.inspect()["at"]["name"].as_str().unwrap_or_default().to_string(), game.face_mask());
            built.push((name, mask, canvas.remesh(&mut game)));
            pictures(&game, &mut canvas, 1);
        }
        assert_eq!(game.game.halted(), None);
        let mask_of = |room: &str| built.iter().find(|(n, ..)| n == room).map(|b| b.1);
        assert_ne!(mask_of("a"), mask_of("b"), "room b has faces of its own: {built:?}");
        // From the second change between them on, nothing is built.
        assert!(built.iter().skip(7).all(|b| b.2 == 0), "{built:?}");
        let swapped = pictures(&game, &mut canvas, 3);
        let mut fresh = crate::headless::Canvas::new(320, 180, false).unwrap();
        fresh.mesh_all(&mut game);
        assert!(swapped == pictures(&game, &mut fresh, 3), "the picture after the swap is a fresh renderer's");
    }

    /// The levels put aside past the budget: the one taken up longest ago
    /// let go, said once.
    #[test]
    fn levels_past_the_budget_are_let_go_and_said() {
        let dir = station("budget", MAIN, None);
        let mut game = Session::<crate::Voxcade>::load(&dir).unwrap();
        let mut canvas = crate::headless::Canvas::new(320, 180, false).unwrap();
        canvas.renderer.keep_at_most(1);
        canvas.mesh_all(&mut game);
        for _ in 0..12 {
            game.step(&crate::input::Input::default());
            canvas.remesh(&mut game);
            pictures(&game, &mut canvas, 1);
        }
        assert_eq!(canvas.renderer.kept_levels().0, 0);
        let said = canvas.renderer.warnings();
        assert_eq!(said.iter().filter(|w| w.starts_with("levels kept built")).count(), 1, "{said:?}");
    }
}
