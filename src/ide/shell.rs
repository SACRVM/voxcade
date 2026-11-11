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
use super::terminal;


/// What the main area shows of the open file.
enum Shown {
    /// A script or a text it reads, to change.
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
    /// The game's settings (the light and camera previews use).
    cfg: crate::config::GameConfig,
    /// The picture of the model, block or level open, and the one before
    /// (the new one takes over its renderer).
    preview: Option<Preview>,
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
    notice: Option<String>,
    notice_until: Option<std::time::Instant>,
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
    reported: Option<String>,
    output_saved: std::time::Instant,
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
}

/// What the window asks of the game, and of the speaker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// The game played, begun from a scenario if one is named.
    Play(Option<String>),
    Stop,
    /// A line of Lua run in the game being played (the line under OUTPUT).
    Eval(String),
    /// A song (its file) to play, and silence again.
    Listen(String),
    Hush,
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

/// A new scenario's file (NEW SCENARIO), to fill in.
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

impl Shell {
    /// The IDE on the game in `dir` (its settings `cfg`), remembering in
    /// `memory_path`, with `console`; what was open last is open again.
    pub fn new(dir: &Path, cfg: crate::config::GameConfig, memory_path: PathBuf, console: Console) -> Self {
        let (title, level) = (cfg.title.clone(), cfg.world.blocks.as_ref().map(|b| b.file.clone()));
        let files = Files::scan(dir);
        let mut memory = Memory::load(&memory_path);
        if memory.open.as_ref().is_none_or(|f| !files.has(f)) {
            memory.open = files.logic.iter().find(|f| *f == "main.lua").or(files.logic.first()).cloned();
        }
        let restore = memory.open.as_ref().and_then(|f| memory.scroll.get(f).copied());
        Self { title, dir: dir.to_path_buf(), files, memory, memory_path, shown: None, outside: None, facts: None, restore, console, level, edits: Vec::new(), message: None, cfg, preview: None, spare: None, retired: Vec::new(), look_at: std::time::Instant::now(), animate: true, stage: None, asks: Vec::new(), goto: None, focus_code: false, notice: None, notice_until: None, listening: None, prefs: Prefs::default(), prefs_path: None, new_game: None, leaving: None, help: false, help_topic: 0, output: Output::open(None), lua: crate::prompt::Prompt::default(), output_shown: false, reported: None, output_saved: std::time::Instant::now() }
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
            self.start_agent(false);
        }
        self
    }

    /// Starts the agent in the console as the settings say (`fresh`: a new
    /// conversation, the console started anew for it).
    fn start_agent(&mut self, fresh: bool) {
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
            self.start_agent(true);
            ui.close();
        }
        ui.label(RichText::new(said).small().color(QUIET));
        if changed && let Err(e) = self.keep_prefs() {
            self.console.tell(Some(format!("the settings: {e:#}")));
        }
    }

    /// Shows `path` (in the game folder) in the main area.
    pub fn open(&mut self, path: &str) {
        if self.outside.take().is_some() {
            self.unload();
        }
        if self.memory.open.as_deref() == Some(path) {
            return;
        }
        self.flush();
        self.hush();
        self.memory.open = Some(path.to_string());
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
        let Some(Shown::Code(buffer)) = &mut self.shown else { return (false, std::time::Duration::from_secs(3600)) };
        let mut changed = false;
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
        let next = buffer.due(now).unwrap_or(LOOK_EVERY).min(self.look_at.saturating_duration_since(now));
        (changed, next)
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

    /// NEW SCENARIO: a scenario's file to fill in, opened.
    pub fn new_scenario(&mut self) {
        let free = (1..).map(|n| if n == 1 { "scenarios/new.lua".to_string() } else { format!("scenarios/new_{n}.lua") }).find(|f| !self.dir.join(f).exists()).unwrap_or_default();
        let made = std::fs::create_dir_all(self.dir.join("scenarios")).and_then(|()| std::fs::write(self.dir.join(&free), NEW_SCENARIO));
        match made {
            Ok(()) => {
                self.files = super::files::Files::scan(&self.dir);
                self.open(&free);
                self.back_to_file();
            }
            Err(e) => self.tell(format!("{free}: {e}")),
        }
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
        if let Some(path) = self.memory.open.clone().filter(|p| p.ends_with(".mml")) {
            self.listening = Some(path.clone());
            self.asks.push(Ask::Listen(path));
        }
    }

    /// Ends the song playing, if one is (another file opened, the game
    /// played).
    fn hush(&mut self) {
        if self.listening.take().is_some() {
            self.asks.push(Ask::Hush);
        }
    }

    /// The song in `path` to play, at the volume the game and the player
    /// set for music.
    pub fn song(&self, path: &str) -> anyhow::Result<crate::audio::music::Player> {
        let file = self.dir.join(path);
        let text = std::fs::read_to_string(&file).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
        let song = crate::audio::music::parse(&text).map_err(|e| anyhow::anyhow!("{path}: {e:#}"))?;
        let gain = crate::settings::Settings::load(&crate::settings::path_for(&self.dir)).music_gain();
        Ok(crate::audio::music::Player::new(song, self.cfg.audio.music_volume * gain))
    }

    /// BUILD: the text being changed saved, the game and this program
    /// packed into one zip (`export.rs`); what came of it shown in the bar.
    pub fn build(&mut self) {
        self.flush();
        let out = super::export::out_path(&self.dir);
        let made = crate::cli::program().map_err(anyhow::Error::from).and_then(|program| super::export::export(&self.dir, &self.title, &program, &out));
        self.notice_until = None;
        let (kind, said) = match made {
            Ok(bytes) => (Kind::Note, format!("BUILT {} ({:.1} MB)", out.canonicalize().unwrap_or(out).display(), bytes as f64 / 1e6)),
            Err(e) => (Kind::Error, format!("BUILD FAILED: {e:#}")),
        };
        self.report(kind, said.clone(), None);
        self.notice = Some(said);
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
                if let Some((line, why)) = &buffer.code.error {
                    file["unreadable"] = json!({ "line": line, "why": why });
                }
            }
            (Some(Shown::Text(_)), Some(path)) if path.ends_with(".mml") => {
                file["kind"] = json!("a song (MML), to read; PLAY SONG over it plays it");
                file["playing"] = json!(self.listening.is_some());
            }
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
            out.push(format!("{path}:{line}: {why}"));
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

    /// Whether an editor opens `path` (in the game folder): the levels of
    /// blocks, their blocks, the models.
    pub fn editable(&self, path: &str) -> bool {
        self.is_level(path) || path.ends_with(".vox")
    }

    /// Whether `path` is a level of blocks the level editor opens: the one
    /// the config names, and in a game of blocks each in `levels/`.
    pub fn is_level(&self, path: &str) -> bool {
        self.level.as_deref() == Some(path) || (self.level.is_some() && path.starts_with("levels/") && path.ends_with(".json"))
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
        self.notice = Some(text);
        self.notice_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
    }

    /// Shows what went wrong opening an editor.
    pub fn tell(&mut self, message: String) {
        self.report(Kind::Error, message.clone(), None);
        self.message = Some(message);
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
                    let shown = self.stage.as_ref().is_some_and(|s| s.shown);
                    if self.stage.is_some() && ui.button("STOP").on_hover_text("Ends the game.").clicked() {
                        self.asks.push(Ask::Stop);
                    }
                    if !shown && ui.button("PLAY").on_hover_text("Plays the game here, straight in (saved first).").clicked() {
                        self.play();
                        ui.memory_mut(|m| m.request_focus(game_id()));
                    }
                    if let Some(until) = self.notice_until {
                        match until.checked_duration_since(std::time::Instant::now()) {
                            Some(left) => ui.ctx().request_repaint_after(left),
                            None => (self.notice, self.notice_until) = (None, None),
                        }
                    }
                    if let Some(notice) = &self.notice {
                        ui.label(RichText::new(notice).small().color(QUIET));
                    } else if shown {
                        ui.label(RichText::new("ESCAPE back to the file · F5 from the start (ENTER once over) · F9 keeps the run").small().color(QUIET));
                    }
                });
            });
        });
        let (mut clicked, mut edit, mut new_scenario) = (None, None, false);
        // The margins on the right lie inside the scroll areas, so that their
        // bars sit at the edge.
        egui::Panel::left("files").default_size(250.0).frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 14, right: 0, top: 12, bottom: 12 })).show(ui, |ui| {
            egui::ScrollArea::vertical().content_margin(egui::Margin { right: 6, ..egui::Margin::ZERO }).show(ui, |ui| {
                let open = if self.outside.is_some() { None } else { self.memory.open.clone() };
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
                    if header.clicked() {
                        state.toggle(ui);
                    }
                    arrow(ui, egui::pos2(header.rect.right() - INSET - 4.0, header.rect.center().y), state.openness(ui.ctx()));
                    state.show_body_unindented(ui, |ui| {
                        // The scripts by their paths; assets say what they
                        // are by their group: the name alone, the path where
                        // two have the same name.
                        let base = |f: &str| f.rsplit('/').next().unwrap_or(f).to_string();
                        for f in files {
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
                        }
                        if sort == "Scenarios" {
                            if files.is_empty() {
                                ui.label(RichText::new("None yet: runs begun late, to try what happens there.").small().color(QUIET));
                            }
                            if ui.small_button("NEW SCENARIO").on_hover_text("A scenario's file to fill in: the states as late in the game, set up in code.").clicked() {
                                new_scenario = true;
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
        if let Some(path) = edit.filter(|p| self.editable(p)) {
            self.edits.push(path);
        }
        if new_scenario {
            self.new_scenario();
        }
        // The console below the main area, folded to its heading or as
        // high as dragged; or none (CONSOLE).
        let folded = self.memory.console_folded;
        if self.prefs.console {
            // At first almost half the window: room for Claude Code's
            // screens.
            let tall = 0.45 * ui.available_height();
            let panel = if folded { egui::Panel::bottom("console folded") } else { egui::Panel::bottom("console").resizable(true).default_size(tall).min_size(120.0) };
            panel.frame(egui::Frame::new().fill(terminal::BACKGROUND)).show(ui, |ui| {
                let pressed = self.console.heading(ui, &mut self.memory.console_folded, self.prefs.agent, self.prefs.claude.connect, &mut self.output_shown, self.output.unseen);
                if pressed.agent {
                    self.output_shown = false;
                    self.start_agent(false);
                }
                if pressed.clear {
                    self.output.clear();
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
                    } else {
                        self.console.body(ui);
                        if let Some((file, line)) = self.console.take_opened() {
                            self.open_from_console(&file, line);
                        }
                    }
                }
            });
            if self.memory.console_folded != folded {
                let _ = self.save();
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(GROUND).inner_margin(egui::Margin { left: 20, right: 0, top: 14, bottom: 0 })).show(ui, |ui| self.main(ui));
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

    /// The main area playing the game: what stops it above (a click there
    /// goes to its line), its picture filling the rest, the keys its while
    /// it has the focus.
    fn game(&mut self, ui: &mut egui::Ui) {
        let Some(stage) = &mut self.stage else { return };
        let mut go = None;
        if let Some(trouble) = &stage.trouble {
            ui.horizontal_wrapped(|ui| {
                // Its first line (the game's own panel has the rest).
                ui.label(RichText::new(trouble.lines().next().unwrap_or("")).color(ERROR));
                if let Some(at) = super::play::line_of(trouble)
                    && ui.button("TO THE LINE").clicked()
                {
                    go = Some(at);
                }
            });
            ui.add_space(8.0);
        }
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
        if let Some((file, line)) = go {
            stage.shown = false;
            self.open(&file);
            self.goto = Some((line, true));
        }
    }

    /// The main area: the open file.
    fn main(&mut self, ui: &mut egui::Ui) {
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
        let Some(path) = self.memory.open.clone() else {
            ui.label(RichText::new("Nothing here yet.").color(QUIET));
            return;
        };
        if self.shown.is_none() {
            // Scripts and the texts they read are to change, the rest to
            // look at.
            self.shown = Some(if self.files.logic.contains(&path) || self.files.scenarios.contains(&path) {
                match Buffer::open(&self.dir, &path, &self.memory_path.with_extension("journal")) {
                    Ok(buffer) => Shown::Code(Box::new(buffer)),
                    Err(e) => Shown::Note(format!("{e:#}")),
                }
            } else {
                load(&self.dir.join(&path), &path, ui.ctx())
            });
            let level = self.is_level(&path);
            self.facts = match () {
                _ if self.editable(&path) => Some(facts(&self.dir.join(&path), &path, level)),
                _ if path.ends_with(".mml") => Some(song_facts(&self.dir.join(&path))),
                _ => None,
            };
            // What an editor opens shows as a picture.
            let preview = match () {
                _ if level => Some(Preview::level(&self.dir, &path, &self.cfg)),
                _ if path.ends_with(".vox") => Some(Preview::model(&self.dir, &path, &self.cfg)),
                _ => None,
            };
            self.preview = preview.and_then(|p| p.map_err(|e| self.message = Some(format!("{e:#}"))).ok());
            match (&mut self.preview, self.spare.take()) {
                (Some(new), Some(old)) => new.adopt(old),
                (None, Some(old)) => self.retired.extend(old.texture()),
                _ => {}
            }
        }
        // What an editor opens: a line of what it is, and EDIT; a song:
        // what it is, and PLAY.
        if let Some(facts) = self.facts.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(facts).color(QUIET));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    if self.editable(&path) {
                        if ui.button("EDIT").on_hover_text("Opens it in its editor, a window of its own (so does a double click in the list).").clicked() {
                            self.edits.push(path.clone());
                        }
                    } else if self.listening.is_some() {
                        if ui.button("STOP SONG").on_hover_text("Ends the song.").clicked() {
                            self.listen();
                        }
                    } else if ui.button("PLAY SONG").on_hover_text("Plays the song as saved, at the game's music volume.").clicked() {
                        self.listen();
                    }
                });
            });
            if let Some(message) = &self.message {
                ui.label(RichText::new(message).color(ERROR));
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
                });
            });
            ui.add_space(6.0);
            if play {
                self.play_scenario(name);
            }
        }
        if let Some(preview) = &mut self.preview {
            // Down to the console, the scroll bar's room kept at the right.
            let animate = self.animate;
            ui.allocate_ui(egui::vec2(ui.available_width() - 12.0, ui.available_height() - 14.0), |ui| preview.ui(ui, animate));
            return;
        }
        // A line to go to (a game's error): the cursor there, in view.
        let id = egui::Id::unique(("code", path.as_str()));
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
        if let Some(Shown::Code(buffer)) = &mut self.shown {
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
                ui.label(RichText::new(format!("LINE {line}: {why}")).color(ERROR));
                ui.add_space(8.0);
            }
            // Where the game played stopped, if in this file: its line
            // marked until the game takes the file anew.
            let trouble = self.stage.as_ref().and_then(|s| s.trouble.as_deref());
            let stopped = trouble.and_then(|t| super::play::line_of(t).filter(|(file, _)| *file == path).map(|(file, line)| (file, line, t)));
            buffer.code.stopped = stopped.as_ref().map(|(_, line, _)| *line);
            if let Some((file, line, trouble)) = stopped {
                let first = trouble.lines().next().unwrap_or("");
                let why = first.split_once(&format!("{file}:{line}:")).map_or(first, |(_, why)| why.trim());
                ui.label(RichText::new(format!("THE GAME STOPPED AT LINE {line}: {why}")).color(ERROR));
                ui.add_space(8.0);
            }
            if let Some(message) = &self.message {
                ui.label(RichText::new(message).color(ERROR));
                ui.add_space(8.0);
            }
        }
        let mut scroll = egui::ScrollArea::both().id_salt(&path).auto_shrink([false, false]).content_margin(egui::Margin { right: 12, bottom: 14, ..egui::Margin::ZERO });
        if let Some(offset) = self.restore.take() {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let out = scroll.show(ui, |ui| match self.shown.as_mut().expect("loaded above") {
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
        });
        self.memory.scroll.insert(path, out.state.clamped_offset().y);
    }
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
fn facts(file: &Path, path: &str, level: bool) -> String {
    let plural = |n: usize, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
    if level {
        return match crate::grid::GridLevel::read(file) {
            Ok(l) => format!("The level: {} by {} by {} cells of {} voxels, {}, {}.", l.size.x, l.size.y, l.size.z, l.n, plural(l.blocks.len(), "block"), plural(l.things.len(), "thing")),
            Err(e) => format!("{e:#}"),
        };
    }
    match crate::vox::load_model(file) {
        Ok(m) if path.starts_with("blocks/") => format!("A block of the level, {} by {} by {} voxels.", m.size.x, m.size.y, m.size.z),
        Ok(m) => format!("A model, {} by {} by {} voxels, {}.", m.size.x, m.size.y, m.size.z, plural(m.frames.len(), "frame")),
        Err(e) => format!("{e:#}"),
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
        // What a model is stands above it (`facts`).
        "vox" => Shown::Note(String::new()),
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
        shell.open("hero.vox");
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
        shell.open("hero.vox");
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
    /// back.
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
        assert!(drawn(&ctx), "there at first");
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
}
