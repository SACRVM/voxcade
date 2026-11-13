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
    let out = dir.join(format!("{}.png", picture.name));
    let args: Vec<String> = picture.args.iter().map(|a| a.replace("{out}", &out.to_string_lossy())).collect();
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
        picture.name,
        args.join(" "),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&out).expect("the picture was written")));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let channels = info.color_type.samples();
    assert!(channels == 3 || channels == 4, "picture {}: unexpected PNG colour type", picture.name);
    let mut rgb = Vec::with_capacity(info.width as usize * info.height as usize * 3);
    for px in buf[..info.buffer_size()].chunks(channels) {
        rgb.extend_from_slice(&px[..3]);
    }
    (info.width as usize, info.height as usize, rgb)
}

/// The mean colour of each of the grid's cells, row by row.
fn means(width: usize, height: usize, rgb: &[u8]) -> Vec<[u8; 3]> {
    let mut cells = Vec::new();
    for row in 0..GRID {
        for col in 0..GRID {
            let (y0, y1) = (row * height / GRID, (row + 1) * height / GRID);
            let (x0, x1) = (col * width / GRID, (col + 1) * width / GRID);
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
    let mut text = format!(
        "{HEADER}\n# The mean colour of each cell of an {GRID} by {GRID} grid over a picture, row by row.\n# Made by `VOXCADE_PICTURES=renew cargo test --release --test pictures`; see tests/pictures.rs.\n"
    );
    for (name, w, h, cells) in pictures {
        write!(text, "\n[{name}] {w}x{h}\n").unwrap();
        for row in cells.chunks(GRID) {
            let line: Vec<String> = row.iter().map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect();
            writeln!(text, "{}", line.join(" ")).unwrap();
        }
    }
    text
}

fn read_text(text: &str) -> Vec<(String, usize, usize, Vec<[u8; 3]>)> {
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(HEADER), "tests/pictures.txt: the first line is `{HEADER}`");
    let mut out: Vec<(String, usize, usize, Vec<[u8; 3]>)> = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(head) = line.strip_prefix('[') {
            let (name, size) = head.split_once("] ").expect("tests/pictures.txt: a picture's line is `[name] WxH`");
            let (w, h) = size.split_once('x').expect("tests/pictures.txt: a size is WxH");
            out.push((name.to_string(), w.parse().unwrap(), h.parse().unwrap(), Vec::new()));
        } else {
            let cells = &mut out.last_mut().expect("tests/pictures.txt: cells before a picture").3;
            for word in line.split_whitespace() {
                let v = u32::from_str_radix(word, 16).expect("tests/pictures.txt: a cell is rrggbb");
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
