//! The cracks in the outer walls that zombies climb through, and the way
//! each one leads to the keep. Shared by the server (where zombies spawn and
//! walk) and every player's machine (where cracks are drawn).
//!
//! The map's middle is the walled keep (`world/arena.rs`), with doors on the
//! north, south and east sides and a solid west wall, so every route ends at
//! a door. The points were checked against the map's fixed pieces: none runs
//! through a pillar, ramp or platform. Random rocks and trees may still be in
//! the way, which the zombies' steering handles.

use bevy::math::{Vec2, Vec3};

/// One crack: where it is on the wall's inner face, which way is inward,
/// and the points a zombie walks through to reach the keep.
pub struct Crack {
    pub at: Vec2,
    pub inward: Vec2,
    pub route: &'static [Vec2],
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
    Crack {
        at: v(3.94, -36.0),
        inward: v(0.0, 1.0),
        route: &[v(3.94, -32.0), NORTH[0], NORTH[1]],
    },
    Crack {
        at: v(-3.94, 36.0),
        inward: v(0.0, -1.0),
        route: &[v(-3.94, 32.0), SOUTH[0], SOUTH[1]],
    },
    Crack {
        at: v(-3.94, -36.0),
        inward: v(0.0, 1.0),
        route: &[v(-3.94, -32.0), NORTH[0], NORTH[1]],
    },
    Crack {
        at: v(3.94, 36.0),
        inward: v(0.0, -1.0),
        route: &[v(3.94, 32.0), v(2.5, 20.0), SOUTH[0], SOUTH[1]],
    },
    Crack {
        at: v(36.0, 23.62),
        inward: v(-1.0, 0.0),
        route: &[v(32.0, 23.62), v(10.0, 4.0), EAST[0], EAST[1]],
    },
    Crack {
        at: v(-9.84, 36.0),
        inward: v(0.0, -1.0),
        route: &[v(-9.84, 32.0), v(-2.0, 11.0), SOUTH[0], SOUTH[1]],
    },
    Crack {
        at: v(9.84, -36.0),
        inward: v(0.0, 1.0),
        route: &[v(9.84, -32.0), v(2.0, -11.0), NORTH[0], NORTH[1]],
    },
    Crack {
        at: v(-36.0, 23.62),
        inward: v(1.0, 0.0),
        route: &[v(-32.0, 23.62), v(-8.0, 9.5), SOUTH[0], SOUTH[1]],
    },
    Crack {
        at: v(36.0, 19.68),
        inward: v(-1.0, 0.0),
        route: &[v(32.0, 19.68), v(10.0, 3.0), EAST[0], EAST[1]],
    },
    Crack {
        at: v(-36.0, 19.68),
        inward: v(1.0, 0.0),
        route: &[v(-32.0, 19.68), v(-8.0, 9.5), SOUTH[0], SOUTH[1]],
    },
    Crack {
        at: v(-36.0, 9.84),
        inward: v(1.0, 0.0),
        route: &[v(-32.0, 9.84), v(-8.0, 10.0), SOUTH[0], SOUTH[1]],
    },
    Crack {
        at: v(36.0, 1.97),
        inward: v(-1.0, 0.0),
        route: &[v(32.0, 1.97), EAST[0], EAST[1]],
    },
];

/// Where a zombie stands as it comes through crack `i`: just inside the
/// wall, on the ground (collider-center height).
pub fn entry(i: usize) -> Vec3 {
    let c = &CRACKS[i];
    let p = c.at + c.inward * 0.8;
    Vec3::new(p.x, 0.9, p.y)
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
    fn every_route_ends_inside_the_keep() {
        for c in &CRACKS {
            let last = *c.route.last().unwrap();
            assert!(last.x.abs() < 6.0 && last.y.abs() < 6.0, "{last:?}");
        }
    }
}
