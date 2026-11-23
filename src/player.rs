//! The character you drive on this machine, plus how everyone else looks.
//!
//! Movement is client-side (bevy_ahoy's kinematic controller); the result is
//! streamed to the server as a [`MoveInput`]. Other players' bodies live in
//! `avatar`; the practice dummies are dressed here.

use avian3d::prelude::*;
use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_ahoy::prelude::*;
use bevy_enhanced_input::prelude::{Press, *};
use bevy_replicon::prelude::*;
use rand::{SeedableRng, rngs::StdRng};

use crate::{
    Layer,
    console::{Console, ConsoleAppExt, ConsoleCommand, Print, SaveSetting},
    lowpoly,
    net::{Damaged, Downed, Dummy, MoveInput, Player, Pose, Respawn, Suicide},
    render::{self, DEFAULT_FOV, Sky, WorldCamera},
    weapon::{Arsenal, Zoom},
};

const DEFAULT_SENS: f32 = 0.07;
const KILL_HEIGHT: f32 = -15.0;

pub(super) fn plugin(app: &mut App) {
    app.add_input_context::<PlayerInput>()
        .init_resource::<GameInput>()
        .init_resource::<ViewSettings>()
        .add_systems(Startup, spawn_local_player)
        .add_systems(
            PreUpdate,
            (update_game_input, grab_cursor)
                .chain()
                .after(bevy::input::InputSystems),
        )
        .add_systems(PreUpdate, apply_respawn.after(ClientSystems::Receive))
        .add_systems(
            Update,
            (
                send_move,
                apply_view,
                fall_out,
                player_commands,
                smooth_remote,
                pose_dummies,
            ),
        )
        .add_observer(dress_dummy);

    app.console_command("sens", "<value>", "mouse sensitivity (default 0.07)")
        .console_command("fov", "<degrees>", "field of view (default 80)")
        .console_command("kill", "", "respawn somewhere else")
        .console_command("pos", "", "print your position and view angles");
}

/// The kinematic character controller owned by this machine. Never replicated.
#[derive(Component)]
pub struct LocalPlayer;

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

/// Whether gameplay input should reach the character this frame.
#[derive(Resource, Default)]
pub struct GameInput {
    pub enabled: bool,
}

#[derive(Component, Default)]
#[component(on_add = PlayerInput::on_add)]
pub struct PlayerInput;

impl PlayerInput {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        world.commands().entity(ctx.entity).apply_scene(bsn! {
            Actions<PlayerInput> [
                Action<Movement>::new()
                DeadZone
                Bindings [
                    {preset::cardinal::wasd_keys()}
                    --
                    {preset::axial::left_stick()}
                ]
                --
                Action<Jump>::new()
                Press
                Bindings [
                    Binding(KeyCode::Space)
                    --
                    Binding(GamepadButton::South)
                ]
                --
                Action<Crane>::new()
                Press
                Bindings [
                    Binding(KeyCode::Space)
                    --
                    Binding(GamepadButton::South)
                ]
                --
                Action<Mantle>::new()
                ~Hold::new(0.2)
                Bindings [
                    Binding(KeyCode::Space)
                    --
                    Binding(GamepadButton::South)
                ]
                --
                Action<Crouch>::new()
                Bindings [
                    Binding(KeyCode::ControlLeft)
                    --
                    Binding(KeyCode::KeyC)
                    --
                    Binding(GamepadButton::LeftTrigger2)
                ]
                --
                Action<RotateCamera>::new()
                Bindings [
                    Binding::mouse_motion()
                    ~Scale::splat(0.07)
                    --
                    {preset::axial::new(
                        (GamepadAxis::RightStickY, Scale::splat(4.0), DeadZone::default()),
                        (GamepadAxis::RightStickX, Scale::splat(4.0), DeadZone::default())
                    )}
                ]
            ]
        });
    }
}

fn spawn_local_player(mut commands: Commands, sky: Res<Sky>) {
    let start = Vec3::new(0.0, 1.2, 20.0);
    let player = commands
        .spawn((
            Name::new("Local player"),
            LocalPlayer,
            PlayerInput,
            CharacterController {
                filter: SpatialQueryFilter::from_mask(Layer::World),
                ..default()
            },
            RigidBody::Kinematic,
            Collider::cylinder(0.4, 1.8),
            CollisionLayers::new(Layer::LocalBody, Layer::World),
            Mass(90.0),
            Transform::from_translation(start),
        ))
        .id();

    commands
        .spawn((
            Name::new("View rig"),
            ViewRig,
            CharacterControllerCameraOf::new(player),
            Visibility::default(),
        ))
        .with_child((Name::new("Player camera"), render::world_camera(&sky)));
}

/// Apply sensitivity and FOV, scaled by the scope zoom so the mouse moves the
/// same distance on screen per count whether zoomed or not.
fn apply_view(
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

fn update_game_input(
    console: Res<Console>,
    cursor: Single<&CursorOptions>,
    mut input: ResMut<GameInput>,
    mut commands: Commands,
    player: Single<Entity, With<LocalPlayer>>,
) {
    let enabled = !console.open && cursor.grab_mode == CursorGrabMode::Locked;
    if enabled != input.enabled {
        input.enabled = enabled;
        commands
            .entity(*player)
            .insert(ContextActivity::<PlayerInput>::new(enabled));
    }
}

/// Click to grab the mouse, Esc or the console releases it.
fn grab_cursor(
    console: Res<Console>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: Single<&mut CursorOptions>,
    window: Single<&Window>,
) {
    let locked = cursor.grab_mode == CursorGrabMode::Locked;
    let release = console.open || keys.just_pressed(KeyCode::Escape) || !window.focused;
    if locked && release {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if !locked
        && !console.open
        && (mouse.just_pressed(MouseButton::Left) || console.is_changed())
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

/// Yaw and pitch from a camera rotation (Y then X).
pub fn yaw_pitch(rotation: Quat) -> (f32, f32) {
    let (yaw, pitch, _) = rotation.to_euler(EulerRot::YXZ);
    (yaw, pitch)
}

fn send_move(
    player: Single<(&Transform, &LinearVelocity, &CharacterControllerState), With<LocalPlayer>>,
    camera: Single<&Transform, With<ViewRig>>,
    arsenal: Res<Arsenal>,
    mut moves: MessageWriter<MoveInput>,
) {
    let (transform, velocity, state) = *player;
    let (yaw, pitch) = yaw_pitch(camera.rotation);
    let mut pose = Pose {
        pos: transform.translation,
        yaw,
        pitch,
        vel: velocity.0,
        flags: 0,
    };
    pose.set(Pose::CROUCH, state.crouching);
    pose.set(Pose::AIR, state.grounded.is_none());
    pose.set(Pose::RELOAD, arsenal.reloading > 0.0);
    moves.write(MoveInput(pose));
}

fn apply_respawn(
    mut respawns: MessageReader<Respawn>,
    mut player: Single<(&mut Transform, &mut Position, &mut LinearVelocity), With<LocalPlayer>>,
    mut camera: Single<&mut Transform, (With<ViewRig>, Without<LocalPlayer>)>,
) {
    let Some(respawn) = respawns.read().last() else {
        return;
    };
    let (transform, position, velocity) = &mut *player;
    transform.translation = respawn.pos;
    position.0 = respawn.pos;
    velocity.0 = Vec3::ZERO;
    camera.rotation = Quat::from_rotation_y(respawn.yaw);
}

fn fall_out(
    player: Single<&Transform, With<LocalPlayer>>,
    mut suicide: MessageWriter<Suicide>,
    mut sent: Local<f32>,
    time: Res<Time>,
) {
    *sent -= time.delta_secs();
    if player.translation.y < KILL_HEIGHT && *sent <= 0.0 {
        suicide.write(Suicide);
        *sent = 1.0;
    }
}

fn player_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut print: MessageWriter<Print>,
    mut suicide: MessageWriter<Suicide>,
    mut save: MessageWriter<SaveSetting>,
    mut settings: ResMut<ViewSettings>,
    local: Query<&Transform, With<LocalPlayer>>,
    cameras: Query<&GlobalTransform, With<WorldCamera>>,
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
            "kill" => {
                suicide.write(Suicide);
            }
            "pos" => {
                for t in &local {
                    print.write(Print(format!("body {:.2}", t.translation)));
                }
                for g in &cameras {
                    let (yaw, pitch) = yaw_pitch(g.rotation());
                    print.write(Print(format!(
                        "eye {:.2} yaw {:.0} pitch {:.0}",
                        g.translation(),
                        yaw.to_degrees(),
                        pitch.to_degrees()
                    )));
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Practice dummies
// ---------------------------------------------------------------------------

#[derive(Component)]
struct DummyPivot;

/// A dummy rocking on its post after being hit: (pitch, roll), a damped spring.
#[derive(Component, Default)]
struct Wobble {
    angle: Vec2,
    velocity: Vec2,
}

fn part(meshes: &mut Assets<Mesh>, size: Vec3, color: Color, rng: &mut StdRng) -> Mesh3d {
    Mesh3d(meshes.add(lowpoly::facet(
        Cuboid::from_size(size).mesh().build(),
        color,
        0.08,
        rng,
    )))
}

fn dress_dummy(
    add: On<Add<Dummy>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = StdRng::seed_from_u64(add.entity.to_bits());
    let base = MeshMaterial3d(materials.add(StandardMaterial {
        perceptual_roughness: 0.95,
        ..default()
    }));
    let straw = Color::srgb(0.72, 0.58, 0.3);
    let sack = Color::srgb(0.62, 0.5, 0.33);
    let wood = Color::srgb(0.36, 0.22, 0.12);
    let red = Color::srgb(0.7, 0.1, 0.08);

    let post = part(&mut meshes, Vec3::new(0.12, 1.0, 0.12), wood, &mut rng);
    let body = part(&mut meshes, Vec3::new(0.56, 0.72, 0.3), straw, &mut rng);
    let head = Mesh3d(meshes.add(lowpoly::facet(
        lowpoly::rock(Vec3::splat(0.2), 11),
        sack,
        0.1,
        &mut rng,
    )));
    let bullseye = Mesh3d(meshes.add(lowpoly::facet(
        Cylinder::new(0.17, 0.03).mesh().resolution(8).build(),
        red,
        0.05,
        &mut rng,
    )));
    let arm = part(&mut meshes, Vec3::new(0.9, 0.1, 0.1), wood, &mut rng);

    // Pivot at the feet so a downed dummy tips over backward.
    commands
        .entity(add.entity)
        .insert(Wobble::default())
        .with_children(|root| {
            root.spawn((
                DummyPivot,
                Transform::from_xyz(0.0, -0.95, 0.0),
                Visibility::default(),
            ))
            .with_children(|pivot| {
                pivot.spawn((post, base.clone(), Transform::from_xyz(0.0, 0.5, 0.0)));
                pivot.spawn((body, base.clone(), Transform::from_xyz(0.0, 1.2, 0.0)));
                pivot.spawn((arm, base.clone(), Transform::from_xyz(0.0, 1.35, 0.0)));
                pivot.spawn((head, base.clone(), Transform::from_xyz(0.0, 1.72, 0.0)));
                pivot.spawn((
                    bullseye,
                    base.clone(),
                    Transform::from_xyz(0.0, 1.2, -0.16)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ));
            });
        });
}

/// Replicated poses arrive at the server tick rate; ease toward them.
pub fn smooth_remote(
    time: Res<Time>,
    mut remotes: Query<(&Pose, &mut Transform), (With<Remote>, Or<(With<Player>, With<Dummy>)>)>,
) {
    let dt = time.delta_secs();
    for (pose, mut transform) in &mut remotes {
        let target_rot = Quat::from_rotation_y(pose.yaw);
        if transform.translation.distance(pose.pos) > 4.0 {
            transform.translation = pose.pos;
            transform.rotation = target_rot;
        } else {
            transform.translation.smooth_nudge(&pose.pos, 18.0, dt);
            transform.rotation.smooth_nudge(&target_rot, 18.0, dt);
        }
    }
}

/// Tip downed dummies over, and rock standing ones when they're hit.
fn pose_dummies(
    time: Res<Time>,
    mut hits: MessageReader<Damaged>,
    mut dummies: Query<(&Pose, Has<Downed>, &Children, &mut Wobble), With<Dummy>>,
    mut pivots: Query<&mut Transform, With<DummyPivot>>,
) {
    for hit in hits.read() {
        if let Ok((pose, _, _, mut wobble)) = dummies.get_mut(hit.target) {
            let local = Quat::from_rotation_y(-pose.yaw) * hit.dir;
            wobble.velocity += Vec2::new(local.z, -local.x) * 3.0;
        }
    }
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (_, downed, children, mut wobble) in &mut dummies {
        let w = &mut *wobble;
        let accel = -90.0 * w.angle - 5.0 * w.velocity;
        w.velocity += accel * dt;
        w.angle += w.velocity * dt;
        let rock = Quat::from_rotation_x(w.angle.x) * Quat::from_rotation_z(w.angle.y);
        for child in children {
            if let Ok(mut pivot) = pivots.get_mut(*child) {
                if downed {
                    pivot
                        .rotation
                        .smooth_nudge(&Quat::from_rotation_x(1.35), 14.0, dt);
                } else {
                    let upright = pivot.rotation.angle_between(Quat::IDENTITY) < 0.3;
                    if upright {
                        pivot.rotation = rock;
                    } else {
                        pivot.rotation.smooth_nudge(&Quat::IDENTITY, 6.0, dt);
                    }
                }
            }
        }
    }
}
