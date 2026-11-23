#!/bin/bash
# Measure frame and GPU times along a fixed route (tests/harness/bench.cfg).
#
#   scripts/bench.sh [options] [label]
#
#   -s WxH      render size (default 3840x2160)
#   --release   measure an optimized build (built if needed) instead of the dev build
#   -c "CMDS"   console commands to run first, ';'-separated, e.g. -c "r_ssao 0"
#   --window    run fullscreen on this desktop instead of an invisible display;
#               frame rates are only trustworthy this way. It takes over the
#               screen for about a minute.
#
# Results print as a table and are saved to target/bench/<date>-<label>.txt.
# GPU times are per render pass and don't depend on the display, so even an
# invisible run gives good GPU numbers; its frame rates are approximate.
set -u
cd "$(dirname "$0")/.." || exit 2

SIZE=3840x2160
RELEASE=0
WINDOW=0
LABEL=""
SETUP=""
while [ $# -gt 0 ]; do
    case $1 in
    -s) SIZE=$2; shift 2 ;;
    -c) SETUP=$2; shift 2 ;;
    --release) RELEASE=1; shift ;;
    --window) WINDOW=1; shift ;;
    -h | --help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "bench: unknown option $1" >&2; exit 2 ;;
    *) LABEL=$1; shift ;;
    esac
done

NAME=$(sed -n '/^\[package\]/,/^\[/s/^name *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
TARGET=${CARGO_TARGET_DIR:-target}
build="dev"
args=(-t 240 -s "$SIZE")
[ $WINDOW = 1 ] && args+=(-w)
if [ $RELEASE = 1 ]; then
    build="release"
    cargo build --release --no-default-features -q || exit 2
    export HARNESS_BIN=$TARGET/release/$NAME
fi
mode=$([ $WINDOW = 1 ] && echo "window" || echo "headless")
LABEL=${LABEL:-$build-$mode-$SIZE}

# With setup commands, run a copy of the route with them at the top.
script=tests/harness/bench.cfg
if [ -n "$SETUP" ]; then
    mkdir -p "$TARGET/bench"
    script=$TARGET/bench/bench-$LABEL.cfg
    { echo "$SETUP" | tr ';' '\n'; cat tests/harness/bench.cfg; } >"$script"
fi
scripts/harness.sh "${args[@]}" "$script" >/dev/null
status=$?
console=$TARGET/harness/latest/$(basename "$script" .cfg)/console.txt
[ -s "$console" ] || { echo "bench: the run produced no console output" >&2; exit 1; }

mkdir -p "$TARGET/bench"
out=$TARGET/bench/$(date +%Y%m%d-%H%M%S)-$LABEL.txt
{
    echo "bench $LABEL  ($build build, $mode, $SIZE, $(git rev-parse --short HEAD 2>/dev/null)$(git diff --quiet 2>/dev/null || echo '+changes'))"
    [ -n "$SETUP" ] && echo "setup: $SETUP"
    [ $WINDOW = 0 ] && echo "frame rates from an invisible display are approximate; GPU times are not"
    echo
    printf '%-16s %9s %7s %9s %9s %8s\n' view "avg ms" fps "p99 ms" "worst" "gpu ms"
    sed -n 's/^perf \([a-z_]*\): \([0-9.]*\) ms avg (\([0-9]*\) fps), \([0-9.]*\) ms p99, \([0-9.]*\) ms worst, \(gpu \([0-9.]*\) ms\|gpu n\/a\).*/\1 \2 \3 \4 \5 \7/p' "$console" |
        awk '{ printf "%-16s %9s %7s %9s %9s %8s\n", $1, $2, $3, $4, $5, ($6 == "" ? "n/a" : $6) }'
    echo
    echo "Slowest GPU passes per view (ms):"
    awk '/^perf [a-z_]+:/ { view = $2; sub(":", "", view) } /^  gpu: / { sub("^  gpu: ", ""); print "  " view ": " $0 }' "$console"
} | tee "$out"
echo
echo "saved $out"
[ $status = 0 ] || echo "note: the harness reported a failure; see target/harness/latest/result.txt"
exit $status
