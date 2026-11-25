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

use std::sync::{Mutex, mpsc};

use bevy::prelude::*;

use crate::{
    config::GAME_NAME,
    console::{CommandRegistry, Console, ConsoleCommand, Print, settings::Settings},
    dedicated::Dedicated,
    platform,
};

/// Runs the saved settings, then the launch arguments: `+name alice +host
/// 27015` runs `name alice` then `host 27015`. In a browser the launch
/// arguments come from the page's URL instead (`?c=name alice&c=host`).
pub fn queue_launch_commands(
    mut console: ResMut<Console>,
    mut settings: ResMut<Settings>,
    mut print: MessageWriter<Print>,
    dedicated: Option<Res<Dedicated>>,
) {
    if dedicated.is_none() {
        print.write(Print(format!(
            "{GAME_NAME}. Press ~ for this console. Try: help, host, find, connect <ip>"
        )));
    }
    // A player's saved settings are for a player's game, not a server.
    let saved = platform::load_settings().filter(|_| dedicated.is_none());
    if let Some(text) = saved {
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
    // A browser starts on medium graphics until the player picks (docs/web.md).
    if cfg!(target_arch = "wasm32") && !settings.0.contains_key("r_quality") {
        console.script.push_front("r_quality medium".into());
    }
    console.script.extend(platform::launch_lines());
}

/// Frames drawn before the first script line runs, so shaders are compiled
/// and the first screenshot isn't of a half-built scene.
const WARMUP_FRAMES: u32 = 30;

/// Script files on their way to `exec` (`platform::read_script`).
#[derive(Resource)]
pub struct Scripts {
    send: mpsc::Sender<(String, Result<String, String>)>,
    receive: Mutex<mpsc::Receiver<(String, Result<String, String>)>>,
}

impl Default for Scripts {
    fn default() -> Self {
        let (send, receive) = mpsc::channel();
        Self {
            send,
            receive: Mutex::new(receive),
        }
    }
}

/// Reasons the script can't continue yet. Add one when you start something
/// the next line depends on (loading an asset, saving a screenshot) and
/// subtract it when that finishes.
#[derive(Resource, Default)]
pub struct ScriptHold(pub u32);

/// Run typed lines at once and the next script line when it is due.
pub fn run_script(
    time: Res<Time>,
    mut hold: ResMut<ScriptHold>,
    scripts: Res<Scripts>,
    mut console: ResMut<Console>,
    registry: Res<CommandRegistry>,
    mut commands: MessageWriter<ConsoleCommand>,
    mut print: MessageWriter<Print>,
    mut frames: Local<u32>,
) {
    let mut lines: Vec<String> = std::mem::take(&mut console.queued);

    // Scripts that `exec` asked for and that have arrived.
    let arrived: Vec<_> = match scripts.receive.lock() {
        Ok(receive) => receive.try_iter().collect(),
        Err(_) => Vec::new(),
    };
    for (path, text) in arrived {
        hold.0 = hold.0.saturating_sub(1);
        match text {
            // Run the file before whatever was already scripted.
            Ok(text) => {
                for (i, l) in text.lines().enumerate() {
                    console.script.insert(i, l.to_string());
                }
            }
            Err(e) => {
                print.write(Print::error(format!("exec {path}: {e}")));
            }
        }
    }

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
                "exec" => match args.first() {
                    Some(path) => {
                        // Natively the file is read at once; in a browser
                        // it's fetched, and the script waits for it.
                        hold.0 += 1;
                        let (send, path) = (scripts.send.clone(), path.clone());
                        platform::read_script(&path.clone(), move |text| {
                            let _ = send.send((path, text));
                        });
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
