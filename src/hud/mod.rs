//! The heads-up display. Minimal by design: large numerals, thin bars, weapon
//! icons, and nothing that isn't needed right now.
//!
//! - this file: the theme and the layout. Every HUD element is spawned in
//!   [`spawn_hud`] with a marker component; the other files find it by that
//!   marker and keep it up to date.
//! - [`crosshair`]: the accuracy crosshair, reload bar and scope overlay.
//! - [`panels`]: health, weapon, slots, status, scoreboard, damage flash.
//! - [`feed`]: the kill feed.
//!
//! Sizes are in pixels at 720p and scale with the window height.

mod crosshair;
mod feed;
mod panels;

use bevy::{
    asset::LoadState, diagnostic::FrameTimeDiagnosticsPlugin, prelude::*, text::FontWeight,
};
use bevy_replicon::prelude::*;

use crate::{
    console::{ConsoleAppExt, Print, ScriptHold},
    weapon::{ICON_ASPECT, WeaponIcons, WeaponKind},
};

// --- Theme -------------------------------------------------------------------

pub const TEXT: Color = Color::srgb(0.96, 0.95, 0.93);
pub const ACCENT: Color = Color::srgb(1.0, 0.62, 0.28);
pub const DANGER: Color = Color::srgb(1.0, 0.33, 0.25);
/// Secondary text: labels, inactive slots.
pub const DIM: f32 = 0.5;
pub const PANEL: Color = Color::srgba(0.03, 0.03, 0.05, 0.6);
const TRACK: Color = Color::srgba(1.0, 1.0, 1.0, 0.14);
/// The window height the layout is designed for.
const DESIGN_HEIGHT: f32 = 720.0;

/// The HUD typeface: Martian Grotesk, a variable font, so any weight works.
#[derive(Resource)]
pub struct HudFont(pub Handle<Font>);

impl HudFont {
    pub fn at(&self, size: f32, weight: u16) -> TextFont {
        TextFont {
            font: self.0.clone().into(),
            font_size: size.into(),
            weight: FontWeight(weight),
            ..default()
        }
    }
}

/// A line of HUD text with a soft shadow so it reads against any background.
pub fn text(font: &HudFont, content: &str, size: f32, weight: u16, color: Color) -> impl Bundle {
    (
        Text::new(content),
        font.at(size, weight),
        TextColor(color),
        TextShadow {
            offset: Vec2::new(0.0, 1.5),
            color: Color::srgba(0.0, 0.0, 0.0, 0.55),
        },
    )
}

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(FrameTimeDiagnosticsPlugin::default())
        .init_resource::<panels::ForceScores>()
        .add_systems(PreStartup, load_font)
        .add_systems(Startup, spawn_hud)
        .add_systems(PreUpdate, feed::collect.after(ClientSystems::Receive))
        .add_systems(
            Update,
            (
                font_loaded,
                scale_with_window,
                crosshair::crosshair,
                crosshair::reload_bar,
                crosshair::scope,
                panels::vitals,
                panels::weapon,
                panels::slots,
                panels::status,
                panels::scoreboard,
                panels::scoreboard_command,
                panels::damage_flash,
                feed::expire,
            ),
        );

    app.console_command(
        "scoreboard",
        "<0|1>",
        "keep the scoreboard up (Tab holds it)",
    );
}

fn load_font(mut commands: Commands, assets: Res<AssetServer>, mut hold: ResMut<ScriptHold>) {
    // Scripts wait for the font, so screenshots never show the fallback.
    hold.0 += 1;
    commands.insert_resource(HudFont(assets.load("fonts/MartianGrotesk-VF.ttf")));
}

fn font_loaded(
    font: Res<HudFont>,
    assets: Res<AssetServer>,
    mut hold: ResMut<ScriptHold>,
    mut print: MessageWriter<Print>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let failed = matches!(assets.load_state(font.0.id()), LoadState::Failed(_));
    if failed {
        print.write(Print::error(
            "could not load assets/fonts/MartianGrotesk-VF.ttf",
        ));
    }
    if failed || assets.is_loaded_with_dependencies(font.0.id()) {
        *done = true;
        hold.0 = hold.0.saturating_sub(1);
    }
}

fn scale_with_window(window: Single<&Window>, mut scale: ResMut<UiScale>) {
    let wanted = (window.height() / DESIGN_HEIGHT).clamp(0.6, 3.0);
    if (scale.0 - wanted).abs() > 0.01 {
        scale.0 = wanted;
    }
}

// --- Markers -----------------------------------------------------------------

#[derive(Component)]
pub struct CrosshairRoot;
/// One of the four crosshair bars, with the direction it sits in.
#[derive(Component)]
pub struct CrossBar(pub Vec2);
#[derive(Component)]
pub struct ReloadTrack;
#[derive(Component)]
pub struct ReloadFill;
#[derive(Component)]
pub struct ScopeOverlay;

#[derive(Component)]
pub struct HealthText;
#[derive(Component)]
pub struct HealthFill;
#[derive(Component)]
pub struct AmmoText;
#[derive(Component)]
pub struct MagText;
#[derive(Component)]
pub struct AmmoFill;
#[derive(Component)]
pub struct WeaponName;
#[derive(Component)]
pub struct WeaponIcon;

/// Parts of a weapon slot in the strip.
#[derive(Component)]
pub struct SlotIcon(pub WeaponKind);
#[derive(Component)]
pub struct SlotKey(pub WeaponKind);
#[derive(Component)]
pub struct SlotMark(pub WeaponKind);

#[derive(Component)]
pub struct FeedList;
#[derive(Component)]
pub struct StatusText;
#[derive(Component)]
pub struct ScoreBoard;
#[derive(Component)]
pub struct ScoreRows;
#[derive(Component)]
pub struct DamageFlash;

/// A full-screen layer that centers its children and ignores the mouse.
fn layer() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Pickable::IGNORE,
    )
}

/// A thin rounded bar; spawn a fill inside it.
fn track(width: f32) -> impl Bundle {
    (
        Node {
            width: px(width),
            height: px(4),
            border_radius: BorderRadius::all(px(2)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(TRACK),
    )
}

fn fill() -> impl Bundle {
    (
        Node {
            width: percent(100),
            height: percent(100),
            border_radius: BorderRadius::all(px(2)),
            ..default()
        },
        BackgroundColor(TEXT),
    )
}

fn spawn_hud(mut commands: Commands, font: Res<HudFont>, icons: Res<WeaponIcons>) {
    let font = &*font;

    // --- Crosshair, with the reload bar under it --------------------------
    commands
        .spawn((layer(), CrosshairRoot))
        .with_children(|root| {
            for dir in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                root.spawn((
                    CrossBar(dir),
                    Node {
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    BackgroundColor(TEXT),
                ));
            }
            root.spawn((
                ReloadTrack,
                Node {
                    position_type: PositionType::Absolute,
                    margin: UiRect::top(px(92)),
                    width: px(64),
                    height: px(3),
                    border_radius: BorderRadius::all(px(2)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(TRACK),
                Visibility::Hidden,
            ))
            .with_child((ReloadFill, fill()));
        });

    // --- Sniper scope: a round window cut out of black --------------------
    commands
        .spawn((ScopeOverlay, layer(), Visibility::Hidden))
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
                for (w, h) in [(percent(100), px(1)), (px(1), percent(100))] {
                    lens.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: w,
                            height: h,
                            ..default()
                        },
                        BackgroundColor(line),
                    ));
                }
            });
        });

    // --- Damage flash: a red edge that fades -------------------------------
    commands.spawn((
        DamageFlash,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            border: UiRect::all(px(36)),
            ..default()
        },
        BorderColor::all(Color::NONE),
        Pickable::IGNORE,
    ));

    // --- Health, bottom left ------------------------------------------------
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(34),
                bottom: px(28),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|col| {
            col.spawn((HealthText, text(font, "100", 46.0, 700, TEXT)));
            col.spawn(track(190.0)).with_child((HealthFill, fill()));
            col.spawn(text(font, "HEALTH", 10.0, 600, TEXT.with_alpha(DIM)));
        });

    // --- Weapon, bottom right -------------------------------------------------
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(34),
                bottom: px(28),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: px(5),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|col| {
            col.spawn((
                WeaponIcon,
                ImageNode::new(icons.get(WeaponKind::Ak)).with_color(TEXT.with_alpha(0.92)),
                Node {
                    width: px(190),
                    height: px(190.0 / ICON_ASPECT),
                    ..default()
                },
            ));
            col.spawn(Node {
                align_items: AlignItems::Baseline,
                column_gap: px(8),
                ..default()
            })
            .with_children(|row| {
                row.spawn((AmmoText, text(font, "30", 46.0, 700, TEXT)));
                row.spawn((MagText, text(font, "/ 30", 17.0, 500, TEXT.with_alpha(DIM))));
            });
            col.spawn(track(190.0)).with_child((AmmoFill, fill()));
            col.spawn((
                WeaponName,
                text(font, "AK", 10.0, 600, TEXT.with_alpha(DIM)),
            ));
        });

    // --- Weapon slots, bottom center; shown when you switch -------------------
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(30),
                width: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexEnd,
                column_gap: px(22),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            for (i, kind) in WeaponKind::ALL.into_iter().enumerate() {
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(4),
                    ..default()
                })
                .with_children(|slot| {
                    slot.spawn((
                        SlotIcon(kind),
                        ImageNode::new(icons.get(kind)).with_color(Color::NONE),
                        Node {
                            width: px(74),
                            height: px(74.0 / ICON_ASPECT),
                            ..default()
                        },
                    ));
                    // No shadow here: a shadow would stay visible after
                    // the number itself has faded out.
                    slot.spawn((
                        SlotKey(kind),
                        Text::new((i + 1).to_string()),
                        font.at(9.0, 600),
                        TextColor(Color::NONE),
                    ));
                    slot.spawn((
                        SlotMark(kind),
                        Node {
                            width: px(18),
                            height: px(2),
                            border_radius: BorderRadius::all(px(1)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ));
                });
            }
        });

    // --- Kill feed, top right ---------------------------------------------
    commands.spawn((
        FeedList,
        Node {
            position_type: PositionType::Absolute,
            right: px(24),
            top: px(22),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: px(6),
            ..default()
        },
        Pickable::IGNORE,
    ));

    // --- Status, top left ---------------------------------------------------
    commands.spawn((
        StatusText,
        text(font, "", 10.0, 500, TEXT.with_alpha(0.45)),
        Node {
            position_type: PositionType::Absolute,
            left: px(24),
            top: px(20),
            ..default()
        },
    ));

    // --- Scoreboard, center, while Tab is held ------------------------------
    commands
        .spawn((ScoreBoard, layer(), Visibility::Hidden))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(440),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(px(22), px(18)),
                    row_gap: px(8),
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(PANEL.with_alpha(0.86)),
            ))
            .with_children(|panel| {
                panel.spawn(text(font, "SCOREBOARD", 10.0, 600, TEXT.with_alpha(DIM)));
                panel.spawn((
                    ScoreRows,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        ..default()
                    },
                ));
            });
        });
}
