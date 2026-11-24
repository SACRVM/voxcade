//! Keys as small signs in a line of text: an action in braces
//! (`frame:text("{undo} UNDO")`) drawn as its key's sign where there is
//! one, else its name. In the pixel font's own size and style (the
//! owner, 2026-11-24: "dezent und nie übertreiben"): the line's nine rows
//! at most, ink in the text's color, frames and outlines in a quieter tone
//! of it (`quiet`), no colored buttons. A letter or digit key is its letter
//! (a small one, three by five) in a thin frame with cut corners; Escape,
//! Ctrl and Alt their short words so; the arrows, Tab, Shift, Return,
//! Backspace and Space plain signs; the mouse a small outline with the
//! button filled; a pad's face buttons a letter in a thin ring (the
//! PlayStation's its shapes alone), its shoulders and BACK in a frame,
//! START and OPTIONS the three bars in a ring, the d-pad a cross with the
//! way filled.
//!
//! A sign is one character of the Unicode private use area in the text
//! (`SIGN_BASE` on), so every reader of a line (widths, wrapping, the
//! drawing) takes it as one; the drawing knows it by `sign`. A script's own
//! text never carries one (`lua::world::frame::fill_keys` leaves them out).

use std::sync::LazyLock;

use crate::controls::Device;

/// The first character of the signs.
pub const SIGN_BASE: u32 = 0xE000;

/// A sign's rows, top first: `#` ink, `+` the quieter tone, `.` nothing;
/// every row as wide as the sign. Its first row is the line's row `top`
/// (0 the capitals' top, 6 their foot on the line): a sign of the line's
/// nine rows (a frame, a ring, the mouse, the d-pad) begins a row above
/// the capitals and ends a row below the line, so it stands round them as
/// a box round a word; the plain signs stand as letters do.
pub struct Sign {
    pub rows: [String; 9],
    pub width: usize,
    pub top: i32,
}

/// The small letters inside a frame or a ring, three wide and five tall.
const SMALL: &[(char, [&str; 5])] = &[
    ('A', [".#.", "#.#", "###", "#.#", "#.#"]),
    ('B', ["##.", "#.#", "##.", "#.#", "##."]),
    ('C', [".##", "#..", "#..", "#..", ".##"]),
    ('D', ["##.", "#.#", "#.#", "#.#", "##."]),
    ('E', ["###", "#..", "##.", "#..", "###"]),
    ('F', ["###", "#..", "##.", "#..", "#.."]),
    ('G', [".##", "#..", "#.#", "#.#", ".##"]),
    ('H', ["#.#", "#.#", "###", "#.#", "#.#"]),
    ('I', ["###", ".#.", ".#.", ".#.", "###"]),
    ('J', ["..#", "..#", "..#", "#.#", ".#."]),
    ('K', ["#.#", "#.#", "##.", "#.#", "#.#"]),
    ('L', ["#..", "#..", "#..", "#..", "###"]),
    ('M', ["#.#", "###", "###", "#.#", "#.#"]),
    ('N', ["##.", "#.#", "#.#", "#.#", "#.#"]),
    ('O', [".#.", "#.#", "#.#", "#.#", ".#."]),
    ('P', ["##.", "#.#", "##.", "#..", "#.."]),
    ('Q', [".#.", "#.#", "#.#", "##.", ".##"]),
    ('R', ["##.", "#.#", "##.", "#.#", "#.#"]),
    ('S', [".##", "#..", ".#.", "..#", "##."]),
    ('T', ["###", ".#.", ".#.", ".#.", ".#."]),
    ('U', ["#.#", "#.#", "#.#", "#.#", "###"]),
    ('V', ["#.#", "#.#", "#.#", "#.#", ".#."]),
    ('W', ["#.#", "#.#", "###", "###", "#.#"]),
    ('X', ["#.#", "#.#", ".#.", "#.#", "#.#"]),
    ('Y', ["#.#", "#.#", ".#.", ".#.", ".#."]),
    ('Z', ["###", "..#", ".#.", "#..", "###"]),
    ('0', ["###", "#.#", "#.#", "#.#", "###"]),
    ('1', [".#.", "##.", ".#.", ".#.", "###"]),
    ('2', ["##.", "..#", ".#.", "#..", "###"]),
    ('3', ["##.", "..#", ".#.", "..#", "##."]),
    ('4', ["#.#", "#.#", "###", "..#", "..#"]),
    ('5', ["###", "#..", "##.", "..#", "##."]),
    ('6', [".##", "#..", "###", "#.#", "###"]),
    ('7', ["###", "..#", ".#.", ".#.", ".#."]),
    ('8', ["###", "#.#", "###", "#.#", "###"]),
    ('9', ["###", "#.#", "###", "..#", "##."]),
    ('+', ["...", ".#.", "###", ".#.", "..."]),
    ('-', ["...", "...", "###", "...", "..."]),
    // The three bars of a pad's menu button (START, OPTIONS).
    ('≡', ["###", "...", "###", "...", "###"]),
];

/// What a sign looks like.
enum Look {
    /// Small letters in a thin frame with cut corners (a key, a shoulder).
    Cap(&'static str),
    /// A small letter or sign in a thin ring (a pad's face button).
    Ring(char),
    /// Rows of its own, ink only, from the top of the capitals.
    Plain(&'static [&'static str]),
    /// The mouse, a button filled, a wheel's arrow up (true) or down beside.
    Mouse(Button, Option<bool>),
    /// The d-pad, the way filled (0 up, 1 down, 2 left, 3 right).
    Pad(usize),
}

/// A mouse's button, filled in its sign.
enum Button {
    Left,
    Middle,
    Right,
}

const UP: &[&str] = &["..#..", ".###.", "#.#.#", "..#..", "..#..", "..#..", "..#.."];
const DOWN: &[&str] = &["..#..", "..#..", "..#..", "..#..", "#.#.#", ".###.", "..#.."];
const LEFT: &[&str] = &[".......", "..#....", ".#.....", "#######", ".#.....", "..#...."];
const RIGHT: &[&str] = &[".......", "....#..", ".....#.", "#######", ".....#.", "....#.."];
const RETURN: &[&str] = &[".....#", ".....#", "..#..#", ".#...#", "######", ".#....", "..#..."];
const TAB: &[&str] = &[".......", "....#.#", ".....##", "#######", ".....##", "....#.#"];
const SHIFT: &[&str] = &["...#...", "..#.#..", ".#...#.", "##...##", ".#...#.", ".#...#.", ".#####."];
const BACKSPACE: &[&str] = &[".......", "..######", ".#.#.#.#", "#...#..#", ".#.#.#.#", "..######"];
const SPACE: &[&str] = &[".......", ".......", ".......", ".......", ".......", "#.....#", "#######"];
// The PlayStation's face buttons: their shapes alone, as big as a letter.
const CROSS: &[&str] = &[".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#"];
const CIRCLE: &[&str] = &[".....", ".###.", "#...#", "#...#", "#...#", ".###."];
const SQUARE: &[&str] = &[".....", "#####", "#...#", "#...#", "#...#", "#####"];
const TRIANGLE: &[&str] = &[".....", "..#..", ".#.#.", ".#.#.", "#...#", "#####"];

/// The mouse's outline (`+`), round at both ends, its buttons above the
/// line across it, the wheel between them.
const MOUSE: [&str; 9] = ["..+++..", ".+.+.+.", "+..+..+", "+..+..+", "+++++++", "+.....+", "+.....+", ".+...+.", "..+++.."];

/// Every sign: the device and the key's label as `controls` names it
/// (`Controls::keys`), and its look; the keyboard's and the mouse's
/// first, then the pads'.
const SIGNS: &[(Device, &str, Look)] = &[
    (Device::Keyboard, "UP", Look::Plain(UP)),
    (Device::Keyboard, "DOWN", Look::Plain(DOWN)),
    (Device::Keyboard, "LEFT", Look::Plain(LEFT)),
    (Device::Keyboard, "RIGHT", Look::Plain(RIGHT)),
    (Device::Keyboard, "RETURN", Look::Plain(RETURN)),
    (Device::Keyboard, "KEYPAD ENTER", Look::Plain(RETURN)),
    (Device::Keyboard, "TAB", Look::Plain(TAB)),
    (Device::Keyboard, "LEFT SHIFT", Look::Plain(SHIFT)),
    (Device::Keyboard, "RIGHT SHIFT", Look::Plain(SHIFT)),
    (Device::Keyboard, "BACKSPACE", Look::Plain(BACKSPACE)),
    (Device::Keyboard, "SPACE", Look::Plain(SPACE)),
    (Device::Keyboard, "ESCAPE", Look::Cap("ESC")),
    (Device::Keyboard, "LEFT CTRL", Look::Cap("CTRL")),
    (Device::Keyboard, "RIGHT CTRL", Look::Cap("CTRL")),
    (Device::Keyboard, "LEFT ALT", Look::Cap("ALT")),
    (Device::Keyboard, "RIGHT ALT", Look::Cap("ALT")),
    (Device::Keyboard, "MOUSE LEFT", Look::Mouse(Button::Left, None)),
    (Device::Keyboard, "MOUSE RIGHT", Look::Mouse(Button::Right, None)),
    (Device::Keyboard, "MOUSE MIDDLE", Look::Mouse(Button::Middle, None)),
    (Device::Keyboard, "WHEEL UP", Look::Mouse(Button::Middle, Some(true))),
    (Device::Keyboard, "WHEEL DOWN", Look::Mouse(Button::Middle, Some(false))),
    (Device::Gamepad, "A", Look::Ring('A')),
    (Device::Gamepad, "B", Look::Ring('B')),
    (Device::Gamepad, "X", Look::Ring('X')),
    (Device::Gamepad, "Y", Look::Ring('Y')),
    (Device::Gamepad, "CROSS", Look::Plain(CROSS)),
    (Device::Gamepad, "CIRCLE", Look::Plain(CIRCLE)),
    (Device::Gamepad, "SQUARE", Look::Plain(SQUARE)),
    (Device::Gamepad, "TRIANGLE", Look::Plain(TRIANGLE)),
    (Device::Gamepad, "START", Look::Ring('≡')),
    (Device::Gamepad, "OPTIONS", Look::Ring('≡')),
    (Device::Gamepad, "+", Look::Ring('+')),
    (Device::Gamepad, "-", Look::Ring('-')),
    (Device::Gamepad, "BACK", Look::Cap("BACK")),
    (Device::Gamepad, "SHARE", Look::Cap("SHARE")),
    (Device::Gamepad, "LB", Look::Cap("LB")),
    (Device::Gamepad, "RB", Look::Cap("RB")),
    (Device::Gamepad, "LT", Look::Cap("LT")),
    (Device::Gamepad, "RT", Look::Cap("RT")),
    (Device::Gamepad, "L", Look::Cap("L")),
    (Device::Gamepad, "R", Look::Cap("R")),
    (Device::Gamepad, "ZL", Look::Cap("ZL")),
    (Device::Gamepad, "ZR", Look::Cap("ZR")),
    (Device::Gamepad, "L1", Look::Cap("L1")),
    (Device::Gamepad, "R1", Look::Cap("R1")),
    (Device::Gamepad, "L2", Look::Cap("L2")),
    (Device::Gamepad, "R2", Look::Cap("R2")),
    (Device::Gamepad, "L3", Look::Cap("L3")),
    (Device::Gamepad, "R3", Look::Cap("R3")),
    (Device::Gamepad, "UP", Look::Pad(0)),
    (Device::Gamepad, "DOWN", Look::Pad(1)),
    (Device::Gamepad, "LEFT", Look::Pad(2)),
    (Device::Gamepad, "RIGHT", Look::Pad(3)),
];

/// The keys of letters, digits and F1 to F12, each its small letters in a
/// frame (after `SIGNS`, in this order).
const CAPS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

fn small(ch: char) -> &'static [&'static str; 5] {
    SMALL.iter().find(|(c, _)| *c == ch).map(|(_, g)| g).unwrap_or(&["...", "...", "...", "...", "..."])
}

/// Small letters side by side, a column of air between them.
fn small_line(text: &str) -> [String; 5] {
    std::array::from_fn(|r| text.chars().map(|c| small(c)[r]).collect::<Vec<_>>().join("."))
}

/// Rows padded into a sign of nine, its first the line's row `top`.
fn sign_from(rows: Vec<String>, top: i32) -> Sign {
    let width = rows.iter().map(String::len).max().unwrap_or(0);
    Sign { rows: std::array::from_fn(|r| rows.get(r).map_or_else(|| ".".repeat(width), |row| format!("{row:.<width$}"))), width, top }
}

/// `text` in small letters in a thin frame with cut corners, a voxel of
/// air round them: the line's nine rows.
fn cap(text: &str) -> Sign {
    let inner = small_line(text);
    let w = inner[0].len() + 4;
    let edge = format!(".{}.", "+".repeat(w - 2));
    let side = format!("+{}+", ".".repeat(w - 2));
    let mut rows = vec![edge.clone(), side.clone()];
    rows.extend(inner.iter().map(|r| format!("+.{r}.+")));
    rows.extend([side, edge]);
    sign_from(rows, -1)
}

fn sign_of(look: &Look) -> Sign {
    let rows: Vec<String> = match look {
        Look::Cap(text) => return cap(text),
        Look::Plain(rows) => return sign_from(rows.iter().map(|r| r.to_string()).collect(), 0),
        Look::Ring(ch) => {
            let inner = small(*ch);
            let mut rows = vec!["..+++..".to_string(), ".+...+.".to_string()];
            rows.extend(inner.iter().map(|r| format!("+.{r}.+")));
            rows.extend([".+...+.".to_string(), "..+++..".to_string()]);
            rows
        }
        Look::Mouse(filled, wheel) => {
            let mut rows: Vec<String> = MOUSE.iter().map(|r| r.to_string()).collect();
            let cells: &[(usize, usize)] = match filled {
                Button::Left => &[(2, 1), (1, 2), (2, 2), (1, 3), (2, 3)],
                Button::Right => &[(4, 1), (4, 2), (5, 2), (4, 3), (5, 3)],
                Button::Middle => &[(3, 1), (3, 2), (3, 3)],
            };
            for &(c, r) in cells {
                rows[r].replace_range(c..=c, "#");
            }
            // A wheel turned: a small arrow beside it, up or down.
            if let Some(up) = wheel {
                let arrow = if *up { ["...", ".#.", "###"] } else { ["...", "###", ".#."] };
                for (r, row) in rows.iter_mut().enumerate() {
                    row.push('.');
                    row.push_str(arrow.get(r).copied().unwrap_or("..."));
                }
            }
            rows
        }
        Look::Pad(way) => {
            // A solid cross in the quieter tone, the way pressed its arm in
            // ink.
            let mut rows: Vec<String> = [".......", "..+++..", "..+++..", "+++++++", "+++++++", "+++++++", "..+++..", "..+++.."].iter().map(|r| r.to_string()).collect();
            let cells: &[(usize, usize)] = match way {
                0 => &[(2, 1), (3, 1), (4, 1), (2, 2), (3, 2), (4, 2)],
                1 => &[(2, 6), (3, 6), (4, 6), (2, 7), (3, 7), (4, 7)],
                2 => &[(0, 3), (0, 4), (0, 5), (1, 3), (1, 4), (1, 5)],
                _ => &[(5, 3), (5, 4), (5, 5), (6, 3), (6, 4), (6, 5)],
            };
            for &(c, r) in cells {
                rows[r].replace_range(c..=c, "#");
            }
            rows
        }
    };
    // The ring, the mouse and the d-pad round the capitals as a frame.
    sign_from(rows, -1)
}

/// The keys of `CAPS` and F1 to F12, framed.
fn caps() -> impl Iterator<Item = String> {
    CAPS.chars().map(String::from).chain((1..=12).map(|n| format!("F{n}")))
}

/// Every sign, by its character's place after `SIGN_BASE`, with the
/// device and label it is for.
static ALL: LazyLock<Vec<(Device, String, Sign)>> = LazyLock::new(|| {
    let mut all: Vec<(Device, String, Sign)> = SIGNS.iter().map(|(device, label, look)| (*device, label.to_string(), sign_of(look))).collect();
    all.extend(caps().map(|key| (Device::Keyboard, key.clone(), cap(&key))));
    all
});

/// The sign of a key as `controls` labels it on `device` (a pad's labels
/// already say which pad: A, CROSS), if there is one.
pub fn key_sign(device: Device, label: &str) -> Option<char> {
    let at = ALL.iter().position(|(d, l, _)| *d == device && l == label)?;
    char::from_u32(SIGN_BASE + at as u32)
}

/// A character's sign, if it is one.
pub fn sign(ch: char) -> Option<&'static Sign> {
    ALL.get((ch as u32).checked_sub(SIGN_BASE)? as usize).map(|(_, _, s)| s)
}

/// Whether a character lies where the signs do (one or not): a script's
/// text never carries one.
pub fn in_signs(ch: char) -> bool {
    (SIGN_BASE..SIGN_BASE + 0x100).contains(&(ch as u32))
}

/// Every sign with what it is for, in order: the sheets and the letters'
/// test draw them all.
pub fn every_sign() -> Vec<(Device, &'static str, char)> {
    ALL.iter().enumerate().map(|(i, (d, l, _))| (*d, l.as_str(), char::from_u32(SIGN_BASE + i as u32).unwrap_or(' '))).collect()
}

/// A quieter tone of `color` for a sign's frame or outline: darker where
/// the color is light (light text over dark ground), lighter where it is
/// dark.
pub fn quiet(color: [u8; 4]) -> [u8; 4] {
    let [r, g, b, a] = color;
    let lum = (299 * r as u32 + 587 * g as u32 + 114 * b as u32) / 1000;
    let mix = |v: u8| if lum > 127 { (v as u32 * 70 / 100) as u8 } else { (v as u32 + (255 - v as u32) * 30 / 100) as u8 };
    [mix(r), mix(g), mix(b), a]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every sign stays within the line's nine rows, as wide as its rows
    /// say, and every key `controls` labels with one finds it.
    #[test]
    fn every_sign_fits_the_line() {
        for (device, label, ch) in every_sign() {
            let s = sign(ch).unwrap();
            assert!(s.rows.iter().all(|r| r.len() == s.width && r.chars().all(|c| ".#+".contains(c))), "{label}: {:?}", s.rows);
            assert!(s.rows.iter().any(|r| r.contains('#')), "{label}: no ink");
            assert_eq!(key_sign(device, label), Some(ch));
            assert!(in_signs(ch));
            assert_eq!(crate::ui::text_width(&format!("{ch}A"), 1), s.width as f32 + 6.0, "{label}: measured as drawn");
        }
        assert_eq!(key_sign(Device::Keyboard, "PAGE UP"), None, "no sign: its name");
        assert_ne!(key_sign(Device::Keyboard, "A"), key_sign(Device::Gamepad, "A"), "a key and a button are two signs");
    }
}
