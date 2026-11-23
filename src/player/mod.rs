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
    config::{CROUCH_SPEED_SCALE, GRAVITY, JUMP_HEIGHT, KILL_HEIGHT, RUN_SPEED},
    console::{ConsoleAppExt, ConsoleCommand, Print},
    net::{MoveInput, Pose, Respawn, Suicide},
    physics::Layer,
    player::input::PlayerInput,
    render::{self, WorldCamera},
    weapon::Arsenal,
};

pub(super) fn plugin(app: &mut App) {
    app.add_plugins((input::plugin, view::plugin))
        .add_systems(Startup, spawn_local_player)
        .add_systems(PreUpdate, apply_respawn.after(ClientSystems::Receive))
        .add_systems(Update, (send_move, fall_out, player_commands));

    app.console_command("kill", "", "respawn somewhere else")
        .console_command("pos", "", "print your position and view angles");
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
                filter: SpatialQueryFilter::from_mask(Layer::World),
                speed: RUN_SPEED,
                crouch_speed_scale: CROUCH_SPEED_SCALE,
                jump_height: JUMP_HEIGHT,
                gravity: GRAVITY,
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
