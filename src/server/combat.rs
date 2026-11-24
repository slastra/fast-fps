//! Hit detection and damage. A client says "I fired this weapon from here
//! in these directions"; the server ray casts each one against the world and
//! the hitboxes and decides what it hit. Knife attacks work the same way with
//! a short swept sphere. [`Hits`] applies the damage for every weapon,
//! grenades included.

use avian3d::prelude::*;
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_replicon::prelude::*;

use crate::{
    config::{DUMMY_DOWN_SECS, MAX_HEALTH, SHOT_RANGE},
    net::{
        Damaged, Downed, Feed, Fire, Health, HitConfirm, Melee, Player, PlayerName, Pose, Respawn,
        Score, ShotFx,
    },
    physics::Layer,
    server::{
        dummies::DownTimer,
        players::{PlayerClient, find_player, respawn},
    },
    weapon::{Class, KNIFE, WeaponKind},
};

/// Server-only: a player entity was killed (bots use this to respawn).
#[derive(Message, Clone, Copy)]
pub struct Died(pub Entity);

/// Height above the collider center where the head starts.
pub fn head_line(pose: &Pose) -> f32 {
    if pose.has(Pose::CROUCH) { 0.12 } else { 0.48 }
}

/// What one hit did to its target.
pub struct Hurt {
    pub killed: bool,
    /// Where the target was standing and facing when hit, for the `Damaged`
    /// message (corpses spawn there, not at the respawned entity).
    pub pos: Vec3,
    pub yaw: f32,
}

/// Everything needed to damage players and dummies, shared by guns, the
/// knife and grenades so the rules for kills, scores, the feed and respawns
/// live in one place.
#[derive(SystemParam)]
pub struct Hits<'w, 's> {
    pub clients: Query<'w, 's, (Entity, &'static PlayerClient)>,
    pub targets: Query<'w, 's, (&'static mut Health, &'static Pose, Has<Player>, Has<Downed>)>,
    scores: Query<'w, 's, &'static mut Score>,
    names: Query<'w, 's, &'static PlayerName>,
    pub owners: Query<'w, 's, &'static Player>,
    commands: Commands<'w, 's>,
    confirm: MessageWriter<'w, ToClients<HitConfirm>>,
    feed: MessageWriter<'w, ToClients<Feed>>,
    respawns: MessageWriter<'w, ToClients<Respawn>>,
    damaged: MessageWriter<'w, ToClients<Damaged>>,
    died: MessageWriter<'w, Died>,
}

impl Hits<'_, '_> {
    /// Take `amount` health from `target`. A kill scores for `attacker`
    /// (unless it was themselves), goes in the feed and respawns the victim.
    /// `None` if the target can't be hurt.
    pub fn hurt(
        &mut self,
        attacker: Entity,
        target: Entity,
        amount: f32,
        head: bool,
        weapon: WeaponKind,
    ) -> Option<Hurt> {
        let Ok((mut health, pose, is_player, downed)) = self.targets.get_mut(target) else {
            return None;
        };
        if downed {
            return None;
        }
        let mut hurt = Hurt {
            killed: false,
            pos: pose.pos,
            yaw: pose.yaw,
        };
        health.0 -= amount;
        if health.0 > 0.0 {
            return Some(hurt);
        }
        hurt.killed = true;
        if !is_player {
            health.0 = 0.0;
            self.commands.entity(target).insert((
                Downed,
                DownTimer(Timer::from_seconds(DUMMY_DOWN_SECS, TimerMode::Once)),
            ));
            return Some(hurt);
        }
        health.0 = MAX_HEALTH;
        if let Ok(mut score) = self.scores.get_mut(target) {
            score.deaths += 1;
        }
        if attacker != target
            && let Ok(mut score) = self.scores.get_mut(attacker)
        {
            score.kills += 1;
        }
        let name = |e| {
            self.names
                .get(e)
                .map(|n| n.0.clone())
                .unwrap_or_else(|_| "?".into())
        };
        self.feed.write(ToClients {
            targets: SendTargets::All,
            message: Feed::Kill {
                killer: name(attacker),
                victim: name(target),
                weapon,
                headshot: head,
            },
        });
        if let Ok((_, victim_client)) = self.clients.get(target) {
            respawn(victim_client.0, &mut self.respawns);
        }
        self.died.write(Died(target));
        Some(hurt)
    }

    /// Tell everyone a target was hit (for blood, flinches and ragdolls).
    pub fn wounded(
        &mut self,
        target: Entity,
        hurt: &Hurt,
        dir: Vec3,
        point: Vec3,
        weapon: WeaponKind,
    ) {
        self.damaged.write(ToClients {
            targets: SendTargets::All,
            message: Damaged {
                target,
                dir: dir.normalize_or_zero(),
                point,
                weapon,
                killed: hurt.killed,
                pos: hurt.pos,
                yaw: hurt.yaw,
            },
        });
    }

    /// The hit marker for whoever caused the damage, if they're a player.
    pub fn confirm(&mut self, attacker: Entity, head: bool, kill: bool) {
        if let Ok((_, client)) = self.clients.get(attacker) {
            self.confirm.write(ToClients {
                targets: SendTargets::Single(client.0),
                message: HitConfirm { head, kill },
            });
        }
    }
}

pub fn receive_fire(
    mut shots: MessageReader<FromClient<Fire>>,
    spatial: SpatialQuery,
    mut hits: Hits,
    mut fx: MessageWriter<ToClients<ShotFx>>,
) {
    for FromClient { client_id, message } in shots.read() {
        let Some(shooter) = find_player(*client_id, &hits.clients) else {
            continue;
        };
        let shooter_id = hits.owners.get(shooter).map(|p| p.owner).unwrap_or(0);
        let def = message.weapon.def();
        let origin = message.origin;
        if def.class != Class::Gun
            || !origin.is_finite()
            || message.dirs.len() > def.pellets as usize
        {
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
        // One `Damaged` per target per blast: (target, summed dir, first point, result).
        let mut wounds: Vec<(Entity, Vec3, Vec3, Hurt)> = Vec::new();

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
                && let Ok((_, pose, ..)) = hits.targets.get(hit.entity)
            {
                let head = end.y > pose.pos.y + head_line(pose);
                let falloff = def.falloff_per_10m.powf(hit.distance / 10.0);
                let mult = if head { def.head_mult } else { 1.0 };
                if let Some(hurt) = hits.hurt(
                    shooter,
                    hit.entity,
                    def.damage * mult * falloff,
                    head,
                    message.weapon,
                ) {
                    flesh = true;
                    any_hit = true;
                    any_head |= head;
                    if hurt.killed {
                        any_kill = true;
                        killed.push(hit.entity);
                    }
                    match wounds.iter_mut().find(|w| w.0 == hit.entity) {
                        Some(w) => {
                            w.1 += dir.as_vec3();
                            w.3.killed |= hurt.killed;
                        }
                        None => wounds.push((hit.entity, dir.as_vec3(), end, hurt)),
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

        for (target, dir, point, hurt) in wounds {
            hits.wounded(target, &hurt, dir, point, message.weapon);
        }
        if any_hit {
            hits.confirm(shooter, any_head, any_kill);
        }
    }
}

/// Knife attacks. The swing is a short swept sphere from the eye, so it
/// forgives a little aim; a hit from behind kills outright.
pub fn receive_melee(
    mut attacks: MessageReader<FromClient<Melee>>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut hits: Hits,
    mut fx: MessageWriter<ToClients<ShotFx>>,
    // When each player may swing again, so a client can't swing faster than
    // the knife allows.
    mut ready: Local<bevy::platform::collections::HashMap<Entity, f32>>,
) {
    let now = time.elapsed_secs();
    for FromClient { client_id, message } in attacks.read() {
        let Some(attacker) = find_player(*client_id, &hits.clients) else {
            continue;
        };
        let Ok(dir) = Dir3::new(message.dir) else {
            continue;
        };
        if !message.origin.is_finite() {
            continue;
        }
        // A little slack for network jitter.
        if ready.get(&attacker).is_some_and(|&at| now < at - 0.1) {
            continue;
        }
        let interval = if message.heavy {
            KNIFE.stab_secs
        } else {
            KNIFE.slash_secs
        };
        ready.insert(attacker, now + interval);

        let attacker_id = hits.owners.get(attacker).map(|p| p.owner).unwrap_or(0);
        let filter = SpatialQueryFilter::from_mask([Layer::World, Layer::Hitbox])
            .with_excluded_entities([attacker]);
        let hit = spatial.cast_shape(
            &Collider::sphere(KNIFE.width),
            message.origin,
            Quat::IDENTITY,
            dir,
            &ShapeCastConfig::from_max_distance(KNIFE.reach),
            &filter,
        );
        let mut end = message.origin + dir * KNIFE.reach;
        let mut normal = Vec3::ZERO;
        let mut flesh = false;
        if let Some(hit) = hit {
            end = hit.point1;
            normal = hit.normal1;
            if let Ok((_, pose, ..)) = hits.targets.get(hit.entity) {
                let amount = if KNIFE.is_behind(dir.as_vec3(), pose.yaw) {
                    // Enough to kill from full health.
                    1000.0
                } else if message.heavy {
                    KNIFE.stab
                } else {
                    KNIFE.slash
                };
                if let Some(hurt) =
                    hits.hurt(attacker, hit.entity, amount, false, WeaponKind::Knife)
                {
                    flesh = true;
                    let kill = hurt.killed;
                    hits.wounded(hit.entity, &hurt, dir.as_vec3(), end, WeaponKind::Knife);
                    hits.confirm(attacker, false, kill);
                }
            }
        }
        fx.write(ToClients {
            targets: SendTargets::AllExcept(*client_id),
            message: ShotFx {
                shooter: attacker_id,
                weapon: WeaponKind::Knife,
                origin: message.origin,
                end,
                normal,
                flesh,
            },
        });
    }
}
