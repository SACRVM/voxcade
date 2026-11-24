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
        let (near, way) = ray(scene.view_proj, x / width.max(1.0) * 2.0 - 1.0, 1.0 - y / height.max(1.0) * 2.0)?;
        let world = &self.game.sim().world;
        let level = level_hit(world, near, way);
        let reach = level.map_or(f32::INFINITY, |(t, ..)| t);
        // The thing whose drawn voxels it meets first, before the level.
        let thing = thing_met(&self.models, &scene.picks, near, way, reach, model_met);
        // The level's area it enters first, before it meets the level.
        let area = self.game.areas().iter().enumerate().filter_map(|(i, &(lo, hi))| Some((box_enter(near, way, lo, hi)?, i))).filter(|&(t, _)| t < reach).min_by(|a, b| a.0.total_cmp(&b.0)).map_or(0, |(_, i)| (i + 1).min(u16::MAX as usize) as u16);
        let at = level.map(|(t, face)| {
            let p = near + way * t;
            ((p * 16.0).round().as_ivec3().to_array(), face)
        });
        Some(crate::input::Met { thing: thing.as_ref().map(|t| t.1), part: thing.and_then(|t| t.2), area, at })
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

/// The ray through the point (`nx`, `ny`) of the picture (-1 to 1, y up)
/// of a camera `view_proj`: where it begins on the near plane and its
/// way, of length 1.
fn ray(view_proj: Mat4, nx: f32, ny: f32) -> Option<(Vec3, Vec3)> {
    let inverse = view_proj.inverse();
    let (near, far) = (inverse.project_point3(Vec3::new(nx, ny, 0.0)), inverse.project_point3(Vec3::new(nx, ny, 1.0)));
    let way = (far - near).normalize_or_zero();
    (way != Vec3::ZERO && near.is_finite()).then_some((near, way))
}

/// The thing of `picks` (their models in `models`) whose drawn voxels a
/// ray from `from` along `way` meets first, nearer than `reach`, as `met`
/// finds a model's: where, the thing's key, its part. Of two met as near
/// the one drawn first, a figure's part before the parts after it.
fn thing_met(models: &Models, picks: &[Pick], from: Vec3, way: Vec3, reach: f32, met: Meeting) -> Option<(f32, u32, Option<String>)> {
    let mut thing: Option<(f32, u32, Option<String>)> = None;
    for pick in picks {
        let Some(model) = models.drawn(&pick.model) else { continue };
        let middle = pick.origin + Vec3::new(0.5, 0.5, model.size.z as f32 * 0.5);
        let radius = model.size.as_vec3().length() * 0.5 + 1.0;
        let best = thing.as_ref().map_or(reach, |t| t.0.min(reach));
        if sphere_enter(from, way, middle, radius).is_none_or(|t| t >= best) {
            continue;
        }
        if let Some((t, part)) = met(&model, pick, from, way, best) {
            thing = Some((t, pick.thing, part.map(|i| model.parts[i].name.clone())));
        }
    }
    thing
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

/// How a model drawn with a thing is met (`thing_met`): where a
/// ray from `from` along `way` first meets its opaque voxels as drawn,
/// nearer than `best`, and the part met (a figure's); None where nothing
/// is nearer. Of two parts met as near the first.
type Meeting = fn(&VoxModel, &Pick, Vec3, Vec3, f32) -> Option<(f32, Option<usize>)>;

/// How much further than the nearest voxel found so far a walk goes on
/// (`model_met`): a voxel the ray enters at `t` lies among the neighbours
/// of the cell the walk is in at `t`, entered no later, so this only takes
/// up rounding; more than a cell's diagonal.
const PAST: f32 = 2.0;

/// `Meeting` by a walk through each piece's own grid (a figure's part, or the
/// frame whole): the ray taken into the cells its voxels lie in after the
/// quarter turns (a voxel from `local` to `local + 1`), turned back by the
/// piece's free turn and pose and shifted back as `figures::voxel_middle`
/// places them, then walked cell by cell (as `level_hit` walks the level)
/// through the box of its voxels grown a cell each way. At each cell its
/// 26 neighbours are looked up in the piece's bits (`VoxModel::pick_cells`;
/// stepping on, the nine new ones) and each voxel there tried as every
/// voxel was (`cube_enter` on the same middle), so a voxel the ray grazes
/// beside the cells walked is tried too. The walk ends where the cells lie
/// `PAST` further than the nearest voxel met: the same answers as trying
/// every voxel (`model_met_by_every_voxel`, test
/// `the_walk_meets_what_every_voxel_meets`), a few dozen voxels tried.
fn model_met(model: &VoxModel, pick: &Pick, from: Vec3, way: Vec3, mut best: f32) -> Option<(f32, Option<usize>)> {
    let figures::Pieces { frame, turn, pieces } = figures::pieces(model, pick.frame, pick.look, pick.turn, &pick.pose);
    let back_turn = (4 - turn) % 4;
    let mut found = None;
    for (part, _, rot, shift) in pieces {
        let cells = model.pick_cells(part.unwrap_or(frame));
        if cells.size == IVec3::ZERO {
            continue;
        }
        let (from_g, way_g) = if crate::render::off_grid(rot, shift) {
            let back = rot.inverse();
            (back * (from - pick.origin - shift), back * way)
        } else {
            (from - pick.origin, way)
        };
        // The box of its voxels after the quarter turns, grown a cell.
        let top = cells.lo + cells.size - 1;
        let ((ax, ay), (bx, by)) = (crate::render::quarter(cells.lo.x, cells.lo.y, turn), crate::render::quarter(top.x, top.y, turn));
        let (lo, hi) = (IVec3::new(ax.min(bx), ay.min(by), cells.lo.z) - 1, IVec3::new(ax.max(bx), ay.max(by), top.z) + 1);
        let Some(enter) = box_enter(from_g, way_g, lo.as_vec3(), (hi + 1).as_vec3()) else { continue };
        if enter > best + PAST {
            continue;
        }
        let mut cell = (from_g + way_g * enter).floor().as_ivec3().clamp(lo, hi);
        let step = IVec3::new(way_g.x.signum() as i32, way_g.y.signum() as i32, way_g.z.signum() as i32);
        let mut t_max = Vec3::from_array(std::array::from_fn(|a| if way_g[a] == 0.0 { f32::INFINITY } else { ((cell[a] + (step[a] > 0) as i32) as f32 - from_g[a]) / way_g[a] }));
        let delta = Vec3::from_array(std::array::from_fn(|a| if way_g[a] == 0.0 { f32::INFINITY } else { 1.0 / way_g[a].abs() }));
        let mut tried = |c: IVec3, best: &mut f32| {
            let (x, y) = crate::render::quarter(c.x, c.y, back_turn);
            if !cells.has(IVec3::new(x, y, c.z)) {
                return;
            }
            let (middle, turned) = figures::voxel_middle(pick.origin, c.as_vec3(), rot, shift);
            if let Some(t) = cube_enter(from, way, middle, turned)
                && t < *best
            {
                *best = t;
                found = Some((t, part));
            }
        };
        // The first cell's neighbours, then each step's new ones.
        for d in 0..27 {
            tried(cell + IVec3::new(d % 3, d / 3 % 3, d / 9) - 1, &mut best);
        }
        for _ in 0..(hi - lo + 1).element_sum() {
            let axis = if t_max.x < t_max.y && t_max.x < t_max.z { 0 } else if t_max.y < t_max.z { 1 } else { 2 };
            let t = t_max[axis];
            t_max[axis] += delta[axis];
            cell[axis] += step[axis];
            if t > best + PAST || cell[axis] < lo[axis] || cell[axis] > hi[axis] {
                break;
            }
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for d in 0..9 {
                let mut c = cell;
                c[axis] += step[axis];
                c[u] += d % 3 - 1;
                c[v] += d / 3 - 1;
                tried(c, &mut best);
            }
        }
    }
    found
}

/// `Meeting` by trying every visible voxel of the model (the pointer's way
/// before the walk, `model_met`): what the walk must answer alike.
#[cfg(test)]
fn model_met_by_every_voxel(model: &VoxModel, pick: &Pick, from: Vec3, way: Vec3, mut best: f32) -> Option<(f32, Option<usize>)> {
    let mut found = None;
    figures::each_voxel(model, pick.frame, pick.origin, pick.look, pick.turn, &pick.pose, &mut |part, centre, rot, color| {
        // Glass is looked through, as light is.
        if color[3] < 255 {
            return;
        }
        if let Some(t) = cube_enter(from, way, centre, rot)
            && t < best
        {
            best = t;
            found = Some((t, part));
        }
    });
    found
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
    #[cfg(test)]
    tests::TRIED.with(|n| n.set(n.get() + 1));
    // Most voxels lie off the ray: none is tried whose ball the line
    // misses (half a cube's diagonal, squared 0.75, and a little more so
    // a grazed corner is never lost to rounding), turned any way.
    let to = centre - from;
    let along = to.dot(way);
    if to.length_squared() - along * along > 0.8 {
        return None;
    }
    let back = rot.inverse();
    let (from, way) = (back * (from - centre), back * way);
    box_enter(from, way, Vec3::splat(-0.5), Vec3::splat(0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ray: where it begins and its way, of length 1.
    type Ray = (Vec3, Vec3);

    thread_local! {
        /// How many voxels this thread tried (`cube_enter`).
        pub(super) static TRIED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }

    /// Random numbers for the scans, the same every run (xorshift).
    struct Dice(u64);

    impl Dice {
        /// From 0 to 1.
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }

        fn within(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * self.next()
        }

        fn way(&mut self) -> Vec3 {
            loop {
                let v = Vec3::new(self.within(-1.0, 1.0), self.within(-1.0, 1.0), self.within(-1.0, 1.0));
                if (0.01..=1.0).contains(&v.length_squared()) {
                    return v.normalize();
                }
            }
        }
    }

    /// The points of a voxel a ray grazes most easily: its corners, the
    /// middles of its edges and faces (offsets from its middle).
    fn grazed() -> Vec<Vec3> {
        let h = [-0.5, 0.0, 0.5];
        (0..27).map(|i| Vec3::new(h[i % 3], h[i / 3 % 3], h[i / 9])).filter(|o| *o != Vec3::ZERO).collect()
    }

    /// Rays aimed at a model drawn (`pick`): at a corner, an edge or a
    /// face of every `every`-th visible voxel (each point also a hair and
    /// a hundredth off), from where `from` says (the camera's ray through
    /// it, or a random eye); along an axis past it, grazing; and random
    /// rays through the model's ball.
    fn aimed(model: &VoxModel, pick: &Pick, every: usize, from: &dyn Fn(Vec3, &mut Dice) -> Option<Ray>, dice: &mut Dice) -> Vec<Ray> {
        let (offsets, mut points, mut k) = (grazed(), Vec::new(), 0);
        figures::each_voxel(model, pick.frame, pick.origin, pick.look, pick.turn, &pick.pose, &mut |_, centre, rot, color| {
            k += 1;
            if color[3] == 255 && k % every == 0 {
                points.push(centre + rot * offsets[(k / every) % offsets.len()]);
            }
        });
        let middle = pick.origin + Vec3::new(0.5, 0.5, model.size.z as f32 * 0.5);
        let reach = model.size.as_vec3().length() + 20.0;
        let mut rays = Vec::new();
        for p in points {
            for off in [Vec3::ZERO, dice.way() * 1e-5, dice.way() * 0.01] {
                rays.extend(from(p + off, dice));
            }
            let axis = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z][(dice.next() * 6.0) as usize % 6];
            rays.push((p - axis * reach, axis));
        }
        for _ in 0..rays.len() / 4 {
            let way = dice.way();
            rays.push((middle + dice.way() * reach * dice.next() - way * reach, way));
        }
        rays
    }

    /// Whether the walk meets the model drawn (`pick`) as trying every
    /// voxel does, for a ray from `from` along `way`: nearer than any,
    /// nearer than the nearest voxel by a hair or by half a voxel, and not
    /// nearer than it; whether it met it, or the mistake with the ray.
    fn walks_alike(model: &VoxModel, pick: &Pick, (from, way): Ray) -> Result<bool, String> {
        let bits = |m: Option<(f32, Option<usize>)>| m.map(|(t, part)| (t.to_bits(), part));
        let every = model_met_by_every_voxel(model, pick, from, way, f32::INFINITY);
        let mut asked = vec![(f32::INFINITY, bits(every))];
        if let Some((t, _)) = every {
            asked.extend([(f32::from_bits(t.to_bits() + 1), bits(every)), (t + 0.5, bits(every)), (t, None)]);
        }
        for (best, expected) in asked {
            let walked = bits(model_met(model, pick, from, way, best));
            if walked != expected {
                return Err(format!(
                    "{}, frame {}, at {}, look {}, turn {}, {} parts posed: the ray from {from} along {way} (bits {:?} {:?}), nearer than {best}: every voxel {expected:?}, the walk {walked:?}",
                    pick.model,
                    pick.frame,
                    pick.origin,
                    pick.look,
                    pick.turn,
                    pick.pose.len(),
                    from.to_array().map(f32::to_bits),
                    way.to_array().map(f32::to_bits)
                ));
            }
        }
        Ok(every.is_some())
    }

    /// `walks_alike` for every ray, on the cores there are: how many met
    /// the model; the first mistake fails.
    fn scan(model: &VoxModel, pick: &Pick, rays: &[Ray]) -> usize {
        let met = crate::par::in_order(rays, 256, |part| part.iter().map(|&r| walks_alike(model, pick, r)).collect());
        met.into_iter().map(|m| m.unwrap_or_else(|e| panic!("{e}")) as usize).sum()
    }

    /// The ray of the picture `view_proj` through the point `p`.
    fn through(view_proj: Mat4, p: Vec3) -> Option<Ray> {
        let c = view_proj * p.extend(1.0);
        (c.w > 0.0).then(|| ray(view_proj, c.x / c.w, c.y / c.w)).flatten()
    }

    /// A scene's pointer answered by the walk as by trying every voxel:
    /// each thing drawn by rays aimed at its voxels from the camera, then
    /// the picture's grid of `w` by `h` pointers and as many at random,
    /// every thing (`thing_met`) before the level. The rays tried.
    fn scan_scene(g: &Session<crate::Voxcade>, every: usize, (w, h): (usize, usize), dice: &mut Dice) -> usize {
        let scene = g.picked.borrow().clone();
        assert!(!scene.picks.is_empty(), "something drawn with a thing");
        let mut n = 0;
        for pick in &scene.picks {
            let model = g.models.drawn(&pick.model).expect("drawn");
            let rays = aimed(&model, pick, every, &|p, _| through(scene.view_proj, p), dice);
            let met = scan(&model, pick, &rays);
            n += rays.len();
            println!("  {}: {} rays, {met} on it", pick.model, rays.len());
        }
        let mut points: Vec<(f32, f32)> = (0..w * h).map(|i| (((i % w) as f32 + 0.5) / w as f32 * 2.0 - 1.0, ((i / w) as f32 + 0.5) / h as f32 * 2.0 - 1.0)).collect();
        points.extend((0..w * h).map(|_| (dice.within(-1.0, 1.0), dice.within(-1.0, 1.0))));
        let rays: Vec<Ray> = points.iter().filter_map(|&(x, y)| ray(scene.view_proj, x, y)).collect();
        let (world, models) = (&g.game.sim().world, &g.models);
        let bits = |m: Option<(f32, u32, Option<String>)>| m.map(|(t, k, part)| (t.to_bits(), k, part));
        let differ = crate::par::in_order(&rays, 256, |part| {
            part.iter()
                .map(|&(from, way)| {
                    let reach = level_hit(world, from, way).map_or(f32::INFINITY, |(t, _)| t);
                    let walked = bits(thing_met(models, &scene.picks, from, way, reach, model_met));
                    let every = bits(thing_met(models, &scene.picks, from, way, reach, model_met_by_every_voxel));
                    (walked != every).then(|| format!("the ray from {from} along {way} (bits {:?} {:?}): every voxel {every:?}, the walk {walked:?}", from.to_array().map(f32::to_bits), way.to_array().map(f32::to_bits)))
                })
                .collect()
        });
        if let Some(e) = differ.into_iter().flatten().next() {
            panic!("{e}");
        }
        println!("  the picture's pointers: {}", rays.len());
        n + rays.len()
    }

    /// The pointer's walk through each part's own grid (`model_met`)
    /// answers as trying every voxel did (`model_met_by_every_voxel`), to
    /// the bit: where, which thing, which part, over rays aimed at
    /// voxels' corners, edges and faces (a hair off and not), grazing
    /// along the axes, at random, and the picture's pointers, on Studio's
    /// machine (posed: keys, lamps, knobs, switches) and its table (the
    /// level's land), the Lights'
    /// posed figure and its load, More Blood's start; and on figures and
    /// models placed every way (four facings, turned freely and tipped off
    /// the quarter turns, posed and moved by less than a voxel, between
    /// voxels): a figure with mirrored clones and an empty part, Studio's
    /// machine, More Blood's soldier of frames. Studio's machine in fine
    /// voxels (380,000 of them) is aimed at more thinly, every 160th and
    /// 1800th voxel: trying every voxel costs with the voxels squared, and
    /// at the coarse machine's thinning the test took over five minutes.
    #[test]
    fn the_walk_meets_what_every_voxel_meets() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut dice = Dice(0x9e37_79b9_7f4a_7c15);
        let mut rays = 0;
        for (game, scenario, every, grid) in [("games/studio", None, 160, (128, 72)), ("games/lights", Some("posed"), 1, (320, 180)), ("games/lights", Some("load"), 7, (320, 180)), ("games/moreblood", None, 1, (320, 180))] {
            let mut g = Session::<crate::Voxcade>::load(&root.join(game)).unwrap();
            if let Some(s) = scenario {
                g.start_scenario(s).unwrap();
            }
            for _ in 0..30 {
                g.step(&crate::input::Input::default());
            }
            g.look();
            println!("{game} {scenario:?}:");
            rays += scan_scene(&g, every, grid, &mut dice);
        }
        let car = crate::vvx::read(&crate::vvx::tests::empty_car_text()).unwrap();
        let machine = crate::vox::load_model(&root.join("games/studio/machine.vvx")).unwrap();
        let soldier = crate::vox::load_model(&root.join("games/moreblood/enemy.vvx")).unwrap();
        assert!(car.parts.iter().any(|p| p.empty.is_some()) && soldier.frames.len() > 1);
        let origins = [Vec3::new(40.0, 30.0, 2.0), Vec3::new(40.37, 29.81, 2.25)];
        let turns = [Quat::IDENTITY, turning(90.0, 0.0, 0.0), turning(33.0, 0.0, 0.0), turning(17.0, 23.0, -11.0), turning(0.0, 90.0, 0.0), turning(-140.0, -61.0, 37.0)];
        for (name, model, every) in [("car", &car, 1), ("machine", &machine, 1800), ("soldier", &soldier, 1)] {
            let mut n = 0;
            for (i, look) in [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X].into_iter().enumerate() {
                for (j, &turn) in turns.iter().enumerate() {
                    let pose: Vec<PartPose> = if (i + j) % 2 == 1 {
                        let mut posed = || PartPose { turn: turning(dice.within(-40.0, 40.0), dice.within(-40.0, 40.0), dice.within(-40.0, 40.0)), offset: Vec3::new(dice.within(-0.5, 0.5), dice.within(-0.5, 0.5), dice.within(-0.5, 0.5)), glow: None };
                        model.parts.iter().map(|_| posed()).collect()
                    } else {
                        Vec::new()
                    };
                    let pick = Pick { thing: 1, model: name.into(), frame: (i + j) % model.frames.len(), origin: origins[j % 2], look, turn, pose };
                    let eye = |p: Vec3, dice: &mut Dice| {
                        let from = p + dice.way() * (model.size.as_vec3().length() + 30.0);
                        Some((from, (p - from).normalize()))
                    };
                    let rays = aimed(model, &pick, every, &eye, &mut dice);
                    scan(model, &pick, &rays);
                    n += rays.len();
                }
            }
            println!("{name} placed every way: {n} rays");
            rays += n;
        }
        println!("{rays} rays in all, every one answered alike");
    }

    /// What a pointer costs on Studio, a machine of 230 parts in fine
    /// voxels (512 by 310, 358,000 voxels drawn) on the level's land: a
    /// grid of 64 by 36 pointers over the picture, the things met by the
    /// walk and by trying every voxel, and whole answers (`point`); said,
    /// and the walk kept under 300 voxels tried and 40 µs a pointer on
    /// average (2026-11-24: 17 voxels and 12 µs, over a key 27 and 13 µs;
    /// every voxel tried: some 343,000 and 3.5 ms).
    #[test]
    fn the_studios_pointer_is_answered_quickly() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut g = Session::<crate::Voxcade>::load(&root.join("games/studio")).unwrap();
        for _ in 0..30 {
            g.step(&crate::input::Input::default());
        }
        g.look();
        let scene = g.picked.borrow().clone();
        let points: Vec<(f32, f32)> = (0..64 * 36).map(|i| ((i % 64) as f32 + 0.5, (i / 64) as f32 + 0.5)).collect();
        let rays: Vec<Ray> = points.iter().filter_map(|&(x, y)| ray(scene.view_proj, x / 32.0 - 1.0, 1.0 - y / 18.0)).collect();
        let world = &g.game.sim().world;
        let started = std::time::Instant::now();
        let first = thing_met(&g.models, &scene.picks, rays[0].0, rays[0].1, f32::INFINITY, model_met);
        let made = started.elapsed().as_secs_f64() * 1e3;
        let bytes: usize = scene.picks.iter().map(|p| g.models.drawn(&p.model).map_or(0, |m| (0..m.parts.len().max(1)).map(|i| m.pick_cells(i).bytes()).sum())).sum();
        println!("studio: its bits made with the first pointer in {made:.3} ms, {bytes} bytes ({})", if first.is_some() { "on a thing" } else { "on none" });
        let mut each = [0.0; 2];
        for (k, (how, met)) in [("the walk", model_met as Meeting), ("every voxel", model_met_by_every_voxel)].into_iter().enumerate() {
            let tried = TRIED.with(|n| n.get());
            let (mut things, started) = (0, std::time::Instant::now());
            for &(from, way) in &rays {
                let reach = level_hit(world, from, way).map_or(f32::INFINITY, |(t, _)| t);
                things += thing_met(&g.models, &scene.picks, from, way, reach, met).is_some() as u32;
            }
            each[k] = started.elapsed().as_secs_f64() * 1e6 / rays.len() as f64;
            let tried = (TRIED.with(|n| n.get()) - tried) as f64 / rays.len() as f64;
            println!("studio, {how}: {:.1} µs and {tried:.0} voxels tried a pointer ({things} of {} on a thing)", each[k], rays.len());
            if k == 0 {
                assert!(tried < 300.0, "{tried:.0} voxels tried a pointer");
            }
        }
        // Over the machine (step_2).
        let p = g.point(99.0, 144.0, 16.0 / 9.0);
        let (tried, started) = (TRIED.with(|n| n.get()), std::time::Instant::now());
        for _ in 0..100 {
            g.point(99.0, 144.0, 16.0 / 9.0);
        }
        let over = started.elapsed().as_secs_f64() * 1e4;
        println!("studio, the walk over {:?}: {over:.1} µs and {} voxels tried a pointer", p.level.as_ref().and_then(|m| m.part.clone()), (TRIED.with(|n| n.get()) - tried) / 100);
        assert!(each[0] < 40.0, "{:.1} µs a pointer", each[0]);
    }

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
