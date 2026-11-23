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

pub use arena::{DUMMIES, SPAWNS};

use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, arena::spawn_level)
        .add_plugins((lights::plugin, dummy::plugin));
}
