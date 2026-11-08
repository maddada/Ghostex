// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the bundled-skill name lookup, the
// Manage-ignored directory list, CEF entry/app-modal-host URL resolution,
// and the command-to-Agents drop placement + terminal runtime action-event
// helpers. See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::env;
use std::path::PathBuf;

use anyhow::{Context as _, Result};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_bundled_agent_skill_name(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "browserUse" => Some("ghostex-browser-use"),
        "computerUse" => Some("ghostex-computer-use"),
        "spaceo" => Some("ghostex-spaceo"),
        "embeddedBrowserUse" => Some("ghostex-embedded-browser-use"),
        "cli" => Some("ghostex-cli"),
        "help" => Some("ghostex-help"),
        "agentGuide" => Some("ghostex-agent-guide"),
        "agentsOrchestration" => Some("ghostex-agents"),
        "manageBeads" => Some("ghostex-manage-beads"),
        "generateTitle" => Some("ghostex-auto-rename-session"),
        "moveCodexSession" => Some("ghostex-move-codex-session"),
        "visuals" => Some("ghostex-visuals"),
        "secrets" => Some("ghostex-secrets"),
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
    if let Ok(value) = env::var(env_var)
        && !value.trim().is_empty()
    {
        return Ok(value);
    }

    let executable = env::current_exe().context("failed to resolve current executable")?;
    if let Some(bundle_root) = find_app_bundle_root(&executable) {
        let bundled = bundle_root
            .join("Contents/Resources/sidebar")
            .join(entry_file_name);
        if bundled.exists() {
            return file_url(&bundled);
        }
    }

    /*
    CDXC:PlatformSupport 2026-08-04:
    Packaged Windows and Linux builds stage every first-party CEF entry in
    dist/sidebar beside the executable. Resolve that directory before the
    compile-time checkout path so an installed Ghostex never loads Manage
    artifacts from the source tree that
    happened to build the binary.
    */
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if let Some(exe_dir) = executable.parent() {
        let packaged = exe_dir.join("dist/sidebar").join(entry_file_name);
        if packaged.exists() {
            return file_url(&packaged);
        }
    }

    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dist/sidebar")
        .join(entry_file_name);
    if local.exists() {
        return file_url(&local);
    }

    anyhow::bail!("GPUI CEF workarea bundle entry was not found")
}

/// Where a command tab dropped into the Agents workspace should land.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandToAgentsDropPlacement {
    PaneBody(WorkspaceDropZone),
}
