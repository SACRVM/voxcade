//! The command line's table: every command and every option it takes,
//! a person's first, and the help written from it (`usage_of`,
//! `all_help`, `short_help`). The parsers (`parse.rs`) take nothing it
//! does not list (`known`).

use anyhow::{Context, Result, bail};

use super::parse::seat_input;
use crate::headless;

/// Where an option stands in its command's help.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Group {
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
    pub(super) const ALL: [Group; 4] = [Group::Main, Group::Writes, Group::Looking, Group::Testing];

    pub(super) fn heading(self) -> &'static str {
        match self {
            Group::Main => "options:",
            Group::Writes => "changes written without a window:",
            Group::Looking => "for looking without a window (a picture; nothing is written):",
            Group::Testing => "for testing:",
        }
    }
}

/// One option of a command, as its parser takes it and its help says it.
pub(super) struct Opt {
    /// `--level`.
    pub(super) name: &'static str,
    /// What follows it (`FILE`, `NAME --by DEG`); empty for a switch, in
    /// brackets when it may be left out (`[PORT | internet]`).
    pub(super) value: &'static str,
    /// What it does.
    pub(super) about: &'static str,
    pub(super) group: Group,
}

pub(super) const fn opt(name: &'static str, value: &'static str, about: &'static str) -> Opt {
    Opt { name, value, about, group: Group::Main }
}

pub(super) const fn writes(name: &'static str, value: &'static str, about: &'static str) -> Opt {
    Opt { name, value, about, group: Group::Writes }
}

pub(super) const fn look(name: &'static str, value: &'static str, about: &'static str) -> Opt {
    Opt { name, value, about, group: Group::Looking }
}

pub(super) const fn trial(name: &'static str, value: &'static str, about: &'static str) -> Opt {
    Opt { name, value, about, group: Group::Testing }
}

/// A command: its name, what follows the name before the options (each a
/// form of its own), a line for `voxcade --help`, whether it is for
/// playing and making games (else for looking and checking without a
/// window), what it does, and its options.
pub(super) struct Command {
    pub(super) name: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) line: &'static str,
    pub(super) making: bool,
    pub(super) text: &'static str,
    pub(super) opts: &'static [Opt],
}

/// Old names of options, refused with the new one.
pub(super) const RENAMED: &[(&str, &str)] = &[("--turn", "--rotate"), ("--colour", "--color")];

pub(super) const INPUT_SCRIPT: &str = "\
INPUT SCRIPT: segments separated by ';', each 'keys:ticks', keys joined by
'+' from: left right up down, the game's own actions by name (`voxcade api`
and its conf.lua list them; More Blood: jump fire grenade strafe) and none.
  example: --input \"right+fire:40;none:20;up+jump:15\"
";

/// Every command the program answers to, a person's first.
pub(super) const COMMANDS: &[Command] = &[
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
pub(super) fn command(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// The option `flag` of `command` as its table has it; anything else is a
/// mistake (an old name says the new one).
pub(super) fn known(command: &'static Command, flag: &str) -> Result<&'static Opt> {
    if let Some((_, new)) = RENAMED.iter().find(|(old, _)| *old == flag) {
        bail!("{flag} is called {new} now");
    }
    let name = if seat_input(flag).is_some() { "--input2" } else { flag };
    command.opts.iter().find(|o| o.name == name).with_context(|| format!("unknown option {flag} (voxcade {} --help says what it takes)", command.name))
}

/// The help's width.
pub(super) const WIDTH: usize = 100;

/// `words` set into lines of at most `WIDTH` signs, the first line after
/// `first`, the others after `rest` spaces; a word longer than a line
/// stands alone.
pub(super) fn wrap<'a>(first: &str, words: impl IntoIterator<Item = &'a str>, rest: usize) -> String {
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
pub(super) fn head(o: &Opt) -> String {
    if o.value.is_empty() { o.name.to_string() } else { format!("{} {}", o.name, o.value) }
}

/// A command's usage without the input script's lines: its forms (the
/// options a person uses in them), what it does, then its options under
/// their headings.
pub(super) fn usage_body(c: &Command) -> String {
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
pub(super) fn takes_a_script(c: &Command) -> bool {
    c.opts.iter().any(|o| o.value == "SCRIPT")
}

/// A command's usage (`voxcade COMMAND --help`), the input script's lines
/// where it takes one. None for a word that is no command.
pub(super) fn usage_of(name: &str) -> Option<String> {
    let c = command(name)?;
    let mut out = usage_body(c);
    if takes_a_script(c) {
        out += &format!("\n{INPUT_SCRIPT}");
    }
    Some(out)
}

/// Everything at once (`voxcade help all`).
pub(super) fn all_help() -> String {
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
pub(super) fn short_help() -> String {
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
