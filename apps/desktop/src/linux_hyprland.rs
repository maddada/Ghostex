//! Ghostex's Hyprland integration while Ghostex runs: tags its windows as terminals, keeps its
//! dialogs and menus floating and undecorated (`owned_window_rule_requests`), and reads whether
//! Hyprland blurs see-through windows (`blurs_windows`).
//!
//! CDXC:Clipboard 2026-10-28 WHY:
//! Omarchy's universal copy and paste (Super+C / Super+V) send Ctrl+Insert / Shift+Insert to windows
//! tagged `terminal` and Ctrl+C / Ctrl+V to every other window, so in a Ghostex terminal Super+C
//! sent ^C and stopped the running command. The user chose to add the tag from Ghostex itself
//! ("we add the rules for users on launch just for ghostex"): a window rule for Ghostex's own class
//! only, added at runtime over Hyprland's request socket, so no config file of the user's is
//! rewritten and the rule is gone once Hyprland reloads without Ghostex. A config reload drops
//! runtime rules, so the rules are added again (and the blur read again) on each `configreloaded`
//! event. Text fields and the
//! terminal take Ctrl+Insert / Shift+Insert (main.rs, terminal_element.rs, zorite-editor), so a
//! tagged Ghostex copies and pastes everywhere. Hyprland 0.56 reads its config as Lua and takes the
//! rule through `eval`; older versions take `keyword windowrule` (0.53 syntax) or `windowrulev2`.
//! Nothing here runs outside Hyprland, and it never fails the launch; the one blur read at start
//! waits at most `REQUEST_TIMEOUT` on a socket that does not answer.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

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

/// Ghostex's own windows that are not a main window (each workspace window and the detached board
/// open titled `Ghostex` and `Beads…`): the app modals, onboarding, Ghostex pages, the chat's menus
/// and dialogs. Matched on the title the window opened with, which these never change.
const OWNED_WINDOW_MATCH_LUA: &str =
    r#"class = "^(ghostex)$", initial_title = "negative:^(Ghostex|Beads.*)$""#;
const OWNED_WINDOW_MATCH_V2: &str = "class:^(ghostex)$, initialTitle:negative:^(Ghostex|Beads.*)$";

/// The requests that keep Ghostex's dialogs and menus floating and undecorated, newest syntax
/// first, as `rule_requests`.
///
/// CDXC:PlatformSupport 2026-11-14 DECISION:
/// User (Omarchy audit Q10): Ghostex adds its own Hyprland runtime rules for its menus and dialogs, with no Omarchy titlebar or border, like the terminal tag it already adds; and "windows that SHOULD float (Settings, onboarding, dialogs, pages and the like) must open floating even on Omarchy's tiled and scrolling workspaces, not only on floating ones". Every Ghostex window but a main window or the detached board floats with no border (`border_size = 0`; 0.56 has no `no_border`) and no shadow; the titlebar Omarchy's floating workspaces add (its hyprbars plugin) is turned off in a request of its own, which a Hyprland without the plugin refuses.
fn owned_window_rule_requests() -> [String; 2] {
    [
        format!(
            "eval hl.window_rule({{ match = {{ {OWNED_WINDOW_MATCH_LUA} }}, float = true, border_size = 0, no_shadow = true }})"
        ),
        format!(
            "[[BATCH]]keyword windowrulev2 float, {OWNED_WINDOW_MATCH_V2}; keyword windowrulev2 noborder, {OWNED_WINDOW_MATCH_V2}; keyword windowrulev2 noshadow, {OWNED_WINDOW_MATCH_V2}"
        ),
    ]
}

/// Ghostex's menus, popovers and dropdowns: its own windows that open without a title (the app
/// modals, onboarding and the like open titled).
const MENU_WINDOW_MATCH_LUA: &str = r#"class = "^(ghostex)$", initial_title = "^$""#;
const MENU_WINDOW_MATCH_V2: &str = "class:^(ghostex)$, initialTitle:^$";

/// CDXC:PlatformSupport 2026-11-14 DECISION:
/// User, of a chat message's ⋯ menu opening with Omarchy's titlebar, a grey border and Hyprland's open animation: "Menus, popovers and dropdowns must open with no animation (Hyprland's noanim or equivalent), no titlebar buttons and no border. Dialogs that should be windows keep what you decide in Q10, but ... avoid the top-right buttons there too if possible." Menus get `no_anim` on top of the rules every Ghostex dialog and menu gets (no titlebar, which carries the buttons, no border); dialogs keep Hyprland's animation.
fn menu_window_rule_requests() -> [String; 2] {
    [
        format!("eval hl.window_rule({{ match = {{ {MENU_WINDOW_MATCH_LUA} }}, no_anim = true }})"),
        format!("keyword windowrulev2 noanim, {MENU_WINDOW_MATCH_V2}"),
    ]
}

fn owned_window_titlebar_requests() -> [String; 2] {
    [
        format!(
            r#"eval hl.window_rule({{ match = {{ {OWNED_WINDOW_MATCH_LUA} }}, ["hyprbars:no_bar"] = true }})"#
        ),
        format!("keyword windowrulev2 plugin:hyprbars:nobar, {OWNED_WINDOW_MATCH_V2}"),
    ]
}

/// Whether Hyprland blurs what is behind a see-through window (`decoration:blur:enabled`): 0
/// not asked yet, 1 off, 2 on.
static BLUR: AtomicU8 = AtomicU8::new(0);
static BLUR_WATCHERS: Mutex<Vec<UnboundedSender<()>>> = Mutex::new(Vec::new());

/// Whether this Hyprland blurs behind see-through windows, `None` outside Hyprland or before it
/// answered.
///
/// CDXC:Theming 2026-11-14 SEE-ALSO: app/helpers/window_glass.rs (`desktop_withholds_blur`) paints the window opaque while this is `Some(false)`.
pub(crate) fn blurs_windows() -> Option<bool> {
    match BLUR.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// A message each time `blurs_windows` changes (Omarchy's Glassy Windows reloads the config).
pub(crate) fn watch_blur() -> UnboundedReceiver<()> {
    let (sender, receiver) = futures::channel::mpsc::unbounded();
    if let Ok(mut watchers) = BLUR_WATCHERS.lock() {
        watchers.push(sender);
    }
    receiver
}

fn read_blur(directory: &PathBuf) {
    let Ok(answer) = send_request(directory, "j/getoption decoration:blur:enabled") else {
        return;
    };
    let Some(enabled) = parse_blur_answer(&answer) else {
        return;
    };
    let value = if enabled { 2 } else { 1 };
    if BLUR.swap(value, Ordering::Relaxed) != value
        && let Ok(mut watchers) = BLUR_WATCHERS.lock()
    {
        watchers.retain(|watcher| watcher.unbounded_send(()).is_ok());
    }
}

/// `{"option": …, "bool": false}` on Hyprland 0.56, `{"option": …, "int": 0}` before it.
fn parse_blur_answer(answer: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_str(answer.trim()).ok()?;
    value
        .get("bool")
        .and_then(serde_json::Value::as_bool)
        .or_else(|| {
            value
                .get("int")
                .and_then(serde_json::Value::as_i64)
                .map(|value| value != 0)
        })
}

/// Starts Ghostex's Hyprland rules and watches its blur; does nothing anywhere else. The blur is
/// read before this returns, so the first window opens with the right glass.
pub(crate) fn start_hyprland_integration() {
    let Some(directory) = instance_directory() else {
        return;
    };
    read_blur(&directory);
    let spawned = std::thread::Builder::new()
        .name("hyprland-integration".to_string())
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

/// Whether Ghostex runs inside a Hyprland session (its request socket answers to the signature).
pub(crate) fn running_on_hyprland() -> bool {
    static ON_HYPRLAND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON_HYPRLAND.get_or_init(|| instance_directory().is_some())
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
    apply_first_accepted(directory, &rule_requests(), "the terminal tag");
    apply_first_accepted(directory, &owned_window_rule_requests(), "the dialog rules");
    apply_first_accepted(directory, &menu_window_rule_requests(), "the menu rule");
    // Refused where the titlebar plugin is not loaded, which needs no rule.
    for request in owned_window_titlebar_requests() {
        if send_request(directory, &request).is_ok_and(|answer| answer.trim() == "ok") {
            break;
        }
    }
}

fn apply_first_accepted(directory: &PathBuf, requests: &[String], what: &str) {
    let mut answers = Vec::new();
    for request in requests {
        match send_request(directory, request) {
            // A batch answers one `ok` per command.
            Ok(answer)
                if !answer.trim().is_empty()
                    && answer.split_whitespace().all(|word| word == "ok") =>
            {
                return;
            }
            Ok(answer) => answers.push(answer.trim().chars().take(160).collect::<String>()),
            Err(error) => {
                log_failure("Hyprland's socket did not answer", &error.to_string());
                return;
            }
        }
    }
    log_failure(
        &format!("this Hyprland accepted no known window rule syntax for {what}"),
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
            read_blur(directory);
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
