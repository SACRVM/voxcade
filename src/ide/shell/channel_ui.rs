//! A channel's controls on a song's page (under the instrument maker): the
//! channel the text cursor is in (or the lines or notes selected), PLAY
//! CHANNEL and LOOP CHANNEL (it alone, through the song's speaker), and
//! OCTAVE and TRANSPOSE, each a press its notes moved in the text
//! (`music::shift`), one step back for UNDO and Ctrl+Z alike.

use super::*;
use crate::audio::music::shift::{self, Move, Picked};

/// The channel's controls' own state over a song open.
#[derive(Default)]
pub struct Strip {
    /// What the cursor or the selection picked, by the text's revision and
    /// the selection (bytes): looked for again only when either changes.
    seen: Option<(u64, (usize, usize), Option<Here>)>,
    /// LOOP CHANNEL: the channel plays round and round.
    pub(super) round: bool,
    /// TRANSPOSE pressed this many times (down where less than 0) as the
    /// controls are next drawn (`voxcade ide --shot --transpose`).
    pub(super) asked: Option<i32>,
}

/// What the controls work on, and why each cannot, where it cannot.
#[derive(Clone)]
struct Here {
    picked: Picked,
    /// OCTAVE -, OCTAVE +, TRANSPOSE -, TRANSPOSE +.
    why_not: [Option<String>; 4],
    play_why_not: Option<String>,
}

/// The moves of OCTAVE - and +, TRANSPOSE - and +.
const MOVES: [Move; 4] = [Move::Octaves(-1), Move::Octaves(1), Move::Semitones(-1), Move::Semitones(1)];

/// The byte of `text` at character `at`.
fn byte_at(text: &str, at: usize) -> usize {
    text.char_indices().nth(at).map_or(text.len(), |(b, _)| b)
}

/// The character of `text` at byte `at`.
fn char_at_byte(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].chars().count()
}

impl Shell {
    /// The channel's row over the song open (its text in the text edit
    /// `id`): what the cursor picked, PLAY CHANNEL, LOOP CHANNEL, OCTAVE
    /// and TRANSPOSE; quiet where the cursor is in no channel's line or the
    /// song does not read.
    pub(super) fn channel_ui(&mut self, ui: &mut egui::Ui, id: egui::Id) {
        let ctx = ui.ctx().clone();
        // Presses a shot asked for, once the cursor stands in a channel.
        let placed = matches!(&self.shown, Some(Shown::Code(b)) if b.code.selection(&ctx, id).is_some());
        if placed && self.channel_here(&ctx, id).is_some() && let Some(n) = self.sound.strip.asked.take() {
            for _ in 0..n.unsigned_abs() {
                self.move_channel(&ctx, id, Move::Semitones(n.signum()));
            }
        }
        let here = self.channel_here(&ctx, id);
        let Some(Shown::Code(buffer)) = &self.shown else { return };
        let mistake = buffer.code.error.as_ref().map(|(line, why)| format!("The song does not read (line {line}: {why}): mend it first."));
        let lead = match &here {
            Some(h) if h.picked.letters.len() == 1 => format!("CHANNEL {}", h.picked.letters[0]),
            Some(h) => format!("CHANNELS {}", h.picked.letters.iter().map(char::to_string).collect::<Vec<_>>().join(" ")),
            None => "NO CHANNEL".into(),
        };
        let nowhere = "Put the text cursor in a channel's line (A: notes), or select notes or lines: these work on that channel.";
        let why = |w: &Option<String>| mistake.clone().or_else(|| w.clone());
        let playing = self.sound.alone.is_some() && self.sound.listening.is_some();
        let (mut play, mut looped, mut moved) = (false, None, None);
        let lit = self.sound.strip.round && playing;
        // On one row where the room allows, else the moves on a row of
        // their own.
        let one_row = {
            let font = TextStyle::Button.resolve(ui.style());
            let pad = 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x;
            let words = [lead.as_str(), "PLAY CHANNEL", "LOOP CHANNEL", "OCTAVE", "-", "+", "TRANSPOSE", "-", "+"];
            let width: f32 = words.iter().map(|w| ui.fonts_mut(|f| f.layout_no_wrap(w.to_string(), font.clone(), TEXT).size().x) + pad).sum();
            width + 12.0 + 18.0 + 12.0 + 16.0 <= ui.available_width()
        };
        let mut controls = |ui: &mut egui::Ui| {
            let (color, hover) = if here.is_some() { (TEXT, "The channel the text cursor is in (or the lines or notes selected): what these play and move.") } else { (QUIET, nowhere) };
            ui.label(RichText::new(&lead).color(color)).on_hover_text(hover);
            ui.add_space(12.0);
            let play_why = here.as_ref().map_or(Some(nowhere.to_string()), |h| why(&h.play_why_not));
            let stretch = here.as_ref().is_some_and(|h| h.picked.stretch);
            let play_tip = if stretch { "Plays the notes selected alone, once, with the song's instruments, at the game's music volume (STOP SONG ends it)." } else { "Plays the channel alone from the song's start, once, with the song's instruments, at the game's music volume (STOP SONG ends it)." };
            play = ui.add_enabled(play_why.is_none(), egui::Button::new("PLAY CHANNEL")).on_hover_text(play_tip).on_disabled_hover_text(play_why.clone().unwrap_or_default()).clicked();
            let loop_tip = "Plays it alone round and round, so you hear what the moves and your typing do. Press again to stop.";
            if ui.add_enabled(play_why.is_none(), egui::Button::new("LOOP CHANNEL").selected(lit)).on_hover_text(loop_tip).on_disabled_hover_text(play_why.unwrap_or_default()).clicked() {
                looped = Some(!lit);
            }
        };
        let mut moves = |ui: &mut egui::Ui| {
            let words = [
                ("OCTAVE", "Its notes moved by octaves, as a synthesizer's octave switch: the octave commands (o4) change, the notes stay as they are written."),
                ("TRANSPOSE", "Its notes moved by semitones: each note written anew in the line's way of writing sharps and flats, a > or < where one crosses between b and c."),
            ];
            for (row, (word, hover)) in words.iter().enumerate() {
                ui.label(RichText::new(*word).color(QUIET)).on_hover_text(*hover);
                for (sign, tip) in [("-", if row == 0 { "An octave down." } else { "A semitone down." }), ("+", if row == 0 { "An octave up." } else { "A semitone up." })] {
                    let i = 2 * row + usize::from(sign == "+");
                    let why_not = here.as_ref().map_or(Some(nowhere.to_string()), |h| why(&h.why_not[i]));
                    if ui.add_enabled(why_not.is_none(), egui::Button::new(sign)).on_hover_text(tip).on_disabled_hover_text(why_not.unwrap_or_default()).clicked() {
                        moved = Some(MOVES[i]);
                    }
                }
                if row == 0 {
                    ui.add_space(12.0);
                }
            }
        };
        if one_row {
            ui.horizontal(|ui| {
                controls(ui);
                ui.add_space(18.0);
                moves(ui);
            });
        } else {
            ui.horizontal(|ui| controls(ui));
            ui.horizontal(|ui| moves(ui));
        }
        if let Some(mv) = moved {
            self.move_channel(&ctx, id, mv);
        }
        if play {
            self.sound.strip.round = false;
            self.play_channel(&ctx, id);
        }
        match looped {
            Some(true) => {
                self.sound.strip.round = true;
                self.play_channel(&ctx, id);
            }
            Some(false) => self.hush(),
            None => {}
        }
    }

    /// What the cursor or the selection of the text edit `id` picked, and
    /// why each control cannot, looked for again only when the text or the
    /// selection changed.
    fn channel_here(&mut self, ctx: &egui::Context, id: egui::Id) -> Option<Here> {
        let Some(Shown::Code(buffer)) = &self.shown else { return None };
        let text = buffer.code.text();
        let (from, to) = buffer.code.selection(ctx, id).unwrap_or((0, 0));
        let bytes = (byte_at(text, from), byte_at(text, to));
        let revision = buffer.code.revision();
        let strip = &mut self.sound.strip;
        if strip.seen.as_ref().is_none_or(|(r, b, _)| (*r, *b) != (revision, bytes)) {
            let here = shift::pick(text, bytes.0, bytes.1).map(|picked| {
                let why_not = MOVES.map(|mv| shift::can(text, &picked, mv).err());
                let play_why_not = shift::alone(text, &picked, false).err();
                Here { picked, why_not, play_why_not }
            });
            strip.seen = Some((revision, bytes, here));
        }
        strip.seen.as_ref().and_then(|(_, _, h)| h.clone())
    }

    /// OCTAVE or TRANSPOSE: the notes picked in the text edit `id` moved
    /// by `mv` (`music::shift`), one step back for UNDO, what was selected
    /// held over the notes as they are now; the channel played alone again
    /// where it plays.
    pub(super) fn move_channel(&mut self, ctx: &egui::Context, id: egui::Id, mv: Move) {
        let Some(Shown::Code(buffer)) = &mut self.shown else { return };
        let text = buffer.code.text().to_string();
        let (from, to) = buffer.code.selection(ctx, id).unwrap_or((0, 0));
        let (a, b) = (byte_at(&text, from), byte_at(&text, to));
        let Some(picked) = shift::pick(&text, a, b) else { return };
        match shift::shift(&text, &picked, mv) {
            Ok(moved) => {
                let (a, b) = if a == b { (moved.map(a, false), moved.map(a, false)) } else { (moved.map(a, true), moved.map(b, false)) };
                let (from, to) = (char_at_byte(&moved.text, a), char_at_byte(&moved.text, b));
                buffer.code.mark(ctx, id);
                buffer.code.set_text(moved.text);
                buffer.code.check();
                buffer.code.put_selection(ctx, id, from, to);
                buffer.code.mark(ctx, id);
                buffer.edited(std::time::Instant::now());
                if self.sound.alone.is_some() && self.sound.listening.is_some() {
                    self.play_channel(ctx, id);
                }
            }
            Err(why) => self.tell(why),
        }
    }

    /// PLAY CHANNEL (LOOP CHANNEL round and round): the channel picked in
    /// the text edit `id` alone, or the notes selected, through the song's
    /// speaker as PLAY SONG plays (STOP SONG ends it).
    pub(super) fn play_channel(&mut self, ctx: &egui::Context, id: egui::Id) {
        let Some(path) = self.memory.open.clone() else { return };
        let Some(Shown::Code(buffer)) = &self.shown else { return };
        let text = buffer.code.text();
        let (from, to) = buffer.code.selection(ctx, id).unwrap_or((0, 0));
        let Some(picked) = shift::pick(text, byte_at(text, from), byte_at(text, to)) else { return };
        match shift::alone(text, &picked, self.sound.strip.round) {
            Ok(song) => {
                self.sound.alone = Some(song);
                self.sound.listening = Some(path.clone());
                self.message = None;
                self.asks.push(Ask::Listen(path));
            }
            Err(why) => self.tell(why),
        }
    }

    /// TRANSPOSE pressed `n` times (down where less than 0) as the song's
    /// controls are next drawn (`voxcade ide --shot --transpose`).
    pub fn ask_transpose(&mut self, n: i32) {
        self.sound.strip.asked = Some(n);
    }
}
