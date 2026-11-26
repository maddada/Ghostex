//! Desktop audio on macOS: system audio from ScreenCaptureKit (no loopback driver
//! needed) -> Opus (10 ms frames) -> clients, while any client wants audio.

use super::ffi::*;
use super::stream::{self, OutputType};
use crate::hub::{Hub, Out};
use bytes::Bytes;
use parking_lot::Mutex;
use std::ffi::{c_int, c_void};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

#[link(name = "opus")]
extern "C" {
    fn opus_encoder_create(
        fs: i32,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut c_void;
    fn opus_encoder_ctl(st: *mut c_void, request: c_int, ...) -> c_int;
    fn opus_encode(
        st: *mut c_void,
        pcm: *const i16,
        frame_size: c_int,
        data: *mut u8,
        max: i32,
    ) -> i32;
}

const RATE: u32 = 48000;
const FRAME: usize = 480;

struct Encoder {
    enc: *mut c_void,
    /// Interleaved stereo samples waiting for a whole frame.
    pending: Vec<i16>,
    out: Vec<u8>,
}
unsafe impl Send for Encoder {}

impl Encoder {
    fn push(&mut self, hub: &Hub, l: &[f32], r: &[f32]) {
        for (&a, &b) in l.iter().zip(r) {
            self.pending.push((a.clamp(-1.0, 1.0) * 32767.0) as i16);
            self.pending.push((b.clamp(-1.0, 1.0) * 32767.0) as i16);
        }
        while self.pending.len() >= FRAME * 2 {
            let pcm: Vec<i16> = self.pending.drain(..FRAME * 2).collect();
            if pcm.iter().all(|&s| s == 0) {
                continue;
            }
            let n = unsafe {
                opus_encode(
                    self.enc,
                    pcm.as_ptr(),
                    FRAME as c_int,
                    self.out.as_mut_ptr(),
                    self.out.len() as i32,
                )
            };
            if n <= 0 {
                continue;
            }
            let mut m = Vec::with_capacity(n as usize + 1);
            m.push(0x02);
            m.extend_from_slice(&self.out[..n as usize]);
            let b = Bytes::from(m);
            for c in hub.clients.lock().iter() {
                if c.audio.load(Ordering::Relaxed) {
                    c.send(Out::Bin(b.clone()));
                }
            }
        }
    }
}

/// Planar or interleaved float samples of one audio sample buffer as (left, right).
unsafe fn samples(sb: CMSampleBufferRef) -> Option<(Vec<f32>, Vec<f32>)> {
    let asbd =
        CMAudioFormatDescriptionGetStreamBasicDescription(CMSampleBufferGetFormatDescription(sb))
            .as_ref()?;
    if asbd.mFormatFlags & kAudioFormatFlagIsFloat == 0 || asbd.mBitsPerChannel != 32 {
        return None;
    }
    // Ask how big the buffer list must be, then fill one that size.
    let mut need = 0usize;
    CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        sb,
        &mut need,
        std::ptr::null_mut(),
        0,
        std::ptr::null(),
        std::ptr::null(),
        kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
        std::ptr::null_mut(),
    );
    if need == 0 || need > std::mem::size_of::<AudioBufferList8>() {
        return None;
    }
    let mut list: AudioBufferList8 = std::mem::zeroed();
    let mut block: CFTypeRef = std::ptr::null();
    let st = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        sb,
        std::ptr::null_mut(),
        &mut list,
        need,
        std::ptr::null(),
        std::ptr::null(),
        kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
        &mut block,
    );
    if st != 0 {
        return None;
    }
    let buf = |i: usize| {
        let b = list.mBuffers[i];
        std::slice::from_raw_parts(b.mData as *const f32, b.mDataByteSize as usize / 4)
    };
    let r = if list.mNumberBuffers >= 2 {
        Some((buf(0).to_vec(), buf(1).to_vec()))
    } else if list.mNumberBuffers == 1 {
        let ch = asbd.mChannelsPerFrame.max(1) as usize;
        let s = buf(0);
        let l: Vec<f32> = s.iter().step_by(ch).copied().collect();
        let r: Vec<f32> = if ch > 1 {
            s.iter().skip(1).step_by(ch).copied().collect()
        } else {
            l.clone()
        };
        Some((l, r))
    } else {
        None
    };
    if !block.is_null() {
        CFRelease(block);
    }
    r
}

pub fn spawn(_device: String, bitrate: i32, hub: Arc<Hub>) {
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let mut err = 0;
            let enc = unsafe { opus_encoder_create(RATE as i32, 2, 2051, &mut err) };
            if enc.is_null() {
                log::error!("opus init failed: {err}");
                return;
            }
            unsafe { opus_encoder_ctl(enc, 4002, bitrate as c_int) };
            let encoder = Arc::new(Mutex::new(Encoder {
                enc,
                pending: Vec::with_capacity(FRAME * 4),
                out: vec![0u8; 4000],
            }));
            let want = || {
                hub.clients
                    .lock()
                    .iter()
                    .any(|c| c.audio.load(Ordering::Relaxed))
            };
            loop {
                if !want() {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
                let (e, h) = (encoder.clone(), hub.clone());
                let on_sample = move |sb: CMSampleBufferRef, ty: OutputType| {
                    if ty != OutputType::Audio {
                        return;
                    }
                    if let Some((l, r)) = unsafe { samples(sb) } {
                        e.lock().push(&h, &l, &r);
                    }
                };
                // Audio rides on a display stream; keep its video as small and rare as possible.
                let s = stream::start(
                    super::display(),
                    "audio",
                    &[OutputType::Audio],
                    |cfg| unsafe {
                        cfg.setCapturesAudio(true);
                        cfg.setSampleRate(RATE as isize);
                        cfg.setChannelCount(2);
                        // voxl plays no audio itself, but a viewer on this Mac would loop.
                        cfg.setExcludesCurrentProcessAudio(true);
                        cfg.setWidth(2);
                        cfg.setHeight(2);
                        cfg.setMinimumFrameInterval(objc2_core_media::CMTime {
                            value: 1,
                            timescale: 1,
                            flags: objc2_core_media::CMTimeFlags::Valid,
                            epoch: 0,
                        });
                    },
                    on_sample,
                );
                let s = match s {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!("audio capture failed ({e}); retrying");
                        std::thread::sleep(Duration::from_secs(2));
                        continue;
                    }
                };
                log::info!("audio capture started");
                while want() && !s.stopped.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(200));
                }
                drop(s);
                encoder.lock().pending.clear();
                log::info!("audio capture stopped");
            }
        })
        .unwrap();
}
