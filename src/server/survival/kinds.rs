//! What sets some kinds of zombie apart from walking up and swinging:
//!
//! - The leaper stops, crouches (a tell you can see), then leaps up to
//!   [`LEAP_RANGE`] in an arc and lands on you. Its landing is hit in
//!   `zombies::attack`.
//! - The bloater bursts when it reaches you or when it's killed: a blast
//!   that hurts players and other zombies, then gas that lingers.
//!
//! The crawler needs nothing here: a low hitbox and a crawling pose
//! (`zombies.rs`), and packs (`zombies::fill_wave`).

use std::f32::consts::PI;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use super::{
    effects::SECONDARY,
    status::Status,
    zombies::{Zombie, can_see, ground, kind},
};
use crate::{
    net::{Dead, Out, Player, Pose, to_all},
    physics::Layer,
    server::{
        combat::{Blast, Hits, blast},
        rules::Died,
    },
    survival::protocol::{BLOATER, Fallen, Flash, LEAPER},
    weapon::WeaponKind,
};

/// A leaper leaps at players this far away.
pub const LEAP_RANGE: (f32, f32) = (3.5, 9.0);
/// How long it crouches first: long enough to see and sidestep.
const LEAP_TELL: f32 = 0.6;
/// How high the arc goes, and its speed over the ground.
const LEAP_HEIGHT: f32 = 1.4;
const LEAP_SPEED: f32 = 12.0;
/// It stays put this long after landing, and waits this long between leaps.
const LEAP_RECOVER: f32 = 0.6;
const LEAP_WAIT: f32 = 3.5;
/// Who the landing hits, and how hard.
pub const LEAP_RADIUS: f32 = 1.7;
pub const LEAP_DAMAGE: f32 = 30.0;

/// The bloater's gas: how far it reaches, how long it lingers, how much it
/// hurts each half second.
const GAS_RADIUS: f32 = 3.0;
const GAS_SECS: f32 = 4.0;
const GAS_TICK: f32 = 4.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Leap {
    #[default]
    Ready,
    /// Crouched, about to go, tracking `target`.
    Crouch { t: f32, target: Vec3 },
    Air {
        from: Vec3,
        to: Vec3,
        t: f32,
        secs: f32,
    },
    /// Just landed.
    Land { t: f32 },
}

impl Leap {
    pub fn busy(&self) -> bool {
        *self != Leap::Ready
    }
}

/// Server-only: a bloater's gas, lingering where it burst.
#[derive(Component)]
pub struct Gas {
    at: Vec3,
    left: f32,
    tick: f32,
    /// Players are hurt by the bloater; zombies by whoever killed it.
    bloater: Entity,
    killer: Entity,
}

/// Leapers watch for someone in range, crouch, leap and land.
#[allow(clippy::type_complexity)]
pub fn leap(
    time: Res<Time>,
    spatial: SpatialQuery,
    mut zombies: Query<(&mut Zombie, &mut Pose, &Status), (Without<Dead>, Without<Player>)>,
    players: Query<&Pose, (With<Player>, Without<Zombie>, Without<Fallen>, Without<Out>)>,
    mut flash: MessageWriter<ToClients<Flash>>,
) {
    let dt = time.delta_secs();
    let walls = SpatialQueryFilter::from_mask(Layer::World);
    for (mut zombie, mut pose, status) in &mut zombies {
        if zombie.kind != LEAPER {
            continue;
        }
        let at = pose.pos;
        // The nearest player it could leap at: in range, in sight, on its level.
        let prey = players
            .iter()
            .map(|p| p.pos)
            .filter(|p| {
                let d = p.with_y(at.y).distance(at);
                d < LEAP_RANGE.1 + 1.0
                    && (p.y - at.y).abs() < 1.2
                    && can_see(&spatial, &walls, at, *p)
            })
            .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)));
        let face = |to: Vec3| {
            let d = Vec2::new(to.x - at.x, to.z - at.z);
            (d.length() > 0.01).then(|| (-d.x).atan2(-d.y))
        };
        let mut flags = Pose::REACH;
        let mut next = at;
        zombie.leap = match zombie.leap {
            Leap::Ready => {
                if status.frozen() {
                    continue;
                }
                zombie.leap_wait -= dt;
                match prey {
                    Some(p)
                        if zombie.leap_wait <= 0.0
                            && p.with_y(at.y).distance(at) > LEAP_RANGE.0 =>
                    {
                        Leap::Crouch { t: 0.0, target: p }
                    }
                    // Walking, like any other zombie (`zombies::steer`).
                    _ => continue,
                }
            }
            Leap::Crouch { t, target } => {
                if status.frozen() {
                    continue;
                }
                flags = Pose::CROUCH;
                // Tracks its target until it goes, so a late sidestep works.
                let target = prey.unwrap_or(target);
                if let Some(yaw) = face(target) {
                    zombie.yaw = yaw;
                }
                let t = t + dt;
                if t < LEAP_TELL {
                    Leap::Crouch { t, target }
                } else {
                    let flat = Vec2::new(target.x - at.x, target.z - at.z);
                    let reach = flat.clamp_length_max(LEAP_RANGE.1);
                    let to = Vec3::new(at.x + reach.x, target.y, at.z + reach.y);
                    let secs = 0.2 + reach.length() / LEAP_SPEED;
                    Leap::Air {
                        from: at,
                        to,
                        t: 0.0,
                        secs,
                    }
                }
            }
            Leap::Air { from, to, t, secs } => {
                // Mid-air it can't be frozen or stopped, only shot down.
                flags = Pose::AIR | Pose::REACH;
                let t = t + dt;
                let s = (t / secs).min(1.0);
                next = from.lerp(to, s) + Vec3::Y * LEAP_HEIGHT * (PI * s).sin();
                if s < 1.0 {
                    Leap::Air { from, to, t, secs }
                } else {
                    next.y = ground(&spatial, &walls, next).unwrap_or(to.y);
                    zombie.slam = Some(next);
                    flash.write(to_all(Flash::Dust { at: next }));
                    Leap::Land { t: 0.0 }
                }
            }
            Leap::Land { t } => {
                let t = t + dt;
                if t < LEAP_RECOVER {
                    Leap::Land { t }
                } else {
                    zombie.leap_wait = LEAP_WAIT;
                    Leap::Ready
                }
            }
        };
        let new = Pose {
            pos: next,
            yaw: zombie.yaw,
            pitch: 0.0,
            vel: (next - at) / dt.max(1e-4),
            flags,
        };
        if *pose != new {
            *pose = new;
        }
    }
}

/// A bloater that dies, shot or by reaching someone, bursts: hurting the
/// players and the zombies near it, and leaving gas.
#[allow(clippy::type_complexity)]
pub fn bursts(
    // `Hits` sends `Died` too, so the two take turns.
    mut io: ParamSet<(MessageReader<Died>, Hits)>,
    bloaters: Query<(&Zombie, &Pose)>,
    players: Query<(Entity, &Pose), (With<Player>, Without<Fallen>, Without<Out>)>,
    zombies: Query<(Entity, &Pose), (With<Zombie>, Without<Dead>)>,
    is_player: Query<(), With<Player>>,
    spatial: SpatialQuery,
    mut flash: MessageWriter<ToClients<Flash>>,
    mut commands: Commands,
) {
    let deaths: Vec<Died> = io.p0().read().cloned().collect();
    let mut hits = io.p1();
    for died in deaths {
        let Ok((zombie, pose)) = bloaters.get(died.victim) else {
            continue;
        };
        if zombie.kind != BLOATER {
            continue;
        }
        let at = pose.pos;
        // Zombies killed by the blast count for whoever popped it.
        let killer = if is_player.contains(died.killer) {
            died.killer
        } else {
            died.victim
        };
        let damage = kind(BLOATER).damage;
        let near_players: Vec<(Entity, Vec3)> = players.iter().map(|(e, p)| (e, p.pos)).collect();
        let near_zombies: Vec<(Entity, Vec3)> = zombies
            .iter()
            .filter(|(e, _)| *e != died.victim)
            .map(|(e, p)| (e, p.pos))
            .collect();
        for (near, attacker) in [(near_players, died.victim), (near_zombies, killer)] {
            blast(
                &spatial,
                &mut hits,
                &near,
                Blast {
                    at,
                    attacker,
                    damage,
                    radius: GAS_RADIUS,
                    weapon: WeaponKind::Grenade,
                    tag: SECONDARY,
                },
            );
        }
        commands.spawn(Gas {
            at,
            left: GAS_SECS,
            tick: 0.5,
            bloater: died.victim,
            killer,
        });
        flash.write(to_all(Flash::Gas {
            at,
            radius: GAS_RADIUS,
            secs: GAS_SECS,
        }));
    }
}

/// Gas hurts whoever stands in it, every half second, until it clears.
#[allow(clippy::type_complexity)]
pub fn gas(
    time: Res<Time>,
    mut clouds: Query<(Entity, &mut Gas)>,
    players: Query<(Entity, &Pose), (With<Player>, Without<Fallen>, Without<Out>)>,
    zombies: Query<(Entity, &Pose), (With<Zombie>, Without<Dead>)>,
    mut hits: Hits,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, mut cloud) in &mut clouds {
        cloud.left -= dt;
        cloud.tick -= dt;
        if cloud.left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        if cloud.tick > 0.0 {
            continue;
        }
        cloud.tick = 0.5;
        let inside = |p: &Pose| p.pos.distance(cloud.at) < GAS_RADIUS;
        let victims: Vec<(Entity, Vec3, Entity)> = players
            .iter()
            .filter(|(_, p)| inside(p))
            .map(|(e, p)| (e, p.pos, cloud.bloater))
            .chain(
                zombies
                    .iter()
                    .filter(|(_, p)| inside(p))
                    .map(|(e, p)| (e, p.pos, cloud.killer)),
            )
            .collect();
        for (target, pos, by) in victims {
            if let Some(hurt) = hits.hurt(by, target, GAS_TICK, false, WeaponKind::Grenade) {
                hits.wounded(target, &hurt, Vec3::Y, pos, WeaponKind::Grenade);
            }
        }
    }
}
