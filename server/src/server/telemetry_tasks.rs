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
    let flush = crate::telemetry::spawn_flush_task(state.shutdown_tx.subscribe());
    let heartbeat_state = state.clone();
    let heartbeat = crate::telemetry::spawn_heartbeat_task(
        state.paths.clone(),
        state.shutdown_tx.subscribe(),
        move || collect_heartbeat_snapshot(&heartbeat_state, &install_created_at),
    );
    TelemetryTasks { flush, heartbeat }
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
    let extension_count = state.extension_registry.list().ok()?.len();
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
        remote_machine_count: settings.remote_machine_count,
        days_since_install: crate::telemetry::heartbeat::days_since_install(install_created_at),
    })
}
