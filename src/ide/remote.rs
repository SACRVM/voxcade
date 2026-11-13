//! Another session testing the IDE as a person uses it (`voxcade ide <game>
//! --remote`, the tools of `voxcade mcp --remote`): `windows`,
//! `window_shot`, `pointer`, `scroll` and `keys`. The IDE has no mouse of
//! the system's to move, so the input is its own: real SDL events pushed
//! into SDL's queue with the id of the window, which everything that reads
//! events then sees as it sees a person's (egui through `gui::input`, the
//! IDE's keys in `project`, the editors' `handle`). The mouse events carry
//! a mouse id of their own (`SYNTHETIC`): a real mouse event takes the
//! pointer's mark away again. A call becomes a job of steps, one step to a
//! frame of the loop (a click is a move, a press and a release, each
//! processed before the next, as egui needs them), and its answer is sent
//! after the last was processed. The game's keys, which the game reads from
//! the keyboard's state and no event, are held for it (`hold_ms`,
//! `Remote::held`).

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Mod, Scancode};
use sdl3::mouse::{MouseButton, MouseState};
use serde_json::{Value, json};

use super::bridge::Call;

/// The tools of this channel, listed by `voxcade mcp --remote` and answered
/// only by an IDE started with `--remote`.
pub const TOOLS: [&str; 5] = ["windows", "window_shot", "pointer", "scroll", "keys"];

/// The mouse id of synthetic mouse events (a real mouse's is small).
pub const SYNTHETIC: u32 = 0xFFFF_FFF0;

/// How long after the last pointer event a picture waits, so that a tooltip
/// (egui shows one after half a second at rest) is in it.
const SETTLE: Duration = Duration::from_millis(700);

/// The longest a key is held, in milliseconds.
pub const MOST_HOLD: u64 = 10_000;

/// A drag moves in this many steps.
const DRAG_STEPS: usize = 8;

/// Past this a picture is taken as the frame is, settled or not; past the
/// second it is not to be had (the window is hidden, say).
const LATE: Duration = Duration::from_millis(1500);
const GIVE_UP: Duration = Duration::from_secs(4);

/// What the IDE says of one of its windows.
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    /// In pixels, as the pictures are.
    pub width: u32,
    pub height: u32,
    /// Pixels to one of SDL's points, which the mouse is told in.
    pub density: f32,
    pub keyboard: bool,
    pub main: bool,
}

/// What a pointer tool does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Move,
    Click,
    DoubleClick,
    Press,
    Release,
    Drag,
}

/// The action a name stands for.
pub fn action_of(name: &str) -> Result<Action, String> {
    Ok(match name {
        "move" => Action::Move,
        "click" => Action::Click,
        "double_click" => Action::DoubleClick,
        "press" => Action::Press,
        "release" => Action::Release,
        "drag" => Action::Drag,
        other => return Err(format!("no pointer action \"{other}\" (move, click, double_click, press, release, drag)")),
    })
}

/// The mouse button a name stands for.
pub fn button_of(name: &str) -> Result<MouseButton, String> {
    Ok(match name {
        "left" => MouseButton::Left,
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        other => return Err(format!("no button \"{other}\" (left, right, middle)")),
    })
}

/// The modifier keys named ("shift", "ctrl", "alt"; "cmd" the system's
/// key), as SDL's left ones.
pub fn modifiers_of(names: &[&str]) -> Result<Mod, String> {
    let mut mods = Mod::empty();
    for name in names {
        mods |= match name.to_ascii_lowercase().as_str() {
            "shift" => Mod::LSHIFTMOD,
            "ctrl" | "control" => Mod::LCTRLMOD,
            "alt" => Mod::LALTMOD,
            "cmd" | "super" | "meta" | "gui" => Mod::LGUIMOD,
            other => return Err(format!("no modifier \"{other}\" (shift, ctrl, alt)")),
        };
    }
    Ok(mods)
}

/// A key and the modifiers held with it from a chord's text: "Escape",
/// "ctrl+s", "ctrl+shift+z", "a", "ctrl++".
pub fn parse_chord(chord: &str) -> Result<(Keycode, Mod), String> {
    let chord = chord.trim();
    // (A plus at the end is the key.)
    let (mods, key) = match chord.strip_suffix('+') {
        Some("") => ("", "plus"),
        Some(rest) => (rest.strip_suffix('+').ok_or_else(|| format!("the chord \"{chord}\" ends in a plus"))?, "plus"),
        None => match chord.rsplit_once('+') {
            Some((mods, key)) => (mods, key),
            None => ("", chord),
        },
    };
    let names: Vec<&str> = mods.split('+').filter(|m| !m.is_empty()).collect();
    Ok((key_of(key)?, modifiers_of(&names)?))
}

/// The key a name stands for (any case): the letters and digits, the
/// named keys.
pub fn key_of(name: &str) -> Result<Keycode, String> {
    let lower = name.to_ascii_lowercase();
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_ascii_alphanumeric()
    {
        return Keycode::from_u32(c as u32).ok_or_else(|| format!("unknown key \"{name}\""));
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=12).contains(n)) {
        return Ok([Keycode::F1, Keycode::F2, Keycode::F3, Keycode::F4, Keycode::F5, Keycode::F6, Keycode::F7, Keycode::F8, Keycode::F9, Keycode::F10, Keycode::F11, Keycode::F12][n as usize - 1]);
    }
    Ok(match lower.as_str() {
        "enter" | "return" => Keycode::Return,
        "escape" | "esc" => Keycode::Escape,
        "tab" => Keycode::Tab,
        "space" => Keycode::Space,
        "backspace" => Keycode::Backspace,
        "delete" | "del" => Keycode::Delete,
        "insert" => Keycode::Insert,
        "home" => Keycode::Home,
        "end" => Keycode::End,
        "pageup" | "pgup" => Keycode::PageUp,
        "pagedown" | "pgdn" => Keycode::PageDown,
        "up" => Keycode::Up,
        "down" => Keycode::Down,
        "left" => Keycode::Left,
        "right" => Keycode::Right,
        "plus" => Keycode::Plus,
        "minus" => Keycode::Minus,
        "comma" => Keycode::Comma,
        "period" => Keycode::Period,
        "slash" => Keycode::Slash,
        "backslash" => Keycode::Backslash,
        "semicolon" => Keycode::Semicolon,
        "equals" => Keycode::Equals,
        "quote" | "apostrophe" => Keycode::Apostrophe,
        "backtick" | "grave" => Keycode::Grave,
        _ => return Err(format!("unknown key \"{name}\" (a letter or digit, F1 to F12, Enter, Escape, Tab, Space, Backspace, Delete, Home, End, PageUp, PageDown, Up, Down, Left, Right, a punctuation's name)")),
    })
}

/// The text a key types with `mods` held, as a keyboard's text input has it
/// (letters, digits, the space and the plain punctuation; nothing with Ctrl,
/// Alt or the system's key down).
pub fn typed(key: Keycode, mods: Mod) -> Option<String> {
    if mods.intersects(Mod::LCTRLMOD | Mod::LALTMOD | Mod::LGUIMOD) {
        return None;
    }
    let c = match key {
        Keycode::Space => ' ',
        Keycode::Plus => '+',
        Keycode::Minus => '-',
        Keycode::Comma => ',',
        Keycode::Period => '.',
        Keycode::Slash => '/',
        Keycode::Backslash => '\\',
        Keycode::Semicolon => ';',
        Keycode::Equals => '=',
        Keycode::Apostrophe => '\'',
        Keycode::Grave => '`',
        other => char::from_u32(other as i32 as u32).filter(char::is_ascii_alphanumeric)?,
    };
    let shifted = mods.intersects(Mod::LSHIFTMOD);
    match c {
        _ if c.is_ascii_alphabetic() => Some(if shifted { c.to_ascii_uppercase() } else { c }.to_string()),
        ' ' | '+' => Some(c.to_string()),
        _ if shifted => None,
        _ => Some(c.to_string()),
    }
}

/// The modifier keys down in `mods`, each with the modifiers held once it
/// is down.
fn modifier_keys(mods: Mod) -> Vec<(Keycode, Mod)> {
    let mut held = Mod::empty();
    let mut keys = Vec::new();
    for (key, flag) in [(Keycode::LShift, Mod::LSHIFTMOD), (Keycode::LCtrl, Mod::LCTRLMOD), (Keycode::LAlt, Mod::LALTMOD), (Keycode::LGui, Mod::LGUIMOD)] {
        if mods.contains(flag) {
            held |= flag;
            keys.push((key, held));
        }
    }
    keys
}

/// A key's event, a press or a release, with the modifiers held.
fn key_event(window: u32, key: Keycode, keymod: Mod, down: bool) -> Result<Event, String> {
    // Safety: no modifier state wanted back (null).
    let scancode = unsafe { Scancode::from_keycode(key, std::ptr::null_mut()) }.ok_or_else(|| format!("{key:?} has no place on this keyboard"))?;
    Ok(if down {
        Event::KeyDown { timestamp: 0, window_id: window, keycode: Some(key), scancode: Some(scancode), keymod, repeat: false, which: 0, raw: 0 }
    } else {
        Event::KeyUp { timestamp: 0, window_id: window, keycode: Some(key), scancode: Some(scancode), keymod, repeat: false, which: 0, raw: 0 }
    })
}

/// A step of a job: what happens, one to a frame of the IDE's loop.
#[derive(Debug)]
pub enum Step {
    /// These events into SDL's queue.
    Push(Vec<Event>),
    /// Typed text for a window (SDL's text input).
    Text(u32, String),
    /// These keys held for the game this many milliseconds.
    Hold(Vec<Scancode>, u64),
    /// Nothing for this long.
    Pause(Duration),
    /// Until the pointer has rested long enough for a tooltip.
    Settle,
    /// A picture of the window, once it is drawn.
    Capture(u32),
}

/// Where the pointer is and which buttons it holds (as the events say).
#[derive(Default)]
pub struct Pointer {
    pos: Option<(f32, f32)>,
    buttons: u32,
}

/// The bit of a button in SDL's button state.
fn mask(button: MouseButton) -> u32 {
    1 << (button as u8 - 1)
}

impl Pointer {
    fn motion(&mut self, window: u32, x: f32, y: f32) -> Event {
        let (px, py) = self.pos.unwrap_or((x, y));
        self.pos = Some((x, y));
        Event::MouseMotion { timestamp: 0, window_id: window, which: SYNTHETIC, mousestate: MouseState::from_sdl_state(self.buttons), x, y, xrel: x - px, yrel: y - py }
    }

    fn button(&mut self, window: u32, button: MouseButton, down: bool, clicks: u8) -> Event {
        let (x, y) = self.pos.unwrap_or_default();
        if down {
            self.buttons |= mask(button);
            Event::MouseButtonDown { timestamp: 0, window_id: window, which: SYNTHETIC, mouse_btn: button, clicks, x, y }
        } else {
            self.buttons &= !mask(button);
            Event::MouseButtonUp { timestamp: 0, window_id: window, which: SYNTHETIC, mouse_btn: button, clicks, x, y }
        }
    }

    /// The steps of a pointer `action` at `at` (SDL's points; a drag to
    /// `to`), with `mods` held through it.
    pub fn steps(&mut self, window: u32, action: Action, button: MouseButton, at: (f32, f32), to: Option<(f32, f32)>, mods: Mod) -> Result<Vec<Step>, String> {
        let mut steps = Vec::new();
        let one = |e: Event| Step::Push(vec![e]);
        // A click or a drag is whole in itself: no button is held from an
        // earlier press that was never let go (its motions would carry it
        // into every window, a drag nobody began).
        if matches!(action, Action::Click | Action::DoubleClick | Action::Drag) {
            self.buttons = 0;
        }
        steps.push(one(self.motion(window, at.0, at.1)));
        match action {
            Action::Move => {}
            Action::Click => {
                steps.push(one(self.button(window, button, true, 1)));
                steps.push(one(self.button(window, button, false, 1)));
            }
            Action::DoubleClick => {
                for clicks in [1, 2] {
                    steps.push(one(self.button(window, button, true, clicks)));
                    steps.push(one(self.button(window, button, false, clicks)));
                }
            }
            Action::Press => steps.push(one(self.button(window, button, true, 1))),
            Action::Release => steps.push(one(self.button(window, button, false, 1))),
            Action::Drag => {
                let to = to.ok_or("a drag needs to_x and to_y")?;
                steps.push(one(self.button(window, button, true, 1)));
                for i in 1..=DRAG_STEPS {
                    let t = i as f32 / DRAG_STEPS as f32;
                    steps.push(one(self.motion(window, at.0 + (to.0 - at.0) * t, at.1 + (to.1 - at.1) * t)));
                }
                steps.push(one(self.button(window, button, false, 1)));
            }
        }
        wrap(window, mods, steps)
    }

    /// The steps of the wheel turned `dx`, `dy` steps at `at`.
    pub fn scroll(&mut self, window: u32, at: (f32, f32), dx: f32, dy: f32) -> Vec<Step> {
        let wheel = Event::MouseWheel { timestamp: 0, window_id: window, which: SYNTHETIC, x: dx, y: dy, direction: sdl3::mouse::MouseWheelDirection::Normal, mouse_x: at.0, mouse_y: at.1, integer_x: dx.round() as i32, integer_y: dy.round() as i32 };
        vec![Step::Push(vec![self.motion(window, at.0, at.1)]), Step::Push(vec![wheel])]
    }
}

/// The events of letting go of `keys` (from `modifier_keys`), the last
/// pressed first.
fn releases(window: u32, keys: &[(Keycode, Mod)]) -> Result<Vec<Event>, String> {
    (0..keys.len()).rev().map(|i| key_event(window, keys[i].0, if i == 0 { Mod::empty() } else { keys[i - 1].1 }, false)).collect()
}

/// `steps` with the modifier keys pressed before and let go after.
fn wrap(window: u32, mods: Mod, steps: Vec<Step>) -> Result<Vec<Step>, String> {
    if mods.is_empty() {
        return Ok(steps);
    }
    let keys = modifier_keys(mods);
    let down = keys.iter().map(|&(k, m)| key_event(window, k, m, true)).collect::<Result<Vec<_>, _>>()?;
    let up = releases(window, &keys)?;
    let mut all = vec![Step::Push(down)];
    all.extend(steps);
    all.push(Step::Push(up));
    Ok(all)
}

/// The steps of a key chord typed in `window`; `hold`: how long the game
/// has the keys held (the events press and release as usual).
pub fn key_steps(window: u32, key: Keycode, mods: Mod, hold: Option<u64>) -> Result<Vec<Step>, String> {
    let keys = modifier_keys(mods);
    let mut press = keys.iter().map(|&(k, m)| key_event(window, k, m, true)).collect::<Result<Vec<_>, _>>()?;
    press.push(key_event(window, key, mods, true)?);
    let mut release = vec![key_event(window, key, mods, false)?];
    release.extend(releases(window, &keys)?);
    let mut steps = vec![Step::Push(press)];
    if let Some(text) = typed(key, mods) {
        steps.push(Step::Text(window, text));
    }
    if let Some(ms) = hold {
        let codes = keys.iter().map(|&(k, _)| k).chain([key]).filter_map(|k| unsafe { Scancode::from_keycode(k, std::ptr::null_mut()) }).collect();
        steps.push(Step::Hold(codes, ms));
        steps.push(Step::Pause(Duration::from_millis(ms)));
    }
    steps.push(Step::Push(release));
    Ok(steps)
}

/// The steps of text typed in `window`: a line at a time, Enter between
/// (control characters are no text to egui).
pub fn text_steps(window: u32, text: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            steps.extend(key_steps(window, Keycode::Return, Mod::empty(), None)?);
        }
        if !line.is_empty() {
            steps.push(Step::Text(window, line.to_string()));
        }
    }
    Ok(steps)
}

/// The window of a real mouse event (not a synthetic one): it takes the
/// pointer's mark away.
pub fn real_mouse(event: &Event) -> Option<u32> {
    match event {
        Event::MouseMotion { window_id, which, .. } | Event::MouseButtonDown { window_id, which, .. } | Event::MouseButtonUp { window_id, which, .. } | Event::MouseWheel { window_id, which, .. } if *which != SYNTHETIC => Some(*window_id),
        _ => None,
    }
}

/// Puts `text` into SDL's queue as typed in `window`. SDL does not copy
/// the text: it stays in `keep` (the last few, long enough for the loop to
/// have taken them in).
fn push_text(keep: &mut VecDeque<std::ffi::CString>, window: u32, text: &str) -> Result<(), String> {
    use sdl3::sys::events::{SDL_EVENT_TEXT_INPUT, SDL_Event, SDL_PushEvent, SDL_TextInputEvent};
    use sdl3::sys::video::SDL_WindowID;
    // A real keyboard's text reaches only a window that asked SDL for it. The
    // synthetic text must not get further than a person's keys would (it
    // once did: an editor's fields seemed to take typing and took none).
    // Safety: plain calls with the window's id.
    let asked = unsafe {
        let w = sdl3::sys::video::SDL_GetWindowFromID(SDL_WindowID(window));
        !w.is_null() && sdl3::sys::keyboard::SDL_TextInputActive(w)
    };
    if !asked {
        return Err(format!("window {window} takes no typed text: SDL's text input was never started for it, so a real keyboard types nothing there either"));
    }
    keep.push_back(std::ffi::CString::new(text).map_err(|_| "the text has a NUL in it".to_string())?);
    while keep.len() > 64 {
        keep.pop_front();
    }
    let at = keep.back().map_or(std::ptr::null(), |t| t.as_ptr());
    // Safety: the text outlives its event (`keep`).
    unsafe {
        let mut event: SDL_Event = std::mem::zeroed();
        event.text = SDL_TextInputEvent { r#type: SDL_EVENT_TEXT_INPUT, windowID: SDL_WindowID(window), text: at, ..Default::default() };
        if SDL_PushEvent(&mut event) { Ok(()) } else { Err(format!("pushing the text: {}", sdl3::get_error())) }
    }
}

/// A call waiting for its steps.
struct Job {
    call: Call,
    steps: VecDeque<Step>,
    /// What it says when done.
    said: String,
    resume_at: Option<Instant>,
    /// A picture waited for: the window, since when.
    capture: Option<(u32, Instant)>,
}

/// The input another session sends, and what it waits for.
pub struct Remote {
    sender: sdl3::event::EventSender,
    jobs: VecDeque<Job>,
    pointer: Pointer,
    /// The modifier keys held by a synthetic key, for the code that asks
    /// the keyboard's state.
    mods: Mod,
    /// Keys held for the game, till when.
    held: Vec<(Scancode, Instant)>,
    /// Where each window's synthetic pointer is (SDL's points).
    markers: HashMap<u32, (f32, f32)>,
    last_pointer: Option<Instant>,
    /// The texts pushed lately, which SDL's events point into.
    texts: VecDeque<std::ffi::CString>,
}

impl Remote {
    pub fn new(sender: sdl3::event::EventSender) -> Self {
        Self { sender, jobs: VecDeque::new(), pointer: Pointer::default(), mods: Mod::empty(), held: Vec::new(), markers: HashMap::new(), last_pointer: None, texts: VecDeque::new() }
    }

    /// Whether a shift key is held by a synthetic key.
    pub fn shift(&self) -> bool {
        self.mods.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD)
    }

    /// Where the synthetic pointer is in a window, if its last pointer
    /// event was synthetic.
    pub fn marker(&self, window: u32) -> Option<(f32, f32)> {
        self.markers.get(&window).copied()
    }

    /// Takes note of an event coming in: a real mouse's takes the mark away.
    pub fn saw(&mut self, event: &Event) {
        if let Some(window) = real_mouse(event) {
            self.markers.remove(&window);
        }
    }

    /// The keys held for the game now.
    pub fn held(&mut self, now: Instant) -> Vec<Scancode> {
        self.held.retain(|&(_, until)| until > now);
        self.held.iter().map(|&(k, _)| k).collect()
    }

    /// Whether a call is being worked on.
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// The window a picture is waited for of, and whether it is time to
    /// take it as it is.
    pub fn capturing(&self, now: Instant) -> Option<(u32, bool)> {
        self.jobs.front().and_then(|j| j.capture).map(|(id, since)| (id, now.duration_since(since) >= LATE))
    }

    /// When the loop is to look again (None: nothing is waited for).
    pub fn wakes_at(&self, now: Instant) -> Option<Instant> {
        let job = self.jobs.front()?;
        Some(match (job.resume_at, job.capture) {
            (Some(at), _) => at,
            (None, Some(_)) => now + Duration::from_millis(30),
            (None, None) => now,
        })
    }

    /// Takes a call of the tools above: `windows` answered at once, the
    /// others made a job.
    pub fn submit(&mut self, call: Call, windows: &[WindowInfo]) {
        if call.tool == "windows" {
            let list: Vec<Value> = windows.iter().map(|w| json!({ "id": w.id, "title": w.title, "width": w.width, "height": w.height, "keyboard": w.keyboard, "main": w.main })).collect();
            return call.answer(vec![json!({ "type": "text", "text": serde_json::to_string_pretty(&list).unwrap_or_default() })]);
        }
        match self.plan(&call.tool, &call.args, windows) {
            Ok((steps, said)) => self.jobs.push_back(Job { call, steps: steps.into(), said, resume_at: None, capture: None }),
            Err(why) => call.fail(&why),
        }
    }

    /// The window an argument names, else the main one.
    fn window<'a>(args: &Value, windows: &'a [WindowInfo]) -> Result<&'a WindowInfo, String> {
        match args.get("window").and_then(Value::as_u64) {
            Some(id) => windows.iter().find(|w| u64::from(w.id) == id).ok_or_else(|| format!("no window {id} (windows lists them)")),
            None => windows.iter().find(|w| w.main).ok_or_else(|| "the IDE has no main window".to_string()),
        }
    }

    /// A number argument.
    fn number(args: &Value, name: &str) -> Result<f32, String> {
        args.get(name).and_then(Value::as_f64).map(|n| n as f32).ok_or_else(|| format!("{name} is needed: a number"))
    }

    /// A point of the window's picture (pixels) in SDL's points.
    fn point(args: &Value, w: &WindowInfo, x: &str, y: &str) -> Result<(f32, f32), String> {
        let (px, py) = (Self::number(args, x)?, Self::number(args, y)?);
        if px < 0.0 || py < 0.0 || px >= w.width as f32 || py >= w.height as f32 {
            return Err(format!("{x}, {y} lie outside the window's picture ({} by {} pixels)", w.width, w.height));
        }
        Ok((px / w.density, py / w.density))
    }

    /// What a call is made of: its steps and what it says when done.
    fn plan(&mut self, tool: &str, args: &Value, windows: &[WindowInfo]) -> Result<(Vec<Step>, String), String> {
        let w = Self::window(args, windows)?;
        match tool {
            "window_shot" => Ok((vec![Step::Settle, Step::Capture(w.id)], String::new())),
            "pointer" => {
                let action = action_of(args.get("action").and_then(Value::as_str).unwrap_or("click"))?;
                let button = button_of(args.get("button").and_then(Value::as_str).unwrap_or("left"))?;
                let names: Vec<&str> = args.get("modifiers").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                let at = Self::point(args, w, "x", "y")?;
                let to = if action == Action::Drag { Some(Self::point(args, w, "to_x", "to_y")?) } else { None };
                let steps = self.pointer.steps(w.id, action, button, at, to, modifiers_of(&names)?)?;
                let said = format!("{} {} at {},{} in window {}", args.get("action").and_then(Value::as_str).unwrap_or("click"), args.get("button").and_then(Value::as_str).unwrap_or("left"), args["x"], args["y"], w.id);
                Ok((steps, said))
            }
            "scroll" => {
                let at = Self::point(args, w, "x", "y")?;
                let dy = Self::number(args, "dy")?;
                let dx = args.get("dx").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                Ok((self.pointer.scroll(w.id, at, dx, dy), format!("scrolled {dx},{dy} steps at {},{} in window {}", args["x"], args["y"], w.id)))
            }
            "keys" => {
                let hold = args.get("hold_ms").and_then(Value::as_u64).map(|ms| ms.min(MOST_HOLD));
                match (args.get("keys").and_then(Value::as_str), args.get("text").and_then(Value::as_str)) {
                    (Some(chord), None) => {
                        let (key, mods) = parse_chord(chord)?;
                        Ok((key_steps(w.id, key, mods, hold)?, format!("pressed {chord} in window {}", w.id)))
                    }
                    (None, Some(text)) => Ok((text_steps(w.id, text)?, format!("typed {} characters in window {}", text.chars().count(), w.id))),
                    _ => Err("keys needs one of keys (a key or chord, \"ctrl+s\") or text (what to type)".to_string()),
                }
            }
            other => Err(format!("the IDE has no tool {other}")),
        }
    }

    /// Works on the call at the front: its next step, when it is due (one a
    /// frame). `windows`: the IDE's windows now.
    pub fn run(&mut self, now: Instant, windows: &[WindowInfo]) {
        let Some(job) = self.jobs.front_mut() else { return };
        if let Some((id, since)) = job.capture {
            if !windows.iter().any(|w| w.id == id) {
                return self.finish(Err(format!("window {id} is gone")));
            }
            if now.duration_since(since) >= GIVE_UP {
                return self.finish(Err("the window was not drawn (hidden or minimised?)".to_string()));
            }
            return;
        }
        if job.resume_at.is_some_and(|at| now < at) {
            return;
        }
        job.resume_at = None;
        let Some(step) = job.steps.pop_front() else {
            let said = std::mem::take(&mut job.said);
            return self.finish(Ok(vec![json!({ "type": "text", "text": said })]));
        };
        match step {
            Step::Push(events) => {
                for event in events {
                    self.note(&event);
                    if let Err(e) = self.sender.push_event(event) {
                        return self.finish(Err(format!("pushing an event: {e}")));
                    }
                }
            }
            Step::Text(window, text) => {
                if let Err(e) = push_text(&mut self.texts, window, &text) {
                    self.finish(Err(e));
                }
            }
            Step::Hold(codes, ms) => self.held.extend(codes.into_iter().map(|c| (c, now + Duration::from_millis(ms)))),
            Step::Pause(d) => job.resume_at = Some(now + d),
            Step::Settle => job.resume_at = self.last_pointer.map(|t| t + SETTLE).filter(|&t| t > now),
            Step::Capture(id) => job.capture = Some((id, now)),
        }
    }

    /// What an event pushed changes of what the IDE remembers of the
    /// synthetic pointer and keys.
    fn note(&mut self, event: &Event) {
        match event {
            Event::MouseMotion { window_id, x, y, .. } | Event::MouseButtonDown { window_id, x, y, .. } | Event::MouseButtonUp { window_id, x, y, .. } => {
                self.markers.insert(*window_id, (*x, *y));
                self.last_pointer = Some(Instant::now());
            }
            Event::MouseWheel { window_id, mouse_x, mouse_y, .. } => {
                self.markers.insert(*window_id, (*mouse_x, *mouse_y));
                self.last_pointer = Some(Instant::now());
            }
            Event::KeyDown { keymod, .. } | Event::KeyUp { keymod, .. } => self.mods = *keymod,
            _ => {}
        }
    }

    /// The picture of `window` was drawn (PNG bytes), or could not be.
    pub fn captured(&mut self, window: u32, picture: Result<Vec<u8>, String>) {
        if self.jobs.front().and_then(|j| j.capture).is_some_and(|(id, _)| id == window) {
            self.finish(picture.map(|png| vec![json!({ "type": "image", "data": crate::mcp::base64(&png), "mimeType": "image/png" })]));
        }
    }

    /// Answers the call at the front.
    fn finish(&mut self, answer: Result<Vec<Value>, String>) {
        if let Some(job) = self.jobs.pop_front() {
            match answer {
                Ok(content) => job.call.answer(content),
                Err(why) => job.call.fail(&why),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chords are a key and the modifiers held with it, in any case; a
    /// key it does not know is named.
    #[test]
    fn chords_become_keys_and_modifiers() {
        assert_eq!(parse_chord("Escape"), Ok((Keycode::Escape, Mod::empty())));
        assert_eq!(parse_chord("F5"), Ok((Keycode::F5, Mod::empty())));
        assert_eq!(parse_chord("enter"), Ok((Keycode::Return, Mod::empty())));
        assert_eq!(parse_chord("Up"), Ok((Keycode::Up, Mod::empty())));
        assert_eq!(parse_chord("a"), Ok((Keycode::A, Mod::empty())));
        assert_eq!(parse_chord("7"), Ok((Keycode::_7, Mod::empty())));
        assert_eq!(parse_chord("ctrl+s"), Ok((Keycode::S, Mod::LCTRLMOD)));
        assert_eq!(parse_chord("Ctrl+Shift+Z"), Ok((Keycode::Z, Mod::LCTRLMOD | Mod::LSHIFTMOD)));
        assert_eq!(parse_chord("ctrl++"), Ok((Keycode::Plus, Mod::LCTRLMOD)));
        assert_eq!(parse_chord("+"), Ok((Keycode::Plus, Mod::empty())));
        let unknown = parse_chord("ctrl+blorp").unwrap_err();
        assert!(unknown.contains("unknown key \"blorp\""), "{unknown}");
        let modifier = parse_chord("hyper+a").unwrap_err();
        assert!(modifier.contains("no modifier \"hyper\""), "{modifier}");
        assert!(parse_chord("F13").is_err());
    }

    /// Names of actions and buttons; a wrong one says what there is.
    #[test]
    fn actions_and_buttons_by_name() {
        assert_eq!(action_of("double_click"), Ok(Action::DoubleClick));
        assert_eq!(action_of("drag"), Ok(Action::Drag));
        assert!(action_of("tap").unwrap_err().contains("click"));
        assert_eq!(button_of("right"), Ok(MouseButton::Right));
        assert!(button_of("fourth").is_err());
        assert_eq!(modifiers_of(&["shift", "ctrl"]), Ok(Mod::LSHIFTMOD | Mod::LCTRLMOD));
        assert!(modifiers_of(&["super_shift"]).is_err());
    }

    /// A key types its text as a keyboard does (not with Ctrl down).
    #[test]
    fn keys_type_their_text() {
        assert_eq!(typed(Keycode::A, Mod::empty()).as_deref(), Some("a"));
        assert_eq!(typed(Keycode::A, Mod::LSHIFTMOD).as_deref(), Some("A"));
        assert_eq!(typed(Keycode::_5, Mod::empty()).as_deref(), Some("5"));
        assert_eq!(typed(Keycode::Space, Mod::empty()).as_deref(), Some(" "));
        assert_eq!(typed(Keycode::S, Mod::LCTRLMOD), None);
        assert_eq!(typed(Keycode::Return, Mod::empty()), None);
        assert_eq!(typed(Keycode::Escape, Mod::empty()), None);
    }

    /// The events of a step.
    fn events(steps: &[Step]) -> Vec<&Event> {
        steps.iter().flat_map(|s| if let Step::Push(e) = s { e.iter().collect() } else { Vec::new() }).collect()
    }

    /// A click is a move, a press and a release, one frame each; a double
    /// click two presses with their click counts; a drag presses, moves
    /// with the button held and lets go where it ends.
    #[test]
    fn a_click_is_a_move_a_press_and_a_release() {
        let mut pointer = Pointer::default();
        let click = pointer.steps(7, Action::Click, MouseButton::Left, (10.0, 20.0), None, Mod::empty()).unwrap();
        assert_eq!(click.len(), 3, "a frame each");
        assert!(matches!(events(&click)[..], [Event::MouseMotion { x: 10.0, y: 20.0, which: SYNTHETIC, window_id: 7, .. }, Event::MouseButtonDown { mouse_btn: MouseButton::Left, clicks: 1, .. }, Event::MouseButtonUp { mouse_btn: MouseButton::Left, clicks: 1, .. }]));
        let double = pointer.steps(7, Action::DoubleClick, MouseButton::Left, (10.0, 20.0), None, Mod::empty()).unwrap();
        let clicks: Vec<u8> = events(&double).iter().filter_map(|e| if let Event::MouseButtonDown { clicks, .. } = e { Some(*clicks) } else { None }).collect();
        assert_eq!(clicks, [1, 2]);
        let drag = pointer.steps(7, Action::Drag, MouseButton::Right, (0.0, 0.0), Some((80.0, 40.0)), Mod::empty()).unwrap();
        let all = events(&drag);
        assert!(matches!(all[1], Event::MouseButtonDown { mouse_btn: MouseButton::Right, .. }));
        let Event::MouseMotion { mousestate, x, y, xrel, .. } = all[all.len() - 2] else { panic!("a move before the release") };
        assert!(mousestate.right() && !mousestate.left(), "moved with the right button held");
        assert_eq!((*x, *y, *xrel), (80.0, 40.0, 10.0));
        assert!(matches!(all[all.len() - 1], Event::MouseButtonUp { mouse_btn: MouseButton::Right, x: 80.0, y: 40.0, .. }));
        assert!(pointer.steps(7, Action::Drag, MouseButton::Left, (0.0, 0.0), None, Mod::empty()).unwrap_err().contains("to_x"));
        // Released: nothing held.
        let after = pointer.steps(7, Action::Move, MouseButton::Left, (1.0, 1.0), None, Mod::empty()).unwrap();
        assert!(matches!(events(&after)[0], Event::MouseMotion { mousestate, .. } if mousestate.to_sdl_state() == 0));
    }

    /// Modifiers are pressed before the pointer's events and let go after.
    #[test]
    fn modifiers_wrap_a_click() {
        let mut pointer = Pointer::default();
        let steps = pointer.steps(3, Action::Click, MouseButton::Left, (1.0, 1.0), None, Mod::LSHIFTMOD | Mod::LCTRLMOD).unwrap();
        let all = events(&steps);
        assert!(matches!(all[0], Event::KeyDown { keycode: Some(Keycode::LShift), .. }));
        assert!(matches!(all[1], Event::KeyDown { keycode: Some(Keycode::LCtrl), keymod, .. } if keymod.contains(Mod::LSHIFTMOD | Mod::LCTRLMOD)));
        let n = all.len();
        assert!(matches!(all[n - 2], Event::KeyUp { keycode: Some(Keycode::LCtrl), .. }));
        assert!(matches!(all[n - 1], Event::KeyUp { keycode: Some(Keycode::LShift), keymod, .. } if keymod.is_empty()));
    }

    /// A chord presses its modifiers and the key (the text it types after),
    /// and lets go in the other order; held for the game it names the keys.
    #[test]
    fn a_chord_presses_and_releases() {
        let steps = key_steps(2, Keycode::S, Mod::LCTRLMOD, Some(50)).unwrap();
        assert!(matches!(&steps[0], Step::Push(e) if matches!(e[..], [Event::KeyDown { keycode: Some(Keycode::LCtrl), .. }, Event::KeyDown { keycode: Some(Keycode::S), .. }])));
        assert!(steps.iter().any(|s| matches!(s, Step::Hold(codes, 50) if codes.len() == 2)));
        assert!(!steps.iter().any(|s| matches!(s, Step::Text(..))), "Ctrl+S types nothing");
        assert!(matches!(steps.last(), Some(Step::Push(e)) if matches!(e[..], [Event::KeyUp { keycode: Some(Keycode::S), .. }, Event::KeyUp { keycode: Some(Keycode::LCtrl), .. }])));
        let plain = key_steps(2, Keycode::A, Mod::empty(), None).unwrap();
        assert!(plain.iter().any(|s| matches!(s, Step::Text(2, t) if t == "a")));
        // Text with a line break: its lines and an Enter between.
        let text = text_steps(2, "ab\ncd").unwrap();
        assert_eq!(text.iter().filter(|s| matches!(s, Step::Text(..))).count(), 2);
        assert!(events(&text).iter().any(|e| matches!(e, Event::KeyDown { keycode: Some(Keycode::Return), .. })));
    }

    /// Only a real mouse's events take the pointer's mark away.
    #[test]
    fn a_real_mouse_is_told_from_a_synthetic_one() {
        let motion = |which| Event::MouseMotion { timestamp: 0, window_id: 4, which, mousestate: MouseState::from_sdl_state(0), x: 0.0, y: 0.0, xrel: 0.0, yrel: 0.0 };
        assert_eq!(real_mouse(&motion(1)), Some(4));
        assert_eq!(real_mouse(&motion(SYNTHETIC)), None);
        assert_eq!(real_mouse(&Event::Quit { timestamp: 0 }), None);
    }
}
