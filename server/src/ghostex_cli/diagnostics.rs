use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Map, Value};

use crate::ghostex_cli::actions;
use crate::ghostex_cli::args::{parse_args, parse_boolean, Flags};
use crate::ghostex_cli::launchers;
use crate::ghostex_cli::output::{is_failed_cli_result, print_json, timestamp_slug};
use crate::ghostex_cli::rpc::{
    call_gxserver_rpc, compact_object, fetch_gxserver_health, ghostex_config_home, ghostex_home,
    ghostex_logs_home, resolve_gxserver_server_target, CliError, CliResult,
};

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI evidence and readiness commands: screenshot,
logs (gxserver API first, local file fallback), bundle, and android-check.
Output strings and exit-code behavior match scripts/ghostex-cli.mjs.
*/

fn log_dir() -> PathBuf {
    ghostex_logs_home()
}

fn cli_dir() -> PathBuf {
    ghostex_home().join("cli")
}

fn gxserver_log_path() -> PathBuf {
    ghostex_logs_home().join("gxserver.jsonl")
}

fn shared_settings_path() -> PathBuf {
    ghostex_config_home().join("native-sidebar-settings.json")
}

/// String(value) for the JS values these commands interpolate into messages.
fn js_display(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

/// JS truthiness for JSON values (`value || fallback` semantics).
fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().map(|n| n != 0.0).unwrap_or(true),
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

/// Number(flags.key ?? default) with JS NaN semantics preserved.
fn number_flag_or(flags: &Flags, key: &str, default: f64) -> f64 {
    if !flags.contains(key) {
        return default;
    }
    flags.number(key).unwrap_or(f64::NAN)
}

/// JS Number → JSON value (NaN/Infinity serialize as null, like JSON.stringify).
fn number_value(number: f64) -> Value {
    if number.is_finite() && number.fract() == 0.0 && number.abs() < 9_007_199_254_740_992.0 {
        return json!(number as i64);
    }
    serde_json::Number::from_f64(number)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// Array.prototype.slice(-lines) over collected lines.
fn slice_last<T>(items: Vec<T>, lines: f64) -> Vec<T> {
    let len = items.len() as f64;
    let n = if lines.is_nan() { 0.0 } else { lines.trunc() };
    let begin_arg = -n;
    let begin = if begin_arg < 0.0 {
        (len + begin_arg).max(0.0)
    } else {
        begin_arg.min(len)
    } as usize;
    items.into_iter().skip(begin).collect()
}

fn split_log_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Port of filterLogLines: keep non-empty lines, then apply --since and --grep.
pub fn filter_log_lines(text: &str, flags: &Flags) -> Vec<String> {
    let mut lines = split_log_lines(text);
    if flags.truthy("since") {
        let since = flags.text("since").unwrap_or_default();
        let bracket_prefix = format!("[{since}");
        lines.retain(|line| line.contains(&since) || line.as_str() > bracket_prefix.as_str());
    }
    if flags.truthy("grep") {
        let grep = flags.text("grep").unwrap_or_default();
        lines.retain(|line| line.contains(&grep));
    }
    lines
}

// ---------------------------------------------------------------------------
// External macOS tools (osascript / screencapture / plutil like the Node CLI)
// ---------------------------------------------------------------------------

fn exec_file(command: &str, args: &[String]) -> CliResult<(String, String)> {
    let output = Command::new(command)
        .args(args)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CliError::Other(format!("spawn {command} ENOENT")),
            _ => CliError::Other(format!("spawn {command} {error}")),
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(CliError::Other(format!(
            "Command failed: {command} {}\n{stderr}",
            args.join(" ")
        )));
    }
    Ok((stdout, stderr))
}

static RESOLVED_GHOSTEX_APP_BUNDLE_ID: std::sync::OnceLock<Option<String>> =
    std::sync::OnceLock::new();

fn resolve_ghostex_app_bundle_id() -> Option<String> {
    /*
    Activation must target the app bundle this CLI copy is installed inside
    (macOS "Ghostex" and "Ghostex GPUI" are separate bundles sharing this
    binary), so activate by bundle identifier read from the owning .app's
    Info.plist. GHOSTEX_APP_BUNDLE_ID overrides; a repo checkout outside any
    bundle keeps the historical name-based activation.
    */
    RESOLVED_GHOSTEX_APP_BUNDLE_ID
        .get_or_init(|| {
            let override_id = std::env::var("GHOSTEX_APP_BUNDLE_ID")
                .unwrap_or_default()
                .trim()
                .to_string();
            if !override_id.is_empty() {
                return Some(override_id);
            }
            let cli_path = launchers::current_cli_executable();
            let cli_path_text = cli_path.to_string_lossy();
            let app_root_end = cli_path_text.find(".app/Contents/")? + ".app".len();
            let app_root = &cli_path_text[..app_root_end];
            let (stdout, _stderr) = exec_file(
                "plutil",
                &[
                    "-extract".to_string(),
                    "CFBundleIdentifier".to_string(),
                    "raw".to_string(),
                    Path::new(app_root)
                        .join("Contents")
                        .join("Info.plist")
                        .to_string_lossy()
                        .to_string(),
                ],
            )
            .ok()?;
            let bundle_id = stdout.trim().to_string();
            if bundle_id.is_empty() {
                None
            } else {
                Some(bundle_id)
            }
        })
        .clone()
}

fn activate_ghostex_app() {
    let script = match resolve_ghostex_app_bundle_id() {
        Some(bundle_id) => format!("tell application id \"{bundle_id}\" to activate"),
        None => "tell application \"Ghostex\" to activate".to_string(),
    };
    let _ = exec_file("osascript", &["-e".to_string(), script]);
}

fn capture_screenshot(output: &Path, flags: &Flags) -> CliResult<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if flags.text("activate").as_deref() != Some("false") {
        activate_ghostex_app();
    }
    exec_file(
        "screencapture",
        &["-x".to_string(), output.to_string_lossy().to_string()],
    )?;
    Ok(())
}

fn collect_logs(lines: f64) -> Value {
    let mut result = Map::new();
    let mut entries: Vec<String> = std::fs::read_dir(log_dir())
        .map(|reader| {
            reader
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    entries.sort();
    for file in entries.into_iter().filter(|entry| entry.ends_with(".log")) {
        let text = std::fs::read_to_string(log_dir().join(&file)).unwrap_or_default();
        let file_lines = slice_last(split_log_lines(&text), lines);
        result.insert(
            file,
            Value::Array(file_lines.into_iter().map(Value::String).collect()),
        );
    }
    Value::Object(result)
}

// ---------------------------------------------------------------------------
// screenshot / logs / bundle
// ---------------------------------------------------------------------------

pub fn screenshot_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let output = launchers::js_path_resolve(&match parsed.rest.first() {
        Some(target) => PathBuf::from(target),
        None => cli_dir().join(format!("screenshot-{}.png", timestamp_slug())),
    });
    capture_screenshot(&output, &parsed.flags)?;
    print_json(&json!({ "ok": true, "output": output.to_string_lossy() }));
    Ok(())
}

pub fn logs_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let lines = number_flag_or(flags, "lines", 200.0);
    let api_attempt = (|| -> CliResult<()> {
        let params = compact_object(json!({
            "event": flags.0.get("event").map(|value| value.as_json()),
            "eventPrefix": flags.0.get("eventPrefix").map(|value| value.as_json()),
            "level": flags.0.get("level").map(|value| value.as_json()),
            "limit": number_value(lines),
            "order": flags.0.get("order").map(|value| value.as_json()),
            "reverse": flags.0.get("reverse").map(parse_boolean),
            "since": flags.0.get("since").map(|value| value.as_json()),
            "until": flags.0.get("until").map(|value| value.as_json()),
        }));
        let result = call_gxserver_rpc("/api/queryLogs", &params, flags)?;
        let entries: Vec<Value> = result
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let filtered: Vec<Value> = if flags.truthy("grep") {
            let grep = flags.text("grep").unwrap_or_default();
            entries
                .into_iter()
                .filter(|entry| {
                    serde_json::to_string(entry)
                        .unwrap_or_default()
                        .contains(&grep)
                })
                .collect()
        } else {
            entries
        };
        if flags.truthy("json") {
            let mut merged = result.as_object().cloned().unwrap_or_default();
            merged.insert("entries".to_string(), Value::Array(filtered));
            merged.insert("ok".to_string(), Value::Bool(true));
            merged.insert(
                "source".to_string(),
                Value::String("gxserver-api".to_string()),
            );
            print_json(&Value::Object(merged));
            return Ok(());
        }
        for entry in filtered {
            println!("{}", serde_json::to_string(&entry).unwrap_or_default());
        }
        Ok(())
    })();
    match api_attempt {
        Ok(()) => return Ok(()),
        Err(CliError::Connection(_)) => {}
        Err(error) => return Err(error),
    }

    let file = flags
        .text("file")
        .unwrap_or_else(|| "gxserver.jsonl".to_string());
    let log_path = if Path::new(&file).is_absolute() {
        PathBuf::from(&file)
    } else if file == "gxserver.jsonl" {
        gxserver_log_path()
    } else {
        log_dir().join(&file)
    };
    let text = std::fs::read_to_string(&log_path).map_err(|error| {
        CliError::Other(format!("Could not read {}: {error}", log_path.display()))
    })?;
    let filtered = slice_last(filter_log_lines(&text, flags), lines);
    if flags.truthy("json") {
        print_json(&json!({
            "file": log_path.to_string_lossy(),
            "lines": filtered,
            "ok": true,
            "source": "local-file",
        }));
        return Ok(());
    }
    println!("{}", filtered.join("\n"));
    Ok(())
}

pub fn bundle_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let output_dir = launchers::js_path_resolve(&match parsed.rest.first() {
        Some(target) => PathBuf::from(target),
        None => cli_dir().join(format!("bundle-{}", timestamp_slug())),
    });
    std::fs::create_dir_all(&output_dir)?;
    let state = actions::send_gxserver_cli_action("state", &json!({}), flags)?;
    let screenshot = output_dir.join("screenshot.png");
    capture_screenshot(&screenshot, flags)?;
    let logs = collect_logs(number_flag_or(flags, "lines", 500.0));
    std::fs::write(
        output_dir.join("state.json"),
        serde_json::to_string_pretty(&state).unwrap_or_else(|_| "null".to_string()),
    )?;
    std::fs::write(
        output_dir.join("logs.json"),
        serde_json::to_string_pretty(&logs).unwrap_or_else(|_| "null".to_string()),
    )?;
    print_json(&json!({
        "logs": output_dir.join("logs.json").to_string_lossy(),
        "ok": true,
        "outputDir": output_dir.to_string_lossy(),
        "screenshot": screenshot.to_string_lossy(),
    }));
    Ok(())
}

// ---------------------------------------------------------------------------
// android-check
// ---------------------------------------------------------------------------

pub fn android_check_command(args: &[String]) -> CliResult<()> {
    let flags = parse_args(args).flags;
    let result = run_android_readiness_check(&flags);
    /*
    The hard cutover readiness check uses authenticated gxserver health and
    inventory APIs. zmx availability comes from gxserver tool capabilities, and
    the macOS app bridge is not a readiness fallback.
    */
    let ok = result.get("ok") == Some(&Value::Bool(true));
    if flags.truthy("json") {
        print_json(&result);
    } else if ok {
        println!(
            "Ghostex Android ready: {} sessions, persistence {}.",
            js_display(result.get("sessions")),
            js_display(result.get("sessionPersistenceProvider"))
        );
    } else {
        eprintln!("{}", js_display(result.get("error")));
    }
    if !ok {
        crate::ghostex_cli::set_exit_code(1);
    }
    Ok(())
}

pub fn run_android_readiness_check(flags: &Flags) -> Value {
    let target = match resolve_gxserver_server_target(flags, &json!({})) {
        Ok(target) => target,
        Err(error) => {
            return json!({ "error": error.to_string(), "ok": false });
        }
    };
    let health = fetch_gxserver_health(&target, 1_000).ok().flatten();
    let Some(health) = health else {
        return json!({
            "error": format!(
                "Could not load gxserver health from {}. Start gxserver and try again.",
                target.base_url
            ),
            "ok": false,
        });
    };
    let zmx_tool = health
        .get("tools")
        .and_then(Value::as_array)
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| tool.get("tool").and_then(Value::as_str) == Some("zmx"))
        });
    let availability = zmx_tool
        .and_then(|tool| tool.get("availability"))
        .and_then(Value::as_str);
    if availability != Some("available") {
        let mut failure = Map::new();
        failure.insert(
            "error".to_string(),
            zmx_tool
                .and_then(|tool| tool.get("message"))
                .filter(|message| !message.is_null())
                .cloned()
                .unwrap_or_else(|| {
                    Value::String("gxserver zmx capability is unavailable.".to_string())
                }),
        );
        failure.insert("ok".to_string(), Value::Bool(false));
        if let Some(server_id) = health.get("serverId") {
            failure.insert("serverId".to_string(), server_id.clone());
        }
        return Value::Object(failure);
    }
    let zmx_path = zmx_tool
        .and_then(|tool| tool.get("executablePath"))
        .cloned();

    let result = actions::send_gxserver_cli_action("listSessions", &json!({}), flags)
        .unwrap_or_else(|error| json!({ "error": error.to_string(), "ok": false }));
    if is_failed_cli_result(&result) {
        let mut failure = Map::new();
        failure.insert(
            "error".to_string(),
            result
                .get("error")
                .filter(|error| !error.is_null())
                .cloned()
                .unwrap_or_else(|| {
                    Value::String("Could not load Ghostex sessions from gxserver.".to_string())
                }),
        );
        failure.insert("ok".to_string(), Value::Bool(false));
        if let Some(server_id) = health.get("serverId") {
            failure.insert("serverId".to_string(), server_id.clone());
        }
        if let Some(zmx_path) = &zmx_path {
            failure.insert("zmxPath".to_string(), zmx_path.clone());
        }
        return Value::Object(failure);
    }

    let sessions = result
        .get("sessions")
        .and_then(Value::as_array)
        .map(|sessions| sessions.len())
        .unwrap_or(0);
    let mut ready = Map::new();
    ready.insert("ok".to_string(), Value::Bool(true));
    if let Some(server_id) = health.get("serverId") {
        ready.insert("serverId".to_string(), server_id.clone());
    }
    ready.insert("sessions".to_string(), json!(sessions));
    if let Some(zmx_path) = &zmx_path {
        ready.insert("zmxPath".to_string(), zmx_path.clone());
    }
    Value::Object(ready)
}

/// Port of readAndroidReadinessSettings (exported for tests/tools like the
/// Node CLI export; the readiness check itself now uses gxserver health).
pub fn read_android_readiness_settings(settings_path: Option<&Path>) -> Value {
    let default_path = shared_settings_path();
    let settings_path = settings_path.unwrap_or(&default_path);
    let text = std::fs::read_to_string(settings_path).unwrap_or_default();
    if text.trim().is_empty() {
        return json!({
            "error": format!(
                "Ghostex settings were not found at {}. Start Ghostex, set Session persistence to zmx, and try again.",
                settings_path.display()
            ),
            "ok": false,
        });
    }
    let settings: Option<Value> = serde_json::from_str(&text).ok();
    let Some(settings) = settings.filter(Value::is_object) else {
        return json!({
            "error": format!(
                "Ghostex settings at {} are not valid JSON. Open Ghostex settings, save Session persistence as zmx, and try again.",
                settings_path.display()
            ),
            "ok": false,
        });
    };
    /*
    Android supports zmx only for this release, but readiness should not depend
    on presentation casing or accidental surrounding whitespace in the shared
    settings JSON. Normalize the provider token before enforcing the contract.
    */
    let provider = settings.get("sessionPersistenceProvider");
    let provider_string = match provider {
        None | Some(Value::Null) => String::new(),
        Some(value) => js_display(Some(value)),
    };
    let normalized_provider = provider_string.trim().to_lowercase();
    if normalized_provider != "zmx" {
        let fallback_provider = match provider {
            Some(value) if js_truthy(value) => value.clone(),
            _ => Value::String("off".to_string()),
        };
        return json!({
            "error": format!(
                "Ghostex session persistence is set to {}. Open Ghostex Settings and set Session persistence to zmx before connecting from Android.",
                js_display(Some(&fallback_provider))
            ),
            "ok": false,
            "sessionPersistenceProvider": fallback_provider,
        });
    }
    json!({ "ok": true, "sessionPersistenceProvider": normalized_provider })
}

/// Port of resolveCommandPath: `command -v -- '<command>'` through the
/// interactive shell, first stdout line or "".
pub fn resolve_command_path(command: &str) -> String {
    let shell = launchers::resolve_cli_interactive_shell_launch();
    let output = Command::new(&shell.executable)
        .args([
            shell.command_flag.clone(),
            format!("command -v -- {}", launchers::shell_quote(command)),
        ])
        .output();
    let stdout = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).to_string()
        }
        _ => String::new(),
    };
    stdout
        .trim()
        .split(['\n'])
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .next()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghostex_cli::args::parse_args;

    fn flags_of(args: &[&str]) -> Flags {
        parse_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).flags
    }

    #[test]
    fn filter_log_lines_applies_since_and_grep() {
        let text = "[2026-07-01T00:00:00Z] alpha\n\n[2026-07-02T00:00:00Z] beta\r\n[2026-07-03T00:00:00Z] gamma beta\n";
        let all = filter_log_lines(text, &flags_of(&[]));
        assert_eq!(all.len(), 3);
        // --since keeps lines containing the value or sorting after "[<since>".
        let since = filter_log_lines(text, &flags_of(&["--since", "2026-07-02"]));
        assert_eq!(
            since,
            vec![
                "[2026-07-02T00:00:00Z] beta",
                "[2026-07-03T00:00:00Z] gamma beta"
            ]
        );
        // --grep is a plain substring filter.
        let grep = filter_log_lines(text, &flags_of(&["--grep", "beta"]));
        assert_eq!(grep.len(), 2);
        let both = filter_log_lines(
            text,
            &flags_of(&["--since", "2026-07-03", "--grep", "beta"]),
        );
        assert_eq!(both, vec!["[2026-07-03T00:00:00Z] gamma beta"]);
    }

    #[test]
    fn slice_last_matches_js_negative_slice() {
        let items: Vec<i32> = (1..=5).collect();
        assert_eq!(slice_last(items.clone(), 2.0), vec![4, 5]);
        assert_eq!(slice_last(items.clone(), 10.0), vec![1, 2, 3, 4, 5]);
        // NaN behaves like slice(0): keep everything.
        assert_eq!(slice_last(items.clone(), f64::NAN), vec![1, 2, 3, 4, 5]);
        // Negative --lines becomes a positive slice start, like the JS.
        assert_eq!(slice_last(items, -2.0), vec![3, 4, 5]);
    }

    #[test]
    fn number_flag_or_preserves_nan_for_unparseable_values() {
        assert_eq!(number_flag_or(&flags_of(&[]), "lines", 200.0), 200.0);
        assert_eq!(
            number_flag_or(&flags_of(&["--lines", "40"]), "lines", 200.0),
            40.0
        );
        assert!(number_flag_or(&flags_of(&["--lines", "abc"]), "lines", 200.0).is_nan());
    }

    #[test]
    fn android_readiness_settings_report_missing_invalid_and_wrong_provider() {
        let root = std::env::temp_dir().join(format!(
            "gx-cli-diagnostics-{}-settings",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("mkdir");

        let missing = root.join("missing.json");
        let result = read_android_readiness_settings(Some(&missing));
        assert_eq!(result.get("ok"), Some(&Value::Bool(false)));
        assert_eq!(
            result.get("error").and_then(Value::as_str),
            Some(
                format!(
                    "Ghostex settings were not found at {}. Start Ghostex, set Session persistence to zmx, and try again.",
                    missing.display()
                )
                .as_str()
            )
        );

        let invalid = root.join("invalid.json");
        std::fs::write(&invalid, "not-json").expect("write");
        let result = read_android_readiness_settings(Some(&invalid));
        assert_eq!(
            result.get("error").and_then(Value::as_str),
            Some(
                format!(
                    "Ghostex settings at {} are not valid JSON. Open Ghostex settings, save Session persistence as zmx, and try again.",
                    invalid.display()
                )
                .as_str()
            )
        );

        let wrong = root.join("wrong.json");
        std::fs::write(&wrong, r#"{"sessionPersistenceProvider":"tmux"}"#).expect("write");
        let result = read_android_readiness_settings(Some(&wrong));
        assert_eq!(result.get("ok"), Some(&Value::Bool(false)));
        assert_eq!(
            result.get("sessionPersistenceProvider"),
            Some(&Value::String("tmux".to_string()))
        );
        assert!(result
            .get("error")
            .and_then(Value::as_str)
            .expect("error")
            .starts_with("Ghostex session persistence is set to tmux."));

        let off = root.join("off.json");
        std::fs::write(&off, r#"{"sessionPersistenceProvider":""}"#).expect("write");
        let result = read_android_readiness_settings(Some(&off));
        assert_eq!(
            result.get("sessionPersistenceProvider"),
            Some(&Value::String("off".to_string()))
        );

        let ok = root.join("ok.json");
        std::fs::write(&ok, r#"{"sessionPersistenceProvider":" ZMX "}"#).expect("write");
        let result = read_android_readiness_settings(Some(&ok));
        assert_eq!(result.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(
            result.get("sessionPersistenceProvider"),
            Some(&Value::String("zmx".to_string()))
        );
    }

    #[test]
    fn number_value_serializes_integers_without_fraction() {
        assert_eq!(number_value(200.0), json!(200));
        assert_eq!(number_value(1.5), json!(1.5));
        assert_eq!(number_value(f64::NAN), Value::Null);
    }
}
