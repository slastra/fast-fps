//! Grenades: a `Throw` from a client becomes a physics body here, bounces
//! around the level, and goes off when its fuse runs out. The blast hurts
//! everyone in range that a wall doesn't shield, thrower included.
//!
//! Each player gets one grenade per life. Clients see the grenade through its
//! replicated [`Projectile`] and the bang through [`Exploded`].

use avian3d::prelude::*;
use bevy::{platform::collections::HashMap, prelude::*};
use bevy_replicon::prelude::*;

use crate::{
    net::{Exploded, Health, Player, Pose, Projectile, Throw},
    physics::Layer,
    server::{
        combat::{Died, Hits},
        players::{PlayerClient, find_player},
    },
    weapon::{GRENADE, WeaponKind},
};

/// The grenade's collision radius.
pub const RADIUS: f32 = 0.06;
/// Height above a target's collider center that the blast aims at.
const CHEST: f32 = 0.2;

/// Server-only: the live grenade's thrower and the seconds until it goes off.
#[derive(Component)]
pub struct Fuse {
    pub thrower: Entity,
    pub left: f32,
}

/// Server-only: grenades thrown by each player since they last died.
#[derive(Resource, Default)]
pub struct Thrown(HashMap<Entity, u32>);

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Thrown>();
}

pub fn receive_throws(
    mut throws: MessageReader<FromClient<Throw>>,
    mut deaths: MessageReader<Died>,
    mut thrown: ResMut<Thrown>,
    clients: Query<(Entity, &PlayerClient)>,
    players: Query<(&Player, &Pose)>,
    spatial: SpatialQuery,
    mut commands: Commands,
) {
    for Died(entity) in deaths.read() {
        thrown.0.remove(entity);
    }
    for FromClient { client_id, message } in throws.read() {
        let Some(thrower) = find_player(*client_id, &clients) else {
            continue;
        };
        let Ok(dir) = Dir3::new(message.dir) else {
            continue;
        };
        if !message.origin.is_finite() {
            continue;
        }
        let count = thrown.0.entry(thrower).or_default();
        if *count >= WeaponKind::Grenade.def().mag {
            continue;
        }
        *count += 1;

        // Start a little in front of the eye, but never inside a wall.
        let ahead = 0.5;
        let walls = SpatialQueryFilter::from_mask(Layer::World);
        let start = match spatial.cast_ray(message.origin, dir, ahead, true, &walls) {
            Some(hit) => message.origin + dir * (hit.distance - RADIUS * 2.0).max(0.0),
            None => message.origin + dir * ahead,
        };
        let speed = GRENADE.speed(message.lob, message.strength);
        let lift = if message.lob { 2.0 } else { 1.5 };
        let Ok((player, pose)) = players.get(thrower) else {
            continue;
        };
        let (owner, carried) = (player.owner, pose.vel);
        commands.spawn((
            Name::new("Grenade"),
            Replicated,
            Projectile { owner, pos: start },
            Transform::from_translation(start),
            RigidBody::Dynamic,
            Collider::sphere(RADIUS),
            Mass(0.4),
            Restitution::new(GRENADE.bounce),
            (
                Friction::new(0.8),
                // A ball rolls forever; a grenade skids and stops. Damping
                // spin hard and speed a little lets it settle in a couple of
                // meters.
                AngularDamping(5.0),
                LinearDamping(0.4),
            ),
            LinearVelocity(dir * speed + Vec3::Y * lift + carried),
            AngularVelocity(Vec3::new(8.0, 3.0, 0.0)),
            SweptCcd::default(),
            CollisionLayers::new(Layer::Projectile, Layer::World),
            Fuse {
                thrower,
                left: GRENADE.fuse,
            },
        ));
    }
}

/// Keep the replicated position current, and set grenades off.
pub fn tick_grenades(
    time: Res<Time>,
    mut grenades: Query<(Entity, &Transform, &mut Projectile, &mut Fuse)>,
    victims: Query<(Entity, &Pose), With<Health>>,
    spatial: SpatialQuery,
    mut hits: Hits,
    mut exploded: MessageWriter<ToClients<Exploded>>,
    mut commands: Commands,
) {
    for (entity, transform, mut projectile, mut fuse) in &mut grenades {
        let pos = transform.translation;
        if projectile.pos.distance_squared(pos) > 1e-6 {
            projectile.pos = pos;
        }
        fuse.left -= time.delta_secs();
        if fuse.left > 0.0 {
            continue;
        }
        commands.entity(entity).despawn();
        exploded.write(ToClients {
            targets: SendTargets::All,
            message: Exploded { pos },
        });

        let blast = pos + Vec3::Y * 0.1;
        let walls = SpatialQueryFilter::from_mask(Layer::World);
        let mut any_hit = false;
        let mut any_kill = false;
        for (target, pose) in &victims {
            let chest = pose.pos + Vec3::Y * CHEST;
            let offset = chest - blast;
            let distance = offset.length();
            let damage = GRENADE.damage_at(distance);
            if damage <= 0.0 {
                continue;
            }
            // Walls shield you. The ray stops a little short of the target so
            // the floor it stands on doesn't count.
            if let Ok(dir) = Dir3::new(offset)
                && spatial
                    .cast_ray(blast, dir, (distance - 0.3).max(0.0), true, &walls)
                    .is_some()
            {
                continue;
            }
            let Some(hurt) = hits.hurt(fuse.thrower, target, damage, false, WeaponKind::Grenade)
            else {
                continue;
            };
            any_hit |= target != fuse.thrower;
            any_kill |= hurt.killed && target != fuse.thrower;
            // Bodies fly up and away from the blast.
            let push = offset.normalize_or_zero() + Vec3::Y * 0.6;
            hits.wounded(target, &hurt, push, chest, WeaponKind::Grenade);
        }
        if any_hit {
            hits.confirm(fuse.thrower, false, any_kill);
        }
    }
}
