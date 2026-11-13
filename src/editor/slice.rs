//! SLICE (MagicaVoxel's and Goxel's way into a model): a plane across one
//! axis at a layer; what lies on its near side (towards the camera) is
//! neither drawn nor met by the tools, so they work on the cut face and
//! what lies behind it. The window's alone: the file never knows. A
//! block, a model's frame and a figure alike (every part cut by the same
//! plane, in the figure's own box), one arrow along the axis to drag it.

use glam::{IVec3, Vec3};

use super::handles::{self, Drag, Grip, Handles};
use super::model::{Editor, Scope};

/// The plane: across `axis` at layer `at` (in the model's box, or the
/// figure's), the side cut away (+1: above `at`, -1: below), and the drag
/// of its arrow going on (the layer it began at).
#[derive(Clone, Debug)]
pub struct Slice {
    pub axis: usize,
    pub at: i32,
    pub side: i32,
    pub drag: Option<(Drag, Handles, i32)>,
}

/// How big the slice's arrow is: a share of the view's height.
const SLICE_HANDLE: f32 = 0.09;

impl Editor {
    /// Where the shown grid's cell 0 lies in the box the slice cuts (a
    /// figure's when a part is shaped).
    fn slice_shift(&self) -> IVec3 {
        self.figure_box().map_or(IVec3::ZERO, |(lo, _)| lo)
    }

    /// The box the slice cuts: the figure's, else the grid shown.
    pub fn slice_room(&self) -> IVec3 {
        self.figure_box().map_or_else(|| self.grid(), |(_, size)| size)
    }

    /// Whether the slice can be had here (a block, a frame, a figure).
    pub fn can_slice(&self) -> bool {
        matches!(self.scope, Scope::Block(_) | Scope::Frame(_) | Scope::Figure)
    }

    /// Whether cell `c` of the grid shown lies on the side the slice cuts
    /// away.
    pub fn cut(&self, c: IVec3) -> bool {
        let Some(s) = self.slice.as_ref().filter(|_| self.can_slice()) else { return false };
        let m = c - self.slice_shift();
        s.side * (m[s.axis] - s.at) > 0
    }

    /// SLICE along `axis` (the same again, or None: off), at the middle of
    /// the box at first.
    pub fn set_slice(&mut self, axis: Option<usize>) {
        self.slice = match (axis, &self.slice) {
            (Some(a), Some(s)) if s.axis == a => None,
            (Some(a), _) => Some(Slice { axis: a, at: self.slice_room()[a] / 2, side: 1, drag: None }),
            (None, _) => None,
        };
    }

    /// The slice's layer (kept inside the box).
    pub fn set_slice_at(&mut self, at: i32) {
        let room = self.slice_room();
        if let Some(s) = &mut self.slice {
            s.at = at.clamp(0, (room[s.axis] - 1).max(0));
        }
    }

    /// The side cut away: the one the eye (in the grid shown) is on.
    pub fn slice_toward(&mut self, eye: Vec3) {
        let shift = self.slice_shift().as_vec3();
        if let Some(s) = &mut self.slice {
            s.side = if eye[s.axis] - shift[s.axis] >= s.at as f32 + 0.5 { 1 } else { -1 };
        }
    }

    /// The slice's arrow: at the middle of the cut face, along its axis.
    pub fn slice_handles(&self, view_proj: glam::Mat4) -> Option<Handles> {
        let s = self.slice.as_ref().filter(|_| self.can_slice())?;
        let mut centre = self.slice_shift().as_vec3() + self.slice_room().as_vec3() / 2.0;
        centre[s.axis] = self.slice_shift()[s.axis] as f32 + s.at as f32 + 0.5;
        let mut h = Handles::at(centre, handles::world_size(view_proj, centre, SLICE_HANDLE));
        let mut way = Vec3::ZERO;
        way[s.axis] = 1.0;
        h.arrows[s.axis] = Some(way);
        // Only the head grabs: the tools work on the cut face round it.
        h.heads_only = true;
        Some(h)
    }

    /// The press on the slice's arrow: dragged until let go.
    pub fn grab_slice(&mut self, drag: Drag, h: Handles) {
        if let Some(s) = &mut self.slice {
            s.drag = Some((drag, h, s.at));
        }
    }

    /// Whether the slice's arrow is being dragged.
    pub fn slicing(&self) -> bool {
        self.slice.as_ref().is_some_and(|s| s.drag.is_some())
    }

    /// The mouse dragged on with the slice's arrow: the layer by whole
    /// voxels from where it was pressed.
    pub fn drag_slice(&mut self, view_proj: glam::Mat4, screen: glam::Vec2, mouse: glam::Vec2, ray: (Vec3, Vec3)) {
        let Some((at, amount)) = self.slice.as_mut().and_then(|s| s.drag.as_mut().map(|(d, h, start)| (*start, d.amount(h, view_proj, screen, mouse, ray)))) else { return };
        self.set_slice_at(at + amount.round() as i32);
    }

    /// The slice's arrow let go of.
    pub fn let_go_of_slice(&mut self) {
        if let Some(s) = &mut self.slice {
            s.drag = None;
        }
    }

    /// The slice's arrow under the mouse (pixels in a room `screen` big).
    pub fn slice_grip(&self, view_proj: glam::Mat4, screen: glam::Vec2, at: glam::Vec2) -> Option<(Grip, Handles)> {
        let h = self.slice_handles(view_proj)?;
        Some((h.grip_at(view_proj, screen, at)?, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::tests::{N, folder};

    /// A block sliced at z 1 from above: the cells above are not drawn and
    /// not met by the tools, which work on the cut face; from below the
    /// side under it goes; off, all of it again. The file is not touched.
    #[test]
    fn a_slice_hides_the_near_side_from_the_tools() {
        let dir = folder("slice");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        ed.open_block(0);
        let file = crate::blocks::path(&dir, "1");
        let text = std::fs::read_to_string(&file).unwrap();
        let down = |ed: &Editor| ed.aim(Vec3::new(1.5, 1.5, 50.0), Vec3::NEG_Z).map(|h| h.cell);
        assert_eq!(down(&ed), Some(IVec3::new(1, 1, N - 1)), "the top");
        ed.set_slice(Some(2));
        ed.set_slice_at(1);
        ed.slice_toward(Vec3::new(1.5, 1.5, 50.0));
        assert!(ed.cut(IVec3::new(1, 1, 2)) && !ed.cut(IVec3::new(1, 1, 1)));
        assert_eq!(down(&ed), Some(IVec3::new(1, 1, 1)), "the cut face");
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        assert!(shown.world.get(IVec3::new(1, 1, 2)) == 0 && shown.world.get(IVec3::new(1, 1, 1)) != 0, "not drawn above the cut");
        // From below, the side under the plane goes.
        ed.slice_toward(Vec3::new(1.5, 1.5, -50.0));
        assert!(ed.cut(IVec3::new(1, 1, 0)) && !ed.cut(IVec3::new(1, 1, 2)));
        ed.set_slice(Some(2));
        assert!(ed.slice.is_none() && !ed.cut(IVec3::new(1, 1, 2)), "the same axis again: off");
        assert_eq!(down(&ed), Some(IVec3::new(1, 1, N - 1)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "the file untouched");
    }

    /// A figure sliced: every part cut by the same plane, in the figure's
    /// box, whichever part is shaped; the parts drawn as cubes show their
    /// inside where the cut opens them, nothing on the cut side.
    #[test]
    fn a_figure_is_cut_by_one_plane() {
        let dir = crate::scratch::Scratch::new("slice-figure");
        let text = format!("format = \"{}\"\nsize = [4, 4, 6]\n\n[palette]\na = \"#ff0000\"\nb = \"#0000ff\"\n\n[[part]]\nname = \"body\"\nat = [-2, -2, 0]\nlayers = \"\"\"\naaaa\naaaa\naaaa\naaaa\n*4\n\"\"\"\n\n[[part]]\nname = \"head\"\nat = [-1, -1, 4]\nlayers = \"\"\"\nbb\nbb\n*2\n\"\"\"\n", crate::vvx::FORMAT);
        std::fs::write(dir.join("fig.vvx"), text).unwrap();
        let mut ed = Editor::open_model(&dir, "fig.vvx").unwrap();
        ed.follow_figure();
        let head = ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == "head").unwrap();
        ed.choose_part(head);
        ed.set_slice(Some(0));
        ed.set_slice_at(1);
        ed.slice_toward(Vec3::new(-50.0, 2.0, 2.0));
        // In the figure's box x 0 and below go (the eye on that side).
        let (lo, _) = ed.figure_box().unwrap();
        assert!(ed.cut(lo + IVec3::new(0, 1, 1)) && !ed.cut(lo + IVec3::new(1, 1, 1)));
        // The body, drawn as cubes round the head: none at x 0, its
        // inside at x 1 (the cut face), not its middle further in.
        let figure = ed.figure.as_ref().unwrap();
        // (Dimmed round the part shaped: the see-through ones it gives back.)
        let mut cubes = Vec::new();
        let through = figure.cubes(lo, &|c| ed.cut(c), &mut cubes);
        cubes.extend(through);
        let at = |c: &crate::render::Instance| Vec3::from_slice(&c.pos_size[..3]).floor().as_ivec3() - lo;
        assert!(cubes.iter().all(|c| at(c).x >= 1), "nothing on the cut side");
        assert!(cubes.iter().any(|c| at(c) == IVec3::new(1, 1, 1)), "the cut face, inside the body");
        assert!(!cubes.iter().any(|c| at(c) == IVec3::new(2, 1, 1)), "not the body's middle behind it");
        // The head's own cells (the world shown) are cut the same.
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        assert!(shown.world.get(IVec3::new(0, 0, 0)) != 0, "the head's x 1 in the figure: kept");
        ed.set_slice_at(2);
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        assert!(shown.world.get(IVec3::new(0, 0, 0)) == 0 && shown.world.get(IVec3::new(1, 0, 0)) != 0, "moved a layer: the head's first column gone");
    }
}
