//! The steps an agent follows to turn a Ghostex problem into a GitHub issue, in one place for every
//! agent that files one: the feedback agent Send Bug / Feedback's Start Agent starts (agent.rs puts
//! them under what the user wrote) and Ask Ghostex AI, which reads them with `ghostex feedback brief`
//! (`POST /api/feedbackBrief`). Both get a report folder of their own in the Ghostex config folder,
//! the same rules for what to read, what to leave alone and what to keep out of the issue, the same
//! issue layout, the draft shown to the user before anything is posted, and the same post
//! (`ghostex feedback send`, the dialog's own relay call).
//!
//! CDXC:Feedback 2026-11-21 DECISION:
//! User: "we need to add to the Ask Ghostex AI that it can help the user fix issues with ghostex and
//! submit bug reports also but it needs to use the same that we have here" (the Send Bug / Feedback
//! dialog with "Collect more data with an agent"). Ask Ghostex AI files reports through this same
//! brief and the same `ghostex feedback send`, never a second set of rules: gathering only reads,
//! the issue keeps out the same private things, it names the user's GitHub username, and the user
//! sees the whole draft and says yes before it is posted.

use std::fs;
use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use serde_json::{Map, Value, json};

use crate::domain::{DomainStateError, read_domain_rpc_params};
use crate::protocol::rpc_success;
use crate::server::{AppState, RoutedResponse, domain_error_response, routed_json};

use super::draft::github_username;
use super::relay::FeedbackImage;

pub(crate) const FEEDBACK_BRIEF_ENDPOINT: &str = "/api/feedbackBrief";

/// Whether the agent shows the user its draft and waits for a yes before it posts.
///
/// CDXC:Feedback 2026-10-16 DECISION:
/// User, asked whether the feedback agent may post the issue straight away: "Show me the draft in its chat first". The agent shows its full draft (title, body and the screenshots it will attach) in its chat and posts only after the user says yes there, as the 2026-10-04 rule has the pop-up's Review step do (apps/desktop/src/app/window/feedback_modal/state.rs). [`report_steps`] words the last step from this switch, for Ask Ghostex AI too.
const ASKS_BEFORE_POSTING: bool = true;

/// A new folder for one report's screenshots, notes and draft: `feedback/<timestamp>/` in the
/// Ghostex config folder, the one folder the agent is told it may write to.
pub(super) fn new_report_folder(config_dir: &Path) -> Result<PathBuf, DomainStateError> {
    let folder = config_dir.join("feedback").join(
        chrono::Local::now()
            .format("%Y-%m-%d-%H%M%S%3f")
            .to_string(),
    );
    fs::create_dir_all(&folder).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not create the report folder: {error}"),
    })?;
    Ok(folder)
}

/// Writes the screenshots into the report's folder (`screenshots/screenshot-N.png`) and returns
/// their paths, in order.
pub(super) fn save_screenshots(
    report_folder: &Path,
    images: &[FeedbackImage],
) -> Result<Vec<PathBuf>, DomainStateError> {
    if images.is_empty() {
        return Ok(Vec::new());
    }
    let folder = report_folder.join("screenshots");
    fs::create_dir_all(&folder).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save the screenshots: {error}"),
    })?;
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let path = folder.join(format!("screenshot-{}.{}", index + 1, image.extension()));
            fs::write(&path, image.bytes()).map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("Could not save {}: {error}", image.name()),
            })?;
            Ok(path)
        })
        .collect()
}

/// "What to do": checking the version, reading, gathering, changing nothing, keeping private
/// things out, the issue's layout, showing the draft and posting it with `ghostex feedback send`.
pub(super) fn report_steps(username: &str, report_folder: &Path, images: &[PathBuf]) -> String {
    let report_folder = report_folder.display();
    let image_flags = images
        .iter()
        .map(|path| format!(" --image \"{}\"", path.display()))
        .collect::<String>();
    let posting = if ASKS_BEFORE_POSTING {
        "8. Show the user the whole draft here: the title, the full body, and the screenshots you will attach. Ask whether to post it, apply any changes they ask for, and post only after they say yes."
    } else {
        "8. Post the issue when the draft is complete."
    };
    format!(
        r#"What to do:
1. Check the version first. Run `ghostex app-update check`; its JSON says `updateAvailable` and `availableVersion` when a newer Ghostex than the installed `appVersion` is out. If one is, stop here: tell the user "You're on <appVersion>; <availableVersion> is out. Please update first: it may already be fixed.", say they can update from Settings > Updates (or the Update button in the top bar), and ask whether to update first or carry on anyway. Do not install it yourself. Carry on with the steps below only once they say to carry on; if they update, the report waits for them to come back on the new version. When the check says nothing is available, or cannot tell (an error, no network, `updateAvailable` missing), say nothing about it and go on.
2. Read `ghostex guide` (the overview) and `ghostex guide features` for how the part of Ghostex they describe is meant to work, so the issue says what was expected.
3. Gather what helps us fix it from this computer: the Ghostex version (`ghostex server status --json`), the operating system, the settings that touch this feature (`ghostex settings list`, `ghostex settings get <key>`), the recent gxserver and app logs around it (`ghostex logs --lines 200`, `ghostex logs --grep <text>`), and the sessions involved (`ghostex sessions`).
4. Debug it, and reproduce it where you can do that safely, to find what goes wrong and, if you can, why.
5. Change nothing on this computer: do not change settings, files, projects or sessions, do not restart Ghostex or its background service, and do not install anything. The only files you write are your notes and the draft, in this report's folder: {report_folder}
6. Keep private things out of the issue: no tokens, keys, passwords, cookies, email addresses or account names other than @{username}, and no contents of the user's own files. Shorten paths in their home folder to ~, and quote only the log lines that matter.
7. Draft the issue: a short title, and a body with What happened, What was expected, Steps to reproduce, What you found (Ghostex version, operating system, relevant settings, log excerpts) and the likely cause if you found one.
{posting}
9. Post it: write the body to a file in {report_folder}, then run
   ghostex feedback send --title "<title>" --body-file <file> --github-user {username}{image_flags}
   It adds "Reported by @{username}" at the top so GitHub tells them about the issue, and Ghostex adds a line with its version and the operating system. Run it with --dry-run first if you want to see exactly what will be sent. Then give the user the issue link it prints."#
    )
}

/// Their screenshots as the brief lists them, or that there are none.
pub(super) fn screenshot_lines(images: &[PathBuf]) -> String {
    if images.is_empty() {
        return "They pasted no screenshots.".to_string();
    }
    let list = images
        .iter()
        .enumerate()
        .map(|(index, path)| format!("[Image #{}]({})", index + 1, path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    format!("Their screenshots:\n{list}")
}

/// `feedbackBrief { githubUsername, images?: [{ name, dataBase64 }] }` answers
/// `{ reportFolder, screenshots, brief }`: a new report folder with the screenshots saved in it,
/// and the steps to file the report from there (`ghostex feedback brief`). It writes only that
/// folder; nothing is posted.
pub(crate) fn handle_feedback_brief_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = read_domain_rpc_params(body).and_then(|params| feedback_brief(state, &params));
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn feedback_brief(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let raw = params
        .get("githubUsername")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let username = github_username(raw)
        .map_err(DomainStateError::bad_request)?
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Ask the user for their GitHub username first: the issue mentions them so GitHub tells them about it.",
            )
        })?;
    let images = FeedbackImage::from_params(params).map_err(DomainStateError::bad_request)?;
    let folder = new_report_folder(&state.paths.app_config_dir)?;
    let screenshots = save_screenshots(&folder, &images)?;
    let brief = format!(
        "You are filing a Ghostex bug report or feedback as a GitHub issue on the Ghostex repository, the same way Send Bug / Feedback's \"Collect more data with an agent\" does. What the user wants reported is what they told you in your chat.\n\nTheir GitHub username: @{username}\n\n{}\n\n{}",
        screenshot_lines(&screenshots),
        report_steps(&username, &folder, &screenshots)
    );
    Ok(json!({
        "reportFolder": folder.to_string_lossy(),
        "screenshots": screenshots
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        "brief": brief,
    }))
}
