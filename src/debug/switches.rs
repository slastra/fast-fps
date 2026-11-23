//! Graphics settings: `r_quality` for the whole set at once, and `r_*`
//! switches for individual features, for tracking down visual bugs and
//! measuring what each one costs (see `docs/perf.md`).

use bevy::{
    pbr::{
        ContactShadows, DistanceFog, ScreenSpaceAmbientOcclusion,
        ScreenSpaceAmbientOcclusionQualityLevel,
    },
    post_process::bloom::Bloom,
    prelude::*,
};

use crate::{
    console::{ConsoleAppExt, ConsoleCommand, Print, SaveSetting},
    render::{WorldCamera, set_anti_aliasing},
    sky::{Cascades, Sky},
    weapon::HideViewModel,
    world::lights::{CampfireLight, LampShadows},
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, (render_switches, quality_command));

    app.console_command(
        "r_quality",
        "<low|medium|high>",
        "graphics preset (default high); low is for laptops",
    );

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
        "r_lamp_shadows",
        "<count>",
        "lamps nearest you that cast shadows (default 2)",
    )
    .console_command(
        "r_cascades",
        "<1-4>",
        "shadow cascades for the sun or moon (default 4)",
    )
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
        // r_quality has its own handler, quality_command.
        if !name.starts_with("r_") || name == "r_quality" {
            continue;
        }
        if name == "r_lamp_shadows" || name == "r_cascades" {
            let Some(n) = cmd.arg(0).and_then(|v| v.parse::<usize>().ok()) else {
                print.write(Print::error(format!("usage: {name} <count>")));
                continue;
            };
            if name == "r_lamp_shadows" {
                commands.insert_resource(LampShadows(n));
            } else {
                commands.insert_resource(Cascades(n.clamp(1, 4)));
            }
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
                // Lamps get theirs from the budget, so turn that off too.
                commands.insert_resource(LampShadows(if on { 2 } else { 0 }));
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

/// A graphics preset: what each level turns on. `high` is the game as
/// designed; the others trade effects for speed, cheapest last.
struct Preset {
    lamp_shadows: usize,
    campfire_shadow: bool,
    cascades: usize,
    ssao: Option<ScreenSpaceAmbientOcclusionQualityLevel>,
    contact: bool,
    bloom: bool,
    aa: &'static str,
}

fn preset(name: &str) -> Option<Preset> {
    Some(match name {
        "high" => Preset {
            lamp_shadows: 2,
            campfire_shadow: true,
            cascades: 4,
            ssao: Some(ScreenSpaceAmbientOcclusionQualityLevel::High),
            contact: true,
            bloom: true,
            aa: "smaa",
        },
        "medium" => Preset {
            lamp_shadows: 1,
            campfire_shadow: true,
            cascades: 2,
            ssao: Some(ScreenSpaceAmbientOcclusionQualityLevel::Low),
            contact: false,
            bloom: true,
            aa: "smaa",
        },
        "low" => Preset {
            lamp_shadows: 0,
            campfire_shadow: false,
            cascades: 1,
            ssao: None,
            contact: false,
            bloom: false,
            aa: "fxaa",
        },
        _ => return None,
    })
}

fn quality_command(
    mut cmds: MessageReader<ConsoleCommand>,
    mut commands: Commands,
    mut print: MessageWriter<Print>,
    mut save: MessageWriter<SaveSetting>,
    world_camera: Single<Entity, With<WorldCamera>>,
    mut campfire: Query<&mut PointLight, With<CampfireLight>>,
) {
    let camera = *world_camera;
    for cmd in cmds.read() {
        if cmd.name != "r_quality" {
            continue;
        }
        let Some(name) = cmd.arg(0) else {
            print.write(Print::new("usage: r_quality <low|medium|high>"));
            continue;
        };
        let Some(p) = preset(name) else {
            print.write(Print::error("usage: r_quality <low|medium|high>"));
            continue;
        };
        commands.insert_resource(LampShadows(p.lamp_shadows));
        commands.insert_resource(Cascades(p.cascades));
        for mut light in &mut campfire {
            light.shadow_maps_enabled = p.campfire_shadow;
        }
        match p.ssao {
            Some(quality_level) => {
                commands.entity(camera).insert(ScreenSpaceAmbientOcclusion {
                    quality_level,
                    ..default()
                });
            }
            None => {
                commands
                    .entity(camera)
                    .remove::<ScreenSpaceAmbientOcclusion>();
            }
        }
        toggle(&mut commands, camera, p.contact, ContactShadows::default);
        toggle(&mut commands, camera, p.bloom, || Bloom::NATURAL);
        set_anti_aliasing(&mut commands, camera, p.aa);
        save.write(SaveSetting {
            key: "r_quality",
            value: name.to_string(),
        });
    }
}
