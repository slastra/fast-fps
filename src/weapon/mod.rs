//! Weapons: switching, Counter-Strike style spray patterns and accuracy,
//! firing, reloading, the sniper scope, the knife, the grenade, and the
//! first-person view model.
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
//!
//! - [`defs`]: stats and spray patterns. Add or tune guns here.
//! - [`models`]: what each gun looks like.
//! - [`arsenal`]: what you're holding, switching, and the scope.
//! - [`firing`]: accuracy, shooting, reloading and recoil.
//! - [`knife`]: slashes and stabs.
//! - [`throw`]: throwing the grenade.
//! - [`view_model`]: the gun on screen and in other players' hands.
//! - [`fx`]: tracers, sparks and bullet holes.
//! - [`blast`]: grenades in flight and their explosions.

mod arsenal;
mod blast;
mod defs;
mod firing;
pub mod fx;
mod icons;
mod knife;
mod models;
mod throw;
mod view_model;

pub use arsenal::{Arsenal, HideViewModel, HitMarker, ScriptedFire, Zoom};
pub use blast::Explosions;
pub use defs::{Class, GRENADE, KNIFE, WeaponKind};
pub use icons::{ICON_ASPECT, WeaponIcons};
pub use view_model::{muzzle, spawn_avatar_guns, spawn_gun};

use bevy::prelude::*;

use crate::{
    console::ConsoleAppExt,
    weapon::{
        arsenal::{switch_weapon, weapon_commands, zoom_input},
        firing::{fire, hit_confirms, recover, reload, update_accuracy},
        knife::knife,
        throw::grenade,
        view_model::{animate_gun, build_view_models, show_avatar_guns},
    },
};

/// The gun is modeled at full size, then shrunk toward the eye by this factor.
/// Scaling about the eye doesn't change how it looks, but it keeps the whole
/// gun inside the character's 0.4 m collision radius, so it never pokes
/// through walls and needs no separate view-model camera.
const VM_SCALE: f32 = 0.45;
/// Fraction of the spray pattern shown as view punch.
const PUNCH_SCALE: f32 = 0.5;

pub(super) fn plugin(app: &mut App) {
    app.add_plugins((fx::plugin, icons::plugin, blast::plugin))
        .init_resource::<Arsenal>()
        .init_resource::<HitMarker>()
        .init_resource::<ScriptedFire>()
        .init_resource::<HideViewModel>()
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
                knife,
                grenade,
                reload,
                recover,
                animate_gun,
                hit_confirms,
            )
                .chain(),
        )
        .add_systems(Update, show_avatar_guns);

    app.console_command(
        "weapon",
        "<ak|mp5|glock|shotgun|sniper|knife|grenade>",
        "switch weapons",
    )
    .console_command("zoom", "<level>", "scope zoom level (sniper only)")
    .console_command("reload", "", "reload (also R)");
}
