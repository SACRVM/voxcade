//! What `on_draw` asks for (`Cmd`), and the menus and grids it draws.

use super::*;

/// What `on_draw` asked for, done after it returns.
#[derive(Clone, Debug)]
pub enum Cmd {
    /// `turn`: degrees round, nose up, rolled right (`session::turning`).
    Model { file: String, thing: u32, frame: Option<usize>, turn: [f32; 3], pose: Vec<crate::session::PoseEntry> },
    Cube { at: Vec3, color: [u8; 4], glow: f32 },
    Light { at: Vec3, radius: f32, color: [f32; 3] },
    /// The HUD's, from its top left in UI voxels.
    Text { text: String, x: f32, y: f32, color: [u8; 4], size: i32 },
    Box { x: f32, y: f32, width: f32, height: f32, color: [u8; 4] },
    Icon { name: String, x: f32, y: f32 },
    Drop { text: String, x: f32, y: f32, ticks: f32, color: [u8; 4], size: i32 },
    /// This picture's daylight (`frame:daylight`).
    Daylight(crate::session::Daylight),
    /// This picture's camera (`frame:camera`).
    Camera(crate::session::PictureCamera),
    /// A menu in the engine's look (`frame:menu`).
    Menu(Menu),
    /// Slots in a grid, an inventory's (`frame:grid`).
    Grid(Grid),
}

/// `frame:menu`: a list in the engine's look, the game's to move.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    /// Its top left on the HUD (None: in the middle), and how wide.
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: Option<f32>,
    pub title: Option<String>,
    pub tabs: Vec<String>,
    /// The chosen tab and row, from 0.
    pub tab: usize,
    pub rows: Vec<MenuRow>,
    pub at: Option<usize>,
    /// How many rows show at most (scrolled to keep `at` in view).
    pub shown: usize,
    pub hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuRow {
    pub text: String,
    pub value: Option<String>,
    pub icon: Option<String>,
    pub dim: bool,
}

/// `frame:grid`: slots in rows, each an icon and a count.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub columns: usize,
    /// A slot's size in UI voxels.
    pub cell: f32,
    pub title: Option<String>,
    pub cells: Vec<GridCell>,
    pub at: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GridCell {
    pub icon: Option<String>,
    pub count: Option<i64>,
    pub dim: bool,
}

/// A table's keys checked against those a call takes.
pub(super) fn known_keys(t: &Table, keys: &[&str], what: &str) -> mlua::Result<()> {
    for pair in t.pairs::<Value, Value>() {
        let (k, _) = pair?;
        let ok = matches!(&k, Value::String(s) if keys.contains(&&*s.to_str()?));
        if !ok {
            let name = match &k {
                Value::String(s) => s.to_str()?.to_string(),
                other => format!("{other:?}"),
            };
            return Err(mlua::Error::runtime(format!("{what}: no '{name}' (it takes {})", keys.join(", "))));
        }
    }
    Ok(())
}

/// A chosen entry, counted from 1 by the script, from 0 here.
fn chosen(t: &Table, key: &str, count: usize, what: &str) -> mlua::Result<Option<usize>> {
    match t.raw_get::<Option<i64>>(key)? {
        None => Ok(None),
        Some(n) if n >= 1 && (n as usize) <= count => Ok(Some(n as usize - 1)),
        Some(n) => Err(mlua::Error::runtime(format!("{what}: {key} = {n}, but there are {count} (from 1)"))),
    }
}

/// `frame:menu{ ... }` read and checked, its texts' keys filled in.
pub(super) fn menu(lua: &Lua, t: &Table) -> mlua::Result<Menu> {
    const WHAT: &str = "frame:menu";
    known_keys(t, &["x", "y", "width", "title", "tabs", "tab", "rows", "at", "shown", "hint"], WHAT)?;
    let fill = |s: String| look(lua, |h| fill_keys(h.drawn_controls(), &s));
    let mut rows = Vec::new();
    if let Some(list) = t.raw_get::<Option<Table>>("rows")? {
        for (n, v) in list.sequence_values::<Value>().enumerate() {
            rows.push(match v? {
                Value::String(s) => MenuRow { text: fill(s.to_str()?.to_string())?, value: None, icon: None, dim: false },
                Value::Table(r) => {
                    known_keys(&r, &["text", "value", "icon", "dim"], &format!("{WHAT}: rows[{}]", n + 1))?;
                    let value = match r.raw_get::<Value>("value")? {
                        Value::Nil => None,
                        Value::String(s) => Some(fill(s.to_str()?.to_string())?),
                        Value::Integer(i) => Some(i.to_string()),
                        Value::Number(x) => Some(x.to_string()),
                        _ => return Err(mlua::Error::runtime(format!("{WHAT}: rows[{}].value is a text or a number", n + 1))),
                    };
                    MenuRow {
                        text: fill(r.raw_get::<Option<String>>("text")?.unwrap_or_default())?,
                        value,
                        icon: r.raw_get("icon")?,
                        dim: r.raw_get::<Option<bool>>("dim")?.unwrap_or(false),
                    }
                }
                _ => return Err(mlua::Error::runtime(format!("{WHAT}: rows[{}] is a text or {{ text =, value =, icon =, dim = }}", n + 1))),
            });
        }
    }
    let tabs: Vec<String> = match t.raw_get::<Option<Table>>("tabs")? {
        Some(list) => list.sequence_values::<String>().map(|s| s.and_then(fill)).collect::<mlua::Result<_>>()?,
        None => Vec::new(),
    };
    let shown = match t.raw_get::<Option<i64>>("shown")? {
        Some(n) if n >= 1 => n as usize,
        Some(n) => return Err(mlua::Error::runtime(format!("{WHAT}: shown = {n}, at least 1 row"))),
        None => rows.len().max(1),
    };
    Ok(Menu {
        x: t.raw_get("x")?,
        y: t.raw_get("y")?,
        width: t.raw_get("width")?,
        title: t.raw_get::<Option<String>>("title")?.map(fill).transpose()?,
        tab: chosen(t, "tab", tabs.len(), WHAT)?.unwrap_or(0),
        at: chosen(t, "at", rows.len(), WHAT)?,
        tabs,
        rows,
        shown,
        hint: t.raw_get::<Option<String>>("hint")?.map(fill).transpose()?,
    })
}

/// `frame:grid{ ... }` read and checked.
pub(super) fn grid(lua: &Lua, t: &Table) -> mlua::Result<Grid> {
    const WHAT: &str = "frame:grid";
    known_keys(t, &["x", "y", "columns", "cell", "title", "cells", "at"], WHAT)?;
    let mut cells = Vec::new();
    if let Some(list) = t.raw_get::<Option<Table>>("cells")? {
        for (n, v) in list.sequence_values::<Value>().enumerate() {
            cells.push(match v? {
                Value::Table(c) => {
                    known_keys(&c, &["icon", "count", "dim"], &format!("{WHAT}: cells[{}]", n + 1))?;
                    GridCell { icon: c.raw_get("icon")?, count: c.raw_get("count")?, dim: c.raw_get::<Option<bool>>("dim")?.unwrap_or(false) }
                }
                Value::Boolean(false) => GridCell::default(),
                _ => return Err(mlua::Error::runtime(format!("{WHAT}: cells[{}] is {{ icon =, count =, dim = }}, or false for an empty slot", n + 1))),
            });
        }
    }
    let columns = match t.raw_get::<Option<i64>>("columns")? {
        Some(n) if n >= 1 => n as usize,
        Some(n) => return Err(mlua::Error::runtime(format!("{WHAT}: columns = {n}, at least 1"))),
        None => cells.len().clamp(1, 8),
    };
    let cell = match t.raw_get::<Option<f32>>("cell")? {
        Some(c) if c >= 8.0 => c,
        Some(c) => return Err(mlua::Error::runtime(format!("{WHAT}: cell = {c}, at least 8 voxels"))),
        None => 18.0,
    };
    let title = t.raw_get::<Option<String>>("title")?.map(|s| look(lua, |h| fill_keys(h.drawn_controls(), &s))).transpose()?;
    Ok(Grid { x: t.raw_get("x")?, y: t.raw_get("y")?, columns, cell, title, at: chosen(t, "at", cells.len(), WHAT)?, cells })
}
