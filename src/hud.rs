//! A bare-bones HUD: an accuracy crosshair with hit marker, sniper scope,
//! health, ammo, weapon slots, kill feed, a status line and a Tab scoreboard.

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_replicon::prelude::*;
use bevy_replicon_renet::{RenetClient, RenetServer};

use crate::{
    game::MAX_HEALTH,
    net::{Feed, Health, LocalId, Player, PlayerName, Score},
    player::GameInput,
    render::WorldCamera,
    weapon::{Arsenal, HitMarker, WeaponKind},
};

const TEXT: Color = Color::srgb(0.93, 0.9, 0.86);
const FEED_SECS: f32 = 6.0;

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(FrameTimeDiagnosticsPlugin::default())
        .init_resource::<FeedLines>()
        .add_systems(Startup, spawn_hud)
        .add_systems(PreUpdate, collect_feed.after(ClientSystems::Receive))
        .add_systems(
            Update,
            (
                crosshair,
                scope,
                vitals,
                slots,
                feed,
                status,
                scoreboard,
                damage_flash,
            ),
        );
}

#[derive(Component)]
struct CrossBar(Vec2);

#[derive(Component)]
struct CrosshairRoot;

#[derive(Component)]
struct ScopeOverlay;

#[derive(Component)]
struct SlotText(WeaponKind);

#[derive(Component)]
struct HealthText;

#[derive(Component)]
struct AmmoText;

#[derive(Component)]
struct FeedText;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct ScoreText;

#[derive(Component)]
struct DamageFlash;

#[derive(Resource, Default)]
struct FeedLines(Vec<(String, f32)>);

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: size.into(),
        ..default()
    }
}

fn spawn_hud(mut commands: Commands) {
    // Crosshair: four bars around the center.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            CrosshairRoot,
        ))
        .with_children(|root| {
            for dir in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                root.spawn((
                    CrossBar(dir),
                    Node {
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    BackgroundColor(TEXT.with_alpha(0.85)),
                ));
            }
        });

    // Sniper scope: a round window cut out of black by a huge outline,
    // with thin crosshair lines across it.
    commands
        .spawn((
            ScopeOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    height: percent(94),
                    aspect_ratio: Some(1.0),
                    border_radius: BorderRadius::MAX,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Outline {
                    width: px(4000),
                    offset: px(0),
                    color: Color::BLACK,
                },
            ))
            .with_children(|lens| {
                let line = Color::srgba(0.0, 0.0, 0.0, 0.9);
                lens.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: px(1),
                        ..default()
                    },
                    BackgroundColor(line),
                ));
                lens.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(1),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(line),
                ));
            });
        });

    // Weapon slots, bottom center.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(26),
                width: percent(100),
                justify_content: JustifyContent::Center,
                column_gap: px(18),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            for (i, kind) in WeaponKind::ALL.into_iter().enumerate() {
                row.spawn((
                    SlotText(kind),
                    Text::new(format!("{} {}", i + 1, kind.def().name)),
                    font(14.0),
                    TextColor(TEXT.with_alpha(0.35)),
                ));
            }
        });

    commands.spawn((
        DamageFlash,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            border: UiRect::all(px(36)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.8, 0.05, 0.02, 0.0)),
    ));

    commands.spawn((
        HealthText,
        Text::new("100"),
        font(34.0),
        TextColor(TEXT),
        Node {
            position_type: PositionType::Absolute,
            left: px(28),
            bottom: px(22),
            ..default()
        },
    ));
    commands.spawn((
        AmmoText,
        Text::new("30 / 30"),
        font(34.0),
        TextColor(TEXT),
        Node {
            position_type: PositionType::Absolute,
            right: px(28),
            bottom: px(22),
            ..default()
        },
    ));
    commands.spawn((
        FeedText,
        Text::new(""),
        font(16.0),
        TextColor(TEXT),
        TextLayout::justify(Justify::Right),
        Node {
            position_type: PositionType::Absolute,
            right: px(20),
            top: px(16),
            ..default()
        },
    ));
    commands.spawn((
        StatusText,
        Text::new(""),
        font(13.0),
        TextColor(TEXT.with_alpha(0.6)),
        Node {
            position_type: PositionType::Absolute,
            left: px(16),
            top: px(12),
            ..default()
        },
    ));
    commands.spawn((
        ScoreText,
        Text::new(""),
        font(18.0),
        TextColor(TEXT),
        BackgroundColor(Color::srgba(0.03, 0.02, 0.05, 0.85)),
        Visibility::Hidden,
        Node {
            position_type: PositionType::Absolute,
            left: percent(30),
            top: percent(22),
            width: percent(40),
            padding: UiRect::all(px(18)),
            ..default()
        },
    ));
}

/// The gap is the inaccuracy cone projected onto the screen: a bullet will
/// land somewhere between the bars (plus the spray pattern while spraying).
fn crosshair(
    time: Res<Time>,
    mut marker: ResMut<HitMarker>,
    arsenal: Res<Arsenal>,
    window: Single<&Window>,
    camera: Single<&Projection, With<WorldCamera>>,
    mut root: Single<&mut Visibility, With<CrosshairRoot>>,
    mut bars: Query<(&CrossBar, &mut Node, &mut BackgroundColor)>,
) {
    marker.time = (marker.time - time.delta_secs()).max(0.0);
    root.set_if_neq(if arsenal.scoped() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    let fov = match *camera {
        Projection::Perspective(p) => p.fov,
        _ => 1.4,
    };
    let half_height = window.height() / 2.0;
    let cone = arsenal.inaccuracy.to_radians().tan() / (fov / 2.0).tan() * half_height;
    let gap = cone.max(3.0);

    let hit = marker.time > 0.0;
    let color = match (hit, marker.kill) {
        (true, true) => Color::srgb(1.0, 0.25, 0.15),
        (true, false) => Color::WHITE,
        _ if arsenal.reloading > 0.0 || arsenal.draw > 0.0 => TEXT.with_alpha(0.35),
        _ => TEXT.with_alpha(0.9),
    };
    for (bar, mut node, mut bg) in &mut bars {
        let (len, thick) = (8.0, 2.0);
        let (w, h) = if bar.0.x != 0.0 {
            (len, thick)
        } else {
            (thick, len)
        };
        node.width = px(w);
        node.height = px(h);
        node.margin = UiRect {
            left: px(bar.0.x * (gap + len / 2.0) * 2.0),
            top: px(-bar.0.y * (gap + len / 2.0) * 2.0),
            ..default()
        };
        bg.0 = color;
    }
}

fn scope(arsenal: Res<Arsenal>, mut overlay: Single<&mut Visibility, With<ScopeOverlay>>) {
    overlay.set_if_neq(if arsenal.scoped() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
}

fn slots(arsenal: Res<Arsenal>, mut texts: Query<(&SlotText, &mut TextColor)>) {
    for (slot, mut color) in &mut texts {
        color.0 = if slot.0 == arsenal.current {
            Color::srgb(1.0, 0.62, 0.28)
        } else {
            TEXT.with_alpha(0.35)
        };
    }
}

fn local_player<'a>(
    players: &'a Query<(&Player, &Health, &PlayerName, &Score)>,
    local: &LocalId,
) -> Option<(&'a Player, &'a Health, &'a PlayerName, &'a Score)> {
    players.iter().find(|(p, ..)| p.owner == local.0)
}

fn vitals(
    arsenal: Res<Arsenal>,
    local: Res<LocalId>,
    players: Query<(&Player, &Health, &PlayerName, &Score)>,
    mut health: Single<(&mut Text, &mut TextColor), (With<HealthText>, Without<AmmoText>)>,
    mut ammo: Single<&mut Text, (With<AmmoText>, Without<HealthText>)>,
) {
    let hp = local_player(&players, &local)
        .map(|(_, h, ..)| h.0)
        .unwrap_or(MAX_HEALTH);
    health.0.0 = format!("{}", hp.max(0.0).round() as i32);
    health.1.0 = if hp < 35.0 {
        Color::srgb(1.0, 0.3, 0.2)
    } else {
        TEXT
    };
    let def = arsenal.def();
    ammo.0 = if arsenal.reloading > 0.0 {
        format!("{}  reloading", def.name)
    } else {
        format!(
            "{}  {} / {}",
            def.name,
            arsenal.ammo[arsenal.current.index()],
            def.mag
        )
    };
}

fn damage_flash(
    time: Res<Time>,
    local: Res<LocalId>,
    players: Query<(&Player, &Health, &PlayerName, &Score)>,
    mut flash: Single<&mut BorderColor, With<DamageFlash>>,
    mut last: Local<Option<f32>>,
    mut alpha: Local<f32>,
) {
    let hp = local_player(&players, &local).map(|(_, h, ..)| h.0);
    if let (Some(hp), Some(prev)) = (hp, *last)
        && hp < prev
    {
        *alpha = (*alpha + 0.35 + (prev - hp) / 100.0).min(0.6);
    }
    *last = hp;
    *alpha = (*alpha - time.delta_secs() * 1.2).max(0.0);
    **flash = BorderColor::all(Color::srgba(0.8, 0.05, 0.02, *alpha));
}

fn collect_feed(mut feed: MessageReader<Feed>, mut lines: ResMut<FeedLines>, time: Res<Time>) {
    for Feed(line) in feed.read() {
        lines.0.push((line.clone(), time.elapsed_secs()));
    }
}

fn feed(
    time: Res<Time>,
    mut lines: ResMut<FeedLines>,
    mut text: Single<&mut Text, With<FeedText>>,
) {
    let now = time.elapsed_secs();
    lines.0.retain(|(_, at)| now - at < FEED_SECS);
    let start = lines.0.len().saturating_sub(5);
    let joined: Vec<&str> = lines.0[start..].iter().map(|(l, _)| l.as_str()).collect();
    let new = joined.join("\n");
    if text.0 != new {
        text.0 = new;
    }
}

fn status(
    diagnostics: Res<DiagnosticsStore>,
    server: Option<Res<RenetServer>>,
    client: Option<Res<RenetClient>>,
    client_state: Res<State<ClientState>>,
    players: Query<(), With<Player>>,
    mut text: Single<&mut Text, With<StatusText>>,
    mut timer: Local<f32>,
    time: Res<Time>,
) {
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 0.25;
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let count = players.iter().count();
    let mode = if server.is_some() {
        format!("hosting, {count} players")
    } else if let Some(client) = client {
        match client_state.get() {
            ClientState::Connected => {
                format!("online, {count} players, {:.0} ms", client.rtt() * 1000.0)
            }
            _ => "connecting...".into(),
        }
    } else {
        "offline".into()
    };
    text.0 = format!("{mode}  |  {fps:.0} fps  |  ~ console");
}

fn scoreboard(
    keys: Res<ButtonInput<KeyCode>>,
    input: Res<GameInput>,
    local: Res<LocalId>,
    players: Query<(&Player, &Health, &PlayerName, &Score)>,
    mut board: Single<(&mut Text, &mut Visibility), With<ScoreText>>,
) {
    let show = input.enabled && keys.pressed(KeyCode::Tab);
    *board.1 = if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if !show {
        return;
    }
    let mut rows: Vec<_> = players.iter().collect();
    rows.sort_by(|a, b| b.3.kills.cmp(&a.3.kills).then(a.3.deaths.cmp(&b.3.deaths)));
    let mut out = format!("{:<18} {:>6} {:>7}\n", "player", "kills", "deaths");
    for (player, _, name, score) in rows {
        let me = if player.owner == local.0 { " <" } else { "" };
        out.push_str(&format!(
            "{:<18} {:>6} {:>7}{me}\n",
            name.0, score.kills, score.deaths
        ));
    }
    board.0.0 = out;
}
