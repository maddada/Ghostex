//! Whether an agent's Ghostex hooks belong to another Ghostex on this computer.
//!
//! CDXC:AgentHooks 2026-10-17 DECISION:
//! The user chose that "a portable copy never rewrites another Ghostex's hooks": every gxserver start repaired every Ghostex hook to its own command, so an installed Ghostex and a portable copy (ghostex_paths::portable) kept rewriting the user's Claude, Codex, Cursor, Pi and OMP configs to themselves. A portable copy now leaves a hook that names another Ghostex's gxserver alone, reports it as installed, and neither repairs, reinstalls nor removes it; that Ghostex's hook serves the portable copy's sessions too, because events and sidecars follow the session's own environment (CDXC:AgentHooks 2026-10-17 in server/src/zmx/env.rs). A hook whose gxserver no longer exists (a deleted copy) is not anyone's, and is repaired as before. An installed Ghostex is unchanged: it still repairs every Ghostex hook to itself, once, and the portable copy does not take it back.
//! SEE-ALSO: server/src/agent_hooks/install/inspect.rs, server/src/agent_hooks/api.rs (status, install, uninstall).

use std::path::{Path, PathBuf};

use super::config::{hook_format, HookDefinition, HookFormat};
use super::probing::read_file_text;

/// True only in a portable copy, when a Ghostex hook in `config_paths` names a gxserver that
/// exists outside this copy and none names this copy's own.
pub(crate) fn hooks_belong_to_another_ghostex(
    definition: &HookDefinition,
    config_paths: &[PathBuf],
) -> bool {
    let Some(copy) = ghostex_paths::portable::current() else {
        return false;
    };
    let Some(own_root) = canonical(&copy.root) else {
        return false;
    };
    let mut another = false;
    for file in files_naming_gxserver(definition, config_paths) {
        for executable in gxserver_executables_in(&read_file_text(&file)) {
            let Some(executable) = canonical(&executable) else {
                continue;
            };
            if executable.starts_with(&own_root) {
                return false;
            }
            another = true;
        }
    }
    another
}

/// The files whose text carries the hook command: the configs themselves, or OpenCode's plugin.
fn files_naming_gxserver(definition: &HookDefinition, config_paths: &[PathBuf]) -> Vec<PathBuf> {
    if hook_format(definition.agent_id) == HookFormat::Opencode {
        return config_paths
            .get(1)
            .and_then(|path| path.parent())
            .map(|directory| vec![super::install::opencode_plugin_file(directory)])
            .unwrap_or_default();
    }
    config_paths.to_vec()
}

/// Every `…gxserver.exe` path written in `text`, in the forms hook commands use: quoted or bare,
/// short or long, with JSON-doubled backslashes undone.
fn gxserver_executables_in(text: &str) -> Vec<PathBuf> {
    const NAME: &str = "gxserver.exe";
    let lower = text.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = lower[from..].find(NAME) {
        let name_start = from + offset;
        let end = name_start + NAME.len();
        from = end;
        let before = &text[..name_start];
        // A quoted path may hold spaces; it starts after the quote that closes it.
        let start = match text[end..].trim_start_matches('\\').chars().next() {
            Some(quote @ ('\'' | '"')) => before.rfind(quote),
            _ => before.rfind(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '`')),
        }
        .map_or(0, |index| index + 1);
        let path = text[start..end].replace("\\\\", "\\");
        if Path::new(&path).is_absolute() {
            found.push(PathBuf::from(path));
        }
    }
    found
}

fn canonical(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}
