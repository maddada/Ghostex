//! `ghostex visual export <page>`: writes a Ghostex page as an exported page, one HTML file that
//! works in any browser without Ghostex; its Send exports the reader's answers as JSON, which a
//! chat imports back (server/src/visual_pages/exported.rs, import.rs). The page card's and page
//! window's Export button download the same file.

use std::{fs, path::PathBuf};

use serde_json::{json, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::rpc::{
    request_gxserver_rpc, resolve_gxserver_server_target, CliError, CliResult,
};
use crate::visual_pages::EXPORT_VISUAL_PAGE_ENDPOINT;

const USAGE: &str =
    "ghostex visual export <file.html|page id|page URL> [--out <file.html>] [--json]";

pub fn export_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let Some(page) = parsed.rest.first().map(|page| page.trim().to_string()) else {
        return Err(CliError::Other(format!("export needs a page: {USAGE}")));
    };
    let params = match page_id(&page) {
        Some(id) => json!({ "pageId": id }),
        None => json!({ "file": absolute(&page).to_string_lossy() }),
    };
    let target = resolve_gxserver_server_target(&parsed.flags, &params)?;
    let result =
        request_gxserver_rpc(&target, EXPORT_VISUAL_PAGE_ENDPOINT, &params, &parsed.flags)?;
    let text = |key: &str| result.get(key).and_then(Value::as_str).unwrap_or_default();
    let html = text("html");
    if html.is_empty() {
        return Err(CliError::Other(
            "gxserver did not return the exported page. Update Ghostex and try again.".to_string(),
        ));
    }
    let out = parsed
        .flags
        .string_value("out")
        .map(absolute)
        .unwrap_or_else(|| absolute(text("fileName")));
    fs::write(&out, html)
        .map_err(|error| CliError::Other(format!("Could not write {}: {error}", out.display())))?;
    if parsed.flags.truthy("json") {
        println!(
            "{}",
            json!({
                "ok": true,
                "pageId": text("pageId"),
                "title": text("title"),
                "path": out.to_string_lossy(),
            })
        );
    } else {
        println!(
            "Exported \"{}\" to {}\nOpen it in any browser; its Export answers button gives the answers as JSON. Drop or paste that JSON into the chat to give them to the agent.",
            text("title"),
            out.display()
        );
    }
    Ok(())
}

/// The page id in `page` when it is one, or a `/visual/<id>` address.
fn page_id(page: &str) -> Option<String> {
    let candidate = match page.find("/visual/") {
        Some(at) => &page[at + "/visual/".len()..],
        None => page,
    };
    let id: String = candidate
        .chars()
        .take_while(|character| character.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase();
    (id.len() == 32 && (candidate.len() == 32 || page.contains("/visual/"))).then_some(id)
}

fn absolute(file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        return path;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or(path)
}
