//! The documentation names files and tests; a name that no longer exists is
//! drift. Every backticked `src/...` path in CLAUDE.md and `docs/*.md` must
//! exist, and every test named after the word "test" or "tests" (a list
//! joined by commas or "and") must be a `fn` somewhere under `src/`,
//! `tests/` or `examples/`.

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
