//! The kill feed: who got whom, with what.

use bevy::prelude::*;

use crate::{
    hud::{ACCENT, FeedList, HudFont, PANEL, TEXT, text},
    net::{Feed, LocalName},
    weapon::{ICON_ASPECT, WeaponIcons},
};

const ROW_SECS: f32 = 6.0;
const MAX_ROWS: usize = 6;

/// A row in the feed, with how long it has left.
#[derive(Component)]
pub struct FeedRow(f32);

/// Turn each feed message into a row.
pub fn collect(
    mut feed: MessageReader<Feed>,
    mut commands: Commands,
    list: Single<(Entity, Option<&Children>), With<FeedList>>,
    font: Res<HudFont>,
    icons: Res<WeaponIcons>,
    me: Res<LocalName>,
) {
    let (list, rows) = *list;
    let mut count = rows.map_or(0, |r| r.len());
    for message in feed.read() {
        let font = &*font;
        let mine = matches!(message, Feed::Kill { killer, victim, .. }
            if *killer == me.0 || *victim == me.0);
        let row = commands
            .spawn((
                FeedRow(ROW_SECS),
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(9),
                    padding: UiRect::axes(px(11), px(6)),
                    border: UiRect::left(px(2)),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(if mine { ACCENT } else { Color::NONE }),
                ChildOf(list),
            ))
            .id();
        commands.entity(row).with_children(|row| match message {
            Feed::Note(note) => {
                row.spawn(text(font, note, 11.0, 500, TEXT.with_alpha(0.7)));
            }
            Feed::Kill {
                killer,
                victim,
                weapon,
                headshot,
            } => {
                let name = |who: &String| if *who == me.0 { ACCENT } else { TEXT };
                row.spawn(text(font, killer, 12.0, 600, name(killer)));
                row.spawn((
                    ImageNode::new(icons.get(*weapon)).with_color(TEXT.with_alpha(0.9)),
                    Node {
                        width: px(46),
                        height: px(46.0 / ICON_ASPECT),
                        ..default()
                    },
                ));
                if *headshot {
                    row.spawn(text(font, "HEAD", 8.0, 700, ACCENT));
                }
                row.spawn(text(font, victim, 12.0, 600, name(victim)));
            }
        });
        count += 1;
    }
    // Too many: drop the oldest.
    if let Some(rows) = rows {
        let extra = count.saturating_sub(MAX_ROWS);
        for old in rows.iter().take(extra) {
            commands.entity(old).try_despawn();
        }
    }
}

pub fn expire(time: Res<Time>, mut commands: Commands, mut rows: Query<(Entity, &mut FeedRow)>) {
    for (entity, mut row) in &mut rows {
        row.0 -= time.delta_secs();
        if row.0 <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}
