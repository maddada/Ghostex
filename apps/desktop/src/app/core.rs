// C1 wave-4 extraction: the `GhostexGpuiApp` god object itself -- its 301
// (now `pub(crate)`) fields plus the three trait impls that only it can own:
// `Drop`, `EntityInputHandler`, and `Render`. Every other `impl GhostexGpuiApp`
// block lives in a sibling app/*.rs module. Pure move, no logic changes.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.
use std::cell::Cell;

use crate::terminal_surface_host::NativeTerminalSurfaceHost;
use crate::terminal_surface_lifecycle::NativeTerminalSurfaceLifecycleState;
use gpui::Bounds;
use gpui::ClipboardItem;
use gpui::Entity;
use gpui::EntityInputHandler;
use gpui::FocusHandle;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Point;
use gpui::Render;
use gpui::ScrollHandle;
use gpui::Styled as _;
use gpui::UTF16Selection;
use gpui::Window;
use gpui::WindowHandle;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
#[cfg(target_os = "linux")]
use gpui::{CursorStyle, Decorations, ResizeEdge};
use gpui_component::h_flex;
use gpui_component::input::InputState;
use gpui_component::v_flex;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::hotkeys::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

#[cfg(target_os = "linux")]
const GPUI_LINUX_CLIENT_FRAME_WIDTH: Pixels = px(4.0);

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum GpuiLinuxResizeRegionShape {
    Horizontal,
    Vertical,
    Corner,
}

#[cfg(target_os = "linux")]
fn gpui_linux_resize_region(
    edge: ResizeEdge,
    shape: GpuiLinuxResizeRegionShape,
) -> gpui::AnyElement {
    let cursor = match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    };
    div()
        .flex()
        .flex_shrink_0()
        .bg(titlebar_button_border_color())
        .cursor(cursor)
        .when(
            matches!(shape, GpuiLinuxResizeRegionShape::Horizontal),
            |this| this.h(GPUI_LINUX_CLIENT_FRAME_WIDTH).flex_1(),
        )
        .when(
            matches!(shape, GpuiLinuxResizeRegionShape::Vertical),
            |this| this.w(GPUI_LINUX_CLIENT_FRAME_WIDTH).h_full(),
        )
        .when(
            matches!(shape, GpuiLinuxResizeRegionShape::Corner),
            |this| this.size(GPUI_LINUX_CLIENT_FRAME_WIDTH),
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            window.start_window_resize(edge);
        })
        .into_any_element()
}

#[cfg(target_os = "linux")]
fn gpui_linux_client_window_frame(
    content: gpui::AnyElement,
    window: &mut Window,
) -> gpui::AnyElement {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return content;
    };

    /*
    Keep resize ownership in eight exact, non-overlapping layout regions. The
    visible four-pixel frame is itself the grab target; no transparent view or
    full-window hit-test layer extends across app or CEF content. Tiled edges
    leave the layout entirely, matching the compositor's edge constraints.
    */
    window.set_client_inset(GPUI_LINUX_CLIENT_FRAME_WIDTH);
    v_flex()
        .size_full()
        .min_w_0()
        .min_h_0()
        .when(!tiling.top, |this| {
            this.child(
                h_flex()
                    .w_full()
                    .h(GPUI_LINUX_CLIENT_FRAME_WIDTH)
                    .when(!tiling.left, |this| {
                        this.child(gpui_linux_resize_region(
                            ResizeEdge::TopLeft,
                            GpuiLinuxResizeRegionShape::Corner,
                        ))
                    })
                    .child(gpui_linux_resize_region(
                        ResizeEdge::Top,
                        GpuiLinuxResizeRegionShape::Horizontal,
                    ))
                    .when(!tiling.right, |this| {
                        this.child(gpui_linux_resize_region(
                            ResizeEdge::TopRight,
                            GpuiLinuxResizeRegionShape::Corner,
                        ))
                    }),
            )
        })
        .child(
            h_flex()
                .w_full()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .when(!tiling.left, |this| {
                    this.child(gpui_linux_resize_region(
                        ResizeEdge::Left,
                        GpuiLinuxResizeRegionShape::Vertical,
                    ))
                })
                .child(
                    div()
                        .h_full()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .overflow_hidden()
                        .child(content),
                )
                .when(!tiling.right, |this| {
                    this.child(gpui_linux_resize_region(
                        ResizeEdge::Right,
                        GpuiLinuxResizeRegionShape::Vertical,
                    ))
                }),
        )
        .when(!tiling.bottom, |this| {
            this.child(
                h_flex()
                    .w_full()
                    .h(GPUI_LINUX_CLIENT_FRAME_WIDTH)
                    .when(!tiling.left, |this| {
                        this.child(gpui_linux_resize_region(
                            ResizeEdge::BottomLeft,
                            GpuiLinuxResizeRegionShape::Corner,
                        ))
                    })
                    .child(gpui_linux_resize_region(
                        ResizeEdge::Bottom,
                        GpuiLinuxResizeRegionShape::Horizontal,
                    ))
                    .when(!tiling.right, |this| {
                        this.child(gpui_linux_resize_region(
                            ResizeEdge::BottomRight,
                            GpuiLinuxResizeRegionShape::Corner,
                        ))
                    }),
            )
        })
        .into_any_element()
}

pub struct GhostexGpuiApp {
    pub(crate) parent_ns_view: *mut std::ffi::c_void,
    pub(crate) project_name: String,
    pub(crate) sidebar_url: String,
    pub(crate) browser_url: String,
    pub(crate) active_mode: TitlebarMode,
    pub(crate) shell_focus: ShellFocusTarget,
    pub(crate) previous_non_command_focus: Option<ShellFocusTarget>,
    pub(crate) first_responder_target: FirstResponderTarget,
    pub(crate) first_responder_transition_suppressed_by_programmatic_focus: bool,
    #[cfg(target_os = "macos")]
    pub(crate) source_workarea_cef_menu_passthrough_active: bool,
    #[cfg(target_os = "macos")]
    pub(crate) renderer_edit_hotkey_passthrough_active: bool,
    pub(crate) programmatic_focus_depth: u32,
    pub(crate) sidebar_focus_border_handoff: Option<SidebarFocusBorderHandoff>,
    pub(crate) agents_workspace: WorkspaceModel,
    /*
    Agents split/tab topology is owned by the exact project/worktree id. The
    live model belongs to `agents_workspace_project_id`; inactive project
    models are parked instead of being destructively reconciled with another
    project's session projection.
    */
    pub(crate) agents_workspace_project_id: Option<String>,
    pub(crate) parked_agents_workspaces_by_project: HashMap<String, serde_json::Value>,
    pub(crate) parked_agents_terminal_runtimes_by_project:
        HashMap<String, ParkedAgentsTerminalRuntime>,
    /*
    CDXC:GPUISessionChatSurfacePerProject 2026-08-26:
    The chat pages of inactive projects, parked beside their terminal owners on
    the same switch path instead of being destroyed and reloaded. Runtime-only,
    and dropped with the app because the entity owns the Chromium browser.
    */
    pub(crate) parked_agents_chat_runtimes_by_project: HashMap<String, ParkedAgentsChatRuntime>,
    /*
    CDXC:GPUIProjectSwitchCoalescing 2026-07-29:
    Leading-edge + trailing-debounce state for project switches. `until` is the
    end of the settle window opened by the switch that is currently executing,
    the queue holds at most one collapsed request per bridge kind (newest wins,
    arrival order preserved), and the scheduled flag keeps one trailing timer in
    flight instead of one per superseded click.
    */
    pub(crate) project_switch_settling_until: Option<Instant>,
    pub(crate) project_switch_pending_requests: Vec<GpuiPendingProjectSwitchRequest>,
    pub(crate) project_switch_flush_scheduled: bool,
    pub(crate) command_pane: CommandPaneModel,
    /*
    CDXC:GPUICommandPanePerProject 2026-07-10:
    Command panes are per-project state like macOS `NativeProject.commandsPanel`.
    The live `command_pane` belongs to `command_pane_project_id`; inactive
    projects keep their panes parked as writer-owned shell-state JSON so a
    project switch swaps panels instead of sharing one global panel. Parking
    must never kill daemon-side gxserver sessions; only explicit tab close
    does. The epoch invalidates in-flight attach-plan completions so a
    completion from before a swap cannot mutate or kill the wrong project's
    sessions.
    */
    pub(crate) command_pane_project_id: Option<String>,
    pub(crate) parked_command_panes_by_project: HashMap<String, serde_json::Value>,
    pub(crate) command_pane_project_epoch: u64,
    pub(crate) project_editor_shell: ProjectEditorShellModel,
    pub(crate) project_editor_auto_sleep_epochs: ProjectEditorAutoSleepEpochs,
    pub(crate) project_editor_auto_sleep_policy: ProjectEditorAutoSleepPolicySnapshot,
    pub(crate) browser_profiles: BrowserProfileModel,
    pub(crate) browser_tabs: BrowserTabModel,
    pub(crate) browser_tabs_project_id: Option<String>,
    pub(crate) parked_browser_tabs_by_project: HashMap<String, BrowserTabModel>,
    /*
    CDXC:GPUIBrowserProjectParking 2026-08-26:
    The live browser pages of inactive projects, parked beside their tab models
    on the same switch path instead of being destroyed. Dropping the
    `Entity<CefSurface>` closes the Chromium browser, so a project switch that
    cleared `browser_surfaces` slept every browser tab of the project the user
    just left and reloaded its pages from scratch on the way back.
    */
    pub(crate) parked_browser_runtimes_by_project: HashMap<String, ParkedBrowserRuntime>,
    pub(crate) browser_tabs_project_epoch: u64,
    /*
    CDXC:GPUIBrowserProjectParking 2026-08-26:
    Identity of the browser tab model currently mounted in the Browser workarea.
    BrowserTabIds are project-local counters, so a surface's async CEF callbacks
    capture the key that was live when the surface was created and resolve it
    back to their own model — live or parked — instead of applying a title,
    favicon, address, or load update to whichever project happens to hold the
    same tab id now.
    */
    pub(crate) browser_tabs_runtime_key: u64,
    pub(crate) sidebar_browser_tabs_snapshot: String,
    /*
    CDXC:AutoSleepDisplayedSessions 2026-08-20:
    Last published set of local gxserver sessions this shell is actually showing
    (terminal body or chat surface). The sidebar runtime's Auto Sleep sweep
    otherwise decides visibility from its own click history, which cannot see a
    parked terminal behind a chat surface and is wiped on a daemon reconnect.
    */
    pub(crate) sidebar_displayed_sessions_snapshot: String,
    /*
    CDXC:SidebarBrowserTabReveal 2026-08-18:
    A Browser tab the user just opened, waiting to be revealed in the sidebar.
    The reveal is deferred to the tab-snapshot publish instead of being sent at
    creation time because the sidebar can only expand and scroll to a row it
    already knows about, and the row reaches it in that same snapshot.
    */
    pub(crate) pending_sidebar_browser_tab_reveal: Option<PendingSidebarBrowserTabReveal>,
    pub(crate) sidebar_browser_tab_reveal_request_id: u64,
    /*
    CDXC:ExportTranscript 2026-08-20:
    The path of the markdown file the open Export Transcript result dialog is
    describing, captured from the dialog's own open message. Reveal in Finder
    reads this instead of trusting a path posted back by the modal page, and it
    stays `None` for remote sessions because their export lives on the remote
    machine's disk, not this one.
    */
    pub(crate) pending_export_transcript_reveal_path: Option<String>,
    pub(crate) latest_sidebar_project_snapshot: Option<GpuiProjectSnapshot>,
    /*
    CDXC:NavigationHistory 2026-08-19:
    Back/Forward availability plus their tooltips, pushed by the sidebar runtime
    whenever gxserver's trail changes. The render path may only read this cached
    value — see `navigation_history` for why the titlebar owns no trail state.
    */
    pub(crate) navigation_history_state: navigation_history::GpuiNavigationHistoryState,
    pub(crate) titlebar_git_menu_state: Option<GpuiTitlebarGitMenuState>,
    // Titlebar Actions are projected from gxserver's `/api/readSidebarHud`.
    // The render path must only read this cached snapshot; the RPC runs on
    // the background executor (startup, active-project change, and the shared
    // 2s policy poll), never per frame on the main thread.
    pub(crate) titlebar_actions_snapshot: Vec<GpuiTitlebarAction>,
    pub(crate) titlebar_actions_refresh_in_flight: bool,
    pub(crate) extensions_snapshot: GpuiExtensionsSnapshot,
    pub(crate) extension_projects: HashMap<String, GpuiExtensionProjectMetadata>,
    pub(crate) extension_session_details: HashMap<String, serde_json::Value>,
    pub(crate) extensions_refresh_in_flight: bool,
    pub(crate) titlebar_tips_unread_count: u64,
    // Platform-neutral updater state drives the native titlebar. Sparkle owns
    // macOS delivery and Velopack owns Windows delivery; only the Windows
    // backend retains release metadata for the confirmation child window.
    pub(crate) updater_started: bool,
    #[allow(dead_code)] // read by the windows update path (begin_windows_update_check)
    pub(crate) update_checking: bool,
    pub(crate) update_available: bool,
    pub(crate) update_downloading: bool,
    pub(crate) update_download_progress: Option<f64>,
    #[cfg(target_os = "windows")]
    pub(crate) windows_updater: Option<windows_updater::WindowsUpdater>,
    #[cfg(target_os = "windows")]
    pub(crate) windows_update: Option<windows_updater::WindowsUpdate>,
    #[cfg(target_os = "windows")]
    pub(crate) windows_ready_update: Option<windows_updater::WindowsReadyUpdate>,
    #[cfg(target_os = "windows")]
    pub(crate) windows_first_run_setup_state: GpuiWindowsFirstRunSetupState,
    // True while the standalone GhostexEditor daemon reports at least one
    // open editor window (polled over its unix socket). Drives the titlebar
    // "Prompt Editor" bring-to-front affordance.
    pub(crate) prompt_editor_daemon_open: bool,
    // Portless setup prompt suppression is memory-only for this app run
    // (macOS `portlessSetupPromptSuppressedUntilRestart` /
    // `activePortlessSetupPromptMode` parity).
    pub(crate) portless_setup_prompt_suppressed_until_restart: bool,
    pub(crate) active_portless_setup_prompt_mode: Option<GpuiPortlessSetupPromptMode>,
    /*
    CDXC:GPUIPortlessPromptDeferral 2026-08-18:
    GPUI hosts a single app-modal window, so a Portless prompt resolved while
    the user already has a modal open cannot be shown right away. Remember that
    deferral instead of dropping the prompt for the rest of the run, and let
    the next modal close re-run the check.
    */
    pub(crate) portless_setup_prompt_pending_modal_close: bool,
    /*
    CDXC:GPUIFirstRunOnboardingOnce 2026-08-18:
    First-run onboarding markers are now persisted only after their surface is
    actually shown, so the on-disk flags can no longer dedupe the entry points
    that start onboarding (gxserver bootstrap, CEF init, daemon respawn). This
    memory-only latch owns that dedupe for the process.
    */
    pub(crate) first_run_onboarding_started: bool,
    pub(crate) active_open_target_id: Option<String>,
    pub(crate) active_action_command_id: Option<String>,
    /*
    CDXC:GPUIQuickActionCooldown 2026-08-27:
    Clicking the titlebar Quick Actions button now restarts a still-running
    Action's terminal, so rapid re-clicks would churn kill/create cycles. The
    button holds a short runtime-only cooldown after each terminal launch;
    nothing about it is persisted or logged.
    */
    pub(crate) titlebar_quick_action_cooldown_until: Option<std::time::Instant>,
    /*
    CDXC:GPUITitlebarActions 2026-06-27-09:26:
    The GPUI titlebar Action button needs the same click-time Debug rerun decision as the shared command palette, but Rust owns that native control. Keep only command ids, active run ids, and coarse run state in memory so titlebar clicks can mirror close-on-exit failure reruns without storing command text, URLs, cwd/env, paths, status-file paths, terminal output, logs, or shell-state data.
    */
    pub(crate) sidebar_command_run_feedback_states:
        HashMap<String, GpuiSidebarCommandRunFeedbackState>,
    pub(crate) keep_awake_runtime: Option<GpuiKeepAwakeRuntime>,
    pub(crate) keep_awake_runtime_generation: u64,
    /*
    CDXC:GPUITitlebarKeepAwake 2026-06-25-23:49:
    GPUI Keep Awake automation is runtime-only. Store only the GPUI-owned caffeinate child, autostart suppression, power-rule ticker state, and active working-session count/grace state; do not persist runtime state, shell commands, probe output, session names, paths, titles, or renderer-provided power data.

    CDXC:GPUITitlebarKeepAwake 2026-06-26-00:09:
    Closed-lid prevention is a lease attached to the active GPUI Keep Awake runtime. Track only runtime ids and helper lease booleans so async helper enable/heartbeat completions can disable stale leases without storing helper paths, installer output, pmset text, commands, project data, or user content.

    CDXC:GPUITitlebarKeepAwake 2026-06-26-00:29:
    Working-session automation now uses only GPUI's safe terminal activity enums and lifecycle booleans. Cache the last observed count plus the 20-minute grace deadline in memory so Settings parity can react to Working-to-idle transitions without persisting or logging terminal titles, commands, paths, output, project data, or renderer-provided session metadata.
    */
    pub(crate) keep_awake_auto_start_suppressed: bool,
    pub(crate) keep_awake_power_ticker_active: bool,
    pub(crate) keep_awake_previous_working_session_count: usize,
    pub(crate) keep_awake_working_session_grace_until: Option<Instant>,
    /*
    CDXC:GPUIRemoteMachinesSettings 2026-06-24-14:34:
    GPUI Remote Settings reconnect owns only live SSH tunnel processes and runtime auth needed to talk to the remote gxserver. Keep remote tokens out of settings, logs, sidebar state, Browser/workarea CEF clients, and persistent progress; terminate the old tunnel before replacing it so Settings reconnect behaves like the macOS app.
    */
    /*
    CDXC:GPUIRemoteConnectOverlay 2026-08-07:
    Latest bounded connect wire state per saved machine, written at the single
    status dispatch choke point. The terminal body renders a status overlay
    from this while a surfaced remote session's machine is unreachable, so a
    restored remote tab explains itself instead of showing an empty rectangle.
    */
    pub(crate) remote_machine_connect_states: HashMap<String, String>,
    pub(crate) remote_gxserver_connections: HashMap<String, GpuiRemoteGxserverConnection>,
    pub(crate) remote_gxserver_connect_generations: HashMap<String, u64>,
    pub(crate) remote_gxserver_watchdog_probe_in_flight: bool,
    /*
    CDXC:GPUIRemotePresentationStreaming 2026-06-24-19:54:
    Live remote presentation streams are owned by Rust beside the saved-machine tunnel. Use a runtime generation counter plus per-connection cancel flag so reconnect/disconnect prevents stale WebSocket snapshots or deltas from mutating a newer machine-scoped sidebar cache.
    */
    pub(crate) remote_gxserver_presentation_stream_generation: u64,
    /*
    CDXC:RemoteClone 2026-06-24-19:35:
    Remote repository clone progress is runtime-only GPUI bookkeeping keyed by the modal request id and remote gxserver job id. Store no repository URLs, remote paths, folder names, branch names, stdout/stderr, daemon bodies, SSH targets, tokens, or prompts; Rust uses this map only to poll/cancel the daemon-owned job through the live tunnel.
    */
    pub(crate) remote_repository_clone_requests: HashMap<String, GpuiRemoteRepositoryCloneRequest>,
    /*
    CDXC:GPUIRemoteAttach 2026-08-06:
    Remote attach focus state maps the bounded saved-machine/project/session
    identity to the GPUI terminal tab that owns the process-local SSH attach.
    Only that canonical identity and shell tab id cross the shell-state boundary
    so restored tabs reconcile one-to-one; SSH targets, tokens, remote paths,
    command text, titles, project/session names, logs, and runtime payloads do
    not.
    */
    pub(crate) remote_attach_sessions: HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
    /*
    CDXC:GPUIWorkspaceSessionReattach 2026-08-07:
    Project keys (local or machine-scoped remote) whose restored-from-disk
    Agents workspace has not yet run its one-shot resume pass. The first
    authoritative sidebar snapshot for such a project wakes the sleeping
    sessions its rendered panes actively surface, so reopening the app (or
    revisiting a restored project) resumes what the user was looking at
    instead of showing dead placeholders until a click. Consuming the key
    makes every later sleep a user decision this pass must not override.
    */
    pub(crate) startup_restore_wake_pending: HashSet<String>,
    /*
    In-flight surfaced-restore SSH plan preparations, so the repeated
    authoritative snapshots that arrive during startup cannot stack duplicate
    remote round trips for the same tab.
    */
    pub(crate) remote_workspace_attach_pending: HashSet<GpuiRemoteAttachSessionKey>,
    // CDXC:GPUIProjectViewMemory 2026-08-07: last workarea + companion
    // arrangement per canonical workspace project key. See GpuiProjectViewState.
    pub(crate) project_view_states_by_project: HashMap<String, GpuiProjectViewState>,
    #[cfg(target_os = "macos")]
    pub(crate) remote_attach_askpass_scripts:
        HashMap<GpuiRemoteAttachSessionKey, GpuiRemoteAskpassScript>,
    /*
    CDXC:GPUISourceRuntime 2026-06-24-23:17:
    GPUI Source uses a runtime-only code-server owner equivalent to macOS's shared editor process. It may hold the owned Child and current in-memory folder URL target while the app runs, but it must not persist paths, URLs, command text, stdout/stderr, tokens, page titles, or project names into shell state or support logs.

    CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
    Source readiness is represented by this direct code-server runtime owner, not by a parallel sidebar proof store. Kanban, Automate, and Manage readiness likewise derives from current project URL gates and owned CEF surfaces instead of stored readiness messages.
    */
    pub(crate) source_code_server_runtime: SourceCodeServerRuntimeOwner,
    pub(crate) pending_source_file_open: Option<PendingSourceFileOpen>,
    /*
    CDXC:GPUISessionChatLinks 2026-08-03:
    A chat file link that routes to Docs parks its project-relative path here
    until the Manage surface exists and accepts the app-owned open script.
    In-memory only: the path is a private project fact and never reaches shell
    state or logs.
    */
    pub(crate) pending_docs_file_open: Option<String>,
    /*
    Bounded filesystem authority for the one external or out-of-tree document
    explicitly opened from chat. The Docs bridge and HTML resource loader share
    it, while the project id prevents cross-project reuse.
    */
    pub(crate) session_chat_docs_file_authorization:
        Arc<Mutex<Option<GpuiSessionChatDocsFileAuthorization>>>,
    /*
    CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
    Source, Kanban, Automate, and Manage real CEF panes now have permanent app-owned runtime surface storage keyed by the safe workarea slot. The map owns Entity<CefSurface> plus the process-local direct runtime URL identity required to reject stale slot reuse; it must not store project names/paths, page titles, bridge payloads, file contents, tokens, cookies, shell text, or fallback navigation state, and creation is allowed only through a helper that receives a real runtime URL value.

    CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-28-17:09:
    Runtime surface ownership no longer keeps slot, URL-issuance, or owner-gate proof maps. The direct runtime URL gate is the only authority for retaining already-created project workarea CefSurface entities.

    CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-29-00:15:
    The map stores the current direct runtime URL identity only as process-local ownership metadata so Source/Kanban/Automate/Manage cannot reuse a valid-but-stale slot surface after active project changes. Do not persist, log, expose, or treat this as sidebar readiness proof.
    */
    pub(crate) project_workarea_runtime_cef_surfaces:
        HashMap<ProjectWorkareaCefSurfaceSlotKey, ProjectWorkareaRuntimeCefSurface>,
    /*
    CDXC:GPUIProjectSidebarBridge 2026-06-23-08:23:
    GPUI stores the last sidebar runtime settings snapshot it installed or sent so polling and Settings-save refreshes can no-op unchanged strict debug/beta plus saved-settings payloads and refresh only the sidebar CEF bridge when they change. Docs titlebar visibility and active-mode fallback use project-context availability instead of this settings snapshot.
    */
    pub(crate) sidebar_runtime_settings_snapshot: cef::SidebarRuntimeSettingsSnapshot,
    /*
    CDXC:GPUISidebarGxserverBootstrap 2026-06-24-11:17:
    The GPUI sidebar gxserver bootstrap is runtime memory only and may contain the localhost base URL plus bearer token read through the existing gxserver token helper. Store only the last sidebar-sent snapshot for change detection; never persist it, log it, copy it to Browser/workarea/modal CEF clients, or derive optional project/session ids from paths, titles, shell placeholders, or fixtures.

    CDXC:GPUISidebarGxserverBootstrap 2026-06-24-13:34:
    The sidebar's live active-project snapshot is now the GPUI-owned source for `initialActiveProjectId` when it carries an explicit gxserver project id. Do not use that project snapshot as a fallback source for focused or visible session ids; Worker 39 owns those separately only after real gxserver presentation session ids exist.

    CDXC:GPUISidebarGxserverFocusState 2026-06-24-21:07:
    Focused and visible gxserver session ids are now a separate runtime-only GPUI state sourced only from React's gxserver presentation session ids or Rust's remote attach session references. Store raw local gxserver ids and machine-scoped remote ids only; never derive this state from terminal shell ids, titles, paths, project names, command text, logs, or persisted layout.
    */
    pub(crate) sidebar_gxserver_bootstrap: Option<cef::SidebarGxserverBootstrap>,
    pub(crate) sidebar_gxserver_presentation_focus_state: GpuiGxserverPresentationFocusState,
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:08:
    Local sidebar session clicks need a runtime-only bridge from gxserver project/session identity to the GPUI Agents shell tab that owns the real attach process. Keep the latest focus key, map, pending attach set, and native tab lifecycle request ids process-local, prune them against the shell workspace, and store no titles, paths, commands, tokens, daemon bodies, terminal text, or persistent layout metadata here.

    CDXC:GPUIWorkspaceLifecycle 2026-06-26-07:25:
    Mapped GPUI workspace tab Close is local-first: mutate the Rust shell immediately, then notify the sidebar runtime for best-effort gxserver cleanup. Sleep/Wake still apply only from typed lifecycle results because their visible state depends on the backend transition. This mirrors macOS lifecycle ownership without logging or persisting project names, session titles, commands, paths, terminal content, or raw renderer payloads.

    CDXC:GPUIWorkspaceRenameCommand 2026-06-27-02:27:
    Mapped workspace rename uses this same runtime-only gxserver project/session to shell-tab map, then requires a currently mounted Running Agents Ghostty surface before sending `/rename <title>` and a real Return key. Do not store rename titles, raw renderer JSON, command text, paths, output, or fallback target choices here.
    */
    pub(crate) local_workspace_latest_focus_key: Option<GpuiLocalWorkspaceSessionKey>,
    pub(crate) local_workspace_session_mappings:
        HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    pub(crate) local_workspace_attach_pending: HashSet<GpuiLocalWorkspaceSessionKey>,
    /*
    CDXC:GPUISessionChatSurface 2026-07-31:
    Session Chat view mode is a runtime-only per-shell-session flag plus a
    per-session CEF surface. Keyed by shell TerminalSessionId — not
    (pane, session) — because the surface is pane-agnostic and must follow a
    tab across pane drags. Local chat surfaces
    use the local gxserver bootstrap; remote attach chat surfaces use the
    already-owned localhost SSH tunnel for that machine.

    CDXC:GPUISessionChatViewPersistence 2026-07-31:
    The set of chat-mode session ids IS persisted (bare shell session ids in
    shell-state JSON, `agentsChatModeSessions`) so each session reopens in its
    last-used view after an app restart. The CEF surfaces stay runtime-only.
    */
    pub(crate) agents_chat_mode_sessions: HashSet<TerminalSessionId>,
    /// The one terminal agent action bar whose "More actions" menu is open, by
    /// shell session id. Runtime-only, and single-valued because opening a
    /// second bar's menu closes the first, exactly like the chat composer's
    /// dropdown.
    pub(crate) agents_terminal_action_bar_menu_session: Option<TerminalSessionId>,
    /// A companion-pane maximize route waiting for the matching Agents bar's
    /// minimize action to restore its exact project view and companion slot.
    pub(crate) terminal_agent_bar_companion_focus_return:
        Option<TerminalAgentBarCompanionFocusReturn>,
    /// Sessions whose current compatibility state has already been considered
    /// for the saved automatic Chat preference, together with the effective
    /// Default Agent View each one was considered under. Runtime-only so a
    /// later agent detection in the same shell slot is still a fresh
    /// eligibility edge, and keyed by effective value so a session is swept
    /// again when that value changes — whether the user flipped the global
    /// preference or only this agent's per-agent override.
    pub(crate) agents_chat_auto_switch_observed_sessions:
        HashMap<TerminalSessionId, GpuiPreferredAgentInterface>,
    /// One-shot Chat launch requests waiting for a shell-session mapping. The
    /// mapped session enters Chat mode before any terminal focus handoff.
    pub(crate) pending_agents_chat_launch_intents: HashSet<GpuiWorkspaceTerminalSessionKey>,
    pub(crate) agents_chat_surfaces: HashMap<TerminalSessionId, Entity<CefSurface>>,
    /// When each currently hidden chat surface last became hidden, the clock the
    /// RAM eviction pass ages out. A surface that is visible has no entry, so a
    /// transient hide (a tab drag hides every surface for its duration) neither
    /// resets a running timer nor starts a spurious one.
    pub(crate) agents_chat_surface_hidden_since: HashMap<TerminalSessionId, Instant>,
    /// Chat surfaces whose page-side composer bridge has registered.
    pub(crate) session_chat_composer_ready_sessions: HashSet<TerminalSessionId>,
    /// Last composer-content report per chat page: `true` = the page said its
    /// composer is empty, `false` = it holds draft text or attached images.
    /// Reported by the page on composer mount, on every empty↔non-empty
    /// transition, and re-asserted on composer blur (which fires when the
    /// surface is hidden). RAM eviction requires an explicit `true`: a missing
    /// entry means the state is unknown (report lost or page never loaded) and
    /// unknown must never read as "empty" to a pass that destroys pages.
    pub(crate) session_chat_composer_empty_reports: HashMap<TerminalSessionId, bool>,
    /// One-shot terminal-to-chat keyboard handoff, completed only after the
    /// target page reports that its composer bridge is mounted.
    pub(crate) pending_session_chat_composer_focus: Option<TerminalSessionId>,
    pub(crate) pending_session_chat_composer_insert: HashMap<TerminalSessionId, String>,
    /// Chat-to-terminal drafts waiting for the exact terminal owner to remount.
    /// Each entry names the Saved Prompts row that holds the same text, so
    /// dropping the entry (teardown, a session that never comes back) leaves a
    /// recoverable copy behind instead of destroying the draft.
    pub(crate) pending_session_terminal_composer_insert:
        HashMap<TerminalSessionId, crate::app::session_chat::GpuiSessionChatDraftHandoff>,
    pub(crate) pending_session_chat_draft_handoffs: HashSet<TerminalSessionId>,
    /// Queued Ghostex prompts per on-screen terminal-view session, the input to
    /// the pane's "Queued: N" chrome row. Absent means zero.
    pub(crate) session_chat_queued_counts: HashMap<TerminalSessionId, GpuiSessionChatQueuedCounts>,
    pub(crate) session_chat_queued_count_refresh_in_flight: bool,
    pub(crate) local_workspace_lifecycle_requests: HashMap<u64, GpuiLocalWorkspaceLifecycleRequest>,
    pub(crate) next_local_workspace_lifecycle_request_id: u64,
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
    Status indicator and pet overlay bridge state is runtime-only GPUI presentation input from the sidebar CEF surface. Store only parsed bounded counts, booleans, size/pet ids, project/session ids, order, and short titles; do not persist it, log raw JSON, or use it as authority for filesystem, command, URL, token, terminal-content, or renderer side effects.

    CDXC:GPUIMenuBarStatusItem 2026-06-26-05:42:
    The compact GPUI menu-bar badge must be driven from this already parsed Rust status state, not from a renderer-owned menu-bar bridge.

    CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
    The primary-click Running Agents dropdown shares this state. Only counts, hide booleans, and bounded dropdown project/session ids/titles/status/order/timestamps may cross to AppKit; focus routing stays in fixed Rust/sidebar callbacks.

    CDXC:GPUISettingsNotifications 2026-06-26-06:56:
    GPUI macOS attention banners are derived only from this sanitized status snapshot on attention transition edges. Keep first-snapshot replay suppression, per-session/global rate state, and notification click routing in runtime memory only; never persist or log titles, ids, paths, URLs, commands, stdout/stderr, settings JSON, tokens, raw payloads, or terminal content.
    */
    pub(crate) sidebar_global_actions: Vec<GpuiSidebarGlobalActionState>,
    pub(crate) tab_strip_built_in_buttons: shared_settings::SharedTabStripBuiltInButtons,
    pub(crate) sidebar_session_status_indicators: GpuiSidebarSessionStatusIndicatorsState,
    pub(crate) sidebar_session_status_indicators_snapshot_seen: bool,
    pub(crate) session_attention_notification_rate_limiter:
        GpuiSessionAttentionNotificationRateLimiter,
    pub(crate) sidebar_pet_overlay: GpuiSidebarPetOverlayState,
    /*
    CDXC:GPUIAppShots 2026-06-25-23:28:
    App Shot insertion needs a bounded runtime-only map from local gxserver presentation session ids to GPUI Agents shell tabs because those id spaces may differ. Populate it only from explicit sidebar focus-state handoffs and currently mounted Agents surfaces; store no prompts, app/window metadata, project paths, titles, command text, terminal output, or persistent state.
    */
    pub(crate) local_app_shot_session_mappings: HashMap<String, TerminalSessionId>,
    /*
    CDXC:GPUICommandPane 2026-06-25-10:50:
    Sidebar command-session indicator refresh is change-detected against a sanitized JSON summary of command-pane sessions. Cache only that safe summary string for CEF bridge dedupe; do not store command text, paths, status-file paths, env, output, or persisted shell-state JSON here.
    */
    pub(crate) sidebar_command_pane_sessions_snapshot: String,
    /*
    Agents Delayed Send sidebar chrome crosses CEF as a sanitized list of
    combined project/session ids plus timer deadlines, labels, and remaining
    milliseconds. Cache only that display projection for bridge dedupe; never
    include terminal text, commands, paths, titles, agent prompts, or output.
    */
    pub(crate) sidebar_agents_delayed_sends_snapshot: String,
    pub(crate) sidebar_timer_presentations_replayed_after_ready: bool,
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-15:11:
    GPUI Delayed Send timers for command-pane terminals are runtime-owned session timers. Store only shell session ids, UTC deadlines, and cancellation generations in memory; persist only the bounded restart checkpoint described below.

    CDXC:GPUICommandDelayedSend 2026-06-25-16:41:
    The runtime map remains process-owned, but shell persistence may snapshot the UTC deadline plus remaining milliseconds for restart re-arm parity with macOS. Never expand that snapshot to command text, terminal content, titles, paths, runtime ids, stdout/stderr, or countdown labels.
    */
    pub(crate) command_delayed_send_timers: HashMap<CommandSessionId, GpuiCommandDelayedSendTimer>,
    pub(crate) command_delayed_send_generation: u64,
    pub(crate) command_delayed_send_countdown_ticker_active: bool,
    pub(crate) command_delayed_send_persistence_ticker_active: bool,
    /*
    CDXC:GPUICommandPaneGxserverAttach 2026-07-04:
    Command-pane GPUI-engine terminals are local shell tabs only until their
    gxserver command-surface session has been created and attached. Keep the
    real daemon project/session identity beside the command tab id so launch
    payloads, OSC state, close cleanup, and Phase 2 restore can use the
    daemon-owned zmx session without persisting project paths, command text, or
    attach commands.
    */
    pub(crate) command_gxserver_session_mappings:
        HashMap<CommandSessionId, GpuiLocalWorkspaceSessionKey>,
    pub(crate) command_gxserver_attach_pending: HashSet<CommandSessionId>,
    pub(crate) pending_command_gxserver_cleanup: HashSet<GpuiLocalWorkspaceSessionKey>,
    pub(crate) command_gxserver_cleanup_in_flight: HashSet<GpuiLocalWorkspaceSessionKey>,
    /*
    CDXC:GPUICommandWorkspaceTransfer 2026-08-01:
    Agents shell sessions that were just dragged out of the command pane and
    whose gxserver row has not finished moving from the `commands` surface to
    `workspace`. Sidebar-driven tab reconciliation deletes Agents sessions that
    are absent from the sidebar projection, and a `commands`-surface row is
    absent by definition, so these ids are held out of that prune until the
    daemon confirms the move. Runtime-only: never persisted, and cleared on
    success or after the bounded retry budget so a stuck update cannot pin a
    session out of reconciliation forever.
    */
    pub(crate) agents_sessions_pending_surface_transfer: HashSet<TerminalSessionId>,
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-15:24:
    Command Close After Done deadlines are runtime-only countdowns derived from armed command sessions that are currently done. Store only command session ids, deadlines, and cancellation generations here; the persisted shell state carries only the safe armed boolean.
    */
    pub(crate) command_close_after_done_timers:
        HashMap<CommandSessionId, GpuiCommandCloseAfterDoneTimer>,
    pub(crate) command_close_after_done_generation: u64,
    pub(crate) command_close_after_done_countdown_ticker_active: bool,
    /*
    CDXC:GPUISettingsGxserverAgentPolicy 2026-06-24-12:14:
    Startup and Settings-open hydration may both ask local gxserver for canonical agent policy. Keep only a runtime in-flight guard here so a missing daemon row is not seeded twice concurrently; the persisted values still live only in gxserver plus the central shared Settings render cache.
    */
    pub(crate) gxserver_agent_settings_reconciliation_in_flight: bool,
    pub(crate) workspace_drop_feedback: Option<WorkspaceDropFeedback>,
    pub(crate) command_drop_feedback: Option<CommandPaneDropFeedback>,
    pub(crate) workspace_tab_drag_active: bool,
    pub(crate) pending_workspace_tab_click: Option<WorkspacePendingTabClick>,
    pub(crate) command_tab_drag_active: bool,
    pub(crate) pending_command_tab_click: Option<CommandPanePendingTabClick>,
    pub(crate) browser_tab_drop_feedback: Option<BrowserDropFeedback>,
    pub(crate) browser_tab_drag_active: bool,
    pub(crate) workspace_leaf_layout_bounds: HashMap<WorkspacePaneId, Bounds<Pixels>>,
    pub(crate) browser_leaf_layout_bounds: HashMap<BrowserPaneId, Bounds<Pixels>>,
    pub(crate) command_group_layout_bounds: HashMap<CommandPaneGroupId, Bounds<Pixels>>,
    pub(crate) command_pane_layout_bounds: Option<Bounds<Pixels>>,
    pub(crate) project_editor_surface_layout_bounds: Option<ProjectEditorFocusBounds>,
    pub(crate) project_editor_companion_layout_bounds: Option<ProjectEditorFocusBounds>,
    pub(crate) agents_terminal_mount_slot_bounds:
        HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    pub(crate) command_terminal_mount_slot_bounds:
        HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    pub(crate) project_editor_companion_terminal_session_id: Option<TerminalSessionId>,
    pub(crate) project_editor_companion_secondary_terminal_session_id: Option<TerminalSessionId>,
    pub(crate) project_editor_companion_focused_terminal_slot: ProjectEditorCompanionTerminalSlot,
    pub(crate) project_editor_companion_terminal_mount_slot_bounds:
        HashMap<ProjectEditorCompanionTerminalBodyMountSlotId, Bounds<Pixels>>,
    /*
    CDXC:GPUIZmxPersistenceRefresh 2026-07-06:
    Runtime-only zmx conditional-refresh triggers mirroring macOS
    `scheduleZmxPersistenceTerminalRefreshAfterResize` and the focus-changed
    refresh path: a trailing-edge resize debounce generation and the last
    focused terminal slot identity. Both stay out of shell-state JSON and logs.
    */
    pub(crate) zmx_persistence_resize_refresh_generation: u64,
    pub(crate) zmx_persistence_last_focused_terminal_slot:
        Option<ZmxPersistenceFocusedTerminalSlot>,
    /*
    CDXC:GPUIZmxPersistenceRefresh 2026-07-11:
    The mount-slot bounds maps above are per-render measurement state and are
    cleared at every render start, so they cannot answer "was this slot
    already surfaced with these bounds?". Using them for that (as the bounds
    hooks originally did) made every frame look like a first surfacing: one
    zmx refresh-if-stale subprocess per visible terminal slot per frame, and
    a dead resize-debounce branch. These sibling maps persist across renders
    for exactly that question; they are pruned at render start against the
    currently rendered slots so a slot that leaves and returns still gets its
    surfaced refresh. Runtime-only, never serialized or logged.
    */
    // One-shot guard for the delayed sidebar-surface creation retry
    // (CDXC:GPUICefBrowserCreateFallible 2026-07-11).
    pub(crate) cef_sidebar_creation_retried: bool,
    pub(crate) cef_context_initialization_waiting: bool,
    pub(crate) agents_terminal_zmx_refresh_recorded_bounds:
        HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    pub(crate) command_terminal_zmx_refresh_recorded_bounds:
        HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    pub(crate) project_editor_companion_zmx_refresh_recorded_bounds:
        HashMap<ProjectEditorCompanionTerminalBodyMountSlotId, Bounds<Pixels>>,
    /*
    CDXC:GPUITerminalTextInput 2026-06-23-10:45:
    Terminal IME/preedit ownership uses one app-level GPUI focus handle that is focused only through mounted terminal body focus paths. The text-service state may remember only runtime slot identity and sanitized UTF-16 marked ranges, never raw typed text, preedit text, terminal content, paths, commands, output, URLs, titles, tokens, cookies, or secrets.
    */
    pub(crate) terminal_text_focus_handle: FocusHandle,
    pub(crate) terminal_text_marked_range: Option<TerminalTextMarkedRange>,
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-27-14:59:
    Sidebar-created GPUI terminals and agents must become type-ready like macOS sidebar launches. Track one exact Agents mount slot until its real Ghostty surface exists, then focus the existing GPUI terminal text service from the mounted body instead of adding keyboard fallbacks, overlays, or input rerouting.
    */
    pub(crate) pending_agents_terminal_text_focus_slot: Option<AgentsTerminalBodyMountSlotId>,
    /*
    CDXC:GPUICommandTerminalFocus 2026-07-04:
    Command-pane terminal creation and wake paths must become type-ready after
    the rendered terminal body actually exists. Track one exact command mount
    slot until either the GPUI-rendered terminal entity or native text handler
    is ready, then focus that terminal without adding broad key routing,
    overlays, fallback focused surfaces, or persistent shell fields.
    */
    pub(crate) pending_command_terminal_text_focus_slot: Option<CommandTerminalBodyMountSlotId>,
    pub(crate) pending_project_editor_companion_terminal_text_focus_slot:
        Option<ProjectEditorCompanionTerminalBodyMountSlotId>,
    pub(crate) agents_terminal_startup_body_slot_geometries:
        HashMap<AgentsTerminalStartupBodySlotId, AgentsTerminalStartupBodyGeometry>,
    pub(crate) agents_terminal_parked_owner_body_slot_geometries:
        HashMap<AgentsTerminalBodyMountSlotId, AgentsTerminalParkedOwnerBodyGeometry>,
    pub(crate) agents_terminal_runtime_sessions: AgentsTerminalRuntimeSessionRegistry,
    pub(crate) agents_terminal_startup_coordinator: AgentsTerminalStartupCoordinator,
    pub(crate) agents_terminal_startup_launch_payload_source:
        AgentsTerminalStartupLaunchPayloadSource,
    pub(crate) agents_terminal_launch_payload_source: AgentsTerminalLaunchPayloadSource,
    pub(crate) command_terminal_launch_payload_source: CommandTerminalLaunchPayloadSource,
    pub(crate) project_editor_companion_terminal_launch_payload_source:
        ProjectEditorCompanionTerminalLaunchPayloadSource,
    pub(crate) project_editor_companion_terminal_attach_plan_pending:
        HashSet<ProjectEditorCompanionTerminalBodyMountSlotId>,
    pub(crate) project_editor_companion_remote_attach_states: HashMap<
        ProjectEditorCompanionTerminalBodyMountSlotId,
        GpuiProjectEditorCompanionRemoteAttachState,
    >,
    pub(crate) agents_terminal_surface_host: NativeTerminalSurfaceHost,
    pub(crate) agents_terminal_surface_lifecycle: NativeTerminalSurfaceLifecycleState,
    pub(crate) command_terminal_surface_host:
        NativeTerminalSurfaceHost<CommandTerminalBodyMountSlotId>,
    pub(crate) command_terminal_surface_lifecycle:
        NativeTerminalSurfaceLifecycleState<CommandTerminalBodyMountSlotId>,
    pub(crate) project_editor_companion_terminal_surface_host:
        NativeTerminalSurfaceHost<ProjectEditorCompanionTerminalBodyMountSlotId>,
    pub(crate) project_editor_companion_terminal_surface_lifecycle:
        NativeTerminalSurfaceLifecycleState<ProjectEditorCompanionTerminalBodyMountSlotId>,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_ghostty_surfaces:
        HashMap<AgentsTerminalBodyMountSlotId, terminal_ghostty_surface::GhosttySurfaceOwner>,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_parked_runtime_owners:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalParkedRuntimeOwner>,
    #[cfg(target_os = "macos")]
    pub(crate) command_terminal_ghostty_surfaces: HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) project_editor_companion_terminal_ghostty_surfaces: HashMap<
        ProjectEditorCompanionTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<
            ProjectEditorCompanionTerminalBodyMountSlotId,
        >,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) command_terminal_parked_runtime_owners:
        HashMap<AgentsTerminalRuntimeSessionId, CommandTerminalParkedRuntimeOwner>,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_close_confirms: AgentsTerminalCloseConfirmState,
    #[cfg(target_os = "macos")]
    pub(crate) command_terminal_close_confirms: CommandTerminalCloseConfirmState,
    #[cfg(target_os = "macos")]
    pub(crate) terminal_close_confirm_dialog_key: Option<TerminalCloseConfirmDialogKey>,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_startup_ghostty_surfaces: HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_ghostty_app: Option<terminal_ghostty_surface::GhosttyAppOwner>,
    #[cfg(target_os = "macos")]
    pub(crate) command_terminal_ghostty_app: Option<terminal_ghostty_surface::GhosttyAppOwner>,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_host_native_views: HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) command_terminal_host_native_views: HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) project_editor_companion_terminal_host_native_views: HashMap<
        ProjectEditorCompanionTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView<
            ProjectEditorCompanionTerminalBodyMountSlotId,
        >,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_startup_host_native_views: HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_appkit_focused_host:
        Option<terminal_native_view::AppOwnedTerminalHostFocusIdentity>,
    #[cfg(target_os = "macos")]
    pub(crate) command_terminal_appkit_focused_host: Option<
        terminal_native_view::AppOwnedTerminalHostFocusIdentity<CommandTerminalBodyMountSlotId>,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) project_editor_companion_terminal_appkit_focused_host: Option<
        terminal_native_view::AppOwnedTerminalHostFocusIdentity<
            ProjectEditorCompanionTerminalBodyMountSlotId,
        >,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_ghostty_surface_config_requests: HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) command_terminal_ghostty_surface_config_requests: HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) project_editor_companion_terminal_ghostty_surface_config_requests: HashMap<
        ProjectEditorCompanionTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    #[cfg(target_os = "macos")]
    pub(crate) agents_terminal_startup_ghostty_surface_config_requests: HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    pub(crate) workspace_tab_scroll_handles: HashMap<WorkspacePaneId, ScrollHandle>,
    pub(crate) browser_tab_scroll_handles: HashMap<BrowserPaneId, ScrollHandle>,
    pub(crate) command_tab_scroll_handles: HashMap<CommandPaneGroupId, ScrollHandle>,
    pub(crate) command_collapsed_tab_scroll_handle: ScrollHandle,
    pub(crate) workspace_split_layout_metrics: HashMap<WorkspaceSplitId, SplitResizeMetrics>,
    pub(crate) command_split_layout_metrics: HashMap<CommandPaneSplitId, SplitResizeMetrics>,
    pub(crate) browser_split_layout_metrics: HashMap<BrowserSplitId, SplitResizeMetrics>,
    pub(crate) project_editor_companion_layout_metrics: Option<SplitResizeMetrics>,
    pub(crate) project_editor_companion_split_layout_metrics: Option<SplitResizeMetrics>,
    pub(crate) workspace_split_drag: Option<WorkspaceSplitResizeDragState>,
    pub(crate) workspace_split_hovering: Option<WorkspaceSplitId>,
    pub(crate) workspace_split_hover_visible: Option<WorkspaceSplitId>,
    pub(crate) workspace_split_hover_epoch: u64,
    pub(crate) command_split_drag: Option<CommandPaneSplitResizeDragState>,
    pub(crate) browser_split_drag: Option<BrowserSplitResizeDragState>,
    pub(crate) project_editor_companion_drag: Option<ProjectEditorCompanionResizeDragState>,
    pub(crate) project_editor_companion_split_drag:
        Option<ProjectEditorCompanionSplitResizeDragState>,
    pub(crate) project_editor_companion_divider_hovering: Option<TitlebarMode>,
    pub(crate) project_editor_companion_divider_hover_visible: Option<TitlebarMode>,
    pub(crate) project_editor_companion_divider_hover_epoch: u64,
    pub(crate) project_editor_companion_split_divider_hovering: Option<TitlebarMode>,
    pub(crate) project_editor_companion_split_divider_hover_visible: Option<TitlebarMode>,
    pub(crate) project_editor_companion_split_divider_hover_epoch: u64,
    pub(crate) hovered_workspace_tab: Option<WorkspaceHoverTab>,
    pub(crate) hovered_command_tab: Option<CommandPaneHoverTab>,
    pub(crate) hovered_browser_tab: Option<BrowserHoverTab>,
    pub(crate) command_resize_hovering: Option<CommandPaneResizeHoverTarget>,
    pub(crate) command_resize_hover_visible: Option<CommandPaneResizeHoverTarget>,
    pub(crate) command_resize_hover_epoch: u64,
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-11:17:
    Worker 53 keeps Pet Overlay interaction state inside the existing visible GPUI stack. The avatar's expanded/collapsed activity-card boolean is persisted with shell state to match native restart behavior, but the only stored pet UI data is this boolean; do not store activity titles, session/project ids, paths, settings JSON, commands, URLs, terminal output, tokens, detached panel origin, or drag state.

    CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
    GPUI Pet Overlay Reduce Motion is a runtime-only macOS accessibility display option. When enabled, keep the avatar on a stable semantic state frame and do not start the pet animation ticker; unsupported/non-macOS reads default to animated behavior without persisting or logging system settings.
    */
    pub(crate) gpui_pet_overlay_activities_visible: bool,
    pub(crate) gpui_pet_overlay_avatar_hovered: bool,
    pub(crate) gpui_pet_overlay_animation_state: GpuiPetOverlayAnimationState,
    pub(crate) gpui_pet_overlay_animation_started_at: Instant,
    pub(crate) gpui_pet_overlay_animation_ticker_active: bool,
    pub(crate) gpui_pet_overlay_reduce_motion_enabled: bool,
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    Sidebar side is placement-only shell state sourced from shared Settings. Keep it independent from `sidebar_width` and `sidebar_collapsed` so Move Sidebar can mirror native without resizing, expanding, or hiding the sidebar.
    */
    pub(crate) sidebar_side: GpuiSidebarSide,
    pub(crate) command_pane_side: GpuiCommandPaneSide,
    pub(crate) sidebar_width: f32,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) sidebar_drag: Option<SidebarDragState>,
    pub(crate) sidebar_divider_hovering: bool,
    pub(crate) sidebar_divider_hover_visible: bool,
    pub(crate) sidebar_divider_hover_epoch: u64,
    pub(crate) app_modal_window: Option<WindowHandle<GpuiAppModalHostWindow>>,
    pub(crate) app_modal_window_id: Rc<Cell<Option<gpui::WindowId>>>,
    pub(crate) app_modal_open_attempt_id: u64,
    pub(crate) app_modal_ready_retry_used: bool,
    pub(crate) app_modal_command_return_focus_target: Option<CommandPaneAppModalReturnFocusTarget>,
    pub(crate) plugins_modal_window: Option<WindowHandle<plugins_modal::GpuiPluginsModalWindow>>,
    pub(crate) plugin_settings_action_progress:
        HashMap<&'static str, component_store::ComponentStoreProgressPhase>,
    pub(crate) plugin_settings_action_errors: HashMap<&'static str, String>,
    pub(crate) plugin_settings_action_generations: HashMap<&'static str, u64>,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    pub(crate) cef_component_window:
        Option<WindowHandle<cef_component_window::GpuiCefComponentWindow>>,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    pub(crate) cef_component_install_generation: u64,
    /*
    CDXC:GPUIMainWindowToasts 2026-08-18:
    The last-known frame and display of the main workspace window, refreshed
    from this entity's render pass and main-window bounds observer. Child
    window placement must never read the ambient
    `&mut Window` of whatever context happens to deliver the toast: gpui
    resolves `WeakEntity::update_in` through `current_window_by_entity`, which
    points at the last window that drew after this entity was touched. Any
    child popup (titlebar dropdown, app modal, or the toast panel itself) can
    win that race, which put toasts on top of the wrong window and walked the
    stack up the screen one toast at a time.
    */
    pub(crate) main_window_bounds: Bounds<Pixels>,
    pub(crate) main_window_display_id: Option<gpui::DisplayId>,
    pub(crate) app_toast_window: Option<WindowHandle<GpuiAppToastWindow>>,
    pub(crate) app_toast_window_height: Pixels,
    pub(crate) app_toast_anchor: Option<Point<Pixels>>,
    pub(crate) app_toasts: Vec<GpuiAppToast>,
    pub(crate) app_toast_epoch: u64,
    pub(crate) app_toast_id_counter: u64,
    pub(crate) agents_terminal_runtime_osc_states:
        HashMap<AgentsTerminalRuntimeSessionId, GpuiTerminalRuntimeOscState>,
    pub(crate) command_terminal_runtime_osc_states:
        HashMap<AgentsTerminalRuntimeSessionId, GpuiTerminalRuntimeOscState>,
    /*
    CDXC:GPUITerminalGpuiEngine 2026-07-04:
    Runtime-only GPUI-engine terminal views keyed by shell session identity.
    On every OS, sessions are claimed exactly when their queued launch payload
    is consumed. Agents, command-pane, companion, restored, and newly launched
    terminals render the composited TerminalElement in the same body slot.
    GhosttyKit owners remain available in the macOS build but are never selected
    while this engine is enabled. Records are never persisted.
    */
    pub(crate) agents_gpui_engine_terminals:
        HashMap<TerminalSessionId, terminal_gpui_engine::GpuiEngineTerminalRecord>,
    pub(crate) command_gpui_engine_terminals:
        HashMap<CommandSessionId, terminal_gpui_engine::GpuiEngineTerminalRecord>,
    /// Pending close confirmations for GPUI-engine slots. Kept separate from
    /// the native Ghostty close-confirm state machines because engine
    /// liveness is checked at close-request time instead of via runtime
    /// close callbacks.
    pub(crate) agents_gpui_engine_close_confirms: HashSet<AgentsTerminalBodyMountSlotId>,
    pub(crate) command_gpui_engine_close_confirms: HashSet<CommandTerminalBodyMountSlotId>,
    pub(crate) pending_terminal_paste_confirmation: Option<PendingGpuiTerminalPasteConfirmation>,
    pub(crate) terminal_paste_confirmation_dialog_open: bool,
    /*
    CDXC:GPUITerminalGpuiEngineDiagnostics 2026-07-04-12:40:
    Runtime-only last-logged grid per live command engine terminal, so the
    gated `native.terminal.focus` diagnostic can record spawn and every
    applied cols/rows change without per-frame log spam. A command engine
    terminal whose grid stays at a degenerate size (for example rows == 1
    while the pinned panel is expanded) renders as a visually blank body, and
    this breadcrumb is the decisive signal separating "element never rendered"
    from "element rendered with a collapsed body rectangle". Never persisted;
    numeric ids and cell counts only.
    */
    pub(crate) command_gpui_engine_grid_log_states: HashMap<CommandSessionId, (u16, u16)>,
    pub(crate) terminal_search_inputs: HashMap<AgentsTerminalRuntimeSessionId, Entity<InputState>>,
    pub(crate) terminal_search_input_subscriptions:
        HashMap<AgentsTerminalRuntimeSessionId, gpui::Subscription>,
    pub(crate) terminal_search_focus_pending: Option<AgentsTerminalRuntimeSessionId>,
    /// Live Delayed Send timers for mounted Agents terminals. Shell state keeps
    /// only stable project/session identity plus a bounded remaining-time or
    /// status-trigger checkpoint; deadlines, generations, and mount owners stay
    /// process-local.
    pub(crate) agents_delayed_send_timers: HashMap<TerminalSessionId, GpuiCommandDelayedSendTimer>,
    pub(crate) agents_send_when_stopped_watchers:
        HashMap<TerminalSessionId, GpuiAgentsSendWhenStoppedWatcher>,
    pub(crate) agents_delayed_send_generation: u64,
    pub(crate) agents_delayed_send_countdown_ticker_active: bool,
    pub(crate) agents_delayed_send_persistence_ticker_active: bool,
    /*
    CDXC:GPUITitlebarTips 2026-06-24-23:17:
    The titlebar Tips dropdown owns a runtime-only React titlebar-host CEF panel inside an app-owned anchored GPUI overlay positioned directly below TITLEBAR_HEIGHT. Store only the panel entity, open boolean, and transient focus handoff state so closing the overlay can hide the native CEF child view; do not duplicate tips data, persist dropdown state, create AppKit child windows, or rely on invisible overlays.
    */
    pub(crate) titlebar_dropdown_focus_handle: FocusHandle,
    pub(crate) titlebar_dropdown_previous_focus_handle: Option<FocusHandle>,
    pub(crate) titlebar_popup_menu: Option<GpuiTitlebarPopupState>,
    pub(crate) titlebar_popup_window: Option<WindowHandle<GpuiTitlebarPopupWindow>>,
    pub(crate) titlebar_extension_popup_generation: u64,
    pub(crate) titlebar_extension_popup: Option<GpuiTitlebarExtensionPopupState>,
    pub(crate) titlebar_tips_panel_open: bool,
    pub(crate) titlebar_tips_panel: Option<Entity<GpuiTitlebarTipsPanel>>,
    pub(crate) titlebar_resources_panel_open: bool,
    pub(crate) titlebar_resources_panel_ready: bool,
    pub(crate) titlebar_resources_panel_open_generation: u64,
    pub(crate) titlebar_resources_panel: Option<Entity<GpuiTitlebarResourcesPanel>>,
    /*
    CDXC:GPUIResourcesDevServers 2026-07-26:
    Resources attributes a localhost listener to a project by matching the
    listener's cwd against the project paths in the resource groups GPUI sends.
    Sending only the active project's mounted panes hid every dev server started
    from another project or from a session this window has not mounted, so the
    panel also receives project groups projected from the gxserver presentation
    snapshot. Cache only that projection; the snapshot itself is refetched on
    each Resources open.
    */
    pub(crate) titlebar_resources_presentation_groups: Vec<serde_json::Value>,
    pub(crate) titlebar_tips_cli_status: Option<serde_json::Value>,
    pub(crate) titlebar_tips_agent_hook_status: Option<serde_json::Value>,
    pub(crate) agent_hook_status_request_in_flight: bool,
    pub(crate) sidebar: Option<Entity<CefSurface>>,
    pub(crate) browser_surfaces: HashMap<BrowserTabId, Entity<CefSurface>>,
    pub(crate) browser_address_inputs: HashMap<BrowserPaneId, Entity<InputState>>,
    pub(crate) browser_address_input_subscriptions: HashMap<BrowserPaneId, gpui::Subscription>,
    pub(crate) browser_address_input_editing: HashSet<BrowserPaneId>,
    pub(crate) browser_find_states: HashMap<BrowserTabId, GpuiBrowserFindState>,
    pub(crate) browser_media_permission_prompts:
        HashMap<BrowserTabId, VecDeque<GpuiBrowserMediaPermissionPrompt>>,
    pub(crate) browser_media_permission_decisions: GpuiBrowserMediaPermissionDecisions,
    pub(crate) browser_find_inputs: HashMap<BrowserTabId, Entity<InputState>>,
    pub(crate) browser_find_input_subscriptions: HashMap<BrowserTabId, gpui::Subscription>,
    pub(crate) pending_browser_find_focus: Option<BrowserTabId>,
    pub(crate) pending_browser_address_focus: Option<BrowserPaneId>,
    pub(crate) pending_browser_content_focus: Option<BrowserPaneId>,
}

impl Drop for GhostexGpuiApp {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        unregister_gpui_app_shots_callback_target();
        #[cfg(target_os = "macos")]
        unregister_gpui_menu_bar_status_callback_target();
        #[cfg(target_os = "macos")]
        unregister_gpui_sidebar_pointer_callback_target();
        #[cfg(target_os = "macos")]
        unregister_gpui_session_attention_notification_callback_target();
        #[cfg(target_os = "macos")]
        unregister_gpui_accessibility_display_options_callback_target();
        #[cfg(target_os = "macos")]
        unregister_gpui_workspace_power_events_callback_target();
        #[cfg(target_os = "macos")]
        unregister_gpui_sparkle_updater_callback_target();
        #[cfg(target_os = "macos")]
        unregister_gpui_os_integration_callback_target();
        #[cfg(target_os = "macos")]
        unregister_gpui_first_responder_callback_target(self.parent_ns_view);
        #[cfg(target_os = "macos")]
        unregister_gpui_keyboard_router_target(self.parent_ns_view);
        #[cfg(target_os = "macos")]
        unregister_gpui_terminal_key_event_callback_target(self.parent_ns_view);
        hide_gpui_menu_bar_status_item();
        self.source_code_server_runtime.stop();
        self.stop_gpui_keep_awake_runtime();
        self.stop_all_gpui_remote_gxserver_connections();
    }
}

impl EntityInputHandler for GhostexGpuiApp {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<String> {
        /*
        CDXC:GPUITerminalTextInput 2026-06-23-10:45:
        Terminal text services must not expose terminal document content back to GPUI/AppKit. IME queries may learn only sanitized selection/marked ranges and candidate bounds; raw terminal text, commands, paths, stdout/stderr, URLs, titles, tokens, cookies, and secrets stay behind the Ghostty surface boundary.
        */
        let _ = range;
        *adjusted_range = None;
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<UTF16Selection> {
        self.terminal_text_service_accepts_text_input(window)
            .then(|| UTF16Selection {
                range: 0..0,
                reversed: false,
            })
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<Range<usize>> {
        if !self.terminal_text_service_accepts_text_input(window) {
            return None;
        }
        let target = self.exact_focused_terminal_text_surface_target()?;
        self.terminal_text_marked_range
            .as_ref()
            .and_then(|marked_range| {
                (marked_range.target == target).then(|| marked_range.range.clone())
            })
    }

    fn unmark_text(&mut self, window: &mut Window, _cx: &mut gpui::Context<Self>) {
        if let Some(marked_target) = self
            .terminal_text_marked_range
            .as_ref()
            .map(|marked_range| marked_range.target)
        {
            let _ = self.set_preedit_on_terminal_text_target(marked_target, b"");
            self.terminal_text_marked_range = None;
            return;
        }

        if self.terminal_text_focus_handle.is_focused(window) {
            self.clear_focused_terminal_preedit();
        }
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.terminal_text_focus_handle.is_focused(window) {
            if let Some(marked_target) = self
                .terminal_text_marked_range
                .as_ref()
                .map(|marked_range| marked_range.target)
            {
                let _ = self.set_preedit_on_terminal_text_target(marked_target, b"");
            }
            self.terminal_text_marked_range = None;
            return;
        }

        /*
        CDXC:GPUITerminalTextInput 2026-06-23-10:52:
        IME commit must clear Ghostty preedit before forwarding final committed text, matching terminal text-service behavior without retaining or logging raw composition content.
        */
        if let Some(marked_target) = self
            .terminal_text_marked_range
            .as_ref()
            .map(|marked_range| marked_range.target)
        {
            let _ = self.set_preedit_on_terminal_text_target(marked_target, b"");
            self.terminal_text_marked_range = None;
        } else {
            let _ = self.set_preedit_on_focused_terminal_surface(b"");
        }
        if !text.is_empty() {
            let _ = self.send_text_to_focused_terminal_surface(text, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) {
        if !self.terminal_text_focus_handle.is_focused(window) {
            if let Some(marked_target) = self
                .terminal_text_marked_range
                .as_ref()
                .map(|marked_range| marked_range.target)
            {
                let _ = self.set_preedit_on_terminal_text_target(marked_target, b"");
            }
            self.terminal_text_marked_range = None;
            return;
        }

        let Some(target) = self.exact_focused_terminal_text_surface_target() else {
            self.terminal_text_marked_range = None;
            return;
        };
        if self.set_preedit_on_terminal_text_target(target, new_text.as_bytes()) {
            self.terminal_text_marked_range =
                terminal_text_marked_range_for_preedit(range, new_text)
                    .map(|range| TerminalTextMarkedRange { target, range });
        } else {
            self.terminal_text_marked_range = None;
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let _ = range_utf16;
        if !self.terminal_text_service_accepts_text_input(window) {
            return None;
        }
        self.bounds_for_focused_terminal_ime_point(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<usize> {
        let _ = point;
        None
    }

    fn accepts_text_input(&self, window: &mut Window, _cx: &mut gpui::Context<Self>) -> bool {
        self.terminal_text_service_accepts_text_input(window)
    }
}

impl Render for GhostexGpuiApp {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Only the main workspace window renders this entity, so this keeps
        // the authoritative frame and display every child popup anchors against.
        self.main_window_bounds = window.bounds();
        self.main_window_display_id = window.display(cx).map(|display| display.id());
        #[cfg(target_os = "windows")]
        if self.windows_first_run_setup_state != GpuiWindowsFirstRunSetupState::Ready {
            return self.render_windows_first_run_setup(cx);
        }
        /*
        CDXC:GPUICefShellLayout 2026-06-14-12:06:
        GPUI must prove the macOS sidebar React UI and normal browser surfaces can run as CEF children inside the shell. Keep the CEF child views as exact GPUI layout siblings, with the address-bar chrome owned by GPUI above only the main browser area, so future Linux and Windows backends can replace the macOS FFI without changing the app layout contract.

        CDXC:GPUISidebarChrome 2026-06-21-18:34:
        The GPUI sidebar must match native macOS sidebar resizing: start from the persisted native sidebarWidth, reserve a real five-pixel divider rail between the sidebar and browser siblings, clamp drag/reset width to 150px..520px while preserving a 240px workspace minimum, and use the Settings-owned sidebarDefaultWidthPx only for double-click reset.

        CDXC:GPUISidebarChrome 2026-06-21-22:17:
        The GPUI divider rail must behave like the native sidebar divider without AppKit dependencies: the rail stays a real GPUI sibling, keeps the sidebar background color, uses the ew-resize cursor over the rail, and reveals the white hover line after the same short delay/fade from pointer hover instead of requiring a click.

        CDXC:GPUIWorkspaceLayout 2026-06-22-13:36:
        The main workspace column must own the full height available below the titlebar. The body row top-aligns its full-height workspace column and uses a black shell background so GPUI's h_flex center alignment or any late child surface cannot expose a white window fill above or below the workspace.

        CDXC:GPUISidebarSide 2026-06-26-23:35:
        Sidebar side parity is implemented as normal sibling order, never overlays or hit-test rerouting: expanded left is sidebar/divider/workspace, expanded right is workspace/divider/sidebar, and collapsed mode removes sidebar/divider while preserving the saved expanded width.
        */
        self.sidebar_width =
            clamp_sidebar_width(self.sidebar_width, current_sidebar_max_width(window));
        self.refresh_gpui_sidebar_browser_tabs_if_changed(cx);
        self.refresh_gpui_sidebar_displayed_sessions_if_changed(cx);
        self.prepare_focus_bounds_for_render(window.scale_factor(), cx);
        #[cfg(target_os = "macos")]
        self.sync_terminal_close_confirm_dialog(window, cx);
        self.sync_terminal_paste_confirmation_dialog(window, cx);
        self.sync_terminal_search_inputs(window, cx);
        self.drain_pending_gpui_engine_terminal_focus(window, cx);
        self.drain_pending_session_chat_composer_focus_handoff(window, cx);
        self.refresh_zmx_persistence_focused_terminal_if_changed(cx);
        let sidebar_chrome_visible = gpui_sidebar_chrome_visible(self.sidebar_collapsed);
        let sidebar_on_left = self.sidebar_side == GpuiSidebarSide::Left;
        let titlebar_popup_dismissal_active =
            self.titlebar_popup_menu.is_some() || self.titlebar_extension_popup.is_some();

        let content = v_flex()
            .relative()
            .size_full()
            .bg(workspace_background_color())
            .when(titlebar_popup_dismissal_active, |this| {
                /*
                Native titlebar dropdowns live in non-activating panels, while
                extension popups are anchored in this main window. Close either
                popup on a main-window mouse-down outside both its content and
                trigger button (a mouse-down on the trigger itself is left to
                the button's own toggle handler), and put the dropdown key
                context on the root so the existing
                Escape -> TitlebarDropdownCancel binding dispatches from
                wherever focus currently is.
                */
                this.key_context(TITLEBAR_DROPDOWN_KEY_CONTEXT)
                    .capture_any_mouse_down(cx.listener(
                        |app, event: &MouseDownEvent, window, cx| {
                            let outside_popup_menu_trigger =
                                app.titlebar_popup_menu.as_ref().is_some_and(|state| {
                                    !state.trigger_bounds.contains(&event.position)
                                });
                            let outside_extension_trigger =
                                app.titlebar_extension_popup.as_ref().is_some_and(|state| {
                                    !state.trigger_bounds.contains(&event.position)
                                });
                            let outside_extension_popup = app
                                .titlebar_extension_popup_bounds(window)
                                .is_some_and(|bounds| !bounds.contains(&event.position));
                            let popup_kind = app
                                .titlebar_popup_menu
                                .as_ref()
                                .map(|state| state.kind.diagnostic_label())
                                .or_else(|| {
                                    app.titlebar_extension_popup
                                        .as_ref()
                                        .map(|_| "extension")
                                });
                            let trigger_bounds = app
                                .titlebar_popup_menu
                                .as_ref()
                                .map(|state| {
                                    gpui_titlebar_popup_bounds_diagnostic(Some(
                                        state.trigger_bounds,
                                    ))
                                })
                                .or_else(|| {
                                    app.titlebar_extension_popup.as_ref().map(|state| {
                                        gpui_titlebar_popup_bounds_diagnostic(Some(
                                            state.trigger_bounds,
                                        ))
                                    })
                                });
                            log_gpui_titlebar_popup_repro(
                                "gpui.titlebarPopup.mainWindowMouseCapture",
                                serde_json::json!({
                                    "kind": popup_kind,
                                    "mainWindowActive": window.is_window_active(),
                                    "outsideTrigger": outside_popup_menu_trigger || outside_extension_trigger,
                                    "pointerX": event.position.x.as_f32(),
                                    "pointerY": event.position.y.as_f32(),
                                    "triggerBounds": trigger_bounds,
                                }),
                            );
                            if outside_popup_menu_trigger {
                                app.close_gpui_titlebar_popup(None, window, cx);
                            } else if outside_extension_trigger && outside_extension_popup {
                                app.close_titlebar_extension_popup(window, cx);
                            }
                        },
                    ))
            })
            .when(
                self.sidebar_drag.is_some()
                    || self.command_split_drag.is_some()
                    || self
                        .browser_split_drag
                        .is_some_and(|drag| drag.axis == WorkspaceSplitAxis::Horizontal)
                    || self.project_editor_companion_drag.is_some()
                    || self
                        .workspace_split_drag
                        .is_some_and(|drag| drag.axis == WorkspaceSplitAxis::Horizontal),
                |this| this.cursor_ew_resize(),
            )
            .when(
                self.command_pane.resize_drag.is_some()
                    || self
                        .browser_split_drag
                        .is_some_and(|drag| drag.axis == WorkspaceSplitAxis::Vertical)
                    || self
                        .workspace_split_drag
                        .is_some_and(|drag| drag.axis == WorkspaceSplitAxis::Vertical),
                |this| this.cursor_ns_resize(),
            )
            /*
            CDXC:GPUITerminalNativeKeyBridge 2026-07-10:
            Root committed-text forwarding is only for GPUI-composited engine terminals. Native libghostty surfaces own keyDown and NSTextInputClient directly on their exact AppKit host NSView; routing their ordinary text through this GPUI listener would recreate the competing committed-text-only path that loses physical keys and modifiers.
            CDXC:GPUICommandSleepingPlaceholder 2026-06-25-14:49:
            Focused sleeping command placeholders consume plain alphanumeric key-downs to wake before terminal text delivery. This matches native "Press Any Key to Wake" behavior without forwarding the wake key to Ghostty or creating a broad keyboard fallback for non-terminal surfaces.
            CDXC:GPUITerminalSearchFocus 2026-07-04-08:20:
            Root key-down forwarding derives its terminal target from app-level shell focus, which intentionally stays on the terminal pane while the Cmd+F search bar is open. If a terminal search input holds GPUI keyboard focus, forwarding here would write every typed character into the focused terminal PTY and consume the event, so macOS never runs the insertText path that feeds the focused input. Keyboard focus on a search input therefore ends root terminal key forwarding (and placeholder wake) for the keystroke; the search input's own dispatch path owns it.
            */
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.terminal_search_input_owns_keyboard_focus(window, cx)
                    || this.browser_find_input_owns_keyboard_focus(window, cx)
                {
                    return;
                }
                if this
                    .wake_focused_sleeping_command_placeholder_from_keystroke(&event.keystroke, cx)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if this
                    .wake_focused_sleeping_agents_placeholder_from_keystroke(&event.keystroke, cx)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if this.focused_gpui_engine_terminal_view().is_none() {
                    return;
                }
                let Some(text) = committed_terminal_text_from_key_down_event(event) else {
                    return;
                };
                if this.send_text_to_focused_terminal_surface(text, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenCommandPane, window, cx| {
                this.open_command_pane_from_keyboard(window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &RunConfiguredGhostexHotkey, window, cx| {
                    if this.propagate_source_workarea_cef_configured_hotkey_passthrough(
                        action.action_id.as_str(),
                        cx,
                    ) {
                        return;
                    }
                    this.handle_gpui_app_modal_sidebar_command(
                        serde_json::json!({
                            "message": {
                                "actionId": action.action_id,
                                "type": "runGhostexHotkeyAction",
                            },
                        }),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &PasteIntoFocusedTerminal, _window, cx| {
                    if this.propagate_renderer_edit_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    if !this.paste_into_focused_terminal_from_clipboard(cx) {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &FindInFocusedTerminal, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                if this.start_find_in_focused_browser(window, cx) {
                    return;
                }
                let _ = this.start_search_in_focused_terminal_surface(cx);
            }))
            .on_action(
                cx.listener(|this, _: &FindNextInFocusedBrowser, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    let ShellFocusTarget::BrowserPane(pane_id) = this.shell_focus else {
                        cx.propagate();
                        return;
                    };
                    let Some(tab_id) = this.browser_tabs.active_tab_id_for_pane(pane_id) else {
                        cx.propagate();
                        return;
                    };
                    if !this.perform_browser_find_navigation(tab_id, true, cx) {
                        cx.propagate();
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &FindPreviousInFocusedBrowser, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    let ShellFocusTarget::BrowserPane(pane_id) = this.shell_focus else {
                        cx.propagate();
                        return;
                    };
                    let Some(tab_id) = this.browser_tabs.active_tab_id_for_pane(pane_id) else {
                        cx.propagate();
                        return;
                    };
                    if !this.perform_browser_find_navigation(tab_id, false, cx) {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &ZoomInFocusedSurface, _window, cx| {
                if !this.perform_focused_surface_zoom(GpuiFocusedSurfaceZoomCommand::In, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ZoomOutFocusedSurface, _window, cx| {
                if !this.perform_focused_surface_zoom(GpuiFocusedSurfaceZoomCommand::Out, cx) {
                    cx.propagate();
                }
            }))
            .on_action(
                cx.listener(|this, _: &ResetFocusedSurfaceZoom, _window, cx| {
                    if !this.perform_focused_surface_zoom(GpuiFocusedSurfaceZoomCommand::Reset, cx)
                    {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &TitlebarDropdownCancel, window, cx| {
                if this.titlebar_popup_menu.is_some() {
                    this.close_gpui_titlebar_popup(None, window, cx);
                } else if this.titlebar_extension_popup.is_some() {
                    this.close_titlebar_extension_popup(window, cx);
                } else if this.titlebar_resources_panel_open {
                    this.set_gpui_titlebar_resources_panel_open(false, window, cx);
                } else if this.titlebar_tips_panel_open {
                    this.set_gpui_titlebar_tips_panel_open(false, window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(
                cx.listener(|this, _: &SleepInactiveSessionsFromTitlebar, _window, cx| {
                    let _ = this.dispatch_gpui_workspace_sleep_inactive_sessions(cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &StartGpuiGxserverFromTitlebar, _window, cx| {
                    this.start_gpui_local_gxserver_bootstrap(cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &StopGpuiGxserverFromTitlebar, _window, cx| {
                    this.stop_gpui_local_gxserver_from_titlebar(false, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &RestartGpuiGxserverFromTitlebar, _window, cx| {
                    this.stop_gpui_local_gxserver_from_titlebar(true, cx);
                }),
            )
            .on_action(cx.listener(
                |this, _: &OpenGpuiPortlessSetupModalFromTitlebar, window, cx| {
                    if GPUI_PORTLESS_APP_INTEGRATION_ENABLED {
                        this.open_gpui_app_modal_from_titlebar(
                            GpuiAppModalKind::PortlessSetup,
                            window,
                            cx,
                        );
                    }
                },
            ))
            .on_action(cx.listener(|this, _: &CycleFocusedTabForward, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.cycle_focused_tab(false, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &CycleFocusedTabBackward, window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    this.cycle_focused_tab(true, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &CloseFocusedSurface, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.close_focused_surface(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &CloseFocusedSurfaceMenuOnly, window, cx| {
                    this.close_focused_surface(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &ToggleGpuiSidebarCollapsed, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    this.toggle_gpui_sidebar_collapsed(cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &ToggleProjectEditorCompanion, window, cx| {
                    this.toggle_project_editor_companion_from_hotkey(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SleepFocusedSession, _window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                /*
                CDXC:GPUICommandFocusedSessionActions 2026-06-25-14:56:
                Option+Shift+S sleeps whichever terminal session owns shell focus. Command tabs use their native sleep mutation; Agents and companion sessions use the same scoped lifecycle request as their tab action.
                */
                if !this.sleep_focused_command_pane_session(cx)
                    && let Some(session_id) = this.focused_agents_or_companion_shell_session_id()
                    && let Some(pane_id) = this.agents_workspace.pane_id_for_session(session_id)
                {
                    let _ = this.sleep_agents_tabs_for_scope(
                        pane_id,
                        session_id,
                        AgentsWorkspaceTabSleepScope::Sleep,
                        cx,
                    );
                }
            }))
            .on_action(cx.listener(|this, _: &WakeFocusedSession, _window, cx| {
                /*
                CDXC:GPUICommandFocusedSessionActions 2026-06-25-15:01:
                Wake Focused Session has no default key, but the shared command palette and Hotkeys UI expose it. Keep the GPUI action unbound by default and route it only through the command-pane focused sleeping tab wake path.
                */
                this.wake_focused_command_pane_session(cx);
            }))
            .on_action(cx.listener(|this, _: &NewTerminalTab, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.add_terminal_placeholder_tab_from_hotkey(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SplitFocusedTerminalRight, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    this.split_focused_terminal_from_hotkey(
                        FocusedTerminalSplitDirection::Right,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &SplitFocusedTerminalDown, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    this.split_focused_terminal_from_hotkey(
                        FocusedTerminalSplitDirection::Down,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, _: &NewBrowserTab, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.add_browser_tab_from_hotkey(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleAgentsFocusMode, _window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.toggle_agents_focus_mode(cx);
            }))
            .on_action(cx.listener(|this, _: &MergeAllTabs, _window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.merge_all_agents_tabs_from_hotkey(cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchAgentsWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Agents, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchSourceWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchBrowserWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Browser, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchKanbanWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Kanban, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchManageWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenGpuiHotkeysModal, window, cx| {
                this.open_gpui_app_modal_from_titlebar(GpuiAppModalKind::Hotkeys, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &OpenGpuiConfigureAgentsModal, window, cx| {
                    this.open_gpui_app_modal_from_titlebar(
                        GpuiAppModalKind::ConfigureAgents,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiConfigureActionsModal, window, cx| {
                    this.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiOpenTargetsModal, window, cx| {
                    this.open_gpui_app_modal_from_titlebar(
                        GpuiAppModalKind::OpenTargets,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &OpenGpuiWorkspaceInTarget, window, cx| {
                    this.open_active_project_with_open_target_index(
                        action.target_index as usize,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &SelectGpuiTitlebarMode, window, cx| {
                    this.select_titlebar_mode_from_menu(action.mode_index, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarAction, window, cx| {
                    this.run_gpui_titlebar_action_index(action.action_index as usize, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarGitMenuAction, _window, cx| {
                    this.run_gpui_titlebar_git_menu_row(action.row_index as usize, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &CopyGpuiTitlebarGitBranch, _window, cx| {
                    let Some(branch) = this
                        .titlebar_git_menu_state
                        .as_ref()
                        .and_then(|state| state.branch.clone())
                    else {
                        return;
                    };
                    cx.write_to_clipboard(ClipboardItem::new_string(branch));
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiTitlebarGitCommitScreen, _window, cx| {
                    this.dispatch_gpui_titlebar_git_action_selector(
                        GpuiTitlebarGitMenuActionId::Commit.selector(),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &RunGpuiTitlebarGitRemoteSync, _window, cx| {
                    this.dispatch_gpui_titlebar_git_action_selector(
                        GpuiTitlebarGitMenuActionId::SyncRemote.selector(),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &ConfigureGpuiTitlebarActions, window, cx| {
                    this.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &StartGpuiKeepAwakePeriod, window, cx| {
                    let Some(duration_minutes) =
                        shared_settings::SharedKeepAwakeDurationMinutes::from_minutes(
                            action.duration_minutes,
                        )
                    else {
                        return;
                    };
                    this.start_gpui_keep_awake_period(duration_minutes, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &StopGpuiKeepAwake, _window, cx| {
                this.stop_gpui_keep_awake_from_titlebar(cx);
            }))
            .on_action(
                cx.listener(|this, _: &OpenGpuiPowerSettingsModal, window, cx| {
                    this.open_gpui_power_settings_modal_from_titlebar(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SleepGpuiPetOverlay, _window, cx| {
                this.sleep_gpui_pet_overlay_from_context_menu(cx);
            }))
            .on_action(
                cx.listener(|this, _: &GoToGhostexFromGpuiPetOverlay, window, cx| {
                    this.go_to_ghostex_from_gpui_pet_overlay(window, cx);
                }),
            )
            .on_action(cx.listener(|_this, _: &GpuiKeepAwakeMenuLabel, _window, _cx| {}))
            .on_action(
                cx.listener(|this, _: &OpenGpuiCommandPaletteModal, window, cx| {
                    this.open_gpui_app_modal_from_titlebar(
                        GpuiAppModalKind::CommandPalette,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiCustomizeSettingsModal, window, cx| {
                    /*
                    CDXC:GPUITitlebarCustomize 2026-08-13:
                    NativeMenu dispatches through the main window's rendered
                    action tree. Handle Customize on that tree, alongside the
                    other titlebar menu actions, so right-click selection opens
                    the explicit Settings > Customize route instead of relying
                    on an app-global fallback that this window dispatch may
                    never reach.
                    */
                    this.open_gpui_settings_plugins_page(Some(window), cx);
                }),
            )
            .on_action(cx.listener(|this, _: &OpenGpuiPluginsModal, window, cx| {
                this.open_gpui_settings_plugins_page(Some(window), cx);
            }))
            .on_action(
                cx.listener(|this, _: &OpenGpuiPreviousSessionsModal, window, cx| {
                    this.open_gpui_app_modal_from_titlebar(
                        GpuiAppModalKind::PreviousSessions,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, _: &OpenGpuiAgentsHubModal, window, cx| {
                this.open_gpui_app_modal_from_titlebar(GpuiAppModalKind::AgentsHub, window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &NewTerminalTabInPane, _window, cx| {
                    this.add_agents_registered_terminal_tab(WorkspacePaneId(action.pane_id), cx);
                }),
            )
            .on_action(cx.listener(
                |this, action: &SplitPaneRightWithNewTerminal, _window, cx| {
                    this.split_agents_registered_terminal_right(
                        WorkspacePaneId(action.pane_id),
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(
                |this, action: &SplitPaneBelowWithNewTerminal, _window, cx| {
                    this.split_agents_registered_terminal_below(
                        WorkspacePaneId(action.pane_id),
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &RotateAgentsPanesForPane, _window, cx| {
                    this.rotate_agents_panes_for_pane(WorkspacePaneId(action.pane_id), cx);
                }),
            )
            .on_action(cx.listener(
                |this, action: &AppendFullWidthTerminalRowForPane, _window, cx| {
                    this.append_agents_registered_terminal_bottom_row(
                        WorkspacePaneId(action.pane_id),
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &MergeAllTabsForPane, _window, cx| {
                    this.merge_all_agents_tabs_for_pane(WorkspacePaneId(action.pane_id), cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &ToggleFocusModeForPane, _window, cx| {
                    this.toggle_agents_focus_mode_for_pane(WorkspacePaneId(action.pane_id), cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &SelectAgentsWorkspaceTab, _window, cx| {
                    this.select_agents_tab_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &CloseAgentsWorkspaceTab, _window, cx| {
                    this.close_agents_tab_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &CloseAgentsWorkspaceTabsByScope, _window, cx| {
                    this.close_agents_tabs_for_scope_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        action.scope,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(
                |this, action: &SleepAgentsWorkspaceTabsByScope, _window, cx| {
                    this.sleep_agents_tabs_for_scope_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        action.scope,
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &FocusAgentsWorkspaceTab, _window, cx| {
                    this.toggle_agents_focus_mode_for_tab_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &ForkAgentsWorkspaceTab, _window, cx| {
                    let _ = this.dispatch_gpui_workspace_terminal_runtime_action(
                        "forkSession",
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &ReloadAgentsWorkspaceTab, _window, cx| {
                    let _ = this.dispatch_gpui_workspace_terminal_runtime_action(
                        "fullReloadSession",
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &OpenBrowserPaneInExternalBrowser, _window, _cx| {
                    this.open_browser_pane_in_external_browser(BrowserPaneId(action.pane_id));
                },
            ))
            .on_action(
                cx.listener(|this, action: &NewBrowserTabInPane, window, cx| {
                    this.add_browser_tab_in_pane_from_action(
                        BrowserPaneId(action.pane_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &SplitBrowserPaneRightWithBrowserTab, window, cx| {
                    this.split_browser_pane_with_new_tab_from_action(
                        BrowserPaneId(action.pane_id),
                        WorkspaceDropZone::Right,
                        window,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(
                |this, action: &SplitBrowserPaneBelowWithBrowserTab, window, cx| {
                    this.split_browser_pane_with_new_tab_from_action(
                        BrowserPaneId(action.pane_id),
                        WorkspaceDropZone::Bottom,
                        window,
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &SelectBrowserTabInPane, window, cx| {
                    this.select_browser_tab_from_action(
                        BrowserPaneId(action.pane_id),
                        BrowserTabId(action.tab_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &CloseBrowserTabInPane, window, cx| {
                    this.close_browser_tab_from_action(
                        BrowserPaneId(action.pane_id),
                        BrowserTabId(action.tab_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &OpenBrowserHistoryEntryInNewTab, window, cx| {
                    let Ok(index) = usize::try_from(action.index) else {
                        return;
                    };
                    this.open_browser_history_entry_in_new_tab(
                        BrowserPaneId(action.pane_id),
                        index,
                        window,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(|this, _: &RunBrowserFeedbackTool, window, cx| {
                this.run_browser_feedback_tool_from_toolbar(
                    this.browser_tabs.focused_pane,
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &ResetBrowserZoom, window, cx| {
                this.reset_browser_zoom_from_toolbar(this.browser_tabs.focused_pane, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleBrowserDevTools, window, cx| {
                this.toggle_browser_devtools_from_toolbar(
                    this.browser_tabs.focused_pane,
                    window,
                    cx,
                );
            }))
            .on_action(
                cx.listener(|this, action: &SelectBrowserProfile, window, cx| {
                    this.select_browser_profile_from_menu(
                        BrowserPaneId(action.pane_id),
                        BrowserProfileId(action.profile_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &CreateBrowserProfile, window, cx| {
                    this.create_browser_profile_from_menu(
                        BrowserPaneId(action.pane_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &CloseCommandPaneTabsByScope, _window, cx| {
                    this.close_command_pane_tabs_for_scope_from_action(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        action.scope,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &SleepCommandPaneTabsByScope, _window, cx| {
                    this.sleep_command_pane_tabs_for_scope_from_action(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        action.scope,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &RenameCommandPaneTab, _window, cx| {
                    this.open_gpui_rename_session_modal_for_command_pane_tab(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &DelayedSendCommandPaneTab, window, cx| {
                    this.open_gpui_delayed_send_modal_for_command_pane_tab(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &ToggleCloseAfterDoneCommandPaneTab, _window, cx| {
                    this.toggle_gpui_command_close_after_done_for_command_pane_tab(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &FocusCommandPaneTab, _window, cx| {
                    this.toggle_command_pane_focus_mode_for_tab(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, _: &FocusWorkspaceLeft, window, cx| {
                this.focus_workspace_direction(WorkspaceFocusDirection::Left, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusWorkspaceUp, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.focus_workspace_direction(WorkspaceFocusDirection::Up, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusWorkspaceRight, window, cx| {
                this.focus_workspace_direction(WorkspaceFocusDirection::Right, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusWorkspaceDown, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.focus_workspace_direction(WorkspaceFocusDirection::Down, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.handle_sidebar_root_mouse_move(event, window, cx);
                this.handle_command_pane_resize_drag_move(event, window, cx);
                this.handle_workspace_split_resize_drag_move(event, window, cx);
                this.handle_command_split_resize_drag_move(event, window, cx);
                this.handle_browser_split_resize_drag_move(event, window, cx);
                this.handle_project_editor_companion_resize_drag_move(event, window, cx);
                this.handle_project_editor_companion_split_resize_drag_move(event, window, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.handle_sidebar_drag_mouse_up(event, window, cx);
                    this.handle_command_pane_resize_mouse_up(event, window, cx);
                    this.handle_workspace_split_resize_mouse_up(event, window, cx);
                    this.handle_command_split_resize_mouse_up(event, window, cx);
                    this.handle_browser_split_resize_mouse_up(event, window, cx);
                    this.handle_project_editor_companion_resize_mouse_up(event, window, cx);
                    this.handle_project_editor_companion_split_resize_mouse_up(event, window, cx);
                    this.finish_workspace_tab_drag(cx);
                    this.finish_command_tab_drag(cx);
                    this.finish_browser_tab_drag(cx);
                }),
            )
            .child(self.render_titlebar(window, cx))
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .items_start()
                    .overflow_hidden()
                    .bg(workspace_background_color())
                    .when(sidebar_chrome_visible && sidebar_on_left, |this| {
                        this.child(
                            /*
                            CDXC:GPUISidebarCollapse 2026-06-26-10:04:
                            Sidebar collapse is real layout ownership in GPUI: the sidebar CEF child and divider are removed as body-row siblings instead of being covered, overlapped, or resized to zero. Keep `sidebar_width` untouched so expand restores the previous user width.
                            */
                            div()
                                .w(px(self.sidebar_width))
                                .h_full()
                                .pl(px(SIDEBAR_LEFT_INSET))
                                .when_some(self.sidebar.clone(), |this, sidebar| {
                                    this.child(sidebar)
                                }),
                        )
                    })
                    .when(sidebar_chrome_visible && sidebar_on_left, |this| {
                        this.child(self.render_sidebar_resize_divider(cx))
                    })
                    .child(
                        v_flex()
                            .id("ghostex-gpui-workspace-column")
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .bg(workspace_background_color())
                            .child(self.render_workspace_with_command_pane(window, cx)),
                    )
                    .when(sidebar_chrome_visible && !sidebar_on_left, |this| {
                        this.child(self.render_sidebar_resize_divider(cx))
                    })
                    .when(sidebar_chrome_visible && !sidebar_on_left, |this| {
                        this.child(
                            div()
                                .w(px(self.sidebar_width))
                                .h_full()
                                .pl(px(SIDEBAR_LEFT_INSET))
                                .when_some(self.sidebar.clone(), |this, sidebar| {
                                    this.child(sidebar)
                                }),
                        )
                    }),
            )
            .child(self.render_gpui_status_pet_presentation(cx))
            .into_any_element();

        #[cfg(target_os = "linux")]
        let content = gpui_linux_client_window_frame(content, window);

        content
    }
}
