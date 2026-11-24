//! The grenade, on this machine: left click throws, right click lobs. The arm
//! winds up for a moment, then a `Throw` goes to the server, which simulates
//! the grenade and sets it off (`server::grenades`). You get one per life.

use bevy::prelude::*;

use super::arsenal::{Arsenal, ScriptedFire, Swing};
use crate::{
    net::{Respawn, SelectWeapon, Throw},
    player::{GameInput, ViewRig},
    sound::{Sounds, play},
    weapon::WeaponKind,
};

/// Seconds from the click to the grenade leaving the hand.
const WINDUP: f32 = 0.25;
/// Seconds after the release before switching back to the last weapon.
const FOLLOW_THROUGH: f32 = 0.4;

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
    let slot = WeaponKind::Grenade.index();
    if respawns.read().count() > 0 {
        arsenal.ammo[slot] = WeaponKind::Grenade.def().mag;
    }
    if arsenal.current != WeaponKind::Grenade {
        return;
    }

    // Release a throw that has finished winding up.
    if let Some((left, lob)) = arsenal.windup {
        let left = left - time.delta_secs();
        if left > 0.0 {
            arsenal.windup = Some((left, lob));
            return;
        }
        arsenal.windup = None;
        arsenal.ammo[slot] = 0;
        arsenal.cooldown = FOLLOW_THROUGH;
        throws.write(Throw {
            lob,
            origin: rig.translation(),
            dir: rig.forward().as_vec3(),
        });
        return;
    }
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

    let throw = scripted.primary > 0 || (input.enabled && mouse.just_pressed(MouseButton::Left));
    let lob = scripted.alt > 0 || (input.enabled && mouse.just_pressed(MouseButton::Right));
    if !(throw || lob) || arsenal.draw > 0.0 || arsenal.cooldown > 0.0 {
        return;
    }
    if throw {
        scripted.primary = scripted.primary.saturating_sub(1);
    } else {
        scripted.alt = scripted.alt.saturating_sub(1);
    }
    arsenal.windup = Some((WINDUP, !throw));
    arsenal.start_swing(Swing::Throw);
    play(&mut commands, &sounds.shots[slot], 0.5);
}
