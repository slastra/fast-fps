//! How each kind of zombie looks: colors, the shape the player skeleton is
//! given (`avatar::rig::Look`), and two dot eyes in a color of its own, so
//! kinds can be told apart by their eyes at night.

use bevy::prelude::*;

use super::protocol::{BLOATER, BRUTE, CRAWLER, LEAPER, RUNNER, WALKER};
use crate::avatar::{
    mob::{MobLook, MobLookAppExt},
    rig::{Bone, Extra, Eyes, Look, Paint, extra},
};

const fn zombie(body: Color, trim: Color, eyes: Color) -> Look {
    Look {
        eyes: Eyes::Dots,
        ..Look::new(body, trim, eyes)
    }
}

/// The bloater's swollen belly, with glowing boils (the eye color), and
/// wider hips.
const BELLY: &[Extra] = &[
    extra(
        Bone::Torso,
        [0.66, 0.46, 0.5],
        [0.0, 0.2, -0.12],
        Paint::Team,
    ),
    extra(
        Bone::Torso,
        [0.52, 0.24, 0.4],
        [0.0, -0.02, -0.1],
        Paint::Team,
    ),
    extra(
        Bone::Torso,
        [0.1, 0.1, 0.06],
        [0.2, 0.32, -0.38],
        Paint::Glow,
    ),
    extra(
        Bone::Torso,
        [0.08, 0.08, 0.06],
        [-0.17, 0.12, -0.37],
        Paint::Glow,
    ),
    extra(
        Bone::Torso,
        [0.07, 0.07, 0.06],
        [0.06, 0.4, -0.37],
        Paint::Glow,
    ),
    extra(
        Bone::Hips,
        [0.5, 0.2, 0.32],
        [0.0, 0.02, 0.0],
        Paint::TeamDark,
    ),
];

/// The crawler drags what's left of its hips.
const STUMPS: &[Extra] = &[
    extra(
        Bone::Hips,
        [0.14, 0.12, 0.16],
        [-0.1, -0.09, 0.0],
        Paint::Dark,
    ),
    extra(
        Bone::Hips,
        [0.14, 0.1, 0.16],
        [0.1, -0.08, 0.02],
        Paint::Dark,
    ),
];
const LEGS: &[Bone] = &[Bone::ThighL, Bone::ShinL, Bone::ThighR, Bone::ShinR];

/// The leaper's claws and the ridge of its spine.
const CLAWS: &[Extra] = &[
    extra(
        Bone::ForearmL,
        [0.13, 0.12, 0.15],
        [0.0, -0.34, -0.03],
        Paint::Pale,
    ),
    extra(
        Bone::ForearmR,
        [0.13, 0.12, 0.15],
        [0.0, -0.34, -0.03],
        Paint::Pale,
    ),
    extra(
        Bone::Torso,
        [0.08, 0.42, 0.08],
        [0.0, 0.3, 0.17],
        Paint::Pale,
    ),
];

pub(super) fn plugin(app: &mut App) {
    let looks = [
        (
            WALKER,
            zombie(
                Color::srgb(0.36, 0.45, 0.30),
                Color::srgb(0.22, 0.20, 0.17),
                Color::srgb(1.0, 0.25, 0.15),
            ),
        ),
        (
            RUNNER,
            zombie(
                Color::srgb(0.62, 0.60, 0.38),
                Color::srgb(0.30, 0.18, 0.14),
                Color::srgb(1.0, 0.85, 0.2),
            ),
        ),
        (
            BRUTE,
            zombie(
                Color::srgb(0.33, 0.27, 0.36),
                Color::srgb(0.16, 0.12, 0.14),
                Color::srgb(0.9, 0.2, 1.0),
            ),
        ),
        (
            CRAWLER,
            Look {
                hide: LEGS,
                extras: STUMPS,
                ..zombie(
                    Color::srgb(0.29, 0.37, 0.24),
                    Color::srgb(0.18, 0.16, 0.13),
                    Color::srgb(1.0, 0.25, 0.15),
                )
            },
        ),
        (
            BLOATER,
            Look {
                extras: BELLY,
                ..zombie(
                    Color::srgb(0.49, 0.6, 0.33),
                    Color::srgb(0.24, 0.22, 0.16),
                    Color::srgb(0.6, 0.95, 0.3),
                )
            },
        ),
        (
            LEAPER,
            Look {
                extras: CLAWS,
                ..zombie(
                    Color::srgb(0.42, 0.48, 0.32),
                    Color::srgb(0.2, 0.17, 0.14),
                    Color::srgb(1.0, 0.25, 0.15),
                )
            },
        ),
    ];
    for (kind, look) in looks {
        app.mob_look(kind, MobLook { look, gun: false });
    }
}
