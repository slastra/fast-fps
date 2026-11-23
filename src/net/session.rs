//! Starting and ending a session: the `host`, `connect`, `disconnect`,
//! `status` and `name` commands, and swapping worlds when the role changes.

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

use crate::{
    config::{DEFAULT_PORT, MAX_CLIENTS, PROTOCOL_ID},
    console::{Console, ConsoleAppExt, ConsoleCommand, Print, SaveSetting},
    net::{
        discovery::{Beacon, Discovery},
        protocol::{Feed, Health, Player, PlayerName, Score, SetName},
    },
};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<LocalId>()
        .init_resource::<LocalName>()
        .add_systems(Update, (net_commands, watch_client))
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

pub fn net_commands(
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
                    print.write(Print::error("already hosting"));
                    continue;
                }
                if client.is_some() {
                    print.write(Print::error("disconnect first"));
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
                        print.write(Print::error(format!("could not host: {e}")));
                    }
                }
            }
            "connect" => {
                if server.is_some() {
                    print.write(Print::error("stop hosting first ('disconnect')"));
                    continue;
                }
                if client.is_some() {
                    print.write(Print::error("already connected; 'disconnect' first"));
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
                        print.write(Print::error(format!("bad address '{target}': {e}")));
                        continue;
                    }
                };
                match start_client(&mut commands, &channels, addr) {
                    Ok(id) => {
                        local_id.0 = id;
                        print.write(Print(format!("connecting to {addr}...")));
                    }
                    Err(e) => {
                        print.write(Print::error(format!("could not connect: {e}")));
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
                    print.write(Print::error("not connected"));
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
pub fn watch_client(
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
pub fn drop_local_world(
    mut commands: Commands,
    entities: Query<Entity, (With<Replicated>, Without<Remote>)>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}

pub fn drop_remote_world(
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

pub fn on_connected(
    mut print: MessageWriter<Print>,
    mut set_name: MessageWriter<SetName>,
    name: Res<LocalName>,
) {
    print.write(Print::new("connected"));
    set_name.write(SetName(name.0.clone()));
}

pub fn print_feed(mut feed: MessageReader<Feed>, mut print: MessageWriter<Print>) {
    for Feed(line) in feed.read() {
        print.write(Print(line.clone()));
    }
}
