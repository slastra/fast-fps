# Working on this project

This file is for AI coding agents (and people). It is the one source of truth; `CLAUDE.md`
and `GEMINI.md` only point here.

fast-fps is a small low-poly LAN shooter built on Bevy 0.20, and a starting point for other
games. It has no menus: everything runs from an in-game console, and that same console is how
tests drive the game.

## Commands

```sh
cargo run                         # play (first build compiles the engine: many minutes)
cargo run -- +exec cfg/bots.cfg   # play with practice bots (launch args are console commands)
cargo test                        # unit tests, seconds
scripts/check.sh --quick          # format + lint + unit tests + smoke scenario
scripts/check.sh                  # everything, including all scripted scenarios (minutes)
scripts/harness.sh tests/harness/smoke.cfg    # one scripted scenario
scripts/new-game.sh ~/Projects/Rust/my-game   # start a new game from this one
```

## How to make a change

1. Read `docs/tour.md` to find the right file. Each source file starts with a comment saying
   what it is for.
2. Make the change. Check `docs/recipes.md` first; common changes have step-by-step recipes.
3. Run `scripts/check.sh --quick`.
4. **Prove it in the running game.** Write or extend a script in `tests/harness/`, run it with
   `scripts/harness.sh`, and read the result. Use `assert` for anything that can be checked as
   a value. For anything visual, open the screenshots and look at them. A passing build proves
   very little here; most bugs in this project were only visible in a screenshot.
5. Run `scripts/check.sh` before calling it done. If you added a scenario worth keeping, add
   it to `tests/harness/suite.txt`.

`docs/harness.md` is the full reference for step 4.

## Where things are

| You want to change | Look in |
|---|---|
| A number: health, run speed, jump height, ports | `src/config.rs` |
| A weapon's stats or spray pattern, or add a weapon | `src/weapon/defs.rs` |
| The map | `src/world/arena.rs` |
| What gets sent over the network | `src/net/protocol.rs` |
| Game rules: damage, kills, respawns | `src/server/` |
| How other players look and move | `src/avatar/` |
| Keys and mouse | `src/player/input.rs` |
| The HUD | `src/hud/` |
| Console commands for testing | `src/debug/` |

## Rules of this codebase

- **The server decides.** Anything that affects other players (damage, score, spawning) runs
  in `src/server/`, guarded by `run_if(in_state(ClientState::Disconnected))`. That condition
  means "this machine is not a client", so it covers both hosting and offline play. A client
  only *asks*, by sending a message defined in `src/net/protocol.rs`.
- **Everything on the wire lives in `src/net/protocol.rs`** and is registered in its
  `register` function. Client and server must register the same things in the same order.
- **Console commands are the interface.** Register one with `app.console_command(name, usage,
  help)` and handle it by reading `ConsoleCommand` messages. Report problems with
  `Print::error(...)`, never a plain `Print`: the `error:` prefix is what fails a test.
- **Make new state checkable.** If you add something a test should be able to see, publish it
  to the probe (`probe.set("coins", n)`, see `src/debug/probe.rs`) so scripts can `assert` it.
- **Tunable numbers go in `src/config.rs`**, not scattered through the code.
- **One camera.** The first-person gun is drawn by the main camera, scaled toward the eye. Do
  not add a second camera for it (see Traps).
- Keep `cargo fmt` and `cargo clippy --all-targets -- -D warnings` clean. `check.sh` enforces both.

## Traps

Each of these cost real debugging time once.

- **A corpse or effect for a dead player must use the position sent in the `Damaged`
  message**, not the player's entity. Bots respawn instantly and real players within a
  network round trip, so the entity has usually moved by the time the message arrives.
- **Never scale a physics body that has joints.** The solver explodes. Remove `RigidBody` and
  `Collider` first, then scale (see `avatar/ragdoll.rs`).
- **Temporal anti-aliasing plus a second camera smears.** TAA blends each frame with the last
  one, so anything composited on top ghosts across the screen when you turn. Default is SMAA.
- **Camera rotation written in `Update` is lost.** The character controller copies the camera
  into the character at the start of the fixed loop and back out in `Update`. Scripted camera
  writes happen in `PreUpdate` (see `debug/mod.rs`).
- **The view rig is not the camera.** The mouse turns `ViewRig`; the camera is its child and
  only carries recoil. Aim from the rig, or recoil will move where bullets go.
- **Shaders compile in the background during normal play**, so the first frames are missing
  geometry. Under the harness they compile synchronously (`HARNESS=1`). If a manual screenshot
  looks empty, that is why.
- **Rust arrays indexed by weapon** are sized with `WeaponKind::COUNT`. Adding a weapon means
  following the recipe; the compiler finds most of what you miss.
- **Two instances can't share a UDP port.** The LAN scenario uses 27555 so it can run beside a
  real game on the default 27015.

## Building

- Linux packages (Arch/CachyOS names): `base-devel alsa-lib systemd-libs wayland libxkbcommon
  vulkan-icd-loader` plus a Vulkan driver for the GPU (`vulkan-intel`, `vulkan-radeon` or
  `nvidia-utils`). The harness also needs `sway`. `imagemagick` is optional.
- The first build compiles Bevy and takes a long time on a laptop. After that, changes to the
  game rebuild in seconds because the engine is linked as a shared library in dev builds.
- For faster linking, install `mold` and `clang` and copy the relevant lines from
  `.cargo/config_fast_builds.toml` into `.cargo/config.toml` (which is not committed).

## What the tests cannot tell you

- How aiming, recoil and movement *feel* with a real mouse.
- Sound.
- Two physical machines on a real network (the LAN scenario runs both sides on localhost).
- Frame rate on other hardware.

Say so when a change touches one of these, instead of reporting it verified.
