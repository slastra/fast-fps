//! Other players' bodies: a jointed low-poly skeleton, procedural animation
//! driven by the replicated pose, hit reactions, and death ragdolls.
//!
//! - [`rig`]: the skeleton and its boxes.
//! - [`ik`]: two-bone inverse kinematics.
//! - [`anim`]: animation parameters and the pose solver.
//! - [`ragdoll`]: physics corpses.
//!
//! Your own body isn't drawn; you only see your view model.

pub mod anim;
mod ik;
pub mod mob;
mod ragdoll;
pub mod rig;

pub use ragdoll::Ragdoll;

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::net::{Damaged, Dead, Dummy, Loadout, LocalId, Mob, Out, Player, Pose, ShotFx};
use anim::AvatarAnim;
use rig::AvatarRig;

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(mob::plugin)
        .init_resource::<ragdoll::Ragdolls>()
        .add_observer(dress_player)
        .add_systems(
            Update,
            (smooth_remote, shots, damaged, anim::advance, anim::apply).chain(),
        )
        .add_systems(Update, (ragdoll::age, hide_out));
}

/// Players who are out of play (`Out`: dead for the rest of a round,
/// watching, bled out) aren't drawn, nor are dead mobs (their ragdoll is).
/// Bodies are under the entity, so hiding it hides them.
fn hide_out(
    mut bodies: Query<(&mut Visibility, Has<Out>, Has<Dead>), Or<(With<Player>, With<Mob>)>>,
) {
    for (mut visibility, out, dead) in &mut bodies {
        visibility.set_if_neq(if out || dead {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

/// Give every player but yourself a body.
fn dress_player(
    add: On<Add<Player>>,
    players: Query<&Player>,
    local: Res<LocalId>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(player) = players.get(add.entity) else {
        return;
    };
    if player.owner == local.0 {
        return;
    }
    let rig = rig::spawn(
        &mut commands,
        add.entity,
        player.owner,
        &mut meshes,
        &mut materials,
    );
    commands
        .entity(add.entity)
        .insert((rig, AvatarAnim::default()));
}

/// Kick the gun of whoever fired.
fn shots(mut shots: MessageReader<ShotFx>, mut avatars: Query<(&Player, &mut AvatarAnim)>) {
    for shot in shots.read() {
        for (player, mut anim) in &mut avatars {
            if player.owner == shot.shooter {
                anim.recoil = 1.0;
            }
        }
    }
}

/// Flinch when hit; on a kill, leave a ragdoll and hide until respawned.
fn damaged(
    mut hits: MessageReader<Damaged>,
    mut avatars: Query<(&AvatarRig, &mut AvatarAnim, Option<&Loadout>)>,
    mut commands: Commands,
    mut ragdolls: ResMut<ragdoll::Ragdolls>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for hit in hits.read() {
        let Ok((rig, mut anim, loadout)) = avatars.get_mut(hit.target) else {
            continue;
        };
        // A shot from the front rocks the upper body back, and so on.
        let local = Quat::from_rotation_y(-hit.yaw) * hit.dir;
        anim.flinch_vel += Vec2::new(local.z, -local.x) * 4.0;

        if hit.killed && anim.hide <= 0.0 {
            // Build the corpse from where the server says they died, in the
            // pose they are in right now. Their entity may already have moved.
            let (frames, gun) = anim::compute(&anim).frames(hit.pos, hit.yaw);
            ragdoll::spawn(
                &mut commands,
                &mut ragdolls,
                rig,
                &mut meshes,
                ragdoll::Death {
                    frames,
                    gun,
                    velocity: Quat::from_rotation_y(hit.yaw) * anim.vel,
                    dir: hit.dir,
                    point: hit.point,
                    knock: ragdoll::knock(hit.weapon),
                    held: loadout.map(|l| l.0).unwrap_or_default(),
                },
            );
            anim.hide = 0.6;
            anim.death_pos = hit.pos;
            anim.flinch = Vec2::ZERO;
            anim.flinch_vel = Vec2::ZERO;
        }
    }
}

/// Replicated poses arrive at the server tick rate; ease toward them.
pub fn smooth_remote(
    time: Res<Time>,
    mut remotes: Query<
        (&Pose, &mut Transform),
        (With<Remote>, Or<(With<Player>, With<Dummy>, With<Mob>)>),
    >,
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
