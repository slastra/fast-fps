//! Settings that survive a restart. A module writes a [`SaveSetting`] and the
//! value is stored in `config.cfg` as a console line, replayed at startup.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::{config::GAME_NAME, console::Print};

/// Remember a setting: it is written to `config.cfg` as `key value` and
/// replayed on the next launch.
#[derive(Message, Clone, Debug)]
pub struct SaveSetting {
    pub key: &'static str,
    pub value: String,
}

#[derive(Resource, Default)]
pub struct Settings(pub BTreeMap<String, String>);

/// `$XDG_CONFIG_HOME/fast-fps/config.cfg`, falling back to `~/.config`.
pub fn config_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join(GAME_NAME).join("config.cfg"))
}

pub fn save_settings(
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
    let mut text = format!("// Written by {GAME_NAME}; runs at startup before launch arguments.\n");
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
