//! The console panel: scrollback and an input line.

use bevy::prelude::*;

use crate::console::{Console, VISIBLE_LINES};

#[derive(Component)]
pub struct ConsoleRoot;

#[derive(Component)]
pub struct ConsoleLog;

#[derive(Component)]
pub struct ConsoleInput;

const ACCENT: Color = Color::srgb(1.0, 0.62, 0.28);

pub fn spawn_console_ui(mut commands: Commands) {
    let font = TextFont {
        font_size: 15.0.into(),
        ..default()
    };
    commands.spawn((
        Name::new("Console"),
        ConsoleRoot,
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            width: percent(100),
            height: percent(45),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            padding: UiRect::axes(px(14), px(10)),
            border: UiRect::bottom(px(2)),
            overflow: Overflow::clip(),
            ..default()
        },
        BorderColor::all(ACCENT),
        BackgroundColor(Color::srgba(0.035, 0.02, 0.06, 0.9)),
        GlobalZIndex(100),
        Visibility::Hidden,
        children![
            (
                ConsoleLog,
                Text::new(""),
                font.clone(),
                TextColor(Color::srgb(0.82, 0.8, 0.86)),
            ),
            (
                ConsoleInput,
                Text::new("> "),
                font,
                TextColor(ACCENT),
                Node {
                    margin: UiRect::top(px(6)),
                    ..default()
                },
            ),
        ],
    ));
}

pub fn render_console(
    console: Res<Console>,
    time: Res<Time<Real>>,
    mut root: Single<&mut Visibility, With<ConsoleRoot>>,
    mut log: Single<&mut Text, (With<ConsoleLog>, Without<ConsoleInput>)>,
    mut input: Single<&mut Text, (With<ConsoleInput>, Without<ConsoleLog>)>,
) {
    **root = if console.open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if !console.open {
        return;
    }
    if console.is_changed() {
        let start = console.lines.len().saturating_sub(VISIBLE_LINES);
        let visible: Vec<&str> = console.lines.range(start..).map(String::as_str).collect();
        log.0 = visible.join("\n");
    }
    let caret = if time.elapsed_secs().fract() < 0.5 {
        "_"
    } else {
        " "
    };
    input.0 = format!("> {}{caret}", console.input);
}
