//! The duel game mode: two players, one flintlock each, best of five rounds.
//!
//! `mode duel` turns it on and `mode ffa` back off. While it's on there is
//! one replicated [`Duel`] entity, and every round goes:
//!
//! 1. **Walk**: the duelists start back to back at [`DUEL_START`] and walk to
//!    their marks ([`DUEL_MARKS`]).
//! 2. **Steady**: both are on their marks. If either steps off, back to Walk.
//! 3. **Draw**: after a random moment, fire! Shots before this are ignored.
//! 4. **RoundOver**: the first kill wins the round (see `fallen`).
//!
//! The first to [`Duel::WINS`] rounds wins the duel. The winner stays, the
//! loser goes to the back of the line, and the next player steps up.
//! Everyone not dueling is [`Out`]: unseen, unhurt, unable to fire, and
//! watching from the edge of the yard.
//!
//! The round logic is [`step`], a plain function, so it can be unit tested.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use rand::Rng;

use crate::{
    config::{
        DUEL_MARK_RADIUS, DUEL_MARKS, DUEL_OVER_SECS, DUEL_ROUND_OVER_SECS, DUEL_ROUND_SECS,
        DUEL_START, DUEL_STEADY_SECS, DUEL_WATCH, MAX_HEALTH,
    },
    net::{Duel, DuelPhase, Feed, Health, Kit, Out, Player, Pose, Respawn},
    server::{
        bots::Bot,
        players::{Joined, PlayerClient, place},
        rules::{Died, Disarmed, GameModeAppExt, ModeChange, ModeSwitch, Rules, Unstuck},
    },
    weapon::WeaponKind,
};

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.game_mode("duel").add_systems(
        Update,
        (start_or_stop, run_duel, arm, fallen, unstuck)
            .chain()
            .after(ModeSwitch)
            .run_if(authority),
    );
}

/// `mode duel` starts a duel; any other mode ends it.
fn start_or_stop(
    mut changes: MessageReader<ModeChange>,
    duels: Query<Entity, With<Duel>>,
    mut rules: ResMut<Rules>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut commands: Commands,
) {
    for change in changes.read() {
        for entity in &duels {
            commands.entity(entity).despawn();
        }
        if change.to != "duel" {
            continue;
        }
        // The fallen stay down until the next round; `fallen` below.
        rules.respawn = false;
        commands.spawn((
            Replicated,
            Duel {
                phase: DuelPhase::Waiting,
                duelists: [0, 0],
                wins: [0, 0],
                round: 0,
                on_mark: [false, false],
                queue: Vec::new(),
            },
            DuelClock::default(),
        ));
        feed.write(ToClients {
            targets: SendTargets::All,
            message: Feed::Note(format!(
                "Duel! Walk to your mark, wait for the bell, first to {}",
                Duel::WINS
            )),
        });
    }
}

/// A duel is the flintlock only, and duelists can't fire before "Draw!".
fn arm(
    duels: Query<&Duel>,
    players: Query<(Entity, &Player, Option<&Kit>, Has<Disarmed>)>,
    mut commands: Commands,
) {
    let Ok(duel) = duels.single() else {
        return;
    };
    let flintlock = Kit::only(&[WeaponKind::Flintlock]);
    for (entity, player, kit, disarmed) in &players {
        if kit != Some(&flintlock) {
            commands.entity(entity).insert(flintlock.clone());
        }
        let may_fire = duel.phase == DuelPhase::Draw && duel.slot(player.owner).is_some();
        if may_fire && disarmed {
            commands.entity(entity).remove::<Disarmed>();
        } else if !may_fire && !disarmed {
            commands.entity(entity).insert(Disarmed);
        }
    }
}

/// A duelist who falls is out for the rest of the round, and the other
/// wins it.
fn fallen(
    mut deaths: MessageReader<Died>,
    mut duels: Query<(&mut Duel, &mut DuelClock)>,
    owners: Query<&Player>,
    mut commands: Commands,
) {
    let Ok((mut duel, mut clock)) = duels.single_mut() else {
        deaths.clear();
        return;
    };
    for died in deaths.read() {
        let Some(slot) = owners
            .get(died.victim)
            .ok()
            .and_then(|p| duel.slot(p.owner))
        else {
            continue;
        };
        commands.entity(died.victim).insert(Out);
        record_loss(&mut duel, &mut clock.0, slot);
    }
}

/// `kill` or a fall in a duel puts you back where you belong this round.
fn unstuck(
    mut requests: MessageReader<Unstuck>,
    duels: Query<&Duel>,
    mut players: Query<(&Player, Option<&PlayerClient>, Option<&mut Bot>)>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
) {
    let Ok(duel) = duels.single() else {
        requests.clear();
        return;
    };
    for Unstuck(entity) in requests.read() {
        let Ok((player, client, bot)) = players.get_mut(*entity) else {
            continue;
        };
        let spot = duel.slot(player.owner).map_or_else(watch_spot, start_spot);
        place(client, bot, spot, &mut respawns);
    }
}

/// Server-only, beside [`Duel`]: seconds left in the current phase.
#[derive(Component, Default)]
pub struct DuelClock(pub f32);

/// What [`step`] asks of the players.
#[derive(Debug, PartialEq)]
pub enum Cue {
    /// Put the duelists back to back, loaded and healed.
    StartRound,
}

/// The yaw that turns -Z to face along `dir`.
fn yaw_of(dir: Vec3) -> f32 {
    (-dir.x).atan2(-dir.z)
}

/// Where duelist `slot` starts a round: back to back with the other one,
/// facing their own mark.
pub fn start_spot(slot: usize) -> (Vec3, f32) {
    let toward = (DUEL_MARKS[slot] - DUEL_START)
        .with_y(0.0)
        .normalize_or(Vec3::NEG_Z);
    (DUEL_START + toward * 0.5, yaw_of(toward))
}

/// Where everyone else watches from, facing the stage.
pub fn watch_spot() -> (Vec3, f32) {
    (DUEL_WATCH, yaw_of((DUEL_START - DUEL_WATCH).with_y(0.0)))
}

/// Whether a duelist standing at `pos` is on mark `slot`.
pub fn on_mark(slot: usize, pos: Vec3) -> bool {
    pos.with_y(0.0).distance(DUEL_MARKS[slot].with_y(0.0)) < DUEL_MARK_RADIUS
}

/// Keep the line in step with who is here: newcomers join the back, and
/// anyone gone is dropped. A duelist leaving hands the duel to the other.
pub fn sync_queue(duel: &mut Duel, clock: &mut f32, present: &[u64]) {
    duel.queue.retain(|owner| present.contains(owner));
    for owner in present {
        if !duel.queue.contains(owner) {
            duel.queue.push(*owner);
        }
    }
    if matches!(duel.phase, DuelPhase::Waiting | DuelPhase::Over { .. }) {
        return;
    }
    let here = duel.duelists.map(|d| present.contains(&d));
    match here {
        [true, true] => {}
        [true, false] | [false, true] => {
            let winner = if here[0] { 0 } else { 1 };
            duel.phase = DuelPhase::Over { winner };
            *clock = DUEL_OVER_SECS;
        }
        [false, false] => duel.phase = DuelPhase::Waiting,
    }
}

/// Advance the round by `dt` seconds. `steady` is how long Steady lasts if
/// it starts now (random in [`DUEL_STEADY_SECS`]; tests pass a fixed one).
pub fn step(duel: &mut Duel, clock: &mut f32, dt: f32, steady: f32) -> Option<Cue> {
    match duel.phase {
        DuelPhase::Waiting => {
            if duel.queue.len() >= 2 {
                duel.duelists = [duel.queue[0], duel.queue[1]];
                duel.wins = [0, 0];
                duel.round = 0;
                return Some(Cue::StartRound);
            }
        }
        DuelPhase::Walk => {
            if duel.on_mark == [true, true] {
                duel.phase = DuelPhase::Steady;
                *clock = steady;
            }
        }
        DuelPhase::Steady => {
            if duel.on_mark != [true, true] {
                duel.phase = DuelPhase::Walk;
            } else {
                *clock -= dt;
                if *clock <= 0.0 {
                    duel.phase = DuelPhase::Draw;
                    *clock = DUEL_ROUND_SECS;
                }
            }
        }
        DuelPhase::Draw => {
            *clock -= dt;
            if *clock <= 0.0 {
                duel.phase = DuelPhase::RoundOver { winner: None };
                *clock = DUEL_ROUND_OVER_SECS;
            }
        }
        DuelPhase::RoundOver { .. } => {
            *clock -= dt;
            if *clock <= 0.0 {
                if let Some(winner) = duel.wins.iter().position(|&w| w >= Duel::WINS) {
                    duel.phase = DuelPhase::Over {
                        winner: winner as u8,
                    };
                    *clock = DUEL_OVER_SECS;
                } else {
                    return Some(Cue::StartRound);
                }
            }
        }
        DuelPhase::Over { winner } => {
            *clock -= dt;
            if *clock <= 0.0 {
                // The winner stays at the front; the loser waits their turn.
                let slot = winner as usize;
                let (winner, loser) = (duel.duelists[slot], duel.duelists[1 - slot]);
                if let Some(i) = duel.queue.iter().position(|&o| o == loser) {
                    duel.queue.remove(i);
                    duel.queue.push(loser);
                }
                if let Some(i) = duel.queue.iter().position(|&o| o == winner) {
                    duel.queue.remove(i);
                    duel.queue.insert(0, winner);
                }
                duel.phase = DuelPhase::Waiting;
            }
        }
    }
    None
}

/// Duelist `slot` fell. Only a fall after "Draw!" decides a round.
pub fn record_loss(duel: &mut Duel, clock: &mut f32, slot: usize) {
    if duel.phase != DuelPhase::Draw {
        return;
    }
    let winner = 1 - slot;
    duel.wins[winner] += 1;
    duel.phase = DuelPhase::RoundOver {
        winner: Some(winner as u8),
    };
    *clock = DUEL_ROUND_OVER_SECS;
}

#[allow(clippy::type_complexity)]
fn run_duel(
    time: Res<Time>,
    mut duels: Query<(&mut Duel, &mut DuelClock)>,
    mut players: Query<(
        Entity,
        &Player,
        &Pose,
        Has<Out>,
        Option<&PlayerClient>,
        Option<&mut Bot>,
        &mut Health,
    )>,
    joined: Query<&Joined>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
    mut commands: Commands,
) {
    let Ok((mut shared, mut clock)) = duels.single_mut() else {
        return;
    };
    // Work on a copy and write it back only if it changed, so the duel is
    // replicated when something happens rather than every frame.
    let mut duel = shared.clone();

    // Whoever is here when the duel starts lines up in the order they
    // arrived, so on a listen server the host goes first. Anyone who arrives
    // later joins the back.
    let mut arrived: Vec<(Option<Joined>, u64)> = players
        .iter()
        .map(|p| (joined.get(p.0).ok().copied(), p.1.owner))
        .collect();
    // Not yet numbered (this very frame) goes last.
    arrived.sort_by_key(|&(joined, owner)| (joined.is_none(), joined, owner));
    let present: Vec<u64> = arrived.into_iter().map(|(_, owner)| owner).collect();
    sync_queue(&mut duel, &mut clock.0, &present);

    let mut marks = [false; 2];
    for (_, player, pose, out, ..) in &players {
        if let Some(slot) = duel.slot(player.owner) {
            marks[slot] = !out && on_mark(slot, pose.pos);
        }
    }
    duel.on_mark = marks;

    let steady = rand::rng().random_range(DUEL_STEADY_SECS.0..DUEL_STEADY_SECS.1);
    let cue = step(&mut duel, &mut clock.0, time.delta_secs(), steady);

    if cue == Some(Cue::StartRound) {
        duel.round += 1;
        duel.phase = DuelPhase::Walk;
        duel.on_mark = [false, false];
        for (entity, player, _, out, client, bot, mut health) in &mut players {
            match duel.slot(player.owner) {
                Some(slot) => {
                    commands.entity(entity).remove::<Out>();
                    health.0 = MAX_HEALTH;
                    place(client, bot, start_spot(slot), &mut respawns);
                }
                None if !out => {
                    commands.entity(entity).insert(Out);
                    place(client, bot, watch_spot(), &mut respawns);
                }
                None => {}
            }
        }
    } else {
        // Nobody to duel: everyone may walk around. Somebody new while a duel
        // is on: they watch.
        for (entity, player, _, out, client, bot, _) in &mut players {
            let dueling = duel.slot(player.owner).is_some();
            if duel.phase == DuelPhase::Waiting && out {
                commands.entity(entity).remove::<Out>();
            } else if duel.phase != DuelPhase::Waiting && !dueling && !out {
                commands.entity(entity).insert(Out);
                place(client, bot, watch_spot(), &mut respawns);
            }
        }
    }

    if *shared != duel {
        *shared = duel;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duel(queue: &[u64]) -> (Duel, f32) {
        (
            Duel {
                phase: DuelPhase::Waiting,
                duelists: [0, 0],
                wins: [0, 0],
                round: 0,
                on_mark: [false, false],
                queue: queue.to_vec(),
            },
            0.0,
        )
    }

    /// Start a round the way `run_duel` does.
    fn start(d: &mut Duel) {
        d.round += 1;
        d.phase = DuelPhase::Walk;
        d.on_mark = [false, false];
    }

    #[test]
    fn two_players_start_a_round() {
        let (mut d, mut clock) = duel(&[7]);
        assert_eq!(step(&mut d, &mut clock, 0.1, 2.0), None, "one alone waits");
        sync_queue(&mut d, &mut clock, &[7, 9]);
        assert_eq!(step(&mut d, &mut clock, 0.1, 2.0), Some(Cue::StartRound));
        assert_eq!(d.duelists, [7, 9]);
    }

    #[test]
    fn marks_then_steady_then_draw() {
        let (mut d, mut clock) = duel(&[1, 2]);
        step(&mut d, &mut clock, 0.1, 2.0);
        start(&mut d);
        d.on_mark = [true, false];
        step(&mut d, &mut clock, 0.1, 2.0);
        assert_eq!(d.phase, DuelPhase::Walk, "one on a mark isn't enough");
        d.on_mark = [true, true];
        step(&mut d, &mut clock, 0.1, 2.0);
        assert_eq!(d.phase, DuelPhase::Steady);
        step(&mut d, &mut clock, 1.0, 2.0);
        assert_eq!(d.phase, DuelPhase::Steady);
        d.on_mark = [true, false];
        step(&mut d, &mut clock, 0.1, 2.0);
        assert_eq!(d.phase, DuelPhase::Walk, "stepping off starts over");
        d.on_mark = [true, true];
        step(&mut d, &mut clock, 0.1, 2.0);
        step(&mut d, &mut clock, 2.1, 2.0);
        assert_eq!(d.phase, DuelPhase::Draw);
    }

    #[test]
    fn only_a_fall_after_draw_counts() {
        let (mut d, mut clock) = duel(&[1, 2]);
        d.phase = DuelPhase::Steady;
        record_loss(&mut d, &mut clock, 0);
        assert_eq!(d.wins, [0, 0]);
        d.phase = DuelPhase::Draw;
        record_loss(&mut d, &mut clock, 0);
        assert_eq!(d.wins, [0, 1]);
        assert_eq!(d.phase, DuelPhase::RoundOver { winner: Some(1) });
        record_loss(&mut d, &mut clock, 1);
        assert_eq!(d.wins, [0, 1], "a second fall in the same round is ignored");
    }

    #[test]
    fn a_round_with_no_winner_is_replayed() {
        let (mut d, _) = duel(&[1, 2]);
        d.phase = DuelPhase::Draw;
        let mut clock = 0.5;
        step(&mut d, &mut clock, 1.0, 2.0);
        assert_eq!(d.phase, DuelPhase::RoundOver { winner: None });
        assert_eq!(
            step(&mut d, &mut clock, DUEL_ROUND_OVER_SECS, 2.0),
            Some(Cue::StartRound)
        );
        assert_eq!(d.wins, [0, 0]);
    }

    #[test]
    fn three_wins_take_the_duel_and_the_line_turns() {
        let (mut d, mut clock) = duel(&[1, 2, 3]);
        step(&mut d, &mut clock, 0.1, 2.0);
        for round in 0..3 {
            start(&mut d);
            d.phase = DuelPhase::Draw;
            record_loss(&mut d, &mut clock, 0);
            let cue = step(&mut d, &mut clock, DUEL_ROUND_OVER_SECS, 2.0);
            if round < 2 {
                assert_eq!(cue, Some(Cue::StartRound));
            }
        }
        assert_eq!(d.phase, DuelPhase::Over { winner: 1 });
        step(&mut d, &mut clock, DUEL_OVER_SECS, 2.0);
        assert_eq!(d.queue, vec![2, 3, 1], "winner first, loser last");
        assert_eq!(step(&mut d, &mut clock, 0.1, 2.0), Some(Cue::StartRound));
        assert_eq!(d.duelists, [2, 3]);
        assert_eq!(d.wins, [0, 0]);
    }

    #[test]
    fn a_duelist_leaving_hands_over_the_duel() {
        let (mut d, mut clock) = duel(&[1, 2, 3]);
        step(&mut d, &mut clock, 0.1, 2.0);
        start(&mut d);
        sync_queue(&mut d, &mut clock, &[2, 3]);
        assert_eq!(d.phase, DuelPhase::Over { winner: 1 });
        step(&mut d, &mut clock, DUEL_OVER_SECS, 2.0);
        assert_eq!(step(&mut d, &mut clock, 0.1, 2.0), Some(Cue::StartRound));
        assert_eq!(d.duelists, [2, 3]);
    }

    #[test]
    fn duelists_start_back_to_back_facing_their_marks() {
        let (a, yaw_a) = start_spot(0);
        let (b, yaw_b) = start_spot(1);
        assert!(a.distance(b) < 1.5, "close together");
        let facing = |yaw: f32| Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
        assert!(facing(yaw_a).dot(facing(yaw_b)) < -0.99, "opposite ways");
        assert!(facing(yaw_a).dot((DUEL_MARKS[0] - a).with_y(0.0).normalize()) > 0.99);
        assert!(on_mark(0, DUEL_MARKS[0] + Vec3::new(0.5, 1.2, 0.0)));
        assert!(!on_mark(0, a));
    }
}
