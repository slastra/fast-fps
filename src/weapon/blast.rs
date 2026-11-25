//! Grenades as everyone sees them: the grenade in flight, its bounces, and
//! the explosion (flash, light, smoke, sparks, a scorch mark, the bang and a
//! shake of the view). The server decides where and when; this only draws it.

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{
    arsenal::Arsenal,
    fx::{self, Decals, FxAssets, FxRng},
};
use crate::{
    lowpoly,
    net::{Exploded, Projectile},
    physics::Layer,
    render::{WorldCamera, no_shadow},
    sound::{Sounds, play},
    weapon::{GRENADE, WeaponKind},
};

/// How many explosions this machine has seen and where the last one was
/// (the `explosions`, `blast_x` and `blast_z` probe values).
#[derive(Resource, Default)]
pub struct Explosions {
    pub count: u32,
    pub last: Vec3,
}

#[derive(Resource)]
struct BlastAssets {
    grenade: Vec<(Mesh3d, Transform)>,
    grenade_mat: MeshMaterial3d<StandardMaterial>,
    flash: Handle<Mesh>,
    flash_mat: Handle<StandardMaterial>,
    puff: Handle<Mesh>,
    puff_mat: Handle<StandardMaterial>,
    /// See-through gray for gun smoke, so a shot doesn't blind the shooter.
    haze_mat: Handle<StandardMaterial>,
}

/// Motion of a grenade as last seen, to hear it bounce.
#[derive(Component)]
struct Seen {
    at: Vec3,
    vel: Vec3,
}

/// A part of an explosion that grows and then fades: seconds lived, seconds
/// it lasts, and how it moves.
#[derive(Component)]
struct Puff {
    age: f32,
    life: f32,
    size: f32,
    rise: Vec3,
}

/// Black-powder smoke from a flintlock's muzzle, pointing along `dir`. Sent
/// by your own shot and by other players' (see `fx::remote_shots`).
#[derive(Message, Clone, Copy)]
pub struct Smoke {
    pub at: Vec3,
    pub dir: Vec3,
}

/// The explosion's light, dimming fast.
#[derive(Component)]
struct BlastLight(f32);

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Explosions>()
        .add_message::<Smoke>()
        .add_systems(Startup, make_assets)
        .add_systems(
            Update,
            (
                dress_grenades,
                follow_grenades,
                explosions,
                muzzle_smoke,
                age_puffs,
            ),
        );
}

fn make_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = StdRng::seed_from_u64(77);
    let model = super::models::model(WeaponKind::Grenade);
    let grenade = super::models::build(&model, &mut meshes, &mut rng)
        .into_iter()
        .map(|(mesh, transform, _)| (mesh, transform))
        .collect();
    let puff = lowpoly::facet(
        Sphere::new(1.0).mesh().ico(1).unwrap(),
        Color::srgb(0.55, 0.53, 0.5),
        0.12,
        &mut rng,
    );
    commands.insert_resource(BlastAssets {
        grenade,
        grenade_mat: MeshMaterial3d(materials.add(StandardMaterial {
            perceptual_roughness: 0.6,
            ..default()
        })),
        flash: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()),
        flash_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.45),
            emissive: LinearRgba::rgb(80.0, 40.0, 10.0),
            unlit: true,
            ..default()
        }),
        puff: meshes.add(puff),
        puff_mat: materials.add(StandardMaterial {
            perceptual_roughness: 1.0,
            ..default()
        }),
        haze_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(0.85, 0.84, 0.8, 0.42),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            ..default()
        }),
    });
}

/// Give every new grenade its model.
fn dress_grenades(
    added: Query<(Entity, &Projectile, &Transform), Added<Projectile>>,
    assets: Res<BlastAssets>,
    mut commands: Commands,
) {
    for (entity, projectile, transform) in &added {
        let at = if transform.translation == Vec3::ZERO {
            projectile.pos
        } else {
            transform.translation
        };
        commands
            .entity(entity)
            .insert((
                Seen {
                    at,
                    vel: Vec3::ZERO,
                },
                Transform::from_translation(at),
            ))
            .with_children(|grenade| {
                for (mesh, part) in &assets.grenade {
                    grenade.spawn((mesh.clone(), assets.grenade_mat.clone(), *part));
                }
            });
    }
}

/// On a client, move each grenade to where the server says it is. Where the
/// grenade is simulated (offline, hosting), physics moves it already. Either
/// way, listen for bounces.
fn follow_grenades(
    time: Res<Time>,
    mut grenades: Query<(&Projectile, &mut Transform, &mut Seen, Has<RigidBody>)>,
    listener: Single<&GlobalTransform, With<WorldCamera>>,
    sounds: Res<Sounds>,
    mut commands: Commands,
) {
    let dt = time.delta_secs().max(1e-4);
    for (projectile, mut transform, mut seen, simulated) in &mut grenades {
        if !simulated {
            transform
                .translation
                .smooth_nudge(&projectile.pos, 30.0, dt);
            transform.rotate_local_x(8.0 * dt);
        }
        let vel = (transform.translation - seen.at) / dt;
        // Falling, then suddenly not: it hit something.
        let bounced =
            seen.vel.y < -1.5 && vel.y > seen.vel.y * 0.3 && (vel - seen.vel).length() > 2.5;
        if bounced {
            let distance = listener.translation().distance(transform.translation);
            let volume = (1.0 - distance / 40.0).clamp(0.0, 1.0) * 0.5;
            if volume > 0.02 {
                play(&mut commands, &sounds.bounce, volume);
            }
        }
        seen.at = transform.translation;
        seen.vel = vel;
    }
}

#[allow(clippy::too_many_arguments)]
fn explosions(
    mut blasts: MessageReader<Exploded>,
    mut count: ResMut<Explosions>,
    assets: Res<BlastAssets>,
    fx: Res<FxAssets>,
    mut fx_rng: ResMut<FxRng>,
    mut decals: ResMut<Decals>,
    spatial: SpatialQuery,
    listener: Single<&GlobalTransform, With<WorldCamera>>,
    mut arsenal: ResMut<Arsenal>,
    sounds: Res<Sounds>,
    mut commands: Commands,
) {
    for blast in blasts.read() {
        count.count += 1;
        count.last = blast.pos;
        let at = blast.pos;
        let rng = &mut fx_rng.0;

        // The fireball: a bright ball that swells and is gone in a moment.
        commands.spawn((
            Mesh3d(assets.flash.clone()),
            MeshMaterial3d(assets.flash_mat.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(0.2)),
            Puff {
                age: 0.0,
                life: 0.14,
                size: 1.2,
                rise: Vec3::ZERO,
            },
            no_shadow(),
        ));
        // A brief light with no shadow maps: a shadowed point light would
        // add six render views (docs/perf.md).
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.65, 0.3),
                intensity: 3_000_000.0,
                range: 18.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(at + Vec3::Y * 0.5),
            BlastLight(0.25),
        ));
        // Smoke: small puffs that roll outward and up, then shrink away.
        for i in 0..16 {
            let angle = i as f32 / 16.0 * TAU + rng.random_range(-0.3..0.3);
            let out = Vec3::new(angle.cos(), 0.0, angle.sin());
            let high = rng.random_range(0.0..1.0_f32);
            commands.spawn((
                Mesh3d(assets.puff.clone()),
                MeshMaterial3d(assets.puff_mat.clone()),
                Transform::from_translation(at + out * 0.25 + Vec3::Y * (0.15 + high * 0.4))
                    .with_rotation(Quat::from_rotation_y(angle))
                    .with_scale(Vec3::splat(0.05)),
                Puff {
                    age: 0.0,
                    life: rng.random_range(0.9..1.6),
                    size: rng.random_range(0.35..0.7),
                    rise: out * rng.random_range(1.2..2.6) + Vec3::Y * (0.6 + high * 1.8),
                },
                no_shadow(),
            ));
        }
        // Sparks, and a scorch mark on whatever it went off on.
        for _ in 0..3 {
            fx::sparks(&mut commands, &fx, at, Vec3::Y, rng);
        }
        let below = Dir3::NEG_Y;
        if let Some(hit) = spatial.cast_ray(
            at + Vec3::Y * 0.2,
            below,
            1.5,
            true,
            &SpatialQueryFilter::from_mask(Layer::World),
        ) {
            let ground = at + Vec3::Y * 0.2 + below * hit.distance;
            fx::scorch(&mut commands, &fx, &mut decals, ground, hit.normal, rng);
        }

        // The bang carries far; the shake is for those close by.
        let distance = listener.translation().distance(at);
        let volume = (1.0 - distance / 140.0).clamp(0.08, 1.0);
        play(&mut commands, &sounds.explode, volume);
        let shake = (1.0 - distance / (GRENADE.radius * 3.0)).clamp(0.0, 1.0);
        if shake > 0.0 {
            arsenal.kick_punch += Vec2::new(rng.random_range(-1.0..1.0), 1.0) * 4.0 * shake;
        }
    }
}

/// A thick cloud that bursts out of the barrel, hangs, and drifts up.
/// Unlit by any light of its own and casting no shadow, so it costs little.
fn muzzle_smoke(
    mut smokes: MessageReader<Smoke>,
    assets: Res<BlastAssets>,
    mut fx_rng: ResMut<FxRng>,
    mut commands: Commands,
) {
    for smoke in smokes.read() {
        let rng = &mut fx_rng.0;
        let dir = smoke.dir.normalize_or(Vec3::NEG_Z);
        for i in 0..12 {
            let along = i as f32 / 12.0;
            let jitter = Vec3::new(
                rng.random_range(-1.0..1.0),
                rng.random_range(-0.5..1.0),
                rng.random_range(-1.0..1.0),
            );
            commands.spawn((
                Mesh3d(assets.puff.clone()),
                MeshMaterial3d(assets.haze_mat.clone()),
                Transform::from_translation(smoke.at)
                    .with_rotation(Quat::from_rotation_y(rng.random_range(0.0..TAU)))
                    .with_scale(Vec3::splat(0.02)),
                Puff {
                    age: 0.0,
                    life: rng.random_range(1.6..2.6),
                    size: 0.08 + along * 0.24 + rng.random_range(0.0..0.06),
                    // Faster puffs end up farther out, so the cloud stretches
                    // along the shot and hangs a few meters ahead.
                    rise: dir * (1.0 + along * 3.5) + jitter * 0.3 + Vec3::Y * 0.3,
                },
                no_shadow(),
            ));
        }
    }
}

fn age_puffs(
    time: Res<Time>,
    mut commands: Commands,
    mut puffs: Query<(Entity, &mut Puff, &mut Transform), Without<BlastLight>>,
    mut lights: Query<(Entity, &mut BlastLight, &mut PointLight)>,
) {
    let dt = time.delta_secs();
    for (entity, mut puff, mut transform) in &mut puffs {
        puff.age += dt;
        if puff.age >= puff.life {
            commands.entity(entity).despawn();
            continue;
        }
        let t = puff.age / puff.life;
        // Swell quickly, then shrink away to nothing.
        let grow = (t * 6.0).min(1.0);
        let fade = 1.0 - ((t - 0.4) / 0.6).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(puff.size * grow * fade.sqrt() + 0.01);
        transform.translation += puff.rise * dt * (1.0 - t);
    }
    for (entity, mut light, mut point) in &mut lights {
        light.0 -= dt;
        point.intensity *= 0.85;
        if light.0 <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}
