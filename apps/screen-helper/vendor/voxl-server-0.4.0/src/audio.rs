//! Desktop audio: PulseAudio monitor capture -> Opus (10 ms frames) -> clients.

use crate::hub::{Hub, Out};
use bytes::Bytes;
use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

#[repr(C)]
struct PaSampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}
#[repr(C)]
struct PaBufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}
#[link(name = "pulse-simple")]
extern "C" {
    fn pa_simple_new(
        server: *const c_char,
        name: *const c_char,
        dir: c_int,
        dev: *const c_char,
        stream: *const c_char,
        ss: *const PaSampleSpec,
        map: *const c_void,
        attr: *const PaBufferAttr,
        error: *mut c_int,
    ) -> *mut c_void;
    fn pa_simple_read(s: *mut c_void, data: *mut c_void, bytes: usize, error: *mut c_int) -> c_int;
    fn pa_simple_free(s: *mut c_void);
}
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

pub fn spawn(device: String, bitrate: i32, hub: Arc<Hub>) {
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || unsafe {
            let mut err = 0;
            let enc = opus_encoder_create(RATE as i32, 2, 2051, &mut err);
            if enc.is_null() {
                log::error!("opus init failed: {err}");
                return;
            }
            opus_encoder_ctl(enc, 4002, bitrate as c_int);
            let name = c"voxl";
            let dev = CString::new(device).unwrap();
            let ss = PaSampleSpec {
                format: 3,
                rate: RATE,
                channels: 2,
            };
            let frag = (FRAME * 4) as u32;
            let attr = PaBufferAttr {
                maxlength: frag * 8,
                tlength: u32::MAX,
                prebuf: u32::MAX,
                minreq: u32::MAX,
                fragsize: frag,
            };
            let mut pcm = vec![0i16; FRAME * 2];
            let mut out = vec![0u8; 4000];
            loop {
                let want = || {
                    hub.clients
                        .lock()
                        .iter()
                        .any(|c| c.audio.load(Ordering::Relaxed))
                };
                if !want() {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
                let s = pa_simple_new(
                    std::ptr::null(),
                    name.as_ptr(),
                    2,
                    dev.as_ptr(),
                    name.as_ptr(),
                    &ss,
                    std::ptr::null(),
                    &attr,
                    &mut err,
                );
                if s.is_null() {
                    log::warn!("pulse connect failed ({err}); retrying");
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
                log::info!("audio capture started");
                let mut checked = 0u32;
                loop {
                    if pa_simple_read(s, pcm.as_mut_ptr() as *mut c_void, pcm.len() * 2, &mut err)
                        < 0
                    {
                        log::warn!("pulse read failed ({err})");
                        break;
                    }
                    checked += 1;
                    if checked % 50 == 0 && !want() {
                        break;
                    }
                    if pcm.iter().all(|&s| s == 0) {
                        continue;
                    }
                    let n = opus_encode(
                        enc,
                        pcm.as_ptr(),
                        FRAME as c_int,
                        out.as_mut_ptr(),
                        out.len() as i32,
                    );
                    if n <= 0 {
                        continue;
                    }
                    let mut m = Vec::with_capacity(n as usize + 1);
                    m.push(0x02);
                    m.extend_from_slice(&out[..n as usize]);
                    let b = Bytes::from(m);
                    for c in hub.clients.lock().iter() {
                        if c.audio.load(Ordering::Relaxed) {
                            c.send(Out::Bin(b.clone()));
                        }
                    }
                }
                pa_simple_free(s);
                log::info!("audio capture stopped");
            }
        })
        .unwrap();
}
