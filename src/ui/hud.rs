//! A game's menus in the engine's look, drawn from its HUD
//! (`frame:menu`, `frame:grid`).

use super::*;

// --- a game's menus (frame:menu, frame:grid) -----------------------------------------

/// The plate behind a game's menu, its edge, a slot's inside.
pub(super) const PLATE: [u8; 4] = [26, 22, 20, 255];
pub(super) const PLATE_EDGE: [u8; 4] = [92, 82, 68, 255];
pub(super) const SLOT: [u8; 4] = [38, 33, 30, 255];
/// A row's height, the room round the edge, in UI voxels.
pub(super) const ROW: f32 = 10.0;
pub(super) const PAD: f32 = 6.0;

/// A small text's width (5 a letter, 1 between).
pub(super) fn small_width(s: &str) -> f32 {
    (s.chars().count() as f32 * 6.0 - 1.0).max(0.0)
}

/// A plate from (x, y), its top left counted down from the top `top`, w by
/// h, behind what is drawn on it, with a quiet edge.
pub(super) fn plate(out: &mut Vec<Instance>, x: f32, y: f32, w: f32, h: f32, top: f32) {
    let (w, h) = (w.round() as i32, h.round() as i32);
    for i in 0..w {
        for j in 0..h {
            let edge = i == 0 || j == 0 || i == w - 1 || j == h - 1;
            voxel(out, x + i as f32, top - y - 1.0 - j as f32, -1.0, if edge { PLATE_EDGE } else { PLATE });
        }
    }
}

/// An icon of `c.icons` with its top left at (x, y) counted down from the
/// top `top`.
pub(super) fn hud_icon(out: &mut Vec<Instance>, icon: &crate::config::IconConfig, x: f32, y: f32, top: f32, depth: i32) {
    for (c, r, color) in icon.voxels() {
        for z in 0..depth {
            voxel(out, x + c as f32, top - y - 1.0 - r as f32, z as f32, color);
        }
    }
}

pub(super) fn icon_size(icon: &crate::config::IconConfig) -> (f32, f32) {
    (icon.rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f32, icon.rows.len() as f32)
}

/// A game's menu (`frame:menu`) in the engine's look: on a plate, its
/// title and tabs above, the chosen row yellow behind the engine's red
/// marker, values at the right, a long list scrolled to keep the chosen
/// row in the middle with a bar at the edge, the hint below.
pub fn game_menu(out: &mut Vec<Instance>, m: &crate::lua::world::Menu, layout: &Layout, depth: i32, icons: &std::collections::BTreeMap<String, crate::config::IconConfig>, tick: u64) {
    let top = layout.height;
    let n = m.rows.len();
    let shown = m.shown.min(n);
    let first = match m.at {
        Some(at) if n > shown => (at as i64 - shown as i64 / 2).clamp(0, (n - shown) as i64) as usize,
        _ => 0,
    };
    let icon_of = |row: &crate::lua::world::MenuRow| row.icon.as_ref().and_then(|i| icons.get(i));
    let row_width = |row: &crate::lua::world::MenuRow| {
        let icon = icon_of(row).map_or(0.0, |i| icon_size(i).0 + 3.0);
        10.0 + icon + small_width(&row.text) + row.value.as_deref().map_or(0.0, |v| 12.0 + small_width(v))
    };
    let mut inner = m.rows.iter().map(row_width).fold(0.0f32, f32::max);
    inner = inner.max(m.title.as_deref().map_or(0.0, small_width));
    inner = inner.max(m.tabs.iter().map(|t| small_width(t) + 10.0).sum::<f32>() - 10.0);
    inner = inner.max(m.hint.as_deref().map_or(0.0, small_width));
    let scrolled = n > shown;
    let width = m.width.unwrap_or(inner + 2.0 * PAD + if scrolled { 4.0 } else { 0.0 }).round();
    let height = PAD + if m.title.is_some() { 12.0 } else { 0.0 } + if m.tabs.is_empty() { 0.0 } else { 13.0 } + shown as f32 * ROW + if m.hint.is_some() { 13.0 } else { 0.0 } + PAD - 3.0;
    let x = m.x.unwrap_or(((layout.width - width) / 2.0).round());
    let y = m.y.unwrap_or(((layout.height - height) / 2.0).round());
    plate(out, x, y, width, height, top);
    let line = |cy: f32| top - cy - 7.0;
    let mut cy = y + PAD;
    if let Some(title) = &m.title {
        written(out, title, x + PAD, line(cy), 1, NUMBERS, depth);
        cy += 12.0;
    }
    if !m.tabs.is_empty() {
        let mut tx = x + PAD;
        for (i, tab) in m.tabs.iter().enumerate() {
            let w = small_width(tab);
            let chosen = i == m.tab;
            written(out, tab, tx, line(cy), 1, if chosen { NUMBERS } else { MUTED_LIGHT }, depth);
            if chosen {
                for k in 0..w as i32 {
                    voxel(out, tx + k as f32, line(cy) - 4.0, 0.0, NUMBERS);
                }
            }
            tx += w + 10.0;
        }
        cy += 13.0;
    }
    let rows_top = cy;
    for (i, row) in m.rows.iter().enumerate().skip(first).take(shown) {
        let chosen = m.at == Some(i);
        let color = if row.dim { MUTED_LIGHT } else if chosen { NUMBERS } else { TEXT };
        if chosen {
            let nudge = ((tick as f32 / 8.0).sin() * 1.5).round();
            text(out, ">", x + PAD + nudge, line(cy), 1, RED, depth);
        }
        let mut rx = x + PAD + 10.0;
        if let Some(icon) = icon_of(row) {
            let (w, h) = icon_size(icon);
            hud_icon(out, icon, rx, cy + ((7.0 - h) / 2.0).round(), top, depth);
            rx += w + 3.0;
        }
        written(out, &row.text, rx, line(cy), 1, color, depth);
        if let Some(value) = &row.value {
            let right = x + width - PAD - if scrolled { 4.0 } else { 0.0 };
            written(out, value, right - small_width(value), line(cy), 1, color, depth);
        }
        cy += ROW;
    }
    if scrolled {
        // A bar at the edge: where the rows shown lie among all.
        let (bx, track) = (x + width - PAD + 1.0, shown as f32 * ROW - 3.0);
        let (from, to) = (track * first as f32 / n as f32, track * (first + shown) as f32 / n as f32);
        for k in 0..track as i32 {
            let on = (k as f32) >= from.floor() && (k as f32) < to.ceil();
            voxel(out, bx, top - rows_top - 1.0 - k as f32, 0.0, if on { TEXT } else { PLATE_EDGE });
        }
    }
    if let Some(hint) = &m.hint {
        written(out, hint, x + PAD, line(cy + 3.0), 1, MUTED_LIGHT, depth);
    }
}

/// A game's slots (`frame:grid`): on a plate, each an icon and its count,
/// the chosen one edged in yellow, empty ones dark.
pub fn game_grid(out: &mut Vec<Instance>, g: &crate::lua::world::Grid, layout: &Layout, depth: i32, icons: &std::collections::BTreeMap<String, crate::config::IconConfig>) {
    let top = layout.height;
    let n = g.cells.len();
    let rows = n.div_ceil(g.columns).max(1);
    let (cell, gap) = (g.cell.round(), 2.0);
    let width = 2.0 * PAD + g.columns as f32 * (cell + gap) - gap;
    let head = if g.title.is_some() { 12.0 } else { 0.0 };
    let height = 2.0 * PAD + head + rows as f32 * (cell + gap) - gap;
    let x = g.x.unwrap_or(((layout.width - width) / 2.0).round());
    let y = g.y.unwrap_or(((layout.height - height) / 2.0).round());
    plate(out, x, y, width, height, top);
    if let Some(title) = &g.title {
        written(out, title, x + PAD, top - y - PAD - 7.0, 1, NUMBERS, depth);
    }
    for (i, c) in g.cells.iter().enumerate() {
        let cx = x + PAD + (i % g.columns) as f32 * (cell + gap);
        let cy = y + PAD + head + (i / g.columns) as f32 * (cell + gap);
        let chosen = g.at == Some(i);
        let edge = if chosen { NUMBERS } else if c.dim { PLATE } else { PLATE_EDGE };
        let size = cell as i32;
        for a in 0..size {
            for b in 0..size {
                let on_edge = a == 0 || b == 0 || a == size - 1 || b == size - 1;
                let (z, color) = if on_edge { (0.0, edge) } else { (-0.5, SLOT) };
                voxel(out, cx + a as f32, top - cy - 1.0 - b as f32, z, color);
            }
        }
        if let Some(icon) = c.icon.as_ref().and_then(|i| icons.get(i)) {
            let (w, h) = icon_size(icon);
            hud_icon(out, icon, cx + ((cell - w) / 2.0).floor(), cy + ((cell - h) / 2.0).floor(), top, depth);
        }
        if let Some(count) = c.count {
            let digits = count.to_string();
            written(out, &digits, cx + cell - 1.0 - small_width(&digits), top - cy - cell + 1.0, 1, if c.dim { MUTED_LIGHT } else { TEXT }, depth + 1);
        }
    }
}
