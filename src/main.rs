//! fast-fps: a small low-poly LAN shooter.
//!
//! Everything is driven from the `~` console. Launch arguments are console
//! commands too, Quake style: `fast-fps +name alice +host` or
//! `fast-fps +connect 192.168.1.20`.

mod console;
mod debug;
mod game;
mod hud;
mod level;
mod lowpoly;
mod net;
mod player;
mod render;
mod sound;
mod weapon;

use avian3d::prelude::*;
use bevy::{prelude::*, window::PresentMode};
use bevy_ahoy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::RepliconRenetPlugins;

fn main() -> AppExit {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Window {
                    title: "fast-fps".to_string(),
                    present_mode: PresentMode::AutoNoVsync,
                    ..default()
                }
                .into(),
                ..default()
            }),
        )
        .add_plugins((
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugins::default(),
            RepliconPlugins,
            RepliconRenetPlugins,
        ))
        // Order matters: `net` registers the replication protocol, which has to be
        // identical on every peer, and the console has to exist before anyone
        // registers commands with it.
        .add_plugins((
            console::plugin,
            net::plugin,
            render::plugin,
            sound::plugin,
            level::plugin,
            game::plugin,
            player::plugin,
            weapon::plugin,
            hud::plugin,
            debug::plugin,
        ))
        .run()
}

/// Physics layers. The local character controller only collides with `World`,
/// so other players never block movement; hit scans test `World | Hitbox`.
#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum Layer {
    #[default]
    World,
    /// The character controller you are driving on this machine.
    LocalBody,
    /// Server-side player and dummy hitboxes.
    Hitbox,
}
