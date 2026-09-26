//! Runs the client against the daemon on this computer and reports what the core made of it.
//!
//! Usage: `cargo run --release --example live -- [seconds]` (default 30).
//!
//! The daemon is found the way it publishes itself: the bearer token in
//! `<gxserver state dir>/auth/token`, the port in `<gxserver state dir>/runtime/server.json`
//! (else `GHOSTEX_GXSERVER_DEV_PORT`, else the fixed local port). The report holds counts and frame
//! types; only an invariant violation names project and session ids. This is tooling, not a test
//! suite.

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ghostex_gx_client::{ClientDiagnostic, ClientOutput, GxClient, GxClientConfig};
use ghostex_gx_core::protocol::{GXSERVER_LOCAL_HOST, GXSERVER_LOCAL_PORT};
use ghostex_gx_core::{
    ActiveGroup, ConnectionPhase, Core, Effect, Event, Loadable, MachineId, ProjectKey,
};
use ghostex_paths::GhostexPaths;
use serde_json::Value;

fn main() -> ExitCode {
    let seconds = match std::env::args().nth(1).map(|value| value.parse::<u64>()) {
        None => 30,
        Some(Ok(seconds)) => seconds,
        Some(Err(_)) => {
            eprintln!("usage: live [seconds]");
            return ExitCode::from(2);
        }
    };
    let state_dir = GhostexPaths::resolve().gxserver_state_dir();
    let auth_token = match std::fs::read_to_string(state_dir.join("auth").join("token")) {
        Ok(token) if !token.trim().is_empty() => token.trim().to_string(),
        _ => {
            eprintln!("no gxserver auth token under the resolved state directory");
            return ExitCode::FAILURE;
        }
    };
    let metadata_port = std::fs::read_to_string(state_dir.join("runtime").join("server.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|metadata| metadata["port"].as_u64())
        .and_then(|port| u16::try_from(port).ok());
    let env_port = std::env::var("GHOSTEX_GXSERVER_DEV_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok());
    let (port, port_source) = match (metadata_port, env_port) {
        (Some(port), _) => (port, "runtime/server.json"),
        (None, Some(port)) => (port, "GHOSTEX_GXSERVER_DEV_PORT"),
        (None, None) => (GXSERVER_LOCAL_PORT, "default"),
    };
    let base_url = format!("http://{GXSERVER_LOCAL_HOST}:{port}");
    println!("daemon: {base_url} (port from {port_source}), running for {seconds}s");

    let machine = MachineId::Local;
    let held_revision = Arc::new(AtomicI64::new(0));
    let (wake_sender, wakes) = mpsc::channel::<()>();
    let client = match GxClient::start(
        GxClientConfig {
            machine: machine.clone(),
            base_url,
            auth_token,
            client_id: "ghostex-gx-client-live-example".to_string(),
            held_revision: held_revision.clone(),
            // A tool wants to see every wire type; the desktop host leaves this off.
            forward_chat_frames: true,
            // A tool must never take the CLI's renderer commands away from the running app.
            renderer_commands: false,
        },
        move || {
            let _ = wake_sender.send(());
        },
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let mut core = Core::new();
    let mut report = Report::default();
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    // Halfway through, ask for a full snapshot the way `Effect::ResubscribePresentation` would,
    // so the run also covers a resubscribe on a live socket.
    let mut resubscribe_at = Some(started + Duration::from_secs(seconds / 2));
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        if resubscribe_at.is_some_and(|at| now >= at) {
            resubscribe_at = None;
            report.resubscribes_requested += 1;
            client.request_resubscribe();
        }
        let wait = deadline
            .min(resubscribe_at.unwrap_or(deadline))
            .saturating_duration_since(now);
        if wakes.recv_timeout(wait).is_err() {
            continue;
        }
        report.wakes += 1;
        let mut events = Vec::new();
        for output in client.drain() {
            match output {
                ClientOutput::Event(event) => {
                    report.note_event(&event, started);
                    events.push(event);
                }
                ClientOutput::Diagnostic(diagnostic) => report.note_diagnostic(diagnostic),
                // Never produced: this example does not register as the renderer target.
                ClientOutput::RendererCommand(_) => {}
            }
        }
        if events.is_empty() {
            continue;
        }
        report.bursts += 1;
        report.largest_burst = report.largest_burst.max(events.len());
        let applying = Instant::now();
        let output = core.handle_batch(events, now_ms());
        report.apply_total += applying.elapsed();
        report.apply_max = report.apply_max.max(applying.elapsed());
        if let Some(loaded) = core.presentation().loaded(&machine) {
            held_revision.store(loaded.revision, Ordering::Release);
            if report.loaded_after.is_none() {
                report.loaded_after = Some(started.elapsed());
            }
        }
        for effect in output.effects {
            *report.effects.entry(effect_name(&effect)).or_default() += 1;
            if let Effect::ResubscribePresentation { .. } = effect {
                client.request_resubscribe();
            }
        }
        check_invariants(&core, &machine, &mut report.violations);
    }

    report.print(&core, &machine, &client);
    drop(client);
    if report.violations.is_empty() && report.parse_failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

fn effect_name(effect: &Effect) -> &'static str {
    match effect {
        Effect::ResubscribePresentation { .. } => "resubscribePresentation",
        Effect::RefetchSidebarHud { .. } => "refetchSidebarHud",
        Effect::RefetchNotificationFeed { .. } => "refetchNotificationFeed",
        Effect::RememberProjectSession { .. } => "rememberProjectSession",
        Effect::ReportSkippedRows { .. } => "reportSkippedRows",
        _ => "other",
    }
}

#[derive(Default)]
struct Report {
    wakes: u64,
    bursts: u64,
    largest_burst: usize,
    apply_total: Duration,
    apply_max: Duration,
    loaded_after: Option<Duration>,
    resubscribes_requested: u64,
    phases: Vec<String>,
    frames: BTreeMap<String, u64>,
    domain_project_reads: Vec<usize>,
    parse_failures: Vec<String>,
    other_diagnostics: Vec<String>,
    effects: BTreeMap<&'static str, u64>,
    violations: Vec<String>,
}

impl Report {
    fn note_event(&mut self, event: &Event, started: Instant) {
        match event {
            Event::Frame { frame, .. } => {
                *self
                    .frames
                    .entry(frame.event_type().to_string())
                    .or_default() += 1;
            }
            Event::Connection { update, .. } => self.phases.push(format!(
                "+{:.3}s {update:?}",
                started.elapsed().as_secs_f64()
            )),
            Event::DomainProjectsRead { projects, .. } => {
                self.domain_project_reads.push(projects.len());
            }
            _ => {}
        }
    }

    fn note_diagnostic(&mut self, diagnostic: ClientDiagnostic) {
        match diagnostic {
            ClientDiagnostic::FrameParseFailed {
                event_type,
                error,
                resubscribe_scheduled,
            } => self.parse_failures.push(format!(
                "{}: {error} (resubscribe scheduled: {resubscribe_scheduled})",
                event_type.as_deref().unwrap_or("<no type>")
            )),
            other => self.other_diagnostics.push(format!("{other:?}")),
        }
    }

    fn print(&self, core: &Core, machine: &MachineId, client: &GxClient) {
        println!("\n== connection ==");
        for phase in &self.phases {
            println!("  {phase}");
        }
        let connection = core
            .presentation()
            .machine(machine)
            .map(|entry| entry.connection().clone())
            .unwrap_or_default();
        println!(
            "  store phase at exit: {:?} (live: {})",
            connection.phase,
            connection.phase == ConnectionPhase::Live
        );

        println!("\n== socket ==");
        let stats = client.stats();
        println!(
            "  connects {}, subscribes sent {} (of which {} requested by this tool)",
            stats.connects, stats.subscribes, self.resubscribes_requested
        );
        println!(
            "  frames received {} ({} bytes): {} apiRequestHandled dropped unparsed, {} chat dropped unparsed, {} forwarded, {} parse failures",
            stats.frames_received,
            stats.bytes_received,
            stats.api_request_handled_dropped,
            stats.chat_frames_dropped,
            stats.frames_forwarded,
            stats.parse_failures
        );
        println!(
            "  wakes {}, bursts {}, largest burst {} events, apply total {:?}, apply max {:?}",
            self.wakes, self.bursts, self.largest_burst, self.apply_total, self.apply_max
        );

        println!("\n== forwarded frames by type ==");
        for (event_type, count) in &self.frames {
            println!("  {count:>6}  {event_type}");
        }
        let renderer_commands = self.frames.get("rendererCommand").copied().unwrap_or(0);
        let chat_snapshots = self.frames.get("sessionChatSnapshot").copied().unwrap_or(0);
        println!(
            "  rendererCommand frames: {renderer_commands} (must be 0: this socket never registers); sessionChatSnapshot frames: {chat_snapshots} (any seen here were caused by another client's subscribe)"
        );

        println!("\n== domain project reads ==");
        println!("  {:?} rows per read", self.domain_project_reads);

        println!("\n== diagnostics ==");
        println!("  parse failures: {}", self.parse_failures.len());
        for failure in self.parse_failures.iter().take(20) {
            println!("    {failure}");
        }
        println!("  other: {}", self.other_diagnostics.len());
        for diagnostic in self.other_diagnostics.iter().take(20) {
            println!("    {diagnostic}");
        }

        println!("\n== effects ==");
        for (effect, count) in &self.effects {
            println!("  {count:>6}  {effect}");
        }

        println!("\n== store ==");
        match core.presentation().loaded(machine) {
            None => println!("  NOT LOADED"),
            Some(loaded) => {
                println!(
                    "  loaded after {:?}, revision {}, projects {}, groups {}, sessions {}",
                    self.loaded_after.unwrap_or_default(),
                    loaded.revision,
                    loaded.projects().len(),
                    loaded.groups().len(),
                    loaded.session_count()
                );
                let chat_projects = loaded
                    .projects()
                    .iter()
                    .filter(|project| {
                        core.presentation()
                            .is_chat_project(&ProjectKey::local(project.project_id.as_str()))
                    })
                    .count();
                println!("  chat projects {chat_projects}");
            }
        }
        println!("  tabs generation {}", core.tabs_generation());
        let tabs = match core.active_tab_sessions() {
            Loadable::NotLoaded => "not loaded".to_string(),
            Loadable::Missing => "missing".to_string(),
            Loadable::Loaded(tabs) => format!("{} tabs", tabs.len()),
        };
        let group = match &core.focus().active_group {
            None => "none",
            Some(ActiveGroup::Project(_)) => "project",
            Some(ActiveGroup::Chats(_)) => "chats",
            Some(ActiveGroup::Subgroup { .. }) => "subgroup",
        };
        println!("  active group kind {group}, active tab list: {tabs}");

        println!("\n== invariants ==");
        if self.violations.is_empty() {
            println!("  all hold (checked after every burst)");
        } else {
            println!("  {} violations", self.violations.len());
            for violation in self.violations.iter().take(20) {
                println!("    {violation}");
            }
        }
    }
}

/// The store invariants the gx-core replay tool checks (`packages/gx-core/examples/replay.rs`,
/// `check_invariants`). An example cannot import another crate's example, so the checks are
/// repeated here; keep the two in step.
fn check_invariants(core: &Core, machine: &MachineId, violations: &mut Vec<String>) {
    let store = core.presentation();
    let Some(loaded) = store.loaded(machine) else {
        return;
    };
    let revision = loaded.revision;
    for session in loaded.server_sessions() {
        if loaded.project(&session.project_id).is_none() {
            violations.push(format!(
                "revision {revision}: session {}/{} has no project",
                session.project_id, session.session_id
            ));
        }
        let group = loaded.groups().iter().find(|group| {
            group.project_id == session.project_id && group.group_id == session.group_id
        });
        match group {
            None => violations.push(format!(
                "revision {revision}: session {}/{} names group {}, which does not exist",
                session.project_id, session.session_id, session.group_id
            )),
            Some(group) if !group.session_ids.contains(&session.session_id) => {
                violations.push(format!(
                    "revision {revision}: session {}/{} is missing from its group {}",
                    session.project_id, session.session_id, session.group_id
                ));
            }
            Some(_) => {}
        }
    }
    for group in loaded.groups() {
        for session_id in &group.session_ids {
            if loaded
                .server_session(&group.project_id, session_id)
                .is_none()
            {
                violations.push(format!(
                    "revision {revision}: group {} lists {session_id}, which has no row",
                    group.group_id
                ));
            }
        }
    }
    for project in loaded.projects() {
        let group = ActiveGroup::Project(ProjectKey::local(project.project_id.as_str()));
        match store.tab_sessions(&group) {
            Loadable::NotLoaded | Loadable::Missing => violations.push(format!(
                "revision {revision}: tab sessions of loaded project {} read as not loaded or missing",
                project.project_id
            )),
            Loadable::Loaded(tabs) => {
                for tab in tabs {
                    if store.session(&tab.key).is_none() {
                        violations.push(format!(
                            "revision {revision}: tab {}/{} does not resolve to a session",
                            tab.key.project_id, tab.key.session_id
                        ));
                    }
                }
            }
        }
    }
    for key in core
        .focus()
        .focused_session
        .iter()
        .chain(&core.focus().visible_sessions)
    {
        if key.machine == *machine && store.session(key).is_none() {
            violations.push(format!(
                "revision {revision}: focus points at missing session {}/{}",
                key.project_id, key.session_id
            ));
        }
    }
    if let Some(group) = &core.focus().active_group {
        match store.tab_session_keys(group) {
            Loadable::Loaded(keys) => {
                if let Some(focused) = &core.focus().focused_session {
                    let is_tab = store
                        .session(focused)
                        .is_some_and(|session| session.visible_in_sidebar_by_default);
                    if is_tab && !keys.contains(focused) {
                        violations.push(format!(
                            "revision {revision}: the active group's tab list does not hold the focused session {}/{}",
                            focused.project_id, focused.session_id
                        ));
                    }
                }
            }
            Loadable::NotLoaded | Loadable::Missing => violations.push(format!(
                "revision {revision}: the active group does not resolve to a tab list"
            )),
        }
    }
}
