//! The cracks in the outer walls that zombies climb through, and the way
//! each one leads to the keep. Shared by the server (where zombies spawn and
//! walk) and every player's machine (where cracks are drawn).
//!
//! A crack is one of the level's bricked-up breaches (`BREACHES` in
//! `world/arena.rs`), opened: [`BreachPlugin`] hides its plug and lets
//! zombies, shots and grenades through, while still stopping players.
//! Zombies come from outside the wall and climb in through the hole.
//!
//! The map's middle is the walled keep (`world/arena.rs`), with doors on the
//! north, south and east sides and a solid west wall, so every route ends at
//! a door. The points were checked against the map's fixed pieces: none runs
//! through a pillar, ramp or platform. Random rocks and trees may still be in
//! the way, which the zombies' steering handles.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::protocol::Survival;
use crate::{
    physics::Layer,
    world::{Breach, breach},
};

/// One crack: where it is on the wall's inner face, which way is inward,
/// and the points a zombie walks through to reach the keep, after climbing
/// in (see [`waypoint`]).
pub struct Crack {
    pub at: Vec2,
    pub inward: Vec2,
    pub route: &'static [Vec2],
}

const fn crack(i: usize, route: &'static [Vec2]) -> Crack {
    let (at, inward) = breach(i);
    Crack { at, inward, route }
}

const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

/// Outside each door, and just inside it.
const NORTH: [Vec2; 2] = [v(0.0, -9.0), v(0.0, -3.0)];
const SOUTH: [Vec2; 2] = [v(0.0, 9.0), v(0.0, 3.0)];
const EAST: [Vec2; 2] = [v(9.0, 0.0), v(3.0, 0.0)];

/// The keep's doors: a point outside each, and one inside.
pub const DOORS: [[Vec2; 2]; 3] = [NORTH, SOUTH, EAST];

/// Inside the keep's walls (which stand at +-6 m).
pub fn in_keep(p: Vec2) -> bool {
    p.x.abs() < 6.4 && p.y.abs() < 6.4
}

/// Where to head for `to` from `from` when there's no clear line: through
/// the keep's best door if one of the two is inside it and the other isn't,
/// otherwise straight at it.
pub fn way_to(from: Vec2, to: Vec2) -> Vec2 {
    let (inside, target_inside) = (in_keep(from), in_keep(to));
    if inside == target_inside {
        return to;
    }
    // [near side, far side] for each door.
    let side = |d: &[Vec2; 2]| if inside { [d[1], d[0]] } else { [d[0], d[1]] };
    let best = DOORS
        .iter()
        .map(side)
        .min_by(|a, b| {
            let cost = |d: &[Vec2; 2]| from.distance(d[0]) + d[0].distance(to);
            cost(a).total_cmp(&cost(b))
        })
        .unwrap_or([to, to]);
    // Close to the near side: go on through.
    if from.distance(best[0]) < 1.2 {
        best[1]
    } else {
        best[0]
    }
}

/// In the order they open: the four beside the north and south doors first.
pub const CRACKS: [Crack; 12] = [
    crack(0, &[NORTH[0], NORTH[1]]),
    crack(1, &[SOUTH[0], SOUTH[1]]),
    crack(2, &[NORTH[0], NORTH[1]]),
    crack(3, &[v(2.5, 20.0), SOUTH[0], SOUTH[1]]),
    crack(4, &[v(10.0, 4.0), EAST[0], EAST[1]]),
    crack(5, &[v(-2.0, 11.0), SOUTH[0], SOUTH[1]]),
    crack(6, &[v(2.0, -11.0), NORTH[0], NORTH[1]]),
    crack(7, &[v(-8.0, 9.5), SOUTH[0], SOUTH[1]]),
    crack(8, &[v(10.0, 3.0), EAST[0], EAST[1]]),
    crack(9, &[v(-8.0, 9.5), SOUTH[0], SOUTH[1]]),
    crack(10, &[v(-8.0, 10.0), SOUTH[0], SOUTH[1]]),
    crack(11, &[EAST[0], EAST[1]]),
];

/// How many points a zombie from crack `i` walks through.
pub fn route_len(i: usize) -> usize {
    CRACKS[i].route.len() + 1
}

/// Point `n` of crack `i`'s way to the keep: first 4 m straight in through
/// the hole, then its route.
pub fn waypoint(i: usize, n: usize) -> Vec2 {
    let c = &CRACKS[i];
    if n == 0 {
        c.at + c.inward * 4.0
    } else {
        c.route[n - 1]
    }
}

/// Where a zombie stands as it arrives at crack `i`: outside the wall,
/// in front of the hole, on the ground (collider-center height).
pub fn entry(i: usize) -> Vec3 {
    let c = &CRACKS[i];
    let p = c.at - c.inward * 2.0;
    Vec3::new(p.x, 0.9, p.y)
}

/// Opens and closes the breaches to match the run's open cracks, on every
/// machine (the server needs it for shots, each player for walking).
pub struct BreachPlugin;

impl Plugin for BreachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, open_breaches);
    }
}

/// Add [`BreachPlugin`] unless it's there already (a host has both the
/// server's and the player's survival plugins).
pub fn add_breaches(app: &mut App) {
    if !app.is_plugin_added::<BreachPlugin>() {
        app.add_plugins(BreachPlugin);
    }
}

fn open_breaches(
    runs: Query<&Survival>,
    mut plugs: Query<(Entity, &Breach, Option<&mut Visibility>, Has<Collider>)>,
    mut shown: Local<u16>,
    mut commands: Commands,
) {
    let open = runs.single().map_or(0, |r| r.cracks);
    if open == *shown {
        return;
    }
    *shown = open;
    for (entity, breach, visibility, collider) in &mut plugs {
        let is_open = open & (1 << breach.0) != 0;
        if let Some(mut visibility) = visibility {
            visibility.set_if_neq(if is_open {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        }
        if collider {
            commands.entity(entity).insert(if is_open {
                CollisionLayers::new(Layer::Barrier, Layer::LocalBody)
            } else {
                CollisionLayers::new(Layer::World, LayerMask::ALL)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaving_the_keep_goes_by_a_door() {
        // From inside, toward the south yard: the south door's inside point,
        // then out through it.
        assert_eq!(way_to(v(2.0, 2.5), v(0.0, 16.0)), v(0.0, 3.0));
        assert_eq!(way_to(v(0.3, 3.4), v(0.0, 16.0)), v(0.0, 9.0));
        // From outside the west wall to inside: round to a door, not
        // through the wall.
        let first = way_to(v(-10.0, 0.0), v(-2.0, 0.0));
        assert!(DOORS.iter().any(|d| d[0] == first), "{first:?}");
        // Both outside: straight there.
        assert_eq!(way_to(v(10.0, 10.0), v(20.0, 20.0)), v(20.0, 20.0));
    }

    #[test]
    fn cracks_sit_on_the_walls_and_face_in() {
        for c in &CRACKS {
            let on_wall = c.at.x.abs() == 36.0 || c.at.y.abs() == 36.0;
            assert!(on_wall, "{:?}", c.at);
            // A step inward heads toward the middle.
            assert!((c.at + c.inward).length() < c.at.length());
            assert!(c.route.len() >= 2);
        }
    }

    #[test]
    fn zombies_climb_straight_in_through_the_hole() {
        for (i, c) in CRACKS.iter().enumerate() {
            let from = entry(i);
            let from = Vec2::new(from.x, from.z);
            let to = waypoint(i, 0);
            // Outside the wall, then inside, on one line through the hole.
            assert!(from.length() > c.at.length() && to.length() < c.at.length());
            let across = (to - from).normalize().perp_dot(c.at - from).abs();
            assert!(across < 0.01, "crack {i}");
        }
    }

    #[test]
    fn every_route_ends_inside_the_keep() {
        for c in &CRACKS {
            let last = *c.route.last().unwrap();
            assert!(last.x.abs() < 6.0 && last.y.abs() < 6.0, "{last:?}");
        }
    }
}
