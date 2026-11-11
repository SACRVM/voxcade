//! The console at the bottom of the IDE: a terminal in the game folder
//! (`terminal.rs`) and a button that starts the agent in it (Claude Code;
//! how, the IDE's settings say). It folds away to its heading. A click in it gives it the keyboard; Ctrl+C there
//! copies what is selected, or else interrupts the program.

use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::term::TermMode;
use egui::{Color32, Event, EventFilter, FontId, Id, Key, Pos2, Rect, RichText, Sense, TextStyle, Vec2};

use crate::gui::{ACCENT, ERROR, INSET, PANEL, QUIET, ROW_HOVERED, ROW_OPEN, TEXT, arrow};
use super::prefs::Agent;
use super::terminal::{self, Size, Terminal, BACKGROUND, FOREGROUND};

/// What the console runs.
pub enum Start {
    /// The user's shell.
    Shell,
    /// One command through the system's shell (a shot's `--console`).
    Command(String),
    /// Nothing (a shot without `--console`).
    Nothing,
}

/// What was pressed in the heading.
#[derive(Default)]
pub struct Heading {
    /// The button that starts the agent.
    pub agent: bool,
    /// CLEAR, over the output.
    pub clear: bool,
}

/// Selected letters' ground, as in the rest of the IDE.
const SELECTED: Color32 = Color32::from_rgb(70, 64, 86);

/// Room around the terminal's cells.
const MARGIN: Vec2 = Vec2::new(20.0, 6.0);

/// The console's state between frames.
pub struct Console {
    dir: PathBuf,
    start: Start,
    wake: Arc<dyn Fn() + Send + Sync>,
    terminal: Option<Terminal>,
    /// Why the shell could not start.
    failed: Option<String>,
    /// Bytes typed before the terminal ran (the agent's command).
    typed: Vec<u8>,
    /// The console takes the keys when next drawn (the agent started:
    /// asked for in a click's frame, egui would take them back, the click
    /// being elsewhere).
    take_keys: bool,
    /// Text just selected with the mouse, for the system's selection
    /// (Linux's primary: a middle click pastes it elsewhere), and a middle
    /// click asking for it here.
    selected: Option<String>,
    wants_selection: bool,
    /// For a program that hears the mouse: the button held since pressed
    /// in the console, and the cell last told.
    held: Option<u8>,
    told: Option<(usize, usize)>,
    /// The wheel's part of a line not scrolled yet.
    wheel: f32,
    /// Whether the game folder's `.mcp.json` names `voxcade mcp` (looked
    /// at once, and after the MCP button), and what went wrong writing it.
    mcp: Option<bool>,
    mcp_failed: Option<String>,
    /// What went wrong starting the agent, to say.
    trouble: Option<String>,
    /// A command was typed into this shell (the agent's), for systems
    /// that cannot tell what runs (`busy`).
    typed_once: bool,
}

impl Console {
    /// A console in the game folder `dir` running `start`; `wake` wakes
    /// the window (from another thread) when there is output to draw.
    pub fn new(dir: PathBuf, start: Start, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { dir, start, wake, terminal: None, failed: None, typed: Vec::new(), take_keys: false, selected: None, wants_selection: false, held: None, told: None, wheel: 0.0, mcp: None, mcp_failed: None, trouble: None, typed_once: false }
    }

    /// Types `command` (and Enter) into the shell, `anew`: a new one
    /// started for it (so is one that ended); the console takes the keys.
    pub fn type_command(&mut self, command: &str, anew: bool) {
        if matches!(self.start, Start::Shell) && (anew || self.ended()) {
            self.terminal = None;
            self.typed.clear();
        }
        self.typed.extend_from_slice(command.as_bytes());
        self.typed.push(b'\r');
        self.take_keys = true;
        self.typed_once = true;
    }

    /// Whether a program runs in it besides the shell (Claude, a build):
    /// leaving the game would end it. Where the system cannot tell,
    /// whether the agent was started in the shell still running.
    pub fn busy(&self) -> bool {
        match &self.terminal {
            Some(t) if !t.ended() => t.busy().unwrap_or(self.typed_once),
            _ => false,
        }
    }

    /// Says what went wrong starting the agent (None: nothing).
    pub fn tell(&mut self, trouble: Option<String>) {
        self.trouble = trouble;
    }

    /// Whether nothing runs in it (any more).
    pub fn ended(&self) -> bool {
        self.terminal.as_ref().is_none_or(Terminal::ended)
    }

    /// Text just selected with the mouse, once.
    pub fn take_selected(&mut self) -> Option<String> {
        self.selected.take()
    }

    /// Whether a middle click asked for the system's selection, once.
    pub fn wants_selection(&mut self) -> bool {
        std::mem::take(&mut self.wants_selection)
    }

    /// Pastes `text` into the program running (as typed, marked as pasted
    /// if it asked for that).
    pub fn paste(&mut self, text: &str) {
        if let Some(t) = &self.terminal {
            t.paste(text);
        }
    }

    /// The agent's remote (`console_send`): `text` pasted as the program
    /// asked, then Enter if `enter`; what to answer, or why not.
    pub fn send(&mut self, text: &str, enter: bool) -> Result<String, String> {
        let Some(t) = self.terminal.as_ref().filter(|t| !t.ended()) else {
            return Err("the console has no program running (it starts when the console is drawn: show it, or press CLAUDE)".into());
        };
        t.paste(text);
        if enter {
            t.write(&b"\r"[..]);
        }
        Ok(format!("{} characters typed into the console{}", text.chars().count(), if enter { ", then Enter" } else { "" }))
    }

    /// What the terminal holds, the last `lines` lines as plain text
    /// (`console_read`).
    pub fn read(&self, lines: usize) -> Result<String, String> {
        let t = self.terminal.as_ref().ok_or("the console has not started (it starts when the console is drawn)")?;
        Ok(t.with(|term| terminal::history_text(term, lines)))
    }

    /// The heading: CONSOLE and OUTPUT to switch between as tabs
    /// (`output`: the output shown; `unseen` its errors not seen yet), the
    /// arrow that folds it (only the arrow); for the console the button
    /// that starts `agent` and the MCP button while the agent is not told
    /// of the IDE (`told_of_ide`) and the game folder names no `voxcade
    /// mcp`, for the output CLEAR. A band of its own, a line under it: what
    /// scrolls below stays apart.
    pub fn heading(&mut self, ui: &mut egui::Ui, folded: &mut bool, agent: Agent, told_of_ide: bool, output: &mut bool, unseen: usize) -> Heading {
        let height = ui.text_style_height(&TextStyle::Button) + 2.0 * ui.spacing().button_padding.y + 8.0;
        let (id, rect) = ui.allocate_space(egui::vec2(ui.available_width(), height));
        ui.painter().rect_filled(rect, 0.0, PANEL);
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, ROW_OPEN));
        let fold = egui::Rect::from_center_size(egui::pos2(rect.right() - INSET - 4.0, rect.center().y), egui::vec2(28.0, height - 10.0));
        let tip = if *folded { "Opens the console again." } else { "Folds the console away to this line." };
        let fold_response = ui.interact(fold, id.with("fold"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
        if fold_response.hovered() {
            ui.painter().rect_filled(fold, 3.0, ROW_HOVERED);
        }
        if fold_response.clicked() {
            *folded = !*folded;
        }
        let openness = ui.ctx().animate_bool_responsive(id, !*folded);
        arrow(ui, fold.center(), openness);
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(MARGIN.x, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let note = match (&self.failed, &self.terminal) {
            _ if self.trouble.is_some() => format!("  {}", self.trouble.as_deref().unwrap_or_default()),
            _ if self.mcp_failed.is_some() => "  .mcp.json could not be written".into(),
            (Some(_), _) => "  the shell could not start".into(),
            (None, Some(t)) if t.ended() && matches!(self.start, Start::Shell) => "  the shell has ended: Enter starts a new one".into(),
            _ => String::new(),
        };
        let mut pressed = Heading::default();
        // The two to switch between as tabs: the one shown lit and
        // underlined, the other lit where the mouse is; a click on either
        // opens the console folded away.
        let tab = |ui: &mut egui::Ui, text: RichText, on: bool| {
            let galley = egui::WidgetText::from(text.small()).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Small);
            let (r, response) = ui.allocate_exact_size(egui::vec2(galley.size().x + 20.0, rect.height()), Sense::click());
            if response.hovered() && !on {
                ui.painter().rect_filled(r.shrink2(egui::vec2(0.0, 6.0)), 3.0, ROW_HOVERED);
            }
            ui.painter().galley(r.center() - galley.size() / 2.0, galley, TEXT);
            if on {
                ui.painter().hline(r.shrink2(egui::vec2(4.0, 0.0)).x_range(), rect.bottom() - 1.5, egui::Stroke::new(2.0, ACCENT));
            }
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        };
        inner.add_space(-10.0);
        if tab(&mut inner, RichText::new("CONSOLE").color(if *output { QUIET } else { TEXT }), !*output).on_hover_text("The shell in the game folder, Claude's place.").clicked() {
            (*output, *folded) = (false, false);
        }
        inner.add_space(4.0);
        let label = if unseen > 0 { RichText::new(format!("OUTPUT {unseen}")).color(ERROR) } else { RichText::new("OUTPUT").color(if *output { TEXT } else { QUIET }) };
        if tab(&mut inner, label, *output).on_hover_text("What BUILD made, the game's errors, what the script printed.").clicked() {
            (*output, *folded) = (true, false);
        }
        inner.add_space(8.0);
        if !*output {
            inner.label(RichText::new(note).small().color(QUIET));
        }
        inner.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(INSET + 14.0);
            if *output {
                if ui.button("CLEAR").on_hover_text("Forgets every line of the output.").clicked() {
                    pressed.clear = true;
                }
                return;
            }
            let tip = format!("Starts {} in the console, as the settings (top right) say.", agent.called());
            if ui.button(agent.button()).on_hover_text(tip).clicked() {
                *folded = false;
                pressed.agent = true;
            }
            // The agent not told of the IDE, until the game folder's
            // .mcp.json names voxcade mcp: the button that writes it
            // (Claude Code started here then has the game's and the IDE's
            // tools; it asks once).
            if !told_of_ide && !*self.mcp.get_or_insert_with(|| has_mcp(&self.dir)) {
                let tip = "Writes .mcp.json into the game folder: Claude Code started here finds voxcade mcp, the game's tools and the IDE's (what you see, the game you play). It asks once whether to trust it.";
                if ui.button("MCP").on_hover_text(tip).clicked() {
                    self.mcp_failed = write_mcp(&self.dir).err().map(|e| format!("{e:#}"));
                    self.mcp = None;
                }
            }
        });
        pressed
    }

    /// The terminal, filling what is left of `ui`.
    pub fn body(&mut self, ui: &mut egui::Ui) {
        let id = Id::unique("terminal");
        let rect = ui.available_rect_before_wrap();
        ui.advance_cursor_after_rect(rect);
        let response = ui.interact(rect, id, Sense::click_and_drag());
        if std::mem::take(&mut self.take_keys) {
            response.request_focus();
        }
        let font = ui.style().text_styles[&TextStyle::Monospace].clone();
        let cell = ui.fonts_mut(|f| Vec2::new(f.glyph_width(&font, 'M'), f.row_height(&font)));
        let inner = rect.shrink2(MARGIN);
        let size = Size { columns: (inner.width() / cell.x).floor().clamp(20.0, 1000.0) as u16, lines: (inner.height() / cell.y).floor().clamp(2.0, 500.0) as u16 };
        let pixels = ui.ctx().pixels_per_point();
        let cell_pixels = [(cell.x * pixels).round() as u16, (cell.y * pixels).round() as u16];

        if let Some(failed) = &self.failed {
            ui.painter().text(inner.min, egui::Align2::LEFT_TOP, failed, font, QUIET);
            return;
        }
        if self.terminal.is_none() {
            let command = match &self.start {
                Start::Shell => None,
                Start::Command(c) => Some(c.as_str()),
                Start::Nothing => return,
            };
            let wake = self.wake.clone();
            match Terminal::start(&self.dir, command, size, cell_pixels, move || wake()) {
                Ok(t) => self.terminal = Some(t),
                Err(e) => {
                    self.failed = Some(format!("{e:#}"));
                    return;
                }
            }
        }
        let terminal = self.terminal.as_mut().expect("started above");
        terminal.resize(size, cell_pixels);
        if !self.typed.is_empty() {
            terminal.write(std::mem::take(&mut self.typed));
        }
        for text in terminal.copies() {
            ui.ctx().copy_text(text);
        }

        // The mouse. A program that asked for it (Claude Code's full
        // screen) hears it, buttons, drags and the wheel, as xterm tells it;
        // with Shift held it is the console's, as for any other: a click
        // gives the console the keys, a drag selects, a double click
        // selects a word, the wheel goes through the history.
        let at = |p: Pos2| ((p.x - inner.left()) / cell.x, (p.y - inner.top()) / cell.y);
        let mode = terminal.with(|t| *t.mode());
        let heard = mode.intersects(TermMode::MOUSE_MODE) && !ui.input(|i| i.modifiers.shift);
        if heard {
            let cell_of = |p: Pos2| {
                let (x, y) = at(p);
                ((x.max(0.0) as usize).min(size.columns as usize - 1), (y.max(0.0) as usize).min(size.lines as usize - 1))
            };
            let number = |b: egui::PointerButton| match b {
                egui::PointerButton::Primary => Some(0),
                egui::PointerButton::Middle => Some(1),
                egui::PointerButton::Secondary => Some(2),
                _ => None,
            };
            // (Only where nothing lies over the console: a menu, NEW's form.)
            let (ctx, layer) = (ui.ctx().clone(), ui.layer_id());
            let over_console = |p: Pos2| rect.contains(p) && ctx.layer_id_at(p) == Some(layer);
            for event in ui.input(|i| i.events.clone()) {
                let (mouse, pos, modifiers) = match event {
                    Event::PointerButton { pos, button, pressed: true, modifiers } if over_console(pos) => {
                        let Some(b) = number(button) else { continue };
                        response.request_focus();
                        self.held = Some(b);
                        (terminal::Mouse::Press(b), pos, modifiers)
                    }
                    Event::PointerButton { pos, button, pressed: false, modifiers } if self.held.is_some() && number(button) == self.held => {
                        let b = self.held.take().expect("held");
                        (terminal::Mouse::Release(b), pos, modifiers)
                    }
                    Event::PointerMoved(pos) if (over_console(pos) || self.held.is_some()) && self.told != Some(cell_of(pos)) => {
                        (terminal::Mouse::Moved(self.held), pos, ui.input(|i| i.modifiers))
                    }
                    _ => continue,
                };
                let (column, line) = cell_of(pos);
                self.told = Some((column, line));
                if let Some(bytes) = terminal::mouse_bytes(mouse, column, line, modifiers, mode) {
                    terminal.write(bytes);
                }
            }
        } else {
            if response.clicked() || response.drag_started() {
                response.request_focus();
            }
            if response.drag_started()
                && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            {
                let (x, y) = at(origin);
                terminal.with(|t| terminal::select(t, terminal::point_at(t, y as usize, x as usize), terminal::side(x), Some(false)));
            } else if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let (x, y) = at(pos);
                terminal.with(|t| terminal::select(t, terminal::point_at(t, y as usize, x as usize), terminal::side(x), None));
            } else if response.double_clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let (x, y) = at(pos);
                terminal.with(|t| terminal::select(t, terminal::point_at(t, y as usize, x as usize), terminal::side(x), Some(true)));
            } else if response.clicked() {
                terminal.with(|t| t.selection = None);
            }
            if response.drag_stopped() || response.double_clicked() {
                self.selected = terminal.with(|t| t.selection_to_string()).filter(|s| !s.is_empty());
            }
            if response.middle_clicked() {
                response.request_focus();
                self.wants_selection = true;
            }
        }
        if response.hovered() {
            self.wheel += ui.input(|i| i.smooth_scroll_delta().y) / cell.y;
            let lines = self.wheel.trunc() as i32;
            self.wheel -= lines as f32;
            if lines != 0 {
                if heard && let Some(pos) = response.hover_pos() {
                    // The program scrolls itself, a wheel step a line.
                    let (x, y) = at(pos);
                    let (column, line) = ((x.max(0.0) as usize).min(size.columns as usize - 1), (y.max(0.0) as usize).min(size.lines as usize - 1));
                    let step = if lines > 0 { terminal::Mouse::WheelUp } else { terminal::Mouse::WheelDown };
                    let modifiers = ui.input(|i| i.modifiers);
                    let bytes = terminal::mouse_bytes(step, column, line, modifiers, mode).unwrap_or_default();
                    terminal.write(bytes.repeat(lines.unsigned_abs() as usize));
                } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
                    // A full-screen program scrolls itself: the arrow keys.
                    let key = if lines > 0 { Key::ArrowUp } else { Key::ArrowDown };
                    let bytes = terminal::key_bytes(key, egui::Modifiers::default(), mode).unwrap_or_default();
                    terminal.write(bytes.repeat(lines.unsigned_abs() as usize));
                } else {
                    terminal.with(|t| terminal::scroll(t, lines));
                }
            }
        }

        // The keys, while the console has them: all of them, Tab, the
        // arrows and Escape too.
        let focused = response.has_focus();
        if focused {
            ui.memory_mut(|m| m.set_focus_lock_filter(id, EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }));
            let restart = terminal.ended() && matches!(self.start, Start::Shell);
            for event in ui.input(|i| i.events.clone()) {
                match event {
                    Event::Key { key: Key::Enter, pressed: true, .. } if restart => {
                        self.terminal = None;
                        ui.ctx().request_repaint();
                        return;
                    }
                    Event::Text(text) | Event::Ime(egui::ImeEvent::Commit(text)) => terminal.write(text.into_bytes()),
                    Event::Paste(text) => terminal.paste(&text),
                    Event::Copy => match terminal.with(|t| t.selection_to_string()).filter(|s| !s.is_empty()) {
                        Some(text) => ui.ctx().copy_text(text),
                        None => terminal.write(vec![0x03]),
                    },
                    Event::Cut => terminal.write(vec![0x18]),
                    Event::Key { key: Key::C, pressed: true, modifiers, .. } if modifiers.ctrl && modifiers.shift => {
                        if let Some(text) = terminal.with(|t| t.selection_to_string()) {
                            ui.ctx().copy_text(text);
                        }
                    }
                    Event::Key { key, pressed: true, modifiers, .. } => {
                        let mode = terminal.with(|t| *t.mode());
                        if let Some(bytes) = terminal::key_bytes(key, modifiers, mode) {
                            terminal.write(bytes);
                        }
                    }
                    _ => {}
                }
            }
        }

        draw(ui, inner, cell, &font, &terminal.with(|t| terminal::picture(t)), focused);
    }
}

/// Whether the game folder's `.mcp.json` names `voxcade mcp`.
fn has_mcp(dir: &std::path::Path) -> bool {
    std::fs::read_to_string(dir.join(".mcp.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).is_some_and(|v| v["mcpServers"]["voxcade"].is_object())
}

/// Writes `voxcade mcp` (this program) into the game folder's
/// `.mcp.json`, keeping what else it names.
fn write_mcp(dir: &std::path::Path) -> anyhow::Result<()> {
    let path = dir.join(".mcp.json");
    let mut config: serde_json::Value = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("{}: {e} (left as it is)", path.display()))?,
        Err(_) => serde_json::json!({}),
    };
    let exe = crate::cli::program()?;
    config["mcpServers"]["voxcade"] = serde_json::json!({ "type": "stdio", "command": exe, "args": ["mcp"] });
    crate::blocks::save_at_once(&path, (serde_json::to_string_pretty(&config)? + "\n").as_bytes())
}

/// The terminal's cells into `inner`, `cell` points each.
fn draw(ui: &egui::Ui, inner: Rect, cell: Vec2, font: &FontId, picture: &terminal::Picture, focused: bool) {
    let painter = ui.painter_at(inner.expand2(MARGIN));
    let at = |x: usize, y: usize| egui::pos2(inner.left() + x as f32 * cell.x, inner.top() + y as f32 * cell.y);
    for (y, line) in picture.lines.iter().enumerate() {
        // The grounds first.
        for (x, g) in line.iter().enumerate() {
            let bg = if g.selected { SELECTED } else { g.bg };
            if bg != BACKGROUND && !g.spacer {
                let width = if g.wide { 2.0 } else { 1.0 };
                painter.rect_filled(Rect::from_min_size(at(x, y), egui::vec2(cell.x * width, cell.y)), 0.0, bg);
            }
        }
        // Then the letters: runs of plain ones in one colour together,
        // every other letter at its own cell (its font may be wider).
        let mut run = (0, FOREGROUND, String::new());
        let flush = |run: &mut (usize, Color32, String)| {
            if !run.2.trim().is_empty() {
                painter.text(at(run.0, y), egui::Align2::LEFT_TOP, &run.2, font.clone(), run.1);
            }
            run.2.clear();
        };
        for (x, g) in line.iter().enumerate() {
            if g.spacer {
                continue;
            }
            if g.underline {
                let bottom = at(x, y + 1).y - 1.0;
                painter.hline(at(x, y).x..=at(x + 1, y).x, bottom, egui::Stroke::new(1.0, g.fg));
            }
            if g.c.is_ascii() && !g.c.is_ascii_control() {
                if run.2.is_empty() || run.1 != g.fg {
                    flush(&mut run);
                    run.0 = x;
                    run.1 = g.fg;
                }
                run.2.push(g.c);
            } else {
                flush(&mut run);
                painter.text(at(x, y), egui::Align2::LEFT_TOP, g.c.to_string(), font.clone(), g.fg);
            }
        }
        flush(&mut run);
    }
    // The cursor: a block over its letter while the console has the keys,
    // a frame around it when not.
    if let Some((y, x, shape)) = picture.cursor {
        let r = Rect::from_min_size(at(x, y), cell);
        let under = picture.lines.get(y).and_then(|l| l.get(x)).copied().unwrap_or_default();
        use alacritty_terminal::vte::ansi::CursorShape as Shape;
        match (focused, shape) {
            (true, Shape::Beam) => {
                painter.rect_filled(Rect::from_min_size(r.min, egui::vec2(2.0, cell.y)), 0.0, FOREGROUND);
            }
            (true, Shape::Underline) => {
                painter.rect_filled(Rect::from_min_max(egui::pos2(r.left(), r.bottom() - 2.0), r.max), 0.0, FOREGROUND);
            }
            (true, _) => {
                painter.rect_filled(r, 0.0, FOREGROUND);
                painter.text(r.min, egui::Align2::LEFT_TOP, under.c.to_string(), font.clone(), BACKGROUND);
            }
            (false, _) => {
                painter.rect_stroke(r.shrink(0.5), 0.0, egui::Stroke::new(1.0, QUIET), egui::StrokeKind::Inside);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CLAUDE pressed gives the console the keys (asked for in the
    /// click's own frame, egui took them back: the click was elsewhere).
    #[test]
    fn claude_gives_the_console_the_keys() {
        let dir = std::env::temp_dir().join(format!("voxcade-console-keys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut console = Console::new(dir.clone(), Start::Nothing, Arc::new(|| {}));
        let ctx = egui::Context::default();
        let mut folded = false;
        let mut frame = |events: Vec<egui::Event>, console: &mut Console| {
            let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 400.0))), events, ..Default::default() };
            let _ = ctx.run_ui(raw, |ui| {
                // (The shell types the agent's command.)
                if console.heading(ui, &mut folded, Agent::Claude, true, &mut false, 0).agent {
                    console.type_command("claude", false);
                }
                console.body(ui);
            });
        };
        frame(Vec::new(), &mut console);
        // The heading's button: CLAUDE (the rightmost but the arrow's
        // narrower square).
        let button = ctx.viewport(|v| {
            let widgets = &v.prev_pass.widgets;
            widgets.layer_ids().flat_map(|l| widgets.get_layer(l).cloned().collect::<Vec<_>>()).filter(|w| (40.0..200.0).contains(&w.rect.width()) && w.rect.top() < 40.0).max_by(|a, b| a.rect.right().total_cmp(&b.rect.right())).map(|w| w.rect)
        });
        let at = button.expect("the CLAUDE button").center();
        frame(vec![egui::Event::PointerMoved(at)], &mut console);
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame(vec![press(true), press(false)], &mut console);
        assert_eq!(console.typed, b"claude\r", "CLAUDE was pressed");
        frame(Vec::new(), &mut console);
        assert_eq!(ctx.memory(|m| m.focused()), Some(Id::unique("terminal")), "the console has the keys");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only the arrow folds the console: a click beside the tabs leaves it
    /// as it is.
    #[test]
    fn only_the_arrow_folds() {
        let dir = std::env::temp_dir().join(format!("voxcade-console-fold-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut console = Console::new(dir.clone(), Start::Nothing, Arc::new(|| {}));
        let ctx = egui::Context::default();
        let mut folded = false;
        let mut click = |at: Pos2, folded: &mut bool| {
            for events in [vec![egui::Event::PointerMoved(at)], vec![true, false].into_iter().map(|pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }).collect()] {
                let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 400.0))), events, ..Default::default() };
                let _ = ctx.run_ui(raw, |ui| {
                    console.heading(ui, folded, Agent::Claude, true, &mut false, 0);
                });
            }
        };
        click(Pos2::new(400.0, 16.0), &mut folded);
        assert!(!folded, "a click beside the tabs");
        click(Pos2::new(800.0 - INSET - 4.0, 16.0), &mut folded);
        assert!(folded, "the arrow");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Text dragged over with the mouse goes to the system's selection,
    /// and to the clipboard with Ctrl+C or Ctrl+Shift+C.
    #[test]
    fn what_is_selected_is_copied() {
        let dir = std::env::temp_dir().join(format!("voxcade-console-copy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut console = Console::new(dir.clone(), Start::Command("printf 'hello world'; sleep 3".into()), Arc::new(|| {}));
        let ctx = egui::Context::default();
        let frame = |events: Vec<egui::Event>, console: &mut Console| {
            let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 400.0))), events, ..Default::default() };
            let out = ctx.run_ui(raw, |ui| console.body(ui));
            out.platform_output.commands.iter().filter_map(|c| if let egui::OutputCommand::CopyText(t) = c { Some(t.clone()) } else { None }).collect::<Vec<_>>()
        };
        frame(Vec::new(), &mut console);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !console.terminal.as_ref().unwrap().with(|t| terminal::picture(t)).lines[0].iter().map(|g| g.c).collect::<String>().starts_with("hello") {
            assert!(std::time::Instant::now() < until, "the text came");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // From the first letter to the fifth, as the mouse drags.
        let font = ctx.global_style().text_styles[&TextStyle::Monospace].clone();
        let cell = ctx.fonts_mut(|f| Vec2::new(f.glyph_width(&font, 'M'), f.row_height(&font)));
        let (from, to) = (MARGIN + Vec2::new(0.2 * cell.x, 0.5 * cell.y), MARGIN + Vec2::new(4.8 * cell.x, 0.5 * cell.y));
        let button = |at: Vec2, pressed| egui::Event::PointerButton { pos: at.to_pos2(), button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame(vec![egui::Event::PointerMoved(from.to_pos2()), button(from, true)], &mut console);
        for i in 1..=10 {
            frame(vec![egui::Event::PointerMoved((from + (to - from) * i as f32 / 10.0).to_pos2())], &mut console);
        }
        frame(vec![button(to, false)], &mut console);
        assert_eq!(console.take_selected().as_deref(), Some("hello"), "for the system's selection");
        assert_eq!(frame(vec![egui::Event::Copy], &mut console), ["hello"], "Ctrl+C");
        let shift = egui::Modifiers { ctrl: true, shift: true, command: true, ..Default::default() };
        let key = egui::Event::Key { key: Key::C, physical_key: None, pressed: true, repeat: false, modifiers: shift };
        assert_eq!(frame(vec![key], &mut console), ["hello"], "Ctrl+Shift+C");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A program that asked for the mouse (as Claude Code's full screen
    /// does) hears the wheel and the buttons, not arrow keys.
    #[test]
    fn a_program_that_asks_for_the_mouse_hears_it() {
        let dir = std::env::temp_dir().join(format!("voxcade-console-mouse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let heard = dir.join("heard");
        let command = format!("printf '\\033[?1049h\\033[?1002h\\033[?1006h'; stty raw -echo; cat > '{}'", heard.display());
        let mut console = Console::new(dir.clone(), Start::Command(command), Arc::new(|| {}));
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut frame = |events: Vec<egui::Event>, console: &mut Console| {
            time += 1.0 / 60.0;
            let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 400.0))), events, time: Some(time), ..Default::default() };
            let _ = ctx.run_ui(raw, |ui| console.body(ui));
        };
        frame(Vec::new(), &mut console);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !console.terminal.as_ref().unwrap().with(|t| t.mode().contains(TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE)) {
            assert!(std::time::Instant::now() < until, "the program asked for the mouse");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        let at = Pos2::new(100.0, 100.0);
        frame(vec![egui::Event::PointerMoved(at)], &mut console);
        frame(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: Vec2::new(0.0, 3.0), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: egui::Modifiers::NONE }], &mut console);
        for _ in 0..60 {
            frame(Vec::new(), &mut console);
        }
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame(vec![button(true)], &mut console);
        frame(vec![button(false)], &mut console);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let got = loop {
            let got = String::from_utf8_lossy(&std::fs::read(&heard).unwrap_or_default()).into_owned();
            if got.contains("m") && got.contains("<0;") || std::time::Instant::now() > until {
                break got;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(got.contains("\x1b[<64;"), "the wheel up as button 64: {got:?}");
        assert!(!got.contains("\x1b[A") && !got.contains("\x1bOA"), "no arrow keys: {got:?}");
        assert!(got.contains("M") && got.contains("\x1b[<0;") && got.ends_with('m'), "pressed and let go: {got:?}");
        assert!(console.terminal.as_ref().unwrap().with(|t| t.selection.is_none()), "the console selected nothing");
        drop(console);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The MCP button names voxcade mcp in the game folder's .mcp.json,
    /// keeping the servers it named already.
    #[test]
    fn the_mcp_button_names_voxcade_in_the_game_folder() {
        let dir = std::env::temp_dir().join(format!("voxcade-mcp-json-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!has_mcp(&dir));
        std::fs::write(dir.join(".mcp.json"), r#"{"mcpServers":{"other":{"command":"x"}}}"#).unwrap();
        write_mcp(&dir).unwrap();
        assert!(has_mcp(&dir));
        let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(config["mcpServers"]["other"]["command"], "x", "kept");
        assert_eq!(config["mcpServers"]["voxcade"]["args"][0], "mcp");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
