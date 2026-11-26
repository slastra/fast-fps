//! What a player has in survival: their cards (`Upgrades`) turned into the
//! core's `Kit` (weapons owned, rounds left) and `WeaponMods`, their full
//! health ([`Vigor`]), and a cached [`Build`] the other systems read.

use bevy::prelude::*;

use crate::{
    config::{MAX_HEALTH, SURVIVAL_START_MAGS},
    net::{Health, Kit, WeaponMods},
    survival::{
        cards::{Ammo, Build, LEVELED, Perk},
        protocol::{Offer, Upgrades},
    },
    weapon::WeaponKind,
};

/// Server-only: a player's health when full (the Juggernaut perk raises it).
#[derive(Component, Clone, Copy)]
pub struct Vigor(pub f32);

/// What a player starts a run with: no cards, a Glock and the knife, full
/// health.
pub fn fresh() -> impl Bundle {
    (
        Upgrades::default(),
        Build::from_cards(&[]),
        Offer::default(),
        Vigor(MAX_HEALTH),
        starting_kit(),
        WeaponMods::default(),
    )
}

/// A Glock with spare magazines, and the knife.
fn starting_kit() -> Kit {
    let mut kit = Kit::only(&[WeaponKind::Knife]);
    kit.give(WeaponKind::Glock, rounds_for(WeaponKind::Glock, None));
    kit
}

/// Rounds a newly owned weapon comes with: a full magazine and spares (or
/// two grenades).
fn rounds_for(weapon: WeaponKind, mods: Option<&WeaponMods>) -> u16 {
    if weapon == WeaponKind::Grenade {
        return 2;
    }
    let mag = WeaponMods::mag_of(mods, weapon) as u16;
    mag * (1 + SURVIVAL_START_MAGS)
}

/// What a build does to weapons.
pub fn mods_of(build: &Build) -> WeaponMods {
    let mut m = WeaponMods::default();
    let bit = |w: WeaponKind| 1u16 << w.index();
    for w in LEVELED {
        let i = w.index();
        let level = build.level(w);
        if level >= 1 {
            m.damage[i] = 1.3;
        }
        if level >= 2 {
            if w != WeaponKind::Flintlock {
                m.mag[i] = 1.5;
            }
            m.reload[i] = 0.75;
        }
    }
    if build.level(WeaponKind::Glock) >= 3 {
        m.automatic |= bit(WeaponKind::Glock);
    }
    if build.level(WeaponKind::Mp5) >= 3 {
        m.steady |= bit(WeaponKind::Mp5);
    }
    if build.level(WeaponKind::Ak) >= 3 {
        m.settle |= bit(WeaponKind::Ak);
    }
    if build.level(WeaponKind::Sniper) >= 3 {
        m.pierce[WeaponKind::Sniper.index()] += 3;
    }
    if build.level(WeaponKind::Flintlock) >= 3 {
        // Eight smaller balls instead of one big one.
        let i = WeaponKind::Flintlock.index();
        m.pellets[i] = 8;
        m.damage[i] *= 0.3;
    }
    if build.loaded(Ammo::Piercing) {
        for w in LEVELED {
            m.pierce[w.index()] += 1;
        }
    }
    if build.has(Perk::QuickHands) {
        for r in &mut m.reload {
            *r *= 0.6;
        }
        m.swap = 0.6;
    }
    if build.has(Perk::Deadeye) {
        m.head = 1.5;
    }
    if build.has(Perk::Butcher) {
        m.damage[WeaponKind::Knife.index()] = 1.5;
    }
    if build.has(Perk::FleetFoot) {
        m.speed = 1.15;
    }
    m
}

/// Keep every player's `Build`, `Kit`, `WeaponMods` and full health in step
/// with their cards. A weapon they've just been given comes with spare
/// rounds.
#[allow(clippy::type_complexity)]
pub fn equip_all(
    mut players: Query<
        (
            &Upgrades,
            &mut Build,
            &mut Kit,
            &mut WeaponMods,
            &mut Vigor,
            &mut Health,
        ),
        Changed<Upgrades>,
    >,
) {
    for (upgrades, mut build, mut kit, mut mods, mut vigor, mut health) in &mut players {
        build.set_if_neq(Build::from_cards(&upgrades.0));
        let new_mods = mods_of(&build);
        equip(&mut kit, &build, &new_mods);
        if *mods != new_mods {
            *mods = new_mods;
        }
        let max = MAX_HEALTH
            + if build.has(Perk::Juggernaut) {
                50.0
            } else {
                0.0
            };
        if vigor.0 != max {
            // More health to have: fill the difference.
            health.0 += (max - vigor.0).max(0.0);
            vigor.0 = max;
        }
    }
}

/// Own what the build says, and give each newly owned weapon its rounds.
fn equip(kit: &mut Kit, build: &Build, mods: &WeaponMods) {
    for w in WeaponKind::ALL {
        if build.owns(w) && !kit.owns(w) {
            kit.give(w, rounds_for(w, Some(mods)));
        }
    }
}

/// Spare magazines for every gun a player owns (ammo boxes, breathers, the
/// Supplies card), and grenades if they have them.
pub fn resupply(kit: &mut Kit, mods: &WeaponMods, mags: f32) {
    for w in WeaponKind::ALL {
        if !kit.owns(w) || w == WeaponKind::Knife {
            continue;
        }
        // Two grenades for each magazine's worth (so two a breather).
        let rounds = if w == WeaponKind::Grenade {
            (mags * 2.0).round().max(1.0) as u16
        } else {
            (WeaponMods::mag_of(Some(mods), w) as f32 * mags).round() as u16
        };
        kit.add(w, rounds);
    }
}
