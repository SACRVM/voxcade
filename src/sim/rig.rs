//! Where a figure's part stands as the tick reckons it (`thing:part`): a
//! walk of its own from the root down to the part, in plain f32 sums in a
//! fixed order and `sim::math`'s sine, so every machine gets the same
//! bits; a turn of whole quarter turns goes through whole numbers
//! (`quarter_matrix`), exactly. The picture walks its own way
//! (`session::part_chain`, glam's quaternions); test
//! `the_ticks_part_is_where_the_picture_draws_it` holds them together.

use glam::{IVec2, IVec3, Vec3};

/// A turn as a matrix, by rows.
type Mat = [[f32; 3]; 3];

const ONE: Mat = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// What the tick reads of a figure of parts (a level's piece's model, read
/// as the game loads, so in the fingerprint): each part's place in the
/// rig and its own box.
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub parts: Vec<RigPart>,
    /// The height of the point the whole model turns about, over its feet
    /// (`render::turn_centre`).
    pub centre: f32,
}

/// A part as the rig holds it, in the model's own directions from the
/// pivot cell's low corner (as `vox::Part::joint`).
#[derive(Clone, Debug, PartialEq)]
pub struct RigPart {
    pub name: String,
    pub parent: Option<usize>,
    pub joint: Vec3,
    /// Its rest rotation, degrees (`rotation`, `pitch`, `roll`).
    pub rest: [f32; 3],
    pub offset: Vec3,
    /// Its own box, low and high corners; None for an empty part.
    pub own: Option<(Vec3, Vec3)>,
}

/// A part's pose as its thing's state gives it: degrees round, nose up,
/// rolled, and its offset in voxels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Posed {
    pub turn: [f32; 3],
    pub offset: Vec3,
}

/// Where a part is: the middle of its own box, the box round that box's
/// corners along the level's axes, and its joint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub middle: Vec3,
    pub low: Vec3,
    pub high: Vec3,
    pub joint: Vec3,
}

impl Rig {
    /// The part called `name`, by its place among the parts.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.parts.iter().position(|p| p.name == name)
    }

    /// The parts from the root down to part `i`, the root first.
    pub fn chain(&self, i: usize) -> Vec<usize> {
        let mut chain = vec![i];
        while let Some(p) = self.parts[*chain.last().expect("a part")].parent.filter(|_| chain.len() <= self.parts.len()) {
            chain.push(p);
        }
        chain.reverse();
        chain
    }

    /// Where part `i` is with the model's feet at the cell `at` (its pivot's
    /// cell), the model turned `way` (`way`), each part of its chain posed
    /// by `posed` (by its place) on its rest rotation: as the picture
    /// places it (`session::draw`), the voxels' middles `turn_centre` turns
    /// about.
    pub fn place(&self, i: usize, at: IVec3, way: [f32; 3], posed: impl Fn(usize) -> Posed) -> Placed {
        // Each part of the chain turned about its joint, carried by its
        // parent: a point p of the part lies at t·p + s in the model.
        let (mut t, mut s) = (ONE, Vec3::ZERO);
        for k in self.chain(i) {
            let part = &self.parts[k];
            let pose = posed(k);
            let own = mul(rotation(pose.turn), rotation(part.rest));
            let (joint, offset) = (part.joint, part.offset + pose.offset);
            (t, s) = match part.parent {
                None => (own, joint - apply(own, joint) + offset),
                Some(_) => {
                    let turned = mul(t, own);
                    (turned, apply(t, joint + offset) + s - apply(turned, joint))
                }
            };
        }
        // The model turned about the middle of the voxel over its pivot
        // cell at half its height (`render::turn_centre`), its feet there.
        let about = Vec3::new(0.5, 0.5, self.centre);
        let m = rotation(way);
        let (w, u) = (mul(m, t), at.as_vec3() + about + apply(m, s - about));
        let place = |p: Vec3| apply(w, p) + u;
        let joint = place(self.parts[i].joint);
        let Some((low, high)) = self.parts[i].own else { return Placed { middle: joint, low: joint, high: joint, joint } };
        let corners = (0..8).map(|c| place(Vec3::new(if c & 1 == 0 { low.x } else { high.x }, if c & 2 == 0 { low.y } else { high.y }, if c & 4 == 0 { low.z } else { high.z })));
        let (lo, hi) = corners.fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(lo, hi), c| (lo.min(c), hi.max(c)));
        Placed { middle: place((low + high) * 0.5), low: lo, high: hi, joint }
    }
}

/// The way a piece's model faces as the picture turns it (`session::facing`
/// on a body facing `facing` with its quarter turn `look`, tipped by
/// `pitch` and `roll`): `[rotation, pitch, roll]` in degrees, its bearing
/// the quarter turn exactly unless the body faces off it.
pub fn way(look: IVec2, facing: Vec3, pitch: f32, roll: f32) -> [f32; 3] {
    let quarter: f32 = match (look.x, look.y) {
        (0, 1) => 0.0,
        (1, 0) => 90.0,
        (0, -1) => 180.0,
        _ => 270.0,
    };
    let (pi, tau) = (std::f32::consts::PI, std::f32::consts::TAU);
    let bearing = super::math::atan2(facing.x, facing.y);
    let off = (bearing - quarter.to_radians() + pi).rem_euclid(tau) - pi;
    let free = facing.truncate() != glam::Vec2::ZERO && off.abs() >= 1e-4;
    [if free { bearing.to_degrees() } else { quarter }, pitch, roll]
}

/// `session::turning`'s rotation as a matrix: `Rz(-rotation) · Rx(pitch) ·
/// Ry(roll)`, in degrees; whole quarter turns as whole numbers, exactly
/// (the sine of 90 degrees as floats is no exact 1).
pub fn rotation(turn: [f32; 3]) -> Mat {
    let quarters = turn.map(|d| (d.is_finite() && d % 90.0 == 0.0 && d.abs() <= 1e6).then_some(d as i32));
    if let [Some(a), Some(b), Some(c)] = quarters {
        return quarter_matrix([a, b, c]).expect("quarter turns").map(|row| row.map(|v| v as f32));
    }
    let [round, pitch, roll] = turn;
    let (zs, zc) = super::math::sin_cos((-round).to_radians());
    let (xs, xc) = super::math::sin_cos(pitch.to_radians());
    let (ys, yc) = super::math::sin_cos(roll.to_radians());
    let z = [[zc, -zs, 0.0], [zs, zc, 0.0], [0.0, 0.0, 1.0]];
    let x = [[1.0, 0.0, 0.0], [0.0, xc, -xs], [0.0, xs, xc]];
    let y = [[yc, 0.0, ys], [0.0, 1.0, 0.0], [-ys, 0.0, yc]];
    mul(mul(z, x), y)
}

/// A turn of whole quarter turns as a matrix of whole numbers: `turning`'s
/// `Rz(-turn) · Rx(pitch) · Ry(roll)` for `[turn, pitch, roll]` in degrees,
/// each a multiple of 90 (None if one is not).
pub fn quarter_matrix([turn, pitch, roll]: [i32; 3]) -> Option<[[i32; 3]; 3]> {
    if [turn, pitch, roll].iter().any(|d| d.rem_euclid(90) != 0) {
        return None;
    }
    // The cosine and sine of a quarter turn by its count.
    let cs = |d: i32| [(1, 0), (0, 1), (-1, 0), (0, -1)][(d.rem_euclid(360) / 90) as usize];
    let mul = |a: [[i32; 3]; 3], b: [[i32; 3]; 3]| std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()));
    let (zc, zs) = cs(-turn);
    let (xc, xs) = cs(pitch);
    let (yc, ys) = cs(roll);
    let z = [[zc, -zs, 0], [zs, zc, 0], [0, 0, 1]];
    let x = [[1, 0, 0], [0, xc, -xs], [0, xs, xc]];
    let y = [[yc, 0, ys], [0, 1, 0], [-ys, 0, yc]];
    Some(mul(mul(z, x), y))
}

/// Two turns one after the other (`b` first), each sum in a fixed order.
fn mul(a: Mat, b: Mat) -> Mat {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j]))
}

/// A point or way turned.
fn apply(m: Mat, v: Vec3) -> Vec3 {
    let row = |r: [f32; 3]| r[0] * v.x + r[1] * v.y + r[2] * v.z;
    Vec3::new(row(m[0]), row(m[1]), row(m[2]))
}
