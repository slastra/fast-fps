//! Hit detection and damage. A client says "I fired this weapon from here
//! in these directions"; the server ray casts each one against the world and
//! the hitboxes and decides what it hit. Knife attacks work the same way with
//! a short swept sphere. [`Hits`] applies the damage for every weapon,
//! grenades included, under the current [`Rules`]; what a death means in a
//! game mode is up to the mode (`server/rules.rs`).

use avian3d::prelude::*;
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_replicon::prelude::*;

use crate::{
    config::{DUMMY_DOWN_SECS, MAX_HEALTH, MOB_CORPSE_SECS, SHOT_RANGE},
    net::{
        Damaged, Dead, Downed, Feed, Fire, Health, HitConfirm, Kit, Melee, Mob, Out, Player,
        PlayerName, Pose, Respawn, Score, ShotFx, WeaponMods,
    },
    physics::Layer,
    server::{
        dummies::DownTimer,
        players::{PlayerClient, find_player, respawn},
        rules::{Died, Disarmed, Rules, Untouchable},
    },
    weapon::{Class, KNIFE, WeaponKind},
};

/// Server-only: a hit that did damage, for game modes to react to (special
/// ammo, healing on hit, ...) without touching how weapons work.
#[derive(Message, Clone, Copy, Debug)]
pub struct HitLanded {
    pub attacker: Entity,
    pub target: Entity,
    pub weapon: WeaponKind,
    pub amount: f32,
    pub point: Vec3,
    pub dir: Vec3,
    pub head: bool,
    pub killed: bool,
}

/// Server-only: something exploded (see [`blast`]), for game modes to react
/// to.
#[derive(Message, Clone, Copy, Debug)]
pub struct Boom {
    pub at: Vec3,
    pub attacker: Entity,
    pub weapon: WeaponKind,
    /// From [`Blast::tag`].
    pub tag: u8,
}

/// Server-only: how long a killed mob stays before it's removed.
#[derive(Component)]
pub struct Corpse(pub f32);

/// Remove killed mobs once their ragdoll has had time to be made.
pub fn clear_corpses(
    time: Res<Time>,
    mut corpses: Query<(Entity, &mut Corpse)>,
    mut commands: Commands,
) {
    for (entity, mut corpse) in &mut corpses {
        corpse.0 -= time.delta_secs();
        if corpse.0 <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// Server-only: a bot pulled the trigger (bots have no client to send `Fire`).
#[derive(Message, Clone, Copy)]
pub struct BotShot {
    pub shooter: Entity,
    pub weapon: WeaponKind,
    pub origin: Vec3,
    pub dir: Vec3,
}

/// Height above the collider center where the head starts.
pub fn head_line(pose: &Pose) -> f32 {
    if pose.has(Pose::CROUCH) { 0.12 } else { 0.48 }
}

/// What one hit did to its target.
pub struct Hurt {
    pub killed: bool,
    /// The target is a player (not a mob or a dummy).
    pub player: bool,
    /// Where the target was standing and facing when hit, for the `Damaged`
    /// message (corpses spawn there, not at the respawned entity).
    pub pos: Vec3,
    pub yaw: f32,
}

/// Everything needed to damage players and dummies, shared by guns, the
/// knife and grenades so the rules for kills, scores, the feed and respawns
/// live in one place.
#[derive(SystemParam)]
pub struct Hits<'w, 's> {
    pub clients: Query<'w, 's, (Entity, &'static PlayerClient)>,
    #[allow(clippy::type_complexity)]
    pub targets: Query<
        'w,
        's,
        (
            &'static mut Health,
            &'static Pose,
            Has<Player>,
            Has<Downed>,
            Has<Out>,
            Has<Mob>,
            Has<Untouchable>,
        ),
    >,
    rules: Res<'w, Rules>,
    pub kits: Query<'w, 's, &'static mut Kit>,
    pub mods: Query<'w, 's, &'static WeaponMods>,
    disarmed: Query<'w, 's, (), With<Disarmed>>,
    scores: Query<'w, 's, &'static mut Score>,
    names: Query<'w, 's, &'static PlayerName>,
    pub owners: Query<'w, 's, &'static Player>,
    commands: Commands<'w, 's>,
    confirm: MessageWriter<'w, ToClients<HitConfirm>>,
    feed: MessageWriter<'w, ToClients<Feed>>,
    respawns: MessageWriter<'w, ToClients<Respawn>>,
    damaged: MessageWriter<'w, ToClients<Damaged>>,
    died: MessageWriter<'w, Died>,
    pub landed: MessageWriter<'w, HitLanded>,
    booms: MessageWriter<'w, Boom>,
}

impl Hits<'_, '_> {
    /// Take `amount` health from `target`. A kill scores for `attacker`
    /// (unless it was themselves) and is announced with [`Died`]; a killed
    /// player respawns if the [`Rules`] say so, and goes in the feed. `None`
    /// if the target can't be hurt (out of play, untouchable, or a teammate
    /// without friendly fire).
    pub fn hurt(
        &mut self,
        attacker: Entity,
        target: Entity,
        amount: f32,
        head: bool,
        weapon: WeaponKind,
    ) -> Option<Hurt> {
        let attacker_is_player = self.targets.get(attacker).is_ok_and(|t| t.2);
        let friendly_fire = self.rules.friendly_fire;
        let respawns = self.rules.respawn;
        let Ok((mut health, pose, is_player, downed, out, mob, untouchable)) =
            self.targets.get_mut(target)
        else {
            return None;
        };
        if downed || out || untouchable || health.0 <= 0.0 {
            return None;
        }
        if is_player && attacker_is_player && attacker != target && !friendly_fire {
            return None;
        }
        let mut hurt = Hurt {
            killed: false,
            player: is_player,
            pos: pose.pos,
            yaw: pose.yaw,
        };
        health.0 -= amount;
        if health.0 > 0.0 {
            return Some(hurt);
        }
        hurt.killed = true;
        if mob {
            // It stays a moment for clients to make its ragdoll from.
            health.0 = 0.0;
            self.commands
                .entity(target)
                .insert((Dead, Corpse(MOB_CORPSE_SECS)));
            if let Ok(mut score) = self.scores.get_mut(attacker) {
                score.kills += 1;
            }
            self.died.write(Died {
                victim: target,
                killer: attacker,
            });
            return Some(hurt);
        }
        if !is_player {
            health.0 = 0.0;
            self.commands.entity(target).insert((
                Downed,
                DownTimer(Timer::from_seconds(DUMMY_DOWN_SECS, TimerMode::Once)),
            ));
            return Some(hurt);
        }
        // Without respawning, the fallen stay at 0 until the mode decides.
        health.0 = if respawns { MAX_HEALTH } else { 0.0 };
        if let Ok(mut score) = self.scores.get_mut(target) {
            score.deaths += 1;
        }
        if attacker != target
            && let Ok(mut score) = self.scores.get_mut(attacker)
        {
            score.kills += 1;
        }
        let name = |e| {
            self.names
                .get(e)
                .map(|n| n.0.clone())
                .unwrap_or_else(|_| "?".into())
        };
        self.feed.write(ToClients {
            targets: SendTargets::All,
            message: Feed::Kill {
                killer: name(attacker),
                victim: name(target),
                weapon,
                headshot: head,
            },
        });
        if respawns && let Ok((_, victim_client)) = self.clients.get(target) {
            respawn(victim_client.0, &mut self.respawns);
        }
        self.died.write(Died {
            victim: target,
            killer: attacker,
        });
        Some(hurt)
    }

    /// Whether `shooter` may attack with `weapon` right now: in play, not
    /// disarmed by the mode, and owning the weapon if they have a `Kit`.
    pub fn allows(&self, shooter: Entity, weapon: WeaponKind) -> bool {
        if self.targets.get(shooter).is_ok_and(|t| t.4) || self.disarmed.contains(shooter) {
            return false;
        }
        self.kits.get(shooter).is_ok_and(|kit| kit.owns(weapon)) || !self.kits.contains(shooter)
    }

    /// Damage for one hit of `weapon` by `shooter`, before distance falloff.
    pub fn damage(&self, shooter: Entity, weapon: WeaponKind, head: bool) -> f32 {
        let def = weapon.def();
        let mods = self.mods.get(shooter).ok();
        let mult = mods.map_or(1.0, |m| m.damage[weapon.index()]);
        let head_mult = if head {
            def.head_mult * mods.map_or(1.0, |m| m.head)
        } else {
            1.0
        };
        def.damage * mult * head_mult
    }

    /// Tell everyone a target was hit (for blood, flinches and ragdolls).
    pub fn wounded(
        &mut self,
        target: Entity,
        hurt: &Hurt,
        dir: Vec3,
        point: Vec3,
        weapon: WeaponKind,
    ) {
        self.damaged.write(ToClients {
            targets: SendTargets::All,
            message: Damaged {
                target,
                dir: dir.normalize_or_zero(),
                point,
                weapon,
                // Drop a ragdoll, unless the rules say players don't.
                killed: hurt.killed && (!hurt.player || self.rules.player_corpses),
                pos: hurt.pos,
                yaw: hurt.yaw,
            },
        });
    }

    /// The hit marker for whoever caused the damage, if they're a player.
    pub fn confirm(&mut self, attacker: Entity, head: bool, kill: bool) {
        if let Ok((_, client)) = self.clients.get(attacker) {
            self.confirm.write(ToClients {
                targets: SendTargets::Single(client.0),
                message: HitConfirm { head, kill },
            });
        }
    }
}

/// An explosion: who set it off, where, how hard and how far it reaches.
pub struct Blast {
    pub at: Vec3,
    pub attacker: Entity,
    /// Damage at the center, falling off evenly to nothing at `radius`.
    pub damage: f32,
    pub radius: f32,
    pub weapon: WeaponKind,
    /// A game mode's mark on its own explosions, passed on in [`Boom`] (0 for
    /// an ordinary grenade).
    pub tag: u8,
}

/// Hurt everyone in `near` (entity and position) that `b` reaches and no
/// wall shields, throw them up and away, and confirm hits to the attacker.
/// Used by grenades, and by game modes for anything else that explodes.
pub fn blast(spatial: &SpatialQuery, hits: &mut Hits, near: &[(Entity, Vec3)], b: Blast) {
    hits.booms.write(Boom {
        at: b.at,
        attacker: b.attacker,
        weapon: b.weapon,
        tag: b.tag,
    });
    let walls = SpatialQueryFilter::from_mask(Layer::World);
    let mut any_hit = false;
    let mut any_kill = false;
    for &(target, pos) in near {
        let chest = pos + Vec3::Y * CHEST;
        let offset = chest - b.at;
        let distance = offset.length();
        let damage = (b.damage * (1.0 - distance / b.radius)).max(0.0);
        if damage <= 0.0 {
            continue;
        }
        // Walls shield you. The ray stops a little short of the target so the
        // floor it stands on doesn't count.
        if let Ok(dir) = Dir3::new(offset)
            && spatial
                .cast_ray(b.at, dir, (distance - 0.3).max(0.0), true, &walls)
                .is_some()
        {
            continue;
        }
        let Some(hurt) = hits.hurt(b.attacker, target, damage, false, b.weapon) else {
            continue;
        };
        any_hit |= target != b.attacker;
        any_kill |= hurt.killed && target != b.attacker;
        hits.landed.write(HitLanded {
            attacker: b.attacker,
            target,
            weapon: b.weapon,
            amount: damage,
            point: chest,
            dir: offset.normalize_or_zero(),
            head: false,
            killed: hurt.killed,
        });
        // Bodies fly up and away from the blast.
        let push = offset.normalize_or_zero() + Vec3::Y * 0.6;
        hits.wounded(target, &hurt, push, chest, b.weapon);
    }
    if any_hit {
        hits.confirm(b.attacker, false, any_kill);
    }
}

/// How high above a body's center a blast aims (the chest).
const CHEST: f32 = 0.2;

/// Fire `dirs` (one per pellet) from `origin`: ray cast each, damage what
/// they hit, tell everyone who was hurt, and confirm hits to the shooter.
/// Returns the effects for other players to draw.
pub fn shoot(
    spatial: &SpatialQuery,
    hits: &mut Hits,
    shooter: Entity,
    weapon: WeaponKind,
    origin: Vec3,
    dirs: &[Vec3],
) -> Vec<ShotFx> {
    let shooter_id = hits.owners.get(shooter).map(|p| p.owner).unwrap_or(0);
    let def = weapon.def();
    let filter = SpatialQueryFilter::from_mask([Layer::World, Layer::Hitbox])
        .with_excluded_entities([shooter]);

    let mut effects = Vec::with_capacity(dirs.len());
    let mut any_hit = false;
    let mut any_head = false;
    let mut any_kill = false;
    // A target killed by an earlier pellet has already respawned at full
    // health, so the rest of this blast must not touch it.
    let mut killed: Vec<Entity> = Vec::new();
    // One `Damaged` per target per blast: (target, summed dir, first point, result).
    let mut wounds: Vec<(Entity, Vec3, Vec3, Hurt)> = Vec::new();

    // Each shot can pass through this many targets after the first.
    let pierce = hits
        .mods
        .get(shooter)
        .map_or(0, |m| m.pierce[weapon.index()]) as usize;
    for dir in dirs {
        let Ok(dir) = Dir3::new(*dir) else {
            continue;
        };
        let mut from = origin;
        let mut travelled = 0.0;
        let mut passed: Vec<Entity> = Vec::new();
        let mut end = origin + dir * SHOT_RANGE;
        let mut normal = Vec3::ZERO;
        let mut flesh = false;
        loop {
            // Never the shooter, nor anything this shot passed through or
            // has killed already.
            let skip = std::iter::once(shooter).chain(passed.iter().chain(&killed).copied());
            let filter = filter.clone().with_excluded_entities(skip);
            let Some(hit) = spatial.cast_ray(from, dir, SHOT_RANGE - travelled, true, &filter)
            else {
                break;
            };
            travelled += hit.distance;
            let point = from + dir * hit.distance;
            end = point;
            normal = hit.normal;
            let Ok((_, pose, ..)) = hits.targets.get(hit.entity) else {
                // The world stops a bullet.
                break;
            };
            let head = point.y > pose.pos.y + head_line(pose);
            let falloff = def.falloff_per_10m.powf(travelled / 10.0);
            let amount = hits.damage(shooter, weapon, head) * falloff;
            if let Some(hurt) = hits.hurt(shooter, hit.entity, amount, head, weapon) {
                flesh = true;
                any_hit = true;
                any_head |= head;
                hits.landed.write(HitLanded {
                    attacker: shooter,
                    target: hit.entity,
                    weapon,
                    amount,
                    point,
                    dir: dir.as_vec3(),
                    head,
                    killed: hurt.killed,
                });
                if hurt.killed {
                    any_kill = true;
                    killed.push(hit.entity);
                }
                match wounds.iter_mut().find(|w| w.0 == hit.entity) {
                    Some(w) => {
                        w.1 += dir.as_vec3();
                        w.3.killed |= hurt.killed;
                    }
                    None => wounds.push((hit.entity, dir.as_vec3(), point, hurt)),
                }
            }
            if passed.len() >= pierce {
                break;
            }
            passed.push(hit.entity);
            from = point;
        }
        effects.push(ShotFx {
            shooter: shooter_id,
            weapon,
            origin,
            end,
            normal,
            flesh,
        });
    }

    for (target, dir, point, hurt) in wounds {
        hits.wounded(target, &hurt, dir, point, weapon);
    }
    if any_hit {
        hits.confirm(shooter, any_head, any_kill);
    }
    effects
}

pub fn receive_fire(
    mut shots: MessageReader<FromClient<Fire>>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut hits: Hits,
    mut fx: MessageWriter<ToClients<ShotFx>>,
    // When each player's flintlock is loaded again, so a client can't fire it
    // faster than it reloads.
    mut loaded: Local<bevy::platform::collections::HashMap<Entity, f32>>,
) {
    let now = time.elapsed_secs();
    for FromClient { client_id, message } in shots.read() {
        let Some(shooter) = find_player(*client_id, &hits.clients) else {
            continue;
        };
        let def = message.weapon.def();
        let pellets = WeaponMods::pellets_of(hits.mods.get(shooter).ok(), message.weapon);
        if def.class != Class::Gun
            || !message.origin.is_finite()
            || message.dirs.len() > pellets as usize
            || !hits.allows(shooter, message.weapon)
        {
            continue;
        }
        // Out of ammo, in a mode that counts it.
        if let Ok(mut kit) = hits.kits.get_mut(shooter)
            && !kit.spend(message.weapon)
        {
            continue;
        }
        if message.weapon == WeaponKind::Flintlock {
            // A little slack for network jitter.
            if loaded.get(&shooter).is_some_and(|&at| now < at - 0.25) {
                continue;
            }
            loaded.insert(shooter, now + def.reload_secs);
        }
        for effect in shoot(
            &spatial,
            &mut hits,
            shooter,
            message.weapon,
            message.origin,
            &message.dirs,
        ) {
            fx.write(ToClients {
                targets: SendTargets::AllExcept(*client_id),
                message: effect,
            });
        }
    }
}

/// Shots from bots that fight back (in a duel). Same rules as players'.
pub fn receive_bot_shots(
    mut shots: MessageReader<BotShot>,
    spatial: SpatialQuery,
    mut hits: Hits,
    mut fx: MessageWriter<ToClients<ShotFx>>,
) {
    for shot in shots.read() {
        if !hits.allows(shot.shooter, shot.weapon) {
            continue;
        }
        for effect in shoot(
            &spatial,
            &mut hits,
            shot.shooter,
            shot.weapon,
            shot.origin,
            &[shot.dir],
        ) {
            fx.write(ToClients {
                targets: SendTargets::All,
                message: effect,
            });
        }
    }
}

/// Knife attacks. The swing is a short swept sphere from the eye, so it
/// forgives a little aim; a hit from behind kills outright.
pub fn receive_melee(
    mut attacks: MessageReader<FromClient<Melee>>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut hits: Hits,
    mut fx: MessageWriter<ToClients<ShotFx>>,
    // When each player may swing again, so a client can't swing faster than
    // the knife allows.
    mut ready: Local<bevy::platform::collections::HashMap<Entity, f32>>,
) {
    let now = time.elapsed_secs();
    for FromClient { client_id, message } in attacks.read() {
        let Some(attacker) = find_player(*client_id, &hits.clients) else {
            continue;
        };
        if !hits.allows(attacker, WeaponKind::Knife) {
            continue;
        }
        let Ok(dir) = Dir3::new(message.dir) else {
            continue;
        };
        if !message.origin.is_finite() {
            continue;
        }
        // A little slack for network jitter.
        if ready.get(&attacker).is_some_and(|&at| now < at - 0.1) {
            continue;
        }
        let interval = if message.heavy {
            KNIFE.stab_secs
        } else {
            KNIFE.slash_secs
        };
        ready.insert(attacker, now + interval);

        let attacker_id = hits.owners.get(attacker).map(|p| p.owner).unwrap_or(0);
        let filter = SpatialQueryFilter::from_mask([Layer::World, Layer::Hitbox])
            .with_excluded_entities([attacker]);
        let hit = spatial.cast_shape(
            &Collider::sphere(KNIFE.width),
            message.origin,
            Quat::IDENTITY,
            dir,
            &ShapeCastConfig::from_max_distance(KNIFE.reach),
            &filter,
        );
        let mut end = message.origin + dir * KNIFE.reach;
        let mut normal = Vec3::ZERO;
        let mut flesh = false;
        if let Some(hit) = hit {
            end = hit.point1;
            normal = hit.normal1;
            if let Ok((_, pose, ..)) = hits.targets.get(hit.entity) {
                let mult = hits
                    .mods
                    .get(attacker)
                    .map_or(1.0, |m| m.damage[WeaponKind::Knife.index()]);
                let amount = if KNIFE.is_behind(dir.as_vec3(), pose.yaw) {
                    // Enough to kill from full health.
                    1000.0
                } else if message.heavy {
                    KNIFE.stab * mult
                } else {
                    KNIFE.slash * mult
                };
                if let Some(hurt) =
                    hits.hurt(attacker, hit.entity, amount, false, WeaponKind::Knife)
                {
                    flesh = true;
                    let kill = hurt.killed;
                    hits.landed.write(HitLanded {
                        attacker,
                        target: hit.entity,
                        weapon: WeaponKind::Knife,
                        amount,
                        point: end,
                        dir: dir.as_vec3(),
                        head: false,
                        killed: kill,
                    });
                    hits.wounded(hit.entity, &hurt, dir.as_vec3(), end, WeaponKind::Knife);
                    hits.confirm(attacker, false, kill);
                }
            }
        }
        fx.write(ToClients {
            targets: SendTargets::AllExcept(*client_id),
            message: ShotFx {
                shooter: attacker_id,
                weapon: WeaponKind::Knife,
                origin: message.origin,
                end,
                normal,
                flesh,
            },
        });
    }
}
