//! `ghostex orchestrator guide`, `playbook` and `set-playbook`: the orchestrator playbook in
//! effect for a project, and the edited copies for all projects or one project (and its
//! worktrees). See CDXC:Coordinators 2026-10-19 in server/src/coordinators/playbook.rs.

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

fn scope_line(scope: &Value, label: &str, inherited: &str) -> String {
    if scope["edited"] != true {
        return format!("{label}: {inherited}");
    }
    let mut line = format!(
        "{label}: edited, {}",
        if scope["enabled"] == false {
            "off"
        } else {
            "on"
        }
    );
    let saved = agents::text(scope, "updatedAt");
    if let Some(date) = saved.get(..10) {
        line.push_str(&format!(", saved {date}"));
    }
    if scope["defaultChanged"] == true {
        line.push_str("; the playbook it was edited from changed since (see --default)");
    }
    line
}

/// `playbook [--project-id <id> | --this-project] [--text | --default] [--json]`.
pub(super) fn show(parsed: &ParsedArgs) -> CliResult<()> {
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
    let scope = if view["project"].is_object() && project_id.is_some() {
        &view["project"]
    } else {
        &view["allProjects"]
    };
    if parsed.flags.truthy("default") {
        println!("{}", agents::text(scope, "defaultText"));
        return Ok(());
    }
    if parsed.flags.truthy("text") {
        let text = if scope["edited"] == true {
            agents::text(scope, "text")
        } else {
            agents::text(scope, "defaultText")
        };
        println!("{text}");
        return Ok(());
    }
    println!(
        "{}",
        scope_line(
            &view["allProjects"],
            "For all projects",
            "Ghostex's playbook"
        )
    );
    let project = &view["project"];
    if project.is_object() {
        let name = agents::text(project, "name");
        let label = if project["isWorktree"] == true {
            format!("For {name} (also covers this worktree)")
        } else {
            format!("For {name} and its worktrees")
        };
        println!(
            "{}",
            scope_line(project, &label, "uses the playbook for all projects")
        );
        let source = match agents::text(&view, "source") {
            "project" => format!("{name}'s edited playbook"),
            "allProjects" => "the edited playbook for all projects".to_string(),
            _ => "Ghostex's playbook".to_string(),
        };
        println!("Coordinators here read {source}.");
    }
    println!("--text prints it, --default prints what Reset to Default goes back to.");
    Ok(())
}

/// `set-playbook (<text> | --body-file <path> | --reset | --off | --on) [--project-id <id> |
/// --this-project] [--json]`.
pub(super) fn set(parsed: &ParsedArgs) -> CliResult<()> {
    let mut params = json!({});
    if parsed.flags.truthy("reset") {
        params["reset"] = json!(true);
    } else if parsed.flags.truthy("off") || parsed.flags.truthy("on") {
        params["enabled"] = json!(parsed.flags.truthy("on"));
    } else {
        let text = match text_or_file(parsed, "text")? {
            Some(text) => text,
            None => parsed.rest.join(" "),
        };
        if text.trim().is_empty() {
            return Err(CliError::Other(
                "Usage: ghostex coordinator set-playbook (\"<text>\" | --body-file <path> | --reset | --off | --on) [--project-id <id> | --this-project]. Put flags after the text.".into(),
            ));
        }
        params["text"] = json!(text);
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
    let (scope, place) = match project_id {
        Some(_) => (
            &view["project"],
            format!(
                "{} and its worktrees",
                agents::text(&view["project"], "name")
            ),
        ),
        None => (&view["allProjects"], "all projects".to_string()),
    };
    if scope["edited"] != true {
        println!("The playbook for {place} is the default one now (no edited copy is stored).");
    } else if scope["enabled"] == false {
        println!(
            "The edited playbook for {place} is kept but off; coordinators read the default one."
        );
    } else {
        println!("The edited playbook for {place} is on.");
    }
    println!("Coordinators, existing ones too, read it at their next `ghostex coordinator guide`; `status` tells them it changed.");
    Ok(())
}
