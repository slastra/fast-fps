//! The rifle: a low-poly view model, hit-scan firing, tracers and sparks.
//!
//! Firing is predicted locally (tracer, sound, kick) and the shot itself is
//! sent to the server, which does the authoritative ray cast.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{
    Layer, lowpoly,
    net::{Fire, HitConfirm, ShotFx},
    player::{GameInput, LocalPlayer},
    render::{WorldCamera, no_shadow},
    sound::{Sounds, play},
};

const MAG: u32 = 30;
const FIRE_INTERVAL: f32 = 0.1;
const RELOAD_SECS: f32 = 1.4;
const RANGE: f32 = 250.0;
const GUN_REST: Vec3 = Vec3::new(0.2, -0.19, -0.38);
const MUZZLE: Vec3 = Vec3::new(0.0, 0.035, -0.5);
/// The gun is modeled at full size, then shrunk toward the eye by this factor.
/// Scaling about the eye doesn't change how it looks, but it keeps the whole
/// gun inside the character's 0.4 m collision radius, so it never pokes
/// through walls and needs no separate view-model camera.
const VM_SCALE: f32 = 0.45;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Gun>()
        .init_resource::<HitMarker>()
        .init_resource::<ScriptedFire>()
        .add_systems(Startup, make_fx_assets)
        .add_systems(PostStartup, build_view_model)
        .add_systems(
            Update,
            (
                fire,
                reload,
                animate_gun,
                remote_shots,
                hit_confirms,
                fade_tracers,
                move_sparks,
            ),
        );
}

#[derive(Resource)]
pub struct Gun {
    pub ammo: u32,
    pub reloading: f32,
    cooldown: f32,
    kick: f32,
    flash: f32,
    bob: f32,
    last_pos: Option<Vec3>,
    rng: StdRng,
}

impl Default for Gun {
    fn default() -> Self {
        Self {
            ammo: MAG,
            reloading: 0.0,
            cooldown: 0.0,
            kick: 0.0,
            flash: 0.0,
            bob: 0.0,
            last_pos: None,
            rng: StdRng::seed_from_u64(1),
        }
    }
}

impl Gun {
    pub const MAG: u32 = MAG;
}

/// Shots queued by the `fire` console command (for scripted tests).
#[derive(Resource, Default)]
pub struct ScriptedFire(pub u32);

/// Set by a server hit confirmation; the HUD fades it out.
#[derive(Resource, Default)]
pub struct HitMarker {
    pub time: f32,
    pub kill: bool,
}

#[derive(Resource)]
struct FxAssets {
    tracer: Handle<Mesh>,
    tracer_mat: Handle<StandardMaterial>,
    spark: Handle<Mesh>,
    spark_mat: Handle<StandardMaterial>,
    blood_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct GunRoot;

#[derive(Component)]
struct MuzzleFlash;

#[derive(Component)]
struct MuzzleLight;

#[derive(Component)]
struct Tracer {
    life: f32,
    width: f32,
}

#[derive(Component)]
struct Spark {
    velocity: Vec3,
    life: f32,
}

/// One-shot flash for other players' shots.
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
    });
}

fn build_view_model(
    mut commands: Commands,
    camera: Single<Entity, With<WorldCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = StdRng::seed_from_u64(42);
    let material = MeshMaterial3d(materials.add(StandardMaterial {
        perceptual_roughness: 0.55,
        metallic: 0.2,
        ..default()
    }));
    let flash_mat = MeshMaterial3d(materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.7, 0.3),
        emissive: LinearRgba::rgb(60.0, 30.0, 8.0),
        unlit: true,
        ..default()
    }));
    let metal = Color::srgb(0.12, 0.12, 0.14);
    let worn = Color::srgb(0.2, 0.2, 0.22);
    let wood = Color::srgb(0.4, 0.22, 0.1);

    let mut boxed = |size: Vec3, color: Color| {
        Mesh3d(meshes.add(lowpoly::facet(
            Cuboid::from_size(size).mesh().build(),
            color,
            0.1,
            &mut rng,
        )))
    };
    let parts = [
        // receiver
        (
            boxed(Vec3::new(0.075, 0.1, 0.36), metal),
            Vec3::new(0.0, 0.0, -0.1),
            Quat::IDENTITY,
        ),
        // barrel shroud
        (
            boxed(Vec3::new(0.05, 0.05, 0.26), worn),
            Vec3::new(0.0, 0.02, -0.4),
            Quat::IDENTITY,
        ),
        // top rail + sight
        (
            boxed(Vec3::new(0.03, 0.03, 0.22), metal),
            Vec3::new(0.0, 0.065, -0.08),
            Quat::IDENTITY,
        ),
        (
            boxed(Vec3::new(0.045, 0.05, 0.03), metal),
            Vec3::new(0.0, 0.09, -0.02),
            Quat::IDENTITY,
        ),
        // magazine
        (
            boxed(Vec3::new(0.05, 0.16, 0.07), worn),
            Vec3::new(0.0, -0.11, -0.16),
            Quat::from_rotation_x(0.25),
        ),
        // grip
        (
            boxed(Vec3::new(0.045, 0.12, 0.06), wood),
            Vec3::new(0.0, -0.09, 0.03),
            Quat::from_rotation_x(-0.35),
        ),
        // stock
        (
            boxed(Vec3::new(0.06, 0.08, 0.2), wood),
            Vec3::new(0.0, -0.02, 0.17),
            Quat::IDENTITY,
        ),
        // forearm
        (
            boxed(Vec3::new(0.065, 0.06, 0.16), wood),
            Vec3::new(0.0, -0.035, -0.32),
            Quat::IDENTITY,
        ),
    ];
    let flash_mesh = Mesh3d(
        meshes.add(
            Cone {
                radius: 0.06,
                height: 0.22,
            }
            .mesh()
            .resolution(5)
            .build(),
        ),
    );

    commands.entity(*camera).with_children(|cam| {
        cam.spawn((
            GunRoot,
            Transform::from_translation(GUN_REST * VM_SCALE).with_scale(Vec3::splat(VM_SCALE)),
            Visibility::default(),
        ))
        .with_children(|gun| {
            for (mesh, at, rot) in parts {
                gun.spawn((
                    mesh,
                    material.clone(),
                    Transform::from_translation(at).with_rotation(rot),
                    no_shadow(),
                ));
            }
            gun.spawn((
                MuzzleFlash,
                flash_mesh,
                flash_mat,
                Transform::from_translation(MUZZLE + Vec3::Z * -0.1)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::Hidden,
                no_shadow(),
            ));
            gun.spawn((
                MuzzleLight,
                PointLight {
                    color: Color::srgb(1.0, 0.65, 0.3),
                    intensity: 0.0,
                    range: 12.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_translation(MUZZLE + Vec3::Z * -0.15),
            ));
        });
    });
}

fn fire(
    time: Res<Time>,
    input: Res<GameInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut gun: ResMut<Gun>,
    mut scripted: ResMut<ScriptedFire>,
    camera: Single<&GlobalTransform, With<WorldCamera>>,
    spatial: SpatialQuery,
    mut shots: MessageWriter<Fire>,
    mut commands: Commands,
    fx: Res<FxAssets>,
    sounds: Res<Sounds>,
) {
    let dt = time.delta_secs();
    gun.cooldown = (gun.cooldown - dt).max(0.0);
    gun.kick = (gun.kick - dt * 3.5).max(0.0);
    let trigger = scripted.0 > 0 || (input.enabled && mouse.pressed(MouseButton::Left));
    if !trigger || gun.reloading > 0.0 || gun.cooldown > 0.0 {
        return;
    }
    scripted.0 = scripted.0.saturating_sub(1);
    if gun.ammo == 0 {
        if mouse.just_pressed(MouseButton::Left) {
            play(&mut commands, &sounds.click, 0.5);
        }
        return;
    }
    gun.ammo -= 1;
    gun.cooldown = FIRE_INTERVAL;
    gun.flash = 0.05;

    let spread = 0.0035 + gun.kick * 0.018;
    let jitter = Vec2::new(
        gun.rng.random_range(-1.0..1.0),
        gun.rng.random_range(-1.0..1.0),
    ) * spread;
    gun.kick = (gun.kick + 0.35).min(1.0);

    let origin = camera.translation();
    let dir = (camera.forward().as_vec3()
        + camera.right().as_vec3() * jitter.x
        + camera.up().as_vec3() * jitter.y)
        .normalize();
    shots.write(Fire { origin, dir });

    // Local prediction: where the tracer ends and what it hit.
    let filter = SpatialQueryFilter::from_mask(Layer::World);
    let end = match Dir3::new(dir)
        .ok()
        .and_then(|d| spatial.cast_ray(origin, d, RANGE, true, &filter))
    {
        Some(hit) => {
            let end = origin + dir * hit.distance;
            sparks(&mut commands, &fx, end, hit.normal, false, &mut gun.rng);
            end
        }
        None => origin + dir * RANGE,
    };
    let muzzle = camera.transform_point((GUN_REST + MUZZLE) * VM_SCALE);
    tracer(&mut commands, &fx, muzzle, end);
    play(&mut commands, &sounds.shot, 0.55);
}

fn reload(
    time: Res<Time>,
    input: Res<GameInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut gun: ResMut<Gun>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    if gun.reloading > 0.0 {
        gun.reloading -= time.delta_secs();
        if gun.reloading <= 0.0 {
            gun.reloading = 0.0;
            gun.ammo = MAG;
            play(&mut commands, &sounds.click, 0.6);
        }
        return;
    }
    let wants = keys.just_pressed(KeyCode::KeyR)
        || (gun.ammo == 0 && mouse.just_pressed(MouseButton::Left));
    if input.enabled && wants && gun.ammo < MAG {
        gun.reloading = RELOAD_SECS;
        play(&mut commands, &sounds.reload, 0.6);
    }
}

fn animate_gun(
    time: Res<Time>,
    mut gun: ResMut<Gun>,
    player: Single<&Transform, With<LocalPlayer>>,
    mut root: Single<&mut Transform, (With<GunRoot>, Without<LocalPlayer>)>,
    mut flash: Single<&mut Visibility, With<MuzzleFlash>>,
    mut light: Single<&mut PointLight, With<MuzzleLight>>,
) {
    let dt = time.delta_secs().max(1e-4);
    let pos = player.translation;
    let speed = gun
        .last_pos
        .map(|last| (pos - last).with_y(0.0).length() / dt)
        .unwrap_or(0.0);
    gun.last_pos = Some(pos);
    let moving = (speed / 6.0).clamp(0.0, 1.0);
    gun.bob += dt * (4.0 + speed * 1.2);

    let bob = Vec3::new(
        gun.bob.sin() * 0.012,
        (gun.bob * 2.0).sin().abs() * -0.01,
        0.0,
    ) * moving;
    let kick = gun.kick;
    let reload_t = if gun.reloading > 0.0 {
        let t = 1.0 - gun.reloading / RELOAD_SECS;
        (t * std::f32::consts::PI).sin()
    } else {
        0.0
    };
    let target = (GUN_REST + bob + Vec3::new(0.0, -0.12 * reload_t, 0.045 * kick)) * VM_SCALE;
    root.translation.smooth_nudge(&target, 30.0, dt);
    let rotation =
        Quat::from_rotation_x(0.06 * kick - 0.7 * reload_t) * Quat::from_rotation_z(0.5 * reload_t);
    root.rotation.smooth_nudge(&rotation, 25.0, dt);

    gun.flash -= dt;
    let lit = gun.flash > 0.0;
    **flash = if lit {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    light.intensity = if lit { 60_000.0 } else { 0.0 };
}

fn tracer(commands: &mut Commands, fx: &FxAssets, from: Vec3, to: Vec3) {
    let length = from.distance(to);
    if length < 0.5 {
        return;
    }
    let mid = (from + to) / 2.0;
    commands.spawn((
        Mesh3d(fx.tracer.clone()),
        MeshMaterial3d(fx.tracer_mat.clone()),
        Transform::from_translation(mid)
            .looking_at(to, Vec3::Y)
            .with_scale(Vec3::new(0.025, 0.025, length)),
        Tracer {
            life: 0.07,
            width: 0.025,
        },
        no_shadow(),
    ));
}

fn sparks(
    commands: &mut Commands,
    fx: &FxAssets,
    at: Vec3,
    normal: Vec3,
    flesh: bool,
    rng: &mut StdRng,
) {
    let normal = normal.try_normalize().unwrap_or(Vec3::Y);
    let count = if flesh { 10 } else { 7 };
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
}

fn remote_shots(
    mut shots: MessageReader<ShotFx>,
    mut commands: Commands,
    fx: Res<FxAssets>,
    sounds: Res<Sounds>,
    mut gun: ResMut<Gun>,
    listener: Single<&GlobalTransform, With<WorldCamera>>,
) {
    for shot in shots.read() {
        let dir = (shot.end - shot.origin).normalize_or_zero();
        let start = shot.origin + dir * 0.7 - Vec3::Y * 0.15;
        tracer(&mut commands, &fx, start, shot.end);
        if shot.normal != Vec3::ZERO {
            sparks(
                &mut commands,
                &fx,
                shot.end,
                shot.normal,
                shot.flesh,
                &mut gun.rng,
            );
        }
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
        play(&mut commands, &sounds.shot, volume);
    }
}

fn hit_confirms(
    mut confirms: MessageReader<HitConfirm>,
    mut marker: ResMut<HitMarker>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    for confirm in confirms.read() {
        marker.time = if confirm.kill { 0.45 } else { 0.18 };
        marker.kill = confirm.kill;
        let sound = if confirm.kill {
            &sounds.kill
        } else {
            &sounds.hit
        };
        play(&mut commands, sound, if confirm.head { 0.7 } else { 0.5 });
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
        let w = tracer.width * (tracer.life / 0.07).clamp(0.1, 1.0);
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
