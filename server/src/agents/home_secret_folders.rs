//! The secret folders an agent Ghostex starts in the home folder is kept out of, handed to the
//! agent through its own deny or permission settings on that one launch: Claude Code's
//! `permissions.deny` rules in its `--settings` flag, and a Codex permission profile whose
//! filesystem denies them. No file is written into the home folder (`~/.claude` is the user's own
//! settings folder) or into the agent's shared config.
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1915, "deny list"): agents Ghostex starts in the home folder are kept out of secret folders: ~/.ssh, ~/.aws, ~/.gnupg, ~/.config/gh, keychains, password stores, browser profiles, ~/.netrc and the like, through each agent's own deny or permission settings at start, listed and editable in Settings (`homeSecretFolders`). What each agent can be held to: Claude Code's deny rules stop its own file tools (Read, Edit, Write, Glob, Grep) but not a shell command such as `cat`; Codex's profile denies reads and writes to every command it runs, and so replaces a bypass (`--yolo`) launch with "never ask" inside that profile (full disk and network minus the denied folders); every other agent can't be held to the list.
//! SEE-ALSO: packages/settings-catalog/src/data/defaults.rs (`homeSecretFolders`), apps/desktop/src/app/window/settings_modal/fields/home_secret_folders.rs (the Settings list), server/src/agents/launch_plan.rs and resume_plan.rs (where it is applied).

use serde_json::{json, Map, Value};

use super::quote_shell_arg;

/// The setting that lists the folders, `~/`-relative or absolute.
pub(crate) const HOME_SECRET_FOLDERS_SETTING: &str = "homeSecretFolders";

/// The Codex permission profile the list is handed over in.
const CODEX_PROFILE: &str = "ghostex-home";

/// The list from this computer's settings, else its default.
pub(crate) fn home_secret_folders() -> Vec<String> {
    let saved =
        crate::session_lifecycle::read_sidebar_settings(&crate::paths::get_gxserver_paths(None))
            .and_then(|settings| settings.get(HOME_SECRET_FOLDERS_SETTING).cloned());
    match saved {
        Some(value) => folders_in(
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        ),
        None => folders_in(
            ghostex_settings_catalog::default_value(HOME_SECRET_FOLDERS_SETTING)
                .map(|value| value.as_array())
                .unwrap_or_default()
                .iter()
                .filter_map(|folder| folder.as_str()),
        ),
    }
}

/// The usable entries: `~/`-relative or absolute, without a trailing separator, once each.
fn folders_in<'a>(entries: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut folders: Vec<String> = Vec::new();
    for folder in entries
        .map(|folder| folder.trim().trim_end_matches(['/', '\\']).to_string())
        .filter(|folder| folder.starts_with("~/") || std::path::Path::new(folder).is_absolute())
    {
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    folders
}

/// Whether `runtime_settings` mark a GBot chat: `runtimeSettings.gbot` (server/src/gbots/session.rs
/// `CHAT_KEY`) on the GBot's own computer, or `runtimeSettings.gbotHome` on a computer it was given
/// (server/src/gbots/computer/remote_chat.rs `HOME_KEY`), which knows nothing else of the GBot.
pub(crate) fn is_gbot_chat(runtime_settings: Option<&Map<String, Value>>) -> bool {
    runtime_settings.is_some_and(|runtime| {
        ["gbot", "gbotHome"]
            .iter()
            .any(|key| runtime.get(*key).is_some_and(Value::is_object))
    })
}

/*
CDXC:GBots 2026-11-09 DECISION:
User: a Codex GBot at Full is still kept out of the secret folders. Codex GBots start at Full (server/src/gbots/session.rs `default_approval`), which outside the home folder is `--yolo` with no sandbox at all, so a Codex GBot chat launched at Full gets the same `ghostex-home` profile wherever its folder is: never ask, write anywhere and network on (so `ghostex` reaches gxserver), minus the denied folders. Only Codex at Full: Codex GBots at Ask or Edits and Claude GBots keep their launch as it was. A Windows Codex has no such profile here, so there a Codex GBot at Full is not held to the list.
*/
/// `command` with the secret folders denied, when the session starts in the home folder (or is a
/// Codex GBot chat at Full, `gbot_chat`) and the agent has a way to be held to them; otherwise unchanged.
pub(crate) fn with_home_secret_folders_denied(
    family: Option<&str>,
    cwd: Option<&str>,
    gbot_chat: bool,
    command: &str,
) -> String {
    if cfg!(test) || command.trim().is_empty() {
        return command.to_string();
    }
    // Part of the Main Hub beta extension: off, launches at the home folder are as before. A GBot
    // chat is held to the list whatever Main Hub is: on a computer a GBot was given, Main Hub may be
    // off (the security review of M12, finding 2).
    let at_home = cwd.is_some_and(crate::chief::is_home_folder) && crate::chief::chief_shown();
    let gbot_at_full = gbot_chat && family == Some("codex") && codex_bypass(command);
    if !at_home && !gbot_at_full {
        return command.to_string();
    }
    let folders = home_secret_folders();
    if folders.is_empty() {
        return command.to_string();
    }
    match family {
        Some("claude") => with_claude_denies(command, &folders).unwrap_or_else(|| {
            eprintln!(
                "Ghostex could not hand Claude its home-folder deny list: the command already names its own --settings."
            );
            command.to_string()
        }),
        // A Windows Codex sandboxes differently; the profile below is the macOS and Linux one.
        Some("codex") if !cfg!(windows) => with_codex_denies(command, &folders),
        _ => command.to_string(),
    }
}

/// Claude's rules for `folders`: each path itself and everything under it, for reading and
/// editing (`Read` covers Glob and Grep, `Edit` covers Write).
fn claude_rules(folders: &[String]) -> Vec<Value> {
    folders
        .iter()
        .flat_map(|folder| {
            // Claude writes an absolute path as `//abs`; `~/` is the home folder.
            let path = if folder.starts_with("~/") {
                folder.clone()
            } else {
                format!("/{}", folder.replace('\\', "/"))
            };
            ["Read", "Edit"].into_iter().flat_map(move |tool| {
                [
                    json!(format!("{tool}({path})")),
                    json!(format!("{tool}({path}/**)")),
                ]
            })
        })
        .collect()
}

/// Adds the deny rules to the command's one `--settings` JSON (Claude keeps only the last
/// `--settings`, and the Connectors and Bypass steps put theirs there), or appends one. `None` when
/// the command names a `--settings` that isn't inline JSON (a file of the user's).
fn with_claude_denies(command: &str, folders: &[String]) -> Option<String> {
    with_claude_deny_rules(command, claude_rules(folders))
}

/// Adds Claude deny `rules` to the command's one `--settings` JSON, or appends one (as
/// [`with_claude_denies`] does for the folders; gbot_computer_use.rs adds its tools the same way).
pub(super) fn with_claude_deny_rules(command: &str, rules: Vec<Value>) -> Option<String> {
    with_claude_settings(command, |settings| {
        let permissions = settings
            .entry("permissions")
            .or_insert_with(|| json!({}))
            .as_object_mut()?;
        let deny = permissions
            .entry("deny")
            .or_insert_with(|| json!([]))
            .as_array_mut()?;
        for rule in rules {
            if !deny.contains(&rule) {
                deny.push(rule);
            }
        }
        Some(())
    })
}

/// Lets `edit` change the command's one `--settings` JSON (Claude keeps only the last
/// `--settings`), or an empty one appended to the command. `None` when the command names a
/// `--settings` that isn't inline JSON (a file of the user's), or `edit` gives up.
pub(super) fn with_claude_settings(
    command: &str,
    edit: impl FnOnce(&mut Map<String, Value>) -> Option<()>,
) -> Option<String> {
    let command = command.trim_end();
    let Some(start) = command.rfind("--settings ") else {
        let mut settings = Map::new();
        edit(&mut settings)?;
        return Some(format!(
            "{command} --settings {}",
            quote_shell_arg(&Value::Object(settings).to_string())
        ));
    };
    let value_start = start + "--settings ".len();
    let (text, end) = read_quoted_word(command, value_start)?;
    let mut settings: Map<String, Value> = serde_json::from_str(&text).ok()?;
    edit(&mut settings)?;
    let settings = Value::Object(settings).to_string();
    Some(format!(
        "{}{}{}",
        &command[..value_start],
        quote_shell_arg(&settings),
        &command[end..]
    ))
}

/// The single-quoted shell word that starts at `start` (as `quote_shell_arg` writes it) and the
/// index just past it.
fn read_quoted_word(command: &str, start: usize) -> Option<(String, usize)> {
    let rest = command.get(start..)?;
    // PowerShell: a single-quoted literal, or the expression a value holding `"` is written as.
    #[cfg(windows)]
    return crate::platform::shell::read_powershell_native_arg(rest)
        .filter(|(_, len)| rest[*len..].chars().next().is_none_or(char::is_whitespace))
        .map(|(text, len)| (text, start + len));
    #[cfg(not(windows))]
    read_posix_quoted_word(rest, start)
}

#[cfg_attr(windows, allow(dead_code))]
fn read_posix_quoted_word(rest: &str, start: usize) -> Option<(String, usize)> {
    let mut chars = rest.char_indices().peekable();
    let mut text = String::new();
    let mut quoted = false;
    let mut end = rest.len();
    while let Some((index, character)) = chars.next() {
        match (quoted, character) {
            (false, '\'') => quoted = true,
            (true, '\'') => quoted = false,
            // POSIX `'\''`: close, an escaped quote, reopen.
            (false, '\\') => {
                let (_, escaped) = chars.next()?;
                text.push(escaped);
            }
            (false, character) if character.is_whitespace() => {
                end = index;
                break;
            }
            (false, _) => return None,
            (true, character) => text.push(character),
        }
    }
    (!quoted).then_some((text, start + end))
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Codex in a permission profile that denies `folders`. A bypass launch (`--yolo`) would switch
/// the sandbox off and with it the denies, so it becomes "never ask" in a profile that may write
/// anywhere and reach the network, minus the denied folders; any other launch keeps Codex's
/// workspace profile with the denies added.
fn with_codex_denies(command: &str, folders: &[String]) -> String {
    let bypass = codex_bypass(command);
    let denies: Vec<String> = folders
        .iter()
        .map(|folder| format!("{}=\"deny\"", toml_string(folder)))
        .collect();
    let profile = if bypass {
        format!(
            "{{filesystem={{\"/\"=\"write\", {}}}, network={{enabled=true}}}}",
            denies.join(", ")
        )
    } else {
        format!(
            "{{extends=\":workspace\", filesystem={{{}}}}}",
            denies.join(", ")
        )
    };
    let base = if bypass {
        let mut base = command.trim_end().to_string();
        for flag in ["--dangerously-bypass-approvals-and-sandbox", "--yolo"] {
            base = without_flag(&base, flag);
        }
        format!("{base} --ask-for-approval never")
    } else {
        command.trim_end().to_string()
    };
    format!(
        "{base} -c {} -c {}",
        quote_shell_arg(&format!("permissions.{CODEX_PROFILE}={profile}")),
        quote_shell_arg(&format!("default_permissions=\"{CODEX_PROFILE}\""))
    )
}

/// Whether a Codex `command` runs without its sandbox (`--yolo`).
fn codex_bypass(command: &str) -> bool {
    command.split_whitespace().any(|token| {
        matches!(
            token.trim_matches(['"', '\'']),
            "--yolo" | "--dangerously-bypass-approvals-and-sandbox"
        )
    })
}

/// `command` without the whole word `flag`, leaving every other word and its quoting as it was.
fn without_flag(command: &str, flag: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut rest = command;
    while let Some(index) = rest.find(flag) {
        let before_ok = index == 0 || rest[..index].ends_with(char::is_whitespace);
        let after = &rest[index + flag.len()..];
        let after_ok = after.is_empty() || after.starts_with(char::is_whitespace);
        if before_ok && after_ok {
            out.push_str(rest[..index].trim_end());
            rest = after;
        } else {
            out.push_str(&rest[..index + flag.len()]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Whether `runtime_settings` mark a GBot API chat (`runtimeSettings.gbot.openedBy` is
/// `api:<keyId>`, server/src/gbots/api/chat.rs).
pub(crate) fn is_gbot_api_chat(runtime_settings: Option<&Map<String, Value>>) -> bool {
    runtime_settings
        .and_then(|runtime| runtime.get("gbot"))
        .and_then(|gbot| gbot.get("openedBy"))
        .and_then(Value::as_str)
        .is_some_and(|by| by.starts_with("api:"))
}

/// gxserver's own files an API chat is kept out of on top of the secret folders: its auth folder
/// (the token file and `security.json`), its database and logs, every GBot's folder (memory,
/// instructions), the memory journal, shared preferences and presets.
fn gxserver_private_paths() -> Vec<String> {
    let paths = crate::paths::get_gxserver_paths(None);
    let gbots = crate::gbots::gbots_folder(&paths);
    let mut out = vec![
        paths.auth_dir.clone(),
        paths.state_db_file.clone(),
        paths.logs_dir.clone(),
        gbots.join("bots"),
        gbots.join("journal"),
        gbots.join("PREFERENCES.md"),
        gbots.join("presets.json"),
        gbots.join("rooms"),
    ];
    let db = paths.state_db_file.to_string_lossy().to_string();
    out.push(format!("{db}-wal").into());
    out.push(format!("{db}-shm").into());
    out.into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

/*
CDXC:GBots 2026-11-09 WHY:
Security review of the GBot API (finding 1): "Ask" alone kept an API chat from editing without asking but not from reading. Codex's sandbox below Full reads the whole disk and runs plain reads (`cat`, `ls`) without asking, and the secret-folder list applied only at the home folder or to Codex at Full, so a message relayed from outside could have the reply carry ~/.ssh or the GBot's memory back out. So every API chat starts held to the list whatever its folder and whether Main Hub is on, with gxserver's own files added: Codex in a profile that reads everywhere but the denied paths, writes only its own folder, reaches the network (so `ghostex` reaches gxserver) and never asks, so text from outside can't raise it; Claude with its deny rules (its file tools; a shell `cat` still asks first, and only the user answers an API chat). Other agents can't be held to the list, so the Bot API is for Claude and Codex GBots only (gbots/api/chat.rs).
*/
/// `command` for a GBot API chat (`cwd` its own folder): held to the secret folders and gxserver's
/// own files. `None` when this agent can't be held to them.
pub(crate) fn with_api_chat_sandbox(
    family: Option<&str>,
    cwd: Option<&str>,
    command: &str,
) -> Option<String> {
    if cfg!(test) {
        return Some(command.to_string());
    }
    let mut folders = home_secret_folders();
    for path in gxserver_private_paths() {
        if !folders.contains(&path) {
            folders.push(path);
        }
    }
    match family {
        Some("claude") => with_claude_denies(command, &folders),
        Some("codex") if !cfg!(windows) => {
            let mut base = command.trim_end().to_string();
            for flag in [
                "--dangerously-bypass-approvals-and-sandbox",
                "--yolo",
                "--full-auto",
            ] {
                base = without_flag(&base, flag);
            }
            let mut filesystem = vec!["\"/\"=\"read\"".to_string()];
            if let Some(cwd) = cwd.filter(|cwd| !cwd.trim().is_empty()) {
                filesystem.push(format!("{}=\"write\"", toml_string(cwd)));
            }
            filesystem.extend(
                folders
                    .iter()
                    .map(|folder| format!("{}=\"deny\"", toml_string(folder))),
            );
            let profile = format!(
                "{{filesystem={{{}}}, network={{enabled=true}}}}",
                filesystem.join(", ")
            );
            Some(format!(
                "{base} --ask-for-approval never -c {} -c {}",
                quote_shell_arg(&format!("permissions.{CODEX_API_PROFILE}={profile}")),
                quote_shell_arg(&format!("default_permissions=\"{CODEX_API_PROFILE}\""))
            ))
        }
        _ => None,
    }
}

/// The Codex permission profile a GBot API chat runs in.
const CODEX_API_PROFILE: &str = "ghostex-gbot-api";
