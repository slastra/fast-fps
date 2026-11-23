//! The first-person view: the rig the mouse turns, mouse sensitivity and
//! field of view (both scaled by the sniper scope).

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;

use crate::{
    config::{DEFAULT_FOV, DEFAULT_SENS},
    console::{ConsoleAppExt, ConsoleCommand, Print, SaveSetting},
    render::WorldCamera,
    weapon::Zoom,
};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<ViewSettings>()
        .add_systems(Update, (apply_view, view_commands));

    app.console_command("sens", "<value>", "mouse sensitivity (default 0.07)")
        .console_command("fov", "<degrees>", "field of view (default 80)");
}

/// What the mouse turns. Ahoy writes yaw and pitch here; the camera is a
/// child of it so recoil can punch the view without moving your aim.
#[derive(Component)]
pub struct ViewRig;

/// Mouse sensitivity and field of view, before any scope zoom.
#[derive(Resource)]
pub struct ViewSettings {
    pub sens: f32,
    pub fov: f32,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            sens: DEFAULT_SENS,
            fov: DEFAULT_FOV,
        }
    }
}

/// Apply sensitivity and FOV, scaled by the scope zoom so the mouse moves the
/// same distance on screen per count whether zoomed or not.
pub fn apply_view(
    settings: Res<ViewSettings>,
    zoom: Res<Zoom>,
    mut projection: Single<&mut Projection, With<WorldCamera>>,
    mut bindings: Query<(&Binding, &mut Scale)>,
) {
    let half = (settings.fov.to_radians() / 2.0).tan() * zoom.0;
    let fov = 2.0 * half.atan();
    if let Projection::Perspective(p) = &mut **projection
        && (p.fov - fov).abs() > 1e-4
    {
        p.fov = fov;
    }
    let sens = Scale::splat(settings.sens * zoom.0);
    for (binding, mut scale) in &mut bindings {
        if matches!(binding, Binding::MouseMotion { .. }) && scale.factor != sens.factor {
            *scale = sens;
        }
    }
}

/// `sens` and `fov`. Both are saved for next time.
fn view_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut print: MessageWriter<Print>,
    mut save: MessageWriter<SaveSetting>,
    mut settings: ResMut<ViewSettings>,
) {
    for cmd in cmds.read() {
        match cmd.name.as_str() {
            "sens" => {
                let Some(value) = cmd.arg(0).and_then(|v| v.parse::<f32>().ok()) else {
                    print.write(Print(format!("sens is {}", settings.sens)));
                    continue;
                };
                settings.sens = value.clamp(0.001, 2.0);
                save.write(SaveSetting {
                    key: "sens",
                    value: settings.sens.to_string(),
                });
            }
            "fov" => {
                let Some(value) = cmd.arg(0).and_then(|v| v.parse::<f32>().ok()) else {
                    print.write(Print(format!("fov is {}", settings.fov)));
                    continue;
                };
                settings.fov = value.clamp(50.0, 120.0);
                save.write(SaveSetting {
                    key: "fov",
                    value: settings.fov.to_string(),
                });
            }
            _ => {}
        }
    }
}
