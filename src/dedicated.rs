//! A dedicated server: `fast-fps --dedicated`. The game's server code with no
//! window, no GPU, no sound and no player of its own.
//!
//! It hosts as soon as it starts, takes console commands on stdin, and
//! reports on stdout for whoever started it (the lobby, in `lobby/`):
//!
//! ```text
//! @fastfps ready port=27100
//! @fastfps status players=2 mode=duel
//! ```
//!
//! With nobody in the game for [`Dedicated::idle_secs`] it exits, so an
//! abandoned game cleans itself up.
//!
//! Options (before any `+command`):
//!
//! | | |
//! |---|---|
//! | `--port <n>` | UDP port to listen on (default `DEFAULT_PORT`) |
//! | `--public <host:port>` | the address clients reach it at, for connect tokens |
//! | `--idle <secs>` | exit after this long empty (default `IDLE_SECS`) |
//! | `--owned` | exit when stdin closes: started by the lobby, which is gone |
//! | `--protocol` | print `@fastfps protocol netcode=<id> replicon=<hash>` and exit |
//!
//! With `FASTFPS_KEY` set (64 hex characters) only clients holding a connect
//! token signed with that key can join; without it anyone can, as on a LAN.

use std::{
    io::{BufRead, Write},
    net::{SocketAddr, ToSocketAddrs},
    sync::{Mutex, mpsc},
};

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    config::{DEFAULT_PORT, IDLE_SECS, MAX_CLIENTS, PROTOCOL_ID},
    console::{Console, Print},
    net::{Duel, HostOptions, Player, protocol_text, start_server},
    server::PlayerClient,
};

/// Present only in a dedicated server, inserted before any plugin is added
/// so plugins can check for it while they build.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Dedicated {
    pub port: u16,
    /// Where clients reach this server, if it differs from the bound socket
    /// (behind NAT or Docker). Connect tokens must name this address.
    pub public: Option<SocketAddr>,
    /// The connect token key. `None` accepts anyone.
    pub key: Option<[u8; 32]>,
    pub idle_secs: f32,
    /// Exit when stdin closes (`--owned`).
    pub owned: bool,
    /// Only say which protocol this build speaks, then exit (`--protocol`).
    pub protocol_only: bool,
}

impl Dedicated {
    /// `Ok(None)` unless `--dedicated` is among the arguments.
    pub fn from_args(args: &[String], key: Option<&str>) -> Result<Option<Self>, String> {
        if !args.iter().any(|a| a == "--dedicated") {
            return Ok(None);
        }
        let value = |flag: &str| -> Result<Option<&str>, String> {
            match args.iter().position(|a| a == flag) {
                None => Ok(None),
                Some(i) => args
                    .get(i + 1)
                    .map(|v| Some(v.as_str()))
                    .ok_or_else(|| format!("{flag} needs a value")),
            }
        };
        let port = match value("--port")? {
            Some(p) => p.parse().map_err(|_| format!("bad --port '{p}'"))?,
            None => DEFAULT_PORT,
        };
        let public = match value("--public")? {
            Some(p) => Some(
                p.to_socket_addrs()
                    .map_err(|e| format!("bad --public '{p}': {e}"))?
                    .find(SocketAddr::is_ipv4)
                    .ok_or_else(|| format!("--public '{p}' has no IPv4 address"))?,
            ),
            None => None,
        };
        let idle_secs = match value("--idle")? {
            Some(s) => s.parse().map_err(|_| format!("bad --idle '{s}'"))?,
            None => IDLE_SECS,
        };
        let key = key.filter(|k| !k.is_empty()).map(parse_key).transpose()?;
        let flag = |name: &str| args.iter().any(|a| a == name);
        Ok(Some(Self {
            port,
            public,
            key,
            idle_secs,
            owned: flag("--owned"),
            protocol_only: flag("--protocol"),
        }))
    }
}

/// 64 hex characters to the 32-byte key.
pub fn parse_key(hex: &str) -> Result<[u8; 32], String> {
    let hex = hex.trim();
    if hex.len() != 64 || !hex.is_ascii() {
        return Err("FASTFPS_KEY must be 64 hex characters".into());
    }
    let mut key = [0; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| "FASTFPS_KEY must be 64 hex characters".to_string())?;
    }
    Ok(key)
}

pub(super) fn plugin(app: &mut App) {
    if app.world().resource::<Dedicated>().protocol_only {
        app.add_systems(Startup, say_protocol);
        return;
    }
    let (send, lines) = mpsc::channel();
    // Lines typed into the server's terminal, or written by the lobby.
    // `None` means stdin closed.
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if send.send(Some(line)).is_err() {
                return;
            }
        }
        let _ = send.send(None);
    });
    app.insert_resource(Stdin(Mutex::new(lines)))
        .add_systems(Startup, host)
        .add_systems(PreUpdate, read_stdin)
        .add_systems(Update, (report, idle_exit));
}

#[derive(Resource)]
struct Stdin(Mutex<mpsc::Receiver<Option<String>>>);

/// For the lobby, which hands out tokens for this build and checks that
/// players' games speak the same protocol.
fn say_protocol(hash: Res<ProtocolHash>, mut exit: MessageWriter<AppExit>) {
    announce(&format!(
        "protocol netcode={PROTOCOL_ID} replicon={}",
        protocol_text(&hash)
    ));
    exit.write(AppExit::Success);
}

/// One line on stdout for the lobby, flushed at once.
fn announce(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "@fastfps {line}");
    let _ = out.flush();
}

fn host(
    dedicated: Res<Dedicated>,
    mut commands: Commands,
    channels: Res<RepliconChannels>,
    mut print: MessageWriter<Print>,
    mut exit: MessageWriter<AppExit>,
) {
    let options = HostOptions {
        port: dedicated.port,
        key: dedicated.key,
        public: dedicated.public,
        // Only an open server is for the LAN to find.
        beacon: dedicated.key.is_none(),
    };
    match start_server(&mut commands, &channels, &options) {
        Ok(()) => {
            let secure = if dedicated.key.is_some() {
                "secure"
            } else {
                "open"
            };
            print.write(Print(format!(
                "dedicated server on UDP {} ({secure})",
                dedicated.port
            )));
            announce(&format!("ready port={} max={MAX_CLIENTS}", dedicated.port));
        }
        Err(e) => {
            print.write(Print::error(format!("could not host: {e}")));
            announce(&format!("failed {e}"));
            exit.write(AppExit::error());
        }
    }
}

fn read_stdin(
    stdin: Res<Stdin>,
    dedicated: Res<Dedicated>,
    mut console: ResMut<Console>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(lines) = stdin.0.lock() else { return };
    while let Ok(line) = lines.try_recv() {
        match line {
            Some(line) if !line.trim().is_empty() => console.exec(line),
            Some(_) => {}
            // The lobby that started this game has gone.
            None if dedicated.owned => {
                announce("exit orphaned");
                exit.write(AppExit::Success);
            }
            None => {}
        }
    }
}

/// Real players connected right now (bots don't count).
fn players_here(players: &Query<&Player, With<PlayerClient>>) -> usize {
    players.iter().filter(|p| !p.is_bot()).count()
}

/// Tell the lobby how many are playing and what, whenever that changes.
fn report(
    players: Query<&Player, With<PlayerClient>>,
    duels: Query<(), With<Duel>>,
    mut last: Local<Option<(usize, bool)>>,
) {
    let now = (players_here(&players), !duels.is_empty());
    if *last != Some(now) {
        *last = Some(now);
        let mode = if now.1 { "duel" } else { "ffa" };
        announce(&format!("status players={} mode={mode}", now.0));
    }
}

/// Exit once the game has been empty for a while, including when nobody
/// ever joined.
fn idle_exit(
    time: Res<Time>,
    dedicated: Res<Dedicated>,
    players: Query<&Player, With<PlayerClient>>,
    mut empty_for: Local<f32>,
    mut exit: MessageWriter<AppExit>,
    mut print: MessageWriter<Print>,
) {
    if players_here(&players) > 0 {
        *empty_for = 0.0;
        return;
    }
    let before = *empty_for;
    *empty_for += time.delta_secs();
    if before < dedicated.idle_secs && *empty_for >= dedicated.idle_secs {
        print.write(Print::new("nobody here; shutting down"));
        announce("exit idle");
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn not_dedicated_without_the_flag() {
        assert_eq!(Dedicated::from_args(&args("+host"), None), Ok(None));
    }

    #[test]
    fn reads_options() {
        let key = "ab".repeat(32);
        let d = Dedicated::from_args(
            &args("--dedicated --port 27100 --public 127.0.0.1:27100 --idle 5 +mode duel"),
            Some(&key),
        )
        .unwrap()
        .unwrap();
        assert_eq!(d.port, 27100);
        assert_eq!(d.public, Some("127.0.0.1:27100".parse().unwrap()));
        assert_eq!(d.idle_secs, 5.0);
        assert_eq!(d.key, Some([0xab; 32]));
        assert!(!d.owned && !d.protocol_only);
        let d = Dedicated::from_args(&args("--dedicated --owned --protocol"), None)
            .unwrap()
            .unwrap();
        assert!(d.owned && d.protocol_only);
    }

    #[test]
    fn defaults() {
        let d = Dedicated::from_args(&args("--dedicated"), Some(""))
            .unwrap()
            .unwrap();
        assert_eq!(d.port, DEFAULT_PORT);
        assert_eq!(d.public, None);
        assert_eq!(d.key, None);
        assert_eq!(d.idle_secs, IDLE_SECS);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Dedicated::from_args(&args("--dedicated --port"), None).is_err());
        assert!(Dedicated::from_args(&args("--dedicated --port x"), None).is_err());
        assert!(Dedicated::from_args(&args("--dedicated"), Some("abc")).is_err());
        assert!(parse_key(&"zz".repeat(32)).is_err());
    }
}
