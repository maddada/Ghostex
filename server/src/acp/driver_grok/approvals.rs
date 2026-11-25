//! Permission request parsing, xAI ask_user_question and exit_plan_mode handling, and cancellation.

use std::sync::Mutex;

use serde_json::{json, Map, Value};

use crate::acp::approval::AcpPermissionOption;

use super::protocol::{non_empty_str, resolve_xai_tool_title};
use super::state::{GrokPendingRequest, GrokState, PendingPromptKind};
use super::transport::GrokTransport;
use super::GROK_ALLOW_EDITS_SESSION_OPTION_ID;

pub(super) fn parse_grok_permission_request(rpc_id: Value, params: &Value) -> GrokPendingRequest {
    let tool_call = params.get("toolCall").unwrap_or(&Value::Null);
    let tool_use_id = non_empty_str(tool_call.get("toolCallId")).map(str::to_string);
    let tool_name = resolve_xai_tool_title(
        tool_call
            .get("name")
            .or_else(|| tool_call.get("title"))
            .or_else(|| tool_call.get("kind"))
            .and_then(Value::as_str),
        tool_call.get("_meta"),
        tool_call.get("rawInput").unwrap_or(&Value::Null),
        tool_call.get("rawOutput").unwrap_or(&Value::Null),
    )
    .unwrap_or_else(|| "Tool".to_string());
    let summary = tool_call
        .get("rawInput")
        .and_then(|v| {
            [
                "command",
                "CommandLine",
                "file_path",
                "filePath",
                "path",
                "Path",
                "target_path",
                "targetPath",
                "url",
                "query",
                "description",
            ]
            .iter()
            .find_map(|key| non_empty_str(v.get(*key)))
            .map(str::to_string)
            .or_else(|| non_empty_str(tool_call.pointer("/locations/0/path")).map(str::to_string))
            .or_else(|| serde_json::to_string(v).ok())
        })
        .or_else(|| non_empty_str(tool_call.pointer("/locations/0/path")).map(str::to_string))
        .unwrap_or_else(|| tool_name.clone());

    // Filter options to match Grok's safe approval policy:
    // keep allow_once, reject_once, reject_always, and only session-scoped `allow-edits-session`
    // for allow_always (other allow_always choices persist project grants).
    let raw_options: Vec<AcpPermissionOption> = params
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

    let filtered_options: Vec<AcpPermissionOption> = raw_options
        .iter()
        .filter(|opt| {
            opt.kind != "allow_always" || opt.option_id.trim() == GROK_ALLOW_EDITS_SESSION_OPTION_ID
        })
        .cloned()
        .collect();
    let options = if filtered_options.is_empty() {
        raw_options
    } else {
        filtered_options
    };

    GrokPendingRequest {
        rpc_id,
        tool_name,
        summary,
        tool_use_id,
        reason: None,
        options,
        kind: PendingPromptKind::Permission,
        custom_prompt_override: None,
    }
}

pub(super) fn unwrap_xai_ext_params<'a>(params: &'a Value) -> &'a Value {
    if params.get("method").is_some() && params.get("params").is_some_and(Value::is_object) {
        params.get("params").unwrap_or(params)
    } else {
        params
    }
}

pub(super) fn parse_xai_ask_user_question_request(
    rpc_id: Value,
    params: &Value,
) -> GrokPendingRequest {
    let unwrapped = unwrap_xai_ext_params(params);
    let tool_call_id = non_empty_str(unwrapped.get("toolCallId")).map(str::to_string);
    let raw_questions: Vec<Value> = unwrapped
        .get("questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let ghostex_questions: Vec<Value> = raw_questions
        .iter()
        .map(|q| {
            let question_text = q
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("Choose an option.");
            let multi_select = q
                .get("multiSelect")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let options: Vec<Value> = q
                .get("options")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|opt| {
                            let label = opt.get("label").and_then(Value::as_str)?;
                            let desc = opt
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or(label);
                            Some(json!({
                                "label": label,
                                "description": desc,
                            }))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let options = if options.is_empty() {
                vec![json!({ "label": "OK", "description": "Continue" })]
            } else {
                options
            };
            json!({
                "question": question_text,
                "header": "Question",
                "multiSelect": multi_select,
                "allowCustom": true,
                "options": options,
            })
        })
        .collect();

    let first_question = raw_questions
        .first()
        .and_then(|q| q.get("question"))
        .and_then(Value::as_str)
        .unwrap_or("Question")
        .to_string();

    let prompt_override = json!({
        "kind": "question",
        "toolUseId": tool_call_id,
        "questions": ghostex_questions,
    });

    GrokPendingRequest {
        rpc_id,
        tool_name: "AskUserQuestion".to_string(),
        summary: first_question,
        tool_use_id: tool_call_id,
        reason: None,
        options: Vec::new(),
        kind: PendingPromptKind::AskUserQuestion { raw_questions },
        custom_prompt_override: Some(prompt_override),
    }
}

pub(super) fn build_xai_ask_user_question_response(
    raw_questions: &[Value],
    selections: &[Value],
) -> Value {
    let mut answers_map = Map::new();
    let mut annotations_map = Map::new();

    for (idx, q) in raw_questions.iter().enumerate() {
        let q_text = q
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if q_text.is_empty() {
            continue;
        }
        let Some(sel) = selections.get(idx) else {
            continue;
        };
        let multi_select = q
            .get("multiSelect")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let options = q.get("options").and_then(Value::as_array);
        let mut selected_labels: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        let mut preview: Option<String> = None;

        if let Some(indices) = sel.get("indices").and_then(Value::as_array) {
            for i in indices.iter().filter_map(Value::as_u64) {
                if let Some(opt) = options.and_then(|arr| arr.get(i as usize)) {
                    if let Some(label) = opt.get("label").and_then(Value::as_str) {
                        selected_labels.push(label.to_string());
                        if !multi_select && preview.is_none() {
                            preview = non_empty_str(opt.get("preview")).map(str::to_string);
                        }
                    }
                }
            }
        }
        if let Some(other) = non_empty_str(sel.get("other")) {
            // Check if "other" matches a preset option label.
            let matched = options.and_then(|arr| {
                arr.iter().find(|opt| {
                    opt.get("label")
                        .and_then(Value::as_str)
                        .is_some_and(|l| l == other)
                })
            });
            if let Some(opt) = matched {
                if let Some(label) = opt.get("label").and_then(Value::as_str) {
                    selected_labels.push(label.to_string());
                    if !multi_select && preview.is_none() {
                        preview = non_empty_str(opt.get("preview")).map(str::to_string);
                    }
                }
            } else {
                notes.push(other.to_string());
            }
        }

        if selected_labels.is_empty() && notes.is_empty() {
            continue;
        }
        let final_labels = if selected_labels.is_empty() {
            vec!["Other".to_string()]
        } else {
            selected_labels
        };
        answers_map.insert(q_text.to_string(), json!(final_labels));

        if preview.is_some() || !notes.is_empty() {
            let mut ann = Map::new();
            if let Some(p) = preview {
                ann.insert("preview".to_string(), json!(p));
            }
            if !notes.is_empty() {
                ann.insert("notes".to_string(), json!(notes.join("\n")));
            }
            annotations_map.insert(q_text.to_string(), Value::Object(ann));
        }
    }

    if answers_map.is_empty() {
        return json!({ "outcome": "cancelled" });
    }
    let mut resp = Map::new();
    resp.insert("outcome".to_string(), json!("accepted"));
    resp.insert("answers".to_string(), Value::Object(answers_map));
    if !annotations_map.is_empty() {
        resp.insert("annotations".to_string(), Value::Object(annotations_map));
    }
    Value::Object(resp)
}

pub(super) fn parse_xai_exit_plan_mode_request(
    rpc_id: Value,
    params: &Value,
    fallback_plan: Option<String>,
) -> GrokPendingRequest {
    let unwrapped = unwrap_xai_ext_params(params);
    let tool_call_id = non_empty_str(unwrapped.get("toolCallId")).map(str::to_string);
    let _plan_content = non_empty_str(unwrapped.get("planContent"))
        .map(str::to_string)
        .or(fallback_plan);
    let options = vec![
        AcpPermissionOption {
            option_id: "approved".to_string(),
            name: "Approve plan".to_string(),
            kind: "allow_once".to_string(),
        },
        AcpPermissionOption {
            option_id: "abandoned".to_string(),
            name: "Decline".to_string(),
            kind: "reject_once".to_string(),
        },
    ];
    GrokPendingRequest {
        rpc_id,
        tool_name: "ExitPlanMode".to_string(),
        summary: "Approve plan and start implementing".to_string(),
        tool_use_id: tool_call_id,
        reason: None,
        options,
        kind: PendingPromptKind::ExitPlanMode,
        custom_prompt_override: None,
    }
}

pub(super) fn respond_pending_request_cancelled(
    transport: &GrokTransport,
    req: GrokPendingRequest,
) {
    let payload = match req.kind {
        PendingPromptKind::Permission => json!({ "outcome": { "outcome": "cancelled" } }),
        PendingPromptKind::AskUserQuestion { .. } => json!({ "outcome": "cancelled" }),
        PendingPromptKind::ExitPlanMode => json!({
            "outcome": "abandoned",
            "feedback": "Cancelled.",
        }),
    };
    let _ = transport.respond_to_agent(req.rpc_id, payload);
}

pub(super) fn cancel_all_pending_requests(transport: &GrokTransport, state: &Mutex<GrokState>) {
    let drained: Vec<GrokPendingRequest> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_requests.drain(..).collect()
    };
    for req in drained {
        respond_pending_request_cancelled(transport, req);
    }
}
