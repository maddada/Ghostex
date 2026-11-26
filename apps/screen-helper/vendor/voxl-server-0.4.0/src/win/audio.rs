//! Desktop audio on Windows: WASAPI loopback of an output device (what the speakers
//! play, no driver needed) -> Opus (10 ms frames) -> clients, while any client wants
//! audio.

use crate::hub::{Hub, Out};
use bytes::Bytes;
use std::ffi::{c_int, c_void};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE, WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};

// build.rs links the static Opus from vcpkg (a #[link] would ask for a DLL).
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
const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

struct Encoder {
    enc: *mut c_void,
    /// Interleaved stereo samples waiting for a whole frame.
    pending: Vec<i16>,
    out: Vec<u8>,
}

impl Encoder {
    fn push(&mut self, hub: &Hub, stereo: &[f32]) {
        self.pending.extend(
            stereo
                .iter()
                .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16),
        );
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

/// Starts COM on this thread (multithreaded apartment), once.
pub fn com_init() {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn enumerator() -> Result<IMMDeviceEnumerator, String> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(|e| e.to_string())
}

/// Active output devices as (name, ID).
pub fn render_devices() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(en) = enumerator() else {
        return out;
    };
    let Ok(list) = (unsafe { en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }) else {
        return out;
    };
    for i in 0..unsafe { list.GetCount() }.unwrap_or(0) {
        let Ok(d) = (unsafe { list.Item(i) }) else {
            continue;
        };
        let name = unsafe { d.OpenPropertyStore(STGM_READ) }
            .and_then(|p| unsafe { p.GetValue(&PKEY_Device_FriendlyName) })
            .map(|v| v.to_string())
            .unwrap_or_default();
        if let Some(id) = device_id(&d) {
            out.push((name, id));
        }
    }
    out
}

fn device_id(d: &IMMDevice) -> Option<String> {
    unsafe {
        let p = d.GetId().ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const c_void));
        s
    }
}

/// The device with `id`, or the default output device.
pub fn device(id: Option<&str>) -> Result<IMMDevice, String> {
    let en = enumerator()?;
    unsafe {
        match id {
            Some(id) => {
                let w = super::wide(id);
                en.GetDevice(PCWSTR(w.as_ptr()))
            }
            None => en.GetDefaultAudioEndpoint(eRender, eConsole),
        }
    }
    .map_err(|e| e.to_string())
}

/// 16-bit or float PCM at 48 kHz; WASAPI converts from the device's own format.
pub fn wave_format(channels: u16, float: bool) -> WAVEFORMATEX {
    let bits = if float { 32 } else { 16 };
    let align = channels * bits / 8;
    WAVEFORMATEX {
        wFormatTag: if float {
            WAVE_FORMAT_IEEE_FLOAT
        } else {
            WAVE_FORMAT_PCM
        },
        nChannels: channels,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * align as u32,
        nBlockAlign: align,
        wBitsPerSample: bits,
        cbSize: 0,
    }
}

/// Captures what `dev` plays until nobody wants audio or the device goes away.
fn capture(
    dev: &IMMDevice,
    enc: &mut Encoder,
    hub: &Hub,
    want: &dyn Fn() -> bool,
) -> Result<(), String> {
    let e = |e: windows::core::Error| e.to_string();
    unsafe {
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None).map_err(e)?;
        let fmt = wave_format(2, true);
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                // 100 ms of buffer, in 100 ns units.
                1_000_000,
                0,
                &fmt,
                None,
            )
            .map_err(e)?;
        let cap: IAudioCaptureClient = client.GetService().map_err(e)?;
        client.Start().map_err(e)?;
        log::info!("audio capture started");
        let res = loop {
            if !want() {
                break Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
            let mut failed = None;
            loop {
                match cap.GetNextPacketSize() {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(err) => {
                        failed = Some(err.to_string());
                        break;
                    }
                }
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                if let Err(err) = cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None) {
                    failed = Some(err.to_string());
                    break;
                }
                let n = frames as usize * 2;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    enc.push(hub, &vec![0.0; n]);
                } else {
                    enc.push(hub, std::slice::from_raw_parts(data as *const f32, n));
                }
                let _ = cap.ReleaseBuffer(frames);
            }
            if let Some(f) = failed {
                break Err(f);
            }
        };
        let _ = client.Stop();
        log::info!("audio capture stopped");
        res
    }
}

/// `device`: an output device name (or its start) to capture instead of the default
/// one; the Linux default ("output.monitor") means the default device.
pub fn spawn(device_name: String, bitrate: i32, hub: Arc<Hub>) {
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            com_init();
            let mut err = 0;
            let enc = unsafe { opus_encoder_create(RATE as i32, 2, 2051, &mut err) };
            if enc.is_null() {
                log::error!("opus init failed: {err}");
                return;
            }
            unsafe { opus_encoder_ctl(enc, 4002, bitrate as c_int) };
            let mut encoder = Encoder {
                enc,
                pending: Vec::with_capacity(FRAME * 4),
                out: vec![0u8; 4000],
            };
            let want = || {
                hub.clients
                    .lock()
                    .iter()
                    .any(|c| c.audio.load(Ordering::Relaxed))
            };
            let named =
                (device_name != "output.monitor" && !device_name.is_empty()).then_some(device_name);
            loop {
                if !want() {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
                // Looked up each time: the default device can change (headphones).
                let id = named.as_deref().and_then(|n| {
                    let devs = render_devices();
                    let found = devs
                        .iter()
                        .find(|(d, _)| d.starts_with(n))
                        .map(|d| d.1.clone());
                    if found.is_none() {
                        log::warn!("no output device \"{n}\"; capturing the default one");
                    }
                    found
                });
                let r = device(id.as_deref()).and_then(|d| capture(&d, &mut encoder, &hub, &want));
                encoder.pending.clear();
                if let Err(e) = r {
                    log::warn!("audio capture failed ({e}); retrying");
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        })
        .unwrap();
}
