//! Permission rule mapping, interactive approval/form handling, and live approval level updates.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Map, Value};

use super::super::approval::{
    evaluate_permission, AcpApprovalLevel, AcpPermissionOption, PermissionDecision,
};
use super::super::host::PendingPermission;
use super::http::OpenCodeHttp;
use super::state::{OpenCodeActivityReporter, OpenCodeState};
use super::{DISMISSED_FORM_MESSAGE, HTTP_TIMEOUT};

#[derive(Clone, Debug)]
pub(super) enum PendingOpenCodeKind {
    Permission {
        action: String,
        resources: Vec<String>,
    },
    Form {
        form: Value,
    },
}

#[derive(Clone, Debug)]
pub(super) struct PendingOpenCodeRequest {
    pub(super) request_id: String,
    pub(super) session_id: String,
    pub(super) is_subagent: bool,
    pub(super) tool_name: String,
    pub(super) summary: String,
    pub(super) reason: Option<String>,
    pub(super) kind: PendingOpenCodeKind,
}

impl PendingOpenCodeRequest {
    pub(super) fn is_question(&self) -> bool {
        matches!(self.kind, PendingOpenCodeKind::Form { .. })
    }

    pub(super) fn to_ghostex_prompt(&self) -> Value {
        match &self.kind {
            PendingOpenCodeKind::Permission { .. } => {
                let mut prompt = json!({
                    "kind": "approval",
                    "tool": self.tool_name,
                    "summary": self.summary,
                    "toolUseId": self.request_id,
                    "options": [
                        { "optionId": "once", "label": "Allow", "kind": "allow_once" },
                        { "optionId": "reject", "label": "Deny", "kind": "reject_once" }
                    ],
                });
                if let Some(reason) = self.reason.as_deref().filter(|s| !s.trim().is_empty()) {
                    prompt["reason"] = json!(reason);
                }
                prompt
            }
            PendingOpenCodeKind::Form { form } => {
                if let Some(prompt) = crate::session_chat_opencode::form_prompt(form) {
                    if let Ok(val) = serde_json::to_value(prompt) {
                        return val;
                    }
                }
                json!({
                    "kind": "question",
                    "toolUseId": self.request_id,
                    "questions": [],
                })
            }
        }
    }

    pub(super) fn to_activity_permission(&self) -> PendingPermission {
        let prompt = self.to_ghostex_prompt();
        PendingPermission {
            rpc_id: Value::String(self.request_id.clone()),
            tool_name: self.tool_name.clone(),
            summary: self.summary.clone(),
            tool_use_id: Some(self.request_id.clone()),
            reason: self.reason.clone(),
            is_question: self.is_question(),
            options: vec![
                AcpPermissionOption {
                    option_id: "once".to_string(),
                    name: "Allow".to_string(),
                    kind: "allow_once".to_string(),
                },
                AcpPermissionOption {
                    option_id: "reject".to_string(),
                    name: "Deny".to_string(),
                    kind: "reject_once".to_string(),
                },
            ],
            custom_prompt_override: Some(prompt),
        }
    }
}

/// Extract the allowed plan-file edit paths from OpenCode's `GET /api/agent` response for `"plan"`.
pub(super) fn extract_plan_agent_paths(agents_resp: &Value) -> Vec<String> {
    let Some(agents) = agents_resp.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for agent in agents {
        let id = agent
            .get("id")
            .or_else(|| agent.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !id.eq_ignore_ascii_case("plan") {
            continue;
        }
        for rule in agent
            .get("permissions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if rule.get("action").and_then(Value::as_str) == Some("edit")
                && rule.get("effect").and_then(Value::as_str) == Some("allow")
            {
                if let Some(res) = rule.get("resource").and_then(Value::as_str) {
                    if !res.is_empty() && res != "*" && !paths.iter().any(|p| p == res) {
                        paths.push(res.to_string());
                    }
                }
            }
        }
    }
    paths
}

/// Build OpenCode 2 session permission rules (last match wins) for Ghostex's approval level,
/// optional `--read-only` mode, and in-memory session-scoped grants.
pub(super) fn build_session_rules(
    level: AcpApprovalLevel,
    read_only: bool,
    plan_paths: &[String],
    grants: &[(String, String)],
) -> Vec<Value> {
    let mut rules = Vec::new();
    if read_only {
        rules.push(json!({ "action": "shell", "resource": "*", "effect": "ask" }));
        rules.push(json!({ "action": "external_directory", "resource": "*", "effect": "ask" }));
        for (action, resource) in grants {
            if action != "edit" {
                rules.push(json!({ "action": action, "resource": resource, "effect": "allow" }));
            }
        }
        rules.push(json!({ "action": "edit", "resource": "*", "effect": "deny" }));
        for path in plan_paths {
            rules.push(json!({ "action": "edit", "resource": path, "effect": "allow" }));
        }
        return rules;
    }
    match level {
        AcpApprovalLevel::FullAccess => {
            rules.push(json!({ "action": "*", "resource": "*", "effect": "allow" }));
        }
        AcpApprovalLevel::AutoEdits => {
            rules.push(json!({ "action": "shell", "resource": "*", "effect": "ask" }));
            rules.push(json!({ "action": "edit", "resource": "*", "effect": "allow" }));
            rules.push(json!({ "action": "external_directory", "resource": "*", "effect": "ask" }));
            for (action, resource) in grants {
                rules.push(json!({ "action": action, "resource": resource, "effect": "allow" }));
            }
        }
        AcpApprovalLevel::Ask | AcpApprovalLevel::Auto => {
            rules.push(json!({ "action": "shell", "resource": "*", "effect": "ask" }));
            rules.push(json!({ "action": "edit", "resource": "*", "effect": "ask" }));
            rules.push(json!({ "action": "external_directory", "resource": "*", "effect": "ask" }));
            for (action, resource) in grants {
                rules.push(json!({ "action": action, "resource": resource, "effect": "allow" }));
            }
        }
    }
    rules
}

pub(super) fn opencode_permission_tool_name(action: &str) -> String {
    match action.trim().to_ascii_lowercase().as_str() {
        "shell" | "bash" => "Bash".to_string(),
        "edit" | "write" | "apply_patch" => "Edit".to_string(),
        "read" => "Read".to_string(),
        "external_directory" => "External Directory".to_string(),
        "webfetch" | "websearch" => "Web".to_string(),
        other if !other.is_empty() => action.to_string(),
        _ => "OpenCode".to_string(),
    }
}

fn synthetic_acp_permission_params(action: &str, resources: &[String]) -> Value {
    let lower = action.trim().to_ascii_lowercase();
    let (kind, raw_input, locations) = match lower.as_str() {
        "edit" | "write" | "apply_patch" => {
            let locs: Vec<Value> = resources
                .iter()
                .map(|r| r.trim())
                .filter(|r| !r.is_empty() && *r != "*")
                .map(|r| json!({ "path": r }))
                .collect();
            ("edit", json!({ "resources": resources }), locs)
        }
        "read" => {
            let locs: Vec<Value> = resources
                .iter()
                .map(|r| r.trim())
                .filter(|r| !r.is_empty() && *r != "*")
                .map(|r| json!({ "path": r }))
                .collect();
            // Do not auto-approve `*.env` secret reads in Ask/AutoEdits.
            let is_secret = resources
                .iter()
                .any(|r| r.to_ascii_lowercase().contains(".env"));
            (
                if is_secret { "other" } else { "read" },
                json!({ "resources": resources }),
                locs,
            )
        }
        "shell" | "bash" => (
            "execute",
            json!({ "command": resources.join(" ") }),
            Vec::new(),
        ),
        "external_directory" => ("other", json!({ "resources": resources }), Vec::new()),
        "webfetch" | "websearch" => ("fetch", json!({ "resources": resources }), Vec::new()),
        _ => ("other", json!({ "resources": resources }), Vec::new()),
    };
    json!({
        "toolCall": {
            "name": opencode_permission_tool_name(action),
            "kind": kind,
            "rawInput": raw_input,
            "locations": locations,
        }
    })
}

pub(super) fn asked_form_fields(form: &Value) -> Option<Vec<&Value>> {
    let fields = form.get("fields")?.as_array()?;
    if fields.iter().any(|field| field["type"] == "external") {
        return None;
    }
    Some(
        fields
            .iter()
            .filter(|field| field["hidden"] != true)
            .collect(),
    )
}

pub(super) fn external_form_url(form: &Value) -> Option<String> {
    form.get("fields")?
        .as_array()?
        .iter()
        .find(|field| field["type"] == "external")
        .and_then(|field| field.get("url").and_then(Value::as_str))
        .map(str::to_string)
}

fn form_condition_holds(field: &Value, answers: &Map<String, Value>) -> bool {
    let Some(when) = field.get("when").filter(|w| w.is_object()) else {
        return true;
    };
    let Some(answer) = when["key"].as_str().and_then(|key| answers.get(key)) else {
        return false;
    };
    let equal = match answer {
        Value::Array(values) => values.contains(&when["value"]),
        other => other == &when["value"],
    };
    if when["op"] == "neq" {
        !equal
    } else {
        equal
    }
}

pub(super) fn build_form_reply_answers(
    form: &Value,
    selections: &[Value],
) -> Result<Value, String> {
    let fields = asked_form_fields(form)
        .ok_or_else(|| "This OpenCode form includes an unsupported external step.".to_string())?;
    if fields.len() != selections.len() {
        return Err("Answer every OpenCode question.".to_string());
    }
    let mut answers = Map::new();
    for (field, sel) in fields.iter().copied().zip(selections) {
        let key = field["key"]
            .as_str()
            .ok_or_else(|| "Invalid OpenCode question key.".to_string())?;
        if !form_condition_holds(field, &answers) {
            continue;
        }
        let indices: Vec<usize> = sel
            .get("indices")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_u64)
                    .map(|n| n as usize)
                    .collect()
            })
            .unwrap_or_default();
        let other = sel
            .get("other")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let mut values = Vec::new();
        if field["type"] != "boolean" {
            for idx in &indices {
                let opt = field["options"]
                    .get(*idx)
                    .and_then(|o| {
                        o.get("value")
                            .or_else(|| o.get("label"))
                            .and_then(Value::as_str)
                    })
                    .ok_or_else(|| "A selected option no longer exists.".to_string())?;
                values.push(opt.to_string());
            }
        }
        if let Some(custom) = other {
            values.push(custom.to_string());
        }

        let value = if field["type"] == "boolean" {
            match indices.as_slice() {
                [0] => json!(true),
                [1] => json!(false),
                _ => return Err("Choose Yes or No.".to_string()),
            }
        } else if field["type"] == "multiselect" {
            json!(values)
        } else {
            if values.len() != 1 {
                return Err("Provide one answer for each question.".to_string());
            }
            if matches!(field["type"].as_str(), Some("number" | "integer")) {
                serde_json::from_str::<Value>(&values[0])
                    .ok()
                    .filter(|v| v.is_number())
                    .ok_or_else(|| "This question needs a number.".to_string())?
            } else {
                json!(values[0])
            }
        };
        answers.insert(key.to_string(), value);
    }
    Ok(json!({ "answer": Value::Object(answers) }))
}

pub(super) fn handle_incoming_permission(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    perm: &Value,
    target_session_id: &str,
    is_subagent: bool,
) {
    let Some(req_id) = perm
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| crate::session_chat_opencode::safe_id(id))
    else {
        return;
    };
    let action = perm
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("permission")
        .to_string();
    let resources: Vec<String> = perm
        .get("resources")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let summary = if resources.is_empty() {
        action.clone()
    } else {
        resources.join("\n")
    };
    let tool_name = opencode_permission_tool_name(&action);
    let (level, roots, cwd, already_pending) = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        let dup = s
            .pending_requests
            .iter()
            .any(|p| p.request_id == req_id && !p.is_question());
        (
            s.approval_level,
            s.writable_roots.clone(),
            s.cwd.clone(),
            dup,
        )
    };
    if already_pending {
        return;
    }

    let allow_opts = vec![
        AcpPermissionOption {
            option_id: "once".to_string(),
            name: "Allow".to_string(),
            kind: "allow_once".to_string(),
        },
        AcpPermissionOption {
            option_id: "reject".to_string(),
            name: "Deny".to_string(),
            kind: "reject_once".to_string(),
        },
    ];
    let synthetic = synthetic_acp_permission_params(&action, &resources);
    let reason = match evaluate_permission(level, &synthetic, &allow_opts, &roots, &cwd) {
        PermissionDecision::AutoApprove { .. } => {
            println!(
                "\r\n[OpenCode Native] Auto-approved ({}): {} ({})",
                level.label(),
                tool_name,
                summary
            );
            let _ = std::io::stdout().flush();
            let _ = http.session(
                target_session_id,
                &format!("/permission/{req_id}/reply"),
                "POST",
                Some(&json!({ "decision": "once" })),
                HTTP_TIMEOUT,
            );
            return;
        }
        PermissionDecision::AskUser { reason } => reason,
    };

    let pending = PendingOpenCodeRequest {
        request_id: req_id.to_string(),
        session_id: target_session_id.to_string(),
        is_subagent,
        tool_name,
        summary,
        reason,
        kind: PendingOpenCodeKind::Permission { action, resources },
    };
    let was_empty = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let empty = s.pending_requests.is_empty();
        s.pending_requests.push_back(pending);
        empty
    };
    if was_empty {
        activity.sync();
    }
}

pub(super) fn handle_incoming_form(
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    form: &Value,
    target_session_id: &str,
    is_subagent: bool,
) {
    let Some(form_id) = form
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| crate::session_chat_opencode::safe_id(id))
    else {
        return;
    };
    if asked_form_fields(form).is_none() {
        let url = external_form_url(form);
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.external_form_url = url.or_else(|| Some(String::new()));
        return;
    }
    let summary = form
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Question")
        .to_string();
    let pending = PendingOpenCodeRequest {
        request_id: form_id.to_string(),
        session_id: target_session_id.to_string(),
        is_subagent,
        tool_name: "AskUserQuestion".to_string(),
        summary,
        reason: None,
        kind: PendingOpenCodeKind::Form { form: form.clone() },
    };
    let was_empty = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.pending_requests.iter().any(|p| p.request_id == form_id) {
            return;
        }
        let empty = s.pending_requests.is_empty();
        s.pending_requests.push_back(pending);
        empty
    };
    if was_empty {
        activity.sync();
    }
}

/// Reconcile full session state from the HTTP API (used on startup, after turn events, and on
/// SSE reconnect).

pub(super) fn cancel_all_pending(http: &OpenCodeHttp, state: &Arc<Mutex<OpenCodeState>>) {
    let drained: Vec<PendingOpenCodeRequest> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_requests.drain(..).collect()
    };
    for req in drained {
        match req.kind {
            PendingOpenCodeKind::Permission { .. } => {
                let _ = http.session(
                    &req.session_id,
                    &format!("/permission/{}/reply", req.request_id),
                    "POST",
                    Some(&json!({ "decision": "reject" })),
                    Duration::from_secs(5),
                );
            }
            PendingOpenCodeKind::Form { .. } => {
                let body = req
                    .is_subagent
                    .then(|| json!({ "message": DISMISSED_FORM_MESSAGE }));
                let _ = http.session(
                    &req.session_id,
                    &format!("/form/{}", req.request_id),
                    "DELETE",
                    body.as_ref(),
                    Duration::from_secs(5),
                );
            }
        }
    }
}

pub(super) fn apply_approval_level(
    http: &OpenCodeHttp,
    state: &Arc<Mutex<OpenCodeState>>,
    activity: &OpenCodeActivityReporter,
    desired: AcpApprovalLevel,
) -> Result<(), String> {
    let (sid, rules, leave_plan) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let was_read_only = s.read_only;
        s.approval_level = desired;
        if desired != AcpApprovalLevel::Ask {
            s.read_only = false;
        }
        let leave_plan = was_read_only && !s.read_only && s.current_agent == "plan";
        let rules = build_session_rules(
            s.approval_level,
            s.read_only,
            &s.plan_agent_paths,
            &s.session_grants,
        );
        (s.session_id.clone(), rules, leave_plan)
    };
    let Some(sid) = sid else {
        return Err("OpenCode session is not initialized.".to_string());
    };
    http.session(
        &sid,
        "",
        "PATCH",
        Some(&json!({ "permissions": rules })),
        HTTP_TIMEOUT,
    )?;
    if leave_plan {
        let _ = http.session(
            &sid,
            "/agent",
            "POST",
            Some(&json!({ "agent": "build" })),
            HTTP_TIMEOUT,
        );
        state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .current_agent = "build".to_string();
    }
    // If raising to FullAccess while permissions are already waiting, auto-approve them now.
    if desired == AcpApprovalLevel::FullAccess {
        let to_auto: Vec<PendingOpenCodeRequest> = {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            let mut rest = VecDeque::new();
            let mut auto = Vec::new();
            while let Some(req) = s.pending_requests.pop_front() {
                if !req.is_question() {
                    auto.push(req);
                } else {
                    rest.push_back(req);
                }
            }
            s.pending_requests = rest;
            auto
        };
        for req in to_auto {
            let _ = http.session(
                &req.session_id,
                &format!("/permission/{}/reply", req.request_id),
                "POST",
                Some(&json!({ "decision": "once" })),
                HTTP_TIMEOUT,
            );
        }
    }
    activity.sync();
    Ok(())
}
