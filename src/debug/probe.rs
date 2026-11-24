//! `state`, `assert` and `wait_until`: how a script finds out what the game
//! is actually doing.
//!
//! The [`Probe`] is a table of named values. `state` prints them, `assert`
//! checks one right now, and `wait_until` pauses the script until one becomes
//! true (or gives up). Failures print with the `error:` prefix, which fails a
//! harness run.
//!
//! ```text
//! bot
//! assert bots == 1
//! connect 127.0.0.1
//! wait_until mode == client 15
//! ```
//!
//! To make something of your own checkable, write it into the probe from any
//! system: `probe.set("coins", coins.0)`. Built-in values are filled in by
//! [`collect`].

use std::collections::BTreeMap;

use avian3d::prelude::*;
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_ahoy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::RenetServer;

use crate::{
    avatar::Ragdoll,
    console::{ConsoleAppExt, ConsoleCommand, Print, ScriptHold},
    net::{Downed, Dummy, Health, LocalId, LocalName, Player, Projectile, Score},
    player::{LocalPlayer, ViewSettings},
    sky::Sky,
    weapon::{Arsenal, Explosions, fx::Decal},
};

/// Seconds `wait_until` waits when the script doesn't say.
const DEFAULT_TIMEOUT: f32 = 10.0;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Probe>().add_systems(Update, probe);

    app.console_command("state", "[key ...]", "print probe values (all if no keys)")
        .console_command(
            "assert",
            "<key> <op> <value>",
            "check a probe value now; ops: == != < <= > >=",
        )
        .console_command(
            "wait_until",
            "<key> <op> <value> [seconds]",
            "pause a script until a probe value matches (default 10 s)",
        );
}

/// Named values a script can inspect.
#[derive(Resource, Default)]
pub struct Probe {
    values: BTreeMap<String, String>,
    waiting: Option<Waiting>,
}

impl Probe {
    /// Publish a value under `key`. Numbers compare as numbers.
    pub fn set(&mut self, key: &str, value: impl ToString) {
        self.values.insert(key.to_string(), value.to_string());
    }
}

struct Condition {
    key: String,
    op: String,
    value: String,
}

struct Waiting {
    condition: Condition,
    waited: f32,
    timeout: f32,
}

impl Condition {
    fn parse(args: &[String]) -> Option<Self> {
        let [key, op, value, ..] = args else {
            return None;
        };
        matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=").then(|| Self {
            key: key.clone(),
            op: op.clone(),
            value: value.clone(),
        })
    }

    /// `Ok(true)` if it holds, `Err` if it can't be evaluated at all.
    fn holds(&self, values: &BTreeMap<String, String>) -> Result<bool, String> {
        let Some(actual) = values.get(&self.key) else {
            let known: Vec<&str> = values.keys().map(String::as_str).collect();
            return Err(format!(
                "no probe value '{}'; known: {}",
                self.key,
                known.join(" ")
            ));
        };
        if let (Ok(a), Ok(b)) = (actual.parse::<f32>(), self.value.parse::<f32>()) {
            let close = (a - b).abs() <= 1e-3 * b.abs().max(1.0);
            return Ok(match self.op.as_str() {
                "==" => close,
                "!=" => !close,
                "<" => a < b,
                "<=" => a <= b || close,
                ">" => a > b,
                _ => a >= b || close,
            });
        }
        let same = actual.eq_ignore_ascii_case(&self.value);
        match self.op.as_str() {
            "==" => Ok(same),
            "!=" => Ok(!same),
            op => Err(format!(
                "'{op}' needs numbers, but {} is '{actual}'",
                self.key
            )),
        }
    }

    fn describe(&self, values: &BTreeMap<String, String>) -> String {
        let actual = values.get(&self.key).map_or("unset", String::as_str);
        format!(
            "{} {} {} ({} is {actual})",
            self.key, self.op, self.value, self.key
        )
    }
}

/// Fill in the built-in probe values.
fn collect(
    probe: &mut Probe,
    time: &Time,
    state: &State<ClientState>,
    hosting: bool,
    (local, name): (&LocalId, &LocalName),
    players: &Query<(&Player, &Health, &Score)>,
    (ragdolls, decals, dummies_down): (usize, usize, usize),
    (grenades, explosions): (usize, u32),
    arsenal: &Arsenal,
    (body, velocity, controller): (&Transform, &LinearVelocity, &CharacterControllerState),
    view: &ViewSettings,
    sky: &Sky,
    fps: f64,
) {
    let mode = if hosting {
        "hosting"
    } else {
        match state.get() {
            ClientState::Connected => "client",
            ClientState::Connecting => "connecting",
            ClientState::Disconnected => "offline",
        }
    };
    probe.set("mode", mode);
    probe.set("name", &name.0);
    probe.set("time", format!("{:.2}", time.elapsed_secs()));
    probe.set("fps", format!("{fps:.0}"));

    // Everyone in the game, bots included.
    probe.set("players", players.iter().count());
    probe.set("bots", players.iter().filter(|(p, ..)| p.is_bot()).count());
    if let Some((_, health, score)) = players.iter().find(|(p, ..)| p.owner == local.0) {
        probe.set("health", health.0.round());
        probe.set("kills", score.kills);
        probe.set("deaths", score.deaths);
    }

    probe.set("ragdolls", ragdolls);
    probe.set("decals", decals);
    probe.set("dummies_down", dummies_down);
    probe.set("grenades", grenades);
    probe.set("explosions", explosions);

    probe.set("weapon", arsenal.def().name.to_lowercase());
    probe.set("ammo", arsenal.ammo[arsenal.current.index()]);
    probe.set("reloading", u8::from(arsenal.reloading > 0.0));
    probe.set("scoped", u8::from(arsenal.scoped()));
    probe.set("spread", format!("{:.2}", arsenal.inaccuracy));

    let at = body.translation;
    probe.set("x", format!("{:.2}", at.x));
    probe.set("y", format!("{:.2}", at.y));
    probe.set("z", format!("{:.2}", at.z));
    probe.set("speed", format!("{:.2}", velocity.0.with_y(0.0).length()));
    probe.set("grounded", u8::from(controller.grounded.is_some()));
    probe.set("crouching", u8::from(controller.crouching));

    probe.set("sky", sky.kind.name());
    probe.set("sens", view.sens);
    probe.set("fov", view.fov);
}

fn probe(
    mut cmds: MessageReader<ConsoleCommand>,
    mut print: MessageWriter<Print>,
    mut probe: ResMut<Probe>,
    mut hold: ResMut<ScriptHold>,
    time: Res<Time>,
    (state, server): (Res<State<ClientState>>, Option<Res<RenetServer>>),
    (local, name): (Res<LocalId>, Res<LocalName>),
    players: Query<(&Player, &Health, &Score)>,
    (ragdolls, decals, dummies_down): (
        Query<(), With<Ragdoll>>,
        Query<(), With<Decal>>,
        Query<(), (With<Dummy>, With<Downed>)>,
    ),
    arsenal: Res<Arsenal>,
    (grenades, explosions): (Query<(), With<Projectile>>, Res<Explosions>),
    body: Single<(&Transform, &LinearVelocity, &CharacterControllerState), With<LocalPlayer>>,
    (view, sky): (Res<ViewSettings>, Res<Sky>),
    diagnostics: Res<DiagnosticsStore>,
) {
    let wanted: Vec<&ConsoleCommand> = cmds
        .read()
        .filter(|c| matches!(c.name.as_str(), "state" | "assert" | "wait_until"))
        .collect();
    if wanted.is_empty() && probe.waiting.is_none() {
        return;
    }

    // Frames over the average frame time of the last 120. Bevy's smoothed FPS
    // is barely smoothed at high frame rates (its factor is in seconds, about
    // 16 ms), so one slow frame dragged the readout down to that frame's rate.
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.average())
        .filter(|ms| *ms > 0.0)
        .map_or(0.0, |ms| 1000.0 / ms);
    collect(
        &mut probe,
        &time,
        &state,
        server.is_some(),
        (&local, &name),
        &players,
        (
            ragdolls.iter().count(),
            decals.iter().count(),
            dummies_down.iter().count(),
        ),
        (grenades.iter().count(), explosions.0),
        &arsenal,
        *body,
        &view,
        &sky,
        fps,
    );

    for cmd in wanted {
        match cmd.name.as_str() {
            "state" => {
                let pairs: Vec<String> = probe
                    .values
                    .iter()
                    .filter(|(k, _)| cmd.args.is_empty() || cmd.args.contains(k))
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect();
                print.write(Print(format!("state: {}", pairs.join(" "))));
            }
            "assert" => {
                let Some(condition) = Condition::parse(&cmd.args) else {
                    print.write(Print::error("usage: assert <key> <op> <value>"));
                    continue;
                };
                match condition.holds(&probe.values) {
                    Ok(true) => {
                        print.write(Print(format!(
                            "assert ok: {}",
                            condition.describe(&probe.values)
                        )));
                    }
                    Ok(false) => {
                        print.write(Print::error(format!(
                            "assert failed: {}",
                            condition.describe(&probe.values)
                        )));
                    }
                    Err(why) => {
                        print.write(Print::error(format!("assert: {why}")));
                    }
                }
            }
            _ => {
                let Some(condition) = Condition::parse(&cmd.args) else {
                    print.write(Print::error(
                        "usage: wait_until <key> <op> <value> [seconds]",
                    ));
                    continue;
                };
                let timeout = cmd
                    .arg(3)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_TIMEOUT);
                if probe.waiting.is_none() {
                    hold.0 += 1;
                }
                probe.waiting = Some(Waiting {
                    condition,
                    waited: 0.0,
                    timeout,
                });
            }
        }
    }

    // Re-check a pending `wait_until` every frame.
    let Some(mut waiting) = probe.waiting.take() else {
        return;
    };
    waiting.waited += time.delta_secs();
    let what = waiting.condition.describe(&probe.values);
    let finished = match waiting.condition.holds(&probe.values) {
        Ok(true) => Some(Print(format!(
            "wait_until ok after {:.1}s: {what}",
            waiting.waited
        ))),
        Err(why) => Some(Print::error(format!("wait_until: {why}"))),
        Ok(false) if waiting.waited >= waiting.timeout => Some(Print::error(format!(
            "wait_until timed out after {:.0}s: {what}",
            waiting.timeout
        ))),
        Ok(false) => None,
    };
    match finished {
        Some(message) => {
            print.write(message);
            hold.0 = hold.0.saturating_sub(1);
        }
        None => probe.waiting = Some(waiting),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn check(expr: &str, pairs: &[(&str, &str)]) -> Result<bool, String> {
        let args: Vec<String> = expr.split_whitespace().map(str::to_string).collect();
        Condition::parse(&args)
            .ok_or_else(|| "parse".to_string())?
            .holds(&values(pairs))
    }

    #[test]
    fn numbers_compare_as_numbers() {
        let v = [("ammo", "29"), ("y", "0.90")];
        assert_eq!(check("ammo == 29", &v), Ok(true));
        assert_eq!(check("ammo < 30", &v), Ok(true));
        assert_eq!(check("ammo >= 30", &v), Ok(false));
        assert_eq!(check("y == 0.9", &v), Ok(true));
        assert_eq!(check("y != 0.9", &v), Ok(false));
    }

    #[test]
    fn text_compares_ignoring_case() {
        let v = [("mode", "client"), ("weapon", "ak")];
        assert_eq!(check("mode == Client", &v), Ok(true));
        assert_eq!(check("weapon != sniper", &v), Ok(true));
        assert!(check("mode > client", &v).is_err());
    }

    #[test]
    fn unknown_keys_and_bad_operators_are_errors() {
        assert!(check("nope == 1", &[("ammo", "1")]).is_err());
        assert!(check("ammo = 1", &[("ammo", "1")]).is_err());
        assert!(check("ammo ==", &[("ammo", "1")]).is_err());
    }
}
