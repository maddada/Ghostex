//! `ghostex extensions panel get|set|patch|events <id>`: a thin wrapper over the scoped extension
//! API (`/api/ext/v1`, server/src/extensions/ext_api.rs).
//!
//! Inside an extension's own process (`GHOSTEX_API_TOKEN` and `GHOSTEX_API_URL` set by gxserver)
//! it speaks as that extension, which is the only caller that may change its panel
//! (CDXC:Extensions 2026-11-14 WHY in ext_api.rs). Anywhere else it uses this computer's usual
//! token and may read the panel and follow its events.

use std::io::{BufRead, BufReader, Read};

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{resolve_local_gxserver_target, CliError, CliResult},
};

pub fn panel_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let verb = parsed.rest.first().map(String::as_str);
    if matches!(verb, None | Some("help" | "-h" | "--help")) {
        println!("{}", panel_usage());
        return Ok(());
    }
    let id = parsed
        .rest
        .get(1)
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| CliError::Other(format!("Name the extension.\n\n{}", panel_usage())))?;
    let api = ExtApi::resolve()?;
    let query = panel_query(&id, &parsed.flags);
    match verb {
        Some("get") => {
            let answer = api.call("GET", &format!("panel?{query}"), None)?;
            print_json(&answer);
            Ok(())
        }
        Some("set") | Some("patch") => {
            let source = parsed.rest.get(2).map(String::as_str).unwrap_or("-");
            let input = read_json_input(source)?;
            let mut body = if verb == Some("set") {
                // A bare document, or `{document, baseRevision}`.
                if input.get("document").is_some() {
                    input
                } else {
                    json!({ "document": input })
                }
            } else {
                input
            };
            if let Some(revision) = parsed.flags.number("revision") {
                body["baseRevision"] = json!(revision as u64);
            }
            let method = if verb == Some("set") { "PUT" } else { "PATCH" };
            let answer = api.call(method, &format!("panel?{query}"), Some(&body))?;
            if parsed.flags.truthy("json") {
                print_json(&answer);
            } else {
                println!(
                    "Saved {id}'s panel at revision {}.",
                    answer.get("revision").and_then(Value::as_u64).unwrap_or(0)
                );
            }
            Ok(())
        }
        Some("events") => api.follow(&format!("events?extension={}", encode(&id))),
        Some(other) => Err(CliError::Other(format!(
            "Unknown panel command: {other}\n\n{}",
            panel_usage()
        ))),
        None => unreachable!("handled above"),
    }
}

/// `ghostex extensions message <id> <name> [<file.json>|-] [--data <json>]`: inside an agent's
/// session, sends the extension a message about this session (gxserver attaches which one) and
/// prints its answer as JSON (`/api/ext/v1/agentMessage`).
pub fn message_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let usage = "Usage: ghostex extensions message <id> <name> [<file.json>|-] [--data <json>]\n\nRun inside an agent's session: the extension hears the message about that session only, if you allowed it to hear from your agents.";
    let (Some(id), Some(name)) = (parsed.rest.first(), parsed.rest.get(1)) else {
        println!("{usage}");
        return Ok(());
    };
    let data = match (parsed.flags.string_value("data"), parsed.rest.get(2)) {
        (Some(text), _) => serde_json::from_str(text)
            .map_err(|error| CliError::Other(format!("--data is not JSON: {error}")))?,
        (None, Some(source)) => read_json_input(source)?,
        (None, None) => Value::Null,
    };
    let api = ExtApi::resolve()?;
    let answer = api.call(
        "POST",
        &format!("agentMessage?extension={}", encode(id)),
        Some(&json!({ "name": name, "data": data })),
    )?;
    print_json(answer.get("answer").unwrap_or(&answer));
    Ok(())
}

pub fn panel_usage() -> String {
    "Ghostex extension panels - read, write and follow a sidebar extension's panel

Usage:
  ghostex extensions panel get <id> [--surface sidePanel] [--session <projectId>:<sessionId>]
  ghostex extensions panel set <id> [<file.json>|-] [--revision <n>] [--surface …] [--session …]
  ghostex extensions panel patch <id> [<file.json>|-] [--revision <n>] [--surface …] [--session …]
  ghostex extensions panel events <id>

set takes a panel document (schema 1) or {\"document\": …, \"baseRevision\": n}; patch takes
{\"blocks\": [...], \"removeBlocks\": [ids], \"title\", \"tabs\", \"baseRevision\"} and replaces
blocks by id. --revision refuses the write when the panel moved on since that revision.
events prints one JSON line per event until stopped (panelChanged, checklistEdited,
panelAction, and projectsChanged/sessionsChanged when the extension may read them).

Only the extension's own process changes its panel: run set and patch from its server, where
Ghostex sets GHOSTEX_API_TOKEN. Agents and scripts can read it and follow its events.
"
    .to_string()
}

struct ExtApi {
    base_url: String,
    token: String,
}

impl ExtApi {
    fn resolve() -> CliResult<Self> {
        let env = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        // Inside an extension's own process: speak as the extension.
        if let (Some(token), Some(base_url)) = (env("GHOSTEX_API_TOKEN"), env("GHOSTEX_API_URL")) {
            return Ok(Self { base_url, token });
        }
        let target = resolve_local_gxserver_target()?;
        Ok(Self {
            base_url: target.base_url,
            token: target.token,
        })
    }

    fn url(&self, route: &str) -> CliResult<String> {
        // The token goes only to a listener of this account (ghostex_paths::loopback_owner).
        ghostex_paths::loopback_owner::check_url(&self.base_url)
            .map_err(|error| CliError::Other(error.to_string()))?;
        Ok(format!(
            "{}{}{route}",
            self.base_url.trim_end_matches('/'),
            crate::extensions::ext_api::EXT_API_ROUTE_PREFIX
        ))
    }

    fn call(&self, method: &str, route: &str, body: Option<&Value>) -> CliResult<Value> {
        let url = self.url(route)?;
        let agent = crate::http_agent::answering_agent();
        let authorization = format!("Bearer {}", self.token);
        let timeout = Some(std::time::Duration::from_secs(30));
        let response = match (method, body) {
            ("GET", _) => agent
                .get(&url)
                .header("authorization", &authorization)
                .config()
                .timeout_global(timeout)
                .build()
                .call(),
            (method, body) => {
                let text = body.map(Value::to_string).unwrap_or_else(|| "{}".into());
                let request = match method {
                    "PUT" => agent.put(&url),
                    "PATCH" => agent.patch(&url),
                    _ => agent.post(&url),
                };
                request
                    .header("authorization", &authorization)
                    .header("content-type", "application/json")
                    .config()
                    .timeout_global(timeout)
                    .build()
                    .send(text.as_str())
            }
        }
        .map_err(|error| CliError::Connection(format!("Could not reach gxserver: {error}")))?;
        let status = response.status().as_u16();
        let answer: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
        if answer.get("ok") == Some(&Value::Bool(true)) {
            return Ok(answer);
        }
        Err(CliError::Other(
            answer
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("gxserver answered HTTP {status}.")),
        ))
    }

    /// Prints each server-sent event's data as one JSON line until the stream ends.
    fn follow(&self, route: &str) -> CliResult<()> {
        let url = self.url(route)?;
        let response = crate::http_agent::answering_agent()
            .get(&url)
            .header("authorization", format!("Bearer {}", self.token))
            .header("accept", "text/event-stream")
            .config()
            .timeout_global(None)
            .build()
            .call()
            .map_err(|error| CliError::Connection(format!("Could not reach gxserver: {error}")))?;
        let status = response.status().as_u16();
        if status != 200 {
            let answer: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
            return Err(CliError::Other(
                answer
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("gxserver answered HTTP {status}.")),
            ));
        }
        let reader = BufReader::new(response.into_body().into_reader());
        for line in reader.lines() {
            let line = line?;
            if let Some(data) = line.strip_prefix("data: ") {
                println!("{data}");
            }
        }
        Ok(())
    }
}

fn panel_query(id: &str, flags: &Flags) -> String {
    let mut query = format!("extension={}", encode(id));
    for key in ["surface", "session"] {
        if let Some(value) = flags.text(key) {
            query.push_str(&format!("&{key}={}", encode(&value)));
        }
    }
    query
}

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn read_json_input(source: &str) -> CliResult<Value> {
    let mut text = String::new();
    if source == "-" {
        std::io::stdin().read_to_string(&mut text)?;
    } else {
        text = std::fs::read_to_string(source)
            .map_err(|error| CliError::Other(format!("Could not read {source}: {error}")))?;
    }
    serde_json::from_str(&text)
        .map_err(|error| CliError::Other(format!("{source} is not JSON: {error}")))
}
