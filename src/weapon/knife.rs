//! The knife, on this machine: left click slashes (hold to keep slashing),
//! right click stabs. The swing and its sound play at once; the server
//! decides what it hit (`server::combat::receive_melee`) and answers with the
//! usual hit confirmation.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{
    arsenal::{Arsenal, ScriptedFire, Swing},
    fx::{self, Decals, FxAssets, FxRng},
};
use crate::{
    net::Melee,
    physics::Layer,
    player::{GameInput, ViewRig},
    sound::{Sounds, play},
    weapon::{KNIFE, WeaponKind},
};

#[allow(clippy::too_many_arguments)]
pub fn knife(
    input: Res<GameInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut arsenal: ResMut<Arsenal>,
    mut scripted: ResMut<ScriptedFire>,
    rig: Single<&GlobalTransform, With<ViewRig>>,
    spatial: SpatialQuery,
    mut attacks: MessageWriter<Melee>,
    mut commands: Commands,
    fx: Res<FxAssets>,
    mut fx_rng: ResMut<FxRng>,
    mut decals: ResMut<Decals>,
    sounds: Res<Sounds>,
) {
    if arsenal.current != WeaponKind::Knife {
        return;
    }
    let slash = scripted.primary > 0 || (input.enabled && mouse.pressed(MouseButton::Left));
    let stab = scripted.alt > 0 || (input.enabled && mouse.pressed(MouseButton::Right));
    if !(slash || stab) || arsenal.draw > 0.0 || arsenal.cooldown > 0.0 {
        return;
    }
    // A slash wins if both are held: it is the quicker one.
    let heavy = !slash;
    if heavy {
        scripted.alt = scripted.alt.saturating_sub(1);
        arsenal.cooldown = KNIFE.stab_secs;
        arsenal.start_swing(Swing::Stab);
    } else {
        scripted.primary = scripted.primary.saturating_sub(1);
        arsenal.cooldown = KNIFE.slash_secs;
        // Alternate the direction of the sweep, like a real flurry.
        let side = match arsenal.swing {
            Swing::Slash(side) => -side,
            _ => 1.0,
        };
        arsenal.start_swing(Swing::Slash(side));
    }

    let origin = rig.translation();
    let dir = rig.forward();
    // Predict hitting the world for sparks and a sound now. Bodies are the
    // server's to decide; its hit confirmation plays the flesh sound.
    let wall = spatial.cast_shape(
        &Collider::sphere(KNIFE.width * 0.5),
        origin,
        Quat::IDENTITY,
        dir,
        &ShapeCastConfig::from_max_distance(KNIFE.reach),
        &SpatialQueryFilter::from_mask(Layer::World),
    );
    if let Some(hit) = wall {
        fx::impact(
            &mut commands,
            &fx,
            &mut decals,
            hit.point1,
            hit.normal1,
            false,
            &mut fx_rng.0,
        );
        play(&mut commands, &sounds.knife_wall, 0.4);
    }
    play(
        &mut commands,
        &sounds.shots[WeaponKind::Knife.index()],
        if heavy { 0.6 } else { 0.45 },
    );
    attacks.write(Melee {
        heavy,
        origin,
        dir: dir.as_vec3(),
    });
}
