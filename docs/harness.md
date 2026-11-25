# The test harness

The game is driven by console commands, so a test is a text file of console commands. The
harness starts the game on an invisible display, feeds it that file, and reports PASS or FAIL.
Nothing appears on your desktop and no keyboard or mouse input is faked: the script runs
*inside* the game.

```sh
scripts/harness.sh tests/harness/smoke.cfg
```

```
building fast-fps...
started smoke
PASS  smoke  (9 checks, 1 screenshots)
results: /…/target/harness/20261123-022244
RESULT: PASS
```

## What makes a run pass

An instance passes only if all of these hold:

- its script reached `quit`
- the console printed no line starting with `error:` (failed `assert`, timed-out `wait_until`,
  unknown command, wrong arguments)
- nothing panicked, and the log has no `ERROR` line
- every `screenshot NAME` in the script produced a file

The exit status is `0` if every instance passed, `1` if any failed, `2` if the run couldn't
start (build failed, missing script, script without `quit`, missing `sway`).

On failure the harness prints each reason and the last console lines, so you can usually see
what went wrong without opening a file:

```
FAIL  bad  (bad.cfg)
      error: assert failed: ammo == 12 (ammo is 30)
      error: unknown command 'frobnicate', try 'help'
      --- last console lines (…/bad/console.txt) ---
      ] assert ammo == 12
      error: assert failed: ammo == 12 (ammo is 30)
```

**A PASS means the checks you wrote passed.** A script with no `assert` only proves the game
didn't crash. For anything visual, open the screenshots and look.

## Results

Each run gets a folder, and `target/harness/latest` always points at the newest one.

```
target/harness/<run>/
  result.txt            the report that was printed
  <script>/console.txt  everything the console printed, in order. Read this first.
  <script>/game.log     the full engine log
  <script>/*.png        screenshots
```

`console.txt` reads like a transcript: each command (`] like this`) followed by what it printed.

## Options

| | |
|---|---|
| `-t SECS` | Give up after this long. Default 120. Raise it on a slow machine. |
| `-s WxH` | Display size, which is also the screenshot size. Default `1280x720`. |
| `-v` | Watch: show each instance in a nested sway window on your desktop. |
| `-w` | Use ordinary windows on your desktop. No sway needed, but screenshot size is whatever the window manager gives. |
| `-c DIR` | Use this settings folder instead of a fresh one, to test that settings survive a restart. |
| `-S "ARGS"` | First start a dedicated server, `fast-fps --dedicated ARGS`, for the scripts to `connect` to. Its output goes to `server/server.log`; a panic there fails the run. |
| `-L` | First start the online lobby on `127.0.0.1:8099` (test key, game ports 27560-27561, games idle out after 6 s). Scripts use it with `lobby http://127.0.0.1:8099`. Its output and its games' go to `lobby/lobby.log`; a panic there fails the run. |

Several scripts in one command start several game instances, in order, each once the one
before it has loaded. That is how multiplayer is tested:

```sh
scripts/harness.sh tests/harness/lan_host.cfg tests/harness/lan_client.cfg
```

## Writing a script

A script is one console command per line. Blank lines and lines starting with `//` or `#` are
ignored. It **must end with `quit`**.

```
// tests/harness/smoke.cfg
wait_until grounded == 1 5
assert mode == offline
assert ammo == 30
screenshot boot
bot
assert bots == 1
fire 3
wait_until ammo == 27 3
quit
```

How scripts run:

- **Nothing runs until the game has loaded** and drawn a few frames. Don't add a startup wait.
- **One line runs per frame**, so each line sees the result of the one before it. `bot` then
  `assert bots == 1` works. (`;` puts several commands on one line and in one frame.)
- **`wait SECONDS` counts game time**, not wall-clock time, so it stays in step with physics
  and animation on a slow machine.
- **`screenshot` pauses the script** until the file is saved.
- **Prefer `wait_until` to `wait`** whenever you are waiting *for something*. A fixed wait is
  a guess that breaks on a slower machine; `wait_until` is exact and fails loudly.

### Commands for tests

| Command | What it does |
|---|---|
| `assert KEY OP VALUE` | Check a value right now. Prints `assert ok: …` or `error: assert failed: …`. |
| `wait_until KEY OP VALUE [SECS]` | Pause until the check is true. Fails after SECS (default 10). |
| `state [KEY …]` | Print values (all of them with no keys). Handy while writing a script. |
| `wait SECS` | Pause for that much game time. |
| `screenshot NAME` | Save `NAME.png` into the results folder. |
| `tp X Y Z` | Teleport yourself. Y is your body's center; standing on flat ground is 0.9. |
| `look YAW PITCH` | Set view angles in degrees. Yaw 0 looks toward -Z, 90 toward -X. Pitch up is positive. |
| `spin DEG_PER_SEC` | Turn steadily, for motion artifacts. `spin 0` stops. |
| `fire [N]` | Pull the trigger N times (as fast as the weapon allows). |
| `weapon NAME`, `zoom LEVEL` | Switch weapon; set the sniper scope level (0 is unscoped). |
| `bot [X Y Z YAW]`, `bot_do ID ORDER [ARG]` | Spawn and direct practice bots. See below. |
| `sky day`, `r_viewmodel 0` | Daylight and hiding your own gun, for clear screenshots. |
| `reload`, `scoreboard 1` | Things a player does with a key, for scripts. |
| `host PORT`, `connect IP:PORT`, `disconnect` | Multiplayer. |

`OP` is one of `== != < <= > >=`. Values that look like numbers compare as numbers (`==`
allows a tiny rounding error); anything else compares as text, ignoring case, with `==` and
`!=` only. `docs/console.md` lists every command.

### What you can check

`state` prints all of these. They are filled in by `collect` in `src/debug/probe.rs`.

| Key | Meaning |
|---|---|
| `mode` | `offline`, `hosting`, `connecting` or `client` |
| `players` | everyone in the game, bots included |
| `bots` | practice bots |
| `health`, `kills`, `deaths` | yours |
| `weapon` | `ak`, `mp5`, `glock`, `shotgun`, `sniper`, `knife`, `grenade`, `flintlock` |
| `ammo` | rounds left in the current weapon (the knife has 0, the grenade 1 or 0) |
| `reloading`, `scoped` | `1` or `0` |
| `spread` | your accuracy cone in degrees; bigger is less accurate |
| `x`, `y`, `z`, `speed` | your body's position and horizontal speed |
| `grounded`, `crouching` | `1` or `0` |
| `ragdolls` | corpses in the world, including ones fading out |
| `decals` | bullet holes in the world |
| `dummies_down` | practice dummies currently knocked over |
| `grenades` | grenades in flight |
| `explosions` | grenade explosions this machine has seen |
| `blast_x`, `blast_z` | where the last one went off |
| `duel` | `none`, or the duel's phase: `waiting`, `walk`, `steady`, `draw`, `round_over`, `over` |
| `duel_round`, `duel_wins` | the round number, and the wins as `1-0` |
| `duel_me`, `duel_slot` | `duelist`, `spectator` or `none`; your duelist slot `0` or `1` (`-1` if not dueling) |
| `out`, `on_mark` | `1` if you're out of play (dead this round, or watching); `1` if you're standing on your mark |
| `holstered`, `raised` | `1` while a duelist's pistol is still in the holster; how far it has come up since the draw, `0.00` to `1.00` |
| `lobby_games` | how many games the online lobby listed, the last time anything asked it (`games`, `join`) |
| `sens`, `fov` | view settings |
| `sky` | `night`, `dusk`, `dawn` or `day` |
| `name`, `time`, `fps` | your name, game seconds since start, frame rate |

To check something of your own, publish it from any system and it becomes a key:

```rust
fn publish(mut probe: ResMut<Probe>, coins: Res<Coins>) {
    probe.set("coins", coins.0);
}
```

### Bots

Bots are players with no person behind them, so a script can set up a scene and test things
that need someone else in the world. They work when hosting or offline.

```
bot -2 0.9 11 180        // at x=-2, standing height, z=11, facing +Z (yaw 180)
bot                      // or: 5 m in front of you, facing you
bot_do all pin           // animate in place instead of walking a circle
bot_do 1 run
bot_do 2 crouch
bot_do 3 weapon sniper
bot_do all fire 6
```

Orders: `idle walk run strafe back crouch stand jump reload fire weapon look pin free remove`.
Bots are numbered from 1 in the order they were spawned. Their shots are for show and do no
damage, but you can shoot them: one scoped sniper shot kills, or a few AK body shots.

To kill a bot from a script, face it, wait for your weapon to settle, and check the result:

```
tp 0 1.2 17
look 0 -6
weapon sniper
wait 1.0                 // draw time
zoom 1
wait 0.4                 // accuracy settles
fire 1
wait_until ragdolls == 1 3
assert kills == 1
```

### Looking at screenshots

Screenshots are the only way to verify anything visual. Two habits make them useful:

- Light the scene and clear the view at the top of the script: `sky day` and `r_viewmodel 0`.
  The default sky is dusk, which is moody but hides detail, and your own gun covers a third of
  the frame. (`r_ambient 12` adds flat fill light under any sky.)
- Take two shots a few frames apart (`wait 0.13`) for anything that moves, so a frozen
  animation or a one-frame glitch shows up.

If you have ImageMagick, crop and enlarge the part you care about before judging it:

```sh
magick target/harness/latest/anim/b_side_1.png -crop 800x400+300+200 +repage -resize 200% /tmp/crop.png
```

### Multiplayer scripts

Use `wait_until` to keep two instances in step instead of guessing at timing. The host waits
for the client to arrive; the client waits until it is really connected:

```
// host                               // client
host 27555                            connect 127.0.0.1:27555
wait_until players == 2 60            wait_until mode == client 20
…                                     …
wait_until players == 1 30            disconnect
quit                                  wait_until mode == offline 5
                                      quit
```

Use a port other than 27015 so the test can run while a real game is hosting.

A dedicated server has no script of its own: give it commands as launch arguments
(`-S "--port 27591 +mode duel"`), and check it from the clients. The first player to arrive
runs the game, so their `mode` and `bot` commands work. Online scripts start with `lobby
http://127.0.0.1:8099`, and `wait_until lobby_games == 1` after `games` waits for the
lobby's answer.

To test a binary that is already built (a release build, say) instead of building one:

```sh
HARNESS_BIN=target/release/fast-fps scripts/harness.sh tests/harness/smoke.cfg
```

### The browser version

`scripts/web-harness.sh` runs the same scripts in the browser version, in Chrome, with the
same rules for passing; `-c DIR` shares the browser's storage between scripts, like `-c` here.
[web.md](web.md) explains it. `smoke.cfg`, `duel.cfg`, `web.cfg` and the settings pair pass
there.

## The scenarios that ship

`scripts/check.sh` runs every line of `tests/harness/suite.txt`.

| Script | What it proves |
|---|---|
| `smoke.cfg` | The game starts, draws, answers commands, spawns a bot, fires. |
| `spray.cfg` | Each weapon fires and empties; accuracy changes scoped, unscoped and airborne. Screenshots show the spray patterns as bullet holes. |
| `ragdoll.cfg` | Three kills each leave a ragdoll, and they all fade away. |
| `anim.cfg` | Screenshots of every animation state, pose and weapon hold. For looking at. |
| `artifacts.cfg` | Screenshots while turning, in two anti-aliasing modes. For looking at. |
| `hud.cfg` | Every HUD state: slots, low ammo, reloading, kill feed, scoreboard. |
| `look.cfg` | The same views under all four skies, plus close-ups. For looking at. |
| `lan_host.cfg` + `lan_client.cfg` | A client joins, sees the host's bots, kills one, and both sides see the corpse. |
| `settings_save.cfg` then `settings_load.cfg` | Settings survive a restart (they share a `-c` folder). |
| `knife_grenade.cfg`, `flintlock.cfg` | The knife, the grenade and the flintlock: damage, smoke, the spark delay. |
| `duel.cfg` | A duel against bots: the marks, the hold, the quick draw, a low shot, rounds to 3-1, the next duelist. |
| `dedicated_a.cfg` + `dedicated_b.cfg` (`-S`) | Two players duel on a headless dedicated server; the first runs it and ends the duel. |
| `online_a.cfg` + `online_b.cfg` (`-L`) | One player starts a duel in the lobby, the other finds and joins it, they play a round, and the empty game shuts down. |

## When it doesn't work

- **"sway is needed"**: install sway, or pass `-w`.
- **Timed out**: read `console.txt` to see the last line that ran. Usually a `wait_until` that
  can never be true, or a machine slower than the timeout allows (`-t 300`).
- **A screenshot is black or empty**: you are inside or facing a wall (`state x y z`, then
  move), or it is simply dark (`sky day`).
- **"could not host: Address already in use"**: another instance has that port. Pick another.
- **It passes here and fails on another machine**: look for a fixed `wait` that should be a
  `wait_until`.
- **A note about "exit status … after 'quit'"**: the game crashed while shutting down, after
  the script had finished. It is reported but doesn't fail the run.

## How it works

- `scripts/harness.sh` builds the game, starts one headless `sway` per instance with a private
  runtime folder, and runs the game binary in it with `+name <script> +exec <script>`.
- It sets `HARNESS=1`, which makes the game compile shaders before drawing instead of in the
  background, so a screenshot is never of a half-drawn scene. `SCREENSHOT_DIR` tells
  `screenshot` where to save, and `CONSOLE_LOG` names the file the console transcript is
  written to. Scripts also wait for the HUD font to load.
- The script engine is `src/console/script.rs`. `assert`, `wait_until` and `state` are in
  `src/debug/probe.rs`. `screenshot`, `tp`, `look` are in `src/debug/mod.rs`.
- Anything that must finish before the next script line takes a `ScriptHold`
  (`hold.0 += 1`) and releases it when done. Screenshots and asset loading do this. If you add
  something slow that scripts depend on, do the same.
