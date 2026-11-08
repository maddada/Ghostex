/*
CDXC:AppModal 2026-09-15 WHY:
Standalone preview of the native GPUI app modals so their look can be checked
against the React Storybook stories without launching Ghostex. Run with:
    cargo run --release --bin native-modal-demo
Environment:
    GHOSTEX_NATIVE_MODAL_DEMO=export-transcript|updates-menu|start-menu|chief-row|notice-modal|... (which modal; default export-transcript)
    GHOSTEX_NATIVE_MODAL_DEMO_THEME=dark|light                (default dark)
    GHOSTEX_NATIVE_MODAL_DEMO_SIDEBAR_THEME=dark-green|...    (a dark theme's text colours)
    GHOSTEX_NATIVE_MODAL_DEMO_CHROME=<rrggbb>                 (tint the modal from a theme chrome)
    GHOSTEX_NATIVE_MODAL_DEMO_GLASS=1                         (the frosted dialog palette, as under window glass)
    GHOSTEX_NATIVE_MODAL_DEMO_STATE=<per-modal state name>    (default: the opening state)
    GHOSTEX_NATIVE_MODAL_DEMO_BACKGROUND=1                    (open without taking focus, for screenshots)
    GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT=<png>                  (any preview, built with
        --features modal-demo-snapshot: draw offscreen to <png> and quit;
        GHOSTEX_NATIVE_MODAL_DEMO_HOVER=<x>,<y> points at that spot first, for a tooltip)
    GHOSTEX_EXPORT_MODAL_DEMO_MODE=handoff|export             (Handoff / Export only)
    GHOSTEX_EXPORT_MODAL_DEMO_PREFS=<file>                    (Handoff / Export's saved choices)
Hosts simulate the daemon: a primary action answers after one second with a
fake success, or with a failure message when the state is `failed`. Closing
actions quit the demo.
*/
#![allow(dead_code)]
#[path = "../assets.rs"]
mod assets;
#[path = "../app/window/export_transcript_modal.rs"]
mod export_transcript_modal;
#[path = "../fold_motion.rs"]
mod fold_motion;
#[path = "../host_name.rs"]
mod host_name;
#[path = "../hotkey_label.rs"]
mod hotkey_label;
#[path = "../app/window/native_modal_kit/mod.rs"]
mod native_modal_kit;
#[path = "../app/window/session_handoff_modal.rs"]
mod session_handoff_modal;
#[path = "../ui_fonts.rs"]
mod ui_fonts;
// DEMO-MODULES: one `#[path]` include per converted modal, plus its demo module under native_modal_demo/.
#[path = "native_modal_demo/add_project.rs"]
mod add_project_demo;
#[path = "../app/window/add_project_modal/mod.rs"]
mod add_project_modal;
#[path = "native_modal_demo/agent_hooks_required.rs"]
mod agent_hooks_required_demo;
#[path = "../app/window/agent_hooks_required_modal.rs"]
mod agent_hooks_required_modal;
#[path = "native_modal_demo/agents_hub.rs"]
mod agents_hub_demo;
#[path = "../app/window/agents_hub/mod.rs"]
mod agents_hub_modal;
#[path = "native_modal_demo/chief_row.rs"]
mod chief_row_demo;
#[path = "../app/native_sidebar/chief_row_view.rs"]
mod chief_row_view;
#[path = "native_modal_demo/close_buttons.rs"]
mod close_buttons_demo;
#[path = "native_modal_demo/coordinator_details.rs"]
mod coordinator_details_demo;
#[path = "../app/window/coordinator_details_modal.rs"]
mod coordinator_details_modal;
#[path = "native_modal_demo/create_worktree.rs"]
mod create_worktree_demo;
#[path = "../app/window/create_worktree_modal.rs"]
mod create_worktree_modal;
#[path = "native_modal_demo/delayed_send.rs"]
mod delayed_send_demo;
#[path = "../app/window/delayed_send_modal.rs"]
mod delayed_send_modal;
#[path = "native_modal_demo/delete_worktree.rs"]
mod delete_worktree_demo;
#[path = "../app/window/delete_worktree_modal.rs"]
mod delete_worktree_modal;
#[path = "native_modal_demo/extension_menu.rs"]
mod extension_menu_demo;
#[path = "native_modal_demo/missing_project_folder.rs"]
mod missing_project_folder_demo;
#[path = "../app/window/missing_project_folder_modal.rs"]
mod missing_project_folder_modal;
#[path = "native_modal_demo/new_coordinator.rs"]
mod new_coordinator_demo;
#[path = "../app/window/new_coordinator_modal.rs"]
mod new_coordinator_modal;
#[path = "../app/window/new_thread_picker.rs"]
mod new_thread_picker;
#[path = "native_modal_demo/new_thread_picker.rs"]
mod new_thread_picker_demo;
#[path = "../app/window/notice_modal.rs"]
mod notice_modal;
#[path = "native_modal_demo/notice_modal.rs"]
mod notice_modal_demo;
#[path = "../app/window/onboarding/mod.rs"]
mod onboarding;
#[path = "native_modal_demo/onboarding.rs"]
mod onboarding_demo;
#[path = "native_modal_demo/permission_guide.rs"]
mod permission_guide_demo;
#[path = "../app/os_integration/permission_guide/tag.rs"]
mod permission_guide_tag;
#[path = "../app/window/popup_dismissal.rs"]
mod popup_dismissal;
#[path = "native_modal_demo/remote_gxserver_install.rs"]
mod remote_gxserver_install_demo;
#[path = "../app/window/remote_gxserver_install_modal.rs"]
mod remote_gxserver_install_modal;
#[path = "native_modal_demo/remote_setup.rs"]
mod remote_setup_demo;
#[path = "../app/window/remote_setup_modal.rs"]
mod remote_setup_modal;
#[path = "native_modal_demo/rename_session.rs"]
mod rename_session_demo;
#[path = "../app/window/rename_session_modal.rs"]
mod rename_session_modal;
#[path = "native_modal_demo/rename_worktree.rs"]
mod rename_worktree_demo;
#[path = "../app/window/rename_worktree_modal.rs"]
mod rename_worktree_modal;
#[path = "../app/window/round_close_button.rs"]
mod round_close_button;
#[path = "native_modal_demo/security_upgrade.rs"]
mod security_upgrade_demo;
#[path = "../app/window/security_upgrade_modal.rs"]
mod security_upgrade_modal;
#[path = "native_modal_demo/session_note.rs"]
mod session_note_demo;
#[path = "../app/window/session_note_modal.rs"]
mod session_note_modal;
#[path = "native_modal_demo/settings.rs"]
mod settings_demo;
#[path = "../app/window/settings_modal/mod.rs"]
mod settings_modal;
// The chat header's Share popover (Ghostex Collab), drawn from the app's own view.
#[path = "native_modal_demo/share_popover.rs"]
mod share_popover_demo;
#[path = "../app/window/share_popover_view.rs"]
mod share_popover_view;
#[path = "native_modal_demo/space_editor.rs"]
mod space_editor_demo;
#[path = "../app/window/space_editor_modal.rs"]
mod space_editor_modal;
#[path = "native_modal_demo/start_menu_click.rs"]
mod start_menu_click_demo;
#[path = "native_modal_demo/start_menu.rs"]
mod start_menu_demo;
#[path = "native_modal_demo/update_available.rs"]
mod update_available_demo;
// The top bar's Updates button and list: `app` holds the stand-ins its rows file reads.
#[path = "native_modal_demo/updates_menu_app.rs"]
mod app;
#[path = "../app/window/update_available_modal.rs"]
mod update_available_modal;
#[path = "native_modal_demo/updates_menu.rs"]
mod updates_menu_demo;
/// The voice panel's confirm card reads `super::controls` and `super::rows`, as in the panel.
#[path = "../app/voice_agent/panel_confirm_card.rs"]
mod voice_confirm_card;
#[path = "../app/voice_agent/panel_controls.rs"]
mod voice_panel_controls;
use voice_panel_controls as controls;
#[allow(dead_code)]
#[path = "../app/voice_agent/rows.rs"]
mod rows;
#[path = "native_modal_demo/voice_panel.rs"]
mod voice_panel_demo;
/// The panel's size limits, the voice list's scroll and the Scroll to bottom pill, as in the panel.
#[allow(dead_code)]
#[path = "../app/voice_agent/panel_layout.rs"]
mod voice_panel_layout;
#[path = "native_modal_demo/web_runtime_prompt.rs"]
mod web_runtime_prompt_demo;
#[path = "../app/window/web_runtime_prompt_modal.rs"]
mod web_runtime_prompt_modal;

use export_transcript_modal::*;
use gpui::{
    App, AppContext as _, Bounds, Entity, Render, Styled as _, WindowBounds, WindowHandle,
    WindowOptions, point, px, size,
};
use gpui_component::Root;
use native_modal_kit::ModalPalette;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const FAKE_PATH: &str = "/Users/you/Library/Application Support/ghostex/exports/fix-hookless-agent-modal-g04t1-20260915-081928.md";

fn env(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

struct DemoEnv {
    modal: String,
    state: String,
    palette: ModalPalette,
}

impl DemoEnv {
    fn read() -> Self {
        let modal = env("GHOSTEX_NATIVE_MODAL_DEMO");
        Self {
            modal: if modal.is_empty() {
                "export-transcript".to_string()
            } else {
                modal
            },
            state: env("GHOSTEX_NATIVE_MODAL_DEMO_STATE"),
            palette: demo_palette(),
        }
    }
}

/// The demo's palette: `GHOSTEX_NATIVE_MODAL_DEMO_THEME`, plus
/// `GHOSTEX_NATIVE_MODAL_DEMO_SIDEBAR_THEME=dark-green|dark-blue|...` (a dark theme's text colours)
/// and `GHOSTEX_NATIVE_MODAL_DEMO_CHROME=<rrggbb>` (the theme chrome the app tints modals from), so
/// the theme colours of Settings > Interface can be previewed.
fn demo_palette() -> ModalPalette {
    let sidebar_theme = env("GHOSTEX_NATIVE_MODAL_DEMO_SIDEBAR_THEME");
    let palette = ModalPalette::resolve(
        env("GHOSTEX_NATIVE_MODAL_DEMO_THEME") == "light",
        (!sidebar_theme.is_empty()).then_some(sidebar_theme.as_str()),
    );
    let palette = match u32::from_str_radix(
        env("GHOSTEX_NATIVE_MODAL_DEMO_CHROME").trim_start_matches('#'),
        16,
    ) {
        Ok(chrome) => palette.tinted(gpui::rgb(chrome)),
        Err(_) => palette,
    };
    // The form dialogs' palette under window glass: the surface at the macOS frosted alpha (75%
    // light, 60% dark, helpers/window_glass.rs), over whatever the snapshot is laid on.
    if env("GHOSTEX_NATIVE_MODAL_DEMO_GLASS") == "1" {
        let alpha = if palette.light { 0.75 } else { 0.6 };
        return palette.frosted_dialog(native_modal_kit::rgba_of(palette.surface, alpha));
    }
    palette
}

/// Opens one modal window the way the app does: centered, borderless, at the
/// modal's first-frame size, with a gpui-component `Root` as the window root.
fn open_modal_window<V: Render>(
    width: f32,
    initial_height: f32,
    build: impl FnOnce(&mut gpui::Window, &mut App) -> Entity<V>,
    cx: &mut App,
) -> (WindowHandle<Root>, Entity<V>) {
    let window_size = size(px(width), px(initial_height));
    // GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT=<png> (with --features modal-demo-snapshot) draws any
    // preview in a window far off every screen, writes its frame after
    // GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT_WAIT ms (2500) and quits, as updates_menu.rs does for its own.
    let snapshot = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT").unwrap_or_default();
    let snapshot = snapshot.trim().to_string();
    let bounds = if !snapshot.is_empty() {
        Bounds::new(point(px(-10000.0), px(-10000.0)), window_size)
    } else {
        cx.primary_display()
            .map(|display| Bounds::centered_at(display.bounds().center(), window_size))
            .unwrap_or_else(|| Bounds::new(point(px(240.0), px(160.0)), window_size))
    };
    // GHOSTEX_NATIVE_MODAL_DEMO_BACKGROUND=1 opens the window without taking
    // focus, so an agent can screenshot it while the user keeps typing elsewhere.
    let background = env("GHOSTEX_NATIVE_MODAL_DEMO_BACKGROUND") == "1" || !snapshot.is_empty();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        focus: !background,
        show: true,
        is_resizable: false,
        is_minimizable: false,
        titlebar: None,
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let slot: Rc<RefCell<Option<Entity<V>>>> = Rc::new(RefCell::new(None));
    let out = slot.clone();
    let window = cx
        .open_window(options, move |window, cx| {
            window.set_window_title("");
            if !background {
                window.activate_window();
            }
            let view = build(window, cx);
            *out.borrow_mut() = Some(view.clone());
            cx.new(|cx| Root::new(view, window, cx).bg(gpui::transparent_black()))
        })
        .expect("open the demo window");
    let view = slot.borrow_mut().take().expect("the demo view");
    if !snapshot.is_empty() {
        snapshot_after_wait(window, snapshot, cx);
    }
    (window, view)
}

#[cfg(feature = "modal-demo-snapshot")]
fn snapshot_after_wait(window: WindowHandle<Root>, path: String, cx: &mut App) {
    let wait = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT_WAIT")
        .ok()
        .and_then(|ms| ms.trim().parse::<u64>().ok())
        .unwrap_or(2500);
    // GHOSTEX_NATIVE_MODAL_DEMO_HOVER=<x>,<y>: point there before drawing, for a hover or tooltip.
    let hover = std::env::var("GHOSTEX_NATIVE_MODAL_DEMO_HOVER")
        .ok()
        .and_then(|spot| {
            let (x, y) = spot.split_once(',')?;
            Some(point(
                px(x.trim().parse().ok()?),
                px(y.trim().parse().ok()?),
            ))
        });
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(wait))
            .await;
        if let Some(position) = hover {
            // Through the plain window handle: a typed update would hold the root the event reads.
            let _ = cx.update_window(window.into(), |_, window, cx| {
                window.dispatch_event(
                    gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                        position,
                        pressed_button: None,
                        modifiers: gpui::Modifiers::default(),
                    }),
                    cx,
                );
                window.refresh();
            });
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
        }
        let _ = window.update(cx, |_, window, _| window.refresh());
        cx.background_executor()
            .timer(std::time::Duration::from_millis(300))
            .await;
        let _ = window.update(cx, |_, window, _| match window.render_to_image() {
            Ok(image) => match image.save(&path) {
                Ok(()) => eprintln!("wrote {path}"),
                Err(error) => eprintln!("couldn't write {path}: {error}"),
            },
            Err(error) => eprintln!("couldn't draw the snapshot: {error}"),
        });
        let _ = cx.update(|cx| cx.quit());
    })
    .detach();
}

#[cfg(not(feature = "modal-demo-snapshot"))]
fn snapshot_after_wait(_window: WindowHandle<Root>, _path: String, cx: &mut App) {
    eprintln!("snapshots need: cargo build --bin native-modal-demo --features modal-demo-snapshot");
    cx.quit();
}

/// `open_modal_window` for the large panel modals (Settings, Agents Hub, Commit Changes, File
/// Diff), which the app opens 10% bigger than their base frame.
fn open_large_modal_window<V: Render>(
    width: f32,
    height: f32,
    build: impl FnOnce(&mut gpui::Window, &mut App) -> Entity<V>,
    cx: &mut App,
) -> (WindowHandle<Root>, Entity<V>) {
    let (width, height) = native_modal_kit::large_panel_modal_size(width, height);
    open_modal_window(width, height, build, cx)
}

/// `open_modal_window` for the Settings dialog, which the app opens 20% bigger than the other
/// large panels (CDXC:Settings 2026-10-14 in app/model/app_modal_kind.rs).
fn open_settings_dialog_window<V: Render>(
    width: f32,
    height: f32,
    build: impl FnOnce(&mut gpui::Window, &mut App) -> Entity<V>,
    cx: &mut App,
) -> (WindowHandle<Root>, Entity<V>) {
    let (width, height) = native_modal_kit::settings_dialog_modal_size(width, height);
    open_modal_window(width, height, build, cx)
}

fn deliver_export_result(
    window: WindowHandle<Root>,
    view: Entity<GpuiExportTranscriptModalWindow>,
    ok: bool,
    cx: &mut App,
) {
    let _ = window.update(cx, |_root, window, cx| {
        view.update(cx, |modal, cx| {
            if ok {
                modal.receive_result(
                    true,
                    Some(FAKE_PATH.to_string()),
                    true,
                    Some("codex".to_string()),
                    None,
                    window,
                    cx,
                );
            } else {
                modal.receive_result(
                    false,
                    None,
                    false,
                    None,
                    Some("This session has no transcript yet. Send a prompt first.".to_string()),
                    window,
                    cx,
                );
            }
        });
    });
}

/// This computer, and two connected ones: one with the repository, one without it.
fn demo_handoff_computers() -> Vec<ExportTranscriptComputer> {
    let project = |id: &str, title: &str, path: &str, same_repo: bool| ExportTranscriptProject {
        project_id: id.to_string(),
        title: title.to_string(),
        path: Some(path.to_string()),
        same_repo,
        is_own: false,
    };
    vec![
        ExportTranscriptComputer {
            machine_id: "local".to_string(),
            label: "This machine".to_string(),
            is_source: true,
            projects: vec![
                ExportTranscriptProject {
                    is_own: true,
                    ..project("p-scratch", "Scratch", "~/dev/scratch", false)
                },
                project("p-acme", "acme-web", "~/dev/acme-web", false),
                project("p-docs", "gamma-docs", "~/dev/gamma-docs", false),
            ],
            preferred_project_id: Some("p-scratch".to_string()),
            missing_repository: None,
        },
        ExportTranscriptComputer {
            machine_id: "machine-studio".to_string(),
            label: "studio-mac".to_string(),
            is_source: false,
            projects: vec![
                project("p-ghostex", "Ghostex", "~/dev/Ghostex", true),
                project("p-site", "website", "~/sites/website", false),
            ],
            preferred_project_id: Some("p-ghostex".to_string()),
            missing_repository: None,
        },
        ExportTranscriptComputer {
            machine_id: "machine-linux".to_string(),
            label: "linux-box".to_string(),
            is_source: false,
            projects: vec![project("p-notes", "notes", "~/notes", false)],
            preferred_project_id: None,
            missing_repository: Some("ghostex".to_string()),
        },
    ]
}

/// `GHOSTEX_NATIVE_MODAL_DEMO_STATE=working` shows the message for a session that is still working.
fn open_session_handoff(demo: &DemoEnv, cx: &mut App) {
    use session_handoff_modal::*;
    let agents = ["claude", "codex"]
        .into_iter()
        .map(|id| ExportTranscriptAgent {
            agent_id: id.to_string(),
            name: {
                let mut name = id.to_string();
                name[..1].make_ascii_uppercase();
                name
            },
            family: Some(id.to_string()),
        })
        .collect();
    let config = SessionHandoffModalConfig {
        project_title: "acme-web".to_string(),
        agents,
        default_agent_id: Some("claude".to_string()),
        working_agent: (demo.state == "working").then(|| "Claude".to_string()),
        palette: demo.palette,
    };
    let host: SessionHandoffModalHost = Rc::new(move |command, cx: &mut App| match command {
        SessionHandoffModalCommand::Confirm {
            agent_id,
            close_source,
            warned,
        } => {
            eprintln!("handoff to {agent_id}: close_source={close_source} warned={warned}");
            cx.quit();
        }
        SessionHandoffModalCommand::Cancel => cx.quit(),
    });
    open_modal_window(
        SESSION_HANDOFF_MODAL_WIDTH,
        SESSION_HANDOFF_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiSessionHandoffModalWindow::new(config, host, window, cx)),
        cx,
    );
}

fn open_export_transcript(demo: &DemoEnv, cx: &mut App) {
    let initial_mode = match env("GHOSTEX_EXPORT_MODAL_DEMO_MODE").as_str() {
        "export" => Some(ExportTranscriptMode::Export),
        "handoff" => Some(ExportTranscriptMode::Handoff),
        _ => None,
    };
    let agents = if demo.state == "noagents" {
        Vec::new()
    } else {
        ["codex", "claude", "gemini"]
            .into_iter()
            .map(|id| ExportTranscriptAgent {
                agent_id: id.to_string(),
                name: {
                    let mut name = id.to_string();
                    name[..1].make_ascii_uppercase();
                    name
                },
                family: Some(id.to_string()),
            })
            .collect()
    };
    let slot: Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiExportTranscriptModalWindow>)>>> =
        Rc::new(RefCell::new(None));
    let host_slot = slot.clone();
    let fail = demo.state == "failed";
    let host: ExportTranscriptModalHost = Rc::new(move |command, cx: &mut App| match command {
        ExportTranscriptModalCommand::RunExport {
            include,
            last_turns,
            carry,
        } => {
            eprintln!(
                "run export: commands={} patches={} reasoning={} last_turns={last_turns:?} carry={carry}",
                include.commands, include.patches, include.reasoning
            );
            let slot = host_slot.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let target = slot.borrow().clone();
                if let Some((window, view)) = target {
                    cx.update(|cx| deliver_export_result(window, view, !fail, cx));
                }
            })
            .detach();
        }
        ExportTranscriptModalCommand::StartConversation { agent_id, target } => {
            eprintln!("handoff to {agent_id} at {target:?}");
            cx.quit();
        }
        // A 120-turn conversation of about 3.6 KB a turn, a little more with each switch on.
        ExportTranscriptModalCommand::CountTurns(include) => {
            let slot = host_slot.clone();
            cx.defer(move |cx| {
                let open = slot.borrow().clone();
                if let Some((window, view)) = open {
                    let _ = window.update(cx, |_, _window, cx| {
                        view.update(cx, |modal, cx| {
                            let turn = 3_600
                                + 900 * usize::from(include.commands)
                                + 600 * usize::from(include.patches)
                                + 400 * usize::from(include.reasoning);
                            let sizes: Vec<usize> = (1..=120).map(|n| n * turn).collect();
                            modal.set_turn_count(
                                include,
                                ExportTranscriptTurnCount::Counted {
                                    turns: 120,
                                    whole_bytes: 120 * turn,
                                    last_turns_bytes: sizes,
                                },
                                cx,
                            );
                        })
                    });
                }
            });
        }
        ExportTranscriptModalCommand::TargetChanged(target) => {
            eprintln!("target {target:?}");
            let slot = host_slot.clone();
            cx.defer(move |cx| {
                let open = slot.borrow().clone();
                if let Some((window, view)) = open {
                    let _ = window.update(cx, |_, _window, cx| {
                        view.update(cx, |modal, cx| {
                            let line = (target.machine_id == "machine-studio").then(|| {
                                "This session is on feature-x; the project on studio-mac is on main."
                                    .to_string()
                            });
                            modal.set_branch_notice(target, line, cx);
                        });
                    });
                }
            });
        }
        ExportTranscriptModalCommand::AddProjectOn { machine_id, picks } => {
            eprintln!("add project on {machine_id}, reopen with {picks:?}");
            cx.quit();
        }
        ExportTranscriptModalCommand::Cancel => {
            eprintln!("cancel");
            cx.quit();
        }
        ExportTranscriptModalCommand::Reveal => {
            eprintln!("reveal");
            cx.quit();
        }
        ExportTranscriptModalCommand::PathCopied => eprintln!("path copied (copy feedback)"),
    });
    let config = ExportTranscriptModalConfig {
        agents,
        default_agent_id: Some("codex".to_string()),
        handoff_target: None,
        palette: demo.palette,
        // A saved prefs file, to draw what a user comes back to.
        prefs_path: Some(env("GHOSTEX_EXPORT_MODAL_DEMO_PREFS"))
            .filter(|path| !path.is_empty())
            .map(std::path::PathBuf::from),
        initial_mode,
        computers: demo_handoff_computers(),
        restore: None,
    };
    let (window, view) = open_modal_window(
        EXPORT_TRANSCRIPT_MODAL_WIDTH,
        EXPORT_TRANSCRIPT_MODAL_INITIAL_HEIGHT,
        move |window, cx| {
            cx.new(|cx| GpuiExportTranscriptModalWindow::new(config, host, window, cx))
        },
        cx,
    );
    *slot.borrow_mut() = Some((window, view.clone()));
    if demo.state == "done" || demo.state == "failed" {
        let ok = demo.state == "done";
        cx.defer(move |cx| deliver_export_result(window, view, ok, cx));
    }
}

fn main() {
    let demo = DemoEnv::read();
    gpui_platform::application()
        .with_assets(assets::GhostexAssets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            ui_fonts::register(cx);
            match demo.modal.as_str() {
                "export-transcript" => open_export_transcript(&demo, cx),
                "session-handoff" => open_session_handoff(&demo, cx),
                // DEMO-ARMS: one arm per converted modal.
                "add-project" => add_project_demo::open(&demo, cx),
                "new-thread-picker" => new_thread_picker_demo::open(&demo, cx),
                "update-available" => update_available_demo::open(&demo, cx),
                "notice-modal" => notice_modal_demo::open(&demo, cx),
                "updates-menu" => updates_menu_demo::open(&demo, cx),
                "share-popover" => share_popover_demo::open(&demo, cx),
                "start-menu" => start_menu_demo::open(&demo, cx),
                "start-menu-click" => start_menu_click_demo::open(&demo, cx),
                "remote-setup" => remote_setup_demo::open(&demo, cx),
                "delayed-send" => delayed_send_demo::open(&demo, cx),
                "space-editor" => space_editor_demo::open(&demo, cx),
                "settings" => settings_demo::open(&demo, cx),
                "create-worktree" => create_worktree_demo::open(&demo, cx),
                "remote-gxserver-install" => remote_gxserver_install_demo::open(&demo, cx),
                "delete-worktree" => delete_worktree_demo::open(&demo, cx),
                "rename-worktree" => rename_worktree_demo::open(&demo, cx),
                "missing-project-folder" => missing_project_folder_demo::open(&demo, cx),
                "rename-session" => rename_session_demo::open(&demo, cx),
                "new-orchestrator" => new_coordinator_demo::open(&demo, cx),
                "orchestrator-memory" => coordinator_details_demo::open(&demo, cx),
                "close-buttons" => close_buttons_demo::open(&demo, cx),
                "session-note" => session_note_demo::open(&demo, cx),
                "chief-row" => chief_row_demo::open(&demo, cx),
                "extension-menu" => extension_menu_demo::open(&demo, cx),
                "voice-panel" => voice_panel_demo::open(&demo, cx),
                "permission-guide" => permission_guide_demo::open(&demo, cx),
                "install-hooks" => agent_hooks_required_demo::open(&demo, cx),
                "security-upgrade" => security_upgrade_demo::open(&demo, cx),
                "agents-hub" => agents_hub_demo::open(&demo, cx),
                "onboarding" => onboarding_demo::open(&demo, cx),
                "web-runtime-prompt" => web_runtime_prompt_demo::open(&demo, cx),
                other => {
                    eprintln!("unknown modal {other:?}");
                    cx.quit();
                    return;
                }
            }
            // A background or snapshot run never takes focus from the app the user is typing in.
            if env("GHOSTEX_NATIVE_MODAL_DEMO_BACKGROUND") != "1"
                && env("GHOSTEX_NATIVE_MODAL_DEMO_SNAPSHOT").is_empty()
            {
                cx.activate(true);
            }
        });
}

/// CDXC:AppModal 2026-10-08 WHY:
/// The demo compiles the app's modals without the app's AppKit glue. `popup_dismissal`'s click-away check calls `GhostexGpuiApplicationIsActive`, and the only definitions live in Objective-C objects that also reference symbols defined in the main app, so linking them into the demo failed the 10.15.5 macOS build. The demo answers "active" itself: clicking another app never closes a demo modal.
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiApplicationIsActive() -> bool {
    true
}
