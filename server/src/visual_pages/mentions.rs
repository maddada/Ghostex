use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::StatusCode;
use ghostex_gx_chat_core::composer::suggestions::agent_display_name;
use ghostex_gx_chat_core::composer::trigger::{
    is_command_entry, linked_skill_mention, skill_detail, Skill,
};
use ghostex_gx_chat_core::transcript::session_mentions::{
    choice_detail, choice_mention, session_choices,
};
use ghostex_gx_chat_core::KnownSession;
use serde_json::{json, Value};

use crate::coordinators::{list_coordinators, main_coordinators};
use crate::domain::DomainRepository;
use crate::server::{read_runtime_text, AppState, RoutedResponse};
use crate::session_chat_agent_tasks::claude_config_directory;
use crate::session_chat_follower::session_chat_display_agent_for_session;
use crate::session_chat_skills::read_session_chat_skills;
use crate::storage::open_gxserver_database;

use super::{
    answers::reply,
    file_page::opened_from_file,
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{is_page_id, read_page, read_record},
};

pub(crate) const VISUAL_PAGE_MENTIONS_SUFFIX: &str = "/mentions";

enum MentionsResult {
    Ready(Value),
    NotFound,
    Unavailable(&'static str),
}

/// Whether `GET <path>` is a page's mentions route, `/visual/<id>/mentions`.
pub(crate) fn is_mentions_route(path: &str) -> bool {
    path.starts_with(VISUAL_PAGE_ROUTE_PREFIX) && path.ends_with(VISUAL_PAGE_MENTIONS_SUFFIX)
}

/// `GET /visual/<id>/mentions`: Boosts, active sibling agent sessions in the publishing project,
/// and skills for the page's publishing session, so Live page note and reply boxes can show
/// compact `*`, `+`, `/` and `$` mention menus that insert the same tokens as the chat composer.
///
/// Answered before the token gate like the page itself, `/draft` and `/image`: the unguessable
/// 128-bit page id is the grant (the sandboxed page has an opaque origin and no auth token, and
/// `peer_http`, Collab guest pages and exported pages never expose `/visual/*`). Only an open,
/// non-retired page published by an active session is answered, matching `/draft` and `/image`,
/// and the payload is scoped to the publishing session's own project and only the fields the
/// picker renders.
pub(crate) async fn read(state: Arc<AppState>, path: String) -> RoutedResponse {
    let Some(id) = page_of(&path) else {
        return no_such_page();
    };
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let payload =
        tokio::task::spawn_blocking(move || build_mentions(&paths, &server_id, &id)).await;
    match payload {
        Ok(MentionsResult::Ready(value)) => reply(StatusCode::OK, value),
        Ok(MentionsResult::NotFound) => no_such_page(),
        Ok(MentionsResult::Unavailable(error)) => {
            reply(StatusCode::CONFLICT, json!({"ok": false, "error": error}))
        }
        Err(_) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok": false, "error": "Mentions could not be loaded."}),
        ),
    }
}

fn build_mentions(
    paths: &crate::paths::GxserverPaths,
    server_id: &str,
    page_id: &str,
) -> MentionsResult {
    let Some(record) = read_record(&paths.root_dir, page_id) else {
        return MentionsResult::NotFound;
    };
    if read_page(&paths.root_dir, page_id).is_none() {
        return MentionsResult::NotFound;
    }
    if record
        .get("answeredAt")
        .is_some_and(|value| !value.is_null())
    {
        return MentionsResult::Unavailable("These answers were already sent.");
    }
    if record
        .get("retiredAt")
        .is_some_and(|value| !value.is_null())
    {
        return MentionsResult::Unavailable("This page is no longer active.");
    }
    if opened_from_file(&record) {
        return MentionsResult::Unavailable("The session that published this page is unknown.");
    }
    let Some((Some(page_project_id), Some(page_session_id))) = record
        .get("sessionRef")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(crate::agent_hooks::probing::parse_global_session_ref)
    else {
        return MentionsResult::Unavailable("The session that published this page is unknown.");
    };

    let Ok(db) = open_gxserver_database(paths) else {
        return MentionsResult::Unavailable("The session that published this page is unavailable.");
    };
    let repository = DomainRepository::new(&db, server_id);
    let Ok(Some(publishing_session)) = repository.get_session(&page_project_id, &page_session_id)
    else {
        return MentionsResult::Unavailable("The session that published this page is unknown.");
    };
    if field(&publishing_session, "lifecycleState") == "stopped" {
        return MentionsResult::Unavailable(
            "The session that published this page is no longer active.",
        );
    }

    let boosts = crate::boosts::mention_choices(paths);
    let coordinators: HashSet<(String, String)> = list_coordinators(&db)
        .unwrap_or_default()
        .into_iter()
        .map(|record| record.key())
        .collect();
    let mains = main_coordinators(&db, &repository);

    // Scope agent mentions strictly to the publishing session's own project so a page URL can
    // never enumerate sessions from other projects on the computer.
    let known: Vec<KnownSession> = repository
        .list_sessions_excluding_stopped(Some(&page_project_id))
        .unwrap_or_default()
        .into_iter()
        .map(|session| {
            let project_id = field(&session, "projectId");
            let session_id = field(&session, "sessionId");
            let agent_id = session_chat_display_agent_for_session(&session)
                .unwrap_or_else(|| field(&session, "agentId"));
            let agent_name = read_runtime_text(&session, "agentName").unwrap_or_default();
            let coordinator = coordinators.contains(&(project_id.clone(), session_id.clone()));
            let main_coordinator = coordinator && mains.get(&project_id) == Some(&session_id);
            let active_at = {
                let last = field(&session, "lastActiveAt");
                if last.is_empty() {
                    field(&session, "updatedAt")
                } else {
                    last
                }
            };
            KnownSession {
                global_ref: field(&session, "globalRef"),
                project_id,
                session_id,
                title: field(&session, "title"),
                agent_id,
                agent_name,
                coordinator,
                main_coordinator,
                activity: field(&session, "activity"),
                lifecycle_state: field(&session, "lifecycleState"),
                active_at,
                computer: String::new(),
                project_name: String::new(),
                offline: false,
            }
        })
        .collect();

    let sessions_out: Vec<Value> = session_choices(&known, &page_project_id, &page_session_id)
        .into_iter()
        .map(|choice| {
            json!({
                "title": choice.title,
                "detail": choice_detail(&choice),
                "working": choice.working,
                "coordinator": choice.coordinator,
                "mainCoordinator": choice.main_coordinator,
                "token": choice_mention(&choice),
            })
        })
        .collect();

    let mut skills_out = Vec::new();
    let mut skills_heading = "Skills".to_string();
    if let Ok(Some(project)) = repository.get_project(&page_project_id) {
        let agent_id =
            session_chat_display_agent_for_session(&publishing_session).unwrap_or_default();
        let project_path =
            abs_path(&publishing_session, "cwd").or_else(|| abs_path(&project, "path"));
        let claude_config_dir = read_runtime_text(&publishing_session, "agentSessionPath")
            .map(|path| claude_config_directory(Some(&path)));
        let raw = read_session_chat_skills(
            paths,
            &agent_id,
            project_path.as_deref(),
            claude_config_dir.as_deref(),
        );
        let resolved_agent = raw
            .get("agentId")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or(&agent_id);
        if let Some(display) = agent_display_name(Some(resolved_agent)) {
            skills_heading = format!("{display} skills");
        }
        let parsed: Vec<Skill> = raw
            .get("skills")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        skills_out = parsed
            .iter()
            .map(|skill| {
                json!({
                    "name": skill.name,
                    "detail": skill_detail(skill),
                    "command": is_command_entry(skill),
                    "token": linked_skill_mention(skill),
                })
            })
            .collect();
    }

    MentionsResult::Ready(json!({
        "ok": true,
        "boosts": boosts,
        "sessions": sessions_out,
        "skills": skills_out,
        "skillsHeading": skills_heading,
    }))
}

fn field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn abs_path(value: &Value, key: &str) -> Option<PathBuf> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| Path::new(path).is_absolute())
        .map(PathBuf::from)
}

fn page_of(path: &str) -> Option<String> {
    path.strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .and_then(|rest| rest.strip_suffix(VISUAL_PAGE_MENTIONS_SUFFIX))
        .filter(|id| is_page_id(id))
        .map(str::to_string)
}

fn no_such_page() -> RoutedResponse {
    reply(
        StatusCode::NOT_FOUND,
        json!({"ok": false, "error": "No such page."}),
    )
}
