//! Picture tests (audit Nr 43, the owner's D12: the references are numbers
//! in text, `tests/pictures.txt`, never PNGs in the repository).
//!
//! A handful of pictures are drawn by the program's own `shot`, `edit
//! --shot` and `ide --shot` (a game frame of two games, a menu, the voxel
//! editor, the IDE), each cut into an 8 by 8 grid; the mean colour of every
//! cell is the reference. A GPU draws the same picture to within a few
//! levels (the filter, the blur and the shadow's edge differ in the last
//! bits between drivers, see below), so a cell holds while every channel of
//! its mean is within `TOLERANCE` levels of 255 of the reference: robust
//! across GPUs, yet a moved sprite, a lost shadow, a missing panel or a
//! changed colour moves some cell by far more. Checked by hand on two
//! different GPUs (the RX 9070 XT and the Ryzen's Radeon 610M, both RADV);
//! on a software renderer (lavapipe) a different `VOXCADE_PICTURE_TOLERANCE`
//! may be needed (the Tests workflow sets it).
//!
//! A test that cannot draw (no GPU, no Vulkan driver) FAILS, it never
//! skips: a green run means the pictures were drawn and held.
//!
//! On purpose changed pictures: renew the numbers with
//! `VOXCADE_PICTURES=renew cargo test --release --test pictures`, look at
//! the diff of `tests/pictures.txt`, and say in the commit what changed.
//! The pictures themselves can be made again for looking with the command
//! each failure prints.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

const GRID: usize = 8;
/// Levels (of 255) a cell's mean colour may differ per channel.
const TOLERANCE: f32 = 6.0;

/// One picture: its name, the program's arguments (the game's folder first
/// where it has one) and the way to the PNG, which `{out}` stands for.
struct Picture {
    name: &'static str,
    args: &'static [&'static str],
}

const PICTURES: &[Picture] = &[
    Picture { name: "voxkeep_frame", args: &["shot", "games/voxkeep", "--frames", "60", "--size", "640x360", "--out", "{out}"] },
    Picture { name: "pulsar_frame", args: &["shot", "games/pulsar", "--frames", "90", "--size", "640x360", "--out", "{out}"] },
    Picture { name: "sokovox_title_menu", args: &["shot", "games/sokovox", "--screen", "title", "--size", "640x360", "--out", "{out}"] },
    Picture { name: "voxkeep_block_editor", args: &["edit", "games/voxkeep", "--block", "stone", "--size", "960x540", "--shot", "{out}"] },
    Picture { name: "sokovox_ide", args: &["ide", "games/sokovox", "--size", "960x540", "--shot", "{out}"] },
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A folder of the test's own, removed when dropped, a panic included (as
/// `scratch::Scratch`, which is not public: the same naming, so
/// `tools/check_tmp.sh` sees a leftover).
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        Self::new_named("one")
    }
    fn new_named(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("voxcade-pictures-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Draws a picture with the program (its settings and the IDE's and the
/// editors' files in a folder of their own, so nothing of the person's
/// running the test changes it) and reads the PNG: width, height, RGBA.
fn draw(picture: &Picture, dir: &Path) -> (usize, usize, Vec<u8>) {
    let args: Vec<String> = picture.args.iter().map(|a| a.to_string()).collect();
    draw_with(picture.name, &args, dir)
}

/// `draw` with the program's arguments given whole (`{out}` the PNG).
fn draw_with(name: &str, args: &[String], dir: &Path) -> (usize, usize, Vec<u8>) {
    let out = dir.join(format!("{}.png", name));
    let args: Vec<String> = args.iter().map(|a| a.replace("{out}", &out.to_string_lossy())).collect();
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_voxcade"))
        .args(&args)
        .current_dir(root())
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("APPDATA", home.join("config"))
        .output()
        .expect("the voxcade program starts");
    assert!(
        result.status.success(),
        "picture {}: `voxcade {}` failed (a GPU, or a Vulkan driver, is needed: lavapipe will do):\n{}{}",
        name,
        args.join(" "),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&out).expect("the picture was written")));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let channels = info.color_type.samples();
    assert!(channels == 3 || channels == 4, "picture {}: unexpected PNG colour type", name);
    let mut rgb = Vec::with_capacity(info.width as usize * info.height as usize * 3);
    for px in buf[..info.buffer_size()].chunks(channels) {
        rgb.extend_from_slice(&px[..3]);
    }
    (info.width as usize, info.height as usize, rgb)
}

/// The mean colour of each of the grid's cells, row by row.
fn means(width: usize, height: usize, rgb: &[u8]) -> Vec<[u8; 3]> {
    means_of(width, height, rgb, GRID, GRID)
}

/// The mean colour of each cell of a grid `cols` by `rows`, row by row.
fn means_of(width: usize, height: usize, rgb: &[u8], cols: usize, rows: usize) -> Vec<[u8; 3]> {
    let mut cells = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let (y0, y1) = (row * height / rows, (row + 1) * height / rows);
            let (x0, x1) = (col * width / cols, (col + 1) * width / cols);
            let mut sum = [0u64; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    for c in 0..3 {
                        sum[c] += rgb[(y * width + x) * 3 + c] as u64;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            cells.push([0, 1, 2].map(|c| ((sum[c] + n / 2) / n) as u8));
        }
    }
    cells
}

const HEADER: &str = "voxcade pictures 1";

/// The file's text: a header, then for each picture its name and size and
/// the grid's rows of `rrggbb` cells.
fn write_text(pictures: &[(String, usize, usize, Vec<[u8; 3]>)]) -> String {
    let intro = format!("# The mean colour of each cell of an {GRID} by {GRID} grid over a picture, row by row.\n# Made by `VOXCADE_PICTURES=renew cargo test --release --test pictures`; see tests/pictures.rs.\n");
    write_text_as(HEADER, &intro, GRID, pictures)
}

/// A file of numbers as `write_text`'s, under `header` and `intro`, rows
/// of `cols` cells.
fn write_text_as(header: &str, intro: &str, cols: usize, pictures: &[(String, usize, usize, Vec<[u8; 3]>)]) -> String {
    let mut text = format!("{header}\n{intro}");
    for (name, w, h, cells) in pictures {
        write!(text, "\n[{name}] {w}x{h}\n").unwrap();
        for row in cells.chunks(cols) {
            let line: Vec<String> = row.iter().map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect();
            writeln!(text, "{}", line.join(" ")).unwrap();
        }
    }
    text
}

fn read_text(text: &str) -> Vec<(String, usize, usize, Vec<[u8; 3]>)> {
    read_text_as(HEADER, "tests/pictures.txt", text)
}

/// The numbers of a file `write_text_as` wrote under `header`.
fn read_text_as(header: &str, file: &str, text: &str) -> Vec<(String, usize, usize, Vec<[u8; 3]>)> {
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(header), "{file}: the first line is `{header}`");
    let mut out: Vec<(String, usize, usize, Vec<[u8; 3]>)> = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(head) = line.strip_prefix('[') {
            let (name, size) = head.split_once("] ").unwrap_or_else(|| panic!("{file}: a picture's line is `[name] WxH`"));
            let (w, h) = size.split_once('x').unwrap_or_else(|| panic!("{file}: a size is WxH"));
            out.push((name.to_string(), w.parse().unwrap(), h.parse().unwrap(), Vec::new()));
        } else {
            let cells = &mut out.last_mut().unwrap_or_else(|| panic!("{file}: cells before a picture")).3;
            for word in line.split_whitespace() {
                let v = u32::from_str_radix(word, 16).unwrap_or_else(|_| panic!("{file}: a cell is rrggbb"));
                cells.push([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
            }
        }
    }
    out
}

fn tolerance() -> f32 {
    std::env::var("VOXCADE_PICTURE_TOLERANCE").ok().and_then(|v| v.parse().ok()).unwrap_or(TOLERANCE)
}

#[test]
fn the_pictures_look_as_they_did() {
    let dir = Scratch::new();
    let file = root().join("tests/pictures.txt");
    let drawn: Vec<(String, usize, usize, Vec<[u8; 3]>)> = PICTURES
        .iter()
        .map(|p| {
            let (w, h, rgb) = draw(p, dir.path());
            (p.name.to_string(), w, h, means(w, h, &rgb))
        })
        .collect();

    if std::env::var("VOXCADE_PICTURES").is_ok_and(|v| v == "renew") {
        std::fs::write(&file, write_text(&drawn)).unwrap();
        eprintln!("pictures: the numbers in tests/pictures.txt are renewed; look at the diff");
        return;
    }

    let text = std::fs::read_to_string(&file).expect("tests/pictures.txt (renew it with VOXCADE_PICTURES=renew)");
    let kept = read_text(&text);
    let tol = tolerance();
    let mut said = String::new();
    for (name, w, h, cells) in &drawn {
        let Some((_, kw, kh, expected)) = kept.iter().find(|k| &k.0 == name) else {
            writeln!(said, "picture {name}: no numbers in tests/pictures.txt (renew them)").unwrap();
            continue;
        };
        if (kw, kh) != (w, h) || expected.len() != cells.len() {
            writeln!(said, "picture {name}: drawn {w}x{h}, kept {kw}x{kh}").unwrap();
            continue;
        }
        let moved: Vec<String> = cells
            .iter()
            .zip(expected)
            .enumerate()
            .filter(|(_, (a, b))| (0..3).any(|c| (a[c] as f32 - b[c] as f32).abs() > tol))
            .map(|(i, (a, b))| {
                format!(
                    "row {} column {} is #{:02x}{:02x}{:02x}, was #{:02x}{:02x}{:02x}",
                    i / GRID + 1,
                    i % GRID + 1,
                    a[0],
                    a[1],
                    a[2],
                    b[0],
                    b[1],
                    b[2]
                )
            })
            .collect();
        if !moved.is_empty() {
            let args = PICTURES.iter().find(|p| p.name == name).unwrap().args.join(" ");
            writeln!(said, "picture {name}: {} of {} regions moved by more than {tol} levels:\n  {}\n  look at it: voxcade {args}", moved.len(), cells.len(), moved.join("\n  ")).unwrap();
        }
    }
    for (name, ..) in &kept {
        if !PICTURES.iter().any(|p| p.name == name) {
            writeln!(said, "tests/pictures.txt holds numbers of `{name}`, which no test draws (renew them)").unwrap();
        }
    }
    assert!(said.is_empty(), "the pictures changed (on purpose: VOXCADE_PICTURES=renew cargo test --release --test pictures):\n{said}");
}

/// The finer check, where it can be robust: on one machine the same
/// picture comes out the same to the byte, drawn twice (the chunks drawn
/// in a fixed order, the particles, the faces' memory of a moving sun's
/// shadow: nothing may depend on a hash map's order or a thread's timing).
/// Exact references kept per GPU were weighed and left: every driver
/// update would renew them, and the cells above already hold across GPUs.
#[test]
fn a_picture_drawn_twice_is_the_same_to_the_byte() {
    const TWICE: &[Picture] = &[
        Picture { name: "voxkeep_frame", args: &["shot", "games/voxkeep", "--frames", "60", "--size", "640x360", "--out", "{out}"] },
        Picture { name: "pulsar_frame", args: &["shot", "games/pulsar", "--frames", "90", "--size", "640x360", "--out", "{out}"] },
        // A game moving its sun: the faces remember their shadow.
        Picture { name: "bunnyfarm_frame", args: &["shot", "games/bunnyfarm", "--frames", "60", "--size", "640x360", "--out", "{out}"] },
    ];
    let (one, two) = (Scratch::new_named("twice"), Scratch::new_named("again"));
    for p in TWICE {
        let (a, b) = (draw(p, one.path()), draw(p, two.path()));
        assert!(a == b, "picture {}: drawn twice on this machine, the two differ (voxcade {})", p.name, p.args.join(" "));
    }
}

/// Every character the pixel font writes, in lines of the small letters.
const PIXEL_LETTERS: [&str; 3] = [
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,:!?-+/%()x",
    "><=_{}~|\\^[]\";#*'abcdefghijklmnopqrstuvwyz",
    "äàâéèêëöôüûùîïçÇßœŒ&·→←▲▼✓…„«»• ÄÀÂÉÈÊËÎÏÖÔÜÛÙ",
];
/// The capitals, digits, accented letters and signs in the big letters (a
/// display font: More Blood's), lower case drawn as capitals.
const DISPLAY_LETTERS: [&str; 4] = ["ABCDEFGHIJKLMNOPQRSTUVWXYZ", "0123456789 ÄÀÂÉÈÊËÎÏÖÔÜÛÙ", "ß ÇŒÆ çœæ ?!.,:-+/%&\"'", "→«»…–’×▲▼✓•"];
/// The letters' picture's grid: cells of 8 by 8 pixels, 4 by 4 of the
/// UI's voxels, so even two voxels of a letter changed move a cell's mean
/// by some 19 levels, far past the tolerance (two GPUs drew it the same to
/// the pixel; a third differed in 3 pixels, a cell by under 2 levels).
const LETTER_GRID: (usize, usize) = (80, 45);
const LETTERS_HEADER: &str = "voxcade letters 1";

/// The finer check the coarse grid cannot make (a wrong glyph moves an 80
/// by 45 cell's mean by a level or two): a game of nothing but text,
/// every character the pixel font writes in its small letters and every
/// letter in a display font's big ones, drawn on a plain sky (the UI is
/// laid over after the blur, so nothing but the letters' own edges can
/// differ between drivers), its mean colours in a grid of 80 by 45 cells
/// against `tests/letters.txt` (renewed with the pictures'
/// `VOXCADE_PICTURES=renew`). The display font once wrote ß, Ç and Œ as
/// '?': that moves cells here.
#[test]
fn every_letter_reads_as_it_did() {
    let dir = Scratch::new_named("letters");
    let game = dir.path().join("letters");
    std::fs::create_dir_all(game.join("ui")).unwrap();
    std::fs::copy(root().join("games/moreblood/ui/display_font.png"), game.join("ui/display_font.png")).unwrap();
    std::fs::write(
        game.join("conf.lua"),
        "function conf(c)\n  c.voxcade = \"0.6.0\"\n  c.title = \"Letters\"\n  c.world.size = { 16, 16, 8 }\n  c.ui.display_font = \"ui/display_font.png\"\n  c.light.sky = { 0.1, 0.12, 0.2 }\nend\n",
    )
    .unwrap();
    // (a brace is written doubled: one alone names an action's key)
    let lua = |s: &str| format!("[==[{}]==]", s.replace('{', "{{"));
    let mut main = String::from("function on_draw(frame)\n");
    for (i, line) in PIXEL_LETTERS.iter().enumerate() {
        main += &format!("  frame:text({}, 4, {}, nil, 1)\n", lua(line), 4 + 10 * i);
    }
    for (i, line) in DISPLAY_LETTERS.iter().enumerate() {
        main += &format!("  frame:text({}, 4, {}, nil, 2)\n", lua(line), 40 + 22 * i);
    }
    main += "end\n";
    std::fs::write(game.join("main.lua"), main).unwrap();
    let args: Vec<String> = ["shot", &game.to_string_lossy(), "--frames", "2", "--size", "640x360", "--out", "{out}"].iter().map(|a| a.to_string()).collect();
    let (w, h, rgb) = draw_with("letters", &args, dir.path());
    let (cols, rows) = LETTER_GRID;
    let drawn = vec![("letters".to_string(), w, h, means_of(w, h, &rgb, cols, rows))];
    let file = root().join("tests/letters.txt");
    if std::env::var("VOXCADE_PICTURES").is_ok_and(|v| v == "renew") {
        let intro = format!("# The mean colour of each cell of a {cols} by {rows} grid over a picture of every letter, row by row.\n# Made by `VOXCADE_PICTURES=renew cargo test --release --test pictures`; see tests/pictures.rs.\n");
        std::fs::write(&file, write_text_as(LETTERS_HEADER, &intro, cols, &drawn)).unwrap();
        eprintln!("pictures: the numbers in tests/letters.txt are renewed; look at the diff");
        return;
    }
    let text = std::fs::read_to_string(&file).expect("tests/letters.txt (renew it with VOXCADE_PICTURES=renew)");
    let kept = read_text_as(LETTERS_HEADER, "tests/letters.txt", &text);
    let [(_, kw, kh, expected)] = &kept[..] else { panic!("tests/letters.txt holds one picture") };
    assert_eq!((*kw, *kh, expected.len()), (w, h, cols * rows), "tests/letters.txt is of another size (renew it)");
    let tol = tolerance();
    let moved: Vec<String> = drawn[0]
        .3
        .iter()
        .zip(expected)
        .enumerate()
        .filter(|(_, (a, b))| (0..3).any(|c| (a[c] as f32 - b[c] as f32).abs() > tol))
        .map(|(i, (a, b))| format!("row {} column {} is #{:02x}{:02x}{:02x}, was #{:02x}{:02x}{:02x}", i / cols + 1, i % cols + 1, a[0], a[1], a[2], b[0], b[1], b[2]))
        .collect();
    assert!(moved.is_empty(), "the letters changed ({} of {} cells; on purpose: VOXCADE_PICTURES=renew):\n  {}", moved.len(), cols * rows, moved.join("\n  "));
}

#[test]
fn the_picture_numbers_are_written_and_read_back() {
    let pictures = vec![("a".to_string(), 16, 8, (0..GRID * GRID).map(|i| [i as u8, 255 - i as u8, 7]).collect::<Vec<_>>())];
    assert_eq!(read_text(&write_text(&pictures)), pictures);
}

#[test]
fn a_moved_region_is_named() {
    // The mean of a cell is the cell's own pixels, nothing else.
    let (w, h) = (16, 16);
    let mut rgb = vec![10u8; w * h * 3];
    for y in 0..2 {
        for x in 0..2 {
            rgb[(y * w + x) * 3] = 250;
        }
    }
    let cells = means(w, h, &rgb);
    assert_eq!(cells[0], [250, 10, 10]);
    assert_eq!(cells[1], [10, 10, 10]);
}
