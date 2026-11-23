//! Weapon icons for the HUD, rendered from the gun models themselves.
//!
//! At startup each gun gets a tiny photo studio: a copy of its model painted
//! flat white, and a side-on camera that renders it into a texture. After a
//! few seconds the studios are torn down and the textures remain. Nobody has
//! to draw an icon, and a new weapon gets one for free.

use bevy::{
    camera::{RenderTarget, ScalingMode, visibility::RenderLayers},
    core_pipeline::tonemapping::Tonemapping,
    light::NotShadowCaster,
    prelude::*,
    render::render_resource::TextureFormat,
};
use rand::{SeedableRng, rngs::StdRng};

use super::{WeaponKind, models};
use crate::lowpoly;

/// Icon textures are this many times wider than they are tall.
pub const ICON_ASPECT: f32 = 3.2;
const HEIGHT: u32 = 160;
/// Studios render on their own layers, so the game camera never sees them.
const FIRST_LAYER: usize = 8;
/// Frames to keep the studio cameras running. Shaders compile in the
/// background, so the first frames can come out empty.
const STUDIO_FRAMES: u32 = 240;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PreStartup, open_studios)
        .add_systems(Update, close_studios);
}

/// One white-on-transparent side view per weapon, indexed by
/// `WeaponKind::index`. Tint it with `ImageNode::color`.
#[derive(Resource)]
pub struct WeaponIcons(pub [Handle<Image>; WeaponKind::COUNT]);

impl WeaponIcons {
    pub fn get(&self, kind: WeaponKind) -> Handle<Image> {
        self.0[kind.index()].clone()
    }
}

#[derive(Component)]
struct Studio;

fn open_studios(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let white = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });
    let mut rng = StdRng::seed_from_u64(5);
    let icons = WeaponKind::ALL.map(|kind| {
        let width = (HEIGHT as f32 * ICON_ASPECT) as u32;
        let image = images.add(Image::new_target_texture(
            width,
            HEIGHT,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
        let layer = RenderLayers::layer(FIRST_LAYER + kind.index());

        // The gun's outline from the side: z runs along the barrel, y is up.
        let parts = models::build(&models::model(kind), &mut meshes, &mut rng);
        let (mut low, mut high) = (Vec2::MAX, Vec2::MIN);
        for (mesh, transform, _) in &parts {
            let Some(mesh) = meshes.get(&mesh.0) else {
                continue;
            };
            for point in lowpoly::positions(mesh) {
                let p = transform.transform_point(point);
                low = low.min(Vec2::new(p.z, p.y));
                high = high.max(Vec2::new(p.z, p.y));
            }
        }
        let center = (low + high) / 2.0;
        let size = high - low;
        // Fill the frame, but don't blow a pistol up to the size of a rifle.
        let view_width = (size.x * 1.1).max(size.y * 1.1 * ICON_ASPECT).max(0.6);

        for (mesh, transform, _) in parts {
            // Strip the baked colors so the icon is a clean white silhouette.
            let Some(mut plain) = meshes.get(&mesh.0).cloned() else {
                continue;
            };
            plain.remove_attribute(Mesh::ATTRIBUTE_COLOR);
            commands.spawn((
                Studio,
                Mesh3d(meshes.add(plain)),
                MeshMaterial3d(white.clone()),
                transform,
                layer.clone(),
                NotShadowCaster,
            ));
        }
        // From the gun's left, so the barrel points left in the icon.
        commands.spawn((
            Studio,
            Camera3d::default(),
            Camera {
                order: -10 - kind.index() as isize,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::from(image.clone()),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed {
                    width: view_width,
                    height: view_width / ICON_ASPECT,
                },
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(-5.0, center.y, center.x)
                .looking_at(Vec3::new(0.0, center.y, center.x), Vec3::Y),
            layer,
            Msaa::Sample4,
            Tonemapping::None,
        ));
        image
    });
    commands.insert_resource(WeaponIcons(icons));
}

fn close_studios(
    mut commands: Commands,
    studios: Query<Entity, With<Studio>>,
    mut frames: Local<u32>,
) {
    *frames += 1;
    if *frames == STUDIO_FRAMES {
        for entity in &studios {
            commands.entity(entity).despawn();
        }
    }
}
