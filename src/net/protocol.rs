//! Everything that crosses the wire: replicated components and messages.
//!
//! Every peer must register these in the same order, which is why they all
//! live in one file and are registered by one function. To sync something
//! new, add it here and to [`register`].

use bevy::{ecs::entity::MapEntities, prelude::*};
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::weapon::WeaponKind;

/// Register everything below with Replicon. The order here is part of the
/// protocol: client and server must match, so only ever add to it in one
/// place.
pub(super) fn register(app: &mut App) {
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
        .add_mapped_server_message::<Damaged>(Channel::Ordered)
        .add_server_message::<HitConfirm>(Channel::Ordered)
        .add_server_message::<Respawn>(Channel::Ordered)
        .add_server_message::<Feed>(Channel::Ordered);
}

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
    /// World-space velocity, used to animate the body.
    pub vel: Vec3,
    /// `Pose::CROUCH | Pose::AIR | Pose::RELOAD`.
    pub flags: u8,
}

impl Pose {
    pub const CROUCH: u8 = 1;
    pub const AIR: u8 = 2;
    pub const RELOAD: u8 = 4;

    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    pub fn set(&mut self, flag: u8, on: bool) {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }
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
    /// `Player::owner` of whoever fired.
    pub shooter: u64,
    pub weapon: WeaponKind,
    pub origin: Vec3,
    pub end: Vec3,
    pub normal: Vec3,
    pub flesh: bool,
}

/// Server -> everyone: `target` was hit. Drives flinches, dummy wobble and,
/// when `killed`, the ragdoll.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug, MapEntities)]
pub struct Damaged {
    #[entities]
    pub target: Entity,
    /// Direction the shot was traveling.
    pub dir: Vec3,
    pub point: Vec3,
    pub weapon: WeaponKind,
    pub killed: bool,
    /// Where the target stood when hit. A killed player may already have
    /// respawned elsewhere by the time this arrives.
    pub pos: Vec3,
    pub yaw: f32,
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
