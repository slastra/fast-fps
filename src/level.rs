//! The arena: a walled courtyard at night with a ruined keep in the middle,
//! raised corner platforms, crates, rocks and pines, lit by lamps and a fire.
//!
//! Everything is generated from a fixed seed so every peer builds the exact
//! same colliders, which keeps server-side hit scans honest.

use std::f32::consts::{FRAC_PI_2, TAU};

use avian3d::prelude::*;
use bevy::{light::CascadeShadowConfigBuilder, prelude::*};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{Layer, lowpoly};

/// Half the width of the playable square.
pub const ARENA: f32 = 36.0;

/// Collider-center heights for respawning.
pub const SPAWNS: [Vec3; 8] = [
    Vec3::new(-15.0, 1.2, -19.0),
    Vec3::new(15.0, 1.2, -19.0),
    Vec3::new(-15.0, 1.2, 19.0),
    Vec3::new(15.0, 1.2, 19.0),
    Vec3::new(0.0, 1.2, -27.0),
    Vec3::new(0.0, 1.2, 27.0),
    Vec3::new(-27.0, 1.2, 0.0),
    Vec3::new(12.0, 1.2, 12.0),
];

/// Practice dummies along the east wall: (anchor, sway).
pub const DUMMIES: [(Vec3, f32); 5] = [
    (Vec3::new(30.0, 0.95, -10.0), 0.0),
    (Vec3::new(30.0, 0.95, -5.0), 2.5),
    (Vec3::new(30.0, 0.95, 0.0), 0.0),
    (Vec3::new(30.0, 0.95, 5.0), 2.5),
    (Vec3::new(30.0, 0.95, 10.0), 0.0),
];

const STONE: Color = Color::srgb(0.33, 0.33, 0.37);
const STONE_DARK: Color = Color::srgb(0.24, 0.24, 0.28);
const WOOD: Color = Color::srgb(0.42, 0.26, 0.14);
const CRATE: Color = Color::srgb(0.55, 0.37, 0.19);
const IRON: Color = Color::srgb(0.08, 0.08, 0.09);
const ROCK: Color = Color::srgb(0.29, 0.29, 0.31);

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_level)
        .add_systems(Update, flicker);
}

/// A light that wobbles like fire.
#[derive(Component)]
struct Flicker {
    base: f32,
    seed: f32,
}

/// Flame meshes that wobble along with the light.
#[derive(Component)]
struct FlameMesh(f32);

struct Builder<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    rng: StdRng,
}

impl Builder<'_, '_, '_> {
    fn mesh(&mut self, mesh: Mesh, transform: Transform) -> EntityCommands<'_> {
        let handle = self.meshes.add(mesh);
        self.commands.spawn((
            Mesh3d(handle),
            MeshMaterial3d(self.material.clone()),
            transform,
        ))
    }

    fn solid(&mut self, mesh: Mesh, transform: Transform, collider: Collider) {
        self.mesh(mesh, transform).insert((
            RigidBody::Static,
            collider,
            CollisionLayers::new(Layer::World, LayerMask::ALL),
        ));
    }

    /// An axis-aligned (optionally rotated) box with a matching collider.
    fn block(&mut self, center: Vec3, size: Vec3, rotation: Quat, color: Color, jitter: f32) {
        let mesh = lowpoly::facet(
            Cuboid::from_size(size).mesh().build(),
            color,
            jitter,
            &mut self.rng,
        );
        self.solid(
            mesh,
            Transform::from_translation(center).with_rotation(rotation),
            Collider::cuboid(size.x, size.y, size.z),
        );
    }

    /// A ramp whose top surface runs from `low` to `high` (both on the surface).
    fn ramp(&mut self, low: Vec3, high: Vec3, width: f32) {
        let run = high - low;
        let horizontal = Vec2::new(run.x, run.z);
        let length = run.length();
        let thickness = 0.4;
        let yaw = (-horizontal.y).atan2(horizontal.x);
        let pitch = run.y.atan2(horizontal.length());
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(pitch);
        let center = (low + high) / 2.0 - rotation * Vec3::Y * (thickness / 2.0);
        self.block(
            center,
            Vec3::new(length, thickness, width),
            rotation,
            WOOD,
            0.1,
        );
    }

    fn crate_stack(&mut self, base: Vec3, count: u32) {
        for i in 0..count {
            let size = self.rng.random_range(1.1..1.5);
            let offset = Vec3::new(
                self.rng.random_range(-1.4..1.4),
                0.0,
                self.rng.random_range(-1.4..1.4),
            );
            let stacked = i > 0 && self.rng.random_bool(0.35);
            let y = if stacked { size * 1.5 } else { size * 0.5 };
            let at = if stacked { base } else { base + offset };
            let yaw = self.rng.random_range(-0.4..0.4);
            self.block(
                Vec3::new(at.x, base.y + y, at.z),
                Vec3::splat(size),
                Quat::from_rotation_y(yaw),
                CRATE,
                0.14,
            );
        }
    }

    fn rock(&mut self, at: Vec3, size: Vec3) {
        let seed = self.rng.random::<u32>();
        let raw = lowpoly::rock(size, seed);
        let hull = Collider::convex_hull(&lowpoly::positions(&raw));
        let mesh = lowpoly::facet(raw, ROCK, 0.18, &mut self.rng);
        let transform = Transform::from_translation(at)
            .with_rotation(Quat::from_rotation_y(self.rng.random_range(0.0..TAU)));
        match hull {
            Some(collider) => self.solid(mesh, transform, collider),
            None => {
                self.mesh(mesh, transform);
            }
        }
    }

    fn pine(&mut self, at: Vec3, height: f32, collide: bool) {
        let mesh = lowpoly::pine(height, &mut self.rng);
        let transform = Transform::from_translation(at);
        if collide {
            self.solid(
                mesh,
                transform,
                Collider::compound(vec![(
                    Vec3::Y * height * 0.5,
                    Quat::IDENTITY,
                    Collider::cylinder(height * 0.09, height),
                )]),
            );
        } else {
            self.mesh(mesh, transform);
        }
    }

    fn pillar(&mut self, at: Vec3, height: f32) {
        let mesh = Cylinder::new(0.6, height).mesh().resolution(7).build();
        let mesh = lowpoly::facet(mesh, STONE, 0.1, &mut self.rng);
        self.solid(
            mesh,
            Transform::from_translation(at + Vec3::Y * height / 2.0),
            Collider::cylinder(0.6, height),
        );
    }
}

fn spawn_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Every surface uses one material; color lives in the vertex colors.
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.92,
        reflectance: 0.2,
        ..default()
    });
    let lantern = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.75, 0.45),
        emissive: LinearRgba::rgb(14.0, 6.5, 2.2),
        ..default()
    });
    let flame = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.5, 0.1),
        emissive: LinearRgba::rgb(30.0, 9.0, 1.5),
        unlit: true,
        ..default()
    });

    let mut b = Builder {
        commands: &mut commands,
        meshes: &mut meshes,
        material,
        rng: StdRng::seed_from_u64(0xF0C5),
    };

    terrain(&mut b);
    perimeter(&mut b);
    keep(&mut b);
    platforms(&mut b);

    // Broken colonnade ring.
    for i in 0..8 {
        let angle = (i as f32 + 0.5) / 8.0 * TAU;
        let at = Vec3::new(angle.cos(), 0.0, angle.sin()) * 17.0;
        let height = b.rng.random_range(1.4..5.0);
        b.pillar(at, height);
    }
    for (at, yaw) in [
        (Vec3::new(-9.0, 0.6, 19.0), 0.4),
        (Vec3::new(14.0, 0.6, -9.5), 2.1),
    ] {
        let mesh = Cylinder::new(0.6, 4.0).mesh().resolution(7).build();
        let mesh = lowpoly::facet(mesh, STONE, 0.1, &mut b.rng);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(FRAC_PI_2);
        b.solid(
            mesh,
            Transform::from_translation(at).with_rotation(rotation),
            Collider::cylinder(0.6, 4.0),
        );
    }

    for (at, count) in [
        (Vec3::new(10.0, 0.0, -19.0), 4),
        (Vec3::new(-12.0, 0.0, 18.0), 3),
        (Vec3::new(19.0, 0.0, 6.0), 3),
        (Vec3::new(-20.0, 0.0, -9.0), 4),
        (Vec3::new(5.0, 0.0, 25.0), 2),
        (Vec3::new(-6.0, 0.0, -25.0), 3),
        (Vec3::new(24.0, 2.6, 24.0), 2),
        (Vec3::new(-24.0, 2.6, -24.0), 2),
        (Vec3::new(-3.5, 4.9, -3.0), 2),
    ] {
        b.crate_stack(at, count);
    }

    // Rocks, kept clear of the keep and spawn points.
    let mut placed = 0;
    while placed < 14 {
        let at = Vec3::new(
            b.rng.random_range(-31.0..31.0),
            0.0,
            b.rng.random_range(-31.0..31.0),
        );
        let near_spawn = SPAWNS.iter().any(|s| s.with_y(0.0).distance(at) < 4.0);
        let near_platform = at.x.abs() > 19.0 && at.z.abs() > 19.0;
        if at.length() < 10.0 || near_spawn || near_platform || at.x > 26.0 {
            continue;
        }
        let size = Vec3::new(
            b.rng.random_range(0.7..1.8),
            b.rng.random_range(0.5..1.3),
            b.rng.random_range(0.7..1.8),
        );
        b.rock(at + Vec3::Y * size.y * 0.25, size);
        placed += 1;
    }

    // A few pines inside the walls, a forest outside them.
    let mut placed = 0;
    while placed < 9 {
        let angle = b.rng.random_range(0.0..TAU);
        let radius = b.rng.random_range(24.0..32.0);
        let at = Vec3::new(angle.cos(), 0.0, angle.sin()) * radius;
        let near_platform = at.x.abs() > 19.0 && at.z.abs() > 19.0;
        let near_spawn = SPAWNS.iter().any(|s| s.with_y(0.0).distance(at) < 4.0);
        if near_platform || near_spawn || at.x > 26.0 || at.x.abs() > 33.0 || at.z.abs() > 33.0 {
            continue;
        }
        let height = b.rng.random_range(5.0..8.0);
        b.pine(at, height, true);
        placed += 1;
    }
    for _ in 0..180 {
        let angle = b.rng.random_range(0.0..TAU);
        let radius = b.rng.random_range(42.0..88.0);
        let (x, z) = (angle.cos() * radius, angle.sin() * radius);
        let y = terrain_height(x, z) - 0.3;
        let height = b.rng.random_range(6.0..12.0);
        b.pine(Vec3::new(x, y, z), height, false);
    }

    // Lamps.
    for (i, at) in [
        Vec3::new(11.0, 0.0, 11.0),
        Vec3::new(-11.0, 0.0, 11.0),
        Vec3::new(11.0, 0.0, -11.0),
        Vec3::new(-11.0, 0.0, -11.0),
        Vec3::new(0.0, 0.0, 31.0),
        Vec3::new(0.0, 0.0, -31.0),
        Vec3::new(-31.0, 0.0, 0.0),
        Vec3::new(31.0, 0.0, -15.0),
        Vec3::new(31.0, 0.0, 15.0),
    ]
    .into_iter()
    .enumerate()
    {
        lamp(&mut b, at, lantern.clone(), i % 3 == 0);
    }

    campfire(&mut b, Vec3::new(2.8, 0.0, 0.0), flame);

    // Moonlight.
    b.commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.55, 0.62, 1.0),
            illuminance: 9.0,
            shadow_maps_enabled: true,
            contact_shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 1.0, 0.0).looking_at(Vec3::new(-0.6, -1.0, 0.45), Vec3::Y),
        CascadeShadowConfigBuilder {
            maximum_distance: 90.0,
            first_cascade_far_bound: 14.0,
            ..default()
        }
        .build(),
    ));
}

fn terrain_height(x: f32, z: f32) -> f32 {
    let d = x.abs().max(z.abs());
    let t = ((d - (ARENA + 2.0)) / 45.0).clamp(0.0, 1.0);
    let ramp = t * t * (3.0 - 2.0 * t);
    let n = lowpoly::hash3(Vec3::new(x, 0.0, z), 7);
    ramp * (18.0 + n * 5.0) + n * 0.9 * t.sqrt()
}

fn terrain(b: &mut Builder) {
    let raw = lowpoly::grid(200.0, 80, terrain_height);
    let grass = Color::srgb(0.11, 0.17, 0.10);
    let moss = Color::srgb(0.16, 0.2, 0.12);
    let rock = Color::srgb(0.22, 0.22, 0.25);
    let mesh = lowpoly::facet_with(raw, &mut b.rng, |c, rng| {
        let t = (c.y / 14.0).clamp(0.0, 1.0);
        let base = if rng.random_bool(0.3) { moss } else { grass };
        let mixed = base.mix(&rock, t);
        lowpoly::shade(mixed, 0.16, rng)
    });
    b.mesh(mesh, Transform::default());
    // The playable floor is flat, so a slab is all physics needs.
    b.commands.spawn((
        RigidBody::Static,
        Collider::cuboid(ARENA * 2.0 + 10.0, 1.0, ARENA * 2.0 + 10.0),
        CollisionLayers::new(Layer::World, LayerMask::ALL),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
}

fn perimeter(b: &mut Builder) {
    let thickness = 1.4;
    let span = ARENA * 2.0 + thickness * 2.0;
    let segments = 19;
    let seg = span / segments as f32;
    let edge = ARENA + thickness / 2.0;
    for side in 0..4 {
        for i in 0..segments {
            let along = -span / 2.0 + seg * (i as f32 + 0.5);
            let height = b.rng.random_range(3.0..4.3);
            let (center, size) = if side < 2 {
                let z = if side == 0 { -edge } else { edge };
                (
                    Vec3::new(along, height / 2.0, z),
                    Vec3::new(seg + 0.02, height, thickness),
                )
            } else {
                let x = if side == 2 { -edge } else { edge };
                (
                    Vec3::new(x, height / 2.0, along),
                    Vec3::new(thickness, height, seg + 0.02),
                )
            };
            let color = if b.rng.random_bool(0.3) {
                STONE_DARK
            } else {
                STONE
            };
            b.block(center, size, Quat::IDENTITY, color, 0.06);
        }
    }
}

fn keep(b: &mut Builder) {
    let h = 4.5;
    let t = 0.8;
    let door = 1.3;
    // North and south walls with a doorway and lintel.
    for z in [-6.0, 6.0] {
        for x in [-1.0, 1.0] {
            let len = 6.0 + t / 2.0 - door;
            b.block(
                Vec3::new(x * (door + len / 2.0), h / 2.0, z),
                Vec3::new(len, h, t),
                Quat::IDENTITY,
                STONE,
                0.06,
            );
        }
        b.block(
            Vec3::new(0.0, h - 0.45, z),
            Vec3::new(door * 2.0, 0.9, t),
            Quat::IDENTITY,
            STONE_DARK,
            0.06,
        );
    }
    // East wall with a doorway.
    for z in [-1.0, 1.0] {
        let len = 6.0 - t / 2.0 - door;
        b.block(
            Vec3::new(6.0, h / 2.0, z * (door + len / 2.0)),
            Vec3::new(t, h, len),
            Quat::IDENTITY,
            STONE,
            0.06,
        );
    }
    b.block(
        Vec3::new(6.0, h - 0.45, 0.0),
        Vec3::new(t, 0.9, door * 2.0),
        Quat::IDENTITY,
        STONE_DARK,
        0.06,
    );
    // Solid west wall.
    b.block(
        Vec3::new(-6.0, h / 2.0, 0.0),
        Vec3::new(t, h, 12.0 - t),
        Quat::IDENTITY,
        STONE,
        0.06,
    );
    // Half roof over the west side, with a parapet and a ramp up.
    b.block(
        Vec3::new(-3.2, h + 0.2, 0.0),
        Vec3::new(6.4 + 0.4, 0.4, 12.8),
        Quat::IDENTITY,
        WOOD,
        0.12,
    );
    b.block(
        Vec3::new(-0.2, h + 0.4 + 0.35, 0.0),
        Vec3::new(0.4, 0.7, 12.8),
        Quat::IDENTITY,
        STONE_DARK,
        0.06,
    );
    b.ramp(
        Vec3::new(-16.5, 0.0, 0.0),
        Vec3::new(-6.4, h + 0.4, 0.0),
        3.0,
    );
}

fn platforms(b: &mut Builder) {
    let height = 2.6;
    for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let center = Vec3::new(24.0 * sx, height / 2.0, 24.0 * sz);
        b.block(
            center,
            Vec3::new(7.0, height, 7.0),
            Quat::IDENTITY,
            STONE,
            0.06,
        );
        // Ramp runs along x toward the middle of the arena.
        b.ramp(
            Vec3::new(13.0 * sx, 0.0, 24.0 * sz),
            Vec3::new(20.5 * sx, height, 24.0 * sz),
            2.6,
        );
    }
}

fn lamp(b: &mut Builder, at: Vec3, lantern: Handle<StandardMaterial>, shadows: bool) {
    let post_h = 3.4;
    b.block(
        at + Vec3::Y * post_h / 2.0,
        Vec3::new(0.2, post_h, 0.2),
        Quat::IDENTITY,
        IRON,
        0.0,
    );
    let mesh = b.meshes.add(Cuboid::new(0.34, 0.42, 0.34));
    b.commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(lantern),
        Transform::from_translation(at + Vec3::Y * (post_h + 0.2)),
        bevy::light::NotShadowCaster,
    ));
    b.commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.68, 0.38),
            intensity: 14_000.0,
            range: 20.0,
            radius: 0.15,
            shadow_maps_enabled: shadows,
            contact_shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(at + Vec3::Y * (post_h - 0.1)),
        Flicker {
            base: 14_000.0,
            seed: at.x * 0.37 + at.z * 0.71,
        },
    ));
}

fn campfire(b: &mut Builder, at: Vec3, flame: Handle<StandardMaterial>) {
    for i in 0..4 {
        let yaw = i as f32 / 4.0 * TAU + 0.3;
        let mesh = Cylinder::new(0.11, 1.2).mesh().resolution(6).build();
        let mesh = lowpoly::facet(mesh, WOOD, 0.15, &mut b.rng);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(1.25);
        b.mesh(
            mesh,
            Transform::from_translation(at + Vec3::Y * 0.25).with_rotation(rotation),
        );
    }
    for i in 0..8 {
        let angle = i as f32 / 8.0 * TAU;
        let seed = b.rng.random::<u32>();
        let mesh = lowpoly::facet(
            lowpoly::rock(Vec3::new(0.22, 0.16, 0.18), seed),
            ROCK,
            0.15,
            &mut b.rng,
        );
        b.mesh(
            mesh,
            Transform::from_translation(at + Vec3::new(angle.cos(), 0.05, angle.sin()) * 0.75),
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
            bevy::light::NotShadowCaster,
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
        Flicker {
            base: 40_000.0,
            seed: 3.3,
        },
    ));
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
