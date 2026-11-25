#!/bin/bash
# Build the browser version of the game into dist/web/.
#
#   scripts/web.sh            build
#   scripts/web.sh --serve    build, then serve it on http://localhost:8070
#
# The result is a static folder: index.html, the game as fast-fps_bg.wasm and
# its JavaScript glue, gzipped copies of both, and the cfg/ scripts. Any web
# server can serve it; the lobby does at https://fps.lastra.us/. It needs a
# browser with WebGPU. See docs/web.md.
#
# Needs: the wasm32-unknown-unknown target and wasm-bindgen-cli at the same
# version as the wasm-bindgen crate in Cargo.lock (both installed if missing).
# Optional: wasm-opt (binaryen) makes the download smaller.
set -eu
cd "$(dirname "$0")/.."

SERVE=0
PROFILE=web-release
for arg in "$@"; do
    case $arg in
    --serve) SERVE=1 ;;
    *) echo "web.sh: unknown option $arg" >&2; exit 2 ;;
    esac
done

NAME=$(sed -n '/^\[package\]/,/^\[/s/^name *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
TARGET_DIR=${CARGO_TARGET_DIR:-target}
OUT=dist/web
export PATH="$HOME/.cargo/bin:$PATH"

rustup target list --installed | grep -qx wasm32-unknown-unknown ||
    rustup target add wasm32-unknown-unknown
WANT=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p')
HAVE=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$HAVE" != "$WANT" ]; then
    echo "installing wasm-bindgen-cli $WANT (have: ${HAVE:-none})"
    cargo install wasm-bindgen-cli --version "$WANT" --locked
fi

# Randomness in a browser comes from its crypto API; getrandom 0.3 is told so
# with this flag (0.2 with a feature, in Cargo.toml). Added only once, so a
# caller that already set it (the Dockerfile) keeps the same flags, and so
# its cached build.
case "${RUSTFLAGS:-}" in
*getrandom_backend*) ;;
*) export RUSTFLAGS="${RUSTFLAGS:-} --cfg getrandom_backend=\"wasm_js\"" ;;
esac
cargo build -p "$NAME" --target wasm32-unknown-unknown --profile "$PROFILE" \
    --no-default-features --features web
WASM=$TARGET_DIR/wasm32-unknown-unknown/$PROFILE/$NAME.wasm

rm -rf "$OUT"
mkdir -p "$OUT"
# Without the function names (half the file), a panic in the browser shows
# addresses rather than names; a native build is the place to debug.
wasm-bindgen --target web --no-typescript --remove-name-section --remove-producers-section \
    --out-dir "$OUT" --out-name "$NAME" "$WASM"
if command -v wasm-opt >/dev/null; then
    wasm-opt -O2 --enable-bulk-memory --enable-nontrapping-float-to-int \
        "$OUT/${NAME}_bg.wasm" -o "$OUT/${NAME}_bg.wasm"
fi
cp web/index.html "$OUT/"
cp -r cfg "$OUT/"
# Served as is to browsers that accept gzip (most of the download is the wasm).
gzip -9 -k "$OUT/${NAME}_bg.wasm" "$OUT/$NAME.js"

echo "Built $OUT:"
du -h "$OUT/${NAME}_bg.wasm" "$OUT/${NAME}_bg.wasm.gz" | sed 's/^/  /'

if [ $SERVE = 1 ]; then
    echo "Serving on http://localhost:8070 (Ctrl+C stops)"
    exec python3 -m http.server 8070 --bind 127.0.0.1 --directory "$OUT"
fi
