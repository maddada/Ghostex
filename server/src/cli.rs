use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use serde_json::{Map, Value};

use crate::{
    agent_hooks::{repair_installed_agent_hook_paths, run_notify_hook},
    agent_skills::{install_agent_skills, read_agent_skill_status},
    auth::read_gxserver_auth_token,
    config::read_selected_local_api_port,
    constants::{GXSERVER_LOCAL_API_HOST, GXSERVER_PRODUCT, GXSERVER_VERSION},
    http_client::{fetch_server_health, request_server_stop, request_server_stop_all},
    paths::{get_gxserver_paths, migrate_legacy_storage},
    protocol::StatusResponse,
    runtime::{
        create_running_status, create_source_build_identity, create_stopped_status,
        is_build_identity_reusable, is_process_running, read_current_build_identity,
        read_runtime_metadata,
    },
    server::{run_gxserver_foreground, GxserverForegroundOptions},
};

pub async fn run_from_env() -> Result<()> {
    run(env::args().skip(1).collect()).await
}

/*
CDXC:GxserverCli 2026-06-14-20:37:
The Rust CLI intentionally keeps the TypeScript command surface and --json behavior for start, stop, stop-all, and status so app/CLI opt-in can swap binaries without changing client command construction.

CDXC:GxserverCli 2026-06-22-04:47:
`gxserver status` reports any reachable same-product, same-protocol daemon as running; build identity is a start-time replacement decision. `gxserver start` must match TypeScript by requesting control-plane shutdown for a running build-identity mismatch instead of returning a Rust-only portConflict status.
*/
pub async fn run(args: Vec<String>) -> Result<()> {
    let command = args.first().map(String::as_str);
    /*
    `setup` is the recovery boundary for an uploaded managed package. It must
    not depend on the existing storage layout being migratable: setup replaces
    stale package/tool links, after which the newly activated runtime can run
    the normal migration path. This also lets setup repair legacy links whose
    targets were moved into XDG storage by an interrupted older migration.
    */
    if command == Some("setup") {
        crate::setup::run_setup(args.iter().skip(1).cloned().collect())?;
        return Ok(());
    }
    migrate_legacy_storage().context("migrate legacy Ghostex storage")?;
    if matches!(command, None | Some("--foreground")) {
        let paths = get_gxserver_paths(None);
        repair_installed_agent_hook_paths(&paths)
            .context("repair installed Ghostex agent hook paths")?;
    }
    let version = GXSERVER_VERSION.to_string();
    let build_identity = read_current_build_identity(&version)?;
    match command {
        None | Some("--foreground") => {
            let result = run_gxserver_foreground(GxserverForegroundOptions {
                build_identity: Some(build_identity),
                home_dir: None,
                version,
            })
            .await?;
            if result.reused {
                println!("gxserver is already running and uses the expected protocol.");
            }
        }
        Some("start") => {
            print_status(
                &start_gxserver_background(&build_identity, &version).await?,
                args.iter().skip(1).any(|arg| arg == "--json"),
            )?;
        }
        Some("stop") => {
            print_status(
                &stop_gxserver_control_plane(&build_identity, &version).await?,
                args.iter().skip(1).any(|arg| arg == "--json"),
            )?;
        }
        Some("stop-all") => {
            print_status(
                &stop_gxserver_and_sessions(&build_identity, &version).await?,
                args.iter().skip(1).any(|arg| arg == "--json"),
            )?;
        }
        Some("status") => {
            print_status(
                &get_gxserver_status(&build_identity, &version).await?,
                args.iter().skip(1).any(|arg| arg == "--json"),
            )?;
        }
        Some("agent-skills") => {
            run_agent_skills_command(args.iter().skip(1).cloned().collect()).await?;
        }
        Some("agent-hook-notify") => {
            run_notify_hook(args.iter().skip(1).cloned().collect())?;
        }
        Some("resume-lookup") => {
            crate::resume_lookup::run_resume_lookup(args.iter().skip(1).cloned().collect())?;
        }
        Some("--version") | Some("version") => {
            println!("{version}");
        }
        Some("--help") | Some("help") => {
            print_help(&version);
        }
        Some(other) => return Err(anyhow!("Unknown gxserver command: {other}")),
    }
    Ok(())
}

pub async fn get_gxserver_status(_build_identity: &str, _version: &str) -> Result<StatusResponse> {
    let paths = get_gxserver_paths(None);
    let metadata = read_runtime_metadata(&paths)?;
    let auth = read_gxserver_auth_token(&paths)?;
    if let Some(health) = fetch_server_health(auth.as_ref().map(|auth| auth.token.as_str()), 800)? {
        return Ok(create_running_status(health, metadata));
    }
    if let Some(metadata) = metadata.clone() {
        if is_process_running(metadata.pid) {
            let pid = metadata.pid;
            return Ok(StatusResponse {
                health: None,
                metadata: Some(metadata),
                message: format!(
                    "gxserver runtime metadata exists for pid {}, but {GXSERVER_LOCAL_API_HOST}:{} is unreachable.",
                    pid,
                    read_selected_local_api_port()?
                ),
                ok: false,
                product: GXSERVER_PRODUCT.to_string(),
                state: "unreachable".to_string(),
            });
        }
    }
    Ok(create_stopped_status(metadata))
}

async fn start_gxserver_background(build_identity: &str, version: &str) -> Result<StatusResponse> {
    let before = get_gxserver_status(build_identity, version).await?;
    if before.state == "running" {
        if is_build_identity_reusable(
            before
                .health
                .as_ref()
                .map(|health| health.build_identity.as_str()),
            Some(build_identity),
        ) {
            return Ok(before);
        }
        let paths = get_gxserver_paths(None);
        let auth = read_gxserver_auth_token(&paths)?;
        let _ = request_server_stop(auth.as_ref().map(|auth| auth.token.as_str()), 2_000)?;
        let stopped = wait_for_status(
            build_identity,
            version,
            Duration::from_millis(5_000),
            |status| status.state != "running",
        )
        .await?;
        if stopped.state == "running" {
            if is_build_identity_reusable(
                stopped
                    .health
                    .as_ref()
                    .map(|health| health.build_identity.as_str()),
                Some(build_identity),
            ) {
                return Ok(stopped);
            }
            return Ok(StatusResponse {
                message:
                    "gxserver build identity changed, but the old control plane did not stop. Stop gxserver and start it again so the current migration code can run."
                        .to_string(),
                ok: false,
                state: "stopping".to_string(),
                ..before
            });
        }
    }

    let current_exe = env::current_exe().with_context(|| "resolve current gxserver binary")?;
    let mut command = Command::new(current_exe);
    command
        .arg("--foreground")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child_pid = spawn_detached(&mut command)?;

    let status = wait_for_status(
        build_identity,
        version,
        Duration::from_millis(5_000),
        |status| status.state == "running",
    )
    .await?;
    if status.state != "running" {
        return Ok(StatusResponse {
            health: None,
            metadata: None,
            message: format!(
                "gxserver start launched pid {child_pid} but health did not become ready on {GXSERVER_LOCAL_API_HOST}:{}.",
                read_selected_local_api_port()?
            ),
            ok: false,
            product: GXSERVER_PRODUCT.to_string(),
            state: "starting".to_string(),
        });
    }
    Ok(status)
}

async fn stop_gxserver_control_plane(
    build_identity: &str,
    version: &str,
) -> Result<StatusResponse> {
    let before = get_gxserver_status(build_identity, version).await?;
    if before.state != "running" {
        return Ok(before);
    }
    let paths = get_gxserver_paths(None);
    let auth = read_gxserver_auth_token(&paths)?;
    let _ = request_server_stop(auth.as_ref().map(|auth| auth.token.as_str()), 800)?;
    let stopped = wait_for_status(
        build_identity,
        version,
        Duration::from_millis(5_000),
        |status| status.state != "running",
    )
    .await?;
    if stopped.state == "running" {
        return Ok(StatusResponse {
            message:
                "gxserver stop requested control-plane shutdown, but the server is still running."
                    .to_string(),
            ok: false,
            state: "stopping".to_string(),
            ..stopped
        });
    }
    Ok(StatusResponse {
        message: "gxserver control plane stopped. zmx sessions were not signaled or killed."
            .to_string(),
        ok: true,
        ..stopped
    })
}

async fn stop_gxserver_and_sessions(build_identity: &str, version: &str) -> Result<StatusResponse> {
    let before = get_gxserver_status(build_identity, version).await?;
    if before.state != "running" {
        return Ok(before);
    }
    let paths = get_gxserver_paths(None);
    let auth = read_gxserver_auth_token(&paths)?;
    let stop_all = request_server_stop_all(auth.as_ref().map(|auth| auth.token.as_str()), 10_000)?;
    if stop_all.is_none() {
        return Ok(StatusResponse {
            message: "gxserver stop-all could not kill zmx sessions before shutdown.".to_string(),
            ok: false,
            state: "stopping".to_string(),
            ..before
        });
    }
    let stopped = wait_for_status(
        build_identity,
        version,
        Duration::from_millis(10_000),
        |status| status.state != "running",
    )
    .await?;
    if stopped.state == "running" {
        return Ok(StatusResponse {
            message:
                "gxserver stop-all killed zmx sessions but the control plane is still running."
                    .to_string(),
            ok: false,
            state: "stopping".to_string(),
            ..stopped
        });
    }
    let result = stop_all
        .and_then(|value| value.get("result").cloned())
        .unwrap_or(Value::Null);
    let killed = result
        .get("killedSessions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failed = result
        .get("failedSessions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Ok(StatusResponse {
        message: format!("gxserver control plane stopped after stop-all. zmx sessions killed: {killed}; failed: {failed}."),
        ok: failed == 0,
        ..stopped
    })
}

async fn wait_for_status(
    build_identity: &str,
    version: &str,
    timeout: Duration,
    done: impl Fn(&StatusResponse) -> bool,
) -> Result<StatusResponse> {
    let deadline = Instant::now() + timeout;
    let mut status = get_gxserver_status(build_identity, version).await?;
    while !done(&status) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        status = get_gxserver_status(build_identity, version).await?;
    }
    Ok(status)
}

fn print_status(status: &StatusResponse, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(status)?);
    } else {
        println!("{}", status.message);
    }
    Ok(())
}

fn print_help(version: &str) {
    println!(
        "gxserver {version}

Usage:
  gxserver           Run gxserver in the foreground
  gxserver start     Start gxserver in the background
  gxserver stop      Stop only the gxserver control plane
  gxserver stop-all  Stop gxserver and kill tracked zmx sessions
  gxserver status    Print gxserver runtime state
  gxserver agent-skills status [skill...] [--json]
  gxserver agent-skills install <skill...> --source <path> [--json]
  gxserver setup [--install-root <dir>] [--release-dir <dir>] [--upload-path <file>]
                 [--analytics-role remote|local]
                     Activate an extracted release package (stop old server, link tools).
                     --analytics-role remote marks this install as an SSH helper
                     driven by a desktop, so it never emits anonymous analytics.
  gxserver --version Print the gxserver package version
"
    );
}

/*
CDXC:AgentSkills 2026-06-19-13:59:
The Rust CLI mirrors TypeScript's direct agent-skill setup commands so app and shell callers can check/install Ghostex skills without constructing gxserver RPC bodies or duplicating discovery/install argument rules.
*/
async fn run_agent_skills_command(args: Vec<String>) -> Result<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("status");
    if matches!(subcommand, "help" | "-h" | "--help") {
        print_agent_skills_help();
        return Ok(());
    }
    let parsed = parse_cli_args(&args.iter().skip(1).cloned().collect::<Vec<_>>());
    let paths = get_gxserver_paths(None);
    match subcommand {
        "status" => {
            let mut params = Map::new();
            if !parsed.rest.is_empty() {
                params.insert("skillNames".to_string(), json_array(parsed.rest));
            }
            if let Some(repository_paths) = parsed.values.get("repository") {
                params.insert(
                    "repositoryPaths".to_string(),
                    json_array(repository_paths.clone()),
                );
            }
            let result = read_agent_skill_status(&paths, &params)?;
            if parsed.flags.contains("json") {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print_agent_skill_status(&result);
            }
        }
        "install" => {
            let source = parsed
                .values
                .get("source")
                .or_else(|| parsed.values.get("packageSource"))
                .and_then(|values| values.first())
                .cloned();
            let mut params = Map::new();
            params.insert("skillNames".to_string(), json_array(parsed.rest));
            if let Some(source) = source {
                params.insert("packageSource".to_string(), Value::String(source));
            }
            if let Some(agent_ids) = parsed
                .values
                .get("agent")
                .or_else(|| parsed.values.get("agents"))
            {
                params.insert("agentIds".to_string(), json_array(agent_ids.clone()));
            }
            if let Some(repository_paths) = parsed.values.get("repository") {
                params.insert(
                    "repositoryPaths".to_string(),
                    json_array(repository_paths.clone()),
                );
            }
            let result = install_agent_skills(&paths, &params).await?;
            if parsed.flags.contains("json") {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                if let Some(stdout) = result.get("stdout").and_then(Value::as_str) {
                    print!("{stdout}");
                }
                if let Some(stderr) = result.get("stderr").and_then(Value::as_str) {
                    eprint!("{stderr}");
                }
                print_agent_skill_status(&result);
            }
        }
        other => return Err(anyhow!("Unknown gxserver agent-skills command: {other}")),
    }
    Ok(())
}

fn print_agent_skill_status(status: &Value) {
    if let Some(skills) = status.get("skills").and_then(Value::as_array) {
        for skill in skills {
            let skill_name = skill
                .get("skillName")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let marker = if skill
                .get("installed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "installed"
            } else {
                "missing"
            };
            println!("{skill_name}: {marker}");
            if let Some(locations) = skill.get("locations").and_then(Value::as_array) {
                for location in locations {
                    if let Some(directory_path) =
                        location.get("directoryPath").and_then(Value::as_str)
                    {
                        println!("  {directory_path}");
                    }
                }
            }
        }
    }
}

fn print_agent_skills_help() {
    println!(
        "gxserver agent-skills

Usage:
  gxserver agent-skills status [skill...] [--repository path] [--json]
  gxserver agent-skills install <skill...> --source <skills-package-path> [--agent id...] [--json]
"
    );
}

#[derive(Debug, Default)]
struct ParsedCliArgs {
    flags: HashSet<String>,
    rest: Vec<String>,
    values: HashMap<String, Vec<String>>,
}

fn parse_cli_args(args: &[String]) -> ParsedCliArgs {
    let mut parsed = ParsedCliArgs::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            parsed.rest.extend(args.iter().skip(index + 1).cloned());
            break;
        }
        if !arg.starts_with("--") {
            parsed.rest.push(arg.clone());
            index += 1;
            continue;
        }
        let body = &arg[2..];
        let (key, value) = if let Some((key, value)) = body.split_once('=') {
            (to_camel_case(key), Some(value.to_string()))
        } else {
            (to_camel_case(body), None)
        };
        if let Some(value) = value {
            push_cli_value(&mut parsed.values, key, value);
            index += 1;
            continue;
        }
        if is_value_less_cli_flag(&key) {
            parsed.flags.insert(key);
            index += 1;
            continue;
        }
        let next = args.get(index + 1);
        if next.map(|value| value.starts_with("--")).unwrap_or(true) {
            parsed.flags.insert(key);
            index += 1;
            continue;
        }
        push_cli_value(
            &mut parsed.values,
            key.clone(),
            next.cloned().unwrap_or_default(),
        );
        index += 2;
        if key == "agent" || key == "agents" {
            while index < args.len() && !args[index].starts_with("--") {
                push_cli_value(&mut parsed.values, key.clone(), args[index].clone());
                index += 1;
            }
        }
    }
    parsed
}

/*
CDXC:AgentSkillsCli 2026-06-19-18:44:
Agent-skill commands must treat known value-less switches such as --json as flags even when a positional skill name follows, so `gxserver agent-skills status --json ghostex-browser-use` keeps the skill name in rest while enabling JSON output.
*/
fn is_value_less_cli_flag(key: &str) -> bool {
    matches!(key, "json")
}

fn push_cli_value(values: &mut HashMap<String, Vec<String>>, key: String, value: String) {
    values.entry(key).or_default().push(value);
}

fn to_camel_case(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = false;
    for character in value.chars() {
        if character == '-' {
            uppercase_next = true;
            continue;
        }
        if uppercase_next {
            output.extend(character.to_uppercase());
            uppercase_next = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn json_array(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}

fn spawn_detached(command: &mut Command) -> Result<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let child = command
        .spawn()
        .with_context(|| "spawn gxserver background")?;
    Ok(child.id())
}

#[allow(dead_code)]
fn _home_dir_for_tests(path: PathBuf) -> GxserverForegroundOptions {
    GxserverForegroundOptions {
        build_identity: Some(create_source_build_identity(GXSERVER_VERSION)),
        home_dir: Some(path),
        version: GXSERVER_VERSION.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parse_cli_args_keeps_skill_name_after_json_flag() {
        let parsed = parse_cli_args(&args(&["--json", "ghostex-browser-use"]));

        assert!(parsed.flags.contains("json"));
        assert_eq!(parsed.rest, vec!["ghostex-browser-use".to_string()]);
        assert_eq!(parsed.values.get("json"), None);
    }

    #[test]
    fn parse_cli_args_still_consumes_known_value_options() {
        let parsed = parse_cli_args(&args(&[
            "--repository",
            "/tmp/skills",
            "--json",
            "ghostex-browser-use",
        ]));

        assert!(parsed.flags.contains("json"));
        assert_eq!(parsed.rest, vec!["ghostex-browser-use".to_string()]);
        assert_eq!(
            parsed.values.get("repository"),
            Some(&vec!["/tmp/skills".to_string()])
        );
    }
}
