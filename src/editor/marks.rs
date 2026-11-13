//! The marks over the view among the pieces (`marks`) and a figure's
//! joints (`joint_marks`), on the screen.

use super::*;

/// How a mark over the 3D view is drawn: the chosen one's, the others',
/// a quiet one's (the room's edges, the item the mouse is over).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Chosen,
    Plain,
    Quiet,
}

/// What the view marks over the voxels among the pieces, on the screen
/// (shares from the top left): lines (boxes, arrows, a light's line down
/// to the floor), rings (lights, in their colour; places), see-through
/// faces (areas) and names.
#[derive(Default, Debug)]
pub struct Marks {
    pub lines: Vec<(glam::Vec2, glam::Vec2, Tone)>,
    pub rings: Vec<(glam::Vec2, f32, Option<[u8; 3]>, Tone)>,
    pub faces: Vec<([glam::Vec2; 4], Tone)>,
    pub names: Vec<(glam::Vec2, String, Tone)>,
}

/// The marks among the pieces (`aspect`, the panels taking `inset`).
pub fn marks(ed: &Editor, view: &View, figures: &Figures, aspect: f32, inset: glam::Vec2) -> Marks {
    let mut out = Marks::default();
    if !ed.among_pieces() {
        return out;
    }
    let view_proj = Mat4::from_translation(Vec3::new(-inset.x, inset.y, 0.0)) * view.level.view_proj(aspect);
    let on_screen = |p: Vec3| {
        let c = view_proj * p.extend(1.0);
        (c.w > 1e-3).then(|| glam::Vec2::new((c.x / c.w + 1.0) / 2.0, (1.0 - c.y / c.w) / 2.0))
    };
    let mut line = |out: &mut Marks, a: Vec3, b: Vec3, tone: Tone| {
        if let (Some(a), Some(b)) = (on_screen(a), on_screen(b)) {
            out.lines.push((a, b, tone));
        }
    };
    // A box's twelve edges, its corners turned by `turn` about `about`.
    let boxed = |out: &mut Marks, line: &mut dyn FnMut(&mut Marks, Vec3, Vec3, Tone), lo: Vec3, hi: Vec3, turn: Quat, about: Vec3, tone: Tone| {
        let corner = |k: usize| {
            let p = Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
            about + turn * (p - about)
        };
        for k in 0..8 {
            for bit in [1, 2, 4] {
                if k & bit == 0 {
                    line(out, corner(k), corner(k | bit), tone);
                }
            }
        }
    };
    let tone_of = |item: Item| if ed.pieces.chosen == Some(item) { Tone::Chosen } else { Tone::Plain };
    let things: Vec<(Option<Item>, &crate::grid::Thing)> = ed.level.things.iter().enumerate().filter(|&(i, _)| !ed.is_hidden(Item::Thing(i))).map(|(i, t)| (Some(Item::Thing(i)), t)).chain(match &ed.pieces.hanging {
        Some(Hanging::Thing(t)) if ed.pieces.hang_spot.is_some() => Some((None, t)),
        _ => None,
    }).collect();
    for (item, t) in things {
        let at = IVec3::from(t.at).as_vec3();
        let tone = item.map_or(Tone::Plain, tone_of);
        let marked = item.is_none() || tone == Tone::Chosen || item == view.over;
        let size = ed.size_of(t);
        if (t.model.is_some() || !t.is_piece()) && marked {
            let (lo, hi, turn, about) = if t.is_piece() {
                let (lo, hi) = figures.piece_box(t);
                (lo, hi, room::rotation_of(t), room::centre_in(size))
            } else {
                let (lo, hi) = figures.bounds(&figures.key_in(t));
                (lo.as_vec3(), hi.as_vec3(), Quat::IDENTITY, Vec3::ZERO)
            };
            let hovered = item == view.over && tone != Tone::Chosen && item.is_some();
            let shown = if hovered { Tone::Quiet } else { tone };
            boxed(&mut out, &mut line, at + lo, at + hi, turn, at + about, shown);
            // Its name over it, chosen or under the mouse: the top of its
            // box as it lies.
            let top = (0..8).map(|k| (turn * (Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z }) - about)).z + about.z).fold(f32::MIN, f32::max);
            // (The chosen one's name is in the outliner and the inspector:
            // over it here it would lie under its handles.)
            if hovered
                && let Some(p) = on_screen(at + Vec3::new(0.5, 0.5, top + 2.0))
            {
                out.names.push((p, ed.label_of(item.expect("hovered")).0, Tone::Plain));
            }
        }
        // A light: a ring in its colour where it shines from; a light
        // alone with its line down to the floor and its name.
        for (k, (spot, l)) in room::light_spots(t, size).enumerate() {
            let tone = if tone == Tone::Chosen && ed.pieces.chosen_light.is_some_and(|c| c != k) { Tone::Plain } else { tone };
            let rgb = l.rgb().map(|c| (c * 255.0) as u8);
            let alone = t.model.is_none();
            if alone {
                line(&mut out, spot, Vec3::new(spot.x, spot.y, 0.0), Tone::Quiet);
            }
            if let Some(p) = on_screen(spot) {
                let size = if alone { 7.0 } else { 5.0 };
                out.rings.push((p, if tone == Tone::Chosen { size + 2.0 } else { size }, Some(rgb), tone));
                if alone && tone != Tone::Chosen && let Some(item) = item {
                    out.names.push((p + glam::Vec2::new(0.0, -0.025), ed.label_of(item).0, tone));
                }
            }
        }
    }
    let places: Vec<(Option<Item>, &crate::grid::Place)> = ed.level.places.iter().enumerate().filter(|&(i, _)| !ed.is_hidden(Item::Place(i))).map(|(i, p)| (Some(Item::Place(i)), p)).chain(match &ed.pieces.hanging {
        Some(Hanging::Place(p)) if ed.pieces.hang_spot.is_some() => Some((None, p)),
        _ => None,
    }).collect();
    for (item, p) in places {
        let tone = item.map_or(Tone::Plain, tone_of);
        let feet = IVec3::from(p.at).as_vec3() + Vec3::new(0.5, 0.5, 0.0);
        // A ring on the floor and an arrow the way it faces.
        let round: Vec<Vec3> = (0..=16).map(|k| feet + Quat::from_rotation_z(k as f32 / 16.0 * std::f32::consts::TAU) * Vec3::new(4.0, 0.0, 0.0)).collect();
        for w in round.windows(2) {
            line(&mut out, w[0], w[1], tone);
        }
        let [fx, fy] = p.facing.unwrap_or([0, -1]);
        let way = Vec3::new(fx as f32, fy as f32, 0.0).normalize_or(Vec3::NEG_Y);
        let tip = feet + way * 12.0;
        line(&mut out, feet + way * 4.0, tip, tone);
        for side in [1.0f32, -1.0] {
            line(&mut out, tip, tip - way * 4.0 + Vec3::new(-way.y, way.x, 0.0) * 3.0 * side, tone);
        }
        if let (Some(item), Some(at)) = (item, on_screen(feet + Vec3::Z * 3.0)) {
            out.names.push((at + glam::Vec2::new(0.0, 0.02), ed.label_of(item).0, tone));
        }
    }
    let areas: Vec<(Option<Item>, &crate::grid::Area)> = ed.level.areas.iter().enumerate().filter(|&(i, _)| !ed.is_hidden(Item::Area(i))).map(|(i, a)| (Some(Item::Area(i)), a)).chain(match &ed.pieces.hanging {
        Some(Hanging::Area(a)) if ed.pieces.hang_spot.is_some() => Some((None, a)),
        _ => None,
    }).collect();
    for (item, a) in areas {
        let tone = item.map_or(Tone::Plain, tone_of);
        let (lo, hi) = (IVec3::from(a.at).as_vec3(), (IVec3::from(a.at) + IVec3::from(a.size)).as_vec3());
        boxed(&mut out, &mut line, lo, hi, Quat::IDENTITY, lo, tone);
        // Its six faces, see-through.
        for axis in 0..3 {
            for side in [lo[axis], hi[axis]] {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let corner = |cu: f32, cv: f32| {
                    let mut p = Vec3::ZERO;
                    (p[axis], p[u], p[v]) = (side, cu, cv);
                    on_screen(p)
                };
                if let (Some(a), Some(b), Some(c), Some(d)) = (corner(lo[u], lo[v]), corner(hi[u], lo[v]), corner(hi[u], hi[v]), corner(lo[u], hi[v])) {
                    out.faces.push(([a, b, c, d], tone));
                }
            }
        }
        if let (Some(item), Some(at)) = (item, on_screen(Vec3::new(lo.x, lo.y, hi.z))) {
            out.names.push((at + glam::Vec2::new(0.0, -0.02), ed.label_of(item).0, tone));
        }
    }
    out
}

/// A joint of a figure as FIGURE marks it over the 3D view: where (shares
/// of the screen, from the top left), where its parent's joint is (the
/// chosen part's only), and whether it is the chosen part's or has a
/// warning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointMark {
    pub part: usize,
    pub at: glam::Vec2,
    pub parent: Option<glam::Vec2>,
    pub chosen: bool,
    pub warned: bool,
}

/// FIGURE's joints on the screen (`aspect`, the panels taking `inset`):
/// one for each part shown that lies before the eye.
pub fn joint_marks(ed: &Editor, view: &View, aspect: f32, inset: glam::Vec2) -> Vec<JointMark> {
    let (Some(figure), Some((lo, _))) = (&ed.figure, ed.figure_box()) else { return Vec::new() };
    // GAME VIEW shows the figure alone, as the game does.
    if view.before_game.is_some() {
        return Vec::new();
    }
    // Each joint where it shows at rest (`Figure::joint_shown`), seen as
    // the view draws: through the chosen part's rest (`View::rest`).
    let view_proj = Mat4::from_translation(Vec3::new(-inset.x, inset.y, 0.0)) * view.block.view_proj(aspect);
    let on_screen = |p: Vec3| {
        let c = view_proj * p.extend(1.0);
        (c.w > 1e-4).then(|| glam::Vec2::new((c.x / c.w + 1.0) / 2.0, (1.0 - c.y / c.w) / 2.0))
    };
    let parts = figure.parts();
    (0..parts.len())
        .filter(|&p| !figure.hidden[p])
        .filter_map(|p| {
            let chosen = figure.selected == Some(p) && !ed.rigging.painting_whole;
            let parent = parts[p].parent.filter(|_| chosen).and_then(|q| on_screen(figure.joint_shown(q, lo)));
            Some(JointMark { part: p, at: on_screen(figure.joint_shown(p, lo))?, parent, chosen, warned: !figure.warnings(p).is_empty() })
        })
        .collect()
}
