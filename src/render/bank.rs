//! The model bank: every model frame's opaque voxels on the GPU once, and
//! where a draw of one puts them (`ModelDraw`, the shader's `vs_model`).

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Quat, Vec3};

use super::Instance;
#[cfg(test)]
use super::snorm_turn;
#[cfg(test)]
use crate::mesh;

/// The opaque voxels of every model frame a game draws, put on the GPU once
/// (the renderer uploads a bank the first time a frame brings it). A model
/// is then drawn with one entry, `ModelDraw`, instead of a cube per voxel:
/// at 16-bit scale building those cubes was most of the CPU's frame.
#[derive(Default)]
pub struct ModelBank {
    /// Relative to the model's pivot (`pos_size` xyz, before the turn).
    pub voxels: Vec<Instance>,
}

/// One model frame drawn from the bank.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelDraw {
    /// Its voxels in the bank.
    pub voxels: Range<u32>,
    /// Where the pivot's cell has its minimum corner.
    pub origin: Vec3,
    /// Quarter turns: 0 facing the model's forward (+y), 1 facing +x, 2 -y,
    /// 3 -x (`quarter`).
    pub turn: u32,
    /// Turned freely after the quarter turns (identity: not at all) and
    /// shifted (`turned_centre`): a whole model about the pivot cell's
    /// middle at half its height, a figure's part about its joint too.
    pub rot: Quat,
    pub shift: Vec3,
    /// Every voxel this colour (a flash).
    pub tint: Option<[u8; 3]>,
    /// How many steps nearer the eye its depth is taken (`PART_NEARER` of
    /// a voxel each, the picture's place unchanged): a posed figure's part
    /// by how far down the figure it hangs, so where a bent limb's faces
    /// lie in one plane with its parent's the part further out wins,
    /// instead of both flickering through each other. 0: as it lies.
    pub nearer: f32,
    /// Only in the sun's shadow map: casting, never drawn in the scene,
    /// outlined or glowing (a level's `shadow_only` piece: a ceiling the
    /// camera looks through).
    pub shadow_only: bool,
}

impl ModelDraw {
    /// Whether it is drawn in a pass: into the sun's shadow map
    /// (`shadows`) every one with voxels, into the scene, its outline and
    /// the UI's those not only casting.
    pub fn in_pass(&self, shadows: bool) -> bool {
        !self.voxels.is_empty() && (shadows || !self.shadow_only)
    }
}

/// Where a voxel of a freely turned model has its centre: `local` its
/// minimum corner from the pivot cell's, after the quarter turns; its
/// centre turned by `rot` and then shifted by `shift` (`turn_about`,
/// `ModelDraw`), as the shader's `model_centre` does it.
pub fn turned_centre(origin: Vec3, local: Vec3, shift: Vec3, rot: Quat) -> Vec3 {
    origin + shift + rot * (local + 0.5)
}

/// The shift that turns a model by `rot` about the point `about` (from the
/// pivot cell's minimum corner): a whole model's, `turn_centre`.
pub fn turn_about(rot: Quat, about: Vec3) -> Vec3 {
    about - rot * about
}

/// Where a whole model of `size` turns freely (from the pivot cell's
/// minimum corner): the middle of the voxel over the pivot cell at half the
/// model's height, rounded down (a model 20 tall: the middle of its 11th
/// voxel, 10.5 up; 21 tall: 10.5 too). A voxel's middle, not the line
/// between two, so that a quarter turn about any axis (a cupboard laid on
/// its side, stood on its head) puts every voxel on the grid again: what a
/// solid piece needs (`session::piece_cells`). Turns round the upright
/// axis do not depend on the height.
pub fn turn_centre(size: IVec3) -> Vec3 {
    Vec3::new(0.5, 0.5, (size.z / 2) as f32 + 0.5)
}

/// Whether a draw leaves the grid: turned, or shifted (a figure's part).
pub fn off_grid(rot: Quat, shift: Vec3) -> bool {
    rot != Quat::IDENTITY || shift != Vec3::ZERO
}

/// A position or direction turned by `turn` quarter turns (`ModelDraw`), as
/// the shader's `quarter` turns it.
pub fn quarter(x: i32, y: i32, turn: u32) -> (i32, i32) {
    match turn % 4 {
        0 => (x, y),
        1 => (y, -x),
        2 => (-x, -y),
        _ => (-y, x),
    }
}

impl ModelBank {
    /// Puts a fixed set of cubes in the bank (the title logo's letters),
    /// relative to the origin a draw gives them; drawn unturned (the faces
    /// a neighbour covers, `hide_inner_faces`, stay as they are).
    pub fn add_cubes(&mut self, cubes: &[Instance]) -> Range<u32> {
        let start = self.voxels.len() as u32;
        self.voxels.extend(cubes.iter().map(|c| {
            let [x, y, z, size] = c.pos_size;
            Instance { pos_size: [x - 0.5, y - 0.5, z - 0.5, size], ..*c }
        }));
        start..self.voxels.len() as u32
    }

    /// Puts the opaque visible voxels of each of `model`'s frames in the
    /// bank and notes where (`VoxModel::bank`).
    pub fn add(&mut self, model: &mut crate::vox::VoxModel) {
        model.bank = (0..model.frames.len()).map(|f| self.add_voxels(model, f, &model.visible[f], &model.covered[f])).collect();
        // A figure's parts: each its own run of its one frame, its joints'
        // faces too.
        let banked: Vec<Range<u32>> = model.parts.iter().map(|part| self.add_voxels(model, 0, &part.visible, &part.covered)).collect();
        for (part, bank) in model.parts.iter_mut().zip(banked) {
            part.bank = bank;
        }
    }

    /// The opaque ones of `visible` (voxels of frame `f`, each with its
    /// `covered` faces) relative to the pivot: where they lie in the bank.
    fn add_voxels(&mut self, model: &crate::vox::VoxModel, f: usize, visible: &[u32], covered: &[u8]) -> Range<u32> {
        let pivot = IVec3::new(model.size.x / 2, model.size.y / 2, 0);
        let start = self.voxels.len() as u32;
        for (i, &k) in visible.iter().enumerate() {
            let k = k as usize;
            let (v, color) = model.frames[f][k];
            if color[3] < 255 {
                continue;
            }
            let l = (v - pivot).as_vec3();
            let fx = [model.glow_at(f, k), 255, covered.get(i).copied().unwrap_or(0), model.metal_at(f, k)];
            self.voxels.push(Instance { pos_size: [l.x, l.y, l.z, 1.0], color, fx, shade: model.shade_at(f, k), turn: [0; 4] });
        }
        start..self.voxels.len() as u32
    }

    /// The cubes a draw puts in the world, worked out as the GPU does
    /// (`vs_model`): for tests.
    #[cfg(test)]
    pub fn cubes<'a>(&'a self, draw: &'a ModelDraw) -> impl Iterator<Item = Instance> + 'a {
        let turn = draw.turn;
        let free = off_grid(draw.rot, draw.shift);
        // The faces lit by the shade direction, counted in the world's
        // directions after the turn.
        let dirs = mesh::DIRS;
        let to: [usize; 6] = std::array::from_fn(|f| {
            let (x, y) = quarter(dirs[f].x, dirs[f].y, turn);
            dirs.iter().position(|e| *e == IVec3::new(x, y, dirs[f].z)).unwrap_or(f)
        });
        self.voxels[draw.voxels.start as usize..draw.voxels.end as usize].iter().map(move |v| {
            let [lx, ly, lz, size] = v.pos_size;
            let (x, y) = quarter(lx as i32, ly as i32, turn);
            let local = Vec3::new(x as f32, y as f32, lz);
            let centre = if free { turned_centre(draw.origin, local, draw.shift, draw.rot) } else { draw.origin + local + 0.5 };
            let [nx, ny, nz, faces] = v.shade;
            let (nx, ny) = quarter(nx as i32, ny as i32, turn);
            let lit = (0..6).filter(|f| faces >> f & 1 == 1).fold(0i8, |bits, f| bits | 1 << to[f]);
            let color = draw.tint.map_or(v.color, |[r, g, b]| [r, g, b, v.color[3]]);
            let shade = [nx as i8, ny as i8, nz, lit];
            // The covered faces too, as a cube carries them (`vs_inst`).
            let [glow, opacity, covered, metal] = v.fx;
            let covered = (0..6).filter(|f| covered >> f & 1 == 1).fold(0u8, |bits, f| bits | 1 << to[f]);
            Instance { pos_size: centre.extend(size).to_array(), color, fx: [glow, opacity, covered, metal], shade, turn: snorm_turn(draw.rot) }
        })
    }
}

/// A model draw's place on the GPU: a uniform at its own offset.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct ActorData {
    /// xyz: the origin, w: quarter turns.
    pub(super) origin: [f32; 4],
    /// rgb: the flash colour, w: 1 while it flashes.
    pub(super) tint: [f32; 4],
    /// x: how many steps nearer the eye its depth is taken (`ModelDraw::nearer`).
    pub(super) extra: [f32; 4],
    /// The free turn (xyzw; 0 0 0 1 none), and the shift after it (xyz;
    /// w 1 when it is off the grid at all).
    pub(super) rot: [f32; 4],
    pub(super) pivot: [f32; 4],
}

/// Model draws' uniforms lie this far apart (the most any GPU asks).
pub(super) const ACTOR_STRIDE: usize = 256;
