//! `r_*` console switches for individual render features, for tracking down
//! visual bugs and for lighting test screenshots.

use bevy::{
    pbr::{ContactShadows, DistanceFog, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
};

use crate::{
    console::{ConsoleAppExt, ConsoleCommand, Print},
    render::{WorldCamera, set_anti_aliasing},
    sky::Sky,
    weapon::HideViewModel,
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, render_switches);

    app.console_command(
        "r_aa",
        "<smaa|taa|fxaa|off>",
        "anti-aliasing (default smaa)",
    )
    .console_command("r_ssao", "<0|1>", "screen-space ambient occlusion")
    .console_command("r_bloom", "<0|1>", "bloom")
    .console_command("r_fog", "<0|1>", "distance fog")
    .console_command("r_contact", "<0|1>", "contact shadows")
    .console_command("r_shadows", "<0|1>", "shadow maps on every light")
    .console_command("r_viewmodel", "<0|1>", "draw your own gun")
    .console_command(
        "r_ambient",
        "<brightness>",
        "flat fill light for inspecting models; 0 is off",
    );
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

fn render_switches(
    mut cmds: MessageReader<ConsoleCommand>,
    mut commands: Commands,
    mut print: MessageWriter<Print>,
    world_camera: Single<Entity, With<WorldCamera>>,
    mut hide_view_model: ResMut<HideViewModel>,
    sky: Res<Sky>,
    mut point_lights: Query<&mut PointLight>,
    mut directional: Query<&mut DirectionalLight>,
) {
    let camera = *world_camera;
    for cmd in cmds.read() {
        let name = cmd.name.as_str();
        if !name.starts_with("r_") {
            continue;
        }
        if name == "r_ambient" {
            let Some(brightness) = cmd.arg(0).and_then(|v| v.parse::<f32>().ok()) else {
                print.write(Print::error("usage: r_ambient <brightness>"));
                continue;
            };
            commands.insert_resource(GlobalAmbientLight {
                brightness: brightness.max(0.0),
                ..default()
            });
            continue;
        }
        if name == "r_aa" {
            let mode = cmd.arg(0).unwrap_or("");
            if !set_anti_aliasing(&mut commands, camera, mode) {
                print.write(Print::error("usage: r_aa <smaa|taa|fxaa|off>"));
            }
            continue;
        }
        let Some(on) = flag(cmd) else {
            print.write(Print::error(format!("usage: {name} <0|1>")));
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
            "r_fog" => toggle::<DistanceFog>(&mut commands, camera, on, || sky.kind.look().fog()),
            "r_contact" => toggle(&mut commands, camera, on, ContactShadows::default),
            "r_viewmodel" => hide_view_model.0 = !on,
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
    }
}
