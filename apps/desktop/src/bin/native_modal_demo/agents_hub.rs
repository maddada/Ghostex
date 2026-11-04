//! Agents Hub preview, on the React stories' fixtures (the catalog of
//! packages/core-ui/agents-hub-modal.stories.tsx (deleted 2026-10-01) and the Agent Sync report and plan of
//! packages/core-ui/agents-hub-sync/agent-sync-fixture.ts (deleted 2026-10-01), dumped to agents_hub_fixture/).
//! States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `skills` (default), `wrap` (Skills with the wrap toggle on), `mds`, `hooks`, `configs`,
//! `expanded` (Configs with its first group open), `profiles` (MDs with a group open),
//! `search` (Skills filtered by "asc"), `nomatch`, `dirty` (an unsaved edit), `empty` (an empty
//! catalog), `loading` (no catalog yet), `fileloading` (the file body never arrives),
//! `fileerror` (the file cannot be read), `sync` (Agent Sync overview), `syncfix` (a fix row
//! open), `syncdetail` (Kiro CLI), `syncnotinstalled`, `syncplan` (the plan sheet), `syncplanempty` (the sheet when nothing needs changing), `syncresult`
//! (the sheet after an apply with a failure), `syncsynced` (every agent in sync), `syncloading`
//! (no report yet), `syncerror` (the scan failed), `syncprofiles` (All filter, Claude Code's
//! profiles open). The `key*` states drive the keyboard contract through
//! `Window::dispatch_keystroke` and log the outcome: `keytab` presses Cmd/Ctrl+3 (Hooks),
//! `keysync` Cmd/Ctrl+5, `keysave` edits the file and presses Cmd/Ctrl+S, `keyescape` closes.
use super::agents_hub_modal::*;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

type Slot = Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiAgentsHubModalWindow>)>>>;

const SKILLS: [&str; 14] = [
    "tooltip-cleanup",
    "agent-reviews",
    "apple",
    "asc-app-create-ui",
    "asc-apple-ads",
    "asc-aso-audit",
    "asc-build-lifecycle",
    "asc-cli-usage",
    "asc-crash-triage",
    "asc-id-resolver",
    "asc-localize-metadata",
    "asc-metadata-sync",
    "asc-notarization",
    "asc-ppp-pricing",
];

fn claude_profile() -> serde_json::Value {
    json!({
        "agentIcon": "claude",
        "filePath": "/Users/madda/.claude/CLAUDE.md",
        "label": "Claude Code",
        "profilePath": "/Users/madda/.claude",
    })
}

/// The stories' `mockCatalog`, metadata only, as the desktop sends it.
fn catalog_json(empty: bool) -> serde_json::Value {
    if empty {
        return json!({
            "generatedAt": "2026-08-24T09:00:00.000Z",
            "groupsByTab": { "configs": [], "hooks": [], "mds": [], "skills": [] },
            "type": "agentsHubCatalog",
        });
    }
    let skills: Vec<serde_json::Value> = SKILLS
        .iter()
        .map(|name| {
            let root = if *name == "tooltip-cleanup" {
                format!("/Users/madda/agents/skills/{name}")
            } else {
                format!("/Users/madda/.agents/skills/{name}")
            };
            let profile = if *name == "tooltip-cleanup" {
                json!({
                    "agentIcon": "codex",
                    "filePath": "/Users/madda/.codex/AGENTS.md",
                    "label": "Codex main",
                    "profilePath": "/Users/madda/.codex",
                    "targetPath": "/Users/madda/.agents/main.md",
                })
            } else {
                claude_profile()
            };
            json!({
                "description": if *name == "tooltip-cleanup" {
                    "Shared skill installed under ~/agents/skills."
                } else {
                    "Shared skill installed under ~/.agents/skills."
                },
                "files": [{
                    "id": format!("{name}-skill"),
                    "language": "markdown",
                    "name": "SKILL.md",
                    "path": format!("{root}/SKILL.md"),
                }],
                "id": format!("skill-shared-{name}"),
                "name": name,
                "path": root,
                "profiles": [profile],
            })
        })
        .collect();
    json!({
        "generatedAt": "2026-05-15T11:41:00.000Z",
        "groupsByTab": {
            "configs": [
                {
                    "description": "MCP servers and CLI config owned by the Codex profile.",
                    "files": [
                        { "id": "codex-config", "language": "json", "name": "config.toml", "path": "/Users/madda/.codex/config.toml" },
                        { "id": "codex-mcp", "language": "json", "name": "mcp.json", "path": "/Users/madda/.codex/mcp.json" },
                    ],
                    "id": "config-codex",
                    "name": "Codex configuration",
                    "path": "/Users/madda/.codex",
                    "profiles": [
                        { "agentIcon": "codex", "filePath": "/Users/madda/.codex/config.toml", "label": "Codex main", "profilePath": "/Users/madda/.codex" },
                        { "agentIcon": "claude", "filePath": "/Users/madda/.claude-profiles/work/settings.json", "label": "Claude Code work", "profilePath": "/Users/madda/.claude-profiles/work" },
                    ],
                },
                {
                    "description": "Claude Code settings for every installed profile.",
                    "files": [
                        { "id": "claude-settings", "language": "json", "name": "settings.json", "path": "/Users/madda/.claude/settings.json" },
                    ],
                    "id": "config-claude",
                    "name": "Claude Code settings",
                    "path": "/Users/madda/.claude",
                    "profiles": [
                        { "agentIcon": "claude", "filePath": "/Users/madda/.claude/settings.json", "label": "Claude Code main", "profilePath": "/Users/madda/.claude" },
                    ],
                },
            ],
            "hooks": [],
            "mds": [
                {
                    "description": "Shared instructions and best-practice markdown linked by agent profiles.",
                    "files": [
                        { "id": "shared-agents", "language": "markdown", "name": "AGENTS.md", "path": "/Users/madda/.agents/AGENTS.md" },
                        { "id": "shared-main", "language": "markdown", "name": "main.md", "path": "/Users/madda/.agents/main.md" },
                        { "id": "shared-best-practices", "language": "markdown", "name": "best-practices.md", "path": "/Users/madda/.agents/best-practices.md" },
                    ],
                    "id": "md-shared-agents",
                    "name": "Shared agent markdown",
                    "path": "/Users/madda/.agents",
                    "profiles": [],
                },
                {
                    "description": "CLAUDE.md files owned by Claude profiles.",
                    "files": [
                        { "id": "claude-code-work", "language": "markdown", "name": "work/CLAUDE.md", "path": "/Users/madda/.claude-profiles/work/CLAUDE.md" },
                    ],
                    "id": "md-claude-profiles",
                    "name": "Claude profile instructions",
                    "path": "/Users/madda/.claude-profiles",
                    "profiles": [
                        { "agentIcon": "claude", "filePath": "/Users/madda/.claude-profiles/work/CLAUDE.md", "label": "Claude Code work", "profilePath": "/Users/madda/.claude-profiles/work", "targetPath": "/Users/madda/.agents/main.md" },
                    ],
                },
            ],
            "skills": skills,
        },
        "type": "agentsHubCatalog",
    })
}

/// The stories' inline file bodies, answered the way the desktop answers a file request.
fn file_body(path: &str) -> String {
    if let Some(name) = path
        .strip_suffix("/SKILL.md")
        .and_then(|root| root.rsplit('/').next())
    {
        return format!(
            "---\nname: {name}\n---\n\nSkill instructions. Run all CLI commands using the project's package runner, check the registries before writing anything by hand, and keep each component composable so the wrap toggle has a long line to fold.\n\n## Next\n\nMore instructions."
        );
    }
    match path {
        "/Users/madda/.codex/config.toml" => "{\n  \"mcpServers\": {}\n}\n".to_string(),
        "/Users/madda/.codex/mcp.json" => "{\n  \"servers\": []\n}\n".to_string(),
        "/Users/madda/.claude/settings.json" => "{\n  \"permissions\": {}\n}\n".to_string(),
        "/Users/madda/.agents/AGENTS.md" => "Follow the instructions in main.md.".to_string(),
        "/Users/madda/.agents/main.md" => {
            "# Shared instructions\n\nInstructions used by every agent profile.".to_string()
        }
        "/Users/madda/.agents/best-practices.md" => {
            "# Best practices\n\nProject workflow guidance.".to_string()
        }
        _ => "# Claude Code work\n\nProject instructions.".to_string(),
    }
}

fn sync_report(synced: bool) -> serde_json::Value {
    let text = if synced {
        include_str!("agents_hub_fixture/sync-report-synced.json")
    } else {
        include_str!("agents_hub_fixture/sync-report.json")
    };
    serde_json::from_str(text).expect("the Agent Sync fixture report")
}

/// The fixture plan with the groups that have nothing to change the app's planner also reports
/// (`moveBackups`, `removeRedundantLinks`), which the sheet must leave out; `syncplanempty` has
/// no change in any group at all.
fn sync_plan(all_in_sync: bool) -> serde_json::Value {
    let mut plan: serde_json::Value =
        serde_json::from_str(include_str!("agents_hub_fixture/sync-plan.json"))
            .expect("the Agent Sync fixture plan");
    let groups = plan["groups"].as_array_mut().expect("plan groups");
    if all_in_sync {
        groups.clear();
    }
    for (kind, title) in [
        ("moveBackups", "Move backup folders out of skills folders"),
        (
            "removeRedundantLinks",
            "Remove links agents that read ~/.agents/skills do not need",
        ),
    ] {
        groups.insert(
            0,
            json!({
                "changeCount": 0,
                "enabledByDefault": true,
                "kind": kind,
                "ops": [],
                "title": title,
            }),
        );
    }
    plan
}

/// Answers after the window exists, the way the app answers from its background executor.
fn later(cx: &mut App, answer: impl FnOnce(&mut App) + 'static) {
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(30))
            .await;
        cx.update(answer);
    })
    .detach();
}

fn deliver(
    slot: &Slot,
    cx: &mut App,
    update: impl FnOnce(
        &mut GpuiAgentsHubModalWindow,
        &mut gpui::Window,
        &mut gpui::Context<GpuiAgentsHubModalWindow>,
    ),
) {
    let target = slot.borrow().clone();
    let Some((window, view)) = target else {
        eprintln!("agents hub demo: no window yet");
        return;
    };
    if let Err(error) = window.update(cx, |_root, window, cx| {
        view.update(cx, |modal, cx| update(modal, window, cx));
    }) {
        eprintln!("agents hub demo: {error}");
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    gpui_component::Theme::change(
        if demo.palette.light {
            gpui_component::ThemeMode::Light
        } else {
            gpui_component::ThemeMode::Dark
        },
        None,
        cx,
    );
    let initial_tab = match state.as_str() {
        "mds" | "profiles" => AgentsHubTab::Mds,
        "hooks" => AgentsHubTab::Hooks,
        "configs" | "expanded" | "empty" => AgentsHubTab::Configs,
        s if s.starts_with("sync") => AgentsHubTab::Sync,
        _ => AgentsHubTab::Skills,
    };
    let slot: Slot = Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let host_state = state.clone();
    let host: AgentsHubModalHost = Rc::new(move |command, cx: &mut App| {
        let slot = host_slot.clone();
        let state = host_state.clone();
        match command {
            AgentsHubModalCommand::RequestCatalog => {
                if state == "loading" {
                    return;
                }
                let catalog = AgentsHubCatalog::from_json(catalog_json(state == "empty"));
                if catalog.is_none() {
                    eprintln!("agents hub demo: the catalog fixture did not parse");
                }
                later(cx, move |cx| {
                    if let Some(catalog) = catalog {
                        deliver(&slot, cx, |modal, window, cx| {
                            modal.receive_catalog(catalog, window, cx)
                        });
                    }
                });
            }
            AgentsHubModalCommand::RequestFileContent {
                file_path,
                request_id,
            } => {
                if state == "fileloading" {
                    return;
                }
                let answer = if state == "fileerror" {
                    json!({
                        "errorMessage": "Unable to load file contents.",
                        "filePath": file_path,
                        "requestId": request_id,
                        "type": "agentsHubFileContent",
                    })
                } else {
                    json!({
                        "content": file_body(&file_path),
                        "filePath": file_path,
                        "requestId": request_id,
                        "type": "agentsHubFileContent",
                    })
                };
                let answer = AgentsHubFileContent::from_json(answer);
                later(cx, move |cx| {
                    if let Some(answer) = answer {
                        deliver(&slot, cx, |modal, window, cx| {
                            modal.receive_file_content(answer, window, cx)
                        });
                    }
                });
            }
            AgentsHubModalCommand::SaveFile { file_path, content } => {
                eprintln!("save {file_path}: {} bytes", content.len());
            }
            AgentsHubModalCommand::OpenPath { path } => eprintln!("open path {path}"),
            AgentsHubModalCommand::OpenInBuiltInEditor { file_path } => {
                eprintln!("open in built-in editor {file_path}");
                cx.quit();
            }
            AgentsHubModalCommand::Copied => eprintln!("copied (copy sound)"),
            AgentsHubModalCommand::SetWordWrap { wrap } => eprintln!("word wrap {wrap}"),
            AgentsHubModalCommand::RequestSyncReport => {
                if state == "syncloading" {
                    return;
                }
                let report = if state == "syncerror" {
                    SyncReport::from_json(json!({
                        "errorMessage": "HOME is not set.",
                        "type": "agentSyncReport",
                    }))
                } else {
                    SyncReport::from_json(sync_report(state == "syncsynced"))
                };
                later(cx, move |cx| {
                    if let Some(report) = report {
                        deliver(&slot, cx, |modal, _window, cx| {
                            modal.receive_sync_report(report, cx)
                        });
                    }
                });
            }
            AgentsHubModalCommand::RequestSyncPlan { scope } => {
                let mut plan = sync_plan(state == "syncplanempty");
                plan["scope"] = json!(scope);
                let plan = SyncPlan::from_json(plan);
                later(cx, move |cx| {
                    if let Some(plan) = plan {
                        deliver(&slot, cx, |modal, _window, cx| {
                            modal.receive_sync_plan(plan, cx)
                        });
                    }
                });
            }
            AgentsHubModalCommand::ApplySyncPlan { scope, groups } => {
                eprintln!("apply {scope}: {groups:?}");
                let result = SyncApplyResult::from_json(apply_result(&scope));
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    cx.update(|cx| {
                        if let Some(result) = result {
                            deliver(&slot, cx, |modal, _window, cx| {
                                modal.receive_sync_apply_result(result, cx)
                            });
                        }
                    });
                })
                .detach();
            }
            AgentsHubModalCommand::Close => {
                eprintln!("close");
                cx.quit();
            }
        }
    });
    let config = AgentsHubModalConfig {
        palette: demo.palette,
        initial_tab,
        // `skill:<name>` opens the Hub on that skill, as the Session details panel's skill rows do.
        initial_skill: state.strip_prefix("skill:").map(str::to_string),
        initial_skill_path: None,
        word_wrap: state == "wrap",
    };
    let (window, view) = super::open_large_modal_window(
        AGENTS_HUB_MODAL_WIDTH,
        AGENTS_HUB_MODAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiAgentsHubModalWindow::new(config, host, window, cx)),
        cx,
    );
    *slot.borrow_mut() = Some((window, view));
    let preview_slot = slot.clone();
    cx.spawn(async move |cx| {
        // Let the catalog, the file body and the report land before the preview state is set.
        cx.background_executor()
            .timer(Duration::from_millis(300))
            .await;
        cx.update(|cx| apply_preview_state(&preview_slot, &state, cx));
    })
    .detach();
}

fn apply_result(scope: &str) -> serde_json::Value {
    json!({
        "done": [
            { "path": "~/.kiro/skills/faster-chrome-devtools-skill", "verb": "unlink" },
            { "path": "~/.claude/skills/ghostex-cli", "target": "../../.agents/skills/ghostex-cli", "verb": "link" },
        ],
        "enabledGroups": ["removeDangling", "perSkillLinks"],
        "failed": [
            {
                "error": "Permission denied (os error 13)",
                "op": { "path": "~/.config/opencode/skills/test", "verb": "unlink" },
            }
        ],
        "generatedAt": "2026-09-16T12:00:00.000Z",
        "plan": sync_plan(false),
        "scope": scope,
        "skippedKeeps": 3,
        "type": "agentSyncApplyResult",
    })
}

/// Presses `keys` in the Hub window, the way a user would.
fn press(slot: &Slot, keys: &str, cx: &mut App) {
    let target = slot.borrow().clone();
    let Some((window, _view)) = target else {
        return;
    };
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let keys = keys.replace("primary", primary);
    // Through the untyped handle: the typed one leases `Root`, which the dispatched event
    // re-enters while it re-renders the window.
    let any_window: gpui::AnyWindowHandle = window.into();
    let _ = any_window.update(cx, |_root, window, cx| {
        if let Ok(keystroke) = gpui::Keystroke::parse_with_os(&keys, cx.platform_info().os) {
            window.dispatch_keystroke(keystroke, cx);
        }
    });
}

fn apply_preview_state(slot: &Slot, state: &str, cx: &mut App) {
    match state {
        "keytab" => return press(slot, "primary-3", cx),
        "keysync" => return press(slot, "primary-5", cx),
        "keyescape" => return press(slot, "escape", cx),
        "keysave" => {
            deliver(slot, cx, |modal, window, cx| {
                modal.preview_type_in_editor(" More instructions.", window, cx)
            });
            return press(slot, "primary-s", cx);
        }
        _ => {}
    }
    let state = state.to_string();
    deliver(slot, cx, move |modal, window, cx| match state.as_str() {
        "expanded" => modal.preview_expand_group(0, window, cx),
        "profiles" => modal.preview_expand_group(1, window, cx),
        "search" => modal.preview_search("asc", window, cx),
        "nomatch" => modal.preview_search("zzz", window, cx),
        "dirty" => modal.preview_type_in_editor("\nMore instructions.", window, cx),
        "syncdetail" => modal.preview_select_sync_agent("kiro-cli", cx),
        "syncnotinstalled" => modal.preview_select_sync_agent("goose", cx),
        "syncfix" => modal.preview_open_fix("danglingLinks", cx),
        "syncplan" | "syncplanempty" => modal.open_sync_plan("all".to_string(), None, cx),
        "syncresult" => {
            modal.open_sync_plan("all".to_string(), None, cx);
            modal.apply_sync_plan(cx);
        }
        "syncprofiles" => modal.preview_sync_filter_all_with_profiles("claude-code", cx),
        _ => {}
    });
}
