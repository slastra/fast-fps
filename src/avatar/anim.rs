//! Procedural animation.
//!
//! Nothing here is keyframed. Each avatar carries a small set of smoothed
//! parameters ([`AvatarAnim`]) driven by its replicated [`Pose`], and
//! [`compute`] turns those into a full-body pose every frame:
//!
//! 1. Hips: height (crouch, stride bob, landing), lean and twist.
//! 2. Feet: stride targets along the movement direction, or a tuck in the
//!    air. Legs are solved to them with IK, so walking, strafing,
//!    backpedaling and crouch-walking all come from the same code.
//! 3. Torso and head: aim pitch, crouch lean, hit flinch.
//! 4. Gun: held in front of the chest along the aim direction, with recoil
//!    and reload layered on. Arms are solved to its grips with IK.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;

use super::{
    ik,
    rig::{ARM, AvatarRig, BONE_COUNT, Bone, FEET_Y, LEG},
};
use crate::{
    net::{Loadout, Pose},
    weapon::WeaponKind,
};

/// Speed at which the run cycle is at full stretch.
const RUN_SPEED: f32 = 10.0;
/// How far the hips drop in a full crouch.
const CROUCH_DROP: f32 = 0.36;
/// How far the hips drop kneeling (downed).
const KNEEL_DROP: f32 = 0.5;

#[derive(Component)]
pub struct AvatarAnim {
    /// Stride phase in cycles; one cycle is a left and a right step.
    pub phase: f32,
    /// Smoothed velocity in body space (x right, -z forward).
    pub vel: Vec3,
    pub crouch: f32,
    pub air: f32,
    pub reload: f32,
    /// 1 while a duelist's pistol is holstered, easing to 0 as it's drawn.
    pub holster: f32,
    /// 1 down on one knee.
    pub down: f32,
    /// 1 with both arms reaching forward (a mob), and 1 mid-swing.
    pub reach: f32,
    pub swing: f32,
    /// Aim pitch in radians, positive up.
    pub pitch: f32,
    /// 1 right after a shot, decaying to 0.
    pub recoil: f32,
    /// Upper-body knock from being hit: (pitch, roll), a damped spring.
    pub flinch: Vec2,
    pub flinch_vel: Vec2,
    /// 1 right after landing, decaying to 0.
    pub land: f32,
    /// Seconds alive, for breathing.
    pub time: f32,
    pub weapon: WeaponKind,
    /// Seconds left to stay hidden after dying, until the pose jumps away.
    pub hide: f32,
    pub death_pos: Vec3,
    was_air: bool,
}

impl Default for AvatarAnim {
    fn default() -> Self {
        Self {
            phase: 0.0,
            vel: Vec3::ZERO,
            crouch: 0.0,
            air: 0.0,
            reload: 0.0,
            holster: 0.0,
            down: 0.0,
            reach: 0.0,
            swing: 0.0,
            pitch: 0.0,
            recoil: 0.0,
            flinch: Vec2::ZERO,
            flinch_vel: Vec2::ZERO,
            land: 0.0,
            time: 0.0,
            weapon: WeaponKind::Ak,
            hide: 0.0,
            death_pos: Vec3::ZERO,
            was_air: false,
        }
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Full stride cycles per second at a given speed.
fn stride_freq(speed: f32) -> f32 {
    1.1 + 1.7 * (1.0 - (-speed / 5.0).exp())
}

/// Move the parameters toward what the replicated pose says.
pub fn advance(
    time: Res<Time>,
    mut avatars: Query<(&Pose, Option<&Loadout>, &AvatarRig, &mut AvatarAnim)>,
    mut visibility: Query<&mut Visibility>,
) {
    // Clamp so a hitch can't blow up the flinch spring.
    let dt = time.delta_secs().min(1.0 / 30.0);
    for (pose, loadout, rig, mut anim) in &mut avatars {
        let a = &mut *anim;
        a.time += dt;
        if let Some(loadout) = loadout {
            a.weapon = loadout.0;
        }

        let local_vel = Quat::from_rotation_y(-pose.yaw) * pose.vel;
        a.vel.smooth_nudge(&local_vel, 12.0, dt);
        a.pitch.smooth_nudge(&pose.pitch, 20.0, dt);

        let flag = |f: u8| if pose.has(f) { 1.0 } else { 0.0 };
        a.crouch.smooth_nudge(&flag(Pose::CROUCH), 12.0, dt);
        a.air.smooth_nudge(&flag(Pose::AIR), 16.0, dt);
        a.reload.smooth_nudge(&flag(Pose::RELOAD), 9.0, dt);
        // About as quick as the draw itself (`DUEL_RAISE_SECS`).
        a.holster.smooth_nudge(&flag(Pose::HOLSTER), 8.0, dt);
        a.down.smooth_nudge(&flag(Pose::DOWN), 6.0, dt);
        a.reach.smooth_nudge(&flag(Pose::REACH), 10.0, dt);
        a.swing.smooth_nudge(&flag(Pose::SWING), 14.0, dt);

        let speed = Vec2::new(a.vel.x, a.vel.z).length();
        a.phase = (a.phase + stride_freq(speed) * smoothstep(0.1, 0.6, speed) * dt).fract();

        let in_air = pose.has(Pose::AIR);
        if a.was_air && !in_air {
            a.land = (0.35 + (-pose.vel.y).max(0.0) / 14.0).min(1.0);
        }
        a.was_air = in_air;
        a.land.smooth_nudge(&0.0, 7.0, dt);
        a.recoil.smooth_nudge(&0.0, 14.0, dt);

        let accel = -150.0 * a.flinch - 15.0 * a.flinch_vel;
        a.flinch_vel += accel * dt;
        a.flinch += a.flinch_vel * dt;

        // Stay hidden after dying until the server moves us to a new spawn.
        if a.hide > 0.0 {
            a.hide -= dt;
            if pose.pos.distance(a.death_pos) > 1.5 {
                a.hide = 0.0;
            }
        }
        if let Ok(mut vis) = visibility.get_mut(rig.root) {
            vis.set_if_neq(if a.hide > 0.0 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        }
    }
}

/// How a weapon is held, relative to the chest and the aim direction.
struct Hold {
    /// Gun origin from the chest pivot, in aim space.
    offset: Vec3,
    /// Hand positions in gun space.
    grip_r: Vec3,
    grip_l: Vec3,
    /// How far the torso turns the support shoulder forward.
    blade: f32,
}

fn hold(weapon: WeaponKind) -> Hold {
    let rifle = |fore: f32| Hold {
        offset: Vec3::new(0.08, 0.05, -0.16),
        grip_r: Vec3::new(0.0, -0.07, 0.03),
        grip_l: Vec3::new(0.0, -0.04, -fore),
        blade: 0.45,
    };
    match weapon {
        WeaponKind::Ak => rifle(0.32),
        WeaponKind::Mp5 => rifle(0.28),
        WeaponKind::Shotgun => rifle(0.33),
        WeaponKind::Sniper => rifle(0.31),
        WeaponKind::Glock => Hold {
            offset: Vec3::new(0.02, 0.12, -0.42),
            grip_r: Vec3::new(0.01, -0.07, 0.03),
            grip_l: Vec3::new(-0.03, -0.09, 0.03),
            blade: 0.0,
        },
        // One-handed, low and forward; the free hand rests by the chest.
        WeaponKind::Knife => Hold {
            offset: Vec3::new(0.12, -0.02, -0.34),
            grip_r: Vec3::new(0.0, -0.004, 0.01),
            grip_l: Vec3::new(-0.24, -0.06, 0.16),
            blade: 0.1,
        },
        WeaponKind::Grenade => Hold {
            offset: Vec3::new(0.1, 0.06, -0.3),
            grip_r: Vec3::new(0.0, -0.03, 0.01),
            grip_l: Vec3::new(-0.22, -0.08, 0.12),
            blade: 0.1,
        },
        // A duelist's stance: one arm out at shoulder height, the other hand
        // tucked at the small of the back.
        WeaponKind::Flintlock => Hold {
            offset: Vec3::new(0.14, 0.14, -0.48),
            grip_r: Vec3::new(0.0, -0.05, 0.06),
            grip_l: Vec3::new(-0.3, -0.3, 0.42),
            blade: -0.15,
        },
    }
}

/// Local transforms for every bone, plus the gun in avatar-root space.
pub struct RigPose {
    pub bones: [Transform; BONE_COUNT],
    pub gun: Transform,
}

pub fn compute(a: &AvatarAnim) -> RigPose {
    let mut bones = [Transform::IDENTITY; BONE_COUNT];
    for (i, def) in super::rig::SKELETON.iter().enumerate() {
        bones[i].translation = Vec3::from(def.offset);
    }
    let hold = hold(a.weapon);

    // --- Locomotion parameters -------------------------------------------
    let planar = Vec3::new(a.vel.x, 0.0, a.vel.z);
    let speed = planar.length();
    let dir = if speed > 0.05 {
        planar / speed
    } else {
        Vec3::NEG_Z
    };
    let ground = 1.0 - a.air;
    let moving = smoothstep(0.15, 1.0, speed) * ground;
    let gait = (speed / RUN_SPEED).clamp(0.0, 1.0);
    // Sized so a planted foot matches ground speed mid-stance, up to the
    // reach of the leg. Past that the feet slide, which reads fine at a sprint.
    let stride = (speed / (TAU * stride_freq(speed))).min(0.36) * (1.0 - 0.3 * a.crouch) * ground;
    let lift = (0.05 + 0.2 * gait) * moving;
    let cycle = a.phase * TAU;
    let breathe = (a.time * 1.6).sin();
    let forward = (-a.vel.z / RUN_SPEED).clamp(-1.0, 1.0);
    let sideways = (a.vel.x / RUN_SPEED).clamp(-1.0, 1.0);

    // --- Hips ------------------------------------------------------------
    // Lowest when the feet are furthest apart, twice per cycle.
    let bob = -(0.012 + 0.03 * gait) * moving * (2.0 * cycle).cos();
    let hips_pos = Vec3::new(
        0.0,
        Bone::Hips.offset().y - CROUCH_DROP * a.crouch - 0.04 * gait * ground + bob - 0.14 * a.land
            + 0.004 * breathe
            - KNEEL_DROP * a.down,
        0.05 * a.crouch,
    );
    // Swing the hip of the leading leg forward.
    let twist = 0.1 * moving * cycle.cos() * dir.z;
    let hips_rot = Quat::from_rotation_y(twist)
        * Quat::from_rotation_x(-0.1 * forward * ground)
        * Quat::from_rotation_z(-0.08 * sideways * ground);
    bones[Bone::Hips.index()] = Transform {
        translation: hips_pos,
        rotation: hips_rot,
        ..default()
    };

    // --- Legs ------------------------------------------------------------
    // Rising: legs trail. Falling: knees come up.
    let tuck = (0.2 - a.vel.y * 0.02).clamp(0.08, 0.36);
    for (side, thigh, shin, offset) in [
        (-1.0, Bone::ThighL, Bone::ShinL, 0.0),
        (1.0, Bone::ThighR, Bone::ShinR, PI),
    ] {
        let (sin, cos) = (cycle + offset).sin_cos();
        // At rest the support-side foot sits a little forward.
        let rest = Vec3::new(
            side * (0.11 + 0.04 * a.crouch),
            FEET_Y,
            side * 0.06 * (1.0 - moving),
        );
        // Stance half of the cycle slides the foot back; swing lifts it forward.
        let planted = rest + dir * (stride * cos) + Vec3::Y * (lift * (-sin).max(0.0));
        let airborne = Vec3::new(
            side * 0.12,
            FEET_Y + tuck + if side > 0.0 { 0.1 } else { 0.0 },
            if side > 0.0 { 0.14 } else { -0.16 },
        );
        let foot = planted.lerp(airborne, a.air);
        let hip = hips_pos + hips_rot * thigh.offset();
        let pole = hips_rot * Vec3::new(side * 0.2, 0.0, -1.0);
        let limb = ik::two_bone(hip, foot, LEG.0, LEG.1, pole, -1.0);
        bones[thigh.index()].rotation = hips_rot.inverse() * limb.upper;
        bones[shin.index()].rotation = limb.upper.inverse() * limb.lower;
    }

    // --- Torso and head --------------------------------------------------
    // Lean into the crouch, follow part of the aim pitch, and cancel the hip
    // twist so the shoulders stay square to the target.
    let torso_local =
        Quat::from_rotation_x(-0.3 * a.crouch - 0.45 * a.down + 0.3 * a.pitch + a.flinch.x)
            * Quat::from_rotation_z(a.flinch.y)
            * Quat::from_rotation_y(-hold.blade - twist);
    let torso_rot = hips_rot * torso_local;
    let torso_pos = hips_pos + hips_rot * Bone::Torso.offset();
    bones[Bone::Torso.index()].rotation = torso_local;

    let aim = Quat::from_rotation_x(a.pitch);
    let head_rot =
        aim * Quat::from_rotation_x(0.7 * a.flinch.x) * Quat::from_rotation_z(0.7 * a.flinch.y);
    bones[Bone::Head.index()].rotation = torso_rot.inverse() * head_rot;

    // --- Gun and arms ----------------------------------------------------
    let reload = smoothstep(0.0, 1.0, a.reload);
    let chest = torso_pos + torso_rot * Vec3::new(0.0, 0.42, 0.0);
    let gun_rot = aim
        * Quat::from_rotation_x(0.12 * a.recoil - 0.45 * reload)
        * Quat::from_rotation_z(-0.5 * reload);
    let gun_pos = chest
        + aim
            * (hold.offset
                + Vec3::new(
                    0.0,
                    0.005 * breathe - 0.06 * reload,
                    0.05 * a.recoil + 0.03 * reload,
                ));

    let support = gun_pos + gun_rot * hold.grip_l;
    // Holstered: the hand rests at the right hip with the pistol hanging
    // muzzle down, then swings it up into aim as it's drawn.
    let holster = smoothstep(0.0, 1.0, a.holster);
    let (gun_pos, gun_rot) = if holster > 0.0 {
        let hip_hand = hips_pos + hips_rot * Vec3::new(0.27, -0.04, -0.02);
        let hip_rot = hips_rot * Quat::from_rotation_x(-1.35);
        let hip_gun = hip_hand - hip_rot * hold.grip_r;
        (
            gun_pos.lerp(hip_gun, holster),
            gun_rot.slerp(hip_rot, holster),
        )
    } else {
        (gun_pos, gun_rot)
    };
    let right_hand = gun_pos + gun_rot * hold.grip_r;
    // While reloading the support hand works the magazine.
    let magazine = gun_pos + gun_rot * Vec3::new(0.0, -0.2 + 0.04 * (a.time * 9.0).sin(), -0.14);
    let left_hand = support.lerp(magazine, reload);
    // A mob reaches forward with both hands, and raises them to swing.
    let reach = smoothstep(0.0, 1.0, a.reach);
    let (left_hand, right_hand) = if reach > 0.0 {
        let out = |side: f32, arm: Bone| {
            torso_pos
                + torso_rot
                    * (arm.offset()
                        + Vec3::new(side * 0.04, -0.06 + 0.32 * a.swing, -0.56 + 0.12 * a.swing))
        };
        (
            left_hand.lerp(out(-1.0, Bone::UpperArmL), reach),
            right_hand.lerp(out(1.0, Bone::UpperArmR), reach),
        )
    } else {
        (left_hand, right_hand)
    };

    for (side, upper, fore, hand) in [
        (-1.0, Bone::UpperArmL, Bone::ForearmL, left_hand),
        (1.0, Bone::UpperArmR, Bone::ForearmR, right_hand),
    ] {
        let shoulder = torso_pos + torso_rot * upper.offset();
        // Elbows point down, out and a little back.
        let pole = torso_rot * Vec3::new(side * 0.6, -1.0, 0.4);
        let limb = ik::two_bone(shoulder, hand, ARM.0, ARM.1, pole, 1.0);
        bones[upper.index()].rotation = torso_rot.inverse() * limb.upper;
        bones[fore.index()].rotation = limb.upper.inverse() * limb.lower;
    }

    RigPose {
        bones,
        gun: Transform {
            translation: gun_pos,
            rotation: gun_rot,
            ..default()
        },
    }
}

impl RigPose {
    /// World position and rotation of every joint, and of the gun, for an
    /// avatar standing at `root`.
    pub fn frames(&self, root: Vec3, yaw: f32) -> ([(Vec3, Quat); BONE_COUNT], (Vec3, Quat)) {
        let base = Quat::from_rotation_y(yaw);
        let mut out = [(root, base); BONE_COUNT];
        for (i, def) in super::rig::SKELETON.iter().enumerate() {
            let (pos, rot) = def.parent.map_or((root, base), |p| out[p.index()]);
            out[i] = (
                pos + rot * self.bones[i].translation,
                rot * self.bones[i].rotation,
            );
        }
        let gun = (root + base * self.gun.translation, base * self.gun.rotation);
        (out, gun)
    }
}

/// Write the computed pose to the joint entities.
pub fn apply(avatars: Query<(&AvatarRig, &AvatarAnim)>, mut transforms: Query<&mut Transform>) {
    for (rig, anim) in &avatars {
        if anim.hide > 0.0 {
            continue;
        }
        let pose = compute(anim);
        for (entity, local) in rig.bones.iter().zip(pose.bones) {
            if let Ok(mut transform) = transforms.get_mut(*entity) {
                *transform = local;
            }
        }
        if let Ok(mut transform) = transforms.get_mut(rig.gun) {
            *transform = pose.gun;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Root-space position and rotation of every joint.
    fn globals(pose: &RigPose) -> [(Vec3, Quat); BONE_COUNT] {
        pose.frames(Vec3::ZERO, 0.0).0
    }

    fn sweep() -> Vec<AvatarAnim> {
        let mut out = Vec::new();
        for weapon in WeaponKind::ALL {
            for vel in [
                Vec3::ZERO,
                Vec3::new(0.0, 0.0, -3.0),
                Vec3::new(0.0, 0.0, -12.0),
                Vec3::new(9.0, 0.0, 0.0),
                Vec3::new(-4.0, 0.0, 6.0),
                Vec3::new(2.0, 8.0, -5.0),
                Vec3::new(0.0, -20.0, 0.0),
            ] {
                for (crouch, air, reload, holster) in [
                    (0.0, 0.0, 0.0, 0.0),
                    (1.0, 0.0, 0.0, 0.0),
                    (0.0, 1.0, 1.0, 0.0),
                    (0.0, 0.0, 0.0, 1.0),
                    (1.0, 0.0, 0.0, 0.5),
                ] {
                    for pitch in [-1.5, -0.4, 0.0, 0.9, 1.5] {
                        for phase in [0.0, 0.2, 0.5, 0.77] {
                            out.push(AvatarAnim {
                                weapon,
                                vel,
                                crouch,
                                air,
                                reload,
                                holster,
                                pitch,
                                phase,
                                recoil: 1.0,
                                land: 0.5,
                                flinch: Vec2::new(0.3, -0.2),
                                time: 3.3,
                                ..default()
                            });
                        }
                    }
                }
            }
        }
        // A mob reaching and swinging, and someone kneeling (downed).
        let extra: Vec<AvatarAnim> = out
            .iter()
            .step_by(7)
            .map(|a| AvatarAnim {
                reach: 1.0,
                swing: if a.phase > 0.3 { 1.0 } else { 0.0 },
                down: if a.crouch > 0.5 { 1.0 } else { 0.0 },
                weapon: a.weapon,
                vel: a.vel,
                pitch: a.pitch,
                phase: a.phase,
                time: a.time,
                ..default()
            })
            .collect();
        out.extend(extra);
        out
    }

    #[test]
    fn every_pose_is_finite_and_normalized() {
        for anim in sweep() {
            let pose = compute(&anim);
            for t in pose.bones.iter().chain([&pose.gun]) {
                assert!(t.translation.is_finite());
                assert!(t.rotation.is_finite());
                assert!((t.rotation.length() - 1.0).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn feet_stay_above_the_ground() {
        for anim in sweep() {
            let g = globals(&compute(&anim));
            for shin in [Bone::ShinL, Bone::ShinR] {
                let (pos, rot) = g[shin.index()];
                let sole = pos + rot * Vec3::NEG_Y * LEG.1;
                assert!(
                    sole.y > FEET_Y - 0.02,
                    "sole at {} for vel {}",
                    sole.y,
                    anim.vel
                );
            }
        }
    }

    #[test]
    fn idle_stands_at_full_height_and_crouch_fits_the_hitbox() {
        let top = |anim: &AvatarAnim| {
            let g = globals(&compute(anim));
            let (pos, rot) = g[Bone::Head.index()];
            (pos + rot * Vec3::new(0.0, 0.32, 0.0)).y - FEET_Y
        };
        let standing = top(&AvatarAnim::default());
        assert!((1.6..1.8).contains(&standing), "standing {standing}");
        let crouched = top(&AvatarAnim {
            crouch: 1.0,
            ..default()
        });
        assert!((1.15..1.4).contains(&crouched), "crouched {crouched}");
    }

    /// Elbows and knees must only ever bend the way the ragdoll hinges allow.
    #[test]
    fn hinges_stay_inside_their_limits() {
        for anim in sweep() {
            let pose = compute(&anim);
            for (bone, sign) in [
                (Bone::ForearmL, 1.0),
                (Bone::ForearmR, 1.0),
                (Bone::ShinL, -1.0),
                (Bone::ShinR, -1.0),
            ] {
                // Signed turn about X, in (-pi, pi].
                let q = pose.bones[bone.index()].rotation;
                let q = if q.w < 0.0 { -q } else { q };
                let (axis, angle) = q.to_axis_angle();
                if angle < 0.02 {
                    continue;
                }
                assert!(
                    axis.y.abs() < 0.02 && axis.z.abs() < 0.02,
                    "{bone:?} off axis"
                );
                assert!(
                    angle * axis.x.signum() * sign > 0.0,
                    "{bone:?} bent backward: angle {angle} axis {axis} weapon {:?} vel {} crouch {} air {} pitch {}",
                    anim.weapon,
                    anim.vel,
                    anim.crouch,
                    anim.air,
                    anim.pitch
                );
            }
        }
    }

    #[test]
    fn hands_reach_the_gun_when_idle() {
        for weapon in WeaponKind::ALL {
            let anim = AvatarAnim {
                weapon,
                ..default()
            };
            let pose = compute(&anim);
            let g = globals(&pose);
            let h = hold(weapon);
            for (fore, grip) in [(Bone::ForearmL, h.grip_l), (Bone::ForearmR, h.grip_r)] {
                let (pos, rot) = g[fore.index()];
                let hand = pos + rot * Vec3::NEG_Y * ARM.1;
                let target = pose.gun.translation + pose.gun.rotation * grip;
                assert!(
                    hand.distance(target) < 0.03,
                    "{weapon:?} {fore:?} misses by {}",
                    hand.distance(target)
                );
            }
        }
    }
}
