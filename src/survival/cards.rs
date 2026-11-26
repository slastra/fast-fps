//! The upgrade cards: what each is, which a player may be offered, and what
//! a player's cards add up to ([`Build`]).
//!
//! A player's cards are a list of ids in the order taken (`Upgrades`), and
//! everything else follows from that list, on the server (to set their `Kit`
//! and `WeaponMods`) and on every player's machine (to show it). To add a
//! card, add it to [`CARDS`] and give its effect a home in [`Build::add`]
//! (or in `server/survival/effects.rs`, for what happens on a hit).

use bevy::prelude::Component;
use rand::Rng;

use crate::weapon::WeaponKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rarity {
    Common,
    Rare,
    Legendary,
}

/// Special ammo: loaded into a slot, it changes every gun's shots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ammo {
    Incendiary,
    Frost,
    Shock,
    Piercing,
    Explosive,
    Leech,
}

/// Two ammo types loaded together make a combo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combo {
    /// Incendiary + Explosive: blasts leave burning ground.
    Napalm,
    /// Frost + Shock: a frozen zombie killed bursts and hurts those around.
    Shatter,
    /// Incendiary + Frost: hitting a burning, chilled zombie hits hard.
    ThermalShock,
    /// Piercing + Shock: arcs leap farther and to more zombies.
    Rail,
    /// Explosive + Leech: blasts heal teammates caught in them.
    BloodBomb,
    /// Piercing + Incendiary: everything a shot passes through burns longer.
    TracerFire,
}

impl Combo {
    pub fn of(a: Ammo, b: Ammo) -> Option<Combo> {
        use Ammo::*;
        Some(match (a, b) {
            (Incendiary, Explosive) | (Explosive, Incendiary) => Combo::Napalm,
            (Frost, Shock) | (Shock, Frost) => Combo::Shatter,
            (Incendiary, Frost) | (Frost, Incendiary) => Combo::ThermalShock,
            (Piercing, Shock) | (Shock, Piercing) => Combo::Rail,
            (Explosive, Leech) | (Leech, Explosive) => Combo::BloodBomb,
            (Piercing, Incendiary) | (Incendiary, Piercing) => Combo::TracerFire,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Combo::Napalm => "Napalm",
            Combo::Shatter => "Shatter",
            Combo::ThermalShock => "Thermal Shock",
            Combo::Rail => "Rail",
            Combo::BloodBomb => "Blood Bomb",
            Combo::TracerFire => "Tracer Fire",
        }
    }
}

/// Lasting traits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perk {
    Juggernaut,
    QuickHands,
    Deadeye,
    SecondWind,
    Scavenger,
    Medic,
    Rally,
    Barricade,
    Butcher,
    Cluster,
    LastStand,
    Bandolier,
    FleetFoot,
}

impl Perk {
    fn bit(self) -> u32 {
        1 << self as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Own a weapon.
    Unlock(WeaponKind),
    /// Raise a weapon you own to this level (1 to 3).
    Level(WeaponKind, u8),
    /// Load an ammo type.
    Load(Ammo),
    /// A second ammo slot.
    TwinBelt,
    Perk(Perk),
    /// Refills, always available: spare magazines for every gun.
    Supplies,
    /// Refills, always available: full health.
    Patch,
}

pub struct Card {
    pub name: &'static str,
    pub text: &'static str,
    pub rarity: Rarity,
    pub effect: Effect,
}

const fn card(name: &'static str, text: &'static str, rarity: Rarity, effect: Effect) -> Card {
    Card {
        name,
        text,
        rarity,
        effect,
    }
}

use Effect::*;
use Rarity::*;
use WeaponKind as W;

/// Every card. A card's id is its index here, so only ever add to the end.
pub const CARDS: &[Card] = &[
    // Weapons you don't have yet.
    card("MP5", "Fast and light: 30 rounds", Common, Unlock(W::Mp5)),
    card(
        "Shotgun",
        "Eight pellets, up close",
        Common,
        Unlock(W::Shotgun),
    ),
    card("AK", "Hits hard, kicks hard", Common, Unlock(W::Ak)),
    card("Sniper", "One shot, a long way", Common, Unlock(W::Sniper)),
    card("Flintlock", "One huge ball", Common, Unlock(W::Flintlock)),
    card("Grenades", "Two a wave", Common, Unlock(W::Grenade)),
    // Level I: damage.
    card("Glock I", "Glock damage +30%", Common, Level(W::Glock, 1)),
    card("MP5 I", "MP5 damage +30%", Common, Level(W::Mp5, 1)),
    card(
        "Shotgun I",
        "Shotgun damage +30%",
        Common,
        Level(W::Shotgun, 1),
    ),
    card("AK I", "AK damage +30%", Common, Level(W::Ak, 1)),
    card(
        "Sniper I",
        "Sniper damage +30%",
        Common,
        Level(W::Sniper, 1),
    ),
    card(
        "Flintlock I",
        "Flintlock damage +30%",
        Common,
        Level(W::Flintlock, 1),
    ),
    // Level II: magazine and reload.
    card(
        "Glock II",
        "Bigger magazine, faster reload",
        Rare,
        Level(W::Glock, 2),
    ),
    card(
        "MP5 II",
        "Bigger magazine, faster reload",
        Rare,
        Level(W::Mp5, 2),
    ),
    card(
        "Shotgun II",
        "Bigger magazine, faster reload",
        Rare,
        Level(W::Shotgun, 2),
    ),
    card(
        "AK II",
        "Bigger magazine, faster reload",
        Rare,
        Level(W::Ak, 2),
    ),
    card(
        "Sniper II",
        "Bigger magazine, faster reload",
        Rare,
        Level(W::Sniper, 2),
    ),
    card(
        "Flintlock II",
        "Faster reload",
        Rare,
        Level(W::Flintlock, 2),
    ),
    // Level III: a signature.
    card(
        "Hair Trigger",
        "Glock III: fully automatic",
        Legendary,
        Level(W::Glock, 3),
    ),
    card(
        "Run-and-Gun",
        "MP5 III: no spread from moving",
        Legendary,
        Level(W::Mp5, 3),
    ),
    card(
        "Slug Kick",
        "Shotgun III: knocks zombies back",
        Legendary,
        Level(W::Shotgun, 3),
    ),
    card(
        "Settle",
        "AK III: spray tightens after 5 shots",
        Legendary,
        Level(W::Ak, 3),
    ),
    card(
        "Overpenetration",
        "Sniper III: through 3 zombies",
        Legendary,
        Level(W::Sniper, 3),
    ),
    card(
        "Grapeshot",
        "Flintlock III: 8 balls a shot",
        Legendary,
        Level(W::Flintlock, 3),
    ),
    // Ammo.
    card(
        "Incendiary",
        "Shots set zombies burning",
        Rare,
        Load(Ammo::Incendiary),
    ),
    card(
        "Frost",
        "Shots slow; enough freeze solid",
        Rare,
        Load(Ammo::Frost),
    ),
    card(
        "Shock",
        "Shots arc to 2 more zombies",
        Rare,
        Load(Ammo::Shock),
    ),
    card(
        "Piercing",
        "Shots pass through a zombie",
        Rare,
        Load(Ammo::Piercing),
    ),
    card(
        "Explosive Tip",
        "Shots burst where they hit",
        Rare,
        Load(Ammo::Explosive),
    ),
    card(
        "Leech",
        "Hits heal you a little, headshots more",
        Rare,
        Load(Ammo::Leech),
    ),
    card(
        "Twin Belt",
        "A second ammo slot: combos",
        Legendary,
        TwinBelt,
    ),
    // Perks.
    card("Juggernaut", "+50 health", Common, Perk(Perk::Juggernaut)),
    card(
        "Quick Hands",
        "Reload and swap 40% faster",
        Rare,
        Perk(Perk::QuickHands),
    ),
    card("Deadeye", "Headshots +50%", Rare, Perk(Perk::Deadeye)),
    card(
        "Second Wind",
        "Get back up by yourself, once",
        Legendary,
        Perk(Perk::SecondWind),
    ),
    card(
        "Scavenger",
        "Kills drop more ammo, and health",
        Rare,
        Perk(Perk::Scavenger),
    ),
    card("Medic", "Heal teammates near you", Rare, Perk(Perk::Medic)),
    card(
        "Rally",
        "Revive twice as fast, with more health",
        Rare,
        Perk(Perk::Rally),
    ),
    card(
        "Barricade",
        "Fewer zombies through the nearest crack",
        Rare,
        Perk(Perk::Barricade),
    ),
    card(
        "Butcher",
        "Knife +50%; knife kills heal",
        Rare,
        Perk(Perk::Butcher),
    ),
    card(
        "Cluster",
        "Grenades burst into bomblets",
        Rare,
        Perk(Perk::Cluster),
    ),
    card(
        "Last Stand",
        "Keep shooting while down",
        Legendary,
        Perk(Perk::LastStand),
    ),
    card(
        "Bandolier",
        "Ammo drops give half again",
        Common,
        Perk(Perk::Bandolier),
    ),
    card(
        "Fleet Foot",
        "Run 15% faster",
        Common,
        Perk(Perk::FleetFoot),
    ),
    // Refills, when nothing else fits.
    card(
        "Supplies",
        "Two spare magazines for every gun",
        Common,
        Supplies,
    ),
    card("Patch Up", "Back to full health", Common, Patch),
];

/// The guns that have levels (the knife and grenade don't).
pub const LEVELED: [WeaponKind; 6] = [W::Glock, W::Mp5, W::Shotgun, W::Ak, W::Sniper, W::Flintlock];

/// What a player's cards add up to. On the server it's also a component on
/// each player, kept in step with their `Upgrades`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Build {
    /// Weapons owned beyond the starting Glock and knife.
    pub unlocked: Vec<WeaponKind>,
    pub levels: [u8; WeaponKind::COUNT],
    pub ammo: Vec<Ammo>,
    pub slots: u8,
    perks: u32,
}

impl Build {
    pub fn from_cards(cards: &[u8]) -> Self {
        let mut build = Build {
            slots: 1,
            ..Default::default()
        };
        for &id in cards {
            if let Some(card) = CARDS.get(id as usize) {
                build.add(card.effect);
            }
        }
        build
    }

    fn add(&mut self, effect: Effect) {
        match effect {
            Unlock(w) if !self.unlocked.contains(&w) => self.unlocked.push(w),
            Level(w, n) => self.levels[w.index()] = self.levels[w.index()].max(n),
            Load(a) if !self.ammo.contains(&a) && self.ammo.len() < self.slots as usize => {
                self.ammo.push(a)
            }
            TwinBelt => self.slots = 2,
            Perk(p) => self.perks |= p.bit(),
            _ => {}
        }
    }

    pub fn owns(&self, w: WeaponKind) -> bool {
        matches!(w, W::Glock | W::Knife) || self.unlocked.contains(&w)
    }

    pub fn has(&self, perk: Perk) -> bool {
        self.perks & perk.bit() != 0
    }

    pub fn loaded(&self, ammo: Ammo) -> bool {
        self.ammo.contains(&ammo)
    }

    pub fn level(&self, w: WeaponKind) -> u8 {
        self.levels[w.index()]
    }

    /// The combo the loaded ammo makes, if any.
    pub fn combo(&self) -> Option<Combo> {
        match self.ammo.as_slice() {
            [a, b] => Combo::of(*a, *b),
            _ => None,
        }
    }

    /// Whether `card` may be offered to this player now.
    pub fn eligible(&self, card: &Card) -> bool {
        match card.effect {
            Unlock(w) => !self.owns(w),
            Level(w, n) => self.owns(w) && self.level(w) + 1 == n,
            Load(a) => !self.loaded(a) && self.ammo.len() < self.slots as usize,
            TwinBelt => self.slots == 1 && !self.ammo.is_empty(),
            Perk(p) => !self.has(p),
            // Refills only fill gaps; see `offer`.
            Supplies | Patch => false,
        }
    }
}

/// How likely each rarity is at `wave`, out of 100: legendaries from wave
/// 6, a little likelier every wave after.
pub fn rarity_weights(wave: u16) -> [(Rarity, u32); 3] {
    let legendary = if wave >= 6 {
        (4 + 2 * (wave as u32 - 6)).min(20)
    } else {
        0
    };
    let rare = 28 + (wave as u32).min(12);
    [
        (Common, 100 - rare - legendary),
        (Rare, rare),
        (Legendary, legendary),
    ]
}

/// Three different cards for a player with `build` after `wave`, drawn by
/// rarity, from those they may take. Refills fill any gap (so very late in
/// a run there may be only the two refills).
pub fn offer(build: &Build, wave: u16, rng: &mut impl Rng) -> Vec<u8> {
    let mut pool: Vec<u8> = (0..CARDS.len() as u8)
        .filter(|&id| build.eligible(&CARDS[id as usize]))
        .collect();
    let weights = rarity_weights(wave);
    let mut picked = Vec::new();
    while picked.len() < 3 && !pool.is_empty() {
        // A rarity first, then a card of it (falling back to any card).
        let mut roll = rng.random_range(0..100u32);
        let mut rarity = Common;
        for (r, w) in weights {
            if roll < w {
                rarity = r;
                break;
            }
            roll -= w;
        }
        let of_rarity: Vec<usize> = (0..pool.len())
            .filter(|&i| CARDS[pool[i] as usize].rarity == rarity)
            .collect();
        let i = if of_rarity.is_empty() {
            rng.random_range(0..pool.len())
        } else {
            of_rarity[rng.random_range(0..of_rarity.len())]
        };
        picked.push(pool.swap_remove(i));
    }
    for refill in ["Supplies", "Patch Up"] {
        if picked.len() < 3 {
            picked.push(id_of(refill));
        }
    }
    picked
}

/// A card's id by name (for tests, scripts and refills).
pub fn id_of(name: &str) -> u8 {
    CARDS
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no card named {name}")) as u8
}

/// A card's id by name, or `None`.
pub fn find(name: &str) -> Option<u8> {
    let name = name.replace(['_', '-'], " ");
    CARDS
        .iter()
        .position(|c| c.name.replace('-', " ").eq_ignore_ascii_case(&name))
        .map(|i| i as u8)
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;

    #[test]
    fn a_new_player_has_a_glock_and_a_knife() {
        let build = Build::from_cards(&[]);
        assert!(build.owns(W::Glock) && build.owns(W::Knife));
        assert!(!build.owns(W::Ak));
        assert_eq!(build.slots, 1);
    }

    #[test]
    fn levels_come_in_order_and_need_the_weapon() {
        let build = Build::from_cards(&[]);
        assert!(build.eligible(&CARDS[id_of("Glock I") as usize]));
        assert!(!build.eligible(&CARDS[id_of("Glock II") as usize]));
        assert!(!build.eligible(&CARDS[id_of("AK I") as usize]));
        let build = Build::from_cards(&[id_of("AK"), id_of("AK I")]);
        assert!(build.eligible(&CARDS[id_of("AK II") as usize]));
        assert_eq!(build.level(W::Ak), 1);
    }

    #[test]
    fn ammo_needs_a_free_slot_and_two_make_a_combo() {
        let one = Build::from_cards(&[id_of("Frost")]);
        assert!(!one.eligible(&CARDS[id_of("Shock") as usize]));
        assert!(one.eligible(&CARDS[id_of("Twin Belt") as usize]));
        let two = Build::from_cards(&[id_of("Frost"), id_of("Twin Belt"), id_of("Shock")]);
        assert_eq!(two.combo(), Some(Combo::Shatter));
        assert!(!two.eligible(&CARDS[id_of("Leech") as usize]));
    }

    #[test]
    fn every_pair_of_the_six_combos() {
        use Ammo::*;
        assert_eq!(Combo::of(Explosive, Incendiary), Some(Combo::Napalm));
        assert_eq!(Combo::of(Incendiary, Frost), Some(Combo::ThermalShock));
        assert_eq!(Combo::of(Shock, Piercing), Some(Combo::Rail));
        assert_eq!(Combo::of(Leech, Explosive), Some(Combo::BloodBomb));
        assert_eq!(Combo::of(Piercing, Incendiary), Some(Combo::TracerFire));
        assert_eq!(Combo::of(Leech, Frost), None);
    }

    #[test]
    fn offers_are_three_different_eligible_cards() {
        let mut rng = StdRng::seed_from_u64(7);
        for wave in 1..30 {
            let build = Build::from_cards(&[id_of("AK"), id_of("Incendiary")]);
            let cards = offer(&build, wave, &mut rng);
            assert_eq!(cards.len(), 3);
            let mut unique = cards.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), 3, "wave {wave}: {cards:?}");
            for id in cards {
                let card = &CARDS[id as usize];
                assert!(build.eligible(card) || matches!(card.effect, Supplies | Patch));
                if wave < 6 {
                    assert_ne!(card.rarity, Legendary, "wave {wave}");
                }
            }
        }
    }

    #[test]
    fn refills_fill_in_when_little_is_left() {
        // Everything taken (Twin Belt first, so two ammo types fill both
        // slots): only the two refills are left to offer.
        let belt = id_of("Twin Belt");
        let everything: Vec<u8> = std::iter::once(belt)
            .chain((0..CARDS.len() as u8).filter(|&id| id != belt))
            .collect();
        let build = Build::from_cards(&everything);
        let cards = offer(&build, 20, &mut StdRng::seed_from_u64(1));
        assert_eq!(cards, vec![id_of("Supplies"), id_of("Patch Up")]);
        // One thing left: it, then the refills.
        let almost: Vec<u8> = everything
            .iter()
            .copied()
            .filter(|&id| id != id_of("Deadeye"))
            .collect();
        let cards = offer(
            &Build::from_cards(&almost),
            20,
            &mut StdRng::seed_from_u64(1),
        );
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[0], id_of("Deadeye"));
    }

    #[test]
    fn names_find_cards() {
        assert_eq!(find("hair_trigger"), Some(id_of("Hair Trigger")));
        assert_eq!(find("run-and-gun"), Some(id_of("Run-and-Gun")));
        assert_eq!(find("nope"), None);
    }
}
