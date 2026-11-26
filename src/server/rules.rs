//! What game modes have in common: the [`Rules`] they set, the `mode`
//! command that switches between them, and the messages they react to.
//!
//! A game mode is a plugin that:
//! - names itself with [`GameModeAppExt::game_mode`],
//! - starts when a [`ModeChange`] names it and stops when one doesn't,
//! - sets [`Rules`] for its game,
//! - with `Rules::respawn` off, decides what a death means by reading
//!   [`Died`], and where a stuck player goes by reading [`Unstuck`].
//!
//! The core never asks which mode is on. Every switch starts everyone
//! fresh (scores cleared, full health, back in play at a spawn point, no
//! `Kit` or `WeaponMods`, default `Rules`) before the new mode sets its own.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    config::MAX_HEALTH,
    console::{ConsoleAppExt, ConsoleCommand, Print},
    net::{Feed, GameMode, Health, Kit, Out, Respawn, Score, WeaponMods},
    server::{
        bots::Bot,
        players::{PlayerClient, pick_spawn, place},
    },
};

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.init_resource::<Rules>()
        .init_resource::<Modes>()
        .add_message::<ModeChange>()
        .add_message::<Died>()
        .add_message::<Unstuck>()
        .add_systems(PreUpdate, show_mode.run_if(authority))
        .add_systems(Update, mode_command.in_set(ModeSwitch));
    app.console_command(
        "mode",
        "[name]",
        "the game mode (host or offline); 'mode' alone lists them",
    );
}

/// How the current game works. Each mode sets these when it starts; a
/// switch puts back the defaults (free-for-all).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// A killed player comes straight back at a spawn point. Off, they stay
    /// at 0 health and the mode decides (by reading [`Died`]).
    pub respawn: bool,
    /// Players can hurt each other.
    pub friendly_fire: bool,
    /// The practice dummies stand by the east wall.
    pub dummies: bool,
    /// A killed player falls as a ragdoll. Off, nothing falls (the mode
    /// shows it another way, such as kneeling).
    pub player_corpses: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            respawn: true,
            friendly_fire: true,
            dummies: true,
            player_corpses: true,
        }
    }
}

/// The modes there are and which is on.
#[derive(Resource)]
pub struct Modes {
    names: Vec<&'static str>,
    current: &'static str,
}

impl Default for Modes {
    fn default() -> Self {
        Self {
            names: vec!["ffa"],
            current: "ffa",
        }
    }
}

/// The `mode` command runs in this set. A mode's system that starts or
/// stops it on [`ModeChange`] goes `.after(ModeSwitch)`, so the switch takes
/// effect in the same frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModeSwitch;

/// Server-only: the game mode is now `to`. Modes start when it's theirs and
/// stop when it isn't.
#[derive(Message, Clone, Copy, Debug)]
pub struct ModeChange {
    pub to: &'static str,
}

/// Server-only: `victim` was killed by `killer` (itself for `kill` or a
/// fall). Players, bots and mobs alike.
#[derive(Message, Clone, Copy, Debug)]
pub struct Died {
    pub victim: Entity,
    pub killer: Entity,
}

/// Server-only: a player asked to be moved (the `kill` command, or falling
/// off the world) while `Rules::respawn` is off. The mode puts them
/// somewhere that makes sense in it.
#[derive(Message, Clone, Copy, Debug)]
pub struct Unstuck(pub Entity);

/// Server-only: this player may not attack right now (a mode's rule, such
/// as before "Draw!").
#[derive(Component)]
pub struct Disarmed;

/// Server-only: nothing can hurt this player or mob right now (a mode's
/// rule, such as while downed).
#[derive(Component)]
pub struct Untouchable;

pub trait GameModeAppExt {
    /// Make `mode <name>` switch to this mode.
    fn game_mode(&mut self, name: &'static str) -> &mut Self;
}

impl GameModeAppExt for App {
    fn game_mode(&mut self, name: &'static str) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<Modes>()
            .names
            .push(name);
        self
    }
}

/// Keep the replicated [`GameMode`] entity in step, so clients can show it.
fn show_mode(modes: Res<Modes>, mut shown: Query<&mut GameMode>, mut commands: Commands) {
    match shown.single_mut() {
        Ok(mut mode) if mode.0 != modes.current => mode.0 = modes.current.to_string(),
        Ok(_) => {}
        Err(_) => {
            commands.spawn((Replicated, GameMode(modes.current.to_string())));
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn mode_command(
    mut cmds: MessageReader<ConsoleCommand>,
    state: Res<State<ClientState>>,
    mut modes: ResMut<Modes>,
    mut rules: ResMut<Rules>,
    shown: Query<&GameMode>,
    mut players: Query<(
        Entity,
        &mut Score,
        &mut Health,
        Option<&PlayerClient>,
        Option<&mut Bot>,
    )>,
    mut respawns: MessageWriter<ToClients<Respawn>>,
    mut feed: MessageWriter<ToClients<Feed>>,
    mut print: MessageWriter<Print>,
    mut changes: MessageWriter<ModeChange>,
    mut commands: Commands,
) {
    for cmd in cmds.read() {
        if cmd.name != "mode" {
            continue;
        }
        let Some(wanted) = cmd.arg(0) else {
            let current = shown
                .single()
                .map_or(modes.current.to_string(), |m| m.0.clone());
            print.write(Print(format!(
                "mode: {current} (modes: {})",
                modes.names.join(", ")
            )));
            continue;
        };
        // On a client, `net/admin.rs` sends it to the server instead.
        if *state.get() != ClientState::Disconnected {
            continue;
        }
        let Some(&to) = modes.names.iter().find(|n| **n == wanted) else {
            print.write(Print::error(format!(
                "usage: mode <{}>",
                modes.names.join("|")
            )));
            continue;
        };
        if to == modes.current {
            print.write(Print(format!("starting {to} over")));
        }
        // Everyone starts fresh; the new mode sets up its own game next.
        *rules = Rules::default();
        modes.current = to;
        for (entity, mut score, mut health, client, bot) in &mut players {
            *score = Score::default();
            health.0 = MAX_HEALTH;
            commands
                .entity(entity)
                .remove::<(Out, Disarmed, Untouchable, Kit, WeaponMods)>();
            place(client, bot, pick_spawn(), &mut respawns);
        }
        if to == "ffa" {
            feed.write(ToClients {
                targets: SendTargets::All,
                message: Feed::Note("Free-for-all".into()),
            });
        }
        changes.write(ModeChange { to });
    }
}
