//! Models as they are drawn: quarter turns and free turns, figures of
//! parts posed and swung, a model's cubes and its draw from the bank.

use super::*;

/// The quarter turn that faces a model's forward (+y) along `look`, as a
/// rotation (clockwise seen from above, as `render::quarter` turns).
pub(super) fn look_turn(look: IVec2) -> Quat {
    Quat::from_rotation_z(-(look.x as f32).atan2(look.y as f32))
}

/// A free turn in a model's own directions (forward +y, right +x, up +z),
/// in degrees: `turn` round clockwise seen from above (90 faces +x the way
/// a quarter turn does), `pitch` the nose up, `roll` the right side down (banking right:
/// its top leans right). They are taken roll first, then pitch, then the
/// turn round: the rotation `Rz(-turn) · Rx(pitch) · Ry(roll)`, so the turn
/// round stays the bearing seen from above whatever the other two (a
/// level's piece and `frame:model` write it `rotation`, `pitch`, `roll`,
/// `grid::Thing`).
/// All 0 is no turn at all.
pub fn turning(turn: f32, pitch: f32, roll: f32) -> Quat {
    if turn == 0.0 && pitch == 0.0 && roll == 0.0 {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_z(-turn.to_radians()) * Quat::from_rotation_x(pitch.to_radians()) * Quat::from_rotation_y(roll.to_radians())
}

/// A kind's spin (`spin`, degrees a second round its upright axis) after
/// `ticks` of the game (between two ticks too): only the picture turns,
/// every thing of the kind alike.
pub fn spinning(spin: f32, ticks: f32) -> Quat {
    if spin == 0.0 { Quat::IDENTITY } else { turning((spin * ticks * crate::sim::DT).rem_euclid(360.0), 0.0, 0.0) }
}

/// The turn that shows a body's model facing exactly its way on the ground
/// while its quarter turn is its `look` (a figure drawn at its diagonal),
/// for a kind that turns freely (`free`, `rotates = "free"`): none when it
/// does not, or faces straight along `look`.
pub fn facing(free: bool, body: &Body) -> Quat {
    let (facing, look) = (body.facing, body.look);
    if !free {
        return Quat::IDENTITY;
    }
    let way = facing.x.atan2(facing.y) - (look.x as f32).atan2(look.y as f32);
    let way = (way + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    if facing.truncate() == Vec2::ZERO || way.abs() < 1e-4 { Quat::IDENTITY } else { Quat::from_rotation_z(-way) }
}

/// A figure's parts turned as they swing with its steps: each part named
/// in `swing` pitched forward and back by its degrees (forward first; less
/// than 0: back first), a stride every half its height (`tall`) walked;
/// standing, as it stands.
pub fn swinging<'a>(swing: &'a BTreeMap<String, f32>, body: &Body, tall: f32) -> impl Fn(&str) -> PartPose + 'a {
    let phase = crate::sim::math::sin(std::f32::consts::PI * body.walked / (tall * 0.5).max(1.0));
    move |part| swing.get(part).map_or(PartPose::REST, |&degrees| PartPose::turned(turning(0.0, degrees * phase, 0.0)))
}

/// How a figure's part is posed: turned about its joint (on its rest
/// rotation), and moved by less than a voxel (`offset`, on its own), with
/// what hangs on it. Only the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartPose {
    pub turn: Quat,
    pub offset: Vec3,
}

impl PartPose {
    pub const REST: Self = Self { turn: Quat::IDENTITY, offset: Vec3::ZERO };

    pub fn turned(turn: Quat) -> Self {
        Self { turn, ..Self::REST }
    }
}

/// A script's pose entry: a part by its name, degrees round, nose up,
/// rolled (`turning`), and its offset in voxels.
pub type PoseEntry = (String, [f32; 3], [f32; 3]);

/// A script's pose (each part by its name: degrees round, nose up, rolled,
/// `turning`, an offset) over `under` (the kind's swing).
pub fn posing<'a>(pose: &'a [PoseEntry], under: impl Fn(&str) -> PartPose + 'a) -> impl Fn(&str) -> PartPose + 'a {
    move |part| {
        let under = under(part);
        match pose.iter().find(|(p, ..)| p == part) {
            Some((_, [round, pitch, roll], offset)) => PartPose { turn: turning(*round, *pitch, *roll) * under.turn, offset: Vec3::from(*offset) + under.offset },
            None => under,
        }
    }
}

/// A script's pose alone (a level's piece, `thing.state.parts`), no swing
/// under it.
pub fn posing_alone(pose: &[PoseEntry]) -> impl Fn(&str) -> PartPose + '_ {
    posing(pose, |_| PartPose::REST)
}

/// What `draw` hands over for each voxel it draws as a cube: its corner,
/// colour, metal, glow, the way it is lit and its free turn.
pub(super) type Put<'a> = dyn FnMut(Vec3, [u8; 4], u8, u8, [i8; 4], [i8; 4], u8) + 'a;

/// What `draw` puts down at once: its voxels in the bank (if there), the
/// see-through ones and all the visible ones (indices into the frame), its
/// turn and its shift (`render::turned_centre`).
pub(super) type Piece<'a> = (Option<std::ops::Range<u32>>, &'a [u32], &'a [u32], &'a [u8], Quat, Vec3, f32);

/// Draws one frame of a model with its feet at `origin`, turned in quarter
/// turns (model forward is +y) around a whole-voxel pivot, and freely by
/// `free_turn` in its own directions (identity: on the grid) about
/// `render::turn_centre` (that pivot's middle, the voxel at half its
/// height); a figure of parts each part posed by `pose`
/// about its joint (in its own directions, carrying what hangs on it;
/// empty: as it stands). Its opaque voxels come from the model bank on the
/// GPU, one entry (a part) in `on_gpu`; the see-through ones are cubes
/// (sorted with the frame's other see-through ones), and all of them
/// without `on_gpu` or a bank (a model fading out).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw(put: &mut Put, on_gpu: Option<&mut Vec<ModelDraw>>, model: &VoxModel, frame: usize, origin: Vec3, look: IVec2, free_turn: Quat, pose: &[PartPose], tint: Option<[u8; 4]>) {
    draw_placed(put, on_gpu, model, frame, origin, look, free_turn, pose, tint, false);
}

/// `draw`; `as_drawn` places the cubes of what is turned freely by the
/// turn the shader turns them by (`render::turn_as_drawn`), from the
/// voxel it turns about, so that neighbours meet corner to corner: a model
/// drawn all as cubes (the editors' pieces). Placed by the exact turn, as
/// the bank's voxels are, each cube was turned by the packed turn about a
/// middle the exact one put it at, and neighbours were apart or overlapped
/// by a hundredth of a voxel: single pixels of what lies behind showed on
/// its faces. Without it the cubes lie where the bank's voxels lie (the
/// glass beside them).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_placed(
    put: &mut Put,
    mut on_gpu: Option<&mut Vec<ModelDraw>>,
    model: &VoxModel,
    frame: usize,
    origin: Vec3,
    look: IVec2,
    free_turn: Quat,
    pose: &[PartPose],
    tint: Option<[u8; 4]>,
    as_drawn: bool,
) {
    // The free turn as the world sees it, after the quarter turns.
    let quarter_turn = look_turn(look);
    let rot = if free_turn == Quat::IDENTITY { free_turn } else { quarter_turn * free_turn * quarter_turn.inverse() };
    let pivot = IVec3::new(model.size.x / 2, model.size.y / 2, 0);
    let frame = if model.parts.is_empty() { frame.min(model.frames.len() - 1) } else { 0 };
    let metal = model.metal.get(frame);
    let shade = model.shade.get(frame);
    let facets = model.facets.get(frame);
    let glow = model.glow.get(frame);
    let turn = match (look.x, look.y) {
        (0, 1) => 0,
        (1, 0) => 1,
        (0, -1) => 2,
        _ => 3,
    };
    let about = crate::render::turn_centre(model.size);
    // What is drawn: the frame whole, or each part with its own turn and
    // shift (its chain of turns about the joints, in the model's own
    // directions, seen after the quarter turns and the free turn).
    let mut pieces: Vec<Piece> = Vec::new();
    if model.parts.is_empty() {
        pieces.push((model.bank.get(frame).cloned(), &model.glass[frame], &model.visible[frame], model.covered.get(frame).map_or(&[][..], |c| &c[..]), rot, crate::render::turn_about(rot, about), 0.0));
    } else {
        let mut chain: Vec<Option<(Quat, Vec3)>> = vec![None; model.parts.len()];
        let centre = Vec3::new(0.5, 0.5, 0.0);
        // Posed (or a part turned at rest), a part's depth is taken nearer
        // by how far down the figure it hangs (`ModelDraw::nearer`); at
        // rest nothing overlaps.
        let posed = pose.iter().any(|q| *q != PartPose::REST) || rests_turned(model);
        let steps = |mut i: usize| {
            let mut n = 0.0;
            while let Some(p) = model.parts[i].parent.filter(|_| n < 16.0) {
                (i, n) = (p, n + 1.0);
            }
            n
        };
        for i in 0..model.parts.len() {
            let (own, b) = part_chain(model, pose, i, &mut chain);
            let seen = quarter_turn * own * quarter_turn.inverse();
            let shift = about + rot * (-(seen * centre) + quarter_turn * (own * centre) + quarter_turn * (b - centre) + centre - about);
            let p = &model.parts[i];
            let bank = (!model.bank.is_empty()).then(|| p.bank.clone());
            pieces.push((bank, &p.glass, &p.visible, &p.covered, rot * seen, shift, if posed { steps(i) } else { 0.0 }));
        }
    }
    // What each set of lit faces (bits in mesh::DIRS order) becomes after
    // the turn, once for all the voxels.
    let dirs = crate::mesh::DIRS;
    let to: [usize; 6] = std::array::from_fn(|f| {
        let (x, y) = crate::render::quarter(dirs[f].x, dirs[f].y, turn);
        dirs.iter().position(|e| *e == IVec3::new(x, y, dirs[f].z)).unwrap_or(f)
    });
    let turned_faces: [i8; 64] = std::array::from_fn(|mask| (0..6).filter(|f| mask >> f & 1 == 1).fold(0, |bits, f| bits | 1 << to[f]));
    let voxels = &model.frames[frame];
    for (bank, glass, visible, covered, rot, shift, nearer) in pieces {
        // As cubes the visible voxels carry the faces their own neighbours
        // cover, as the bank's do (`vox::covered`); glass carries none.
        let (as_cubes, covered) = match (on_gpu.as_deref_mut(), bank) {
            (Some(on_gpu), Some(voxels)) => {
                on_gpu.push(ModelDraw { voxels, origin, turn, rot, shift, tint: tint.map(|[r, g, b, _]| [r, g, b]), nearer });
                (glass, &[][..])
            }
            _ => (visible, covered),
        };
        let free = crate::render::off_grid(rot, shift);
        let snorm = crate::render::snorm_turn(rot);
        // As drawn: from the voxel it turns about, at its exact place, by the
        // packed turn (`draw_placed`).
        let middle = about - 0.5;
        let placed = (as_drawn && snorm != [0; 4]).then(|| (crate::render::turned_centre(origin, middle, shift, rot), crate::render::turn_as_drawn(rot)));
        for (i, &k) in as_cubes.iter().enumerate() {
            // in the world's directions after the quarter turns, as a cube's faces are counted
            let hidden = covered.get(i).map_or(0, |&c| turned_faces[(c & 63) as usize] as u8);
            let k = k as usize;
            let (v, color) = &voxels[k];
            let l = *v - pivot;
            let (x, y) = crate::render::quarter(l.x, l.y, turn);
            let mut lit = shade.and_then(|s| s.get(k)).map_or([0; 4], |&[nx, ny, nz, faces]| {
                let (nx, ny) = crate::render::quarter(nx as i32, ny as i32, turn);
                [nx as i8, ny as i8, nz, turned_faces[(faces & 63) as usize]]
            });
            // Smooth glass mirrors and glints by its facet's way (lit by its
            // faces' own, as glass is).
            if let Some(&[nx, ny, nz]) = facets.and_then(|f| f.get(k)).filter(|f| **f != [0; 3]) {
                let (nx, ny) = crate::render::quarter(nx as i32, ny as i32, turn);
                lit = [nx as i8, ny as i8, nz, 0];
            }
            // A flash whitens, glass stays glass.
            let [r, g, b, _] = tint.unwrap_or(*color);
            let local = IVec3::new(x, y, l.z).as_vec3();
            let corner = if let Some((at, drawn)) = placed {
                at + drawn * (local - middle) - 0.5
            } else if free {
                crate::render::turned_centre(origin, local, shift, rot) - 0.5
            } else {
                origin + IVec3::new(x, y, l.z).as_vec3()
            };
            put(corner, [r, g, b, color[3]], metal.and_then(|m| m.get(k)).copied().unwrap_or(0), glow.and_then(|g| g.get(k)).copied().unwrap_or(0), lit, snorm, hidden);
        }
    }
}

/// The cells a model's first frame fills (every voxel, glass too: a
/// figure's parts as they stand), facing `look` in quarter turns, from its
/// pivot's cell: exactly where `draw` puts its voxels on the grid. What a
/// solid piece holds up (`world::Solids`).
pub fn quarter_cells(model: &VoxModel, look: IVec2) -> Vec<IVec3> {
    let bearing = match (look.x, look.y) {
        (0, 1) => 0,
        (1, 0) => 90,
        (0, -1) => 180,
        _ => 270,
    };
    piece_cells(model, [bearing, 0, 0]).expect("quarter turns")
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

/// The cells a model's first frame fills (every voxel, glass too: a
/// figure's parts as they stand), turned by `[turn, pitch, roll]` in
/// whole quarter turns (`turning`'s words and order; turn a bearing,
/// clockwise seen from above, 0 the model's own forward), from its pivot's
/// cell: exactly where `draw` puts its voxels, which lie on the grid as
/// they turn about a voxel's middle (`render::turn_centre`). What a solid
/// piece holds up (`world::Solids`). None if a turn is off the quarters.
pub fn piece_cells(model: &VoxModel, turns: [i32; 3]) -> Option<Vec<IVec3>> {
    let m = quarter_matrix(turns)?;
    let pivot = IVec3::new(model.size.x / 2, model.size.y / 2, 0);
    let h = model.size.z / 2;
    let mut cells: Vec<IVec3> = model.frames.first().map_or(Vec::new(), |f| {
        f.iter()
            .map(|(v, _)| {
                // The voxel's middle from the turn's centre is whole: x and
                // y from the pivot cell's middle, z from the voxel at half
                // the height.
                let l = *v - pivot;
                let d = [l.x, l.y, l.z - h];
                let r: [i32; 3] = std::array::from_fn(|i| (0..3).map(|k| m[i][k] * d[k]).sum());
                IVec3::new(r[0], r[1], r[2] + h)
            })
            .collect()
    });
    cells.sort_unstable_by_key(|c| (c.z, c.y, c.x));
    cells.dedup();
    Some(cells)
}

/// A model's frame as cubes, every voxel that shows, as a game draws it
/// with its feet at `origin` facing `look` and turned `turn` freely after
/// (`Drawing::model_turned`): the opaque ones into `opaque`, the glass into
/// `glass`. The level editor's pieces, so they look as in the game; what
/// is turned freely placed as drawn (`draw_placed`), neighbours meeting.
pub fn model_cubes(model: &VoxModel, frame: usize, origin: Vec3, look: IVec2, turn: Quat, opaque: &mut Vec<Instance>, glass: &mut Vec<Instance>) {
    let mut put = |corner: Vec3, color: [u8; 4], metal: u8, glow: u8, shade: [i8; 4], turn: [i8; 4], covered: u8| {
        let covered = if color[3] < 255 { 0 } else { covered };
        let instance = Instance { pos_size: (corner + 0.5).extend(1.0).to_array(), color, fx: [glow, color[3], covered, metal], shade, turn };
        if color[3] < 255 { glass.push(instance) } else { opaque.push(instance) }
    };
    draw_placed(&mut put, None, model, frame, origin, look, turn, &[], None, true);
}

/// A model as a game draws it alone (the editors' GAME VIEW): with its
/// feet at `origin` facing `look`, a figure posed by `pose` (a turn per
/// part, on its rest), its opaque voxels from the model bank its
/// `VoxModel::bank` was filled into (`draws`: outlined and shaded as the
/// game's), its glass as cubes.
pub fn model_drawn(model: &VoxModel, origin: Vec3, look: IVec2, pose: &[PartPose], draws: &mut Vec<ModelDraw>, glass: &mut Vec<Instance>) {
    let mut put = |corner: Vec3, color: [u8; 4], metal: u8, glow: u8, shade: [i8; 4], turn: [i8; 4], covered: u8| {
        let covered = if color[3] < 255 { 0 } else { covered };
        glass.push(Instance { pos_size: (corner + 0.5).extend(1.0).to_array(), color, fx: [glow, color[3], covered, metal], shade, turn });
    };
    draw(&mut put, Some(draws), model, 0, origin, look, Quat::IDENTITY, pose, None);
}

/// Where part `i` of a figure stands posed by `pose` (a turn per part, on
/// its rest rotation): its turn and shift, as `part_at_rest` gives them.
pub fn part_posed(model: &VoxModel, pose: &[PartPose], i: usize) -> (Quat, Vec3) {
    let mut chain = vec![None; model.parts.len()];
    part_chain(model, pose, i, &mut chain)
}

/// Where part `i` of a figure stands, posed: the turn and the shift that
/// take a voxel's middle (from the pivot cell's minimum corner, in the
/// model's own directions) to where it is, each part turned about its
/// joint and carried by the part it hangs on.
pub(super) fn part_chain(model: &VoxModel, pose: &[PartPose], i: usize, chain: &mut [Option<(Quat, Vec3)>]) -> (Quat, Vec3) {
    if let Some(done) = chain[i] {
        return done;
    }
    let part = &model.parts[i];
    // The pose turns on from the part's rest rotation, about the same
    // joint: `pose · rest` (the rest first).
    let posed = pose.get(i).copied().unwrap_or(PartPose::REST);
    let own = match rest_turn(part) {
        Some(rest) => posed.turn * rest,
        None => posed.turn,
    };
    // Turned about its joint, then moved by its offset (the file's and
    // the pose's), in its parent's directions: the joint goes along, and
    // what hangs on it.
    let offset = part.offset + posed.offset;
    let joint = part.joint;
    let done = match part.parent {
        None => (own, joint - own * joint + offset),
        Some(p) => {
            let (turn, shift) = part_chain(model, pose, p, chain);
            let turned = turn * own;
            (turned, turn * (joint + offset) + shift - turned * joint)
        }
    };
    chain[i] = Some(done);
    done
}

/// A part's rest rotation (`vox::Part::rest`) as a turn in the model's own
/// directions about its joint (`turning`), none where it has none.
pub fn rest_turn(part: &crate::vox::Part) -> Option<Quat> {
    let [round, pitch, roll] = part.rest;
    (part.rest != [0.0; 3]).then(|| turning(round, pitch, roll))
}

/// Where part `i` of a figure stands at rest (no pose): its turn and its
/// shift in the model's own directions, its parents' rest rotations and
/// its own about their joints: a point `p` of it, measured from the
/// pivot's low corner as `Part::joint` is (a voxel's middle half a voxel
/// past its corner), lies at `turn * p + shift`. What `draw` turns a
/// figure's part by, for the editors' own drawing of a figure.
pub fn part_at_rest(model: &VoxModel, i: usize) -> (Quat, Vec3) {
    let mut chain = vec![None; model.parts.len()];
    part_chain(model, &[], i, &mut chain)
}

/// Whether a figure has a part turned at rest (`vox::Part::rest`).
pub fn rests_turned(model: &VoxModel) -> bool {
    model.parts.iter().any(|p| p.rest != [0.0; 3] || p.offset != Vec3::ZERO)
}
