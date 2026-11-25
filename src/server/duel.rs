//! The duel game mode: two players, one flintlock each, best of five rounds.
//!
//! `mode duel` turns it on and `mode ffa` back off. While it's on there is
//! one replicated [`Duel`] entity, and every round goes:
//!
//! 1. **Walk**: the duelists start back to back at [`DUEL_START`] and walk to
//!    their marks ([`DUEL_MARKS`]).
//! 2. **Steady**: both are on their marks. If either steps off, back to Walk.
//! 3. **Draw**: after a random moment, fire! Shots before this are ignored.
//! 4. **RoundOver**: the first kill wins the round (see `Hits::hurt`).
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
    console::{ConsoleAppExt, ConsoleCommand, Print},
    net::{Duel, DuelPhase, Feed, Health, Out, Player, Pose, Respawn, Score},
    server::{
        bots::Bot,
        players::{Joined, PlayerClient, pick_spawn, respawn_at},
    },
};

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.add_systems(Update, mode_command)
        .add_systems(Update, run_duel.run_if(authority));
    app.console_command(
        "mode",
        "[ffa|duel]",
        "the game mode: free-for-all or a flintlock duel (host or offline)",
    );
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

/// Put a player somewhere: a real player is told where to respawn, a bot is
/// simply moved.
fn place(
    client: Option<&PlayerClient>,
    bot: Option<Mut<Bot>>,
    (pos, yaw): (Vec3, f32),
    respawns: &mut MessageWriter<ToClients<Respawn>>,
) {
    if let Some(client) = client {
        respawn_at(client.0, pos, yaw, respawns);
    }
    if let Some(mut bot) = bot {
        bot.place(pos, yaw);
    }
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

#[allow(clippy::too_many_arguments)]
fn mode_command(
    mut cmds: MessageReader<ConsoleCommand>,
    state: Res<State<ClientState>>,
    duels: Query<(Entity, &Duel)>,
    mut players: Query<(
        Entity,
        &mut Score,
        &mut Health,
        Option<&PlayerClient>,
        Option<&mut Bot>,
    )>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut print: MessageWriter<Print>,
    mut commands: Commands,
) {
    for cmd in cmds.read() {
        if cmd.name != "mode" {
            continue;
        }
        let current = if duels.is_empty() { "ffa" } else { "duel" };
        let Some(wanted) = cmd.arg(0) else {
            print.write(Print(format!("mode: {current}")));
            continue;
        };
        if *state.get() != ClientState::Disconnected {
            print.write(Print::error("only the host can change the mode"));
            continue;
        }
        if wanted == current {
            print.write(Print(format!("already in {current}")));
            continue;
        }
        match wanted {
            "duel" => {
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
                for (_, mut score, ..) in &mut players {
                    *score = Score::default();
                }
                feed.write(ToClients {
                    targets: SendTargets::All,
                    message: Feed::Note(format!(
                        "Duel! Walk to your mark, wait for the bell, first to {}",
                        Duel::WINS
                    )),
                });
            }
            "ffa" => {
                for (entity, _) in &duels {
                    commands.entity(entity).despawn();
                }
                for (entity, mut score, mut health, client, bot) in &mut players {
                    *score = Score::default();
                    health.0 = MAX_HEALTH;
                    commands.entity(entity).remove::<Out>();
                    place(client, bot, pick_spawn(), &mut respawns);
                }
                feed.write(ToClients {
                    targets: SendTargets::All,
                    message: Feed::Note("Free-for-all".into()),
                });
            }
            _ => {
                print.write(Print::error("usage: mode <ffa|duel>"));
            }
        }
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
