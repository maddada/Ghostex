use serde_json::{Map, Value};

use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/*
CDXC:GxserverAutomations 2026-06-29-15:55 (ported 2026-07-13):
`ghostex` and `gx` automation commands should talk to gxserver-rs automation
RPCs directly. Keep this in a separate CLI module so the main dispatcher does
not own automation parsing or route through renderer command automation
actions. Faithful port of scripts/ghostex-cli-automations.mjs.
*/

/// AUTOMATION_ENDPOINTS + registerAutomationCommands from the Node module:
/// command name → gxserver RPC pathname + payload parser.
#[derive(Clone, Copy, Debug, PartialEq)]
enum AutomationParser {
    Project,
    Save,
    Id,
    Enabled,
    Run,
}

const AUTOMATION_COMMANDS: [(&str, &str, AutomationParser); 7] = [
    (
        "automation-state",
        "/api/readAutomationState",
        AutomationParser::Project,
    ),
    (
        "automation-save",
        "/api/saveAutomation",
        AutomationParser::Save,
    ),
    (
        "automation-delete",
        "/api/deleteAutomation",
        AutomationParser::Id,
    ),
    (
        "automation-run-now",
        "/api/runAutomationNow",
        AutomationParser::Id,
    ),
    (
        "automation-set-enabled",
        "/api/setAutomationEnabled",
        AutomationParser::Enabled,
    ),
    (
        "automation-archive-run",
        "/api/archiveAutomationRun",
        AutomationParser::Run,
    ),
    (
        "automation-mark-run-read",
        "/api/markAutomationRunRead",
        AutomationParser::Run,
    ),
];

fn automation_command_entry(name: &str) -> Option<(&'static str, AutomationParser)> {
    AUTOMATION_COMMANDS
        .iter()
        .find(|(command, _, _)| *command == name)
        .map(|(_, pathname, parser)| (*pathname, *parser))
}

pub fn is_automation_command(name: &str) -> bool {
    automation_command_entry(name).is_some()
}

pub fn run_automation_command(name: &str, args: &[String]) -> CliResult<()> {
    let Some((pathname, parser)) = automation_command_entry(name) else {
        return Err(CliError::Other(format!("Unknown command: {name}")));
    };
    let parsed = parse_args(args);
    let payload = build_automation_payload(parser, &parsed.rest, &parsed.flags)?;
    let result = call_gxserver_rpc(pathname, &payload, &parsed.flags)?;
    if is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    print_json(&result);
    Ok(())
}

fn build_automation_payload(
    parser: AutomationParser,
    rest: &[String],
    flags: &Flags,
) -> CliResult<Value> {
    Ok(match parser {
        AutomationParser::Project => parse_automation_project(rest, flags),
        AutomationParser::Save => parse_automation_save(rest, flags)?,
        AutomationParser::Id => parse_automation_id(rest, flags),
        AutomationParser::Enabled => parse_automation_enabled(rest, flags),
        AutomationParser::Run => parse_automation_run(rest, flags),
    })
}

/// `flags.key` as the JSON value the JS payload would carry (boolean flags
/// stay booleans), or None when the flag is absent (JS `undefined`, dropped
/// from the serialized payload by JSON.stringify).
fn flag_json(flags: &Flags, key: &str) -> Option<Value> {
    flags.0.get(key).map(|value| value.as_json())
}

fn insert_present(payload: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        payload.insert(key.to_string(), value);
    }
}

fn parse_automation_project(rest: &[String], flags: &Flags) -> Value {
    let mut payload = Map::new();
    insert_present(&mut payload, "projectId", flag_json(flags, "projectId"));
    insert_present(
        &mut payload,
        "projectPath",
        flag_json(flags, "projectPath")
            .or_else(|| flag_json(flags, "path"))
            .or_else(|| rest.first().map(|value| Value::String(value.clone()))),
    );
    Value::Object(payload)
}

fn parse_automation_save(rest: &[String], flags: &Flags) -> CliResult<Value> {
    // definitionJson = flags.definitionJson ?? flags.payloadJson ?? rest.join(" ")
    // JS keeps `definition` undefined when the flag captured a boolean instead
    // of a string; parseJson turns blank text into undefined and throws on
    // invalid JSON.
    let definition_json = flag_json(flags, "definitionJson")
        .or_else(|| flag_json(flags, "payloadJson"))
        .unwrap_or_else(|| Value::String(rest.join(" ")));
    let mut payload = match parse_automation_project(&[], flags) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    if let Value::String(text) = definition_json {
        if let Some(definition) = parse_automation_json(&text)? {
            payload.insert("definition".to_string(), definition);
        }
    }
    Ok(Value::Object(payload))
}

fn parse_automation_id(rest: &[String], flags: &Flags) -> Value {
    let mut payload = match parse_automation_project(&[], flags) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    insert_present(
        &mut payload,
        "automationId",
        flag_json(flags, "automationId")
            .or_else(|| flag_json(flags, "id"))
            .or_else(|| rest.first().map(|value| Value::String(value.clone()))),
    );
    Value::Object(payload)
}

fn parse_automation_enabled(rest: &[String], flags: &Flags) -> Value {
    let mut payload = match parse_automation_id(rest, flags) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let enabled_value = flag_json(flags, "enabled")
        .or_else(|| flag_json(flags, "value"))
        .or_else(|| rest.get(1).map(|value| Value::String(value.clone())))
        .unwrap_or_else(|| Value::String("true".to_string()));
    payload.insert(
        "enabled".to_string(),
        Value::Bool(parse_automation_boolean(&enabled_value)),
    );
    Value::Object(payload)
}

fn parse_automation_run(rest: &[String], flags: &Flags) -> Value {
    let mut payload = match parse_automation_project(&[], flags) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let remove_worktree =
        flag_json(flags, "removeWorktree").unwrap_or_else(|| Value::String("false".to_string()));
    payload.insert(
        "removeWorktree".to_string(),
        Value::Bool(parse_automation_boolean(&remove_worktree)),
    );
    insert_present(
        &mut payload,
        "runId",
        flag_json(flags, "runId")
            .or_else(|| flag_json(flags, "id"))
            .or_else(|| rest.first().map(|value| Value::String(value.clone()))),
    );
    Value::Object(payload)
}

/// Local parseBoolean from ghostex-cli-automations.mjs:
/// String(value ?? "").trim().toLowerCase() in ["1","true","yes","y","on"].
fn parse_automation_boolean(value: &Value) -> bool {
    let text = match value {
        Value::Null => String::new(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    matches!(
        text.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on"
    )
}

/// Local parseJson: blank text → undefined; invalid JSON throws (the JS error
/// aborts the command the same way).
fn parse_automation_json(text: &str) -> CliResult<Option<Value>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(text)
        .map(Some)
        .map_err(|error| CliError::Other(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn automation_command_registry_matches_node_module() {
        for (name, pathname) in [
            ("automation-state", "/api/readAutomationState"),
            ("automation-save", "/api/saveAutomation"),
            ("automation-delete", "/api/deleteAutomation"),
            ("automation-run-now", "/api/runAutomationNow"),
            ("automation-set-enabled", "/api/setAutomationEnabled"),
            ("automation-archive-run", "/api/archiveAutomationRun"),
            ("automation-mark-run-read", "/api/markAutomationRunRead"),
        ] {
            assert!(is_automation_command(name), "{name} should be registered");
            assert_eq!(
                automation_command_entry(name).map(|(path, _)| path),
                Some(pathname)
            );
        }
        assert!(!is_automation_command("automation"));
        assert!(!is_automation_command("automation-unknown"));
        assert!(!is_automation_command("state"));
    }

    #[test]
    fn project_payload_prefers_flags_then_rest() {
        let parsed = parse_args(&args(&[
            "--project-id",
            "P1",
            "--path",
            "/tmp/x",
            "ignored",
        ]));
        let payload = parse_automation_project(&parsed.rest, &parsed.flags);
        assert_eq!(payload["projectId"], "P1");
        assert_eq!(payload["projectPath"], "/tmp/x");

        let parsed = parse_args(&args(&["/repo/path"]));
        let payload = parse_automation_project(&parsed.rest, &parsed.flags);
        assert!(payload.get("projectId").is_none());
        assert_eq!(payload["projectPath"], "/repo/path");

        let parsed = parse_args(&args(&["--project-path", "/a", "--path", "/b"]));
        let payload = parse_automation_project(&parsed.rest, &parsed.flags);
        assert_eq!(payload["projectPath"], "/a");
    }

    #[test]
    fn save_payload_parses_definition_json() {
        let parsed = parse_args(&args(&[
            "--path",
            "/repo",
            "--definition-json",
            "{\"name\":\"nightly\"}",
        ]));
        let payload = parse_automation_save(&parsed.rest, &parsed.flags).expect("payload");
        assert_eq!(payload["projectPath"], "/repo");
        assert_eq!(payload["definition"]["name"], "nightly");

        // rest.join(" ") fallback
        let parsed = parse_args(&args(&["{\"a\":", "1}"]));
        let payload = parse_automation_save(&parsed.rest, &parsed.flags).expect("payload");
        assert_eq!(payload["definition"]["a"], 1);

        // blank definition → omitted
        let parsed = parse_args(&args(&["--path", "/repo"]));
        let payload = parse_automation_save(&parsed.rest, &parsed.flags).expect("payload");
        assert!(payload.get("definition").is_none());

        // boolean-captured flag (no value) → not a string → definition omitted
        let parsed = parse_args(&args(&["--definition-json", "--path", "/repo"]));
        let payload = parse_automation_save(&parsed.rest, &parsed.flags).expect("payload");
        assert!(payload.get("definition").is_none());

        // invalid JSON → error like JSON.parse throw
        let parsed = parse_args(&args(&["--definition-json", "{nope"]));
        assert!(parse_automation_save(&parsed.rest, &parsed.flags).is_err());
    }

    #[test]
    fn id_payload_never_uses_rest_for_project_path() {
        let parsed = parse_args(&args(&["A1", "--path", "/repo"]));
        let payload = parse_automation_id(&parsed.rest, &parsed.flags);
        assert_eq!(payload["automationId"], "A1");
        assert_eq!(payload["projectPath"], "/repo");

        let parsed = parse_args(&args(&["A1"]));
        let payload = parse_automation_id(&parsed.rest, &parsed.flags);
        assert!(payload.get("projectPath").is_none());

        let parsed = parse_args(&args(&["--id", "A2"]));
        let payload = parse_automation_id(&parsed.rest, &parsed.flags);
        assert_eq!(payload["automationId"], "A2");
    }

    #[test]
    fn enabled_payload_boolean_table() {
        for (cli, expected) in [
            (vec!["A1", "true"], true),
            (vec!["A1", "1"], true),
            (vec!["A1", "YES"], true),
            (vec!["A1", "y"], true),
            (vec!["A1", "on"], true),
            (vec!["A1", "false"], false),
            (vec!["A1", "0"], false),
            (vec!["A1", "off"], false),
            (vec!["A1"], true), // default "true"
        ] {
            let parsed = parse_args(&args(&cli));
            let payload = parse_automation_enabled(&parsed.rest, &parsed.flags);
            assert_eq!(payload["enabled"], Value::Bool(expected), "cli: {cli:?}");
            assert_eq!(payload["automationId"], "A1");
        }

        let parsed = parse_args(&args(&["A1", "off", "--enabled", "true"]));
        let payload = parse_automation_enabled(&parsed.rest, &parsed.flags);
        assert_eq!(payload["enabled"], Value::Bool(true));

        // bare --enabled captures boolean true → String(true) → "true" → true
        let parsed = parse_args(&args(&["A1", "--enabled"]));
        let payload = parse_automation_enabled(&parsed.rest, &parsed.flags);
        assert_eq!(payload["enabled"], Value::Bool(true));
    }

    #[test]
    fn run_payload_defaults_remove_worktree_false() {
        let parsed = parse_args(&args(&["--run-id", "R1", "--path", "/repo"]));
        let payload = parse_automation_run(&parsed.rest, &parsed.flags);
        assert_eq!(payload["runId"], "R1");
        assert_eq!(payload["removeWorktree"], Value::Bool(false));

        let parsed = parse_args(&args(&["R2", "--remove-worktree", "true"]));
        let payload = parse_automation_run(&parsed.rest, &parsed.flags);
        assert_eq!(payload["runId"], "R2");
        assert_eq!(payload["removeWorktree"], Value::Bool(true));
    }
}
