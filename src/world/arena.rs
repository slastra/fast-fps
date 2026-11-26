//! The arena layout: a walled courtyard with a ruined keep in the middle,
//! raised corner platforms, crates, rocks and pines.
//!
//! To change the map, edit [`spawn_level`] and the constants above it. Things
//! with collision are placed first, using the builder's layout `rng`. The
//! decoration after them (grass, bushes, trim) uses `paint` and can be
//! changed freely without moving anything players collide with.

use std::f32::consts::{FRAC_PI_2, TAU};

use avian3d::prelude::*;
use bevy::{light::NotShadowCaster, prelude::*};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{
    lowpoly,
    physics::Layer,
    world::{
        builder::{Builder, Footprint, STONE, STONE_DARK, WOOD, WOOD_DARK},
        lights::{campfire, fireflies, lamp},
    },
};

/// Half the width of the playable square.
pub const ARENA: f32 = 36.0;

/// The outer wall: how thick it is, and the segments it's built from.
const WALL_THICKNESS: f32 = 1.4;
const WALL_SEGMENTS: usize = 19;
const WALL_SEG: f32 = (ARENA * 2.0 + WALL_THICKNESS * 2.0) / WALL_SEGMENTS as f32;

/// Breaches in the outer wall, as (side, segment): holes the level always
/// has, bricked up. A game mode can open them (survival's cracks, in
/// `survival/cracks.rs`, open them in this order). Sides: 0 north (-z),
/// 1 south (+z), 2 west (-x), 3 east (+x).
pub const BREACHES: [(usize, usize); 12] = [
    (0, 10),
    (1, 8),
    (0, 8),
    (1, 10),
    (3, 15),
    (1, 7),
    (0, 11),
    (2, 15),
    (3, 14),
    (2, 14),
    (2, 11),
    (3, 10),
];
/// How wide and high a breach is.
pub const BREACH_SIZE: Vec2 = Vec2::new(1.7, 2.4);

/// Where breach `i` is on the wall's inner face (x, z), and which way is
/// inward.
pub const fn breach(i: usize) -> (Vec2, Vec2) {
    let (side, seg) = BREACHES[i];
    let along = -WALL_SEG * WALL_SEGMENTS as f32 / 2.0 + WALL_SEG * (seg as f32 + 0.5);
    match side {
        0 => (Vec2::new(along, -ARENA), Vec2::new(0.0, 1.0)),
        1 => (Vec2::new(along, ARENA), Vec2::new(0.0, -1.0)),
        2 => (Vec2::new(-ARENA, along), Vec2::new(1.0, 0.0)),
        _ => (Vec2::new(ARENA, along), Vec2::new(-1.0, 0.0)),
    }
}

/// The outline of breach `i`, as courses of stone from the ground up: each
/// (bottom, top, half the hole's width there). Ragged at the sides and
/// narrowing to a rough arch at the top. It has collision, so it's the same
/// on every machine; `survival/scene.rs` dresses the hole along it.
pub fn breach_courses(i: usize) -> Vec<(f32, f32, f32)> {
    let mut rng = StdRng::seed_from_u64(0xb4ea_c500 + i as u64);
    let (w, h) = (BREACH_SIZE.x, BREACH_SIZE.y);
    let mut out = Vec::new();
    let mut y = 0.0;
    while y < h - 0.01 {
        let mut top = y + rng.random_range(0.3..0.48);
        if h - top < 0.2 {
            top = h;
        }
        let t = ((y + top) / 2.0 / h - 0.55).max(0.0) / 0.45;
        let arch = 1.0 - 0.5 * t * t * (3.0 - 2.0 * t);
        let half = (w / 2.0 * arch + rng.random_range(-0.12..0.12)).clamp(0.32, w / 2.0 + 0.12);
        out.push((y, top, half));
        y = top;
    }
    out
}

/// The block filling breach `i`, and its collider: both carry this. Open,
/// the block is hidden and the collider only stops the local player
/// (`Layer::Barrier`), so zombies, bullets and grenades go through but
/// nobody walks out of the arena.
#[derive(Component)]
pub struct Breach(pub usize);

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

pub fn spawn_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Every surface uses one material; color lives in the vertex colors.
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        reflectance: 0.25,
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
    let firefly = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 1.0, 0.4),
        emissive: LinearRgba::rgb(9.0, 14.0, 2.5),
        unlit: true,
        ..default()
    });

    let mut b = Builder {
        commands: &mut commands,
        meshes: &mut meshes,
        material,
        rng: StdRng::seed_from_u64(0xF0C5),
        paint: StdRng::seed_from_u64(0xC0FFEE),
        taken: Vec::new(),
        batches: default(),
    };

    // ---- Things you can collide with (layout) ------------------------------

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
        let mesh = Cylinder::new(0.6, 4.0).mesh().resolution(8).build();
        let mesh = lowpoly::mossy(mesh, STONE, &mut b.paint);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(FRAC_PI_2);
        // It lies along its own x axis once tipped over.
        b.taken
            .push(Footprint::new(at, Vec3::new(4.0, 1.2, 1.2), yaw));
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
    for at in [
        Vec3::new(7.2, 0.0, -17.2),
        Vec3::new(6.4, 0.0, -17.9),
        Vec3::new(-14.6, 0.0, 15.6),
        Vec3::new(21.5, 0.0, 3.4),
        Vec3::new(-17.6, 0.0, -11.8),
        Vec3::new(-18.5, 0.0, -11.5),
        Vec3::new(4.6, 0.0, -4.4),
    ] {
        b.barrel(at);
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
        if at.length() < 10.0 || near_spawn || near_platform || at.x > 26.0 || in_yard(at) {
            continue;
        }
        let size = Vec3::new(
            b.rng.random_range(0.7..1.8),
            b.rng.random_range(0.5..1.3),
            b.rng.random_range(0.7..1.8),
        );
        if b.rock(at + Vec3::Y * size.y * 0.25, size) {
            placed += 1;
        }
    }

    // A few pines inside the walls.
    let mut pines = Vec::new();
    while pines.len() < 9 {
        let angle = b.rng.random_range(0.0..TAU);
        let radius = b.rng.random_range(24.0..32.0);
        let at = Vec3::new(angle.cos(), 0.0, angle.sin()) * radius;
        let near_platform = at.x.abs() > 19.0 && at.z.abs() > 19.0;
        let near_spawn = SPAWNS.iter().any(|s| s.with_y(0.0).distance(at) < 4.0);
        if near_platform || near_spawn || at.x > 26.0 || at.x.abs() > 33.0 || at.z.abs() > 33.0 {
            continue;
        }
        let height = b.rng.random_range(5.0..8.0);
        if b.pine(at, height, true) {
            pines.push(at);
        }
    }

    // Lamps.
    for at in [
        Vec3::new(11.0, 0.0, 11.0),
        Vec3::new(-11.0, 0.0, 11.0),
        Vec3::new(11.0, 0.0, -11.0),
        Vec3::new(-11.0, 0.0, -11.0),
        Vec3::new(0.0, 0.0, 31.0),
        Vec3::new(0.0, 0.0, -31.0),
        Vec3::new(-31.0, 0.0, 0.0),
        Vec3::new(31.0, 0.0, -15.0),
        Vec3::new(31.0, 0.0, 15.0),
    ] {
        lamp(&mut b, at, lantern.clone());
    }

    campfire(&mut b, Vec3::new(2.8, 0.0, 0.0), flame);

    // ---- Decoration (no collision; uses `paint` only) ----------------------

    // The forest outside the walls.
    for _ in 0..200 {
        let angle = b.paint.random_range(0.0..TAU);
        let radius = b.paint.random_range(42.0..90.0);
        let (x, z) = (angle.cos() * radius, angle.sin() * radius);
        let y = terrain_height(x, z) - 0.3;
        let height = b.paint.random_range(6.0..13.0);
        b.pine(Vec3::new(x, y, z), height, false);
    }

    ground_cover(&mut b);

    // Bushes along the inside of the walls and under the pines.
    let mut spots = Vec::new();
    while spots.len() < 34 {
        let along = b.paint.random_range(-34.0..34.0);
        let depth = b.paint.random_range(32.2..35.0);
        let at = match b.paint.random_range(0..4) {
            0 => Vec3::new(along, 0.0, -depth),
            1 => Vec3::new(along, 0.0, depth),
            2 => Vec3::new(-depth, 0.0, along),
            _ => Vec3::new(depth, 0.0, along),
        };
        // Not along the east wall, where they would hide the practice dummies.
        if at.x > 26.0 || SPAWNS.iter().any(|s| s.with_y(0.0).distance(at) < 3.0) {
            continue;
        }
        let size = b.paint.random_range(0.6..1.2);
        b.bush(at, size);
        spots.push(at);
    }
    for pine in &pines {
        let offset = Vec3::new(
            b.paint.random_range(-1.6..1.6),
            0.0,
            b.paint.random_range(-1.6..1.6),
        );
        let size = b.paint.random_range(0.5..0.9);
        b.bush(*pine + offset, size);
    }

    // Fireflies hang around the pines and bushes.
    let homes: Vec<Vec3> = pines
        .iter()
        .chain(spots.iter().step_by(2))
        .enumerate()
        .map(|(i, at)| {
            let height = 0.7 + (i % 4) as f32 * 0.45;
            Vec3::new(at.x * 0.93, height, at.z * 0.93)
        })
        .collect();
    fireflies(&mut b, firefly, &homes);

    // Merge all the static pieces into one mesh per area.
    b.finish();
}

/// The open yard south of the keep. Nothing is placed here: it keeps the
/// approach to the south door clear, and the test scenarios use it as a stage.
fn in_yard(at: Vec3) -> bool {
    at.x.abs() < 8.0 && at.z > 7.0 && at.z < 22.0
}

fn terrain_height(x: f32, z: f32) -> f32 {
    let d = x.abs().max(z.abs());
    let t = ((d - (ARENA + 2.0)) / 45.0).clamp(0.0, 1.0);
    let ramp = t * t * (3.0 - 2.0 * t);
    let n = lowpoly::hash3(Vec3::new(x, 0.0, z), 7);
    ramp * (18.0 + n * 5.0) + n * 0.9 * t.sqrt()
}

/// How much of a spot on the ground is worn to dirt, 0 to 1: a track around
/// the keep and paths out from its doors.
fn dirt(x: f32, z: f32) -> f32 {
    let wobble = lowpoly::noise2(x * 0.35, z * 0.35, 3) * 0.9;
    let ring = x.abs().max(z.abs());
    let track = 1.0 - ((ring - 8.1 + wobble).abs() / 1.3).clamp(0.0, 1.0);
    let cross = x.abs().min(z.abs());
    let inside = if ring < ARENA - 1.0 && ring > 6.5 {
        1.0
    } else {
        0.0
    };
    let path = (1.0 - ((cross + wobble * 0.6) / 1.5).clamp(0.0, 1.0)) * inside;
    track.max(path)
}

fn terrain(b: &mut Builder) {
    let raw = lowpoly::grid(200.0, 128, terrain_height);
    let grass_dark = Color::srgb(0.09, 0.17, 0.09);
    let grass_light = Color::srgb(0.16, 0.26, 0.11);
    let earth = Color::srgb(0.23, 0.17, 0.11);
    let flagstone = Color::srgb(0.24, 0.24, 0.27);
    let cliff = Color::srgb(0.24, 0.24, 0.28);
    let mesh = lowpoly::facet_with(raw, &mut b.paint, |c, _, rng| {
        // Inside the keep the floor is stone.
        if c.x.abs() < 5.6 && c.z.abs() < 5.6 {
            return lowpoly::shade(flagstone, 0.13, rng);
        }
        let patch = lowpoly::noise2(c.x * 0.16, c.z * 0.16, 11) * 0.5 + 0.5;
        let grass = grass_dark.mix(&grass_light, patch);
        let ground = grass.mix(&earth, dirt(c.x, c.z));
        let height = (c.y / 14.0).clamp(0.0, 1.0);
        lowpoly::shade(ground.mix(&cliff, height), 0.13, rng)
    });
    b.single(mesh, Transform::default());
    // The playable floor is flat, so a slab is all physics needs.
    b.commands.spawn((
        RigidBody::Static,
        Collider::cuboid(ARENA * 2.0 + 10.0, 1.0, ARENA * 2.0 + 10.0),
        CollisionLayers::new(Layer::World, LayerMask::ALL),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
}

/// Grass tufts and pebbles, merged into a few big meshes.
fn ground_cover(b: &mut Builder) {
    let open = |x: f32, z: f32| {
        let in_keep = x.abs() < 6.6 && z.abs() < 6.6;
        let on_platform = x.abs() > 20.3 && z.abs() > 20.3 && x.abs() < 27.7 && z.abs() < 27.7;
        !in_keep && !on_platform && dirt(x, z) < 0.35
    };
    // One mesh per quarter of the arena, so off-screen grass is skipped.
    for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let mut spots = Vec::new();
        while spots.len() < 520 {
            let x = b.paint.random_range(0.0..ARENA - 0.3) * sx;
            let z = b.paint.random_range(0.0..ARENA - 0.3) * sz;
            // Thicker in patches.
            let patch = lowpoly::noise2(x * 0.2, z * 0.2, 5) * 0.5 + 0.5;
            if open(x, z) && b.paint.random::<f32>() < 0.25 + patch {
                spots.push(Vec2::new(x, z));
            }
        }
        let mesh = lowpoly::grass(&spots, &mut b.paint);
        b.single(mesh, Transform::default()).insert(NotShadowCaster);
    }

    let mut pebbles = Vec::new();
    while pebbles.len() < 150 {
        let x = b.paint.random_range(-ARENA + 1.0..ARENA - 1.0);
        let z = b.paint.random_range(-ARENA + 1.0..ARENA - 1.0);
        if x.abs() < 6.6 && z.abs() < 6.6 {
            continue;
        }
        let size = b.paint.random_range(0.05..0.16);
        let seed = b.paint.random::<u32>();
        let shape = Vec3::new(size, size * 0.6, size * b.paint.random_range(0.7..1.4));
        let stone = lowpoly::rock(shape, seed).translated_by(Vec3::new(x, size * 0.2, z));
        let color = Color::srgb(0.3, 0.3, 0.32);
        pebbles.push(lowpoly::facet(stone, color, 0.2, &mut b.paint));
    }
    let mesh = lowpoly::merge(pebbles);
    b.single(mesh, Transform::default()).insert(NotShadowCaster);
}

fn perimeter(b: &mut Builder) {
    let thickness = WALL_THICKNESS;
    let span = ARENA * 2.0 + thickness * 2.0;
    let segments = WALL_SEGMENTS;
    let seg = WALL_SEG;
    let edge = ARENA + thickness / 2.0;
    for side in 0..4 {
        for i in 0..segments {
            let along = -span / 2.0 + seg * (i as f32 + 0.5);
            let height = b.rng.random_range(3.0..4.3);
            // Walls 0 and 1 run along x; 2 and 3 along z.
            let place = |a: f32, y: f32, across: f32| {
                let offset = edge + across;
                match side {
                    0 => Vec3::new(a, y, -offset),
                    1 => Vec3::new(a, y, offset),
                    2 => Vec3::new(-offset, y, a),
                    _ => Vec3::new(offset, y, a),
                }
            };
            let dims = |length: f32, h: f32, depth: f32| {
                if side < 2 {
                    Vec3::new(length, h, depth)
                } else {
                    Vec3::new(depth, h, length)
                }
            };
            let color = if b.paint.random_bool(0.3) {
                STONE_DARK
            } else {
                STONE
            };
            let full = dims(seg + 0.02, height, thickness);
            let footing = dims(seg + 0.02, 0.6, thickness + 0.3);
            match BREACHES.iter().position(|&at| at == (side, i)) {
                None => {
                    b.block(
                        place(along, height / 2.0, 0.0),
                        full,
                        Quat::IDENTITY,
                        color,
                        0.06,
                    );
                    // A wider footing.
                    b.trim(place(along, 0.3, 0.0), footing, STONE_DARK);
                }
                Some(n) => {
                    // Use up the same decoration numbers a whole segment
                    // would, so the rest of the map looks the same.
                    lowpoly::block(full, color, 0.06, &mut b.paint);
                    lowpoly::block(footing, STONE_DARK, 0.07, &mut b.paint);
                    let paint = StdRng::seed_from_u64(0xb4ea_c400 + n as u64);
                    let saved = std::mem::replace(&mut b.paint, paint);
                    // Stone courses around a ragged hole, then solid wall
                    // above it.
                    let h = BREACH_SIZE.y;
                    let edge_of_seg = (seg + 0.02) / 2.0;
                    let courses = breach_courses(n);
                    // Each side: one tall block out to the hole's widest
                    // point, then a strip of stones in courses for its
                    // ragged edge.
                    let widest = courses.iter().map(|c| c.2).fold(0.0, f32::max);
                    for s in [-1.0, 1.0] {
                        let pier = edge_of_seg - widest;
                        b.block(
                            place(along + s * (widest + pier / 2.0), h / 2.0, 0.0),
                            dims(pier, h, thickness),
                            Quat::IDENTITY,
                            color,
                            0.06,
                        );
                        for &(y0, y1, half) in &courses {
                            let strip = widest - half;
                            if strip < 0.02 {
                                continue;
                            }
                            b.block(
                                place(along + s * (half + strip / 2.0), (y0 + y1) / 2.0, 0.0),
                                dims(strip, y1 - y0, thickness),
                                Quat::IDENTITY,
                                color,
                                0.06,
                            );
                        }
                    }
                    let bottom = courses[0].2;
                    for s in [-1.0, 1.0] {
                        let pier = edge_of_seg - bottom;
                        b.trim(
                            place(along + s * (bottom + pier / 2.0), 0.3, 0.0),
                            dims(pier, 0.6, thickness + 0.3),
                            STONE_DARK,
                        );
                    }
                    b.block(
                        place(along, h + (height - h) / 2.0, 0.0),
                        dims(seg + 0.02, height - h, thickness),
                        Quat::IDENTITY,
                        color,
                        0.06,
                    );
                    // The plug: the hole bricked up with rough, darker
                    // stones, set a little back. One mesh and one collider.
                    let center = place(along, h / 2.0, 0.0);
                    let mut stones = Vec::new();
                    let mut shape = Vec::new();
                    for &(y0, y1, half) in &courses {
                        let mid = (y0 + y1) / 2.0 - h / 2.0;
                        let course = dims(half * 2.0, y1 - y0, thickness);
                        shape.push((
                            Vec3::Y * mid,
                            Quat::IDENTITY,
                            Collider::cuboid(course.x, course.y, course.z),
                        ));
                        // Two or three stones to a course.
                        let count = if half > 0.55 { 3 } else { 2 };
                        let length = half * 2.0 / count as f32;
                        for k in 0..count {
                            let a = -half + length * (k as f32 + 0.5);
                            let size = dims(length + 0.01, y1 - y0 + 0.01, thickness - 0.24);
                            let offset = dims(a, 0.0, 0.0) + Vec3::Y * mid;
                            let mesh = lowpoly::block(size, STONE_DARK, 0.14, &mut b.paint);
                            stones.push(mesh.transformed_by(Transform::from_translation(offset)));
                        }
                    }
                    let handle = b.meshes.add(lowpoly::merge(stones));
                    b.commands.spawn((
                        Name::new("Breach plug"),
                        Breach(n),
                        Mesh3d(handle),
                        MeshMaterial3d(b.material.clone()),
                        Transform::from_translation(center),
                    ));
                    b.commands.spawn((
                        Name::new("Breach plug collider"),
                        Breach(n),
                        RigidBody::Static,
                        Collider::compound(shape),
                        CollisionLayers::new(Layer::World, LayerMask::ALL),
                        Transform::from_translation(center),
                    ));
                    let size = dims(BREACH_SIZE.x + 0.4, h, thickness);
                    b.taken.push(Footprint::new(center, size, 0.0));
                    b.paint = saved;
                }
            }
            // A capping course, and battlements on top.
            b.trim(
                place(along, height - 0.1, 0.0),
                dims(seg + 0.02, 0.2, thickness + 0.16),
                STONE_DARK,
            );
            for m in [-0.25, 0.25] {
                b.trim(
                    place(along + seg * m, height + 0.28, 0.0),
                    dims(seg * 0.3, 0.56, thickness),
                    color,
                );
            }
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
            let center = Vec3::new(x * (door + len / 2.0), h / 2.0, z);
            b.block(center, Vec3::new(len, h, t), Quat::IDENTITY, STONE, 0.06);
            b.trim(
                center.with_y(0.25),
                Vec3::new(len, 0.5, t + 0.22),
                STONE_DARK,
            );
        }
        b.block(
            Vec3::new(0.0, h - 0.45, z),
            Vec3::new(door * 2.0, 0.9, t),
            Quat::IDENTITY,
            STONE_DARK,
            0.06,
        );
        // Stone jambs either side of the doorway.
        for x in [-1.0, 1.0] {
            b.trim(
                Vec3::new(x * (door + 0.16), (h - 0.9) / 2.0, z),
                Vec3::new(0.32, h - 0.9, t + 0.14),
                STONE_DARK,
            );
        }
    }
    // East wall with a doorway.
    for z in [-1.0, 1.0] {
        let len = 6.0 - t / 2.0 - door;
        let center = Vec3::new(6.0, h / 2.0, z * (door + len / 2.0));
        b.block(center, Vec3::new(t, h, len), Quat::IDENTITY, STONE, 0.06);
        b.trim(
            center.with_y(0.25),
            Vec3::new(t + 0.22, 0.5, len),
            STONE_DARK,
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
    b.trim(
        Vec3::new(-6.0, 0.25, 0.0),
        Vec3::new(t + 0.22, 0.5, 12.0 - t),
        STONE_DARK,
    );
    // Heavier stones at the four corners.
    for x in [-6.0, 6.0] {
        for z in [-6.0, 6.0] {
            b.trim(
                Vec3::new(x, (h + 0.5) / 2.0, z),
                Vec3::new(t + 0.3, h + 0.5, t + 0.3),
                STONE_DARK,
            );
        }
    }
    // Half roof over the west side, on beams, with a parapet and a ramp up.
    b.block(
        Vec3::new(-3.2, h + 0.2, 0.0),
        Vec3::new(6.4 + 0.4, 0.4, 12.8),
        Quat::IDENTITY,
        WOOD,
        0.12,
    );
    for i in 0..5 {
        let z = -4.8 + i as f32 * 2.4;
        b.trim(
            Vec3::new(-3.0, h - 0.14, z),
            Vec3::new(5.6, 0.28, 0.3),
            WOOD_DARK,
        );
    }
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
    // Banners beside the south and east doors.
    let cloth = Color::srgb(0.5, 0.07, 0.08);
    let gold = Color::srgb(0.75, 0.55, 0.16);
    for x in [-2.6, 2.6] {
        b.trim(Vec3::new(x, 2.7, 6.46), Vec3::new(0.8, 2.3, 0.05), cloth);
        b.trim(Vec3::new(x, 3.6, 6.5), Vec3::new(0.8, 0.14, 0.05), gold);
        b.trim(Vec3::new(x, 3.92, 6.5), Vec3::new(1.0, 0.1, 0.1), WOOD_DARK);
    }
    for z in [-2.6, 2.6] {
        b.trim(Vec3::new(6.46, 2.7, z), Vec3::new(0.05, 2.3, 0.8), cloth);
        b.trim(Vec3::new(6.5, 3.6, z), Vec3::new(0.05, 0.14, 0.8), gold);
        b.trim(Vec3::new(6.5, 3.92, z), Vec3::new(0.1, 0.1, 1.0), WOOD_DARK);
    }
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
        // A footing and a lip around the top.
        b.trim(center.with_y(0.3), Vec3::new(7.3, 0.6, 7.3), STONE_DARK);
        b.trim(
            center.with_y(height - 0.13),
            Vec3::new(7.2, 0.24, 7.2),
            STONE_DARK,
        );
        // Ramp runs along x toward the middle of the arena.
        b.ramp(
            Vec3::new(13.0 * sx, 0.0, 24.0 * sz),
            Vec3::new(20.5 * sx, height, 24.0 * sz),
            2.6,
        );
    }
}
