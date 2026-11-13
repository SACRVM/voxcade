//! The command line every game gets: run, shot, check, film, sounds,
//! music ... for game folders of one kind. The `voxcade` program is this
//! for Lua games; a game written in Rust calls `main` with its own type
//! (`examples/rust_digger`).
//!
//! One table, `COMMANDS`, says every command and every option it takes:
//! the parsers take nothing it does not list (`known`), and the help is
//! written from it, so the two cannot drift (tests
//! `the_help_says_every_option_the_parsers_take`,
//! `the_parsers_take_every_option_the_help_says`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::controls::PadStyle;
use crate::game::Game;
use crate::headless::Networked;
use crate::{app, headless, pads, replay};

/// Where an option stands in its command's help.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Group {
    /// What a person uses.
    Main,
    /// Changes written without a window (`edit --fill`).
    Writes,
    /// A picture without a window, nothing written (`--shot`).
    Looking,
    /// For tests, agents and other sessions.
    Testing,
}

impl Group {
    const ALL: [Group; 4] = [Group::Main, Group::Writes, Group::Looking, Group::Testing];

    fn heading(self) -> &'static str {
        match self {
            Group::Main => "options:",
            Group::Writes => "changes written without a window:",
            Group::Looking => "for looking without a window (a picture; nothing is written):",
            Group::Testing => "for testing:",
        }
    }
}

/// One option of a command, as its parser takes it and its help says it.
struct Opt {
    /// `--level`.
    name: &'static str,
    /// What follows it (`FILE`, `NAME --by DEG`); empty for a switch, in
    /// brackets when it may be left out (`[PORT | internet]`).
    value: &'static str,
    /// What it does.
    about: &'static str,
    group: Group,
}

const fn opt(name: &'static str, value: &'static str, about: &'static str) -> Opt {
    Opt { name, value, about, group: Group::Main }
}

const fn writes(name: &'static str, value: &'static str, about: &'static str) -> Opt {
    Opt { name, value, about, group: Group::Writes }
}

const fn look(name: &'static str, value: &'static str, about: &'static str) -> Opt {
    Opt { name, value, about, group: Group::Looking }
}

const fn trial(name: &'static str, value: &'static str, about: &'static str) -> Opt {
    Opt { name, value, about, group: Group::Testing }
}

/// A command: its name, what follows the name before the options (each a
/// form of its own), a line for `voxcade --help`, whether it is for
/// playing and making games (else for looking and checking without a
/// window), what it does, and its options.
struct Command {
    name: &'static str,
    args: &'static [&'static str],
    line: &'static str,
    making: bool,
    text: &'static str,
    opts: &'static [Opt],
}

/// Old names of options, refused with the new one.
const RENAMED: &[(&str, &str)] = &[("--turn", "--rotate"), ("--colour", "--color")];

const INPUT_SCRIPT: &str = "\
INPUT SCRIPT: segments separated by ';', each 'keys:ticks', keys joined by
'+' from: left right up down, the game's own actions by name (`voxcade api`
and its conf.lua list them; More Blood: jump fire grenade strafe) and none.
  example: --input \"right+fire:40;none:20;up+jump:15\"
";

/// Every command the program answers to, a person's first.
const COMMANDS: &[Command] = &[
    Command {
        name: "run",
        args: &["<game-dir>"],
        line: "play a game in a window",
        making: true,
        text: "opens the game in a window. Saving any file in the game folder reloads it while the game keeps running. Every run is recorded: runs of 10 s or more are kept in the config folder (the newest 20, see replays). The key left of 1 opens the console, a line of Lua between two ticks; it and a scenario are recorded with the run.",
        opts: &[
            opt("--maker", "", "the maker's keys, which a player of the game does not have: F5 plays it again on the files as they are, F9 puts the run being played or the last one in the game's replays/ folder, 1 to 9 pick the song (the IDE's PLAY has them always)"),
            opt("--scenario", "NAME", "begin every run there (scenarios/NAME.lua: a late situation set up in code), past the title"),
            opt("--do", "LINE", "run a line of Lua in the game before its first tick (again for more)"),
            opt("--host", "[PORT | internet]", "host a network game at once: on the LAN (port 26500 or the next free), or across the internet"),
            opt("--join", "ADDRESS[:PORT] | CODE | TICKET", "join a network game at once"),
            trial("--input", "SCRIPT", "player 1's keys from a script instead of the devices (INPUT SCRIPT below): a game tried hands-free"),
        ],
    },
    Command {
        name: "replay",
        args: &["<game-dir> <replay-file>"],
        line: "show a replay in a window",
        making: true,
        text: "shows a replay in a window; F5 plays it again, Escape quits.",
        opts: &[],
    },
    Command {
        name: "replays",
        args: &["<game-dir>"],
        line: "list the runs kept for a game",
        making: true,
        text: "lists the runs kept for a game (in the config folder; F9 in a window puts one in the game's replays/ folder).",
        opts: &[],
    },
    Command {
        name: "ide",
        args: &["[<game-dir>]"],
        line: "open the IDE (a window)",
        making: true,
        text: "opens the IDE on a game (docs/ide.md): its logic and assets on the left, the one thing open in the main area, the console with your shell below; without a game, its start page (games opened last, OPEN, NEW).",
        opts: &[
            look("--shot", "FILE", "draw the IDE as it opens into a PNG instead of a window"),
            look("--size", "WxH", "the picture's size (default 1920x1080)"),
            look("--open", "PATH", "that file open (help: HELP's list of every name, help:N its topic N; text:FILE a model as text)"),
            look("--console", "CMD", "CMD run in the console and waited for"),
            look("--play", "TICKS", "the game played in the main area, TICKS in (with --open: held behind the file)"),
            look("--find", "WORD", "the find row open over the text"),
            look("--replace", "TEXT", "with --find: the replace row too"),
            look("--search", "WORD", "the search through the game's files"),
            look("--hit", "N", "with --search: its line N chosen"),
            look("--journal", "FILE", "a text left in the open file's journal before the IDE opens it"),
            look("--upgraded", "", "a game for an older Voxcade: its upgrade's page as after UPGRADE run dry"),
            trial("--remote", "", "let another session type into the console and read it, and use the IDE's windows as a person does (`voxcade mcp --remote`); without it the IDE refuses them all"),
        ],
    },
    Command {
        name: "edit",
        args: &["<game-dir>"],
        line: "open the level or model editor (a window)",
        making: true,
        text: "opens the editor in a window: on the game's level (of voxel blocks, or a room of pieces: models and lights set down, places, areas), a block of it, or one of the game's models (any size, its frames; a figure of parts opens whole, its parts by what they hang on). A level whose pieces are turned off the quarter turns, in a game that sets things on the grid, asks before it opens: ALIGN ALL or DON'T OPEN.",
        opts: &[
            opt("--level", "FILE", "another level than the config's (levels/02_crypt.toml)"),
            opt("--block", "NAME", "a block of the level"),
            opt("--things", "", "the level's things, not its blocks"),
            opt("--model", "FILE", "a model of the game (its path in the game folder)"),
            writes("--clear", "X0,Y0,Z0 --to X1,Y1,Z1", "with --model: that box of the model's cells emptied (coordinates from its pivot, as the file's at)"),
            writes("--fill", "SIGN|#rrggbb --from X0,Y0,Z0 --to X1,Y1,Z1", "with --model: that box filled with a color (its sign, or a new one)"),
            writes("--frame", "N", "the frame --clear and --fill change (from 1; default the first)"),
            writes("--part", "NAME", "the part they change instead (in a picture: the part chosen)"),
            writes("--scatter", "N", "scatter rule N's pieces set into the level (TERRAIN's SCATTER; in a picture: pressed)"),
            writes("--dry", "", "with --clear, --fill or --scatter: say what would change, write nothing"),
            look("--shot", "FILE", "draw the editor as it opens into a PNG instead of a window"),
            look("--size", "WxH", "the picture's size (default 1920x1080)"),
            look("--view", "YAW,PITCH", "the camera turned and tilted (degrees)"),
            look("--background", "#rrggbb", "the view's background (default the dark one)"),
            look("--align", "", "ALIGN ALL answered to a level turned off the grid"),
            look("--tool", "WORD", "a tool chosen (pointer, add, erase, paint, box, sphere, select, fill ...)"),
            look("--held", "X,Y,Z", "BOX or SPHERE: a shape that big set under the mouse (the middle of the view), fresh (outlined)"),
            look("--stamp", "", "the shape set and no more fresh"),
            look("--shift", "", "as with Shift held (the tool is PICK)"),
            look("--click", "X,Y", "a click at a pixel of the picture; what it chooses printed and chosen"),
            look("--hover", "WORD", "a handle lit as under the mouse: arrow-x, ring-z, face-y, face-y-low ... (a part's joint's too)"),
            look("--handles", "move|rotate", "the chosen one's arrows (move) or rings (rotate)"),
            look("--menu", "NAME|floor|voxel|selection|part:NAME|keys", "a menu open: a thing's right-click menu, the floor's under the middle of the view, the voxel editor's, the selection's, a part's, or F1's page of the keys"),
            look("--slice", "x|y|z,LAYER", "SLICE across an axis at a layer"),
            look("--context", "LEVEL:NAME", "with --model: shaped in place, the level's other pieces faint round it"),
            look("--context-off", "", "CONTEXT switched off"),
            look("--game-view", "", "as the game's camera sees it (GAME VIEW)"),
            look("--parent", "NAME", "with --part: the part's parent set"),
            look("--joint", "X,Y,Z", "with --part: its joint set (from the pivot)"),
            look("--move-part", "NAME --by X,Y,Z", "a part moved (whole voxels into at, tenths into offset), what hangs on it along"),
            look("--move-joint", "NAME --by X,Y,Z", "a part's joint moved by halves"),
            look("--rotate-part", "NAME --by DEG", "a part rotated at rest about its joint (--about after it)"),
            look("--pose-part", "NAME --by DEG", "a part posed, POSE lit (--about after it)"),
            look("--add-part", "", "ADD PART on the part --part chose"),
            look("--merge-into", "NAME", "the part --part chose merged into another"),
            look("--mirror", "", "MIRROR on the part --part chose"),
            look("--select", "X0,Y0,Z0 --to X1,Y1,Z1", "the voxels in that box selected (from the pivot)"),
            look("--selection", "OP", "done at the selection, in turn: remove, paint, duplicate, flip-x|y|z, rotate-x|y|z, new-part, move-to:PART, copy, paste, move:X,Y,Z"),
            look("--marquee", "X0,Y0,X1,Y1", "SELECT's rectangle dragged there (pixels of the picture)"),
            look("--choose", "NAME", "an item of the level chosen (after the changes)"),
            look("--add", "MODEL|light|place|area", "one set down (--at where)"),
            look("--hang", "MODEL|light|place|area", "as --add, left on the mouse"),
            look("--at", "X,Y,Z", "where the --add, --move, --copy, --light or --add-light before it goes"),
            look("--move", "NAME", "an item moved (--at where)"),
            look("--copy", "NAME", "an item copied (--at where)"),
            look("--rotate", "NAME --by DEG", "an item rotated by degrees round (--about x tips it nose up, --about y rolls it, kept on its floor)"),
            look("--about", "x|y|z|rotation|pitch|roll", "the axis of the --rotate, --rotate-part or --pose-part before it"),
            look("--lay", "NAME", "an item laid down (a quarter about x)"),
            look("--remove", "NAME", "an item removed"),
            look("--rename", "NAME --to NEW", "an item renamed"),
            look("--solid", "NAME", "a piece's voxels hold bodies up"),
            look("--light", "NAME", "a piece's first light (--radius, --color, --at after it)"),
            look("--add-light", "NAME", "one more light on a piece (the same words after it)"),
            look("--radius", "N", "the light's radius"),
            look("--color", "#rrggbb", "the light's color"),
            look("--room-size", "X,Y,Z", "the room's size"),
            look("--choose-light", "K", "the chosen piece's light K (0 the first)"),
            look("--hide", "NAME", "an item hidden in the editor"),
            look("--picker", "", "ADD MODEL's pictures shown"),
            look("--add-menu", "", "ADD's menu open"),
            look("--level-settings", "", "LEVEL shown in the inspector"),
            look("--lights-off", "", "the view without the lights"),
            look("--terrain", "", "TERRAIN shown (a flat one where the level has none)"),
            look("--raise", "X,Y --by N [--radius R]", "a point of the land raised with the brush (radius 3 unless said)"),
            look("--shape", "X,Y smooth|linear|step", "a point of the land given a shape"),
            look("--material", "N", "TERRAIN's material N chosen (from 1)"),
        ],
    },
    Command {
        name: "import",
        args: &["<game-dir> <tiled-map.json>", "<game-dir> <model.vox>..."],
        line: "bring a Tiled map or MagicaVoxel models into a game",
        making: true,
        text: "brings a Tiled map into a game once, as a level of voxel blocks (c.world.blocks): every tile it uses a block, its picture drawn out through the depth, unless the game has made that block already (blocks/ID.vvx); each tile layer a row of cells, the top layer in front (a map of one layer: what holds in front, decoration behind); what a block is from its tile's class in the tileset (solid, oneway, ladder, hazard; none: decoration); the objects as things. Given MagicaVoxel models (.vox), it brings each into the game as a model of Voxcade's own (.vvx, text), beside the others in the game folder, its parts hanging as they did; the .vox is left alone. Name the .vvx in the game's Lua.",
        opts: &[
            opt("--file", "NAME", "a map: the level's file (default level.toml; levels/02_crypt.toml)"),
            opt("--over", "", "a map: over a level already there"),
            opt("--as", "PATH.vvx", "one model: another path in the game folder"),
        ],
    },
    Command {
        name: "upgrade",
        args: &["<game-dir>"],
        line: "bring a game made for an older Voxcade up to date",
        making: true,
        text: "brings a game made for an older Voxcade up to date: every step it needs (0.6.0: its models, figures and blocks become .vvx; its levels of blocks and maps .toml; its worlds and map prefabs .vvx too, their markers named in them instead of c.markers; the Lua naming the files follows), then c.voxcade in conf.lua says this version. It says what each step did. It commits nothing, and says first when the folder has changes git has not committed. A game for an older Voxcade does not open until then; one for a newer neither.",
        opts: &[opt("--dry", "", "only say what it would do")],
    },
    Command {
        name: "update",
        args: &[""],
        line: "look for a newer version; --install puts it in place",
        making: true,
        text: "looks whether a newer version is there (the feed at voxca.de; the launcher does it too, unless switched off there). VOXCADE_UPDATE_FEED names another feed (a file's path too).",
        opts: &[opt("--install", "", "fetch it, check it and put it in place in the folder the program lies in; what was there before goes to before-update")],
    },
    Command {
        name: "version",
        args: &[""],
        line: "say which version this is (also --version)",
        making: true,
        text: "says which version this is (also voxcade --version, voxcade -V).",
        opts: &[],
    },
    Command {
        name: "help",
        args: &["[all | COMMAND]"],
        line: "this list (also --help); help all: everything at once",
        making: true,
        text: "lists the commands; help all says everything, help COMMAND (or voxcade COMMAND --help) one command.",
        opts: &[],
    },
    Command {
        name: "shot",
        args: &["<game-dir>"],
        line: "play a game without a window and write a picture of it (PNG)",
        making: false,
        text: "simulates N ticks without a window and writes one rendered frame as PNG; it says which sounds the run made and how long the frame took.",
        opts: &[
            opt("--frames", "N", "ticks to play (default 90; a replay: to its end)"),
            opt("--input", "SCRIPT", "the player's keys (INPUT SCRIPT below)"),
            opt("--replay", "FILE", "a replay played instead of --input; says whether it runs as recorded"),
            opt("--record", "FILE", "the run written as a replay"),
            opt("--scenario", "NAME", "the run begun there, as for run"),
            opt("--do", "LINE", "a line of Lua before the first tick, as for run (again for more)"),
            opt("--out", "FILE", "where the PNG goes (default shots/shot.png)"),
            opt("--size", "WxH", "the picture's size (default 1920x1080)"),
            trial("--screen", "NAME", "a menu or screen instead of the game: one of the screens below"),
            trial("--input2", "SCRIPT", "a game of several: seat 2's keys (--input3 and on the seats after it)"),
            trial("--at", "X,Y", "the player starting at X,Y"),
            trial("--shadows", "high|medium|low", "the shadow quality, as in the settings"),
            trial("--pad", "xbox|nintendo|playstation", "the hints for a gamepad of that kind"),
        ],
    },
    Command {
        name: "check",
        args: &["<game-dir>"],
        line: "play every replay of a game and say whether each runs as recorded",
        making: false,
        text: "plays every replay in the game's replays/ folder without a window and says whether each runs as recorded (the regression check after changing the engine), the state its run ended in too; a replay recorded on other data names the files that changed, an older one without that state plays and says how many of its last ticks went unchecked, one with nothing to check it by fails. It goes on after a replay that cannot be played and ends with the mistakes. One of the options at most.",
        opts: &[
            opt("--update", "", "record their checks anew from their input, after a change to the game or engine that was meant (it runs the hidden-state guard and records no run it refuses)"),
            opt("--refit", "", "play them whatever data they were recorded on and, only if every one runs exactly as recorded, write the data's new fingerprint and its parts into them: after a change to the files meant to leave the game as it was"),
            opt("--rollback", "TICKS", "go back that many ticks again and again (every TICKS ticks) and play on, as rollback over a network would: each replay must still come out as recorded; says what keeping and bringing back the game cost"),
        ],
    },
    Command {
        name: "inspect",
        args: &["<game-dir>"],
        line: "play a game without a window and print how it stands (JSON)",
        making: false,
        text: "plays N ticks without a window and prints how the game stands as JSON: tick, outcome, score, the report, the sounds, the error that stopped it, the things and the script's state. Over the network VOXCADE_REACH=relayed (only through a relay) or direct (no relay: a LAN without the internet) tries the other ways, VOXCADE_RELAY=URL another relay, VOXCADE_MATCHMAKER=KEY another matchmaker.",
        opts: &[
            opt("--frames", "N", "ticks to play (default 60)"),
            opt("--input", "SCRIPT", "the player's keys (INPUT SCRIPT below)"),
            opt("--scenario", "NAME", "the run begun there, as for run"),
            opt("--do", "LINE", "a line of Lua before the first tick, as for run (again for more)"),
            opt("--record", "FILE", "the run written as a replay (a guest's equals the host's)"),
            opt("--host", "PORT | internet", "host a network game in real time (UDP) playing N ticks, then say farewell once its guests have them all; internet: across the internet (iroh), printing its ticket, then its code from our server, letting in every machine that asks"),
            opt("--join", "ADDRESS[:PORT] | CODE | TICKET | any", "join one as a guest: play the run so far through, take a seat and play --input until the host leaves"),
            trial("--input2", "SCRIPT", "a game of several: seat 2's keys (--input3 and on the seats after it)"),
            trial("--at", "X,Y", "the player starting at X,Y"),
        ],
    },
    Command {
        name: "scenarios",
        args: &["<game-dir>"],
        line: "list a game's scenarios (a run begun late)",
        making: false,
        text: "lists the game's scenarios (scenarios/NAME.lua) and what each is about; run, shot and inspect begin from one with --scenario.",
        opts: &[],
    },
    Command {
        name: "sheet",
        args: &["<game-dir> <model>"],
        line: "draw one of a game's models alone, as the game draws it",
        making: false,
        text: "draws one of the game's models (a kind's, or one in c.models: its file or name) alone as the game draws it, its light, camera angle and shading, into one PNG: five sides close up, every frame from the side, the five sides at the game's size.",
        opts: &[opt("--out", "FILE", "where the sheet goes (default shots/<model>_sheet.png)")],
    },
    Command {
        name: "film",
        args: &["<game-dir> <replay-file>"],
        line: "render a replay as a video or a GIF",
        making: false,
        text: "renders a replay as a video with its music and sounds (.mp4: H.264 and AAC, plays everywhere; .webm: VP9 and Opus; through ffmpeg) or as a GIF without sound (built in). Default: the whole replay, 1920x1080, 60 fps, shots/film.mp4; a GIF 480x270, 20 fps, 10 s.",
        opts: &[
            opt("--from", "S", "where it starts (seconds)"),
            opt("--to", "S", "where it ends (seconds)"),
            opt("--fps", "N", "frames a second (dividing 60)"),
            opt("--size", "WxH", "the picture's size"),
            opt("--max-mb", "N", "a video kept under that size (a chat's upload limit)"),
            opt("--out", "FILE.mp4|.webm|.gif", "where it goes"),
        ],
    },
    Command {
        name: "sounds",
        args: &["<game-dir>"],
        line: "write a game's sounds as a WAV file",
        making: false,
        text: "writes every sound of the game one after another as a WAV file, to listen to them without playing.",
        opts: &[
            opt("--out", "FILE", "where it goes (default shots/sounds.wav)"),
            opt("--only", "NAME", "just that one"),
            opt("--list", "", "list them instead: where each comes from (a file in sounds/, or built in), its length and peak"),
            opt("--to-files", "", "write the c.sounds of an older conf.lua as files in sounds/ and say which lines to take out (sounds are files now)"),
        ],
    },
    Command {
        name: "music",
        args: &["<game-dir> | <song.mml>"],
        line: "write a game's music as a WAV file",
        making: false,
        text: "writes the game's music (c.audio.music), or one song file, as a WAV file, once through.",
        opts: &[
            opt("--out", "FILE", "where it goes (default shots/music.wav)"),
            opt("--seconds", "N", "N seconds of it instead"),
        ],
    },
    Command {
        name: "kinds",
        args: &["<game-dir>"],
        line: "print a game's kinds of things and weapons with every number in effect",
        making: false,
        text: "prints the game's kinds of things and weapons with every number in effect, as conf.lua would write them: the defaults it leaves unset too (scaled to a kind's height), nothing hidden.",
        opts: &[],
    },
    Command {
        name: "api",
        args: &["[NAME]"],
        line: "list the Lua API; --lua writes the definitions for editors",
        making: false,
        text: "lists the Lua API (the names containing NAME).",
        opts: &[opt("--lua", "FILE", "the definitions for editors (LuaLS: completion and checks) written to FILE, lua/voxcade.lua beside the program")],
    },
    Command {
        name: "mcp",
        args: &[""],
        line: "serve the engine's tools to an agent over MCP",
        making: false,
        text: "serves the engine's tools to an agent over MCP on stdin/stdout: shot (a frame as an image), check, run, things, scenarios, api, and the IDE's tools passed on to the IDE running on the game. Claude Code: claude mcp add voxcade -- /path/to/voxcade mcp",
        opts: &[trial("--remote", "", "console_send and console_read (type into the IDE's console and read it) and windows, window_shot, pointer, scroll, keys (pictures of the IDE's windows, a synthetic mouse and keyboard), for a session testing the IDE (run the IDE with --remote too; never in the game's own .mcp.json)")],
    },
    Command {
        name: "rooms",
        args: &["<game-dir>"],
        line: "list a game's open rooms across the internet",
        making: false,
        text: "lists the game's open rooms across the internet at our matchmaker (../matchmaker): each one's code, title, host, seats taken, age. VOXCADE_RELAY, VOXCADE_RELAY_TOKEN and VOXCADE_MATCHMAKER name another.",
        opts: &[],
    },
    Command {
        name: "pads",
        args: &[""],
        line: "list the controllers and show what they send",
        making: false,
        text: "lists the controllers SDL sees (and whether it knows them as gamepads).",
        opts: &[opt("--seconds", "N", "then show what they send for N seconds")],
    },
    Command {
        name: "soak",
        args: &["[SEEDS]"],
        line: "play network games on simulated networks",
        making: false,
        text: "plays network games of a stub game on simulated networks, up to eight machines on bad and hard ones (default 100 seeds): every machine must play the same ticks.",
        opts: &[opt("--digest", "FILE", "the digest of every hash, the same on every system, written to FILE")],
    },
    Command {
        name: "nettest",
        args: &["[internet | relayed | direct]", "host [internet | relayed | direct]", "join TICKET [internet | relayed | direct]"],
        line: "try network play across the internet",
        making: false,
        text: "tries network play across the internet (iroh, docs/multiplayer.md stage 6, step 2): a host and its guests (default 3) in this process, reaching each other over the internet (default), only through a relay, or only directly (a LAN without the internet), 60 datagrams a second each way (default 20 s, 1000 bytes); says what got through and how late, the biggest datagram, how soon a machine that vanished is noticed and what it does to the main loop's frames, each pass or fail. nettest host and nettest join TICKET try two machines.",
        opts: &[
            opt("--guests", "N", "how many guests (default 3)"),
            opt("--seconds", "N", "how long (default 20)"),
            opt("--size", "BYTES", "a datagram's size (default 1000)"),
        ],
    },
];

/// The command named `name`.
fn command(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// The option `flag` of `command` as its table has it; anything else is a
/// mistake (an old name says the new one).
fn known(command: &'static Command, flag: &str) -> Result<&'static Opt> {
    if let Some((_, new)) = RENAMED.iter().find(|(old, _)| *old == flag) {
        bail!("{flag} is called {new} now");
    }
    let name = if seat_input(flag).is_some() { "--input2" } else { flag };
    command.opts.iter().find(|o| o.name == name).with_context(|| format!("unknown option {flag} (voxcade {} --help says what it takes)", command.name))
}

/// The help's width.
const WIDTH: usize = 100;

/// `words` set into lines of at most `WIDTH` signs, the first line after
/// `first`, the others after `rest` spaces; a word longer than a line
/// stands alone.
fn wrap<'a>(first: &str, words: impl IntoIterator<Item = &'a str>, rest: usize) -> String {
    let mut out = String::new();
    let mut line = first.to_string();
    let mut fresh = true;
    for word in words.into_iter().filter(|w| !w.is_empty()) {
        if !fresh && line.chars().count() + 1 + word.chars().count() > WIDTH {
            out += line.trim_end();
            out.push('\n');
            line = " ".repeat(rest);
            fresh = true;
        }
        if !fresh {
            line.push(' ');
        }
        line += word;
        fresh = false;
    }
    out += line.trim_end();
    out.push('\n');
    out
}

/// An option as its help shows it: `--level FILE`.
fn head(o: &Opt) -> String {
    if o.value.is_empty() { o.name.to_string() } else { format!("{} {}", o.name, o.value) }
}

/// A command's usage without the input script's lines: its forms (the
/// options a person uses in them), what it does, then its options under
/// their headings.
fn usage_body(c: &Command) -> String {
    let mut out = String::new();
    let main: Vec<String> = c.opts.iter().filter(|o| o.group == Group::Main).map(|o| format!("[{}]", head(o))).collect();
    for (i, form) in c.args.iter().enumerate() {
        let start = format!("voxcade {}", c.name);
        let mut words: Vec<&str> = form.split(' ').filter(|w| !w.is_empty()).collect();
        // A command of one form names a person's options in it; one of
        // several forms leaves them to the list below.
        if i == 0 && c.args.len() == 1 {
            words.extend(main.iter().map(String::as_str));
        }
        out += &wrap(&format!("{start} "), words, start.len() + 1);
    }
    out += "\n";
    out += &wrap("  ", c.text.split(' '), 2);
    let width = c.opts.iter().map(|o| head(o).chars().count()).filter(|&n| n <= 26).max().unwrap_or(0);
    for group in Group::ALL {
        let opts: Vec<&Opt> = c.opts.iter().filter(|o| o.group == group).collect();
        if opts.is_empty() {
            continue;
        }
        out += &format!("\n{}\n", group.heading());
        for o in opts {
            let head = head(o);
            if head.chars().count() > width {
                out += &format!("  {head}\n");
                out += &wrap(&" ".repeat(width + 4), o.about.split(' '), width + 4);
            } else {
                out += &wrap(&format!("  {head:<width$}  "), o.about.split(' '), width + 4);
            }
        }
    }
    if c.name == "shot" {
        out += "\nscreens (--screen NAME):\n";
        let width = headless::SCREENS.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
        for (screen, what) in headless::SCREENS {
            out += &wrap(&format!("  {screen:<width$}  "), what.split(' '), width + 4);
        }
    }
    out
}

/// Whether a command takes an input script.
fn takes_a_script(c: &Command) -> bool {
    c.opts.iter().any(|o| o.value == "SCRIPT")
}

/// A command's usage (`voxcade COMMAND --help`), the input script's lines
/// where it takes one. None for a word that is no command.
fn usage_of(name: &str) -> Option<String> {
    let c = command(name)?;
    let mut out = usage_body(c);
    if takes_a_script(c) {
        out += &format!("\n{INPUT_SCRIPT}");
    }
    Some(out)
}

/// Everything at once (`voxcade help all`).
fn all_help() -> String {
    let mut out = String::from("voxcade - a voxel arcade engine\n\nWithout anything after it: the launcher, to play a game or make one.\n");
    for c in COMMANDS {
        out += &format!("\n{}\n", "-".repeat(WIDTH));
        out += &usage_body(c);
    }
    out += &format!("\n{}\n{INPUT_SCRIPT}", "-".repeat(WIDTH));
    out
}

/// The short help: what `voxcade` alone does, and every command in a line,
/// a person's first.
fn short_help() -> String {
    let mut out = String::from(
        "voxcade - a voxel arcade engine\n\nWithout anything after it, voxcade opens the launcher (a window): play a game or make one.\nvoxcade COMMAND --help says how to use a command; voxcade help all says everything.\n",
    );
    let width = COMMANDS.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for (making, heading) in [(true, "TO PLAY AND MAKE GAMES:"), (false, "TO LOOK, CHECK AND TEST WITHOUT A WINDOW:")] {
        out += &format!("\n{heading}\n");
        for c in COMMANDS.iter().filter(|c| c.making == making) {
            out += &format!("  {:<width$}  {}\n", c.name, c.line);
        }
    }
    out
}

/// A command's flags, each one known to its table before it is read.
struct Flags<'a> {
    command: &'static Command,
    it: std::iter::Peekable<std::slice::Iter<'a, String>>,
}

impl<'a> Flags<'a> {
    fn new(name: &str, args: &'a [String]) -> Self {
        Flags { command: command(name).expect("a command of the table"), it: args.iter().peekable() }
    }

    /// The next flag and its option.
    fn next(&mut self) -> Result<Option<(&'a str, &'static Opt)>> {
        let Some(flag) = self.it.next() else { return Ok(None) };
        Ok(Some((flag.as_str(), known(self.command, flag)?)))
    }

    /// The value after `flag`.
    fn value(&mut self, flag: &str) -> Result<&'a String> {
        self.it.next().with_context(|| format!("{flag} needs a value"))
    }

    /// `WORD VALUE` after a flag's value (`--by 90`), else the usage.
    fn then(&mut self, word: &str, usage: impl Fn() -> String) -> Result<&'a String> {
        let found = self.it.next().filter(|b| *b == word);
        let value = self.it.next();
        found.and(value).with_context(usage)
    }
}

/// The arguments from `at` on.
fn tail(args: &[String], at: usize) -> &[String] {
    args.get(at..).unwrap_or(&[])
}

/// The game folder a command names (its first argument), not yet looked at.
fn named_dir(args: &[String]) -> Result<PathBuf> {
    args.get(1).map(PathBuf::from).context("missing <game-dir>")
}

/// How `check` plays the replays.
#[derive(Debug, PartialEq)]
enum CheckHow {
    AsRecorded,
    Update,
    Refit,
    Rollback(u64),
}

/// What `edit` does: draws a picture, writes a model's cells or a level's
/// scattered pieces without a window, or opens its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditMode {
    Shot,
    Cells,
    Scatter,
    Window,
}

/// What `nettest` tries.
enum NetTest {
    Here,
    Host,
    Join(String),
}

/// What `sounds` does.
#[derive(Clone, Copy)]
enum SoundsDo {
    Write,
    List,
    ToFiles,
}

/// What the command line asks for, read and not yet done.
enum Call {
    /// The launcher (nothing after the program's name), or a Rust game's
    /// own folder in a window.
    Launcher,
    Run { dir: PathBuf, begin: crate::session::Begin, net: Option<Networked>, keys: Option<String>, maker: bool },
    Replay { dir: PathBuf, file: PathBuf },
    Replays { dir: PathBuf },
    Ide { dir: Option<PathBuf>, opts: crate::ide::ShotOptions, shot: bool, remote: bool },
    Edit { dir: PathBuf, opts: Box<crate::editor::ShotOptions>, mode: EditMode },
    ImportMap { dir: PathBuf, source: PathBuf, file: String, over: bool },
    ImportModels { dir: PathBuf, sources: Vec<PathBuf>, into: Option<String> },
    Upgrade { dir: PathBuf, dry: bool },
    Update { install: bool },
    Version,
    Help(Option<String>),
    /// `voxcade COMMAND --help`.
    Usage(String),
    Shot { dir: PathBuf, opts: headless::ShotOptions },
    Check { dir: PathBuf, how: CheckHow },
    Inspect { dir: PathBuf, frames: u32, input: String, seats: Vec<(usize, String)>, at: Option<[f32; 2]>, begin: crate::session::Begin, net: Option<Networked>, record: Option<PathBuf> },
    Scenarios { dir: PathBuf },
    Sheet { dir: PathBuf, opts: headless::SheetOptions },
    Film { dir: PathBuf, opts: headless::FilmOptions },
    Sounds { dir: PathBuf, out: PathBuf, only: Option<String>, what: SoundsDo },
    /// A game's music, or a song file (`first` is either).
    Music { first: PathBuf, out: PathBuf, seconds: Option<f32> },
    Kinds { dir: PathBuf },
    Api { filter: Option<String>, lua: Option<PathBuf> },
    Mcp { remote: bool },
    Rooms { dir: PathBuf },
    Pads { seconds: f32 },
    Soak { seeds: u64, digest: Option<PathBuf> },
    Nettest { reach: crate::net::quic::Reach, guests: usize, seconds: u32, size: usize, what: NetTest },
    /// A word that is no command.
    Unknown(String),
}

impl Call {
    /// Whether doing it opens a window (where a mistake is said in a
    /// window too: nobody may be reading a terminal).
    fn opens_a_window(&self) -> bool {
        match self {
            Call::Launcher | Call::Run { .. } | Call::Replay { .. } => true,
            Call::Ide { shot, .. } => !shot,
            Call::Edit { mode, .. } => *mode == EditMode::Window,
            _ => false,
        }
    }
}

/// Reads the command line (the program's name left out) without doing
/// anything or looking at the disk.
fn parse(args: &[String]) -> Result<Call> {
    let Some(word) = args.first() else { return Ok(Call::Launcher) };
    // `voxcade COMMAND --help`: that command's usage.
    if matches!(args.get(1).map(String::as_str), Some("--help" | "-h")) && command(word).is_some() {
        return Ok(Call::Usage(word.clone()));
    }
    Ok(match word.as_str() {
        "run" => parse_run(args)?,
        "replay" => {
            let dir = named_dir(args)?;
            let file = PathBuf::from(args.get(2).context("missing the replay file")?);
            no_flags("replay", tail(args, 3))?;
            Call::Replay { dir, file }
        }
        "replays" => {
            no_flags("replays", tail(args, 2))?;
            Call::Replays { dir: named_dir(args)? }
        }
        "scenarios" => {
            no_flags("scenarios", tail(args, 2))?;
            Call::Scenarios { dir: named_dir(args)? }
        }
        "rooms" => {
            no_flags("rooms", tail(args, 2))?;
            Call::Rooms { dir: named_dir(args)? }
        }
        "kinds" => {
            no_flags("kinds", tail(args, 2))?;
            Call::Kinds { dir: named_dir(args)? }
        }
        "ide" => parse_ide(args)?,
        "edit" => {
            let dir = named_dir(args)?;
            let (opts, mode) = parse_edit(tail(args, 2))?;
            Call::Edit { dir, opts: Box::new(opts), mode }
        }
        "import" => parse_import(args)?,
        "upgrade" => {
            let dir = named_dir(args)?;
            let mut f = Flags::new("upgrade", tail(args, 2));
            let mut dry = false;
            while let Some((flag, _)) = f.next()? {
                match flag {
                    "--dry" => dry = true,
                    other => bail!("unknown option {other} (upgrade GAME [--dry])"),
                }
            }
            Call::Upgrade { dir, dry }
        }
        "update" => {
            let mut f = Flags::new("update", tail(args, 1));
            let mut install = false;
            while let Some((flag, _)) = f.next()? {
                match flag {
                    "--install" => install = true,
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Update { install }
        }
        "--version" | "-V" | "version" => {
            no_flags("version", tail(args, 1))?;
            Call::Version
        }
        "--help" | "-h" | "help" => Call::Help(args.get(1).cloned()),
        "shot" => {
            let dir = named_dir(args)?;
            Call::Shot { dir, opts: shot_options(tail(args, 2))? }
        }
        "check" => {
            let dir = named_dir(args)?;
            Call::Check { dir, how: parse_check(tail(args, 2))? }
        }
        "inspect" => parse_inspect(args)?,
        "sheet" => {
            let dir = named_dir(args)?;
            let model = args.get(2).filter(|a| !a.starts_with("--")).context("voxcade sheet <game-dir> <model>")?.clone();
            let stem = Path::new(&model).file_stem().map_or("model".into(), |s| s.to_string_lossy().to_string());
            let mut out = PathBuf::from(format!("shots/{stem}_sheet.png"));
            let mut f = Flags::new("sheet", tail(args, 3));
            while let Some((flag, _)) = f.next()? {
                let value = f.value(flag)?;
                match flag {
                    "--out" => out = PathBuf::from(value),
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Sheet { dir, opts: headless::SheetOptions { model, out } }
        }
        "film" => parse_film(args)?,
        "sounds" => {
            let dir = named_dir(args)?;
            let (mut out, mut only, mut what) = (PathBuf::from("shots/sounds.wav"), None, None);
            let mut f = Flags::new("sounds", tail(args, 2));
            while let Some((flag, _)) = f.next()? {
                match flag {
                    // The first of these says what is done.
                    "--list" => {
                        what.get_or_insert(SoundsDo::List);
                    }
                    "--to-files" => {
                        what.get_or_insert(SoundsDo::ToFiles);
                    }
                    flag => {
                        let value = f.value(flag)?;
                        match flag {
                            "--out" => out = PathBuf::from(value),
                            "--only" => only = Some(value.clone()),
                            other => bail!("unknown option {other}"),
                        }
                    }
                }
            }
            Call::Sounds { dir, out, only, what: what.unwrap_or(SoundsDo::Write) }
        }
        "music" => {
            let first = named_dir(args)?;
            let (mut out, mut seconds) = (PathBuf::from("shots/music.wav"), None);
            let mut f = Flags::new("music", tail(args, 2));
            while let Some((flag, _)) = f.next()? {
                let value = f.value(flag)?;
                match flag {
                    "--out" => out = PathBuf::from(value),
                    "--seconds" => seconds = Some(value.parse().context("--seconds")?),
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Music { first, out, seconds }
        }
        "api" => {
            let (mut filter, mut lua) = (None, None);
            let mut it = tail(args, 1).iter();
            while let Some(arg) = it.next() {
                match arg.as_str() {
                    "--lua" => lua = Some(PathBuf::from(it.next().context("--lua needs a file")?)),
                    flag if flag.starts_with('-') => {
                        known(command("api").expect("api"), flag)?;
                        bail!("{flag}: not taken here");
                    }
                    name => filter = Some(name.to_string()),
                }
            }
            Call::Api { filter, lua }
        }
        "mcp" => {
            let mut f = Flags::new("mcp", tail(args, 1));
            let mut remote = false;
            while let Some((flag, _)) = f.next()? {
                match flag {
                    "--remote" => remote = true,
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Mcp { remote }
        }
        "pads" => {
            let mut f = Flags::new("pads", tail(args, 1));
            let mut seconds = 0.0;
            while let Some((flag, _)) = f.next()? {
                let value = f.value(flag)?;
                match flag {
                    "--seconds" => seconds = value.parse().context("--seconds")?,
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Pads { seconds }
        }
        "soak" => {
            let (mut seeds, mut digest) = (100, None);
            let mut it = tail(args, 1).iter();
            while let Some(arg) = it.next() {
                match arg.as_str() {
                    "--digest" => digest = Some(PathBuf::from(it.next().context("--digest needs a file")?)),
                    flag if flag.starts_with("--") => {
                        known(command("soak").expect("soak"), flag)?;
                        bail!("{flag}: not taken here");
                    }
                    n => seeds = n.parse().with_context(|| format!("soak: '{n}' is no number of seeds"))?,
                }
            }
            Call::Soak { seeds, digest }
        }
        "nettest" => parse_nettest(args)?,
        _ => Call::Unknown(word.clone()),
    })
}

/// A command that takes no options: anything after its words is a mistake.
fn no_flags(name: &str, rest: &[String]) -> Result<()> {
    match Flags::new(name, rest).next()? {
        Some((flag, _)) => bail!("{flag}: in the table of {name}, not taken"),
        None => Ok(()),
    }
}

fn parse_run(args: &[String]) -> Result<Call> {
    let dir = named_dir(args)?;
    let (mut begin, mut net, mut keys, mut maker) = (crate::session::Begin::default(), None, None, false);
    let mut f = Flags::new("run", tail(args, 2));
    while let Some((flag, _)) = f.next()? {
        // --host with a port or without (26500 or the next free), or
        // across the internet.
        if flag == "--host" {
            if f.it.next_if(|p| *p == "internet").is_some() {
                net = Some(Networked::HostNet);
                continue;
            }
            let port = f.it.next_if(|p| p.parse::<u16>().is_ok()).map(|p| p.parse::<u16>().unwrap_or(crate::net::udp::PORT));
            net = Some(Networked::Host(port.unwrap_or(0)));
            continue;
        }
        // The maker's keys (F5, F9, 1 to 9): off for players.
        if flag == "--maker" {
            maker = true;
            continue;
        }
        let value = f.value(flag)?;
        match flag {
            "--join" => net = Some(Networked::Join(value.clone())),
            "--input" => keys = Some(value.clone()),
            flag if begin.flag(flag, value) => {}
            _ => bail!("unknown option {flag}"),
        }
    }
    Ok(Call::Run { dir, begin, net, keys, maker })
}

fn parse_ide(args: &[String]) -> Result<Call> {
    // Without a game: the start page.
    // (A game made for another Voxcade opens on its upgrade's page.)
    let dir = args.get(1).filter(|a| !a.starts_with("--")).map(PathBuf::from);
    let mut opts = crate::ide::ShotOptions { open: None, console: None, play: None, upgraded: false, journal: None, find: None, replace: None, search: None, hit: None, width: 1920, height: 1080, out: PathBuf::new() };
    let (mut shot, mut remote) = (false, false);
    let mut f = Flags::new("ide", tail(args, if dir.is_some() { 2 } else { 1 }));
    while let Some((flag, _)) = f.next()? {
        if flag == "--remote" {
            remote = true;
            continue;
        }
        if flag == "--upgraded" {
            opts.upgraded = true;
            continue;
        }
        let value = f.value(flag)?;
        match flag {
            "--open" => opts.open = Some(value.clone()),
            "--console" => opts.console = Some(value.clone()),
            "--journal" => opts.journal = Some(PathBuf::from(value)),
            "--find" => opts.find = Some(value.clone()),
            "--replace" => opts.replace = Some(value.clone()),
            "--search" => opts.search = Some(value.clone()),
            "--hit" => opts.hit = Some(value.parse().context("--hit N")?),
            "--play" => opts.play = Some(value.parse().context("--play TICKS")?),
            "--shot" => {
                opts.out = PathBuf::from(value);
                shot = true;
            }
            "--size" => {
                let (w, h) = value.split_once('x').context("--size WxH")?;
                (opts.width, opts.height) = (w.parse().context("--size width")?, h.parse().context("--size height")?);
            }
            other => bail!("unknown option {other}"),
        }
    }
    Ok(Call::Ide { dir, opts, shot, remote })
}

/// `X,Y,Z` after `flag`: three whole numbers.
fn three(flag: &str, value: &str) -> Result<glam::IVec3> {
    let n: Vec<i32> = value.split(',').map(|n| n.trim().parse()).collect::<Result<_, _>>().with_context(|| format!("{flag} X,Y,Z"))?;
    let [x, y, z] = n[..] else { bail!("{flag} X,Y,Z: three numbers") };
    Ok(glam::IVec3::new(x, y, z))
}

fn parse_edit(rest: &[String]) -> Result<(crate::editor::ShotOptions, EditMode)> {
    use crate::editor::ShotOp;
    let mut opts = crate::editor::ShotOptions { level: None, block: None, model: None, part: None, parent: None, joint: None, stamp: false, things: false, shift: false, align: false, tool: None, held: None, view: None, width: 1920, height: 1080, out: PathBuf::new(), ops: Vec::new(), choose: None, game_view: false, picker: false, lights_off: false, choose_light: None, menu: None, add_menu: false, level_settings: false, hover: None, turn_handles: false, part_moves: Vec::new(), hide: Vec::new(), click: None, slice: None, cells: Vec::new(), frame: None, dry: false, context: None, context_off: false, part_turns: Vec::new(), part_poses: Vec::new(), last_pose: false, add_part: false, merge_into: None, mirror: false, background: None, marquee: None, select: None, selection_ops: Vec::new(), terrain: false, land_ops: Vec::new(), material: None, scatter: Vec::new() };
    let mut shot = false;
    let mut f = Flags::new("edit", rest);
    while let Some((flag, opt)) = f.next()? {
        if opt.value.is_empty() {
            match flag {
                "--things" => opts.things = true,
                "--stamp" => opts.stamp = true,
                "--mirror" => opts.mirror = true,
                "--dry" => opts.dry = true,
                "--context-off" => opts.context_off = true,
                "--add-part" => opts.add_part = true,
                "--shift" => opts.shift = true,
                "--align" => opts.align = true,
                "--terrain" => opts.terrain = true,
                "--game-view" => opts.game_view = true,
                "--picker" => opts.picker = true,
                "--lights-off" => opts.lights_off = true,
                "--add-menu" => opts.add_menu = true,
                "--level-settings" => opts.level_settings = true,
                other => bail!("unknown option {other}"),
            }
            continue;
        }
        let value = f.value(flag)?;
        match flag {
            "--level" => opts.level = Some(value.clone()),
            "--tool" => opts.tool = Some(value.clone()),
            "--held" => {
                let n: Vec<i32> = value.split(',').map(|n| n.trim().parse()).collect::<Result<_, _>>().context("--held X,Y,Z")?;
                let [x, y, z] = n[..] else { bail!("--held X,Y,Z: three numbers") };
                opts.held = Some(glam::IVec3::new(x, y, z));
            }
            "--block" => opts.block = Some(value.clone()),
            // Among the pieces: changes tried in turn, nothing written.
            "--choose" => opts.choose = Some(value.clone()),
            "--add" => opts.ops.push(ShotOp::Add(value.clone(), None)),
            "--hang" => opts.ops.push(ShotOp::Hang(value.clone(), None)),
            "--move" => opts.ops.push(ShotOp::Move(value.clone(), None)),
            "--copy" => opts.ops.push(ShotOp::Copy(value.clone(), None)),
            "--remove" => opts.ops.push(ShotOp::Remove(value.clone())),
            "--light" => opts.ops.push(ShotOp::Light(value.clone(), None, None, None, false)),
            "--add-light" => opts.ops.push(ShotOp::Light(value.clone(), None, None, None, true)),
            "--solid" => opts.ops.push(ShotOp::Solid(value.clone())),
            "--room-size" => opts.ops.push(ShotOp::Size(three(flag, value)?)),
            "--at" => {
                let spot = three(flag, value)?;
                anyhow::ensure!(opts.ops.last_mut().is_some_and(|op| op.at(spot)), "--at follows --add, --move, --copy, --light or --add-light (where its light shines from)");
            }
            "--rotate" => {
                let by = f.then("--by", || "--rotate NAME --by DEGREES".into())?;
                opts.ops.push(ShotOp::Turn(value.clone(), by.parse().context("--by DEGREES")?, 2));
            }
            // (After --rotate-part or --pose-part: rotation, pitch or roll.)
            "--about" if ["rotation", "pitch", "roll"].contains(&value.as_str()) => {
                let axis = ["rotation", "pitch", "roll"].iter().position(|a| a == value).expect("one of them");
                match (opts.last_pose, opts.part_poses.last_mut(), opts.part_turns.last_mut()) {
                    (true, Some(t), _) | (false, _, Some(t)) => t.2 = axis,
                    _ => bail!("--about rotation|pitch|roll follows --rotate-part NAME --by DEG (or --pose-part)"),
                }
            }
            "--about" => match opts.ops.last_mut() {
                Some(ShotOp::Turn(_, _, axis)) => *axis = ["x", "y", "z"].iter().position(|a| a == value).context("--about x, y or z")?,
                _ => bail!("--about follows --rotate NAME --by DEGREES"),
            },
            "--lay" => opts.ops.push(ShotOp::Turn(value.clone(), 90.0, 0)),
            "--menu" => opts.menu = Some(value.clone()),
            "--move-part" | "--move-joint" => {
                let by = f.then("--by", || format!("{flag} NAME --by X,Y,Z"))?;
                let n: Vec<f32> = by.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().with_context(|| format!("{flag} NAME --by X,Y,Z"))?;
                let [x, y, z] = n[..] else { bail!("{flag} NAME --by X,Y,Z: three numbers") };
                opts.part_moves.push((value.clone(), glam::Vec3::new(x, y, z), flag == "--move-joint"));
            }
            "--merge-into" => opts.merge_into = Some(value.clone()),
            "--material" => opts.material = Some(value.parse().context("--material N: a terrain's material, from 1")?),
            "--scatter" => opts.scatter.push(value.parse().context("--scatter N: a terrain's scatter rule, from 1")?),
            "--raise" | "--shape" => {
                let n: Vec<i32> = value.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().with_context(|| format!("{flag} X,Y: a point's column and row"))?;
                let [x, y] = n[..] else { bail!("{flag} X,Y: two numbers") };
                let p = glam::IVec2::new(x, y);
                if flag == "--shape" {
                    let word = f.it.next().context("--shape X,Y smooth|linear|step")?;
                    let shape = crate::terrain::Shape::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(word)).context("--shape X,Y smooth|linear|step")?;
                    opts.land_ops.push(crate::editor::LandOp::Shape(p, shape));
                } else {
                    let by = f.then("--by", || "--raise X,Y --by N [--radius R]".into())?;
                    let by: i32 = by.parse().context("--raise X,Y --by N: whole voxels")?;
                    let radius = match f.it.peek() {
                        Some(r) if r.as_str() == "--radius" => {
                            f.it.next();
                            f.it.next().context("--radius R")?.parse().context("--radius R: points")?
                        }
                        _ => 3,
                    };
                    opts.land_ops.push(crate::editor::LandOp::Raise(p, by, radius));
                }
            }
            "--select" => {
                let to = f.then("--to", || "--select X0,Y0,Z0 --to X1,Y1,Z1".into())?;
                let corner = |v: &str| -> Result<glam::IVec3> {
                    let n: Vec<i32> = v.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--select X0,Y0,Z0 --to X1,Y1,Z1: whole numbers")?;
                    let [x, y, z] = n[..] else { bail!("--select X0,Y0,Z0 --to X1,Y1,Z1: three numbers each") };
                    Ok(glam::IVec3::new(x, y, z))
                };
                opts.select = Some((corner(value)?, corner(to)?));
            }
            "--selection" => opts.selection_ops.push(value.clone()),
            "--marquee" => {
                let n: Vec<f32> = value.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--marquee X0,Y0,X1,Y1")?;
                let [a, b, c, d] = n[..] else { bail!("--marquee X0,Y0,X1,Y1: four numbers") };
                opts.marquee = Some([a, b, c, d]);
            }
            "--background" => opts.background = Some(crate::editor::background::hex(value).context("--background #rrggbb")?),
            "--rotate-part" => {
                let by = f.then("--by", || "--rotate-part NAME --by DEG [--about rotation|pitch|roll]".into())?;
                let by: f32 = by.parse().context("--rotate-part NAME --by DEG: degrees")?;
                opts.part_turns.push((value.clone(), by, 0));
                opts.last_pose = false;
            }
            "--pose-part" => {
                let by = f.then("--by", || "--pose-part NAME --by DEG [--about rotation|pitch|roll]".into())?;
                let by: f32 = by.parse().context("--pose-part NAME --by DEG: degrees")?;
                opts.part_poses.push((value.clone(), by, 0));
                opts.last_pose = true;
            }
            "--hover" => opts.hover = Some(value.clone()),
            "--hide" => opts.hide.push(value.clone()),
            "--context" => {
                let (level, name) = value.rsplit_once(':').context("--context LEVEL:NAME (a level's file and a piece in it)")?;
                opts.context = Some((level.to_string(), name.to_string()));
            }
            "--frame" => opts.frame = Some(value.parse().context("--frame N (from 1)")?),
            "--clear" => {
                let to = f.then("--to", || "--clear X0,Y0,Z0 --to X1,Y1,Z1".into())?;
                opts.cells.push(crate::editor::frames::CellOp::Clear(three("--clear", value)?, three("--to", to)?));
            }
            "--fill" => {
                let usage = || "--fill SIGN|#rrggbb --from X0,Y0,Z0 --to X1,Y1,Z1".to_string();
                let from = f.then("--from", usage)?;
                let to = f.then("--to", usage)?;
                opts.cells.push(crate::editor::frames::CellOp::Fill(value.clone(), three("--from", from)?, three("--to", to)?));
            }
            "--slice" => {
                let (axis, at) = value.split_once(',').context("--slice x|y|z,LAYER")?;
                let axis = ["x", "y", "z"].iter().position(|a| *a == axis).context("--slice x|y|z,LAYER")?;
                opts.slice = Some((axis, at.trim().parse().context("--slice x|y|z,LAYER: a whole number")?));
            }
            "--click" => {
                let n: Vec<f32> = value.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--click X,Y: a pixel of the shot")?;
                let [x, y] = n[..] else { bail!("--click X,Y: two numbers") };
                opts.click = Some((x, y));
            }
            "--handles" => {
                opts.turn_handles = match value.as_str() {
                    "move" => false,
                    "rotate" => true,
                    _ => bail!("--handles move or rotate"),
                }
            }
            "--choose-light" => opts.choose_light = Some(value.parse().context("--choose-light K (0 the first)")?),
            "--rename" => {
                let to = f.then("--to", || "--rename NAME --to NEW".into())?;
                opts.ops.push(ShotOp::Rename(value.clone(), to.clone()));
            }
            "--radius" | "--color" => match opts.ops.last_mut() {
                Some(ShotOp::Light(_, radius, colour, _, _)) => {
                    if flag == "--radius" {
                        *radius = Some(value.parse().context("--radius N")?);
                    } else {
                        *colour = Some(value.clone());
                    }
                }
                _ => bail!("{flag} follows --light NAME or --add-light NAME"),
            },
            "--model" => opts.model = Some(value.clone()),
            "--part" => opts.part = Some(value.clone()),
            "--parent" => opts.parent = Some(value.clone()),
            "--joint" => {
                let n: Vec<f32> = value.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--joint X,Y,Z")?;
                let [x, y, z] = n[..] else { bail!("--joint X,Y,Z: three numbers") };
                opts.joint = Some(glam::Vec3::new(x, y, z));
            }
            "--shot" => {
                opts.out = PathBuf::from(value);
                shot = true;
            }
            "--view" => {
                let (yaw, pitch) = value.split_once(',').context("--view YAW,PITCH")?;
                opts.view = Some((yaw.trim().parse().context("--view yaw")?, pitch.trim().parse().context("--view pitch")?));
            }
            "--size" => {
                let (w, h) = value.split_once('x').context("--size WxH")?;
                (opts.width, opts.height) = (w.parse().context("--size width")?, h.parse().context("--size height")?);
            }
            other => bail!("unknown option {other}"),
        }
    }
    let mode = if shot {
        EditMode::Shot
    } else if !opts.cells.is_empty() {
        // Without a window: written as the editor writes.
        EditMode::Cells
    } else if !opts.scatter.is_empty() {
        EditMode::Scatter
    } else if opts.dry {
        bail!("--dry goes with --clear, --fill or --scatter: without them there is nothing to write")
    } else {
        EditMode::Window
    };
    Ok((opts, mode))
}

fn parse_import(args: &[String]) -> Result<Call> {
    let dir = named_dir(args)?;
    let source = PathBuf::from(args.get(2).context("import needs a Tiled map (.json) or MagicaVoxel models (.vox)")?);
    let c = command("import").expect("import");
    if source.extension().is_some_and(|e| e.eq_ignore_ascii_case("vox")) {
        // Models from MagicaVoxel: each a .vvx in the game, the source
        // left alone.
        let mut into = None;
        let mut sources = Vec::new();
        let mut it = tail(args, 2).iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--as" => into = Some(it.next().context("--as needs the model's path in the game (hero.vvx)")?.clone()),
                flag if flag.starts_with("--") => {
                    known(c, flag)?;
                    bail!("{flag} is for a Tiled map, not models");
                }
                file => sources.push(PathBuf::from(file)),
            }
        }
        anyhow::ensure!(into.is_none() || sources.len() == 1, "--as names the .vvx of one model");
        return Ok(Call::ImportModels { dir, sources, into });
    }
    let (mut file, mut over) = ("level.toml".to_string(), false);
    let mut f = Flags::new("import", tail(args, 3));
    while let Some((flag, _)) = f.next()? {
        match flag {
            "--file" => file = f.it.next().context("--file needs a name")?.clone(),
            "--over" => over = true,
            "--as" => bail!("--as is for models (.vox), not a Tiled map"),
            other => bail!("unknown option {other}"),
        }
    }
    anyhow::ensure!(file.ends_with(".toml"), "--file {file}: a level of the game is a .toml (levels/02_crypt.toml)");
    Ok(Call::ImportMap { dir, source, file, over })
}

fn parse_check(rest: &[String]) -> Result<CheckHow> {
    let mut f = Flags::new("check", rest);
    let how = match f.next()? {
        None => CheckHow::AsRecorded,
        Some(("--update", _)) => CheckHow::Update,
        Some(("--refit", _)) => CheckHow::Refit,
        Some(("--rollback", _)) => {
            let every = f.it.next().context("--rollback needs how many ticks to go back")?;
            CheckHow::Rollback(every.parse().context("--rollback: a number of ticks")?)
        }
        Some((other, _)) => bail!("unknown option {other}"),
    };
    if let Some((flag, _)) = f.next()? {
        bail!("check takes one of --update, --refit and --rollback TICKS ({flag} besides)");
    }
    Ok(how)
}

fn parse_inspect(args: &[String]) -> Result<Call> {
    let dir = named_dir(args)?;
    let (mut frames, mut input, mut at, mut begin) = (60, String::new(), None, crate::session::Begin::default());
    let (mut seats, mut net, mut record) = (Vec::new(), None, None);
    let mut f = Flags::new("inspect", tail(args, 2));
    while let Some((flag, _)) = f.next()? {
        let value = f.value(flag)?;
        match flag {
            flag if begin.flag(flag, value) => {}
            "--frames" => frames = value.parse().context("--frames")?,
            "--input" => input = value.clone(),
            "--host" if value == "internet" => net = Some(Networked::HostNet),
            "--host" => net = Some(Networked::Host(value.parse().context("--host PORT or internet")?)),
            "--join" => net = Some(Networked::Join(value.clone())),
            "--record" => record = Some(PathBuf::from(value)),
            flag if seat_input(flag).is_some() => seats.push((seat_input(flag).unwrap_or(0), value.clone())),
            "--at" => {
                let (x, y) = value.split_once(',').context("--at expects x,y")?;
                at = Some([x.trim().parse().context("--at x")?, y.trim().parse().context("--at y")?]);
            }
            other => bail!("unknown option {other}"),
        }
    }
    if net.is_some() {
        anyhow::ensure!(seats.is_empty() && at.is_none(), "over the network each machine plays its own keys (--input): no --input2 or --at");
    }
    Ok(Call::Inspect { dir, frames, input, seats, at, begin, net, record })
}

fn parse_film(args: &[String]) -> Result<Call> {
    let dir = named_dir(args)?;
    let replay = PathBuf::from(args.get(2).context("missing the replay file")?);
    let mut opts = headless::FilmOptions { replay, from: 0.0, to: None, fps: None, size: None, max_mb: None, out: PathBuf::from("shots/film.mp4") };
    let mut f = Flags::new("film", tail(args, 3));
    while let Some((flag, _)) = f.next()? {
        let value = f.value(flag)?;
        match flag {
            "--from" => opts.from = value.parse().context("--from")?,
            "--to" => opts.to = Some(value.parse().context("--to")?),
            "--fps" => opts.fps = Some(value.parse().context("--fps")?),
            "--max-mb" => opts.max_mb = Some(value.parse().context("--max-mb")?),
            "--size" => {
                let (w, h) = value.split_once('x').context("--size expects WxH")?;
                opts.size = Some((w.parse().context("--size width")?, h.parse().context("--size height")?));
            }
            "--out" => opts.out = PathBuf::from(value),
            other => bail!("unknown option {other}"),
        }
    }
    Ok(Call::Film { dir, opts })
}

// The iroh trial (docs/multiplayer.md, stage 6, step 2).
fn parse_nettest(args: &[String]) -> Result<Call> {
    let (mut reach, mut guests, mut seconds, mut size) = (crate::net::quic::Reach::Internet, 3, 20, 1000);
    let mut words = Vec::new();
    let mut it = tail(args, 1).iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--guests" => guests = it.next().context("--guests needs a number")?.parse().context("--guests")?,
            "--seconds" => seconds = it.next().context("--seconds needs a number")?.parse().context("--seconds")?,
            "--size" => size = it.next().context("--size needs a number")?.parse().context("--size")?,
            word @ ("internet" | "relayed" | "direct") => reach = crate::net::trial::reach_named(word)?,
            flag if flag.starts_with("--") => {
                known(command("nettest").expect("nettest"), flag)?;
                bail!("{flag}: not taken here");
            }
            word => words.push(word),
        }
    }
    let what = match words.as_slice() {
        [] => NetTest::Here,
        ["host"] => NetTest::Host,
        ["join", ticket] => NetTest::Join(ticket.to_string()),
        _ => bail!("nettest [internet | relayed | direct] [--guests N] [--seconds N] [--size BYTES], nettest host, nettest join TICKET"),
    };
    Ok(Call::Nettest { reach, guests, seconds, size, what })
}

/// This program's file. Rebuilt while running (the IDE, `voxcade mcp`),
/// Linux names the running one `voxcade (deleted)`: the new one at the
/// same place is meant.
pub fn program() -> std::io::Result<PathBuf> {
    Ok(rebuilt(std::env::current_exe()?))
}

/// Numbers as Lua writes and reads them ("1.5"), whatever language the
/// system speaks: the C library's locale for numbers is "C" from the start.
/// Lua's `tostring` and `string.format` go through it, and two machines
/// playing one game must write the same text (docs/multiplayer.md).
fn numbers_as_written() {
    // SAFETY: the program's first step, before any other thread runs.
    unsafe {
        libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
    }
}

/// The program at `exe`, or the one built anew in its place.
fn rebuilt(exe: PathBuf) -> PathBuf {
    match exe.to_str().and_then(|s| s.strip_suffix(" (deleted)")).map(PathBuf::from) {
        Some(new) if !exe.exists() && new.exists() => new,
        _ => exe,
    }
}

/// Reads the command line and does what it says, with game folders of
/// kind `G`; without arguments it runs `default` (a game's own folder) in
/// a window, or opens the launcher.
pub fn main<G: Game>(default: Option<&Path>) -> Result<()> {
    numbers_as_written();
    crate::desktop::name_app();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty()
        && let Some(dir) = default
    {
        return app::run::<G>(dir, None, Default::default(), None, None, false);
    }
    match parse(&args)? {
        // Started by a double click: the launcher (the IDE's start page).
        Call::Launcher => {
            crate::desktop::keep_entry();
            crate::ide::run::<G>(None, false)
        }
        Call::Run { dir, begin, net, keys, maker } => app::run::<G>(&opened(&dir)?, None, begin, net, keys.as_deref(), maker),
        Call::Replay { dir, file } => {
            let dir = opened(&dir)?;
            app::run::<G>(&dir, Some(replay::Replay::load(&file)?), Default::default(), None, None, false)
        }
        Call::Replays { dir } => replay::list::<G>(&opened(&dir)?),
        Call::Ide { dir, opts, shot, remote } => {
            let dir = dir.as_deref().map(game_dir).transpose()?;
            if shot { crate::ide::shot::<G>(dir.as_deref(), &opts) } else { crate::ide::run::<G>(dir.as_deref(), remote) }
        }
        Call::Edit { dir, opts, mode } => {
            let dir = opened(&dir)?;
            match mode {
                EditMode::Shot => crate::editor::shot::<G>(&dir, &opts),
                EditMode::Cells => crate::editor::change_cells(&dir, &opts),
                EditMode::Scatter => crate::editor::scatter_level::<G>(&dir, &opts),
                EditMode::Window => crate::editor::run::<G>(&dir, opts.level.as_deref(), opts.block.as_deref(), opts.things, opts.model.as_deref()),
            }
        }
        Call::ImportMap { dir, source, file, over } => {
            println!("{}", crate::tiled::import_into(&game_dir(&dir)?, &source, &file, over)?);
            Ok(())
        }
        Call::ImportModels { dir, sources, into } => import_models(&game_dir(&dir)?, &sources, into.as_deref()),
        Call::Upgrade { dir, dry } => {
            for line in crate::upgrade::upgrade(&game_dir(&dir)?, dry)? {
                println!("{line}");
            }
            Ok(())
        }
        Call::Update { install } => update(install),
        Call::Version => {
            println!("voxcade {}", crate::update::VERSION);
            Ok(())
        }
        Call::Help(what) => {
            match what.as_deref() {
                None => print!("{}", short_help()),
                Some("all") => print!("{}", all_help()),
                Some(command) => match usage_of(command) {
                    Some(usage) => print!("{usage}"),
                    None => bail!("{command} is no command of voxcade's (voxcade --help lists them)"),
                },
            }
            Ok(())
        }
        Call::Usage(command) => {
            print!("{}", usage_of(&command).unwrap_or_default());
            Ok(())
        }
        Call::Shot { dir, opts } => headless::shot::<G>(&opened(&dir)?, &opts),
        Call::Check { dir, how } => check::<G>(&opened(&dir)?, how),
        Call::Inspect { dir, frames, input, seats, at, begin, net, record } => {
            let dir = opened(&dir)?;
            match net {
                Some(net) => headless::inspect_net::<G>(&dir, frames, &input, &begin, &net, record.as_deref()),
                None => headless::inspect::<G>(&dir, frames, &input, &seats, at, &begin),
            }
        }
        Call::Scenarios { dir } => scenarios::<G>(&opened(&dir)?),
        Call::Sheet { dir, opts } => headless::sheet::<G>(&opened(&dir)?, &opts),
        Call::Film { dir, opts } => headless::film::<G>(&opened(&dir)?, &opts),
        Call::Sounds { dir, out, only, what } => {
            let dir = game_dir(&dir)?;
            match what {
                SoundsDo::List => headless::sound_list::<G>(&dir),
                SoundsDo::ToFiles => headless::sounds_to_files::<G>(&dir),
                SoundsDo::Write => headless::sounds::<G>(&dir, &out, only.as_deref()),
            }
        }
        Call::Music { first, out, seconds } => {
            // A song file by itself, or a game's first song.
            if first.is_file() && first.extension().is_some_and(|e| e == "mml") {
                headless::song(&first, None, &out, seconds)
            } else {
                headless::music::<G>(&game_dir(&first)?, &out, seconds)
            }
        }
        Call::Kinds { dir } => {
            let session = crate::session::Session::<G>::load(&opened(&dir)?)?;
            for line in session.game.numbers() {
                println!("{line}");
            }
            Ok(())
        }
        Call::Api { filter, lua } => api(filter.as_deref(), lua.as_deref()),
        Call::Mcp { remote } => crate::mcp::serve(remote),
        Call::Rooms { dir } => rooms::<G>(&opened(&dir)?),
        Call::Pads { seconds } => pads::pads(seconds),
        Call::Soak { seeds, digest } => soak(seeds, digest.as_deref()),
        Call::Nettest { reach, guests, seconds, size, what } => nettest(reach, guests, seconds, size, what),
        Call::Unknown(other) => {
            eprintln!("{other} is no command of voxcade's\n");
            eprint!("{}", short_help());
            std::process::exit(2);
        }
    }
}

/// `check`: every replay as recorded, recorded anew, refitted, or played
/// going back again and again; a replay that does not end the program
/// with 1.
fn check<G: Game>(dir: &Path, how: CheckHow) -> Result<()> {
    let fine = match how {
        CheckHow::AsRecorded => replay::check::<G>(dir, false)?,
        CheckHow::Update => replay::check::<G>(dir, true)?,
        CheckHow::Refit => replay::refit::<G>(dir)?,
        CheckHow::Rollback(every) => replay::check_going_back::<G>(dir, every)?,
    };
    if !fine {
        std::process::exit(1);
    }
    Ok(())
}

/// `scenarios`: each of the game's with what it is about.
fn scenarios<G: Game>(dir: &Path) -> Result<()> {
    let mut session = crate::session::Session::<G>::load(dir)?;
    let scenarios = session.scenarios()?;
    if scenarios.is_empty() {
        println!("no scenarios yet: scenarios/NAME.lua in the game folder (docs/game-model.md, \"Scenarios and the console\")");
    }
    for (name, about) in scenarios {
        println!("{name:<24} {about}");
    }
    Ok(())
}

/// `rooms`: the game's open rooms at the matchmaker.
fn rooms<G: Game>(dir: &Path) -> Result<()> {
    let session = crate::session::Session::<G>::load(dir)?;
    let (app, version) = crate::network::app_of(&session)?;
    anyhow::ensure!(crate::net::quic::matchmaker_known(), "no matchmaker known (VOXCADE_MATCHMAKER and VOXCADE_RELAY, or ours in src/net/quic.rs)");
    let quic = crate::net::quic::Quic::start(crate::net::quic::reach())?;
    quic.list(matchmaker_protocol::Search { app: app.clone(), version: version.clone(), ..Default::default() });
    let start = std::time::Instant::now();
    loop {
        for event in quic.drain() {
            match event {
                crate::net::quic::Event::Rooms(rooms) => {
                    println!("{} open room{} of {app} {version}", rooms.rooms.len(), if rooms.rooms.len() == 1 { "" } else { "s" });
                    for room in rooms.rooms {
                        let name = room.attributes.get("name").cloned().unwrap_or_default();
                        println!("{}  {:<20} {:<16} {}/{}  {} s", room.code, room.title, name, room.taken, room.seats, room.age);
                    }
                    return Ok(());
                }
                crate::net::quic::Event::Trouble(t) => bail!("{t}"),
                _ => {}
            }
        }
        anyhow::ensure!(start.elapsed() < std::time::Duration::from_secs(20), "the matchmaker did not answer");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// `import` of MagicaVoxel models: each a .vvx in the game.
fn import_models(dir: &Path, sources: &[PathBuf], into: Option<&str>) -> Result<()> {
    for vox in sources {
        let name = match into {
            Some(name) => PathBuf::from(name),
            None => PathBuf::from(vox.file_name().context("a model's file")?).with_extension(crate::vvx::EXTENSION),
        };
        anyhow::ensure!(crate::vvx::is_model(&name), "{}: a model of the game is a .vvx", name.display());
        println!("{}", crate::upgrade::bring_in(vox, &dir.join(name), false)?);
    }
    Ok(())
}

/// `update`: whether a newer version is there, and with `install` put in
/// place.
fn update(install: bool) -> Result<()> {
    match crate::update::look()? {
        None => println!("voxcade {} is the newest version", crate::update::VERSION),
        Some(release) if !install => println!("voxcade {} is there (this is {}): {}\nvoxcade update --install puts it in place", release.version, crate::update::VERSION, release.notes),
        Some(release) => {
            let dir = crate::update::install_dir().context("this program does not lie in a Voxcade folder (its games beside it): fetch the new version from voxca.de")?;
            crate::update::install(&release, &dir, &|_| {})?;
            println!("voxcade {} is in place in {}; what was there before lies in before-update", release.version, dir.display());
        }
    }
    Ok(())
}

/// `api`: the names containing `filter`, or the definitions for editors
/// written to `lua`.
fn api(filter: Option<&str>, lua: Option<&Path>) -> Result<()> {
    if let Some(file) = lua {
        if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, crate::lua::api::definitions())?;
        println!("wrote {}", file.display());
        return Ok(());
    }
    print!("{}", crate::lua::api::text(filter));
    Ok(())
}

/// The network's soak (docs/multiplayer.md, stage 4): machines on
/// simulated networks play the same ticks; the digest is compared between
/// systems.
fn soak(seeds: u64, digest: Option<&Path>) -> Result<()> {
    let started = std::time::Instant::now();
    let sum = format!("{:016x}", crate::net::soak::soak(seeds));
    println!("{seeds} seeds played alike in {:.1} s: digest {sum}", started.elapsed().as_secs_f32());
    if let Some(path) = digest {
        std::fs::write(path, format!("{sum}\n")).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// `nettest`: the iroh trial here, or one machine's side of it.
fn nettest(reach: crate::net::quic::Reach, guests: usize, seconds: u32, size: usize, what: NetTest) -> Result<()> {
    let fine = match what {
        NetTest::Here => crate::net::trial::run(reach, guests, seconds, size)?,
        NetTest::Host => return crate::net::trial::host(reach),
        NetTest::Join(ticket) => crate::net::trial::join(&ticket, reach, seconds, size)?,
    };
    if !fine {
        std::process::exit(1);
    }
    Ok(())
}

/// What went wrong in plain words, in the system's language (English,
/// German or French), with what the program itself said below.
pub fn plainly(error: &anyhow::Error, languages: &[String]) -> String {
    let said = format!("{error:#}");
    let lang = languages.iter().map(String::as_str).find(|l| matches!(*l, "en" | "de" | "fr")).unwrap_or("en");
    let graphics = ["no suitable GPU", "creating surface", "creating window", "request_device"].iter().any(|s| said.contains(s));
    let plain = match (graphics, lang) {
        (true, "de") => "Voxcade findet keine Grafik, mit der es zeichnen kann.\n\nEs braucht eine Grafikkarte mit Vulkan oder, unter Windows, DirectX 12. Oft hilft ein neuer Grafiktreiber. Mehr dazu: voxca.de/de/install.html",
        (true, "fr") => "Voxcade ne trouve pas de carte graphique avec laquelle dessiner.\n\nIl lui faut une carte graphique avec Vulkan ou, sous Windows, DirectX 12. Un pilote graphique plus récent aide souvent. Plus d'aide : voxca.de/install.html",
        (true, _) => "Voxcade found no graphics it can draw with.\n\nIt needs a graphics card with Vulkan or, on Windows, DirectX 12. A newer graphics driver often helps. More help: voxca.de/install.html",
        (false, "de") => "Voxcade ist auf einen Fehler gestoßen und muss aufhören.",
        (false, "fr") => "Voxcade a rencontré une erreur et doit s'arrêter.",
        (false, _) => "Voxcade ran into a mistake and has to stop.",
    };
    let it_said = match lang {
        "de" => "Das Programm meldet",
        "fr" => "Le programme dit",
        _ => "The program says",
    };
    format!("{plain}\n\n{it_said}: {said}")
}

/// Whether the command line `args` (the program's name left out) was a
/// window's (the launcher, a game, a replay, the IDE, an editor), where a
/// mistake is said in a window too. A mistake in the command line itself
/// was made at a terminal, and a command without a window (`edit --fill`,
/// `--clear`, `--scatter`, any `--shot`) never shows one: a script or an
/// agent would hang on it.
fn opens_a_window(args: &[String]) -> bool {
    parse(args).is_ok_and(|call| call.opens_a_window())
}

/// Says `error` in a window of its own, if the program was started for a
/// window (`opens_a_window`) and not to draw a picture or work without one.
pub fn tell_in_a_window(error: &anyhow::Error) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if opens_a_window(&args) {
        let message = plainly(error, &crate::settings::system_languages());
        let _ = sdl3::messagebox::show_simple_message_box(sdl3::messagebox::MessageBoxFlag::ERROR, "Voxcade", &message, None);
    }
}

/// The game folder `dir`, checked to be for this Voxcade
/// (`upgrade::opens`): one made for another is a plain mistake.
fn opened(dir: &Path) -> Result<PathBuf> {
    let dir = game_dir(dir)?;
    crate::upgrade::opens(&dir)?;
    Ok(dir)
}

/// The game folder `dir`, checked to be a folder.
fn game_dir(dir: &Path) -> Result<PathBuf> {
    if !dir.is_dir() {
        bail!("{} is no game folder", dir.display());
    }
    Ok(dir.to_path_buf())
}

/// The seat an `--inputN` names (`--input2`: seat 2), if it is one.
fn seat_input(flag: &str) -> Option<usize> {
    flag.strip_prefix("--input")?.parse().ok().filter(|&n| n >= 2)
}

fn shot_options(args: &[String]) -> Result<headless::ShotOptions> {
    let mut opts = headless::ShotOptions {
        frames: None,
        replay: None,
        record: None,
        screen: None,
        input: String::new(),
        seats: Vec::new(),
        at: None,
        shadows: Default::default(),
        pad: None,
        width: 1920,
        height: 1080,
        out: PathBuf::from("shots/shot.png"),
        begin: Default::default(),
    };
    let mut f = Flags::new("shot", args);
    while let Some((flag, _)) = f.next()? {
        let value = f.value(flag)?;
        match flag {
            flag if opts.begin.flag(flag, value) => {}
            "--frames" => opts.frames = Some(value.parse().context("--frames")?),
            "--replay" => opts.replay = Some(PathBuf::from(value)),
            "--record" => opts.record = Some(PathBuf::from(value)),
            "--input" => opts.input = value.clone(),
            flag if seat_input(flag).is_some() => opts.seats.push((seat_input(flag).unwrap_or(0), value.clone())),
            "--screen" => {
                let names = || headless::SCREENS.iter().map(|(s, _)| *s).collect::<Vec<_>>().join(", ");
                anyhow::ensure!(headless::SCREENS.iter().any(|(s, _)| s == value), "no screen '{value}' ({})", names());
                opts.screen = Some(value.clone());
            }
            "--at" => {
                let (x, y) = value.split_once(',').context("--at expects x,y")?;
                opts.at = Some([x.trim().parse().context("--at x")?, y.trim().parse().context("--at y")?]);
            }
            "--out" => opts.out = PathBuf::from(value),
            "--shadows" => {
                opts.shadows = toml::Value::String(value.clone()).try_into().context("--shadows: high, medium or low")?;
            }
            "--pad" => opts.pad = Some(PadStyle::named(value).context("--pad: xbox, nintendo or playstation")?),
            "--size" => {
                let (w, h) = value.split_once('x').context("--size expects WxH")?;
                opts.width = w.parse().context("--size width")?;
                opts.height = h.parse().context("--size height")?;
            }
            other => bail!("unknown option {other}"),
        }
    }
    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(|w| w.to_string()).collect()
    }

    /// What follows each command's name before its options in the tests:
    /// a sample of its forms' words.
    fn sample(command: &str) -> &'static [&'static str] {
        match command {
            "replay" | "film" => &["g", "r.toml"],
            "sheet" => &["g", "hero"],
            "import" => &["g", "map.json"],
            "update" | "version" | "help" | "api" | "mcp" | "pads" | "soak" | "nettest" => &[],
            _ => &["g"],
        }
    }

    /// The short help lists every command the program answers to, each
    /// with a line, a person's first; each has its usage (`voxcade COMMAND
    /// --help`), and every word of the table is a command the parser takes.
    #[test]
    fn the_help_lists_every_command() {
        let help = short_help();
        assert!(help.contains("Without anything after it, voxcade opens the launcher (a window)"), "{help}");
        assert!(help.find("TO PLAY AND MAKE GAMES").unwrap() < help.find("\n  run ").unwrap());
        assert!(help.find("\n  run ").unwrap() < help.find("TO LOOK, CHECK").unwrap() && help.find("TO LOOK, CHECK").unwrap() < help.find("\n  shot ").unwrap());
        for c in COMMANDS {
            assert!(help.contains(&format!("\n  {} ", c.name)), "{} is missing from voxcade --help", c.name);
            let usage = usage_of(c.name).unwrap_or_else(|| panic!("{} has no usage", c.name));
            assert!(usage.starts_with(&format!("voxcade {}\n", c.name)) || usage.starts_with(&format!("voxcade {} ", c.name)), "{}: {usage}", c.name);
            let mut line = vec![c.name];
            line.extend(sample(c.name));
            assert!(!matches!(parse(&words(&line)), Ok(Call::Unknown(_))), "{} is in the table and not taken", c.name);
        }
        assert!(matches!(parse(&words(&["nothing"])), Ok(Call::Unknown(_))));
        assert!(matches!(parse(&words(&["shot", "--help"])), Ok(Call::Usage(c)) if c == "shot"));
        assert!(matches!(parse(&[]), Ok(Call::Launcher)));
        let shot = usage_of("shot").unwrap();
        assert!(shot.contains("simulates N ticks") && shot.contains("INPUT SCRIPT") && !shot.contains("voxcade check"), "{shot}");
        assert!(usage_of("replay").unwrap().lines().all(|l| !l.contains("voxcade replays")));
        assert!(usage_of("nothing").is_none());
        let all = all_help();
        assert!(COMMANDS.iter().all(|c| all.contains(&usage_body(c))) && all.matches("INPUT SCRIPT:").count() == 1, "{all}");
        assert!(all.lines().all(|l| l.chars().count() <= WIDTH), "a line of help all is wider than {WIDTH}");
    }

    /// Every option a parser takes is in its command's help, with what it
    /// takes, and under its heading: what a person uses first, the
    /// pictures' and tests' options after it. Every screen of `shot
    /// --screen` is listed, and the parser takes only those.
    #[test]
    fn the_help_says_every_option_the_parsers_take() {
        for c in COMMANDS {
            let usage = usage_of(c.name).unwrap();
            let lines: Vec<&str> = usage.lines().collect();
            let mut last = 0;
            for o in c.opts {
                let head = head(o);
                let at = lines.iter().position(|l| *l == format!("  {head}") || l.starts_with(&format!("  {head}  "))).unwrap_or_else(|| panic!("{} {}: not in its help:\n{usage}", c.name, o.name));
                let heading = lines[..at].iter().rev().find(|l| !l.starts_with(' ') && l.ends_with(':'));
                assert_eq!(heading, Some(&o.group.heading()), "{} {}: under its heading", c.name, o.name);
                last = last.max(Group::ALL.iter().position(|g| *g == o.group).unwrap());
                assert_eq!(Group::ALL.iter().position(|g| *g == o.group).unwrap(), last, "{} {}: the groups in order, a person's first", c.name, o.name);
            }
            // Anything else is refused, saying so.
            let mut line = vec![c.name];
            line.extend(sample(c.name));
            line.push("--no-such-option");
            if !matches!(c.name, "help") {
                let said = parse(&words(&line)).err().map(|e| format!("{e:#}")).unwrap_or_default();
                assert!(said.starts_with("unknown option --no-such-option"), "{}: {said}", c.name);
            }
        }
        let shot = usage_of("shot").unwrap();
        for (screen, _) in headless::SCREENS {
            assert!(shot.contains(&format!("\n  {screen} ")), "{screen} is not in shot's help");
            assert!(parse(&words(&["shot", "g", "--screen", screen])).is_ok(), "{screen}");
            assert!(headless::screen_menu(Some(screen), "Game", 2, true).is_ok(), "{screen}: not begun");
        }
        assert!(parse(&words(&["shot", "g", "--screen", "nowhere"])).is_err());
        // The edit's synopsis is what a person uses: no picture's options.
        let edit = usage_of("edit").unwrap();
        let synopsis = &edit[..edit.find("\n\n").unwrap()];
        assert!(synopsis.contains("[--level FILE]") && !synopsis.contains("--hover") && !synopsis.contains("--shot"), "{synopsis}");
        assert!(!usage_of("shot").unwrap()[..shot.find("\n\n").unwrap()].contains("--screen"));
    }

    /// Every option the help says is one its parser takes: a value where
    /// the help says one, none for a switch (a parser that had no arm for
    /// it would call it unknown).
    #[test]
    fn the_parsers_take_every_option_the_help_says() {
        for c in COMMANDS {
            for o in c.opts {
                let mut line = vec![c.name];
                line.extend(sample(c.name));
                line.push(o.name);
                let alone = parse(&words(&line)).err().map(|e| format!("{e:#}"));
                if o.value.is_empty() {
                    let said = alone.unwrap_or_default();
                    assert!(!said.contains("unknown option") && !said.contains("needs a value"), "{} {}: {said}", c.name, o.name);
                    continue;
                }
                if !o.value.starts_with('[') {
                    assert!(alone.is_some(), "{} {}: taken without its value", c.name, o.name);
                }
                line.push("1");
                let said = parse(&words(&line)).err().map(|e| format!("{e:#}")).unwrap_or_default();
                assert!(!said.starts_with(&format!("unknown option {}", o.name)), "{} {}: {said}", c.name, o.name);
            }
        }
        // Old names say the new one.
        let said = format!("{:#}", parse(&words(&["edit", "g", "--turn", "x", "--by", "90"])).err().unwrap());
        assert_eq!(said, "--turn is called --rotate now");
    }

    /// A mistake is said in a window only where the command opens one: a
    /// command that writes or draws without a window says it on the
    /// terminal (a modal box would hang a script or an agent), and so does
    /// a mistake in the command line itself.
    #[test]
    fn only_a_window_says_a_mistake_in_a_window() {
        for line in [
            &["edit", "g", "--model", "m.vvx", "--fill", "a", "--from", "0,0,0", "--to", "1,1,1"][..],
            &["edit", "g", "--model", "m.vvx", "--clear", "0,0,0", "--to", "1,1,1", "--dry"],
            &["edit", "g", "--level", "levels/a.toml", "--scatter", "1"],
            &["edit", "g", "--level", "levels/a.toml", "--scatter", "1", "--dry"],
            &["edit", "g", "--dry"],
            &["edit", "g", "--shot", "x.png"],
            &["edit", "g", "--no-such-option"],
            &["edit"],
            &["run", "g", "--no-such-option"],
            &["ide", "g", "--shot", "x.png"],
            &["shot", "g"],
            &["check", "g"],
            &["mcp"],
        ] {
            assert!(!opens_a_window(&words(line)), "{line:?}");
        }
        for line in [&[][..], &["run", "g"], &["replay", "g", "r.toml"], &["edit", "g"], &["edit", "g", "--model", "m.vvx"], &["ide"], &["ide", "g"]] {
            assert!(opens_a_window(&words(line)), "{line:?}");
        }
    }

    #[test]
    fn a_mistake_is_said_plainly_in_the_systems_language() {
        let none = anyhow::anyhow!("no suitable GPU");
        assert!(plainly(&none, &["de".into()]).starts_with("Voxcade findet keine Grafik"));
        assert!(plainly(&none, &["ja".into(), "fr".into()]).contains("carte graphique"));
        let other = anyhow::anyhow!("games/x is no game folder");
        let said = plainly(&other, &[]);
        assert!(said.starts_with("Voxcade ran into a mistake") && said.ends_with("The program says: games/x is no game folder"), "{said}");
    }

    /// A program rebuilt while it runs is found in its place.
    #[test]
    fn a_rebuilt_program_is_found_in_its_place() {
        let dir = crate::scratch::Scratch::new("rebuilt");
        std::fs::create_dir_all(&dir).unwrap();
        let new = dir.join("voxcade");
        let gone = dir.join("voxcade (deleted)");
        assert_eq!(rebuilt(gone.clone()), gone, "nothing in its place yet");
        std::fs::write(&new, b"").unwrap();
        assert_eq!(rebuilt(gone), new);
        assert_eq!(rebuilt(new.clone()), new);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
