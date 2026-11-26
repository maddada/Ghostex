//! Microphone sink on Windows. Windows has no built-in virtual microphone, so the
//! browser's microphone is played into the output side of a virtual audio cable, such
//! as VB-CABLE ("CABLE Input"); apps then record from its input ("CABLE Output"). The
//! device's clock paces playback: WASAPI asks for audio as its buffer drains, and gets
//! 20 ms frames from the same jitter queue as the Linux FIFO writer.

use super::audio::{com_init, device, render_devices, wave_format};
use crate::mic::{next_frame, Queue, FRAME};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::Media::Audio::{
    IAudioClient, IAudioRenderClient, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
};
use windows::Win32::System::Com::CLSCTX_ALL;

/// Names of virtual cables picked when --microphone-device isn't given.
const LOOPBACK: &[&str] = &[
    "CABLE Input",
    "VB-Audio",
    "Voicemeeter Input",
    "Virtual Audio",
];

/// The ID of the output device to play the microphone into: the one named (exactly
/// or by prefix), else the first virtual cable found. None turns the mic off.
pub fn find_device(name: Option<&str>) -> Option<String> {
    com_init();
    let devs = render_devices();
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
        Some((d, id)) => {
            log::info!("microphone: playing into \"{d}\"; apps record from its input");
            Some(id.clone())
        }
        None => {
            log::warn!(
                "microphone off: no {} (install a virtual audio cable such as VB-CABLE, or \
                 pick one with --microphone-device). Output devices: {}",
                name.map(|n| format!("output device \"{n}\""))
                    .unwrap_or_else(|| "virtual cable output device".into()),
                devs.iter()
                    .map(|d| format!("\"{}\"", d.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            None
        }
    }
}

/// Plays into the device until it fails.
fn run(id: &str, queue: &Mutex<Queue>, frame: &mut [i16]) -> Result<(), String> {
    let e = |e: windows::core::Error| e.to_string();
    unsafe {
        let client: IAudioClient = device(Some(id))?.Activate(CLSCTX_ALL, None).map_err(e)?;
        let fmt = wave_format(1, false);
        // 60 ms of buffer; kept 40-60 ms full.
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                600_000,
                0,
                &fmt,
                None,
            )
            .map_err(e)?;
        let size = client.GetBufferSize().map_err(e)? as usize;
        let render: IAudioRenderClient = client.GetService().map_err(e)?;
        let mut started = false;
        loop {
            let free = size - client.GetCurrentPadding().map_err(e)? as usize;
            if free >= FRAME {
                let p = render.GetBuffer(FRAME as u32).map_err(e)?;
                next_frame(queue, frame);
                std::ptr::copy_nonoverlapping(frame.as_ptr(), p as *mut i16, FRAME);
                render.ReleaseBuffer(FRAME as u32, 0).map_err(e)?;
                continue;
            }
            if !started {
                client.Start().map_err(e)?;
                started = true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// Plays the microphone queue into the device with ID `id`, forever.
pub fn play(id: &str, queue: Arc<Mutex<Queue>>) {
    com_init();
    let mut frame = vec![0i16; FRAME];
    loop {
        if let Err(e) = run(id, &queue, &mut frame) {
            log::warn!("microphone: playback failed ({e}); retrying");
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
