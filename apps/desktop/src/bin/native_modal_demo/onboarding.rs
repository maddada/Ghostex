//! Preview host for the native Onboarding, mirroring packages/core-ui/onboarding/onboarding-modal.stories.tsx (deleted 2026-10-01).
//! States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`), each the story of the same name: `welcome` (default),
//! `welcome-chat`, `welcome-mobile`, `agents`, `agents-scanning`, `agents-already-connected`,
//! `agents-none-installed`, `agents-install-available`, `agents-install-partial`,
//! `agents-installing`, `agents-install-failed`, `computer-use-installing`,
//! `computer-use-permissions`, `computer-use-on`, `computer-use-on-tab`, `agents-integration`, `agents-guide`,
//! `workspace`, `workspace-no-browser-skill`, `workspace-agents`, `workspace-docs`,
//! `workspace-code`, `workspace-kanban`, `workspace-automate`, `mobile`, `get-started`,
//! `get-started-no-folder`, `get-started-no-folder-with-projects`, `finished`, `finished-guide`,
//! and the first-run intro video page: `intro-video` (the real player in the system web view),
//! `intro-video-offline`, `intro-video-no-web-view` (what Linux shows), `intro-video-continue`
//! (presses Continue after 6s).
//! The host simulates the app: detection answers at once (after 1.4s on a rescan), the helper
//! install connects after 0.9s, CLI installs finish a few seconds after they start, Computer Use
//! asks for permission after 1.8s, and closing the flow quits the demo. `GHOSTEX_ONBOARDING_DEMO_SIZE=WxH`
//! opens the window at another size.
use super::onboarding::model::{COLOURFULNESS_LAST_POSITION, colourfulness_points};
use super::onboarding::*;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

const CATALOG: [(&str, &str, &str); 24] = [
    ("codex", "Codex", "codex"),
    ("claude", "Claude", "claude"),
    ("cursor", "Cursor CLI", "cursor-cli"),
    ("pi", "Pi Agent", "pi"),
    ("opencode", "OpenCode", "opencode"),
    ("gemini", "Gemini", "gemini"),
    ("copilot", "Copilot", "copilot"),
    ("droid", "Factory Droid", "factory-droid"),
    ("grok", "Grok Build", "grok-build"),
    ("antigravity", "Antigravity CLI", "antigravity-cli"),
    ("amp", "Amp CLI", "amp-cli"),
    ("hermes-agent", "Hermes Agent", "hermes-agent"),
    ("rovodev", "Rovo Dev", "rovo-dev"),
    ("codebuddy", "CodeBuddy", "codebuddy"),
    ("qoder", "Qoder", "qoder"),
    ("kiro", "Kiro CLI", "kiro"),
    ("omp", "OMP", "omp"),
    ("kimi", "Kimi Code", "kimi"),
    ("openclaude", "OpenClaude", "openclaude"),
    ("command-code", "Command Code", "command-code"),
    ("devin", "Devin", "devin"),
    ("mastra", "Mastra Code", "mastra"),
    ("zcode", "ZCode", "zcode"),
    ("empryo", "Empryo", "empryo"),
];

const STORY_OUTPUT: [&str; 5] = [
    "Resolving latest release…",
    "Downloading package (24.1 MB)…",
    "Verifying checksum…",
    "Linking binary into ~/.local/bin",
    "Done. Run the CLI once to sign in.",
];

fn catalog() -> Vec<CatalogAgent> {
    CATALOG
        .iter()
        .map(|(agent_id, name, icon)| CatalogAgent {
            agent_id: agent_id.to_string(),
            name: name.to_string(),
            icon: icon.to_string(),
        })
        .collect()
}

fn theme_table() -> ThemeTable {
    let fixture: Value =
        serde_json::from_str(include_str!("onboarding_theme.json")).unwrap_or_default();
    let hex = |value: &Value| {
        value
            .as_str()
            .and_then(|text| u32::from_str_radix(text.trim_start_matches('#'), 16).ok())
            .unwrap_or(0)
    };
    let presets = |key: &str| {
        fixture
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        let chrome = item
                            .get("chrome")
                            .and_then(Value::as_array)
                            .map(|chrome| chrome.iter().map(hex).collect::<Vec<_>>())
                            .unwrap_or_default();
                        ThemeSwatchPreset {
                            value: item
                                .get("value")
                                .and_then(Value::as_str)
                                .unwrap_or("gray")
                                .to_string(),
                            // The fixture holds the five named points; in-between positions show the nearest.
                            chrome: (0..=COLOURFULNESS_LAST_POSITION)
                                .map(|position| {
                                    chrome.get((position + 4) / 8).copied().unwrap_or(0)
                                })
                                .collect(),
                            accent: item.get("accent").map(hex).unwrap_or(0),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    ThemeTable {
        dark: presets("dark"),
        light: presets("light"),
        colourfulness_patches: fixture
            .get("patches")
            .and_then(Value::as_array)
            .map(|patches| {
                let named: Vec<_> = patches
                    .iter()
                    .filter_map(|patch| patch.as_object().cloned())
                    .collect();
                (0..=COLOURFULNESS_LAST_POSITION)
                    .filter_map(|position| {
                        let mut patch = named.get((position + 4) / 8)?.clone();
                        let points = json!(colourfulness_points(position));
                        patch.insert("themeSidebarContrast".into(), points.clone());
                        patch.insert("themeWorkAreaContrast".into(), points);
                        Some(patch)
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// The story's `mockAgents` as the host's `agentHookStatus` payload.
fn hook_status(installed: &HashSet<String>, hooks: bool, complete: bool) -> Value {
    json!({
        "type": "agentHookStatus",
        "complete": complete,
        "agents": CATALOG.iter().map(|(agent_id, _, _)| {
            let is_installed = installed.contains(*agent_id);
            json!({
                "agentId": agent_id,
                "cliInstalled": is_installed,
                "hookInstalled": is_installed && hooks,
                "status": if is_installed && hooks { "installed" } else { "missing" },
                "detail": if is_installed { format!("/opt/homebrew/bin/{agent_id}") } else { "CLI was not found on PATH.".to_string() },
            })
        }).collect::<Vec<_>>(),
    })
}

fn story_methods(agent_id: &str) -> Vec<Value> {
    match agent_id {
        "claude" => vec![
            json!({ "id": "native", "label": "Official installer", "command": "curl -fsSL https://claude.ai/install.sh | bash" }),
            json!({ "id": "npm", "label": "npm", "command": "npm install -g @anthropic-ai/claude-code@latest" }),
            json!({ "id": "brew", "label": "Homebrew", "command": "brew install --cask claude-code" }),
        ],
        "codex" => vec![
            json!({ "id": "npm", "label": "npm", "command": "npm install -g @openai/codex@latest" }),
            json!({ "id": "brew", "label": "Homebrew", "command": "brew install --cask codex" }),
        ],
        "cursor" => vec![
            json!({ "id": "native", "label": "Official installer", "command": "curl -fsSL https://cursor.com/install | bash" }),
        ],
        _ => vec![
            json!({ "id": "npm", "label": "npm", "command": format!("npm install -g {agent_id}@latest") }),
        ],
    }
}

struct DemoHost {
    window: Option<WindowHandle<Root>>,
    view: Option<Entity<GpuiOnboardingWindow>>,
    settings: serde_json::Map<String, Value>,
    installed: HashSet<String>,
    hooks: bool,
    rescan_answers: bool,
    installed_now: HashSet<String>,
    jobs: HashMap<String, Value>,
    cli_status: Value,
}

fn with_view(
    host: &Rc<RefCell<DemoHost>>,
    cx: &mut App,
    update: impl FnOnce(&mut GpuiOnboardingWindow, &mut gpui::Context<GpuiOnboardingWindow>) + 'static,
) {
    let (window, view) = {
        let host = host.borrow();
        (host.window, host.view.clone())
    };
    let (Some(window), Some(view)) = (window, view) else {
        return;
    };
    let _ = window.update(cx, |_, _, cx| view.update(cx, |view, cx| update(view, cx)));
}

fn later(cx: &mut App, delay: Duration, run: impl FnOnce(&mut App) + 'static) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(delay).await;
        let _ = cx.update(run);
    })
    .detach();
}

fn cli_state(host: &DemoHost, agent_id: &str) -> Value {
    let mut state = json!({
        "agentId": agent_id,
        "platform": "windows",
        "methods": story_methods(agent_id),
    });
    if host.installed_now.contains(agent_id) {
        state["executablePath"] = json!(format!("/opt/homebrew/bin/{agent_id}"));
    }
    if let Some(job) = host.jobs.get(agent_id) {
        state["job"] = job.clone();
    }
    state
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.as_str();
    let (
        initial_panel,
        installed,
        hooks,
        cli,
        computer_use,
        browser_skill,
        has_projects,
        picked,
        loading,
    ): (
        InitialPanel,
        &[&str],
        bool,
        &str,
        &str,
        bool,
        bool,
        Option<&str>,
        bool,
    ) = {
        let three: &[&str] = &["claude", "codex", "cursor"];
        let folder = Some("~/Projects/my-app");
        match state {
            "agents" | "agents-guide" | "agents-integration" => (
                InitialPanel::Panel(2),
                three,
                false,
                "none",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "agents-scanning" => (
                InitialPanel::Panel(2),
                three,
                false,
                "none",
                "off",
                true,
                false,
                folder,
                true,
            ),
            "agents-already-connected" => (
                InitialPanel::Panel(2),
                three,
                true,
                "none",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "agents-none-installed" => (
                InitialPanel::Panel(2),
                &[],
                false,
                "none",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "agents-install-available" => (
                InitialPanel::Panel(2),
                &[],
                false,
                "ready",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "agents-install-partial" => (
                InitialPanel::Panel(2),
                &["codex"],
                false,
                "ready",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "agents-installing" => (
                InitialPanel::Panel(2),
                &[],
                false,
                "installing",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "agents-install-failed" => (
                InitialPanel::Panel(2),
                &[],
                false,
                "failed",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "computer-use-installing" => (
                InitialPanel::Panel(2),
                three,
                false,
                "none",
                "installing",
                true,
                false,
                folder,
                false,
            ),
            "computer-use-permissions" => (
                InitialPanel::Panel(2),
                three,
                false,
                "none",
                "permissions",
                true,
                false,
                folder,
                false,
            ),
            "computer-use-on" | "computer-use-on-tab" => (
                InitialPanel::Panel(2),
                three,
                false,
                "none",
                "on",
                true,
                false,
                folder,
                false,
            ),
            "workspace" | "workspace-agents" | "workspace-docs" | "workspace-code"
            | "workspace-kanban" | "workspace-automate" => (
                InitialPanel::Panel(3),
                three,
                false,
                "none",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "workspace-no-browser-skill" => (
                InitialPanel::Panel(3),
                three,
                false,
                "none",
                "off",
                false,
                false,
                folder,
                false,
            ),
            "mobile" => (
                InitialPanel::Panel(4),
                three,
                false,
                "none",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "get-started" => (
                InitialPanel::Panel(5),
                three,
                false,
                "none",
                "off",
                true,
                false,
                folder,
                false,
            ),
            "get-started-no-folder" => (
                InitialPanel::Panel(5),
                three,
                false,
                "none",
                "off",
                true,
                false,
                None,
                false,
            ),
            "get-started-no-folder-with-projects" => (
                InitialPanel::Panel(5),
                three,
                false,
                "none",
                "off",
                true,
                true,
                None,
                false,
            ),
            "finished" | "finished-guide" => (
                InitialPanel::Finished,
                three,
                true,
                "none",
                "off",
                true,
                false,
                folder,
                false,
            ),
            _ => (
                InitialPanel::Panel(1),
                three,
                false,
                "none",
                "off",
                true,
                false,
                folder,
                false,
            ),
        }
    };
    let mut settings = serde_json::Map::new();
    settings.insert("defaultPromptAgentId".into(), json!("claude"));
    settings.insert("codeViewTabHidden".into(), json!(true));
    settings.insert("kanbanViewTabHidden".into(), json!(true));
    settings.insert("automateViewTabHidden".into(), json!(true));
    if let Some(view) = state
        .strip_prefix("workspace-")
        .filter(|view| *view != "no-browser-skill")
    {
        let key = match view {
            "code" => Some("codeViewTabHidden"),
            "kanban" => Some("kanbanViewTabHidden"),
            "automate" => Some("automateViewTabHidden"),
            _ => None,
        };
        if let Some(key) = key {
            settings.insert(key.into(), json!(false));
        }
    }
    let mut jobs = HashMap::new();
    if cli == "installing" {
        jobs.insert(
            "claude".to_string(),
            json!({ "id": "story-running", "operation": "install", "command": "curl -fsSL https://claude.ai/install.sh | bash", "status": "running", "output": format!("{}\n{}\n", STORY_OUTPUT[0], STORY_OUTPUT[1]) }),
        );
    }
    if cli == "failed" {
        jobs.insert(
            "claude".to_string(),
            json!({ "id": "story-failed", "operation": "install", "command": "npm install -g @anthropic-ai/claude-code@latest", "status": "failed", "output": "npm ERR! code EACCES\nnpm ERR! syscall mkdir\nnpm ERR! path /usr/local/lib/node_modules/@anthropic-ai\n", "error": "Command exited with exit status: 243." }),
        );
    }
    let cli_status = match computer_use {
        "on" => {
            json!({ "type": "ghostexCliStatus", "browserSkillInstalled": browser_skill, "cuaDriverInstalled": true, "computerUseSkillInstalled": true, "cuaDriverAccessibilityPermissionGranted": true, "cuaDriverScreenRecordingPermissionGranted": true })
        }
        "permissions" => {
            json!({ "type": "ghostexCliStatus", "browserSkillInstalled": browser_skill, "cuaDriverInstalled": true, "computerUseSkillInstalled": true, "cuaDriverAccessibilityPermissionGranted": false, "cuaDriverScreenRecordingPermissionGranted": false })
        }
        _ => {
            json!({ "type": "ghostexCliStatus", "browserSkillInstalled": browser_skill, "cuaDriverInstalled": false, "computerUseSkillInstalled": false })
        }
    };
    let host_state = Rc::new(RefCell::new(DemoHost {
        window: None,
        view: None,
        settings: settings.clone(),
        installed: installed.iter().map(|id| id.to_string()).collect(),
        hooks,
        rescan_answers: !loading,
        installed_now: HashSet::new(),
        jobs,
        cli_status,
    }));
    let host_for_commands = host_state.clone();
    let host: OnboardingHost = Rc::new(move |command, cx: &mut App| {
        let host = host_for_commands.clone();
        match command {
            OnboardingCommand::UpdateSettings(patch) => {
                let snapshot = {
                    let mut state = host.borrow_mut();
                    for (key, value) in patch {
                        state.settings.insert(key, value);
                    }
                    state.settings.clone()
                };
                eprintln!("updateSettings");
                cx.defer(move |cx| {
                    let settings = demo_settings(&snapshot);
                    with_view(&host, cx, move |view, cx| {
                        view.receive_settings(settings, cx)
                    });
                });
            }
            OnboardingCommand::RescanAgents => {
                let (answers, first) = {
                    let state = host.borrow();
                    (state.rescan_answers, state.view.is_none())
                };
                let delay = if first {
                    Duration::from_millis(0)
                } else {
                    Duration::from_millis(1400)
                };
                later(cx, delay, move |cx| {
                    // `agents-scanning` keeps the walk open: the result is a partial post.
                    let payload = {
                        let state = host.borrow();
                        let mut installed = state.installed.clone();
                        installed.extend(state.installed_now.iter().cloned());
                        hook_status(&installed, state.hooks, answers)
                    };
                    with_view(&host, cx, move |view, cx| {
                        view.receive_agent_hook_status(payload, cx)
                    });
                });
            }
            OnboardingCommand::RequestCliStatus => {
                later(cx, Duration::from_millis(0), move |cx| {
                    let payload = host.borrow().cli_status.clone();
                    with_view(&host, cx, move |view, cx| {
                        view.receive_cli_status(payload, cx)
                    });
                });
            }
            OnboardingCommand::InstallAgentHooks(agent_ids) => {
                eprintln!("installAgentHooks {agent_ids:?}");
                later(cx, Duration::from_millis(900), move |cx| {
                    let payload = {
                        let mut state = host.borrow_mut();
                        state.hooks = true;
                        let mut installed = state.installed.clone();
                        installed.extend(state.installed_now.iter().cloned());
                        let mut payload = hook_status(&installed, true, true);
                        payload
                            .as_object_mut()
                            .map(|payload| payload.remove("complete"));
                        payload
                    };
                    with_view(&host, cx, move |view, cx| {
                        view.receive_agent_hook_status(payload, cx)
                    });
                });
            }
            OnboardingCommand::InstallComputerUse { install_driver } => {
                eprintln!("installComputerUse driver={install_driver}");
                later(cx, Duration::from_millis(1800), move |cx| {
                    let payload = {
                        let mut state = host.borrow_mut();
                        state.cli_status["cuaDriverInstalled"] = json!(true);
                        state.cli_status["computerUseSkillInstalled"] = json!(true);
                        state.cli_status["cuaDriverAccessibilityPermissionGranted"] = json!(false);
                        state.cli_status["cuaDriverScreenRecordingPermissionGranted"] =
                            json!(false);
                        state.cli_status.clone()
                    };
                    with_view(&host, cx, move |view, cx| {
                        view.receive_cli_status(payload, cx)
                    });
                });
            }
            OnboardingCommand::OpenAccessibilityPreferences
            | OnboardingCommand::OpenScreenRecordingPreferences => {
                let payload = {
                    let mut state = host.borrow_mut();
                    state.cli_status["cuaDriverAccessibilityPermissionGranted"] = json!(true);
                    state.cli_status["cuaDriverScreenRecordingPermissionGranted"] = json!(true);
                    state.cli_status.clone()
                };
                cx.defer(move |cx| {
                    with_view(&host, cx, move |view, cx| {
                        view.receive_cli_status(payload, cx)
                    })
                });
            }
            OnboardingCommand::InstallBrowserSkill | OnboardingCommand::UninstallBrowserSkill => {
                let installed = matches!(command, OnboardingCommand::InstallBrowserSkill);
                let payload = {
                    let mut state = host.borrow_mut();
                    state.cli_status["browserSkillInstalled"] = json!(installed);
                    state.cli_status.clone()
                };
                cx.defer(move |cx| {
                    with_view(&host, cx, move |view, cx| {
                        view.receive_cli_status(payload, cx)
                    })
                });
            }
            OnboardingCommand::OpenExternalUrl(url) => eprintln!("open {url}"),
            OnboardingCommand::IntroVideoSeen => eprintln!("introVideoSeen"),
            OnboardingCommand::PickProjectFolder => {
                cx.defer(move |cx| {
                    with_view(&host, cx, |view, cx| {
                        view.receive_picked_folder("~/Projects/my-app".to_string(), cx)
                    })
                });
            }
            OnboardingCommand::FinishFirstLaunch {
                request_id,
                agent_id,
                path,
            } => {
                eprintln!("firstLaunchCreateProjectSession {agent_id} {path}");
                later(cx, Duration::from_millis(900), move |cx| {
                    with_view(&host, cx, move |view, cx| {
                        view.receive_finish_result(&request_id, true, None, cx)
                    });
                });
            }
            OnboardingCommand::Finish(target) => {
                eprintln!("finish -> {target:?}");
                cx.quit();
            }
            OnboardingCommand::AgentCli {
                request_id,
                agent_id,
                request,
            } => {
                let result = {
                    let mut state = host.borrow_mut();
                    match &request {
                        AgentCliRequest::Start { method_id } => {
                            if state.jobs.values().any(|job| {
                                job.get("status").and_then(Value::as_str) == Some("running")
                            }) {
                                Err("Another CLI install or update is still running. Wait for it to finish.".to_string())
                            } else {
                                let command = story_methods(&agent_id)
                                    .into_iter()
                                    .find(|method| {
                                        method.get("id").and_then(Value::as_str)
                                            == Some(method_id.as_str())
                                    })
                                    .and_then(|method| {
                                        method
                                            .get("command")
                                            .and_then(Value::as_str)
                                            .map(str::to_string)
                                    })
                                    .unwrap_or_default();
                                let job_id = format!("story-{agent_id}-{request_id}");
                                state.jobs.insert(
                                    agent_id.clone(),
                                    json!({ "id": job_id, "operation": "install", "command": command, "status": "running", "output": "" }),
                                );
                                for (index, line) in STORY_OUTPUT.iter().enumerate() {
                                    let host = host.clone();
                                    let agent_id = agent_id.clone();
                                    let job_id = job_id.clone();
                                    later(
                                        cx,
                                        Duration::from_millis(1100 * (index as u64 + 1)),
                                        move |_| {
                                            let mut state = host.borrow_mut();
                                            let last = index == STORY_OUTPUT.len() - 1;
                                            if last {
                                                state.installed_now.insert(agent_id.clone());
                                            }
                                            if let Some(job) = state.jobs.get_mut(&agent_id)
                                                && job.get("id").and_then(Value::as_str)
                                                    == Some(job_id.as_str())
                                            {
                                                let output = format!(
                                                    "{}{line}\n",
                                                    job.get("output")
                                                        .and_then(Value::as_str)
                                                        .unwrap_or_default()
                                                );
                                                job["output"] = json!(output);
                                                job["status"] = json!(if last {
                                                    "succeeded"
                                                } else {
                                                    "running"
                                                });
                                            }
                                        },
                                    );
                                }
                                Ok(cli_state(&state, &agent_id))
                            }
                        }
                        _ => Ok(cli_state(&state, &agent_id)),
                    }
                };
                later(cx, Duration::from_millis(120), move |cx| {
                    with_view(&host, cx, move |view, cx| {
                        view.receive_agent_cli_result(request_id, result, cx)
                    });
                });
            }
        }
    });
    let config = OnboardingConfig {
        first_run: false,
        has_projects,
        settings: demo_settings(&settings),
        catalog: catalog(),
        theme: theme_table(),
        system_light: std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_THEME")
            .is_ok_and(|theme| theme.trim() == "light"),
        cli_available: cli != "none",
        initial_panel,
        picked_folder: picked.map(str::to_string),
        intro_video: state.starts_with("intro-video"),
    };
    let host_for_window = host_state.clone();
    // `GHOSTEX_ONBOARDING_DEMO_SIZE=WxH` opens another window size, to check the stage scaling.
    let (width, height) = std::env::var("GHOSTEX_ONBOARDING_DEMO_SIZE")
        .ok()
        .and_then(|size| {
            let (width, height) = size.split_once('x')?;
            Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
        })
        .unwrap_or((ONBOARDING_MODAL_WIDTH, ONBOARDING_MODAL_HEIGHT));
    let (window, view) = super::open_modal_window(
        width,
        height,
        move |window, cx| cx.new(|cx| GpuiOnboardingWindow::new(config, host, window, cx)),
        cx,
    );
    {
        let mut state = host_for_window.borrow_mut();
        state.window = Some(window);
        state.view = Some(view);
    }
    // States that start partway through an interaction.
    let host = host_for_window;
    match state {
        "computer-use-installing" => with_view(&host, cx, |view, cx| {
            view.preview_computer_use_installing(cx)
        }),
        "agents-guide" => with_view(&host, cx, |view, cx| view.preview_open_install_guide(cx)),
        "agents-integration" => with_view(&host, cx, |view, cx| view.preview_right_tab(1, cx)),
        "computer-use-permissions" | "computer-use-on-tab" => {
            with_view(&host, cx, |view, cx| view.preview_right_tab(2, cx))
        }
        "welcome-chat" => with_view(&host, cx, |view, cx| view.preview_welcome_tab(1, cx)),
        "welcome-mobile" => with_view(&host, cx, |view, cx| view.preview_welcome_tab(2, cx)),
        "workspace-agents" => with_view(&host, cx, |view, cx| view.preview_workspace_tab(None, cx)),
        "workspace-docs" => with_view(&host, cx, |view, cx| {
            view.preview_workspace_tab(Some(1), cx)
        }),
        "workspace-code" => with_view(&host, cx, |view, cx| {
            view.preview_workspace_tab(Some(2), cx)
        }),
        "workspace-kanban" => with_view(&host, cx, |view, cx| {
            view.preview_workspace_tab(Some(3), cx)
        }),
        "workspace-automate" => with_view(&host, cx, |view, cx| {
            view.preview_workspace_tab(Some(4), cx)
        }),
        "finished-guide" => with_view(&host, cx, |view, cx| view.preview_finished_guide(cx)),
        "intro-video-offline" => {
            with_view(&host, cx, |view, cx| view.preview_intro_video_offline(cx))
        }
        "intro-video-no-web-view" => with_view(&host, cx, |view, cx| {
            view.preview_intro_video_no_web_view(cx)
        }),
        "intro-video-continue" => later(cx, Duration::from_secs(6), move |cx| {
            with_view(&host, cx, |view, cx| view.leave_intro_video(cx))
        }),
        _ => {}
    }
}

fn demo_settings(object: &serde_json::Map<String, Value>) -> OnboardingSettings {
    OnboardingSettings::from_object(object)
}
