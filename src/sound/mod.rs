//! Sound effects. Every sound is a recording generated with an AI model
//! (ByteDance Seed Audio) by `scripts/sounds.py`, trimmed and leveled into
//! `assets/sounds/`, and built into the binary, so the game loads no audio
//! files at run time.
//!
//! To add a sound: make it with `scripts/sounds.py`, add a field to
//! [`Sounds`], load it in `make_sounds`, and call [`play`] where it should be
//! heard.

use std::sync::Arc;

use bevy::{audio::Volume, prelude::*};

use crate::weapon::WeaponKind;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PreStartup, make_sounds);
}

#[derive(Resource)]
pub struct Sounds {
    /// One per weapon, indexed by `WeaponKind::index`.
    pub shots: [Handle<AudioSource>; WeaponKind::COUNT],
    pub zoom: Handle<AudioSource>,
    pub hit: Handle<AudioSource>,
    pub kill: Handle<AudioSource>,
    pub click: Handle<AudioSource>,
    pub reload: Handle<AudioSource>,
    /// The knife meeting a body, and meeting a wall.
    pub knife_flesh: Handle<AudioSource>,
    pub knife_wall: Handle<AudioSource>,
    /// A grenade hitting the ground, and going off.
    pub bounce: Handle<AudioSource>,
    pub explode: Handle<AudioSource>,
    /// Pulling a grenade's pin.
    pub pin: Handle<AudioSource>,
    /// The flintlock's spark lighting the pan, and its ramrod reload.
    pub flint_pan: Handle<AudioSource>,
    pub flint_reload: Handle<AudioSource>,
    /// A duelist reaching their mark, and the bell for "Draw!".
    pub duel_mark: Handle<AudioSource>,
    pub duel_draw: Handle<AudioSource>,
    /// Survival (`survival/sounds.rs`).
    pub survival: SurvivalSounds,
}

/// Survival's sounds: the zombies, the run, the player, the special ammo.
pub struct SurvivalSounds {
    pub groan: Handle<AudioSource>,
    pub crawl: Handle<AudioSource>,
    /// A leaper crouching to leap.
    pub screech: Handle<AudioSource>,
    /// A bloater bursting.
    pub bloat: Handle<AudioSource>,
    /// A zombie's blow landing on you.
    pub claw: Handle<AudioSource>,
    /// A crack breaking open, a wave starting, a wave cleared.
    pub breach: Handle<AudioSource>,
    pub horn: Handle<AudioSource>,
    pub cleared: Handle<AudioSource>,
    pub card: Handle<AudioSource>,
    pub downed: Handle<AudioSource>,
    pub revive: Handle<AudioSource>,
    pub ammo: Handle<AudioSource>,
    pub ignite: Handle<AudioSource>,
    pub freeze: Handle<AudioSource>,
    pub zap: Handle<AudioSource>,
}

/// How loud a sound at `distance` meters is: `volume` up close, fading to
/// nothing at `range`.
pub fn faded(volume: f32, distance: f32, range: f32) -> f32 {
    (1.0 - distance / range).clamp(0.0, 1.0) * volume
}

pub fn play(commands: &mut Commands, sound: &Handle<AudioSource>, volume: f32) {
    commands.spawn((
        AudioPlayer::new(sound.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
    ));
}

/// A WAV file from `assets/sounds/`, embedded in the binary.
macro_rules! recorded {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/sounds/",
            $name,
            ".wav"
        ))
    };
}

fn make_sounds(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut load = |bytes: &'static [u8]| {
        sources.add(AudioSource {
            bytes: Arc::from(bytes),
        })
    };
    commands.insert_resource(Sounds {
        // In `WeaponKind::ALL` order. The shotgun's pump and the sniper's
        // bolt are mixed into their shots (`pump.wav`, `bolt.wav`).
        shots: [
            load(recorded!("ak")),
            load(recorded!("mp5")),
            load(recorded!("glock")),
            load(recorded!("shotgun")),
            load(recorded!("sniper")),
            // The knife's swish, and the grenade leaving the hand.
            load(recorded!("knife")),
            load(recorded!("throw")),
            load(recorded!("flintlock")),
        ],
        zoom: load(recorded!("zoom")),
        hit: load(recorded!("hit")),
        kill: load(recorded!("kill")),
        click: load(recorded!("click")),
        reload: load(recorded!("reload")),
        knife_flesh: load(recorded!("flesh")),
        knife_wall: load(recorded!("wall")),
        bounce: load(recorded!("bounce")),
        explode: load(recorded!("explode")),
        pin: load(recorded!("pin")),
        flint_pan: load(recorded!("flint_pan")),
        flint_reload: load(recorded!("flint_reload")),
        duel_mark: load(recorded!("duel_mark")),
        duel_draw: load(recorded!("duel_draw")),
        survival: SurvivalSounds {
            groan: load(recorded!("groan")),
            crawl: load(recorded!("crawl")),
            screech: load(recorded!("screech")),
            bloat: load(recorded!("bloat")),
            claw: load(recorded!("claw")),
            breach: load(recorded!("breach")),
            horn: load(recorded!("horn")),
            cleared: load(recorded!("cleared")),
            card: load(recorded!("card")),
            downed: load(recorded!("downed")),
            revive: load(recorded!("revive")),
            ammo: load(recorded!("ammo")),
            ignite: load(recorded!("ignite")),
            freeze: load(recorded!("freeze")),
            zap: load(recorded!("zap")),
        },
    });
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    /// Write every sound to `target/sounds/all/` to listen to:
    /// `cargo test dump_sounds -- --ignored`.
    #[test]
    #[ignore]
    fn dump_sounds() {
        let mut world = World::new();
        world.init_resource::<Assets<AudioSource>>();
        world.run_system_once(make_sounds).unwrap();
        let sounds = world.resource::<Sounds>();
        let mut named: Vec<(String, &Handle<AudioSource>)> = WeaponKind::ALL
            .iter()
            .map(|kind| {
                (
                    format!("shot-{}", kind.def().name.to_lowercase()),
                    &sounds.shots[kind.index()],
                )
            })
            .collect();
        named.extend([
            ("zoom".into(), &sounds.zoom),
            ("hit".into(), &sounds.hit),
            ("kill".into(), &sounds.kill),
            ("click".into(), &sounds.click),
            ("reload".into(), &sounds.reload),
            ("knife_flesh".into(), &sounds.knife_flesh),
            ("knife_wall".into(), &sounds.knife_wall),
            ("bounce".into(), &sounds.bounce),
            ("explode".into(), &sounds.explode),
            ("pin".into(), &sounds.pin),
            ("flint_pan".into(), &sounds.flint_pan),
            ("flint_reload".into(), &sounds.flint_reload),
            ("duel_mark".into(), &sounds.duel_mark),
            ("duel_draw".into(), &sounds.duel_draw),
        ]);
        let s = &sounds.survival;
        named.extend([
            ("groan".into(), &s.groan),
            ("crawl".into(), &s.crawl),
            ("screech".into(), &s.screech),
            ("bloat".into(), &s.bloat),
            ("claw".into(), &s.claw),
            ("breach".into(), &s.breach),
            ("horn".into(), &s.horn),
            ("cleared".into(), &s.cleared),
            ("card".into(), &s.card),
            ("downed".into(), &s.downed),
            ("revive".into(), &s.revive),
            ("ammo".into(), &s.ammo),
            ("ignite".into(), &s.ignite),
            ("freeze".into(), &s.freeze),
            ("zap".into(), &s.zap),
        ]);
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/sounds/all");
        std::fs::create_dir_all(&out).unwrap();
        let sources = world.resource::<Assets<AudioSource>>();
        for (name, handle) in named {
            let bytes = &sources.get(handle).unwrap().bytes;
            std::fs::write(out.join(format!("{name}.wav")), bytes).unwrap();
        }
    }
}
