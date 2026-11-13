---
name: night-mode
description: >-
  The owner's /night-mode command (2026-11-13): they go to bed, I work alone through what does not need them,
  switch the monitors off, keep the machine awake, and report in the morning. Covers taking stock (running agents and
  sessions, the open list, unanswered questions with a clear default), what may be done alone and what must wait, the
  night's limits (no release, no publishing, no new API names, nothing of the owner's games overwritten), window tests
  with the monitors off, commits at night, and the morning report. TRIGGER when the user types /night-mode or says
  they are going to bed and wants work done overnight. SKIP for daytime work with the owner present.
---

# Night mode

The owner, 2026-11-13: "ich gehe jetzt schlafen und du arbeitest bitte vor damit wir morgen früh einige offene punkte
vom tisch haben - entscheide was du ohne mich umsetzen kannst... wenn du meine monitore nicht brauchst, schalte sie
ab". Modelled on the night mode of another project of theirs.

## 1. Start (in this order, then work; do not wait for an answer)

1. **Take stock.**
   - What runs: background agents of this session, linked sessions (`list_sessions` linked), my own background
     commands, test IDEs I left open (`pgrep -fa "voxcade ide"`: close only the ones I started).
   - Open work: the local request list (`shots/reviews/*.md`, git-ignored), "To come" notes in `CLAUDE.md`, plans in
     the knowledge base (`firepit_knowledge_search`), the inbox (`firepit_inbox_list`), what a game's session asked.
   - Questions the owner has not answered: one with a default I stated and a rule of theirs behind it (a memory, a
     line of `CLAUDE.md`) counts as answered with the default; a taste call or a new name does not.
2. **Pick what I can finish alone**, in this order:
   1. what is running: verify it, try it, mend it, commit it;
   2. faults the owner or a game's session reported, with a known cause or a reproduction;
   3. stages of a plan the owner approved, up to the next point where they must look or choose;
   4. what a standing rule of theirs already decides (no sentences in the UI, standard terms, American English,
      short mouse ways, everything a right click offers also visible);
   5. upkeep: docs, `CLAUDE.md`, skills, the knowledge base, scratch files, temp leftovers.
3. **Tell the owner the night's list in one short message (German):** what I take, what I leave and why.
4. **Keep the machine awake** (`mcp__ccd_host__request_keep_awake`, `session_idle`) and **switch the monitors off:**
   `kscreen-doctor --dpms off` (Plasma on Wayland, all outputs; they wake on input). Check the exit code and
   `kscreen-doctor --dpms show`; a failure goes into the morning report.

## 2. Working at night

- **All daytime rules hold:** `CLAUDE.md`, the verify skill (release build, `cargo test --release`, clippy
  `--all-targets` 0 warnings, `voxcade check` of voxkeep, bunnyfarm, moreblood, pulsar, sokovox, arena, pictures
  looked at, no `voxcade-*` left in `/tmp`), the model choice for delegated work, one writer per set of files, no
  cargo fmt. I verify every agent's hand-back myself before it counts.
- **A window with the monitors off.** The IDE's remote channel (`voxcade ide GAME --remote`, `tools/ide_call.py`:
  `window_shot`, `pointer`, `keys`) draws from the program's own picture, so try it first with the monitors off. If
  the window does not answer within a few seconds (a compositor gives no frames to a dark output), wake them
  (`kscreen-doctor --dpms on`), run the window tests in one group, switch them off again (also when a test fails),
  and list each wake in the morning report. Only scratch copies of games in the scratchpad; never the owner's games.
- **Commits at night:** invoking /night-mode is the owner's word to commit and push to `main` what they asked
  for or what a rule of theirs decides, once it is verified as above and tried in a window where it is UI. One
  commit a finished piece, explicit about what it is. Everything else stays uncommitted and goes on the report.
- **Never at night:** a release, a tag, the website, a blog post published (a draft in `shots/` is fine), the Builds
  workflow, messages to people, deleting anything of the owner's, regenerating or overwriting a game's models or
  levels, `check --update` on a game's replays (a `--refit` after a rename is fine), a new name in the Lua API, a
  file format or `conf.lua` (prepare the proposal with the names; the owner decides), a change of plan, typing into
  the console of the owner's IDE or answering a prompt for them, killing a process I did not start.
- **A game's session** (the private game): tell it what landed, as by day; it may ask, it never approves. Nothing
  of that game is copied into this public repo, and its title is not written here.
- **Taste stays the owner's:** looks, words they have not settled, how something feels in the hand. Prepare
  variants or pictures and a recommendation; do not pick.
- **Blocked on the owner:** note what I need, move on to the next item. Two failed tries at one fault: stop, report.
- **Waiting:** while an agent or a long command runs, end the turn (its notification wakes me); never poll or sleep.
- **Memory and the temp folder:** `/tmp` is RAM here; check `df -h /tmp` and `ls /tmp | grep -c '^voxcade-'` now
  and then, and clear my own scratch games when a stage is done.

## 3. Morning report

- **One message in German** when the list is done or the owner is back: what is done (commit, the proof: tests,
  replays, what I tried in a window and what I only read), pictures to look at (paths in `shots/`), what waits for
  them (numbered, each with my recommendation and the default), what failed and why, what still runs, each time
  the monitors were woken.
- Leave the monitors off at the end; the owner switches them on.
