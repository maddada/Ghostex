//! ACP permission request parsing, `cursor/ask_question` translation, and pending request
//! cancellation for Native Cursor sessions.

use std::sync::Mutex;

use serde_json::{json, Value};

use super::transport_and_state::{CursorState, CursorTransport};
use crate::acp::approval::AcpPermissionOption;
use crate::acp::host::{now_ms, PendingPermission};

#[derive(Clone, Debug)]
pub(super) struct CursorQuestionEntry {
    question_id: String,
    option_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) enum CursorRequestKind {
    AcpPermission,
    CursorAskQuestion { questions: Vec<CursorQuestionEntry> },
}

#[derive(Clone, Debug)]
pub(super) struct PendingCursorRequest {
    pub(super) perm: PendingPermission,
    pub(super) kind: CursorRequestKind,
}

/* ------------------------------------------- permissions & questions */

pub(super) fn parse_cursor_permission_request(
    rpc_id: Value,
    params: &Value,
) -> (PendingCursorRequest, Value) {
    let tool_call = params.get("toolCall").unwrap_or(&Value::Null);
    let tool_use_id = tool_call
        .get("toolCallId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let options: Vec<AcpPermissionOption> = params
        .get("options")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let option_id = item.get("optionId").and_then(Value::as_str)?.to_string();
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&option_id)
                        .to_string();
                    let kind = item
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    Some(AcpPermissionOption {
                        option_id,
                        name,
                        kind,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Cursor's ACP fallback for ask_question sends `optionId: "__ask_question_skip__"`.
    let is_question = options
        .iter()
        .any(|o| o.option_id == "__ask_question_skip__");

    let kind_str = tool_call
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("other");
    let title = tool_call
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Tool");

    // Extract diff path or text reason from `toolCall.content` so `evaluate_permission`
    // has full location & command metadata for `Auto-edits` boundary checks.
    let mut eval_params = params.clone();
    let diff_path = tool_call
        .get("content")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter().find_map(|item| {
                (item.get("type").and_then(Value::as_str) == Some("diff"))
                    .then(|| item.get("path").and_then(Value::as_str))
                    .flatten()
                    .map(str::to_string)
            })
        });
    if let Some(ref p) = diff_path {
        if let Some(tc) = eval_params
            .get_mut("toolCall")
            .and_then(Value::as_object_mut)
        {
            tc.entry("locations".to_string())
                .or_insert_with(|| json!([{ "path": p }]));
            tc.entry("rawInput".to_string())
                .or_insert_with(|| json!({ "path": p }));
        }
    } else if kind_str == "execute" {
        let cmd = title.trim().trim_matches('`');
        if !cmd.is_empty() {
            if let Some(tc) = eval_params
                .get_mut("toolCall")
                .and_then(Value::as_object_mut)
            {
                tc.entry("rawInput".to_string())
                    .or_insert_with(|| json!({ "command": cmd }));
            }
        }
    }

    let tool_name = if is_question {
        title.to_string()
    } else {
        match kind_str {
            "execute" => "Bash".to_string(),
            "edit" | "delete" | "move" => {
                if title.to_ascii_lowercase().starts_with("delete ") {
                    "Delete".to_string()
                } else if title.to_ascii_lowercase().starts_with("write ") {
                    "Write".to_string()
                } else {
                    "Edit".to_string()
                }
            }
            "read" => "Read".to_string(),
            "fetch" => "WebFetch".to_string(),
            _ => title.to_string(),
        }
    };

    let summary = diff_path
        .or_else(|| {
            if kind_str == "execute" {
                let cmd = title.trim().trim_matches('`');
                (!cmd.is_empty()).then(|| cmd.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| title.to_string());

    let req = PendingCursorRequest {
        perm: PendingPermission {
            rpc_id,
            tool_name,
            summary,
            tool_use_id,
            reason: None,
            is_question,
            options,
            custom_prompt_override: None,
        },
        kind: CursorRequestKind::AcpPermission,
    };
    (req, eval_params)
}

pub(super) fn parse_cursor_ask_question_request(
    rpc_id: Value,
    params: &Value,
) -> PendingCursorRequest {
    let tool_use_id = params
        .get("toolCallId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| Some(format!("cursor-q-{}", now_ms())));
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Question");

    let mut entries = Vec::new();
    let mut ghostex_questions = Vec::new();

    if let Some(questions) = params.get("questions").and_then(Value::as_array) {
        for q in questions {
            let qid = q
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let prompt = q
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("Choose an option.")
                .to_string();
            let allow_multiple = q
                .get("allowMultiple")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut option_ids = Vec::new();
            let mut ghostex_opts = Vec::new();
            if let Some(opts) = q.get("options").and_then(Value::as_array) {
                for opt in opts {
                    let oid = opt
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let label = opt
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or(&oid)
                        .to_string();
                    option_ids.push(oid);
                    ghostex_opts.push(json!({ "label": label }));
                }
            }
            entries.push(CursorQuestionEntry {
                question_id: qid,
                option_ids,
            });
            ghostex_questions.push(json!({
                "question": prompt,
                "header": title,
                "multiSelect": allow_multiple,
                "allowCustom": false,
                "options": ghostex_opts,
            }));
        }
    }

    let prompt_override = json!({
        "kind": "question",
        "toolUseId": tool_use_id,
        "questions": ghostex_questions,
    });

    PendingCursorRequest {
        perm: PendingPermission {
            rpc_id,
            tool_name: title.to_string(),
            summary: title.to_string(),
            tool_use_id,
            reason: None,
            is_question: true,
            options: Vec::new(),
            custom_prompt_override: Some(prompt_override),
        },
        kind: CursorRequestKind::CursorAskQuestion { questions: entries },
    }
}

pub(super) fn build_cursor_ask_question_response(
    questions: &[CursorQuestionEntry],
    selections: &[Value],
) -> Value {
    let mut answers = Vec::new();
    for (idx, q) in questions.iter().enumerate() {
        let Some(sel) = selections.get(idx) else {
            continue;
        };
        let mut selected_ids = Vec::new();
        if let Some(indices) = sel.get("indices").and_then(Value::as_array) {
            for i in indices.iter().filter_map(Value::as_u64) {
                if let Some(oid) = q.option_ids.get(i as usize) {
                    selected_ids.push(oid.clone());
                }
            }
        }
        if !selected_ids.is_empty() {
            answers.push(json!({
                "questionId": q.question_id,
                "selectedOptionIds": selected_ids,
            }));
        }
    }
    if answers.is_empty() {
        json!({
            "outcome": {
                "outcome": "cancelled",
                "reason": "Questions cancelled by user",
            }
        })
    } else {
        json!({
            "outcome": {
                "outcome": "answered",
                "answers": answers,
            }
        })
    }
}

pub(super) fn resolve_acp_permission_option(
    options: &[AcpPermissionOption],
    explicit_option_id: Option<&str>,
    decision: &str,
) -> Option<String> {
    if let Some(id) = explicit_option_id.filter(|s| !s.is_empty()) {
        if options.iter().any(|opt| opt.option_id == id) {
            return Some(id.to_string());
        }
    }
    let normalized = decision.trim().to_ascii_lowercase();
    let (is_deny, wanted_kinds): (bool, &[&str]) = match normalized.as_str() {
        "allow" | "allow_once" | "1" | "y" | "yes" => (false, &["allow_once", "allow_always"]),
        "always" | "allow_always" | "2" => (false, &["allow_always", "allow_once"]),
        "deny" | "reject" | "reject_once" | "escape" | "cancel" | "\x1b" | "n" | "no" => {
            (true, &["reject_once", "reject_always"])
        }
        "reject_always" => (true, &["reject_always", "reject_once"]),
        _ => (false, &["allow_once"]),
    };
    for kind in wanted_kinds {
        if let Some(opt) = options.iter().find(|opt| opt.kind == *kind) {
            return Some(opt.option_id.clone());
        }
    }
    if is_deny {
        return None;
    }
    options
        .iter()
        .find(|opt| !opt.kind.starts_with("reject"))
        .map(|opt| opt.option_id.clone())
}

pub(super) fn cancel_all_pending_requests(transport: &CursorTransport, state: &Mutex<CursorState>) {
    let drained: Vec<PendingCursorRequest> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_requests.drain(..).collect()
    };
    for req in drained {
        match req.kind {
            CursorRequestKind::AcpPermission => {
                let _ = transport.respond_to_agent(
                    req.perm.rpc_id,
                    json!({ "outcome": { "outcome": "cancelled" } }),
                );
            }
            CursorRequestKind::CursorAskQuestion { .. } => {
                let _ = transport.respond_to_agent(
                    req.perm.rpc_id,
                    json!({ "outcome": { "outcome": "cancelled", "reason": "Cancelled" } }),
                );
            }
        }
    }
}
