# The Voxcade IDE (phase 2)

The user's vision of the IDE (2026-10-16) and the research on how to build
it, done the same day while step 4 (Lua) was planned. The vision replaces
the earlier idea of separate editors that feel like a game (`engine.md`,
"Editors"). The estimates are a research result, not a promise.

## The vision

- A very stripped-down VS Code without clutter or distraction: "less is
  more", "I am making a game, not learning an IDE". Getting in must be
  super simple.
- On the left only two areas: **Logic** (the Lua code) and **Assets**
  (models, levels, height and material images, sounds, music).
- No open tabs: the main area shows one thing. The IDE remembers what was
  open and where you were, above all unsaved work.
- One save for everything, and it happens by itself; play and build save
  too.
- Editors built in: a voxel editor, an image editor for the height and
  material maps, and a level editor that puts everything together.
- Play and Build, nothing more.
- A console running Claude Code (or another agent), with an MCP server of
  the IDE, so the agent sees what the user sees and can show things.
- Lua only. Whoever writes Rust uses the library directly and needs no IDE.

## In short

- **Doable, but not on the side.** The leanest version of the whole vision
  costs roughly 9 to 12 person-months (one developer with agents); a
  polished one about 30 to 38. A skeleton (code, a terminal with Claude
  Code, MCP, play and build, assets only as previews) takes 3 to 4.
- **The engine's view decides, not the code editor.** Three of the four
  editors (voxels, height and material maps, levels) are 3D voxel views.
  They should draw with the engine's renderer: the same light,
  `model_shading` and shadows. Only a native app gets that without a
  detour: webview shells (Tauri/wry, VS Code/Electron) cannot embed a
  native wgpu surface on Linux/Wayland (wry offers child webviews only on
  macOS, Windows and Linux/X11, and Wayland has no protocol for it).
- **Recommended: a native app on the engine itself.** SDL3, wgpu and egui
  for the lean, VS Code-like shell; the voxel UI for the playful tools; a
  Lua editor of its own speaking LSP with LuaLS; a terminal from
  `alacritty_terminal` and `portable-pty`; the MCP server `voxcade mcp`.
  The expensive and risky parts are the text editor and the terminal; both
  stay bounded (Lua scripts are small, there are no tabs, and the agent
  writes much of the code).
- **Step 4 laid the foundation** (built 2026-10-16, see the end): Lua state
  in snapshots, errors that halt the game, one source for the API,
  `conf.lua`, and `voxcade mcp` without a window.

## 1. What the IDE must hold

| Part | What | In the engine |
|---|---|---|
| Navigation | Only Logic (Lua) and Assets on the left. No tabs: the main area shows one thing; a back/forward history instead (go to definition jumps, Alt+Left returns). | – |
| Memory | Per file: cursor, scroll, selection, undo. Per asset: camera, selection, tool. Unsaved buffers go crash-safe into a journal every 1 to 2 s. All of it in the user's config folder, never the game folder, like the settings. | `src/settings.rs` |
| One save | A buffer goes to the journal at once, into the game's files after a short pause, always on play and build. The engine must take half-written Lua: the last good version keeps running, the error shows. | hot reload; errors halt (step 4) |
| Lua editor | Highlighting, diagnostics, completion, hover docs, signature help, go to definition, references, rename, search in the project, formatting: all through LSP from LuaLS. | `lua/voxcade.lua`, `.luarc.json` (step 4) |
| Voxel editor | Set, delete, paint, box, fill, mirror, the palette (glass, metal, rough, glow on a colour), frames with onion skin, a preview in the engine's light. | `.vvx`, text (`src/vvx.rs`) |
| Image editor (maps) | Raise, lower, smooth, flatten the height; paint material. Best right in the 3D level, the 2D image as a second view. | 8-bit grey = height in voxels (`src/map.rs`) |
| Level editor | Ground, prefabs (quarter turns), things and markers by kind; writes `map.json`. | mouse and picking missing (phase 3 so far) |
| Play | The engine's view in the main area, starting at the spot being edited. | offscreen rendering (`headless.rs`), `place_player` |
| Build | One file per system, LÖVE's way: the ready runtime and the zipped game. Lua only, so no compiler for the user, and every system builds for all. | CI builds the runtimes |
| Terminal and agent | A PTY and a terminal emulator running Claude Code (or Codex, Gemini CLI). | – |
| The IDE's MCP | The agent sees what the user sees and can show things. | `voxcade mcp` (step 4) |

A sound is a file of the game, `sounds/NAME.toml` (a few numbers a line,
in words shared with MML's instruments: `src/audio/words.rs`; no
recordings, everything in a game is code or text); over its text the sound maker (`src/ide/maker.rs`):
a kind of sound to pick, a dice that only gives good ones, a few named
faders, all writing the text. Songs open as text (MML) with a play key. A library of background songs ships with
the editor for games that need only something simple (`music/`, made by
`tools/compose.py` from chord progressions and moods); a "song from a
mood" button would move that tool to Rust, so users need no Python.

## 2. How others do it

| Tool | How it is built | Lesson |
|---|---|---|
| Unity | 4,412 full-time staff at the end of 2025 (10-K). Code is written in external IDEs. | Even Unity sends you out to code. |
| Godot | The editor is a Godot project with its own UI, no GTK or Qt. Script editor and debugger built in, LSP (6005) and DAP (6006) for external editors. The running game is embedded only since 4.4: window tricks on Windows and Linux, on macOS later by passing the framebuffer. 10 paid engine developers in 2024. | Its own UI on the engine carries. Embedding the game is delicate even for Godot. |
| Defold | Clojure/JavaFX, the code editor a canvas. A LuaLS fork built in (Luacheck, F12), a Lua debugger too. A team of about 6 in 2023, 20 at most. | The built-in code editor stays disputed (forum: "should drop the code editor"). |
| Roblox Studio | Luau, everything inside. Its MCP server reads and changes scripts, runs Luau, starts and stops play, captures the screen, sends keys and mouse. | Our MCP pattern, built by a big company. |
| PICO-8, TIC-80, Picotron | Editors for code, sprites, map, sound effects and music in one program; Picotron's tools are written in Lua themselves. | "Never leave" works through limits. |
| Voxatron | A designer with voxel and room editors, its own text editor, Lua as glue. | The model for Voxcade. |
| Fyrox | Rust, its own UI (`fyrox-ui`), 1.0 in March 2026. Code stays outside, hot reload as a DLL. | – |
| Bevy | Still no editor at 0.20 (October 2026), only building blocks (Feathers, BSN, the remote inspector). | Without a clear aim an editor drags on for years. |
| LÖVE | No editor; ZeroBrane, or VS Code with Local Lua Debugger. | – |
| Positron, Arduino IDE 2 | A Code-OSS fork, and Theia. Posit: the extension API is not enough to change the workbench. | A lean VS Code only works as a fork, and a fork needs a team. |

## 3. The ways, measured against the vision

**The engine's view.** Natively, the texture `headless.rs` already draws
offscreen becomes a panel on the same wgpu device: no copy, no latency, the
same input, and it runs on Wayland; the three 3D editors use the real
renderer and the voxel UI. Tauri/wry and VS Code/Electron have only poor
options: child surfaces do not work on Wayland; streaming frames into a
webview costs about 500 MB/s at 1080p RGBA and 60 frames, encoding them
adds latency, neither fit for arcade play; the engine on WASM/WebGPU is not
practical (SDL3, threads, and WebKitGTK has no WebGPU). The editors would
have to be rebuilt in three.js, a second renderer never pixel for pixel the
same, plus the WebKitGTK troubles with NVIDIA the Tauri docs list. Two
windows (shell and engine) would be the way out, but on Wayland only the
compositor places windows side by side, not the app.

**The Lua editor.** On the web ways Monaco (VS Code's editor) is very good
at once, with LuaLS over a WebSocket bridge. Natively there is no ready
widget: `egui_code_editor` only highlights keywords; the editors of
Lapce/floem and Zed bring their own UI framework (Zed's editor is GPL, only
GPUI is Apache). So it is built: ropey as the buffer, tree-sitter-lua, an
LSP client with `lsp-types` talking to LuaLS (or `emmylua_ls` in Rust,
only partly LuaLS-compatible). Without tabs only one buffer is ever shown,
which keeps it simple. Risks: IME (SDL3's text editing events wired to egui
by hand) and accessibility (AccessKit has only a winit adapter, with SDL3
it needs `accesskit_xplat` or similar; the voxel UI is invisible to screen
readers).

**The terminal.** On the web xterm.js with a PTY is the standard (VS
Code's terminal). Natively: `portable-pty` (from WezTerm: ConPTY on
Windows, openpty on Unix), `alacritty_terminal` and a renderer of our own.
`egui_term` shows it works, but it is 0.1.0 from April 2025, tied to egui
0.31 and untested on Windows: a template to maintain ourselves. Claude Code
needs true colour, correct Unicode widths, bracketed paste, Shift+Enter,
copy and paste, scrollback and resizing.

**MCP** is the same on every way: it belongs to the engine.

**Cost** in person-months, lean / polished, one developer with agents:

| Part | Native | Tauri, Monaco, xterm.js | VS Code extension | VS Code fork |
|---|---|---|---|---|
| Shell (Logic/Assets, no tabs, memory, autosave, play/build) | 1–1.5 / 3 | 1–1.5 / 3 | 0.5 / only partly lean | 2 / 4 plus upstream merges |
| Lua editor with LuaLS | 2–3 / 6–9 | 0.5 / 2 | 0.25 / 1 | 0.25 / 1 |
| Terminal and agent | 1–1.5 / 2 | 0.3 / 0.5 | 0 | 0 |
| The engine's view | 0.25 / 0.5 | 1–3, Wayland unsolved | as Tauri | as Tauri |
| MCP server | 0.5–1 / 1.5 | the same | the same | the same |
| Voxel, image and level editors | 5–7 / 15–20 | +2–3 (a web renderer) | as Tauri | as Tauri |
| Build (3 systems) | 1 / 2 | 1 / 2 | 1 / 2 | 1 / 2 |
| **Sum** | **11–15 / 30–38** | **11–16 / 30–40 plus risk** | **10–15, not lean** | **12–17 plus the fork's upkeep** |

The web ways save 2 to 3 months on the code editor and lose them again on
the voxel editors: with this vision the editors dominate the cost. The
estimates count the agents in; what they do not speed up is tuning the
feel together with the user.

## 4. Recommendation and road

Native, on the engine:

1. **Step 4** (done): the groundwork below, with `voxcade mcp` headless.
2. **Phase 2 begins with the skeleton**, not with the level editor: it
   gives the "never leave" feeling earliest. The editors open from it in
   windows of their own (the user's choice, 2026-10-18), not as panels in
   its main area: each has the whole window, its camera and keys need not
   share egui's focus, two models open side by side can later pass voxels
   back and forth. They run in the IDE's process on its GPU, so it knows
   which file is open where and whether it is saved.
3. **Then the level editor**, then the voxel editor.
4. **Pulled forward:** mouse and picking from phase 3 to the start of
   phase 2; a minimal build (runtime and zip fused) into the skeleton,
   signing later.
5. **Step 5 (the jump & run)** stays as it is; with the skeleton its levels
   can already be built.

The leanest version of the vision:

| Phase | What | Cost |
|---|---|---|
| I, skeleton | The shell (Logic/Assets, memory, journal and autosave); the Lua editor with LuaLS (diagnostics, completion, hover, go to definition, rename); assets as previews (models turning, levels in the engine) with "open in MagicaVoxel" as the way out; play in the panel; build (unsigned); the terminal; MCP | 3–4 PM |
| II | Level editor: placing prefabs and things, painting height and material right in 3D (it writes the PNGs), a 2D view of the map | 3–4 PM |
| III | Voxel editor: the basic tools, palette, frames, onion skin, undo; its first part, the block editor for voxel tiles, is pulled forward (`tiles.md`) | 3–4 PM |

Together 9 to 12 months. After phase I nobody has to leave for code or the
agent.

## 5. Claude Code inside

- The IDE starts the user's shell in a pseudo terminal in the game
  folder (`alacritty_terminal`, which brings the PTY too, ConPTY on
  Windows); a button types `claude`.
- The game folder holds a `.mcp.json` (project scope; Claude Code asks once)
  with `{"command": "voxcade", "args": ["mcp"]}`.
- While the IDE runs, that stdio server finds it through a lock file (port
  and a random token, readable only by the user) and passes the requests
  on; without the IDE it serves the headless tools (as today).
- Claude Code takes pictures from MCP tools (the game's frame). Optionally
  "channels" can push events into the session ("the user selected rifleman
  #12"): new, to be checked before use.
- Claude Code's unofficial IDE protocol (lock files in `~/.claude/ide`) is
  not to be copied: it changes between versions and has had a security hole
  without a token (CVE-2025-52882).

The IDE's MCP tools:

| Purpose | Tools |
|---|---|
| Seeing | `view_state` (area, file, cursor, selection, unsaved changes), `diagnostics`, `game_frame` (PNG), `errors` (file:line:tick), `level_selection`, `things`/`thing(id)`, `timeline` |
| Showing | `open(path, line, range)`, `reveal(thing or cell)`, `select_asset`, `play_replay(file, tick)`, `pause`/`step`/`seek`, `play`, `build` |

The agent edits through the files. When the IDE has no unsaved changes in a
file it reloads the agent's change quietly; otherwise it merges three ways
against the last save.

## 6. What sets it apart (and suggestions beyond the vision)

- **Rewinding.** A snapshot costs 34 µs; 60 a second are about 2 ms of CPU
  a second. That gives a timeline to drag and "back to the tick before the
  error".
- **Change the code and play the same run on from tick N with it.**
  Comparing two runs, `state_hash` shows the first tick that differs.
- **Breakpoints after the fact.** A past tick runs again from its
  snapshot while `mlua::set_hook` with `every_line` writes down every step:
  forwards and backwards without stopping the game. A classic blocking
  breakpoint would freeze the single-threaded SDL loop (`src/app.rs`).
  Models: Tomorrow Corporation's tool demo and Wesley's GDC talk of 2013.
- **Replays as tests and bug reports**, which the agent plays over MCP and
  looks at with `shot`.
- **Everything is text.** People, agents and editors work on the same
  files; git-friendly.
- **Editors with the gamepad**, in the voxel UI.
- Unity and Godot step with a variable `dt` and non-deterministic physics,
  their game state is no value to copy: cheap rewinding cannot be added
  there (general knowledge, not checked one by one).

A debugger, the timeline, an sfxr-style editor, a tracker, git and
profiler panels are suggestions, not part of the vision.

## 7. Laid down by step 4

What the research asked of step 4, and what came of it (2026-10-16):

1. **Lua state in snapshots.** `mlua::Lua` clones as a handle to the same
   VM, so a game's `Clone` would have shared it between a snapshot and the
   running game. Built: the script's state lives only in `world.state` and
   `thing.data`, plain data the engine copies; globals are frozen after
   `load`; `Game::snapshot`/`restore` are explicit; `voxcade check` plays a
   second twice from a snapshot and names a local of `main.lua` that keeps
   state anyway. (Since the API's version 2, `docs/game-model.md`: the
   states are `world.state`, `level.state`, `player.state` and
   `thing.state`, and globals freeze after the start, `on_world_start`
   and the first `on_level_start`.)
2. **Errors halt.** A Lua error stops the game at its tick with file and
   line over the frozen frame; saving the mended file goes on, the state
   kept. Saving half-written code halts it the same way until the file
   loads again (the IDE's autosave may rather keep the last good version
   running: to decide with the skeleton).
3. **One source for the API.** `src/lua/api.rs` writes the LuaLS
   definitions (`lua/voxcade.lua`, with `Conf` for `conf.lua`) and the
   text `voxcade api` and the MCP tool show; every game folder has a
   `.luarc.json`.
4. **`conf.lua` instead of `game.toml`**, run sandboxed and
   deterministically; the replays' fingerprint takes the effective
   settings and `main.lua` as tokens (comments and layout do not count).
   Editors write data files (`map.json`, PNGs, `.vvx`), not `conf.lua`;
   should one ever need to, `full_moon` keeps comments and layout. Kinds of
   things are read through the engine (`level:things`, `Game::inspect`),
   never by parsing the file.
5. **`voxcade mcp` without a window**: `shot` (the frame as an image),
   `check`, `run`, `things`, `api`, a small JSON-RPC server of our own on
   stdio (no `rmcp` needed). Later the same command passes requests on to
   the running IDE.

Still open, for phase 2: a control API of the session without `app::run`
(run, pause, step, seek over a ring of snapshots, the frame as a picture;
`voxcade inspect` is its first piece); writing files safely (atomically
through a temporary file and a rename, `map.json` with sorted keys and one
entry a line, the watcher telling its own writes apart) and changing levels
in place (set a column, a material or a prefab and remesh only the chunks
touched); checking that dot_vox keeps the `_trans` material when writing
(the Huey's cockpit needs it).

## 8. The skeleton, as built

Stage by stage (the plan of 2026-10-18):

1. **The window and the files** (done): `voxcade ide <game>`, egui 0.37
   with egui-wgpu on the engine's SDL3 window and wgpu device; a small glue
   of our own (`src/gui/input.rs`: keys, text and IME, mouse and wheel,
   clipboard, cursor, HiDPI) instead of a ready-made integration. It draws
   only when something happened. The left side lists the logic (`.lua`,
   and the text files a script `read`s) and the assets by sort (level,
   models, blocks, images, music, other; `src/ide/files.rs`), the main
   area one file, read-only for now; what was open, how far each file was
   scrolled and where its cursor stood live in the config folder (a file
   chosen, or Escape from the game, gives the text the keys there; test
   `the_cursor_comes_back_and_the_agent_takes_no_keys`). `--shot` draws it without a window, frames a sixtieth of a
   second apart until egui's animations have ended.
2. **The console with Claude** (done): `src/ide/terminal.rs` runs the
   user's shell in a pseudo terminal through `alacritty_terminal` (its
   emulator and its PTY, ConPTY on Windows; no `portable-pty`): colours by
   name, number and RGB, wide letters, scrollback, lines rewrapped on
   resizing, answers to the program's questions, OSC 52 copies. Keys
   become a terminal's bytes (`key_bytes`: the cursor keys by the
   program's mode, Ctrl with a letter, Shift+Enter as Alt+Enter, a new
   line in Claude Code); pastes are bracketed when asked. `console.rs`
   draws it under the main area (folding, almost half the window at
   first), selects with the mouse (a double click a word), scrolls the
   history with the wheel (a program that asked for the mouse, as Claude
   Code's full screen does, hears buttons, drags and the wheel as xterm
   tells them, and copies through OSC 52; Shift held, the mouse is the
   console's; tests `the_mouse_is_told_as_xterm_tells_it`,
   `a_program_that_asks_for_the_mouse_hears_it`); Ctrl+C copies a
   selection or else interrupts,
   Ctrl+Shift+C/V copy and paste; what the mouse selects goes to the
   system's selection too (Linux's primary), a middle click pastes it; the
   clipboard refusing is said in the bar (tests
   `what_is_selected_is_copied`). CLAUDE first looks for the agent's
   program (`claude::Finder`: PATH, `~/.local/bin`, `~/.claude/local`,
   `~/.npm-global/bin`, then the user's login shell's `command -v`, once,
   three seconds at most, shown nowhere); missing, it types nothing and the
   console says in plain words what an agent is, that it is a program of its
   own needing a subscription or credit, where to get Claude Code (the
   address opens in the browser), that the IDE works without one and
   where to choose the agent, with LOOK AGAIN (finds it after installing
   and starts it); started with the IDE it leaves the console folded over
   a first-time maker's game and the bar says NO AGENT FOUND (the
   owner's wording: "einen Agenten, zum Beispiel Claude Code"; tests
   `a_missing_agent_is_explained_and_nothing_is_typed`). Found, it types `claude` and gives the
   console the keys (asked for after egui's own focus rule, which took
   them back: the click was elsewhere; test
   `claude_gives_the_console_the_keys`). How, the IDE's settings say: the
   gear at the top right (`ide/prefs.rs`, `config_dir()/ide/settings/ide.toml`,
   the same for every game; the user's wish, more to come). SHOW THE
   CONSOLE (off: none at all, no shell started, for those who need no
   agent; test `the_console_can_be_left_out`), the AGENT (Claude Code for
   now), and for Claude (`ide/claude.rs`): START WITH THE IDE (off at
   first), GO ON WITH THE LAST CONVERSATION (each game's by its session
   id, `claude --session-id` when it begins, `--resume` after; without
   one remembered the newest Claude Code kept in the game folder, begun
   there by hand), SEE THE IDE (`--mcp-config` naming `ide/mcp.json`
   beside the settings: `voxcade mcp`, no file in the game folder; off,
   the console's MCP button writes `.mcp.json` as before), NEW
   CONVERSATION (the console started anew). Claude is told its part
   (`claude::brief`, `--append-system-prompt-file` from
   `ide/settings/agents/`): the game in its folder, Voxcade the program
   it uses, what the engine lacks said rather than reached around. A
   game folder inside the engine's own source (ours: More Blood, Voxkeep)
   also gets `--settings` that leave out the CLAUDE.md files above it
   (the engine developers' notes), hide the engine skills (`voxcade-api`,
   `voxcade-verify`) and give it a memory of its own (Claude Code keeps
   one per repository, the engine's): Voxkeep's agent, seeing the
   engine's notes, took itself for the engine's developer, changed the
   kit and pushed (2026-10-21). No git ban: out there the game folder is
   the maker's own project. A conversation keeps the instructions it began
   with until it is compacted: NEW CONVERSATION takes new ones. The reading
   thread wakes the window with an SDL event of our own. Signs Hack lacks
   come from the system's fonts (`fonts.rs`), emoji from egui's.
   `--shot --console CMD` runs a command in it. Claude Code's first-run
   screen (choosing a theme) garbles in its redraw: it does the same in
   xterm.js, byte for byte, so it is Claude's, not the console's.
3. **The Lua editor** (done): the logic's files are changed in the main area
   (`src/ide/code.rs`, egui's text edit), coloured by the lexer's pieces
   (`lua::lex::spans`; `tokens`, the replays' fingerprint, is made from
   them and stays the same byte for byte, a test against the lexer as it
   was), the line numbers beside them. After every change Lua reads the
   text without running it (and the sandbox's `^` rule): the line it
   cannot read is marked and named. After `world:`, `world.`, `thing.`,
   `input.`, `out:` and `function` the API's names offer themselves with
   what they take (Up, Down, Enter or Tab, Escape), the mouse over one
   shows what it does, all from the register (`lua::api::signature`).
   `src/ide/buffer.rs` saves a second after the last change (at once,
   `save_at_once`), keeps the text meanwhile in a journal
   (`config_dir()/ide/<game>.journal/`) that brings it back after a
   crash, and looks at the file twice a second: changed from outside, it
   comes in by itself when there are no changes of one's own, else the
   IDE asks (KEEP MINE, TAKE THE NEW). The IDE draws only when something
   happened (an event, a change on disk, a save).
   **Scenarios** (2026-10-29): `scenarios/NAME.lua` listed under a heading
   of their own below the logic, always there (a plus in the heading, as at LOGIC, MODELS and MUSIC:
   a name typed in place makes a small file to change); PLAY SCENARIO over one plays the game from it (F5 begins it
   again). While the game plays, a line under OUTPUT runs Lua in it
   between two ticks, its answer in OUTPUT (up and down bring earlier
   lines back); the agent's `eval` does the same.
4. **Play in the main area** (done): PLAY saves the text being changed
   and plays the game straight in, no title and no menus
   (`src/ide/play.rs`: a `Session` stepped at the fixed tick, drawn into
   a texture of its own as big as the area, its sound through the
   player's speaker, its keys bound as the player set them;
   `controls::read_input`, `audio::speaker` and `Settings::options` came
   out of `app.rs` for it, `voxcade run` uses them as before). The game
   has the keys while it shows and nothing else has them (a click in the
   console takes them, one on the game gives them back); Escape goes
   back to the file and the game holds still, BACK TO THE GAME in the bar
   (instead of PLAY; a game that waits behind the file says so) shows it
   again, AGAIN (F5) starts it over, from its scenario if it was played
   from one, STOP ends it; the window elsewhere holds it too. Under
   the game a line says what its keys do (`MOVE ARROWS · JUMP SPACE`: moving, then
   the game's actions by their names with the keys as the player's keyboard
   names them, `play::keys_of`, the same the game's own controls menu
   shows; the keyboard's, the IDE plays with no pad's; tests
   `the_keys_line_names_the_actions_and_their_keys`,
   `again_and_back_to_the_game`). A file
   saved comes in at once (hot reload); a script's error halts it, named
   above the picture with TO THE LINE, which opens the file there, its
   line marked and the error named above it until the game takes the
   file anew (a game held takes in what is saved too, so the mark goes
   once the file is mended; tests `a_held_game_takes_in_the_mended_script`,
   `the_line_the_game_stopped_at_is_marked`). It
   steps as `voxcade shot` does (test `play_steps_and_draws_into_its_panel`);
   `voxcade ide --shot --play TICKS` draws it. Gamepads drive it too, any
   plugged in, at the start or later (the keyboard only while the game
   has the keys). A run won or lost (PRESS START) starts over with Enter,
   or Start or A on a pad: the IDE has no title to go back to. Its runs
   are recorded as `voxcade run` records them
   (`replay::Runs`): kept in the config folder from 10 s on, F9 puts the
   run being played (or the last) in the game's `replays/`, the bar says
   where; a run with a file saved during it is not kept (test
   `a_run_played_is_kept_with_f9`).
5. **Assets** (in part): the level editor and the voxel editor open in
   windows of their own, on the IDE's GPU (`window::Gpu`), one a file,
   brought to the front when asked again; the editor's loop is a window
   type (`editor::EditorWindow`), `voxcade edit` runs one. The voxel
   editor is the block editor on a model (`editor::frames::ModelFile`, a
   `.vvx` as text: `vvx::read_shape`, `vvx::write_shape`): frames as big
   as the model (`blocks::Block` is a box now), PREV, NEXT, COPY,
   DELETE, TURN for all frames, ADD beyond the box grows it and TRIM cuts
   it to the voxels, both about the model's pivot (it stands where it
   stood); a figure's parts each in its own box, placed from the pivot,
   shaped whole (FIGURE, `editor/figure.rs`). The palette keeps its signs:
   a new colour takes the next free one, a colour no voxel has any more
   stays (and keeps its sign) until a right click on it takes it out; the
   held colour's material (GLASS, METAL, ROUGH, GLOW, 0 to 1) is a quiet
   word beside the colour that opens to four sliders, a slider dragged
   one step back. Every model, figure and block of the games and
   templates written anew without a change is its text byte for byte (a
   test); saved unchanged, a file is the bytes it was. A MagicaVoxel
   `.vox` is not shaped: the editor says how to bring it in (`voxcade
   import`, the upgrade page); nothing writes `.vox`. In the main area a
   picture of what an editor opens (`ide/preview.rs`): a model or block
   round about (a figure of parts whole, a `.vox` world or prefab to look
   at), the level whole from the front, in the game's light, turned by
   dragging; a `.vvx` is text as well: TEXT beside EDIT shows it to read
   and change as a sound's text, read after every change (`vvx::read`,
   "LINE n: why"), PICTURE back; the editor's own view without its panels, drawn
   into a texture on the IDE's GPU only when it changed, fitted to the
   room. Back in the IDE (its window focused) the open file is read
   afresh. A model is seen as the game shows it as a run starts (the hero
   looking right, the other figures left at him, props turned to the
   camera, blocks as they lie in the level), and its frames play on the
   voxel editor's panel (PLAY, PAUSE) and in the IDE's picture, which has
   no floor (the editor's, to build on), PAUSE, PREV and NEXT above it
   (test `a_models_frames_are_held_and_stepped`). A song (`.mml`) is text to
   change under a line of what it is (title, length, channels) with PLAY
   SONG: it is parsed after every change (a mistake shows "LINE n: why" and
   marks the line; inside a channel, its first line) and plays as it
   stands, at the game's music volume and the player's, through a speaker
   of its own; opening another file or playing the game ends it (test
   `a_song_plays_until_something_else_is_open`). A sound (`sounds/NAME.toml`)
   has its group SOUNDS, always there, with a plus; the
   sounds the game brings from the engine and its rules are listed under
   them as "built in" and open read-only with MAKE IT YOURS. In the main
   area a sound is text to change under a line in words of what it is and
   PLAY SOUND (the text as it stands), its mistakes shown like Lua's, and
   under the text USED: the lines of the game's scripts that name it, each
   opening the file there. Between the line and the text the sound maker
   (`ide/maker.rs`): COIN, JUMP, SHOT, BLAST, HIT, POWER UP and BLIP each
   roll a fresh sound of that sort from a small table of ranges in the
   file's words and play it, a dice drawn before them as their sign (the
   same sort pressed again rolls another), UNDO goes back a step; before
   them PLAY (the way to PLAY SOUND at the top right is long) and LOOP,
   which plays the sound again and again, half a second between, until
   switched off, another file is opened or the game played; every rolled sound
   is rendered and measured first (`maker::good`: loud enough, not longer
   than the sort's, mostly between 80 Hz and 8 kHz, not below 30 Hz for
   long, round zero, quiet at its end) and drawn again if it fails (test
   `the_dice_gives_back_only_good_sounds`). The faders PITCH, LENGTH,
   SLIDE, BRIGHT and PUNCH each move words of their own (freq; hold and
   decay together; sweep; lowpass, off at the top; punch), read their
   place from the text and play the sound when let go. Every change is the
   text rewritten through `toml_edit`, its comments kept, into the buffer
   as typing is, and a step of the text edit's own undo: UNDO and Ctrl+Z
   go back over the same steps.
   Every text one can change says SAVED or NOT SAVED YET and Ctrl+S saves
   at once.
   **RENAME and DELETE** (`ide/shell/manage.rs`): for the logic, the
   scenarios, models, sounds, songs and the images and the rest, by a right
   click on a row of the list and by two small words beside SAVED over the
   open file (a first-time user does not find a right click). RENAME types
   the name in place in the list, as the plus does (the ending beside the
   field, not typed; the plus's checks, an image's name may keep capitals
   and dots); the open file stays open under its new name with its cursor
   and scroll, a text not saved is saved first, no journal is left under
   the old name. The lines of the game's Lua that name the file are
   asked about ("3 lines of your game play this sound by its name. Change
   them too?": CHANGE THEM, ONLY THE FILE, CANCEL): a sound by its quoted
   name, a Lua file by `require`, a model, a song and any other file by its
   quoted path; a scenario is named by nothing. DELETE asks nothing: the file
   goes to the system's trash (`trash` crate; if it will not, into
   `config_dir()/ide/deleted/<game>/`, said in OUTPUT) and the bar shows
   DELETED, what in the game still names it, and UNDO for 20 seconds (the
   bytes of a file up to 8 MB are kept in memory; a bigger one has UNDO only
   from our own folder, from the system's trash only by hand). `main.lua` and
   `conf.lua` stay (the game needs them), levels and blocks not yet (the
   level files link to them by name), the buttons off with the reason in
   their hover.
6. **The MCP bridge** (done): the IDE listens on 127.0.0.1 at a free port
   and writes the port and a random token into
   `config_dir()/ide/<game>.lock` (mode 0600, gone when the IDE closes;
   `src/ide/bridge.rs`). `voxcade mcp` lists the IDE's tools beside its
   own and passes them on: `view_state` (the area, the open file, its
   cursor, selection, unsaved changes and the line Lua cannot read; or
   the game and what stops it), `open(path, line)` (the cursor there, the
   keys left where they are: in the console the user types on),
   `game_frame` (the game played, as a PNG), `errors`, `play`, `stop`; a
   call without the token is refused. A tool's game is the folder the
   server runs in unless named, so Claude Code started in the IDE's
   console needs no paths. Rebuilt while the server runs, the program is
   found in its place (`cli::program`: Linux names the running one
   `voxcade (deleted)`; found by the agent in the IDE's console). The console's MCP button writes `.mcp.json` into the game
   folder (keeping the servers it names; Claude Code asks once whether to
   trust it). Tests `the_bridge_needs_the_token`,
   `mcp_forwards_to_a_running_ide`.
7. **Build** (done): BUILD saves the text being changed and packs this
   program and the game into `builds/<game>-<system>.zip`
   (`src/ide/export.rs`): a folder named for the game with the program,
   `games/<game>/` (no hidden files, no `replays/tests/`; the title's
   replays stay), a launcher to double-click (`play-<game>.sh`,
   `.command` or `.bat`, as `dist/`) and a short README; unsigned, for
   this system. The bar says where it went. Test
   `an_export_holds_the_game_and_runs` (unpacked, the game has the same
   fingerprint and plays); a real build ran `check` and `shot` from a
   folder of its own. (2026-11-11, the usability review: the zip goes into
   the game's own `builds/`, which the list and the next build leave out;
   it is packed on a thread of its own, the bar says BUILDING… and then
   BUILT with the zip's path and SHOW IN FOLDER, or BUILD FAILED in red
   with COPY; test `build_works_in_the_background_and_lands_in_the_games_folder`.)
8. **Projects** (2026-10-20, the user's choices: a projects folder, a
   template to choose, OPEN and NEW in the bar, not in a menu on the
   name, and a start page): beside the game's name OPEN, the games opened
   last (`prefs.rs`, `recent`) and OTHER FOLDER… (the system's folder
   dialog, `sdl3::dialog`, at once while there are none; a folder without
   `conf.lua` is refused), and NEW (`projects.rs`: a name and a
   template, the game begun in the projects folder, `~/Voxcade` unless
   the settings' CHANGE… says another, and opened). `voxcade ide` without
   a game shows the start page with the same. One window shows one game
   after another (`mod.rs`: `Window` what they share, `project` a game
   until another is chosen, `start_page`); leaving one saves the text,
   ends the run, closes its editors, its console and its bridge, and egui
   starts afresh; while a program runs in the console (Claude at work;
   Linux tells through `/proc`, elsewhere: the agent was started there)
   the IDE asks first, STILL RUNNING (tests
   `a_program_in_the_shell_makes_it_busy`,
   `another_game_opens_at_once_when_nothing_runs`). The templates lie in `templates/` and are built into
   the program (`templates.rs`, test
   `the_templates_built_in_are_the_repositorys`): EMPTY (no kit: a floor, a
   figure of cubes the arrows walk), JUMP & RUN (Voxkeep's room, blocks of
   10, the platformer kit) and SHOOTER (the shooter
   kit, a yard built by `on_level_start`, riflemen); their figures went to
   16-bit size on 2026-10-22 and back the next day (the user: the 8-bit
   ones had more charm; the hero 16 voxels tall in blocks of 10 again);
   their title "My Game" becomes the
   game's name (test `every_template_begins_a_game_that_plays`). A
   world the script builds (`c.world.size`) is empty when a kit places
   its player: after `on_level_start` the player is put on what it built.
9. **Help** (the user's wish: the typical handlers by example, as simple
   as can be, in plain English): HELP in the bar shows the Lua API by
   example in the main area (`help.rs`): the topics on the left, one
   beside them: a few plain sentences, the lines for `conf.lua` and a
   whole `main.lua`, each with COPY; the first topic, REFERENCE, every
   name of the API from its register. Every example runs as a game on
   the EMPTY template in a test (`every_example_runs`): change the API,
   and an example that no longer works says so.
10. **Output** (the user's choices: beside the console, switched, kept
   per game): the console's heading is CONSOLE and OUTPUT to switch
   between (`output.rs`). OUTPUT is what the IDE and the game report:
   BUILD's result, a file that would not load (`Play::said`, once printed
   to the terminal only), the game stopped by a script's error (once
   each, with its line: a click goes there), what the editors could not
   do (`Shell::tell`), runs kept (F9), and what the script printed
   (`print` in the sandbox keeps the newest 500 lines, `Game::printed`;
   it still goes to stderr for the command line). Errors while OUTPUT
   is not in view count beside its name, in red. Each game keeps the last
   300 lines in `config_dir()/ide/output/<game>.toml`, under OPENED AGAIN
   the next time; CLEAR forgets them. The agent's `errors` names the last
   errors too (tests `the_output_is_kept_for_the_next_time`,
   `the_game_reports_into_the_output`,
   `what_a_script_prints_is_kept_for_the_ide`).
11. **Seen at a glance** (2026-11-11, the usability review, findings 2, 5,
   6, 14, 30): the list is read again when the folder's names change
   (`Shell::look_at_folder`: names only, once a second, polled as
   `watch.rs` polls, no inotify), so what the agent makes is there at once
   (a file open that goes says so, its text kept, SAVE AGAIN; test
   `the_open_file_vanishing_is_said_and_not_recreated`). The console is
   folded to its bar for a game opened the first time, the shell started
   only when it is first unfolded; folded or not and the height dragged to
   are kept in `ide/<game>.toml` (an agent that starts with the IDE has it
   open); while it is folded, `CONSOLE •` in the accent colour says the
   shell wrote something (`Terminal::arrivals`), OUTPUT's red count the
   errors. What is wrong with the game (a script's error that halted it,
   a file that would not load, a game that would not start) is one red
   line over whatever the main area shows, with TO THE LINE, SHOW (OUTPUT,
   the console unfolded) and COPY, gone once mended (tests
   `the_console_is_folded_until_wanted_and_remembered`,
   `the_games_error_shows_over_whatever_is_open`).
12. **The upgrade page** (2026-11-12, the owner: a game of an older
   Voxcade does not open until it is brought up to date, by a wizard):
   opening such a game (OPEN, a folder, the launcher's EDIT and PLAY,
   `voxcade ide GAME`) shows a calm page instead (`ide/wizard.rs`): the
   Voxcade it was made for and this one, each step of `upgrade::standing`
   with what it would do in its own words (the dry run), the version line
   last; above UPGRADE and BACK a word when git has changes not committed
   (save a step first; the upgrade commits nothing). UPGRADE runs on a
   thread as BUILD does, then says what was done (COPY) or what went
   wrong in red (COPY); then REFIT THE REPLAYS (`replay::refit_saying`,
   as `voxcade check --refit`: written only if every replay plays as
   recorded, each one's line shown) and OPEN THE GAME (or PLAY, from the
   launcher's PLAY). A game made with a newer Voxcade gets a page saying
   so and pointing to the update. `voxcade ide GAME --shot FILE` draws
   the page, `--upgraded` as after UPGRADE run dry (nothing changed);
   tests `the_wizard_brings_an_older_game_up_to_date`,
   `a_dry_run_changes_nothing_and_replays_are_refitted`.

## Sources

Read locally: `docs/concept.md`, `docs/engine.md`, `src/headless.rs`,
`src/app.rs`, `src/map.rs`, dot_vox 5.2.0 (`write_vox`).

On the web:
- [Bevy 0.20](https://bevy.org/news/bevy-0-20/), [Fyrox blog](https://fyrox.rs/blog/)
- [Roblox Studio MCP](https://create.roblox.com/docs/studio/mcp), [Roblox studio-rust-mcp-server](https://github.com/Roblox/studio-rust-mcp-server)
- [Godot: editor development](https://godot.readthedocs.io/en/3.4/development/editor/introduction_to_editor_development.html), [Godot: external editor, LSP/DAP](https://docs.godotengine.org/en/stable/getting_started/editor/external_editor.html), [Godot 4.4](https://godotengine.org/releases/4.4/), [Godot 4.5 dev 4](https://godotengine.org/article/dev-snapshot-godot-4-5-dev-4/), [Godot Foundation, December 2024](https://godotengine.org/article/godot-foundation-update-dec-2024/)
- [Defold: Clojure/JavaFX](https://forum.defold.com/t/clojure-and-defold-questions-and-the-lecture/23411), [Defold: LSP in the editor](https://forum.defold.com/t/linting-and-code-navigation-in-the-code-editor/72465), [Defold: debugger](https://defold.com/manuals/debugging-game-logic/), [Defold: "drop the code editor"](https://forum.defold.com/t/defold-editor-should-drop-the-code-editor/72368), [Defold interview 2020](https://defold.com/2020/06/05/Interview-with-Defold-product-owner/)
- [Unity 10-K FY2025](https://www.sec.gov/Archives/edgar/data/1810806/000181080626000011/unity-20251231.htm), [Unity AI MCP](https://unity.com/blog/unity-ai-mcp-how-to-get-started)
- [Picotron](https://www.lexaloffle.com/picotron.php), [PICO-8](https://www.lexaloffle.com/pico-8.php), [Voxatron](https://www.lexaloffle.com/voxatron.php), [Voxatron designer](https://lexaloffle.com/bbs/?pid=11365), [TIC-80](https://github.com/nesbox/TIC-80/wiki)
- [Tomorrow Corporation demo (write-up)](https://handmade.network/p/675/blindspot/blog), the video (not checked): https://www.youtube.com/watch?v=72y2EC5fkcE, [GDC 2013, Wesley](https://gdcvault.com/play/1017769/Implementing-a-Rewindable-Instant-Replay)
- [Claude Code MCP](https://code.claude.com/docs/en/mcp), [Claude Code's IDE protocol (unofficial)](https://codepointer.substack.com/p/claude-code-cli-bridging-terminal), [ACP / Zed](https://zed.dev/blog/claude-code-via-acp), [Zed: GPUI licence](https://zed.dev/blog/zed-is-now-open-source)
- [egui_term](https://docs.rs/crate/egui_term/latest), [portable-pty](https://docs.rs/portable-pty), [egui-sdl3](https://docs.rs/egui-sdl3), [egui_code_editor](https://lib.rs/crates/egui_code_editor), [floem text_editor](https://lapce.dev/floem/floem/views/text_editor/index.html)
- [mlua Lua](https://docs.rs/mlua/latest/mlua/struct.Lua.html), [mlua HookTriggers](https://docs.rs/mlua/latest/mlua/debug/struct.HookTriggers.html), [rmcp](https://docs.rs/crate/rmcp)
- [LuaLS addons](https://github.com/LuaLS/lua-language-server/wiki/Addons), [emmylua-analyzer-rust](https://www.mintlify.com/EmmyLuaLs/emmylua-analyzer-rust), [lua-debug](https://github.com/actboy168/vscode-lua-debug), [full_moon](https://docs.rs/crate/full_moon/latest)
- [wry](https://docs.rs/wry/0.46.3/wry), [Tauri: Linux graphics](https://v2.tauri.app/develop/debug/linux-graphics/), [Wayland: no embedding protocol](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/issues/103)
- [VS Code custom editors](https://code.visualstudio.com/api/extension-guides/custom-editors), [Positron](https://github.com/posit-dev/positron), [Arduino IDE 2 / Theia](https://blogs.eclipse.org/node/7147)
- [Rerun architecture](https://github.com/rerun-io/rerun/blob/main/ARCHITECTURE.md), [Rerun timeline](https://rerun.io/docs/reference/viewer/timeline)
- [LÖVE distribution](https://love2d.org/wiki/Game_Distribution), [ZeroBrane and LÖVE](https://www.notebook.kulchenko.com/zerobrane/love2d-debugging)
- [AccessKit](https://github.com/AccessKit/accesskit), [accesskit_xplat](https://docs.rs/accesskit_xplat), [vengi](https://mgerhardy.itch.io/vengi-voxel-editor), [GDevelop and JavaScript](https://gdevelop.io/page/using-javascript-with-gdevelop)
