//! Running console lines that weren't typed: launch arguments (`+host`),
//! the saved settings file, and `exec` scripts with `wait`.

use bevy::prelude::*;

use crate::{
    config::GAME_NAME,
    console::{
        CommandRegistry, Console, ConsoleCommand, Print,
        settings::{Settings, config_path},
    },
};

/// Runs `config.cfg`, then the launch arguments:
/// `fast-fps +name alice +host 27015` runs `name alice` then `host 27015`.
pub fn queue_launch_commands(
    mut console: ResMut<Console>,
    mut settings: ResMut<Settings>,
    mut print: MessageWriter<Print>,
) {
    print.write(Print(format!(
        "{GAME_NAME}. Press ~ for this console. Try: help, host, find, connect <ip>"
    )));
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

/// Parse queued lines (`;` separates commands) and emit them as messages.
/// Script lines run until a `wait` pauses them.
pub fn run_queued(
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
