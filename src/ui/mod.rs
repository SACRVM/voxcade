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

// The parts: a game's menus drawn from its HUD (`hud`), big letters and
// the title's logo (`logo`), the engine's menus (`menus`), the panels over
// the game (`panels`), the pixel font (`pixel`).
mod hud;
mod logo;
mod menus;
mod panels;
mod pixel;

pub use hud::*;
pub use logo::*;
pub use menus::*;
pub use panels::*;
pub use pixel::*;

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
    let view = look_at_mat4(eye(layout, slant, front), middle, Vec3::Y);
    perspective(2.0 * slant.atan(), w / h, 1.0, distance + 400.0) * view
}

/// Where the UI's camera (`camera`) looks from: in front of the middle of
/// the screen.
pub fn eye(layout: &Layout, slant: f32, front: f32) -> Vec3 {
    let (w, h) = (layout.width.max(1.0), layout.height.max(1.0));
    let distance = h / 2.0 / slant.max(0.01);
    Vec3::new(w / 2.0, h / 2.0, front) + Vec3::Z * distance
}

pub fn voxel(out: &mut Vec<Instance>, x: f32, y: f32, z: f32, color: [u8; 4]) {
    out.push(Instance { pos_size: [x + 0.5, y + 0.5, z + 0.5, 1.0], color, fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] });
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
        let top = |v: &Vec<Instance>| v.iter().map(|i| i.pos_size[1]).fold(f32::MIN, f32::max);
        assert!(top(&tall) == top(&plain) && tall.len() != plain.len(), "an Ü is a U's height, the dots inside it");
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


    /// A hint wider than the screen goes on over more lines, each within
    /// the room, whatever the page, device and language.
    #[test]
    fn hints_stay_within_the_screen() {
        let cols = (Layout::of(&UiConfig::default(), 16.0 / 9.0).width / 6.0) as usize - 2;
        let mut broke = 0;
        for lang in ["en", "de", "fr"] {
            for device in [Device::Keyboard, Device::Gamepad] {
                for page in [Page::Title, Page::Initials, Page::HighScores, Page::Join, Page::Code, Page::Joining, Page::Settings] {
                    let mut menu = Menu::title();
                    menu.device = device;
                    let hint = hint_text(&menu, page, lang);
                    let lines = wrap_hint(&hint, cols);
                    broke += (lines.len() > 1) as usize;
                    for l in &lines {
                        assert!(l.chars().count() <= cols, "{lang} {page:?} {device:?}: '{l}' is too wide");
                    }
                    assert_eq!(lines.join(" ").split_whitespace().collect::<Vec<_>>(), hint.split_whitespace().collect::<Vec<_>>());
                }
            }
        }
        assert!(broke > 0, "no hint needed breaking");
        assert_eq!(wrap_hint("ENTER: SELECT   ESC: BACK", 53), vec!["ENTER: SELECT   ESC: BACK"]);
    }

    /// An accented capital stays within the capitals' height (a menu row has
    /// no room above it): its ink spans the plain letter's rows, accent and
    /// body apart by a row of air, in both the pixel font and `letter`.
    #[test]
    fn an_accented_capital_keeps_the_height_of_the_plain_one() {
        for scale in [1, 2, 3] {
            for (acc, plain) in [('Ü', 'U'), ('Ä', 'A'), ('Ö', 'O'), ('É', 'E'), ('È', 'E'), ('À', 'A'), ('Ê', 'E')] {
                let (a, p) = (pixel_glyph(acc, scale), pixel_glyph(plain, scale));
                assert_eq!((a.ascent, a.rows.len()), (p.ascent, p.rows.len()), "{acc} at {scale}");
                let ink: Vec<bool> = a.rows.iter().map(|r| r.iter().any(|&v| v)).collect();
                let pink: Vec<bool> = p.rows.iter().map(|r| r.iter().any(|&v| v)).collect();
                assert_eq!(ink.iter().position(|&v| v), pink.iter().position(|&v| v), "{acc} rises above {plain}");
                assert_eq!(ink.iter().rposition(|&v| v), pink.iter().rposition(|&v| v));
                // Dots stand clear of the body by a row of air; a stroke sits on it.
                let first_gap = ink.iter().position(|&v| !v);
                if matches!(acc, 'Ü' | 'Ä' | 'Ö') {
                    let gap = first_gap.unwrap();
                    assert_eq!(gap, scale as usize, "{acc}'s dots are not one row");
                    assert!(ink[gap..].iter().any(|&v| v), "{acc} has no body under its dots");
                } else {
                    assert!(first_gap.is_none_or(|g| g > 2 * scale as usize - 1), "{acc}'s stroke is not two rows");
                }
                assert_ne!(a.rows, p.rows, "{acc} is its plain letter");
            }
        }
    }
}
