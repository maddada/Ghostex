//! `ghostex orchestrator guide`, `playbook` and `set-playbook`: the orchestrator playbook in
//! effect for a project, the user's playbook overrides, and the edited full copies saved before
//! overrides. See CDXC:Coordinators 2026-11-07 in server/src/coordinators/playbook.rs.

use serde_json::{json, Value};

use super::command::{flag_text, server_flags, text_or_file};
use crate::ghostex_cli::{
    agents,
    args::{Flags, ParsedArgs},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The project a command names (`--project-id`, or the calling session's with `--this-project`;
/// `implicit` also takes the calling session's without the flag), with the flags that reach the
/// machine it lives on, and the calling session when it was used.
fn target_project(
    parsed: &ParsedArgs,
    implicit: bool,
) -> CliResult<(Option<String>, Flags, Option<Value>)> {
    let base = server_flags(&parsed.flags);
    if let Some(project_id) = flag_text(&parsed.flags, "projectId") {
        return Ok((Some(project_id), base, None));
    }
    let wants_own = parsed.flags.truthy("thisProject");
    if !wants_own && !implicit {
        return Ok((None, base, None));
    }
    let caller = match agents::caller() {
        Ok(caller) => caller,
        Err(_) if !wants_own => return Ok((None, base, None)),
        Err(error) => {
            return Err(CliError::Other(format!(
                "{error} Outside an agent session, pass --project-id <id> (ghostex sessions --json lists project ids)."
            )))
        }
    };
    let project_id = agents::text(&caller, "projectId").to_string();
    let flags = agents::inventory_flags(&base, agents::text(&caller, "globalRef"))?;
    Ok((
        Some(project_id).filter(|id| !id.is_empty()),
        flags,
        Some(caller),
    ))
}

/// `guide [--project-id <id>]`: the playbook in effect for the orchestrator's project and the reply
/// format. Run inside an orchestrator, gxserver notes that it read this version, so `status`
/// can say when it changes.
pub(super) fn guide(parsed: &ParsedArgs) -> CliResult<()> {
    let (project_id, flags, caller) = target_project(parsed, true)?;
    let mut params = json!({});
    if let Some(project_id) = &project_id {
        params["projectId"] = json!(project_id);
    }
    if let Some(caller) = &caller {
        params["sessionId"] = json!(agents::text(caller, "sessionId"));
    }
    let result = call_gxserver_rpc("/api/readCoordinatorGuide", &params, &flags)?;
    print!("{}", agents::text(&result, "guide"));
    Ok(())
}

/// The old full copies a view lists (`oldCopies`), one line each.
fn old_copy_lines(view: &Value) -> Vec<String> {
    view["oldCopies"]
        .as_array()
        .map(|copies| {
            copies
                .iter()
                .map(|copy| {
                    let place = if agents::text(copy, "scope") == "allProjects" {
                        "all projects".to_string()
                    } else {
                        let name = agents::text(copy, "name");
                        if name.is_empty() {
                            agents::text(copy, "projectId").to_string()
                        } else {
                            name.to_string()
                        }
                    };
                    let state = if copy["enabled"] == false {
                        "off"
                    } else {
                        "in use"
                    };
                    format!("  An edited full copy for {place} ({state})")
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `playbook [--project-id <id> | --this-project] [--overrides | --text | --default] [--json]`,
/// `playbook --set-overrides (<text> | --body-file <path>)` and `playbook --clear-overrides`.
pub(super) fn show(parsed: &ParsedArgs) -> CliResult<()> {
    if parsed.flags.contains("setOverrides") || parsed.flags.truthy("clearOverrides") {
        return set_overrides(parsed);
    }
    let (project_id, flags, _) = target_project(parsed, true)?;
    let mut params = json!({});
    if let Some(project_id) = &project_id {
        params["projectId"] = json!(project_id);
    }
    let view = call_gxserver_rpc("/api/readCoordinatorPlaybook", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&view);
        return Ok(());
    }
    let overrides = agents::text(&view["overrides"], "text");
    if parsed.flags.truthy("overrides") {
        println!("{overrides}");
        return Ok(());
    }
    let scope = if view["project"].is_object() && project_id.is_some() {
        &view["project"]
    } else {
        &view["allProjects"]
    };
    if parsed.flags.truthy("default") {
        println!("{}", agents::text(&view["allProjects"], "defaultText"));
        return Ok(());
    }
    if parsed.flags.truthy("text") {
        let text = if scope["edited"] == true && scope["enabled"] != false {
            agents::text(scope, "text")
        } else {
            agents::text(scope, "defaultText")
        };
        println!("{text}");
        return Ok(());
    }
    let source = match agents::text(&view, "source") {
        "project" => format!(
            "an edited full copy for {} saved before overrides",
            agents::text(&view["project"], "name")
        ),
        "allProjects" => "an edited full copy for all projects saved before overrides".to_string(),
        _ => "Ghostex's playbook, with its updates".to_string(),
    };
    println!("Coordinators here read {source}.");
    if overrides.trim().is_empty() {
        println!("Overrides: none. --set-overrides \"<text>\" adds them.");
    } else {
        let lines = overrides
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        println!(
            "Overrides: {lines} line{} after the playbook, taking precedence over it (--overrides prints them).",
            if lines == 1 { "" } else { "s" }
        );
    }
    let old = old_copy_lines(&view);
    if !old.is_empty() {
        println!("Saved before overrides, these copies don't get Ghostex's updates:");
        for line in old {
            println!("{line}");
        }
        println!("`ghostex coordinator set-playbook --use-ghostex` saves them as Markdown files and moves every coordinator to Ghostex's playbook.");
    }
    Ok(())
}

/// `playbook --set-overrides (<text> | --body-file <path>)`, `playbook --clear-overrides`.
fn set_overrides(parsed: &ParsedArgs) -> CliResult<()> {
    let text = if parsed.flags.truthy("clearOverrides") {
        String::new()
    } else {
        match text_or_file(parsed, "setOverrides")? {
            Some(text) => text,
            None => parsed.rest.join(" "),
        }
    };
    if text.trim().is_empty() && !parsed.flags.truthy("clearOverrides") {
        return Err(CliError::Other(
            "Usage: ghostex coordinator playbook --set-overrides (\"<text>\" | --body-file <path>), or --clear-overrides to remove them.".into(),
        ));
    }
    // The machine the calling session runs on, like `guide`: its coordinators read them.
    let (_, flags, _) = target_project(parsed, true)?;
    let view = call_gxserver_rpc(
        "/api/setCoordinatorPlaybook",
        &json!({ "overrides": text }),
        &flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&view);
        return Ok(());
    }
    if agents::text(&view["overrides"], "text").trim().is_empty() {
        println!("No playbook overrides now: coordinators follow the playbook alone.");
    } else {
        println!("Saved the playbook overrides. Every coordinator reads them after the playbook, ranked above it.");
    }
    println!("Coordinators, existing ones too, read it at their next `ghostex coordinator guide`; the next message each one gets tells it the playbook changed.");
    Ok(())
}

/// `set-playbook (--use-ghostex | --reset | --off | --on) [--project-id <id> | --this-project]
/// [--json]`: the edited full copies saved before overrides. `--use-ghostex` saves every copy as a
/// Markdown file and removes them all; the others act on one copy.
pub(super) fn set(parsed: &ParsedArgs) -> CliResult<()> {
    let mut params = json!({});
    if parsed.flags.truthy("useGhostex") {
        params["retireOldCopies"] = json!(true);
    } else if parsed.flags.truthy("reset") {
        params["reset"] = json!(true);
    } else if parsed.flags.truthy("off") || parsed.flags.truthy("on") {
        params["enabled"] = json!(parsed.flags.truthy("on"));
    } else {
        return Err(CliError::Other(
            "Ghostex's playbook is no longer edited, so its updates keep arriving: write overrides instead with `ghostex coordinator playbook --set-overrides \"<text>\"`. set-playbook only handles copies saved before overrides: --use-ghostex, or --reset, --off, --on with [--project-id <id> | --this-project].".into(),
        ));
    }
    let (project_id, flags, _) = target_project(parsed, false)?;
    if let Some(project_id) = &project_id {
        params["projectId"] = json!(project_id);
    }
    let view = call_gxserver_rpc("/api/setCoordinatorPlaybook", &params, &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&view);
        return Ok(());
    }
    if let Some(saved) = view["savedFiles"].as_array() {
        if saved.is_empty() {
            println!(
                "No copies saved before overrides: coordinators already read Ghostex's playbook."
            );
        } else {
            println!(
                "Every coordinator reads Ghostex's playbook now. The old copies are saved here:"
            );
            for path in saved.iter().filter_map(Value::as_str) {
                println!("  {path}");
            }
        }
    } else {
        let old = old_copy_lines(&view);
        if old.is_empty() {
            println!(
                "No copies saved before overrides are left: coordinators read Ghostex's playbook."
            );
        } else {
            println!("Copies saved before overrides:");
            for line in old {
                println!("{line}");
            }
        }
    }
    println!("Coordinators, existing ones too, read it at their next `ghostex coordinator guide`; the next message each one gets tells it the playbook changed.");
    Ok(())
}
