//! `/api/createBot`: the Bots list's New bot. Runs `hermes profile create` on this computer (a
//! client on another computer reaches it through this computer's gxserver), then adds the new
//! profile's bot project and publishes it at once, so its row is there before the answer arrives.
//! SEE-ALSO: crate::bot_projects (`hermes_profile_create_args`, the name rule), apps/desktop/src/app/new_bot_modal_lifecycle.rs (the dialog that calls it).

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map, Value};

use super::{
    domain_error_response, handle_domain_http, schedule_presentation_project_delta, value_text,
    AppState, RoutedResponse,
};
use crate::bot_projects::{
    bot_profile, discover_bot_profiles, hermes_profile_create_args, hermes_refusal,
    is_new_bot_profile_name, new_bot_profile, sync_bot_projects,
};
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::session_chat_hermes::hermes_home;

/// `hermes profile create` copies a few files and writes a wrapper script; a minute is plenty.
const CREATE_TIMEOUT: Duration = Duration::from_secs(60);
/// The description is one line; a longer one is refused rather than cut.
const DESCRIPTION_MAX_CHARS: usize = 300;

/// What New bot asked for, checked.
struct NewBot {
    profile: String,
    clone_from: Option<String>,
    description: Option<String>,
}

fn read_new_bot(params: &Map<String, Value>) -> Result<NewBot, DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    let profile = text("profile").unwrap_or_default();
    if !is_new_bot_profile_name(profile) {
        return Err(DomainStateError::bad_request(
            "Use lowercase letters and digits for the name, with - or _ after the first one.",
        ));
    }
    let description = text("description")
        .map(|description| description.split_whitespace().collect::<Vec<_>>().join(" "));
    if description
        .as_ref()
        .is_some_and(|description| description.chars().count() > DESCRIPTION_MAX_CHARS)
    {
        return Err(DomainStateError::bad_request(format!(
            "Keep the description under {DESCRIPTION_MAX_CHARS} characters."
        )));
    }
    let clone_from = text("cloneFrom").map(str::to_string);
    if let Some(source) = &clone_from {
        if !discover_bot_profiles(&hermes_home())
            .iter()
            .any(|bot| &bot.profile == source)
        {
            return Err(DomainStateError::bad_request(
                "The bot to copy is no longer on this computer. Pick Blank or another bot.",
            ));
        }
    }
    Ok(NewBot {
        profile: profile.to_string(),
        clone_from,
        description,
    })
}

/// Creates the profile, then adds and publishes its bot project. Answers `{ projectId, profile }`.
pub(super) async fn create_bot_http(
    state: Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> RoutedResponse {
    let refuse = |error: DomainStateError| {
        domain_error_response(endpoint_path.clone(), request_id.clone(), error)
    };
    let bot = match read_domain_rpc_params(&body).and_then(|params| read_new_bot(&params)) {
        Ok(bot) => bot,
        Err(error) => return refuse(error),
    };
    // The user's own home, not an isolated agent home: the Bots list reads `hermes_home()` under
    // it, Hermes puts the bot's `~/.local/bin` wrapper there, and a mise shim finds its tools there.
    let home = state.paths.home_dir.clone();
    let resolve_home = home.clone();
    let executable = tokio::task::spawn_blocking(move || {
        crate::agent_cli::process::resolve("hermes", &resolve_home)
    })
    .await
    .ok()
    .flatten();
    let Some(executable) = executable else {
        return refuse(DomainStateError::bad_request(
            "Hermes isn't installed on this computer.",
        ));
    };
    let args = hermes_profile_create_args(
        &bot.profile,
        bot.clone_from.as_deref(),
        bot.description.as_deref(),
    );
    if let Err(said) = run_hermes(&executable, &args, &home).await {
        return refuse(DomainStateError::bad_request(said));
    }
    let worker_state = state.clone();
    let (worker_endpoint, worker_request_id) = (endpoint_path.clone(), request_id.clone());
    let profile = bot.profile;
    match tokio::task::spawn_blocking(move || {
        handle_domain_http(
            &worker_state,
            worker_endpoint,
            worker_request_id,
            &json!({}),
            |repository, db, _, _| add_new_bot_project(&worker_state, db, repository, &profile),
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => refuse(DomainStateError::corrupt_state(format!(
            "The bot was created, but its row could not be added: {error}"
        ))),
    }
}

/// Adds the new profile's project (whether or not Bots is on here: the user made it from a Bots
/// list) and publishes it.
fn add_new_bot_project(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    profile: &str,
) -> Result<Value, DomainStateError> {
    let added = sync_bot_projects(repository, &[new_bot_profile(&hermes_home(), profile)])?;
    if !added.is_empty() {
        crate::external_sessions::rescan_on_next_discovery(&state.paths);
    }
    for project in &added {
        let project_id = value_text(project, "projectId")?;
        schedule_presentation_project_delta(state, db, repository, &project_id, "projectAdded")?;
    }
    let project_id = repository
        .list_projects()?
        .iter()
        .find(|project| bot_profile(project) == Some(profile))
        .and_then(|project| project.get("projectId").and_then(Value::as_str))
        .map(str::to_string);
    Ok(json!({ "profile": profile, "projectId": project_id }))
}

/// Runs `hermes <args>` for the user, with `HERMES_HOME` set to the folder the Bots list reads so
/// the profile lands there whatever the user's shell sets. Fails with what Hermes said.
async fn run_hermes(executable: &str, args: &[String], home: &Path) -> Result<(), String> {
    let mut command = tokio::process::Command::new(executable);
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(not(windows))]
    command.env(
        "PATH",
        crate::agent_hooks::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            home,
        ),
    );
    #[cfg(windows)]
    command.env("PATH", crate::platform::live_path::value());
    {
        use crate::platform::process::NoConsoleWindow as _;
        command.no_console_window();
    }
    command
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("HERMES_HOME", hermes_home())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(CREATE_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("Hermes couldn't be started: {error}")),
        Err(_) => return Err("Hermes didn't answer within a minute. Try again.".to_string()),
    };
    if output.status.success() {
        return Ok(());
    }
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Err(hermes_refusal(&said).unwrap_or_else(|| "Hermes couldn't create the bot.".to_string()))
}
