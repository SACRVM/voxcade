//! The upgrade wizard: a game made for an older Voxcade does not open
//! (`crate::upgrade`); the IDE, and the launcher's EDIT and PLAY on such a
//! game, show this calm page instead. It says which Voxcade the game was
//! made for and which is running, the steps and what each would do (the
//! dry run, in the steps' own plain words), and warns when git has changes
//! not yet committed (it never commits by itself). UPGRADE runs on a
//! thread of its own, as BUILD does; then what was done, or what went
//! wrong (with COPY); then it offers to refit the game's replays (as
//! `voxcade check GAME --refit`) and opens the game. A game made with a
//! newer Voxcade gets a page that says so and points to the update.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, TextStyle};

use crate::gui::{ACCENT, ERROR, GROUND, PANEL, QUIET, TEXT};
use crate::upgrade::{Standing, engine};

/// The replays refitted: whether every one was, and what each said.
pub type Refitter = fn(&Path, &mut dyn FnMut(String)) -> anyhow::Result<bool>;

/// What the page was asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wish {
    /// Back to the start page, nothing done.
    Back,
    /// The game opened in the IDE.
    Open,
    /// The game played (it came from the launcher's PLAY).
    Play,
}

/// What the upgrade has come to.
enum Stage {
    Asking,
    Upgrading(Receiver<Result<Vec<String>, String>>),
    Upgraded { lines: Vec<String>, dry: bool, refit: Refit },
    Failed(String),
}

/// The replays after the upgrade.
enum Refit {
    /// The game has none.
    Nothing,
    /// How many it has.
    Offered(usize),
    Running(Receiver<Result<(bool, Vec<String>), String>>),
    Done(bool, Vec<String>),
    Failed(String),
}

pub struct Wizard {
    pub dir: PathBuf,
    title: String,
    standing: Result<Standing, String>,
    /// Changes git has not committed in the folder, if it is a repository.
    changes: Option<usize>,
    /// Git holds every file of the game (`upgrade::committed`): the upgrade
    /// keeps no copy, else it keeps one first.
    committed: bool,
    stage: Stage,
    /// It came from PLAY: the game is played once it is up to date.
    pub play: bool,
}

/// Whether the game in `dir` needs the page: made for another Voxcade.
pub fn needed(dir: &Path) -> bool {
    !matches!(crate::upgrade::standing(dir), Ok(Standing::Current))
}

impl Wizard {
    /// The page for the game in `dir` (`play`: it came from PLAY).
    pub fn new(dir: &Path, play: bool) -> Self {
        let standing = crate::upgrade::standing(dir).map_err(|e| format!("{e:#}"));
        let (changes, committed) = (crate::upgrade::uncommitted(dir), crate::upgrade::committed(dir));
        Self { dir: dir.to_path_buf(), title: super::projects::title_of(dir), standing, changes, committed, stage: Stage::Asking, play }
    }

    /// UPGRADE: the upgrade on a thread of its own.
    pub fn upgrade(&mut self) {
        let (tell, heard) = channel();
        let dir = self.dir.clone();
        std::thread::spawn(move || {
            let _ = tell.send(crate::upgrade::upgrade(&dir, false).map_err(|e| format!("{e:#}")));
        });
        self.stage = Stage::Upgrading(heard);
    }

    /// The page as after UPGRADE, the upgrade only run dry: nothing is
    /// changed (`voxcade ide GAME --shot FILE --upgraded`).
    pub fn upgraded_dry(&mut self) {
        self.stage = match crate::upgrade::upgrade(&self.dir, true) {
            Ok(lines) => Stage::Upgraded { lines, dry: true, refit: self.refit_offer() },
            Err(e) => Stage::Failed(format!("{e:#}")),
        };
    }

    fn refit_offer(&self) -> Refit {
        match crate::upgrade::files_ending(&self.dir.join("replays"), "toml").len() {
            0 => Refit::Nothing,
            n => Refit::Offered(n),
        }
    }

    /// REFIT THE REPLAYS: `refit` (the game's `replay::refit_saying`) on a
    /// thread of its own.
    pub fn refit(&mut self, refit: Refitter) {
        let Stage::Upgraded { refit: state, .. } = &mut self.stage else { return };
        let (tell, heard) = channel();
        let dir = self.dir.clone();
        std::thread::spawn(move || {
            let mut said = Vec::new();
            let done = refit(&dir, &mut |line| said.push(line));
            let _ = tell.send(done.map(|all| (all, said)).map_err(|e| format!("{e:#}")));
        });
        *state = Refit::Running(heard);
    }

    /// Whether a thread is at work (the window looks again soon).
    pub fn busy(&self) -> bool {
        matches!(self.stage, Stage::Upgrading(_) | Stage::Upgraded { refit: Refit::Running(_), .. })
    }

    /// What the threads said since the last look.
    fn hear(&mut self) {
        if let Stage::Upgrading(heard) = &self.stage
            && let Ok(done) = heard.try_recv()
        {
            self.stage = match done {
                Ok(lines) => Stage::Upgraded { lines, dry: false, refit: self.refit_offer() },
                Err(e) => Stage::Failed(e),
            };
            self.changes = crate::upgrade::uncommitted(&self.dir);
        }
        if let Stage::Upgraded { refit, .. } = &mut self.stage
            && let Refit::Running(heard) = refit
            && let Ok(done) = heard.try_recv()
        {
            *refit = match done {
                Ok((all, said)) => Refit::Done(all, said),
                Err(e) => Refit::Failed(e),
            };
        }
    }

    /// Waits for the threads (the tests).
    #[cfg(test)]
    pub fn wait(&mut self) {
        while self.busy() {
            std::thread::sleep(std::time::Duration::from_millis(20));
            self.hear();
        }
    }

    /// The page, filling `ui`: what it is about above, what the upgrade
    /// would do (or did) in the middle, and along the bottom what to do
    /// now. What was asked.
    pub fn ui(&mut self, ui: &mut egui::Ui, refit: Refitter) -> Option<Wish> {
        self.hear();
        if self.busy() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut wish = None;
        egui::Panel::bottom("upgrade foot").frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(40, 16))).show(ui, |ui| wish = self.foot(ui, refit));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(GROUND).inner_margin(egui::Margin { left: 40, right: 24, top: 32, bottom: 12 })).show(ui, |ui| {
            ui.label(RichText::new(self.title.to_uppercase()).text_style(TextStyle::Heading).color(ACCENT));
            ui.label(RichText::new(self.dir.display().to_string()).small().color(QUIET));
            ui.add_space(18.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(980.0);
                self.body(ui);
            });
        });
        wish
    }

    /// Who made the game for what, and what the upgrade would do or did.
    fn body(&mut self, ui: &mut egui::Ui) {
        let standing = match &self.standing {
            Ok(s) => s.clone(),
            Err(e) => return error(ui, e),
        };
        let made = match &standing {
            Standing::Newer { made } => {
                ui.label(format!("MADE WITH VOXCADE {made} · THIS IS {}", engine()));
                ui.add_space(6.0);
                ui.label(RichText::new("GET THE NEW VERSION").color(QUIET)).on_hover_text("The start page's line about new versions fetches it (voxca.de has it too). The game is left as it is.");
                return;
            }
            Standing::Current if matches!(self.stage, Stage::Asking) => {
                ui.label(format!("MADE FOR VOXCADE {} · UP TO DATE", engine()));
                return;
            }
            Standing::Older { made: Some(v), .. } => format!("Voxcade {v}"),
            _ => "an older Voxcade (its conf.lua names none)".to_string(),
        };
        match &self.stage {
            Stage::Asking => {
                ui.label(format!("MADE FOR {} · THIS IS VOXCADE {}", made.to_uppercase(), engine()));
                ui.add_space(4.0);
                ui.label(RichText::new("STEPS").color(QUIET)).on_hover_text("The files are brought up to date before the game opens here. This is what the upgrade would do.");
                ui.add_space(12.0);
                if let Standing::Older { steps, .. } = &standing {
                    // Only the steps that change something; the rest counted.
                    for step in steps.iter().filter(|s| !s.would.is_empty()) {
                        ui.label(RichText::new(format!("{} (Voxcade {})", step.name.to_uppercase(), step.since)).color(TEXT));
                        for line in &step.would {
                            said(ui, line);
                        }
                        ui.add_space(8.0);
                    }
                    if let Some(idle) = idle_steps(steps.iter().filter(|s| s.would.is_empty()).count()) {
                        said(ui, &idle);
                    }
                    said(ui, &format!("then conf.lua says c.voxcade = \"{}\"", engine()));
                }
            }
            Stage::Upgrading(_) => {
                ui.label(RichText::new("UPGRADING…").color(ACCENT));
            }
            Stage::Failed(e) => {
                ui.label(RichText::new("THE UPGRADE STOPPED").color(ACCENT)).on_hover_text("What it did before is kept; what it could not do is said below. Try again once it is mended, or bring it up to date by hand (voxcade upgrade).");
                ui.add_space(6.0);
                error(ui, e);
            }
            Stage::Upgraded { lines, dry, refit } => {
                ui.label(if *dry { "DRY RUN · NOTHING CHANGED".to_string() } else { format!("BROUGHT UP TO DATE FOR VOXCADE {}", engine()) });
                ui.add_space(8.0);
                let lines = shown_lines(lines);
                for line in &lines {
                    said(ui, line);
                }
                ui.add_space(4.0);
                copy_button(ui, &lines.join("\n"));
                ui.add_space(14.0);
                match refit {
                    Refit::Nothing => {}
                    Refit::Offered(n) => {
                        let line = format!("{n} replay{} recorded on the old files", if *n == 1 { "" } else { "s" });
                        egui::Frame::new().inner_margin(egui::Margin { left: 16, ..egui::Margin::ZERO }).show(ui, |ui| ui.label(RichText::new(line).color(QUIET)).on_hover_text("REFIT THE REPLAYS plays each on the new files; if every one still plays exactly as recorded, they take the new files (nothing else in them changes)."));
                    }
                    Refit::Running(_) => {
                        ui.label(RichText::new("PLAYING THE REPLAYS…").color(ACCENT));
                    }
                    Refit::Done(fine, lines) => {
                        ui.label(if *fine { "EVERY REPLAY PLAYS AS RECORDED" } else { "NOT EVERY REPLAY PLAYS AS RECORDED · NOTHING WRITTEN" });
                        for line in lines {
                            said(ui, line);
                        }
                    }
                    Refit::Failed(e) => error(ui, e),
                }
            }
        }
    }

    /// What to do now: the buttons, and before UPGRADE a word on changes
    /// git has not committed.
    fn foot(&mut self, ui: &mut egui::Ui, refit: Refitter) -> Option<Wish> {
        let mut wish = None;
        let open = if self.play { ("PLAY", Wish::Play) } else { ("OPEN THE GAME", Wish::Open) };
        let asking = matches!(self.stage, Stage::Asking) && matches!(self.standing, Ok(Standing::Older { .. }));
        if asking {
            ui.horizontal(|ui| {
                if let Some(n) = self.changes {
                    ui.label(RichText::new(format!("{n} UNCOMMITTED CHANGE{} ·", if n == 1 { "" } else { "S" })).color(ACCENT)).on_hover_text("Save a step first (commit them), so that the upgrade's changes stand apart from yours: the upgrade commits nothing.");
                }
                let (word, tip) = keeping(self.committed);
                ui.label(RichText::new(word).color(QUIET)).on_hover_text(tip);
            });
            ui.add_space(8.0);
        }
        ui.horizontal(|ui| {
            match (&self.standing, &self.stage) {
                (Ok(Standing::Older { .. }), Stage::Asking) => {
                    if ui.button("UPGRADE").on_hover_text("Brings the game's files up to date, as written above.").clicked() {
                        self.upgrade();
                    }
                }
                (Ok(Standing::Current), Stage::Asking) => {
                    if ui.button(open.0).clicked() {
                        wish = Some(open.1);
                    }
                }
                (_, Stage::Upgraded { refit: state, dry, .. }) => {
                    if matches!(state, Refit::Offered(_)) && ui.button("REFIT THE REPLAYS").on_hover_text("Plays every replay on the new files (as voxcade check --refit).").clicked() && !*dry {
                        self.refit(refit);
                    } else if !matches!(state, Refit::Running(_)) && ui.button(open.0).clicked() {
                        wish = Some(open.1);
                    }
                }
                _ => {}
            }
            if !self.busy() && ui.button("BACK").on_hover_text("Back to the start page.").clicked() {
                wish = Some(Wish::Back);
            }
        });
        wish
    }
}

/// What the upgrade said, as the page shows it: without the line telling
/// the command that refits the replays (REFIT THE REPLAYS below does it;
/// the command stays in the docs and in `voxcade upgrade`'s words), and
/// the steps with nothing to change counted in one line where the first
/// of them stood.
fn shown_lines(lines: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let (mut idle, mut at) = (0, None);
    let mut lines = lines.iter().filter(|l| !(l.contains("voxcade check") && l.contains("--refit"))).peekable();
    while let Some(line) = lines.next() {
        if line.ends_with("):") && lines.peek().is_some_and(|next| next.trim() == "nothing to change") {
            lines.next();
            idle += 1;
            at.get_or_insert(out.len());
            continue;
        }
        out.push(line.clone());
    }
    if let (Some(at), Some(line)) = (at, idle_steps(idle)) {
        out.insert(at, line);
    }
    out
}

/// The quiet line for `n` steps that find nothing to change (None: none).
fn idle_steps(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("1 step has nothing to change".into()),
        n => Some(format!("{n} steps have nothing to change")),
    }
}

/// What keeps the game's files as they are before UPGRADE, as
/// `upgrade::upgrade` does: git where it holds every file, else a copy
/// made first. The label and its hover.
fn keeping(committed: bool) -> (&'static str, String) {
    if committed {
        ("GIT KEEPS THE OLD FILES", "Every file of the game is committed in git, so git can give back what the upgrade changes. No copy is made.".into())
    } else {
        let to = crate::settings::config_dir().join(crate::upgrade::KEPT);
        ("A COPY IS KEPT FIRST", format!("The game is not committed in git, so before anything changes its files are copied to {}.", to.display()))
    }
}

/// A line of what a step does, quiet, indented.
fn said(ui: &mut egui::Ui, line: &str) {
    egui::Frame::new().inner_margin(egui::Margin { left: 16, bottom: 2, ..egui::Margin::ZERO }).show(ui, |ui| ui.label(RichText::new(line).color(QUIET)));
}

/// A mistake in red, and COPY.
fn error(ui: &mut egui::Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(text).color(ERROR));
        copy_button(ui, text);
    });
}

fn copy_button(ui: &mut egui::Ui, text: &str) {
    if ui.small_button("COPY").on_hover_text("Copies it, to paste to someone who can help.").clicked() {
        ui.ctx().copy_text(text.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game of an older Voxcade: a MagicaVoxel figure named in its conf,
    /// a recording among its sounds.
    fn older_game(name: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(&format!("wizard-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        crate::vox::write_figure(&dir.join("robot.vox"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO), ("arm", glam::IVec3::new(1, 2, 5), glam::IVec3::new(6, 1, 3))]);
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.5.0\"\n  c.things.robot = { model = \"robot.vox\" }\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick() end\n").unwrap();
        dir
    }

    /// The page asks, UPGRADE brings the game up to date on its thread, the
    /// replays (none here) are refitted, and the game opens.
    #[test]
    fn the_wizard_brings_an_older_game_up_to_date() {
        let dir = older_game("run");
        assert!(needed(&dir));
        let mut w = Wizard::new(&dir, false);
        assert!(matches!(&w.standing, Ok(Standing::Older { made: Some(v), steps }) if v == "0.5.0" && !steps.is_empty()));
        w.upgrade();
        w.wait();
        let Stage::Upgraded { lines, dry: false, refit } = &w.stage else { panic!("not upgraded") };
        assert!(lines.iter().any(|l| l.contains("robot.vvx")), "{lines:?}");
        assert!(matches!(refit, Refit::Nothing), "no replays to refit");
        assert!(!needed(&dir) && dir.join("robot.vvx").is_file() && !dir.join("robot.vox").exists());
        crate::upgrade::opens(&dir).unwrap();
        // The page drawn: OPEN THE GAME is there.
        let ctx = egui::Context::default();
        let mut wish = None;
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| wish = w.ui(ui, crate::replay::refit_saying::<crate::Voxcade>));
        }
        assert_eq!(wish, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A dry run shows the page as after UPGRADE and changes nothing; a
    /// game with replays is offered the refit, which runs on its thread.
    #[test]
    fn a_dry_run_changes_nothing_and_replays_are_refitted() {
        let dir = older_game("dry");
        let mut w = Wizard::new(&dir, true);
        w.upgraded_dry();
        assert!(matches!(&w.stage, Stage::Upgraded { dry: true, .. }));
        assert!(dir.join("robot.vox").is_file() && needed(&dir), "nothing changed");
        // Brought up to date, a replay recorded, then refitted.
        crate::upgrade::upgrade(&dir, false).unwrap();
        std::fs::create_dir_all(dir.join("replays")).unwrap();
        let mut w = Wizard::new(&dir, false);
        w.stage = Stage::Upgraded { lines: Vec::new(), dry: false, refit: Refit::Offered(1) };
        w.refit(|_, say| {
            say("demo.toml: same".into());
            Ok(true)
        });
        w.wait();
        assert!(matches!(&w.stage, Stage::Upgraded { refit: Refit::Done(true, said), .. } if said == &["demo.toml: same"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The upgrade's line naming the command that refits the replays is
    /// not shown on the page, whose REFIT THE REPLAYS does the same; the
    /// other lines are.
    #[test]
    fn the_page_shows_no_command_beside_its_button() {
        // As the upgrade says it, of a game with a replay.
        let dir = older_game("command");
        std::fs::create_dir_all(dir.join("replays")).unwrap();
        std::fs::write(dir.join("replays/demo.toml"), "").unwrap();
        let lines = crate::upgrade::upgrade(&dir, true).unwrap();
        assert!(lines.iter().any(|l| l.contains("voxcade check")), "{lines:?}");
        let shown = shown_lines(&lines);
        assert!(!shown.iter().any(|l| l.contains("voxcade check")), "{shown:?}");
        assert!(shown.iter().any(|l| l.contains("robot.vvx")), "{shown:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The texts a frame of the page draws, at 1600 by 900.
    fn drawn(ctx: &egui::Context, w: &mut Wizard) -> Vec<String> {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))), ..Default::default() };
        let out = ctx.run_ui(raw, |ui| {
            let _ = w.ui(ui, crate::replay::refit_saying::<crate::Voxcade>);
        });
        out.shapes.iter().filter_map(|s| if let egui::Shape::Text(t) = &s.shape { Some(t.galley.text().to_string()) } else { None }).collect()
    }

    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                copy(&path, &to.join(entry.file_name()));
            } else {
                std::fs::copy(&path, to.join(entry.file_name())).unwrap();
            }
        }
    }

    /// Sokovox said to be made for 0.3: every step finds nothing, so the
    /// page names no step but counts them in one line, says that a copy is
    /// kept first (the folder is in no git repository), and after a dry
    /// UPGRADE counts the replays on a line of their own, no empty heading.
    #[test]
    fn the_page_shows_only_the_steps_that_change_something() {
        let root = crate::scratch::Scratch::new("wizard-page");
        let dir = root.join("sokovox");
        copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/sokovox"), &dir);
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap().replace("c.voxcade = \"0.6.0\"", "c.voxcade = \"0.3\"");
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        let mut w = Wizard::new(&dir, false);
        let Ok(Standing::Older { steps, .. }) = &w.standing else { panic!("older") };
        assert!(steps.len() > 1 && steps.iter().all(|s| s.would.is_empty()), "every step idle: {steps:?}");
        let n = steps.len();
        let ctx = egui::Context::default();
        let texts = (0..2).map(|_| drawn(&ctx, &mut w)).last().unwrap();
        let says = |word: &str| texts.iter().any(|t| t.contains(word));
        assert!(!says("nothing to change\n") && texts.iter().all(|t| t != "nothing to change"), "{texts:?}");
        assert!(says(&format!("{n} steps have nothing to change")), "{texts:?}");
        assert!(!says("ROTATION AND BEHAVIOR"), "an idle step is not named: {texts:?}");
        assert!(says("A COPY IS KEPT FIRST") && !says("GIT KEEPS"), "{texts:?}");
        assert!(says("UPGRADE"));
        // As after a dry UPGRADE.
        w.upgraded_dry();
        let texts = (0..2).map(|_| drawn(&ctx, &mut w)).last().unwrap();
        let says = |word: &str| texts.iter().any(|t| t.contains(word));
        assert!(!texts.iter().any(|t| t.trim() == "nothing to change"), "{texts:?}");
        assert!(says(&format!("{n} steps have nothing to change")), "{texts:?}");
        assert!(says("2 replays recorded on the old files") && !says("REPLAYS RECORDED ON THE OLD FILES"), "{texts:?}");
        assert!(says("REFIT THE REPLAYS"));
        // In a git repository with every file committed, git keeps them.
        assert_eq!(keeping(true).0, "GIT KEEPS THE OLD FILES");
    }

    /// The steps with nothing to change are counted where the first stood;
    /// a step that changes something keeps its lines.
    #[test]
    fn idle_steps_are_counted_in_one_line() {
        let lines: Vec<String> = ["note: x", "a (Voxcade 0.6.0):", "  nothing to change", "b (Voxcade 0.6.0):", "  robot.vox becomes robot.vvx", "c (Voxcade 0.6.0):", "  nothing to change", "conf.lua would say c.voxcade = \"0.6.0\""].map(String::from).to_vec();
        assert_eq!(shown_lines(&lines), ["note: x", "2 steps have nothing to change", "b (Voxcade 0.6.0):", "  robot.vox becomes robot.vvx", "conf.lua would say c.voxcade = \"0.6.0\""]);
        assert_eq!(idle_steps(1).as_deref(), Some("1 step has nothing to change"));
        assert_eq!(idle_steps(0), None);
    }

    /// A game made with a newer Voxcade is said to be so.
    #[test]
    fn a_newer_game_points_to_the_update() {
        let dir = older_game("newer");
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"99.0.0\"\nend\n").unwrap();
        assert!(needed(&dir));
        assert!(matches!(Wizard::new(&dir, false).standing, Ok(Standing::Newer { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
