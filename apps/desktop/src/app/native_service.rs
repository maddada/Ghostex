use crate::*;
use futures::StreamExt as _;
use ghostex_chat_runtime::ServiceWorker;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Set once the runtime failed to start in this process, so the app neither retries it on every
/// CEF readiness tick nor waits for anything it would have posted.
///
/// CDXC:CefRuntime 2026-09-25 WHY:
/// Start-up no longer depends on the QuickJS runtime (docs/2026-09-25/app-runtime-port/PLAN.md,
/// step 0 item 4): CEF, the sidebar list and client storage all come up without it, and a runtime
/// that throws is reported once instead of blocking CEF for the life of the app.
static NATIVE_SERVICE_START_FAILED: AtomicBool = AtomicBool::new(false);

pub(crate) struct NativeService {
    runtime: ServiceWorker,
    /// Arms the runtime thread's `native.runtime.trace` records.
    trace: Arc<AtomicBool>,
    sidebar_handler: cef::SidebarBridgeEventHandler,
    host_handler: cef::AppModalHostBridgeEventHandler,
}

impl NativeService {
    pub(crate) fn new(
        settings: cef::SidebarRuntimeSettingsSnapshot,
        bootstrap: Option<cef::SidebarGxserverBootstrap>,
        sidebar_handler: cef::SidebarBridgeEventHandler,
        host_handler: cef::AppModalHostBridgeEventHandler,
        cx: &mut gpui::App,
    ) -> Result<Entity<Self>, String> {
        let config = json!({
            "runtimeSettings": Self::settings(&settings),
            "gxserverBootstrap": Self::bootstrap(bootstrap.as_ref()),
            "bridgeFunctions": cef::sidebar_bridge_manifest::SIDEBAR_BRIDGE_FUNCTION_SPECS.iter().map(|spec|spec.js_function_name).collect::<Vec<_>>(),
        });
        let path = shared_settings::ghostex_storage_paths()
            .state_dir
            .join("client-storage.sqlite3");
        let (wake, mut wakes) = futures::channel::mpsc::unbounded();
        let trace = Arc::new(AtomicBool::new(
            crate::app::gx_store::runtime_trace_enabled(),
        ));
        let runtime = ServiceWorker::start(
            config,
            path,
            move || {
                let _ = wake.unbounded_send(());
            },
            trace.clone(),
        )
        .map_err(|error| {
            NATIVE_SERVICE_START_FAILED.store(true, Ordering::Relaxed);
            error.to_string()
        })?;
        Ok(cx.new(|cx: &mut gpui::Context<Self>| {
            cx.spawn(async move |service, cx| {
                while wakes.next().await.is_some() {
                    if service.update(cx, |service, _| service.pump()).is_err() {
                        break;
                    }
                }
            })
            .detach();
            Self {
                runtime,
                trace,
                sidebar_handler,
                host_handler,
            }
        }))
    }

    fn pump(&mut self) {
        for result in self.runtime.drain_ready() {
            match result {
                Ok(message) if message["kind"] == "trace" => {
                    crate::app::gx_store::trace_runtime_rpc(&message)
                }
                Ok(message) if message["kind"] == "traceEntry" => {
                    crate::app::gx_store::trace_runtime_handler(&message)
                }
                Ok(message) => {
                    crate::app::gx_store::trace_runtime_post(&message);
                    match message["kind"].as_str() {
                        Some("sidebar") => {
                            if let Some(event) = cef::sidebar_event_for_function(
                                message["name"].as_str().unwrap_or_default(),
                                message["payload"].as_str().unwrap_or_default().to_owned(),
                            ) {
                                (self.sidebar_handler)(event);
                            }
                        }
                        Some("nativeHost") => {
                            (self.host_handler)(cef::AppModalHostBridgeEvent::NativeHostMessage(
                                message["message"].to_string(),
                            ))
                        }
                        Some("modalHost") => (self.host_handler)(
                            cef::AppModalHostBridgeEvent::Message(message["message"].to_string()),
                        ),
                        Some("log")
                            if message["level"] == "error" || message["level"] == "warn" =>
                        {
                            Self::report(message["message"].as_str().unwrap_or_default())
                        }
                        _ => {}
                    }
                }
                Err(error) => Self::report(&error.to_string()),
            }
        }
    }
    fn report(error: &str) {
        support_logs::append(
            support_logs::GpuiSupportLog::CrashReports,
            "gpui.nativeService.error",
            json!({"error":error.lines().next().unwrap_or_default(),"stack":error.lines().skip(1).take(12).collect::<Vec<_>>()}),
        );
    }
    /// Whether the runtime failed to start in this process.
    pub(crate) fn start_failed() -> bool {
        NATIVE_SERVICE_START_FAILED.load(Ordering::Relaxed)
    }
    pub(crate) fn execute_app_owned_script(&mut self, source: &str) -> bool {
        crate::app::gx_store::trace_runtime_entry(source);
        match self.runtime.evaluate(source) {
            Ok(()) => true,
            Err(error) => {
                Self::report(&error.to_string());
                false
            }
        }
    }
    /// A settings save: re-arms the runtime thread's trace records.
    ///
    /// CDXC:Diagnostics 2026-09-25 WHY:
    /// The runtime no longer reads settings after start (nothing in it reacts to a change), so the
    /// saved settings are not sent into it any more; the trace switch is the one thing a save still
    /// has to reach on this thread.
    pub(crate) fn refresh_sidebar_runtime_settings(&mut self) {
        self.trace.store(
            crate::app::gx_store::runtime_trace_enabled(),
            Ordering::Relaxed,
        );
    }
    pub(crate) fn refresh_sidebar_gxserver_bootstrap(
        &mut self,
        bootstrap: Option<cef::SidebarGxserverBootstrap>,
    ) {
        let bootstrap = Self::bootstrap(bootstrap.as_ref());
        self.execute_app_owned_script(&format!("window.ghostexGpui.gxserverBootstrap = {bootstrap}; window.ghostexGpui.onGxserverBootstrapChanged?.({bootstrap}); void 0;"));
    }
    fn settings(settings: &cef::SidebarRuntimeSettingsSnapshot) -> Value {
        json!({"debuggingMode":settings.debugging_mode,"showBetaFeatures":settings.show_beta_features,"settings":serde_json::from_str::<Value>(&settings.saved_settings_json).unwrap_or_else(|_| json!({}))})
    }
    fn bootstrap(bootstrap: Option<&cef::SidebarGxserverBootstrap>) -> Value {
        bootstrap.map(|b| json!({"baseUrl":b.base_url,"authToken":b.auth_token,"protocolVersion":b.protocol_version,"clientId":b.client_id,"initialActiveProjectId":b.initial_active_project_id,"focusedSessionId":b.focused_session_id,"visibleSessionIds":b.visible_session_ids})).unwrap_or_else(||json!({}))
    }
}
