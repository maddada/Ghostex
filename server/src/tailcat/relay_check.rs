//! Noticing that a helper no longer hears its relay, and starting it again.
//!
//! CDXC:RemotePairing 2026-11-11 WHY:
//! GitHub #526 ("Easy Connect remote machine keeps dropping"): the reporter saw a link carry nothing for about 70 s, once for 5 minutes, while both computers read connected, and the user asked for the silent periods to be found and fixed without the reporter's logs. Reproduced in the lab: when a helper's connection to its relay stops carrying (a path that drops packets without closing the connection: a router that lost its NAT table, a provider's route change) the helper notices only when its relay client's read limit runs out, 120 s after the last frame, and the relay sends a keepalive only every 60 s. Until then this computer hears no device that reaches it through the relay and no new device at all (a hello always travels through the relay): the link was silent for 110 and 123 s in two runs, the connecting side restarted its own helper every 20 s to no effect, and nothing on this computer said anything. Nothing here can make tailcat look sooner; a helper has no command for it, so the only cure is to start the helper again, which makes it connect to the relay anew.
//!
//! What tells this computer that something may be wrong: 10 s after a device's traffic stops arriving, the helper's WireGuard logs that it is sending a keepalive (and, when it had data waiting, that it stopped hearing back). That happens every time a device goes away too, so it is only a reason to look. Looking is one short run of the newest tailcat as a client, with a key of its own that is on no allow-list, saying hello to this computer's own address: a hello always goes through the relay, the helper logs every hello it hears before it turns the key away, and that line is the proof that the relay still delivers to it. The key lets nobody in and is never added anywhere: the helper refuses it like any stranger, and its hello is not a device knocking while a pairing code is shown (`observe` takes its lines out before `door.rs` sees them). If the client reached the relay and five seconds of hellos were not heard, the helper is started again. Measured on the same lab pair: silent for 21 s instead of 110 to 123 s.
//!
//! Kept cheap and safe on purpose: no check while a device's own hello was heard in the last minute (that is the same proof for free), none before this helper has logged a hello at all (an older tailcat that words it differently is never judged), a check that got an answer doubles the wait before the next one (one minute up to fifteen), a check that could not reach the relay itself decides nothing (this computer is offline), and one helper is started again for this at most once in ten minutes. The pairing door is not checked: it lives for a minute.
//!
//! With the helper's install and update (GitHub #559, `install.rs`): while an install or an update runs there is no check and no restart for one, since an update swaps the helper file and starts the helpers again itself (`TailcatRuntime::restart_helpers`), and a second restart beside it would cut the devices twice. Every restart for another reason (an update, a changed allow-list) makes a new `RelayCheck` for the new helper, so a check still in flight for the old one decides nothing; its key is read from its file when the check is made, so a hello of that late check is still taken out of the new helper's log and is never a knock.
//! SEE-ALSO: supervisor.rs (`supervise` starts the helper again), link_log.rs (`follow_tailcat_log`), install.rs (`installation_status`), server/src/remote_access/computer_pairing.rs (`read_tailcat_client_key`).

use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::{Duration, Instant},
};

use super::link_log::LinkLogContext;

/// What WireGuard logs when a device it was exchanging traffic with is no longer heard.
const QUIET_MARKERS: [&str; 2] = ["Sending keepalive packet", "stopped hearing back"];
/// A device's hello heard this recently proves the relay still delivers to the helper.
const HELLO_IS_PROOF_FOR: Duration = Duration::from_secs(60);
/// The wait before the next check after one that got an answer; it doubles up to the longest.
const FIRST_CHECK_GAP: Duration = Duration::from_secs(60);
const LONGEST_CHECK_GAP: Duration = Duration::from_secs(15 * 60);
/// How long the hellos of a check that reached the relay may go unheard.
const HEAR_WITHIN: Duration = Duration::from_secs(5);
/// How long one check may take in all; tailcat itself gives a hello ten seconds.
const CHECK_LIMIT: Duration = Duration::from_secs(12);
/// One helper is started again for a silent relay at most this often.
const RESTART_GAP: Duration = Duration::from_secs(10 * 60);

/// What one check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// The helper logged the check's hello: the relay delivers to it.
    Answered,
    /// The check reached the relay and its hellos were not heard.
    Silent,
    /// The check could not be made (no tailcat to run, no address yet, the relay out of reach).
    Unknown,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::Silent => "silent",
            Self::Unknown => "unknown",
        }
    }
}

struct State {
    /// The check's own key (`nodekey:<hex>`): read from its file, or made at the first check.
    check_key: Option<String>,
    /// When the helper last logged the check's hello.
    check_heard: Option<Instant>,
    /// When the helper last logged a device's hello.
    device_hello: Option<Instant>,
    checking: bool,
    next_check_after: Option<Instant>,
    gap: Duration,
    last_restart: Option<Instant>,
    restart_wanted: bool,
}

struct Inner {
    state: Mutex<State>,
    /// Where the check's key is kept.
    key_file: PathBuf,
    /// The helper's address, once it has published it.
    address: Box<dyn Fn() -> Option<String> + Send + Sync>,
    context: LinkLogContext,
}

/// The relay check of one of the two helpers paired devices dial.
#[derive(Clone)]
pub(crate) struct RelayCheck(Arc<Inner>);

impl RelayCheck {
    pub(crate) fn new(
        key_file: PathBuf,
        address: Box<dyn Fn() -> Option<String> + Send + Sync>,
        context: LinkLogContext,
    ) -> Self {
        Self(Arc::new(Inner {
            state: Mutex::new(State {
                check_key: saved_check_key(&key_file),
                check_heard: None,
                device_hello: None,
                checking: false,
                next_check_after: None,
                gap: FIRST_CHECK_GAP,
                last_restart: None,
                restart_wanted: false,
            }),
            key_file,
            address,
            context,
        }))
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Reads one line of the helper's log. True when the line is about the check's own hello,
    /// which is no device: the caller drops it.
    pub(crate) fn observe(&self, line: &str) -> bool {
        let heard = line.split("got meow from ").nth(1);
        let refused = line.split("ignoring meow from ").nth(1);
        if let Some(rest) = heard.or(refused) {
            let key = rest
                .split(|c: char| c.is_whitespace() || c == ',')
                .next()
                .unwrap_or_default()
                .trim_end_matches(':');
            let mut state = self.state();
            if state.check_key.as_deref() == Some(key) {
                if heard.is_some() {
                    state.check_heard = Some(Instant::now());
                }
                return true;
            }
            if heard.is_some() {
                state.device_hello = Some(Instant::now());
            }
            return false;
        }
        if QUIET_MARKERS.iter().any(|marker| line.contains(marker)) {
            self.device_went_quiet();
        }
        false
    }

    /// A device this helper was carrying is no longer heard: looks whether the relay still
    /// delivers, unless that is known or was asked a moment ago.
    fn device_went_quiet(&self) {
        {
            let mut state = self.state();
            let now = Instant::now();
            let Some(device_hello) = state.device_hello else {
                return;
            };
            if state.checking
                || now.saturating_duration_since(device_hello) < HELLO_IS_PROOF_FOR
                || state.next_check_after.is_some_and(|at| now < at)
                || helper_is_being_replaced()
            {
                return;
            }
            state.checking = true;
        }
        let check = self.clone();
        thread::spawn(move || {
            let started = Instant::now();
            let verdict = check.look();
            let restarting = {
                let mut state = check.state();
                let now = Instant::now();
                state.checking = false;
                let mut restarting = false;
                match verdict {
                    Verdict::Answered => {
                        state.next_check_after = Some(now + state.gap);
                        state.gap = (state.gap * 2).min(LONGEST_CHECK_GAP);
                    }
                    Verdict::Silent => {
                        state.gap = FIRST_CHECK_GAP;
                        state.next_check_after = Some(now + FIRST_CHECK_GAP);
                        // An update that began meanwhile starts the helpers again itself.
                        if !helper_is_being_replaced()
                            && state
                                .last_restart
                                .is_none_or(|at| now.saturating_duration_since(at) >= RESTART_GAP)
                        {
                            state.last_restart = Some(now);
                            state.restart_wanted = true;
                            restarting = true;
                        }
                    }
                    Verdict::Unknown => state.next_check_after = Some(now + FIRST_CHECK_GAP),
                }
                restarting
            };
            super::link_log::record_relay_check(
                &check.0.context,
                verdict.as_str(),
                started.elapsed().as_millis() as u64,
                restarting,
            );
        });
    }

    /// Whether the helper is to be started again for a silent relay; asked once per request.
    pub(crate) fn take_restart_request(&self) -> bool {
        std::mem::take(&mut self.state().restart_wanted)
    }

    /// One check: a tailcat client with the check's key says hello to this computer's own address
    /// for a few seconds, and the helper's log (`observe`) says whether it heard.
    fn look(&self) -> Verdict {
        let Some(binary) = super::binary::resolve_client_tailcat_binary() else {
            return Verdict::Unknown;
        };
        let Some(address) = (self.0.address)() else {
            return Verdict::Unknown;
        };
        if self.state().check_key.is_none() {
            // Made once; known here before its first hello can be logged.
            let Some(key) = make_check_key(&binary, &self.0.key_file) else {
                return Verdict::Unknown;
            };
            self.state().check_key = Some(key);
        }
        let started = Instant::now();
        let Ok(mut child) = crate::platform::process::background_command(&binary)
            .arg("--verbose")
            .arg(format!("--key={}", self.0.key_file.display()))
            .args(["ping", "--timeout=10s"])
            .arg(address)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        else {
            return Verdict::Unknown;
        };
        crate::platform::process::end_with_server(child.id());
        // The client's own log says when it reached the relay; from then on it says hello every
        // second.
        let reached_relay = Arc::new(AtomicBool::new(false));
        if let Some(stderr) = child.stderr.take() {
            let reached_relay = reached_relay.clone();
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if line.contains("magicsock: derp-") && line.contains(" connected") {
                        reached_relay.store(true, Ordering::SeqCst);
                    }
                }
            });
        }
        let mut reached_at: Option<Instant> = None;
        let verdict = loop {
            if self
                .state()
                .check_heard
                .is_some_and(|heard| heard >= started)
            {
                break Verdict::Answered;
            }
            if reached_at.is_none() && reached_relay.load(Ordering::SeqCst) {
                reached_at = Some(Instant::now());
            }
            if reached_at.is_some_and(|at| at.elapsed() >= HEAR_WITHIN) {
                break Verdict::Silent;
            }
            let ended = !matches!(child.try_wait(), Ok(None));
            if ended || started.elapsed() >= CHECK_LIMIT {
                break Verdict::Unknown;
            }
            thread::sleep(Duration::from_millis(100));
        };
        let _ = child.kill();
        let _ = child.wait();
        verdict
    }
}

/// Whether an install or an update of the helper is running (`install.rs`).
fn helper_is_being_replaced() -> bool {
    super::install::installation_status().running
}

/// The public half of the check's key when its file exists already (`Public.ServerPublic`, as
/// tailcat writes a client key).
fn saved_check_key(key_file: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(key_file).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.pointer("/Public/ServerPublic")
        .and_then(serde_json::Value::as_str)
        .filter(|key| key.starts_with("nodekey:"))
        .map(str::to_string)
}

/// `tailcat genkey --client --key=<file>`: the public half of the check's key, made the first time.
fn make_check_key(binary: &std::path::Path, key_file: &std::path::Path) -> Option<String> {
    let output = crate::platform::process::background_command(binary)
        .args([
            "genkey",
            "--client",
            &format!("--key={}", key_file.display()),
        ])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    crate::remote_access::computer_pairing::read_tailcat_client_key(
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
        |path| std::fs::read_to_string(path).ok(),
    )
    .ok()
}
