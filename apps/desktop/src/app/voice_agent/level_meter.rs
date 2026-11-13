//! How loud the microphone is while Dictate records, for the waveform in the chat box's Dictate
//! pill (`native_chat/dictation.rs`). The recorder's writer thread hands it every captured chunk
//! (`recorder.rs` `write_samples`); each 50 ms of audio becomes a level from 0 to 1 that rises fast
//! and falls a little slower, so brief peaks read without flicker. The phone computes the same
//! level from the file its recorder writes (`src/voice/dictationLevel.ts` in the phone app).
//!
//! CDXC:Dictate 2026-11-13 DECISION: User, after a dictation they couldn't tell was being heard: "we need much better indicator that shows the audio I'm saying is reaching and being recorded, like a waveform", on the desktop and the phone, drawn the way their reference app draws its dictation waveform.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

const SAMPLE_RATE: f64 = 24_000.0;
/// 50 ms of 24 kHz audio: one level each.
const WINDOW: u32 = 1_200;
/// Speech sits far below full scale; this brings ordinary speaking to the upper half.
const GAIN: f64 = 4.0;
const RISE_SECONDS: f64 = 0.075;
const FALL_SECONDS: f64 = 0.2;

/// The level as the pill reads it, shared between the writer thread and the UI.
#[derive(Clone, Default)]
pub(crate) struct SharedLevel(Arc<AtomicU32>);

impl SharedLevel {
    pub(crate) fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn set(&self, level: f32) {
        self.0.store(level.to_bits(), Ordering::Relaxed);
    }
}

/// Turns captured samples into [`SharedLevel`] updates.
pub(crate) struct LevelMeter {
    shared: SharedLevel,
    energy: f64,
    count: u32,
    level: f64,
}

impl LevelMeter {
    pub(crate) fn new(shared: SharedLevel) -> Self {
        Self {
            shared,
            energy: 0.0,
            count: 0,
            level: 0.0,
        }
    }

    pub(crate) fn push(&mut self, samples: &[i16]) {
        for &sample in samples {
            let value = f64::from(sample) / 32_768.0;
            self.energy += value * value;
            self.count += 1;
            if self.count < WINDOW {
                continue;
            }
            let target = ((self.energy / f64::from(self.count)).sqrt() * GAIN).min(1.0);
            let time_constant = if target > self.level {
                RISE_SECONDS
            } else {
                FALL_SECONDS
            };
            let decay = (-(f64::from(self.count) / SAMPLE_RATE) / time_constant).exp();
            self.level = target + (self.level - target) * decay;
            self.shared.set(self.level as f32);
            self.energy = 0.0;
            self.count = 0;
        }
    }
}
