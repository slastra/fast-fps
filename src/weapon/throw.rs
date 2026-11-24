//! The grenade, on this machine. Press and hold to pull the pin and wind up;
//! the longer you hold (up to `GRENADE.charge_secs`), the harder it goes.
//! Release to throw. Left button throws overarm, right button lobs
//! underhand. A `Throw` then goes to the server, which simulates the grenade
//! and sets it off (`server::grenades`). You get one per life.

use bevy::prelude::*;

use super::arsenal::{Arsenal, ScriptedFire, Swing};
use crate::{
    net::{Respawn, SelectWeapon, Throw},
    player::{GameInput, ViewRig},
    sound::{Sounds, play},
    weapon::{GRENADE, WeaponKind},
};

/// Seconds after the release before switching back to the last weapon.
const FOLLOW_THROUGH: f32 = 0.4;
/// Where the throw animation picks up on release: the arm is already back.
const RELEASE_AT: f32 = 0.4;

#[allow(clippy::too_many_arguments)]
pub fn grenade(
    time: Res<Time>,
    input: Res<GameInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut arsenal: ResMut<Arsenal>,
    mut scripted: ResMut<ScriptedFire>,
    rig: Single<&GlobalTransform, With<ViewRig>>,
    mut throws: MessageWriter<Throw>,
    mut select: MessageWriter<SelectWeapon>,
    mut respawns: MessageReader<Respawn>,
    mut commands: Commands,
    sounds: Res<Sounds>,
) {
    let dt = time.delta_secs();
    let slot = WeaponKind::Grenade.index();
    if respawns.read().count() > 0 {
        arsenal.ammo[slot] = WeaponKind::Grenade.def().mag;
    }
    if arsenal.current != WeaponKind::Grenade {
        return;
    }

    // A scripted click is a full-strength throw: hold it for a full charge.
    if scripted.primary > 0 && arsenal.charge.is_none() && scripted.hold <= 0.0 {
        scripted.primary -= 1;
        scripted.hold = GRENADE.charge_secs + 0.05;
    }
    if scripted.alt > 0 && arsenal.charge.is_none() && scripted.hold_alt <= 0.0 {
        scripted.alt -= 1;
        scripted.hold_alt = GRENADE.charge_secs + 0.05;
    }
    let left = scripted.hold > 0.0 || (input.enabled && mouse.pressed(MouseButton::Left));
    let right = scripted.hold_alt > 0.0 || (input.enabled && mouse.pressed(MouseButton::Right));
    scripted.hold = (scripted.hold - dt).max(0.0);
    scripted.hold_alt = (scripted.hold_alt - dt).max(0.0);

    // Nothing left in hand: go back to what you were holding.
    if arsenal.ammo[slot] == 0 {
        if arsenal.cooldown <= 0.0 {
            let last = match arsenal.last() {
                WeaponKind::Grenade => WeaponKind::Ak,
                other => other,
            };
            if arsenal.select(last) {
                select.write(SelectWeapon(last));
            }
        }
        return;
    }

    match arsenal.charge {
        // Start charging when a button goes down.
        None => {
            if (left || right) && arsenal.draw <= 0.0 && arsenal.cooldown <= 0.0 {
                arsenal.charge = Some((0.0, !left));
                play(&mut commands, &sounds.pin, 0.5);
            }
        }
        // Keep charging while that button is held; throw on release.
        Some((held, lob)) => {
            let holding = if lob { right } else { left };
            if holding {
                arsenal.charge = Some((held + dt, lob));
                return;
            }
            arsenal.charge = None;
            arsenal.ammo[slot] = 0;
            arsenal.cooldown = FOLLOW_THROUGH;
            arsenal.start_swing(Swing::Throw);
            arsenal.swing_t = RELEASE_AT;
            let strength = GRENADE.strength(held);
            throws.write(Throw {
                lob,
                strength,
                origin: rig.translation(),
                dir: rig.forward().as_vec3(),
            });
            play(&mut commands, &sounds.shots[slot], 0.3 + 0.4 * strength);
        }
    }
}
