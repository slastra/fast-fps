//! Two-bone inverse kinematics for arms and legs.
//!
//! Bones point down their local -Y and bend about their local X, like a
//! hinge. The solver returns rotations that keep the bend purely about X, so
//! a ragdoll with hinge joints can take over from any solved pose.

use bevy::prelude::*;

/// A rotation whose local -Y points along `dir` and whose local X is as close
/// to `x_hint` as possible.
pub fn basis(dir: Vec3, x_hint: Vec3) -> Quat {
    let y = -dir.try_normalize().unwrap_or(Vec3::NEG_Y);
    let mut x = x_hint - y * x_hint.dot(y);
    if x.length_squared() < 1e-8 {
        x = y.any_orthonormal_vector();
    }
    let x = x.normalize();
    let z = x.cross(y);
    Quat::from_mat3(&Mat3::from_cols(x, y, z))
}

pub struct Limb {
    /// Rotation of the upper bone, in the space the inputs were given in.
    pub upper: Quat,
    /// Rotation of the lower bone, in the same space.
    pub lower: Quat,
}

/// Solve a limb from `root` to `target` with bone lengths `l1` and `l2`.
/// The middle joint bends toward `pole`. `bend` is +1 if the joint flexes by
/// a positive rotation about local X (elbows) and -1 if negative (knees).
pub fn two_bone(root: Vec3, target: Vec3, l1: f32, l2: f32, pole: Vec3, bend: f32) -> Limb {
    let to = target - root;
    let dist = to.length();
    let t = if dist > 1e-5 { to / dist } else { Vec3::NEG_Y };
    let d = dist.clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3);

    let mut p = pole - t * pole.dot(t);
    if p.length_squared() < 1e-8 {
        p = t.any_orthonormal_vector();
    }
    let p = p.normalize();

    // Law of cosines for the angle at the root.
    let cos_a = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).sqrt();
    let joint = root + (t * cos_a + p * sin_a) * l1;
    let end = root + t * d;

    let axis = p.cross(t) * bend;
    Limb {
        upper: basis(joint - root, axis),
        lower: basis(end - joint, axis),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tip(root: Vec3, limb: &Limb, l1: f32, l2: f32) -> Vec3 {
        root + limb.upper * Vec3::NEG_Y * l1 + limb.lower * Vec3::NEG_Y * l2
    }

    #[test]
    fn rest_pose_is_identity() {
        let q = basis(Vec3::NEG_Y, Vec3::X);
        assert!(q.angle_between(Quat::IDENTITY) < 1e-4);
    }

    #[test]
    fn reaches_reachable_targets() {
        let root = Vec3::new(0.1, 0.7, 0.0);
        for target in [
            Vec3::new(0.1, 0.1, -0.2),
            Vec3::new(0.3, 0.3, 0.3),
            Vec3::new(-0.2, 0.9, -0.3),
        ] {
            let limb = two_bone(root, target, 0.37, 0.37, Vec3::NEG_Z, -1.0);
            assert!(tip(root, &limb, 0.37, 0.37).distance(target) < 1e-3);
        }
    }

    #[test]
    fn clamps_unreachable_targets() {
        let limb = two_bone(Vec3::ZERO, Vec3::NEG_Y * 5.0, 0.3, 0.3, Vec3::NEG_Z, -1.0);
        let end = tip(Vec3::ZERO, &limb, 0.3, 0.3);
        assert!(end.is_finite());
        assert!((end.length() - 0.6).abs() < 2e-3);
    }

    /// The lower bone must differ from the upper only by a turn about X, with
    /// the sign the hinge limits expect.
    #[test]
    fn bend_is_a_pure_hinge() {
        for (bend, pole) in [(-1.0, Vec3::NEG_Z), (1.0, Vec3::new(0.5, -1.0, 0.4))] {
            let limb = two_bone(
                Vec3::ZERO,
                Vec3::new(0.1, -0.4, -0.15),
                0.3,
                0.3,
                pole,
                bend,
            );
            let local = limb.upper.inverse() * limb.lower;
            let local = if local.w < 0.0 { -local } else { local };
            let (axis, angle) = local.to_axis_angle();
            assert!(angle > 0.1);
            // Axis is +X or -X; fold the sign into the angle.
            assert!(axis.y.abs() < 1e-3 && axis.z.abs() < 1e-3);
            let signed = angle * axis.x.signum();
            assert!(signed * bend > 0.0, "bend {bend} gave angle {signed}");
        }
    }
}
