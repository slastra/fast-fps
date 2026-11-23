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

mod discovery;
mod protocol;
mod session;

pub use protocol::*;
pub use session::{LocalId, LocalName};

use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    protocol::register(app);
    app.add_plugins((session::plugin, discovery::plugin));
}
