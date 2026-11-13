//! BACKGROUND: the editors' view behind what is shaped, chosen by the
//! person (a black skirt on black was hard to see): one for the voxel
//! editor (a block, a model, a figure) and one for the level editor, kept
//! in the config folder beside the IDE's settings (`ide/settings/
//! editors.toml`), never in a game. None: as always (the game's sky).
//! The marks over the view (handles, the selection's outline, joints)
//! take deeper colors on a light background (`Marks`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The four to choose from: the dark one of before (the editors' sky; in
/// the menu it stands for none, the game's sky as always), a mid gray, a light gray, a warm paper tone. The marks over the
/// view read on each (test `the_marks_read_on_every_preset`).
pub const PRESETS: [[u8; 3]; 4] = [[15, 18, 26], [80, 82, 88], [184, 184, 184], [233, 225, 205]];

/// The settings: each editor's background as `#rrggbb` (none: as before).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Backgrounds {
    pub voxel: Option<String>,
    pub level: Option<String>,
}

impl Backgrounds {
    /// Where they are kept: beside the IDE's settings.
    pub fn path() -> PathBuf {
        crate::settings::config_dir().join("ide").join("settings").join("editors.toml")
    }

    /// The settings at `path` (none yet, or unreadable: as before; the
    /// file is not written over until one is chosen).
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::blocks::save_at_once(path, toml::to_string(self)?.as_bytes()).with_context(|| format!("writing {}", path.display()))
    }

    /// As colors: the level editor's, the voxel editor's.
    pub fn colours(&self) -> [Option<[u8; 3]>; 2] {
        [self.level.as_deref().and_then(hex), self.voxel.as_deref().and_then(hex)]
    }

    pub fn of(colours: [Option<[u8; 3]>; 2]) -> Self {
        Self { level: colours[0].map(to_hex), voxel: colours[1].map(to_hex) }
    }
}

/// `#rrggbb` as a color.
pub fn hex(text: &str) -> Option<[u8; 3]> {
    let t = text.trim().strip_prefix('#')?;
    if t.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(t, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

pub fn to_hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// How bright a color looks (WCAG's relative luminance, 0 to 1).
pub fn luminance(c: [u8; 3]) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// The contrast of two colors (WCAG's ratio, 1 to 21).
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The colors of the marks over the view on a background: the usual ones
/// on a dark one, deeper ones on a light one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marks {
    pub light: bool,
}

impl Marks {
    /// The marks for `background` (none: the usual dark).
    pub fn on(background: Option<[u8; 3]>) -> Self {
        Self { light: background.is_some_and(|c| luminance(c) > 0.25) }
    }

    /// An axis's color (0 X red, 1 Y green, 2 Z blue), lit or not.
    pub fn axis(self, a: usize, lit: bool) -> [u8; 3] {
        const DEEP: [[u8; 3]; 3] = [[176, 32, 22], [28, 116, 28], [28, 70, 188]];
        const DEEP_LIT: [[u8; 3]; 3] = [[120, 10, 0], [10, 80, 10], [10, 40, 130]];
        let c = match (self.light, lit) {
            (false, false) => super::zones::AXIS[a.min(2)],
            (false, true) => super::zones::AXIS_LIT[a.min(2)],
            (true, false) => return DEEP[a.min(2)],
            (true, true) => return DEEP_LIT[a.min(2)],
        };
        [c.r(), c.g(), c.b()]
    }

    /// The gold of what is chosen: the selection's outline, the joint.
    pub fn gold(self) -> [u8; 3] {
        if self.light { [150, 96, 0] } else { [255, 214, 90] }
    }

    /// A quiet line (the fresh box's outline).
    pub fn line(self) -> [u8; 3] {
        if self.light { [70, 64, 56] } else { [225, 220, 205] }
    }

    pub fn colour(c: [u8; 3]) -> egui::Color32 {
        egui::Color32::from_rgb(c[0], c[1], c[2])
    }
}

/// The floor's two tones under `background` (none: the editors' own).
pub fn floor(background: Option<[u8; 3]>) -> [[u8; 4]; 2] {
    match background {
        // (The dark one: the floor as it always was.)
        None => [[58, 56, 66, 255], [66, 64, 74, 255]],
        Some(c) if c == PRESETS[0] => floor(None),
        Some(c) => {
            // A little darker on a light ground, a little lighter on a
            // dark one: there, but quiet.
            let shift = |by: f32| -> [u8; 4] {
                let f = |v: u8| if luminance(c) > 0.25 { v as f32 * (1.0 - by) } else { v as f32 + 255.0 * by };
                [f(c[0]).round().min(255.0) as u8, f(c[1]).round().min(255.0) as u8, f(c[2]).round().min(255.0) as u8, 255]
            };
            [shift(0.14), shift(0.18)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings come back as written; none or a broken file: as before.
    #[test]
    fn the_backgrounds_come_back() {
        let dir = crate::scratch::Scratch::new("backgrounds");
        let path = dir.join("settings").join("editors.toml");
        assert_eq!(Backgrounds::load(&path), Backgrounds::default());
        let b = Backgrounds::of([Some(PRESETS[3]), Some([1, 2, 3])]);
        b.save(&path).unwrap();
        assert_eq!(Backgrounds::load(&path).colours(), [Some(PRESETS[3]), Some([1, 2, 3])]);
        std::fs::write(&path, "voxel = [").unwrap();
        assert_eq!(Backgrounds::load(&path), Backgrounds::default());
        assert_eq!(hex("#0a0b0c"), Some([10, 11, 12]));
        assert_eq!(hex("0a0b0c"), None);
    }

    /// On every preset the marks over the view stand out: the three axes,
    /// lit or not, the gold of the selection and the joint, the quiet
    /// line; and a black model on it.
    #[test]
    fn the_marks_read_on_every_preset() {
        for bg in PRESETS {
            let m = Marks::on(Some(bg));
            let mut marks: Vec<(String, [u8; 3])> = (0..3).flat_map(|a| [(format!("axis {a}"), m.axis(a, false)), (format!("axis {a} lit"), m.axis(a, true))]).collect();
            marks.extend([("gold".to_string(), m.gold()), ("line".into(), m.line())]);
            for (what, c) in marks {
                let r = contrast(c, bg);
                assert!(r >= 2.0, "{what} {c:?} on {bg:?}: contrast {r:.2}");
            }
            // (The dark one is as before; on the others a black model
            // shows.)
            if bg != PRESETS[0] {
                assert!(contrast([0, 0, 0], bg) >= 2.5, "black on {bg:?}");
            }
            // The floor: there but quiet.
            for f in floor(Some(bg)) {
                let r = contrast([f[0], f[1], f[2]], bg);
                assert!((1.1..2.2).contains(&r), "floor {f:?} on {bg:?}: {r:.2}");
            }
        }
    }

    /// A BACKGROUND changes the view's clear color and the floor's tones,
    /// nothing of the model: every other cube the same, the same light; in
    /// GAME VIEW none. The level editor's and the voxel editor's apart.
    #[test]
    fn a_background_changes_only_the_background_and_the_floor() {
        use crate::editor::view::{Action, Figures, Shown, act, frame};
        let (mut ed, mut view, dir) = crate::editor::model::parts::tests::figure("background");
        let figures = Figures::load(&dir, &crate::config::GameConfig::default());
        let light = crate::config::LightConfig::default();
        let shown = Shown::new(&ed).unwrap();
        let before = frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        act(&mut ed, &mut view, Action::Background(Some(PRESETS[3])));
        assert!(view.backgrounds_changed);
        assert_eq!(view.backgrounds, [None, Some(PRESETS[3])], "the voxel editor's");
        let after = frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        assert_eq!(after.sky, PRESETS[3].map(|v| v as f32 / 255.0));
        assert_ne!(before.sky, after.sky);
        assert_eq!(before.instances.len(), after.instances.len());
        let [a, b] = floor(None);
        let [c, d] = floor(Some(PRESETS[3]));
        let mut floors = 0;
        for (x, y) in before.instances.iter().zip(&after.instances) {
            if bytemuck::bytes_of(x) != bytemuck::bytes_of(y) {
                assert!([a, b].contains(&x.color) && [c, d].contains(&y.color), "only the floor changes: {:?} {:?}", x.color, y.color);
                floors += 1;
            }
        }
        assert!(floors > 0, "the floor's tones follow");
        assert_eq!((before.sun_dir, before.ambient, &before.models), (after.sun_dir, after.ambient, &after.models));
        // GAME VIEW: the game's own sky.
        act(&mut ed, &mut view, Action::GameView);
        assert!(view.background(ed.scope).is_none());
    }
}
