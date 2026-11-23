//! Low-poly gun models built from boxes and cylinders.
//!
//! Model space is meters at full size, -Z forward, origin near the trigger.
//! The same parts are used for the first-person view model (scaled toward the
//! eye) and for the gun other players see in your hands.

use bevy::prelude::*;
use rand::rngs::StdRng;

use super::WeaponKind;
use crate::lowpoly;

#[derive(Clone, Copy)]
enum Shape {
    Box(Vec3),
    /// Radius and length, along Z.
    Cyl(f32, f32),
}

#[derive(Clone, Copy)]
pub struct Part {
    shape: Shape,
    at: Vec3,
    tilt: f32,
    color: Color,
    /// Slides back after a shot (the shotgun's pump).
    pub pump: bool,
}

pub struct GunModel {
    pub parts: Vec<Part>,
    /// Muzzle position in model space.
    pub muzzle: Vec3,
    /// Where the view model rests relative to the eye, before scaling.
    pub rest: Vec3,
}

const METAL: Color = Color::srgb(0.11, 0.11, 0.13);
const WORN: Color = Color::srgb(0.2, 0.2, 0.22);
const WOOD: Color = Color::srgb(0.42, 0.22, 0.1);
const DARK_WOOD: Color = Color::srgb(0.28, 0.15, 0.08);
const POLYMER: Color = Color::srgb(0.07, 0.07, 0.08);
const OLIVE: Color = Color::srgb(0.24, 0.27, 0.16);

fn b(size: [f32; 3], at: [f32; 3], tilt: f32, color: Color) -> Part {
    Part {
        shape: Shape::Box(Vec3::from(size)),
        at: Vec3::from(at),
        tilt,
        color,
        pump: false,
    }
}

fn c(radius: f32, length: f32, at: [f32; 3], color: Color) -> Part {
    Part {
        shape: Shape::Cyl(radius, length),
        at: Vec3::from(at),
        tilt: 0.0,
        color,
        pump: false,
    }
}

fn pump(mut part: Part) -> Part {
    part.pump = true;
    part
}

pub fn model(kind: WeaponKind) -> GunModel {
    match kind {
        WeaponKind::Ak => GunModel {
            parts: vec![
                b([0.075, 0.1, 0.36], [0.0, 0.0, -0.1], 0.0, METAL),
                b([0.07, 0.03, 0.3], [0.0, 0.06, -0.1], 0.0, WORN),
                c(0.018, 0.38, [0.0, 0.02, -0.45], METAL),
                c(0.016, 0.22, [0.0, 0.058, -0.36], WORN),
                b([0.074, 0.066, 0.2], [0.0, 0.005, -0.36], 0.0, WOOD),
                b([0.015, 0.05, 0.02], [0.0, 0.06, -0.6], 0.0, METAL),
                c(0.024, 0.05, [0.0, 0.02, -0.66], METAL),
                b([0.05, 0.12, 0.075], [0.0, -0.1, -0.17], 0.15, WORN),
                b([0.05, 0.11, 0.07], [0.0, -0.2, -0.14], 0.45, WORN),
                b([0.045, 0.12, 0.06], [0.0, -0.09, 0.04], -0.35, DARK_WOOD),
                b([0.06, 0.09, 0.24], [0.0, -0.03, 0.22], 0.08, WOOD),
            ],
            muzzle: Vec3::new(0.0, 0.02, -0.7),
            rest: Vec3::new(0.2, -0.19, -0.38),
        },
        WeaponKind::Mp5 => GunModel {
            parts: vec![
                b([0.07, 0.085, 0.3], [0.0, 0.0, -0.08], 0.0, POLYMER),
                b([0.03, 0.02, 0.22], [0.0, 0.052, -0.08], 0.0, METAL),
                b([0.078, 0.072, 0.16], [0.0, -0.005, -0.3], 0.0, POLYMER),
                c(0.015, 0.1, [0.0, 0.01, -0.42], METAL),
                b([0.04, 0.05, 0.02], [0.0, 0.055, -0.36], 0.0, METAL),
                b([0.04, 0.04, 0.03], [0.0, 0.06, 0.04], 0.0, METAL),
                b([0.03, 0.13, 0.05], [0.0, -0.1, -0.13], 0.2, METAL),
                b([0.03, 0.1, 0.05], [0.0, -0.2, -0.1], 0.5, METAL),
                b([0.04, 0.11, 0.06], [0.0, -0.09, 0.04], -0.3, POLYMER),
                b([0.01, 0.012, 0.18], [0.025, 0.0, 0.15], 0.0, METAL),
                b([0.01, 0.012, 0.18], [-0.025, 0.0, 0.15], 0.0, METAL),
                b([0.07, 0.08, 0.02], [0.0, -0.02, 0.25], 0.0, POLYMER),
            ],
            muzzle: Vec3::new(0.0, 0.01, -0.48),
            rest: Vec3::new(0.19, -0.18, -0.36),
        },
        WeaponKind::Glock => GunModel {
            parts: vec![
                b([0.035, 0.045, 0.2], [0.0, 0.035, -0.06], 0.0, METAL),
                b([0.033, 0.025, 0.17], [0.0, 0.0, -0.05], 0.0, POLYMER),
                b([0.034, 0.12, 0.06], [0.0, -0.07, 0.03], -0.25, POLYMER),
                b([0.01, 0.03, 0.05], [0.0, -0.025, -0.03], 0.0, POLYMER),
                b([0.01, 0.012, 0.012], [0.0, 0.063, -0.15], 0.0, METAL),
                b([0.02, 0.012, 0.012], [0.0, 0.063, 0.03], 0.0, METAL),
            ],
            muzzle: Vec3::new(0.0, 0.035, -0.17),
            rest: Vec3::new(0.15, -0.14, -0.34),
        },
        WeaponKind::Shotgun => GunModel {
            parts: vec![
                b([0.07, 0.09, 0.26], [0.0, 0.0, -0.05], 0.0, METAL),
                c(0.022, 0.6, [0.0, 0.03, -0.48], METAL),
                c(0.018, 0.5, [0.0, -0.015, -0.42], WORN),
                pump(b([0.078, 0.062, 0.18], [0.0, -0.02, -0.36], 0.0, WOOD)),
                b([0.065, 0.1, 0.3], [0.0, -0.04, 0.24], 0.1, WOOD),
                b([0.045, 0.09, 0.06], [0.0, -0.08, 0.06], -0.4, DARK_WOOD),
                b([0.01, 0.01, 0.01], [0.0, 0.058, -0.76], 0.0, WORN),
            ],
            muzzle: Vec3::new(0.0, 0.03, -0.8),
            rest: Vec3::new(0.2, -0.2, -0.4),
        },
        WeaponKind::Sniper => GunModel {
            parts: vec![
                b([0.07, 0.08, 0.3], [0.0, 0.0, -0.05], 0.0, METAL),
                c(0.017, 0.75, [0.0, 0.02, -0.55], METAL),
                c(0.025, 0.07, [0.0, 0.02, -0.94], METAL),
                b([0.08, 0.07, 0.36], [0.0, -0.03, -0.33], 0.0, OLIVE),
                b([0.075, 0.12, 0.2], [0.0, -0.05, 0.05], 0.0, OLIVE),
                b([0.07, 0.12, 0.26], [0.0, -0.04, 0.28], 0.0, OLIVE),
                b([0.06, 0.04, 0.16], [0.0, 0.04, 0.26], 0.0, OLIVE),
                c(0.03, 0.32, [0.0, 0.1, -0.06], METAL),
                c(0.042, 0.07, [0.0, 0.1, -0.25], METAL),
                c(0.036, 0.05, [0.0, 0.1, 0.12], METAL),
                b([0.02, 0.04, 0.03], [0.0, 0.06, -0.15], 0.0, METAL),
                b([0.02, 0.04, 0.03], [0.0, 0.06, 0.04], 0.0, METAL),
                b([0.03, 0.03, 0.03], [0.0, 0.14, -0.06], 0.0, WORN),
                b([0.07, 0.014, 0.014], [0.05, 0.02, 0.03], 0.0, METAL),
                b([0.022, 0.022, 0.022], [0.09, 0.02, 0.03], 0.0, WORN),
                b([0.05, 0.06, 0.09], [0.0, -0.08, -0.05], 0.0, METAL),
            ],
            muzzle: Vec3::new(0.0, 0.02, -0.98),
            rest: Vec3::new(0.2, -0.2, -0.42),
        },
    }
}

/// Faceted meshes and local transforms for every part of a gun.
pub fn build(
    model: &GunModel,
    meshes: &mut Assets<Mesh>,
    rng: &mut StdRng,
) -> Vec<(Mesh3d, Transform, Part)> {
    model
        .parts
        .iter()
        .map(|part| {
            let (mesh, rotation) = match part.shape {
                Shape::Box(size) => (
                    Cuboid::from_size(size).mesh().build(),
                    Quat::from_rotation_x(part.tilt),
                ),
                Shape::Cyl(radius, length) => (
                    Cylinder::new(radius, length).mesh().resolution(7).build(),
                    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                ),
            };
            let mesh = Mesh3d(meshes.add(lowpoly::facet(mesh, part.color, 0.1, rng)));
            (
                mesh,
                Transform::from_translation(part.at).with_rotation(rotation),
                *part,
            )
        })
        .collect()
}
