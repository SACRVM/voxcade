//! Handles on the thing chosen in a 3D view, as 3D tools have them: three
//! arrows from its middle to move it along an axis each, three rings round
//! it to turn it about one each, and small squares on a box's faces to
//! size it. X red, Y green, Z blue, the convention people know; the one
//! under the mouse lit.
//!
//! This knows nothing of levels or editors: it is given where the middle
//! is, which ways the arrows and rings go and how big they are, and works
//! out what a person sees (lines on the screen, in pixels), what the mouse
//! is over (`Handles::grip_at`) and how far a drag has moved or turned
//! (`Drag`). The level editor puts them on its pieces (`room.rs`); the
//! figure editor can put a ring on a part's joint the same way.
//!
//! Everything is measured on the screen where that is what a person
//! judges: the handles are a share of the view's height whatever the
//! thing's size (a small piece is not drowned, a big one not lost), and
//! the mouse grabs within a few pixels of a line.

use glam::{Mat4, Vec2, Vec3};

/// What part of the handles is grabbed: an arrow or a ring by its axis
/// (0 x, 1 y, 2 z), a box's face by its axis and side (`true`: the high
/// side).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    Arrow(usize),
    Ring(usize),
    Face(usize, bool),
}

impl Grip {
    /// The axis it belongs to, for its colour.
    pub fn axis(self) -> usize {
        match self {
            Grip::Arrow(a) | Grip::Ring(a) | Grip::Face(a, _) => a.min(2),
        }
    }
}

/// The handles on one thing: where their middle is, the arrows' and the
/// rings' directions (unit; None leaves one out), how long an arrow is at
/// least (world units: `world_size` makes it a share of the view), a
/// ring's radius, a box whose faces are sized (low and high corner), and
/// the thing's own outline (points round it: the corners of its box), which
/// the handles leave to the thing: a press there is the thing's, an
/// arrow's head lies beyond it.
#[derive(Clone, Debug, PartialEq)]
pub struct Handles {
    pub centre: Vec3,
    pub length: f32,
    pub ring: f32,
    pub arrows: [Option<Vec3>; 3],
    pub rings: [Option<Vec3>; 3],
    pub faces: Option<(Vec3, Vec3)>,
    pub body: Vec<Vec3>,
    /// Only the arrows' heads grab (where a press on the shaft's line has
    /// work of its own: the voxel editor's tools).
    pub heads_only: bool,
}

/// A ring's radius and where an arrow's shaft begins, as shares of an
/// arrow's length (the shaft clear of the middle, where the thing itself
/// is grabbed, when it has no outline).
const RING: f32 = 0.62;
const SHAFT_FROM: f32 = 0.2;
/// How many straight pieces a ring is drawn of.
const RING_STEPS: usize = 48;
/// How near the mouse must be to a line to grab it (pixels).
pub const GRAB: f32 = 9.0;
/// A face's square and an arrow's head (pixels across), how far a tip
/// lies beyond the thing's outline at least, and how short an arrow's
/// visible part may be before it counts as seen end on (dimmed, not
/// grabbed: the thing dragged over the floor moves that way too).
pub const SQUARE: f32 = 11.0;
const HEAD: f32 = 10.0;
const VISIBLE: f32 = 60.0;
pub const END_ON: f32 = 15.0;

/// What a handle is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// A line (an arrow's shaft).
    Shaft,
    /// A filled triangle (an arrow's head: tip first).
    Head,
    /// A closed line round (a ring), each piece before or behind the
    /// middle as seen.
    Ring,
    /// A filled square (a face's).
    Face,
}

/// One handle on the screen: what it does, how it is drawn, its points in
/// pixels from the top left, and for a ring whether each of its pieces
/// lies behind the middle (drawn quieter).
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub grip: Grip,
    pub what: What,
    pub points: Vec<Vec2>,
    pub behind: Vec<bool>,
    /// Seen end on: drawn dimmed, not grabbed.
    pub dim: bool,
}

/// The box round points on the screen (None if one lies behind the eye).
fn screen_box(view_proj: Mat4, screen: Vec2, points: &[Vec3]) -> Option<(Vec2, Vec2)> {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for p in points {
        let (s, _) = project(view_proj, screen, *p)?;
        (lo, hi) = (lo.min(s), hi.max(s));
    }
    (!points.is_empty()).then_some((lo, hi))
}

/// Whether a point of the screen lies in a box grown by `by`.
fn in_box(p: Vec2, (lo, hi): (Vec2, Vec2), by: f32) -> bool {
    p.cmpge(lo - by).all() && p.cmple(hi + by).all()
}

/// A point of the world on the screen (pixels from the top left of a
/// `screen` big), and how far from the eye (the clip's w); None behind the
/// eye.
pub fn project(view_proj: Mat4, screen: Vec2, p: Vec3) -> Option<(Vec2, f32)> {
    let c = view_proj * p.extend(1.0);
    if c.w <= 1e-4 {
        return None;
    }
    let (x, y) = (c.x / c.w, c.y / c.w);
    Some((Vec2::new((x + 1.0) * 0.5 * screen.x, (1.0 - y) * 0.5 * screen.y), c.w))
}

/// How long something at `at` must be in the world to look `share` of the
/// screen's height: the handles' size, the same on the screen however near
/// or far the thing is and however big.
pub fn world_size(view_proj: Mat4, at: Vec3, share: f32) -> f32 {
    let c = view_proj * at.extend(1.0);
    if c.w <= 1e-4 {
        return 1.0;
    }
    let inv = view_proj.inverse();
    let ndc = c.truncate() / c.w;
    let back = |y: f32| {
        let p = inv * glam::Vec4::new(ndc.x, y, ndc.z, 1.0);
        p.truncate() / p.w
    };
    (back(ndc.y + share * 2.0) - back(ndc.y)).length().max(1e-3)
}

/// The nearest a point comes to the piece of line from `a` to `b`.
fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

/// Two directions square to `n` (unit) and to each other: a ring's plane.
fn plane_of(n: Vec3) -> (Vec3, Vec3) {
    let other = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let u = n.cross(other).normalize();
    (u, n.cross(u))
}

impl Handles {
    /// Handles at `centre` `length` long: no arrows, rings or faces yet.
    pub fn at(centre: Vec3, length: f32) -> Self {
        Self { centre, length, ring: length * RING, arrows: [None; 3], rings: [None; 3], faces: None, body: Vec::new(), heads_only: false }
    }

    /// The thing's outline on the screen: the box round its points.
    pub fn body_on_screen(&self, view_proj: Mat4, screen: Vec2) -> Option<(Vec2, Vec2)> {
        screen_box(view_proj, screen, &self.body)
    }

    /// The handles as the screen shows them, in the order they are drawn:
    /// the rings first, then the faces, then the arrows on top.
    pub fn shapes(&self, view_proj: Mat4, screen: Vec2) -> Vec<Shape> {
        let mut out = Vec::new();
        let on = |p: Vec3| project(view_proj, screen, p);
        let Some((_, middle_w)) = on(self.centre) else { return out };
        for (axis, n) in self.rings.iter().enumerate() {
            let Some(n) = n else { continue };
            let (u, v) = plane_of(*n);
            let r = self.ring;
            let round: Vec<(Vec2, f32)> = (0..=RING_STEPS)
                .filter_map(|k| {
                    let a = k as f32 / RING_STEPS as f32 * std::f32::consts::TAU;
                    on(self.centre + (u * a.cos() + v * a.sin()) * r)
                })
                .collect();
            if round.len() < RING_STEPS + 1 {
                continue;
            }
            let behind = round.windows(2).map(|w| (w[0].1 + w[1].1) * 0.5 > middle_w).collect();
            out.push(Shape { grip: Grip::Ring(axis), what: What::Ring, points: round.into_iter().map(|(p, _)| p).collect(), behind, dim: false });
        }
        if let Some((lo, hi)) = self.faces {
            let mid = (lo + hi) * 0.5;
            for axis in 0..3 {
                for high in [false, true] {
                    let mut p = mid;
                    p[axis] = if high { hi[axis] } else { lo[axis] };
                    let Some((at, _)) = on(p) else { continue };
                    let h = SQUARE * 0.5;
                    let points = vec![at + Vec2::new(-h, -h), at + Vec2::new(h, -h), at + Vec2::new(h, h), at + Vec2::new(-h, h)];
                    out.push(Shape { grip: Grip::Face(axis, high), what: What::Face, points, behind: Vec::new(), dim: false });
                }
            }
        }
        let body = self.body_on_screen(view_proj, screen);
        let Some((middle, _)) = on(self.centre) else { return out };
        for (axis, d) in self.arrows.iter().enumerate() {
            let Some(d) = d else { continue };
            let point = |t: f32| on(self.centre + *d * t).map(|(p, _)| p);
            // Out of the thing's outline (where the line leaves it, seen
            // towards a far point), its head a good stretch beyond: as long
            // as that asks, the view's share at least, a few times it at most.
            let Some(far) = point(self.length * 6.0) else { continue };
            let from = match body {
                Some(b) => (0..=120).map(|k| middle + (far - middle) * (k as f32 / 120.0)).find(|p| !in_box(*p, b, 0.0)).unwrap_or(far),
                None => point(self.length * SHAFT_FROM).unwrap_or(middle),
            };
            let mut t = self.length;
            while t < self.length * 6.0 && point(t).is_some_and(|tip| tip.distance(middle) < from.distance(middle) + VISIBLE) {
                t *= 1.08;
            }
            let Some(tip) = point(t) else { continue };
            let along = (tip - from).try_normalize().unwrap_or(Vec2::Y);
            let dim = tip.distance(from) < END_ON;
            let base = tip - along * (HEAD * 1.6).min(tip.distance(from));
            out.push(Shape { grip: Grip::Arrow(axis), what: What::Shaft, points: vec![from, base], behind: Vec::new(), dim });
            // The head: square to the shaft on the screen.
            let side = Vec2::new(-along.y, along.x) * HEAD * 0.75;
            out.push(Shape { grip: Grip::Arrow(axis), what: What::Head, points: vec![tip, base + side, base - side], behind: Vec::new(), dim });
        }
        out
    }

    /// The handle under the mouse (pixels from the top left), if one is
    /// within `GRAB`: the one whose line is nearest. Inside the thing's
    /// outline only an arrow's head grabs (the thing is pressed there);
    /// an arrow seen end on never.
    pub fn grip_at(&self, view_proj: Mat4, screen: Vec2, mouse: Vec2) -> Option<Grip> {
        let mut best: Option<(f32, Grip)> = None;
        let middle = project(view_proj, screen, self.centre).map_or(Vec2::ZERO, |(p, _)| p);
        let body = self.body_on_screen(view_proj, screen);
        let on_body = body.is_some_and(|b| in_box(mouse, b, 0.0));
        for s in self.shapes(view_proj, screen).into_iter().filter(|s| !s.dim) {
            let d = match s.what {
                What::Shaft if on_body || self.heads_only => f32::MAX,
                What::Ring if on_body => f32::MAX,
                What::Shaft => to_segment(mouse, s.points[0], s.points[1]),
                What::Ring => {
                    // A ring seen nearly edge on is a line across the
                    // thing: its inner part is left to the thing.
                    let across = s.points.iter().map(|p| p.distance(middle)).fold(0.0, f32::max);
                    let outer = s.points.windows(2).filter(|w| ((w[0] + w[1]) * 0.5).distance(middle) >= across * 0.5);
                    outer.map(|w| to_segment(mouse, w[0], w[1])).fold(f32::MAX, f32::min)
                }
                What::Head | What::Face => {
                    let middle = s.points.iter().copied().sum::<Vec2>() / s.points.len() as f32;
                    let edge = s.points.iter().zip(s.points.iter().cycle().skip(1)).map(|(a, b)| to_segment(mouse, *a, *b)).fold(f32::MAX, f32::min);
                    if mouse.distance(middle) <= SQUARE * 0.6 { 0.0 } else { edge }
                }
            };
            if d <= GRAB && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, s.grip));
            }
        }
        best.map(|(_, g)| g)
    }

    /// The direction a grip moves along (an arrow's, a face's axis) or
    /// turns about (a ring's).
    pub fn direction(&self, grip: Grip) -> Option<Vec3> {
        match grip {
            Grip::Arrow(a) => self.arrows.get(a).copied().flatten(),
            Grip::Ring(a) => self.rings.get(a).copied().flatten(),
            Grip::Face(a, _) => self.faces.map(|_| Vec3::AXES[a.min(2)]),
        }
    }
}

/// How a drag measures: along a line in the world (the mouse's ray's
/// nearest point to it), along the line as the screen shows it (where the
/// line points nearly at the eye), or round the middle on the screen (a
/// ring).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Measure {
    Line { from: f32 },
    Screen { unit: Vec2 },
    Round { toward: bool, last: f32, swept: f32 },
}

/// A handle being dragged: what was grabbed, where the mouse was, and what
/// it has measured so far. `amount` says how far it went since the press:
/// world units along an arrow's or a face's axis, degrees about a ring's
/// (a turn the right hand's way about it, as many times round as dragged).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub grip: Grip,
    press: Vec2,
    measure: Measure,
    /// The pointer has gone `TRAVEL` from the press: only then does the
    /// drag move anything (a click on a handle moves nothing).
    travelled: bool,
}

/// How far the pointer goes (pixels) before a handle's drag moves
/// anything, as a click becomes a drag.
pub const TRAVEL: f32 = 4.0;

/// The most an arrow's or a face's drag goes from its press (voxels): a
/// ray nearly along an arrow measures absurd distances, which would throw
/// a part where it cannot be seen.
pub const MOST: f32 = 512.0;

/// The parameter along the line through `p` in direction `a` (unit) of
/// its nearest point to the ray from `o` along `d` (unit); None where the
/// two are nearly one way.
fn along_line(p: Vec3, a: Vec3, o: Vec3, d: Vec3) -> Option<f32> {
    let b = a.dot(d);
    let denom = 1.0 - b * b;
    if denom < 0.04 {
        return None;
    }
    let w = p - o;
    Some((b * d.dot(w) - a.dot(w)) / denom)
}

/// The angle of a point about the middle on the screen, counterclockwise
/// as seen (the screen's y runs down), in degrees.
fn screen_angle(middle: Vec2, p: Vec2) -> f32 {
    (-(p.y - middle.y)).atan2(p.x - middle.x).to_degrees()
}

impl Drag {
    /// The press on `grip` with the mouse at `mouse` (pixels) and its ray
    /// (`origin`, `dir` unit) into the world.
    pub fn begin(h: &Handles, grip: Grip, view_proj: Mat4, screen: Vec2, mouse: Vec2, (origin, dir): (Vec3, Vec3)) -> Option<Drag> {
        let axis = h.direction(grip)?;
        let (middle, middle_w) = project(view_proj, screen, h.centre)?;
        let measure = match grip {
            Grip::Ring(_) => {
                // The axis pointing at the eye: turning the right hand's
                // way is turning counterclockwise as seen.
                let toward = project(view_proj, screen, h.centre + axis * h.length * 0.1).is_some_and(|(_, w)| w < middle_w);
                Measure::Round { toward, last: screen_angle(middle, mouse), swept: 0.0 }
            }
            Grip::Arrow(_) | Grip::Face(..) => match along_line(h.centre, axis, origin, dir) {
                Some(from) => Measure::Line { from },
                None => {
                    let (end, _) = project(view_proj, screen, h.centre + axis)?;
                    Measure::Screen { unit: end - middle }
                }
            },
        };
        Some(Drag { grip, press: mouse, measure, travelled: false })
    }

    /// How far the drag has gone since the press, the mouse now at
    /// `mouse` with its ray: along the axis in world units, or degrees
    /// about the ring's axis.
    pub fn amount(&mut self, h: &Handles, view_proj: Mat4, screen: Vec2, mouse: Vec2, ray: (Vec3, Vec3)) -> f32 {
        let a = self.measured(h, view_proj, screen, mouse, ray);
        // Never NaN or endless: none, or along an axis as far as `MOST` (a
        // ring may go round and round).
        let most = if matches!(self.measure, Measure::Round { .. }) { f32::MAX } else { MOST };
        if a.is_finite() { a.clamp(-most, most) } else { 0.0 }
    }

    /// Whether the pointer at `mouse` has gone far enough from the press
    /// for the drag to move anything (once it has, it stays so).
    pub fn travels(&mut self, mouse: Vec2) -> bool {
        self.travelled = self.travelled || mouse.distance(self.press) >= TRAVEL;
        self.travelled
    }

    fn measured(&mut self, h: &Handles, view_proj: Mat4, screen: Vec2, mouse: Vec2, (origin, dir): (Vec3, Vec3)) -> f32 {
        let Some(axis) = h.direction(self.grip) else { return 0.0 };
        match &mut self.measure {
            Measure::Line { from } => along_line(h.centre, axis, origin, dir).map_or(0.0, |s| s - *from),
            Measure::Screen { unit } => (mouse - self.press).dot(*unit) / unit.length_squared().max(1e-6),
            Measure::Round { toward, last, swept } => {
                let Some((middle, _)) = project(view_proj, screen, h.centre) else { return *swept };
                let now = screen_angle(middle, mouse);
                // The way round since the last motion, the short way.
                *swept += (now - *last + 180.0).rem_euclid(360.0) - 180.0;
                *last = now;
                if *toward { *swept } else { -*swept }
            }
        }
    }
}

/// An angle a ring turns to, `degrees` dragged on from `from`: on the
/// grid (`quarters`) the nearest quarter turn; else whole degrees, the
/// quarter turns within `snap` of them taken. Always 0 to 360.
pub fn step(from: f64, degrees: f64, quarters: bool, snap: f64) -> f64 {
    let to = (from + degrees).rem_euclid(360.0);
    let quarter = (to / 90.0).round() * 90.0;
    let out = if quarters || (to - quarter).abs() <= snap { quarter } else { to.round() };
    out.rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::view::{FOV, Orbit, mouse_ray};

    const SCREEN: Vec2 = Vec2::new(1600.0, 900.0);

    /// The handles of a thing at the middle of the view, the world's
    /// three ways, and the view seen from `yaw` and `pitch`.
    fn seen(yaw: f32, pitch: f32) -> (Handles, Mat4) {
        let centre = Vec3::new(40.0, 30.0, 10.0);
        let orbit = Orbit { yaw, pitch, zoom: 160.0, target: centre, fov: FOV };
        let vp = orbit.view_proj(SCREEN.x / SCREEN.y);
        let mut h = Handles::at(centre, world_size(vp, centre, 0.12));
        h.arrows = [Some(Vec3::X), Some(Vec3::Y), Some(Vec3::Z)];
        h.rings = [Some(Vec3::X), Some(Vec3::Y), Some(Vec3::Z)];
        (h, vp)
    }

    /// The ray through a pixel.
    fn ray(vp: Mat4, p: Vec2) -> (Vec3, Vec3) {
        mouse_ray(vp, p.x / SCREEN.x, p.y / SCREEN.y)
    }

    /// The handles are as big on the screen however far the camera is.
    #[test]
    fn handles_are_a_share_of_the_view() {
        for zoom in [40.0, 160.0, 900.0] {
            let centre = Vec3::new(5.0, 5.0, 5.0);
            let vp = Orbit { yaw: 20.0, pitch: 30.0, zoom, target: centre, fov: FOV }.view_proj(16.0 / 9.0);
            let up = world_size(vp, centre, 0.1);
            let (a, _) = project(vp, SCREEN, centre).unwrap();
            let (b, _) = project(vp, SCREEN, centre + Vec3::Z * up).unwrap();
            // (Seen from above at 30 degrees an upright line looks shorter.)
            assert!((a.distance(b) / SCREEN.y - 0.1 * 30f32.to_radians().cos()).abs() < 0.01, "{zoom}: {}", a.distance(b));
        }
    }

    /// From several sides the mouse on an arrow's head or shaft grabs that
    /// arrow, on a ring away from the arrows that ring, and nothing far
    /// away; the middle is left to the thing itself.
    #[test]
    fn the_mouse_grabs_the_arrow_or_ring_under_it() {
        for (yaw, pitch) in [(0.0, 30.0), (35.0, 20.0), (200.0, 45.0), (300.0, 10.0), (90.0, 60.0)] {
            let (h, vp) = seen(yaw, pitch);
            for axis in 0..3 {
                let shaft = project(vp, SCREEN, h.centre + h.arrows[axis].unwrap() * h.length * 0.9).unwrap().0;
                // An arrow pointing nearly at the eye is too short to tell.
                let middle = project(vp, SCREEN, h.centre).unwrap().0;
                if shaft.distance(middle) < 30.0 {
                    continue;
                }
                // Nor where another arrow lies over it on the screen.
                let over = (0..3).filter(|&b| b != axis).any(|b| {
                    let (a, z) = (project(vp, SCREEN, h.centre).unwrap().0, project(vp, SCREEN, h.centre + h.arrows[b].unwrap() * h.length).unwrap().0);
                    to_segment(shaft, a, z) < GRAB * 2.0
                });
                if over {
                    continue;
                }
                assert_eq!(h.grip_at(vp, SCREEN, shaft), Some(Grip::Arrow(axis)), "{yaw}, {pitch}: arrow {axis}");
                // A point of the ring between two arrows (where its plane
                // shows: not edge on).
                let (u, v) = plane_of(h.rings[axis].unwrap());
                let best = (0..36)
                    .map(|k| {
                        let a = k as f32 * 10f32.to_radians();
                        h.centre + (u * a.cos() + v * a.sin()) * h.length * RING
                    })
                    .filter_map(|p| project(vp, SCREEN, p).map(|(s, _)| s))
                    .find(|s| h.grip_at(vp, SCREEN, *s) == Some(Grip::Ring(axis)));
                assert!(best.is_some(), "{yaw}, {pitch}: ring {axis} grabbed somewhere");
            }
            let middle = project(vp, SCREEN, h.centre).unwrap().0;
            assert_eq!(h.grip_at(vp, SCREEN, middle), None, "{yaw}, {pitch}: the middle is the thing's");
            assert_eq!(h.grip_at(vp, SCREEN, middle + Vec2::new(400.0, 300.0)), None);
        }
    }

    /// Dragging an arrow measures how far along its axis the mouse went,
    /// from any side; a ring measures the degrees round, the way of the
    /// right hand about its axis; a face slides along its axis.
    #[test]
    fn dragging_measures_along_and_round() {
        for (yaw, pitch) in [(0.0, 30.0), (35.0, 20.0), (200.0, 45.0), (300.0, 70.0)] {
            let (h, vp) = seen(yaw, pitch);
            for axis in 0..3 {
                let d = h.arrows[axis].unwrap();
                let at = |k: f32| project(vp, SCREEN, h.centre + d * k).unwrap().0;
                if at(0.0).distance(at(h.length)) < 30.0 {
                    continue;
                }
                let press = at(h.length * 0.9);
                let mut drag = Drag::begin(&h, Grip::Arrow(axis), vp, SCREEN, press, ray(vp, press)).unwrap();
                let to = at(h.length * 0.9 + 7.0);
                let went = drag.amount(&h, vp, SCREEN, to, ray(vp, to));
                assert!((went - 7.0).abs() < 0.05, "{yaw}, {pitch}: arrow {axis} went {went}");
            }
            // Round the upright axis, seen from above: a quarter round
            // counterclockwise on the screen is +90 (the right hand's way
            // about +z, seen from its tip).
            let middle = project(vp, SCREEN, h.centre).unwrap().0;
            let r = 100.0;
            let press = middle + Vec2::new(r, 0.0);
            let mut drag = Drag::begin(&h, Grip::Ring(2), vp, SCREEN, press, ray(vp, press)).unwrap();
            let mut went = 0.0;
            for k in 1..=9 {
                let a = (k as f32 * 10.0).to_radians();
                let p = middle + Vec2::new(a.cos(), -a.sin()) * r;
                went = drag.amount(&h, vp, SCREEN, p, ray(vp, p));
            }
            assert!((went - 90.0).abs() < 0.5, "{yaw}, {pitch}: turned {went}");
            // On round past a half turn, twice.
            for k in 10..=72 {
                let a = (k as f32 * 10.0).to_radians();
                let p = middle + Vec2::new(a.cos(), -a.sin()) * r;
                went = drag.amount(&h, vp, SCREEN, p, ray(vp, p));
            }
            assert!((went - 720.0).abs() < 0.5, "{yaw}, {pitch}: twice round, {went}");
        }
        // A face of a box slides along its axis.
        let (mut h, vp) = seen(30.0, 30.0);
        h.faces = Some((Vec3::new(30.0, 20.0, 0.0), Vec3::new(50.0, 40.0, 20.0)));
        let face = project(vp, SCREEN, Vec3::new(50.0, 30.0, 10.0)).unwrap().0;
        h.arrows = [None; 3];
        h.rings = [None; 3];
        assert_eq!(h.grip_at(vp, SCREEN, face), Some(Grip::Face(0, true)));
        let mut drag = Drag::begin(&h, Grip::Face(0, true), vp, SCREEN, face, ray(vp, face)).unwrap();
        let to = project(vp, SCREEN, Vec3::new(54.0, 30.0, 10.0)).unwrap().0;
        assert!((drag.amount(&h, vp, SCREEN, to, ray(vp, to)) - 4.0).abs() < 0.05);
    }

    /// A ring steps in quarter turns on the grid, turns freely otherwise,
    /// the quarters within a few degrees taken; always 0 to 360.
    #[test]
    fn a_ring_steps_in_quarters_or_turns_freely() {
        assert_eq!(step(0.0, 50.0, true, 5.0), 90.0);
        assert_eq!(step(0.0, 40.0, true, 5.0), 0.0);
        assert_eq!(step(90.0, -200.0, true, 5.0), 270.0);
        assert_eq!(step(0.0, 40.4, false, 5.0), 40.0);
        assert_eq!(step(0.0, 87.0, false, 5.0), 90.0);
        assert_eq!(step(180.0, -183.0, false, 5.0), 0.0);
        assert_eq!(step(10.0, 400.0, false, 5.0), 50.0);
    }
}
