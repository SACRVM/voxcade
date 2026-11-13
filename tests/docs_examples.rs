//! The examples in `docs/tiles.md` are read by the engine's own readers, so
//! the documentation cannot rot: a level, a map or a model written there
//! must load. A fenced `toml` block that names no format is a piece of a
//! level (the terrain's) and is read as the level's body.

use voxcade::grid::GridLevel;
use voxcade::map::MapData;

/// The fenced blocks of a markdown text: the language and the lines, with
/// the line the block begins on.
fn fenced(text: &str) -> Vec<(String, String, usize)> {
    let mut blocks = Vec::new();
    let mut open: Option<(String, String, usize)> = None;
    for (n, line) in text.lines().enumerate() {
        match (&mut open, line.strip_prefix("```")) {
            (None, Some(lang)) => open = Some((lang.trim().to_string(), String::new(), n + 1)),
            (Some(_), Some(_)) => blocks.push(open.take().unwrap()),
            (Some((_, body, _)), None) => {
                body.push_str(line);
                body.push('\n');
            }
            (None, None) => {}
        }
    }
    assert!(open.is_none(), "a fenced block is never closed");
    blocks
}

#[test]
fn the_examples_in_the_docs_load() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/tiles.md")).unwrap();
    let mut read = 0;
    for (lang, body, line) in fenced(&text) {
        if lang != "toml" {
            continue;
        }
        let first = body.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).unwrap_or_default().replace(' ', "");
        let result = if first.starts_with("format=\"voxcadelevel") {
            GridLevel::from_text(&body).map(|_| ())
        } else if first.starts_with("format=\"voxcademap") {
            MapData::read(&body).map(|_| ())
        } else if first.starts_with("format=") {
            panic!("docs/tiles.md line {line}: an example of a format this test does not know: {first}");
        } else {
            let whole = format!("format = \"voxcade level 1\"\nsize = [96, 96, 64]\n{body}");
            GridLevel::from_text(&whole).map(|_| ())
        };
        if let Err(e) = result {
            panic!("the example at docs/tiles.md line {line} does not load: {e:#}");
        }
        read += 1;
    }
    assert!(read >= 2, "the examples of docs/tiles.md were not found ({read})");
}
