//! SLICE (MagicaVoxel's and Goxel's way into a model, a CT scan's): a
//! plane across each axis, each on or off on its own, each at a layer;
//! what lies on a plane's cut side is neither drawn nor met by the tools
//! (a voxel cut by any plane is cut), so they work on the cut faces and
//! what lies behind them. A plane's side is the one the eye was on when
//! it was switched on, and stays until FLIP (with three planes a side
//! following the camera would be confusing). The window's alone: the file
//! never knows. A block, a model's frame and a figure alike (every part
//! cut by the same planes, in the figure's own box), an arrow along each
//! plane's axis to drag it.

use glam::{IVec3, Vec3};

use super::handles::{self, Drag, Grip, Handles};
use super::model::{Editor, Scope};

/// A plane: across `axis` at layer `at` (in the model's box, or the
/// figure's), the side cut away (+1: above `at`, -1: below), and the drag
/// of its arrow going on (the layer it began at).
#[derive(Clone, Debug)]
pub struct Slice {
    pub axis: usize,
    pub at: i32,
    pub side: i32,
    pub drag: Option<(Drag, Handles, i32)>,
}

/// The planes, one an axis (None: off), and the one switched on last
/// (what Page Up and Page Down move).
#[derive(Clone, Debug, Default)]
pub struct Slices {
    pub planes: [Option<Slice>; 3],
    pub last: Option<usize>,
}

impl Slices {
    /// The planes on.
    pub fn on(&self) -> impl Iterator<Item = &Slice> {
        self.planes.iter().flatten()
    }

    /// Whether any plane is on.
    pub fn any(&self) -> bool {
        self.planes.iter().any(Option::is_some)
    }
}

/// How big a plane's arrow is: a share of the view's height.
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

    /// Whether cell `c` of the grid shown lies on the side a plane cuts
    /// away (any plane).
    pub fn cut(&self, c: IVec3) -> bool {
        if !self.can_slice() || !self.slices.any() {
            return false;
        }
        let m = c - self.slice_shift();
        self.slices.on().any(|s| s.side * (m[s.axis] - s.at) > 0)
    }

    /// The plane across `axis` switched on (at the middle of the box, the
    /// side the eye at `eye`, in the grid shown, is on cut away), or off.
    pub fn set_slice(&mut self, axis: usize, eye: Vec3) {
        let a = axis.min(2);
        if self.slices.planes[a].take().is_some() {
            if self.slices.last == Some(a) {
                self.slices.last = (0..3).rev().find(|&b| self.slices.planes[b].is_some());
            }
            return;
        }
        let at = self.slice_room()[a] / 2;
        let side = if eye[a] - self.slice_shift()[a] as f32 >= at as f32 + 0.5 { 1 } else { -1 };
        self.slices.planes[a] = Some(Slice { axis: a, at, side, drag: None });
        self.slices.last = Some(a);
    }

    /// FLIP: the plane across `axis` cuts the other side away.
    pub fn flip_slice(&mut self, axis: usize) {
        if let Some(s) = self.slices.planes.get_mut(axis).and_then(Option::as_mut) {
            s.side = -s.side;
        }
    }

    /// The layer of the plane across `axis` (kept inside the box).
    pub fn set_slice_at(&mut self, axis: usize, at: i32) {
        let room = self.slice_room();
        if let Some(s) = self.slices.planes.get_mut(axis).and_then(Option::as_mut) {
            s.at = at.clamp(0, (room[s.axis] - 1).max(0));
        }
    }

    /// The plane switched on last moved by `by` layers (Page Up, Page
    /// Down).
    pub fn step_slice(&mut self, by: i32) {
        if let Some(a) = self.slices.last
            && let Some(at) = self.slices.planes[a].as_ref().map(|s| s.at)
        {
            self.set_slice_at(a, at + by);
        }
    }

    /// Each plane's arrow: at the middle of its cut face, along its axis.
    pub fn slice_handles(&self, view_proj: glam::Mat4) -> Vec<(usize, Handles)> {
        if !self.can_slice() {
            return Vec::new();
        }
        let (shift, room) = (self.slice_shift().as_vec3(), self.slice_room().as_vec3());
        self.slices
            .on()
            .map(|s| {
                let mut centre = shift + room / 2.0;
                centre[s.axis] = shift[s.axis] + s.at as f32 + 0.5;
                let mut h = Handles::at(centre, handles::world_size(view_proj, centre, SLICE_HANDLE));
                let mut way = Vec3::ZERO;
                way[s.axis] = 1.0;
                h.arrows[s.axis] = Some(way);
                // Only the head grabs: the tools work on the cut face round it.
                h.heads_only = true;
                (s.axis, h)
            })
            .collect()
    }

    /// The press on a plane's arrow (its grip names its axis): dragged
    /// until let go.
    pub fn grab_slice(&mut self, drag: Drag, grip: Grip, h: Handles) {
        if let Some(s) = self.slices.planes.get_mut(grip.axis()).and_then(Option::as_mut) {
            s.drag = Some((drag, h, s.at));
        }
    }

    /// Whether a plane's arrow is being dragged.
    pub fn slicing(&self) -> bool {
        self.slices.on().any(|s| s.drag.is_some())
    }

    /// The mouse dragged on with a plane's arrow: its layer by whole
    /// voxels from where it was pressed.
    pub fn drag_slice(&mut self, view_proj: glam::Mat4, screen: glam::Vec2, mouse: glam::Vec2, ray: (Vec3, Vec3)) {
        let Some(s) = self.slices.planes.iter_mut().flatten().find(|s| s.drag.is_some()) else { return };
        let axis = s.axis;
        let Some((d, h, start)) = s.drag.as_mut() else { return };
        if !d.travels(mouse) {
            return;
        }
        let (start, amount) = (*start, d.amount(h, view_proj, screen, mouse, ray));
        self.set_slice_at(axis, start + amount.round() as i32);
    }

    /// The planes' arrows let go of.
    pub fn let_go_of_slice(&mut self) {
        for s in self.slices.planes.iter_mut().flatten() {
            s.drag = None;
        }
    }

    /// A plane's arrow under the mouse (pixels in a room `screen` big).
    pub fn slice_grip(&self, view_proj: glam::Mat4, screen: glam::Vec2, at: glam::Vec2) -> Option<(Grip, Handles)> {
        self.slice_handles(view_proj).into_iter().find_map(|(_, h)| Some((h.grip_at(view_proj, screen, at)?, h)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::tests::{N, folder};

    /// A block sliced at z 1 from above: the cells above are not drawn and
    /// not met by the tools, which work on the cut face; flipped, the side
    /// under it goes; off, all of it again. The file is not touched.
    #[test]
    fn a_slice_hides_the_near_side_from_the_tools() {
        let dir = folder("slice");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        ed.open_block(0);
        let file = crate::blocks::path(&dir, "1");
        let text = std::fs::read_to_string(&file).unwrap();
        let down = |ed: &Editor| ed.aim(Vec3::new(1.5, 1.5, 50.0), Vec3::NEG_Z).map(|h| h.cell);
        assert_eq!(down(&ed), Some(IVec3::new(1, 1, N - 1)), "the top");
        ed.set_slice(2, Vec3::new(1.5, 1.5, 50.0));
        ed.set_slice_at(2, 1);
        assert!(ed.cut(IVec3::new(1, 1, 2)) && !ed.cut(IVec3::new(1, 1, 1)));
        assert_eq!(down(&ed), Some(IVec3::new(1, 1, 1)), "the cut face");
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        assert!(shown.world.get(IVec3::new(1, 1, 2)) == 0 && shown.world.get(IVec3::new(1, 1, 1)) != 0, "not drawn above the cut");
        // FLIP: the side under the plane goes.
        ed.flip_slice(2);
        assert!(ed.cut(IVec3::new(1, 1, 0)) && !ed.cut(IVec3::new(1, 1, 2)));
        ed.set_slice(2, Vec3::ZERO);
        assert!(!ed.slices.any() && !ed.cut(IVec3::new(1, 1, 2)), "the same axis again: off");
        assert_eq!(down(&ed), Some(IVec3::new(1, 1, N - 1)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "the file untouched");
    }

    /// Three planes at once, as a scan: a voxel is cut where any plane
    /// cuts it; each side is taken from the eye once, as it is switched
    /// on, and stays until FLIP; Page Up and Page Down move the plane
    /// switched on last.
    #[test]
    fn three_planes_cut_each_its_own_side() {
        let dir = folder("slices");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        ed.open_block(0);
        // The eye above, in front (low y) and on the right (high x).
        let eye = Vec3::new(50.0, -50.0, 50.0);
        for a in 0..3 {
            ed.set_slice(a, eye);
            ed.set_slice_at(a, 1);
        }
        assert_eq!(ed.slices.on().map(|s| s.side).collect::<Vec<_>>(), vec![1, -1, 1]);
        assert!(ed.cut(IVec3::new(2, 1, 0)), "past x 1");
        assert!(ed.cut(IVec3::new(1, 0, 0)), "before y 1");
        assert!(ed.cut(IVec3::new(0, 2, 2)), "above z 1");
        assert!(!ed.cut(IVec3::new(1, 1, 1)) && !ed.cut(IVec3::new(0, 2, 0)), "kept where no plane cuts");
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        assert!(shown.world.get(IVec3::new(2, 2, 0)) == 0 && shown.world.get(IVec3::new(1, 2, 1)) != 0);
        ed.flip_slice(1);
        assert!(!ed.cut(IVec3::new(1, 0, 0)) && ed.cut(IVec3::new(1, 2, 0)), "y flipped");
        // The last one switched on (z) moves; switched off, the one before.
        ed.step_slice(1);
        assert_eq!(ed.slices.planes[2].as_ref().map(|s| s.at), Some(2));
        ed.set_slice(2, eye);
        assert_eq!(ed.slices.last, Some(1));
        ed.step_slice(-1);
        assert_eq!(ed.slices.planes[1].as_ref().map(|s| s.at), Some(0));
        // Each plane its arrow.
        assert_eq!(ed.slice_handles(glam::Mat4::IDENTITY).iter().map(|(a, _)| *a).collect::<Vec<_>>(), vec![0, 1]);
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
        ed.set_slice(0, Vec3::new(-50.0, 2.0, 2.0));
        ed.set_slice_at(0, 1);
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
        ed.set_slice_at(0, 2);
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        assert!(shown.world.get(IVec3::new(0, 0, 0)) == 0 && shown.world.get(IVec3::new(1, 0, 0)) != 0, "moved a layer: the head's first column gone");
    }
}
