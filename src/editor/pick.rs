//! What a ray meets among a level's pieces, voxel by voxel: a small piece
//! hanging in the hollow of a big one is chosen where the picture shows
//! it, not the big one whose box encloses it (the owner, 2026-11-12: "es
//! wird immer nur die wand dahinter selektiert"). Each model and each
//! kind's figure keeps a grid of its filled cells (`Cells`); a ray is
//! walked through it in the thing's own frame, turned back (quarter turns
//! and free ones alike), and the nearest voxel met wins. Glass counts.

use glam::{IVec3, Vec3};

/// Which cells of a box `size` big are filled.
#[derive(Clone, Debug, Default)]
pub struct Cells {
    pub size: IVec3,
    filled: Vec<bool>,
}

impl Cells {
    /// The cells of `points` (those inside the box).
    pub fn of(size: IVec3, points: impl IntoIterator<Item = IVec3>) -> Self {
        let size = size.max(IVec3::ZERO);
        let mut filled = vec![false; (size.x * size.y * size.z) as usize];
        for p in points {
            if p.cmpge(IVec3::ZERO).all() && p.cmplt(size).all() {
                filled[(p.x + p.y * size.x + p.z * size.x * size.y) as usize] = true;
            }
        }
        Self { size, filled }
    }

    pub fn get(&self, p: IVec3) -> bool {
        p.cmpge(IVec3::ZERO).all() && p.cmplt(self.size).all() && self.filled[(p.x + p.y * self.size.x + p.z * self.size.x * self.size.y) as usize]
    }

    /// The first filled cell a ray meets (in the box's cells, from
    /// `origin` along `dir`): how far along it the ray enters it, and the
    /// way out of the face it enters through (zero when it starts inside
    /// it). Amanatides and Woo's walk, through the box only.
    pub fn hit(&self, origin: Vec3, dir: Vec3) -> Option<(f32, IVec3)> {
        let hi = self.size.as_vec3();
        let (mut enter, mut leave, mut normal) = (0.0f32, f32::INFINITY, IVec3::ZERO);
        for a in 0..3 {
            if dir[a].abs() < 1e-9 {
                if origin[a] < 0.0 || origin[a] >= hi[a] {
                    return None;
                }
                continue;
            }
            let (t0, t1) = ((0.0 - origin[a]) / dir[a], (hi[a] - origin[a]) / dir[a]);
            let (near, far) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
            if near > enter {
                enter = near;
                normal = IVec3::ZERO;
                normal[a] = if dir[a] > 0.0 { -1 } else { 1 };
            }
            leave = leave.min(far);
        }
        if enter > leave || self.size.cmple(IVec3::ZERO).any() {
            return None;
        }
        let start = origin + dir * (enter + 1e-4);
        let mut cell = start.floor().as_ivec3().clamp(IVec3::ZERO, self.size - 1);
        let step = dir.signum().as_ivec3();
        let (mut next, mut delta) = (Vec3::ZERO, Vec3::ZERO);
        for a in 0..3 {
            if dir[a].abs() < 1e-9 {
                next[a] = f32::INFINITY;
                delta[a] = f32::INFINITY;
            } else {
                let edge = if dir[a] > 0.0 { cell[a] + 1 } else { cell[a] } as f32;
                next[a] = (edge - origin[a]) / dir[a];
                delta[a] = 1.0 / dir[a].abs();
            }
        }
        let mut t = enter;
        loop {
            if self.get(cell) {
                return Some((t, normal));
            }
            // The nearest of the three walls ahead.
            let a = if next.x <= next.y && next.x <= next.z {
                0
            } else if next.y <= next.z {
                1
            } else {
                2
            };
            t = next[a];
            if t > leave {
                return None;
            }
            cell[a] += step[a];
            if cell[a] < 0 || cell[a] >= self.size[a] {
                return None;
            }
            next[a] += delta[a];
            normal = IVec3::ZERO;
            normal[a] = -step[a];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_meets_the_first_filled_cell() {
        // A hollow: a back wall at y 3 and a small cell in front at (1, 1, 0).
        let cells = Cells::of(IVec3::new(4, 4, 1), (0..4).map(|x| IVec3::new(x, 3, 0)).chain([IVec3::new(1, 1, 0)]));
        let (t, n) = cells.hit(Vec3::new(1.5, -5.0, 0.5), Vec3::Y).unwrap();
        assert_eq!((t, n), (6.0, IVec3::NEG_Y), "the small one in front");
        let (t, n) = cells.hit(Vec3::new(2.5, -5.0, 0.5), Vec3::Y).unwrap();
        assert_eq!((t, n), (8.0, IVec3::NEG_Y), "beside it the wall");
        assert!(cells.hit(Vec3::new(2.5, -5.0, 0.5), Vec3::NEG_Y).is_none());
        assert!(cells.hit(Vec3::new(9.5, -5.0, 0.5), Vec3::Y).is_none());
    }

    /// A model of one frame as its `.vvx` text: `size`, one grey colour,
    /// filled where `filled` says.
    fn model_text(size: IVec3, filled: impl Fn(IVec3) -> bool) -> String {
        let mut layers = Vec::new();
        for z in 0..size.z {
            let rows: Vec<String> = (0..size.y).rev().map(|y| (0..size.x).map(|x| if filled(IVec3::new(x, y, z)) { 'a' } else { '.' }).collect()).collect();
            layers.push(rows.join("\n"));
        }
        format!("format = \"{}\"\nsize = [{}, {}, {}]\n\n[palette]\na = \"#808080\"\n\n[[frame]]\nlayers = \"\"\"\n{}\n\"\"\"\n", crate::vvx::FORMAT, size.x, size.y, size.z, layers.join("\n\n"))
    }

    /// The owner's case: a wall (60 by 8 by 40: a plate at the back, a rim
    /// in front round a hollow) and a picture (20 by 2 by 14) hanging in
    /// the hollow before the plate, its box inside the wall's; both facing
    /// `way`, at the same spot. The editor on it, the figures loaded.
    fn wall_and_picture(name: &str, way: &str) -> (crate::editor::model::Editor, crate::editor::view::Figures, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("pick-{name}"));
        std::fs::write(dir.join("wall.vvx"), model_text(IVec3::new(60, 8, 40), |p| p.y >= 6 || p.x < 6 || p.x >= 54 || p.z < 6 || p.z >= 34)).unwrap();
        std::fs::write(dir.join("picture.vvx"), model_text(IVec3::new(20, 2, 14), |_| true)).unwrap();
        let level = format!(
            "format = \"voxcade level 1\"\nsize = [260, 200, 80]\n\n[[thing]]\nname = \"wall\"\nmodel = \"wall.vvx\"\nat = [100, 100, 0]\nfacing = \"{way}\"\n\n[[thing]]\nname = \"picture\"\nmodel = \"picture.vvx\"\nat = [100, 100, 13]\nfacing = \"{way}\"\n"
        );
        std::fs::write(dir.join("room.toml"), level).unwrap();
        let mut ed = crate::editor::model::Editor::open(&dir, "room.toml").unwrap();
        let figures = crate::editor::view::Figures::load(&dir, &crate::config::GameConfig::default()).with_pieces(&dir, &ed.level);
        ed.sizes = figures.sizes();
        (ed, figures, dir)
    }

    fn name(ed: &crate::editor::model::Editor, item: Option<crate::editor::model::Item>) -> String {
        item.map_or("nothing".into(), |i| ed.label_of(i).0)
    }

    /// A click chooses the piece whose voxel the ray meets first, as the
    /// picture shows: the picture in the wall's hollow before the wall
    /// whose box encloses it; beside it the wall's plate behind; through
    /// the hollow over the picture nothing of the picture. Turned a quarter
    /// (both facing east), the same from the side.
    #[test]
    fn a_click_chooses_the_voxel_it_meets_not_the_box() {
        use crate::editor::view::item_along;
        let (ed, figures, _dir) = wall_and_picture("front", "north");
        let from_front = |x: f32, z: f32| name(&ed, item_along(&ed, &figures, Vec3::new(x, -50.0, z), Vec3::Y));
        assert_eq!(from_front(100.5, 20.5), "picture", "in the hollow, before the wall's plate");
        assert_eq!(from_front(115.5, 20.5), "wall", "beside it: the plate behind");
        assert_eq!(from_front(72.5, 20.5), "wall", "the rim");
        assert_eq!(from_front(250.5, 20.5), "nothing");
        // From behind the plate comes first.
        assert_eq!(name(&ed, item_along(&ed, &figures, Vec3::new(100.5, 300.0, 20.5), Vec3::NEG_Y)), "wall");
        let (ed, figures, _dir) = wall_and_picture("east", "east");
        let from_left = |y: f32, z: f32| name(&ed, item_along(&ed, &figures, Vec3::new(-50.0, y, z), Vec3::X));
        assert_eq!(from_left(100.5, 20.5), "picture", "turned: still the picture in the hollow");
        assert_eq!(from_left(115.5, 20.5), "wall");
    }

    /// A thing hidden in the editor is neither drawn nor met by a click
    /// (the click goes on to what is behind), and the level's file is not
    /// touched; SHOW ALL shows it again; ONLY THIS hides the rest.
    #[test]
    fn a_hidden_thing_is_not_met_and_the_file_stays() {
        use crate::editor::view::item_along;
        let (mut ed, figures, dir) = wall_and_picture("hidden", "north");
        let text = std::fs::read_to_string(dir.join("room.toml")).unwrap();
        let picture = ed.find("picture").unwrap();
        ed.choose_item(Some(picture));
        ed.toggle_hidden(picture);
        assert!(ed.is_hidden(picture) && ed.chosen.is_none(), "hidden, chosen no more");
        let ray = (Vec3::new(100.5, -50.0, 20.5), Vec3::Y);
        assert_eq!(name(&ed, item_along(&ed, &figures, ray.0, ray.1)), "wall", "through it to the wall");
        let frame = crate::editor::view::frame(&ed, &crate::editor::view::View::new(&ed, &Default::default()), &crate::editor::view::Shown::new(&ed).unwrap(), &figures, &Default::default(), 1.6);
        let drawn = |ed: &crate::editor::model::Editor| crate::editor::view::frame(ed, &crate::editor::view::View::new(ed, &Default::default()), &crate::editor::view::Shown::new(ed).unwrap(), &figures, &Default::default(), 1.6).instances.len();
        assert_eq!(frame.instances.len(), drawn(&ed));
        assert_eq!(std::fs::read_to_string(dir.join("room.toml")).unwrap(), text, "nothing written");
        ed.show_all();
        assert_eq!(name(&ed, item_along(&ed, &figures, ray.0, ray.1)), "picture");
        let shown_all = drawn(&ed);
        assert!(shown_all > frame.instances.len(), "drawn again");
        ed.only(picture);
        assert!(ed.is_hidden(ed.find("wall").unwrap()) && !ed.is_hidden(picture));
        assert_eq!(name(&ed, item_along(&ed, &figures, Vec3::new(115.5, -50.0, 20.5), Vec3::Y)), "nothing", "the wall hidden");
        assert_eq!(std::fs::read_to_string(dir.join("room.toml")).unwrap(), text);
    }

    /// ADD sets down on the face of the voxel met: in front of the
    /// picture, not on the floor behind the wall's box.
    #[test]
    fn add_sets_down_on_the_face_met() {
        let (ed, figures, _dir) = wall_and_picture("set-down", "north");
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        let spot = crate::editor::view::set_down_spot(&ed, &figures, &shown.world, Vec3::new(100.5, -50.0, 20.5), Vec3::Y);
        assert_eq!(spot, Some(IVec3::new(100, 98, 20)), "before the picture's front face (y 99)");
    }

    /// A thing of a kind without a behaviour rotates as a piece does
    /// (ROTATE 90° and its field write `facing`); one with a behaviour does
    /// not; EDIT opens its kind's model.
    #[test]
    fn a_thing_of_a_kind_rotates_and_edits_its_model() {
        let dir = crate::scratch::Scratch::new("pick-kind");
        std::fs::write(dir.join("room.toml"), "format = \"voxcade level 1\"\nsize = [100, 100, 40]\n\n[[thing]]\nname = \"door\"\nkind = \"tuer\"\nat = [10, 10, 0]\n\n[[thing]]\nname = \"bat\"\nkind = \"bat\"\nat = [30, 10, 0]\n").unwrap();
        let mut ed = crate::editor::model::Editor::open(&dir, "room.toml").unwrap();
        ed.still_kinds.insert("tuer".into());
        ed.kind_models.insert("tuer".into(), "door.vvx".into());
        ed.choose_item(ed.find("door"));
        assert!(ed.can_turn());
        ed.turn_item(1);
        let text = std::fs::read_to_string(dir.join("room.toml")).unwrap();
        assert!(text.contains("name = \"door\"\nkind = \"tuer\"\nat = [10, 10, 0]\nfacing = \"west\"\n"), "{text}");
        ed.edit_model();
        assert_eq!(ed.wants_model.as_deref(), Some("door.vvx"));
        ed.choose_item(ed.find("bat"));
        assert!(!ed.can_turn());
        ed.turn_item(1);
        assert_eq!(std::fs::read_to_string(dir.join("room.toml")).unwrap(), text, "a kind its rules turn: nothing");
    }
}
