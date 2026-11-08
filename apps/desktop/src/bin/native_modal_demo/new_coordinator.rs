//! New Orchestrator preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (Claude
//! selected, Opus 5.5 at High), `codex`, `cursor` (an agent whose launch takes no model),
//! `antigravity` (no XHigh: Medium and High), `none` (no agent an orchestrator can run on),
//! `overrides` (the user wrote playbook overrides), `overrides-edit` (their editor open),
//! `old-playbook` (an edited full playbook saved before overrides is still in use, editor open),
//! `view` (the read-only playbook page, no overrides), `overrides-view` (the page with overrides),
//! `overrides-view-end` (that page scrolled to the overrides),
//! `effort-menu` (the Effort menu open), `all-efforts` (Show all effort levels on, the Effort menu
//! open on every level of the model).
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
            NewCoordinatorModalCommand::SavePlaybook { change, reply } => {
                let mut view = demo_playbook_view(&demo_state);
                match change {
                    PlaybookChange::Overrides(text) => {
                        eprintln!("save overrides: {text:?}");
                        view["overrides"]["text"] = serde_json::json!(text);
                    }
                    PlaybookChange::UseGhostex => {
                        eprintln!("use Ghostex's playbook");
                        view["source"] = serde_json::json!("shipped");
                        view["oldCopies"] = serde_json::json!([]);
                        view["savedFiles"] = serde_json::json!([
                            "~/.ghostex/coordinators/old-playbooks/all-projects-2026-11-07.md"
                        ]);
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
        playbook: OrchestratorPlaybookInfo::from_view(&demo_playbook_view(&demo.state)),
        palette: demo.palette,
        all_efforts: demo.state == "all-efforts",
    };
    let open_effort_menu = matches!(demo.state.as_str(), "effort-menu" | "all-efforts");
    let open_editor = matches!(demo.state.as_str(), "overrides-edit" | "old-playbook");
    let open_view = demo.state.ends_with("view") || demo.state.ends_with("view-end");
    let view_end = demo.state.ends_with("view-end");
    let _ = super::open_modal_window(
        NEW_COORDINATOR_MODAL_WIDTH,
        NEW_COORDINATOR_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| {
                let mut modal = GpuiNewCoordinatorModalWindow::new(config, host, window, cx);
                if open_effort_menu {
                    modal.toggle_effort_menu(window, cx);
                }
                if open_editor {
                    modal.open_playbook_editor(window, cx);
                }
                modal.set_playbook_guide(Ok(demo_guide()), cx);
                if open_view {
                    modal.open_playbook_view(window, cx);
                }
                if view_end {
                    // Applied when the page first lays out its content.
                    modal.scroll_playbook_view_to_end();
                }
                modal
            })
        },
        cx,
    );
}

/// The scripted `/api/readCoordinatorPlaybook` answer for a demo state.
fn demo_playbook_view(state: &str) -> serde_json::Value {
    let overrides = if state.starts_with("overrides") {
        "Never push without asking.\nIgnore the board rules.\nThreads never restart the app."
    } else {
        ""
    };
    let old = state == "old-playbook";
    serde_json::json!({
        "source": if old { "allProjects" } else { "shipped" },
        "overrides": { "text": overrides },
        "oldCopies": if old {
            serde_json::json!([{ "scope": "allProjects", "enabled": true }])
        } else {
            serde_json::json!([])
        },
    })
}

/// The scripted `/api/readCoordinatorGuide` answer: the playbook Ghostex ships (the skill's body)
/// and the marked reply format.
pub(super) fn demo_guide() -> String {
    let skill = include_str!("../../../../../skills/ghostex-orchestrator/SKILL.md");
    let body = skill
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(skill, |(_, body)| body)
        .trim();
    format!(
        "{body}\n\nReply format: the user chose marked handovers. Start each status and action section with its marker.\n"
    )
}
