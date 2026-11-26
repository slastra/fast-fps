//! Zombies: how they come through the cracks, find their way to the keep
//! and to players, and hit them.
//!
//! There is no navigation mesh. Each crack has a fixed route to a keep door
//! (`survival/cracks.rs`); along it a zombie steers with three short "whisker"
//! rays against the world, keeps clear of other zombies, and follows the
//! ground with a ray down (so it climbs ramps). A player it can see within
//! [`SIGHT`] meters, or any player once it reaches the keep, it chases.
//! The server writes each zombie's `Pose`, which every machine draws.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use rand::Rng;

use super::status::Status;
use crate::{
    config::{SURVIVAL_MAX_ALIVE, SURVIVAL_SPAWN_SECS},
    net::{Dead, Health, Mob, Out, Player, PlayerName, Pose},
    physics::Layer,
    server::{
        combat::Hits,
        players::{PlayerClient, hitbox},
    },
    survival::{
        cracks::{CRACKS, entry, way_to},
        protocol::{BRUTE, Fallen, Flash, RUNNER, WALKER},
    },
    weapon::WeaponKind,
};

/// How far a zombie sees a player (when the walls don't hide them).
const SIGHT: f32 = 18.0;
/// Close enough to the next route point to head for the one after.
const ARRIVE: f32 = 1.4;
/// How quickly a zombie turns, radians a second.
const TURN: f32 = 7.0;
/// How long a swing takes, and when in it the blow lands.
const SWING_SECS: f32 = 0.7;
const SWING_LANDS: f32 = 0.3;

/// What each kind of zombie is like, before the wave makes it tougher.
pub struct Kind {
    pub id: u8,
    pub name: &'static str,
    pub health: f32,
    pub speed: f32,
    pub damage: f32,
    pub reach: f32,
    /// Seconds between swings.
    pub cooldown: f32,
}

pub const KINDS: [Kind; 3] = [
    Kind {
        id: WALKER,
        name: "Walker",
        health: 80.0,
        speed: 2.2,
        damage: 15.0,
        reach: 1.3,
        cooldown: 1.2,
    },
    Kind {
        id: RUNNER,
        name: "Runner",
        health: 50.0,
        speed: 4.5,
        damage: 10.0,
        reach: 1.2,
        cooldown: 0.9,
    },
    Kind {
        id: BRUTE,
        name: "Brute",
        health: 400.0,
        speed: 1.6,
        damage: 35.0,
        reach: 1.6,
        cooldown: 1.8,
    },
];

pub fn kind(id: u8) -> &'static Kind {
    KINDS.iter().find(|k| k.id == id).unwrap_or(&KINDS[0])
}

/// Which kind wave `n` sends next: walkers, then some runners from wave 5
/// and some brutes from wave 10.
pub fn pick_kind(n: u16, rng: &mut impl Rng) -> u8 {
    let roll = rng.random::<f32>();
    if n >= 10 && roll < 0.08 {
        BRUTE
    } else if n >= 5 && roll < 0.28 {
        RUNNER
    } else {
        WALKER
    }
}

/// Health grows 8% a wave, speed 2% a wave (up to a third more).
pub fn scaled(k: &Kind, wave: u16) -> (f32, f32) {
    let w = wave.saturating_sub(1) as f32;
    (
        k.health * 1.08f32.powf(w),
        k.speed * (1.0 + 0.02 * w).min(1.33),
    )
}

/// Server-only: a zombie's mind.
#[derive(Component)]
pub struct Zombie {
    pub kind: u8,
    pub crack: usize,
    /// The next point on its crack's route (past the end: chase anyone).
    pub next: usize,
    pub speed: f32,
    pub yaw: f32,
    /// Seconds until it can swing again, and how far into a swing it is.
    pub cooldown: f32,
    pub swing: Option<f32>,
    /// A push it's taking (a shotgun's Slug Kick), applied as it walks.
    pub shove: Vec2,
}

/// Server-only: zombies waiting to come through the cracks this wave.
#[derive(Resource, Default)]
pub struct Spawner {
    pub queue: Vec<u8>,
    pub wave: u16,
    pub timer: f32,
    /// Open cracks, and how often each is chosen (the Barricade perk lowers
    /// the one nearest whoever has it).
    pub weights: Vec<(usize, f32)>,
}

/// Bring the next queued zombie through a crack, when there's room.
pub fn spawn(
    time: Res<Time>,
    mut spawner: ResMut<Spawner>,
    alive: Query<(), (With<Zombie>, Without<Dead>)>,
    mut flash: MessageWriter<ToClients<Flash>>,
    mut commands: Commands,
) {
    spawner.timer -= time.delta_secs();
    if spawner.queue.is_empty()
        || spawner.timer > 0.0
        || alive.iter().count() >= SURVIVAL_MAX_ALIVE
        || spawner.weights.is_empty()
    {
        return;
    }
    spawner.timer = SURVIVAL_SPAWN_SECS;
    let mut rng = rand::rng();
    let total: f32 = spawner.weights.iter().map(|w| w.1).sum();
    let mut roll = rng.random::<f32>() * total;
    let crack = spawner
        .weights
        .iter()
        .find(|w| {
            roll -= w.1;
            roll <= 0.0
        })
        .map_or(spawner.weights[0].0, |w| w.0);
    let Some(id) = spawner.queue.pop() else {
        return;
    };
    let pos = entry(crack);
    flash.write(ToClients {
        targets: SendTargets::All,
        message: Flash::Dust { at: pos },
    });
    let inward = CRACKS[crack].inward;
    let yaw = (-inward.x).atan2(-inward.y);
    commands.spawn(zombie(id, spawner.wave, crack, 0, pos, yaw));
}

/// A zombie of kind `id`, as tough as wave `wave` makes it, at `pos`,
/// following crack `crack`'s route from point `next` (past the end: it goes
/// for whoever is nearest).
pub fn zombie(id: u8, wave: u16, crack: usize, next: usize, pos: Vec3, yaw: f32) -> impl Bundle {
    let k = kind(id);
    let (health, speed) = scaled(k, wave);
    (
        Name::new(k.name),
        Replicated,
        Mob { kind: id, flags: 0 },
        PlayerName(k.name.into()),
        Health(health),
        Pose {
            pos,
            yaw,
            flags: Pose::REACH,
            ..default()
        },
        Transform::from_translation(pos),
        hitbox(),
        Zombie {
            kind: id,
            crack,
            next,
            speed,
            yaw,
            cooldown: 0.0,
            swing: None,
            shove: Vec2::ZERO,
        },
        Status::default(),
    )
}

/// A player a zombie could go for.
struct Prey {
    pos: Vec3,
}

/// Walk every zombie: toward a player it sees, else along its route.
#[allow(clippy::type_complexity)]
pub fn steer(
    time: Res<Time>,
    spatial: SpatialQuery,
    mut zombies: Query<(&mut Zombie, &mut Pose, &Status), Without<Dead>>,
    players: Query<&Pose, (With<Player>, Without<Zombie>, Without<Fallen>, Without<Out>)>,
) {
    let dt = time.delta_secs();
    let prey: Vec<Prey> = players.iter().map(|pose| Prey { pos: pose.pos }).collect();
    let crowd: Vec<Vec3> = zombies.iter().map(|z| z.1.pos).collect();
    let walls = SpatialQueryFilter::from_mask(Layer::World);
    let mut rng = rand::rng();

    for (mut zombie, mut pose, status) in &mut zombies {
        let at = pose.pos;
        let flat = |v: Vec3| Vec2::new(v.x, v.z);
        // Someone to chase: in sight, or anyone once inside the keep.
        let route = CRACKS[zombie.crack].route;
        let arrived = zombie.next >= route.len();
        let seen = prey
            .iter()
            .filter(|p| {
                let d = p.pos.distance(at);
                arrived || (d < SIGHT && can_see(&spatial, &walls, at, p.pos))
            })
            .min_by(|a, b| a.pos.distance(at).total_cmp(&b.pos.distance(at)));
        let goal = match seen {
            // In sight: straight for them. Otherwise by a door if the keep's
            // wall is between.
            Some(p) if !arrived || can_see(&spatial, &walls, at, p.pos) => flat(p.pos),
            Some(p) => way_to(flat(at), flat(p.pos)),
            None if !arrived => route[zombie.next],
            // Nobody left standing: mill about in the keep.
            None => Vec2::new(rng.random_range(-3.0..3.0), rng.random_range(-3.0..3.0)),
        };
        if !arrived && flat(at).distance(route[zombie.next]) < ARRIVE {
            zombie.next += 1;
        }

        let reach = kind(zombie.kind).reach;
        let close = seen.is_some_and(|p| flat(p.pos).distance(flat(at)) < reach * 0.9);
        let mut speed = zombie.speed * status.speed_scale();
        if close || zombie.swing.is_some() {
            speed = 0.0;
        }
        let mut dir = (goal - flat(at)).normalize_or_zero();
        if speed > 0.0 {
            dir = avoid_walls(&spatial, &walls, at, dir);
            dir = (dir + keep_apart(&crowd, at)).normalize_or_zero();
        }
        let step = dir * speed * dt + std::mem::take(&mut zombie.shove);
        let mut next = at + Vec3::new(step.x, 0.0, step.y);
        next.y = ground(&spatial, &walls, next).unwrap_or(next.y);

        // Face where it's going, or whoever it's about to hit.
        let face = if close {
            (goal - flat(at)).normalize_or_zero()
        } else {
            dir
        };
        if face != Vec2::ZERO {
            let want = (-face.x).atan2(-face.y);
            let turn = (want - zombie.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            zombie.yaw += turn.clamp(-TURN * dt, TURN * dt);
        }

        let mut flags = Pose::REACH;
        if zombie.swing.is_some() {
            flags |= Pose::SWING;
        }
        let new = Pose {
            pos: next,
            yaw: zombie.yaw,
            pitch: 0.0,
            vel: (next - at) / dt.max(1e-4),
            flags,
        };
        if *pose != new {
            *pose = new;
        }
    }
}

/// No wall between a zombie's eyes and a player's chest.
fn can_see(spatial: &SpatialQuery, walls: &SpatialQueryFilter, from: Vec3, to: Vec3) -> bool {
    let eye = from + Vec3::Y * 0.6;
    let chest = to + Vec3::Y * 0.3;
    let Ok(dir) = Dir3::new(chest - eye) else {
        return true;
    };
    spatial
        .cast_ray(eye, dir, eye.distance(chest), true, walls)
        .is_none()
}

/// Turn away from what's just ahead: three rays at knee height, straight
/// on and 35 degrees to each side. Take the clear side nearer the goal.
fn avoid_walls(spatial: &SpatialQuery, walls: &SpatialQueryFilter, at: Vec3, dir: Vec2) -> Vec2 {
    const LOOK: f32 = 1.3;
    let knee = at - Vec3::Y * 0.5;
    let blocked = |d: Vec2| {
        Dir3::new(Vec3::new(d.x, 0.0, d.y))
            .ok()
            .and_then(|ray| spatial.cast_ray(knee, ray, LOOK, true, walls))
            .is_some()
    };
    if dir == Vec2::ZERO || !blocked(dir) {
        return dir;
    }
    let left = Vec2::from_angle(0.6).rotate(dir);
    let right = Vec2::from_angle(-0.6).rotate(dir);
    match (blocked(left), blocked(right)) {
        (false, _) => left,
        (true, false) => right,
        // Boxed in: sidestep square to the way it was going.
        (true, true) => dir.perp(),
    }
}

/// A gentle push away from other zombies within arm's reach.
fn keep_apart(crowd: &[Vec3], at: Vec3) -> Vec2 {
    let mut push = Vec2::ZERO;
    for other in crowd {
        let d = Vec2::new(at.x - other.x, at.z - other.z);
        let len = d.length();
        if len > 0.01 && len < 0.9 {
            push += d / len * (0.9 - len) * 1.5;
        }
    }
    push
}

/// The collider-center height for a body standing at `at`: the ground
/// below, plus half its height.
fn ground(spatial: &SpatialQuery, walls: &SpatialQueryFilter, at: Vec3) -> Option<f32> {
    let from = Vec3::new(at.x, at.y + 1.0, at.z);
    spatial
        .cast_ray(from, Dir3::NEG_Y, 4.0, true, walls)
        .map(|hit| from.y - hit.distance + 0.9)
}

/// Swing at a player in reach; the blow lands partway through the swing.
#[allow(clippy::type_complexity)]
pub fn attack(
    time: Res<Time>,
    mut zombies: Query<(Entity, &mut Zombie, &Status), Without<Dead>>,
    players: Query<
        Entity,
        (
            With<Player>,
            With<PlayerClient>,
            Without<Fallen>,
            Without<Out>,
        ),
    >,
    bots: Query<
        Entity,
        (
            With<Player>,
            Without<PlayerClient>,
            Without<Fallen>,
            Without<Out>,
        ),
    >,
    mut hits: Hits,
) {
    let dt = time.delta_secs();
    let targets: Vec<(Entity, Vec3)> = players
        .iter()
        .chain(&bots)
        .filter_map(|e| hits.targets.get(e).ok().map(|t| (e, t.1.pos)))
        .collect();
    for (entity, mut zombie, status) in &mut zombies {
        zombie.cooldown -= dt;
        if status.frozen() {
            zombie.swing = None;
            continue;
        }
        let Ok(at) = hits.targets.get(entity).map(|t| t.1.pos) else {
            continue;
        };
        let k = kind(zombie.kind);
        let in_reach = |reach: f32| {
            targets
                .iter()
                .filter(|(_, pos)| pos.distance(at) < reach && (pos.y - at.y).abs() < 1.4)
                .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
                .copied()
        };
        match zombie.swing {
            Some(t) => {
                let t = t + dt;
                if t - dt < SWING_LANDS
                    && t >= SWING_LANDS
                    && let Some((target, pos)) = in_reach(k.reach + 0.4)
                    && let Some(hurt) =
                        hits.hurt(entity, target, k.damage, false, WeaponKind::Knife)
                {
                    hits.wounded(target, &hurt, pos - at, pos, WeaponKind::Knife);
                }
                zombie.swing = (t < SWING_SECS).then_some(t);
            }
            None if zombie.cooldown <= 0.0 && in_reach(k.reach).is_some() => {
                zombie.swing = Some(0.0);
                zombie.cooldown = k.cooldown;
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;

    #[test]
    fn early_waves_are_walkers() {
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..200 {
            assert_eq!(pick_kind(1, &mut rng), WALKER);
            assert_ne!(pick_kind(7, &mut rng), BRUTE);
        }
        let late: Vec<u8> = (0..500).map(|_| pick_kind(12, &mut rng)).collect();
        assert!(late.contains(&BRUTE) && late.contains(&RUNNER));
    }

    #[test]
    fn waves_make_zombies_tougher_and_faster_up_to_a_point() {
        let (h1, s1) = scaled(kind(WALKER), 1);
        let (h10, s10) = scaled(kind(WALKER), 10);
        let (_, s50) = scaled(kind(WALKER), 50);
        assert_eq!(h1, 80.0);
        assert!(h10 > 150.0 && h10 < 170.0);
        assert!(s10 > s1);
        assert!((s50 / s1 - 1.33).abs() < 0.01);
    }

    #[test]
    fn crowds_push_apart() {
        let push = keep_apart(&[Vec3::ZERO, Vec3::new(0.5, 0.0, 0.0)], Vec3::ZERO);
        assert!(push.x < 0.0);
        assert_eq!(
            keep_apart(&[Vec3::new(5.0, 0.0, 0.0)], Vec3::ZERO),
            Vec2::ZERO
        );
    }
}
