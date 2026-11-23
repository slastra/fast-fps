//! How practice dummies look: a straw target on a post that rocks when hit
//! and tips over when "killed". The server side is in `server::dummies`.

use bevy::prelude::*;
use rand::{SeedableRng, rngs::StdRng};

use crate::{
    lowpoly,
    net::{Damaged, Downed, Dummy, Pose},
};

pub(super) fn plugin(app: &mut App) {
    app.add_observer(dress_dummy)
        .add_systems(Update, pose_dummies);
}

#[derive(Component)]
pub struct DummyPivot;

/// A dummy rocking on its post after being hit: (pitch, roll), a damped spring.
#[derive(Component, Default)]
pub struct Wobble {
    angle: Vec2,
    velocity: Vec2,
}

fn part(meshes: &mut Assets<Mesh>, size: Vec3, color: Color, rng: &mut StdRng) -> Mesh3d {
    Mesh3d(meshes.add(lowpoly::block(size, color, 0.08, rng)))
}

pub fn dress_dummy(
    add: On<Add<Dummy>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = StdRng::seed_from_u64(add.entity.to_bits());
    let base = MeshMaterial3d(materials.add(StandardMaterial {
        perceptual_roughness: 0.95,
        ..default()
    }));
    let straw = Color::srgb(0.72, 0.58, 0.3);
    let sack = Color::srgb(0.62, 0.5, 0.33);
    let wood = Color::srgb(0.36, 0.22, 0.12);
    let red = Color::srgb(0.7, 0.1, 0.08);

    let post = part(&mut meshes, Vec3::new(0.12, 1.0, 0.12), wood, &mut rng);
    let body = part(&mut meshes, Vec3::new(0.56, 0.72, 0.3), straw, &mut rng);
    let head = Mesh3d(meshes.add(lowpoly::facet(
        lowpoly::rock(Vec3::splat(0.2), 11),
        sack,
        0.1,
        &mut rng,
    )));
    let bullseye = Mesh3d(meshes.add(lowpoly::facet(
        Cylinder::new(0.17, 0.03).mesh().resolution(8).build(),
        red,
        0.05,
        &mut rng,
    )));
    let arm = part(&mut meshes, Vec3::new(0.9, 0.1, 0.1), wood, &mut rng);

    // Pivot at the feet so a downed dummy tips over backward.
    commands
        .entity(add.entity)
        .insert(Wobble::default())
        .with_children(|root| {
            root.spawn((
                DummyPivot,
                Transform::from_xyz(0.0, -0.95, 0.0),
                Visibility::default(),
            ))
            .with_children(|pivot| {
                pivot.spawn((post, base.clone(), Transform::from_xyz(0.0, 0.5, 0.0)));
                pivot.spawn((body, base.clone(), Transform::from_xyz(0.0, 1.2, 0.0)));
                pivot.spawn((arm, base.clone(), Transform::from_xyz(0.0, 1.35, 0.0)));
                pivot.spawn((head, base.clone(), Transform::from_xyz(0.0, 1.72, 0.0)));
                pivot.spawn((
                    bullseye,
                    base.clone(),
                    Transform::from_xyz(0.0, 1.2, -0.16)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ));
            });
        });
}

/// Tip downed dummies over, and rock standing ones when they're hit.
pub fn pose_dummies(
    time: Res<Time>,
    mut hits: MessageReader<Damaged>,
    mut dummies: Query<(&Pose, Has<Downed>, &Children, &mut Wobble), With<Dummy>>,
    mut pivots: Query<&mut Transform, With<DummyPivot>>,
) {
    for hit in hits.read() {
        if let Ok((pose, _, _, mut wobble)) = dummies.get_mut(hit.target) {
            let local = Quat::from_rotation_y(-pose.yaw) * hit.dir;
            wobble.velocity += Vec2::new(local.z, -local.x) * 3.0;
        }
    }
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (_, downed, children, mut wobble) in &mut dummies {
        let w = &mut *wobble;
        let accel = -90.0 * w.angle - 5.0 * w.velocity;
        w.velocity += accel * dt;
        w.angle += w.velocity * dt;
        let rock = Quat::from_rotation_x(w.angle.x) * Quat::from_rotation_z(w.angle.y);
        for child in children {
            if let Ok(mut pivot) = pivots.get_mut(*child) {
                if downed {
                    pivot
                        .rotation
                        .smooth_nudge(&Quat::from_rotation_x(1.35), 14.0, dt);
                } else {
                    let upright = pivot.rotation.angle_between(Quat::IDENTITY) < 0.3;
                    if upright {
                        pivot.rotation = rock;
                    } else {
                        pivot.rotation.smooth_nudge(&Quat::IDENTITY, 6.0, dt);
                    }
                }
            }
        }
    }
}
