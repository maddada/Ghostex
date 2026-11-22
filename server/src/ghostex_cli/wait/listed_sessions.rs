use serde_json::Value;

#[cfg(test)]
use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::args::Flags;
use crate::ghostex_cli::rpc::{ghostex_home, project_id_from_global_ref, CliError, CliResult};

use super::*;

pub(super) fn resolve_one_listed_session(
    selector_text: &str,
    sessions_list: &[Value],
    flags: &Flags,
) -> CliResult<Value> {
    let matches = resolve_listed_sessions(selector_text, sessions_list, flags)?;
    if matches.len() == 1 {
        return Ok(matches.into_iter().next().expect("one match"));
    }
    if matches.is_empty() {
        return Err(CliError::Other(format!(
            "No matching session found for \"{selector_text}\". Run \"ghostex sessions\" or \"gx sessions\" to list sessions."
        )));
    }
    Err(CliError::Other(format!(
        "Multiple sessions matched \"{selector_text}\":\n{}",
        format_session_matches(&matches)
    )))
}

pub(super) fn resolve_listed_sessions(
    selector_text: &str,
    sessions_list: &[Value],
    flags: &Flags,
) -> CliResult<Vec<Value>> {
    let normalized_selector = selector_text.trim();
    if normalized_selector.is_empty() {
        return Err(CliError::Other(
            "Provide a session alias, id, provider session name, title, or project:title selector."
                .to_string(),
        ));
    }
    // Bare G session ids can repeat across projects; honor --project-id.
    let scoped_sessions = project_scoped_sessions(sessions_list, flags);
    if normalized_selector.chars().all(|c| c.is_ascii_digit()) {
        let alias = normalized_selector.parse::<f64>().unwrap_or(f64::NAN);
        let cache = read_session_alias_cache();
        let cached_session_id = cache
            .as_ref()
            .and_then(|cache| cache.get("sessions"))
            .and_then(Value::as_array)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.get("alias").and_then(Value::as_f64) == Some(alias))
            })
            .and_then(|entry| entry.get("sessionId"))
            .cloned();
        if let Some(cached_session_id) = cached_session_id {
            if js_truthy(Some(&cached_session_id)) {
                if let Some(live_session) = scoped_sessions
                    .iter()
                    .find(|session| session.get("sessionId") == Some(&cached_session_id))
                {
                    return Ok(vec![(*live_session).clone()]);
                }
            }
        }
        let live_alias_match = scoped_sessions
            .iter()
            .find(|session| session.get("alias").and_then(Value::as_f64) == Some(alias));
        return Ok(live_alias_match
            .map(|session| vec![(*session).clone()])
            .unwrap_or_default());
    }
    let exact_id_matches: Vec<Value> = scoped_sessions
        .iter()
        .filter(|session| {
            session.get("sessionId").and_then(Value::as_str) == Some(normalized_selector)
        })
        .map(|session| (*session).clone())
        .collect();
    if !exact_id_matches.is_empty() {
        return Ok(exact_id_matches);
    }
    if let Some(exact_global_ref) = scoped_sessions.iter().find(|session| {
        session.get("globalRef").and_then(Value::as_str) == Some(normalized_selector)
    }) {
        return Ok(vec![(*exact_global_ref).clone()]);
    }
    // Terminals export GHOSTEX_SESSION_ID as the provider persistence name;
    // resolve that before falling back to title matching.
    let provider_matches = rank_provider_session_matches(&scoped_sessions, normalized_selector);
    if !provider_matches.is_empty() {
        return Ok(provider_matches);
    }
    if let Some(project_separator_index) = normalized_selector.find(':').filter(|index| *index > 0)
    {
        let project_selector = normalized_selector[..project_separator_index]
            .trim()
            .to_lowercase();
        let title_selector = normalized_selector[project_separator_index + 1..]
            .trim()
            .to_lowercase();
        let filtered: Vec<&Value> = scoped_sessions
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
        return Ok(rank_session_title_matches(&filtered, &title_selector));
    }
    Ok(rank_session_title_matches(
        &scoped_sessions,
        &normalized_selector.to_lowercase(),
    ))
}

fn project_scoped_sessions<'a>(sessions_list: &'a [Value], flags: &Flags) -> Vec<&'a Value> {
    let project_id = flags
        .text("projectId")
        .unwrap_or_default()
        .trim()
        .to_string();
    if project_id.is_empty() {
        return sessions_list.iter().collect();
    }
    sessions_list
        .iter()
        .filter(|session| session_project_id(session) == project_id)
        .collect()
}

fn session_project_id(session: &Value) -> String {
    // String(session.projectId ?? projectIdFromGlobalRef(session.globalRef) ?? "").trim()
    if let Some(project_id) = session.get("projectId").filter(|value| !value.is_null()) {
        return js_string(project_id).trim().to_string();
    }
    let global_ref = session
        .get("globalRef")
        .and_then(Value::as_str)
        .unwrap_or("");
    project_id_from_global_ref(global_ref)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn rank_provider_session_matches(sessions_list: &[&Value], selector_text: &str) -> Vec<Value> {
    let normalized_selector = selector_text.trim();
    if normalized_selector.is_empty() {
        return Vec::new();
    }
    if let Some(slash_index) = normalized_selector.find('/').filter(|index| *index > 0) {
        let provider = normalized_selector[..slash_index].trim().to_lowercase();
        let provider_session_name = normalized_selector[slash_index + 1..].trim();
        if provider.is_empty() || provider_session_name.is_empty() {
            return Vec::new();
        }
        return sessions_list
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
            .map(|session| (*session).clone())
            .collect();
    }
    sessions_list
        .iter()
        .filter(|session| {
            session.get("providerSessionName").and_then(Value::as_str) == Some(normalized_selector)
        })
        .map(|session| (*session).clone())
        .collect()
}

fn rank_session_title_matches(sessions_list: &[&Value], selector_lower: &str) -> Vec<Value> {
    let exact: Vec<Value> = sessions_list
        .iter()
        .filter(|session| {
            session
                .get("title")
                .and_then(Value::as_str)
                .map(|title| title.to_lowercase() == selector_lower)
                .unwrap_or(false)
                || session
                    .get("displayTitle")
                    .and_then(Value::as_str)
                    .map(|title| title.to_lowercase() == selector_lower)
                    .unwrap_or(false)
        })
        .map(|session| (*session).clone())
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    sessions_list
        .iter()
        .filter(|session| {
            session
                .get("title")
                .and_then(Value::as_str)
                .map(|title| title.to_lowercase().contains(selector_lower))
                .unwrap_or(false)
                || session
                    .get("displayTitle")
                    .and_then(Value::as_str)
                    .map(|title| title.to_lowercase().contains(selector_lower))
                    .unwrap_or(false)
        })
        .map(|session| (*session).clone())
        .collect()
}

pub(super) fn format_session_matches(sessions_list: &[Value]) -> String {
    sessions_list
        .iter()
        .map(|session| {
            let title = match session.get("displayTitle") {
                Some(value) if !value.is_null() => js_string(value),
                _ => js_display(session.get("title")),
            };
            format!(
                "{}. {} - {}",
                js_display(session.get("alias")),
                js_display(session.get("projectName")),
                title
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_session_alias_cache() -> Option<Value> {
    let path = ghostex_home().join("cli").join("session-aliases.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flags_from(args: &[&str]) -> (Flags, Vec<String>) {
        let owned: Vec<String> = args.iter().map(|value| value.to_string()).collect();
        let parsed = parse_args(&owned);
        (parsed.flags, parsed.rest)
    }

    #[test]
    fn parse_wait_for_text_defaults_and_clamps() {
        let (flags, rest) = flags_from(&["my-session", "PHASE", "1", "COMPLETE"]);
        let parsed = parse_wait_for_text(&rest, &flags);
        assert_eq!(parsed.interval_seconds, 20.0);
        assert_eq!(parsed.lines, 200.0);
        assert_eq!(parsed.timeout_seconds, 1800.0);
        assert_eq!(parsed.pattern, "PHASE 1 COMPLETE");
        assert_eq!(parsed.selector, Some("my-session".to_string()));

        let (flags, rest) = flags_from(&[
            "--interval-seconds",
            "1",
            "--lines",
            "99999",
            "--timeout-seconds",
            "2",
            "--pattern",
            "DONE$",
            "--session-id",
            "s1",
        ]);
        let parsed = parse_wait_for_text(&rest, &flags);
        assert_eq!(parsed.interval_seconds, 2.0);
        assert_eq!(parsed.lines, 2000.0);
        assert_eq!(parsed.timeout_seconds, 5.0);
        assert_eq!(parsed.pattern, "DONE$");
        // --session-id suppresses positional selector parsing entirely.
        assert_eq!(parsed.selector, None);

        // Non-numeric values fall back to defaults.
        let (flags, rest) = flags_from(&["s", "p", "--interval-seconds", "soon"]);
        assert_eq!(parse_wait_for_text(&rest, &flags).interval_seconds, 20.0);
    }

    #[test]
    fn limit_text_lines_keeps_tail_window() {
        assert_eq!(limit_text_lines("a\nb\nc\nd", Some(2.0)), "c\nd");
        assert_eq!(limit_text_lines("a\r\nb\r\nc", Some(2.0)), "b\nc");
        assert_eq!(limit_text_lines("a\nb", Some(10.0)), "a\nb");
        assert_eq!(limit_text_lines("a\nb", None), "a\nb");
        assert_eq!(limit_text_lines("a\nb", Some(0.0)), "a\nb");
        assert_eq!(limit_text_lines("a\nb", Some(-3.0)), "a\nb");
        assert_eq!(limit_text_lines("a\nb\nc", Some(2.5)), "b\nc");
    }

    #[test]
    fn wait_for_text_match_scans_from_last_line() {
        let regex = js_regex::Regex::new("^\\s*PHASE 1 (COMPLETE|BLOCKED)").unwrap();
        let text = "noise\n  PHASE 1 COMPLETE\nmore noise\n  PHASE 1 BLOCKED\ntail";
        assert_eq!(
            find_wait_for_text_match(text, &regex),
            Some("  PHASE 1 BLOCKED")
        );
        // The anchor binds to line starts, so mid-line mentions never match.
        let text = "the agent said PHASE 1 COMPLETE in its reasoning";
        assert_eq!(find_wait_for_text_match(text, &regex), None);
        assert_eq!(find_wait_for_text_match("", &regex), None);
    }

    #[test]
    fn js_regex_supports_sentinel_patterns() {
        let test = |pattern: &str, input: &str| js_regex::Regex::new(pattern).unwrap().test(input);
        assert!(test(
            "^\\s*PHASE 1 (COMPLETE|BLOCKED)",
            "   PHASE 1 COMPLETE"
        ));
        assert!(!test(
            "^\\s*PHASE 1 (COMPLETE|BLOCKED)",
            "x PHASE 1 COMPLETE"
        ));
        assert!(test("READY$", "worker READY"));
        assert!(!test("READY$", "READY to go"));
        assert!(test("PHASE(?= 2)", "PHASE 2 START"));
        assert!(!test("PHASE(?= 2)", "PHASE 1 START"));
        assert!(test("PHASE(?! 1)", "PHASE 2"));
        assert!(!test("PHASE(?! 1)", "PHASE 1"));
        assert!(test("\\bDONE\\b", "task DONE."));
        assert!(!test("\\bDONE\\b", "ABANDONED"));
        assert!(test("[0-9]+m?s", "took 250ms"));
        assert!(test("a{2,3}", "caaad"));
        assert!(!test("^a{2,3}$", "aaaa"));
        assert!(test("colou?r", "color"));
        assert!(test("colou?r", "colour"));
        assert!(test("^[^#].*done", "all done"));
        assert!(!test("^[^#].*done", "# all done"));
        assert!(test("a.c", "abc"));
        assert!(!test("a.c", "a\nc"));
        assert!(test("x|y|z{2}", "wzz"));
        assert!(test("\\d\\d:\\d\\d", "at 10:42 today"));
        assert!(test("(?:ab)+c", "ababc"));
        assert!(test("a+?b", "aaab"));
        assert!(test("^$", ""));
        assert!(!test("^$", "x"));
        // Literal braces that do not form quantifiers.
        assert!(test("\\{3}", "{3}"));
        assert!(test("a\\*b", "a*b"));
        assert!(test("a{", "a{"));
        assert!(test("{x}", "{x}"));
        assert!(test("a{,3}", "a{,3}"));
        // Lookaheads stay quantifiable (Annex B), assertions do not.
        assert!(js_regex::Regex::new("(?=a)*").is_ok());
        assert!(test("\\x1", "x1"));
        assert!(test("\\u12", "u12"));
        assert!(test("[a-]", "-"));
        assert!(test("[\\d-x]", "-"));
        assert!(test("[^]", "x"));
        assert!(!test("[]", "x"));
    }

    #[test]
    fn js_regex_rejects_invalid_patterns() {
        let error = js_regex::Regex::new("(").unwrap_err();
        assert!(error.contains("Unterminated group"), "{error}");
        let error = js_regex::Regex::new("a)").unwrap_err();
        assert!(error.contains("Unmatched ')'"), "{error}");
        let error = js_regex::Regex::new("[a").unwrap_err();
        assert!(error.contains("Unterminated character class"), "{error}");
        let error = js_regex::Regex::new("*a").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("a{3,1}").unwrap_err();
        assert!(error.contains("numbers out of order"), "{error}");
        let error = js_regex::Regex::new("a\\").unwrap_err();
        assert!(error.contains("\\ at end of pattern"), "{error}");
        let error = js_regex::Regex::new("(a)\\1").unwrap_err();
        assert!(error.contains("backreferences"), "{error}");
        let error = js_regex::Regex::new("(?<=a)b").unwrap_err();
        assert!(error.contains("lookbehind"), "{error}");
        // V8 rejects bare quantifiers and quantified assertions.
        let error = js_regex::Regex::new("{3}").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("{2,}").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("^*").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("\\b*").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("[b-a]").unwrap_err();
        assert!(
            error.contains("Range out of order in character class"),
            "{error}"
        );
        let error = js_regex::Regex::new("(?xa)").unwrap_err();
        assert!(error.contains("Invalid group"), "{error}");
    }

    #[test]
    fn js_regex_zero_width_repeats_terminate() {
        let test = |pattern: &str, input: &str| js_regex::Regex::new(pattern).unwrap().test(input);
        assert!(test("(?:)*", ""));
        assert!(test("(?:a?)*b", "b"));
        assert!(test("(a*)*b", "aab"));
        assert!(!test("(a+)+c", "aaab"));
    }

    // Differential battery: expected values generated with Node's RegExp on
    // the same (pattern, input) pairs; the private engine must agree.
    #[test]
    fn js_regex_matches_node_regexp_battery() {
        let cases: &[(&str, &str, bool)] = &[
            (
                "^\\s*PHASE \\d+ (COMPLETE|BLOCKED)$",
                "PHASE 12 BLOCKED",
                true,
            ),
            (
                "^\\s*PHASE \\d+ (COMPLETE|BLOCKED)$",
                "\tPHASE 3 COMPLETE",
                true,
            ),
            (
                "^\\s*PHASE \\d+ (COMPLETE|BLOCKED)$",
                "PHASE 3 COMPLETE.",
                false,
            ),
            ("error|warning", "no problems here", false),
            ("error|warning", "1 warning generated", true),
            ("\\$\\d+\\.\\d{2}", "cost $14.99 total", true),
            ("\\$\\d+\\.\\d{2}", "cost $14.9 total", false),
            ("(?=.*foo)(?=.*bar)", "bar then foo", true),
            ("(?=.*foo)(?=.*bar)", "only foo", false),
            ("^\\[worker-[0-9]+\\] ready$", "[worker-2] ready", true),
            ("a[^bc]d", "axd", true),
            ("a[^bc]d", "abd", false),
            ("^\\W+$", "!!  ??", true),
            ("\\S+@\\S+\\.[a-z]{2,}", "mail me at x@y.io ok", true),
            ("^(foo)?bar", "bar", true),
            ("^(foo)?bar", "foobar", true),
            ("z{0}", "anything", true),
            ("^(a|b)+$", "abab", true),
            ("^(a|b)+$", "abcab", false),
            ("done(?!!)", "done!", false),
            ("done(?!!)", "done.", true),
            ("^.{3,5}$", "abcd", true),
            ("^.{3,5}$", "ab", false),
            ("[A-Fa-f0-9]{6}", "color a1B2c3 here", true),
            ("\\bv\\d+\\.\\d+\\.\\d+\\b", "release v1.22.3 shipped", true),
            ("  +", "double  space", true),
            ("  +", "single space", false),
            ("^\\d*$", "", true),
            ("ab*?c", "abbbc", true),
            ("^[-+]?\\d+$", "-42", true),
            ("\\u0041BC", "ABC", true),
            ("\\x41BC", "ABC", true),
            ("\\t\\w", "\tx", true),
            ("\\0", "a b", false),
        ];
        for (pattern, input, expected) in cases {
            let regex = js_regex::Regex::new(pattern)
                .unwrap_or_else(|error| panic!("pattern {pattern:?} failed to compile: {error}"));
            assert_eq!(
                regex.test(input),
                *expected,
                "pattern {pattern:?} on {input:?}"
            );
        }
    }

    fn fixture_sessions() -> Vec<Value> {
        vec![
            json!({
                "alias": 1,
                "sessionId": "sess-1",
                "globalRef": "gx:p1:sess-1",
                "projectId": "p1",
                "projectName": "Ghostex",
                "projectPath": "/Users/dev/ghostex",
                "provider": "zmx",
                "providerSessionName": "g-0713-090001",
                "title": "Fix sidebar drag",
                "displayTitle": "Fix sidebar drag",
            }),
            json!({
                "alias": 2,
                "sessionId": "sess-2",
                "projectId": "p1",
                "projectName": "Ghostex",
                "projectPath": "/Users/dev/ghostex",
                "provider": "zmx",
                "providerSessionName": "g-0713-090002",
                "title": "Port CLI",
            }),
            json!({
                "alias": 3,
                "sessionId": "sess-3",
                "projectId": "p2",
                "projectName": "Zephyr",
                "projectPath": "/Users/dev/zephyr",
                "title": "Port CLI",
            }),
        ]
    }

    #[test]
    fn live_agent_session_owner_requires_matching_agent_and_live_session() {
        let supported_agents = ["claude", "codex", "pi", "opencode", "cursor", "grok"];
        let mut sessions_list = supported_agents
            .iter()
            .enumerate()
            .map(|(index, agent)| {
                json!({
                    "agent": agent,
                    "agentId": agent,
                    "agentSessionId": format!("conversation-{agent}"),
                    "alias": index + 1,
                    "isLive": true,
                    "projectName": "Ghostex",
                    "sessionId": format!("session-{agent}"),
                    "title": format!("Live {agent}"),
                })
            })
            .collect::<Vec<_>>();
        sessions_list.push(json!({
            "agent": "codex",
            "agentId": "codex",
            "agentSessionId": "conversation-stopped",
            "alias": supported_agents.len() + 1,
            "isLive": false,
            "projectName": "Ghostex",
            "sessionId": "session-stopped",
            "title": "Stopped Codex",
        }));

        for agent in supported_agents {
            let conversation_id = format!("conversation-{agent}");
            let live = resolve_live_agent_session_owner(
                &conversation_id,
                Some(&agent.to_uppercase()),
                &sessions_list,
            )
            .unwrap()
            .expect("live owner");
            assert_eq!(live["sessionId"], json!(format!("session-{agent}")));
        }
        assert!(resolve_live_agent_session_owner(
            "conversation-codex",
            Some("claude"),
            &sessions_list,
        )
        .unwrap()
        .is_none());
        assert!(resolve_live_agent_session_owner(
            "conversation-stopped",
            Some("codex"),
            &sessions_list,
        )
        .unwrap()
        .is_none());
        assert!(
            resolve_live_agent_session_owner("missing", None, &sessions_list)
                .unwrap()
                .is_none()
        );
        assert!(resolve_live_agent_session_owner("  ", None, &sessions_list).is_err());
    }

    #[test]
    fn resolve_listed_sessions_matches_by_id_ref_provider_and_title() {
        let sessions_list = fixture_sessions();
        let flags = Flags::default();
        let by_id = resolve_listed_sessions("sess-2", &sessions_list, &flags).unwrap();
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0]["alias"], json!(2));
        let by_ref = resolve_listed_sessions("gx:p1:sess-1", &sessions_list, &flags).unwrap();
        assert_eq!(by_ref[0]["alias"], json!(1));
        let by_provider_name =
            resolve_listed_sessions("g-0713-090002", &sessions_list, &flags).unwrap();
        assert_eq!(by_provider_name[0]["alias"], json!(2));
        let by_provider_pair =
            resolve_listed_sessions("zmx/g-0713-090001", &sessions_list, &flags).unwrap();
        assert_eq!(by_provider_pair[0]["alias"], json!(1));
        let by_title = resolve_listed_sessions("port cli", &sessions_list, &flags).unwrap();
        assert_eq!(by_title.len(), 2);
        let by_partial = resolve_listed_sessions("sidebar", &sessions_list, &flags).unwrap();
        assert_eq!(by_partial.len(), 1);
        assert_eq!(by_partial[0]["alias"], json!(1));
        let by_project_title =
            resolve_listed_sessions("zephyr:port cli", &sessions_list, &flags).unwrap();
        assert_eq!(by_project_title.len(), 1);
        assert_eq!(by_project_title[0]["alias"], json!(3));
        let none = resolve_listed_sessions("no-such-thing", &sessions_list, &flags).unwrap();
        assert!(none.is_empty());
        assert!(resolve_listed_sessions("   ", &sessions_list, &flags).is_err());
    }

    #[test]
    fn resolve_listed_sessions_honors_project_scope() {
        let sessions_list = fixture_sessions();
        let mut flags = Flags::default();
        flags.insert_text("projectId", "p2");
        let scoped = resolve_listed_sessions("port cli", &sessions_list, &flags).unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0]["alias"], json!(3));
    }

    #[test]
    fn resolve_one_listed_session_error_messages() {
        let sessions_list = fixture_sessions();
        let flags = Flags::default();
        let missing = resolve_one_listed_session("nope", &sessions_list, &flags).unwrap_err();
        assert_eq!(
            missing.to_string(),
            "No matching session found for \"nope\". Run \"ghostex sessions\" or \"gx sessions\" to list sessions."
        );
        let ambiguous = resolve_one_listed_session("port cli", &sessions_list, &flags).unwrap_err();
        assert_eq!(
            ambiguous.to_string(),
            "Multiple sessions matched \"port cli\":\n2. Ghostex - Port CLI\n3. Zephyr - Port CLI"
        );
    }

    #[test]
    fn format_session_matches_uses_display_title_fallbacks() {
        let sessions_list = vec![
            json!({ "alias": 5, "projectName": "P", "displayTitle": "D", "title": "T" }),
            json!({ "alias": 6, "projectName": "P", "title": "T" }),
            json!({ "alias": 7 }),
        ];
        assert_eq!(
            format_session_matches(&sessions_list),
            "5. P - D\n6. P - T\n7. undefined - undefined"
        );
    }

    #[test]
    fn json_finite_number_mirrors_json_stringify() {
        assert_eq!(json_finite_number(Some(5000.0)), json!(5000));
        assert_eq!(json_finite_number(Some(2.5)), json!(2.5));
        assert_eq!(json_finite_number(None), Value::Null);
        assert_eq!(json_finite_number(Some(f64::INFINITY)), Value::Null);
    }

    #[test]
    fn result_error_message_prefers_error_field() {
        let fallback = || "Could not focus X.".to_string();
        assert_eq!(
            result_error_message(&json!({ "error": "boom" }), fallback),
            "boom"
        );
        let fallback = || "Could not focus X.".to_string();
        assert_eq!(
            result_error_message(&json!({ "error": null }), fallback),
            "Could not focus X."
        );
        let fallback = || "Could not focus X.".to_string();
        assert_eq!(
            result_error_message(&json!({}), fallback),
            "Could not focus X."
        );
    }

    #[test]
    fn js_helpers_coerce_like_javascript() {
        assert_eq!(js_display(Some(&json!(4))), "4");
        assert_eq!(js_display(None), "undefined");
        assert_eq!(js_display(Some(&json!(null))), "null");
        assert_eq!(js_string_or_empty(Some(&json!(null))), "");
        assert_eq!(js_f64_string(1800.0), "1800");
        assert_eq!(js_f64_string(7.5), "7.5");
        assert!(js_truthy(Some(&json!("x"))));
        assert!(!js_truthy(Some(&json!(""))));
        assert!(!js_truthy(None));
    }
}
