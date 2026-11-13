//! Games to play and to work on: OPEN and NEW in the bar beside the game's
//! name, NEW's form, and the launcher (the program started without a
//! game, `StartPage`): the games that come with the program and the
//! user's own, each to PLAY or to EDIT, and the line about new versions
//! (`crate::update`). They share the games opened last and the projects
//! folder (`prefs.rs`); NEW begins a game from a template (`templates.rs`)
//! in the projects folder.

use std::path::{Path, PathBuf};

use egui::{RichText, TextStyle};

use std::sync::mpsc;

use crate::gui::{ACCENT, ERROR, GROUND, PANEL, QUIET, TEXT};
use crate::update::{self, Release};

use super::prefs::Prefs;
use super::templates::{self, TEMPLATES};

/// What the user chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wish {
    /// A game's folder, to open.
    Open(PathBuf),
    /// The system's folder dialog, to pick a game's folder.
    Browse,
    /// The system's folder dialog, to pick where NEW puts games.
    BrowseProjects,
    /// A game made for another Voxcade, to bring up to date (`wizard`),
    /// then to play (PLAY) or to open.
    Upgrade(PathBuf, bool),
}

/// Whether `dir` holds a game (its `conf.lua`).
pub fn is_game(dir: &Path) -> bool {
    dir.join("conf.lua").is_file()
}

/// How many folders below a folder that is no game its games are looked for.
const SEARCH_DEPTH: usize = 3;

/// Folders no search for games goes into, whatever the `.gitignore` says
/// (hidden ones too).
const NOT_SEARCHED: [&str; 4] = ["target", "node_modules", "shots", "builds"];

/// The games below `folder`: folders with a `conf.lua` up to
/// `SEARCH_DEPTH` levels down, by their paths. Hidden folders, `NOT_SEARCHED`
/// and what the `.gitignore` files ignore are skipped, and a game's own
/// folders are not searched.
pub fn find_games(folder: &Path) -> Vec<PathBuf> {
    let root = folder.canonicalize().unwrap_or_else(|_| folder.to_path_buf());
    let mut found = Vec::new();
    search(&root, "", 1, &crate::gitignore::Ignore::for_game(&root), &mut found);
    found.sort();
    found
}

fn search(root: &Path, prefix: &str, depth: usize, ignore: &crate::gitignore::Ignore, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root.join(prefix)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || NOT_SEARCHED.contains(&name.as_str()) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if ignore.ignores(&path, true) {
            continue;
        }
        let dir = root.join(&path);
        if is_game(&dir) {
            found.push(dir);
        } else if depth < SEARCH_DEPTH {
            search(root, &path, depth + 1, &ignore.entered(&path), found);
        }
    }
}

/// What opening a folder comes to.
#[derive(Debug, PartialEq, Eq)]
pub enum Opening {
    /// The folder is a game.
    Here,
    /// It is not, but one game lies below it: that one.
    Only(PathBuf),
    /// Several games lie below it, to choose from.
    Several(Vec<PathBuf>),
    /// No game at all.
    Nothing,
}

/// What opening `folder` comes to (`find_games` when it is no game).
pub fn opening(folder: &Path) -> Opening {
    if is_game(folder) {
        return Opening::Here;
    }
    let mut games = find_games(folder);
    match games.len() {
        0 => Opening::Nothing,
        1 => Opening::Only(games.remove(0)),
        _ => Opening::Several(games),
    }
}

/// The games opened last (but `current`), as rows under their heading;
/// the folder of the one chosen.
pub fn recent(ui: &mut egui::Ui, prefs: &Prefs, current: Option<&Path>) -> Option<PathBuf> {
    let recent = prefs.recent_games(current);
    if recent.is_empty() {
        return None;
    }
    let mut chosen = None;
    ui.label(RichText::new("OPENED LAST").small().color(QUIET));
    for r in recent {
        let path = r.dir.display().to_string();
        if crate::gui::row(ui, &r.title, TextStyle::Body, TEXT, false, Some(&short(&r.dir))).on_hover_text(&path).clicked() {
            chosen = Some(r.dir.clone());
        }
    }
    chosen
}

/// `dir` short: its last two folders.
fn short(dir: &Path) -> String {
    let parts: Vec<_> = dir.components().rev().take(2).map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    format!("…/{}", parts.into_iter().rev().collect::<Vec<_>>().join("/"))
}

/// NEW's form while it is open: the game's name and its template.
pub struct NewGame {
    name: String,
    template: usize,
    /// The tick: the game's notes for the agent go into it (ticked at
    /// first when the settings show the console).
    notes: bool,
    trouble: Option<String>,
}

/// What came of NEW's form this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Form {
    Open,
    Cancelled,
    /// The new game's folder.
    Created(PathBuf),
}

impl NewGame {
    /// The form as the settings leave it.
    pub fn new(prefs: &Prefs) -> Self {
        Self { name: String::new(), template: 0, notes: prefs.console, trouble: None }
    }

    /// The form over everything; the new game goes into `projects`, with
    /// the agent's notes if ticked (the agent the settings name).
    pub fn ui(&mut self, ctx: &egui::Context, projects: &Path, prefs: &Prefs) -> Form {
        let mut form = Form::Open;
        let modal = egui::Modal::new(egui::Id::unique("new game")).frame(egui::Frame::popup(&ctx.global_style()).fill(crate::gui::PANEL).inner_margin(egui::Margin::same(20))).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new("NEW GAME").color(ACCENT));
            ui.add_space(10.0);
            ui.label(RichText::new("NAME").small().color(QUIET));
            let field = ui.add(egui::TextEdit::singleline(&mut self.name).hint_text("My Game").desired_width(f32::INFINITY));
            field.request_focus();
            let folder = if self.name.trim().is_empty() { projects.display().to_string() } else { templates::folder_for(projects, &self.name).display().to_string() };
            ui.label(RichText::new(format!("in {folder}")).small().color(QUIET));
            ui.add_space(10.0);
            ui.label(RichText::new("TEMPLATE").small().color(QUIET));
            // What each is, under its name: the circle alone on the left.
            let indent = (ui.spacing().icon_width + ui.spacing().icon_spacing) as i8;
            for (i, t) in TEMPLATES.iter().enumerate() {
                ui.radio_value(&mut self.template, i, t.name);
                egui::Frame::new().inner_margin(egui::Margin { left: indent, ..egui::Margin::ZERO }).show(ui, |ui| ui.label(RichText::new(t.about).small().color(QUIET)));
                ui.add_space(4.0);
            }
            ui.checkbox(&mut self.notes, format!("WITH {}", prefs.agent.name())).on_hover_text(format!(
                "Puts a file {} into the game, with the game's own notes for {}: what it is, what was decided, how you work on it. Nothing in Voxcade needs it.",
                prefs.agent.notes_file(),
                prefs.agent.called()
            ));
            if let Some(trouble) = &self.trouble {
                ui.add_space(6.0);
                ui.label(RichText::new(trouble).color(ERROR));
            }
            ui.add_space(10.0);
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                if ui.button("CREATE").clicked() || enter {
                    match templates::create(&TEMPLATES[self.template], projects, &self.name, self.notes.then_some(prefs.agent)) {
                        // What git did (`begun.git.said()`) has no place to be said yet.
                        Ok(begun) => form = Form::Created(begun.dir),
                        Err(e) => self.trouble = Some(format!("{e:#}")),
                    }
                }
                if ui.button("CANCEL").clicked() {
                    form = Form::Cancelled;
                }
            });
        });
        if form == Form::Open && modal.should_close() {
            form = Form::Cancelled;
        }
        form
    }
}

/// A game the launcher lists.
struct Entry {
    dir: PathBuf,
    title: String,
    /// Its picture (`cover.png` in its folder, 16 to 9), once looked for.
    cover: Option<Option<egui::TextureHandle>>,
}

impl Entry {
    fn new(dir: PathBuf) -> Self {
        Self { title: title_of(&dir), dir, cover: None }
    }

    /// The game's picture, read when first shown.
    fn cover(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        self.cover
            .get_or_insert_with(|| {
                let (w, h, pixels) = crate::map::read_png_rgba(&self.dir.join("cover.png")).ok()?;
                let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], pixels.as_flattened());
                Some(ctx.load_texture(self.dir.display().to_string(), image, egui::TextureOptions::LINEAR))
            })
            .clone()
    }
}

/// The logo over the launcher, the engine's own picture of its name.
fn logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("logo.png").as_slice()));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    (info.color_type == png::ColorType::Rgba).then(|| ctx.load_texture("voxcade logo", egui::ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &pixels[..info.buffer_size()]), egui::TextureOptions::LINEAR))
}

/// What was asked of a game's card.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asked {
    Play,
    Edit,
}

/// A card's least width and the room between cards.
const CARD: f32 = 250.0;
const GAP: f32 = 16.0;

/// A game's title as its `conf.lua` sets it (`c.title = "..."`), read
/// without running it; else its folder's name.
pub(crate) fn title_of(dir: &Path) -> String {
    let named = std::fs::read_to_string(dir.join("conf.lua")).ok().and_then(|conf| {
        let line = conf.lines().find(|l| l.trim_start().starts_with("c.title"))?;
        let rest = line.split_once('=')?.1.trim_start();
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        Some(rest[1..].split(quote).next()?.to_string())
    });
    named.filter(|t| !t.is_empty()).unwrap_or_else(|| dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned()))
}

/// The games in `folder`, by their titles.
fn games_in(folder: &Path) -> Vec<Entry> {
    let mut games: Vec<Entry> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|dir| is_game(dir))
        .map(|dir| Entry::new(dir.canonicalize().unwrap_or(dir)))
        .collect();
    games.sort_by_key(|g| g.title.to_lowercase());
    games
}

/// Where the launcher stands with new versions.
enum Update {
    /// Not looked: switched off, or a picture drawn without a window.
    Idle,
    Looking(mpsc::Receiver<Result<Option<Release>, String>>),
    Newest,
    There(Release),
    Fetching(Release, mpsc::Receiver<Fetch>, f32),
    Failed(String),
}

enum Fetch {
    Share(f32),
    Done(Result<(), String>),
}

/// The launcher, the program opened without a game: the games that come
/// with it and the user's own, each to PLAY or EDIT, NEW GAME, OPEN A
/// FOLDER, and whether a new version is there.
pub struct StartPage {
    pub prefs: Prefs,
    new: Option<NewGame>,
    /// What went wrong opening one.
    message: Option<String>,
    /// The games that come with the program, then the user's own (the
    /// projects folder's and those opened last).
    bundled: Vec<Entry>,
    own: Vec<Entry>,
    /// A game being played from here, in a window of its own.
    playing: Option<(String, std::process::Child)>,
    update: Update,
    logo: Option<Option<egui::TextureHandle>>,
    /// The games found below a folder that is none, to choose from.
    found: Option<(PathBuf, Vec<Entry>)>,
}

impl StartPage {
    /// The launcher; `look`: whether to look for a new version now (if
    /// the settings allow it).
    pub fn new(prefs: Prefs, message: Option<String>, look: bool) -> Self {
        // Of the games beside the program, those with a picture: the ones
        // made to be shown (a test game in the source has none).
        let mut bundled = update::games_dir().map(|d| games_in(&d)).unwrap_or_default();
        bundled.retain(|g| g.dir.join("cover.png").is_file());
        let mut page = Self { prefs, new: None, message, bundled, own: Vec::new(), playing: None, update: Update::Idle, logo: None, found: None };
        page.find_own();
        if look && page.prefs.updates {
            page.look();
        }
        page
    }

    /// The user's own games anew: the projects folder's, then those
    /// opened last that lie elsewhere.
    fn find_own(&mut self) {
        let mut own = games_in(&self.prefs.projects_dir());
        for r in self.prefs.recent_games(None) {
            let dir = r.dir.canonicalize().unwrap_or_else(|_| r.dir.clone());
            if is_game(&dir) && !self.bundled.iter().chain(&own).any(|g| g.dir == dir) {
                own.push(Entry::new(dir));
            }
        }
        self.own = own;
    }

    /// NEW GAME puts games in `dir` from now on, and its games are listed.
    pub fn set_projects(&mut self, dir: PathBuf) {
        self.prefs.projects = Some(dir);
        if let Err(e) = self.prefs.save(&Prefs::path()) {
            self.message = Some(format!("The settings: {e:#}"));
        }
        self.find_own();
    }

    /// NEW GAME's form open with `name` typed and JUMP & RUN chosen, as a
    /// picture of it wants it (`voxcade ide --shot FILE --open new:NAME`).
    pub fn show_new(&mut self, name: &str) {
        let template = TEMPLATES.iter().position(|t| t.name == "JUMP & RUN").unwrap_or(0);
        self.new = Some(NewGame { name: name.to_string(), template, notes: self.prefs.console, trouble: None });
    }

    /// The games found below `folder`, which is none, listed first.
    pub fn show_found(&mut self, folder: PathBuf, games: Vec<PathBuf>) {
        self.found = Some((folder, games.into_iter().map(Entry::new).collect()));
    }

    /// Says what went wrong.
    pub fn tell(&mut self, message: String) {
        self.message = Some(message);
    }

    fn look(&mut self) {
        let (tell, heard) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tell.send(update::look().map_err(|e| format!("{e:#}")));
        });
        self.update = Update::Looking(heard);
    }

    fn fetch(&mut self, release: Release, dir: PathBuf) {
        let (tell, heard) = mpsc::channel();
        let wanted = release.clone();
        std::thread::spawn(move || {
            let share = tell.clone();
            let done = update::install(&wanted, &dir, &move |s| {
                let _ = share.send(Fetch::Share(s));
            });
            let _ = tell.send(Fetch::Done(done.map_err(|e| format!("{e:#}"))));
        });
        self.update = Update::Fetching(release, heard, 0.0);
    }

    /// Plays the game in `dir` in a window of its own (this program
    /// started once more), the launcher waiting behind it.
    pub fn play(&mut self, dir: &Path, title: &str) {
        // A game made for another Voxcade does not open: said here, not
        // in a window that would close at once.
        if let Err(e) = crate::upgrade::opens(dir) {
            self.message = Some(format!("{e:#}"));
            return;
        }
        let started = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg("run").arg(dir).spawn());
        match started {
            Ok(child) => {
                self.message = None;
                self.playing = Some((title.to_string(), child));
            }
            Err(e) => self.message = Some(format!("{title} would not start: {e}")),
        }
    }

    /// What the threads and the game played said since the last frame.
    fn hear(&mut self) {
        if let Some((title, child)) = &mut self.playing
            && let Ok(Some(status)) = child.try_wait()
        {
            if !status.success() {
                self.message = Some(format!("{title} ended with a mistake."));
            }
            self.playing = None;
        }
        match &mut self.update {
            Update::Looking(heard) => match heard.try_recv() {
                Ok(Ok(Some(release))) => self.update = Update::There(release),
                Ok(Ok(None)) => self.update = Update::Newest,
                // No way to the site is no trouble worth a red line.
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => self.update = Update::Idle,
                Err(mpsc::TryRecvError::Empty) => {}
            },
            Update::Fetching(_, heard, share) => {
                let mut done = None;
                while let Ok(step) = heard.try_recv() {
                    match step {
                        Fetch::Share(s) => *share = s,
                        Fetch::Done(result) => done = Some(result),
                    }
                }
                match (done, update::install_dir()) {
                    // In place: the new program takes over.
                    (Some(Ok(())), Some(dir)) => update::restart(&dir),
                    (Some(Ok(())), None) => self.update = Update::Newest,
                    (Some(Err(e)), _) => self.update = Update::Failed(e),
                    (None, _) => {}
                }
            }
            _ => {}
        }
    }

    /// The games as cards in rows that fill the width,
    /// each its picture, its title, PLAY and EDIT; what was asked of one.
    fn cards(ui: &mut egui::Ui, games: &mut [Entry], can_play: bool) -> Option<(usize, Asked)> {
        let mut asked = None;
        let room = ui.available_width();
        let columns = (((room + GAP) / (CARD + GAP)).floor() as usize).clamp(1, 4);
        let width = ((room - GAP * (columns - 1) as f32) / columns as f32).floor();
        let picture = egui::vec2(width, (width * 9.0 / 16.0).round());
        let count = games.len();
        for row in 0..count.div_ceil(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for (i, game) in games.iter_mut().enumerate().skip(row * columns).take(columns) {
                    let cover = game.cover(ui.ctx());
                    egui::Frame::new().fill(PANEL).corner_radius(6.0).show(ui, |ui| {
                        ui.set_width(width);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            // The picture plays the game too.
                            let (rect, shown) = ui.allocate_exact_size(picture, if can_play { egui::Sense::click() } else { egui::Sense::hover() });
                            let corners = egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 };
                            match &cover {
                                Some(texture) => {
                                    egui::Image::new(egui::load::SizedTexture::new(texture.id(), picture)).corner_radius(corners).paint_at(ui, rect);
                                }
                                // No picture: the game's name, big.
                                None => {
                                    ui.painter().rect_filled(rect, corners, crate::gui::ROW_OPEN);
                                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, &game.title, TextStyle::Heading.resolve(ui.style()), ACCENT);
                                }
                            }
                            if shown.hovered() && can_play {
                                ui.painter().rect_stroke(rect, corners, egui::Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if shown.on_hover_text("Plays the game in a window of its own.").clicked() {
                                asked = Some((i, Asked::Play));
                            }
                            egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                                ui.allocate_ui_with_layout(egui::vec2(width - 20.0, ui.spacing().interact_size.y + 4.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    ui.add(egui::Label::new(RichText::new(&game.title).color(TEXT)).truncate()).on_hover_text(game.dir.display().to_string());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button("EDIT").on_hover_text("Opens the game in the IDE: its code, models and levels, to change it.").clicked() {
                                            asked = Some((i, Asked::Edit));
                                        }
                                        if ui.add_enabled(can_play, egui::Button::new(RichText::new("PLAY").color(ACCENT))).on_hover_text("Plays the game in a window of its own.").clicked() {
                                            asked = Some((i, Asked::Play));
                                        }
                                    });
                                });
                            });
                        });
                    });
                }
            });
            ui.add_space(GAP);
        }
        ui.add_space(6.0);
        asked
    }

    /// This version and new ones, in the foot's right half (laid out
    /// from the right: what is added first stands rightmost).
    fn versions(&mut self, ui: &mut egui::Ui) {
        let mut looks = self.prefs.updates;
        if ui.checkbox(&mut looks, RichText::new("Look for new versions").small().color(QUIET)).on_hover_text("When Voxcade starts, it asks voxca.de for a small file that names the newest version. Nothing is fetched or changed until you say so.").changed() {
            self.prefs.updates = looks;
            if let Err(e) = self.prefs.save(&Prefs::path()) {
                self.message = Some(format!("The settings: {e:#}"));
            }
            if looks && matches!(self.update, Update::Idle) {
                self.look();
            }
        }
        ui.add_space(10.0);
        let (mut fetch, mut again) = (None, false);
        let quiet = |ui: &mut egui::Ui, text: String| {
            ui.label(RichText::new(text).small().color(QUIET));
        };
        match &self.update {
            Update::Idle => quiet(ui, format!("Version {}", update::VERSION)),
            Update::Looking(_) => quiet(ui, format!("Version {} · looking for a new one …", update::VERSION)),
            Update::Newest => quiet(ui, format!("Version {} · the newest", update::VERSION)),
            Update::There(release) => {
                if ui.button("WHAT IS NEW").on_hover_text(&release.notes).clicked() {
                    update::show(&release.notes);
                }
                if update::install_dir().is_some() {
                    let about = format!("Fetches version {}, checks it and puts it in place of this one. What is here now goes to the folder before-update, your own games stay. Voxcade then starts anew.", release.version);
                    if ui.add_enabled(self.playing.is_none(), egui::Button::new(RichText::new("UPDATE NOW").color(ACCENT))).on_hover_text(about).clicked() {
                        fetch = Some(release.clone());
                    }
                } else if ui.button("GET IT AT VOXCA.DE").clicked() {
                    update::show("https://voxca.de/#download");
                }
                ui.label(RichText::new(format!("VERSION {} IS THERE", release.version)).color(ACCENT)).on_hover_text(format!("This is version {}.", update::VERSION));
            }
            Update::Fetching(release, _, share) => {
                ui.add(egui::ProgressBar::new(share.clamp(0.0, 1.0)).desired_width(140.0));
                ui.label(RichText::new(format!("Fetching version {} …", release.version)).color(ACCENT));
            }
            Update::Failed(why) => {
                if ui.button("TRY AGAIN").clicked() {
                    again = true;
                }
                ui.add(egui::Label::new(RichText::new(format!("The new version did not come: {why}")).color(ERROR)).truncate()).on_hover_text(why);
            }
        }
        if let (Some(release), Some(dir)) = (fetch, update::install_dir()) {
            self.fetch(release, dir);
        } else if again {
            self.look();
        }
    }

    /// A frame of it; what was chosen.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Wish> {
        self.hear();
        if self.playing.is_some() || matches!(self.update, Update::Looking(_) | Update::Fetching(..)) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
        let mut wish = None;
        let logo = self.logo.get_or_insert_with(|| logo(ui.ctx())).clone();
        // Below: what to make, and the versions; above them the games.
        egui::Panel::bottom("launcher foot").frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(28, 12))).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("NEW GAME").on_hover_text("Begins a game of your own from a template, in the projects folder.").clicked() {
                    self.new = Some(NewGame::new(&self.prefs));
                }
                if ui.button("OPEN A FOLDER").on_hover_text("Opens a game's folder (the one with its conf.lua) in the IDE.").clicked() {
                    wish = Some(Wish::Browse);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.versions(ui));
            });
            if let Some(message) = &self.message {
                ui.add_space(6.0);
                ui.label(RichText::new(message).color(ERROR));
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(GROUND)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // As wide as four cards at most, in the middle, room at the
                // sides; the scroll bar at the window's edge.
                let room = ui.available_width() - 56.0;
                let width = room.min(4.0 * (CARD + 60.0));
                let side = 28.0 + ((room - width) / 2.0).floor();
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.add_space(26.0);
                        ui.horizontal(|ui| {
                            match &logo {
                                Some(texture) => {
                                    let size = texture.size_vec2() * (44.0 / texture.size_vec2().y);
                                    ui.add(egui::Image::new(egui::load::SizedTexture::new(texture.id(), size)));
                                }
                                None => {
                                    ui.label(RichText::new("VOXCADE").heading().color(ACCENT));
                                }
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if let Some((title, _)) = &self.playing {
                                    let say = format!("PLAYING {} …", title.to_uppercase());
                                    ui.label(RichText::new(say).color(QUIET));
                                }
                            });
                        });
                        ui.add_space(22.0);
                        let can_play = self.playing.is_none();
                        let mut asked = None;
                        if let Some((folder, games)) = &mut self.found {
                            ui.label(RichText::new(format!("GAMES IN {}", short(folder).to_uppercase())).small().color(QUIET)).on_hover_text(folder.display().to_string());
                            ui.add_space(4.0);
                            if let Some((i, what)) = Self::cards(ui, games, can_play) {
                                asked = Some((games[i].dir.clone(), games[i].title.clone(), what));
                            }
                        }
                        if !self.bundled.is_empty() {
                            ui.label(RichText::new("GAMES").small().color(QUIET));
                            ui.add_space(4.0);
                            if let Some((i, what)) = Self::cards(ui, &mut self.bundled, can_play) {
                                asked = Some((self.bundled[i].dir.clone(), self.bundled[i].title.clone(), what));
                            }
                        }
                        // Your games, and beside the heading where they
                        // lie and the way to another folder.
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("YOUR GAMES").small().color(QUIET));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("CHANGE FOLDER").on_hover_text("Chooses another folder for your games: NEW GAME puts games there, and the games in it are listed here.").clicked() {
                                    wish = Some(Wish::BrowseProjects);
                                }
                                ui.add(egui::Label::new(RichText::new(format!("in {}", self.prefs.projects_dir().display())).small().color(QUIET)).truncate());
                            });
                        });
                        ui.add_space(4.0);
                        if self.own.is_empty() {
                            ui.add_space(GAP);
                        } else if let Some((i, what)) = Self::cards(ui, &mut self.own, can_play) {
                            asked = Some((self.own[i].dir.clone(), self.own[i].title.clone(), what));
                        }
                        match asked {
                            // A game made for another Voxcade: the upgrade's
                            // page first.
                            Some((dir, _, Asked::Play)) if super::wizard::needed(&dir) => wish = Some(Wish::Upgrade(dir, true)),
                            Some((dir, title, Asked::Play)) => self.play(&dir, &title),
                            Some((dir, _, Asked::Edit)) => wish = Some(Wish::Open(dir)),
                            None => {}
                        }
                    });
                });
            });
        });
        if let Some(new) = &mut self.new {
            match new.ui(ui.ctx(), &self.prefs.projects_dir(), &self.prefs) {
                Form::Open => {}
                Form::Cancelled => self.new = None,
                Form::Created(dir) => {
                    self.new = None;
                    wish = Some(Wish::Open(dir));
                }
            }
        }
        wish
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of `files` (paths with their text), in a scratch folder.
    fn tree(name: &str, files: &[(&str, &str)]) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(name);
        for (path, text) in files {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        dir
    }

    fn names(root: &Path, games: &[PathBuf]) -> Vec<String> {
        let root = root.canonicalize().unwrap();
        games.iter().map(|g| g.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/")).collect()
    }

    /// Games lie up to three folders down; deeper ones, hidden folders,
    /// `target`, `node_modules`, `shots`, `builds` and what the
    /// `.gitignore` ignores are not looked into, nor a game's own folders.
    #[test]
    fn games_are_found_three_folders_down_and_the_skips_hold() {
        let dir = tree(
            "find-games",
            &[
                ("games/arena/conf.lua", ""),
                ("games/arena/levels/inner/conf.lua", ""),
                ("a/b/c/deep/conf.lua", ""),
                ("one/two/three/conf.lua", ""),
                (".hidden/game/conf.lua", ""),
                ("target/game/conf.lua", ""),
                ("node_modules/game/conf.lua", ""),
                ("shots/game/conf.lua", ""),
                ("builds/game/conf.lua", ""),
                ("old/game/conf.lua", ""),
                ("tools/old/conf.lua", ""),
                ("tools/kept/conf.lua", ""),
                (".gitignore", "old/\ntools/old\n"),
            ],
        );
        let found = find_games(&dir);
        assert_eq!(names(&dir, &found), ["games/arena", "one/two/three", "tools/kept"]);
    }

    /// A folder that is a game opens as it is; one game below it opens
    /// that; several are listed; none says so.
    #[test]
    fn opening_a_folder_comes_to_one_of_four_things() {
        let game = tree("open-here", &[("conf.lua", ""), ("sub/conf.lua", "")]);
        assert_eq!(opening(&game), Opening::Here);
        let one = tree("open-one", &[("x/y/conf.lua", "")]);
        let Opening::Only(game) = opening(&one) else { panic!("not the only game") };
        assert_eq!(names(&one, &[game]), ["x/y"]);
        let many = tree("open-many", &[("games/a/conf.lua", ""), ("games/b/conf.lua", "")]);
        assert!(matches!(opening(&many), Opening::Several(g) if names(&many, &g) == ["games/a", "games/b"]));
        let none = tree("open-none", &[("notes/readme.txt", "")]);
        assert_eq!(opening(&none), Opening::Nothing);
    }

    /// The start page draws the games found below a folder, and the
    /// message when there are none.
    #[test]
    fn the_start_page_lists_found_games() {
        let many = tree("found-page", &[("games/a/conf.lua", ""), ("games/b/conf.lua", "")]);
        let mut page = StartPage::new(Prefs::default(), None, false);
        page.show_found(many.to_path_buf(), find_games(&many));
        assert_eq!(page.found.as_ref().map(|(_, g)| g.len()), Some(2));
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), ..Default::default() }, |ui| {
                page.ui(ui);
            });
        }
    }
}
