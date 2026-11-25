//! Commands that only whoever runs the game may use ([`ADMIN_COMMANDS`]),
//! typed by a client: they're sent to the server, which decides whether
//! this player may (`server/admin.rs`) and answers in [`ConsoleReply`] lines.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    console::{ConsoleCommand, Print},
    net::{ADMIN_COMMANDS, AdminCommand, ConsoleReply},
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, forward)
        .add_systems(PreUpdate, show_replies.after(ClientSystems::Receive));
}

fn forward(
    mut cmds: MessageReader<ConsoleCommand>,
    state: Res<State<ClientState>>,
    mut send: MessageWriter<AdminCommand>,
    mut print: MessageWriter<Print>,
) {
    for cmd in cmds.read() {
        if !ADMIN_COMMANDS.contains(&cmd.name.as_str()) {
            continue;
        }
        // `mode` on its own only shows the mode, which a client knows.
        if cmd.name == "mode" && cmd.args.is_empty() {
            continue;
        }
        match state.get() {
            // Offline or hosting: the command runs here.
            ClientState::Disconnected => {}
            ClientState::Connecting => {
                print.write(Print::error("still connecting"));
            }
            ClientState::Connected => {
                let mut line = cmd.name.clone();
                for arg in &cmd.args {
                    line.push(' ');
                    line.push_str(arg);
                }
                send.write(AdminCommand(line));
            }
        }
    }
}

fn show_replies(mut replies: MessageReader<ConsoleReply>, mut print: MessageWriter<Print>) {
    for reply in replies.read() {
        print.write(Print(reply.0.clone()));
    }
}
