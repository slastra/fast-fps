//! Running console lines that weren't typed: launch arguments (`+host`),
//! the saved settings file, and `exec` scripts.
//!
//! Scripts are what the test harness drives the game with, so they are built
//! to behave the same on a fast desktop and a slow laptop:
//!
//! - Nothing runs until the game has loaded and drawn a few frames.
//! - One line runs per frame, so a line always sees the result of the one
//!   before it (spawn a bot, then order it).
//! - `wait` counts game time, not wall-clock time.
//! - Work that finishes later, like saving a screenshot, takes a
//!   [`ScriptHold`] and the script pauses until it is released.

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

/// Frames drawn before the first script line runs, so shaders are compiled
/// and the first screenshot isn't of a half-built scene.
const WARMUP_FRAMES: u32 = 30;

/// Reasons the script can't continue yet. Add one when you start something
/// the next line depends on (loading an asset, saving a screenshot) and
/// subtract it when that finishes.
#[derive(Resource, Default)]
pub struct ScriptHold(pub u32);

/// Run typed lines at once and the next script line when it is due.
pub fn run_script(
    time: Res<Time>,
    hold: Res<ScriptHold>,
    mut console: ResMut<Console>,
    registry: Res<CommandRegistry>,
    mut commands: MessageWriter<ConsoleCommand>,
    mut print: MessageWriter<Print>,
    mut frames: Local<u32>,
) {
    let mut lines: Vec<String> = std::mem::take(&mut console.queued);

    *frames += 1;
    console.wait -= time.delta_secs();
    if *frames > WARMUP_FRAMES && hold.0 == 0 && console.wait <= 0.0 {
        while let Some(line) = console.script.pop_front() {
            let text = line.trim();
            if text.is_empty() || text.starts_with("//") || text.starts_with('#') {
                continue;
            }
            let mut words = text.split_whitespace();
            if words.next() == Some("wait") {
                match words.next().and_then(|w| w.parse::<f32>().ok()) {
                    Some(seconds) => console.wait = seconds,
                    None => {
                        print.write(Print::error("usage: wait <seconds>"));
                    }
                }
            } else {
                lines.push(line);
            }
            break;
        }
    }

    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        print.write(Print(format!("] {line}")));
        // `;` puts several commands on one line, and so in one frame.
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
                        print.write(Print::error(format!("exec {}: {e}", args[0])));
                    }
                    None => {
                        print.write(Print::error("usage: exec <file>"));
                    }
                },
                "wait" => {
                    print.write(Print::error("wait only works inside a script"));
                }
                _ if registry.0.contains_key(name.as_str()) => {
                    commands.write(ConsoleCommand { name, args });
                }
                _ => {
                    print.write(Print::error(format!(
                        "unknown command '{name}', try 'help'"
                    )));
                }
            }
        }
    }
}
