//! Microphone sink on macOS. macOS has no built-in virtual microphone, so the browser's
//! microphone is played into an output device that loops back to an input of the same
//! name, such as BlackHole (`brew install blackhole-2ch`); apps then record from that
//! input. The device's clock paces playback through an AudioQueue, which takes 20 ms
//! frames from the same jitter queue as the Linux FIFO writer.

use super::ffi::*;
use crate::mic::{next_frame, Queue, FRAME, RATE};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::sync::Arc;

/// Names of loopback devices picked when --microphone-device isn't given.
const LOOPBACK: &[&str] = &["BlackHole", "Loopback", "VB-Cable", "Soundflower"];

/// Output devices as (name, UID).
fn output_devices() -> Vec<(String, String)> {
    let mut out = Vec::new();
    unsafe {
        let addr = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut size = 0u32;
        if AudioObjectGetPropertyDataSize(
            kAudioObjectSystemObject,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
        ) != 0
        {
            return out;
        }
        let mut ids = vec![0u32; size as usize / 4];
        if AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            ids.as_mut_ptr() as *mut c_void,
        ) != 0
        {
            return out;
        }
        for id in ids {
            let streams = AudioObjectPropertyAddress {
                mSelector: kAudioDevicePropertyStreams,
                mScope: kAudioObjectPropertyScopeOutput,
                mElement: kAudioObjectPropertyElementMain,
            };
            let mut n = 0u32;
            if AudioObjectGetPropertyDataSize(id, &streams, 0, std::ptr::null(), &mut n) != 0
                || n == 0
            {
                continue;
            }
            let string = |sel: u32| {
                let a = AudioObjectPropertyAddress {
                    mSelector: sel,
                    mScope: kAudioObjectPropertyScopeGlobal,
                    mElement: kAudioObjectPropertyElementMain,
                };
                let mut s: CFStringRef = std::ptr::null();
                let mut size = std::mem::size_of::<CFStringRef>() as u32;
                if AudioObjectGetPropertyData(
                    id,
                    &a,
                    0,
                    std::ptr::null(),
                    &mut size,
                    &mut s as *mut _ as *mut c_void,
                ) != 0
                {
                    return String::new();
                }
                let r = cf_string(s);
                if !s.is_null() {
                    CFRelease(s);
                }
                r
            };
            out.push((
                string(kAudioObjectPropertyName),
                string(kAudioDevicePropertyDeviceUID),
            ));
        }
    }
    out
}

/// The UID of the output device to play the microphone into: the one named (exactly
/// or by prefix), else the first loopback device found. None turns the mic off.
pub fn find_device(name: Option<&str>) -> Option<String> {
    let devs = output_devices();
    let pick = match name {
        Some(n) => devs
            .iter()
            .find(|(d, _)| d == n)
            .or_else(|| devs.iter().find(|(d, _)| d.starts_with(n))),
        None => devs
            .iter()
            .find(|(d, _)| LOOPBACK.iter().any(|l| d.contains(l))),
    };
    match pick {
        Some((d, uid)) => {
            log::info!("microphone: playing into \"{d}\"; apps record from its input");
            Some(uid.clone())
        }
        None => {
            log::warn!(
                "microphone off: no {} (install a loopback device, e.g. `brew install blackhole-2ch`, \
                 or pick one with --microphone-device). Output devices: {}",
                name.map(|n| format!("output device \"{n}\""))
                    .unwrap_or_else(|| "loopback output device".into()),
                devs.iter().map(|d| format!("\"{}\"", d.0)).collect::<Vec<_>>().join(", ")
            );
            None
        }
    }
}

struct Ctx {
    queue: Arc<Mutex<Queue>>,
    frame: Vec<i16>,
}

extern "C" fn refill(user: *mut c_void, aq: AudioQueueRef, buf: *mut AudioQueueBuffer) {
    unsafe {
        let (Some(ctx), Some(b)) = ((user as *mut Ctx).as_mut(), buf.as_mut()) else {
            return;
        };
        if b.mAudioData.is_null() || (b.mAudioDataBytesCapacity as usize) < FRAME * 2 {
            return;
        }
        next_frame(&ctx.queue, &mut ctx.frame);
        std::ptr::copy_nonoverlapping(ctx.frame.as_ptr(), b.mAudioData as *mut i16, FRAME);
        b.mAudioDataByteSize = (FRAME * 2) as u32;
        AudioQueueEnqueueBuffer(aq, buf, 0, std::ptr::null());
    }
}

/// Plays the microphone queue into the device with UID `uid`, forever.
pub fn play(uid: &str, queue: Arc<Mutex<Queue>>) {
    let fmt = AudioStreamBasicDescription {
        mSampleRate: RATE as f64,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked,
        mBytesPerPacket: 2,
        mFramesPerPacket: 1,
        mBytesPerFrame: 2,
        mChannelsPerFrame: 1,
        mBitsPerChannel: 16,
        mReserved: 0,
    };
    // Lives as long as the queue, which runs until voxl exits.
    let ctx = Box::into_raw(Box::new(Ctx {
        queue,
        frame: vec![0; FRAME],
    }));
    unsafe {
        let mut aq: AudioQueueRef = std::ptr::null_mut();
        let st = AudioQueueNewOutput(
            &fmt,
            refill,
            ctx as *mut c_void,
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut aq,
        );
        if st != 0 {
            log::error!("microphone: AudioQueueNewOutput failed ({st})");
            drop(Box::from_raw(ctx));
            return;
        }
        let uid = objc2_core_foundation::CFString::from_str(uid);
        let uid_ref: CFStringRef = &*uid as *const _ as CFStringRef;
        let st = AudioQueueSetProperty(
            aq,
            kAudioQueueProperty_CurrentDevice,
            &uid_ref as *const _ as *const c_void,
            std::mem::size_of::<CFStringRef>() as u32,
        );
        if st != 0 {
            log::error!("microphone: cannot select the output device ({st})");
            AudioQueueDispose(aq, true);
            drop(Box::from_raw(ctx));
            return;
        }
        // Three 20 ms buffers: 40-60 ms between the jitter queue and the device.
        for _ in 0..3 {
            let mut b: *mut AudioQueueBuffer = std::ptr::null_mut();
            if AudioQueueAllocateBuffer(aq, (FRAME * 2) as u32, &mut b) == 0 && !b.is_null() {
                refill(ctx as *mut c_void, aq, b);
            }
        }
        let st = AudioQueueStart(aq, std::ptr::null());
        if st != 0 {
            log::error!("microphone: AudioQueueStart failed ({st})");
        }
    }
}
