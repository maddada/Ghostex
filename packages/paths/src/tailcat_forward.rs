//! One Easy Connect link to a computer: a loopback listener whose connections all ride one
//! supervised `tailcat forward` client, which is started again when it exits or stops carrying.
//!
//! CDXC:RemotePairing 2026-11-10 WHY:
//! GitHub #526: one `tailcat <address> <port>` client per accepted connection, all dialing with one key, stalled each other (a new one starves the older ones, whose connections never close), so each link runs one long-lived `tailcat forward` client that carries every connection. The desktop app (`apps/desktop/src/app/helpers/remote/easy_connect_forward.rs`) and gxserver's own tunnels (`server/src/ghostex_cli/tailcat_tunnel.rs`) each had a copy of that; this is the one both run now.
//!
//! CDXC:RemotePairing 2026-11-10 WHY:
//! A user reported "a lot of reconnects" between app and remote. Measured with tailcat v0.6.0: when the far computer's `tailcat serve` starts again (its gxserver restarted or updated, a pairing code was shown, a key was added to its allow-list), a client that was connected never comes back by itself. The new server process does not know it, the client never says hello again, and its connections, old and new, hang without closing (2.5 minutes watched; a fresh client connected in 1.6 s). The desktop came back only after SSH's own keepalive gave up (45 s), the watchdog's next look (up to 15 s) and a whole reconnect; gxserver's tunnels never did. The link now notices on its own: bytes sent with nothing coming back for [`SUSPECT_AFTER`] make it open one connection of its own through the client ([`ForwardProbe`]), and when that hears nothing either the client is started again, with the reason in a [`ForwardEvent`]. A client that works is never restarted: any byte from the far side clears the suspicion, on whichever connection it arrives.
//! A network that is down looks the same from inside the client (nothing comes back), and there a restart would only cut connections that resume by themselves when it returns, as they always did. So before a quiet client is replaced the link asks whether this computer has a way to the address's relay at all ([`relay_hosts`], `network_is_down`): when it has none, the network is the problem and the client is left alone.
//! This replaces the desktop's earlier rule (a fresh connection that heard nothing for 20 s restarted the client), which could end every working connection of a machine because one new connection was slow.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const FORWARD_HOST: &str = "127.0.0.1";
/// How long the accept loop sleeps between non-blocking accepts; this is also the worst-case
/// delay before the listener is released after a stop.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const PIPE_BUFFER_BYTES: usize = 32 * 1024;
/// How long an accepted connection waits for the tailcat client to be listening.
const CLIENT_READY_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the supervisor looks at its tailcat client, the link's health and the stop flag.
const CLIENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CLIENT_RESTART_MIN: Duration = Duration::from_millis(500);
const CLIENT_RESTART_MAX: Duration = Duration::from_secs(10);
/// A tailcat client that ran this long was healthy: its next restart starts the backoff over.
const CLIENT_STABLE_AFTER: Duration = Duration::from_secs(30);
/// Bytes sent to the far side with nothing coming back for this long, on a client that carried
/// bytes before, make the link check the client itself. SSH through the link answers a keepalive
/// every few seconds, so a working link stays far below this.
pub const SUSPECT_AFTER: Duration = Duration::from_secs(6);
/// How long the link's own check waits for the far side before the client counts as carrying nothing.
const PROBE_TIMEOUT: Duration = Duration::from_secs(6);
/// A client that never carried a byte is given this long from its first unanswered connection: a
/// first rendezvous over a relay takes up to about ten seconds.
const NEVER_CARRIED_TIMEOUT: Duration = Duration::from_secs(20);

/// How the link checks, on its own connection, that its client still reaches the far side.
#[derive(Clone, Copy, Debug)]
pub enum ForwardProbe {
    /// The far port speaks first (SSH's banner): connect and wait.
    ServerSpeaksFirst,
    /// The far port is HTTP (gxserver): ask for this path; any answer counts, whatever its status.
    HttpGet(&'static str),
}

/// What to forward and with which helper.
#[derive(Clone, Debug)]
pub struct ForwardSpec {
    pub binary_path: PathBuf,
    pub address_blob: String,
    pub remote_port: u16,
    pub probe: ForwardProbe,
}

/// Why a tailcat client ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientEnd {
    /// The link was stopped.
    Stopped,
    /// The helper could not be started.
    SpawnFailed,
    /// The helper exited by itself.
    Exited,
    /// It carried bytes before, then nothing came back, and the link's own check heard nothing.
    CarriedNothing,
    /// It never carried a byte within [`NEVER_CARRIED_TIMEOUT`] of a connection asking.
    NeverCarried,
    /// The owner asked for a fresh one while nothing rode on it ([`TailcatForward::refresh_idle_client`]).
    Refreshed,
}

impl ClientEnd {
    /// The word logs use for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::SpawnFailed => "helperDidNotStart",
            Self::Exited => "helperExited",
            Self::CarriedNothing => "carriedNothing",
            Self::NeverCarried => "neverCarried",
            Self::Refreshed => "refreshed",
        }
    }
}

/// What happened to the link's tailcat client, for the owner's log.
#[derive(Clone, Copy, Debug)]
pub enum ForwardEvent {
    /// The client listens on its loopback port, `after` its start.
    ClientListening { after: Duration, own_key: bool },
    /// The client ended after `uptime`, taking `connections_cut` open connections with it; unless
    /// the reason is [`ClientEnd::Stopped`] another one starts.
    ClientEnded {
        reason: ClientEnd,
        uptime: Duration,
        connections_cut: usize,
    },
}

/// Names the key file the next client dials with, or `None` for tailcat's `client-default`. Called
/// on the supervisor thread before each client start; it may block.
pub type ClientKeyProvider = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;
pub type ForwardEventSink = Arc<dyn Fn(ForwardEvent) + Send + Sync>;

/// Both sockets of one relayed connection, so `stop` can end it.
struct LiveConnection {
    client: TcpStream,
    upstream: TcpStream,
}

#[derive(Default)]
struct ClientState {
    /// The loopback port `tailcat forward` listens on while it runs.
    port: Option<u16>,
    /// Its pid, so `stop` can end it at once.
    pid: Option<u32>,
}

/// What the pumps saw, in milliseconds since `epoch` (0: nothing).
struct Health {
    epoch: Instant,
    /// The last byte from the far side through the current client.
    last_rx_ms: AtomicU64,
    /// The first send (or connection to a port that speaks first) no byte has answered yet.
    unanswered_since_ms: AtomicU64,
}

impl Health {
    fn now_ms(&self) -> u64 {
        // Never 0, which means "nothing".
        self.epoch.elapsed().as_millis() as u64 + 1
    }

    fn reset(&self) {
        self.last_rx_ms.store(0, Ordering::SeqCst);
        self.unanswered_since_ms.store(0, Ordering::SeqCst);
    }

    /// Something was sent that the far side should answer.
    fn note_sent(&self) {
        let _ = self.unanswered_since_ms.compare_exchange(
            0,
            self.now_ms(),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }

    /// Bytes came from the far side.
    fn note_received(&self) {
        self.last_rx_ms.store(self.now_ms(), Ordering::SeqCst);
        self.unanswered_since_ms.store(0, Ordering::SeqCst);
    }

    /// The far side closed a connection: an answer of its own, though it carried no byte.
    fn note_answered(&self) {
        self.unanswered_since_ms.store(0, Ordering::SeqCst);
    }

    fn carried(&self) -> bool {
        self.last_rx_ms.load(Ordering::SeqCst) != 0
    }

    fn unanswered_for(&self) -> Option<Duration> {
        match self.unanswered_since_ms.load(Ordering::SeqCst) {
            0 => None,
            since => Some(Duration::from_millis(self.now_ms().saturating_sub(since))),
        }
    }
}

struct Shared {
    shutdown: AtomicBool,
    /// The owner asked for a fresh client.
    refresh: AtomicBool,
    connections: Mutex<HashMap<u64, LiveConnection>>,
    client: (Mutex<ClientState>, Condvar),
    health: Health,
}

impl Shared {
    fn stopping(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    fn client_state(&self) -> std::sync::MutexGuard<'_, ClientState> {
        self.client
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_client_port(&self, port: Option<u16>) {
        self.client_state().port = port;
        self.client.1.notify_all();
    }

    fn live_connections(&self) -> usize {
        self.connections
            .lock()
            .map(|connections| connections.len())
            .unwrap_or(0)
    }
}

/// A running link. Dropping it does not stop it: call [`TailcatForward::stop`].
pub struct TailcatForward {
    local_port: u16,
    shared: Arc<Shared>,
    accept_thread: Option<JoinHandle<()>>,
    supervisor_thread: Option<JoinHandle<()>>,
}

impl TailcatForward {
    /// Starts the link on `listener` (a bound loopback listener).
    pub fn start(
        listener: TcpListener,
        spec: ForwardSpec,
        client_key: ClientKeyProvider,
        events: ForwardEventSink,
    ) -> std::io::Result<Self> {
        let local_port = listener.local_addr()?.port();
        // Non-blocking accepts are what let the loop observe the stop flag and release the port
        // instead of parking forever inside `accept`.
        listener.set_nonblocking(true)?;
        let shared = Arc::new(Shared {
            shutdown: AtomicBool::new(false),
            refresh: AtomicBool::new(false),
            connections: Mutex::new(HashMap::new()),
            client: (Mutex::default(), Condvar::new()),
            health: Health {
                epoch: Instant::now(),
                last_rx_ms: AtomicU64::new(0),
                unanswered_since_ms: AtomicU64::new(0),
            },
        });
        let speaks_first = matches!(spec.probe, ForwardProbe::ServerSpeaksFirst);
        let supervisor_thread = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || supervise_client(spec, shared, client_key, events))
        };
        let accept_thread = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || accept_loop(listener, shared, speaks_first))
        };
        Ok(Self {
            local_port,
            shared,
            accept_thread: Some(accept_thread),
            supervisor_thread: Some(supervisor_thread),
        })
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Starts a fresh tailcat client when no connection rides on the current one, and says whether
    /// it did. A client that sat unused may have lost the far side without anything to show it
    /// (CDXC:RemotePairing 2026-11-10 at the top of this file), and a fresh one costs nobody a
    /// connection, so an owner about to dial through an idle link asks for this first.
    pub fn refresh_idle_client(&self) -> bool {
        if self.shared.live_connections() > 0 {
            return false;
        }
        // New connections wait for the next client from here on.
        self.shared.client_state().port = None;
        self.shared.refresh.store(true, Ordering::SeqCst);
        true
    }

    /// Ends the link: its tailcat client and every connection it carries. Returns at once; the
    /// listener is released within one accept poll.
    pub fn stop(&self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        /*
        The tailcat client is killed here, synchronously: it does not exit by itself while its
        tunnel link stays up, and when `stop` runs during process exit the supervisor thread never
        gets scheduled again to reap it, which would orphan a live tailcat process.
        */
        if let Some(pid) = self.shared.client_state().pid.take() {
            kill_pid(pid);
        }
        self.shared.client.1.notify_all();
        // Unblocks both pumps of every in-flight connection so their threads can finish.
        if let Ok(mut connections) = self.shared.connections.lock() {
            for (_, connection) in connections.drain() {
                let _ = connection.client.shutdown(Shutdown::Both);
                let _ = connection.upstream.shutdown(Shutdown::Both);
            }
        }
    }

    /// [`TailcatForward::stop`], then waits for the listener to be released and the client reaped,
    /// so an immediate restart can bind the same port.
    pub fn stop_and_wait(mut self) {
        self.stop();
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.supervisor_thread.take() {
            let _ = thread.join();
        }
    }
}

/// Ends the client started as `pid`: on Unix that is the lifeline shell, whose process group holds
/// the helper too.
#[cfg(unix)]
fn kill_pid(pid: u32) {
    if let Ok(pid) = i32::try_from(pid) {
        if pid > 0 {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let _ = background_command("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(any(unix, windows)))]
fn kill_pid(_pid: u32) {}

/// A command that opens no console window on Windows.
#[cfg(not(unix))]
fn background_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Runs the link's `tailcat forward` until the link stops, starting it again whenever it ends.
fn supervise_client(
    spec: ForwardSpec,
    shared: Arc<Shared>,
    client_key: ClientKeyProvider,
    events: ForwardEventSink,
) {
    let relays = relay_hosts(&spec);
    let mut backoff = CLIENT_RESTART_MIN;
    while !shared.stopping() {
        let started = Instant::now();
        let key = client_key();
        if shared.stopping() {
            break;
        }
        let (reason, connections_cut) = run_client_once(&spec, key, &shared, &events, &relays);
        let uptime = started.elapsed();
        events(ForwardEvent::ClientEnded {
            reason,
            uptime,
            connections_cut,
        });
        if reason == ClientEnd::Stopped {
            break;
        }
        let stable = uptime >= CLIENT_STABLE_AFTER;
        if stable {
            backoff = CLIENT_RESTART_MIN;
        }
        // A client that worked and then went quiet is replaced at once, as is one the owner asked
        // to replace: a connection is waiting for the next.
        let at_once =
            reason == ClientEnd::Refreshed || (stable && reason == ClientEnd::CarriedNothing);
        if !at_once {
            let deadline = Instant::now() + backoff;
            while Instant::now() < deadline && !shared.stopping() {
                thread::sleep(CLIENT_POLL_INTERVAL);
            }
        }
        backoff = (backoff * 2).min(CLIENT_RESTART_MAX);
    }
}

/// One run of `tailcat forward --bind=127.0.0.1 <address> 0:<port>`: its loopback port is
/// published once it prints it, and the run ends when it exits, the link stops, or it carries
/// nothing.
fn run_client_once(
    spec: &ForwardSpec,
    key: Option<PathBuf>,
    shared: &Arc<Shared>,
    events: &ForwardEventSink,
    relays: &[String],
) -> (ClientEnd, usize) {
    let own_key = key.is_some();
    let Ok(mut child) = spawn_client(spec, key) else {
        return (ClientEnd::SpawnFailed, 0);
    };
    // Held open for as long as this client may run (`CLIENT_LIFELINE_SCRIPT`).
    let _lifeline = child.stdin.take();
    shared.health.reset();
    // A fresh client is what a pending ask for one wanted.
    shared.refresh.store(false, Ordering::SeqCst);
    {
        let mut state = shared.client_state();
        // `stop` ran while this client started: it never saw the pid, so it goes now.
        if shared.stopping() {
            drop(state);
            end_child(&mut child);
            return (ClientEnd::Stopped, 0);
        }
        state.pid = Some(child.id());
    }
    let started = Instant::now();
    // tailcat prints "forwarding 127.0.0.1:<port> -> remote …" once it listens, then its log;
    // the reader keeps draining it so the pipe never fills.
    let reader = child.stderr.take().map(|stderr| {
        let shared = Arc::clone(shared);
        let events = Arc::clone(events);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { return };
                if let Some(port) = forward_listen_port(&line) {
                    shared.set_client_port(Some(port));
                    events(ForwardEvent::ClientListening {
                        after: started.elapsed(),
                        own_key,
                    });
                }
            }
        })
    });
    let reason = loop {
        if shared.stopping() {
            break ClientEnd::Stopped;
        }
        if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
            break ClientEnd::Exited;
        }
        if shared.refresh.swap(false, Ordering::SeqCst) {
            break ClientEnd::Refreshed;
        }
        if let Some(reason) = client_carries_nothing(spec, shared, relays) {
            break reason;
        }
        thread::sleep(CLIENT_POLL_INTERVAL);
    };
    let connections_cut = shared.live_connections();
    end_child(&mut child);
    {
        let mut state = shared.client_state();
        state.port = None;
        state.pid = None;
    }
    shared.client.1.notify_all();
    if let Some(reader) = reader {
        let _ = reader.join();
    }
    // The connections it carried end with it; a stopped link's are drained by `stop`.
    if reason != ClientEnd::Stopped {
        cut_connections(shared);
    }
    (reason, connections_cut)
}

/// The variable that turns off UDP send batching (GSO) in tailcat's data plane. It is a Tailscale
/// debug knob, so its name is pinned by a test here.
pub const TAILCAT_DISABLE_UDP_GSO_ENV: &str = "TS_DEBUG_DISABLE_UDP_GSO";

/*
CDXC:RemotePairing 2026-11-14 DECISION:
A user's Easy Connect link from a Mac to a Linux computer on the same LAN carried 12 to 18 KB/s and dropped every 2 to 8 minutes ("healthUnanswered": health checks queued behind a minute of other data on the one SSH stream). Measured with tailcat v0.6.0 on throwaway clients: the link was direct, but through each computer's Tailscale address (fd7a:115c:a1e0::/48), so tailcat's WireGuard ran inside Tailscale's, over a 1280-byte MTU. tailcat's packets are about 1440 bytes there, and Linux's batched (GSO) sends of them crawl, while plain UDP of the same size passes: Mac to Linux ran 25.7 MB/s, Linux to Mac 13 KB/s, and with this variable set on the Linux side 15 to 20 MB/s. User (Q3204 A): every tailcat Ghostex starts on Linux runs with UDP GSO off; macOS and Windows are unchanged. The cost is a little more CPU on Linux at high rates. The tailcat behavior itself (using a peer's Tailscale address as a direct path) is drafted as an upstream issue.
*/
/// The environment a tailcat helper that carries links (`serve`, `forward`) gets on `os`
/// (`std::env::consts::OS`).
fn tailcat_link_environment_for(os: &str) -> &'static [(&'static str, &'static str)] {
    if os == "linux" {
        &[(TAILCAT_DISABLE_UDP_GSO_ENV, "true")]
    } else {
        &[]
    }
}

/// Gives `command`, a tailcat `serve` or `forward`, this computer's link environment
/// (CDXC:RemotePairing 2026-11-14 above). gxserver's `tailcat serve` uses it too.
pub fn apply_tailcat_link_environment(command: &mut Command) {
    for (name, value) in tailcat_link_environment_for(std::env::consts::OS) {
        command.env(name, value);
    }
}

/// `tailcat [--key=<file>] forward --bind=127.0.0.1 <address> 0:<port>`, tied to this process.
fn spawn_client(spec: &ForwardSpec, key: Option<PathBuf>) -> std::io::Result<Child> {
    let mut arguments: Vec<std::ffi::OsString> = Vec::new();
    if let Some(key) = key {
        arguments.push(format!("--key={}", key.display()).into());
    }
    arguments.push("forward".into());
    arguments.push(format!("--bind={FORWARD_HOST}").into());
    arguments.push(spec.address_blob.as_str().into());
    arguments.push(format!("0:{}", spec.remote_port).into());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(CLIENT_LIFELINE_SCRIPT)
            .arg("ghostex-easy-connect")
            .arg(&spec.binary_path)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // Its own group, so the shell, its reader and the helper are ended together.
            .process_group(0);
        // The shell passes it on to the helper.
        apply_tailcat_link_environment(&mut command);
        command.spawn()
    }
    #[cfg(not(unix))]
    {
        let mut command = background_command(&spec.binary_path);
        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        apply_tailcat_link_environment(&mut command);
        let child = command.spawn()?;
        #[cfg(windows)]
        end_with_this_process(&child);
        Ok(child)
    }
}

/*
CDXC:RemotePairing 2026-11-10 WHY:
A tailcat client does not exit while its tunnel stays up, so one whose owner died without stopping it (Ghostex crashed or was force-quit, a `ghostex` command was killed by a timeout) ran on for good, holding its key; the next client with that key then stalled every few seconds, because two clients with one key take turns starving each other (GitHub #526; seen again on 2026-11-10 when a killed test owner left its client behind and the next one was restarted as carrying nothing every 17 to 44 s). On macOS and Linux the helper now runs under a small shell that holds this process's pipe as its standard input and ends the helper the moment that pipe closes, which the system does when this process dies in any way. Windows does not end a child with its parent either and has no such pipe trick, so there the helper joins a job that only this process holds (`end_with_this_process`).
*/
/// Runs `"$@"` (the helper and its arguments) and ends it when standard input closes; exits with
/// the helper's status when the helper exits first.
#[cfg(unix)]
const CLIENT_LIFELINE_SCRIPT: &str = r#"exec 3<&0
"$@" </dev/null 3<&- &
child=$!
( read -r _ <&3; kill -9 "$child" 2>/dev/null ) &
reader=$!
wait "$child"
status=$?
kill "$reader" 2>/dev/null
exit "$status""#;

/// Ends `child` when this process ends, however it ends.
///
/// CDXC:RemotePairing 2026-11-10 WHY:
/// The coordinator's brief for dependable device connections: "a Windows helper ends with its owner". Until now only macOS and Linux tied the helper to its owner (the lifeline above), so on Windows a Ghostex that crashed or was ended from Task Manager, or a `ghostex` command killed by a timeout, left its tailcat client running with its key, and the next client with that key was starved by it. The helper joins a job with kill-on-close whose only handle this process holds and never closes: when this process ends in any way, Windows closes the handle and ends the job's members. gxserver has ended its other helpers this way since 2026-10-24 (`end_with_server` in server/src/platform/process.rs, GitHub #289). The handle is not inheritable, so the helper itself cannot keep the job alive. A helper that could not join (the job could not be made) runs as before.
#[cfg(windows)]
fn end_with_this_process(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
    };
    // Never closed here: the system closes it when this process ends.
    static CLIENT_JOB: OnceLock<usize> = OnceLock::new();
    let job = *CLIENT_JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return 0;
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            CloseHandle(job);
            return 0;
        }
        job as usize
    });
    if job == 0 {
        return;
    }
    unsafe {
        AssignProcessToJobObject(job as _, child.as_raw_handle() as _);
    }
}

fn end_child(child: &mut Child) {
    kill_pid(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

/// Ends every relayed connection: the client they rode on is gone.
fn cut_connections(shared: &Shared) {
    if let Ok(connections) = shared.connections.lock() {
        for connection in connections.values() {
            let _ = connection.client.shutdown(Shutdown::Both);
            let _ = connection.upstream.shutdown(Shutdown::Both);
        }
    }
}

/// Whether the running client stopped reaching the far side, and how that is called. `relays` are
/// the address's relays ([`relay_hosts`]).
fn client_carries_nothing(
    spec: &ForwardSpec,
    shared: &Shared,
    relays: &[String],
) -> Option<ClientEnd> {
    let unanswered = shared.health.unanswered_for()?;
    if !shared.health.carried() {
        return (unanswered >= NEVER_CARRIED_TIMEOUT).then_some(ClientEnd::NeverCarried);
    }
    if unanswered < SUSPECT_AFTER {
        return None;
    }
    let port = shared.client_state().port?;
    match probe_client(port, spec.probe, shared) {
        ProbeOutcome::Answered => {
            shared.health.note_answered();
            None
        }
        ProbeOutcome::Silent if network_is_down(relays) => {
            // Nothing a new client could do better; ask again after the next unanswered send.
            shared.health.note_answered();
            None
        }
        ProbeOutcome::Silent => Some(ClientEnd::CarriedNothing),
        // The link is stopping; the caller's loop sees that next.
        ProbeOutcome::Aborted => None,
    }
}

/// How long the look for a way to the relay may take: it is the name lookup, which has no timeout
/// of its own.
const RELAY_CHECK_TIMEOUT: Duration = Duration::from_secs(4);

/*
CDXC:RemotePairing 2026-11-10 WHY:
The first version of this look opened a TCP connection to the relay's port 443. On 2026-11-10 Ghostex's relay was crash-looping because its accept-rate limit was exceeded by reconnecting clients, and a connection per quiet link, sent by every client at the moment a relay fails, is exactly what such a relay cannot take. The look therefore sends the relay nothing: it resolves the relay's name and asks the system for a route to it (connecting a UDP socket sends no packet). No answer to the name lookup or no route means this computer is offline.
*/
/// Whether this computer has no way to any of `relays`: their names do not resolve, or the system
/// has no route to them. `false` when the address names no relay.
fn network_is_down(relays: &[String]) -> bool {
    if relays.is_empty() {
        return false;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    for relay in relays.iter().cloned() {
        let tx = tx.clone();
        // On a thread that is left behind when the lookup takes too long.
        thread::spawn(move || {
            use std::net::ToSocketAddrs;
            let routed = (relay.as_str(), 443)
                .to_socket_addrs()
                .map(|addresses| addresses.take(4).any(|address| has_route_to(&address)))
                .unwrap_or(false);
            let _ = tx.send(routed);
        });
    }
    drop(tx);
    let deadline = Instant::now() + RELAY_CHECK_TIMEOUT;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(true) => return false,
            Ok(false) => continue,
            // Every look failed, or none finished in time.
            Err(_) => return true,
        }
    }
}

/// Whether the system would send a packet to `address` somewhere. Sends none.
fn has_route_to(address: &std::net::SocketAddr) -> bool {
    let local = if address.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    std::net::UdpSocket::bind(local).is_ok_and(|socket| socket.connect(address).is_ok())
}

/// The relay host names in `address_blob`, as `tailcat parse` prints them (`Region[].Nodes[].HostName`);
/// empty for an address that names a relay region by number only, or when the helper says nothing.
fn relay_hosts(spec: &ForwardSpec) -> Vec<String> {
    let mut command = Command::new(&spec.binary_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let Ok(output) = command
        .arg("parse")
        .arg(&spec.address_blob)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return Vec::new();
    };
    parsed["Region"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|region| region["Nodes"].as_array().into_iter().flatten())
        .filter_map(|node| node["HostName"].as_str())
        .filter(|host| !host.is_empty())
        .map(str::to_string)
        .collect()
}

enum ProbeOutcome {
    Answered,
    Silent,
    Aborted,
}

/// Opens one connection through the client and waits up to [`PROBE_TIMEOUT`] for the far side to
/// say anything or to close it.
fn probe_client(port: u16, probe: ForwardProbe, shared: &Shared) -> ProbeOutcome {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(2)) else {
        // The client no longer listens: it is exiting, which the supervisor sees by itself.
        return ProbeOutcome::Aborted;
    };
    if let ForwardProbe::HttpGet(path) = probe {
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: {FORWARD_HOST}\r\nConnection: close\r\n\r\n");
        if stream.write_all(request.as_bytes()).is_err() {
            return ProbeOutcome::Aborted;
        }
    }
    if stream.set_read_timeout(Some(CLIENT_POLL_INTERVAL)).is_err() {
        return ProbeOutcome::Aborted;
    }
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let mut buffer = [0u8; 64];
    let outcome = loop {
        if shared.stopping() {
            break ProbeOutcome::Aborted;
        }
        // A byte on any relayed connection meanwhile proves the client as well.
        if shared.health.unanswered_for().is_none() {
            break ProbeOutcome::Answered;
        }
        match stream.read(&mut buffer) {
            // Bytes, or the far side closing the connection: both are answers.
            Ok(_) => break ProbeOutcome::Answered,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                if Instant::now() >= deadline {
                    break ProbeOutcome::Silent;
                }
            }
            // The client reset it: it is going away.
            Err(_) => break ProbeOutcome::Aborted,
        }
    };
    let _ = stream.shutdown(Shutdown::Both);
    outcome
}

/// The loopback port in tailcat forward's "forwarding 127.0.0.1:<port> -> …" line.
fn forward_listen_port(line: &str) -> Option<u16> {
    let rest = line.split("forwarding ").nth(1)?;
    let address = rest.split_whitespace().next()?;
    address.rsplit(':').next()?.parse().ok()
}

/// The tailcat client's loopback port, waiting up to `CLIENT_READY_TIMEOUT` for it to listen.
fn wait_for_client(shared: &Shared) -> Option<u16> {
    let deadline = Instant::now() + CLIENT_READY_TIMEOUT;
    let mut state = shared.client_state();
    loop {
        if shared.stopping() {
            return None;
        }
        if let Some(port) = state.port {
            return Some(port);
        }
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        let wait = (deadline - now).min(Duration::from_millis(200));
        state = shared
            .client
            .1
            .wait_timeout(state, wait)
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .0;
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>, speaks_first: bool) {
    let mut next_id: u64 = 0;
    loop {
        if shared.stopping() {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let id = next_id;
                next_id = next_id.wrapping_add(1);
                let shared = Arc::clone(&shared);
                thread::spawn(move || serve_connection(id, stream, shared, speaks_first));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

fn serve_connection(id: u64, stream: TcpStream, shared: Arc<Shared>, speaks_first: bool) {
    // Accepted sockets inherit the listener's non-blocking mode on macOS, so the pumps must put
    // this one back into blocking mode before reading.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let Some(port) = wait_for_client(&shared) else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let Ok(upstream) = TcpStream::connect((FORWARD_HOST, port)) else {
        // The client went away between printing its port and this connect; the supervisor starts
        // it again.
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let (Ok(registered_client), Ok(registered_upstream)) =
        (stream.try_clone(), upstream.try_clone())
    else {
        let _ = stream.shutdown(Shutdown::Both);
        let _ = upstream.shutdown(Shutdown::Both);
        return;
    };
    shared
        .connections
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            id,
            LiveConnection {
                client: registered_client,
                upstream: registered_upstream,
            },
        );
    // `stop` sets the flag before draining the registry, so a connection that registered after
    // the drain sees the flag here and tears itself down.
    if !shared.stopping() {
        if speaks_first {
            // The far side owes this connection its first bytes.
            shared.health.note_sent();
        }
        pump_connection(&stream, &upstream, &shared.health);
    }
    let _ = stream.shutdown(Shutdown::Both);
    let _ = upstream.shutdown(Shutdown::Both);
    if let Ok(mut live) = shared.connections.lock() {
        live.remove(&id);
    }
}

/// Relays one connection both ways until either side ends.
fn pump_connection(stream: &TcpStream, upstream: &TcpStream, health: &Health) {
    let (Ok(mut client_reader), Ok(mut upstream_writer)) =
        (stream.try_clone(), upstream.try_clone())
    else {
        return;
    };
    thread::scope(|scope| {
        let uploader = scope.spawn(move || {
            copy_stream(&mut client_reader, &mut upstream_writer, || {
                health.note_sent()
            });
            // Half close: the client is done sending, and the far side sees the same
            // end-of-request it would have seen on a direct socket.
            let _ = upstream_writer.shutdown(Shutdown::Write);
        });
        let (Ok(mut upstream_reader), Ok(mut downstream)) =
            (upstream.try_clone(), stream.try_clone())
        else {
            let _ = stream.shutdown(Shutdown::Both);
            let _ = upstream.shutdown(Shutdown::Both);
            let _ = uploader.join();
            return;
        };
        copy_stream(&mut upstream_reader, &mut downstream, || {
            health.note_received()
        });
        // The far side (or the client) ended it, which answers whatever this connection sent.
        health.note_answered();
        let _ = downstream.shutdown(Shutdown::Write);
        // Nothing more can travel in either direction; closing both read halves releases the
        // uploader if it is still parked on `read`.
        let _ = downstream.shutdown(Shutdown::Both);
        let _ = upstream_reader.shutdown(Shutdown::Both);
        let _ = uploader.join();
    });
}

fn copy_stream(reader: &mut impl Read, writer: &mut impl Write, mut moved: impl FnMut()) {
    let mut buffer = vec![0u8; PIPE_BUFFER_BYTES];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => {
                if writer.write_all(&buffer[..count]).is_err() {
                    return;
                }
                if writer.flush().is_err() {
                    return;
                }
                moved();
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailcat_link_environment_turns_udp_gso_off_on_linux_only() {
        // The Tailscale knob tailcat reads; a renamed knob would silently stop working.
        assert_eq!(TAILCAT_DISABLE_UDP_GSO_ENV, "TS_DEBUG_DISABLE_UDP_GSO");
        assert_eq!(
            tailcat_link_environment_for("linux"),
            &[("TS_DEBUG_DISABLE_UDP_GSO", "true")]
        );
        for os in ["macos", "windows", "android", "ios", "freebsd"] {
            assert!(tailcat_link_environment_for(os).is_empty(), "{os}");
        }

        let mut command = Command::new("tailcat");
        apply_tailcat_link_environment(&mut command);
        let gso = command
            .get_envs()
            .find(|(name, _)| *name == std::ffi::OsStr::new(TAILCAT_DISABLE_UDP_GSO_ENV))
            .and_then(|(_, value)| value.map(|value| value.to_os_string()));
        if cfg!(target_os = "linux") {
            assert_eq!(gso.as_deref(), Some(std::ffi::OsStr::new("true")));
        } else {
            assert_eq!(gso, None);
        }
    }
}
