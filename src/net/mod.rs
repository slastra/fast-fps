//! Networking.
//!
//! The game uses Replicon's recommended layout. Authoritative logic (damage,
//! respawns, dummies, bots) runs whenever this machine is *not* a client, so
//! the same code drives offline play and a listen server. Clients only send
//! intent: where they are and where they shot.
//!
//! - [`protocol`]: everything that crosses the wire. Start here.
//! - [`session`]: hosting, connecting and leaving.
//! - [`discovery`]: finding servers on the LAN.
//! - [`admin`]: sending host-only commands to the server you're on.
//! - [`lobby`]: finding, starting and joining games online.

mod admin;
mod discovery;
mod lobby;
mod protocol;
mod session;

pub use protocol::*;
pub use session::{HostOptions, LocalId, LocalName, start_server};

use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    protocol::register(app);
    app.add_plugins((
        session::plugin,
        discovery::plugin,
        admin::plugin,
        lobby::plugin,
    ));
}
