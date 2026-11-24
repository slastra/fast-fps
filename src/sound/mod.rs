//! Sound effects. Most are synthesized into WAV buffers at startup (see
//! [`synth`]). The knife and grenade sounds are recordings generated with an
//! AI model by `scripts/sounds.py` and built into the binary from
//! `assets/sounds/`, so the game still loads no audio files at run time.
//!
//! To add a sound: add a field to [`Sounds`], build or embed it in
//! `make_sounds`, and call [`play`] where it should be heard.

mod synth;

use std::sync::Arc;

use bevy::{audio::Volume, prelude::*};

use crate::weapon::WeaponKind;

use crate::sound::synth::{Gun, click, shot, silence, tone, wav, with_action};

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
    let knife = load(recorded!("knife"));
    let throw = load(recorded!("throw"));
    let knife_flesh = load(recorded!("flesh"));
    let knife_wall = load(recorded!("wall"));
    let bounce = load(recorded!("bounce"));
    let explode = load(recorded!("explode"));
    let pin = load(recorded!("pin"));

    let mut add = |samples: Vec<f32>| {
        sources.add(AudioSource {
            bytes: Arc::from(wav(&samples)),
        })
    };
    commands.insert_resource(Sounds {
        shots: [
            // AK: heavy crack and a deep thump.
            add(shot(Gun {
                crack: 0.7,
                body: 1.6,
                thump_hz: 140.0,
                thump: 0.8,
                decay: 11.0,
                secs: 0.34,
                seed: 1,
            })),
            // MP5: lighter and quicker.
            add(shot(Gun {
                crack: 0.55,
                body: 1.0,
                thump_hz: 190.0,
                thump: 0.45,
                decay: 18.0,
                secs: 0.22,
                seed: 2,
            })),
            // Glock: snappy.
            add(shot(Gun {
                crack: 0.8,
                body: 0.8,
                thump_hz: 230.0,
                thump: 0.4,
                decay: 22.0,
                secs: 0.2,
                seed: 3,
            })),
            // Shotgun: a long boom, then the pump.
            add(with_action(
                shot(Gun {
                    crack: 0.6,
                    body: 2.2,
                    thump_hz: 90.0,
                    thump: 1.0,
                    decay: 6.5,
                    secs: 0.6,
                    seed: 4,
                }),
                &[(0.32, 0.05), (0.52, 0.06)],
            )),
            // Sniper: a big crack with a rolling tail, then the bolt.
            add(with_action(
                shot(Gun {
                    crack: 1.0,
                    body: 2.4,
                    thump_hz: 110.0,
                    thump: 1.0,
                    decay: 4.5,
                    secs: 0.9,
                    seed: 5,
                }),
                &[(0.62, 0.04), (0.78, 0.05), (0.98, 0.04)],
            )),
            // Knife: a quick swish through the air.
            knife,
            // Grenade: the arm's whoosh as it leaves the hand.
            throw,
        ],
        zoom: add(click(0.015)),
        hit: add(tone(&[(1900.0, 0.045)], 70.0)),
        kill: add(tone(&[(880.0, 0.07), (1320.0, 0.16)], 18.0)),
        click: add(click(0.025)),
        reload: add([
            click(0.03),
            silence(0.35),
            click(0.04),
            silence(0.5),
            click(0.03),
        ]
        .concat()),
        knife_flesh,
        knife_wall,
        bounce,
        explode,
        pin,
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
