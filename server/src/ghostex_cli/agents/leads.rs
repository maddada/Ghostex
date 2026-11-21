//! `ghostex agents link | unlink | links` and `create --lead`: lead links (gxserver's
//! `/api/leadLinks`, server/src/agents/lead_links/). Only `ghostex agents guide leads` and
//! `ghostex agents link --help` teach them (CDXC:LeadLinks 2026-11-20 in that module).

use super::{
    arguments::Arguments,
    identity::{self, text},
};
use crate::ghostex_cli::{
    args::Flags,
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector, sessions,
};
use serde_json::{json, Value};

/// The session a ref names on this computer (`self` is the calling session).
fn resolve(reference: &str, flags: &Flags) -> CliResult<Value> {
    if reference == "self" {
        return identity::caller().map_err(|_| {
            CliError::Other(
                "--lead self works only inside a Ghostex session. From your own terminal, name the lead's session ref.".into(),
            )
        });
    }
    let flags = identity::inventory_flags(flags, reference)?;
    // A full ref of this computer's own session reads as `--server local`.
    let elsewhere =
        flags.contains("computer") || flags.text("server").is_some_and(|server| server != "local");
    if elsewhere {
        return Err(CliError::Other(
            "Lead links work on this computer only; that ref names another computer's session."
                .into(),
        ));
    }
    let rows = sessions::fetch_session_list(&flags, false)?;
    let session = selector::resolve_one_listed_session(reference, &rows, &flags)?;
    if !identity::is_agent(&session) {
        return Err(CliError::Other(format!(
            "{reference} is not an agent session."
        )));
    }
    Ok(session)
}

fn local_flags(flags: &Flags) -> Flags {
    let mut flags = flags.clone();
    flags.0.remove("computer");
    flags.0.remove("server");
    flags
}

/// Links `linked` to `lead`. `owed`: the lead handed it a task with the link. `started_for_lead`:
/// `agents create --lead` just made it for the lead (a lean Codex worker, agents/codex_lean.rs).
fn link_to(
    linked: &Value,
    lead: &Value,
    owed: bool,
    started_for_lead: bool,
    flags: &Flags,
) -> CliResult<Value> {
    call_gxserver_rpc(
        crate::agents::lead_links::LEAD_LINKS_ENDPOINT,
        &json!({
            "op": "link",
            "projectId": linked["projectId"], "sessionId": linked["sessionId"],
            "leadProjectId": lead["projectId"], "leadSessionId": lead["sessionId"],
            "owed": owed,
            "startedForLead": started_for_lead,
        }),
        &local_flags(flags),
    )
}

/// `agents create --lead`, before anything is created: the lead `--lead` names, once gxserver
/// says a new session may be linked under it (a bad ref, a coordinator, a lead two levels down or
/// another session's lead is refused here, so nothing is created; Opus review of #701, finding 2).
/// A gxserver older than the `check` op only resolves the lead; its link still checks the rules.
pub(super) fn lead_for_new_session(lead_ref: &str, flags: &Flags) -> CliResult<Value> {
    let lead = resolve(lead_ref, flags)?;
    match call_gxserver_rpc(
        crate::agents::lead_links::LEAD_LINKS_ENDPOINT,
        &json!({ "op": "check", "leadProjectId": lead["projectId"], "leadSessionId": lead["sessionId"] }),
        &local_flags(flags),
    ) {
        Err(CliError::Rpc { message, .. }) if message.contains("Unknown lead link op") => Ok(lead),
        Err(error) => Err(error),
        Ok(_) => Ok(lead),
    }
}

/// Links a session `agents create --lead` just made to `lead` (from `lead_for_new_session`).
pub(super) fn link_new_session(
    linked: &Value,
    lead: &Value,
    owed: bool,
    flags: &Flags,
) -> CliResult<Value> {
    link_to(linked, lead, owed, true, flags)
}

pub(super) fn link(args: &Arguments) -> CliResult<Value> {
    let linked = resolve(&args.positional[0], &args.flags)?;
    let lead = resolve(args.lead.as_deref().unwrap_or("self"), &args.flags)?;
    let result = link_to(&linked, &lead, false, false, &args.flags)?;
    Ok(json!({
        "ok": true,
        "status": "linked",
        "session": identity::summary(&linked),
        "lead": result["leadGlobalRef"],
        "leadTitle": result["leadTitle"],
        "note": "Its final message reaches the lead after each turn the lead starts with a message (ghostex agents send).",
    }))
}

pub(super) fn unlink(args: &Arguments) -> CliResult<Value> {
    let flags = local_flags(&args.flags);
    if args.all {
        let lead = resolve(args.lead.as_deref().unwrap_or("self"), &args.flags).map_err(|_| {
            CliError::Other("unlink --all ends the links to the calling session; from your own terminal, name the lead with --lead <session-ref>.".into())
        })?;
        let result = call_gxserver_rpc(
            crate::agents::lead_links::LEAD_LINKS_ENDPOINT,
            &json!({ "op": "stopLeading", "leadProjectId": lead["projectId"], "leadSessionId": lead["sessionId"] }),
            &flags,
        )?;
        return Ok(json!({ "ok": true, "status": "unlinked", "count": result["unlinked"] }));
    }
    let linked = resolve(&args.positional[0], &args.flags)?;
    let result = call_gxserver_rpc(
        crate::agents::lead_links::LEAD_LINKS_ENDPOINT,
        &json!({ "op": "unlink", "projectId": linked["projectId"], "sessionId": linked["sessionId"] }),
        &flags,
    )?;
    Ok(json!({
        "ok": true,
        "status": "unlinked",
        "session": identity::summary(&linked),
        "lead": result["leadGlobalRef"],
    }))
}

pub(super) fn links(args: &Arguments) -> CliResult<Value> {
    let mut params = json!({ "op": "list" });
    if !args.all {
        if let Ok(caller) = identity::caller() {
            params["leadProjectId"] = caller["projectId"].clone();
            params["leadSessionId"] = caller["sessionId"].clone();
        }
    }
    let result = call_gxserver_rpc(
        crate::agents::lead_links::LEAD_LINKS_ENDPOINT,
        &params,
        &local_flags(&args.flags),
    )?;
    let mut answer = json!({ "ok": true, "links": result["links"] });
    if result["reportsTo"]
        .as_array()
        .is_some_and(|chain| !chain.is_empty())
    {
        answer["reportsTo"] = result["reportsTo"].clone();
    }
    Ok(answer)
}

fn one_line(value: &str) -> String {
    value.replace(['\n', '\r', '\t'], " ")
}

/// A chain of leads, top first, as "Main lead > Sub-lead" (`leadChain` rows).
fn chain_text(chain: &[Value]) -> Option<String> {
    let titles = chain.iter().map(entry_title).collect::<Vec<_>>();
    (!titles.is_empty()).then(|| titles.join(" > "))
}

fn entry_title(entry: &Value) -> String {
    match text(entry, "title") {
        "" => text(entry, "globalRef").to_string(),
        title => one_line(title),
    }
}

/// "S (under M)" for a chain of leads, top first: the direct lead, and the one above it.
fn reports_to_text(chain: &[Value]) -> Option<String> {
    let (direct, above) = chain.split_last()?;
    Some(match chain_text(above) {
        Some(above) => format!("{} (under {above})", entry_title(direct)),
        None => entry_title(direct),
    })
}

pub(super) fn print_links(result: &Value) {
    // CDXC:LeadLinks 2026-11-21 WHY: with two levels (#701) a lead's list holds its sub-leads'
    // sessions too. Each row is printed under its own lead, and a lead that is not a row of the
    // list (the top lead, or the sub-lead whose list it is) gets a row of its own above its
    // sessions (Opus review of #701, finding 4); the last column names the whole chain. A gxserver
    // older than two levels sends no chain, and the direct lead stands in.
    let reports_to = result["reportsTo"].as_array().cloned().unwrap_or_default();
    if let Some(lead) = reports_to_text(&reports_to) {
        println!("You report to: {lead}");
    }
    let rows = result["links"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        println!("No sessions are linked to a lead.");
        return;
    }
    println!("REFERENCE\tTITLE\tLEADS (TOP FIRST)\tREPORT OWED");
    let mut printed: Vec<(String, usize)> = Vec::new();
    for row in rows {
        let chain = row["leadChain"].as_array().cloned().unwrap_or_default();
        let lead_ref = text(&row, "leadGlobalRef").to_string();
        let lead_indent = match printed.iter().find(|(reference, _)| *reference == lead_ref) {
            Some((_, indent)) => *indent,
            None => {
                let above = chain
                    .split_last()
                    .map(|(_, above)| above)
                    .unwrap_or_default();
                println!(
                    "{lead_ref}\t{} (lead)\t{}\t-",
                    one_line(text(&row, "leadTitle")),
                    chain_text(above).unwrap_or_else(|| "-".to_string()),
                );
                printed.push((lead_ref.clone(), 0));
                0
            }
        };
        let indent = lead_indent + 1;
        printed.push((text(&row, "globalRef").to_string(), indent));
        println!(
            "{}\t{}{}{}\t{}\t{}",
            text(&row, "globalRef"),
            "  ".repeat(indent),
            one_line(text(&row, "title")),
            if row["isLead"] == json!(true) {
                " (sub-lead)"
            } else {
                ""
            },
            chain_text(&chain).unwrap_or_else(|| one_line(text(&row, "leadTitle"))),
            if row["reportOwed"] == json!(true) {
                "yes"
            } else {
                "no"
            },
        );
    }
}
