//! Player bookkeeping on the server: who is connected, where they spawn,
//! where they are, and what shape their hitbox is.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::{prelude::*, shared::backend::connected_client::NetworkId};
use rand::Rng;

use crate::{
    config::MAX_HEALTH,
    net::{
        Damaged, Duel, Feed, Health, Loadout, LocalName, MoveInput, Player, PlayerName, Pose,
        Respawn, Score, SelectWeapon, SetName, Suicide,
    },
    physics::Layer,
    server::{
        combat::Died,
        duel::{start_spot, watch_spot},
    },
    world::SPAWNS,
};

/// Server-only: which connection a player entity belongs to. Bots have none.
#[derive(Component, Clone, Copy)]
pub struct PlayerClient(pub ClientId);

/// Server-only: whether the hitbox is currently the crouched shape.
#[derive(Component, Default, PartialEq, Clone, Copy)]
pub struct CrouchedHitbox(bool);

/// Standing: 1.8 m tall. Crouched: 1.34 m, keeping the feet where they are.
fn hitbox_shape(crouched: bool) -> Collider {
    if crouched {
        Collider::compound(vec![(
            Vec3::Y * -0.23,
            Quat::IDENTITY,
            Collider::capsule(0.38, 0.58),
        )])
    } else {
        Collider::capsule(0.38, 1.04)
    }
}

pub fn hitbox() -> impl Bundle {
    (
        RigidBody::Kinematic,
        hitbox_shape(false),
        CrouchedHitbox(false),
        CollisionLayers::new(Layer::Hitbox, LayerMask::NONE),
    )
}

fn spawn_player(
    commands: &mut Commands,
    respawns: &mut MessageWriter<ToClients<Respawn>>,
    client: ClientId,
    owner: u64,
    name: String,
) {
    let (pos, yaw) = pick_spawn();
    commands.spawn((
        Replicated,
        Player { owner },
        PlayerName(name),
        Health(MAX_HEALTH),
        Score::default(),
        Loadout::default(),
        Pose {
            pos,
            yaw,
            ..default()
        },
        Transform::from_translation(pos),
        PlayerClient(client),
        hitbox(),
    ));
    respawns.write(ToClients {
        targets: SendTargets::Single(client),
        message: Respawn { pos, yaw },
    });
}

pub fn pick_spawn() -> (Vec3, f32) {
    let mut rng = rand::rng();
    let pos = SPAWNS[rng.random_range(0..SPAWNS.len())];
    // Face roughly toward the middle of the arena.
    let to_center = -pos.with_y(0.0);
    let yaw = (-to_center.x).atan2(-to_center.z);
    (pos, yaw)
}

/// Offline and hosting both need a player for this machine.
pub fn ensure_local_player(
    mut commands: Commands,
    players: Query<&Player>,
    name: Res<LocalName>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
) {
    if !players.iter().any(|p| p.owner == 0) {
        spawn_player(
            &mut commands,
            &mut respawns,
            ClientId::Server,
            0,
            name.0.clone(),
        );
    }
}

pub fn on_client_authorized(
    add: On<Add<AuthorizedClient>>,
    mut commands: Commands,
    ids: Query<&NetworkId>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
) {
    let Ok(id) = ids.get(add.entity) else {
        return;
    };
    spawn_player(
        &mut commands,
        &mut respawns,
        ClientId::Client(add.entity),
        id.get(),
        "newcomer".into(),
    );
    feed.write(ToClients {
        targets: SendTargets::All,
        message: Feed::Note("a player joined".into()),
    });
}

pub fn on_client_removed(
    remove: On<Remove<ConnectedClient>>,
    mut commands: Commands,
    players: Query<(Entity, &PlayerClient, &PlayerName)>,
    mut feed: MessageWriter<ToClients<Feed>>,
) {
    for (entity, client, name) in &players {
        if client.0 == ClientId::Client(remove.entity) {
            feed.write(ToClients {
                targets: SendTargets::All,
                message: Feed::Note(format!("{} left", name.0)),
            });
            commands.entity(entity).despawn();
        }
    }
}

pub fn find_player(client: ClientId, players: &Query<(Entity, &PlayerClient)>) -> Option<Entity> {
    players
        .iter()
        .find(|(_, c)| c.0 == client)
        .map(|(entity, _)| entity)
}

pub fn receive_names(
    mut names: MessageReader<FromClient<SetName>>,
    clients: Query<(Entity, &PlayerClient)>,
    mut players: Query<&mut PlayerName>,
    mut feed: MessageWriter<ToClients<Feed>>,
) {
    for FromClient { client_id, message } in names.read() {
        let Some(entity) = find_player(*client_id, &clients) else {
            continue;
        };
        let Ok(mut name) = players.get_mut(entity) else {
            continue;
        };
        let new: String = message.0.trim().chars().take(20).collect();
        if new.is_empty() || new == name.0 {
            continue;
        }
        let line = if name.0 == "newcomer" {
            format!("{new} joined the fight")
        } else {
            format!("{} is now {new}", name.0)
        };
        feed.write(ToClients {
            targets: SendTargets::All,
            message: Feed::Note(line),
        });
        name.0 = new;
    }
}

pub fn receive_moves(
    mut moves: MessageReader<FromClient<MoveInput>>,
    clients: Query<(Entity, &PlayerClient)>,
    mut poses: Query<&mut Pose, With<Player>>,
) {
    for FromClient { client_id, message } in moves.read() {
        let Some(entity) = find_player(*client_id, &clients) else {
            continue;
        };
        if let Ok(mut pose) = poses.get_mut(entity)
            && message.0.pos.is_finite()
            && message.0.vel.is_finite()
        {
            *pose = message.0;
        }
    }
}

pub fn hitboxes_follow_pose(
    mut commands: Commands,
    mut query: Query<(Entity, &Pose, &mut Transform, &mut CrouchedHitbox), Changed<Pose>>,
) {
    for (entity, pose, mut transform, mut crouched) in &mut query {
        transform.translation = pose.pos;
        transform.rotation = Quat::from_rotation_y(pose.yaw);
        let now = pose.has(Pose::CROUCH);
        if crouched.0 != now {
            crouched.0 = now;
            commands.entity(entity).insert(hitbox_shape(now));
        }
    }
}

pub fn receive_select(
    mut selects: MessageReader<FromClient<SelectWeapon>>,
    clients: Query<(Entity, &PlayerClient)>,
    mut loadouts: Query<&mut Loadout>,
) {
    for FromClient { client_id, message } in selects.read() {
        if let Some(entity) = find_player(*client_id, &clients)
            && let Ok(mut loadout) = loadouts.get_mut(entity)
        {
            loadout.set_if_neq(Loadout(message.0));
        }
    }
}

/// Send a fresh spawn point to the client; its pose follows on the next move.
pub fn respawn(client: ClientId, respawns: &mut MessageWriter<ToClients<Respawn>>) {
    let (pos, yaw) = pick_spawn();
    respawn_at(client, pos, yaw, respawns);
}

/// Send the client to a chosen spot.
pub fn respawn_at(
    client: ClientId,
    pos: Vec3,
    yaw: f32,
    respawns: &mut MessageWriter<ToClients<Respawn>>,
) {
    respawns.write(ToClients {
        targets: SendTargets::Single(client),
        message: Respawn { pos, yaw },
    });
}

#[allow(clippy::too_many_arguments)]
pub fn receive_suicide(
    mut requests: MessageReader<FromClient<Suicide>>,
    clients: Query<(Entity, &PlayerClient)>,
    mut players: Query<(&mut Health, &mut Score, &PlayerName, &Pose, &Loadout)>,
    owners: Query<&Player>,
    duels: Query<&Duel>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut damaged: MessageWriter<ToClients<Damaged>>,
    mut died: MessageWriter<Died>,
) {
    for FromClient { client_id, .. } in requests.read() {
        let Some(entity) = find_player(*client_id, &clients) else {
            continue;
        };
        // In a duel, `kill` (or falling off the world) just puts you back
        // where you belong this round.
        if let Ok(duel) = duels.single() {
            let slot = owners.get(entity).ok().and_then(|p| duel.slot(p.owner));
            let (pos, yaw) = slot.map_or_else(watch_spot, start_spot);
            respawn_at(*client_id, pos, yaw, &mut respawns);
            continue;
        }
        let Ok((mut health, mut score, name, pose, loadout)) = players.get_mut(entity) else {
            continue;
        };
        damaged.write(ToClients {
            targets: SendTargets::All,
            message: Damaged {
                target: entity,
                dir: Vec3::ZERO,
                point: pose.pos,
                weapon: loadout.0,
                killed: true,
                pos: pose.pos,
                yaw: pose.yaw,
            },
        });
        health.0 = MAX_HEALTH;
        score.deaths += 1;
        feed.write(ToClients {
            targets: SendTargets::All,
            message: Feed::Note(format!("{} gave up", name.0)),
        });
        died.write(Died(entity));
        respawn(*client_id, &mut respawns);
    }
}
