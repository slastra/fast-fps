# Playing online

On a LAN, one player hosts and the others join (`host`, `connect`). Online, nobody has to host:
a server runs the games, and a lobby on it lists them and starts new ones.

```
your game                       the server (one Docker container)
  games / create / join  ──HTTPS──>  lobby          lists games, starts them,
                                       │             hands out tickets
                                       ├─ fast-fps --dedicated --port 27100   one game
  connect with the ticket ──UDP────>   ├─ fast-fps --dedicated --port 27101   another
                                       └─ ...
```

- **The lobby** (`lobby/`) is a small web service with no Bevy in it. `create` asks it to start
  a game; it starts a headless game server for it on a free UDP port. `join` asks it for a
  ticket into a game.
- **A game** is the game's own server code running headless (`fast-fps --dedicated`,
  `src/dedicated.rs`), one process per game. It shuts itself down after two minutes with
  nobody in it, and the lobby forgets it.
- **A ticket** is a netcode connect token, signed with a key the lobby and its games share
  (`FASTFPS_KEY`). A game started by the lobby turns away anyone without one, so the only way
  in is through the lobby. A ticket is good for 30 seconds.
- **Whoever creates a game runs it.** `mode`, `bot`, `bot_do` and `bot_duel_aim` work for them
  as if they were hosting: their game sends the command to the server, which runs it and
  sends back what it printed (`src/net/admin.rs`, `src/server/admin.rs`). If they leave, the
  player who has been there longest takes over.
- **The lobby also serves the browser version** of the game at `/` (`WEB_ROOT`, see
  [web.md](web.md)), so `https://fps.lastra.us/` is a page you can play in.
- **Versions must match.** The lobby asks the game program which protocol it speaks, and a
  player whose game was built from different code is told to update it, instead of failing to
  connect.

## Playing

```
games                   list the games
create duel Fast hands  start a game (ffa or duel, then an optional name) and join it
join fast               join by name, or the start of it, or by number
lobby                   show which lobby the game uses
lobby <url>             use another one (saved)
```

## Running a lobby

### On your own machine

```sh
cargo build --workspace
FASTFPS_KEY=$(openssl rand -hex 32) LOBBY_ADDR=127.0.0.1:8099 \
  FASTFPS_BIN=target/debug/fast-fps \
  LD_LIBRARY_PATH=target/debug/deps:target/debug:$(rustc --print target-libdir) \
  target/debug/lobby
```

Then in the game: `lobby http://127.0.0.1:8099`, `create`. (`LD_LIBRARY_PATH` is only for a
dev build, which links Bevy as a shared library. A release build doesn't need it.)
`scripts/harness.sh -L` does all this for the test scenarios.

### In Docker

The `Dockerfile` builds the game (headless use only; it never opens a window) and the lobby
into one image:

```sh
docker build -t fast-fps-server .
docker run -e FASTFPS_KEY=$(openssl rand -hex 32) -e PUBLIC_HOST=<address players use> \
  -p 8080:8080 -p 27100-27107:27100-27107/udp fast-fps-server
```

| Variable | |
|---|---|
| `FASTFPS_KEY` | 64 hex characters. Keep it secret: anyone with it can make tickets. Required. |
| `PUBLIC_HOST` | The address players reach the games at: the server's public IP or a name for it. Tickets name it, and games check they do. Default `127.0.0.1`. |
| `GAME_PORTS` | The UDP ports for games, one game each. Default `27100-27107`, so at most 8 games. |
| `LOBBY_ADDR` | Where the lobby listens for HTTP. Default `0.0.0.0:8080`. |
| `FASTFPS_BIN` | The game program. Default: `fast-fps` next to the lobby. |
| `GAME_IDLE` | Seconds an empty game lasts. Default: the game's own `IDLE_SECS` (120). |

Put the lobby behind HTTPS (a reverse proxy such as Coolify's). The game ports are plain UDP
and must be published directly, not through the proxy, and open in the server's firewall.

### How fps.lastra.us is hosted

- `.github/workflows/server-image.yml` builds the image on every push to `master` and
  publishes it as `ghcr.io/slastra/fast-fps-server` (`latest` and the commit's hash). The
  Dockerfile builds the dependencies in a layer of their own, which later builds reuse, so
  only the first build compiles all of Bevy.
- Coolify (the main instance, project `fast-fps`) runs that image: the lobby on port 8080
  behind its proxy at `https://fps.lastra.us`, and UDP 27100-27107 published straight to the
  host. `FASTFPS_KEY` is set there as a secret, and `PUBLIC_HOST` is `fps.lastra.us`.
- With the `COOLIFY_DEPLOY_URL` and `COOLIFY_TOKEN` repository secrets set, the workflow tells
  Coolify to redeploy after each build, so the server always matches `master`. A player
  whose game is older is told to update it.
- The UDP range is open in the Linode firewall. The DNS record is not proxied by Cloudflare,
  since the game ports must reach the host directly.

### Limits

- One new game per address every 10 seconds, at most one per game port, 16 players a game.
- Game names are cut to 24 printable characters.
- The lobby trusts `X-Forwarded-For` for the caller's address, so it should only be reachable
  through the proxy.

## Not covered by the tests

`tests/harness/online_*.cfg` runs the lobby, a game and two players on one machine. Latency,
packet loss and players' home routers on the real internet are not tested.
