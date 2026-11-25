//! A small low-poly LAN shooter, and a starting point for your own game.
//!
//! Everything is driven from the `~` console. Launch arguments are console
//! commands too, Quake style: `cargo run -- +name alice +host` or
//! `cargo run -- +connect 192.168.1.20`.
//!
//! This file only assembles the app. `docs/tour.md` maps the rest.

mod avatar;
mod config;
mod console;
mod debug;
mod dedicated;
mod duel;
mod hud;
mod lowpoly;
mod net;
mod physics;
mod player;
mod render;
mod server;
mod sky;
mod sound;
mod weapon;
mod world;

use avian3d::prelude::*;
use std::time::Duration;

use bevy::{
    app::ScheduleRunnerPlugin,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    window::{ExitCondition, PresentMode},
};
use bevy_ahoy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet2::RepliconRenetPlugins;

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let key = std::env::var("FASTFPS_KEY").ok();
    match dedicated::Dedicated::from_args(&args, key.as_deref()) {
        Ok(None) => game(),
        Ok(Some(options)) => dedicated_server(options),
        Err(e) => {
            eprintln!("{e}");
            AppExit::error()
        }
    }
}

/// The game you play: a window, a renderer, sound, and the server code for
/// offline play and hosting.
fn game() -> AppExit {
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
            sky::plugin,
            sound::plugin,
            world::plugin,
            server::plugin,
            player::plugin,
            weapon::plugin,
            hud::plugin,
            debug::plugin,
            avatar::plugin,
            duel::plugin,
        ))
        .run()
}

/// A headless server (`--dedicated`, see `dedicated.rs`): the same world and
/// server code, with no window, GPU, sound or local player.
fn dedicated_server(options: dedicated::Dedicated) -> AppExit {
    App::new()
        // First, so every plugin can see this is a dedicated server.
        .insert_resource(options)
        .add_plugins(
            DefaultPlugins
                .build()
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::audio::AudioPlugin>()
                .disable::<bevy::gilrs::GilrsPlugin>()
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                // The mesh and material types still exist, so the level is
                // built exactly as in the game, but nothing touches a GPU.
                .set(RenderPlugin {
                    render_creation: WgpuSettings {
                        backends: None,
                        ..default()
                    }
                    .into(),
                    ..default()
                }),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / config::DEDICATED_TICK_HZ,
        )))
        .add_plugins((
            PhysicsPlugins::default(),
            RepliconPlugins,
            RepliconRenetPlugins,
        ))
        // The same order as the game, so the protocol matches.
        .add_plugins((
            console::plugin,
            net::plugin,
            world::plugin,
            server::plugin,
            dedicated::plugin,
        ))
        .run()
}
