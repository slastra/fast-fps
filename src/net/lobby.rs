//! Playing online: the `lobby`, `games`, `create` and `join` commands.
//!
//! The lobby (`lobby/` in this repository) is a small web service. It lists
//! the games running on it, starts a dedicated server for each new one, and
//! hands out the connect tokens those servers require. This side asks it
//! over HTTP, then connects to the game with the token.
//!
//! Requests run in the background; their answers come back through a
//! channel and are handled in [`answers`].

use std::sync::{Mutex, mpsc};

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet2::netcode::{ConnectToken, NetcodeClientTransport, NetcodeServerTransport};
use serde::{Deserialize, Serialize};

use crate::{
    config::DEFAULT_LOBBY,
    console::{ConsoleAppExt, ConsoleCommand, Print, SaveSetting},
    debug::probe::Probe,
    net::{
        protocol_text,
        session::{LocalId, LocalName},
        transport::start_client_secure,
    },
};

pub(super) fn plugin(app: &mut App) {
    let (send, receive) = mpsc::channel();
    app.insert_resource(Lobby {
        url: DEFAULT_LOBBY.into(),
        send,
        receive: Mutex::new(receive),
        listed: 0,
    })
    .add_systems(Update, (lobby_commands, answers));

    app.console_command("lobby", "[url]", "show or set the online lobby")
        .console_command("games", "", "list the games in the lobby")
        .console_command(
            "create",
            "[ffa|duel] [name]",
            "start a game in the lobby and join it",
        )
        .console_command("join", "<id|name>", "join a game in the lobby");
}

#[derive(Resource)]
struct Lobby {
    url: String,
    send: mpsc::Sender<Answer>,
    receive: Mutex<mpsc::Receiver<Answer>>,
    /// How many games the last `games` listed, for test scripts.
    listed: usize,
}

/// A game as the lobby lists it.
#[derive(Deserialize, Debug, Clone)]
struct GameInfo {
    id: u32,
    name: String,
    mode: String,
    players: u32,
    max: u32,
}

#[derive(Deserialize)]
struct Listing {
    games: Vec<GameInfo>,
}

/// A ticket into a game.
#[derive(Deserialize)]
struct Ticket {
    name: String,
    token: String,
}

#[derive(Deserialize)]
struct Refusal {
    error: String,
}

#[derive(Serialize)]
struct CreateBody<'a> {
    name: &'a str,
    mode: &'a str,
    player: &'a str,
    protocol: &'a str,
}

#[derive(Serialize)]
struct JoinBody<'a> {
    player: &'a str,
    protocol: &'a str,
}

/// What came back from the lobby.
enum Answer {
    /// The list of games, to print, or to find `join`'s game in.
    Games {
        games: Vec<GameInfo>,
        join: Option<String>,
    },
    Ticket(Ticket),
    Failed(String),
}

/// Send a request; its answer arrives as an [`Answer`].
fn ask<T: for<'a> Deserialize<'a>>(
    request: ehttp::Request,
    send: &mpsc::Sender<Answer>,
    then: impl FnOnce(T) -> Answer + Send + 'static,
) {
    let send = send.clone();
    ehttp::fetch(request, move |result| {
        let answer = match result {
            Err(e) => Answer::Failed(format!("can't reach the lobby: {e}")),
            Ok(response) if response.ok => match response.json::<T>() {
                Ok(value) => then(value),
                Err(e) => Answer::Failed(format!("the lobby's answer made no sense: {e}")),
            },
            Ok(response) => Answer::Failed(match response.json::<Refusal>() {
                Ok(refusal) => refusal.error,
                Err(_) => format!(
                    "the lobby said {} {}",
                    response.status, response.status_text
                ),
            }),
        };
        let _ = send.send(answer);
    });
}

fn list_games(lobby: &Lobby, join: Option<String>) {
    let request = ehttp::Request::get(format!("{}/games", lobby.url));
    ask(request, &lobby.send, move |listing: Listing| {
        Answer::Games {
            games: listing.games,
            join,
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn lobby_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut lobby: ResMut<Lobby>,
    name: Res<LocalName>,
    protocol: Res<ProtocolHash>,
    server: Option<Res<NetcodeServerTransport>>,
    client: Option<Res<NetcodeClientTransport>>,
    mut print: MessageWriter<Print>,
    mut save: MessageWriter<SaveSetting>,
) {
    let busy = server.is_some() || client.is_some();
    for cmd in cmds.read() {
        match cmd.name.as_str() {
            "lobby" => match cmd.arg(0) {
                None => {
                    print.write(Print(format!("lobby: {}", lobby.url)));
                }
                Some(url) => {
                    lobby.url = url.trim_end_matches('/').to_string();
                    save.write(SaveSetting {
                        key: "lobby",
                        value: lobby.url.clone(),
                    });
                }
            },
            "games" => {
                print.write(Print(format!("asking {}...", lobby.url)));
                list_games(&lobby, None);
            }
            "create" | "join" if busy => {
                print.write(Print::error("'disconnect' first"));
            }
            "create" => {
                let (mode, words) = match cmd.arg(0) {
                    Some(m @ ("ffa" | "duel")) => (m, &cmd.args[1..]),
                    _ => ("ffa", &cmd.args[..]),
                };
                let protocol = protocol_text(&protocol);
                let body = CreateBody {
                    name: &words.join(" "),
                    mode,
                    player: &name.0,
                    protocol: &protocol,
                };
                let Ok(request) = ehttp::Request::post_json(format!("{}/games", lobby.url), &body)
                else {
                    continue;
                };
                print.write(Print(format!("starting a {mode} game...")));
                ask(request, &lobby.send, Answer::Ticket);
            }
            "join" => {
                if cmd.args.is_empty() {
                    print.write(Print::error("usage: join <id|name>"));
                    continue;
                }
                list_games(&lobby, Some(cmd.args.join(" ")));
            }
            _ => {}
        }
    }
}

/// The game `wanted` names: its id, its exact name, or the only name that
/// starts with it (ignoring case).
fn find_game<'a>(games: &'a [GameInfo], wanted: &str) -> Result<&'a GameInfo, String> {
    if let Ok(id) = wanted.parse::<u32>()
        && let Some(game) = games.iter().find(|g| g.id == id)
    {
        return Ok(game);
    }
    let wanted = wanted.to_lowercase();
    if let Some(game) = games.iter().find(|g| g.name.to_lowercase() == wanted) {
        return Ok(game);
    }
    let starts: Vec<&GameInfo> = games
        .iter()
        .filter(|g| g.name.to_lowercase().starts_with(&wanted))
        .collect();
    match starts.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no game called '{wanted}'; try 'games'")),
        _ => Err(format!(
            "more than one game starts with '{wanted}'; use its id"
        )),
    }
}

fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn answers(
    mut lobby: ResMut<Lobby>,
    mut commands: Commands,
    channels: Res<RepliconChannels>,
    name: Res<LocalName>,
    protocol: Res<ProtocolHash>,
    mut local_id: ResMut<LocalId>,
    mut print: MessageWriter<Print>,
    probe: Option<ResMut<Probe>>,
) {
    let received: Vec<Answer> = match lobby.receive.lock() {
        Ok(receive) => receive.try_iter().collect(),
        Err(_) => return,
    };
    for answer in received {
        match answer {
            Answer::Failed(why) => {
                print.write(Print::error(why));
            }
            Answer::Games { games, join: None } => {
                lobby.listed = games.len();
                if games.is_empty() {
                    print.write(Print::new("no games yet; start one with 'create'"));
                }
                for g in &games {
                    print.write(Print(format!(
                        "  {:>3}  {:<24} {:<5} {}/{}",
                        g.id, g.name, g.mode, g.players, g.max
                    )));
                }
            }
            Answer::Games {
                games,
                join: Some(wanted),
            } => {
                lobby.listed = games.len();
                let game = match find_game(&games, &wanted) {
                    Ok(game) => game,
                    Err(why) => {
                        print.write(Print::error(why));
                        continue;
                    }
                };
                let protocol = protocol_text(&protocol);
                let body = JoinBody {
                    player: &name.0,
                    protocol: &protocol,
                };
                let url = format!("{}/games/{}/join", lobby.url, game.id);
                if let Ok(request) = ehttp::Request::post_json(url, &body) {
                    print.write(Print(format!("joining '{}'...", game.name)));
                    ask(request, &lobby.send, Answer::Ticket);
                }
            }
            Answer::Ticket(ticket) => {
                let token = hex_bytes(&ticket.token)
                    .and_then(|bytes| ConnectToken::read(&mut bytes.as_slice()).ok());
                let Some(token) = token else {
                    print.write(Print::error("the lobby sent a broken ticket"));
                    continue;
                };
                match start_client_secure(&mut commands, &channels, token) {
                    Ok(id) => {
                        local_id.0 = id;
                        print.write(Print(format!("connecting to '{}'...", ticket.name)));
                    }
                    Err(e) => {
                        print.write(Print::error(format!("could not connect: {e}")));
                    }
                }
            }
        }
    }
    if let Some(mut probe) = probe {
        probe.set("lobby_games", lobby.listed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn games() -> Vec<GameInfo> {
        ["Duel at Dawn", "duel practice", "Shaun's game"]
            .iter()
            .enumerate()
            .map(|(i, name)| GameInfo {
                id: i as u32 + 1,
                name: name.to_string(),
                mode: "ffa".into(),
                players: 0,
                max: 16,
            })
            .collect()
    }

    #[test]
    fn finding_a_game() {
        let games = games();
        assert_eq!(find_game(&games, "3").unwrap().name, "Shaun's game");
        assert_eq!(find_game(&games, "duel at dawn").unwrap().id, 1);
        assert_eq!(find_game(&games, "shaun").unwrap().id, 3);
        assert!(find_game(&games, "duel").is_err());
        assert!(find_game(&games, "nope").is_err());
    }

    #[test]
    fn hex() {
        assert_eq!(hex_bytes("00ff10"), Some(vec![0, 255, 16]));
        assert_eq!(hex_bytes("0"), None);
        assert_eq!(hex_bytes("zz"), None);
    }
}
