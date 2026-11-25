#!/bin/bash
# Run console scripts in the browser version of the game and report PASS or
# FAIL, like scripts/harness.sh does for the native game.
#
#   scripts/web-harness.sh [options] script.cfg [script2.cfg ...]
#
# Builds the web version (scripts/web.sh), serves it with tests/ beside it,
# and opens each script in Chrome (/?c=exec <script>) inside its own
# invisible sway, so nothing appears on your desktop. Chrome runs with a
# real GPU there, which WebGPU needs.
#
# Options:
#   -n        don't rebuild; use what's in dist/web
#   -v        watch: show the browser in a nested sway window
#   -t SECS   give up on a script after this long (default 180)
#   -s WxH    browser size, and so the screenshot size (default 1280x720)
#   -c DIR    keep the browser's storage in DIR, shared by every script, to
#             test that settings survive a reload (default: fresh each time)
#   -L        serve the page from the online lobby (lobby/) instead, on
#             127.0.0.1:8098 with game ports 27570-27571 (WebTransport on
#             27670-27671), so `games`, `create` and `join` work. The lobby's
#             output, and its games', goes to lobby.log.
#   -h        this help
#
# Passes the same way as harness.sh: the script reached `quit`, the console
# printed no `error:` line, nothing panicked, and every `screenshot` it asked
# for was saved. Results go to target/web-harness/<run>/<script>/:
#   console.txt   the game's console
#   browser.log   everything else the browser printed
#   *.png         screenshots
#
# Needs: Chrome (/usr/bin/google-chrome-stable, or set CHROME), bun, sway.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 2

BUILD=1
MODE=headless
TIMEOUT=180
SIZE=1280x720
PROFILE=""
LOBBY=0
usage() { sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'; }
while getopts "nvt:s:c:Lh" opt; do
    case $opt in
    n) BUILD=0 ;;
    v) MODE=nested ;;
    t) TIMEOUT=$OPTARG ;;
    s) SIZE=$OPTARG ;;
    c) PROFILE=$(realpath -m "$OPTARG") ;;
    L) LOBBY=1 ;;
    h) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
    esac
done
shift $((OPTIND - 1))
[ $# -ge 1 ] || { usage >&2; exit 2; }
for script in "$@"; do
    [ -f "$script" ] || { echo "web-harness: no such script: $script" >&2; exit 2; }
done
for tool in bun sway python3; do
    command -v $tool >/dev/null || { echo "web-harness: $tool not found" >&2; exit 2; }
done

if [ $BUILD = 1 ]; then
    scripts/web.sh || { echo "web-harness: web build failed" >&2; exit 2; }
fi
[ -f dist/web/index.html ] || { echo "web-harness: no dist/web; build it first" >&2; exit 2; }
if [ ! -d scripts/web-harness/node_modules ]; then
    (cd scripts/web-harness && bun install --silent) || exit 2
fi

RUN_DIR=$ROOT/target/web-harness/$(date +%Y%m%d-%H%M%S)
mkdir -p "$RUN_DIR"
ln -sfn "$(basename "$RUN_DIR")" "$ROOT/target/web-harness/latest"
RT=$(mktemp -d /tmp/webh.XXXXXX)
PIDS=()
cleanup() {
    for pid in "${PIDS[@]:-}"; do [ -n "$pid" ] && kill "$pid" 2>/dev/null; done
    wait 2>/dev/null
    rm -rf "$RT"
}
trap cleanup EXIT

# The page, plus tests/ so `exec tests/harness/x.cfg` can fetch scripts.
SERVE=$RT/www
mkdir -p "$SERVE"
for f in "$ROOT"/dist/web/*; do ln -s "$f" "$SERVE/"; done
ln -s "$ROOT/tests" "$SERVE/tests"
if [ $LOBBY = 1 ]; then
    # The lobby serves the page, so the game finds it as its own origin, and
    # starts native dedicated servers (a dev build) for its games.
    cargo build -q -p lobby -p fast-fps || { echo "web-harness: build failed" >&2; exit 2; }
    TARGET=${CARGO_TARGET_DIR:-$ROOT/target}
    PORT=8098
    env "LD_LIBRARY_PATH=$TARGET/debug/deps:$TARGET/debug:$(rustc --print target-libdir)" \
        "FASTFPS_KEY=$(printf '5b%.0s' $(seq 32))" "FASTFPS_BIN=$TARGET/debug/fast-fps" \
        "WEB_ROOT=$SERVE" "XDG_CONFIG_HOME=$RUN_DIR/lobby-config" \
        PUBLIC_HOST=127.0.0.1 GAME_PORTS=27570-27571 LOBBY_ADDR=127.0.0.1:$PORT GAME_IDLE=6 \
        "$TARGET/debug/lobby" </dev/null >"$RUN_DIR/lobby.log" 2>&1 &
    PIDS+=($!)
    for _ in $(seq 100); do
        grep -q '^lobby on ' "$RUN_DIR/lobby.log" 2>/dev/null && break
        sleep 0.2
    done
    grep -q '^lobby on ' "$RUN_DIR/lobby.log" ||
        { echo "web-harness: the lobby did not start, see $RUN_DIR/lobby.log" >&2; exit 2; }
else
    PORT=8071
    python3 -m http.server $PORT --bind 127.0.0.1 --directory "$SERVE" >"$RUN_DIR/http.log" 2>&1 &
    PIDS+=($!)
fi

start_sway() {
    local dir=$1 log=$2
    mkdir -p "$dir" && chmod 700 "$dir"
    printf 'default_border none\noutput * resolution %s\n' "$SIZE" >"$dir/sway.conf"
    if [ "$MODE" = nested ]; then
        local host=${WAYLAND_DISPLAY:-}
        case $host in /*) ;; *) host=${XDG_RUNTIME_DIR:-}/$host ;; esac
        XDG_RUNTIME_DIR=$dir WAYLAND_DISPLAY=$host WLR_BACKENDS=wayland \
            sway --unsupported-gpu -c "$dir/sway.conf" >"$log" 2>&1 &
    else
        XDG_RUNTIME_DIR=$dir WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
            env -u WAYLAND_DISPLAY -u DISPLAY \
            sway --unsupported-gpu -c "$dir/sway.conf" >"$log" 2>&1 &
    fi
    PIDS+=($!)
    for _ in $(seq 100); do
        sleep 0.1
        SWAY_DISPLAY=$(cd "$dir" && ls wayland-* 2>/dev/null | grep -v '\.lock$' | head -1)
        [ -n "$SWAY_DISPLAY" ] && return 0
    done
    return 1
}

REPORT=$RUN_DIR/result.txt
: >"$REPORT"
say() { echo "$@" | tee -a "$REPORT"; }
overall=0
i=0
for script in "$@"; do
    i=$((i + 1))
    name=$(basename "$script" .cfg)
    out=$RUN_DIR/$name
    mkdir -p "$out"
    if ! start_sway "$RT/s$i" "$out/sway.log"; then
        echo "web-harness: sway did not start, see $out/sway.log" >&2
        exit 2
    fi
    rel=$(realpath --relative-to="$ROOT" "$script")
    url="http://127.0.0.1:$PORT/?c=name%20$name&c=exec%20$rel"
    echo "running $name in the browser"
    XDG_RUNTIME_DIR=$RT/s$i WAYLAND_DISPLAY=$SWAY_DISPLAY PROFILE_DIR=$PROFILE \
        bun scripts/web-harness/run.mjs "$url" "$out" "$TIMEOUT" "$SIZE" \
        >"$out/runner.log" 2>&1
    status=$?

    problems=()
    grep -q '^\] quit' "$out/console.txt" ||
        problems+=("didn't reach 'quit' ($(tail -1 "$out/browser.log" 2>/dev/null))")
    [ $status -le 3 ] || problems+=("the browser runner failed, see $out/runner.log")
    while IFS= read -r line; do
        [ -n "$line" ] && problems+=("$line")
    done < <(grep '^error: ' "$out/console.txt")
    while IFS= read -r line; do
        [ -n "$line" ] && problems+=("browser: $line")
    done < <(grep -E 'panicked at|pageerror' "$out/browser.log" | cut -c1-240 | head -5)
    shots=0
    while read -r shot; do
        [ -n "$shot" ] || continue
        if [ -s "$out/${shot%.png}.png" ]; then
            shots=$((shots + 1))
        else
            problems+=("screenshot '$shot' was not saved")
        fi
    done < <(sed -n 's/^[[:space:]]*screenshot[[:space:]]\+\([^[:space:];]\+\).*/\1/p' "$script")
    checks=$(grep -c -E '^(assert|wait_until) ok' "$out/console.txt")

    if [ ${#problems[@]} -eq 0 ]; then
        say "PASS  $name  ($checks checks, $shots screenshots)"
    else
        overall=1
        say "FAIL  $name  ($script)"
        for p in "${problems[@]}"; do say "      $p"; done
        say "      --- last console lines ($out/console.txt) ---"
        tail -6 "$out/console.txt" | sed 's/^/      /' | tee -a "$REPORT"
    fi
done

if [ $LOBBY = 1 ]; then
    lobby_problems=$(sed 's/\x1b\[[0-9;]*m//g' "$RUN_DIR/lobby.log" | grep -E 'panicked at| ERROR ' |
        grep -v 'Sub app did not exist' | cut -c1-240 | head -5)
    if [ -n "$lobby_problems" ]; then
        overall=1
        say "FAIL  lobby  ($RUN_DIR/lobby.log)"
        echo "$lobby_problems" | sed 's/^/      /' | tee -a "$REPORT"
    else
        say "PASS  lobby"
    fi
fi

say "results: $RUN_DIR"
if [ $overall = 0 ]; then say "RESULT: PASS"; else say "RESULT: FAIL"; fi
exit $overall
