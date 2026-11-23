//! fast-fps: a small low-poly LAN shooter.
//!
//! Everything is driven from the `~` console. Launch arguments are console
//! commands too, Quake style: `fast-fps +name alice +host` or
//! `fast-fps +connect 192.168.1.20`.

mod avatar;
mod config;
mod console;
mod debug;
mod hud;
mod lowpoly;
mod net;
mod physics;
mod player;
mod render;
mod server;
mod sound;
mod weapon;
mod world;

use avian3d::prelude::*;
use bevy::{prelude::*, render::RenderPlugin, window::PresentMode};
use bevy_ahoy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::RepliconRenetPlugins;

fn main() -> AppExit {
    // Set by scripts/harness.sh. Shaders then compile before the frame that
    // needs them is drawn, so a test screenshot never catches a half-drawn
    // scene. Normal play compiles them in the background instead.
    let under_test = std::env::var_os("HARNESS").is_some();

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Window {
                        title: config::GAME_NAME.to_string(),
                        present_mode: PresentMode::AutoNoVsync,
                        ..default()
                    }
                    .into(),
                    ..default()
                })
                .set(RenderPlugin {
                    synchronous_pipeline_compilation: under_test,
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
            world::plugin,
            server::plugin,
            player::plugin,
            weapon::plugin,
            hud::plugin,
            debug::plugin,
            avatar::plugin,
        ))
        .run()
}
