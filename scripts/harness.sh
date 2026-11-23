#!/bin/bash
# Run fast-fps scripts in a throwaway sway, collect screenshots and logs.
#
#   scripts/harness.sh [-v] [-t SECS] [-s WxH] script.cfg [script2.cfg ...]
#
# Each script is a file of console commands run with `exec` (see
# tests/harness/). One game instance starts per script, a second apart, so a
# host script can come first and client scripts after it. Scripts should end
# with `quit`; anything still running after the timeout is killed.
#
#   -v        visible: nested sway in a window on the desktop (default headless)
#   -t SECS   timeout for the whole run (default 90)
#   -s WxH    output size (default 1280x720)
#   -c DIR    config home to reuse (default: fresh per instance, kept in the
#             run dir), e.g. to test that settings survive a restart
#
# Results land in target/harness/<run>/<script-name>/ (shots + game.log), and
# the run directory is printed at the end.
#
# Why sway: the game grabs the pointer and wants a real Wayland surface, and
# none of that should touch the live Hyprland session. Nothing here injects
# input; the scripts drive the game from inside.
set -u
cd "$(dirname "$0")/.." || exit 1

VISIBLE=0
TIMEOUT=90
SIZE=1280x720
CONFIG_BASE=""
while getopts "vt:s:c:" opt; do
    case $opt in
    v) VISIBLE=1 ;;
    t) TIMEOUT=$OPTARG ;;
    s) SIZE=$OPTARG ;;
    c) CONFIG_BASE=$(realpath -m "$OPTARG") ;;
    *) exit 2 ;;
    esac
done
shift $((OPTIND - 1))
[ $# -ge 1 ] || { sed -n '2,20p' "$0"; exit 2; }

cargo build -q || exit 1

RUN_DIR=$PWD/target/harness/$(date +%Y%m%d-%H%M%S)
mkdir -p "$RUN_DIR"

# Short private runtime dir: unix socket paths cap at 108 bytes.
RT=$(mktemp -d /tmp/ffh.XXXX)
chmod 700 "$RT"
cat >"$RT/sway.conf" <<CONF
default_border none
output * resolution $SIZE
CONF

if [ "$VISIBLE" = 1 ]; then
    SWAY_ENV=(WLR_BACKENDS=wayland WAYLAND_DISPLAY="${WAYLAND_DISPLAY:?}")
else
    SWAY_ENV=(WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1)
fi
env "${SWAY_ENV[@]}" XDG_RUNTIME_DIR="$RT" \
    sway --unsupported-gpu -c "$RT/sway.conf" >"$RUN_DIR/sway.log" 2>&1 &
SWAY_PID=$!

cleanup() {
    for pid in "${GAME_PIDS[@]:-}"; do kill "$pid" 2>/dev/null; done
    kill "$SWAY_PID" 2>/dev/null
    wait 2>/dev/null
    rm -rf "$RT"
}
trap cleanup EXIT

DISPLAY_NAME=""
for _ in $(seq 100); do
    sleep 0.1
    DISPLAY_NAME=$(cd "$RT" && ls wayland-* 2>/dev/null | grep -v lock | head -1)
    [ -n "$DISPLAY_NAME" ] && break
done
[ -n "$DISPLAY_NAME" ] || { echo "sway did not start, see $RUN_DIR/sway.log" >&2; exit 1; }

GAME_PIDS=()
for script in "$@"; do
    name=$(basename "$script" .cfg)
    out=$RUN_DIR/$name
    mkdir -p "$out"
    XDG_RUNTIME_DIR="$RT" WAYLAND_DISPLAY="$DISPLAY_NAME" \
        XDG_CONFIG_HOME="${CONFIG_BASE:-$out/config}" XDG_STATE_HOME="$out/state" \
        FASTFPS_SHOT_DIR="$out" \
        cargo run -q -- +name "$name" +exec "$script" >"$out/game.log" 2>&1 &
    GAME_PIDS+=($!)
    echo "started $name"
    sleep 1
done

deadline=$((SECONDS + TIMEOUT))
for pid in "${GAME_PIDS[@]}"; do
    while kill -0 "$pid" 2>/dev/null; do
        if [ $SECONDS -ge $deadline ]; then
            echo "timeout after ${TIMEOUT}s, killing $pid" >&2
            kill "$pid" 2>/dev/null
            break
        fi
        sleep 0.5
    done
done

for script in "$@"; do
    name=$(basename "$script" .cfg)
    out=$RUN_DIR/$name
    # Console output only, without ANSI colors.
    sed 's/\x1b\[[0-9;]*m//g' "$out/game.log" | grep -E 'fast_fps::console|ERROR|panicked' \
        | sed -E 's/^.*fast_fps::console: //' >"$out/console.txt"
    errors=$(grep -c -E 'ERROR|panicked' "$out/game.log")
    shots=$(ls "$out"/*.png 2>/dev/null | wc -l)
    echo "$name: $shots screenshot(s), $errors error line(s)"
done
echo "$RUN_DIR"
