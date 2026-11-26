//! Survival as players see it: co-op waves of zombies through cracks in the
//! walls, with a draft of upgrades between waves. The rules run on the
//! server (`server/survival/`); everything here reads what it replicates.
//! See docs/survival.md.
//!
//! - [`cards`]: the card table and what a hand of cards adds up to.
//! - [`cracks`]: where the cracks are and the routes from them.
//! - [`protocol`]: survival's wire types.
//! - [`hud`]: the wave line, the cards, being down, reviving.
//! - [`scene`]: cracks, pickups, what's happening to zombies, flashes.

pub mod cards;
pub mod cracks;
mod hud;
mod looks;
pub mod protocol;
pub mod scene;
mod sounds;

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    config::SURVIVAL_REVIVE_RANGE,
    console::{ConsoleAppExt, ConsoleCommand, Print},
    debug::probe::Probe,
    net::{Dead, LocalId, Mob, Out, Player, PlayerName, Pose},
    player::{GameInput, MoveLimits},
    weapon::{Arsenal, MenuKeys},
};
use cards::{Build, CARDS, Perk};
use protocol::{Fallen, Offer, Phase, PickCard, Revive, Survival, Upgrades};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Me>()
        .add_systems(
            PreUpdate,
            (sum_up, limits, picks, revive)
                .chain()
                .after(ClientSystems::Receive)
                .after(bevy::input::InputSystems),
        )
        .add_systems(Update, probe);
    hud::plugin(app);
    looks::plugin(app);
    scene::plugin(app);
    sounds::plugin(app);
    cracks::add_breaches(app);
    app.console_command(
        "pick",
        "<1|2|3>",
        "survival: take one of the cards on offer",
    )
    .console_command(
        "revive",
        "<on|off>",
        "survival: hold (on) or let go of (off) the revive key, E",
    );
}

/// Where this player stands in the run, if one is on.
#[derive(Resource, Default, Clone, PartialEq)]
pub struct Me {
    pub run: Option<Survival>,
    /// Down, and how long until bleeding out.
    pub fallen: Option<Fallen>,
    /// Bled out: watching until the next breather.
    pub out: bool,
    pub offer: Vec<u8>,
    pub cards: Vec<u8>,
    pub build: Build,
    /// A downed teammate close enough to revive: their name, and how far
    /// the revive has got.
    pub helping: Option<(String, u8)>,
    /// Holding the revive key.
    pub holding: bool,
    /// Players (bots too) down, and bled out.
    pub team_down: u8,
    pub team_out: u8,
}

#[allow(clippy::type_complexity)]
fn sum_up(
    runs: Query<&Survival>,
    local: Res<LocalId>,
    players: Query<(
        &Player,
        &PlayerName,
        &Pose,
        Option<&Fallen>,
        Has<Out>,
        Option<&Offer>,
        Option<&Upgrades>,
    )>,
    mut me: ResMut<Me>,
) {
    let holding = me.holding;
    let mut now = Me {
        run: runs.single().ok().cloned(),
        holding,
        ..default()
    };
    if now.run.is_some() {
        now.team_down = players.iter().filter(|p| p.3.is_some()).count() as u8;
        now.team_out = players.iter().filter(|p| p.4).count() as u8;
        let mine = players.iter().find(|p| p.0.owner == local.0);
        if let Some((_, _, pose, fallen, out, offer, upgrades)) = mine {
            now.fallen = fallen.copied();
            now.out = out;
            now.offer = offer.map(|o| o.0.clone()).unwrap_or_default();
            now.cards = upgrades.map(|u| u.0.clone()).unwrap_or_default();
            now.build = Build::from_cards(&now.cards);
            if fallen.is_none() && !out {
                now.helping = players
                    .iter()
                    .filter(|p| p.0.owner != local.0)
                    .filter_map(|p| p.3.map(|f| (p, f)))
                    .find(|(p, _)| p.2.pos.distance(pose.pos) < SURVIVAL_REVIVE_RANGE + 0.3)
                    .map(|(p, f)| (p.1.0.clone(), f.revive));
            }
        }
    }
    if now.run.is_none() {
        now.holding = false;
    }
    me.set_if_neq(now);
}

/// While down: kneel, don't move, and (without Last Stand) don't shoot.
/// While cards are on offer, 1-3 pick a card instead of a weapon.
fn limits(
    me: Res<Me>,
    mut limits: ResMut<MoveLimits>,
    mut arsenal: ResMut<Arsenal>,
    mut menu: ResMut<MenuKeys>,
) {
    let choosing =
        me.run.as_ref().is_some_and(|r| r.phase == Phase::Breather) && !me.offer.is_empty();
    if menu.0 != choosing {
        menu.0 = choosing;
    }
    if me.fallen.is_some() {
        limits.speed = 0.0;
        limits.jump = false;
        limits.low = true;
        limits.flags |= crate::net::Pose::DOWN;
        if !me.build.has(Perk::LastStand) {
            arsenal.locked = true;
        }
    }
    if me.helping.is_some() && me.holding {
        limits.flags |= crate::net::Pose::REACH;
        limits.speed *= 0.2;
    }
}

/// Keys 1-3 (or `pick <n>`) take a card.
fn picks(
    input: Res<GameInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cmds: MessageReader<ConsoleCommand>,
    me: Res<Me>,
    mut send: MessageWriter<PickCard>,
    mut print: MessageWriter<Print>,
) {
    let mut chosen = None;
    for cmd in cmds.read().filter(|c| c.name == "pick") {
        match cmd.args.first().and_then(|a| a.parse::<u8>().ok()) {
            Some(n @ 1..=3) if (n as usize) <= me.offer.len() => chosen = Some(n - 1),
            _ if me.offer.is_empty() => {
                print.write(Print::error("no cards on offer"));
            }
            _ => {
                print.write(Print::error("pick 1, 2 or 3"));
            }
        }
    }
    if input.enabled && !me.offer.is_empty() {
        for (i, key) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
            .into_iter()
            .enumerate()
        {
            if keys.just_pressed(key) && i < me.offer.len() {
                chosen = Some(i as u8);
            }
        }
    }
    if let Some(n) = chosen {
        let id = me.offer[n as usize];
        print.write(Print(format!("took {}", CARDS[id as usize].name)));
        send.write(PickCard(n));
    }
}

/// Holding E beside a downed teammate (or `revive on|off`).
fn revive(
    input: Res<GameInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cmds: MessageReader<ConsoleCommand>,
    mut me: ResMut<Me>,
    mut send: MessageWriter<Revive>,
    mut print: MessageWriter<Print>,
) {
    if me.run.is_none() {
        return;
    }
    let mut want = me.holding;
    if input.enabled {
        if keys.just_pressed(KeyCode::KeyE) {
            want = true;
        }
        if keys.just_released(KeyCode::KeyE) {
            want = false;
        }
    }
    for cmd in cmds.read().filter(|c| c.name == "revive") {
        match cmd.args.first().map(String::as_str) {
            Some("on") => want = true,
            Some("off") => want = false,
            _ => {
                print.write(Print::error("revive on, or revive off"));
            }
        }
    }
    if want != me.holding {
        me.holding = want;
        send.write(Revive(want));
    }
}

fn probe(
    me: Res<Me>,
    arsenal: Res<Arsenal>,
    local: Res<LocalId>,
    players: Query<(&Player, &Pose)>,
    mobs: Query<(&Pose, &Mob), Without<Dead>>,
    flashes: Res<scene::Flashes>,
    mut probe: ResMut<Probe>,
) {
    let with = |flag: u8| mobs.iter().filter(|m| m.1.flags & flag != 0).count();
    probe.set("burning", with(protocol::BURNING));
    probe.set("chilled", with(protocol::CHILLED));
    probe.set("frozen", with(protocol::FROZEN));
    probe.set("arcs", flashes.arcs);
    probe.set("bursts", flashes.bursts);
    probe.set("fires", flashes.fires);
    probe.set("heals", flashes.heals);
    probe.set("gases", flashes.gases);
    probe.set(
        "crawling",
        mobs.iter().filter(|m| m.0.has(Pose::CRAWL)).count(),
    );
    probe.set(
        "leaping",
        mobs.iter().filter(|m| m.0.has(Pose::AIR)).count(),
    );
    probe.set(
        "tells",
        mobs.iter().filter(|m| m.0.has(Pose::CROUCH)).count(),
    );
    // How far the nearest zombie is (99: none).
    let at = players
        .iter()
        .find(|p| p.0.owner == local.0)
        .map(|p| p.1.pos);
    let near = at.and_then(|at| {
        mobs.iter()
            .map(|m| m.0.pos.distance(at))
            .min_by(f32::total_cmp)
    });
    probe.set("zombie_near", format!("{:.1}", near.unwrap_or(99.0)));
    // Spare rounds for the weapon in hand (-1: unlimited).
    probe.set(
        "spare",
        arsenal.spare(arsenal.current).map_or(-1, |n| n as i64),
    );
    probe.set("owned", arsenal.owned.count_ones());
    let Some(run) = &me.run else {
        probe.set("survival", "none");
        probe.set("wave", 0);
        probe.set("zombies", 0);
        probe.set("cracks", 0);
        probe.set("fallen", 0);
        probe.set("offer", 0);
        probe.set("cards", 0);
        probe.set("team_down", 0);
        probe.set("team_out", 0);
        return;
    };
    probe.set(
        "survival",
        match run.phase {
            Phase::Breather => "breather",
            Phase::Wave => "wave",
            Phase::Over => "over",
        },
    );
    probe.set("wave", run.wave);
    probe.set("zombies", run.left);
    probe.set("cracks", run.cracks.count_ones());
    probe.set(
        "fallen",
        match (me.fallen, me.out) {
            (Some(_), _) => 1,
            (_, true) => 2,
            _ => 0,
        },
    );
    probe.set("bleed", me.fallen.map_or(0, |f| f.bleed));
    probe.set("offer", me.offer.len());
    probe.set("cards", me.cards.len());
    probe.set(
        "card_names",
        me.cards
            .iter()
            .map(|&id| CARDS[id as usize].name.to_lowercase().replace(' ', "_"))
            .collect::<Vec<_>>()
            .join(","),
    );
    probe.set("helping", u8::from(me.helping.is_some()));
    probe.set("team_down", me.team_down);
    probe.set("team_out", me.team_out);
}
