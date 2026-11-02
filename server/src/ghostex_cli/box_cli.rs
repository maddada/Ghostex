//! `ghostex box`: the verbs home gxserver (or a phone) runs on a Cloud Box through the
//! provider's exec API (contract: `ghostex_gx_protocol::cloud_box::box_side`). Each prints exactly
//! one JSON object and exits non-zero on failure.

use ghostex_gx_protocol::cloud_box::box_verbs;
use serde_json::{json, Value};

use crate::cloud_boxes::box_mode;
use crate::ghostex_cli::{
    args::parse_args,
    rpc::{CliError, CliResult},
};

const HELP: &str = "Commands Ghostex runs on a Cloud Box. You don't run these yourself.

USAGE
  ghostex box boot --ticket <ticket>
  ghostex box prepare-session [--request <file>]
  ghostex box prepare-snapshot
  ghostex box finish-snapshot
  ghostex box idle-status
  ghostex box work-status
  ghostex box write-back [--ack <id,id>]
  ghostex box relay --listen <addr:port> --to <addr:port>
  ghostex box idle-watch
";

fn print_ok(mut value: Value) {
    if let Some(object) = value.as_object_mut() {
        object.insert("ok".to_string(), json!(true));
    }
    println!("{}", serde_json::to_string(&value).unwrap_or_default());
}

fn fail(message: String) -> CliResult<()> {
    println!("{}", json!({ "ok": false, "error": message }));
    crate::ghostex_cli::set_exit_code(1);
    Ok(())
}

/// Prints a verb's answer, or its failure.
fn answer<T: serde::Serialize>(result: Result<T, String>) -> CliResult<()> {
    match result {
        Ok(value) => {
            print_ok(serde_json::to_value(value).unwrap_or_default());
            Ok(())
        }
        Err(message) => fail(message),
    }
}

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let Some(verb) = parsed.rest.first() else {
        print!("{HELP}");
        return Ok(());
    };
    if flags.contains("help") {
        print!("{HELP}");
        return Ok(());
    }
    match verb.as_str() {
        box_verbs::BOOT => {
            let Some(ticket) = flags.string_value("ticket") else {
                return fail("ghostex box boot needs --ticket.".to_string());
            };
            let ticket = match ghostex_gx_cloud::ticket::decode_ticket(ticket) {
                Ok(ticket) => ticket,
                Err(message) => return fail(message),
            };
            answer(box_mode::boot::run_boot(&ticket))
        }
        box_verbs::PREPARE_SESSION => answer(box_mode::prepare_session::run_prepare_session(
            flags.string_value("request"),
        )),
        box_verbs::PREPARE_SNAPSHOT => answer(box_mode::snapshot::run_prepare_snapshot()),
        box_verbs::FINISH_SNAPSHOT => answer(box_mode::snapshot::run_finish_snapshot()),
        box_verbs::IDLE_STATUS => answer(box_mode::idle::run_idle_status()),
        box_verbs::WORK_STATUS => answer(box_mode::work::run_work_status()),
        box_verbs::WRITE_BACK => {
            let ack: Vec<String> = flags
                .string_value("ack")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect();
            answer(box_mode::logins::run_write_back(&ack))
        }
        box_verbs::IDLE_WATCH => box_mode::idle::run_idle_watch().map_err(CliError::Other),
        box_verbs::RELAY => {
            let (Some(listen), Some(target)) =
                (flags.string_value("listen"), flags.string_value("to"))
            else {
                return Err(CliError::Other(format!(
                    "ghostex box relay needs --listen and --to.\n\n{HELP}"
                )));
            };
            box_mode::relay::run_relay(listen, target).map_err(CliError::Other)
        }
        other => fail(format!("Unknown box command: {other}")),
    }
}
