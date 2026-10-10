//! The Work page's requests. The page posts `{ action, requestId, ... }` through the project
//! workarea bridge (`postProjectBoardRequest`, apps/desktop/views/work/bridge.ts) and gets the
//! answer back as a `ghostex-work-response` event. Reads go to gxserver; Open chat, Start chat and
//! links act on the app, because only the app can select a session or open a tab.

use std::time::Duration;

use ghostex_gx_core::SessionKey;
use serde_json::{Map, Value, json};

use super::host::work_view_launcher_agents;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

const WORK_LIST_TIMEOUT: Duration = Duration::from_secs(20);
const WORK_ITEM_TIMEOUT: Duration = Duration::from_secs(30);
/// Starting work creates a worktree and starts the agent.
const START_WORK_TIMEOUT: Duration = Duration::from_secs(90);
/// Drafting a cloud task reads the ticket, the team instructions and `git ls-remote`.
const DRAFT_CLOUD_TIMEOUT: Duration = Duration::from_secs(45);
/// A cloud start takes 5 to 30 seconds and the runner gives up after 180 (server/src/cloud_runner.rs).
const START_CLOUD_TIMEOUT: Duration = Duration::from_secs(200);

fn text(request: &Value, key: &str) -> Option<String> {
    request
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// The ticket fields a request may carry, copied as they are for gxserver to check.
fn ticket_params(request: &Value, params: &mut Map<String, Value>) {
    for key in [
        "projectId",
        "linearIssue",
        "githubIssue",
        "pullRequest",
        "agentId",
    ] {
        if let Some(value) = request.get(key).filter(|value| !value.is_null()) {
            params.insert(key.to_string(), value.clone());
        }
    }
}

impl GhostexGpuiApp {
    pub(crate) fn receive_work_view_request(
        &mut self,
        payload: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let request = serde_json::from_str::<Value>(&payload).unwrap_or_default();
        let action = text(&request, "action").unwrap_or_default();
        let request_id = text(&request, "requestId").unwrap_or_default();
        let project_ids = self.work_view_project_ids();
        match action.as_str() {
            "work.ready" => {
                let agents = self
                    .native_sidebar
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| {
                        snapshot
                            .groups
                            .iter()
                            .find(|group| group.work_mode_project_id().is_some())
                            .map(|group| work_view_launcher_agents(&group.header_actions))
                    })
                    .unwrap_or_default();
                let current_session = self.work_view_current_session();
                self.work_view.current_session_sent = Some(current_session.clone());
                let answer = json!({
                    "projectIds": project_ids,
                    "agents": agents,
                    "pendingOpen": self.work_view.pending_open.clone(),
                    "currentSession": current_session,
                });
                self.answer_work_view_request(&request_id, Ok(answer), cx);
            }
            "work.ackOpen" => {
                self.work_view.pending_open = None;
            }
            "work.list" => {
                let params = json!({
                    "projectIds": project_ids,
                    "force": request.get("force").and_then(Value::as_bool).unwrap_or(false),
                });
                self.work_view_rpc(
                    request_id,
                    "/api/listWorkItems",
                    params,
                    WORK_LIST_TIMEOUT,
                    cx,
                );
            }
            "work.read" => {
                let mut params = Map::new();
                ticket_params(&request, &mut params);
                params.insert("projectIds".to_string(), json!(project_ids));
                params.insert(
                    "force".to_string(),
                    json!(
                        request
                            .get("force")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
                self.work_view_rpc(
                    request_id,
                    "/api/readWorkItem",
                    Value::Object(params),
                    WORK_ITEM_TIMEOUT,
                    cx,
                );
            }
            "work.readTeamFlow" => {
                let mut params = Map::new();
                ticket_params(&request, &mut params);
                self.work_view_rpc(
                    request_id,
                    "/api/readTeamFlow",
                    Value::Object(params),
                    WORK_LIST_TIMEOUT,
                    cx,
                );
            }
            "work.startChat" => self.start_work_view_chat(request_id, &request, cx),
            // Start in cloud: the task the box opens with, then the start (crate::cloud_runner).
            "work.draftCloud" | "work.startCloud" => {
                let mut params = Map::new();
                ticket_params(&request, &mut params);
                params.remove("agentId");
                let start = action == "work.startCloud";
                if start {
                    for key in ["provider", "prompt"] {
                        if let Some(value) = text(&request, key) {
                            params.insert(key.to_string(), json!(value));
                        }
                    }
                }
                let (path, timeout) = if start {
                    ("/api/startCloudWork", START_CLOUD_TIMEOUT)
                } else {
                    ("/api/draftCloudWork", DRAFT_CLOUD_TIMEOUT)
                };
                self.work_view_rpc(request_id, path, Value::Object(params), timeout, cx);
            }
            // Link to current session (current_session.rs): adds the ticket to the session's
            // links, and Undo sends back exactly what the add answered with.
            "work.linkCurrentSession" | "work.undoLink" => {
                let params = match (text(&request, "projectId"), text(&request, "sessionId")) {
                    (Some(project_id), Some(session_id)) => {
                        let mut params = Map::new();
                        params.insert("projectId".to_string(), json!(project_id));
                        params.insert("sessionId".to_string(), json!(session_id));
                        if action == "work.undoLink" {
                            let undo = request.get("undo").and_then(Value::as_object);
                            for key in ["linearIssues", "githubIssues", "pullRequest"] {
                                if let Some(value) = undo.and_then(|undo| undo.get(key)) {
                                    params.insert(key.to_string(), value.clone());
                                }
                            }
                        } else if let Some(issue) =
                            request.get("linearIssue").filter(|value| !value.is_null())
                        {
                            params.insert("addLinearIssues".to_string(), json!([issue]));
                        } else if let Some(issue) =
                            request.get("githubIssue").filter(|value| !value.is_null())
                        {
                            params.insert("addGithubIssues".to_string(), json!([issue]));
                        } else if let Some(pr) =
                            request.get("pullRequest").filter(|value| !value.is_null())
                        {
                            params.insert("addPullRequest".to_string(), pr.clone());
                        }
                        Ok(params)
                    }
                    _ => Err("Pick a session in the sidebar first.".to_string()),
                };
                match params {
                    Ok(params) => self.work_view_rpc(
                        request_id,
                        "/api/setSessionWorkLinks",
                        Value::Object(params),
                        WORK_LIST_TIMEOUT,
                        cx,
                    ),
                    Err(error) => self.answer_work_view_request(&request_id, Err(error), cx),
                }
            }
            "work.openCloudInTerminal" => {
                self.open_cloud_session_in_terminal(request_id, &request, cx)
            }
            // CDXC:WorkMode 2026-10-09 DECISION:
            // User: the GitHub Projects scope notice is "a closable notice on the page that appears once"; closing it is remembered by gxserver, so it stays closed in every window.
            "work.dismissNotice" => {
                let params = json!({ "notice": text(&request, "notice").unwrap_or_default() });
                self.work_view_rpc(
                    request_id,
                    "/api/dismissWorkNotice",
                    params,
                    WORK_LIST_TIMEOUT,
                    cx,
                );
            }
            // CDXC:WorkMode 2026-10-09 DECISION:
            // User: "We need a button to create a linear ticket to start work in the ... dropdown in the project header (also can be created from the 'Work' page)". New ticket opens the same native dialog, for the project the page is filtered to, or with the dialog's Project picker over the window's work-mode projects; the page refreshes when a ticket is made (`work_view_ticket_created`).
            "work.createTicket" => {
                let projects = self.work_view_projects();
                let wanted = text(&request, "projectId").filter(|wanted| {
                    projects
                        .iter()
                        .any(|project| project["projectId"] == *wanted)
                });
                let answer = if projects.is_empty() {
                    Err("Turn on Work mode for a project to create tickets from here.".to_string())
                } else {
                    let message = match wanted {
                        Some(project_id) => {
                            let name = projects
                                .iter()
                                .find(|project| project["projectId"] == project_id)
                                .and_then(|project| project["name"].as_str())
                                .map(str::to_string);
                            json!({ "projectId": project_id, "projectName": name })
                        }
                        None => json!({ "projects": projects }),
                    };
                    self.open_gpui_create_linear_ticket_modal(&message, cx);
                    Ok(json!({ "opened": true }))
                };
                self.answer_work_view_request(&request_id, answer, cx);
            }
            "work.openChat" => {
                let opened = match (text(&request, "projectId"), text(&request, "sessionId")) {
                    (Some(project_id), Some(session_id)) => self.gx_store_focus_activated_session(
                        &SessionKey::local(project_id, session_id).to_sidebar_session_id(),
                        cx,
                    ),
                    _ => false,
                };
                let answer = if opened {
                    Ok(json!({ "opened": true }))
                } else {
                    Err("That session is not in the sidebar any more.".to_string())
                };
                self.answer_work_view_request(&request_id, answer, cx);
            }
            "work.openUrl" => {
                let url = text(&request, "url")
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
                let answer = match url {
                    Some(url) => {
                        let message = GpuiSidebarOpenBrowserUrlMessage {
                            url,
                            reuse: GpuiBrowserRendererOpenReuse::Exact,
                            from_quick_header: false,
                            project_id: None,
                        };
                        self.defer_in_main_window(cx, move |this, window, cx| {
                            this.open_browser_url_from_renderer_command(message, window, cx);
                        });
                        Ok(json!({ "opened": true }))
                    }
                    None => Err("Only web links open from the Work page.".to_string()),
                };
                self.answer_work_view_request(&request_id, answer, cx);
            }
            _ => {
                self.answer_work_view_request(
                    &request_id,
                    Err(format!(
                        "The Work page asked for \"{action}\", which this app does not know."
                    )),
                    cx,
                );
            }
        }
    }

    /// Start chat: `/api/startWorkOnTicket` makes the session (worktree on the ticket's branch,
    /// linked, nothing sent), then the sidebar selects it and the chat column shows it, while the
    /// Work view stays where it is.
    fn start_work_view_chat(
        &mut self,
        request_id: String,
        request: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut params = Map::new();
        ticket_params(request, &mut params);
        params.remove("pullRequest");
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let params = Value::Object(params);
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result("/api/startWorkOnTicket", &params, START_WORK_TIMEOUT)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(answer) = &result {
                    if let (Some(project_id), Some(session_id)) =
                        (text(answer, "projectId"), text(answer, "sessionId"))
                    {
                        this.gx_store_focus_created_session(
                            &project_id,
                            &session_id,
                            true,
                            Some("chat"),
                            cx,
                        );
                    }
                }
                this.answer_work_view_request(&request_id, result, cx);
            });
        })
        .detach();
    }

    /// Open in terminal for a cloud session: a new terminal session in the ticket's project runs
    /// `claude --cloud <url>`, which attaches to the running cloud session, and the window
    /// selects it.
    ///
    /// CDXC:WorkMode 2026-10-10 DECISION:
    /// User: a cloud session's row in the Conversations card gets "Open in terminal" next to "Open
    /// in Claude": a new Ghostex terminal session in the ticket's project running `claude --cloud
    /// <session url>` (Claude Code 2.1.296: "attach to an existing one by session ID or
    /// claude.ai/code URL"), titled after the ticket, and selected.
    ///
    /// CDXC:WorkMode 2026-10-10 WHY:
    /// Attaching is gated per account by Claude: on the account this was built with, 2.1.296
    /// answers "Attaching to an existing cloud session is not enabled for your account." for a URL
    /// or a session id, and the terminal session shows that line. It is Claude's switch, not a
    /// Ghostex fault, so the button stays and the terminal says why.
    fn open_cloud_session_in_terminal(
        &mut self,
        request_id: String,
        request: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        // The CLI prints `…/session_…?from=cli&m=0`; the query is only tracking, and a bare `&`
        // would end the command in a shell, so the terminal gets the session's own URL.
        let url = text(request, "sessionUrl")
            .map(|url| url.split(['?', '#']).next().unwrap_or_default().to_string())
            .filter(|url| {
                url.starts_with("https://claude.ai/code/")
                    && url.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "-_./:".contains(character)
                    })
            });
        let (Some(url), Some(project_id)) = (url, text(request, "projectId")) else {
            self.answer_work_view_request(
                &request_id,
                Err("Only a Claude Code cloud session's link opens in a terminal.".to_string()),
                cx,
            );
            return;
        };
        let title =
            text(request, "title").unwrap_or_else(|| "Claude Code in the cloud".to_string());
        let params = ghostex_gx_core::os_integration_command_params(
            &format!("claude --cloud {url}"),
            &project_id,
            &title,
        );
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result("/api/createAgentSession", &params, START_WORK_TIMEOUT)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let result = result.and_then(|answer| {
                    let (created_project, session_id) =
                        ghostex_gx_core::created_session(&answer, Some(&project_id)).ok_or_else(
                            || "Ghostex did not say which session it made.".to_string(),
                        )?;
                    let created_project = created_project.unwrap_or_else(|| project_id.clone());
                    // Like Start chat: the Work view stays, and the session shows its terminal.
                    this.gx_store_focus_created_session(
                        &created_project,
                        &session_id,
                        true,
                        Some("terminal"),
                        cx,
                    );
                    Ok(json!({ "projectId": created_project, "sessionId": session_id }))
                });
                this.answer_work_view_request(&request_id, result, cx);
            });
        })
        .detach();
    }

    fn work_view_rpc(
        &mut self,
        request_id: String,
        path: &'static str,
        params: Value,
        timeout: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_gxserver_rpc_result(path, &params, timeout) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.answer_work_view_request(&request_id, result, cx);
            });
        })
        .detach();
    }

    fn answer_work_view_request(
        &mut self,
        request_id: &str,
        result: Result<Value, String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let response = match result {
            Ok(payload) => json!({ "requestId": request_id, "ok": true, "payload": payload }),
            Err(error) => json!({ "requestId": request_id, "ok": false, "error": error }),
        };
        self.dispatch_project_workarea_json_event(
            ProjectWorkareaCefSurfaceSlotKey::Work,
            "ghostex-work-response",
            &response.to_string(),
            cx,
        );
    }
}
