//! What special ammo leaves on a zombie: burning, chill and frozen solid.
//! Ticked here; shown to everyone through `Mob::flags`.

use bevy::prelude::*;

use crate::{
    net::{Dead, Mob},
    server::combat::Hits,
    survival::protocol::{BURNING, CHILLED, FROZEN},
    weapon::WeaponKind,
};

/// Chill hits in a row that freeze a zombie solid.
const FREEZE_AT: u8 = 4;
const FROZEN_SECS: f32 = 2.0;
/// How long chill lasts after the last frost hit.
const CHILL_SECS: f32 = 3.0;
/// Burn damage lands this often.
const BURN_TICK: f32 = 0.5;

/// Server-only: what's happening to a zombie.
#[derive(Component, Default, Debug)]
pub struct Status {
    burn: f32,
    burn_dps: f32,
    burn_by: Option<Entity>,
    burn_tick: f32,
    chill: u8,
    chill_left: f32,
    frozen: f32,
}

impl Status {
    pub fn burning(&self) -> bool {
        self.burn > 0.0
    }

    pub fn chilled(&self) -> bool {
        self.chill > 0
    }

    pub fn frozen(&self) -> bool {
        self.frozen > 0.0
    }

    /// How fast it can move: slowed by chill, stopped while frozen.
    pub fn speed_scale(&self) -> f32 {
        if self.frozen() {
            0.0
        } else if self.chilled() {
            0.6
        } else {
            1.0
        }
    }

    /// Set it burning for `secs` at `dps`, on behalf of `by` (who scores
    /// the kill).
    pub fn ignite(&mut self, by: Entity, secs: f32, dps: f32) {
        self.burn = self.burn.max(secs);
        self.burn_dps = self.burn_dps.max(dps);
        self.burn_by = Some(by);
    }

    /// One more frost hit. True if that froze it solid.
    pub fn chill(&mut self) -> bool {
        self.chill_left = CHILL_SECS;
        if self.frozen() {
            return false;
        }
        self.chill += 1;
        if self.chill >= FREEZE_AT {
            self.chill = 0;
            self.frozen = FROZEN_SECS;
            return true;
        }
        false
    }

    /// Clear burning and chill (a thermal shock spends both).
    pub fn quench(&mut self) {
        self.burn = 0.0;
        self.chill = 0;
    }

    fn flags(&self) -> u8 {
        let mut flags = 0;
        if self.burning() {
            flags |= BURNING;
        }
        if self.chilled() {
            flags |= CHILLED;
        }
        if self.frozen() {
            flags |= FROZEN;
        }
        flags
    }
}

/// Burn, thaw, and show it.
pub fn tick(
    time: Res<Time>,
    mut zombies: Query<(Entity, &mut Status, &mut Mob), Without<Dead>>,
    mut hits: Hits,
) {
    let dt = time.delta_secs();
    let mut burns = Vec::new();
    for (entity, mut status, mut mob) in &mut zombies {
        if status.burn > 0.0 {
            status.burn -= dt;
            status.burn_tick -= dt;
            if status.burn_tick <= 0.0 {
                status.burn_tick = BURN_TICK;
                if let Some(by) = status.burn_by {
                    burns.push((entity, by, status.burn_dps * BURN_TICK));
                }
            }
        } else {
            status.burn_dps = 0.0;
        }
        status.frozen = (status.frozen - dt).max(0.0);
        status.chill_left -= dt;
        if status.chill_left <= 0.0 {
            status.chill = 0;
        }
        let flags = status.flags();
        if mob.flags != flags {
            mob.flags = flags;
        }
    }
    for (target, by, amount) in burns {
        // The fire was lit by a gun; score it to whoever lit it.
        if let Some(hurt) = hits.hurt(by, target, amount, false, WeaponKind::Flintlock)
            && hurt.killed
        {
            hits.wounded(target, &hurt, Vec3::Y, hurt.pos, WeaponKind::Flintlock);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_frost_hits_freeze() {
        let mut s = Status::default();
        assert!(!s.chill() && !s.chill() && !s.chill());
        assert!(s.chilled());
        assert!(s.chill());
        assert!(s.frozen());
        assert_eq!(s.speed_scale(), 0.0);
        // Hits on a frozen zombie don't stack more.
        assert!(!s.chill());
    }

    #[test]
    fn fire_keeps_the_hottest_and_longest() {
        let mut s = Status::default();
        s.ignite(Entity::PLACEHOLDER, 3.0, 8.0);
        s.ignite(Entity::PLACEHOLDER, 1.0, 12.0);
        assert!(s.burning());
        assert_eq!(s.burn, 3.0);
        assert_eq!(s.burn_dps, 12.0);
        s.quench();
        assert!(!s.burning());
    }
}
