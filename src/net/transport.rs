//! Opening connections. Natively that's a UDP socket under renet2's
//! netcode; a browser has no UDP sockets, so there these are stand-ins that
//! say so (the WebSocket transport comes later, see docs/web.md).

use std::net::SocketAddr;

pub use imp::*;

/// How to host: `host` on a LAN, or a dedicated server (`dedicated.rs`).
// Read only by the native transport; the browser has nothing to host with.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct HostOptions {
    pub port: u16,
    /// With a key, only clients holding a connect token signed with it can
    /// join (see `lobby/`). Without one, anyone can.
    pub key: Option<[u8; 32]>,
    /// The address clients use, if not the socket's own. Connect tokens name
    /// it, and the server checks they do.
    pub public: Option<SocketAddr>,
    /// Announce the game on the LAN.
    pub beacon: bool,
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::{
        net::{Ipv4Addr, SocketAddr, UdpSocket},
        time::{Duration, SystemTime},
    };

    use bevy::prelude::*;
    use bevy_replicon::prelude::*;
    use bevy_replicon_renet2::{
        RenetChannelsExt,
        netcode::{
            ClientAuthentication, ConnectToken, NativeSocket, NetcodeClientTransport,
            NetcodeServerTransport, ServerAuthentication, ServerSetupConfig,
        },
        renet2::{ConnectionConfig, RenetClient, RenetServer},
    };

    use super::HostOptions;
    use crate::{
        config::{CONNECT_TOKEN_SECS, MAX_CLIENTS, PROTOCOL_ID},
        net::discovery::Beacon,
    };

    fn connection_config(channels: &RepliconChannels) -> ConnectionConfig {
        ConnectionConfig::from_channels(channels.server_configs(), channels.client_configs())
    }

    fn now() -> Duration {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
    }

    pub fn start_server(
        commands: &mut Commands,
        channels: &RepliconChannels,
        options: &HostOptions,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, options.port))?;
        let address = options.public.unwrap_or(socket.local_addr()?);
        let authentication = match options.key {
            Some(private_key) => ServerAuthentication::Secure { private_key },
            None => ServerAuthentication::Unsecure,
        };
        let config = ServerSetupConfig {
            current_time: now(),
            max_token_lifetime: CONNECT_TOKEN_SECS,
            max_clients: MAX_CLIENTS,
            protocol_id: PROTOCOL_ID,
            authentication,
            socket_addresses: vec![vec![address]],
        };
        let transport = NetcodeServerTransport::new(config, NativeSocket::new(socket)?)?;
        commands.insert_resource(RenetServer::new(connection_config(channels)));
        commands.insert_resource(transport);

        if !options.beacon {
            return Ok(());
        }
        let port = options.port;
        let beacon = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        beacon.set_broadcast(true)?;
        commands.insert_resource(Beacon {
            socket: beacon,
            port,
            timer: Timer::from_seconds(1.0, TimerMode::Repeating),
        });
        Ok(())
    }

    pub fn start_client(
        commands: &mut Commands,
        channels: &RepliconChannels,
        server_addr: SocketAddr,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let client_id = rand::random::<u64>().max(1);
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        let authentication = ClientAuthentication::Unsecure {
            client_id,
            protocol_id: PROTOCOL_ID,
            socket_id: 0,
            server_addr,
            user_data: None,
        };
        let transport =
            NetcodeClientTransport::new(now(), authentication, NativeSocket::new(socket)?)?;
        // UDP is unreliable, so renet does its own resends.
        commands.insert_resource(RenetClient::new(connection_config(channels), false));
        commands.insert_resource(transport);
        Ok(client_id)
    }

    /// Join a server with a connect token from the lobby (`net/lobby.rs`).
    /// Returns the player id the token was made for.
    pub fn start_client_secure(
        commands: &mut Commands,
        channels: &RepliconChannels,
        token: ConnectToken,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let client_id = token.client_id;
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        let authentication = ClientAuthentication::Secure {
            connect_token: token,
        };
        let transport =
            NetcodeClientTransport::new(now(), authentication, NativeSocket::new(socket)?)?;
        commands.insert_resource(RenetClient::new(connection_config(channels), false));
        commands.insert_resource(transport);
        Ok(client_id)
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::net::SocketAddr;

    use bevy::prelude::*;
    use bevy_replicon::prelude::*;
    use bevy_replicon_renet2::netcode::ConnectToken;

    use super::HostOptions;

    /// A browser has no UDP sockets: LAN and online play there need the
    /// WebSocket transport, which isn't built yet (docs/web.md).
    const NOT_IN_BROWSER: &str = "multiplayer isn't in the browser version yet";

    pub fn start_server(
        _: &mut Commands,
        _: &RepliconChannels,
        _: &HostOptions,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Err(NOT_IN_BROWSER.into())
    }

    pub fn start_client(
        _: &mut Commands,
        _: &RepliconChannels,
        _: SocketAddr,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        Err(NOT_IN_BROWSER.into())
    }

    pub fn start_client_secure(
        _: &mut Commands,
        _: &RepliconChannels,
        _: ConnectToken,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        Err(NOT_IN_BROWSER.into())
    }
}
