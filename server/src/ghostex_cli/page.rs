use serde_json::{json, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::rpc::{
    request_gxserver_rpc, resolve_gxserver_server_target, CliError, CliResult,
};
use crate::visual_pages::{page_id_of, RETIRE_VISUAL_PAGES_ENDPOINT};

/// `ghostex page retire <page>... [--json]`: takes back Ghostex pages the agent published and
/// replaced, with their open questions (CDXC:Coordinators 2026-11-16 DECISION in
/// server/src/visual_pages/retire.rs).
pub fn page_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("retire") => retire_command(&args[1..]),
        None | Some("help") => {
            println!("{}", crate::ghostex_cli::usage::command_usage("page"));
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown page command: {other}\n\n{}",
            crate::ghostex_cli::usage::command_usage("page")
        ))),
    }
}

fn retire_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let pages: Vec<String> = parsed.rest.clone();
    if pages.is_empty() {
        return Err(CliError::Other(
            "page retire needs a page: ghostex page retire <page id or address>... [--json]"
                .to_string(),
        ));
    }
    if let Some(bad) = pages.iter().find(|page| page_id_of(page).is_none()) {
        return Err(CliError::Other(format!(
            "{bad} is not a Live page. Give the page's id (32 characters) or its address, the \"url\" in the block `ghostex show` printed."
        )));
    }
    let params = json!({ "pageIds": pages });
    let target = resolve_gxserver_server_target(&parsed.flags, &params)?;
    let result = request_gxserver_rpc(
        &target,
        RETIRE_VISUAL_PAGES_ENDPOINT,
        &params,
        &parsed.flags,
    )?;
    if parsed.flags.truthy("json") {
        println!("{result}");
        return Ok(());
    }
    let list = |key: &str| {
        result
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    for page in list("retired") {
        let text = |key: &str| {
            page.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        println!(
            "Retired {} ({}): it no longer waits on the reader.",
            text("title"),
            text("id")
        );
    }
    for id in list("noSession") {
        println!(
            "{}: no chat shows it and no session published it, so it waits on nobody.",
            id.as_str().unwrap_or_default()
        );
    }
    let missing = list("notFound");
    if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().filter_map(Value::as_str).collect();
        return Err(CliError::Other(format!(
            "No such page on this computer: {}",
            names.join(", ")
        )));
    }
    Ok(())
}
