//! The page's side of the Beads view's Board (the desktop's `app/native_kanban/`, compiled here
//! file by file): the project it shows (the store's active project on this computer), the context
//! its Beads calls run with, the board's menus drawn over the board itself, and honest answers for
//! what only the desktop app can do (the board's own window, installing Beads).
//!
//! CDXC:WebGpui 2026-11-10 WHY:
//! The desktop's board menus are child windows hung off the titlebar's popup machinery, which the page does not have; the board's menus are drawn here as an overlay inside the board, with the same rows and the same actions (`NativeKanbanAction`).

use std::cell::RefCell;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Action, AnyElement, AnyWindowHandle, App, Bounds, Context, ElementId, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Point, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, anchored, deferred, div, point, px,
};

use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_svg_icon;
use crate::app::model::{GpuiRemoteGxserverRequestTarget, TitlebarMode};
use crate::app::native_kanban::actions::NativeKanbanAction;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::native_kanban::state::{KanbanProject, KanbanProjectKey};

/// The desktop's `ProjectBoardBridgeRuntimeContext` (`helpers/board_gxserver/project_board_bridge.rs`):
/// the project a Beads call is for. The page reaches only this computer's gxserver, so
/// `remote_machine_id` and `remote_target` stay empty.
#[derive(Clone)]
pub(crate) struct ProjectBoardBridgeRuntimeContext {
    pub(crate) project_id: Option<String>,
    pub(crate) project_path: String,
    pub(crate) remote_machine_id: Option<String>,
    pub(crate) remote_target: Option<GpuiRemoteGxserverRequestTarget>,
}

/// The board's own window (Open in New Window); a page never opens one.
#[allow(dead_code)]
pub(crate) struct KanbanDetachedWindow {
    pub(crate) window: AnyWindowHandle,
}

/// The first load's skeleton pulse: the desktop's `session_chat_skeleton::skeleton_pulse`, read
/// from the same shared skeleton file.
pub(crate) fn skeleton_pulse() -> (Duration, f32) {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Pulse {
        pulse_ms: u64,
        pulse_min_opacity: f32,
    }
    thread_local! {
        static PULSE: Option<(Duration, f32)> = serde_json::from_str::<Pulse>(include_str!(
            "../../../../../packages/gx-chat-core/visual/transcript-skeleton.json"
        ))
        .ok()
        .map(|pulse| (Duration::from_millis(pulse.pulse_ms), pulse.pulse_min_opacity));
    }
    PULSE.with(|pulse| pulse.unwrap_or((Duration::from_millis(1600), 0.5)))
}

#[derive(Clone)]
enum WebMenuRow {
    Item {
        label: SharedString,
        icon: Option<&'static str>,
        checked: bool,
        disabled: bool,
        action: Option<NativeKanbanAction>,
    },
    Separator,
}

/// The menu open over the board: its rows, where it hangs, and the button bounds that toggle it.
#[derive(Clone)]
struct WebMenuState {
    rows: Vec<WebMenuRow>,
    position: Point<Pixels>,
    trigger: Option<Bounds<Pixels>>,
}

thread_local! {
    /// The page is one thread; the board's menu functions build a menu without the app at hand
    /// (the desktop's `GpuiContextMenu` shape), so the open menu lives here until it is closed.
    static OPEN_MENU: RefCell<Option<WebMenuState>> = const { RefCell::new(None) };
}

/// The desktop's `GpuiContextMenu` builder, as far as the board uses it.
pub(crate) struct GpuiContextMenu {
    rows: Vec<WebMenuRow>,
}

impl GpuiContextMenu {
    pub(crate) fn new() -> Self {
        Self { rows: Vec::new() }
    }

    fn row(
        mut self,
        label: impl Into<SharedString>,
        icon: Option<&'static str>,
        checked: bool,
        disabled: bool,
        action: Box<dyn Action>,
    ) -> Self {
        self.rows.push(WebMenuRow::Item {
            label: label.into(),
            icon,
            checked,
            disabled,
            action: action
                .as_any()
                .downcast_ref::<NativeKanbanAction>()
                .cloned(),
        });
        self
    }

    pub(crate) fn menu(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.row(label, None, false, false, action)
    }

    pub(crate) fn menu_with_check(
        self,
        label: impl Into<SharedString>,
        checked: bool,
        action: Box<dyn Action>,
    ) -> Self {
        self.row(label, None, checked, false, action)
    }

    pub(crate) fn menu_with_icon(
        self,
        label: impl Into<SharedString>,
        icon: &'static str,
        disabled: bool,
        action: Box<dyn Action>,
    ) -> Self {
        self.row(label, Some(icon), false, disabled, action)
    }

    pub(crate) fn separator(mut self) -> Self {
        if self
            .rows
            .last()
            .is_some_and(|row| !matches!(row, WebMenuRow::Separator))
        {
            self.rows.push(WebMenuRow::Separator);
        }
        self
    }

    /// Opens the menu below `trigger`, or closes it when that button's menu is already open.
    pub(crate) fn toggle_below(self, trigger: Bounds<Pixels>, window: &mut Window, _cx: &mut App) {
        let already_open = OPEN_MENU
            .with(|open| open.borrow().as_ref().and_then(|menu| menu.trigger) == Some(trigger));
        self.open(
            (!already_open).then(|| WebMenuState {
                rows: Vec::new(),
                position: point(trigger.left(), trigger.bottom() + px(4.0)),
                trigger: Some(trigger),
            }),
            window,
        );
    }

    /// Opens the menu at `position` (a card's right-click).
    pub(crate) fn show(self, position: Point<Pixels>, window: &mut Window, _cx: &mut App) {
        self.open(
            Some(WebMenuState {
                rows: Vec::new(),
                position,
                trigger: None,
            }),
            window,
        );
    }

    fn open(mut self, state: Option<WebMenuState>, window: &mut Window) {
        while matches!(self.rows.last(), Some(WebMenuRow::Separator)) {
            self.rows.pop();
        }
        let state = state
            .filter(|_| !self.rows.is_empty())
            .map(|state| WebMenuState {
                rows: self.rows,
                ..state
            });
        OPEN_MENU.with(|open| *open.borrow_mut() = state);
        window.refresh();
    }
}

fn close_menu() {
    OPEN_MENU.with(|open| open.borrow_mut().take());
}

impl GhostexGpuiApp {
    /// The project the board shows: the store's active project, when it is on this computer and
    /// has a folder.
    pub(crate) fn native_kanban_project(&self) -> Option<KanbanProject> {
        let key = self.git_active_scoped_project_id()?;
        let scope = self.git_scope_for_project(&key).ok()?;
        if scope.is_remote() {
            return None;
        }
        Some(KanbanProject {
            key: KanbanProjectKey {
                project_id: Some(scope.project_id()),
                project_path: scope.path.clone()?,
                remote_machine_id: None,
            },
            display_name: scope.title,
        })
    }

    pub(crate) fn native_kanban_bridge_context(&self) -> Option<ProjectBoardBridgeRuntimeContext> {
        let project = self.native_kanban_project()?;
        Some(ProjectBoardBridgeRuntimeContext {
            project_id: project.key.project_id,
            project_path: project.key.project_path,
            remote_machine_id: None,
            remote_target: None,
        })
    }

    /// The page's text fields keep the keys on their own.
    pub(crate) fn native_kanban_input_focused(&mut self) {}

    /// The board polls while the Beads panel shows it (`native_kanban_shown`, set by the panel).
    pub(crate) fn native_kanban_visible(&self) -> bool {
        self.native_kanban_shown && self.native_kanban.project.is_some()
    }

    /// The page has no project editor surfaces to name.
    pub(crate) fn native_kanban_project_editor_id(&self) -> Option<String> {
        None
    }

    /// The board never has a window of its own in a page.
    pub(crate) fn native_kanban_in_own_window(&self, _window: &Window) -> bool {
        false
    }

    pub(crate) fn native_kanban_sync_window_title(&mut self, _cx: &mut Context<Self>) {}

    /// Open in New Window is the desktop app's; the page never has the board out of its panel.
    #[allow(dead_code)] // for the Beads header's controls
    pub(crate) fn native_kanban_detached(&self) -> bool {
        false
    }

    #[allow(dead_code)] // for the Beads header's controls
    pub(crate) fn native_kanban_can_open_window(&self) -> bool {
        false
    }

    #[allow(dead_code)] // for the Beads header's controls
    pub(crate) fn native_kanban_open_window(&mut self, cx: &mut Context<Self>) {
        self.dispatch_gpui_app_modal_toast(
            "info",
            "Open the board in the Ghostex app",
            "A new window is only available in the app.",
            cx,
        );
    }

    /// A press on the board gives it the keyboard (the desktop routes it through the view
    /// panel's focus).
    pub(crate) fn focus_project_editor_surface(
        &mut self,
        _mode: TitlebarMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.default_prevented() {
            return;
        }
        if let Some(focus) = self.native_kanban.focus.clone() {
            window.focus(&focus, cx);
        }
    }

    /// The board's notice offers Install Beads; the page has nothing to read about it.
    pub(crate) fn native_kanban_fetch_beads_tool(&mut self, _cx: &mut Context<Self>) {
        self.native_kanban.beads_install.requested = true;
    }

    /// Installing Beads runs a download on the computer, which the Ghostex app does.
    pub(crate) fn native_kanban_install_beads(&mut self, cx: &mut Context<Self>) {
        self.dispatch_gpui_app_modal_toast(
            "info",
            "Install Beads in the Ghostex app",
            "The browser can't install tools on this computer.",
            cx,
        );
    }

    /// The board's agent and link requests, checked as the desktop checks them, to the store
    /// (`gx_store/create/board.rs`), which answers through
    /// `receive_sidebar_project_board_conversation_response_payload`.
    pub(crate) fn dispatch_gpui_project_board_conversation_request(
        &mut self,
        request: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> bool {
        let request_id_valid = request
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .is_some_and(|value| {
                !value.is_empty()
                    && value.chars().count() <= crate::GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
            });
        request_id_valid && self.gx_store_run_board_conversation_request(request, cx)
    }

    /// The menu open over the board (Filters, card details, a card's right-click menu).
    pub(crate) fn native_kanban_menu_overlay(
        &self,
        p: &KanbanPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = OPEN_MENU.with(|open| open.borrow().clone())?;
        let hover = p.control_hover;
        let trigger = menu.trigger;
        let rows = menu
            .rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| match row {
                WebMenuRow::Separator => {
                    div().my(px(4.0)).h(px(1.0)).bg(p.border).into_any_element()
                }
                WebMenuRow::Item {
                    label,
                    icon,
                    checked,
                    disabled,
                    action,
                } => div()
                    .id(ElementId::Name(format!("kanban-web-menu-{index}").into()))
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .text_color(p.foreground)
                    .when(disabled, |this| this.opacity(0.45))
                    .when(!disabled, |this| {
                        this.cursor_pointer().hover(move |style| style.bg(hover))
                    })
                    .when_some(icon, |this, icon| {
                        this.child(titlebar_svg_icon(icon, 14.0, p.muted))
                    })
                    .child(div().flex_1().truncate().child(label))
                    .when(checked, |this| {
                        this.child(titlebar_svg_icon("titlebar/check.svg", 14.0, p.muted))
                    })
                    .when_some(action.filter(|_| !disabled), |this, action| {
                        this.on_click(cx.listener(move |this, _, window, cx| {
                            close_menu();
                            this.handle_native_kanban_action(&action, window, cx);
                        }))
                    })
                    .into_any_element(),
            })
            .collect::<Vec<_>>();
        Some(
            deferred(
                anchored().position(menu.position).snap_to_window().child(
                    div()
                        .id("kanban-web-menu")
                        .min_w(px(200.0))
                        .max_w(px(320.0))
                        .p(px(6.0))
                        .flex()
                        .flex_col()
                        .rounded(px(8.0))
                        .bg(p.panel)
                        .border_1()
                        .border_color(p.border_strong)
                        .font_family(p.font.clone())
                        // A press on the button that opened the menu is left to its toggle.
                        .on_mouse_down_out(cx.listener(
                            move |_, event: &gpui::MouseDownEvent, window, _| {
                                if trigger.is_some_and(|trigger| trigger.contains(&event.position))
                                {
                                    return;
                                }
                                close_menu();
                                window.refresh();
                            },
                        ))
                        .children(rows),
                ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
