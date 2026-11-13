//! What a game draws into a frame (`Drawing`) and the cubes of one frame
//! (`Voxels`).

use super::*;

/// What a game draws into a frame (`Game::draw`): its models from the bank
/// on the GPU, cubes (bullets, grenades, anything small), and lights, on
/// the voxel grid. Models outside the view and the shadow box are left out;
/// where two things share a cell, the first drawn keeps it.
pub struct Drawing<'a> {
    pub(super) alpha: f32,
    pub(super) title: bool,
    pub(super) snap: bool,
    pub(super) chunky: i32,
    pub(super) view_proj: Mat4,
    pub(super) light_vp: Mat4,
    pub(super) models: &'a Models,
    pub(super) voxels: Voxels<'a>,
    /// What is drawn becomes see-through on top (the title's props leaving).
    pub(super) fade: f32,
    pub(super) draws: Vec<ModelDraw>,
    pub(super) lights: Vec<Light>,
    /// The daylight the game set for this picture (`frame:daylight`).
    pub(super) daylight: Option<Daylight>,
    /// The HUD's grid this frame, in UI voxels across and up, and the big
    /// letters' font.
    pub(super) hud: crate::ui::Layout,
    pub(super) font: Option<Arc<DisplayFont>>,
    /// How the play is seen unless the game says otherwise (None on the
    /// title: its camera is the title's), and the camera the game set for
    /// this picture (`frame:camera`).
    pub(super) viewing: Option<Viewing>,
    pub(super) camera: Option<PictureCamera>,
    /// Whether models out of view are left out as they come; else (a game
    /// that sets its camera, which comes after them) each draw's box is
    /// kept in `bounds` and they are left out once the camera is known.
    pub(super) cull: bool,
    pub(super) bounds: Vec<(Vec3, Vec3)>,
}

impl Drawing<'_> {
    /// This picture's daylight, as the game sets it.
    pub fn daylight(&mut self, day: Daylight) {
        self.daylight = Some(day);
    }

    /// This picture's camera, as the game sets it (the play's only: the
    /// title keeps its own).
    pub fn camera(&mut self, camera: PictureCamera) {
        self.camera = Some(camera);
    }

    /// How the play is seen unless the game says otherwise (None on the
    /// title), to work out where a point shows after `camera`.
    pub fn viewing(&self) -> Option<&Viewing> {
        self.viewing.as_ref()
    }

    /// The HUD's grid this frame: UI voxels across (by the screen's shape)
    /// and up (`c.ui.height`).
    pub fn hud_size(&self) -> (f32, f32) {
        (self.hud.width, self.hud.height)
    }

    /// The game's big letters (`c.ui.display_font`), if it has them.
    pub fn font(&self) -> Option<Arc<DisplayFont>> {
        self.font.clone()
    }

    /// How far into the tick the frame is drawn (0..1).
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// The camera's projection this picture (where a point shows).
    pub fn view_proj(&self) -> Mat4 {
        self.view_proj
    }

    /// Whether moving things are drawn on the voxel grid.
    pub fn snaps(&self) -> bool {
        self.snap
    }

    /// On the title scene (the game's things in it, the player not).
    pub fn title(&self) -> bool {
        self.title
    }

    /// Between where something was a tick ago and where it is now.
    pub fn between(&self, prev: Vec3, pos: Vec3) -> Vec3 {
        between(prev, pos, self.alpha)
    }

    /// On the grid when the camera snaps (`[camera] snap`).
    pub fn snapped(&self, at: Vec3) -> Vec3 {
        if self.snap { at.floor() } else { at }
    }

    /// Where a body's feet are drawn.
    pub fn body_origin(&self, body: &Body) -> Vec3 {
        self.snapped(between(body.prev, body.pos, self.alpha))
    }

    /// Where something moving at `point` is drawn: the minimum corner of the
    /// voxel it fills. On the grid when snapping, gliding otherwise.
    pub fn corner(&self, point: Vec3) -> Vec3 {
        if self.snap { point.floor() } else { point - 0.5 }
    }

    /// How many voxels across the chunky things are (a third of a body).
    pub fn chunky(&self) -> i32 {
        self.chunky
    }

    /// The game's model called `name`, if it has one.
    pub fn model_of(&self, name: &str) -> Option<&VoxModel> {
        self.models.get(name)
    }

    /// The frame of the model `name` for what a figure's body does: its
    /// kind's `sets` (idle, walk, jump), else the model's own layout
    /// (`things::figure_frame`).
    pub fn figure_frame(&self, name: &str, sets: &BTreeMap<String, Frames>, body: &Body) -> usize {
        self.models.get(name).map_or(0, |m| crate::sim::things::figure_frame(sets, body, m.frames.len(), m.size.z as f32))
    }

    /// One frame of the game's model `name` with its feet at `origin`,
    /// turned to `look` (quarter turns; the model's forward is +y),
    /// whitened by `tint`.
    pub fn model(&mut self, name: &str, frame: usize, origin: Vec3, look: IVec2, tint: Option<[u8; 4]>) {
        self.model_turned(name, frame, origin, look, Quat::IDENTITY, tint);
    }

    /// `model`, turned freely besides (the game's choice: a ship banking, a
    /// figure facing a diagonal): `turn` in the model's own directions
    /// (`turning`, `facing`), about `render::turn_centre` (the middle of
    /// the pivot cell, the voxel at half the model's height). Its voxels
    /// leave the grid then, but for quarter turns.
    pub fn model_turned(&mut self, name: &str, frame: usize, origin: Vec3, look: IVec2, turn: Quat, tint: Option<[u8; 4]>) {
        let models = self.models;
        if let Some(m) = models.get(name) {
            self.put_model(m, frame, origin, look, turn, tint);
        }
    }

    /// `model_turned`, a figure of parts posed besides: `pose` turns each
    /// part by its name about its joint (`turning`, in the model's own
    /// directions; identity: as it stands), moving what hangs on it along.
    #[allow(clippy::too_many_arguments)]
    pub fn model_posed(&mut self, name: &str, frame: usize, origin: Vec3, look: IVec2, turn: Quat, pose: &dyn Fn(&str) -> PartPose, tint: Option<[u8; 4]>) {
        let models = self.models;
        if let Some(m) = models.get(name) {
            let turns: Vec<PartPose> = m.parts.iter().map(|p| pose(&p.name)).collect();
            self.put_posed(m, frame, origin, look, turn, &turns, tint);
        }
    }

    /// Frame `frame` of a model on the grid, a figure of parts swinging its
    /// parts with its steps (`swing`, `swinging`; its height `tall`).
    #[allow(clippy::too_many_arguments)]
    pub fn model_swung(&mut self, name: &str, frame: usize, origin: Vec3, look: IVec2, swing: &BTreeMap<String, f32>, body: &Body, tall: f32, tint: Option<[u8; 4]>) {
        let swung = swinging(swing, body, tall);
        self.model_posed(name, frame, origin, look, Quat::IDENTITY, &swung, tint);
    }

    /// A figure as its kind animates it: the frame for what its body does
    /// (`sets`, `things::figure_frame`), its parts swinging with its steps
    /// (`swing`), turned `turn`.
    #[allow(clippy::too_many_arguments)]
    pub fn figure(&mut self, name: &str, sets: &BTreeMap<String, Frames>, swing: &BTreeMap<String, f32>, body: &Body, origin: Vec3, look: IVec2, turn: Quat, tint: Option<[u8; 4]>) {
        let models = self.models;
        let Some(m) = models.get(name) else { return };
        let frame = crate::sim::things::figure_frame(sets, body, m.frames.len(), m.size.z as f32);
        let swung = swinging(swing, body, m.size.z as f32);
        let turns: Vec<PartPose> = m.parts.iter().map(|p| swung(&p.name)).collect();
        self.put_posed(m, frame, origin, look, turn, &turns, tint);
    }

    pub(super) fn put_model(&mut self, m: &VoxModel, frame: usize, origin: Vec3, look: IVec2, turn: Quat, tint: Option<[u8; 4]>) {
        self.put_posed(m, frame, origin, look, turn, &[], tint);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn put_posed(&mut self, m: &VoxModel, frame: usize, origin: Vec3, look: IVec2, turn: Quat, pose: &[PartPose], tint: Option<[u8; 4]>) {
        let posed = pose.iter().any(|q| *q != PartPose::REST) || rests_turned(m);
        let (lo, hi) = if turn == Quat::IDENTITY && !posed {
            let reach = (m.size.x.max(m.size.y) / 2 + 1) as f32;
            (origin - Vec3::new(reach, reach, 0.0), origin + Vec3::new(reach, reach, m.size.z as f32))
        } else {
            let (middle, reach) = (origin + Vec3::new(0.5, 0.5, m.size.z as f32 * 0.5), m.size.as_vec3().length() * 0.5 + 1.0);
            (middle - reach, middle + reach)
        };
        if self.cull && !crate::render::on_screen(&self.view_proj, lo, hi) && !crate::render::on_screen(&self.light_vp, lo, hi) {
            return;
        }
        // A model colour's alpha is how opaque it is (glass); `fade` makes
        // what is drawn see-through on top. Fading, a model is all cubes.
        let fade = self.fade;
        let voxels = &mut self.voxels;
        let cubes_before = (voxels.opaque.len(), voxels.see_through.len());
        // A model's glow is a byte (0 to 255); half a step over it keeps
        // the cube's rounding down from losing one.
        let mut put = |corner: Vec3, color: [u8; 4], metal: u8, glow: u8, shade: [i8; 4], turn: [i8; 4], covered: u8| {
            let glow = if glow == 0 { 0.0 } else { (glow as f32 + 0.5) / 255.0 };
            voxels.add(corner, color, glow, color[3] as f32 / 255.0 * fade, metal, shade, turn, covered)
        };
        draw(&mut put, (fade >= 1.0).then_some(&mut self.draws), m, frame, origin, look, turn, pose, tint);
        if !self.cull {
            self.bounds.resize(self.draws.len(), (lo, hi));
            let v = &mut self.voxels;
            v.boxes.push(((lo, hi), cubes_before.0..v.opaque.len(), cubes_before.1..v.see_through.len()));
        }
    }

    /// A cube with its minimum corner at `corner` (see `corner`): `glow`
    /// and `opacity` 0..1.
    pub fn cube(&mut self, corner: Vec3, color: [u8; 4], glow: f32, opacity: f32) {
        self.voxels.add(corner, color, glow, opacity, 0, [0; 4], [0; 4], 0);
    }

    /// A chunky cube of voxels (`chunky` across) around `centre`.
    pub fn blob(&mut self, centre: Vec3, color: [u8; 4], glow: f32, opacity: f32) {
        let chunky = self.chunky;
        let corner = self.corner(centre - (chunky - 1) as f32 * 0.5);
        for z in 0..chunky {
            for y in 0..chunky {
                for x in 0..chunky {
                    self.voxels.add(corner + IVec3::new(x, y, z).as_vec3(), color, glow, opacity, 0, [0; 4], [0; 4], 0);
                }
            }
        }
    }

    /// A light lighting what is around it this frame.
    pub fn light(&mut self, light: Light) {
        self.lights.push(light);
    }
}

/// A model's box (low and high corner) and the ranges of its opaque and
/// see-through cubes among a frame's (`Voxels::leave_out`).
pub(super) type ModelCubes = ((Vec3, Vec3), std::ops::Range<usize>, std::ops::Range<usize>);

/// The cubes of one frame: opaque ones, and see-through ones that are drawn
/// after them, far to near.
pub(super) struct Voxels<'a> {
    pub(super) world: &'a crate::world::World,
    /// Where the eye is (the see-through cubes far to near from it), and
    /// whether it moved after they came (a picture's camera).
    pub(super) eye: Vec3,
    pub(super) moved: bool,
    /// The cubes of each model drawn while what is out of view is not yet
    /// known (`Drawing::cull`): its box, its opaque and see-through ones.
    pub(super) boxes: Vec<ModelCubes>,
    pub(super) taken: crate::world::CellSet,
    pub(super) opaque: Vec<Instance>,
    pub(super) see_through: Vec<(f32, Instance)>,
}

impl Voxels<'_> {
    /// Claims the voxel with its minimum corner at `corner`, unless the
    /// world or something drawn earlier fills it; one turned freely
    /// (`turn`, `render::snorm_turn`) lies on no cell and claims none.
    /// An opaque cube leaves out the faces `covered` names (a model's own
    /// voxels lying against them, `vox::covered`); a see-through one keeps
    /// them, for `render::join_see_through`'s skins.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn add(&mut self, corner: Vec3, color: [u8; 4], glow: f32, opacity: f32, metal: u8, shade: [i8; 4], turn: [i8; 4], covered: u8) {
        if turn == [0; 4] {
            let cell = (corner + 0.5).floor().as_ivec3();
            if self.world.get(cell) != 0 || !self.taken.insert(cell) {
                return;
            }
        }
        let centre = corner + 0.5;
        let opaque = opacity >= 0.99;
        let fx = [(glow.clamp(0.0, 1.0) * 255.0) as u8, (opacity.clamp(0.0, 1.0) * 255.0) as u8, if opaque { covered } else { 0 }, metal];
        let instance = Instance { pos_size: centre.extend(1.0).to_array(), color, fx, shade, turn };
        if !opaque {
            self.see_through.push((centre.distance_squared(self.eye), instance));
        } else {
            self.opaque.push(instance);
        }
    }

    /// Leaves out the cubes of the models whose box `shows` does not show.
    pub(super) fn leave_out(&mut self, shows: impl Fn(&(Vec3, Vec3)) -> bool) {
        let (mut opaque, mut see_through) = (vec![true; self.opaque.len()], vec![true; self.see_through.len()]);
        for (bounds, o, s) in std::mem::take(&mut self.boxes) {
            if !shows(&bounds) {
                opaque[o].fill(false);
                see_through[s].fill(false);
            }
        }
        let mut keep = opaque.into_iter();
        self.opaque.retain(|_| keep.next().unwrap_or(true));
        let mut keep = see_through.into_iter();
        self.see_through.retain(|_| keep.next().unwrap_or(true));
    }

    pub(super) fn finish(mut self) -> Vec<Instance> {
        if self.moved {
            for (far, cube) in &mut self.see_through {
                *far = Vec3::from_slice(&cube.pos_size[..3]).distance_squared(self.eye);
            }
        }
        self.see_through.sort_by(|a, b| b.0.total_cmp(&a.0));
        let from = self.opaque.len();
        self.opaque.extend(self.see_through.into_iter().map(|(_, i)| i));
        // Water and glass are one body: the faces between their cubes go,
        // but for the one towards the eye, a skin a layer.
        let cubes = &mut self.opaque[from..];
        if !cubes.is_empty() {
            let middle = cubes.iter().map(|i| Vec3::from_slice(&i.pos_size[..3])).sum::<Vec3>() / cubes.len() as f32;
            crate::render::join_see_through(cubes, self.eye - middle);
        }
        self.opaque
    }
}
