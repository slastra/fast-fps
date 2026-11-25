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

// --- Corpses ---------------------------------------------------------------

pub const MAX_RAGDOLLS: usize = 6;
/// Seconds before a corpse fades away.
pub const RAGDOLL_SECS: f32 = 12.0;

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
