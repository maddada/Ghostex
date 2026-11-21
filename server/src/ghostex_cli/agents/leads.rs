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

/// Links `linked` to the lead `--lead` names. `owed`: the lead handed it a task with the link.
pub(super) fn link_session(
    linked: &Value,
    lead_ref: &str,
    owed: bool,
    flags: &Flags,
) -> CliResult<Value> {
    let lead = resolve(lead_ref, flags)?;
    call_gxserver_rpc(
        crate::agents::lead_links::LEAD_LINKS_ENDPOINT,
        &json!({
            "op": "link",
            "projectId": linked["projectId"], "sessionId": linked["sessionId"],
            "leadProjectId": lead["projectId"], "leadSessionId": lead["sessionId"],
            "owed": owed,
        }),
        &local_flags(flags),
    )
}

pub(super) fn link(args: &Arguments) -> CliResult<Value> {
    let linked = resolve(&args.positional[0], &args.flags)?;
    let lead_ref = args.lead.as_deref().unwrap_or("self");
    let result = link_session(&linked, lead_ref, false, &args.flags)?;
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

/// A chain of leads, top first, as "Main lead > Sub-lead" (`leadChain` / `reportsTo` rows).
fn chain_text(chain: &Value) -> Option<String> {
    let titles = chain
        .as_array()?
        .iter()
        .map(|entry| match text(entry, "title") {
            "" => text(entry, "globalRef").to_string(),
            title => one_line(title),
        })
        .collect::<Vec<_>>();
    (!titles.is_empty()).then(|| titles.join(" > "))
}

pub(super) fn print_links(result: &Value) {
    // CDXC:LeadLinks 2026-11-21 WHY: with two levels (#701) a lead's list holds its sub-leads'
    // sessions too, so each row names its whole chain of leads, and a sub-lead is told whom it
    // reports to; a gxserver older than two levels sends no chain, and the direct lead stands in.
    if let Some(chain) = chain_text(&result["reportsTo"]) {
        println!("You report to: {chain}");
    }
    let rows = result["links"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        println!("No sessions are linked to a lead.");
        return;
    }
    println!("REFERENCE\tTITLE\tLEADS (TOP FIRST)\tREPORT OWED");
    for row in rows {
        let depth = row["depth"].as_u64().unwrap_or(1).max(1) as usize;
        println!(
            "{}\t{}{}{}\t{}\t{}",
            text(&row, "globalRef"),
            "  ".repeat(depth - 1),
            one_line(text(&row, "title")),
            if row["isLead"] == json!(true) {
                " (sub-lead)"
            } else {
                ""
            },
            chain_text(&row["leadChain"]).unwrap_or_else(|| one_line(text(&row, "leadTitle"))),
            if row["reportOwed"] == json!(true) {
                "yes"
            } else {
                "no"
            },
        );
    }
}
