//! Camera and lighting setup, modeled on Foxtrot's night look: no ambient
//! light, a night-sky environment map, warm point lights, HDR with
//! TonyMcMapface, bloom and SSAO.
//!
//! One camera renders everything, gun included. The gun is drawn small and
//! close to the eye (see `weapon.rs`), which looks identical to a separate
//! view-model camera without the compositing and TAA-ghosting problems.

use bevy::{
    anti_alias::{fxaa::Fxaa, smaa::Smaa, taa::TemporalAntiAliasing},
    camera::{Exposure, Hdr},
    core_pipeline::tonemapping::Tonemapping,
    light::{NotShadowCaster, ShadowFilteringMethod, Skybox},
    pbr::{ContactShadows, DistanceFog, FogFalloff, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
};

pub const DEFAULT_FOV: f32 = 80.0;

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(GlobalAmbientLight::NONE)
        .insert_resource(ClearColor(Color::srgb_u8(12, 8, 18)))
        .add_systems(PreStartup, load_sky);
}

#[derive(Resource)]
pub struct Sky {
    diffuse: Handle<Image>,
    specular: Handle<Image>,
}

fn load_sky(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Sky {
        diffuse: assets.load("cubemaps/NightSkyHDRI001_4K-HDR_diffuse.ktx2"),
        specular: assets.load("cubemaps/NightSkyHDRI001_4K-HDR_specular.ktx2"),
    });
}

/// Marker for the player's camera.
#[derive(Component)]
pub struct WorldCamera;

pub fn world_camera(sky: &Sky) -> impl Bundle {
    (
        (
            WorldCamera,
            Camera3d::default(),
            Projection::from(PerspectiveProjection {
                fov: DEFAULT_FOV.to_radians(),
                near: 0.02,
                ..default()
            }),
            Hdr,
            Exposure { ev100: 4.5 },
            Tonemapping::TonyMcMapface,
            Bloom::NATURAL,
        ),
        (
            Skybox {
                image: Some(sky.specular.clone()),
                brightness: 8.0,
                ..default()
            },
            EnvironmentMapLight {
                diffuse_map: sky.diffuse.clone(),
                specular_map: sky.specular.clone(),
                intensity: 300.0,
                ..default()
            },
            fog(),
            Msaa::Off,
            Smaa::default(),
            ShadowFilteringMethod::Gaussian,
            ScreenSpaceAmbientOcclusion::default(),
            ContactShadows::default(),
        ),
    )
}

pub fn fog() -> DistanceFog {
    DistanceFog {
        color: Color::srgba(0.045, 0.03, 0.07, 1.0),
        falloff: FogFalloff::from_visibility_colors(
            140.0,
            Color::srgb(0.05, 0.035, 0.08),
            Color::srgb(0.06, 0.05, 0.1),
        ),
        ..default()
    }
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
