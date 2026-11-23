#!/bin/bash
# Make a standalone copy of the game to give to someone, so they can play
# without building anything.
#
#   scripts/package.sh
#
# Produces dist/<name>/ (a folder you can run in place) and
# dist/<name>-linux-x86_64.tar.gz (the same folder, packed). The copy holds
# the program, its assets, the console scripts in cfg/, and install-deps.sh.
#
# The program is built on this machine, so it runs on Linux systems about as
# new as this one. It still needs a Vulkan driver and the usual system
# libraries; install-deps.sh in the folder checks for them.
set -eu
cd "$(dirname "$0")/.."

NAME=$(sed -n '/^\[package\]/,/^\[/s/^name *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
TARGET=${CARGO_TARGET_DIR:-target}

# No default features: those are for development (the engine as a shared
# library, asset hot-reloading) and don't belong in a copy you hand out.
cargo build --release --no-default-features

OUT=dist/$NAME
rm -rf "$OUT"
mkdir -p "$OUT"
cp "$TARGET/release/$NAME" "$OUT/"
strip "$OUT/$NAME" 2>/dev/null || true
cp -r assets cfg "$OUT/"
cp scripts/install-deps.sh "$OUT/"
cat >"$OUT/README.txt" <<TXT
$NAME

Run it from this folder:

    ./$NAME

Click to grab the mouse. WASD to move, Space to jump, Ctrl to crouch, left
click to fire, R to reload, 1-5 to switch weapons, Tab for scores. Press ~
for the console, where everything else lives:

    host                 start a LAN game
    connect              join the first LAN game found (or: connect <ip>)
    exec cfg/bots.cfg    practice bots to shoot at
    exec cfg/low.cfg     lighter graphics, if it runs slowly
    sky next             change the time of day
    help                 every command

If it doesn't start, run ./install-deps.sh --check to see what is missing
(Arch and CachyOS).

This is a finished build. It doesn't change when the source code changes.
TXT
tar -C dist -czf "dist/$NAME-linux-x86_64.tar.gz" "$NAME"
echo "Packaged:"
du -sh "$OUT" "dist/$NAME-linux-x86_64.tar.gz"
