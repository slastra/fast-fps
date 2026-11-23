//! LAN discovery. A host broadcasts a small UDP beacon once a second;
//! `find` and a bare `connect` listen for it.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

use bevy::prelude::*;

use crate::{
    config::{DISCOVERY_PORT, GAME_NAME},
    console::Print,
    net::{protocol::Player, session::LocalName},
};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Discovery>().add_systems(
        Update,
        (
            send_beacon.run_if(resource_exists::<Beacon>),
            listen_for_beacons,
        ),
    );
}

#[derive(Resource)]
pub struct Beacon {
    pub socket: UdpSocket,
    pub port: u16,
    pub timer: Timer,
}

pub fn send_beacon(
    mut beacon: ResMut<Beacon>,
    time: Res<Time<Real>>,
    name: Res<LocalName>,
    players: Query<(), With<Player>>,
) {
    if !beacon.timer.tick(time.delta()).just_finished() {
        return;
    }
    let msg = format!(
        "{GAME_NAME} {} {} {}",
        beacon.port,
        players.iter().count(),
        name.0
    );
    let _ = beacon
        .socket
        .send_to(msg.as_bytes(), (Ipv4Addr::BROADCAST, DISCOVERY_PORT));
}

#[derive(Resource, Default)]
pub struct Discovery {
    socket: Option<UdpSocket>,
    timer: Timer,
    found: Vec<(SocketAddr, String)>,
    auto_connect: bool,
    pub pending_connect: Option<String>,
}

impl Discovery {
    pub fn start(&mut self, auto_connect: bool, print: &mut MessageWriter<Print>) {
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

pub fn listen_for_beacons(
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
        if parts.next() != Some(GAME_NAME) {
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
