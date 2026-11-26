//! Shared state between media producers (video/audio/cursor/clipboard) and websocket clients.

use bytes::Bytes;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

pub enum Out {
    Bin(Bytes),
    Text(String),
}

pub struct Inflight {
    pub id: u32,
    pub sent: Instant,
    pub bytes: usize,
}

pub struct Client {
    pub id: u64,
    pub tx: mpsc::UnboundedSender<Out>,
    pub video: AtomicBool,
    pub audio: AtomicBool,
    pub inflight: Mutex<VecDeque<Inflight>>,
    /// Smoothed min RTT (ms), from pings.
    pub min_rtt_ms: AtomicU32,
    /// Exponentially-weighted estimate of delivered bytes/sec, from acks.
    pub delivered: Mutex<RateEst>,
    pub needs_keyframe: AtomicBool,
    /// Has received a keyframe since (re)starting video.
    pub started: AtomicBool,
    pub delay: Mutex<DelayEst>,
}

/// Send->ack delay tracking: `base` is the (slowly expiring) minimum, the excess over
/// it is queueing somewhere on the path (network buffers or a slow decoder).
pub struct DelayEst {
    pub base_ms: f64,
    pub base_at: Instant,
    pub queue_ms: f64,
}

pub struct RateEst {
    pub window_start: Instant,
    pub bytes: usize,
    pub rate_bps: f64,
}

impl Client {
    pub fn send(&self, m: Out) {
        let _ = self.tx.send(m);
    }

    pub fn on_ack(&self, id: u32) {
        let mut q = self.inflight.lock();
        let mut acked = 0;
        let mut sample = None;
        while let Some(f) = q.front() {
            if wrapping_le(f.id, id) {
                acked += f.bytes;
                if f.id == id {
                    sample = Some(f.sent.elapsed().as_secs_f64() * 1000.0);
                }
                q.pop_front();
            } else {
                break;
            }
        }
        drop(q);
        if let Some(ms) = sample {
            let mut e = self.delay.lock();
            if ms < e.base_ms || e.base_at.elapsed().as_secs() >= 10 {
                e.base_ms = ms;
                e.base_at = Instant::now();
            }
            let qd = ms - e.base_ms;
            e.queue_ms = e.queue_ms * 0.7 + qd * 0.3;
        }
        let mut d = self.delivered.lock();
        d.bytes += acked;
        let el = d.window_start.elapsed().as_secs_f64();
        if el >= 0.5 {
            let r = d.bytes as f64 * 8.0 / el;
            d.rate_bps = if d.rate_bps == 0.0 {
                r
            } else {
                d.rate_bps * 0.6 + r * 0.4
            };
            d.bytes = 0;
            d.window_start = Instant::now();
        }
    }

    /// Frames in flight and age of the oldest unacknowledged one.
    pub fn backlog(&self) -> (usize, u128) {
        let q = self.inflight.lock();
        (
            q.len(),
            q.front().map(|f| f.sent.elapsed().as_millis()).unwrap_or(0),
        )
    }
}

/// A non-blocking, close-on-exec pipe for waking a thread that polls file descriptors
/// (`pipe2` with those flags, which macOS lacks).
#[cfg(unix)]
pub fn wake_pipe() -> nix::Result<(std::os::fd::OwnedFd, std::os::fd::OwnedFd)> {
    use nix::fcntl::{fcntl, FcntlArg, FdFlag, OFlag};
    let (r, w) = nix::unistd::pipe()?;
    for fd in [&r, &w] {
        fcntl(fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;
        fcntl(fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?;
    }
    Ok((r, w))
}

/// Something `wait_any` can wait for: a file descriptor on Unix, a handle on Windows.
#[cfg(unix)]
pub type Waitable<'a> = std::os::fd::BorrowedFd<'a>;
#[cfg(windows)]
pub type Waitable<'a> = std::os::windows::io::BorrowedHandle<'a>;

/// Wakes a thread blocked in `wait_any`: a pipe on Unix, an event on Windows.
pub struct Wake {
    #[cfg(unix)]
    r: std::os::fd::OwnedFd,
    #[cfg(unix)]
    w: std::os::fd::OwnedFd,
    #[cfg(windows)]
    event: std::os::windows::io::OwnedHandle,
}

impl Wake {
    #[cfg(unix)]
    pub fn new() -> std::io::Result<Self> {
        let (r, w) = wake_pipe()?;
        Ok(Wake { r, w })
    }

    #[cfg(windows)]
    pub fn new() -> std::io::Result<Self> {
        use std::os::windows::io::FromRawHandle;
        // Manual reset: signalled until drained, however many wakes came in between.
        let h = unsafe { windows::Win32::System::Threading::CreateEventW(None, true, false, None) }
            .map_err(std::io::Error::other)?;
        Ok(Wake {
            event: unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(h.0) },
        })
    }

    pub fn signal(&self) {
        #[cfg(unix)]
        let _ = nix::unistd::write(&self.w, &[1u8]);
        #[cfg(windows)]
        unsafe {
            use std::os::windows::io::AsRawHandle;
            let _ = windows::Win32::System::Threading::SetEvent(
                windows::Win32::Foundation::HANDLE(self.event.as_raw_handle()),
            );
        }
    }

    pub fn drain(&self) {
        #[cfg(unix)]
        {
            let mut buf = [0u8; 64];
            while matches!(nix::unistd::read(&self.r, &mut buf), Ok(n) if n > 0) {}
        }
        #[cfg(windows)]
        unsafe {
            use std::os::windows::io::AsRawHandle;
            let _ = windows::Win32::System::Threading::ResetEvent(
                windows::Win32::Foundation::HANDLE(self.event.as_raw_handle()),
            );
        }
    }

    pub fn waitable(&self) -> Waitable<'_> {
        #[cfg(unix)]
        return std::os::fd::AsFd::as_fd(&self.r);
        #[cfg(windows)]
        return std::os::windows::io::AsHandle::as_handle(&self.event);
    }
}

/// Blocks until one of `w` is ready or `timeout_ms` passes (negative: no timeout).
pub fn wait_any(w: &[Waitable], timeout_ms: i32) {
    #[cfg(unix)]
    {
        use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
        let mut p: Vec<PollFd> = w
            .iter()
            .map(|&fd| PollFd::new(fd, PollFlags::POLLIN))
            .collect();
        let _ = poll(
            &mut p,
            PollTimeout::try_from(timeout_ms).unwrap_or(PollTimeout::NONE),
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        let hs: Vec<HANDLE> = w.iter().map(|h| HANDLE(h.as_raw_handle())).collect();
        let ms = if timeout_ms < 0 {
            windows::Win32::System::Threading::INFINITE
        } else {
            timeout_ms as u32
        };
        unsafe { windows::Win32::System::Threading::WaitForMultipleObjects(&hs, false, ms) };
    }
}

fn wrapping_le(a: u32, b: u32) -> bool {
    b.wrapping_sub(a) < 0x8000_0000
}

#[derive(Default)]
pub struct Stats {
    pub captured: AtomicU64,
    pub encoded: AtomicU64,
    pub bytes: AtomicU64,
    pub encode_us: AtomicU64,
    pub grab_us: AtomicU64,
    pub convert_us: AtomicU64,
    pub flow_blocked: AtomicU64,
    pub paintovers: AtomicU64,
    pub keyframes: AtomicU64,
    pub refines: AtomicU64,
    pub damage_wait_us: AtomicU64,
    pub send_lat_us: AtomicU64,
}

pub struct Hub {
    #[cfg(target_os = "macos")]
    pub display_req: Mutex<Option<crate::mac::DisplayRequest>>,
    /// Connections older than this may no longer inject input after a display switch.
    #[cfg(target_os = "macos")]
    pub input_floor: AtomicU64,
    pub clients: Mutex<Vec<Arc<Client>>>,
    pub next_id: AtomicU64,
    pub stats: Stats,
    pub width: AtomicU32,
    pub height: AtomicU32,
    /// Encoded stripes as (y, height).
    pub stripes: Mutex<Vec<(u32, u32)>>,
    /// Adaptive bitrate cap applied by the encoder (kbit/s).
    pub target_kbps: AtomicU32,
    /// Last cursor message, replayed to new clients.
    pub cursor: Mutex<Option<Bytes>>,
    /// Wakes the capture thread (acks, joins, keyframe requests).
    pub waker: Wake,
    pub input: Mutex<Option<std::sync::mpsc::Sender<crate::input::InputEvent>>>,
    pub last_clipboard: Mutex<Option<String>>,
    /// Desktop size a client asked for (dynamic resolution), applied by the video thread.
    pub resize_req: Mutex<Option<(u32, u32)>>,
    /// Largest desktop size (Some when dynamic resolution is on).
    pub resize_max: Mutex<Option<(u32, u32)>>,
    /// Server settings sent in every `hello` besides the geometry (audio, files, ...).
    pub hello_extra: Mutex<serde_json::Map<String, serde_json::Value>>,
    /// Screenshot requests, answered by the capture thread with the current screen.
    pub snapshots: Mutex<Vec<tokio::sync::oneshot::Sender<Arc<crate::record::Snapshot>>>>,
    /// When each part of the screen last changed (`/wait`).
    pub activity: crate::wait::Activity,
}

impl Hub {
    pub fn new() -> Arc<Self> {
        Arc::new(Hub {
            #[cfg(target_os = "macos")]
            display_req: Mutex::new(None),
            #[cfg(target_os = "macos")]
            input_floor: AtomicU64::new(0),
            clients: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            stats: Stats::default(),
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
            target_kbps: AtomicU32::new(0),
            stripes: Mutex::new(Vec::new()),
            cursor: Mutex::new(None),
            waker: Wake::new().expect("wake"),
            input: Mutex::new(None),
            last_clipboard: Mutex::new(None),
            resize_req: Mutex::new(None),
            resize_max: Mutex::new(None),
            hello_extra: Mutex::new(serde_json::Map::new()),
            snapshots: Mutex::new(Vec::new()),
            activity: Default::default(),
        })
    }

    pub fn wake(&self) {
        self.waker.signal();
    }

    pub fn drain_wake(&self) {
        self.waker.drain();
    }

    pub fn add_client(&self, tx: mpsc::UnboundedSender<Out>) -> Arc<Client> {
        let c = Arc::new(Client {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            tx,
            video: AtomicBool::new(false),
            audio: AtomicBool::new(false),
            inflight: Mutex::new(VecDeque::new()),
            min_rtt_ms: AtomicU32::new(0),
            delivered: Mutex::new(RateEst {
                window_start: Instant::now(),
                bytes: 0,
                rate_bps: 0.0,
            }),
            needs_keyframe: AtomicBool::new(true),
            started: AtomicBool::new(false),
            delay: Mutex::new(DelayEst {
                base_ms: f64::MAX,
                base_at: Instant::now(),
                queue_ms: 0.0,
            }),
        });
        self.clients.lock().push(c.clone());
        c
    }

    pub fn remove_client(&self, id: u64) {
        self.clients.lock().retain(|c| c.id != id);
        self.wake();
    }

    pub fn video_clients(&self) -> Vec<Arc<Client>> {
        self.clients
            .lock()
            .iter()
            .filter(|c| c.video.load(Ordering::Relaxed))
            .cloned()
            .collect()
    }

    /// The `hello` message: current geometry plus the server's settings.
    pub fn hello(&self) -> String {
        let mut m = self.hello_extra.lock().clone();
        m.insert("t".into(), "hello".into());
        m.insert("w".into(), self.width.load(Ordering::Relaxed).into());
        m.insert("h".into(), self.height.load(Ordering::Relaxed).into());
        m.insert("stripes".into(), serde_json::json!(*self.stripes.lock()));
        serde_json::Value::Object(m).to_string()
    }

    /// After the stream restarts (new size): every client needs the new geometry and
    /// a keyframe, and acks for the old stream's frames no longer apply.
    pub fn restart_clients(&self) {
        let hello = self.hello();
        for c in self.clients.lock().iter() {
            c.inflight.lock().clear();
            c.started.store(false, Ordering::Relaxed);
            c.needs_keyframe.store(true, Ordering::Relaxed);
            c.send(Out::Text(hello.clone()));
        }
    }

    pub fn broadcast(&self, m: impl Fn() -> Out) {
        for c in self.clients.lock().iter() {
            c.send(m());
        }
    }

    /// Share the admission lock with input senders, so the barrier cannot overtake input.
    #[cfg(target_os = "macos")]
    pub fn pause_display_input(&self, id: u32, client: u64) -> Option<crate::mac::DisplayRequest> {
        let mut input = self.input.lock();
        if client < self.input_floor.load(Ordering::Relaxed) {
            return None;
        }
        let sender = input.take()?;
        let (done, ready) = std::sync::mpsc::channel();
        if sender
            .send(crate::input::InputEvent::DisplayBarrier(done))
            .is_err()
        {
            *input = Some(sender);
            return None;
        }
        self.input_floor
            .store(self.next_id.load(Ordering::Relaxed), Ordering::Relaxed);
        // Retire their subscriptions too: an old client ignoring close must not
        // stall flow control for the replacement connections.
        for client in self.clients.lock().drain(..) {
            client.send(Out::Text("{\"t\":\"close\"}".into()));
        }
        Some(crate::mac::DisplayRequest { id, sender, ready })
    }
}
