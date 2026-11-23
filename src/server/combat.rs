//! Hit detection and damage. A client says "I fired this weapon from here
//! in these directions"; the server ray casts each one against the world and
//! the hitboxes and decides what it hit.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    config::{DUMMY_DOWN_SECS, MAX_HEALTH, SHOT_RANGE},
    net::{
        Damaged, Downed, Feed, Fire, Health, HitConfirm, Player, PlayerName, Pose, Respawn, Score,
        ShotFx,
    },
    physics::Layer,
    server::{
        dummies::DownTimer,
        players::{PlayerClient, find_player, respawn},
    },
};

/// Server-only: a player entity was killed (bots use this to respawn).
#[derive(Message, Clone, Copy)]
pub struct Died(pub Entity);

/// Height above the collider center where the head starts.
fn head_line(pose: &Pose) -> f32 {
    if pose.has(Pose::CROUCH) { 0.12 } else { 0.48 }
}

pub fn receive_fire(
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
    mut damaged: MessageWriter<ToClients<Damaged>>,
    mut died: MessageWriter<Died>,
    owners: Query<&Player>,
) {
    for FromClient { client_id, message } in shots.read() {
        let Some(shooter) = find_player(*client_id, &clients) else {
            continue;
        };
        let shooter_id = owners.get(shooter).map(|p| p.owner).unwrap_or(0);
        let def = message.weapon.def();
        let origin = message.origin;
        if !origin.is_finite() || message.dirs.len() > def.pellets as usize {
            continue;
        }
        let filter = SpatialQueryFilter::from_mask([Layer::World, Layer::Hitbox])
            .with_excluded_entities([shooter]);

        let mut any_hit = false;
        let mut any_head = false;
        let mut any_kill = false;
        // A target killed by an earlier pellet has already respawned at full
        // health, so the rest of this blast must not touch it.
        let mut killed: Vec<Entity> = Vec::new();
        // One `Damaged` per target per blast.
        struct Wound {
            target: Entity,
            dir: Vec3,
            point: Vec3,
            killed: bool,
            pos: Vec3,
            yaw: f32,
        }
        let mut wounds: Vec<Wound> = Vec::new();

        for dir in &message.dirs {
            let Ok(dir) = Dir3::new(*dir) else {
                continue;
            };
            let hit = spatial.cast_ray(origin, dir, SHOT_RANGE, true, &filter);
            let (end, normal) = match hit {
                Some(hit) => (origin + dir * hit.distance, hit.normal),
                None => (origin + dir * SHOT_RANGE, Vec3::ZERO),
            };
            let mut flesh = false;
            if let Some(hit) = hit
                && !killed.contains(&hit.entity)
                && let Ok((mut health, pose, is_player, downed)) = targets.get_mut(hit.entity)
                && !downed
            {
                flesh = true;
                any_hit = true;
                let head = end.y > pose.pos.y + head_line(pose);
                any_head |= head;
                let wound = match wounds.iter().position(|w| w.target == hit.entity) {
                    Some(i) => i,
                    None => {
                        wounds.push(Wound {
                            target: hit.entity,
                            dir: Vec3::ZERO,
                            point: end,
                            killed: false,
                            pos: pose.pos,
                            yaw: pose.yaw,
                        });
                        wounds.len() - 1
                    }
                };
                wounds[wound].dir += dir.as_vec3();
                let falloff = def.falloff_per_10m.powf(hit.distance / 10.0);
                let mult = if head { def.head_mult } else { 1.0 };
                health.0 -= def.damage * mult * falloff;
                if health.0 <= 0.0 {
                    any_kill = true;
                    killed.push(hit.entity);
                    wounds[wound].killed = true;
                    if is_player {
                        health.0 = MAX_HEALTH;
                        if let Ok(mut score) = scores.get_mut(hit.entity) {
                            score.deaths += 1;
                        }
                        if let Ok(mut score) = scores.get_mut(shooter) {
                            score.kills += 1;
                        }
                        let killer = names.get(shooter).map(|n| n.0.as_str()).unwrap_or("?");
                        let victim = names.get(hit.entity).map(|n| n.0.as_str()).unwrap_or("?");
                        feed.write(ToClients {
                            targets: SendTargets::All,
                            message: Feed::Kill {
                                killer: killer.to_string(),
                                victim: victim.to_string(),
                                weapon: message.weapon,
                                headshot: head,
                            },
                        });
                        if let Ok((_, victim_client)) = clients.get(hit.entity) {
                            respawn(victim_client.0, &mut respawns);
                        }
                        died.write(Died(hit.entity));
                    } else {
                        health.0 = 0.0;
                        commands.entity(hit.entity).insert((
                            Downed,
                            DownTimer(Timer::from_seconds(DUMMY_DOWN_SECS, TimerMode::Once)),
                        ));
                    }
                }
            }
            fx.write(ToClients {
                targets: SendTargets::AllExcept(*client_id),
                message: ShotFx {
                    shooter: shooter_id,
                    weapon: message.weapon,
                    origin,
                    end,
                    normal,
                    flesh,
                },
            });
        }

        for wound in wounds {
            damaged.write(ToClients {
                targets: SendTargets::All,
                message: Damaged {
                    target: wound.target,
                    dir: wound.dir.normalize_or_zero(),
                    point: wound.point,
                    weapon: message.weapon,
                    killed: wound.killed,
                    pos: wound.pos,
                    yaw: wound.yaw,
                },
            });
        }
        if any_hit {
            confirm.write(ToClients {
                targets: SendTargets::Single(*client_id),
                message: HitConfirm {
                    head: any_head,
                    kill: any_kill,
                },
            });
        }
    }
}
