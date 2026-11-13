//! The IDE's window as egui lays it out: the game's name with PLAY and
//! BUILD along the top, its logic and assets on the left, and in the main
//! area the one thing open (no tabs). Calm on purpose: a dark ground, big
//! readable letters, few words.
//!
//! Here the shell's state and its frame; its parts in `shell/`: `bar` (the
//! top), `list` (the left, `new_files` what a plus makes, `manage` RENAME
//! and DELETE), `main_area` (the file open), `sound_ui` (sounds, songs, the
//! maker, USED), `finding` (find and search), `game` (PLAY), `errors` (the
//! red line, OUTPUT's lines), `below` (the console, OUTPUT), `agent` (the
//! agent and the settings), `view_state` (what the agent sees); `tests`.

use std::path::{Path, PathBuf};

use egui::{RichText, TextStyle};

use crate::gui::{ACCENT, ERROR, GROUND, INSET, PANEL, QUIET, TEXT, arrow};
use super::output::{Kind, Output};
use super::prefs::{Agent, Prefs};
use super::projects::{self, Form, NewGame, Wish};

use super::buffer::Buffer;
use super::console::Console;
use super::files::Files;
use super::memory::Memory;
use super::preview::Preview;
use super::maker;
use super::sounds;
use super::terminal;
mod agent;
mod bar;
mod below;
mod errors;
mod finding;
mod game;
mod list;
mod main_area;
mod manage;
mod new_files;
mod sound_ui;
mod view_state;
use manage::{Act, Gone, Question, Renaming};
use agent::*;
use errors::*;
use new_files::*;
use sound_ui::*;
pub use main_area::is_model_file;
pub(super) use main_area::{level_line, model_line};
use crate::audio::sound::{self, SoundDef};

/// What the main area shows of the open file.
enum Shown {
    /// A script or a text it reads, a sound or a song, to change.
    Code(Box<Buffer>),
    /// Any other text, to read.
    Text(String),
    Image(egui::TextureHandle),
    /// No picture of it yet: a line saying what it is.
    Note(String),
}

/// The window's state between frames.
pub struct Shell {
    title: String,
    dir: PathBuf,
    files: Files,
    /// The folder's file names as last looked at, and when to look next
    /// (once a second: a file the agent makes comes into the list by
    /// itself).
    listing: Vec<String>,
    list_at: std::time::Instant,
    memory: Memory,
    memory_path: PathBuf,
    /// The open file as shown (loaded on the first frame it is open), and
    /// what it is when an editor opens it.
    shown: Option<Shown>,
    /// A picture from outside the game folder shown instead of the open
    /// file (one the agent made, clicked in the console): to look at, never
    /// remembered as open.
    outside: Option<PathBuf>,
    facts: Option<String>,
    /// How far down to scroll the file just opened, once.
    restore: Option<f32>,
    console: Console,
    /// The game's level of blocks (its file, `c.world.blocks.file`), which
    /// the level editor opens; with levels in `levels/`, each of those too.
    level: Option<String>,
    /// Files to open in an editor's window, asked for since last taken.
    edits: Vec<String>,
    /// What went wrong opening one, to show.
    message: Option<String>,
    /// A name being typed for a new file (a plus at a heading).
    naming: Option<Naming>,
    /// A name being typed for a file of the list (RENAME), the question
    /// about the lines that name it, the file just deleted (UNDO), the file
    /// open before this one, and the folder a file goes to when the system's
    /// trash will not take it.
    renaming: Option<Renaming>,
    asking: Option<Question>,
    gone: Option<Gone>,
    previous: Option<String>,
    /// The open file was in the folder when it was opened (else it never
    /// was: not said to be gone).
    opened_there: bool,
    deleted_dir: PathBuf,
    /// Where UNDO was drawn (for the tests' mouse).
    #[cfg(test)]
    undo_button: Option<egui::Rect>,
    /// Where the pluses and their headings were drawn (for the tests' mouse).
    #[cfg(test)]
    headings: Vec<(String, egui::Rect, egui::Rect)>,
    /// The game's settings (the light and camera previews use).
    cfg: crate::config::GameConfig,
    /// A model's picture lies on the voxel editor's BACKGROUND as the
    /// person chose it (else, and in a shot or a test, on a mid gray).
    pub own_background: bool,
    /// The picture of the model, block or level open, and the one before
    /// (the new one takes over its renderer).
    preview: Option<Preview>,
    /// A model shown as its text (TEXT), not as its picture.
    model_text: bool,
    spare: Option<Preview>,
    /// Pictures gone, their textures to free.
    retired: Vec<egui::TextureId>,
    /// When to look next whether the open file changed on disk.
    look_at: std::time::Instant,
    /// Pictures play (a model's frames); a shot holds them still.
    pub animate: bool,
    /// The game being played (the IDE runs it, `play.rs`), and what the
    /// window asks of it.
    pub stage: Option<Stage>,
    asks: Vec<Ask>,
    /// The line of the open file to go to, once, and whether the text
    /// takes the keys there (the user asked, not the agent).
    goto: Option<(usize, bool)>,
    /// The text open takes the keys on its next frame (a file chosen in
    /// the list, back from the game).
    focus_code: bool,
    /// What the last BUILD made (or why not), what F9 did; until when
    /// (None: until the next).
    notice: Option<Notice>,
    notice_until: Option<std::time::Instant>,
    /// BUILD at work on a thread of its own: its answer, and where the zip
    /// goes.
    building: Option<(std::sync::mpsc::Receiver<anyhow::Result<super::export::Packed>>, PathBuf)>,
    /// Why the game would not start (PLAY found no game to begin), until it
    /// starts or the files are mended (`start_mended`).
    start_failed: Option<String>,
    /// The song playing (its file), started with PLAY SONG over it.
    pub listening: Option<String>,
    /// The IDE's settings (the gear at the top right) and where they are
    /// kept (None: nowhere, a test's or a shot's).
    prefs: Prefs,
    prefs_path: Option<PathBuf>,
    /// NEW's form while it is open.
    new_game: Option<NewGame>,
    /// The game to open once the user agrees to end what runs in the
    /// console.
    leaving: Option<PathBuf>,
    /// HELP shown in the main area, and its topic (kept while it is away;
    /// 0 the reference, as HELP opens).
    pub help: bool,
    pub help_topic: usize,
    /// OUTPUT, and whether it is shown below instead of the console; the
    /// game's trouble already in it, and when it was last written down.
    output: Output,
    /// The line under OUTPUT while the game plays: Lua run in it.
    lua: crate::prompt::Prompt,
    output_shown: bool,
    /// The console's height as last drawn, and whether it changed since the
    /// mouse went down on it (to be kept when the mouse is let go).
    console_seen: Option<f32>,
    console_dragged: bool,
    reported: Option<String>,
    output_saved: std::time::Instant,
    /// The sounds the game's code brings (a kit's, the engine's), which it
    /// has no file for until MAKE IT YOURS; the one of them shown in the
    /// main area, by name.
    brought: Vec<(&'static str, SoundDef)>,
    builtin: Option<&'static str>,
    /// A sound open: its line of words (with the text it was made from),
    /// and where the game's code names it (looked for again now and then).
    sound_line: Option<(String, String)>,
    used: Option<Used>,
    /// What PLAY SOUND asked the speaker to play, and how loud.
    heard: Option<(std::sync::Arc<[f32]>, f32)>,
    /// The sound maker over a sound open: how many rolls so far (a part of
    /// the next one's seed), and the fader held, if one is.
    rolls: u64,
    /// LOOP: the sound open plays again and again, a short pause between,
    /// until switched off (or another file is opened, or the game played);
    /// when it is due next.
    looping: bool,
    again_at: Option<std::time::Instant>,
    fading: Option<maker::Fader>,
    /// Where the agent's program is looked for, and whether it was not
    /// found when last looked (the console says so, instead of the shell).
    finder: super::claude::Finder,
    no_agent: bool,
    /// Where the rows of USED were drawn (for the tests' mouse).
    #[cfg(test)]
    used_rows: Vec<egui::Rect>,
    /// The find row over the open text (Ctrl+F, Ctrl+H), and the search
    /// across the game in the list's place (Ctrl+Shift+F, SEARCH).
    pub find: super::find::Find,
    pub search: super::find::Search,
}

/// Where the game's code names the sound open.
struct Used {
    name: String,
    rows: Vec<super::refs::Ref>,
    at: std::time::Instant,
}

/// How long the places a sound is used are kept before looking again.
const USED_FOR: std::time::Duration = std::time::Duration::from_secs(1);

/// What the bar along the top says beside the buttons.
enum Notice {
    /// A sentence, quiet (what F9 did, a rename).
    Said(String),
    /// BUILD at work.
    Building,
    /// BUILD made the zip at the path (the sentence says where, and how big).
    Built(PathBuf, String),
    /// BUILD could not, and why.
    Failed(String),
}

/// The game being played, as the main area shows it.
#[derive(Default)]
pub struct Stage {
    /// The main area shows it (else a file, and it holds still).
    pub shown: bool,
    /// Its picture as last drawn, and the room (pixels) it has.
    pub picture: Option<egui::TextureId>,
    pub wanted: Option<[u32; 2]>,
    /// What stops it (a script's error).
    pub trouble: Option<String>,
    /// A file of the game that would not load when it changed (it plays on
    /// as it was), until it loads again.
    pub unloaded: Option<String>,
    /// The keys of the game as seat 1 plays it (`Play::keys`): what each
    /// does and the key as the keyboard names it, shown under the game.
    pub keys: Vec<(String, String)>,
}

/// What the window asks of the game, and of the speaker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The game played, begun from a scenario if one is named.
    Play(Option<String>),
    Stop,
    /// The game begun anew from the start (F5), from its scenario if it
    /// was played from one.
    Again,
    /// A line of Lua run in the game being played (the line under OUTPUT).
    Eval(String),
    /// A song (its file) to play, and silence again.
    Listen(String),
    Hush,
    /// The sound PLAY SOUND made ready (`Shell::heard`) to play once.
    Sound,
    /// Another game, or the system's dialog to pick a folder (the menu on
    /// the game's name, the settings).
    Project(Wish),
}

/// The game's picture as a widget: it has the keys while focused.
pub fn game_id() -> egui::Id {
    egui::Id::unique("game")
}

/// How often the open file is compared with the disk (the agent may
/// change it).
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// How often the folder's file names are compared with the list (names
/// only, no file read; polled, as `watch.rs` does, so no inotify watch is
/// used).
const LIST_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

impl Shell {
    /// The IDE on the game in `dir` (its settings `cfg`), remembering in
    /// `memory_path`, with `console`; what was open last is open again.
    pub fn new(dir: &Path, cfg: crate::config::GameConfig, memory_path: PathBuf, console: Console) -> Self {
        let (title, level) = (cfg.title.clone(), cfg.world.blocks.as_ref().map(|b| b.file.clone()));
        let listing = Files::names(dir);
        let files = Files::from_names(dir, listing.clone());
        // A game never opened here has its console folded until it is wanted;
        // what the user does with it is remembered from then on.
        let first_time = !memory_path.exists();
        let mut memory = Memory::load(&memory_path);
        if first_time {
            memory.console_folded = true;
        }
        if memory.open.as_ref().is_none_or(|f| !files.has(f)) {
            memory.open = files.logic.iter().find(|f| *f == "main.lua").or(files.logic.first()).cloned();
        }
        let restore = memory.open.as_ref().and_then(|f| memory.scroll.get(f).copied());
        Self { title, dir: dir.to_path_buf(), files, listing, list_at: std::time::Instant::now() + LIST_EVERY, memory, memory_path, shown: None, outside: None, facts: None, restore, console, level, edits: Vec::new(), message: None, naming: None, renaming: None, asking: None, gone: None, previous: None, opened_there: true, #[cfg(test)] undo_button: None, deleted_dir: crate::settings::config_dir().join("ide").join("deleted").join(crate::settings::game_name(dir)), #[cfg(test)] headings: Vec::new(), cfg, own_background: cfg!(not(test)), preview: None, model_text: false, spare: None, retired: Vec::new(), look_at: std::time::Instant::now(), animate: true, stage: None, asks: Vec::new(), goto: None, focus_code: false, notice: None, notice_until: None, building: None, start_failed: None, listening: None, prefs: Prefs::default(), prefs_path: None, new_game: None, leaving: None, help: false, help_topic: 0, output: Output::open(None), lua: crate::prompt::Prompt::default(), output_shown: false, console_seen: None, console_dragged: false, reported: None, output_saved: std::time::Instant::now(), brought: Vec::new(), builtin: None, sound_line: None, used: None, heard: None, rolls: 0, looping: false, again_at: None, fading: None, #[cfg(test)] finder: super::claude::Finder::in_dirs(Vec::new()), #[cfg(not(test))] finder: super::claude::Finder::system(), no_agent: false, #[cfg(test)] used_rows: Vec::new(), find: Default::default(), search: Default::default() }
    }

    /// The sounds the game's code brings (`Game::sounds`), listed under
    /// SOUNDS for the ones the game has no file for.
    pub fn with_sounds(mut self, brought: Vec<(&'static str, SoundDef)>) -> Self {
        self.brought = brought;
        self
    }

    /// OUTPUT's lines kept at `path` (`Output::path_for`).
    pub fn with_output(mut self, path: PathBuf) -> Self {
        self.output = Output::open(Some(path));
        self
    }

    /// Adds a line to OUTPUT.
    fn report(&mut self, kind: Kind, text: impl Into<String>, at: Option<(String, usize)>) {
        let shown = self.output_shown && self.prefs.console && !self.memory.console_folded;
        self.output.add(kind, text, at, shown);
    }

    /// Shows `path` (in the game folder) in the main area.
    pub fn open(&mut self, path: &str) {
        if self.outside.take().is_some() || self.builtin.take().is_some() {
            self.unload();
        }
        if self.memory.open.as_deref() == Some(path) {
            return;
        }
        self.flush();
        self.hush();
        self.previous = self.memory.open.clone();
        self.memory.open = Some(path.to_string());
        self.opened_there = self.files.has(path);
        self.model_text = false;
        self.unload();
        self.restore = Some(self.memory.scroll.get(path).copied().unwrap_or(0.0));
        let _ = self.save();
    }

    /// Back in the IDE: the open file afresh (an editor may have changed
    /// it). A text being changed stays (its changes, its undo): only what
    /// changed on disk comes in.
    pub fn refresh(&mut self) {
        match &mut self.shown {
            Some(Shown::Code(buffer)) => {
                buffer.look();
            }
            _ => self.unload(),
        }
    }

    /// Lets go of how the open file looked: it is loaded anew on the next
    /// frame (its picture's renderer kept for the next one).
    fn unload(&mut self) {
        self.shown = None;
        self.message = None;
        self.sound_line = None;
        self.used = None;
        if let Some(p) = self.preview.take() {
            self.spare = Some(p);
        }
    }

    /// Saves the text being changed now, if it has changes (on leaving it,
    /// before Play, when the IDE closes). A text whose file changed outside
    /// too is not saved while the question waits: it is kept in its
    /// journal and asked about again when it opens, and the bar says so.
    pub fn flush(&mut self) {
        let Some(Shown::Code(buffer)) = &mut self.shown else { return };
        match buffer.save() {
            Err(e) => self.message = Some(format!("{e:#}")),
            Ok(()) if buffer.theirs.is_some() => {
                let path = self.memory.open.clone().unwrap_or_default();
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                self.note(format!("{name} NOT SAVED: CHANGED OUTSIDE TOO · YOUR TEXT IS KEPT, ASKED AGAIN WHEN IT OPENS"));
            }
            Ok(()) => {}
        }
    }

    /// Saves the text being changed when it is due, and looks whether the
    /// file changed on disk: whether there is something new to show, and
    /// how long until the next look.
    pub fn tick(&mut self, now: std::time::Instant) -> (bool, std::time::Duration) {
        // OUTPUT written down now and then (a script may print every tick).
        if now.duration_since(self.output_saved) >= std::time::Duration::from_secs(3) {
            let _ = self.output.save();
            self.output_saved = now;
        }
        // BUILD's answer, looked for while it works (a tenth of a second).
        let mut built = false;
        let mut wait = None;
        if let Some((receive, out)) = self.building.take() {
            match receive.try_recv() {
                Ok(made) => {
                    self.built(made, out);
                    built = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.building = Some((receive, out));
                    wait = Some(std::time::Duration::from_millis(100));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.built(Err(anyhow::anyhow!("the packing stopped unexpectedly")), out);
                    built = true;
                }
            }
        }
        // LOOP: the sound again once it has sounded and the pause is over.
        if self.looping && self.again_at.is_none_or(|at| now >= at) {
            self.again_at = None;
            self.play_sound();
            // (A sound that would not play ends the loop: nothing to wait for.)
            self.looping = self.again_at.is_some();
        }
        let again = self.again_at.filter(|_| self.looping).map(|at| at.saturating_duration_since(now));
        // The folder's file names: a file made, removed or renamed from
        // outside comes into the list (or goes from it).
        let mut changed = built;
        if now >= self.list_at {
            changed |= self.look_at_folder();
            self.list_at = now + LIST_EVERY;
        }
        let to_list = self.list_at.saturating_duration_since(now).min(wait.unwrap_or(LIST_EVERY));
        let Some(Shown::Code(buffer)) = &mut self.shown else { return (changed, again.unwrap_or(std::time::Duration::from_secs(3600)).min(to_list)) };
        if now >= self.look_at {
            changed |= buffer.look();
            self.look_at = now + LOOK_EVERY;
        }
        match buffer.tick(now) {
            Ok(saved) => changed |= saved,
            Err(e) => {
                self.message = Some(format!("{e:#}"));
                changed = true;
            }
        }
        let next = buffer.due(now).unwrap_or(LOOK_EVERY).min(self.look_at.saturating_duration_since(now)).min(again.unwrap_or(LOOK_EVERY)).min(to_list);
        (changed, next)
    }

    /// Reads the list anew (the IDE itself made, renamed or deleted a file).
    fn rescan(&mut self) {
        self.listing = Files::names(&self.dir);
        self.files = Files::from_names(&self.dir, self.listing.clone());
    }

    /// Compares the folder's file names with the list, and reads the list
    /// anew if they differ; whether it did (a frame is due then). What the
    /// list keeps of itself (the headings folded, the scroll, a name being
    /// typed) is not touched; a file open that is gone says so in the main
    /// area (`main`), its text kept.
    fn look_at_folder(&mut self) -> bool {
        let names = Files::names(&self.dir);
        if names == self.listing {
            return false;
        }
        self.files = Files::from_names(&self.dir, names.clone());
        self.listing = names;
        // A name being typed for a file that is gone has no row to stand in.
        if self.renaming.as_ref().is_some_and(|r| !self.files.has(&r.path)) {
            self.renaming = None;
        }
        // The text open learns at once that its file is gone (or back), so
        // that its autosave does not make the file again.
        if let Some(Shown::Code(buffer)) = &mut self.shown {
            buffer.look();
        }
        true
    }

    /// Lets go of the pictures it drew (the game is left).
    pub fn release(&mut self, egui: &mut egui_wgpu::Renderer) {
        self.unload();
        let gone = self.spare.take().into_iter().chain(self.preview.take()).filter_map(|p| p.texture());
        for id in self.retired.drain(..).chain(gone) {
            egui.free_texture(&id);
        }
    }

    /// Draws the open file's picture if it changed: true if it did.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, egui: &mut egui_wgpu::Renderer) -> bool {
        for id in self.retired.drain(..) {
            egui.free_texture(&id);
        }
        self.preview.as_mut().is_some_and(|p| p.render(device, queue, egui))
    }

    /// Whether the game folder has the file `path`.
    pub fn has(&self, path: &str) -> bool {
        self.files.has(path)
    }

    /// Shows the file `path` (the game held still), at `line` if given
    /// (the agent's `open`: the keys stay where they are, in the console
    /// most likely).
    pub fn go_to(&mut self, path: &str, line: Option<usize>) {
        self.hide_game();
        self.open(path);
        self.goto = line.map(|l| (l, false));
    }

    /// A path clicked in the console (resolved by it: an existing file,
    /// inside the game folder or a picture): a file of the game opens as a
    /// click in the list opens it, at `line` if given; a picture from
    /// elsewhere is shown to look at, the game's files untouched; what
    /// else lies in the game folder is not opened.
    pub fn open_from_console(&mut self, file: &Path, line: Option<usize>) {
        let inside = self.dir.canonicalize().ok().and_then(|root| file.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/")));
        self.help = false;
        match inside {
            Some(path) if self.files.has(&path) => self.go_to(&path, line),
            _ if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) => {
                self.flush();
                self.hush();
                self.hide_game();
                self.outside = Some(file.to_path_buf());
                self.unload();
                self.restore = None;
            }
            _ => {}
        }
    }

    /// What the window asked of the game since last asked.
    pub fn take_asks(&mut self) -> Vec<Ask> {
        std::mem::take(&mut self.asks)
    }

    /// The model open shown as its text (TEXT).
    pub fn model_as_text(&mut self) {
        self.model_text = true;
        self.unload();
    }

    /// Whether an editor opens `path` (in the game folder): the levels of
    /// blocks, their blocks, the models (Voxcade's own: a MagicaVoxel file
    /// is brought in first).
    pub fn editable(&self, path: &str) -> bool {
        self.is_level(path) || (path.ends_with(".vvx") && !self.is_scene(path))
    }

    /// Whether `path` is a level of blocks the level editor opens: the one
    /// the config names, and in a game of blocks each in `levels/`.
    pub fn is_level(&self, path: &str) -> bool {
        let in_levels = path.starts_with("levels/") && path.ends_with(".toml");
        // (In a game of worlds a level of pieces says what it is.)
        self.level.as_deref() == Some(path) || (in_levels && (self.level.is_some() || crate::levels::says_level(&crate::files::Files::disk(), &self.dir.join(path))))
    }

    /// Whether `path` is a map (`map.toml`): no editor, its text to change.
    pub fn is_map(&self, path: &str) -> bool {
        self.files.level_text(path) && !self.is_level(path)
    }

    /// Whether `path` is a world or a map's prefab (`crate::scene`): shown,
    /// never shaped as a model, for its palette's order is its colours'
    /// numbers.
    pub fn is_scene(&self, path: &str) -> bool {
        path.ends_with(".vvx") && crate::levels::scene_files(&self.dir, &self.cfg).iter().any(|f| f == path)
    }

    /// The level the config names, where a block opens when no level's
    /// editor is open.
    pub fn first_level(&self) -> Option<&str> {
        self.level.as_deref()
    }

    /// Asks for `path` to be opened in an editor (a model a level's editor
    /// wants shaped).
    pub fn ask_edit(&mut self, path: String) {
        if self.editable(&path) {
            self.edits.push(path);
        }
    }

    /// The files asked to be opened in an editor since last asked.
    pub fn take_edits(&mut self) -> Vec<String> {
        std::mem::take(&mut self.edits)
    }

    /// Says `text` in the bar along the top for a few seconds (what F9
    /// did).
    pub fn note(&mut self, text: String) {
        self.report(Kind::Note, text.clone(), None);
        self.notice = Some(Notice::Said(text));
        self.notice_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
    }

    /// Shows what went wrong opening an editor.
    pub fn tell(&mut self, message: String) {
        self.report(Kind::Error, message.clone(), None);
        self.message = Some(message);
    }

    /// Unfolds the console (a shot of one that runs a command), and shows
    /// the console and not OUTPUT.
    pub fn unfold_console(&mut self) {
        self.memory.console_folded = false;
        self.output_shown = false;
    }

    /// The console.
    pub fn console(&self) -> &Console {
        &self.console
    }

    pub fn console_mut(&mut self) -> &mut Console {
        &mut self.console
    }

    /// What a model's picture lies on: the voxel editor's BACKGROUND, else
    /// a mid gray (the game's night sky hid dark figures).
    fn model_background(&self) -> [u8; 3] {
        use crate::editor::background::{Backgrounds, PRESETS};
        let chosen = if self.own_background { Backgrounds::load(&Backgrounds::path()).colours()[1] } else { None };
        chosen.unwrap_or(PRESETS[1])
    }

    /// What the main area shows, in words for a shot: HELP, the game, the
    /// open file (and whether it is not in the game), or nothing.
    pub fn showing(&self) -> String {
        if self.help {
            return "HELP".into();
        }
        if self.stage.as_ref().is_some_and(|s| s.shown) {
            return "the game".into();
        }
        match self.opened() {
            Some(path) if !self.files.has(path) => format!("{path} (no such file in the game)"),
            Some(path) => path.to_string(),
            None => "nothing".into(),
        }
    }

    /// The file open in the main area.
    pub fn opened(&self) -> Option<&str> {
        self.memory.open.as_deref()
    }

    /// Writes down what the IDE remembers.
    pub fn save(&mut self) -> anyhow::Result<()> {
        self.output.save()?;
        self.memory.save(&self.memory_path)
    }

    /// A frame of the window.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // Ctrl+S saves the text at once (it is saved by itself a second after
        // the last change anyway); with the game shown the keys are its.
        if !self.stage.as_ref().is_some_and(|s| s.shown) && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.flush();
        }
        self.find_keys(ui);
        self.bar(ui);
        self.list(ui);
        self.below(ui);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(GROUND).inner_margin(egui::Margin { left: 20, right: 0, top: 14, bottom: 0 })).show(ui, |ui| self.main(ui));
        self.question_ui(ui);
        // NEW's form over everything; the game begun opens.
        if let Some(new) = &mut self.new_game {
            match new.ui(ui.ctx(), &self.prefs.projects_dir(), &self.prefs) {
                Form::Open => {}
                Form::Cancelled => self.new_game = None,
                Form::Created(dir) => {
                    self.new_game = None;
                    self.leave_for(dir);
                }
            }
        }
        self.leaving_ui(ui);
    }
}

#[cfg(test)]
mod tests;
