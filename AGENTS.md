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
scripts/bench.sh --release        # frame, CPU and GPU times along a fixed route
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
| The sky, light, fog and mood of each time of day | `src/sky.rs` |
| The shapes things are built from (blocks, crates, trees) | `src/lowpoly.rs` |
| What gets sent over the network | `src/net/protocol.rs` |
| Game rules: damage, kills, respawns | `src/server/` |
| How other players look and move | `src/avatar/` |
| Keys and mouse | `src/player/input.rs` |
| The HUD | `src/hud/` |
| Console commands for testing | `src/debug/` |
| The headless server (`--dedicated`) | `src/dedicated.rs`, `src/main.rs` |
| Online play: the lobby and its commands | `lobby/`, `src/net/lobby.rs`, `docs/online.md` |
| The browser version, and anything that differs there | `src/platform.rs`, `web/`, `docs/web.md` |

## Rules of this codebase

- **The server decides.** Anything that affects other players (damage, score, spawning) runs
  in `src/server/`, guarded by `run_if(in_state(ClientState::Disconnected))`. That condition
  means "this machine is not a client", so it covers both hosting and offline play. A client
  only *asks*, by sending a message defined in `src/net/protocol.rs`.
- **Server code must not need the local player.** The same `src/server/` code runs in a
  dedicated server (`fast-fps --dedicated`), which has no `LocalPlayer`, camera, HUD, sky or
  sound. Anything server-side that reads those must cope with them being absent. Visual-only
  plugins are left out of the dedicated app in `src/main.rs` (and `world::plugin` skips its
  lamps and dummy dressing there).
- **The game also runs in a browser** (`scripts/web.sh`, `docs/web.md`). Anything that differs
  there goes in `src/platform.rs` with a native and a web side, so the rest stays one code
  path. CI lints the wasm build; check it yourself with
  `RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo clippy -p fast-fps --target
  wasm32-unknown-unknown --no-default-features --features web`.
- **Everything on the wire lives in `src/net/protocol.rs`** and is registered in its
  `register` function. Client and server must register the same things in the same order.
- **Console commands are the interface.** Register one with `app.console_command(name, usage,
  help)` and handle it by reading `ConsoleCommand` messages. Report problems with
  `Print::error(...)`, never a plain `Print`: the `error:` prefix is what fails a test.
- **Make new state checkable.** If you add something a test should be able to see, publish it
  to the probe (`probe.set("coins", n)`, see `src/debug/probe.rs`) so scripts can `assert` it.
- **Tunable numbers go in `src/config.rs`**, not scattered through the code.
- **The level has two random number generators.** `b.rng` places things with collision and
  must give the same map on every machine. `b.paint` is for shading and decoration. Never use
  `rng` for looks, or a cosmetic change will move the map (see `world/builder.rs`).
- **Light, fog and exposure belong to the sky.** Set them in a `Look` in `src/sky.rs`, not on
  the camera or in the level, or the next `sky` change will overwrite them.
- **One camera.** The first-person gun is drawn by the main camera, scaled toward the eye. Do
  not add a second camera for it (see Traps).
- **Measure before and after anything that could change speed:** lights, shadows,
  post-processing, the level, or anything spawned in numbers. Run `scripts/bench.sh
  --release` both times and put the two tables in the commit message. `docs/perf.md` says
  how, and what each setting costs. New expensive effects go off in the `low` and `medium`
  presets (`r_quality`, in `src/debug/switches.rs`).
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
- **Color grading contrast crushes dark scenes.** Even `contrast = 1.05` turned the whole
  world black at night. `sky.rs` only raises saturation.
- **A dark screenshot is not always a lighting bug, and vice versa.** To judge geometry, switch
  to `sky day` first; it is the clearest light.
- **The yard south of the keep is kept empty on purpose** (`in_yard` in `world/arena.rs`). The
  test scenarios use it as a stage; putting something there breaks them.
- **Frame rates from the invisible harness display are rough; GPU and CPU times are not.**
  Compare `gpu ms` and `cpu ms` between runs. Trust fps only from `bench.sh --window`.
- **A render diagnostic keeps its last value after its pass stops running.** `perf` only counts
  fresh measurements for that reason; keep it that way if you touch `debug/perf.rs`.
- **A dev build can't be run directly.** It links Bevy as a shared library, so
  `target/debug/fast-fps` fails with "cannot open shared object file". Use `cargo run --`, or
  set `LD_LIBRARY_PATH` as `scripts/harness.sh` does. Release builds don't have this problem.
- **In a browser, `std::time::Instant::now()` panics, and files, threads and UDP sockets don't
  exist.** Use `bevy::platform::time::Instant`; put anything else in `src/platform.rs`. It
  compiles fine natively, so only the wasm check or a browser run catches it.
- **A browser lets go of the mouse on Esc without telling the game,** and Ctrl+W closes the
  tab. `player/input.rs` handles both (it asks the page, and crouch is C there).
- **Two instances can't share a UDP port.** The LAN scenario uses 27555 so it can run beside a
  real game on the default 27015.

## Building

Run `scripts/install-deps.sh` first on Arch or CachyOS. It installs any missing system
packages (`--check` only reports).

- Linux packages (Arch/CachyOS names): `base-devel alsa-lib systemd-libs wayland libxkbcommon
  vulkan-icd-loader` plus a Vulkan driver for the GPU (`vulkan-intel`, `vulkan-radeon` or
  `nvidia-utils`). The harness also needs `sway`. `imagemagick` is optional.
- The first build compiles Bevy and takes a long time on a laptop. After that, changes to the
  game rebuild in seconds because the engine is linked as a shared library in dev builds.
- For faster linking, install `mold` and `clang` and copy the relevant lines from
  `.cargo/config_fast_builds.toml` into `.cargo/config.toml` (which is not committed).

## On a slow or small machine

On a laptop with a couple of cores and 8 GB of memory the first build can take an hour or
more. That is the engine compiling, once. Until it finishes there is nothing to run.

- **Never start a second build while one is running.** Check with `pgrep -a cargo`. A second
  `cargo build`, `cargo run`, `cargo test`, `cargo clippy` or `scripts/check.sh` doesn't help;
  the two fight over memory and both get slower. Wait for the first, or stop it.
- **Don't treat a long, quiet build as a hang** and don't cancel and restart it. Restarting
  throws nothing away, but it doesn't speed anything up either.
- **After the first build, `cargo run`, `cargo test` and `scripts/harness.sh` rebuild only the
  game**, which is far quicker.
- **`cargo check` and `cargo clippy` make their own, lighter pass over the engine the first
  time they run**, so the first `scripts/check.sh` is slow too. Run it when the machine is
  otherwise idle. After that, `cargo check` is the fastest way to find compile errors.
- **If the game is slow to play,** `r_quality low` in the console (or `exec cfg/low.cfg`)
  turns off the expensive graphics, and is saved. The harness can be given a longer timeout with `-t 300`.

## What the tests cannot tell you

- How aiming, recoil and movement *feel* with a real mouse.
- Sound.
- Two physical machines on a real network (the LAN scenario runs both sides on localhost).
- Online play over the internet: latency, packet loss, players' home routers. The online
  scenario runs the lobby, its games and both players on localhost.
- Frame rate on other hardware, and in browsers other than Chrome.
- How the browser version feels and sounds. `scripts/web-harness.sh` runs scripts in Chrome,
  but nobody is holding the mouse.

Say so when a change touches one of these, instead of reporting it verified.
