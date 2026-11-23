//! Console commands for testing and the scripted harness: screenshots,
//! putting the camera somewhere exact, a steady spin to reproduce motion
//! artifacts, and `r_*` switches for each render feature.
//!
//! See `scripts/harness.sh` and `tests/harness/*.cfg`.

use std::path::PathBuf;

use avian3d::prelude::*;
use bevy::{
    pbr::{ContactShadows, DistanceFog, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

use crate::{
    console::{ConsoleAppExt, ConsoleCommand, Print},
    player::{LocalPlayer, ViewRig},
    render::{WorldCamera, fog, set_anti_aliasing},
    weapon::{GunRoot, ScriptedFire},
};

pub(super) fn plugin(app: &mut App) {
    // Camera writes happen in PreUpdate: ahoy copies the camera rotation into
    // the character at the start of the fixed loop and back out in Update, so
    // anything written later in the frame is lost.
    app.init_resource::<Spin>()
        .init_resource::<PendingLook>()
        .add_systems(Update, debug_commands)
        .add_systems(PreUpdate, spin);

    app.console_command(
        "screenshot",
        "<name>",
        "save a PNG (into $FASTFPS_SHOT_DIR if set)",
    )
    .console_command("tp", "<x> <y> <z>", "teleport your character")
    .console_command("look", "<yaw> <pitch>", "set view angles in degrees")
    .console_command("spin", "<deg/s>", "turn steadily; 0 stops")
    .console_command("fire", "[shots]", "pull the trigger from a script")
    .console_command(
        "r_aa",
        "<smaa|taa|fxaa|off>",
        "anti-aliasing (default smaa)",
    )
    .console_command("r_ssao", "<0|1>", "screen-space ambient occlusion")
    .console_command("r_bloom", "<0|1>", "bloom")
    .console_command("r_fog", "<0|1>", "distance fog")
    .console_command("r_contact", "<0|1>", "contact shadows")
    .console_command("r_shadows", "<0|1>", "shadow maps on every light")
    .console_command("r_viewmodel", "<0|1>", "draw the gun");
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
    match std::env::var_os("FASTFPS_SHOT_DIR") {
        Some(dir) => PathBuf::from(dir).join(name),
        None => PathBuf::from(name),
    }
}

fn flag(cmd: &ConsoleCommand) -> Option<bool> {
    match cmd.arg(0)? {
        "1" | "on" | "true" => Some(true),
        "0" | "off" | "false" => Some(false),
        _ => None,
    }
}

fn toggle<C: Component>(
    commands: &mut Commands,
    entity: Entity,
    on: bool,
    make: impl FnOnce() -> C,
) {
    if on {
        commands.entity(entity).insert(make());
    } else {
        commands.entity(entity).remove::<C>();
    }
}

fn debug_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut commands: Commands,
    mut print: MessageWriter<Print>,
    mut spin: ResMut<Spin>,
    mut scripted_fire: ResMut<ScriptedFire>,
    mut pending_look: ResMut<PendingLook>,
    world_camera: Single<Entity, With<WorldCamera>>,
    mut gun: Single<&mut Visibility, With<GunRoot>>,
    mut player: Single<(&mut Transform, &mut Position, &mut LinearVelocity), With<LocalPlayer>>,
    mut point_lights: Query<&mut PointLight>,
    mut directional: Query<&mut DirectionalLight>,
) {
    let camera = *world_camera;
    for cmd in cmds.read() {
        let name = cmd.name.as_str();
        if name == "r_aa" {
            let mode = cmd.arg(0).unwrap_or("");
            if !set_anti_aliasing(&mut commands, camera, mode) {
                print.write(Print::new("usage: r_aa <smaa|taa|fxaa|off>"));
            }
            continue;
        }
        if name.starts_with("r_") {
            let Some(on) = flag(cmd) else {
                print.write(Print(format!("usage: {name} <0|1>")));
                continue;
            };
            match name {
                "r_ssao" => toggle(
                    &mut commands,
                    camera,
                    on,
                    ScreenSpaceAmbientOcclusion::default,
                ),
                "r_bloom" => toggle(&mut commands, camera, on, || Bloom::NATURAL),
                "r_fog" => toggle::<DistanceFog>(&mut commands, camera, on, fog),
                "r_contact" => toggle(&mut commands, camera, on, ContactShadows::default),
                "r_viewmodel" => {
                    **gun = if on {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
                "r_shadows" => {
                    for mut light in &mut point_lights {
                        light.shadow_maps_enabled = on;
                    }
                    for mut light in &mut directional {
                        light.shadow_maps_enabled = on;
                    }
                }
                _ => {}
            }
            continue;
        }
        match name {
            "screenshot" => {
                let path = shot_path(cmd.arg(0).unwrap_or("shot"));
                print.write(Print(format!("screenshot -> {}", path.display())));
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(path));
            }
            "tp" => {
                let v: Vec<f32> = cmd.args.iter().filter_map(|a| a.parse().ok()).collect();
                let [x, y, z] = v[..] else {
                    print.write(Print::new("usage: tp <x> <y> <z>"));
                    continue;
                };
                let at = Vec3::new(x, y, z);
                let (transform, position, velocity) = &mut *player;
                transform.translation = at;
                position.0 = at;
                velocity.0 = Vec3::ZERO;
            }
            "look" => {
                let v: Vec<f32> = cmd.args.iter().filter_map(|a| a.parse().ok()).collect();
                let [yaw, pitch] = v[..] else {
                    print.write(Print::new("usage: look <yaw> <pitch>"));
                    continue;
                };
                pending_look.0 = Some(Quat::from_euler(
                    EulerRot::YXZ,
                    yaw.to_radians(),
                    pitch.to_radians(),
                    0.0,
                ));
            }
            "fire" => {
                scripted_fire.0 += cmd.arg(0).and_then(|n| n.parse().ok()).unwrap_or(1);
            }
            "spin" => {
                spin.0 = cmd.arg(0).and_then(|v| v.parse().ok()).unwrap_or(0.0);
            }
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
