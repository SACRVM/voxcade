//! Where the picture looks from: the sun, the faces the meshes hold, the
//! camera following the player, the title's camera, a picture's own
//! camera (`PictureCamera`, `Viewing`) and daylight (`Daylight`).

use super::*;

impl<G: Game> Session<G> {
    /// The sun of the level being played: its file's (`[light]`), else
    /// `c.light`'s.
    pub fn sun(&self) -> Vec3 {
        self.game.daylight().sun.unwrap_or_else(|| Vec3::from(self.cfg.light.sun)).normalize_or(Vec3::NEG_Z)
    }

    /// The faces the meshes hold: those a view ray or the sun can hit, the
    /// suns of every level (`[light]`) with `c.light`'s, so a level entered
    /// finds its faces there; for a game that moves its sun
    /// (`frame:daylight`), every face a sun above the horizon can. A
    /// picture the game moves or zooms (`frame:camera`'s `look_at` and
    /// `width`) looks along the same rays and needs no more; one at
    /// another pitch or field of view adds the faces its rays can hit, from
    /// that picture on (`seen_faces`; the meshes are built anew once).
    pub fn face_mask(&self) -> FaceMask {
        // (A picture of its own shape: as wide as that; a wider window
        // adds its faces when drawn, `seen_faces`.)
        let rays = camera::corner_rays(&self.cfg.camera, self.cfg.camera.aspect.unwrap_or(camera::WIDEST_KEPT));
        let mut mask = if self.game.moves_sun() {
            mesh::face_mask_any_sun(&rays)
        } else {
            let mut mask = mesh::face_mask(&rays, Vec3::from(self.cfg.light.sun).normalize_or(Vec3::NEG_Z));
            for sun in self.game.suns() {
                let more = mesh::face_mask(&rays, sun.normalize_or(Vec3::NEG_Z));
                for (m, o) in mask.iter_mut().zip(more) {
                    *m |= o;
                }
            }
            mask
        };
        for (m, o) in mask.iter_mut().zip(self.seen_faces.get()) {
            *m |= o;
        }
        mask
    }

    /// Where a body's feet are drawn, `alpha` of the way into the tick.
    pub(super) fn body_origin(&self, body: &Body, alpha: f32) -> Vec3 {
        let feet = between(body.prev, body.pos, alpha);
        if self.cfg.camera.snap { feet.floor() } else { feet }
    }

    /// Where the feet of the body the camera keeps on screen are drawn,
    /// `alpha` of the way into the tick (`Game::focus`; the camera's target
    /// without one).
    pub fn player_origin(&self, alpha: f32) -> Vec3 {
        let sim = self.game.sim();
        let group = self.game.focus_group();
        let Some(first) = group.first() else { return between(sim.cam_prev, sim.cam_target, alpha) };
        let anchor = self.body_origin(first, alpha);
        if group.len() == 1 {
            return anchor;
        }
        // Several players: the middle of their span, the way there from the
        // first's drawn voxel in whole voxels when snapping (so none of them
        // flickers against the view).
        let at: Vec<Vec3> = group.iter().map(|b| between(b.prev, b.pos, alpha)).collect();
        let (lo, hi) = at.iter().fold((at[0], at[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let middle = (lo + hi) * 0.5;
        let way = (middle - between(first.prev, first.pos, alpha)) * Vec3::new(1.0, 1.0, 0.0);
        anchor + if self.cfg.camera.snap { way.round() } else { way }
    }

    /// The point the camera looks at, `alpha` of the way into the tick. The
    /// same everywhere, edges included: the camera never tilts or zooms,
    /// and beyond the edge lies the dark (unless it stops at the stage's
    /// edges, `within_stage`).
    pub fn view_target(&self, alpha: f32) -> Vec3 {
        let sim = self.game.sim();
        let snap = self.cfg.camera.snap;
        let mut target = between(sim.cam_prev, sim.cam_target, alpha);
        if snap {
            // On the ground plane the camera sits on the drawn player's voxel,
            // so the world moves exactly when the player steps. Rounding the
            // two separately makes the player flicker; letting the camera lag
            // behind makes it catch up in hitching steps after every stop.
            // Height stays the camera's own, since it ignores jumps; a side
            // view's camera keeps its height to the drawn feet within its
            // band, in whole voxels: inside the band the view stands still,
            // at its edge it moves with the feet, never rounded apart.
            let feet = self.player_origin(alpha);
            let z = match self.cfg.camera.follow_height {
                Some(band) => feet.z + (target.z - feet.z).floor().max(-band.above).min(band.below),
                None => target.z,
            };
            target = Vec3::new(feet.x, feet.y, z);
        }
        // Looking ahead (a side view leading where the hero runs), in whole
        // voxels when snapping, as the target is.
        let lead = between(sim.lead_prev, sim.lead, alpha);
        target += if snap { lead.round() } else { lead };
        if let Some(shake) = self.shaking() {
            target += shake;
        }
        if snap { target.round() } else { target }
    }

    /// How far the screen's shake moves the camera this tick (at most a
    /// body width each way), if it shakes and the player lets it.
    pub(super) fn shaking(&self) -> Option<Vec3> {
        let sim = self.game.sim();
        if !(sim.shake > 0.0 && self.options.shake) {
            return None;
        }
        let t = sim.tick as f32;
        let amount = sim.shake * sim.shake * self.cfg.physics.body_width;
        Some(Vec3::new((t * 1.7).sin(), (t * 2.3).cos(), 0.0) * amount)
    }

    /// `target` moved so the view shows nothing beyond the stage's sides,
    /// top and bottom (`c.camera.stop_at_edges`): at a level's end the
    /// camera stops and the player walks on towards the screen's edge. A
    /// stage smaller than the view is centred. Seen from above, the ground
    /// the screen shows stays on the stage across and in depth (BunnyFarm's
    /// meadow: the bunny goes out through the farm's gate to its edges).
    pub(super) fn within_stage(&self, target: Vec3, cam: &crate::config::CameraConfig, aspect: f32) -> Vec3 {
        within(self.game.sim().world.stage_box(), target, cam, aspect)
    }

    /// The picture's shape (width over height) in a window of the shape
    /// `window`: the game's own (`c.camera.aspect`), else the window's.
    pub fn picture_aspect(&self, window: f32) -> f32 {
        self.cfg.camera.aspect.unwrap_or(window)
    }

    /// Builds the frame `alpha` of the way from the previous tick to the current one.
    #[cfg(test)]
    pub fn frame(&self, alpha: f32, aspect: f32) -> FrameData {
        self.compose(alpha, aspect, Scene::Play, None)
    }

    /// The title scene's camera: the game's, closer.
    pub(super) fn title_camera(&self) -> crate::config::CameraConfig {
        let mut cam = self.cfg.camera.clone();
        if let Some(w) = self.cfg.title_screen.view_width {
            cam.show_width(w);
        }
        cam
    }

    /// What the title's camera looks at: `[title_screen] at` on the
    /// ground, at the camera's own height.
    pub(super) fn title_target(&self) -> Vec3 {
        let at = self.cfg.title_screen.at.unwrap_or(self.cfg.player.spawn);
        Vec3::new(at[0], at[1], self.game.sim().cam_target.z).floor()
    }

    /// Where the title's flyby is `time` seconds in (its skids), if it is
    /// crossing the view now: from the left edge to the right, a little up
    /// the screen, `c.title_screen.flyby_altitude` over the ground.
    pub fn flyby_at(&self, time: f32) -> Option<Vec3> {
        let time = time - self.flyby_from;
        let t = &self.cfg.title_screen;
        t.flyby.as_ref()?;
        let every = t.flyby_every.max(FLYBY_TIME + 1.0);
        let into = (time - 3.0).rem_euclid(every);
        if time < 3.0 || into > FLYBY_TIME {
            return None;
        }
        let target = self.title_target();
        let span = self.title_camera().view_width * 0.9;
        let x = target.x - span + 2.0 * span * into / FLYBY_TIME;
        let y = target.y + span * 0.12;
        let ground = self.game.sim().world.column_top(target.x as i32, y as i32) as f32;
        Some(Vec3::new(x, y, ground + t.flyby_altitude).floor())
    }
}

/// `target` moved so the view of `cam` shows nothing beyond the stage's
/// (`lo`, `hi`) sides, top and bottom, if the camera stops at its edges
/// (`Session::within_stage`).
pub(crate) fn within((lo, hi): (Vec3, Vec3), target: Vec3, cam: &crate::config::CameraConfig, aspect: f32) -> Vec3 {
    if !cam.stop_at_edges {
        return target;
    }
    let fit = |v: f32, lo: f32, hi: f32| if hi - lo < 0.0 { (lo + hi) * 0.5 } else { v.max(lo).min(hi) };
    let mut t = target;
    if let Some((near, far)) = camera::ground_extent(cam, aspect) {
        t.x = fit(t.x, lo.x - near.x, hi.x - far.x);
        t.y = fit(t.y, lo.y - near.y, hi.y - far.y);
    } else {
        let (half, down, up) = camera::view_extent(cam, aspect);
        t.x = fit(t.x, lo.x + half, hi.x - half);
        t.z = fit(t.z, lo.z + down, hi.z - up);
    }
    if cam.snap { t.round() } else { t }
}

/// A picture's daylight as the game sets it (`frame:daylight`); what it
/// leaves out is `c.light`'s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Daylight {
    pub sun: Option<Vec3>,
    pub sun_color: Option<[f32; 3]>,
    pub ambient: Option<f32>,
    pub shadow: Option<f32>,
    pub sky: Option<[f32; 3]>,
}

/// A picture's camera as the game sets it (`frame:camera`): what it
/// leaves out is the game's camera's. Not kept: the next picture without
/// it is the game's camera's again.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PictureCamera {
    /// The point the view centres on, voxels across and deep, and high
    /// (None: the height of the game's camera).
    pub look_at: Option<(f32, f32, Option<f32>)>,
    /// How many voxels the picture shows across where it looks, or high
    /// (one of them).
    pub width: Option<f32>,
    pub height: Option<f32>,
    /// Degrees below the horizon, and the field of view up and down.
    pub pitch: Option<f32>,
    pub fov: Option<f32>,
}

impl PictureCamera {
    /// The game's camera `cam` as this picture changes it.
    pub fn applied(&self, cam: &crate::config::CameraConfig) -> crate::config::CameraConfig {
        let mut cam = cam.clone();
        if let Some(width) = self.width {
            cam.show_width(width);
        }
        if let Some(height) = self.height {
            (cam.view_height, cam.view_at_least) = (Some(height), None);
        }
        if let Some(pitch) = self.pitch {
            cam.pitch = pitch;
        }
        if let Some(fov) = self.fov {
            cam.fov = fov;
        }
        cam
    }
}

/// How the play is seen before the game says otherwise: the game's camera,
/// the point it follows (shaken, before it stops at the stage's edges),
/// the shake alone, the picture's shape and the stage. A picture's camera
/// (`frame:camera`) is worked out from it.
#[derive(Clone, Debug)]
pub struct Viewing {
    pub(super) cam: crate::config::CameraConfig,
    pub(super) follows: Vec3,
    pub(super) shake: Vec3,
    pub(super) aspect: f32,
    pub(super) stage: (Vec3, Vec3),
}

impl Viewing {
    /// The camera of a picture and the point it looks at: the game's own
    /// without `picture`. Without `look_at` the view keeps following what
    /// the game's camera follows (stopping at the stage's edges if it
    /// does), only seen as `picture` says; with it, at that point as given
    /// (the screen's shake on top), never rounded or eased.
    pub fn seen(&self, picture: Option<&PictureCamera>) -> (crate::config::CameraConfig, Vec3) {
        let Some(picture) = picture else { return (self.cam.clone(), within(self.stage, self.follows, &self.cam, self.aspect)) };
        let cam = picture.applied(&self.cam);
        let target = match picture.look_at {
            Some((x, y, z)) => Vec3::new(x, y, z.unwrap_or(self.follows.z)) + self.shake,
            None => within(self.stage, self.follows, &cam, self.aspect),
        };
        (cam, target)
    }

    /// Where a point shows in that picture.
    pub fn view_proj(&self, picture: Option<&PictureCamera>) -> Mat4 {
        let (cam, target) = self.seen(picture);
        camera::view_proj(&cam, target, self.aspect)
    }
}
