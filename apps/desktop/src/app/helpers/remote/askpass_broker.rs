//! How a saved SSH password reaches ssh on macOS and Linux: ssh runs the askpass helper script,
//! the helper reads the answer from a named pipe in a private folder with `/bin/cat`, and a broker
//! thread of this app answers each read with the password from the system keyring.
//! `ssh_process.rs` decides when to make one; everything about the folder, the pipe and the
//! broker is here.
//!
//! CDXC:RemoteMachines 2026-11-26 WHY:
//! A security review of the blocking broker (5b7469b4b) found, among others: two helper folders
//! made in the same microsecond got the same name, so both sign-ins failed when several tunnels
//! started together; the broker answered a reader that held the pipe open without reading as fast
//! as it could (thousands of keyring reads a second); it trusted whatever sat at the pipe path;
//! it could write into the drop's own reader; and every tunnel kept answering anyone who opened
//! the pipe for its whole life. So: folder names carry a counter and random bytes and are created
//! private, never adopted; each helper is answered once and the broker then pauses; the pipe must
//! be the FIFO made here; `cancel` is checked again after the keyring read; tunnels and one-shot
//! commands stop answering once their sign-in window is over; folders of dead runs are swept.
//! Terminals still answer until they are closed: their wrapper runs ssh again after a disconnect,
//! and each of those sign-ins needs the password (CDXC:RemoteMachines 2026-09-23).

use std::{
    env, fs,
    io::{self, Write as _},
    os::{
        fd::AsRawFd as _,
        unix::fs::{
            DirBuilderExt as _, FileTypeExt as _, MetadataExt as _, OpenOptionsExt as _,
            PermissionsExt as _,
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::app::helpers::*;

/// How long a tunnel's or a one-shot command's helper is answered after it is made. ssh connects
/// within `ConnectTimeout=8` and signs in with one password prompt
/// (`gpui_remote_ssh_client_options`), so its asks come in the first seconds; a jump host's
/// second hop comes soon after. Afterwards the pipe is removed and a late helper fails at once.
pub(crate) const GPUI_ASKPASS_HANDSHAKE_WINDOW: Duration = Duration::from_secs(60);

/// After answering, the broker waits this long before it answers again, so the helper just served
/// has seen the end of its line and left; only a reader that stays makes it wait longer.
const SERVED_PAUSE: Duration = Duration::from_millis(3);
const LONGEST_PAUSE: Duration = Duration::from_millis(500);

/// How long `stop` waits for the broker to return before leaving it to finish on its own (it
/// checks `cancel` before writing, so it can only stop).
const STOP_WAIT: Duration = Duration::from_secs(2);

/// How long a retired pipe stays open for reading and writing, so a helper that opened it in the
/// instant it was taken away gets an empty answer rather than waiting for a writer forever.
const RETIRED_PIPE_HOLD: Duration = Duration::from_millis(100);

const FOLDER_PREFIX: &str = "gxap-";
const SCRIPT_NAME: &str = "a";
const PIPE_NAME: &str = "p";
const RETIRED_PIPE_NAME: &str = "p-retired";

/// Reads the secret for one answer; `None` answers with nothing, which the helper reports as the
/// password being unavailable.
pub(crate) type GpuiAskpassSecret = Box<dyn FnMut() -> Option<Vec<u8>> + Send>;

/// The folder, helper script and pipe of one askpass helper.
pub(crate) struct GpuiAskpassFolder {
    pub(crate) directory: PathBuf,
    pub(crate) script: PathBuf,
    pub(crate) pipe: PathBuf,
}

impl GpuiAskpassFolder {
    /// Makes a new private folder under `base` with the helper and the pipe in it. The folder is
    /// created with mode 0700 and never adopted: a name that exists is a failure, and the next
    /// name is tried.
    pub(crate) fn create(base: &Path) -> io::Result<Self> {
        let directory = create_private_directory(base)?;
        let folder = Self {
            script: directory.join(SCRIPT_NAME),
            pipe: directory.join(PIPE_NAME),
            directory,
        };
        if let Err(error) = folder.fill() {
            let _ = fs::remove_dir_all(&folder.directory);
            return Err(error);
        }
        Ok(folder)
    }

    fn fill(&self) -> io::Result<()> {
        // `/bin/cat` takes the answer in one read: the broker writes it in one write and answers
        // each helper once, so two readers at the same moment get the whole line or nothing,
        // never a part (the shell's `read` took one byte at a time, 2026-11-26). A pipe that is
        // gone means the sign-in window ended, which gets its own message
        // (`GPUI_ASKPASS_WINDOW_ENDED_TEXT`, mapped in connect.rs).
        let pipe = gpui_shell_single_quote(gpui_path_string(self.pipe.as_path()).as_str());
        let contents = format!(
            concat!(
                "#!/bin/sh\n",
                "if [ ! -p {pipe} ]; then\n",
                "  printf '%s\\n' {ended} >&2\n",
                "  exit 1\n",
                "fi\n",
                "password=$(/bin/cat {pipe} </dev/null) || password=\n",
                "if [ -z \"$password\" ]; then\n",
                "  printf '%s\\n' 'Ghostex saved SSH password unavailable.' >&2\n",
                "  exit 1\n",
                "fi\n",
                "printf '%s\\n' \"$password\"\n",
                "unset password\n",
            ),
            pipe = pipe,
            ended = gpui_shell_single_quote(GPUI_ASKPASS_WINDOW_ENDED_TEXT),
        );
        let mut script = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&self.script)?;
        script.write_all(contents.as_bytes())?;
        drop(script);
        // The umask may have taken bits off; never more than 0700.
        fs::set_permissions(&self.script, fs::Permissions::from_mode(0o700))?;
        let pipe = std::ffi::CString::new(gpui_path_string(self.pipe.as_path()))
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: `pipe` is a valid NUL-terminated path inside the private folder made above.
        if unsafe { libc::mkfifo(pipe.as_ptr(), 0o600) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

fn create_private_directory(base: &Path) -> io::Result<PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut last_error = io::Error::from(io::ErrorKind::AlreadyExists);
    for _ in 0..8 {
        let directory = base.join(format!(
            "{FOLDER_PREFIX}{}-{:x}-{:016x}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            uuid::Uuid::new_v4().as_u128() as u64,
        ));
        match fs::DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => {
                let owned = fs::symlink_metadata(&directory)
                    .is_ok_and(|metadata| metadata.is_dir() && metadata.uid() == effective_uid());
                if !owned {
                    return Err(io::Error::from(io::ErrorKind::PermissionDenied));
                }
                // The umask may have taken bits off; never more than 0700.
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
                return Ok(directory);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last_error = error,
            Err(error) => return Err(error),
        }
    }
    Err(last_error)
}

/// Where helper folders are made: on Linux the user's private runtime folder when there is one
/// (`/tmp` is shared there), else the temporary folder (per user on macOS).
pub(crate) fn gpui_askpass_base_directory() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Some(runtime) = linux_runtime_directory() {
        return runtime;
    }
    env::temp_dir()
}

#[cfg(target_os = "linux")]
fn linux_runtime_directory() -> Option<PathBuf> {
    let directory = PathBuf::from(env::var_os("XDG_RUNTIME_DIR")?);
    let metadata = fs::symlink_metadata(&directory).ok()?;
    (directory.is_absolute()
        && metadata.is_dir()
        && metadata.uid() == effective_uid()
        && metadata.mode() & 0o077 == 0
        && mounted_with_exec(&directory))
    .then_some(directory)
}

/// ssh runs the helper from this folder, so a `noexec` mount cannot hold it.
#[cfg(target_os = "linux")]
fn mounted_with_exec(directory: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt as _;
    let Ok(path) = std::ffi::CString::new(directory.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is NUL-terminated and `stat` is a plain C struct the call fills in.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return false;
    }
    stat.f_flag & libc::ST_NOEXEC == 0
}

fn effective_uid() -> u32 {
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// Removes the helper folders of runs that are no longer alive. A folder only ever holds the
/// helper and the pipe, never a secret; this keeps them from piling up after a crash.
pub(crate) fn gpui_sweep_stale_askpass_directories() {
    let mut bases = vec![env::temp_dir()];
    let current = gpui_askpass_base_directory();
    if !bases.contains(&current) {
        bases.push(current);
    }
    for base in bases {
        sweep_stale_askpass_directories_in(&base);
    }
}

fn sweep_stale_askpass_directories_in(base: &Path) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    let me = std::process::id();
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(askpass_directory_pid) else {
            continue;
        };
        if pid == me || process_may_be_alive(pid) {
            continue;
        }
        let path = entry.path();
        let ours = fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_dir() && metadata.uid() == effective_uid());
        if ours {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// The pid in a helper folder's name: `gxap-<pid>-<hex>` before 2026-11-26,
/// `gxap-<pid>-<counter>-<random>` since.
fn askpass_directory_pid(name: &str) -> Option<u32> {
    let (pid, rest) = name.strip_prefix(FOLDER_PREFIX)?.split_once('-')?;
    if rest.is_empty()
        || !rest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return None;
    }
    pid.parse().ok()
}

fn process_may_be_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return true;
    };
    if pid <= 0 {
        return true;
    }
    // SAFETY: signal 0 only asks whether the process exists; nothing is delivered.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// The FIFO made for one broker. Every open is checked against it, so a pipe swapped for a file,
/// a link or another FIFO is never written to.
#[derive(Clone, Copy, PartialEq, Eq)]
struct PipeIdentity {
    device: u64,
    inode: u64,
    uid: u32,
}

impl PipeIdentity {
    fn of(metadata: &fs::Metadata) -> Option<Self> {
        (metadata.file_type().is_fifo() && metadata.uid() == effective_uid()).then(|| Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            uid: metadata.uid(),
        })
    }

    fn is(&self, file: &fs::File) -> bool {
        file.metadata()
            .ok()
            .and_then(|metadata| Self::of(&metadata))
            .is_some_and(|identity| identity == *self)
    }
}

#[derive(Default)]
struct BrokerState {
    /// The broker thread has returned.
    server_done: bool,
    /// `stop` has run (by the window or by the drop).
    stopped: bool,
    /// The owner is dropping the broker; the window watcher leaves.
    dropping: bool,
}

struct Shared {
    pipe: PathBuf,
    identity: PipeIdentity,
    cancel: AtomicBool,
    state: Mutex<BrokerState>,
    changed: Condvar,
}

impl Shared {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, BrokerState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// One helper's broker: the thread that answers it, and for `Handshake` the watcher that ends the
/// sign-in window. Dropping it stops both; the owner then removes the folder.
pub(crate) struct GpuiAskpassBroker {
    shared: Arc<Shared>,
    server: Option<thread::JoinHandle<()>>,
    watcher: Option<thread::JoinHandle<()>>,
}

impl GpuiAskpassBroker {
    pub(crate) fn start(
        pipe: PathBuf,
        serve: GpuiAskpassServe,
        window: Duration,
        secret: GpuiAskpassSecret,
    ) -> io::Result<Self> {
        let identity = PipeIdentity::of(&fs::symlink_metadata(&pipe)?)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let shared = Arc::new(Shared {
            pipe,
            identity,
            cancel: AtomicBool::new(false),
            state: Mutex::new(BrokerState::default()),
            changed: Condvar::new(),
        });
        let server = thread::Builder::new()
            .name("gpui-askpass-broker".to_string())
            .spawn({
                let shared = shared.clone();
                move || {
                    // Marked on every way out, a panic included, so a drop never waits for it.
                    let _done = ServerDone(shared.clone());
                    answer_helpers(&shared, secret);
                }
            })?;
        let server_thread = server.thread().clone();
        let mut broker = Self {
            shared,
            server: Some(server),
            watcher: None,
        };
        if serve == GpuiAskpassServe::Handshake {
            let shared = broker.shared.clone();
            // A failure here drops `broker`, which stops the broker thread.
            broker.watcher = Some(
                thread::Builder::new()
                    .name("gpui-askpass-window".to_string())
                    .spawn(move || {
                        if window_ended(&shared, Instant::now() + window) {
                            stop(&shared, &server_thread);
                        }
                    })?,
            );
        }
        Ok(broker)
    }
}

impl Drop for GpuiAskpassBroker {
    fn drop(&mut self) {
        self.shared.state().dropping = true;
        self.shared.changed.notify_all();
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
        if let Some(server) = self.server.take() {
            stop(&self.shared, server.thread());
            if self.shared.state().server_done {
                let _ = server.join();
            }
            // Otherwise nothing can reach the broker (its pipe is gone or was replaced), so it is
            // left asleep rather than holding up this drop. It holds no secret while it waits.
        }
    }
}

struct ServerDone(Arc<Shared>);

impl Drop for ServerDone {
    fn drop(&mut self) {
        self.0.state().server_done = true;
        self.0.changed.notify_all();
    }
}

/// Sleeps until `until`; `false` when the owner drops the broker first.
fn window_ended(shared: &Shared, until: Instant) -> bool {
    let mut state = shared.state();
    loop {
        if state.dropping {
            return false;
        }
        let now = Instant::now();
        if now >= until {
            return true;
        }
        state = shared
            .changed
            .wait_timeout(state, until - now)
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0;
    }
}

/// The broker thread. It sleeps in a blocking open until a helper opens the pipe for reading
/// (no wakeups while idle), answers that helper once, pauses, and goes back to the open.
fn answer_helpers(shared: &Shared, mut secret: GpuiAskpassSecret) {
    // Opens in a row that returned at once: a reader that stays rather than a new helper.
    let mut streak: u32 = 0;
    while !shared.cancelled() {
        let asked = Instant::now();
        let Ok(mut stream) = open_for_writing_when_read(&shared.pipe) else {
            break;
        };
        if shared.cancelled() || !shared.identity.is(&stream) || set_nonblocking(&stream).is_err() {
            break;
        }
        streak = if asked.elapsed() < Duration::from_millis(1) {
            streak + 1
        } else {
            0
        };
        if let Some(mut password) = secret() {
            // A drop that began during the keyring read may be what woke this open: its reader
            // must never get the secret.
            if shared.cancelled() {
                password.fill(0);
                break;
            }
            // One write, so the line reaches the pipe whole and a reader takes it in one read.
            let mut line = Vec::with_capacity(password.len() + 1);
            line.extend_from_slice(password.as_slice());
            line.push(b'\n');
            password.fill(0);
            let _ = stream.write_all(line.as_slice());
            line.fill(0);
        }
        // Closing the pipe ends the helper's read; with nothing written it reports the password
        // as unavailable.
        drop(stream);
        let pause = (SERVED_PAUSE * (1_u32 << streak.saturating_sub(2).min(8))).min(LONGEST_PAUSE);
        let until = Instant::now() + pause;
        while !shared.cancelled() {
            let now = Instant::now();
            if now >= until {
                break;
            }
            thread::park_timeout(until - now);
        }
    }
}

/// Ends the broker: sets `cancel`, wakes it from its pause or its open, waits for it when it
/// could be woken, and takes the pipe away so a later helper fails at once. Runs once.
fn stop(shared: &Shared, server: &thread::Thread) {
    {
        let mut state = shared.state();
        if state.stopped {
            return;
        }
        state.stopped = true;
    }
    shared.cancel.store(true, Ordering::Release);
    server.unpark();
    // The broker sleeps in a blocking open until the pipe has a reader: this one wakes it, and it
    // sees `cancel` and stops without writing. The reader is held until the broker has returned,
    // since on macOS a reader that comes and goes before the broker runs leaves it asleep. It is
    // never read.
    let wake = open_for_reading(&shared.pipe).filter(|wake| shared.identity.is(wake));
    if wake.is_some() {
        // Bounded: a broker still in a slow keyring read (or one a swapped pipe kept asleep) is
        // left to see `cancel` on its own rather than holding up the drop.
        let until = Instant::now() + STOP_WAIT;
        let mut state = shared.state();
        while !state.server_done {
            let now = Instant::now();
            if now >= until {
                break;
            }
            state = shared
                .changed
                .wait_timeout(state, until - now)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }
    drop(wake);
    retire_pipe(shared);
}

/// Moves the pipe aside, so no new helper can open it, and removes it. A helper that opened it
/// just before would wait for a writer forever: the pipe is held open for reading and writing for
/// `RETIRED_PIPE_HOLD`, which lets a helper already waiting, or one whose open lands in that time,
/// through to an empty read when it closes. Nothing is ever written to it.
fn retire_pipe(shared: &Shared) {
    // Only the FIFO made for this broker is moved; anything else at the path is left for the
    // owner's removal of the folder.
    let ours = fs::symlink_metadata(&shared.pipe)
        .ok()
        .and_then(|metadata| PipeIdentity::of(&metadata))
        .is_some_and(|identity| identity == shared.identity);
    let Some(directory) = shared.pipe.parent().filter(|_| ours) else {
        return;
    };
    let retired = directory.join(RETIRED_PIPE_NAME);
    if fs::rename(&shared.pipe, &retired).is_err() {
        return;
    }
    let hold = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(&retired)
        .ok()
        .filter(|hold| shared.identity.is(hold));
    let _ = fs::remove_file(&retired);
    if let Some(hold) = hold {
        // Held off the caller's thread: a drop may run on the UI thread.
        let _ = thread::Builder::new()
            .name("gpui-askpass-retire".to_string())
            .spawn(move || {
                thread::sleep(RETIRED_PIPE_HOLD);
                drop(hold);
            });
    }
}

/// Opens the pipe for writing, sleeping in the kernel until a reader has it open. Rust's `open`
/// retries an interrupted call itself.
fn open_for_writing_when_read(pipe: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(pipe)
}

/// Writes stay non-blocking, so a helper that stops reading can never hold the broker, or the drop
/// that waits for it.
fn set_nonblocking(stream: &fs::File) -> io::Result<()> {
    let fd = stream.as_raw_fd();
    // SAFETY: `fd` is the open descriptor `stream` owns for the duration of both calls.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn open_for_reading(pipe: &Path) -> Option<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(pipe)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::AtomicUsize;

    const SECRET: &[u8] = b"fake secret 'with' $pecial \\ chars ";

    struct Fixture {
        base: PathBuf,
        folder: GpuiAskpassFolder,
        reads: Arc<AtomicUsize>,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let base = env::temp_dir().join(format!(
                "gxap-test-{}-{name}-{:x}",
                std::process::id(),
                uuid::Uuid::new_v4().as_u128() as u64
            ));
            fs::create_dir(&base).unwrap();
            let folder = GpuiAskpassFolder::create(&base).unwrap();
            Self {
                base,
                folder,
                reads: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn secret(&self, delay: Duration) -> GpuiAskpassSecret {
            let reads = self.reads.clone();
            Box::new(move || {
                reads.fetch_add(1, Ordering::SeqCst);
                thread::sleep(delay);
                Some(SECRET.to_vec())
            })
        }

        fn broker(&self, serve: GpuiAskpassServe, window: Duration) -> GpuiAskpassBroker {
            GpuiAskpassBroker::start(
                self.folder.pipe.clone(),
                serve,
                window,
                self.secret(Duration::ZERO),
            )
            .unwrap()
        }

        /// Runs the helper as ssh does: its standard output is the answer.
        fn ask(&self) -> (bool, Vec<u8>) {
            let output = Command::new(&self.folder.script).output().unwrap();
            (output.status.success(), output.stdout)
        }

        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn answer() -> Vec<u8> {
        [SECRET, b"\n"].concat()
    }

    #[test]
    fn each_helper_gets_the_secret_once() {
        let fixture = Fixture::new("once");
        let broker = fixture.broker(GpuiAskpassServe::UntilDropped, Duration::ZERO);
        for _ in 0..3 {
            assert_eq!(fixture.ask(), (true, answer()));
        }
        drop(broker);
        // One read per helper; a helper slow to close after its line can cost one more, never
        // a second answer (it reads a single line).
        assert!((3..=6).contains(&fixture.reads()), "{}", fixture.reads());
    }

    #[test]
    fn a_drop_with_no_helper_reads_nothing_and_returns() {
        let fixture = Fixture::new("idle");
        let broker = fixture.broker(GpuiAskpassServe::UntilDropped, Duration::ZERO);
        thread::sleep(Duration::from_millis(20));
        let started = Instant::now();
        drop(broker);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(fixture.reads(), 0);
        assert!(!fixture.folder.pipe.exists());
    }

    #[test]
    fn the_sign_in_window_removes_the_pipe() {
        let fixture = Fixture::new("window");
        let broker = fixture.broker(GpuiAskpassServe::Handshake, Duration::from_millis(300));
        assert_eq!(fixture.ask(), (true, answer()));
        thread::sleep(Duration::from_millis(600));
        assert!(!fixture.folder.pipe.exists());
        assert!(broker.shared.state().server_done);
        let output = Command::new(&fixture.folder.script).output().unwrap();
        assert!(!output.status.success() && output.stdout.is_empty());
        // Its own message, not the "save the password again" one (connect.rs).
        assert!(String::from_utf8_lossy(&output.stderr).contains(GPUI_ASKPASS_WINDOW_ENDED_TEXT));
        drop(broker);
        assert_eq!(fixture.reads(), 1);
    }

    #[test]
    fn two_helpers_at_once_get_the_whole_answer_or_nothing() {
        let fixture = Fixture::new("pair");
        let broker = fixture.broker(GpuiAskpassServe::UntilDropped, Duration::ZERO);
        let mut answered = 0;
        for _ in 0..20 {
            let helpers: Vec<_> = (0..2)
                .map(|_| {
                    let script = fixture.folder.script.clone();
                    thread::spawn(move || Command::new(script).output().unwrap())
                })
                .collect();
            for helper in helpers {
                let output = helper.join().unwrap();
                if output.status.success() {
                    assert_eq!(output.stdout, answer());
                    answered += 1;
                } else {
                    assert!(output.stdout.is_empty());
                }
            }
        }
        drop(broker);
        assert!(answered >= 20, "{answered} of 40 answered");
    }

    #[test]
    fn a_helper_that_opens_as_the_pipe_is_retired_is_let_go() {
        let fixture = Fixture::new("retire");
        let broker = fixture.broker(GpuiAskpassServe::UntilDropped, Duration::ZERO);
        // Another name for the same FIFO stands in for a helper that looked the pipe up just
        // before it was moved away and opens it just after.
        let late = fixture.folder.directory.join("late");
        fs::hard_link(&fixture.folder.pipe, &late).unwrap();
        drop(broker);
        let started = Instant::now();
        let mut reader = fs::File::open(&late).unwrap();
        let mut read = Vec::new();
        io::Read::read_to_end(&mut reader, &mut read).unwrap();
        assert!(read.is_empty());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(fixture.reads(), 0);
    }

    #[test]
    fn a_drop_never_waits_long_for_a_slow_keyring_read() {
        let fixture = Fixture::new("slow");
        let broker = GpuiAskpassBroker::start(
            fixture.folder.pipe.clone(),
            GpuiAskpassServe::UntilDropped,
            Duration::ZERO,
            fixture.secret(STOP_WAIT + Duration::from_millis(1_500)),
        )
        .unwrap();
        let script = fixture.folder.script.clone();
        let helper = thread::spawn(move || Command::new(script).output().unwrap());
        thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        drop(broker);
        let waited = started.elapsed();
        assert!(
            waited < STOP_WAIT + Duration::from_millis(500),
            "{waited:?}"
        );
        // The broker finishes its read later, sees `cancel` and writes nothing.
        let output = helper.join().unwrap();
        assert!(!output.status.success() && output.stdout.is_empty());
        assert_eq!(fixture.reads(), 1);
    }

    #[test]
    fn a_reader_that_stays_is_not_flooded() {
        let fixture = Fixture::new("stuck");
        let broker = fixture.broker(GpuiAskpassServe::UntilDropped, Duration::ZERO);
        let stuck = fs::OpenOptions::new()
            .read(true)
            .open(&fixture.folder.pipe)
            .unwrap();
        thread::sleep(Duration::from_millis(1_000));
        let reads = fixture.reads();
        drop(broker);
        drop(stuck);
        // The pause doubles from the third answer in a row: about ten answers in the first
        // second, where the old loop gave thousands.
        assert!((1..=20).contains(&reads), "{reads} keyring reads");
    }

    #[test]
    fn a_replaced_pipe_is_never_written() {
        let fixture = Fixture::new("swap");
        let broker = fixture.broker(GpuiAskpassServe::UntilDropped, Duration::ZERO);
        let decoy = fixture.folder.directory.join("decoy");
        fs::write(&decoy, b"").unwrap();
        fs::rename(&decoy, &fixture.folder.pipe).unwrap();
        // The broker is asleep on the FIFO that was moved away; a helper now opens the file.
        let _ = fixture.ask();
        thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        drop(broker);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(fixture.reads(), 0);
        assert_eq!(fs::read(&fixture.folder.pipe).unwrap(), b"");
    }

    #[test]
    fn a_drop_during_the_keyring_read_writes_nothing() {
        let fixture = Fixture::new("cancel");
        let broker = GpuiAskpassBroker::start(
            fixture.folder.pipe.clone(),
            GpuiAskpassServe::UntilDropped,
            Duration::ZERO,
            fixture.secret(Duration::from_millis(300)),
        )
        .unwrap();
        let script = fixture.folder.script.clone();
        let helper = thread::spawn(move || Command::new(script).output().unwrap());
        thread::sleep(Duration::from_millis(100));
        drop(broker);
        let output = helper.join().unwrap();
        assert!(!output.status.success() && output.stdout.is_empty());
        assert_eq!(fixture.reads(), 1);
    }

    #[test]
    fn folders_made_together_never_share_a_name() {
        let fixture = Fixture::new("names");
        let base = Arc::new(fixture.base.clone());
        let folders: Vec<PathBuf> = (0..8)
            .map(|_| {
                let base = base.clone();
                thread::spawn(move || {
                    (0..25)
                        .map(|_| GpuiAskpassFolder::create(&base).unwrap().directory)
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect();
        let mut unique = folders.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 200);
        let mode = fs::metadata(&folders[0]).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn folders_of_dead_runs_are_swept() {
        let fixture = Fixture::new("sweep");
        // A pid far above any real one is never alive.
        let dead = fixture.base.join("gxap-99999999-1a2b");
        let alive = fixture
            .base
            .join(format!("gxap-{}-0-00ff", std::process::id()));
        let other = fixture.base.join("gxap-notes");
        for directory in [&dead, &alive, &other] {
            fs::create_dir(directory).unwrap();
        }
        sweep_stale_askpass_directories_in(&fixture.base);
        assert!(!dead.exists());
        assert!(alive.exists() && other.exists());
    }
}
