//! The IDE's window as egui lays it out: the game's name with PLAY and
//! BUILD along the top, its logic and assets on the left, and in the main
//! area the one thing open (no tabs). Calm on purpose: a dark ground, big
//! readable letters, few words.

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
mod manage;
use manage::{Act, Gone, Question, Renaming};
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
    deleted_dir: PathBuf,
    /// Where UNDO was drawn (for the tests' mouse).
    #[cfg(test)]
    undo_button: Option<egui::Rect>,
    /// Where the pluses and their headings were drawn (for the tests' mouse).
    #[cfg(test)]
    headings: Vec<(String, egui::Rect, egui::Rect)>,
    /// The game's settings (the light and camera previews use).
    cfg: crate::config::GameConfig,
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
}

/// Where the game's code names the sound open.
struct Used {
    name: String,
    rows: Vec<sounds::Use>,
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

/// A new scenario's file (the plus at SCENARIOS), to fill in.
const NEW_SCENARIO: &str = "\
-- A scenario: a run begun late, to try what happens there without
-- playing to it first (docs/game-model.md, \"Scenarios and the console\").
-- PLAY SCENARIO above plays the game from it; voxcade shot, inspect and
-- run take --scenario with its file's name.
return {
  about = \"What it sets up, in a few words\",
  -- level = \"name\", place = \"name\",   -- where the run begins, if not the first level
  setup = function()
    -- The states as late in the game, with the game's own functions:
    -- player.state.score = 5000
  end,
}
";

/// A new Lua file (the plus at LOGIC): `NAME` is the file's name, `IDENT` the name as a Lua variable.
const NEW_LUA: &str = "\
-- NAME.lua: a file of the game's code. main.lua takes it in while the game
-- starts, at its top (not inside a handler):
--   local IDENT = require(\"NAME\")
-- and gets back what this file returns (true if it returns nothing). The
-- files share main.lua's globals; saving one runs the game's code anew.
local IDENT = {}

function IDENT.hello()
  return \"hello from NAME\"
end

return IDENT
";

/// A new song (the plus at MUSIC): four bars over and over, a tune and a bass.
const NEW_SONG: &str = "\
; NAME: a short song to change. The format is described at the top of
; src/audio/music.rs: notes c d e f g a b (+ sharp, - flat), a length
; (4 quarter, 8 eighth), o4 the octave, l8 the default length, v12 the
; volume, | only marks a bar. A game plays it with c.audio.music or
; music:play(\"music/NAME.mml\"); saving it plays it anew.

#title NAME
#tempo 110

@lead triangle attack=0.02 decay=0.3 sustain=0.6 release=0.2 vibrato=0.006 vibrato_speed=5 volume=0.5
@bass triangle attack=0.01 decay=0.3 sustain=0.7 release=0.2 volume=0.6

A: @lead v12 q7 o4 l8 c e g e c e g e | a g f e d4 d4 | c e g e c e g e | f e d c g2
B: @bass v12 q7 o2 l4 c c c c | f f c c | c c c c | f f g g
";

/// A new sound (the plus at SOUNDS): a short blip to change, with the one
/// line saying what the file is.
fn new_sound(name: &str) -> String {
    let blip = SoundDef::Synth(crate::audio::Synth { freq: 660.0, hold: 0.04, decay: 0.12, volume: 0.3, ..Default::default() });
    sound::write(&blip, &[format!("{name}: a sound, a few numbers a line; level:sound(\"{name}\", player) plays it.")])
}

/// A name being typed for a new file under a heading (a plus).
struct Naming {
    sort: String,
    text: String,
    /// Why the last Enter made nothing.
    problem: Option<String>,
    /// Its field is to take the keys, once.
    fresh: bool,
}

/// What a plus at a heading makes, and its hover text (`None`: no plus there).
fn new_what(sort: &str) -> Option<&'static str> {
    Some(match sort {
        "Logic" => "New Lua file",
        "Scenarios" => "New scenario",
        "Models" => "New model",
        "Sounds" => "New sound",
        "Music" => "New song",
        _ => return None,
    })
}

/// The file a name typed under the heading `sort` stands for, and its text
/// (a model's bytes are made by the editor's writer, not here): the name
/// lower case, spaces `_`, `.lua` and the like left off if typed; letters,
/// digits, `_` and `-` only. A plain sentence says why not.
fn new_file(dir: &Path, sort: &str, typed: &str) -> Result<String, String> {
    let (folder, ext) = match sort {
        "Logic" => ("", "lua"),
        "Scenarios" => ("scenarios/", "lua"),
        "Models" => ("", "vvx"),
        "Sounds" => ("sounds/", "toml"),
        "Music" => ("music/", "mml"),
        _ => return Err("Nothing is made here.".into()),
    };
    let name = typed.trim().to_lowercase().replace(' ', "_");
    let name = name.strip_suffix(&format!(".{ext}")).unwrap_or(&name);
    if name.is_empty() {
        return Err("Type a name first.".into());
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err("A name takes letters, digits, _ and - only.".into());
    }
    let path = format!("{folder}{name}.{ext}");
    if dir.join(&path).exists() {
        return Err(format!("{path} is already there."));
    }
    Ok(path)
}

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
        Self { title, dir: dir.to_path_buf(), files, listing, list_at: std::time::Instant::now() + LIST_EVERY, memory, memory_path, shown: None, outside: None, facts: None, restore, console, level, edits: Vec::new(), message: None, naming: None, renaming: None, asking: None, gone: None, previous: None, #[cfg(test)] undo_button: None, deleted_dir: crate::settings::config_dir().join("ide").join("deleted").join(crate::settings::game_name(dir)), #[cfg(test)] headings: Vec::new(), cfg, preview: None, model_text: false, spare: None, retired: Vec::new(), look_at: std::time::Instant::now(), animate: true, stage: None, asks: Vec::new(), goto: None, focus_code: false, notice: None, notice_until: None, building: None, start_failed: None, listening: None, prefs: Prefs::default(), prefs_path: None, new_game: None, leaving: None, help: false, help_topic: 0, output: Output::open(None), lua: crate::prompt::Prompt::default(), output_shown: false, console_seen: None, console_dragged: false, reported: None, output_saved: std::time::Instant::now(), brought: Vec::new(), builtin: None, sound_line: None, used: None, heard: None, rolls: 0, looping: false, again_at: None, fading: None, #[cfg(test)] finder: super::claude::Finder::in_dirs(Vec::new()), #[cfg(not(test))] finder: super::claude::Finder::system(), no_agent: false, #[cfg(test)] used_rows: Vec::new() }
    }

    /// The sounds the game's code brings (`Game::sounds`), listed under
    /// SOUNDS for the ones the game has no file for.
    pub fn with_sounds(mut self, brought: Vec<(&'static str, SoundDef)>) -> Self {
        self.brought = brought;
        self
    }

    /// The IDE's settings kept at `path` (`Prefs::path`); the agent
    /// started at once if it starts with the IDE (and there is a console).
    pub fn with_prefs(mut self, path: PathBuf) -> Self {
        match Prefs::load(&path) {
            Ok(prefs) => (self.prefs, self.prefs_path) = (prefs, Some(path)),
            // Not written over: the defaults for now, kept nowhere.
            Err(e) => self.console.tell(Some(format!("the settings: {e:#}"))),
        }
        // Opened now: first among the games opened last.
        self.prefs.opened(&self.dir, &self.title);
        if let Err(e) = self.keep_prefs() {
            self.console.tell(Some(format!("the settings: {e:#}")));
        }
        if self.prefs.console && self.prefs.claude.with_ide {
            self.start_agent(false, false);
        }
        self
    }

    /// Starts the agent in the console as the settings say (`fresh`: a new
    /// conversation, the console started anew for it; `asked`: someone
    /// pressed a button for it). Its program is looked for first: if it is
    /// not there nothing is typed, the console says what an agent is and
    /// where to get one (`agent_missing`), and the console opens for it
    /// only if it was asked for (started with the IDE, a first-time
    /// maker's game is not covered by it; the bar says NO AGENT FOUND).
    fn start_agent(&mut self, fresh: bool, asked: bool) {
        self.no_agent = !self.finder.finds(self.prefs.agent.program());
        if self.no_agent {
            self.console.tell(Some("NO AGENT FOUND".into()));
            if asked {
                self.memory.console_folded = false;
                self.output_shown = false;
            }
            return;
        }
        let mcp = self.prefs_path.as_ref().map(|p| p.with_file_name("mcp.json"));
        let mut trouble = None;
        if let Some(file) = mcp.as_ref().filter(|_| self.prefs.claude.connect)
            && let Err(e) = super::claude::write_mcp(file)
        {
            trouble = Some(format!("the IDE's tools: {e:#}"));
        }
        // Its part: the game in its folder (`claude::brief`).
        let briefing = match self.prefs_path.as_ref().map(|p| super::claude::brief(&p.with_file_name("agents"), &self.dir, &self.title)) {
            Some(Ok(b)) => b,
            Some(Err(e)) => {
                trouble = trouble.or(Some(format!("what Claude is told: {e:#}")));
                Default::default()
            }
            None => Default::default(),
        };
        let command = match self.prefs.agent {
            Agent::Claude => self.prefs.claude.command(&self.dir, &self.title, fresh, mcp.as_deref(), &briefing),
        };
        trouble = trouble.or_else(|| self.keep_prefs().err().map(|e| format!("the settings: {e:#}")));
        self.console.tell(trouble);
        self.console.type_command(&command, fresh);
        self.memory.console_folded = false;
    }

    /// Opens the game in `dir` instead of this one; while something runs
    /// in the console (Claude at work), which leaving ends, after asking.
    pub fn leave_for(&mut self, dir: PathBuf) {
        if self.prefs.console && self.console.busy() {
            self.leaving = Some(dir);
        } else {
            self.flush();
            self.asks.push(Ask::Project(Wish::Open(dir)));
        }
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

    /// What the game said since last asked: what stops it (once each), a
    /// file that would not load, what the script printed.
    pub fn game_said(&mut self, trouble: Option<String>, said: Vec<(Kind, String)>) {
        if trouble != self.reported {
            if let Some(t) = &trouble {
                let at = super::play::line_of(t);
                self.report(Kind::Error, format!("THE GAME STOPPED: {}", t.lines().next().unwrap_or("")), at);
            }
            self.reported = trouble;
        }
        for (kind, text) in said {
            let at = (kind == Kind::Error).then(|| super::play::line_of(&text)).flatten();
            self.report(kind, text, at);
        }
    }

    /// Where NEW puts games.
    pub fn projects_dir(&self) -> PathBuf {
        self.prefs.projects_dir()
    }

    /// NEW puts games in `dir` from now on (the settings' CHANGE…).
    pub fn set_projects(&mut self, dir: PathBuf) {
        self.prefs.projects = Some(dir);
        if let Err(e) = self.keep_prefs() {
            self.console.tell(Some(format!("the settings: {e:#}")));
        }
    }

    /// Writes down the IDE's settings.
    fn keep_prefs(&self) -> anyhow::Result<()> {
        match &self.prefs_path {
            Some(path) => self.prefs.save(path),
            None => Ok(()),
        }
    }

    /// The settings (the gear at the top right): the console, the agent
    /// and how it starts.
    fn settings(&mut self, ui: &mut egui::Ui) {
        let heading = |ui: &mut egui::Ui, text: &str| {
            ui.add_space(4.0);
            ui.label(RichText::new(text).small().color(QUIET));
        };
        let mut changed = false;
        heading(ui, "PROJECTS");
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.prefs.projects_dir().display().to_string()).small().color(TEXT));
            if ui.button("CHANGE…").on_hover_text("Where NEW puts games.").clicked() {
                self.asks.push(Ask::Project(Wish::BrowseProjects));
                ui.close();
            }
        });
        ui.add_space(8.0);
        heading(ui, "CONSOLE");
        changed |= ui.checkbox(&mut self.prefs.console, "SHOW THE CONSOLE").on_hover_text("The console below the main area; off, none at all (it is for the agents).").changed();
        ui.add_space(8.0);
        heading(ui, "AGENT");
        for a in Agent::ALL {
            changed |= ui.radio_value(&mut self.prefs.agent, a, a.name()).changed();
        }
        let agent = self.prefs.agent;
        ui.add_space(8.0);
        heading(ui, agent.name());
        let claude = &mut self.prefs.claude;
        changed |= ui.checkbox(&mut claude.with_ide, "START WITH THE IDE").on_hover_text("Claude starts in the console when the IDE opens.").changed();
        changed |= ui.checkbox(&mut claude.resume, "GO ON WITH THE LAST CONVERSATION").on_hover_text("Claude goes on with this game's conversation; off, it begins a new one each time.").changed();
        changed |= ui
            .checkbox(&mut claude.connect, "SEE THE IDE")
            .on_hover_text("Claude gets the IDE's tools (what you see, the game you play) through voxcade mcp, named when it starts: no file in the game folder.")
            .changed();
        let said = match claude.session(&self.dir) {
            Some(id) => format!("THIS GAME'S CONVERSATION: {}", id.chars().take(8).collect::<String>()),
            None => "NO CONVERSATION FOR THIS GAME YET".into(),
        };
        ui.add_space(6.0);
        if ui.add_enabled(self.prefs.console, egui::Button::new("NEW CONVERSATION")).on_hover_text("Starts the console anew, Claude in a new conversation (the last one stays with Claude Code: claude --resume finds it).").clicked() {
            self.start_agent(true, true);
            ui.close();
        }
        ui.label(RichText::new(said).small().color(QUIET));
        if changed && let Err(e) = self.keep_prefs() {
            self.console.tell(Some(format!("the settings: {e:#}")));
        }
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
    /// before Play, when the IDE closes).
    pub fn flush(&mut self) {
        if let Some(Shown::Code(buffer)) = &mut self.shown
            && let Err(e) = buffer.save()
        {
            self.message = Some(format!("{e:#}"));
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

    /// Plays the game: shows it again, or starts it (the text being
    /// changed saved first).
    pub fn play(&mut self) {
        self.hush();
        self.notice = None;
        self.help = false;
        match &mut self.stage {
            Some(stage) => stage.shown = true,
            None => {
                self.flush();
                self.asks.push(Ask::Play(None));
            }
        }
    }

    /// AGAIN: the game shown, begun anew from the start (as F5 does).
    pub fn again(&mut self) {
        self.hush();
        (self.notice, self.help) = (None, false);
        if let Some(stage) = &mut self.stage {
            stage.shown = true;
            self.asks.push(Ask::Again);
        }
    }

    /// PLAY SCENARIO over a scenario's file: the game played afresh, begun
    /// from it (the text being changed saved first).
    pub fn play_scenario(&mut self, name: &str) {
        self.hush();
        (self.notice, self.help) = (None, false);
        self.flush();
        if self.stage.is_some() {
            self.asks.push(Ask::Stop);
        }
        self.asks.push(Ask::Play(Some(name.to_string())));
    }

    /// The line under OUTPUT while the game plays: Lua run in it between
    /// two ticks (up and down bring earlier lines back), answered in OUTPUT.
    fn lua_line(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("lua line").frame(egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 12, top: 4, bottom: 8 })).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(">").text_style(TextStyle::Monospace).color(QUIET));
                let edit = ui.add(egui::TextEdit::singleline(&mut self.lua.line).font(TextStyle::Monospace).desired_width(f32::INFINITY).hint_text("Lua in the game being played: player.state.score = 5000"));
                if edit.has_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        self.lua.recall(-1);
                    } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        self.lua.recall(1);
                    }
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(line) = self.lua.take() {
                        self.asks.push(Ask::Eval(line));
                    }
                    edit.request_focus();
                }
            });
        });
    }

    /// Makes the file `typed` names under the heading `sort` (a file of
    /// logic, a scenario, a model, a sound or a song, each a small one to change)
    /// and opens it; its path, or why not (nothing is ever overwritten).
    pub fn make_file(&mut self, sort: &str, typed: &str) -> Result<String, String> {
        let path = new_file(&self.dir, sort, typed)?;
        let name = path.rsplit('/').next().and_then(|n| n.rsplit_once('.')).map_or("", |(n, _)| n);
        let ident: String = name.chars().map(|c| if c == '-' { '_' } else { c }).collect();
        let ident = if ident.starts_with(|c: char| c.is_ascii_digit()) { format!("m_{ident}") } else { ident };
        let full = self.dir.join(&path);
        let made = std::fs::create_dir_all(full.parent().unwrap_or(&self.dir)).map_err(anyhow::Error::from).and_then(|()| match sort {
            "Logic" => Ok(std::fs::write(&full, NEW_LUA.replace("IDENT", &ident).replace("NAME", name))?),
            "Scenarios" => Ok(std::fs::write(&full, NEW_SCENARIO)?),
            "Music" => Ok(std::fs::write(&full, NEW_SONG.replace("NAME", name))?),
            "Sounds" => Ok(std::fs::write(&full, new_sound(name))?),
            _ => crate::editor::frames::ModelFile::begun(&full).save(),
        });
        made.map_err(|e| format!("{path}: {e:#}"))?;
        self.rescan();
        self.open(&path);
        self.back_to_file();
        self.help = false;
        Ok(path)
    }

    /// A line run in the game being played, and what it answered, into
    /// OUTPUT (the user's, or the agent's `eval`).
    pub fn evaluated(&mut self, line: &str, said: &anyhow::Result<String>) {
        let shown = self.output_shown;
        self.output.add(Kind::Note, format!("> {line}"), None, shown);
        match said {
            Ok(text) => {
                for answer in text.lines() {
                    self.output.add(Kind::Print, answer, None, shown);
                }
            }
            Err(e) => {
                let text = format!("{e:#}");
                let at = super::play::line_of(&text);
                let first = text.lines().next().unwrap_or("").to_string();
                self.output.add(Kind::Error, first, at, shown);
            }
        }
    }

    /// PLAY SONG and STOP SONG over a song: plays the open file, or ends it.
    pub fn listen(&mut self) {
        if self.listening.is_some() {
            return self.hush();
        }
        // (A song with a mistake plays nothing: the mistake stands above it.)
        let mistaken = matches!(&self.shown, Some(Shown::Code(b)) if b.code.error.is_some());
        if let Some(path) = self.memory.open.clone().filter(|p| p.ends_with(".mml") && !mistaken) {
            self.listening = Some(path.clone());
            self.asks.push(Ask::Listen(path));
        }
    }

    /// Ends the song playing, if one is (another file opened, the game
    /// played).
    fn hush(&mut self) {
        (self.looping, self.again_at) = (false, None);
        if self.listening.take().is_some() {
            self.asks.push(Ask::Hush);
        }
    }

    /// How hard the game's sound goes into the limiter (`c.audio.drive`).
    pub fn drive(&self) -> f32 {
        self.cfg.audio.drive
    }

    /// The song in `path` to play, at the volume the game and the player
    /// set for music.
    pub fn song(&self, path: &str) -> anyhow::Result<crate::audio::music::Player> {
        let file = self.dir.join(path);
        // As the text stands now, saved or not.
        let text = match (&self.shown, self.memory.open.as_deref()) {
            (Some(Shown::Code(b)), Some(open)) if open == path => b.code.text.clone(),
            _ => std::fs::read_to_string(&file).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?,
        };
        let song = crate::audio::music::parse(&text).map_err(|e| anyhow::anyhow!("{path}: {e:#}"))?;
        let gain = crate::settings::Settings::load(&crate::settings::path_for(&self.dir)).music_gain();
        Ok(crate::audio::music::Player::new(song, self.cfg.audio.music_volume * gain))
    }

    /// PLAY SOUND: the sound open (as its text stands now) rendered for the
    /// speaker, which takes it with `heard` after `Ask::Sound`.
    pub fn play_sound(&mut self) {
        let def = match (&self.shown, &self.memory.open, self.builtin) {
            _ if self.builtin.is_some() => self.brought.iter().find(|(n, _)| Some(*n) == self.builtin).map(|(_, d)| d.clone()),
            (Some(Shown::Code(b)), ..) => b.code.sound.clone(),
            _ => None,
        };
        // (A sound that cannot be read, or is silent, plays nothing: the line
        // above its text says why.)
        let Some(def) = def.filter(|d| *d != SoundDef::Silent) else { return };
        match sounds::render(&def, &self.dir) {
            Ok(samples) if samples.is_empty() => {}
            Ok(samples) => {
                // At the game's volume and the player's, as the game plays a
                // sound in the middle of the view.
                let gain = crate::settings::Settings::load(&crate::settings::path_for(&self.dir)).sound_gain();
                self.again_at = Some(std::time::Instant::now() + std::time::Duration::from_secs_f32(samples.len() as f32 / crate::audio::SAMPLE_RATE as f32) + LOOP_PAUSE);
                self.heard = Some((samples, self.cfg.audio.volume.max(0.0) * gain));
                self.message = None;
                self.asks.push(Ask::Sound);
            }
            Err(e) => self.tell(format!("{e:#}")),
        }
    }

    /// The sound PLAY SOUND made ready, with its volume.
    pub fn heard(&mut self) -> Option<(std::sync::Arc<[f32]>, f32)> {
        self.heard.take()
    }

    /// MAKE IT YOURS: the built-in sound `name` copied into the game as
    /// `sounds/NAME.toml`, and opened.
    pub fn make_it_yours(&mut self, name: &str) {
        let Some((_, def)) = self.brought.iter().find(|(n, _)| *n == name) else { return };
        let path = format!("sounds/{name}.toml");
        if !self.dir.join(&path).exists() {
            let text = sound::write(def, &[format!("{name}: a sound the game brought, copied here to change; the game now plays this one.")]);
            let made = std::fs::create_dir_all(self.dir.join("sounds")).and_then(|()| std::fs::write(self.dir.join(&path), text));
            if let Err(e) = made {
                return self.tell(format!("{path}: {e}"));
            }
        }
        self.rescan();
        self.builtin = None;
        self.unload();
        self.open(&path);
        self.back_to_file();
    }

    /// BUILD: the text being changed saved, the game and this program
    /// packed into one zip in the game's `builds/` (`export.rs`) on a thread
    /// of its own, so the window goes on; the bar says BUILDING… and then
    /// what came of it (`built`).
    pub fn build(&mut self) {
        if self.building.is_some() {
            return;
        }
        self.flush();
        let out = super::export::out_path(&self.dir);
        let (dir, title, to) = (self.dir.clone(), self.title.clone(), out.clone());
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let made = crate::cli::program().map_err(anyhow::Error::from).and_then(|program| super::export::export(&dir, &title, &program, &to));
            let _ = send.send(made);
        });
        self.building = Some((receive, out));
        (self.notice, self.notice_until) = (Some(Notice::Building), None);
    }

    /// BUILD's answer: where the zip is and how big, or why it was not made.
    fn built(&mut self, made: anyhow::Result<super::export::Packed>, out: PathBuf) {
        self.notice_until = None;
        match made {
            Ok(packed) => {
                let bytes = packed.bytes;
                let zip = out.canonicalize().unwrap_or(out);
                // (From the game's folder: the whole path would not fit the
                // bar, and its end is what tells.)
                let dir = self.dir.canonicalize().unwrap_or_else(|_| self.dir.clone());
                let mut said = format!("BUILT {} ({:.1} MB)", zip.strip_prefix(&dir).unwrap_or(&zip).display(), bytes as f64 / 1e6);
                self.report(Kind::Note, said.clone(), None);
                // Ignored by a `.gitignore` and so not packed, yet named by
                // the game: the build would not find them.
                if !packed.missing.is_empty() {
                    let list = packed.missing.join(", ");
                    let warning = format!("NOT PACKED, IGNORED BY .gitignore, BUT NAMED BY THE GAME: {list}. The build will not find them: take them out of .gitignore.");
                    self.report(Kind::Error, warning, None);
                    said = format!("{said}; {} IGNORED FILES THE GAME NAMES NOT PACKED (OUTPUT)", packed.missing.len());
                }
                self.notice = Some(Notice::Built(zip, said));
            }
            Err(e) => {
                let said = format!("BUILD FAILED: {e:#}");
                self.report(Kind::Error, said.clone(), None);
                self.notice = Some(Notice::Failed(said));
            }
        }
    }

    /// Ends the game.
    pub fn stop(&mut self) {
        if self.stage.is_some() {
            self.asks.push(Ask::Stop);
        }
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

    /// What the user sees, for the agent (`view_state`): the game, or the
    /// file open with its cursor, selection, changes and the line Lua
    /// cannot read.
    pub fn state(&self, ctx: &egui::Context) -> serde_json::Value {
        use serde_json::json;
        let game = self.stage.as_ref().map(|s| json!({ "shown": s.shown, "stopped_by": s.trouble }));
        if self.stage.as_ref().is_some_and(|s| s.shown) {
            return json!({ "area": "game", "game": game, "file": self.memory.open });
        }
        if let Some(name) = self.builtin {
            return json!({ "area": "file", "file": { "kind": format!("the built-in sound {name}, read-only; MAKE IT YOURS copies it into the game") }, "game": game });
        }
        let mut file = json!({ "path": self.outside.as_ref().map(|p| p.display().to_string()).or_else(|| self.memory.open.clone()) });
        match (&self.shown, &self.memory.open) {
            (Some(Shown::Code(buffer)), Some(path)) => {
                let text = &buffer.code.text;
                let place = |at: usize| {
                    let before: String = text.chars().take(at).collect();
                    let line = before.matches('\n').count() + 1;
                    json!({ "line": line, "column": before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1 })
                };
                if let Some(range) = egui::TextEdit::load_state(ctx, egui::Id::unique(("code", path.as_str()))).and_then(|s| s.cursor.char_range()) {
                    let [a, b] = range.sorted_cursors().map(|c| c.index.0);
                    file["cursor"] = place(range.primary.index.0);
                    if a != b {
                        file["selected"] = json!(text.chars().skip(a).take(b - a).collect::<String>());
                    }
                }
                file["unsaved"] = json!(buffer.unsaved());
                if path.ends_with(".mml") {
                    file["kind"] = json!("a song (MML), to change; PLAY SONG over it plays it");
                } else if sounds::name_of(path).is_some() {
                    file["kind"] = json!("a sound, to change; PLAY SOUND over it plays it, a sort's button (COIN, JUMP, ...) rolls a fresh one, the faders move it");
                }
                if let Some((line, why)) = &buffer.code.error {
                    file["unreadable"] = json!({ "line": line, "why": why });
                }
                if path.ends_with(".mml") {
                    file["playing"] = json!(self.listening.is_some());
                }
            }
            (Some(Shown::Text(_)), Some(path)) if path.ends_with(".mml") => {
                file["kind"] = json!("a song (MML), to read; PLAY SONG over it plays it");
                file["playing"] = json!(self.listening.is_some());
            }
            (Some(Shown::Note(_)), Some(path)) if sounds::name_of(path).is_some() => file["kind"] = json!("a sound recording; PLAY SOUND over it plays it"),
            (Some(Shown::Text(_)), _) => file["kind"] = json!("text, to read"),
            (Some(Shown::Image(_)), _) => file["kind"] = json!("picture"),
            _ if self.preview.is_some() => file["kind"] = json!("a model, block or level, shown as a picture; its editor opens with EDIT"),
            _ => {}
        }
        json!({ "area": "file", "file": file, "game": game })
    }

    /// What stops the game and what Lua cannot read in the open file, as
    /// text (`errors`).
    pub fn errors(&self) -> String {
        let mut out = Vec::new();
        if let Some(trouble) = self.stage.as_ref().and_then(|s| s.trouble.as_ref()) {
            out.push(format!("The game is stopped by:\n{trouble}"));
        }
        if let (Some(Shown::Code(buffer)), Some(path)) = (&self.shown, &self.memory.open)
            && let Some((line, why)) = &buffer.code.error
        {
            out.push(if *line > 0 { format!("{path}:{line}: {why}") } else { format!("{path}: {why}") });
        }
        let recent: Vec<String> = self.output.errors(5).iter().map(|l| l.text.clone()).collect();
        if !recent.is_empty() {
            out.push(format!("The last errors in OUTPUT:\n{}", recent.join("\n")));
        }
        if out.is_empty() { "No errors.".into() } else { out.join("\n\n") }
    }

    /// What the window asked of the game since last asked.
    pub fn take_asks(&mut self) -> Vec<Ask> {
        std::mem::take(&mut self.asks)
    }

    /// Whether the game has the keys: it shows and nothing else has them
    /// (a click in the console takes them, one on the game gives them
    /// back).
    pub fn keys_to_game(&self, ctx: &egui::Context) -> bool {
        self.stage.as_ref().is_some_and(|s| s.shown) && ctx.memory(|m| m.focused()).is_none_or(|f| f == game_id())
    }

    /// Back to the file from the game (Escape): the game holds still, the
    /// text takes the keys where its cursor stood.
    pub fn back_to_file(&mut self) {
        self.hide_game();
        self.focus_code = true;
    }

    /// The file shown instead of the game, which holds still.
    pub fn hide_game(&mut self) {
        if let Some(stage) = &mut self.stage {
            stage.shown = false;
        }
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
        self.level.as_deref() == Some(path) || (self.level.is_some() && path.starts_with("levels/") && path.ends_with(".toml"))
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
        let (mut undo, mut forget_gone) = (false, false);
        let mut show_zip: Option<PathBuf> = None;
        egui::Panel::top("bar").frame(egui::Frame::new().fill(GROUND).inner_margin(egui::Margin::symmetric(16, 10))).show(ui, |ui| {
            // The name centred on the buttons' height.
            ui.spacing_mut().interact_size.y = ui.text_style_height(&TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
            ui.horizontal(|ui| {
                ui.label(RichText::new(self.title.to_uppercase()).heading().color(ACCENT));
                ui.add_space(16.0);
                // Another game: OPEN, one opened last or another folder (the
                // system's dialog at once while there are none); NEW, one
                // begun from a template.
                let open = ui.button("OPEN").on_hover_text("Another game: one opened last, or a folder of your own.");
                if self.prefs.recent_games(Some(&self.dir)).is_empty() {
                    if open.clicked() {
                        self.asks.push(Ask::Project(Wish::Browse));
                    }
                } else {
                    egui::Popup::from_toggle_button_response(&open)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .align(egui::RectAlign::BOTTOM_START)
                        .gap(6.0)
                        .width(380.0)
                        .frame(egui::Frame::popup(ui.style()).fill(PANEL).inner_margin(egui::Margin::same(14)))
                        .show(|ui| {
                            if let Some(dir) = projects::recent(ui, &self.prefs, Some(&self.dir)) {
                                self.leave_for(dir);
                                ui.close();
                            }
                            ui.add_space(8.0);
                            if ui.button("OTHER FOLDER…").on_hover_text("A game's folder (the one with its conf.lua), in the system's dialog.").clicked() {
                                self.asks.push(Ask::Project(Wish::Browse));
                                ui.close();
                            }
                        });
                }
                if ui.button("NEW").on_hover_text("Begins a new game from a template, in the projects folder (settings).").clicked() {
                    self.new_game = Some(NewGame::new(&self.prefs));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The settings, apart from the game's buttons.
                    let gear = ui.button(RichText::new("⚙").size(18.0)).on_hover_text("Settings: the console, the agent in it and how it starts.");
                    egui::Popup::from_toggle_button_response(&gear)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .align(egui::RectAlign::BOTTOM_END)
                        .gap(6.0)
                        .frame(egui::Frame::popup(ui.style()).fill(PANEL).inner_margin(egui::Margin::same(14)))
                        .show(|ui| self.settings(ui));
                    // HELP in the main area, or the file again.
                    if ui.add(egui::Button::new("HELP").selected(self.help)).on_hover_text("The Lua API by example, to copy into your game.").clicked() {
                        self.help = !self.help;
                        self.hide_game();
                    }
                    ui.add_space(16.0);
                    if ui.button("BUILD").on_hover_text("Packs the game and the program into one zip for this system, to pass on (saved first).").clicked() {
                        self.build();
                    }
                    // PLAY starts the game (saving first) or shows it again,
                    // STOP ends it.
                    // A game held behind the file (after Escape) is gone back
                    // to with BACK TO THE GAME; AGAIN begins it anew.
                    let shown = self.stage.as_ref().is_some_and(|s| s.shown);
                    let held = self.stage.is_some() && !shown;
                    if self.stage.is_some() && ui.button("STOP").on_hover_text("Ends the game.").clicked() {
                        self.asks.push(Ask::Stop);
                    }
                    if self.stage.is_some() && ui.button("AGAIN").on_hover_text("Begins the game again from the start, as F5 does (from its scenario, if it was played from one).").clicked() {
                        self.again();
                        ui.memory_mut(|m| m.request_focus(game_id()));
                    }
                    let (word, tip) = if held { ("BACK TO THE GAME", "The game waits behind the file, as you left it: shows it again (Escape comes back to the file).") } else { ("PLAY", "Plays the game here, straight in (saved first).") };
                    if !shown && ui.button(word).on_hover_text(tip).clicked() {
                        self.play();
                        ui.memory_mut(|m| m.request_focus(game_id()));
                    }
                    if let Some(until) = self.notice_until {
                        match until.checked_duration_since(std::time::Instant::now()) {
                            Some(left) => ui.ctx().request_repaint_after(left),
                            None => (self.notice, self.notice_until) = (None, None),
                        }
                    }
                    // A file just deleted: what was done, and UNDO for a few seconds.
                    if let Some(gone) = &self.gone {
                        match gone.until.checked_duration_since(std::time::Instant::now()) {
                            Some(left) => {
                                ui.ctx().request_repaint_after(left);
                                if gone.can_undo() {
                                    let button = ui.button("UNDO").on_hover_text("Puts the file back as it was.");
                                    #[cfg(test)]
                                    {
                                        self.undo_button = Some(button.rect);
                                    }
                                    undo |= button.clicked();
                                }
                                ui.add(egui::Label::new(RichText::new(&gone.said).small().color(QUIET)).truncate()).on_hover_text(&gone.said);
                            }
                            None => forget_gone = true,
                        }
                    } else if let Some(notice) = &self.notice {
                        // (Truncated, with the whole in its hover text: a long
                        // path never pushes the buttons about.)
                        match notice {
                            Notice::Said(text) => {
                                ui.add(egui::Label::new(RichText::new(text).small().color(QUIET)).truncate()).on_hover_text(text);
                            }
                            Notice::Building => {
                                ui.label(RichText::new("BUILDING…").small().color(ACCENT));
                            }
                            Notice::Built(zip, said) => {
                                if ui.button("SHOW IN FOLDER").on_hover_text("Opens the folder the zip lies in.").clicked() {
                                    show_zip = Some(zip.clone());
                                }
                                ui.add(egui::Label::new(RichText::new(said).small().color(QUIET)).truncate()).on_hover_text(said);
                            }
                            Notice::Failed(said) => {
                                copy_button(ui, said);
                                ui.add(egui::Label::new(RichText::new(said).small().color(ERROR)).truncate()).on_hover_text(said);
                            }
                        }
                    } else if shown {
                        ui.label(RichText::new("ESCAPE back to the file · F5 again · F9 keeps the run").small().color(QUIET));
                    }
                });
            });
        });
        if let Some(zip) = show_zip {
            super::export::show_in_folder(&zip);
        }
        if forget_gone {
            self.gone = None;
        }
        if undo {
            self.undo();
        }
        let (mut clicked, mut edit, mut made, mut builtin) = (None, None, None, None);
        let mut managed: Option<(Act, String)> = None;
        let mut renamed: Option<(String, String)> = None;
        let mut stop_renaming = false;
        // The margins on the right lie inside the scroll areas, so that their
        // bars sit at the edge.
        egui::Panel::left("files").default_size(250.0).frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 14, right: 0, top: 12, bottom: 12 })).show(ui, |ui| {
            egui::ScrollArea::vertical().content_margin(egui::Margin { right: 6, ..egui::Margin::ZERO }).show(ui, |ui| {
                let open = if self.outside.is_some() || self.builtin.is_some() { None } else { self.memory.open.clone() };
                #[cfg(test)]
                self.headings.clear();
                // Each sort of file under its name, which opens and closes
                // it: the logic, then the assets, the big groups closed at
                // first.
                // The scenarios after the logic, there even while the game
                // has none: every game should have some to be tried with.
                let logic = [("Logic", &self.files.logic, true), ("Scenarios", &self.files.scenarios, true)].into_iter();
                let assets = self.files.assets.iter().map(|g| (g.name, &g.files, g.files.len() <= 12 && g.name != "Other"));
                for (i, (sort, files, open_at_first)) in logic.chain(assets).enumerate() {
                    if i > 0 {
                        ui.add_space(10.0);
                    }
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(sort), open_at_first);
                    let header = crate::gui::row(ui, &format!("{} ({})", sort.to_uppercase(), files.len()), TextStyle::Small, QUIET, false, None);
                    let at = egui::pos2(header.rect.right() - INSET - 4.0, header.rect.center().y);
                    // A plus of its own left of the arrow (over the row, so it
                    // takes its clicks): a new file there, its name typed
                    // below.
                    let mut over_plus = false;
                    if let Some(tip) = new_what(sort) {
                        let rect = egui::Rect::from_center_size(egui::pos2(at.x - 26.0, at.y), egui::vec2(24.0, 22.0));
                        let plus = ui.interact(rect, ui.make_persistent_id((sort, "plus")), egui::Sense::click()).on_hover_text(tip);
                        over_plus = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| rect.contains(p));
                        if plus.hovered() {
                            ui.painter().rect_filled(rect, 3.0, crate::gui::ROW_HOVERED);
                        }
                        #[cfg(test)]
                        self.headings.push((sort.to_string(), rect, header.rect));
                        let colour = if plus.hovered() { TEXT } else { QUIET };
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "+", TextStyle::Body.resolve(ui.style()), colour);
                        if plus.clicked() {
                            self.naming = Some(Naming { sort: sort.to_string(), text: String::new(), problem: None, fresh: true });
                            state.set_open(true);
                        }
                    }
                    if header.clicked() && !over_plus {
                        state.toggle(ui);
                    }
                    arrow(ui, at, state.openness(ui.ctx()));
                    // A file being renamed from the main area: its heading opens.
                    if self.renaming.as_ref().is_some_and(|r| r.fresh && files.contains(&r.path)) {
                        state.set_open(true);
                    }
                    state.show_body_unindented(ui, |ui| {
                        // The name being typed for a new file, first.
                        if let Some(naming) = self.naming.as_mut().filter(|n| n.sort == sort) {
                            let (mut enter, mut cancel) = (false, false);
                            ui.horizontal(|ui| {
                                ui.add_space(INSET);
                                let edit = ui.add(egui::TextEdit::singleline(&mut naming.text).font(TextStyle::Body).hint_text("name, then Enter").desired_width(f32::INFINITY));
                                if std::mem::take(&mut naming.fresh) {
                                    edit.request_focus();
                                }
                                if edit.lost_focus() {
                                    enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    cancel = ui.input(|i| i.key_pressed(egui::Key::Escape)) || (!enter && naming.text.is_empty());
                                    if enter {
                                        // (The file made opens in a text that would take the key too.)
                                        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                                        edit.request_focus();
                                    }
                                }
                            });
                            if let Some(problem) = &naming.problem {
                                ui.label(RichText::new(problem).small().color(ERROR));
                            }
                            if cancel {
                                self.naming = None;
                            } else if enter {
                                made = Some((sort.to_string(), naming.text.clone()));
                            }
                        }
                        // The scripts by their paths; assets say what they
                        // are by their group: the name alone, the path where
                        // two have the same name.
                        let base = |f: &str| f.rsplit('/').next().unwrap_or(f).to_string();
                        for f in files {
                            // A file being renamed: its name typed in its place.
                            if let Some(r) = self.renaming.as_mut().filter(|r| r.path == *f) {
                                let (ext, mut enter, mut cancel) = (format!(".{}", manage::split_name(f).2), false, false);
                                ui.horizontal(|ui| {
                                    ui.add_space(INSET);
                                    let id = ui.make_persistent_id(("renaming", f.as_str()));
                                    let room = if ext.len() > 1 { 12.0 + 7.0 * ext.len() as f32 } else { 0.0 };
                                    let edit = ui.add(egui::TextEdit::singleline(&mut r.text).id(id).font(TextStyle::Body).desired_width((ui.available_width() - room).max(60.0)));
                                    if ext.len() > 1 {
                                        ui.label(RichText::new(&ext).small().color(QUIET));
                                    }
                                    if std::mem::take(&mut r.fresh) {
                                        // The name is selected, to be typed over.
                                        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                                        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(r.text.chars().count()))));
                                        state.store(ui.ctx(), id);
                                        edit.request_focus();
                                        edit.scroll_to_me(None);
                                    }
                                    if edit.lost_focus() {
                                        enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                        // Anything but Enter leaves it (a click elsewhere too).
                                        cancel = !enter;
                                        if enter {
                                            // (A name that cannot be keeps the field, with the keys.)
                                            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                                            edit.request_focus();
                                        }
                                    }
                                });
                                if let Some(problem) = &r.problem {
                                    ui.label(RichText::new(problem).small().color(ERROR));
                                }
                                if cancel {
                                    stop_renaming = true;
                                } else if enter {
                                    renamed = Some((f.clone(), r.text.clone()));
                                }
                                continue;
                            }
                            let short = sort != "Logic" && files.iter().filter(|g| base(g) == base(f)).count() == 1;
                            let on = open.as_deref() == Some(f.as_str());
                            let response = crate::gui::row(ui, &if short { base(f) } else { f.clone() }, TextStyle::Body, if on { ACCENT } else { TEXT }, on, None);
                            if response.clicked() {
                                clicked = Some(f.clone());
                            }
                            // A double click opens it in its editor.
                            if response.double_clicked() {
                                edit = Some(f.clone());
                            }
                            // A right click: RENAME and DELETE.
                            let why = manage::why_not(f, sort);
                            response.context_menu(|ui| {
                                for (word, act, tip) in [("RENAME", Act::Rename, "Gives the file another name, typed here in the list."), ("DELETE", Act::Delete, "Moves the file to the trash. UNDO shows for a few seconds.")] {
                                    if ui.add_enabled(why.is_none(), egui::Button::new(word)).on_hover_text(tip).on_disabled_hover_text(why.unwrap_or("")).clicked() {
                                        managed = Some((act, f.clone()));
                                        ui.close();
                                    }
                                }
                            });
                        }
                        if files.is_empty() && self.naming.as_ref().is_none_or(|n| n.sort != sort) {
                            let none = match sort {
                                "Scenarios" => "None yet: runs begun late, to try what happens there.",
                                "Models" | "Sounds" | "Music" => "None yet: the plus makes one.",
                                _ => "",
                            };
                            if !none.is_empty() {
                                ui.label(RichText::new(none).small().color(QUIET));
                            }
                        }
                        // The sounds the game has from the engine and its
                        // ready-made rules, which no file of its own replaces.
                        if sort == "Sounds" {
                            for (name, _) in &self.brought {
                                if files.iter().any(|f| sounds::name_of(f) == Some(name)) {
                                    continue;
                                }
                                let on = self.builtin == Some(*name) && self.outside.is_none();
                                if crate::gui::row(ui, name, TextStyle::Body, if on { ACCENT } else { QUIET }, on, Some("built in")).clicked() {
                                    builtin = Some(*name);
                                }
                            }
                        }
                    });
                }
            });
        });
        if let Some(path) = clicked {
            self.open(&path);
            self.back_to_file();
            self.help = false;
        }
        if let Some(name) = builtin {
            // Shown like a file, which is let go of (saved) first.
            self.flush();
            self.hush();
            self.outside = None;
            self.unload();
            self.builtin = Some(name);
            self.hide_game();
            self.help = false;
        }
        if let Some(path) = edit.filter(|p| self.editable(p)) {
            self.edits.push(path);
        }
        if let Some((sort, typed)) = made {
            match self.make_file(&sort, &typed) {
                Ok(_) => self.naming = None,
                Err(problem) => {
                    if let Some(n) = &mut self.naming {
                        n.problem = Some(problem);
                    }
                }
            }
        }
        if let Some((act, path)) = managed {
            self.manage(act, &path);
        }
        if stop_renaming {
            self.renaming = None;
        }
        if let Some((path, typed)) = renamed {
            match self.rename(&path, &typed) {
                Ok(()) => self.renaming = None,
                Err(problem) => {
                    if let Some(r) = &mut self.renaming {
                        r.problem = Some(problem);
                    }
                }
            }
        }
        // The console below the main area, folded to its heading or as
        // high as dragged (and kept so); or none (CONSOLE). A game's first
        // time it is folded: the bar only, the room is the main area's.
        let folded = self.memory.console_folded;
        if self.prefs.console {
            // At first almost half the window: room for Claude Code's
            // screens.
            let tall = self.memory.console_height.unwrap_or(0.45 * ui.available_height());
            let panel = if folded { egui::Panel::bottom("console folded") } else { egui::Panel::bottom("console").resizable(true).default_size(tall).min_size(120.0) };
            let shown = panel.frame(egui::Frame::new().fill(terminal::BACKGROUND)).show(ui, |ui| {
                let pressed = self.console.heading(ui, &mut self.memory.console_folded, self.prefs.agent, self.prefs.claude.connect, &mut self.output_shown, self.output.unseen);
                if pressed.agent {
                    self.output_shown = false;
                    self.start_agent(false, true);
                }
                if pressed.clear {
                    self.output.clear();
                }
                if pressed.copy_all {
                    ui.ctx().copy_text(self.output.text());
                }
                if !folded {
                    if self.output_shown {
                        if self.stage.is_some() {
                            self.lua_line(ui);
                        }
                        // An error clicked: its file, at its line.
                        if let Some((file, line)) = self.output.ui(ui).filter(|(f, _)| self.files.has(f)) {
                            self.hide_game();
                            self.help = false;
                            self.open(&file);
                            self.goto = Some((line, true));
                        }
                    } else if self.no_agent {
                        if agent_missing(ui, self.prefs.agent) {
                            self.start_agent(false, true);
                        }
                    } else {
                        self.console.body(ui);
                        if let Some((file, line)) = self.console.take_opened() {
                            self.open_from_console(&file, line);
                        }
                    }
                }
            });
            // The height the user dragged it to, kept once the mouse is let go.
            if !folded && !self.memory.console_folded {
                let height = shown.response.rect.height();
                if self.console_seen.is_some_and(|h| (h - height).abs() > 0.5) && ui.input(|i| i.pointer.any_down()) {
                    self.console_dragged = true;
                }
                self.console_seen = Some(height);
                if self.console_dragged && !ui.input(|i| i.pointer.any_down()) {
                    self.console_dragged = false;
                    self.memory.console_height = Some(height);
                    let _ = self.save();
                }
            } else {
                (self.console_seen, self.console_dragged) = (None, false);
            }
            if self.memory.console_folded != folded {
                let _ = self.save();
            }
        }
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
        // Leaving would end what runs in the console: asked first.
        if let Some(dir) = self.leaving.clone() {
            let name = dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned());
            let mut answer = None;
            let modal = egui::Modal::new(egui::Id::unique("leaving")).frame(egui::Frame::popup(ui.style()).fill(PANEL).inner_margin(egui::Margin::same(20))).show(ui.ctx(), |ui| {
                ui.set_width(460.0);
                ui.label(RichText::new("STILL RUNNING").color(ACCENT));
                ui.add_space(8.0);
                ui.label(format!("Something still runs in the console ({} at work?). Opening {name} ends it.", self.prefs.agent.called()));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("OPEN {}", name.to_uppercase())).clicked() {
                        answer = Some(true);
                    }
                    if ui.button("STAY").clicked() {
                        answer = Some(false);
                    }
                });
            });
            if answer.is_none() && modal.should_close() {
                answer = Some(false);
            }
            if let Some(go) = answer {
                self.leaving = None;
                if go {
                    self.flush();
                    self.asks.push(Ask::Project(Wish::Open(dir)));
                }
            }
        }
    }

    /// The main area playing the game: its picture filling it (what stops
    /// it is said above, by `main`), the keys its while it has the focus.
    fn game(&mut self, ui: &mut egui::Ui) {
        let keys = self.stage.as_ref().map(|s| s.keys.clone()).unwrap_or_default();
        // The game's keys on a line under it (wrapping, never wider than
        // the room), the picture takes what is left.
        if !keys.is_empty() {
            egui::Panel::bottom("keys line").frame(egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 12, top: 6, bottom: 8 })).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, (what, key)) in keys.iter().enumerate() {
                        if i > 0 {
                            ui.label(RichText::new("·").color(QUIET));
                        }
                        ui.label(RichText::new(what).color(QUIET));
                        ui.label(RichText::new(key).color(TEXT));
                    }
                });
            });
        }
        let Some(stage) = &mut self.stage else { return };
        let room = egui::vec2(ui.available_width() - 20.0, ui.available_height() - 14.0).max(egui::vec2(1.0, 1.0));
        let (rect, response) = ui.allocate_exact_size(room, egui::Sense::click());
        let pixels = ui.ctx().pixels_per_point();
        stage.wanted = Some([(rect.width() * pixels).round() as u32, (rect.height() * pixels).round() as u32]);
        let response = ui.interact(rect, game_id(), egui::Sense::click()).union(response);
        if response.clicked() {
            response.request_focus();
        }
        if response.has_focus() {
            ui.memory_mut(|m| m.set_focus_lock_filter(game_id(), egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }));
        }
        if let Some(id) = stage.picture {
            ui.painter().image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }

    /// Whether the text shown is saved: "SAVED" when the file holds what is
    /// shown, "NOT SAVED YET" between a change and the autosave (None: no
    /// text to change).
    fn saved_word(&self) -> Option<&'static str> {
        match &self.shown {
            Some(Shown::Code(b)) if b.gone => Some("FILE GONE"),
            Some(Shown::Code(b)) => Some(if b.unsaved() { "NOT SAVED YET" } else { "SAVED" }),
            _ => None,
        }
    }

    /// Where the game's code names the sound `name`, looked for afresh when
    /// it is another sound or a second has gone.
    fn refresh_used(&mut self, name: &str) {
        if self.used.as_ref().is_none_or(|u| u.name != name || u.at.elapsed() >= USED_FOR) {
            let rows = sounds::used(&self.dir, &self.files.logic, name);
            self.used = Some(Used { name: name.to_string(), rows, at: std::time::Instant::now() });
        }
    }

    /// What is said under USED of the sound `name` beside the lines that
    /// name it: that the game's own rules play it, when it is one of the
    /// sounds they bring.
    fn rules_say(&self, name: &str) -> Vec<String> {
        if !self.brought.iter().any(|(n, _)| *n == name) {
            return Vec::new();
        }
        let core = crate::audio::CORE_FILES.iter().any(|(n, _)| *n == name);
        vec![if core { "Played by the engine itself, when the world is hit, blown up or falls apart.".into() } else { "Played by the game's ready-made rules.".to_string() }]
    }

    /// A sound the game brings, shown in the main area: what it is, PLAY
    /// SOUND, MAKE IT YOURS, its text to read, and where it is used.
    fn builtin_ui(&mut self, ui: &mut egui::Ui, name: &'static str) {
        let Some(def) = self.brought.iter().find(|(n, _)| *n == name).map(|(_, d)| d.clone()) else {
            self.builtin = None;
            return;
        };
        let line = format!("{} Built in: the game has no file of its own for it.", sounds::describe(&def));
        let (mut play, mut yours) = (false, false);
        ui.horizontal(|ui| {
            ui.label(RichText::new(line).color(QUIET));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                play = ui.button("PLAY SOUND").on_hover_text("Plays the sound as it stands, at the game's sound volume.").clicked();
                yours = ui.button("MAKE IT YOURS").on_hover_text(format!("Copies it into your game as sounds/{name}.toml, to change. The game then plays yours.")).clicked();
            });
        });
        if let Some(message) = &self.message {
            error_row(ui, message, message);
        }
        ui.add_space(10.0);
        self.refresh_used(name);
        let rows = self.used.as_ref().map(|u| u.rows.clone()).unwrap_or_default();
        let says = self.rules_say(name);
        let (mut rects, mut go) = (Vec::new(), None);
        let text = sound::write(&def, &[]);
        egui::ScrollArea::both().id_salt(("builtin", name)).auto_shrink([false, false]).content_margin(egui::Margin { right: 12, bottom: 14, ..egui::Margin::ZERO }).show(ui, |ui| {
            let font = TextStyle::Monospace.resolve(ui.style());
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| ui.fonts_mut(|f| f.layout_job(super::code::highlight(text.as_str(), &font, super::code::Syntax::Sound)));
            let mut shown = text.as_str();
            ui.add(egui::TextEdit::multiline(&mut shown).code_editor().frame(egui::Frame::NONE).desired_width(f32::INFINITY).layouter(&mut layouter));
            go = used_ui(ui, name, &says, &rows, &mut rects);
        });
        #[cfg(test)]
        {
            self.used_rows = rects;
        }
        if play {
            self.play_sound();
        }
        if yours {
            self.make_it_yours(name);
        }
        if let Some((file, line)) = go {
            self.builtin = None;
            self.hide_game();
            self.open(&file);
            self.goto = Some((line, true));
        }
    }

    /// What stops the game or a file of it, as one line in red over
    /// whatever the main area shows: its first line, TO THE LINE where it
    /// names one of the game's files, SHOW (OUTPUT, the console unfolded)
    /// and COPY. It goes when the files are mended. None: all is well.
    fn game_error(&self) -> Option<(String, String)> {
        let first = |t: &str| t.lines().next().unwrap_or("").to_string();
        let stage = self.stage.as_ref();
        if let Some(t) = stage.and_then(|s| s.trouble.as_deref()) {
            Some((format!("THE GAME STOPPED: {}", first(t)), t.to_string()))
        } else if let Some(t) = stage.and_then(|s| s.unloaded.as_deref()) {
            Some((first(t), t.to_string()))
        } else {
            self.start_failed.as_deref().map(|t| (format!("THE GAME WOULD NOT START: {}", first(t)), t.to_string()))
        }
    }

    /// The game would not start (PLAY, or the game as the files are now):
    /// said in the red line and in OUTPUT, until it starts or the files are
    /// mended.
    pub fn wont_start(&mut self, why: String) {
        let at = super::play::line_of(&why);
        self.report(Kind::Error, format!("THE GAME WOULD NOT START: {}", why.lines().next().unwrap_or("")), at);
        self.start_failed = Some(why);
    }

    /// The game starts, or loads again: the red line for it goes.
    pub fn start_mended(&mut self) {
        self.start_failed = None;
    }

    /// Whether the game would not start, as far as is known.
    pub fn start_failed(&self) -> bool {
        self.start_failed.is_some()
    }

    /// SHOW: OUTPUT in the console below, which unfolds (and is there
    /// again if the settings had it off).
    fn show_output(&mut self) {
        self.prefs.console = true;
        self.output_shown = true;
        self.memory.console_folded = false;
        let _ = self.save();
    }

    /// The main area: the red line of what is wrong with the game, over the
    /// open file (or the game, HELP, nothing).
    fn main(&mut self, ui: &mut egui::Ui) {
        if let Some((said, copy)) = self.game_error() {
            let at = super::play::line_of(&copy).filter(|(file, _)| self.files.has(file));
            let (go, show) = game_error_row(ui, &said, &copy, at.is_some());
            ui.add_space(8.0);
            if show {
                self.show_output();
            }
            if let (true, Some((file, line))) = (go, at) {
                self.hide_game();
                self.help = false;
                self.outside = None;
                self.open(&file);
                self.goto = Some((line, true));
            }
        }
        self.main_area(ui);
    }

    /// The main area: the open file.
    fn main_area(&mut self, ui: &mut egui::Ui) {
        // Asked for the text open, once: the line to go to, the keys to it
        // (gone if what shows is no text).
        let (goto, focus) = (self.goto.take(), std::mem::take(&mut self.focus_code));
        if self.stage.as_ref().is_some_and(|s| s.shown) {
            return self.game(ui);
        }
        if self.help {
            return super::help::ui(ui, &mut self.help_topic);
        }
        if let Some(file) = self.outside.clone() {
            if self.shown.is_none() {
                self.shown = Some(load(&file, &file.to_string_lossy(), ui.ctx()));
                self.facts = None;
            }
            ui.label(RichText::new(format!("{} (not a file of the game, to look at)", file.display())).color(QUIET));
            ui.add_space(10.0);
            egui::ScrollArea::both().id_salt("outside").auto_shrink([false, false]).content_margin(egui::Margin { right: 12, bottom: 14, ..egui::Margin::ZERO }).show(ui, |ui| match self.shown.as_ref() {
                Some(Shown::Image(texture)) => picture(ui, texture),
                Some(Shown::Note(note)) => {
                    ui.label(RichText::new(note.as_str()).color(QUIET));
                }
                _ => {}
            });
            return;
        }
        if let Some(name) = self.builtin {
            return self.builtin_ui(ui, name);
        }
        let Some(path) = self.memory.open.clone() else {
            ui.label(RichText::new("Nothing here yet.").color(QUIET));
            return;
        };
        // The open file gone from the folder (the agent, or anything outside
        // the IDE, removed or renamed it): said plainly, nothing made again
        // by itself. A text keeps its text (SAVE AGAIN, below); anything else
        // has nothing to show.
        if !self.files.has(&path) && !matches!(self.shown, Some(Shown::Code(_))) {
            ui.label(RichText::new(format!("{path} is gone from the game folder: it was removed or renamed outside the IDE. Choose another file in the list.")).color(ACCENT));
            return;
        }
        if self.shown.is_none() {
            // Scripts and the texts they read, sounds and songs are to
            // change, the rest to look at.
            let to_change = self.files.logic.contains(&path) || self.files.scenarios.contains(&path) || path.ends_with(".mml") || (path.ends_with(".toml") && sounds::name_of(&path).is_some()) || (self.model_text && path.ends_with(".vvx"));
            self.shown = Some(if to_change {
                match Buffer::open(&self.dir, &path, &self.memory_path.with_extension("journal")) {
                    Ok(buffer) => Shown::Code(Box::new(buffer)),
                    Err(e) => Shown::Note(format!("{e:#}")),
                }
            } else {
                load(&self.dir.join(&path), &path, ui.ctx())
            });
            let level = self.is_level(&path);
            self.facts = match () {
                _ if self.is_scene(&path) => Some(scene_facts(&self.dir.join(&path), &path)),
                _ if self.editable(&path) || is_model_file(&path) => Some(facts(&self.dir.join(&path), &path, level)),
                _ if path.ends_with(".mml") => Some(song_facts(&self.dir.join(&path))),
                _ => None,
            };
            // What an editor opens shows as a picture.
            let preview = match () {
                _ if level => Some(Preview::level(&self.dir, &path, &self.cfg)),
                _ if self.is_scene(&path) && !self.model_text => Some(Preview::scene(&self.dir, &path, &self.cfg)),
                _ if is_model_file(&path) && !self.model_text => Some(Preview::model(&self.dir, &path, &self.cfg)),
                _ => None,
            };
            self.preview = preview.and_then(|p| p.map_err(|e| self.message = Some(format!("{e:#}"))).ok());
            match (&mut self.preview, self.spare.take()) {
                (Some(new), Some(old)) => new.adopt(old),
                (None, Some(old)) => self.retired.extend(old.texture()),
                _ => {}
            }
        }
        // What the text says is read afresh if it changed (its line of words and
        // its mistake stand above it).
        if let Some(Shown::Code(b)) = &mut self.shown {
            b.code.check();
        }
        // Whether the text shown is saved (a word above it, left of the buttons),
        // and RENAME and DELETE beside it (off with the reason for a file the
        // game needs).
        let saved = self.saved_word();
        let why_not = self.files.sort_of(&path).map_or(Some("This is not a file of the game."), |group| manage::why_not(&path, group));
        let mut act = None;
        // What an editor opens: a line of what it is, and EDIT; a song:
        // what it is, and PLAY.
        let facts = match &self.shown {
            // A song's line follows its text, a model's too.
            Some(Shown::Code(b)) if path.ends_with(".mml") => Some(b.code.song.clone().unwrap_or_else(|| "The song cannot be read.".into())),
            Some(Shown::Code(b)) if path.ends_with(".vvx") => Some(b.code.model.clone().unwrap_or_else(|| "The model cannot be read.".into())),
            _ => self.facts.clone(),
        };
        let had_facts = facts.is_some();
        if let Some(facts) = facts {
            ui.horizontal(|ui| {
                ui.label(RichText::new(facts).color(QUIET));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    if self.editable(&path) {
                        if ui.button("EDIT").on_hover_text("Opens it in its editor, a window of its own (so does a double click in the list).").clicked() {
                            self.flush();
                            self.edits.push(path.clone());
                        }
                        // A model is text too: shown as its text to change,
                        // or as its picture.
                        if path.ends_with(".vvx") {
                            let (word, tip) = if self.model_text { ("PICTURE", "Shows the model as the game draws it.") } else { ("TEXT", "Shows the model's text, to read and change (its palette, its frames or parts, layer by layer).") };
                            if ui.button(word).on_hover_text(tip).clicked() {
                                self.flush();
                                self.model_text = !self.model_text;
                                self.unload();
                            }
                        }
                    } else if !path.ends_with(".mml") {
                    } else if self.listening.is_some() {
                        if ui.button("STOP SONG").on_hover_text("Ends the song.").clicked() {
                            self.listen();
                        }
                    } else if ui.button("PLAY SONG").on_hover_text("Plays the song as it stands, at the game's music volume.").clicked() {
                        self.listen();
                    }
                    act = act.or(manage::buttons(ui, why_not));
                    saved_label(ui, saved);
                });
            });
            // (A text to change shows it with its own messages.)
            if !matches!(self.shown, Some(Shown::Code(_))) && let Some(message) = &self.message {
                error_row(ui, message, message);
            }
            ui.add_space(10.0);
        }
        // A scenario: what it is, and PLAY SCENARIO.
        if let Some(name) = path.strip_prefix("scenarios/").and_then(|n| n.strip_suffix(".lua")).filter(|_| self.files.scenarios.contains(&path)) {
            let mut play = false;
            ui.horizontal(|ui| {
                ui.label(RichText::new("A scenario: the game begun late, set up by this code").color(QUIET));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    play = ui.button("PLAY SCENARIO").on_hover_text("Plays the game from it (saved first); F5 begins it again.").clicked();
                    act = act.or(manage::buttons(ui, why_not));
                    saved_label(ui, saved);
                });
            });
            ui.add_space(6.0);
            if play {
                self.play_scenario(name);
            }
        } else if !had_facts && sounds::name_of(&path).is_none() {
            // Any other file: its buttons, and the word if it is a text to change.
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    act = act.or(manage::buttons(ui, why_not));
                    saved_label(ui, saved);
                });
            });
            ui.add_space(6.0);
        }
        // A sound: what it is in words, and PLAY SOUND; where it is used
        // lies under its text.
        let sound_name = sounds::name_of(&path).map(str::to_string);
        let mut play_sound = false;
        if sound_name.is_some() {
            let key = match &self.shown {
                Some(Shown::Code(b)) => format!("{path}\0{}", b.code.text),
                _ => path.clone(),
            };
            if self.sound_line.as_ref().is_none_or(|(k, _)| *k != key) {
                let line = match &self.shown {
                    Some(Shown::Code(b)) => b.code.sound.as_ref().map_or_else(|| "The sound cannot be read.".to_string(), sounds::describe),
                    _ => "The sound cannot be read.".to_string(),
                };
                self.sound_line = Some((key, line));
            }
            let line = self.sound_line.as_ref().map(|(_, l)| l.clone()).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(RichText::new(line).color(QUIET));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    play_sound = ui.button("PLAY SOUND").on_hover_text("Plays the sound as it stands, at the game's sound volume.").clicked();
                    act = act.or(manage::buttons(ui, why_not));
                    saved_label(ui, saved);
                });
            });
            if !matches!(self.shown, Some(Shown::Code(_))) && let Some(message) = &self.message {
                error_row(ui, message, message);
            }
            ui.add_space(6.0);
        }
        // A sound of numbers to change: the maker between its line and its text.
        let id = egui::Id::unique(("code", path.as_str()));
        if sound_name.is_some() && path.ends_with(".toml") && matches!(self.shown, Some(Shown::Code(_))) {
            play_sound |= self.maker_ui(ui, id);
            ui.add_space(10.0);
        }
        if play_sound {
            self.play_sound();
        }
        if let Some(act) = act {
            self.manage(act, &path);
            return;
        }
        if let Some(preview) = &mut self.preview {
            // Down to the console, the scroll bar's room kept at the right.
            let animate = self.animate;
            ui.allocate_ui(egui::vec2(ui.available_width() - 12.0, ui.available_height() - 14.0), |ui| preview.ui(ui, animate));
            return;
        }
        // A line to go to (a game's error): the cursor there, in view.
        let mut focus = focus;
        if let (Some((line, keys)), Some(Shown::Code(buffer))) = (goto, &self.shown) {
            let at: usize = buffer.code.text.split('\n').take(line.saturating_sub(1)).map(|l| l.chars().count() + 1).sum();
            let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at))));
            state.store(ui.ctx(), id);
            focus |= keys;
            let row = ui.text_style_height(&TextStyle::Monospace);
            self.restore = Some(line.saturating_sub(6) as f32 * row);
        }
        // The cursor where it stood when the text was last open (another
        // session: in this one egui keeps it), the keys to the text if
        // asked.
        if let Some(Shown::Code(buffer)) = &self.shown {
            if egui::TextEdit::load_state(ui.ctx(), id).is_none()
                && let Some(&at) = self.memory.cursor.get(&path)
            {
                let mut state = egui::text_edit::TextEditState::default();
                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at.min(buffer.code.text.chars().count())))));
                state.store(ui.ctx(), id);
            }
            if focus {
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
        // A text being changed: what it asks of the user above it.
        let mut put_back = false;
        if let Some(Shown::Code(buffer)) = &mut self.shown {
            if buffer.gone {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{path} is gone from the game folder: it was removed or renamed outside the IDE. Its text is kept here.")).color(ACCENT));
                    if ui.button("SAVE AGAIN").on_hover_text(format!("Writes the text back into the game folder as {path}.")).clicked() {
                        put_back = true;
                    }
                });
                ui.add_space(8.0);
            }
            if buffer.theirs.is_some() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Changed outside while you changed it too.").color(ACCENT));
                    if ui.button("KEEP MINE").clicked() {
                        buffer.keep_mine();
                    }
                    if ui.button("TAKE THE NEW").clicked() {
                        buffer.take_theirs();
                    }
                });
                ui.add_space(8.0);
            }
            if buffer.restored {
                ui.label(RichText::new("Your changes came back from the journal.").color(QUIET));
                ui.add_space(8.0);
            }
            if let Some((line, why)) = &buffer.code.error {
                // Copied with the file's name, to stand alone when pasted.
                let (shown, copy) = if *line > 0 { (format!("LINE {line}: {why}"), format!("{path} line {line}: {why}")) } else { (why.clone(), format!("{path}: {why}")) };
                error_row(ui, &shown, &copy);
                ui.add_space(8.0);
            }
            // Where the game played stopped, if in this file: its line
            // marked until the game takes the file anew (the red line over the
            // main area says why).
            let trouble = self.stage.as_ref().and_then(|s| s.trouble.as_deref());
            let stopped = trouble.and_then(|t| super::play::line_of(t).filter(|(file, _)| *file == path).map(|(file, line)| (file, line, t)));
            buffer.code.stopped = stopped.as_ref().map(|(_, line, _)| *line);
            if let Some(message) = &self.message {
                error_row(ui, message, message);
                ui.add_space(8.0);
            }
            if put_back {
                match buffer.put_back() {
                    Ok(()) => self.message = None,
                    Err(e) => self.message = Some(format!("{e:#}")),
                }
            }
        }
        if put_back {
            self.rescan();
        }
        let (mut used_rows, mut says) = (Vec::new(), Vec::new());
        if let Some(name) = &sound_name {
            self.refresh_used(name);
            used_rows = self.used.as_ref().map(|u| u.rows.clone()).unwrap_or_default();
            says = self.rules_say(name);
        }
        let (mut rects, mut go_used) = (Vec::new(), None);
        let mut scroll = egui::ScrollArea::both().id_salt(&path).auto_shrink([false, false]).content_margin(egui::Margin { right: 12, bottom: 14, ..egui::Margin::ZERO });
        if let Some(offset) = self.restore.take() {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let out = scroll.show(ui, |ui| {
            match self.shown.as_mut().expect("loaded above") {
            Shown::Code(buffer) => {
                if buffer.code.ui(ui, id) {
                    buffer.edited(std::time::Instant::now());
                }
                if let Some(range) = egui::TextEdit::load_state(ui.ctx(), id).and_then(|s| s.cursor.char_range()) {
                    self.memory.cursor.insert(path.clone(), range.primary.index.0);
                }
            }
            Shown::Text(text) => {
                let mut text = text.as_str();
                ui.add(egui::TextEdit::multiline(&mut text).code_editor().frame(egui::Frame::NONE).desired_width(f32::INFINITY));
            }
            Shown::Image(texture) => picture(ui, texture),
            Shown::Note(note) => {
                if !note.is_empty() {
                    ui.label(RichText::new(note.as_str()).color(QUIET));
                }
            }
            }
            if let Some(name) = &sound_name {
                go_used = used_ui(ui, name, &says, &used_rows, &mut rects);
            }
        });
        self.memory.scroll.insert(path, out.state.clamped_offset().y);
        #[cfg(test)]
        {
            self.used_rows = rects;
        }
        if let Some((file, line)) = go_used {
            self.hide_game();
            self.help = false;
            self.open(&file);
            self.goto = Some((line, true));
        }
    }
}

impl Shell {
    /// The sound maker over the sound open (`maker`; its text in the text
    /// edit `id`): a dice and a row of sorts, then UNDO, a row of faders.
    /// Every change is the text rewritten in the buffer, as typing is, and
    /// one step back for UNDO and Ctrl+Z alike; a roll and UNDO play the
    /// sound at once. Whether it is to play now because a fader was let go.
    fn maker_ui(&mut self, ui: &mut egui::Ui, id: egui::Id) -> bool {
        let ctx = ui.ctx().clone();
        let (mut rolled, mut back, mut play) = (None, false, false);
        let can_go_back = matches!(&self.shown, Some(Shown::Code(b)) if b.code.can_go_back(&ctx, id));
        ui.horizontal_wrapped(|ui| {
            play = ui.button("PLAY").on_hover_text("Plays the sound as it stands (as PLAY SOUND at the top right does).").clicked();
            if ui.add(egui::Button::new("LOOP").selected(self.looping)).on_hover_text("Plays the sound again and again with a short pause, so you hear what the faders and your typing do. Press again to stop.").clicked() {
                self.looping = !self.looping;
                self.again_at = None;
                play |= self.looping;
            }
            ui.add_space(18.0);
            dice(ui).on_hover_text("These roll the dice: each press makes a new sound of that sort, never the same twice. Press one again for another.");
            for sort in maker::Sort::ALL {
                if ui.button(sort.word()).on_hover_text(sort.hover()).clicked() {
                    rolled = Some(sort);
                }
            }
            ui.add_space(18.0);
            back = ui.add_enabled(can_go_back, egui::Button::new("UNDO")).on_hover_text("Back to the sound before the last change (a roll, a fader or typing); again for the one before. Ctrl+Z in the text does the same.").clicked();
        });
        if let Some(sort) = rolled {
            self.roll(&ctx, sort);
        }
        if back {
            self.go_back(&ctx);
        }
        let Some(Shown::Code(buffer)) = &mut self.shown else { return false };
        let now = std::time::Instant::now();
        // The faders, where the text says a sound of numbers.
        ui.add_space(6.0);
        let synth = match &buffer.code.sound {
            Some(SoundDef::Synth(s)) => Some(s.clone()),
            _ => None,
        };
        let fading = &mut self.fading;
        ui.add_enabled_ui(synth.is_some(), |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 22.0;
                for fader in maker::Fader::ALL {
                    let (lo, hi) = fader.range();
                    let mut value = synth.as_ref().map_or(lo, |s| fader.place(s));
                    let held = ui
                        .allocate_ui_with_layout(egui::vec2(FADER_WIDTH, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(fader.word()).on_hover_text(fader.hover());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(fader.shown(value)).small().color(QUIET));
                                });
                            });
                            ui.spacing_mut().slider_width = FADER_WIDTH;
                            ui.add(egui::Slider::new(&mut value, lo..=hi).logarithmic(fader.logarithmic()).show_value(false)).on_hover_text(fader.hover()).on_disabled_hover_text("The faders move a sound made of numbers; this text is not one now (a recording, silent, or a mistake in it).")
                        })
                        .inner;
                    let down = held.is_pointer_button_down_on() || held.dragged();
                    if held.changed()
                        && let Some(s) = &synth
                    {
                        // Marked before the first change of a move, after its last.
                        if *fading != Some(fader) {
                            buffer.code.mark(&ctx, id);
                            *fading = Some(fader);
                        }
                        buffer.code.text = maker::faded(&buffer.code.text, s, fader, value);
                        buffer.code.check();
                        buffer.edited(now);
                    }
                    if *fading == Some(fader) && !down {
                        buffer.code.mark(&ctx, id);
                        *fading = None;
                        play = true;
                    }
                }
            });
        });
        play
    }
}

impl Shell {
    /// The text edit of the file open.
    fn code_id(&self) -> Option<(String, egui::Id)> {
        let path = self.memory.open.clone()?;
        let id = egui::Id::unique(("code", path.as_str()));
        Some((path, id))
    }

    /// A sort's button: the sound open replaced by a fresh one
    /// of `sort` (its leading comments kept), one step back for UNDO, and
    /// played.
    fn roll(&mut self, ctx: &egui::Context, sort: maker::Sort) {
        let Some((_, id)) = self.code_id() else { return };
        let Some(Shown::Code(buffer)) = &mut self.shown else { return };
        self.rolls += 1;
        let clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let fresh = maker::fresh(&buffer.code.text, &maker::roll(sort, clock ^ self.rolls.rotate_left(32)));
        buffer.code.mark(ctx, id);
        buffer.code.text = fresh;
        buffer.code.check();
        buffer.code.mark(ctx, id);
        buffer.edited(std::time::Instant::now());
        self.play_sound();
    }

    /// UNDO: the text a step back, as Ctrl+Z in it, and played.
    fn go_back(&mut self, ctx: &egui::Context) {
        let Some((_, id)) = self.code_id() else { return };
        let Some(Shown::Code(buffer)) = &mut self.shown else { return };
        if buffer.code.go_back(ctx, id) {
            buffer.edited(std::time::Instant::now());
            self.play_sound();
        }
    }
}

/// How wide a fader of the sound maker is.
const FADER_WIDTH: f32 = 150.0;

/// A dice showing five, drawn (no font has one we can count on): the sign
/// that the buttons after it roll.
fn dice(ui: &mut egui::Ui) -> egui::Response {
    let side = ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side + 4.0, side), egui::Sense::hover());
    let face = egui::Rect::from_center_size(rect.center(), egui::vec2(side - 2.0, side - 2.0));
    let ink = ui.visuals().text_color();
    ui.painter().rect_stroke(face, 4.0, egui::Stroke::new(1.5, ink), egui::StrokeKind::Inside);
    let step = face.width() * 0.26;
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (0.0, 0.0), (-1.0, 1.0), (1.0, 1.0)] {
        ui.painter().circle_filled(face.center() + egui::vec2(x * step, y * step), 2.2, ink);
    }
    response
}

/// The pause between a sound's end and its next go under LOOP.
const LOOP_PAUSE: std::time::Duration = std::time::Duration::from_millis(500);

/// COPY, small and quiet: puts `text` on the clipboard.
fn copy_button(ui: &mut egui::Ui, text: &str) {
    let button = ui.small_button("COPY").on_hover_text("Copies the message, to paste to someone who can help.");
    #[cfg(test)]
    COPY_BUTTONS.with(|b| b.borrow_mut().push(button.rect));
    if button.clicked() {
        ui.ctx().copy_text(text.to_string());
    }
}

// Where the COPY buttons were drawn since last taken (for the tests' mouse).
#[cfg(test)]
thread_local! {
    static COPY_BUTTONS: std::cell::RefCell<Vec<egui::Rect>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A mistake in red with COPY at its right end: `copy` is the whole
/// message as it stands alone (the file's name in front).
fn error_row(ui: &mut egui::Ui, shown: &str, copy: &str) {
    // The message takes the room left of COPY and breaks into lines
    // there: a long one cannot push the button out of the window.
    ui.horizontal_top(|ui| {
        let room = (ui.available_width() - 96.0).max(120.0);
        ui.allocate_ui_with_layout(egui::vec2(room, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width(room);
            ui.add(egui::Label::new(RichText::new(shown).color(ERROR)).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(16.0);
            copy_button(ui, copy);
        });
    });
}

/// The red line of what is wrong with the game: the message in the room
/// left of the buttons, breaking into lines there (a long one cannot push
/// them out of the window), TO THE LINE if `line`, SHOW and COPY at the
/// right end. Whether TO THE LINE and SHOW were pressed.
fn game_error_row(ui: &mut egui::Ui, shown: &str, copy: &str, line: bool) -> (bool, bool) {
    let (mut go, mut show) = (false, false);
    ui.horizontal_top(|ui| {
        let room = (ui.available_width() - if line { 290.0 } else { 170.0 }).max(120.0);
        ui.allocate_ui_with_layout(egui::vec2(room, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width(room);
            ui.add(egui::Label::new(RichText::new(shown).color(ERROR)).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(16.0);
            copy_button(ui, copy);
            show = ui.small_button("SHOW").on_hover_text("Shows OUTPUT below, where the whole message is.").clicked();
            if line {
                go = ui.small_button("TO THE LINE").on_hover_text("Opens the file at the line the message names.").clicked();
            }
        });
    });
    (go, show)
}

/// What the console shows where the agent's program was not found: what an
/// agent is, that it is a program of its own with a maker's subscription or
/// credit behind it, where to get it (the address opens in the browser),
/// that the IDE works without one and where the agent is chosen. LOOK
/// AGAIN, pressed after installing it: true.
fn agent_missing(ui: &mut egui::Ui, agent: Agent) -> bool {
    let mut again = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).content_margin(egui::Margin { left: 20, right: 20, top: 12, bottom: 12 }).show(ui, |ui| {
        ui.set_max_width(680.0f32.min(ui.available_width()));
        ui.label(RichText::new("NO AGENT FOUND").color(ACCENT));
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new("An agent is a helper you tell in plain words what your game should do: it writes the files for you. It is a program of its own that has to be installed first, for example Claude Code, which needs a subscription or credit with its maker.").color(TEXT)).wrap());
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("Get {}:", agent.called())).color(TEXT));
            if ui.link(agent.home()).on_hover_text("Opens the page in your browser.").clicked() {
                crate::update::show(agent.home());
            }
        });
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new("The IDE works without one: everything else here needs none.").color(TEXT)).wrap());
        ui.add_space(10.0);
        again = ui.button("LOOK AGAIN").on_hover_text("Looks once more for the agent, after you installed it, and starts it when it is there.").clicked();
    });
    again
}

/// The word saying whether the text shown is saved, small and quiet.
fn saved_label(ui: &mut egui::Ui, saved: Option<&str>) {
    if let Some(word) = saved {
        ui.label(RichText::new(word).small().color(QUIET)).on_hover_text("The text is saved by itself a second after the last change; Ctrl+S saves it at once.");
    }
}

/// Where a sound is used, under its text: USED, then what the game's code
/// says of it (`says`) and each line of it that names it, clicked to open
/// there (the file and the line returned; `rects` where the rows are).
fn used_ui(ui: &mut egui::Ui, name: &str, says: &[String], rows: &[sounds::Use], rects: &mut Vec<egui::Rect>) -> Option<(String, usize)> {
    let mut go = None;
    ui.add_space(18.0);
    ui.label(RichText::new("USED").small().color(QUIET));
    ui.add_space(4.0);
    for said in says {
        ui.label(RichText::new(said).color(QUIET));
    }
    if says.is_empty() && rows.is_empty() {
        ui.label(RichText::new(format!("Nothing in the game plays it yet: level:sound(\"{name}\", player) does.")).color(QUIET));
    }
    for row in rows {
        let shown: String = row.text.chars().take(100).collect();
        let at = ui.horizontal(|ui| {
            let place = ui.add(egui::Label::new(RichText::new(format!("{}:{}", row.file, row.line)).color(ACCENT)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand);
            let text = ui.add(egui::Label::new(RichText::new(shown).text_style(TextStyle::Monospace).color(QUIET)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand);
            place.clicked() || text.clicked()
        });
        rects.push(at.response.rect);
        if at.inner {
            go = Some((row.file.clone(), row.line));
        }
    }
    go
}

/// A picture, its pixels as whole squares, as large as fit.
fn picture(ui: &mut egui::Ui, texture: &egui::TextureHandle) {
    let size = texture.size_vec2();
    let room = ui.available_size();
    let scale = (room.x / size.x).min(room.y / size.y).floor().max(1.0);
    ui.image((texture.id(), size * scale));
}

/// A line of what a file an editor opens is (`level`: the game's level of
/// blocks).
/// Whether a file of the game is a model or a block: Voxcade's `.vvx`, or a
/// MagicaVoxel `.vox` (a world, a map's prefab, or a model not brought in
/// yet), shown as a picture.
pub fn is_model_file(path: &str) -> bool {
    path.ends_with(".vvx") || path.ends_with(".vox")
}

fn facts(file: &Path, path: &str, level: bool) -> String {
    let plural = |n: usize, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
    if level {
        return match crate::grid::GridLevel::read(file) {
            Ok(l) => format!("The level: {} by {} by {} cells of {} voxels, {}, {}.", l.size.x, l.size.y, l.size.z, l.n, plural(l.blocks.len(), "block"), plural(l.things.len(), "thing")),
            Err(e) => format!("{e:#}"),
        };
    }
    if !path.ends_with(".vvx") {
        return "A MagicaVoxel file, shown only (a model to shape here: voxcade import).".into();
    }
    match crate::vox::load_model(file) {
        Ok(m) => model_line(&m, path),
        Err(e) => format!("{e:#}"),
    }
}

/// A line of what a world or a map's prefab is: how big, its colours and
/// markers, and that it is shown only.
fn scene_facts(file: &Path, path: &str) -> String {
    match crate::scene::load(file) {
        Ok(s) => {
            let what = if path.starts_with("prefabs/") { "A prefab of the map" } else { "A world" };
            let markers = match s.markers.len() {
                0 => String::new(),
                n => format!(", {n} marker{} ({})", if n == 1 { "" } else { "s" }, s.markers.join(", ")),
            };
            format!("{what}, {} by {} by {} voxels, {} colours{markers}; shown only.", s.size.x, s.size.y, s.size.z, s.colours.len())
        }
        Err(e) => format!("{e:#}"),
    }
}

/// A line of what a model (`path` in the game folder) is: a block, a
/// figure of parts or frames, and how big.
pub(super) fn model_line(m: &crate::vox::VoxModel, path: &str) -> String {
    let plural = |n: usize, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
    let s = m.size;
    match () {
        _ if path.starts_with("blocks/") => format!("A block of the level, {} by {} by {} voxels.", s.x, s.y, s.z),
        _ if !m.parts.is_empty() => format!("A figure, {} by {} by {} voxels, {}.", s.x, s.y, s.z, plural(m.parts.len(), "part")),
        _ => format!("A model, {} by {} by {} voxels, {}.", s.x, s.y, s.z, plural(m.frames.len(), "frame")),
    }
}

/// A line of what the song in `file` is: its title, how long until it
/// starts over, its channels (or why it cannot be read).
fn song_facts(file: &Path) -> String {
    let song = std::fs::read_to_string(file).map_err(anyhow::Error::from).and_then(|text| crate::audio::music::parse(&text));
    match song {
        Ok(song) => {
            let seconds = (song.length / crate::audio::SAMPLE_RATE as u64) as u32;
            let title = if song.title.is_empty() { String::new() } else { format!("{}, ", song.title) };
            let ending = if song.looping { "starting over" } else { "once" };
            format!("A song: {title}{}:{:02}, {} channels, {ending}.", seconds / 60, seconds % 60, song.names.len())
        }
        Err(e) => format!("The song cannot be read: {e:#}"),
    }
}

/// The file at `file` (`path` in the game folder) as the main area shows
/// it.
fn load(file: &Path, path: &str, ctx: &egui::Context) -> Shown {
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    match ext {
        "png" => match crate::map::read_png_rgba(file) {
            Ok((w, h, pixels)) => {
                let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], pixels.as_flattened());
                Shown::Image(ctx.load_texture(path, image, egui::TextureOptions::NEAREST))
            }
            Err(e) => Shown::Note(format!("{e:#}")),
        },
        // What a model is stands above it (`facts`), and what a sound is.
        "vox" | "vvx" => Shown::Note(String::new()),
        _ => match std::fs::read(file) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => Shown::Text(text),
                Err(_) => Shown::Note("Not text, and no picture of it yet.".into()),
            },
            Err(e) => Shown::Note(format!("{}: {e}", file.display())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ide::console::Start;

    /// A frame of the shell without a window.
    fn frame(ctx: &egui::Context, shell: &mut Shell) {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| shell.ui(ui));
    }

    /// Another file chosen shows in the main area, a script after a
    /// script too (the first is let go of, saved), and focus coming back
    /// keeps a script being changed.
    #[test]
    fn choosing_a_file_shows_it() {
        let dir = Path::new("games/voxkeep");
        let memory = std::env::temp_dir().join(format!("voxcade-shell-choose-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&memory);
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        let text = |shell: &Shell| match &shell.shown {
            Some(Shown::Code(b)) => Some(b.code.text.clone()),
            _ => None,
        };
        assert_eq!(text(&shell), std::fs::read_to_string(dir.join("main.lua")).ok(), "main.lua at first");
        shell.open("conf.lua");
        frame(&ctx, &mut shell);
        assert_eq!(text(&shell), std::fs::read_to_string(dir.join("conf.lua")).ok(), "conf.lua chosen");
        if let Some(Shown::Code(b)) = &mut shell.shown {
            b.code.text.push_str("-- not saved yet\n");
        }
        shell.refresh();
        assert!(text(&shell).is_some_and(|t| t.ends_with("-- not saved yet\n")), "kept when focus comes back");
        // (Not saved: the change goes with the buffer.)
        shell.shown = None;
        shell.open("hero.vvx");
        frame(&ctx, &mut shell);
        assert!(matches!(shell.shown, Some(Shown::Note(_))) && shell.preview.is_some(), "a model: its picture");
        let _ = std::fs::remove_file(&memory);
        let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
    }

    /// A path clicked in the console: a file of the game opens at its line,
    /// a picture from elsewhere shows as a picture and is not remembered as
    /// open, any other file elsewhere opens nothing.
    #[test]
    fn a_clicked_path_opens_in_the_main_area() {
        let dir = Path::new("games/voxkeep");
        let memory = std::env::temp_dir().join(format!("voxcade-shell-clicked-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&memory);
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        let elsewhere = std::env::temp_dir().join(format!("voxcade-shell-clicked-{}", std::process::id()));
        std::fs::create_dir_all(&elsewhere).unwrap();
        let png = elsewhere.join("gras.png");
        std::fs::copy("games/pulsar/ground/height.png", &png).unwrap();
        std::fs::write(elsewhere.join("notes.txt"), "x").unwrap();
        shell.open_from_console(&png, None);
        frame(&ctx, &mut shell);
        assert!(matches!(shell.shown, Some(Shown::Image(_))), "the picture from outside");
        assert_eq!(shell.opened(), Some("main.lua"), "the open file is still the game's");
        shell.save().unwrap();
        assert_eq!(Memory::load(&memory).open.as_deref(), Some("main.lua"), "only the game's files are remembered");
        assert!(shell.state(&ctx)["file"]["path"].as_str().is_some_and(|p| p.ends_with("gras.png")), "the agent sees it");
        shell.open_from_console(&elsewhere.join("notes.txt"), None);
        assert!(shell.outside.is_some(), "another file elsewhere changes nothing");
        // A file of the game: its text, at the line.
        let main = Path::new("games/voxkeep/main.lua").canonicalize().unwrap();
        shell.open_from_console(&main, Some(3));
        frame(&ctx, &mut shell);
        assert!(shell.outside.is_none() && matches!(shell.shown, Some(Shown::Code(_))), "main.lua again");
        assert_eq!(shell.goto, None, "the line was gone to");
        let _ = std::fs::remove_file(&memory);
        let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    /// A text opens with its cursor where it stood last time; the
    /// agent's open leaves the keys where they are, Escape from the game
    /// gives them to the text.
    #[test]
    fn the_cursor_comes_back_and_the_agent_takes_no_keys() {
        let dir = Path::new("games/voxkeep");
        let memory = std::env::temp_dir().join(format!("voxcade-shell-cursor-{}.toml", std::process::id()));
        let mut remembered = Memory { open: Some("main.lua".into()), ..Memory::default() };
        remembered.cursor.insert("main.lua".into(), 42);
        remembered.save(&memory).unwrap();
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
        let ctx = egui::Context::default();
        let id = |path: &str| egui::Id::unique(("code", path));
        let cursor = |ctx: &egui::Context, path: &str| egui::TextEdit::load_state(ctx, id(path)).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0);
        frame(&ctx, &mut shell);
        assert_eq!(cursor(&ctx, "main.lua"), Some(42), "where it stood");
        assert_eq!(ctx.memory(|m| m.focused()), None, "the keys nowhere yet");
        // A model chosen in the list (no text to take the keys), then the
        // agent's open: the wish for the keys did not wait for it.
        shell.open("hero.vvx");
        shell.back_to_file();
        frame(&ctx, &mut shell);
        shell.go_to("conf.lua", Some(2));
        frame(&ctx, &mut shell);
        assert!(cursor(&ctx, "conf.lua").is_some_and(|c| c > 0), "at the line");
        assert_eq!(ctx.memory(|m| m.focused()), None, "the agent's open takes no keys");
        shell.stage = Some(Stage { shown: true, ..Stage::default() });
        frame(&ctx, &mut shell);
        shell.back_to_file();
        frame(&ctx, &mut shell);
        assert_eq!(ctx.memory(|m| m.focused()), Some(id("conf.lua")), "back from the game: the keys to the text");
        shell.save().unwrap();
        assert!(Memory::load(&memory).cursor.get("conf.lua").is_some_and(|&c| c > 0), "remembered");
        let _ = std::fs::remove_file(&memory);
        let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
    }

    /// Another game opens at once while nothing runs in the console (the
    /// text being changed saved first).
    #[test]
    fn another_game_opens_at_once_when_nothing_runs() {
        let dir = Path::new("games/voxkeep");
        let memory = std::env::temp_dir().join(format!("voxcade-shell-leave-{}.toml", std::process::id()));
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
        shell.leave_for(PathBuf::from("games/sokovox"));
        assert!(shell.leaving.is_none(), "nothing to ask");
        assert_eq!(shell.take_asks(), [Ask::Project(Wish::Open(PathBuf::from("games/sokovox")))]);
        let _ = std::fs::remove_file(&memory);
    }

    /// What the game says goes into OUTPUT: what stops it once, with its
    /// line, what it printed; the agent's `errors` names the last errors.
    #[test]
    fn the_game_reports_into_the_output() {
        let dir = Path::new("games/voxkeep");
        let memory = std::env::temp_dir().join(format!("voxcade-shell-output-{}.toml", std::process::id()));
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
        let trouble = Some("main.lua:3: attempt to index a nil value\nstack traceback:".to_string());
        shell.game_said(trouble.clone(), vec![(Kind::Print, "hello".into())]);
        shell.game_said(trouble, Vec::new());
        let errors = shell.output.errors(5);
        assert_eq!(errors.len(), 1, "once");
        assert_eq!(errors[0].at, Some(("main.lua".into(), 3)));
        assert!(shell.output.unseen == 1 && shell.errors().contains("THE GAME STOPPED: main.lua:3"));
        let _ = std::fs::remove_file(&memory);
    }

    /// SHOW THE CONSOLE off leaves the console out altogether; on, it is
    /// back (unfolded, as it was).
    #[test]
    fn the_console_can_be_left_out() {
        let dir = Path::new("games/voxkeep");
        let memory = std::env::temp_dir().join(format!("voxcade-shell-console-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&memory);
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
        let ctx = egui::Context::default();
        let drawn = |ctx: &egui::Context| ctx.viewport(|v| v.prev_pass.widgets.contains(egui::Id::unique("terminal")));
        frame(&ctx, &mut shell);
        assert!(!drawn(&ctx), "folded at first");
        shell.unfold_console();
        frame(&ctx, &mut shell);
        assert!(drawn(&ctx), "there once unfolded");
        shell.prefs.console = false;
        frame(&ctx, &mut shell);
        assert!(!drawn(&ctx), "left out");
        shell.prefs.console = true;
        frame(&ctx, &mut shell);
        assert!(drawn(&ctx), "back");
        let _ = std::fs::remove_file(&memory);
        let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
    }

    /// The line the game stopped at is marked in its file, not in
    /// another, and goes with the error.
    #[test]
    fn the_line_the_game_stopped_at_is_marked() {
        let dir = Path::new("games/voxkeep");
        let memory = std::env::temp_dir().join(format!("voxcade-shell-stopped-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&memory);
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
        let ctx = egui::Context::default();
        shell.stage = Some(Stage { trouble: Some("main.lua:3: boom\nstack traceback:".into()), ..Stage::default() });
        let stopped = |shell: &Shell| match &shell.shown {
            Some(Shown::Code(b)) => b.code.stopped,
            _ => None,
        };
        shell.open("main.lua");
        frame(&ctx, &mut shell);
        assert_eq!(stopped(&shell), Some(3));
        shell.open("conf.lua");
        frame(&ctx, &mut shell);
        assert_eq!(stopped(&shell), None, "not in another file");
        shell.open("main.lua");
        frame(&ctx, &mut shell);
        shell.stage.as_mut().unwrap().trouble = None;
        frame(&ctx, &mut shell);
        assert_eq!(stopped(&shell), None, "mended");
        let _ = std::fs::remove_file(&memory);
        let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
    }

    /// A song says what it is and plays with PLAY; opening another file
    /// or playing the game ends it.
    #[test]
    fn a_song_plays_until_something_else_is_open() {
        let dir = Path::new("games/voxkeep");
        let memory = std::env::temp_dir().join(format!("voxcade-shell-song-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&memory);
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
        let ctx = egui::Context::default();
        let song = "music/under_the_dunes.mml";
        shell.open(song);
        frame(&ctx, &mut shell);
        assert!(shell.facts.as_deref().is_some_and(|f| f.starts_with("A song: ") && f.contains("channels")), "{:?}", shell.facts);
        assert!(shell.song(song).is_ok(), "it reads");
        shell.listen();
        assert_eq!(shell.take_asks(), [Ask::Listen(song.into())]);
        shell.open("main.lua");
        assert_eq!(shell.take_asks(), [Ask::Hush], "another file ends it");
        shell.listen();
        assert!(shell.take_asks().is_empty() && shell.listening.is_none(), "a script does not play");
        shell.open(song);
        shell.listen();
        shell.play();
        assert_eq!(shell.take_asks(), [Ask::Listen(song.into()), Ask::Hush, Ask::Play(None)], "the game ends it");
        let _ = std::fs::remove_file(&memory);
        let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
    }

    /// A bare game folder to try the plus signs on: no model, no song, no
    /// scenario.
    fn bare_game(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("voxcade-shell-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\" }\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick() end\n").unwrap();
        (dir.clone(), std::env::temp_dir().join(format!("voxcade-shell-{name}-{}.toml", std::process::id())))
    }

    fn shell_on(dir: &Path, memory: &Path) -> Shell {
        let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
        Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.to_path_buf(), console)
    }

    /// A frame with `events`, a tenth of a second later than the one before (so that
    /// folding finishes).
    fn frame_with(ctx: &egui::Context, shell: &mut Shell, events: Vec<egui::Event>) {
        let time = ctx.input(|i| i.time) + 0.1;
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), time: Some(time), events, ..Default::default() };
        let _ = ctx.run_ui(raw, |ui| shell.ui(ui));
    }

    /// A click of the left button at `at`, with a frame before it (the
    /// mouse arrives) and after.
    fn click(ctx: &egui::Context, shell: &mut Shell, at: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame_with(ctx, shell, vec![egui::Event::PointerMoved(at)]);
        frame_with(ctx, shell, vec![button(true)]);
        frame_with(ctx, shell, vec![button(false)]);
        for _ in 0..4 {
            frame_with(ctx, shell, vec![]);
        }
    }

    fn click_plus(ctx: &egui::Context, shell: &mut Shell, sort: &str) {
        let at = heading(shell, sort).0.center();
        click(ctx, shell, at);
    }

    fn key(ctx: &egui::Context, shell: &mut Shell, key: egui::Key) {
        let event = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
        frame_with(ctx, shell, vec![event(true)]);
        frame_with(ctx, shell, vec![event(false)]);
    }

    /// The plus at a heading and the heading's row, as drawn last.
    fn heading(shell: &Shell, sort: &str) -> (egui::Rect, egui::Rect) {
        shell.headings.iter().find(|(s, ..)| s == sort).map(|(_, p, h)| (*p, *h)).unwrap_or_else(|| panic!("no heading {sort}: {:?}", shell.headings.iter().map(|h| &h.0).collect::<Vec<_>>()))
    }

    /// A name typed under a heading, with the plus clicked and Enter hit.
    fn type_a_name(ctx: &egui::Context, shell: &mut Shell, sort: &str, name: &str) {
        click_plus(ctx, shell, sort);
        assert!(shell.naming.as_ref().is_some_and(|n| n.sort == sort), "the plus asks for a name under {sort}");
        frame_with(ctx, shell, vec![egui::Event::Text(name.into())]);
        key(ctx, shell, egui::Key::Enter);
    }

    /// A game without models, sounds, songs or scenarios still has the headings
    /// LOGIC, SCENARIOS, MODELS, SOUNDS and MUSIC, each with a plus; LEVELS and
    /// the like only when there are some.
    #[test]
    fn the_headings_with_a_plus_are_always_there() {
        let (dir, memory) = bare_game("headings");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        let sorts: Vec<&str> = shell.headings.iter().map(|(s, ..)| s.as_str()).collect();
        assert_eq!(sorts, ["Logic", "Scenarios", "Models", "Sounds", "Music"]);
        assert_eq!(shell.files.assets.iter().map(|g| (g.name, g.files.len())).collect::<Vec<_>>(), [("Models", 0), ("Sounds", 0), ("Music", 0)]);
        for (sort, plus, head) in &shell.headings {
            assert!(head.contains_rect(*plus), "{sort}: the plus lies on its row");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The plus does not fold its heading and the rest of the row does;
    /// a plus on a folded heading unfolds it.
    #[test]
    fn the_plus_and_the_fold_are_apart() {
        let (dir, memory) = bare_game("fold");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        let below = |shell: &Shell| heading(shell, "Music").1.top();
        let open = below(&shell);
        let (plus, row) = heading(&shell, "Models");
        click(&ctx, &mut shell, egui::pos2(row.left() + 40.0, row.center().y));
        let folded = below(&shell);
        assert!(folded < open, "the row folds it: {folded} < {open}");
        click_plus(&ctx, &mut shell, "Models");
        assert!(shell.naming.as_ref().is_some_and(|n| n.sort == "Models"), "the plus asks for a name");
        assert!(below(&shell) > folded, "and unfolds the heading to make room for it");
        // On an open heading it folds nothing.
        let before = below(&shell);
        click_plus(&ctx, &mut shell, "Models");
        assert!(below(&shell) >= before, "an open heading stays open");
        let _ = plus;
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each plus makes its file under the name typed, it is listed at once and open; a name taken is
    /// refused, with the old file left; Escape lets go of the field.
    #[test]
    fn a_plus_makes_a_file_of_its_sort() {
        let (dir, memory) = bare_game("make");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        for (sort, typed, path) in [("Logic", "Market Stall", "market_stall.lua"), ("Scenarios", "late", "scenarios/late.lua"), ("Models", "Hero", "hero.vvx"), ("Sounds", "Coin Pick", "sounds/coin_pick.toml"), ("Music", "theme.mml", "music/theme.mml")] {
            type_a_name(&ctx, &mut shell, sort, typed);
            assert!(dir.join(path).is_file(), "{path} made");
            assert!(shell.naming.is_none(), "{path}: the field is gone");
            assert!(shell.files.has(path), "{path} listed at once");
            assert_eq!(shell.memory.open.as_deref(), Some(path), "{path} open");
            frame(&ctx, &mut shell);
        }
        assert_eq!(std::fs::read_to_string(dir.join("scenarios/late.lua")).unwrap(), NEW_SCENARIO);
        // The sound reads, says what it is and how to play it.
        let blip = std::fs::read_to_string(dir.join("sounds/coin_pick.toml")).unwrap();
        assert!(blip.starts_with("# coin_pick: ") && blip.contains("level:sound(\"coin_pick\", player)"), "{blip}");
        assert!(matches!(sound::read("sounds/coin_pick.toml", &blip), Ok(SoundDef::Synth(_))));
        let lua = std::fs::read_to_string(dir.join("market_stall.lua")).unwrap();
        assert!(lua.contains("local market_stall = require(\"market_stall\")") && lua.contains("return market_stall"), "{lua}");
        // A name taken is refused, the file stays.
        std::fs::write(dir.join("hero.vvx"), "mine").unwrap();
        type_a_name(&ctx, &mut shell, "Models", "HERO");
        assert_eq!(std::fs::read(dir.join("hero.vvx")).unwrap(), b"mine");
        assert_eq!(shell.naming.as_ref().and_then(|n| n.problem.as_deref()), Some("hero.vvx is already there."));
        frame(&ctx, &mut shell);
        key(&ctx, &mut shell, egui::Key::Escape);
        assert!(shell.naming.is_none(), "Escape cancels");
        // Nothing, or a sign a name cannot have.
        assert_eq!(shell.make_file("Logic", "  "), Err("Type a name first.".into()));
        assert_eq!(shell.make_file("Logic", "a/b"), Err("A name takes letters, digits, _ and - only.".into()));
        assert_eq!(shell.make_file("Logic", "main"), Err("main.lua is already there.".into()));
        assert_eq!(std::fs::read_to_string(dir.join("main.lua")).unwrap(), "function on_tick() end\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An empty field loses focus: it goes away.
    #[test]
    fn an_empty_name_field_goes_away_when_left() {
        let (dir, memory) = bare_game("leave");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        click_plus(&ctx, &mut shell, "Music");
        assert!(shell.naming.is_some());
        click(&ctx, &mut shell, egui::pos2(700.0, 400.0));
        assert!(shell.naming.is_none(), "left empty");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What the plus makes works: the model loads and draws, the song
    /// parses, the Lua file can be required by the game's main.lua.
    #[test]
    fn what_a_plus_makes_works() {
        let (dir, memory) = bare_game("works");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        for (sort, name) in [("Logic", "tools"), ("Models", "blob"), ("Music", "tune")] {
            shell.make_file(sort, name).unwrap();
        }
        let model = crate::vox::load_model(&dir.join("blob.vvx")).unwrap();
        assert!(model.frames.len() == 1 && model.frames[0].len() == 64 && model.size == glam::IVec3::splat(8), "{:?} {}", model.size, model.frames[0].len());
        shell.open("blob.vvx");
        frame(&ctx, &mut shell);
        assert!(shell.preview.is_some(), "it shows as a picture: {:?}", shell.message);
        let song = crate::audio::music::parse(&std::fs::read_to_string(dir.join("music/tune.mml")).unwrap()).unwrap();
        assert!(song.names.len() == 2 && song.length > 0 && song.channel_lengths().iter().all(|(_, l)| (*l - song.channel_lengths()[0].1).abs() < 0.01), "{:?}", song.channel_lengths());
        std::fs::write(dir.join("main.lua"), "local tools = require(\"tools\")\nfunction on_world_start() world.state.said = tools.hello() end\n").unwrap();
        use crate::game::Game;
        let mut game = crate::session::Session::<crate::Voxcade>::load(&dir).unwrap();
        game.step(&crate::input::Input::default());
        assert!(game.game.halted().is_none(), "{:?}", game.game.halted());
        assert!(game.game.inspect().to_string().contains("hello from tools"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A game with sounds of its own: `conf.lua` names "coin" in a kind,
    /// `main.lua` plays it, `bell` is never played.
    fn sound_game(name: &str) -> (PathBuf, PathBuf) {
        let (dir, memory) = bare_game(name);
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\" }\n  c.things.gem = { sound = \"coin\" }\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  if world.tick == 5 then\n    level:sound(\"coin\", player)\n  end\nend\n").unwrap();
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        std::fs::write(dir.join("sounds/coin.toml"), "# a coin\nwave = \"triangle\"\nfreq = 880\nhold = 0.04\ndecay = 0.16\nsweep = -12\n").unwrap();
        std::fs::write(dir.join("sounds/bell.toml"), "wave = \"sine\"\n").unwrap();
        (dir, memory)
    }

    /// The text open takes `text` typed at its end, as a person types.
    fn type_into_text(ctx: &egui::Context, shell: &mut Shell, text: &str) {
        shell.back_to_file();
        frame_with(ctx, shell, vec![]);
        frame_with(ctx, shell, vec![]);
        frame_with(ctx, shell, vec![egui::Event::Text(text.into())]);
        frame_with(ctx, shell, vec![]);
    }

    fn buffer(shell: &Shell) -> &Buffer {
        match &shell.shown {
            Some(Shown::Code(b)) => b,
            _ => panic!("no text open"),
        }
    }

    /// SOUNDS is always listed, the game's sounds in it and none under
    /// OTHER; the sounds the game brings but has no file for follow, built
    /// in, and a file of the same name takes one off the list.
    #[test]
    fn sounds_have_a_group_of_their_own() {
        let (dir, memory) = sound_game("group");
        let mut shell = shell_on(&dir, &memory).with_sounds(crate::audio::core_sounds());
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        let groups: Vec<(&str, Vec<&str>)> = shell.files.assets.iter().map(|g| (g.name, g.files.iter().map(String::as_str).collect())).collect();
        assert!(groups.contains(&("Sounds", vec!["sounds/bell.toml", "sounds/coin.toml"])), "{groups:?}");
        assert!(groups.iter().all(|(n, f)| *n == "Sounds" || !f.iter().any(|f| f.starts_with("sounds/"))), "none under OTHER: {groups:?}");
        // The built-in ones: those of the core, none of them replaced by a file.
        std::fs::write(dir.join("sounds/impact.toml"), "wave = \"noise\"\n").unwrap();
        shell.files = Files::scan(&dir);
        let listed: Vec<&str> = shell.brought.iter().map(|(n, _)| *n).filter(|n| !shell.files.has(&format!("sounds/{n}.toml"))).collect();
        assert_eq!(listed, ["explosion", "crumble"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A built-in sound opens read-only in the main area with its text;
    /// MAKE IT YOURS writes it as the game's own file, which then shows
    /// as one of the game's.
    #[test]
    fn a_builtin_sound_is_made_yours() {
        let (dir, memory) = sound_game("yours");
        let mut shell = shell_on(&dir, &memory).with_sounds(crate::audio::core_sounds());
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.builtin = Some("explosion");
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(shell.used.as_ref().is_some_and(|u| u.name == "explosion" && u.rows.is_empty()));
        assert!(shell.rules_say("explosion")[0].starts_with("Played by the engine itself"));
        assert!(!dir.join("sounds/explosion.toml").exists(), "looking writes nothing");
        shell.make_it_yours("explosion");
        let text = std::fs::read_to_string(dir.join("sounds/explosion.toml")).unwrap();
        assert!(text.starts_with("# explosion: ") && sound::read("explosion.toml", &text).is_ok(), "{text}");
        assert!(shell.builtin.is_none() && shell.files.has("sounds/explosion.toml") && shell.memory.open.as_deref() == Some("sounds/explosion.toml"));
        frame(&ctx, &mut shell);
        assert!(matches!(shell.shown, Some(Shown::Code(_))), "now it is text to change");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sound says what it is in words; a mistake typed into it shows its
    /// line and marks it, mended it clears.
    #[test]
    fn a_sound_is_text_with_a_line_of_words_and_its_mistakes() {
        let (dir, memory) = sound_game("text");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.open("sounds/coin.toml");
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert_eq!(shell.sound_line.as_ref().map(|(_, l)| l.as_str()), Some("A sound: 0.20 s, a triangle wave falling an octave."));
        // A mistake on a line of its own, typed at the end.
        type_into_text(&ctx, &mut shell, "frq = 1\n");
        let error = buffer(&shell).code.error.clone();
        assert!(error.as_ref().is_some_and(|(line, why)| *line == 7 && why.starts_with("frq: no such field")), "{error:?}");
        assert_eq!(shell.sound_line.as_ref().map(|(_, l)| l.as_str()), Some("The sound cannot be read."));
        assert!(shell.errors().starts_with("sounds/coin.toml:7: frq: no such field"), "{}", shell.errors());
        // Mended: the line gone.
        if let Some(Shown::Code(b)) = &mut shell.shown {
            b.code.text = b.code.text.replace("frq = 1\n", "");
        }
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(buffer(&shell).code.error.is_none());
        assert!(shell.sound_line.as_ref().is_some_and(|(_, l)| l.starts_with("A sound: 0.20 s")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// USED lists the lines of the game's scripts that name the sound, in
    /// quotes, and a click opens the file there; a sound nothing names
    /// says so; PLAY SOUND renders the text as it stands.
    #[test]
    fn a_sound_shows_where_it_is_used() {
        let (dir, memory) = sound_game("used");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.open("sounds/coin.toml");
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        let rows: Vec<(String, usize, String)> = shell.used.as_ref().unwrap().rows.iter().map(|u| (u.file.clone(), u.line, u.text.clone())).collect();
        assert_eq!(rows, [("conf.lua".to_string(), 5, "c.things.gem = { sound = \"coin\" }".to_string()), ("main.lua".to_string(), 3, "level:sound(\"coin\", player)".to_string())]);
        assert_eq!(shell.used_rows.len(), 2, "two rows drawn");
        // A click on the second opens main.lua at its line.
        let second = shell.used_rows[1].center();
        click(&ctx, &mut shell, second);
        assert_eq!(shell.memory.open.as_deref(), Some("main.lua"));
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        let cursor = egui::TextEdit::load_state(&ctx, egui::Id::unique(("code", "main.lua"))).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0);
        let text = &buffer(&shell).code.text;
        let line = text[..text.char_indices().nth(cursor.unwrap_or(0)).map_or(text.len(), |(b, _)| b)].matches('\n').count() + 1;
        assert_eq!(line, 3, "its cursor on the line");
        // Nothing names the bell.
        shell.open("sounds/bell.toml");
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(shell.used.as_ref().is_some_and(|u| u.name == "bell" && u.rows.is_empty()) && shell.used_rows.is_empty());
        // PLAY SOUND asks the speaker for the text as it stands.
        type_into_text(&ctx, &mut shell, "# louder\n");
        shell.play_sound();
        assert_eq!(shell.take_asks(), [Ask::Sound]);
        let (samples, volume) = shell.heard().expect("rendered");
        assert!(!samples.is_empty() && volume > 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The sound maker: a sort's button replaces the sound with a fresh one
    /// of its sort (the leading comment kept) and plays it; UNDO goes back
    /// over the rolls one by one, and Ctrl+Z in the text over the same
    /// steps; what the maker made is saved as typing is.
    #[test]
    fn the_maker_rolls_and_goes_back() {
        let (dir, memory) = sound_game("maker");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.open("sounds/coin.toml");
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        let text = |shell: &Shell| buffer(shell).code.text.clone();
        let first = text(&shell);
        shell.roll(&ctx, maker::Sort::Blast);
        let blast = text(&shell);
        assert!(blast.starts_with("# a coin\nwave = \"noise\"\n"), "{blast}");
        assert!(matches!(buffer(&shell).code.sound, Some(SoundDef::Synth(_))) && buffer(&shell).code.error.is_none());
        assert_eq!(shell.take_asks(), [Ask::Sound], "played at once");
        assert!(shell.heard().is_some());
        assert_eq!(shell.saved_word(), Some("NOT SAVED YET"));
        frame(&ctx, &mut shell);
        shell.roll(&ctx, maker::Sort::Coin);
        frame(&ctx, &mut shell);
        assert!(text(&shell) != blast && text(&shell).starts_with("# a coin\n"));
        // UNDO: the blast, then the sound as it was.
        shell.go_back(&ctx);
        assert_eq!(text(&shell), blast);
        frame(&ctx, &mut shell);
        shell.go_back(&ctx);
        assert_eq!(text(&shell), first);
        assert_eq!(shell.take_asks(), [Ask::Sound, Ask::Sound, Ask::Sound]);
        // LOOP: the sound again once it has sounded and its pause is over,
        // until another file is opened.
        (shell.looping, shell.again_at) = (true, None);
        let now = std::time::Instant::now();
        shell.tick(now);
        assert_eq!(shell.take_asks(), [Ask::Sound], "at once");
        shell.tick(now);
        assert_eq!(shell.take_asks(), [], "not before it has sounded");
        let (_, next) = shell.tick(now);
        assert!(next <= std::time::Duration::from_secs(3), "the window wakes for it: {next:?}");
        shell.tick(now + std::time::Duration::from_secs(5));
        assert_eq!(shell.take_asks(), [Ask::Sound], "again");
        shell.hush();
        shell.tick(now + std::time::Duration::from_secs(20));
        assert_eq!(shell.take_asks(), [], "switched off");
        // Ctrl+Z in the text goes back over a roll too.
        shell.roll(&ctx, maker::Sort::Jump);
        assert!(text(&shell).contains("sweep = "));
        shell.back_to_file();
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        let ctrl_z = |pressed| egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND };
        frame_with(&ctx, &mut shell, vec![ctrl_z(true)]);
        frame_with(&ctx, &mut shell, vec![ctrl_z(false)]);
        assert_eq!(text(&shell), first, "Ctrl+Z");
        // Saved as typing is.
        shell.roll(&ctx, maker::Sort::Hit);
        shell.tick(std::time::Instant::now() + super::super::buffer::SAVE_AFTER);
        assert_eq!(std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap(), text(&shell));
        assert!(sound::read("sounds/coin.toml", &text(&shell)).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A song's text is changed and saved; a mistake in it shows its line
    /// and PLAY SONG takes the text as it stands.
    #[test]
    fn a_song_is_text_to_change() {
        let (dir, memory) = bare_game("songtext");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.make_file("Music", "tune").unwrap();
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(matches!(shell.shown, Some(Shown::Code(_))), "a song is text to change");
        type_into_text(&ctx, &mut shell, "\n#bogus 1\n");
        let error = buffer(&shell).code.error.clone();
        assert!(error.as_ref().is_some_and(|(_, why)| why.contains("unknown #bogus")), "{error:?}");
        let line = error.unwrap().0;
        assert_eq!(buffer(&shell).code.text.lines().nth(line - 1), Some("#bogus 1"), "the line is the one with the mistake");
        // Saved by the autosave, and PLAY SONG reads the text as it stands.
        assert!(shell.song("music/tune.mml").is_err(), "the text as it stands: it cannot be read now");
        shell.tick(std::time::Instant::now() + super::super::buffer::SAVE_AFTER);
        assert!(std::fs::read_to_string(dir.join("music/tune.mml")).unwrap().contains("#bogus 1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// SAVED while the file holds what is shown, NOT SAVED YET between a
    /// change and the autosave; Ctrl+S saves at once.
    #[test]
    fn the_text_says_whether_it_is_saved() {
        let (dir, memory) = sound_game("saved");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.open("sounds/coin.toml");
        frame(&ctx, &mut shell);
        assert_eq!(shell.saved_word(), Some("SAVED"));
        type_into_text(&ctx, &mut shell, "# one\n");
        assert_eq!(shell.saved_word(), Some("NOT SAVED YET"));
        assert!(!std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap().contains("# one"));
        shell.tick(std::time::Instant::now() + super::super::buffer::SAVE_AFTER);
        assert_eq!(shell.saved_word(), Some("SAVED"), "the autosave");
        assert!(std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap().contains("# one"));
        // Ctrl+S at once.
        type_into_text(&ctx, &mut shell, "# two\n");
        assert_eq!(shell.saved_word(), Some("NOT SAVED YET"));
        let ctrl_s = |pressed| egui::Event::Key { key: egui::Key::S, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND };
        frame_with(&ctx, &mut shell, vec![ctrl_s(true)]);
        frame_with(&ctx, &mut shell, vec![ctrl_s(false)]);
        assert_eq!(shell.saved_word(), Some("SAVED"));
        assert!(std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap().contains("# two"), "written at once");
        // A script says it too.
        shell.open("main.lua");
        frame(&ctx, &mut shell);
        assert_eq!(shell.saved_word(), Some("SAVED"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A game of every sort of file to rename: a module main.lua requires, a
    /// scenario, a model, a song, an image and a text, and sounds as
    /// `sound_game` has them. The folder a deleted file goes to is in the
    /// temp dir too.
    fn managed_game(name: &str) -> (PathBuf, PathBuf, Shell) {
        let (dir, memory) = sound_game(name);
        std::fs::write(dir.join("tools.lua"), "return {}\n").unwrap();
        std::fs::create_dir_all(dir.join("scenarios")).unwrap();
        std::fs::write(dir.join("scenarios/late.lua"), NEW_SCENARIO).unwrap();
        crate::editor::frames::ModelFile::begun(&dir.join("hero.vvx")).save().unwrap();
        std::fs::create_dir_all(dir.join("music")).unwrap();
        std::fs::write(dir.join("music/tune.mml"), NEW_SONG.replace("NAME", "tune")).unwrap();
        std::fs::write(dir.join("back.png"), "not a picture").unwrap();
        std::fs::write(dir.join("notes.txt"), "remember").unwrap();
        let mut shell = shell_on(&dir, &memory);
        shell.deleted_dir = dir.with_extension("deleted");
        let _ = std::fs::remove_dir_all(&shell.deleted_dir);
        (dir, memory, shell)
    }

    fn clean_up(dir: &Path, shell: &Shell) {
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(&shell.deleted_dir);
        let _ = std::fs::remove_dir_all(shell.memory_path.with_extension("journal"));
    }

    /// Every sort of file is renamed under the name typed, its ending kept,
    /// the list read anew; where nothing names it nothing is asked.
    #[test]
    fn each_sort_of_file_is_renamed() {
        let (dir, _, mut shell) = managed_game("rename-sorts");
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        for (from, typed, to) in [
            ("tools.lua", "Kit", "kit.lua"),
            ("scenarios/late.lua", "soon", "scenarios/soon.lua"),
            ("hero.vvx", "knight.vvx", "knight.vvx"),
            ("sounds/bell.toml", "chime", "sounds/chime.toml"),
            ("music/tune.mml", "song", "music/song.mml"),
            ("back.png", "sky", "sky.png"),
            ("notes.txt", "Read Me", "Read_Me.txt"),
        ] {
            let bytes = std::fs::read(dir.join(from)).unwrap();
            assert_eq!(shell.rename(from, typed), Ok(()), "{from}");
            assert!(shell.asking.is_none(), "{from}: nothing names it, nothing is asked");
            assert!(!dir.join(from).exists(), "{from} gone");
            assert_eq!(std::fs::read(dir.join(to)).unwrap(), bytes, "{to} has its bytes");
            assert!(shell.files.has(to) && !shell.files.has(from), "{to} listed anew");
            frame(&ctx, &mut shell);
        }
        let _ = std::fs::remove_dir_all(&dir);
        clean_up(&dir, &shell);
    }

    /// The open file stays open under its new name with its cursor and its
    /// scroll; a text not saved yet is saved first, and no journal is left
    /// under the old name.
    #[test]
    fn the_open_file_stays_open_when_renamed() {
        let (dir, _, mut shell) = managed_game("rename-open");
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.open("tools.lua");
        frame(&ctx, &mut shell);
        type_into_text(&ctx, &mut shell, "-- typed\n");
        assert!(buffer(&shell).unsaved());
        let journal = shell.journals().join("tools.lua");
        assert!(journal.exists(), "the text is in the journal");
        shell.memory.cursor.insert("tools.lua".into(), 4);
        shell.memory.scroll.insert("tools.lua".into(), 30.0);
        assert_eq!(shell.rename("tools.lua", "kit"), Ok(()));
        assert!(!journal.exists() && !shell.journals().join("kit.lua").exists(), "no journal left behind");
        assert!(std::fs::read_to_string(dir.join("kit.lua")).unwrap().ends_with("-- typed\n"), "saved first");
        assert_eq!(shell.memory.open.as_deref(), Some("kit.lua"));
        assert_eq!((shell.memory.cursor.get("kit.lua"), shell.memory.scroll.get("kit.lua")), (Some(&4), Some(&30.0)));
        assert!(!shell.memory.cursor.contains_key("tools.lua") && !shell.memory.scroll.contains_key("tools.lua"));
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(buffer(&shell).code.text.ends_with("-- typed\n") && !buffer(&shell).unsaved(), "shown under the new name");
        // The IDE's memory on disk says so too.
        assert_eq!(Memory::load(&shell.memory_path).open.as_deref(), Some("kit.lua"));
        clean_up(&dir, &shell);
    }

    /// main.lua and conf.lua keep their names and stay; a name taken is
    /// refused, the files left; levels and blocks are not yet.
    #[test]
    fn what_the_game_needs_is_not_renamed_or_deleted() {
        let (dir, _, mut shell) = managed_game("rename-refused");
        std::fs::write(dir.join("level.toml"), "{}").unwrap();
        shell.files = Files::scan(&dir);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        for file in ["main.lua", "conf.lua"] {
            let err = shell.rename(file, "other").unwrap_err();
            assert!(err.contains("needs main.lua and conf.lua"), "{err}");
            shell.delete(file);
            assert!(dir.join(file).is_file() && shell.gone.is_none(), "{file} stays");
            shell.begin_rename(file);
            assert!(shell.renaming.is_none());
        }
        assert!(shell.rename("level.toml", "other").unwrap_err().starts_with("Not yet"));
        shell.delete("level.toml");
        assert!(dir.join("level.toml").is_file() && shell.gone.is_none());
        assert_eq!(shell.rename("sounds/coin.toml", "bell"), Err("sounds/bell.toml is already there.".into()));
        assert_eq!(shell.rename("tools.lua", "a/b"), Err("A name cannot hold a folder: no / in it.".into()));
        assert_eq!(shell.rename("tools.lua", " "), Err("Type a name first.".into()));
        assert!(dir.join("sounds/coin.toml").is_file() && dir.join("tools.lua").is_file());
        // The disabled buttons say why (the open file is main.lua).
        frame(&ctx, &mut shell);
        clean_up(&dir, &shell);
    }

    /// The lines naming a sound are asked about: CANCEL changes nothing,
    /// ONLY THE FILE renames the file alone, CHANGE THEM changes the
    /// lines too, also in a text open that has changes of its own.
    #[test]
    fn the_lines_naming_a_sound_are_asked_about() {
        use manage::Answer;
        let (dir, _, mut shell) = managed_game("rename-asks");
        let (conf, main) = (std::fs::read_to_string(dir.join("conf.lua")).unwrap(), std::fs::read_to_string(dir.join("main.lua")).unwrap());
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        assert_eq!(shell.rename("sounds/coin.toml", "ring"), Ok(()));
        let q = shell.asking.as_ref().expect("asked");
        assert_eq!((q.rows.len(), q.sentence().as_str()), (2, "2 lines of your game play this sound by its name. Change them too?"));
        frame(&ctx, &mut shell);
        assert!(dir.join("sounds/coin.toml").exists(), "nothing done before the answer");
        shell.answer(Answer::Cancel);
        assert!(shell.asking.is_none() && dir.join("sounds/coin.toml").exists() && !dir.join("sounds/ring.toml").exists());
        // ONLY THE FILE.
        assert_eq!(shell.rename("sounds/coin.toml", "ring"), Ok(()));
        shell.answer(Answer::OnlyTheFile);
        assert!(dir.join("sounds/ring.toml").is_file() && !dir.join("sounds/coin.toml").exists());
        assert_eq!((std::fs::read_to_string(dir.join("conf.lua")).unwrap(), std::fs::read_to_string(dir.join("main.lua")).unwrap()), (conf.clone(), main.clone()), "the lines as they were");
        // CHANGE THEM, main.lua open with a change not saved yet.
        shell.rename("sounds/ring.toml", "coin").unwrap();
        shell.answer(Answer::OnlyTheFile);
        shell.open("main.lua");
        frame(&ctx, &mut shell);
        type_into_text(&ctx, &mut shell, "-- mine\n");
        assert_eq!(shell.rename("sounds/coin.toml", "ring"), Ok(()));
        shell.answer(Answer::Change);
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        shell.tick(std::time::Instant::now() + super::super::buffer::SAVE_AFTER);
        assert_eq!(std::fs::read_to_string(dir.join("conf.lua")).unwrap(), conf.replace("\"coin\"", "\"ring\""));
        let main_now = std::fs::read_to_string(dir.join("main.lua")).unwrap();
        assert_eq!(main_now, format!("{}-- mine\n", main.replace("\"coin\"", "\"ring\"")), "the change and the new name both, on disk");
        assert_eq!(buffer(&shell).code.text, main_now, "the open text took it in");
        assert!(matches!(&shell.notice, Some(Notice::Said(n)) if n.starts_with("RENAMED coin.toml TO ring.toml")));
        // A module, a model and a song are asked about by their own lines.
        std::fs::write(dir.join("main.lua"), "local tools = require \"tools\"\nm = \"hero.vvx\"\nplay(\"music/tune.mml\")\n").unwrap();
        shell.files = Files::scan(&dir);
        shell.unload();
        for (from, typed, line) in [("tools.lua", "kit", "require \"kit\""), ("hero.vvx", "knight", "m = \"knight.vvx\""), ("music/tune.mml", "song", "play(\"music/song.mml\")")] {
            assert_eq!(shell.rename(from, typed), Ok(()));
            assert_eq!(shell.asking.as_ref().map(|q| q.rows.len()), Some(1), "{from}");
            shell.answer(Answer::Change);
            assert!(std::fs::read_to_string(dir.join("main.lua")).unwrap().contains(line), "{line}");
        }
        clean_up(&dir, &shell);
    }

    /// DELETE asks nothing: the file goes away, the notice says what still
    /// names it; the file open before it takes the main area; UNDO puts the
    /// same bytes back and opens it again.
    #[test]
    fn a_deleted_file_can_be_put_back() {
        let (dir, _, mut shell) = managed_game("delete-undo");
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.open("main.lua");
        shell.open("sounds/coin.toml");
        frame(&ctx, &mut shell);
        let bytes = std::fs::read(dir.join("sounds/coin.toml")).unwrap();
        shell.delete("sounds/coin.toml");
        assert!(!dir.join("sounds/coin.toml").exists() && !shell.files.has("sounds/coin.toml"));
        let gone = shell.gone.as_ref().expect("a notice with UNDO");
        assert_eq!(gone.said, "DELETED coin.toml · conf.lua and main.lua still play it in 2 lines");
        assert!(gone.can_undo() && gone.until > std::time::Instant::now() + std::time::Duration::from_secs(6));
        assert_eq!(std::fs::read(shell.deleted_dir.join("sounds/coin.toml")).unwrap(), bytes, "kept where it went");
        assert_eq!(shell.memory.open.as_deref(), Some("main.lua"), "the file open before it");
        frame(&ctx, &mut shell);
        assert!(matches!(shell.shown, Some(Shown::Code(_))));
        // UNDO.
        shell.undo();
        assert_eq!(std::fs::read(dir.join("sounds/coin.toml")).unwrap(), bytes);
        assert!(shell.gone.is_none() && shell.files.has("sounds/coin.toml") && !shell.deleted_dir.join("sounds/coin.toml").exists());
        assert_eq!(shell.memory.open.as_deref(), Some("sounds/coin.toml"), "open again");
        frame(&ctx, &mut shell);
        assert!(buffer(&shell).code.sound.is_some());
        // A file nothing names says nothing more; one not open leaves the open one be.
        shell.delete("sounds/bell.toml");
        assert_eq!(shell.gone.as_ref().map(|g| g.said.as_str()), Some("DELETED bell.toml"));
        assert_eq!(shell.memory.open.as_deref(), Some("sounds/coin.toml"));
        // The notice goes after its time, with the bytes.
        shell.gone.as_mut().unwrap().until = std::time::Instant::now();
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(shell.gone.is_none(), "gone after its time");
        clean_up(&dir, &shell);
    }

    /// The open file deleted with none before it leaves nothing open; the
    /// UNDO button is on the bar and a click on it puts the file back; a
    /// journal of the file is gone with it.
    #[test]
    fn deleting_the_only_open_file_leaves_the_area_empty() {
        let (dir, _, mut shell) = managed_game("delete-open");
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.open("tools.lua");
        frame(&ctx, &mut shell);
        type_into_text(&ctx, &mut shell, "-- typed\n");
        shell.previous = None;
        shell.delete("tools.lua");
        assert!(!shell.journals().join("tools.lua").exists(), "no journal left");
        assert_eq!(shell.memory.open, None);
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(shell.shown.is_none(), "nothing shown");
        // What was saved on the way out is what comes back.
        shell.undo();
        assert!(std::fs::read_to_string(dir.join("tools.lua")).unwrap().ends_with("-- typed\n"));
        assert_eq!(shell.memory.open.as_deref(), Some("tools.lua"));
        // The same through the button.
        shell.delete("notes.txt");
        frame(&ctx, &mut shell);
        let undo = shell.undo_button.expect("UNDO drawn on the bar");
        click(&ctx, &mut shell, undo.center());
        assert!(dir.join("notes.txt").is_file() && shell.gone.is_none(), "UNDO clicked");
        clean_up(&dir, &shell);
    }

    /// A name typed in place in the list: the field takes the name (the
    /// ending is beside it, not typed), Enter takes it, Escape leaves it, a
    /// name that cannot be is said in plain words and the field stays.
    #[test]
    fn a_name_is_typed_in_place_in_the_list() {
        let (dir, _, mut shell) = managed_game("rename-field");
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        shell.begin_rename("sounds/bell.toml");
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(shell.renaming.as_ref().is_some_and(|r| r.text == "bell" && !r.fresh));
        frame_with(&ctx, &mut shell, vec![egui::Event::Text("coin".into())]);
        key(&ctx, &mut shell, egui::Key::Enter);
        assert_eq!(shell.renaming.as_ref().and_then(|r| r.problem.as_deref()), Some("sounds/coin.toml is already there."));
        frame_with(&ctx, &mut shell, vec![egui::Event::Text("x".into())]);
        key(&ctx, &mut shell, egui::Key::Escape);
        assert!(shell.renaming.is_none() && dir.join("sounds/bell.toml").is_file(), "Escape leaves it");
        // Enter takes it (the old name is selected: typing replaces it).
        shell.begin_rename("sounds/bell.toml");
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        frame_with(&ctx, &mut shell, vec![egui::Event::Text("Chime".into())]);
        key(&ctx, &mut shell, egui::Key::Enter);
        assert!(shell.renaming.is_none() && dir.join("sounds/chime.toml").is_file() && !dir.join("sounds/bell.toml").exists());
        // A right click on a row opens a menu (RENAME, DELETE).
        let at = egui::pos2(60.0, heading(&shell, "Logic").1.bottom() + 16.0);
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE };
        frame_with(&ctx, &mut shell, vec![egui::Event::PointerMoved(at)]);
        frame_with(&ctx, &mut shell, vec![button(true)]);
        frame_with(&ctx, &mut shell, vec![button(false)]);
        frame_with(&ctx, &mut shell, vec![]);
        assert!(egui::Popup::is_any_open(&ctx), "the menu is open");
        clean_up(&dir, &shell);
    }

    /// A frame with `events`; what the frame asked to be copied.
    fn copied_in(ctx: &egui::Context, shell: &mut Shell, events: Vec<egui::Event>) -> Vec<String> {
        let time = ctx.input(|i| i.time) + 0.1;
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), time: Some(time), events, ..Default::default() };
        let out = ctx.run_ui(raw, |ui| shell.ui(ui));
        out.platform_output.commands.iter().filter_map(|c| if let egui::OutputCommand::CopyText(t) = c { Some(t.clone()) } else { None }).collect()
    }

    /// COPY at a mistake's right end copies it with the file's name in
    /// front; COPY ALL over OUTPUT copies every line.
    #[test]
    fn a_mistake_and_the_output_can_be_copied() {
        let (dir, memory) = sound_game("copy");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.open("sounds/coin.toml");
        frame(&ctx, &mut shell);
        type_into_text(&ctx, &mut shell, "frq = 1\n");
        COPY_BUTTONS.with(|b| b.borrow_mut().clear());
        frame(&ctx, &mut shell);
        let at = COPY_BUTTONS.with(|b| b.borrow().first().copied()).expect("a COPY button at the mistake").center();
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        copied_in(&ctx, &mut shell, vec![egui::Event::PointerMoved(at)]);
        copied_in(&ctx, &mut shell, vec![button(true)]);
        let copied = copied_in(&ctx, &mut shell, vec![button(false)]);
        assert_eq!(copied, ["sounds/coin.toml line 7: frq: no such field (did you mean freq?)"]);
        // OUTPUT's COPY ALL: the lines in order.
        shell.output.add(Kind::Note, "one", None, false);
        shell.output.add(Kind::Error, "two", None, false);
        shell.output_shown = true;
        frame(&ctx, &mut shell);
        frame(&ctx, &mut shell);
        assert!(shell.output.text().ends_with("one\ntwo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A frame with `events` and what it drew: the texts, and where (the
    /// middle of each).
    fn drawn_text(ctx: &egui::Context, shell: &mut Shell, events: Vec<egui::Event>) -> Vec<(String, egui::Pos2)> {
        let time = ctx.input(|i| i.time) + 0.1;
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), time: Some(time), events, ..Default::default() };
        let out = ctx.run_ui(raw, |ui| shell.ui(ui));
        out.shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.pos + t.galley.rect.center().to_vec2())),
                _ => None,
            })
            .collect()
    }

    fn says(texts: &[(String, egui::Pos2)], word: &str) -> bool {
        texts.iter().any(|(t, _)| t.contains(word))
    }

    /// Time to look at the folder, as a later moment (the tick is given it).
    fn later() -> std::time::Instant {
        std::time::Instant::now() + LIST_EVERY * 2
    }

    /// A file the agent makes in the folder comes into the list by itself,
    /// one removed goes from it, a rename is both; what the list keeps (the
    /// open file, a name being typed) is not disturbed; nothing is read
    /// again while the names are the same.
    #[test]
    fn the_list_sees_what_is_made_outside() {
        let (dir, memory, mut shell) = managed_game("outside-list");
        let ctx = egui::Context::default();
        shell.open("main.lua");
        shell.naming = Some(Naming { sort: "Scenarios".into(), text: "half a na".into(), problem: None, fresh: false });
        frame(&ctx, &mut shell);
        let mut at = later();
        let mut look = |shell: &mut Shell| {
            at += LIST_EVERY * 2;
            shell.tick(at)
        };
        let (changed, next) = look(&mut shell);
        assert!(!changed && next <= LIST_EVERY, "nothing new: no frame due, a look within a second");
        std::fs::write(dir.join("scenarios/boss.lua"), NEW_SCENARIO).unwrap();
        crate::editor::frames::ModelFile::begun(&dir.join("enemy.vvx")).save().unwrap();
        assert!(!shell.has("scenarios/boss.lua"), "not before the look");
        let (changed, _) = look(&mut shell);
        assert!(changed, "a frame is due");
        assert!(shell.has("scenarios/boss.lua") && shell.has("enemy.vvx"));
        assert!(shell.take_asks().is_empty());
        assert_eq!(shell.opened(), Some("main.lua"), "the open file stays");
        assert_eq!(shell.naming.as_ref().map(|n| n.text.as_str()), Some("half a na"), "a name being typed is left alone");
        assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "boss"), "and it is in the list");
        std::fs::rename(dir.join("scenarios/boss.lua"), dir.join("scenarios/last.lua")).unwrap();
        std::fs::remove_file(dir.join("enemy.vvx")).unwrap();
        assert!(look(&mut shell).0);
        assert!(shell.has("scenarios/last.lua") && !shell.has("scenarios/boss.lua") && !shell.has("enemy.vvx"));
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(says(&texts, "last") && !says(&texts, "boss"));
        // A file being renamed in the list whose file goes: the field goes.
        shell.renaming = Some(manage::Renaming { path: "scenarios/last.lua".into(), text: "last".into(), problem: None, fresh: false });
        std::fs::remove_file(dir.join("scenarios/last.lua")).unwrap();
        look(&mut shell);
        assert!(shell.renaming.is_none());
        clean_up(&dir, &shell);
        let _ = std::fs::remove_file(&memory);
    }

    /// The open file made away with outside the IDE: the main area says so
    /// in plain words; a text keeps its text (with changes too), is not
    /// written again by itself, and SAVE AGAIN puts it back; a picture has
    /// nothing to show but the sentence.
    #[test]
    fn the_open_file_vanishing_is_said_and_not_recreated() {
        let (dir, memory, mut shell) = managed_game("vanishing");
        let ctx = egui::Context::default();
        shell.open("scenarios/late.lua");
        frame(&ctx, &mut shell);
        type_into_text(&ctx, &mut shell, "-- mine\n");
        std::fs::remove_file(dir.join("scenarios/late.lua")).unwrap();
        let later = later();
        shell.tick(later);
        assert!(!shell.has("scenarios/late.lua"));
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(says(&texts, "scenarios/late.lua is gone from the game folder") && says(&texts, "SAVE AGAIN") && says(&texts, "FILE GONE"), "{texts:?}");
        // The autosave's second goes by, and the folder is looked at again.
        shell.tick(later + super::super::buffer::SAVE_AFTER * 3);
        shell.flush();
        assert!(!dir.join("scenarios/late.lua").exists(), "not made again by itself");
        assert!(buffer(&shell).code.text.contains("-- mine"), "the text is kept");
        // SAVE AGAIN writes it back, the changes in it.
        let at = drawn_text(&ctx, &mut shell, vec![]).into_iter().find(|(t, _)| t == "SAVE AGAIN").expect("the button").1;
        click(&ctx, &mut shell, at);
        let back = std::fs::read_to_string(dir.join("scenarios/late.lua")).unwrap();
        assert!(back.contains("-- mine") && shell.has("scenarios/late.lua"), "put back: {back}");
        assert_eq!(shell.saved_word(), Some("SAVED"));
        // A picture: nothing to keep, only the sentence.
        std::fs::write(dir.join("back.png"), "x").unwrap();
        shell.tick(later + LIST_EVERY * 4);
        shell.open("back.png");
        frame(&ctx, &mut shell);
        std::fs::remove_file(dir.join("back.png")).unwrap();
        shell.tick(later + LIST_EVERY * 8);
        assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "back.png is gone from the game folder"));
        clean_up(&dir, &shell);
        let _ = std::fs::remove_file(&memory);
    }

    /// A game opened for the first time has its console folded: only its
    /// bar, no shell started; the arrow (and the tabs and CLAUDE) unfold it,
    /// the arrow folds it again, and what was done is remembered for the next
    /// time, an older memory without the height included.
    #[test]
    fn the_console_is_folded_until_wanted_and_remembered() {
        let (dir, memory) = bare_game("console-folded");
        let _ = std::fs::remove_file(&memory);
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        let arrow = egui::pos2(1280.0 - INSET - 4.0, 720.0 - 15.0);
        for _ in 0..3 {
            frame(&ctx, &mut shell);
        }
        assert!(shell.memory.console_folded && !shell.console().started(), "folded, nothing started");
        click(&ctx, &mut shell, arrow);
        frame(&ctx, &mut shell);
        assert!(!shell.memory.console_folded && shell.console().started(), "unfolded by the arrow, the shell asked for");
        assert!(!Memory::load(&memory).console_folded, "remembered unfolded");
        let again = shell_on(&dir, &memory);
        assert!(!again.memory.console_folded, "and unfolded the next time");
        // (Its bar is now at the top of the console.)
        let top = 720.0 - shell.console_seen.expect("unfolded") + 15.0;
        click(&ctx, &mut shell, egui::pos2(arrow.x, top));
        assert!(shell.memory.console_folded && Memory::load(&memory).console_folded, "folded again by the arrow");
        // A tab unfolds it too (CONSOLE at the left of the bar).
        let tab = drawn_text(&ctx, &mut shell, vec![]).into_iter().find(|(t, _)| t == "CONSOLE").expect("the tab").1;
        click(&ctx, &mut shell, tab);
        assert!(!shell.memory.console_folded);
        // What an older IDE wrote has no height: it loads, unfolded as it said.
        std::fs::write(&memory, "open = \"main.lua\"\nconsole_folded = false\n").unwrap();
        let old = shell_on(&dir, &memory);
        assert!(!old.memory.console_folded && old.memory.console_height.is_none());
        // The height the user dragged to is the height it opens at.
        std::fs::write(&memory, "open = \"main.lua\"\nconsole_folded = false\nconsole_height = 200.0\n").unwrap();
        let mut tall = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&ctx, &mut tall);
        }
        assert!(tall.console_seen.is_some_and(|h| (h - 200.0).abs() < 3.0), "{:?}", tall.console_seen);
        // Dragged by its top edge, let go: kept.
        let (from, to) = (egui::pos2(640.0, 720.0 - 200.0), egui::pos2(640.0, 720.0 - 320.0));
        let button = |at, pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame_with(&ctx, &mut tall, vec![egui::Event::PointerMoved(from)]);
        frame_with(&ctx, &mut tall, vec![button(from, true)]);
        frame_with(&ctx, &mut tall, vec![egui::Event::PointerMoved(to)]);
        frame_with(&ctx, &mut tall, vec![egui::Event::PointerMoved(to)]);
        frame_with(&ctx, &mut tall, vec![button(to, false)]);
        frame_with(&ctx, &mut tall, vec![]);
        let kept = Memory::load(&memory).console_height.expect("kept");
        assert!(kept > 280.0, "{kept}");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&memory);
        let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
    }

    /// START WITH THE IDE (the agent in the console) behaves as before: the
    /// console is unfolded and the agent asked for.
    #[test]
    fn an_agent_that_starts_with_the_ide_has_the_console_open() {
        let (dir, memory) = bare_game("console-agent");
        let _ = std::fs::remove_file(&memory);
        let prefs = dir.with_extension("ide.toml");
        let mut settings = Prefs::default();
        settings.claude.with_ide = true;
        settings.save(&prefs).unwrap();
        let mut shell = shell_on(&dir, &memory);
        shell.finder = super::super::claude::Finder::in_dirs(vec![fake_agent("console-agent")]);
        let shell = shell.with_prefs(prefs.clone());
        assert!(!shell.memory.console_folded);
        assert!(shell.console.typed().starts_with("claude "), "typed: {}", shell.console.typed());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(prefs.with_file_name("agents"));
        let _ = std::fs::remove_file(&prefs);
        let _ = std::fs::remove_file(&memory);
    }

    /// A folder with a program called `claude` that runs (nothing starts it).
    fn fake_agent(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("voxcade-agent-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("claude");
        std::fs::write(&program, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    /// Where the word `word` was drawn (its middle), for a click.
    fn at_word(texts: &[(String, egui::Pos2)], word: &str) -> egui::Pos2 {
        texts.iter().find(|(t, _)| t == word).unwrap_or_else(|| panic!("no {word} in {texts:?}")).1
    }

    /// The agent's program missing: CLAUDE types nothing; the console says in
    /// plain words what an agent is, where to get one, that the IDE works
    /// without it and where to choose it; LOOK AGAIN finds it once it is
    /// installed and starts it.
    #[test]
    fn a_missing_agent_is_explained_and_nothing_is_typed() {
        let (dir, memory) = bare_game("no-agent");
        let _ = std::fs::remove_file(&memory);
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        shell.start_agent(false, true);
        assert!(shell.no_agent && shell.console.typed().is_empty(), "nothing typed");
        assert!(!shell.memory.console_folded, "asked for: the console opens");
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        for word in ["NO AGENT FOUND", "An agent is a helper", "Claude Code", "subscription or credit", "https://claude.com/claude-code", "works without one", "LOOK AGAIN"] {
            assert!(says(&texts, word), "{word} in {texts:?}");
        }
        // Looking again with nothing installed changes nothing.
        let at = at_word(&texts, "LOOK AGAIN");
        click(&ctx, &mut shell, at);
        assert!(shell.no_agent && shell.console.typed().is_empty());
        // Installed: LOOK AGAIN finds it and types its command.
        let bin = fake_agent("no-agent");
        shell.finder = super::super::claude::Finder::in_dirs(vec![bin.clone()]);
        let at = at_word(&drawn_text(&ctx, &mut shell, vec![]), "LOOK AGAIN");
        click(&ctx, &mut shell, at);
        assert!(!shell.no_agent, "found");
        assert!(shell.console.typed().starts_with("claude "), "typed: {}", shell.console.typed());
        assert!(!says(&drawn_text(&ctx, &mut shell, vec![]), "NO AGENT FOUND"));
        let _ = std::fs::remove_dir_all(&bin);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&memory);
    }

    /// Started with the IDE and no agent: nothing typed, the console stays
    /// folded over the maker's game, and the bar says NO AGENT FOUND.
    #[test]
    fn a_missing_agent_does_not_open_the_console_by_itself() {
        let (dir, memory) = bare_game("no-agent-start");
        let _ = std::fs::remove_file(&memory);
        let prefs = dir.with_extension("ide.toml");
        let mut settings = Prefs::default();
        settings.claude.with_ide = true;
        settings.save(&prefs).unwrap();
        let mut shell = shell_on(&dir, &memory).with_prefs(prefs.clone());
        assert!(shell.no_agent && shell.memory.console_folded && shell.console.typed().is_empty());
        let ctx = egui::Context::default();
        assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "NO AGENT FOUND"), "said in the bar");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(prefs.with_file_name("agents"));
        let _ = std::fs::remove_file(&prefs);
        let _ = std::fs::remove_file(&memory);
    }

    /// The game's keys stand on a line under it: moving, then each action by
    /// its own name with its key.
    #[test]
    fn the_games_keys_stand_under_it() {
        let (dir, memory, mut shell) = managed_game("keys-line");
        let ctx = egui::Context::default();
        shell.stage = Some(Stage { shown: true, keys: vec![("MOVE".into(), "ARROWS".into()), ("JUMP".into(), "Y".into()), ("FIRE".into(), "X".into())], ..Stage::default() });
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        for word in ["MOVE", "ARROWS", "JUMP", "Y", "FIRE", "X"] {
            assert!(texts.iter().any(|(t, _)| t == word), "{word} in {texts:?}");
        }
        let row = |w: &str| at_word(&texts, w);
        assert!(row("MOVE").y > 500.0 && (row("MOVE").y - row("FIRE").y).abs() < 1.0, "on a line low in the main area");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&memory);
    }

    /// AGAIN beside STOP begins the game anew and shows it; once the game is
    /// held behind a file the bar says BACK TO THE GAME instead of PLAY, with
    /// STOP and AGAIN still there; shown, neither is.
    #[test]
    fn again_and_back_to_the_game() {
        let (dir, memory, mut shell) = managed_game("again");
        let ctx = egui::Context::default();
        assert!(!says(&drawn_text(&ctx, &mut shell, vec![]), "AGAIN"), "no game, nothing to begin again");
        assert!(drawn_text(&ctx, &mut shell, vec![]).iter().any(|(t, _)| t == "PLAY"));
        shell.stage = Some(Stage { shown: true, ..Stage::default() });
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(texts.iter().any(|(t, _)| t == "STOP") && texts.iter().any(|(t, _)| t == "AGAIN"));
        assert!(!texts.iter().any(|(t, _)| t == "PLAY" || t == "BACK TO THE GAME"), "playing: neither");
        // Held behind the file.
        shell.back_to_file();
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(texts.iter().any(|(t, _)| t == "BACK TO THE GAME"), "{texts:?}");
        assert!(!texts.iter().any(|(t, _)| t == "PLAY") && texts.iter().any(|(t, _)| t == "STOP") && texts.iter().any(|(t, _)| t == "AGAIN"));
        click(&ctx, &mut shell, at_word(&texts, "BACK TO THE GAME"));
        assert!(shell.stage.as_ref().unwrap().shown, "the game again");
        assert!(shell.take_asks().is_empty(), "shown, not begun anew");
        // AGAIN asks for a new start, from the held state too, and shows the game.
        shell.back_to_file();
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        click(&ctx, &mut shell, at_word(&texts, "AGAIN"));
        assert_eq!(shell.take_asks(), [Ask::Again]);
        assert!(shell.stage.as_ref().unwrap().shown);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&memory);
    }

    /// While the console is folded a quiet sign says the shell wrote
    /// something new (in the accent colour, a dot), and it goes once the
    /// console is in view again.
    #[test]
    fn a_folded_console_shows_that_something_was_written() {
        let (dir, memory) = bare_game("console-news");
        let _ = std::fs::remove_file(&memory);
        let console = Console::new(dir.clone(), Start::Command("echo one; sleep 1; echo two".into()), std::sync::Arc::new(|| {}));
        let mut shell = Shell::new(&dir, crate::session::config::<crate::Voxcade>(&dir).unwrap(), memory.clone(), console);
        let ctx = egui::Context::default();
        shell.unfold_console();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while shell.console().arrivals() == 0 && std::time::Instant::now() < until {
            frame_with(&ctx, &mut shell, vec![]);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!shell.console().news(), "in view: nothing new");
        shell.memory.console_folded = true;
        while !shell.console().news() && std::time::Instant::now() < until {
            frame_with(&ctx, &mut shell, vec![]);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(shell.console().news(), "written while folded");
        assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "CONSOLE •"));
        shell.memory.console_folded = false;
        frame_with(&ctx, &mut shell, vec![]);
        assert!(!shell.console().news(), "seen");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&memory);
    }

    /// What is wrong with the game is a red line over whatever the main area
    /// shows (a sound, nothing, HELP, the game), with TO THE LINE, SHOW and
    /// COPY; it goes when the game is mended.
    #[test]
    fn the_games_error_shows_over_whatever_is_open() {
        let (dir, memory, mut shell) = managed_game("error-line");
        let ctx = egui::Context::default();
        shell.open("sounds/coin.toml");
        shell.stage = Some(Stage::default());
        frame(&ctx, &mut shell);
        assert!(!says(&drawn_text(&ctx, &mut shell, vec![]), "THE GAME STOPPED"), "all is well");
        shell.stage.as_mut().unwrap().trouble = Some("main.lua:3: attempt to index a nil value\nstack traceback:\n\t[C]: in ?".into());
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(says(&texts, "THE GAME STOPPED: main.lua:3: attempt to index a nil value"), "over a sound: {texts:?}");
        assert!(!says(&texts, "stack traceback"), "its first line");
        assert!(["TO THE LINE", "SHOW", "COPY"].iter().all(|b| says(&texts, b)));
        // Over HELP, the game and nothing.
        shell.help = true;
        assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "THE GAME STOPPED"), "over HELP");
        shell.help = false;
        shell.stage.as_mut().unwrap().shown = true;
        assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "THE GAME STOPPED"), "over the game");
        shell.stage.as_mut().unwrap().shown = false;
        shell.memory.open = None;
        shell.unload();
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(says(&texts, "THE GAME STOPPED") && says(&texts, "Nothing here yet."), "over nothing");
        // A file that would not load, and a game that would not start.
        shell.stage.as_mut().unwrap().trouble = None;
        shell.stage.as_mut().unwrap().unloaded = Some("sounds/coin.toml would not load: LINE 2: nope".into());
        assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "sounds/coin.toml would not load"));
        shell.stage = None;
        shell.wont_start("conf.lua:2: unexpected symbol".into());
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(says(&texts, "THE GAME WOULD NOT START: conf.lua:2: unexpected symbol") && says(&texts, "TO THE LINE"));
        assert!(shell.output.errors(5).iter().any(|l| l.text.starts_with("THE GAME WOULD NOT START") && l.at == Some(("conf.lua".into(), 2))), "and in OUTPUT");
        // SHOW: OUTPUT, the console unfolded (and there, though the settings left it out).
        shell.prefs.console = false;
        let at = texts.iter().find(|(t, _)| t == "SHOW").expect("SHOW").1;
        click(&ctx, &mut shell, at);
        assert!(shell.prefs.console && shell.output_shown && !shell.memory.console_folded);
        // TO THE LINE: the file at the line.
        let at = drawn_text(&ctx, &mut shell, vec![]).into_iter().find(|(t, _)| t == "TO THE LINE").expect("the button").1;
        click(&ctx, &mut shell, at);
        assert_eq!(shell.opened(), Some("conf.lua"));
        // Mended: the line goes.
        shell.start_mended();
        assert!(!says(&drawn_text(&ctx, &mut shell, vec![]), "TO THE LINE"));
        clean_up(&dir, &shell);
        let _ = std::fs::remove_file(&memory);
    }

    /// BUILD says BUILDING… while it packs (the window goes on), makes the
    /// zip in the game's own folder under `builds/`, and says where, with
    /// SHOW IN FOLDER; a build that cannot says why, in red, with COPY. The
    /// list does not show `builds/`.
    #[test]
    fn build_works_in_the_background_and_lands_in_the_games_folder() {
        let (dir, memory) = bare_game("build");
        let mut shell = shell_on(&dir, &memory);
        let ctx = egui::Context::default();
        frame(&ctx, &mut shell);
        shell.build();
        assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "BUILDING…"));
        let until = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while shell.building.is_some() && std::time::Instant::now() < until {
            shell.tick(std::time::Instant::now());
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let Some(Notice::Built(zip, said)) = &shell.notice else { panic!("built") };
        let name = crate::settings::game_name(&dir);
        assert!(zip.starts_with(dir.canonicalize().unwrap().join("builds")) && zip.is_file(), "{zip:?}");
        assert!(said.starts_with("BUILT ") && said.contains(&format!("{name}-")), "{said}");
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(says(&texts, "SHOW IN FOLDER") && says(&texts, "BUILT "), "{texts:?}");
        assert!(!shell.files.assets.iter().any(|g| g.files.iter().any(|f| f.starts_with("builds/"))), "not listed");
        assert!(!Files::names(&dir).iter().any(|f| f.starts_with("builds/")));
        // A build that cannot: `builds` is a file.
        std::fs::remove_dir_all(dir.join("builds")).unwrap();
        std::fs::write(dir.join("builds"), "in the way").unwrap();
        shell.build();
        while shell.building.is_some() && std::time::Instant::now() < until {
            shell.tick(std::time::Instant::now());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(matches!(&shell.notice, Some(Notice::Failed(w)) if w.starts_with("BUILD FAILED: ")), "failed");
        let texts = drawn_text(&ctx, &mut shell, vec![]);
        assert!(says(&texts, "BUILD FAILED") && says(&texts, "COPY"));
        assert!(shell.output.errors(2).iter().any(|l| l.text.starts_with("BUILD FAILED")));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&memory);
    }
}
