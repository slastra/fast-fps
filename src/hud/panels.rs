//! The readouts around the edge of the screen: health, weapon and ammo,
//! weapon slots, the status line, the scoreboard and the damage flash.

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_replicon::prelude::*;
use bevy_replicon_renet::{RenetClient, RenetServer};

use crate::{
    config::MAX_HEALTH,
    console::{ConsoleCommand, Print},
    hud::{
        ACCENT, AmmoFill, AmmoText, DANGER, DIM, DamageFlash, HealthFill, HealthText, HudFont,
        MagText, ScoreBoard, ScoreRows, SlotIcon, SlotKey, SlotMark, StatusText, TEXT, WeaponIcon,
        WeaponName, text,
    },
    net::{Health, LocalId, Player, PlayerName, Score},
    player::GameInput,
    weapon::{Arsenal, WeaponIcons, WeaponKind},
};

/// Seconds the slot strip stays up after you switch weapons.
const SLOTS_SHOWN: f32 = 2.6;

fn my_health(players: &Query<(&Player, &Health)>, local: &LocalId) -> Option<f32> {
    players
        .iter()
        .find(|(p, _)| p.owner == local.0)
        .map(|(_, h)| h.0)
}

pub fn vitals(
    local: Res<LocalId>,
    players: Query<(&Player, &Health)>,
    mut number: Single<(&mut Text, &mut TextColor), With<HealthText>>,
    mut bar: Single<(&mut Node, &mut BackgroundColor), With<HealthFill>>,
) {
    let hp = my_health(&players, &local).unwrap_or(MAX_HEALTH).max(0.0);
    let shown = format!("{}", hp.round() as i32);
    if number.0.0 != shown {
        number.0.0 = shown;
    }
    let color = if hp < 35.0 { DANGER } else { TEXT };
    number.1.0 = color;
    bar.0.width = percent(hp / MAX_HEALTH * 100.0);
    bar.1.0 = color;
}

pub fn weapon(
    arsenal: Res<Arsenal>,
    icons: Res<WeaponIcons>,
    mut ammo: Single<(&mut Text, &mut TextColor), (With<AmmoText>, Without<MagText>)>,
    mut mag: Single<&mut Text, (With<MagText>, Without<AmmoText>, Without<WeaponName>)>,
    mut name: Single<&mut Text, (With<WeaponName>, Without<AmmoText>, Without<MagText>)>,
    mut icon: Single<&mut ImageNode, With<WeaponIcon>>,
    mut bar: Single<(&mut Node, &mut BackgroundColor), With<AmmoFill>>,
) {
    let def = arsenal.def();
    let rounds = arsenal.ammo[arsenal.current.index()];
    let reloading = arsenal.reloading > 0.0;
    let low = rounds as f32 <= def.mag as f32 * 0.25;

    let shown = rounds.to_string();
    if ammo.0.0 != shown {
        ammo.0.0 = shown;
    }
    ammo.1.0 = if reloading {
        TEXT.with_alpha(DIM)
    } else if rounds == 0 {
        DANGER
    } else if low {
        ACCENT
    } else {
        TEXT
    };
    let of = format!("/ {}", def.mag);
    if mag.0 != of {
        mag.0 = of;
    }
    let label = if reloading {
        format!("{}  RELOADING", def.name.to_uppercase())
    } else {
        def.name.to_uppercase()
    };
    if name.0 != label {
        name.0 = label;
    }
    let image = icons.get(arsenal.current);
    if icon.image != image {
        icon.image = image;
    }
    // The bar is the magazine; while reloading it fills back up.
    let fraction = if reloading {
        1.0 - arsenal.reloading / def.reload_secs
    } else {
        rounds as f32 / def.mag as f32
    };
    bar.0.width = percent(fraction.clamp(0.0, 1.0) * 100.0);
    bar.1.0 = if reloading || low { ACCENT } else { TEXT };
}

/// The slot strip appears when you switch weapons and fades out again.
pub fn slots(
    time: Res<Time>,
    arsenal: Res<Arsenal>,
    mut icons: Query<(&SlotIcon, &mut ImageNode)>,
    mut keys: Query<(&SlotKey, &mut TextColor)>,
    mut marks: Query<(&SlotMark, &mut BackgroundColor)>,
    mut shown: Local<f32>,
    mut last: Local<Option<WeaponKind>>,
) {
    if *last != Some(arsenal.current) {
        // Not on the very first frame: nobody switched anything yet.
        if last.is_some() {
            *shown = SLOTS_SHOWN;
        }
        *last = Some(arsenal.current);
    }
    *shown = (*shown - time.delta_secs()).max(0.0);
    let strip = (*shown / 0.5).clamp(0.0, 1.0);
    let alpha = |kind: WeaponKind| strip * if kind == arsenal.current { 1.0 } else { 0.38 };
    for (slot, mut icon) in &mut icons {
        icon.color = TEXT.with_alpha(alpha(slot.0));
    }
    for (slot, mut color) in &mut keys {
        color.0 = TEXT.with_alpha(alpha(slot.0) * 0.8);
    }
    for (slot, mut color) in &mut marks {
        let on = if slot.0 == arsenal.current {
            strip
        } else {
            0.0
        };
        color.0 = ACCENT.with_alpha(on);
    }
}

pub fn damage_flash(
    time: Res<Time>,
    local: Res<LocalId>,
    players: Query<(&Player, &Health)>,
    mut flash: Single<&mut BorderColor, With<DamageFlash>>,
    mut last: Local<Option<f32>>,
    mut alpha: Local<f32>,
) {
    let hp = my_health(&players, &local);
    if let (Some(hp), Some(prev)) = (hp, *last)
        && hp < prev
    {
        *alpha = (*alpha + 0.35 + (prev - hp) / 100.0).min(0.6);
    }
    *last = hp;
    *alpha = (*alpha - time.delta_secs() * 1.2).max(0.0);
    **flash = BorderColor::all(Color::srgba(0.8, 0.05, 0.02, *alpha));
}

pub fn status(
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
        format!("HOSTING  ·  {count} PLAYERS")
    } else if let Some(client) = client {
        match client_state.get() {
            ClientState::Connected => {
                format!(
                    "ONLINE  ·  {count} PLAYERS  ·  {:.0} MS",
                    client.rtt() * 1000.0
                )
            }
            _ => "CONNECTING".into(),
        }
    } else {
        "OFFLINE".into()
    };
    text.0 = format!("{mode}  ·  {fps:.0} FPS  ·  ~ CONSOLE");
}

/// Set by the `scoreboard` command to keep the board up without holding Tab.
#[derive(Resource, Default)]
pub struct ForceScores(bool);

pub fn scoreboard_command(
    mut cmds: MessageReader<ConsoleCommand>,
    mut force: ResMut<ForceScores>,
    mut print: MessageWriter<Print>,
) {
    for cmd in cmds.read() {
        if cmd.name != "scoreboard" {
            continue;
        }
        match cmd.arg(0) {
            Some("1") => force.0 = true,
            Some("0") => force.0 = false,
            _ => {
                print.write(Print::error("usage: scoreboard <0|1>"));
            }
        }
    }
}

pub fn scoreboard(
    keys: Res<ButtonInput<KeyCode>>,
    input: Res<GameInput>,
    force: Res<ForceScores>,
    local: Res<LocalId>,
    font: Res<HudFont>,
    players: Query<(&Player, &PlayerName, &Score)>,
    mut board: Single<&mut Visibility, With<ScoreBoard>>,
    rows: Single<Entity, With<ScoreRows>>,
    mut commands: Commands,
    mut drawn: Local<Vec<(u64, String, u32, u32)>>,
) {
    let show = force.0 || (input.enabled && keys.pressed(KeyCode::Tab));
    board.set_if_neq(if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !show {
        return;
    }
    let mut table: Vec<(u64, String, u32, u32)> = players
        .iter()
        .map(|(p, name, score)| (p.owner, name.0.clone(), score.kills, score.deaths))
        .collect();
    table.sort_by(|a, b| b.2.cmp(&a.2).then(a.3.cmp(&b.3)).then(a.1.cmp(&b.1)));
    if *drawn == table {
        return;
    }
    *drawn = table.clone();

    // Rebuild the rows only when something changed.
    let font = &*font;
    commands.entity(*rows).despawn_related::<Children>();
    commands.entity(*rows).with_children(|list| {
        let row = |list: &mut ChildSpawnerCommands,
                   cells: [(&str, Color); 3],
                   size: f32,
                   weight: u16,
                   highlight: bool| {
            list.spawn((
                Node {
                    padding: UiRect::axes(px(10), px(5)),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(if highlight {
                    ACCENT.with_alpha(0.16)
                } else {
                    Color::NONE
                }),
            ))
            .with_children(|row| {
                for (i, (cell, color)) in cells.into_iter().enumerate() {
                    row.spawn(Node {
                        width: if i == 0 { percent(56) } else { percent(22) },
                        justify_content: if i == 0 {
                            JustifyContent::FlexStart
                        } else {
                            JustifyContent::FlexEnd
                        },
                        ..default()
                    })
                    .with_child(text(font, cell, size, weight, color));
                }
            });
        };
        let dim = TEXT.with_alpha(DIM);
        row(
            list,
            [("PLAYER", dim), ("KILLS", dim), ("DEATHS", dim)],
            9.0,
            600,
            false,
        );
        for (owner, name, kills, deaths) in &table {
            let me = *owner == local.0;
            let color = if me { ACCENT } else { TEXT };
            row(
                list,
                [
                    (name.as_str(), color),
                    (&kills.to_string(), TEXT),
                    (&deaths.to_string(), TEXT.with_alpha(0.75)),
                ],
                13.0,
                if me { 700 } else { 500 },
                me,
            );
        }
    });
}
