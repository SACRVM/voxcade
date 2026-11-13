//! A level's text (`voxcade level 1`): reading, writing, rewriting only
//! what changed (see the module's doc in `grid`).

use super::*;
use crate::vvx::{self, ONE_SIGN, Reader, SIGNS};
use toml_edit::{Document, Item};

/// The level's text, keeping the view and the signs of `kept` (the
/// legend the file had; `Legend::default()` for a new one).
pub fn write(level: &GridLevel, kept: &Legend) -> String {
    if !level.has_grid() {
        let s = level.size;
        let mut out = format!("format = \"{FORMAT}\"\nsize = [{}, {}, {}]    # voxels: across (x), deep (y, 0 in front), up (z); no grid of blocks\n", s.x, s.y, s.z);
        if let Some(d) = &level.daylight {
            out += &format!("\n{}", daylight_text(d));
        }
        if let Some(t) = &level.terrain {
            out += &format!("\n{}", crate::terrain::text(t));
        }
        write_set_down(level, &mut out);
        return out;
    }
    let view = kept.view.unwrap_or_else(|| View::for_size(level.size));
    // The pairs of block and turn the cells hold, by first use in
    // reading order.
    let order = reading_order(level.size, view);
    let mut pairs: Vec<Cell> = Vec::new();
    for c in order.iter().flatten().flatten() {
        let cell = level.cells[*c];
        if cell.block != 0 && !pairs.contains(&cell) {
            pairs.push(cell);
        }
    }
    let name = |c: &Cell| level.blocks[c.block as usize - 1].name.clone();
    let width = if pairs.len() <= ONE_SIGN { 1 } else { 2 };
    let kept: Vec<&(String, String, u8)> = kept.signs.iter().filter(|(s, ..)| s.len() == width).collect();
    let mut taken: Vec<String> = kept.iter().map(|(s, ..)| s.clone()).collect();
    let mut sign_of: Vec<(Cell, String)> = Vec::new();
    // Kept signs first, in the file's order.
    for (s, block, turn) in &kept {
        if let Some(c) = pairs.iter().find(|c| name(c) == *block && c.turn.number() == *turn)
            && !sign_of.iter().any(|(d, _)| d == c)
        {
            sign_of.push((*c, s.clone()));
        }
    }
    let kept_count = sign_of.len();
    for c in &pairs {
        if sign_of.iter().any(|(d, _)| d == c) {
            continue;
        }
        let s = new_sign(&name(c), width, &taken);
        taken.push(s.clone());
        sign_of.push((*c, s));
    }
    // The legend's order: the kept ones as the file had them, then the
    // new ones as they came.
    let mut legend = sign_of[..kept_count].to_vec();
    legend.extend(sign_of[kept_count..].iter().cloned());
    let s = level.size;
    let mut out = format!("format = \"{FORMAT}\"\nblock = {}            # a block is this many voxels each way\nsize = [{}, {}, {}]    # cells: across (x), deep (y, 0 in front), up (z)\n", level.n, s.x, s.y, s.z);
    out += &match view {
        View::Front => "view = \"front\"        # each layer the grid seen from the front, the front layer first, its top row first\n".to_string(),
        View::Above => "view = \"above\"        # each layer the grid seen from above, the bottom layer first, its far row first\n".to_string(),
    };
    out += "\n[blocks]              # blocks/NAME.vvx, each what it is to the game\n";
    for b in &level.blocks {
        out += &format!("{} = \"{}\"\n", key(&b.name), b.kind.name());
    }
    out += &format!("\n{LEGEND_HEAD}\n");
    for (c, sign) in &legend {
        match c.turn.number() {
            0 => out += &format!("{sign} = {}\n", vvx::quoted(&name(c))),
            t => out += &format!("{sign} = {{ block = {}, orientation = {t} }}\n", vvx::quoted(&name(c))),
        }
    }
    let dots = ".".repeat(width);
    let layers: Vec<Vec<String>> = order
        .iter()
        .map(|layer| {
            layer
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|&i| {
                            let cell = level.cells[i];
                            if cell.block == 0 { dots.as_str() } else { sign_of.iter().find(|(d, _)| *d == cell).map_or("?", |(_, s)| s.as_str()) }
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    out += "\n[cells]\n";
    out += &vvx::layers_text(&layers);
    if let Some(d) = &level.daylight {
        out += &format!("\n{}", daylight_text(d));
    }
    if let Some(t) = &level.terrain {
        out += &format!("\n{}", crate::terrain::text(t));
    }
    write_set_down(level, &mut out);
    out
}

/// The things, places and areas, in their order: each field as the
/// sketch in `docs/tiles.md` writes it, a field left out when the file
/// left it out.
fn write_set_down(level: &GridLevel, out: &mut String) {
    for t in &level.things {
        *out += &format!("\n{}", thing_text(t));
    }
    for p in &level.places {
        *out += &format!("\n{}", place_text(p));
    }
    for a in &level.areas {
        *out += &format!("\n{}", area_text(a));
    }
}

/// Three whole numbers as the file writes them.
fn triple([x, y, z]: [i32; 3]) -> String {
    format!("[{x}, {y}, {z}]")
}

/// The way a thing faces as its line says it: the key and the value.
fn facing_text(f: Facing) -> (&'static str, String) {
    match f {
        Facing::Way(w) => ("facing", format!("\"{}\"", w.name())),
        Facing::Turn(turn) => ("rotation", format!("{turn}")),
    }
}

/// A light as its line says it (the value after `light = `).
fn light_text(l: &ThingLight) -> String {
    let mut parts = Vec::new();
    if let Some(at) = l.at {
        parts.push(format!("at = {}", triple(at)));
    }
    parts.push(format!("radius = {}", l.radius));
    if let Some(c) = &l.color {
        parts.push(format!("color = {}", vvx::quoted(c)));
    }
    format!("{{ {} }}", parts.join(", "))
}

/// Several lights as their lines say them (the value after `lights = `),
/// one a line.
fn lights_text(lights: &[ThingLight]) -> String {
    let mut out = "[\n".to_string();
    for l in lights {
        out += &format!("  {},\n", light_text(l));
    }
    out + "]"
}

/// A thing's lights as its line says them: `light = {...}` for one,
/// `lights = [...]` for more; the key and the value.
fn lights_field(lights: &[ThingLight]) -> Option<(&'static str, String)> {
    match lights {
        [] => None,
        [one] => Some(("light", light_text(one))),
        more => Some(("lights", lights_text(more))),
    }
}

/// A number as the file writes it: whole without a point, else as Rust
/// prints it.
fn number(v: f64) -> String {
    format!("{v}")
}

/// A level's daylight as its table, `[light]` and its lines, each word
/// the file gave.
fn daylight_text(d: &Daylight) -> String {
    let mut out = "[light]               # this level's daylight; what it leaves out is the game's c.light\n".to_string();
    if let Some([x, y, z]) = d.sun {
        out += &format!("sun = [{}, {}, {}]\n", number(x), number(y), number(z));
    }
    if let Some(c) = &d.sun_color {
        out += &format!("sun_color = {}\n", vvx::quoted(c));
    }
    if let Some(v) = d.ambient {
        out += &format!("ambient = {}\n", number(v));
    }
    if let Some(v) = d.shadow {
        out += &format!("shadow = {}\n", number(v));
    }
    if let Some(c) = &d.sky {
        out += &format!("sky = {}\n", vvx::quoted(c));
    }
    out
}

/// A place's facing as its line says it: the way's name where the file
/// named it, else the step.
fn place_facing_text(p: &Place, pair: [i32; 2]) -> String {
    match Way::of_pair(pair) {
        Some(w) if p.named => format!("\"{}\"", w.name()),
        _ => format!("[{}, {}]", pair[0], pair[1]),
    }
}

/// A thing as its table, `[[thing]]` and its lines, each field as the
/// sketch writes it, a field left out when the file left it out.
fn thing_text(t: &Thing) -> String {
    let mut out = "[[thing]]\n".to_string();
    if let Some(name) = &t.name {
        out += &format!("name = {}\n", vvx::quoted(name));
    }
    // A piece says what it is by its model or its light.
    if !t.is_piece() {
        out += &format!("kind = {}\n", vvx::quoted(&t.kind));
    }
    if let Some(model) = &t.model {
        out += &format!("model = {}\n", vvx::quoted(model));
    }
    out += &format!("at = {}\n", triple(t.at));
    if let Some(f) = t.facing {
        let (key, value) = facing_text(f);
        out += &format!("{key} = {value}\n");
    }
    if t.pitch != 0.0 {
        out += &format!("pitch = {}\n", number(t.pitch));
    }
    if t.roll != 0.0 {
        out += &format!("roll = {}\n", number(t.roll));
    }
    if t.solid {
        out += "solid = true\n";
    }
    if let Some(n) = t.scatter {
        out += &format!("scatter = {n}\n");
    }
    if let Some((key, value)) = lights_field(&t.lights) {
        out += &format!("{key} = {value}\n");
    }
    out
}

fn place_text(p: &Place) -> String {
    let mut out = format!("[[place]]\nname = {}\nat = {}\n", vvx::quoted(&p.name), triple(p.at));
    if let Some(pair) = p.facing {
        out += &format!("facing = {}\n", place_facing_text(p, pair));
    }
    out
}

fn area_text(a: &Area) -> String {
    format!("[[area]]\nname = {}\nat = {}\nsize = {}\n", vvx::quoted(&a.name), triple(a.at), triple(a.size))
}

/// The level `now` written over the text it was read from (`src`, which
/// says `was`), so that what did not change stays as it was, to the
/// character, comments too: a changed field is its value changed on
/// its line, a field come or gone its line; a new thing, place or area
/// comes after the last of its sort, one taken away goes with its
/// lines (and the comments right above it). A grid that changed is
/// written anew with the signs of `kept` (as `write`). Where the text
/// cannot be followed so (a text that does not read, things in another
/// order than the file's), the level is written whole (`write`).
pub fn rewrite(src: &str, was: &GridLevel, now: &GridLevel, kept: &Legend) -> String {
    if was == now && GridLevel::from_text(src).is_ok_and(|l| l == *was) {
        return src.to_string();
    }
    over(src, was, now, kept).unwrap_or_else(|| write(now, kept))
}

/// The sorts of what is set down, as the file names their tables.
const SORTS: [&str; 3] = ["thing", "place", "area"];

/// One `[[thing]]`, `[[place]]` or `[[area]]` of a text: its sort (its
/// place in `SORTS`), its place among those of its sort, where its
/// text begins (the comments right above its header with it), where
/// its header is, and its table.
struct Section<'a> {
    sort: usize,
    k: usize,
    start: usize,
    header: usize,
    table: &'a dyn toml_edit::TableLike,
}

/// `rewrite`, or None where it cannot follow the text.
fn over(src: &str, was: &GridLevel, now: &GridLevel, kept: &Legend) -> Option<String> {
    if was.has_grid() != now.has_grid() || !src.ends_with('\n') {
        return None;
    }
    let doc = Document::parse(src).ok()?;
    let top = doc.as_table();
    let line_begin = |at: usize| src[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |at: usize| src[at.min(src.len())..].find('\n').map_or(src.len(), |i| at + i + 1);
    // Every section with where its text begins.
    let mut sections: Vec<Section> = Vec::new();
    for (sort, key) in SORTS.iter().enumerate() {
        let Some(item) = top.get(key) else { continue };
        let tables = item.as_array_of_tables()?;
        for (k, t) in tables.iter().enumerate() {
            let first = t.iter().filter_map(|(key, v)| t.key(key).and_then(|k| k.span()).or_else(|| v.span())).map(|s| s.start).min()?;
            // Its header: the nearest line above that begins with [[.
            let mut header = line_begin(first);
            while !src[header..].trim_start().starts_with(&format!("[[{key}]]")) {
                if header == 0 {
                    return None;
                }
                header = line_begin(header - 1);
            }
            // The comments right above it are its.
            let mut start = header;
            while start > 0 {
                let above = line_begin(start - 1);
                if !src[above..start].trim_start().starts_with('#') {
                    break;
                }
                start = above;
            }
            sections.push(Section { sort, k, start, header, table: t as &dyn toml_edit::TableLike });
        }
    }
    sections.sort_by_key(|s| s.header);
    // Past the first section only sections' headers.
    if let Some(first) = sections.first() {
        let mut at = first.header;
        while at < src.len() {
            let line = src[at..line_end(at)].trim_start();
            if line.starts_with('[') && !SORTS.iter().any(|key| line.starts_with(&format!("[[{key}]]"))) {
                return None;
            }
            at = line_end(at);
        }
    }
    let ends: Vec<usize> = (0..sections.len()).map(|i| sections.get(i + 1).map_or(src.len(), |s| s.start)).collect();
    let head_end = sections.first().map_or(src.len(), |s| s.start);
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();

    // The grid, or a level's size.
    let same_head = was.n == now.n && was.size == now.size && was.blocks == now.blocks && was.cells == now.cells;
    if !same_head {
        if now.has_grid() {
            let bare = GridLevel { things: Vec::new(), places: Vec::new(), areas: Vec::new(), ..now.clone() };
            let gap = if sections.is_empty() { "" } else { "\n" };
            edits.push((0..head_end, format!("{}{gap}", write(&bare, kept))));
        } else {
            let span = top.get("size")?.span()?;
            let s = now.size;
            edits.push((span, triple(s.to_array())));
        }
    }

    // The level's own daylight: its table changed, put in before what
    // is set down, or taken out (a grid written anew brought it along).
    if was.daylight != now.daylight && (same_head || !now.has_grid()) {
        let mut header = None;
        let mut at = 0;
        while at < head_end {
            if src[at..line_end(at)].trim_start().starts_with("[light]") {
                header = Some(at);
                break;
            }
            at = line_end(at);
        }
        match (header, &now.daylight) {
            (Some(h), day) => {
                // Its lines: to the next table, its blank lines last.
                let mut end = line_end(h);
                let mut body_end = end;
                while end < head_end && !src[end..line_end(end)].trim_start().starts_with('[') {
                    if !src[end..line_end(end)].trim().is_empty() {
                        body_end = line_end(end);
                    }
                    end = line_end(end);
                }
                match day {
                    Some(d) => edits.push((h..body_end, daylight_text(d))),
                    None => {
                        let mut start = h;
                        if end >= src.len() {
                            while start > 0 && src[line_begin(start - 1)..start].trim().is_empty() {
                                start = line_begin(start - 1);
                            }
                        }
                        edits.push((start..end, String::new()));
                    }
                }
            }
            (None, Some(d)) if sections.is_empty() => edits.push((src.len()..src.len(), format!("\n{}", daylight_text(d)))),
            (None, Some(d)) => edits.push((head_end..head_end, format!("{}\n", daylight_text(d)))),
            (None, None) => {}
        }
    }

    // The terrain: its fields and materials changed in place, put in
    // before what is set down, or taken out.
    if was.terrain != now.terrain && (same_head || !now.has_grid()) {
        terrain_edits(src, top, head_end, sections.is_empty(), was.terrain.as_ref(), now.terrain.as_ref(), &mut edits)?;
    }

    // Each sort: what became of the file's own, and the new ones after
    // the last of them.
    let lines: [Vec<usize>; 3] = [was.things.iter().map(|t| t.line).collect(), was.places.iter().map(|p| p.line).collect(), was.areas.iter().map(|a| a.line).collect()];
    let now_lines: [Vec<usize>; 3] = [now.things.iter().map(|t| t.line).collect(), now.places.iter().map(|p| p.line).collect(), now.areas.iter().map(|a| a.line).collect()];
    let mut appended = String::new();
    for sort in 0..3 {
        // The file's own (their place among the was's), then the new.
        let of = |line: usize| (line > 0).then(|| lines[sort].iter().position(|&l| l == line)).flatten();
        let origins: Vec<Option<usize>> = now_lines[sort].iter().map(|&l| of(l)).collect();
        let own: Vec<usize> = origins.iter().flatten().copied().collect();
        if own.windows(2).any(|w| w[0] >= w[1]) || origins.iter().skip_while(|o| o.is_some()).any(Option::is_some) {
            return None;
        }
        let of_sort: Vec<usize> = (0..sections.len()).filter(|&i| sections[i].sort == sort).collect();
        if of_sort.len() != lines[sort].len() {
            return None;
        }
        for (k, &i) in of_sort.iter().enumerate() {
            let s = &sections[i];
            debug_assert_eq!(s.k, k);
            match origins.iter().position(|o| *o == Some(k)) {
                // Taken away: its text goes; the last one's blank lines
                // above too.
                None => {
                    let mut start = s.start;
                    if ends[i] == src.len() {
                        while start > 0 && src[line_begin(start - 1)..start].trim().is_empty() {
                            start = line_begin(start - 1);
                        }
                    }
                    edits.push((start..ends[i], String::new()));
                }
                Some(j) => {
                    let changed = match sort {
                        0 => thing_edits(src, s, &was.things[k], &now.things[j], &mut edits),
                        1 => place_edits(src, s, &was.places[k], &now.places[j], &mut edits),
                        _ => area_edits(src, s, &was.areas[k], &now.areas[j], &mut edits),
                    };
                    if changed.is_none() {
                        // Written anew from its header on (its comments
                        // above kept).
                        let body = match sort {
                            0 => thing_text(&now.things[j]),
                            1 => place_text(&now.places[j]),
                            _ => area_text(&now.areas[j]),
                        };
                        let end = s.table.iter().filter_map(|(_, v)| v.span()).map(|v| line_end(v.end)).max()?;
                        edits.push((s.header..end, body));
                    }
                }
            }
        }
        // The new ones: after the last of their sort, else at the end.
        let new: Vec<String> = origins
            .iter()
            .enumerate()
            .filter(|(_, o)| o.is_none())
            .map(|(j, _)| match sort {
                0 => thing_text(&now.things[j]),
                1 => place_text(&now.places[j]),
                _ => area_text(&now.areas[j]),
            })
            .collect();
        if new.is_empty() {
            continue;
        }
        match of_sort.last().map(|&i| ends[i]).filter(|&e| e < src.len()) {
            Some(at) => edits.push((at..at, new.iter().map(|t| format!("{t}\n")).collect())),
            None => appended += &new.iter().map(|t| format!("\n{t}")).collect::<String>(),
        }
    }
    if !appended.is_empty() {
        edits.push((src.len()..src.len(), appended));
    }
    // From the last to the first, so that the places stay right; where
    // one is taken away and another put at its start, the taking first.
    edits.sort_by_key(|(r, _)| (r.start, r.end));
    if edits.windows(2).any(|w| w[0].0.end > w[1].0.start) {
        return None;
    }
    let mut out = src.to_string();
    for (range, with) in edits.into_iter().rev() {
        out.replace_range(range, &with);
    }
    Some(out)
}

/// The edits that make the file's `[terrain]` (in the text before
/// `head_end`) say `now` instead of `was`. None where the text cannot
/// be followed.
fn terrain_edits(src: &str, top: &toml_edit::Table, head_end: usize, alone: bool, was: Option<&crate::terrain::Terrain>, now: Option<&crate::terrain::Terrain>, edits: &mut Vec<(std::ops::Range<usize>, String)>) -> Option<()> {
    use crate::terrain;
    let line_begin = |at: usize| src[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |at: usize| src[at.min(src.len())..].find('\n').map_or(src.len(), |i| at + i + 1);
    let mut header = None;
    let mut at = 0;
    while at < head_end {
        if src[at..line_end(at)].trim_start().starts_with("[terrain]") {
            header = Some(at);
            break;
        }
        at = line_end(at);
    }
    // Its lines: to the next table that is not its.
    let end = match header {
        None => 0,
        Some(h) => {
            let mut end = line_end(h);
            while end < head_end {
                let line = src[end..line_end(end)].trim_start();
                if line.starts_with('[') && !line.starts_with("[[terrain.material]]") && !line.starts_with("[[terrain.layer]]") && !line.starts_with("[[terrain.scatter]]") {
                    break;
                }
                end = line_end(end);
            }
            end
        }
    };
    match (header, now) {
        (Some(h), Some(now)) => {
            let was = was?;
            let table = top.get("terrain")?.as_table_like()?;
            let sec = Section { sort: 0, k: 0, start: h, header: h, table };
            let was_f = terrain::fields(was);
            let now_f = terrain::fields(now);
            for key in ["spacing", "at", "heights", "shapes", "terrace", "rough", "roughness", "dip", "dip_to", "fold", "fault", "seed", "under"] {
                let get = |f: &Vec<(&str, String)>| f.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
                let (a, b) = (get(&was_f), get(&now_f));
                if a != b {
                    let after = match key {
                        "at" => "spacing",
                        "terrace" | "rough" | "roughness" | "dip" | "dip_to" | "fold" | "fault" if spans(table, "shapes").is_some() => "shapes",
                        _ => "heights",
                    };
                    field(src, &sec, key, b, (spans(table, after).is_some() && key != after).then_some(after), edits)?;
                }
            }
            // The materials, then the layers before them (where none
            // were, put in before the first material).
            let mats = vvx::tables(table.get("material")?)?;
            let mat_headers = headers_of(src, &mats, "[[terrain.material]]")?;
            let fields = |l: &[crate::terrain::Material]| l.iter().map(terrain::material_fields).collect::<Vec<_>>();
            let texts: Vec<String> = now.materials.iter().map(terrain::material_text).collect();
            list_edits(src, &mats, &mat_headers, &fields(&was.materials), &fields(&now.materials), &texts, 0, edits)?;
            let layers = table.get("layer").map_or(Some(Vec::new()), vvx::tables)?;
            let layer_headers = headers_of(src, &layers, "[[terrain.layer]]")?;
            let fields = |l: &[crate::terrain::Layer]| l.iter().map(terrain::layer_fields).collect::<Vec<_>>();
            let texts: Vec<String> = now.layers.iter().map(terrain::layer_text).collect();
            list_edits(src, &layers, &layer_headers, &fields(&was.layers), &fields(&now.layers), &texts, *mat_headers.first()?, edits)?;
            // The scatter rules, after the materials.
            let rules = table.get("scatter").map_or(Some(Vec::new()), vvx::tables)?;
            let rule_headers = headers_of(src, &rules, "[[terrain.scatter]]")?;
            let after_materials = mats.last()?.iter().filter_map(|(_, v)| v.span()).map(|v| src[v.end.min(src.len())..].find('\n').map_or(src.len(), |i| v.end + i + 1)).max()?;
            let fields = |l: &[crate::terrain::Scatter]| l.iter().map(terrain::scatter_fields).collect::<Vec<_>>();
            let texts: Vec<String> = now.scatter.iter().map(terrain::scatter_text).collect();
            let at = after_materials;
            let texts_at: Vec<String> = if was.scatter.is_empty() { texts.iter().map(|t| format!("\n{t}")).collect() } else { texts.clone() };
            list_edits(src, &rules, &rule_headers, &fields(&was.scatter), &fields(&now.scatter), if was.scatter.is_empty() { &texts_at } else { &texts }, at, edits)?;
        }
        (Some(h), None) => {
            let mut start = h;
            if end >= src.len() {
                while start > 0 && src[line_begin(start - 1)..start].trim().is_empty() {
                    start = line_begin(start - 1);
                }
            }
            edits.push((start..end, String::new()));
        }
        (None, Some(t)) if alone => edits.push((src.len()..src.len(), format!("\n{}", terrain::text(t)))),
        (None, Some(t)) => edits.push((head_end..head_end, format!("{}\n", terrain::text(t)))),
        (None, None) => {}
    }
    Some(())
}

/// Each table's header in a list of tables (`tag`, `[[terrain.material]]`):
/// the nearest line above its first field that begins with it.
fn headers_of(src: &str, tables: &[&dyn toml_edit::TableLike], tag: &str) -> Option<Vec<usize>> {
    let line_begin = |at: usize| src[..at].rfind('\n').map_or(0, |i| i + 1);
    let mut out = Vec::new();
    for t in tables {
        let first = t.iter().filter_map(|(key, v)| t.key(key).and_then(|k| k.span()).or_else(|| v.span())).map(|s| s.start).min()?;
        let mut h = line_begin(first);
        while !src[h..].trim_start().starts_with(tag) {
            if h == 0 {
                return None;
            }
            h = line_begin(h - 1);
        }
        out.push(h);
    }
    Some(out)
}

/// The edits of a list of tables (a terrain's materials or layers): as
/// many as before, each one's fields that changed (after its first,
/// where new); else written anew from the first one's header to the
/// last one's end, or put in at `at` where there were none.
#[allow(clippy::too_many_arguments)]
fn list_edits(src: &str, tables: &[&dyn toml_edit::TableLike], headers: &[usize], was: &[Vec<(&str, String)>], now: &[Vec<(&str, String)>], texts: &[String], at: usize, edits: &mut Vec<(std::ops::Range<usize>, String)>) -> Option<()> {
    let line_end = |at: usize| src[at.min(src.len())..].find('\n').map_or(src.len(), |i| at + i + 1);
    if tables.len() != was.len() {
        return None;
    }
    if was == now {
        return Some(());
    }
    if was.len() == now.len() {
        for (k, t) in tables.iter().enumerate() {
            if was[k] == now[k] {
                continue;
            }
            let sec = Section { sort: 0, k, start: headers[k], header: headers[k], table: *t };
            let mut keys: Vec<&str> = Vec::new();
            for (key, _) in was[k].iter().chain(&now[k]) {
                if !keys.contains(key) {
                    keys.push(key);
                }
            }
            let first = now[k].first().map(|(key, _)| *key);
            for key in keys {
                let get = |f: &Vec<(&str, String)>| f.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
                let (a, b) = (get(&was[k]), get(&now[k]));
                if a != b {
                    let anchor = [first, Some("color"), Some("colors"), Some("thick")].into_iter().flatten().find(|c| *c != key && spans(*t, c).is_some());
                    field(src, &sec, key, b, anchor, edits)?;
                }
            }
        }
        return Some(());
    }
    let text = texts.join("\n");
    match (headers.first(), tables.last()) {
        (Some(&first), Some(last)) => {
            let end = last.iter().filter_map(|(_, v)| v.span()).map(|v| line_end(v.end)).max()?;
            edits.push((first..end, text));
        }
        _ => edits.push((at..at, format!("{text}\n"))),
    }
    Some(())
}

/// The spans of a key of a table and of its value.
fn spans(t: &dyn toml_edit::TableLike, key: &str) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let (k, v) = t.get_key_value(key)?;
    Some((k.span()?, v.span()?))
}

/// The edits of one field of a section: its value changed, its line
/// put in after the line `after` ends (`None`: after the header), or
/// taken out. None where its place is not known.
fn field(src: &str, s: &Section, key: &str, now: Option<String>, after: Option<&str>, edits: &mut Vec<(std::ops::Range<usize>, String)>) -> Option<()> {
    let line_begin = |at: usize| src[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |at: usize| src[at.min(src.len())..].find('\n').map_or(src.len(), |i| at + i + 1);
    match (spans(s.table, key), now) {
        (Some((_, v)), Some(value)) => {
            if src[v.clone()] != value {
                edits.push((v, value));
            }
        }
        (Some((k, v)), None) => edits.push((line_begin(k.start)..line_end(v.end), String::new())),
        (None, Some(value)) => {
            let at = match after {
                Some(key) => line_end(spans(s.table, key)?.1.end),
                None => line_end(s.header),
            };
            edits.push((at..at, format!("{key} = {value}\n")));
        }
        (None, None) => {}
    }
    Some(())
}

/// Where the last field of a section ends: the key a new line goes
/// after.
fn last_key(s: &Section) -> Option<String> {
    s.table.iter().filter_map(|(k, v)| Some((k.to_string(), v.span()?.end))).max_by_key(|(_, end)| *end).map(|(k, _)| k)
}

/// The edits that make a thing's section say `now` instead of `was`;
/// None where they cannot (it is written anew).
fn thing_edits(src: &str, s: &Section, was: &Thing, now: &Thing, edits: &mut Vec<(std::ops::Range<usize>, String)>) -> Option<()> {
    if was == now {
        return Some(());
    }
    if was.kind != now.kind || was.model.is_some() != now.model.is_some() {
        return None;
    }
    let mut mine = Vec::new();
    if was.name != now.name {
        field(src, s, "name", now.name.as_deref().map(vvx::quoted), None, &mut mine)?;
    }
    if was.model != now.model {
        field(src, s, "model", now.model.as_deref().map(vvx::quoted), None, &mut mine)?;
    }
    if was.at != now.at {
        field(src, s, "at", Some(triple(now.at)), None, &mut mine)?;
    }
    if was.facing != now.facing {
        match (was.facing, now.facing) {
            (Some(a), Some(b)) => {
                let ((ka, _), (kb, vb)) = (facing_text(a), facing_text(b));
                if ka == kb {
                    field(src, s, kb, Some(vb), None, &mut mine)?;
                } else {
                    // facing = "east" for rotation = 30, or the other way.
                    let (k, v) = spans(s.table, ka)?;
                    mine.push((k.start..v.end, format!("{kb} = {vb}")));
                }
            }
            (None, Some(b)) => {
                let (key, value) = facing_text(b);
                field(src, s, key, Some(value), Some("at"), &mut mine)?;
            }
            (Some(a), None) => field(src, s, facing_text(a).0, None, None, &mut mine)?,
            (None, None) => {}
        }
    }
    // Tipped further: its line after the way it faces, removed at 0.
    if was.pitch != now.pitch {
        let after = ["rotation", "facing", "at"].into_iter().find(|k| s.table.contains_key(k));
        field(src, s, "pitch", (now.pitch != 0.0).then(|| number(now.pitch)), after, &mut mine)?;
    }
    if was.roll != now.roll {
        // (A pitch line new too goes in at the same place, before it.)
        let after = ["pitch", "rotation", "facing", "at"].into_iter().find(|k| s.table.contains_key(k));
        field(src, s, "roll", (now.roll != 0.0).then(|| number(now.roll)), after, &mut mine)?;
    }
    if was.solid != now.solid {
        let after = ["roll", "pitch", "rotation", "facing", "at"].into_iter().find(|k| s.table.contains_key(k));
        field(src, s, "solid", now.solid.then(|| "true".to_string()), after, &mut mine)?;
    }
    if was.scatter != now.scatter {
        let after = ["solid", "roll", "pitch", "rotation", "facing", "at"].into_iter().find(|k| s.table.contains_key(k));
        field(src, s, "scatter", now.scatter.map(|n| n.to_string()), after, &mut mine)?;
    }
    if was.lights != now.lights {
        // The key the file used, and the one the lights want now: one
        // light is `light = {...}`, more `lights = [...]`.
        let had = ["light", "lights"].into_iter().find(|k| s.table.contains_key(k));
        match (had, lights_field(&now.lights)) {
            (Some(old), Some((key, value))) if old != key => {
                let (k, v) = spans(s.table, old)?;
                mine.push((k.start..v.end, format!("{key} = {value}")));
            }
            (Some(old), None) => field(src, s, old, None, None, &mut mine)?,
            (_, Some((key, value))) => {
                let last = last_key(s)?;
                field(src, s, key, Some(value), Some(&last), &mut mine)?;
            }
            (None, None) => {}
        }
    }
    edits.extend(mine);
    Some(())
}

fn place_edits(src: &str, s: &Section, was: &Place, now: &Place, edits: &mut Vec<(std::ops::Range<usize>, String)>) -> Option<()> {
    if was == now {
        return Some(());
    }
    let mut mine = Vec::new();
    if was.name != now.name {
        field(src, s, "name", Some(vvx::quoted(&now.name)), None, &mut mine)?;
    }
    if was.at != now.at {
        field(src, s, "at", Some(triple(now.at)), None, &mut mine)?;
    }
    if was.facing != now.facing || was.named != now.named {
        field(src, s, "facing", now.facing.map(|pair| place_facing_text(now, pair)), Some("at"), &mut mine)?;
    }
    edits.extend(mine);
    Some(())
}

fn area_edits(src: &str, s: &Section, was: &Area, now: &Area, edits: &mut Vec<(std::ops::Range<usize>, String)>) -> Option<()> {
    if was == now {
        return Some(());
    }
    let mut mine = Vec::new();
    if was.name != now.name {
        field(src, s, "name", Some(vvx::quoted(&now.name)), None, &mut mine)?;
    }
    if was.at != now.at {
        field(src, s, "at", Some(triple(now.at)), None, &mut mine)?;
    }
    if was.size != now.size {
        field(src, s, "size", Some(triple(now.size)), None, &mut mine)?;
    }
    edits.extend(mine);
    Some(())
}

/// The level's text with every thing turned off the quarter turns
/// (`rotation = 30`) facing its nearest way instead (`facing = "north"`; a
/// turn right between two goes clockwise: `Way::nearest`), the rest of
/// the file as it was, to the character: what a game on the grid shows.
pub fn align(src: &str) -> Result<String> {
    let doc = Document::parse(src).map_err(|e| anyhow!("{}", e.message()))?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    if let Some(item) = doc.as_table().get("thing")
        && let Some(tables) = vvx::tables(item)
    {
        let degrees = |value: &Item| value.as_float().or_else(|| value.as_integer().map(|i| i as f64));
        for t in tables {
            if let Some((key, value)) = t.get_key_value("rotation")
                && let Some(turn) = degrees(value)
                && Facing::Turn(turn).off_grid()
                && let (Some(k), Some(v)) = (key.span(), value.span())
            {
                edits.push((k.start..v.end, format!("facing = \"{}\"", Way::nearest(turn).name())));
            }
            // Tipped off the quarter turns: the nearest quarter (a tie
            // goes up, as a turn's goes clockwise); none, its line goes.
            for word in ["pitch", "roll"] {
                let Some((key, value)) = t.get_key_value(word) else { continue };
                let Some(d) = degrees(value).filter(|d| d.rem_euclid(90.0) != 0.0) else { continue };
                let (Some(k), Some(v)) = (key.span(), value.span()) else { continue };
                let quarter = Way::nearest(d).bearing();
                if quarter == 0.0 {
                    let begin = src[..k.start].rfind('\n').map_or(0, |i| i + 1);
                    let end = src[v.end..].find('\n').map_or(src.len(), |i| v.end + i + 1);
                    edits.push((begin..end, String::new()));
                } else {
                    edits.push((k.start..v.end, format!("{word} = {quarter}")));
                }
            }
        }
        edits.sort_by_key(|(r, _)| r.start);
    }
    let mut out = src.to_string();
    for (range, with) in edits.into_iter().rev() {
        out.replace_range(range, &with);
    }
    Ok(out)
}

/// A block's name as a key of TOML: bare when it can be.
fn key(name: &str) -> String {
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { name.to_string() } else { vvx::quoted(name) }
}

/// A sign for a block not taken yet: the letters of its name first
/// (`s` for stone, `S` turned), then any.
fn new_sign(name: &str, width: usize, taken: &[String]) -> String {
    let free = |s: &String| !taken.contains(s);
    if width == 1 {
        let letters = name.chars().filter(|c| c.is_ascii_alphanumeric());
        let mut wished: Vec<String> = Vec::new();
        for c in letters {
            wished.push(c.to_ascii_lowercase().to_string());
            wished.push(c.to_ascii_uppercase().to_string());
        }
        if let Some(s) = wished.into_iter().find(free) {
            return s;
        }
        return SIGNS.iter().map(|&b| (b as char).to_string()).find(free).expect("62 signs for 62 pairs");
    }
    (0..ONE_SIGN * ONE_SIGN).map(|i| format!("{}{}", SIGNS[i / ONE_SIGN] as char, SIGNS[i % ONE_SIGN] as char)).find(free).expect("signs enough")
}

/// The cells' places in `cells` as the layers read: layers of rows of
/// cells.
fn reading_order(size: IVec3, view: View) -> Vec<Vec<Vec<usize>>> {
    let i = |x: i32, y: i32, z: i32| (x + y * size.x + z * size.x * size.y) as usize;
    match view {
        View::Above => (0..size.z).map(|z| (0..size.y).rev().map(|y| (0..size.x).map(|x| i(x, y, z)).collect()).collect()).collect(),
        View::Front => (0..size.y).map(|y| (0..size.z).rev().map(|z| (0..size.x).map(|x| i(x, y, z)).collect()).collect()).collect(),
    }
}

/// The level a text says and its legend, every mistake said with its
/// line.
pub fn read(src: &str) -> Result<(GridLevel, Legend)> {
    let doc = Document::parse(src).map_err(|e| match e.span() {
        Some(s) => anyhow!("line {}: {}", vvx::line_of(src, s.start), e.message()),
        None => anyhow!("{}", e.message()),
    })?;
    let r = Reader { src };
    let top = doc.as_table();
    vvx::check_format_of(&r, top, FORMAT_NAME, VERSION, "level")?;
    r.only(top, &["format", "block", "size", "view", "blocks", "legend", "cells", "light", "terrain", "thing", "place", "area"], "a level")?;
    let daylight = top.get("light").map(|item| read_daylight(&r, item)).transpose()?;
    let whole = |item: &Item, what: &str| -> Result<i32> {
        let v = r.number(item, what)?;
        if v.fract() != 0.0 || v < 1.0 || v > 100_000.0 {
            return Err(r.fail(item.span(), format!("{what} must be a whole number, 1 or more")));
        }
        Ok(v as i32)
    };
    // Without a block's size the level has no grid: its size is in
    // voxels, and it is what is set down in it.
    let Some(block) = top.get("block") else {
        for key in ["view", "blocks", "legend", "cells"] {
            if let Some(item) = top.get(key) {
                return Err(r.fail(item.span(), format!("{key} is for a level of blocks: block = N first (a block's voxels each way)")));
            }
        }
        let size_item = top.get("size").ok_or_else(|| anyhow!("no size = [x, y, z] (the level's voxels; or block = N and the grid's cells)"))?;
        let size = r.ints(size_item, "size")?;
        if size.cmplt(IVec3::ONE).any() {
            return Err(r.fail(size_item.span(), "size must be at least one voxel each way"));
        }
        if (size.x as i64) * (size.y as i64) * (size.z as i64) > 1 << 31 {
            return Err(r.fail(size_item.span(), "the level is too big"));
        }
        let mut level = GridLevel::new(0, size);
        level.daylight = daylight;
        read_terrain(&r, top, &mut level)?;
        read_set_down(&r, top, &mut level)?;
        return Ok((level, Legend::default()));
    };
    let n = whole(block, "block")?;
    let size_item = top.get("size").ok_or_else(|| anyhow!("no size = [x, y, z] (the grid's cells)"))?;
    let size = r.ints(size_item, "size")?;
    if size.cmplt(IVec3::ONE).any() {
        return Err(r.fail(size_item.span(), "size must be at least one cell each way"));
    }
    if (size.x as i64) * (size.y as i64) * (size.z as i64) > 1 << 26 {
        return Err(r.fail(size_item.span(), "the grid is too big"));
    }
    let view = match top.get("view") {
        None => None,
        Some(item) => Some(match item.as_str() {
            Some("above") => View::Above,
            Some("front") => View::Front,
            _ => return Err(r.fail(item.span(), "view must be \"above\" or \"front\" (which way the layers read)")),
        }),
    };
    let mut level = GridLevel::new(n, size);
    if let Some(item) = top.get("blocks") {
        let table = item.as_table_like().ok_or_else(|| r.fail(item.span(), "blocks must be a table: name = \"solid\" and the like"))?;
        for (name, kind) in table.iter() {
            let word = kind.as_str().unwrap_or_default();
            let kind = Kind::ALL.into_iter().find(|k| k.name() == word);
            let names: Vec<&str> = Kind::ALL.iter().map(|k| k.name()).collect();
            let kind = kind.ok_or_else(|| r.fail(table.get(name).and_then(Item::span), format!("block '{name}': '{word}' is no kind of block ({})", names.join(", "))))?;
            level.blocks.push(BlockEntry { name: name.to_string(), kind });
        }
    }
    // The legend: sign -> (block's number, turn).
    let mut legend = Legend { view, signs: Vec::new() };
    let mut cells_of: Vec<(String, Cell)> = Vec::new();
    let mut width = 0;
    if let Some(item) = top.get("legend") {
        let table = item.as_table_like().ok_or_else(|| r.fail(item.span(), "legend must be a table of signs"))?;
        for (sign, entry) in table.iter() {
            let len = sign.chars().count();
            if !(1..=2).contains(&len) || sign.chars().any(|c| !c.is_ascii_alphanumeric()) {
                return Err(r.fail(entry.span(), format!("'{sign}' cannot be a sign: one or two letters or digits")));
            }
            if width != 0 && width != len {
                return Err(r.fail(entry.span(), format!("'{sign}' has {len} characters, the signs before have {width}: all signs of a file are as long as one another")));
            }
            width = len;
            let (block, turn) = match (entry.as_str(), entry.as_table_like()) {
                (Some(name), _) => (name.to_string(), 0),
                (None, Some(t)) => {
                    r.renamed(t, &[("turn", "orientation")], &format!("the legend's '{sign}'"))?;
                    r.only(t, &["block", "orientation"], &format!("the legend's '{sign}'"))?;
                    let name = t.get("block").and_then(Item::as_str).ok_or_else(|| r.fail(entry.span(), format!("the legend's '{sign}' needs block = \"name\"")))?;
                    let turn = match t.get("orientation") {
                        None => 0,
                        Some(i) => {
                            let v = r.number(i, "orientation")?;
                            if v.fract() != 0.0 || !(0.0..Turn::COUNT as f32).contains(&v) {
                                return Err(r.fail(i.span(), format!("orientation must be a whole number from 0 to {}", Turn::COUNT - 1)));
                            }
                            v as u8
                        }
                    };
                    (name.to_string(), turn)
                }
                _ => return Err(r.fail(entry.span(), format!("the legend's '{sign}' must be a block's name, or {{ block = \"name\", orientation = 1 }}"))),
            };
            let number = level.blocks.iter().position(|b| b.name == block).ok_or_else(|| r.fail(entry.span(), format!("the legend's '{sign}': no block '{block}' in [blocks]")))?;
            cells_of.push((sign.to_string(), Cell { block: number as u16 + 1, turn: Turn::from_number(turn).expect("checked") }));
            legend.signs.push((sign.to_string(), block, turn));
        }
    }
    let width = width.max(1);
    let cells = top.get("cells").ok_or_else(|| anyhow!("no [cells] with the layers of signs"))?;
    let cells = cells.as_table_like().ok_or_else(|| r.fail(cells.span(), "cells must be a table with its layers: [cells]"))?;
    r.only(cells, &["layers"], "[cells]")?;
    let item = cells.get("layers").ok_or_else(|| anyhow!("[cells] has no layers = \"\"\" ... \"\"\""))?;
    let text = item.as_str().ok_or_else(|| r.fail(item.span(), "layers must be a text between \"\"\" marks"))?;
    let first = r.first_line(item);
    let line = |i: usize| if first > 0 { format!("line {}: ", first + i) } else { String::new() };
    let layers = vvx::layer_list(text, &line)?;
    let order = reading_order(size, view.unwrap_or_default());
    let (want_layers, want_rows) = (order.len(), order.first().map_or(0, Vec::len));
    let read_layers: usize = layers.iter().map(|l| l.times).sum();
    let what = match view.unwrap_or_default() {
        View::Above => ("layers up", "rows deep"),
        View::Front => ("layers deep", "rows up"),
    };
    if read_layers != want_layers {
        return Err(r.fail(item.span(), format!("the cells have {read_layers} layers, but a grid {} by {} by {} seen {} has {want_layers} ({})", size.x, size.y, size.z, view.unwrap_or_default().name(), what.0)));
    }
    let known = || cells_of.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join(", ");
    let mut k = 0;
    for layer in &layers {
        if layer.rows.len() != want_rows {
            bail!("{}this layer has {} rows, a layer here has {want_rows} ({})", line(layer.rows[0].0), layer.rows.len(), what.1);
        }
        for _ in 0..layer.times {
            for (r_, &(i, row)) in layer.rows.iter().enumerate() {
                let bytes = row.as_bytes();
                if bytes.len() != size.x as usize * width {
                    let want = if width == 1 { format!("{} cells, a sign each", size.x) } else { format!("{} cells of {width} characters", size.x) };
                    bail!("{}this row has {} characters, a row here is {want}", line(i), bytes.len());
                }
                for (x, sign) in bytes.chunks(width).enumerate() {
                    if sign.iter().all(|&b| b == b'.') {
                        continue;
                    }
                    let cell = cells_of.iter().find(|(s, _)| s.as_bytes() == sign).map(|(_, c)| *c);
                    let cell = cell.ok_or_else(|| anyhow!("{}'{}' is no sign of the legend (it has {})", line(i), String::from_utf8_lossy(sign), known()))?;
                    level.cells[order[k][r_][x]] = cell;
                }
            }
            k += 1;
        }
    }
    level.daylight = daylight;
    read_terrain(&r, top, &mut level)?;
    read_set_down(&r, top, &mut level)?;
    Ok((level, legend))
}

/// The level's `[terrain]`, if it has one, within its voxels.
fn read_terrain(r: &Reader, top: &toml_edit::Table, level: &mut GridLevel) -> Result<()> {
    if let Some(item) = top.get("terrain") {
        let t = crate::terrain::read(r, item)?;
        crate::terrain::check_room(&t, level.voxels())?;
        level.terrain = Some(t);
    }
    Ok(())
}

/// The line a table of the file begins on (its first field's), 0
/// without a place in the file.
fn line_of_table(r: &Reader, t: &dyn toml_edit::TableLike) -> usize {
    t.iter().filter_map(|(k, v)| t.key(k).and_then(|k| k.span()).or_else(|| v.span())).map(|s| s.start).min().map_or(0, |at| vvx::line_of(r.src, at))
}

/// One light of a thing (`light = {...}`, or one of `lights = [...]`);
/// `span` where its field is, for a mistake without a place of its own.
fn read_light(r: &Reader, l: &dyn toml_edit::TableLike, span: Option<std::ops::Range<usize>>) -> Result<ThingLight> {
    r.only(l, &["at", "radius", "color"], "a light")?;
    let at = l.get("at").map(|a| r.ints(a, "a light's at").map(|v| v.to_array())).transpose()?;
    let radius_item = l.get("radius").ok_or_else(|| r.fail(span.clone(), "a light needs radius = N (how far it reaches, voxels)"))?;
    let radius = radius_item.as_float().or_else(|| radius_item.as_integer().map(|i| i as f64)).filter(|v| v.is_finite() && *v > 0.0);
    let radius = radius.ok_or_else(|| r.fail(radius_item.span(), "a light's radius must be a number above 0 (voxels)"))?;
    let color = match l.get("color") {
        None => None,
        Some(c) => match c.as_str().filter(|s| hex(s).is_some()) {
            Some(s) => Some(s.to_string()),
            None => return Err(r.fail(c.span(), "a light's color must be \"#rrggbb\", like \"#ffd9a0\"")),
        },
    };
    Ok(ThingLight { at, radius, color })
}

/// A level's own daylight (`[light]`), each word checked.
fn read_daylight(r: &Reader, item: &Item) -> Result<Daylight> {
    let t = item.as_table_like().ok_or_else(|| r.fail(item.span(), "light must be a table: [light] with sun, sun_color, ambient, shadow, sky"))?;
    r.only(t, &Daylight::WORDS, "the level's [light]")?;
    let colour = |key: &str| -> Result<Option<String>> {
        match t.get(key) {
            None => Ok(None),
            Some(c) => match c.as_str().filter(|s| hex(s).is_some()) {
                Some(s) => Ok(Some(s.to_string())),
                // Three numbers from 0 to 1, as conf.lua's c.light writes them.
                None if c.as_array().is_some() => {
                    let rgb = r.triple(c, key).ok().filter(|v| v.iter().all(|v| (0.0..=1.0).contains(v)));
                    let rgb = rgb.ok_or_else(|| r.fail(c.span(), format!("{key} must be a color \"#rrggbb\" or three numbers from 0 to 1, like [1, 0.85, 0.63]")))?;
                    Ok(Some(format!("#{:02x}{:02x}{:02x}", (rgb[0] * 255.0).round() as u8, (rgb[1] * 255.0).round() as u8, (rgb[2] * 255.0).round() as u8)))
                }
                None => Err(r.fail(c.span(), format!("{key} must be a color \"#rrggbb\", like \"#ffd9a0\", or three numbers from 0 to 1"))),
            },
        }
    };
    let sun = match t.get("sun") {
        None => None,
        Some(s) => {
            let [x, y, z] = r.triple(s, "sun")?;
            if z >= 0.0 {
                return Err(r.fail(s.span(), "sun is the way its light travels, [x, y, z] with z below 0 (towards the ground)"));
            }
            // As the file wrote it (an f32 read back as the decimal it was).
            let back = |v: f32| format!("{v}").parse::<f64>().unwrap_or(v as f64);
            Some([back(x), back(y), back(z)])
        }
    };
    let unit = |key: &str| -> Result<Option<f64>> { t.get(key).map(|i| r.fraction(i, key).map(|v| format!("{v}").parse::<f64>().unwrap_or(v as f64))).transpose() };
    Ok(Daylight { sun, sun_color: colour("sun_color")?, ambient: unit("ambient")?, shadow: unit("shadow")?, sky: colour("sky")? })
}

/// The things, places and areas of a level's text.
fn read_set_down(r: &Reader, top: &toml_edit::Table, level: &mut GridLevel) -> Result<()> {
    let triple = |t: &dyn toml_edit::TableLike, key: &str, what: &str| -> Result<[i32; 3]> {
        let line = line_of_table(r, t);
        let item = t.get(key).ok_or_else(|| if line > 0 { anyhow!("line {line}: {what} has no {key} = [x, y, z]") } else { anyhow!("{what} has no {key} = [x, y, z]") })?;
        Ok(r.ints(item, key)?.to_array())
    };
    let text = |t: &dyn toml_edit::TableLike, key: &str, what: &str| -> Result<Option<String>> {
        match t.get(key) {
            None => Ok(None),
            Some(item) => match item.as_str() {
                Some(s) if !s.is_empty() => Ok(Some(s.to_string())),
                _ => Err(r.fail(item.span(), format!("{what}'s {key} must be a name in quotes"))),
            },
        }
    };
    if let Some(item) = top.get("thing") {
        for t in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "things must be tables: [[thing]] with kind (or model) and at"))? {
            r.renamed(t, &[("turn", "rotation")], "a thing")?;
            r.only(t, &["name", "kind", "model", "at", "facing", "rotation", "pitch", "roll", "solid", "light", "lights", "scatter"], "a thing")?;
            let line = line_of_table(r, t);
            let here = |why: &str| if line > 0 { anyhow!("line {line}: {why}") } else { anyhow!("{why}") };
            let name = text(t, "name", "a thing")?;
            let kind = text(t, "kind", "a thing")?;
            let model = text(t, "model", "a thing")?;
            if let Some(m) = &model
                && !m.ends_with(&format!(".{}", crate::vvx::EXTENSION))
            {
                return Err(r.fail(t.get("model").and_then(Item::span), format!("model = \"{m}\": a model is a .vvx file of the game")));
            }
            let lights = match (t.get("light"), t.get("lights")) {
                (Some(_), Some(more)) => return Err(r.fail(more.span(), "a thing has light = { ... } (one) or lights = [ ... ] (several), not both")),
                (Some(item), None) => {
                    let l = item.as_table_like().ok_or_else(|| r.fail(item.span(), "light must be a table: light = { at = [0, 0, 10], radius = 60, color = \"#ffd9a0\" }"))?;
                    vec![read_light(r, l, item.span())?]
                }
                (None, Some(item)) => {
                    let tables = vvx::tables(item).ok_or_else(|| r.fail(item.span(), "lights must be a list of lights: lights = [ { at = [0, 0, 10], radius = 60 }, { radius = 120 } ]"))?;
                    if tables.is_empty() {
                        return Err(r.fail(item.span(), "lights = [] holds no light: leave it out, or give one (light = { radius = 60 })"));
                    }
                    tables.into_iter().map(|l| read_light(r, l, item.span())).collect::<Result<Vec<_>>>()?
                }
                (None, None) => Vec::new(),
            };
            let solid = match t.get("solid") {
                None => false,
                Some(item) => item.as_bool().ok_or_else(|| r.fail(item.span(), "solid must be true or false"))?,
            };
            let facing = match (t.get("facing"), t.get("rotation")) {
                (Some(_), Some(turn)) => return Err(r.fail(turn.span(), "a thing faces a way (facing = \"south\") or has a rotation (rotation = 30), not both")),
                (Some(f), None) => match f.as_str().and_then(Way::of_name) {
                    Some(w) => Some(Facing::Way(w)),
                    None => return Err(r.fail(f.span(), "facing must be \"north\", \"east\", \"south\" or \"west\" (or rotation = degrees, where the game rotates things freely)")),
                },
                (None, Some(turn)) => match turn.as_float().or_else(|| turn.as_integer().map(|i| i as f64)).filter(|v| v.is_finite() && v.abs() <= 1e6) {
                    Some(v) => Some(Facing::Turn(v)),
                    None => return Err(r.fail(turn.span(), "rotation must be degrees clockwise seen from above (north 0, east 90, south 180, west 270)")),
                },
                (None, None) => None,
            };
            // Tipped: degrees, a piece's with a model only.
            let mut tips = [0.0; 2];
            for (k, key) in ["pitch", "roll"].into_iter().enumerate() {
                let Some(item) = t.get(key) else { continue };
                tips[k] = match item.as_float().or_else(|| item.as_integer().map(|i| i as f64)).filter(|v| v.is_finite() && v.abs() <= 1e6) {
                    Some(v) => v,
                    None => return Err(r.fail(item.span(), format!("{key} must be degrees ({})", if k == 0 { "its nose up: 90 lays it on its back" } else { "its right side down: 90 lays it on its right side" }))),
                };
                if model.is_none() {
                    return Err(r.fail(item.span(), format!("{key} tips a model: a piece with model = \"...\" (a thing of a kind or a light alone has none to tip)")));
                }
            }
            let [pitch, roll] = tips;
            let light = lights.first();
            let kind = match (kind, &model, &light) {
                (Some(k), None, _) if k == PIECE && light.is_none() => return Err(here("a piece is a model set down (model = \"room/stove.vvx\") or a light (light = { radius = 60 })")),
                (Some(k), _, _) if k == PIECE => k,
                (Some(k), Some(_), _) => return Err(here(&format!("kind = \"{k}\" and a model: a thing of a kind has its kind's model (c.things.{k}.model); a model alone is a piece, set down as it is"))),
                (Some(k), None, Some(_)) => return Err(here(&format!("kind = \"{k}\" with a light: lights are pieces' so far (a light alone, or a model with its light)"))),
                (Some(k), None, None) => k,
                (None, Some(_), _) | (None, None, Some(_)) => PIECE.to_string(),
                (None, None, None) => return Err(here("a thing needs kind = \"name\" (a kind of c.things), model = \"file.vvx\" (a piece) or a light")),
            };
            if solid && model.is_none() {
                return Err(here(if kind == PIECE { "solid = true needs a model: a light alone has no voxels to hold anything up" } else { "solid = true is a piece's (a model set down); a thing of a kind is held up or stopped by its rules" }));
            }
            if let Some(other) = name.as_ref().and_then(|n| level.things.iter().find(|o| o.name.as_ref() == Some(n))) {
                return Err(here(&format!("a second thing called '{}' (the first is on line {}): a name is one thing's", name.as_deref().unwrap_or_default(), other.line)));
            }
            let scatter = match t.get("scatter") {
                None => None,
                Some(i) => match i.as_integer() {
                    Some(v) if v >= 1 => Some(v as u32),
                    _ => return Err(r.fail(i.span(), "scatter must be the number of the terrain's scatter rule that set it down, from 1")),
                },
            };
            level.things.push(Thing { kind, at: triple(t, "at", "a thing")?, name, model, facing, pitch, roll, lights, solid, scatter, line });
        }
    }
    if let Some(item) = top.get("place") {
        for t in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "places must be tables: [[place]] with name and at"))? {
            r.only(t, &["name", "at", "facing"], "a place")?;
            let name = t.get("name").and_then(Item::as_str).ok_or_else(|| anyhow!("a place has no name = \"name\""))?;
            let (facing, named) = match t.get("facing") {
                None => (None, false),
                Some(f) if f.as_str().is_some() => match f.as_str().and_then(Way::of_name) {
                    Some(w) => (Some(w.pair()), true),
                    None => return Err(r.fail(f.span(), "facing must be \"north\", \"east\", \"south\" or \"west\", or two whole numbers, like [0, -1]")),
                },
                Some(f) => {
                    let wrong = || r.fail(f.span(), "facing must be two whole numbers, like [0, -1], or a way's name (\"south\")");
                    let a = f.as_array().filter(|a| a.len() == 2).ok_or_else(wrong)?;
                    let v: Vec<i64> = a.iter().map(|v| v.as_integer().ok_or_else(wrong)).collect::<Result<_>>()?;
                    (Some([v[0] as i32, v[1] as i32]), false)
                }
            };
            level.places.push(Place { name: name.to_string(), at: triple(t, "at", "a place")?, facing, named, line: line_of_table(r, t) });
        }
    }
    if let Some(item) = top.get("area") {
        for t in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "areas must be tables: [[area]] with name, at and size"))? {
            r.only(t, &["name", "at", "size"], "an area")?;
            let line = line_of_table(r, t);
            let here = |why: &str| if line > 0 { anyhow!("line {line}: {why}") } else { anyhow!("{why}") };
            let name = text(t, "name", "an area")?.ok_or_else(|| here("an area has no name = \"name\""))?;
            let size = triple(t, "size", "an area")?;
            if size.iter().any(|s| *s < 1) {
                return Err(r.fail(t.get("size").and_then(Item::span), "an area's size must be at least one voxel each way"));
            }
            if let Some(other) = level.areas.iter().find(|a| a.name == name) {
                return Err(here(&format!("a second area called '{name}' (the first is on line {}): a name is one area's", other.line)));
            }
            level.areas.push(Area { name, at: triple(t, "at", "an area")?, size, line });
        }
    }
    Ok(())
}
