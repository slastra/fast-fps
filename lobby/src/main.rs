//! The lobby: a small HTTP service that lists fast-fps games, starts a
//! dedicated server (`fast-fps --dedicated`) for each new one, and hands
//! players the connect tokens those servers require.
//!
//! | Request | Answer |
//! |---|---|
//! | `GET /health` | `ok` |
//! | `GET /games` | `{protocol, games: [{id, name, mode, players, max}]}` |
//! | `POST /games {name, mode, player, protocol, transport}` | starts a game: `{id, token}` |
//! | `POST /games/<id>/join {player, protocol, transport}` | `{token}` |
//!
//! `transport` is `udp` (the native game, the default) or `webtransport` (a
//! browser). A webtransport ticket also carries `wt_addr` and `cert_hash`:
//! where the game takes browser players, and the SHA-256 of its self-signed
//! certificate, which the browser checks instead of a certificate authority.
//!
//! `protocol` is the client's Replicon protocol hash. A client built from
//! different code gets "update your game" instead of a failed connection.
//! A token is a netcode connect token, hex encoded, good for
//! `TOKEN_SECS`. The player who creates a game gets one marked admin (see
//! [`USER_DATA_ADMIN`]).
//!
//! A game exits on its own once it has been empty for a while, and the
//! lobby forgets it. Games are started with `--owned`, so they also exit if
//! the lobby does.
//!
//! Configured by environment variables:
//!
//! | | |
//! |---|---|
//! | `FASTFPS_KEY` | 64 hex characters, shared with the games (required) |
//! | `PUBLIC_HOST` | the address players reach games at (default `127.0.0.1`) |
//! | `GAME_PORTS` | UDP ports for games, `27100-27107`; one game each |
//! | `LOBBY_ADDR` | where to listen for HTTP (default `0.0.0.0:8080`) |
//! | `FASTFPS_BIN` | the game program (default: `fast-fps` beside this one) |
//! | `GAME_IDLE` | seconds an empty game lasts (default: the game's own) |
//! | `WT_PORT_OFFSET` | each game also takes browser players over WebTransport on its port plus this (default `100`; `off` to disable) |
//! | `WEB_ROOT` | the browser version to serve at `/` (default: `web` beside this program) |
//!
//! Any other `GET` serves a file from `WEB_ROOT`: the browser version of the
//! game (`scripts/web.sh`), with `/` as its `index.html`.

use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, BufReader, Read},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime},
};

use rand::Rng;
use renetcode2::{ConnectToken, NETCODE_USER_DATA_BYTES};
use serde::Deserialize;
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server};

/// How long a connect token stays valid. Matches `CONNECT_TOKEN_SECS` in the
/// game's `config.rs`, which the server checks tokens against.
const TOKEN_SECS: u64 = 30;
/// How long a client waits on a silent server before giving up.
const TIMEOUT_SECS: i32 = 15;
/// Byte 0 of a token's user data: 1 if this player runs the game. The game
/// reads it in `server/admin.rs`.
pub const USER_DATA_ADMIN: usize = 0;
/// Player ids the game keeps for bots (`Player::BOT_OWNER` in
/// `net/protocol.rs`). Clients never get one.
const BOT_IDS: std::ops::Range<u64> = 0xB07_0000..0xB07_0000 + 0x1_0000;
/// One game created per address per this long.
const CREATE_EVERY: Duration = Duration::from_secs(10);
/// How long a new game has to start.
const START_TIMEOUT: Duration = Duration::from_secs(20);
const NAME_CHARS: usize = 24;
const MODES: [&str; 2] = ["ffa", "duel"];

struct Config {
    key: [u8; 32],
    public_ip: IpAddr,
    ports: Vec<u16>,
    bin: PathBuf,
    idle: Option<String>,
    /// Games take browser players on their port plus this, if set.
    wt_offset: Option<u16>,
    /// The browser version's files, if there are any.
    web_root: Option<PathBuf>,
    /// From the game program itself: netcode's id and Replicon's hash.
    netcode_protocol: u64,
    replicon_protocol: String,
}

#[derive(Debug)]
struct Game {
    name: String,
    mode: String,
    port: u16,
    players: u32,
    max: u32,
    ready: bool,
    /// Where it takes browser players, and its certificate's hash (hex).
    webtransport: Option<(u16, String)>,
    /// Held so the game knows the lobby is alive (see `--owned`).
    _stdin: Option<ChildStdin>,
}

#[derive(Default)]
struct Lobby {
    games: BTreeMap<u32, Game>,
    next_id: u32,
    last_create: HashMap<IpAddr, Instant>,
}

type Shared = Arc<(Config, Mutex<Lobby>)>;

fn main() {
    let config = match config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("lobby: {e}");
            std::process::exit(2);
        }
    };
    let listen = std::env::var("LOBBY_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let server = match Server::http(&listen) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("lobby: can't listen on {listen}: {e}");
            std::process::exit(2);
        }
    };
    println!(
        "lobby on http://{listen}, games on {}:{}-{}, protocol {}",
        config.public_ip,
        config.ports[0],
        config.ports[config.ports.len() - 1],
        config.replicon_protocol,
    );
    let shared: Shared = Arc::new((config, Mutex::new(Lobby::default())));
    for request in server.incoming_requests() {
        let shared = shared.clone();
        // Creating a game waits for it to start, so each request gets its
        // own thread rather than holding up the others.
        thread::spawn(move || handle(request, &shared));
    }
}

fn config() -> Result<Config, String> {
    let key = std::env::var("FASTFPS_KEY").map_err(|_| "FASTFPS_KEY is not set")?;
    let key = parse_key(&key)?;
    let host = std::env::var("PUBLIC_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let public_ip = format!("{host}:0")
        .to_socket_addrs()
        .map_err(|e| format!("PUBLIC_HOST '{host}': {e}"))?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| format!("PUBLIC_HOST '{host}' has no IPv4 address"))?
        .ip();
    let ports = parse_ports(&std::env::var("GAME_PORTS").unwrap_or_else(|_| "27100-27107".into()))?;
    let bin = match std::env::var_os("FASTFPS_BIN") {
        Some(bin) => PathBuf::from(bin),
        None => std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("fast-fps")))
            .unwrap_or_else(|| "fast-fps".into()),
    };
    let idle = std::env::var("GAME_IDLE").ok().filter(|s| !s.is_empty());
    let wt_offset = match std::env::var("WT_PORT_OFFSET").as_deref() {
        Ok("off") | Ok("0") => None,
        Ok(n) => Some(
            n.parse()
                .map_err(|_| format!("WT_PORT_OFFSET '{n}' isn't a number"))?,
        ),
        Err(_) => Some(100),
    };
    let web_root = std::env::var_os("WEB_ROOT")
        .map(PathBuf::from)
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            Some(exe.parent()?.join("web"))
        })
        .filter(|dir| dir.join("index.html").is_file());
    let (netcode_protocol, replicon_protocol) = ask_protocol(&bin)?;
    Ok(Config {
        key,
        public_ip,
        ports,
        bin,
        idle,
        wt_offset,
        web_root,
        netcode_protocol,
        replicon_protocol,
    })
}

/// Ask the game program which protocol it speaks.
fn ask_protocol(bin: &PathBuf) -> Result<(u64, String), String> {
    let out = Command::new(bin)
        .args(["--dedicated", "--protocol"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("can't run {}: {e}", bin.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(parse_protocol)
        .ok_or_else(|| format!("{} didn't say its protocol", bin.display()))
}

/// `@fastfps protocol netcode=<u64> replicon=<hash>`
fn parse_protocol(line: &str) -> Option<(u64, String)> {
    let rest = line.strip_prefix("@fastfps protocol ")?;
    let field = |name: &str| {
        rest.split_whitespace()
            .find_map(|w| w.strip_prefix(name)?.strip_prefix('='))
    };
    Some((
        field("netcode")?.parse().ok()?,
        field("replicon")?.to_string(),
    ))
}

fn parse_key(hex: &str) -> Result<[u8; 32], String> {
    let hex = hex.trim();
    let bad = || "FASTFPS_KEY must be 64 hex characters".to_string();
    if hex.len() != 64 || !hex.is_ascii() {
        return Err(bad());
    }
    let mut key = [0; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| bad())?;
    }
    Ok(key)
}

/// `27100-27107` or `27100`.
fn parse_ports(text: &str) -> Result<Vec<u16>, String> {
    let bad = || format!("GAME_PORTS '{text}' should look like 27100-27107");
    let (first, last) = match text.split_once('-') {
        Some((a, b)) => (
            a.trim().parse().map_err(|_| bad())?,
            b.trim().parse().map_err(|_| bad())?,
        ),
        None => {
            let p = text.trim().parse().map_err(|_| bad())?;
            (p, p)
        }
    };
    if first == 0 || last < first {
        return Err(bad());
    }
    Ok((first..=last).collect())
}

/// A game name as shown to others: printable, trimmed, not too long.
fn clean_name(name: &str, fallback: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(NAME_CHARS)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        fallback.chars().take(NAME_CHARS).collect()
    } else {
        name.to_string()
    }
}

fn free_port(lobby: &Lobby, ports: &[u16]) -> Option<u16> {
    ports
        .iter()
        .copied()
        .find(|p| lobby.games.values().all(|g| g.port != *p))
}

/// Whether `ip` may create a game now, and if so, note that it did.
fn may_create(last: &mut HashMap<IpAddr, Instant>, ip: IpAddr, now: Instant) -> bool {
    last.retain(|_, at| now.duration_since(*at) < CREATE_EVERY);
    if last.contains_key(&ip) {
        return false;
    }
    last.insert(ip, now);
    true
}

/// A player id: never 0 (the host on a LAN) and never a bot's.
fn client_id(rng: &mut impl Rng) -> u64 {
    loop {
        let id = rng.random::<u64>();
        if id != 0 && !BOT_IDS.contains(&id) {
            return id;
        }
    }
}

/// A ticket for the game listening at `port` on socket `socket` (0 is UDP,
/// 1 WebTransport; see the game's `net/transport.rs`).
fn token(config: &Config, port: u16, socket: u8, admin: bool) -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let mut user_data = [0u8; NETCODE_USER_DATA_BYTES];
    user_data[USER_DATA_ADMIN] = u8::from(admin);
    let token = ConnectToken::generate(
        now,
        config.netcode_protocol,
        TOKEN_SECS,
        client_id(&mut rand::rng()),
        TIMEOUT_SECS,
        socket,
        vec![SocketAddr::new(config.public_ip, port)],
        Some(&user_data),
        &config.key,
    )
    .map_err(|e| format!("token: {e}"))?;
    let mut bytes = Vec::new();
    token.write(&mut bytes).map_err(|e| format!("token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// What a game says on stdout.
#[derive(Debug, PartialEq)]
enum Said {
    Ready {
        max: u32,
        /// Its WebTransport port and certificate hash, if it has them.
        webtransport: Option<(u16, String)>,
    },
    Status {
        players: u32,
        mode: String,
    },
    Other,
}

fn parse_said(line: &str) -> Said {
    let Some(rest) = line.strip_prefix("@fastfps ") else {
        return Said::Other;
    };
    let field = |name: &str| {
        rest.split_whitespace()
            .find_map(|w| w.strip_prefix(name)?.strip_prefix('='))
    };
    match rest.split_whitespace().next() {
        Some("ready") => Said::Ready {
            max: field("max").and_then(|m| m.parse().ok()).unwrap_or(16),
            webtransport: field("wt")
                .and_then(|p| p.parse().ok())
                .zip(field("cert").map(str::to_string)),
        },
        Some("status") => Said::Status {
            players: field("players").and_then(|p| p.parse().ok()).unwrap_or(0),
            mode: field("mode").unwrap_or("ffa").to_string(),
        },
        _ => Said::Other,
    }
}

/// Start a dedicated server for game `id` and follow what it says.
fn start_game(shared: &Shared, id: u32, port: u16, mode: &str) -> Result<(), String> {
    let (config, lobby) = &**shared;
    let mut command = Command::new(&config.bin);
    command
        .args(["--dedicated", "--owned", "--port", &port.to_string()])
        .args([
            "--public",
            &SocketAddr::new(config.public_ip, port).to_string(),
        ]);
    if let Some(idle) = &config.idle {
        command.args(["--idle", idle]);
    }
    if let Some(offset) = config.wt_offset {
        command.args(["--webtransport", &(port + offset).to_string()]);
    }
    if mode == "duel" {
        command.args(["+mode", "duel"]);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("can't start {}: {e}", config.bin.display()))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    if let Some(game) = lobby.lock().unwrap().games.get_mut(&id) {
        game._stdin = child.stdin.take();
    }
    let shared = shared.clone();
    thread::spawn(move || {
        let (_, lobby) = &*shared;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let mut lobby = lobby.lock().unwrap();
            let Some(game) = lobby.games.get_mut(&id) else {
                continue;
            };
            match parse_said(&line) {
                Said::Ready { max, webtransport } => {
                    game.ready = true;
                    game.max = max;
                    game.webtransport = webtransport;
                }
                Said::Status { players, mode } => {
                    game.players = players;
                    game.mode = mode;
                }
                Said::Other => println!("[game {id}] {line}"),
            }
        }
        let status = child.wait();
        lobby.lock().unwrap().games.remove(&id);
        println!("[game {id}] ended ({status:?})");
    });
    Ok(())
}

#[derive(Deserialize)]
struct CreateBody {
    #[serde(default)]
    name: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    player: String,
    #[serde(default)]
    protocol: String,
    #[serde(default)]
    transport: String,
}

#[derive(Deserialize)]
struct JoinBody {
    #[serde(default)]
    protocol: String,
    #[serde(default)]
    transport: String,
}

/// The answer to `create` or `join`: a ticket into game `id`, for the
/// native game (UDP) or a browser (WebTransport).
fn ticket(config: &Config, id: u32, game: &Game, transport: &str, admin: bool) -> (u16, Value) {
    let made = match transport {
        "" | "udp" => token(config, game.port, 0, admin).map(|t| json!({"token": t})),
        "webtransport" => match &game.webtransport {
            None => return error(409, "this server doesn't take browser players"),
            Some((port, cert)) => token(config, *port, 1, admin).map(|t| {
                json!({
                    "token": t,
                    "wt_addr": SocketAddr::new(config.public_ip, *port).to_string(),
                    "cert_hash": cert,
                })
            }),
        },
        _ => return error(400, "transport must be udp or webtransport"),
    };
    match made {
        Ok(mut reply) => {
            reply["id"] = json!(id);
            reply["name"] = json!(game.name);
            (200, reply)
        }
        Err(e) => error(500, &e),
    }
}

fn handle(mut request: Request, shared: &Shared) {
    if let Some(response) = static_file(&request, &shared.0) {
        let _ = request.respond(response);
        return;
    }
    let (code, body) = route(&mut request, shared);
    let header = Header::from_bytes("Content-Type", "application/json").expect("valid header");
    let response = Response::from_string(body.to_string())
        .with_status_code(code)
        .with_header(header);
    let _ = request.respond(response);
}

/// The file in `root` a URL path names, if it is a plain path inside it:
/// `/` is `index.html`, and nothing may climb out with `..` or reach a
/// hidden file.
fn web_path(root: &Path, url_path: &str) -> Option<PathBuf> {
    let path = url_path.split('?').next()?.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let mut file = root.to_path_buf();
    for part in path.split('/') {
        if part.is_empty() || part.starts_with('.') || part.contains('\\') {
            return None;
        }
        file.push(part);
    }
    Some(file)
}

fn content_type(file: &Path) -> &'static str {
    match file.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("css") => "text/css",
        Some("png") => "image/png",
        _ => "text/plain; charset=utf-8",
    }
}

/// Serve a file of the browser version, for any `GET` that isn't the API.
/// A gzipped copy (`name.gz`, made by `scripts/web.sh`) goes to browsers
/// that take gzip. Files are revalidated each visit with an ETag, so a new
/// build is picked up at once and an unchanged one isn't downloaded again.
fn static_file(request: &Request, config: &Config) -> Option<Response<std::fs::File>> {
    if *request.method() != Method::Get {
        return None;
    }
    let first = request
        .url()
        .trim_start_matches('/')
        .split(['/', '?'])
        .next()?;
    if matches!(first, "games" | "health") {
        return None;
    }
    let file = web_path(config.web_root.as_ref()?, request.url())?;
    let header = |name: &str, value: &str| Header::from_bytes(name, value).ok();
    let accepts_gzip = request
        .headers()
        .iter()
        .any(|h| h.field.equiv("Accept-Encoding") && h.value.as_str().contains("gzip"));
    let gz = PathBuf::from(format!("{}.gz", file.display()));
    let (path, gzipped) = if accepts_gzip && gz.is_file() {
        (gz, true)
    } else if file.is_file() {
        (file.clone(), false)
    } else {
        return None;
    };
    let meta = std::fs::metadata(&path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let etag = format!(
        "\"{:x}-{modified:x}{}\"",
        meta.len(),
        if gzipped { "-gz" } else { "" }
    );
    let mut headers = vec![
        header("Content-Type", content_type(&file)),
        header("Cache-Control", "no-cache"),
        header("ETag", &etag),
        header("Vary", "Accept-Encoding"),
    ];
    let unchanged = request
        .headers()
        .iter()
        .any(|h| h.field.equiv("If-None-Match") && h.value.as_str() == etag);
    if unchanged {
        let empty = std::fs::File::open(&path).ok()?;
        let mut response = Response::new(304.into(), Vec::new(), empty, Some(0), None);
        for h in headers.into_iter().flatten() {
            response.add_header(h);
        }
        return Some(response);
    }
    if gzipped {
        headers.push(header("Content-Encoding", "gzip"));
    }
    let mut response = Response::from_file(std::fs::File::open(&path).ok()?);
    for h in headers.into_iter().flatten() {
        response.add_header(h);
    }
    Some(response)
}

fn error(code: u16, text: &str) -> (u16, Value) {
    (code, json!({ "error": text }))
}

/// The caller's address. Behind Coolify's proxy that's the first entry of
/// `X-Forwarded-For`.
fn caller(request: &Request) -> IpAddr {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Forwarded-For"))
        .and_then(|h| h.value.as_str().split(',').next()?.trim().parse().ok())
        .or_else(|| request.remote_addr().map(SocketAddr::ip))
        .unwrap_or(IpAddr::from([0, 0, 0, 0]))
}

fn read_json<T: for<'a> Deserialize<'a>>(request: &mut Request) -> Result<T, (u16, Value)> {
    let mut text = String::new();
    request
        .as_reader()
        .take(4096)
        .read_to_string(&mut text)
        .map_err(|_| error(400, "unreadable body"))?;
    serde_json::from_str(if text.trim().is_empty() { "{}" } else { &text })
        .map_err(|e| error(400, &format!("bad request: {e}")))
}

fn route(request: &mut Request, shared: &Shared) -> (u16, Value) {
    let (config, lobby) = &**shared;
    let url = request.url().split('?').next().unwrap_or("").to_string();
    let parts: Vec<&str> = url.trim_matches('/').split('/').collect();
    let wrong_version = || {
        error(
            409,
            "this server runs a different version of the game; update yours",
        )
    };
    match (request.method(), parts.as_slice()) {
        (Method::Get, ["health"]) => (200, json!("ok")),
        (Method::Get, ["games"]) => {
            let lobby = lobby.lock().unwrap();
            let games: Vec<Value> = lobby
                .games
                .iter()
                .filter(|(_, g)| g.ready)
                .map(|(id, g)| {
                    json!({"id": id, "name": g.name, "mode": g.mode, "players": g.players, "max": g.max})
                })
                .collect();
            (
                200,
                json!({"protocol": config.replicon_protocol, "games": games}),
            )
        }
        (Method::Post, ["games"]) => {
            let body: CreateBody = match read_json(request) {
                Ok(body) => body,
                Err(e) => return e,
            };
            if body.protocol != config.replicon_protocol {
                return wrong_version();
            }
            let mode = if body.mode.is_empty() {
                "ffa"
            } else {
                &body.mode
            };
            if !MODES.contains(&mode) {
                return error(400, "mode must be ffa or duel");
            }
            let player = clean_name(&body.player, "someone");
            let name = clean_name(&body.name, &format!("{player}'s game"));
            let ip = caller(request);
            let (id, port) = {
                let mut lobby = lobby.lock().unwrap();
                let Some(port) = free_port(&lobby, &config.ports) else {
                    return error(503, "the server is full; try again later");
                };
                if !may_create(&mut lobby.last_create, ip, Instant::now()) {
                    return error(429, "one new game every few seconds, please");
                }
                lobby.next_id += 1;
                let id = lobby.next_id;
                lobby.games.insert(
                    id,
                    Game {
                        name: name.clone(),
                        mode: mode.to_string(),
                        port,
                        players: 0,
                        max: 0,
                        ready: false,
                        webtransport: None,
                        _stdin: None,
                    },
                );
                (id, port)
            };
            println!("[game {id}] '{name}' ({mode}) for {player} at {ip}, port {port}");
            if let Err(e) = start_game(shared, id, port, mode) {
                lobby.lock().unwrap().games.remove(&id);
                eprintln!("[game {id}] {e}");
                return error(500, "could not start a game");
            }
            // Wait for it to listen before handing out a ticket to it.
            let deadline = Instant::now() + START_TIMEOUT;
            loop {
                match lobby.lock().unwrap().games.get(&id).map(|g| g.ready) {
                    Some(true) => break,
                    None => return error(500, "the game failed to start"),
                    Some(false) => {}
                }
                if Instant::now() > deadline {
                    return error(500, "the game took too long to start");
                }
                thread::sleep(Duration::from_millis(50));
            }
            let lobby = lobby.lock().unwrap();
            match lobby.games.get(&id) {
                Some(game) => ticket(config, id, game, &body.transport, true),
                None => error(500, "the game failed to start"),
            }
        }
        (Method::Post, ["games", id, "join"]) => {
            let body: JoinBody = match read_json(request) {
                Ok(body) => body,
                Err(e) => return e,
            };
            if body.protocol != config.replicon_protocol {
                return wrong_version();
            }
            let Ok(id) = id.parse::<u32>() else {
                return error(404, "no such game");
            };
            let lobby = lobby.lock().unwrap();
            match lobby.games.get(&id) {
                Some(g) if g.ready && g.players >= g.max => error(409, "that game is full"),
                Some(g) if g.ready => ticket(config, id, g, &body.transport, false),
                _ => error(404, "no such game"),
            }
        }
        _ => error(404, "not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            key: [7; 32],
            public_ip: IpAddr::from([127, 0, 0, 1]),
            ports: vec![27100, 27101],
            bin: "fast-fps".into(),
            idle: None,
            wt_offset: Some(100),
            web_root: None,
            netcode_protocol: 42,
            replicon_protocol: "123".into(),
        }
    }

    fn game(port: u16) -> Game {
        Game {
            name: "g".into(),
            mode: "ffa".into(),
            port,
            players: 0,
            max: 16,
            ready: true,
            webtransport: None,
            _stdin: None,
        }
    }

    #[test]
    fn ports() {
        assert_eq!(parse_ports("27100-27102"), Ok(vec![27100, 27101, 27102]));
        assert_eq!(parse_ports("27100"), Ok(vec![27100]));
        assert!(parse_ports("27102-27100").is_err());
        assert!(parse_ports("x").is_err());
    }

    #[test]
    fn free_ports_run_out() {
        let config = config();
        let mut lobby = Lobby::default();
        assert_eq!(free_port(&lobby, &config.ports), Some(27100));
        lobby.games.insert(1, game(27100));
        assert_eq!(free_port(&lobby, &config.ports), Some(27101));
        lobby.games.insert(2, game(27101));
        assert_eq!(free_port(&lobby, &config.ports), None);
        lobby.games.remove(&1);
        assert_eq!(free_port(&lobby, &config.ports), Some(27100));
    }

    #[test]
    fn names() {
        assert_eq!(clean_name("  duel at dawn ", "x"), "duel at dawn");
        assert_eq!(clean_name("a\u{7}b\nc", "x"), "abc");
        assert_eq!(clean_name("   ", "Shaun's game"), "Shaun's game");
        assert_eq!(clean_name(&"y".repeat(100), "x").len(), NAME_CHARS);
    }

    #[test]
    fn one_game_per_address_at_a_time() {
        let mut last = HashMap::new();
        let ip = IpAddr::from([10, 0, 0, 1]);
        let t = Instant::now();
        assert!(may_create(&mut last, ip, t));
        assert!(!may_create(&mut last, ip, t + Duration::from_secs(1)));
        assert!(may_create(&mut last, IpAddr::from([10, 0, 0, 2]), t));
        assert!(may_create(&mut last, ip, t + CREATE_EVERY));
    }

    #[test]
    fn client_ids_avoid_the_host_and_bots() {
        let mut rng = rand::rng();
        for _ in 0..1000 {
            let id = client_id(&mut rng);
            assert_ne!(id, 0);
            assert!(!BOT_IDS.contains(&id));
        }
    }

    #[test]
    fn what_games_say() {
        assert_eq!(
            parse_said("@fastfps ready port=27100 max=16"),
            Said::Ready {
                max: 16,
                webtransport: None
            }
        );
        assert_eq!(
            parse_said("@fastfps ready port=27100 max=16 wt=27200 cert=ab12"),
            Said::Ready {
                max: 16,
                webtransport: Some((27200, "ab12".into()))
            }
        );
        assert_eq!(
            parse_said("@fastfps status players=2 mode=duel"),
            Said::Status {
                players: 2,
                mode: "duel".into()
            }
        );
        assert_eq!(parse_said("bot1 spawned"), Said::Other);
        assert_eq!(
            parse_protocol("@fastfps protocol netcode=9 replicon=77"),
            Some((9, "77".into()))
        );
        assert_eq!(parse_protocol("@fastfps ready port=1"), None);
    }

    #[test]
    fn tokens_read_back() {
        let config = config();
        let hex = token(&config, 27101, 0, true).unwrap();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let token = ConnectToken::read(&mut bytes.as_slice()).unwrap();
        assert_eq!(token.protocol_id, 42);
        assert_eq!(
            token.server_addresses[0],
            Some("127.0.0.1:27101".parse().unwrap())
        );
        assert_eq!(token.expire_timestamp - token.create_timestamp, TOKEN_SECS);
        // The admin flag is in the encrypted part, for the server's eyes
        // only; the online harness scenario checks it arrives.
        assert_ne!(token.client_id, 0);
    }

    #[test]
    fn web_paths_stay_inside() {
        let root = Path::new("/app/web");
        assert_eq!(web_path(root, "/"), Some(root.join("index.html")));
        assert_eq!(
            web_path(root, "/?c=exec%20x"),
            Some(root.join("index.html"))
        );
        assert_eq!(
            web_path(root, "/cfg/bots.cfg"),
            Some(root.join("cfg/bots.cfg"))
        );
        assert_eq!(web_path(root, "/../etc/passwd"), None);
        assert_eq!(web_path(root, "/cfg/../../x"), None);
        assert_eq!(web_path(root, "/.git/config"), None);
        // A doubled slash is trimmed: still inside the root.
        assert_eq!(
            web_path(root, "//etc/passwd"),
            Some(root.join("etc/passwd"))
        );
    }

    #[test]
    fn browser_tickets() {
        let config = config();
        let mut g = game(27101);
        // A game without WebTransport turns browsers away.
        assert_eq!(ticket(&config, 1, &g, "webtransport", false).0, 409);
        assert_eq!(ticket(&config, 1, &g, "carrier pigeon", false).0, 400);
        g.webtransport = Some((27201, "ab".repeat(32)));
        let (code, reply) = ticket(&config, 1, &g, "webtransport", false);
        assert_eq!(code, 200);
        assert_eq!(reply["wt_addr"], "127.0.0.1:27201");
        assert_eq!(reply["cert_hash"], "ab".repeat(32));
        let hex = reply["token"].as_str().unwrap();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let token = ConnectToken::read(&mut bytes.as_slice()).unwrap();
        // Socket 1 is the game's WebTransport socket.
        assert_eq!(token.socket_id, 1);
        assert_eq!(
            token.server_addresses[0],
            Some("127.0.0.1:27201".parse().unwrap())
        );
        // The native game's ticket is still for UDP, socket 0.
        let (_, reply) = ticket(&config, 1, &g, "", false);
        assert!(reply.get("cert_hash").is_none());
    }

    #[test]
    fn keys() {
        assert_eq!(parse_key(&"0a".repeat(32)), Ok([10; 32]));
        assert!(parse_key("0a").is_err());
    }
}
