//! The documentation names files, tests and commands; a name that no longer
//! exists is drift. Every backticked `src/...` path in CLAUDE.md, README.md, CONTRIBUTING.md, `docs/*.md` and the skills must
//! exist; every test named after the word "test" or "tests" (a list
//! joined by commas or "and") must be a `fn` somewhere under `src/`,
//! `tests/` or `examples/`; every `voxcade …` example in README.md,
//! CONTRIBUTING.md and `dist/*.txt` must be a command line the program
//! takes, naming files of the repository that are there.

use std::path::{Path, PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn documents() -> Vec<(String, String)> {
    let mut names = vec![PathBuf::from(ROOT).join("CLAUDE.md")];
    let mut docs: Vec<_> = std::fs::read_dir(Path::new(ROOT).join("docs")).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "md")).collect();
    docs.sort();
    names.extend(docs);
    names.push(PathBuf::from(ROOT).join("README.md"));
    names.push(PathBuf::from(ROOT).join("CONTRIBUTING.md"));
    let mut skills: Vec<_> = std::fs::read_dir(Path::new(ROOT).join(".claude/skills")).unwrap().flatten().map(|e| e.path().join("SKILL.md")).filter(|p| p.exists()).collect();
    skills.sort();
    names.extend(skills);
    names.into_iter().map(|p| (p.strip_prefix(ROOT).unwrap().display().to_string(), std::fs::read_to_string(&p).unwrap())).collect()
}

/// The backticked spans of a line: where each one's opening tick is, and
/// what is inside.
fn spans(line: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(a) = line[at..].find('`') {
        let open = at + a;
        let Some(b) = line[open + 1..].find('`') else { break };
        found.push((open, &line[open + 1..open + 1 + b]));
        at = open + b + 2;
    }
    found
}

#[test]
fn the_paths_the_docs_name_exist() {
    let mut missing = Vec::new();
    for (doc, text) in documents() {
        for (n, line) in text.lines().enumerate() {
            for (_, span) in spans(line) {
                for word in span.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
                    let Some(path) = word.strip_prefix("src/") else { continue };
                    let path = path.split(':').next().unwrap_or("");
                    // Patterns and placeholders name no one file.
                    if path.is_empty() || path.contains(['*', '<', '{']) || path.contains("NAME") || path.contains("...") {
                        continue;
                    }
                    if !Path::new(ROOT).join("src").join(path.trim_end_matches('/')).exists() {
                        missing.push(format!("{doc}:{}: src/{path}", n + 1));
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "the docs name files that do not exist:\n{}", missing.join("\n"));
}

#[test]
fn the_tests_the_docs_name_exist() {
    let mut files = Vec::new();
    for dir in ["src", "tests", "examples"] {
        rust_files(&Path::new(ROOT).join(dir), &mut files);
    }
    let code: String = files.iter().map(|f| std::fs::read_to_string(f).unwrap_or_default()).collect::<Vec<_>>().join("\n");
    let exists = |name: &str| code.contains(&format!("fn {name}("));
    let mut missing = Vec::new();
    let mut checked = 0;
    for (doc, text) in documents() {
        for (n, line) in text.lines().enumerate() {
            let spans = spans(line);
            for (i, (start, _)) in spans.iter().enumerate() {
                let before = line[..*start].trim_end().trim_end_matches(':').to_lowercase();
                if !(before.ends_with("test") || before.ends_with("tests")) {
                    continue;
                }
                // This span and the ones after it, joined by ", " or " and ".
                let mut j = i;
                loop {
                    let (open, name) = spans[j];
                    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        checked += 1;
                        if !exists(name) {
                            missing.push(format!("{doc}:{}: {name}", n + 1));
                        }
                    }
                    let Some(&(next, _)) = spans.get(j + 1) else { break };
                    let between = &line[open + name.len() + 2..next];
                    if between == ", " || between == " and " || between == ", and " {
                        j += 1;
                    } else {
                        break;
                    }
                }
            }
        }
    }
    assert!(checked > 20, "the scan found only {checked} test names: has the wording changed?");
    assert!(missing.is_empty(), "the docs name tests that do not exist:\n{}", missing.join("\n"));
}

/// The texts whose `voxcade …` examples are tried: README.md and
/// CONTRIBUTING.md (their code blocks and backticked spans) and the
/// package's `dist/*.txt` (every line).
fn example_texts() -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for name in ["README.md", "CONTRIBUTING.md"] {
        let text = std::fs::read_to_string(Path::new(ROOT).join(name)).unwrap();
        let mut pieces = Vec::new();
        let mut prose = String::new();
        let mut fenced = false;
        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                // A shell line, its comment left out.
                pieces.push(line.split(" # ").next().unwrap_or("").to_string());
            } else {
                prose.push_str(line.trim());
                prose.push(' ');
            }
        }
        // Backticked spans (`` ` `` too), a line break inside one read as a space.
        pieces.extend(code_spans(&prose));
        out.push((name.to_string(), pieces));
    }
    let mut txts: Vec<PathBuf> = std::fs::read_dir(Path::new(ROOT).join("dist")).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "txt")).collect();
    txts.sort();
    for path in txts {
        // A line's example ends where two spaces lead to what it does.
        let lines = std::fs::read_to_string(&path).unwrap().lines().map(|l| l.trim().split("  ").next().unwrap_or("").to_string()).collect();
        out.push((path.strip_prefix(ROOT).unwrap().display().to_string(), lines));
    }
    out
}

/// Markdown's code spans in `text`: from a run of backticks to the next
/// run as long.
fn code_spans(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let run = |at: usize| b[at..].iter().take_while(|&&c| c == b'`').count();
    let (mut spans, mut at) = (Vec::new(), 0);
    while let Some(open) = text[at..].find('`').map(|o| o + at) {
        let n = run(open);
        let mut look = open + n;
        let close = loop {
            match text[look..].find('`').map(|o| o + look) {
                Some(c) if run(c) == n => break Some(c),
                Some(c) => look = c + run(c),
                None => break None,
            }
        };
        let Some(close) = close else { break };
        spans.push(text[open + n..close].trim().to_string());
        at = close + n;
    }
    spans
}

/// A piece's words as a shell splits them (double quotes kept together).
fn shell_words(piece: &str) -> Vec<String> {
    let (mut words, mut word, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    for c in piece.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    words.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            c => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(word);
    }
    words
}

/// The `voxcade` command lines in a piece of a text: from each `voxcade`
/// standing as a word (not `c.voxcade`, `/path/to/voxcade`) followed by a
/// command's name or `--help`, to the piece's end or the first word of
/// prose after it (one starting with a bracket, or ending a sentence).
fn examples(piece: &str, commands: &[&str]) -> Vec<Vec<String>> {
    let words = shell_words(piece);
    let mut found = Vec::new();
    for (i, w) in words.iter().enumerate() {
        if w != "voxcade" {
            continue;
        }
        let Some(first) = words.get(i + 1) else { continue };
        if !commands.contains(&first.as_str()) && first != "--help" {
            continue;
        }
        let mut line = Vec::new();
        for w in &words[i + 1..] {
            if w.starts_with('(') {
                break;
            }
            let end = w.ends_with(['.', ',', ';', ')']) && !w.contains('/');
            line.push(w.trim_end_matches(['.', ',', ';', ')']).to_string());
            if end {
                break;
            }
        }
        found.push(line);
    }
    found
}

/// Options whose value is a file the command writes: no need to be there.
const WRITTEN: [&str; 6] = ["--out", "--lua", "--record", "--shot", "--digest", "--journal"];

/// Every `voxcade …` example in README.md, CONTRIBUTING.md and
/// `dist/*.txt` is a command line the program takes (unknown commands and
/// options are refused, `cli::takes`), and every file of the repository it
/// names (a game, a replay, a scenario) is there. A command named alone
/// (`voxcade check`) only needs to be a command; one with a placeholder
/// (`[--install]`, `...`, `A | B`) is a form, not an example.
#[test]
fn the_command_examples_in_the_docs_run() {
    let commands = voxcade::cli::command_names();
    let (mut wrong, mut tried) = (Vec::new(), 0);
    for (doc, pieces) in example_texts() {
        for piece in &pieces {
            for line in examples(piece, &commands) {
                let shown = format!("{doc}: voxcade {}", line.join(" "));
                if line.len() == 1 || line.iter().any(|w| w.contains(['[', '|', '…']) || w.contains("...")) {
                    continue;
                }
                tried += 1;
                if let Err(e) = voxcade::cli::takes(&line) {
                    wrong.push(format!("{shown}: {e:#}"));
                    continue;
                }
                let mut game: Option<&str> = None;
                for (i, w) in line.iter().enumerate() {
                    if i > 0 && WRITTEN.contains(&line[i - 1].as_str()) {
                        continue;
                    }
                    let in_repo = ["games/", "templates/", "examples/", "tools/", "music/", "lua/", "dist/", "docs/"].iter().any(|top| w.starts_with(top));
                    if in_repo {
                        if !Path::new(ROOT).join(w).exists() {
                            wrong.push(format!("{shown}: {w} is not there"));
                        }
                        game = game.or(Some(w.as_str()).filter(|w| Path::new(ROOT).join(w).join("conf.lua").is_file()));
                    }
                    if i > 0 && line[i - 1] == "--scenario"
                        && let Some(game) = game
                        && !Path::new(ROOT).join(game).join("scenarios").join(format!("{w}.lua")).is_file()
                    {
                        wrong.push(format!("{shown}: {game} has no scenario {w}"));
                    }
                }
            }
        }
    }
    assert!(tried > 15, "only {tried} examples found: has the wording changed?");
    assert!(wrong.is_empty(), "examples in the docs that do not run:\n{}", wrong.join("\n"));
}

/// Every workflow builds with the Rust `.github/rust-toolchain` names (the
/// releases as the tests ran), never `stable`; only the rust-version job
/// names its own, Cargo.toml's floor.
#[test]
fn the_workflows_build_with_one_rust() {
    let version = std::fs::read_to_string(Path::new(ROOT).join(".github/rust-toolchain")).unwrap();
    let version = version.lines().next().unwrap();
    assert!(version.split('.').all(|n| n.parse::<u32>().is_ok()), "{version}");
    let floor = std::fs::read_to_string(Path::new(ROOT).join("Cargo.toml")).unwrap();
    let floor = floor.lines().find_map(|l| l.strip_prefix("rust-version = ")).unwrap().trim_matches('"').to_string();
    let mut used = 0;
    for name in ["builds.yml", "tests.yml", "audit.yml"] {
        let text = std::fs::read_to_string(Path::new(ROOT).join(".github/workflows").join(name)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            let Some(at) = line.find("dtolnay/rust-toolchain@") else { continue };
            used += 1;
            assert_eq!(&line[at + "dtolnay/rust-toolchain@".len()..], "master", "{name}:{}: a version by name, not a channel", n + 1);
            let given = lines[n + 1..].iter().take(4).find_map(|l| l.trim().strip_prefix("toolchain: ")).unwrap_or_else(|| panic!("{name}:{}: no toolchain", n + 1));
            assert!(given == "${{ steps.rust.outputs.toolchain }}" || given == format!("\"{floor}\""), "{name}:{}: {given}", n + 1);
        }
        assert!(!text.contains("rust-toolchain@stable"));
        assert!(text.contains("head -n 1 .github/rust-toolchain"), "{name} does not read .github/rust-toolchain");
    }
    assert!(used >= 4, "{used}");
}
