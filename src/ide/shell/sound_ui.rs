//! Sounds and songs in the main area: PLAY SONG, PLAY SOUND (where the
//! maker is not), a built-in sound and MAKE IT YOURS, USED (where the game
//! names a sound), and the sound maker (PLAY, LOOP, UNDO, its sorts, the
//! faders in as many rows as the room asks).

use super::*;

impl Shell {
    /// PLAY SONG and STOP SONG over a song: plays the open file, or ends it.
    pub fn listen(&mut self) {
        if self.sound.listening.is_some() {
            return self.hush();
        }
        // (A song with a mistake plays nothing: the mistake stands above it.)
        let mistaken = matches!(&self.shown, Some(Shown::Code(b)) if b.code.error.is_some());
        if let Some(path) = self.memory.open.clone().filter(|p| p.ends_with(".mml") && !mistaken) {
            self.sound.listening = Some(path.clone());
            self.asks.push(Ask::Listen(path));
        }
    }

    /// Ends the song playing, if one is (another file opened, the game
    /// played).
    pub(super) fn hush(&mut self) {
        (self.sound.looping, self.sound.again_at) = (false, None);
        if self.sound.listening.take().is_some() {
            self.asks.push(Ask::Hush);
        }
    }

    /// How hard the game's sound goes into the limiter (`c.audio.drive`),
    /// and the limiter's ceiling (`c.audio.ceiling`).
    pub fn drive(&self) -> (f32, f32) {
        (self.cfg.audio.drive, self.cfg.audio.ceiling)
    }

    /// The song in `path` to play, at the volume the game and the player
    /// set for music.
    pub fn song(&self, path: &str) -> anyhow::Result<crate::audio::music::Player> {
        let file = self.dir.join(path);
        // As the text stands now, saved or not.
        let text = match (&self.shown, self.memory.open.as_deref()) {
            (Some(Shown::Code(b)), Some(open)) if open == path => b.code.text().to_string(),
            _ => std::fs::read_to_string(&file).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?,
        };
        let mut song = crate::audio::music::parse(&text).map_err(|e| anyhow::anyhow!("{path}: {e:#}"))?;
        // Its samples as the game's folder holds them now.
        let samples = crate::audio::music::Samples::read_folder(&crate::files::Files::folder(&self.dir))?;
        song.bind(&samples).map_err(|e| anyhow::anyhow!("{path}: {e:#}"))?;
        let gain = crate::settings::Settings::load(&crate::settings::path_for(&self.dir)).music_gain();
        Ok(crate::audio::music::Player::new(song, self.cfg.audio.music_volume * gain))
    }

    /// PLAY SOUND: the sound open (as its text stands now) rendered for the
    /// speaker, which takes it with `heard` after `Ask::Sound`.
    pub fn play_sound(&mut self) {
        let def = match (&self.shown, &self.memory.open, self.sound.builtin) {
            _ if self.sound.builtin.is_some() => self.sound.brought.iter().find(|(n, _)| Some(*n) == self.sound.builtin).map(|(_, d)| d.clone()),
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
                self.sound.again_at = Some(std::time::Instant::now() + std::time::Duration::from_secs_f32(samples.len() as f32 / crate::audio::SAMPLE_RATE as f32) + LOOP_PAUSE);
                self.sound.heard = Some((samples, self.cfg.audio.volume.max(0.0) * gain));
                self.message = None;
                self.asks.push(Ask::Sound);
            }
            Err(e) => self.tell(format!("{e:#}")),
        }
    }

    /// The sound PLAY SOUND made ready, with its volume.
    pub fn heard(&mut self) -> Option<(std::sync::Arc<[f32]>, f32)> {
        self.sound.heard.take()
    }

    /// MAKE IT YOURS: the built-in sound `name` copied into the game as
    /// `sounds/NAME.toml`, and opened.
    pub fn make_it_yours(&mut self, name: &str) {
        let Some((_, def)) = self.sound.brought.iter().find(|(n, _)| *n == name) else { return };
        let path = format!("sounds/{name}.toml");
        if !self.dir.join(&path).exists() {
            let text = sound::write(def, &[format!("{name}: a sound the game brought, copied here to change; the game now plays this one.")]);
            let made = std::fs::create_dir_all(self.dir.join("sounds")).and_then(|()| std::fs::write(self.dir.join(&path), text));
            if let Err(e) = made {
                return self.tell(format!("{path}: {e}"));
            }
        }
        self.show_made(&path);
    }

    /// Where the game's code names the sound `name`, looked for afresh when
    /// it is another sound or a second has gone.
    pub(super) fn refresh_used(&mut self, name: &str) {
        if self.sound.used.as_ref().is_none_or(|u| u.name != name || u.at.elapsed() >= USED_FOR) {
            let rows = sounds::used(&self.dir, &self.files.logic, name);
            self.sound.used = Some(Used { name: name.to_string(), rows, at: std::time::Instant::now() });
        }
    }

    /// What is said under USED of the sound `name` beside the lines that
    /// name it: that the game's own rules play it, when it is one of the
    /// sounds they bring.
    pub(super) fn rules_say(&self, name: &str) -> Vec<String> {
        if !self.sound.brought.iter().any(|(n, _)| *n == name) {
            return Vec::new();
        }
        let core = crate::audio::CORE_FILES.iter().any(|(n, _)| *n == name);
        vec![if core { "THE ENGINE".into() } else { "THE GAME'S READY-MADE RULES".to_string() }]
    }

    /// A sound the game brings, shown in the main area: what it is, PLAY
    /// SOUND, MAKE IT YOURS, its text to read, and where it is used.
    pub(super) fn builtin_ui(&mut self, ui: &mut egui::Ui, name: &'static str) {
        let Some(def) = self.sound.brought.iter().find(|(n, _)| *n == name).map(|(_, d)| d.clone()) else {
            self.sound.builtin = None;
            return;
        };
        let line = format!("{} · built in", sounds::describe(&def));
        let (mut play, mut yours) = (false, false);
        ui.horizontal(|ui| {
            ui.label(RichText::new(line.to_uppercase()).color(QUIET)).on_hover_text("Built in: the game has no file of its own for it.");
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
        let rows = self.sound.used.as_ref().map(|u| u.rows.clone()).unwrap_or_default();
        let says = self.rules_say(name);
        let mut go = None;
        let text = sound::write(&def, &[]);
        egui::ScrollArea::both().id_salt(("builtin", name)).auto_shrink([false, false]).content_margin(egui::Margin { right: 12, bottom: 14, ..egui::Margin::ZERO }).show(ui, |ui| {
            let font = TextStyle::Monospace.resolve(ui.style());
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| ui.fonts_mut(|f| f.layout_job(crate::ide::code::highlight(text.as_str(), &font, crate::ide::code::Syntax::Sound)));
            let mut shown = text.as_str();
            ui.add(egui::TextEdit::multiline(&mut shown).code_editor().frame(egui::Frame::NONE).desired_width(f32::INFINITY).layouter(&mut layouter));
            go = used_ui(ui, name, &says, &rows);
        });
        if play {
            self.play_sound();
        }
        if yours {
            self.make_it_yours(name);
        }
        if let Some((file, line)) = go {
            self.show_at(&file, line);
        }
    }

    /// The sound maker over the sound open (`maker`; its text in the text
    /// edit `id`): a dice and a row of sorts, then UNDO, a row of faders.
    /// Every change is the text rewritten in the buffer, as typing is, and
    /// one step back for UNDO and Ctrl+Z alike; a roll and UNDO play the
    /// sound at once. Whether it is to play now because a fader was let go.
    pub(super) fn maker_ui(&mut self, ui: &mut egui::Ui, id: egui::Id) -> bool {
        let ctx = ui.ctx().clone();
        let can_go_back = matches!(&self.shown, Some(Shown::Code(b)) if b.code.can_go_back(&ctx, id));
        let sorts: Vec<(&str, &str)> = maker::Sort::ALL.iter().map(|s| (s.word(), s.hover())).collect();
        let tips = MakerTips {
            play: "Plays the sound as it stands, at the game's sound volume.",
            looping: "Plays the sound again and again with a short pause, so you hear what the faders and your typing do. Press again to stop.",
            undo: "Back to the sound before the last change (a roll, a fader or typing); again for the one before. Ctrl+Z in the text does the same.",
            dice: "These roll the dice: each press makes a new sound of that sort, never the same twice. Press one again for another.",
        };
        let pressed = self.maker_rows(ui, None, can_go_back, &tips, &sorts);
        let mut play = pressed.play;
        if let Some(i) = pressed.rolled {
            self.roll(&ctx, maker::Sort::ALL[i]);
        }
        if pressed.back {
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
        let knobs: Vec<Knob> = maker::Fader::ALL
            .iter()
            .map(|&fader| {
                let value = synth.as_ref().map_or(fader.range().0, |s| fader.place(s));
                Knob { word: fader.word(), hover: fader.hover(), range: fader.range(), logarithmic: fader.logarithmic(), value, shown: fader.shown(value), quiet: synth.is_none().then(|| "The faders move a sound made of numbers; this text is not one now (a recording, silent, or a mistake in it).".to_string()) }
            })
            .collect();
        let fading = &mut self.sound.fading;
        for (&fader, (to, down)) in maker::Fader::ALL.iter().zip(fader_grid(ui, &knobs)) {
            if let (Some(to), Some(s)) = (to, &synth) {
                // Marked before the first change of a move, after its last.
                if *fading != Some(fader) {
                    buffer.code.mark(&ctx, id);
                    *fading = Some(fader);
                }
                buffer.code.set_text(maker::faded(buffer.code.text(), s, fader, to));
                buffer.code.check();
                buffer.edited(now);
            }
            if *fading == Some(fader) && !down {
                buffer.code.mark(&ctx, id);
                *fading = None;
                play = true;
            }
        }
        play
    }

    /// A maker's first rows (the sound maker's, the instrument maker's):
    /// `lead` (a word and its hover) first, then PLAY, LOOP and UNDO, then
    /// the dice and the sorts (each a word and its hover), on one row
    /// where the room allows, else the sorts on a row of their own. LOOP
    /// switched here. What was pressed.
    pub(super) fn maker_rows(&mut self, ui: &mut egui::Ui, lead: Option<(&str, egui::Color32, &str)>, can_go_back: bool, tips: &MakerTips, sorts: &[(&str, &str)]) -> Pressed {
        let mut pressed = Pressed { play: false, back: false, rolled: None };
        let room = ui.available_width();
        let width = |ui: &egui::Ui, words: &[&str]| -> f32 {
            let font = TextStyle::Button.resolve(ui.style());
            let pad = 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x;
            words.iter().map(|w| ui.fonts_mut(|f| f.layout_no_wrap(w.to_string(), font.clone(), TEXT).size().x) + pad).sum()
        };
        let words: Vec<&str> = sorts.iter().map(|(w, _)| *w).collect();
        let lead_width = lead.map_or(0.0, |(w, ..)| width(ui, &[w]) + 12.0);
        let one_row = lead_width + width(ui, &["PLAY", "LOOP", "UNDO"]) + 18.0 + 30.0 + width(ui, &words) <= room;
        let looping = &mut self.sound.looping;
        let again_at = &mut self.sound.again_at;
        let mut controls = |ui: &mut egui::Ui| {
            if let Some((word, color, hover)) = lead {
                ui.label(RichText::new(word).color(color)).on_hover_text(hover);
                ui.add_space(12.0);
            }
            pressed.play = ui.button("PLAY").on_hover_text(tips.play).clicked();
            if ui.add(egui::Button::new("LOOP").selected(*looping)).on_hover_text(tips.looping).clicked() {
                *looping = !*looping;
                *again_at = None;
                pressed.play |= *looping;
            }
            pressed.back = ui.add_enabled(can_go_back, egui::Button::new("UNDO")).on_hover_text(tips.undo).clicked();
        };
        let mut rolled = None;
        let mut rolls = |ui: &mut egui::Ui| {
            crate::gui::dice(ui).on_hover_text(tips.dice);
            for (i, (word, hover)) in sorts.iter().enumerate() {
                if ui.button(*word).on_hover_text(*hover).clicked() {
                    rolled = Some(i);
                }
            }
        };
        if one_row {
            ui.horizontal(|ui| {
                controls(ui);
                ui.add_space(18.0);
                rolls(ui);
            });
        } else {
            ui.horizontal(|ui| controls(ui));
            ui.horizontal_wrapped(|ui| rolls(ui));
        }
        pressed.rolled = rolled;
        pressed
    }

    /// The text edit of the file open.
    pub(super) fn code_id(&self) -> Option<(String, egui::Id)> {
        let path = self.memory.open.clone()?;
        let id = egui::Id::unique(("code", path.as_str()));
        Some((path, id))
    }

    /// A sort's button: the sound open replaced by a fresh one
    /// of `sort` (its leading comments kept), one step back for UNDO, and
    /// played.
    pub(super) fn roll(&mut self, ctx: &egui::Context, sort: maker::Sort) {
        let Some((_, id)) = self.code_id() else { return };
        let Some(Shown::Code(buffer)) = &mut self.shown else { return };
        self.sound.rolls += 1;
        let clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let fresh = maker::fresh(buffer.code.text(), &maker::roll(sort, clock ^ self.sound.rolls.rotate_left(32)));
        buffer.code.mark(ctx, id);
        buffer.code.set_text(fresh);
        buffer.code.check();
        buffer.code.mark(ctx, id);
        buffer.edited(std::time::Instant::now());
        self.play_sound();
    }

    /// UNDO: the text a step back, as Ctrl+Z in it, and played.
    pub(super) fn go_back(&mut self, ctx: &egui::Context) {
        let Some((_, id)) = self.code_id() else { return };
        let Some(Shown::Code(buffer)) = &mut self.shown else { return };
        if buffer.code.go_back(ctx, id) {
            buffer.edited(std::time::Instant::now());
            self.play_again();
        }
    }
}

/// How wide a fader of the makers is.
pub(super) const FADER_WIDTH: f32 = 150.0;

/// The hover words of a maker's first row.
pub(super) struct MakerTips {
    pub play: &'static str,
    pub looping: &'static str,
    pub undo: &'static str,
    pub dice: &'static str,
}

/// What was pressed on a maker's first row: PLAY (or LOOP switched on),
/// UNDO, a sort (its place among them).
pub(super) struct Pressed {
    pub play: bool,
    pub back: bool,
    pub rolled: Option<usize>,
}

/// A fader of a maker as drawn: its word and hover, its ends, whether it
/// moves in octaves (or times), where it stands and its number as shown,
/// and why it means nothing here, if it does not (quiet, the why on hover).
pub(super) struct Knob {
    pub word: &'static str,
    pub hover: &'static str,
    pub range: (f64, f64),
    pub logarithmic: bool,
    pub value: f64,
    pub shown: String,
    pub quiet: Option<String>,
}

/// A maker's faders, as many a row as the room takes, the rows as even as
/// can be (five: three and two), never one cut at the edge. For each,
/// where it was moved to this frame (if it was), and whether it is held.
pub(super) fn fader_grid(ui: &mut egui::Ui, knobs: &[Knob]) -> Vec<(Option<f64>, bool)> {
    let gap = 22.0;
    let fit = (((ui.available_width() + gap) / (FADER_WIDTH + gap)).floor() as usize).max(1);
    let rows = knobs.len().div_ceil(fit).max(1);
    let per_row = knobs.len().div_ceil(rows).max(1);
    let mut out = Vec::new();
    for row in knobs.chunks(per_row) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for knob in row {
                let (lo, hi) = knob.range;
                let mut value = knob.value.clamp(lo, hi);
                let held = ui
                    .add_enabled_ui(knob.quiet.is_none(), |ui| {
                        ui.allocate_ui_with_layout(egui::vec2(FADER_WIDTH, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            let quiet = knob.quiet.as_deref().unwrap_or("");
                            ui.horizontal(|ui| {
                                ui.label(knob.word).on_hover_text(knob.hover).on_disabled_hover_text(quiet);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(&knob.shown).small().color(QUIET));
                                });
                            });
                            ui.spacing_mut().slider_width = FADER_WIDTH;
                            ui.add(egui::Slider::new(&mut value, lo..=hi).logarithmic(knob.logarithmic).show_value(false)).on_hover_text(knob.hover).on_disabled_hover_text(quiet)
                        })
                        .inner
                    })
                    .inner;
                let down = held.is_pointer_button_down_on() || held.dragged();
                out.push((held.changed().then_some(value), down));
            }
        });
        ui.add_space(4.0);
    }
    out
}

/// The pause between a sound's end and its next go under LOOP.
pub(super) const LOOP_PAUSE: std::time::Duration = std::time::Duration::from_millis(500);

/// Where a sound is used, under its text: USED, then what the game's code
/// says of it (`says`) and each line of it that names it, clicked to open
/// there (the file and the line returned).
pub(super) fn used_ui(ui: &mut egui::Ui, name: &str, says: &[String], rows: &[crate::ide::refs::Ref]) -> Option<(String, usize)> {
    let mut go = None;
    ui.add_space(18.0);
    ui.label(RichText::new("USED").small().color(QUIET));
    ui.add_space(4.0);
    for said in says {
        ui.label(RichText::new(said).color(QUIET));
    }
    if says.is_empty() && rows.is_empty() {
        ui.label(RichText::new("NOT USED YET").color(QUIET)).on_hover_text(format!("level:sound(\"{name}\", player) plays it."));
    }
    let maybes = rows.iter().position(|r| !r.sure);
    for (i, row) in rows.iter().enumerate() {
        // The same name quoted where the code may mean something else.
        if Some(i) == maybes {
            ui.add_space(8.0);
            ui.label(RichText::new("MAYBE").small().color(QUIET)).on_hover_text("The name in quotes where the game's code may mean something else (its own function, a variable).");
        }
        let shown: String = row.text.chars().take(100).collect();
        let at = ui.horizontal(|ui| {
            let place = ui.add(egui::Label::new(RichText::new(format!("{}:{}", row.file, row.line)).color(ACCENT)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand);
            let text = ui.add(egui::Label::new(RichText::new(shown).text_style(TextStyle::Monospace).color(QUIET)).sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand);
            place.clicked() || text.clicked()
        });
        if at.inner {
            go = Some((row.file.clone(), row.line));
        }
    }
    go
}
