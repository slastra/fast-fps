//! A tiny synthesizer: noise, tones, clicks, gunshots, and a WAV encoder.

use std::f32::consts::TAU;

use bevy::prelude::*;

const RATE: u32 = 44_100;

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

pub struct Gun {
    pub crack: f32,
    pub body: f32,
    pub thump_hz: f32,
    pub thump: f32,
    pub decay: f32,
    pub secs: f32,
    pub seed: u32,
}

/// A gunshot: a filtered noise crack, a noise body, and a falling low thump.
pub fn shot(g: Gun) -> Vec<f32> {
    let mut noise = Noise(0x1234_5678 ^ g.seed.wrapping_mul(0x9E37_79B9));
    let mut low = 0.0;
    let mut phase = 0.0;
    (0..samples(g.secs))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let n = noise.next();
            low += (n - low) * 0.18;
            let crack = n * (-t * 60.0).exp() * g.crack;
            let body = low * (-t * g.decay).exp() * g.body;
            let freq = g.thump_hz * (-t * 9.0).exp() + 45.0;
            phase += freq / RATE as f32 * TAU;
            let thump = phase.sin() * (-t * 16.0).exp() * g.thump;
            ((crack + body + thump) * 0.8).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Mix mechanical clicks (start seconds, length) into a shot: pump or bolt.
pub fn with_action(mut samples_in: Vec<f32>, clicks: &[(f32, f32)]) -> Vec<f32> {
    let end = clicks
        .iter()
        .map(|(at, len)| samples(at + len))
        .max()
        .unwrap_or(0);
    if samples_in.len() < end {
        samples_in.resize(end, 0.0);
    }
    for &(at, len) in clicks {
        let start = samples(at);
        for (i, v) in click(len).into_iter().enumerate() {
            samples_in[start + i] += v * 1.4;
        }
    }
    samples_in
}

pub fn tone(notes: &[(f32, f32)], decay: f32) -> Vec<f32> {
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

pub fn click(secs: f32) -> Vec<f32> {
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

pub fn silence(secs: f32) -> Vec<f32> {
    vec![0.0; samples(secs)]
}

/// 16-bit mono PCM WAV.
pub fn wav(samples: &[f32]) -> Vec<u8> {
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
