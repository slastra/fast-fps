//! A Quake-style drop-down console, toggled with `~`.
//!
//! Other modules register commands with [`ConsoleAppExt::console_command`] and
//! handle them by reading [`ConsoleCommand`] messages. Anything can print to the
//! console by writing a [`Print`] message.

use std::collections::{BTreeMap, VecDeque};

use bevy::{
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
};

const MAX_LINES: usize = 400;
const VISIBLE_LINES: usize = 24;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Console>()
        .init_resource::<CommandRegistry>()
        .add_message::<ConsoleCommand>()
        .add_message::<Print>()
        .add_message::<SaveSetting>()
        .init_resource::<Settings>()
        .add_systems(Startup, (spawn_console_ui, queue_launch_commands))
        .add_systems(
            PreUpdate,
            (toggle_console, edit_input).chain().after(InputSystems),
        )
        .add_systems(Update, (run_queued, builtin_commands))
        .add_systems(
            PostUpdate,
            (collect_prints, render_console, save_settings).chain(),
        );

    app.console_command("help", "", "list commands")
        .console_command("clear", "", "clear the console")
        .console_command("quit", "", "exit the game")
        .console_command("echo", "<text>", "print text")
        .console_command("exec", "<file>", "run a script of console commands")
        .console_command("wait", "<seconds>", "pause a script");
}

/// A parsed console command, emitted for every line the user submits.
#[derive(Message, Clone, Debug)]
pub struct ConsoleCommand {
    pub name: String,
    pub args: Vec<String>,
}

impl ConsoleCommand {
    pub fn arg(&self, i: usize) -> Option<&str> {
        self.args.get(i).map(String::as_str)
    }
}

/// Print a line to the console (and the log).
#[derive(Message, Clone, Debug)]
pub struct Print(pub String);

impl Print {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

/// Remember a setting: it is written to `config.cfg` as `key value` and
/// replayed on the next launch.
#[derive(Message, Clone, Debug)]
pub struct SaveSetting {
    pub key: &'static str,
    pub value: String,
}

#[derive(Resource, Default)]
struct Settings(BTreeMap<String, String>);

/// `$XDG_CONFIG_HOME/fast-fps/config.cfg`, falling back to `~/.config`.
fn config_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("fast-fps").join("config.cfg"))
}

fn save_settings(
    mut saves: MessageReader<SaveSetting>,
    mut settings: ResMut<Settings>,
    mut print: MessageWriter<Print>,
) {
    let mut changed = false;
    for save in saves.read() {
        settings.0.insert(save.key.to_string(), save.value.clone());
        changed = true;
    }
    if !changed {
        return;
    }
    let Some(path) = config_path() else { return };
    let mut text =
        String::from("// Written by fast-fps; runs at startup before launch arguments.\n");
    for (key, value) in &settings.0 {
        text.push_str(&format!("{key} {value}\n"));
    }
    let result = path
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|()| std::fs::write(&path, text));
    if let Err(e) = result {
        print.write(Print(format!("could not save {}: {e}", path.display())));
    }
}

#[derive(Resource, Default)]
pub struct Console {
    pub open: bool,
    input: String,
    lines: VecDeque<String>,
    history: Vec<String>,
    history_cursor: Option<usize>,
    /// Lines to execute on the next update (typed lines, launch args).
    queued: Vec<String>,
    /// Lines from `exec` scripts, run in order and paused by `wait`.
    script: VecDeque<String>,
    wait: f32,
}

impl Console {
    /// Queue a command line to run as if it was typed.
    pub fn exec(&mut self, line: impl Into<String>) {
        self.queued.push(line.into());
    }
}

#[derive(Resource, Default)]
struct CommandRegistry(BTreeMap<&'static str, (&'static str, &'static str)>);

pub trait ConsoleAppExt {
    fn console_command(
        &mut self,
        name: &'static str,
        usage: &'static str,
        help: &'static str,
    ) -> &mut Self;
}

impl ConsoleAppExt for App {
    fn console_command(
        &mut self,
        name: &'static str,
        usage: &'static str,
        help: &'static str,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<CommandRegistry>()
            .0
            .insert(name, (usage, help));
        self
    }
}

#[derive(Component)]
struct ConsoleRoot;

#[derive(Component)]
struct ConsoleLog;

#[derive(Component)]
struct ConsoleInput;

const ACCENT: Color = Color::srgb(1.0, 0.62, 0.28);

fn spawn_console_ui(mut commands: Commands) {
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

/// Runs `config.cfg`, then the launch arguments:
/// `fast-fps +name alice +host 27015` runs `name alice` then `host 27015`.
fn queue_launch_commands(
    mut console: ResMut<Console>,
    mut settings: ResMut<Settings>,
    mut print: MessageWriter<Print>,
) {
    print.write(Print::new(
        "fast-fps. Press ~ for this console. Try: help, host, find, connect <ip>",
    ));
    if let Some(text) = config_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if let Some((key, value)) = line.split_once(' ') {
                settings.0.insert(key.to_string(), value.trim().to_string());
            }
            console.script.push_back(line.to_string());
        }
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut current: Vec<String> = Vec::new();
    for arg in args {
        if let Some(name) = arg.strip_prefix('+') {
            if !current.is_empty() {
                console.script.push_back(current.join(" "));
            }
            current = vec![name.to_string()];
        } else if !current.is_empty() {
            current.push(arg);
        }
    }
    if !current.is_empty() {
        console.script.push_back(current.join(" "));
    }
}

/// `~` toggles. Matches the character too, so non-US layouts and virtual
/// keyboards that don't report the physical Backquote key still work.
fn toggle_console(mut keyboard: MessageReader<KeyboardInput>, mut console: ResMut<Console>) {
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed || event.repeat {
            continue;
        }
        let is_tilde_char = matches!(&event.logical_key, Key::Character(c) if c == "`" || c == "~");
        if event.key_code == KeyCode::Backquote || is_tilde_char {
            console.open = !console.open;
        }
    }
}

fn edit_input(
    mut keyboard: MessageReader<KeyboardInput>,
    mut console: ResMut<Console>,
    registry: Res<CommandRegistry>,
) {
    if !console.open {
        keyboard.clear();
        return;
    }
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        match &event.logical_key {
            Key::Enter => {
                let line = std::mem::take(&mut console.input);
                if !line.trim().is_empty() {
                    console.history.push(line.clone());
                    console.queued.push(line);
                }
                console.history_cursor = None;
            }
            Key::Backspace => {
                console.input.pop();
            }
            Key::Escape => console.open = false,
            Key::ArrowUp => {
                if console.history.is_empty() {
                    continue;
                }
                let i = match console.history_cursor {
                    Some(i) => i.saturating_sub(1),
                    None => console.history.len() - 1,
                };
                console.history_cursor = Some(i);
                console.input = console.history[i].clone();
            }
            Key::ArrowDown => match console.history_cursor {
                Some(i) if i + 1 < console.history.len() => {
                    console.history_cursor = Some(i + 1);
                    console.input = console.history[i + 1].clone();
                }
                _ => {
                    console.history_cursor = None;
                    console.input.clear();
                }
            },
            Key::Tab => {
                let prefix = console.input.trim().to_string();
                let matches: Vec<&str> = registry
                    .0
                    .keys()
                    .copied()
                    .filter(|name| name.starts_with(&prefix))
                    .collect();
                if let [only] = matches.as_slice() {
                    console.input = format!("{only} ");
                } else if !matches.is_empty() {
                    let line = matches.join("  ");
                    console.lines.push_back(line);
                }
            }
            _ => {
                let Some(text) = &event.text else { continue };
                for ch in text.chars() {
                    if !ch.is_control() && ch != '`' && ch != '~' {
                        console.input.push(ch);
                    }
                }
            }
        }
    }
}

/// Parse queued lines (`;` separates commands) and emit them as messages.
/// Script lines run until a `wait` pauses them.
fn run_queued(
    time: Res<Time<Real>>,
    mut console: ResMut<Console>,
    registry: Res<CommandRegistry>,
    mut commands: MessageWriter<ConsoleCommand>,
    mut print: MessageWriter<Print>,
) {
    let mut lines: Vec<String> = std::mem::take(&mut console.queued);
    console.wait -= time.delta_secs();
    while console.wait <= 0.0 {
        let Some(line) = console.script.pop_front() else {
            break;
        };
        let mut words = line.split_whitespace();
        match words.next() {
            Some("wait") => {
                console.wait = words.next().and_then(|w| w.parse().ok()).unwrap_or(1.0);
            }
            Some("screenshot") => {
                // Let the capture finish before the script changes anything else.
                lines.push(line);
                console.wait = 0.2;
            }
            _ => lines.push(line),
        }
    }
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        print.write(Print(format!("] {line}")));
        for part in line.split(';') {
            let mut words = part.split_whitespace().map(str::to_string);
            let Some(name) = words.next() else { continue };
            let name = name.to_lowercase();
            let args: Vec<String> = words.collect();
            match name.as_str() {
                "exec" => match args.first().map(std::fs::read_to_string) {
                    Some(Ok(text)) => {
                        // Run the file before whatever was already scripted.
                        for (i, l) in text.lines().enumerate() {
                            console.script.insert(i, l.to_string());
                        }
                    }
                    Some(Err(e)) => {
                        print.write(Print(format!("exec: {e}")));
                    }
                    None => {
                        print.write(Print::new("usage: exec <file>"));
                    }
                },
                "wait" => {
                    print.write(Print::new("wait only works inside an exec script"));
                }
                _ if registry.0.contains_key(name.as_str()) => {
                    commands.write(ConsoleCommand { name, args });
                }
                _ => {
                    print.write(Print(format!("unknown command '{name}', try 'help'")));
                }
            }
        }
    }
}

fn builtin_commands(
    mut commands: MessageReader<ConsoleCommand>,
    mut console: ResMut<Console>,
    registry: Res<CommandRegistry>,
    mut print: MessageWriter<Print>,
    mut exit: MessageWriter<AppExit>,
) {
    for cmd in commands.read() {
        match cmd.name.as_str() {
            "help" => {
                for (name, (usage, help)) in &registry.0 {
                    let left = format!("{name} {usage}");
                    print.write(Print(format!("  {left:<26} {help}")));
                }
            }
            "clear" => console.lines.clear(),
            "echo" => {
                print.write(Print(cmd.args.join(" ")));
            }
            "quit" => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

fn collect_prints(mut prints: MessageReader<Print>, mut console: ResMut<Console>) {
    for Print(text) in prints.read() {
        info!("{text}");
        for line in text.lines() {
            console.lines.push_back(line.to_string());
        }
    }
    while console.lines.len() > MAX_LINES {
        console.lines.pop_front();
    }
}

fn render_console(
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
