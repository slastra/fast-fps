//! Survival, the game mode's rules: co-op waves of zombies through cracks in
//! the walls, a draft of upgrades between waves, and players who go down
//! rather than die. `mode survival` starts it. See docs/survival.md.
//!
//! - [`waves`]: breathers and waves, their size, the cracks they open.
//! - [`zombies`]: spawning at the cracks, walking to the keep, attacking.
//! - [`status`]: burning, chill and frozen.
//! - [`effects`]: what special ammo, combos and perks do on a hit.
//! - [`downed`]: going down, reviving, bleeding out.
//! - [`drops`]: ammo boxes and medkits.
//! - [`kinds`]: the leaper's leap, the bloater's burst and gas.
//! - [`draft`]: offering and taking cards.
//! - [`loadout`]: what a player's cards give them: weapons, rounds, mods.
//! - [`commands`]: console commands for trying things out and for tests.
//!
//! Every system here runs only while the [`Survival`] entity exists. What a
//! player has follows from their `Upgrades` (`survival/cards.rs`), turned into
//! the core's `Kit` and `WeaponMods` by [`loadout`].

mod commands;
mod downed;
mod draft;
mod drops;
mod effects;
mod kinds;
mod loadout;
mod status;
mod waves;
mod zombies;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use rand::Rng;

use crate::{
    config::{MAX_HEALTH, SURVIVAL_FIRST_BREATHER_SECS, SURVIVAL_KEEP},
    net::{Dead, Feed, Health, Out, Player, Pose, Respawn, to_all},
    server::{
        bots::Bot,
        players::{PlayerClient, place},
        rules::{Disarmed, GameModeAppExt, ModeChange, ModeSwitch, Rules, Unstuck, Untouchable},
    },
    survival::{
        cards::{Build, Perk},
        cracks::CRACKS,
        protocol::{Fallen, Offer, Pickup, Survival, Upgrades},
    },
};
use waves::{Cue, Seen};
use zombies::{Spawner, Zombie, fill_wave};

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.game_mode("survival")
        .init_resource::<Spawner>()
        .add_systems(
            Update,
            (
                start_or_stop,
                set_up_players,
                run,
                draft::offers,
                downed::return_fallen,
                loadout::equip_all,
            )
                .chain()
                .after(ModeSwitch)
                .run_if(authority.clone()),
        )
        .add_systems(
            Update,
            (
                zombies::spawn,
                kinds::leap,
                zombies::steer,
                zombies::attack,
                kinds::bursts,
                kinds::gas,
                status::tick,
                effects::on_hit,
                effects::on_boom,
                effects::fires,
                downed::fall,
                downed::revive,
                downed::bleed,
                downed::medics,
                drops::drop,
                drops::collect,
                draft::receive_picks,
                unstuck,
                commands::give_card,
                commands::test_commands,
            )
                .chain()
                .after(loadout::equip_all)
                .run_if(authority)
                .run_if(any_with_component::<Survival>),
        );
    commands::plugin(app);
    downed::plugin(app);
    draft::plugin(app);
    crate::survival::cracks::add_breaches(app);
}

/// Server-only: seconds left in a breather or on the end screen.
#[derive(Component, Default)]
struct RunClock(f32);

pub use loadout::{Vigor, resupply};

/// Everything survival puts on a player, for taking it off again.
type Gear = (
    Upgrades,
    Build,
    Offer,
    Vigor,
    Fallen,
    downed::Bleed,
    downed::RiseIn,
    downed::WindUsed,
);

/// `mode survival` starts a run; any other mode ends it.
#[allow(clippy::too_many_arguments)]
fn start_or_stop(
    mut changes: MessageReader<ModeChange>,
    runs: Query<Entity, With<Survival>>,
    zombies: Query<Entity, Or<(With<Zombie>, With<kinds::Gas>)>>,
    pickups: Query<Entity, With<Pickup>>,
    players: Query<Entity, With<Player>>,
    mut rules: ResMut<Rules>,
    mut spawner: ResMut<Spawner>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut commands: Commands,
) {
    for change in changes.read() {
        for entity in runs.iter().chain(&zombies).chain(&pickups) {
            commands.entity(entity).despawn();
        }
        for entity in &players {
            commands.entity(entity).remove::<Gear>();
        }
        *spawner = Spawner::default();
        if change.to != "survival" {
            continue;
        }
        *rules = Rules {
            // The fallen go down instead (`downed`).
            respawn: false,
            friendly_fire: false,
            dummies: false,
            // They kneel instead.
            player_corpses: false,
        };
        commands.spawn((
            Replicated,
            waves::new_run(),
            RunClock(SURVIVAL_FIRST_BREATHER_SECS),
        ));
        feed.write(to_all(Feed::Note(
            "Survival! Hold the keep. Zombies come through the cracks".into(),
        )));
    }
}

/// Anyone without survival's gear gets a fresh start: a Glock, a knife and
/// no cards, inside the keep. (New players, and everyone at a restart.)
#[allow(clippy::type_complexity)]
fn set_up_players(
    runs: Query<(), With<Survival>>,
    mut players: Query<
        (Entity, &mut Health, Option<&PlayerClient>, Option<&mut Bot>),
        (With<Player>, Without<Upgrades>),
    >,
    mut respawns: MessageWriter<ToClients<Respawn>>,
    mut commands: Commands,
) {
    if runs.is_empty() {
        return;
    }
    for (i, (entity, mut health, client, bot)) in players.iter_mut().enumerate() {
        health.0 = MAX_HEALTH;
        commands.entity(entity).insert(loadout::fresh());
        commands
            .entity(entity)
            .remove::<(Out, Disarmed, Untouchable, Fallen, downed::Bleed)>();
        place(client, bot, keep_spot(i), &mut respawns);
    }
}

/// A spot inside the keep, facing out of a door.
pub fn keep_spot(i: usize) -> (Vec3, f32) {
    let pos = SURVIVAL_KEEP[i % SURVIVAL_KEEP.len()];
    let yaw = (-pos.x).atan2(-pos.z) + std::f32::consts::PI;
    (pos, yaw)
}

/// The run's clock, and what happens as it moves on.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run(
    time: Res<Time>,
    mut runs: Query<(&mut Survival, &mut RunClock)>,
    alive: Query<(), (With<Zombie>, Without<Dead>)>,
    zombies: Query<Entity, With<Zombie>>,
    players: Query<(Entity, &Build, &Pose, Has<Fallen>, Has<Out>), With<PlayerClient>>,
    everyone: Query<Entity, With<Player>>,
    mut spawner: ResMut<Spawner>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut breathers: MessageWriter<draft::Breather>,
    mut waves_begun: MessageWriter<draft::WaveBegun>,
    mut commands: Commands,
) {
    let Ok((mut shared, mut clock)) = runs.single_mut() else {
        return;
    };
    let mut run = shared.clone();
    let seen = Seen {
        zombies: alive.iter().count() as u16 + spawner.queue.len() as u16,
        all_down: !players.is_empty() && players.iter().all(|p| p.3 || p.4),
    };
    match waves::step(&mut run, &mut clock.0, time.delta_secs(), &seen) {
        Some(Cue::Wave(n)) => {
            let mut rng = rand::rng();
            spawner.wave = n;
            spawner.queue = fill_wave(n, waves::wave_size(n) as usize, &mut rng);
            spawner.timer = 0.5;
            let open = waves::cracks_open(n, CRACKS.len());
            run.cracks = (1u16 << open) - 1;
            spawner.weights = crack_weights(open, &players);
            run.left = seen.zombies.max(waves::wave_size(n));
            waves_begun.write(draft::WaveBegun);
            feed.write(to_all(Feed::Note(format!("Wave {n}"))));
        }
        Some(Cue::Breather) => {
            breathers.write(draft::Breather(run.wave));
        }
        Some(Cue::Over) => {
            feed.write(to_all(Feed::Note(format!(
                "Everyone is down. The run ends at wave {}",
                run.wave
            ))));
        }
        Some(Cue::Restart) => {
            for entity in &zombies {
                commands.entity(entity).despawn();
            }
            *spawner = Spawner::default();
            // Everyone starts over (`set_up_players`), bots too.
            for entity in &everyone {
                commands.entity(entity).remove::<Gear>();
            }
        }
        None => {}
    }
    if *shared != run {
        *shared = run;
    }
}

/// How often each open crack is used: evenly, but a third less for the one
/// nearest each player with the Barricade perk.
fn crack_weights(
    open: usize,
    players: &Query<(Entity, &Build, &Pose, Has<Fallen>, Has<Out>), With<PlayerClient>>,
) -> Vec<(usize, f32)> {
    let mut weights: Vec<(usize, f32)> = (0..open).map(|i| (i, 1.0)).collect();
    for (_, build, pose, ..) in players {
        if !build.has(Perk::Barricade) {
            continue;
        }
        let at = Vec2::new(pose.pos.x, pose.pos.z);
        if let Some(nearest) = weights.iter_mut().min_by(|a, b| {
            CRACKS[a.0]
                .at
                .distance(at)
                .total_cmp(&CRACKS[b.0].at.distance(at))
        }) {
            nearest.1 *= 0.7;
        }
    }
    weights
}

/// `kill` or a fall in survival: back into the keep.
fn unstuck(
    mut requests: MessageReader<Unstuck>,
    mut players: Query<(Option<&PlayerClient>, Option<&mut Bot>)>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
) {
    let mut rng = rand::rng();
    for Unstuck(entity) in requests.read() {
        if let Ok((client, bot)) = players.get_mut(*entity) {
            place(
                client,
                bot,
                keep_spot(rng.random_range(0..4)),
                &mut respawns,
            );
        }
    }
}
