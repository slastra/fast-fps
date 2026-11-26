//! Ammo boxes and medkits: a killed zombie sometimes leaves one (more often
//! for a player with Scavenger), and walking over it takes it.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use rand::Rng;

use super::{Vigor, resupply, zombies::Zombie};
use crate::{
    config::{
        SURVIVAL_AMMO_DROP, SURVIVAL_MED_DROP, SURVIVAL_MEDKIT, SURVIVAL_PICKUP_RANGE,
        SURVIVAL_PICKUP_SECS,
    },
    net::{Health, Kit, Out, Player, Pose, WeaponMods},
    server::rules::Died,
    survival::{
        cards::{Build, Perk},
        protocol::{Fallen, Pickup, PickupKind},
    },
};

/// Server-only: seconds until a pickup is gone.
#[derive(Component)]
pub struct Lifetime(f32);

/// What drops when a zombie is killed by someone with `build`.
pub fn roll(build: Option<&Build>, rng: &mut impl Rng) -> Option<PickupKind> {
    let scavenger = build.is_some_and(|b| b.has(Perk::Scavenger));
    let (ammo, med) = if scavenger {
        (0.45, 0.12)
    } else {
        (SURVIVAL_AMMO_DROP, SURVIVAL_MED_DROP)
    };
    let r = rng.random::<f32>();
    if r < med {
        Some(PickupKind::Medkit)
    } else if r < med + ammo {
        Some(PickupKind::Ammo)
    } else {
        None
    }
}

/// A killed zombie may leave something where it fell.
pub fn drop(
    mut deaths: MessageReader<Died>,
    zombies: Query<&Pose, With<Zombie>>,
    killers: Query<&Build>,
    mut commands: Commands,
) {
    let mut rng = rand::rng();
    for died in deaths.read() {
        let Ok(pose) = zombies.get(died.victim) else {
            continue;
        };
        let Some(kind) = roll(killers.get(died.killer).ok(), &mut rng) else {
            continue;
        };
        let pos = Vec3::new(pose.pos.x, pose.pos.y - 0.75, pose.pos.z);
        commands.spawn((
            Name::new("Pickup"),
            Replicated,
            Pickup { kind, pos },
            Transform::from_translation(pos),
            Lifetime(SURVIVAL_PICKUP_SECS),
        ));
    }
}

/// Walk over a pickup to take it; it's gone after a while either way.
#[allow(clippy::type_complexity)]
pub fn collect(
    time: Res<Time>,
    mut pickups: Query<(Entity, &Pickup, &mut Lifetime)>,
    mut players: Query<
        (&Pose, &Build, &mut Kit, &WeaponMods, &mut Health, &Vigor),
        (With<Player>, Without<Fallen>, Without<Out>),
    >,
    mut commands: Commands,
) {
    for (entity, pickup, mut life) in &mut pickups {
        life.0 -= time.delta_secs();
        if life.0 <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let taker = players
            .iter_mut()
            .find(|p| p.0.pos.distance(pickup.pos + Vec3::Y * 0.9) < SURVIVAL_PICKUP_RANGE);
        let Some((_, build, mut kit, mods, mut health, vigor)) = taker else {
            continue;
        };
        match pickup.kind {
            PickupKind::Ammo => {
                let bandolier = build.has(Perk::Bandolier);
                resupply(&mut kit, mods, if bandolier { 1.5 } else { 1.0 });
            }
            PickupKind::Medkit => {
                if health.0 >= vigor.0 {
                    // Full: leave it for someone who needs it.
                    continue;
                }
                health.0 = (health.0 + SURVIVAL_MEDKIT).min(vigor.0);
            }
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;
    use crate::survival::cards::id_of;

    fn rate(build: Option<&Build>, kind: PickupKind) -> f32 {
        let mut rng = StdRng::seed_from_u64(11);
        let n = 20_000;
        (0..n)
            .filter(|_| roll(build, &mut rng) == Some(kind))
            .count() as f32
            / n as f32
    }

    #[test]
    fn scavengers_find_more() {
        let plain = rate(None, PickupKind::Ammo);
        assert!((plain - SURVIVAL_AMMO_DROP).abs() < 0.02, "{plain}");
        assert_eq!(rate(None, PickupKind::Medkit), 0.0);
        let scav = Build::from_cards(&[id_of("Scavenger")]);
        assert!((rate(Some(&scav), PickupKind::Ammo) - 0.45).abs() < 0.02);
        assert!((rate(Some(&scav), PickupKind::Medkit) - 0.12).abs() < 0.02);
    }
}
