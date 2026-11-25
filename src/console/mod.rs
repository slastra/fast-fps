//! A Quake-style drop-down console, toggled with `~`.
//!
//! This is the game's whole interface: there are no menus. Other modules
//! register commands with [`ConsoleAppExt::console_command`] and handle them
//! by reading [`ConsoleCommand`] messages. Anything can print to the console
//! by writing a [`Print`] message.
//!
//! - [`ui`]: the drop-down panel.
//! - [`script`]: launch arguments, `exec` files and `wait`.
//! - [`settings`]: values remembered between runs.

mod script;
mod settings;
mod ui;

pub use script::ScriptHold;
pub use settings::SaveSetting;

use std::collections::{BTreeMap, VecDeque};

use bevy::{
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
};

use crate::{
    console::{
        script::{queue_launch_commands, run_script},
        settings::{Settings, save_settings},
        ui::{render_console, spawn_console_ui},
    },
    dedicated::Dedicated,
};

const MAX_LINES: usize = 400;
const VISIBLE_LINES: usize = 24;

pub(super) fn plugin(app: &mut App) {
    // A dedicated server has no window: no panel and no typing. Its lines
    // come from stdin (`dedicated.rs`) and its output goes to stdout.
    let headless = app.world().contains_resource::<Dedicated>();
    app.insert_resource(Console {
        stdout: headless,
        ..default()
    })
    .init_resource::<CommandRegistry>()
    .add_message::<ConsoleCommand>()
    .add_message::<Print>()
    .add_message::<SaveSetting>()
    .init_resource::<Settings>()
    .add_systems(Startup, queue_launch_commands)
    .init_resource::<ScriptHold>()
    .add_systems(Update, builtin_commands);
    if headless {
        app.add_systems(PreUpdate, run_script)
            .add_systems(PostUpdate, collect_prints);
    } else {
        // Commands are issued in PreUpdate so every handler, in any module,
        // sees them during Update of the same frame.
        app.add_systems(Startup, spawn_console_ui)
            .add_systems(
                PreUpdate,
                (toggle_console, edit_input, run_script)
                    .chain()
                    .after(InputSystems),
            )
            .add_systems(
                PostUpdate,
                (collect_prints, render_console, save_settings).chain(),
            );
    }

    app.console_command("help", "", "list commands")
        .console_command("clear", "", "clear the console")
        .console_command("quit", "", "exit the game")
        .console_command("echo", "<text>", "print text")
        .console_command("exec", "<file>", "run a script of console commands")
        .console_command("wait", "<seconds>", "pause a script (game time)");
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

    /// A problem with a command. The `error:` prefix is what the test
    /// harness looks for, so use this for anything a script should fail on.
    pub fn error(text: impl std::fmt::Display) -> Self {
        Self(format!("error: {text}"))
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
    /// Also print every line to stdout (a dedicated server, where the log
    /// level hides `info!` in release builds).
    stdout: bool,
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

/// Everything printed goes to the console, the log, and, if `CONSOLE_LOG`
/// names a file, to that file. The test harness reads the file: it works the
/// same in release builds, where info-level logging is compiled out.
fn collect_prints(
    mut prints: MessageReader<Print>,
    mut console: ResMut<Console>,
    mut transcript: Local<Option<Option<std::fs::File>>>,
) {
    use std::io::Write;
    let file = transcript.get_or_insert_with(|| {
        std::env::var_os("CONSOLE_LOG").and_then(|path| std::fs::File::create(path).ok())
    });
    for Print(text) in prints.read() {
        if console.stdout {
            println!("{text}");
        } else {
            info!("{text}");
        }
        for line in text.lines() {
            if let Some(file) = file {
                let _ = writeln!(file, "{line}");
            }
            console.lines.push_back(line.to_string());
        }
    }
    while console.lines.len() > MAX_LINES {
        console.lines.pop_front();
    }
}
