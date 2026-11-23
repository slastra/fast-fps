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
mod ragdoll;
pub mod rig;

use bevy::prelude::*;

use crate::{
    net::{Damaged, Loadout, LocalId, Player, ShotFx},
    player::smooth_remote,
};
use anim::AvatarAnim;
use rig::AvatarRig;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<ragdoll::Ragdolls>()
        .add_observer(dress_player)
        .add_systems(
            Update,
            (shots, damaged, anim::advance, anim::apply)
                .chain()
                .after(smooth_remote),
        )
        .add_systems(Update, ragdoll::age);
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
