//! Practice bots: server-side players with no client behind them.
//!
//! They exist to be looked at and shot. A bot follows simple orders from the
//! console (walk, strafe, crouch, jump, reload, fire) so every animation can
//! be seen offline and scripted in the test harness. Their shots are for show
//! and do no damage.
//!
//! Bots only exist when hosting or offline; clients see them as players.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::{
    config::{GRAVITY, MAX_HEALTH, SHOT_RANGE},
    console::{ConsoleAppExt, ConsoleCommand, Print},
    net::{Health, Loadout, Player, PlayerName, Pose, Score, ShotFx},
    physics::Layer,
    player::LocalPlayer,
    server::{
        combat::Died,
        players::{hitbox, pick_spawn},
    },
    weapon::{Class, WeaponKind},
};

const JUMP_SPEED: f32 = 8.4;
const EYE: f32 = 0.8;

pub(super) fn plugin(app: &mut App) {
    let authority = in_state(ClientState::Disconnected);
    app.init_resource::<NextBot>()
        .add_systems(Update, bot_commands)
        .add_systems(Update, (drive, respawn).run_if(authority));

    app.console_command(
        "bot",
        "[x y z yaw]",
        "spawn a practice bot (host or offline)",
    )
    .console_command(
        "bot_do",
        "<id|all> <order> [arg]",
        "idle walk run strafe back crouch stand jump reload fire weapon look pin free remove",
    );
}

#[derive(Resource, Default)]
struct NextBot(u32);

#[derive(Clone, Copy, PartialEq)]
enum Travel {
    Idle,
    /// Direction in body space (x right, y forward) and speed.
    Move(Vec2, f32),
}

#[derive(Component)]
pub struct Bot {
    id: u32,
    home: Vec3,
    yaw: f32,
    travel: Travel,
    /// Distance covered along the patrol circle.
    distance: f32,
    /// Animate in place instead of patrolling.
    pinned: bool,
    crouch: bool,
    /// Seconds since jumping.
    jump: Option<f32>,
    reload: f32,
    shots: u32,
    cooldown: f32,
    pitch: f32,
}

/// The yaw that turns -Z to face along `dir`.
fn yaw_of(dir: Vec3) -> f32 {
    (-dir.x).atan2(-dir.z)
}

fn bot_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut commands: Commands,
    mut print: MessageWriter<Print>,
    mut next: ResMut<NextBot>,
    state: Res<State<ClientState>>,
    viewer: Query<&Transform, With<LocalPlayer>>,
    camera: Query<&GlobalTransform, With<crate::player::ViewRig>>,
    mut bots: Query<(Entity, &mut Bot, &mut Loadout)>,
) {
    for cmd in cmds.read() {
        if cmd.name != "bot" && cmd.name != "bot_do" {
            continue;
        }
        if *state.get() != ClientState::Disconnected {
            print.write(Print::error("bots only work when hosting or offline"));
            continue;
        }
        if cmd.name == "bot" {
            let nums: Vec<f32> = cmd.args.iter().filter_map(|a| a.parse().ok()).collect();
            let (pos, yaw) = match nums[..] {
                [x, y, z] => (Vec3::new(x, y, z), 0.0),
                [x, y, z, yaw] => (Vec3::new(x, y, z), yaw.to_radians()),
                _ => {
                    // Five meters in front of you, facing you.
                    let (Ok(body), Ok(view)) = (viewer.single(), camera.single()) else {
                        continue;
                    };
                    let ahead = (view.forward().as_vec3().with_y(0.0))
                        .try_normalize()
                        .unwrap_or(Vec3::NEG_Z);
                    (body.translation + ahead * 5.0, yaw_of(-ahead))
                }
            };
            next.0 += 1;
            let id = next.0;
            commands.spawn((
                Replicated,
                Player {
                    owner: Player::BOT_OWNER + id as u64,
                },
                PlayerName(format!("bot{id}")),
                Health(MAX_HEALTH),
                Score::default(),
                Loadout::default(),
                Pose {
                    pos,
                    yaw,
                    ..default()
                },
                Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)),
                hitbox(),
                Bot {
                    id,
                    home: pos,
                    yaw,
                    travel: Travel::Idle,
                    distance: 0.0,
                    pinned: false,
                    crouch: false,
                    jump: None,
                    reload: 0.0,
                    shots: 0,
                    cooldown: 0.0,
                    pitch: 0.0,
                },
            ));
            print.write(Print(format!(
                "bot{id} spawned; order it with 'bot_do {id} walk'"
            )));
            continue;
        }

        let (Some(who), Some(order)) = (cmd.arg(0), cmd.arg(1)) else {
            print.write(Print::error("usage: bot_do <id|all> <order> [arg]"));
            continue;
        };
        let id: Option<u32> = who.parse().ok();
        if id.is_none() && who != "all" {
            print.write(Print::error("usage: bot_do <id|all> <order> [arg]"));
            continue;
        }
        let arg = cmd.arg(2);
        let number = arg.and_then(|a| a.parse::<f32>().ok());
        let mut matched = false;
        for (entity, mut bot, mut loadout) in &mut bots {
            if id.is_some_and(|id| id != bot.id) {
                continue;
            }
            matched = true;
            let speed = |default: f32| number.unwrap_or(default);
            match order {
                "idle" | "stop" => bot.travel = Travel::Idle,
                "walk" => bot.travel = Travel::Move(Vec2::Y, speed(3.5)),
                "run" => bot.travel = Travel::Move(Vec2::Y, speed(9.0)),
                "strafe" => bot.travel = Travel::Move(Vec2::X, speed(5.0)),
                "back" => bot.travel = Travel::Move(Vec2::NEG_Y, speed(3.5)),
                "crouch" => bot.crouch = true,
                "stand" => bot.crouch = false,
                "jump" => {
                    if bot.jump.is_none() {
                        bot.jump = Some(0.0);
                    }
                }
                "reload" => bot.reload = loadout.0.def().reload_secs,
                "fire" => bot.shots += number.map_or(1, |n| n as u32),
                "look" => bot.pitch = number.unwrap_or(0.0).to_radians().clamp(-1.5, 1.5),
                "pin" => bot.pinned = true,
                "free" => bot.pinned = false,
                "weapon" => match arg.and_then(WeaponKind::parse) {
                    Some(kind) if kind.def().class == Class::Gun => loadout.0 = kind,
                    Some(_) => {
                        print.write(Print::error("bots can only use guns"));
                    }
                    None => {
                        print.write(Print::new(
                            "bot_do <id> weapon <ak|mp5|glock|shotgun|sniper> (bots only use guns)",
                        ));
                    }
                },
                "remove" => commands.entity(entity).despawn(),
                _ => {
                    print.write(Print::error(format!("unknown bot order '{order}'")));
                }
            }
        }
        if !matched {
            print.write(Print::error(format!("no bot '{who}'")));
        }
    }
}

/// Move every bot and write the pose a real client would have sent.
fn drive(
    time: Res<Time>,
    spatial: SpatialQuery,
    mut bots: Query<(&mut Bot, &mut Pose, &Player, &Loadout)>,
    mut fx: MessageWriter<ToClients<ShotFx>>,
) {
    let dt = time.delta_secs();
    for (mut bot, mut pose, player, loadout) in &mut bots {
        let bot = &mut *bot;
        let crouch_scale = if bot.crouch { 0.4 } else { 1.0 };
        let (mut pos, mut yaw, mut vel) = (bot.home, bot.yaw, Vec3::ZERO);

        if let Travel::Move(local, speed) = bot.travel {
            let speed = speed * crouch_scale;
            let local = Vec3::new(local.x, 0.0, -local.y);
            if bot.pinned {
                vel = Quat::from_rotation_y(bot.yaw) * local * speed;
            } else {
                // Patrol a circle that starts at home, heading the way the bot faces.
                let radius = (speed * 0.8).clamp(2.5, 7.0);
                bot.distance += speed * dt;
                let angle = bot.distance / radius;
                let start = Quat::from_rotation_y(bot.yaw);
                let offset = Vec3::new(radius * (1.0 - angle.cos()), 0.0, -radius * angle.sin());
                let heading = start * Vec3::new(angle.sin(), 0.0, -angle.cos());
                pos = bot.home + start * offset;
                yaw = yaw_of(heading) - yaw_of(local);
                vel = heading * speed;
            }
        }

        let mut airborne = false;
        if let Some(t) = bot.jump {
            let t = t + dt;
            let height = JUMP_SPEED * t - 0.5 * GRAVITY * t * t;
            if height > 0.0 {
                bot.jump = Some(t);
                pos.y += height;
                vel.y = JUMP_SPEED - GRAVITY * t;
                airborne = true;
            } else {
                bot.jump = None;
            }
        }

        bot.reload = (bot.reload - dt).max(0.0);
        bot.cooldown -= dt;
        if bot.shots > 0 && bot.cooldown <= 0.0 && bot.reload <= 0.0 {
            bot.shots -= 1;
            let def = loadout.0.def();
            bot.cooldown = def.interval;
            let aim = Quat::from_euler(EulerRot::YXZ, yaw, bot.pitch, 0.0);
            let origin = pos + Vec3::Y * if bot.crouch { 0.3 } else { EYE };
            let filter = SpatialQueryFilter::from_mask(Layer::World);
            for _ in 0..def.pellets {
                let spread = Quat::from_euler(
                    EulerRot::YXZ,
                    (rand::random::<f32>() - 0.5) * def.pellet_spread.to_radians() * 2.0,
                    (rand::random::<f32>() - 0.5) * def.pellet_spread.to_radians() * 2.0,
                    0.0,
                );
                let dir = aim * spread * Vec3::NEG_Z;
                let hit = Dir3::new(dir)
                    .ok()
                    .and_then(|d| spatial.cast_ray(origin, d, SHOT_RANGE, true, &filter));
                let (end, normal) = match hit {
                    Some(hit) => (origin + dir * hit.distance, hit.normal),
                    None => (origin + dir * SHOT_RANGE, Vec3::ZERO),
                };
                fx.write(ToClients {
                    targets: SendTargets::All,
                    message: ShotFx {
                        shooter: player.owner,
                        weapon: loadout.0,
                        origin,
                        end,
                        normal,
                        flesh: false,
                    },
                });
            }
        }

        let mut next = Pose {
            pos,
            yaw,
            pitch: bot.pitch,
            vel,
            flags: 0,
        };
        next.set(Pose::CROUCH, bot.crouch);
        next.set(Pose::AIR, airborne);
        next.set(Pose::RELOAD, bot.reload > 0.0);
        *pose = next;
    }
}

/// A killed bot starts over at a spawn point, like a player would.
fn respawn(mut deaths: MessageReader<Died>, mut bots: Query<&mut Bot>) {
    for Died(entity) in deaths.read() {
        if let Ok(mut bot) = bots.get_mut(*entity) {
            let (pos, yaw) = pick_spawn();
            bot.home = pos;
            bot.yaw = yaw;
            bot.distance = 0.0;
            bot.jump = None;
        }
    }
}
