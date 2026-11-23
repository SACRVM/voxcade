//! The instrument maker on a song's page (`instrument`): the instrument
//! the text cursor is in, PLAY, LOOP and UNDO, the dice and its sorts, the
//! faders, and a row of keys that play it as its text stands, through the
//! sounds' speaker at the game's music volume and the player's.

use super::*;

/// The instrument maker's own state over a song open.
pub struct Keys {
    /// The instrument the cursor was in when last drawn (its name).
    pub(super) of: Option<String>,
    /// What the cursor was found in, by the text's revision and the
    /// cursor's place: looked for again only when either changes.
    seen: Option<(u64, usize, Option<instrument::Block>)>,
    /// The octave the keys play in (MML's, 0 to 9) and the key last
    /// played (from C).
    pub(super) octave: i32,
    pub(super) key: usize,
    /// The fader held, if one is.
    fading: Option<instrument::Fader>,
    /// How many rolls so far (a part of the next one's seed).
    rolls: u64,
    /// A sort to roll as the maker is next drawn, and its seed (`voxcade
    /// ide --shot --roll`).
    pub(super) asked: Option<(instrument::Sort, u64)>,
}

impl Default for Keys {
    fn default() -> Keys {
        Keys { of: None, seen: None, octave: 4, key: 0, fading: None, rolls: 0, asked: None }
    }
}

impl Shell {
    /// The instrument maker over the song open (its text in the text edit
    /// `id`): the instrument the cursor is in, PLAY, LOOP, UNDO, the sorts,
    /// the faders, the octave and the twelve keys; quiet where the cursor
    /// is in no instrument. Every change is the text rewritten in the
    /// buffer as typing is, one step back for UNDO and Ctrl+Z alike; a
    /// roll, UNDO and a fader let go play the key last played. Whether the
    /// key is to play now (PLAY, LOOP, a key, a fader let go).
    pub(super) fn instrument_ui(&mut self, ui: &mut egui::Ui, id: egui::Id) -> bool {
        let ctx = ui.ctx().clone();
        let block = self.instrument_here(&ctx, id);
        let Some(Shown::Code(buffer)) = &self.shown else { return false };
        let keys = &mut self.sound.keys;
        let name = block.as_ref().map(|b| b.name.clone());
        if name.is_some() && name != keys.of {
            keys.octave = name.as_deref().and_then(|n| instrument::octave_used(buffer.code.text(), n)).unwrap_or(4);
        }
        keys.of = name.clone();
        let can_go_back = buffer.code.can_go_back(&ctx, id);
        let hz = crate::audio::music::note_hz(&format!("{}{}", instrument::NOTES[keys.key], keys.octave)).unwrap_or(261.63) as f64;
        let mut play = false;
        // A roll a shot asked for.
        if let Some((sort, seed)) = keys.asked.take_if(|_| block.is_some()) {
            self.roll_instrument(&ctx, id, sort, seed);
        }
        // The first rows: what the cursor is in, PLAY, LOOP, UNDO, the sorts.
        let shown_name = name.as_ref().map(|n| format!("@{n}"));
        let lead = match &shown_name {
            Some(n) => (n.as_str(), TEXT, "The instrument the text cursor is in: the sorts, the faders and the keys work on its lines."),
            None => ("NO INSTRUMENT", QUIET, "Put the text cursor on an instrument's line (@name ...) or in a channel's notes: the maker works on the instrument there."),
        };
        let sorts: Vec<(&str, &str)> = instrument::Sort::ALL.iter().map(|s| (s.word(), s.hover())).collect();
        let tips = MakerTips {
            play: "Plays the key last played, the instrument as its text stands, at the game's music volume.",
            looping: "Plays the key last played again and again with a short pause, so you hear what the faders and your typing do. Press again to stop.",
            undo: "Back to the text before the last change (a roll, a fader or typing); again for the one before. Ctrl+Z in the text does the same.",
            dice: "These roll the dice: each press makes a new instrument of that sort in place of this one, keeping its name, never the same twice. Press one again for another.",
        };
        let pressed = ui.add_enabled_ui(block.is_some(), |ui| self.maker_rows(ui, Some(lead), can_go_back, &tips, &sorts)).inner;
        play |= pressed.play;
        if let Some(i) = pressed.rolled {
            self.sound.keys.rolls += 1;
            let clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
            self.roll_instrument(&ctx, id, instrument::Sort::ALL[i], clock ^ self.sound.keys.rolls.rotate_left(32));
        }
        if pressed.back {
            self.go_back(&ctx);
        }
        // The faders, on the instrument as its text stands now.
        let block = self.instrument_here(&ctx, id).filter(|b| Some(&b.name) == name.as_ref());
        let Some(Shown::Code(buffer)) = &mut self.shown else { return play };
        ui.add_space(6.0);
        let knobs: Vec<Knob> = instrument::Fader::ALL
            .iter()
            .map(|&fader| {
                let place = match &block {
                    Some(b) => fader.place(buffer.code.text(), b, hz).map_err(str::to_string),
                    None => Err("The faders move the instrument the text cursor is in.".to_string()),
                };
                let (value, shown, quiet) = match place {
                    Ok((v, shown)) => (v, shown, None),
                    Err(why) => (fader.range().0, String::new(), Some(why)),
                };
                Knob { word: fader.word(), hover: fader.hover(), range: fader.range(), logarithmic: fader.logarithmic(), value, shown, quiet }
            })
            .collect();
        let now = std::time::Instant::now();
        let keys = &mut self.sound.keys;
        for (&fader, (to, down)) in instrument::Fader::ALL.iter().zip(fader_grid(ui, &knobs)) {
            if let (Some(to), Some(b)) = (to, &block) {
                // Marked before the first change of a move, after its last.
                if keys.fading != Some(fader) {
                    buffer.code.mark(&ctx, id);
                    keys.fading = Some(fader);
                }
                let old = buffer.code.text();
                let (row, col) = instrument::row_col(old, buffer.code.cursor(&ctx, id).unwrap_or(0));
                let new = fader.moved(old, b, hz, to);
                let at = instrument::char_at(&new, row, col);
                buffer.code.set_text(new);
                buffer.code.check();
                buffer.code.put_cursor(&ctx, id, at);
                buffer.edited(now);
            }
            if keys.fading == Some(fader) && !down {
                buffer.code.mark(&ctx, id);
                keys.fading = None;
                play = true;
            }
        }
        // The keys: the octave, then C to B.
        ui.add_enabled_ui(block.is_some(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("OCTAVE").color(QUIET)).on_hover_text("The octave the keys play in, as MML counts them (o4 holds the C over the middle of a piano's keys).");
                if ui.add_enabled(keys.octave > 0, egui::Button::new("-")).on_hover_text("The keys an octave lower.").clicked() {
                    keys.octave -= 1;
                }
                ui.label(RichText::new(keys.octave.to_string()).color(TEXT));
                if ui.add_enabled(keys.octave < 9, egui::Button::new("+")).on_hover_text("The keys an octave higher.").clicked() {
                    keys.octave += 1;
                }
                ui.add_space(12.0);
                for (k, word) in instrument::KEYS.iter().enumerate() {
                    let tip = format!("Plays {word}{} (o{} {}), a quarter at the song's tempo, the instrument as its text stands.", keys.octave, keys.octave, instrument::NOTES[k]);
                    if ui.add(egui::Button::new(*word).selected(k == keys.key)).on_hover_text(tip).clicked() {
                        keys.key = k;
                        play = true;
                    }
                }
            });
        });
        play
    }

    /// The instrument the cursor of the text edit `id` is in, looked for
    /// again only when the text or the cursor moved.
    fn instrument_here(&mut self, ctx: &egui::Context, id: egui::Id) -> Option<instrument::Block> {
        let Some(Shown::Code(buffer)) = &self.shown else { return None };
        let at = buffer.code.cursor(ctx, id).unwrap_or(0);
        let revision = buffer.code.revision();
        let keys = &mut self.sound.keys;
        if keys.seen.as_ref().is_none_or(|(r, a, _)| (*r, *a) != (revision, at)) {
            keys.seen = Some((revision, at, instrument::at_cursor(buffer.code.text(), at)));
        }
        keys.seen.as_ref().and_then(|(_, _, b)| b.clone())
    }

    /// A sort's button: the instrument the cursor is in written anew as a
    /// fresh one of `sort` from `seed` (its name and comments kept, the
    /// cursor where it was), one step back for UNDO, and played.
    pub(super) fn roll_instrument(&mut self, ctx: &egui::Context, id: egui::Id, sort: instrument::Sort, seed: u64) {
        let Some(Shown::Code(buffer)) = &mut self.shown else { return };
        let Some(name) = self.sound.keys.of.clone() else { return };
        let text = buffer.code.text().to_string();
        let at = buffer.code.cursor(ctx, id).unwrap_or(0);
        let Some(block) = instrument::at_cursor(&text, at).filter(|b| b.name == name).or_else(|| instrument::named(&text, &name)) else { return };
        let (_, new) = instrument::roll(&text, &block, sort, seed, self.sound.keys.octave);
        let cursor = instrument::cursor_after(&text, &new, &block, at);
        buffer.code.mark(ctx, id);
        buffer.code.set_text(new);
        buffer.code.check();
        buffer.code.put_cursor(ctx, id, cursor);
        buffer.code.mark(ctx, id);
        buffer.edited(std::time::Instant::now());
        self.play_key();
    }

    /// The key last played, the instrument the cursor was in as its text
    /// stands, rendered for the speaker (which takes it with `heard` after
    /// `Ask::Sound`) at the game's music volume and the player's.
    pub fn play_key(&mut self) {
        let Some(Shown::Code(buffer)) = &self.shown else { return };
        let Some(name) = self.sound.keys.of.clone() else { return };
        let text = buffer.code.text();
        let Some(block) = instrument::named(text, &name) else { return };
        // Its recordings as the game's folder holds them now, if it plays any.
        let samples = match instrument::uses_samples(text, &block) {
            true => match crate::audio::music::Samples::read_folder(&crate::files::Files::folder(&self.dir)) {
                Ok(s) => s,
                Err(e) => return self.tell(format!("{e:#}")),
            },
            false => crate::audio::music::Samples::default(),
        };
        let keys = &self.sound.keys;
        match instrument::note(text, &block, keys.octave, keys.key, &samples) {
            Ok(samples) if samples.iter().all(|x| *x == 0.0) => {}
            Ok(samples) => {
                let gain = crate::settings::Settings::load(&crate::settings::path_for(&self.dir)).music_gain();
                self.sound.again_at = Some(std::time::Instant::now() + std::time::Duration::from_secs_f32(samples.len() as f32 / crate::audio::SAMPLE_RATE as f32) + LOOP_PAUSE);
                self.sound.heard = Some((samples.into(), self.cfg.audio.music_volume.max(0.0) * gain));
                self.message = None;
                self.asks.push(Ask::Sound);
            }
            Err(e) => self.tell(format!("@{name}: {e:#}")),
        }
    }

    /// A sort rolled as the instrument maker is next drawn, from `seed`
    /// (`voxcade ide --shot --roll`).
    pub fn ask_roll(&mut self, sort: instrument::Sort, seed: u64) {
        self.sound.keys.asked = Some((sort, seed));
    }

    /// PLAY, LOOP and UNDO's sound again: over a song the key last played,
    /// over a sound the sound.
    pub(super) fn play_again(&mut self) {
        if self.memory.open.as_deref().is_some_and(|p| p.ends_with(".mml")) {
            self.play_key();
        } else {
            self.play_sound();
        }
    }
}
