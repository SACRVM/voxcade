//! The console's terminal: the user's shell in a pseudo terminal in the
//! game folder, its output through alacritty's terminal emulator
//! (`alacritty_terminal`: colours, wide letters, scrollback, lines that
//! rewrap when it is resized; on Windows ConPTY), and the keys turned into
//! the bytes a terminal sends. The pseudo terminal is read on a thread of
//! its own, which wakes the window when something new arrived.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{point_to_viewport, viewport_to_point, Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use anyhow::{Context, Result};
use egui::Color32;

/// The console's ground and its letters.
pub const BACKGROUND: Color32 = Color32::from_rgb(19, 18, 24);
pub const FOREGROUND: Color32 = Color32::from_rgb(226, 218, 198);

/// The sixteen colours programs name, calm and warm like the IDE; the
/// bright yellow is its yellow.
const ANSI: [[u8; 3]; 16] = [
    [45, 42, 54],
    [214, 104, 92],
    [152, 187, 108],
    [224, 182, 92],
    [110, 152, 206],
    [188, 128, 196],
    [106, 180, 180],
    [200, 192, 176],
    [110, 104, 100],
    [240, 128, 112],
    [176, 214, 128],
    [255, 214, 90],
    [136, 178, 232],
    [214, 152, 222],
    [132, 206, 206],
    [236, 230, 214],
];

/// A terminal of `columns` by `lines` cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub columns: u16,
    pub lines: u16,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines as usize
    }

    fn screen_lines(&self) -> usize {
        self.lines as usize
    }

    fn columns(&self) -> usize {
        self.columns as usize
    }
}

/// What the reading thread hands to the window.
struct Shared {
    /// Where answers to the program's questions go (its cursor position,
    /// the colours): into the pseudo terminal.
    sender: OnceLock<EventLoopSender>,
    /// The program has ended, everything it wrote read.
    ended: AtomicBool,
    /// Text the program asked to put on the clipboard (OSC 52).
    copies: Mutex<Vec<String>>,
    /// How many times output arrived (what a folded console shows quietly).
    arrivals: AtomicU64,
    wake: Box<dyn Fn() + Send + Sync>,
}

/// The emulator's and the reading thread's line to the window.
#[derive(Clone)]
pub struct Listener(Arc<Shared>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => {
                if let Some(sender) = self.0.sender.get() {
                    let _ = sender.send(Msg::Input(text.into_bytes().into()));
                }
            }
            Event::ClipboardStore(_, text) => self.0.copies.lock().expect("copies").push(text),
            Event::Exit => self.0.ended.store(true, Ordering::Release),
            Event::Wakeup => {
                self.0.arrivals.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        (self.0.wake)();
    }
}

/// A running terminal.
pub struct Terminal {
    term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    shared: Arc<Shared>,
    size: Size,
    /// The shell's process (on Unix), to tell whether something else runs
    /// in the foreground.
    shell: Option<u32>,
}

impl Terminal {
    /// Starts the user's shell, or `command` through the system's shell,
    /// in `dir`, on a terminal of `size` cells of `cell` pixels; `wake` is
    /// called (from another thread) whenever there is something new to
    /// draw.
    pub fn start(dir: &Path, command: Option<&str>, size: Size, cell: [u16; 2], wake: impl Fn() + Send + Sync + 'static) -> Result<Self> {
        let env = HashMap::from([("TERM".to_string(), "xterm-256color".to_string()), ("COLORTERM".to_string(), "truecolor".to_string())]);
        let shell = command.map(|c| {
            if cfg!(windows) {
                tty::Shell::new("cmd.exe".into(), vec!["/C".into(), c.into()])
            } else {
                tty::Shell::new("/bin/sh".into(), vec!["-c".into(), c.into()])
            }
        });
        // (The rest: Windows has a field more.)
        #[allow(clippy::needless_update)]
        let options = tty::Options { shell, working_directory: Some(dir.to_path_buf()), drain_on_exit: true, env, ..Default::default() };
        let pty = tty::new(&options, window_size(size, cell), 0).context("starting the console's shell")?;
        #[cfg(unix)]
        let shell = Some(pty.child().id());
        #[cfg(not(unix))]
        let shell = None;
        let shared = Arc::new(Shared { sender: OnceLock::new(), ended: AtomicBool::new(false), copies: Mutex::default(), arrivals: AtomicU64::new(0), wake: Box::new(wake) });
        let term = Arc::new(FairMutex::new(Term::new(Config::default(), &size, Listener(shared.clone()))));
        let reader = EventLoop::new(term.clone(), Listener(shared.clone()), pty, true, false).context("the console's reader")?;
        let sender = reader.channel();
        let _ = shared.sender.set(sender.clone());
        reader.spawn();
        Ok(Self { term, sender, shared, size, shell })
    }

    /// Sends `bytes` to the program, as if typed; the view goes back to
    /// the bottom.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let bytes = bytes.into();
        if !bytes.is_empty() {
            self.term.lock().scroll_display(Scroll::Bottom);
            let _ = self.sender.send(Msg::Input(bytes));
        }
    }

    /// Pastes `text`: marked as pasted if the program asked for that (so
    /// that a line break in it runs nothing), else with the line breaks a
    /// terminal sends.
    pub fn paste(&self, text: &str) {
        let bracketed = self.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        self.write(paste_bytes(text, bracketed));
    }

    /// Makes the terminal `size` cells of `cell` pixels big; its lines
    /// rewrap.
    pub fn resize(&mut self, size: Size, cell: [u16; 2]) {
        if size != self.size {
            self.size = size;
            self.term.lock().resize(size);
            let _ = self.sender.send(Msg::Resize(window_size(size, cell)));
        }
    }

    /// Whether the program has ended.
    pub fn ended(&self) -> bool {
        self.shared.ended.load(Ordering::Acquire)
    }

    /// Whether a program other than the shell runs in the foreground (one
    /// it started: Claude, a build); None where the system cannot tell
    /// (only Linux says, through `/proc`).
    pub fn busy(&self) -> Option<bool> {
        if self.ended() {
            return Some(false);
        }
        foreground_job(self.shell?)
    }

    /// How many times output has arrived since the start: it changes when
    /// the program wrote something new.
    pub fn arrivals(&self) -> u64 {
        self.shared.arrivals.load(Ordering::Relaxed)
    }

    /// The text the program asked to copy since last asked.
    pub fn copies(&self) -> Vec<String> {
        std::mem::take(&mut *self.shared.copies.lock().expect("copies"))
    }

    /// The emulator, locked against the reading thread for `f`.
    pub fn with<R>(&self, f: impl FnOnce(&mut Term<Listener>) -> R) -> R {
        f(&mut self.term.lock())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

fn window_size(size: Size, cell: [u16; 2]) -> WindowSize {
    WindowSize { num_lines: size.lines, num_cols: size.columns, cell_width: cell[0], cell_height: cell[1] }
}

/// Whether the process `pid`'s terminal has another process group in the
/// foreground than its own (a program it started), from Linux's
/// `/proc/<pid>/stat`: after the name in brackets come the state, the
/// parent, the process group, the session, the terminal and the
/// terminal's foreground group.
fn foreground_job(pid: u32) -> Option<bool> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = stat.get(stat.rfind(')')? + 1..)?.split_whitespace().collect();
    let (group, foreground): (i64, i64) = (fields.get(2)?.parse().ok()?, fields.get(5)?.parse().ok()?);
    Some(foreground > 0 && foreground != group)
}

/// Pasted text as a terminal sends it.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        // No escape inside, or the text could end the paste itself.
        let text = text.replace('\x1b', "");
        [b"\x1b[200~", text.as_bytes(), b"\x1b[201~"].concat()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// The last `lines` lines of what the terminal holds, the scrollback
/// too, as plain text: lines that wrapped joined, trailing blanks of each
/// line and blank lines at the end cut off.
pub fn history_text<T>(term: &Term<T>, lines: usize) -> String {
    let (top, bottom) = (term.topmost_line(), term.bottommost_line());
    let text = term.bounds_to_string(Point::new(top, Column(0)), Point::new(bottom, Column(term.columns().saturating_sub(1))));
    last_lines(&text, lines)
}

/// The last `n` lines of `text` (at least one), without blank ones at the
/// end and without spaces at the ends of lines.
pub fn last_lines(text: &str, n: usize) -> String {
    let all: Vec<&str> = text.lines().map(str::trim_end).collect();
    let end = all.iter().rposition(|l| !l.is_empty()).map_or(0, |i| i + 1);
    all[end.saturating_sub(n.max(1))..end].join("\n")
}

/// One cell as drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub c: char,
    pub fg: Color32,
    pub bg: Color32,
    /// The letter takes two cells (CJK, many emoji)...
    pub wide: bool,
    /// ...and this is the second, drawn by the first.
    pub spacer: bool,
    pub underline: bool,
    pub selected: bool,
}

impl Default for Glyph {
    fn default() -> Self {
        Self { c: ' ', fg: FOREGROUND, bg: BACKGROUND, wide: false, spacer: false, underline: false, selected: false }
    }
}

/// What the terminal shows now: its lines cell by cell, the cursor (line,
/// column, shape) unless hidden or scrolled away.
pub struct Picture {
    pub lines: Vec<Vec<Glyph>>,
    pub cursor: Option<(usize, usize, CursorShape)>,
}

/// The terminal's view as cells with their colours.
pub fn picture<T: EventListener>(term: &Term<T>) -> Picture {
    let content = term.renderable_content();
    let (columns, lines) = (term.columns(), term.screen_lines());
    let mut out = vec![vec![Glyph::default(); columns]; lines];
    for cell in content.display_iter {
        let Some(at) = point_to_viewport(content.display_offset, cell.point) else { continue };
        let Some(glyph) = out.get_mut(at.line).and_then(|l| l.get_mut(at.column.0)) else { continue };
        let flags = cell.flags;
        let mut fg = colour(cell.fg, content.colors, flags);
        let mut bg = colour(cell.bg, content.colors, Flags::empty());
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        *glyph = Glyph {
            c: cell.c,
            fg,
            bg,
            wide: flags.contains(Flags::WIDE_CHAR),
            spacer: flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            selected: content.selection.is_some_and(|s| s.contains(cell.point)),
        };
    }
    let cursor = match content.cursor.shape {
        CursorShape::Hidden => None,
        shape => point_to_viewport(content.display_offset, content.cursor.point).map(|p| (p.line, p.column.0, shape)),
    };
    Picture { lines: out, cursor }
}

/// A colour as the program named it, through the colours it set itself;
/// bold makes the eight plain colours bright, dim makes them dim.
fn colour(c: Color, colors: &Colors, flags: Flags) -> Color32 {
    let rgb = |c: Rgb| Color32::from_rgb(c.r, c.g, c.b);
    let indexed = |i: usize| colors[i].map_or_else(|| palette(i), rgb);
    let dim = |c: Color32| c.lerp_to_gamma(BACKGROUND, 0.4);
    match c {
        Color::Spec(c) => rgb(c),
        Color::Indexed(i) => {
            let i = i as usize;
            if flags.contains(Flags::BOLD) && i < 8 { indexed(i + 8) } else { indexed(i) }
        }
        Color::Named(n) => {
            let i = n as usize;
            let c = match n {
                NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::DimForeground => colors[NamedColor::Foreground].map_or(FOREGROUND, rgb),
                NamedColor::Background => colors[NamedColor::Background].map_or(BACKGROUND, rgb),
                NamedColor::Cursor => colors[NamedColor::Cursor].map_or(FOREGROUND, rgb),
                _ if i < 8 && flags.contains(Flags::BOLD) => indexed(i + 8),
                _ if i < 16 => indexed(i),
                // The dim ones, after Cursor: dim forms of the eight.
                _ => dim(indexed(i - NamedColor::DimBlack as usize)),
            };
            if flags.contains(Flags::DIM) || n == NamedColor::DimForeground { dim(c) } else { c }
        }
    }
}

/// The 256 colours: the sixteen, a cube of 6 by 6 by 6, 24 greys.
fn palette(i: usize) -> Color32 {
    match i {
        0..16 => Color32::from_rgb(ANSI[i][0], ANSI[i][1], ANSI[i][2]),
        16..232 => {
            let level = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
            let v = i - 16;
            Color32::from_rgb(level(v / 36), level(v / 6 % 6), level(v % 6))
        }
        232..256 => {
            let g = (8 + (i - 232) * 10) as u8;
            Color32::from_rgb(g, g, g)
        }
        _ => FOREGROUND,
    }
}

/// The cell at `line` and `column` of the view as a point of the
/// terminal's history.
pub fn point_at<T>(term: &Term<T>, line: usize, column: usize) -> Point {
    let line = line.min(term.screen_lines().saturating_sub(1));
    let column = column.min(term.columns().saturating_sub(1));
    viewport_to_point(term.grid().display_offset(), Point::new(line, Column(column)))
}

/// Starts a selection at `point` (`words`: whole words, as a double
/// click does), or carries it on to there.
pub fn select<T>(term: &mut Term<T>, point: Point, side: Side, start: Option<bool>) {
    match start {
        Some(words) => term.selection = Some(Selection::new(if words { SelectionType::Semantic } else { SelectionType::Simple }, point, side)),
        None => {
            if let Some(selection) = &mut term.selection {
                selection.update(point, side);
            }
        }
    }
}

/// Scrolls the view through the history, `lines` up (or down when
/// negative).
pub fn scroll<T: EventListener>(term: &mut Term<T>, lines: i32) {
    term.scroll_display(Scroll::Delta(lines));
}

/// What the mouse did, as a program that asked for the mouse hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouse {
    /// A button (0 left, 1 middle, 2 right) pressed or let go.
    Press(u8),
    Release(u8),
    /// Moved with a button held (None: none).
    Moved(Option<u8>),
    /// The wheel, a line up or down.
    WheelUp,
    WheelDown,
}

/// The bytes a terminal sends for `mouse` at `column` and `line` (from
/// 0) with `m` held, in the terminal's `mode`: SGR's form where the
/// program asked for it, else the old one; `None` where the mode does not
/// report it (moving without a button unless every motion is asked for,
/// with a button unless drags are, the old form beyond its 223 cells).
pub fn mouse_bytes(mouse: Mouse, column: usize, line: usize, m: egui::Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    let (button, released) = match mouse {
        Mouse::Press(b) => (b as u32, false),
        Mouse::Release(b) => (b as u32, true),
        Mouse::Moved(Some(b)) if mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) => (b as u32 + 32, false),
        Mouse::Moved(None) if mode.contains(TermMode::MOUSE_MOTION) => (3 + 32, false),
        Mouse::Moved(_) => return None,
        Mouse::WheelUp => (64, false),
        Mouse::WheelDown => (65, false),
    };
    let held = if m.shift { 4 } else { 0 } + if m.alt { 8 } else { 0 } + if m.ctrl { 16 } else { 0 };
    let (column, line) = (column + 1, line + 1);
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if released { 'm' } else { 'M' };
        return Some(format!("\x1b[<{};{column};{line}{end}", button + held).into_bytes());
    }
    // The old form: a release says no button; places past 223 do not fit.
    let button = if released { 3 } else { button } + held;
    if column > 223 || line > 223 {
        return None;
    }
    Some(vec![0x1b, b'[', b'M', 32 + button as u8, 32 + column as u8, 32 + line as u8])
}

/// The bytes a terminal sends for `key` with `modifiers` held, in the
/// terminal's `mode`; `None` for keys that come as typed text instead.
pub fn key_bytes(key: egui::Key, m: egui::Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    use egui::Key as K;
    // xterm's number for the modifiers held, 1 for none.
    let code = 1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.ctrl as u8;
    let cursor = |end: char| {
        if code > 1 {
            format!("\x1b[1;{code}{end}").into_bytes()
        } else if mode.contains(TermMode::APP_CURSOR) {
            format!("\x1bO{end}").into_bytes()
        } else {
            format!("\x1b[{end}").into_bytes()
        }
    };
    let tilde = |n: u8| if code > 1 { format!("\x1b[{n};{code}~") } else { format!("\x1b[{n}~") }.into_bytes();
    let function = |end: char| if code > 1 { format!("\x1b[1;{code}{end}") } else { format!("\x1bO{end}") }.into_bytes();
    Some(match key {
        // Shift+Enter: a new line in the program's input (Claude Code's
        // too), not sending it; the bytes of Alt+Enter.
        K::Enter if m.shift => b"\x1b\r".to_vec(),
        K::Enter => b"\r".to_vec(),
        K::Tab if m.shift => b"\x1b[Z".to_vec(),
        K::Tab => b"\t".to_vec(),
        K::Backspace if m.ctrl => vec![0x08],
        K::Backspace => vec![0x7f],
        K::Escape => vec![0x1b],
        K::ArrowUp => cursor('A'),
        K::ArrowDown => cursor('B'),
        K::ArrowRight => cursor('C'),
        K::ArrowLeft => cursor('D'),
        K::Home => cursor('H'),
        K::End => cursor('F'),
        K::Insert => tilde(2),
        K::Delete => tilde(3),
        K::PageUp => tilde(5),
        K::PageDown => tilde(6),
        K::F1 => function('P'),
        K::F2 => function('Q'),
        K::F3 => function('R'),
        K::F4 => function('S'),
        K::F5 => tilde(15),
        K::F6 => tilde(17),
        K::F7 => tilde(18),
        K::F8 => tilde(19),
        K::F9 => tilde(20),
        K::F10 => tilde(21),
        K::F11 => tilde(23),
        K::F12 => tilde(24),
        // Ctrl with a letter: the control characters, Ctrl+C the
        // interrupt among them.
        _ if m.ctrl && !m.alt => {
            let name = key.name();
            match name.as_bytes() {
                [c @ b'A'..=b'Z'] => vec![c - b'A' + 1],
                _ => match key {
                    K::Space => vec![0],
                    K::OpenBracket => vec![0x1b],
                    K::Backslash => vec![0x1c],
                    K::CloseBracket => vec![0x1d],
                    _ => return None,
                },
            }
        }
        _ => return None,
    })
}

/// The text of the line shown at `line` of the view, rows the terminal
/// wrapped joined (at most a screenful each way), as letters with the cell
/// each came from (its line of the view, possibly above or below it, and
/// its column), and the index of the letter at `column`.
pub fn line_at<T>(term: &Term<T>, line: usize, column: usize) -> (Vec<char>, Vec<(i32, usize)>, Option<usize>) {
    let grid = term.grid();
    let (top, bottom) = (term.topmost_line().0, term.bottommost_line().0);
    let offset = grid.display_offset() as i32;
    let columns = term.columns();
    let row = line as i32 - offset;
    let wraps = |l: i32| columns > 0 && grid[alacritty_terminal::index::Line(l)][Column(columns - 1)].flags.contains(Flags::WRAPLINE);
    let reach = term.screen_lines() as i32;
    let mut first = row;
    while first > top && first > row - reach && wraps(first - 1) {
        first -= 1;
    }
    let (mut chars, mut cells, mut at) = (Vec::new(), Vec::new(), None);
    let mut l = first;
    loop {
        for c in 0..columns {
            let cell = &grid[alacritty_terminal::index::Line(l)][Column(c)];
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            // (A wide letter answers for its second cell too.)
            if l == row && (c == column || (cell.flags.contains(Flags::WIDE_CHAR) && c + 1 == column)) {
                at = Some(chars.len());
            }
            chars.push(cell.c);
            cells.push((l + offset, c));
        }
        if l >= bottom || l >= row + reach || !wraps(l) {
            break;
        }
        l += 1;
    }
    (chars, cells, at)
}

/// Which half of the cell a point at `x` cells from the left lies in.
pub fn side(x: f32) -> Side {
    if x.fract() < 0.5 { Side::Left } else { Side::Right }
}

/// A line of the view as text, its blanks at the end dropped (for tests
/// and shots).
#[cfg(test)]
fn text(picture: &Picture) -> Vec<String> {
    picture.lines.iter().map(|l| l.iter().filter(|g| !g.spacer).map(|g| g.c).collect::<String>().trim_end().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::vte::ansi::Processor;

    /// A program the shell runs (here `sleep`, in the IDE Claude) makes
    /// the terminal busy until it ends (Linux tells; elsewhere it cannot).
    #[cfg(target_os = "linux")]
    #[test]
    fn a_program_in_the_shell_makes_it_busy() {
        let dir = std::env::temp_dir();
        let t = Terminal::start(&dir, None, Size { columns: 80, lines: 24 }, [8, 16], || {}).unwrap();
        let wait = |want: bool| {
            // (Generous: the shell starts slowly while every other test runs.)
            let until = std::time::Instant::now() + std::time::Duration::from_secs(45);
            while t.busy() != Some(want) {
                assert!(std::time::Instant::now() < until, "busy: {:?}, wanted {want}", t.busy());
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        wait(false);
        t.write(b"sleep 30\r".to_vec());
        wait(true);
        t.write(vec![0x03]);
        wait(false);
    }

    /// A program that asked for the mouse hears it as xterm tells it: the
    /// wheel as buttons 64 and 65, SGR's form when asked for, drags only
    /// when asked for.
    #[test]
    fn the_mouse_is_told_as_xterm_tells_it() {
        let none = egui::Modifiers::NONE;
        let sgr = TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE;
        assert_eq!(mouse_bytes(Mouse::WheelUp, 4, 9, none, sgr).unwrap(), b"\x1b[<64;5;10M");
        assert_eq!(mouse_bytes(Mouse::WheelDown, 0, 0, none, sgr).unwrap(), b"\x1b[<65;1;1M");
        assert_eq!(mouse_bytes(Mouse::Press(0), 2, 3, none, sgr).unwrap(), b"\x1b[<0;3;4M");
        assert_eq!(mouse_bytes(Mouse::Release(0), 2, 3, none, sgr).unwrap(), b"\x1b[<0;3;4m");
        assert_eq!(mouse_bytes(Mouse::Moved(Some(0)), 2, 3, none, sgr).unwrap(), b"\x1b[<32;3;4M");
        assert_eq!(mouse_bytes(Mouse::Moved(None), 2, 3, none, sgr), None, "no motion without a button unless asked");
        let ctrl = egui::Modifiers { ctrl: true, ..none };
        assert_eq!(mouse_bytes(Mouse::Press(2), 0, 0, ctrl, sgr).unwrap(), b"\x1b[<18;1;1M");
        let old = TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(mouse_bytes(Mouse::Press(0), 0, 0, none, old).unwrap(), [0x1b, b'[', b'M', 32, 33, 33]);
        assert_eq!(mouse_bytes(Mouse::Release(0), 0, 0, none, old).unwrap(), [0x1b, b'[', b'M', 35, 33, 33]);
        assert_eq!(mouse_bytes(Mouse::Moved(Some(0)), 0, 0, none, old), None, "no drags unless asked");
        assert_eq!(mouse_bytes(Mouse::Press(0), 300, 0, none, old), None);
    }

    /// A line the terminal wrapped is read whole, from either of its rows.
    #[test]
    fn a_wrapped_line_is_read_whole() {
        let t = term(10, 4, b"a /tmp/x/pic.png b\r\nnext");
        let (chars, cells, at) = line_at(&t, 1, 3);
        assert_eq!(chars.iter().collect::<String>().trim_end(), "a /tmp/x/pic.png b", "joined");
        assert_eq!(at.map(|i| chars[i]), Some('p'), "the cell under the mouse");
        assert_eq!(cells[at.unwrap()], (1, 3));
        assert_eq!(cells[0], (0, 0), "its first row is above");
        let (chars, _, _) = line_at(&t, 2, 0);
        assert_eq!(chars.iter().collect::<String>().trim_end(), "next", "the next line apart");
    }

    fn term(columns: u16, lines: u16, bytes: &[u8]) -> Term<VoidListener> {
        let mut term = Term::new(Config::default(), &Size { columns, lines }, VoidListener);
        Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new().advance(&mut term, bytes);
        term
    }

    /// Colours by name, by number and as RGB, bold making a plain colour
    /// bright, inverse swapping; a wide letter takes two cells.
    #[test]
    fn the_terminal_shows_colours_and_wide_letters() {
        let t = term(20, 3, "\x1b[31mred\x1b[0m \x1b[1;32mok\x1b[0m\r\n\x1b[38;2;1;2;3;48;5;196mX\x1b[0m\x1b[7mI\x1b[0m\r\n日本!".as_bytes());
        let p = picture(&t);
        assert_eq!(text(&p), ["red ok", "XI", "日本!"]);
        let cell = |l: usize, c: usize| p.lines[l][c];
        assert_eq!(cell(0, 0).fg, palette(1), "red by name");
        assert_eq!(cell(0, 4).fg, palette(10), "bold green is bright green");
        assert_eq!(cell(0, 3).fg, FOREGROUND);
        assert_eq!((cell(1, 0).fg, cell(1, 0).bg), (Color32::from_rgb(1, 2, 3), palette(196)), "RGB letters on color 196");
        assert_eq!((cell(1, 1).fg, cell(1, 1).bg), (BACKGROUND, FOREGROUND), "inverse");
        assert!(cell(2, 0).wide && cell(2, 1).spacer && cell(2, 2).wide && cell(2, 4).c == '!', "two cells a letter");
        assert_eq!(p.cursor.map(|c| (c.0, c.1)), Some((2, 5)));
    }

    /// Lines narrowed wrap, widened again they are whole as before.
    #[test]
    fn a_resized_terminal_keeps_its_lines() {
        let mut t = term(20, 5, b"one\r\ntwo two two two\r\nthree");
        let before = text(&picture(&t));
        t.resize(Size { columns: 6, lines: 5 });
        assert!(text(&picture(&t)).iter().all(|l| l.chars().count() <= 6), "wrapped: {:?}", text(&picture(&t)));
        t.resize(Size { columns: 20, lines: 5 });
        assert_eq!(text(&picture(&t)), before);
    }

    /// What `console_read` returns: the scrollback too, the last lines
    /// asked for, no blank ones at the end.
    #[test]
    fn the_terminal_reads_out_as_plain_text() {
        let t = term(10, 3, b"one  \r\ntwo\r\nthree\r\nfour\r\n\r\n");
        assert_eq!(history_text(&t, 200), "one\ntwo\nthree\nfour", "scrollback included, blanks at the end cut");
        assert_eq!(history_text(&t, 2), "three\nfour");
        assert_eq!(history_text(&t, 0), "four", "at least one line");
        assert_eq!(history_text(&term(10, 3, b""), 5), "");
        assert_eq!(last_lines("a  \n\nb\n\n\n", 10), "a\n\nb");
    }

    /// Pasted text goes in as the program asked: marked, or with the
    /// breaks a terminal sends.
    #[test]
    fn pasted_text_is_marked_when_asked() {
        assert_eq!(paste_bytes("a\nb", true), b"\x1b[200~a\nb\x1b[201~");
        assert_eq!(paste_bytes("a\nb", false), b"a\rb");
    }

    /// Keys as a terminal sends them: the cursor keys by the program's
    /// mode and with modifiers, Ctrl with a letter, Shift+Enter a new line.
    #[test]
    fn keys_become_terminal_bytes() {
        use egui::{Key, Modifiers};
        let none = Modifiers::default();
        let bytes = |key, m, mode| key_bytes(key, m, mode).map(|b| String::from_utf8(b).unwrap());
        assert_eq!(bytes(Key::ArrowUp, none, TermMode::empty()).as_deref(), Some("\x1b[A"));
        assert_eq!(bytes(Key::ArrowUp, none, TermMode::APP_CURSOR).as_deref(), Some("\x1bOA"));
        assert_eq!(bytes(Key::ArrowLeft, Modifiers::CTRL, TermMode::empty()).as_deref(), Some("\x1b[1;5D"));
        assert_eq!(bytes(Key::C, Modifiers::CTRL, TermMode::empty()).as_deref(), Some("\x03"));
        assert_eq!(bytes(Key::Enter, Modifiers::SHIFT, TermMode::empty()).as_deref(), Some("\x1b\r"));
        assert_eq!(bytes(Key::Delete, none, TermMode::empty()).as_deref(), Some("\x1b[3~"));
        assert_eq!(bytes(Key::A, none, TermMode::empty()), None, "letters come as text");
        assert_eq!(paste_bytes("a\nb", true), b"\x1b[200~a\nb\x1b[201~");
        assert_eq!(paste_bytes("a\r\nb", false), b"a\rb");
    }
}
