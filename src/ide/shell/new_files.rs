//! What a plus at a heading makes: a small Lua file, scenario, model,
//! sound, song or level to change, its name typed in place.

use super::*;

impl Shell {
    /// Makes the file `typed` names under the heading `sort` (a file of
    /// logic, a scenario, a model, a sound or a song, each a small one to change)
    /// and opens it; its path, or why not (nothing is ever overwritten).
    pub fn make_file(&mut self, sort: &str, typed: &str) -> Result<String, String> {
        let path = new_file(&self.dir, sort, typed)?;
        let name = path.rsplit('/').next().and_then(|n| n.rsplit_once('.')).map_or("", |(n, _)| n);
        let ident: String = name.chars().map(|c| if c == '-' { '_' } else { c }).collect();
        let ident = if ident.starts_with(|c: char| c.is_ascii_digit()) { format!("m_{ident}") } else { ident };
        let full = self.dir.join(&path);
        let made = std::fs::create_dir_all(full.parent().unwrap_or(&self.dir)).map_err(anyhow::Error::from).and_then(|()| match sort {
            "Logic" => Ok(std::fs::write(&full, NEW_LUA.replace("IDENT", &ident).replace("NAME", name))?),
            "Scenarios" => Ok(std::fs::write(&full, NEW_SCENARIO)?),
            "Music" => Ok(std::fs::write(&full, NEW_SONG.replace("NAME", name))?),
            "Sounds" => Ok(std::fs::write(&full, new_sound(name))?),
            _ => crate::editor::frames::ModelFile::begun(&full).save().map(drop),
        });
        made.map_err(|e| format!("{path}: {e:#}"))?;
        self.rescan();
        self.open(&path);
        self.back_to_file();
        self.help = false;
        Ok(path)
    }

    /// Makes the level `typed` names in `levels/` and opens it in the level
    /// editor: a room of pieces as big as the game's camera shows (or a
    /// plain size), or a level of blocks with the blocks, the block size
    /// and the grid of the game's first level of blocks, its cells empty.
    pub fn make_level(&mut self, typed: &str, blocks: bool) -> Result<String, String> {
        let path = new_file(&self.dir, "Levels", typed)?;
        let level = if blocks {
            let first = self.level.clone().into_iter().chain(crate::editor::model::level_file(&self.dir, &self.cfg).ok()).find_map(|f| crate::grid::GridLevel::read(&self.dir.join(f)).ok().filter(|l| l.has_grid()));
            match first {
                Some(l) => crate::grid::GridLevel { blocks: l.blocks.clone(), ..crate::grid::GridLevel::new(l.n, l.size) },
                None => crate::grid::GridLevel::new(10, glam::IVec3::new(24, 2, 13)),
            }
        } else {
            let cam = &self.cfg.camera;
            let wide = cam.view_at_least.map_or(cam.view_width, |[w, _]| w);
            let round = |v: f32| ((v / 10.0).round() as i32 * 10).max(10);
            crate::grid::GridLevel::new(0, glam::IVec3::new(round(wide), round(wide * 0.6), round(wide * 0.4)))
        };
        let full = self.dir.join(&path);
        let made = std::fs::create_dir_all(full.parent().unwrap_or(&self.dir)).map_err(anyhow::Error::from).and_then(|()| Ok(std::fs::write(&full, level.to_text())?));
        made.map_err(|e| format!("{path}: {e:#}"))?;
        self.rescan();
        self.open(&path);
        self.back_to_file();
        self.help = false;
        self.edits.push(path.clone());
        Ok(path)
    }
}

/// A new scenario's file (the plus at SCENARIOS), to fill in.
pub(super) const NEW_SCENARIO: &str = "\
-- A scenario: a run begun late, to try what happens there without
-- playing to it first (docs/game-model.md, \"Scenarios and the console\").
-- PLAY SCENARIO above plays the game from it; voxcade shot, inspect and
-- run take --scenario with its file's name.
return {
  about = \"What it sets up, in a few words\",
  -- level = \"name\", place = \"name\",   -- where the run begins, if not the first level
  setup = function()
    -- The states as late in the game, with the game's own functions:
    -- player.state.score = 5000
  end,
}
";

/// A new Lua file (the plus at LOGIC): `NAME` is the file's name, `IDENT` the name as a Lua variable.
pub(super) const NEW_LUA: &str = "\
-- NAME.lua: a file of the game's code. main.lua takes it in while the game
-- starts, at its top (not inside a handler):
--   local IDENT = require(\"NAME\")
-- and gets back what this file returns (true if it returns nothing). The
-- files share main.lua's globals; saving one runs the game's code anew.
local IDENT = {}

function IDENT.hello()
  return \"hello from NAME\"
end

return IDENT
";

/// A new song (the plus at MUSIC): four bars over and over, a tune and a bass.
pub(super) const NEW_SONG: &str = "\
; NAME: a short song to change. Each line after a letter and : is a voice:
; notes c d e f g a b (+ sharp, - flat), a length (4 quarter, 8 eighth),
; r a rest, o4 the octave, l8 the default length, v12 the volume (0 to 15),
; @lead the instrument (an @ line above), | only marks a bar. PLAY SONG
; above plays it; HELP: A SONG IN MML. A game names it in conf.lua,
;   c.audio.music = { \"music/NAME.mml\" }
; and its code plays it by its name: music:play(\"NAME\").

#title NAME
#tempo 110

@lead triangle attack=0.02 decay=0.3 sustain=0.6 release=0.2 vibrato=0.006 vibrato_speed=5 volume=0.5
@bass triangle attack=0.01 decay=0.3 sustain=0.7 release=0.2 volume=0.6

A: @lead v12 q7 o4 l8 c e g e c e g e | a g f e d4 d4 | c e g e c e g e | f e d c g2
B: @bass v12 q7 o2 l4 c c c c | f f c c | c c c c | f f g g
";

/// A new sound (the plus at SOUNDS): a short blip to change, with the one
/// line saying what the file is.
pub(super) fn new_sound(name: &str) -> String {
    let blip = SoundDef::Synth(crate::audio::Synth { freq: 660.0, hold: 0.04, decay: 0.12, volume: 0.3, ..Default::default() });
    sound::write(&blip, &[format!("{name}: a sound, a few numbers a line; level:sound(\"{name}\", player) plays it.")])
}

/// A name being typed for a new file under a heading (a plus).
pub(super) struct Naming {
    pub(super) sort: String,
    pub(super) text: String,
    /// A new level: of blocks (else a room of pieces).
    pub(super) blocks: bool,
    /// Why the last Enter made nothing.
    pub(super) problem: Option<String>,
    /// Its field is to take the keys, once.
    pub(super) fresh: bool,
}

/// What a plus at a heading makes, and its hover text (`None`: no plus there).
pub(super) fn new_what(sort: &str) -> Option<&'static str> {
    Some(match sort {
        "Logic" => "New Lua file: code the game runs",
        "Scenarios" => "New scenario: a run begun late, to try what happens there",
        "Models" => "New model: a figure, prop or block, to shape in the editor",
        "Sounds" => "New sound: a blip to change",
        "Music" => "New song: a text to change and play",
        "Levels" => "New level",
        _ => return None,
    })
}

/// The file a name typed under the heading `sort` stands for, and its text
/// (a model's bytes are made by the editor's writer, not here): the name
/// lower case, spaces `_`, `.lua` and the like left off if typed; letters,
/// digits, `_` and `-` only. A plain sentence says why not.
pub(super) fn new_file(dir: &Path, sort: &str, typed: &str) -> Result<String, String> {
    let (folder, ext) = match sort {
        "Logic" => ("", "lua"),
        "Scenarios" => ("scenarios/", "lua"),
        "Models" => ("", "vvx"),
        "Sounds" => ("sounds/", "toml"),
        "Music" => ("music/", "mml"),
        "Levels" => ("levels/", "toml"),
        _ => return Err("Nothing is made here.".into()),
    };
    let name = typed.trim().to_lowercase().replace(' ', "_");
    let name = name.strip_suffix(&format!(".{ext}")).unwrap_or(&name);
    if name.is_empty() {
        return Err("Type a name first.".into());
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err("A name takes letters, digits, _ and - only.".into());
    }
    let path = format!("{folder}{name}.{ext}");
    if dir.join(&path).exists() {
        return Err(format!("{path} is already there."));
    }
    Ok(path)
}
