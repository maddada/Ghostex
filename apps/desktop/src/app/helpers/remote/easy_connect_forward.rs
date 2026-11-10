// Loopback forwarder for Easy Connect remote machines: the desktop-side
// counterpart of `server/src/ghostex_cli/tailcat_tunnel.rs`, adapted to the
// desktop crate (which does not depend on gxserver) and to this app's
// per-machine connection lifecycle.

use std::{
    collections::HashMap,
    env, fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use crate::app::helpers::*;
use crate::*;
use ghostex_paths::tailcat::{
    TAILCAT_MIN_VERSION, bundled_tailcat_binary, managed_tailcat_binary, tailcat_version_supported,
};
use ghostex_paths::tailcat_forward::{
    ClientEnd, ForwardEvent, ForwardProbe, ForwardSpec, TailcatForward,
};

/*
CDXC:RemotePairing 2026-09-03:
An Easy Connect machine has no reachable SSH host. Its pairing code carries a
tailcat address blob, so this module runs one loopback listener per machine
and relays every accepted connection to that machine's one tailcat client
(`ghostex_paths::tailcat_forward`, which gxserver's own tunnels run too). Everything else in `helpers/remote/` then dials a
plain `127.0.0.1:<port>` exactly as it would dial an SSH host: the config
builder reads the port from the registry here, so the SSH tunnel, command
runner, terminal attach, and VS Code launch all go through the same forwarder.

The binary is the first of: explicit override, the helper Ghostex installs (Settings), the one the Linux packages ship beside their gxserver, PATH, `~/go/bin`, then Homebrew, that is at least `TAILCAT_MIN_VERSION` (`gpui_resolve_easy_connect_binary`).

CDXC:RemotePairing 2026-11-10 WHY:
A user reported "a lot of reconnects" between app and remote. The forwarder was stopped with every drop of the machine's gxserver connection and before every connect, automatic ones included, so a reconnect for any reason (the far gxserver restarting for an update, a slow health check) also cut every terminal attached to that machine, which ride the forwarder with SSH connections of their own; and since the registry here is one per app while each window keeps its own connection, one window's reconnect ended the other windows' tunnels, which then did the same back. The forwarder now lives for as long as the machine is saved with the same address: `gpui_start_easy_connect_forward` reuses it, and only a changed address or SSH port, the machine being turned off or removed, and the app quitting stop it. A forwarder whose machine cannot be reached simply fails the SSH dial through it, as an unreachable host would.
*/

pub(crate) const GPUI_EASY_CONNECT_FORWARD_HOST: &str = "127.0.0.1";
pub(crate) const GPUI_EASY_CONNECT_DEFAULT_SSH_PORT: u16 = 22;

struct EasyConnectForward {
    address_blob: String,
    remote_port: u16,
    forward: TailcatForward,
}

struct EasyConnectForwardRegistry {
    running: HashMap<String, EasyConnectForward>,
    /// Port each machine last forwarded on, so a reconnect reuses it when it
    /// is still free and long-lived attach terminals keep a valid endpoint.
    last_ports: HashMap<String, u16>,
}

static EASY_CONNECT_FORWARDS: Mutex<Option<EasyConnectForwardRegistry>> = Mutex::new(None);

fn with_registry<T>(apply: impl FnOnce(&mut EasyConnectForwardRegistry) -> T) -> T {
    let mut guard = EASY_CONNECT_FORWARDS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let registry = guard.get_or_insert_with(|| EasyConnectForwardRegistry {
        running: HashMap::new(),
        last_ports: HashMap::new(),
    });
    apply(registry)
}

/// The loopback port currently forwarding to the machine, if its forwarder is running.
pub(crate) fn gpui_easy_connect_forward_port(remote_machine_id: &str) -> Option<u16> {
    with_registry(|registry| {
        registry
            .running
            .get(remote_machine_id)
            .map(|forward| forward.forward.local_port())
    })
}

/// Starts the machine's forwarder to `remote_port` there, or reuses the one running for the same
/// address and port, and returns the loopback port to dial. `probe` is how the link checks itself
/// (`ghostex_paths::tailcat_forward`): by what the far port is, SSH or a box's gxserver.
pub(crate) fn gpui_start_easy_connect_forward(
    remote_machine_id: &str,
    address_blob: &str,
    remote_port: u16,
    probe: ForwardProbe,
) -> Result<u16, String> {
    let address_blob = address_blob.trim();
    let running = with_registry(|registry| {
        registry.running.get(remote_machine_id).map(|forward| {
            (
                forward.forward.local_port(),
                forward.address_blob == address_blob && forward.remote_port == remote_port,
            )
        })
    });
    match running {
        Some((port, true)) => return Ok(port),
        // The machine was paired again or its port changed: the old link goes.
        Some((_, false)) => gpui_stop_easy_connect_forward(remote_machine_id),
        None => {}
    }
    if address_blob.is_empty() || address_blob.chars().any(char::is_whitespace) {
        return Err("The saved Easy Connect address is invalid.".to_string());
    }
    let binary_path = match gpui_resolve_easy_connect_binary() {
        EasyConnectBinary::Ready(path) => path,
        EasyConnectBinary::TooOld(path, version) => {
            request_easy_connect_install();
            return Err(format!(
                "Easy Connect's helper on this computer ({version} at {}) is older than {TAILCAT_MIN_VERSION} and can't reach this machine. Ghostex is installing the current one (Settings → Remote → Easy Connect shows its progress); reconnect when it finishes.",
                path.display()
            ));
        }
        EasyConnectBinary::Missing => {
            request_easy_connect_install();
            return Err(
                "Easy Connect is not installed on this computer. Ghostex is installing it now (Settings → Remote → Easy Connect shows its progress); reconnect when it finishes."
                    .to_string(),
            );
        }
    };
    let preferred_port =
        with_registry(|registry| registry.last_ports.get(remote_machine_id).copied());
    let listener = gpui_bind_easy_connect_listener(preferred_port)?;
    let spec = ForwardSpec {
        binary_path,
        address_blob: address_blob.to_string(),
        remote_port,
        probe,
    };
    let key_address = address_blob.to_string();
    let event_machine_id = remote_machine_id.to_string();
    let forward = TailcatForward::start(
        listener,
        spec,
        Arc::new(move || easy_connect_link_key(&key_address)),
        Arc::new(move |event| log_easy_connect_forward_event(&event_machine_id, event)),
    )
    .map_err(|_| "Could not start the Easy Connect forwarder.".to_string())?;
    let local_port = forward.local_port();
    with_registry(|registry| {
        registry
            .last_ports
            .insert(remote_machine_id.to_string(), local_port);
        registry.running.insert(
            remote_machine_id.to_string(),
            EasyConnectForward {
                address_blob: address_blob.to_string(),
                remote_port,
                forward,
            },
        );
    });
    Ok(local_port)
}

/// One line in the remote log when a machine's Easy Connect helper ends for a reason of its own
/// (`helperExited`, `carriedNothing`, `neverCarried`, `helperDidNotStart`), with how long it ran
/// and how many connections went with it. Written whatever the Diagnostic logs switches say: it is
/// why a machine's terminals and tunnel just reconnected.
fn log_easy_connect_forward_event(remote_machine_id: &str, event: ForwardEvent) {
    let ForwardEvent::ClientEnded {
        reason,
        uptime,
        connections_cut,
    } = event
    else {
        return;
    };
    if matches!(reason, ClientEnd::Stopped | ClientEnd::Refreshed) {
        return;
    }
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.easyConnectHelperEnded",
        serde_json::json!({
            "level": "warning",
            "machineId": remote_machine_id,
            "reason": reason.as_str(),
            "uptimeMs": uptime.as_millis() as u64,
            "connectionsCut": connections_cut,
        }),
    );
}

/// Stops the machine's forwarder and its tailcat client, and waits for its port to be free. Only
/// for a machine that is turned off, removed or given another address, and a failed first connect.
pub(crate) fn gpui_stop_easy_connect_forward(remote_machine_id: &str) {
    if let Some(forward) = with_registry(|registry| registry.running.remove(remote_machine_id)) {
        /*
        Waiting makes the port free by the time this returns, so an immediate reconnect can bind
        the same preferred port instead of moving to a new one and stranding attach terminals that
        poll the old one, and no tailcat client outlives the app (one does not exit by itself
        while its tunnel link stays up).
        */
        forward.forward.stop_and_wait();
    }
}

/// Stops the forwarder of every saved machine that `settings` turned off or no longer holds,
/// connected or not. A Cloud Box's forwarder (gx_store/cloud_boxes.rs) is no saved machine and
/// stays.
pub(crate) fn gpui_stop_easy_connect_forwards_of_unsaved_machines(
    settings: &serde_json::Map<String, serde_json::Value>,
) {
    let running: Vec<String> = with_registry(|registry| registry.running.keys().cloned().collect());
    let disabled = gpui_disabled_remote_machine_ids(settings);
    for machine_id in running {
        if ghostex_gx_core::protocol::cloud_box::cloud_box_id_from_machine_id(&machine_id).is_some()
        {
            continue;
        }
        if disabled.contains(&machine_id)
            || !gpui_settings_object_has_remote_machine_id(settings, &machine_id)
        {
            gpui_stop_easy_connect_forward(&machine_id);
        }
    }
}

pub(crate) fn gpui_stop_all_easy_connect_forwards() {
    let forwards: Vec<EasyConnectForward> = with_registry(|registry| {
        registry
            .running
            .drain()
            .map(|(_, forward)| forward)
            .collect()
    });
    for forward in forwards {
        forward.forward.stop_and_wait();
    }
}

/// Rewrites an Easy Connect machine's config so its SSH endpoint is the running
/// forwarder. SSH machines pass through untouched.
pub(crate) fn gpui_remote_machine_config_with_easy_connect_forward(
    mut config: GpuiRemoteMachineConfig,
) -> Result<GpuiRemoteMachineConfig, String> {
    if !config.uses_easy_connect() {
        return Ok(config);
    }
    let Some(address) = config.easy_connect_address.clone() else {
        return Err("The saved machine is missing its Easy Connect address.".to_string());
    };
    // A connect through a forwarder nothing rides on (no terminal of this machine, no tunnel)
    // dials with a fresh helper, as every connect did before the forwarder outlived its connection:
    // an unused helper may have lost the machine with nothing to show it.
    with_registry(|registry| {
        if let Some(forward) = registry.running.get(config.remote_machine_id.as_str()) {
            forward.forward.refresh_idle_client();
        }
    });
    let local_port = gpui_start_easy_connect_forward(
        config.remote_machine_id.as_str(),
        address.as_str(),
        config.easy_connect_ssh_port,
        // The forwarded port is SSH, whose server speaks first.
        ForwardProbe::ServerSpeaksFirst,
    )?;
    config.ssh_host = GPUI_EASY_CONNECT_FORWARD_HOST.to_string();
    config.ssh_port = Some(local_port);
    Ok(config)
}

fn gpui_bind_easy_connect_listener(preferred_port: Option<u16>) -> Result<TcpListener, String> {
    if let Some(port) = preferred_port
        && let Ok(listener) = TcpListener::bind((GPUI_EASY_CONNECT_FORWARD_HOST, port))
    {
        return Ok(listener);
    }
    TcpListener::bind((GPUI_EASY_CONNECT_FORWARD_HOST, 0))
        .map_err(|error| format!("Could not bind the Easy Connect forwarder on loopback: {error}"))
}

// ---------------------------------------------------------------------------
// Binary resolution: a tailcat new enough to dial current servers.
// ---------------------------------------------------------------------------

/// What the lookup found: a helper to dial with, an older one that cannot complete a handshake
/// with a current server (its path and version), or none.
pub(crate) enum EasyConnectBinary {
    Ready(PathBuf),
    TooOld(PathBuf, String),
    Missing,
}

/// The first candidate whose `tailcat version` is at least `TAILCAT_MIN_VERSION`.
///
/// CDXC:RemotePairing 2026-11-02 WHY: Ghostex's own install comes first and an older helper
/// anywhere (a `go install` in `~/go/bin`, Homebrew) is skipped, never replaced: a v0.4.0 client
/// pings a v0.6.0 Cloud Box but never completes its handshake (`ghostex_paths::tailcat`).
pub(crate) fn gpui_resolve_easy_connect_binary() -> EasyConnectBinary {
    let mut too_old: Option<(PathBuf, String)> = None;
    for candidate in easy_connect_binary_candidates() {
        if !is_executable_file(&candidate) {
            continue;
        }
        match easy_connect_binary_version(&candidate) {
            Some(version) if tailcat_version_supported(&version) => {
                return EasyConnectBinary::Ready(candidate);
            }
            version => {
                too_old.get_or_insert((candidate, version.unwrap_or_else(|| "unknown".into())));
            }
        }
    }
    match too_old {
        Some((path, version)) => EasyConnectBinary::TooOld(path, version),
        None => EasyConnectBinary::Missing,
    }
}

fn easy_connect_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = env::var_os("GHOSTEX_TAILCAT_BIN")
        && !value.is_empty()
    {
        candidates.push(PathBuf::from(value));
    }
    // CDXC:RemotePairing 2026-09-05 SEE-ALSO: server/src/tailcat/binary.rs installs the Settings helper at this shared per-user location.
    candidates.push(managed_tailcat_binary(
        &ghostex_paths::GhostexPaths::resolve().data_dir,
    ));
    // CDXC:CloudBoxes 2026-11-02 WHY: the Linux packages ship tailcat in the bundled gxserver
    // package (package-remote-linux.mjs), so a Linux desktop dials boxes without installing one.
    if let Some(bundled) =
        gpui_resolve_local_gxserver_binary().and_then(|gxserver| bundled_tailcat_binary(&gxserver))
    {
        candidates.push(bundled);
    }
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(
            env::split_paths(&path).map(|directory| directory.join(easy_connect_executable_name())),
        );
    }
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        candidates.push(
            home.join("go")
                .join("bin")
                .join(easy_connect_executable_name()),
        );
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin").join(easy_connect_executable_name()));
    candidates
}

/// Versions already read, by path, with the file's size and change time when they were read.
type VersionCache = HashMap<PathBuf, (u64, Option<std::time::SystemTime>, Option<String>)>;
static VERSIONS: Mutex<Option<VersionCache>> = Mutex::new(None);
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// `tailcat version`'s first line, read once per file (and again when the file changes).
fn easy_connect_binary_version(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    let stamp = (metadata.len(), metadata.modified().ok());
    if let Some((len, modified, version)) = VERSIONS
        .lock()
        .ok()
        .and_then(|cache| cache.as_ref()?.get(path).cloned())
        && (len, modified) == stamp
    {
        return version;
    }
    let version = read_easy_connect_binary_version(path);
    if let Ok(mut cache) = VERSIONS.lock() {
        cache
            .get_or_insert_with(HashMap::new)
            .insert(path.to_path_buf(), (stamp.0, stamp.1, version.clone()));
    }
    version
}

fn read_easy_connect_binary_version(path: &Path) -> Option<String> {
    let mut child = gpui_background_command(path)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + VERSION_TIMEOUT;
    while child.try_wait().ok()?.is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output().ok()?;
    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    (!version.is_empty()).then_some(version)
}

/// Asks this computer's gxserver, once per app run, to install the current helper (the job
/// Settings › Remote › Easy Connect › Install runs), so a missing or old helper heals itself.
fn request_easy_connect_install() {
    static REQUESTED: AtomicBool = AtomicBool::new(false);
    if REQUESTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(|| {
        if gpui_gxserver_rpc_result(
            "/api/installTailcat",
            &serde_json::json!({}),
            Duration::from_secs(30),
        )
        .is_err()
        {
            // Asked again on the next connect.
            REQUESTED.store(false, Ordering::SeqCst);
        }
    });
}

fn easy_connect_executable_name() -> &'static str {
    if cfg!(windows) {
        "tailcat.exe"
    } else {
        "tailcat"
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

// ---------------------------------------------------------------------------
// The key the machine's one tailcat client dials with.
// ---------------------------------------------------------------------------

/// How long the forwarder waits for gxserver to name its key for a machine: the first ask registers
/// the keys there, which takes a tunnel's rendezvous and an answer, for each port it tries.
const LINK_KEY_TIMEOUT: Duration = Duration::from_secs(40);

/// This computer's own Easy Connect key for the machine at `address`, or `None` for `client-default`.
///
/// CDXC:RemotePairing 2026-11-10 WHY: the forwarder and gxserver's own tunnels to one machine,
/// and the forwarders to two machines on one relay, stalled each other while all dialed with
/// `client-default` (GitHub #526). gxserver, which holds the pairing device key, registers keys of
/// this computer's own with each machine (server/src/remote_access/computer_link_keys.rs) and names
/// the app's here; a machine whose Ghostex can't take them, or a gxserver that doesn't answer,
/// leaves the forwarder on `client-default` as before.
fn easy_connect_link_key(address: &str) -> Option<PathBuf> {
    let params = serde_json::json!({ "address": address, "purpose": "app" });
    let (status, body) =
        gxserver_post_typed_operation("/api/easyConnectLinkKey", &params, LINK_KEY_TIMEOUT).ok()?;
    if status != 200 {
        return None;
    }
    let answer: serde_json::Value = serde_json::from_str(&body).ok()?;
    let path = PathBuf::from(answer.pointer("/result/keyPath")?.as_str()?);
    path.is_file().then_some(path)
}
