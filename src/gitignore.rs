//! What a game folder's `.gitignore` files leave out, read without the
//! `git` program (the IDE lists, searches and builds only what belongs to
//! the game: test renders, notes and the like lie in ignored folders).
//!
//! A small matcher of the patterns people write: names (`notes`, matched
//! at any depth), `dir/` (folders only), `*.ext`, a leading `/` or any
//! other `/` in the middle (from the folder the `.gitignore` lies in),
//! `?`, `[a-z]` and `[!a]`, `**` (`**/x`, `a/**`, `a/**/b`), `!` to take an
//! ignored path back, `\` to escape, `#` comments. Rules of a deeper
//! `.gitignore` win over a shallower one's, the last matching rule over
//! the ones before it; what lies in an ignored folder is ignored whatever
//! a later `!` says, as git has it. Not read: `.git/info/exclude`, the
//! user's global ignore file, case-insensitive file systems' folding.
//!
//! Which `.gitignore` files count: the game folder's and its subfolders',
//! and those above the game folder up to the root of the git repository
//! it lies in (the folder with `.git`; no repository: none above).

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One line of a `.gitignore`.
#[derive(Debug, PartialEq)]
struct Rule {
    pattern: Vec<char>,
    /// `!`: the path is taken back.
    negated: bool,
    /// A trailing `/`: only folders.
    dir_only: bool,
    /// Matched against the path from the `.gitignore`'s folder, not any
    /// one name.
    anchored: bool,
}

impl Rule {
    fn parse(line: &str) -> Option<Rule> {
        let mut line = line.trim_end_matches(['\r', '\n']);
        // Trailing spaces go unless escaped.
        while line.ends_with(' ') && !line.ends_with("\\ ") {
            line = &line[..line.len() - 1];
        }
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (negated, mut line) = match line.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        // `\#` and `\!` at the start name those characters.
        if line.starts_with("\\#") || line.starts_with("\\!") {
            line = &line[1..];
        }
        let dir_only = line.ends_with('/');
        let line = line.trim_end_matches('/');
        let anchored = line.contains('/');
        let line = line.trim_start_matches('/');
        if line.is_empty() {
            return None;
        }
        Some(Rule { pattern: line.chars().collect(), negated, dir_only, anchored })
    }

    /// Whether the rule names `path` (from its `.gitignore`'s folder).
    fn names(&self, path: &str, is_dir: bool) -> bool {
        if self.dir_only && !is_dir {
            return false;
        }
        let target = if self.anchored { path } else { path.rsplit('/').next().unwrap_or(path) };
        glob(&self.pattern, &target.chars().collect::<Vec<_>>())
    }
}

/// `pattern` against `text` (both whole): `*` and `?` stop at `/`, `**` does not.
fn glob(p: &[char], t: &[char]) -> bool {
    match p {
        [] => t.is_empty(),
        // `**/` (at the start or after a `/`): any folders, none too.
        ['*', '*', '/', rest @ ..] => glob(rest, t) || (0..t.len()).any(|i| t[i] == '/' && glob(rest, &t[i + 1..])),
        // `/**/`: a `/`, then any folders, none too.
        ['/', '*', '*', '/', rest @ ..] => glob_from_slash(rest, t),
        // `/**` at the end: everything inside.
        ['/', '*', '*'] => t.len() > 1 && t[0] == '/',
        ['*', rest @ ..] => {
            let run = t.iter().take_while(|&&c| c != '/').count();
            (0..=run).any(|k| glob(rest, &t[k..]))
        }
        ['?', rest @ ..] => matches!(t.first(), Some(&c) if c != '/') && glob(rest, &t[1..]),
        ['[', rest @ ..] => match class(rest) {
            Some((set, after)) => matches!(t.first(), Some(&c) if c != '/' && set.has(c)) && glob(after, &t[1..]),
            None => t.first() == Some(&'[') && glob(rest, &t[1..]),
        },
        ['\\', c, rest @ ..] => t.first() == Some(c) && glob(rest, &t[1..]),
        [c, rest @ ..] => t.first() == Some(c) && glob(rest, &t[1..]),
    }
}

/// After `/**/`: `rest` must match from a `/` in `t` on, the folders in
/// between being any.
fn glob_from_slash(rest: &[char], t: &[char]) -> bool {
    (0..t.len()).any(|i| t[i] == '/' && glob(&[&['/'][..], rest].concat(), &t[i..]))
}

/// A `[...]` class.
struct Class {
    negated: bool,
    ranges: Vec<(char, char)>,
}

impl Class {
    fn has(&self, c: char) -> bool {
        self.ranges.iter().any(|&(a, b)| a <= c && c <= b) != self.negated
    }
}

/// The class `p` begins (after its `[`) and what follows its `]`; None if
/// it is not closed.
fn class(p: &[char]) -> Option<(Class, &[char])> {
    let (negated, mut i) = match p.first() {
        Some('!' | '^') => (true, 1),
        _ => (false, 0),
    };
    let first = i;
    let mut ranges = Vec::new();
    while i < p.len() {
        // A `]` first is itself.
        if p[i] == ']' && i > first {
            return Some((Class { negated, ranges }, &p[i + 1..]));
        }
        let a = if p[i] == '\\' && i + 1 < p.len() {
            i += 1;
            p[i]
        } else {
            p[i]
        };
        if p.get(i + 1) == Some(&'-') && p.get(i + 2).is_some_and(|&b| b != ']') {
            ranges.push((a, p[i + 2]));
            i += 3;
        } else {
            ranges.push((a, a));
            i += 1;
        }
    }
    None
}

/// The rules of one `.gitignore` and the folder it lies in.
#[derive(Debug)]
struct Layer {
    dir: PathBuf,
    rules: Vec<Rule>,
}

impl Layer {
    fn read(dir: &Path) -> Option<Arc<Layer>> {
        let text = std::fs::read(dir.join(".gitignore")).ok()?;
        let rules: Vec<Rule> = String::from_utf8_lossy(&text).lines().filter_map(Rule::parse).collect();
        (!rules.is_empty()).then(|| Arc::new(Layer { dir: dir.to_path_buf(), rules }))
    }
}

/// What the `.gitignore` files that count for a game folder leave out.
/// Paths are the game's own: from its folder, `/` between names.
#[derive(Clone, Debug, Default)]
pub struct Ignore {
    /// The game folder, as the paths are taken from.
    root: PathBuf,
    /// Shallowest first: a later layer's rule wins.
    layers: Vec<Arc<Layer>>,
}

impl Ignore {
    /// The rules for the game in `dir`: the `.gitignore` files above it up to
    /// its git repository's root (none if it lies in none), and its own.
    pub fn for_game(dir: &Path) -> Self {
        let root = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let mut above = Vec::new();
        let mut at = root.as_path();
        let mut repository = root.join(".git").exists();
        while !repository {
            let Some(parent) = at.parent() else { break };
            at = parent;
            above.push(at.to_path_buf());
            repository = at.join(".git").exists();
        }
        // Not in a repository: no folder above counts.
        if !repository {
            above.clear();
        }
        let mut layers: Vec<Arc<Layer>> = above.iter().rev().filter_map(|d| Layer::read(d)).collect();
        layers.extend(Layer::read(&root));
        Self { root, layers }
    }

    /// The rules in effect inside the game's folder `folder` (its path from
    /// the game's): these and its own `.gitignore`.
    pub fn entered(&self, folder: &str) -> Self {
        let mut next = self.clone();
        next.layers.extend(Layer::read(&self.root.join(folder)));
        next
    }

    /// Whether the path (a folder if `is_dir`) is left out: it, or a folder
    /// it lies in, is ignored.
    pub fn ignores(&self, path: &str, is_dir: bool) -> bool {
        let names: Vec<&str> = path.split('/').filter(|n| !n.is_empty()).collect();
        let mut so_far = String::new();
        for (i, name) in names.iter().enumerate() {
            if !so_far.is_empty() {
                so_far.push('/');
            }
            so_far.push_str(name);
            if self.decides(&so_far, i + 1 < names.len() || is_dir) {
                return true;
            }
        }
        false
    }

    /// What the rules say of exactly this path: the last rule that names it.
    fn decides(&self, path: &str, is_dir: bool) -> bool {
        let full = self.root.join(path);
        let mut ignored = false;
        for layer in &self.layers {
            let Ok(within) = full.strip_prefix(&layer.dir) else { continue };
            let within = within.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            for rule in layer.rules.iter().filter(|r| r.names(&within, is_dir)) {
                ignored = !rule.negated;
            }
        }
        ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> Ignore {
        let layer = Layer { dir: PathBuf::from("/game"), rules: text.lines().filter_map(Rule::parse).collect() };
        Ignore { root: PathBuf::from("/game"), layers: vec![Arc::new(layer)] }
    }

    /// Names, folders, extensions, anchoring, negation, comments and the
    /// globs, as git reads them.
    #[test]
    fn the_common_patterns_match_as_git_reads_them() {
        let ig = rules("# a comment\n\nnotes/\n*.log\n/top.txt\ndocs/*.md\n!keep.log\nout?\nsrc/**/gen\nbuild/**\n[ab]x\n*.py[co]\n\\#hash\n");
        for (path, dir, ignored) in [
            ("notes", true, true),
            ("notes", false, false),
            ("a/notes", true, true),
            ("notes/deep/file.txt", false, true),
            ("a.log", false, true),
            ("sub/a.log", false, true),
            ("keep.log", false, false),
            ("top.txt", false, true),
            ("sub/top.txt", false, false),
            ("docs/a.md", false, true),
            ("docs/deep/a.md", false, false),
            ("out1", false, true),
            ("out12", false, false),
            ("src/gen", true, true),
            ("src/a/b/gen", false, true),
            ("other/gen", false, false),
            ("build/x.bin", false, true),
            ("build", true, false),
            ("ax", false, true),
            ("cx", false, false),
            ("m.pyc", false, true),
            ("m.pyx", false, false),
            ("#hash", false, true),
            ("main.lua", false, false),
        ] {
            assert_eq!(ig.ignores(path, dir), ignored, "{path} (dir: {dir})");
        }
    }

    /// A file in an ignored folder stays ignored whatever a later `!` says,
    /// and a `!` takes back what an earlier rule ignored.
    #[test]
    fn nothing_comes_back_out_of_an_ignored_folder() {
        let ig = rules("shots/\n!shots/keep.png\n*.png\n!logo.png\n");
        assert!(ig.ignores("shots/keep.png", false));
        assert!(ig.ignores("a.png", false));
        assert!(!ig.ignores("logo.png", false));
    }

    /// The game folder's `.gitignore`, its subfolders' (from their own
    /// folder), and those above it up to the repository's root count; above
    /// a game that lies in no repository none do; a deeper `!` wins.
    #[test]
    fn gitignore_files_count_where_git_would_read_them() {
        let top = std::env::temp_dir().join(format!("voxcade-ignore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let game = top.join("repo/games/g");
        std::fs::create_dir_all(game.join("sub")).unwrap();
        std::fs::create_dir_all(top.join("repo/.git")).unwrap();
        std::fs::write(top.join(".gitignore"), "outside.txt\n").unwrap();
        std::fs::write(top.join("repo/.gitignore"), "*.tmp\nscratch/\n").unwrap();
        std::fs::write(game.join(".gitignore"), "!kept.tmp\n/own.txt\n").unwrap();
        std::fs::write(game.join("sub/.gitignore"), "local.txt\n/only-here.txt\n").unwrap();
        let ig = Ignore::for_game(&game);
        assert!(ig.ignores("a.tmp", false) && ig.ignores("scratch", true), "the repository's");
        assert!(!ig.ignores("kept.tmp", false), "the game's own ! wins");
        assert!(ig.ignores("own.txt", false) && !ig.ignores("sub/own.txt", false));
        assert!(!ig.ignores("outside.txt", false), "above the repository: not read");
        let sub = ig.entered("sub");
        assert!(sub.ignores("sub/local.txt", false) && sub.ignores("sub/only-here.txt", false) && !sub.ignores("only-here.txt", false));
        assert!(!ig.ignores("sub/local.txt", false), "a subfolder's rules are its own");
        // No repository: no folder above counts, the game's own does.
        let alone = top.join("alone");
        std::fs::create_dir_all(&alone).unwrap();
        std::fs::write(alone.join(".gitignore"), "mine\n").unwrap();
        let ig = Ignore::for_game(&alone);
        assert!(ig.ignores("mine", false) && !ig.ignores("outside.txt", false));
        let _ = std::fs::remove_dir_all(&top);
    }
}
