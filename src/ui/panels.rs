//! Panels over the game: a script's error, the console's prompt.

use super::*;

/// A problem over the screen (a script's error, a reload that failed): the
/// first lines of its message on a dark plate at the top, until mended.
pub fn error_panel(message: &str, cfg: &UiConfig, aspect: f32) -> Vec<Instance> {
    let layout = Layout::of(cfg, aspect);
    let columns = ((layout.width - 8.0) / 6.0).max(10.0) as usize;
    // The message's first line in red, wrapped or not.
    let mut lines: Vec<(String, bool)> = Vec::new();
    // Lua's own frames (`[C]: in ...`) say nothing about the game's code;
    // an x is a letter here, not the font's times sign.
    for (n, line) in message.lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("[C]:")).enumerate() {
        let line = line.trim_end().replace('\t', "  ").replace('x', "X");
        let indent = line.len() - line.trim_start().len();
        let mut now = " ".repeat(indent);
        for word in line.split_whitespace() {
            if now.trim().is_empty() || now.chars().count() + 1 + word.chars().count() <= columns {
                if !now.trim().is_empty() {
                    now.push(' ');
                }
                now.push_str(word);
            } else {
                lines.push((std::mem::replace(&mut now, format!("{}{word}", " ".repeat(indent + 2))), n == 0));
            }
            while now.chars().count() > columns {
                let cut: String = now.chars().take(columns).collect();
                now = format!("{}{}", " ".repeat(indent + 2), now.chars().skip(columns).collect::<String>());
                lines.push((cut, n == 0));
            }
        }
        lines.push((now, n == 0));
    }
    lines.truncate(((layout.height * 0.5) / 9.0) as usize);
    let mut out = Vec::new();
    let top = layout.height - 4.0;
    let bottom = top - lines.len() as f32 * 9.0 - 4.0;
    for y in (bottom as i32 - 2)..=(layout.height as i32) {
        for x in 0..layout.width as i32 {
            voxel(&mut out, x as f32, y as f32, -1.0, [20, 8, 10, 255]);
        }
    }
    for (n, (line, first)) in lines.iter().enumerate() {
        let color = if *first { RED } else { TEXT };
        text(&mut out, line, 4.0, top - 7.0 - n as f32 * 9.0, 1, color, cfg.depth);
    }
    out
}

/// The engine's own text (menus, pages, panels) with its lower left corner
/// at (x, y), `scale` voxels per pixel: in capitals, as it always was.
/// How many of the console's voxels its screen is high, in every game
/// alike: its letters as fine whatever the game's UI is.
pub(super) const PROMPT_ROWS: f32 = 300.0;

/// The console's lines: what was run and answered, then the one typed.
pub(super) const PROMPT_LINES: usize = 9;

/// The console's height in its voxels.
pub(super) const PROMPT_HEIGHT: f32 = 4.0 + PROMPT_LINES as f32 * 9.0 + 3.0;

/// How far the console pushes the game up, a share of the screen's height
/// (how far it is in, `Prompt::part`).
pub fn prompt_lift(prompt: &crate::prompt::Prompt) -> f32 {
    prompt.part * PROMPT_HEIGHT / PROMPT_ROWS
}

/// The console at the bottom of the screen, in place of the HUD, as far as
/// it slid in (`crate::prompt`; the game pushed up above it,
/// `prompt_lift`): what was run and answered, the line being typed below
/// it, as typed (lower case, the signs of code), the cursor blinking
/// (`blink`: shown now).
pub fn prompt_panel(prompt: &crate::prompt::Prompt, blink: bool, cfg: &UiConfig, aspect: f32) -> Vec<Instance> {
    use crate::prompt::Tone;
    let layout = Layout::of(cfg, aspect);
    // Drawn in voxels of its own, then made the size they are on screen.
    let scale = layout.height / PROMPT_ROWS;
    let width = layout.width / scale;
    let columns = ((width - 8.0) / 6.0).max(10.0) as usize;
    let wrap = |text: &str| -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return vec![String::new()];
        }
        chars.chunks(columns).map(|c| c.iter().collect()).collect()
    };
    // The typed line's end in view; above it what fits of what was shown.
    let mut typed = wrap(&format!("> {}{}", prompt.line, if blink && prompt.open { "_" } else { " " }));
    typed.drain(..typed.len().saturating_sub(PROMPT_LINES));
    let mut lines: Vec<(String, [u8; 4])> = Vec::new();
    for (text, tone) in prompt.shown.iter().rev() {
        if lines.len() + typed.len() >= PROMPT_LINES {
            break;
        }
        let color = match tone {
            Tone::Typed => [150, 180, 205, 255],
            Tone::Answer => TEXT,
            Tone::Mistake => RED,
        };
        for line in wrap(text).into_iter().rev() {
            lines.push((line, color));
        }
    }
    lines.truncate(PROMPT_LINES - typed.len());
    lines.reverse();
    lines.extend(typed.into_iter().map(|l| (l, [255, 255, 255, 255])));
    // As far as it slid in: the rest below the screen's edge.
    let below = (1.0 - prompt.part) * PROMPT_HEIGHT;
    // Its ground's front on the plane that fills the screen exactly (the
    // UI camera's), edge to edge with nothing of the game between; its
    // letters just before it.
    let front = cfg.depth as f32 / scale;
    let mut out = Vec::new();
    for y in 0..PROMPT_HEIGHT as i32 {
        for x in 0..=(width as i32) {
            voxel(&mut out, x as f32, y as f32 - below, front - 1.0, [10, 12, 18, 255]);
        }
    }
    let ground = out.len();
    // The newest line at the bottom, those before it above.
    for (n, (line, color)) in lines.iter().rev().enumerate() {
        written(&mut out, line, 4.0, 7.0 + n as f32 * 9.0 - below, 1, *color, cfg.depth);
    }
    for letter in &mut out[ground..] {
        letter.pos_size[2] += front;
    }
    for cube in &mut out {
        for c in &mut cube.pos_size {
            *c *= scale;
        }
    }
    out
}
