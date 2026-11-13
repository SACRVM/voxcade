//! Shaping in place (the owner's wish from a game's room): EDIT on a piece
//! in the level editor opens its model with the level's other pieces
//! drawn faint round it, as they stand relative to this piece. The model
//! stays upright as always; the room is turned instead (the piece's
//! rotation, pitch and roll undone). Only drawn: never met by the tools,
//! never written. CONTEXT in the top bar shows it or not; the level's
//! hide marks of the moment hold; the level file read anew when it
//! changes (`disk.rs`).

use glam::{IVec2, IVec3, Vec3};

use super::model::Editor;
use super::model::room::{self, Hidden, Item};
use super::view::Figures;
use crate::grid::GridLevel;
use crate::render::Instance;

/// How opaque the room is round the model: faint, so that it never hides
/// what is being shaped. Each piece keeps the faces between its own
/// voxels left out (`model_cubes` marks them): a piece turned freely is
/// its surface, not stripes of every cube's faces.
const FAINT: u8 = 100;

/// What the level editor hands the model's window for EDIT on a piece:
/// the level's file, the piece (its place in the list) and what the level
/// editor hides.
#[derive(Clone, Debug)]
pub struct Seed {
    pub level: String,
    pub piece: usize,
    pub hidden: Hidden,
}

/// The level round the model: its file, as read, the piece shaped (its
/// place, its name to find it again), what is hidden, and CONTEXT.
#[derive(Clone, Debug)]
pub struct Context {
    pub file: String,
    pub level: GridLevel,
    pub piece: usize,
    pub name: Option<String>,
    pub hidden: Hidden,
    pub shown: bool,
}

impl Context {
    /// The level in the file `file` of the game in `dir` round its piece
    /// `piece`.
    pub fn read(dir: &std::path::Path, seed: Seed) -> anyhow::Result<Self> {
        let level = GridLevel::read(&dir.join(&seed.level))?;
        let t = level.things.get(seed.piece).filter(|t| t.model.is_some()).ok_or_else(|| anyhow::anyhow!("{}: no piece {} to shape in place", seed.level, seed.piece + 1))?;
        let name = t.name.clone();
        Ok(Self { file: seed.level, level, piece: seed.piece, name, hidden: seed.hidden, shown: true })
    }

    /// The level read anew (another program changed it): the piece found
    /// again by its name, else at its place; false where it is gone.
    pub fn reread(&mut self, level: GridLevel) -> bool {
        let found = match &self.name {
            Some(n) => level.things.iter().position(|t| t.name.as_ref() == Some(n)),
            None => (self.piece < level.things.len()).then_some(self.piece),
        };
        self.level = level;
        match found.filter(|&i| self.level.things[i].model.is_some()) {
            Some(i) => {
                self.piece = i;
                true
            }
            None => false,
        }
    }

    /// The models the room needs (their files).
    pub fn models(&self) -> impl Iterator<Item = &str> {
        self.level.things.iter().filter_map(|t| t.model.as_deref())
    }
}

impl Editor {
    /// Opens the level round the model (EDIT on a piece of it).
    pub fn set_context(&mut self, seed: Seed) -> anyhow::Result<()> {
        self.context = Some(Context::read(&self.dir, seed)?);
        Ok(())
    }

    /// CONTEXT: the room shown, or not.
    pub fn toggle_context(&mut self) {
        if let Some(c) = &mut self.context {
            c.shown = !c.shown;
        }
    }

    /// The room round the model as faint cubes in the grid shown: every
    /// other piece of a model not hidden, placed and turned relative to the
    /// piece shaped (its turn undone: the model stands upright), all
    /// see-through (joined as the game joins glass), and moved by `seen`
    /// (into the grid the view draws in). The view draws them before the
    /// model (`FrameData::behind`): the model always lies over them in its
    /// own colors (the owner's finding: the gold frame read brown).
    pub fn context_cubes(&self, figures: &Figures, seen: glam::Mat4, glass_out: &mut Vec<Instance>) {
        let (_, seen_turn, _) = seen.to_scale_rotation_translation();
        let Some(c) = self.context.as_ref().filter(|c| c.shown) else { return };
        let Some(p) = c.level.things.get(c.piece) else { return };
        let Some(pm) = p.model.as_deref().and_then(|m| figures.model(m)) else { return };
        let size_p = pm.size;
        let (about_p, back) = (room::centre_in(Some(size_p)), room::rotation_of(p).inverse());
        // The grid shown: the model's box from its low corner (a figure's
        // part shaped: the figure's box where it lies in it).
        let pivot = IVec3::new(size_p.x / 2, size_p.y / 2, 0).as_vec3();
        let shift = self.figure_box().map_or(Vec3::ZERO, |(lo, _)| lo.as_vec3());
        let at_p = IVec3::from(p.at).as_vec3();
        let north = IVec2::from(crate::grid::Way::North.pair());
        let hidden = |i: usize| c.hidden.hides(&c.level, Item::Thing(i));
        for (i, q) in c.level.things.iter().enumerate() {
            if i == c.piece || hidden(i) {
                continue;
            }
            let Some(qm) = q.model.as_deref().and_then(|m| figures.model(m)) else { continue };
            let about_q = room::centre_in(Some(qm.size));
            // (Turned alike, as most of a room is: on the grid, exactly.)
            let turn = back * room::rotation_of(q);
            let turn = if turn.dot(glam::Quat::IDENTITY).abs() > 1.0 - 1e-6 { glam::Quat::IDENTITY } else { turn };
            let origin = pivot + shift + about_p + back * (IVec3::from(q.at).as_vec3() + about_q - at_p - about_p) - about_q;
            let near = origin.round();
            let origin = if (origin - near).abs().max_element() < 1e-3 { near } else { origin };
            // Seen from a part shaped at rest: into its own grid.
            let (origin, turn) = if seen == glam::Mat4::IDENTITY { (origin, turn) } else { (seen.transform_point3(origin + about_q) - about_q, seen_turn * turn) };
            let (mut opaque, mut glass) = (Vec::new(), Vec::new());
            crate::session::model_cubes(qm, 0, origin, north, turn, &mut opaque, &mut glass);
            let faint = |c: Instance| Instance { fx: [c.fx[0], c.fx[1].min(FAINT), c.fx[2], c.fx[3]], ..c };
            glass_out.extend(opaque.into_iter().chain(glass).map(faint));
        }
    }
}

impl Hidden {
    /// Whether this hides the thing `item` of `level` (as the level editor
    /// that made it did).
    pub fn hides(&self, level: &GridLevel, item: Item) -> bool {
        self.hides_in(level, item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model's `.vvx` text: `size`, one color, filled where `filled` says.
    fn model_text(size: IVec3, filled: impl Fn(IVec3) -> bool) -> String {
        let layers: Vec<String> = (0..size.z).map(|z| (0..size.y).rev().map(|y| (0..size.x).map(|x| if filled(IVec3::new(x, y, z)) { 'a' } else { '.' }).collect::<String>()).collect::<Vec<_>>().join("\n")).collect();
        format!("format = \"{}\"\nsize = [{}, {}, {}]\n\n[palette]\na = \"#808080\"\n\n[[frame]]\nlayers = \"\"\"\n{}\n\"\"\"\n", crate::vvx::FORMAT, size.x, size.y, size.z, layers.join("\n\n"))
    }

    /// A room of a wall and a picture before it, both facing `way`; the
    /// picture's model open in place.
    fn in_place(name: &str, way: &str) -> (Editor, Figures, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("context-{name}"));
        std::fs::write(dir.join("wall.vvx"), model_text(IVec3::new(60, 8, 40), |p| p.y >= 6 || p.x < 6 || p.x >= 54 || p.z < 6 || p.z >= 34)).unwrap();
        std::fs::write(dir.join("picture.vvx"), model_text(IVec3::new(20, 2, 14), |_| true)).unwrap();
        std::fs::write(dir.join("room.toml"), room(way, 100)).unwrap();
        let mut ed = Editor::open_model(&dir, "picture.vvx").unwrap();
        ed.set_context(Seed { level: "room.toml".into(), piece: 1, hidden: Hidden::default() }).unwrap();
        ed.remember_held();
        let mut figures = Figures::load(&dir, &crate::config::GameConfig::default());
        figures.follow(&dir, &ed);
        (ed, figures, dir)
    }

    fn room(way: &str, wall_x: i32) -> String {
        format!("format = \"voxcade level 1\"\nsize = [260, 200, 80]\n\n[[thing]]\nname = \"wall\"\nmodel = \"wall.vvx\"\nat = [{wall_x}, 100, 0]\nfacing = \"{way}\"\n\n[[thing]]\nname = \"picture\"\nmodel = \"picture.vvx\"\nat = [100, 100, 13]\nfacing = \"{way}\"\n")
    }

    fn cubes(ed: &Editor, figures: &Figures) -> Vec<Vec3> {
        let mut out = Vec::new();
        ed.context_cubes(figures, glam::Mat4::IDENTITY, &mut out);
        out.iter().map(|c| Vec3::from_slice(&c.pos_size[..3])).collect()
    }

    /// The room stands round the model as it stands round the piece: the
    /// wall's corner voxel where the level puts it relative to the picture,
    /// faint; the same with both turned east (the model upright, the room
    /// turned with it); a hidden piece or CONTEXT off: none.
    #[test]
    fn the_room_stands_round_the_model_as_round_its_piece() {
        let (mut ed, figures, _dir) = in_place("stands", "north");
        // The wall's voxel (0, 7, 0): 30 left, 3 back from its pivot at
        // (100, 100, 0); the picture's pivot cell (10, 1, 0) at (100, 100, 13).
        let corner = Vec3::new(-20.0, 4.0, -13.0) + 0.5;
        let all = cubes(&ed, &figures);
        assert!(all.contains(&corner), "the wall's corner where it stands");
        let mut faint = Vec::new();
        ed.context_cubes(&figures, glam::Mat4::IDENTITY, &mut faint);
        assert!(faint.iter().all(|c| c.fx[1] <= FAINT), "faint");
        ed.toggle_context();
        assert!(cubes(&ed, &figures).is_empty(), "CONTEXT off");
        let (ed, figures, _dir) = in_place("east", "east");
        assert!(cubes(&ed, &figures).contains(&corner), "both turned: the same round the upright model");
        // Hidden in the level editor: not there.
        let (mut ed, figures, _dir) = in_place("hidden", "north");
        let mut level = Editor::open(&ed.dir, "room.toml").unwrap();
        level.set_hidden(level.find("wall").unwrap(), true);
        ed.set_context(Seed { level: "room.toml".into(), piece: 1, hidden: level.hidden().clone() }).unwrap();
        assert!(cubes(&ed, &figures).is_empty());
    }

    /// The level changed on disk comes in (the room drawn anew), the piece
    /// found by its name; shaping the model never writes the level.
    #[test]
    fn the_room_follows_its_file_and_is_never_written() {
        let (mut ed, figures, dir) = in_place("follows", "north");
        std::fs::write(dir.join("room.toml"), room("north", 110)).unwrap();
        assert!(ed.look_at_disk());
        assert!(cubes(&ed, &figures).contains(&(Vec3::new(-10.0, 4.0, -13.0) + 0.5)), "the wall moved 10 right");
        let text = std::fs::read_to_string(dir.join("room.toml")).unwrap();
        let before = std::fs::read_to_string(dir.join("picture.vvx")).unwrap();
        // (Its color made glass: a step, written.)
        ed.set_material(crate::vvx::Mat { rgba: [128, 128, 128, 200], metal: 0, glow: 0 });
        assert_ne!(std::fs::read_to_string(dir.join("picture.vvx")).unwrap(), before, "the model written");
        assert_eq!(std::fs::read_to_string(dir.join("room.toml")).unwrap(), text, "the level not");
    }

    /// The room is drawn before everything else (`FrameData::behind`),
    /// whole (nothing left out), all of it see-through: the model, drawn
    /// after it, lies over it in its own colors from any side.
    #[test]
    fn the_room_is_drawn_behind_the_model() {
        let (ed, figures, _dir) = in_place("behind", "north");
        let view = crate::editor::view::View::new(&ed, &Default::default());
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        let frame = crate::editor::view::frame(&ed, &view, &shown, &figures, &Default::default(), 1.6);
        assert_eq!(frame.behind, cubes(&ed, &figures).len(), "the whole room, first");
        assert!(frame.instances[..frame.behind].iter().all(|c| c.fx[1] < 255), "see-through");
    }

    /// The room round the model is see-through, its glass too, and casts
    /// no shadow, as no see-through cube does: the sun's map takes only the
    /// opaque cubes after it (it took the faint room, a window's panes
    /// shutting the sun out as a wall does).
    #[test]
    fn the_room_casts_no_shadow() {
        let (ed, figures, _dir) = in_place("shadow", "north");
        let view = crate::editor::view::View::new(&ed, &Default::default());
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        let frame = crate::editor::view::frame(&ed, &view, &shown, &figures, &Default::default(), 1.6);
        assert!(frame.behind > 0, "a room to cast with");
        let casting = frame.shadow_cubes();
        assert!(casting.start as usize >= frame.behind, "the room casts: {casting:?} of {} behind", frame.behind);
        assert!(frame.instances[casting.start as usize..casting.end as usize].iter().all(|c| c.fx[1] == 255), "only opaque cubes cast");
    }
}
