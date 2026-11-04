//! The Work page's Link to current session: `setSessionWorkLinks` with `source` + `ticketId` adds
//! that ticket to the session's links and answers with the request that takes it back.

use serde_json::{json, Map, Value};

use super::*;

/// What gxserver merges into the session's links and what it answers as `undo`, for a session on
/// a branch that names no ticket.
fn add(
    session_id: &str,
    request: Value,
) -> Result<(Map<String, Value>, Map<String, Value>), String> {
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let project_id = format!("PLinkAdd{session_id}");
    let cwd = format!("/gx-char/link-add/{session_id}");
    set_branch(&cwd, "leandro/tidy-up");
    let mut params = json!({ "projectId": project_id, "sessionId": session_id });
    params
        .as_object_mut()
        .unwrap()
        .extend(request.as_object().unwrap().clone());
    let added = expand_added_work_links(
        &project(&project_id, &cwd),
        &session(&project_id, session_id, Value::Null),
        None,
        params.as_object().unwrap(),
    )
    .map_err(|error| error.message)?
    .expect("a ticket is an add");
    Ok((added.request, added.undo))
}

#[test]
fn a_github_ticket_links_its_issue() {
    let (request, undo) = add("Github", json!({ "source": "github", "ticketId": "6" })).unwrap();
    assert_eq!(
        Value::Object(request),
        json!({ "projectId": "PLinkAddGithub", "sessionId": "Github", "githubIssues": [6] })
    );
    assert_eq!(Value::Object(undo), json!({ "githubIssues": null }));
}

#[test]
fn a_linear_ticket_links_its_identifier() {
    let (request, undo) =
        add("Linear", json!({ "source": "linear", "ticketId": "ACM-1" })).unwrap();
    assert_eq!(
        Value::Object(request),
        json!({ "projectId": "PLinkAddLinear", "sessionId": "Linear", "linearIssues": ["ACM-1"] })
    );
    assert_eq!(Value::Object(undo), json!({ "linearIssues": null }));
}

/// A ticket with no source is refused here. One whose source isn't Linear or GitHub is a ticket
/// command's: it goes on, as typed, to the check under the locks (`checked_ticket_links`), which
/// refuses a source the session's workspace doesn't pin in its fixed words.
#[test]
fn a_ticket_of_a_missing_source_is_refused_and_another_goes_to_the_check() {
    assert_eq!(
        add("Unknown", json!({ "ticketId": "ACM-1" })).unwrap_err(),
        "source must be linear or github."
    );
    let (request, undo) = add(
        "Unknown",
        json!({ "source": " jira ", "ticketId": " acm-1" }),
    )
    .unwrap();
    assert_eq!(request["tickets"], json!({ "jira": [" acm-1"] }));
    assert_eq!(Value::Object(undo), json!({ "tickets": { "jira": null } }));
}

/// A source is read as `WorkSourceId::parse` reads one, spaces around it included.
#[test]
fn a_built_in_source_with_spaces_is_still_linear() {
    let (request, _) = add(
        "Spaced",
        json!({ "source": " linear ", "ticketId": "ACM-1" }),
    )
    .unwrap();
    assert_eq!(request["linearIssues"], json!(["ACM-1"]));
}
