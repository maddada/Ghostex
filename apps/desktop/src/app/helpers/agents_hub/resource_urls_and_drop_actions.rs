// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the bundled-skill name lookup, the
// Manage-ignored directory list, CEF entry/app-modal-host URL resolution,
// and the command-to-Agents drop placement + terminal runtime action-event
// helpers. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_bundled_agent_skill_name(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "browserUse" => Some("ghostex-browser-use"),
        "computerUse" => Some("ghostex-computer-use"),
        "embeddedBrowserUse" => Some("ghostex-embedded-browser-use"),
        "cli" => Some("ghostex-cli"),
        "fable56Orchestration" => Some("ghostex-fable-5.6-orchestration"),
        "manageBeads" => Some("ghostex-manage-beads"),
        "generateTitle" => Some("ghostex-auto-rename-session"),
        "moveCodexSession" => Some("ghostex-move-codex-session"),
        _ => None,
    }
}

pub(crate) const MANAGE_IGNORED_DIRECTORY_NAMES: &[&str] = &[
    ".cache",
    ".git",
    ".ghostex",
    ".gradle",
    ".next",
    ".nuxt",
    ".pytest_cache",
    ".ruff_cache",
    ".svelte-kit",
    ".turbo",
    ".tox",
    ".venv",
    ".vite",
    "DerivedData",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "storybook-static",
    "target",
    "tmp",
    "venv",
    "zig-out",
];

pub(crate) fn gpui_cef_html_entry_url(env_var: &str, entry_file_name: &str) -> Result<String> {
    if let Ok(value) = env::var(env_var) {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }

    let executable = env::current_exe().context("failed to resolve current executable")?;
    if let Some(bundle_root) = find_app_bundle_root(&executable) {
        let bundled = bundle_root
            .join("Contents/Resources/sidebar")
            .join(entry_file_name);
        if bundled.exists() {
            return Ok(file_url(&bundled));
        }
    }

    /*
    CDXC:GPUIWindowsAppModalBundle 2026-08-04:
    Packaged Windows and Linux builds stage every first-party CEF entry in
    dist/sidebar beside the executable, just like sidebar_url's packaged
    lookup. Resolve that directory before the compile-time checkout path so an
    installed Ghostex never loads modal-host, titlebar-host, Kanban, Manage, or
    Chat artifacts from the source tree that happened to build the binary.
    */
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if let Some(exe_dir) = executable.parent() {
        let packaged = exe_dir.join("dist/sidebar").join(entry_file_name);
        if packaged.exists() {
            return Ok(file_url(&packaged));
        }
    }

    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dist/sidebar")
        .join(entry_file_name);
    if local.exists() {
        return Ok(file_url(&local));
    }

    anyhow::bail!("GPUI CEF workarea bundle entry was not found")
}

/// The one-page document the first-launch tutorial player iframe points at.
/// Served from the app's synthetic https origin so YouTube's embed player has
/// a real embedding origin (CDXC:GPUIFirstLaunchTutorialVideo 2026-08-19).
pub(crate) fn gpui_tutorial_video_player_document() -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<style>html,body{{margin:0;height:100%;background:#000;overflow:hidden}}\
iframe{{border:0;display:block;height:100%;width:100%}}</style></head><body>\
<iframe src=\"{GHOSTEX_TUTORIAL_VIDEO_EMBED_URL}\" title=\"Ghostex introduction\" \
allow=\"accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share\" \
allowfullscreen></iframe></body></html>"
    )
    .into_bytes()
}

pub(crate) fn gpui_app_modal_host_resource_scope() -> cef::ManageDocsResourceScope {
    cef::ManageDocsResourceScope::new_remote(Arc::new(|relative_path: &str| {
        (relative_path == GPUI_TUTORIAL_VIDEO_PLAYER_RESOURCE_PATH)
            .then(gpui_tutorial_video_player_document)
    }))
}

pub(crate) fn app_modal_host_url() -> Result<String> {
    gpui_cef_html_entry_url("GHOSTEX_GPUI_APP_MODAL_HOST_URL", "modal-host.html")
        .context("failed to resolve GPUI app-modal host bundle URL")
}

/// Where a command tab dropped into the Agents workspace should land.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandToAgentsDropPlacement {
    PaneBody(WorkspaceDropZone),
    TabStrip(usize),
}

pub(crate) fn apply_gpui_terminal_runtime_action_events(
    osc_states: &mut HashMap<AgentsTerminalRuntimeSessionId, GpuiTerminalRuntimeOscState>,
    runtime_session_id: AgentsTerminalRuntimeSessionId,
    events: Vec<terminal_ghostty_surface::GhosttyRuntimeActionEvent>,
) -> bool {
    use terminal_ghostty_surface::GhosttyRuntimeActionEvent;

    let mut runtime_state_changed = false;
    for event in events {
        match event {
            GhosttyRuntimeActionEvent::OpenUrl { .. } => {}
            GhosttyRuntimeActionEvent::RingBell => {
                let state = osc_states.entry(runtime_session_id).or_default();
                state.bell_count = state.bell_count.wrapping_add(1);
                runtime_state_changed = true;
            }
            GhosttyRuntimeActionEvent::SetTitle { title } => {
                if title == TEMP_REMOTE_LOCAL_READY_TITLE || title == TEMP_REMOTE_SSH_READY_TITLE {
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        if title == TEMP_REMOTE_LOCAL_READY_TITLE {
                            "TEMP.remoteNewTerminal.localWrapperReady"
                        } else {
                            "TEMP.remoteNewTerminal.remoteCommandReady"
                        },
                        serde_json::json!({ "engine": "ghostty" }),
                    );
                }
                osc_states.entry(runtime_session_id).or_default().title = Some(title);
                runtime_state_changed = true;
            }
            GhosttyRuntimeActionEvent::Pwd { pwd } => {
                osc_states.entry(runtime_session_id).or_default().pwd = Some(pwd);
                runtime_state_changed = true;
            }
            GhosttyRuntimeActionEvent::MouseOverLink { url } => {
                let state = osc_states.entry(runtime_session_id).or_default();
                if state.hovered_link_url != url {
                    state.hovered_link_url = url;
                    runtime_state_changed = true;
                }
            }
            GhosttyRuntimeActionEvent::StartSearch { needle } => {
                let state = osc_states.entry(runtime_session_id).or_default();
                match (&mut state.search, needle) {
                    (Some(search), Some(needle)) => search.needle = needle,
                    (Some(_), None) => {}
                    (search @ None, needle) => {
                        *search = Some(GpuiTerminalSearchState {
                            needle: needle.unwrap_or_default(),
                            ..GpuiTerminalSearchState::default()
                        });
                    }
                }
                runtime_state_changed = true;
            }
            GhosttyRuntimeActionEvent::EndSearch => {
                let state = osc_states.entry(runtime_session_id).or_default();
                if state.search.take().is_some() {
                    runtime_state_changed = true;
                }
            }
            GhosttyRuntimeActionEvent::SearchTotal { total } => {
                let state = osc_states.entry(runtime_session_id).or_default();
                if let Some(search) = &mut state.search {
                    if search.total != total {
                        search.total = total;
                        runtime_state_changed = true;
                    }
                }
            }
            GhosttyRuntimeActionEvent::SearchSelected { selected } => {
                let state = osc_states.entry(runtime_session_id).or_default();
                if let Some(search) = &mut state.search {
                    if search.selected != selected {
                        search.selected = selected;
                        runtime_state_changed = true;
                    }
                }
            }
        }
    }
    runtime_state_changed
}
