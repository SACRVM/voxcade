//! `screen`: the picture's effects.

use super::*;

/// `screen`: the picture's effects, from anywhere the game changes (they
/// are part of the game: replays see them).
pub struct ScreenView;

impl UserData for ScreenView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        // The HUD's height in UI voxels (c.ui.height), for icons that burst
        // near its bottom.
        f.add_field_function_get("height", |lua, _| look(lua, |h| Ok(h.hud.height)));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("screen"));
        // What was drawn on `screen` before is drawn on `on_draw`'s frame.
        m.add_meta_function(MetaMethod::Index, |_, (_, key): (AnyUserData, String)| -> mlua::Result<Value> {
            match key.as_str() {
                "text" | "cube" | "model" | "light" => Err(mlua::Error::runtime(format!("screen.{key} is frame.{key} now: function on_draw(frame) ... frame:{key}(...) ... end"))),
                "key" | "device" => Err(mlua::Error::runtime(format!("screen.{key} is controls.{key} now (or the action in braces in frame:text: \"PRESS {{jump}}\")"))),
                other => Err(mlua::Error::runtime(format!("screen has no {other} (screen:shake, screen:flash, screen:burst, screen.height; drawing is on_draw's frame)"))),
            }
        });
        m.add_function("shake", |lua, (_, amount): (AnyUserData, f32)| {
            change(lua, "screen:shake", |h| {
                let sim = h.host.sim_mut();
                sim.shake = (sim.shake + amount.clamp(0.0, 1.0)).min(1.0);
                Ok(())
            })
        });
        m.add_function("burst", |lua, (_, name, x, y): (AnyUserData, String, f32, f32)| {
            change(lua, "screen:burst", |h| h.hud.burst(&name, x, y).map_err(|e| mlua::Error::runtime(format!("screen:burst: {e}"))))
        });
        m.add_function("flash", |lua, (_, strength): (AnyUserData, f32)| {
            change(lua, "screen:flash", |h| {
                let sim = h.host.sim_mut();
                let flash = strength.clamp(0.0, 1.0) * sim.cfg.effects.screen_flash;
                sim.screen_flash = sim.screen_flash.max(flash).min(1.0);
                Ok(())
            })
        });
    }
}
