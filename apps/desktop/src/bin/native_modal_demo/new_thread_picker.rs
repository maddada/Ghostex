//! New Thread picker preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`):
//! default (nine agents, two Codex and one Claude account), `accounts` (the
//! Codex account list opens after a moment), `noaccounts` (accounts read but
//! none for Codex: Current CLI login and Add account), `error` (the accounts
//! list could not be read: Try again), `loading` (agents not read yet), `boxes`
//! (the Run on row with Docker, Hetzner, an SSH host and Daytona ready, Docker
//! picked), `boxes-ssh` (the same, the SSH host picked: the chips scroll to it),
//! `noboxes` (agentbox installed, no box ready: the Settings link), `many`
//! (sixteen agents, so the list scrolls). Every other state has no Run on row.
use super::new_thread_picker::*;
use gpui::{App, AppContext as _, Bounds, Entity, WindowHandle, point, px, size};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

fn agent(light: bool, agent_id: &str, name: &str, icon: Option<&str>) -> NewThreadPickerAgent {
    let (icon_path, icon_svg_size, icon_accent) = match icon {
        Some("codex") => (Some("agent-icons/codex.svg"), 10.5, 0xffffff),
        Some("claude") => (Some("agent-icons/claude.svg"), 11.0, 0xd97757),
        Some("claude-cloud") => (Some("agent-icons/claude-cloud.svg"), 13.0, 0xd67556),
        Some("cursor-cli") => (Some("agent-icons/cursor-cli.svg"), 11.5, 0xedecec),
        Some("grok-build") => (Some("agent-icons/grok-build.svg"), 11.5, 0xffffff),
        Some("hermes-agent") => (Some("agent-icons/hermes-agent.svg"), 11.5, 0xf3c46b),
        Some("pi") => (Some("agent-icons/pi.svg"), 10.5, 0xc8ff62),
        Some("omp") => (Some("agent-icons/omp.svg"), 11.5, 0xc8ff62),
        Some("antigravity-cli") => (Some("agent-icons/antigravity-cli.svg"), 11.5, 0x749bff),
        Some("zcode") => (
            Some("agent-icons/zcode.svg"),
            11.5,
            if light { 0x000000 } else { 0xffffff },
        ),
        _ => (None, 12.0, 0xffffff),
    };
    NewThreadPickerAgent {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        icon: icon.map(str::to_string),
        icon_path,
        icon_svg_size,
        icon_accent,
        uses_own_login: false,
    }
}

fn account(
    id: &str,
    provider: &str,
    name: &str,
    usage: &str,
    ready: bool,
    is_default: bool,
) -> NewThreadPickerAccount {
    NewThreadPickerAccount {
        id: id.to_string(),
        provider: provider.to_string(),
        name: name.to_string(),
        usage: Some(usage.to_string()),
        ready,
        is_default,
    }
}

fn accounts(state: &str) -> Option<Vec<NewThreadPickerAccount>> {
    match state {
        "error" => None,
        "noaccounts" => Some(vec![account(
            "claude-1",
            "claude",
            "m•••a@•••••.•••",
            "7d: 31% · 5h: 12%",
            true,
            true,
        )]),
        _ => Some(vec![
            account(
                "codex-1",
                "codex",
                "m•••a@•••••.•••",
                "7d: 42% · 3rs",
                true,
                true,
            ),
            account(
                "codex-2",
                "codex",
                "work@example.com",
                "7d: 88% · 0rs",
                false,
                false,
            ),
            account(
                "claude-1",
                "claude",
                "m•••a@•••••.•••",
                "7d: 31% · 5h: 12%",
                true,
                true,
            ),
        ]),
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.clone();
    let light = demo.palette.light;
    let agents = if state == "loading" {
        Vec::new()
    } else {
        vec![
            agent(light, "codex", "Codex", Some("codex")),
            agent(light, "claude", "Claude", Some("claude")),
            agent(
                light,
                "claude-cloud",
                "Claude (cloud)",
                Some("claude-cloud"),
            ),
            agent(light, "cursor", "Cursor CLI", Some("cursor-cli")),
            agent(light, "grok", "Grok Build", Some("grok-build")),
            agent(light, "hermes", "Hermes Agent", Some("hermes-agent")),
            agent(light, "pi", "Pi Agent", Some("pi")),
            agent(light, "omp", "OMP", Some("omp")),
            agent(
                light,
                "antigravity",
                "Antigravity CLI",
                Some("antigravity-cli"),
            ),
            agent(light, "zcode", "ZCode", Some("zcode")),
        ]
    };
    let agents = if state == "many" {
        let mut agents = agents;
        for name in ["Gemini", "Copilot", "Kiro", "Qoder", "Amp", "Droid"] {
            agents.push(agent(light, &name.to_lowercase(), name, None));
        }
        agents
    } else {
        agents
    };
    let slot: Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiNewThreadPickerWindow>)>>> =
        Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let host: NewThreadPickerHost = Rc::new(move |command, cx: &mut App| match command {
        NewThreadPickerCommand::LaunchAgent {
            agent_id,
            account_id,
            run_location,
            target,
        } => {
            eprintln!(
                "launch {agent_id} with account {account_id:?} in {run_location:?} at {target:?}"
            );
            cx.quit();
        }
        NewThreadPickerCommand::OpenCloudBoxesSettings => {
            eprintln!("open Settings > Cloud Boxes");
            cx.quit();
        }
        NewThreadPickerCommand::NewCoordinator { .. } => {
            eprintln!("new coordinator");
            cx.quit();
        }
        NewThreadPickerCommand::OpenBrowser { .. } => {
            eprintln!("open browser");
            cx.quit();
        }
        NewThreadPickerCommand::CreateTerminal { .. } => {
            eprintln!("create terminal");
            cx.quit();
        }
        NewThreadPickerCommand::AddAccount => {
            eprintln!("add account");
            cx.quit();
        }
        NewThreadPickerCommand::RetryAccounts => {
            eprintln!("retry accounts");
            let slot = host_slot.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let target = slot.borrow().clone();
                if let Some((window, view)) = target {
                    cx.update(|cx| {
                        let _ = window.update(cx, |_root, _window, cx| {
                            view.update(cx, |picker, cx| {
                                picker.set_accounts(Ok(accounts("").unwrap_or_default()), cx);
                            });
                        });
                    });
                }
            })
            .detach();
        }
        NewThreadPickerCommand::Closed => {
            eprintln!("closed");
            cx.quit();
        }
    });
    let location = |run_location: &str, label: &str, kind: &str| NewThreadPickerLocation {
        run_location: run_location.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        tooltip: run_location
            .strip_prefix("agentbox:docker:")
            .map(|alias| format!("Your server {alias} over SSH")),
    };
    let boxes = match state.as_str() {
        "boxes" | "boxes-ssh" => NewThreadPickerBoxes::Ready(vec![
            location("agentbox:docker", "Docker", "local"),
            location("agentbox:hetzner", "Hetzner", "cloud"),
            location("agentbox:docker:selfhost", "selfhost", "remoteDocker"),
            location("agentbox:daytona", "Daytona", "cloud"),
        ]),
        "noboxes" => NewThreadPickerBoxes::NotSetUp,
        _ => NewThreadPickerBoxes::Unknown,
    };
    // The Cloud Box chip shows with the boxes states (Cloud Boxes on).
    let cloud_box = state.starts_with("boxes");
    // `places` and `projects` draw the project row and machine chips (`projects` opens the list).
    let place = state.starts_with("places") || state.starts_with("projects");
    let run_on_shown = !place && (boxes.shows_run_on() || cloud_box);
    let config = NewThreadPickerConfig {
        place: place.then(|| demo_place(state == "projects-long")),
        palette: demo.palette,
        agents: agents.clone(),
        agents_loaded: state != "loading",
        accounts: accounts(&state),
        boxes,
        default_run_location: match state.as_str() {
            "boxes" => "agentbox:docker".to_string(),
            "boxes-ssh" => "agentbox:docker:selfhost".to_string(),
            _ => "local".to_string(),
        },
        close_when_inactive: false,
        cloud_default_account: None,
        cloud_box,
        // GHOSTEX_NATIVE_MODAL_DEMO_ROOM_HEIGHT=<points>: the height of the app window the picker
        // opens over, so a short window can be previewed (its height cap).
        room: std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_ROOM_HEIGHT")
            .ok()
            .and_then(|height| height.trim().parse::<f32>().ok())
            .map(|height| Bounds::new(point(px(0.0), px(0.0)), size(px(1200.0), px(height)))),
    };
    let (window, view) = super::open_modal_window(
        NEW_THREAD_PICKER_WIDTH,
        new_thread_picker_capped_height(
            new_thread_picker_window_height(agents.len(), place, run_on_shown),
            config.room,
            None,
        ),
        move |window, cx| cx.new(|cx| GpuiNewThreadPickerWindow::new(config, host, window, cx)),
        cx,
    );
    *slot.borrow_mut() = Some((window, view.clone()));
    if state.starts_with("projects") {
        let _ = window.update(cx, |_root, window, cx| {
            view.update(cx, |picker, cx| {
                picker.toggle_project_list(window, cx);
            })
        });
    }
    if state == "boxes-ssh" {
        // A background preview draws one frame; a second one shows the chips scrolled to the pick.
        let view = view.clone();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            cx.update(|cx| {
                let _ = window.update(cx, |_root, _window, cx| {
                    view.update(cx, |_, cx| cx.notify())
                });
            });
        })
        .detach();
    }
    if state == "accounts" || state == "noaccounts" || state == "error" {
        let error = state == "error";
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            cx.update(|cx| {
                let _ = window.update(cx, |_root, window, cx| {
                    view.update(cx, |picker, cx| {
                        if error {
                            picker.set_accounts(
                                Err("Could not reach gxserver on this computer.".to_string()),
                                cx,
                            );
                        }
                        picker.preview_enter_accounts(0, window, cx);
                    });
                });
            });
        })
        .detach();
    }
}

/// Three projects on three machines, one machine not connected, for the `places` states.
fn demo_place(long: bool) -> NewThreadPickerPlace {
    let machine = |id: &str, label: &str, connected: bool, project: &str| NewThreadPickerMachine {
        machine_id: id.to_string(),
        label: label.to_string(),
        connected,
        project_id: project.to_string(),
    };
    let project =
        |label: &str, recent: bool, has_sessions: bool, machines| NewThreadPickerProject {
            repo_key: format!("github.com/o/{label}"),
            label: label.to_string(),
            has_sessions,
            recent,
            machines,
            start_machine: 0,
            detail: None,
        };
    let mut projects = vec![
        project(
            "project-b",
            true,
            true,
            vec![machine("serverb", "Server B", true, "b")],
        ),
        project(
            "project-a",
            true,
            false,
            vec![
                machine("local", "MacBook", true, "a1"),
                machine("servera", "Server A", true, "a2"),
                machine("serverc", "Server C", false, "a3"),
            ],
        ),
        project(
            "dotfiles",
            false,
            true,
            vec![machine("local", "MacBook", true, "d")],
        ),
        project(
            "site",
            false,
            false,
            vec![machine("local", "MacBook", true, "s")],
        ),
    ];
    // A long list: more projects than any window shows, most on three computers.
    if long {
        projects.extend((1..=24).map(|n| {
            project(
                &format!("project-{n:02}"),
                false,
                n % 3 == 0,
                vec![
                    machine("local", "Local", true, "l"),
                    machine("windows", "Windows", true, "w"),
                    machine("cloud", "Cloud Box", true, "c"),
                ],
            )
        }));
    }
    NewThreadPickerPlace {
        projects,
        project: 1,
        machine: 1,
        hotkey: Some("⌘N".to_string()),
    }
}
