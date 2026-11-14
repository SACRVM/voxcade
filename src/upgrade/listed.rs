use super::*;

/// The braces and parentheses a line opens less those it closes,
/// outside strings and its comment.
fn depth(line: &str) -> i32 {
    let (mut depth, mut quote, mut chars) = (0, None, line.chars().peekable());
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '-') if chars.peek() == Some(&'-') => break,
            (None, '{' | '(') => depth += 1,
            (None, '}' | ')') => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// Whether a line's code begins by setting `c.models`, or is a loop on
/// one line that only does (`for _, f in pairs(t) do c.models[...] = f
/// end`).
fn sets(line: &str) -> bool {
    let code = line.trim_start();
    let after = code.strip_prefix("c.models").or_else(|| code.strip_prefix("table.insert(c.models"));
    let loop_body = || {
        let code = code.split("--").next().unwrap_or_default().trim_end();
        let body = code.strip_prefix("for ")?.split_once(" do ")?.1.strip_suffix(" end")?;
        Some(!body.contains(" end") && sets(body))
    };
    after.is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '=', '[', ',', ')'])) || loop_body() == Some(true)
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Lines of `conf.lua` by number (from 1), each with its text.
type Lines = Vec<(usize, String)>;

/// The lines of `conf.lua` that go, and those naming `c.models` that
/// stay.
fn removals(conf: &str) -> (Lines, Lines) {
    let lines: Vec<&str> = conf.lines().collect();
    let mut gone = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if !sets(lines[i]) {
            i += 1;
            continue;
        }
        // The statement to its last line: what it opens closed.
        let mut open = depth(lines[i]);
        gone[i] = true;
        while open > 0 && i + 1 < lines.len() {
            i += 1;
            gone[i] = true;
            open += depth(lines[i]);
        }
        i += 1;
    }
    // Loops that only filled it, the inner first.
    let quiet = |l: &str| l.trim().is_empty() || l.trim_start().starts_with("--");
    loop {
        let mut more = false;
        for (top, line) in lines.iter().enumerate() {
            let code = line.split("--").next().unwrap_or_default().trim();
            if gone[top] || !code.starts_with("for ") || !code.ends_with(" do") {
                continue;
            }
            let Some(end) = (top + 1..lines.len()).find(|&e| indent(lines[e]) <= indent(line) && !lines[e].trim().is_empty()) else { continue };
            let mut body = top + 1..end;
            let emptied = body.clone().any(|b| gone[b]) && body.all(|b| gone[b] || quiet(lines[b]));
            if lines[end].trim() != "end" || indent(lines[end]) != indent(line) || !emptied {
                continue;
            }
            gone[top..=end].iter_mut().for_each(|g| *g = true);
            more = true;
        }
        if !more {
            break;
        }
    }
    // A block of them alone: the comment straight above heads it, and
    // a blank line left double goes too.
    let blank = |i: usize| lines.get(i).is_some_and(|l| l.trim().is_empty());
    let mut i = 0;
    while i < lines.len() {
        if !gone[i] {
            i += 1;
            continue;
        }
        let mut end = i;
        while end + 1 < lines.len() && gone[end + 1] {
            end += 1;
        }
        let alone = end + 1 >= lines.len() || blank(end + 1) || lines[end + 1].trim() == "end";
        let mut top = i;
        while alone && top > 0 && lines[top - 1].trim_start().starts_with("--") {
            top -= 1;
            gone[top] = true;
        }
        if top > 0 && blank(top - 1) && blank(end + 1) {
            gone[end + 1] = true;
        }
        i = end + 1;
    }
    let taken = lines.iter().enumerate().filter(|(i, _)| gone[*i]).map(|(i, l)| (i + 1, l.to_string())).collect();
    let named = |l: &str| l.split("--").next().unwrap_or_default().contains("c.models");
    let left = lines.iter().enumerate().filter(|(i, l)| !gone[*i] && named(l)).map(|(i, l)| (i + 1, l.to_string())).collect();
    (taken, left)
}

fn conf(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default()
}

pub fn left(dir: &Path) -> bool {
    let (taken, left) = removals(&conf(dir));
    !taken.is_empty() || !left.is_empty()
}

const WHY: &str = "c.models is no more (frame:model draws any model file of the game by its path, loaded when it is first drawn): its lines go";

/// What is left that the step cannot take out.
fn staying(left: &[(usize, String)]) -> Vec<String> {
    left.iter().map(|(n, l)| format!("conf.lua:{n} still names c.models, which is no more: take it out by hand (frame:model draws any model file of the game): {}", l.trim())).collect()
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let (taken, left) = removals(&conf(dir));
    let mut out: Vec<String> = taken
        .iter()
        .map(|(n, l)| if l.trim().is_empty() { format!("conf.lua:{n} taken out: a blank line left double") } else { format!("conf.lua:{n} taken out: {}", l.trim()) })
        .collect();
    if !taken.is_empty() {
        out.insert(0, WHY.into());
    }
    out.extend(staying(&left));
    Ok(out)
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let (taken, left) = removals(&conf(dir));
    let edits: Vec<(PathBuf, usize, String, Option<String>)> = taken.into_iter().map(|(n, l)| (PathBuf::from("conf.lua"), n, l, None)).collect();
    let mut out = if edits.is_empty() { Vec::new() } else { vec![WHY.to_string()] };
    out.extend(write_lua(dir, &edits)?);
    out.extend(staying(&left));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BunnyFarm's lines as they were: the list made empty, then filled
    /// by loops; the loops go with it, the locals they read stay.
    #[test]
    fn the_lines_and_the_loops_that_filled_the_list_go() {
        let conf = r#"function conf(c)
  -- The crops.
  local CROPS = { "carrot", "wheat" }
  c.models = {}
  for _, crop in ipairs(CROPS) do
    c.models[#c.models + 1] = crop .. ".vvx"
  end
  local COLOURS = { chicken = { "white" } }
  for _, sp in ipairs({ "chicken" }) do
    for _, colour in ipairs(COLOURS[sp]) do
      c.models[#c.models + 1] = sp .. "_" .. colour .. ".vvx"
    end
  end

  for i, crop in ipairs(CROPS) do
    c.items["seed_" .. crop] = { stack = 99 }
  end
end
"#;
        let (taken, left) = removals(conf);
        let numbers: Vec<usize> = taken.iter().map(|(n, _)| *n).collect();
        assert_eq!(numbers, vec![4, 5, 6, 7, 9, 10, 11, 12, 13], "{taken:?}");
        assert!(left.is_empty(), "{left:?}");
    }

    /// A list over several lines goes to its closing brace; the comment
    /// over a block that goes on stays, one over the list alone goes; a
    /// loop on one line filling it goes (UNTZ's chapter 0); a line that
    /// only reads it is said and left.
    #[test]
    fn a_list_goes_to_its_closing_brace() {
        let conf = r#"function conf(c)
  -- The ship and the rest.
  c.models = { "ship.vvx", -- the ship {
    "ship2.vvx" }
  c.things.ship = {}

  -- Drawn only.
  c.models = { "a.vvx" }

  table.insert(c.models, "b.vvx")
  local n = #c.models
  if type(model) == "table" then
    for _, file in pairs(model) do c.models[#c.models + 1] = file end
  end
end
"#;
        let (taken, left) = removals(conf);
        let numbers: Vec<usize> = taken.iter().map(|(n, _)| *n).collect();
        assert_eq!(numbers, vec![3, 4, 7, 8, 9, 10, 13], "{taken:?}");
        assert_eq!(left.iter().map(|(n, _)| *n).collect::<Vec<_>>(), vec![11], "named but not set: said and left");
    }
}
