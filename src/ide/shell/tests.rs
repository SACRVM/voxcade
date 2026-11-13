//! The shell's tests: a frame of egui with the mouse and the keys of a
//! person, on games made in scratch folders.

use super::*;
use crate::ide::console::Start;

/// A frame of the shell without a window.
fn frame(ctx: &egui::Context, shell: &mut Shell) {
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), ..Default::default() };
    let _ = ctx.run_ui(raw, |ui| shell.ui(ui));
}

/// Another file chosen shows in the main area, a script after a
/// script too (the first is let go of, saved), and focus coming back
/// keeps a script being changed.
#[test]
fn choosing_a_file_shows_it() {
    let dir = Path::new("games/voxkeep");
    let scratch = crate::scratch::Scratch::new("shell-choose");
    let memory = scratch.join("memory.toml");
    let _ = std::fs::remove_file(&memory);
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    let text = |shell: &Shell| match &shell.shown {
        Some(Shown::Code(b)) => Some(b.code.text.clone()),
        _ => None,
    };
    assert_eq!(text(&shell), std::fs::read_to_string(dir.join("main.lua")).ok(), "main.lua at first");
    shell.open("conf.lua");
    frame(&ctx, &mut shell);
    assert_eq!(text(&shell), std::fs::read_to_string(dir.join("conf.lua")).ok(), "conf.lua chosen");
    if let Some(Shown::Code(b)) = &mut shell.shown {
        b.code.text.push_str("-- not saved yet\n");
    }
    shell.refresh();
    assert!(text(&shell).is_some_and(|t| t.ends_with("-- not saved yet\n")), "kept when focus comes back");
    // (Not saved: the change goes with the buffer.)
    shell.shown = None;
    shell.open("hero.vvx");
    frame(&ctx, &mut shell);
    assert!(matches!(shell.shown, Some(Shown::Note(_))) && shell.preview.is_some(), "a model: its picture");
    let _ = std::fs::remove_file(&memory);
    let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
}

/// A path clicked in the console: a file of the game opens at its line,
/// a picture from elsewhere shows as a picture and is not remembered as
/// open, any other file elsewhere opens nothing.
#[test]
fn a_clicked_path_opens_in_the_main_area() {
    let dir = Path::new("games/voxkeep");
    let scratch = crate::scratch::Scratch::new("shell-clicked");
    let memory = scratch.join("memory.toml");
    let _ = std::fs::remove_file(&memory);
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    let elsewhere = crate::scratch::Scratch::new("shell-clicked");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let png = elsewhere.join("gras.png");
    std::fs::copy("games/pulsar/ground/height.png", &png).unwrap();
    std::fs::write(elsewhere.join("notes.txt"), "x").unwrap();
    shell.open_from_console(&png, None);
    frame(&ctx, &mut shell);
    assert!(matches!(shell.shown, Some(Shown::Image(_))), "the picture from outside");
    assert_eq!(shell.opened(), Some("main.lua"), "the open file is still the game's");
    shell.save().unwrap();
    assert_eq!(Memory::load(&memory).open.as_deref(), Some("main.lua"), "only the game's files are remembered");
    assert!(shell.state(&ctx)["file"]["path"].as_str().is_some_and(|p| p.ends_with("gras.png")), "the agent sees it");
    shell.open_from_console(&elsewhere.join("notes.txt"), None);
    assert!(shell.outside.is_some(), "another file elsewhere changes nothing");
    // A file of the game: its text, at the line.
    let main = Path::new("games/voxkeep/main.lua").canonicalize().unwrap();
    shell.open_from_console(&main, Some(3));
    frame(&ctx, &mut shell);
    assert!(shell.outside.is_none() && matches!(shell.shown, Some(Shown::Code(_))), "main.lua again");
    assert_eq!(shell.goto, None, "the line was gone to");
    let _ = std::fs::remove_file(&memory);
    let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
    let _ = std::fs::remove_dir_all(&elsewhere);
}

/// A text opens with its cursor where it stood last time; the
/// agent's open leaves the keys where they are, Escape from the game
/// gives them to the text.
#[test]
fn the_cursor_comes_back_and_the_agent_takes_no_keys() {
    let dir = Path::new("games/voxkeep");
    let scratch = crate::scratch::Scratch::new("shell-cursor");
    let memory = scratch.join("memory.toml");
    let mut remembered = Memory { open: Some("main.lua".into()), ..Memory::default() };
    remembered.cursor.insert("main.lua".into(), 42);
    remembered.save(&memory).unwrap();
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
    let ctx = egui::Context::default();
    let id = |path: &str| egui::Id::unique(("code", path));
    let cursor = |ctx: &egui::Context, path: &str| egui::TextEdit::load_state(ctx, id(path)).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0);
    frame(&ctx, &mut shell);
    assert_eq!(cursor(&ctx, "main.lua"), Some(42), "where it stood");
    assert_eq!(ctx.memory(|m| m.focused()), None, "the keys nowhere yet");
    // A model chosen in the list (no text to take the keys), then the
    // agent's open: the wish for the keys did not wait for it.
    shell.open("hero.vvx");
    shell.back_to_file();
    frame(&ctx, &mut shell);
    shell.go_to("conf.lua", Some(2));
    frame(&ctx, &mut shell);
    assert!(cursor(&ctx, "conf.lua").is_some_and(|c| c > 0), "at the line");
    assert_eq!(ctx.memory(|m| m.focused()), None, "the agent's open takes no keys");
    shell.stage = Some(Stage { shown: true, ..Stage::default() });
    frame(&ctx, &mut shell);
    shell.back_to_file();
    frame(&ctx, &mut shell);
    assert_eq!(ctx.memory(|m| m.focused()), Some(id("conf.lua")), "back from the game: the keys to the text");
    shell.save().unwrap();
    assert!(Memory::load(&memory).cursor.get("conf.lua").is_some_and(|&c| c > 0), "remembered");
    let _ = std::fs::remove_file(&memory);
    let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
}

/// Another game opens at once while nothing runs in the console (the
/// text being changed saved first).
#[test]
fn another_game_opens_at_once_when_nothing_runs() {
    let dir = Path::new("games/voxkeep");
    let scratch = crate::scratch::Scratch::new("shell-leave");
    let memory = scratch.join("memory.toml");
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
    shell.leave_for(PathBuf::from("games/sokovox"));
    assert!(shell.leaving.is_none(), "nothing to ask");
    assert_eq!(shell.take_asks(), [Ask::Project(Wish::Open(PathBuf::from("games/sokovox")))]);
    let _ = std::fs::remove_file(&memory);
}

/// What the game says goes into OUTPUT: what stops it once, with its
/// line, what it printed; the agent's `errors` names the last errors.
#[test]
fn the_game_reports_into_the_output() {
    let dir = Path::new("games/voxkeep");
    let scratch = crate::scratch::Scratch::new("shell-output");
    let memory = scratch.join("memory.toml");
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
    let trouble = Some("main.lua:3: attempt to index a nil value\nstack traceback:".to_string());
    shell.game_said(trouble.clone(), vec![(Kind::Print, "hello".into())]);
    shell.game_said(trouble, Vec::new());
    let errors = shell.output.errors(5);
    assert_eq!(errors.len(), 1, "once");
    assert_eq!(errors[0].at, Some(("main.lua".into(), 3)));
    assert!(shell.output.unseen == 1 && shell.errors().contains("THE GAME STOPPED: main.lua:3"));
    let _ = std::fs::remove_file(&memory);
}

/// SHOW THE CONSOLE off leaves the console out altogether; on, it is
/// back (unfolded, as it was).
#[test]
fn the_console_can_be_left_out() {
    let dir = Path::new("games/voxkeep");
    let scratch = crate::scratch::Scratch::new("shell-console");
    let memory = scratch.join("memory.toml");
    let _ = std::fs::remove_file(&memory);
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
    let ctx = egui::Context::default();
    let drawn = |ctx: &egui::Context| ctx.viewport(|v| v.prev_pass.widgets.contains(egui::Id::unique("terminal")));
    frame(&ctx, &mut shell);
    assert!(!drawn(&ctx), "folded at first");
    shell.unfold_console();
    frame(&ctx, &mut shell);
    assert!(drawn(&ctx), "there once unfolded");
    shell.prefs.console = false;
    frame(&ctx, &mut shell);
    assert!(!drawn(&ctx), "left out");
    shell.prefs.console = true;
    frame(&ctx, &mut shell);
    assert!(drawn(&ctx), "back");
    let _ = std::fs::remove_file(&memory);
    let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
}

/// The line the game stopped at is marked in its file, not in
/// another, and goes with the error.
#[test]
fn the_line_the_game_stopped_at_is_marked() {
    let dir = Path::new("games/voxkeep");
    let scratch = crate::scratch::Scratch::new("shell-stopped");
    let memory = scratch.join("memory.toml");
    let _ = std::fs::remove_file(&memory);
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
    let ctx = egui::Context::default();
    shell.stage = Some(Stage { trouble: Some("main.lua:3: boom\nstack traceback:".into()), ..Stage::default() });
    let stopped = |shell: &Shell| match &shell.shown {
        Some(Shown::Code(b)) => b.code.stopped,
        _ => None,
    };
    shell.open("main.lua");
    frame(&ctx, &mut shell);
    assert_eq!(stopped(&shell), Some(3));
    shell.open("conf.lua");
    frame(&ctx, &mut shell);
    assert_eq!(stopped(&shell), None, "not in another file");
    shell.open("main.lua");
    frame(&ctx, &mut shell);
    shell.stage.as_mut().unwrap().trouble = None;
    frame(&ctx, &mut shell);
    assert_eq!(stopped(&shell), None, "mended");
    let _ = std::fs::remove_file(&memory);
    let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
}

/// A song says what it is and plays with PLAY; opening another file
/// or playing the game ends it.
#[test]
fn a_song_plays_until_something_else_is_open() {
    let dir = Path::new("games/voxkeep");
    let scratch = crate::scratch::Scratch::new("shell-song");
    let memory = scratch.join("memory.toml");
    let _ = std::fs::remove_file(&memory);
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.clone(), console);
    let ctx = egui::Context::default();
    let song = "music/under_the_dunes.mml";
    shell.open(song);
    frame(&ctx, &mut shell);
    assert!(shell.facts.as_deref().is_some_and(|f| f.contains("channels")), "{:?}", shell.facts);
    assert!(shell.song(song).is_ok(), "it reads");
    shell.listen();
    assert_eq!(shell.take_asks(), [Ask::Listen(song.into())]);
    shell.open("main.lua");
    assert_eq!(shell.take_asks(), [Ask::Hush], "another file ends it");
    shell.listen();
    assert!(shell.take_asks().is_empty() && shell.listening.is_none(), "a script does not play");
    shell.open(song);
    shell.listen();
    shell.play();
    assert_eq!(shell.take_asks(), [Ask::Listen(song.into()), Ask::Hush, Ask::Play(None)], "the game ends it");
    let _ = std::fs::remove_file(&memory);
    let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
}

/// A bare game folder to try the plus signs on: no model, no song, no
/// scenario.
fn bare_game(name: &str) -> (crate::scratch::Scratch, PathBuf) {
    let dir = crate::scratch::Scratch::new(&format!("shell-{name}"));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\" }\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_tick() end\n").unwrap();
    let memory = dir.join("memory.toml");
    (dir, memory)
}

fn shell_on(dir: &Path, memory: &Path) -> Shell {
    let console = Console::new(dir.to_path_buf(), Start::Nothing, std::sync::Arc::new(|| {}));
    Shell::new(dir, crate::session::config::<crate::Voxcade>(dir).unwrap(), memory.to_path_buf(), console)
}

/// A frame with `events`, a tenth of a second later than the one before (so that
/// folding finishes).
fn frame_with(ctx: &egui::Context, shell: &mut Shell, events: Vec<egui::Event>) {
    let time = ctx.input(|i| i.time) + 0.1;
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), time: Some(time), events, ..Default::default() };
    let _ = ctx.run_ui(raw, |ui| shell.ui(ui));
}

/// A click of the left button at `at`, with a frame before it (the
/// mouse arrives) and after.
fn click(ctx: &egui::Context, shell: &mut Shell, at: egui::Pos2) {
    let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(ctx, shell, vec![egui::Event::PointerMoved(at)]);
    frame_with(ctx, shell, vec![button(true)]);
    frame_with(ctx, shell, vec![button(false)]);
    for _ in 0..4 {
        frame_with(ctx, shell, vec![]);
    }
}

fn click_plus(ctx: &egui::Context, shell: &mut Shell, sort: &str) {
    let at = heading(shell, sort).0.center();
    click(ctx, shell, at);
}

fn key(ctx: &egui::Context, shell: &mut Shell, key: egui::Key) {
    let event = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
    frame_with(ctx, shell, vec![event(true)]);
    frame_with(ctx, shell, vec![event(false)]);
}

/// The plus at a heading and the heading's row, as drawn last.
fn heading(shell: &Shell, sort: &str) -> (egui::Rect, egui::Rect) {
    shell.headings.iter().find(|(s, ..)| s == sort).map(|(_, p, h)| (*p, *h)).unwrap_or_else(|| panic!("no heading {sort}: {:?}", shell.headings.iter().map(|h| &h.0).collect::<Vec<_>>()))
}

/// A name typed under a heading, with the plus clicked and Enter hit.
fn type_a_name(ctx: &egui::Context, shell: &mut Shell, sort: &str, name: &str) {
    click_plus(ctx, shell, sort);
    assert!(shell.naming.as_ref().is_some_and(|n| n.sort == sort), "the plus asks for a name under {sort}");
    frame_with(ctx, shell, vec![egui::Event::Text(name.into())]);
    key(ctx, shell, egui::Key::Enter);
}

/// A game without models, sounds, songs or scenarios still has the headings
/// LOGIC, SCENARIOS, LEVELS, MODELS, SOUNDS and MUSIC, each with a plus;
/// BLOCKS and the like only when there are some.
#[test]
fn the_headings_with_a_plus_are_always_there() {
    let (dir, memory) = bare_game("headings");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    let sorts: Vec<&str> = shell.headings.iter().map(|(s, ..)| s.as_str()).collect();
    assert_eq!(sorts, ["Logic", "Scenarios", "Levels", "Models", "Sounds", "Music"]);
    assert_eq!(shell.files.assets.iter().map(|g| (g.name, g.files.len())).collect::<Vec<_>>(), [("Levels", 0), ("Models", 0), ("Sounds", 0), ("Music", 0)]);
    for (sort, plus, head) in &shell.headings {
        assert!(head.contains_rect(*plus), "{sort}: the plus lies on its row");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The plus does not fold its heading and the rest of the row does;
/// a plus on a folded heading unfolds it.
#[test]
fn the_plus_and_the_fold_are_apart() {
    let (dir, memory) = bare_game("fold");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    let below = |shell: &Shell| heading(shell, "Music").1.top();
    let open = below(&shell);
    let (plus, row) = heading(&shell, "Models");
    click(&ctx, &mut shell, egui::pos2(row.left() + 40.0, row.center().y));
    let folded = below(&shell);
    assert!(folded < open, "the row folds it: {folded} < {open}");
    click_plus(&ctx, &mut shell, "Models");
    assert!(shell.naming.as_ref().is_some_and(|n| n.sort == "Models"), "the plus asks for a name");
    assert!(below(&shell) > folded, "and unfolds the heading to make room for it");
    // On an open heading it folds nothing.
    let before = below(&shell);
    click_plus(&ctx, &mut shell, "Models");
    assert!(below(&shell) >= before, "an open heading stays open");
    let _ = plus;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Each plus makes its file under the name typed, it is listed at once and open; a name taken is
/// refused, with the old file left; Escape lets go of the field.
#[test]
fn a_plus_makes_a_file_of_its_sort() {
    let (dir, memory) = bare_game("make");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    for (sort, typed, path) in [("Logic", "Market Stall", "market_stall.lua"), ("Scenarios", "late", "scenarios/late.lua"), ("Models", "Hero", "hero.vvx"), ("Sounds", "Coin Pick", "sounds/coin_pick.toml"), ("Music", "theme.mml", "music/theme.mml"), ("Levels", "Cellar", "levels/cellar.toml")] {
        type_a_name(&ctx, &mut shell, sort, typed);
        assert!(dir.join(path).is_file(), "{path} made");
        assert!(shell.naming.is_none(), "{path}: the field is gone");
        assert!(shell.files.has(path), "{path} listed at once");
        assert_eq!(shell.memory.open.as_deref(), Some(path), "{path} open");
        frame(&ctx, &mut shell);
    }
    assert_eq!(std::fs::read_to_string(dir.join("scenarios/late.lua")).unwrap(), NEW_SCENARIO);
    // A new level is a room of pieces as big as the game's view (no
    // blocks here to ask about), and opens in the level editor.
    let room = crate::grid::GridLevel::read(&dir.join("levels/cellar.toml")).unwrap();
    assert!(!room.has_grid() && room.voxels().x >= 10, "{room:?}");
    assert!(shell.take_edits().contains(&"levels/cellar.toml".to_string()));
    assert!(shell.is_level("levels/cellar.toml"), "a level of pieces is a level in a game of worlds too");
    // The sound reads, says what it is and how to play it.
    let blip = std::fs::read_to_string(dir.join("sounds/coin_pick.toml")).unwrap();
    assert!(blip.starts_with("# coin_pick: ") && blip.contains("level:sound(\"coin_pick\", player)"), "{blip}");
    assert!(matches!(sound::read("sounds/coin_pick.toml", &blip), Ok(SoundDef::Synth(_))));
    let lua = std::fs::read_to_string(dir.join("market_stall.lua")).unwrap();
    assert!(lua.contains("local market_stall = require(\"market_stall\")") && lua.contains("return market_stall"), "{lua}");
    // A name taken is refused, the file stays.
    std::fs::write(dir.join("hero.vvx"), "mine").unwrap();
    type_a_name(&ctx, &mut shell, "Models", "HERO");
    assert_eq!(std::fs::read(dir.join("hero.vvx")).unwrap(), b"mine");
    assert_eq!(shell.naming.as_ref().and_then(|n| n.problem.as_deref()), Some("hero.vvx is already there."));
    frame(&ctx, &mut shell);
    key(&ctx, &mut shell, egui::Key::Escape);
    assert!(shell.naming.is_none(), "Escape cancels");
    // Nothing, or a sign a name cannot have.
    assert_eq!(shell.make_file("Logic", "  "), Err("Type a name first.".into()));
    assert_eq!(shell.make_file("Logic", "a/b"), Err("A name takes letters, digits, _ and - only.".into()));
    assert_eq!(shell.make_file("Logic", "main"), Err("main.lua is already there.".into()));
    assert_eq!(std::fs::read_to_string(dir.join("main.lua")).unwrap(), "function on_tick() end\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// An empty field loses focus: it goes away.
#[test]
fn an_empty_name_field_goes_away_when_left() {
    let (dir, memory) = bare_game("leave");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    click_plus(&ctx, &mut shell, "Music");
    assert!(shell.naming.is_some());
    click(&ctx, &mut shell, egui::pos2(700.0, 400.0));
    assert!(shell.naming.is_none(), "left empty");
    let _ = std::fs::remove_dir_all(&dir);
}

/// What the plus makes works: the model loads and draws, the song
/// parses, the Lua file can be required by the game's main.lua.
#[test]
fn what_a_plus_makes_works() {
    let (dir, memory) = bare_game("works");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    for (sort, name) in [("Logic", "tools"), ("Models", "blob"), ("Music", "tune")] {
        shell.make_file(sort, name).unwrap();
    }
    let model = crate::vox::load_model(&dir.join("blob.vvx")).unwrap();
    assert!(model.frames.len() == 1 && model.frames[0].len() == 64 && model.size == glam::IVec3::splat(8), "{:?} {}", model.size, model.frames[0].len());
    shell.open("blob.vvx");
    frame(&ctx, &mut shell);
    assert!(shell.preview.is_some(), "it shows as a picture: {:?}", shell.message);
    let song = crate::audio::music::parse(&std::fs::read_to_string(dir.join("music/tune.mml")).unwrap()).unwrap();
    assert!(song.names.len() == 2 && song.length > 0 && song.channel_lengths().iter().all(|(_, l)| (*l - song.channel_lengths()[0].1).abs() < 0.01), "{:?}", song.channel_lengths());
    std::fs::write(dir.join("main.lua"), "local tools = require(\"tools\")\nfunction on_world_start() world.state.said = tools.hello() end\n").unwrap();
    use crate::game::Game;
    let mut game = crate::session::Session::<crate::Voxcade>::load(&dir).unwrap();
    game.step(&crate::input::Input::default());
    assert!(game.game.halted().is_none(), "{:?}", game.game.halted());
    assert!(game.game.inspect().to_string().contains("hello from tools"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A game with sounds of its own: `conf.lua` names "coin" in a kind,
/// `main.lua` plays it, `bell` is never played.
fn sound_game(name: &str) -> (crate::scratch::Scratch, PathBuf) {
    let (dir, memory) = bare_game(name);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\" }\n  c.things.gem = { sound = \"coin\" }\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  if world.tick == 5 then\n    level:sound(\"coin\", player)\n  end\nend\n").unwrap();
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/coin.toml"), "# a coin\nwave = \"triangle\"\nfreq = 880\nhold = 0.04\ndecay = 0.16\nsweep = -12\n").unwrap();
    std::fs::write(dir.join("sounds/bell.toml"), "wave = \"sine\"\n").unwrap();
    (dir, memory)
}

/// The text open takes `text` typed at its end, as a person types.
fn type_into_text(ctx: &egui::Context, shell: &mut Shell, text: &str) {
    shell.back_to_file();
    frame_with(ctx, shell, vec![]);
    frame_with(ctx, shell, vec![]);
    frame_with(ctx, shell, vec![egui::Event::Text(text.into())]);
    frame_with(ctx, shell, vec![]);
}

fn buffer(shell: &Shell) -> &Buffer {
    match &shell.shown {
        Some(Shown::Code(b)) => b,
        _ => panic!("no text open"),
    }
}

/// SOUNDS is always listed, the game's sounds in it and none under
/// OTHER; the sounds the game brings but has no file for follow, built
/// in, and a file of the same name takes one off the list.
#[test]
fn sounds_have_a_group_of_their_own() {
    let (dir, memory) = sound_game("group");
    let mut shell = shell_on(&dir, &memory).with_sounds(crate::audio::core_sounds());
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    let groups: Vec<(&str, Vec<&str>)> = shell.files.assets.iter().map(|g| (g.name, g.files.iter().map(String::as_str).collect())).collect();
    assert!(groups.contains(&("Sounds", vec!["sounds/bell.toml", "sounds/coin.toml"])), "{groups:?}");
    assert!(groups.iter().all(|(n, f)| *n == "Sounds" || !f.iter().any(|f| f.starts_with("sounds/"))), "none under OTHER: {groups:?}");
    // The built-in ones: those of the core, none of them replaced by a file.
    std::fs::write(dir.join("sounds/impact.toml"), "wave = \"noise\"\n").unwrap();
    shell.files = Files::scan(&dir);
    let listed: Vec<&str> = shell.brought.iter().map(|(n, _)| *n).filter(|n| !shell.files.has(&format!("sounds/{n}.toml"))).collect();
    assert_eq!(listed, ["explosion", "crumble"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A built-in sound opens read-only in the main area with its text;
/// MAKE IT YOURS writes it as the game's own file, which then shows
/// as one of the game's.
#[test]
fn a_builtin_sound_is_made_yours() {
    let (dir, memory) = sound_game("yours");
    let mut shell = shell_on(&dir, &memory).with_sounds(crate::audio::core_sounds());
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.builtin = Some("explosion");
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(shell.used.as_ref().is_some_and(|u| u.name == "explosion" && u.rows.is_empty()));
    assert!(shell.rules_say("explosion")[0].starts_with("THE ENGINE"));
    assert!(!dir.join("sounds/explosion.toml").exists(), "looking writes nothing");
    shell.make_it_yours("explosion");
    let text = std::fs::read_to_string(dir.join("sounds/explosion.toml")).unwrap();
    assert!(text.starts_with("# explosion: ") && sound::read("explosion.toml", &text).is_ok(), "{text}");
    assert!(shell.builtin.is_none() && shell.files.has("sounds/explosion.toml") && shell.memory.open.as_deref() == Some("sounds/explosion.toml"));
    frame(&ctx, &mut shell);
    assert!(matches!(shell.shown, Some(Shown::Code(_))), "now it is text to change");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A sound says what it is in words; a mistake typed into it shows its
/// line and marks it, mended it clears.
#[test]
fn a_sound_is_text_with_a_line_of_words_and_its_mistakes() {
    let (dir, memory) = sound_game("text");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("sounds/coin.toml");
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert_eq!(shell.sound_line.as_ref().map(|(_, l)| l.as_str()), Some("0.20 s · triangle wave · falling an octave"));
    // A mistake on a line of its own, typed at the end.
    type_into_text(&ctx, &mut shell, "frq = 1\n");
    let error = buffer(&shell).code.error.clone();
    assert!(error.as_ref().is_some_and(|(line, why)| *line == 7 && why.starts_with("frq: no such field")), "{error:?}");
    assert_eq!(shell.sound_line.as_ref().map(|(_, l)| l.as_str()), Some("The sound cannot be read."));
    assert!(shell.errors().starts_with("sounds/coin.toml:7: frq: no such field"), "{}", shell.errors());
    // Mended: the line gone.
    if let Some(Shown::Code(b)) = &mut shell.shown {
        b.code.text = b.code.text.replace("frq = 1\n", "");
    }
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(buffer(&shell).code.error.is_none());
    assert!(shell.sound_line.as_ref().is_some_and(|(_, l)| l.starts_with("0.20 s")));
    let _ = std::fs::remove_dir_all(&dir);
}

/// USED lists the lines of the game's scripts that name the sound, in
/// quotes, and a click opens the file there; a sound nothing names
/// says so; PLAY SOUND renders the text as it stands.
#[test]
fn a_sound_shows_where_it_is_used() {
    let (dir, memory) = sound_game("used");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("sounds/coin.toml");
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    let rows: Vec<(String, usize, String)> = shell.used.as_ref().unwrap().rows.iter().map(|u| (u.file.clone(), u.line, u.text.clone())).collect();
    assert_eq!(rows, [("conf.lua".to_string(), 5, "c.things.gem = { sound = \"coin\" }".to_string()), ("main.lua".to_string(), 3, "level:sound(\"coin\", player)".to_string())]);
    assert_eq!(shell.used_rows.len(), 2, "two rows drawn");
    // A click on the second opens main.lua at its line.
    let second = shell.used_rows[1].center();
    click(&ctx, &mut shell, second);
    assert_eq!(shell.memory.open.as_deref(), Some("main.lua"));
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    let cursor = egui::TextEdit::load_state(&ctx, egui::Id::unique(("code", "main.lua"))).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0);
    let text = &buffer(&shell).code.text;
    let line = text[..text.char_indices().nth(cursor.unwrap_or(0)).map_or(text.len(), |(b, _)| b)].matches('\n').count() + 1;
    assert_eq!(line, 3, "its cursor on the line");
    // Nothing names the bell.
    shell.open("sounds/bell.toml");
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(shell.used.as_ref().is_some_and(|u| u.name == "bell" && u.rows.is_empty()) && shell.used_rows.is_empty());
    // PLAY SOUND asks the speaker for the text as it stands.
    type_into_text(&ctx, &mut shell, "# louder\n");
    shell.play_sound();
    assert_eq!(shell.take_asks(), [Ask::Sound]);
    let (samples, volume) = shell.heard().expect("rendered");
    assert!(!samples.is_empty() && volume > 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A sound of numbers shows one PLAY for it (the maker's, beside LOOP),
/// no PLAY SOUND over it; at 1024 by 600 the faders lie in two rows, none
/// cut at the edge, and UNDO stays beside PLAY and LOOP; wide, one row.
#[test]
fn the_sound_page_has_one_play_and_its_faders_fit() {
    let (dir, memory) = sound_game("one-play");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("sounds/coin.toml");
    for (width, height, rows) in [(1024.0, 600.0, 2), (1920.0, 1080.0, 1)] {
        let size = egui::vec2(width, height);
        drawn_rects(&ctx, &mut shell, size);
        let texts = drawn_rects(&ctx, &mut shell, size);
        let main: Vec<_> = texts.iter().filter(|(_, r)| r.min.x > 250.0 && r.min.y > 52.0).collect();
        assert!(!main.iter().any(|(t, _)| t == "PLAY SOUND"), "{width}: no PLAY SOUND over the maker");
        assert_eq!(main.iter().filter(|(t, _)| t == "PLAY").count(), 1, "{width}: one PLAY");
        let faders: Vec<egui::Rect> = ["PITCH", "LENGTH", "SLIDE", "BRIGHT", "PUNCH"].iter().map(|w| main.iter().find(|(t, _)| t == w).unwrap_or_else(|| panic!("{width}: no {w}")).1).collect();
        assert!(faders.iter().all(|r| r.max.x + 150.0 - r.width() <= width), "{width}: a fader cut: {faders:?}");
        let mut tops: Vec<i32> = faders.iter().map(|r| r.min.y.round() as i32).collect();
        tops.dedup();
        assert_eq!(tops.len(), rows, "{width}: the faders' rows");
        let at = |w: &str| main.iter().find(|(t, _)| t == w).unwrap().1;
        assert!((at("UNDO").center().y - at("PLAY").center().y).abs() < 2.0 && (at("LOOP").center().y - at("PLAY").center().y).abs() < 2.0, "{width}: PLAY, LOOP and UNDO on one row");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The sound maker: a sort's button replaces the sound with a fresh one
/// of its sort (the leading comment kept) and plays it; UNDO goes back
/// over the rolls one by one, and Ctrl+Z in the text over the same
/// steps; what the maker made is saved as typing is.
#[test]
fn the_maker_rolls_and_goes_back() {
    let (dir, memory) = sound_game("maker");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("sounds/coin.toml");
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    let text = |shell: &Shell| buffer(shell).code.text.clone();
    let first = text(&shell);
    shell.roll(&ctx, maker::Sort::Blast);
    let blast = text(&shell);
    assert!(blast.starts_with("# a coin\nwave = \"noise\"\n"), "{blast}");
    assert!(matches!(buffer(&shell).code.sound, Some(SoundDef::Synth(_))) && buffer(&shell).code.error.is_none());
    assert_eq!(shell.take_asks(), [Ask::Sound], "played at once");
    assert!(shell.heard().is_some());
    assert_eq!(shell.saved_word(), Some("NOT SAVED YET"));
    frame(&ctx, &mut shell);
    shell.roll(&ctx, maker::Sort::Coin);
    frame(&ctx, &mut shell);
    assert!(text(&shell) != blast && text(&shell).starts_with("# a coin\n"));
    // UNDO: the blast, then the sound as it was.
    shell.go_back(&ctx);
    assert_eq!(text(&shell), blast);
    frame(&ctx, &mut shell);
    shell.go_back(&ctx);
    assert_eq!(text(&shell), first);
    assert_eq!(shell.take_asks(), [Ask::Sound, Ask::Sound, Ask::Sound]);
    // LOOP: the sound again once it has sounded and its pause is over,
    // until another file is opened.
    (shell.looping, shell.again_at) = (true, None);
    let now = std::time::Instant::now();
    shell.tick(now);
    assert_eq!(shell.take_asks(), [Ask::Sound], "at once");
    shell.tick(now);
    assert_eq!(shell.take_asks(), [], "not before it has sounded");
    let (_, next) = shell.tick(now);
    assert!(next <= std::time::Duration::from_secs(3), "the window wakes for it: {next:?}");
    shell.tick(now + std::time::Duration::from_secs(5));
    assert_eq!(shell.take_asks(), [Ask::Sound], "again");
    shell.hush();
    shell.tick(now + std::time::Duration::from_secs(20));
    assert_eq!(shell.take_asks(), [], "switched off");
    // Ctrl+Z in the text goes back over a roll too.
    shell.roll(&ctx, maker::Sort::Jump);
    assert!(text(&shell).contains("sweep = "));
    shell.back_to_file();
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    let ctrl_z = |pressed| egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND };
    frame_with(&ctx, &mut shell, vec![ctrl_z(true)]);
    frame_with(&ctx, &mut shell, vec![ctrl_z(false)]);
    assert_eq!(text(&shell), first, "Ctrl+Z");
    // Saved as typing is.
    shell.roll(&ctx, maker::Sort::Hit);
    shell.tick(std::time::Instant::now() + super::super::buffer::SAVE_AFTER);
    assert_eq!(std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap(), text(&shell));
    assert!(sound::read("sounds/coin.toml", &text(&shell)).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A song's text is changed and saved; a mistake in it shows its line
/// and PLAY SONG takes the text as it stands.
#[test]
fn a_song_is_text_to_change() {
    let (dir, memory) = bare_game("songtext");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.make_file("Music", "tune").unwrap();
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(matches!(shell.shown, Some(Shown::Code(_))), "a song is text to change");
    type_into_text(&ctx, &mut shell, "\n#bogus 1\n");
    let error = buffer(&shell).code.error.clone();
    assert!(error.as_ref().is_some_and(|(_, why)| why.contains("unknown #bogus")), "{error:?}");
    let line = error.unwrap().0;
    assert_eq!(buffer(&shell).code.text.lines().nth(line - 1), Some("#bogus 1"), "the line is the one with the mistake");
    // Saved by the autosave, and PLAY SONG reads the text as it stands.
    assert!(shell.song("music/tune.mml").is_err(), "the text as it stands: it cannot be read now");
    shell.tick(std::time::Instant::now() + super::super::buffer::SAVE_AFTER);
    assert!(std::fs::read_to_string(dir.join("music/tune.mml")).unwrap().contains("#bogus 1"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// SAVED while the file holds what is shown, NOT SAVED YET between a
/// change and the autosave; Ctrl+S saves at once.
#[test]
fn the_text_says_whether_it_is_saved() {
    let (dir, memory) = sound_game("saved");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("sounds/coin.toml");
    frame(&ctx, &mut shell);
    assert_eq!(shell.saved_word(), Some("SAVED"));
    type_into_text(&ctx, &mut shell, "# one\n");
    assert_eq!(shell.saved_word(), Some("NOT SAVED YET"));
    assert!(!std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap().contains("# one"));
    shell.tick(std::time::Instant::now() + super::super::buffer::SAVE_AFTER);
    assert_eq!(shell.saved_word(), Some("SAVED"), "the autosave");
    assert!(std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap().contains("# one"));
    // Ctrl+S at once.
    type_into_text(&ctx, &mut shell, "# two\n");
    assert_eq!(shell.saved_word(), Some("NOT SAVED YET"));
    let ctrl_s = |pressed| egui::Event::Key { key: egui::Key::S, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND };
    frame_with(&ctx, &mut shell, vec![ctrl_s(true)]);
    frame_with(&ctx, &mut shell, vec![ctrl_s(false)]);
    assert_eq!(shell.saved_word(), Some("SAVED"));
    assert!(std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap().contains("# two"), "written at once");
    // A script says it too.
    shell.open("main.lua");
    frame(&ctx, &mut shell);
    assert_eq!(shell.saved_word(), Some("SAVED"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A game of every sort of file to rename: a module main.lua requires, a
/// scenario, a model, a song, an image and a text, and sounds as
/// `sound_game` has them. The folder a deleted file goes to is in the
/// temp dir too.
fn managed_game(name: &str) -> (crate::scratch::Scratch, PathBuf, Shell) {
    let (dir, memory) = sound_game(name);
    std::fs::write(dir.join("tools.lua"), "return {}\n").unwrap();
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    std::fs::write(dir.join("scenarios/late.lua"), NEW_SCENARIO).unwrap();
    crate::editor::frames::ModelFile::begun(&dir.join("hero.vvx")).save().unwrap();
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/tune.mml"), NEW_SONG.replace("NAME", "tune")).unwrap();
    std::fs::write(dir.join("back.png"), "not a picture").unwrap();
    std::fs::write(dir.join("notes.txt"), "remember").unwrap();
    let mut shell = shell_on(&dir, &memory);
    shell.deleted_dir = dir.with_extension("deleted");
    let _ = std::fs::remove_dir_all(&shell.deleted_dir);
    (dir, memory, shell)
}

fn clean_up(dir: &Path, shell: &Shell) {
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(&shell.deleted_dir);
    let _ = std::fs::remove_dir_all(shell.memory_path.with_extension("journal"));
}

/// Every sort of file is renamed under the name typed, its ending kept,
/// the list read anew; where nothing names it nothing is asked.
#[test]
fn each_sort_of_file_is_renamed() {
    let (dir, _, mut shell) = managed_game("rename-sorts");
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    for (from, typed, to) in [
        ("tools.lua", "Kit", "kit.lua"),
        ("scenarios/late.lua", "soon", "scenarios/soon.lua"),
        ("hero.vvx", "knight.vvx", "knight.vvx"),
        ("sounds/bell.toml", "chime", "sounds/chime.toml"),
        ("music/tune.mml", "song", "music/song.mml"),
        ("back.png", "sky", "sky.png"),
        ("notes.txt", "Read Me", "Read_Me.txt"),
    ] {
        let bytes = std::fs::read(dir.join(from)).unwrap();
        assert_eq!(shell.rename(from, typed), Ok(()), "{from}");
        assert!(shell.asking.is_none(), "{from}: nothing names it, nothing is asked");
        assert!(!dir.join(from).exists(), "{from} gone");
        assert_eq!(std::fs::read(dir.join(to)).unwrap(), bytes, "{to} has its bytes");
        assert!(shell.files.has(to) && !shell.files.has(from), "{to} listed anew");
        frame(&ctx, &mut shell);
    }
    let _ = std::fs::remove_dir_all(&dir);
    clean_up(&dir, &shell);
}

/// The open file stays open under its new name with its cursor and its
/// scroll; a text not saved yet is saved first, and no journal is left
/// under the old name.
#[test]
fn the_open_file_stays_open_when_renamed() {
    let (dir, _, mut shell) = managed_game("rename-open");
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("tools.lua");
    frame(&ctx, &mut shell);
    type_into_text(&ctx, &mut shell, "-- typed\n");
    assert!(buffer(&shell).unsaved());
    let journal = shell.journals().join("tools.lua");
    assert!(journal.exists(), "the text is in the journal");
    shell.memory.cursor.insert("tools.lua".into(), 4);
    shell.memory.scroll.insert("tools.lua".into(), 30.0);
    assert_eq!(shell.rename("tools.lua", "kit"), Ok(()));
    assert!(!journal.exists() && !shell.journals().join("kit.lua").exists(), "no journal left behind");
    assert!(std::fs::read_to_string(dir.join("kit.lua")).unwrap().ends_with("-- typed\n"), "saved first");
    assert_eq!(shell.memory.open.as_deref(), Some("kit.lua"));
    assert_eq!((shell.memory.cursor.get("kit.lua"), shell.memory.scroll.get("kit.lua")), (Some(&4), Some(&30.0)));
    assert!(!shell.memory.cursor.contains_key("tools.lua") && !shell.memory.scroll.contains_key("tools.lua"));
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(buffer(&shell).code.text.ends_with("-- typed\n") && !buffer(&shell).unsaved(), "shown under the new name");
    // The IDE's memory on disk says so too.
    assert_eq!(Memory::load(&shell.memory_path).open.as_deref(), Some("kit.lua"));
    clean_up(&dir, &shell);
}

/// main.lua and conf.lua keep their names and stay; a name taken is
/// refused, the files left; levels and blocks are not yet.
#[test]
fn what_the_game_needs_is_not_renamed_or_deleted() {
    let (dir, _, mut shell) = managed_game("rename-refused");
    std::fs::write(dir.join("level.toml"), "{}").unwrap();
    shell.files = Files::scan(&dir);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    for file in ["main.lua", "conf.lua"] {
        let err = shell.rename(file, "other").unwrap_err();
        assert!(err.contains("needs main.lua and conf.lua"), "{err}");
        shell.delete(file);
        assert!(dir.join(file).is_file() && shell.gone.is_none(), "{file} stays");
        shell.begin_rename(file);
        assert!(shell.renaming.is_none());
    }
    assert!(shell.rename("level.toml", "other").unwrap_err().starts_with("Not yet"));
    shell.delete("level.toml");
    assert!(dir.join("level.toml").is_file() && shell.gone.is_none());
    assert_eq!(shell.rename("sounds/coin.toml", "bell"), Err("sounds/bell.toml is already there.".into()));
    assert_eq!(shell.rename("tools.lua", "a/b"), Err("A name cannot hold a folder: no / in it.".into()));
    assert_eq!(shell.rename("tools.lua", " "), Err("Type a name first.".into()));
    assert!(dir.join("sounds/coin.toml").is_file() && dir.join("tools.lua").is_file());
    // The disabled buttons say why (the open file is main.lua).
    frame(&ctx, &mut shell);
    clean_up(&dir, &shell);
}

/// The lines naming a sound are asked about: CANCEL changes nothing,
/// ONLY THE FILE renames the file alone, CHANGE THEM changes the
/// lines too, also in a text open that has changes of its own.
#[test]
fn the_lines_naming_a_sound_are_asked_about() {
    use manage::Answer;
    let (dir, _, mut shell) = managed_game("rename-asks");
    let (conf, main) = (std::fs::read_to_string(dir.join("conf.lua")).unwrap(), std::fs::read_to_string(dir.join("main.lua")).unwrap());
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    assert_eq!(shell.rename("sounds/coin.toml", "ring"), Ok(()));
    let q = shell.asking.as_ref().expect("asked");
    assert_eq!((q.rows.len(), q.ticked.as_slice()), (2, [true, true].as_slice()), "both certain, both ticked");
    frame(&ctx, &mut shell);
    assert!(dir.join("sounds/coin.toml").exists(), "nothing done before the answer");
    shell.answer(Answer::Cancel);
    assert!(shell.asking.is_none() && dir.join("sounds/coin.toml").exists() && !dir.join("sounds/ring.toml").exists());
    // ONLY THE FILE.
    assert_eq!(shell.rename("sounds/coin.toml", "ring"), Ok(()));
    shell.answer(Answer::OnlyTheFile);
    assert!(dir.join("sounds/ring.toml").is_file() && !dir.join("sounds/coin.toml").exists());
    assert_eq!((std::fs::read_to_string(dir.join("conf.lua")).unwrap(), std::fs::read_to_string(dir.join("main.lua")).unwrap()), (conf.clone(), main.clone()), "the lines as they were");
    // CHANGE THEM, main.lua open with a change not saved yet.
    shell.rename("sounds/ring.toml", "coin").unwrap();
    shell.answer(Answer::OnlyTheFile);
    shell.open("main.lua");
    frame(&ctx, &mut shell);
    type_into_text(&ctx, &mut shell, "-- mine\n");
    assert_eq!(shell.rename("sounds/coin.toml", "ring"), Ok(()));
    shell.answer(Answer::Change);
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    shell.tick(std::time::Instant::now() + super::super::buffer::SAVE_AFTER);
    assert_eq!(std::fs::read_to_string(dir.join("conf.lua")).unwrap(), conf.replace("\"coin\"", "\"ring\""));
    let main_now = std::fs::read_to_string(dir.join("main.lua")).unwrap();
    assert_eq!(main_now, format!("{}-- mine\n", main.replace("\"coin\"", "\"ring\"")), "the change and the new name both, on disk");
    assert_eq!(buffer(&shell).code.text, main_now, "the open text took it in");
    assert!(matches!(&shell.notice, Some(Notice::Said(n)) if n.starts_with("RENAMED coin.toml TO ring.toml")));
    // A module, a model and a song are asked about by their own lines.
    std::fs::write(dir.join("main.lua"), "local tools = require \"tools\"\nm = \"hero.vvx\"\nplay(\"music/tune.mml\")\n").unwrap();
    shell.files = Files::scan(&dir);
    shell.unload();
    for (from, typed, line) in [("tools.lua", "kit", "require \"kit\""), ("hero.vvx", "knight", "m = \"knight.vvx\""), ("music/tune.mml", "song", "play(\"music/song.mml\")")] {
        assert_eq!(shell.rename(from, typed), Ok(()));
        assert_eq!(shell.asking.as_ref().map(|q| q.rows.len()), Some(1), "{from}");
        shell.answer(Answer::Change);
        assert!(std::fs::read_to_string(dir.join("main.lua")).unwrap().contains(line), "{line}");
    }
    clean_up(&dir, &shell);
}

/// The game folder `from` copied into `to`, but its recorded runs.
fn copy_game(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let (path, name) = (entry.path(), entry.file_name());
        if path.is_dir() {
            if name != "replays" {
                copy_game(&path, &to.join(name));
            }
        } else {
            std::fs::copy(&path, to.join(name)).unwrap();
        }
    }
}

/// RENAME of a model asks about the levels and maps that name it beside
/// the Lua, ticked: a map's prefab (Pulsar's rocks, listed with the
/// levels) and a level's piece and a kind's model (Voxkeep's torch).
/// Changed, only the names change, and the game loads with the new file.
#[test]
fn a_model_is_renamed_in_the_levels_and_maps_too() {
    use manage::Answer;
    let root = crate::scratch::Scratch::new("rename-data");
    let games = Path::new(env!("CARGO_MANIFEST_DIR")).join("games");
    // Pulsar: map.toml's prefab.
    let dir = root.join("pulsar");
    copy_game(&games.join("pulsar"), &dir);
    let map = std::fs::read_to_string(dir.join("map.toml")).unwrap();
    let mut shell = shell_on(&dir, &root.join("pulsar.toml"));
    assert_eq!(shell.rename("prefabs/rock_1.vvx", "boulder"), Ok(()));
    let q = shell.asking.as_ref().expect("asked");
    assert_eq!(q.rows.iter().map(|r| (r.file.as_str(), r.text.as_str())).collect::<Vec<_>>(), [("map.toml", "rock_1 = \"prefabs/rock_1.vvx\"")]);
    assert_eq!(q.ticked, [true]);
    shell.answer(Answer::Change);
    assert_eq!(std::fs::read_to_string(dir.join("map.toml")).unwrap(), map.replace("\"prefabs/rock_1.vvx\"", "\"prefabs/boulder.vvx\""));
    let data = crate::map::MapData::read(&std::fs::read_to_string(dir.join("map.toml")).unwrap()).unwrap();
    assert_eq!(data.prefabs["rock_1"].file, "prefabs/boulder.vvx");
    crate::session::Session::<crate::Voxcade>::load(&dir).expect("Pulsar loads with the prefab renamed");
    // Voxkeep: a piece set down in a level, and the kind's model.
    let dir = root.join("voxkeep");
    copy_game(&games.join("voxkeep"), &dir);
    let hall = std::fs::read_to_string(dir.join("levels/01_hall.toml")).unwrap() + "\n[[thing]]\nname = \"lamp\"\nmodel = \"torch.vvx\"\nat = [95, 7, 21]\n";
    std::fs::write(dir.join("levels/01_hall.toml"), &hall).unwrap();
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    let mut shell = shell_on(&dir, &root.join("voxkeep.toml"));
    assert_eq!(shell.rename("torch.vvx", "lamp"), Ok(()));
    let q = shell.asking.as_ref().expect("asked");
    let files: Vec<&str> = q.rows.iter().map(|r| r.file.as_str()).collect();
    assert_eq!(files, ["conf.lua", "levels/01_hall.toml"], "the kind's model and the piece");
    assert!(q.ticked.iter().all(|t| *t));
    shell.answer(Answer::Change);
    assert_eq!(std::fs::read_to_string(dir.join("levels/01_hall.toml")).unwrap(), hall.replace("\"torch.vvx\"", "\"lamp.vvx\""));
    assert_eq!(std::fs::read_to_string(dir.join("conf.lua")).unwrap(), conf.replace("\"torch.vvx\"", "\"lamp.vvx\""));
    let level = crate::grid::GridLevel::read(&dir.join("levels/01_hall.toml")).unwrap();
    assert!(level.things.iter().any(|t| t.model.as_deref() == Some("lamp.vvx")));
    crate::session::Session::<crate::Voxcade>::load(&dir).expect("Voxkeep loads with the model renamed");
    clean_up(&dir, &shell);
}

/// A file asked for that never was in the game is said so, not as gone;
/// a shot says what the main area shows (HELP, not the file behind it).
#[test]
fn a_file_that_never_was_is_not_said_to_be_gone() {
    let (dir, _, mut shell) = managed_game("never-was");
    let ctx = egui::Context::default();
    shell.open("models/bat.vvx");
    drawn_text(&ctx, &mut shell, Vec::new());
    let texts = drawn_text(&ctx, &mut shell, Vec::new());
    assert!(says(&texts, "NO FILE models/bat.vvx IN THE GAME") && !says(&texts, "GONE"), "{texts:?}");
    assert_eq!(shell.showing(), "models/bat.vvx (no such file in the game)");
    // One that was there and went is gone.
    shell.open("notes.txt");
    drawn_text(&ctx, &mut shell, Vec::new());
    std::fs::remove_file(dir.join("notes.txt")).unwrap();
    shell.rescan();
    shell.unload();
    assert!(says(&drawn_text(&ctx, &mut shell, Vec::new()), "notes.txt IS GONE FROM THE FOLDER"));
    shell.help = true;
    assert_eq!(shell.showing(), "HELP");
    clean_up(&dir, &shell);
}

/// RENAME lists the places with a tick each: the certain ones ticked,
/// a maybe (the game's own call) apart and not, an icon of the same
/// name not at all (BunnyFarm's coin); a tick clicked takes a maybe in,
/// and RENAME changes only the places ticked.
#[test]
fn the_places_to_change_are_ticked_one_by_one() {
    let (dir, _, mut shell) = managed_game("rename-ticks");
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  level:sound(\"coin\", player)\n  fly(\"coin\")\nend\nfunction on_draw(frame)\n  frame:icon(\"coin\", 1, 1)\nend\n").unwrap();
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    assert_eq!(shell.rename("sounds/coin.toml", "ring"), Ok(()));
    let q = shell.asking.as_ref().expect("asked");
    let rows: Vec<(&str, usize, bool)> = q.rows.iter().map(|r| (r.file.as_str(), r.line, r.sure)).collect();
    assert_eq!(rows, [("conf.lua", 5, true), ("main.lua", 2, true), ("main.lua", 3, false)], "the icon is no place of the sound");
    assert_eq!(q.ticked, [true, true, false]);
    drawn_text(&ctx, &mut shell, Vec::new());
    let texts = drawn_text(&ctx, &mut shell, Vec::new());
    assert!(says(&texts, "MAYBE") && says(&texts, "RENAME coin.toml TO ring.toml"), "{texts:?}");
    // The maybe's tick, by its text.
    let maybe = texts.iter().find(|(t, _)| t.starts_with("main.lua:3 ")).expect("the maybe's row").1;
    click(&ctx, &mut shell, maybe);
    assert_eq!(shell.asking.as_ref().map(|q| q.ticked.clone()), Some(vec![true, true, true]));
    let texts = drawn_text(&ctx, &mut shell, Vec::new());
    // The question's RENAME, drawn over the main area's.
    let rename = texts.iter().rev().find(|(t, _)| t == "RENAME").expect("the button").1;
    click(&ctx, &mut shell, rename);
    assert!(shell.asking.is_none() && dir.join("sounds/ring.toml").is_file());
    assert_eq!(std::fs::read_to_string(dir.join("main.lua")).unwrap(), "function on_tick()\n  level:sound(\"ring\", player)\n  fly(\"ring\")\nend\nfunction on_draw(frame)\n  frame:icon(\"coin\", 1, 1)\nend\n");
    clean_up(&dir, &shell);
}

/// DELETE asks nothing: the file goes away, the notice says what still
/// names it; the file open before it takes the main area; UNDO puts the
/// same bytes back and opens it again.
#[test]
fn a_deleted_file_can_be_put_back() {
    let (dir, _, mut shell) = managed_game("delete-undo");
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("main.lua");
    shell.open("sounds/coin.toml");
    frame(&ctx, &mut shell);
    let bytes = std::fs::read(dir.join("sounds/coin.toml")).unwrap();
    shell.delete("sounds/coin.toml");
    assert!(!dir.join("sounds/coin.toml").exists() && !shell.files.has("sounds/coin.toml"));
    let gone = shell.gone.as_ref().expect("a notice with UNDO");
    assert_eq!(gone.said, "DELETED coin.toml · conf.lua and main.lua still play it in 2 lines");
    assert!(gone.can_undo() && gone.until > std::time::Instant::now() + std::time::Duration::from_secs(6));
    assert_eq!(std::fs::read(shell.deleted_dir.join("sounds/coin.toml")).unwrap(), bytes, "kept where it went");
    assert_eq!(shell.memory.open.as_deref(), Some("main.lua"), "the file open before it");
    frame(&ctx, &mut shell);
    assert!(matches!(shell.shown, Some(Shown::Code(_))));
    // UNDO.
    shell.undo();
    assert_eq!(std::fs::read(dir.join("sounds/coin.toml")).unwrap(), bytes);
    assert!(shell.gone.is_none() && shell.files.has("sounds/coin.toml") && !shell.deleted_dir.join("sounds/coin.toml").exists());
    assert_eq!(shell.memory.open.as_deref(), Some("sounds/coin.toml"), "open again");
    frame(&ctx, &mut shell);
    assert!(buffer(&shell).code.sound.is_some());
    // A file nothing names says nothing more; one not open leaves the open one be.
    shell.delete("sounds/bell.toml");
    assert_eq!(shell.gone.as_ref().map(|g| g.said.as_str()), Some("DELETED bell.toml"));
    assert_eq!(shell.memory.open.as_deref(), Some("sounds/coin.toml"));
    // The notice goes after its time, with the bytes.
    shell.gone.as_mut().unwrap().until = std::time::Instant::now();
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(shell.gone.is_none(), "gone after its time");
    clean_up(&dir, &shell);
}

/// The open file deleted with none before it leaves nothing open; the
/// UNDO button is on the bar and a click on it puts the file back; a
/// journal of the file is gone with it.
#[test]
fn deleting_the_only_open_file_leaves_the_area_empty() {
    let (dir, _, mut shell) = managed_game("delete-open");
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("tools.lua");
    frame(&ctx, &mut shell);
    type_into_text(&ctx, &mut shell, "-- typed\n");
    shell.previous = None;
    shell.delete("tools.lua");
    assert!(!shell.journals().join("tools.lua").exists(), "no journal left");
    assert_eq!(shell.memory.open, None);
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(shell.shown.is_none(), "nothing shown");
    // What was saved on the way out is what comes back.
    shell.undo();
    assert!(std::fs::read_to_string(dir.join("tools.lua")).unwrap().ends_with("-- typed\n"));
    assert_eq!(shell.memory.open.as_deref(), Some("tools.lua"));
    // The same through the button.
    shell.delete("notes.txt");
    frame(&ctx, &mut shell);
    let undo = shell.undo_button.expect("UNDO drawn on the bar");
    click(&ctx, &mut shell, undo.center());
    assert!(dir.join("notes.txt").is_file() && shell.gone.is_none(), "UNDO clicked");
    clean_up(&dir, &shell);
}

/// A name typed in place in the list: the field takes the name (the
/// ending is beside it, not typed), Enter takes it, Escape leaves it, a
/// name that cannot be is said in plain words and the field stays.
#[test]
fn a_name_is_typed_in_place_in_the_list() {
    let (dir, _, mut shell) = managed_game("rename-field");
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    shell.begin_rename("sounds/bell.toml");
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(shell.renaming.as_ref().is_some_and(|r| r.text == "bell" && !r.fresh));
    frame_with(&ctx, &mut shell, vec![egui::Event::Text("coin".into())]);
    key(&ctx, &mut shell, egui::Key::Enter);
    assert_eq!(shell.renaming.as_ref().and_then(|r| r.problem.as_deref()), Some("sounds/coin.toml is already there."));
    frame_with(&ctx, &mut shell, vec![egui::Event::Text("x".into())]);
    key(&ctx, &mut shell, egui::Key::Escape);
    assert!(shell.renaming.is_none() && dir.join("sounds/bell.toml").is_file(), "Escape leaves it");
    // Enter takes it (the old name is selected: typing replaces it).
    shell.begin_rename("sounds/bell.toml");
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    frame_with(&ctx, &mut shell, vec![egui::Event::Text("Chime".into())]);
    key(&ctx, &mut shell, egui::Key::Enter);
    assert!(shell.renaming.is_none() && dir.join("sounds/chime.toml").is_file() && !dir.join("sounds/bell.toml").exists());
    // A right click on a row opens a menu (RENAME, DELETE).
    let at = egui::pos2(60.0, heading(&shell, "Logic").1.bottom() + 16.0);
    let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&ctx, &mut shell, vec![egui::Event::PointerMoved(at)]);
    frame_with(&ctx, &mut shell, vec![button(true)]);
    frame_with(&ctx, &mut shell, vec![button(false)]);
    frame_with(&ctx, &mut shell, vec![]);
    assert!(egui::Popup::is_any_open(&ctx), "the menu is open");
    clean_up(&dir, &shell);
}

/// A frame with `events`; what the frame asked to be copied.
fn copied_in(ctx: &egui::Context, shell: &mut Shell, events: Vec<egui::Event>) -> Vec<String> {
    let time = ctx.input(|i| i.time) + 0.1;
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), time: Some(time), events, ..Default::default() };
    let out = ctx.run_ui(raw, |ui| shell.ui(ui));
    out.platform_output.commands.iter().filter_map(|c| if let egui::OutputCommand::CopyText(t) = c { Some(t.clone()) } else { None }).collect()
}

/// COPY at a mistake's right end copies it with the file's name in
/// front; COPY ALL over OUTPUT copies every line.
#[test]
fn a_mistake_and_the_output_can_be_copied() {
    let (dir, memory) = sound_game("copy");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("sounds/coin.toml");
    frame(&ctx, &mut shell);
    type_into_text(&ctx, &mut shell, "frq = 1\n");
    COPY_BUTTONS.with(|b| b.borrow_mut().clear());
    frame(&ctx, &mut shell);
    let at = COPY_BUTTONS.with(|b| b.borrow().first().copied()).expect("a COPY button at the mistake").center();
    let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    copied_in(&ctx, &mut shell, vec![egui::Event::PointerMoved(at)]);
    copied_in(&ctx, &mut shell, vec![button(true)]);
    let copied = copied_in(&ctx, &mut shell, vec![button(false)]);
    assert_eq!(copied, ["sounds/coin.toml line 7: frq: no such field (did you mean freq?)"]);
    // OUTPUT's COPY ALL: the lines in order.
    shell.output.add(Kind::Note, "one", None, false);
    shell.output.add(Kind::Error, "two", None, false);
    shell.output_shown = true;
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(shell.output.text().ends_with("one\ntwo"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A frame with `events` and what it drew: the texts, and where (the
/// middle of each).
fn drawn_text(ctx: &egui::Context, shell: &mut Shell, events: Vec<egui::Event>) -> Vec<(String, egui::Pos2)> {
    let time = ctx.input(|i| i.time) + 0.1;
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), time: Some(time), events, ..Default::default() };
    let out = ctx.run_ui(raw, |ui| shell.ui(ui));
    out.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.pos + t.galley.rect.center().to_vec2())),
            _ => None,
        })
        .collect()
}

fn says(texts: &[(String, egui::Pos2)], word: &str) -> bool {
    texts.iter().any(|(t, _)| t.contains(word))
}

/// Time to look at the folder, as a later moment (the tick is given it).
fn later() -> std::time::Instant {
    std::time::Instant::now() + LIST_EVERY * 2
}

/// A file the agent makes in the folder comes into the list by itself,
/// one removed goes from it, a rename is both; what the list keeps (the
/// open file, a name being typed) is not disturbed; nothing is read
/// again while the names are the same.
#[test]
fn the_list_sees_what_is_made_outside() {
    let (dir, memory, mut shell) = managed_game("outside-list");
    let ctx = egui::Context::default();
    shell.open("main.lua");
    shell.naming = Some(Naming { sort: "Scenarios".into(), text: "half a na".into(), blocks: false, problem: None, fresh: false });
    frame(&ctx, &mut shell);
    let mut at = later();
    let mut look = |shell: &mut Shell| {
        at += LIST_EVERY * 2;
        shell.tick(at)
    };
    let (changed, next) = look(&mut shell);
    assert!(!changed && next <= LIST_EVERY, "nothing new: no frame due, a look within a second");
    std::fs::write(dir.join("scenarios/boss.lua"), NEW_SCENARIO).unwrap();
    crate::editor::frames::ModelFile::begun(&dir.join("enemy.vvx")).save().unwrap();
    assert!(!shell.has("scenarios/boss.lua"), "not before the look");
    let (changed, _) = look(&mut shell);
    assert!(changed, "a frame is due");
    assert!(shell.has("scenarios/boss.lua") && shell.has("enemy.vvx"));
    assert!(shell.take_asks().is_empty());
    assert_eq!(shell.opened(), Some("main.lua"), "the open file stays");
    assert_eq!(shell.naming.as_ref().map(|n| n.text.as_str()), Some("half a na"), "a name being typed is left alone");
    assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "boss"), "and it is in the list");
    std::fs::rename(dir.join("scenarios/boss.lua"), dir.join("scenarios/last.lua")).unwrap();
    std::fs::remove_file(dir.join("enemy.vvx")).unwrap();
    assert!(look(&mut shell).0);
    assert!(shell.has("scenarios/last.lua") && !shell.has("scenarios/boss.lua") && !shell.has("enemy.vvx"));
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(says(&texts, "last") && !says(&texts, "boss"));
    // A file being renamed in the list whose file goes: the field goes.
    shell.renaming = Some(manage::Renaming { path: "scenarios/last.lua".into(), text: "last".into(), problem: None, fresh: false });
    std::fs::remove_file(dir.join("scenarios/last.lua")).unwrap();
    look(&mut shell);
    assert!(shell.renaming.is_none());
    clean_up(&dir, &shell);
    let _ = std::fs::remove_file(&memory);
}

/// The open file made away with outside the IDE: the main area says so
/// in plain words; a text keeps its text (with changes too), is not
/// written again by itself, and SAVE AGAIN puts it back; a picture has
/// nothing to show but the sentence.
#[test]
fn the_open_file_vanishing_is_said_and_not_recreated() {
    let (dir, memory, mut shell) = managed_game("vanishing");
    let ctx = egui::Context::default();
    shell.open("scenarios/late.lua");
    frame(&ctx, &mut shell);
    type_into_text(&ctx, &mut shell, "-- mine\n");
    std::fs::remove_file(dir.join("scenarios/late.lua")).unwrap();
    let later = later();
    shell.tick(later);
    assert!(!shell.has("scenarios/late.lua"));
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(says(&texts, "scenarios/late.lua IS GONE FROM THE FOLDER") && says(&texts, "SAVE AGAIN") && says(&texts, "FILE GONE"), "{texts:?}");
    // The autosave's second goes by, and the folder is looked at again.
    shell.tick(later + super::super::buffer::SAVE_AFTER * 3);
    shell.flush();
    assert!(!dir.join("scenarios/late.lua").exists(), "not made again by itself");
    assert!(buffer(&shell).code.text.contains("-- mine"), "the text is kept");
    // SAVE AGAIN writes it back, the changes in it.
    let at = drawn_text(&ctx, &mut shell, vec![]).into_iter().find(|(t, _)| t == "SAVE AGAIN").expect("the button").1;
    click(&ctx, &mut shell, at);
    let back = std::fs::read_to_string(dir.join("scenarios/late.lua")).unwrap();
    assert!(back.contains("-- mine") && shell.has("scenarios/late.lua"), "put back: {back}");
    assert_eq!(shell.saved_word(), Some("SAVED"));
    // A picture: nothing to keep, only the sentence.
    std::fs::write(dir.join("back.png"), "x").unwrap();
    shell.tick(later + LIST_EVERY * 4);
    shell.open("back.png");
    frame(&ctx, &mut shell);
    std::fs::remove_file(dir.join("back.png")).unwrap();
    shell.tick(later + LIST_EVERY * 8);
    assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "back.png IS GONE FROM THE FOLDER"));
    clean_up(&dir, &shell);
    let _ = std::fs::remove_file(&memory);
}

/// A game opened for the first time has its console folded: only its
/// bar, no shell started; the arrow (and the tabs and CLAUDE) unfold it,
/// the arrow folds it again, and what was done is remembered for the next
/// time, an older memory without the height included.
#[test]
fn the_console_is_folded_until_wanted_and_remembered() {
    let (dir, memory) = bare_game("console-folded");
    let _ = std::fs::remove_file(&memory);
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    let arrow = egui::pos2(1280.0 - INSET - 4.0, 720.0 - 15.0);
    for _ in 0..3 {
        frame(&ctx, &mut shell);
    }
    assert!(shell.memory.console_folded && !shell.console().started(), "folded, nothing started");
    click(&ctx, &mut shell, arrow);
    frame(&ctx, &mut shell);
    assert!(!shell.memory.console_folded && shell.console().started(), "unfolded by the arrow, the shell asked for");
    assert!(!Memory::load(&memory).console_folded, "remembered unfolded");
    let again = shell_on(&dir, &memory);
    assert!(!again.memory.console_folded, "and unfolded the next time");
    // (Its bar is now at the top of the console.)
    let top = 720.0 - shell.console_seen.expect("unfolded") + 15.0;
    click(&ctx, &mut shell, egui::pos2(arrow.x, top));
    assert!(shell.memory.console_folded && Memory::load(&memory).console_folded, "folded again by the arrow");
    // A tab unfolds it too (CONSOLE at the left of the bar).
    let tab = drawn_text(&ctx, &mut shell, vec![]).into_iter().find(|(t, _)| t == "CONSOLE").expect("the tab").1;
    click(&ctx, &mut shell, tab);
    assert!(!shell.memory.console_folded);
    // What an older IDE wrote has no height: it loads, unfolded as it said.
    std::fs::write(&memory, "open = \"main.lua\"\nconsole_folded = false\n").unwrap();
    let old = shell_on(&dir, &memory);
    assert!(!old.memory.console_folded && old.memory.console_height.is_none());
    // The height the user dragged to is the height it opens at.
    std::fs::write(&memory, "open = \"main.lua\"\nconsole_folded = false\nconsole_height = 200.0\n").unwrap();
    let mut tall = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut tall);
    }
    assert!(tall.console_seen.is_some_and(|h| (h - 200.0).abs() < 3.0), "{:?}", tall.console_seen);
    // Dragged by its top edge, let go: kept.
    let (from, to) = (egui::pos2(640.0, 720.0 - 200.0), egui::pos2(640.0, 720.0 - 320.0));
    let button = |at, pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    frame_with(&ctx, &mut tall, vec![egui::Event::PointerMoved(from)]);
    frame_with(&ctx, &mut tall, vec![button(from, true)]);
    frame_with(&ctx, &mut tall, vec![egui::Event::PointerMoved(to)]);
    frame_with(&ctx, &mut tall, vec![egui::Event::PointerMoved(to)]);
    frame_with(&ctx, &mut tall, vec![button(to, false)]);
    frame_with(&ctx, &mut tall, vec![]);
    let kept = Memory::load(&memory).console_height.expect("kept");
    assert!(kept > 280.0, "{kept}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&memory);
    let _ = std::fs::remove_dir_all(memory.with_extension("journal"));
}

/// START WITH THE IDE (the agent in the console) behaves as before: the
/// console is unfolded and the agent asked for.
#[test]
fn an_agent_that_starts_with_the_ide_has_the_console_open() {
    let (dir, memory) = bare_game("console-agent");
    let _ = std::fs::remove_file(&memory);
    let prefs = dir.join("ide.toml");
    let mut settings = Prefs::default();
    settings.claude.with_ide = true;
    settings.save(&prefs).unwrap();
    let mut shell = shell_on(&dir, &memory);
    let agent = fake_agent("console-agent");
    shell.finder = super::super::claude::Finder::in_dirs(vec![agent.to_path_buf()]);
    let shell = shell.with_prefs(prefs.clone());
    assert!(!shell.memory.console_folded);
    assert!(shell.console.typed().starts_with("claude "), "typed: {}", shell.console.typed());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(prefs.with_file_name("agents"));
    let _ = std::fs::remove_file(&prefs);
    let _ = std::fs::remove_file(&memory);
}

/// A folder with a program called `claude` that runs (nothing starts it).
fn fake_agent(name: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("agent-{name}"));
    let program = dir.join("claude");
    std::fs::write(&program, "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

/// Where the word `word` was drawn (its middle), for a click.
fn at_word(texts: &[(String, egui::Pos2)], word: &str) -> egui::Pos2 {
    texts.iter().find(|(t, _)| t == word).unwrap_or_else(|| panic!("no {word} in {texts:?}")).1
}

/// The agent's program missing: CLAUDE types nothing; the console says in
/// plain words what an agent is, where to get one, that the IDE works
/// without it and where to choose it; LOOK AGAIN finds it once it is
/// installed and starts it.
#[test]
fn a_missing_agent_is_explained_and_nothing_is_typed() {
    let (dir, memory) = bare_game("no-agent");
    let _ = std::fs::remove_file(&memory);
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    shell.start_agent(false, true);
    assert!(shell.no_agent && shell.console.typed().is_empty(), "nothing typed");
    assert!(!shell.memory.console_folded, "asked for: the console opens");
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    for word in ["NO AGENT FOUND", "Claude Code", "https://claude.com/claude-code", "LOOK AGAIN"] {
        assert!(says(&texts, word), "{word} in {texts:?}");
    }
    // Looking again with nothing installed changes nothing.
    let at = at_word(&texts, "LOOK AGAIN");
    click(&ctx, &mut shell, at);
    assert!(shell.no_agent && shell.console.typed().is_empty());
    // Installed: LOOK AGAIN finds it and types its command.
    let bin = fake_agent("no-agent");
    shell.finder = super::super::claude::Finder::in_dirs(vec![bin.to_path_buf()]);
    let at = at_word(&drawn_text(&ctx, &mut shell, vec![]), "LOOK AGAIN");
    click(&ctx, &mut shell, at);
    assert!(!shell.no_agent, "found");
    assert!(shell.console.typed().starts_with("claude "), "typed: {}", shell.console.typed());
    assert!(!says(&drawn_text(&ctx, &mut shell, vec![]), "NO AGENT FOUND"));
    let _ = std::fs::remove_dir_all(&bin);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&memory);
}

/// Started with the IDE and no agent: nothing typed, the console stays
/// folded over the maker's game, and the bar says NO AGENT FOUND.
#[test]
fn a_missing_agent_does_not_open_the_console_by_itself() {
    let (dir, memory) = bare_game("no-agent-start");
    let _ = std::fs::remove_file(&memory);
    let prefs = dir.join("ide.toml");
    let mut settings = Prefs::default();
    settings.claude.with_ide = true;
    settings.save(&prefs).unwrap();
    let mut shell = shell_on(&dir, &memory).with_prefs(prefs.clone());
    assert!(shell.no_agent && shell.memory.console_folded && shell.console.typed().is_empty());
    let ctx = egui::Context::default();
    assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "NO AGENT FOUND"), "said in the bar");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(prefs.with_file_name("agents"));
    let _ = std::fs::remove_file(&prefs);
    let _ = std::fs::remove_file(&memory);
}

/// The game's keys stand on a line under it: moving, then each action by
/// its own name with its key.
#[test]
fn the_games_keys_stand_under_it() {
    let (dir, memory, mut shell) = managed_game("keys-line");
    let ctx = egui::Context::default();
    shell.stage = Some(Stage { shown: true, keys: vec![("MOVE".into(), "ARROWS".into()), ("JUMP".into(), "Y".into()), ("FIRE".into(), "X".into())], ..Stage::default() });
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    for word in ["MOVE", "ARROWS", "JUMP", "Y", "FIRE", "X"] {
        assert!(texts.iter().any(|(t, _)| t == word), "{word} in {texts:?}");
    }
    let row = |w: &str| at_word(&texts, w);
    assert!(row("MOVE").y > 500.0 && (row("MOVE").y - row("FIRE").y).abs() < 1.0, "on a line low in the main area");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&memory);
}

/// AGAIN beside STOP begins the game anew and shows it; once the game is
/// held behind a file the bar says BACK TO THE GAME instead of PLAY, with
/// STOP and AGAIN still there; shown, neither is.
#[test]
fn again_and_back_to_the_game() {
    let (dir, memory, mut shell) = managed_game("again");
    let ctx = egui::Context::default();
    assert!(!says(&drawn_text(&ctx, &mut shell, vec![]), "AGAIN"), "no game, nothing to begin again");
    assert!(drawn_text(&ctx, &mut shell, vec![]).iter().any(|(t, _)| t == "PLAY"));
    shell.stage = Some(Stage { shown: true, ..Stage::default() });
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(texts.iter().any(|(t, _)| t == "STOP") && texts.iter().any(|(t, _)| t == "AGAIN"));
    assert!(!texts.iter().any(|(t, _)| t == "PLAY" || t == "BACK TO THE GAME"), "playing: neither");
    // Held behind the file.
    shell.back_to_file();
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(texts.iter().any(|(t, _)| t == "BACK TO THE GAME"), "{texts:?}");
    assert!(!texts.iter().any(|(t, _)| t == "PLAY") && texts.iter().any(|(t, _)| t == "STOP") && texts.iter().any(|(t, _)| t == "AGAIN"));
    click(&ctx, &mut shell, at_word(&texts, "BACK TO THE GAME"));
    assert!(shell.stage.as_ref().unwrap().shown, "the game again");
    assert!(shell.take_asks().is_empty(), "shown, not begun anew");
    // AGAIN asks for a new start, from the held state too, and shows the game.
    shell.back_to_file();
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    click(&ctx, &mut shell, at_word(&texts, "AGAIN"));
    assert_eq!(shell.take_asks(), [Ask::Again]);
    assert!(shell.stage.as_ref().unwrap().shown);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&memory);
}

/// While the console is folded a quiet sign says the shell wrote
/// something new (in the accent colour, a dot), and it goes once the
/// console is in view again.
#[test]
fn a_folded_console_shows_that_something_was_written() {
    let (dir, memory) = bare_game("console-news");
    let _ = std::fs::remove_file(&memory);
    let console = Console::new(dir.to_path_buf(), Start::Command("echo one; sleep 1; echo two".into()), std::sync::Arc::new(|| {}));
    let mut shell = Shell::new(&dir, crate::session::config::<crate::Voxcade>(&dir).unwrap(), memory.clone(), console);
    let ctx = egui::Context::default();
    shell.unfold_console();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while shell.console().arrivals() == 0 && std::time::Instant::now() < until {
        frame_with(&ctx, &mut shell, vec![]);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!shell.console().news(), "in view: nothing new");
    shell.memory.console_folded = true;
    while !shell.console().news() && std::time::Instant::now() < until {
        frame_with(&ctx, &mut shell, vec![]);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(shell.console().news(), "written while folded");
    assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "CONSOLE •"));
    shell.memory.console_folded = false;
    frame_with(&ctx, &mut shell, vec![]);
    assert!(!shell.console().news(), "seen");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&memory);
}

/// What is wrong with the game is a red line over whatever the main area
/// shows (a sound, nothing, HELP, the game), with TO THE LINE, SHOW and
/// COPY; it goes when the game is mended.
#[test]
fn the_games_error_shows_over_whatever_is_open() {
    let (dir, memory, mut shell) = managed_game("error-line");
    let ctx = egui::Context::default();
    shell.open("sounds/coin.toml");
    shell.stage = Some(Stage::default());
    frame(&ctx, &mut shell);
    assert!(!says(&drawn_text(&ctx, &mut shell, vec![]), "THE GAME STOPPED"), "all is well");
    shell.stage.as_mut().unwrap().trouble = Some("main.lua:3: attempt to index a nil value\nstack traceback:\n\t[C]: in ?".into());
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(says(&texts, "THE GAME STOPPED: main.lua:3: attempt to index a nil value"), "over a sound: {texts:?}");
    assert!(!says(&texts, "stack traceback"), "its first line");
    assert!(says(&texts, "Something here is nil"), "in plain words under it: {texts:?}");
    assert!(["TO THE LINE", "SHOW", "COPY"].iter().all(|b| says(&texts, b)));
    // OUTPUT has it whole, the traceback too, Lua's own frames left out.
    shell.game_said(shell.stage.as_ref().unwrap().trouble.clone(), Vec::new());
    assert!(shell.output.text().contains("THE GAME STOPPED: main.lua:3: attempt to index a nil value\nstack traceback:") && !shell.output.text().contains("[C]"), "{}", shell.output.text());
    // Over HELP, the game and nothing.
    shell.help = true;
    assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "THE GAME STOPPED"), "over HELP");
    shell.help = false;
    shell.stage.as_mut().unwrap().shown = true;
    assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "THE GAME STOPPED"), "over the game");
    shell.stage.as_mut().unwrap().shown = false;
    shell.memory.open = None;
    shell.unload();
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(says(&texts, "THE GAME STOPPED"), "over nothing");
    // A file that would not load, and a game that would not start.
    shell.stage.as_mut().unwrap().trouble = None;
    shell.stage.as_mut().unwrap().unloaded = Some("sounds/coin.toml would not load: LINE 2: nope".into());
    assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "sounds/coin.toml would not load"));
    shell.stage = None;
    shell.wont_start("conf.lua:2: unexpected symbol".into());
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(says(&texts, "THE GAME WOULD NOT START: conf.lua:2: unexpected symbol") && says(&texts, "TO THE LINE"));
    assert!(shell.output.errors(5).iter().any(|l| l.text.starts_with("THE GAME WOULD NOT START") && l.at == Some(("conf.lua".into(), 2))), "and in OUTPUT");
    // SHOW: OUTPUT, the console unfolded (and there, though the settings left it out).
    shell.prefs.console = false;
    let at = texts.iter().find(|(t, _)| t == "SHOW").expect("SHOW").1;
    click(&ctx, &mut shell, at);
    assert!(shell.prefs.console && shell.output_shown && !shell.memory.console_folded);
    // TO THE LINE: the file at the line.
    let at = drawn_text(&ctx, &mut shell, vec![]).into_iter().find(|(t, _)| t == "TO THE LINE").expect("the button").1;
    click(&ctx, &mut shell, at);
    assert_eq!(shell.opened(), Some("conf.lua"));
    // Mended: the line goes.
    shell.start_mended();
    assert!(!says(&drawn_text(&ctx, &mut shell, vec![]), "TO THE LINE"));
    clean_up(&dir, &shell);
    let _ = std::fs::remove_file(&memory);
}

/// BUILD says BUILDING… while it packs (the window goes on), makes the
/// zip in the game's own folder under `builds/`, and says where, with
/// SHOW IN FOLDER; a build that cannot says why, in red, with COPY. The
/// list does not show `builds/`.
#[test]
fn build_works_in_the_background_and_lands_in_the_games_folder() {
    let (dir, memory) = bare_game("build");
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.build();
    assert!(says(&drawn_text(&ctx, &mut shell, vec![]), "BUILDING…"));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while shell.building.is_some() && std::time::Instant::now() < until {
        shell.tick(std::time::Instant::now());
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let Some(Notice::Built(zip, said)) = &shell.notice else { panic!("built") };
    let name = crate::settings::game_name(&dir);
    assert!(zip.starts_with(dir.canonicalize().unwrap().join("builds")) && zip.is_file(), "{zip:?}");
    assert!(said.starts_with("BUILT ") && said.contains(&format!("{name}-")), "{said}");
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(says(&texts, "SHOW IN FOLDER") && says(&texts, "BUILT "), "{texts:?}");
    assert!(!shell.files.assets.iter().any(|g| g.files.iter().any(|f| f.starts_with("builds/"))), "not listed");
    assert!(!Files::names(&dir).iter().any(|f| f.starts_with("builds/")));
    // A build that cannot: `builds` is a file.
    std::fs::remove_dir_all(dir.join("builds")).unwrap();
    std::fs::write(dir.join("builds"), "in the way").unwrap();
    shell.build();
    while shell.building.is_some() && std::time::Instant::now() < until {
        shell.tick(std::time::Instant::now());
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(matches!(&shell.notice, Some(Notice::Failed(w)) if w.starts_with("BUILD FAILED: ")), "failed");
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert!(says(&texts, "BUILD FAILED") && says(&texts, "COPY"));
    assert!(shell.output.errors(2).iter().any(|l| l.text.starts_with("BUILD FAILED")));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&memory);
}

/// `text` written into `file` from outside (the agent), its time set
/// ahead so that the next look sees it.
fn write_outside(file: &Path, text: &str) {
    std::fs::write(file, text).unwrap();
    std::fs::File::options().write(true).open(file).unwrap().set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(2)).unwrap();
}

/// `tools.lua` open with the user's typing in it, then changed outside
/// too: CHANGED OUTSIDE TOO shows. The user's text, and theirs.
fn asked(ctx: &egui::Context, shell: &mut Shell, dir: &Path) -> (String, &'static str) {
    shell.open("tools.lua");
    frame(ctx, shell);
    type_into_text(ctx, shell, "-- mine\n");
    let mine = buffer(shell).code.text.clone();
    assert!(mine.contains("-- mine") && buffer(shell).unsaved());
    let theirs = "return { theirs = true }\n";
    write_outside(&dir.join("tools.lua"), theirs);
    shell.tick(later());
    assert_eq!(buffer(shell).theirs.as_deref(), Some(theirs));
    assert!(says(&drawn_text(ctx, shell, vec![]), "CHANGED OUTSIDE TOO"));
    (mine, theirs)
}

/// While CHANGED OUTSIDE TOO waits for an answer, another file chosen
/// (as OPEN, PLAY, BUILD and the rest: they save the text first) writes
/// nothing over the file and loses nothing: the bar says the text was
/// kept, DELETE waits for the answer, and the file opened again shows
/// both and asks again; KEEP MINE then saves the user's text.
#[test]
fn a_question_left_open_keeps_the_text_when_another_file_is_chosen() {
    let (dir, _memory, mut shell) = managed_game("asked-then-switched");
    let ctx = egui::Context::default();
    let (mine, theirs) = asked(&ctx, &mut shell, &dir);
    shell.open("main.lua");
    frame(&ctx, &mut shell);
    assert_eq!(std::fs::read_to_string(dir.join("tools.lua")).unwrap(), theirs, "nothing written over theirs");
    assert!(matches!(&shell.notice, Some(Notice::Said(n)) if n.starts_with("tools.lua NOT SAVED")), "said");
    assert!(shell.journals().join("tools.lua").is_file(), "the text kept in its journal");
    // DELETE would take the user's text with the file: not before the answer.
    shell.delete("tools.lua");
    assert!(dir.join("tools.lua").is_file() && shell.message.as_deref().is_some_and(|m| m.contains("KEEP MINE")), "{:?}", shell.message);
    shell.open("tools.lua");
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert_eq!(buffer(&shell).code.text, mine, "the user's text back");
    assert_eq!(buffer(&shell).theirs.as_deref(), Some(theirs), "asked again");
    assert!(says(&texts, "CHANGED OUTSIDE TOO"), "{texts:?}");
    let at = texts.iter().find(|(t, _)| t == "KEEP MINE").expect("the button").1;
    click(&ctx, &mut shell, at);
    shell.tick(later() + super::super::buffer::SAVE_AFTER * 2);
    assert_eq!(std::fs::read_to_string(dir.join("tools.lua")).unwrap(), mine, "kept and saved");
    assert!(!shell.journals().join("tools.lua").exists());
    clean_up(&dir, &shell);
}

/// The IDE closed (or another game opened) while CHANGED OUTSIDE TOO
/// waits: opened again, the text is there with the question.
#[test]
fn a_question_left_open_keeps_the_text_when_the_ide_closes() {
    let (dir, memory, mut shell) = managed_game("asked-then-closed");
    let ctx = egui::Context::default();
    let (mine, theirs) = asked(&ctx, &mut shell, &dir);
    // What the window does as it closes.
    shell.flush();
    shell.save().unwrap();
    drop(shell);
    assert_eq!(std::fs::read_to_string(dir.join("tools.lua")).unwrap(), theirs, "nothing written over theirs");
    let mut shell = shell_on(&dir, &memory);
    shell.deleted_dir = dir.with_extension("deleted");
    let ctx = egui::Context::default();
    let texts = drawn_text(&ctx, &mut shell, vec![]);
    assert_eq!(shell.opened(), Some("tools.lua"));
    assert_eq!((buffer(&shell).code.text.as_str(), buffer(&shell).theirs.as_deref()), (mine.as_str(), Some(theirs)));
    assert!(says(&texts, "CHANGED OUTSIDE TOO"), "{texts:?}");
    // TAKE THE FILE'S: the user's text let go of, by the user.
    let at = texts.iter().find(|(t, _)| t == "TAKE THE FILE'S").expect("the button").1;
    click(&ctx, &mut shell, at);
    assert_eq!(buffer(&shell).code.text, theirs);
    assert!(!shell.journals().join("tools.lua").exists());
    clean_up(&dir, &shell);
}

/// A sound's file that is no text says so to the agent (not "a sound
/// recording": recordings are refused).
#[test]
fn a_sound_file_that_is_no_text_is_said_so() {
    let (dir, _memory, mut shell) = managed_game("sound-not-text");
    std::fs::write(dir.join("sounds/broken.toml"), [0xff, 0xfe, 0x00]).unwrap();
    shell.rescan();
    let ctx = egui::Context::default();
    shell.open("sounds/broken.toml");
    frame(&ctx, &mut shell);
    let kind = shell.state(&ctx)["file"]["kind"].as_str().unwrap_or_default().to_string();
    assert!(kind.starts_with("a sound file that would not open as text") && !kind.contains("recording"), "{kind}");
    clean_up(&dir, &shell);
}

/// A new song's own words say how a game plays it, and they work: the
/// line for conf.lua and the call for the code, in a game.
#[test]
fn a_new_song_says_how_a_game_plays_it() {
    let song = NEW_SONG.replace("NAME", "tune");
    crate::audio::music::parse(&song).expect("the new song parses");
    let comments: Vec<&str> = song.lines().filter_map(|l| l.strip_prefix(';')).map(str::trim).collect();
    let conf_line = comments.iter().find(|l| l.starts_with("c.audio.music =")).expect("the line for conf.lua");
    let call = comments.iter().find_map(|l| l.find("music:play(").map(|at| &l[at..])).expect("the call");
    let call = &call[..=call.find(')').expect("the call closed")];
    let dir = crate::scratch::Scratch::new("shell-new-song");
    let base = include_str!("../../../templates/empty/conf.lua");
    let at = base.rfind("end").expect("conf ends");
    std::fs::write(dir.join("conf.lua"), format!("{}  {conf_line}\n{}", &base[..at], &base[at..])).unwrap();
    for model in ["hero.vvx", "hero2.vvx"] {
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/empty").join(model), dir.join(model)).unwrap();
    }
    std::fs::write(dir.join("main.lua"), format!("function on_level_start(how)\n  {call}\nend\nfunction on_tick()\n  world.state.song = music.song\nend\n")).unwrap();
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/tune.mml"), &song).unwrap();
    let mut game = crate::session::Session::<crate::Voxcade>::load(&dir).unwrap();
    for _ in 0..30 {
        game.step(&crate::input::Input::default());
    }
    assert!(game.trouble().is_none(), "{call}: {:?}", game.trouble());
    use crate::game::Game;
    let state = game.game.inspect().to_string();
    assert!(state.contains("\"song\":\"tune\""), "{state}");
}

/// A key with modifiers, down and up.
fn chord(ctx: &egui::Context, shell: &mut Shell, key: egui::Key, modifiers: egui::Modifiers) {
    let event = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
    frame_with(ctx, shell, vec![event(true)]);
    frame_with(ctx, shell, vec![event(false)]);
}

/// A text with a few gems, open in the main area.
fn gems(name: &str) -> (crate::scratch::Scratch, Shell, egui::Context) {
    let (dir, memory) = bare_game(name);
    std::fs::write(dir.join("main.lua"), "-- a gem\nfunction on_tick()\n  local Gem = gem\nend\n").unwrap();
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("main.lua");
    shell.back_to_file();
    for _ in 0..3 {
        frame_with(&ctx, &mut shell, vec![]);
    }
    (dir, shell, ctx)
}

/// Ctrl+F shows the find row with the keys in its field: the word typed
/// there (not into the text) is counted and marked, Enter and F3 go on
/// round the end, Shift+Enter back, MATCH CASE counts exactly; Escape
/// puts the row away and gives the text the keys again.
#[test]
fn ctrl_f_finds_in_the_open_text() {
    let (_dir, mut shell, ctx) = gems("find");
    let before = buffer(&shell).code.text.clone();
    chord(&ctx, &mut shell, egui::Key::F, egui::Modifiers::COMMAND);
    assert!(shell.find.shown, "Ctrl+F shows the row");
    frame_with(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(super::super::find::word_id())), "with the keys in its field");
    frame_with(&ctx, &mut shell, vec![egui::Event::Text("gem".into())]);
    frame_with(&ctx, &mut shell, vec![]);
    assert_eq!(buffer(&shell).code.text, before, "typed into the field, not the text");
    assert_eq!(shell.find.count(), "1 OF 3");
    assert!(buffer(&shell).code.marking.as_ref().is_some_and(|(w, case, _)| w == "gem" && !case), "marked in the text");
    key(&ctx, &mut shell, egui::Key::Enter);
    assert_eq!(shell.find.count(), "2 OF 3", "Enter: the next");
    assert!(ctx.memory(|m| m.has_focus(super::super::find::word_id())), "the field keeps the keys");
    key(&ctx, &mut shell, egui::Key::F3);
    key(&ctx, &mut shell, egui::Key::F3);
    assert_eq!(shell.find.count(), "1 OF 3", "F3 round the end");
    chord(&ctx, &mut shell, egui::Key::Enter, egui::Modifiers::SHIFT);
    assert_eq!(shell.find.count(), "3 OF 3", "Shift+Enter back round the start");
    // The text's cursor after the current one.
    let cursor = egui::TextEdit::load_state(&ctx, shell.code_id().unwrap().1).and_then(|s| s.cursor.char_range()).unwrap();
    assert_eq!(cursor.primary.index.0, 45);
    shell.find.match_case = true;
    frame_with(&ctx, &mut shell, vec![]);
    assert_eq!(shell.find.count(), "2 OF 2", "MATCH CASE: the Gem not counted, the current one kept");
    key(&ctx, &mut shell, egui::Key::Escape);
    frame_with(&ctx, &mut shell, vec![]);
    assert!(!shell.find.shown, "Escape puts it away");
    assert!(buffer(&shell).code.marking.is_none(), "no marks left");
    assert!(ctx.memory(|m| m.has_focus(shell.code_id().unwrap().1)), "the text has the keys again");
    assert_eq!(buffer(&shell).code.text, before);
}

/// REPLACE is one step of the text's undo, REPLACE ALL one step whole;
/// Ctrl+H shows the replace field.
#[test]
fn replacing_is_one_step_back() {
    let (_dir, mut shell, ctx) = gems("replace");
    let first = buffer(&shell).code.text.clone();
    let id = shell.code_id().unwrap().1;
    chord(&ctx, &mut shell, egui::Key::H, egui::Modifiers::COMMAND);
    assert!(shell.find.shown && shell.find.replacing, "Ctrl+H: the replace field too");
    shell.find.word = "gem".into();
    shell.find.with = "jewel".into();
    frame_with(&ctx, &mut shell, vec![]);
    shell.find.press = Some(super::super::find::Act::Replace);
    frame_with(&ctx, &mut shell, vec![]);
    frame_with(&ctx, &mut shell, vec![]);
    assert_eq!(buffer(&shell).code.text, first.replacen("gem", "jewel", 1));
    assert_eq!(shell.find.count(), "1 OF 2", "on to the next");
    let back = |shell: &mut Shell| {
        let Some(Shown::Code(b)) = &mut shell.shown else { panic!("no text open") };
        assert!(b.code.go_back(&ctx, id), "a step to go back");
        b.code.text.clone()
    };
    assert_eq!(back(&mut shell), first, "one step back");
    shell.find.press = Some(super::super::find::Act::ReplaceAll);
    frame_with(&ctx, &mut shell, vec![]);
    frame_with(&ctx, &mut shell, vec![]);
    assert_eq!(buffer(&shell).code.text, "-- a jewel\nfunction on_tick()\n  local jewel = jewel\nend\n");
    assert!(buffer(&shell).unsaved(), "saved as typing is");
    assert_eq!(shell.find.count(), "NO MATCHES");
    assert_eq!(back(&mut shell), first, "REPLACE ALL: one step back whole");
}

/// Ctrl+Shift+F (or SEARCH) shows the search across the game in the
/// list's place: the word typed there finds the lines of the game's
/// texts (not what its .gitignore ignores); a line clicked opens its
/// file there, the word found current in the find row; Escape in the
/// field shows the list again.
#[test]
fn the_project_search_opens_the_line_found() {
    let (dir, mut shell, ctx) = gems("search");
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\" }\n  -- no gem here\nend\n").unwrap();
    std::fs::create_dir_all(dir.join("notes")).unwrap();
    std::fs::write(dir.join("notes/gem.lua"), "gem\n").unwrap();
    std::fs::write(dir.join(".gitignore"), "notes/\n").unwrap();
    chord(&ctx, &mut shell, egui::Key::F, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
    assert!(shell.search.shown && !shell.find.shown, "Ctrl+Shift+F: the search, not the row");
    frame_with(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(super::super::find::search_id())), "with the keys");
    frame_with(&ctx, &mut shell, vec![egui::Event::Text("GEM".into())]);
    shell.search.wait(&dir, &ctx);
    frame_with(&ctx, &mut shell, vec![]);
    let places: Vec<(&str, usize)> = shell.search.hits.iter().map(|h| (h.file.as_str(), h.line)).collect();
    assert_eq!(places, [("conf.lua", 5), ("main.lua", 1), ("main.lua", 3)]);
    // The panel wider while it searches (the list's is 250): a hit shows
    // more of its line.
    assert!(shell.search.rows[0].width() > 340.0, "{:?}", shell.search.rows[0]);
    let row = shell.search.rows[2];
    click(&ctx, &mut shell, row.center());
    assert_eq!(shell.opened(), Some("main.lua"));
    assert!(shell.find.shown && shell.find.word == "GEM");
    assert_eq!(shell.find.count(), "2 OF 3", "the first on the line clicked current");
    assert!(shell.search.shown, "the search stays beside the file");
    let row = shell.search.rows[0];
    click(&ctx, &mut shell, row.center());
    assert_eq!(shell.opened(), Some("conf.lua"));
    assert_eq!(shell.find.count(), "1 OF 1");
    // Escape in its field: the list again.
    shell.search.open(None);
    frame_with(&ctx, &mut shell, vec![]);
    key(&ctx, &mut shell, egui::Key::Escape);
    assert!(!shell.search.shown, "Escape shows the list again");
}

/// A frame at `size` and the texts it drew with the room each takes.
fn drawn_rects(ctx: &egui::Context, shell: &mut Shell, size: egui::Vec2) -> Vec<(String, egui::Rect)> {
    let time = ctx.input(|i| i.time) + 0.1;
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), time: Some(time), ..Default::default() };
    let out = ctx.run_ui(raw, |ui| shell.ui(ui));
    out.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.galley.rect.translate(t.pos.to_vec2()))),
            _ => None,
        })
        .collect()
}

/// The button with `word` in the main area (right of the list), by where it was drawn.
fn button_in_main(ctx: &egui::Context, shell: &mut Shell, word: &str) -> egui::Pos2 {
    drawn_text(ctx, shell, vec![]).into_iter().find(|(t, at)| t == word && at.x > 300.0).unwrap_or_else(|| panic!("no button {word}")).1
}

/// The row over an open file never runs into its buttons: in a narrow
/// window the line of facts is cut with "…" and ends left of SAVED (a
/// sound, a song, a script, a model), in a wide one it is whole.
#[test]
fn the_row_over_a_file_leaves_its_buttons_their_room() {
    let (dir, _, mut shell) = managed_game("header-row");
    std::fs::write(dir.join("sounds/coin.toml"), "wave = \"triangle\"\nfreq = 880\nhold = 0.04\ndecay = 0.16\nsweep = -12\njump = 7\njump_at = 0.05\n").unwrap();
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    let mut widths = Vec::new();
    for (file, facts) in [("sounds/coin.toml", "0.2"), ("music/tune.mml", "TUNE"), ("hero.vvx", "MODEL"), ("main.lua", "")] {
        shell.open(file);
        for width in [860.0, 1280.0, 1920.0] {
            let size = egui::vec2(width, 720.0);
            drawn_rects(&ctx, &mut shell, size);
            let texts = drawn_rects(&ctx, &mut shell, size);
            let in_main: Vec<_> = texts.iter().filter(|(_, r)| r.min.x > 250.0 && r.min.y < 110.0).collect();
            let name = in_main.iter().find(|(t, _)| t == file).unwrap_or_else(|| panic!("{file} at {width}: {in_main:?}")).1;
            let buttons_from = in_main.iter().filter(|(t, _)| ["RENAME", "DELETE", "SAVED", "FIND", "EDIT", "TEXT", "PLAY SONG", "PLAY SOUND"].contains(&t.as_str())).map(|(_, r)| r.min.x).fold(f32::MAX, f32::min);
            assert!(name.max.x <= buttons_from, "{file} at {width}: the path runs into the buttons");
            if let Some((line, r)) = in_main.iter().find(|(t, _)| !facts.is_empty() && t.starts_with(facts) && t != file) {
                assert!(r.max.x <= buttons_from, "{file} at {width}: {line:?} runs into the buttons");
                widths.push((file, width, r.width()));
            }
        }
    }
    // The sound's long line is cut in the narrow window, whole in the wide one.
    let of = |w: f32| widths.iter().find(|(f, width, _)| *f == "sounds/coin.toml" && *width == w).map(|x| x.2).unwrap();
    assert!(of(860.0) < of(1920.0), "{widths:?}");
    clean_up(&dir, &shell);
}

/// TEXT on a model shows its text and PICTURE brings the picture back: the
/// button unloads the file in the middle of the frame, and nothing below it
/// may expect what was loaded.
#[test]
fn a_model_goes_to_its_text_and_back() {
    let (dir, _, mut shell) = managed_game("model-text");
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("hero.vvx");
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    assert!(shell.preview.is_some() && !matches!(shell.shown, Some(Shown::Code(_))), "the picture first");
    let at = button_in_main(&ctx, &mut shell, "TEXT");
    click(&ctx, &mut shell, at);
    assert!(shell.model_text && matches!(shell.shown, Some(Shown::Code(_))), "the text shows");
    assert!(shell.preview.is_none());
    let at = button_in_main(&ctx, &mut shell, "PICTURE");
    click(&ctx, &mut shell, at);
    assert!(!shell.model_text && !matches!(shell.shown, Some(Shown::Code(_))) && shell.preview.is_some(), "the picture is back");
    clean_up(&dir, &shell);
}

/// A level's line SEARCH found opens in the level's text at that line
/// (it opened the picture, where the word cannot be marked), read by the
/// level's reader after every change, its mistake "LINE n: why"; PICTURE
/// brings the picture back. A map is text to change always.
#[test]
fn a_level_found_by_search_opens_as_its_text() {
    let (dir, memory) = sound_game("level-text");
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("levels/01.toml"), "format = \"voxcade level 1\"\nsize = [40, 40, 20]\n\n[[place]]\nname = \"gem_spot\"\nat = [4, 4, 0]\n").unwrap();
    std::fs::write(dir.join("map.toml"), "format = \"voxcade map 1\"\n# gem\n").unwrap();
    let mut shell = shell_on(&dir, &memory);
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    let (hits, _) = crate::ide::find::search(&dir, "gem_spot", false);
    assert_eq!(hits.iter().map(|h| (h.file.as_str(), h.line)).collect::<Vec<_>>(), [("levels/01.toml", 5)]);
    shell.go_to_hit("levels/01.toml", 5);
    frame(&ctx, &mut shell);
    frame(&ctx, &mut shell);
    let Some(Shown::Code(b)) = &mut shell.shown else { panic!("the level's text") };
    assert!(b.code.error.is_none() && b.code.model.as_deref().is_some_and(|l| l.contains("1 place")), "{:?}", b.code.model);
    b.code.text = b.code.text.replace("size = [40, 40, 20]", "size = [40, 40");
    b.code.check();
    assert!(b.code.error.as_ref().is_some_and(|(line, _)| *line > 0), "a mistake with its line: {:?}", b.code.error);
    b.code.text = b.code.text.replace("size = [40, 40", "size = [40, 40, 20]");
    b.code.check();
    let at = button_in_main(&ctx, &mut shell, "PICTURE");
    click(&ctx, &mut shell, at);
    frame(&ctx, &mut shell);
    assert!(!shell.model_text && shell.preview.is_some(), "the picture is back");
    shell.open("map.toml");
    frame(&ctx, &mut shell);
    assert!(matches!(&shell.shown, Some(Shown::Code(b)) if b.code.error.as_ref().is_some_and(|(_, why)| why.contains("size"))), "the map's text to change, read by the map's reader");
    clean_up(&dir, &shell);
}

/// The other buttons that change what is shown in the middle of a frame:
/// PLAY SCENARIO asks for the game and leaves the text, MAKE IT YOURS makes
/// the file and opens it as text.
#[test]
fn buttons_that_change_what_is_shown_leave_a_whole_frame() {
    let (dir, _, shell) = managed_game("mid-frame");
    let mut shell = shell.with_sounds(crate::audio::core_sounds());
    let ctx = egui::Context::default();
    frame(&ctx, &mut shell);
    shell.open("scenarios/late.lua");
    frame(&ctx, &mut shell);
    let at = button_in_main(&ctx, &mut shell, "PLAY SCENARIO");
    click(&ctx, &mut shell, at);
    assert!(shell.take_asks().iter().any(|a| matches!(a, Ask::Play(Some(n)) if n == "late")));
    shell.builtin = Some("explosion");
    frame(&ctx, &mut shell);
    let at = button_in_main(&ctx, &mut shell, "MAKE IT YOURS");
    click(&ctx, &mut shell, at);
    assert!(dir.join("sounds/explosion.toml").exists() && matches!(shell.shown, Some(Shown::Code(_))));
    clean_up(&dir, &shell);
}
