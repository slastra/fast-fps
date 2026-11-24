//! The sky and everything that depends on it: sun or moon, stars, clouds,
//! the main light, fill light, fog and exposure.
//!
//! A [`Look`] is one complete time of day. Switch between them in the console
//! with `sky night`, `sky dusk`, `sky dawn`, `sky day` or `sky next`. To add
//! or retune one, edit [`SkyKind::look`]: every number that changes the mood
//! of the game is in that one function.
//!
//! The sky itself is a smooth dome with its gradient baked into vertex
//! colors. There is no sky texture. The sun or moon and the clouds are
//! faceted, like the rest of the game.

use std::f32::consts::TAU;

use bevy::{
    camera::Exposure,
    light::{CascadeShadowConfig, CascadeShadowConfigBuilder, NotShadowCaster},
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
    render::{render_resource::Face, view::ColorGrading},
};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{
    console::{ConsoleAppExt, ConsoleCommand, Print, SaveSetting},
    lowpoly,
    render::WorldCamera,
};

/// How far away the dome is drawn. It follows the camera, so this only has
/// to be beyond the level and inside the camera's far plane.
const DOME_RADIUS: f32 = 420.0;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Sky>()
        .init_resource::<Cascades>()
        .add_systems(Update, apply_cascades)
        .add_systems(Startup, spawn_sky)
        .add_systems(Update, (sky_command, apply_look).chain())
        .add_systems(Update, drift_clouds)
        .add_systems(
            PostUpdate,
            follow_camera.before(TransformSystems::Propagate),
        );

    app.console_command(
        "sky",
        "[night|dusk|dawn|day|next]",
        "time of day (default dusk)",
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SkyKind {
    Night,
    #[default]
    Dusk,
    Dawn,
    Day,
}

/// How many shadow cascades the sun or moon uses. Each is another render of
/// the shadow casters from the light's side. Set with `r_cascades`.
#[derive(Resource)]
pub struct Cascades(pub usize);

impl Default for Cascades {
    fn default() -> Self {
        Self(4)
    }
}

fn cascade_config(count: usize) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder {
        num_cascades: count.max(1),
        maximum_distance: 90.0,
        first_cascade_far_bound: 14.0,
        ..default()
    }
    .build()
}

/// Rebuild the key light's cascades when the count changes.
fn apply_cascades(
    cascades: Res<Cascades>,
    mut commands: Commands,
    key: Single<Entity, With<KeyLight>>,
) {
    if cascades.is_changed() {
        commands.entity(*key).insert(cascade_config(cascades.0));
    }
}

/// The current time of day.
#[derive(Resource, Default)]
pub struct Sky {
    pub kind: SkyKind,
}

/// One complete time of day.
pub struct Look {
    // The dome, bottom to top.
    ground: Color,
    horizon: Color,
    band: Color,
    zenith: Color,
    /// Tint around the sun or moon.
    glow: Color,

    /// Where the sun or moon is drawn (a direction toward it).
    disc_at: Vec3,
    disc_color: Color,
    /// How bright the disc is; above 1 it blooms.
    disc_gain: f32,
    disc_size: f32,

    /// Where the main light comes from. Not always the disc: at dusk the sun
    /// is on the horizon but the light is steeper, or the walls would shade
    /// the whole arena.
    light_from: Vec3,
    light_color: Color,
    /// In lux.
    light: f32,
    /// Soft light from straight above and below, so faces in shadow still
    /// have some shape.
    fill: (Color, f32),
    bounce: (Color, f32),
    ambient: (Color, f32),

    fog: Color,
    /// How far you can see, in meters.
    visibility: f32,
    /// Star brightness, 0 to 1.
    stars: f32,
    cloud_top: Color,
    cloud_under: Color,
    /// Camera exposure. Higher is darker.
    ev100: f32,
}

/// A direction from compass bearing and height above the horizon, in degrees.
fn toward(bearing: f32, elevation: f32) -> Vec3 {
    let (b, e) = (bearing.to_radians(), elevation.to_radians());
    Vec3::new(b.sin() * e.cos(), e.sin(), -b.cos() * e.cos())
}

impl SkyKind {
    pub const ALL: [SkyKind; 4] = [SkyKind::Night, SkyKind::Dusk, SkyKind::Dawn, SkyKind::Day];

    pub fn name(self) -> &'static str {
        match self {
            SkyKind::Night => "night",
            SkyKind::Dusk => "dusk",
            SkyKind::Dawn => "dawn",
            SkyKind::Day => "day",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub fn look(self) -> Look {
        match self {
            SkyKind::Night => Look {
                ground: Color::srgb(0.01, 0.012, 0.03),
                horizon: Color::srgb(0.05, 0.08, 0.2),
                band: Color::srgb(0.025, 0.04, 0.13),
                zenith: Color::srgb(0.008, 0.012, 0.05),
                glow: Color::srgb(0.16, 0.22, 0.42),
                disc_at: toward(-40.0, 38.0),
                disc_color: Color::srgb(0.86, 0.92, 1.0),
                disc_gain: 2.6,
                disc_size: 20.0,
                light_from: toward(-40.0, 46.0),
                light_color: Color::srgb(0.55, 0.66, 1.0),
                light: 38.0,
                fill: (Color::srgb(0.3, 0.42, 0.9), 16.0),
                bounce: (Color::srgb(0.25, 0.3, 0.5), 2.5),
                ambient: (Color::srgb(0.4, 0.5, 0.9), 2.6),
                fog: Color::srgb(0.03, 0.05, 0.13),
                visibility: 170.0,
                stars: 1.0,
                cloud_top: Color::srgb(0.1, 0.14, 0.28),
                cloud_under: Color::srgb(0.03, 0.04, 0.1),
                ev100: 4.5,
            },
            SkyKind::Dusk => Look {
                ground: Color::srgb(0.06, 0.04, 0.08),
                horizon: Color::srgb(0.95, 0.42, 0.2),
                band: Color::srgb(0.5, 0.2, 0.42),
                zenith: Color::srgb(0.045, 0.06, 0.24),
                glow: Color::srgb(1.0, 0.62, 0.25),
                disc_at: toward(-70.0, 4.0),
                disc_color: Color::srgb(1.0, 0.5, 0.2),
                disc_gain: 2.8,
                disc_size: 26.0,
                light_from: toward(-70.0, 33.0),
                light_color: Color::srgb(1.0, 0.6, 0.42),
                light: 90.0,
                fill: (Color::srgb(0.4, 0.42, 0.95), 62.0),
                bounce: (Color::srgb(0.55, 0.32, 0.3), 18.0),
                ambient: (Color::srgb(0.5, 0.45, 0.8), 9.0),
                fog: Color::srgb(0.42, 0.2, 0.27),
                visibility: 230.0,
                stars: 0.55,
                cloud_top: Color::srgb(0.3, 0.2, 0.4),
                cloud_under: Color::srgb(0.95, 0.42, 0.28),
                ev100: 5.0,
            },
            SkyKind::Dawn => Look {
                ground: Color::srgb(0.2, 0.2, 0.24),
                horizon: Color::srgb(1.0, 0.76, 0.55),
                band: Color::srgb(0.62, 0.62, 0.75),
                zenith: Color::srgb(0.16, 0.3, 0.55),
                glow: Color::srgb(1.0, 0.85, 0.6),
                disc_at: toward(95.0, 7.0),
                disc_color: Color::srgb(1.0, 0.86, 0.62),
                disc_gain: 3.0,
                disc_size: 20.0,
                light_from: toward(95.0, 26.0),
                light_color: Color::srgb(1.0, 0.8, 0.62),
                light: 230.0,
                fill: (Color::srgb(0.55, 0.65, 0.95), 150.0),
                bounce: (Color::srgb(0.5, 0.48, 0.42), 40.0),
                ambient: (Color::srgb(0.7, 0.72, 0.9), 22.0),
                fog: Color::srgb(0.78, 0.7, 0.68),
                visibility: 150.0,
                stars: 0.0,
                cloud_top: Color::srgb(0.85, 0.82, 0.88),
                cloud_under: Color::srgb(1.0, 0.72, 0.56),
                ev100: 6.3,
            },
            SkyKind::Day => Look {
                ground: Color::srgb(0.42, 0.5, 0.55),
                horizon: Color::srgb(0.72, 0.84, 0.93),
                band: Color::srgb(0.42, 0.63, 0.88),
                zenith: Color::srgb(0.14, 0.36, 0.76),
                glow: Color::srgb(1.0, 0.98, 0.9),
                disc_at: toward(140.0, 52.0),
                disc_color: Color::srgb(1.0, 0.97, 0.85),
                disc_gain: 4.0,
                disc_size: 13.0,
                light_from: toward(140.0, 52.0),
                light_color: Color::srgb(1.0, 0.96, 0.88),
                light: 14000.0,
                fill: (Color::srgb(0.5, 0.66, 1.0), 4500.0),
                bounce: (Color::srgb(0.45, 0.48, 0.36), 1500.0),
                ambient: (Color::srgb(0.7, 0.8, 1.0), 700.0),
                fog: Color::srgb(0.72, 0.84, 0.94),
                visibility: 420.0,
                stars: 0.0,
                cloud_top: Color::srgb(1.0, 1.0, 1.0),
                cloud_under: Color::srgb(0.68, 0.74, 0.84),
                ev100: 10.6,
            },
        }
    }
}

impl Look {
    /// The sky's color in a direction.
    fn sky_color(&self, dir: Vec3) -> Color {
        let y = dir.y;
        let base = if y < 0.0 {
            self.horizon.mix(&self.ground, (-y / 0.22).clamp(0.0, 1.0))
        } else if y < 0.2 {
            self.horizon.mix(&self.band, y / 0.2)
        } else {
            let t = ((y - 0.2) / 0.6).clamp(0.0, 1.0);
            self.band.mix(&self.zenith, t * (2.0 - t))
        };
        // A wide soft glow plus a tight one around the disc, fading with height.
        let facing = dir.dot(self.disc_at).max(0.0);
        let low = 1.0 - (y.max(0.0) / 0.7).clamp(0.0, 1.0);
        let glow = facing.powi(5) * 0.55 * low + facing.powi(60) * 0.6;
        base.mix(&self.glow, glow.min(1.0))
    }

    /// Distant things fade into `fog`, which is chosen to sit close to the
    /// horizon so the far forest melts into the sky. The squared falloff
    /// leaves everything nearby clear.
    pub fn fog(&self) -> DistanceFog {
        DistanceFog {
            color: self.fog,
            falloff: FogFalloff::from_visibility_squared(self.visibility),
            // A warm bloom in the haze when you look toward the light.
            directional_light_color: self.glow.with_alpha(0.35),
            directional_light_exponent: 24.0,
        }
    }
}

#[derive(Component)]
struct SkyRoot;

#[derive(Component)]
struct Dome;

#[derive(Component)]
struct Disc;

#[derive(Component)]
struct Stars;

#[derive(Component)]
struct Clouds;

#[derive(Component)]
struct KeyLight;

#[derive(Component)]
struct FillLight;

#[derive(Component)]
struct BounceLight;

/// Meshes and materials that change with the look.
#[derive(Resource)]
struct SkyAssets {
    dome: Handle<Mesh>,
    disc: Handle<Mesh>,
    clouds: Handle<Mesh>,
    stars: Handle<StandardMaterial>,
    applied: Option<SkyKind>,
}

/// Flat color straight to the screen: no lighting, no fog, visible from inside.
fn sky_material(color: LinearRgba) -> StandardMaterial {
    StandardMaterial {
        base_color: color.into(),
        unlit: true,
        fog_enabled: false,
        cull_mode: None::<Face>,
        ..default()
    }
}

/// A smooth gradient: the color is set at each vertex and blended across the
/// triangles. Fine enough (about 2.8 degrees between rows) that the tight glow
/// around the sun or moon stays round.
fn dome_mesh(look: &Look) -> Mesh {
    let mut sphere = Sphere::new(1.0).mesh().uv(128, 64);
    sphere.remove_attribute(Mesh::ATTRIBUTE_UV_0);
    let colors: Vec<[f32; 4]> = lowpoly::positions(&sphere)
        .into_iter()
        .map(|p| lowpoly::scaled(look.sky_color(p.normalize_or(Vec3::Y)), 1.0))
        .collect();
    sphere.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    sphere
}

fn disc_mesh(look: &Look) -> Mesh {
    let mut rng = StdRng::seed_from_u64(12);
    let sphere = Sphere::new(1.0).mesh().ico(2).unwrap();
    let c = look.disc_color.to_linear() * look.disc_gain;
    let color = Color::LinearRgba(c);
    // Uneven facets read as craters on the moon and shimmer on the sun.
    lowpoly::facet(sphere, color, 0.09, &mut rng)
}

fn star_mesh() -> Mesh {
    let mut rng = StdRng::seed_from_u64(13);
    let mut stars = Vec::new();
    for i in 0..360 {
        let dir = loop {
            let v = Vec3::new(
                rng.random_range(-1.0..1.0),
                rng.random_range(0.03..1.0),
                rng.random_range(-1.0..1.0),
            );
            if v.length_squared() < 1.0 && v.length_squared() > 0.01 {
                break v.normalize();
            }
        };
        // Mostly faint, a few bright.
        let big = i % 17 == 0;
        let size = if big {
            rng.random_range(1.7..2.4)
        } else {
            rng.random_range(0.6..1.2)
        };
        let warmth = rng.random_range(-1.0..1.0_f32);
        let tint = Color::srgb(1.0 + 0.15 * warmth, 1.0, 1.0 - 0.2 * warmth);
        let brightness = if big { 1.6 } else { rng.random_range(0.4..1.0) };
        let mesh = RegularPolygon::new(size, 4)
            .mesh()
            .build()
            .rotated_by(Quat::from_rotation_arc(Vec3::Z, -dir))
            .translated_by(dir * (DOME_RADIUS - 12.0));
        stars.push(lowpoly::facet(mesh, tint, 0.0, &mut rng).scaled_colors(brightness));
    }
    lowpoly::merge(stars)
}

fn cloud_mesh(look: &Look) -> Mesh {
    // The same seed every time, so changing the look recolors the same clouds.
    let mut rng = StdRng::seed_from_u64(14);
    let mut clouds = Vec::new();
    for i in 0..18 {
        let angle = i as f32 / 18.0 * TAU + rng.random_range(-0.15..0.15);
        let distance = rng.random_range(150.0..330.0);
        let center = Vec3::new(
            angle.cos() * distance,
            rng.random_range(55.0..115.0),
            angle.sin() * distance,
        );
        let scale = rng.random_range(14.0..30.0);
        for _ in 0..rng.random_range(3..=5) {
            let offset = Vec3::new(
                rng.random_range(-1.6..1.6),
                rng.random_range(-0.2..0.3),
                rng.random_range(-0.8..0.8),
            ) * scale;
            let size = Vec3::new(1.0, 0.36, 0.7) * scale * rng.random_range(0.5..1.0);
            let blob = lowpoly::rock(size, rng.random()).translated_by(center + offset);
            clouds.push(lowpoly::facet_with(blob, &mut rng, |_, normal, rng| {
                let lit = (normal.y * 0.5 + 0.5).clamp(0.0, 1.0);
                lowpoly::shade(look.cloud_under.mix(&look.cloud_top, lit), 0.04, rng)
            }));
        }
    }
    lowpoly::merge(clouds)
}

trait ScaleColors {
    fn scaled_colors(self, k: f32) -> Self;
}

impl ScaleColors for Mesh {
    fn scaled_colors(mut self, k: f32) -> Self {
        use bevy::mesh::VertexAttributeValues::Float32x4;
        if let Some(Float32x4(colors)) = self.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
            for c in colors {
                for v in &mut c[..3] {
                    *v *= k;
                }
            }
        }
        self
    }
}

fn spawn_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sky: Res<Sky>,
) {
    let look = sky.kind.look();
    let assets = SkyAssets {
        dome: meshes.add(dome_mesh(&look)),
        disc: meshes.add(disc_mesh(&look)),
        clouds: meshes.add(cloud_mesh(&look)),
        stars: materials.add(sky_material(LinearRgba::WHITE)),
        applied: None,
    };
    let flat = materials.add(sky_material(LinearRgba::WHITE));

    commands
        .spawn((
            Name::new("Sky"),
            SkyRoot,
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|root| {
            root.spawn((
                Dome,
                Mesh3d(assets.dome.clone()),
                MeshMaterial3d(flat.clone()),
                Transform::from_scale(Vec3::splat(DOME_RADIUS)),
                NotShadowCaster,
            ));
            root.spawn((
                Stars,
                Mesh3d(meshes.add(star_mesh())),
                MeshMaterial3d(assets.stars.clone()),
                NotShadowCaster,
            ));
            root.spawn((
                Disc,
                Mesh3d(assets.disc.clone()),
                MeshMaterial3d(flat.clone()),
                NotShadowCaster,
            ));
        });
    // Clouds stay put in the world, so they slide past as you move.
    commands.spawn((
        Clouds,
        Mesh3d(assets.clouds.clone()),
        MeshMaterial3d(flat),
        Transform::default(),
        NotShadowCaster,
    ));

    commands.spawn((
        KeyLight,
        DirectionalLight {
            shadow_maps_enabled: true,
            contact_shadows_enabled: true,
            ..default()
        },
        cascade_config(4),
    ));
    commands.spawn((
        FillLight,
        DirectionalLight::default(),
        Transform::default().looking_to(Vec3::NEG_Y, Vec3::Z),
    ));
    commands.spawn((
        BounceLight,
        DirectionalLight::default(),
        Transform::default().looking_to(Vec3::Y, Vec3::Z),
    ));
    commands.insert_resource(assets);
}

fn sky_command(
    mut cmds: MessageReader<ConsoleCommand>,
    mut sky: ResMut<Sky>,
    mut print: MessageWriter<Print>,
    mut save: MessageWriter<SaveSetting>,
) {
    for cmd in cmds.read() {
        if cmd.name != "sky" {
            continue;
        }
        let kind = match cmd.arg(0) {
            None => {
                print.write(Print(format!(
                    "sky is {} (night, dusk, dawn, day, next)",
                    sky.kind.name()
                )));
                continue;
            }
            Some("next") => {
                let i = SkyKind::ALL
                    .iter()
                    .position(|k| *k == sky.kind)
                    .unwrap_or(0);
                SkyKind::ALL[(i + 1) % SkyKind::ALL.len()]
            }
            Some(name) => match SkyKind::parse(name) {
                Some(kind) => kind,
                None => {
                    print.write(Print::error("usage: sky <night|dusk|dawn|day|next>"));
                    continue;
                }
            },
        };
        sky.kind = kind;
        save.write(SaveSetting {
            key: "sky",
            value: kind.name().to_string(),
        });
    }
}

/// Push the current look into the dome, the lights and the camera.
fn apply_look(
    sky: Res<Sky>,
    mut assets: ResMut<SkyAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    camera: Query<Entity, With<WorldCamera>>,
    mut disc: Single<&mut Transform, With<Disc>>,
    mut stars: Single<&mut Visibility, With<Stars>>,
    mut lights: ParamSet<(
        Single<(&mut DirectionalLight, &mut Transform), (With<KeyLight>, Without<Disc>)>,
        Single<&mut DirectionalLight, With<FillLight>>,
        Single<&mut DirectionalLight, With<BounceLight>>,
    )>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    if assets.applied == Some(sky.kind) {
        return;
    }
    assets.applied = Some(sky.kind);
    let look = sky.kind.look();

    let _ = meshes.insert(assets.dome.id(), dome_mesh(&look));
    let _ = meshes.insert(assets.disc.id(), disc_mesh(&look));
    let _ = meshes.insert(assets.clouds.id(), cloud_mesh(&look));
    if let Some(mut material) = materials.get_mut(assets.stars.id()) {
        material.base_color = Color::LinearRgba(LinearRgba::WHITE * (look.stars * 2.6));
    }
    **stars = if look.stars > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    **disc = Transform::from_translation(look.disc_at * (DOME_RADIUS - 25.0))
        .with_scale(Vec3::splat(look.disc_size));

    {
        let mut key = lights.p0();
        key.0.color = look.light_color;
        key.0.illuminance = look.light;
        *key.1 = Transform::default().looking_to(-look.light_from, Vec3::Y);
    }
    {
        let mut fill = lights.p1();
        fill.color = look.fill.0;
        fill.illuminance = look.fill.1;
    }
    {
        let mut bounce = lights.p2();
        bounce.color = look.bounce.0;
        bounce.illuminance = look.bounce.1;
    }

    commands.insert_resource(GlobalAmbientLight {
        color: look.ambient.0,
        brightness: look.ambient.1,
        ..default()
    });
    commands.insert_resource(ClearColor(look.horizon));
    commands
        .entity(camera)
        .insert((Exposure { ev100: look.ev100 }, look.fog(), grading()));
}

/// A little more color than the tonemapper gives by itself. (Leave the
/// contrast settings alone: even small values crush a dark scene to black.)
fn grading() -> ColorGrading {
    let mut grading = ColorGrading::default();
    grading.global.post_saturation = 1.15;
    grading
}

fn follow_camera(
    camera: Single<&GlobalTransform, With<WorldCamera>>,
    mut root: Single<&mut Transform, With<SkyRoot>>,
) {
    // Last frame's camera position is close enough for something this far away.
    root.translation = camera.translation();
}

fn drift_clouds(time: Res<Time>, mut clouds: Single<&mut Transform, With<Clouds>>) {
    clouds.rotate_y(0.004 * time.delta_secs());
}
