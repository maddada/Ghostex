//! New Orchestrator preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (Claude
//! selected, Opus 5.5 at High), `codex`, `cursor` (an agent whose launch takes no model),
//! `antigravity` (no XHigh: Medium and High), `none` (no agent an orchestrator can run on),
//! `overrides` (the user wrote playbook overrides), `overrides-long` (long ones, over 20,000
//! characters), `overrides-edit` (their editor open),
//! `old-playbook` (an edited full playbook saved before overrides is still in use, editor open),
//! `view` (the read-only playbook page, no overrides), `overrides-view` (the page with overrides),
//! `overrides-view-end` (that page scrolled to the overrides),
//! `effort-menu` (the Effort menu open), `all-efforts` (Show all effort levels on, the Effort menu
//! open on every level of the model). A project on several computers: `computers` (two clones
//! here with the same folder name, mini with other launchers, mbpro not connected), `computer-menu`
//! (its list open), `computer-unavailable` (opened on mbpro, which is not connected),
//! `computer-no-agents` (picked a computer with no coordinator agent), `computer-loading` (mini's
//! models still being read), `computer-stale-playbook` (mini picked while this computer's
//! playbook read is still on its way: the late answer is dropped).
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
    let family = id.split('-').next().unwrap_or(id);
    NewCoordinatorAgent {
        agent_id: id.to_string(),
        name: name.to_string(),
        family: family.to_string(),
        models,
        skill_line: ghostex_gx_chat_core::composer::skill_invocation::coordinator_skill_line(Some(
            family,
        )),
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
    let computers = demo.state.starts_with("computer");
    let host: NewCoordinatorModalHost = Rc::new(move |command, cx: &mut App| {
        match command {
            NewCoordinatorModalCommand::Create {
                group_id,
                agent_id,
                name,
                model,
                effort,
                reply,
            } => {
                eprintln!("create: {group_id} {agent_id} {name:?} {model:?} {effort:?}");
                // The app's host answers after the dialog's own update, as here.
                cx.defer(move |cx| {
                    reply.finish(Ok(()), cx);
                    cx.quit();
                });
                return;
            }
            NewCoordinatorModalCommand::ChooseCopy {
                group_id,
                generation,
            } => {
                eprintln!("choose copy: {group_id} (playbook read {generation})");
                return;
            }
            NewCoordinatorModalCommand::RefreshCopies => {
                eprintln!("refresh copies");
                return;
            }
            NewCoordinatorModalCommand::SavePlaybook {
                group_id,
                change,
                reply,
            } => {
                eprintln!("save playbook of {group_id}");
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
                cx.defer(move |cx| reply.finish(Ok(view), cx));
                return;
            }
            NewCoordinatorModalCommand::Cancel => eprintln!("cancel"),
        }
        cx.quit();
    });
    let copies = if computers {
        demo_copies(&demo.state, agents)
    } else {
        vec![NewCoordinatorCopy {
            group_id: "combined-project:local:project-1".to_string(),
            label: String::new(),
            computer: String::new(),
            connected: true,
            gone: false,
            agents: Some(agents),
            lineup: CopyLineup::Held,
        }]
    };
    let selected_copy = match demo.state.as_str() {
        "computer-unavailable" => 3,
        _ => 0,
    };
    let pick_copy = match demo.state.as_str() {
        "computer-no-agents" => Some(3usize.min(copies.len() - 1)),
        "computer-loading" | "computer-stale-playbook" => Some(2),
        _ => None,
    };
    let open_computer_menu = demo.state == "computer-menu";
    let stale_playbook = demo.state == "computer-stale-playbook";
    let config = NewCoordinatorModalConfig {
        project_name: "Ghostex".to_string(),
        copies,
        selected_copy,
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
        NEW_COORDINATOR_MODAL_INITIAL_HEIGHT
            + if computers {
                NEW_COORDINATOR_COMPUTER_FIELD_HEIGHT
            } else {
                0.0
            },
        move |window, cx| {
            cx.new(|cx| {
                let mut modal = GpuiNewCoordinatorModalWindow::new(config, host, window, cx);
                if open_effort_menu {
                    modal.toggle_effort_menu(window, cx);
                }
                if let Some(index) = pick_copy {
                    modal.demo_choose_copy(index, cx);
                }
                if stale_playbook {
                    // This computer's read was sent for pick 0 and lands after mini was picked.
                    modal.set_playbook_for(
                        0,
                        OrchestratorPlaybookInfo::from_view(&demo_playbook_view("overrides")),
                        cx,
                    );
                }
                if open_computer_menu {
                    modal.demo_toggle_copy_menu(window, cx);
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

/// The `computer…` states' copies: two clones here that share a folder name, mini (Codex and a
/// Claude launcher named differently, no Opus row), and mbpro, which is not connected. In
/// `computer-no-agents` mbpro is connected but has no agent a coordinator runs on.
fn demo_copies(state: &str, here: Vec<NewCoordinatorAgent>) -> Vec<NewCoordinatorCopy> {
    let copy = |group: &str, label: &str, computer: &str, connected: bool| NewCoordinatorCopy {
        group_id: group.to_string(),
        label: label.to_string(),
        computer: computer.to_string(),
        connected,
        gone: false,
        agents: Some(here.clone()),
        lineup: CopyLineup::Held,
    };
    let claude_levels = ["low", "medium", "high", "xhigh", "max"];
    let mini_agents = vec![
        agent(
            "claude-work",
            "Claude (work account)",
            vec![model("sonnet", "Sonnet 5.5", &claude_levels, true)],
        ),
        agent(
            "codex",
            "Codex",
            vec![model(
                "gpt-6.1-sol",
                "GPT-6.1 Sol",
                &["low", "medium", "high"],
                true,
            )],
        ),
    ];
    let loading = state == "computer-loading";
    let no_agents = state == "computer-no-agents";
    vec![
        copy(
            "combined-project:local:project-1",
            "This computer · customer-a/ghostex",
            "This computer",
            true,
        ),
        copy(
            "combined-project:local:project-2",
            "This computer · customer-b/ghostex",
            "This computer",
            true,
        ),
        NewCoordinatorCopy {
            agents: Some(if loading {
                mini_agents
                    .iter()
                    .cloned()
                    .map(|agent| NewCoordinatorAgent {
                        models: Vec::new(),
                        ..agent
                    })
                    .collect()
            } else {
                mini_agents
            }),
            lineup: if loading {
                CopyLineup::Loading
            } else {
                CopyLineup::Held
            },
            ..copy("remote:mini:project:p1", "mini", "mini", true)
        },
        NewCoordinatorCopy {
            agents: Some(if no_agents { Vec::new() } else { here.clone() }),
            ..copy("remote:mbpro:project:p9", "mbpro", "mbpro", no_agents)
        },
    ]
}

/// The scripted `/api/readCoordinatorPlaybook` answer for a demo state.
fn demo_playbook_view(state: &str) -> serde_json::Value {
    let overrides = if state == "overrides-long" {
        // Past the 20,000 characters that make overrides long (GitHub #584).
        let rule = "Every thread runs the review loop twice before it lands anything that touches gxserver.";
        let mut text = "Never push without asking.".to_string();
        while text.chars().count() <= 24_000 {
            text.push_str("\n\n");
            text.push_str(rule);
        }
        text
    } else if state.starts_with("overrides") {
        "Never push without asking.\nIgnore the board rules.\nThreads never restart the app."
            .to_string()
    } else {
        String::new()
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

/// The scripted `/api/readCoordinatorGuide` answer: the playbook Ghostex ships and the marked
/// reply format.
pub(super) fn demo_guide() -> String {
    let body = include_str!("../../../../../server/src/coordinators/playbook.md").trim();
    format!(
        "{body}\n\nReply format: the user chose marked handovers. Start each status and action section with its marker.\n"
    )
}
