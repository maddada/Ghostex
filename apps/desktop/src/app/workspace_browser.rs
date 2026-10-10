//! Which Browser profile a workspace's tabs use, and the wait for its disk-backed context.
use crate::cef::WorkspaceBrowserContextState;
use crate::*;
use futures::FutureExt as _;

const WORKSPACE_BROWSER_CONTEXT_WAIT: Duration = Duration::from_secs(10);

thread_local! {
    static WORKSPACE_BROWSER_CONTEXT_WAITS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// CDXC:Browser 2026-10-09 DECISION:
/// User: each workspace has its own browser sign-ins (cookies), so a user can be signed in to github.com or linear.app as their work account in the Work workspace and as themselves in Personal. A workspace's Browser profile id is `workspace-<workspaceId>`; the default (Personal) workspace, passed as `None`, keeps today's Default profile so existing sign-ins stay where they are. Ids that are not already lowercase letters, digits and dashes (or too long for a cache folder name) are hashed so two workspaces never share a folder.
pub(crate) fn workspace_browser_profile(workspace_id: Option<&str>) -> String {
    let Some(workspace_id) = workspace_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return BrowserProfileId::default_profile().cef_profile_string();
    };
    let profile = format!("workspace-{workspace_id}");
    if cef::cef_profile_cache_segment(&profile).is_some() {
        return profile;
    }
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(workspace_id.as_bytes()));
    format!("workspace-h{}", &digest[..32])
}

/// CDXC:Browser 2026-10-09 WHY:
/// Only the Default Browser profile follows the workspace. Generated "Profile N" ids are app-global and memory-backed, and stay that way: they are reused across windows and workspaces, so making them disk-backed would carry one workspace's sign-ins into another's "Profile 2".
pub(crate) fn browser_cef_profile_for_workspace(
    profile_id: BrowserProfileId,
    workspace_id: Option<&str>,
) -> String {
    if profile_id == BrowserProfileId::default_profile() {
        workspace_browser_profile(workspace_id)
    } else {
        profile_id.cef_profile_string()
    }
}

/// Signs a workspace's Browser profile out of everything (see `cef::clear_workspace_browser_profile`).
/// The Workspaces settings page's "Sign out of all sites" (`clearWorkspaceBrowserSignins`).
pub(crate) fn clear_workspace_browser_signins(workspace_id: &str) -> Result<(), String> {
    let profile = workspace_browser_profile(Some(workspace_id));
    if !cef::cef_profile_is_workspace(&profile) {
        return Err("The default workspace uses the Default Browser profile".into());
    }
    cef::clear_workspace_browser_profile(&profile).map_err(|error| error.to_string())
}

/// Starts a workspace profile's context with no page on it, for work that needs only the context
/// (Settings' Forget all answers), and resolves true once it is ready. False for other profiles,
/// when it fails or takes too long, and when the CEF runtime is not running: that stays deferred
/// until a web view needs it (CDXC:CefRuntime 2026-09-19).
pub(crate) fn start_workspace_browser_context(
    profile: &str,
    cx: &App,
) -> impl std::future::Future<Output = bool> + 'static {
    let state = (cef::cef_profile_is_workspace(profile) && cef::context_initialized())
        .then(|| cef::prepare_workspace_browser_context(profile).ok())
        .flatten();
    let timer = cx
        .background_executor()
        .timer(WORKSPACE_BROWSER_CONTEXT_WAIT);
    async move {
        let receiver = match state {
            Some(WorkspaceBrowserContextState::Ready) => return true,
            Some(WorkspaceBrowserContextState::Pending(receiver)) => receiver,
            _ => return false,
        };
        let timer = timer.fuse();
        let mut receiver = receiver.fuse();
        futures::pin_mut!(timer);
        futures::select! {
            result = receiver => result.unwrap_or(false),
            _ = timer => false,
        }
    }
}

impl GhostexGpuiApp {
    /// The CEF profile a Browser tab or website view opens with in this window.
    pub(crate) fn browser_tab_cef_profile(&self, profile_id: BrowserProfileId) -> String {
        // The window's workspace (`None` = the default workspace, gx_store/workspaces.rs).
        let workspace_id = self.gx_store_window_non_default_workspace_id();
        browser_cef_profile_for_workspace(profile_id, workspace_id.as_deref())
    }

    /// True when `profile` can create a browser now. A workspace profile whose
    /// disk-backed context is still initializing returns false and re-runs the
    /// Browser and website-view reconcile when CEF reports it initialized.
    pub(crate) fn prepare_workspace_browser_context(
        &mut self,
        profile: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !cef::cef_profile_is_workspace(profile) {
            return true;
        }
        let receiver = match cef::prepare_workspace_browser_context(profile) {
            Ok(WorkspaceBrowserContextState::Ready) => return true,
            Ok(WorkspaceBrowserContextState::RuntimeNotReady) => return false,
            Ok(WorkspaceBrowserContextState::Pending(receiver)) => receiver,
            Err(error) => {
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.workspaceBrowserContext.failed",
                    serde_json::json!({ "error": error.to_string() }),
                );
                return false;
            }
        };
        if !WORKSPACE_BROWSER_CONTEXT_WAITS
            .with(|waits| waits.borrow_mut().insert(profile.to_string()))
        {
            return false;
        }
        let profile = profile.to_string();
        cx.spawn(async move |this, cx| {
            let timer = cx
                .background_executor()
                .timer(WORKSPACE_BROWSER_CONTEXT_WAIT)
                .fuse();
            let mut receiver = receiver.fuse();
            futures::pin_mut!(timer);
            let outcome = futures::select! {
                result = receiver => Some(result.unwrap_or(false)),
                _ = timer => None,
            };
            WORKSPACE_BROWSER_CONTEXT_WAITS.with(|waits| waits.borrow_mut().remove(&profile));
            if outcome != Some(true) {
                // Not ready in time or failed: the next reconcile asks again and
                // waits on the same context; no memory-backed context is used.
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.workspaceBrowserContext.notReady",
                    serde_json::json!({ "timedOut": outcome.is_none() }),
                );
                return;
            }
            let _ = this.update_in(cx, |app, window, cx| {
                app.sync_active_browser_tab_to_surface(window, cx);
                app.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
                cx.notify();
            });
        })
        .detach();
        false
    }
}
