//! Survival's HUD: the wave line along the top, call-outs in the middle, the
//! three cards to choose from at a breather, being down, and the revive
//! prompt.

use bevy::prelude::*;

use super::{
    Me,
    cards::{CARDS, Rarity},
    protocol::Phase,
};
use crate::hud::{ACCENT, DANGER, DIM, HudFont, PANEL, TEXT, text};

/// How long "Wave 4" stays up when a wave begins.
const CALL_SECS: f32 = 2.2;
const RARE: Color = Color::srgb(0.45, 0.7, 1.0);

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn)
        .add_systems(Update, (top_line, call_out, cards, downed, helping));
}

#[derive(Component)]
struct WaveLine;
#[derive(Component)]
struct BuildLine;
#[derive(Component)]
struct CallOut;
#[derive(Component)]
struct CardRow;
#[derive(Component)]
struct CardPanel(usize);
#[derive(Component)]
enum CardText {
    Name(usize),
    Rarity(usize),
    Body(usize),
}
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

    // The draft: three cards side by side.
    commands
        .spawn((
            CardRow,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                top: percent(38),
                justify_content: JustifyContent::Center,
                column_gap: px(14),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            for i in 0..3 {
                row.spawn((
                    CardPanel(i),
                    Node {
                        width: px(190),
                        min_height: px(118),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        padding: UiRect::all(px(12)),
                        border: UiRect::top(px(3)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor::all(TEXT),
                ))
                .with_children(|card| {
                    card.spawn(text(font, &format!("{}", i + 1), 11.0, 700, ACCENT));
                    card.spawn((CardText::Name(i), text(font, "", 16.0, 700, TEXT)));
                    card.spawn((CardText::Rarity(i), text(font, "", 10.0, 600, TEXT)));
                    card.spawn((
                        CardText::Body(i),
                        text(font, "", 12.0, 500, TEXT.with_alpha(0.85)),
                    ));
                });
            }
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
fn cards(
    me: Res<Me>,
    mut row: Single<&mut Visibility, With<CardRow>>,
    mut panels: Query<(&CardPanel, &mut BorderColor, &mut Node)>,
    mut texts: Query<(&CardText, &mut Text, &mut TextColor)>,
) {
    if !me.is_changed() {
        return;
    }
    let show = me.run.as_ref().is_some_and(|r| r.phase == Phase::Breather) && !me.offer.is_empty();
    row.set_if_neq(if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !show {
        return;
    }
    let color = |r: Rarity| match r {
        Rarity::Common => TEXT.with_alpha(DIM + 0.2),
        Rarity::Rare => RARE,
        Rarity::Legendary => ACCENT,
    };
    for (panel, mut border, mut node) in &mut panels {
        let card = me.offer.get(panel.0).map(|&id| &CARDS[id as usize]);
        node.display = if card.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(card) = card {
            *border = BorderColor::all(color(card.rarity));
        }
    }
    for (which, mut text, mut tint) in &mut texts {
        let (i, what) = match which {
            CardText::Name(i) => (*i, 0),
            CardText::Rarity(i) => (*i, 1),
            CardText::Body(i) => (*i, 2),
        };
        let Some(card) = me.offer.get(i).map(|&id| &CARDS[id as usize]) else {
            continue;
        };
        match what {
            0 => set(&mut text, card.name.into()),
            1 => {
                set(&mut text, format!("{:?}", card.rarity).to_uppercase());
                tint.set_if_neq(TextColor(color(card.rarity)));
            }
            _ => set(&mut text, card.text.into()),
        }
    }
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
