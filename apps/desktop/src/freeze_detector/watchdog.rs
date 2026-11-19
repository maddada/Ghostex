//! The GPUI-free half of the freeze detector: the heartbeat clock, the watchdog thread, and the
//! signal that makes the stuck main thread record its own stack.
//!
//! The main thread calls [`tick`] a few times a second. When the watchdog sees no tick for
//! [`FREEZE_THRESHOLD`], it sends the main thread [`sample_signal`]; the handler unwinds the
//! main thread's stack into a fixed buffer, and the watchdog symbolizes it and appends it to the
//! freeze log. A long freeze is sampled again every [`RESAMPLE_EVERY`] (at most
//! [`MAX_SAMPLES_PER_FREEZE`] times), and the end of the freeze is logged with its length.

use std::ffi::c_void;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// How long the main thread may go without a tick before it counts as frozen.
pub const FREEZE_THRESHOLD: Duration = Duration::from_secs(3);
/// How often the main thread ticks while it is free.
pub const TICK_EVERY: Duration = Duration::from_millis(250);
const POLL_EVERY: Duration = Duration::from_millis(500);
const RESAMPLE_EVERY: Duration = Duration::from_secs(15);
const MAX_SAMPLES_PER_FREEZE: u32 = 6;
const MAX_FRAMES: usize = 256;
/// How long the watchdog waits for the signal handler to finish unwinding.
const SAMPLE_TIMEOUT: Duration = Duration::from_secs(2);

static EPOCH: OnceLock<Instant> = OnceLock::new();
/// Milliseconds since [`EPOCH`] at the main thread's last tick; 0 until the first one.
static LAST_TICK_MS: AtomicU64 = AtomicU64::new(0);
static MAIN_THREAD: OnceLock<usize> = OnceLock::new();

static FRAMES: [AtomicUsize; MAX_FRAMES] = [const { AtomicUsize::new(0) }; MAX_FRAMES];
static FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);
static SAMPLE_DONE: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    // +1 so a tick in the first millisecond is not read as "never ticked".
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// Marks the main thread as alive. Call it from the main thread only.
pub fn tick() {
    LAST_TICK_MS.store(now_ms(), Ordering::Relaxed);
}

/// The signal that asks the main thread for its stack: a real-time signal on Linux (nothing in
/// the app or CEF uses it), SIGINFO on macOS (no real-time signals there; ignored by default and
/// unused by GPUI and CEF).
fn sample_signal() -> libc::c_int {
    #[cfg(target_os = "linux")]
    {
        libc::SIGRTMIN() + 6
    }
    #[cfg(not(target_os = "linux"))]
    {
        libc::SIGINFO
    }
}

extern "C" fn on_sample_signal(_: libc::c_int, _: *mut libc::siginfo_t, _: *mut c_void) {
    let saved_errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    let mut count = 0usize;
    // SAFETY: runs on the frozen main thread, which is blocked inside whatever it was doing; the
    // unwinder only reads its stack. Not strictly async-signal-safe, which is accepted for a
    // dev-build diagnostic (the watchdog gives up after SAMPLE_TIMEOUT).
    unsafe {
        backtrace::trace_unsynchronized(|frame| {
            if count >= MAX_FRAMES {
                return false;
            }
            FRAMES[count].store(frame.ip() as usize, Ordering::Relaxed);
            count += 1;
            true
        });
    }
    FRAME_COUNT.store(count, Ordering::Relaxed);
    SAMPLE_DONE.store(true, Ordering::Release);
    set_errno(saved_errno);
}

fn set_errno(value: libc::c_int) {
    // SAFETY: writes this thread's errno slot.
    unsafe {
        #[cfg(target_os = "linux")]
        {
            *libc::__errno_location() = value;
        }
        #[cfg(not(target_os = "linux"))]
        {
            *libc::__error() = value;
        }
    }
}

/// Starts the watchdog. Call it once, from the main thread; `log_path` is the file freezes are
/// appended to (created on the first freeze).
pub fn start(log_path: PathBuf) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    // SAFETY: pthread_self has no preconditions; the value is only handed back to pthread_kill.
    let main = unsafe { libc::pthread_self() } as usize;
    let _ = MAIN_THREAD.set(main);
    let _ = EPOCH.get_or_init(Instant::now);
    // SAFETY: installs a process-wide handler for a signal nothing else uses (see sample_signal).
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_sample_signal
            as extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut c_void)
            as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(sample_signal(), &action, std::ptr::null_mut()) != 0 {
            eprintln!(
                "[freeze-detector] could not install the sample signal handler: {}",
                std::io::Error::last_os_error()
            );
            return;
        }
    }
    let spawned = std::thread::Builder::new()
        .name("ghostex-freeze-watchdog".into())
        .spawn(move || watch(log_path));
    if let Err(error) = spawned {
        eprintln!("[freeze-detector] could not start the watchdog thread: {error}");
    }
}

struct Freeze {
    /// The last tick before the freeze, in ms since EPOCH.
    last_tick_before: u64,
    samples: u32,
    next_sample_after: Duration,
}

fn watch(log_path: PathBuf) {
    let mut freeze: Option<Freeze> = None;
    loop {
        std::thread::sleep(POLL_EVERY);
        let last_tick = LAST_TICK_MS.load(Ordering::Relaxed);
        if last_tick == 0 {
            continue;
        }
        let stalled = Duration::from_millis(now_ms().saturating_sub(last_tick));
        match freeze.as_mut() {
            Some(current) if last_tick != current.last_tick_before => {
                let stuck = Duration::from_millis(last_tick - current.last_tick_before);
                append(
                    &log_path,
                    &format!(
                        "{} main thread free again after about {:.1}s\n\n",
                        timestamp(),
                        stuck.as_secs_f64()
                    ),
                );
                freeze = None;
            }
            Some(current) => {
                if current.samples < MAX_SAMPLES_PER_FREEZE && stalled >= current.next_sample_after
                {
                    record_sample(&log_path, stalled, current.samples + 1);
                    current.samples += 1;
                    current.next_sample_after = stalled + RESAMPLE_EVERY;
                }
            }
            None if stalled >= FREEZE_THRESHOLD => {
                append(
                    &log_path,
                    &format!(
                        "{} FREEZE: the main thread has not ticked for {:.1}s (pid {})\n",
                        timestamp(),
                        stalled.as_secs_f64(),
                        std::process::id()
                    ),
                );
                eprintln!(
                    "[freeze-detector] main thread stuck for {:.1}s; stack in {}",
                    stalled.as_secs_f64(),
                    log_path.display()
                );
                record_sample(&log_path, stalled, 1);
                freeze = Some(Freeze {
                    last_tick_before: last_tick,
                    samples: 1,
                    next_sample_after: stalled + RESAMPLE_EVERY,
                });
            }
            None => {}
        }
    }
}

fn record_sample(log_path: &PathBuf, stalled: Duration, number: u32) {
    let Some(&main) = MAIN_THREAD.get() else {
        return;
    };
    SAMPLE_DONE.store(false, Ordering::Release);
    FRAME_COUNT.store(0, Ordering::Relaxed);
    // SAFETY: `main` is the main thread's pthread_t, which lives as long as the process.
    let sent = unsafe { libc::pthread_kill(main as libc::pthread_t, sample_signal()) };
    let mut text = format!("  stack {number} at {:.1}s stuck:\n", stalled.as_secs_f64());
    if sent != 0 {
        text.push_str(&format!(
            "    (could not signal the main thread: error {sent})\n"
        ));
        append(log_path, &text);
        return;
    }
    let waited = Instant::now();
    while !SAMPLE_DONE.load(Ordering::Acquire) {
        if waited.elapsed() > SAMPLE_TIMEOUT {
            text.push_str(
                "    (the main thread did not answer; it may be blocked with signals masked)\n",
            );
            append(log_path, &text);
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let count = FRAME_COUNT.load(Ordering::Relaxed);
    let frames: Vec<usize> = (0..count)
        .map(|index| FRAMES[index].load(Ordering::Relaxed))
        .collect();
    let mut lines = Vec::with_capacity(frames.len());
    for ip in frames {
        let mut described = Vec::new();
        backtrace::resolve(ip as *mut c_void, |symbol| {
            let name = symbol
                .name()
                .map(|name| format!("{name:#}"))
                .unwrap_or_else(|| "<unknown>".into());
            let location = match (symbol.filename(), symbol.lineno()) {
                (Some(file), Some(line)) => format!(" at {}:{line}", file.display()),
                _ => String::new(),
            };
            described.push(format!("{name}{location}"));
        });
        if described.is_empty() {
            described.push(format!("{ip:#x}"));
        }
        lines.extend(described);
    }
    // Skip the unwinder's and the signal handler's own frames.
    let start = lines
        .iter()
        .rposition(|line| line.contains("on_sample_signal"))
        .map_or(0, |index| index + 1);
    // The frame after the handler is the kernel's signal trampoline; keep it, it marks where
    // the main thread was interrupted.
    for (index, line) in lines[start..].iter().enumerate() {
        text.push_str(&format!("    {index:3}: {line}\n"));
    }
    append(log_path, &text);
}

fn timestamp() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f %z")
        .to_string()
}

fn append(log_path: &PathBuf, text: &str) {
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = file.write_all(text.as_bytes());
    }
}
