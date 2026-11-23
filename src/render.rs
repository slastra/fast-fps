//! The camera and its post-processing: HDR, tonemapping, bloom, ambient
//! occlusion, contact shadows and anti-aliasing. What the light and sky look
//! like is in `sky.rs`.
//!
//! One camera renders everything, gun included. The gun is drawn small and
//! close to the eye (see `weapon/view_model.rs`), which looks identical to a
//! separate view-model camera without the compositing and ghosting problems.

use bevy::{
    anti_alias::{fxaa::Fxaa, smaa::Smaa, taa::TemporalAntiAliasing},
    camera::Hdr,
    core_pipeline::tonemapping::Tonemapping,
    light::{NotShadowCaster, ShadowFilteringMethod},
    pbr::{ContactShadows, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
};

use crate::config::DEFAULT_FOV;

pub(super) fn plugin(app: &mut App) {
    // The sky module owns ambient light, fog and exposure.
    app.insert_resource(GlobalAmbientLight::NONE);
}

/// Marker for the player's camera.
#[derive(Component)]
pub struct WorldCamera;

pub fn world_camera() -> impl Bundle {
    (
        WorldCamera,
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: DEFAULT_FOV.to_radians(),
            near: 0.02,
            ..default()
        }),
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        Msaa::Off,
        Smaa::default(),
        ShadowFilteringMethod::Gaussian,
        ScreenSpaceAmbientOcclusion::default(),
        ContactShadows::default(),
    )
}

/// Anti-aliasing modes for `r_aa`.
pub fn set_anti_aliasing(commands: &mut Commands, camera: Entity, mode: &str) -> bool {
    let mut camera = commands.entity(camera);
    camera.remove::<(TemporalAntiAliasing, Smaa, Fxaa)>();
    match mode {
        "taa" => {
            camera.insert((
                TemporalAntiAliasing::default(),
                ShadowFilteringMethod::Temporal,
            ));
        }
        "smaa" => {
            camera.insert((Smaa::default(), ShadowFilteringMethod::Gaussian));
        }
        "fxaa" => {
            camera.insert((Fxaa::default(), ShadowFilteringMethod::Gaussian));
        }
        "off" => {
            camera.insert(ShadowFilteringMethod::Gaussian);
        }
        _ => return false,
    }
    true
}

/// Things that should never cast shadows (glows, effects, the gun).
pub fn no_shadow() -> NotShadowCaster {
    NotShadowCaster
}
