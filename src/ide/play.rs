//! Play in the IDE's main area: the game straight from its folder, no
//! title and no menus, stepped at the simulation's fixed tick and drawn
//! into a texture of its own on the IDE's GPU, as big as the area. Its
//! sounds and music come out of the player's speaker, its keys and pads
//! are bound as the player set them (`controls::read_input`, as `voxcade
//! run` reads them). It holds still while the main area shows a file or
//! the window is elsewhere; a file saved comes in at once (hot reload),
//! and a script's error halts it until mended (`trouble`). A local of the
//! game's that changes while it plays, state no snapshot keeps, is named
//! in OUTPUT within a second (`lua::hidden::Watch`). Its runs are
//! recorded as `voxcade run` records them (`replay::Runs`: kept in the
//! config folder from 10 s on, F9 puts one in the game's `replays/`; a
//! run with a file saved during it is not kept).

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

use crate::controls::{Device, PadStyle};
use crate::game::Game;
use crate::saves::Slots;
use crate::menu::Menu;
use crate::render::Renderer;
use crate::replay::Runs;
use crate::session::{Reloaded, Scene, Session};
use crate::settings::Settings;
use crate::sim::DT;
use crate::watch::FolderWatch;

/// The format it is drawn in: the games' own, as the headless canvas's.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Ticks a frame may catch up before time is dropped (as the game's).
const MAX_CATCH_UP: u32 = 5;

/// What the game is drawn into, and its name to egui.
struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
    id: egui::TextureId,
}

/// A game being played.
pub struct Play<G: Game> {
    dir: PathBuf,
    game: Session<G>,
    renderer: Renderer,
    mask: crate::mesh::FaceMask,
    target: Option<Target>,
    settings: Settings,
    watch: FolderWatch,
    speaker: Option<crate::audio::Speaker>,
    runs: Runs,
    /// Files that would not load since last asked (for OUTPUT).
    failed: Vec<String>,
    /// The files that would not load when they changed, by name, with what
    /// was said; a file leaves when it loads again or is gone.
    unloaded: std::collections::BTreeMap<String, String>,
    /// Time not yet stepped, and when the last frame was (None: held, so
    /// it goes on without catching up).
    accumulator: f32,
    last: Option<Instant>,
    /// The scenario every run of it begins from (PLAY SCENARIO; F5 too).
    scenario: Option<String>,
    /// Who plays on which seat (`c.players`): pads and the keyboard's
    /// halves, as in a window of the game.
    seating: crate::seating::Seating,
    /// How the run being played began (`Session::start_mark`).
    mark: u64,
    /// What the play says of itself, for OUTPUT (`said`).
    notes: Vec<String>,
    /// The hidden-state guard: the locals the game's functions keep,
    /// looked at every second (`lua::hidden::Watch`), and what it said of
    /// those that changed, for OUTPUT.
    guard: crate::lua::hidden::Watch,
    hidden: Vec<String>,
}

impl<G: Game> Play<G> {
    /// The game in `dir`, its sound through `speaker` (none in tests),
    /// begun from `scenario` if one is given.
    pub fn start(dir: &Path, device: &wgpu::Device, queue: &wgpu::Queue, speaker: Option<crate::audio::Speaker>, scenario: Option<&str>) -> Result<Self> {
        let mut game = Session::<G>::load_with(dir, Slots::on_disk(dir))?;
        begin(&mut game, scenario)?;
        let mut settings = Settings::load(&crate::settings::path_for(dir));
        settings.adopt(game.actions(), &game.default_keys(), &game.default_pad());
        settings.offer_languages(&game.cfg.languages);
        settings.system_languages = crate::settings::system_languages();
        crate::controls::label_keys(&mut settings);
        game.options = settings.options();
        let mut renderer = Renderer::new(device, FORMAT, 64, 64);
        let mask = game.face_mask();
        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
        let mut runs = Runs::new(dir);
        runs.start(&game);
        let seating = crate::seating::Seating::new(game.cfg.players);
        let mark = game.start_mark();
        let mut play = Self { dir: dir.to_path_buf(), game, renderer, mask, target: None, settings, watch: FolderWatch::new(dir), speaker, runs, failed: Vec::new(), unloaded: Default::default(), accumulator: 0.0, last: None, scenario: scenario.map(str::to_string), seating, mark, notes: Vec::new(), guard: Default::default(), hidden: Vec::new() };
        play.guard.begin(play.game.game.hidden());
        play.music();
        Ok(play)
    }

    /// The game from its files again (F5), as it starts.
    pub fn restart(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
        let mut fresh = Session::<G>::load_with(&self.dir, Slots::on_disk(&self.dir))?;
        begin(&mut fresh, self.scenario.as_deref())?;
        self.adopt(fresh, device, queue);
        Ok(())
    }

    /// `fresh`, a game just begun, plays in place of the one played.
    fn adopt(&mut self, fresh: Session<G>, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.runs.end(&self.game);
        self.game = fresh;
        self.mark = self.game.start_mark();
        self.guard.begin(self.game.game.hidden());
        self.runs.start(&self.game);
        self.game.options = self.settings.options();
        self.mask = self.game.face_mask();
        self.renderer.rebuild_all(device, queue, self.game.world_mut(), &self.mask);
        self.music();
    }

    /// A console line run in the game between two ticks (the line under
    /// OUTPUT, the agent's `eval`): what it answered. It is part of the run.
    pub fn eval(&mut self, line: &str) -> Result<String> {
        self.game.eval(line)
    }

    /// The player's song for the game.
    fn music(&mut self) {
        let songs = self.game.cfg.audio.music.files().len();
        self.game.select_song(self.settings.song.min(songs.saturating_sub(1)));
        self.game.music_changed = true;
    }

    /// What seat 1 holds now, the other seats' set on the game for the
    /// ticks to come (`keys`: the keyboard, while the game has it).
    pub fn read(&mut self, keys: Option<&dyn crate::controls::KeyState>, pads: &[sdl3::gamepad::Gamepad]) -> crate::input::Input {
        use crate::seating::Keys;
        let seating = &self.seating;
        if seating.alone() {
            return crate::controls::read_input(keys, pads, &self.settings, self.game.actions());
        }
        let of_seat = |seat: usize| pads.iter().filter(|p| p.id().ok().and_then(|id| seating.pad_seat(id.raw())) == Some(seat)).collect::<Vec<_>>();
        for seat in 2..=seating.seats() {
            let input = crate::controls::read_seat(keys, seating.keys(seat), &of_seat(seat), self.settings.pad_of(seat), &self.settings, self.game.actions());
            let controls = match seating.keys(seat) {
                Keys::Whole => crate::controls::Controls::of(self.game.actions(), Device::Keyboard, PadStyle::default(), &self.settings),
                Keys::Nothing => crate::controls::Controls::of_pad(self.game.actions(), PadStyle::default(), self.settings.pad_of(seat), &self.settings),
                half => crate::controls::Controls::of_half(self.game.actions(), half, &self.settings),
            };
            self.game.set_seat(seat, seating.taken(seat).then_some(input));
            self.game.set_seat_controls(seat, controls);
        }
        crate::controls::read_seat(keys, seating.keys(1), &of_seat(1), &self.settings.pad, &self.settings, self.game.actions())
    }

    /// The keys of seat 1 as the window's controls menu would show them
    /// (`keys_of`): the keyboard's or the pad's, as that seat plays here.
    pub fn keys(&self) -> Vec<(String, String)> {
        keys_of(self.game.actions(), self.seating.keys(1), &self.settings)
    }

    /// A pad's START: a seat taken (a pad no seat has), else nothing.
    pub fn pad_start(&mut self, pad: u32, plugged: &[u32]) {
        if !self.seating.alone()
            && let crate::seating::Start::Join(seat) = self.seating.pad_start(pad, plugged, None)
        {
            self.game.set_seat(seat, Some(crate::input::Input::default()));
        }
    }

    /// The keyboard's start key (unless the game binds it): a seat taken,
    /// the keyboard shared (seat 1 plays on it here).
    pub fn keyboard_start(&mut self) {
        let name = crate::seating::KEYBOARD_START;
        if !self.seating.alone()
            && self.settings.keys.action_of(name).is_none()
            && let crate::seating::Start::Join(seat) = self.seating.keyboard_start(None)
        {
            self.game.set_seat(seat, Some(crate::input::Input::default()));
        }
    }

    /// What stops the game: a script's error (its file and line in it) or
    /// a file that would not load.
    pub fn trouble(&self) -> Option<String> {
        self.game.trouble()
    }

    /// Holds it still (the main area shows a file, the window is
    /// elsewhere): no ticks, no sound.
    pub fn hold(&mut self) {
        if self.last.take().is_some()
            && let Some(s) = &self.speaker
        {
            s.hold(true);
        }
    }

    /// What was saved since comes in (hot reload; held too, so a file
    /// mended takes away the error that stopped it): whether anything did.
    pub fn reload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let changed = self.watch.poll();
        let mut rebuild = false;
        let mut took = false;
        let mut failed = false;
        for path in &changed {
            let name = path.strip_prefix(&self.dir).unwrap_or(path).display().to_string();
            let mut this_failed = false;
            match self.game.reload(path) {
                Ok(Reloaded::World) => (rebuild, took) = (true, true),
                Ok(Reloaded::Nothing) => {}
                Ok(_) => took = true,
                Err(e) => {
                    (failed, this_failed) = (true, true);
                    let said = format!("{name} would not load: {e:#}");
                    self.failed.push(said.clone());
                    self.unloaded.insert(name.clone(), said);
                }
            }
            // Loaded again (or gone): no longer in the way.
            if !this_failed || !path.exists() {
                self.unloaded.remove(&name);
            }
        }
        // What a level is built from is built when it begins: if the game
        // would begin otherwise now, it begins anew (a number of the rules
        // alone goes on).
        if took && !failed && self.begins_otherwise(device, queue) {
            return true;
        }
        // Code taken in: its functions keep locals of their own.
        if took {
            self.guard.reloaded(self.game.game.hidden());
        }
        if rebuild {
            self.game.options = self.settings.options();
            self.mask = self.game.face_mask();
            self.renderer.rebuild_all(device, queue, self.game.world_mut(), &self.mask);
        }
        !changed.is_empty()
    }

    /// Whether the game as the files have it now begins otherwise than the
    /// run being played did (the world's voxels, the first states, the
    /// settings that wait for a restart): then it plays anew from the
    /// fresh game, said in OUTPUT. A game that would not load or begin
    /// leaves things as they are.
    fn begins_otherwise(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let Ok(mut fresh) = Session::<G>::load_with(&self.dir, Slots::on_disk(&self.dir)) else { return false };
        if begin(&mut fresh, self.scenario.as_deref()).is_err() || fresh.trouble().is_some() {
            return false;
        }
        if fresh.start_mark() == self.mark && self.game.cfg.restart_only(&fresh.cfg).is_empty() {
            return false;
        }
        self.adopt(fresh, device, queue);
        self.notes.push("The game's beginning changed: begun anew.".to_string());
        true
    }

    /// What the game said since last asked, for OUTPUT: what it did of
    /// itself, files that would not load, and what its script printed.
    pub fn said(&mut self) -> Vec<(super::output::Kind, String)> {
        let notes = self.notes.drain(..).map(|n| (super::output::Kind::Note, n));
        let failed = self.failed.drain(..).chain(self.hidden.drain(..)).map(|f| (super::output::Kind::Error, f));
        let printed = self.game.printed().into_iter().map(|p| (super::output::Kind::Print, p));
        notes.chain(failed).chain(printed).collect()
    }

    /// A tick played: once a second the hidden-state guard looks at the
    /// locals the game's functions keep and says which changed.
    fn guard_tick(&mut self) {
        let game = &self.game.game;
        self.hidden.extend(self.guard.tick(|| game.hidden()));
    }

    /// A file of the game that would not load when it last changed (the
    /// first, if several), what was said of it; None once they all load.
    pub fn unloaded(&self) -> Option<String> {
        self.unloaded.values().next().cloned()
    }

    /// Whether the run is won or lost (PRESS START: the IDE has no title
    /// to go back to, so it starts over).
    pub fn over(&self) -> bool {
        self.game.game.outcome().is_some()
    }

    /// Puts the run being played, or else the last one, in the game's
    /// `replays/` (F9); where it went.
    pub fn keep(&self) -> Result<PathBuf> {
        self.runs.keep(&self.game)
    }

    /// What the player plays with, for the ticks to come: the keyboard
    /// here.
    fn controls(&mut self) {
        let controls = self.game.player_controls(Device::Keyboard, PadStyle::default(), &self.settings);
        self.game.set_controls(controls);
    }

    /// Steps `ticks` ticks with `input` at once (a shot, a test; not
    /// recorded).
    pub fn step(&mut self, ticks: u32, input: &crate::input::Input) {
        self.controls();
        for _ in 0..ticks {
            self.game.step(input);
            self.guard_tick();
        }
    }

    /// A frame: what changed on disk comes in, the ticks due are stepped
    /// with `input`, and the game is drawn into its texture, `size`
    /// pixels big (egui told of it). Its name to egui.
    pub fn frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, egui: &mut egui_wgpu::Renderer, size: [u32; 2], input: &crate::input::Input) -> egui::TextureId {
        let now = Instant::now();
        if self.last.is_none()
            && let Some(s) = &self.speaker
        {
            s.hold(false);
        }
        let elapsed = self.last.map_or(0.0, |l| (now - l).as_secs_f32().min(0.25));
        self.last = Some(now);
        self.reload(device, queue);

        // The music (started over when it changed), the player's volume.
        let music_gain = self.settings.music_gain();
        if let Some(mut mixer) = self.speaker.as_mut().and_then(|s| s.lock()) {
            // A level's fade holds no song (`Session::feed_music` says when).
            self.game.feed_music(&mut mixer, false);
            mixer.set_music_volume(self.game.cfg.audio.music_volume * music_gain);
            mixer.set_drive(self.game.cfg.audio.drive);
        }

        // The ticks due.
        self.controls();
        self.accumulator += elapsed;
        let mut ticks = 0;
        while self.accumulator >= DT && ticks < MAX_CATCH_UP {
            self.game.fades = true;
            self.game.step(input);
            self.guard_tick();
            self.runs.step(&self.game);
            if !self.game.game.sim().sounds.is_empty()
                && let Some(mut mixer) = self.speaker.as_mut().and_then(|s| s.lock())
            {
                self.game.play_sounds(&mut mixer, self.settings.sound_gain());
            }
            self.accumulator -= DT;
            ticks += 1;
        }
        if ticks == MAX_CATCH_UP {
            self.accumulator = 0.0;
        }
        self.draw(device, queue, egui, size)
    }

    /// Draws the game as it stands into its texture, `size` pixels big;
    /// its name to egui.
    pub fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, egui: &mut egui_wgpu::Renderer, size: [u32; 2]) -> egui::TextureId {
        self.renderer.update_chunks(device, queue, self.game.world_mut(), &self.mask);
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("play"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.renderer.resize(device, size[0], size[1]);
            let id = match &self.target {
                Some(t) => {
                    egui.update_egui_texture_from_wgpu_texture(device, &view, wgpu::FilterMode::Linear, t.id);
                    t.id
                }
                None => egui.register_native_texture(device, &view, wgpu::FilterMode::Linear),
            };
            self.target = Some(Target { texture, view, size, id });
        }
        let target = self.target.as_ref().expect("made above");
        // The picture's shape: the game's own (bars round it), else the
        // play area's.
        let aspect = self.game.picture_aspect(size[0] as f32 / size[1].max(1) as f32);
        self.renderer.set_shape(device, self.game.cfg.camera.aspect);
        // What stops it is said in the IDE's red line over the picture
        // (the traceback in OUTPUT), not in the game's big letters.
        self.game.problem_shown = false;
        let frame = self.game.menu_frame(&Menu::default(), &self.settings, self.accumulator / DT, aspect, Scene::Play);
        // A picture at another pitch or field of view (`frame:camera`) may
        // need faces the meshes left out: built anew with them, once.
        let faces = self.game.face_mask();
        if faces != self.mask {
            self.mask = faces;
            self.renderer.rebuild_all(device, queue, self.game.world_mut(), &self.mask);
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("play") });
        self.renderer.render(device, queue, &mut encoder, &target.view, &frame);
        queue.submit([encoder.finish()]);
        target.id
    }

    /// The game as last drawn, as a PNG (for the agent: what the player
    /// sees).
    pub fn picture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Vec<u8>> {
        let t = self.target.as_ref().ok_or_else(|| anyhow::anyhow!("the game has not been drawn yet"))?;
        let rgba = crate::headless::read_texture(device, queue, &t.texture)?;
        crate::headless::png_bytes(t.size[0], t.size[1], &rgba)
    }

    /// Ends it: its run over (kept if long enough), its texture freed.
    pub fn end(mut self, egui: &mut egui_wgpu::Renderer) {
        self.runs.end(&self.game);
        if let Some(t) = &self.target {
            egui.free_texture(&t.id);
        }
    }

    /// The game's state as one number (for tests).
    #[cfg(test)]
    pub fn state_hash(&self) -> u64 {
        self.game.game.state_hash()
    }
}

/// Whether the game in `dir` as its files are now begins (what PLAY finds
/// out first), without a window: the error if not.
pub fn begins<G: Game>(dir: &Path) -> Result<()> {
    let mut game = Session::<G>::load_with(dir, Slots::on_disk(dir))?;
    begin(&mut game, None)
}

/// What seat 1 does with which key, for the line under the game: moving
/// first (the arrows and WASD; a half of the keyboard its own four keys;
/// the pad's d-pad), then the game's actions in its order by their own
/// names in capitals, each with its key (or button) as the keyboard in use
/// labels it (`Controls::of`, the same the game's own menu shows); an
/// action nothing is bound to is left out.
pub fn keys_of(actions: &[&str], seat: crate::seating::Keys, settings: &Settings) -> Vec<(String, String)> {
    use crate::controls::Controls;
    use crate::seating::Keys;
    let (moving, controls) = match seat {
        Keys::Whole => ("ARROWS".to_string(), Controls::of(actions, Device::Keyboard, PadStyle::default(), settings)),
        Keys::Nothing => ("D-PAD".to_string(), Controls::of_pad(actions, PadStyle::default(), &settings.pad, settings)),
        half => {
            let bound = settings.keys_of(half);
            let ways: Vec<String> = ["up", "left", "down", "right"].iter().filter_map(|w| Some(settings.key_label(bound?.get(w).first()?))).collect();
            (if ways.len() == 4 { ways.join(" ") } else { "ARROWS".to_string() }, Controls::of_half(actions, half, settings))
        }
    };
    let mut line = vec![("MOVE".to_string(), moving)];
    line.extend(actions.iter().filter_map(|a| Some((a.to_uppercase(), controls.keys.get(*a).filter(|k| !k.is_empty())?.clone()))));
    line
}

/// The file and line a game's trouble names (`main.lua:12: ...`), if it
/// names one.
pub fn line_of(trouble: &str) -> Option<(String, usize)> {
    let at = trouble.find(".lua:")?;
    let start = trouble[..at].rfind(|c: char| !(c.is_ascii_alphanumeric() || "_./-".contains(c))).map_or(0, |i| i + 1);
    let file = &trouble[start..at + 4];
    let digits: String = trouble[at + 5..].chars().take_while(char::is_ascii_digit).collect();
    Some((file.to_string(), digits.parse().ok()?))
}

/// Lua's most common mistakes in plain words, as one short line under the
/// message (by its first line; None: no plain words for it).
pub fn plain_words(trouble: &str) -> Option<String> {
    let first = trouble.lines().next().unwrap_or("");
    // The type Lua names in "attempt to ... a number value".
    let what = |after: &str| first.split(after).nth(1).and_then(|r| r.split_whitespace().next()).map(str::to_string);
    let said = if first.contains("attempt to index a nil value") {
        "Something here is nil: a name misspelled, or not set yet.".to_string()
    } else if first.contains("attempt to call a nil value") {
        "A function here does not exist: its name misspelled, or not defined yet.".to_string()
    } else if first.contains("attempt to perform arithmetic on a nil value") {
        "A number here is nil: a name misspelled, or not set yet.".to_string()
    } else if first.contains("attempt to concatenate a nil value") {
        "Text joined with .. here is nil: a name misspelled, or not set yet.".to_string()
    } else if first.contains("attempt to compare") {
        "Two values here cannot be compared with < or >: one is nil, or a number meets a text.".to_string()
    } else if let Some(t) = what("attempt to index a ") {
        format!("Something here is a {t}, not a table: it has no fields to read.")
    } else if let Some(t) = what("attempt to call a ") {
        format!("Something called here is a {t}, not a function.")
    } else if let Some(t) = what("attempt to perform arithmetic on a ") {
        format!("Something counted with here is a {t}, not a number.")
    } else if first.contains("' expected") || first.contains("unexpected symbol") || first.contains("unfinished string") {
        "Lua cannot read this line: often an end, a bracket or a quote missing, here or above.".to_string()
    } else if first.contains("stack overflow") {
        "A function calls itself again and again without end.".to_string()
    } else {
        return None;
    };
    Some(said)
}

/// A fresh run begun from `scenario`: a mistake in the scenario's code
/// stops the game, shown over it until mended (as one in `main.lua`); a
/// scenario that is not there is said at once.
fn begin<G: Game>(game: &mut Session<G>, scenario: Option<&str>) -> Result<()> {
    if let Some(name) = scenario
        && let Err(e) = game.start_scenario(name)
        && game.game.halted().is_none()
    {
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Played, the game steps as `voxcade shot` steps it (the same state
    /// after the same ticks and keys) and is drawn into a texture of the
    /// area's size.
    #[test]
    fn play_steps_and_draws_into_its_panel() {
        let canvas = crate::headless::Canvas::new(320, 180, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let dir = Path::new("games/voxkeep");
        let mut play = Play::<crate::Voxcade>::start(dir, device, queue, None, None).unwrap();
        let mut shot = Session::<crate::Voxcade>::load(dir).unwrap();
        let right = crate::input::Input { right: true, ..Default::default() };
        play.step(90, &right);
        for _ in 0..90 {
            shot.step(&right);
        }
        assert_eq!(play.state_hash(), shot.game.state_hash(), "stepped as a shot steps");
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        let id = play.draw(device, queue, &mut egui, [320, 180]);
        assert_eq!(play.draw(device, queue, &mut egui, [320, 180]), id, "the same texture while the size stays");
        assert_eq!(play.target.as_ref().map(|t| t.size), Some([320, 180]));
        assert!(play.trouble().is_none());
        play.end(&mut egui);
    }

    /// A game stopped by a script's error and held takes in the file
    /// mended: the error goes, without a tick played.
    #[test]
    fn a_held_game_takes_in_the_mended_script() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let dir = crate::scratch::Scratch::new("play-mend");
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 32, 32, 8 }\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  local t = nil\n  local x = t.x\nend\n").unwrap();
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, None).unwrap();
        play.step(1, &crate::input::Input::default());
        let trouble = play.trouble().expect("stopped");
        assert_eq!(line_of(&trouble), Some(("main.lua".into(), 3)), "{trouble}");
        // The error in big letters over the picture is a window's: the
        // IDE's red line says it instead.
        let letters = |play: &Play<crate::Voxcade>| play.game.compose(0.0, 1.0, crate::session::Scene::Play, None).ui.len();
        assert!(letters(&play) > 0, "a window shows it over the game");
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        play.draw(device, queue, &mut egui, [64, 64]);
        assert_eq!(letters(&play), 0, "PLAY in the IDE does not");
        play.hold();
        // (A file counts as saved once it stood still for a look.)
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("main.lua"), "function on_tick()\nend\n").unwrap();
        let mut came = false;
        for _ in 0..12 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            came |= play.reload(device, queue);
        }
        assert!(came && play.trouble().is_none(), "mended: {:?}", play.trouble());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A game in a folder of its own with `main` as its `main.lua`: played
    /// a few ticks.
    fn played(name: &str, conf: &str, main: &str, device: &wgpu::Device, queue: &wgpu::Queue) -> (crate::scratch::Scratch, Play<crate::Voxcade>) {
        let dir = crate::scratch::Scratch::new(&format!("play-{name}"));
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        std::fs::write(dir.join("main.lua"), main).unwrap();
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, None).unwrap();
        play.step(5, &crate::input::Input::default());
        (dir, play)
    }

    /// Writes `main` over `main.lua` and waits for the play to take it in.
    fn saved(dir: &Path, file: &str, text: &str, play: &mut Play<crate::Voxcade>, device: &wgpu::Device, queue: &wgpu::Queue) {
        // (A file counts as saved once it stood still for a look.)
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join(file), text).unwrap();
        let mut came = false;
        for _ in 0..12 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            came |= play.reload(device, queue);
        }
        assert!(came, "{file} came in");
    }

    const CONF: &str = "function conf(c)\n  c.world.size = { 32, 32, 8 }\nend\n";

    fn floor(play: &Play<crate::Voxcade>) -> [u8; 3] {
        let w = &play.game.game.sim().world;
        let c = w.palette[w.get(glam::IVec3::new(3, 3, 0)) as usize];
        [c[0], c[1], c[2]]
    }

    fn noted(play: &mut Play<crate::Voxcade>) -> bool {
        play.said().iter().any(|(k, t)| matches!(k, super::super::output::Kind::Note) && t.contains("begun anew"))
    }

    const GREEN: &str = "function on_level_start(how)\n  level:fill(0, 0, 0, 31, 31, 0, \"#00ff00\")\nend\nfunction on_tick()\nend\n";

    /// How the level is built changed: the game being played begins anew
    /// and shows the new floor.
    #[test]
    fn a_changed_level_begins_the_game_anew() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let (dir, mut play) = played("level", CONF, GREEN, device, queue);
        assert_eq!(floor(&play), [0, 255, 0]);
        assert_eq!(play.game.game.sim().tick, 5);
        saved(&dir, "main.lua", &GREEN.replace("#00ff00", "#ff0000"), &mut play, device, queue);
        assert_eq!(floor(&play), [255, 0, 0], "the new floor");
        assert_eq!(play.game.game.sim().tick, 0, "begun anew");
        assert!(noted(&mut play));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A number of the rules alone: the run goes on.
    #[test]
    fn a_changed_rule_keeps_the_run_going() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let main = "function on_level_start(how)\n  level:fill(0, 0, 0, 31, 31, 0, \"#00ff00\")\nend\nfunction on_tick()\n  player.state.score = player.state.score + 1\nend\n";
        let (dir, mut play) = played("rule", CONF, main, device, queue);
        saved(&dir, "main.lua", &main.replace("+ 1", "+ 2"), &mut play, device, queue);
        assert_eq!(play.game.game.sim().tick, 5, "played on");
        assert_eq!(floor(&play), [0, 255, 0]);
        assert!(!noted(&mut play));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A setting that waits for a restart changed in `conf.lua`.
    #[test]
    fn a_restart_only_setting_begins_the_game_anew() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let (dir, mut play) = played("conf", CONF, GREEN, device, queue);
        saved(&dir, "conf.lua", "function conf(c)\n  c.world.size = { 32, 32, 8 }\n  c.player.spawn = { 10, 10 }\nend\n", &mut play, device, queue);
        assert_eq!(play.game.game.sim().tick, 0, "begun anew");
        assert!(noted(&mut play));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The run played is recorded: F9 puts it in the game's replays,
    /// and it plays through as it was played.
    #[test]
    fn a_run_played_is_kept_with_f9() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let dir = crate::scratch::Scratch::new("play-keep");
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 32, 32, 8 }\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  if input.right then player.state.score = player.state.score + 1 end\nend\n").unwrap();
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, None).unwrap();
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        let right = crate::input::Input { right: true, ..Default::default() };
        for _ in 0..4 {
            play.frame(device, queue, &mut egui, [64, 64], &right);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let path = play.keep().unwrap();
        assert!(path.starts_with(dir.join("replays")), "{}", path.display());
        let replay = crate::replay::Replay::load(&path).unwrap();
        assert!(replay.ticks > 0 && replay.score as u64 == replay.ticks, "{} ticks, {} points", replay.ticks, replay.score);
        assert!(matches!(crate::replay::play_through::<crate::Voxcade>(&dir, &replay).unwrap(), crate::replay::Verdict::Same));
        play.end(&mut egui);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What OUTPUT shows as a mistake.
    fn errors(play: &mut Play<crate::Voxcade>) -> Vec<String> {
        play.said().into_iter().filter(|(k, _)| matches!(k, super::super::output::Kind::Error)).map(|(_, t)| t).collect()
    }

    /// A local of the game's that its `on_tick` changes is named in OUTPUT
    /// within a second of play, once (`voxcade check` alone found it, and
    /// only in a game with replays); a game that keeps its state in states
    /// hears nothing, nor does one whose code is taken in while it plays.
    #[test]
    fn a_local_that_changes_while_playing_is_named() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let hidden = "local count = 0\nlocal function more(n) return n + 1 end\nfunction on_tick()\n  count = more(count)\nend\n";
        let (dir, mut play) = played("hidden", CONF, hidden, device, queue);
        play.step(55, &crate::input::Input::default());
        let said = errors(&mut play);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("'count' changed while the game ran") && said[0].contains("world.state"), "{}", said[0]);
        play.step(240, &crate::input::Input::default());
        assert!(errors(&mut play).is_empty(), "said once");
        let _ = std::fs::remove_dir_all(&dir);

        let clean = "local function more(n) return n + 1 end\nfunction on_tick()\n  world.state.count = more(world.state.count or 0)\nend\n";
        let (dir, mut play) = played("clean", CONF, clean, device, queue);
        play.step(55, &crate::input::Input::default());
        saved(&dir, "main.lua", &clean.replace("n + 1", "n + 2"), &mut play, device, queue);
        play.step(180, &crate::input::Input::default());
        assert!(errors(&mut play).is_empty(), "{:?}", errors(&mut play));
        assert!(play.game.game.sim().tick > 200, "played on");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The common mistakes have plain words, the rest none.
    #[test]
    fn common_mistakes_are_said_plainly() {
        let said = |t: &str| plain_words(t).unwrap_or_default();
        assert!(said("main.lua:148: attempt to index a nil value (global 'nothing_here')\nstack traceback:").starts_with("Something here is nil"));
        assert!(said("main.lua:3: attempt to call a nil value (method 'jmp')").starts_with("A function here does not exist"));
        assert!(said("main.lua:3: attempt to perform arithmetic on a nil value (field 'x')").starts_with("A number here is nil"));
        assert!(said("main.lua:3: attempt to compare number with nil").starts_with("Two values"));
        assert_eq!(said("main.lua:3: attempt to index a number value (local 'n')"), "Something here is a number, not a table: it has no fields to read.");
        assert!(said("main.lua:9: 'end' expected (to close 'function' at line 3) near <eof>").starts_with("Lua cannot read"));
        assert_eq!(plain_words("main.lua:1: the level 'x' is not there"), None);
    }

    /// A script's error names its file and line.
    #[test]
    fn a_games_trouble_names_its_line() {
        assert_eq!(line_of("main.lua:12: attempt to index a nil value (tick 30)"), Some(("main.lua".into(), 12)));
        assert_eq!(line_of("in levels/a.lua:3: boom"), Some(("levels/a.lua".into(), 3)));
        assert_eq!(line_of("conf.lua could not be read"), None);
    }

    /// The line under the game names moving and then each action of the game
    /// by its own name with its key as the keyboard in use labels it (a German
    /// keyboard's Y for the bindings' Z); an action with no key is left out.
    #[test]
    fn the_keys_line_names_the_actions_and_their_keys() {
        use crate::controls::Bindings;
        use crate::seating::Keys;
        let mut settings = Settings::default();
        settings.adopt(&["jump", "fire", "bomb"], &Bindings::new(&[("jump", &["Z"]), ("fire", &["X"])]), &Bindings::new(&[("jump", &["south"])]));
        settings.key_names.insert("Z".into(), "Y".into());
        let line = |seat| keys_of(&["jump", "fire", "bomb"], seat, &settings).into_iter().map(|(a, k)| format!("{a} {k}")).collect::<Vec<_>>();
        assert_eq!(line(Keys::Whole), ["MOVE ARROWS", "JUMP Y", "FIRE X"]);
        assert_eq!(line(Keys::Nothing), ["MOVE D-PAD", "JUMP A"], "a pad's buttons");
        assert_eq!(line(Keys::Left)[0].split(' ').count(), 5, "a half's own four keys: {:?}", line(Keys::Left));
    }

    /// AGAIN (`restart`) begins the game from the start again, from its
    /// scenario if it was played from one.
    #[test]
    fn again_begins_from_the_scenario_again() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let dir = crate::scratch::Scratch::new("play-again");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenarios")).unwrap();
        std::fs::write(dir.join("conf.lua"), CONF).unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  player.state.score = player.state.score + 1\nend\n").unwrap();
        std::fs::write(dir.join("scenarios/late.lua"), "return {\n  about = \"late\",\n  setup = function()\n    player.state.score = 1000\n  end,\n}\n").unwrap();
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, Some("late")).unwrap();
        play.step(5, &crate::input::Input::default());
        assert_eq!(play.game.game.sim().tick, 5);
        play.restart(device, queue).unwrap();
        assert_eq!(play.game.game.sim().tick, 0, "from the start");
        play.step(3, &crate::input::Input::default());
        assert_eq!(play.scenario.as_deref(), Some("late"), "the scenario again");
        let score = play.eval("player.state.score").unwrap();
        assert!(score.contains("1003"), "begun from the scenario's 1000, three ticks on: {score}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
