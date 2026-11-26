# Survival

Co-op against endless waves of zombies. `mode survival` starts a run (on the host, offline, or
by whoever runs an online game); `mode survival` again starts it over, and `mode ffa` ends it.

## How a run goes

- **Everyone starts in the keep** with a Glock, three spare magazines and the knife, and no
  cards.
- **Waves.** After a 5 second breather, wave 1 sends 3 walkers. Each wave after brings 2 more
  (`3 + 2·(n−1)`), a little tougher and faster. At most 24 are up at once; the rest wait their
  turn at the cracks.
- **Cracks.** The outer wall has twelve bricked-up breaches. A crack is one of them broken
  open: zombies come from outside and climb in through the hole, and shots and grenades go
  through it, but players can't walk out. Wave 1 opens one crack, wave 2 a second, then one
  more every other wave, up to all twelve. The four beside the north and south doors open
  first. A zombie that sees you (within 18 m, no wall between) comes straight for you; once
  inside the keep, they all come for whoever is nearest, by a door if the keep's wall is in
  the way.
- **Kinds.** Each has its own two glowing dot eyes, so you can tell them apart at night.

  | Kind | From wave | Health | Speed | | Eyes |
  |---|---|---|---|---|---|
  | Walker | 1 | 80 | 2.2 m/s | Swings for 15 | Red |
  | Crawler | 3 | 35 | 3.4 m/s | Legless, low to the ground, in packs of four through one crack. Swings for 8. Aim low. | Red |
  | Bloater | 4 | 140 | 1.4 m/s | Bursts when it reaches you or is killed: up to 50 to players and zombies within 3 m, then gas that hurts anyone in it for 4 s. Pop it at range, ideally in a crowd. | Green |
  | Runner | 5 | 50 | 4.5 m/s | Swings for 10 | Yellow |
  | Leaper | 8 | 70 | 2.6 m/s | From 3.5 to 9 m away it crouches for 0.6 s, then leaps and lands on you for 30. Sidestep the crouch. Claws for 12 up close. | Red |
  | Brute | 10 | 400 | 1.6 m/s | Swings for 35 | Purple |
- **Breathers.** When a wave is cleared there are 15 seconds before the next. Everyone gets a
  spare magazine for every gun they own and is offered three cards (see below).
- **Ammo is scarce.** Guns have the rounds they came with and what you find: a killed zombie
  sometimes drops an ammo box (a magazine for every gun you own). Walk over it to take it.
- **No friendly fire.** Bots can play too, but don't count toward keeping a run alive.

## Going down

A hit that would kill you puts you on your knees instead. You can't move or shoot (unless you
took Last Stand), and zombies leave you alone. A teammate who holds **E** beside you for 3
seconds brings you back with 40 health. Nobody comes within 30 seconds and you bleed out: you
watch until the next breather, then come back with your cards and full health.

**The run ends when everyone is down at once.** Solo, that's the first time you go down. Ten
seconds later a new run starts from nothing.

## The cards

At each breather you're offered three cards; press **1**, **2** or **3** (or `pick 1`). If you
haven't chosen when the wave starts, one is picked for you. Cards are only offered when they
make sense: a level for a gun you own, the next level up, ammo while you have a free slot.

| | Rarity | |
|---|---|---|
| **Guns** | Common | MP5, shotgun, AK, sniper, flintlock, grenades (two a wave) |
| **Level I** | Common | That gun's damage +30% |
| **Level II** | Rare | Magazine half again as big, reload a quarter faster |
| **Level III** | Legendary | Glock: fully automatic. MP5: no spread from moving. Shotgun: knocks zombies back. AK: spray tightens after 5 shots. Sniper: through 3 zombies. Flintlock: 8 balls a shot |
| **Ammo** | Rare | One slot (two with Twin Belt): Incendiary (they burn), Frost (they slow, and freeze solid after four hits), Shock (a hit arcs to 2 more), Piercing (through one zombie), Explosive Tip (a burst where it hits), Leech (hits heal you, headshots more) |
| **Twin Belt** | Legendary | A second ammo slot |
| **Perks** | Common to legendary | Juggernaut (+50 health), Quick Hands (reload and swap 40% faster), Deadeye (headshots +50%), Second Wind (get up by yourself, once a run), Scavenger (more ammo drops, and medkits), Medic (teammates near you heal), Rally (revive twice as fast, with more health), Barricade (fewer zombies through the crack nearest you), Butcher (knife +50%, knife kills heal), Cluster (grenades burst into bomblets), Last Stand (shoot while down), Bandolier (ammo boxes give half again), Fleet Foot (run 15% faster) |
| **Refills** | | Supplies (two spare magazines for every gun) and Patch Up (full health), offered when little else is left |

Legendaries appear from wave 6 and get likelier each wave after.

### Combos

Two ammo types loaded together make a combo:

| Ammo | Combo | |
|---|---|---|
| Incendiary + Explosive Tip | Napalm | Bursts leave burning ground |
| Frost + Shock | Shatter | A frozen zombie killed bursts and hurts those around it |
| Incendiary + Frost | Thermal Shock | Hitting a zombie that's burning and chilled spends both for a big extra hit |
| Piercing + Shock | Rail | Arcs leap farther, to more zombies |
| Explosive Tip + Leech | Blood Bomb | Bursts heal teammates caught in them |
| Piercing + Incendiary | Tracer Fire | Fire burns longer and hotter |

The line under the wave count shows your loaded ammo and combo.

## Sounds

All made with `scripts/sounds.py` like the rest of the game's sounds. Zombies groan now and
then (a crawler drags itself instead), and a leaper screeches as it crouches to leap: that's
the warning to listen for. A bloater bursting, fire catching, ice forming and shock arcing
are heard where they happen, fading with distance. Everyone hears stone giving way where a
crack opens, a war horn when a wave starts and a gong when it's cleared. You hear a blow land
on you, your heartbeat when you go down, chimes when someone is healed, a card being taken,
and an ammo box picked up.

## Console

| Command | |
|---|---|
| `mode survival` | Start a run, or start it over. |
| `pick <1\|2\|3>` | Take one of the cards on offer. |
| `revive <on\|off>` | Hold (or let go of) the revive key, E. |

These are for trying things out and for test scripts, and work when hosting or offline (not
yet for whoever runs an online game):

| Command | |
|---|---|
| `give_card <name>` | Take a card now, if it could be offered (`give_card Twin Belt`). |
| `zombie [walker\|runner\|brute\|crawler\|bloater\|leaper] [x z]` | A zombie now, 6 m in front of you or at x z, going for the nearest player. |
| `next_wave` | End the breather now. |
| `end_wave` | Clear the wave's zombies now. |

Probe values for scripts: `survival` (`none`, `breather`, `wave`, `over`), `wave`, `zombies`
(left this wave), `cracks` (open), `fallen` (0 up, 1 down, 2 bled out), `bleed`, `offer`,
`cards`, `card_names`, `owned`, `spare` (spare rounds for the gun in hand, -1 unlimited),
`helping` (a downed teammate is in reach), `team_down`, `team_out`, `zombie_near` (meters),
`burning`, `chilled`, `frozen`, `crawling`, `leaping`, `tells` (zombies crouched to leap),
and `arcs`, `bursts`, `fires`, `heals`, `gases` (effects seen so far).

## Where it lives

The mode is contained: the rest of the game knows nothing of zombies or cards.

| | |
|---|---|
| `src/server/survival/` | The rules: `mod.rs` (the run, the mode switch), `loadout.rs` (what a player's cards give them: weapons, rounds, mods, and the `Build` the other systems read), `commands.rs` (the test commands), `waves.rs` (phases and sizes, unit tested), `zombies.rs` (the kinds, spawning, walking, attacking), `kinds.rs` (the leaper's leap, the bloater's burst and gas), `status.rs` (burning, chill, frozen), `effects.rs` (ammo, combos, perks on a hit), `downed.rs`, `drops.rs`, `draft.rs` |
| `src/survival/` | Shared and client: `cards.rs` (the card table and what a hand adds up to, unit tested), `cracks.rs` (which breaches the cracks are, the routes, the doors, opening breaches), `protocol.rs` (wire types), `looks.rs` (each kind's colors, parts and eyes), `sounds.rs` (what plays when), `hud.rs`, `scene.rs` (broken breaches, pickups, auras, flashes, gas), `mod.rs` (keys, limits, probe) |
| `src/config.rs` | `SURVIVAL_*`: wave size, breather lengths, drop rates, bleed-out, revive |

It uses the core's general hooks, which any mode can: `Rules` (respawns, friendly fire,
dummies, player ragdolls), `ModeChange` and `Died`, `Kit` (owned weapons and finite ammo),
`WeaponMods` (damage, magazine, reload, pierce, pellets, full auto...), `Mob` and `mob_look`
(enemies drawn with the player skeleton), `HitLanded` and `Boom` (react to hits and blasts),
`blast()`, `MoveLimits` (speed, jumping, kneeling, pose flags), `MenuKeys` and
`Arsenal::locked`.

## Add a card

1. Add a line to `CARDS` in `src/survival/cards.rs` with a name, a line of text, a rarity and
   an `Effect`. A new perk is a new `Perk` variant; new ammo, a new `Ammo` variant (and a
   `Combo` if it pairs with one).
2. Say when it may be offered in `Build::eligible`.
3. Make it do something: what it changes about weapons goes in `mods_of`
   (`src/server/survival/loadout.rs`); what happens on a hit goes in `effects.rs`; anything else
   reads the player's `Build` where it applies (`downed.rs` for Rally, `drops.rs` for Scavenger).
4. Try it with `give_card`, and add a part to `tests/harness/survival_cards.cfg`.

## What the tests can't tell you

The scenarios (`survival.cfg`, `survival_down.cfg`, `survival_cards.cfg`,
`survival_kinds.cfg` and the `survival_coop_*` pair) check that waves, cracks, going down, reviving, bleeding out, the
draft and the ammo effects work. They can't tell you whether the difficulty curve is fun,
whether the cards are balanced, or how a horde feels and sounds with a real mouse.
