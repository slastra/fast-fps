//! The authoritative game rules.
//!
//! Despite the name this also runs offline: everything here is active
//! whenever this machine is *not* a client, which covers single player and
//! hosting. Clients never run it; they see the results through replication.
//!
//! - [`players`]: joining, leaving, spawning, movement and hitboxes.
//! - [`combat`]: turning a `Fire` message into damage and kills.
//! - [`dummies`]: the practice targets.
//! - [`bots`]: practice bots driven from the console.

mod bots;
mod combat;
mod dummies;
mod players;

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::server::{
    combat::{Died, receive_fire},
    dummies::{ensure_dummies, move_dummies, tick_downed},
    players::{
        ensure_local_player, hitboxes_follow_pose, on_client_authorized, on_client_removed,
        receive_moves, receive_names, receive_select, receive_suicide,
    },
};

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.add_message::<Died>()
        .add_systems(
            PreUpdate,
            (
                ensure_local_player,
                ensure_dummies,
                receive_names,
                receive_moves,
                receive_fire,
                receive_select,
                receive_suicide,
            )
                .chain()
                .after(ServerSystems::Receive)
                .run_if(authority.clone()),
        )
        .add_systems(
            FixedUpdate,
            (move_dummies, tick_downed).run_if(authority.clone()),
        )
        .add_systems(Update, hitboxes_follow_pose.run_if(authority))
        .add_observer(on_client_authorized)
        .add_observer(on_client_removed)
        .add_plugins(bots::plugin);
}
