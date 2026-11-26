//! Survival's sounds, played on every player's machine from what the server
//! replicates: the run's cues (a crack breaking open, the horn for a wave,
//! the gong when it's cleared), the zombies (groans, a leaper's screech, a
//! bloater bursting, a blow landing on you), what happens to you (a card,
//! going down, being healed, an ammo box) and the special ammo (fire, ice,
//! shock). Sounds made in the world fade with distance from the camera.

use bevy::{platform::collections::HashMap, prelude::*};
use rand::Rng;

use super::{
    Me,
    cracks::CRACKS,
    protocol::{BURNING, CRAWLER, FROZEN, Flash, LEAPER, Phase, Pickup, PickupKind},
};
use crate::{
    net::{Damaged, Dead, LocalId, Mob, Player, Pose},
    render::WorldCamera,
    sound::{Sounds, faded, play},
    weapon::WeaponKind,
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (run_cues, my_cues, flash_cues, zombie_cues, blows, pickups),
    );
}

/// Where the ears are.
type Listener<'w, 's> = Single<'w, 's, &'static GlobalTransform, With<WorldCamera>>;

/// The horn when a wave starts, the gong when it's cleared, and stone
/// giving way where a crack opens.
fn run_cues(
    me: Res<Me>,
    sounds: Res<Sounds>,
    ears: Listener,
    mut seen: Local<Option<(Phase, u16)>>,
    mut commands: Commands,
) {
    let Some(run) = &me.run else {
        *seen = None;
        return;
    };
    let s = &sounds.survival;
    if let Some((phase, cracks)) = *seen {
        if phase != run.phase {
            match run.phase {
                Phase::Wave => play(&mut commands, &s.horn, 0.7),
                Phase::Breather if run.wave > 0 => play(&mut commands, &s.cleared, 0.6),
                _ => {}
            }
        }
        let opened = run.cracks & !cracks;
        for (i, crack) in CRACKS.iter().enumerate() {
            if opened & (1 << i) != 0 {
                let at = Vec3::new(crack.at.x, 1.0, crack.at.y);
                let d = ears.translation().distance(at);
                play(&mut commands, &s.breach, faded(0.9, d, 90.0).max(0.15));
            }
        }
    }
    *seen = Some((run.phase, run.cracks));
}

/// Going down, and taking a card.
fn my_cues(
    me: Res<Me>,
    sounds: Res<Sounds>,
    mut seen: Local<(bool, usize)>,
    mut commands: Commands,
) {
    if !me.is_changed() {
        return;
    }
    let s = &sounds.survival;
    let down = me.fallen.is_some();
    if down && !seen.0 {
        play(&mut commands, &s.downed, 0.8);
    }
    // An offer that's gone means a card was taken.
    let offered = me.offer.len();
    if offered == 0 && seen.1 > 0 && me.run.is_some() {
        play(&mut commands, &s.card, 0.6);
    }
    *seen = (down, offered);
}

/// A bloater bursting, someone healed, shock arcing.
fn flash_cues(
    mut flashes: MessageReader<Flash>,
    sounds: Res<Sounds>,
    ears: Listener,
    mut commands: Commands,
) {
    let s = &sounds.survival;
    let mut zapped = false;
    for flash in flashes.read() {
        let (sound, at, volume, range) = match *flash {
            Flash::Gas { at, .. } => (&s.bloat, at, 1.0, 45.0),
            Flash::Heal { at } => (&s.revive, at, 0.7, 30.0),
            // One zap a frame, however many arcs.
            Flash::Arc { from, .. } if !zapped => {
                zapped = true;
                (&s.zap, from, 0.5, 35.0)
            }
            _ => continue,
        };
        let volume = faded(volume, ears.translation().distance(at), range);
        if volume > 0.01 {
            play(&mut commands, sound, volume);
        }
    }
}

/// What each zombie last sounded like, and when it groans next.
#[derive(Default)]
struct Heard {
    flags: u8,
    pose: u8,
    groan: f32,
}

/// Groans (or a crawler's dragging) now and then, a leaper's screech as it
/// crouches, and fire and ice taking hold.
#[allow(clippy::type_complexity)]
fn zombie_cues(
    time: Res<Time>,
    me: Res<Me>,
    mobs: Query<(Entity, &Mob, &Pose), Without<Dead>>,
    sounds: Res<Sounds>,
    ears: Listener,
    mut heard: Local<HashMap<Entity, Heard>>,
    // Seconds until any zombie may groan again, so a horde isn't a choir.
    mut hush: Local<f32>,
    mut commands: Commands,
) {
    if me.run.is_none() {
        heard.clear();
        return;
    }
    let s = &sounds.survival;
    let dt = time.delta_secs();
    *hush -= dt;
    let mut rng = rand::rng();
    let ears = ears.translation();
    heard.retain(|e, _| mobs.contains(*e));
    for (entity, mob, pose) in &mobs {
        let d = ears.distance(pose.pos);
        let h = heard.entry(entity).or_insert_with(|| Heard {
            flags: mob.flags,
            pose: pose.flags,
            groan: rng.random_range(0.5..4.0),
        });
        let gained = mob.flags & !h.flags;
        if gained & BURNING != 0 {
            play(&mut commands, &s.ignite, faded(0.6, d, 30.0));
        }
        if gained & FROZEN != 0 {
            play(&mut commands, &s.freeze, faded(0.7, d, 30.0));
        }
        if mob.kind == LEAPER && pose.flags & !h.pose & Pose::CROUCH != 0 {
            // The tell: loud and far-reaching, it's a warning.
            play(&mut commands, &s.screech, faded(1.0, d, 40.0).max(0.2));
        }
        h.flags = mob.flags;
        h.pose = pose.flags;
        h.groan -= dt;
        if h.groan <= 0.0 {
            h.groan = rng.random_range(3.0..9.0);
            if *hush <= 0.0 && d < 28.0 {
                *hush = 0.5;
                let sound = if mob.kind == CRAWLER {
                    &s.crawl
                } else {
                    &s.groan
                };
                play(&mut commands, sound, faded(0.55, d, 28.0));
            }
        }
    }
}

/// A zombie's blow landing on you. (In survival nothing else hits players
/// with a knife: there's no friendly fire.)
fn blows(
    me: Res<Me>,
    mut hits: MessageReader<Damaged>,
    players: Query<&Player>,
    local: Res<LocalId>,
    sounds: Res<Sounds>,
    mut commands: Commands,
) {
    for hit in hits.read() {
        if me.run.is_some()
            && hit.weapon == WeaponKind::Knife
            && players.get(hit.target).is_ok_and(|p| p.owner == local.0)
        {
            play(&mut commands, &sounds.survival.claw, 0.7);
        }
    }
}

/// An ammo box or medkit taken from under your feet.
fn pickups(
    lying: Query<(Entity, &Pickup)>,
    mut taken: RemovedComponents<Pickup>,
    ears: Listener,
    sounds: Res<Sounds>,
    mut known: Local<HashMap<Entity, Pickup>>,
    mut commands: Commands,
) {
    for entity in taken.read() {
        let Some(pickup) = known.remove(&entity) else {
            continue;
        };
        // Close enough that it was you who took it (one that timed out at
        // your feet sounds the same; that's rare).
        if ears.translation().distance(pickup.pos) < 2.6 {
            let s = &sounds.survival;
            match pickup.kind {
                PickupKind::Ammo => play(&mut commands, &s.ammo, 0.6),
                PickupKind::Medkit => play(&mut commands, &s.revive, 0.45),
            }
        }
    }
    for (entity, pickup) in &lying {
        known.entry(entity).or_insert(*pickup);
    }
}
