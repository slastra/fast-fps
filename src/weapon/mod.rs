//! Weapons: switching, Counter-Strike style spray patterns and accuracy,
//! firing, reloading, the sniper scope, and the first-person view model.
//!
//! How a shot is aimed:
//! 1. Start from where the view rig points (your mouse aim, without recoil).
//! 2. Offset by the weapon's spray pattern for this shot in the spray.
//! 3. Add a random offset inside the inaccuracy cone, which grows with
//!    movement, jumping and sustained fire and shrinks when crouched.
//!
//! The view itself is punched by half of the pattern, so bullets land beyond
//! the crosshair while spraying and you control it by pulling down.
//!
//! Shots are predicted locally (tracers, holes, sound) and sent to the server,
//! which does the authoritative ray cast with the same directions.

mod defs;
pub mod fx;
mod models;

pub use defs::{WeaponDef, WeaponKind};

use avian3d::prelude::*;
use bevy::{input::mouse::MouseWheel, prelude::*};
use bevy_ahoy::prelude::*;
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{
    Layer,
    console::{ConsoleAppExt, ConsoleCommand, Print},
    net::{Fire, HitConfirm, Loadout, SelectWeapon},
    player::{GameInput, LocalPlayer, ViewRig},
    render::{WorldCamera, no_shadow},
    sound::{Sounds, play},
};
use fx::{Decals, FxAssets, FxRng};

/// The gun is modeled at full size, then shrunk toward the eye by this factor.
/// Scaling about the eye doesn't change how it looks, but it keeps the whole
/// gun inside the character's 0.4 m collision radius, so it never pokes
/// through walls and needs no separate view-model camera.
const VM_SCALE: f32 = 0.45;
const RANGE: f32 = 250.0;
/// Fraction of the spray pattern shown as view punch.
const PUNCH_SCALE: f32 = 0.5;

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(fx::plugin)
        .init_resource::<Arsenal>()
        .init_resource::<HitMarker>()
        .init_resource::<ScriptedFire>()
        .init_resource::<Zoom>()
        .add_systems(PostStartup, build_view_models)
        .add_systems(
            Update,
            (
                weapon_commands,
                switch_weapon,
                zoom_input,
                update_accuracy,
                fire,
                reload,
                recover,
                animate_gun,
                hit_confirms,
            )
                .chain(),
        )
        .add_systems(Update, show_avatar_guns);

    app.console_command("weapon", "<ak|mp5|glock|shotgun|sniper>", "switch weapons")
        .console_command("zoom", "<level>", "scope zoom level (sniper only)");
}

/// Shots queued by the `fire` console command (for scripted tests).
#[derive(Resource, Default)]
pub struct ScriptedFire(pub u32);

/// Set by a server hit confirmation; the HUD fades it out.
#[derive(Resource, Default)]
pub struct HitMarker {
    pub time: f32,
    pub kill: bool,
}

/// Scales `tan(fov / 2)`: 1.0 unzoomed, smaller when scoped. The view
/// settings in `player.rs` apply it to the camera and the mouse.
#[derive(Resource)]
pub struct Zoom(pub f32);

impl Default for Zoom {
    fn default() -> Self {
        Self(1.0)
    }
}

#[derive(Resource)]
pub struct Arsenal {
    pub current: WeaponKind,
    last: WeaponKind,
    pub ammo: [u32; 5],
    /// Seconds of reload left for the current weapon.
    pub reloading: f32,
    /// Seconds until a freshly drawn weapon can fire.
    pub draw: f32,
    cooldown: f32,
    /// Position in the spray pattern (fractional while recovering).
    spray: f32,
    since_shot: f32,
    /// Decaying punch from single-shot weapons, degrees.
    kick_punch: Vec2,
    /// Current inaccuracy cone half-angle, degrees. The crosshair shows this.
    pub inaccuracy: f32,
    /// Zoom level index; 0 is unscoped.
    pub zoom: usize,
    buffered_click: f32,
    kick: f32,
    flash: f32,
    bob: f32,
    since_pump: f32,
    rng: StdRng,
}

impl Default for Arsenal {
    fn default() -> Self {
        Self {
            current: WeaponKind::Ak,
            last: WeaponKind::Glock,
            ammo: WeaponKind::ALL.map(|k| k.def().mag),
            reloading: 0.0,
            draw: 0.0,
            cooldown: 0.0,
            spray: 0.0,
            since_shot: 10.0,
            kick_punch: Vec2::ZERO,
            inaccuracy: 0.0,
            zoom: 0,
            buffered_click: 0.0,
            kick: 0.0,
            flash: 0.0,
            bob: 0.0,
            since_pump: 10.0,
            rng: StdRng::seed_from_u64(1),
        }
    }
}

impl Arsenal {
    pub fn def(&self) -> &'static WeaponDef {
        self.current.def()
    }

    pub fn scoped(&self) -> bool {
        self.zoom > 0
    }

    fn select(&mut self, kind: WeaponKind) -> bool {
        if kind == self.current {
            return false;
        }
        self.last = self.current;
        self.current = kind;
        self.draw = kind.def().draw_secs;
        self.reloading = 0.0;
        self.zoom = 0;
        self.spray = 0.0;
        self.kick = 0.0;
        true
    }
}

/// Yaw/pitch offset in degrees as a rotation (x right, y up).
fn offset(x_deg: f32, y_deg: f32) -> Quat {
    Quat::from_euler(EulerRot::YXZ, -x_deg.to_radians(), y_deg.to_radians(), 0.0)
}

/// Uniform random point in the unit disc.
fn disc(rng: &mut StdRng) -> Vec2 {
    let r = rng.random::<f32>().sqrt();
    let a = rng.random_range(0.0..std::f32::consts::TAU);
    Vec2::new(a.cos(), a.sin()) * r
}

// ---------------------------------------------------------------------------
// View models
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct GunRoot;

#[derive(Component)]
struct ViewGun(WeaponKind);

#[derive(Component)]
struct PumpPart(Vec3);

#[derive(Component)]
struct MuzzleFlash;

#[derive(Component)]
struct MuzzleLight;

/// Where each weapon's muzzle sits relative to the eye (already scaled).
#[derive(Resource)]
struct Muzzles([Vec3; 5]);

fn build_view_models(
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

    let mut muzzles = [Vec3::ZERO; 5];
    let mut guns = Vec::new();
    for kind in WeaponKind::ALL {
        let model = models::model(kind);
        muzzles[kind.index()] = (model.rest + model.muzzle) * VM_SCALE;
        guns.push((
            kind,
            model.rest,
            models::build(&model, &mut meshes, &mut rng),
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
        let model = models::model(kind);
        let parts = models::build(&model, meshes, rng);
        parent
            .spawn((
                AvatarGun(kind),
                Transform::from_xyz(0.24, -0.22, -0.18),
                if kind == WeaponKind::Ak {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ))
            .with_children(|gun| {
                for (mesh, transform, _) in parts {
                    gun.spawn((mesh, material.clone(), transform));
                }
            });
    }
}

fn show_avatar_guns(
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

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

fn weapon_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut arsenal: ResMut<Arsenal>,
    mut select: MessageWriter<SelectWeapon>,
    mut print: MessageWriter<Print>,
) {
    for cmd in cmds.read() {
        if cmd.name == "zoom" {
            let levels = arsenal.def().zoom.len();
            arsenal.zoom = cmd
                .arg(0)
                .and_then(|z| z.parse::<usize>().ok())
                .unwrap_or(0)
                .min(levels);
            continue;
        }
        if cmd.name != "weapon" {
            continue;
        }
        match cmd.arg(0).and_then(WeaponKind::parse) {
            Some(kind) => {
                if arsenal.select(kind) {
                    select.write(SelectWeapon(kind));
                }
            }
            None => {
                print.write(Print::new("usage: weapon <ak|mp5|glock|shotgun|sniper>"));
            }
        }
    }
}

fn switch_weapon(
    input: Res<GameInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    mut arsenal: ResMut<Arsenal>,
    mut select: MessageWriter<SelectWeapon>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    let scroll: f32 = wheel.read().map(|w| w.y).sum();
    if !input.enabled {
        return;
    }
    let slots = [
        (KeyCode::Digit1, WeaponKind::Ak),
        (KeyCode::Digit2, WeaponKind::Mp5),
        (KeyCode::Digit3, WeaponKind::Glock),
        (KeyCode::Digit4, WeaponKind::Shotgun),
        (KeyCode::Digit5, WeaponKind::Sniper),
    ];
    let mut wanted = slots
        .iter()
        .find(|(key, _)| keys.just_pressed(*key))
        .map(|(_, kind)| *kind);
    if keys.just_pressed(KeyCode::KeyQ) {
        wanted = Some(arsenal.last);
    }
    if scroll != 0.0 {
        let i = arsenal.current.index() as i32 + if scroll < 0.0 { 1 } else { -1 };
        wanted = Some(WeaponKind::ALL[i.rem_euclid(5) as usize]);
    }
    if let Some(kind) = wanted
        && arsenal.select(kind)
    {
        select.write(SelectWeapon(kind));
        play(&mut commands, &sounds.click, 0.4);
    }
}

fn zoom_input(
    input: Res<GameInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut arsenal: ResMut<Arsenal>,
    mut zoom: ResMut<Zoom>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    let levels = arsenal.def().zoom;
    if input.enabled
        && mouse.just_pressed(MouseButton::Right)
        && !levels.is_empty()
        && arsenal.reloading <= 0.0
    {
        arsenal.zoom = (arsenal.zoom + 1) % (levels.len() + 1);
        play(&mut commands, &sounds.zoom, 0.35);
    }
    let target = if arsenal.zoom == 0 {
        1.0
    } else {
        levels[arsenal.zoom - 1]
    };
    zoom.set_if_neq(Zoom(target));
}

impl PartialEq for Zoom {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

// ---------------------------------------------------------------------------
// Accuracy, firing, recoil
// ---------------------------------------------------------------------------

fn update_accuracy(
    time: Res<Time>,
    mut arsenal: ResMut<Arsenal>,
    player: Single<(&LinearVelocity, &CharacterControllerState), With<LocalPlayer>>,
) {
    let dt = time.delta_secs();
    let (velocity, state) = *player;
    let def = arsenal.def();
    let speed = velocity.0.with_y(0.0).length();

    let mut cone = if arsenal.scoped() {
        def.scoped
    } else {
        def.stand
    };
    cone += def.moving * ((speed - 1.5) / 7.5).clamp(0.0, 1.0);
    if state.grounded.is_none() {
        cone += def.airborne;
    }
    if state.crouching {
        cone *= def.crouch_mult;
    }
    cone += (arsenal.spray * def.per_shot).min(def.per_shot_max);

    // Accuracy is lost quickly and regained a little slower.
    let rate = if cone > arsenal.inaccuracy { 30.0 } else { 9.0 };
    arsenal.inaccuracy.smooth_nudge(&cone, rate, dt);

    arsenal.bob += dt * (4.0 + speed * 0.8);
}

fn fire(
    time: Res<Time>,
    input: Res<GameInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut arsenal: ResMut<Arsenal>,
    mut scripted: ResMut<ScriptedFire>,
    rig: Single<&GlobalTransform, With<ViewRig>>,
    camera: Single<&GlobalTransform, With<WorldCamera>>,
    muzzles: Res<Muzzles>,
    spatial: SpatialQuery,
    mut shots: MessageWriter<Fire>,
    mut commands: Commands,
    fx: Res<FxAssets>,
    mut fx_rng: ResMut<FxRng>,
    mut decals: ResMut<Decals>,
    sounds: Res<Sounds>,
) {
    let dt = time.delta_secs();
    arsenal.cooldown -= dt;
    arsenal.draw = (arsenal.draw - dt).max(0.0);
    arsenal.buffered_click -= dt;
    arsenal.since_pump += dt;

    let def = arsenal.def();
    let clicked = input.enabled && mouse.just_pressed(MouseButton::Left);
    if clicked {
        arsenal.buffered_click = 0.15;
    }
    let scripted_shot = scripted.0 > 0;
    let held = input.enabled && mouse.pressed(MouseButton::Left);
    let want = scripted_shot
        || if def.automatic {
            held
        } else {
            arsenal.buffered_click > 0.0
        };
    if !want || arsenal.reloading > 0.0 || arsenal.draw > 0.0 || arsenal.cooldown > 0.0 {
        return;
    }
    scripted.0 = scripted.0.saturating_sub(1);
    arsenal.buffered_click = 0.0;
    let kind = arsenal.current;
    if arsenal.ammo[kind.index()] == 0 {
        if clicked || scripted_shot {
            play(&mut commands, &sounds.click, 0.5);
        }
        return;
    }
    arsenal.ammo[kind.index()] -= 1;
    arsenal.cooldown = def.interval;
    arsenal.flash = 0.05;
    arsenal.since_pump = 0.0;
    arsenal.kick = (arsenal.kick
        + if def.pellets > 1 || def.kick > 2.0 {
            1.0
        } else {
            0.35
        })
    .min(1.0);

    // Pattern offset for this shot, then random spread per pellet.
    let (px, py) = def.pattern_at(arsenal.spray);
    let aim = rig.rotation() * offset(px, py);
    let cone = arsenal.inaccuracy;
    let mut dirs = Vec::with_capacity(def.pellets as usize);
    for _ in 0..def.pellets {
        let mut off = disc(&mut arsenal.rng) * cone;
        if def.pellets > 1 {
            off += disc(&mut arsenal.rng) * def.pellet_spread;
        }
        dirs.push(aim * offset(off.x, off.y) * Vec3::NEG_Z);
    }
    let origin = rig.translation();

    // Local prediction with the exact directions the server will use.
    let muzzle = camera.transform_point(muzzles.0[kind.index()]);
    let filter = SpatialQueryFilter::from_mask(Layer::World);
    for dir in &dirs {
        let end = match Dir3::new(*dir)
            .ok()
            .and_then(|d| spatial.cast_ray(origin, d, RANGE, true, &filter))
        {
            Some(hit) => {
                let end = origin + *dir * hit.distance;
                fx::impact(
                    &mut commands,
                    &fx,
                    &mut decals,
                    end,
                    hit.normal,
                    false,
                    &mut fx_rng.0,
                );
                end
            }
            None => origin + *dir * RANGE,
        };
        fx::tracer(&mut commands, &fx, muzzle, end, kind);
    }
    shots.write(Fire {
        weapon: kind,
        origin,
        dirs,
    });

    arsenal.spray = (arsenal.spray + 1.0).min(def.mag as f32);
    arsenal.since_shot = 0.0;
    if def.kick > 0.0 {
        let side = arsenal.rng.random_range(-0.3..0.3);
        arsenal.kick_punch += Vec2::new(side, 1.0) * def.kick;
    }
    play(&mut commands, &sounds.shots[kind.index()], 0.55);
}

fn reload(
    time: Res<Time>,
    input: Res<GameInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut arsenal: ResMut<Arsenal>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    let kind = arsenal.current;
    let def = kind.def();
    if arsenal.reloading > 0.0 {
        arsenal.reloading -= time.delta_secs();
        if arsenal.reloading <= 0.0 {
            arsenal.reloading = 0.0;
            arsenal.ammo[kind.index()] = def.mag;
            play(&mut commands, &sounds.click, 0.6);
        }
        return;
    }
    let empty = arsenal.ammo[kind.index()] == 0;
    let wants =
        keys.just_pressed(KeyCode::KeyR) || (empty && mouse.just_pressed(MouseButton::Left));
    if input.enabled && wants && arsenal.ammo[kind.index()] < def.mag && arsenal.draw <= 0.0 {
        arsenal.reloading = def.reload_secs;
        arsenal.zoom = 0;
        arsenal.spray = 0.0;
        play(&mut commands, &sounds.reload, 0.6);
    }
}

/// Spray recovery and view punch.
fn recover(
    time: Res<Time>,
    mut arsenal: ResMut<Arsenal>,
    mut camera: Single<&mut Transform, With<WorldCamera>>,
) {
    let dt = time.delta_secs();
    let def = arsenal.def();
    arsenal.since_shot += dt;
    if arsenal.since_shot > def.interval * 1.6 {
        let rate = (def.mag.max(10) as f32) / def.recover_secs;
        arsenal.spray = (arsenal.spray - rate * dt).max(0.0);
    }
    let mut kick = arsenal.kick_punch;
    kick.smooth_nudge(&Vec2::ZERO, 9.0, dt);
    arsenal.kick_punch = kick;

    let (px, py) = def.pattern_at(arsenal.spray);
    let punch = Vec2::new(px, py) * PUNCH_SCALE + arsenal.kick_punch;
    camera.rotation = offset(punch.x, punch.y);
}

fn animate_gun(
    time: Res<Time>,
    mut arsenal: ResMut<Arsenal>,
    player: Single<&LinearVelocity, With<LocalPlayer>>,
    muzzles: Res<Muzzles>,
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
    let model_rest = models::model(kind).rest;
    let moving = (player.0.with_y(0.0).length() / 8.0).clamp(0.0, 1.0);

    for (gun, mut visibility) in &mut guns {
        let want = if gun.0 == kind {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(want);
    }
    // Hide the gun while looking through the scope.
    root.1.set_if_neq(if arsenal.scoped() {
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

fn hit_confirms(
    mut confirms: MessageReader<HitConfirm>,
    mut marker: ResMut<HitMarker>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    for confirm in confirms.read() {
        marker.time = if confirm.kill { 0.45 } else { 0.18 };
        marker.kill = confirm.kill;
        let sound = if confirm.kill {
            &sounds.kill
        } else {
            &sounds.hit
        };
        play(&mut commands, sound, if confirm.head { 0.7 } else { 0.5 });
    }
}
