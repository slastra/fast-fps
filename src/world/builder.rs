//! Building blocks for levels. Each helper spawns a mesh and, unless it says
//! otherwise, a matching collider. Color is baked into the mesh, so
//! everything shares one material.
//!
//! The builder carries two random number generators, and the difference
//! matters:
//! - `rng` decides **layout**: where things with collision go and how big
//!   they are. Every machine must agree on this, so only use it for that.
//! - `paint` decides **looks**: shading, and decoration without collision.
//!   Use it freely; changing how much of it you use never moves the map.

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::{light::NotShadowCaster, prelude::*};
use rand::{Rng, rngs::StdRng};

use crate::{lowpoly, physics::Layer};

pub const STONE: Color = Color::srgb(0.36, 0.36, 0.4);
pub const STONE_DARK: Color = Color::srgb(0.25, 0.25, 0.3);
pub const WOOD: Color = Color::srgb(0.42, 0.26, 0.14);
pub const WOOD_DARK: Color = Color::srgb(0.27, 0.16, 0.09);
const CRATE: Color = Color::srgb(0.56, 0.38, 0.2);
pub const IRON: Color = Color::srgb(0.08, 0.08, 0.09);
pub const ROCK: Color = Color::srgb(0.31, 0.31, 0.33);

pub struct Builder<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub material: Handle<StandardMaterial>,
    /// Layout: anything with collision.
    pub rng: StdRng,
    /// Looks: shading and decoration.
    pub paint: StdRng,
}

impl Builder<'_, '_, '_> {
    /// A mesh with no collision.
    pub fn mesh(&mut self, mesh: Mesh, transform: Transform) -> EntityCommands<'_> {
        let handle = self.meshes.add(mesh);
        self.commands.spawn((
            Mesh3d(handle),
            MeshMaterial3d(self.material.clone()),
            transform,
        ))
    }

    /// A mesh with a collider.
    pub fn solid(&mut self, mesh: Mesh, transform: Transform, collider: Collider) {
        self.mesh(mesh, transform).insert((
            RigidBody::Static,
            collider,
            CollisionLayers::new(Layer::World, LayerMask::ALL),
        ));
    }

    /// A box you can stand on and hide behind.
    pub fn block(&mut self, center: Vec3, size: Vec3, rotation: Quat, color: Color, jitter: f32) {
        let mesh = lowpoly::block(size, color, jitter, &mut self.paint);
        self.solid(
            mesh,
            Transform::from_translation(center).with_rotation(rotation),
            Collider::cuboid(size.x, size.y, size.z),
        );
    }

    /// A decorative box with no collision: ledges, beams, caps, banners.
    pub fn trim(&mut self, center: Vec3, size: Vec3, color: Color) {
        let mesh = lowpoly::block(size, color, 0.07, &mut self.paint);
        self.mesh(mesh, Transform::from_translation(center));
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
        // Side rails, set just below the surface so feet don't catch on them.
        for side in [-1.0, 1.0] {
            let rail = lowpoly::block(
                Vec3::new(length, thickness + 0.12, 0.16),
                WOOD_DARK,
                0.08,
                &mut self.paint,
            );
            let at = center + rotation * Vec3::new(0.0, -0.08, side * (width / 2.0 + 0.04));
            self.mesh(
                rail,
                Transform::from_translation(at).with_rotation(rotation),
            );
        }
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
            let mesh = lowpoly::crate_box(size, CRATE, &mut self.paint);
            self.solid(
                mesh,
                Transform::from_xyz(at.x, base.y + y, at.z)
                    .with_rotation(Quat::from_rotation_y(yaw)),
                Collider::cuboid(size, size, size),
            );
        }
    }

    /// A barrel standing at `at`.
    pub fn barrel(&mut self, at: Vec3) {
        let (radius, height) = (0.42, 1.05);
        let mesh = lowpoly::barrel(radius, height, &mut self.paint);
        self.solid(
            mesh,
            Transform::from_translation(at),
            Collider::compound(vec![(
                Vec3::Y * height / 2.0,
                Quat::IDENTITY,
                Collider::cylinder(radius * 1.08, height),
            )]),
        );
    }

    pub fn rock(&mut self, at: Vec3, size: Vec3) {
        let seed = self.rng.random::<u32>();
        let raw = lowpoly::rock(size, seed);
        let hull = Collider::convex_hull(&lowpoly::positions(&raw));
        let yaw = self.rng.random_range(0.0..TAU);
        let mesh = lowpoly::mossy(raw, ROCK, &mut self.paint);
        let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw));
        match hull {
            Some(collider) => self.solid(mesh, transform, collider),
            None => {
                self.mesh(mesh, transform);
            }
        }
    }

    /// A bush with no collision.
    pub fn bush(&mut self, at: Vec3, size: f32) {
        let mesh = lowpoly::bush(size, &mut self.paint);
        self.mesh(mesh, Transform::from_translation(at));
    }

    pub fn pine(&mut self, at: Vec3, height: f32, collide: bool) {
        let mesh = lowpoly::pine(height, &mut self.paint);
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
            // The far forest is scenery; skip its shadows.
            self.mesh(mesh, transform).insert(NotShadowCaster);
        }
    }

    /// A stone column on a square base.
    pub fn pillar(&mut self, at: Vec3, height: f32) {
        let shaft = Cylinder::new(0.55, height)
            .mesh()
            .resolution(8)
            .build()
            .translated_by(Vec3::Y * height / 2.0);
        let base = lowpoly::block(
            Vec3::new(1.45, 0.3, 1.45),
            STONE_DARK,
            0.06,
            &mut self.paint,
        )
        .translated_by(Vec3::Y * 0.15);
        let ring = Cylinder::new(0.66, 0.14)
            .mesh()
            .resolution(8)
            .build()
            .translated_by(Vec3::Y * 0.37);
        let mesh = lowpoly::merge([
            lowpoly::facet(shaft, STONE, 0.1, &mut self.paint),
            base,
            lowpoly::facet(ring, STONE_DARK, 0.06, &mut self.paint),
        ]);
        self.solid(
            mesh,
            Transform::from_translation(at),
            Collider::compound(vec![(
                Vec3::Y * height / 2.0,
                Quat::IDENTITY,
                Collider::cylinder(0.6, height),
            )]),
        );
    }
}
