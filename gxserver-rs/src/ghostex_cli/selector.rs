use std::path::PathBuf;

use serde_json::Value;

use crate::ghostex_cli::args::{parse_json_value, Flags};
use crate::ghostex_cli::rpc::{ghostex_home, project_id_from_global_ref, CliError, CliResult};
use crate::ghostex_cli::sessions;
use crate::ghostex_cli::sessions::{js_coalesce, js_string, js_template, js_truthy};

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI's session selector surface: the global alias
cache written after every printed live list, and the alias / id / globalRef /
provider-session-name / title / project:title selector resolution shared by
attach, focus, and every cross-session action. Error strings match the Node
CLI byte-for-byte.
*/

fn cli_dir() -> PathBuf {
    ghostex_home().join("cli")
}

fn session_alias_cache_path() -> PathBuf {
    cli_dir().join("session-aliases.json")
}

/// writeSessionAliasCache: the human sessions CLI uses global aliases from the
/// last printed live list so follow-up commands such as `ghostex a 2` and
/// `ghostex k 4` target the rows the user just saw.
pub fn write_session_alias_cache(cache: &Value) -> CliResult<()> {
    std::fs::create_dir_all(cli_dir())?;
    let text =
        serde_json::to_string_pretty(cache).map_err(|error| CliError::Other(error.to_string()))?;
    std::fs::write(session_alias_cache_path(), text)?;
    Ok(())
}

/// readSessionAliasCache: missing/unreadable/invalid cache files are simply
/// absent (the Node CLI swallows those errors).
pub fn read_session_alias_cache() -> Option<Value> {
    let text = std::fs::read_to_string(session_alias_cache_path()).ok()?;
    if text.is_empty() {
        return None;
    }
    parse_json_value(&text)
}

/// sessionSelectorFromArgs: None models the Node CLI's empty-string selector.
pub fn session_selector_from_args(rest: &[String], flags: &Flags) -> Option<String> {
    let selector = flags
        .text("sessionId")
        .or_else(|| flags.text("selector"))
        .or_else(|| flags.text("session"))
        .or_else(|| flags.text("sessionTitle"))
        .or_else(|| flags.text("target"))
        .or_else(|| rest.first().cloned())
        .unwrap_or_default()
        .trim()
        .to_string();
    if selector.is_empty() {
        None
    } else {
        Some(selector)
    }
}

pub fn resolve_cli_session_selector(selector: &str, flags: &Flags) -> CliResult<Value> {
    /*
     * CDXC:CliSessionSelectors 2026-05-23-13:18:
     * Cross-session CLI actions need the same id/title/project:title selector
     * behavior as attach/focus so agents can address another visible sidebar
     * thread without knowing its raw runtime id.
     */
    let session_list = sessions::fetch_session_list(flags, false)?;
    resolve_one_listed_session(selector, &session_list, flags)
}

/// resolveOneListedSession: exactly one match or an error listing candidates.
pub fn resolve_one_listed_session(
    selector: &str,
    sessions: &[Value],
    flags: &Flags,
) -> CliResult<Value> {
    let matches = resolve_listed_sessions(selector, sessions, flags)?;
    if matches.len() == 1 {
        return Ok(matches[0].clone());
    }
    if matches.is_empty() {
        return Err(CliError::Other(format!(
            "No matching session found for \"{selector}\". Run \"ghostex sessions\" or \"gx sessions\" to list sessions."
        )));
    }
    Err(CliError::Other(format!(
        "Multiple sessions matched \"{selector}\":\n{}",
        format_session_matches(&matches)
    )))
}

fn resolve_listed_sessions<'a>(
    selector: &str,
    sessions: &'a [Value],
    flags: &Flags,
) -> CliResult<Vec<&'a Value>> {
    resolve_listed_sessions_with_cache(selector, sessions, flags, &read_session_alias_cache)
}

fn resolve_listed_sessions_with_cache<'a>(
    selector: &str,
    sessions: &'a [Value],
    flags: &Flags,
    cache_loader: &dyn Fn() -> Option<Value>,
) -> CliResult<Vec<&'a Value>> {
    let normalized_selector = selector.trim();
    if normalized_selector.is_empty() {
        return Err(CliError::Other(
            "Provide a session alias, id, provider session name, title, or project:title selector."
                .to_string(),
        ));
    }
    /*
     * CDXC:CliSessionSelectors 2026-06-04-03:20:
     * Bare G session ids can repeat across projects. Honor --project-id when a
     * caller has inventory context, and keep unscoped duplicates ambiguous so
     * the CLI does not silently attach to a different zmx session than the
     * user selected.
     */
    let scoped_sessions = project_scoped_sessions(sessions, flags);
    if normalized_selector
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        let alias: f64 = normalized_selector.parse().unwrap_or(f64::NAN);
        let cache = cache_loader();
        let cached_session_id: Option<Value> = cache
            .as_ref()
            .and_then(|cache| cache.get("sessions"))
            .and_then(Value::as_array)
            .and_then(|list| {
                list.iter()
                    .find(|session| session.get("alias").and_then(Value::as_f64) == Some(alias))
            })
            .and_then(|session| session.get("sessionId"))
            .cloned();
        if js_truthy(cached_session_id.as_ref()) {
            if let Some(live_session) = scoped_sessions
                .iter()
                .find(|session| session.get("sessionId") == cached_session_id.as_ref())
            {
                return Ok(vec![*live_session]);
            }
        }
        let live_alias_match = scoped_sessions
            .iter()
            .find(|session| session.get("alias").and_then(Value::as_f64) == Some(alias));
        return Ok(live_alias_match
            .map(|session| vec![*session])
            .unwrap_or_default());
    }
    let exact_id_matches: Vec<&Value> = scoped_sessions
        .iter()
        .filter(|session| {
            session.get("sessionId").and_then(Value::as_str) == Some(normalized_selector)
        })
        .copied()
        .collect();
    if !exact_id_matches.is_empty() {
        return Ok(exact_id_matches);
    }
    if let Some(exact_global_ref) = scoped_sessions.iter().find(|session| {
        session.get("globalRef").and_then(Value::as_str) == Some(normalized_selector)
    }) {
        return Ok(vec![*exact_global_ref]);
    }
    /*
     * CDXC:CliSessionSelectors 2026-05-28-10:55:
     * Terminals export GHOSTEX_SESSION_ID as the provider persistence name
     * (for example `g-0527-090339`). Resolve that id before falling back to
     * title matching so generate-title and agent orchestration can target the
     * current pane reliably.
     */
    let provider_matches = rank_provider_session_matches(&scoped_sessions, normalized_selector);
    if !provider_matches.is_empty() {
        return Ok(provider_matches);
    }
    if let Some(project_separator_index) = normalized_selector.find(':') {
        if project_separator_index > 0 {
            let project_selector = normalized_selector[..project_separator_index]
                .trim()
                .to_lowercase();
            let title_selector = normalized_selector[project_separator_index + 1..]
                .trim()
                .to_lowercase();
            let project_filtered: Vec<&Value> = scoped_sessions
                .iter()
                .filter(|session| {
                    session
                        .get("projectName")
                        .and_then(Value::as_str)
                        .map(|name| name.to_lowercase() == project_selector)
                        .unwrap_or(false)
                        || session
                            .get("projectPath")
                            .and_then(Value::as_str)
                            .map(|path| path.to_lowercase().contains(&project_selector))
                            .unwrap_or(false)
                })
                .copied()
                .collect();
            return Ok(rank_session_title_matches(
                &project_filtered,
                &title_selector,
            ));
        }
    }
    Ok(rank_session_title_matches(
        &scoped_sessions,
        &normalized_selector.to_lowercase(),
    ))
}

fn project_scoped_sessions<'a>(sessions: &'a [Value], flags: &Flags) -> Vec<&'a Value> {
    let project_id = flags
        .text("projectId")
        .unwrap_or_default()
        .trim()
        .to_string();
    if project_id.is_empty() {
        return sessions.iter().collect();
    }
    sessions
        .iter()
        .filter(|session| session_project_id(session) == project_id)
        .collect()
}

fn session_project_id(session: &Value) -> String {
    let global_ref_project = session
        .get("globalRef")
        .and_then(Value::as_str)
        .and_then(project_id_from_global_ref)
        .map(Value::String);
    js_string(js_coalesce(&[
        session.get("projectId"),
        global_ref_project.as_ref(),
    ]))
    .trim()
    .to_string()
}

fn rank_provider_session_matches<'a>(sessions: &[&'a Value], selector: &str) -> Vec<&'a Value> {
    let normalized_selector = selector.trim();
    if normalized_selector.is_empty() {
        return Vec::new();
    }

    if let Some(slash_index) = normalized_selector.find('/') {
        if slash_index > 0 {
            let provider = normalized_selector[..slash_index].trim().to_lowercase();
            let provider_session_name = normalized_selector[slash_index + 1..].trim();
            if provider.is_empty() || provider_session_name.is_empty() {
                return Vec::new();
            }
            return sessions
                .iter()
                .filter(|session| {
                    session
                        .get("provider")
                        .and_then(Value::as_str)
                        .map(|value| value.to_lowercase() == provider)
                        .unwrap_or(false)
                        && session.get("providerSessionName").and_then(Value::as_str)
                            == Some(provider_session_name)
                })
                .copied()
                .collect();
        }
    }

    sessions
        .iter()
        .filter(|session| {
            session.get("providerSessionName").and_then(Value::as_str) == Some(normalized_selector)
        })
        .copied()
        .collect()
}

fn rank_session_title_matches<'a>(sessions: &[&'a Value], selector: &str) -> Vec<&'a Value> {
    let exact: Vec<&Value> = sessions
        .iter()
        .filter(|session| {
            session
                .get("title")
                .and_then(Value::as_str)
                .map(|title| title.to_lowercase() == selector)
                .unwrap_or(false)
                || session
                    .get("displayTitle")
                    .and_then(Value::as_str)
                    .map(|title| title.to_lowercase() == selector)
                    .unwrap_or(false)
        })
        .copied()
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    sessions
        .iter()
        .filter(|session| {
            session
                .get("title")
                .and_then(Value::as_str)
                .map(|title| title.to_lowercase().contains(selector))
                .unwrap_or(false)
                || session
                    .get("displayTitle")
                    .and_then(Value::as_str)
                    .map(|title| title.to_lowercase().contains(selector))
                    .unwrap_or(false)
        })
        .copied()
        .collect()
}

fn format_session_matches(sessions: &[&Value]) -> String {
    sessions
        .iter()
        .map(|session| {
            format!(
                "{}. {} - {}",
                js_template(session.get("alias")),
                js_template(session.get("projectName")),
                js_template(js_coalesce(&[
                    session.get("displayTitle"),
                    session.get("title"),
                ]))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// limitTextLines: keep only the trailing `lines` lines (JS `slice(-lines)`
/// after a /\r?\n/ split); non-finite or non-positive counts return the text
/// unchanged.
pub fn limit_text_lines(text: &str, lines: f64) -> String {
    if !lines.is_finite() || lines <= 0.0 {
        return text.to_string();
    }
    let parts: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let requested = lines.trunc();
    let count = if requested >= parts.len() as f64 {
        parts.len()
    } else {
        requested as usize
    };
    parts[parts.len() - count..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghostex_cli::args::parse_args;
    use serde_json::json;

    fn fixture_sessions() -> Vec<Value> {
        vec![
            json!({
                "alias": 1, "sessionId": "G1", "globalRef": "S1:P1:G1", "projectId": "P1",
                "projectName": "Alpha", "projectPath": "/dev/alpha", "provider": "zmx",
                "providerSessionName": "g-0601-000001", "title": "Fix login bug",
                "displayTitle": "● Fix login bug",
            }),
            json!({
                "alias": 2, "sessionId": "G2", "globalRef": "S1:P1:G2", "projectId": "P1",
                "projectName": "Alpha", "projectPath": "/dev/alpha", "provider": "zmx",
                "providerSessionName": "g-0601-000002", "title": "Ship release",
            }),
            json!({
                "alias": 3, "sessionId": "G3", "globalRef": "S1:P2:G3", "projectId": "P2",
                "projectName": "Beta", "projectPath": "/dev/beta", "provider": "zmx",
                "providerSessionName": "g-0601-000003", "title": "Fix login bug",
            }),
        ]
    }

    fn no_cache() -> Option<Value> {
        None
    }

    fn resolve<'a>(
        selector: &str,
        sessions: &'a [Value],
        flags: &Flags,
    ) -> CliResult<Vec<&'a Value>> {
        resolve_listed_sessions_with_cache(selector, sessions, flags, &no_cache)
    }

    #[test]
    fn selector_from_args_precedence_and_emptiness() {
        let parsed = parse_args(&[
            "--session-id".to_string(),
            "G9".to_string(),
            "positional".to_string(),
        ]);
        assert_eq!(
            session_selector_from_args(&parsed.rest, &parsed.flags),
            Some("G9".to_string())
        );
        let parsed = parse_args(&["  first  ".to_string()]);
        assert_eq!(
            session_selector_from_args(&parsed.rest, &parsed.flags),
            Some("first".to_string())
        );
        let parsed = parse_args(&[]);
        assert_eq!(
            session_selector_from_args(&parsed.rest, &parsed.flags),
            None
        );
    }

    #[test]
    fn resolves_exact_id_and_global_ref() {
        let sessions = fixture_sessions();
        let flags = Flags::default();
        let matches = resolve("G2", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G2"));
        let matches = resolve("S1:P2:G3", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G3"));
    }

    #[test]
    fn resolves_provider_session_names() {
        let sessions = fixture_sessions();
        let flags = Flags::default();
        let matches = resolve("g-0601-000002", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G2"));
        let matches = resolve("zmx/g-0601-000003", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G3"));
        assert!(resolve("tmux/g-0601-000003", &sessions, &flags)
            .expect("ok")
            .is_empty());
    }

    #[test]
    fn resolves_titles_and_project_scoped_titles() {
        let sessions = fixture_sessions();
        let flags = Flags::default();
        // exact-title match across projects stays ambiguous
        let matches = resolve("Fix login bug", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 2);
        // project:title narrows it
        let matches = resolve("beta:fix login", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G3"));
        // substring match
        let matches = resolve("ship", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G2"));
        // displayTitle also matches
        let matches = resolve("● Fix login bug", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G1"));
    }

    #[test]
    fn project_id_flag_scopes_sessions() {
        let sessions = fixture_sessions();
        let parsed = parse_args(&["--project-id".to_string(), "P2".to_string()]);
        let matches = resolve("Fix login bug", &sessions, &parsed.flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G3"));
    }

    #[test]
    fn alias_selector_prefers_cache_then_live_alias() {
        let sessions = fixture_sessions();
        let flags = Flags::default();
        let cache = json!({
            "sessions": [ { "alias": 2, "sessionId": "G3" } ],
        });
        let loader = move || Some(cache.clone());
        let matches =
            resolve_listed_sessions_with_cache("2", &sessions, &flags, &loader).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G3"));
        // without a cache the live alias wins
        let matches = resolve("2", &sessions, &flags).expect("ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["sessionId"], json!("G2"));
        // unknown alias resolves to nothing
        assert!(resolve("9", &sessions, &flags).expect("ok").is_empty());
    }

    #[test]
    fn resolve_one_error_messages_match_node() {
        let sessions = fixture_sessions();
        let flags = Flags::default();
        let error = resolve_one_listed_session("nope-xyz", &sessions, &flags).expect_err("err");
        assert_eq!(
            error.to_string(),
            "No matching session found for \"nope-xyz\". Run \"ghostex sessions\" or \"gx sessions\" to list sessions."
        );
        let error =
            resolve_one_listed_session("Fix login bug", &sessions, &flags).expect_err("err");
        assert_eq!(
            error.to_string(),
            "Multiple sessions matched \"Fix login bug\":\n1. Alpha - ● Fix login bug\n3. Beta - Fix login bug"
        );
        let error = resolve_one_listed_session("   ", &sessions, &flags).expect_err("err");
        assert_eq!(
            error.to_string(),
            "Provide a session alias, id, provider session name, title, or project:title selector."
        );
    }

    #[test]
    fn limit_text_lines_matches_node_slice() {
        assert_eq!(limit_text_lines("a\nb\nc", 2.0), "b\nc");
        assert_eq!(limit_text_lines("a\r\nb\r\nc", 2.0), "b\nc");
        assert_eq!(limit_text_lines("a\nb\nc", 10.0), "a\nb\nc");
        assert_eq!(limit_text_lines("a\nb\nc", 0.0), "a\nb\nc");
        assert_eq!(limit_text_lines("a\nb\nc", -1.0), "a\nb\nc");
        assert_eq!(limit_text_lines("a\nb\nc", f64::NAN), "a\nb\nc");
        assert_eq!(limit_text_lines("a\nb\nc", 2.9), "b\nc");
    }
}
