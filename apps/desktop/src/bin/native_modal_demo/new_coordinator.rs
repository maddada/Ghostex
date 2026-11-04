//! New Orchestrator preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (Claude
//! selected, Opus 5.5 at High), `codex`, `cursor` (an agent whose launch takes no model),
//! `antigravity` (no XHigh: Medium and High), `none` (no agent an orchestrator can run on),
//! `project-playbook` (the project has its own edited playbook), `playbook-off` (edited copies,
//! both off), `effort-menu` (the Effort menu open), `all-efforts` (Show all effort levels on, the
//! Effort menu open on every level of the model).
use super::new_coordinator_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

fn efforts(values: &[&str]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|value| {
            let label = match *value {
                "xhigh" => "XHigh".to_string(),
                other => {
                    let mut chars = other.chars();
                    chars.next().map_or_else(String::new, |first| {
                        first.to_uppercase().collect::<String>() + chars.as_str()
                    })
                }
            };
            (value.to_string(), label)
        })
        .collect()
}

fn model(value: &str, label: &str, levels: &[&str], default: bool) -> NewCoordinatorModel {
    NewCoordinatorModel {
        value: value.to_string(),
        label: label.to_string(),
        efforts: efforts(levels),
        default,
    }
}

fn agent(id: &str, name: &str, models: Vec<NewCoordinatorModel>) -> NewCoordinatorAgent {
    NewCoordinatorAgent {
        agent_id: id.to_string(),
        name: name.to_string(),
        models,
        skill_line: ghostex_gx_chat_core::composer::skill_invocation::orchestrator_skill_line(
            Some(id),
        ),
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let claude_levels = ["low", "medium", "high", "xhigh", "max", "ultracode"];
    let agents = if demo.state == "none" {
        Vec::new()
    } else {
        vec![
            agent(
                "claude",
                "Claude",
                vec![
                    model("fable", "Fable 5.1", &claude_levels, false),
                    model("opus[1m]", "Opus 5.5", &claude_levels, true),
                    model("sonnet", "Sonnet 5.5", &claude_levels, false),
                ],
            ),
            agent(
                "codex",
                "Codex",
                vec![model(
                    "gpt-6.1-sol",
                    "GPT-6.1 Sol",
                    &["low", "medium", "high", "xhigh", "max", "ultra"],
                    true,
                )],
            ),
            agent(
                "zcode",
                "ZCode",
                vec![model("zai/glm-5.3", "GLM 5.3", &[], true)],
            ),
            agent("cursor", "Cursor CLI", Vec::new()),
            agent("grok", "Grok Build", Vec::new()),
            agent(
                "antigravity",
                "Antigravity CLI",
                vec![model(
                    "gemini-4-argon",
                    "Gemini 4 Argon",
                    &["low", "medium", "high"],
                    true,
                )],
            ),
            agent("pi", "Pi Agent", Vec::new()),
            agent("omp", "OMP", Vec::new()),
        ]
    };
    let selected_agent = match demo.state.as_str() {
        "codex" => 1,
        "cursor" => 3,
        "antigravity" => 5,
        _ => 0,
    };
    let demo_state = demo.state.clone();
    let host: NewCoordinatorModalHost = Rc::new(move |command, cx: &mut App| {
        match command {
            NewCoordinatorModalCommand::Create {
                agent_id,
                name,
                model,
                effort,
            } => eprintln!("create: {agent_id} {name:?} {model:?} {effort:?}"),
            NewCoordinatorModalCommand::SavePlaybook {
                project_id,
                change,
                reply,
            } => {
                let reset = matches!(change, PlaybookChange::Reset);
                eprintln!("save playbook: {project_id:?} reset={reset}");
                let mut view = demo_playbook_view(&demo_state);
                if project_id.is_some() {
                    view["project"]["edited"] = serde_json::json!(!reset);
                    if let PlaybookChange::Text(text) = change {
                        view["project"]["text"] = serde_json::json!(text);
                    }
                } else {
                    view["allProjects"]["edited"] = serde_json::json!(!reset);
                    if let PlaybookChange::Text(text) = change {
                        view["allProjects"]["text"] = serde_json::json!(text);
                    }
                }
                reply.finish(Ok(view), cx);
                return;
            }
            NewCoordinatorModalCommand::Cancel => eprintln!("cancel"),
        }
        cx.quit();
    });
    let config = NewCoordinatorModalConfig {
        project_name: "Ghostex".to_string(),
        agents,
        selected_agent,
        // `project-playbook`: the project has its own edited playbook; `playbook-off`: edited
        // copies for all projects and the project, both off.
        playbook: Some(OrchestratorPlaybookInfo::from_view(&demo_playbook_view(
            &demo.state,
        ))),
        palette: demo.palette,
        all_efforts: demo.state == "all-efforts",
    };
    let open_effort_menu = matches!(demo.state.as_str(), "effort-menu" | "all-efforts");
    let _ = super::open_modal_window(
        NEW_COORDINATOR_MODAL_WIDTH,
        NEW_COORDINATOR_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| {
                let mut modal = GpuiNewCoordinatorModalWindow::new(config, host, window, cx);
                if open_effort_menu {
                    modal.toggle_effort_menu(window, cx);
                }
                modal
            })
        },
        cx,
    );
}

/// The scripted `/api/readCoordinatorPlaybook` answer for a demo state.
fn demo_playbook_view(state: &str) -> serde_json::Value {
    let skill = include_str!("../../../../../skills/ghostex-orchestrator/SKILL.md");
    let shipped = skill
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(skill, |(_, body)| body)
        .trim()
        .to_string();
    let edited_all = format!("{shipped}\n\n## House rules\n\nNever push without asking.");
    let off = state == "playbook-off";
    serde_json::json!({
        "allProjects": {
            "edited": off,
            "text": if off { edited_all.clone() } else { String::new() },
            "enabled": !off,
            "defaultText": shipped,
            "defaultChanged": false,
        },
        "project": {
            "projectId": "P1",
            "name": "Ghostex",
            "edited": off || state == "project-playbook",
            "text": "# Ghostex coordinator for this project\n\nKeep threads small.",
            "enabled": !off,
            "defaultText": shipped,
            "defaultChanged": state == "project-playbook",
        },
    })
}
