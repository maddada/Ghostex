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

/// `guide [--project-id <id>] [--changes]`: the playbook in effect for the orchestrator's project
/// and the reply format. Run inside an orchestrator, gxserver notes that it read this version, so
/// `status` can say when it changes; `--changes` prints only what changed since its last read
/// when gxserver still has that text (coordinators/guide_changes.rs), and the full guide otherwise.
pub(super) fn guide(parsed: &ParsedArgs) -> CliResult<()> {
    let (project_id, flags, caller) = target_project(parsed, true)?;
    let mut params = json!({ "changes": parsed.flags.truthy("changes") });
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
    if parsed.flags.contains("addOverride")
        || parsed.flags.contains("replaceOverride")
        || parsed.flags.contains("removeOverride")
        || parsed.flags.truthy("listOverrides")
    {
        return edit_overrides(parsed);
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
    let (_, flags, caller) = target_project(parsed, true)?;
    let view = save_overrides(&text, caller.as_ref(), &flags)?;
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

/// Saves the whole overrides text. A request from an agent session names that session: gxserver
/// lets a session change the playbook only as itself (server/security_gates.rs
/// `coordinator_refusal`), and before this the CLI named none, so every agent was refused.
fn save_overrides(text: &str, caller: Option<&Value>, flags: &Flags) -> CliResult<Value> {
    let mut params = json!({ "overrides": text });
    if let Some(caller) = caller {
        params["projectId"] = json!(agents::text(caller, "projectId"));
        params["sessionId"] = json!(agents::text(caller, "sessionId"));
    }
    call_gxserver_rpc("/api/setCoordinatorPlaybook", &params, flags)
}

/// The overrides as numbered entries: each paragraph (lines with no blank line between them) is
/// one rule.
fn override_entries(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// `playbook --list-overrides`, `--add-override "<rule>"`, `--replace-override <n> "<rule>"` and
/// `--remove-override <n>`: one rule at a time, so an agent never rewrites the user's own text.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// User: "we need agents to be able to modify the overrides for coordinators please. right now
/// it's manual only". `--set-overrides` replaces the whole text, which an agent would use to
/// clobber what the user wrote, so agents get numbered edits: list, add one rule to the end,
/// replace or remove one by its number. Each saves the whole text through the same endpoint, so
/// the 20,000 character limit and the note that tells every coordinator its playbook changed
/// apply as they do to the Settings editor.
fn edit_overrides(parsed: &ParsedArgs) -> CliResult<()> {
    let (_, flags, caller) = target_project(parsed, true)?;
    let view = call_gxserver_rpc("/api/readCoordinatorPlaybook", &json!({}), &flags)?;
    let mut entries = override_entries(agents::text(&view["overrides"], "text"));
    let number = |flag: &str| -> CliResult<usize> {
        let words = flag_text(&parsed.flags, flag).unwrap_or_default();
        let number: usize = words.trim().trim_start_matches('#').parse().map_err(|_| {
            CliError::Other(format!(
                "Name the override by its number (ghostex coordinator playbook --list-overrides shows them), not \"{words}\"."
            ))
        })?;
        if number == 0 || number > entries.len() {
            return Err(CliError::Other(format!(
                "There is no override {number}: there {} {} (ghostex coordinator playbook --list-overrides).",
                if entries.len() == 1 { "is" } else { "are" },
                entries.len()
            )));
        }
        Ok(number)
    };
    let rule = || -> CliResult<String> {
        // `--body-file` wins; else `--add-override`'s own text; else the words after the number.
        let rule = if flag_text(&parsed.flags, "bodyFile").is_some() {
            text_or_file(parsed, "addOverride")?.unwrap_or_default()
        } else if parsed.flags.contains("addOverride") {
            flag_text(&parsed.flags, "addOverride").unwrap_or_else(|| parsed.rest.join(" "))
        } else {
            parsed.rest.join(" ")
        };
        // One rule is one paragraph: a blank line inside it would split it in two.
        let rule = rule
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if rule.is_empty() {
            return Err(CliError::Other(
                "Say the rule: --add-override \"<rule>\", or --replace-override <n> \"<rule>\" (--body-file <path> for a longer one).".into(),
            ));
        }
        Ok(rule)
    };
    let done = if parsed.flags.contains("addOverride") {
        entries.push(rule()?);
        format!("Added override {}.", entries.len())
    } else if parsed.flags.contains("replaceOverride") {
        let at = number("replaceOverride")?;
        entries[at - 1] = rule()?;
        format!("Replaced override {at}.")
    } else if parsed.flags.contains("removeOverride") {
        let at = number("removeOverride")?;
        entries.remove(at - 1);
        format!("Removed override {at}; the ones after it moved up a number.")
    } else {
        if entries.is_empty() {
            println!("No playbook overrides. --add-override \"<rule>\" adds one.");
        }
        for (index, entry) in entries.iter().enumerate() {
            println!("{}. {}", index + 1, entry.replace('\n', "\n   "));
        }
        return Ok(());
    };
    let text = entries.join("\n\n");
    let length = text.chars().count();
    let limit = crate::coordinators::COORDINATOR_PLAYBOOK_OVERRIDES_MAX_CHARS;
    if length > limit {
        return Err(CliError::Other(format!(
            "Not saved: the overrides would be {length} characters, over the {limit} character limit. Shorten the rule, or remove one first (--remove-override <n>)."
        )));
    }
    let view = save_overrides(&text, caller.as_ref(), &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&view);
        return Ok(());
    }
    println!("{done} Every coordinator reads the overrides after the playbook, ranked above it; each one is told its playbook changed with its next message.");
    Ok(())
}
