//! Disk-backed request contexts for workspace Browser profiles (`workspace-<id>`).
use anyhow::{Context as _, Result};
use cef::rc::Rc as _;
use cef::{
    CefString, ImplCookieManager as _, ImplRequestContext as _, ImplRequestContextHandler,
    RequestContext, RequestContextHandler, WrapRequestContextHandler, wrap_request_context_handler,
};
use futures::channel::oneshot;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
};

const WORKSPACE_PROFILE_PREFIX: &str = "workspace-";
// A context starts at 0 (initializing).
const CONTEXT_READY: u8 = 1;
const CONTEXT_FAILED: u8 = 2;

#[derive(Default)]
struct ContextInit {
    status: Cell<u8>,
    waiters: RefCell<Vec<oneshot::Sender<bool>>>,
}

impl ContextInit {
    fn finish(&self, ready: bool) {
        self.status
            .set(if ready { CONTEXT_READY } else { CONTEXT_FAILED });
        for waiter in self.waiters.borrow_mut().drain(..) {
            let _ = waiter.send(ready);
        }
    }
}

struct WorkspaceContext {
    context: RequestContext,
    init: Rc<ContextInit>,
}

thread_local! {
    static WORKSPACE_CONTEXTS: RefCell<HashMap<String, WorkspaceContext>> = RefCell::new(HashMap::new());
}

wrap_request_context_handler! {
    struct WorkspaceContextHandler { init: Rc<ContextInit>, }
    impl RequestContextHandler {
        fn on_request_context_initialized(&self, context: Option<&mut RequestContext>) {
            self.init.finish(context.is_some());
        }
    }
}

/// Where a workspace Browser context stands for one browser-creation attempt.
pub(crate) enum WorkspaceBrowserContextState {
    Ready,
    /// The CEF runtime is not up yet; the caller retries on its next pass, like every surface.
    RuntimeNotReady,
    /// Resolves with `true` once CEF reports the context initialized, `false` if it failed.
    Pending(oneshot::Receiver<bool>),
}

pub(crate) fn cef_profile_is_workspace(profile: &str) -> bool {
    profile.starts_with(WORKSPACE_PROFILE_PREFIX)
}

/// CDXC:Browser 2026-10-10 WHY:
/// The folder must be a direct child of the root cache path (`<root cache>/workspace-<id>`, beside Chromium's own `Default`). CEF's Chrome runtime opens a disk profile only when `cache_path.DirName() == root_cache_path`; any deeper path (the first version used `<root cache>/profiles/<profile>`) logs "Cannot create profile at path" and silently gets an off-the-record profile, so every workspace sign-in was lost when the app closed.
fn workspace_profile_cache_dir(profile: &str) -> Result<PathBuf> {
    let segment = super::cef_profile_cache_segment(profile)
        .filter(|segment| cef_profile_is_workspace(segment))
        .with_context(|| format!("invalid workspace browser profile id {profile:?}"))?;
    Ok(super::cef_root_cache_path()?.join(segment))
}

fn clear_pending_marker(dir: &std::path::Path) -> PathBuf {
    dir.with_extension("clear-pending")
}

/// CDXC:Browser 2026-10-09 WHY:
/// A workspace's sign-ins must survive restarts, so its context is disk-backed under `<root cache>/<profile>` (see `workspace_profile_cache_dir`). CEF initializes a new disk-backed context asynchronously and `CreateBrowserSync` returns null until it is ready (the 2026-07-09 startup race that kept generated profiles in memory), so the context is created with a handler and no browser is created on it until `on_request_context_initialized` fires; callers get a receiver that resolves at that moment. There is no memory-context fallback: a failed context leaves the tab unloaded and reports the error.
pub(crate) fn prepare_workspace_browser_context(
    profile: &str,
) -> Result<WorkspaceBrowserContextState> {
    if !super::context_initialized() {
        super::request_runtime();
        return Ok(WorkspaceBrowserContextState::RuntimeNotReady);
    }
    let existing = WORKSPACE_CONTEXTS.with(|contexts| {
        contexts
            .borrow()
            .get(profile)
            .map(|entry| entry.init.clone())
    });
    let init = match existing {
        Some(init) => init,
        None => create_workspace_context(profile)?,
    };
    match init.status.get() {
        CONTEXT_READY => Ok(WorkspaceBrowserContextState::Ready),
        CONTEXT_FAILED => {
            // Forget it so the next pass creates the context again.
            WORKSPACE_CONTEXTS.with(|contexts| contexts.borrow_mut().remove(profile));
            anyhow::bail!("The browser profile for this workspace could not be opened")
        }
        _ => {
            let (sender, receiver) = oneshot::channel();
            init.waiters.borrow_mut().push(sender);
            Ok(WorkspaceBrowserContextState::Pending(receiver))
        }
    }
}

fn create_workspace_context(profile: &str) -> Result<Rc<ContextInit>> {
    let dir = workspace_profile_cache_dir(profile)?;
    let marker = clear_pending_marker(&dir);
    if marker.exists() {
        // A clear requested while the profile was open finishes here, before
        // Chromium reopens the directory.
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).context("failed to clear workspace browser profile");
            }
        }
        let _ = std::fs::remove_file(&marker);
    }
    std::fs::create_dir_all(&dir).context("failed to create workspace browser profile")?;
    let init = Rc::new(ContextInit::default());
    let mut handler = WorkspaceContextHandler::new(init.clone());
    let settings = cef::RequestContextSettings {
        cache_path: CefString::from(dir.to_string_lossy().as_ref()),
        persist_session_cookies: 1,
        ..Default::default()
    };
    let context = cef::request_context_create_context(Some(&settings), Some(&mut handler))
        .context("failed to create workspace browser request context")?;
    WORKSPACE_CONTEXTS.with(|contexts| {
        contexts.borrow_mut().insert(
            profile.to_string(),
            WorkspaceContext {
                context,
                init: init.clone(),
            },
        )
    });
    Ok(init)
}

pub(crate) fn workspace_browser_request_context(profile: &str) -> Result<RequestContext> {
    WORKSPACE_CONTEXTS.with(|contexts| {
        let contexts = contexts.borrow();
        let entry = contexts
            .get(profile)
            .context("Workspace browser profile is not prepared")?;
        anyhow::ensure!(
            entry.init.status.get() == CONTEXT_READY,
            "Workspace browser profile is not ready"
        );
        Ok(entry.context.clone())
    })
}

/// CDXC:Browser 2026-10-09 WHY:
/// Clearing signs the workspace out of everything. Cookies, HTTP auth, the HTTP cache and open connections are cleared live on an open context; local storage, IndexedDB and history have no live CEF API, so the profile folder is deleted now when no context has it open, and otherwise at the next start before the context reopens it (a `.clear-pending` marker beside the folder).
pub(crate) fn clear_workspace_browser_profile(profile: &str) -> Result<()> {
    let dir = workspace_profile_cache_dir(profile)?;
    let open_context = WORKSPACE_CONTEXTS.with(|contexts| {
        contexts
            .borrow()
            .get(profile)
            .filter(|entry| entry.init.status.get() == CONTEXT_READY)
            .map(|entry| entry.context.clone())
    });
    let Some(context) = open_context else {
        WORKSPACE_CONTEXTS.with(|contexts| contexts.borrow_mut().remove(profile));
        return match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("failed to clear workspace browser profile"),
        };
    };
    if let Some(cookies) = context.cookie_manager(None) {
        cookies.delete_cookies(None, None, None);
        cookies.flush_store(None);
    }
    context.clear_http_auth_credentials(None);
    context.clear_http_cache(None);
    context.close_all_connections(None);
    std::fs::write(clear_pending_marker(&dir), b"")
        .context("failed to schedule workspace browser profile clear")
}

pub(super) fn clear_workspace_browser_contexts() {
    WORKSPACE_CONTEXTS.with(|contexts| {
        for (_, entry) in contexts.borrow_mut().drain() {
            entry.init.finish(false);
        }
    });
}
