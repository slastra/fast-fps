//! A bare-bones HUD: an accuracy crosshair with hit marker, sniper scope,
//! health, ammo, weapon slots, kill feed, a status line and a Tab scoreboard.
//!
//! - this file: the layout (every HUD element is spawned here).
//! - [`crosshair`]: the crosshair and scope overlay.
//! - [`panels`]: the text readouts.

mod crosshair;
mod panels;

use bevy::{diagnostic::FrameTimeDiagnosticsPlugin, prelude::*};
use bevy_replicon::prelude::*;

use crate::{
    hud::{
        crosshair::{crosshair, scope},
        panels::{collect_feed, damage_flash, feed, scoreboard, slots, status, vitals},
    },
    weapon::WeaponKind,
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
