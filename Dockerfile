# The online server: the lobby (lobby/), which starts a headless game
# server (fast-fps --dedicated) for each game. See docs/online.md.
#
#   docker build -t fast-fps-server .
#   docker run -e FASTFPS_KEY=... -e PUBLIC_HOST=<your address> \
#       -p 8080:8080 -p 27100-27107:27100-27107/udp fast-fps-server

FROM rust:1-trixie AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# No default features: those are for development (the engine as a shared
# library, asset hot-reloading).
RUN cargo build --release --no-default-features -p fast-fps \
    && cargo build --release -p lobby

FROM debian:trixie-slim
# What the game program loads, even with no window or sound to use them.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libasound2t64 libudev1 libwayland-client0 libffi8 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/fast-fps /src/target/release/lobby /app/
COPY assets /app/assets
WORKDIR /app
ENV LOBBY_ADDR=0.0.0.0:8080 \
    GAME_PORTS=27100-27107 \
    BEVY_ASSET_ROOT=/app
EXPOSE 8080/tcp 27100-27107/udp
HEALTHCHECK --interval=30s --timeout=3s CMD ["/bin/bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080"]
CMD ["/app/lobby"]
