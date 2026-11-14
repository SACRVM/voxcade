//! The picture of a Lua game: the kits' things, the pieces a level set down
//! with their lights, the effects played once, `on_draw` and the HUD it
//! drew.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// `Game::draw`: the kits' things, the level's pieces with their lights, the
    /// effects, then `on_draw` and what it asked for.
    pub(super) fn draw_game(&self, out: &mut Drawing) {
        if let Some(kit) = self.kit() {
            kit.draw(out);
        }
        // The pieces the level set down, each its model where its pivot
        // stands, facing its way, and its light as bright as its state says
        // (off at 0). The script draws nothing for them.
        for (&id, p) in &self.stage.here.pieces {
            let Some(t) = self.stage.seen(id).filter(|_| !self.stage.here.dead.contains(&id)) else { continue };
            // Tipped too (`pitch`, `roll`): as `frame:model` would turn it
            // with the same three numbers.
            let tipped = p.pitch != 0.0 || p.roll != 0.0;
            if let Some(model) = &p.model {
                let facing = crate::session::facing(true, t.body);
                let turn = if tipped { facing * crate::session::turning(0.0, p.pitch, p.roll) } else { facing };
                // Its figure's parts posed as its state says (checked at
                // every tick's end: a mistake halts the game, and the
                // piece stands at rest meanwhile).
                let pose = match self.registry("voxcade.data").and_then(|d| Ok(d.raw_get::<Option<Table>>(id)?)) {
                    Ok(Some(state)) => self.piece_pose(id, &state).unwrap_or_else(|e| {
                        if self.halted.borrow().is_none() {
                            self.halt(e);
                        }
                        Vec::new()
                    }),
                    _ => Vec::new(),
                };
                // Only casting a shadow (`shadow_only`): a ceiling the
                // camera looks through.
                out.shadow_only(p.shadow_only);
                if pose.is_empty() {
                    out.model_turned(model, 0, out.body_origin(t.body), t.body.look, turn, None);
                } else {
                    let posed = crate::session::posing_alone(&pose);
                    out.model_posed(model, 0, out.body_origin(t.body), t.body.look, turn, &posed, None);
                }
                out.shadow_only(false);
            }
            // Its lights, all as bright as its state says (one brightness for
            // them all; a light of its own each would be a list of them).
            let brightness = self.stage.here.lights.get(&id).copied().unwrap_or(0.0);
            if brightness > 0.0 {
                let (sin, cos) = crate::sim::math::sin_cos((p.facing.bearing() as f32).to_radians());
                // A tipped piece's lights turn with it about its turn's
                // centre (the voxel at half its height).
                let size = p.model.as_ref().and_then(|m| out.model_of(m)).map(|m| m.size);
                let tip = size.filter(|_| tipped).map(|size| (crate::session::turning(p.facing.bearing() as f32, p.pitch, p.roll), crate::render::turn_centre(size)));
                for l in &p.lights {
                    // Its offset in the model's own voxels, turned with it (a
                    // bearing clockwise seen from above, the model's forward +y).
                    let o = l.at.as_vec3();
                    let turned = match tip {
                        Some((rot, about)) => rot * (o + Vec3::splat(0.5) - about) + about - Vec3::splat(0.5),
                        None => Vec3::new(o.x * cos + o.y * sin, -o.x * sin + o.y * cos, o.z),
                    };
                    let at = t.body.pos + Vec3::Z * 0.5 + turned;
                    out.light(Light { pos: at, radius: l.radius, color: l.color.map(|c| c * brightness) });
                }
            }
        }
        for e in &self.stage.effects {
            out.model(&e.model, (e.age as f32 * e.fps / 60.0) as usize, e.at.floor(), glam::IVec2::NEG_Y, None);
        }
        // Stopped by `on_tick`, the game is still drawn as it stands;
        // stopped by `on_draw`, as `on_draw` last showed it.
        if let Some(draw) = &self.draw
            && !self.draw_failed.get()
        {
            let (width, height) = out.hud_size();
            let mut rec = Frame {
                width,
                height,
                font: out.font(),
                alpha: out.alpha(),
                view_proj: out.view_proj(),
                snap: out.snaps(),
                viewing: out.viewing().cloned(),
                parts: self.stage.parts.clone(),
                known: self.stage.frames.clone(),
                needed: Some(out.needed()),
                ..Frame::default()
            };
            let lua = &self.sandbox.lua;
            let mut result = None;
            let scoped = lua.scope(|scope| {
                let world = scope.create_userdata_ref(&self.stage)?;
                lua.set_named_registry_value("voxcade.world", &world)?;
                lua.set_named_registry_value("voxcade.looking", true)?;
                // The picture is the handler's alone: there is none to keep.
                let frame = scope.create_userdata_ref_mut(&mut rec)?;
                result = Some(self.sandbox.call::<()>(draw, frame, "on_draw"));
                lua.unset_named_registry_value("voxcade.world")?;
                lua.set_named_registry_value("voxcade.looking", false)
            });
            match scoped.map_err(sandbox::script_error).and_then(|()| result.expect("draw ran")) {
                Ok(()) => *self.drawn.borrow_mut() = rec.cmds,
                Err(e) => {
                    self.draw_failed.set(true);
                    if self.halted.borrow().is_none() {
                        self.halt(e);
                        self.draw_halted.set(true);
                    }
                }
            }
        }
        for cmd in self.drawn.borrow().iter() {
            match cmd {
                Cmd::Model { file, thing, frame, turn, pose } => {
                    let Some(t) = self.stage.seen(*thing) else { continue };
                    let kind = self.stage.kinds.get(t.kind);
                    let frame = frame.unwrap_or_else(|| out.figure_frame(file, kind.map_or(&NO_FRAMES, |k| &k.frames), t.body));
                    let origin = out.body_origin(t.body);
                    // Its kind may face exactly its way and spin; the
                    // script turns it further.
                    let facing = crate::session::facing(kind.is_some_and(|k| k.free_turn), t.body);
                    let spin = crate::session::spinning(kind.map_or(0.0, |k| k.spin), self.stage.host.sim().tick as f32 + out.alpha());
                    let [round, pitch, roll] = *turn;
                    let turn = facing * spin * crate::session::turning(round, pitch, roll);
                    // A figure of parts swings as its kind says, the
                    // script's pose on top.
                    let tall = out.model_of(file).map_or(1.0, |m| m.size.z as f32);
                    let swung = crate::session::swinging(kind.map_or(&NO_SWING, |k| &k.swing), t.body, tall);
                    let posed = crate::session::posing(pose, swung);
                    out.model_posed(file, frame, origin, t.body.look, turn, &posed, (t.flash > 0).then_some(FLASH));
                }
                // On the grid when the camera snaps, gliding as the models
                // do otherwise.
                Cmd::Cube { at, color, glow } => out.cube(out.snapped(*at), *color, *glow, color[3] as f32 / 255.0),
                Cmd::Light { at, radius, color } => out.light(Light { pos: *at, radius: *radius, color: *color }),
                Cmd::Daylight(day) => out.daylight(*day),
                // (Its code names it as Lua writes a call, or what is out
                // of the game's own view would be missing.)
                Cmd::Camera(_) if !self.moves_camera => {
                    if self.halted.borrow().is_none() {
                        self.halt(anyhow::anyhow!("frame:camera: call it as frame:camera({{ ... }}) in the game's code, so the engine knows the game sets its camera"));
                    }
                }
                Cmd::Camera(cam) => out.camera(*cam),
                // The HUD's, drawn by `hud`.
                _ => {}
            }
        }
    }

    /// `Game::hud`: the HUD's part of what `on_draw` asked for, and the icons'
    /// voxels that burst.
    pub(super) fn draw_hud(&self, cfg: &UiConfig, alpha: f32, aspect: f32, font: Option<&DisplayFont>) -> Vec<Instance> {
        use crate::ui;
        let mut out = Vec::new();
        let layout = ui::Layout::of(cfg, aspect);
        let (top, depth, finish) = (layout.height, cfg.depth, ui::Finish::of(cfg));
        for cmd in self.drawn.borrow().iter() {
            match cmd {
                Cmd::Text { text, x, y, color, size } => {
                    ui::line(&mut out, text, &layout, *x, top - y - ui::line_height(*size, font), *size, *color, depth, font, finish);
                }
                Cmd::Box { x, y, width, height, color } => {
                    let (w, h) = (width.round() as i32, height.round() as i32);
                    for i in 0..w {
                        for j in 0..h {
                            for z in 0..depth {
                                ui::voxel(&mut out, x + i as f32, top - y - h as f32 + j as f32, z as f32, *color);
                            }
                        }
                    }
                }
                Cmd::Icon { name, x, y } => {
                    let Some(icon) = self.stage.hud.icons.get(name) else { continue };
                    for (c, r, color) in icon.voxels() {
                        for z in 0..depth {
                            ui::voxel(&mut out, x + c as f32, top - y - 1.0 - r as f32, z as f32, color);
                        }
                    }
                }
                Cmd::Drop { text, x, y, ticks, color, size } => {
                    ui::dropping(&mut out, text, &layout, Some(*x), top - y - ui::line_height(*size, font), *size, *color, depth, *ticks + alpha, font, finish);
                }
                Cmd::Menu(m) => ui::game_menu(&mut out, m, &layout, depth, &self.stage.hud.icons, self.stage.host.sim().tick),
                Cmd::Grid(g) => ui::game_grid(&mut out, g, &layout, depth, &self.stage.hud.icons),
                _ => {}
            }
        }
        for b in &self.stage.hud.bits {
            let at = crate::session::between(b.prev, b.pos, alpha);
            let size = (b.life / b.span).clamp(0.2, 1.0);
            out.push(Instance { pos_size: (at + 0.5).extend(size).to_array(), color: b.color, fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] });
        }
        out
    }
}

/// White over a thing that was just hit.
const FLASH: [u8; 4] = [255, 255, 255, 255];

/// A kind's frames where there is no kind: the model's own layout.
static NO_FRAMES: std::collections::BTreeMap<String, crate::sim::things::Frames> = std::collections::BTreeMap::new();

/// A kind's swing where there is no kind: none.
static NO_SWING: std::collections::BTreeMap<String, f32> = std::collections::BTreeMap::new();
