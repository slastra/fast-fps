//! What special ammo, combos and a few perks do. Each hit a player lands on
//! a zombie (`HitLanded`) and each explosion (`Boom`) comes through here; the
//! weapons themselves know nothing of survival.
//!
//! Survival's own explosions (explosive tips, shatters, bomblets) count as
//! grenade damage and carry [`SECONDARY`], so they never set off more ammo
//! effects or more bomblets.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use rand::Rng;

use super::{Vigor, status::Status, zombies::Zombie};
use crate::{
    net::{Dead, Health, Player, Pose},
    server::combat::{Blast, Boom, HitLanded, Hits, blast},
    survival::{
        cards::{Ammo, Build, Combo, Perk},
        protocol::{Fallen, Flash, Upgrades},
    },
    weapon::{Class, WeaponKind},
};

/// `Blast::tag` for survival's own explosions.
pub const SECONDARY: u8 = 1;

/// Server-only: burning ground (Napalm) that sets zombies in it alight.
#[derive(Component)]
pub struct FirePatch {
    at: Vec3,
    by: Entity,
    left: f32,
    tick: f32,
}

/// Server-only: a bomblet from a Cluster grenade, about to go off.
#[derive(Component)]
pub struct Bomblet {
    at: Vec3,
    by: Entity,
    left: f32,
}

/// Damage dealt by an effect, applied after reading every hit.
struct Extra {
    by: Entity,
    target: Entity,
    amount: f32,
    weapon: WeaponKind,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn on_hit(
    // `Hits` writes `HitLanded` too, so the two take turns.
    mut io: ParamSet<(MessageReader<HitLanded>, Hits)>,
    attackers: Query<&Upgrades>,
    vigor: Query<&Vigor>,
    mut zombies: Query<(&mut Status, &mut Zombie), Without<Dead>>,
    near: Query<(Entity, &Pose), (With<Zombie>, Without<Dead>)>,
    allies: Query<(Entity, &Pose), (With<Player>, Without<Fallen>)>,
    spatial: SpatialQuery,
    mut flash: MessageWriter<ToClients<Flash>>,
    mut commands: Commands,
) {
    let landed: Vec<HitLanded> = io.p0().read().cloned().collect();
    if landed.is_empty() {
        return;
    }
    let mut rng = rand::rng();
    let mut extra: Vec<Extra> = Vec::new();
    let mut blasts: Vec<(Blast, bool)> = Vec::new();
    let mut heals: Vec<(Entity, f32)> = Vec::new();
    for h in &landed {
        let Ok(upgrades) = attackers.get(h.attacker) else {
            continue;
        };
        let build = Build::from_cards(&upgrades.0);
        // Butcher: a knife kill heals.
        if h.weapon == WeaponKind::Knife && h.killed && build.has(Perk::Butcher) {
            heals.push((h.attacker, 15.0));
        }
        let Ok((mut status, mut zombie)) = zombies.get_mut(h.target) else {
            continue;
        };
        if h.weapon.def().class != Class::Gun {
            continue;
        }
        let pellets = h.weapon.def().pellets.max(1) as f32;
        // Effects that would stack absurdly per pellet happen once a shot,
        // on average.
        let once = rng.random::<f32>() < 1.0 / pellets;
        let combo = build.combo();
        let target_at = h.point;

        // Slug Kick: shotgun pellets shove zombies back.
        if h.weapon == WeaponKind::Shotgun && build.level(WeaponKind::Shotgun) >= 3 {
            zombie.shove += Vec2::new(h.dir.x, h.dir.z).normalize_or_zero() * 0.35;
        }
        // Thermal shock: a burning, chilled zombie takes a big extra hit.
        if combo == Some(Combo::ThermalShock) && status.burning() && status.chilled() {
            status.quench();
            extra.push(Extra {
                by: h.attacker,
                target: h.target,
                amount: h.amount * 0.6,
                weapon: h.weapon,
            });
            flash.write(ToClients {
                targets: SendTargets::All,
                message: Flash::Burst {
                    at: target_at,
                    radius: 0.8,
                },
            });
        }
        let was_frozen = status.frozen();
        if build.loaded(Ammo::Incendiary) {
            let tracer = combo == Some(Combo::TracerFire);
            let (secs, dps) = if tracer { (5.0, 12.0) } else { (3.0, 8.0) };
            status.ignite(h.attacker, secs, dps);
        }
        if build.loaded(Ammo::Frost) {
            status.chill();
        }
        if build.loaded(Ammo::Shock) && once {
            let rail = combo == Some(Combo::Rail);
            let (count, range) = if rail { (4, 8.0) } else { (2, 5.0) };
            let mut others: Vec<(Entity, Vec3)> = near
                .iter()
                .filter(|(e, p)| *e != h.target && p.pos.distance(target_at) < range)
                .map(|(e, p)| (e, p.pos))
                .collect();
            others.sort_by(|a, b| a.1.distance(target_at).total_cmp(&b.1.distance(target_at)));
            for (other, at) in others.into_iter().take(count) {
                extra.push(Extra {
                    by: h.attacker,
                    target: other,
                    amount: (h.amount * 0.35).max(6.0),
                    weapon: h.weapon,
                });
                flash.write(ToClients {
                    targets: SendTargets::All,
                    message: Flash::Arc {
                        from: target_at,
                        to: at + Vec3::Y * 0.3,
                    },
                });
            }
        }
        if build.loaded(Ammo::Explosive) && once {
            let napalm = combo == Some(Combo::Napalm);
            let blood = combo == Some(Combo::BloodBomb);
            blasts.push((
                Blast {
                    at: target_at,
                    attacker: h.attacker,
                    damage: (h.amount * 0.5).max(25.0),
                    radius: 2.2,
                    weapon: WeaponKind::Grenade,
                    tag: SECONDARY,
                },
                blood,
            ));
            if napalm {
                commands.spawn(FirePatch {
                    at: target_at,
                    by: h.attacker,
                    left: 4.0,
                    tick: 0.0,
                });
                flash.write(ToClients {
                    targets: SendTargets::All,
                    message: Flash::Fire {
                        at: target_at,
                        radius: 2.0,
                        secs: 4.0,
                    },
                });
            }
        }
        if build.loaded(Ammo::Leech) {
            // Twice as much from a headshot.
            let head = if h.head { 2.0 } else { 1.0 };
            heals.push((h.attacker, (h.amount * 0.06).clamp(1.0, 6.0) * head));
        }
        // Shatter: a frozen zombie that dies bursts and hurts those around it.
        if h.killed && was_frozen && combo == Some(Combo::Shatter) {
            blasts.push((
                Blast {
                    at: target_at,
                    attacker: h.attacker,
                    damage: 60.0,
                    radius: 3.0,
                    weapon: WeaponKind::Grenade,
                    tag: SECONDARY,
                },
                false,
            ));
        }
    }

    let mut hits = io.p1();
    for e in extra {
        if let Some(hurt) = hits.hurt(e.by, e.target, e.amount, false, e.weapon) {
            hits.wounded(e.target, &hurt, Vec3::Y, hurt.pos, e.weapon);
        }
    }
    let targets: Vec<(Entity, Vec3)> = near.iter().map(|(e, p)| (e, p.pos)).collect();
    for (b, blood) in blasts {
        let (at, radius, by) = (b.at, b.radius, b.attacker);
        flash.write(ToClients {
            targets: SendTargets::All,
            message: Flash::Burst { at, radius },
        });
        blast(&spatial, &mut hits, &targets, b);
        // Blood Bomb: the blast heals teammates caught in it.
        if blood {
            for (ally, pos) in &allies {
                if pos.pos.distance(at) < radius + 1.0 {
                    heals.push((ally, 10.0));
                }
            }
            heals.push((by, 0.0));
        }
    }
    for (who, amount) in heals {
        if amount <= 0.0 {
            continue;
        }
        let max = vigor.get(who).map_or(100.0, |v| v.0);
        if let Ok((mut health, ..)) = hits.targets.get_mut(who)
            && health.0 > 0.0
        {
            health.0 = (health.0 + amount).min(max);
        }
    }
}

/// Grenades with Cluster burst into bomblets; Blood Bomb heals teammates
/// caught in a grenade's blast.
pub fn on_boom(
    mut booms: MessageReader<Boom>,
    attackers: Query<&Upgrades>,
    mut allies: Query<(&Pose, &mut Health, &Vigor), (With<Player>, Without<Fallen>)>,
    mut commands: Commands,
) {
    let mut rng = rand::rng();
    for boom in booms.read() {
        if boom.tag == SECONDARY || boom.weapon != WeaponKind::Grenade {
            continue;
        }
        let Ok(upgrades) = attackers.get(boom.attacker) else {
            continue;
        };
        let build = Build::from_cards(&upgrades.0);
        if build.has(Perk::Cluster) {
            for i in 0..3 {
                let angle = i as f32 * std::f32::consts::TAU / 3.0 + rng.random_range(-0.5..0.5);
                let offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * rng.random_range(2.0..3.2);
                commands.spawn(Bomblet {
                    at: boom.at + offset,
                    by: boom.attacker,
                    left: 0.35 + 0.12 * i as f32,
                });
            }
        }
        if build.combo() == Some(Combo::BloodBomb) {
            for (pose, mut health, vigor) in &mut allies {
                if pose.pos.distance(boom.at) < 5.0 && health.0 > 0.0 {
                    health.0 = (health.0 + 15.0).min(vigor.0);
                }
            }
        }
    }
}

/// Burning ground and bomblets, over time.
#[allow(clippy::type_complexity)]
pub fn fires(
    time: Res<Time>,
    mut patches: Query<(Entity, &mut FirePatch)>,
    mut bomblets: Query<(Entity, &mut Bomblet)>,
    mut zombies: Query<(&Pose, &mut Status), (With<Zombie>, Without<Dead>)>,
    near: Query<(Entity, &Pose), (With<Zombie>, Without<Dead>)>,
    spatial: SpatialQuery,
    mut hits: Hits,
    mut flash: MessageWriter<ToClients<Flash>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, mut patch) in &mut patches {
        patch.left -= dt;
        patch.tick -= dt;
        if patch.left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        if patch.tick <= 0.0 {
            patch.tick = 0.5;
            for (pose, mut status) in &mut zombies {
                if pose.pos.with_y(patch.at.y).distance(patch.at) < 2.0 {
                    status.ignite(patch.by, 1.5, 10.0);
                }
            }
        }
    }
    let targets: Vec<(Entity, Vec3)> = near.iter().map(|(e, p)| (e, p.pos)).collect();
    for (entity, mut bomblet) in &mut bomblets {
        bomblet.left -= dt;
        if bomblet.left > 0.0 {
            continue;
        }
        commands.entity(entity).despawn();
        flash.write(ToClients {
            targets: SendTargets::All,
            message: Flash::Burst {
                at: bomblet.at,
                radius: 2.5,
            },
        });
        blast(
            &spatial,
            &mut hits,
            &targets,
            Blast {
                at: bomblet.at,
                attacker: bomblet.by,
                damage: 45.0,
                radius: 2.5,
                weapon: WeaponKind::Grenade,
                tag: SECONDARY,
            },
        );
    }
}
