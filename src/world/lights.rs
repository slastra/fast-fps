//! Lamp posts and the campfire, with a flicker.

use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::Rng;

use crate::{
    lowpoly,
    world::builder::{Builder, IRON, ROCK, WOOD},
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, flicker);
}

/// A light that wobbles like fire.
#[derive(Component)]
pub struct Flicker {
    base: f32,
    seed: f32,
}

/// Flame meshes that wobble along with the light.
#[derive(Component)]
pub struct FlameMesh(f32);

pub fn lamp(b: &mut Builder, at: Vec3, lantern: Handle<StandardMaterial>, shadows: bool) {
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

pub fn campfire(b: &mut Builder, at: Vec3, flame: Handle<StandardMaterial>) {
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

pub fn flicker(
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
