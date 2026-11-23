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

/// The space something with collision takes up: a box turned about the
/// vertical axis, with a bottom and a top. Used to keep things from being
/// placed inside each other.
#[derive(Clone, Copy, Debug)]
pub struct Footprint {
    center: Vec2,
    half: Vec2,
    yaw: f32,
    bottom: f32,
    top: f32,
}

impl Footprint {
    /// A box `size` across, centered at `center`, turned by `yaw`.
    pub fn new(center: Vec3, size: Vec3, yaw: f32) -> Self {
        Self {
            center: Vec2::new(center.x, center.z),
            half: Vec2::new(size.x, size.z) / 2.0,
            yaw,
            bottom: center.y - size.y / 2.0,
            top: center.y + size.y / 2.0,
        }
    }

    /// Whether two footprints intersect, or come within `gap` of it.
    /// Resting exactly on top of something doesn't count.
    pub fn overlaps(&self, other: &Footprint, gap: f32) -> bool {
        if self.bottom >= other.top - 0.01 || other.bottom >= self.top - 0.01 {
            return false;
        }
        // Separating axis test for two rectangles: they are apart if their
        // shadows on any of the four edge directions don't touch.
        let between = other.center - self.center;
        let axes = |f: &Footprint| {
            let (sin, cos) = f.yaw.sin_cos();
            [Vec2::new(cos, -sin), Vec2::new(sin, cos)]
        };
        let (mine, theirs) = (axes(self), axes(other));
        let reach = |f: &Footprint, own: [Vec2; 2], axis: Vec2| {
            f.half.x * own[0].dot(axis).abs() + f.half.y * own[1].dot(axis).abs()
        };
        mine.into_iter().chain(theirs).all(|axis| {
            between.dot(axis).abs() < reach(self, mine, axis) + reach(other, theirs, axis) + gap
        })
    }
}

/// One crate in a pile.
#[derive(Clone, Copy, Debug)]
pub struct CratePlace {
    pub center: Vec3,
    pub size: f32,
    pub yaw: f32,
}

impl CratePlace {
    fn footprint(&self) -> Footprint {
        Footprint::new(self.center, Vec3::splat(self.size), self.yaw)
    }
}

/// Lay out a pile of `count` crates around `base` (a point on the ground or
/// on a platform). Crates on the ground sit beside each other without
/// touching anything in `taken`; the rest are stacked squarely on top of one
/// below. Every crate placed is added to `taken`.
pub fn arrange_crates(
    base: Vec3,
    count: u32,
    rng: &mut StdRng,
    taken: &mut Vec<Footprint>,
) -> Vec<CratePlace> {
    let mut pile: Vec<CratePlace> = Vec::new();
    // Index of each ground crate that has nothing on it yet.
    let mut open: Vec<usize> = Vec::new();
    for i in 0..count {
        let size: f32 = rng.random_range(1.1..1.5);
        let yaw: f32 = rng.random_range(-0.4..0.4);
        let stack = i > 0 && rng.random_bool(0.35);

        if stack && !open.is_empty() {
            // On the biggest free crate, a little smaller and nearly square
            // to it, so it sits fully on top.
            let pick = (0..open.len())
                .max_by(|a, b| pile[open[*a]].size.total_cmp(&pile[open[*b]].size))
                .unwrap();
            let below = pile[open.swap_remove(pick)];
            let size = size.min(below.size * 0.85);
            let slack = (below.size - size) * 0.25;
            let place = CratePlace {
                center: below.center
                    + Vec3::new(
                        rng.random_range(-slack..=slack),
                        (below.size + size) / 2.0,
                        rng.random_range(-slack..=slack),
                    ),
                size,
                yaw: below.yaw + rng.random_range(-0.12..0.12),
            };
            taken.push(place.footprint());
            pile.push(place);
            continue;
        }

        // On the ground: try spots near the base first, then further out,
        // which keeps the pile tight.
        for attempt in 0..48 {
            let reach = 0.3 + attempt as f32 * 0.09;
            let place = CratePlace {
                center: base
                    + Vec3::new(
                        rng.random_range(-reach..=reach),
                        size / 2.0,
                        rng.random_range(-reach..=reach),
                    ),
                size,
                yaw,
            };
            let footprint = place.footprint();
            if taken.iter().all(|t| !t.overlaps(&footprint, 0.04)) {
                taken.push(footprint);
                open.push(pile.len());
                pile.push(place);
                break;
            }
        }
    }
    pile
}

pub struct Builder<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub material: Handle<StandardMaterial>,
    /// Layout: anything with collision.
    pub rng: StdRng,
    /// Looks: shading and decoration.
    pub paint: StdRng,
    /// Ground already occupied by something with collision.
    pub taken: Vec<Footprint>,
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

    /// Whether a box of `size` at `center` would be clear of everything placed
    /// so far.
    pub fn is_free(&self, center: Vec3, size: Vec3, yaw: f32) -> bool {
        let footprint = Footprint::new(center, size, yaw);
        self.taken.iter().all(|t| !t.overlaps(&footprint, 0.04))
    }

    /// A box you can stand on and hide behind.
    pub fn block(&mut self, center: Vec3, size: Vec3, rotation: Quat, color: Color, jitter: f32) {
        let mesh = lowpoly::block(size, color, jitter, &mut self.paint);
        // Remember the ground it covers. For a tilted box (a ramp) this is
        // the box around it, which is a little generous.
        let (yaw, _, _) = rotation.to_euler(EulerRot::YXZ);
        let half = size / 2.0;
        let mut height = 0.0_f32;
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let corner = rotation * (half * Vec3::new(sx, sy, sz));
                    height = height.max(corner.y.abs());
                }
            }
        }
        self.taken.push(Footprint::new(
            center,
            Vec3::new(size.x, height * 2.0, size.z),
            yaw,
        ));
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

    /// A pile of crates around `base`, none of them overlapping.
    pub fn crate_stack(&mut self, base: Vec3, count: u32) {
        for place in arrange_crates(base, count, &mut self.rng, &mut self.taken) {
            let mesh = lowpoly::crate_box(place.size, CRATE, &mut self.paint);
            self.solid(
                mesh,
                Transform::from_translation(place.center)
                    .with_rotation(Quat::from_rotation_y(place.yaw)),
                Collider::cuboid(place.size, place.size, place.size),
            );
        }
    }

    /// A barrel standing at `at`, or at the nearest clear spot to it.
    pub fn barrel(&mut self, at: Vec3) {
        let (radius, height) = (0.42, 1.05);
        let size = Vec3::new(radius * 2.2, height, radius * 2.2);
        // Step outward in a spiral until it fits.
        let spot = (0..40).find_map(|i| {
            let angle = i as f32 * 2.4;
            let out = i as f32 * 0.07;
            let spot = at + Vec3::new(angle.cos(), 0.0, angle.sin()) * out;
            self.is_free(spot + Vec3::Y * height / 2.0, size, 0.0)
                .then_some(spot)
        });
        let Some(spot) = spot else { return };
        self.taken
            .push(Footprint::new(spot + Vec3::Y * height / 2.0, size, 0.0));
        let mesh = lowpoly::barrel(radius, height, &mut self.paint);
        self.solid(
            mesh,
            Transform::from_translation(spot),
            Collider::compound(vec![(
                Vec3::Y * height / 2.0,
                Quat::IDENTITY,
                Collider::cylinder(radius * 1.08, height),
            )]),
        );
    }

    /// A rock at `at`. Returns false, placing nothing, if the spot is taken.
    pub fn rock(&mut self, at: Vec3, size: Vec3) -> bool {
        // Rocks are lumpy, so claim a bit more than their nominal size.
        let claim = size * 2.3;
        if !self.is_free(at, claim, 0.0) {
            return false;
        }
        self.taken.push(Footprint::new(at, claim, 0.0));
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
        true
    }

    /// A bush with no collision.
    pub fn bush(&mut self, at: Vec3, size: f32) {
        let mesh = lowpoly::bush(size, &mut self.paint);
        self.mesh(mesh, Transform::from_translation(at));
    }

    /// A pine at `at`. With `collide`, it returns false and places nothing if
    /// the spot is taken; scenery pines always go in.
    pub fn pine(&mut self, at: Vec3, height: f32, collide: bool) -> bool {
        let trunk = Vec3::new(height * 0.3, height, height * 0.3);
        let center = at + Vec3::Y * height / 2.0;
        if collide {
            if !self.is_free(center, trunk, 0.0) {
                return false;
            }
            self.taken.push(Footprint::new(center, trunk, 0.0));
        }
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
        true
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
        self.taken.push(Footprint::new(
            at + Vec3::Y * height / 2.0,
            Vec3::new(1.45, height, 1.45),
            0.0,
        ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn footprints_know_when_they_touch() {
        let a = Footprint::new(Vec3::new(0.0, 0.5, 0.0), Vec3::ONE, 0.0);
        let beside = Footprint::new(Vec3::new(1.2, 0.5, 0.0), Vec3::ONE, 0.0);
        let into = Footprint::new(Vec3::new(0.9, 0.5, 0.0), Vec3::ONE, 0.0);
        let on_top = Footprint::new(Vec3::new(0.0, 1.5, 0.0), Vec3::ONE, 0.3);
        // Clear when square on, but turned 45 degrees its corner reaches in.
        let turned = Footprint::new(Vec3::new(1.15, 0.5, 0.0), Vec3::ONE, 0.785);
        assert!(!a.overlaps(&beside, 0.04));
        assert!(a.overlaps(&into, 0.04));
        assert!(!a.overlaps(&on_top, 0.04));
        assert!(a.overlaps(&turned, 0.04));
    }

    /// No two crates in a pile may intersect, every stacked crate must rest
    /// on one below it, and piles must respect what was already there.
    #[test]
    fn crate_piles_never_overlap() {
        for seed in 0..300 {
            let mut rng = StdRng::seed_from_u64(seed);
            let wall = Footprint::new(Vec3::new(1.6, 1.0, 0.0), Vec3::new(0.8, 2.0, 6.0), 0.0);
            let mut taken = vec![wall];
            let base = Vec3::new(0.0, 0.0, 0.0);
            let pile = arrange_crates(base, 2 + (seed % 5) as u32, &mut rng, &mut taken);
            assert!(!pile.is_empty());
            for (i, a) in pile.iter().enumerate() {
                assert!(
                    !a.footprint().overlaps(&wall, 0.0),
                    "seed {seed}: crate in the wall"
                );
                for b in &pile[i + 1..] {
                    assert!(
                        !a.footprint().overlaps(&b.footprint(), 0.0),
                        "seed {seed}: crates overlap"
                    );
                }
                let bottom = a.center.y - a.size / 2.0;
                if bottom > 0.01 {
                    let supported = pile.iter().any(|b| {
                        let top = b.center.y + b.size / 2.0;
                        (top - bottom).abs() < 1e-3
                            && (b.center - a.center).with_y(0.0).length() < b.size * 0.2
                            && a.size <= b.size
                    });
                    assert!(supported, "seed {seed}: a crate floats");
                }
            }
        }
    }
}
