//! The panel a click on the button (or Cmd/Alt+Ctrl+Shift+S) opens: the four capture actions over
//! the same Running Agents list the macOS menu bar dropdown shows.
//!
//! CDXC:GhostexCapture 2026-09-30 DECISION:
//! User: clicking the button shows "the same dropdown we show from the status bar in macos (below
//! it or above it depending on where this floating icon is shown)" plus four actions, named "Capture
//! Area / Capture App / Capture Screen / Write Prompt" (A, Space, F, T), Write Prompt with a chat
//! icon.
//! Idle sessions follow the menu bar rule (only if active in the past two hours, projects with
//! working or waiting sessions first), and clicking a session opens it in Ghostex.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable as _, KeyDownEvent, MouseButton,
    SharedString, Subscription, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, div, px, rgb,
};
use gpui_component::input::{Input, InputEvent, InputState};

use super::glass::{PanelPalette, panel_background_appearance};
use super::machine_sections::{MachineSection, PanelMachine, split_by_machine};
use super::model::*;
use super::persistence;
use super::placement;
use super::platform;
use super::project_list::{ListState, MachineShown, Shown};
use crate::GhostexGpuiApp;
use crate::app::helpers::{GpuiStatusIndicatorSessionState, GpuiStatusIndicatorStatus};

/// `GhostexGpuiMenuBarStatusIdleSessionMaximumAge`.
const IDLE_SESSION_MAX_AGE_MS: i64 = 2 * 60 * 60 * 1000;

const PANEL_PAD: f32 = 10.0;
const ACTIONS_HEIGHT: f32 = 80.0;
const SEPARATOR_HEIGHT: f32 = 21.0;
const HEADING_HEIGHT: f32 = 22.0;
const PROJECT_TITLE_HEIGHT: f32 = 28.0;
const SESSION_ROW_HEIGHT: f32 = 30.0;
const PROJECT_GAP: f32 = 6.0;
const FOOTER_HEIGHT: f32 = 30.0;
const EMPTY_HEIGHT: f32 = 44.0;
const FILTER_HEIGHT: f32 = 36.0;
const MACHINE_HEADER_HEIGHT: f32 = 28.0;
/// The list's height, whatever it holds: the panel keeps one size so it never moves while open.
const LIST_HEIGHT: f32 = 280.0;
/// Projects expanded when the panel opens (user decision 2026-10-28, see `project_list.rs`).
const OPEN_PROJECTS: usize = 3;

#[derive(Clone)]
pub(crate) struct PanelSession {
    pub(crate) session_id: String,
    pub(crate) title: String,
    pub(crate) status: GpuiStatusIndicatorStatus,
    pub(crate) question: bool,
    pub(crate) trailing: String,
}

#[derive(Clone)]
pub(crate) struct PanelProject {
    pub(crate) project_id: String,
    pub(crate) title: String,
    pub(crate) sessions: Vec<PanelSession>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn parse_ms(value: Option<&str>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value?)
        .ok()
        .map(|time| time.timestamp_millis())
}

fn relative_time(value: Option<&str>) -> String {
    let Some(at) = parse_ms(value) else {
        return String::new();
    };
    let minutes = ((now_ms() - at).max(0) / 60_000) as u64;
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

fn is_waiting(session: &GpuiStatusIndicatorSessionState) -> bool {
    session.pending_question
        || matches!(
            session.status,
            GpuiStatusIndicatorStatus::Working | GpuiStatusIndicatorStatus::Attention
        )
}

impl GhostexGpuiApp {
    /// The Running Agents list: `-visibleProjects` of the menu bar dropdown.
    pub(crate) fn ghostex_capture_panel_projects(&self) -> Vec<PanelProject> {
        let now = now_ms();
        let mut active = Vec::new();
        let mut idle = Vec::new();
        for project in &self.sidebar_session_status_indicators.projects {
            let mut sessions: Vec<&GpuiStatusIndicatorSessionState> = project
                .sessions
                .iter()
                .filter(|session| {
                    is_waiting(session)
                        || parse_ms(session.last_active_at.as_deref())
                            .is_some_and(|at| now - at <= IDLE_SESSION_MAX_AGE_MS)
                })
                .collect();
            if sessions.is_empty() {
                continue;
            }
            sessions.sort_by_key(|session| session.order);
            let has_active = sessions.iter().any(|session| is_waiting(session));
            let entry = PanelProject {
                project_id: project.project_id.clone(),
                title: project.title.clone(),
                sessions: sessions
                    .into_iter()
                    .map(|session| PanelSession {
                        session_id: session.session_id.clone(),
                        title: session.title.clone(),
                        status: session.status,
                        question: session.pending_question,
                        trailing: if session.pending_question {
                            "asking".into()
                        } else if session.status == GpuiStatusIndicatorStatus::Working {
                            String::new()
                        } else {
                            relative_time(session.last_active_at.as_deref())
                        },
                    })
                    .collect(),
            };
            if has_active {
                active.push(entry);
            } else {
                idle.push(entry);
            }
        }
        active.extend(idle);
        active
    }

    /// Every remote machine the sidebar has a tab for, in its order, with its connection state.
    pub(crate) fn ghostex_capture_panel_machines(&self) -> Vec<PanelMachine> {
        self.gx_store
            .remote
            .tabs()
            .iter()
            .filter(|tab| !tab.is_local())
            .map(|tab| {
                let (status, detail) = ghostex_gx_core::machine_state_copy(&tab.state);
                PanelMachine {
                    machine_id: tab.machine_id.clone(),
                    name: tab.label.clone(),
                    connected: tab.is_connected(),
                    busy: ghostex_gx_core::machine_state_is_busy(&tab.state),
                    failed: ghostex_gx_core::machine_state_is_failure(&tab.state),
                    status: status.to_string(),
                    detail: tab
                        .message
                        .clone()
                        .filter(|message| !message.trim().is_empty())
                        .or(detail.map(str::to_string)),
                }
            })
            .collect()
    }

    fn ghostex_capture_panel_height(&self) -> f32 {
        PANEL_PAD * 2.0
            + ACTIONS_HEIGHT
            + SEPARATOR_HEIGHT
            + HEADING_HEIGHT
            + FILTER_HEIGHT
            + LIST_HEIGHT
            + SEPARATOR_HEIGHT
            + FOOTER_HEIGHT
            + 2.0
    }

    /// Where the panel opens: where the user last moved it, else next to the button.
    fn ghostex_capture_panel_target(&self, cx: &App) -> Option<gpui::Bounds<gpui::Pixels>> {
        let height = self.ghostex_capture_panel_height();
        if let Some(frame) = self
            .ghostex_capture
            .saved
            .panel_position
            .as_ref()
            .and_then(|saved| {
                placement::restore_frame(saved, gpui::size(px(PANEL_WIDTH), px(height)), cx)
            })
        {
            return Some(frame);
        }
        let icon = self.ghostex_capture.icon.as_ref()?;
        let screen = placement::screen_at(icon.frame.center(), cx)
            .or_else(|| self.ghostex_capture_home(cx).map(|(screen, _)| screen))?;
        Some(placement::panel_frame(&screen, icon.frame, height))
    }

    pub(super) fn open_ghostex_capture_panel(&mut self, cx: &mut Context<Self>) {
        if self.ghostex_capture.panel.is_some() || self.ghostex_capture.panel_opening {
            return;
        }
        // A docked button slides out first so the panel lines up with the icon.
        self.ghostex_capture.panel_opening = true;
        self.sync_ghostex_capture_icon(cx);
        let Some(frame) = self.ghostex_capture_panel_target(cx) else {
            self.ghostex_capture.panel_opening = false;
            return;
        };
        let app = cx.weak_entity();
        App::defer(cx, move |cx| {
            let observed = app.clone();
            let (bounds, display_id) = crate::app::window::popup_frame::place_global(frame, cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    titlebar: None,
                    focus: true,
                    show: true,
                    kind: if cfg!(target_os = "linux") {
                        crate::app::window::popup_frame::child_window_kind()
                    } else {
                        WindowKind::PopUp
                    },
                    window_decorations: crate::app::window::popup_frame::child_window_decorations(),
                    // Dragged by the empty part of its Running Agents heading.
                    is_movable: true,
                    is_resizable: false,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: panel_background_appearance(),
                    ..Default::default()
                },
                move |window, cx| {
                    // The panel closes once it loses focus, so changing the Glass settings (in
                    // Settings) always reopens it with the new ones.
                    let glass =
                        panel_background_appearance() == WindowBackgroundAppearance::Blurred;
                    crate::app::window::popup_frame::round_popup_window(window, px(14.0), glass);
                    crate::app::helpers::apply_frosted_menu_blur(window);
                    let focus = cx.focus_handle();
                    let filter = cx.new(|cx| {
                        InputState::new(window, cx).placeholder("Filter projects and sessions")
                    });
                    let view = cx.new(|cx| {
                        let observe = match observed.upgrade() {
                            Some(app) => cx.observe(&app, |_, _, cx| cx.notify()),
                            None => Subscription::new(|| {}),
                        };
                        let activation = cx.observe_window_activation(
                            window,
                            |view: &mut CapturePanelView, window, cx| {
                                if !window.is_window_active() {
                                    view.with_app(cx, |app, cx| {
                                        app.close_ghostex_capture_panel(cx)
                                    });
                                }
                            },
                        );
                        let filtered = cx.subscribe(
                            &filter,
                            |view: &mut CapturePanelView, input, event: &InputEvent, cx| {
                                if matches!(event, InputEvent::Change) {
                                    view.list.filter = input.read(cx).value().to_string();
                                    cx.notify();
                                }
                            },
                        );
                        CapturePanelView {
                            app: observed.clone(),
                            focus: focus.clone(),
                            filter: filter.clone(),
                            list: ListState::panel(OPEN_PROJECTS),
                            _observe: observe,
                            _activation: activation,
                            _filtered: filtered,
                        }
                    });
                    focus.focus(window, cx);
                    // Text fields need gpui-component's Root in their window.
                    cx.new(|cx| {
                        gpui_component::Root::new(view, window, cx).bg(gpui::transparent_black())
                    })
                },
            );
            let Some(app) = app.upgrade() else {
                return;
            };
            let Ok(handle) = result else {
                app.update(cx, |app, cx| {
                    app.ghostex_capture.panel_opening = false;
                    app.sync_ghostex_capture_icon(cx);
                });
                return;
            };
            let native = handle
                .update(cx, |_, window, _| {
                    crate::app::helpers::cef_parent_native_view(window)
                        .ok()
                        .map(|view| view as usize)
                })
                .ok()
                .flatten();
            if let Some(native) = native {
                platform::prepare_floating_window(native, true);
            }
            app.update(cx, |app, cx| {
                app.ghostex_capture.panel_opening = false;
                app.ghostex_capture.panel = Some(PanelWindow {
                    handle,
                    native,
                    opened_at: frame,
                });
                cx.notify();
            });
        });
    }

    pub(crate) fn close_ghostex_capture_panel(&mut self, cx: &mut Context<Self>) {
        self.remember_ghostex_capture_panel_position(cx);
        let Some(panel) = self.ghostex_capture.panel.take() else {
            return;
        };
        let handle = panel.handle;
        App::defer(cx, move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
        self.sync_ghostex_capture_icon(cx);
        cx.notify();
    }

    /// Saves where the panel is once the user moved it, so it opens there from then on.
    fn remember_ghostex_capture_panel_position(&mut self, cx: &mut Context<Self>) {
        let Some((handle, native, opened_at)) = self
            .ghostex_capture
            .panel
            .as_ref()
            .map(|panel| (panel.handle, panel.native, panel.opened_at))
        else {
            return;
        };
        let Some(frame) = placement::live_frame(handle, native, cx) else {
            return;
        };
        let moved = (frame.origin.x - opened_at.origin.x).abs() > px(1.0)
            || (frame.origin.y - opened_at.origin.y).abs() > px(1.0);
        if !moved {
            return;
        }
        if let Some(saved) = placement::save_frame(frame, false, cx) {
            self.ghostex_capture.saved.panel_position = Some(saved);
            persistence::save(&self.ghostex_capture.saved);
        }
    }

    /// Keeps the open panel next to the button after the button moved.
    pub(super) fn sync_ghostex_capture_panel_frame(&mut self, cx: &mut Context<Self>) {
        if self.ghostex_capture.panel.is_none() {
            return;
        }
        // The panel is rebuilt where it belongs; it is cheap and avoids a second geometry path.
        self.close_ghostex_capture_panel(cx);
        self.open_ghostex_capture_panel(cx);
    }

    fn ghostex_capture_connect_machine(&mut self, machine_id: String, cx: &mut Context<Self>) {
        self.remote_reconnect_from_sidebar(&machine_id, cx);
    }

    fn ghostex_capture_open_session(
        &mut self,
        project_id: String,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        self.close_ghostex_capture_panel(cx);
        self.dispatch_gpui_menu_bar_session_activation(&project_id, &session_id, cx);
        self.ghostex_capture_bring_ghostex_forward(cx);
    }

    /// Opens a session in Ghostex, the way a click on its sidebar row does.
    pub(super) fn open_ghostex_capture_session(
        &mut self,
        session: &ghostex_gx_core::SessionKey,
        cx: &mut Context<Self>,
    ) {
        self.gx_store_focus_activated_session(&session.to_sidebar_session_id(), cx);
        self.ghostex_capture_bring_ghostex_forward(cx);
    }

    pub(super) fn ghostex_capture_bring_ghostex_forward(&mut self, cx: &mut Context<Self>) {
        let main = self.main_window_handle;
        App::defer(cx, move |cx| {
            cx.activate(true);
            if let Some(main) = main {
                let _ = main.update(cx, |_, window, _| window.activate_window());
            }
        });
    }
}

pub(crate) struct CapturePanelView {
    app: WeakEntity<GhostexGpuiApp>,
    focus: FocusHandle,
    filter: Entity<InputState>,
    list: ListState,
    _observe: Subscription,
    _activation: Subscription,
    _filtered: Subscription,
}

impl CapturePanelView {
    fn with_app(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut GhostexGpuiApp, &mut Context<GhostexGpuiApp>),
    ) {
        if let Some(app) = self.app.upgrade() {
            app.update(cx, f);
        }
    }

    fn run(&self, action: CaptureAction, cx: &mut Context<Self>) {
        self.with_app(cx, move |app, cx| {
            app.run_ghostex_capture_action(action, cx)
        });
    }
}

fn status_dot(session: &PanelSession, palette: &PanelPalette) -> AnyElement {
    let (color, square): (gpui::Hsla, bool) = if session.question {
        (rgb(0xf472b6).into(), false)
    } else {
        match session.status {
            GpuiStatusIndicatorStatus::Working => (rgb(0xc68a06).into(), true),
            GpuiStatusIndicatorStatus::Attention => (rgb(0x0093fe).into(), false),
            GpuiStatusIndicatorStatus::Available => (palette.idle_dot, false),
        }
    };
    let dot = div().flex_none().bg(color);
    if square {
        dot.size(px(8.0)).rounded(px(1.0)).into_any_element()
    } else {
        dot.size(px(9.0)).rounded_full().into_any_element()
    }
}

/// A project's row: a chevron, its name, and how many of its sessions are working, waiting for
/// the user, or asking a question.
fn project_header(
    id: SharedString,
    title: &str,
    sessions: &[PanelSession],
    open: bool,
    palette: &PanelPalette,
) -> gpui::Stateful<gpui::Div> {
    let question = sessions.iter().filter(|session| session.question).count();
    let count = |status: GpuiStatusIndicatorStatus| {
        sessions
            .iter()
            .filter(|session| !session.question && session.status == status)
            .count()
    };
    let badges = [
        (count(GpuiStatusIndicatorStatus::Attention), 0x0093fe),
        (question, 0xf472b6),
        (count(GpuiStatusIndicatorStatus::Working), 0xc68a06),
    ];
    div()
        .id(id)
        .h(px(PROJECT_TITLE_HEIGHT))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(8.0))
        .hover(|style| style.bg(palette.row_hover))
        .cursor_pointer()
        .child(
            gpui::svg()
                .path(if open {
                    "titlebar/chevron-down.svg"
                } else {
                    "titlebar/chevron-right.svg"
                })
                .size(px(12.0))
                .text_color(palette.muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(palette.title)
                .child(SharedString::from(title.to_string())),
        )
        .children(
            badges
                .into_iter()
                .filter(|(value, _)| *value > 0)
                .map(|(value, color)| {
                    div()
                        .text_size(px(11.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(color))
                        .child(value.to_string())
                }),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(palette.faint)
                .child(sessions.len().to_string()),
        )
}

/// A remote machine's row: a chevron, a cloud, its name, and either the same counts as a project
/// header (summed over its projects) or, while it is not connected, what its connection says and
/// a Connect button.
fn machine_header(
    machine: &PanelMachine,
    projects: &[PanelProject],
    open: bool,
    palette: &PanelPalette,
) -> gpui::Stateful<gpui::Div> {
    let sessions: Vec<&PanelSession> = projects
        .iter()
        .flat_map(|project| project.sessions.iter())
        .collect();
    let question = sessions.iter().filter(|session| session.question).count();
    let count = |status: GpuiStatusIndicatorStatus| {
        sessions
            .iter()
            .filter(|session| !session.question && session.status == status)
            .count()
    };
    let badges = [
        (count(GpuiStatusIndicatorStatus::Attention), 0x0093fe),
        (question, 0xf472b6),
        (count(GpuiStatusIndicatorStatus::Working), 0xc68a06),
    ];
    let palette = *palette;
    let row = div()
        .id(SharedString::from(format!(
            "ghostex-capture-machine-{}",
            machine.machine_id
        )))
        .h(px(MACHINE_HEADER_HEIGHT))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(8.0))
        .hover(|style| style.bg(palette.row_hover))
        .cursor_pointer()
        .child(
            gpui::svg()
                .path(if open {
                    "titlebar/chevron-down.svg"
                } else {
                    "titlebar/chevron-right.svg"
                })
                .size(px(12.0))
                .text_color(palette.muted),
        )
        .child(
            gpui::svg()
                .path("titlebar/cloud.svg")
                .size(px(13.0))
                .text_color(if machine.failed {
                    rgb(0xe5767a).into()
                } else if machine.connected {
                    palette.muted
                } else {
                    palette.faint
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(if machine.connected {
                    palette.title
                } else {
                    palette.offline
                })
                .child(SharedString::from(machine.name.clone())),
        );
    if !machine.connected {
        return row.child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_size(px(11.5))
                .text_color(palette.faint)
                .child(SharedString::from(machine.status.clone())),
        );
    }
    row.children(
        badges
            .into_iter()
            .filter(|(value, _)| *value > 0)
            .map(|(value, color)| {
                div()
                    .text_size(px(11.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .child(value.to_string())
            }),
    )
    .child(
        div()
            .text_size(px(11.5))
            .text_color(palette.faint)
            .child(sessions.len().to_string()),
    )
}

/// A quiet line inside a machine's section.
fn machine_note(text: String, palette: &PanelPalette) -> AnyElement {
    div()
        .min_h(px(SESSION_ROW_HEIGHT))
        .px(px(12.0))
        .py(px(6.0))
        .flex()
        .items_center()
        .text_size(px(12.0))
        .text_color(palette.faint)
        .child(SharedString::from(text))
        .into_any_element()
}

fn action_button(
    id: &'static str,
    label: &'static str,
    key: &'static str,
    icon: &'static str,
    palette: &PanelPalette,
) -> gpui::Stateful<gpui::Div> {
    let palette = *palette;
    div()
        .id(id)
        // Equal shares of the row, whatever the label: a long label wraps instead of pushing the
        // last button out of the panel.
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(5.0))
        .h(px(ACTIONS_HEIGHT))
        .px(px(4.0))
        .rounded(px(9.0))
        .bg(palette.tile)
        .border_1()
        .border_color(palette.tile_edge)
        .hover(move |style| {
            style
                .bg(palette.tile_hover)
                .border_color(palette.tile_edge_hover)
        })
        .cursor_pointer()
        .child(
            gpui::svg()
                .path(icon)
                .size(px(22.0))
                .text_color(palette.body),
        )
        .child(
            div()
                .w_full()
                .text_size(px(11.5))
                .text_color(palette.strong)
                .text_center()
                .line_height(px(14.0))
                .child(label),
        )
        .child(
            div()
                .text_size(px(10.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(palette.strong)
                .px(px(5.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(palette.key_edge)
                .bg(palette.key)
                .child(key),
        )
}

fn footer_button(
    id: &'static str,
    label: &'static str,
    palette: &PanelPalette,
) -> gpui::Stateful<gpui::Div> {
    let palette = *palette;
    div()
        .id(id)
        .flex_1()
        .h(px(FOOTER_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .bg(palette.button)
        .border_1()
        .border_color(palette.button_edge)
        .text_size(px(12.0))
        .text_color(palette.muted)
        .hover(move |style| style.bg(palette.button_hover).text_color(palette.strong))
        .cursor_pointer()
        .child(label)
}

fn separator(palette: &PanelPalette) -> impl IntoElement {
    div()
        .h(px(SEPARATOR_HEIGHT))
        .flex()
        .items_center()
        .child(div().h(px(1.0)).w_full().bg(palette.hairline))
}

impl CapturePanelView {
    fn render_actions(
        &self,
        has_draft: bool,
        palette: &PanelPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = *palette;
        let mut prompt_tile = action_button(
            "ghostex-capture-prompt",
            "Write Prompt",
            "T",
            "titlebar/message.svg",
            palette,
        )
        .relative()
        .on_click(cx.listener(|this, _, _, cx| this.run(CaptureAction::Prompt, cx)));
        if has_draft {
            prompt_tile = prompt_tile.child(
                div()
                    .id("ghostex-capture-continue-draft")
                    .absolute()
                    .top(px(4.0))
                    .right(px(4.0))
                    .size(px(22.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(accent.accent_fill)
                    .border_1()
                    .border_color(accent.accent_edge)
                    .text_size(px(12.0))
                    .text_color(accent.accent)
                    .hover(move |style| style.bg(accent.accent_fill_hover))
                    .cursor_pointer()
                    .tooltip(|window, cx| {
                        super::tooltip::solid_tooltip("Continue your draft", window, cx)
                    })
                    .child("↩")
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.run(CaptureAction::ContinuePrompt, cx)
                    })),
            );
        }
        div()
            .flex()
            .gap(px(6.0))
            .child(
                action_button(
                    "ghostex-capture-area",
                    "Capture Area",
                    "A",
                    "capture/area.svg",
                    palette,
                )
                .on_click(cx.listener(|this, _, _, cx| this.run(CaptureAction::Area, cx))),
            )
            .child(
                action_button(
                    "ghostex-capture-app",
                    "Capture App",
                    "Space",
                    "capture/app-window.svg",
                    palette,
                )
                .on_click(cx.listener(|this, _, _, cx| this.run(CaptureAction::CurrentApp, cx))),
            )
            .child(
                action_button(
                    "ghostex-capture-full-screen",
                    "Capture Screen",
                    "F",
                    "titlebar/device-desktop.svg",
                    palette,
                )
                .on_click(cx.listener(|this, _, _, cx| this.run(CaptureAction::FullScreen, cx))),
            )
            .child(prompt_tile)
            .into_any_element()
    }

    fn render_session_row(
        &self,
        project_id: String,
        session: PanelSession,
        palette: &PanelPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = palette.row_hover;
        let session_id = session.session_id.clone();
        div()
            .id(SharedString::from(format!(
                "ghostex-capture-session-{}",
                session.session_id
            )))
            .h(px(SESSION_ROW_HEIGHT))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .hover(move |style| style.bg(hover))
            .cursor_pointer()
            .child(status_dot(&session, palette))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0))
                    .text_color(palette.body)
                    .child(SharedString::from(session.title)),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(palette.faint)
                    .child(SharedString::from(session.trailing)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                let (project_id, session_id) = (project_id.clone(), session_id.clone());
                this.with_app(cx, move |app, cx| {
                    app.ghostex_capture_open_session(project_id, session_id, cx)
                });
            }))
            .into_any_element()
    }

    /// A project's card, or `None` when the filter hides it. `all` shows every session whatever
    /// the filter says (its machine's name matched).
    fn render_project_card(
        &self,
        rank: usize,
        project: &PanelProject,
        all: bool,
        palette: &PanelPalette,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        let titles: Vec<&str> = project
            .sessions
            .iter()
            .map(|session| session.title.as_str())
            .collect();
        let open = if all {
            Some((0..project.sessions.len()).collect())
        } else {
            match self
                .list
                .shown(rank, &project.project_id, &project.title, &titles)
            {
                Shown::Hidden => return None,
                Shown::Collapsed => None,
                Shown::Open(indices) => Some(indices),
            }
        };
        let project_id = project.project_id.clone();
        let header = project_header(
            SharedString::from(format!("ghostex-capture-project-{}", project.project_id)),
            &project.title,
            &project.sessions,
            open.is_some(),
            palette,
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            if this.list.toggle(rank, &project_id) {
                this.filter
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
            cx.notify();
        }));
        let mut card = div()
            .flex()
            .flex_col()
            .rounded(px(8.0))
            .border_1()
            .border_color(palette.card_edge)
            .bg(palette.card)
            .child(header);
        for index in open.unwrap_or_default() {
            if let Some(session) = project.sessions.get(index) {
                card = card.child(self.render_session_row(
                    project.project_id.clone(),
                    session.clone(),
                    palette,
                    cx,
                ));
            }
        }
        Some(card)
    }

    /// A remote machine's section: its header, then (open) its project cards, or a line saying
    /// why there are none.
    fn render_machine_section(
        &self,
        section: MachineSection,
        first_rank: usize,
        palette: &PanelPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let button = *palette;
        let MachineSection { machine, projects } = section;
        let filtering = self.list.filtering();
        let cards: Vec<gpui::Div> = projects
            .iter()
            .enumerate()
            .filter_map(|(index, project)| {
                self.render_project_card(first_rank + index, project, false, palette, cx)
            })
            .collect();
        let shown = self.list.machine_shown(
            &machine.machine_id,
            &machine.name,
            machine.connected,
            filtering && !cards.is_empty(),
        );
        let (open, name_matched) = match shown {
            MachineShown::Hidden => return None,
            MachineShown::Collapsed => (false, false),
            MachineShown::Open { name_matched } => (true, name_matched),
        };
        let machine_id = machine.machine_id.clone();
        let connected = machine.connected;
        let mut header = machine_header(&machine, &projects, open, palette).on_click(cx.listener(
            move |this, _, window, cx| {
                if this.list.toggle_machine(&machine_id, connected) {
                    this.filter
                        .update(cx, |input, cx| input.set_value("", window, cx));
                }
                cx.notify();
            },
        ));
        if !machine.connected && !machine.busy {
            let machine_id = machine.machine_id.clone();
            header = header.child(
                div()
                    .id(SharedString::from(format!(
                        "ghostex-capture-machine-connect-{}",
                        machine.machine_id
                    )))
                    .flex_none()
                    .h(px(20.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(6.0))
                    .bg(button.button)
                    .border_1()
                    .border_color(button.button_edge)
                    .text_size(px(11.0))
                    .text_color(button.body)
                    .hover(move |style| style.bg(button.button_hover).text_color(button.strong))
                    .cursor_pointer()
                    .child(if machine.failed {
                        "Reconnect"
                    } else {
                        "Connect"
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let machine_id = machine_id.clone();
                        this.with_app(cx, move |app, cx| {
                            app.ghostex_capture_connect_machine(machine_id, cx)
                        });
                    })),
            );
        }
        let mut column = div().flex().flex_col().gap(px(PROJECT_GAP)).child(header);
        if !open {
            return Some(column.into_any_element());
        }
        if !machine.connected {
            let note = match &machine.detail {
                Some(detail) => format!("{detail} Connect to see its running agents."),
                None if machine.busy => "Its running agents show once it connects.".to_string(),
                None => "Connect to see its running agents.".to_string(),
            };
            return Some(column.child(machine_note(note, palette)).into_any_element());
        }
        if name_matched {
            let all: Vec<gpui::Div> = projects
                .iter()
                .enumerate()
                .filter_map(|(index, project)| {
                    self.render_project_card(first_rank + index, project, true, palette, cx)
                })
                .collect();
            column = column.children(all);
        } else {
            column = column.children(cards);
        }
        if projects.is_empty() {
            column = column.child(machine_note("No running agents".to_string(), palette));
        }
        Some(column.into_any_element())
    }

    fn render_list(
        &self,
        projects: Vec<PanelProject>,
        machines: Vec<PanelMachine>,
        palette: &PanelPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (local, sections) = split_by_machine(projects, &machines);
        let mut rank = local.len();
        let mut items: Vec<AnyElement> = local
            .iter()
            .enumerate()
            .filter_map(|(rank, project)| {
                self.render_project_card(rank, project, false, palette, cx)
            })
            .map(IntoElement::into_any_element)
            .collect();
        let local_shown = !items.is_empty();
        let mut machine_items = Vec::new();
        for section in sections {
            let count = section.projects.len();
            if let Some(element) = self.render_machine_section(section, rank, palette, cx) {
                machine_items.push(element);
            }
            rank += count;
        }
        let list = div()
            .id("ghostex-capture-agents")
            .h(px(LIST_HEIGHT))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(PROJECT_GAP));
        if !local_shown && (machine_items.is_empty() || !self.list.filtering()) {
            // With machines under it, the line names this computer so it does not read as "none
            // anywhere".
            let empty = if self.list.filtering() {
                "No matching projects or sessions"
            } else if machine_items.is_empty() {
                "No running agents"
            } else {
                "No running agents on this computer"
            };
            items.push(
                div()
                    .h(px(EMPTY_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(palette.faint)
                    .child(empty)
                    .into_any_element(),
            );
        }
        items.extend(machine_items);
        list.children(items).into_any_element()
    }

    fn render_filter(&self, palette: &PanelPalette) -> AnyElement {
        div()
            .h(px(FILTER_HEIGHT))
            .flex()
            .items_center()
            .pb(px(6.0))
            .child(
                div()
                    .flex_1()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(9.0))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(palette.field_edge)
                    .bg(palette.field)
                    .child(
                        gpui::svg()
                            .path("titlebar/search.svg")
                            .size(px(13.0))
                            .text_color(palette.faint),
                    )
                    .child(
                        Input::new(&self.filter)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .flex_1()
                            .px(px(0.0))
                            .py(px(0.0))
                            .text_size(px(12.5))
                            .text_color(palette.strong),
                    ),
            )
            .into_any_element()
    }

    fn render_footer(&self, palette: &PanelPalette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .gap(px(6.0))
            .child(
                footer_button("ghostex-capture-open", "Open Ghostex", palette).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.with_app(cx, |app, cx| {
                            app.close_ghostex_capture_panel(cx);
                            app.ghostex_capture_bring_ghostex_forward(cx);
                        })
                    }),
                ),
            )
            .child(
                footer_button("ghostex-capture-hide", "Hide button", palette).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.with_app(cx, |app, cx| app.hide_ghostex_capture(cx))
                    }),
                ),
            )
            .child(
                footer_button("ghostex-capture-restart", "Restart", palette).on_click(cx.listener(
                    |_, _, _, cx| cx.dispatch_action(&crate::app::actions::RestartGhostexGpui),
                )),
            )
            .child(
                footer_button("ghostex-capture-quit", "Quit", palette).on_click(cx.listener(
                    |_, _, _, cx| cx.dispatch_action(&crate::app::actions::QuitGhostexGpui),
                )),
            )
            .into_any_element()
    }
}

impl Render for CapturePanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (projects, machines, has_draft) = self
            .app
            .upgrade()
            .map(|app| {
                let app = app.read(cx);
                (
                    app.ghostex_capture_panel_projects(),
                    app.ghostex_capture_panel_machines(),
                    app.ghostex_capture_has_draft(),
                )
            })
            .unwrap_or_default();
        let palette = PanelPalette::resolve();
        let heading = div()
            .h(px(HEADING_HEIGHT))
            .px(px(4.0))
            .flex()
            .items_center()
            .justify_between()
            .text_size(px(11.0))
            .text_color(palette.muted)
            .child("RUNNING AGENTS")
            .child(crate::app::render::window_drag_region::window_drag_region(
                div().id("ghostex-capture-panel-drag").flex_1().h_full(),
            ))
            .child("LOCAL + REMOTE");
        let actions = self.render_actions(has_draft, &palette, cx);
        let filter = self.render_filter(&palette);
        let list = self.render_list(projects, machines, &palette, cx);
        let footer = self.render_footer(&palette, cx);
        div()
            .id("ghostex-capture-panel")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // Typing in the filter is text, not the panel's one-key actions.
                if this.filter.focus_handle(cx).is_focused(window) {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        this.focus.focus(window, cx);
                    }
                    return;
                }
                let modifiers = event.keystroke.modifiers;
                if modifiers.platform || modifiers.control || modifiers.alt {
                    return;
                }
                let action = match event.keystroke.key.as_str() {
                    "a" => CaptureAction::Area,
                    "space" => CaptureAction::CurrentApp,
                    "f" => CaptureAction::FullScreen,
                    "t" => CaptureAction::Prompt,
                    "escape" => CaptureAction::TogglePanel,
                    _ => return,
                };
                cx.stop_propagation();
                this.run(action, cx);
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .size_full()
            .flex()
            .flex_col()
            .p(px(PANEL_PAD))
            .rounded(px(14.0))
            .bg(palette.surface)
            .border_1()
            .border_color(palette.edge)
            .child(actions)
            .child(separator(&palette))
            .child(heading)
            .child(filter)
            .child(list)
            .child(separator(&palette))
            .child(footer)
    }
}
