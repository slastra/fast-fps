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
//! - [`draft`]: offering and taking cards.
//!
//! Every system here runs only while the [`Survival`] entity exists. What a
//! player has follows from their `Upgrades` (`survival/cards.rs`), turned into
//! the core's `Kit` and `WeaponMods` by [`equip`].

mod downed;
mod draft;
mod drops;
mod effects;
mod status;
mod waves;
mod zombies;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use rand::Rng;

use crate::{
    config::{MAX_HEALTH, SURVIVAL_FIRST_BREATHER_SECS, SURVIVAL_KEEP, SURVIVAL_START_MAGS},
    console::{ConsoleAppExt, ConsoleCommand, Print},
    net::{Feed, Health, Kit, Out, Player, Respawn, WeaponMods},
    server::{
        bots::Bot,
        players::{PlayerClient, place},
        rules::{Disarmed, GameModeAppExt, ModeChange, ModeSwitch, Rules, Unstuck, Untouchable},
    },
    survival::{
        cards::{Build, LEVELED, Perk},
        cracks::CRACKS,
        protocol::{Fallen, Offer, Pickup, Survival, Upgrades},
    },
    weapon::WeaponKind,
};
use waves::{Cue, Seen};
use zombies::{Spawner, Zombie, pick_kind};

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
                equip_all,
            )
                .chain()
                .after(ModeSwitch)
                .run_if(authority.clone()),
        )
        .add_systems(
            Update,
            (
                zombies::spawn,
                zombies::steer,
                zombies::attack,
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
                give_card,
                test_commands,
            )
                .chain()
                .after(equip_all)
                .run_if(authority)
                .run_if(any_with_component::<Survival>),
        );
    downed::plugin(app);
    draft::plugin(app);
    app.console_command(
        "give_card",
        "<card>",
        "survival: take a card at once (host)",
    )
    .console_command(
        "zombie",
        "<walker|runner|brute> [x z]",
        "survival: a zombie now, going for the nearest player (host)",
    )
    .console_command("next_wave", "", "survival: end the breather now (host)")
    .console_command(
        "end_wave",
        "",
        "survival: clear the wave's zombies now (host)",
    );
}

/// Server-only: seconds left in a breather or on the end screen.
#[derive(Component, Default)]
struct RunClock(f32);

/// Server-only: a player's health when full (the Juggernaut perk raises it).
#[derive(Component, Clone, Copy)]
pub struct Vigor(pub f32);

/// Everything survival puts on a player, for taking it off again.
type Gear = (
    Upgrades,
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
    zombies: Query<Entity, With<Zombie>>,
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
        feed.write(ToClients {
            targets: SendTargets::All,
            message: Feed::Note("Survival! Hold the keep. Zombies come through the cracks".into()),
        });
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
        commands.entity(entity).insert((
            Upgrades::default(),
            Offer::default(),
            Vigor(MAX_HEALTH),
            starting_kit(),
            WeaponMods::default(),
        ));
        commands
            .entity(entity)
            .remove::<(Out, Disarmed, Untouchable, Fallen, downed::Bleed)>();
        place(client, bot, keep_spot(i), &mut respawns);
    }
}

/// A Glock with spare magazines, and the knife.
fn starting_kit() -> Kit {
    let mut kit = Kit::only(&[WeaponKind::Knife]);
    kit.give(WeaponKind::Glock, rounds_for(WeaponKind::Glock, None));
    kit
}

/// Rounds a newly owned weapon comes with: a full magazine and spares (or
/// two grenades).
fn rounds_for(weapon: WeaponKind, mods: Option<&WeaponMods>) -> u16 {
    if weapon == WeaponKind::Grenade {
        return 2;
    }
    let mag = WeaponMods::mag_of(mods, weapon) as u16;
    mag * (1 + SURVIVAL_START_MAGS)
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
    alive: Query<(), (With<Zombie>, Without<crate::net::Dead>)>,
    zombies: Query<Entity, With<Zombie>>,
    players: Query<
        (Entity, &Upgrades, &crate::net::Pose, Has<Fallen>, Has<Out>),
        With<PlayerClient>,
    >,
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
            spawner.queue = (0..waves::wave_size(n))
                .map(|_| pick_kind(n, &mut rng))
                .collect();
            spawner.timer = 0.5;
            let open = waves::cracks_open(n, CRACKS.len());
            run.cracks = (1u16 << open) - 1;
            spawner.weights = crack_weights(open, &players);
            run.left = seen.zombies.max(waves::wave_size(n));
            waves_begun.write(draft::WaveBegun);
            feed.write(ToClients {
                targets: SendTargets::All,
                message: Feed::Note(format!("Wave {n}")),
            });
        }
        Some(Cue::Breather) => {
            breathers.write(draft::Breather(run.wave));
        }
        Some(Cue::Over) => {
            feed.write(ToClients {
                targets: SendTargets::All,
                message: Feed::Note(format!(
                    "Everyone is down. The run ends at wave {}",
                    run.wave
                )),
            });
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
    players: &Query<
        (Entity, &Upgrades, &crate::net::Pose, Has<Fallen>, Has<Out>),
        With<PlayerClient>,
    >,
) -> Vec<(usize, f32)> {
    let mut weights: Vec<(usize, f32)> = (0..open).map(|i| (i, 1.0)).collect();
    for (_, upgrades, pose, ..) in players {
        if !Build::from_cards(&upgrades.0).has(Perk::Barricade) {
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

/// What a build does to weapons.
pub fn mods_of(build: &Build) -> WeaponMods {
    let mut m = WeaponMods::default();
    let bit = |w: WeaponKind| 1u16 << w.index();
    for w in LEVELED {
        let i = w.index();
        let level = build.level(w);
        if level >= 1 {
            m.damage[i] = 1.3;
        }
        if level >= 2 {
            if w != WeaponKind::Flintlock {
                m.mag[i] = 1.5;
            }
            m.reload[i] = 0.75;
        }
    }
    if build.level(WeaponKind::Glock) >= 3 {
        m.automatic |= bit(WeaponKind::Glock);
    }
    if build.level(WeaponKind::Mp5) >= 3 {
        m.steady |= bit(WeaponKind::Mp5);
    }
    if build.level(WeaponKind::Ak) >= 3 {
        m.settle |= bit(WeaponKind::Ak);
    }
    if build.level(WeaponKind::Sniper) >= 3 {
        m.pierce[WeaponKind::Sniper.index()] += 3;
    }
    if build.level(WeaponKind::Flintlock) >= 3 {
        // Eight smaller balls instead of one big one.
        let i = WeaponKind::Flintlock.index();
        m.pellets[i] = 8;
        m.damage[i] *= 0.3;
    }
    if build.loaded(crate::survival::cards::Ammo::Piercing) {
        for w in LEVELED {
            m.pierce[w.index()] += 1;
        }
    }
    if build.has(Perk::QuickHands) {
        for r in &mut m.reload {
            *r *= 0.6;
        }
        m.swap = 0.6;
    }
    if build.has(Perk::Deadeye) {
        m.head = 1.5;
    }
    if build.has(Perk::Butcher) {
        m.damage[WeaponKind::Knife.index()] = 1.5;
    }
    if build.has(Perk::FleetFoot) {
        m.speed = 1.15;
    }
    m
}

/// Keep every player's `Kit`, `WeaponMods` and full health in step with
/// their cards. A weapon they've just been given comes with spare rounds.
fn equip_all(
    mut players: Query<
        (
            &Upgrades,
            &mut Kit,
            &mut WeaponMods,
            &mut Vigor,
            &mut Health,
        ),
        Changed<Upgrades>,
    >,
) {
    for (upgrades, mut kit, mut mods, mut vigor, mut health) in &mut players {
        let build = Build::from_cards(&upgrades.0);
        let new_mods = mods_of(&build);
        equip(&mut kit, &build, &new_mods);
        if *mods != new_mods {
            *mods = new_mods;
        }
        let max = MAX_HEALTH
            + if build.has(Perk::Juggernaut) {
                50.0
            } else {
                0.0
            };
        if vigor.0 != max {
            // More health to have: fill the difference.
            health.0 += (max - vigor.0).max(0.0);
            vigor.0 = max;
        }
    }
}

/// Own what the build says, and give each newly owned weapon its rounds.
fn equip(kit: &mut Kit, build: &Build, mods: &WeaponMods) {
    for w in WeaponKind::ALL {
        if build.owns(w) && !kit.owns(w) {
            kit.give(w, rounds_for(w, Some(mods)));
        }
    }
}

/// Spare magazines for every gun a player owns (ammo boxes, breathers, the
/// Supplies card), and grenades if they have them.
pub fn resupply(kit: &mut Kit, mods: &WeaponMods, mags: f32) {
    for w in WeaponKind::ALL {
        if !kit.owns(w) || w == WeaponKind::Knife {
            continue;
        }
        // Two grenades for each magazine's worth (so two a breather).
        let rounds = if w == WeaponKind::Grenade {
            (mags * 2.0).round().max(1.0) as u16
        } else {
            (WeaponMods::mag_of(Some(mods), w) as f32 * mags).round() as u16
        };
        kit.add(w, rounds);
    }
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

/// `give_card <name>`: the host takes a card now (for trying things out, and
/// for test scripts).
pub fn give_card(
    mut cmds: MessageReader<ConsoleCommand>,
    mut players: Query<(
        &Player,
        &mut Upgrades,
        &mut Kit,
        &mut Health,
        &WeaponMods,
        &Vigor,
    )>,
    mut print: MessageWriter<Print>,
) {
    for cmd in cmds.read() {
        if cmd.name != "give_card" {
            continue;
        }
        let name = cmd.args.join(" ");
        let Some(id) = crate::survival::cards::find(&name) else {
            print.write(Print::error(format!("no card called '{name}'")));
            continue;
        };
        // The host's own player.
        let Some((_, mut upgrades, mut kit, mut health, mods, vigor)) =
            players.iter_mut().find(|p| p.0.owner == 0)
        else {
            print.write(Print::error("give_card works in survival, on the host"));
            continue;
        };
        let card = &crate::survival::cards::CARDS[id as usize];
        let refill = matches!(
            card.effect,
            crate::survival::cards::Effect::Supplies | crate::survival::cards::Effect::Patch
        );
        if !refill && !Build::from_cards(&upgrades.0).eligible(card) {
            print.write(Print::error(format!("{} can't be taken yet", card.name)));
            continue;
        }
        draft::take(id, &mut upgrades, &mut kit, &mut health, mods, vigor);
        print.write(Print(format!(
            "took {}",
            crate::survival::cards::CARDS[id as usize].name
        )));
    }
}

/// `zombie` and `next_wave`, for trying things out and for test scripts.
fn test_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut runs: Query<(&Survival, &mut RunClock)>,
    hosts: Query<(&Player, &crate::net::Pose)>,
    zombies: Query<Entity, With<Zombie>>,
    mut spawner: ResMut<Spawner>,
    mut print: MessageWriter<Print>,
    mut commands: Commands,
) {
    for cmd in cmds.read() {
        match cmd.name.as_str() {
            "end_wave" => {
                spawner.queue.clear();
                for entity in &zombies {
                    commands.entity(entity).despawn();
                }
            }
            "next_wave" => match runs.single_mut() {
                Ok((run, mut clock)) if run.phase == crate::survival::protocol::Phase::Breather => {
                    clock.0 = 0.0;
                }
                _ => {
                    print.write(Print::error("next_wave works during a breather"));
                }
            },
            "zombie" => {
                let id = match cmd.args.first().map(String::as_str) {
                    Some("walker") | None => crate::survival::protocol::WALKER,
                    Some("runner") => crate::survival::protocol::RUNNER,
                    Some("brute") => crate::survival::protocol::BRUTE,
                    Some(other) => {
                        print.write(Print::error(format!("no zombie called '{other}'")));
                        continue;
                    }
                };
                let at = match (
                    cmd.args.get(1).and_then(|a| a.parse::<f32>().ok()),
                    cmd.args.get(2).and_then(|a| a.parse::<f32>().ok()),
                ) {
                    (Some(x), Some(z)) => Vec3::new(x, 0.9, z),
                    _ => {
                        // 6 m in front of the host.
                        let Some((_, pose)) = hosts.iter().find(|h| h.0.owner == 0) else {
                            print.write(Print::error("zombie needs x z here"));
                            continue;
                        };
                        let ahead = Quat::from_rotation_y(pose.yaw) * Vec3::NEG_Z;
                        (pose.pos + ahead * 6.0).with_y(0.9)
                    }
                };
                let wave = runs.single().map_or(1, |r| r.0.wave.max(1));
                let yaw = at.x.atan2(at.z);
                // Past the end of its route, so it goes for the nearest player.
                commands.spawn(zombies::zombie(id, wave, 0, usize::MAX, at, yaw));
            }
            _ => {}
        }
    }
}
