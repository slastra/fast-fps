//! Opening connections, all under renet2's netcode.
//!
//! - Natively, players connect over UDP.
//! - A dedicated server can also listen for browser players over
//!   WebTransport (QUIC), with a self-signed certificate it makes at
//!   startup. Browsers check it by its hash, which the lobby hands them with
//!   their ticket (docs/online.md). Both kinds of player share one game.
//! - In a browser, the client side of that WebTransport connection.

use std::{net::SocketAddr, time::Duration};

use bevy_replicon::prelude::*;
use bevy_replicon_renet2::{RenetChannelsExt, renet2::ConnectionConfig};
#[cfg(not(target_arch = "wasm32"))]
use std::time::SystemTime;
// std's clock panics in a browser.
#[cfg(target_arch = "wasm32")]
use web_time::SystemTime;

pub use imp::*;

fn connection_config(channels: &RepliconChannels) -> ConnectionConfig {
    ConnectionConfig::from_channels(channels.server_configs(), channels.client_configs())
}

/// Netcode's clock: time since the Unix epoch (tokens expire by it).
fn now() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
}

/// How a lobby ticket reaches its game: over UDP (the native game), or over
/// WebTransport (a browser), to a game whose self-signed certificate has
/// this SHA-256 hash.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum Route {
    Udp,
    WebTransport {
        addr: SocketAddr,
        cert_hash: [u8; 32],
    },
}

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
    /// Also listen for browser players over WebTransport on this UDP port.
    pub webtransport: Option<u16>,
}

/// What hosting set up: the WebTransport certificate's SHA-256 hash, if
/// browser players can join, for the lobby to hand them.
pub type CertHash = Option<[u8; 32]>;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

    use bevy::prelude::*;
    use bevy_replicon::prelude::*;
    use bevy_replicon_renet2::{
        netcode::{
            BoxedSocket, ClientAuthentication, ConnectToken, NativeSocket, NetcodeClientTransport,
            NetcodeServerTransport, ServerAuthentication, ServerSetupConfig, WebTransportServer,
            WebTransportServerConfig,
        },
        renet2::{RenetClient, RenetServer},
    };

    use super::{CertHash, HostOptions, Route, connection_config, now};
    use crate::{
        config::{CONNECT_TOKEN_SECS, MAX_CLIENTS, PROTOCOL_ID},
        net::discovery::Beacon,
    };

    /// The async runtime the WebTransport socket runs on, kept for as long
    /// as the server is up.
    #[derive(Resource)]
    pub struct WebTransportRuntime(#[allow(dead_code)] tokio::runtime::Runtime);

    pub fn start_server(
        commands: &mut Commands,
        channels: &RepliconChannels,
        options: &HostOptions,
    ) -> Result<CertHash, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, options.port))?;
        let address = options.public.unwrap_or(socket.local_addr()?);
        let authentication = match options.key {
            Some(private_key) => ServerAuthentication::Secure { private_key },
            None => ServerAuthentication::Unsecure,
        };
        // Socket 0 is UDP for native players; socket 1, if any, is
        // WebTransport for browser players. Tokens name the socket.
        let mut sockets = vec![BoxedSocket::new(NativeSocket::new(socket)?)];
        let mut socket_addresses = vec![vec![address]];
        let mut cert = None;
        if let Some(port) = options.webtransport {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()?;
            let listen = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
            let public = SocketAddr::new(address.ip(), port);
            let (config, hash) = WebTransportServerConfig::new_selfsigned_with_proxies(
                listen,
                vec![public.into()],
                MAX_CLIENTS,
            )
            .map_err(|e| format!("webtransport certificate: {e}"))?;
            let server = WebTransportServer::new(config, runtime.handle().clone())
                .map_err(|e| format!("webtransport on UDP {port}: {e}"))?;
            sockets.push(BoxedSocket::new(server));
            socket_addresses.push(vec![public]);
            commands.insert_resource(WebTransportRuntime(runtime));
            cert = Some(hash.hash);
        }
        let config = ServerSetupConfig {
            current_time: now(),
            max_token_lifetime: CONNECT_TOKEN_SECS,
            max_clients: MAX_CLIENTS,
            protocol_id: PROTOCOL_ID,
            authentication,
            socket_addresses,
        };
        let transport = NetcodeServerTransport::new_with_sockets(config, sockets)?;
        commands.insert_resource(RenetServer::new(connection_config(channels)));
        commands.insert_resource(transport);

        if !options.beacon {
            return Ok(cert);
        }
        let port = options.port;
        let beacon = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        beacon.set_broadcast(true)?;
        commands.insert_resource(Beacon {
            socket: beacon,
            port,
            timer: Timer::from_seconds(1.0, TimerMode::Repeating),
        });
        Ok(cert)
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
        route: Route,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        if !matches!(route, Route::Udp) {
            return Err("the native game connects over UDP".into());
        }
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
    use bevy_replicon_renet2::{
        netcode::{
            ClientAuthentication, ConnectToken, NetcodeClientTransport, ServerCertHash,
            WebTransportClient, WebTransportClientConfig,
            webtransport_is_available_with_cert_hashes,
        },
        renet2::RenetClient,
    };

    use super::{CertHash, HostOptions, Route, connection_config, now};

    /// A browser has no UDP sockets and can't host: it joins games through
    /// the lobby, over WebTransport.
    const NOT_IN_BROWSER: &str = "the browser version can't host or join by address; \
         use the lobby: games, create, join";

    pub fn start_server(
        _: &mut Commands,
        _: &RepliconChannels,
        _: &HostOptions,
    ) -> Result<CertHash, Box<dyn std::error::Error>> {
        Err(NOT_IN_BROWSER.into())
    }

    pub fn start_client(
        _: &mut Commands,
        _: &RepliconChannels,
        _: SocketAddr,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        Err(NOT_IN_BROWSER.into())
    }

    /// Join a game over WebTransport, trusting its self-signed certificate
    /// by the hash the lobby gave (`serverCertificateHashes`).
    pub fn start_client_secure(
        commands: &mut Commands,
        channels: &RepliconChannels,
        token: ConnectToken,
        route: Route,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let Route::WebTransport { addr, cert_hash } = route else {
            return Err("this server doesn't take browser players".into());
        };
        if !webtransport_is_available_with_cert_hashes() {
            return Err(
                "this browser can't make the WebTransport connection the game \
                 needs; try a recent Chrome or Edge"
                    .into(),
            );
        }
        let client_id = token.client_id;
        let config = WebTransportClientConfig::new_with_certs(
            addr,
            vec![ServerCertHash { hash: cert_hash }],
        );
        let authentication = ClientAuthentication::Secure {
            connect_token: token,
        };
        let transport =
            NetcodeClientTransport::new(now(), authentication, WebTransportClient::new(config))?;
        // WebTransport datagrams are unreliable, so renet does its own resends.
        commands.insert_resource(RenetClient::new(connection_config(channels), false));
        commands.insert_resource(transport);
        Ok(client_id)
    }
}
