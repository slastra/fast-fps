//! Weapon stats and spray patterns, plus the knife and the grenade.
//!
//! Angles are in degrees. A spray pattern is where shot N lands relative to
//! where you aimed, x to the right and y up, if you don't pull against it.
//! The patterns are hand-made keyframes, linearly interpolated per shot.

use bevy::math::{Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum WeaponKind {
    #[default]
    Ak,
    Mp5,
    Glock,
    Shotgun,
    Sniper,
    Knife,
    Grenade,
}

/// What kind of thing a weapon is. Guns shoot bullets; the knife and the
/// grenade have their own rules (see [`KNIFE`] and [`GRENADE`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Gun,
    Knife,
    Grenade,
}

impl WeaponKind {
    /// How many weapons there are. Arrays indexed by weapon use this.
    pub const COUNT: usize = 7;

    pub const ALL: [WeaponKind; Self::COUNT] = [
        WeaponKind::Ak,
        WeaponKind::Mp5,
        WeaponKind::Glock,
        WeaponKind::Shotgun,
        WeaponKind::Sniper,
        WeaponKind::Knife,
        WeaponKind::Grenade,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "1" | "ak" | "ak47" | "rifle" => Some(Self::Ak),
            "2" | "mp5" | "smg" => Some(Self::Mp5),
            "3" | "glock" | "pistol" => Some(Self::Glock),
            "4" | "shotgun" | "pump" => Some(Self::Shotgun),
            "5" | "sniper" | "awp" | "bolt" => Some(Self::Sniper),
            "6" | "knife" => Some(Self::Knife),
            "7" | "grenade" | "nade" | "he" | "frag" => Some(Self::Grenade),
            _ => None,
        }
    }

    pub fn def(self) -> &'static WeaponDef {
        &DEFS[self.index()]
    }
}

pub struct WeaponDef {
    pub name: &'static str,
    pub class: Class,
    /// Seconds between shots.
    pub interval: f32,
    /// Hold to fire, or one shot per click.
    pub automatic: bool,
    /// Rounds per magazine. 0 means no ammo at all (the knife); the grenade
    /// has one, refilled when you respawn rather than by reloading.
    pub mag: u32,
    pub reload_secs: f32,
    pub draw_secs: f32,
    pub damage: f32,
    pub head_mult: f32,
    /// Damage multiplier per 10 m of distance.
    pub falloff_per_10m: f32,
    pub pellets: u32,
    /// Fixed cone for each pellet, on top of inaccuracy.
    pub pellet_spread: f32,
    /// Random cone half-angle while standing still.
    pub stand: f32,
    /// Extra cone at full running speed.
    pub moving: f32,
    /// Extra cone in the air.
    pub airborne: f32,
    /// Multiplier while crouched.
    pub crouch_mult: f32,
    /// Extra cone added per shot in a spray, and its cap.
    pub per_shot: f32,
    pub per_shot_max: f32,
    /// Seconds after the last shot for the spray to fully reset.
    pub recover_secs: f32,
    pub pattern: &'static [(f32, f32, f32)],
    /// Single-shot kick for weapons without a pattern.
    pub kick: f32,
    /// Sniper: (fov multiplier per zoom level).
    pub zoom: &'static [f32],
    /// Cone while scoped (replaces `stand`).
    pub scoped: f32,
}

impl WeaponDef {
    /// Where shot `index` (0-based) of a spray lands, in degrees (x right, y up).
    pub fn pattern_at(&self, index: f32) -> (f32, f32) {
        let keys = self.pattern;
        if keys.is_empty() {
            return (0.0, 0.0);
        }
        if index <= keys[0].0 {
            return (keys[0].1, keys[0].2);
        }
        for pair in keys.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if index <= b.0 {
                let t = (index - a.0) / (b.0 - a.0);
                return (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t);
            }
        }
        let last = keys[keys.len() - 1];
        (last.1, last.2)
    }
}

/// Up for nine shots, hard left, back right, left again.
const AK_PATTERN: &[(f32, f32, f32)] = &[
    (0.0, 0.0, 0.0),
    (1.0, 0.05, 0.4),
    (3.0, -0.1, 1.5),
    (6.0, 0.25, 3.5),
    (9.0, 0.1, 5.0),
    (11.0, -0.9, 5.4),
    (14.0, -2.4, 5.8),
    (17.0, -1.6, 6.0),
    (19.0, 0.4, 5.9),
    (22.0, 2.3, 6.2),
    (25.0, 1.6, 6.5),
    (27.0, -0.2, 6.4),
    (29.0, -1.4, 6.7),
];

/// Gentle climb with a lazy drift to the right then back.
const MP5_PATTERN: &[(f32, f32, f32)] = &[
    (0.0, 0.0, 0.0),
    (2.0, 0.0, 0.5),
    (6.0, 0.15, 1.6),
    (10.0, 0.6, 2.3),
    (15.0, 0.9, 2.6),
    (20.0, 0.2, 2.8),
    (25.0, -0.5, 2.9),
    (29.0, -0.2, 3.0),
];

/// A short vertical climb when you click fast.
const GLOCK_PATTERN: &[(f32, f32, f32)] = &[
    (0.0, 0.0, 0.0),
    (3.0, 0.1, 0.9),
    (8.0, 0.3, 1.8),
    (19.0, 0.2, 2.4),
];

const DEFS: [WeaponDef; WeaponKind::COUNT] = [
    WeaponDef {
        name: "AK",
        class: Class::Gun,
        interval: 0.1,
        automatic: true,
        mag: 30,
        reload_secs: 2.3,
        draw_secs: 0.5,
        damage: 34.0,
        head_mult: 4.0,
        falloff_per_10m: 0.98,
        pellets: 1,
        pellet_spread: 0.0,
        stand: 0.25,
        moving: 4.5,
        airborne: 9.0,
        crouch_mult: 0.7,
        per_shot: 0.12,
        per_shot_max: 1.4,
        recover_secs: 0.45,
        pattern: AK_PATTERN,
        kick: 0.0,
        zoom: &[],
        scoped: 0.0,
    },
    WeaponDef {
        name: "MP5",
        class: Class::Gun,
        interval: 0.075,
        automatic: true,
        mag: 30,
        reload_secs: 2.0,
        draw_secs: 0.4,
        damage: 24.0,
        head_mult: 3.0,
        falloff_per_10m: 0.85,
        pellets: 1,
        pellet_spread: 0.0,
        stand: 0.45,
        moving: 1.8,
        airborne: 6.0,
        crouch_mult: 0.75,
        per_shot: 0.06,
        per_shot_max: 0.9,
        recover_secs: 0.35,
        pattern: MP5_PATTERN,
        kick: 0.0,
        zoom: &[],
        scoped: 0.0,
    },
    WeaponDef {
        name: "Glock",
        class: Class::Gun,
        interval: 0.13,
        automatic: false,
        mag: 20,
        reload_secs: 1.6,
        draw_secs: 0.3,
        damage: 22.0,
        head_mult: 3.2,
        falloff_per_10m: 0.88,
        pellets: 1,
        pellet_spread: 0.0,
        stand: 0.5,
        moving: 1.4,
        airborne: 4.0,
        crouch_mult: 0.8,
        per_shot: 0.25,
        per_shot_max: 2.0,
        recover_secs: 0.3,
        pattern: GLOCK_PATTERN,
        kick: 0.0,
        zoom: &[],
        scoped: 0.0,
    },
    WeaponDef {
        name: "Shotgun",
        class: Class::Gun,
        interval: 0.85,
        automatic: false,
        mag: 8,
        reload_secs: 2.6,
        draw_secs: 0.6,
        damage: 17.0,
        head_mult: 1.6,
        falloff_per_10m: 0.62,
        pellets: 9,
        pellet_spread: 3.6,
        stand: 0.3,
        moving: 1.2,
        airborne: 3.0,
        crouch_mult: 0.9,
        per_shot: 0.0,
        per_shot_max: 0.0,
        recover_secs: 0.8,
        pattern: &[],
        kick: 3.2,
        zoom: &[],
        scoped: 0.0,
    },
    WeaponDef {
        name: "Sniper",
        class: Class::Gun,
        interval: 1.45,
        automatic: false,
        mag: 5,
        reload_secs: 3.0,
        draw_secs: 0.8,
        damage: 115.0,
        head_mult: 2.0,
        falloff_per_10m: 0.995,
        pellets: 1,
        pellet_spread: 0.0,
        stand: 7.0,
        moving: 6.0,
        airborne: 14.0,
        crouch_mult: 0.85,
        per_shot: 0.0,
        per_shot_max: 0.0,
        recover_secs: 1.2,
        pattern: &[],
        kick: 3.0,
        zoom: &[0.45, 0.16],
        scoped: 0.04,
    },
    WeaponDef {
        name: "Knife",
        class: Class::Knife,
        interval: KNIFE.slash_secs,
        automatic: false,
        mag: 0,
        reload_secs: 0.0,
        draw_secs: 0.4,
        damage: KNIFE.slash,
        head_mult: 1.0,
        falloff_per_10m: 1.0,
        pellets: 1,
        pellet_spread: 0.0,
        stand: 0.3,
        moving: 0.0,
        airborne: 0.0,
        crouch_mult: 1.0,
        per_shot: 0.0,
        per_shot_max: 0.0,
        recover_secs: 0.3,
        pattern: &[],
        kick: 0.0,
        zoom: &[],
        scoped: 0.0,
    },
    WeaponDef {
        name: "Grenade",
        class: Class::Grenade,
        interval: 0.6,
        automatic: false,
        mag: 1,
        reload_secs: 0.0,
        draw_secs: 0.5,
        damage: GRENADE.damage,
        head_mult: 1.0,
        falloff_per_10m: 1.0,
        pellets: 1,
        pellet_spread: 0.0,
        stand: 0.3,
        moving: 0.0,
        airborne: 0.0,
        crouch_mult: 1.0,
        per_shot: 0.0,
        per_shot_max: 0.0,
        recover_secs: 0.3,
        pattern: &[],
        kick: 0.0,
        zoom: &[],
        scoped: 0.0,
    },
];

/// The knife. Left click slashes, right click stabs, and any hit from behind
/// kills.
pub struct Knife {
    pub slash: f32,
    pub slash_secs: f32,
    pub stab: f32,
    pub stab_secs: f32,
    /// How far the blade reaches from the eye, in meters.
    pub reach: f32,
    /// Radius of the swept sphere, so a slash doesn't need pixel aim.
    pub width: f32,
    /// A hit counts as from behind when the victim faces this much the same
    /// way as the attacker (dot product of their horizontal facings).
    pub behind: f32,
}

pub const KNIFE: Knife = Knife {
    slash: 50.0,
    slash_secs: 0.4,
    stab: 65.0,
    stab_secs: 1.0,
    reach: 1.8,
    width: 0.25,
    behind: 0.3,
};

impl Knife {
    /// Whether an attacker aiming along `aim` hits someone facing `victim`
    /// from behind. Both are directions; only their horizontal parts count.
    pub fn is_behind(&self, aim: Vec3, victim_yaw: f32) -> bool {
        let aim = aim.with_y(0.0).normalize_or_zero();
        let facing = Quat::from_rotation_y(victim_yaw) * Vec3::NEG_Z;
        aim.dot(facing) > self.behind
    }
}

/// The frag grenade, Counter-Strike HE style: one per life.
pub struct Grenade {
    /// Throw speed with left click, and the underhand lob with right click.
    pub throw_speed: f32,
    pub lob_speed: f32,
    /// Seconds from release to the bang.
    pub fuse: f32,
    /// Damage at the center, falling linearly to nothing at `radius`.
    pub damage: f32,
    pub radius: f32,
    /// How much of its speed a bounce keeps.
    pub bounce: f32,
}

pub const GRENADE: Grenade = Grenade {
    throw_speed: 17.0,
    lob_speed: 8.0,
    fuse: 2.5,
    damage: 98.0,
    radius: 7.0,
    bounce: 0.35,
};

impl Grenade {
    /// Damage at `distance` meters from the blast, before walls are checked.
    pub fn damage_at(&self, distance: f32) -> f32 {
        (self.damage * (1.0 - distance / self.radius)).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blast_falls_off_to_nothing() {
        assert_eq!(GRENADE.damage_at(0.0), 98.0);
        assert!((GRENADE.damage_at(3.5) - 49.0).abs() < 0.01);
        assert_eq!(GRENADE.damage_at(7.0), 0.0);
        assert_eq!(GRENADE.damage_at(8.0), 0.0);
    }

    #[test]
    fn backstab_only_is_behind() {
        // The victim faces -z (yaw 0).
        assert!(KNIFE.is_behind(Vec3::NEG_Z, 0.0), "same way: behind");
        assert!(!KNIFE.is_behind(Vec3::Z, 0.0), "face to face");
        assert!(!KNIFE.is_behind(Vec3::X, 0.0), "from the side");
        assert!(
            KNIFE.is_behind(Vec3::new(0.3, -0.5, -1.0), 0.0),
            "behind, aiming down"
        );
    }

    #[test]
    fn new_weapons_parse() {
        assert_eq!(WeaponKind::parse("6"), Some(WeaponKind::Knife));
        assert_eq!(WeaponKind::parse("knife"), Some(WeaponKind::Knife));
        assert_eq!(WeaponKind::parse("7"), Some(WeaponKind::Grenade));
        assert_eq!(WeaponKind::parse("nade"), Some(WeaponKind::Grenade));
        for kind in WeaponKind::ALL {
            assert_eq!(kind.def().class == Class::Gun, kind.index() < 5);
        }
    }
}
