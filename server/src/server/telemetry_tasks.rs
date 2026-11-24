use super::*;

/*
CDXC:Telemetry 2026-08-26:
The `AppState`-shaped edge of `crate::telemetry`. The telemetry crate module is
deliberately free of server types (it is called from domain writes, extension
lifecycle, and HTTP handlers alike), so the two places that need `AppState` —
starting the tasks and collecting the heartbeat snapshot — live here, beside the
other background tasks, instead of pulling server internals into telemetry.

Both tasks are started ONLY from `run_gxserver_foreground`. A one-shot `ghostex`
CLI verb never calls `telemetry::init`, so it has no handle, queues nothing, and
starts nothing.
*/

pub(crate) struct TelemetryTasks {
    pub(crate) flush: tokio::task::JoinHandle<()>,
    pub(crate) heartbeat: tokio::task::JoinHandle<()>,
}

pub(crate) fn spawn_telemetry_tasks(
    state: &Arc<AppState>,
    install_created_at: String,
) -> TelemetryTasks {
    crate::telemetry::init(state.paths.clone(), state.metadata.server_id.as_str());
    let startup_settings = crate::shared_settings_file::read_shared_settings(&state.paths);
    crate::telemetry::session_mode_state(
        "app_start",
        "server",
        crate::telemetry::resolve_install_session_modes(startup_settings.as_ref()),
    );
    let flush = crate::telemetry::spawn_flush_task(state.shutdown_tx.subscribe());
    let heartbeat_state = state.clone();
    let snapshot_state = state.clone();
    let live_modes_state = state.clone();
    let heartbeat = crate::telemetry::spawn_heartbeat_task(
        state.paths.clone(),
        state.shutdown_tx.subscribe(),
        move || collect_heartbeat_snapshot(&heartbeat_state, &install_created_at),
        move || collect_settings_snapshot_facts(&snapshot_state),
        move || collect_live_session_mode_counts(&live_modes_state),
    );
    TelemetryTasks { flush, heartbeat }
}

fn collect_live_session_mode_counts(
    state: &Arc<AppState>,
) -> crate::telemetry::LiveSessionModeCounts {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return crate::telemetry::LiveSessionModeCounts::default();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    crate::telemetry::heartbeat::collect_live_session_mode_counts(&repository)
}

/*
Runs on the blocking worker inside the heartbeat task, so the SQLite read and
the registry's filesystem scan are fine here. Every failure resolves to a
skipped heartbeat rather than a partial one: a snapshot missing half its counts
would be worse than no data point at all, because it would silently drag every
average down.
*/
fn collect_heartbeat_snapshot(
    state: &Arc<AppState>,
    install_created_at: &str,
) -> Option<crate::telemetry::heartbeat::HeartbeatSnapshot> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let counts = crate::telemetry::heartbeat::collect_domain_counts(&repository);
    let agent_settings = crate::agents::read_agent_settings(&db)
        .ok()
        .map(Value::Object);
    let settings = crate::telemetry::heartbeat::collect_settings(&state.paths);
    /*
    The registry takes its own gate mutex and walks the installed directory, so
    a failure here means "we could not count them", not "there are none". A
    heartbeat that reported 0 in that case would look like an uninstall.
    */
    let installed = state.extension_registry.list().ok()?;
    let extension_count = installed.len();
    crate::telemetry::extensions::load_official_store_ids(&state.paths.extensions_dir);
    let enabled_extensions = crate::telemetry::extensions::enabled_extensions_report(
        installed
            .iter()
            .map(|extension| (extension.id.as_str(), extension.state.enabled)),
    );
    Some(crate::telemetry::heartbeat::HeartbeatSnapshot {
        project_count: counts.project_count,
        session_count: counts.session_count,
        running_session_count: counts.running_session_count,
        agents_used: counts.agents_used,
        custom_agent_executables: counts.custom_agent_executables,
        default_agent: crate::telemetry::heartbeat::default_agent_from_settings(
            agent_settings.as_ref(),
        ),
        preferred_interface: settings.preferred_interface,
        extension_count,
        built_in_extensions_on: settings.built_in_extensions_on,
        enabled_extensions,
        remote_machine_count: settings.remote_machine_count,
        days_since_install: crate::telemetry::heartbeat::days_since_install(install_created_at),
    })
}

/*
CDXC:Telemetry 2026-11-20:
What the daily `settings.snapshot` reports beyond the settings file
(telemetry/settings_snapshot.rs). Runs on the heartbeat's blocking worker, only
when the heartbeat is due. A part that cannot be read is left out of the event
(`None`), never reported as zero.

Accounts come from the snapshot gxserver already keeps for Settings > Accounts
and the sidebar meters (`accounts::runtime::current_snapshot`); it is read, not
refreshed, so the snapshot never runs Claude Swap or Codex Swap itself. When no
client has listed the accounts since gxserver started there is nothing to read,
and the account fields are left out.
*/
fn collect_settings_snapshot_facts(
    state: &Arc<AppState>,
) -> crate::telemetry::settings_snapshot::SnapshotFacts {
    use crate::telemetry::settings_snapshot::SnapshotFacts;
    let settings = crate::shared_settings_file::read_shared_settings(&state.paths);
    let remote_machine_count = settings
        .as_ref()
        .and_then(|settings| settings.get("remoteMachines"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let db = open_gxserver_database(&state.paths).ok();
    let workspace_count = db
        .as_ref()
        .and_then(|db| crate::workspaces::read_sidebar_workspaces(db).ok())
        .and_then(|value| value.get("workspaces")?.as_object().map(|map| map.len()));
    let discovered = crate::accounts::runtime::current_snapshot();
    let live_accounts = crate::telemetry::settings_snapshot::accounts_report_from_discovered(
        &state.paths,
        &discovered,
    );
    let accounts = match live_accounts {
        Some(report) => {
            if let Some(db) = db.as_ref() {
                if crate::telemetry::is_enabled(&state.paths) {
                    crate::telemetry::state::write_cached_accounts(db, &report);
                }
            }
            Some(report)
        }
        None => db
            .as_ref()
            .and_then(|db| crate::telemetry::state::read_cached_accounts(db)),
    };
    crate::telemetry::extensions::load_official_store_ids(&state.paths.extensions_dir);
    let mut store_extensions = Vec::new();
    let mut custom_extensions_installed = 0;
    for extension in state.extension_registry.list().unwrap_or_default() {
        if crate::telemetry::extensions::is_official_store_id(&extension.id) {
            store_extensions.push((extension.id.clone(), extension.manifest.version.clone()));
        } else {
            custom_extensions_installed += 1;
        }
    }
    store_extensions.sort();
    store_extensions.truncate(crate::telemetry::extensions::MAX_STORE_EXTENSION_LIST_LENGTH);
    SnapshotFacts {
        linux: crate::telemetry::settings_snapshot::read_linux_report(),
        remote_machine_count,
        workspace_count,
        boosts: crate::boosts::telemetry_summary(&state.paths),
        accounts,
        store_extensions,
        custom_extensions_installed,
    }
}
