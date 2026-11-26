//! The game's identity and the numbers you are most likely to tune.
//!
//! Two other places hold most of the remaining "content":
//! - weapons: `weapon/defs.rs`
//! - the map: `world/arena.rs`

use bevy::math::Vec3;

/// The game's name, taken from `Cargo.toml`. It names the window, the settings
/// folder and the LAN beacon, and seeds [`PROTOCOL_ID`], so renaming the
/// package is all it takes to make a copy of this project its own game.
pub const GAME_NAME: &str = env!("CARGO_PKG_NAME");

// --- Network ---------------------------------------------------------------

/// UDP port a host listens on unless `host <port>` says otherwise.
pub const DEFAULT_PORT: u16 = 27015;
/// UDP port for the "a game is hosted here" LAN broadcast.
pub const DISCOVERY_PORT: u16 = 27016;
pub const MAX_CLIENTS: usize = 16;
/// Peers must agree on this to connect. It is a hash of [`GAME_NAME`], so two
/// different games built from this project never join each other.
pub const PROTOCOL_ID: u64 = fnv1a(GAME_NAME);
/// Seconds a lobby's connect token stays valid. A game server also refuses
/// tokens older than this.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // no tokens in a browser yet
pub const CONNECT_TOKEN_SECS: u16 = 30;
/// The online lobby that lists games and starts new ones (`lobby/`).
/// Players can point at another with the `lobby` command.
pub const DEFAULT_LOBBY: &str = "https://fps.lastra.us";
/// How many times a second a dedicated server (`--dedicated`) runs its game
/// loop. A listen server runs at its frame rate instead.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // no server in a browser
pub const DEDICATED_TICK_HZ: f64 = 60.0;
/// A dedicated server with no players for this many seconds shuts down.
pub const IDLE_SECS: f32 = 120.0;

// --- Player ----------------------------------------------------------------

pub const MAX_HEALTH: f32 = 100.0;
/// Top running speed in m/s. Quake-fast by default.
pub const RUN_SPEED: f32 = 12.0;
/// Running speed with the knife out, as a multiple of [`RUN_SPEED`].
pub const KNIFE_SPEED_SCALE: f32 = 1.1;
/// Crouch-walking speed as a fraction of [`RUN_SPEED`].
pub const CROUCH_SPEED_SCALE: f32 = 1.0 / 3.0;
/// How high a jump goes, in meters.
pub const JUMP_HEIGHT: f32 = 1.8;
/// Downward acceleration in m/s².
pub const GRAVITY: f32 = 29.0;
/// Degrees turned per mouse count, until `sens` changes it.
pub const DEFAULT_SENS: f32 = 0.07;
/// Vertical field of view in degrees, until `fov` changes it.
pub const DEFAULT_FOV: f32 = 80.0;
/// Fall below this height and you respawn.
pub const KILL_HEIGHT: f32 = -15.0;

// --- Combat ----------------------------------------------------------------

/// How far a bullet travels, in meters.
pub const SHOT_RANGE: f32 = 250.0;
/// How long a practice dummy stays down.
pub const DUMMY_DOWN_SECS: f32 = 3.0;

// --- Duel (`mode duel`, see server/duel.rs) --------------------------------

/// Where each round starts: the duelists stand back to back here, in the
/// open yard south of the keep. Collider-center height, like `SPAWNS`.
pub const DUEL_START: Vec3 = Vec3::new(0.0, 1.2, 15.0);
/// The two marks the duelists walk to, one each way along the yard.
pub const DUEL_MARKS: [Vec3; 2] = [Vec3::new(0.0, 0.0, 9.0), Vec3::new(0.0, 0.0, 21.0)];
/// How close to the center of a mark counts as standing on it, in meters.
pub const DUEL_MARK_RADIUS: f32 = 0.9;
/// Seconds from both duelists reaching their marks to "Draw!", picked at
/// random in this range so nobody can count it.
pub const DUEL_STEADY_SECS: (f32, f32) = (1.5, 3.0);
/// After "Draw!", a round with no winner ends after this long.
pub const DUEL_ROUND_SECS: f32 = 45.0;
/// How long the round's result and the duel's result stay on screen.
pub const DUEL_ROUND_OVER_SECS: f32 = 3.0;
pub const DUEL_OVER_SECS: f32 = 5.0;
/// Where everyone who isn't dueling watches from: the yard's west edge.
pub const DUEL_WATCH: Vec3 = Vec3::new(-9.5, 1.2, 15.0);
/// From the bell until "Draw!" is over, a duelist can't leave their mark: the
/// body stays within this distance of its center, in meters, and can't jump.
/// That leaves room to lean aside, not to run.
pub const DUEL_ROOM: f32 = 0.6;
/// Seconds to bring the pistol up from the holster once you draw (Space).
pub const DUEL_RAISE_SECS: f32 = 0.5;
/// How far below your aim a shot goes if fired the moment you draw, in
/// degrees. It shrinks to nothing as the pistol comes up.
pub const DUEL_LOW_DEG: f32 = 25.0;

// --- Survival (`mode survival`, see server/survival/) ----------------------

/// Zombies in the first wave; each wave after brings 2 more.
pub const SURVIVAL_WAVE_START: u16 = 3;
/// Seconds between waves (the draft), and before the very first wave.
pub const SURVIVAL_BREATHER_SECS: f32 = 15.0;
pub const SURVIVAL_FIRST_BREATHER_SECS: f32 = 5.0;
/// Seconds the end of a run stays on screen before the next one starts.
pub const SURVIVAL_OVER_SECS: f32 = 10.0;
/// At most this many zombies at once; the rest wait at the cracks.
pub const SURVIVAL_MAX_ALIVE: usize = 24;
/// Seconds between zombies coming through the cracks.
pub const SURVIVAL_SPAWN_SECS: f32 = 1.1;
/// A downed player bleeds out after this long, unless revived.
pub const SURVIVAL_BLEED_SECS: f32 = 30.0;
/// Holding the revive key (E) this long, this close, brings them back.
pub const SURVIVAL_REVIVE_SECS: f32 = 3.0;
pub const SURVIVAL_REVIVE_RANGE: f32 = 1.8;
/// Health after a revive (the Rally perk gives more).
pub const SURVIVAL_REVIVE_HEALTH: f32 = 40.0;
/// Spare magazines a new gun comes with, and a run starts with for the Glock.
pub const SURVIVAL_START_MAGS: u16 = 3;
/// How often a killed zombie drops an ammo box, or a medkit (the Scavenger
/// perk raises both).
pub const SURVIVAL_AMMO_DROP: f32 = 0.22;
pub const SURVIVAL_MED_DROP: f32 = 0.0;
/// Walk within this of a pickup to take it; it lasts this long.
pub const SURVIVAL_PICKUP_RANGE: f32 = 1.3;
pub const SURVIVAL_PICKUP_SECS: f32 = 30.0;
/// Health a medkit gives back.
pub const SURVIVAL_MEDKIT: f32 = 35.0;
/// Where players start, and come back after bleeding out: inside the keep.
pub const SURVIVAL_KEEP: [Vec3; 4] = [
    Vec3::new(-2.0, 1.2, 2.5),
    Vec3::new(-2.0, 1.2, -2.5),
    Vec3::new(2.0, 1.2, 2.5),
    Vec3::new(2.0, 1.2, -2.5),
];

// --- Corpses ---------------------------------------------------------------

pub const MAX_RAGDOLLS: usize = 6;
/// Seconds before a corpse fades away.
pub const RAGDOLL_SECS: f32 = 12.0;
/// A killed mob (a game mode's enemy) stays this long before the server
/// removes it, so every client has made its ragdoll by then.
pub const MOB_CORPSE_SECS: f32 = 1.5;

/// FNV-1a, usable in a `const`.
const fn fnv1a(text: &str) -> u64 {
    let bytes = text.as_bytes();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    hash
}
