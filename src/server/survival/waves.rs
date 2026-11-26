//! The run's clock: breathers and waves, how big each wave is, which
//! cracks are open, and when the run is over. [`step`] is a plain function
//! of the state, so it's unit tested without the game.

use crate::{
    config::{
        SURVIVAL_BREATHER_SECS, SURVIVAL_FIRST_BREATHER_SECS, SURVIVAL_OVER_SECS,
        SURVIVAL_WAVE_START,
    },
    survival::protocol::{Phase, Survival},
};

/// How many zombies wave `n` (from 1) brings.
pub fn wave_size(n: u16) -> u16 {
    SURVIVAL_WAVE_START + 2 * n.saturating_sub(1)
}

/// How many cracks are open during wave `n`: 1, then 2, then one more
/// every second wave, up to `total`.
pub fn cracks_open(n: u16, total: usize) -> usize {
    let open = match n {
        0 | 1 => 1,
        2 => 2,
        n => 2 + (n as usize - 2) / 2,
    };
    open.min(total)
}

/// What [`step`] asks the rest of survival to do.
#[derive(Debug, PartialEq)]
pub enum Cue {
    /// A breather began: offer cards, top up ammo, bring back the fallen.
    Breather,
    /// Wave `n` began: queue its zombies.
    Wave(u16),
    /// Everyone is down: the run is over.
    Over,
    /// The run starts again from the beginning.
    Restart,
}

/// What [`step`] needs to know about the game this frame.
pub struct Seen {
    /// Zombies alive or still queued at the cracks.
    pub zombies: u16,
    /// Every player is down or out (and there is at least one player).
    pub all_down: bool,
}

/// A fresh run: a short first breather, then wave 1.
pub fn new_run() -> Survival {
    Survival {
        phase: Phase::Breather,
        wave: 0,
        secs: SURVIVAL_FIRST_BREATHER_SECS as u8,
        left: 0,
        cracks: 0,
    }
}

/// Advance the run by `dt` seconds. `clock` holds the seconds left in a
/// breather or the end screen (the replicated `secs` is its whole seconds).
pub fn step(run: &mut Survival, clock: &mut f32, dt: f32, seen: &Seen) -> Option<Cue> {
    match run.phase {
        Phase::Breather => {
            *clock -= dt;
            run.secs = clock.max(0.0).ceil() as u8;
            if *clock <= 0.0 {
                run.wave += 1;
                run.phase = Phase::Wave;
                run.left = wave_size(run.wave);
                return Some(Cue::Wave(run.wave));
            }
        }
        Phase::Wave => {
            run.left = seen.zombies;
            if seen.all_down {
                run.phase = Phase::Over;
                *clock = SURVIVAL_OVER_SECS;
                run.secs = SURVIVAL_OVER_SECS as u8;
                return Some(Cue::Over);
            }
            if seen.zombies == 0 {
                run.phase = Phase::Breather;
                *clock = SURVIVAL_BREATHER_SECS;
                run.secs = SURVIVAL_BREATHER_SECS as u8;
                return Some(Cue::Breather);
            }
        }
        Phase::Over => {
            *clock -= dt;
            run.secs = clock.max(0.0).ceil() as u8;
            if *clock <= 0.0 {
                *run = new_run();
                *clock = SURVIVAL_FIRST_BREATHER_SECS;
                return Some(Cue::Restart);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(zombies: u16, all_down: bool) -> Seen {
        Seen { zombies, all_down }
    }

    #[test]
    fn waves_grow() {
        assert_eq!(wave_size(1), 3);
        assert_eq!(wave_size(2), 5);
        assert_eq!(wave_size(10), 21);
    }

    #[test]
    fn cracks_open_up_over_the_waves() {
        let opened: Vec<usize> = (1..=30).map(|n| cracks_open(n, 12)).collect();
        assert_eq!(&opened[..6], &[1, 2, 2, 3, 3, 4]);
        assert!(opened.windows(2).all(|w| w[1] >= w[0]));
        assert_eq!(*opened.last().unwrap(), 12);
    }

    #[test]
    fn a_run_goes_breather_wave_breather() {
        let mut run = new_run();
        let mut clock = SURVIVAL_FIRST_BREATHER_SECS;
        assert_eq!(step(&mut run, &mut clock, 1.0, &seen(0, false)), None);
        assert_eq!(run.secs, (SURVIVAL_FIRST_BREATHER_SECS - 1.0).ceil() as u8);
        let cue = step(&mut run, &mut clock, 100.0, &seen(0, false));
        assert_eq!(cue, Some(Cue::Wave(1)));
        assert_eq!(run.left, 3);
        assert_eq!(step(&mut run, &mut clock, 1.0, &seen(2, false)), None);
        assert_eq!(run.left, 2);
        assert_eq!(
            step(&mut run, &mut clock, 1.0, &seen(0, false)),
            Some(Cue::Breather)
        );
        assert_eq!(run.phase, Phase::Breather);
        assert_eq!(
            step(&mut run, &mut clock, 100.0, &seen(0, false)),
            Some(Cue::Wave(2))
        );
        assert_eq!(run.left, 5);
    }

    #[test]
    fn everyone_down_ends_the_run_and_it_starts_again() {
        let mut run = new_run();
        let mut clock = 0.0;
        step(&mut run, &mut clock, 1.0, &seen(0, false));
        assert_eq!(
            step(&mut run, &mut clock, 0.1, &seen(3, true)),
            Some(Cue::Over)
        );
        assert_eq!(run.wave, 1);
        assert_eq!(
            step(&mut run, &mut clock, 100.0, &seen(3, true)),
            Some(Cue::Restart)
        );
        assert_eq!(run.wave, 0);
        assert_eq!(run.phase, Phase::Breather);
    }
}
