//! Console commands for trying survival out and for test scripts: take a
//! card, set a zombie loose, skip a breather, clear a wave. They work on the
//! host or offline, on the host's own player.

use bevy::prelude::*;

use super::{
    RunClock, draft,
    loadout::Vigor,
    zombies::{Spawner, Zombie, spawn_zombie},
};
use crate::{
    console::{ConsoleAppExt, ConsoleCommand, Print},
    net::{Health, Kit, Player, Pose, WeaponMods},
    survival::{
        cards::{Build, CARDS, Effect, find},
        protocol::{
            BLOATER, BRUTE, CRAWLER, LEAPER, Offer, Phase, RUNNER, Survival, Upgrades, WALKER,
        },
    },
};

pub(super) fn plugin(app: &mut App) {
    app.console_command(
        "give_card",
        "<card>",
        "survival: take a card at once (host)",
    )
    .console_command(
        "zombie",
        "<walker|runner|brute|crawler|bloater|leaper> [x z]",
        "survival: a zombie now, going for the nearest player (host)",
    )
    .console_command("next_wave", "", "survival: end the breather now (host)")
    .console_command(
        "offer",
        "<card>, <card>[, <card>]",
        "survival: offer these cards now (host)",
    )
    .console_command(
        "end_wave",
        "",
        "survival: clear the wave's zombies now (host)",
    );
}

/// `give_card <name>`: the host takes a card now (for trying things out, and
/// for test scripts).
pub fn give_card(
    mut cmds: MessageReader<ConsoleCommand>,
    mut players: Query<(
        &Player,
        &Build,
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
        let Some(id) = find(&name) else {
            print.write(Print::error(format!("no card called '{name}'")));
            continue;
        };
        // The host's own player.
        let Some((_, build, mut upgrades, mut kit, mut health, mods, vigor)) =
            players.iter_mut().find(|p| p.0.owner == 0)
        else {
            print.write(Print::error("give_card works in survival, on the host"));
            continue;
        };
        let card = &CARDS[id as usize];
        let refill = matches!(card.effect, Effect::Supplies | Effect::Patch);
        if !refill && !build.eligible(card) {
            print.write(Print::error(format!("{} can't be taken yet", card.name)));
            continue;
        }
        draft::take(id, &mut upgrades, &mut kit, &mut health, mods, vigor);
        print.write(Print(format!("took {}", card.name)));
    }
}

/// `zombie` and `next_wave`, for trying things out and for test scripts.
pub fn test_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut runs: Query<(&Survival, &mut RunClock)>,
    hosts: Query<(&Player, &Pose)>,
    zombies: Query<Entity, With<Zombie>>,
    mut offers: Query<(&Player, &mut Offer)>,
    mut spawner: ResMut<Spawner>,
    mut print: MessageWriter<Print>,
    mut commands: Commands,
) {
    for cmd in cmds.read() {
        match cmd.name.as_str() {
            "offer" => {
                let names = cmd.args.join(" ");
                let ids: Option<Vec<u8>> = names.split(',').map(|n| find(n.trim())).collect();
                match (ids, offers.iter_mut().find(|o| o.0.owner == 0)) {
                    (Some(ids), Some((_, mut offer))) if !ids.is_empty() && ids.len() <= 3 => {
                        offer.0 = ids;
                    }
                    (None, _) => {
                        print.write(Print::error(format!("no such card in '{names}'")));
                    }
                    _ => {
                        print.write(Print::error("offer one to three cards, on the host"));
                    }
                }
            }
            "end_wave" => {
                spawner.queue.clear();
                for entity in &zombies {
                    commands.entity(entity).despawn();
                }
            }
            "next_wave" => match runs.single_mut() {
                Ok((run, mut clock)) if run.phase == Phase::Breather => {
                    clock.0 = 0.0;
                }
                _ => {
                    print.write(Print::error("next_wave works during a breather"));
                }
            },
            "zombie" => {
                let id = match cmd.args.first().map(String::as_str) {
                    Some("walker") | None => WALKER,
                    Some("runner") => RUNNER,
                    Some("brute") => BRUTE,
                    Some("crawler") => CRAWLER,
                    Some("bloater") => BLOATER,
                    Some("leaper") => LEAPER,
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
                spawn_zombie(&mut commands, id, wave, 0, usize::MAX, at, yaw);
            }
            _ => {}
        }
    }
}
