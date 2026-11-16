//! `ghostex settings export <file>` and `ghostex settings import <file>`: the Settings > Sync
//! export and import for agents and computers without the window. The rules (what the file
//! carries, what an import changes or skips) are gx-core's `settings_transfer`
//! (CDXC:SettingsSync 2026-11-14 there); an import saves through the same `updateSettingsPatch`
//! path as `ghostex settings set`, so it applies live, or is saved by gxserver on a computer
//! without the desktop app.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ghostex_gx_core::settings_transfer::{
    describe_transfer_value, export_file_name, export_settings_document, plan_settings_import,
    ExportSource, ImportPlan,
};
use serde_json::{json, Map, Value};

use crate::ghostex_cli::actions::send_gxserver_cli_action;
use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::settings::{read_settings_file, values_equal, SETTINGS_UPDATE_SOURCE};

const IMPORT_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);
const IMPORT_POLL_INTERVAL: Duration = Duration::from_millis(100);

fn file_argument(rest: &[String], verb: &str) -> CliResult<PathBuf> {
    rest.first()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::Other(format!(
                "ghostex settings {verb} needs a file, like `ghostex settings {verb} {}`.",
                export_file_name("my-computer", "2026-11-14")
            ))
        })
}

pub(super) fn export_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let path = file_argument(&parsed.rest, "export")?;
    let settings = read_settings_file()?;
    let computer = crate::connected_computers::this_computer_name();
    let exported_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let document = export_settings_document(
        &settings,
        &ExportSource {
            ghostex_version: crate::constants::GXSERVER_VERSION,
            computer_name: &computer,
            exported_at: &exported_at,
        },
    );
    let count = document["settings"].as_object().map_or(0, Map::len);
    let text = serde_json::to_string_pretty(&document)
        .map_err(|error| CliError::Other(format!("Could not write the settings: {error}")))?;
    std::fs::write(&path, format!("{text}\n"))
        .map_err(|error| CliError::Other(format!("Could not save {}: {error}", path.display())))?;
    if parsed.flags.truthy("json") {
        print_json(&json!({ "ok": true, "path": path, "settings": count }));
    } else {
        println!(
            "Saved {count} settings to {}. Secrets, sign-ins and settings that belong to this computer are left out.",
            path.display()
        );
    }
    Ok(())
}

pub(super) fn import_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let path = file_argument(&parsed.rest, "import")?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| CliError::Other(format!("Could not read {}: {error}", path.display())))?;
    let settings = read_settings_file()?;
    let mut plan = plan_settings_import(&text, &settings)
        .map_err(|error| CliError::Other(error.message().to_string()))?;
    // The API's own rule once more, so the import can never send what `settings set` refuses.
    let refused = crate::settings_agent_writable::refused_keys(&plan.patch());
    if !refused.is_empty() {
        plan.changes.retain(|change| !refused.contains(&change.key));
        plan.left_out.extend(refused);
    }
    let dry_run = parsed.flags.truthy("dryRun");
    if !dry_run && !plan.changes.is_empty() {
        save_patch(&plan.patch(), &parsed.flags)?;
    }
    report(&plan, dry_run, parsed.flags.truthy("json"));
    Ok(())
}

/// Sends the patch in pieces the settings route takes, then waits until the settings file shows
/// every value.
fn save_patch(patch: &Map<String, Value>, flags: &Flags) -> CliResult<()> {
    let entries: Vec<(&String, &Value)> = patch.iter().collect();
    for chunk in entries.chunks(ghostex_gx_core::SETTINGS_PATCH_MAX_KEYS) {
        let piece: Map<String, Value> = chunk
            .iter()
            .map(|(key, value)| ((*key).clone(), (*value).clone()))
            .collect();
        send_gxserver_cli_action(
            "updateSettingsPatch",
            &json!({ "patch": piece, "source": SETTINGS_UPDATE_SOURCE }),
            flags,
        )?;
    }
    let started = Instant::now();
    loop {
        let saved = read_settings_file()?;
        let missing = patch
            .iter()
            .filter(|(key, value)| {
                !saved
                    .get(key.as_str())
                    .is_some_and(|saved| values_equal(saved, value))
            })
            .count();
        if missing == 0 {
            return Ok(());
        }
        if started.elapsed() >= IMPORT_CONFIRM_TIMEOUT {
            return Err(CliError::Other(format!(
                "Ghostex accepted the import, but {missing} of {} settings did not show the new value within {} seconds. Check Settings.",
                patch.len(),
                IMPORT_CONFIRM_TIMEOUT.as_secs()
            )));
        }
        std::thread::sleep(IMPORT_POLL_INTERVAL);
    }
}

fn report(plan: &ImportPlan, dry_run: bool, as_json: bool) {
    if as_json {
        let changes: Vec<Value> = plan
            .changes
            .iter()
            .map(|change| {
                json!({ "key": change.key, "title": change.title, "from": change.from, "to": change.to })
            })
            .collect();
        print_json(&json!({
            "ok": true,
            "applied": !dry_run && !plan.changes.is_empty(),
            "changes": changes,
            "unchanged": plan.unchanged,
            "skipped": {
                "unknown": plan.unknown,
                "wrongType": plan.wrong_type,
                "leftOut": plan.left_out,
                "otherSystem": plan.other_system,
            },
            "from": {
                "computer": plan.from_computer,
                "ghostexVersion": plan.from_version,
                "platform": plan.from_platform,
            },
        }));
        return;
    }
    let count = plan.changes.len();
    let headline = match (count, dry_run) {
        (0, _) => "Nothing to change: this computer already has these settings.".to_string(),
        (1, true) => "Importing would change 1 setting:".to_string(),
        (_, true) => format!("Importing would change {count} settings:"),
        (1, false) => "Changed 1 setting:".to_string(),
        (_, false) => format!("Changed {count} settings:"),
    };
    println!("{headline}");
    for change in &plan.changes {
        println!(
            "  {} ({}): {} -> {}",
            change.title,
            change.key,
            describe_transfer_value(&change.key, change.from.as_ref()),
            describe_transfer_value(&change.key, Some(&change.to))
        );
    }
    for note in plan.skipped_notes() {
        println!("{note}");
    }
}
