# Console commands

Press `~` in game. Tab completes command names, up and down recall history, and `help` prints
this list from the running game (which is always right, if this page drifts).

Launch arguments are console commands with a `+`: `cargo run -- +name alice +host`.
`exec FILE` runs a file of commands; that is how `tests/harness/*.cfg` and `cfg/bots.cfg` work.

## Playing

| Command | |
|---|---|
| `host [port]` | Start a LAN server (default port 27015). |
| `connect [ip[:port]]` | Join a server. With no address, joins the first one found on the LAN. |
| `disconnect` | Leave the server, or stop hosting. |
| `find` | Look for servers on the LAN. |
| `status` | Show connection and scores. |
| `name <name>` | Set your player name. Saved. |
| `sens <value>` | Mouse sensitivity (default 0.07). Saved. |
| `fov <degrees>` | Field of view (default 80). Saved. |
| `sky [night\|dusk\|dawn\|day\|next]` | Time of day (default dusk). Saved. |
| `weapon <ak\|mp5\|glock\|shotgun\|sniper>` | Switch weapons (also keys 1-5, the wheel, and Q). |
| `reload` | Reload (also R). |
| `scoreboard <0\|1>` | Keep the scoreboard up (holding Tab also shows it). |
| `zoom <level>` | Sniper scope level; 0 is unscoped (also right click). |
| `kill` | Respawn somewhere else. |
| `quit` | Exit. |

Saved settings live in `~/.config/<game name>/config.cfg`, which is itself a list of console
commands run at startup.

## Bots

| Command | |
|---|---|
| `bot [x y z yaw]` | Spawn a practice bot (host or offline). With no arguments: 5 m in front of you. |
| `bot_do <id\|all> <order> [arg]` | `idle walk run strafe back crouch stand jump reload fire weapon look pin free remove` |

## Scripts and tests

| Command | |
|---|---|
| `exec <file>` | Run a script of console commands. |
| `wait <seconds>` | Pause a script, in game time. |
| `state [key ...]` | Print probe values (all of them with no keys). |
| `assert <key> <op> <value>` | Check a probe value now. Ops: `== != < <= > >=`. |
| `wait_until <key> <op> <value> [seconds]` | Pause a script until a probe value matches (default 10 s). |
| `screenshot <name>` | Save a PNG (into `$SCREENSHOT_DIR` if set). |
| `tp <x> <y> <z>` | Teleport your character. |
| `look <yaw> <pitch>` | Set view angles in degrees. |
| `spin <deg/s>` | Turn steadily; 0 stops. |
| `fire [shots]` | Pull the trigger from a script. |
| `pos` | Print your position and view angles. |
| `echo <text>`, `clear`, `help` | Print text, clear the console, list commands. |

`docs/harness.md` explains how these fit together.

## Render switches

For tracking down visual bugs and lighting test screenshots.

| Command | |
|---|---|
| `r_aa <smaa\|taa\|fxaa\|off>` | Anti-aliasing (default smaa). |
| `r_ssao <0\|1>` | Screen-space ambient occlusion. |
| `r_bloom <0\|1>` | Bloom. |
| `r_fog <0\|1>` | Distance fog. |
| `r_contact <0\|1>` | Contact shadows. |
| `r_shadows <0\|1>` | Shadow maps on every light. |
| `r_viewmodel <0\|1>` | Draw your own gun. |
| `r_ambient <brightness>` | Flat fill light for inspecting models; 0 is off, 12 is a good test value. |
