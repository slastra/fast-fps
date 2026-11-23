//! Console commands for testing and the scripted harness.
//!
//! - this file: `screenshot`, and putting the player and camera somewhere
//!   exact (`tp`, `look`, `spin`, `fire`).
//! - [`switches`]: `r_*` switches for each render feature.
//! - [`probe`]: `state`, `assert` and `wait_until`, which let a script check
//!   what the game is actually doing.
//!
//! See `docs/harness.md`, `scripts/harness.sh` and `tests/harness/*.cfg`.

pub mod probe;
mod switches;

use std::path::PathBuf;

use avian3d::prelude::*;
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};

use crate::{
    console::{ConsoleAppExt, ConsoleCommand, Print, ScriptHold},
    player::{LocalPlayer, ViewRig},
    weapon::ScriptedFire,
};

pub(super) fn plugin(app: &mut App) {
    // Camera writes happen in PreUpdate: ahoy copies the camera rotation into
    // the character at the start of the fixed loop and back out in Update, so
    // anything written later in the frame is lost.
    app.init_resource::<Spin>()
        .init_resource::<PendingLook>()
        .add_systems(Update, scene_commands)
        .add_systems(PreUpdate, spin)
        .add_plugins((switches::plugin, probe::plugin));

    app.console_command(
        "screenshot",
        "<name>",
        "save a PNG (into $SCREENSHOT_DIR if set)",
    )
    .console_command("tp", "<x> <y> <z>", "teleport your character")
    .console_command("look", "<yaw> <pitch>", "set view angles in degrees")
    .console_command("spin", "<deg/s>", "turn steadily; 0 stops")
    .console_command("fire", "[shots]", "pull the trigger from a script");
}

#[derive(Resource, Default)]
struct Spin(f32);

#[derive(Resource, Default)]
struct PendingLook(Option<Quat>);

fn shot_path(name: &str) -> PathBuf {
    let name = if name.ends_with(".png") {
        name.to_string()
    } else {
        format!("{name}.png")
    };
    match std::env::var_os("SCREENSHOT_DIR") {
        Some(dir) => PathBuf::from(dir).join(name),
        None => PathBuf::from(name),
    }
}

/// Lets the script continue once a screenshot is on disk.
fn screenshot_saved(_: On<ScreenshotCaptured>, mut hold: ResMut<ScriptHold>) {
    hold.0 = hold.0.saturating_sub(1);
}

fn scene_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut commands: Commands,
    mut print: MessageWriter<Print>,
    mut hold: ResMut<ScriptHold>,
    mut spin: ResMut<Spin>,
    mut scripted_fire: ResMut<ScriptedFire>,
    mut pending_look: ResMut<PendingLook>,
    mut player: Single<(&mut Transform, &mut Position, &mut LinearVelocity), With<LocalPlayer>>,
) {
    for cmd in cmds.read() {
        let numbers: Vec<f32> = cmd.args.iter().filter_map(|a| a.parse().ok()).collect();
        match cmd.name.as_str() {
            "screenshot" => {
                let path = shot_path(cmd.arg(0).unwrap_or("shot"));
                print.write(Print(format!("screenshot -> {}", path.display())));
                // The capture lands a frame or two later; hold the script so
                // nothing changes (or quits) before it is saved.
                hold.0 += 1;
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(path))
                    .observe(screenshot_saved);
            }
            "tp" => {
                let [x, y, z] = numbers[..] else {
                    print.write(Print::error("usage: tp <x> <y> <z>"));
                    continue;
                };
                let at = Vec3::new(x, y, z);
                let (transform, position, velocity) = &mut *player;
                transform.translation = at;
                position.0 = at;
                velocity.0 = Vec3::ZERO;
            }
            "look" => {
                let [yaw, pitch] = numbers[..] else {
                    print.write(Print::error("usage: look <yaw> <pitch>"));
                    continue;
                };
                pending_look.0 = Some(Quat::from_euler(
                    EulerRot::YXZ,
                    yaw.to_radians(),
                    pitch.to_radians(),
                    0.0,
                ));
            }
            "fire" => scripted_fire.0 += numbers.first().map_or(1, |n| *n as u32),
            "spin" => spin.0 = numbers.first().copied().unwrap_or(0.0),
            _ => {}
        }
    }
}

fn spin(
    time: Res<Time>,
    spin: Res<Spin>,
    mut pending_look: ResMut<PendingLook>,
    mut camera: Single<&mut Transform, (With<ViewRig>, Without<LocalPlayer>)>,
) {
    if let Some(rotation) = pending_look.0.take() {
        camera.rotation = rotation;
    }
    if spin.0 != 0.0 {
        camera.rotate_y(spin.0.to_radians() * time.delta_secs());
    }
}
