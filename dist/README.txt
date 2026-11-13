Voxcade
=======

Start it
  Windows  double-click voxcade (voxcade.exe)
  Linux    double-click start-voxcade.sh (or in a terminal: ./voxcade)
The launcher opens. PLAY starts a game. EDIT opens it in the IDE, the
workshop where games are made. NEW GAME begins one of your own.

Playing
  Keys     Keyboard or gamepad. Every game shows its keys as it begins.
           All of them, to look up and to change: pause, SETTINGS,
           CONTROLS.
  Pause    Escape, or START on a gamepad. From there RESUME, SETTINGS,
           RESTART and QUIT TO TITLE. QUIT on the title ends the game.
  Picture  SETTINGS, VIDEO: FULLSCREEN, and SHADOWS (set them lower if
           the game runs slowly).
  Language SETTINGS, LANGUAGE: English, German or French for the menus.
  Two      A second player joins with START on a second gamepad, or with
           Return on the keyboard (Pulsar, and games made for two).

Playing together over the network (Pulsar, any game for two or more)
  Internet The host chooses NETWORK, HOST ON THE INTERNET. A moment later
           its pause page shows the game's code, six signs like DX67A3,
           and the code is on the clipboard: send it to your friends.
           They choose NETWORK, JOIN, TYPE A CODE. Both screens then show
           the same four signs: tell them to each other, and the host
           lets the other in on its pause page (LET IN). Nothing to open
           in a router or firewall.
  At home  One chooses NETWORK, HOST, the others NETWORK, JOIN and pick
           the game from the list. Only the host must let the game in:
           Windows asks once (allow it for private networks); on Linux
           with a firewall: sudo ufw allow 26500/udp
  Everyone needs the same version of Voxcade.

New versions
  The launcher looks for one when it starts (a small file from voxca.de;
  you can switch that off there). UPDATE NOW fetches it, checks it and
  puts it in place. What was here before goes to the folder
  before-update, a folder for each version replaced (a game that came
  with Voxcade and that you changed is kept there too; delete old ones
  when you like); your own games, settings, high scores and saves stay.

If it will not start
  Update your graphics driver: Voxcade needs graphics with Vulkan or, on
  Windows, DirectX 12. Windows may warn the first time that it protected
  your PC (Voxcade is not signed yet): More info, then Run anyway.
  Linux needs glibc 2.39 or newer (Ubuntu 24.04, Linux Mint 22, Fedora 40,
  Debian 13 or later). More help: https://voxca.de/install.html

Removing it
  Delete this folder. Settings, high scores and saves live in your user
  folder (Windows: %APPDATA%\voxcade, Linux: ~/.config/voxcade); games
  you made in the folder Voxcade in your home folder. On Linux the
  launcher also puts Voxcade into the applications menu, with its icon:
  ~/.local/share/applications/voxcade.desktop and
  ~/.local/share/icons/hicolor/256x256/apps/voxcade.png


For game makers
===============

A game is a folder with conf.lua (its settings) and main.lua (its rules)
and its models beside them. The IDE (EDIT, or NEW GAME) shows it all in
one window; HELP there explains the Lua API by example; BUILD packs your
game to pass on. A first game in five minutes:
https://voxca.de/quickstart.html

CLAUDE in the IDE's console starts Claude Code, an AI agent that helps
with your game. It is Anthropic's program, to install yourself
(https://claude.com/claude-code); it needs a Claude subscription or an
API key, and asks you to sign in the first time it starts there.

While a game plays, F5 starts it again and F9 keeps the run as a replay.
The key left of 1 (^ on a German keyboard, ` on a US one) opens the
console: a line of Lua run in the game (player.state.score = 5000).

In a terminal in this folder, voxcade --help lists every command. Some:
  voxcade run games/voxkeep            a game without the launcher
  voxcade ide games/voxkeep            the IDE on a game
  voxcade scenarios games/bunnyfarm    ways into a game late; then
  voxcade run games/bunnyfarm --scenario wolf_night
  voxcade shot games/moreblood --frames 300 --input up:300
                                       a picture without a window, and
                                       what a frame costs
  voxcade api                          the Lua API
  voxcade update                       looks for a new version
lua/voxcade.lua gives editors with the Lua language server (VS Code,
Zed, Neovim) completion, through each game's .luarc.json. For agents:
voxcade mcp (Claude Code: claude mcp add voxcade -- /path/to/voxcade mcp).

Voxcade is free, under the zlib licence (LICENSE.txt): use it, make games
with it, give them away or sell them. A game you make is yours.
THIRD-PARTY-LICENSES.txt holds the licences of the libraries built into
the program. Sokovox's puzzles are Microban by David W. Skinner.
