//! `ghostex app`: Ghostex Apps for agents. `show` publishes an app (or its next version), `list`
//! lists them, `guide` prints the agent guide. While Ghostex Apps is off every form prints one line
//! saying so and where to turn it on, and nothing else.
//!
//! CDXC:GhostexApps 2026-11-25 DECISION:
//! User: when the extension is off "nothing of it shows anywhere (no menu rows, no agent instructions, no CLI help beyond a hint)". So `ghostex app`, `ghostex app --help` and every subcommand answer only the turned-off hint while it is off, the verb is left out of `ghostex --help`, and the launch note has no Apps line (server/src/ghostex_apps/feature.rs `agent_note`).

use std::path::Path;

use base64::Engine as _;
use ghostex_settings_catalog::built_in_extensions::GHOSTEX_APPS;
use serde_json::{json, Value};

use super::args::parse_args;
use super::built_in_extensions::{built_in_extension_enabled, require_built_in_extension};
use super::rpc::{
    call_gxserver_rpc, request_gxserver_rpc, resolve_gxserver_server_target, CliError, CliResult,
};
use super::show::{absolute_path, calling_session_ref, json_string, read_source, source_key};
use crate::ghostex_apps::{icon, manifest};

const GUIDE: &str = include_str!("ghostex_app_guide.md");

const USAGE: &str = "Usage: ghostex app <command>

Ghostex Apps (Beta): HTML apps the user keeps, opened like a Live page and
installable as their own app.

  ghostex app show <app.html> [--title <name>] [--json]
                     Publish the app, or its next version when the same file
                     was published before, and print the block for your reply
  ghostex app list [--json]
                     The apps on this computer, their versions and powers
  ghostex app guide  How to build one: the head tags, the icon, ghostex.app";

pub fn app_command(args: &[String]) -> CliResult<()> {
    if !built_in_extension_enabled(GHOSTEX_APPS) {
        // One line, whatever was asked (the DECISION above).
        return require_built_in_extension(GHOSTEX_APPS);
    }
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let rest = args.get(1..).unwrap_or_default();
    match sub {
        "show" => show(rest),
        "list" | "ls" => list(rest),
        "guide" => {
            println!("{}", GUIDE.trim_end());
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown command \"ghostex app {other}\".\n\n{USAGE}"
        ))),
    }
}

/// `ghostex app show <app.html>`.
fn show(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    if parsed.flags.contains("help") || parsed.flags.contains("h") {
        println!("{USAGE}");
        return Ok(());
    }
    let Some(file) = parsed.rest.first() else {
        return Err(CliError::Other(
            "app show needs an HTML file: ghostex app show <app.html> [--title <name>] [--json]"
                .to_string(),
        ));
    };
    let path = absolute_path(file);
    let source = read_source(&path)?;
    let base_dir = path.parent().unwrap_or(Path::new("/"));
    // Checked before publishing so the agent hears every problem from its own file.
    manifest::declared_powers(&source).map_err(CliError::Other)?;
    let (kind, icon_bytes) = read_icon(&source, base_dir)?;
    if source.contains("<gx-questions") || source.contains("ghostex.submit(") {
        return Err(CliError::Other(
            "An app has no questions: take out <gx-questions> and ghostex.submit, and send to the chat with ghostex.app.send(text) instead (ghostex app guide). For questions, publish a Live page with ghostex show.".to_string(),
        ));
    }
    let prepared = crate::visual_pages::prepare_page(&source, base_dir).map_err(CliError::Other)?;
    let name = parsed
        .flags
        .string_value("title")
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .or(prepared.title)
        .ok_or_else(|| {
            CliError::Other(
                "Give the app a name in <title> (or --title \"<name>\"), such as \"Morning brief\"."
                    .to_string(),
            )
        })?;
    let mut params = json!({
        "name": name,
        "html": prepared.html,
        "sourcePath": path.to_string_lossy(),
        "icon": {
            "kind": kind.extension(),
            "base64": base64::engine::general_purpose::STANDARD.encode(&icon_bytes),
        },
    });
    if let Some(session_ref) = calling_session_ref() {
        params["sessionRef"] = Value::String(session_ref);
    }
    let target = resolve_gxserver_server_target(&parsed.flags, &params)?;
    let result = request_gxserver_rpc(
        &target,
        crate::ghostex_apps::PUBLISH_GHOSTEX_APP_ENDPOINT,
        &params,
        &parsed.flags,
    )?;
    let text = |key: &str| result.get(key).and_then(Value::as_str).unwrap_or_default();
    let (id, app_path) = (text("id"), text("path"));
    if id.is_empty() || app_path.is_empty() {
        return Err(CliError::Other(
            "gxserver did not return the app's address. Update Ghostex and try again.".to_string(),
        ));
    }
    let name = Some(text("name"))
        .filter(|name| !name.is_empty())
        .unwrap_or(&name);
    let version = result.get("version").and_then(Value::as_u64).unwrap_or(1);
    let url = format!("{}{app_path}", target.base_url.trim_end_matches('/'));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let block = app_block(name, &url, &file_name, &source_key(&path), id);
    let new_powers: Vec<String> = result
        .get("newPermissions")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if parsed.flags.truthy("json") {
        println!(
            "{}",
            json!({"ok": true, "id": id, "name": name, "version": version, "url": url,
                   "block": block, "newPermissions": new_powers})
        );
        return Ok(());
    }
    let what = if version == 1 {
        format!("Published the app \"{name}\".")
    } else {
        format!("Published version {version} of the app \"{name}\"; it opens at this version from now on.")
    };
    let asks = if new_powers.is_empty() {
        String::new()
    } else {
        format!(
            " This version declares new permissions ({}); the user is asked for them.",
            new_powers.join(", ")
        )
    };
    println!(
        "{what}{asks}\nPut this block in your reply where the app should appear (the chat shows it as a card that opens the app):\n\n{block}"
    );
    Ok(())
}

/// The icon the app's `<link rel="icon">` names, read from beside the HTML and checked.
fn read_icon(source: &str, base_dir: &Path) -> CliResult<(icon::IconKind, Vec<u8>)> {
    let how = |problem: &str| CliError::Other(format!("{problem} {}", icon::ICON_HOW_TO));
    let href = manifest::icon_href(source).ok_or_else(|| how("The app has no icon."))?;
    if href.contains(':') || href.starts_with("//") {
        return Err(how(&format!(
            "The icon \"{href}\" is not a file beside the app."
        )));
    }
    let relative = href.split(['?', '#']).next().unwrap_or_default();
    let file = base_dir.join(relative);
    let kind = file
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(icon::IconKind::from_extension)
        .ok_or_else(|| how(&format!("The icon \"{href}\" is not an .svg or .png file.")))?;
    let bytes = std::fs::read(&file)
        .map_err(|_| how(&format!("Could not read the icon {}.", file.display())))?;
    icon::check_icon(kind, &bytes).map_err(|problem| how(&problem))?;
    Ok((kind, bytes))
}

/// The chat block of an app: a Live page block with `"app"`, so every client that can open a page
/// can open it (an older phone or app shows a plain page card), and the desktop knows to open it
/// as an app.
///
/// CDXC:GhostexApps 2026-11-25 SEE-ALSO:
/// The block is the page block of server/src/ghostex_cli/show.rs `visual_page_block` (parsed by packages/gx-visual/src/spec.rs) with one more field; the desktop's app opening reads `"app"`.
fn app_block(name: &str, url: &str, file: &str, source: &str, id: &str) -> String {
    format!(
        "```visual\n{{\"page\":{{\"title\":{},\"url\":{},\"file\":{},\"source\":{},\"open\":\"popup\",\"app\":{}}}}}\n```",
        json_string(name),
        json_string(url),
        json_string(file),
        json_string(source),
        json_string(id)
    )
}

/// `ghostex app list`.
fn list(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let result = call_gxserver_rpc(
        crate::ghostex_apps::LIST_GHOSTEX_APPS_ENDPOINT,
        &json!({}),
        &parsed.flags,
    )?;
    if parsed.flags.truthy("json") {
        println!("{result}");
        return Ok(());
    }
    let apps = result
        .get("apps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if apps.is_empty() {
        println!("No apps yet. Build one with `ghostex app guide`.");
        return Ok(());
    }
    for app in apps {
        let text = |key: &str| {
            app.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let powers = app
            .get("permissions")
            .and_then(Value::as_object)
            .map(|powers| {
                powers
                    .iter()
                    .map(|(name, state)| format!("{name}: {}", state.as_str().unwrap_or("ask")))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|powers| !powers.is_empty())
            .unwrap_or_else(|| "like a web page".to_string());
        println!(
            "{}  v{}  {}  updated {}  ({powers})",
            text("name"),
            app.get("version").and_then(Value::as_u64).unwrap_or(0),
            text("id"),
            text("updatedAt"),
        );
    }
    Ok(())
}
