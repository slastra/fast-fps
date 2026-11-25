# The browser version

The game also builds for the web: the same code compiled to WebAssembly, drawing with WebGPU.
It is served at `https://fps.lastra.us/` by the lobby (see [online.md](online.md)), and plays
offline against bots. Multiplayer in the browser needs a WebSocket transport, which isn't
built yet.

## Playing

Open the page in a browser with WebGPU (a recent Chrome or Edge; Firefox and Safari depend on
the version and the system). Click to play. The controls are the game's, with two
differences:

- **C crouches** (not Left Ctrl). In a browser, Ctrl+W closes the tab, and holding W to move
  makes that easy to hit by accident. A page can't stop it.
- **Esc lets go of the mouse** (the browser does that itself); click to take it again.

Settings are saved in the browser's local storage. Graphics start on `r_quality medium`; type
`r_quality high` for the full look if your machine keeps up.

The page's address can carry console lines, like launch arguments natively: each `c=` is one
line, so `https://fps.lastra.us/?c=exec%20cfg/bots.cfg&c=sky%20day` starts with bots, in
daylight.

## Building

```sh
scripts/web.sh            # build into dist/web/
scripts/web.sh --serve    # build, then serve on http://localhost:8070
```

It needs the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the version
`Cargo.lock` names; the script installs both if they are missing. With `wasm-opt` (the
`binaryen` package) the download gets smaller. The result is a static folder: `index.html`
(from `web/`), the game as `fast-fps_bg.wasm` with its JavaScript glue, gzipped copies of
both, and `cfg/`.

The web build uses `--no-default-features --features web`: the `dev` features (Bevy as a
shared library, file watching) can't target wasm, and `web` turns on Bevy's WebGPU renderer.
Randomness comes from the browser's crypto API, which takes one feature per `getrandom`
version in `Cargo.toml` and the `getrandom_backend` flag that `scripts/web.sh` sets.

## What is different in a browser

`src/platform.rs` holds every difference, each with a native and a web side:

| | Native | Browser |
|---|---|---|
| Saved settings | `config.cfg` | local storage |
| Launch lines | `+command` arguments | `?c=` in the URL |
| `exec` | reads the file | fetches it from the server; the script waits |
| Mouse capture | Bevy's state | asks the page, which lets go on Esc by itself |

Elsewhere:

- **No UDP, no files, no threads.** `host`, `connect` and `find` say multiplayer isn't in the
  browser yet (`net/transport.rs`). The dedicated server isn't built in. `quit` stops the game
  but can't close the tab.
- **Nothing is loaded at run time.** The sounds and the HUD font are built into the program.
- **`std::time::Instant` panics in a browser.** Use `bevy::platform::time::Instant`.
- **Sound starts on the first click.** Browsers keep audio off until the page is clicked;
  `web/index.html` wakes the game's audio then.
- **Shaders compile when first needed,** so the first seconds and the first shot or explosion
  may stutter.

## How it runs

Measured 2026-11-25 in Chrome 158 on the desk machine (RTX 4080 SUPER, 60 Hz display):

| | |
|---|---|
| Download | 50 MB of WebAssembly, 13 MB gzipped (what the lobby sends) |
| Frame rate, `low` / `medium` / `high` | 60 / 60 / 60, the browser's cap at 60 Hz |
| During a duel's smoke | about 50 |
| The first seconds | about 11, while shaders compile |

So `high` keeps up on a fast desktop. The browser starts on `medium` anyway, since a laptop is
the likelier visitor; nobody has measured one yet. Without `wasm-opt` (binaryen) installed the
download is a little larger than it could be.

## Testing in a browser

```sh
scripts/web-harness.sh tests/harness/smoke.cfg
```

This builds the web version, serves it with `tests/` beside it, and runs each script in
Chrome inside an invisible sway (a real GPU, which WebGPU needs), through Playwright
(`scripts/web-harness/`, run with bun). It judges a run exactly as `scripts/harness.sh`
does. The game prints its console to the browser's console with a `[fast-fps] ` prefix,
which becomes `console.txt`; `screenshot` saves arrive as downloads. `-n` skips the build,
`-v` shows the browser in a window.

CI checks that the game still compiles for wasm (`.github/workflows/ci.yaml`).
