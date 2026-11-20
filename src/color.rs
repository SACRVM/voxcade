//! A color as a game writes it, read one way wherever it is written (the
//! Lua calls, `conf.lua`, a level's lights): `"#rrggbb"`, or `{ r, g, b }`
//! with numbers from 0 to 255. Where a color can be see-through (a cube, a
//! text, a box, an icon) a fourth value too: `"#rrggbbaa"` or `{ r, g, b,
//! a }`, 255 opaque; elsewhere a fourth value is a mistake saying why.
//!
//! A fraction in a table is kept where the engine holds the color as
//! fractions (a light, the daylight, the fog, `conf.lua`'s colors) and
//! rounded where it holds bytes (a cube, a text, an icon, a voxel). A table
//! whose every number is at most 1, not all 0, reads as the scale from 0
//! to 1 and is refused, the value rewritten in the mistake: `{ 0, 0, 0 }`
//! is black, `{ 1, 1, 1 }` is no color (white is `{ 255, 255, 255 }`).

/// The rule, said in every mistake about a color that cannot be see-through.
pub const RULE: &str = "a color is \"#rrggbb\" or { r, g, b } with numbers from 0 to 255";

/// The rule where a color may be see-through.
pub const RULE_SEE_THROUGH: &str = "a color is \"#rrggbb\" or { r, g, b } with numbers from 0 to 255, and \"#rrggbbaa\" or { r, g, b, a } to make it see-through (a: 0 clear, 255 opaque)";

/// Whether a color may have a fourth value, and if not, why: said in the
/// mistake.
#[derive(Clone, Copy, Debug)]
pub enum SeeThrough {
    Yes,
    No(&'static str),
}

impl SeeThrough {
    pub fn rule(self) -> &'static str {
        match self {
            SeeThrough::Yes => RULE_SEE_THROUGH,
            SeeThrough::No(_) => RULE,
        }
    }
}

/// A color as it was written, before it is read.
pub enum Written {
    Text(String),
    Numbers(Vec<f64>),
    /// Something else, in words for the mistake ("a boolean", "a table
    /// with named keys").
    Other(String),
}

/// A color read: red, green, blue and alpha from 0 to 255, fractions as
/// written (alpha 255 unless given).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub rgba: [f64; 4],
}

impl Color {
    pub const WHITE: Color = Color { rgba: [255.0; 4] };

    pub fn from_bytes(c: [u8; 4]) -> Self {
        Color { rgba: c.map(f64::from) }
    }

    /// As bytes, each rounded.
    pub fn bytes(self) -> [u8; 4] {
        self.rgba.map(|v| v.round().clamp(0.0, 255.0) as u8)
    }

    pub fn rgb_bytes(self) -> [u8; 3] {
        let [r, g, b, _] = self.bytes();
        [r, g, b]
    }

    /// Red, green and blue from 0 to 1, as the picture takes them.
    pub fn unit(self) -> [f32; 3] {
        [unit(self.rgba[0]), unit(self.rgba[1]), unit(self.rgba[2])]
    }

    /// Whether every value is a whole number (it is a hex color exactly).
    pub fn whole(self) -> bool {
        self.rgba.iter().all(|v| v.fract() == 0.0)
    }

    /// `"#rrggbb"`, or `"#rrggbbaa"` when it is see-through; rounded.
    pub fn hex(self) -> String {
        let [r, g, b, a] = self.bytes();
        if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
    }
}

/// A value from 0 to 255 as the picture's 0 to 1 (whole numbers as
/// `n as f32 / 255.0`, the same f32 to the bit: test
/// `a_whole_number_is_the_same_fraction_either_way`).
pub fn unit(v: f64) -> f32 {
    (v / 255.0) as f32
}

/// The shortest number from 0 to 255 (fewest decimals) that is read back as
/// `v` (`unit`): how a fraction of the picture is written on the scale of
/// 0 to 255 without changing it to the bit.
pub fn on_255(v: f32) -> f64 {
    let exact = f64::from(v) * 255.0;
    for digits in 0..=12 {
        let scale = 10f64.powi(digits);
        let d = (exact * scale).round() / scale;
        if unit(d) == v {
            return d;
        }
    }
    exact
}

/// `"#rrggbb"` or `"#rrggbbaa"` (the `#` needed, either case).
pub fn hex(text: &str) -> Option<[u8; 4]> {
    let h = text.strip_prefix('#').filter(|h| (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit()))?;
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).expect("hex digits");
    Some([byte(0), byte(2), byte(4), if h.len() == 8 { byte(6) } else { 255 }])
}

/// A number as a mistake shows it: whole numbers without a point, others
/// with up to four decimals.
pub fn shown(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{v:.0}");
    }
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A table of numbers as a mistake shows it: `{ 1, 0.5, 0 }`.
pub fn shown_numbers(n: &[f64]) -> String {
    if n.is_empty() {
        return "{}".into();
    }
    format!("{{ {} }}", n.iter().map(|v| shown(*v)).collect::<Vec<_>>().join(", "))
}

/// Whether numbers read as the scale from 0 to 1: every one at most 1, not
/// all 0.
pub fn old_scale(n: &[f64]) -> bool {
    n.iter().all(|v| (0.0..=1.0).contains(v)) && n.iter().any(|v| *v != 0.0)
}

/// A color that is not one, in words.
#[derive(Clone, Debug, PartialEq)]
pub struct Mistake {
    pub text: String,
    /// It was a table on the scale from 0 to 1.
    pub old_scale: bool,
}

impl std::fmt::Display for Mistake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// "a boolean", "an integer": a value's kind with its article.
pub fn a(kind: &str) -> String {
    if kind.starts_with(['a', 'e', 'i', 'o', 'u']) { format!("an {kind}") } else { format!("a {kind}") }
}

/// Reads a color as it was written.
pub fn read(written: Written, see: SeeThrough) -> Result<Color, Mistake> {
    let wrong = |text: String| Mistake { text, old_scale: false };
    let rule = see.rule();
    match written {
        Written::Text(t) => {
            let Some(c) = hex(&t) else { return Err(wrong(format!("\"{t}\" is no color: {rule}"))) };
            if let (SeeThrough::No(why), 9) = (see, t.len()) {
                return Err(wrong(format!("\"{t}\": {why}, so no \"#rrggbbaa\": {rule}")));
            }
            Ok(Color::from_bytes(c))
        }
        Written::Numbers(n) => {
            let shown = shown_numbers(&n);
            match (n.len(), see) {
                (3, _) | (4, SeeThrough::Yes) => {}
                (4, SeeThrough::No(why)) => return Err(wrong(format!("{shown}: {why}, so no fourth number: {rule}"))),
                (count, _) => return Err(wrong(format!("{shown} has {count} number{}: {rule}", if count == 1 { "" } else { "s" }))),
            }
            if let Some(bad) = n.iter().find(|v| !(0.0..=255.0).contains(*v)) {
                return Err(wrong(format!("{shown}: each number is from 0 to 255, not {}", if bad.is_finite() { self::shown(*bad) } else { "a number past them".into() })));
            }
            if old_scale(&n) {
                let scaled: Vec<f64> = n.iter().map(|v| v * 255.0).collect();
                let mut rgba = [255.0; 4];
                rgba[..n.len()].copy_from_slice(&scaled);
                let hex = Color { rgba }.hex();
                return Err(Mistake { text: format!("{shown} reads as numbers from 0 to 1, but a color's are from 0 to 255: write {} or \"{hex}\"", shown_numbers(&scaled)), old_scale: true });
            }
            let mut rgba = [255.0; 4];
            rgba[..n.len()].copy_from_slice(&n);
            Ok(Color { rgba })
        }
        Written::Other(what) => Err(wrong(format!("{rule}, not {what}"))),
    }
}

/// A color in a settings or level file (`toml`) as written.
pub fn written_toml(v: &toml::Value) -> Written {
    match v {
        toml::Value::String(s) => Written::Text(s.clone()),
        toml::Value::Array(a) => match a.iter().map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))).collect::<Option<Vec<f64>>>() {
            Some(n) => Written::Numbers(n),
            None => Written::Other("a list of other things than numbers".into()),
        },
        other => Written::Other(a(other.type_str())),
    }
}

/// The same for `toml_edit`'s items (a level's file).
pub fn written_item(i: &toml_edit::Item) -> Written {
    if let Some(s) = i.as_str() {
        return Written::Text(s.to_string());
    }
    match i.as_array() {
        Some(a) => match a.iter().map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))).collect::<Option<Vec<f64>>>() {
            Some(n) => Written::Numbers(n),
            None => Written::Other("a list of other things than numbers".into()),
        },
        None => Written::Other(a(i.type_name())),
    }
}

/// `serde` for a color setting: read as `read` reads it (the mistakes said
/// with the setting's name by `lua::conf` first), written back as hex when
/// every value is whole, else as numbers.
pub mod setting {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Opaque(pub Color);

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct SeeThrough(pub Color);

    fn de<'de, D: Deserializer<'de>>(d: D, see: super::SeeThrough) -> Result<Color, D::Error> {
        let v = toml::Value::deserialize(d)?;
        read(written_toml(&v), see).map_err(|e| serde::de::Error::custom(e.text))
    }

    fn ser<S: Serializer>(c: &Color, s: S) -> Result<S::Ok, S::Error> {
        if c.whole() { c.hex().serialize(s) } else { c.rgba[..if c.rgba[3] == 255.0 { 3 } else { 4 }].serialize(s) }
    }

    impl<'de> Deserialize<'de> for Opaque {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            de(d, super::SeeThrough::No("it cannot be see-through")).map(Opaque)
        }
    }

    impl Serialize for Opaque {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            ser(&self.0, s)
        }
    }

    impl<'de> Deserialize<'de> for SeeThrough {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            de(d, super::SeeThrough::Yes).map(SeeThrough)
        }
    }

    impl Serialize for SeeThrough {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            ser(&self.0, s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rd(w: Written) -> Result<Color, Mistake> {
        read(w, SeeThrough::Yes)
    }

    /// Hex and numbers read as the same color; a fraction is kept, a
    /// fourth value is alpha.
    #[test]
    fn a_color_is_hex_or_numbers() {
        assert_eq!(rd(Written::Text("#ff8000".into())).unwrap().bytes(), [255, 128, 0, 255]);
        assert_eq!(rd(Written::Text("#FF800080".into())).unwrap().bytes(), [255, 128, 0, 128]);
        assert_eq!(rd(Written::Numbers(vec![255.0, 128.0, 0.0])).unwrap().bytes(), [255, 128, 0, 255]);
        assert_eq!(rd(Written::Numbers(vec![255.0, 127.5, 0.0])).unwrap().rgba, [255.0, 127.5, 0.0, 255.0]);
        assert_eq!(rd(Written::Numbers(vec![255.0, 127.5, 0.0])).unwrap().bytes(), [255, 128, 0, 255], "rounded for bytes");
        assert_eq!(rd(Written::Numbers(vec![0.0, 0.0, 0.0, 100.0])).unwrap().bytes(), [0, 0, 0, 100]);
        assert_eq!(rd(Written::Numbers(vec![0.0, 0.0, 0.0])).unwrap().bytes(), [0, 0, 0, 255], "black is black");
    }

    /// Each wrong color says what it is and the rule.
    #[test]
    fn a_wrong_color_says_why() {
        let no = SeeThrough::No("a light has no transparency");
        let say = |w: Written, see: SeeThrough| read(w, see).unwrap_err().text;
        assert_eq!(say(Written::Text("orange".into()), no), format!("\"orange\" is no color: {RULE}"));
        assert_eq!(say(Written::Text("ff8000".into()), no), format!("\"ff8000\" is no color: {RULE}"));
        assert_eq!(say(Written::Text("#ff800080".into()), no), format!("\"#ff800080\": a light has no transparency, so no \"#rrggbbaa\": {RULE}"));
        assert_eq!(say(Written::Numbers(vec![255.0, 0.0, 0.0, 9.0]), no), format!("{{ 255, 0, 0, 9 }}: a light has no transparency, so no fourth number: {RULE}"));
        assert_eq!(say(Written::Numbers(vec![255.0, 0.0]), SeeThrough::Yes), format!("{{ 255, 0 }} has 2 numbers: {RULE_SEE_THROUGH}"));
        assert_eq!(say(Written::Numbers(vec![255.0, 0.0, 0.0, 0.0, 0.0]), no), format!("{{ 255, 0, 0, 0, 0 }} has 5 numbers: {RULE}"));
        assert_eq!(say(Written::Numbers(vec![300.0, 0.0, 0.0]), no), "{ 300, 0, 0 }: each number is from 0 to 255, not 300");
        assert_eq!(say(Written::Numbers(vec![-1.0, 0.0, 0.0]), no), "{ -1, 0, 0 }: each number is from 0 to 255, not -1");
        assert_eq!(say(Written::Other("a boolean".into()), no), format!("{RULE}, not a boolean"));
    }

    /// A table on the scale from 0 to 1 is refused, rewritten in the
    /// mistake; black is black, and `{ 1, 1, 1 }` is no color.
    #[test]
    fn the_scale_from_0_to_1_is_refused_with_the_value_rewritten() {
        let m = rd(Written::Numbers(vec![1.0, 0.5, 0.0])).unwrap_err();
        assert!(m.old_scale);
        assert_eq!(m.text, "{ 1, 0.5, 0 } reads as numbers from 0 to 1, but a color's are from 0 to 255: write { 255, 127.5, 0 } or \"#ff8000\"");
        let m = rd(Written::Numbers(vec![1.0, 1.0, 1.0])).unwrap_err();
        assert_eq!(m.text, "{ 1, 1, 1 } reads as numbers from 0 to 1, but a color's are from 0 to 255: write { 255, 255, 255 } or \"#ffffff\"");
        let m = rd(Written::Numbers(vec![0.06, 0.07, 0.1])).unwrap_err();
        assert_eq!(m.text, "{ 0.06, 0.07, 0.1 } reads as numbers from 0 to 1, but a color's are from 0 to 255: write { 15.3, 17.85, 25.5 } or \"#0f121a\"");
        assert!(rd(Written::Numbers(vec![0.0, 0.0, 0.0])).is_ok());
        assert!(rd(Written::Numbers(vec![2.0, 0.0, 0.0])).is_ok(), "above 1: the scale of 0 to 255");
    }

    /// A whole number is the same fraction of the picture whether divided
    /// as f32 (how the engine read hex and Lua's whole numbers) or as f64
    /// (`unit`), so reading through `unit` changes no picture.
    #[test]
    fn a_whole_number_is_the_same_fraction_either_way() {
        for n in 0..=255u8 {
            assert_eq!(unit(f64::from(n)), f32::from(n) / 255.0, "{n}");
        }
    }

    /// Every fraction a setting held is written on the scale of 0 to 255
    /// so that it reads back to the bit, in few digits.
    #[test]
    fn a_fraction_is_written_on_255_and_read_back_to_the_bit() {
        assert_eq!(on_255(0.06), 15.3);
        assert_eq!(on_255(1.0), 255.0);
        assert_eq!(on_255(0.45), 114.75);
        for v in [0.06f32, 0.07, 0.1, 0.45, 0.62, 0.85, 0.01, 0.03, 0.02, 0.04, 0.09, 0.05, 0.86, 0.78, 0.3, 0.32, 0.42, 0.95, 0.48, 0.56, 0.7] {
            assert_eq!(unit(on_255(v)), v, "{v}");
        }
        for i in 0..=10_000 {
            let v = i as f32 / 10_000.0;
            assert_eq!(unit(on_255(v)), v, "{v}");
        }
    }
}
