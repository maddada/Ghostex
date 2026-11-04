//! Tags Ghostex's windows as terminals on Hyprland while Ghostex runs.
//!
//! CDXC:Clipboard 2026-10-28 WHY:
//! Omarchy's universal copy and paste (Super+C / Super+V) send Ctrl+Insert / Shift+Insert to windows
//! tagged `terminal` and Ctrl+C / Ctrl+V to every other window, so in a Ghostex terminal Super+C
//! sent ^C and stopped the running command. The user chose to add the tag from Ghostex itself
//! ("we add the rules for users on launch just for ghostex"): a window rule for Ghostex's own class
//! only, added at runtime over Hyprland's request socket, so no config file of the user's is
//! rewritten and the rule is gone once Hyprland reloads without Ghostex. A config reload drops
//! runtime rules, so the rule is added again on each `configreloaded` event. Text fields and the
//! terminal take Ctrl+Insert / Shift+Insert (main.rs, terminal_element.rs, zorite-editor), so a
//! tagged Ghostex copies and pastes everywhere. Hyprland 0.56 reads its config as Lua and takes the
//! rule through `eval`; older versions take `keyword windowrule` (0.53 syntax) or `windowrulev2`.
//! Nothing here runs outside Hyprland, and it never delays or fails the launch.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// Ghostex's X11 class (`gpui_platform_window_app_id`).
const WINDOW_CLASS: &str = "ghostex";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// The requests that add the rule, newest syntax first; the first answered with `ok` wins.
fn rule_requests() -> [String; 3] {
    let class = format!("^({WINDOW_CLASS})$");
    [
        format!(r#"eval hl.window_rule({{ match = {{ class = "{class}" }}, tag = "+terminal" }})"#),
        format!("keyword windowrule tag +terminal, match:class {class}"),
        format!("keyword windowrulev2 tag +terminal, class:{class}"),
    ]
}

/// Starts tagging on Hyprland; does nothing anywhere else.
pub(crate) fn start_terminal_tag() {
    let Some(directory) = instance_directory() else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("hyprland-terminal-tag".to_string())
        .spawn(move || {
            apply_rule(&directory);
            watch_reloads(&directory);
        });
    if let Err(error) = spawned {
        log_failure(
            "could not start the Hyprland rule thread",
            &error.to_string(),
        );
    }
}

/// `$XDG_RUNTIME_DIR/hypr/<signature>` (Hyprland 0.40 and later), else `/tmp/hypr/<signature>`.
fn instance_directory() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .ok()
        .filter(|signature| !signature.is_empty() && !signature.contains('/'))?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    [
        runtime.map(|runtime| runtime.join("hypr")),
        Some(PathBuf::from("/tmp/hypr")),
    ]
    .into_iter()
    .flatten()
    .map(|base| base.join(&signature))
    .find(|directory| directory.join(".socket.sock").exists())
}

fn apply_rule(directory: &PathBuf) {
    let mut answers = Vec::new();
    for request in rule_requests() {
        match send_request(directory, &request) {
            Ok(answer) if answer.trim() == "ok" => return,
            Ok(answer) => answers.push(answer.trim().chars().take(160).collect::<String>()),
            Err(error) => {
                log_failure("Hyprland's socket did not answer", &error.to_string());
                return;
            }
        }
    }
    log_failure(
        "this Hyprland accepted no known window rule syntax",
        &answers.join(" | "),
    );
}

fn send_request(directory: &PathBuf, request: &str) -> std::io::Result<String> {
    let mut stream = UnixStream::connect(directory.join(".socket.sock"))?;
    stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
    stream.write_all(request.as_bytes())?;
    let mut answer = String::new();
    stream.take(64 * 1024).read_to_string(&mut answer)?;
    Ok(answer)
}

/// Re-adds the rule after each config reload, for as long as Hyprland's event socket stays open.
fn watch_reloads(directory: &PathBuf) {
    let Ok(stream) = UnixStream::connect(directory.join(".socket2.sock")) else {
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else {
            return;
        };
        if is_config_reloaded_event(&line) {
            apply_rule(directory);
        }
    }
}

fn is_config_reloaded_event(line: &str) -> bool {
    line.split_once(">>")
        .map_or(line, |(event, _)| event)
        .trim()
        == "configreloaded"
}

fn log_failure(what: &str, detail: &str) {
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::HostLifecycle,
        "linux.hyprlandTerminalTag.failed",
        serde_json::json!({ "what": what, "detail": detail }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_names_only_ghostex_and_tries_lua_first() {
        let requests = rule_requests();
        assert_eq!(
            requests[0],
            r#"eval hl.window_rule({ match = { class = "^(ghostex)$" }, tag = "+terminal" })"#
        );
        assert_eq!(
            requests[1],
            "keyword windowrule tag +terminal, match:class ^(ghostex)$"
        );
        assert_eq!(
            requests[2],
            "keyword windowrulev2 tag +terminal, class:^(ghostex)$"
        );
    }

    #[test]
    fn only_the_reload_event_reapplies() {
        assert!(is_config_reloaded_event("configreloaded>>"));
        assert!(is_config_reloaded_event("configreloaded"));
        assert!(!is_config_reloaded_event("activewindow>>ghostex,ftue-demo"));
        assert!(!is_config_reloaded_event("configreloadedx>>"));
    }
}
