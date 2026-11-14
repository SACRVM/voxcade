//! What the user sees, for the agent (`view_state`).

use super::*;

impl Shell {
    /// What the user sees, for the agent (`view_state`): the game, or the
    /// file open with its cursor, selection, changes and the line Lua
    /// cannot read.
    pub fn state(&self, ctx: &egui::Context) -> serde_json::Value {
        use serde_json::json;
        let game = self.stage.as_ref().map(|s| json!({ "shown": s.shown, "stopped_by": s.trouble }));
        if self.stage.as_ref().is_some_and(|s| s.shown) {
            return json!({ "area": "game", "game": game, "file": self.memory.open });
        }
        if let Some(name) = self.sound.builtin {
            return json!({ "area": "file", "file": { "kind": format!("the built-in sound {name}, read-only; MAKE IT YOURS copies it into the game") }, "game": game });
        }
        let mut file = json!({ "path": self.outside.as_ref().map(|p| p.display().to_string()).or_else(|| self.memory.open.clone()) });
        match (&self.shown, &self.memory.open) {
            (Some(Shown::Code(buffer)), Some(path)) => {
                let text = buffer.code.text();
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
                    file["kind"] = json!("a sound, to change; PLAY over its text plays it, a sort's button (COIN, JUMP, ...) rolls a fresh one, the faders move it");
                }
                if let Some((line, why)) = &buffer.code.error {
                    file["unreadable"] = json!({ "line": line, "why": why });
                }
                if path.ends_with(".mml") {
                    file["playing"] = json!(self.sound.listening.is_some());
                }
            }
            (Some(Shown::Text(_)), Some(path)) if path.ends_with(".mml") => {
                file["kind"] = json!("a song (MML), to read; PLAY SONG over it plays it");
                file["playing"] = json!(self.sound.listening.is_some());
            }
            (Some(Shown::Note(note)), Some(path)) if sounds::name_of(path).is_some() => file["kind"] = json!(format!("a sound file that would not open as text: {note}")),
            (Some(Shown::Text(_)), _) => file["kind"] = json!("text, to read"),
            (Some(Shown::Image(_)), _) => file["kind"] = json!("picture"),
            _ if self.pictures.preview.is_some() => file["kind"] = json!("a model, block or level, shown as a picture; its editor opens with EDIT"),
            _ => {}
        }
        json!({ "area": "file", "file": file, "game": game })
    }
}
