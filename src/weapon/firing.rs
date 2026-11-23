//! Pulling the trigger: the accuracy cone, spray patterns, the shot itself,
//! reloading, and recoil recovery.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_ahoy::prelude::*;
use rand::{Rng, rngs::StdRng};

use super::fx::{self, Decals, FxAssets, FxRng};
use crate::{
    config::SHOT_RANGE,
    net::{Fire, HitConfirm},
    physics::Layer,
    player::{GameInput, LocalPlayer, ViewRig},
    render::WorldCamera,
    sound::{Sounds, play},
    weapon::{
        PUNCH_SCALE,
        arsenal::{Arsenal, HitMarker, ScriptedFire},
        view_model::Muzzles,
    },
};

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

pub fn update_accuracy(
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

pub fn fire(
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
            .and_then(|d| spatial.cast_ray(origin, d, SHOT_RANGE, true, &filter))
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
            None => origin + *dir * SHOT_RANGE,
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

pub fn reload(
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
pub fn recover(
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

pub fn hit_confirms(
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
