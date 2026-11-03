use std::path::{Component, Path, PathBuf};

use serde_json::{json, Map, Value};

#[cfg(test)]
use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::args::{FlagValue, Flags};
use crate::ghostex_cli::rpc::{self, CliError, CliResult};

#[cfg(test)]
use super::*;

// ---------------------------------------------------------------------------
// JS-coercion helpers
// ---------------------------------------------------------------------------

/// Insert the value when it is defined (JS non-undefined), otherwise remove
/// the key so JSON serialization matches JS `undefined` handling.
pub(super) fn set_or_remove(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    match value {
        Some(value) => {
            map.insert(key.to_string(), value);
        }
        None => {
            map.remove(key);
        }
    }
}

pub(super) fn flag_json(flags: &Flags, key: &str) -> Option<Value> {
    flags.0.get(key).map(FlagValue::as_json)
}

pub(super) fn rest_string(rest: &[String], index: usize) -> Option<Value> {
    rest.get(index).map(|value| Value::String(value.clone()))
}

pub(super) fn join_rest(rest: &[String], skip: usize) -> String {
    if rest.len() <= skip {
        return String::new();
    }
    rest[skip..].join(" ")
}

/// Number(flags.key) rendered like JSON.stringify: NaN/Infinity → null,
/// integral values without a decimal point.
pub(super) fn flag_number_value(flags: &Flags, key: &str) -> Value {
    flags
        .number(key)
        .map(js_number_to_value)
        .unwrap_or(Value::Null)
}

pub(super) fn js_number_to_value(number: f64) -> Value {
    if !number.is_finite() {
        return Value::Null;
    }
    if number.fract() == 0.0 && number.abs() < 9.007_199_254_740_992e15 {
        return json!(number as i64);
    }
    json!(number)
}

/// JS truthiness of a possibly-absent JSON value.
pub(super) fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number
            .as_f64()
            .map(|value| value != 0.0 && !value.is_nan())
            .unwrap_or(true),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// String(value) coercion for JSON values.
pub(super) fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number
            .as_f64()
            .map(|value| {
                if value.is_finite()
                    && value.fract() == 0.0
                    && value.abs() < 9.007_199_254_740_992e15
                {
                    (value as i64).to_string()
                } else {
                    value.to_string()
                }
            })
            .unwrap_or_else(|| number.to_string()),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| {
                if item.is_null() {
                    String::new()
                } else {
                    js_string(item)
                }
            })
            .collect::<Vec<String>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// String(value ?? "") — empty string for absent/null values.
pub(super) fn string_or_empty(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(value) => js_string(value),
    }
}

/// String.prototype.slice(0, limit) over UTF-16 code units.
pub(super) fn js_slice_utf16(value: &str, limit: usize) -> String {
    let units: Vec<u16> = value.encode_utf16().take(limit).collect();
    String::from_utf16_lossy(&units)
}

pub(super) fn cwd_string() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Node path.resolve: absolute-ize against cwd and lexically normalize
/// (`.`/`..` handling, duplicate separators) without touching the filesystem.
pub(super) fn node_path_resolve(input: &str) -> String {
    let path = Path::new(input);
    let joined: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(_) => path.to_path_buf(),
        }
    };
    let mut result = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(part) => result.push(part),
        }
    }
    result.to_string_lossy().into_owned()
}

/// A `--cwd` for `/api/createSession`, which refuses anything but an absolute folder or `~/…` on
/// the computer that runs the session. A relative folder is read from this shell's folder, so it is
/// resolved only when gxserver is this computer's; for another computer it is refused rather than
/// sent as a path of this one. `~` is left for gxserver to expand with its own home.
pub(crate) fn resolve_cli_cwd(
    cwd: &str,
    flags: &Flags,
    params: &Value,
    verb: &str,
) -> CliResult<String> {
    let cwd = cwd.trim();
    if cwd.starts_with('~') || Path::new(cwd).is_absolute() {
        return Ok(cwd.to_string());
    }
    let target = rpc::resolve_gxserver_server_target(flags, params)?;
    if rpc::targets_this_computer(&target) {
        return Ok(node_path_resolve(cwd));
    }
    Err(CliError::Other(format!(
        "{verb} --cwd needs an absolute path or ~/… when the session runs on another computer."
    )))
}

pub(super) fn to_base36(mut value: u128) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut output = Vec::new();
    while value > 0 {
        output.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    output.reverse();
    String::from_utf8(output).expect("base36 digits are ASCII")
}

pub(super) fn random_base36(length: usize) -> String {
    use rand::RngExt;
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::rng();
    (0..length)
        .map(|_| DIGITS[rng.random_range(0..36)] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn parsed(values: &[&str]) -> (Vec<String>, Flags) {
        let parsed = parse_args(&strings(values));
        (parsed.rest, parsed.flags)
    }

    #[test]
    fn create_session_payload_shape() {
        let (rest, flags) = parsed(&[
            "My Title",
            "run",
            "the",
            "tests",
            "--project-id",
            "P1",
            "--start",
        ]);
        let payload = parse_create_session(&rest, &flags);
        assert_eq!(
            payload,
            json!({
                "input": "run the tests",
                "projectId": "P1",
                "start": true,
                "title": "My Title",
            })
        );
    }

    #[test]
    fn create_session_omits_undefined_fields() {
        let (rest, flags) = parsed(&[]);
        let payload = parse_create_session(&rest, &flags);
        assert_eq!(payload, json!({ "input": "" }));
    }

    #[test]
    fn session_tag_valid_and_clear_values() {
        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "Bug"]);
        let payload = parse_session_tag(&rest, &flags).expect("valid tag");
        assert_eq!(
            payload,
            json!({ "isFavorite": false, "sessionId": "G1abc", "sessionTag": "bug" })
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "none"]);
        let payload = parse_session_tag(&rest, &flags).expect("clear tag");
        assert_eq!(
            payload,
            json!({ "isFavorite": false, "sessionId": "G1abc", "sessionTag": null })
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "favorite"]);
        let payload = parse_session_tag(&rest, &flags).expect("favorite tag");
        assert_eq!(payload.get("isFavorite"), Some(&Value::Bool(true)));
    }

    #[test]
    fn session_tag_error_messages_match_node_cli() {
        let list = "favorite, high-priority, low-priority, todo, research, in-progress, testing, blocked, on-hold, done, bug, feature, design";
        let (rest, flags) = parsed(&["--session-id", "G1abc"]);
        let error = parse_session_tag(&rest, &flags).expect_err("missing tag");
        assert_eq!(
            error.to_string(),
            format!("Missing session tag. Use one of: {list}, or none.")
        );

        // Anything that is not a built-in tag or a clear word is deferred to the
        // tagSession bridge branch, which resolves it against the daemon's custom
        // tag catalog (by id or case-insensitive name) and produces the error.
        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "Nonsense"]);
        let payload = parse_session_tag(&rest, &flags).expect("deferred custom tag");
        assert_eq!(
            payload,
            json!({ "customSessionTagQuery": "Nonsense", "sessionId": "G1abc" })
        );
        let (rest, flags) = parsed(&["--session-id", "G1abc", "--tag", "custom-abcd"]);
        let payload = parse_session_tag(&rest, &flags).expect("deferred custom tag id");
        assert_eq!(
            payload,
            json!({ "customSessionTagQuery": "custom-abcd", "sessionId": "G1abc" })
        );
    }

    #[test]
    fn session_boolean_positional_vs_flag_selector() {
        // Positional selector: rest[0] = session, rest[1] = value.
        let (rest, flags) = parsed(&["G1abc", "false"]);
        let payload = parse_session_boolean("sleeping", &rest, &flags);
        assert_eq!(payload, json!({ "sessionId": "G1abc", "sleeping": false }));

        // Flag selector: rest[0] is the value.
        let (rest, flags) = parsed(&["yes", "--session-id", "G1abc"]);
        let payload = parse_session_boolean("pinned", &rest, &flags);
        assert_eq!(payload, json!({ "sessionId": "G1abc", "pinned": true }));

        // Default value is "true".
        let (rest, flags) = parsed(&["--session-id", "G1abc"]);
        let payload = parse_session_boolean("sleeping", &rest, &flags);
        assert_eq!(payload, json!({ "sessionId": "G1abc", "sleeping": true }));
    }

    #[test]
    fn delayed_send_accepts_each_trigger_and_rejects_ambiguous_triggers() {
        let (rest, flags) = parsed(&["--session-id", "G1abc", "--delay-ms", "300000"]);
        assert_eq!(
            parse_delayed_send(&rest, &flags).expect("delay trigger"),
            json!({ "delayMs": 300000, "sessionId": "G1abc" })
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--when-agent-finishes"]);
        assert_eq!(
            parse_delayed_send(&rest, &flags).expect("agent trigger"),
            json!({ "sendWhenAgentStops": true, "sessionId": "G1abc" })
        );

        let (rest, flags) = parsed(&["--session-id", "G1abc", "--when-all-agents-finish"]);
        assert_eq!(
            parse_delayed_send(&rest, &flags).expect("project trigger"),
            json!({ "sendWhenAllProjectSessionsStop": true, "sessionId": "G1abc" })
        );

        let (rest, flags) = parsed(&[
            "--session-id",
            "G1abc",
            "--delay-ms",
            "300000",
            "--when-agent-finishes",
        ]);
        assert!(parse_delayed_send(&rest, &flags).is_err());
    }

    #[test]
    fn send_text_selector_positional_split() {
        let (rest, flags) = parsed(&["G1abc", "hello", "world"]);
        let payload = parse_send_text(&rest, &flags);
        assert_eq!(
            payload,
            json!({ "sessionId": "G1abc", "text": "hello world" })
        );

        let (rest, flags) = parsed(&["hello", "world", "--session-id", "G1abc"]);
        let payload = parse_send_text(&rest, &flags);
        assert_eq!(
            payload,
            json!({ "sessionId": "G1abc", "text": "hello world" })
        );
    }

    #[test]
    fn visible_count_number_coercion() {
        let (rest, flags) = parsed(&["--count", "4"]);
        assert_eq!(parse_visible_count(&rest, &flags), json!({ "count": 4 }));

        let (rest, flags) = parsed(&["7"]);
        assert_eq!(parse_visible_count(&rest, &flags), json!({ "count": 7 }));

        let (rest, flags) = parsed(&["abc"]);
        assert_eq!(parse_visible_count(&rest, &flags), json!({ "count": null }));

        let (rest, flags) = parsed(&[]);
        assert_eq!(parse_visible_count(&rest, &flags), json!({ "count": null }));
    }

    #[test]
    fn vs_code_path_positions() {
        assert_eq!(
            parse_vs_code_path_position("file.txt:12:5"),
            ("file.txt".to_string(), Some(12), Some(5))
        );
        assert_eq!(
            parse_vs_code_path_position("file.txt:12"),
            ("file.txt".to_string(), Some(12), None)
        );
        assert_eq!(
            parse_vs_code_path_position("file:0"),
            ("file:0".to_string(), None, None)
        );
        assert_eq!(
            parse_vs_code_path_position(":12"),
            (":12".to_string(), None, None)
        );
        assert_eq!(
            parse_vs_code_path_position("a:12:5:7"),
            ("a:12".to_string(), Some(5), Some(7))
        );
        assert_eq!(
            parse_vs_code_path_position(""),
            ("".to_string(), None, None)
        );
    }

    #[cfg(unix)]
    #[test]
    fn open_path_target_resolves_lexically() {
        let target = parse_open_path_target(&json!("/tmp/x/../logs/app.log:12:3"), false);
        assert_eq!(
            target,
            json!({
                "column": 3,
                "line": 12,
                "path": "/tmp/logs/app.log",
                "raw": "/tmp/x/../logs/app.log:12:3",
            })
        );
    }

    #[test]
    fn edit_paths_wait_consumed_target() {
        // `edit --wait file.txt` — parseArgs consumes file.txt as the wait value.
        let (rest, flags) = parsed(&["--wait", "file.txt"]);
        let payload = parse_edit_paths(&rest, &flags);
        assert_eq!(payload.get("mode"), Some(&json!("edit")));
        assert_eq!(payload.get("wait"), Some(&json!(true)));
        let targets = payload
            .get("targets")
            .and_then(Value::as_array)
            .expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].get("raw"), Some(&json!("file.txt")));
        let token = targets[0]
            .get("waitToken")
            .and_then(Value::as_str)
            .expect("wait token");
        assert!(token.starts_with("wait-"));

        // No wait flag: no wait tokens.
        let (rest, flags) = parsed(&["file.txt"]);
        let payload = parse_edit_paths(&rest, &flags);
        assert_eq!(payload.get("wait"), Some(&json!(false)));
        let targets = payload
            .get("targets")
            .and_then(Value::as_array)
            .expect("targets");
        assert!(targets[0].get("waitToken").is_none());
    }

    #[test]
    fn quick_terminal_double_dash_split() {
        let (rest, flags) = parsed(&["extra", "--", "npm", "run", "dev", "--title", "Dev"]);
        // "--" ends flag parsing, so everything after lands in rest untouched.
        let payload = parse_quick_terminal(&rest, &flags);
        assert_eq!(
            payload.get("command"),
            Some(&json!("extra npm run dev --title Dev"))
        );

        let (rest, flags) = parsed(&["--title", "Dev", "--", "npm", "run", "dev"]);
        let payload = parse_quick_terminal(&rest, &flags);
        assert_eq!(payload, json!({ "command": "npm run dev", "title": "Dev" }));
    }

    #[test]
    fn save_command_defaults() {
        let (rest, flags) = parsed(&["dev", "Dev Server", "npm", "run", "dev"]);
        let payload = parse_save_command(&rest, &flags);
        assert_eq!(
            payload,
            json!({
                "actionType": "terminal",
                "closeTerminalOnExit": false,
                "command": "npm run dev",
                "commandId": "dev",
                "name": "Dev Server",
                "playCompletionSound": true,
                "showOnProjectRow": false,
            })
        );
    }

    #[test]
    fn save_command_preserves_enabled_show_on_project_row() {
        // The flag is opt-in, so the enabled path needs its own coverage: a
        // default-only assertion would still pass if the flag were hardcoded.
        let (rest, flags) = parsed(&[
            "--showOnProjectRow",
            "true",
            "lazygit",
            "Lazygit",
            "lazygit",
        ]);
        let payload = parse_save_command(&rest, &flags);
        assert_eq!(payload.get("showOnProjectRow"), Some(&json!(true)));
        assert_eq!(payload.get("commandId"), Some(&json!("lazygit")));
    }

    #[test]
    fn browser_open_reuse_and_new() {
        let (rest, flags) = parsed(&["https://example.com", "--new", "--active-project"]);
        let payload = parse_browser_open(&rest, &flags);
        assert_eq!(payload.get("reuse"), Some(&json!("none")));
        assert_eq!(payload.get("url"), Some(&json!("https://example.com")));
        assert!(payload.get("projectPath").is_none());

        let (rest, flags) = parsed(&["https://example.com"]);
        let payload = parse_browser_open(&rest, &flags);
        assert_eq!(payload.get("reuse"), Some(&json!("similar")));
        assert_eq!(
            payload.get("projectPath"),
            Some(&Value::String(cwd_string()))
        );
    }

    #[test]
    fn rename_payload_joins_positional_title() {
        let (rest, flags) = parsed(&["G1abc", "My", "New", "Title"]);
        let payload = parse_rename(&rest, &flags);
        assert_eq!(
            payload,
            json!({ "sessionId": "G1abc", "title": "My New Title" })
        );
    }

    #[test]
    fn resolved_session_params_split_global_ref() {
        let payload = json!({ "sessionId": "S1abc:P2def:G3xyz" });
        let flags = Flags::default();
        let params = with_resolved_session_params(&payload, &flags);
        assert_eq!(
            params,
            json!({
                "globalRef": "S1abc:P2def:G3xyz",
                "projectId": "P2def",
                "sessionId": "G3xyz",
            })
        );
    }

    #[test]
    fn resolved_session_params_prefer_explicit_project() {
        let payload = json!({ "sessionId": "G3xyz", "projectId": "P9" });
        let params = with_resolved_session_params(&payload, &Flags::default());
        assert_eq!(params, json!({ "projectId": "P9", "sessionId": "G3xyz" }));

        let mut flags = Flags::default();
        flags.insert_text("projectId", "P7");
        let params = with_resolved_session_params(&json!({ "sessionId": "G3xyz" }), &flags);
        assert_eq!(params, json!({ "projectId": "P7", "sessionId": "G3xyz" }));
    }

    #[test]
    fn resolved_session_params_keep_explicit_nulls() {
        // JS compactObject only strips undefined; sessionTag: null must survive.
        let payload = json!({ "sessionId": "G3xyz", "projectId": "P1", "sessionTag": null });
        let params = with_resolved_session_params(&payload, &Flags::default());
        assert_eq!(
            params,
            json!({ "projectId": "P1", "sessionId": "G3xyz", "sessionTag": null })
        );
    }

    #[test]
    fn renderer_session_target_shapes() {
        let payload = json!({ "projectId": "P1", "sessionId": "G2", "title": "x" });
        assert_eq!(
            with_renderer_session_target(&payload),
            json!({
                "projectId": "P1",
                "sessionId": "G2",
                "sessionTarget": { "projectId": "P1", "sessionId": "G2" },
                "title": "x",
            })
        );

        let payload = json!({ "projectId": "P1", "sessionId": "G2", "globalRef": "S1:P1:G2" });
        assert_eq!(
            with_renderer_session_target(&payload)
                .get("sessionTarget")
                .cloned(),
            Some(json!({ "globalRef": "S1:P1:G2", "projectId": "P1", "sessionId": "G2" }))
        );

        // Missing ids: payload unchanged.
        let payload = json!({ "sessionId": "G2" });
        assert_eq!(with_renderer_session_target(&payload), payload);

        // Existing sessionTarget object: payload unchanged.
        let payload = json!({ "projectId": "P1", "sessionId": "G2", "sessionTarget": { "a": 1 } });
        assert_eq!(with_renderer_session_target(&payload), payload);
    }

    #[test]
    fn terminal_key_mapping() {
        assert_eq!(terminal_text_for_cli_key("ctrl-c"), Some("\u{0003}"));
        assert_eq!(terminal_text_for_cli_key("Escape"), Some("\u{001b}"));
        assert_eq!(terminal_text_for_cli_key("tab"), Some("\t"));
        assert_eq!(terminal_text_for_cli_key("arrow-up"), Some("\u{001b}[A"));
        assert_eq!(terminal_text_for_cli_key("ArrowDown"), Some("\u{001b}[B"));
        assert_eq!(terminal_text_for_cli_key("arrow-right"), Some("\u{001b}[C"));
        assert_eq!(terminal_text_for_cli_key("ArrowLeft"), Some("\u{001b}[D"));
        assert_eq!(terminal_text_for_cli_key("enter"), Some("\r"));
    }

    #[test]
    fn click_button_and_project_move_positionals() {
        let (rest, flags) = parsed(&["command", "dev"]);
        assert_eq!(
            parse_click_button(&rest, &flags),
            json!({ "id": "dev", "kind": "command" })
        );

        let (rest, flags) = parsed(&["P1", "up"]);
        assert_eq!(
            parse_project_move(&rest, &flags),
            json!({ "direction": "up", "projectId": "P1" })
        );
        let (rest, flags) = parsed(&["P1", "--dir", "down"]);
        assert_eq!(
            parse_project_move(&rest, &flags),
            json!({ "direction": "down", "projectId": "P1" })
        );
    }

    #[test]
    fn assert_card_and_wait_for_payloads() {
        let (rest, flags) = parsed(&[
            "--session-id",
            "G1",
            "--agent-name",
            "Claude",
            "--visible",
            "false",
        ]);
        assert_eq!(
            Value::Object(parse_assert_card(&rest, &flags)),
            json!({ "agentName": "Claude", "sessionId": "G1", "visible": false })
        );

        let (rest, flags) = parsed(&["--session-id", "G1", "--timeout-ms", "5000"]);
        assert_eq!(
            parse_wait_for(&rest, &flags),
            json!({ "sessionId": "G1", "timeoutMs": 5000 })
        );
    }

    #[test]
    fn export_transcript_passes_only_the_include_flags_it_was_given() {
        let (rest, flags) = parsed(&[
            "S1",
            "--include-commands",
            "false",
            "--include-reasoning",
            "true",
        ]);
        assert_eq!(
            parse_export_transcript(&rest, &flags),
            json!({ "sessionId": "S1", "includeCommands": false, "includeReasoning": true })
        );
        let (rest, flags) = parsed(&["S1"]);
        assert_eq!(
            parse_export_transcript(&rest, &flags),
            json!({ "sessionId": "S1" })
        );

        let (rest, flags) = parsed(&["S1", "--last-turns", "20"]);
        assert_eq!(
            parse_export_transcript(&rest, &flags),
            json!({ "sessionId": "S1", "lastTurns": 20 })
        );
        let (rest, flags) = parsed(&["S1", "--last-turns", "0"]);
        assert_eq!(
            parse_export_transcript(&rest, &flags),
            json!({ "sessionId": "S1" })
        );
    }

    #[test]
    fn session_selector_number_coercion() {
        let (rest, flags) = parsed(&["--index", "2", "--session-number", "abc"]);
        assert_eq!(
            Value::Object(parse_session_selector(&rest, &flags)),
            json!({ "index": 2, "sessionNumber": null })
        );
    }

    #[test]
    fn sidebar_project_collections_state_payload() {
        let state = r#"{"collections":{"C1":{"collectionId":"C1","title":"Group 1","color":"transparent","collapsed":false,"projectIds":["P1"]}},"order":["C1"],"nextCollectionNumber":2}"#;
        let (rest, flags) = parsed(&["--state-json", state]);
        let payload = parse_sidebar_project_collections_state(&rest, &flags).expect("valid state");
        assert_eq!(
            payload.get("state").and_then(|value| value.get("order")),
            Some(&json!(["C1"]))
        );

        // Positional JSON fallback mirrors automation-save's rest.join(" ").
        let (rest, flags) = parsed(&["{\"collections\":", "{}}"]);
        let payload =
            parse_sidebar_project_collections_state(&rest, &flags).expect("positional state");
        assert_eq!(payload, json!({ "state": { "collections": {} } }));

        // Missing state → error.
        let (rest, flags) = parsed(&[]);
        assert!(parse_sidebar_project_collections_state(&rest, &flags).is_err());

        // Invalid JSON → error.
        let (rest, flags) = parsed(&["--state-json", "{nope"]);
        assert!(parse_sidebar_project_collections_state(&rest, &flags).is_err());

        // Non-object JSON → error.
        let (rest, flags) = parsed(&["--state-json", "\"text\""]);
        assert!(parse_sidebar_project_collections_state(&rest, &flags).is_err());
    }

    #[test]
    fn project_collection_payload_supports_stable_selectors() {
        let (rest, flags) = parsed(&["--path", "/Users/example/project", "--group", "ShortPoint"]);
        assert_eq!(
            parse_project_collection(&rest, &flags),
            json!({
                "collectionTitle": "ShortPoint",
                "path": "/Users/example/project",
            })
        );

        let (rest, flags) = parsed(&["P123", "Clients"]);
        assert_eq!(
            parse_project_collection(&rest, &flags),
            json!({ "collectionTitle": "Clients", "projectId": "P123" })
        );
    }

    #[test]
    fn js_string_coercions() {
        assert_eq!(js_string(&json!(true)), "true");
        assert_eq!(js_string(&json!(2.0)), "2");
        assert_eq!(js_string(&json!(1.5)), "1.5");
        assert_eq!(js_string(&Value::Null), "null");
        assert_eq!(string_or_empty(None), "");
        assert_eq!(string_or_empty(Some(&Value::Null)), "");
    }
}
