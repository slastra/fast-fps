#!/bin/bash
# Run the game from console scripts and report PASS or FAIL.
#
#   scripts/harness.sh [options] script.cfg [script2.cfg ...]
#
# Each script is a file of console commands (see tests/harness/ and
# docs/harness.md). One game instance starts per script, in the order given,
# each once the one before it has loaded, so a host script comes first and its
# clients after it.
# Every instance gets its own invisible display, so nothing appears on your
# desktop and no input is injected: scripts drive the game from the inside.
#
# Options:
#   -v        watch: show each instance in a nested sway window
#   -w        run in normal windows on this desktop (no sway needed)
#   -t SECS   give up after this long (default 120)
#   -s WxH    display size, which is also the screenshot size (default 1280x720)
#   -c DIR    settings folder to use, to test that settings survive a restart
#             (default: a fresh one per instance)
#   -h        this help
#
# An instance passes when its script reached `quit`, the game printed no
# `error:` line (failed asserts, unknown commands, bad arguments), nothing
# panicked or logged an ERROR, and every `screenshot` it asked for exists.
#
# Results go to target/harness/<run>/<script>/ (also target/harness/latest):
#   console.txt   what the console printed, in order
#   game.log      the full log
#   *.png         screenshots
# plus result.txt for the whole run.
#
# Exit status: 0 all passed, 1 something failed, 2 couldn't start.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 2

MODE=headless
TIMEOUT=120
SIZE=1280x720
CONFIG_BASE=""
usage() { sed -n '2,33p' "$0" | sed 's/^# \{0,1\}//'; }
while getopts "vwt:s:c:h" opt; do
    case $opt in
    v) MODE=nested ;;
    w) MODE=window ;;
    t) TIMEOUT=$OPTARG ;;
    s) SIZE=$OPTARG ;;
    c) CONFIG_BASE=$(realpath -m "$OPTARG") ;;
    h) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
    esac
done
shift $((OPTIND - 1))
[ $# -ge 1 ] || { usage >&2; exit 2; }
for script in "$@"; do
    [ -f "$script" ] || { echo "harness: no such script: $script" >&2; exit 2; }
    last=$(grep -v -E '^[[:space:]]*(//|#|$)' "$script" | tail -1 | tr -d '[:space:]')
    if [ "$last" != quit ]; then
        echo "harness: $script must end with 'quit', or the game never exits" >&2
        exit 2
    fi
done

need() { command -v "$1" >/dev/null 2>&1; }
need cargo || { echo "harness: cargo not found" >&2; exit 2; }
if [ "$MODE" != window ] && ! need sway; then
    echo "harness: sway is needed for invisible runs." >&2
    echo "  Install it (Arch/CachyOS: sudo pacman -S sway), or pass -w to use normal windows." >&2
    exit 2
fi
if [ "$MODE" != headless ] && [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
    echo "harness: -v and -w need a running desktop session" >&2
    exit 2
fi

# The package name is the binary name, and names the log lines we read.
NAME=$(sed -n '/^\[package\]/,/^\[/s/^name *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$NAME" ] || { echo "harness: can't read the package name from Cargo.toml" >&2; exit 2; }
LOG_TARGET="${NAME//-/_}::console"

echo "building $NAME..."
if ! cargo build -q; then
    echo "harness: build failed" >&2
    exit 2
fi
BIN=$ROOT/target/debug/$NAME
# Dev builds link Bevy dynamically; `cargo run` sets this up, so we do too.
LIBS="$ROOT/target/debug/deps:$ROOT/target/debug:$(rustc --print target-libdir)"

RUN_DIR=$ROOT/target/harness/$(date +%Y%m%d-%H%M%S)
[ -e "$RUN_DIR" ] && RUN_DIR=$RUN_DIR-$$
mkdir -p "$RUN_DIR"
ln -sfn "$(basename "$RUN_DIR")" "$ROOT/target/harness/latest"

# Short private runtime dirs: unix socket paths cap at 108 bytes.
RT=$(mktemp -d /tmp/harness.XXXXXX)
HOST_RUNTIME=${XDG_RUNTIME_DIR:-}
HOST_WAYLAND=${WAYLAND_DISPLAY:-}

GAME_PIDS=()
SWAY_PIDS=()
cleanup() {
    for pid in "${GAME_PIDS[@]:-}" "${SWAY_PIDS[@]:-}"; do
        [ -n "$pid" ] && kill "$pid" 2>/dev/null
    done
    wait 2>/dev/null
    rm -rf "$RT"
}
trap cleanup EXIT

# Start a sway for one instance and print its display name.
start_sway() {
    local dir=$1 log=$2
    mkdir -p "$dir" && chmod 700 "$dir"
    printf 'default_border none\noutput * resolution %s\n' "$SIZE" >"$dir/sway.conf"
    if [ "$MODE" = nested ]; then
        # A nested sway is a client of the real compositor, found by full path.
        local host=$HOST_WAYLAND
        case $host in /*) ;; *) host=$HOST_RUNTIME/$host ;; esac
        XDG_RUNTIME_DIR=$dir WAYLAND_DISPLAY=$host WLR_BACKENDS=wayland \
            sway --unsupported-gpu -c "$dir/sway.conf" >"$log" 2>&1 &
    else
        # WLR_LIBINPUT_NO_DEVICES keeps it away from the real keyboard and mouse.
        XDG_RUNTIME_DIR=$dir WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
            env -u WAYLAND_DISPLAY -u DISPLAY \
            sway --unsupported-gpu -c "$dir/sway.conf" >"$log" 2>&1 &
    fi
    SWAY_PIDS+=($!)
    for _ in $(seq 100); do
        sleep 0.1
        local socket
        socket=$(cd "$dir" && ls wayland-* 2>/dev/null | grep -v '\.lock$' | head -1)
        [ -n "$socket" ] && { SWAY_DISPLAY=$socket; return 0; }
    done
    return 1
}

NAMES=()
SCRIPTS=()
i=0
for script in "$@"; do
    i=$((i + 1))
    name=$(basename "$script" .cfg)
    # The same script twice (two clients) gets numbered folders.
    case " ${NAMES[*]:-} " in *" $name "*) name=$name-$i ;; esac
    out=$RUN_DIR/$name
    mkdir -p "$out"
    NAMES+=("$name")
    SCRIPTS+=("$script")

    game_env=(
        "LD_LIBRARY_PATH=$LIBS${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
        "HARNESS=1"
        "BEVY_ASSET_ROOT=$ROOT"
        "SCREENSHOT_DIR=$out"
        "XDG_CONFIG_HOME=${CONFIG_BASE:-$out/config}"
        "XDG_STATE_HOME=$out/state"
    )
    if [ "$MODE" != window ]; then
        if ! start_sway "$RT/$i" "$out/sway.log"; then
            echo "harness: sway did not start, see $out/sway.log" >&2
            exit 2
        fi
        game_env+=("XDG_RUNTIME_DIR=$RT/$i" "WAYLAND_DISPLAY=$SWAY_DISPLAY")
        env -u DISPLAY "${game_env[@]}" "$BIN" +name "$name" +exec "$script" \
            >"$out/game.log" 2>&1 &
    else
        env "${game_env[@]}" "$BIN" +name "$name" +exec "$script" >"$out/game.log" 2>&1 &
    fi
    GAME_PIDS+=($!)
    echo "started $name"
    # Let this instance load and begin its script before starting the next,
    # so a host is listening by the time its clients try to connect.
    if [ $i -lt $# ]; then
        for _ in $(seq 300); do
            # (The live log still has color codes, so match the message only.)
            grep -q -F "] exec " "$out/game.log" 2>/dev/null && break
            kill -0 "${GAME_PIDS[-1]}" 2>/dev/null || break
            sleep 0.2
        done
        sleep 1
    fi
done

# Wait for every instance, up to the deadline.
deadline=$((SECONDS + TIMEOUT))
STATUS=()
TIMED_OUT=()
for idx in "${!GAME_PIDS[@]}"; do
    pid=${GAME_PIDS[$idx]}
    timed_out=0
    while kill -0 "$pid" 2>/dev/null; do
        if [ $SECONDS -ge $deadline ]; then
            timed_out=1
            kill "$pid" 2>/dev/null
            break
        fi
        sleep 0.2
    done
    wait "$pid" 2>/dev/null
    STATUS[$idx]=$?
    TIMED_OUT[$idx]=$timed_out
done

# Judge each instance.
REPORT=$RUN_DIR/result.txt
: >"$REPORT"
say() { echo "$@" | tee -a "$REPORT"; }
overall=0
for idx in "${!NAMES[@]}"; do
    name=${NAMES[$idx]}
    script=${SCRIPTS[$idx]}
    out=$RUN_DIR/$name
    sed 's/\x1b\[[0-9;]*m//g' "$out/game.log" >"$out/game.plain"
    mv "$out/game.plain" "$out/game.log"
    grep -F "$LOG_TARGET: " "$out/game.log" | sed "s/^.*$LOG_TARGET: //" >"$out/console.txt"

    problems=()
    notes=()
    finished=0
    grep -q '^\] quit' "$out/console.txt" && finished=1

    if [ "${TIMED_OUT[$idx]}" = 1 ]; then
        problems+=("timed out after ${TIMEOUT}s without reaching 'quit'")
    elif [ $finished = 0 ]; then
        problems+=("the game stopped (exit status ${STATUS[$idx]}) before the script reached 'quit'")
    elif [ "${STATUS[$idx]}" != 0 ]; then
        notes+=("exit status ${STATUS[$idx]} after 'quit' (a crash during shutdown; not counted)")
    fi
    while IFS= read -r line; do
        [ -n "$line" ] && problems+=("$line")
    done < <(grep '^error: ' "$out/console.txt")
    while IFS= read -r line; do
        [ -n "$line" ] && problems+=("log: $line")
    done < <(grep -E 'panicked at| ERROR ' "$out/game.log" | cut -c1-240 | head -5)

    shots=0
    while read -r shot; do
        [ -n "$shot" ] || continue
        file=$out/${shot%.png}.png
        if [ ! -s "$file" ]; then
            problems+=("screenshot '$shot' was not saved")
            continue
        fi
        shots=$((shots + 1))
        if need magick; then
            spread=$(magick "$file" -colorspace Gray -format '%[fx:standard_deviation]' info: 2>/dev/null)
            case $spread in 0 | 0.000* | 0.001* | 0.002*)
                notes+=("screenshot '$shot' looks blank (one flat color)") ;;
            esac
        fi
    done < <(sed -n 's/^[[:space:]]*screenshot[[:space:]]\+\([^[:space:];]\+\).*/\1/p' "$script")
    checks=$(grep -c -E '^(assert|wait_until) ok' "$out/console.txt")

    if [ ${#problems[@]} -eq 0 ]; then
        say "PASS  $name  ($checks checks, $shots screenshots)"
    else
        overall=1
        say "FAIL  $name  ($script)"
        for p in "${problems[@]}"; do say "      $p"; done
        if [ -s "$out/console.txt" ]; then
            say "      --- last console lines ($out/console.txt) ---"
            tail -8 "$out/console.txt" | sed 's/^/      /' | tee -a "$REPORT"
        else
            say "      --- the console printed nothing; last log lines ($out/game.log) ---"
            tail -5 "$out/game.log" | cut -c1-240 | sed 's/^/      /' | tee -a "$REPORT"
        fi
    fi
    for n in "${notes[@]:-}"; do [ -n "$n" ] && say "      note: $n"; done
done

say "results: $RUN_DIR"
if [ $overall = 0 ]; then say "RESULT: PASS"; else say "RESULT: FAIL"; fi
exit $overall
