//! Procedural sound effects, synthesized into WAV buffers at startup so the
//! game needs no audio files.

use std::{f32::consts::TAU, sync::Arc};

use bevy::{audio::Volume, prelude::*};

const RATE: u32 = 44_100;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PreStartup, make_sounds);
}

#[derive(Resource)]
pub struct Sounds {
    pub shot: Handle<AudioSource>,
    pub hit: Handle<AudioSource>,
    pub kill: Handle<AudioSource>,
    pub click: Handle<AudioSource>,
    pub reload: Handle<AudioSource>,
}

pub fn play(commands: &mut Commands, sound: &Handle<AudioSource>, volume: f32) {
    commands.spawn((
        AudioPlayer::new(sound.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
    ));
}

fn make_sounds(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut add = |samples: Vec<f32>| {
        sources.add(AudioSource {
            bytes: Arc::from(wav(&samples)),
        })
    };
    commands.insert_resource(Sounds {
        shot: add(shot()),
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
    });
}

struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn samples(secs: f32) -> usize {
    (secs * RATE as f32) as usize
}

/// A punchy rifle crack: filtered noise burst plus a falling low thump.
fn shot() -> Vec<f32> {
    let mut noise = Noise(0x1234_5678);
    let mut low = 0.0;
    let mut phase = 0.0;
    (0..samples(0.34))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let n = noise.next();
            low += (n - low) * 0.18;
            let crack = n * (-t * 60.0).exp() * 0.7;
            let body = low * (-t * 11.0).exp() * 1.6;
            let freq = 140.0 * (-t * 9.0).exp() + 45.0;
            phase += freq / RATE as f32 * TAU;
            let thump = phase.sin() * (-t * 16.0).exp() * 0.8;
            ((crack + body + thump) * 0.8).clamp(-1.0, 1.0)
        })
        .collect()
}

fn tone(notes: &[(f32, f32)], decay: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for &(freq, secs) in notes {
        for i in 0..samples(secs) {
            let t = i as f32 / RATE as f32;
            let env = (-t * decay).exp() * (t * 800.0).min(1.0);
            out.push(((t * freq * TAU).sin() + (t * freq * 2.0 * TAU).sin() * 0.25) * env * 0.5);
        }
    }
    out
}

fn click(secs: f32) -> Vec<f32> {
    let mut noise = Noise(0xBEEF);
    let mut last = 0.0;
    (0..samples(secs))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let n = noise.next();
            let high = n - last;
            last = n;
            high * (-t * 180.0).exp() * 0.6
        })
        .collect()
}

fn silence(secs: f32) -> Vec<f32> {
    vec![0.0; samples(secs)]
}

/// 16-bit mono PCM WAV.
fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}
