//! The display font for big letters (the logo, headings, messages): a glyph
//! atlas, a grey PNG made from a real typeface (`tools/gen_moreblood.py`),
//! sampled down to the size the UI draws at. Big letters get the voxels a
//! real font gives them instead of a blown-up 5 x 7 pixel font.
//!
//! The atlas is one row of equal cells, one per character from ' ' to 'Z';
//! each glyph stands on the cell's baseline, a capital reaching from the top
//! of the cell down to it (the capital is four fifths of the cell's height,
//! the rest is room below the baseline).

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use anyhow::{Result, bail};

const FIRST: u8 = b' ';
const LAST: u8 = b'Z';

/// How far `Glyph::inside` looks for the outline.
pub const EDGE_REACH: f32 = 8.0;

/// A glyph at one size: its rows from the top, `ascent` of them above the
/// baseline (the capital height), trimmed to its ink left and right.
pub struct Glyph {
    pub width: i32,
    pub ascent: i32,
    pub rows: Vec<Vec<bool>>,
    /// How far each cell lies inside the outline, worked out when first
    /// asked.
    depths: OnceCell<Vec<Vec<f32>>>,
}

impl Glyph {
    pub fn new(width: i32, ascent: i32, rows: Vec<Vec<bool>>) -> Self {
        Self { width, ascent, rows, depths: OnceCell::new() }
    }

    pub fn on(&self, c: i32, r: i32) -> bool {
        r >= 0 && c >= 0 && (r as usize) < self.rows.len() && (c as usize) < self.rows[r as usize].len() && self.rows[r as usize][c as usize]
    }

    /// How far inside the outline the centre of cell (c, r) lies, in voxels:
    /// half a voxel on the edge, at most `EDGE_REACH`, 0 off the glyph.
    pub fn inside(&self, c: i32, r: i32) -> f32 {
        if !self.on(c, r) {
            return 0.0;
        }
        self.depths.get_or_init(|| self.measure())[r as usize][c as usize]
    }

    /// For every cell, the distance from its centre to the nearest empty cell.
    fn measure(&self) -> Vec<Vec<f32>> {
        let reach = EDGE_REACH as i32;
        let mut depths: Vec<Vec<f32>> = self.rows.iter().map(|row| vec![0.0; row.len()]).collect();
        for (r, row) in depths.iter_mut().enumerate() {
            for (c, depth) in row.iter_mut().enumerate() {
                let (c, r) = (c as i32, r as i32);
                if !self.on(c, r) {
                    continue;
                }
                let mut best = EDGE_REACH;
                for dr in -reach..=reach {
                    for dc in -reach..=reach {
                        if !self.on(c + dc, r + dr) {
                            let (x, y) = ((dc.abs() as f32 - 0.5).max(0.0), (dr.abs() as f32 - 0.5).max(0.0));
                            best = best.min((x * x + y * y).sqrt());
                        }
                    }
                }
                *depth = best;
            }
        }
        depths
    }
}

#[derive(Clone, Copy)]
enum Accent {
    Diaeresis,
    Acute,
    Grave,
    Circumflex,
}

/// The capital an accented capital is made of, and its accent.
fn accent_of(ch: char) -> Option<(char, Accent)> {
    use Accent::*;
    Some(match ch {
        'Ä' => ('A', Diaeresis),
        'À' => ('A', Grave),
        'Â' => ('A', Circumflex),
        'É' => ('E', Acute),
        'È' => ('E', Grave),
        'Ê' => ('E', Circumflex),
        'Ë' => ('E', Diaeresis),
        'Î' => ('I', Circumflex),
        'Ï' => ('I', Diaeresis),
        'Ö' => ('O', Diaeresis),
        'Ô' => ('O', Circumflex),
        'Ü' => ('U', Diaeresis),
        'Û' => ('U', Circumflex),
        'Ù' => ('U', Grave),
        _ => return None,
    })
}

pub struct DisplayFont {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    cell: usize,
    /// The capital height in the atlas, in pixels.
    cap: f32,
    cache: RefCell<HashMap<(char, u32), Rc<Glyph>>>,
}

impl DisplayFont {
    pub fn load(path: &Path) -> Result<Self> {
        Self::load_in(&crate::files::Files::disk(), path)
    }

    /// The font of the game's files (`crate::files`).
    pub fn load_in(files: &crate::files::Files, path: &Path) -> Result<Self> {
        let (width, height, pixels) = crate::map::read_png_in(files, path)?;
        let count = (LAST - FIRST + 1) as usize;
        if !(width as usize).is_multiple_of(count) {
            bail!("{}: {width} pixels wide is not {count} equal cells", path.display());
        }
        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels: pixels.iter().map(|p| p[0]).collect(),
            cell: width as usize / count,
            cap: height as f32 * 0.8,
            cache: RefCell::new(HashMap::new()),
        })
    }

    /// The gap between letters at a capital height of `height` voxels.
    pub fn gap(height: u32) -> i32 {
        (height as f32 * 0.07).round().max(1.0) as i32
    }

    /// `ch` with a capital height of `height` voxels (lower case drawn as
    /// capitals, an accented letter as its capital with the accent over it,
    /// characters it lacks as '?').
    pub fn glyph(&self, ch: char, height: u32) -> Rc<Glyph> {
        let mut upper = ch.to_uppercase();
        let up = match (upper.next(), upper.next()) {
            (Some(u), None) => u,
            _ => ch,
        };
        let key = match accent_of(up) {
            Some(_) => up,
            None if up.is_ascii() && (FIRST..=LAST).contains(&(up as u8)) => up,
            None => '?',
        };
        if let Some(g) = self.cache.borrow().get(&(key, height)) {
            return g.clone();
        }
        let g = Rc::new(match accent_of(key) {
            Some((base, accent)) => self.accented(base, accent, height),
            None => self.sample(key as u8, height),
        });
        self.cache.borrow_mut().insert((key, height), g.clone());
        g
    }

    /// The capital `base` with `accent` over it, inside the capital's own
    /// height as pixel fonts do it: the letter's body is squeezed from the
    /// top (rows dropped evenly, its width kept) to make room, so the
    /// baseline, the ascent and the line height stay.
    fn accented(&self, base: char, accent: Accent, height: u32) -> Glyph {
        let g = self.sample(base as u8, height);
        let (w, thick) = (g.width as usize, ((height as f32 / 10.0).round() as usize).max(2));
        let room = (height as f32 * 0.05).round().max(1.0) as usize;
        let mut rows = vec![vec![false; w]; thick + room];
        let mid = w as f32 / 2.0;
        let stroke = (thick as f32 / 2.0).ceil().max(1.0);
        for (i, row) in rows.iter_mut().take(thick).enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                let x = c as f32 + 0.5;
                *cell = match accent {
                    Accent::Diaeresis => {
                        let d = (thick as f32).min(w as f32 / 4.0).max(1.0);
                        [0.25, 0.75].iter().any(|&f| (x - w as f32 * f).abs() <= d / 2.0)
                    }
                    // Slanting up to the right / left, the stroke `stroke` wide.
                    Accent::Acute => (x - (mid + (thick as f32 / 2.0 - i as f32 - 0.5))).abs() <= stroke / 2.0,
                    Accent::Grave => (x - (mid - (thick as f32 / 2.0 - i as f32 - 0.5))).abs() <= stroke / 2.0,
                    // A roof: wide at the foot.
                    Accent::Circumflex => (x - (mid - (i as f32 + 0.5))).abs() <= stroke / 2.0 || (x - (mid + (i as f32 + 0.5))).abs() <= stroke / 2.0,
                };
            }
        }
        let (cap, k) = (g.ascent as usize, rows.len());
        let body = cap - k;
        for r in 0..body {
            rows.push(g.rows[r * cap / body].clone());
        }
        rows.extend(g.rows[cap.min(g.rows.len())..].iter().cloned());
        Glyph::new(g.width, g.ascent, rows)
    }

    /// How wide `s` is at `height`, gaps included.
    pub fn width(&self, s: &str, height: u32) -> i32 {
        let gap = Self::gap(height);
        let n = s.chars().count() as i32;
        s.chars().map(|c| self.glyph(c, height).width).sum::<i32>() + gap * (n - 1).max(0)
    }

    /// Averages the atlas cell over each target voxel; half covered or more
    /// is a voxel.
    fn sample(&self, code: u8, height: u32) -> Glyph {
        let scale = height as f32 / self.cap;
        let x0 = (code - FIRST) as usize * self.cell;
        let cols = (self.cell as f32 * scale).ceil() as usize;
        let rows = (self.height as f32 * scale).ceil() as usize;
        let mut grid = vec![vec![false; cols]; rows];
        for (ty, row) in grid.iter_mut().enumerate() {
            let (sy0, sy1) = ((ty as f32 / scale) as usize, (((ty + 1) as f32 / scale).ceil() as usize).min(self.height));
            for (tx, cell) in row.iter_mut().enumerate() {
                let (sx0, sx1) = ((tx as f32 / scale) as usize, (((tx + 1) as f32 / scale).ceil() as usize).min(self.cell));
                let (mut sum, mut n) = (0u32, 0u32);
                for sy in sy0..sy1.max(sy0 + 1).min(self.height) {
                    for sx in sx0..sx1.max(sx0 + 1).min(self.cell) {
                        sum += self.pixels[sy * self.width + x0 + sx] as u32;
                        n += 1;
                    }
                }
                *cell = n > 0 && sum >= n * 128;
            }
        }
        // Trim to the ink; a space keeps a third of its height.
        let inked: Vec<usize> = (0..cols).filter(|&c| grid.iter().any(|r| r[c])).collect();
        let (left, right) = match (inked.first(), inked.last()) {
            (Some(&l), Some(&r)) => (l, r),
            _ => return Glyph::new((height as f32 * 0.3).round() as i32, height as i32, Vec::new()),
        };
        Glyph::new(
            (right - left + 1) as i32,
            (self.cap * scale).round() as i32,
            grid.into_iter().map(|r| r[left..=right].to_vec()).collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// More Blood's display font samples to the asked capital height, an O
    /// is hollow and wider than an I, a space is empty.
    #[test]
    fn the_display_font_samples_its_glyphs() {
        let font = DisplayFont::load(Path::new("games/moreblood/ui/display_font.png")).unwrap();
        let o = font.glyph('O', 40);
        assert_eq!(o.ascent, 40);
        let ink_rows = o.rows.iter().filter(|r| r.iter().any(|&v| v)).count() as i32;
        assert!((ink_rows - 40).abs() <= 2, "an O 40 high has {ink_rows} rows of ink");
        assert!(!o.on(o.width / 2, 20), "the O is not hollow");
        assert!(o.width > font.glyph('I', 40).width);
        assert!(font.glyph(' ', 40).rows.is_empty());
        assert_eq!(font.glyph('o', 40).width, o.width, "lower case is drawn as capitals");
    }

    /// An accented capital is its letter with the accent over it, never the
    /// '?' of a character the atlas lacks; the baseline stays.
    #[test]
    fn accented_capitals_are_their_letters_with_the_accent_over_them() {
        let font = DisplayFont::load(Path::new("games/moreblood/ui/display_font.png")).unwrap();
        let q = font.glyph('?', 40);
        for (accented, plain) in [('É', 'E'), ('è', 'E'), ('Ö', 'O'), ('Ü', 'U'), ('Â', 'A'), ('Ï', 'I'), ('Ù', 'U')] {
            let (g, p) = (font.glyph(accented, 40), font.glyph(plain, 40));
            assert_ne!((g.width, &g.rows), (q.width, &q.rows), "{accented} is a question mark");
            assert_eq!(g.width, p.width);
            assert_eq!(g.ascent, p.ascent, "{accented} rises above the capital");
            assert_eq!(g.rows.len(), p.rows.len(), "{accented} changes the line's height");
            let ink = |r: &Vec<bool>| r.iter().any(|&v| v);
            let top = g.rows.iter().position(ink).unwrap();
            assert_eq!(top, p.rows.iter().position(ink).unwrap(), "{accented}'s accent is not at the capital's top");
            // The accent, a gap with no ink, then the letter's body.
            let gap = g.rows.iter().skip(top).position(|r| !ink(r)).expect("the accent touches the letter");
            assert!(gap >= 2 && top + gap < g.rows.len() / 3, "{accented}'s accent is {gap} rows");
            assert!(g.rows.iter().skip(top + gap).any(ink), "{accented} lost its letter");
            let last = |g: &Glyph| g.rows.iter().rposition(ink).unwrap();
            assert_eq!(last(&g), last(&p), "{accented}'s baseline moved");
        }
        assert_eq!(font.glyph('ß', 40).rows, q.rows);
    }
}
