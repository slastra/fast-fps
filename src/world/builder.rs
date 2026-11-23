//! Building blocks for levels. Each helper spawns a mesh and a matching
//! collider, colored through vertex colors on one shared material.

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::{Rng, rngs::StdRng};

use crate::{lowpoly, physics::Layer};

pub const STONE: Color = Color::srgb(0.33, 0.33, 0.37);
pub const STONE_DARK: Color = Color::srgb(0.24, 0.24, 0.28);
pub const WOOD: Color = Color::srgb(0.42, 0.26, 0.14);
const CRATE: Color = Color::srgb(0.55, 0.37, 0.19);
pub const IRON: Color = Color::srgb(0.08, 0.08, 0.09);
pub const ROCK: Color = Color::srgb(0.29, 0.29, 0.31);

pub struct Builder<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub rng: StdRng,
}

impl Builder<'_, '_, '_> {
    pub fn mesh(&mut self, mesh: Mesh, transform: Transform) -> EntityCommands<'_> {
        let handle = self.meshes.add(mesh);
        self.commands.spawn((
            Mesh3d(handle),
            MeshMaterial3d(self.material.clone()),
            transform,
        ))
    }

    pub fn solid(&mut self, mesh: Mesh, transform: Transform, collider: Collider) {
        self.mesh(mesh, transform).insert((
            RigidBody::Static,
            collider,
            CollisionLayers::new(Layer::World, LayerMask::ALL),
        ));
    }

    /// An axis-aligned (optionally rotated) box with a matching collider.
    pub fn block(&mut self, center: Vec3, size: Vec3, rotation: Quat, color: Color, jitter: f32) {
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
    pub fn ramp(&mut self, low: Vec3, high: Vec3, width: f32) {
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

    pub fn crate_stack(&mut self, base: Vec3, count: u32) {
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

    pub fn rock(&mut self, at: Vec3, size: Vec3) {
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

    pub fn pine(&mut self, at: Vec3, height: f32, collide: bool) {
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

    pub fn pillar(&mut self, at: Vec3, height: f32) {
        let mesh = Cylinder::new(0.6, height).mesh().resolution(7).build();
        let mesh = lowpoly::facet(mesh, STONE, 0.1, &mut self.rng);
        self.solid(
            mesh,
            Transform::from_translation(at + Vec3::Y * height / 2.0),
            Collider::cylinder(0.6, height),
        );
    }
}
