//! Microphone: Opus packets (or raw PCM) from a browser -> a FIFO that PulseAudio reads
//! as the VM's microphone (module-pipe-source), or on macOS and Windows an audio
//! output device that loops back to an input (see `mac::mic`, `win::mic`). One client
//! holds the microphone at a time; a client that turns its microphone on takes it over.
//!
//! A writer thread feeds the FIFO a 20 ms frame on its own clock: the user's audio
//! when there is some, silence otherwise, so apps recording the microphone never
//! stall. A pipe source plays whatever arrives, so this clock (not PulseAudio's timer,
//! which runs slow at low latency on some VMs) sets the pace. Audio waits in a short
//! jitter queue that drops its oldest samples past MAX_QUEUED, so bursts after a
//! network stall or clock drift never become growing delay.

use crate::hub::{Hub, Out};
use bytes::Bytes;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

pub enum MicMsg {
    Start(u64),
    Opus(u64, Bytes),
    Pcm(u64, Bytes),
    Stop(u64),
}

// On Windows, build.rs links the static Opus from vcpkg (a #[link] would ask for a DLL).
#[cfg_attr(not(windows), link(name = "opus"))]
extern "C" {
    fn opus_decoder_create(fs: i32, channels: c_int, error: *mut c_int) -> *mut c_void;
    fn opus_decode(
        st: *mut c_void,
        data: *const u8,
        len: i32,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;
}

pub const RATE: i32 = 48000;
/// 20 ms, the pace at which the sink takes audio.
pub const FRAME: usize = 960;
/// Start playing once 40 ms is buffered, absorbing network jitter.
const PREFILL: usize = 2 * FRAME;
/// At most 120 ms of audio waits to be played.
const MAX_QUEUED: usize = 6 * FRAME;

#[derive(Default)]
pub struct Queue {
    samples: VecDeque<i16>,
    playing: bool,
    dropped: usize,
    /// Times the jitter queue ran dry mid-stream (each one is an audible gap).
    underruns: usize,
}

/// The next 20 ms to play: queued audio once the jitter buffer has filled, else silence.
pub fn next_frame(queue: &Mutex<Queue>, frame: &mut [i16]) {
    let mut q = queue.lock();
    if !q.playing && q.samples.len() >= PREFILL {
        q.playing = true;
    }
    frame.fill(0);
    if q.playing {
        let n = q.samples.len().min(frame.len());
        for (d, s) in frame.iter_mut().zip(q.samples.drain(..n)) {
            *d = s;
        }
        if n < frame.len() {
            q.playing = false; // ran dry: refill the jitter buffer first
            q.underruns += 1;
        }
    }
}

/// Opens the FIFO without blocking (fails until PulseAudio has it open for reading)
/// and shrinks its buffer, so audio that piles up while nobody records is at most
/// ~40 ms stale when someone starts.
#[cfg(target_os = "linux")]
fn open_fifo(path: &std::ffi::CStr) -> Option<std::os::fd::OwnedFd> {
    use nix::fcntl::{fcntl, open, FcntlArg, OFlag};
    use nix::sys::stat::Mode;
    let fd = open(
        path,
        OFlag::O_WRONLY | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    let _ = fcntl(&fd, FcntlArg::F_SETPIPE_SZ(4096));
    Some(fd)
}

#[cfg(target_os = "linux")]
fn writer(path: std::ffi::CString, queue: Arc<Mutex<Queue>>) {
    use nix::errno::Errno;
    use nix::unistd::write;
    use std::time::Duration;
    let mut fd: Option<std::os::fd::OwnedFd> = None;
    let mut warned = false;
    let mut frame = vec![0i16; FRAME];
    let mut next = Instant::now();
    loop {
        next += Duration::from_millis(20);
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(200) {
            next = now; // fell behind (suspended?): don't try to catch up
        }
        next_frame(&queue, &mut frame);
        if fd.is_none() {
            fd = open_fifo(&path);
            match fd {
                Some(_) => {
                    log::info!("microphone: writing to {path:?}");
                    warned = false;
                }
                None if !warned => {
                    log::warn!(
                        "microphone: cannot open {path:?} (is PulseAudio's pipe source loaded?)"
                    );
                    warned = true;
                }
                None => {}
            }
        }
        if let Some(f) = &fd {
            let bytes =
                unsafe { std::slice::from_raw_parts(frame.as_ptr() as *const u8, FRAME * 2) };
            // EAGAIN: nobody is reading right now, so the frame isn't needed.
            if matches!(write(f, bytes), Err(e) if e != Errno::EAGAIN) {
                fd = None; // dropping the OwnedFd closes it
            }
        }
    }
}

/// `sink` is the FIFO's path on Linux and the output device's ID on macOS and Windows.
pub fn spawn(sink: String, hub: Arc<Hub>) -> mpsc::Sender<MicMsg> {
    let (tx, rx) = mpsc::channel::<MicMsg>();
    let queue = Arc::new(Mutex::new(Queue::default()));
    {
        let queue = queue.clone();
        #[cfg(target_os = "linux")]
        let run = move || writer(std::ffi::CString::new(sink).unwrap(), queue);
        #[cfg(target_os = "macos")]
        let run = move || crate::mac::mic::play(&sink, queue);
        #[cfg(windows)]
        let run = move || crate::win::mic::play(&sink, queue);
        std::thread::Builder::new()
            .name("mic-out".into())
            .spawn(run)
            .unwrap();
    }
    std::thread::Builder::new()
        .name("mic".into())
        .spawn(move || {
            let mut err = 0;
            let dec = unsafe { opus_decoder_create(RATE, 1, &mut err) };
            if dec.is_null() {
                log::error!("opus decoder init failed: {err}");
                return;
            }
            let mut owner: Option<u64> = None;
            let mut since = Instant::now();
            let mut pcm = vec![0i16; 5760];
            let tell = |id: u64, on: bool| {
                if let Some(c) = hub.clients.lock().iter().find(|c| c.id == id) {
                    c.send(Out::Text(format!("{{\"t\":\"mic\",\"on\":{on}}}")));
                }
            };
            let push = |samples: &[i16]| {
                let mut q = queue.lock();
                q.samples.extend(samples);
                let over = q.samples.len().saturating_sub(MAX_QUEUED);
                if over > 0 {
                    q.samples.drain(..over);
                    q.dropped += over;
                }
            };
            let reset = || {
                let mut q = queue.lock();
                let stats = (q.dropped, q.underruns);
                *q = Queue::default();
                stats
            };
            while let Ok(m) = rx.recv() {
                match m {
                    MicMsg::Start(id) => {
                        if let Some(prev) = owner.filter(|&p| p != id) {
                            tell(prev, false);
                        }
                        reset();
                        owner = Some(id);
                        since = Instant::now();
                        tell(id, true);
                        log::info!("microphone on (client {id})");
                    }
                    MicMsg::Opus(id, b) if owner == Some(id) => {
                        let n = unsafe { opus_decode(dec, b.as_ptr(), b.len() as i32, pcm.as_mut_ptr(), pcm.len() as c_int, 0) };
                        if n > 0 {
                            push(&pcm[..n as usize]);
                        }
                    }
                    MicMsg::Pcm(id, b) if owner == Some(id) => {
                        let n = (b.len() / 2).min(pcm.len());
                        for (i, d) in pcm[..n].iter_mut().enumerate() {
                            *d = i16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
                        }
                        push(&pcm[..n]);
                    }
                    MicMsg::Stop(id) if owner == Some(id) => {
                        owner = None;
                        let (dropped, underruns) = reset();
                        log::info!("microphone off (client {id}, {:.1} s, {} ms dropped, {underruns} underruns)", since.elapsed().as_secs_f64(), dropped * 1000 / RATE as usize);
                    }
                    _ => {}
                }
            }
        })
        .unwrap();
    tx
}
