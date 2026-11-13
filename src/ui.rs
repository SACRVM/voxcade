//! The UI as a second voxel scene drawn over the world: a pixel font and
//! pixel-art icons, every pixel a voxel a couple deep, lit by their own sun
//! and seen in perspective from in front of the middle of the screen, so
//! their sides facing the middle show. It lives on its own grid (`[ui]
//! height` voxels from the bottom of the screen to the top, whatever the
//! resolution) and behaves like the game: a lost heart bursts into voxels,
//! changed digits hop, messages drop in letter by letter.

use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_at_mat4;
use glam::{IVec3, Mat4, Vec3};

use crate::config::UiConfig;
use std::rc::Rc;

use crate::controls::{Device, pad_label};
use crate::font::{DisplayFont, Glyph};
use crate::menu::{Item, Menu, Page};
use crate::render::{Instance, ModelDraw};
use crate::rng::Rng;
use crate::settings::{Settings, VOLUME_STEPS};
use crate::sim::DT;

/// Glyphs 5 wide and 7 tall, top row first.
const FONT: &[(char, [&str; 9])] = &[
    ('A', [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#", ".....", "....."]),
    ('B', ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.", ".....", "....."]),
    ('C', [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###.", ".....", "....."]),
    ('D', ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####.", ".....", "....."]),
    ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####", ".....", "....."]),
    ('F', ["#####", "#....", "#....", "####.", "#....", "#....", "#....", ".....", "....."]),
    ('G', [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####", ".....", "....."]),
    ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#", ".....", "....."]),
    ('I', [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('J', ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##..", ".....", "....."]),
    ('K', ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#", ".....", "....."]),
    ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####", ".....", "....."]),
    ('M', ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#", ".....", "....."]),
    ('N', ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", ".....", "....."]),
    ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.", ".....", "....."]),
    ('P', ["####.", "#...#", "#...#", "####.", "#....", "#....", "#....", ".....", "....."]),
    ('Q', [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#", ".....", "....."]),
    ('R', ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#", ".....", "....."]),
    ('S', [".####", "#....", "#....", ".###.", "....#", "....#", "####.", ".....", "....."]),
    ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..", ".....", "....."]),
    ('U', ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.", ".....", "....."]),
    ('V', ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#..", ".....", "....."]),
    ('W', ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#.", ".....", "....."]),
    ('X', ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#", ".....", "....."]),
    ('Y', ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#..", ".....", "....."]),
    ('Z', ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####", ".....", "....."]),
    ('0', [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.", ".....", "....."]),
    ('1', ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('2', [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####", ".....", "....."]),
    ('3', ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###.", ".....", "....."]),
    ('4', ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.", ".....", "....."]),
    ('5', ["#####", "#....", "####.", "....#", "....#", "#...#", ".###.", ".....", "....."]),
    ('6', ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###.", ".....", "....."]),
    ('7', ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...", ".....", "....."]),
    ('8', [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.", ".....", "....."]),
    ('9', [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##..", ".....", "....."]),
    ('.', [".....", ".....", ".....", ".....", ".....", ".##..", ".##..", ".....", "....."]),
    (',', [".....", ".....", ".....", ".....", ".##..", "..#..", ".#...", ".....", "....."]),
    (':', [".....", ".##..", ".##..", ".....", ".##..", ".##..", ".....", ".....", "....."]),
    ('!', ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#..", ".....", "....."]),
    ('?', [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#..", ".....", "....."]),
    ('-', [".....", ".....", ".....", "#####", ".....", ".....", ".....", ".....", "....."]),
    ('+', [".....", "..#..", "..#..", "#####", "..#..", "..#..", ".....", ".....", "....."]),
    ('/', [".....", "....#", "...#.", "..#..", ".#...", "#....", ".....", ".....", "....."]),
    ('%', ["##...", "##..#", "...#.", "..#..", ".#...", "#..##", "...##", ".....", "....."]),
    ('\'', ["..#..", "..#..", ".#...", ".....", ".....", ".....", ".....", ".....", "....."]),
    ('(', ["...#.", "..#..", ".#...", ".#...", ".#...", "..#..", "...#.", ".....", "....."]),
    (')', [".#...", "..#..", "...#.", "...#.", "...#.", "..#..", ".#...", ".....", "....."]),
    ('x', [".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", ".....", "....."]),
    ('>', [".#...", "..#..", "...#.", "....#", "...#.", "..#..", ".#...", ".....", "....."]),
    ('<', ["...#.", "..#..", ".#...", "#....", ".#...", "..#..", "...#.", ".....", "....."]),
    ('=', [".....", ".....", "#####", ".....", "#####", ".....", ".....", ".....", "....."]),
    ('_', [".....", ".....", ".....", ".....", ".....", ".....", "#####", ".....", "....."]),
    ('{', ["..##.", ".#...", ".#...", "#....", ".#...", ".#...", "..##.", ".....", "....."]),
    ('}', [".##..", "...#.", "...#.", "....#", "...#.", "...#.", ".##..", ".....", "....."]),
    ('~', [".....", ".....", ".#...", "#.#.#", "...#.", ".....", ".....", ".....", "....."]),
    ('|', ["..#..", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..", ".....", "....."]),
    ('\\', [".....", "#....", ".#...", "..#..", "...#.", "....#", ".....", ".....", "....."]),
    ('^', ["..#..", ".#.#.", "#...#", ".....", ".....", ".....", ".....", ".....", "....."]),
    ('[', [".###.", ".#...", ".#...", ".#...", ".#...", ".#...", ".###.", ".....", "....."]),
    (']', [".###.", "...#.", "...#.", "...#.", "...#.", "...#.", ".###.", ".....", "....."]),
    ('"', [".#.#.", ".#.#.", ".....", ".....", ".....", ".....", ".....", ".....", "....."]),
    (';', [".....", ".##..", ".##..", ".....", ".##..", "..#..", ".#...", ".....", "....."]),
    ('#', [".#.#.", ".#.#.", "#####", ".#.#.", "#####", ".#.#.", ".#.#.", ".....", "....."]),
    ('*', [".....", "#.#.#", ".###.", "#####", ".###.", "#.#.#", ".....", ".....", "....."]),
    ('a', [".....", ".....", ".###.", "....#", ".####", "#...#", ".####", ".....", "....."]),
    ('b', ["#....", "#....", "####.", "#...#", "#...#", "#...#", "####.", ".....", "....."]),
    ('c', [".....", ".....", ".####", "#....", "#....", "#....", ".####", ".....", "....."]),
    ('d', ["....#", "....#", ".####", "#...#", "#...#", "#...#", ".####", ".....", "....."]),
    ('e', [".....", ".....", ".###.", "#...#", "#####", "#....", ".###.", ".....", "....."]),
    ('f', ["..##.", ".#...", "####.", ".#...", ".#...", ".#...", ".#...", ".....", "....."]),
    ('g', [".....", ".....", ".####", "#...#", "#...#", "#...#", ".####", "....#", ".###."]),
    ('h', ["#....", "#....", "####.", "#...#", "#...#", "#...#", "#...#", ".....", "....."]),
    ('i', ["..#..", ".....", ".##..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('j', ["...#.", ".....", "..##.", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."]),
    ('k', ["#....", "#....", "#..#.", "#.#..", "##...", "#.#..", "#..#.", ".....", "....."]),
    ('l', [".##..", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('m', [".....", ".....", "##.#.", "#.#.#", "#.#.#", "#.#.#", "#.#.#", ".....", "....."]),
    ('n', [".....", ".....", "####.", "#...#", "#...#", "#...#", "#...#", ".....", "....."]),
    ('o', [".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###.", ".....", "....."]),
    ('p', [".....", ".....", "####.", "#...#", "#...#", "#...#", "####.", "#....", "#...."]),
    ('q', [".....", ".....", ".####", "#...#", "#...#", "#...#", ".####", "....#", "....#"]),
    ('r', [".....", ".....", "#.##.", "##..#", "#....", "#....", "#....", ".....", "....."]),
    ('s', [".....", ".....", ".####", "#....", ".###.", "....#", "####.", ".....", "....."]),
    ('t', [".#...", ".#...", "####.", ".#...", ".#...", ".#..#", "..##.", ".....", "....."]),
    ('u', [".....", ".....", "#...#", "#...#", "#...#", "#..##", ".##.#", ".....", "....."]),
    ('v', [".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#..", ".....", "....."]),
    ('w', [".....", ".....", "#...#", "#...#", "#.#.#", "#.#.#", ".#.#.", ".....", "....."]),
    ('y', [".....", ".....", "#...#", "#...#", "#...#", "#...#", ".####", "....#", ".###."]),
    ('z', [".....", ".....", "#####", "...#.", "..#..", ".#...", "#####", ".....", "....."]),
    ('ä', [".#.#.", ".....", ".###.", "....#", ".####", "#...#", ".####", ".....", "....."]),
    ('à', [".#...", "..#..", ".....", ".####", "#...#", "#..##", ".##.#", ".....", "....."]),
    ('â', ["..#..", ".#.#.", ".....", ".####", "#...#", "#..##", ".##.#", ".....", "....."]),
    ('é', ["...#.", "..#..", ".....", ".###.", "#####", "#....", ".###.", ".....", "....."]),
    ('è', [".#...", "..#..", ".....", ".###.", "#####", "#....", ".###.", ".....", "....."]),
    ('ê', ["..#..", ".#.#.", ".....", ".###.", "#####", "#....", ".###.", ".....", "....."]),
    ('ë', [".#.#.", ".....", ".###.", "#...#", "#####", "#....", ".###.", ".....", "....."]),
    ('ö', [".#.#.", ".....", ".###.", "#...#", "#...#", "#...#", ".###.", ".....", "....."]),
    ('ô', ["..#..", ".#.#.", ".....", ".###.", "#...#", "#...#", ".###.", ".....", "....."]),
    ('ü', [".#.#.", ".....", "#...#", "#...#", "#...#", "#..##", ".##.#", ".....", "....."]),
    ('û', ["..#..", ".#.#.", ".....", "#...#", "#...#", "#..##", ".##.#", ".....", "....."]),
    ('ù', [".#...", "..#..", ".....", "#...#", "#...#", "#..##", ".##.#", ".....", "....."]),
    ('î', ["..#..", ".#.#.", ".....", ".##..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('ï', [".#.#.", ".....", ".##..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('ç', [".....", ".....", ".####", "#....", "#....", "#....", ".####", "..#..", ".##.."]),
    ('Ç', [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###.", "..#..", ".##.."]),
    ('ß', [".##..", "#..#.", "#..#.", "#.#..", "#..#.", "#...#", "#.##.", ".....", "....."]),
    ('œ', [".....", ".....", ".#.#.", "#.#.#", "#.##.", "#.#..", ".#.##", ".....", "....."]),
    ('Œ', [".####", "#.#..", "#.#..", "#.###", "#.#..", "#.#..", ".####", ".....", "....."]),
    ('&', [".##..", "#..#.", "#.#..", ".#...", "#.#.#", "#..#.", ".##.#", ".....", "....."]),
    ('·', [".....", ".....", ".....", "..#..", ".....", ".....", ".....", ".....", "....."]),
    ('→', [".....", ".....", "..#..", "...#.", "#####", "...#.", "..#..", ".....", "....."]),
    ('←', [".....", ".....", "..#..", ".#...", "#####", ".#...", "..#..", ".....", "....."]),
    ('▲', [".....", ".....", "..#..", ".###.", "#####", ".....", ".....", ".....", "....."]),
    ('▼', [".....", ".....", "#####", ".###.", "..#..", ".....", ".....", ".....", "....."]),
    ('✓', [".....", ".....", "....#", "...#.", "#.#..", ".#...", ".....", ".....", "....."]),
    ('…', [".....", ".....", ".....", ".....", ".....", ".....", "#.#.#", ".....", "....."]),
    ('„', [".....", ".....", ".....", ".....", ".....", ".#.#.", ".#.#.", "#.#..", "....."]),
    ('«', [".....", ".....", "..#.#", ".#.#.", "#.#..", ".#.#.", "..#.#", ".....", "....."]),
    ('»', [".....", ".....", "#.#..", ".#.#.", "..#.#", ".#.#.", "#.#..", ".....", "....."]),
    ('•', [".....", ".....", ".###.", ".###.", ".###.", ".....", ".....", ".....", "....."]),
];

pub fn icon_color(c: char) -> [u8; 4] {
    match c {
        'R' => [214, 36, 44, 255],
        'W' => [255, 176, 170, 255],
        'K' => [96, 70, 64, 255],
        'S' => [172, 172, 160, 255],
        'G' => [84, 104, 56, 255],
        'L' => [128, 152, 84, 255],
        'H' => [34, 28, 26, 255],
        'B' => [196, 40, 36, 255],
        'E' => [24, 22, 20, 255],
        'T' => [214, 168, 126, 255],
        _ => [255, 0, 255, 255],
    }
}

pub const TEXT: [u8; 4] = [240, 230, 204, 255];
pub const NUMBERS: [u8; 4] = [255, 214, 90, 255];
pub const RED: [u8; 4] = [230, 58, 48, 255];

/// Where on the UI grid things sit: a corner and a size, in UI voxels.
pub struct Layout {
    pub width: f32,
    pub height: f32,
}

impl Layout {
    /// The UI grid of a screen `aspect` wide to high.
    pub fn of(cfg: &UiConfig, aspect: f32) -> Self {
        Self { width: (cfg.height * aspect).round(), height: cfg.height }
    }
}

pub const LETTER_DELAY: u64 = 3;
/// A view square on to the UI from in front of the middle of the screen,
/// with its vanishing point there (a one-point perspective): every voxel
/// shows the sides that face the middle, more the further out it sits. The
/// plane `front` (the fronts of text and icons `[ui] depth` deep) fills the
/// screen exactly; `slant` is how far sides show at the top and bottom
/// edges, in voxels per voxel of depth.
pub fn camera(layout: &Layout, slant: f32, front: f32) -> Mat4 {
    let (w, h) = (layout.width.max(1.0), layout.height.max(1.0));
    let slant = slant.max(0.01);
    let distance = h / 2.0 / slant;
    let middle = Vec3::new(w / 2.0, h / 2.0, front);
    let view = look_at_mat4(middle + Vec3::Z * distance, middle, Vec3::Y);
    perspective(2.0 * slant.atan(), w / h, 1.0, distance + 400.0) * view
}

/// An accented capital: its letter, at its full height, and the accent's
/// two rows above the capitals' top.
fn accented(ch: char) -> Option<(char, [&'static str; 2])> {
    const DIAERESIS: [&str; 2] = [".#.#.", "....."];
    const ACUTE: [&str; 2] = ["...#.", "..#.."];
    const GRAVE: [&str; 2] = [".#...", "..#.."];
    const CIRCUMFLEX: [&str; 2] = ["..#..", ".#.#."];
    Some(match ch {
        'Ä' => ('A', DIAERESIS),
        'À' => ('A', GRAVE),
        'Â' => ('A', CIRCUMFLEX),
        'É' => ('E', ACUTE),
        'È' => ('E', GRAVE),
        'Ê' => ('E', CIRCUMFLEX),
        'Ë' => ('E', DIAERESIS),
        'Î' => ('I', CIRCUMFLEX),
        'Ï' => ('I', DIAERESIS),
        'Ö' => ('O', DIAERESIS),
        'Ô' => ('O', CIRCUMFLEX),
        'Ü' => ('U', DIAERESIS),
        'Û' => ('U', CIRCUMFLEX),
        'Ù' => ('U', GRAVE),
        _ => return None,
    })
}

/// A character's glyph: rows from the top, the bottom two below the line
/// (a g's tail); signs that look alike share one; an accented capital its
/// letter's (the accent: `accented`).
fn glyph(ch: char) -> Option<&'static [&'static str; 9]> {
    let ch = accented(ch).map_or(ch, |(letter, _)| letter);
    let ch = match ch {
        '–' | '—' | '−' => '-',
        '’' | '‘' | '´' | '`' => '\'',
        '“' | '”' => '"',
        '×' => 'x',
        other => other,
    };
    FONT.iter().find(|(c, _)| *c == ch).map(|(_, g)| g)
}

/// A text as the engine writes its own (menus, the logo): in capitals as
/// always, but for the small x of "x3".
pub fn engine_case(s: &str) -> String {
    s.chars().flat_map(|c| if c == 'x' { vec![c] } else { c.to_uppercase().collect() }).collect()
}

pub fn text_width(s: &str, scale: i32) -> f32 {
    (s.chars().count() as f32 * 6.0 - 1.0) * scale as f32
}

/// A line's width at `scale`: in the display font from 2 up when the game
/// has one, else in the pixel font.
pub fn line_width(s: &str, scale: i32, font: Option<&DisplayFont>) -> f32 {
    match font {
        Some(font) if scale >= 2 => font.width(s, 8 * scale as u32) as f32,
        _ => text_width(s, scale),
    }
}

/// A line's height at `scale`: the display font's from 2 up, else the
/// pixel font's seven rows.
pub fn line_height(scale: i32, font: Option<&DisplayFont>) -> f32 {
    if font.is_some() && scale >= 2 { 8.0 * scale as f32 } else { 7.0 * scale as f32 }
}

/// A line standing still at `x`, `y` (its bottom left): big letters in the
/// display font from scale 2 up, else the pixel font.
#[allow(clippy::too_many_arguments)]
pub fn line(out: &mut Vec<Instance>, s: &str, layout: &Layout, x: f32, y: f32, scale: i32, color: [u8; 4], depth: i32, font: Option<&DisplayFont>, finish: Finish) {
    match font {
        // Long after it dropped in: still.
        Some(_) if scale >= 2 => dropping(out, s, layout, Some(x), y, scale, color, depth, 1.0e6, font, finish),
        _ => written(out, s, x, y, scale, color, depth),
    }
}

pub fn voxel(out: &mut Vec<Instance>, x: f32, y: f32, z: f32, color: [u8; 4]) {
    out.push(Instance { pos_size: [x + 0.5, y + 0.5, z + 0.5, 1.0], color, fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] });
}

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
const PROMPT_ROWS: f32 = 300.0;

/// The console's lines: what was run and answered, then the one typed.
const PROMPT_LINES: usize = 9;

/// The console's height in its voxels.
const PROMPT_HEIGHT: f32 = 4.0 + PROMPT_LINES as f32 * 9.0 + 3.0;

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

pub fn text(out: &mut Vec<Instance>, s: &str, x: f32, y: f32, scale: i32, color: [u8; 4], depth: i32) {
    written(out, &engine_case(s), x, y, scale, color, depth);
}

/// A game's text as it is written, lower case and accents too; the line
/// at `y`, a g's tail below it.
pub fn written(out: &mut Vec<Instance>, s: &str, x: f32, y: f32, scale: i32, color: [u8; 4], depth: i32) {
    for (i, ch) in s.chars().enumerate() {
        let Some(g) = glyph(ch) else { continue };
        letter(out, g, accented(ch).map(|(_, a)| a), x + i as f32 * 6.0 * scale as f32, y, scale, color, depth);
    }
}

#[allow(clippy::too_many_arguments)]
fn letter(out: &mut Vec<Instance>, g: &[&str; 9], accent: Option<[&str; 2]>, x: f32, y: f32, scale: i32, color: [u8; 4], depth: i32) {
    // Rows -2 and -1: an accent over a capital.
    let on = |c: i32, r: i32| {
        let row = match r {
            -2 | -1 => accent.map(|a| a[(r + 2) as usize]),
            0..9 => Some(g[r as usize]),
            _ => None,
        };
        (0..5).contains(&c) && row.is_some_and(|row| row.as_bytes()[c as usize] == b'#')
    };
    let d = depth * scale.min(2);
    for r in -2..9 {
        for c in 0..5 {
            if !on(c, r) {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let px = x + (c * scale + sx) as f32;
                    let py = y + ((6 - r) * scale + sy) as f32;
                    for z in 0..d {
                        voxel(out, px, py, z as f32, color);
                    }
                }
            }
        }
    }
}

pub fn icon(out: &mut Vec<Instance>, art: &[&str], x: f32, y: f32, depth: i32) {
    let rows = art.len() as f32;
    for (r, row) in art.iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            if ch == '.' {
                continue;
            }
            for z in 0..depth {
                voxel(out, x + c as f32, y + rows - 1.0 - r as f32, z as f32, icon_color(ch));
            }
        }
    }
}

/// Centred text whose letters fall in one after another from above and
/// bounce once; `t` is ticks since it started (negative: not yet).
#[allow(clippy::too_many_arguments)]
pub fn dropping(out: &mut Vec<Instance>, s: &str, layout: &Layout, x: Option<f32>, y: f32, scale: i32, color: [u8; 4], depth: i32, t: f32, font: Option<&DisplayFont>, finish: Finish) {
    // Big lines in the display font, when the game has one.
    if let (Some(font), true) = (font, scale >= 2) {
        let height = 8 * scale as u32;
        let gap = DisplayFont::gap(height);
        let x0 = x.unwrap_or_else(|| ((layout.width - font.width(s, height) as f32) / 2.0).round());
        let mut x = x0;
        for (i, ch) in s.chars().enumerate() {
            let g = font.glyph(ch, height);
            if let Some(lift) = drop_lift(t - (i as u64 * LETTER_DELAY) as f32, 60.0, 0.25) {
                extrude(out, &g, x, y + lift, color, depth * 2, Finish { seed: i as u32 * 7 + ch as u32, ..finish });
            }
            x += (g.width + gap) as f32;
        }
        return;
    }
    let width = text_width(s, scale);
    let x0 = x.unwrap_or_else(|| ((layout.width - width) / 2.0).round());
    for (i, ch) in s.chars().enumerate() {
        let Some(g) = glyph(ch) else { continue };
        let Some(lift) = drop_lift(t - (i as u64 * LETTER_DELAY) as f32, 60.0, 0.25) else { continue };
        letter(out, g, accented(ch).map(|(_, a)| a), x0 + i as f32 * 6.0 * scale as f32, y + lift, scale, color, depth);
    }
}

// --- a game's menus (frame:menu, frame:grid) -----------------------------------------

/// The plate behind a game's menu, its edge, a slot's inside.
const PLATE: [u8; 4] = [26, 22, 20, 255];
const PLATE_EDGE: [u8; 4] = [92, 82, 68, 255];
const SLOT: [u8; 4] = [38, 33, 30, 255];
/// A row's height, the room round the edge, in UI voxels.
const ROW: f32 = 10.0;
const PAD: f32 = 6.0;

/// A small text's width (5 a letter, 1 between).
fn small_width(s: &str) -> f32 {
    (s.chars().count() as f32 * 6.0 - 1.0).max(0.0)
}

/// A plate from (x, y), its top left counted down from the top `top`, w by
/// h, behind what is drawn on it, with a quiet edge.
fn plate(out: &mut Vec<Instance>, x: f32, y: f32, w: f32, h: f32, top: f32) {
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
fn hud_icon(out: &mut Vec<Instance>, icon: &crate::config::IconConfig, x: f32, y: f32, top: f32, depth: i32) {
    for (c, r, color) in icon.voxels() {
        for z in 0..depth {
            voxel(out, x + c as f32, top - y - 1.0 - r as f32, z as f32, color);
        }
    }
}

fn icon_size(icon: &crate::config::IconConfig) -> (f32, f32) {
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

// --- menus ---------------------------------------------------------------------------

const DIM: [u8; 4] = [8, 6, 6, 255];
const SLIDER_EMPTY: [u8; 4] = [70, 62, 54, 255];
/// Quiet text: hints and column heads, dark over the sand of the title,
/// light over the darkened pages.
const MUTED_DARK: [u8; 4] = [70, 62, 54, 255];
const MUTED_LIGHT: [u8; 4] = [150, 140, 118, 255];
/// The title's rows: idle, chosen, the marker; over a dark scene light.
const TITLE_ROWS: ([u8; 4], [u8; 4], [u8; 4]) = ([58, 48, 38, 255], [214, 34, 32, 255], [58, 48, 38, 255]);
const TITLE_ROWS_LIGHT: ([u8; 4], [u8; 4], [u8; 4]) = (TEXT, [214, 34, 32, 255], TEXT);
const DRIP: [u8; 4] = [150, 14, 16, 255];

/// What the menus show besides their pages: the logo (lines and colours,
/// the one that drips), the songs' titles, the high score.
pub struct MenuView<'a> {
    pub menu: &'a Menu,
    pub settings: &'a Settings,
    pub logo: &'a [(String, [u8; 4])],
    pub drip: Option<usize>,
    pub songs: &'a [String],
    /// The game keeps a score (and so a high score).
    pub scored: bool,
    /// The big letters' font, and the logo lines' heights in voxels.
    pub font: Option<&'a DisplayFont>,
    pub logo_heights: &'a [u32],
    /// Cracks per letter in each logo line.
    pub logo_cracks: &'a [f32],
    /// Where each logo line's letters lie in the model bank, built once
    /// (`logo_letters`); empty: they are built here every frame.
    pub logo_bank: &'a [Vec<std::ops::Range<u32>>],
    /// The game's own logo, a model (`c.title_screen.logo_model`), in
    /// place of the lines when it has one.
    pub logo_model: Option<&'a LogoModel>,
    /// How far the logo's letters bob once landed (`c.title_screen.logo_bob`).
    pub logo_bob: f32,
    /// The title's scene is dark: its rows and hint in light letters.
    pub light_menu: bool,
}

/// The menu's voxels for this frame, the logo's letters drawn from the
/// model bank, and the camera that shows them: on the
/// title the logo and the main items over the scene, the other pages over
/// a darkened screen.
pub fn menu(view: &MenuView, cfg: &UiConfig, alpha: f32, aspect: f32) -> (Vec<Instance>, Vec<ModelDraw>, Mat4) {
    let layout = Layout { width: (cfg.height * aspect).round(), height: cfg.height };
    let camera = camera(&layout, cfg.slant, cfg.depth as f32);
    let finish = Finish::of(cfg);
    let mut out = Vec::new();
    let mut letters = Vec::new();
    let menu = view.menu;
    let Some(page) = menu.page() else { return (out, letters, camera) };
    let depth = cfg.depth;
    let t = (menu.tick - menu.opened) as f32 + alpha;
    let now = menu.tick as f32 + alpha;
    let top = layout.height;
    // The engine's words in the player's language (`c.languages`).
    let lang = view.settings.language();
    let lang = lang.as_str();
    let say = |en: &'static str| crate::lang::say(lang, en);

    if page == Page::Title {
        logo(&mut out, &mut letters, view, &layout, now, finish);
        if view.scored {
            let score = format!("HI-SCORE {:06}", view.settings.best());
            text(&mut out, &score, layout.width - 3.0 - text_width(&score, 1), top - 10.0, 1, NUMBERS, depth);
        }
    } else {
        dim(&mut out, &layout, 0.8);
        // A guest's own menu stops nothing: no PAUSED over it.
        let guest = page == Page::Pause && menu.net.as_ref().is_some_and(|n| !n.hosting);
        let heading = if guest { "MENU" } else { page.heading() };
        dropping(&mut out, say(heading), &layout, None, top - 38.0, 3, TEXT, depth, t, view.font, finish);
    }

    match page {
        Page::Title => {
            // Big items low on the left, clear of what lies in the scene:
            // the last always where it was, a fourth (CONTINUE) above.
            let items = menu.items(page, view.settings);
            let first = 28.0 + (items.len() as f32 - 1.0) * 17.0;
            for (i, item) in items.iter().enumerate() {
                row(&mut out, view, i, &item.label_in(lang), 24.0, first - i as f32 * 17.0, 2, depth, now);
            }
        }
        Page::Pause | Page::Network => {
            let items = menu.items(page, view.settings);
            // Closer where a network game's pause page has many.
            let apart = if items.len() > 6 { 13.0 } else { 17.0 };
            for (i, item) in items.iter().enumerate() {
                let label = match item {
                    Item::Drop(m) => {
                        let name = menu.net.iter().flat_map(|n| &n.drops).find(|(d, _)| d == m).map_or(String::new(), |(_, n)| n.to_uppercase());
                        crate::lang::say_with(lang, "DROP {}", &[&name])
                    }
                    Item::LetIn(p) => {
                        let line = menu.net.iter().flat_map(|n| &n.knocks).find(|(k, _)| k == p).map_or(String::new(), |(_, l)| l.clone());
                        crate::lang::say_with(lang, "LET IN {}", &[&line])
                    }
                    other => other.label_in(lang),
                };
                let scale = if text_width(&label, 2) > layout.width - 8.0 { 1 } else { 2 };
                let x = ((layout.width - text_width(&label, scale)) / 2.0).round();
                row(&mut out, view, i, &label, x, top - 70.0 - i as f32 * apart, scale, depth, now);
            }
            // Where the game is found, small at the top left (one line above
            // the heading's), who plays at the top right, how to let others
            // in under the items.
            if page == Page::Pause
                && let Some(net) = &menu.net
            {
                for (i, line) in net.lines.iter().enumerate() {
                    text(&mut out, line, 6.0, top - 10.0 - i as f32 * 9.0, 1, MUTED_LIGHT, depth);
                }
                for (i, line) in net.who.iter().enumerate() {
                    text(&mut out, line, layout.width - 6.0 - text_width(line, 1), top - 10.0 - i as f32 * 9.0, 1, MUTED_LIGHT, depth);
                }
                for (i, line) in net.help.iter().rev().enumerate() {
                    centred(&mut out, &layout, line, 14.0 + i as f32 * 9.0, 1, MUTED_LIGHT, depth);
                }
            }
        }
        // The games found on the LAN, another game's greyed; typing an
        // address instead.
        Page::Find => {
            let items = menu.items(page, view.settings);
            let x0 = (layout.width / 2.0 - 120.0).round();
            for (i, &item) in items.iter().enumerate() {
                let y = top - 66.0 - i as f32 * 13.0;
                match item {
                    Item::Found(n) if !menu.found[n].same => text(&mut out, &menu.found[n].line, x0, y, 1, MUTED_LIGHT, depth),
                    Item::Found(n) => row(&mut out, view, i, &menu.found[n].line, x0, y, 1, depth, now),
                    other => row(&mut out, view, i, &other.label_in(lang), x0, y, 1, depth, now),
                }
            }
            if menu.found.is_empty() {
                centred(&mut out, &layout, say("SEARCHING THE NETWORK ..."), top - 52.0, 1, MUTED_LIGHT, depth);
                centred(&mut out, &layout, say("NONE FOUND? TYPE THE HOST'S ADDRESS"), 27.0, 1, MUTED_LIGHT, depth);
                centred(&mut out, &layout, say("(A FIREWALL MAY HIDE IT)"), 18.0, 1, MUTED_LIGHT, depth);
            }
        }
        // The host's address being typed, big, the cursor blinking.
        Page::Join => {
            centred(&mut out, &layout, say("THE HOST'S ADDRESS (ITS PAUSE PAGE SAYS IT)"), top - 62.0, 1, MUTED_LIGHT, depth);
            let shown = format!("{}{}", menu.address, if (menu.tick / 20).is_multiple_of(2) { "_" } else { " " });
            let scale = if text_width(&shown, 3) > layout.width - 8.0 { 2 } else { 3 };
            centred(&mut out, &layout, &shown, top - 96.0, scale, NUMBERS, depth);
        }
        // A game's code being typed: six places, the cursor blinking in the
        // next.
        Page::Code => {
            centred(&mut out, &layout, say("THE GAME'S CODE (THE HOST SENDS IT)"), top - 62.0, 1, MUTED_LIGHT, depth);
            let blink = (menu.tick / 20).is_multiple_of(2);
            let shown: String = (0..matchmaker_protocol::LENGTH)
                .map(|i| match menu.code.chars().nth(i) {
                    Some(c) => c,
                    None if i == menu.code.len() && blink => '_',
                    None => '.',
                })
                .collect();
            centred(&mut out, &layout, &shown, top - 96.0, 3, NUMBERS, depth);
        }
        Page::Joining => {
            let lines = menu.net.as_ref().map_or(&[][..], |n| &n.lines[..]);
            for (i, line) in lines.iter().enumerate() {
                centred(&mut out, &layout, line, top - 70.0 - i as f32 * 12.0, 1, if i == 0 { NUMBERS } else { TEXT }, depth);
            }
        }
        // The settings' own page as the pause page: big rows in the middle,
        // LANGUAGE with its value (small where it would not fit).
        Page::Settings => {
            for (i, &item) in menu.items(page, view.settings).iter().enumerate() {
                let mut label = item.label_in(lang);
                if item == Item::Language {
                    label = format!("{label} < {} >", crate::lang::name(lang));
                }
                let scale = if text_width(&label, 2) > layout.width - 8.0 { 1 } else { 2 };
                let x = ((layout.width - text_width(&label, scale)) / 2.0).round();
                row(&mut out, view, i, &label, x, top - 70.0 - i as f32 * 17.0, scale, depth, now);
            }
        }
        Page::Audio | Page::Video => {
            let x0 = (layout.width / 2.0 - 100.0).round();
            let items = menu.items(page, view.settings);
            // A few rows each, roomy under the heading.
            for (i, &item) in items.iter().enumerate() {
                let y = top - 58.0 - i as f32 * 13.0;
                row(&mut out, view, i, &item.label_in(lang), x0, y, 1, depth, now);
                let selected = menu.cursor() == i;
                setting(&mut out, view, item, x0 + 120.0, y, selected, depth);
            }
        }
        Page::Controls => {
            let x0 = (layout.width / 2.0 - 110.0).round();
            let (kx, px) = (x0 + 70.0, x0 + 160.0);
            text(&mut out, say("KEYBOARD"), kx, top - 56.0, 1, MUTED_LIGHT, depth);
            text(&mut out, say("GAMEPAD"), px, top - 56.0, 1, MUTED_LIGHT, depth);
            let items = menu.items(page, view.settings);
            // Down to above the hint at the bottom, closer where there are many.
            let apart = ((top - 70.0 - 30.0) / (items.len().max(2) - 1) as f32).floor().min(12.0);
            for (i, &item) in items.iter().enumerate() {
                let y = top - 70.0 - i as f32 * apart;
                row(&mut out, view, i, &item.label_in(lang), x0, y, 1, depth, now);
                setting(&mut out, view, item, kx, y, menu.cursor() == i, depth);
                let Item::Bind(action) = item else { continue };
                if menu.rebinding == Some(action) {
                    if (menu.tick / 15).is_multiple_of(2) {
                        text(&mut out, say("PRESS A KEY OR BUTTON"), kx, y, 1, NUMBERS, depth);
                    }
                    continue;
                }
                let keys = view.settings.keys.get(action).first().map(|k| view.settings.key_label(k)).unwrap_or_default();
                let pads = view.settings.pad.get(action).first().map(|b| pad_label(b, menu.pad_style)).unwrap_or_default();
                text(&mut out, &keys, kx, y, 1, TEXT, depth);
                text(&mut out, &pads, px, y, 1, TEXT, depth);
            }
            let y = top - 70.0 - items.len() as f32 * apart - 4.0;
            text(&mut out, say("MOVE: ARROWS OR WASD, LEFT STICK OR D-PAD"), x0, y, 1, MUTED_LIGHT, depth);
        }
        Page::Players => {
            // A heading for each seat with its three rows under it, then
            // SWAP, RUMBLE and BACK; scrolled so the chosen row stays in view.
            let x0 = (layout.width / 2.0 - 110.0).round();
            let vx = x0 + 100.0;
            let items = menu.items(page, view.settings);
            let mut rows = Vec::with_capacity(items.len());
            let mut heads = Vec::new();
            let mut y = top - 54.0;
            for &item in &items {
                match item {
                    Item::SeatKeys(n) => {
                        y -= 3.0;
                        heads.push((n, y));
                        y -= 10.0;
                    }
                    Item::Swap => y -= 4.0,
                    _ => {}
                }
                rows.push(y);
                y -= 10.0;
            }
            let lowest = 16.0;
            let shift = (lowest - rows.get(menu.cursor()).copied().unwrap_or(lowest)).max(0.0);
            let shown = |y: f32| (lowest..=top - 50.0).contains(&y);
            for (n, y) in heads {
                let y = y + shift;
                if !shown(y) {
                    continue;
                }
                let head = crate::lang::say_with(lang, "PLAYER {}", &[&n.to_string()]);
                text(&mut out, &head, x0, y, 1, NUMBERS, depth);
                // A seat nobody sits on: START takes it.
                if !view.menu.seats.get(n - 1).is_some_and(|s| s.taken) {
                    text(&mut out, say("PRESS START"), vx, y, 1, MUTED_LIGHT, depth);
                }
            }
            for (i, (&item, &y)) in items.iter().zip(&rows).enumerate() {
                let y = y + shift;
                if !shown(y) {
                    continue;
                }
                let indent = if matches!(item, Item::SeatKeys(_) | Item::SeatPad(_) | Item::SeatControls(_)) { 10.0 } else { 0.0 };
                row(&mut out, view, i, &item.label_in(lang), x0 + indent, y, 1, depth, now);
                setting(&mut out, view, item, vx, y, menu.cursor() == i, depth);
                let value = match item {
                    Item::SeatKeys(n) => say(menu.seats.get(n - 1).map_or(crate::seating::Keys::Nothing, |s| s.keyboard).label()).to_string(),
                    Item::SeatPad(n) => match menu.seats.get(n - 1).map(|s| s.pad.as_str()) {
                        Some(name) if !name.is_empty() => name.to_uppercase().chars().take(24).collect(),
                        _ => say("NONE").to_string(),
                    },
                    _ => continue,
                };
                let selected = menu.cursor() == i;
                let shown = if selected { format!("< {value} >") } else { value };
                text(&mut out, &shown, vx, y, 1, if selected { NUMBERS } else { TEXT }, depth);
            }
        }
        Page::PlayerControls(seat) => {
            let part = menu.seats.get(seat - 1).map_or(crate::seating::Keys::Nothing, |s| s.keyboard);
            let head = crate::lang::say_with(lang, "PLAYER {}", &[&seat.to_string()]);
            centred(&mut out, &layout, &head, top - 46.0, 1, NUMBERS, depth);
            let x0 = (layout.width / 2.0 - 110.0).round();
            let (kx, px) = (x0 + 70.0, x0 + 160.0);
            text(&mut out, say("KEYBOARD"), kx, top - 56.0, 1, MUTED_LIGHT, depth);
            text(&mut out, say("GAMEPAD"), px, top - 56.0, 1, MUTED_LIGHT, depth);
            let items = menu.items(page, view.settings);
            // Down to above the hint at the bottom, closer where there are many.
            let apart = ((top - 66.0 - 24.0) / (items.len().max(2) - 1) as f32).floor().min(12.0);
            let keys = view.settings.keys_of(part);
            let pad = view.settings.pad_of(seat);
            for (i, &item) in items.iter().enumerate() {
                let y = top - 66.0 - i as f32 * apart;
                row(&mut out, view, i, &item.label_in(lang), x0, y, 1, depth, now);
                let Item::Bind(action) = item else { continue };
                if menu.rebinding == Some(action) {
                    if (menu.tick / 15).is_multiple_of(2) {
                        text(&mut out, say("PRESS A KEY OR BUTTON"), kx, y, 1, NUMBERS, depth);
                    }
                    continue;
                }
                let key = keys.map_or_else(|| "-".to_string(), |b| b.get(action).first().map(|k| view.settings.key_label(k)).unwrap_or_default());
                let button = if crate::seating::DIRECTIONS.contains(&action) { String::new() } else { pad.get(action).first().map(|b| pad_label(b, menu.pad_style)).unwrap_or_default() };
                text(&mut out, &key, kx, y, 1, TEXT, depth);
                text(&mut out, &button, px, y, 1, TEXT, depth);
            }
            // How it moves where its directions are no rows of their own.
            if !matches!(part, crate::seating::Keys::Left | crate::seating::Keys::Right) {
                let y = top - 66.0 - items.len() as f32 * apart - 4.0;
                let moving = if part == crate::seating::Keys::Whole { "MOVE: ARROWS OR WASD, LEFT STICK OR D-PAD" } else { "MOVE: LEFT STICK OR D-PAD" };
                text(&mut out, say(moving), x0, y, 1, MUTED_LIGHT, depth);
            }
        }
        Page::Save | Page::Load => {
            let x0 = (layout.width / 2.0 - 100.0).round();
            for (i, &item) in menu.items(page, view.settings).iter().enumerate() {
                let label = match item {
                    Item::Slot(n) => menu.slots[n].clone().unwrap_or_else(|| format!("{}  {}", n + 1, say("EMPTY"))),
                    other => other.label_in(lang),
                };
                row(&mut out, view, i, &label, x0, top - 70.0 - i as f32 * 14.0, 1, depth, now);
            }
        }
        Page::Initials => {
            let e = &menu.entry;
            let line = crate::lang::say_with(lang, "{} PLACE   SCORE {}", &[&crate::lang::ordinal(lang, e.rank + 1), &format!("{:06}", e.score)]);
            centred(&mut out, &layout, &line, top - 58.0, 1, NUMBERS, depth);
            centred(&mut out, &layout, say("ENTER YOUR INITIALS"), top - 74.0, 1, MUTED_LIGHT, depth);
            // Whose, where several played.
            if let Some(seat) = e.seat {
                centred(&mut out, &layout, &crate::lang::say_with(lang, "PLAYER {}", &[&seat.to_string()]), top - 48.0, 1, TEXT, depth);
            }
            // Three big letters; the one being picked blinks and hops when
            // it changes, those to come wait as lines.
            let (scale, cell) = (5, 40.0);
            let x0 = (layout.width / 2.0 - cell * 1.5).round();
            for (k, &c) in e.letters.iter().enumerate() {
                let x = x0 + k as f32 * cell;
                if k > e.at {
                    for bx in 6..cell as i32 - 6 {
                        for by in 36..39 {
                            for z in 0..depth {
                                voxel(&mut out, x + bx as f32, by as f32, z as f32, MUTED_LIGHT);
                            }
                        }
                    }
                    continue;
                }
                let color = if k < e.at || (menu.tick / 10).is_multiple_of(2) { TEXT } else { RED };
                let since = now - menu.moved as f32;
                let hop = if k == e.at && since < 10.0 { ((std::f32::consts::PI * since / 10.0).sin() * 3.0).round() } else { 0.0 };
                big_letter(&mut out, c, x, cell, 36.0 + hop, scale, color, depth, view.font, finish);
            }
        }
        Page::HighScores => {
            // The lines drop in one after another, the new one blinking.
            for (i, h) in view.settings.high_scores.iter().enumerate() {
                let Some(lift) = drop_lift(t - 15.0 - i as f32 * 4.0, 60.0, 0.25) else { continue };
                let line = format!("{:>4}  {:<3}  {:06}", crate::lang::ordinal(lang, i + 1), h.initials, h.score);
                let color = match (menu.highlight == Some(i), i) {
                    (true, _) if (menu.tick / 8).is_multiple_of(2) => RED,
                    (true, _) | (false, 0) => NUMBERS,
                    _ => TEXT,
                };
                centred(&mut out, &layout, &line, top - 60.0 - i as f32 * 11.0 + lift, 1, color, depth);
            }
        }
    }

    // How to steer, for the device in hand.
    let style = menu.pad_style;
    let with = |en: &'static str, parts: &[&str]| crate::lang::say_with(lang, en, parts);
    let (south, east) = (pad_label("south", style), pad_label("east", style));
    let hint = match (menu.device, menu.rebinding.is_some(), page) {
        (_, true, _) => with("ESC OR {}: CANCEL", &[&pad_label("back", style)]),
        (Device::Gamepad, _, Page::Initials) => with("UP/DOWN: LETTER   {}: NEXT   {}: BACK", &[&south, &east]),
        (Device::Keyboard, _, Page::Initials) => say("TYPE OR UP/DOWN   ENTER: NEXT   ESC: BACK").to_string(),
        (Device::Gamepad, _, Page::HighScores) => with("{}: CONTINUE", &[&south]),
        (Device::Keyboard, _, Page::HighScores) => say("ENTER: CONTINUE").to_string(),
        (Device::Gamepad, _, Page::Join) => with("UP/DOWN: SIGN   RIGHT: ONE MORE   {}: JOIN   {}: BACK", &[&south, &east]),
        (Device::Keyboard, _, Page::Join) => say("TYPE   ENTER: JOIN   ESC: BACK").to_string(),
        (Device::Gamepad, _, Page::Code) => with("UP/DOWN: SIGN   RIGHT: ONE MORE   {}: JOIN   {}: BACK", &[&south, &east]),
        (Device::Keyboard, _, Page::Code) => say("TYPE   ENTER: JOIN   ESC: BACK").to_string(),
        (Device::Gamepad, _, Page::Joining) => with("{}: CANCEL", &[&east]),
        (Device::Keyboard, _, Page::Joining) => say("ESC: CANCEL").to_string(),
        (Device::Gamepad, _, _) => with("{}: SELECT   {}: BACK", &[&south, &east]),
        (Device::Keyboard, _, _) => say("ENTER: SELECT   ESC: BACK").to_string(),
    };
    let hint = hint.as_str();
    let (x, color) = if page == Page::Title {
        (8.0, if view.light_menu { MUTED_LIGHT } else { MUTED_DARK })
    } else {
        (((layout.width - text_width(hint, 1)) / 2.0).round(), MUTED_LIGHT)
    };
    text(&mut out, hint, x, 4.0, 1, color, depth);
    crate::render::hide_inner_faces(&mut out);
    (out, letters, camera)
}

/// One big letter in the middle of a cell `cell` wide from `x`: the
/// display font's when the game has one, else the pixel font's at `scale`.
#[allow(clippy::too_many_arguments)]
fn big_letter(out: &mut Vec<Instance>, c: char, x: f32, cell: f32, y: f32, scale: i32, color: [u8; 4], depth: i32, font: Option<&DisplayFont>, finish: Finish) {
    match font {
        Some(font) => {
            let g = font.glyph(c, 8 * scale as u32);
            let x = x + ((cell - g.width as f32) / 2.0).round();
            extrude(out, &g, x, y, color, depth * 2, Finish { seed: c as u32 * 7, ..finish });
        }
        None => text(out, &c.to_string(), x + ((cell - text_width("A", scale)) / 2.0).round(), y, scale, color, depth),
    }
}

/// Text in the middle of the screen's width.
#[allow(clippy::too_many_arguments)]
fn centred(out: &mut Vec<Instance>, layout: &Layout, s: &str, y: f32, scale: i32, color: [u8; 4], depth: i32) {
    text(out, s, ((layout.width - text_width(s, scale)) / 2.0).round(), y, scale, color, depth);
}

/// A menu row: yellow and marked when selected, hopping a little.
#[allow(clippy::too_many_arguments)]
fn row(out: &mut Vec<Instance>, view: &MenuView, i: usize, label: &str, x: f32, y: f32, scale: i32, depth: i32, now: f32) {
    // Over the bright sand of the title dark letters read, over a dark
    // title and the darkened pages light ones.
    let menu = view.menu;
    let (idle, active, mark) = match menu.page() {
        Some(Page::Title) if view.light_menu => TITLE_ROWS_LIGHT,
        Some(Page::Title) => TITLE_ROWS,
        _ => (TEXT, NUMBERS, RED),
    };
    if menu.cursor() != i {
        text(out, label, x, y, scale, idle, depth);
        return;
    }
    let since = now - menu.moved as f32;
    let hop = if since < 10.0 { (std::f32::consts::PI * since / 10.0).sin() * 2.0 } else { 0.0 };
    let y = y + hop.round();
    text(out, label, x, y, scale, active, depth);
    // The marker nudges towards the item.
    let nudge = ((now / 8.0).sin() * 1.5).round();
    text(out, ">", x - 9.0 * scale as f32 + nudge, y, scale, mark, depth);
}

/// A setting's value next to its row.
fn setting(out: &mut Vec<Instance>, view: &MenuView, item: Item, x: f32, y: f32, selected: bool, depth: i32) {
    let s = view.settings;
    let lang = s.language();
    let flag = |on: bool| crate::lang::say(&lang, if on { "ON" } else { "OFF" });
    let color = if selected { NUMBERS } else { TEXT };
    match item {
        Item::Music | Item::Sound => {
            let v = if item == Item::Music { s.music } else { s.sound };
            for i in 0..VOLUME_STEPS {
                let c = if i < v { color } else { SLIDER_EMPTY };
                for dy in 0..7 {
                    for dx in 0..3 {
                        for z in 0..depth {
                            voxel(out, x + i as f32 * 5.0 + dx as f32, y + dy as f32, z as f32, c);
                        }
                    }
                }
            }
        }
        Item::Song => {
            let name = view.songs.get(s.song).map(String::as_str).unwrap_or("-");
            let shown = if selected { format!("< {} >", name.to_uppercase()) } else { name.to_uppercase() };
            text(out, &shown, x, y, 1, color, depth);
        }
        Item::Fullscreen => text(out, flag(s.fullscreen), x, y, 1, color, depth),
        Item::Shake => text(out, flag(s.shake), x, y, 1, color, depth),
        Item::Flashes => text(out, flag(s.flashes), x, y, 1, color, depth),
        Item::Blur => text(out, flag(s.blur), x, y, 1, color, depth),
        Item::Rumble => text(out, flag(s.rumble), x, y, 1, color, depth),
        Item::Shadows => {
            let label = crate::lang::say(&lang, s.shadows.label());
            let shown = if selected { format!("< {label} >") } else { label.to_string() };
            text(out, &shown, x, y, 1, color, depth);
        }
        Item::FrameRate => {
            let label = crate::lang::say(&lang, s.frame_rate.label());
            let shown = if selected { format!("< {label} >") } else { label.to_string() };
            text(out, &shown, x, y, 1, color, depth);
        }
        Item::Language => {
            let name = crate::lang::name(&lang);
            let shown = if selected { format!("< {name} >") } else { name };
            text(out, &shown, x, y, 1, color, depth);
        }
        Item::ShowFps => text(out, flag(s.show_fps), x, y, 1, color, depth),
        _ => {}
    }
}

/// What the network says, small in the middle, a line each (who went
/// quiet, who paused, how far joining is): at the top over the game, low
/// over a menu (its heading and the host's lines are at the top), from
/// `low` up.
pub fn notice(out: &mut Vec<Instance>, lines: &[String], cfg: &UiConfig, aspect: f32, low: Option<f32>) {
    let layout = Layout { width: (cfg.height * aspect).round(), height: cfg.height };
    let mut voxels = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let y = match low {
            Some(from) => from + (lines.len() - 1 - i) as f32 * 9.0,
            None => layout.height - 10.0 - i as f32 * 9.0,
        };
        centred(&mut voxels, &layout, line, y, 1, NUMBERS, cfg.depth);
    }
    crate::render::hide_inner_faces(&mut voxels);
    out.extend(voxels);
}

/// The frames a second, small in the bottom right corner, over the game
/// and the menus alike (`Settings::show_fps`).
pub fn fps(out: &mut Vec<Instance>, fps: u32, cfg: &UiConfig, aspect: f32) {
    let layout = Layout { width: (cfg.height * aspect).round(), height: cfg.height };
    let label = format!("{fps} FPS");
    let mut voxels = Vec::new();
    text(&mut voxels, &label, layout.width - 3.0 - text_width(&label, 1), 3.0, 1, TEXT, cfg.depth);
    crate::render::hide_inner_faces(&mut voxels);
    out.extend(voxels);
}

/// "PRESS START", blinking over a replay the title plays (attract mode),
/// `seconds` into it.
pub fn press_start(out: &mut Vec<Instance>, seconds: f32, cfg: &UiConfig, aspect: f32, lang: &str) {
    if (seconds * 2.0) as u32 % 2 == 1 {
        return;
    }
    let layout = Layout { width: (cfg.height * aspect).round(), height: cfg.height };
    let label = crate::lang::say(lang, "PRESS START");
    let mut voxels = Vec::new();
    text(&mut voxels, label, ((layout.width - text_width(label, 2)) / 2.0).round(), (layout.height * 0.22).round(), 2, NUMBERS, cfg.depth);
    crate::render::hide_inner_faces(&mut voxels);
    out.extend(voxels);
}

/// A dark see-through layer over the whole screen, behind the menu.
fn dim(out: &mut Vec<Instance>, layout: &Layout, opacity: f32) {
    const SIZE: f32 = 24.0;
    let fx = [0, (opacity.clamp(0.0, 1.0) * 255.0) as u8, 0, 0];
    // Behind the letters (z below 0), reaching past the edges, as the
    // perspective shrinks what is further back.
    let z = -SIZE / 2.0 - 2.0;
    let mut y = -SIZE * 2.0;
    while y < layout.height + SIZE * 2.0 {
        let mut x = -SIZE * 2.0;
        while x < layout.width + SIZE * 2.0 {
            out.push(Instance { pos_size: [x, y, z, SIZE], color: DIM, fx, shade: [0; 4], turn: [0; 4] });
            x += SIZE;
        }
        y += SIZE;
    }
}

/// The game's logo, big and deep, its letters falling in one after the
/// other; the drip line bleeds. In the display font when the game has one.
/// The letters come from the model bank (`letters`, where they are) when
/// it holds them: building them every frame took most of the title's. A
/// game's own logo model instead, its parts falling in one after another.
fn logo(out: &mut Vec<Instance>, letters: &mut Vec<ModelDraw>, view: &MenuView, layout: &Layout, now: f32, finish: Finish) {
    if let Some(model) = view.logo_model {
        let x0 = ((layout.width - model.size.x) / 2.0).round();
        let y0 = layout.height - 14.0 - model.size.y;
        for (i, (voxels, at)) in model.parts.iter().enumerate() {
            let t = now - (i as u64 * LETTER_DELAY * 2) as f32;
            if let Some(lift) = drop_lift(t, 90.0, 0.2) {
                let origin = Vec3::new(x0 + at.x, y0 + at.y + lift + bob(t, now, i, view.logo_bob), at.z);
                letters.push(ModelDraw { voxels: voxels.clone(), origin, turn: 0, rot: glam::Quat::IDENTITY, shift: glam::Vec3::ZERO, tint: None, nearer: 0.0 });
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
                    Some(voxels) => letters.push(ModelDraw { voxels: voxels.clone(), origin: Vec3::new(x, y + lift, 0.0), turn: 0, rot: glam::Quat::IDENTITY, shift: glam::Vec3::ZERO, tint: None, nearer: 0.0 }),
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
fn bob(t: f32, now: f32, i: usize, amount: f32) -> f32 {
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
fn logo_height(n: usize, lines: usize, heights: &[u32]) -> u32 {
    heights.get(n).copied().unwrap_or(if n + 1 == lines { 42 } else { 21 })
}

/// How deep a logo line's letters are: deeper the bigger, so a big line
/// stands out of the screen.
fn logo_depth(height: u32) -> i32 {
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
fn big_glyphs(s: &str, height: u32, font: Option<&DisplayFont>) -> Vec<Rc<Glyph>> {
    match font {
        Some(f) => s.chars().map(|c| f.glyph(c, height)).collect(),
        None => engine_case(s).chars().map(|c| Rc::new(pixel_glyph(c, (height / 7).max(1) as i32))).collect(),
    }
}

fn gap(height: u32, font: Option<&DisplayFont>) -> i32 {
    if font.is_some() { DisplayFont::gap(height) } else { (height / 7).max(1) as i32 }
}

/// A pixel font letter as a glyph, `scale` voxels a pixel (a space for
/// characters it lacks).
fn pixel_glyph(ch: char, scale: i32) -> Glyph {
    let Some(g) = glyph(ch) else {
        return Glyph::new(5 * scale, 7 * scale, Vec::new());
    };
    // An accented capital's accent in two rows above it.
    let accent = accented(ch).map(|(_, a)| a);
    let above = if accent.is_some() { 2 } else { 0 };
    let row = |r: i32| match r {
        r if r < 0 => accent.map_or(".....", |a| a[(r + 2) as usize]),
        r => g[r as usize],
    };
    let rows = (-above * scale..9 * scale)
        .map(|r| (0..5 * scale).map(|c| row(r.div_euclid(scale)).as_bytes()[(c / scale) as usize] == b'#').collect())
        .collect();
    Glyph::new(5 * scale, (7 + above) * scale, rows)
}

/// How far a letter dropping in is still up, `t` ticks after it was let
/// go (None: not yet): it falls from `height` and bounces once, `bounce`
/// as high as it fell fast.
fn drop_lift(t: f32, height: f32, bounce: f32) -> Option<f32> {
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
fn extrude(out: &mut Vec<Instance>, g: &Glyph, x: f32, y: f32, color: [u8; 4], depth: i32, finish: Finish) {
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
fn crack_map(g: &Glyph, finish: Finish, depth: i32) -> Vec<Vec<u8>> {
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
fn grain(color: [u8; 4], c: i32, r: i32, z: i32, finish: Finish) -> [u8; 4] {
    if finish.grain <= 0.0 {
        return color;
    }
    let speckle = hash(c, r, z, finish.seed) - 0.5;
    let blotch = smooth_noise(c as f32 / 5.0, r as f32 / 5.0, finish.seed) - 0.5;
    let k = 1.0 + finish.grain * (speckle * 0.8 + blotch * 1.6);
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    [f(color[0]), f(color[1]), f(color[2]), color[3]]
}

/// A number in [0, 1) for a cell, the same every time.
fn hash(a: i32, b: i32, c: i32, seed: u32) -> f32 {
    let mut h = (a as u32).wrapping_mul(0x8da6_b343) ^ (b as u32).wrapping_mul(0xd816_3841) ^ (c as u32).wrapping_mul(0xcb1a_b31f) ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Smooth noise in [0, 1): `hash` on a lattice, blended between.
fn smooth_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (ease(x - x0), ease(y - y0));
    let v = |dx: i32, dy: i32| hash(x0 as i32 + dx, y0 as i32 + dy, -1, seed);
    let top = v(0, 0) + (v(1, 0) - v(0, 0)) * sx;
    let bottom = v(0, 1) + (v(1, 1) - v(0, 1)) * sx;
    top + (bottom - top) * sy
}

fn darker(c: [u8; 4], k: f32) -> [u8; 4] {
    [(c[0] as f32 * k) as u8, (c[1] as f32 * k) as u8, (c[2] as f32 * k) as u8, c[3]]
}

/// Blood running off the bottom of a letter standing on `y` at `x`: under
/// some of its lowest voxels a drip grows, lets a drop go and starts again.
fn drips(out: &mut Vec<Instance>, g: &Glyph, x: f32, y: f32, letter: usize, now: f32) {
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

/// Parses "#rrggbb" (or gives white).
pub fn hex_color(s: &str) -> [u8; 4] {
    let h = s.trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xffffff);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]
}


#[cfg(test)]
mod tests {
    use super::*;

    /// PRESS START blinks: half a second on, half off.
    #[test]
    fn press_start_blinks() {
        let cfg = UiConfig::default();
        let drawn = |t: f32| {
            let mut out = Vec::new();
            press_start(&mut out, t, &cfg, 16.0 / 9.0, "en");
            out.len()
        };
        assert!(drawn(0.2) > 100 && drawn(1.1) > 100);
        assert_eq!((drawn(0.7), drawn(1.6)), (0, 0));
    }

    /// Every glyph is 5 wide, and the HUD text only uses glyphs the font has.
    #[test]
    fn the_font_covers_the_hud() {
        for (c, g) in FONT {
            assert!(g.iter().all(|row| row.len() == 5), "{c} is not 5 wide");
        }
        for s in ["HOSTAGES 0/4", "MISSION COMPLETE", "GAME OVER", "PRESS F5", "ROCKETS 8", "MG 150", "DOWN!", "GUNSHIP!", "x3", "SCORE 1234567890", "PRESS START"] {
            for ch in s.chars().filter(|c| *c != ' ') {
                assert!(glyph(ch).is_some(), "no glyph for '{ch}' in '{s}'");
            }
        }
    }

    /// A game writes German and French in lower case too, a g's tail below
    /// the line; the capitals and digits are as they were (nothing below
    /// the line), and the engine's own text stays in capitals.
    #[test]
    fn the_font_writes_german_and_french() {
        for s in [
            "abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789",
            "Grüße aus der Möhrenküche: Kürbis, Blaubeeren & Gänse. ÄÖÜ äöü ß",
            "Œufs, crème brûlée, garçon, à bientôt, ça va… «Fenêtre» naïve, Noël. ÀÂÇÈÉÊËÎÏÔÛÙ",
            "Saat · 3 × Weizen → ▲ ▼ ✓ „Ernte“ — 12 – 3 − 4 • it’s",
        ] {
            for ch in s.chars().filter(|c| *c != ' ') {
                assert!(glyph(ch).is_some(), "no glyph for '{ch}' in '{s}'");
            }
        }
        for c in ('A'..='Z').chain('0'..='9') {
            assert_eq!(glyph(c).unwrap()[7..], [".....", "....."], "{c} reaches below the line");
        }
        type Draw = fn(&mut Vec<Instance>, &str, f32, f32, i32, [u8; 4], i32);
        let below = |s: &str, f: Draw| {
            let mut out = Vec::new();
            f(&mut out, s, 0.0, 10.0, 1, TEXT, 1);
            (out.iter().any(|i| i.pos_size[1] < 10.0), out.len())
        };
        assert!(below("g", written).0 && !below("G", written).0, "a g's tail goes below the line, a G stays on it");
        let mut tall = Vec::new();
        written(&mut tall, "Ü", 0.0, 10.0, 1, TEXT, 1);
        let mut plain = Vec::new();
        written(&mut plain, "U", 0.0, 10.0, 1, TEXT, 1);
        assert!(tall.len() > plain.len() && tall.iter().any(|i| i.pos_size[1] > 17.0), "an Ü is a U at its full height, the dots above");
        assert_eq!(engine_case("Pulsar Drive x3 groß"), "PULSAR DRIVE x3 GROSS");
        assert_eq!(below("AB", text), below("AB", written), "capitals as they were");
        assert_ne!(below("ab", text).1, below("ab", written).1, "the engine's text in capitals, a game's as written");
    }

    /// Every menu text, and the default keys' and buttons' names, can be
    /// drawn with the font.
    #[test]
    fn the_font_covers_the_menus() {
        use crate::controls::PadStyle;
        use crate::menu::Page;
        let mut settings = Settings::default();
        settings.adopt(&crate::shooter::ACTIONS, &crate::shooter::keys(), &crate::shooter::pad());
        let mut texts: Vec<String> = Vec::new();
        for page in [Page::Title, Page::Pause, Page::Settings, Page::Audio, Page::Video, Page::Controls, Page::Initials, Page::HighScores] {
            texts.push(page.heading().to_string());
            texts.extend(page.items(&settings.actions).iter().map(|i| i.label()));
        }
        texts.extend(settings.keys.names().map(|k| settings.key_label(k)));
        for style in [PadStyle::Xbox, PadStyle::Nintendo, PadStyle::PlayStation] {
            texts.extend(settings.pad.names().map(|b| pad_label(b, style)));
        }
        texts.push(Item::Continue.label());
        // Its words in German and French too, and the languages' names.
        texts.extend(crate::lang::words().map(|w| w.replace("{}", "")));
        texts.extend(["de", "en", "fr"].map(crate::lang::name));
        texts.extend(["PRESS A KEY OR BUTTON", "ENTER: SELECT   ESC: BACK", "A: SELECT   B: BACK", "HI-SCORE 000000", "< ON >", "OFF"].map(String::from));
        texts.push("MOVE: ARROWS OR WASD, LEFT STICK OR D-PAD".to_string());
        let info = crate::saves::SlotInfo { level: "level_02".into(), ticks: 600, when: 1 };
        texts.extend(["SAVE", "LOAD", "2  EMPTY", "JUST NOW", "5 MIN AGO", "3 DAYS AGO"].map(String::from));
        texts.push(crate::menu::slot_line(1, Some(&info), 3_601, "en"));
        texts.extend(["ENTER YOUR INITIALS", "TYPE OR UP/DOWN   ENTER: NEXT   ESC: BACK", "UP/DOWN: LETTER", "ENTER: CONTINUE", "10TH PLACE   SCORE 000000"].map(String::from));
        texts.extend(crate::menu::LETTERS.chars().map(String::from));
        settings.adopt_scores(&[]);
        texts.extend(settings.high_scores.iter().map(|h| h.initials.clone()));
        for s in texts {
            for ch in s.chars().filter(|c| *c != ' ') {
                assert!(glyph(ch).is_some(), "no glyph for '{ch}' in '{s}'");
            }
        }
    }

    /// A big letter's front edges are rounded off (its front stops short of
    /// the outline, its back reaches it), cracks carve into its face, and
    /// the finish comes out the same every frame.
    #[test]
    fn big_letters_are_rounded_and_cracked() {
        let g = Glyph::new(20, 20, vec![vec![true; 20]; 20]);
        let depth = 8;
        let finish = Finish { round: 0.2, grain: 0.1, cracks: 3.0, seed: 5 };
        let build = |finish: Finish| {
            let mut out = Vec::new();
            extrude(&mut out, &g, 0.0, 0.0, [200, 30, 30, 255], depth, finish);
            out
        };
        let at = |v: &[Instance], x: i32, y: i32, z: i32| v.iter().any(|i| i.pos_size[..3] == [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5]);
        let fronts = |v: &[Instance]| v.iter().filter(|i| i.pos_size[2] == depth as f32 - 0.5).count();

        let smooth = build(Finish { cracks: 0.0, ..finish });
        assert!(!at(&smooth, 0, 10, depth - 1), "the front reaches the outline");
        assert!(at(&smooth, 0, 10, 0), "the back stops short of the outline");
        assert!(at(&smooth, 10, 10, depth - 1), "the middle of the front is missing");

        let cracked = build(finish);
        assert!(fronts(&cracked) < fronts(&smooth), "the cracks carve nothing out of the face");
        let again = build(finish);
        let key = |v: &[Instance]| v.iter().map(|i| (i.pos_size.map(f32::to_bits), i.color)).collect::<Vec<_>>();
        assert_eq!(key(&cracked), key(&again), "the finish changes from frame to frame");
    }

}

