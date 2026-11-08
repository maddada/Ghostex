//! A session's links (crate::session_links): adding one by hand or from an agent, removing one,
//! and reading the list.
//!
//! SEE-ALSO: server/src/session_links/ (the rules), server/src/server/session_links_sync.rs (the
//! pass these kick), server/src/ghostex_cli/session_links.rs (`ghostex link`).

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::session_links::{
    parse_link_input, presentation_session_links, session_link_entries, session_link_mentions,
    session_repo, stored_session_links, write_session_links, SessionLinkKind, SessionLinkRef,
    SessionLinkSource, StoredSessionLink,
};

use super::super::session_links_sync::{spawn_session_links_refresh, want_session_links_now};
use super::*;

pub(super) async fn route_session_links_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
        caller,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/addSessionLink" => {
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    let (project, session) = session_of(repository, params)?;
                    let text = text_param(params, "link")?;
                    let kind =
                        params
                            .get("kind")
                            .and_then(Value::as_str)
                            .and_then(|kind| match kind {
                                "pullRequest" => Some(SessionLinkKind::PullRequest),
                                "issue" => Some(SessionLinkKind::Issue),
                                "linear" => Some(SessionLinkKind::Linear),
                                _ => None,
                            });
                    let repo = session_repo(&project, &session).or_else(|| {
                        crate::session_git_status::effective_session_git_cwd(
                            &session,
                            Some(&project),
                        )
                        .and_then(|cwd| crate::work_mode::work_repo_of(&cwd))
                    });
                    let link = parse_link_input(&text, repo.as_deref(), kind).ok_or_else(|| {
                        DomainStateError::bad_request(format!(
                            "\"{text}\" is not a GitHub PR or issue (a URL, #123 or owner/repo#123) or a Linear ID like ABC-123."
                        ))
                    })?;
                    // A number alone says nothing of whether it is a PR or an issue: GitHub does.
                    let link = match (&link, kind) {
                        (SessionLinkRef::Github { .. }, None) if !text.contains("://") => {
                            github_number_kind(link)?
                        }
                        _ => link,
                    };
                    let source = match params.get("source").and_then(Value::as_str) {
                        Some("agent") => SessionLinkSource::AgentReported,
                        _ => SessionLinkSource::User,
                    };
                    let url = match &link {
                        SessionLinkRef::Linear { .. } if text.starts_with("http") => {
                            Some(text.clone())
                        }
                        _ => None,
                    };
                    let (project_id, session_id) = ids_of(params)?;
                    let mut stored = stored_session_links(&session);
                    let added = stored.add_by_hand(StoredSessionLink {
                        link: link.clone(),
                        source,
                        via: None,
                        url,
                        added_at: Some(crate::session_links::now_iso()),
                    });
                    if added {
                        write_session_links(db, &project_id, &session_id, &stored)?;
                        schedule_presentation_session_delta(
                            &state,
                            db,
                            repository,
                            &project_id,
                            &session_id,
                        )?;
                    }
                    Ok(json!({
                        "projectId": project_id,
                        "sessionId": session_id,
                        "key": link.key(),
                        "added": added,
                    }))
                },
            );
            spawn_session_links_refresh(&state);
            response
        }
        "/api/removeSessionLink" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let (project, session) = session_of(repository, params)?;
                let text = text_param(params, "key")?;
                let key = linked_key(&project, &session, &text)?;
                let (project_id, session_id) = ids_of(params)?;
                let mut stored = stored_session_links(&session);
                // A suggestion's × dismisses it (`dismiss_suggestion`); a linked one is removed.
                let linked = session_link_entries(&project, &session)
                    .iter()
                    .any(|entry| entry.link.key() == key);
                let suggested = session_link_mentions(&project_id, &session_id)
                    .into_iter()
                    .find(|link| link.key() == key);
                let removed = match suggested {
                    Some(link) if !linked => stored.dismiss_suggestion(link),
                    _ => stored.remove(&key),
                };
                if removed {
                    write_session_links(db, &project_id, &session_id, &stored)?;
                    schedule_presentation_session_delta(
                        &state,
                        db,
                        repository,
                        &project_id,
                        &session_id,
                    )?;
                }
                Ok(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "key": key,
                    "removed": removed,
                }))
            },
        ),
        // Pins or unpins one of the session's links (`pinned`, true by default); pinned links lead
        // every list and always show in the sidebar (CDXC:SessionLinks 2026-10-24 in
        // server/src/session_links/store.rs).
        "/api/pinSessionLink" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let (project, session) = session_of(repository, params)?;
                let text = text_param(params, "key")?;
                let key = linked_key(&project, &session, &text)?;
                let pinned = params
                    .get("pinned")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let (project_id, session_id) = ids_of(params)?;
                let mut stored = stored_session_links(&session);
                let changed = stored.set_pinned(&key, pinned);
                if changed {
                    write_session_links(db, &project_id, &session_id, &stored)?;
                    schedule_presentation_session_delta(
                        &state,
                        db,
                        repository,
                        &project_id,
                        &session_id,
                    )?;
                }
                Ok(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "key": key,
                    "pinned": pinned,
                    "changed": changed,
                }))
            },
        ),
        // A removed link back in the session's links, as it was; from then on it can join by
        // itself again (CDXC:SessionLinks 2026-10-24 in server/src/session_links/store.rs).
        "/api/restoreSessionLink" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, db, params, _| {
                let (project, session) = session_of(repository, params)?;
                let text = text_param(params, "key")?;
                let stored = stored_session_links(&session);
                // A key, or the link as a person typed it: `#283` is whichever removed link has
                // that number.
                let key = match SessionLinkRef::from_key(&text) {
                    Some(link) => link.key(),
                    None => {
                        let repo = session_repo(&project, &session);
                        [
                            None,
                            Some(SessionLinkKind::Issue),
                            Some(SessionLinkKind::Linear),
                        ]
                        .into_iter()
                        .filter_map(|kind| parse_link_input(&text, repo.as_deref(), kind))
                        .map(|link| link.key())
                        .find(|key| stored.is_removed(key))
                        .ok_or_else(|| {
                            DomainStateError::bad_request(format!(
                                "\"{text}\" names none of this session's removed links."
                            ))
                        })?
                    }
                };
                let (project_id, session_id) = ids_of(params)?;
                let mut stored = stored;
                // A dismissed suggestion goes back under Suggested, not among the links.
                let was_suggestion = stored.is_dismissed_suggestion(&key);
                let mut restored = stored.restore(&key);
                if restored {
                    write_session_links(db, &project_id, &session_id, &stored)?;
                }
                // A link that came only from the branch or the chat comes back by itself now; one
                // that nothing names any more is put back as the person's own.
                let session = repository
                    .get_session(&project_id, &session_id)?
                    .unwrap_or(session);
                let present = session_link_entries(&project, &session)
                    .iter()
                    .any(|entry| entry.link.key() == key);
                if !present && !was_suggestion {
                    // A ticket command's ticket is never stored by hand here (CDXC:SessionLinks
                    // 2026-11-03 in server/src/session_links/reference.rs).
                    if let Some(link) = SessionLinkRef::from_key(&key)
                        .filter(|link| !matches!(link, SessionLinkRef::Command { .. }))
                    {
                        restored |= stored.add_by_hand(StoredSessionLink {
                            link,
                            source: SessionLinkSource::User,
                            via: None,
                            url: None,
                            added_at: Some(crate::session_links::now_iso()),
                        });
                        write_session_links(db, &project_id, &session_id, &stored)?;
                    }
                }
                if restored {
                    schedule_presentation_session_delta(
                        &state,
                        db,
                        repository,
                        &project_id,
                        &session_id,
                    )?;
                }
                Ok(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "key": key,
                    "restored": restored,
                }))
            },
        ),
        // A session's links list opened (its card, the header's "+N" list, the phone's sheet):
        // its links are read at once, within the pass's budget (work_mode/github_batch.rs).
        "/api/refreshSessionLinks" => {
            let asked = std::sync::Mutex::new(None);
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |_, _, params, _| {
                    let (project_id, session_id) = ids_of(params)?;
                    if let Ok(mut asked) = asked.lock() {
                        *asked = Some((project_id.clone(), session_id.clone()));
                    }
                    Ok(json!({ "projectId": project_id, "sessionId": session_id }))
                },
            );
            if let Some((project_id, session_id)) = asked.into_inner().ok().flatten() {
                want_session_links_now(&state, &project_id, &session_id);
            }
            response
        }
        "/api/readSessionLinks" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| {
                let (project, session) = session_of(repository, params)?;
                let (project_id, session_id) = ids_of(params)?;
                let links = presentation_session_links(&project, &session)
                    .unwrap_or_else(|| json!({ "items": [] }));
                Ok(json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "links": links,
                }))
            },
        ),
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
                caller,
            })
        }
    })
}

fn text_param(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}

fn ids_of(params: &Map<String, Value>) -> Result<(String, String), DomainStateError> {
    Ok((
        text_param(params, "projectId")?,
        text_param(params, "sessionId")?,
    ))
}

fn session_of(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<(Value, Value), DomainStateError> {
    let (project_id, session_id) = ids_of(params)?;
    let project = repository
        .get_project(&project_id)?
        .ok_or_else(|| DomainStateError::bad_request("No such project."))?;
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::bad_request("No such session."))?;
    Ok((project, session))
}

/// A key as clients send it, or one of the session's links as a person typed it: `#283` is
/// whichever of them has that number, a PR or an issue.
/// A GitHub link typed as a number (`#123`, `owner/repo#123`), as the PR or issue GitHub says it
/// is. Without `gh` it stays what `parse_link_input` guessed (a PR).
///
/// CDXC:SessionLinks 2026-10-27 DECISION:
/// User: "also i need a button to add 1 manually (just write SPX-123 or #123 or paste a link to link it)". A bare number is looked up instead of assumed to be a PR, and one GitHub does not know is refused with a short message the add field shows.
fn github_number_kind(link: SessionLinkRef) -> Result<SessionLinkRef, DomainStateError> {
    let SessionLinkRef::Github { repo, number, .. } = &link else {
        return Ok(link);
    };
    if !crate::session_git_status::gh_cli_is_available() {
        return Ok(link);
    }
    let path = format!("repos/{repo}/issues/{number}");
    let answer = crate::session_git_status::run_gh_command(
        None,
        &[
            "api",
            &path,
            "--jq",
            "if .pull_request then \"pr\" else \"issue\" end",
        ],
    );
    let kind = match answer.as_deref().map(str::trim) {
        Some("pr") => SessionLinkKind::PullRequest,
        Some("issue") => SessionLinkKind::Issue,
        _ => {
            return Err(DomainStateError::bad_request(format!(
                "#{number} is not a PR or issue in {repo}."
            )))
        }
    };
    Ok(SessionLinkRef::Github {
        kind,
        repo: repo.clone(),
        number: *number,
    })
}

fn linked_key(project: &Value, session: &Value, text: &str) -> Result<String, DomainStateError> {
    if let Some(link) = SessionLinkRef::from_key(text) {
        return Ok(link.key());
    }
    let repo = session_repo(project, session);
    let linked: Vec<String> = session_link_entries(project, session)
        .into_iter()
        .map(|entry| entry.link.key())
        .collect();
    [
        None,
        Some(SessionLinkKind::Issue),
        Some(SessionLinkKind::Linear),
    ]
    .into_iter()
    .filter_map(|kind| parse_link_input(text, repo.as_deref(), kind))
    .map(|link| link.key())
    .find(|key| linked.contains(key))
    .ok_or_else(|| {
        DomainStateError::bad_request(format!("\"{text}\" names none of this session's links."))
    })
}
