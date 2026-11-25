//! The duel as this machine sees it: the two marks on the ground, the round
//! and score banners, the drum and the bell, and hiding whoever is out.
//!
//! The rules live on the server (`server/duel.rs`). Everything here reads the
//! replicated [`Duel`] entity, so it works the same when hosting, offline and
//! as a client. [`MyDuel`] sums up where this player stands, for the weapon
//! code (no firing before "Draw!", flintlock only).

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    config::{DUEL_MARK_RADIUS, DUEL_MARKS},
    debug::probe::Probe,
    hud::{ACCENT, HudFont, TEXT, text},
    net::{Duel, DuelPhase, LocalId, Out, Player, PlayerName, SelectWeapon},
    render::no_shadow,
    sound::{Sounds, play},
    weapon::{Arsenal, WeaponKind},
};

/// How long "DRAW!" stays up after the bell.
const DRAW_BANNER_SECS: f32 = 1.4;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<MyDuel>()
        .add_systems(Startup, (make_marks, spawn_banners))
        .add_systems(
            PreUpdate,
            (my_duel, weapon_rules)
                .chain()
                .after(ClientSystems::Receive),
        )
        .add_systems(Update, (hide_out, marks, banners, cues, probe));
}

/// This machine's place in the duel, if one is on.
#[derive(Resource, Default, Clone)]
pub struct MyDuel {
    pub duel: Option<Duel>,
    /// 0 or 1 if dueling this round.
    pub slot: Option<usize>,
    /// Dead for the rest of the round, or watching.
    pub out: bool,
}

impl MyDuel {
    /// Whether a shot fired now would count.
    pub fn may_fire(&self) -> bool {
        match &self.duel {
            None => true,
            Some(duel) => duel.phase == DuelPhase::Draw && self.slot.is_some() && !self.out,
        }
    }
}

fn my_duel(
    duels: Query<&Duel>,
    players: Query<(&Player, Has<Out>)>,
    local: Res<LocalId>,
    mut mine: ResMut<MyDuel>,
) {
    let duel = duels.single().ok().cloned();
    let slot = duel.as_ref().and_then(|d| d.slot(local.0));
    let out = duel.is_some()
        && players
            .iter()
            .any(|(player, out)| player.owner == local.0 && out);
    if mine.duel != duel || mine.slot != slot || mine.out != out {
        *mine = MyDuel { duel, slot, out };
    }
}

/// In a duel: the flintlock only, and the trigger works after "Draw!" for
/// duelists still standing. (The server checks all of this too.)
fn weapon_rules(
    mine: Res<MyDuel>,
    mut arsenal: ResMut<Arsenal>,
    mut select: MessageWriter<SelectWeapon>,
) {
    let on = mine.duel.is_some();
    let locked = !mine.may_fire();
    if arsenal.flintlock_only != on || arsenal.locked != locked || arsenal.out != mine.out {
        arsenal.flintlock_only = on;
        arsenal.locked = locked;
        arsenal.out = mine.out;
    }
    if on && arsenal.select(WeaponKind::Flintlock) {
        select.write(SelectWeapon(WeaponKind::Flintlock));
    }
}

/// Players who are out aren't drawn. (Their bodies are under the player
/// entity, so hiding it hides them.)
fn hide_out(mut players: Query<(&mut Visibility, Has<Out>), With<Player>>) {
    for (mut visibility, out) in &mut players {
        visibility.set_if_neq(if out {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

/// One of the two rings on the ground; which duelist it belongs to.
#[derive(Component)]
struct Mark(usize);

#[derive(Resource)]
struct MarkLooks {
    empty: Handle<StandardMaterial>,
    taken: Handle<StandardMaterial>,
}

fn make_marks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ring = meshes.add(Annulus::new(DUEL_MARK_RADIUS - 0.14, DUEL_MARK_RADIUS));
    let looks = MarkLooks {
        empty: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.32, 0.14),
            unlit: true,
            ..default()
        }),
        taken: materials.add(StandardMaterial {
            base_color: ACCENT,
            emissive: LinearRgba::rgb(2.0, 1.1, 0.4),
            unlit: true,
            ..default()
        }),
    };
    for (slot, at) in DUEL_MARKS.into_iter().enumerate() {
        commands.spawn((
            Name::new("Duel mark"),
            Mark(slot),
            Mesh3d(ring.clone()),
            MeshMaterial3d(looks.empty.clone()),
            // The ring is drawn in the XY plane; lay it on the ground.
            Transform::from_translation(at + Vec3::Y * 0.03)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            Visibility::Hidden,
            no_shadow(),
        ));
    }
    commands.insert_resource(looks);
}

/// Show the marks during a duel and light the ones that are taken.
fn marks(
    mine: Res<MyDuel>,
    looks: Res<MarkLooks>,
    mut rings: Query<(
        &Mark,
        &mut Visibility,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    if !mine.is_changed() {
        return;
    }
    for (mark, mut visibility, mut material) in &mut rings {
        let Some(duel) = &mine.duel else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let show = matches!(duel.phase, DuelPhase::Walk | DuelPhase::Steady);
        visibility.set_if_neq(if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let look = if duel.on_mark[mark.0] {
            &looks.taken
        } else {
            &looks.empty
        };
        if material.0 != *look {
            material.0 = look.clone();
        }
    }
}

/// The score along the top, and what's happening in the middle.
#[derive(Component)]
struct ScoreLine;
#[derive(Component)]
struct CallOut;

fn spawn_banners(mut commands: Commands, font: Res<HudFont>) {
    let font = &*font;
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                top: px(22),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_child((ScoreLine, text(font, "", 17.0, 600, TEXT)));
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                top: percent(27),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_child((CallOut, text(font, "", 30.0, 700, TEXT)));
}

/// `owner`'s name as this player should read it.
fn name_of(owner: u64, local: u64, names: &Query<(&Player, &PlayerName)>) -> String {
    if owner == local {
        return "You".into();
    }
    names
        .iter()
        .find(|(p, _)| p.owner == owner)
        .map_or_else(|| "?".into(), |(_, n)| n.0.clone())
}

fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[allow(clippy::type_complexity)]
fn banners(
    time: Res<Time>,
    mine: Res<MyDuel>,
    local: Res<LocalId>,
    names: Query<(&Player, &PlayerName)>,
    mut score: Single<&mut Text, (With<ScoreLine>, Without<CallOut>)>,
    mut call: Single<(&mut Text, &mut TextFont, &mut TextColor), With<CallOut>>,
    font: Res<HudFont>,
    mut draw_since: Local<Option<f32>>,
) {
    let now = time.elapsed_secs();
    let (call_text, call_font, call_color) = &mut *call;
    let Some(duel) = &mine.duel else {
        score.0.clear();
        call_text.0.clear();
        *draw_since = None;
        return;
    };
    let name = |owner| name_of(owner, local.0, &names);
    let [a, b] = duel.duelists;

    let top = if duel.phase == DuelPhase::Waiting {
        "Duel · waiting for an opponent".to_string()
    } else {
        format!(
            "Round {} · {} {} – {} {}",
            duel.round,
            name(a),
            duel.wins[0],
            duel.wins[1],
            name(b)
        )
    };
    if score.0 != top {
        score.0 = top;
    }

    if duel.phase == DuelPhase::Draw {
        draw_since.get_or_insert(now);
    } else {
        *draw_since = None;
    }
    let watching = mine.slot.is_none();
    let mut big = false;
    let line = match duel.phase {
        DuelPhase::Waiting => "Waiting for a second player".to_string(),
        _ if watching => match duel.queue.iter().position(|&o| o == local.0) {
            Some(2) => format!("Watching {} and {} · you're next", name(a), name(b)),
            Some(n) => format!(
                "Watching {} and {} · {} in line",
                name(a),
                name(b),
                ordinal(n - 1)
            ),
            None => format!("Watching {} and {}", name(a), name(b)),
        },
        DuelPhase::Walk => {
            let me = mine.slot.unwrap_or(0);
            if duel.on_mark[me] {
                format!("Wait for {}", name(duel.duelists[1 - me]))
            } else {
                "Walk to your mark".into()
            }
        }
        DuelPhase::Steady => "Steady…".into(),
        DuelPhase::Draw => {
            if draw_since.is_some_and(|t| now - t < DRAW_BANNER_SECS) {
                big = true;
                "DRAW!".into()
            } else {
                String::new()
            }
        }
        DuelPhase::RoundOver { winner: None } => "No winner · the round is replayed".into(),
        DuelPhase::RoundOver {
            winner: Some(winner),
        } => {
            let who = name(duel.duelists[winner as usize]);
            if who == "You" {
                "You win the round".into()
            } else {
                format!("{who} wins the round")
            }
        }
        DuelPhase::Over { winner } => {
            let w = winner as usize;
            let who = name(duel.duelists[w]);
            let tally = format!("{}–{}", duel.wins[w], duel.wins[1 - w]);
            if who == "You" {
                format!("You win the duel {tally}")
            } else {
                format!("{who} wins the duel {tally}")
            }
        }
    };
    // A dead duelist sees the result, not the call to walk or draw.
    let line = if mine.out && matches!(duel.phase, DuelPhase::Draw) && !watching {
        "You're down".to_string()
    } else {
        line
    };
    if call_text.0 != line {
        call_text.0 = line;
        **call_font = font.at(if big { 84.0 } else { 30.0 }, if big { 800 } else { 700 });
        call_color.0 = if big { ACCENT } else { TEXT };
    }
}

/// A drum beat as each duelist reaches their mark, and the bell for "Draw!".
fn cues(
    mine: Res<MyDuel>,
    sounds: Res<Sounds>,
    mut commands: Commands,
    mut last: Local<Option<Duel>>,
) {
    if !mine.is_changed() {
        return;
    }
    if let (Some(before), Some(now)) = (&*last, &mine.duel) {
        let same_round = before.round == now.round;
        for slot in 0..2 {
            if same_round && now.on_mark[slot] && !before.on_mark[slot] {
                play(&mut commands, &sounds.duel_mark, 0.6);
            }
        }
        if now.phase == DuelPhase::Draw && before.phase != DuelPhase::Draw {
            play(&mut commands, &sounds.duel_draw, 0.9);
        }
    }
    last.clone_from(&mine.duel);
}

/// Values for test scripts: `duel`, `duel_round`, `duel_wins`, `duel_me`,
/// `duel_slot`, `out` and `on_mark`.
fn probe(mine: Res<MyDuel>, local: Res<LocalId>, mut probe: ResMut<Probe>) {
    let Some(duel) = &mine.duel else {
        probe.set("duel", "none");
        probe.set("duel_round", 0);
        probe.set("duel_wins", "0-0");
        probe.set("duel_me", "none");
        probe.set("duel_slot", -1);
        probe.set("out", 0);
        probe.set("on_mark", 0);
        return;
    };
    probe.set("duel", duel.phase.name());
    probe.set("duel_round", duel.round);
    probe.set("duel_wins", format!("{}-{}", duel.wins[0], duel.wins[1]));
    let me = match mine.slot {
        Some(_) => "duelist",
        None if duel.queue.contains(&local.0) => "spectator",
        None => "none",
    };
    probe.set("duel_me", me);
    probe.set("duel_slot", mine.slot.map_or(-1, |s| s as i32));
    probe.set("out", u8::from(mine.out));
    probe.set(
        "on_mark",
        u8::from(mine.slot.is_some_and(|s| duel.on_mark[s])),
    );
}
