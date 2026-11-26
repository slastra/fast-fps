//! The map and the things standing in it.
//!
//! Everything is generated from a fixed seed so every peer builds the exact
//! same colliders, which keeps server-side hit scans honest.
//!
//! - [`arena`]: the layout. Edit this to change the map.
//! - [`builder`]: helpers for placing blocks, ramps, rocks and trees.
//! - [`lights`]: lamps and the campfire.
//! - [`dummy`]: how practice dummies look.

mod arena;
mod builder;
pub mod dummy;
pub mod lights;

pub use arena::{BREACH_SIZE, Breach, DUMMIES, SPAWNS, breach, breach_courses};

use bevy::prelude::*;

use crate::dedicated::Dedicated;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, arena::spawn_level);
    // Flickering lamps and dressed-up dummies are for looking at. A
    // dedicated server only needs the colliders.
    if !app.world().contains_resource::<Dedicated>() {
        app.add_plugins((lights::plugin, dummy::plugin));
    }
}
