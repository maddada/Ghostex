//! Preview host for the native Add Worktree dialog.
//! States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): the default answers the worktree
//! request after one second with branches and worktrees; `loading` never
//! answers; `empty` answers with no branches and no worktrees; `error` answers
//! with a failure (`errorexisting` and `emptyexisting` show those in Open
//! Existing mode); `noagents` opens without any command agent. After the
//! answer, `openexisting` switches the mode, `prompt` types a first prompt,
//! `images` types a prompt and inserts two picked image links, `branchmenu`,
//! `branchfilter`, `branchempty`, `existingmenu`, `existingfilter` and
//! `agentmenu` open a picker (with a typed filter where named). The `key*`
//! states drive the keyboard contract through `Window::dispatch_keystroke` and
//! log the outcome: `keyenter` submits a typed prompt, `keyshiftenter` inserts
//! a newline before submitting, `keyescape` cancels, and `keypick` filters the
//! branch picker, moves down once and chooses with Enter before submitting.
use super::create_worktree_modal::*;
use gpui::{AnyWindowHandle, App, AppContext as _, Entity, Keystroke, WindowHandle};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const FAKE_IMAGES: [&str; 2] = [
    "/Users/you/Desktop/Screenshot 2026-09-15 at 09.41.12.png",
    "/Users/you/Desktop/sidebar-mockup.png",
];

fn fake_branches() -> Vec<WorktreeBaseBranchOption> {
    [
        ("main", true, false),
        ("feature/dropdowns", false, false),
        ("release/desktop", false, false),
        ("origin/main", false, true),
        ("fix/search", false, false),
        ("feature/accounts", false, false),
        ("docs/setup", false, false),
        ("release/mobile", false, false),
        ("feature/board", false, false),
    ]
    .into_iter()
    .map(|(name, current, remote)| WorktreeBaseBranchOption {
        name: name.to_string(),
        current,
        remote,
    })
    .collect()
}

fn fake_worktrees() -> Vec<ExistingWorktreeOption> {
    [
        ("Ghostex", "main", "/demo/Ghostex", true, true),
        (
            "UI work",
            "feature/dropdowns",
            "/demo/Ghostex-ui",
            false,
            false,
        ),
        (
            "Search fixes",
            "fix/search",
            "/demo/Ghostex-search",
            false,
            false,
        ),
    ]
    .into_iter()
    .map(
        |(name, branch, path, is_registered, is_current_project)| ExistingWorktreeOption {
            name: name.to_string(),
            path: path.to_string(),
            branch: branch.to_string(),
            is_registered,
            is_current_project,
            worktree_key: None,
        },
    )
    .collect()
}

type Slot = Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiCreateWorktreeModalWindow>)>>>;

fn apply_preview_state(slot: &Slot, state: &str, cx: &mut App) {
    let target = slot.borrow().clone();
    let Some((window, view)) = target else {
        return;
    };
    let _ = window.update(cx, |_root, window, cx| {
        view.update(cx, |modal, cx| match state {
            "openexisting" | "existingmenu" | "existingfilter" | "errorexisting"
            | "emptyexisting" => {
                modal.preview_set_mode(CreateWorktreeMode::OpenExisting, window, cx);
                match state {
                    "existingmenu" => {
                        modal.preview_open_picker(CreateWorktreePicker::Existing, "", window, cx)
                    }
                    "existingfilter" => {
                        modal.preview_open_picker(CreateWorktreePicker::Existing, "ui", window, cx)
                    }
                    _ => {}
                }
            }
            "prompt" => modal.preview_set_prompt("Fix the flaky sidebar test", window, cx),
            "images" => {
                modal.preview_set_prompt("Fix the flaky sidebar test", window, cx);
                modal.receive_image_files_picked(
                    FAKE_IMAGES.iter().map(|path| path.to_string()).collect(),
                    window,
                    cx,
                );
            }
            "branchmenu" => modal.preview_open_picker(CreateWorktreePicker::Branch, "", window, cx),
            "branchfilter" => {
                modal.preview_open_picker(CreateWorktreePicker::Branch, "rel", window, cx)
            }
            "branchempty" => {
                modal.preview_open_picker(CreateWorktreePicker::Branch, "zzz", window, cx)
            }
            "agentmenu" => modal.preview_open_picker(CreateWorktreePicker::Agent, "", window, cx),
            "keyenter" | "keypick" => {
                modal.preview_set_prompt("Fix the flaky sidebar test", window, cx);
                if state == "keypick" {
                    modal.preview_open_picker(CreateWorktreePicker::Branch, "fea", window, cx);
                }
            }
            "keyshiftenter" => modal.preview_set_prompt("a", window, cx),
            _ => {}
        });
    });
    match state {
        "keyenter" => dispatch_keys(slot, &["enter"], cx),
        "keyshiftenter" => dispatch_keys(slot, &["shift-enter", "b", "enter"], cx),
        "keyescape" => dispatch_keys(slot, &["escape"], cx),
        "keypick" => dispatch_keys(slot, &["down", "enter", "enter"], cx),
        _ => {}
    }
}

/// Dispatches keystrokes into the demo window 250ms apart, the way the platform would.
fn dispatch_keys(slot: &Slot, keys: &'static [&'static str], cx: &mut App) {
    let slot = slot.clone();
    cx.spawn(async move |cx| {
        for key in keys {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let target = slot.borrow().clone();
            let Some((window, _view)) = target else {
                return;
            };
            // Through the untyped handle: the typed one leases `Root`, which the
            // dispatched event re-enters while it re-renders the window.
            let any_window: AnyWindowHandle = window.into();
            cx.update(|cx| {
                let _ = any_window.update(cx, |_root, window, cx| {
                    let keystroke = Keystroke::parse(key).expect("a valid demo keystroke");
                    window.dispatch_keystroke(keystroke, cx);
                });
            });
        }
    })
    .detach();
}

fn deliver_worktrees(slot: &Slot, request_id: &str, state: &str, cx: &mut App) {
    let target = slot.borrow().clone();
    let Some((window, view)) = target else {
        return;
    };
    let _ = window.update(cx, |_root, _window, cx| {
        view.update(cx, |modal, cx| match state {
            "error" | "errorexisting" => modal.receive_project_worktrees_result(
                request_id,
                false,
                Some(
                    "fatal: not a git repository (or any of the parent directories): .git"
                        .to_string(),
                ),
                Vec::new(),
                Vec::new(),
                cx,
            ),
            "empty" | "emptyexisting" => modal.receive_project_worktrees_result(
                request_id,
                true,
                None,
                Vec::new(),
                Vec::new(),
                cx,
            ),
            _ => modal.receive_project_worktrees_result(
                request_id,
                true,
                None,
                fake_branches(),
                fake_worktrees(),
                cx,
            ),
        });
    });
}

fn deliver_images(slot: &Slot, cx: &mut App) {
    let target = slot.borrow().clone();
    let Some((window, view)) = target else {
        return;
    };
    let _ = window.update(cx, |_root, window, cx| {
        view.update(cx, |modal, cx| {
            modal.receive_image_files_picked(
                FAKE_IMAGES.iter().map(|path| path.to_string()).collect(),
                window,
                cx,
            );
        });
    });
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let agents = if demo.state == "noagents" {
        Vec::new()
    } else {
        [
            ("claude", "Claude Code"),
            ("codex", "Codex"),
            ("copilot", "Copilot"),
        ]
        .into_iter()
        .map(|(agent_id, name)| CreateWorktreeAgent {
            agent_id: agent_id.to_string(),
            name: name.to_string(),
        })
        .collect()
    };
    let slot: Slot = Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let state = demo.state.clone();
    let host: CreateWorktreeModalHost = Rc::new(move |command, cx: &mut App| match command {
        CreateWorktreeModalCommand::RequestWorktrees { request_id } => {
            eprintln!("request worktrees {request_id}");
            if state == "loading" {
                return;
            }
            let slot = host_slot.clone();
            let state = state.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                cx.update(|cx| {
                    deliver_worktrees(&slot, &request_id, &state, cx);
                    apply_preview_state(&slot, &state, cx);
                });
            })
            .detach();
        }
        CreateWorktreeModalCommand::PickImages => {
            eprintln!("pick images");
            let slot = host_slot.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                cx.update(|cx| deliver_images(&slot, cx));
            })
            .detach();
        }
        CreateWorktreeModalCommand::Create(draft) => {
            eprintln!("create worktree: {draft:?}");
            cx.quit();
        }
        CreateWorktreeModalCommand::Cancel => {
            eprintln!("cancel");
            cx.quit();
        }
    });
    let config = CreateWorktreeModalConfig {
        agents,
        default_agent_id: Some("claude".to_string()),
        palette: demo.palette,
    };
    let (window, view) = super::open_modal_window(
        CREATE_WORKTREE_MODAL_WIDTH,
        CREATE_WORKTREE_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiCreateWorktreeModalWindow::new(config, host, window, cx)),
        cx,
    );
    *slot.borrow_mut() = Some((window, view));
}
