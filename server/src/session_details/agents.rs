//! The panel's read for a session whose agent is not Claude: the sections that agent can fill.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q555-Q557): every chat agent gets Environment, Changes, Context, Skills and Tasks wherever the data exists; Codex and OpenCode come first and get every section their own records can fill; a section with no data for an agent is left out, never shown empty. Environment and Changes are read for every agent before this (`http.rs`), Context and Tasks come from what the chat already reads (its context meter and task list), and this adds Skills for every agent the `$` picker knows, then Codex's and OpenCode's own sections.
//! SEE-ALSO: packages/gx-chat-core/src/extras/session_details/ (which sections each agent shows), server/src/session_details/codex.rs, server/src/session_details/opencode.rs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::server::AppState;

use super::async_cache::SectionLoad;

/// How long a resolved probe binary path is reused before re-running PATH / `mise which`.
const PROGRAM_CACHE_TTL: Duration = Duration::from_secs(60);

fn program_cache() -> &'static Mutex<HashMap<(String, PathBuf), (Instant, Option<Vec<String>>)>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, PathBuf), (Instant, Option<Vec<String>>)>>> =
        OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// Whether `session` is known not to have a running process (sleeping or closed).
pub(super) fn session_is_inactive(session: &Value) -> bool {
    matches!(
        session.get("status").and_then(Value::as_str),
        Some("sleep" | "sleeping" | "closed" | "stopped")
    )
}

/// The argv and environment of the session's live agent process, when it runs.
pub(super) fn live_launch(
    session: &Value,
    home: &Path,
) -> Option<(Vec<String>, Vec<(String, String)>)> {
    if session_is_inactive(session) {
        return None;
    }
    let name = crate::zmx::provider_zmx_session_name(session).ok()?;
    let process_id =
        crate::zmx::read_zmx_session_process_identities(std::slice::from_ref(&name), home)
            .ok()?
            .get(&name)?
            .process_id?;
    crate::accounts::process_launch(process_id).map(|live| (live.arguments, live.environment))
}

/// The program a copy of the session's agent runs: the live process's own (with its script for a
/// `node cli.js` install) while every part of it still exists on disk, else `name` found the way a
/// session start finds it (cached for [`PROGRAM_CACHE_TTL`] while the binary stays on disk).
///
/// CDXC:SessionDetails 2026-11-11 FIX:
/// User: "wtf is this? pls fix". The coordinator's Context card said "Couldn't count: Claude Code couldn't start: No such file or directory (os error 2)". Its Claude had been started days earlier from mise's `installs/claude/2.1.319/claude`; mise then installed 2.1.321 and deleted the 2.1.319 folder. The running Claude kept working (its binary stays open), but the copy that counts the context reused that argv[0] verbatim, so every probe asked macOS for a file that no longer existed. The gxserver restart at 05:17 was not the cause: any gxserver reading that process gets the same dead path. A live program missing on disk is now found again by name with `resolve_probe_command` (the login shell's PATH, gxserver's PATH, mise and asdf shims, `~/.local/bin`, Homebrew), then the account helpers' folders; a mise shim runs whichever version mise now has.
/// SEE-ALSO: server/src/session_details/probe.rs (Claude), server/src/session_details/codex.rs (Codex).
pub(super) fn agent_program(
    live_arguments: Option<&[String]>,
    name: &str,
    home: &Path,
) -> Option<Vec<String>> {
    if let Some(first) = live_arguments.and_then(<[String]>::first) {
        let stem = Path::new(first)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        let mut program = vec![first.clone()];
        if matches!(stem, "node" | "bun" | "deno") {
            if let Some(script) = live_arguments
                .and_then(|arguments| arguments.get(1))
                .filter(|script| !script.starts_with('-'))
            {
                program.push(script.clone());
            }
        }
        let on_disk = |part: &String| {
            let path = Path::new(part);
            path.is_absolute() && path.is_file()
        };
        // A runtime and a script from a relative path depend on the live process's own folder.
        if program.iter().all(on_disk) {
            return Some(program);
        }
    }
    let key = (name.to_string(), home.to_path_buf());
    if let Some((_, cached)) = program_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
        .filter(|(at, _)| at.elapsed() < PROGRAM_CACHE_TTL)
    {
        let valid = match &cached {
            Some(parts) => parts.iter().all(|part| Path::new(part).is_file()),
            None => true,
        };
        if valid {
            return cached;
        }
    }
    let resolved = crate::agent_hooks::probing::resolve_probe_command(name, home)
        .map(PathBuf::from)
        .or_else(|| crate::accounts::helpers::probe_executable(home, name))
        .map(|path| vec![path.to_string_lossy().into_owned()]);
    if let Ok(mut cache) = program_cache().lock() {
        cache.retain(|_, (at, _)| at.elapsed() < PROGRAM_CACHE_TTL);
        cache.insert(key, (Instant::now(), resolved.clone()));
    }
    resolved
}

/// Why the agent's program could not be started, in words for the panel: never the bare OS error.
pub(super) fn start_error(agent: &str, error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => format!("{agent} isn't installed on this computer."),
        std::io::ErrorKind::PermissionDenied => {
            format!("{agent} couldn't start: this computer doesn't allow it to run.")
        }
        _ => format!("{agent} couldn't start. Ghostex will try again in a few minutes."),
    }
}

/// Fills `result` with what `agent` can show beyond Environment and Changes.
/// When `progressive` is true, never blocks on slow probes; any cold section starts in the
/// background and appends its card id to `pending`. When false, runs independent probes in parallel.
pub(super) fn read_agent_details(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
    folder: Option<&str>,
    agent: &str,
    progressive: bool,
    result: &mut Value,
    pending: &mut Vec<&'static str>,
) {
    let folder_path = folder.map(Path::new);
    if progressive {
        match super::picker_skills::picker_skills_progressive(&state.paths, agent, folder_path) {
            SectionLoad::Ready(Some(skills)) => result["skills"] = skills,
            SectionLoad::Ready(None) => {}
            SectionLoad::Pending => {
                if !pending.contains(&"skills") {
                    pending.push("skills");
                }
            }
        }
        match agent {
            "codex" => super::codex::read_codex_details_progressive(
                state,
                project_id,
                session_id,
                session,
                folder_path,
                result,
                pending,
            ),
            "opencode" => {
                super::opencode::read_opencode_details_progressive(session, folder, result, pending)
            }
            _ => {}
        }
        return;
    }

    // Legacy (non-progressive) callers: run skills and agent-specific probes in parallel.
    let (skills, extra) = std::thread::scope(|scope| {
        let skills_handle = scope.spawn(|| {
            super::picker_skills::picker_skills(&state.paths, agent, folder_path)
        });
        let extra_handle = scope.spawn(|| {
            let mut extra = serde_json::json!({});
            match agent {
                "codex" => super::codex::read_codex_details(
                    state,
                    project_id,
                    session_id,
                    session,
                    folder_path,
                    &mut extra,
                ),
                "opencode" => {
                    super::opencode::read_opencode_details(session, folder, &mut extra);
                }
                _ => {}
            }
            extra
        });
        (
            skills_handle.join().ok().flatten(),
            extra_handle.join().ok(),
        )
    });
    if let Some(skills) = skills {
        result["skills"] = skills;
    }
    if let Some(Value::Object(extra_map)) = extra {
        if let Some(target) = result.as_object_mut() {
            for (k, v) in extra_map {
                target.insert(k, v);
            }
        }
    }
}
