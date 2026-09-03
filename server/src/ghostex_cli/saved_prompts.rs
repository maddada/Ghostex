use serde_json::Value;

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/*
CDXC:SavedPrompts 2026-08-26:
Saved Prompts is a shared React surface, but mobile reaches the owning
gxserver only through SSH-executed CLI verbs. Keep this as one narrowly scoped
RPC bridge with an allowlisted subaction set: the page sends the same payloads
as the desktop modal, while the CLI remains incapable of calling arbitrary
gxserver paths.
*/

const SAVED_PROMPT_ACTIONS: [(&str, &str); 6] = [
    ("list", "/api/listStashedPrompts"),
    ("save", "/api/saveStashedPrompt"),
    ("delete", "/api/deleteStashedPrompt"),
    ("save-tag", "/api/saveStashedPromptTag"),
    ("delete-tag", "/api/deleteStashedPromptTag"),
    ("set-tags", "/api/setStashedPromptTags"),
];

fn usage() -> &'static str {
    "Usage: ghostex saved-prompts <list|save|delete|save-tag|delete-tag|set-tags> --payload-json <json> --json"
}

pub fn saved_prompts_command(args: &[String]) -> CliResult<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{}", usage());
        return Ok(());
    }

    let parsed = parse_args(args);
    let action = parsed
        .rest
        .first()
        .map(String::as_str)
        .ok_or_else(|| CliError::Other(usage().to_string()))?;
    if parsed.rest.len() != 1 {
        return Err(CliError::Other(usage().to_string()));
    }
    let pathname = SAVED_PROMPT_ACTIONS
        .iter()
        .find_map(|(candidate, pathname)| (*candidate == action).then_some(*pathname))
        .ok_or_else(|| {
            CliError::Other(format!(
                "Unknown Saved Prompts action: {action}\n\n{}",
                usage()
            ))
        })?;

    let payload_text = parsed.flags.string_value("payloadJson").unwrap_or("{}");
    let payload: Value = serde_json::from_str(payload_text)
        .map_err(|error| CliError::Other(format!("Invalid --payload-json: {error}")))?;
    if !payload.is_object() {
        return Err(CliError::Other(
            "--payload-json must contain a JSON object.".to_string(),
        ));
    }

    let result = call_gxserver_rpc(pathname, &payload, &parsed.flags)?;
    if is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    print_json(&result);
    Ok(())
}
