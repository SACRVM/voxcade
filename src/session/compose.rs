//! Turning the game into a frame for the renderer: the play, the menus,
//! a model alone (`still`), and looking without a picture (`look`).

use super::*;

impl<G: Game> Session<G> {
    /// A frame of `scene` with `menu` over it when it is open (instead of
    /// the HUD).
    /// `aspect` is the window's shape (or the picture's already).
    pub fn menu_frame(&self, menu: &Menu, settings: &Settings, alpha: f32, aspect: f32, scene: Scene) -> FrameData {
        let aspect = self.picture_aspect(aspect);
        let ui = menu.is_open().then(|| {
            let t = &self.cfg.title_screen;
            let logo = self.logo_lines();
            let view = crate::ui::MenuView {
                menu,
                settings,
                logo: &logo,
                drip: t.drip,
                songs: &self.song_titles,
                scored: self.game.scored(),
                font: self.font.as_deref(),
                logo_heights: &t.logo_heights,
                logo_cracks: &t.logo_cracks,
                logo_bank: &self.logo,
                logo_model: self.logo_model.as_ref(),
                logo_bob: self.cfg.title_screen.logo_bob,
                light_menu: self.cfg.title_screen.light_menu,
            };
            crate::ui::menu(&view, &self.cfg.ui, alpha, aspect)
        });
        self.compose(alpha, aspect, scene, ui)
    }

    /// What the game draws into a frame seen as `view` (its projection,
    /// the sun's, the eye), leaving out what lies where `world` is filled;
    /// room for `cubes` to begin with.
    pub(super) fn drawing<'a>(&'a self, world: &'a crate::world::World, alpha: f32, view: (Mat4, Mat4, Vec3), aspect: f32, cubes: usize) -> Drawing<'a> {
        let (view_proj, light_vp, eye) = view;
        Drawing {
            alpha,
            title: false,
            snap: self.cfg.camera.snap,
            // Grenades, bullets, fire and smoke are a third of a body width
            // thick: a cube of voxels, the way an 8-bit shot is a couple of
            // pixels.
            chunky: (self.cfg.physics.body_width / 3.0).round().max(1.0) as i32,
            view_proj,
            light_vp,
            models: &self.models,
            voxels: Voxels {
                world,
                eye,
                moved: false,
                boxes: Vec::new(),
                taken: crate::world::CellSet::with_capacity_and_hasher(1 << 16, Default::default()),
                opaque: Vec::with_capacity(cubes),
                see_through: Vec::new(),
            },
            fade: 1.0,
            draws: Vec::new(),
            lights: Vec::new(),
            daylight: None,
            hud: crate::ui::Layout::of(&self.cfg.ui, aspect),
            font: self.font.clone(),
            viewing: None,
            camera: None,
            cull: true,
            bounds: Vec::new(),
            shadow_only: false,
            cast_top: 0.0,
        }
    }

    /// One of the game's models alone (`name`: its name or its file), as
    /// the game draws it, in its light and shading, from its camera's
    /// angle, on a floor of two greys: the model sheet (`voxcade sheet`).
    /// Frame `frame`, facing `look` (quarter turns) turned on by `turn`
    /// (`turning`), a figure of parts posed by `pose`, `view_width` voxels
    /// across the view. None: no such model.
    #[allow(clippy::too_many_arguments)]
    pub fn still(&self, name: &str, frame: usize, look: IVec2, turn: Quat, pose: &dyn Fn(&str) -> PartPose, view_width: f32, aspect: f32) -> Option<FrameData> {
        // Any model file of the game, in the bank before it is drawn.
        self.models.drawn(name)?;
        self.bank_fresh();
        let m = self.models.drawn(name)?;
        let size = m.size.as_vec3();
        // An empty world of its own round it: the level is not drawn.
        let across = (size.x.max(size.y) * 2.0).ceil() as i32 + 8;
        let world = crate::world::World::new(IVec3::new(across, across, m.size.z + 8), self.game.sim().world.palette);
        let room = world.size.as_vec3();
        let origin = Vec3::new((across / 2) as f32, (across / 2) as f32, 1.0);
        let target = origin + Vec3::Z * (size.z * 0.5);
        let mut cam = self.cfg.camera.clone();
        cam.show_width(view_width);
        let view_proj = camera::view_proj(&cam, target, aspect);
        let (light_vp, depth_range) = camera::light_view_proj(&cam, target, aspect, self.sun(), room, (Vec3::ZERO, room), self.options.shadow_map);
        let eye = camera::eye(&cam, target, aspect);
        let mut out = self.drawing(&world, 1.0, (view_proj, light_vp, eye), aspect, 512);
        let turns: Vec<PartPose> = m.parts.iter().map(|p| pose(&p.name)).collect();
        out.put_posed(&m, frame.min(m.frames.len() - 1), origin, look, turn, &turns, None);
        let floor = (size.x.max(size.y) * 0.5).ceil() as i32 + 3;
        for y in -floor..floor {
            for x in -floor..floor {
                let shade = if (x + y).rem_euclid(2) == 0 { [58, 56, 66, 255] } else { [66, 64, 74, 255] };
                out.cube(origin + Vec3::new(x as f32, y as f32, -1.0), shade, 0.0, 1.0);
            }
        }
        let Drawing { voxels, draws, .. } = out;
        let see_through = voxels.see_through.len();
        let instances = voxels.finish();
        // The game's frame for everything else (its light, sky, glow),
        // without its scene, HUD, lights and blur.
        let mut f = self.compose(1.0, aspect, Scene::Play, Some((Vec::new(), Vec::new(), Mat4::IDENTITY)));
        let haze_start = eye.distance(target);
        (f.view_proj, f.light_vp, f.remember, f.depth_bias, f.eye) = (view_proj, light_vp, false, 0.3 / depth_range, eye);
        (f.haze_start, f.haze_depth) = (haze_start, haze_start * 2.0);
        (f.models, f.instances, f.see_through, f.lights, f.ui) = (draws, instances, see_through, Vec::new(), Vec::new());
        (f.tilt_shift, f.screen_flash, f.fade) = (0.0, 0.0, [0.0; 4]);
        Some(f)
    }

    /// The game's own drawing of the play, as a picture in a window of the
    /// usual shape calls it, without the picture: its things and the
    /// script's `on_draw` (only looking; a mistake there halts the game as
    /// in a window). `check` looks after every tick, so a replay stops
    /// where drawing would stop the game in the window, at a fraction of a
    /// frame's cost (no light, particles or HUD voxels).
    pub fn look(&self) {
        let sim = self.game.sim();
        let (cam, aspect) = (&self.cfg.camera, self.picture_aspect(16.0 / 9.0));
        let target = self.within_stage(self.view_target(1.0), cam, aspect);
        let sun = self.drawn_sun.get().unwrap_or_else(|| self.sun());
        let (light_vp, _) = camera::light_view_proj(cam, target, aspect, sun, sim.world.size.as_vec3(), sim.world.stage_box(), self.options.shadow_map);
        let view = (camera::view_proj(cam, target, aspect), light_vp, camera::eye(cam, target, aspect));
        let mut out = self.drawing(&sim.world, 1.0, view, aspect, 512);
        self.game.draw(&mut out);
        self.count_lights(sim.flares.len() + out.lights.len());
    }

    /// A picture's lights, `asked` of them: past the renderer's
    /// `MAX_LIGHTS` (the nearest the middle drawn, the rest not), said once
    /// (`warnings`).
    fn count_lights(&self, asked: usize) {
        let most = crate::render::MAX_LIGHTS;
        if asked > most {
            self.say_once("lights", format!("lights: {asked} asked, {most} drawn (the {most} nearest the middle of the view; a picture lights {most} at most)"));
        }
    }

    /// Says `text` of the limit `what`, the first time a picture goes past it.
    pub(super) fn say_once(&self, what: &'static str, text: String) {
        let mut limits = self.limits.borrow_mut();
        if limits.told.insert(what) {
            limits.new.push(text);
        }
    }

    /// What pictures went past since last asked (lights dropped and the
    /// like), each limit said once: `shot` prints it, `inspect` lists it
    /// under "warnings", the IDE's PLAY puts it in OUTPUT.
    pub fn warnings(&self) -> Vec<String> {
        std::mem::take(&mut self.limits.borrow_mut().new)
    }

    /// The HUD's cubes as the UI pass draws them, from those `on_draw` gave
    /// (`raw`), seen from `eye`: faces a neighbour covers hidden (else they
    /// show as seams, thin dark lines through the hearts at some sizes),
    /// cubes of which nothing can show left out (a plate's back layer: half
    /// of BunnyFarm's 59,000). A HUD the same as the last picture's is not
    /// worked out again (`HudKept`): the picture is the same either way.
    pub(super) fn hud_cubes(&self, raw: Vec<Instance>, eye: Vec3) -> Vec<Instance> {
        let mut kept = self.hud_kept.borrow_mut();
        if kept.eye == eye && bytemuck::cast_slice::<Instance, u8>(&kept.raw) == bytemuck::cast_slice::<Instance, u8>(&raw) {
            return kept.done.clone();
        }
        let mut done = raw.clone();
        crate::render::hide_inner_faces(&mut done);
        crate::render::leave_out_unseen(&mut done, eye);
        *kept = HudKept { raw, done: done.clone(), eye };
        done
    }

    /// A frame of `scene`; `ui` (the menus) instead of the HUD when given;
    /// `aspect` the window's shape (or the picture's already).
    pub fn compose(&self, alpha: f32, aspect: f32, scene: Scene, ui: Option<(Vec<Instance>, Vec<ModelDraw>, Mat4)>) -> FrameData {
        let aspect = self.picture_aspect(aspect);
        let sim = self.game.sim();
        let world_size = sim.world.size.as_vec3();
        // How the play is seen unless the game sets this picture's camera
        // (`frame:camera`, in its `on_draw`).
        let viewing = matches!(scene, Scene::Play).then(|| Viewing {
            cam: self.cfg.camera.clone(),
            follows: self.view_target(alpha),
            shake: self.shaking().unwrap_or(Vec3::ZERO),
            aspect,
            stage: sim.world.stage_box(),
        });
        let (mut cam, mut target) = self.scene_camera(scene, viewing.as_ref(), alpha, aspect);
        let mut view_proj = camera::view_proj(&cam, target, aspect);
        // The sun the game set last picture, if it sets one: what is drawn
        // is chosen by where its shadows fall.
        let sun = self.drawn_sun.get().unwrap_or_else(|| self.sun());
        let (light_vp, depth_range) =
            camera::light_view_proj(&cam, target, aspect, sun, world_size, sim.world.stage_box(), self.options.shadow_map);

        // A voxel is a 3D pixel: everything moving fills one voxel, on the
        // world's grid when snapping. The first claim on a cell wins, and
        // cells the world fills stay the world's: among the cubes built here
        // by a set of taken cells; for the models on the GPU by drawing
        // order (world, models, cubes: at equal depth the first stays).
        let mut eye = camera::eye(&cam, target, aspect);
        let mut out = self.drawing(&sim.world, alpha, (view_proj, light_vp, eye), aspect, 512 + sim.particles.len());
        out.title = matches!(scene, Scene::Title { .. });
        // A game that sets its camera does so after its things are drawn:
        // what is out of view is known only then.
        out.cull = viewing.is_none() || !self.game.moves_camera();
        out.viewing = viewing.clone();
        self.draw_title(&mut out, scene);
        // The game's things, then the debris, fire, smoke and sparks. A
        // model drawn the first time was loaded meanwhile, drawn as cubes:
        // it goes in the bank and the picture is made again from there.
        self.game.draw(&mut out);
        if self.bank_fresh() {
            drop(out);
            return self.compose(alpha, aspect, scene, ui);
        }
        draw_particles(&mut out, &sim.particles, alpha);
        let Drawing { mut voxels, draws: models, lights: game_lights, daylight, camera: picture, cull, bounds, cast_top, .. } = out;
        // Models only casting a shadow above the world (a ceiling over a
        // room standing higher): the shadows' box reaches up to them.
        let high = cast_top > world_size.z;
        let world_size = if high { world_size.with_z(cast_top) } else { world_size };
        // The camera the game set for this picture: seen from there, the
        // faces its rays can hit kept in the meshes from now on.
        let picture = picture.filter(|_| viewing.is_some());
        if let (Some(viewing), Some(picture)) = (&viewing, &picture) {
            (cam, target) = viewing.seen(Some(picture));
            view_proj = camera::view_proj(&cam, target, aspect);
            eye = camera::eye(&cam, target, aspect);
            (voxels.eye, voxels.moved) = (eye, true);
        }
        self.keep_faces_seen(&cam, aspect, picture.is_some());
        // The daylight the game set for this picture, else its level's,
        // else c.light's; the shadows fitted to its sun.
        let day = self.daylight_of(daylight);
        let light = &self.cfg.light;
        let drawn = day.sun.map(|s| s.normalize_or(Vec3::NEG_Z));
        self.drawn_sun.set(drawn);
        let sun_now = drawn.unwrap_or_else(|| self.sun());
        let (light_vp, depth_range) = if sun_now == sun && picture.is_none() && !high {
            (light_vp, depth_range)
        } else {
            camera::light_view_proj(&cam, target, aspect, sun_now, world_size, sim.world.stage_box(), self.options.shadow_map)
        };
        // The models out of the picture's view and its shadows' box, left
        // out now that it is known.
        let shows = |&(lo, hi): &(Vec3, Vec3)| crate::render::on_screen(&view_proj, lo, hi) || crate::render::on_screen(&light_vp, lo, hi);
        let models = if cull {
            models
        } else {
            voxels.leave_out(shows);
            models.into_iter().zip(bounds).filter(|(_, b)| shows(b)).map(|(m, _)| m).collect()
        };
        let see_through = voxels.see_through.len();
        let instances = voxels.finish();
        let lights = self.lights_of(game_lights, target);
        let (ui, ui_models, ui_view_proj) = self.ui_of(ui, alpha, aspect);

        let fx = &self.cfg.effects;
        let haze_start = eye.distance(target);
        FrameData {
            view_proj,
            light_vp,
            remember: self.game.moves_sun(),
            sun_dir: sun_now,
            sun_color: day.sun_color.unwrap_or(light.sun_color),
            depth_bias: 0.3 / depth_range,
            ambient: day.ambient.unwrap_or(light.ambient),
            shadow: day.shadow.unwrap_or(light.shadow),
            shadow_style: match light.shadow_style {
                ShadowStyle::Smooth => 0.0,
                ShadowStyle::Voxel => 1.0,
                ShadowStyle::VoxelSoft => 2.0,
            },
            sky: day.sky.unwrap_or(light.sky),
            reflection: light.reflection,
            reflection_horizon: light.reflection_horizon,
            metal_sweep: light.metal_sweep,
            time: match scene {
                Scene::Title { time } | Scene::Intro { time, .. } => time,
                Scene::Play => sim.tick as f32 * crate::sim::DT,
            },
            eye,
            haze: fx.haze.clamp(0.0, 1.0),
            haze_color: fx.haze_color,
            haze_start,
            haze_depth: haze_start * 2.0,
            lights,
            glow: fx.glow.max(0.0),
            shadow_map: self.options.shadow_map,
            tilt_shift: if self.options.blur { fx.tilt_shift.clamp(0.0, 1.0) } else { 0.0 },
            screen_flash: if self.options.flashes { sim.screen_flash } else { 0.0 },
            outline: {
                let [r, g, b] = fx.outline_color;
                [r, g, b, fx.outline.max(0.0)]
            },
            fade: self.fade.as_ref().map_or([0.0; 4], Fade::look),
            models,
            ui_models,
            bank: self.bank(),
            instances,
            see_through,
            behind: 0,
            cast_only: 0,
            ui,
            ui_view_proj,
        }
    }

    /// The camera a scene is seen with, and where it looks: the play's
    /// (`viewing`), the title's, or the glide from one to the other.
    fn scene_camera(&self, scene: Scene, viewing: Option<&Viewing>, alpha: f32, aspect: f32) -> (crate::config::CameraConfig, Vec3) {
        match scene {
            Scene::Play => viewing.expect("the play's").seen(None),
            Scene::Title { .. } => (self.title_camera(), self.title_target()),
            Scene::Intro { t, .. } => {
                // Eased, and not on the grid: a glide, not steps.
                let e = t.clamp(0.0, 1.0);
                let e = e * e * (3.0 - 2.0 * e);
                let mut cam = self.title_camera();
                let (from, to) = (camera::shown_width(&cam, aspect), camera::shown_width(&self.cfg.camera, aspect));
                cam.show_width(from + (to - from) * e);
                let end = self.within_stage(self.view_target(alpha), &self.cfg.camera, aspect);
                (cam, between(self.title_target(), end, e))
            }
        }
    }

    /// The title's models lying on the ground (`place_props`), and its
    /// flyby crossing, rotor turning; fading as the glide leaves them.
    fn draw_title(&self, out: &mut Drawing, scene: Scene) {
        let (time, opacity) = match scene {
            Scene::Play => return,
            Scene::Title { time } => (time, 1.0),
            Scene::Intro { time, t } => (time, 1.0 - t.clamp(0.0, 1.0)),
        };
        out.fade = opacity;
        for (prop, &ground) in self.cfg.title_screen.props.iter().zip(&self.props_ground) {
            let Some(m) = self.models.props.get(&prop.model) else { continue };
            let look = prop.look();
            let origin = IVec3::new(prop.at[0] as i32, prop.at[1] as i32, ground).as_vec3();
            out.put_model(m, 0, origin, look, Quat::IDENTITY, None);
        }
        if let (Some(at), Some(m)) = (self.flyby_at(time), self.cfg.title_screen.flyby.as_ref().and_then(|f| self.models.props.get(f))) {
            out.fade = 1.0;
            out.put_model(m, (time * 30.0) as usize % m.frames.len(), at, IVec2::X, Quat::IDENTITY, None);
        }
        out.fade = 1.0;
    }

    /// Another pitch or field of view than the camera's (a picture the
    /// game set), or a window wider than the meshes' faces were chosen for
    /// (`camera::corner_rays`): the faces its rays can hit kept in the
    /// meshes from now on, built anew once.
    fn keep_faces_seen(&self, cam: &crate::config::CameraConfig, aspect: f32, set_by_the_game: bool) {
        let other = set_by_the_game && (cam.pitch != self.cfg.camera.pitch || cam.fov != self.cfg.camera.fov);
        if other || aspect > camera::WIDEST_KEPT {
            let mut seen = self.seen_faces.get();
            for (s, o) in seen.iter_mut().zip(mesh::face_mask(&camera::corner_rays(cam, aspect), self.sun())) {
                *s |= o;
            }
            self.seen_faces.set(seen);
        }
    }

    /// The daylight the game set for this picture (`frame:daylight`), else
    /// its level's (`[light]`); what neither sets stays c.light's.
    fn daylight_of(&self, set: Option<Daylight>) -> Daylight {
        let level = self.game.daylight();
        let day = set.unwrap_or_default();
        Daylight {
            sun: day.sun.or(level.sun),
            sun_color: day.sun_color.or(level.sun_color),
            ambient: day.ambient.or(level.ambient),
            shadow: day.shadow.or(level.shadow),
            sky: day.sky.or(level.sky),
        }
    }

    /// Light from flashes and explosions, and what the game lights (rockets
    /// in flight); the ones nearest the middle of the view `target` when
    /// there are too many.
    fn lights_of(&self, game_lights: Vec<Light>, target: Vec3) -> Vec<Light> {
        let strength = self.cfg.effects.lights;
        let mut lights: Vec<Light> = self
            .game
            .sim()
            .flares
            .iter()
            .map(|f| Light { pos: f.pos, radius: f.radius, color: f.color.map(|c| c * f.strength() * strength) })
            .chain(game_lights)
            .collect();
        lights.sort_by(|a, b| a.pos.distance_squared(target).total_cmp(&b.pos.distance_squared(target)));
        self.count_lights(lights.len());
        lights.truncate(crate::render::MAX_LIGHTS);
        lights
    }

    /// The UI layer: `ui` (the menus) when given, else the game's HUD; what
    /// stopped the game or a reload over everything until mended (in place
    /// of the HUD).
    fn ui_of(&self, ui: Option<(Vec<Instance>, Vec<ModelDraw>, Mat4)>, alpha: f32, aspect: f32) -> (Vec<Instance>, Vec<ModelDraw>, Mat4) {
        let problem = self.game.halted().or_else(|| self.problem.clone());
        let (mut ui, ui_models, ui_view_proj) = ui.unwrap_or_else(|| {
            let ui = &self.cfg.ui;
            let layout = crate::ui::Layout::of(ui, aspect);
            let camera = crate::ui::camera(&layout, ui.slant, ui.depth as f32);
            let hud = if problem.is_some() { Vec::new() } else { self.game.hud(ui, alpha, aspect, self.font.as_deref()) };
            let eye = crate::ui::eye(&layout, ui.slant, ui.depth as f32);
            (self.hud_cubes(hud, eye), Vec::new(), camera)
        });
        if let Some(problem) = problem.filter(|_| self.problem_shown) {
            ui.extend(crate::ui::error_panel(&problem, &self.cfg.ui, aspect));
        }
        (ui, ui_models, ui_view_proj)
    }
}

/// The debris, fire, smoke and sparks, where they are `alpha` into the
/// tick.
fn draw_particles(out: &mut Drawing, particles: &[crate::sim::Particle], alpha: f32) {
    for q in particles {
        let at = between(q.prev, q.pos, alpha);
        let (color, glow, opacity) = q.look();
        if q.chunky() {
            out.blob(at, color, glow, opacity);
        } else {
            out.cube(out.corner(at), color, glow, opacity);
        }
    }
}

/// The limits pictures went past: those said, and those to say
/// (`Session::warnings`).
#[derive(Default)]
pub(crate) struct Limits {
    told: std::collections::BTreeSet<&'static str>,
    new: Vec<String>,
}

/// The HUD as the last picture had it (`Session::hud_cubes`): the cubes
/// `on_draw` gave, the eye they were seen from, and what the UI pass drew
/// of them.
#[derive(Default)]
pub(crate) struct HudKept {
    raw: Vec<Instance>,
    done: Vec<Instance>,
    eye: Vec3,
}
