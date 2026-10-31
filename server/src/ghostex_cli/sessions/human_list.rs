use std::collections::HashMap;

#[cfg(test)]
use serde_json::json;
use serde_json::Value;

#[cfg(test)]
use crate::ghostex_cli::args::Flags;

use super::*;

const QUICK_TERMINALS_PROJECT_NAME: &str = "Quick Terminals";

// ---------------------------------------------------------------------------
// human session list printing (private ports of printSessionList helpers)
// ---------------------------------------------------------------------------

struct SessionProjectGroup<'a> {
    project_name: String,
    project_path: String,
    sessions: Vec<&'a Value>,
}

pub(super) fn print_session_list(sessions: &[Value], grouped: bool) {
    if sessions.is_empty() {
        println!("No running terminal sessions.");
        return;
    }
    /*
     * CDXC:Cli 2026-05-20-12:20:
     * Group by project with the project path as the section header, print each
     * session as a short two-line block without field labels, and preserve the
     * sidebar order returned by the native inventory.
     */
    let project_groups = group_sessions_preserving_sidebar_order(sessions);
    if !grouped {
        for project in &project_groups {
            for session in &project.sessions {
                println!(
                    "{}",
                    format_compact_session_line(session, Some(&project.project_name))
                );
            }
        }
        return;
    }
    for (project_index, project) in project_groups.iter().enumerate() {
        if project_index > 0 {
            println!();
        }
        println!("{}", project.project_name);
        if !project.project_path.is_empty() {
            println!("{}", project.project_path);
        }
        for session in &project.sessions {
            println!("{}", format_compact_session_line(session, None));
        }
    }
}

fn group_sessions_preserving_sidebar_order(sessions: &[Value]) -> Vec<SessionProjectGroup<'_>> {
    let mut groups: Vec<SessionProjectGroup> = Vec::new();
    let mut group_index_by_project: HashMap<Option<String>, usize> = HashMap::new();
    for session in sessions {
        let key = value_key(session.get("projectId"));
        let group_index = match group_index_by_project.get(&key) {
            Some(index) => *index,
            None => {
                let is_first_group = groups.is_empty();
                groups.push(SessionProjectGroup {
                    project_name: resolve_session_picker_project_name(session, is_first_group),
                    project_path: match session.get("projectPath") {
                        Some(value) if js_truthy(Some(value)) => js_display(value),
                        _ => String::new(),
                    },
                    sessions: Vec::new(),
                });
                group_index_by_project.insert(key, groups.len() - 1);
                groups.len() - 1
            }
        };
        groups[group_index].sessions.push(session);
    }
    groups
}

fn resolve_session_picker_project_name(session: &Value, is_first_group: bool) -> String {
    if is_first_group && js_string(session.get("projectPath")).trim().is_empty() {
        return QUICK_TERMINALS_PROJECT_NAME.to_string();
    }
    if js_truthy(session.get("projectName")) {
        return js_display(session.get("projectName").expect("truthy value"));
    }
    if js_truthy(session.get("projectPath")) {
        return js_display(session.get("projectPath").expect("truthy value"));
    }
    QUICK_TERMINALS_PROJECT_NAME.to_string()
}

fn format_compact_session_line(session: &Value, project_label: Option<&str>) -> String {
    let marker = if js_truthy(session.get("isFocused")) {
        "›"
    } else {
        " "
    };
    let title = match js_coalesce(&[session.get("displayTitle")])
        .filter(|value| js_truthy(Some(*value)))
        .or_else(|| session.get("title").filter(|value| js_truthy(Some(*value))))
    {
        Some(value) => js_display(value),
        None => "-".to_string(),
    };
    let alias = js_template(session.get("alias"));
    let headline = match project_label {
        Some(label) => format!("{marker} #{alias}  {label} · {title}"),
        None => format!("{marker} #{alias}  {title}"),
    };
    let details: Vec<String> = [
        js_string(session.get("agent")),
        format_compact_provider(session).unwrap_or_default(),
        js_string(session.get("status")),
        format_awaiting_approval(session.get("awaitingApproval")),
        format_pending_questions(session.get("pendingQuestionCount")),
        format_active_time(session.get("lastInteractionAt")),
    ]
    .iter()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty() && value != "-")
    .collect();
    if details.is_empty() {
        return headline;
    }
    format!("{headline}\n    {}", details.join(" · "))
}

/// The sidebar's red approval dot in words: the agent is stuck on a permission prompt.
fn format_awaiting_approval(value: Option<&Value>) -> String {
    if value.and_then(Value::as_bool) == Some(true) {
        "waiting for approval".to_string()
    } else {
        String::new()
    }
}

/// The sidebar's pink question dot in words: questions the agent asked that wait for an answer.
fn format_pending_questions(value: Option<&Value>) -> String {
    match value.and_then(Value::as_u64).unwrap_or(0) {
        0 => String::new(),
        count => format!("answer requested ({count})"),
    }
}

fn format_compact_provider(session: &Value) -> Option<String> {
    let provider = session
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if provider.is_empty() {
        return None;
    }
    let provider_session_name = session
        .get("providerSessionName")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !provider_session_name.is_empty() {
        return Some(format!("{provider}/{provider_session_name}"));
    }
    Some(provider.to_string())
}

fn format_active_time(value: Option<&Value>) -> String {
    let Some(timestamp_ms) = parse_js_date_ms(&js_string(value)) else {
        return "-".to_string();
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let seconds = (((now_ms - timestamp_ms) as f64) / 1000.0).round().max(0.0);
    if seconds < 60.0 {
        return format!("{}s ago", seconds as i64);
    }
    let minutes = (seconds / 60.0).round();
    if minutes < 60.0 {
        return format!("{}m ago", minutes as i64);
    }
    let hours = (minutes / 60.0).round();
    if hours < 48.0 {
        return format!("{}h ago", hours as i64);
    }
    let days = (hours / 24.0).round();
    format!("{}d ago", days as i64)
}

/// Date.parse() for the ISO shapes gxserver emits (RFC 3339, date-only, or a
/// naive datetime treated as UTC). Anything else is NaN -> None.
fn parse_js_date_ms(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.timestamp_millis());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis());
    }
    if let Ok(datetime) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(datetime.and_utc().timestamp_millis());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_cli_session_overlays_presentation_and_maps_status() {
        let session = json!({
            "sessionId": "G1",
            "projectId": "P1",
            "lifecycleState": "sleeping",
            "zmxName": "g-1",
            "agentId": "claude",
            "title": "Raw",
            "createdAt": "c1",
            "updatedAt": "2026-01-01T00:00:00Z",
            "runtimeSettings": { "agentSessionId": "  abc  " },
            "providerState": { "lifecycleState": "missing" },
            "isFavorite": true,
        });
        let project = json!({ "projectId": "P1", "name": "Alpha", "path": "/a" });
        let presentation = json!({
            "title": "Pres",
            "displayTitle": "● Pres",
            "activity": "Working",
            "agentIcon": "claude-icon",
            "zmxName": "g-1p",
            "cwd": "/cwd",
            "workingStartedAt": "2026-01-01T00:00:05Z",
        });
        let cli = to_cli_session(&session, Some(&project), 0, Some(&presentation), Some(3));
        assert_eq!(cli["status"], json!("sleep"));
        assert_eq!(cli["isSleeping"], json!(true));
        assert_eq!(cli["isLive"], json!(false));
        assert_eq!(cli["alias"], json!(1));
        assert_eq!(cli["agent"], json!("claude"));
        assert_eq!(cli["agentIcon"], json!("claude-icon"));
        assert_eq!(cli["agentSessionId"], json!("abc"));
        assert_eq!(cli["agentSessionPath"], Value::Null);
        assert_eq!(cli["title"], json!("Pres"));
        assert_eq!(cli["displayTitle"], json!("● Pres"));
        assert_eq!(cli["displayTitleTooltip"], json!("● Pres"));
        assert_eq!(cli["projectPath"], json!("/cwd"));
        assert_eq!(cli["projectName"], json!("Alpha"));
        assert_eq!(cli["provider"], json!("zmx"));
        assert_eq!(cli["providerSessionName"], json!("g-1"));
        assert_eq!(cli["sessionPersistenceName"], json!("g-1"));
        assert_eq!(cli["providerSessionState"], json!("missing"));
        assert_eq!(cli["sortOrder"], json!(3));
        assert_eq!(cli["zmxName"], json!("g-1p"));
        assert_eq!(cli["activity"], json!("working"));
        assert_eq!(cli["ownership"], json!("gxserver"));
        assert_eq!(cli["isFocused"], json!(false));
        assert_eq!(cli["isLocalOnly"], json!(false));
        assert_eq!(cli["isFavorite"], json!(true));
        assert_eq!(cli["lastInteractionAt"], json!("2026-01-01T00:00:00Z"));
        assert_eq!(cli["workingStartedAt"], json!("2026-01-01T00:00:05Z"));
        // undefined presentation-only fields are dropped like JSON.stringify does
        assert!(cli.get("actions").is_none());
        assert!(cli.get("attention").is_none());
        assert!(cli.get("groupId").is_none());
    }

    #[test]
    fn to_cli_session_status_fallbacks_and_empty_project_path() {
        let session = json!({ "sessionId": "G2", "projectId": "P1" });
        let cli = to_cli_session(&session, None, 4, None, None);
        assert_eq!(cli["status"], json!("unknown"));
        assert_eq!(cli["alias"], json!(5));
        assert_eq!(cli["projectPath"], json!(""));
        assert_eq!(cli["projectName"], json!("P1"));
        assert_eq!(cli["activity"], json!("idle"));
        assert!(cli.get("sortOrder").is_none());
        assert!(cli.get("providerSessionState").is_none());
        // Without a presentation row (the persisted fallback) there is no stint start to forward.
        assert!(cli.get("workingStartedAt").is_none());
        let running = json!({ "sessionId": "G3", "projectId": "P1", "lifecycleState": "running" });
        let cli = to_cli_session(&running, None, 0, None, None);
        assert_eq!(cli["status"], json!("running"));
        assert_eq!(cli["isLive"], json!(true));
        let provider_only = json!({
            "sessionId": "G4",
            "projectId": "P1",
            "providerState": { "lifecycleState": "exists" },
        });
        let cli = to_cli_session(&provider_only, None, 0, None, None);
        assert_eq!(cli["status"], json!("exists"));
        assert_eq!(cli["isLive"], json!(true));
    }

    #[test]
    fn normalize_cli_session_activity_matches_node() {
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("Attention"))),
            "attention"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("needs_attention"))),
            "attention"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!(" attention required "))),
            "attention"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("BUSY"))),
            "working"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("processing"))),
            "working"
        );
        assert_eq!(
            normalize_cli_session_activity(Some(&json!("something"))),
            "idle"
        );
        assert_eq!(normalize_cli_session_activity(None), "idle");
    }

    #[test]
    fn string_flag_matches_node() {
        assert_eq!(string_flag(None), Value::Null);
        assert_eq!(string_flag(Some(&Value::Null)), Value::Null);
        assert_eq!(string_flag(Some(&json!("  x "))), json!("x"));
        assert_eq!(string_flag(Some(&json!("   "))), Value::Null);
        assert_eq!(string_flag(Some(&json!(5))), json!("5"));
        assert_eq!(string_flag(Some(&json!(true))), json!("true"));
    }

    #[test]
    fn configured_mobile_quick_action_rules() {
        assert!(is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "actionType": "terminal", "command": "make test",
        })));
        assert!(is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "command": "ls",
        })));
        assert!(is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "actionType": "browser", "url": "https://x",
        })));
        assert!(!is_configured_mobile_quick_action(&json!({
            "commandId": "", "command": "ls",
        })));
        assert!(!is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "actionType": "browser", "url": "   ",
        })));
        assert!(!is_configured_mobile_quick_action(&json!({
            "commandId": "c1", "actionType": "terminal", "command": "  ",
        })));
        assert!(!is_configured_mobile_quick_action(
            &json!({ "command": "ls" })
        ));
    }

    #[test]
    fn mobile_session_list_shape_and_ordering() {
        let result = json!({
            "ok": true,
            "product": "gxserver",
            "revision": "r1",
            "projects": [
                { "projectId": "P1", "name": "Alpha", "path": "/a" },
                { "projectId": "P2", "name": "Hidden", "path": "/h", "visibility": "hidden" },
            ],
            "sessions": [
                {
                    "sessionId": "G2", "projectId": "P1", "title": "Two", "alias": 2,
                    "sortOrder": 5, "isFocused": false, "isLive": true, "isSleeping": false,
                    "status": "running", "provider": "zmx", "providerSessionName": "g-2",
                    "activity": "idle", "projectName": "Alpha", "projectPath": "/a",
                    "agent": null, "agentId": "claude",
                },
                { "sessionId": "G3", "projectId": "P2", "title": "Ghost" },
                {
                    "sessionId": "G4", "projectId": "P1", "title": "Four", "sortOrder": 6,
                    "status": "running", "activity": "working",
                    "workingStartedAt": "2026-07-01T00:00:00.000Z",
                },
                {
                    "sessionId": "G5", "projectId": "P1", "title": "Five", "sortOrder": 7,
                    "status": "running", "activity": "attention",
                    "attention": {
                        "acknowledged": false, "enteredAt": "2026-07-01T00:01:00.000Z",
                        "eventId": "attn_5", "kind": "finished",
                        "workingStartedAt": "2026-07-01T00:00:30.000Z", "notificationId": "row-5",
                    },
                },
                {
                    "sessionId": "G1", "projectId": "P1", "title": "One", "alias": 1,
                    "sortOrder": 1, "status": "sleep", "isLive": false, "isSleeping": true,
                    "isFocused": false, "provider": "zmx", "updatedAt": "u1",
                },
            ],
            "capabilities": { "attentionNotificationId": true, "sessionSettlement": true },
            "workspaceGroups": {
                "projectOrder": ["P1", ""],
                "projects": {
                    "P1": { "groups": [ { "groupId": "g1", "sessionIds": ["G1", ""], "title": "" } ] },
                    "P2": { "groups": [ { "groupId": "", "sessionIds": [] } ] },
                },
            },
        });
        let mobile = to_mobile_session_list(&result, &serde_json::Map::new());
        assert_eq!(
            mobile,
            json!({
                "capabilities": { "attentionNotificationId": true, "sessionSettlement": true },
                "ok": true,
                "product": "gxserver",
                "revision": "r1",
                "projects": [ { "name": "Alpha", "path": "/a", "projectId": "P1" } ],
                "recentProjects": [],
                "sessions": [
                    {
                        "sessionId": "G1", "projectId": "P1", "title": "One",
                        "displayTitle": "One", "alias": 1, "sortOrder": 1, "status": "sleep",
                        "isLive": false, "isSleeping": true, "isFocused": false,
                        "provider": "zmx", "lastInteractionAt": "u1",
                    },
                    {
                        "sessionId": "G2", "projectId": "P1", "title": "Two",
                        "displayTitle": "Two", "alias": 2, "sortOrder": 5, "status": "running",
                        "isLive": true, "isSleeping": false, "isFocused": false,
                        "provider": "zmx", "providerSessionName": "g-2", "activity": "idle",
                        "projectName": "Alpha", "projectPath": "/a", "agent": "claude",
                    },
                    {
                        "sessionId": "G4", "projectId": "P1", "title": "Four",
                        "displayTitle": "Four", "sortOrder": 6, "status": "running",
                        "activity": "working", "workingStartedAt": "2026-07-01T00:00:00.000Z",
                    },
                    {
                        "sessionId": "G5", "projectId": "P1", "title": "Five",
                        "displayTitle": "Five", "sortOrder": 7, "status": "running",
                        "activity": "attention",
                        "attention": {
                            "acknowledged": false, "enteredAt": "2026-07-01T00:01:00.000Z",
                            "eventId": "attn_5", "kind": "finished",
                            "workingStartedAt": "2026-07-01T00:00:30.000Z",
                            "notificationId": "row-5",
                        },
                    },
                ],
                "workspaceGroups": {
                    "projectOrder": ["P1"],
                    "projects": {
                        "P1": { "groups": [ { "groupId": "g1", "sessionIds": ["G1"], "title": "g1" } ] },
                    },
                },
            })
        );
    }

    #[test]
    fn mobile_workspace_groups_empty_becomes_undefined() {
        assert_eq!(to_mobile_workspace_groups(None), None);
        assert_eq!(to_mobile_workspace_groups(Some(&Value::Null)), None);
        assert_eq!(to_mobile_workspace_groups(Some(&json!("x"))), None);
        assert_eq!(
            to_mobile_workspace_groups(Some(&json!({ "projectOrder": [], "projects": {} }))),
            None
        );
    }

    #[test]
    fn mobile_sidebar_project_collections_shape() {
        let mobile = to_mobile_sidebar_project_collections(Some(&json!({
            "collections": {
                "c1": {
                    "collapsed": true,
                    "collectionId": "c1",
                    "color": "#7c6df2",
                    "projectIds": ["P1", ""],
                    "title": "Group 1",
                },
                "c2": { "projectIds": ["P2"] },
                "c3": { "projectIds": [] },
            },
            "nextCollectionNumber": 7,
            "order": ["c2", "ghost", "c1"],
        })));
        assert_eq!(
            mobile,
            Some(json!({
                "collections": {
                    "c1": {
                        "collectionId": "c1",
                        "color": "#7c6df2",
                        "projectIds": ["P1"],
                        "title": "Group 1",
                    },
                    "c2": {
                        "collectionId": "c2",
                        "color": "transparent",
                        "projectIds": ["P2"],
                        "title": "c2",
                    },
                },
                "nextCollectionNumber": 7,
                "order": ["c2", "c1"],
            }))
        );
    }

    #[test]
    fn mobile_sidebar_project_collections_empty_becomes_undefined() {
        assert_eq!(to_mobile_sidebar_project_collections(None), None);
        assert_eq!(
            to_mobile_sidebar_project_collections(Some(&Value::Null)),
            None
        );
        assert_eq!(
            to_mobile_sidebar_project_collections(Some(&json!("x"))),
            None
        );
        assert_eq!(
            to_mobile_sidebar_project_collections(Some(&json!({
                "collections": {},
                "nextCollectionNumber": 1,
                "order": [],
            }))),
            None
        );
    }

    #[test]
    fn persisted_inventory_sql_row_mapping() {
        let connection = rusqlite::Connection::open_in_memory().expect("open");
        connection
            .execute_batch(
                "CREATE TABLE projects (projectId TEXT, name TEXT, path TEXT, isRecentProject INTEGER, visibility TEXT, systemKind TEXT);
                 CREATE TABLE sessions (projectId TEXT, sessionId TEXT, kind TEXT, title TEXT, lifecycleState TEXT, providerStateJson TEXT, zmxName TEXT, cwd TEXT, agentId TEXT, updatedAt TEXT, lastActiveAt TEXT);
                 INSERT INTO projects VALUES ('P1', 'Alpha', '/a', 0, 'visible', NULL);
                 INSERT INTO projects VALUES ('P2', 'Hidden', '/h', 0, 'hidden', NULL);
                 INSERT INTO projects VALUES ('P3', 'Recent', '/r', 1, 'visible', NULL);
                 INSERT INTO sessions VALUES ('P1', 'G1', 'terminal', 'One', 'sleeping', '{\"lifecycleState\":\"missing\",\"zmxName\":\"g-1\"}', 'g-1', '/a/w', 'claude', '2026-01-02T00:00:00Z', '2026-01-01T00:00:00Z');
                 INSERT INTO sessions VALUES ('P1', 'G2', 'terminal', 'Stopped', 'stopped', NULL, NULL, NULL, NULL, '2026-01-01T00:00:00Z', NULL);
                 INSERT INTO sessions VALUES ('P2', 'G3', 'terminal', 'Ghost', 'running', NULL, NULL, NULL, NULL, '2026-01-01T00:00:00Z', NULL);",
            )
            .expect("seed");
        let rows = read_persisted_gxserver_inventory_rows_from(&connection, true).expect("rows");
        assert_eq!(
            rows.iter()
                .filter(|row| row["rowType"] == json!("project"))
                .count(),
            3
        );
        let list =
            build_persisted_gxserver_session_list(&rows, Some("S1"), "boom", &Flags::default());
        assert_eq!(list["error"], json!("boom"));
        assert_eq!(list["fallback"], json!("persisted-gxserver-state"));
        assert_eq!(list["ok"], json!(true));
        let projects = list["projects"].as_array().expect("projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["projectId"], json!("P1"));
        assert_eq!(projects[0]["visibility"], json!("visible"));
        assert_eq!(projects[0]["isRecentProject"], json!(false));
        let sessions = list["sessions"].as_array().expect("sessions");
        assert_eq!(sessions.len(), 1); // stopped + hidden-project rows filtered
        assert_eq!(sessions[0]["sessionId"], json!("G1"));
        assert_eq!(sessions[0]["globalRef"], json!("S1:P1:G1"));
        assert_eq!(sessions[0]["status"], json!("sleep"));
        assert_eq!(sessions[0]["providerSessionState"], json!("missing"));
        assert_eq!(sessions[0]["providerSessionName"], json!("g-1"));
        assert_eq!(sessions[0]["projectPath"], json!("/a/w"));

        // --all keeps the stopped row
        let mut all_flags = Flags::default();
        all_flags.insert_bool("all", true);
        let list = build_persisted_gxserver_session_list(&rows, None, "boom", &all_flags);
        assert_eq!(list["sessions"].as_array().expect("sessions").len(), 2);
        assert!(list["sessions"][0].get("globalRef").is_none());
    }

    #[test]
    fn persisted_inventory_visibility_column_fallback() {
        let connection = rusqlite::Connection::open_in_memory().expect("open");
        connection
            .execute_batch(
                "CREATE TABLE projects (projectId TEXT, name TEXT, path TEXT, isRecentProject INTEGER);
                 CREATE TABLE sessions (projectId TEXT, sessionId TEXT, kind TEXT, title TEXT, lifecycleState TEXT, providerStateJson TEXT, zmxName TEXT, cwd TEXT, agentId TEXT, updatedAt TEXT, lastActiveAt TEXT);
                 INSERT INTO projects VALUES ('P1', 'Alpha', '/a', 0);",
            )
            .expect("seed");
        assert!(read_persisted_gxserver_inventory_rows_from(&connection, true).is_none());
        let rows = read_persisted_gxserver_inventory_rows_from(&connection, false).expect("rows");
        let project = rows
            .iter()
            .find(|row| row["rowType"] == json!("project"))
            .expect("project row");
        assert_eq!(project["visibility"], json!("visible"));
        assert_eq!(project["systemKind"], Value::Null);
    }

    #[test]
    fn attach_metadata_apply_and_guards() {
        let session = json!({
            "sessionId": "G1", "projectId": "P1", "title": "One", "status": "sleep",
            "projectPath": "/old", "provider": "zmx", "providerSessionName": "g-old",
        });
        // no attach metadata -> unchanged
        let unchanged = apply_attach_metadata_to_cli_session(&session, None).expect("ok");
        assert_eq!(unchanged, session);
        // restoreBlocked -> error with cwd suffix
        let blocked = json!({ "restoreBlocked": { "cwd": "/gone" } });
        let error =
            apply_attach_metadata_to_cli_session(&session, Some(&blocked)).expect_err("err");
        assert_eq!(
            error.to_string(),
            "Session One cannot be restored because its cwd is missing (/gone)."
        );
        // exists provider -> running
        let attach = json!({
            "attachCommand": "zmx attach g-new",
            "cwd": "/new",
            "provider": "zmx",
            "zmxName": "g-new",
            "providerState": { "lifecycleState": "exists" },
        });
        let applied = apply_attach_metadata_to_cli_session(&session, Some(&attach)).expect("ok");
        assert_eq!(applied["attachCommand"], json!("zmx attach g-new"));
        assert_eq!(applied["projectPath"], json!("/new"));
        assert_eq!(applied["providerSessionName"], json!("g-new"));
        assert_eq!(applied["status"], json!("running"));
        assert!(applied.get("resumeCommand").is_none());
        // resume command -> sleep
        let attach = json!({ "startupText": "claude --resume abc\r\r" });
        let applied = apply_attach_metadata_to_cli_session(&session, Some(&attach)).expect("ok");
        assert_eq!(applied["resumeCommand"], json!("claude --resume abc"));
        assert_eq!(applied["status"], json!("sleep"));
        assert!(applied.get("attachCommand").is_none());
    }

    #[test]
    fn should_start_missing_provider_rules() {
        assert!(should_start_missing_provider_for_cli_attach(Some(&json!({
            "provider": "zmx", "providerState": { "lifecycleState": "missing" },
        }))));
        assert!(!should_start_missing_provider_for_cli_attach(None));
        assert!(!should_start_missing_provider_for_cli_attach(Some(
            &Value::Null
        )));
        assert!(!should_start_missing_provider_for_cli_attach(Some(
            &json!({
                "provider": "zmx",
                "providerState": { "lifecycleState": "missing" },
                "restoreBlocked": { "cwd": "/gone" },
            })
        )));
        assert!(!should_start_missing_provider_for_cli_attach(Some(
            &json!({
                "provider": "tmux", "providerState": { "lifecycleState": "missing" },
            })
        )));
        assert!(!should_start_missing_provider_for_cli_attach(Some(
            &json!({
                "provider": "zmx", "providerState": { "lifecycleState": "exists" },
            })
        )));
    }

    #[test]
    fn normalize_startup_text_matches_node() {
        assert_eq!(normalize_startup_text_for_shell(None), None);
        assert_eq!(normalize_startup_text_for_shell(Some(&json!("  "))), None);
        assert_eq!(
            normalize_startup_text_for_shell(Some(&json!("run me\r\r"))),
            Some("run me".to_string())
        );
        assert_eq!(
            normalize_startup_text_for_shell(Some(&json!(" x "))),
            Some("x".to_string())
        );
    }

    #[test]
    fn compact_session_line_formatting() {
        let session = json!({
            "alias": 2, "isFocused": true, "displayTitle": "Fix bug", "title": "raw",
            "agent": "claude", "provider": "zmx", "providerSessionName": "g-2",
            "status": "running",
        });
        assert_eq!(
            format_compact_session_line(&session, None),
            "› #2  Fix bug\n    claude · zmx/g-2 · running"
        );
        assert_eq!(
            format_compact_session_line(&session, Some("Alpha")),
            "› #2  Alpha · Fix bug\n    claude · zmx/g-2 · running"
        );
        let bare = json!({ "alias": 1 });
        assert_eq!(format_compact_session_line(&bare, None), "  #1  -");
    }

    #[test]
    fn group_sessions_first_group_quick_terminals() {
        let sessions = vec![
            json!({ "projectId": "P0", "projectPath": "", "title": "scratch" }),
            json!({ "projectId": "P1", "projectName": "Alpha", "projectPath": "/a" }),
            json!({ "projectId": "P0", "projectPath": "" }),
        ];
        let groups = group_sessions_preserving_sidebar_order(&sessions);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].project_name, "Quick Terminals");
        assert_eq!(groups[0].sessions.len(), 2);
        assert_eq!(groups[1].project_name, "Alpha");
        assert_eq!(groups[1].project_path, "/a");
    }

    #[test]
    fn active_project_filter_matches_node() {
        assert!(is_active_gxserver_inventory_project(
            &json!({ "projectId": "P1" })
        ));
        assert!(!is_active_gxserver_inventory_project(
            &json!({ "isRecentProject": true })
        ));
        assert!(!is_active_gxserver_inventory_project(
            &json!({ "visibility": "hidden" })
        ));
        assert!(!is_active_gxserver_inventory_project(
            &json!({ "systemKind": "remoteAttachCarrier" })
        ));
        // JS strict !== true: numeric 1 stays active
        assert!(is_active_gxserver_inventory_project(
            &json!({ "isRecentProject": 1 })
        ));
    }

    #[test]
    fn mobile_summary_keeps_active_and_recent_project_contracts_separate() {
        let summary = to_mobile_session_list(
            &json!({
                "ok": true,
                "projects": [
                    { "projectId": "P1", "name": "Empty", "path": "/repo/empty" },
                    { "projectId": "PC", "name": "Chat", "path": "/Users/me/.ghostex/chats/c1" }
                ],
                "recentProjects": [
                    { "projectId": "PR", "title": "Parked", "path": "/repo/parked", "sessionCount": 2 }
                ],
                "sessions": []
            }),
            &serde_json::Map::new(),
        );
        let projects = summary["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0]["projectId"], "P1");
        assert_eq!(projects[1]["projectId"], "PC");
        assert_eq!(summary["recentProjects"][0]["projectId"], "PR");
        assert!(summary["sessions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn mobile_summary_carries_dim_sleeping_sessions_only_when_on() {
        let result = json!({ "ok": true, "projects": [], "sessions": [] });
        let off = to_mobile_session_list(&result, &serde_json::Map::new());
        assert!(off.get("dimSleepingSessions").is_none());
        let mut settings = serde_json::Map::new();
        settings.insert("dimSleepingSessions".to_string(), json!(false));
        assert!(to_mobile_session_list(&result, &settings)
            .get("dimSleepingSessions")
            .is_none());
        settings.insert("dimSleepingSessions".to_string(), json!(true));
        assert_eq!(
            to_mobile_session_list(&result, &settings)["dimSleepingSessions"],
            json!(true)
        );
    }

    #[test]
    fn mobile_summary_carries_the_unified_sidebar_choices_and_inputs() {
        let result = json!({
            "ok": true,
            "projects": [{
                "projectId": "P1",
                "name": "Ghostex",
                "path": "/x",
                "gitRemoteOriginUrl": "git@github.com:o/Ghostex.git",
            }],
            "sessions": [{
                "sessionId": "S1",
                "projectId": "P1",
                "workingStartedAt": "2026-10-30T10:00:00.000Z",
                "modelSelectionFailure": "That model is not available",
            }],
        });
        let unset = to_mobile_session_list(&result, &serde_json::Map::new());
        assert!(unset.get("sidebarView").is_none());
        assert!(unset.get("unifiedWorkingSection").is_none());
        assert_eq!(
            unset["projects"][0]["gitRemoteOriginUrl"],
            "git@github.com:o/Ghostex.git"
        );
        assert_eq!(
            unset["sessions"][0]["workingStartedAt"],
            "2026-10-30T10:00:00.000Z"
        );
        assert_eq!(
            unset["sessions"][0]["modelSelectionFailure"],
            "That model is not available"
        );
        let mut settings = serde_json::Map::new();
        settings.insert("sidebarView".to_string(), json!("project"));
        settings.insert("unifiedWorkingSection".to_string(), json!(true));
        let defaults = to_mobile_session_list(&result, &settings);
        assert!(defaults.get("sidebarView").is_none());
        assert!(defaults.get("unifiedWorkingSection").is_none());
        assert!(defaults.get("enableSessionParking").is_none());
        settings.insert("sidebarView".to_string(), json!("unified"));
        settings.insert("unifiedWorkingSection".to_string(), json!(false));
        settings.insert("enableSessionParking".to_string(), json!(false));
        let picked = to_mobile_session_list(&result, &settings);
        assert_eq!(picked["sidebarView"], "unified");
        assert_eq!(picked["unifiedWorkingSection"], json!(false));

        // CDXC:Sidebar 2026-10-31: the rest of the Unified sidebar's settings reach the phone only when set.
        assert!(defaults.get("unifiedGroupByProject").is_none());
        assert!(defaults.get("unifiedProjectOrder").is_none());
        assert!(defaults.get("unifiedSessionCardHoverButtons").is_none());
        assert!(defaults.get("sidebarHideProjectsWithoutSessions").is_none());
        settings.insert("unifiedGroupByProject".to_string(), json!(true));
        settings.insert("unifiedProjectOrder".to_string(), json!(["github.com/o/a"]));
        settings.insert(
            "unifiedSessionCardHoverButtons".to_string(),
            json!([{"id": "park", "enabled": true}]),
        );
        settings.insert(
            "sidebarHideProjectsWithoutSessions".to_string(),
            json!(true),
        );
        let scale = to_mobile_session_list(&result, &settings);
        assert_eq!(scale["unifiedGroupByProject"], json!(true));
        assert_eq!(scale["unifiedProjectOrder"], json!(["github.com/o/a"]));
        assert_eq!(
            scale["unifiedSessionCardHoverButtons"][0]["id"],
            json!("park")
        );
        assert_eq!(scale["sidebarHideProjectsWithoutSessions"], json!(true));
        assert_eq!(picked["enableSessionParking"], json!(false));
    }

    #[test]
    fn mobile_summary_carries_new_session_agent_scope_only_when_not_per_project() {
        let result = json!({ "ok": true, "projects": [], "sessions": [] });
        let unset = to_mobile_session_list(&result, &serde_json::Map::new());
        assert!(unset.get("newSessionAgentScope").is_none());
        let mut settings = serde_json::Map::new();
        for (saved, sent) in [
            ("project", None),
            ("galaxy", None),
            ("space", Some("space")),
            ("everywhere", Some("everywhere")),
        ] {
            settings.insert("newSessionAgentScope".to_string(), json!(saved));
            assert_eq!(
                to_mobile_session_list(&result, &settings)
                    .get("newSessionAgentScope")
                    .and_then(Value::as_str),
                sent
            );
        }
    }

    #[test]
    fn mobile_summary_carries_sidebar_session_links_only_when_not_badge() {
        let result = json!({ "ok": true, "projects": [], "sessions": [] });
        let unset = to_mobile_session_list(&result, &serde_json::Map::new());
        assert!(unset.get("sidebarSessionLinks").is_none());
        let mut settings = serde_json::Map::new();
        settings.insert("sidebarSessionLinks".to_string(), json!("badge"));
        assert!(to_mobile_session_list(&result, &settings)
            .get("sidebarSessionLinks")
            .is_none());
        // A person who picked Always keeps it on the phone too.
        settings.insert("sidebarSessionLinks".to_string(), json!("always"));
        assert_eq!(
            to_mobile_session_list(&result, &settings)["sidebarSessionLinks"],
            json!("always")
        );
    }

    #[test]
    fn format_active_time_invalid_is_dash() {
        assert_eq!(format_active_time(None), "-");
        assert_eq!(format_active_time(Some(&json!("not a date"))), "-");
        assert_eq!(format_active_time(Some(&Value::Null)), "-");
    }

    const ATTENTION_SERVER: &str = "attention-cli";
    /// 2026-06-07T06:47:25.000Z.
    const T0: i64 = 1_780_814_845_000;

    fn open_database() -> (tempfile::TempDir, crate::storage::GxserverDatabase) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        crate::storage::initialize_gxserver_storage(&paths).expect("storage init");
        let db = crate::storage::open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }

    /// A running agent session in its own project: the project row, project id and session id.
    fn create_running_session(db: &rusqlite::Connection) -> (Value, String, String) {
        let repository = crate::domain::DomainRepository::new(db, ATTENTION_SERVER);
        let project = repository
            .create_project(
                json!({ "name": "Attention", "path": std::env::temp_dir() })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project["projectId"]
            .as_str()
            .expect("projectId")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "projectId": project_id,
                    "kind": "agent",
                    "agentId": "codex",
                    "title": "Attention session",
                    "lifecycleState": "running",
                    "providerState": { "lifecycleState": "exists" },
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let session_id = session["sessionId"]
            .as_str()
            .expect("sessionId")
            .to_string();
        (project, project_id, session_id)
    }

    /// `/api/updateAgentActivity`, the endpoint hooks and acknowledgements go through.
    fn post_activity(
        db: &rusqlite::Connection,
        project_id: &str,
        session_id: &str,
        params: Value,
    ) -> Value {
        let mut params = params.as_object().cloned().expect("activity params");
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        crate::agents::dispatch_agent_endpoint(
            &crate::domain::DomainRepository::new(db, ATTENTION_SERVER),
            db,
            &std::env::temp_dir(),
            "/api/updateAgentActivity",
            &params,
            None,
        )
        .expect("activity update")
        .result
    }

    /// The session as `ghostex sessions --json` and `--mobile-summary` print it: the live
    /// presentation snapshot overlaid on the session row (`fetch_live_gxserver_session_list`), then
    /// the phone's compactor. Returns the CLI row, the summary row and the summary capabilities.
    fn cli_and_mobile_rows(
        db: &rusqlite::Connection,
        project: &Value,
        project_id: &str,
        session_id: &str,
    ) -> (Value, Value, Value) {
        let repository = crate::domain::DomainRepository::new(db, ATTENTION_SERVER);
        let snapshot = crate::presentation::read_presentation_snapshot(
            db,
            ATTENTION_SERVER,
            None,
            false,
            false,
            repository.list_sessions(None).expect("sessions"),
        )
        .expect("snapshot");
        let presented = snapshot["sessions"]
            .as_array()
            .expect("presented sessions")
            .iter()
            .find(|session| session["sessionId"] == json!(session_id))
            .expect("presented session");
        let raw = repository
            .get_session(project_id, session_id)
            .expect("get session")
            .expect("session row");
        let cli = to_cli_session(&raw, Some(project), 0, Some(presented), Some(0));
        let mobile = to_mobile_session_list(
            &json!({
                "ok": true,
                "product": "gxserver",
                "projects": [project],
                "sessions": [cli.clone()],
                "capabilities": snapshot["capabilities"],
            }),
            &serde_json::Map::new(),
        );
        assert_eq!(
            cli.get("attention"),
            presented.get("attention"),
            "the CLI row forwards the whole attention"
        );
        (
            cli,
            mobile["sessions"][0].clone(),
            mobile["capabilities"].clone(),
        )
    }

    #[test]
    fn mobile_summary_forwards_current_working_stint() {
        let (_temp, db) = open_database();
        let (project, project_id, session_id) = create_running_session(&db);
        let at = crate::session_status::iso_from_ms;
        let activity = |params: Value| post_activity(&db, &project_id, &session_id, params);
        let rows = || cli_and_mobile_rows(&db, &project, &project_id, &session_id);

        activity(json!({ "activity": "working", "nowMs": T0 }));
        activity(json!({ "activity": "working", "nowMs": T0 + 3_000 }));
        let (cli, mobile, capabilities) = rows();
        assert_eq!(cli["workingStartedAt"], json!(at(T0)));
        assert_eq!(mobile["activity"], json!("working"));
        assert_eq!(mobile["workingStartedAt"], json!(at(T0)));
        assert!(mobile.get("attention").is_none());
        assert_eq!(capabilities["attentionNotificationId"], json!(true));

        let stop = activity(json!({
            "activity": "attention",
            "attentionSource": "turnComplete",
            "nowMs": T0 + 30_000,
        }));
        let event_id = stop["activity"]["attentionEventId"]
            .as_str()
            .expect("stored attention event id")
            .to_string();
        let (cli, mobile, _) = rows();
        assert!(cli.get("workingStartedAt").is_none());
        assert!(mobile.get("workingStartedAt").is_none());
        assert_eq!(
            mobile["attention"],
            json!({
                "acknowledged": false,
                "enteredAt": at(T0 + 30_000),
                "eventId": event_id,
                "kind": "finished",
                "workingStartedAt": at(T0),
            })
        );

        let row = crate::notification_feed::store::insert_notification_feed_row(
            &db,
            crate::notification_feed::store::NewNotificationFeedRow {
                project_id: project_id.clone(),
                session_id: session_id.clone(),
                kind: crate::notification_feed::store::NOTIFICATION_FEED_KIND_FINISHED,
                title: "Attention session".to_string(),
                subtitle: "Attention".to_string(),
                body: "Finished a turn".to_string(),
                agent_name: None,
                attention_event_id: Some(event_id),
            },
        )
        .expect("feed row");
        let (_, mobile, _) = rows();
        assert_eq!(mobile["attention"]["notificationId"], row["id"]);

        activity(json!({ "event": "acknowledge", "nowMs": T0 + 40_000 }));
        let (cli, mobile, _) = rows();
        assert!(cli.get("attention").is_none());
        assert!(mobile.get("attention").is_none());
        assert!(mobile.get("workingStartedAt").is_none());

        activity(json!({ "activity": "working", "nowMs": T0 + 60_000 }));
        let (_, mobile, _) = rows();
        assert_eq!(
            mobile["workingStartedAt"],
            json!(at(T0 + 60_000)),
            "a new stint replaces the previous start"
        );
    }
}
