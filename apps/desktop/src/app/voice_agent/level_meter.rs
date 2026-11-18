//! How loud the microphone is while Dictate records, for the waveform in the chat box's Dictate
//! pill (`native_chat/dictation.rs`). The recorder's writer thread hands it every captured chunk
//! (`recorder.rs` `write_samples`); each 50 ms of audio becomes a level from 0 to 1 (its loudness
//! on a dB scale) that rises fast and falls a little slower, so brief peaks read without flicker.
//! The phone computes the same level (`src/voice/dictationLevel.ts` in the phone app).
//!
//! CDXC:Dictate 2026-11-13 DECISION: User, after a dictation they couldn't tell was being heard: "we need much better indicator that shows the audio I'm saying is reaching and being recorded, like a waveform", on the desktop and the phone, drawn the way their reference app draws its dictation waveform.
//!
//! CDXC:Dictate 2026-11-18 DECISION: User: "the voice waveform doesn't react much to me speaking. Please make it react more to my voice please. Also same on pc please." The level was the window's RMS × 4, so ordinary speech (-35 to -20 dBFS) sat at 0.07 to 0.4 and mostly under the resting bar. It is now the window's dBFS between [`FLOOR_DBFS`] (rest) and [`CEILING_DBFS`] (full), rising in 30 ms and falling in 120 ms so each word rises and the gaps between words fall back, the same on the phone.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

const SAMPLE_RATE: f64 = 24_000.0;
/// 50 ms of 24 kHz audio: one level each.
const WINDOW: u32 = 1_200;
/// A window this quiet or quieter shows a resting bar (a room's hum sits around -55 to -45 dBFS).
const FLOOR_DBFS: f64 = -50.0;
/// A window this loud or louder fills the bar (loud speech close to the microphone).
const CEILING_DBFS: f64 = -10.0;
const RISE_SECONDS: f64 = 0.03;
const FALL_SECONDS: f64 = 0.12;

/// How full a bar is for a window whose RMS is `rms` (full scale 1): its dBFS between
/// [`FLOOR_DBFS`] and [`CEILING_DBFS`].
pub(crate) fn level_for_loudness(rms: f64) -> f64 {
    if !(rms > 0.0) {
        return 0.0;
    }
    let dbfs = 20.0 * rms.log10();
    ((dbfs - FLOOR_DBFS) / (CEILING_DBFS - FLOOR_DBFS)).clamp(0.0, 1.0)
}

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
            let target = level_for_loudness((self.energy / f64::from(self.count)).sqrt());
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
