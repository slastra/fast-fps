//! Collision layers: who can touch whom.

use avian3d::prelude::*;

/// The local character controller only collides with `World`, so other
/// players never block movement. Hit scans test `World | Hitbox`.
#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum Layer {
    /// Static level geometry.
    #[default]
    World,
    /// The character controller you are driving on this machine.
    LocalBody,
    /// Server-side player and dummy hitboxes.
    Hitbox,
    /// Corpses: collide with the world only, and never stop bullets or players.
    Ragdoll,
    /// Grenades in flight: bounce off the world only.
    Projectile,
    /// Stops the local player and nothing else: an opened breach in the
    /// outer wall, which zombies and shots go through.
    Barrier,
}
