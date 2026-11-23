//! Practice dummies: spawning them, sliding them, and standing them back up.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    config::MAX_HEALTH,
    net::{Downed, Dummy, Health, Pose},
    server::players::hitbox,
    world::DUMMIES,
};

/// Put the practice dummies out if they aren't there yet.
pub fn ensure_dummies(mut commands: Commands, dummies: Query<(), With<Dummy>>) {
    if !dummies.is_empty() {
        return;
    }
    for (anchor, sway) in DUMMIES {
        commands.spawn((
            Replicated,
            Dummy { anchor, sway },
            Health(MAX_HEALTH),
            Pose {
                pos: anchor,
                yaw: std::f32::consts::FRAC_PI_2,
                ..default()
            },
            Transform::from_translation(anchor),
            hitbox(),
        ));
    }
}

#[derive(Component)]
pub struct DownTimer(pub Timer);

pub fn move_dummies(time: Res<Time>, mut dummies: Query<(&Dummy, &mut Pose), Without<Downed>>) {
    let t = time.elapsed_secs();
    for (dummy, mut pose) in &mut dummies {
        if dummy.sway == 0.0 {
            continue;
        }
        let phase = dummy.anchor.z * 0.7;
        pose.pos = dummy.anchor + Vec3::Z * (t * 0.9 + phase).sin() * dummy.sway;
    }
}

pub fn tick_downed(
    time: Res<Time>,
    mut commands: Commands,
    mut dummies: Query<(Entity, &mut DownTimer, &mut Health)>,
) {
    for (entity, mut timer, mut health) in &mut dummies {
        if timer.0.tick(time.delta()).just_finished() {
            health.0 = MAX_HEALTH;
            commands.entity(entity).remove::<(Downed, DownTimer)>();
        }
    }
}
