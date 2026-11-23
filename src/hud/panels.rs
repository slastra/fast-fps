//! Text readouts: health, ammo, weapon slots, kill feed, status and scores.

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_replicon::prelude::*;
use bevy_replicon_renet::{RenetClient, RenetServer};

use crate::{
    config::MAX_HEALTH,
    hud::{
        AmmoText, DamageFlash, FEED_SECS, FeedLines, FeedText, HealthText, ScoreText, SlotText,
        StatusText, TEXT,
    },
    net::{Feed, Health, LocalId, Player, PlayerName, Score},
    player::GameInput,
    weapon::Arsenal,
};

pub fn slots(arsenal: Res<Arsenal>, mut texts: Query<(&SlotText, &mut TextColor)>) {
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

pub fn vitals(
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

pub fn damage_flash(
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

pub fn collect_feed(mut feed: MessageReader<Feed>, mut lines: ResMut<FeedLines>, time: Res<Time>) {
    for Feed(line) in feed.read() {
        lines.0.push((line.clone(), time.elapsed_secs()));
    }
}

pub fn feed(
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

pub fn scoreboard(
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
