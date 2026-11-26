//! Everything that crosses the wire: replicated components and messages.
//!
//! Every peer must register these in the same order, which is why they all
//! live in one file and are registered by one function. To sync something
//! new, add it here and to [`register`].

use bevy::{ecs::entity::MapEntities, prelude::*};
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::weapon::WeaponKind;

/// Register everything below with Replicon. The order here is part of the
/// protocol: client and server must match, so only ever add to it in one
/// place.
pub(super) fn register(app: &mut App) {
    app.replicate::<Player>()
        .replicate::<PlayerName>()
        .replicate::<Pose>()
        .replicate::<Health>()
        .replicate::<Score>()
        .replicate::<Loadout>()
        .replicate::<Dummy>()
        .replicate::<Downed>()
        .replicate::<Projectile>()
        .add_client_message::<MoveInput>(Channel::Unreliable)
        .add_client_message::<Fire>(Channel::Ordered)
        .add_client_message::<SelectWeapon>(Channel::Ordered)
        .add_client_message::<SetName>(Channel::Ordered)
        .add_client_message::<Suicide>(Channel::Ordered)
        .add_client_message::<Melee>(Channel::Ordered)
        .add_client_message::<Throw>(Channel::Ordered)
        .add_server_message::<ShotFx>(Channel::Unreliable)
        .add_mapped_server_message::<Damaged>(Channel::Ordered)
        .add_server_message::<HitConfirm>(Channel::Ordered)
        .add_server_message::<Respawn>(Channel::Ordered)
        .add_server_message::<Feed>(Channel::Ordered)
        .add_server_message::<Exploded>(Channel::Ordered)
        .replicate::<Duel>()
        .replicate::<Out>()
        .add_client_message::<AdminCommand>(Channel::Ordered)
        .add_server_message::<ConsoleReply>(Channel::Ordered)
        .replicate::<GameMode>()
        .replicate::<Kit>()
        .replicate::<WeaponMods>()
        .replicate::<Mob>()
        .replicate::<Dead>();
    // Game modes keep their own wire types in their own modules; they are
    // registered here, last, so the order is still fixed.
    crate::survival::protocol::register(app);
}

/// A build's Replicon protocol hash as text, for the lobby's version check:
/// a game and a server built from different code can't play together.
pub fn protocol_text(hash: &ProtocolHash) -> String {
    // The hash has no getter; its debug form is `ProtocolHash(<number>)`.
    format!("{hash:?}")
        .chars()
        .filter(char::is_ascii_digit)
        .collect()
}

/// Commands only whoever runs the game may use: the host on a LAN, or the
/// player who created a game on a dedicated server (`server/admin.rs`).
/// A client sends them to the server as an [`AdminCommand`].
pub const ADMIN_COMMANDS: [&str; 4] = ["mode", "bot", "bot_do", "bot_duel_aim"];

/// Client -> server: a console line from [`ADMIN_COMMANDS`], to run there.
#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct AdminCommand(pub String);

/// Server -> one client: a line for their console, such as the answer to an
/// [`AdminCommand`].
#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct ConsoleReply(pub String);

/// A player. `owner` is the netcode client id, or `0` for the host / offline player.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct Player {
    pub owner: u64,
}

impl Player {
    /// `owner` ids for practice bots start here, clear of real client ids.
    pub const BOT_OWNER: u64 = 0xB07_0000;

    pub fn is_bot(&self) -> bool {
        (Self::BOT_OWNER..Self::BOT_OWNER + 0x1_0000).contains(&self.owner)
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Debug)]
pub struct PlayerName(pub String);

/// Where a player or dummy is. `pos` is the collider center.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// World-space velocity, used to animate the body.
    pub vel: Vec3,
    /// `Pose::CROUCH`, `AIR`, `RELOAD`, `HOLSTER`, `DOWN`, `REACH`, `SWING`.
    pub flags: u8,
}

impl Pose {
    pub const CROUCH: u8 = 1;
    pub const AIR: u8 = 2;
    pub const RELOAD: u8 = 4;
    /// A duelist's pistol is still in the holster, at the hip.
    pub const HOLSTER: u8 = 8;
    /// Down on one knee (a game mode's "downed").
    pub const DOWN: u8 = 16;
    /// Arms out in front, reaching (a mob).
    pub const REACH: u8 = 32;
    /// Swinging at someone (a mob's attack).
    pub const SWING: u8 = 64;

    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    pub fn set(&mut self, flag: u8, on: bool) {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Health(pub f32);

/// The weapon a player is holding.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Loadout(pub WeaponKind);

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct Score {
    pub kills: u32,
    pub deaths: u32,
}

/// A practice target. `sway` is how far it slides side to side.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct Dummy {
    pub anchor: Vec3,
    pub sway: f32,
}

/// A dummy that has been knocked down and is waiting to pop back up.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Downed;

/// Client -> server: my character's current pose. Sent every frame, unreliable.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct MoveInput(pub Pose);

/// Client -> server: I fired `weapon` from `origin`; one direction per pellet.
#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct Fire {
    pub weapon: WeaponKind,
    pub origin: Vec3,
    pub dirs: Vec<Vec3>,
}

/// Client -> server: I switched weapons (so others see it).
/// A knife attack: a slash, or a heavy stab with right click.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Melee {
    pub heavy: bool,
    pub origin: Vec3,
    pub dir: Vec3,
}

/// A grenade leaving the hand: a throw, or an underhand lob with right click,
/// at a strength from 0 (a tap) to 1 (held for a full charge).
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Throw {
    pub lob: bool,
    pub strength: f32,
    pub origin: Vec3,
    pub dir: Vec3,
}

/// A grenade in flight. The server simulates it and updates `pos`; clients
/// draw it there.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct Projectile {
    pub owner: u64,
    pub pos: Vec3,
}

/// A grenade went off here.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Exploded {
    pub pos: Vec3,
}

#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SelectWeapon(pub WeaponKind);

#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct SetName(pub String);

#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Suicide;

/// Server -> clients: someone else's shot (one per pellet), for tracers and impacts.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ShotFx {
    /// `Player::owner` of whoever fired.
    pub shooter: u64,
    pub weapon: WeaponKind,
    pub origin: Vec3,
    pub end: Vec3,
    pub normal: Vec3,
    pub flesh: bool,
}

/// Server -> everyone: `target` was hit. Drives flinches, dummy wobble and,
/// when `killed`, the ragdoll.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug, MapEntities)]
pub struct Damaged {
    #[entities]
    pub target: Entity,
    /// Direction the shot was traveling.
    pub dir: Vec3,
    pub point: Vec3,
    pub weapon: WeaponKind,
    pub killed: bool,
    /// Where the target stood when hit. A killed player may already have
    /// respawned elsewhere by the time this arrives.
    pub pos: Vec3,
    pub yaw: f32,
}

/// Server -> shooter: your shot connected.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct HitConfirm {
    pub head: bool,
    pub kill: bool,
}

/// Server -> one client: put your character here.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Respawn {
    pub pos: Vec3,
    pub yaw: f32,
}

/// Server -> everyone: something for the kill feed and console.
#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub enum Feed {
    /// Someone joined, left, renamed or gave up.
    Note(String),
    Kill {
        killer: String,
        victim: String,
        weapon: WeaponKind,
        headshot: bool,
    },
}

impl Feed {
    /// The one-line form printed to the console.
    pub fn text(&self) -> String {
        match self {
            Feed::Note(text) => text.clone(),
            Feed::Kill {
                killer,
                victim,
                weapon,
                headshot,
            } => {
                let how = if *headshot { " (headshot)" } else { "" };
                format!("{killer} [{}] {victim}{how}", weapon.def().name)
            }
        }
    }
}

/// The duel game mode, on one replicated entity that exists only while a
/// duel is on (`mode duel`). The server runs it (`server/duel.rs`); clients
/// draw the marks and the banners from it.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[require(Transform, Visibility)]
pub struct Duel {
    pub phase: DuelPhase,
    /// `Player::owner` of the two duelists. Meaningless while `Waiting`.
    pub duelists: [u64; 2],
    /// Rounds won this duel, by the duelist in the same slot.
    pub wins: [u8; 2],
    /// Counts from 1.
    pub round: u8,
    /// Whether each duelist is standing on their mark.
    pub on_mark: [bool; 2],
    /// Everyone in the game, in the order they get to duel. The two duelists
    /// are at the front.
    pub queue: Vec<u64>,
}

impl Duel {
    /// The duelist slot (0 or 1) of `owner`, if they're dueling.
    pub fn slot(&self, owner: u64) -> Option<usize> {
        if self.phase == DuelPhase::Waiting {
            return None;
        }
        self.duelists.iter().position(|&d| d == owner)
    }

    /// The first to win this many rounds wins the duel.
    pub const WINS: u8 = 3;
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum DuelPhase {
    /// Fewer than two players.
    Waiting,
    /// The duelists walk from back to back to their marks.
    Walk,
    /// Both on their marks; "Draw!" is coming.
    Steady,
    /// Fire!
    Draw,
    /// Someone fell, or time ran out (`winner: None`).
    RoundOver { winner: Option<u8> },
    /// Someone has won the duel.
    Over { winner: u8 },
}

impl DuelPhase {
    /// A short name for the console and the probe.
    pub fn name(self) -> &'static str {
        match self {
            DuelPhase::Waiting => "waiting",
            DuelPhase::Walk => "walk",
            DuelPhase::Steady => "steady",
            DuelPhase::Draw => "draw",
            DuelPhase::RoundOver { .. } => "round_over",
            DuelPhase::Over { .. } => "over",
        }
    }
}

/// On a player who isn't in play right now: dead for the rest of a duel
/// round, or watching someone else's duel. Not drawn, can't be hurt, and
/// can't fire.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Out;

/// The game mode that's on, on one replicated entity the server keeps
/// (`server/rules.rs`), so clients can show it.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameMode(pub String);

/// What a player may use, in a mode that limits it: the weapons they own and
/// how many rounds each has left, magazine and spare together. Without a
/// `Kit` (free-for-all) everything is owned and ammo never runs out.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Kit {
    /// One bit per weapon, by `WeaponKind::index`.
    pub owned: u16,
    /// Rounds left per weapon, magazine and spare together. [`Kit::UNLIMITED`]
    /// never runs out. For the grenade, grenades.
    pub ammo: [u16; WeaponKind::COUNT],
    /// Rounds the server has counted as fired, per weapon (wrapping). The
    /// player's game compares it with its own count to know how many of its
    /// shots the server hasn't seen yet, so its ammo display doesn't jump.
    pub spent: [u16; WeaponKind::COUNT],
}

impl Kit {
    pub const UNLIMITED: u16 = u16::MAX;

    /// Only these weapons, each with unlimited ammo.
    pub fn only(weapons: &[WeaponKind]) -> Self {
        let mut kit = Self {
            owned: 0,
            ammo: [0; WeaponKind::COUNT],
            spent: [0; WeaponKind::COUNT],
        };
        for &w in weapons {
            kit.give(w, Self::UNLIMITED);
        }
        kit
    }

    pub fn owns(&self, weapon: WeaponKind) -> bool {
        self.owned & (1 << weapon.index()) != 0
    }

    /// Own `weapon`, with `rounds` more of it.
    pub fn give(&mut self, weapon: WeaponKind, rounds: u16) {
        self.owned |= 1 << weapon.index();
        self.add(weapon, rounds);
    }

    /// More rounds for `weapon` (nothing changes if it's unlimited).
    pub fn add(&mut self, weapon: WeaponKind, rounds: u16) {
        let left = &mut self.ammo[weapon.index()];
        if *left != Self::UNLIMITED {
            *left = left.saturating_add(rounds).min(Self::UNLIMITED - 1);
        }
    }

    /// Rounds left for `weapon`, if it's limited.
    pub fn left(&self, weapon: WeaponKind) -> Option<u16> {
        let left = self.ammo[weapon.index()];
        (left != Self::UNLIMITED).then_some(left)
    }

    /// Spend one round of `weapon`. False if there are none left.
    pub fn spend(&mut self, weapon: WeaponKind) -> bool {
        let i = weapon.index();
        match self.ammo[i] {
            Self::UNLIMITED => {}
            0 => return false,
            _ => self.ammo[i] -= 1,
        }
        self.spent[i] = self.spent[i].wrapping_add(1);
        true
    }
}

/// Changes to how a player's weapons work, from a mode's upgrades. Without
/// one, every weapon works as its `WeaponDef` says. Read by the weapon code
/// on both sides: the player's game for the magazine, reload, fire mode and
/// speed, the server for damage.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WeaponMods {
    /// Damage, per weapon.
    pub damage: [f32; WeaponKind::COUNT],
    /// Magazine size, per weapon.
    pub mag: [f32; WeaponKind::COUNT],
    /// Extra targets a shot passes through, per weapon.
    pub pierce: [u8; WeaponKind::COUNT],
    /// Pellets per shot instead of the weapon's own (0 keeps it), per weapon.
    pub pellets: [u8; WeaponKind::COUNT],
    /// Weapons that fire while the trigger is held, one bit each.
    pub automatic: u16,
    /// Weapons that lose no accuracy from moving, one bit each.
    pub steady: u16,
    /// Weapons whose spray settles after a few shots, one bit each.
    pub settle: u16,
    /// Reload time, per weapon, and the time to draw a weapon.
    pub reload: [f32; WeaponKind::COUNT],
    pub swap: f32,
    /// Running speed.
    pub speed: f32,
    /// Headshot damage.
    pub head: f32,
}

impl Default for WeaponMods {
    fn default() -> Self {
        Self {
            damage: [1.0; WeaponKind::COUNT],
            mag: [1.0; WeaponKind::COUNT],
            pierce: [0; WeaponKind::COUNT],
            pellets: [0; WeaponKind::COUNT],
            automatic: 0,
            steady: 0,
            settle: 0,
            reload: [1.0; WeaponKind::COUNT],
            swap: 1.0,
            speed: 1.0,
            head: 1.0,
        }
    }
}

impl WeaponMods {
    pub fn has(mask: u16, weapon: WeaponKind) -> bool {
        mask & (1 << weapon.index()) != 0
    }

    /// The magazine size for `weapon` with these changes.
    pub fn mag_of(mods: Option<&Self>, weapon: WeaponKind) -> u32 {
        let base = weapon.def().mag;
        match mods {
            Some(m) => ((base as f32 * m.mag[weapon.index()]).round() as u32).max(1),
            None => base,
        }
    }

    /// Pellets per shot for `weapon` with these changes.
    pub fn pellets_of(mods: Option<&Self>, weapon: WeaponKind) -> u32 {
        match mods.map(|m| m.pellets[weapon.index()]) {
            Some(n) if n > 0 => n as u32,
            _ => weapon.def().pellets,
        }
    }
}

/// Something hostile that isn't a player: hittable (with `Pose`, `Health`
/// and a hitbox), drawn with the avatar skeleton, but not on the scoreboard.
/// A game mode spawns them and gives each `kind` a look (`avatar/mob.rs`).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[require(Transform, Visibility)]
pub struct Mob {
    pub kind: u8,
    /// What the mode wants shown on it (burning, frozen, ...); the mode's
    /// client code reads it.
    pub flags: u8,
}

/// A mob that was killed. It stays a moment so its ragdoll can be made from
/// it, then the server removes it.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Dead;
