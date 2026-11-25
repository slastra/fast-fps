//! Settings that survive a restart. A module writes a [`SaveSetting`] and the
//! value is stored as a console line, replayed at startup: in `config.cfg`,
//! or in a browser, its local storage (`platform.rs`).

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::{config::GAME_NAME, console::Print, platform};

/// Remember a setting: it is written to `config.cfg` as `key value` and
/// replayed on the next launch.
#[derive(Message, Clone, Debug)]
pub struct SaveSetting {
    pub key: &'static str,
    pub value: String,
}

#[derive(Resource, Default)]
pub struct Settings(pub BTreeMap<String, String>);

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
    let mut text = format!("// Written by {GAME_NAME}; runs at startup before launch arguments.\n");
    for (key, value) in &settings.0 {
        text.push_str(&format!("{key} {value}\n"));
    }
    if let Err(e) = platform::save_settings(&text) {
        print.write(Print::error(e));
    }
}
