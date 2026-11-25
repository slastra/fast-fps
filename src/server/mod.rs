//! The authoritative game rules.
//!
//! Despite the name this also runs offline: everything here is active
//! whenever this machine is *not* a client, which covers single player and
//! hosting. Clients never run it; they see the results through replication.
//!
//! - [`players`]: joining, leaving, spawning, movement and hitboxes.
//! - [`combat`]: turning `Fire` and `Melee` messages into damage and kills.
//! - [`grenades`]: thrown grenades, their flight and their blast.
//! - [`dummies`]: the practice targets.
//! - [`bots`]: practice bots driven from the console.
//! - [`duel`]: the duel game mode (`mode duel`).

mod admin;
mod bots;
mod combat;
mod duel;
mod dummies;
mod grenades;
mod players;

use bevy::prelude::*;
use bevy_replicon::prelude::*;

pub use players::PlayerClient;

use crate::dedicated::Dedicated;
use crate::server::{
    combat::{BotShot, Died, receive_bot_shots, receive_fire, receive_melee},
    dummies::{ensure_dummies, move_dummies, tick_downed},
    grenades::{receive_throws, tick_grenades},
    players::{
        ensure_local_player, hitboxes_follow_pose, on_client_authorized, on_client_removed,
        receive_moves, receive_names, receive_select, receive_suicide, stamp_joined,
    },
};

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.add_message::<Died>()
        .add_message::<BotShot>()
        .add_systems(
            PreUpdate,
            (
                // A dedicated server has nobody sitting at it.
                ensure_local_player.run_if(not(resource_exists::<Dedicated>)),
                stamp_joined,
                ensure_dummies,
                receive_names,
                receive_moves,
                receive_fire,
                receive_melee,
                receive_select,
                receive_suicide,
                receive_throws,
            )
                .chain()
                .after(ServerSystems::Receive)
                .run_if(authority.clone()),
        )
        .add_systems(
            FixedUpdate,
            (move_dummies, tick_downed).run_if(authority.clone()),
        )
        .add_systems(
            Update,
            (hitboxes_follow_pose, tick_grenades, receive_bot_shots).run_if(authority),
        )
        .add_observer(on_client_authorized)
        .add_observer(on_client_removed)
        .add_plugins((admin::plugin, bots::plugin, duel::plugin, grenades::plugin));
}
