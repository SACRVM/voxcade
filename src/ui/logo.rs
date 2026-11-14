//! Big letters: the title's logo (dropping in, bobbing, its letters in the
//! model bank), extruded glyphs with rounded edges, grain, cracks and
//! drips, a logo model's parts.

use super::*;

/// The game's logo, big and deep, its letters falling in one after the
/// other; the drip line bleeds. In the display font when the game has one.
/// The letters come from the model bank (`letters`, where they are) when
/// it holds them: building them every frame took most of the title's. A
/// game's own logo model instead, its parts falling in one after another.
pub(super) fn logo(out: &mut Vec<Instance>, letters: &mut Vec<ModelDraw>, view: &MenuView, layout: &Layout, now: f32, finish: Finish) {
    if let Some(model) = view.logo_model {
        let x0 = ((layout.width - model.size.x) / 2.0).round();
        let y0 = layout.height - 14.0 - model.size.y;
        for (i, (voxels, at)) in model.parts.iter().enumerate() {
            let t = now - (i as u64 * LETTER_DELAY * 2) as f32;
            if let Some(lift) = drop_lift(t, 90.0, 0.2) {
                let origin = Vec3::new(x0 + at.x, y0 + at.y + lift + bob(t, now, i, view.logo_bob), at.z);
                letters.push(ModelDraw { voxels: voxels.clone(), origin, turn: 0, rot: glam::Quat::IDENTITY, shift: glam::Vec3::ZERO, tint: None, nearer: 0.0, shadow_only: false });
            }
        }
        return;
    }
    let lines = view.logo;
    let mut y = layout.height - 14.0;
    let mut k = 0;
    for (n, (line, color)) in lines.iter().enumerate() {
        let height = logo_height(n, lines.len(), view.logo_heights);
        let glyphs = big_glyphs(line, height, view.font);
        let gap = gap(height, view.font);
        let width = glyphs.iter().map(|g| g.width).sum::<i32>() + gap * (glyphs.len() as i32 - 1).max(0);
        y -= height as f32;
        let x0 = ((layout.width - width as f32) / 2.0).round();
        let t = now - n as f32 * 25.0;
        let cracks = view.logo_cracks.get(n).copied().unwrap_or(0.0);
        let mut x = x0;
        for (i, g) in glyphs.iter().enumerate() {
            let tl = t - (i as u64 * LETTER_DELAY * 2) as f32;
            if let Some(lift) = drop_lift(tl, 90.0, 0.2) {
                let lift = lift + bob(tl, now, k, view.logo_bob);
                match view.logo_bank.get(n).and_then(|line| line.get(i)) {
                    Some(voxels) => letters.push(ModelDraw { voxels: voxels.clone(), origin: Vec3::new(x, y + lift, 0.0), turn: 0, rot: glam::Quat::IDENTITY, shift: glam::Vec3::ZERO, tint: None, nearer: 0.0, shadow_only: false }),
                    None => extrude(out, g, x, y + lift, *color, logo_depth(height), finish.with(cracks, (n * 100 + i) as u32)),
                }
            }
            if view.drip == Some(n) && now > 70.0 {
                drips(out, g, x, y, i, now);
            }
            x += (g.width + gap) as f32;
            k += 1;
        }
        y -= 6.0;
    }
}

/// How far the logo's `i`th letter is up from where it landed, `t` ticks
/// after it was let go, `now` on the title's clock (with the picture's
/// share of a tick): a slow wave running through the letters left to
/// right, `amount` voxels up and down, coming in over a second once the
/// letter has landed and stopped bouncing. Smooth, not in whole voxels:
/// a letter stepping a voxel at a time looked as if the game stuttered
/// (the user); its voxels keep their grid among themselves.
pub(super) fn bob(t: f32, now: f32, i: usize, amount: f32) -> f32 {
    const LANDED: f32 = 60.0;
    const PERIOD: f32 = 150.0;
    if amount <= 0.0 || t < LANDED {
        return 0.0;
    }
    let swell = ((t - LANDED) / 60.0).min(1.0);
    let phase = now / PERIOD * std::f32::consts::TAU - i as f32 * 0.6;
    amount * swell * phase.sin()
}

/// How tall a logo line's capitals are (the last line twice the others'
/// when the game does not say).
pub(super) fn logo_height(n: usize, lines: usize, heights: &[u32]) -> u32 {
    heights.get(n).copied().unwrap_or(if n + 1 == lines { 42 } else { 21 })
}

/// How deep a logo line's letters are: deeper the bigger, so a big line
/// stands out of the screen.
pub(super) fn logo_depth(height: u32) -> i32 {
    (height as i32 / 5).max(6)
}

/// The logo's letters, built once for the model bank: per line, per letter,
/// its cubes with the letter's lower left corner at the origin, the faces a
/// neighbour covers marked. `logo` places them where they fall.
pub fn logo_letters(lines: &[(String, [u8; 4])], heights: &[u32], cracks: &[f32], font: Option<&DisplayFont>, cfg: &UiConfig) -> Vec<Vec<Vec<Instance>>> {
    let finish = Finish::of(cfg);
    lines
        .iter()
        .enumerate()
        .map(|(n, (line, color))| {
            let height = logo_height(n, lines.len(), heights);
            let cracks = cracks.get(n).copied().unwrap_or(0.0);
            big_glyphs(line, height, font)
                .iter()
                .enumerate()
                .map(|(i, g)| {
                    let mut cubes = Vec::new();
                    extrude(&mut cubes, g, 0.0, 0.0, *color, logo_depth(height), finish.with(cracks, (n * 100 + i) as u32));
                    crate::render::hide_inner_faces(&mut cubes);
                    cubes
                })
                .collect()
        })
        .collect()
}

/// A game's own logo, a model (`c.title_screen.logo_model`), in the model
/// bank: how big it is in UI voxels (across, up, deep), and each part's
/// voxels there with its lower left back corner in the logo.
pub struct LogoModel {
    pub size: Vec3,
    pub parts: Vec<(std::ops::Range<u32>, Vec3)>,
}

/// The cubes of a logo model, part by part (the whole as one part when it
/// has none), each with its lower left back corner in the logo: the
/// model's x across, its z up, its y towards the player. Lit as the big
/// letters are: a voxel facing the player in its colour, darker the
/// further it lies behind the model's front (a fifth, three voxels back
/// and more), a voxel showing only at its sides a fifth darker; the rest
/// is hidden and left out.
pub fn logo_model_parts(model: &crate::vox::VoxModel) -> (Vec3, Vec<(Vec<Instance>, Vec3)>) {
    let frame = &model.frames[0];
    let filled: std::collections::HashSet<IVec3> = frame.iter().filter(|(_, c)| c[3] == 255).map(|(p, _)| *p).collect();
    let front = frame.iter().map(|(p, _)| p.y).max().unwrap_or(0);
    let at = |p: IVec3| Vec3::new(p.x as f32, p.z as f32, p.y as f32);
    let all: Vec<u32> = (0..frame.len() as u32).collect();
    let parts: Vec<&[u32]> = if model.parts.is_empty() { vec![&all] } else { model.parts.iter().map(|p| p.voxels.as_slice()).collect() };
    let parts = parts
        .into_iter()
        .map(|voxels| {
            let low = voxels.iter().map(|&v| at(frame[v as usize].0)).fold(Vec3::splat(f32::MAX), Vec3::min);
            let mut cubes = Vec::new();
            for &v in voxels {
                let (p, color) = frame[v as usize];
                let open = |d: IVec3| !filled.contains(&(p + d));
                let shade = if open(IVec3::Y) {
                    1.0 - 0.2 * ((front - p.y) as f32 / 3.0).min(1.0)
                } else if open(IVec3::X) || open(IVec3::NEG_X) || open(IVec3::Z) || open(IVec3::NEG_Z) || open(IVec3::NEG_Y) {
                    0.8
                } else {
                    continue;
                };
                let q = at(p) - low;
                voxel(&mut cubes, q.x, q.y, q.z, darker(color, shade));
            }
            crate::render::hide_inner_faces(&mut cubes);
            (cubes, low)
        })
        .collect();
    (Vec3::new(model.size.x as f32, model.size.z as f32, model.size.y as f32), parts)
}

/// The glyphs of a line of big letters `height` voxels high: from the
/// display font, else the pixel font blown up.
pub(super) fn big_glyphs(s: &str, height: u32, font: Option<&DisplayFont>) -> Vec<Rc<Glyph>> {
    match font {
        Some(f) => s.chars().map(|c| f.glyph(c, height)).collect(),
        None => engine_case(s).chars().map(|c| Rc::new(pixel_glyph(c, (height / 7).max(1) as i32))).collect(),
    }
}

pub(super) fn gap(height: u32, font: Option<&DisplayFont>) -> i32 {
    if font.is_some() { DisplayFont::gap(height) } else { (height / 7).max(1) as i32 }
}

/// A pixel font letter as a glyph, `scale` voxels a pixel (a space for
/// characters it lacks).
pub(super) fn pixel_glyph(ch: char, scale: i32) -> Glyph {
    let Some(g) = glyph(ch) else {
        return Glyph::new(5 * scale, 7 * scale, Vec::new());
    };
    let accent = accented(ch).map(|(_, a)| a);
    let rows = (0..9 * scale)
        .map(|r| (0..5 * scale).map(|c| shown_row(g, accent, r / scale).as_bytes()[(c / scale) as usize] == b'#').collect())
        .collect();
    Glyph::new(5 * scale, 7 * scale, rows)
}

/// How far a letter dropping in is still up, `t` ticks after it was let
/// go (None: not yet): it falls from `height` and bounces once, `bounce`
/// as high as it fell fast.
pub(super) fn drop_lift(t: f32, height: f32, bounce: f32) -> Option<f32> {
    let gravity = 900.0f32;
    let fall = (2.0 * height / gravity).sqrt();
    let tl = t * DT;
    if tl < 0.0 {
        return None;
    }
    let lift = if tl < fall {
        height - 0.5 * gravity * tl * tl
    } else {
        let tb = tl - fall;
        (bounce * gravity * fall * tb - 0.5 * gravity * tb * tb).max(0.0)
    };
    Some(lift.round())
}

/// How big letters are finished: the front edges rounded off, grain in the
/// colour, cracks carved in. Fixed to the letter, so it holds still while
/// the letter moves.
#[derive(Clone, Copy, Default)]
pub struct Finish {
    /// Radius of the rounded front edges, a share of the capital height.
    pub round: f32,
    /// How much the colour varies over the letter, 0..1.
    pub grain: f32,
    /// Cracks per letter, on average.
    pub cracks: f32,
    /// Picks the letter's grain and cracks.
    pub seed: u32,
}

impl Finish {
    pub fn of(cfg: &UiConfig) -> Self {
        Self { round: cfg.round, grain: cfg.grain, ..Self::default() }
    }

    fn with(self, cracks: f32, seed: u32) -> Self {
        Self { cracks, seed, ..self }
    }
}

/// A big letter `depth` voxels deep with its lower left corner at (x, y):
/// the front in `color`, the sides darker, and only the voxels that show
/// (the inside stays empty).
pub(super) fn extrude(out: &mut Vec<Instance>, g: &Glyph, x: f32, y: f32, color: [u8; 4], depth: i32, finish: Finish) {
    let radius = (finish.round * g.ascent as f32).min((depth - 1) as f32);
    let cracks = crack_map(g, finish, depth);
    let carved = |c: i32, r: i32| if g.on(c, r) { cracks[r as usize][c as usize] as i32 } else { 0 };
    // Whether the letter fills voxel (c, r, z): inside the outline, and not
    // cut away by the rounding at the front or by a crack.
    let filled = |c: i32, r: i32, z: i32| {
        if z < 0 || z >= depth || !g.on(c, r) {
            return false;
        }
        let back = depth - 1 - z;
        let behind = back as f32 + 0.5;
        if radius > 0.0 && behind < radius {
            let e = g.inside(c, r);
            let (a, b) = (radius - e, radius - behind);
            if e < radius && a * a + b * b > radius * radius {
                return false;
            }
        }
        back >= carved(c, r)
    };
    let side = darker(color, 0.8);
    let crack = darker(color, 0.35);
    for r in 0..g.rows.len() as i32 {
        for c in 0..g.width {
            let (px, py) = (x + c as f32, y + (g.ascent - 1 - r) as f32);
            for z in 0..depth {
                if !filled(c, r, z) {
                    continue;
                }
                let open = !filled(c, r, z + 1);
                let walled = filled(c - 1, r, z) && filled(c + 1, r, z) && filled(c, r - 1, z) && filled(c, r + 1, z);
                if !open && walled && filled(c, r, z - 1) {
                    continue;
                }
                // Facing out of the screen: the front, the rounding's steps a
                // little darker the further back, a crack's floor dark.
                let back = depth - 1 - z;
                let base = if !open {
                    side
                } else if carved(c, r) > 0 {
                    crack
                } else {
                    darker(color, 1.0 - 0.2 * (back as f32 / radius.max(1.0)).min(1.0))
                };
                voxel(out, px, py, z as f32, grain(base, c, r, z, finish));
            }
        }
    }
}

/// Where cracks run in a letter: how many voxels deep each cell is carved
/// from the front (0: not at all). They start at the outline and run
/// inwards in straight stretches with sharp kinks, wide and deep at the
/// mouth, thin and shallow towards the tip, now and then forking.
pub(super) fn crack_map(g: &Glyph, finish: Finish, depth: i32) -> Vec<Vec<u8>> {
    let mut map: Vec<Vec<u8>> = g.rows.iter().map(|row| vec![0; row.len()]).collect();
    if finish.cracks <= 0.0 {
        return map;
    }
    let mut rng = Rng::new(finish.seed as u64 + 1);
    let count = (finish.cracks + rng.f32()) as usize;
    let mut outline = Vec::new();
    let (mut sum, mut cells) = (glam::Vec2::ZERO, 0.0);
    for r in 0..g.rows.len() as i32 {
        for c in 0..g.width {
            if g.on(c, r) {
                sum += glam::Vec2::new(c as f32, r as f32);
                cells += 1.0;
                if !g.on(c - 1, r) || !g.on(c + 1, r) || !g.on(c, r - 1) || !g.on(c, r + 1) {
                    outline.push((c, r));
                }
            }
        }
    }
    if outline.is_empty() {
        return map;
    }
    let middle = sum / cells;
    let deepest = (depth - 1).clamp(1, 3) as f32;
    let mut runs = Vec::new();
    for _ in 0..count {
        let (c, r) = outline[(rng.f32() * outline.len() as f32) as usize % outline.len()];
        let at = glam::Vec2::new(c as f32 + 0.5, r as f32 + 0.5);
        let inwards = middle + glam::Vec2::splat(0.5) - at;
        let angle = inwards.y.atan2(inwards.x) + rng.range(-0.7, 0.7);
        runs.push((at, angle, g.ascent as f32 * rng.range(0.3, 0.6), deepest));
    }
    while let Some((mut at, mut angle, length, deep)) = runs.pop() {
        let mut gone = 0.0;
        let mut kink = rng.range(3.0, 6.0);
        while gone < length {
            let left = 1.0 - gone / length;
            let d = (deep * left).ceil().max(1.0) as u8;
            // Two voxels wide over the first third.
            let side = glam::Vec2::from_angle(angle).perp() * 0.7;
            let wide = left > 0.67;
            let mut cut = |p: glam::Vec2| {
                let (c, r) = (p.x.floor() as i32, p.y.floor() as i32);
                if g.on(c, r) {
                    let cell = &mut map[r as usize][c as usize];
                    *cell = (*cell).max(d);
                }
            };
            cut(at);
            if wide {
                cut(at + side);
            }
            if !g.on(at.x.floor() as i32, at.y.floor() as i32) && gone > 1.5 {
                break;
            }
            kink -= 1.0;
            if kink <= 0.0 {
                kink = rng.range(3.0, 6.0);
                angle += if rng.f32() < 0.5 { -1.0 } else { 1.0 } * rng.range(0.3, 0.8);
                if rng.f32() < 0.25 && length - gone > 6.0 {
                    let turn = if rng.f32() < 0.5 { -1.0 } else { 1.0 } * rng.range(0.6, 1.0);
                    runs.push((at, angle + turn, (length - gone) * 0.5, (deep - 1.0).max(1.0)));
                }
            }
            at += glam::Vec2::from_angle(angle + rng.range(-0.15, 0.15)) * 0.9;
            gone += 0.9;
        }
    }
    map
}

/// `color` with the grain of voxel (c, r, z): a fine speckle and broad
/// blotches.
pub(super) fn grain(color: [u8; 4], c: i32, r: i32, z: i32, finish: Finish) -> [u8; 4] {
    if finish.grain <= 0.0 {
        return color;
    }
    let speckle = cell_hash(IVec3::new(c, r, z), finish.seed) - 0.5;
    let blotch = smooth_noise(c as f32 / 5.0, r as f32 / 5.0, finish.seed) - 0.5;
    let k = 1.0 + finish.grain * (speckle * 0.8 + blotch * 1.6);
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    [f(color[0]), f(color[1]), f(color[2]), color[3]]
}

/// Smooth noise in [0, 1): `cell_hash` on a lattice, blended between.
pub(super) fn smooth_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (ease(x - x0), ease(y - y0));
    let v = |dx: i32, dy: i32| cell_hash(IVec3::new(x0 as i32 + dx, y0 as i32 + dy, -1), seed);
    let top = v(0, 0) + (v(1, 0) - v(0, 0)) * sx;
    let bottom = v(0, 1) + (v(1, 1) - v(0, 1)) * sx;
    top + (bottom - top) * sy
}

pub(super) fn darker(c: [u8; 4], k: f32) -> [u8; 4] {
    [(c[0] as f32 * k) as u8, (c[1] as f32 * k) as u8, (c[2] as f32 * k) as u8, c[3]]
}

/// Blood running off the bottom of a letter standing on `y` at `x`: under
/// some of its lowest voxels a drip grows, lets a drop go and starts again.
pub(super) fn drips(out: &mut Vec<Instance>, g: &Glyph, x: f32, y: f32, letter: usize, now: f32) {
    let step = (g.width / 3).max(2);
    for c in 0..g.width {
        // The lowest voxel of this column, if any.
        let Some(r) = (0..g.rows.len() as i32).rev().find(|&r| g.on(c, r)) else { continue };
        let seed = (letter * 31 + c as usize * 7) as u32;
        if (c + letter as i32) % step != step / 2 || seed.is_multiple_of(3) {
            continue;
        }
        let period = 150.0 + (seed % 5) as f32 * 40.0;
        let t = (now + seed as f32 * 37.0) % period;
        let px = x + c as f32;
        let bottom = y + (g.ascent - 1 - r) as f32;
        // Grows for most of the period, then a drop falls.
        let grow = (t / (period * 0.7)).min(1.0);
        let hang = (grow * 6.0).round() as i32;
        for k in 1..=hang {
            let w = if k == hang { 1 } else { 2 };
            for dx in 0..w {
                voxel(out, px + dx as f32 - 1.0, bottom - k as f32, 4.0, DRIP);
            }
        }
        if t > period * 0.7 {
            let fall = (t - period * 0.7) * DT;
            let dy = 0.5 * 300.0 * fall * fall;
            voxel(out, px - 1.0, (bottom - 7.0 - dy).round(), 4.0, DRIP);
        }
    }
}
