//! The agent in the console and the IDE's settings (the gear): starting
//! it, NO AGENT FOUND, the projects folder, leaving for another game
//! while something runs in the console.

use super::*;

impl Shell {
    /// Leaving would end what runs in the console: asked first.
    pub(super) fn leaving_ui(&mut self, ui: &mut egui::Ui) {
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
    pub(super) fn start_agent(&mut self, fresh: bool, asked: bool) {
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
            && let Err(e) = crate::ide::claude::write_mcp(file)
        {
            trouble = Some(format!("the IDE's tools: {e:#}"));
        }
        // Its part: the game in its folder (`claude::brief`).
        let briefing = match self.prefs_path.as_ref().map(|p| crate::ide::claude::brief(&p.with_file_name("agents"), &self.dir, &self.title)) {
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
    pub(super) fn keep_prefs(&self) -> anyhow::Result<()> {
        match &self.prefs_path {
            Some(path) => self.prefs.save(path),
            None => Ok(()),
        }
    }

    /// The settings (the gear at the top right): the console, the agent
    /// and how it starts.
    pub(super) fn settings(&mut self, ui: &mut egui::Ui) {
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
}

/// What the console shows where the agent's program was not found: a
/// heading (what an agent is, on hover), where to get it (the address
/// opens in the browser) and LOOK AGAIN, pressed after installing it: true.
pub(super) fn agent_missing(ui: &mut egui::Ui, agent: Agent) -> bool {
    let mut again = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).content_margin(egui::Margin { left: 20, right: 20, top: 12, bottom: 12 }).show(ui, |ui| {
        ui.set_max_width(680.0f32.min(ui.available_width()));
        ui.label(RichText::new("NO AGENT FOUND").color(ACCENT)).on_hover_text("An agent is a helper you tell in plain words what your game should do: it writes the files for you. It is a program of its own, installed first (Claude Code needs a subscription or credit with its maker). The IDE works without one.");
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("Get {}:", agent.called())).color(TEXT));
            if ui.link(agent.home()).on_hover_text("Opens the page in your browser.").clicked() {
                crate::update::show(agent.home());
            }
        });
        ui.add_space(10.0);
        again = ui.button("LOOK AGAIN").on_hover_text("Looks once more for the agent, after you installed it, and starts it when it is there.").clicked();
    });
    again
}
