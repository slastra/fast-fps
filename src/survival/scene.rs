//! Survival in the world: the cracks in the walls, ammo boxes and medkits,
//! a glow on zombies that are burning, chilled or frozen, and the brief
//! flashes the server sends (shock arcs, bursts, burning ground, dust,
//! healing). All of it is for looks; none of it touches the level's
//! collision.

use avian3d::prelude::*;
use bevy::{light::NotShadowCaster, prelude::*};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{
    Me,
    cracks::CRACKS,
    protocol::{BLOATER, BURNING, CHILLED, FROZEN, Flash, LEAPER, Pickup, PickupKind},
};
use crate::{
    avatar::{
        mob::SharedRigs,
        rig::{AvatarRig, Bone},
    },
    net::{Mob, Pose},
    physics::Layer,
    render::no_shadow,
    world::{BREACH_SIZE, breach_courses},
};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Flashes>()
        .add_systems(Startup, (make_assets, make_cracks).chain())
        .add_observer(dress_pickup)
        .add_observer(add_aura)
        .add_systems(
            Update,
            (
                show_cracks,
                burst_open,
                settle_debris,
                spin_pickups,
                auras,
                pulse_boils,
                flare_eyes,
                embers,
                flashes,
                age_fx,
            ),
        );
}

#[derive(Resource)]
struct SceneAssets {
    cube: Handle<Mesh>,
    ball: Handle<Mesh>,
    disc: Handle<Mesh>,
    ring: Handle<Mesh>,
    stone: Handle<StandardMaterial>,
    stone_dark: Handle<StandardMaterial>,
    ammo: Handle<StandardMaterial>,
    ammo_band: Handle<StandardMaterial>,
    med: Handle<StandardMaterial>,
    med_cross: Handle<StandardMaterial>,
    burn: Handle<StandardMaterial>,
    chill: Handle<StandardMaterial>,
    ice: Handle<StandardMaterial>,
    arc: Handle<StandardMaterial>,
    burst: Handle<StandardMaterial>,
    fire: Handle<StandardMaterial>,
    dust: Handle<StandardMaterial>,
    heal: Handle<StandardMaterial>,
    gas: Handle<StandardMaterial>,
    flare: Handle<StandardMaterial>,
    ember: Handle<StandardMaterial>,
}

fn glowing(color: Color, glow: LinearRgba, alpha: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: color.with_alpha(alpha),
        emissive: glow,
        unlit: true,
        alpha_mode: if alpha < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        },
        ..default()
    }
}

fn make_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut lit = |color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.9,
            ..default()
        })
    };
    // The wall's own stone (`world/builder.rs`).
    let stone = lit(Color::srgb(0.36, 0.36, 0.4));
    let stone_dark = lit(Color::srgb(0.25, 0.25, 0.3));
    let ammo = lit(Color::srgb(0.30, 0.34, 0.18));
    let ammo_band = lit(Color::srgb(0.85, 0.68, 0.18));
    let med = lit(Color::srgb(0.92, 0.92, 0.90));
    let med_cross = lit(Color::srgb(0.85, 0.12, 0.10));
    let assets = SceneAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ball: meshes.add(Sphere::new(0.5).mesh().ico(2).unwrap()),
        disc: meshes.add(Cylinder::new(0.5, 0.04)),
        ring: meshes.add(Torus::new(0.38, 0.5)),
        stone,
        stone_dark,
        ammo,
        ammo_band,
        med,
        med_cross,
        burn: materials.add(glowing(
            Color::srgb(1.0, 0.45, 0.1),
            LinearRgba::rgb(3.0, 1.0, 0.2),
            0.28,
        )),
        chill: materials.add(glowing(
            Color::srgb(0.5, 0.75, 1.0),
            LinearRgba::rgb(0.3, 0.8, 2.0),
            0.22,
        )),
        ice: materials.add(glowing(
            Color::srgb(0.75, 0.9, 1.0),
            LinearRgba::rgb(0.6, 1.0, 1.6),
            0.55,
        )),
        arc: materials.add(glowing(
            Color::srgb(0.7, 0.85, 1.0),
            LinearRgba::rgb(4.0, 6.0, 12.0),
            1.0,
        )),
        burst: materials.add(glowing(
            Color::srgb(1.0, 0.7, 0.35),
            LinearRgba::rgb(6.0, 3.0, 0.8),
            0.6,
        )),
        fire: materials.add(glowing(
            Color::srgb(1.0, 0.4, 0.05),
            LinearRgba::rgb(4.0, 1.2, 0.1),
            0.55,
        )),
        dust: materials.add(glowing(
            Color::srgb(0.55, 0.52, 0.48),
            LinearRgba::BLACK,
            0.7,
        )),
        flare: materials.add(glowing(
            Color::srgb(1.0, 0.3, 0.15),
            LinearRgba::rgb(14.0, 2.5, 0.8),
            0.85,
        )),
        ember: materials.add(glowing(
            Color::srgb(1.0, 0.6, 0.15),
            LinearRgba::rgb(9.0, 3.0, 0.4),
            1.0,
        )),
        gas: materials.add(glowing(
            Color::srgb(0.6, 0.85, 0.3),
            LinearRgba::rgb(0.25, 0.5, 0.08),
            0.2,
        )),
        heal: materials.add(glowing(
            Color::srgb(0.4, 1.0, 0.5),
            LinearRgba::rgb(0.8, 4.0, 1.0),
            0.7,
        )),
    };
    commands.insert_resource(assets);
}

/// One crack's drawing, hidden until it opens.
#[derive(Component)]
struct CrackMark(usize);

/// An opened breach: the plug is gone (`cracks::BreachPlugin`), and this
/// dresses the hole. Broken stones jut into it on both faces of the wall,
/// and rubble lies on both sides. Built once for
/// each crack; a cosmetic rng, so it can't move anything.
fn make_cracks(mut commands: Commands, assets: Res<SceneAssets>) {
    // The crack's own frame: x along the wall, y up, z inward. The inner face
    // is z = 0 and the outer face z = -WALL.
    const WALL: f32 = 1.4;
    let (w, h) = (BREACH_SIZE.x, BREACH_SIZE.y);
    for (i, crack) in CRACKS.iter().enumerate() {
        let mut rng = StdRng::seed_from_u64(0x5eed + i as u64);
        let base = Vec3::new(crack.at.x, 0.0, crack.at.y);
        let facing = Quat::from_rotation_y(crack.inward.x.atan2(crack.inward.y));
        commands
            .spawn((
                Name::new("Crack"),
                CrackMark(i),
                Transform::from_translation(base).with_rotation(facing),
                Visibility::Hidden,
            ))
            .with_children(|c| {
                let stone =
                    |c: &mut ChildSpawnerCommands, at: Vec3, size: Vec3, tilt: f32, dark: bool| {
                        c.spawn((
                            Mesh3d(assets.cube.clone()),
                            MeshMaterial3d(if dark {
                                assets.stone_dark.clone()
                            } else {
                                assets.stone.clone()
                            }),
                            Transform::from_translation(at)
                                .with_rotation(Quat::from_rotation_z(tilt))
                                .with_scale(size),
                        ));
                    };
                // Broken edges: stones jutting into the hole along its
                // ragged sides and top, on both faces of the wall.
                let courses = breach_courses(i);
                for face in [0.08, -WALL - 0.08] {
                    for &(y0, y1, half) in &courses {
                        for side in [-1.0, 1.0] {
                            if rng.random_bool(0.25) {
                                continue;
                            }
                            let tall = (y1 - y0) * rng.random_range(0.6..1.0);
                            let size = Vec3::new(
                                rng.random_range(0.16..0.34),
                                tall,
                                rng.random_range(0.14..0.26),
                            );
                            let x = side * (half + rng.random_range(-0.06..0.08));
                            stone(
                                c,
                                Vec3::new(x, (y0 + y1) / 2.0, face),
                                size,
                                rng.random_range(-0.35..0.35),
                                rng.random_bool(0.4),
                            );
                        }
                    }
                    let top = courses.last().map_or(w / 2.0, |c| c.2);
                    let mut x = -top;
                    while x < top {
                        let size = Vec3::new(
                            rng.random_range(0.2..0.4),
                            rng.random_range(0.14..0.3),
                            rng.random_range(0.14..0.26),
                        );
                        let y = h - size.y / 2.0 + rng.random_range(-0.1..0.06);
                        stone(
                            c,
                            Vec3::new(x + size.x / 2.0, y, face),
                            size,
                            rng.random_range(-0.35..0.35),
                            rng.random_bool(0.4),
                        );
                        x += size.x * rng.random_range(0.6..0.95);
                    }
                }
                // Rubble on both sides of the wall, most of it fallen in.
                for (z0, z1, count) in [(0.15, 1.8, 12), (-WALL - 1.2, -WALL - 0.2, 6)] {
                    for _ in 0..count {
                        let s = rng.random_range(0.12..0.38);
                        stone(
                            c,
                            Vec3::new(
                                rng.random_range(-1.3..1.3),
                                s * 0.35,
                                rng.random_range(z0..z1),
                            ),
                            Vec3::splat(s),
                            rng.random_range(0.0..1.0),
                            rng.random_bool(0.5),
                        );
                    }
                }
            });
    }
}

/// A stone from a plug that just burst; the number is how long it's been
/// flying.
#[derive(Component)]
struct Debris(f32);

/// Debris stops moving after this long, starts sinking into the ground
/// after this long, and is gone after this long.
const DEBRIS_SETTLE: f32 = 4.0;
const DEBRIS_SINK: f32 = 8.0;
const DEBRIS_GONE: f32 = 10.0;

/// When a crack opens during a run, its plug bursts inward: its stones fly
/// in and tumble across the ground, in a cloud of dust.
fn burst_open(
    me: Res<Me>,
    assets: Res<SceneAssets>,
    mut seen: Local<Option<u16>>,
    mut commands: Commands,
) {
    let now = me.run.as_ref().map(|r| r.cracks);
    let before = std::mem::replace(&mut *seen, now);
    let (Some(before), Some(now)) = (before, now) else {
        return;
    };
    let opened = now & !before;
    if opened == 0 {
        return;
    }
    let mut rng = rand::rng();
    for (i, crack) in CRACKS.iter().enumerate() {
        if opened & (1 << i) == 0 {
            continue;
        }
        let base = Vec3::new(crack.at.x, 0.0, crack.at.y);
        let facing = Quat::from_rotation_y(crack.inward.x.atan2(crack.inward.y));
        for (y0, y1, half) in breach_courses(i) {
            let count = if half > 0.55 { 3 } else { 2 };
            let length = half * 2.0 / count as f32;
            for k in 0..count {
                let size = Vec3::new(length * 0.9, (y1 - y0) * 0.9, rng.random_range(0.4..0.7));
                let local = Vec3::new(-half + length * (k as f32 + 0.5), (y0 + y1) / 2.0, -0.7);
                let push = Vec3::new(
                    rng.random_range(-1.4..1.4),
                    rng.random_range(0.5..2.5),
                    rng.random_range(2.5..5.5),
                );
                let spin = Vec3::new(
                    rng.random_range(-6.0..6.0),
                    rng.random_range(-6.0..6.0),
                    rng.random_range(-6.0..6.0),
                );
                commands
                    .spawn((
                        Name::new("Debris"),
                        Debris(0.0),
                        RigidBody::Dynamic,
                        Collider::cuboid(size.x, size.y, size.z),
                        CollisionLayers::new(Layer::Ragdoll, Layer::World),
                        LinearVelocity(facing * push),
                        AngularVelocity(spin),
                        Transform::from_translation(base + facing * local).with_rotation(facing),
                        Visibility::default(),
                    ))
                    .with_child((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(assets.stone_dark.clone()),
                        Transform::from_scale(size),
                    ));
            }
        }
        // Dust billowing in through the hole.
        for _ in 0..14 {
            let s = rng.random_range(0.35..0.7);
            let mut puff = fx(
                rng.random_range(1.0..1.8),
                Vec3::splat(s),
                Vec3::splat(s * 2.2),
            );
            puff.rise = facing
                * Vec3::new(
                    rng.random_range(-0.8..0.8),
                    rng.random_range(0.2..0.8),
                    rng.random_range(0.8..2.2),
                );
            let at = Vec3::new(
                rng.random_range(-0.9..0.9),
                rng.random_range(0.2..2.2),
                rng.random_range(-0.5..0.6),
            );
            commands.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.dust.clone()),
                Transform::from_translation(base + facing * at).with_scale(Vec3::splat(s)),
                puff,
                no_shadow(),
            ));
        }
    }
}

/// Debris tumbles, comes to rest, then sinks into the ground and is gone.
fn settle_debris(
    time: Res<Time>,
    mut debris: Query<(Entity, &mut Debris, &mut Transform)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, mut d, mut transform) in &mut debris {
        let was = d.0;
        d.0 += dt;
        if was < DEBRIS_SETTLE && d.0 >= DEBRIS_SETTLE {
            commands
                .entity(entity)
                .remove::<(RigidBody, Collider, LinearVelocity, AngularVelocity)>();
        }
        if d.0 >= DEBRIS_GONE {
            commands.entity(entity).despawn();
        } else if d.0 >= DEBRIS_SINK {
            transform.translation.y -= 0.3 * dt;
        }
    }
}

fn show_cracks(me: Res<Me>, mut cracks: Query<(&CrackMark, &mut Visibility)>) {
    if !me.is_changed() {
        return;
    }
    let open = me.run.as_ref().map_or(0, |r| r.cracks);
    for (mark, mut visibility) in &mut cracks {
        visibility.set_if_neq(if open & (1 << mark.0) != 0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// An ammo box or a medkit, turning slowly where it lies.
fn dress_pickup(
    add: On<Add<Pickup>>,
    pickups: Query<&Pickup>,
    assets: Res<SceneAssets>,
    mut commands: Commands,
) {
    let Ok(pickup) = pickups.get(add.entity) else {
        return;
    };
    commands
        .entity(add.entity)
        .insert(Transform::from_translation(pickup.pos + Vec3::Y * 0.2))
        .with_children(|p| match pickup.kind {
            PickupKind::Ammo => {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.ammo.clone()),
                    Transform::from_scale(Vec3::new(0.42, 0.26, 0.26)),
                ));
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.ammo_band.clone()),
                    Transform::from_scale(Vec3::new(0.44, 0.07, 0.28)),
                ));
            }
            PickupKind::Medkit => {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.med.clone()),
                    Transform::from_scale(Vec3::new(0.36, 0.24, 0.24)),
                ));
                for scale in [Vec3::new(0.2, 0.06, 0.25), Vec3::new(0.06, 0.2, 0.25)] {
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(assets.med_cross.clone()),
                        Transform::from_scale(scale),
                    ));
                }
            }
        });
}

fn spin_pickups(time: Res<Time>, mut pickups: Query<(&Pickup, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (pickup, mut transform) in &mut pickups {
        transform.translation = pickup.pos + Vec3::Y * (0.25 + 0.06 * (t * 2.5).sin());
        transform.rotation = Quat::from_rotation_y(t * 1.4);
    }
}

/// The glow around a zombie that's burning, chilled or frozen.
#[derive(Component)]
struct Aura(u8);

fn add_aura(add: On<Add<Mob>>, assets: Res<SceneAssets>, mut commands: Commands) {
    commands.entity(add.entity).with_child((
        Aura(0),
        Mesh3d(assets.ball.clone()),
        MeshMaterial3d(assets.burn.clone()),
        Transform::from_scale(Vec3::new(1.0, 1.9, 1.0)),
        Visibility::Hidden,
        NotShadowCaster,
    ));
}

fn auras(
    time: Res<Time>,
    mobs: Query<(&Mob, &Children)>,
    mut glows: Query<(
        &mut Aura,
        &mut Visibility,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Mesh3d,
        &mut Transform,
    )>,
    assets: Res<SceneAssets>,
) {
    let flicker = 1.0 + 0.06 * (time.elapsed_secs() * 17.0).sin();
    for (mob, children) in &mobs {
        for child in children {
            let Ok((mut aura, mut visibility, mut material, mut mesh, mut transform)) =
                glows.get_mut(*child)
            else {
                continue;
            };
            let burning = mob.flags & BURNING != 0;
            if burning {
                transform.scale = Vec3::new(flicker, 1.9 * flicker, flicker);
            }
            if aura.0 == mob.flags {
                continue;
            }
            aura.0 = mob.flags;
            let frozen = mob.flags & FROZEN != 0;
            let look = if frozen {
                Some(&assets.ice)
            } else if burning {
                Some(&assets.burn)
            } else if mob.flags & CHILLED != 0 {
                Some(&assets.chill)
            } else {
                None
            };
            match look {
                Some(look) => {
                    material.0 = look.clone();
                    *visibility = Visibility::Inherited;
                    // Frozen: a block of ice. Otherwise a glow around it.
                    if frozen {
                        mesh.0 = assets.cube.clone();
                        transform.scale = Vec3::new(0.95, 1.95, 0.95);
                    } else {
                        mesh.0 = assets.ball.clone();
                        transform.scale = Vec3::new(1.0, 1.9, 1.0);
                    }
                }
                None => *visibility = Visibility::Hidden,
            }
        }
    }
}

/// A bloater's boils (and eyes) swell and fade, all of them together.
fn pulse_boils(
    time: Res<Time>,
    shared: Res<SharedRigs>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut base: Local<Option<LinearRgba>>,
) {
    let Some(handle) = shared.glow(BLOATER) else {
        return;
    };
    let Some(mut material) = materials.get_mut(handle) else {
        return;
    };
    let base = *base.get_or_insert(material.emissive);
    let k = 0.45 + 0.55 * (0.5 + 0.5 * (time.elapsed_secs() * 3.0).sin());
    material.emissive = base * k;
}

/// A glow over a leaper's eyes that flares while it crouches to leap.
#[derive(Component)]
struct Flare(Entity);

/// The leapers already given one.
#[derive(Component)]
struct Flared;

#[allow(clippy::type_complexity)]
fn flare_eyes(
    time: Res<Time>,
    new: Query<(Entity, &Mob, &AvatarRig), Without<Flared>>,
    mobs: Query<&Pose>,
    mut flares: Query<(&Flare, &mut Transform)>,
    assets: Res<SceneAssets>,
    mut commands: Commands,
) {
    for (entity, mob, rig) in &new {
        if mob.kind != LEAPER {
            continue;
        }
        commands.entity(entity).insert(Flared);
        commands.entity(rig.bones[Bone::Head.index()]).with_child((
            Flare(entity),
            Mesh3d(assets.ball.clone()),
            MeshMaterial3d(assets.flare.clone()),
            Transform::from_xyz(0.0, 0.19, -0.17).with_scale(Vec3::ZERO),
            NotShadowCaster,
        ));
    }
    let throb = 1.0 + 0.2 * (time.elapsed_secs() * 22.0).sin();
    for (flare, mut transform) in &mut flares {
        let crouched = mobs.get(flare.0).is_ok_and(|p| p.has(Pose::CROUCH));
        let want = if crouched {
            Vec3::new(0.22, 0.08, 0.04) * throb
        } else {
            Vec3::ZERO
        };
        let now = transform.scale;
        transform.scale = now.lerp(want, (time.delta_secs() * 18.0).min(1.0));
    }
}

/// Sparks thrown off burning zombies.
fn embers(
    time: Res<Time>,
    mobs: Query<(&Mob, &Pose)>,
    assets: Res<SceneAssets>,
    mut commands: Commands,
    mut rng: Local<Option<StdRng>>,
) {
    let rng = rng.get_or_insert_with(|| StdRng::seed_from_u64(31));
    // About eight a second each.
    let chance = (time.delta_secs() * 8.0).min(1.0);
    for (mob, pose) in &mobs {
        if mob.flags & BURNING == 0 || !rng.random_bool(chance as f64) {
            continue;
        }
        let at = pose.pos
            + Vec3::new(
                rng.random_range(-0.3..0.3),
                rng.random_range(-0.6..0.8),
                rng.random_range(-0.3..0.3),
            );
        let s = rng.random_range(0.04..0.08);
        let mut spark = fx(rng.random_range(0.4..0.8), Vec3::splat(s), Vec3::ZERO);
        spark.rise = Vec3::new(
            rng.random_range(-0.4..0.4),
            rng.random_range(1.2..2.4),
            rng.random_range(-0.4..0.4),
        );
        commands.spawn((
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.ember.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(s)),
            spark,
            NotShadowCaster,
        ));
    }
}

/// How many of each flash have arrived, for test scripts.
#[derive(Resource, Default)]
pub struct Flashes {
    pub arcs: u32,
    pub bursts: u32,
    pub fires: u32,
    pub heals: u32,
    pub gases: u32,
}

/// Something brief: it changes size from `from` to `to` over its life,
/// drifting by `rise` a second, then it's gone.
#[derive(Component)]
struct Fx {
    age: f32,
    life: f32,
    from: Vec3,
    to: Vec3,
    rise: Vec3,
}

fn fx(life: f32, from: Vec3, to: Vec3) -> Fx {
    Fx {
        age: 0.0,
        life,
        from,
        to,
        rise: Vec3::ZERO,
    }
}

fn flashes(
    mut flashes: MessageReader<Flash>,
    assets: Res<SceneAssets>,
    mut commands: Commands,
    mut rng: Local<Option<StdRng>>,
    mut count: ResMut<Flashes>,
) {
    let rng = rng.get_or_insert_with(|| StdRng::seed_from_u64(7));
    for flash in flashes.read() {
        match flash {
            Flash::Arc { .. } => count.arcs += 1,
            Flash::Burst { .. } => count.bursts += 1,
            Flash::Fire { .. } => count.fires += 1,
            Flash::Heal { .. } => count.heals += 1,
            Flash::Gas { .. } => count.gases += 1,
            Flash::Dust { .. } => {}
        }
        match *flash {
            Flash::Arc { from, to } => {
                // A jagged line: three segments with a kink or two.
                let mid1 = from.lerp(to, 0.33) + rng_offset(rng, 0.25);
                let mid2 = from.lerp(to, 0.66) + rng_offset(rng, 0.25);
                for (a, b) in [(from, mid1), (mid1, mid2), (mid2, to)] {
                    let len = a.distance(b);
                    if len < 0.01 {
                        continue;
                    }
                    let width = Vec3::new(0.05, 0.05, len);
                    commands.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(assets.arc.clone()),
                        Transform::from_translation((a + b) / 2.0)
                            .looking_at(b, Vec3::Y)
                            .with_scale(width),
                        fx(0.16, width, Vec3::new(0.0, 0.0, len)),
                        no_shadow(),
                    ));
                }
            }
            Flash::Burst { at, radius } => {
                commands.spawn((
                    Mesh3d(assets.ball.clone()),
                    MeshMaterial3d(assets.burst.clone()),
                    Transform::from_translation(at).with_scale(Vec3::splat(0.2)),
                    fx(0.25, Vec3::splat(0.3), Vec3::splat(radius * 2.0)),
                    no_shadow(),
                ));
            }
            Flash::Fire { at, radius, secs } => {
                let size = Vec3::new(radius * 2.0, 1.0, radius * 2.0);
                commands.spawn((
                    Mesh3d(assets.disc.clone()),
                    MeshMaterial3d(assets.fire.clone()),
                    Transform::from_translation(at.with_y(0.05)).with_scale(size),
                    fx(secs, size, size * 0.6),
                    no_shadow(),
                ));
            }
            Flash::Dust { at } => {
                for _ in 0..7 {
                    let s = rng.random_range(0.15..0.3);
                    let mut puff = fx(rng.random_range(0.6..1.1), Vec3::splat(s), Vec3::ZERO);
                    puff.rise = rng_offset(rng, 1.2).with_y(rng.random_range(0.4..1.2));
                    commands.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(assets.dust.clone()),
                        Transform::from_translation(at + rng_offset(rng, 0.4))
                            .with_scale(Vec3::splat(s)),
                        puff,
                        no_shadow(),
                    ));
                }
            }
            Flash::Gas { at, radius, secs } => {
                // A cloud of puffs that swell, drift up and thin out.
                for i in 0..10 {
                    let angle =
                        i as f32 / 10.0 * std::f32::consts::TAU + rng.random_range(-0.3..0.3);
                    let out = Vec3::new(angle.cos(), 0.0, angle.sin())
                        * rng.random_range(0.2..radius * 0.75);
                    let size = rng.random_range(0.6..1.1);
                    let mut puff = fx(
                        secs * rng.random_range(0.75..1.0),
                        Vec3::splat(size * 0.5),
                        Vec3::splat(size * 1.4),
                    );
                    puff.rise = Vec3::Y * rng.random_range(0.05..0.25) + out * 0.05;
                    commands.spawn((
                        Mesh3d(assets.ball.clone()),
                        MeshMaterial3d(assets.gas.clone()),
                        Transform::from_translation(
                            at + out + Vec3::Y * rng.random_range(-0.5..0.4),
                        )
                        .with_scale(Vec3::splat(size * 0.5)),
                        puff,
                        NotShadowCaster,
                    ));
                }
            }
            Flash::Heal { at } => {
                let mut ring = fx(0.9, Vec3::splat(1.0), Vec3::splat(2.2));
                ring.rise = Vec3::Y * 1.2;
                commands.spawn((
                    Mesh3d(assets.ring.clone()),
                    MeshMaterial3d(assets.heal.clone()),
                    Transform::from_translation(at - Vec3::Y * 0.6),
                    ring,
                    no_shadow(),
                ));
            }
        }
    }
}

fn rng_offset(rng: &mut StdRng, size: f32) -> Vec3 {
    Vec3::new(
        rng.random_range(-size..size),
        rng.random_range(-size..size),
        rng.random_range(-size..size),
    )
}

fn age_fx(
    time: Res<Time>,
    mut items: Query<(Entity, &mut Fx, &mut Transform)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, mut fx, mut transform) in &mut items {
        fx.age += dt;
        if fx.age >= fx.life {
            commands.entity(entity).despawn();
            continue;
        }
        let t = fx.age / fx.life;
        transform.scale = fx.from.lerp(fx.to, t);
        transform.translation += fx.rise * dt;
    }
}
