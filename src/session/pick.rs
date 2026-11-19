//! What the pointer meets in the level, found on the CPU from the picture
//! as it was composed (`docs/game-model.md`, "input"): the thing whose
//! drawn voxels it meets first (voxel-exact on the drawn, posed figure,
//! its part), the level's area it enters first, the point where it meets
//! the level's voxels and solid pieces (glass and water looked through,
//! things too). No GPU: `look` composes the same, so `inspect`, `check` and
//! replays have it; the answers are the input, never asked again.

use super::*;

/// A model drawn with a thing in a picture (`Drawing::of_thing`): which,
/// how, to find again what the pointer meets of it.
#[derive(Clone, Debug)]
pub struct Pick {
    /// The thing's key (`Drawing::of_thing`: its raw id, or a player's
    /// handle).
    pub thing: u32,
    pub model: String,
    pub frame: usize,
    pub origin: Vec3,
    pub look: IVec2,
    pub turn: Quat,
    pub pose: Vec<PartPose>,
}

/// The picture the pointer is answered from: its camera, the level's
/// picture it was (`input::picture_mark`), what it drew with things.
#[derive(Clone, Debug, Default)]
pub struct PickScene {
    pub view_proj: Mat4,
    pub picks: Vec<Pick>,
}

/// The most cells a pointer's ray walks through the level.
const MOST_STEPS: usize = 8192;

impl<G: Game> Session<G> {
    /// What the pointer at (`x`, `y`) of a HUD `width` by `height` meets
    /// in the level as the picture drawn last showed it (`Met`); None
    /// where nothing was drawn yet.
    pub(super) fn meets(&self, x: f32, y: f32, width: f32, height: f32) -> Option<crate::input::Met> {
        let scene = self.picked.borrow();
        if scene.view_proj == Mat4::ZERO {
            return None;
        }
        let (nx, ny) = (x / width.max(1.0) * 2.0 - 1.0, 1.0 - y / height.max(1.0) * 2.0);
        let inverse = scene.view_proj.inverse();
        let (near, far) = (inverse.project_point3(Vec3::new(nx, ny, 0.0)), inverse.project_point3(Vec3::new(nx, ny, 1.0)));
        let way = (far - near).normalize_or_zero();
        if way == Vec3::ZERO || !near.is_finite() {
            return None;
        }
        let world = &self.game.sim().world;
        let level = level_hit(world, near, way);
        let reach = level.map_or(f32::INFINITY, |(t, ..)| t);
        // The thing whose drawn voxels it meets first, before the level.
        let mut thing: Option<(f32, u32, Option<String>)> = None;
        for pick in &scene.picks {
            let Some(model) = self.models.drawn(&pick.model) else { continue };
            let middle = pick.origin + Vec3::new(0.5, 0.5, model.size.z as f32 * 0.5);
            let radius = model.size.as_vec3().length() * 0.5 + 1.0;
            let best = thing.as_ref().map_or(reach, |t| t.0.min(reach));
            if sphere_enter(near, way, middle, radius).is_none_or(|t| t >= best) {
                continue;
            }
            figures::each_voxel(&model, pick.frame, pick.origin, pick.look, pick.turn, &pick.pose, &mut |part, centre, rot, color| {
                // Glass is looked through, as light is.
                if color[3] < 255 {
                    return;
                }
                if let Some(t) = cube_enter(near, way, centre, rot)
                    && t < thing.as_ref().map_or(reach, |b| b.0.min(reach))
                {
                    thing = Some((t, pick.thing, part.map(|i| model.parts[i].name.clone())));
                }
            });
        }
        // The level's area it enters first, before it meets the level.
        let area = self.game.areas().iter().enumerate().filter_map(|(i, &(lo, hi))| Some((box_enter(near, way, lo, hi)?, i))).filter(|&(t, _)| t < reach).min_by(|a, b| a.0.total_cmp(&b.0)).map_or(0, |(_, i)| (i + 1).min(u16::MAX as usize) as u16);
        let at = level.map(|(t, face)| {
            let p = near + way * t;
            ((p * 16.0).round().as_ivec3().to_array(), face)
        });
        Some(crate::input::Met { thing: thing.as_ref().map(|t| t.1), part: thing.and_then(|t| t.2).filter(|p| p.len() <= crate::input::MOST_PART_NAME), area, at })
    }

    /// Where the point `at` of the level shows in the picture drawn last,
    /// in the HUD's measure of a window of shape `aspect` (what a pointer
    /// there is over: a script's `pointer=X,Y` found from a thing).
    pub fn to_hud(&self, at: Vec3, aspect: f32) -> Option<[f32; 2]> {
        let layout = crate::ui::Layout::of(&self.cfg.ui, self.picture_aspect(aspect));
        let clip = self.picked.borrow().view_proj * at.extend(1.0);
        (clip.w > 0.0).then(|| [(clip.x / clip.w + 1.0) * 0.5 * layout.width, (1.0 - clip.y / clip.w) * 0.5 * layout.height])
    }

    /// The way from seat `seat`'s figure as drawn to the pointer at (`x`,
    /// `y`) of a HUD `width` by `height`, on the screen (y up), length 1 in
    /// 127ths (`input.aim` from the mouse); none without a figure.
    pub fn aim_at(&self, x: f32, y: f32, width: f32, height: f32, seat: usize) -> Option<[i8; 2]> {
        let at = self.game.seat_at(seat)?;
        let scene = self.picked.borrow();
        if scene.view_proj == Mat4::ZERO {
            return None;
        }
        let clip = scene.view_proj * (at + Vec3::new(0.5, 0.5, 0.0)).extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let (fx, fy) = ((clip.x / clip.w + 1.0) * 0.5 * width, (1.0 - clip.y / clip.w) * 0.5 * height);
        let (dx, dy) = (x - fx, fy - y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            return None;
        }
        let [sx, sy] = crate::input::stick(dx / len, -dy / len, 0.0);
        Some([sx, sy])
    }
}

/// Where a ray from `from` along `way` (of length 1) meets the level's
/// voxels or solid pieces first (glass and water looked through), and the
/// face it meets (`mesh::DIRS`' order): None into the sky or off the level.
fn level_hit(world: &crate::world::World, from: Vec3, way: Vec3) -> Option<(f32, u8)> {
    let size = world.size.as_vec3();
    let start = box_enter(from, way, Vec3::ZERO, size)?;
    let p = from + way * (start + 1e-4);
    let mut cell = p.floor().as_ivec3().clamp(IVec3::ZERO, world.size - 1);
    let step = IVec3::new(way.x.signum() as i32, way.y.signum() as i32, way.z.signum() as i32);
    let next = |c: i32, s: i32| (c + (s > 0) as i32) as f32;
    let across = |w: f32| if w == 0.0 { f32::INFINITY } else { 1.0 / w.abs() };
    let mut t_max = Vec3::new(
        if way.x == 0.0 { f32::INFINITY } else { (next(cell.x, step.x) - from.x) / way.x },
        if way.y == 0.0 { f32::INFINITY } else { (next(cell.y, step.y) - from.y) / way.y },
        if way.z == 0.0 { f32::INFINITY } else { (next(cell.z, step.z) - from.z) / way.z },
    );
    let delta = Vec3::new(across(way.x), across(way.y), across(way.z));
    // The face it came in by: the box's side it entered through, then the
    // side of the cell it stepped across.
    let entered = |axis: usize, s: i32| -> u8 { (axis * 2 + (s > 0) as usize) as u8 };
    let first = {
        let hit = from + way * start;
        let d = [hit.x.min(size.x - hit.x), hit.y.min(size.y - hit.y), hit.z.min(size.z - hit.z)];
        let axis = (0..3).min_by(|&a, &b| d[a].total_cmp(&d[b])).unwrap_or(2);
        entered(axis, [step.x, step.y, step.z][axis])
    };
    let (mut t, mut face) = (start, first);
    for _ in 0..MOST_STEPS {
        if !world.in_bounds(cell) {
            return None;
        }
        let v = world.get(cell);
        if (v != 0 && world.palette[v as usize][3] == 255) || world.piece_at(cell) {
            return Some((t.max(0.0), face));
        }
        let axis = if t_max.x < t_max.y && t_max.x < t_max.z { 0 } else if t_max.y < t_max.z { 1 } else { 2 };
        t = t_max[axis];
        t_max[axis] += delta[axis];
        let s = [step.x, step.y, step.z][axis];
        cell[axis] += s;
        face = entered(axis, s);
    }
    None
}

/// Where a ray enters the box from `lo` to `hi` (0 when it starts inside),
/// if it does.
fn box_enter(from: Vec3, way: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
    let (mut near, mut far) = (0.0f32, f32::INFINITY);
    for a in 0..3 {
        if way[a] == 0.0 {
            if from[a] < lo[a] || from[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (t1, t2) = ((lo[a] - from[a]) / way[a], (hi[a] - from[a]) / way[a]);
        near = near.max(t1.min(t2));
        far = far.min(t1.max(t2));
    }
    (near <= far).then_some(near)
}

/// Where a ray enters a sphere, if it does (0 inside).
fn sphere_enter(from: Vec3, way: Vec3, middle: Vec3, radius: f32) -> Option<f32> {
    let to = middle - from;
    let along = to.dot(way);
    let off = to.length_squared() - along * along;
    if off > radius * radius {
        return None;
    }
    let half = (radius * radius - off).sqrt();
    let t = along - half;
    (along + half >= 0.0).then_some(t.max(0.0))
}

/// Where a ray enters a voxel (a cube a voxel big about `centre`, turned
/// by `rot`), if it does.
fn cube_enter(from: Vec3, way: Vec3, centre: Vec3, rot: Quat) -> Option<f32> {
    let back = rot.inverse();
    let (from, way) = (back * (from - centre), back * way);
    box_enter(from, way, Vec3::splat(-0.5), Vec3::splat(0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a pointer costs to answer on a big scene: the Bench (a map of
    /// 640 by 2368, its soldiers) and the Lights' `load` (many figures
    /// posed and lit), a grid of pointers over the picture; said, and kept
    /// under a millisecond each on average.
    #[test]
    fn a_pointer_is_answered_quickly_on_a_big_scene() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for (game, scenario) in [("games/bench", None), ("games/lights", Some("load")), ("games/moreblood", None)] {
            let mut g = Session::<crate::Voxcade>::load(&root.join(game)).unwrap();
            if let Some(s) = scenario {
                g.start_scenario(s).unwrap();
            }
            for _ in 0..30 {
                g.step(&crate::input::Input::default());
            }
            g.look();
            let layout = crate::ui::Layout::of(&g.cfg.ui, 16.0 / 9.0);
            let (mut n, mut things, started) = (0, 0, std::time::Instant::now());
            for i in 0..16 {
                for j in 0..9 {
                    let p = g.point((i as f32 + 0.5) / 16.0 * layout.width, (j as f32 + 0.5) / 9.0 * layout.height, 16.0 / 9.0);
                    things += p.level.and_then(|m| m.thing).is_some() as u32;
                    n += 1;
                }
            }
            let each = started.elapsed().as_secs_f64() * 1000.0 / n as f64;
            println!("{game}: {each:.3} ms a pointer ({} things drawn, {things} of {n} pointers on one)", g.picked.borrow().picks.len());
            assert!(each < 1.0, "{game}: {each:.3} ms a pointer");
        }
    }

    /// A ray meets a voxel of the level on the face it comes in by; one
    /// into the sky meets nothing.
    #[test]
    fn a_ray_meets_the_level_on_its_face() {
        let mut palette = [[0u8; 4]; 256];
        palette[1] = [200, 200, 200, 255];
        let mut world = crate::world::World::new(IVec3::new(16, 16, 8), palette);
        world.set(IVec3::new(5, 5, 0), 1);
        let (t, face) = level_hit(&world, Vec3::new(5.5, 5.5, 20.0), Vec3::NEG_Z).expect("met");
        assert!((t - 19.0).abs() < 1e-4 && face == 4, "{t} {face}");
        let (t, face) = level_hit(&world, Vec3::new(0.0, 5.5, 0.5), Vec3::X).expect("met");
        assert!((t - 5.0).abs() < 1e-4 && face == 1, "{t} {face}");
        assert_eq!(level_hit(&world, Vec3::new(5.5, 5.5, 20.0), Vec3::Z), None);
        world.palette[1][3] = 120;
        assert_eq!(level_hit(&world, Vec3::new(5.5, 5.5, 20.0), Vec3::NEG_Z), None, "glass is looked through");
    }
}
