//! `ghostex cloud`: Cloud Boxes from the command line. The work happens in gxserver
//! (`/api/cloudBoxes`, server/src/cloud_boxes/), the same calls Settings, the new-chat tray and
//! the phone make; every verb takes `--json` for the contract's JSON.

use std::time::{Duration, Instant};

use crate::ghostex_cli::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};
use ghostex_gx_protocol::cloud_box::{setting_keys, CLOUD_BOXES_ROUTE};
use serde_json::{json, Map, Value};

const HELP: &str =
    "Run agent sessions on Cloud Boxes (Instabox, Boat, or the Local Docker provider).

USAGE
  ghostex cloud providers [--json]
  ghostex cloud connect <instabox|boat> [--key <api-key>] [--email <email>] [--no-browser] [--json]
  ghostex cloud disconnect <provider> [--json]
  ghostex cloud sizes [--provider <provider>] [--json]
  ghostex cloud list [--project <project-id>] [--all] [--json]
  ghostex cloud show <box-id> [--json]
  ghostex cloud create --project <project-id> [--snapshot <id|name>] [--provider <provider>]
                       [--size <2x4>] [--blank] [--idempotency-key <key>] [--wait] [--json]
  ghostex cloud resume <box-id> [--wait] [--json]
  ghostex cloud stop <box-id> [--json]
  ghostex cloud delete <box-id> [--force] [--json]
  ghostex cloud connect-info <box-id> --json
  ghostex cloud snapshots [--project <project-id>] [--json]
  ghostex cloud snapshot save <box-id> [--name <name>] [--default | --replace-default] [--json]
  ghostex cloud snapshot default <snapshot-id> [--json]
  ghostex cloud snapshot delete <snapshot-id> [--json]
  ghostex cloud project <project-id> [--json]
  ghostex cloud spend [--refresh] [--json]
  ghostex cloud cap <dollars>|off

  A box is a machine at your provider running Ghostex, where an agent works on its
  own branch. create returns at once with the box starting; --wait waits until it
  runs (or fails). Sending a message to a stopped box's session wakes it; resume
  does the same by hand. delete refuses a box with unpushed work unless --force.
  cap sets the monthly spending cap: at 80% Ghostex warns, at 100% it refuses new
  boxes, and running boxes keep running.

EXAMPLES
  ghostex cloud connect instabox
  ghostex cloud connect boat --key boat_…
  ghostex cloud create --project P1 --wait
  ghostex cloud list
";

const WAIT_LIMIT: Duration = Duration::from_secs(600);

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let mut flags = parsed.flags.clone();
    if flags.contains("help") || flags.contains("h") || parsed.rest.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    // Creating a local box may pull an image; resume and delete run commands on the box.
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeout", "120000");
    }
    let json_output = flags.contains("json");
    let rest = &parsed.rest;
    let arg = |index: usize, what: &str| -> CliResult<String> {
        rest.get(index).cloned().ok_or_else(|| {
            CliError::Other(format!("ghostex cloud {} needs {what}.\n\n{HELP}", rest[0]))
        })
    };
    let request = |action: &str, mut params: Map<String, Value>| -> CliResult<Value> {
        params.insert("action".to_string(), json!(action));
        call_gxserver_rpc(CLOUD_BOXES_ROUTE, &Value::Object(params), &flags)
    };
    let mut params = Map::new();
    let result = match rest[0].as_str() {
        "providers" => {
            let result = request("providers", params)?;
            if !json_output {
                print_providers(&result);
                return Ok(());
            }
            result
        }
        "connect" => {
            params.insert("provider".into(), json!(arg(1, "a provider")?));
            if let Some(key) = flags.string_value("key") {
                params.insert("key".into(), json!(key));
                params.insert("method".into(), json!("key"));
            }
            if let Some(email) = flags.string_value("email") {
                params.insert("email".into(), json!(email));
            }
            if flags.contains("noBrowser") {
                params.insert("openBrowser".into(), json!(false));
            }
            let mut result = request("connect", params)?;
            while result["state"] == "connecting" {
                if !json_output {
                    if let Some(url) = result["authUrl"].as_str() {
                        eprintln!("Finish signing in at {url}");
                    }
                    if let Some(code) = result["userCode"].as_str() {
                        eprintln!("Type this code there: {code}");
                    }
                }
                std::thread::sleep(Duration::from_secs(2));
                let mut status = Map::new();
                status.insert("provider".into(), result["provider"].clone());
                status.insert("connectId".into(), result["connectId"].clone());
                result = request("connectStatus", status)?;
            }
            result
        }
        "disconnect" => {
            params.insert("provider".into(), json!(arg(1, "a provider")?));
            request("disconnect", params)?
        }
        "sizes" => {
            if let Some(provider) = flags.string_value("provider") {
                params.insert("provider".into(), json!(provider));
            }
            request("sizes", params)?
        }
        "list" | "ls" => {
            if let Some(project) = flags.string_value("project") {
                params.insert("projectId".into(), json!(project));
            }
            if flags.contains("all") {
                params.insert("includeDeleted".into(), json!(true));
            }
            let result = request("list", params)?;
            if !json_output {
                print_boxes(&result);
                return Ok(());
            }
            result
        }
        "show" | "get" => {
            params.insert("boxId".into(), json!(arg(1, "a box id")?));
            request("get", params)?
        }
        "create" => {
            let project = flags
                .string_value("project")
                .or_else(|| flags.string_value("projectId"))
                .ok_or_else(|| {
                    CliError::Other(format!(
                        "ghostex cloud create needs --project <project-id>.\n\n{HELP}"
                    ))
                })?;
            params.insert("projectId".into(), json!(project));
            for (flag, key) in [
                ("snapshot", "snapshotId"),
                ("provider", "provider"),
                ("size", "size"),
                ("idempotencyKey", "idempotencyKey"),
            ] {
                if let Some(value) = flags.string_value(flag) {
                    params.insert(key.into(), json!(value));
                }
            }
            if flags.contains("blank") {
                params.insert("blank".into(), json!(true));
            }
            let result = request("create", params)?;
            maybe_wait(result, &flags, &request)?
        }
        "resume" => {
            params.insert("boxId".into(), json!(arg(1, "a box id")?));
            let result = request("resume", params)?;
            maybe_wait(result, &flags, &request)?
        }
        "stop" => {
            params.insert("boxId".into(), json!(arg(1, "a box id")?));
            request("stop", params)?
        }
        "delete" | "rm" => {
            params.insert("boxId".into(), json!(arg(1, "a box id")?));
            if flags.contains("force") {
                params.insert("force".into(), json!(true));
            }
            request("delete", params)?
        }
        "connect-info" => {
            params.insert("boxId".into(), json!(arg(1, "a box id")?));
            request("connectInfo", params)?
        }
        "snapshots" => {
            if let Some(project) = flags.string_value("project") {
                params.insert("projectId".into(), json!(project));
            }
            request("snapshots", params)?
        }
        "snapshot" => match rest.get(1).map(String::as_str) {
            Some("save") => {
                params.insert("boxId".into(), json!(arg(2, "a box id")?));
                if let Some(name) = flags.string_value("name") {
                    params.insert("name".into(), json!(name));
                }
                if flags.contains("default") {
                    params.insert("makeDefault".into(), json!(true));
                }
                if flags.contains("replaceDefault") {
                    params.insert("replaceDefault".into(), json!(true));
                }
                request("saveSnapshot", params)?
            }
            Some("default") => {
                params.insert("snapshotId".into(), json!(arg(2, "a snapshot id")?));
                request("setDefaultSnapshot", params)?
            }
            Some("delete") => {
                params.insert("snapshotId".into(), json!(arg(2, "a snapshot id")?));
                request("deleteSnapshot", params)?
            }
            _ => {
                return Err(CliError::Other(format!(
                    "ghostex cloud snapshot needs save, default or delete.\n\n{HELP}"
                )))
            }
        },
        "project" => {
            params.insert("projectId".into(), json!(arg(1, "a project id")?));
            request("projectSettings", params)?
        }
        "spend" => {
            if flags.contains("refresh") {
                params.insert("refresh".into(), json!(true));
            }
            let result = request("spend", params)?;
            if !json_output {
                print_spend(&result);
                return Ok(());
            }
            result
        }
        "cap" => {
            let value = arg(1, "a dollar amount or off")?;
            // The setting's 0 means off (packages/settings-catalog pages/cloud_boxes.rs).
            let value = if value == "off" {
                "0".to_string()
            } else {
                value
            };
            return crate::ghostex_cli::settings::settings_command(&[
                "set".to_string(),
                setting_keys::MONTHLY_CAP_USD.to_string(),
                value,
            ]);
        }
        other => {
            return Err(CliError::Other(format!(
                "Unknown cloud command: {other}\n\n{HELP}"
            )));
        }
    };
    if json_output {
        print_json(&result);
    } else if let Some(cloud_box) = result.get("box") {
        print_box_line(cloud_box);
        if result["reused"] == true {
            println!("(an existing box matched the idempotency key; nothing new was made)");
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        );
    }
    Ok(())
}

/// With `--wait`, polls `get` until the box runs or fails.
fn maybe_wait(
    result: Value,
    flags: &Flags,
    request: &dyn Fn(&str, Map<String, Value>) -> CliResult<Value>,
) -> CliResult<Value> {
    if !flags.contains("wait") {
        return Ok(result);
    }
    let box_id = result["box"]["boxId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let started = Instant::now();
    let mut current = result;
    loop {
        let state = current["box"]["state"].as_str().unwrap_or_default();
        if matches!(state, "running" | "idle") {
            return Ok(current);
        }
        if state == "failed" {
            let why = current["box"]["stateDetail"]
                .as_str()
                .unwrap_or("the box failed to start");
            return Err(CliError::Other(format!("Box {box_id} failed: {why}")));
        }
        if started.elapsed() > WAIT_LIMIT {
            return Err(CliError::Other(format!(
                "Box {box_id} is still {state} after {} seconds.",
                WAIT_LIMIT.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_secs(1));
        let mut params = Map::new();
        params.insert("boxId".into(), json!(box_id));
        current = request("get", params)?;
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("-")
}

fn print_providers(result: &Value) {
    println!("PROVIDER\tSTATE\tAVAILABLE\tACCOUNT\tNOTE");
    for provider in result["providers"].as_array().into_iter().flatten() {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            text(provider, "id"),
            text(provider, "state"),
            if provider["available"] == true {
                "yes"
            } else {
                "no"
            },
            text(provider, "accountLabel"),
            provider["unavailableReason"]
                .as_str()
                .or(provider["error"].as_str())
                .unwrap_or(""),
        );
    }
}

fn print_box_line(cloud_box: &Value) {
    println!(
        "{}\t{}\t{}\t{}\t{}",
        text(cloud_box, "boxId"),
        text(cloud_box, "name"),
        text(cloud_box, "provider"),
        text(cloud_box, "state"),
        cloud_box["stateDetail"].as_str().unwrap_or(""),
    );
}

fn print_boxes(result: &Value) {
    let boxes = result["boxes"].as_array().cloned().unwrap_or_default();
    if boxes.is_empty() {
        println!("No boxes.");
        return;
    }
    println!("BOX\tNAME\tPROVIDER\tSTATE\tPROJECT\tCOST");
    for cloud_box in boxes {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            text(&cloud_box, "boxId"),
            text(&cloud_box, "name"),
            text(&cloud_box, "provider"),
            text(&cloud_box, "state"),
            text(&cloud_box, "homeProjectId"),
            cloud_box["costTotalUsd"]
                .as_f64()
                .map(|usd| format!("${usd:.2}"))
                .unwrap_or_else(|| "-".into()),
        );
    }
}

fn print_spend(result: &Value) {
    let lines: Vec<String> = result["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|line| {
            format!(
                "{} ${:.2}",
                text(line, "provider"),
                line["usd"].as_f64().unwrap_or_default()
            )
        })
        .collect();
    let total = result["totalUsd"].as_f64().unwrap_or_default();
    match result["capUsd"].as_f64() {
        Some(cap) => println!(
            "This month ({}): {} · total ${total:.2} of ${cap:.2} cap ({})",
            text(result, "month"),
            lines.join(" · "),
            text(result, "state")
        ),
        None => println!(
            "This month ({}): {} · total ${total:.2} (no cap)",
            text(result, "month"),
            lines.join(" · ")
        ),
    }
}
