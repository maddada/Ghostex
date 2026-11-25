//! Cursor Native transcript paths, isolated `CURSOR_CONFIG_DIR` setup, workspace MCP variable
//! expansion, Terminal <-> Native SQLite store sync, checkpoint context usage reader, and error
//! classifiers.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use md5::{Digest as _, Md5};
use serde_json::{json, Map, Value};

use crate::acp::host::now_ms;

/* ------------------------------------------------------ transcript paths */

/// Directory inside Ghostex's state dir where Native Cursor sessions write their live
/// Cursor-format JSONL transcripts (`<nativeSessionId>.jsonl`).
pub(crate) fn cursor_native_transcript_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("cursor-native-transcripts")
}

/// Live transcript path for a Native Cursor session, if `session_id` is safe.
pub(crate) fn cursor_native_transcript_path(session_id: &str) -> Option<PathBuf> {
    if !crate::session_chat_cursor_mirror::is_safe_cursor_session_id(session_id) {
        return None;
    }
    Some(cursor_native_transcript_dir().join(format!("{session_id}.jsonl")))
}

/// Sidecar metadata path recording the raw Terminal jsonl byte length that seeded this Native
/// transcript, so switching back to Terminal mode can append only new Terminal lines cleanly.
pub(crate) fn cursor_native_transcript_meta_path(session_id: &str) -> Option<PathBuf> {
    if !crate::session_chat_cursor_mirror::is_safe_cursor_session_id(session_id) {
        return None;
    }
    Some(cursor_native_transcript_dir().join(format!("{session_id}.meta.json")))
}

/* ----------------------------------------------------- isolated config dir */

/// Private `CURSOR_CONFIG_DIR` for one `acp-host` process.
///
/// Mirrors the user's `~/.cursor/cli-config.json` read-only with `approvalMode: "allowlist"`,
/// `permissions.allow: []` (keeping any `deny` rules), and `autoAcceptWebSearch: false` so Cursor
/// always routes shell, delete, out-of-workspace edit, web, and MCP approvals through ACP
/// `session/request_permission` even when the user's own config has `approvalMode: "unrestricted"`.
/// Symlinks `acp-sessions`, `skills`, `skills-cursor`, `commands`, `rules`, `plugins`, and
/// `mcp.json` from `~/.cursor/` so sessions, skills, and MCP servers remain shared.
pub(super) struct CursorIsolatedConfig {
    pub(super) dir: PathBuf,
}

impl CursorIsolatedConfig {
    pub(super) fn create(paths: &crate::paths::GxserverPaths) -> Result<Self, String> {
        let base = paths.root_dir.join("cursor-acp-config");
        fs::create_dir_all(&base)
            .map_err(|err| format!("Failed creating Cursor config dir: {err}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&base, fs::Permissions::from_mode(0o700));
        }
        Self::sweep_stale_dirs(&base);

        let dir = base.join(format!("pid-{}-{}", std::process::id(), now_ms()));
        fs::create_dir_all(&dir)
            .map_err(|err| format!("Failed creating Cursor session config dir: {err}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }

        let user_cursor_dir = user_cursor_config_dir();
        let acp_sessions_target = user_cursor_dir.join("acp-sessions");
        let _ = fs::create_dir_all(&acp_sessions_target);
        symlink_entry(&acp_sessions_target, &dir.join("acp-sessions"))?;

        for entry in [
            "skills",
            "skills-cursor",
            "commands",
            "rules",
            "plugins",
            "mcp.json",
            "hooks.json",
        ] {
            let target = user_cursor_dir.join(entry);
            if target.exists() {
                let _ = symlink_entry(&target, &dir.join(entry));
            }
        }

        let sanitized = build_sanitized_cli_config(&user_cursor_dir.join("cli-config.json"));
        let config_path = dir.join("cli-config.json");
        let encoded = serde_json::to_string_pretty(&sanitized)
            .map_err(|err| format!("Failed encoding Cursor cli-config.json: {err}"))?;
        write_private_file(&config_path, encoded.as_bytes())
            .map_err(|err| format!("Failed writing Cursor cli-config.json: {err}"))?;

        Ok(Self { dir })
    }

    pub(super) fn sweep_stale_dirs(base: &Path) {
        let Ok(entries) = fs::read_dir(base) else {
            return;
        };
        let now = SystemTime::now();
        let max_age = Duration::from_secs(24 * 60 * 60);
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if now.duration_since(modified).unwrap_or_default() > max_age {
                        let _ = fs::remove_dir_all(&path);
                    }
                }
            }
        }
    }

    pub(super) fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Drop for CursorIsolatedConfig {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub(super) fn user_cursor_config_dir() -> PathBuf {
    if let Ok(dir) = env::var("CURSOR_CONFIG_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        let trimmed = xdg.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("cursor");
        }
    }
    crate::resume_lookup::home_dir().join(".cursor")
}

pub(super) fn build_sanitized_cli_config(user_config_path: &Path) -> Value {
    let mut cfg = fs::read_to_string(user_config_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Map<String, Value>>(&raw).ok())
        .unwrap_or_default();

    let deny = cfg
        .get("permissions")
        .and_then(Value::as_object)
        .and_then(|p| p.get("deny"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    cfg.insert(
        "permissions".to_string(),
        json!({
            "allow": [],
            "deny": deny,
        }),
    );
    cfg.insert("approvalMode".to_string(), json!("allowlist"));
    cfg.insert("autoAcceptWebSearch".to_string(), json!(false));
    if !cfg.contains_key("version") {
        cfg.insert("version".to_string(), json!(1));
    }
    Value::Object(cfg)
}

pub(super) fn symlink_entry(target: &Path, link: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
            .map_err(|err| format!("Failed symlinking {}: {err}", link.display()))
    }
    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)
                .map_err(|err| format!("Failed symlinking dir {}: {err}", link.display()))
        } else {
            std::os::windows::fs::symlink_file(target, link)
                .map_err(|err| format!("Failed symlinking file {}: {err}", link.display()))
        }
    }
}

pub(super) fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

/* ------------------------------------------- workspace MCP server expansion */

pub(super) fn cursor_project_slug(project_root: &str) -> String {
    let mut out = String::with_capacity(project_root.len());
    let mut last_dash = true;
    for ch in project_root.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

pub(super) fn expand_cursor_mcp_placeholders(
    input: &str,
    project_root: &str,
    project_basename: &str,
    user_home: &str,
) -> String {
    let sep = std::path::MAIN_SEPARATOR.to_string();
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let token = &after[..end];
        let replacement = if let Some(env_name) = token.strip_prefix("env:") {
            Some(env::var(env_name).unwrap_or_default())
        } else {
            let (name, fallback) = match token.split_once(":-") {
                Some((n, f)) => (n, Some(f)),
                None => (token, None),
            };
            match name {
                "workspaceFolder" => Some(project_root.to_string()),
                "workspaceFolderBasename" => Some(project_basename.to_string()),
                "userHome" => Some(user_home.to_string()),
                "pathSeparator" | "/" => Some(sep.clone()),
                _ => env::var(name).ok().or_else(|| fallback.map(str::to_string)),
            }
        };
        match replacement {
            Some(val) => out.push_str(&val),
            None => {
                out.push_str("${");
                out.push_str(token);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

pub(super) fn has_cursor_workspace_var(text: &str) -> bool {
    [
        "${workspaceFolder}",
        "${workspaceFolderBasename}",
        "${userHome}",
        "${pathSeparator}",
        "${/}",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

pub(super) fn mcp_server_needs_workspace_expansion(raw: &Map<String, Value>) -> bool {
    if raw.contains_key("envFile") || raw.contains_key("enabledTools") {
        return false;
    }
    let command = raw.get("command").and_then(Value::as_str);
    let cwd = raw.get("cwd").and_then(Value::as_str);
    if command.is_some_and(has_cursor_workspace_var) || cwd.is_some_and(has_cursor_workspace_var) {
        return true;
    }
    let is_script_interp = command.is_some_and(|cmd| {
        let base = cmd
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(cmd)
            .to_ascii_lowercase();
        let stem = base.strip_suffix(".exe").unwrap_or(&base);
        matches!(stem, "node" | "bun" | "tsx" | "python" | "python3")
    });
    if is_script_interp {
        if let Some(first_arg) = raw
            .get("args")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_str)
        {
            return first_arg.starts_with("${workspaceFolder}/")
                || first_arg.starts_with("${workspaceFolder}\\")
                || first_arg.starts_with("${userHome}/")
                || first_arg.starts_with("${userHome}\\");
        }
    }
    false
}

/// Expands stdio MCP servers in `<projectRoot>/.cursor/mcp.json` and `~/.cursor/mcp.json` whose
/// `command`, `cwd`, or script `args[0]` use editor variables (`${workspaceFolder}`, `${userHome}`,
/// etc.) that Cursor's ACP server otherwise leaves literal.
pub(super) fn resolve_cursor_workspace_mcp_servers(cwd: &Path) -> Vec<Value> {
    let project_root = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cwd.to_string_lossy().into_owned());
    let project_basename = Path::new(&project_root)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let user_home = crate::resume_lookup::home_dir()
        .to_string_lossy()
        .into_owned();
    let cursor_data_dir = env::var("CURSOR_DATA_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&user_home).join(".cursor"));

    let disabled_path = cursor_data_dir
        .join("projects")
        .join(cursor_project_slug(&project_root))
        .join("mcp-disabled.json");
    let disabled: std::collections::HashSet<String> = fs::read_to_string(&disabled_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let files = [
        PathBuf::from(&project_root)
            .join(".cursor")
            .join("mcp.json"),
        PathBuf::from(&user_home).join(".cursor").join("mcp.json"),
    ];
    for file in files {
        let Ok(contents) = fs::read_to_string(&file) else {
            continue;
        };
        let Ok(parsed) = jsonc_parser::parse_to_serde_value::<Value>(
            &contents,
            &jsonc_parser::ParseOptions {
                allow_trailing_commas: true,
                ..Default::default()
            },
        ) else {
            continue;
        };
        let Some(servers) = parsed.get("mcpServers").and_then(Value::as_object) else {
            continue;
        };
        for (name, raw_val) in servers {
            if !seen.insert(name.clone()) || name.starts_with("plugin-") || disabled.contains(name)
            {
                continue;
            }
            let Some(raw_obj) = raw_val.as_object() else {
                continue;
            };
            if !mcp_server_needs_workspace_expansion(raw_obj) {
                continue;
            }
            let Some(raw_cmd) = raw_obj.get("command").and_then(Value::as_str) else {
                continue;
            };
            let cmd = expand_cursor_mcp_placeholders(
                raw_cmd,
                &project_root,
                &project_basename,
                &user_home,
            );
            let args: Vec<String> = raw_obj
                .get("args")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(|s| {
                            expand_cursor_mcp_placeholders(
                                s,
                                &project_root,
                                &project_basename,
                                &user_home,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let env_arr: Vec<Value> = raw_obj
                .get("env")
                .and_then(Value::as_object)
                .map(|map| {
                    map.iter()
                        .filter_map(|(k, v)| {
                            let val = v.as_str()?;
                            Some(json!({
                                "name": k,
                                "value": expand_cursor_mcp_placeholders(
                                    val,
                                    &project_root,
                                    &project_basename,
                                    &user_home,
                                ),
                            }))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let server_cwd = raw_obj
                .get("cwd")
                .and_then(Value::as_str)
                .map(|c| {
                    expand_cursor_mcp_placeholders(c, &project_root, &project_basename, &user_home)
                })
                .unwrap_or_else(|| project_root.clone());
            out.push(json!({
                "name": name,
                "command": cmd,
                "args": args,
                "env": env_arr,
                "cwd": server_cwd,
            }));
        }
    }
    out
}

/* -------------------------------------------------- cross-mode store sync */

pub(super) fn cursor_cwd_hash(cwd: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(cwd.as_bytes());
    hex::encode(hasher.finalize())
}

pub(super) fn store_latest_mtime(dir: &Path) -> Option<SystemTime> {
    ["store.db", "store.db-wal"]
        .iter()
        .filter_map(|name| fs::metadata(dir.join(name)).ok()?.modified().ok())
        .max()
}

pub(super) fn copy_store_files(src_dir: &Path, dst_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst_dir)?;
    for name in ["store.db", "store.db-wal", "store.db-shm"] {
        let src = src_dir.join(name);
        let dst = dst_dir.join(name);
        if src.is_file() {
            fs::copy(&src, &dst)?;
        } else if dst.exists() {
            let _ = fs::remove_file(&dst);
        }
    }
    Ok(())
}

pub(super) fn find_cli_chat_store_dir(cwd: &Path, session_id: &str) -> Option<PathBuf> {
    let chats_root = crate::resume_lookup::home_dir()
        .join(".cursor")
        .join("chats");
    let mut candidates = Vec::new();
    let raw_str = cwd.to_string_lossy().to_string();
    candidates.push(chats_root.join(cursor_cwd_hash(&raw_str)).join(session_id));
    if let Ok(canon) = fs::canonicalize(cwd) {
        let canon_str = canon.to_string_lossy().to_string();
        if canon_str != raw_str {
            candidates.push(
                chats_root
                    .join(cursor_cwd_hash(&canon_str))
                    .join(session_id),
            );
        }
    }
    for dir in candidates {
        if dir.join("store.db").is_file() {
            return Some(dir);
        }
    }
    // Fallback: scan `~/.cursor/chats/*/<session_id>/store.db` (handles moved folders).
    for entry in fs::read_dir(&chats_root).ok()?.flatten() {
        let candidate = entry.path().join(session_id);
        if candidate.join("store.db").is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Before `session/load`, copy Cursor's Terminal-mode SQLite store (`~/.cursor/chats/<md5>/<id>/`)
/// into `~/.cursor/acp-sessions/<id>/` when the Terminal store is missing from `acp-sessions/` or
/// newer, so Terminal -> Native switches resume the exact conversation seamlessly.
pub(super) fn sync_store_from_cli_to_acp(cwd: &Path, session_id: &str) {
    if !crate::session_chat_cursor_mirror::is_safe_cursor_session_id(session_id) {
        return;
    }
    let Some(cli_dir) = find_cli_chat_store_dir(cwd, session_id) else {
        return;
    };
    let acp_dir = user_cursor_config_dir()
        .join("acp-sessions")
        .join(session_id);
    let acp_db = acp_dir.join("store.db");
    let should_copy = if !acp_db.is_file() {
        true
    } else {
        match (store_latest_mtime(&cli_dir), store_latest_mtime(&acp_dir)) {
            (Some(cli_t), Some(acp_t)) => cli_t > acp_t,
            (Some(_), None) => true,
            _ => false,
        }
    };
    if !should_copy {
        return;
    }
    if copy_store_files(&cli_dir, &acp_dir).is_ok() {
        let meta_path = acp_dir.join("meta.json");
        if !meta_path.is_file() {
            let abs_cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
            let meta = json!({
                "schemaVersion": 1,
                "cwd": abs_cwd.to_string_lossy(),
            });
            let _ = fs::write(&meta_path, meta.to_string());
        }
    }
}

/// After stopping `cursor-agent acp` cleanly during Switch to Terminal, copy
/// `~/.cursor/acp-sessions/<id>/store.db` into `~/.cursor/chats/<md5(cwd)>/<id>/` so
/// `cursor-agent --resume <id>` resumes the exact Native conversation in Terminal mode.
pub(super) fn sync_store_from_acp_to_cli(cwd: &Path, session_id: &str) {
    if !crate::session_chat_cursor_mirror::is_safe_cursor_session_id(session_id) {
        return;
    }
    let acp_dir = user_cursor_config_dir()
        .join("acp-sessions")
        .join(session_id);
    if !acp_dir.join("store.db").is_file() {
        return;
    }
    let chats_root = crate::resume_lookup::home_dir()
        .join(".cursor")
        .join("chats");
    let raw_str = cwd.to_string_lossy().to_string();
    let mut targets = vec![chats_root.join(cursor_cwd_hash(&raw_str)).join(session_id)];
    if let Ok(canon) = fs::canonicalize(cwd) {
        let canon_str = canon.to_string_lossy().to_string();
        if canon_str != raw_str {
            targets.push(
                chats_root
                    .join(cursor_cwd_hash(&canon_str))
                    .join(session_id),
            );
        }
    }
    for dst in targets {
        let _ = copy_store_files(&acp_dir, &dst);
    }
}

/* ------------------------------------------- checkpoint context usage read */

pub(super) fn read_pb_varint(bytes: &[u8], idx: &mut usize) -> Option<u64> {
    let mut value: u64 = 0;
    let mut shift = 0u32;
    loop {
        let byte = *bytes.get(*idx)?;
        *idx += 1;
        if shift >= 64 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
}

pub(super) fn find_pb_field_bytes<'a>(bytes: &'a [u8], wanted_field: u32) -> Option<&'a [u8]> {
    let mut idx = 0usize;
    while idx < bytes.len() {
        let key = read_pb_varint(bytes, &mut idx)?;
        let field = u32::try_from(key >> 3).ok()?;
        let wire = key & 0x7;
        match wire {
            0 => {
                read_pb_varint(bytes, &mut idx)?;
            }
            1 => {
                idx = idx.checked_add(8).filter(|end| *end <= bytes.len())?;
            }
            2 => {
                let len = usize::try_from(read_pb_varint(bytes, &mut idx)?).ok()?;
                let end = idx.checked_add(len).filter(|end| *end <= bytes.len())?;
                if field == wanted_field {
                    return Some(&bytes[idx..end]);
                }
                idx = end;
            }
            5 => {
                idx = idx.checked_add(4).filter(|end| *end <= bytes.len())?;
            }
            _ => return None,
        }
    }
    None
}

pub(super) fn parse_pb_token_details(bytes: &[u8]) -> Option<(u64, u64)> {
    let mut idx = 0usize;
    let mut used = None;
    let mut max = None;
    while idx < bytes.len() {
        let key = read_pb_varint(bytes, &mut idx)?;
        let field = u32::try_from(key >> 3).ok()?;
        match key & 0x7 {
            0 => {
                let v = read_pb_varint(bytes, &mut idx)?;
                match field {
                    1 => used = Some(v),
                    2 => max = Some(v),
                    _ => {}
                }
            }
            1 => {
                idx = idx.checked_add(8).filter(|end| *end <= bytes.len())?;
            }
            2 => {
                let len = usize::try_from(read_pb_varint(bytes, &mut idx)?).ok()?;
                idx = idx.checked_add(len).filter(|end| *end <= bytes.len())?;
            }
            5 => {
                idx = idx.checked_add(4).filter(|end| *end <= bytes.len())?;
            }
            _ => return None,
        }
    }
    let u = used.filter(|v| *v > 0)?;
    let m = max.filter(|v| *v > 0).unwrap_or(0);
    Some((u, m))
}

/// Read `(used_tokens, max_tokens)` from Cursor's `acp-sessions/<id>/store.db` latest checkpoint
/// (`ConversationStateStructure` field 5 = `TokenDetails { 1: used_tokens, 2: max_tokens }`).
pub(super) fn read_cursor_acp_context_usage(session_id: &str) -> Option<(u64, Option<u64>)> {
    if !crate::session_chat_cursor_mirror::is_safe_cursor_session_id(session_id) {
        return None;
    }
    let db_path = user_cursor_config_dir()
        .join("acp-sessions")
        .join(session_id)
        .join("store.db");
    if !db_path.is_file() {
        return None;
    }
    let conn = ghostex_find::agent_store::open_agent_store(&db_path).ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(200));
    let mut stmt = conn.prepare("SELECT value FROM meta").ok()?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).ok()?;
    let root_id = rows.flatten().find_map(|raw| {
        let meta = crate::resume_lookup::parse_cursor_meta_value(&raw)?;
        let root = meta.get("latestRootBlobId")?.as_str()?.trim();
        (!root.is_empty()).then(|| root.to_string())
    })?;
    let root_bytes: Vec<u8> = conn
        .query_row(
            "SELECT data FROM blobs WHERE id = ?1",
            rusqlite::params![root_id],
            |row| row.get(0),
        )
        .ok()?;
    let token_bytes = find_pb_field_bytes(&root_bytes, 5)?;
    let (used, max) = parse_pb_token_details(token_bytes)?;
    Some((used, (max > 0).then_some(max)))
}

pub(super) fn read_cursor_acp_meta_title(session_id: &str) -> Option<String> {
    let meta_path = user_cursor_config_dir()
        .join("acp-sessions")
        .join(session_id)
        .join("meta.json");
    let raw = fs::read_to_string(meta_path).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/* ------------------------------------------- transport failure detection */

/// Detects standalone Cursor transport error dumps emitted inside assistant text (e.g.
/// `Error: RetriableError: ...` or `Error: ConnectError: [unavailable]...`), while ignoring
/// ordinary replies that merely quote those strings.
#[derive(Default)]
pub(super) struct CursorTransportFailure {
    disqualified: bool,
    failure: Option<String>,
    line: String,
}

impl CursorTransportFailure {
    const MAX_LINE_LEN: usize = 4096;
    const SERVER_ERROR: &'static str =
        "Something went wrong communicating with the server. Please try again.";

    pub(super) fn is_transport_error_line(text: &str) -> bool {
        if text == Self::SERVER_ERROR {
            return true;
        }
        if let Some(rest) = text.strip_prefix("Error: RetriableError: ") {
            return !rest.is_empty() && !rest.starts_with("[internal]");
        }
        if let Some(rest) = text.strip_prefix("Error: ConnectError: [") {
            return rest.starts_with("unavailable]")
                || rest.starts_with("aborted]")
                || rest.starts_with("deadline_exceeded]");
        }
        false
    }

    pub(super) fn consume_line(disqualified: &mut bool, failure: &mut Option<String>, raw: &str) {
        if *disqualified {
            return;
        }
        let text = raw.trim_end();
        if Self::is_transport_error_line(text) {
            *failure = Some(text.to_string());
        } else if !text.trim().is_empty()
            && !(failure.is_some() && text.trim_start().starts_with("at "))
        {
            *disqualified = true;
            *failure = None;
        }
    }

    pub(super) fn push(&mut self, chunk: &str) {
        for (idx, part) in chunk.split('\n').enumerate() {
            if self.disqualified {
                return;
            }
            if idx > 0 {
                let line = std::mem::take(&mut self.line);
                Self::consume_line(&mut self.disqualified, &mut self.failure, &line);
            }
            if self.line.len() + part.len() > Self::MAX_LINE_LEN {
                self.disqualified = true;
                self.failure = None;
                self.line.clear();
                return;
            }
            self.line.push_str(part);
        }
    }

    pub(super) fn failure(&self) -> Option<String> {
        let mut disq = self.disqualified;
        let mut fail = self.failure.clone();
        Self::consume_line(&mut disq, &mut fail, &self.line);
        fail
    }
}

pub(super) fn is_cursor_usage_limit_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("usage_limit")
        || lower.contains("rate_limit")
        || lower.contains("resource_exhausted")
        || lower.contains("http 429")
        || lower.contains("too many requests")
        || lower.contains("plan_required")
}

pub(super) fn is_cursor_auth_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("authentication required")
        || lower.contains("not authenticated")
        || lower.contains("run 'agent login'")
        || lower.contains("run 'cursor-agent login'")
        || lower.contains("http 401")
        || lower.contains("unauthorized")
}
