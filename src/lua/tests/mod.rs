//! The Lua door's rules, each tried on a small game in a temporary folder
//! with a world made in code.

mod course;
mod events;
mod input;
mod kits;
mod mistakes;
mod music;
mod picture;
mod pieces;
mod runs;
mod sandbox;
mod saves;
mod scenarios;
mod seats;
mod states;

use std::path::{Path, PathBuf};

/// The program's Lua games: with the shooter kit when they ask for it.
type LuaGame = crate::Voxcade;
use crate::game::Game;
use crate::input::Input;
use crate::session::{Scene, Session};

const CONF: &str = r#"
function conf(c)
  c.world.size = { 64, 64, 16 }
  c.player.spawn = { 32, 32 }
  c.actions = { "jump", "fire" }
  c.things.miner = {}
  c.things.gem = { hp = 2 }
end
"#;

/// A game folder with this `main.lua` (and the conf above).
fn folder(name: &str, main: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("lua-{name}"));
    std::fs::write(dir.join("conf.lua"), CONF).unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    dir
}

fn game(name: &str, main: &str) -> Session<LuaGame> {
    game_in(&folder(name, main))
}

fn game_in(dir: &Path) -> Session<LuaGame> {
    Session::<LuaGame>::load(dir).unwrap()
}

/// The first text `on_draw` put on the HUD.
fn first_text(g: &Session<LuaGame>) -> String {
    g.game.drawn.borrow().iter().find_map(|c| match c { super::world::Cmd::Text { text, .. } => Some(text.clone()), _ => None }).expect("a text on the HUD")
}

fn steps(game: &mut Session<LuaGame>, n: usize) {
    for _ in 0..n {
        game.step(&Input::default());
    }
}

fn halted(game: &Session<LuaGame>) -> String {
    game.game.halted().unwrap_or_default()
}

/// What stops a game as it starts: a mistake loading it, or one its first
/// handlers made (it then loads, stopped); empty when it starts.
fn start_error(dir: &Path) -> String {
    match Session::<LuaGame>::load(dir) {
        Err(e) => format!("{e:#}"),
        Ok(g) => halted(&g),
    }
}

/// A number the script kept in `world.state`.
fn state(game: &mut Session<LuaGame>, key: &str) -> String {
    let Some(super::Value::Table(entries)) = game.game.capture().unwrap().state else { return String::new() };
    entries.iter().find(|(k, _)| *k == super::value::Key::Str(key.as_bytes().to_vec())).map(|(_, v)| format!("{v:?}")).unwrap_or_default()
}

/// A game folder on the Arena's ground with the shooter kit, its own
/// `conf.lua` lines and this `main.lua`.
fn kit_folder(name: &str, conf: &str, main: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("kit-{name}"));
    let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena");
    for file in ["world.vvx", "player.vvx"] {
        std::fs::copy(arena.join(file), dir.join(file)).unwrap();
    }
    // The arena's, with the kit's weapons it has not (kit.toml), and the test's.
    let base = std::fs::read_to_string(arena.join("conf.lua")).unwrap();
    let kit: toml::Table = toml::from_str(crate::shooter::kinds::KIT).unwrap();
    let lua = |v: &toml::Value| match v {
        toml::Value::String(s) => format!("{s:?}"),
        toml::Value::Array(a) => format!("{{ {} }}", a.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")),
        other => other.to_string(),
    };
    let weapons: String = kit["weapons"].as_table().unwrap().iter().filter(|(name, _)| !base.contains(&format!("c.weapons.{name} ="))).map(|(name, t)| {
        let keys: Vec<String> = t.as_table().unwrap().iter().map(|(k, v)| format!("{k} = {}", lua(v))).collect();
        format!("  c.weapons.{name} = {{ {} }}\n", keys.join(", "))
    }).collect();
    std::fs::write(dir.join("conf.lua"), base.replace("\nend", &format!("\n{weapons}{conf}\nend"))).unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    dir
}

/// A game folder with a world made in code (`c.world.size`), this
/// `main.lua` and these files beside it.
fn code_world(name: &str, main: &str, files: &[(&str, &str)]) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("code-{name}"));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 32, 32, 8 }\n  c.things.box = {}\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

/// A level's name and its places (name, where).
type LevelPlaces<'a> = (&'a str, &'a [(&'a str, [i32; 3])]);

/// A game folder of levels of blocks in `levels/`, each a floor of rock
/// four cells long with these places, and this `main.lua`.
fn levels_folder(name: &str, main: &str, levels: &[LevelPlaces]) -> crate::scratch::Scratch {
    use crate::blocks::{self, Block};
    use crate::grid::{BlockEntry, Cell, GridLevel, Kind, Place};
    use glam::IVec3;
    let dir = crate::scratch::Scratch::new(&format!("levels-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\n  c.things.box = {}\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    for (level_name, places) in levels {
        let mut level = GridLevel::new(4, IVec3::new(4, 1, 1));
        level.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
        level.cells.fill(Cell { block: 1, turn: Default::default() });
        level.places = places.iter().map(|(n, at)| Place { name: n.to_string(), at: *at, facing: Some([1, 0]), named: false, line: 0 }).collect();
        level.save(&dir.join("levels").join(format!("{level_name}.toml"))).unwrap();
    }
    let mut rock = Block::empty(4);
    rock.set(IVec3::ZERO, Some([90, 80, 70])).unwrap();
    rock.save(&blocks::path(&dir, "rock")).unwrap();
    dir
}

/// A platformer level of blocks in `levels/a.toml`: a floor of rock, the
/// hero's marker at x 3 and these things more; `conf.lua` with these lines
/// more.
fn platformer_levels(name: &str, main: &str, conf: &str, more: &[(&str, [i32; 3])]) -> crate::scratch::Scratch {
    let dir = levels_folder(name, main, &[("a", &[])]);
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = {{ hp = 3, lives = 3, taken = 0 }}\n  c.world.blocks = {{ file = \"level.toml\" }}\n  c.things.hero = {{ behavior = \"hero\", height = 6 }}\n  c.things.door = {{ behavior = \"prop\", stands = true }}\n{conf}end\n")).unwrap();
    let path = dir.join("levels/a.toml");
    let mut level = crate::grid::GridLevel::read(&path).unwrap();
    for (kind, at) in std::iter::once(&("hero", [3, 2, 4])).chain(more) {
        level.things.push(crate::grid::Thing::new(kind.to_string(), *at));
    }
    level.save(&path).unwrap();
    dir
}

/// A game of one level of blocks (16 voxels long) with the player's figure
/// at x 2, a door at x 12 and these lines more in `conf.lua`.
fn events_folder(name: &str, main: &str, conf: &str) -> crate::scratch::Scratch {
    let dir = levels_folder(name, main, &[("a", &[])]);
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.blocks = {{ file = \"level.toml\" }}\n  c.player.spawn = {{ 2, 2 }}\n  c.player.kind = \"hero\"\n  c.things.hero = {{ hp = 3 }}\n  c.things.door = {{}}\n  c.things.box = {{ hp = 2 }}\n{conf}end\n")).unwrap();
    let path = dir.join("levels/a.toml");
    let mut level = crate::grid::GridLevel::read(&path).unwrap();
    level.things.push(crate::grid::Thing::new("door", [12, 2, 4]));
    level.save(&path).unwrap();
    // A solid floor four voxels deep.
    let mut rock = crate::blocks::Block::empty(4);
    for z in 0..4 {
        for y in 0..4 {
            for x in 0..4 {
                rock.set(glam::IVec3::new(x, y, z), Some([90, 80, 70])).unwrap();
            }
        }
    }
    rock.save(&crate::blocks::path(&dir, "rock")).unwrap();
    dir
}

/// A game for two (`docs/game-model.md`, "Several players"): a hero each,
/// walked by its own keys; joins and leaves noted in `world.state`.
fn two_players(name: &str, main: &str) -> (Session<LuaGame>, crate::scratch::Scratch) {
    let dir = folder(name, main);
    std::fs::write(
        dir.join("conf.lua"),
        "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\", \"fire\" }\n  c.players = 2\n  c.things.hero = { hp = 3 }\n  c.things.hero_blue = { like = \"hero\" }\n  c.player.kind = { \"hero\", \"hero_blue\" }\n  c.state.player = { lives = 3 }\nend\n",
    )
    .unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    (game_in(&dir), dir)
}

const TWO_PLAYERS: &str = r#"
function on_world_start()
  world.state.told = ""
end
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, 1)
end
function on_player_join(p, how)
  world.state.told = world.state.told .. "join " .. p.seat .. " " .. how .. ";"
end
function on_player_leave(p)
  world.state.told = world.state.told .. "leave " .. p.seat .. ";"
end
function on_tick()
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 30)
    if p.input:pressed("fire") then p.state.score = p.state.score + 10 end
  end
end
"#;

/// A model 3 by 3 by 4 of one colour, as text.
const BOX: &str = "format = \"voxcade voxel 1\"\nsize = [3, 3, 4]\n\n[palette]\na = { color = \"#808080\" }\n\n[[frame]]\nlayers = \"\"\"\naaa *3\n*4\n\"\"\"\n";

/// A room of pieces (`grid::PIECE`): a stove facing east, a lamp hung in
/// the air with its light, the moon (a light alone), a place, an area.
const ROOM: &str = r##"format = "voxcade level 1"
size = [64, 48, 40]

[[thing]]
name = "stove"
model = "box.vvx"
at = [10, 20, 0]
facing = "east"

[[thing]]
name = "lamp"
model = "box.vvx"
at = [40, 30, 12]
light = { at = [0, 0, 3], radius = 30, color = "#ff8000" }

[[thing]]
name = "moon"
at = [5, 5, 30]
light = { radius = 40 }

[[place]]
name = "start"
at = [32, 10, 0]

[[area]]
name = "corner"
at = [0, 0, 0]
size = [16, 16, 20]
"##;

/// A game whose one level is `level` (a level of pieces: no `c.world`),
/// its player a thing of a kind without a model, `conf` added.
fn pieces_folder(name: &str, conf: &str, level: &str, main: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("pieces-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.things.hero = {{}}\n  c.player.kind = \"hero\"\n  c.saves = {{ slots = 2 }}\n{conf}end\n")).unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    std::fs::write(dir.join("box.vvx"), BOX).unwrap();
    std::fs::write(dir.join("levels/room.toml"), level).unwrap();
    dir
}

/// What a line of the console says.
fn said(g: &mut Session<LuaGame>, line: &str) -> String {
    g.eval(line).unwrap_or_else(|e| format!("{e:#}"))
}
