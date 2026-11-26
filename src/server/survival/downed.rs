//! Going down instead of dying. A player who takes a lethal hit drops to
//! their knees (`Fallen`): zombies leave them be and they can't move or
//! shoot (unless they have Last Stand). A teammate holding E beside them
//! brings them back; after `SURVIVAL_BLEED_SECS` they bleed out and watch
//! (`Out`) until the next breather. Second Wind gets them up by themselves,
//! once a run. The run ends when everyone is down at once (`waves`).

use bevy::{platform::collections::HashSet, prelude::*};
use bevy_replicon::prelude::*;

use super::{Vigor, draft::Breather, keep_spot};
use crate::{
    config::{
        SURVIVAL_BLEED_SECS, SURVIVAL_REVIVE_HEALTH, SURVIVAL_REVIVE_RANGE, SURVIVAL_REVIVE_SECS,
    },
    net::{Feed, Health, Out, Player, PlayerName, Pose, Respawn},
    server::{
        bots::Bot,
        players::{PlayerClient, find_player, place},
        rules::{Died, Disarmed, Untouchable},
    },
    survival::{
        cards::{Build, Perk},
        protocol::{Fallen, Flash, Revive, Upgrades},
    },
};

/// How long Second Wind takes to get you up, and with how much health.
const SECOND_WIND_SECS: f32 = 3.0;
const SECOND_WIND_HEALTH: f32 = 50.0;
/// Medic: health a second to teammates this close.
const MEDIC_RATE: f32 = 3.0;
const MEDIC_RANGE: f32 = 8.0;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Holding>();
}

/// Server-only: a downed player's clock and how far a revive has got.
#[derive(Component, Default)]
pub struct Bleed {
    left: f32,
    revive: f32,
}

/// Server-only: Second Wind will get this player up in this many seconds.
#[derive(Component)]
pub struct RiseIn(f32);

/// Server-only: this player has used their Second Wind this run.
#[derive(Component)]
pub struct WindUsed;

/// Server-only: players holding the revive key.
#[derive(Resource, Default)]
pub struct Holding(HashSet<Entity>);

/// A player took a lethal hit: down they go.
#[allow(clippy::type_complexity)]
pub fn fall(
    mut deaths: MessageReader<Died>,
    players: Query<(&Upgrades, Has<Fallen>, Has<Out>, Has<WindUsed>), With<Player>>,
    mut healths: Query<&mut Health>,
    mut commands: Commands,
) {
    for died in deaths.read() {
        let Ok((upgrades, fallen, out, wind_used)) = players.get(died.victim) else {
            continue;
        };
        if fallen || out {
            continue;
        }
        let build = Build::from_cards(&upgrades.0);
        if let Ok(mut health) = healths.get_mut(died.victim) {
            health.0 = 1.0;
        }
        let mut down = commands.entity(died.victim);
        down.insert((
            Fallen {
                bleed: SURVIVAL_BLEED_SECS as u8,
                revive: 0,
            },
            Bleed {
                left: SURVIVAL_BLEED_SECS,
                revive: 0.0,
            },
            Untouchable,
        ));
        if !build.has(Perk::LastStand) {
            down.insert(Disarmed);
        }
        if build.has(Perk::SecondWind) && !wind_used {
            down.insert((RiseIn(SECOND_WIND_SECS), WindUsed));
        }
    }
}

/// Back on their feet with `health`.
fn stand_up(commands: &mut Commands, entity: Entity, health: &mut Health, to: f32) {
    health.0 = to;
    commands
        .entity(entity)
        .remove::<(Fallen, Bleed, RiseIn, Untouchable, Disarmed)>();
}

/// Holding E (`Revive`) beside a fallen teammate brings them back.
#[allow(clippy::type_complexity)]
pub fn revive(
    time: Res<Time>,
    mut messages: MessageReader<FromClient<Revive>>,
    clients: Query<(Entity, &PlayerClient)>,
    mut holding: ResMut<Holding>,
    rescuers: Query<(Entity, &Pose, &Upgrades), (With<Player>, Without<Fallen>, Without<Out>)>,
    mut fallen: Query<(Entity, &Pose, &mut Bleed, &mut Health, Option<&mut RiseIn>)>,
    mut commands: Commands,
    mut flash: MessageWriter<ToClients<Flash>>,
) {
    for FromClient { client_id, message } in messages.read() {
        if let Some(entity) = find_player(*client_id, &clients) {
            if message.0 {
                holding.0.insert(entity);
            } else {
                holding.0.remove(&entity);
            }
        }
    }
    let dt = time.delta_secs();
    for (entity, pose, mut bleed, mut health, rise) in &mut fallen {
        if let Some(mut rise) = rise {
            rise.0 -= dt;
            if rise.0 <= 0.0 {
                stand_up(&mut commands, entity, &mut health, SECOND_WIND_HEALTH);
                flash.write(ToClients {
                    targets: SendTargets::All,
                    message: Flash::Heal { at: pose.pos },
                });
                continue;
            }
        }
        let helper = rescuers.iter().find(|(r, at, _)| {
            holding.0.contains(r) && at.pos.distance(pose.pos) < SURVIVAL_REVIVE_RANGE
        });
        match helper {
            Some((_, _, upgrades)) => {
                let rally = Build::from_cards(&upgrades.0).has(Perk::Rally);
                bleed.revive += dt * if rally { 2.0 } else { 1.0 };
                if bleed.revive >= SURVIVAL_REVIVE_SECS {
                    let hp = if rally { 70.0 } else { SURVIVAL_REVIVE_HEALTH };
                    stand_up(&mut commands, entity, &mut health, hp);
                    flash.write(ToClients {
                        targets: SendTargets::All,
                        message: Flash::Heal { at: pose.pos },
                    });
                }
            }
            None => bleed.revive = (bleed.revive - dt).max(0.0),
        }
    }
}

/// Bleed out when nobody comes, and keep the replicated `Fallen` current.
pub fn bleed(
    time: Res<Time>,
    mut fallen: Query<(Entity, &mut Bleed, &mut Fallen, &PlayerName)>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, mut bleed, mut shown, name) in &mut fallen {
        // The clock stops while someone is helping.
        if bleed.revive <= 0.0 {
            bleed.left -= dt;
        }
        if bleed.left <= 0.0 {
            commands
                .entity(entity)
                .remove::<(Fallen, Bleed, RiseIn, Disarmed)>()
                .insert(Out);
            feed.write(ToClients {
                targets: SendTargets::All,
                message: Feed::Note(format!("{} bled out", name.0)),
            });
            continue;
        }
        let now = Fallen {
            bleed: bleed.left.ceil() as u8,
            revive: (bleed.revive / SURVIVAL_REVIVE_SECS * 100.0).min(100.0) as u8,
        };
        if *shown != now {
            *shown = now;
        }
    }
}

/// At each breather everyone who went down is back, at full health, in
/// the keep.
#[allow(clippy::type_complexity)]
pub fn return_fallen(
    mut breathers: MessageReader<Breather>,
    mut players: Query<
        (
            Entity,
            &mut Health,
            &Vigor,
            Has<Fallen>,
            Has<Out>,
            Option<&PlayerClient>,
            Option<&mut Bot>,
        ),
        With<Player>,
    >,
    mut respawns: MessageWriter<ToClients<Respawn>>,
    mut commands: Commands,
) {
    if breathers.read().count() == 0 {
        return;
    }
    for (i, (entity, mut health, vigor, fallen, out, client, bot)) in players.iter_mut().enumerate()
    {
        if !fallen && !out {
            continue;
        }
        stand_up(&mut commands, entity, &mut health, vigor.0);
        commands.entity(entity).remove::<Out>();
        place(client, bot, keep_spot(i), &mut respawns);
    }
}

/// Medic: teammates near a medic slowly heal.
pub fn medics(
    time: Res<Time>,
    mut players: Query<
        (Entity, &Pose, &Upgrades, &Vigor, &mut Health),
        (With<Player>, Without<Fallen>, Without<Out>),
    >,
) {
    let medics: Vec<(Entity, Vec3)> = players
        .iter()
        .filter(|p| Build::from_cards(&p.2.0).has(Perk::Medic))
        .map(|p| (p.0, p.1.pos))
        .collect();
    if medics.is_empty() {
        return;
    }
    let heal = MEDIC_RATE * time.delta_secs();
    for (entity, pose, _, vigor, mut health) in &mut players {
        let near = medics
            .iter()
            .any(|(m, at)| *m != entity && at.distance(pose.pos) < MEDIC_RANGE);
        if near && health.0 < vigor.0 {
            health.0 = (health.0 + heal).min(vigor.0);
        }
    }
}
