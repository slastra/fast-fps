//! The draft: at each breather every player is offered three cards and
//! takes one (`PickCard`, keys 1-3); whoever hasn't chosen when the wave
//! starts gets one at random. The breather also tops up ammo.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use rand::Rng;

use super::{Vigor, resupply};
use crate::{
    net::{Health, Kit, WeaponMods},
    server::players::{PlayerClient, find_player},
    survival::{
        cards::{Build, CARDS, Effect, offer},
        protocol::{Offer, PickCard, Upgrades},
    },
};

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Breather>().add_message::<WaveBegun>();
}

/// Server-only: a breather began after wave `0`.
#[derive(Message, Clone, Copy)]
pub struct Breather(pub u16);

/// Server-only: a wave began.
#[derive(Message, Clone, Copy)]
pub struct WaveBegun;

/// Offer cards at a breather (and top up ammo); settle unpicked offers when
/// the wave begins.
pub fn offers(
    mut breathers: MessageReader<Breather>,
    mut waves: MessageReader<WaveBegun>,
    mut players: Query<(
        &Build,
        &mut Upgrades,
        &mut Offer,
        &mut Kit,
        &mut Health,
        &WeaponMods,
        &Vigor,
    )>,
) {
    let mut rng = rand::rng();
    for Breather(wave) in breathers.read() {
        for (build, _, mut offered, mut kit, _, mods, _) in &mut players {
            offered.0 = offer(build, *wave, &mut rng);
            resupply(&mut kit, mods, 1.0);
        }
    }
    if waves.read().count() > 0 {
        for (_, mut upgrades, mut offered, mut kit, mut health, mods, vigor) in &mut players {
            if offered.0.is_empty() {
                continue;
            }
            let id = offered.0[rng.random_range(0..offered.0.len())];
            offered.0.clear();
            take(id, &mut upgrades, &mut kit, &mut health, mods, vigor);
        }
    }
}

/// A player chose a card.
pub fn receive_picks(
    mut picks: MessageReader<FromClient<PickCard>>,
    clients: Query<(Entity, &PlayerClient)>,
    mut players: Query<(
        &mut Upgrades,
        &mut Offer,
        &mut Kit,
        &mut Health,
        &WeaponMods,
        &Vigor,
    )>,
) {
    for FromClient { client_id, message } in picks.read() {
        let Some(entity) = find_player(*client_id, &clients) else {
            continue;
        };
        let Ok((mut upgrades, mut offered, mut kit, mut health, mods, vigor)) =
            players.get_mut(entity)
        else {
            continue;
        };
        let Some(&id) = offered.0.get(message.0 as usize) else {
            continue;
        };
        offered.0.clear();
        take(id, &mut upgrades, &mut kit, &mut health, mods, vigor);
    }
}

/// Take card `id`: add it to the player's cards, or for a refill, refill.
pub fn take(
    id: u8,
    upgrades: &mut Upgrades,
    kit: &mut Kit,
    health: &mut Health,
    mods: &WeaponMods,
    vigor: &Vigor,
) {
    match CARDS[id as usize].effect {
        Effect::Supplies => resupply(kit, mods, 2.0),
        Effect::Patch => health.0 = health.0.max(vigor.0),
        // Everything else lives in the list; `equip_all` applies it.
        _ => upgrades.0.push(id),
    }
}
