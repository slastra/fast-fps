//! Survival's wire types: what the server tells every machine about the run,
//! and what players ask of it. Registered at the end of
//! `net::protocol::register`, so the protocol's order stays fixed.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

pub fn register(app: &mut App) {
    app.replicate::<Survival>()
        .replicate::<Fallen>()
        .replicate::<Upgrades>()
        .replicate::<Offer>()
        .replicate::<Pickup>()
        .add_client_message::<PickCard>(Channel::Ordered)
        .add_client_message::<Revive>(Channel::Ordered)
        .add_server_message::<Flash>(Channel::Unreliable);
}

/// The run, on one replicated entity while survival is on.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Survival {
    pub phase: Phase,
    /// The wave being fought, or just fought (0 before the first).
    pub wave: u16,
    /// Whole seconds left in a breather or on the end screen.
    pub secs: u8,
    /// Zombies left this wave: alive, plus those still to come through.
    pub left: u16,
    /// Open cracks, one bit each, in `survival::cracks::CRACKS` order.
    pub cracks: u16,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Between waves: pick a card.
    Breather,
    Wave,
    /// Everyone went down; the run starts again soon.
    Over,
}

/// `Mob::kind` for each kind of zombie.
pub const WALKER: u8 = 1;
pub const RUNNER: u8 = 2;
pub const BRUTE: u8 = 3;

/// `Mob::flags` on a zombie, for drawing what's happening to it.
pub const BURNING: u8 = 1;
pub const CHILLED: u8 = 2;
pub const FROZEN: u8 = 4;

/// A player who's down: they can't move or (mostly) shoot, and a teammate
/// can bring them back. Replicated so everyone can see it.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Fallen {
    /// Whole seconds left before they bleed out and watch until the next
    /// breather.
    pub bleed: u8,
    /// How far a revive has got, 0 to 100.
    pub revive: u8,
}

/// A player's cards this run, by id (`survival::cards::CARDS`), in the
/// order taken. Everything they have follows from this list.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Upgrades(pub Vec<u8>);

/// The cards offered to a player right now (empty when there's no choice to
/// make).
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Offer(pub Vec<u8>);

/// Something to pick up, lying where a zombie fell.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[require(Transform, Visibility)]
pub struct Pickup {
    pub kind: PickupKind,
    pub pos: Vec3,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupKind {
    /// A spare magazine for every gun you own (and a grenade).
    Ammo,
    /// Some health back.
    Medkit,
}

/// Client -> server: I take card `n` (0, 1 or 2) of my offer.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PickCard(pub u8);

/// Client -> server: I'm holding the revive key (true), or let go (false).
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Revive(pub bool);

/// Server -> everyone: something to draw that has no lasting state.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub enum Flash {
    /// A shock arc from one point to another.
    Arc { from: Vec3, to: Vec3 },
    /// A small burst (explosive tips, shatter, bomblets).
    Burst { at: Vec3, radius: f32 },
    /// Burning ground for a while (napalm).
    Fire { at: Vec3, radius: f32, secs: f32 },
    /// Dust where a zombie climbs through a crack.
    Dust { at: Vec3 },
    /// Someone healed.
    Heal { at: Vec3 },
}
