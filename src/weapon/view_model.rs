//! Gun models in the world: your first-person view model and the guns in
//! other players' hands.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::{SeedableRng, rngs::StdRng};

use crate::{
    net::Loadout,
    player::LocalPlayer,
    render::{WorldCamera, no_shadow},
    weapon::{
        VM_SCALE,
        arsenal::{Arsenal, HideViewModel},
        defs::WeaponKind,
    },
};

#[derive(Component)]
pub struct GunRoot;

#[derive(Component)]
pub struct ViewGun(WeaponKind);

#[derive(Component)]
pub struct PumpPart(Vec3);

#[derive(Component)]
pub struct MuzzleFlash;

#[derive(Component)]
pub struct MuzzleLight;

/// Where each weapon's muzzle sits relative to the eye (already scaled).
#[derive(Resource)]
pub struct Muzzles(pub [Vec3; WeaponKind::COUNT]);

pub fn build_view_models(
    mut commands: Commands,
    camera: Single<Entity, With<WorldCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = StdRng::seed_from_u64(42);
    let material = MeshMaterial3d(materials.add(StandardMaterial {
        perceptual_roughness: 0.55,
        metallic: 0.2,
        ..default()
    }));
    let flash_mat = MeshMaterial3d(materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.7, 0.3),
        emissive: LinearRgba::rgb(60.0, 30.0, 8.0),
        unlit: true,
        ..default()
    }));
    let flash_mesh = Mesh3d(
        meshes.add(
            Cone {
                radius: 0.06,
                height: 0.22,
            }
            .mesh()
            .resolution(5)
            .build(),
        ),
    );

    let mut muzzles = [Vec3::ZERO; WeaponKind::COUNT];
    let mut guns = Vec::new();
    for kind in WeaponKind::ALL {
        let model = super::models::model(kind);
        muzzles[kind.index()] = (model.rest + model.muzzle) * VM_SCALE;
        guns.push((
            kind,
            model.rest,
            super::models::build(&model, &mut meshes, &mut rng),
        ));
    }
    commands.insert_resource(Muzzles(muzzles));

    commands.entity(*camera).with_children(|cam| {
        cam.spawn((
            GunRoot,
            Transform::from_translation(guns[0].1 * VM_SCALE).with_scale(Vec3::splat(VM_SCALE)),
            Visibility::default(),
        ))
        .with_children(|root| {
            for (kind, _, parts) in guns {
                root.spawn((ViewGun(kind), Transform::default(), Visibility::Hidden))
                    .with_children(|gun| {
                        for (mesh, transform, part) in parts {
                            let mut e = gun.spawn((mesh, material.clone(), transform, no_shadow()));
                            if part.pump {
                                e.insert(PumpPart(transform.translation));
                            }
                        }
                    });
            }
            root.spawn((
                MuzzleFlash,
                flash_mesh,
                flash_mat,
                Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::Hidden,
                no_shadow(),
            ));
            root.spawn((
                MuzzleLight,
                PointLight {
                    color: Color::srgb(1.0, 0.65, 0.3),
                    intensity: 0.0,
                    range: 12.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::default(),
            ));
        });
    });
}

pub fn animate_gun(
    time: Res<Time>,
    mut arsenal: ResMut<Arsenal>,
    player: Single<&LinearVelocity, With<LocalPlayer>>,
    muzzles: Res<Muzzles>,
    hidden: Res<HideViewModel>,
    mut root: Single<(&mut Transform, &mut Visibility), With<GunRoot>>,
    mut guns: Query<(&ViewGun, &mut Visibility), Without<GunRoot>>,
    mut pumps: Query<(&PumpPart, &mut Transform), Without<GunRoot>>,
    mut flash: Single<
        (&mut Visibility, &mut Transform),
        (
            With<MuzzleFlash>,
            Without<GunRoot>,
            Without<ViewGun>,
            Without<PumpPart>,
        ),
    >,
    mut light: Single<
        (&mut PointLight, &mut Transform),
        (
            With<MuzzleLight>,
            Without<GunRoot>,
            Without<MuzzleFlash>,
            Without<PumpPart>,
        ),
    >,
) {
    let dt = time.delta_secs().max(1e-4);
    let kind = arsenal.current;
    let def = kind.def();
    let model_rest = super::models::model(kind).rest;
    let moving = (player.0.with_y(0.0).length() / 8.0).clamp(0.0, 1.0);

    for (gun, mut visibility) in &mut guns {
        let want = if gun.0 == kind {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(want);
    }
    // Hide the gun while looking through the scope (or when asked to).
    root.1.set_if_neq(if arsenal.scoped() || hidden.0 {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });

    arsenal.kick = (arsenal.kick - dt * 3.5).max(0.0);
    let kick = arsenal.kick;
    let bob = Vec3::new(
        arsenal.bob.sin() * 0.012,
        (arsenal.bob * 2.0).sin().abs() * -0.01,
        0.0,
    ) * moving;
    let reload_t = if arsenal.reloading > 0.0 {
        (((1.0 - arsenal.reloading / def.reload_secs) * std::f32::consts::PI).sin()).max(0.0)
    } else {
        0.0
    };
    let draw_t = if def.draw_secs > 0.0 {
        arsenal.draw / def.draw_secs
    } else {
        0.0
    };
    let target = (model_rest + bob + Vec3::new(0.0, -0.12 * reload_t - 0.25 * draw_t, 0.05 * kick))
        * VM_SCALE;
    root.0.translation.smooth_nudge(&target, 30.0, dt);
    let rotation = Quat::from_rotation_x(0.08 * kick - 0.7 * reload_t - 0.9 * draw_t)
        * Quat::from_rotation_z(0.5 * reload_t);
    root.0.rotation.smooth_nudge(&rotation, 25.0, dt);

    // Shotgun pump: slides back and forward after each shot.
    let pump_t = ((arsenal.since_pump - 0.15) / 0.45).clamp(0.0, 1.0);
    let pump_offset = (pump_t * std::f32::consts::PI).sin() * 0.09;
    for (part, mut transform) in &mut pumps {
        transform.translation = part.0 + Vec3::Z * pump_offset;
    }

    // Muzzle flash at the current weapon's muzzle (in GunRoot space).
    let muzzle = muzzles.0[kind.index()] / VM_SCALE - model_rest;
    arsenal.flash -= dt;
    let lit = arsenal.flash > 0.0;
    flash.0.set_if_neq(if lit {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    flash.1.translation = muzzle + Vec3::Z * -0.1;
    light.1.translation = muzzle + Vec3::Z * -0.15;
    light.0.intensity = if lit { 60_000.0 } else { 0.0 };
}

/// Guns for other players' bodies: one of each, only the held one visible.
#[derive(Component)]
pub struct AvatarGun(WeaponKind);

pub fn spawn_avatar_guns(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    material: &MeshMaterial3d<StandardMaterial>,
    rng: &mut StdRng,
) {
    for kind in WeaponKind::ALL {
        parent
            .spawn((
                AvatarGun(kind),
                Transform::default(),
                if kind == WeaponKind::Ak {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ))
            .with_children(|gun| spawn_gun(gun, kind, meshes, material, rng));
    }
}

/// Spawn one full-size gun model's parts under `parent`.
pub fn spawn_gun(
    parent: &mut ChildSpawnerCommands,
    kind: WeaponKind,
    meshes: &mut Assets<Mesh>,
    material: &MeshMaterial3d<StandardMaterial>,
    rng: &mut StdRng,
) {
    let model = super::models::model(kind);
    for (mesh, transform, _) in super::models::build(&model, meshes, rng) {
        parent.spawn((mesh, material.clone(), transform));
    }
}

/// Where the muzzle is on a full-size gun model.
pub fn muzzle(kind: WeaponKind) -> Vec3 {
    super::models::model(kind).muzzle
}

pub fn show_avatar_guns(
    players: Query<(Entity, &Loadout), Changed<Loadout>>,
    children: Query<&Children>,
    mut guns: Query<(&AvatarGun, &mut Visibility)>,
) {
    for (player, loadout) in &players {
        for descendant in children.iter_descendants(player) {
            if let Ok((gun, mut visibility)) = guns.get_mut(descendant) {
                *visibility = if gun.0 == loadout.0 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}
