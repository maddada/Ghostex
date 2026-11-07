use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use serde_json::{Map, Value};

use crate::{
    agent_hooks::{repair_installed_agent_hook_paths, run_notify_hook, run_statusline_hook},
    agent_skills::{install_agent_skills, read_agent_skill_status},
    auth::read_gxserver_auth_token,
    config::read_selected_local_api_port,
    constants::{GXSERVER_LOCAL_API_HOST, GXSERVER_PRODUCT, GXSERVER_VERSION},
    http_client::{fetch_server_health, request_server_stop, request_server_stop_all},
    paths::{check_storage_writable, get_gxserver_paths, migrate_legacy_storage},
    protocol::StatusResponse,
    runtime::{
        create_running_status, create_source_build_identity, create_stopped_status,
        is_build_identity_reusable, is_process_running, read_current_build_identity,
        read_runtime_metadata,
    },
    server::{run_gxserver_foreground, GxserverForegroundOptions},
};

pub async fn run_from_env() -> Result<()> {
    run(env::args().skip(1).collect()).await
}

/// CDXC:AgentHooks 2026-10-09 WHY:
/// Hook processes run on every tool event and Codex caps Interrupt at 3 seconds. They need neither the daemon runtime nor storage migration and build identity discovery before they can report an event or return outside Ghostex.
pub fn run_hook_command(args: &[String]) -> Option<Result<()>> {
    match args.first().map(String::as_str) {
        #[cfg(windows)]
        Some("agent-hook-notify-native") => {
            Some(crate::agent_hooks::windows::notify(args[1..].to_vec()))
        }
        #[cfg(windows)]
        Some(crate::platform::process_files::OPEN_PATHS_COMMAND) => {
            Some(crate::platform::process_files::print_open_paths(&args[1..]))
        }
        Some("agent-hook-notify") => Some((|| {
            if let Some(answer) = run_notify_hook(args[1..].to_vec())? {
                println!("{answer}");
            }
            Ok(())
        })()),
        _ => None,
    }
}

/*
CDXC:Cli 2026-06-14-20:37:
The Rust CLI intentionally keeps the TypeScript command surface and --json behavior for start, stop, stop-all, and status so app/CLI opt-in can swap binaries without changing client command construction.

CDXC:Cli 2026-06-22-04:47:
`gxserver status` reports any reachable same-product, same-protocol daemon as running; build identity is a start-time replacement decision. `gxserver start` must match TypeScript by requesting control-plane shutdown for a running build-identity mismatch instead of returning a Rust-only portConflict status.
*/
pub async fn run(args: Vec<String>) -> Result<()> {
    if let Some(result) = run_hook_command(&args) {
        return result;
    }
    let command = args.first().map(String::as_str);
    #[cfg(target_os = "macos")]
    if command == Some("--recover-after-login") {
        return crate::server::macos_login_recovery::recover().await;
    }
    /*
    `setup` is the recovery boundary for an uploaded managed package. It must
    not depend on the existing storage layout being migratable: setup replaces
    stale package/tool links, after which the newly activated runtime can run
    the normal migration path. This also lets setup repair legacy links whose
    targets were moved into XDG storage by an interrupted older migration.
    */
    if command == Some(crate::computer_startup::AT_LOGIN_COMMAND) {
        return crate::computer_startup::run_at_login().await;
    }
    if command == Some("setup") {
        crate::setup::run_setup(args.iter().skip(1).cloned().collect())?;
        return Ok(());
    }
    #[cfg(windows)]
    if command == Some(crate::platform::desktop_session::DESKTOP_LAUNCH_COMMAND) {
        crate::platform::desktop_session::run_desktop_launch(&args[1..])
            .context("start gxserver from your desktop session")?;
        return Ok(());
    }
    // Hands off before touching storage, so nothing is created with administrator ownership.
    #[cfg(windows)]
    if matches!(command, None | Some("--foreground")) {
        if let Some(message) = hand_off_windows_foreground_start().await? {
            println!("{message}");
            return Ok(());
        }
    }
    let storage_warning = migrate_legacy_storage();
    if matches!(command, None | Some("--foreground")) {
        check_storage_writable()?;
    }
    if let Some(warning) = storage_warning {
        eprintln!("{warning}");
    }
    if matches!(command, None | Some("--foreground")) {
        let paths = get_gxserver_paths(None);
        repair_installed_agent_hook_paths(&paths)
            .context("repair installed Ghostex agent hook paths")?;
    }
    let version = GXSERVER_VERSION.to_string();
    let build_identity = read_current_build_identity(&version)?;
    match command {
        None | Some("--foreground") => {
            #[cfg(windows)]
            crate::platform::process::suppress_helper_error_dialogs();
            let result = run_gxserver_foreground(GxserverForegroundOptions {
                build_identity: Some(build_identity),
                home_dir: None,
                version,
            })
            .await?;
            if result.reused {
                println!("gxserver is already running and uses the expected protocol.");
            }
        }
        Some("start") => {
            print_status(
                &start_gxserver_background(&build_identity, &version).await?,
                args.iter().skip(1).any(|arg| arg == "--json"),
            )?;
        }
        Some("stop") => {
            print_status(
                &stop_gxserver_control_plane(&build_identity, &version).await?,
                args.iter().skip(1).any(|arg| arg == "--json"),
            )?;
        }
        Some("stop-all") => {
            print_status(
                &stop_gxserver_and_sessions(&build_identity, &version).await?,
                args.iter().skip(1).any(|arg| arg == "--json"),
            )?;
        }
        Some("status") => {
            print_status(
                &get_gxserver_status(&build_identity, &version).await?,
                args.iter().skip(1).any(|arg| arg == "--json"),
            )?;
        }
        Some("endpoint") => print_endpoint(args.iter().skip(1).any(|arg| arg == "--no-token"))?,
        Some("agent-skills") => {
            run_agent_skills_command(args.iter().skip(1).cloned().collect()).await?;
        }
        #[cfg(windows)]
        Some("agent-statusline-native") => {
            crate::agent_hooks::statusline::run_native_statusline_hook(
                args.iter().skip(1).cloned().collect(),
            )?;
        }
        Some("agent-statusline") => {
            run_statusline_hook(args.iter().skip(1).cloned().collect())?;
        }
        Some("resume-lookup") => {
            crate::resume_lookup::run_resume_lookup(args.iter().skip(1).cloned().collect())?;
        }
        Some("--version") | Some("version") => {
            println!("{version}");
        }
        Some("--help") | Some("help") => {
            print_help(&version);
        }
        Some(other) => return Err(anyhow!("Unknown gxserver command: {other}")),
    }
    Ok(())
}

/// What a client reaching this daemon through `ghostex server endpoint` may rely on.
///
/// CDXC:Mobile 2026-10-09 WHY: the phone retries a chat send whose HTTP request was lost on its SSH port forward only when gxserver dedupes `sendRequestId` (`session_chat_send_requests.rs`, `send_once`); an older daemon would type the message twice, and no other field tells the phone which it is talking to. SEE-ALSO: apps/mobile/app/src/chat/rust/machine-link.ts
/// CDXC:VoiceMode 2026-10-20 SEE-ALSO: `voiceAgent` means `/api/voice` (conversation and dictation) and `/api/readVoiceAgent` exist (docs/2026-10-20/voice-mode/voice-agent-client-contract.md).
/// CDXC:Notifications 2026-10-31 SEE-ALSO: `notificationStream` means `/api/events?stream=notifications` carries only notification feed changes and the daemon's start and stop (`notification_event_matches` in server/src/server/ws.rs); an older daemon would send that socket its whole hub, so the phone opens it only behind this flag and otherwise polls the feed.
/// CDXC:Notifications 2026-11-01 SEE-ALSO: `pushNotifications` means `/api/registerPushToken` exists and this daemon pushes finished, needs-input and failed notices and agent starts to the registered phones (server/src/push/); the phone shows its push switches only behind this flag, since an older daemon has no such route.
const ENDPOINT_CAPABILITIES: [&str; 4] = [
    "sendRequestLedger",
    "voiceAgent",
    ghostex_gx_protocol::GXSERVER_ENDPOINT_CAPABILITY_NOTIFICATION_STREAM,
    ghostex_gx_protocol::GXSERVER_ENDPOINT_CAPABILITY_PUSH_NOTIFICATIONS,
];

/// `gxserver endpoint` (`ghostex server endpoint`): the running daemon's local port and bearer token
/// as JSON.
///
/// CDXC:Mobile 2026-09-27 WHY: a client that reaches this computer over SSH (the phone's chat) opens its own chat socket to gxserver through an SSH port forward, as the desktop's remote chats do, and needs the port and token for it. The desktop reads them with a shell snippet over SSH (`gpui_remote_token_read_command`); this verb answers from the daemon's own paths, so an SSH client needs no copy of that snippet. Anyone who can run it can already read the token file.
/// `--no-token` leaves the main token out: a paired phone that holds its own token asks this way, so
/// the main token never reaches it (CDXC:RemotePairing 2026-10-25 in `auth.rs`).
fn print_endpoint(without_token: bool) -> Result<()> {
    let paths = get_gxserver_paths(None);
    let metadata =
        read_runtime_metadata(&paths)?.ok_or_else(|| anyhow!("gxserver is not running."))?;
    let mut endpoint = endpoint_answer(metadata.port, metadata.protocol_version);
    if !without_token {
        let auth = read_gxserver_auth_token(&paths)?
            .ok_or_else(|| anyhow!("gxserver has no auth token yet; start it first."))?;
        endpoint["authToken"] = Value::String(auth.token);
    }
    println!("{}", serde_json::to_string(&endpoint)?);
    Ok(())
}

fn endpoint_answer(port: u16, protocol_version: u64) -> Value {
    serde_json::json!({
        "ok": true,
        "port": port,
        "baseUrl": format!("http://127.0.0.1:{port}"),
        "protocolVersion": protocol_version,
        "capabilities": ENDPOINT_CAPABILITIES,
    })
}

pub async fn get_gxserver_status(_build_identity: &str, _version: &str) -> Result<StatusResponse> {
    let paths = get_gxserver_paths(None);
    let metadata = read_runtime_metadata(&paths)?;
    let auth = read_gxserver_auth_token(&paths)?;
    if let Some(health) = fetch_server_health(auth.as_ref().map(|auth| auth.token.as_str()), 800)? {
        return Ok(create_running_status(health, metadata));
    }
    // A daemon that holds the port but is still starting answers 503 "starting" (server/src/server/startup_gate.rs).
    if let Some(starting) = crate::http_client::fetch_server_starting(
        auth.as_ref().map(|auth| auth.token.as_str()),
        800,
    )? {
        return Ok(StatusResponse {
            health: None,
            message: starting
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("gxserver is still starting.")
                .to_string(),
            metadata,
            ok: true,
            product: GXSERVER_PRODUCT.to_string(),
            state: "starting".to_string(),
        });
    }
    if let Some(metadata) = metadata.clone() {
        if is_process_running(metadata.pid) {
            let pid = metadata.pid;
            return Ok(StatusResponse {
                health: None,
                metadata: Some(metadata),
                message: format!(
                    "gxserver runtime metadata exists for pid {}, but {GXSERVER_LOCAL_API_HOST}:{} is unreachable.",
                    pid,
                    read_selected_local_api_port()?
                ),
                ok: false,
                product: GXSERVER_PRODUCT.to_string(),
                state: "unreachable".to_string(),
            });
        }
    }
    Ok(create_stopped_status(metadata))
}

pub(crate) async fn start_gxserver_background(
    build_identity: &str,
    version: &str,
) -> Result<StatusResponse> {
    #[cfg(windows)]
    let placement = crate::platform::desktop_session::server_placement()
        .context("find the Windows session gxserver may run in")?;
    let reusable = |status: &StatusResponse| {
        let same_build = is_build_identity_reusable(
            status
                .health
                .as_ref()
                .map(|health| health.build_identity.as_str()),
            Some(build_identity),
        );
        // A server SSH started in the background session is replaced once the user can have one on the desktop.
        #[cfg(windows)]
        let same_build = same_build
            && !(matches!(
                placement,
                crate::platform::desktop_session::ServerPlacement::UserDesktop { .. }
            ) && runs_in_background_session(status));
        same_build
    };
    #[cfg(target_os = "macos")]
    {
        let current_exe = env::current_exe().with_context(|| "resolve current gxserver binary")?;
        if let Some(job) = macos_app_launchd_job::find().filter(|job| !job.runs(&current_exe)) {
            return start_through_app_job(&job, build_identity, version).await;
        }
    }
    let before = get_gxserver_status(build_identity, version).await?;
    if before.state == "running" {
        if reusable(&before) {
            return Ok(before);
        }
        let paths = get_gxserver_paths(None);
        let auth = read_gxserver_auth_token(&paths)?;
        let _ = request_server_stop(auth.as_ref().map(|auth| auth.token.as_str()), 2_000)?;
        let stopped = wait_for_status(
            build_identity,
            version,
            Duration::from_millis(5_000),
            |status| status.state != "running",
        )
        .await?;
        if stopped.state == "running" {
            if reusable(&stopped) {
                return Ok(stopped);
            }
            return Ok(StatusResponse {
                message:
                    "The running gxserver must be replaced, but its control plane did not stop. Stop gxserver and start it again."
                        .to_string(),
                ok: false,
                state: "stopping".to_string(),
                ..before
            });
        }
    }

    #[cfg(windows)]
    if let crate::platform::desktop_session::ServerPlacement::UserDesktop { session_id } = placement
    {
        return start_gxserver_in_user_desktop(build_identity, version, session_id).await;
    }
    let current_exe = env::current_exe().with_context(|| "resolve current gxserver binary")?;
    #[cfg(target_os = "macos")]
    let launchd_job = macos_app_launchd_job::kickstart_if_it_runs(&current_exe)?;
    #[cfg(not(target_os = "macos"))]
    let launchd_job: Option<String> = None;
    let launched = match launchd_job {
        Some(label) => format!("the launchd job {label}"),
        None => {
            let mut command = Command::new(&current_exe);
            command
                .arg("--foreground")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            format!("pid {}", spawn_detached(&mut command)?)
        }
    };

    let status = wait_for_status(
        build_identity,
        version,
        Duration::from_millis(5_000),
        |status| status.state == "running",
    )
    .await?;
    if status.state != "running" {
        return Ok(StatusResponse {
            health: None,
            metadata: None,
            message: format!(
                "gxserver start launched {launched} but health did not become ready on {GXSERVER_LOCAL_API_HOST}:{}.",
                read_selected_local_api_port()?
            ),
            ok: false,
            product: GXSERVER_PRODUCT.to_string(),
            state: "starting".to_string(),
        });
    }
    Ok(status)
}

/// What `gxserver start` does on a Mac whose Ghostex app has its own gxserver job running another program.
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppServerStart {
    /// The app's job is the server: kept whatever its build.
    Keep,
    /// Another gxserver (or none) answers: stop it, then start the app's job (`kickstart -k`
    /// with `restart`, when the job's process is alive but is not the server).
    StopThenStartApp { restart: bool },
}

/// `running_pid`: the pid of the gxserver answering health, `None` when none answers.
#[cfg(any(target_os = "macos", test))]
fn app_server_start(running_pid: Option<u32>, app_job_pid: Option<u32>) -> AppServerStart {
    match (running_pid, app_job_pid) {
        (Some(running), Some(job)) if running == job => AppServerStart::Keep,
        (Some(_), job) => AppServerStart::StopThenStartApp {
            restart: job.is_some(),
        },
        (None, _) => AppServerStart::StopThenStartApp { restart: false },
    }
}

/// How long the app's gxserver may take to answer after `launchctl kickstart`.
#[cfg(target_os = "macos")]
const APP_JOB_START_PATIENCE: Duration = Duration::from_secs(30);

/// CDXC:RemotePairing 2026-11-07 WHY: on a Mac running the Ghostex app, `gxserver start` from the installed package (a remote client's connect runs it after every install and on every connect) found the app's gxserver at another build than its own, stopped it and started the package's program detached, so the remote setup's server ran the Mac instead of the one its app manages, outside the GUI session (GitHub #476). The Mac's own app, its phone and Easy Connect all use the app's server, and a remote client works with any gxserver of the same protocol: it reads features from the capabilities the server advertises, and terminal attach runs the `attachCommand` the server itself returns, so its own zmx. So the app's server is kept whatever its build; one that is not running is started through its launchd job, never as the package's program; and a gxserver that is not the app's (a package one an older connect left) is stopped and the app's started instead. A protocol mismatch fails before anything is stopped (`fetch_server_health`), and the client then says which Ghostex to update. A job that runs this very binary keeps the build-replacement rules below.
/// SEE-ALSO: `server_handoff` in server/src/setup.rs (the same rule while installing), `gpui_remote_protocol_mismatch_message` in apps/desktop/src/app/helpers/remote/connect.rs.
#[cfg(target_os = "macos")]
async fn start_through_app_job(
    job: &macos_app_launchd_job::AppJob,
    build_identity: &str,
    version: &str,
) -> Result<StatusResponse> {
    let mut before = get_gxserver_status(build_identity, version).await?;
    if before.state == "starting" {
        before = wait_for_status(build_identity, version, APP_JOB_START_PATIENCE, |status| {
            status.state != "starting"
        })
        .await?;
    }
    let running_pid = (before.state == "running")
        .then(|| before.health.as_ref().map(|health| health.pid))
        .flatten();
    let restart = match app_server_start(running_pid, job.pid) {
        AppServerStart::Keep => {
            let build = before
                .health
                .as_ref()
                .map(|health| health.version.clone())
                .unwrap_or_default();
            return Ok(StatusResponse {
                message: format!(
                    "gxserver {build} is the Ghostex app's own ({}); it was left running.",
                    job.label
                ),
                ..before
            });
        }
        AppServerStart::StopThenStartApp { restart } => restart,
    };
    if running_pid.is_some() {
        let paths = get_gxserver_paths(None);
        let auth = read_gxserver_auth_token(&paths)?;
        let _ = request_server_stop(auth.as_ref().map(|auth| auth.token.as_str()), 2_000)?;
        let stopped = wait_for_status(
            build_identity,
            version,
            Duration::from_millis(5_000),
            |status| status.state != "running",
        )
        .await?;
        if stopped.state == "running" {
            return Ok(StatusResponse {
                message: format!(
                    "A gxserver that is not the Ghostex app's ({}) answers and its control plane did not stop. Stop it, then start gxserver again.",
                    job.label
                ),
                ok: false,
                state: "stopping".to_string(),
                ..stopped
            });
        }
    }
    macos_app_launchd_job::start(job, restart).map_err(|error| anyhow!(error))?;
    let status = wait_for_status(build_identity, version, APP_JOB_START_PATIENCE, |status| {
        status.state == "running"
    })
    .await?;
    if status.state != "running" {
        return Ok(StatusResponse {
            health: None,
            metadata: None,
            message: format!(
                "gxserver start launched the Ghostex app's launchd job {} but health did not become ready on {GXSERVER_LOCAL_API_HOST}:{}.",
                job.label,
                read_selected_local_api_port()?
            ),
            ok: false,
            product: GXSERVER_PRODUCT.to_string(),
            state: "starting".to_string(),
        });
    }
    Ok(status)
}

/// How long a gxserver handed to the desktop session may take to answer: Task Scheduler, Explorer, and on a cold start the antivirus scan of the executable.
#[cfg(windows)]
const DESKTOP_START_PATIENCE: Duration = Duration::from_secs(90);

#[cfg(windows)]
fn runs_in_background_session(status: &StatusResponse) -> bool {
    status
        .health
        .as_ref()
        .and_then(|health| health.launch_context.as_ref())
        .is_some_and(|context| context.background)
}

/// Starts gxserver in the signed-in user's desktop session (`start_server_in_desktop_session`) and waits until a desktop gxserver answers.
#[cfg(windows)]
async fn start_gxserver_in_user_desktop(
    build_identity: &str,
    version: &str,
    session_id: u32,
) -> Result<StatusResponse> {
    // Explorer starts gxserver with the desktop's environment, so a different home would leave this call waiting on a server that never comes.
    if env::var_os("GHOSTEX_HOME").is_some_and(|home| !home.is_empty()) {
        return Err(anyhow!(
            "GHOSTEX_HOME is set for this call, but gxserver must start in your desktop session (Windows session {session_id}), which cannot inherit it. Start gxserver from a terminal on the desktop instead."
        ));
    }
    let executable = env::current_exe().with_context(|| "resolve current gxserver binary")?;
    let result_file = get_gxserver_paths(None)
        .runtime_dir
        .join(format!("desktop-launch-{}.txt", std::process::id()));
    tokio::task::spawn_blocking(move || {
        crate::platform::desktop_session::start_server_in_desktop_session(&executable, &result_file)
    })
    .await
    .context("wait for the desktop session hand-off")??;
    let status = wait_for_status(build_identity, version, DESKTOP_START_PATIENCE, |status| {
        status.state == "running" && !runs_in_background_session(status)
    })
    .await?;
    if status.state != "running" || runs_in_background_session(&status) {
        return Ok(StatusResponse {
            message: format!(
                "gxserver was started in your desktop session (Windows session {session_id}), but it did not answer on {GXSERVER_LOCAL_API_HOST}:{} within {} seconds.",
                read_selected_local_api_port()?,
                DESKTOP_START_PATIENCE.as_secs()
            ),
            ok: false,
            state: "starting".to_string(),
            ..status
        });
    }
    Ok(StatusResponse {
        message: format!(
            "gxserver started in your desktop session (Windows session {session_id}) instead of this background session."
        ),
        ..status
    })
}

/// `gxserver` / `gxserver --foreground` on Windows: `Some(message)` when the server was started elsewhere and this call is done.
#[cfg(windows)]
async fn hand_off_windows_foreground_start() -> Result<Option<String>> {
    use crate::platform::{
        desktop_session::{server_placement, ServerPlacement},
        process::spawn_server,
        standard_user::current_process_is_elevated,
    };
    let placement = server_placement().context("find the Windows session gxserver may run in")?;
    let elevated =
        current_process_is_elevated().context("check gxserver's administrator rights")?;
    let executable = env::current_exe().context("resolve current gxserver binary")?;
    match placement {
        ServerPlacement::UserDesktop { .. } => {
            let version = GXSERVER_VERSION.to_string();
            let build_identity = read_current_build_identity(&version)?;
            let status = start_gxserver_background(&build_identity, &version).await?;
            if !status.ok {
                return Err(anyhow!(status.message));
            }
            Ok(Some(status.message))
        }
        ServerPlacement::Here | ServerPlacement::NobodySignedIn if elevated => {
            let pid = spawn_server(executable.as_os_str())
                .context("restart gxserver without administrator rights")?;
            Ok(Some(format!(
                "gxserver restarted without administrator rights as pid {pid}."
            )))
        }
        ServerPlacement::Here | ServerPlacement::NobodySignedIn => Ok(None),
    }
}

async fn stop_gxserver_control_plane(
    build_identity: &str,
    version: &str,
) -> Result<StatusResponse> {
    let before = get_gxserver_status(build_identity, version).await?;
    if before.state != "running" {
        return Ok(before);
    }
    let paths = get_gxserver_paths(None);
    let auth = read_gxserver_auth_token(&paths)?;
    let _ = request_server_stop(auth.as_ref().map(|auth| auth.token.as_str()), 800)?;
    let stopped = wait_for_status(
        build_identity,
        version,
        Duration::from_millis(5_000),
        |status| status.state != "running",
    )
    .await?;
    if stopped.state == "running" {
        return Ok(StatusResponse {
            message:
                "gxserver stop requested control-plane shutdown, but the server is still running."
                    .to_string(),
            ok: false,
            state: "stopping".to_string(),
            ..stopped
        });
    }
    Ok(StatusResponse {
        message: "gxserver control plane stopped. zmx sessions were not signaled or killed."
            .to_string(),
        ok: true,
        ..stopped
    })
}

async fn stop_gxserver_and_sessions(build_identity: &str, version: &str) -> Result<StatusResponse> {
    let before = get_gxserver_status(build_identity, version).await?;
    if before.state != "running" {
        return Ok(before);
    }
    let paths = get_gxserver_paths(None);
    let auth = read_gxserver_auth_token(&paths)?;
    let stop_all = request_server_stop_all(auth.as_ref().map(|auth| auth.token.as_str()), 10_000)?;
    if stop_all.is_none() {
        return Ok(StatusResponse {
            message: "gxserver stop-all could not kill zmx sessions before shutdown.".to_string(),
            ok: false,
            state: "stopping".to_string(),
            ..before
        });
    }
    let stopped = wait_for_status(
        build_identity,
        version,
        Duration::from_millis(10_000),
        |status| status.state != "running",
    )
    .await?;
    if stopped.state == "running" {
        return Ok(StatusResponse {
            message:
                "gxserver stop-all killed zmx sessions but the control plane is still running."
                    .to_string(),
            ok: false,
            state: "stopping".to_string(),
            ..stopped
        });
    }
    let result = stop_all
        .and_then(|value| value.get("result").cloned())
        .unwrap_or(Value::Null);
    let killed = result
        .get("killedSessions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failed = result
        .get("failedSessions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Ok(StatusResponse {
        message: format!("gxserver control plane stopped after stop-all. zmx sessions killed: {killed}; failed: {failed}."),
        ok: failed == 0,
        ..stopped
    })
}

async fn wait_for_status(
    build_identity: &str,
    version: &str,
    timeout: Duration,
    done: impl Fn(&StatusResponse) -> bool,
) -> Result<StatusResponse> {
    let deadline = Instant::now() + timeout;
    let mut status = get_gxserver_status(build_identity, version).await?;
    while !done(&status) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        status = get_gxserver_status(build_identity, version).await?;
    }
    Ok(status)
}

fn print_status(status: &StatusResponse, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(status)?);
    } else {
        println!("{}", status.message);
    }
    Ok(())
}

fn print_help(version: &str) {
    println!(
        "gxserver {version}

Usage:
  gxserver           Run gxserver in the foreground
  gxserver start     Start gxserver in the background
  gxserver stop      Stop only the gxserver control plane
  gxserver stop-all  Stop gxserver and kill tracked zmx sessions
  gxserver status    Print gxserver runtime state
  gxserver endpoint  Print the running daemon's port and auth token as JSON (for SSH clients; --no-token leaves the token out)
  gxserver agent-skills status [skill...] [--json]
  gxserver agent-skills install <skill...> --source <path> [--offline] [--json]
  gxserver agent-skills refresh [--json]
                     Update installed bundled skills from GitHub main
  gxserver setup [--install-root <dir>] [--release-dir <dir>] [--upload-path <file>]
                 [--analytics-role remote|local]
                     Activate an extracted release package (stop old server, link tools).
                     --analytics-role remote marks this install as an SSH helper
                     driven by a desktop, so it never emits anonymous analytics.
  gxserver --version Print the gxserver package version
"
    );
}

/*
CDXC:AgentSkills 2026-06-19-13:59:
The Rust CLI mirrors TypeScript's direct agent-skill setup commands so app and shell callers can check/install Ghostex skills without constructing gxserver RPC bodies or duplicating discovery/install argument rules.
*/
async fn run_agent_skills_command(args: Vec<String>) -> Result<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("status");
    if matches!(subcommand, "help" | "-h" | "--help") {
        print_agent_skills_help();
        return Ok(());
    }
    let parsed = parse_cli_args(&args.iter().skip(1).cloned().collect::<Vec<_>>());
    let paths = get_gxserver_paths(None);
    match subcommand {
        "status" => {
            let mut params = Map::new();
            if !parsed.rest.is_empty() {
                params.insert("skillNames".to_string(), json_array(parsed.rest));
            }
            if let Some(repository_paths) = parsed.values.get("repository") {
                params.insert(
                    "repositoryPaths".to_string(),
                    json_array(repository_paths.clone()),
                );
            }
            let result = read_agent_skill_status(&paths, &params)?;
            if parsed.flags.contains("json") {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print_agent_skill_status(&result);
            }
        }
        "install" => {
            let source = parsed
                .values
                .get("source")
                .or_else(|| parsed.values.get("packageSource"))
                .and_then(|values| values.first())
                .cloned();
            let mut params = Map::new();
            params.insert("skillNames".to_string(), json_array(parsed.rest));
            if let Some(source) = source {
                params.insert("packageSource".to_string(), Value::String(source));
            }
            if let Some(agent_ids) = parsed
                .values
                .get("agent")
                .or_else(|| parsed.values.get("agents"))
            {
                params.insert("agentIds".to_string(), json_array(agent_ids.clone()));
            }
            if parsed.flags.contains("offline") {
                params.insert("remote".to_string(), Value::Bool(false));
            }
            if let Some(repository_paths) = parsed.values.get("repository") {
                params.insert(
                    "repositoryPaths".to_string(),
                    json_array(repository_paths.clone()),
                );
            }
            let result = install_agent_skills(&paths, &params).await?;
            if parsed.flags.contains("json") {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                if let Some(stdout) = result.get("stdout").and_then(Value::as_str) {
                    print!("{stdout}");
                }
                if let Some(stderr) = result.get("stderr").and_then(Value::as_str) {
                    eprint!("{stderr}");
                }
                print_agent_skill_status(&result);
            }
        }
        "refresh" => {
            let result = crate::agent_skills_remote::refresh_installed_bundled_skills(&paths)
                .map_err(|error| anyhow!("{error}"))?;
            if parsed.flags.contains("json") {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print_agent_skill_refresh(&result);
            }
        }
        other => return Err(anyhow!("Unknown gxserver agent-skills command: {other}")),
    }
    Ok(())
}

fn print_agent_skill_refresh(result: &Value) {
    let updated = result
        .get("updated")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let failed = result
        .get("failed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for path in updated.iter().filter_map(Value::as_str) {
        println!("updated {path}");
    }
    for failure in &failed {
        let skill = failure.get("skill").and_then(Value::as_str).unwrap_or("?");
        let error = failure.get("error").and_then(Value::as_str).unwrap_or("");
        eprintln!("failed {skill}: {error}");
    }
    if updated.is_empty() && failed.is_empty() {
        println!("All installed bundled skills already match GitHub main.");
    }
}

fn print_agent_skill_status(status: &Value) {
    if let Some(skills) = status.get("skills").and_then(Value::as_array) {
        for skill in skills {
            let skill_name = skill
                .get("skillName")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let marker = if skill
                .get("installed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "installed"
            } else {
                "missing"
            };
            println!("{skill_name}: {marker}");
            if let Some(locations) = skill.get("locations").and_then(Value::as_array) {
                for location in locations {
                    if let Some(directory_path) =
                        location.get("directoryPath").and_then(Value::as_str)
                    {
                        println!("  {directory_path}");
                    }
                }
            }
        }
    }
}

fn print_agent_skills_help() {
    println!(
        "gxserver agent-skills

Usage:
  gxserver agent-skills status [skill...] [--repository path] [--json]
  gxserver agent-skills install <skill...> --source <skills-package-path> [--agent id...] [--offline] [--json]
  gxserver agent-skills refresh [--json]

  install downloads each skill from GitHub main when it can and falls back to the
  local package source; --offline skips the download. refresh replaces every
  installed bundled skill whose files differ from GitHub main (gxserver also does
  this shortly after it starts).
"
    );
}

#[derive(Debug, Default)]
struct ParsedCliArgs {
    flags: HashSet<String>,
    rest: Vec<String>,
    values: HashMap<String, Vec<String>>,
}

fn parse_cli_args(args: &[String]) -> ParsedCliArgs {
    let mut parsed = ParsedCliArgs::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            parsed.rest.extend(args.iter().skip(index + 1).cloned());
            break;
        }
        if !arg.starts_with("--") {
            parsed.rest.push(arg.clone());
            index += 1;
            continue;
        }
        let body = &arg[2..];
        let (key, value) = if let Some((key, value)) = body.split_once('=') {
            (to_camel_case(key), Some(value.to_string()))
        } else {
            (to_camel_case(body), None)
        };
        if let Some(value) = value {
            push_cli_value(&mut parsed.values, key, value);
            index += 1;
            continue;
        }
        if is_value_less_cli_flag(&key) {
            parsed.flags.insert(key);
            index += 1;
            continue;
        }
        let next = args.get(index + 1);
        if next.map(|value| value.starts_with("--")).unwrap_or(true) {
            parsed.flags.insert(key);
            index += 1;
            continue;
        }
        push_cli_value(
            &mut parsed.values,
            key.clone(),
            next.cloned().unwrap_or_default(),
        );
        index += 2;
        if key == "agent" || key == "agents" {
            while index < args.len() && !args[index].starts_with("--") {
                push_cli_value(&mut parsed.values, key.clone(), args[index].clone());
                index += 1;
            }
        }
    }
    parsed
}

/*
CDXC:AgentSkills 2026-06-19-18:44:
Agent-skill commands must treat known value-less switches such as --json as flags even when a positional skill name follows, so `gxserver agent-skills status --json ghostex-browser-use` keeps the skill name in rest while enabling JSON output.
*/
fn is_value_less_cli_flag(key: &str) -> bool {
    matches!(key, "json")
}

fn push_cli_value(values: &mut HashMap<String, Vec<String>>, key: String, value: String) {
    values.entry(key).or_default().push(value);
}

fn to_camel_case(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = false;
    for character in value.chars() {
        if character == '-' {
            uppercase_next = true;
            continue;
        }
        if uppercase_next {
            output.extend(character.to_uppercase());
            uppercase_next = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn json_array(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}

/// CDXC:ServerDaemon 2026-10-25 WHY:
/// On 2026-10-25 a remote client's `gxserver start --background` over SSH started gxserver inside the SSH login while the app was starting its own: it took the port, so the app's launchd start ended at once and the app showed the startup failure report, and because an SSH login is outside the user's GUI security session, `security` could not read the Keychain and Composio read as signed out. When the app's own gxserver launchd job runs this binary, `gxserver start` kickstarts that job instead, so every start of the app's gxserver runs in the GUI session, and two starts at once meet in launchd instead of racing for the port. Without that job (a server-only install, another binary, nobody signed in at the screen) it starts detached as before.
/// SEE-ALSO: `gpui_spawn_local_gxserver_launchd_job` in apps/desktop/src/app/helpers/board_gxserver/gxserver_health_and_daemon.rs, which writes the job.
#[cfg(target_os = "macos")]
pub(crate) mod macos_app_launchd_job {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use anyhow::Result;

    /// The Ghostex app's gxserver LaunchAgent, found when its plist names a program that runs and
    /// the user's GUI session is there to run it.
    #[derive(Clone, Debug)]
    pub(crate) struct AppJob {
        pub(crate) label: String,
        /// `gui/<uid>/<label>`.
        pub(crate) target: String,
        /// `gui/<uid>`.
        pub(crate) domain: String,
        pub(crate) plist: PathBuf,
        pub(crate) program: PathBuf,
        /// The job's process, when launchd runs one.
        pub(crate) pid: Option<u32>,
    }

    impl AppJob {
        pub(crate) fn runs(&self, binary: &Path) -> bool {
            same_file(&self.program, binary)
        }
    }

    pub(crate) fn find() -> Option<AppJob> {
        let port = crate::config::read_selected_local_api_port().ok()?;
        let label = format!("{}gxserver", ghostex_paths::launchd_label_prefix(port));
        let plist = PathBuf::from(std::env::var_os("HOME")?)
            .join("Library/LaunchAgents")
            .join(format!("{label}.plist"));
        let program = job_program(&plist)?;
        if !is_executable(&program) {
            return None;
        }
        let domain = format!("gui/{}", unsafe { libc::getuid() });
        if !launchctl(&["print", &domain]).0 {
            return None;
        }
        let target = format!("{domain}/{label}");
        let (loaded, output) = launchctl(&["print", &target]);
        Some(AppJob {
            pid: loaded.then(|| launchctl_print_pid(&output)).flatten(),
            label,
            target,
            domain,
            plist,
            program,
        })
    }

    /// Loads the job if launchd does not have it, then `launchctl kickstart` (`-k` with `restart`,
    /// which ends a running instance first; without it a running job is left alone).
    pub(crate) fn start(job: &AppJob, restart: bool) -> std::result::Result<(), String> {
        if !launchctl(&["print", &job.target]).0 {
            let (loaded, output) =
                launchctl(&["bootstrap", &job.domain, &job.plist.to_string_lossy()]);
            if !loaded && !launchctl(&["print", &job.target]).0 {
                return Err(format!("launchctl could not load {}: {output}", job.label));
            }
        }
        let arguments: &[&str] = if restart {
            &["kickstart", "-k", &job.target]
        } else {
            &["kickstart", &job.target]
        };
        let (started, output) = launchctl(arguments);
        if started {
            Ok(())
        } else {
            Err(format!("launchctl could not start {}: {output}", job.label))
        }
    }

    /// The first `pid = N` line of `launchctl print` is the job's own (later ones are nested).
    pub(crate) fn launchctl_print_pid(output: &str) -> Option<u32> {
        output.lines().find_map(|line| {
            let (key, value) = line.trim().split_once(" = ")?;
            (key == "pid").then(|| value.trim().parse().ok()).flatten()
        })
    }

    fn is_executable(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }

    /// The job's label when it was kickstarted, `None` when there is no such job to use.
    pub(super) fn kickstart_if_it_runs(binary: &Path) -> Result<Option<String>> {
        let Some(job) = find().filter(|job| job.runs(binary)) else {
            return Ok(None);
        };
        // A refused job must not leave a remote client with no gxserver at all: fall back to the
        // detached start (which may lack Keychain access) and say why.
        if let Err(error) = start(&job, false) {
            eprintln!("{error}; starting gxserver detached");
            return Ok(None);
        }
        Ok(Some(job.label))
    }

    #[cfg(test)]
    pub(super) fn job_program_is(plist: &Path, binary: &Path) -> bool {
        job_program(plist).is_some_and(|program| same_file(&program, binary))
    }

    /// The program a job's plist runs (`ProgramArguments.0`), `None` when it cannot be read.
    pub(crate) fn job_program(plist: &Path) -> Option<PathBuf> {
        let output = Command::new("/usr/bin/plutil")
            .args(["-extract", "ProgramArguments.0", "raw", "-o", "-"])
            .arg(plist)
            .output()
            .ok()?;
        let program = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (output.status.success() && !program.is_empty()).then(|| PathBuf::from(program))
    }

    fn same_file(a: &Path, b: &Path) -> bool {
        match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{job_program_is, launchctl_print_pid};

        #[test]
        fn launchctl_print_pid_reads_the_jobs_own_pid() {
            let running = "gui/501/com.madda.ghostex.gxserver = {\n\tactive count = 1\n\tstate = running\n\tprogram = /Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/gxserver\n\tpid = 4242\n\tresource coalition = {\n\t\tpid = 1\n\t}\n}\n";
            assert_eq!(launchctl_print_pid(running), Some(4242));
            let stopped = "gui/501/com.madda.ghostex.gxserver = {\n\tstate = not running\n\tlast exit code = 0\n}\n";
            assert_eq!(launchctl_print_pid(stopped), None);
        }

        #[test]
        fn only_a_job_that_runs_this_binary_is_used() {
            let folder = tempfile::tempdir().unwrap();
            let binary = folder.path().join("gxserver");
            let other = folder.path().join("other-gxserver");
            std::fs::write(&binary, "").unwrap();
            std::fs::write(&other, "").unwrap();
            let plist = folder.path().join("job.plist");
            std::fs::write(
                &plist,
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict><key>ProgramArguments</key><array><string>{}</string><string>--foreground</string></array></dict></plist>\n",
                    binary.display()
                ),
            )
            .unwrap();
            assert!(job_program_is(&plist, &binary));
            assert!(!job_program_is(&plist, &other));
            assert!(!job_program_is(
                &folder.path().join("missing.plist"),
                &binary
            ));
        }
    }

    pub(crate) fn launchctl(arguments: &[&str]) -> (bool, String) {
        match Command::new("/bin/launchctl").args(arguments).output() {
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let text = if stderr.is_empty() {
                    String::from_utf8_lossy(&output.stdout).trim().to_string()
                } else {
                    stderr
                };
                (output.status.success(), text)
            }
            Err(error) => (false, error.to_string()),
        }
    }
}

fn spawn_detached(command: &mut Command) -> Result<u32> {
    #[cfg(windows)]
    {
        return crate::platform::process::spawn_server(command.get_program())
            .with_context(|| "spawn gxserver background");
    }
    #[cfg(not(windows))]
    {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }
        let child = command
            .spawn()
            .with_context(|| "spawn gxserver background")?;
        Ok(child.id())
    }
}

#[allow(dead_code)]
fn _home_dir_for_tests(path: PathBuf) -> GxserverForegroundOptions {
    GxserverForegroundOptions {
        build_identity: Some(create_source_build_identity(GXSERVER_VERSION)),
        home_dir: Some(path),
        version: GXSERVER_VERSION.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GitHub #476: a remote client's `gxserver start` on a Mac running the Ghostex app keeps the
    /// app's gxserver whatever its build, and otherwise starts the app's job, never the package.
    #[test]
    fn the_app_gxserver_is_kept_or_started_through_its_job() {
        // The app's job answers: kept, even at another build than the package.
        assert_eq!(app_server_start(Some(700), Some(700)), AppServerStart::Keep);
        // Nothing answers: start the app's job (a live job process is left to finish starting).
        assert_eq!(
            app_server_start(None, None),
            AppServerStart::StopThenStartApp { restart: false }
        );
        assert_eq!(
            app_server_start(None, Some(700)),
            AppServerStart::StopThenStartApp { restart: false }
        );
        // Another gxserver (a package one an older connect left) answers: stop it, start the app's.
        assert_eq!(
            app_server_start(Some(800), None),
            AppServerStart::StopThenStartApp { restart: false }
        );
        assert_eq!(
            app_server_start(Some(800), Some(700)),
            AppServerStart::StopThenStartApp { restart: true }
        );
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parse_cli_args_keeps_skill_name_after_json_flag() {
        let parsed = parse_cli_args(&args(&["--json", "ghostex-browser-use"]));

        assert!(parsed.flags.contains("json"));
        assert_eq!(parsed.rest, vec!["ghostex-browser-use".to_string()]);
        assert_eq!(parsed.values.get("json"), None);
    }

    #[test]
    fn parse_cli_args_still_consumes_known_value_options() {
        let parsed = parse_cli_args(&args(&[
            "--repository",
            "/tmp/skills",
            "--json",
            "ghostex-browser-use",
        ]));

        assert!(parsed.flags.contains("json"));
        assert_eq!(parsed.rest, vec!["ghostex-browser-use".to_string()]);
        assert_eq!(
            parsed.values.get("repository"),
            Some(&vec!["/tmp/skills".to_string()])
        );
    }

    #[test]
    fn the_endpoint_answer_offers_the_notification_stream() {
        let answer = endpoint_answer(58744, 1);
        let capabilities = answer["capabilities"].as_array().expect("capabilities");
        assert_eq!(
            capabilities,
            &vec![
                Value::from("sendRequestLedger"),
                Value::from("voiceAgent"),
                Value::from("notificationStream"),
                Value::from("pushNotifications"),
            ]
        );
        // The token names the stream the event socket filters on.
        assert_eq!(
            crate::server::EventStreamMode::from_query(Some(
                ghostex_gx_protocol::GXSERVER_EVENTS_STREAM_NOTIFICATIONS
            )),
            crate::server::EventStreamMode::Notifications
        );
    }
}
