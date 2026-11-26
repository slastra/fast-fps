//! The character you drive on this machine.
//!
//! Movement is client-side (bevy_ahoy's kinematic controller); the result is
//! streamed to the server as a `MoveInput`. Other players' bodies live in
//! `avatar`.
//!
//! - [`input`]: key bindings and when input is live.
//! - [`view`]: the camera rig, sensitivity and field of view.

mod input;
mod view;

pub use input::GameInput;
pub use view::{ViewRig, ViewSettings};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_ahoy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    config::{CROUCH_SPEED_SCALE, GRAVITY, JUMP_HEIGHT, KILL_HEIGHT, KNIFE_SPEED_SCALE, RUN_SPEED},
    console::{ConsoleAppExt, ConsoleCommand, Print},
    net::{MoveInput, Pose, Respawn, Suicide},
    physics::Layer,
    player::input::PlayerInput,
    render::{self, WorldCamera},
    weapon::{Arsenal, WeaponKind},
};

pub(super) fn plugin(app: &mut App) {
    app.add_plugins((input::plugin, view::plugin))
        .init_resource::<MoveLimits>()
        .add_systems(Startup, spawn_local_player)
        .add_systems(First, reset_limits)
        .add_systems(PreUpdate, apply_respawn.after(ClientSystems::Receive))
        .add_systems(Update, (send_move, fall_out, player_commands))
        .add_systems(PostUpdate, apply_limits);

    app.console_command("kill", "", "respawn somewhere else")
        .console_command("pos", "", "print your position and view angles");
}

/// Run a little faster with the knife out, as in Counter-Strike.
/// Limits a game mode puts on the local player this frame: a speed scale,
/// whether they may jump, and pose flags to show (such as `Pose::DOWN`).
/// Back to "no limits" at the start of every frame, so a mode sets what it
/// wants each frame, in `PreUpdate`.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct MoveLimits {
    pub speed: f32,
    pub jump: bool,
    /// See from crouching height, standing or not (kneeling, downed).
    pub low: bool,
    pub flags: u8,
}

impl Default for MoveLimits {
    fn default() -> Self {
        Self {
            speed: 1.0,
            jump: true,
            low: false,
            flags: 0,
        }
    }
}

fn reset_limits(mut limits: ResMut<MoveLimits>) {
    limits.set_if_neq(MoveLimits::default());
}

/// Running speed (a little faster with the knife, as in Counter-Strike, and
/// as a game mode's upgrades and limits say) and whether you can jump.
fn apply_limits(
    arsenal: Res<Arsenal>,
    limits: Res<MoveLimits>,
    mut controller: Single<&mut CharacterController, With<LocalPlayer>>,
    mut standing: Local<Option<f32>>,
) {
    let standing = *standing.get_or_insert(controller.standing_view_height);
    let view = if limits.low {
        controller.crouch_view_height
    } else {
        standing
    };
    if controller.standing_view_height != view {
        controller.standing_view_height = view;
    }
    let knife = if arsenal.current == WeaponKind::Knife {
        KNIFE_SPEED_SCALE
    } else {
        1.0
    };
    let speed = RUN_SPEED * knife * arsenal.mods.speed * limits.speed;
    if controller.speed != speed {
        controller.speed = speed;
    }
    let jump = if limits.jump { JUMP_HEIGHT } else { 0.0 };
    if controller.jump_height != jump {
        controller.jump_height = jump;
    }
}

/// The kinematic character controller owned by this machine. Never replicated.
#[derive(Component)]
pub struct LocalPlayer;

fn spawn_local_player(mut commands: Commands) {
    let start = Vec3::new(0.0, 1.2, 20.0);
    let player = commands
        .spawn((
            Name::new("Local player"),
            LocalPlayer,
            PlayerInput,
            CharacterController {
                // Barriers too: an open breach in the wall stops only you.
                filter: SpatialQueryFilter::from_mask([Layer::World, Layer::Barrier]),
                speed: RUN_SPEED,
                crouch_speed_scale: CROUCH_SPEED_SCALE,
                jump_height: JUMP_HEIGHT,
                gravity: GRAVITY,
                ..default()
            },
            RigidBody::Kinematic,
            Collider::cylinder(0.4, 1.8),
            CollisionLayers::new(Layer::LocalBody, [Layer::World, Layer::Barrier]),
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
        .with_child((Name::new("Player camera"), render::world_camera()));
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
    limits: Res<MoveLimits>,
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
    pose.set(Pose::HOLSTER, arsenal.holstered());
    pose.flags |= limits.flags;
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
    local: Query<&Transform, With<LocalPlayer>>,
    cameras: Query<&GlobalTransform, With<WorldCamera>>,
) {
    for cmd in cmds.read() {
        match cmd.name.as_str() {
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
