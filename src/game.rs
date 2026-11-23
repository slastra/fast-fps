//! Authoritative game rules: spawning players, applying hits, respawns and
//! the practice dummies. These run whenever this machine is *not* a client,
//! which covers both offline play and hosting.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::{prelude::*, shared::backend::connected_client::NetworkId};
use rand::Rng;

use crate::{
    Layer,
    level::{DUMMIES, SPAWNS},
    net::{
        Downed, Dummy, Feed, Fire, Health, HitConfirm, LocalName, MoveInput, Player, PlayerName,
        Pose, Respawn, Score, SetName, ShotFx, Suicide,
    },
};

pub const MAX_HEALTH: f32 = 100.0;
const BODY_DAMAGE: f32 = 20.0;
const HEAD_DAMAGE: f32 = 55.0;
const RANGE: f32 = 250.0;
const DUMMY_DOWN_SECS: f32 = 3.0;

/// Server-only: which connection a player entity belongs to.
#[derive(Component, Clone, Copy)]
struct PlayerClient(ClientId);

#[derive(Component)]
struct DownTimer(Timer);

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.add_systems(
        PreUpdate,
        (
            ensure_world,
            receive_names,
            receive_moves,
            receive_fire,
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
    .add_observer(on_client_removed);
}

fn hitbox() -> impl Bundle {
    (
        RigidBody::Kinematic,
        Collider::capsule(0.38, 1.04),
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
        Pose {
            pos,
            yaw,
            pitch: 0.0,
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

fn pick_spawn() -> (Vec3, f32) {
    let mut rng = rand::rng();
    let pos = SPAWNS[rng.random_range(0..SPAWNS.len())];
    // Face roughly toward the middle of the arena.
    let to_center = -pos.with_y(0.0);
    let yaw = (-to_center.x).atan2(-to_center.z);
    (pos, yaw)
}

/// Offline and hosting both need a player for this machine and the dummies.
fn ensure_world(
    mut commands: Commands,
    players: Query<&Player>,
    dummies: Query<(), With<Dummy>>,
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
    if dummies.is_empty() {
        for (anchor, sway) in DUMMIES {
            commands.spawn((
                Replicated,
                Dummy { anchor, sway },
                Health(MAX_HEALTH),
                Pose {
                    pos: anchor,
                    yaw: std::f32::consts::FRAC_PI_2,
                    pitch: 0.0,
                },
                Transform::from_translation(anchor),
                hitbox(),
            ));
        }
    }
}

fn on_client_authorized(
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
        message: Feed("a player joined".into()),
    });
}

fn on_client_removed(
    remove: On<Remove<ConnectedClient>>,
    mut commands: Commands,
    players: Query<(Entity, &PlayerClient, &PlayerName)>,
    mut feed: MessageWriter<ToClients<Feed>>,
) {
    for (entity, client, name) in &players {
        if client.0 == ClientId::Client(remove.entity) {
            feed.write(ToClients {
                targets: SendTargets::All,
                message: Feed(format!("{} left", name.0)),
            });
            commands.entity(entity).despawn();
        }
    }
}

fn find_player(client: ClientId, players: &Query<(Entity, &PlayerClient)>) -> Option<Entity> {
    players
        .iter()
        .find(|(_, c)| c.0 == client)
        .map(|(entity, _)| entity)
}

fn receive_names(
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
            message: Feed(line),
        });
        name.0 = new;
    }
}

fn receive_moves(
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
        {
            *pose = message.0;
        }
    }
}

fn hitboxes_follow_pose(mut query: Query<(&Pose, &mut Transform), Changed<Pose>>) {
    for (pose, mut transform) in &mut query {
        transform.translation = pose.pos;
        transform.rotation = Quat::from_rotation_y(pose.yaw);
    }
}

fn receive_fire(
    mut shots: MessageReader<FromClient<Fire>>,
    clients: Query<(Entity, &PlayerClient)>,
    spatial: SpatialQuery,
    mut targets: Query<(&mut Health, &Pose, Has<Player>, Has<Downed>)>,
    mut scores: Query<&mut Score>,
    names: Query<&PlayerName>,
    mut commands: Commands,
    mut fx: MessageWriter<ToClients<ShotFx>>,
    mut confirm: MessageWriter<ToClients<HitConfirm>>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
) {
    for FromClient { client_id, message } in shots.read() {
        let Some(shooter) = find_player(*client_id, &clients) else {
            continue;
        };
        let Ok(dir) = Dir3::new(message.dir) else {
            continue;
        };
        let origin = message.origin;
        if !origin.is_finite() {
            continue;
        }
        let filter = SpatialQueryFilter::from_mask([Layer::World, Layer::Hitbox])
            .with_excluded_entities([shooter]);
        let hit = spatial.cast_ray(origin, dir, RANGE, true, &filter);
        let (end, normal) = match hit {
            Some(hit) => (origin + dir * hit.distance, hit.normal),
            None => (origin + dir * RANGE, Vec3::ZERO),
        };

        let mut flesh = false;
        if let Some(hit) = hit
            && let Ok((mut health, pose, is_player, downed)) = targets.get_mut(hit.entity)
            && !downed
        {
            flesh = true;
            let head = end.y > pose.pos.y + 0.48;
            health.0 -= if head { HEAD_DAMAGE } else { BODY_DAMAGE };
            let kill = health.0 <= 0.0;
            confirm.write(ToClients {
                targets: SendTargets::Single(*client_id),
                message: HitConfirm { head, kill },
            });
            if kill && is_player {
                health.0 = MAX_HEALTH;
                if let Ok(mut score) = scores.get_mut(hit.entity) {
                    score.deaths += 1;
                }
                if let Ok(mut score) = scores.get_mut(shooter) {
                    score.kills += 1;
                }
                let killer = names.get(shooter).map(|n| n.0.as_str()).unwrap_or("?");
                let victim = names.get(hit.entity).map(|n| n.0.as_str()).unwrap_or("?");
                let how = if head { " (headshot)" } else { "" };
                feed.write(ToClients {
                    targets: SendTargets::All,
                    message: Feed(format!("{killer} fragged {victim}{how}")),
                });
                if let Some(victim_client) = clients.get(hit.entity).ok().map(|(_, c)| c.0) {
                    respawn(victim_client, &mut respawns);
                }
            } else if kill {
                health.0 = 0.0;
                commands.entity(hit.entity).insert((
                    Downed,
                    DownTimer(Timer::from_seconds(DUMMY_DOWN_SECS, TimerMode::Once)),
                ));
            }
        }

        fx.write(ToClients {
            targets: SendTargets::AllExcept(*client_id),
            message: ShotFx {
                origin,
                end,
                normal,
                flesh,
            },
        });
    }
}

/// Send a fresh spawn point to the client; its pose follows on the next move.
fn respawn(client: ClientId, respawns: &mut MessageWriter<ToClients<Respawn>>) {
    let (pos, yaw) = pick_spawn();
    respawns.write(ToClients {
        targets: SendTargets::Single(client),
        message: Respawn { pos, yaw },
    });
}

fn receive_suicide(
    mut requests: MessageReader<FromClient<Suicide>>,
    clients: Query<(Entity, &PlayerClient)>,
    mut players: Query<(&mut Health, &mut Score, &PlayerName)>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
    mut feed: MessageWriter<ToClients<Feed>>,
) {
    for FromClient { client_id, .. } in requests.read() {
        let Some(entity) = find_player(*client_id, &clients) else {
            continue;
        };
        let Ok((mut health, mut score, name)) = players.get_mut(entity) else {
            continue;
        };
        health.0 = MAX_HEALTH;
        score.deaths += 1;
        feed.write(ToClients {
            targets: SendTargets::All,
            message: Feed(format!("{} gave up", name.0)),
        });
        respawn(*client_id, &mut respawns);
    }
}

fn move_dummies(time: Res<Time>, mut dummies: Query<(&Dummy, &mut Pose), Without<Downed>>) {
    let t = time.elapsed_secs();
    for (dummy, mut pose) in &mut dummies {
        if dummy.sway == 0.0 {
            continue;
        }
        let phase = dummy.anchor.z * 0.7;
        pose.pos = dummy.anchor + Vec3::Z * (t * 0.9 + phase).sin() * dummy.sway;
    }
}

fn tick_downed(
    time: Res<Time>,
    mut commands: Commands,
    mut dummies: Query<(Entity, &mut DownTimer, &mut Health)>,
) {
    for (entity, mut timer, mut health) in &mut dummies {
        if timer.0.tick(time.delta()).just_finished() {
            health.0 = MAX_HEALTH;
            commands.entity(entity).remove::<(Downed, DownTimer)>();
        }
    }
}
