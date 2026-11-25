# The online server: the lobby (lobby/), which starts a headless game
# server (fast-fps --dedicated) for each game and serves the browser version
# of the game. See docs/online.md and docs/web.md.
#
#   docker build -t fast-fps-server .
#   docker run -e FASTFPS_KEY=... -e PUBLIC_HOST=<your address> \
#       -p 8080:8080 -p 27100-27107:27100-27107/udp fast-fps-server

FROM rust:1-trixie AS chef
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /src

# The dependencies (Bevy and the rest) are built in a layer of their own,
# which a rebuild reuses unless Cargo.toml or Cargo.lock change.
FROM chef AS plan
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=plan /src/recipe.json recipe.json
# No default features: those are for development (the engine as a shared
# library, asset hot-reloading).
RUN cargo chef cook --release --no-default-features -p fast-fps --recipe-path recipe.json \
    && cargo chef cook --release -p lobby --recipe-path recipe.json
COPY . .
RUN cargo build --release --no-default-features -p fast-fps \
    && cargo build --release -p lobby

# The browser version (docs/web.md), served by the lobby at /.
FROM chef AS web
RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.130 --locked
# Must match the flags scripts/web.sh uses, or the cooked layer isn't reused.
ENV RUSTFLAGS='--cfg getrandom_backend="wasm_js"'
COPY --from=plan /src/recipe.json recipe.json
RUN cargo chef cook --profile web-release --target wasm32-unknown-unknown \
    --no-default-features --features web -p fast-fps --recipe-path recipe.json
COPY . .
RUN scripts/web.sh

FROM debian:trixie-slim
# What the game program loads, even with no window or sound to use them,
# plus curl for health checks (Coolify runs its own with curl or wget).
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libasound2t64 libudev1 libwayland-client0 libffi8 ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/fast-fps /src/target/release/lobby /app/
COPY --from=web /src/dist/web /app/web
COPY assets /app/assets
WORKDIR /app
ENV LOBBY_ADDR=0.0.0.0:8080 \
    GAME_PORTS=27100-27107 \
    BEVY_ASSET_ROOT=/app
EXPOSE 8080/tcp 27100-27107/udp
HEALTHCHECK --interval=30s --timeout=3s CMD ["curl", "-fsS", "http://127.0.0.1:8080/health"]
CMD ["/app/lobby"]
