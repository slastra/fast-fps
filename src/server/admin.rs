//! Who runs the game, and their commands from afar.
//!
//! On a LAN, the host runs the game: `mode`, `bot` and the other
//! [`ADMIN_COMMANDS`] work in the host's own console. On a dedicated server
//! nobody sits at the server, so one player is its [`Admin`]: whoever the
//! lobby's token says created the game ([`Creator`]), or else whoever has
//! been here longest. Their client sends those commands as an
//! [`AdminCommand`]; the server runs them as if typed into its own console,
//! and sends what they printed back as [`ConsoleReply`] lines.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet2::netcode::NetcodeServerTransport;

use crate::{
    console::{ConsoleCommand, Print},
    dedicated::Dedicated,
    net::{ADMIN_COMMANDS, AdminCommand, ConsoleReply, Player},
    server::players::{Joined, PlayerClient},
};

/// Byte of a connect token's user data that is 1 for the game's creator.
/// The lobby writes it (`USER_DATA_ADMIN` in `lobby/src/main.rs`).
pub const USER_DATA_ADMIN: usize = 0;

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.init_resource::<ReplyTo>()
        .add_systems(
            PreUpdate,
            (
                mark_creators,
                assign_admin.run_if(resource_exists::<Dedicated>),
                receive_admin,
            )
                .chain()
                .after(ServerSystems::Receive)
                .run_if(authority.clone()),
        )
        .add_systems(
            PostUpdate,
            send_replies.before(ServerSystems::Send).run_if(authority),
        );
}

/// Server-only: this player runs the game.
#[derive(Component)]
pub struct Admin;

/// Server-only: this player's token says they created the game.
#[derive(Component)]
pub struct Creator;

/// Whose command is running this frame, to send its output to.
#[derive(Resource, Default)]
struct ReplyTo(Option<ClientId>);

/// Read the creator flag the lobby put in each new player's token.
fn mark_creators(
    mut commands: Commands,
    new: Query<(Entity, &Player), Added<PlayerClient>>,
    transport: Option<Res<NetcodeServerTransport>>,
) {
    let Some(transport) = transport else { return };
    for (entity, player) in &new {
        if transport
            .user_data(player.owner)
            .is_some_and(|data| data[USER_DATA_ADMIN] == 1)
        {
            commands.entity(entity).insert(Creator);
        }
    }
}

/// Make sure a dedicated server's game has someone running it: the creator
/// if they're here, else whoever arrived first.
#[allow(clippy::type_complexity)]
fn assign_admin(
    mut commands: Commands,
    players: Query<(
        Entity,
        &Player,
        &PlayerClient,
        Option<&Joined>,
        Has<Creator>,
        Has<Admin>,
    )>,
    mut replies: MessageWriter<ToClients<ConsoleReply>>,
) {
    let people = || players.iter().filter(|p| !p.1.is_bot());
    // The creator first, then by arrival; anyone not yet numbered last.
    let Some(best) = people().min_by_key(|p| (!p.4, p.3.map_or(u64::MAX, |j| j.0))) else {
        return;
    };
    let current = people().find(|p| p.5);
    match current {
        Some(admin) if admin.0 == best.0 => return,
        // Hand over only to the creator, so it doesn't move around.
        Some(admin) if best.4 && !admin.4 => {
            commands.entity(admin.0).remove::<Admin>();
        }
        Some(_) => return,
        None => {}
    }
    commands.entity(best.0).insert(Admin);
    replies.write(ToClients {
        targets: SendTargets::Single(best.2.0),
        message: ConsoleReply(format!(
            "You run this game: {} work for you.",
            ADMIN_COMMANDS.join(", ")
        )),
    });
}

fn receive_admin(
    mut received: MessageReader<FromClient<AdminCommand>>,
    players: Query<(&PlayerClient, Has<Admin>)>,
    dedicated: Option<Res<Dedicated>>,
    mut run: MessageWriter<ConsoleCommand>,
    mut replies: MessageWriter<ToClients<ConsoleReply>>,
    mut reply_to: ResMut<ReplyTo>,
) {
    for FromClient { client_id, message } in received.read() {
        let mut words = message.0.split_whitespace().map(String::from);
        let name = words.next().unwrap_or_default();
        let refusal = if !ADMIN_COMMANDS.contains(&name.as_str()) {
            Some(format!("'{name}' can't be sent to the server"))
        } else if !players.iter().any(|(c, admin)| c.0 == *client_id && admin) {
            let who = if dedicated.is_some() {
                "the player running this game"
            } else {
                "the host"
            };
            Some(format!("only {who} can use '{name}'"))
        } else {
            None
        };
        if let Some(refusal) = refusal {
            replies.write(ToClients {
                targets: SendTargets::Single(*client_id),
                message: ConsoleReply(Print::error(refusal).0),
            });
            continue;
        }
        run.write(ConsoleCommand {
            name,
            args: words.collect(),
        });
        reply_to.0 = Some(*client_id);
    }
}

/// Send the admin whatever the command printed.
fn send_replies(
    mut prints: MessageReader<Print>,
    mut reply_to: ResMut<ReplyTo>,
    mut replies: MessageWriter<ToClients<ConsoleReply>>,
) {
    let Some(client) = reply_to.0.take() else {
        prints.clear();
        return;
    };
    for Print(text) in prints.read() {
        replies.write(ToClients {
            targets: SendTargets::Single(client),
            message: ConsoleReply(text.clone()),
        });
    }
}
