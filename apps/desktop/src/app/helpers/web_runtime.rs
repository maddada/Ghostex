//! The optional web runtime (CEF): whether it is on this computer, being installed, running or
//! broken, and what a web view shows in its place. Every view that needs a browser engine (the
//! Browser, Code, Files' HTML pages, drawings and media, website and extension views) asks here
//! instead of calling into `cef`, and the Plugins row reads the same state.
//!
//! CDXC:CefRuntime 2026-09-28 DECISION:
//! User: "I want the user to be able to actually use almost all of the app without even installing or running the CEF browser (so we make the browser optional like we did for the code editor)". CEF is an optional component like the Code view's editor: nothing is downloaded until the user presses Install on a web view or in Plugins, and a runtime that cannot start shows its error on the web view instead of stopping the app. Only a web view about to be shown used to start it; since 2026-10-23 an installed runtime is also warmed up after launch (CDXC:CefRuntime 2026-10-23 in app/cef_deferred_startup.rs).
//! SEE-ALSO: docs/2026-09-28/gpui-modals-migration/CEF-OPTIONAL.md, apps/desktop/src/app/web_runtime_install.rs (install, start, uninstall), apps/desktop/src/app/render/web_runtime_prompt.rs (the prompt card), apps/desktop/src/cef_component_window.rs (component checks), apps/desktop/src/cef/windows.rs (the delay-loaded libcef.dll).

use std::sync::Mutex;
use std::sync::OnceLock;

use crate::component_store::ComponentStoreProgressPhase;

/// Where the web runtime stands in this process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WebRuntimeState {
    /// No runtime on this computer: none next to the app, no `GHOSTEX_CEF_DIR`, and no current
    /// version in the component store.
    NotInstalled,
    /// The user asked for it; the component store is fetching, verifying or unpacking it.
    Installing(ComponentStoreProgressPhase),
    /// On disk and not started in this process; the first web view that is shown starts it.
    Installed,
    /// `cef::initialize` returned; pages are created once its context is up.
    Running,
    /// Installing, verifying or starting failed; the message is shown on the web views.
    Failed(String),
}

static WEB_RUNTIME_STATE: Mutex<WebRuntimeState> = Mutex::new(WebRuntimeState::NotInstalled);

/// Whether the install that runs replaces an older runtime the user installed, so the web views
/// say "Updating" rather than "Installing".
static WEB_RUNTIME_INSTALL_IS_UPDATE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(crate) fn set_web_runtime_install_is_update(update: bool) {
    WEB_RUNTIME_INSTALL_IS_UPDATE.store(update, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn web_runtime_state() -> WebRuntimeState {
    WEB_RUNTIME_STATE
        .lock()
        .map(|state| state.clone())
        .unwrap_or(WebRuntimeState::NotInstalled)
}

pub(crate) fn set_web_runtime_state(state: WebRuntimeState) {
    if let Ok(mut current) = WEB_RUNTIME_STATE.lock() {
        *current = state;
    }
}

/// True when a browser area can be created: the runtime is on disk (it starts on first use) or
/// already running.
pub(crate) fn web_runtime_available() -> bool {
    matches!(
        web_runtime_state(),
        WebRuntimeState::Installed | WebRuntimeState::Running
    )
}

/// What the prompt's main button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WebRuntimePromptAction {
    Install,
    Retry,
}

/// The card a web view shows while it cannot show its page, in the Code view prompt's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WebRuntimePrompt {
    pub(crate) title: Option<String>,
    pub(crate) message: String,
    pub(crate) action: Option<WebRuntimePromptAction>,
}

/// The prompt for `subject` ("The Browser", "This file", an extension's name), or `None` while the
/// runtime can show pages.
pub(crate) fn web_runtime_install_prompt(subject: &str) -> Option<WebRuntimePrompt> {
    match web_runtime_state() {
        WebRuntimeState::Installed | WebRuntimeState::Running => None,
        WebRuntimeState::NotInstalled => Some(WebRuntimePrompt {
            title: None,
            message: format!(
                "{subject} needs the web runtime, {} optional install (one-time).\nWould you like to install it?",
                web_runtime_install_size_label()
                    .map(|size| format!("a ~{size}"))
                    .unwrap_or_else(|| "an".to_string())
            ),
            action: Some(WebRuntimePromptAction::Install),
        }),
        WebRuntimeState::Installing(phase) => Some(WebRuntimePrompt {
            title: Some(
                if WEB_RUNTIME_INSTALL_IS_UPDATE.load(std::sync::atomic::Ordering::Relaxed) {
                    "Updating web runtime"
                } else {
                    "Installing web runtime"
                }
                .to_string(),
            ),
            message: web_runtime_install_phase_label(phase).to_string(),
            action: None,
        }),
        WebRuntimeState::Failed(message) => Some(WebRuntimePrompt {
            title: Some("The web runtime needs another try".to_string()),
            message,
            action: Some(WebRuntimePromptAction::Retry),
        }),
    }
}

pub(crate) fn web_runtime_install_phase_label(phase: ComponentStoreProgressPhase) -> &'static str {
    match phase {
        ComponentStoreProgressPhase::Checking => "Checking the component…",
        ComponentStoreProgressPhase::Downloading => "Downloading the component…",
        ComponentStoreProgressPhase::Verifying => "Verifying the download…",
        ComponentStoreProgressPhase::Installing => "Installing the component…",
        ComponentStoreProgressPhase::Pruning => "Finishing installation…",
        ComponentStoreProgressPhase::Ready => "Starting the web runtime…",
    }
}

/// The download size of this platform's sealed CEF component ("140 MB"), read once from the
/// sealed manifest; `None` when this build has no manifest or no CEF entry for this platform.
pub(crate) fn web_runtime_install_size_label() -> Option<String> {
    static SIZE_BYTES: OnceLock<Option<u64>> = OnceLock::new();
    let bytes = (*SIZE_BYTES.get_or_init(|| {
        let path = crate::app::helpers::on_demand_component_manifest_path()?;
        let manifest = crate::component_store::OnDemandManifest::load(&path).ok()?;
        let platform = crate::component_store::current_platform().ok()?;
        manifest
            .components
            .get("cef")?
            .platforms
            .get(&platform)
            .map(|asset| asset.size_bytes)
            .filter(|bytes| *bytes > 0)
    }))?;
    const MEBIBYTE: u64 = 1024 * 1024;
    Some(format!("{} MB", bytes.div_ceil(MEBIBYTE)))
}
