//! The fixed camera: perspective, one angle per game, looking at the player.
//! And the sun, which is just as fixed (and orthographic, since it is far away).

use glam::camera::rh::proj::directx::{orthographic, perspective};
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

use crate::config::CameraConfig;

/// Widest screen the face set has to cover (21:9). Wider windows would
/// need the faces of objects at the far left and right edges re-checked
/// (a picture of its own shape, `c.camera.aspect`, wider: its width).
const MAX_ASPECT: f32 = 2.4;

pub fn view_dir(cfg: &CameraConfig) -> Vec3 {
    let (pitch, yaw) = (cfg.pitch.to_radians(), cfg.yaw.to_radians());
    Vec3::new(yaw.sin() * pitch.cos(), yaw.cos() * pitch.cos(), -pitch.sin()).normalize()
}

fn fov(cfg: &CameraConfig) -> f32 {
    cfg.fov.clamp(10.0, 100.0).to_radians()
}

fn right_up(cfg: &CameraConfig) -> (Vec3, Vec3) {
    let yaw = cfg.yaw.to_radians();
    let right = Vec3::new(yaw.cos(), -yaw.sin(), 0.0);
    (right, right.cross(view_dir(cfg)))
}

/// How many voxels the screen spans across in the plane through the
/// target: `view_width`, or `view_height` times the window's shape, or
/// enough for `view_at_least` both ways.
pub fn shown_width(cfg: &CameraConfig, aspect: f32) -> f32 {
    if let Some([width, height]) = cfg.view_at_least {
        return width.max(height * aspect);
    }
    cfg.view_height.map_or(cfg.view_width, |height| height * aspect)
}

/// Distance from the eye to the target at which `shown_width` voxels span
/// the screen in the plane through the target.
fn eye_distance(cfg: &CameraConfig, aspect: f32) -> f32 {
    shown_width(cfg, aspect) * 0.5 / aspect / (fov(cfg) * 0.5).tan()
}

/// Where the camera is when it looks at `target`.
pub fn eye(cfg: &CameraConfig, target: Vec3, aspect: f32) -> Vec3 {
    target - view_dir(cfg) * eye_distance(cfg, aspect)
}

pub fn view_proj(cfg: &CameraConfig, target: Vec3, aspect: f32) -> Mat4 {
    let dir = view_dir(cfg);
    let up = if dir.z.abs() > 0.999 { Vec3::Y } else { Vec3::Z };
    let distance = eye_distance(cfg, aspect);
    let view = look_at_mat4(target - dir * distance, target, up);
    perspective(fov(cfg), aspect, 1.0, distance * 12.0 + 2000.0) * view
}

/// Directions of the four corner rays of the view. Every face the camera can
/// see points against at least one of them.
pub fn corner_rays(cfg: &CameraConfig) -> [Vec3; 4] {
    let dir = view_dir(cfg);
    let (right, up) = right_up(cfg);
    let v = (fov(cfg) * 0.5).tan();
    // (A picture of its own shape wider still: as wide as that.)
    let h = v * cfg.aspect.map_or(MAX_ASPECT, |a| a.max(MAX_ASPECT));
    [
        (dir - right * h - up * v).normalize(),
        (dir + right * h - up * v).normalize(),
        (dir - right * h + up * v).normalize(),
        (dir + right * h + up * v).normalize(),
    ]
}

/// Seen from above (every corner ray pointing down), how far the ground
/// the screen shows reaches from the target on the level plane through
/// it: the least and the most offset across (x) and in depth (y), in
/// voxels (the far edge the wider: a view in perspective). None for a
/// view from the side. Where the camera stops at a level's edges.
pub fn ground_extent(cfg: &CameraConfig, aspect: f32) -> Option<(glam::Vec2, glam::Vec2)> {
    let dir = view_dir(cfg);
    let (right, up) = right_up(cfg);
    let v = (fov(cfg) * 0.5).tan();
    let h = v * aspect;
    let eye = -dir * eye_distance(cfg, aspect);
    let (mut lo, mut hi) = (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN));
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let ray = (dir + right * h * sx + up * v * sy).normalize();
        if ray.z > -1e-3 {
            return None;
        }
        let p = (eye + ray * (-eye.z / ray.z)).truncate();
        lo = lo.min(p);
        hi = hi.max(p);
    }
    Some((lo, hi))
}

/// How much of the upright plane through the target (across the view, at
/// its depth) the screen shows: half its width, and how far down and up
/// from the target, in voxels. For a view from the side, where the camera
/// stops at a level's edges.
pub fn view_extent(cfg: &CameraConfig, aspect: f32) -> (f32, f32, f32) {
    let dir = view_dir(cfg);
    let (right, up) = right_up(cfg);
    let v = (fov(cfg) * 0.5).tan();
    let ahead = Vec3::new(dir.x, dir.y, 0.0).normalize_or_zero();
    let distance = eye_distance(cfg, aspect);
    // Where a ray from the eye meets the plane, relative to the target.
    let meet = |ray: Vec3| {
        let along = ray.dot(ahead);
        if along <= 1e-3 {
            return Vec3::splat(f32::MAX);
        }
        ray * (distance * dir.dot(ahead) / along) - dir * distance
    };
    let side = meet(dir + right * v * aspect);
    let low = meet(dir - up * v);
    let high = meet(dir + up * v);
    (side.dot(right).abs(), -low.z, high.z)
}

/// Orthographic sun projection around what the camera sees: the ground the
/// view frustum covers between the floor and the top of the world, plus
/// everything between it and the sun. A view from the side (some corner
/// ray not pointing down) takes the stage instead: what the frustum covers
/// between its front and its back. The box keeps its size and moves in
/// whole shadow texels, so shadows stand still while the view scrolls. A
/// sun that moves turns it every picture: the faces keep a memory of
/// their shadow then (`FrameData::remember`).
/// Returns the matrix and the depth range it spans in voxels.
pub fn light_view_proj(
    cfg: &CameraConfig,
    target: Vec3,
    aspect: f32,
    sun: Vec3,
    world: Vec3,
    stage: (Vec3, Vec3),
    texels: u32,
) -> (Mat4, f32) {
    let sun = sun.normalize();
    // A fixed orientation, so moving the box never turns the texel grid.
    let up = if sun.z.abs() > 0.999 { Vec3::Y } else { Vec3::Z };
    let view = look_at_mat4(Vec3::ZERO, sun, up);

    // Where the corner rays meet the floor and the top of the world.
    let distance = eye_distance(cfg, aspect);
    let eye = target - view_dir(cfg) * distance;
    let far = distance * 12.0;
    let (right, screen_up) = right_up(cfg);
    let v = (fov(cfg) * 0.5).tan();
    let h = v * aspect;
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    let corners = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
    let ray = |(sx, sy): (f32, f32)| (view_dir(cfg) + right * h * sx + screen_up * v * sy).normalize();
    let from_above = corners.iter().all(|&c| ray(c).z < -1e-3);
    let (box_lo, box_hi) = if from_above { (Vec3::ZERO, world) } else { stage };
    for c in corners {
        let ray = ray(c);
        if from_above {
            for plane in [0.0, world.z] {
                let t = if ray.z < -1e-3 { ((plane - eye.z) / ray.z).clamp(0.0, far) } else { far };
                let p = view.transform_point3(eye + ray * t);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        } else {
            // From the side: where the ray meets the stage's front and back.
            for plane in [stage.0.y, stage.1.y] {
                let t = if ray.y > 1e-3 { ((plane - eye.y) / ray.y).clamp(0.0, far) } else { far };
                let p = view.transform_point3(eye + ray * t);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    // Depth: from beyond the top of the world (or the stage) towards the
    // sun down to its floor, wherever in it the box is.
    for corner in 0..8 {
        let c = Vec3::new(
            if corner & 1 == 0 { box_lo.x } else { box_hi.x },
            if corner & 2 == 0 { box_lo.y } else { box_hi.y },
            if corner & 4 == 0 { box_lo.z } else { box_hi.z },
        );
        let p = view.transform_point3(c);
        lo.z = lo.z.min(p.z);
        hi.z = hi.z.max(p.z);
    }
    let size = (hi - lo).truncate() + 2.0;
    let texel = size / texels as f32;
    let min = ((lo.truncate() - 1.0) / texel).floor() * texel;
    let max = min + texel * texels as f32;
    // Light view space looks down -z: near is -hi.z, far is -lo.z.
    let proj = orthographic(min.x, max.x, min.y, max.y, -hi.z - 4.0, -lo.z + 4.0);
    (proj * view, hi.z - lo.z + 8.0)
}

/// The sun's orthographic view around the box `lo..hi`, whatever side it
/// is seen from (the editor's orbit, where `light_view_proj`'s fit to a
/// view from above or the side does not hold); and the depth range it
/// spans in voxels.
pub fn light_around(sun: Vec3, lo: Vec3, hi: Vec3) -> (Mat4, f32) {
    let sun = sun.normalize();
    let up = if sun.z.abs() > 0.999 { Vec3::Y } else { Vec3::Z };
    let view = look_at_mat4(Vec3::ZERO, sun, up);
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for corner in 0..8 {
        let c = Vec3::new(
            if corner & 1 == 0 { lo.x } else { hi.x },
            if corner & 2 == 0 { lo.y } else { hi.y },
            if corner & 4 == 0 { lo.z } else { hi.z },
        );
        let p = view.transform_point3(c);
        min = min.min(p);
        max = max.max(p);
    }
    let proj = orthographic(min.x - 1.0, max.x + 1.0, min.y - 1.0, max.y + 1.0, -max.z - 4.0, -min.z + 4.0);
    (proj * view, max.z - min.z + 8.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The editor's light box holds what it is fitted around, from every
    /// side the sun may shine: every corner lands inside the shadow map
    /// and its depth range.
    #[test]
    fn the_light_box_holds_its_box_from_every_side() {
        let (lo, hi) = (Vec3::new(0.0, 0.0, -16.0), Vec3::new(928.0, 32.0, 400.0));
        for sun in [Vec3::new(0.35, 0.8, -1.0), Vec3::new(-1.0, 0.2, -0.3), Vec3::NEG_Z, Vec3::new(0.0, -1.0, 0.1)] {
            let (m, _) = light_around(sun, lo, hi);
            for corner in 0..8 {
                let c = Vec3::new(
                    if corner & 1 == 0 { lo.x } else { hi.x },
                    if corner & 2 == 0 { lo.y } else { hi.y },
                    if corner & 4 == 0 { lo.z } else { hi.z },
                );
                let p = m.project_point3(c);
                assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0 && (0.0..=1.0).contains(&p.z), "{sun}: {c} lands at {p}");
            }
        }
    }

    /// With `view_height` the screen shows the same height in any window
    /// (a wide one, the IDE's 2270 by 779, a 4:3 one) and the width follows
    /// the window; without, the width is fixed and the height follows.
    #[test]
    fn a_fixed_height_shows_the_same_height_in_any_window() {
        let fixed = CameraConfig { pitch: 62.0, view_height: Some(270.0), ..Default::default() };
        let wide = CameraConfig { pitch: 62.0, view_width: 480.0, ..Default::default() };
        let windows = [16.0 / 9.0, 2270.0 / 779.0, 4.0 / 3.0];
        let tall = |cfg: &CameraConfig, aspect: f32| {
            let (_, down, up) = view_extent(cfg, aspect);
            down + up
        };
        for aspect in windows {
            assert!((tall(&fixed, aspect) - tall(&fixed, 16.0 / 9.0)).abs() < 0.01, "{aspect}: another height shown");
            assert!((view_extent(&fixed, aspect).0 * 2.0 - shown_width(&fixed, aspect)).abs() < 0.01 * shown_width(&fixed, aspect), "{aspect}: the width follows the window");
            assert!((view_extent(&wide, aspect).0 * 2.0 - 480.0).abs() < 1.0, "{aspect}: the width stays");
        }
        assert!(tall(&wide, 2270.0 / 779.0) < tall(&wide, 16.0 / 9.0) * 0.7, "a fixed width shows less height in a wide window");
    }

    /// With `view_at_least` the screen shows at least that much both ways
    /// in any window, more across in a wide one, more high in a narrow one:
    /// 320 by 200 is 356 by 200 at 16:9, 320 by 240 at 4:3, 474 by 200 at
    /// 21:9 (across and high where the camera looks, square to the view).
    #[test]
    fn a_view_at_least_shows_that_much_in_any_window() {
        let cfg = CameraConfig { pitch: 50.0, view_at_least: Some([320.0, 200.0]), ..Default::default() };
        for (aspect, want) in [(16.0 / 9.0, (355.6, 200.0)), (16.0 / 10.0, (320.0, 200.0)), (4.0 / 3.0, (320.0, 240.0)), (64.0 / 27.0, (474.1, 200.0))] {
            let width = shown_width(&cfg, aspect);
            let (w, h) = (width, width / aspect);
            assert!((w - want.0).abs() < 0.1 && (h - want.1).abs() < 0.1, "{aspect}: {w} by {h}");
            assert!((view_extent(&cfg, aspect).0 * 2.0 - w).abs() < 0.01 * w, "{aspect}: the picture shows it");
        }
        let mut title = cfg.clone();
        title.show_width(360.0);
        assert_eq!(shown_width(&title, 21.0 / 9.0), 360.0, "a width of its own counts alone");
    }
}
