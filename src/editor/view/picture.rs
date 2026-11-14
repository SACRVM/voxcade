//! A frame of the editor (`frame`): what is shown in the game's light,
//! the floor under it, the things, the tools' marks; the pieces' lights.

use super::*;

/// A frame of the editor: what is shown in the game's light, the floor
/// under it, the things (in the level), where the tool would work (the
/// panels are egui's, `panels.rs`, drawn over it). Without the mouse's
/// marks it is the IDE's preview too.
pub fn frame(ed: &Editor, view: &View, shown: &Shown, figures: &Figures, light: &LightConfig, aspect: f32) -> FrameData {
    let n = ed.level.n.max(1);
    let (orbit, unit) = match ed.scope {
        Scope::Level => (&view.level, n),
        Scope::Block(_) | Scope::Frame(_) | Scope::Figure => (&view.block, 1),
    };
    let grid = ed.grid();
    // The camera and the world as they are, whatever part is chosen (the
    // owner: a view that tilts with every choice disorients). A part
    // turned at rest is drawn where and how it stands: its own voxels and
    // the tools' marks, which lie in its own grid, placed by `rest` (its
    // grid to the figure's); the tools' rays come the other way
    // (`View::view_proj` of the block, `View::ray`).
    let view_proj = Mat4::from_translation(Vec3::new(-view.inset.x, view.inset.y, 0.0)) * orbit.view_proj(aspect);
    let rest = if std::ptr::eq(orbit, &view.block) { view.rest } else { Mat4::IDENTITY };
    let turned = rest != Mat4::IDENTITY;
    let seen = Mat4::IDENTITY;
    let eye = crate::camera::eye(&orbit.camera(), orbit.target, aspect);
    // The level's own daylight over the game's (`[light]`), in the level
    // only; a block or a model in the game's.
    let own = ed.level.daylight.as_ref().filter(|_| ed.scope == Scope::Level).map(crate::grid::Daylight::picture).unwrap_or_default();
    let sun = own.sun.unwrap_or_else(|| Vec3::from(light.sun)).normalize_or(Vec3::NEG_Z);
    // A figure's part is shaped in the figure: the room is the figure's
    // (with the part's box), the floor under it all.
    // (A part turned: its own box is in its own grid, not the figure's.)
    let (room_lo, room_hi) = match ed.figure_box() {
        Some((lo, size)) if turned => (lo, lo + size),
        Some((lo, size)) => (lo.min(IVec3::ZERO), (lo + size).max(shown.world.size)),
        None => (IVec3::ZERO, shown.world.size),
    };
    let (light_vp, depth_range) = crate::camera::light_around(sun, room_lo.as_vec3() * unit as f32 - Vec3::new(0.0, 0.0, unit as f32), room_hi.as_vec3().max(Vec3::ONE) * unit as f32);

    // The floor: a cell deep under the grid, in two shades (the editor's,
    // after its BACKGROUND; not in the IDE's pictures).
    let background = view.background(ed.scope);
    let floor_tones = crate::editor::background::floor(background);
    let marks = view.marks(ed.scope);
    let mut instances = Vec::new();
    let u = unit as f32;
    let (floor_lo, floor_size) = if ed.figure_box().is_some() { (room_lo, room_hi - room_lo) } else { (IVec3::ZERO, grid) };
    // A level without blocks: tiles of `ROOM_TILE` voxels to stand
    // things on.
    let (floor_lo, floor_size, u) = if ed.scope == Scope::Level && !ed.level.has_grid() {
        (IVec3::ZERO, (grid + ROOM_TILE - 1) / ROOM_TILE, ROOM_TILE as f32)
    } else {
        (floor_lo, floor_size, u)
    };
    // (A level's GAME VIEW shows what the game shows: no editor's floor.)
    let floor = view.floor && !(view.before_game.is_some() && ed.scope == Scope::Level);
    for y in floor_lo.y..floor_lo.y + floor_size.y * floor as i32 {
        for x in floor_lo.x..floor_lo.x + floor_size.x {
            let shade = floor_tones[(x + y).rem_euclid(2) as usize];
            instances.push(Instance { pos_size: [(x as f32 + 0.5) * u, (y as f32 + 0.5) * u, (floor_lo.z as f32 - 0.5) * u, u], color: shade, fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] });
        }
    }
    let u = unit as f32;
    // The pieces' glass, and what hangs on the mouse, see-through; the
    // pieces only casting a shadow (`cast`), and whether they show.
    let mut glass = Vec::new();
    let mut cast = Vec::new();
    let ghosts = view.ghosts && view.before_game.is_none();
    let things = ed.scope == Scope::Level && ed.layer == Layer::Things;
    if ed.scope == Scope::Level {
        // The thing the tool would work on tinted by what it would do.
        let marked = if things { ed.moving().or(view.under) } else { None };
        let tint = match ed.tool {
            Tool::Remove => [230, 70, 60],
            Tool::Move => [255, 214, 90],
            _ => [255, 255, 255],
        };
        for (i, t) in ed.level.things.iter().enumerate() {
            // Hidden in the editor (its eye in the outliner).
            if ed.is_hidden(Item::Thing(i)) {
                continue;
            }
            let tint = (Some(i) == marked && ed.tool != Tool::Add).then_some(tint);
            // A piece only casting a shadow: its opaque cubes into the
            // sun's map alone, and shown as a ghost (not in GAME VIEW nor
            // the IDE's picture: the game's look); its glass casts nothing.
            if let Some(m) = t.model.as_deref().and_then(|m| figures.model(m)).filter(|_| t.shadow_only) {
                let start = cast.len();
                let (look, rot) = room::turn_of(t);
                crate::session::model_cubes(m, 0, IVec3::from(t.at).as_vec3(), look, rot, &mut cast, &mut Vec::new());
                if ghosts {
                    let tint = tint.unwrap_or([255, 255, 255]);
                    glass.extend(cast[start..].iter().map(|c| {
                        let [r, g, b] = [0, 1, 2].map(|k| ((c.color[k] as u16 + tint[k] as u16) / 2) as u8);
                        Instance { color: [r, g, b, 255], fx: [c.fx[0], GHOST, 0, c.fx[3]], ..*c }
                    }));
                }
                continue;
            }
            // A piece as the game draws it, at its very turn.
            if let Some(m) = t.model.as_deref().and_then(|m| figures.model(m)) {
                let start = instances.len();
                let (look, rot) = room::turn_of(t);
                crate::session::model_cubes(m, 0, IVec3::from(t.at).as_vec3(), look, rot, &mut instances, &mut glass);
                if let Some(tint) = tint {
                    for c in &mut instances[start..] {
                        let [r, g, b] = [0, 1, 2].map(|k| ((c.color[k] as u16 + tint[k] as u16) / 2) as u8);
                        c.color = [r, g, b, c.color[3]];
                    }
                }
                continue;
            }
            figures.cubes(&figures.key_in(t), IVec3::from(t.at), tint, 255, &mut instances);
        }
        // A piece hanging on the mouse, see-through where it would stand.
        if let (Some(Hanging::Thing(t)), Some(_)) = (&ed.pieces.hanging, ed.pieces.hang_spot)
            && let Some(m) = t.model.as_deref().and_then(|m| figures.model(m))
        {
            let (look, rot) = room::turn_of(t);
            let mut cubes = Vec::new();
            crate::session::model_cubes(m, 0, IVec3::from(t.at).as_vec3(), look, rot, &mut cubes, &mut glass);
            glass.extend(cubes.into_iter().map(|c| Instance { fx: [c.fx[0], 130, c.fx[2], c.fx[3]], ..c }));
        }
    }
    // Where the tool would work: a see-through cube there, or the held kind
    // of thing see-through where it would stand.
    let mut see_through = 0;
    let far = |i: &Instance| -(Vec3::from_slice(&i.pos_size[..3]) - eye).length_squared();
    // A figure: its parts as cubes round the one chosen (that one is the
    // world shown, being shaped), see-through while one is chosen, flush
    // ones one body; drawn far to near with the tool's marks after the
    // opaque ones.
    let mut through = Vec::new();
    let mut drawn = (Vec::new(), Arc::<crate::render::ModelBank>::default(), [0.0; 4]);
    let game_view = view.before_game.is_some() && ed.scope != Scope::Level;
    // (The figure as the game holds it is made only in GAME VIEW, and kept
    // until the model changes: made on every frame it cost a big figure a
    // frame's time and more.)
    if let (true, Some(figure), Some((lo, _))) = (game_view, &ed.figure, ed.figure_box())
        && let Some((model, bank)) = shown.game_view_model(ed, light)
    {
        // GAME VIEW: the figure as the game draws it: shaded as the game
        // shades it, from a model bank (outlined), posed while POSE is lit,
        // facing the way the game shows it.
        let s = model.size;
        let origin = (lo + IVec3::new(s.x / 2, s.y / 2, 0)).as_vec3();
        let look = view.face.look();
        let mut draws = Vec::new();
        crate::session::model_drawn(&model, origin, look, &figure.pose, &mut draws, &mut through);
        drawn = (draws, bank, view.outline);
    } else if let (Some(figure), Some((lo, _))) = (&ed.figure, ed.figure_box()) {
        let start = instances.len();
        // (A stroke from the figure whole dims nothing.)
        through = figure.cubes_seen(lo, seen, figure.selected.is_some() && !ed.rigging.painting_whole, &|c| ed.cut(c), &mut instances);
        // The chosen part turned at rest: its own voxels (not the world
        // shown, which is drawn unturned) where it stands, solid.
        if turned && let Some(b) = ed.shape() {
            let at = |c: IVec3| b.get(c) != 0 && !ed.cut(c);
            let cells: Vec<IVec3> = (0..b.voxels.len()).map(|k| b.at(k)).filter(|&c| at(c) && crate::mesh::DIRS.iter().any(|d| !at(c + *d))).collect();
            let mut own: Vec<Instance> = cells.iter().filter_map(|&c| b.colour_at(c).map(|k| (c, k))).map(|(c, k)| {
                let p = c.as_vec3() + 0.5;
                Instance { pos_size: [p.x, p.y, p.z, 1.0], color: [k[0], k[1], k[2], 255], fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] }
            }).collect();
            crate::render::hide_inner_faces(&mut own);
            place_turned(&mut own, rest);
            instances.extend(own);
        }
        // The faces between its own voxels left out, as the game's bank
        // leaves them (`vox::covered`): seen nearly edge on they won
        // single pixels along the edges. And those against the chosen
        // part's voxels (the world shown), across the seam.
        crate::render::hide_inner_faces(&mut instances[start..]);
        let world = &shown.world;
        crate::render::hide_faces_against(&mut instances[start..], |c| c.cmpge(IVec3::ZERO).all() && c.cmplt(world.size).all() && world.get(c) != 0);
    }
    // The pieces' glass joined too, as the game joins it: one body, not a
    // grid of every cube's faces blended over each other.
    through.extend(glass);
    crate::render::join_see_through(&mut through, eye - grid.as_vec3() / 2.0);
    // From here the tools' marks: in the grid shown (a turned part's own).
    let local_from = instances.len();
    // The selection (SELECT): its outline, thin and light, over all.
    if !game_view {
        let thin = 0.1;
        for (from, to) in ed.selection_edges_kept(&mut shown.outline.borrow_mut()) {
            let n = (from.distance(to) / (thin * 0.8)).ceil().max(1.0) as i32;
            for k in 0..=n {
                let p = from.lerp(to, k as f32 / n as f32);
                instances.push(Instance { pos_size: [p.x, p.y, p.z, thin], color: { let [r, g, b] = marks.gold(); [r, g, b, 255] }, fx: [if marks.light { 0 } else { 60 }, 255, 0, 0], shade: [0; 4], turn: [0; 4] });
            }
        }
    }
    if things {
        if let (Tool::Add, Some(at), Some(kind)) = (ed.tool, view.spot, &ed.held_kind) {
            let start = instances.len();
            figures.cubes(kind, at, None, 120, &mut instances);
            instances[start..].sort_by(|a, b| far(a).total_cmp(&far(b)));
            see_through = instances.len() - start;
        }
    } else if ed.forms() {
        // The shape set last, while fresh (what + and - act on): a quiet
        // outline along its box's edges.
        if let Some((lo, hi)) = ed.form_box().filter(|_| ed.forms.drawing.is_none()) {
            let (lo, hi) = (lo.as_vec3() * u, (hi + 1).as_vec3() * u);
            let thin = 0.12 * u;
            let start = instances.len();
            for a in 0..3 {
                let (b, c) = ((a + 1) % 3, (a + 2) % 3);
                for (sb, sc) in [(lo[b], lo[c]), (lo[b], hi[c]), (hi[b], lo[c]), (hi[b], hi[c])] {
                    let n = ((hi[a] - lo[a]) / (thin * 0.8)).ceil().max(1.0) as i32;
                    for k in 0..=n {
                        let mut p = Vec3::ZERO;
                        (p[a], p[b], p[c]) = (lo[a] + (hi[a] - lo[a]) * k as f32 / n as f32, sb, sc);
                        instances.push(Instance { pos_size: [p.x, p.y, p.z, thin], color: { let [r, g, b] = marks.line(); [r, g, b, 255] }, fx: [0, 120, 0, 0], shade: [0; 4], turn: [0; 4] });
                    }
                }
            }
            instances[start..].sort_by(|a, b| far(a).total_cmp(&far(b)));
            see_through = instances.len() - start;
        }
        // The shape BOX or SPHERE would make, see-through: its surface,
        // in the colour held (red to cut), far to near.
        // Before a press, the cell a footprint would begin at.
        let mut cells = ed.form_cells_now();
        if ed.forms.drawing.is_none()
            && let Some(hit) = view.hover
        {
            cells.push(if view.cut { hit.cell } else { hit.cell + if hit.normal == IVec3::ZERO { IVec3::Z } else { hit.normal } });
        }
        let all: crate::world::CellSet = cells.iter().copied().collect();
        let color = if view.cut || ed.forms.drawing.is_some_and(|d| d.cut) {
            [230, 70, 60, 255]
        } else if ed.scope == Scope::Level {
            [255, 214, 90, 255]
        } else {
            [ed.colour[0], ed.colour[1], ed.colour[2], 255]
        };
        let start = instances.len();
        for c in cells.iter().filter(|c| crate::mesh::DIRS.iter().any(|d| !all.contains(&(**c + *d)))) {
            let p = (c.as_vec3() + 0.5) * u;
            instances.push(Instance { pos_size: [p.x, p.y, p.z, u * 1.02], color, fx: [0, 110, 0, 0], shade: [0; 4], turn: [0; 4] });
        }
        if !cells.is_empty() {
            instances[start..].sort_by(|a, b| far(a).total_cmp(&far(b)));
            see_through = instances.len() - start;
        }
    } else if let Some(c) = view.hover.and_then(|hit| ed.target(hit)).filter(|_| !ed.among_pieces()) {
        let color = match ed.tool {
            Tool::Remove => [230, 70, 60, 255],
            Tool::Pick => [255, 255, 255, 255],
            _ => match ed.scope {
                Scope::Level => [255, 214, 90, 255],
                Scope::Block(_) | Scope::Frame(_) | Scope::Figure => [ed.colour[0], ed.colour[1], ed.colour[2], 255],
            },
        };
        let filled = match ed.scope {
            Scope::Level => ed.level.cell(c).block != 0,
            Scope::Block(_) | Scope::Frame(_) | Scope::Figure => ed.shape().is_some_and(|b| b.get(c) != 0),
        };
        let size = if filled { u * 1.04 } else { u };
        let p = (c.as_vec3() + 0.5) * u;
        instances.push(Instance { pos_size: [p.x, p.y, p.z, size], color, fx: [0, 110, 0, 0], shade: [0; 4], turn: [0; 4] });
        see_through = 1;
    }
    // A turned part's marks placed with it, the see-through ones far to
    // near again.
    if turned && instances.len() > local_from {
        place_turned(&mut instances[local_from..], rest);
        let n = instances.len();
        instances[n - see_through..].sort_by(|a, b| far(a).total_cmp(&far(b)));
    }
    if !through.is_empty() {
        through.extend(instances.drain(instances.len() - see_through..));
        far_to_near(&mut through, eye);
        see_through = through.len();
        instances.extend(through);
    }
    // The level round a model shaped in place, faint (`context.rs`): drawn
    // before everything else (`FrameData::behind`), so the model, drawn
    // after it, always lies over it in its own colors.
    let behind = if matches!(ed.scope, Scope::Frame(_) | Scope::Figure) && !game_view { shown.room_before(ed, figures, seen, eye, grid, &mut instances) } else { 0 };
    // The cubes only casting, after the opaque ones (`FrameData::cast_only`).
    let cast_only = cast.len();
    let at = instances.len() - see_through;
    instances.splice(at..at, cast);

    FrameData {
        view_proj,
        light_vp,
        remember: false,
        sun_dir: sun,
        sun_color: own.sun_color.unwrap_or(light.sun_color),
        depth_bias: 0.3 / depth_range,
        ambient: own.ambient.unwrap_or(light.ambient),
        shadow: own.shadow.unwrap_or(light.shadow),
        shadow_style: match light.shadow_style {
            ShadowStyle::Smooth => 0.0,
            ShadowStyle::Voxel => 1.0,
            ShadowStyle::VoxelSoft => 2.0,
        },
        sky: background.map_or(own.sky.unwrap_or(light.sky), |c| c.map(|v| v as f32 / 255.0)),
        reflection: light.reflection,
        reflection_horizon: light.reflection_horizon,
        metal_sweep: 0.0,
        time: 0.0,
        eye,
        haze: 0.0,
        haze_color: [0.0; 3],
        haze_start: 1.0,
        haze_depth: 1.0,
        lights: if view.lights && ed.scope == Scope::Level { lights(ed, orbit.target) } else { Vec::new() },
        glow: 0.0,
        shadow_map: SHADOW_MAP,
        tilt_shift: 0.0,
        screen_flash: 0.0,
        outline: drawn.2,
        fade: [0.0; 4],
        models: drawn.0,
        ui_models: Vec::new(),
        bank: drawn.1,
        instances,
        see_through,
        behind,
        cast_only,
        ui: Vec::new(),
        ui_view_proj: Mat4::IDENTITY,
    }
}

/// How opaque a piece only casting a shadow shows in the editor (of 255):
/// a ghost.
pub const GHOST: u8 = 70;

/// The size of the floor's tiles in a level without blocks (voxels).
pub const ROOM_TILE: i32 = 10;

/// The lights of the level's pieces as the game shows them at their
/// brightest: the `MAX_LIGHTS` nearest `middle` (the view's), as the game
/// takes those nearest its view's.
pub fn lights(ed: &Editor, middle: Vec3) -> Vec<crate::render::Light> {
    // (A piece hidden in the editor is dark with it.)
    let mut lights: Vec<crate::render::Light> = ed
        .level
        .things
        .iter()
        .enumerate()
        .filter(|&(i, _)| !ed.is_hidden(Item::Thing(i)))
        .map(|(_, t)| t)
        .flat_map(|t| room::light_spots(t, ed.size_of(t)).map(|(pos, l)| crate::render::Light { pos, radius: l.radius as f32, color: l.rgb() }))
        .collect();
    lights.sort_by(|a, b| a.pos.distance_squared(middle).total_cmp(&b.pos.distance_squared(middle)));
    lights.truncate(crate::render::MAX_LIGHTS);
    lights
}

/// The editor's shadow map: the size the game's "high" setting has.
pub const SHADOW_MAP: u32 = 4096;
