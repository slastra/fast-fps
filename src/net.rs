//! Networking: the replication protocol plus host / connect / discovery commands.
//!
//! The game uses Replicon's recommended layout. Authoritative logic (damage,
//! respawns, dummies) runs whenever we are *not* a client, so the same code
//! drives offline play and a listen server. Clients only send intent: where
//! they are and where they shot.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    time::{Duration, SystemTime},
};

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::{
    RenetChannelsExt, RenetClient, RenetServer,
    netcode::{
        ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
        ServerConfig,
    },
    renet::ConnectionConfig,
};
use serde::{Deserialize, Serialize};

use crate::console::{Console, ConsoleAppExt, ConsoleCommand, Print, SaveSetting};
use crate::weapon::WeaponKind;

pub const DEFAULT_PORT: u16 = 27015;
const DISCOVERY_PORT: u16 = 27016;
const PROTOCOL_ID: u64 = 0xFA57_0001;
const BEACON_MAGIC: &str = "FASTFPS";
const MAX_CLIENTS: usize = 16;

// ---------------------------------------------------------------------------
// Replicated state
// ---------------------------------------------------------------------------

/// A player. `owner` is the netcode client id, or `0` for the host / offline player.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct Player {
    pub owner: u64,
}

#[derive(Component, Serialize, Deserialize, Clone, Debug)]
pub struct PlayerName(pub String);

/// Where a player or dummy is. `pos` is the collider center.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct Pose {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Health(pub f32);

/// The weapon a player is holding.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Loadout(pub WeaponKind);

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct Score {
    pub kills: u32,
    pub deaths: u32,
}

/// A practice target. `sway` is how far it slides side to side.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct Dummy {
    pub anchor: Vec3,
    pub sway: f32,
}

/// A dummy that has been knocked down and is waiting to pop back up.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Downed;

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Client -> server: my character's current pose. Sent every frame, unreliable.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct MoveInput(pub Pose);

/// Client -> server: I fired `weapon` from `origin`; one direction per pellet.
#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct Fire {
    pub weapon: WeaponKind,
    pub origin: Vec3,
    pub dirs: Vec<Vec3>,
}

/// Client -> server: I switched weapons (so others see it).
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SelectWeapon(pub WeaponKind);

#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct SetName(pub String);

#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Suicide;

/// Server -> clients: someone else's shot (one per pellet), for tracers and impacts.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ShotFx {
    pub weapon: WeaponKind,
    pub origin: Vec3,
    pub end: Vec3,
    pub normal: Vec3,
    pub flesh: bool,
}

/// Server -> shooter: your shot connected.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct HitConfirm {
    pub head: bool,
    pub kill: bool,
}

/// Server -> one client: put your character here.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Respawn {
    pub pos: Vec3,
    pub yaw: f32,
}

/// Server -> everyone: a line for the kill feed and console.
#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct Feed(pub String);

// ---------------------------------------------------------------------------
// Local identity
// ---------------------------------------------------------------------------

/// The `Player::owner` value that belongs to this machine.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct LocalId(pub u64);

#[derive(Resource, Clone)]
pub struct LocalName(pub String);

impl Default for LocalName {
    fn default() -> Self {
        let name = std::env::var("USER")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "player".into());
        Self(name)
    }
}

pub(super) fn plugin(app: &mut App) {
    app.replicate::<Player>()
        .replicate::<PlayerName>()
        .replicate::<Pose>()
        .replicate::<Health>()
        .replicate::<Score>()
        .replicate::<Loadout>()
        .replicate::<Dummy>()
        .replicate::<Downed>()
        .add_client_message::<MoveInput>(Channel::Unreliable)
        .add_client_message::<Fire>(Channel::Ordered)
        .add_client_message::<SelectWeapon>(Channel::Ordered)
        .add_client_message::<SetName>(Channel::Ordered)
        .add_client_message::<Suicide>(Channel::Ordered)
        .add_server_message::<ShotFx>(Channel::Unreliable)
        .add_server_message::<HitConfirm>(Channel::Ordered)
        .add_server_message::<Respawn>(Channel::Ordered)
        .add_server_message::<Feed>(Channel::Ordered);

    app.init_resource::<LocalId>()
        .init_resource::<LocalName>()
        .init_resource::<Discovery>()
        .add_systems(
            Update,
            (
                net_commands,
                watch_client,
                send_beacon.run_if(resource_exists::<Beacon>),
                listen_for_beacons,
            ),
        )
        .add_systems(PreUpdate, print_feed.after(ClientSystems::Receive))
        .add_systems(OnEnter(ClientState::Connecting), drop_local_world)
        .add_systems(OnEnter(ClientState::Connected), on_connected)
        .add_systems(OnExit(ClientState::Connected), drop_remote_world);

    app.console_command("host", "[port]", "start a LAN server (default 27015)")
        .console_command(
            "connect",
            "[ip[:port]]",
            "join a server; no address = first one found",
        )
        .console_command("disconnect", "", "leave the server or stop hosting")
        .console_command("find", "", "look for servers on the LAN")
        .console_command("status", "", "show connection and scores")
        .console_command("name", "<name>", "set your player name");
}

fn connection_config(channels: &RepliconChannels) -> ConnectionConfig {
    ConnectionConfig {
        server_channels_config: channels.server_configs(),
        client_channels_config: channels.client_configs(),
        ..default()
    }
}

fn now() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
}

fn net_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut commands: Commands,
    mut print: MessageWriter<Print>,
    mut set_name: MessageWriter<SetName>,
    mut save: MessageWriter<SaveSetting>,
    mut console: ResMut<Console>,
    mut local_id: ResMut<LocalId>,
    mut local_name: ResMut<LocalName>,
    mut discovery: ResMut<Discovery>,
    channels: Res<RepliconChannels>,
    server: Option<ResMut<NetcodeServerTransport>>,
    client: Option<ResMut<NetcodeClientTransport>>,
    renet_client: Option<Res<RenetClient>>,
    mut renet_server: Option<ResMut<RenetServer>>,
    players: Query<(&Player, &PlayerName, &Score, Option<&Health>)>,
) {
    let mut server = server;
    let mut client = client;
    for cmd in cmds.read() {
        match cmd.name.as_str() {
            "host" => {
                if server.is_some() {
                    print.write(Print::new("already hosting"));
                    continue;
                }
                if client.is_some() {
                    print.write(Print::new("disconnect first"));
                    continue;
                }
                let port = cmd
                    .arg(0)
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(DEFAULT_PORT);
                match start_server(&mut commands, &channels, port) {
                    Ok(()) => {
                        let lan = lan_ip()
                            .map(|ip| format!("{ip}:{port}"))
                            .unwrap_or_else(|| format!("<this machine>:{port}"));
                        print.write(Print(format!(
                            "hosting on UDP {port}. Others can 'connect {lan}' or just 'connect'"
                        )));
                    }
                    Err(e) => {
                        print.write(Print(format!("could not host: {e}")));
                    }
                }
            }
            "connect" => {
                if server.is_some() {
                    print.write(Print::new("stop hosting first ('disconnect')"));
                    continue;
                }
                if client.is_some() {
                    print.write(Print::new("already connected; 'disconnect' first"));
                    continue;
                }
                let Some(target) = cmd.arg(0) else {
                    print.write(Print::new("searching the LAN..."));
                    discovery.start(true, &mut print);
                    continue;
                };
                let addr = match resolve(target) {
                    Ok(addr) => addr,
                    Err(e) => {
                        print.write(Print(format!("bad address '{target}': {e}")));
                        continue;
                    }
                };
                match start_client(&mut commands, &channels, addr) {
                    Ok(id) => {
                        local_id.0 = id;
                        print.write(Print(format!("connecting to {addr}...")));
                    }
                    Err(e) => {
                        print.write(Print(format!("could not connect: {e}")));
                    }
                }
            }
            "disconnect" => {
                if let Some(transport) = client.as_mut() {
                    transport.disconnect();
                    commands.remove_resource::<RenetClient>();
                    commands.remove_resource::<NetcodeClientTransport>();
                    print.write(Print::new("disconnected"));
                } else if let Some(transport) = server.as_mut() {
                    if let Some(renet) = renet_server.as_deref_mut() {
                        transport.disconnect_all(renet);
                    }
                    commands.remove_resource::<RenetServer>();
                    commands.remove_resource::<NetcodeServerTransport>();
                    commands.remove_resource::<Beacon>();
                    print.write(Print::new("stopped hosting"));
                } else {
                    print.write(Print::new("not connected"));
                }
            }
            "find" => {
                print.write(Print::new("searching the LAN..."));
                discovery.start(false, &mut print);
            }
            "status" => {
                let mode = if server.is_some() {
                    "hosting".to_string()
                } else if let Some(client) = &renet_client {
                    format!("client, rtt {:.0} ms", client.rtt() * 1000.0)
                } else {
                    "offline".to_string()
                };
                print.write(Print(format!("mode: {mode}")));
                for (player, name, score, health) in &players {
                    let me = if player.owner == local_id.0 {
                        " (you)"
                    } else {
                        ""
                    };
                    let hp = health.map(|h| h.0).unwrap_or(0.0);
                    print.write(Print(format!(
                        "  {:<16} {:>3} kills {:>3} deaths {:>3} hp{me}",
                        name.0, score.kills, score.deaths, hp as i32
                    )));
                }
            }
            "name" => {
                let name = cmd.args.join(" ");
                let name: String = name.trim().chars().take(20).collect();
                if name.is_empty() {
                    print.write(Print(format!("name is '{}'", local_name.0)));
                    continue;
                }
                local_name.0 = name.clone();
                save.write(SaveSetting {
                    key: "name",
                    value: name.clone(),
                });
                set_name.write(SetName(name));
            }
            _ => {}
        }
    }
    // Keep the console from swallowing a connect request queued by discovery.
    if let Some(line) = discovery.pending_connect.take() {
        console.exec(line);
    }
}

fn start_server(
    commands: &mut Commands,
    channels: &RepliconChannels,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
    let config = ServerConfig {
        current_time: now(),
        max_clients: MAX_CLIENTS,
        protocol_id: PROTOCOL_ID,
        authentication: ServerAuthentication::Unsecure,
        public_addresses: Default::default(),
    };
    let transport = NetcodeServerTransport::new(config, socket)?;
    commands.insert_resource(RenetServer::new(connection_config(channels)));
    commands.insert_resource(transport);

    let beacon = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    beacon.set_broadcast(true)?;
    commands.insert_resource(Beacon {
        socket: beacon,
        port,
        timer: Timer::from_seconds(1.0, TimerMode::Repeating),
    });
    Ok(())
}

fn start_client(
    commands: &mut Commands,
    channels: &RepliconChannels,
    server_addr: SocketAddr,
) -> Result<u64, Box<dyn std::error::Error>> {
    let client_id = rand::random::<u64>().max(1);
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL_ID,
        server_addr,
        user_data: None,
    };
    let transport = NetcodeClientTransport::new(now(), authentication, socket)?;
    commands.insert_resource(RenetClient::new(connection_config(channels)));
    commands.insert_resource(transport);
    Ok(client_id)
}

fn resolve(target: &str) -> Result<SocketAddr, String> {
    let with_port = if target.contains(':') {
        target.to_string()
    } else {
        format!("{target}:{DEFAULT_PORT}")
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| "no IPv4 address".to_string())
}

/// The address other LAN machines would use to reach us. A UDP `connect` only
/// does a route lookup; nothing is sent.
fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(10, 254, 254, 254), 1)).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// Notice when the server goes away or a connection attempt fails, and clean up.
fn watch_client(
    mut commands: Commands,
    client: Option<Res<RenetClient>>,
    transport: Option<Res<NetcodeClientTransport>>,
    mut print: MessageWriter<Print>,
) {
    let (Some(client), Some(transport)) = (client, transport) else {
        return;
    };
    if client.is_disconnected() {
        let reason = transport
            .disconnect_reason()
            .map(|r| r.to_string())
            .or_else(|| client.disconnect_reason().map(|r| r.to_string()))
            .unwrap_or_else(|| "unknown reason".into());
        print.write(Print(format!("connection closed: {reason}")));
        commands.remove_resource::<RenetClient>();
        commands.remove_resource::<NetcodeClientTransport>();
    }
}

/// Becoming a client: the offline/host world we were simulating is now someone
/// else's job, so drop it and wait for replication.
fn drop_local_world(
    mut commands: Commands,
    entities: Query<Entity, (With<Replicated>, Without<Remote>)>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}

fn drop_remote_world(
    mut commands: Commands,
    entities: Query<Entity, With<Remote>>,
    mut local_id: ResMut<LocalId>,
    mut print: MessageWriter<Print>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
    local_id.0 = 0;
    print.write(Print::new("back to offline play"));
}

fn on_connected(
    mut print: MessageWriter<Print>,
    mut set_name: MessageWriter<SetName>,
    name: Res<LocalName>,
) {
    print.write(Print::new("connected"));
    set_name.write(SetName(name.0.clone()));
}

fn print_feed(mut feed: MessageReader<Feed>, mut print: MessageWriter<Print>) {
    for Feed(line) in feed.read() {
        print.write(Print(line.clone()));
    }
}

// ---------------------------------------------------------------------------
// LAN discovery: the host broadcasts a small UDP beacon once a second.
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct Beacon {
    socket: UdpSocket,
    port: u16,
    timer: Timer,
}

fn send_beacon(
    mut beacon: ResMut<Beacon>,
    time: Res<Time<Real>>,
    name: Res<LocalName>,
    players: Query<(), With<Player>>,
) {
    if !beacon.timer.tick(time.delta()).just_finished() {
        return;
    }
    let msg = format!(
        "{BEACON_MAGIC} {} {} {}",
        beacon.port,
        players.iter().count(),
        name.0
    );
    let _ = beacon
        .socket
        .send_to(msg.as_bytes(), (Ipv4Addr::BROADCAST, DISCOVERY_PORT));
}

#[derive(Resource, Default)]
struct Discovery {
    socket: Option<UdpSocket>,
    timer: Timer,
    found: Vec<(SocketAddr, String)>,
    auto_connect: bool,
    pending_connect: Option<String>,
}

impl Discovery {
    fn start(&mut self, auto_connect: bool, print: &mut MessageWriter<Print>) {
        let socket = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)) {
            Ok(s) => s,
            Err(e) => {
                print.write(Print(format!(
                    "can't listen on UDP {DISCOVERY_PORT} ({e}); use 'connect <ip>'"
                )));
                return;
            }
        };
        let _ = socket.set_nonblocking(true);
        self.socket = Some(socket);
        self.timer = Timer::from_seconds(1.6, TimerMode::Once);
        self.found.clear();
        self.auto_connect = auto_connect;
    }
}

fn listen_for_beacons(
    mut discovery: ResMut<Discovery>,
    time: Res<Time<Real>>,
    mut print: MessageWriter<Print>,
) {
    let Some(socket) = &discovery.socket else {
        return;
    };
    let mut buf = [0u8; 256];
    let mut found = Vec::new();
    while let Ok((len, from)) = socket.recv_from(&mut buf) {
        let Ok(text) = std::str::from_utf8(&buf[..len]) else {
            continue;
        };
        let mut parts = text.splitn(4, ' ');
        if parts.next() != Some(BEACON_MAGIC) {
            continue;
        }
        let Some(port) = parts.next().and_then(|p| p.parse::<u16>().ok()) else {
            continue;
        };
        let players = parts.next().unwrap_or("?");
        let host = parts.next().unwrap_or("?");
        found.push((
            SocketAddr::new(from.ip(), port),
            format!("{host}'s game, {players} player(s)"),
        ));
    }
    for (addr, desc) in found {
        if !discovery.found.iter().any(|(a, _)| *a == addr) {
            discovery.found.push((addr, desc));
        }
    }
    if !discovery.timer.tick(time.delta()).just_finished() {
        return;
    }
    discovery.socket = None;
    if discovery.found.is_empty() {
        print.write(Print::new("no servers found"));
        return;
    }
    for (addr, desc) in &discovery.found {
        print.write(Print(format!("  {addr}  {desc}")));
    }
    if discovery.auto_connect {
        let addr = discovery.found[0].0;
        discovery.pending_connect = Some(format!("connect {addr}"));
    }
}
