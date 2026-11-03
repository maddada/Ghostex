use super::*;
use std::collections::HashMap;

fn env_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn select(
    backend: &str,
    client_capability: Option<&str>,
    env: &HashMap<String, String>,
    available: bool,
) -> (PromptEditorSelection, Vec<String>) {
    let mut warnings = Vec::new();
    let selection = select_prompt_editor_command_with(
        backend,
        client_capability,
        "/tmp/prompt.md",
        &|key| env.get(key).cloned(),
        &move || available,
        &mut |message| warnings.push(message.to_string()),
    );
    (selection, warnings)
}

#[test]
fn select_custom_backend_uses_custom_command_or_default() {
    let env = env_map(&[("GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND", "  subl -w  ")]);
    let (selection, warnings) = select("custom", None, &env, true);
    assert_eq!(selection.kind, "custom");
    assert_eq!(
        selection.command_args,
        vec![
            "/bin/zsh",
            "-lc",
            "exec subl -w \"$@\"",
            "ghostex-prompt-editor",
            "/tmp/prompt.md"
        ]
    );
    assert!(warnings.is_empty());

    let (selection, _) = select("custom", None, &env_map(&[]), true);
    assert_eq!(selection.command_args[2], "exec code --wait \"$@\"");
}

#[test]
fn select_monaco_requires_macos_app_client_and_executable() {
    // Capability monaco → macOS app client → monaco.
    let (selection, _) = select("inherit", Some("monaco"), &env_map(&[]), true);
    assert_eq!(selection.kind, "monaco");
    assert_eq!(
        selection.command_args,
        vec!["ghostex", "floating-monaco-editor", "/tmp/prompt.md"]
    );

    // Backend monaco with macos-app env client (no zmx capability).
    let env = env_map(&[("GHOSTEX_PROMPT_EDITOR_CLIENT", "macos-app")]);
    let (selection, _) = select("monaco", None, &env, true);
    assert_eq!(selection.kind, "monaco");

    // Backend monaco but the zmx leader only advertises the machine
    // editor → not a macOS app client → machine editor.
    let (selection, _) = select("monaco", Some("editor"), &env, true);
    assert_eq!(selection.kind, "editor");

    // Backend monaco without any macOS app marker → machine editor.
    let (selection, _) = select("monaco", None, &env_map(&[]), true);
    assert_eq!(selection.kind, "editor");

    // Monaco requested but executable missing → warn + machine editor.
    let (selection, warnings) = select("inherit", Some("monaco"), &env_map(&[]), false);
    assert_eq!(selection.kind, "editor");
    assert_eq!(warnings, vec![ghostex_editor_unavailable_message(None)]);
}

#[test]
fn select_remote_code_server_never_falls_through_to_monaco_or_machine_editor() {
    let env = env_map(&[("EDITOR", "nvim")]);
    let (selection, warnings) = select("monaco", Some("code-server"), &env, true);
    assert_eq!(selection.kind, "code-server");
    // The command is the bundled code-server's own node running the embedded
    // session-manager script; waiting and window reuse happen inside it, not
    // through `--wait` / `--reuse-window` arguments.
    assert_eq!(selection.command_args.len(), 6);
    assert!(selection.command_args[0].contains("code-server"));
    assert_eq!(selection.command_args[1], "-e");
    assert!(selection.command_args[3].ends_with("vscodeSocket.js"));
    assert!(!selection
        .command_args
        .iter()
        .any(|arg| arg == "--wait" || arg == "--reuse-window"));
    assert_eq!(
        selection.command_args.last().map(String::as_str),
        Some("/tmp/prompt.md")
    );
    assert!(!selection
        .command_args
        .iter()
        .any(|arg| arg.contains("GhostexEditor")));
    assert!(!selection.command_args.iter().any(|arg| arg == "nvim"));
    assert!(warnings.is_empty());
}

#[test]
fn select_machine_editor_falls_through_environment_chain() {
    let env = env_map(&[
        ("GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL", "   "),
        (
            "GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR",
            "ghostex prompt-editor",
        ),
        ("VISUAL", "nvim"),
        ("EDITOR", "nano"),
    ]);
    let (selection, _) = select("inherit", Some("editor"), &env, true);
    assert_eq!(selection.kind, "editor");
    assert_eq!(selection.command_args[2], "exec nvim \"$@\"");

    // Everything unusable → vi.
    let env = env_map(&[("EDITOR", "gte --floating-editor -- gte")]);
    assert_eq!(
        machine_prompt_editor_command_with(&|key| env.get(key).cloned()),
        "vi"
    );
    assert_eq!(machine_prompt_editor_command_with(&|_| None), "vi");
}

#[test]
fn ghostex_prompt_editor_command_detection() {
    assert!(is_ghostex_prompt_editor_command("prompt-editor"));
    assert!(is_ghostex_prompt_editor_command(
        "/usr/local/bin/prompt-editor --flag"
    ));
    assert!(is_ghostex_prompt_editor_command("\"prompt-editor\""));
    assert!(is_ghostex_prompt_editor_command("ghostex prompt-editor"));
    assert!(is_ghostex_prompt_editor_command(
        "node /x/scripts/ghostex-cli.mjs prompt-editor"
    ));
    assert!(is_ghostex_prompt_editor_command(
        "ghostex floating-monaco-editor"
    ));
    assert!(is_ghostex_prompt_editor_command(
        "ghostex floating-editor -- gte"
    ));
    assert!(!is_ghostex_prompt_editor_command("vim"));
    assert!(!is_ghostex_prompt_editor_command("code --wait"));
    assert!(!is_ghostex_prompt_editor_command(
        "node /x/scripts/ghostex-cli.mjs open"
    ));
}

#[test]
fn native_focus_session_id_parses_global_refs() {
    assert_eq!(
        native_focus_session_id_from_global_session_ref("S1a:P2cde:G3fgh"),
        Some("P2cde:G3fgh".to_string())
    );
    assert_eq!(
        native_focus_session_id_from_global_session_ref("  S0a:P0abc:G0abc  "),
        Some("P0abc:G0abc".to_string())
    );
    assert_eq!(native_focus_session_id_from_global_session_ref(""), None);
    assert_eq!(
        native_focus_session_id_from_global_session_ref("P2cde:G3fgh"),
        None
    );
    assert_eq!(
        native_focus_session_id_from_global_session_ref("S1a:P2cde:G3fgh:extra"),
        None
    );
    // ^S[0-9][a-z0-9]$ is exactly three characters.
    assert_eq!(
        native_focus_session_id_from_global_session_ref("S1ab:P2cde:G3fgh"),
        None
    );
    // Uppercase tail rejected by [a-z0-9].
    assert_eq!(
        native_focus_session_id_from_global_session_ref("S1a:P2CDE:G3fgh"),
        None
    );
}

#[test]
fn final_status_matches_multiline_regex() {
    assert_eq!(final_ghostex_editor_status_from_text("saved"), "saved");
    assert_eq!(
        final_ghostex_editor_status_from_text("started\nsaved\n"),
        "saved"
    );
    // JS multiline anchors also break on \r (verified against Node).
    assert_eq!(final_ghostex_editor_status_from_text("saved\r\n"), "saved");
    assert_eq!(
        final_ghostex_editor_status_from_text("started\r\nsaved"),
        "saved"
    );
    assert_eq!(
        final_ghostex_editor_status_from_text("started\ncancelled"),
        "cancelled"
    );
    assert_eq!(final_ghostex_editor_status_from_text("exit:0"), "unknown");
    assert_eq!(final_ghostex_editor_status_from_text(" saved"), "unknown");
    assert_eq!(final_ghostex_editor_status_from_text(""), "unknown");
}

#[test]
fn sanitize_redacts_by_key_and_shape() {
    let payload = json!({
        "authToken": "abc",
        "backend": "monaco",
        "commandElapsedMs": 12,
        "command": "vim file",
        "cwd": "/Users/madda",
        "errorName": "Error\r\nBad",
        "event": "cli.monaco.failed",
        "hasInputFile": true,
        "inputByteCount": Value::Null,
        "items": [1, 2, 3],
        "nested": { "filePath": "/tmp/x", "ok": false },
        "note": "/Users/madda/secret.md",
        "randomKey": "hello",
        "serverUrl": "wat",
        "site": "https://example.com",
    });
    let sanitized = Value::Object(sanitize_prompt_editor_timeline_payload(
        payload.as_object().unwrap(),
    ));
    assert_eq!(sanitized["authToken"], "[redacted:secret]");
    assert_eq!(sanitized["backend"], "monaco");
    assert_eq!(sanitized["commandElapsedMs"], 12);
    assert_eq!(sanitized["command"], "[redacted]");
    assert_eq!(sanitized["cwd"], "[redacted:path]");
    assert_eq!(sanitized["errorName"], "Error\\n\\nBad");
    assert_eq!(sanitized["event"], "cli.monaco.failed");
    assert_eq!(sanitized["hasInputFile"], true);
    assert_eq!(sanitized["inputByteCount"], Value::Null);
    assert_eq!(sanitized["items"], json!({ "count": 3, "redacted": true }));
    assert_eq!(sanitized["nested"]["filePath"], "[redacted:path]");
    assert_eq!(sanitized["nested"]["ok"], false);
    assert_eq!(sanitized["note"], "[redacted:path]");
    assert_eq!(sanitized["randomKey"], "[redacted]");
    assert_eq!(sanitized["serverUrl"], "[redacted:url]");
    assert_eq!(sanitized["site"], "[redacted:url]");
}

#[test]
fn cli_session_key_requires_both_parts() {
    assert_eq!(
        cli_session_key(Some(&json!(" P1 ")), Some(&json!("G2"))),
        "P1:G2"
    );
    assert_eq!(cli_session_key(Some(&json!("P1")), None), "");
    assert_eq!(cli_session_key(Some(&json!("")), Some(&json!("G2"))), "");
    assert_eq!(cli_session_key(None, None), "");
    assert_eq!(cli_session_key(Some(&Value::Null), Some(&json!("G2"))), "");
}

#[test]
fn unavailable_message_appends_error_detail() {
    let expected = if cfg!(target_os = "macos") {
        "Ghostex standalone editor unavailable; using the machine/default editor. Set GHOSTEX_EDITOR_APP or install /Applications/GhostexEditor.app."
    } else {
        "Ghostex standalone editor unavailable; using the machine/default editor. Set GHOSTEX_EDITOR_APP or reinstall Ghostex with its bundled editor."
    };
    assert_eq!(ghostex_editor_unavailable_message(None), expected);
    assert_eq!(
        ghostex_editor_unavailable_message(Some("boom")),
        format!("{expected} boom")
    );
}

#[test]
fn connection_error_codes_match_js_list() {
    for code in ["ECONNREFUSED", "ENOENT", "ENOTFOUND", "ECONNRESET", "EPIPE"] {
        let error = EditorError {
            name: "Error",
            message: "x".to_string(),
            code: Some(match code {
                "ECONNREFUSED" => "ECONNREFUSED",
                "ENOENT" => "ENOENT",
                "ENOTFOUND" => "ENOTFOUND",
                "ECONNRESET" => "ECONNRESET",
                _ => "EPIPE",
            }),
        };
        assert!(is_ghostex_editor_connection_error(&error), "{code}");
    }
    assert!(!is_ghostex_editor_connection_error(&EditorError::new(
        "plain"
    )));
    assert!(EditorError::unavailable("x").is_unavailable());
    assert!(!is_ghostex_editor_connection_error(
        &EditorError::unavailable("x")
    ));
}

#[test]
fn js_path_resolution_is_lexical() {
    let base = Path::new("/base/dir");
    assert_eq!(
        js_path_resolve_from(base, "file.md"),
        PathBuf::from("/base/dir/file.md")
    );
    assert_eq!(
        js_path_resolve_from(base, "../other/./x.md"),
        PathBuf::from("/base/other/x.md")
    );
    assert_eq!(
        js_path_resolve_from(base, "/abs/y.md"),
        PathBuf::from("/abs/y.md")
    );
    assert_eq!(
        js_path_resolve_from(Path::new("/"), ".."),
        PathBuf::from("/")
    );
}

#[test]
fn editor_candidates_per_platform_match_node_lists() {
    let home = Path::new("/home/u");
    let repo = Path::new("/repo");
    assert_eq!(
        ghostex_editor_executable_candidates_for_platform("darwin", Some(repo), home, None),
        vec![
            "/home/u/Applications/GhostexEditor.app/Contents/MacOS/GhostexEditor",
            "/Applications/GhostexEditor.app/Contents/MacOS/GhostexEditor",
            "/repo/editor/dist/GhostexEditor.app/Contents/MacOS/GhostexEditor",
        ]
    );
    assert_eq!(
        ghostex_editor_executable_candidates_for_platform("darwin", None, home, None).len(),
        2
    );
    assert_eq!(
        ghostex_editor_executable_candidates_for_platform("linux", Some(repo), home, None),
        vec![
            "/home/u/.local/bin/ghostex-editor",
            "/usr/local/bin/ghostex-editor",
            "/repo/editor/dist/desktop/ghostex-editor",
        ]
    );
}

#[test]
fn app_bundle_candidate_appends_binary_path() {
    assert_eq!(
        ghostex_editor_executable_candidate("/Applications/GhostexEditor.app"),
        Some("/Applications/GhostexEditor.app/Contents/MacOS/GhostexEditor".to_string())
    );
    assert_eq!(
        ghostex_editor_executable_candidate("/opt/bin/ghostex-editor"),
        Some("/opt/bin/ghostex-editor".to_string())
    );
    assert_eq!(ghostex_editor_executable_candidate("   "), None);
    assert_eq!(ghostex_editor_executable_candidate(""), None);
}

#[test]
fn wrapper_script_matches_node_template() {
    let script = floating_editor_bridge_parity::floating_editor_wrapper_script(
        &["vim".to_string(), "a b".to_string()],
        "/work dir",
        "/w/status",
        "/dev/null",
    );
    assert!(script.starts_with("#!/bin/zsh\nset +e\n"));
    assert!(script.contains("mkdir -p '/w' '/dev' 2>/dev/null\n"));
    assert!(script.contains("printf 'started\\n' > '/w/status'\n"));
    assert!(script.contains("cd '/work dir' || {\n"));
    assert!(script.contains("\n'vim' 'a b'\n_ghostex_status=$?\n"));
    assert!(script.contains("printf 'exit:%s\\n' \"$_ghostex_status\" >> '/w/status'\n"));
    assert!(script.ends_with("exit \"$_ghostex_status\"\n"));
}

#[test]
fn base36_and_random_ids() {
    assert_eq!(to_base36(0), "0");
    assert_eq!(to_base36(35), "z");
    assert_eq!(to_base36(36), "10");
    let id = random_base36(6);
    assert_eq!(id.len(), 6);
    assert!(id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
}

#[test]
fn js_string_coercions() {
    assert_eq!(js_string_or_undefined(None), "undefined");
    assert_eq!(js_string_or_undefined(Some(&Value::Null)), "null");
    assert_eq!(js_string_or_undefined(Some(&json!("ok"))), "ok");
    assert_eq!(js_string_or_undefined(Some(&json!(3))), "3");
    assert_eq!(js_string_or_empty(None), "");
    assert_eq!(js_string_or_empty(Some(&Value::Null)), "");
    assert_eq!(js_string_or_empty(Some(&json!(true))), "true");
}

#[test]
fn the_handshake_folder_is_the_one_the_session_names() {
    let own = || PathBuf::from("/home/u/.other-state/ghostex");
    let named = env_map(&[(
        "GHOSTEX_PROMPT_HANDSHAKE_DIR",
        "/home/u/.local/state/ghostex",
    )]);
    assert_eq!(
        prompt_handshake_dir_with(&|key| named.get(key).cloned(), own),
        PathBuf::from("/home/u/.local/state/ghostex")
    );
    // A session started before gxserver named the folder keeps resolving its own, as before.
    let older = env_map(&[]);
    assert_eq!(
        prompt_handshake_dir_with(&|key| older.get(key).cloned(), own),
        own()
    );
    for unusable in ["", "relative/state"] {
        let env = env_map(&[("GHOSTEX_PROMPT_HANDSHAKE_DIR", unusable)]);
        assert_eq!(
            prompt_handshake_dir_with(&|key| env.get(key).cloned(), own),
            own()
        );
    }
}

/// GitHub #400: a session shell whose profile sets another XDG_STATE_HOME still finds the marker
/// gxserver wrote, because the session's setup names gxserver's folder after the profile ran.
#[cfg(unix)]
#[test]
fn a_session_with_another_xdg_state_home_finds_gxservers_marker() {
    let home = tempfile::tempdir().unwrap();
    let shell_state = home.path().join("elsewhere-state");
    let setup = crate::zmx::zmx_provider_prompt_editor_setup_shell_command(None);
    let output = Command::new("/bin/sh")
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_STATE_HOME", &shell_state)
        .arg("-c")
        .arg(format!("{setup}\nenv"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let session_env: HashMap<String, String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    // What the session's own environment would resolve, which is not where gxserver writes.
    let shell_own = shell_state.join("ghostex");
    let gxserver_dir = crate::zmx::prompt_handshake_dir();
    assert_ne!(shell_own, gxserver_dir);
    let resolved =
        prompt_handshake_dir_with(&|key| session_env.get(key).cloned(), || shell_own.clone());
    assert_eq!(resolved, gxserver_dir);
    assert_eq!(
        resolved.join("prompt-stash-requests").join("P1abc-G1abc"),
        gxserver_dir
            .join("prompt-stash-requests")
            .join("P1abc-G1abc")
    );
    // The wrapper itself still lands where the shell's own environment puts it.
    assert!(shell_own.join("prompt-editor").is_file());
}
