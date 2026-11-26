//! Survival in the world: the cracks in the walls, ammo boxes and medkits,
//! a glow on zombies that are burning, chilled or frozen, and the brief
//! flashes the server sends (shock arcs, bursts, burning ground, dust,
//! healing). All of it is for looks; none of it touches the level's
//! collision.

use bevy::{light::NotShadowCaster, prelude::*};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{
    Me,
    cracks::CRACKS,
    protocol::{BURNING, CHILLED, FROZEN, Flash, Pickup, PickupKind},
};
use crate::{net::Mob, render::no_shadow, world::BREACH_SIZE};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Flashes>()
        .add_systems(Startup, (make_assets, make_cracks).chain())
        .add_observer(dress_pickup)
        .add_observer(add_aura)
        .add_systems(Update, (show_cracks, spin_pickups, auras, flashes, age_fx));
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
                // Broken edges: stones jutting into the hole along its sides
                // and top, on both faces, so it isn't a clean rectangle.
                for face in [0.08, -WALL - 0.08] {
                    for side in [-1.0, 1.0] {
                        let mut y = rng.random_range(0.05..0.3);
                        while y < h - 0.1 {
                            let size = Vec3::new(
                                rng.random_range(0.18..0.42),
                                rng.random_range(0.16..0.4),
                                rng.random_range(0.16..0.3),
                            );
                            let x = side * (w / 2.0 - rng.random_range(0.02..0.16));
                            stone(
                                c,
                                Vec3::new(x, y + size.y / 2.0, face),
                                size,
                                rng.random_range(-0.4..0.4),
                                rng.random_bool(0.4),
                            );
                            y += size.y * rng.random_range(0.7..1.1);
                        }
                    }
                    let mut x = -w / 2.0;
                    while x < w / 2.0 {
                        let size = Vec3::new(
                            rng.random_range(0.2..0.45),
                            rng.random_range(0.14..0.34),
                            rng.random_range(0.16..0.3),
                        );
                        let y = h - size.y / 2.0 + rng.random_range(-0.12..0.06);
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
                // Rubble on both sides of the wall.
                for (z0, z1) in [(0.2, 1.4), (-WALL - 1.2, -WALL - 0.2)] {
                    for _ in 0..7 {
                        let s = rng.random_range(0.12..0.34);
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
