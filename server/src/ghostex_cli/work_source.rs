//! `ghostex issues source`: a workspace's ticket command, a program that describes its own
//! tracker (crate::work_mode::command). Shows what is set, sets or re-checks the command's path,
//! clears it, and stores one of its credentials read from stdin, like `issues linear-key`.

use std::path::{Component, Path};

use serde_json::{json, Map, Value};

use super::args::Flags;
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};
use crate::work_mode::SWITCH_OFFER;

const USAGE: &str = "Use: ghostex issues source [set <path> [--switch] | check | clear | credential <name> [--clear]] [--workspace name|id]";

/// `ghostex issues source [set <path> [--switch] | check | clear | credential <name> [--clear]]
/// [--workspace name|id]`. Without `--workspace` it is the default (Personal) workspace's.
pub(super) fn work_source_command(rest: &[String], flags: &Flags) -> CliResult<()> {
    let mut params = Map::new();
    if let Some(workspace) = flags.string_value("workspace") {
        params.insert("workspaceId".to_string(), json!(workspace));
    }
    let flags = &super::work_mode::with_default_timeout(flags, "30000");
    let result = match rest.first().map(String::as_str) {
        None => rpc::call_gxserver_rpc("/api/readWorkSource", &Value::Object(params), flags)?,
        Some("set") => {
            let path = rest
                .get(1)
                .ok_or_else(|| CliError::Other("Pass the command's path. ".to_string() + USAGE))?;
            let target =
                rpc::resolve_gxserver_server_target(flags, &Value::Object(params.clone()))?;
            params.insert(
                "path".to_string(),
                json!(path_to_send(path, rpc::targets_this_computer(&target))),
            );
            params.insert("switch".to_string(), json!(flags.truthy("switch")));
            rpc::call_gxserver_rpc("/api/setWorkSource", &Value::Object(params), flags)
                .map_err(with_switch_hint)?
        }
        Some("check") => {
            // The server reads the path it has under its writer lock, so a path set meanwhile from
            // another client is never put back.
            params.insert("check".to_string(), json!(true));
            rpc::call_gxserver_rpc("/api/setWorkSource", &Value::Object(params), flags)
                .map_err(with_switch_hint)?
        }
        Some("clear") => {
            params.insert("path".to_string(), Value::Null);
            rpc::call_gxserver_rpc("/api/setWorkSource", &Value::Object(params), flags)?
        }
        Some("credential") => {
            let name = rest.get(1).ok_or_else(|| {
                CliError::Other("Pass the credential's name. ".to_string() + USAGE)
            })?;
            let value = if flags.truthy("clear") {
                Value::Null
            } else {
                json!(read_credential_line(name)?)
            };
            params.insert("values".to_string(), json!({ name.as_str(): value }));
            rpc::call_gxserver_rpc(
                "/api/setWorkSourceCredentials",
                &Value::Object(params),
                flags,
            )?
        }
        Some(other) => {
            return Err(CliError::Other(format!(
                "Unknown issues source command \"{other}\". {USAGE}"
            )))
        }
    };
    print_json(&result);
    // A check that ran and failed is in the answer; it still fails the command, for scripts.
    match result.pointer("/check/error").and_then(Value::as_str) {
        Some(error) => Err(CliError::Other(error.to_string())),
        None => Ok(()),
    }
}

/// The path `set` sends for `typed`. The server refuses a relative path that isn't a bare name,
/// since its own current folder isn't one the user knows; the CLI knows the user's, so it makes
/// such a path absolute (`std::path::absolute`: on Windows with the system's own rules, a
/// drive-relative `C:tools\x.ps1` or `C:x.ps1` included; elsewhere joined to the current folder,
/// symlinks left as they are). A bare name (looked up on PATH), `~` and `~/…` (the server
/// expands them) and an absolute path are sent as typed, and so is a path `absolute` can't make
/// absolute, which the server then refuses in its own words. For another computer's gxserver
/// (`--server`, `this_computer` false) this shell's folder means nothing there, so a relative
/// path goes as typed too, and that server refuses it.
fn path_to_send(typed: &str, this_computer: bool) -> String {
    let trimmed = typed.trim();
    let path = Path::new(trimmed);
    let drive_prefixed = matches!(path.components().next(), Some(Component::Prefix(_)));
    let bare_name = !trimmed.contains('/') && !trimmed.contains('\\') && !drive_prefixed;
    if !this_computer || trimmed.starts_with('~') || path.is_absolute() || bare_name {
        return typed.to_string();
    }
    std::path::absolute(path)
        .ok()
        .and_then(|absolute| absolute.into_os_string().into_string().ok())
        .unwrap_or_else(|| typed.to_string())
}

/// The server's refusal of a changed id, followed by how to accept it. It names no path and no
/// workspace: the server masked its own message, and either could hold a stored credential.
///
/// The refusal carries only its message, so its fixed ending is what tells it apart. A stored
/// secret that matches a word of that ending is masked in it too, and a secret too short to mask
/// in place withholds the whole refusal; the hint is then left out, and the refusal still says
/// what happened, or that it was hidden.
fn with_switch_hint(error: CliError) -> CliError {
    let CliError::Rpc { message, response } = error else {
        return error;
    };
    if !message.ends_with(SWITCH_OFFER) {
        return CliError::Rpc { message, response };
    }
    let message = format!("{message} {SWITCH_HINT}");
    CliError::Rpc { message, response }
}

const SWITCH_HINT: &str =
    "Run `ghostex issues source set` again with the command's path and `--switch` to switch.";

/// One line from stdin, so a credential never sits in argv or the shell history.
fn read_credential_line(name: &str) -> CliResult<String> {
    super::work_mode::read_stdin_line(
        &format!("Paste the value of {name} and press Enter: "),
        name,
        format!("No value was given for {name}. Pipe it in, or pass --clear to remove it."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(message: &str) -> CliError {
        rpc::rpc_error(json!({ "ok": false, "error": "badRequest", "message": message }))
    }

    #[test]
    fn a_relative_path_is_sent_under_the_current_folder() {
        let folder = std::env::current_dir().expect("current folder");
        for (typed, under) in [("./tracker.sh", "tracker.sh"), ("bin/source", "bin/source")] {
            let sent = path_to_send(typed, true);
            assert!(Path::new(&sent).is_absolute(), "{sent}");
            assert_eq!(Path::new(&sent), folder.join(under));
        }
        // Looked up on PATH, expanded by the server, or already absolute: sent as typed, even
        // where `absolute` would drop a `.`.
        let absolute = folder.join(".").join("tracker").display().to_string();
        for typed in ["tracker", "~", "~/bin/x", absolute.as_str()] {
            assert_eq!(path_to_send(typed, true), typed);
        }
    }

    /// Windows' own forms; these run only on Windows.
    #[cfg(windows)]
    #[test]
    fn a_windows_relative_path_is_sent_absolute() {
        let folder = std::env::current_dir().expect("current folder");
        assert_eq!(
            Path::new(&path_to_send(r".\x.ps1", true)),
            folder.join("x.ps1")
        );
        // Drive-relative: absolute on drive C, from that drive's own current folder.
        for (typed, end) in [("C:x.ps1", r"x.ps1"), (r"C:dir\x.ps1", r"dir\x.ps1")] {
            let sent = path_to_send(typed, true);
            assert!(Path::new(&sent).is_absolute(), "{sent}");
            assert!(sent.starts_with(r"C:\") && sent.ends_with(end), "{sent}");
        }
        // Root-relative: absolute on the current folder's own drive.
        let Some(Component::Prefix(drive)) = folder.components().next() else {
            panic!("{} has no drive", folder.display());
        };
        let drive = drive.as_os_str().to_string_lossy();
        assert_eq!(path_to_send(r"\x.ps1", true), format!(r"{drive}\x.ps1"));
        for typed in [
            r"C:\tools\x.ps1",
            r"\\server\share\x.ps1",
            r"\\?\C:\tools\x.ps1",
        ] {
            assert_eq!(path_to_send(typed, true), typed);
        }
    }

    /// Another computer's gxserver: this shell's folder means nothing there, so the path goes as
    /// typed and that server refuses it in its own words.
    #[test]
    fn another_computers_server_gets_a_relative_path_as_typed() {
        for typed in [
            "./tracker.sh",
            "bin/source",
            r".\x.ps1",
            "tracker",
            "~/bin/x",
        ] {
            assert_eq!(path_to_send(typed, false), typed);
        }
    }

    #[test]
    fn only_this_computers_server_counts_as_this_computer() {
        let target = |kind: &str, base_url: &str| rpc::Target {
            base_url: base_url.to_string(),
            forward_plan: None,
            kind: kind.to_string(),
            profile_id: None,
            server_id: None,
            tailcat_plan: None,
            token: String::new(),
        };
        assert!(rpc::targets_this_computer(&target(
            "local",
            "http://127.0.0.1:58744"
        )));
        assert!(rpc::targets_this_computer(&target(
            "direct",
            "http://localhost:58799"
        )));
        assert!(rpc::targets_this_computer(&target(
            "direct",
            "http://[::1]:58744"
        )));
        assert!(rpc::targets_this_computer(&target(
            "direct",
            "http://[::1]/"
        )));
        // The expanded spelling of the IPv6 loopback.
        assert!(rpc::targets_this_computer(&target(
            "direct",
            "http://[0:0:0:0:0:0:0:1]:58744"
        )));
        // Before an `@`, a loopback name is the URL's userinfo; the request goes to the host after it.
        assert!(!rpc::targets_this_computer(&target(
            "direct",
            "http://[::1]@remote.example:58744"
        )));
        assert!(!rpc::targets_this_computer(&target(
            "direct",
            "http://localhost@remote.example:58744"
        )));
        assert!(!rpc::targets_this_computer(&target(
            "direct",
            "http://localhost:58744@remote.example/"
        )));
        assert!(!rpc::targets_this_computer(&target(
            "direct",
            "http://[fd00::5]:58744"
        )));
        // A bracketed host without its closing bracket is malformed, never this computer.
        assert!(!rpc::targets_this_computer(&target(
            "direct",
            "http://[::1"
        )));
        assert!(!rpc::targets_this_computer(&target("direct", "[::1")));
        // Forwards to another computer listen on this one's loopback.
        assert!(!rpc::targets_this_computer(&target(
            "ssh",
            "http://127.0.0.1:61000"
        )));
        assert!(!rpc::targets_this_computer(&target(
            "tailcat",
            "http://127.0.0.1:61001"
        )));
        assert!(!rpc::targets_this_computer(&target(
            "direct",
            "http://10.0.0.5:58744"
        )));
    }

    #[test]
    fn a_refused_new_id_says_how_to_switch_to_it() {
        let refusal = format!(
            "This command now calls itself \"renamed\"; this workspace's links use \"fixture\". {SWITCH_OFFER}"
        );
        let hinted = with_switch_hint(refused(&refusal));
        assert_eq!(hinted.to_string(), format!("{refusal} {SWITCH_HINT}"));
        assert!(matches!(hinted, CliError::Rpc { .. }));
    }

    #[test]
    fn a_masked_refusal_gets_only_the_fixed_hint() {
        // The server masked a stored secret in the ids; the hint adds nothing of the request.
        let refusal = format!(
            "This command now calls itself \"r••••\"; this workspace's links use \"fixture\". {SWITCH_OFFER}"
        );
        let hinted = with_switch_hint(refused(&refusal)).to_string();
        assert_eq!(hinted, format!("{refusal} {SWITCH_HINT}"));
        let added = &hinted[refusal.len()..];
        assert!(
            !added.contains('/') && !added.contains("--workspace"),
            "{added}"
        );
    }

    #[test]
    fn other_refusals_get_no_switch_hint() {
        let other = "A workspace connected to a team uses its team's tracker; ticket commands are for this computer's workspaces.";
        assert_eq!(with_switch_hint(refused(other)).to_string(), other);
        // A secret matching a word of the ending masks it, and the hint is left out.
        let masked = "This command now calls itself \"renamed\"; this workspace's links use \"fixture\". Switch to it to use the •••• id.";
        assert_eq!(with_switch_hint(refused(masked)).to_string(), masked);
        let connection = with_switch_hint(CliError::Connection(SWITCH_OFFER.to_string()));
        assert_eq!(connection.to_string(), SWITCH_OFFER);
    }
}
