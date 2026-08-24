// C1 wave-3 re-cluster: the neutral unavailable/sleeping placeholder signatures shown in Source/Kanban/Automate/Docs project-editor surfaces, plus the pending Agents Hub Source file open, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

/*
CDXC:GPUIProjectEditorPlaceholders 2026-06-28-17:09:
Source, Kanban, Automate, and Docs neutral placeholders are unavailable/loading/error surfaces only. Real Source/Kanban/Automate/Docs replacement is owned by the direct runtime URL plus normal-layout CefSurface gate, so placeholder rendering must not create CEF views, start code-server, run file operations, synthesize fallback URLs, persist private details, or add WKWebView/WebKit paths.
*/
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ProjectEditorPlaceholderSignature {
    pub(crate) mode: TitlebarMode,
    pub(crate) title: Option<String>,
    pub(crate) message: String,
    pub(crate) actions: Vec<ProjectEditorPlaceholderAction>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectEditorPlaceholderAction {
    HideCodeViewTab,
    InstallSourceComponent,
    RetrySourceLoad,
}

impl ProjectEditorPlaceholderSignature {
    pub(crate) fn for_mode(mode: TitlebarMode) -> Option<Self> {
        if matches!(mode, TitlebarMode::Agents | TitlebarMode::Browser) {
            return None;
        }

        Some(Self {
            mode,
            title: None,
            message: mode.placeholder_message().to_string(),
            actions: Vec::new(),
        })
    }

    pub(crate) fn for_source_code_server_launch_state(
        state: SourceCodeServerRuntimeLaunchState,
        loading_elapsed: Option<Duration>,
    ) -> Option<Self> {
        let signature = Self::for_mode(TitlebarMode::Source)?;
        let (title, message, actions) = match state {
            SourceCodeServerRuntimeLaunchState::Launching
                if loading_elapsed.is_some_and(|elapsed| {
                    elapsed < SOURCE_CODE_SERVER_LOADING_PLACEHOLDER_DELAY
                }) =>
            {
                (None, "".to_string(), Vec::new())
            }
            SourceCodeServerRuntimeLaunchState::Launching => (
                Some("Loading source...".to_string()),
                "".to_string(),
                Vec::new(),
            ),
            _ => return Some(signature),
        };

        Some(Self {
            title,
            message,
            actions,
            ..signature
        })
    }
}

/*
CDXC:GPUIProjectEditorSleepingPlaceholders 2026-06-28-17:09:
Selected sleeping/restored project-editor modes remain real layout participants with neutral text-only shell surfaces. Surface activation expresses wake intent for shell state; Browser hides existing CEF while sleeping, and Source/Kanban/Automate/Docs must not mount or replace runtime surfaces until their awake direct CEF gates permit it.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectEditorSleepingPlaceholderSignature {
    pub(crate) mode: TitlebarMode,
    pub(crate) title: &'static str,
    pub(crate) message: &'static str,
}

impl ProjectEditorSleepingPlaceholderSignature {
    pub(crate) fn for_mode(mode: TitlebarMode) -> Option<Self> {
        /*
        CDXC:GPUIProjectEditorSleepingPlaceholder 2026-06-28-17:09:
        Sleeping/restored Source, Browser, Kanban, Automate, and Docs visible copy is private-detail-free shell state. It must not include project/session/URL details, create CEF views, mount bridges, replace placeholders, or introduce WKWebView/WebKit paths.
        */
        let (title, message) = match mode {
            TitlebarMode::Source => (
                "Source is sleeping",
                "Source shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Browser => (
                "Browser is sleeping",
                "Browser shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Kanban => (
                "Kanban is sleeping",
                "Kanban shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Automate => (
                "Automate is sleeping",
                "Automate shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Manage => (
                "Docs is sleeping",
                "Docs shell state is retained. Activate this surface to restore it.",
            ),
            TitlebarMode::Agents => return None,
        };

        Some(Self {
            mode,
            title,
            message,
        })
    }
}

/*
Agents Hub Source opens are process-local navigation intent. Keep the validated
file and its containing workspace only until the matching owned Source surface
is ready, then hand the file to code-server IPC. Never persist or log either
path, and never accept a path that was not present in the current Hub catalog.
*/
pub(crate) struct PendingSourceFileOpen {
    pub(crate) file_path: PathBuf,
    pub(crate) project_path: PathBuf,
}
