//! Death ragdolls.
//!
//! When a player dies, each machine builds a physics copy of the body in the
//! exact pose it had: one rigid body per bone, joined the way the skeleton
//! says (ball joints at the spine, neck, shoulders and hips; hinges at the
//! elbows and knees), then shoves it the way the killing shot was going.
//!
//! Ragdolls are cosmetic. They aren't replicated, they collide only with the
//! world, and bullets and players pass through them.

use std::collections::VecDeque;

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::Rng;

use super::rig::{AvatarRig, BONE_COUNT, JointKind, SKELETON};
use crate::{
    Layer,
    render::no_shadow,
    weapon::{self, WeaponKind},
};

const MAX_RAGDOLLS: usize = 6;
const LIFE_SECS: f32 = 12.0;
const FADE_SECS: f32 = 0.6;

/// Live ragdolls, oldest first.
#[derive(Resource, Default)]
pub struct Ragdolls(VecDeque<Entity>);

#[derive(Component)]
pub struct Ragdoll {
    bodies: Vec<Entity>,
    joints: Vec<Entity>,
    age: f32,
    fading: bool,
}

pub struct Death {
    /// World position and rotation of every joint at the moment of death.
    pub frames: [(Vec3, Quat); BONE_COUNT],
    pub gun: (Vec3, Quat),
    /// The player's velocity when they died.
    pub velocity: Vec3,
    /// Direction the killing shot was traveling (zero for a suicide).
    pub dir: Vec3,
    pub point: Vec3,
    /// Speed the shot adds, in m/s.
    pub knock: f32,
    /// The gun the victim drops.
    pub held: WeaponKind,
}

/// How hard each weapon throws a body.
pub fn knock(weapon: WeaponKind) -> f32 {
    match weapon {
        WeaponKind::Glock => 2.0,
        WeaponKind::Mp5 => 2.5,
        WeaponKind::Ak => 3.5,
        WeaponKind::Shotgun => 7.5,
        WeaponKind::Sniper => 9.0,
    }
}

pub fn spawn(
    commands: &mut Commands,
    list: &mut Ragdolls,
    rig: &AvatarRig,
    meshes: &mut Assets<Mesh>,
    death: Death,
) {
    let mut rng = rand::rng();
    let frames = death.frames;

    let carried = death.velocity.clamp_length_max(9.0) * 0.9;
    // Tumble the way the shot pushes: the top of the body goes with it.
    let tumble = Vec3::Y.cross(death.dir) * death.knock * 0.35;
    let layers = CollisionLayers::new(Layer::Ragdoll, Layer::World);

    let mut bodies = [Entity::PLACEHOLDER; BONE_COUNT];
    let mut parts = Vec::new();
    let mut joints = Vec::new();
    for (i, def) in SKELETON.iter().enumerate() {
        let (pos, rot) = frames[i];
        let main = &def.boxes[0];
        let size = Vec3::from(main.size);
        let center = Vec3::from(main.center);

        // The part nearest the wound takes the full hit.
        let reach = (pos + rot * center).distance(death.point);
        let share = 0.45 + 0.55 * (-reach * reach / 0.12).exp();
        let jitter = Vec3::new(
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
        );
        let body = commands
            .spawn((
                Name::new(format!("Ragdoll {:?}", def.bone)),
                RigidBody::Dynamic,
                Transform::from_translation(pos).with_rotation(rot),
                Visibility::default(),
                Collider::compound(vec![(
                    center,
                    Quat::IDENTITY,
                    Collider::cuboid(size.x, size.y, size.z),
                )]),
                layers,
                LinearVelocity(carried + death.dir * death.knock * share + Vec3::Y * 1.2),
                AngularVelocity(tumble + jitter * 1.5),
                LinearDamping(0.1),
                AngularDamping(1.2),
                TransformInterpolation,
            ))
            .with_children(|body| {
                for part in rig.parts.iter().filter(|p| p.bone == def.bone) {
                    let mut e = body.spawn((
                        Mesh3d(part.mesh.clone()),
                        MeshMaterial3d(part.material.clone()),
                        Transform::from_translation(part.center),
                    ));
                    if part.glow {
                        e.insert(no_shadow());
                    }
                }
            })
            .id();
        bodies[i] = body;
        parts.push(body);

        let Some(parent) = def.parent else { continue };
        let parent = bodies[parent.index()];
        let anchor = Vec3::from(def.offset);
        let joint = match def.joint {
            JointKind::Root => continue,
            JointKind::Ball { swing, twist } => commands
                .spawn(
                    SphericalJoint::new(parent, body)
                        .with_local_anchor1(anchor)
                        .with_swing_limits(-swing, swing)
                        .with_twist_limits(-twist, twist),
                )
                .id(),
            JointKind::Hinge { min, max } => commands
                .spawn(
                    RevoluteJoint::new(parent, body)
                        .with_hinge_axis(Vec3::X)
                        .with_local_anchor1(anchor)
                        .with_angle_limits(min, max),
                )
                .id(),
        };
        joints.push(joint);
    }

    // The gun falls on its own.
    {
        let (gun_pos, gun_rot) = death.gun;
        let material = MeshMaterial3d(rig.material.clone());
        let spin = Vec3::new(
            rng.random_range(-6.0..6.0),
            rng.random_range(-6.0..6.0),
            rng.random_range(-6.0..6.0),
        );
        let held = death.held;
        let dropped = commands
            .spawn((
                Name::new("Dropped gun"),
                RigidBody::Dynamic,
                Transform::from_translation(gun_pos).with_rotation(gun_rot),
                Visibility::default(),
                Collider::compound(vec![(
                    Vec3::new(0.0, -0.02, -0.15),
                    Quat::IDENTITY,
                    Collider::cuboid(0.07, 0.18, 0.6),
                )]),
                layers,
                LinearVelocity(carried + death.dir * death.knock * 0.4 + Vec3::Y * 2.5),
                AngularVelocity(spin),
                AngularDamping(0.8),
                TransformInterpolation,
            ))
            .with_children(|body| {
                let mut seeded = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(7);
                weapon::spawn_gun(body, held, meshes, &material, &mut seeded);
            })
            .id();
        parts.push(dropped);
    }

    let ragdoll = commands
        .spawn(Ragdoll {
            bodies: parts,
            joints,
            age: 0.0,
            fading: false,
        })
        .id();
    list.0.push_back(ragdoll);
}

/// Shrink old ragdolls away, and the oldest early if there are too many.
pub fn age(
    time: Res<Time>,
    mut commands: Commands,
    mut list: ResMut<Ragdolls>,
    mut ragdolls: Query<(Entity, &mut Ragdoll)>,
    mut transforms: Query<&mut Transform>,
) {
    while list.0.len() > MAX_RAGDOLLS {
        if let Some(oldest) = list.0.pop_front()
            && let Ok((_, mut ragdoll)) = ragdolls.get_mut(oldest)
        {
            ragdoll.age = ragdoll.age.max(LIFE_SECS);
        }
    }
    for (entity, mut ragdoll) in &mut ragdolls {
        ragdoll.age += time.delta_secs();
        if ragdoll.age < LIFE_SECS {
            continue;
        }
        // Take the corpse out of the simulation before shrinking it: scaling
        // jointed bodies makes the solver blow up.
        if !ragdoll.fading {
            ragdoll.fading = true;
            for joint in ragdoll.joints.drain(..) {
                commands.entity(joint).try_despawn();
            }
            for body in &ragdoll.bodies {
                commands
                    .entity(*body)
                    .try_remove::<(RigidBody, Collider, TransformInterpolation)>();
            }
        }
        let scale = 1.0 - (ragdoll.age - LIFE_SECS) / FADE_SECS;
        if scale <= 0.02 {
            for body in &ragdoll.bodies {
                commands.entity(*body).try_despawn();
            }
            commands.entity(entity).despawn();
            list.0.retain(|e| *e != entity);
            continue;
        }
        for body in &ragdoll.bodies {
            if let Ok(mut transform) = transforms.get_mut(*body) {
                transform.scale = Vec3::splat(scale);
            }
        }
    }
}
