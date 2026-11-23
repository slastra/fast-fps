//! Lamp posts, the campfire, and the small moving lights that make the
//! place feel alive: flicker, embers and fireflies.

use std::f32::consts::TAU;

use bevy::{light::NotShadowCaster, prelude::*};
use rand::Rng;

use crate::{
    lowpoly,
    render::WorldCamera,
    sky::{Sky, SkyKind},
    world::builder::{Builder, IRON, ROCK, STONE_DARK, WOOD},
};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<LampShadows>()
        .add_systems(Update, (flicker, drift, lamp_shadow_budget));
}

/// How many lamps cast shadow maps: the ones nearest the camera. Each costs a
/// six-sided shadow render of everything around it, every frame, and a
/// distant lamp's shadows can't be seen anyway. Set with `r_lamp_shadows`.
#[derive(Resource)]
pub struct LampShadows(pub usize);

impl Default for LampShadows {
    fn default() -> Self {
        Self(2)
    }
}

/// A lamp post's light (not the campfire, which always casts shadows).
#[derive(Component)]
pub struct LampLight;

/// The campfire's light.
#[derive(Component)]
pub struct CampfireLight;

/// A light that wobbles like fire.
#[derive(Component)]
pub struct Flicker {
    base: f32,
    seed: f32,
}

/// Flame meshes that wobble along with the light.
#[derive(Component)]
pub struct FlameMesh(f32);

/// A firefly wandering around `home`.
#[derive(Component)]
struct Firefly {
    home: Vec3,
    phase: f32,
}

/// A spark rising from the fire at `origin`.
#[derive(Component)]
struct Ember {
    origin: Vec3,
    phase: f32,
}

pub fn lamp(b: &mut Builder, at: Vec3, lantern: Handle<StandardMaterial>) {
    let post_h = 3.4;
    b.block(
        at + Vec3::Y * post_h / 2.0,
        Vec3::new(0.2, post_h, 0.2),
        Quat::IDENTITY,
        IRON,
        0.0,
    );
    b.trim(at + Vec3::Y * 0.18, Vec3::new(0.5, 0.36, 0.5), STONE_DARK);
    // The lantern: a plate, the glowing glass, and a pointed cap.
    let top = at + Vec3::Y * post_h;
    b.trim(top + Vec3::Y * 0.02, Vec3::new(0.46, 0.06, 0.46), IRON);
    let glass = b.meshes.add(Cuboid::new(0.3, 0.38, 0.3));
    b.commands.spawn((
        Mesh3d(glass),
        MeshMaterial3d(lantern),
        Transform::from_translation(top + Vec3::Y * 0.24),
        NotShadowCaster,
    ));
    let cap = Cone {
        radius: 0.36,
        height: 0.26,
    }
    .mesh()
    .resolution(4)
    .build()
    .rotated_by(Quat::from_rotation_y(TAU / 8.0));
    let cap = lowpoly::facet(cap, IRON, 0.1, &mut b.paint);
    b.mesh_without_shadow(cap, Transform::from_translation(top + Vec3::Y * 0.56));

    b.commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.68, 0.38),
            intensity: 14_000.0,
            range: 20.0,
            radius: 0.15,
            // Decided each frame by `lamp_shadow_budget`.
            shadow_maps_enabled: false,
            // Screen-space contact shadows cost a ray march per pixel for every
            // light that has them. A lamp's are too faint to see, so only the
            // campfire and the sun or moon keep them.
            contact_shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(top + Vec3::Y * -0.25),
        LampLight,
        Flicker {
            base: 14_000.0,
            seed: at.x * 0.37 + at.z * 0.71,
        },
    ));
}

pub fn campfire(b: &mut Builder, at: Vec3, flame: Handle<StandardMaterial>) {
    for i in 0..4 {
        let yaw = i as f32 / 4.0 * TAU + 0.3;
        let mesh = Cylinder::new(0.11, 1.2).mesh().resolution(6).build();
        let mesh = lowpoly::facet(mesh, WOOD, 0.15, &mut b.paint);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(1.25);
        b.mesh(
            mesh,
            Transform::from_translation(at + Vec3::Y * 0.25).with_rotation(rotation),
        );
    }
    for i in 0..9 {
        let angle = i as f32 / 9.0 * TAU;
        let seed = b.paint.random::<u32>();
        let mesh = lowpoly::facet(
            lowpoly::rock(Vec3::new(0.24, 0.17, 0.2), seed),
            ROCK,
            0.15,
            &mut b.paint,
        );
        b.mesh(
            mesh,
            Transform::from_translation(at + Vec3::new(angle.cos(), 0.06, angle.sin()) * 0.8),
        );
    }
    for (i, (radius, height)) in [(0.32, 0.9), (0.2, 1.25), (0.24, 0.7)]
        .into_iter()
        .enumerate()
    {
        let mesh = b
            .meshes
            .add(Cone { radius, height }.mesh().resolution(5).build());
        let offset = Vec3::new(
            (i as f32 - 1.0) * 0.12,
            height / 2.0 + 0.1,
            (i as f32 * 2.1).sin() * 0.1,
        );
        b.commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(flame.clone()),
            Transform::from_translation(at + offset),
            FlameMesh(i as f32 * 1.7),
            NotShadowCaster,
        ));
    }
    let spark = b.meshes.add(Cuboid::from_length(0.035));
    for i in 0..12 {
        b.commands.spawn((
            Mesh3d(spark.clone()),
            MeshMaterial3d(flame.clone()),
            Transform::from_translation(at),
            Ember {
                origin: at + Vec3::Y * 0.5,
                phase: i as f32 * 0.83,
            },
            NotShadowCaster,
        ));
    }
    b.commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.5, 0.18),
            intensity: 40_000.0,
            range: 24.0,
            radius: 0.3,
            shadow_maps_enabled: true,
            contact_shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(at + Vec3::Y * 0.9),
        CampfireLight,
        Flicker {
            base: 40_000.0,
            seed: 3.3,
        },
    ));
}

/// Scatter fireflies around the given spots.
pub fn fireflies(b: &mut Builder, glow: Handle<StandardMaterial>, spots: &[Vec3]) {
    let mesh = b.meshes.add(Cuboid::from_length(0.03));
    for (i, spot) in spots.iter().enumerate() {
        b.commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(glow.clone()),
            Transform::from_translation(*spot),
            Firefly {
                home: *spot,
                phase: i as f32 * 1.37,
            },
            NotShadowCaster,
        ));
    }
}

fn flicker(
    time: Res<Time>,
    mut lights: Query<(&mut PointLight, &Flicker)>,
    mut flames: Query<(&mut Transform, &FlameMesh)>,
) {
    let t = time.elapsed_secs();
    for (mut light, f) in &mut lights {
        let s = f.seed;
        let wobble = (t * 7.3 + s).sin() * 0.06
            + (t * 13.1 + s * 2.0).sin() * 0.04
            + (t * 23.7 + s * 3.0).sin() * 0.03;
        light.intensity = f.base * (1.0 + wobble);
    }
    for (mut transform, FlameMesh(phase)) in &mut flames {
        let k = 1.0 + (t * 11.0 + phase).sin() * 0.12 + (t * 17.0 + phase * 2.0).sin() * 0.08;
        transform.scale = Vec3::new(1.0 / k.sqrt(), k, 1.0 / k.sqrt());
        transform.rotation = Quat::from_rotation_y(t * 1.5 + phase);
    }
}

fn drift(
    time: Res<Time>,
    sky: Res<Sky>,
    mut fireflies: Query<(&mut Transform, &Firefly), Without<Ember>>,
    mut embers: Query<(&mut Transform, &Ember), Without<Firefly>>,
) {
    let t = time.elapsed_secs();
    for (mut transform, fly) in &mut fireflies {
        let p = fly.phase;
        transform.translation = fly.home
            + Vec3::new(
                (t * 0.31 + p).sin() * 1.1,
                (t * 0.53 + p * 2.0).sin() * 0.35,
                (t * 0.27 + p * 3.0).cos() * 1.1,
            );
        // Blink slowly, each on its own beat.
        let glow = ((t * 1.3 + p * 5.0).sin() * 0.5 + 0.5).powi(3);
        // They only come out when it is dark.
        let out = matches!(sky.kind, SkyKind::Night | SkyKind::Dusk);
        transform.scale = if out {
            Vec3::splat(0.2 + glow * 1.3)
        } else {
            Vec3::ZERO
        };
    }
    for (mut transform, ember) in &mut embers {
        let life = ((t * 0.45 + ember.phase) % 1.0 + 1.0) % 1.0;
        let p = ember.phase;
        transform.translation = ember.origin
            + Vec3::new(
                (t * 1.7 + p * 4.0).sin() * 0.25 * life,
                life * 2.6,
                (t * 1.3 + p * 7.0).cos() * 0.25 * life,
            );
        transform.scale = Vec3::splat((1.0 - life).powi(2) * 1.4);
    }
}

/// Give shadow maps to the lamps nearest the camera, and take them from the rest.
fn lamp_shadow_budget(
    budget: Res<LampShadows>,
    camera: Query<&GlobalTransform, With<WorldCamera>>,
    mut lamps: Query<(Entity, &GlobalTransform, &mut PointLight), With<LampLight>>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    let eye = camera.translation();
    let mut by_distance: Vec<(Entity, f32)> = lamps
        .iter()
        .map(|(entity, at, _)| (entity, at.translation().distance_squared(eye)))
        .collect();
    by_distance.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (rank, (entity, _)) in by_distance.into_iter().enumerate() {
        if let Ok((_, _, mut light)) = lamps.get_mut(entity) {
            let want = rank < budget.0;
            if light.shadow_maps_enabled != want {
                light.shadow_maps_enabled = want;
            }
        }
    }
}
