//! Voxcade's IDE (`docs/ide.md`): one window to make a Lua game in, a very
//! stripped-down VS Code. On the left only the game's logic and its
//! assets, in the main area the one thing open (no tabs), PLAY and BUILD
//! along the top. It is drawn with egui on the engine's own window and GPU,
//! and it draws only when something happened.

pub mod bridge;
mod buffer;
mod claude;
mod code;
mod console;
mod export;
mod files;
mod find;
mod help;
mod maker;
mod memory;
mod output;
mod play;
mod prefs;
pub use prefs::is_notes_file;
mod preview;
mod project;
mod projects;
pub(crate) mod remote;
mod shell;
mod shot;
pub use shot::{ShotOptions, shot};
mod refs;
mod sounds;
mod templates;
mod terminal;
mod window;
pub mod wizard;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::game::Game;
use window::Window;

/// Where an event goes: to the IDE's window, or to the editor window of
/// that id.
#[derive(Debug, PartialEq)]
enum Route {
    Ide,
    Editor(u32),
}

/// Where `event` goes when the IDE's window has the id `ide`: an event of
/// another window to that window, the rest (and the program's own) to the
/// IDE.
fn route(event: &sdl3::event::Event, ide: u32) -> Route {
    match event.get_window_id() {
        Some(id) if id != ide && id != 0 => Route::Editor(id),
        _ => Route::Ide,
    }
}

/// What the IDE's window shows next: a game, the start page (saying what
/// went wrong, if something did), or nothing: it closes.
enum Next {
    Game(PathBuf),
    /// The start page with the games found below this folder.
    Found(PathBuf, Vec<PathBuf>),
    Start(Option<String>),
    /// A game made for another Voxcade: the upgrade's page (`play`: from
    /// the launcher's PLAY).
    Upgrade(PathBuf, bool),
    /// The launcher, playing this game at once (the upgrade's PLAY).
    Play(PathBuf),
    Quit,
}

/// Opens the IDE in a window on the game in `first` (or its start page)
/// until it is closed; another game opens in the same window. With
/// `remote` it answers the console's tools (`console_send`,
/// `console_read`) for another session testing it.
pub fn run<G: Game>(first: Option<&Path>, remote: bool) -> Result<()> {
    // Another session's keys arrive while the person works elsewhere.
    crate::gui::input::keep_focus(remote);
    let mut window = Window::open()?;
    let mut next = match first {
        Some(dir) => choose(&mut window.note, dir),
        None => Next::Start(None),
    };
    loop {
        next = match next {
            // A game made for another Voxcade: the upgrade's page first.
            Next::Game(dir) if wizard::needed(&dir) => Next::Upgrade(dir, false),
            Next::Game(dir) => match project::project::<G>(&mut window, &dir, remote) {
                Ok(next) => next,
                // A game that would not open: the start page says why.
                Err(e) => Next::Start(Some(format!("{}: {e:#}", dir.display()))),
            },
            Next::Upgrade(dir, play) => window::upgrade_page(&mut window, wizard::Wizard::new(&dir, play), crate::replay::refit_saying::<G>)?,
            Next::Start(message) => window::start_page(&mut window, message, None, None)?,
            Next::Found(folder, games) => window::start_page(&mut window, None, None, Some((folder, games)))?,
            Next::Play(dir) => window::start_page(&mut window, None, Some(dir), None)?,
            Next::Quit => {
                // SDL not quit: its audio may hang in a sound server gone
                // away, and the window's end must not wait for it (the
                // program's end takes everything).
                std::mem::forget(window.sdl);
                return Ok(());
            }
        };
    }
}

/// What the IDE says of a folder without a game.
fn no_game(dir: &Path) -> String {
    format!("NO GAME HERE · {}", dir.display())
}

/// What a folder chosen to open comes to: the game itself, the only game
/// below it (`note` says which, in OUTPUT of that game), the games found
/// below it to choose from, or the start page saying there is none.
fn choose(note: &mut Option<String>, dir: &Path) -> Next {
    match projects::opening(dir) {
        projects::Opening::Here => Next::Game(dir.to_path_buf()),
        projects::Opening::Only(game) => {
            *note = Some(format!("Opened {}, the only game in {}.", game.display(), dir.display()));
            Next::Game(game)
        }
        projects::Opening::Several(games) => Next::Found(dir.to_path_buf(), games),
        projects::Opening::Nothing => Next::Start(Some(no_game(dir))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::refused;
    use sdl3::event::Event;

    /// A folder chosen to open: a game opens as it is, the only game below
    /// one opens with a note saying which, several are listed, none is
    /// said to be none.
    #[test]
    fn a_folder_that_is_no_game_opens_what_lies_below() {
        let root = crate::scratch::Scratch::new("ide-choose");
        for path in ["here/conf.lua", "one/deep/game/conf.lua", "many/a/conf.lua", "many/b/conf.lua", "none/notes.txt"] {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "").unwrap();
        }
        let mut note = None;
        assert!(matches!(choose(&mut note, &root.join("here")), Next::Game(g) if g == root.join("here")));
        assert_eq!(note, None);
        let Next::Game(game) = choose(&mut note, &root.join("one")) else { panic!("not opened") };
        assert!(game.ends_with("one/deep/game"));
        assert!(note.as_deref().is_some_and(|n| n.contains("the only game in")), "{note:?}");
        assert!(matches!(choose(&mut note, &root.join("many")), Next::Found(_, games) if games.len() == 2));
        assert!(matches!(choose(&mut note, &root.join("none")), Next::Start(Some(m)) if m.starts_with("NO GAME HERE")));
    }

    /// `--play` on a game whose script Lua cannot read draws the IDE with
    /// the red line the window shows (THE GAME WOULD NOT START), not a
    /// mistake of the shot.
    #[test]
    fn a_shot_of_a_game_that_would_not_start_shows_the_red_line() {
        let dir = crate::scratch::Scratch::new("ide-shot-broken");
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\nend\n").unwrap();
        // The mistake in a module, so the open main.lua marks no line of its own.
        std::fs::write(dir.join("main.lua"), "local tools = require(\"tools\")\nfunction on_tick()\nend\n").unwrap();
        std::fs::write(dir.join("tools.lua"), "local x = = 1\nreturn {}\n").unwrap();
        let out = dir.join("shots/ide.png");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        let opts = ShotOptions { open: None, console: None, play: Some(10), upgraded: false, journal: None, find: None, replace: None, search: None, hit: None, width: 960, height: 540, out: out.clone() };
        shot::<crate::Voxcade>(Some(&dir), &opts).expect("drawn, not stopped");
        let (_, _, pixels) = crate::map::read_png_rgba(&out).unwrap();
        let red = crate::gui::ERROR;
        let near = |p: &[u8; 4]| (p[0] as i32 - red.r() as i32).abs() + (p[1] as i32 - red.g() as i32).abs() + (p[2] as i32 - red.b() as i32).abs() < 40;
        assert!(pixels.iter().filter(|p| near(p)).count() > 100, "the red line is drawn");
    }

    /// The console's tools are answered only by an IDE started with
    /// `--remote`; the other tools never wait on it.
    #[test]
    fn the_console_tools_need_remote() {
        for tool in ["console_send", "console_read", "windows", "window_shot", "pointer", "scroll", "keys"] {
            let why = refused(tool, false).expect("refused without --remote");
            assert!(why.contains("--remote"), "{why}");
            assert_eq!(refused(tool, true), None);
        }
        for tool in ["view_state", "open", "play", "eval", "errors"] {
            assert_eq!(refused(tool, false), None);
        }
    }

    /// An editor window's events go to it, the IDE's own and the
    /// program's (quit, the console's wake) to the IDE.
    #[test]
    fn an_editor_window_takes_only_its_own_events() {
        let key = |window_id| Event::KeyDown { timestamp: 0, window_id, keycode: None, scancode: None, keymod: sdl3::keyboard::Mod::empty(), repeat: false, which: 0, raw: 0 };
        let wake = Event::User { timestamp: 0, window_id: 0, type_: 0x8000, code: 0, data1: std::ptr::null_mut(), data2: std::ptr::null_mut() };
        assert_eq!(route(&key(7), 3), Route::Editor(7));
        assert_eq!(route(&key(3), 3), Route::Ide);
        assert_eq!(route(&Event::Quit { timestamp: 0 }, 3), Route::Ide);
        assert_eq!(route(&wake, 3), Route::Ide);
    }
}
