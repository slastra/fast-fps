//! Survival's HUD: the wave line along the top, call-outs in the middle, the
//! three cards to choose from at a breather, being down, and the revive
//! prompt.

use bevy::prelude::*;

use super::{Me, protocol::Phase};
use crate::hud::{DANGER, HudFont, TEXT, text};

/// How long "Wave 4" stays up when a wave begins.
const CALL_SECS: f32 = 2.2;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn)
        .add_systems(Update, (top_line, call_out, downed, helping));
}

#[derive(Component)]
struct WaveLine;
#[derive(Component)]
struct BuildLine;
#[derive(Component)]
struct CallOut;
#[derive(Component)]
struct DownVeil;
#[derive(Component)]
struct DownText;
#[derive(Component)]
struct HelpText;

fn centered(top: Val) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4),
            ..default()
        },
        Pickable::IGNORE,
    )
}

fn spawn(mut commands: Commands, font: Res<HudFont>) {
    let font = &*font;
    commands.spawn(centered(px(22))).with_children(|top| {
        top.spawn((WaveLine, text(font, "", 17.0, 600, TEXT)));
        top.spawn((BuildLine, text(font, "", 11.0, 500, TEXT.with_alpha(0.7))));
    });
    commands
        .spawn(centered(percent(27)))
        .with_child((CallOut, text(font, "", 30.0, 700, TEXT)));

    // Being down: the screen goes red.
    commands.spawn((
        DownVeil,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::srgba(0.45, 0.02, 0.0, 0.22)),
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
    commands.spawn(centered(percent(58))).with_children(|c| {
        c.spawn((DownText, text(font, "", 18.0, 600, TEXT)));
        c.spawn((HelpText, text(font, "", 15.0, 600, TEXT)));
    });
}

fn set(text: &mut Text, to: String) {
    if text.0 != to {
        text.0 = to;
    }
}

#[allow(clippy::type_complexity)]
fn top_line(
    me: Res<Me>,
    mut wave: Single<&mut Text, (With<WaveLine>, Without<BuildLine>)>,
    mut build: Single<&mut Text, (With<BuildLine>, Without<WaveLine>)>,
) {
    if !me.is_changed() {
        return;
    }
    let Some(run) = &me.run else {
        set(&mut wave, String::new());
        set(&mut build, String::new());
        return;
    };
    let zombies = |n: u16| {
        if n == 1 {
            "1 zombie".into()
        } else {
            format!("{n} zombies")
        }
    };
    let line = match run.phase {
        Phase::Breather if run.wave == 0 => format!("Survival · the first wave in {}s", run.secs),
        Phase::Breather => format!("Wave {} cleared · the next in {}s", run.wave, run.secs),
        Phase::Wave => format!("Wave {} · {}", run.wave, zombies(run.left)),
        Phase::Over => format!(
            "The run ended at wave {} · again in {}s",
            run.wave, run.secs
        ),
    };
    set(&mut wave, line);
    let b = &me.build;
    let mut parts: Vec<String> = Vec::new();
    if !b.ammo.is_empty() {
        let names: Vec<String> = b.ammo.iter().map(|a| format!("{a:?}")).collect();
        parts.push(names.join(" + "));
    }
    if let Some(combo) = b.combo() {
        parts.push(combo.name().into());
    }
    if !me.cards.is_empty() {
        parts.push(format!("{} cards", me.cards.len()));
    }
    set(&mut build, parts.join(" · "));
}

/// "Wave 4" as each wave begins; "Pick a card" while there's one to pick;
/// the end of a run.
fn call_out(
    time: Res<Time>,
    me: Res<Me>,
    mut call: Single<(&mut Text, &mut TextColor), With<CallOut>>,
    mut seen: Local<(u16, Option<Phase>, f32)>,
) {
    let (call_text, color) = &mut *call;
    let now = time.elapsed_secs();
    let Some(run) = &me.run else {
        set(call_text, String::new());
        *seen = (0, None, 0.0);
        return;
    };
    if seen.1 != Some(run.phase) || seen.0 != run.wave {
        *seen = (run.wave, Some(run.phase), now);
    }
    let fresh = now - seen.2 < CALL_SECS;
    let (line, tint) = match run.phase {
        Phase::Wave if fresh => (format!("Wave {}", run.wave), DANGER),
        Phase::Breather if !me.offer.is_empty() => ("Pick a card · 1, 2 or 3".into(), TEXT),
        Phase::Over => ("Everyone is down".into(), DANGER),
        _ => (String::new(), TEXT),
    };
    set(call_text, line);
    color.set_if_neq(TextColor(tint));
}

#[allow(clippy::type_complexity)]
fn downed(
    me: Res<Me>,
    mut veil: Single<&mut Visibility, With<DownVeil>>,
    mut line: Single<&mut Text, (With<DownText>, Without<HelpText>)>,
) {
    if !me.is_changed() {
        return;
    }
    let over = me.run.as_ref().is_some_and(|r| r.phase == Phase::Over);
    let (show, to) = match (me.run.is_some(), me.fallen, me.out) {
        (true, Some(_), _) if over => (true, String::new()),
        (true, Some(f), _) if f.revive > 0 => (true, format!("Being revived · {}%", f.revive)),
        (true, Some(f), _) => (true, format!("You're down · bleeding out in {}s", f.bleed)),
        (true, None, true) => (false, "You bled out · back at the next breather".into()),
        _ => (false, String::new()),
    };
    veil.set_if_neq(if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    set(&mut line, to);
}

fn helping(me: Res<Me>, mut line: Single<&mut Text, With<HelpText>>) {
    if !me.is_changed() {
        return;
    }
    let to = match &me.helping {
        Some((name, revive)) if me.holding => format!("Reviving {name} · {revive}%"),
        Some((name, _)) => format!("Hold E to revive {name}"),
        None => String::new(),
    };
    set(&mut line, to);
}
