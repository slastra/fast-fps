//! Shot effects: tracers, sparks, bullet holes, and other players' shots.

use std::collections::VecDeque;

use bevy::prelude::*;
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::WeaponKind;
use crate::{
    avatar::rig::AvatarRig,
    net::{Player, ShotFx},
    render::{WorldCamera, no_shadow},
    sound::{Sounds, play},
};

const MAX_DECALS: usize = 300;
const DECAL_SECS: f32 = 25.0;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Decals>()
        .insert_resource(FxRng(StdRng::seed_from_u64(7)))
        .add_systems(Startup, make_fx_assets)
        .add_systems(
            Update,
            (remote_shots, fade_tracers, move_sparks, age_decals),
        );
}

#[derive(Resource)]
pub struct FxAssets {
    tracer: Handle<Mesh>,
    tracer_mat: Handle<StandardMaterial>,
    spark: Handle<Mesh>,
    spark_mat: Handle<StandardMaterial>,
    blood_mat: Handle<StandardMaterial>,
    decal: Handle<Mesh>,
    decal_mat: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub struct FxRng(pub StdRng);

#[derive(Resource, Default)]
pub struct Decals(VecDeque<Entity>);

#[derive(Component)]
struct Tracer {
    life: f32,
    total: f32,
    width: f32,
}

#[derive(Component)]
struct Spark {
    velocity: Vec3,
    life: f32,
}

/// A bullet hole; the number is how long it has left.
#[derive(Component)]
pub struct Decal(f32);

/// One-shot muzzle flash light for other players' shots.
#[derive(Component)]
struct FlashLight(f32);

fn make_fx_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(FxAssets {
        tracer: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        tracer_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.5),
            emissive: LinearRgba::rgb(40.0, 22.0, 8.0),
            unlit: true,
            ..default()
        }),
        spark: meshes.add(Cuboid::new(0.04, 0.04, 0.04)),
        spark_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.7, 0.3),
            emissive: LinearRgba::rgb(30.0, 14.0, 3.0),
            unlit: true,
            ..default()
        }),
        blood_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.05, 0.03),
            emissive: LinearRgba::rgb(6.0, 0.2, 0.1),
            ..default()
        }),
        decal: meshes.add(RegularPolygon::new(0.04, 6)),
        decal_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.02, 0.018, 0.02),
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            ..default()
        }),
    });
}

pub fn tracer(commands: &mut Commands, fx: &FxAssets, from: Vec3, to: Vec3, weapon: WeaponKind) {
    let length = from.distance(to);
    if length < 0.5 {
        return;
    }
    let (width, life) = match weapon {
        WeaponKind::Sniper => (0.05, 0.16),
        WeaponKind::Shotgun => (0.012, 0.05),
        _ => (0.025, 0.07),
    };
    commands.spawn((
        Mesh3d(fx.tracer.clone()),
        MeshMaterial3d(fx.tracer_mat.clone()),
        Transform::from_translation((from + to) / 2.0)
            .looking_at(to, Vec3::Y)
            .with_scale(Vec3::new(width, width, length)),
        Tracer {
            life,
            total: life,
            width,
        },
        no_shadow(),
    ));
}

/// Sparks or blood where a bullet landed, plus a bullet hole on walls.
pub fn impact(
    commands: &mut Commands,
    fx: &FxAssets,
    decals: &mut Decals,
    at: Vec3,
    normal: Vec3,
    flesh: bool,
    rng: &mut StdRng,
) {
    let normal = normal.try_normalize().unwrap_or(Vec3::Y);
    let count = if flesh { 8 } else { 5 };
    for _ in 0..count {
        let random = Vec3::new(
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
            rng.random_range(-1.0..1.0),
        );
        let velocity = (normal * 1.2 + random).normalize_or_zero() * rng.random_range(2.0..6.0);
        let material = if flesh {
            fx.blood_mat.clone()
        } else {
            fx.spark_mat.clone()
        };
        commands.spawn((
            Mesh3d(fx.spark.clone()),
            MeshMaterial3d(material),
            Transform::from_translation(at + normal * 0.03),
            Spark {
                velocity,
                life: rng.random_range(0.2..0.45),
            },
            no_shadow(),
        ));
    }
    if flesh {
        return;
    }
    let up = if normal.y.abs() > 0.95 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let spin = Quat::from_rotation_z(rng.random_range(0.0..std::f32::consts::TAU));
    let decal = commands
        .spawn((
            Mesh3d(fx.decal.clone()),
            MeshMaterial3d(fx.decal_mat.clone()),
            Transform::from_translation(at + normal * 0.004)
                .looking_to(-normal, up)
                .mul_transform(Transform::from_rotation(spin))
                .with_scale(Vec3::splat(rng.random_range(0.8..1.2))),
            Decal(DECAL_SECS),
            no_shadow(),
        ))
        .id();
    decals.0.push_back(decal);
    while decals.0.len() > MAX_DECALS {
        if let Some(old) = decals.0.pop_front() {
            commands.entity(old).try_despawn();
        }
    }
}

fn remote_shots(
    mut shots: MessageReader<ShotFx>,
    mut commands: Commands,
    fx: Res<FxAssets>,
    sounds: Res<Sounds>,
    mut rng: ResMut<FxRng>,
    mut decals: ResMut<Decals>,
    listener: Single<&GlobalTransform, With<WorldCamera>>,
    avatars: Query<(&Player, &AvatarRig)>,
    globals: Query<&GlobalTransform>,
) {
    // Shotguns send one message per pellet; flash and play sound once per blast.
    let mut blasts: Vec<Vec3> = Vec::new();
    for shot in shots.read() {
        // Start at the shooter's muzzle if we can see their body.
        let dir = (shot.end - shot.origin).normalize_or_zero();
        let start = avatars
            .iter()
            .find(|(player, _)| player.owner == shot.shooter)
            .and_then(|(_, rig)| globals.get(rig.gun).ok())
            .map(|gun| gun.transform_point(super::muzzle(shot.weapon)))
            .unwrap_or(shot.origin + dir * 0.7 - Vec3::Y * 0.15);
        tracer(&mut commands, &fx, start, shot.end, shot.weapon);
        if shot.normal != Vec3::ZERO {
            impact(
                &mut commands,
                &fx,
                &mut decals,
                shot.end,
                shot.normal,
                shot.flesh,
                &mut rng.0,
            );
        }
        if blasts.iter().any(|b| b.distance(shot.origin) < 0.01) {
            continue;
        }
        blasts.push(shot.origin);
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.65, 0.3),
                intensity: 50_000.0,
                range: 10.0,
                ..default()
            },
            Transform::from_translation(start),
            FlashLight(0.05),
        ));
        let distance = listener.translation().distance(shot.origin);
        let volume = (1.0 - distance / 90.0).clamp(0.05, 1.0) * 0.45;
        play(&mut commands, &sounds.shots[shot.weapon.index()], volume);
    }
}

fn fade_tracers(
    time: Res<Time>,
    mut commands: Commands,
    mut tracers: Query<(Entity, &mut Tracer, &mut Transform)>,
    mut flashes: Query<(Entity, &mut FlashLight, &mut PointLight)>,
) {
    let dt = time.delta_secs();
    for (entity, mut tracer, mut transform) in &mut tracers {
        tracer.life -= dt;
        if tracer.life <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let w = tracer.width * (tracer.life / tracer.total).clamp(0.1, 1.0);
        transform.scale.x = w;
        transform.scale.y = w;
    }
    for (entity, mut flash, mut light) in &mut flashes {
        flash.0 -= dt;
        light.intensity *= 0.6;
        if flash.0 <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn move_sparks(
    time: Res<Time>,
    mut commands: Commands,
    mut sparks: Query<(Entity, &mut Spark, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut spark, mut transform) in &mut sparks {
        spark.life -= dt;
        if spark.life <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        spark.velocity.y -= 12.0 * dt;
        transform.translation += spark.velocity * dt;
        transform.scale = Vec3::splat((spark.life / 0.3).clamp(0.2, 1.0));
    }
}

fn age_decals(
    time: Res<Time>,
    mut commands: Commands,
    mut list: ResMut<Decals>,
    mut decals: Query<(Entity, &mut Decal)>,
) {
    let dt = time.delta_secs();
    for (entity, mut decal) in &mut decals {
        decal.0 -= dt;
        if decal.0 <= 0.0 {
            commands.entity(entity).despawn();
            list.0.retain(|e| *e != entity);
        }
    }
}
