# Multiplayer and GAME JAM

The plan for playing together and for making a game together, written with
the user on 2026-10-29 and reviewed the same day by Fable (against the
code, with measurements) and Grok. Both stand on one base: a deterministic
game whose runs travel as inputs, and a session one machine hosts.

## What is decided

**Playing together**

- **One screen, as at an arcade machine:** 2 to 4 players at one machine,
  no split screen. One camera follows the group; its height stays fixed (no
  zoom), and a fixed rectangle from `conf.lua` (never the window's view,
  which grows with the window) holds the players together, scrolling only
  as far as everyone can follow. One keyboard can drive two players (WASD
  and the arrows), so one person can try it.
- **Over the network: 8 as the aim, not a wall.** Only inputs travel; the
  host gathers and sends them out. Lockstep with a few ticks of input delay
  is the default, the host building every tick and waiting for nobody
  (the user, 2026-10-30: a late input goes into the next tick); rollback is a game's choice (`c.network.rollback`), for
  action games with few players. Players at one machine and over the
  network mix: every source of input is a player's place. Each machine
  shows its own picture of the shared game.
- **Co-op, versus, teams: the game decides,** the engine carries every
  form. The threat model is friends playing: no anti-cheat, every machine
  holds the whole game.
- **Reaching each other:** on a LAN by stage 4's own UDP; over the
  internet through iroh, encrypted, with a relay of our own where two
  machines cannot reach each other directly (agreed 2026-10-31 with stage
  6's model: the relay comes with GAME JAM, before friends play; the
  lobby and matchmaking stay the server's, stage 7).

**Making a game together: GAME JAM** (JAM in the IDE)

- The IDE becomes a session for up to 4 people (a recommendation; more
  gets messy). One hosts: the project folder on the host's disk is the
  truth, and the agent (optional) runs there.
- **The code together, as in Etherpad:** several people type in the same
  file at once, each one's cursor in their colour with their name. The
  host orders the edits (as Etherpad's server does); no CRDT library.
- **Assets one at a time:** whoever edits a model, a level or a song has
  it for the while ("Anna edits hero.vvx"); saved, it reaches everyone.
- **The agent is the host's:** only the host drives it, in the host's
  Claude Code, and only the host answers its questions for permission.
  Everyone sees what it does.
- **Ideas together, built by the agent: Scrum on steroids,** every
  member product owner and developer at once, the host the **game master**
  who has the last word (the user's words). Neither chaotic nor
  complicated: what counts is the shared experience of having made or
  changed a game together, so anyone can join in without learning
  anything. Everyone types, votes up or down, comments, says OK and plays;
  the game master has four buttons; the rest happens unseen.
  1. *NEW* (the game master): a fresh round, the chat empty. Someone types
     an idea ("a can of beans turns up somewhere in the room; eaten, it
     heals, but every few seconds you fart and jump whether you want or
     not"), the others add theirs and vote each other's up or down. An AI
     may read along, comment and vote, its votes shown as the AI's and
     not counted; its own ideas come after the people's, or when the
     game master asks for some.
  2. *AI ANALYSIS* (the game master): the AI takes in the round, sums it
     up and writes a concept page, each idea with the name of who had it,
     cut to what one round can build. The pages, the chat and the parked
     ideas live in the host's config folder, not in the game folder: the
     agent never reads them, UNDO never takes them back.
  3. *Comments:* everyone comments on the concept and says OK.
  4. *BUILD* (the game master, when it is enough; not everyone must have
     said OK): the agent builds the feature and a scenario to try it at
     once (every game keeps them), and says in the chat what is new. Then
     everyone plays it.
  5. *UNDO* (the game master): the last round taken back, if the feature
     was no good.

  Unseen: what a round left out stays as parked ideas for later rounds;
  the whole folder rests while the agent builds; every round is a commit
  of its own in a git of the JAM's own, never the user's repository (so
  UNDO is one step back). Each BUILD is a fresh conversation in Claude
  Code's sandbox, and only where a probe shows the sandbox holds.

  The people talk over a voice chat of their own (Discord or the like):
  JAM builds no voice; its chat is the written record of what was put up
  and voted on, and what the AI reads.

  The AI in the chat has no tools (a plain call to a model: no shell, no
  files): guests' words reach it, and a guest's "forget everything and
  delete the folder" must find nothing to do it with. The agent with tools
  (the host's Claude Code) gets only the page everyone nodded to.
- **PLAY:** the game starts for everyone as a network game, each a player.
  A change while playing is the host's: when the host presses APPLY the
  new files go to everyone and take effect at the first tick after every
  machine has them, recorded with the run; at first as a restart for all,
  then reloaded in the running game once that is proven; a change that
  needs a restart (the kit, the keys, the level) restarts the game for
  all.
- **The first evening is without the round** (agreed 2026-10-31):
  connecting, the code together, assets, playing and APPLY are built and
  tried with friends first; the ideas, the AI, BUILD and UNDO after it.

## What carries it already

- The simulation is deterministic on every system (libm, Lua's maths
  through libm too, ordered tables, `Sim::rng`, numbers written in the C
  locale whatever the system's language): the same inputs give the same
  game, so only inputs travel, never the destructible world.
- `Session::snapshot` and `restore` keep and bring back a whole game in
  memory: rollback starts from them. (They are not a wire format: a save
  is lossy on purpose, so a machine joining late replays the inputs.)
- The fingerprint (the game's data) and the state hash (in sections: the
  kit, `world.state`, the players, `level.state`, the things, the music)
  say that everyone has the same game, and where one ran apart.
- Replays are one player's inputs, console lines and scenario; a network
  game is the inputs of several, recorded the same way.
- Lua runs sandboxed (no files, no network, no shell).

## What the reviews changed

- **Lockstep by default, rollback opt-in.** Fable estimated copying
  BunnyFarm's states out at 0.5 ms; `check --rollback 8` measured
  (2026-10-29): keeping BunnyFarm 0.10 to 0.13 ms, bringing it back 0.11 to
  0.19 ms, a tick 0.12 ms; the other games 0.003 to 0.02 ms to keep, up to
  0.08 ms to bring back. Going back 8 ticks costs BunnyFarm about 1.2 ms:
  rollback is affordable even for the biggest Lua game; a snapshot only at
  the confirmed tick all the same. Every replay of every game came out as
  recorded, going back every 8 ticks.
- **Late join plays the inputs since the start** (10 minutes of BunnyFarm
  in 3 to 5 s); a machine that ran apart leaves and joins again. A
  desync is shown and named by the section of the hash that differs,
  never healed in silence.
- **What each machine has of its own** cannot steer the game in a network
  game: the device, the language, the save slots, F5, LOAD, a hot reload,
  the console. Each player's controls travel with their inputs, loads
  are the host's, the language the host's in the simulation and each
  machine's own in its picture (`on_draw`), and F5, LOAD, reloads and console
  lines are events the host stamps with a tick. The console is the
  host's alone in a network game.
- **Joining checks exactly:** the build (not a version string), the
  game's fingerprint, the code's revision.
- **The host leaving ends the game** (no migration in the first version).
- **Under rollback** sounds of ticks played again sound once, and the
  music follows the confirmed clock.
- **Files from others are foreign:** paths checked (no `..`, no links, no
  hidden files), sizes bounded, and the readers of models, levels and
  songs fail cleanly on broken input.
- **The model's shapes:** `on_touch` with two players names its order,
  each player respawns and keeps a checkpoint of their own, saves hold
  every player's state (and keep the absent ones' for their return), the
  shooter's one-player hash stays byte for byte (a test, so every replay
  of More Blood plays on).

## Stages

Each stage is checked on its own (build, tests, clippy, every game's
replays) and committed on its own. The model is written, attacked by
other models and agreed with the user before it is built.

### 0. The proof first, without a network

- `voxcade check <game> --rollback K`: every K ticks the game goes back
  to the state of K ticks before and plays the inputs again; every replay
  must come out as recorded. The Builds workflow runs `check` on Linux,
  Windows and macOS: with the flag there, going back is proven across the
  systems before a byte travels. It prints what keeping, bringing back and
  capturing the states cost, game by game.
- Done 2026-10-29: every game's replays come out as recorded going back
  every 8 ticks, and the Builds workflow checks them so on every system.
- Left for when it is needed: the tick's end walking the script's states
  once (the plain-data check, the hash in sections and the capture
  together). Measured, keeping costs a tick's worth at most, so it is no
  longer a condition of rollback.

### 1. The model for several players (a document, no code)

`docs/game-model.md`, "Several players", and the voxcade-api skill:
seats (`c.players`, `world.players[n]` seat n or `false`, `p.seat`,
`player` seat 1 only), each player's `p.state`, `p.input`, `p.controls`;
the engine's keys `p.state.kind` (a character select), `out` (out of
play) and `team`; `level:players()`; `on_player_join(p, how)` and
`on_player_leave(p)`; one rule for a figure coming into a level (at the
tick's end, placed, at `NAME_n`, or with the first player); the hold as
a span over every player in play (`c.camera.hold`), the camera the
picture's, `frame.players`; touches, `by` and `on_interact` with several;
high scores per player or `c.team_score`; saves with every seat. Written
2026-10-29 against five games (More Blood for two, Pulsar with two ships,
Voxkeep with two heroes, a versus game for four, BunnyFarm for two),
attacked by Fable and Grok and revised; for the user to agree to before
stage 2.

### 2. Several players on one screen

Input for each player, a pad or a keyboard half joining with its start
button, WASD and the arrows on one keyboard; the shared camera; menus any
player opens; pause for everyone. Replays, `shot`, `inspect`, `check` and
MCP with input for each player.

Built 2026-10-30 for games without ready-made rules (`docs/game-model.md`,
"Several players"): seats and their input through `Session::set_seat`
and `Game::set_seats`, `src/seating.rs` (pads and keyboard halves), the
hold and the camera, LEAVE on the pause page of a player after the
first, high score lines per player, replays with each seat (`away`
while free), `shot` and `inspect` with `--input2` and on, MCP's
`players`; tested by a run of two replayed and gone back over every 8
ticks. The IDE's PLAY takes seats the same way. Then the players page
(the user's wish: more say in which player plays with what), the
settings' CONTROLS in a game of several (first on the title and the
pause page; moved when the settings were parted into AUDIO, VIDEO and
CONTROLS): each seat's part of the keyboard (all, a half,
none) and its pad, SWAP, each player's own keys (the halves' with their
directions, a pad's per seat); what the page shows plays from a run's
start, kept in the settings with the pads by name.

### 3. The kits for several players

The shooter and the platformer with several heroes, teams for versus (the
one-player hash byte for byte); the templates and one example game played
by two, each with scenarios and test replays for two.

Partly built 2026-10-30: the platformer kit plays several (a hero for
each seat, `c.player.kind` per seat in a game of several, each hero by
its own keys and hearts, held together and the camera on their middle;
Voxkeep's replays the same), the platformer template for two (its rules
each player's, a second hero in other colours), and Pulsar for two as
the example game (a test replay of two ships); then the shooter kit (a
figure of `c.player.kind`'s for every seat in a game of several, seat
1's too, each with its own hit points, gun and rounds; soldiers,
gunships, hostages and huts after the nearest player in the fight;
rounds and blasts naming who fired them; More Blood's replays and the
pins byte for byte, the kit's own player only in a game of one) and the
shooter template for two (tested by a run of two replayed and gone back
over every 8 ticks); the empty template for two (each player walked by
its own keys, drawn in its seat's colour); teams in the shooter's rules
of who hurts whom (`p.state.team`: rounds, blows and blasts of one
team hurt players of another, the aim assist finds them, the hit the
shooter's; tested by two players of one team and then of two).

### 4. Network play in lockstep, on a LAN

The transport (researched first: our own small UDP protocol or a library
such as GGRS used in lockstep); the host's tick as the clock, the others
keeping pace; hosting and joining by address and port; the exact checks
on joining; input delay; section hashes on the inputs' acknowledgements;
a desync shown; late join by playing the inputs since the start; leaving
and the host leaving; pause and the host's stamped events; a network game
recorded as a replay of every player's inputs. Tests without a window:
two or more sessions in one process over a simulated network (delay,
loss, jitter), and the same between two systems.

The model below was written 2026-10-30 from the research
(`reports/Netzwerkspiel im Lockstep über LAN.md` and its notes in
`research_notes/`, this machine only), with the user's choice that
nobody waits; attacked the same day by Fable (against the code, with
measurements) and Grok (`shots/reviews/fable-network.md`,
`grok-network.md`) and revised; agreed by the user the same day and
built in the six steps below (where the build made it more exact, "As
built" says how). Where a number is our own reasoning, not a source's,
it says so. What remains is the evening on the home network.

#### The host builds every tick, and nobody waits

- **One machine hosts, the others are guests, in a star.** Every machine
  plays the whole game; only inputs travel. The host gathers what every
  seat holds, builds each tick's **bundle** (every seat's input and the
  host's events, below) and sends it to every guest. This is lockstep
  relayed by a server, as Factorio has had since 0.14 (FFF #147): with 8
  machines, 14 packets a tick instead of the 56 a full mesh needs.
- **The host's clock is the game's.** The host's window runs its 60 Hz
  accumulator as in a game of one: at each tick it builds that tick's
  bundle, sends it and plays it. Whenever the host's loop plays no tick
  (below, the pause), nobody does.
- **A guest plays the bundles in order as they come,** keeping a few in
  hand against jitter: a target of one, raised or lowered by one only
  after some seconds outside its band (between 1 and 6), running 2 %
  fast with more in hand and 2 % slow with less (so a guest whose
  crystal runs fast does not drain its hand every few minutes), up to
  `MAX_CATCH_UP` (5) ticks in a frame after a hitch, drawing the last.
  With none in hand its picture waits. More than 2 s in hand means the
  machine cannot keep up: it leaves, saying so. (Our own reasoning, after
  the research's pacing and Fiedler's limit against the spiral of death.)
  As built: the hand is weighed as each tick is played (at 144 frames a
  second, between ticks, it looked fuller than it was); it runs dry twice
  in 2 s though it held its target, the target rises, four calm windows
  lower it; a bundle a little late is played as it comes and the next
  ones on time again (the pace kept for up to 6 ticks of waiting, so a
  hand being built up is not lost). On the simulated LAN every guest
  keeps a hand of one and the host holds its keys back 2 ticks.
- **Nobody waits for anyone** (the user's choice; Factorio's since 0.14
  and Photon Quantum's): the host never holds a tick back for a missing
  input. The host keeps **one slot for each seat**, the newest input it
  has, never a queue: a tick takes it if it came since the last tick;
  otherwise the seat repeats its last input for up to 6 ticks (100 ms),
  then holds nothing (`none`), so its figure stops instead of running on
  into a pit. A late input simply is the newest when the next tick is
  built. On Wi-Fi a laptop's packets now and then come 250 ms late (the
  99th percentile, MobiSys 2016): classic lockstep would freeze every
  screen each time, this only stops that laptop's figure.
- **A tap is never lost:** a guest reads its seats' keys every frame
  (the window does already) and sends each reading at once, numbered,
  with every one the host has not acknowledged. The host keeps for each
  seat the number of the newest reading it used; older readings, resent,
  are ignored. A tick takes the newest reading, and an action held in any
  reading that came since the last tick is held in this one too. Its two
  costs, which a game's author should know: a release and a press inside
  one tick become one hold (two taps, one press), and a key held through
  a gap is released and pressed again after it (a cut jump, then a jump:
  edges come from the last tick's input, `input:pressed`).
- **A seat is in play, holds nothing, or has left:** three states, and
  only the last is an event. Holding nothing is `Some(none)`, the
  seat's player stays (`on_player_leave` never fires for a gap); `None`,
  the seat left, comes only from LEAVE, QUIT TO TITLE, the machine's
  farewell, the host's DROP and 10 s of silence, stamped once at a tick.
  Every seat taken gets a new **generation**; readings of an older one
  (a former player's packets still on their way) are dropped.
- **The delay is the same for everyone, about 3 ticks (50 ms).** A
  guest's keys act after the round trip to the host and back plus what
  it keeps in hand: 2 ticks on a cable, about 3 on good Wi-Fi. The
  host's own seats would act at once, so the host holds their readings
  back by the guests' round trip in ticks plus their hand (measured from
  the acknowledgements, between 2 and 8, changed by one tick only after
  the measure differed for 2 s; or what the host sets): in a versus game
  nobody is quicker for hosting. It is the host's own business, so a
  change is no event; when the hold-back shrinks, the two readings
  falling on one tick are merged by the same rule, never one dropped.
- **Ticks, not steps:** bundles count the ticks the game played
  (`Session::ticked`, hit stop included). A fade between levels takes no
  ticks (recordings skip it already), so it is each machine's picture:
  each fades as its window does and goes on with the next bundle.
- **Silence:** after 1 s without a guest's packets every screen names it
  small at the top ("ANNA ..."); after 5 s the host's pause page offers
  DROP for it; after 10 s it is dropped by itself, its seats leaving.
  A guest that hears its host for 10 s no more ends with "THE HOST
  LEFT". Heartbeats every 50 ms while nothing else goes (paused, or a
  machine without a seat), so the Wi-Fi radio does not doze (our own
  reasoning from MobiSys 2016).

#### One feed of ticks: the window, a replay, the network

- **What a tick plays is one value:** seat 1's input and every other
  seat's `Option<Input>`, their controls, and the events before it (the
  console lines and the pause page's saves run before the tick, saves
  first, as the recorder stamps them today: `EvalAt` played before the
  step with `at <= steps`, `menu_saves` after the step with `at`). The
  window, a `Playback` and a guest all feed it; the recorder writes it.
- **The window's loop learns a third form,** beside the keys and a
  replay: a guest. Today it plays no tick while a menu or the console is
  open or the window has lost its focus, and every frame it writes this
  machine's controls into seat 1 and its `Seating` into the seats
  (`app.rs`). A guest instead plays whenever a bundle is in hand,
  whatever menu is open (drawn over the running game) and whatever the
  focus; the bundle sets every seat and seat 1's controls; what the
  guest's devices give becomes its readings and its asks.
- **`Seating` learns seats taken elsewhere:** over the network it only
  says which of this machine's devices drives which of this machine's
  seats; who sits where is the bundle's. Seats held by other machines
  stay taken whatever the PLAYERS page does, START skips them, SWAP
  refuses them; on a guest seat 1 is the host's from the start. As
  built: `Seating` describes this machine's players alone (its seats 1,
  2, ... are the players here), and `net::play::Locals` gives each the
  game's seat a bundle gave it: a player here who sits asks for one (the
  host's first has seat 1 from the start), one who leaves gives it up,
  one no seat was free for watches; the PLAYERS page so never touches
  another machine's seat.
- **The clock of a save is the host's:** a save's text carries the
  machine's time (`when`, for CONTINUE's newest), and the replays carry
  those texts; the bundle of a tick with a save in it carries the host's
  time, which every machine writes. (The game itself never reads `when`.)
- **Each machine's own seats** (`Session`'s local seats; at one machine
  every taken seat, so nothing changes there): `frame.players`, the
  high score lines a machine enters (its own players'), the language
  `on_draw` sees (the machine's; in the simulation the host's, seat 1's
  controls), rumble. A machine without a seat watches: `frame.players`
  empty, a key in braces then seat 1's.

#### What travels

- **Our own small protocol on UDP** (the research's choice): GGRS 0.13
  is a full mesh, meets each pair's clocks in the middle, fixes its
  players at the start and has no late join in a release; its desync
  check probably never fires in lockstep. renet and the others would
  only carry bytes, the lockstep remains ours. It is written as state
  machines without I/O (time and packets handed in, packets and ticks
  handed out), so the tests drive it on a simulated network, under a
  transport of two calls, `send(to, bytes)` and `drain()`: a UDP socket
  in a game, `SimNet` in a test (below), iroh's datagrams through routers
  (stage 6's model). About 1,000 to 2,000 lines with tests (an estimate).
- **Every packet** begins with `VXC`, the protocol's version, its kind
  and the session's key; HELLO and the search for games carry key 0, any
  other kind with a wrong key is dropped. The key (64 bits, from the
  system's random numbers, never `Sim::rng`, or joining would change the
  game) comes with WELCOME and tells a new session's packets from an old
  one's. Each machine has an id of its own, a random 128-bit number kept
  in its config folder; the computer's name, cut short, is only what
  people read. As built: a window joins with that id mixed with its
  process (`udp::program_id`), so two windows on one computer are two
  guests (the first was thrown out as "started anew" when testing with
  several windows); a program started anew is then a new guest, the old
  one's seat freed after 10 s of silence.
- **Two channels, one size limit:** at most 1,200 bytes a datagram (more
  is dropped as read; the receive buffer is bigger so nothing is cut),
  aiming under 508.
  - *Live:* a guest sends its readings not yet acknowledged, what
    changed in its players' controls, its asks (a seat taken or left),
    the highest bundle it has with none missing before it plus a bitmask
    of the 64 after, and every 10 ticks its section hashes with their
    tick. The host sends each guest the bundles it lacks from that
    acknowledgement on, as many as fit, in every datagram until the
    guest has them (a gap after silence in up to 8 datagrams a frame:
    built so, waiting for a lost one to time out starved the guests of
    the hard network), and the newest reading it has of each of its
    seats. A bundle is acknowledged when received, not when played.
  - *Pieces:* what does not fit a datagram (the run so far for a guest
    coming late, a save's text for LOAD) goes as numbered pieces of the
    same size, several in flight, each acknowledged; a bundle never
    carries such a text, only its number, and the host stamps LOAD only
    when every guest has all of it.
- **Bounds before memory:** every count is checked before anything is
  kept (seats by `c.players`, bundles and pieces by the datagram); a
  broken packet is dropped, never a panic (a test throws random bytes at
  the reader). A seat's input is 1 to 4 bytes, a tick of 8 seats about
  17; 7 guests cost the host about 40 KB/s, nothing on a LAN.
- **One socket, not blocking,** emptied every frame: the host on
  `0.0.0.0:26500` (UDP; unassigned by IANA, above Fedora's open 1024,
  below Linux's short-lived ports; 26501 to 26515 if taken, for a second
  host on one machine), a guest on any port. On Windows the socket gets
  `SIO_UDP_CONNRESET` and `SIO_UDP_NETRESET` off (a send to a closed
  port fails the next receive; Rust's std leaves it on,
  rust-lang/rust#128072) and no `SO_REUSEADDR`. IPv4 only until the
  server (stage 7). Whether SDL3 stops the main loop while a window is
  dragged on Windows (then a thread of its own receives) the Windows
  laptop tells; so does whether a laptop with several networks answers
  from the wrong one.
- **Kept open for rollback (stage 5):** a seat's input stays a small
  value with `none` as its default, a hash a tick can be had, and
  `Session::snapshot`/`restore` are there, so GGRS's design (or GGRS)
  can come in later; Photon Quantum runs rollback over a relaying server,
  so the star is no dead end.

#### Hosting, joining, finding

- **HOST and JOIN** are on the title of a game of several (`c.players`
  above 1) under NETWORK, and on the command line (`voxcade run GAME
  --host [PORT]`, `--join ADDRESS[:PORT]`). Not in the IDE's PLAY (no
  title, no menus, its hot reload): that comes with GAME JAM. HOST starts
  the run at once, open to the network: the arcade's way, a seat free is
  for whoever comes, during a level too, and there is no lobby to wait
  in. The host's pause page shows its addresses (each private IPv4
  address of a network that is up, the default route's first; no
  loopback, link-local, container, VM or VPN addresses) with the port,
  and who is there with their round trip.
- **JOIN** lists the games of this game found on the LAN (title,
  players, free seats; another game or another build shown but greyed)
  and takes an address typed (digits, dots, a colon; on a pad picked as
  initials are). Found by a broadcast query on the game's port, which the
  host answers from its game socket (only the host needs a firewall
  rule; a guest behind ufw, as on CachyOS, drops the answer, so the page
  says to type the address there). Typing always works, unless a guest
  network isolates its clients. mDNS only if the evening on the home
  network shows it is needed (`mdns-sd` calls itself beta and brings a
  thread).
- **Joining is two messages:** HELLO (the machine's id, a random number
  of this joining's own so a copy of it late on the network is not taken
  for the machine starting anew, its name, the protocol, the engine's
  commit) and WELCOME or a
  refusal saying what differs. Fiedler's padded request and challenge
  guard public servers against spoofing; they come with the server
  (stage 7). Refusals to one address: at most five a minute for one
  reason.
- **WELCOME says how the run began before anything is compared:** the
  run so far (the host's recording since its last start, with its
  scenario or the save it began from, the slots as it began, the seats,
  the console lines and the controls; it is the run's header and its
  ticks in one), the host's ms a tick, the key. The guest begins the run
  as the recording says (a scenario's file only counts for the
  fingerprint once a run has begun from it), then compares the
  fingerprint file by file (a hash for each, so a refusal names the
  file: "levels/3.json differs"), then takes its seats.
- **The build is not refused on:** each system has its own program, and
  hashing the sources would differ by Windows' line ends, refusing every
  Windows guest of a Linux host; the user's own machine builds from
  sources not yet committed. The commit (`git describe --always --dirty`
  when it is built, `unknown` without `.git`) travels for people, a
  difference is warned of, and exactness comes from the checks: the
  recording's hash every second while catching up, then the sections
  every 10 ticks. A build that plays differently runs apart at the first
  check and is named with its commit.
- **Taking seats:** the player who joined takes a seat with the device
  that joined; the others at that machine take seats as at one machine
  (START, Return), each an ask the host stamps into a bundle (or answers
  that none is free: the machine watches). The guest shows a seat as
  taken only when the bundle says so. Seat 1 is the host's.

#### Coming late, going, coming back

- **A guest coming late plays the run so far,** sent as pieces (the
  recording as the recorder writes it, run-length text: Pulsar's 1,500
  ticks for two are 1.5 KB, a farm's 10 minutes 17 KB, an hour a few
  hundred KB), while the bundles that come meanwhile wait in line. It
  plays them as `inspect` does, not drawing (as `check` does it is ten
  times slower), checking the recording's hash every second, then plays
  on with the bundles; nothing is heard or drawn meanwhile but CATCHING
  UP and how far, and its own recorder runs through it, so its recording
  begins at the run's start as the host's does. The host never waits for
  it. Only then do its players take seats.
- **How long:** measured on this machine, BunnyFarm plays 75 times real
  time without drawing (10 minutes in 8 s); since the game runs on
  meanwhile, catching up T takes T/74: an hour about 50 s, on an older
  laptop two or three times that. As built: a window plays the run so
  far through for 40 ms of each frame (only the progress is drawn);
  two programs over 127.0.0.1, BunnyFarm: 2 minutes of play caught up
  in 2.4 s, sending and loading included (at 10 ms a frame 3 minutes
  took 4.3 s). JOIN's guess is the host's ms a tick
  times the ticks so far, a warning above 30 s. F5 and LOAD start a new
  run and keep it short.
- **Sending a state instead** (Factorio's map, Quantum's snapshot) is
  left out on purpose: snapshots are no wire format and saves are lossy.
- **Leaving:** LEAVE on a guest's pause page leaves its seat; QUIT TO
  TITLE there leaves the network game (its farewell sent ten times).
  The host stamps a seat's leaving at the first tick after it knows.
  Gone silent and back within 10 s, a guest catches up from the bundles
  it missed on the live channel; later it comes as a guest coming late.
- **The host leaving ends the game** (decided): its farewell, or 10 s of
  silence; every guest keeps its recording up to its last tick.

#### The host's events and what each machine keeps

- **Everything that changes the shared game is in a bundle,** stamped
  with its tick, never applied as it arrives (Doom's mistake): seats
  taken and left, a player's controls changed, the host's console lines
  (`evals`, the run then helped for everyone), F5, LOAD (the save's
  number, its text sent as pieces first), the host's SAVE from the pause
  page with the host's time.
- **The pause is the host's loop standing still** (stage 1: the pause
  is the host's): whenever the host's window plays no tick today, its
  pause page and every page under it, SAVE and LOAD, its console open,
  no bundles are built and every machine stops within its hand; the
  heartbeats say PAUSED and by whom, so nobody takes it for a lost host.
  Going on needs nothing more: the guests fill their hands and play on.
  The host's window losing its focus pauses a game of one as today, but
  not a network game (the host fetching a drink would freeze every
  screen; ESCAPE does it). A guest's ESCAPE opens its own menu without
  stopping the game: its SETTINGS (each machine's own), LEAVE, QUIT TO
  TITLE; its players hold nothing meanwhile.
- **F5 restarts the run for everyone** on the files the network game
  began with: hot reload is off in a network game (the guests have only
  those files; sending files is GAME JAM's, stage 6). Each machine loads
  from its own disk with the host's slots in memory (never its own on
  disk) and checks the fingerprint against the network game's; the host
  with a file changed since the start says which and does not restart, a
  guest whose files changed leaves, naming the file. Loading costs every
  screen the host's load time (BunnyFarm 0.7 s here, 1 to 2 s on a
  laptop), under the 1 s notice.
- **Saves are the host's** (stage 1): `world:save` and SAVE write the
  host's slots; the guests keep the host's slots in memory (as the
  recording carries them), so `world.saves` reads alike, and write
  nothing. LOAD sends the save's text, a new run for everyone from its
  tick.
- **Each machine's own stays its own and steers nothing:** its settings,
  sound, picture, music keys, high score table (its own players' lines),
  `frame.players` (its seats), `world.language` in `on_draw`, where the
  camera looks without a hold, rumble for its pads. A network run kept
  with F9 is an ordinary replay and plays in attract mode like any.

#### Desyncs

- **The hash in sections are checkpoints of the one chain:** the state
  hash is fed in a fixed order today (the ready-made rules with their
  things, `world.state`, `player.state`, `level.state`, the levels and
  visits, the kept states, the things' states, the seats, the music);
  the hash after each part is kept as that section's, the last is the
  number every replay carries (unchanged, no cost beyond today's), and
  the first that differs between two machines names the section. One
  more beside it, the inputs: a running hash of every bundle played.
  Measured, a hash every 10 ticks costs BunnyFarm 6 to 8 % of the
  simulation's time, 0.01 ms a tick.
- **Every 10 ticks** a guest that has caught up sends its sections with
  their tick; the host compares the ticks it still holds (32 checks) with
  its own. While catching up the recording's checks are the comparison.
  The host is the reference (without it the game ends anyway): no vote.
- **What is done:** every screen shows the tick, the section and the
  machine with its commit ("DESYNC AT 18240 IN LEVEL.STATE, ANNA,
  2cb5d76"); each machine writes a report into its config folder: the
  network game's recording (the snapshot in file form: replayed, it
  brings back any tick), the states as `inspect` prints them at the last
  check that agreed and the first that did not, the section hashes.
  - *Only the inputs differ:* the machines got different bundles (the
    transport's fault); the guest leaves and joins again by itself,
    once.
  - *The inputs alike, a state not:* the game is not deterministic
    there; joining again would play the same inputs through the same
    code and run apart again. The guest leaves, its seats leaving, and
    says so; it may JOIN again by hand.
  As built: the report lies in `desyncs/<game>/<when>-<tick>/` of the
  config folder: `recording.toml`, `sections.txt` (each part's hash on
  this machine and the other's, the first that differs marked) and, worked
  out by a thread playing the recording, `agreed.json` and `apart.json`.
  The inputs' part leaves out the time a save is stamped with and the
  controls of seats nobody sits on (a guest coming late stamps the saves
  it plays through with its own clock: its part would always differ).
  Later, offline: the recording replayed to the last agreeing check,
  every tick hashed to the first that differs, hashes by thing and by
  key naming the value (`things[412].state.hp`); `check` on each
  machine's recording tells a machine's own fault from a difference in
  the inputs.

#### Recording

- **The host records its run as any run,** each input at the tick it
  acts (no delay in it), with its seats, controls, console lines, saves
  with the host's time and the hash every second: an ordinary replay, so
  `check`, `replay`, `film` and attract mode play it as they are. Every
  guest records the same stream (through its catch-up too): equal to the
  host's byte for byte, a check of the transport and the report's. For
  people only, not the simulation: which machine sat where, why each left
  (went, dropped, desync, the host gone). F9 keeps it on any machine.

#### Tests without a window

- **`SimNet` in one process:** a queue by delivery time and sending
  order, its own seed (never `Sim::rng`), time in whole microseconds and
  chances as fractions, the datagram's size limit enforced; per link
  delay, jitter, loss (also in bursts), duplicates, reordering, cut links
  and held packets; per machine a clock of its own with an offset and a
  drift. The machines step in a fixed order on the simulated clock (every
  500 µs), so a test is as deterministic as a replay. turmoil and madsim
  need async code on tokio: not for a fixed-tick loop.
- **What the tests say:** every tick two machines played has the same
  hash on both (every tick, as the test sees them all); each guest's
  recording equals the host's byte for byte and passes `check` (a game
  that saves and loads among them); Pulsar's `tests/two_ships` split over
  two machines, each with its seat's keys (as built: the network shifts
  the keys by its delay, so the run is not the replay's; the oracle is
  the guest's recording equal to the host's and the host's playing back
  as recorded); on
  a LAN's profile (1 to 5 ms, up to 1 % lost) no figure stops; a hole in
  the bundles is filled; a gap never makes a seat leave; a reading of an
  older generation steers nobody; a desync slipped in on purpose is
  found within the 10 ticks plus the delay, in the right section, and
  answered as its kind says; no false alarm over all the seeds; a late
  guest reaches the host's hash; a seat's leaving lands on the same tick
  everywhere; the host gone leaves no panic and no hang; no duplicate is
  played twice; random bytes never crash the reader.
- **Where:** `cargo test` a stub game (a hashed counter) with 2 to 4
  machines over 8 to 16 seeds and the Pulsar run; the Builds workflow
  1,000 seeds with up to 8 machines and a hard profile (1 s, 100 ms
  jitter, 25 % lost, 25 % doubled: yojimbo's soak), on Linux, Windows
  and macOS each writing a digest the last job compares bit for bit
  (GitHub does not connect two runners), and two real programs over
  127.0.0.1 on each (`voxcade inspect GAME --host PORT --input ...` and
  `--join 127.0.0.1:PORT --input ...`, paced in real time, their end
  hashes and recordings compared: a small test, `SimNet` covers the
  logic). By hand: netem on Linux, clumsy on Windows, `dnctl`/`pfctl` on
  macOS; then an evening on the home network (the Linux host, the
  Windows laptop, the Mac mini; cable and Wi-Fi).

#### What each system asks

- Only the host needs to let packets in (a guest's answers come back
  through any firewall). **Windows** asks once, for an administrator;
  CANCEL or a user without the right leaves rules that block in silence
  and never asks again (and a zip unpacked elsewhere asks anew): when
  nobody comes, the host's page says where to allow it and to make the
  network private. **CachyOS** drops what comes in (ufw): the host needs
  `sudo ufw allow 26500/udp`, and the page prints it. **macOS** asks
  nothing while the game starts from the Terminal (Apple's TN3179 spares
  programs started there, the Terminal left open); a `.app` later needs
  `NSLocalNetworkUsageDescription` and a signature. Fedora and Ubuntu as
  they come let it in. The help suggests the host on a cable and Wi-Fi at
  5 GHz.

#### Built in this order, each part checked and committed on its own

1. The hash's sections as checkpoints and the inputs' section; a tick's
   value (every seat's input, controls and the events before it) that
   the window, a replay and the network feed; the save's time from the
   feed; `Session`'s local seats with `frame.players`, the high score
   lines and the language in `on_draw` (stage 1's promises, nothing
   changing at one machine).
2. The protocol as state machines (the slot per seat, generations, the
   live channel and the pieces, the size limit), `SimNet`, the stub
   game's tests.
3. The network game in the session without a window (`inspect --host`,
   `--join`), Pulsar's run over two simulated machines as the oracle,
   coming late, F5 and LOAD with the host's slots.
4. The window: the guest's feed in the loop, `Seating` with seats taken
   elsewhere, the UDP transport, NETWORK, HOST, JOIN, the pause as the
   host's loop, a guest's own menu, the notices, LEAVE and DROP,
   CATCHING UP.
5. The checks on joining with each file's hash, the desync answer and
   report, joining again once.
6. Finding games by broadcast, the systems' help texts, the Windows
   socket's settings, the digests and the two programs in the Builds
   workflow. Then the evening on the home network.

Stage 4 adds no Lua name: what the game sees of it is stage 1's model
(seats taken and left, `frame.players`, `world.language` in `on_draw`),
built in step 1.

### 5. Rollback, a game's choice

The snapshot at the confirmed tick, guessing the others' inputs, going
back and playing forward; sounds once, the music on the confirmed clock,
the chunk meshes rebuilt where seen. `check --rollback` (stage 0) guards
it.

### 6. GAME JAM

A JAM hosted and joined in the IDE: the folder sent as a checked manifest
(no hidden files, no `target`, text with its line ends kept), who is there
and which file each has open; the code edited together in the host's
order, the others' cursors in the code view; assets one at a time; the
idea pages; the agent the host's (TO THE AGENT with a page); PLAY as a
network game on a sealed revision of the files; a change while playing
sent by the host for a named tick and recorded with the run.

The model below was written 2026-10-31 from the research
(`reports/GAME JAM gemeinsam Spiele bauen.md` and its four notes in
`research_notes/`, this machine only), attacked the same day by Fable
(against the code, with measurements) and Grok
(`shots/reviews/fable-jam.md`, `grok-jam.md`) and revised; agreed by the
user the same day, to be built in the steps at its end. Where a number or
a mechanism is our own reasoning, not a source's, it says so.

#### One rule: the host is the truth

Every part stays small for the same reason. One machine orders the
edits, so the code needs only the easy half of operational transform
(TP1). One machine knows who is connected, so a lock has one deadline.
One machine holds the folder, so syncing goes one way. And everything
that executes (the shell, the agent, git) runs on that one machine, so
there is one boundary to guard. A guest sends only the messages of a
written list (below); anything else is dropped.

#### What the research and the reviews change in what was decided

Agreed by the user 2026-10-31; "What is decided" above says so now.

- **A JAM needs the internet, and the internet needs a relay.** The
  people talk on Discord, so they sit in different homes. Direct
  connections between home routers succeed for 70 % (the one
  independent measurement, libp2p 2022) to "well north of 90 %"
  (Tailscale) of pairs; with three guests a quarter to two thirds of
  JAMs have someone who cannot get in without a relay (our arithmetic).
  Vodafone's and o2's cable lines sit behind DS-Lite: the host has no
  IPv4 port to forward. And stage 4's protocol must not go on the
  internet as it is: unencrypted, with the session key in every packet,
  anyone on the path reads the files and injects inputs. So the JAM is
  built on **iroh** (below), and the relay half of the server stage
  comes before the JAM goes to friends; the lobby can wait.
- **Ideas, concept pages and the chat live in the host's config folder,
  not in `ideas/` in the game folder:** the agent with tools must not
  read guests' words beyond the page, and UNDO must not take them back.
- **The AI's votes are shown, not counted, and it puts its own ideas up
  only after the people have** (AI help makes a group's ideas more
  alike: Doshi & Hauser 2024 and others; it lifts the quiet members: a
  CSCW 2026 study of 281 chats), or when the game master asks for some.
- **The whole folder rests while BUILD runs,** not "the files the agent
  builds in": nobody knows those beforehand, and only so the round's
  commit holds the agent's work alone.
- **Each BUILD is a fresh conversation in Claude Code's sandbox,** and
  only where a probe shows the sandbox holds.
- **A change reaches the running game when the host presses APPLY,** at
  the first tick after every machine has its files (LOAD's way), at
  first as a restart for everyone, hot reload once it is proven.
- **The round's history is a git of the JAM's own,** never the user's
  repository.
- **The first evening is without the round:** the base (connecting,
  the code together, assets, playing and APPLY) is built and tried with
  friends first; ideas, the AI, BUILD and UNDO come after it (Grok: the
  convergence test, the sandbox probe and a relayed 60 Hz test are the
  whole risk; the user: test before building more).

#### The engine loads files from bytes

Today every load and reload reads the game folder (`Session::reload_file`,
`run_main` reading `main.lua`; a late joiner's `replay_session` loads
what its folder holds now). A JAM needs the opposite: files applied at a
tick on every machine, a recording that plays a run across its changes,
PLAY on sealed files while the folder moves on. So first, without any
network:

- **`Session` loads and reloads from a set of files in memory** (path to
  bytes) laid over the folder (`load_from`, `reload_from`). In a JAM
  every machine plays from bytes; `check`, `replay`, `film` and a guest
  coming late do the same, so no replay ever writes a game folder.
- **A content store by hash** in the config folder (`jam/objects/`),
  on every machine; what a machine lacks it fetches from the host.
- **A recording carries the sealed start set** (each file the game
  reads, by path and hash) **and each APPLY** (its tick and the set it
  brought); the bytes lie in the store, and only a run kept with F9
  embeds them (base64: `toml` writes bytes as numbers, four times as
  big). The replay format gets version 2, so an engine of today refuses
  such a run instead of playing it on the wrong files. A run with an
  APPLY is kept (a flag of its own beside `touched`, which drops runs
  that no longer start from their files) and counts as helped in the
  high scores (GOD).
- **The fingerprint moves with each sealed set,** and F5 compares with
  the current one (`fresh` compares with the run's first today).
- **Names are NFC and line ends as the file has them:** the fingerprint
  folds `\r\n` for text as now; egui types `\n`, so a text file is
  turned to `\n` once when it enters a JAM (said in OUTPUT).

As built (step 1, 2026-10-31):

- **`src/files.rs`:** `Files` is a game's files as the engine reads
  them, the folder or bytes laid over it; every loader reads through
  the session's (`Session::files`: `conf.lua`, `main.lua`, `require`,
  `world:read`, scenarios, levels, maps, blocks, models, the display
  font, sounds, songs, the fingerprint), never the disk itself. `Store`
  keeps files by BLAKE3 in `config_dir()/jam/objects/` (a name that is
  no hash is refused, bytes are checked against theirs); `Files::seal`
  takes the folder as it is (not hidden files, `builds/`, `shots/`,
  `target/`, `replays/`, links), `of_set` the files of a set from the
  store, `changes_since` what the folder holds since, `differing` what
  two sets do not share.
- **`Session::load_from(files, slots)`** (`load_with(dir)` is the
  folder's). A sealed game is not touched by the folder: `reload` does
  nothing, its changes come by **`Session::apply(changes)`**, which lays
  them over its files and takes each in as a reload does (`conf.lua`,
  the levels, the models, the code, the rest), the states kept; the
  run counts as helped and is kept (`applied`, beside `touched`).
- **`Event::Apply`** is part of the tick's value, its files' hashes in
  the inputs section of the state hash; `Session::play` applies it. A
  snapshot holds the files of its time, and `restore` takes them in
  again before the state: `check`'s guard and going back
  (`--rollback`) pass across an APPLY.
- **Replays of version 2** (version 1, the folder's, as before): `files`
  the set it began on (its fingerprint the start's), `applies` (after
  how many steps and after how many console lines, each file's new hash,
  "" for one gone), `carried` the bytes as base64, put in by F9.
  `replay_session_in` and `Playback::new_in` take a store;
  `replay_session` and `Playback::new` this machine's. A guest coming
  late plays the run so far this way; over the network that comes with
  step 7.
- **Not yet on the network:** a bundle does not carry an APPLY (its
  files go first, as LOAD's, step 7); F5 and LOAD start again on the
  game's own files (`fresh` takes the session's `Files`), still compared
  with the fingerprint the network game began with, which moves with
  the APPLYs in step 7. NFC names and `\n` line ends come with the
  manifest (step 4).
- Tested: a run on sealed files with a console line and an APPLY plays
  back the same from the store, going back every 8 ticks too, and from
  itself on a machine with an empty store; the folder changed on disk
  meanwhile reaches neither the game nor its replay. Every game's
  replays, `--rollback 8` and the soak's digest came out as before.

#### Reaching each other: iroh

- **iroh 1.x** (n0, MIT/Apache-2.0, 1.0 on 2026-06-15, 1.4.1 now): QUIC
  dialled by a machine's public key, hole punching, a relay when that
  fails, reliable streams and unreliable datagrams on one connection,
  end to end encrypted, the peer authenticated by its key; a relay sees
  only ciphertext. matchbox (WebRTC) would need a signalling and a TURN
  server besides; libp2p brings a large framework without datagrams;
  Steam's and Epic's relays tie the program to their platforms.
- **Stage 4 runs over it with its limits as the transport's:** `Host`
  and `Guest` take datagrams from outside, so bundles and readings go as
  iroh datagrams, but `MAX_DATAGRAM` and `MAX_BUNDLE` become what the
  connection carries (QUIC's frames take room), the seat is bound to the
  peer's key (the session key in clear becomes redundant), and files,
  edits, chat and votes take reliable streams. Streams and datagrams are
  not ordered against each other: that is why APPLY is stamped only
  after delivery (below).
- **The LAN stays the LAN:** HOST offers "this network" (stage 4's UDP
  and its search, as now) and "over the internet" (iroh, a ticket);
  nothing falls to a relay unasked. Two transports for good.
- **Relays:** n0's public ones for our own trials only (free,
  rate-limited, "for development and testing", n0's words); before
  anyone else plays, one `iroh-relay` of ours (one binary, ports 80 and
  443, Let's Encrypt by itself; about 6 EUR a month at Hetzner with
  20 TB), its access control letting only Voxcade's machines relay and
  its rate limits on (both off by default). A fully relayed JAM of four
  playing is a few hundred MB an hour (our estimate): cost is no limit,
  looking after the server is.
- **iroh runs on its own thread** with a tokio runtime and talks to the
  window's loop through channels; the simulation never sees it.
- **The trial is pass or fail, with numbers:** at 60 Hz with four
  machines, direct and fully relayed, the 99th percentile of a
  datagram's delay and its loss, the biggest datagram that passes the
  relay, how soon a vanished machine is noticed, two machines on a LAN
  with the internet unplugged, and the frame times beside SDL3 and egui
  on all three systems. The relay hop runs over a reliable connection,
  so a loss there arrives late and datagrams bunch (the guest's hand,
  stage 4's jitter buffer, takes some of it); if relayed play fails the
  numbers, the JAM keeps iroh for editing and the evening says whether
  play needs more.

As built (step 2, 2026-10-31), the trial:

- **`src/net/quic.rs`:** one machine's iroh endpoint (`Quic`) on a
  thread of its own with a tokio runtime of one thread, talking to its
  user through channels (`send`, `connect`, `drain`), never waited on:
  what step 3 builds on. `Reach::Internet` (n0's preset: relays and
  their lookup, directly where it can), `Relayed` (relays only, the worst
  case), `Direct` (no relay, no lookup: a LAN without the internet).
  Silent `SILENT` (10 s, stage 4's) is gone. A ticket is `vxc1` and the
  address as base64; over the internet it names the key and the relay
  only, not the home address (iroh allows it: the machines learn each
  other's through the relay and go directly where they can), on a LAN
  the addresses. iroh 1.4.1 with `ring` (no aws-lc, so it builds on the
  three systems as the rest does); the program grew by 15 MB stripped
  (22.6 to 37.8), something to slim later (iroh's metrics and port
  mapping are features).
- **`voxcade nettest`** plays a host and three guests in one process on
  one clock (one-way delays, not half round trips), 60 datagrams a
  second each way of 1,000 bytes; `nettest host` and `nettest join
  TICKET` between two machines (half the round trip). The bounds: at
  most 1 % lost; p99 one way 100 ms through a relay (a guest's hand holds
  6 ticks), 20 ms directly; 1,000 bytes pass; a vanished machine noticed
  within 15 s; the main loop's p99 frame no more than 2 ms worse than
  alone, none over 50 ms. Our numbers, not a source's.
- **Measured on this machine** (all passed): directly 0 % lost, p99
  under 1 ms; only through n0's relays (Frankfurt, `euc1`) 0 % lost,
  p50 15 ms, p99 33 to 37 ms one way, the worst 58 ms; over the internet
  the connections begin through the relay and are direct within about a
  second, one datagram of 900 lost in the change; datagrams up to 1,400
  bytes pass (iroh says 1,162 at first, 1,414 directly once the path is
  measured); a vanished machine noticed after 13 s (QUIC's idle time of
  10 s and its timers); the main loop's frames unchanged (p99 16.7 ms).
  All four endpoints on one core (`taskset -c 0`): p99 50 ms through the
  relays, the frames still unchanged. In a network namespace with only
  loopback (`unshare -rn`), `direct` still connects: a LAN needs no
  internet. Two programs by a ticket: directly p99 0.6 ms, through the
  relay 27 ms.
- **Left for the evening and the Builds workflow:** two real homes (no
  machine of ours stands behind DS-Lite here), and Windows and macOS:
  the workflow runs `nettest direct` and `nettest relayed` on the three
  systems (not started yet). The frames were measured beside a loop of
  ours, not beside SDL3's and egui's; the window takes the bridge in
  step 3, its frame times are measured there.

As built (step 3, 2026-10-31), stage 4 over iroh:

- **`src/net/link.rs`:** a network game's transport, the LAN's UDP or
  iroh (`Link`), machines addressed as `Where::Lan(address)` or
  `Where::Net(connection)`; `Hosting` and `Joining` are `<G, Where>` in
  the window and in `inspect`. Over the internet a machine that connects
  only asks in (`Knock`): what it sends is dropped (its HELLO read for
  its name) until the host lets it in; both screens show four signs from
  the two keys (`link::code`, no 0, O, 1 or I).
- **The host fills datagrams to its transport's limit**
  (`Host::set_limit`, `wire::down`'s `limit`): `wire::MAX_DATAGRAM` on a
  LAN as before (the soak's digest the same), 1,100 over iroh (QUIC
  carries 1,162 until it has measured the path). A seat is bound to its
  connection, which iroh authenticates by its key.
- **The window:** NETWORK has HOST (the LAN) and HOST ON THE INTERNET
  (the ticket copied to the clipboard, TICKET COPIED: SEND IT); JOIN has
  PASTE A TICKET (a ticket is never kept as the last address); the
  host's pause page LET IN NAME CODE for each machine asking in and COPY
  THE TICKET; a line over the game while one asks in (NAME ASKS IN: CODE
  ...); a guest's JOINING page THE HOST LETS YOU IN: SAY CODE ...; in
  German and French too. `run --host internet`, `--join TICKET`; `shot
  --screen host-net-pause | joining-net`.
- **Without a window:** `inspect --host internet` prints its ticket and
  lets in every machine that asks (no one to ask), `--join TICKET`.
  `VOXCADE_REACH=relayed | direct` tries the worst case and a LAN without
  the internet. Measured: two programs played Pulsar by a ticket
  directly, over the internet (relay first, then direct) and only
  through n0's relays; the guest caught up in 0.4 to 1.7 s, the two
  recordings the same byte for byte each time. The Builds workflow does
  the same on the three systems (not started yet).
- **Our relay:** `OUR_RELAY` in `src/net/quic.rs` (its address and the
  token every Voxcade shows it), set since 2026-11-03 (below);
  `VOXCADE_RELAY` and `VOXCADE_RELAY_TOKEN` name another. Tried locally
  with `iroh-relay --dev`: with the token `nettest relayed` passed,
  without it or with a wrong one the relay refused ("the relay would
  not have us: not authorized", iroh's reason said). The relay became
  part of our matchmaker (below).
- **Our server running** (2026-11-03; a TLS proxy in front, `OUR_RELAY`
  and `MATCHMAKER` set): `rooms` lists the open rooms, `inspect --host
  internet` gets its code, `--join CODE` finds it there and plays
  (through the relay, then directly within a second; with
  `VOXCADE_REACH=relayed` only through it, 35 ms round trip), the two
  recordings the same byte for byte. `nettest relayed` loses nothing,
  but its host's way down to the guests (one connection carrying 180
  datagrams a second) passed the bound of p99 100 ms in 3 runs of 10
  (p99 76 to 81 ms) and missed it in the others (p99 125 to 246 ms);
  its way up passes. n0's Frankfurt relay passed in 4 runs of 5. Not the
  proxy in front (past it the same; it only copies an upgraded
  connection's bytes, Nagle off everywhere) and not the relay's limit
  per machine (a local matchmaker with the same limit passed at p99
  0.3 ms). What it is: packets lost on the way to the server, before it
  (its network adapter counted every packet in and discarded none). The
  relay hop is one TCP connection, so a lost retransmission waits some
  220 ms: the peaks. It goes by the packet rate of one connection, not
  its bytes: with datagrams of 250 bytes (still 180 a second) it missed
  the bound too, with one guest (60 a second) it passed. A host with
  three guests sends 180 datagrams a second through the relay however
  small its bundles, so a game relayed through our server may stall 250
  to 300 ms now and then; direct paths do not go through it.
- **Left:** our server does for now; for real play a more reliable
  one (the user, 2026-11-04): another provider's network, the loss
  lying before this server; the window's frame times beside iroh measured in a
  window; real homes on the evening.

As built (2026-11-01), game codes and our server, brought forward from
stage 7 (the user: a code like DX67A3, said or sent on Discord, is
simpler than a ticket):

- **`matchmaker`**, a project of its own (`../matchmaker`, to be public
  as SACRVM/matchmaker, open source for any peer-to-peer game; the user:
  "mach ein extra Projekt draus"; built here first as `server/`, moved
  out the same day): iroh's relay (`iroh-relay` as a library, plain HTTP
  on 127.0.0.1:7340, the token checked, rate limits) and the codes, an
  iroh endpoint of its own key reached through that relay (no second
  port). On our server a TLS proxy in front takes 443 and the
  certificate. Its config is made on the first start: the secret key
  never leaves the server, the log says the token and the public key for
  the game. Its README has the steps, its `docs/design.md` what it is to
  become (apps and rooms, a list, quick join, a WebSocket door for every
  engine).
- **The codes** (matchmaker's `matchmaker-protocol`, taken in by the
  game): six signs of 32 (no 0, O, 1 or I; 30 bits), taken as typed
  (small letters, a dash). A host's endpoint opens a room with its
  ticket (`Quic::open`) and keeps the connection: the code lives as long,
  and is gone when the host ends. A guest asks for the room of a code
  (`Quic::join`), then connects as by the ticket; the four signs and
  letting in stay. (Rooms, the list and any game: below.)
- **The window:** the host's code on the clipboard once it came (in
  place of the ticket), ON THE INTERNET: CODE DX67A3 on its pause page,
  COPY THE CODE; JOIN > TYPE A CODE (`Page::Code`, six places, a pad
  steps through the code's signs), PASTE A TICKET OR CODE; NO GAME WITH
  THE CODE ... on JOINING. The four signs are said without the word CODE
  now (NAME ASKS IN, SAYS KQ7P; SAY KQ7P). `shot --screen code`.
- **Without a window:** `inspect --host internet` prints the code
  (stderr) once it came, `--join DX67A3` joins by it; a code that leads
  nowhere ends the guest with the server's answer.
- **Rooms** (matchmaker 0.2, the same night; the user: "bau Stufe 2"):
  a host's game opens a room, `voxcade/<folder>` with its fingerprint as
  the version (`network::app_of`, `room_of`: the game's title, its seats,
  the host's name and build as attributes), and keeps its seats up to
  date (`Host::seats_taken`, `Link::update_room`). The bridge asks the
  matchmaker on one connection kept, in turn (`quic::matchmaker`);
  should it end while a room is open (the matchmaker started anew), the
  room is opened again and its new code told. JOIN lists the open games
  of this game and version (`network::NetFinder`, every 3 s, its
  endpoint started on a thread so the page never waits; a line as a LAN
  game's with the code for the address), ANY GAME ON THE INTERNET joins
  the fullest room with a free seat (`Network::join_any`); a code is
  asked with the game and version, so another game's or version's is
  said as such (`network::refusal_line`, the matchmaker's refusals in
  the game's languages). These items only where a matchmaker is known
  (`Menu::internet`). `voxcade rooms <game>` prints the list; `inspect
  --join any`. Tried: a host listed (1/2, then 2/2 once a guest sat), a
  guest by `any`, the recordings the same byte for byte; Sokovox with
  Pulsar's code: "the code is another game's"; an unknown code: "no game
  with this code".
- **Tried** (2026-11-01, on this machine): a local matchmaker, a
  host given B5XP6J, a guest joining by `b5x-p6j`: caught up, the two
  recordings the same byte for byte; joining the code after the host
  ended: "no game with this code" in 3 s, the server's book empty again.

#### Joining a JAM

- **JAM** at the top of the IDE, beside OPEN and NEW: HOST JAM checks
  the folder first (below), then shows a COPY button for the ticket;
  JOIN JAM takes a pasted one. A ticket names our relay and the host's
  key, not its home address (to check that iroh allows it), and the
  protocol's version: a guest of another build is told "you need the
  same version" (the code view offers the API of its own build, so a
  different commit is refused, not only warned of).
- **Admitting by a spoken code:** before it is let in, a machine may
  only say hello; both screens show the same four letters, read out on
  Discord, and the host lets them in (a ticket in a chat is a key anyone
  who sees it holds). REMOVE bans that machine's key for the JAM. Up to
  four, the host and three.
- **The folder travels as a manifest** built from the host's memory
  (each shared text at its current revision, every other file's bytes):
  a path (relative, `/`, NFC), a size and a BLAKE3 hash each, whole
  files (game folders measured here: 54 KB to 5.1 MB, 5 to 119 files).
  A file the joiner has with the same hash is not sent; each lands in a
  temporary file, renamed when its hash is right. Left out: hidden
  files, `builds/`, `shots/`, `target/`. Bounds before memory: 2,000
  files, 16 MB a file, 64 MB in all (our numbers).
- **The host checks its folder when it opens the JAM** and names what
  another system cannot hold, before anyone joins: two names alike but
  for case, a reserved Windows name (`CON`, `NUL`, `COM1` ...), a
  trailing dot or space, a forbidden sign, a link or anything not a
  plain file. The same rules hold for every file a JAM creates.
- **The guest's copy:** one folder per JAM game in its projects folder
  (`~/Voxcade/<game>`), found again by the game's JAM id, updated when
  it joins again after asking (what it changed alone meanwhile would be
  overwritten). Written only from what the host has ordered. A guest's
  own console and agent stay its own, but a change found in that folder
  is put back from the host's text and said in OUTPUT.
- **Who is there:** a name and a colour each (the seats' colours); the
  list on the left marks the files others have open; a click on a name
  goes to their cursor. Losing the connection shows AWAY, and coming
  back within minutes keeps the name, colour and place; nothing shows a
  hash. The host's machine is kept awake while it hosts. When the host
  leaves or crashes the JAM is over, every copy stays, the ticket is
  dead (no moving the host, as in stage 4).
- **What a guest may send:** hello and the spoken code's answer; an
  edit and a cursor; asking for a lock, giving it back, the save of an
  asset it holds; asking for a new, deleted or renamed file; a chat
  line, an idea, a comment, a vote, an OK; "I have these files";
  readings and asks of the game (stage 4); leaving. Each is checked
  (sizes, names, that the sender may), anything else is dropped, and a
  test says so.

#### The code together

- **Etherpad's way, as ot.js and Google Docs have it.** A change is a
  run of retain, insert and delete counted in Unicode scalar values
  (Rust's `char`, egui's `CharIndex`; never ot.js's UTF-16). The host
  keeps, for each shared text, its text, its revision and the whole log
  of the JAM (the files are small). Its own editor is a client of a
  server in its own process: one path for everyone. A change names the
  revision it was made on and its author's sequence number; the host
  transforms it over every logged change since, applies it as the next
  revision and sends every revision, in order, on each guest's stream:
  to its author as the acknowledgement of that sequence number, to the
  others as the change.
- **One `transform` with an explicit side,** the same on every machine:
  of two inserts at one place, the logged one stays first and the other
  moves behind it (Etherpad's convention). Where a remote change lands
  beside one's own caret is the caret's affinity, a matter of the view,
  not of the transform. A property test runs exactly this case, emoji,
  combining marks and line ends included.
- **A guest is ot.js's client of three states** (in step; one change in
  flight; one in flight and a buffer typed since): a remote change is
  transformed through the one in flight, then the result through the
  buffer, before it shows. A change goes out as soon as none is in
  flight (about one a frame on a direct path, more gathered over a
  relay; Jupiter was made for answers of seconds). Beside the text it
  shows, a guest keeps a **shadow at the last revision the host
  acknowledged**; its folder is written from the shadow only.
- **Coming back:** the guest names its last acknowledged revision, the
  host sends what followed, its change in flight goes again with its
  sequence number (counted once). Whatever cannot be rebased (the file
  replaced meanwhile) is kept as a copy beside the folder and said.
- **`transform`, `compose`, `invert` and `apply`** written in the image
  of the `operational-transform` crate (0.6.1, MIT, a port of ot.js;
  untouched since 2022), counting scalar values, about 600 lines.
- **egui gives the operations already** (the old open point, "a diff
  each frame", was wrong): `TextEdit` edits any `TextBuffer`, and every
  key reaches it as `insert_text` or `delete_char_range`; read in egui
  0.37.0's source, only its own undo and redo put back a whole text
  (`replace_with`). So a buffer that records sits round the code view's
  `String`. Undo and redo (Ctrl, Cmd on macOS) are taken before
  `TextEdit` sees them; while an input method composes nothing is sent,
  only its result; the API names offered after `world:` write through
  `replace_range` beside `TextEdit` and go through the same recorder;
  switching files sends what is waiting first. One insert is at most
  64 KB and a shared text at most 1 MB (our numbers); a bigger paste asks.
- **Undo is each person's own steps:** a stack of inverses of one's own
  changes (50, ot.js's number), each transformed through every remote
  change, an undo sent as an ordinary change.
- **Cursors and selections** travel with their revision, at most every
  100 ms (Liveblocks) and with every edit, are transformed like inserts
  and painted by us in the person's colour with their name over the
  galley `TextEditOutput` gives.
- **One function sets a file's bytes** on the host: the shared text, the
  disk and the guests change there together or not at all. The host
  saves a second after the last change, as `ide/buffer.rs` does, the
  head only, never reading the disk back over it. A file changed on the
  host's disk from outside (outside BUILD) is diffed against the head and
  goes in as a change by DISK; the host's own Claude Code works through
  BUILD while a JAM is on. A guest keeps no crash journal and has no
  "whose to keep" question: the host has the text.
- **Which files:** every text file opened in the main area (Lua,
  scenarios, `conf.lua`, songs, TOML, Markdown).

#### Assets one at a time

- **Opening a model, a level or its blocks in an editor window takes
  the lock** (UEFN's way; Unreal's "the first to save wins" loses work);
  others open it read only, with "ANNA EDITS HERO.VOX" over it and her
  colour in the list. A lock is a token (the holder's key and a count).
  It is given back when the window closes or when the host counts the
  machine gone (10 s of silence, as stage 4); the host can take it back
  after a warning (Perforce's admin unlock), counting the token on.
- **A save goes to the host first,** with the token: the host checks
  path, size and token, has the reader take it (models, levels and songs
  fail cleanly on broken input; a model's size is bounded before it is
  built), writes it through the one function and sends it to everyone;
  the saver's folder too is written from that answer. A save refused
  (the lock gone meanwhile) is kept beside the folder as
  `hero.vvx (Anna's)` and offered again once the lock is hers.
- **New, deleted and renamed files** are asks the host confirms; a JAM
  never creates a hidden name, `CLAUDE.md`, `CLAUDE.local.md`, `.claude/`
  or `.mcp.json` in the game folder (below).

#### Who may do what

The host is the game master: the console (its shell and its Claude
Code), the agent, git, NEW, AI ANALYSIS, BUILD, UNDO, PLAY and STOP,
APPLY, letting people in and out, taking a lock back, confirming new,
deleted and renamed files. Everyone edits code, holds assets, writes
ideas, votes, comments, says OK and plays. The console is never shared
(Live Share lets only the host start a terminal, and shares it read
only); the game's Lua console is the host's in a network game, as in
stage 4. A guest's Lua runs in the engine's sandbox on every machine (no
files, network or shell; its instruction budget and memory limit are the
boundary).

#### PLAY

- **PLAY starts a network game for everyone** (stage 4 over the JAM's
  connection): each person takes a seat as at a machine of their own,
  more people than seats watch. It plays a **sealed set**: the host's
  revision of every shared text and the hash of every other file at that
  moment; a machine takes part once it has those bytes, and plays them,
  not its folder. STOP ends it for all (Roblox has one shared test at a
  time, ended for all). PLAY's saves are the host's slots, as in stage 4:
  a JAM writes the host's real saves.
- **Editing goes on while it runs; APPLY brings it in, LOAD's way:** the
  new set goes to every guest first on the stream, each says "I have
  these files", and the first bundle the host builds after that carries
  the set's hash; every machine applies it before playing that tick (in
  order: `conf.lua`, levels, models, code). Only the hash travels in the
  bundle (a bundle's own bytes are at most 900). A guest that has not
  got the files within a while is told and watches; nobody waits.
- **At first an APPLY restarts the run for everyone on the new set**
  (F5 with new files). Then hot reload, once a test shows an APPLY
  across three machines comes out as its recording replayed:

  | Change | In the running game |
  |---|---|
  | Lua (`main.lua`, its modules) | reloaded, the states kept, as today |
  | models, songs | swapped (models count for the simulation) |
  | levels, live settings of `conf.lua` | the level built anew |
  | restart-only settings (`GameConfig::restart_only`) | restarted for all on the new set |

- **The set's hash is part of the tick's value,** so the inputs section
  of the state hash names a machine that applied another; it catches up
  again from the host's recording (stage 4's answer, done by itself;
  people see "CATCHING UP", never a section's name).
- **A script's error is a state of the game in a JAM, not the end of a
  network game:** every machine halts at the same tick, shows the file
  and line, bundles still count and catching up goes through the halted
  stretch, and the next APPLY that mends it lifts the halt everywhere
  (today a halted guest leaves the game, and the host ticks on halted).
  A start that failed stays a halt, never a silent new start.

#### The round

A panel JAM beside CONSOLE and OUTPUT; one big button per phase for the
game master (NEW, AI ANALYSIS, BUILD, UNDO, as decided), one line of
rules in the panel, nothing to learn (the Jackbox way: one task at a time). The
phase lines are the IDE's own words, not a model's.

1. **NEW:** an empty round. Everyone types ideas at once (a typed chat
   removes the waiting for one's turn that loses most ideas in spoken
   brainstorming, Diehl & Stroebe 1987); each idea a message with an id,
   comments under it. Each person has three up-votes and a "not this"
   (NN/g: about a quarter as many votes as options; down as a veto, not
   a minus), hidden until AI ANALYSIS, then shown with names; the AI's
   shown apart, never counted, never a fifth player. The parked ideas of
   earlier rounds are offered again. CANCEL ROUND drops the round and
   touches no code.
2. **AI ANALYSIS:** the chat AI writes the concept page in a fixed form:
   a title, one sentence, what the player will notice, what is built,
   the scenario to try it, at most three checks that it is done, and
   what is left out (back to the parked ideas). Who had which idea the
   code writes in from the ids, never the model (wrong attributions were
   16 % of a meeting assistant's failures). The code checks the form
   (only its sections, only ideas that exist, a length bound, no
   addresses, no shell, no paths outside the game) and marks what it
   finds instead of dropping it silently.
3. **Comments and OK:** the page is shown as plain text, not rendered
   Markdown (a comment a preview hides is visible), invisible and
   bidirectional characters shown as such, a diff after every revision.
   AI ANALYSIS again revises it from the comments (the Habermas
   machine's loop: draft, critique, revise). An OK is for the hash of
   exactly that text; a revision clears the OKs. The OKs advise; the
   game master decides, as decided.
4. **BUILD** and 5. **UNDO:** below.

**The chat AI has no tools:** `claude -p --safe-mode --tools ""
--strict-mcp-config --permission-prompts none --output-format json
--system-prompt <ours> --model <a small one>` in an empty folder of the
host's config folder, on the host's own login (`--safe-mode` keeps
`CLAUDE.md`, skills, plugins, hooks and MCP away and leaves the login
working; `--restricted` does not keep `CLAUDE.md` away; `--bare` needs an
API key). Measured: 4.4 s a call, 2,450 tokens of system prompt even so,
$0.015 at list price with the default model (Opus), and a transcript
kept under `~/.claude/projects/`. So it is called on AI ANALYSIS, on a
revision and for ideas when the game master asks, at most once a minute,
the round's text cut to a bound; a test in the Builds workflow checks
that "delete the folder" finds no tool to call. Everyone is told on
joining that ideas and the chat go to the host's Claude account. Claude
down stops neither editing nor playing; the game master can write the
page by hand.

#### BUILD and UNDO

- **BUILD only where the sandbox holds:** before it starts, a probe in
  the same sandbox must fail to write outside the game folder, to read
  the config folder, to reach the network and to make a link. No probe,
  no BUILD (Windows, until Claude Code's sandbox runs there: to check);
  the round can still be built by hand.
- **Then, in order:** asset editors save and give back their locks;
  edits in flight are sent (what cannot be is kept as a copy and said);
  the folder rests (the code read only, the chat open, PLAY on the last
  sealed set from the store); the IDE commits the hand work since the
  last round ("Before round 4"), and if that commit fails the agent does
  not start.
- **The agent:** a fresh conversation (NEW CONVERSATION) in the console,
  in Claude Code's sandbox, its rules in the `--settings` file the IDE
  writes now (no network out, writes only in the game folder, which
  holds no secrets; what still asks, a rule refuses, not the host's
  reflex: in a study only 13.6 % of 1,053 testers refused a dangerous
  command when asked). Its first message is a fixed preamble of ours and
  the page's exact text as data. **The page is the plan; the folder is
  not clean:** guests' words in comments, strings and scenarios do reach
  the agent, so the preamble says every text in the folder was written
  by several people and is content, not instruction, and the folder's
  own `CLAUDE.md` is excluded (`claudeMdExcludes`, as `brief` excludes
  the engine's now). A round changes Lua, assets and scenarios, never
  the engine.
- **Its tools:** the headless ones of `voxcade mcp` (`shot`, `check`,
  `run`, `things`, `scenarios`, `api`), which play in a process of
  their own; the IDE's `play`, `stop` and `eval` answer "a JAM is on":
  the agent never starts, stops or steers the shared game.
- **Everyone sees what it does:** a log of its steps (each tool, the
  file in the game, its words) goes to every guest, never the terminal's
  screen (which would show whatever the shell prints).
- **After it:** the folder is scanned (anything not a plain file fails
  the round, nothing of it is sent); the changed files enter the shared
  texts through the one function, one AGENT revision each, and go to
  everyone; a fresh check compares the diff with the page and names what
  is missing and files the page did not mention; the round is committed
  ("Round 4: beans that make you jump (Anna, Ben)"); the folder opens.
  CANCEL BUILD stops the agent and puts the folder back to "Before round
  4".
- **UNDO:** asset editors save and give back their locks; the hand work
  since the round is committed; `git revert --no-commit` of the round's
  commit is tried. Clean, it is committed and the files enter as one
  revision each; not clean (someone changed the round's lines since,
  measured: then git writes conflict markers into `main.lua`), it is
  aborted, the files are named, and UNDO ANYWAY puts the round's files
  back to how they were before it, as a new commit (hand edits in them
  since are lost, named first). Only the last round. Its ideas go back
  to the parked ones. It says what it does not touch: saves, settings
  and high scores live in each player's config folder (an old save
  meets code without the round: the game's `bring_up_to_date`).
- **The history:** a git directory and index of the JAM's own in the
  host's config folder (keyed by the folder's full path), the game
  folder its work tree (`git --git-dir`), `info/exclude` listing
  `shots/`, `builds/`, `target/`; every call with `core.autocrlf=false`
  and the JAM as author, never the user's name or key; never the user's
  repository nor the engine's (`games/` lives in it), and out of the
  agent's sandbox. Without git on the host, BUILD says it cannot.

#### Tests without a window

- `transform` by properties: two random changes, transformed, give one
  text in either order; the concurrent insert at one index; emoji,
  combining marks and line ends; four clients and the host's own over
  `SimNet` (delay, loss, reconnect) typing at random end with one text.
- The loader: a run with APPLYs recorded, then played by `check` from
  the store, comes out the same; a guest coming late across an APPLY.
- Joining: manifests with bad names, sizes and hashes refused; the
  host's check of its folder; a join broken off and resumed; a guest
  coming back fetching only what changed; messages off the list dropped.
- Locks: a save with an old token refused and kept as a copy.
- APPLY across three machines on `SimNet`, restart first, then hot
  reload; a script's error halting all three at one tick and the next
  APPLY lifting it.
- BUILD with a stub agent (a script writing files), the probe failing
  closed, CANCEL BUILD, the commits, UNDO clean and UNDO ANYWAY.

#### Built in this order, each part checked and committed on its own

1. The engine loads files from bytes: the store, `load_from`, the
   recording version 2 with its sets, the moving fingerprint (no network
   yet).
2. The iroh trial, pass or fail with its numbers.
3. Stage 4 over iroh: HOST and JOIN over the internet with a ticket and
   the spoken code, the LAN as it is; our own `iroh-relay` on a server
   the user rents (the relay half of stage 7).
4. The JAM session: the folder's check, the manifest, the guest's
   folder, who is there, AWAY, the list of messages.
5. The code together: the recorder, transform, shadows, cursors, undo,
   the one function.
6. Assets with their tokens; new, deleted and renamed files.
7. PLAY together: the sealed set, APPLY as a restart, then hot reload;
   the halt as a state; catching up on its own.

Then an evening with friends in their homes, "we edited and played";
a probe on each friend's line first (no measurement exists for German
home lines).

8. The round: the panel, ideas and votes, the chat AI, the page and its
   OK.
9. BUILD and UNDO: the probe, the fresh sandboxed conversation, the
   tools withheld, the log, the scan, the check, the JAM's git.

Then the second evening, with the round.

### 7. The server

The introducer and relay of our own: the lobby and matchmaking for games
and JAMs, relaying where machines cannot reach each other. On a server
the user runs. Its relay and the game codes came with stage 6
(`matchmaker`, a project of its own); the lobby and matchmaking stay
here, in the same program (its `docs/design.md`).

## Open, to research before its stage

- GGRS again for rollback (stage 5). The transport through routers:
  iroh, with a relay of our own (stage 6's model, researched
  2026-10-31; matchbox would need a signalling and a TURN server); on a
  LAN our own UDP (stage 4, researched 2026-10-30).
- Apple silicon: glam's vector paths there may fuse multiplications; the
  simulation keeps to scalar maths, and a test says so before it is built.
