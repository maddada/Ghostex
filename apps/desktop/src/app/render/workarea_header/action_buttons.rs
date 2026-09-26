//! The header's trailing half: the Start / Open / Commit split buttons, the ⋯ menu, the two panel
//! toggles. Platform caption controls belong to the rightmost band region.

use gpui::AnyElement;
use gpui::Bounds;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::SharedString;
use gpui::Styled as _;
use gpui::Window;
use gpui::canvas;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::h_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

/// What a press on one half of a split button runs. A plain `fn` pointer so the two halves and the
/// diagnostics wrapper can share it without boxing.
type WorkareaHeaderPress = fn(
    &mut GhostexGpuiApp,
    Option<Bounds<Pixels>>,
    &mut Window,
    &mut gpui::Context<GhostexGpuiApp>,
);

/// CDXC:Titlebar 2026-09-20 DECISION:
/// User: Quick Actions, Open In and Git actions become Start / Open / Commit split buttons in the
/// header, each labelled, each with a caret that opens the menu it already had, and each dropping
/// its label when the column is narrow.
pub(crate) struct WorkareaHeaderSplitButton {
    pub(crate) id: &'static str,
    pub(crate) kind: GpuiTitlebarPopupKind,
    pub(crate) icon: &'static str,
    pub(crate) icon_size: f32,
    pub(crate) label: SharedString,
    /// Dimmed while the button's primary press is on cooldown.
    pub(crate) dimmed: bool,
    /// Replaces the icon with the shared busy spinner, as the Git button did.
    pub(crate) busy: bool,
}

/// A name longer than this is cut and given an ellipsis, so one verbose Action cannot push Open
/// and Commit out of the header.
const WORKAREA_HEADER_QUICK_ACTION_LABEL_MAX_CHARS: usize = 9;

/// CDXC:Titlebar 2026-09-21 DECISION:
/// User: the Quick Actions split button shows the name of the Action that was last used, like it
/// already does for its icon, cut to nine characters with an ellipsis when it is longer. "Start"
/// remains only when no Action is configured (or the name is blank). Supersedes the fixed "Start"
/// label from the 2026-09-20 split-button decision.
fn workarea_header_quick_action_label(action: Option<&GpuiTitlebarAction>) -> SharedString {
    let Some(name) = action
        .map(|action| action.name.trim())
        .filter(|name| !name.is_empty())
    else {
        return SharedString::new_static("Start");
    };
    let mut chars = name.chars();
    let head: String = chars
        .by_ref()
        .take(WORKAREA_HEADER_QUICK_ACTION_LABEL_MAX_CHARS)
        .collect();
    if chars.next().is_some() {
        SharedString::from(format!("{head}…"))
    } else {
        SharedString::from(head)
    }
}

fn run_quick_action(
    app: &mut GhostexGpuiApp,
    _trigger_bounds: Option<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    app.run_active_gpui_titlebar_action(window, cx);
}

fn toggle_quick_actions_menu(
    app: &mut GhostexGpuiApp,
    trigger_bounds: Option<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    let open = app.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Actions);
    app.set_gpui_titlebar_popup_open(
        GpuiTitlebarPopupKind::Actions,
        !open,
        trigger_bounds,
        window,
        cx,
    );
}

fn open_active_open_target(
    app: &mut GhostexGpuiApp,
    _trigger_bounds: Option<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    app.open_active_project_with_active_open_target(window, cx);
}

fn toggle_open_targets_menu(
    app: &mut GhostexGpuiApp,
    trigger_bounds: Option<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    let open = app.titlebar_popup_menu_open(GpuiTitlebarPopupKind::OpenTargets);
    app.set_gpui_titlebar_popup_open(
        GpuiTitlebarPopupKind::OpenTargets,
        !open,
        trigger_bounds,
        window,
        cx,
    );
}

/*
CDXC:Git 2026-09-20 WHY:
Commit's main half opens the Git menu rather than running the primary Git action, because that is
exactly what the titlebar's Git button did on a left click. Nothing in the product confirms a
one-click commit, so the split button's shape changed and its behaviour did not.
*/
fn toggle_git_menu(
    app: &mut GhostexGpuiApp,
    trigger_bounds: Option<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    app.show_gpui_titlebar_git_menu(trigger_bounds, window, cx);
}

impl GhostexGpuiApp {
    fn render_workarea_header_split_button(
        &self,
        spec: WorkareaHeaderSplitButton,
        compact: bool,
        primary: WorkareaHeaderPress,
        menu: WorkareaHeaderPress,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:PlatformSupport 2026-07-26:
        These controls are exact normal-layout children of the draggable
        header. On Windows each interactive frame must occlude the ancestor
        Drag hitbox so WM_NCHITTEST leaves that rectangle in the client area
        and GPUI delivers its normal mouse handlers. This is button-local
        ownership, not an overlay or synthetic event route.
        */
        let kind = spec.kind;
        let open = self.titlebar_popup_menu_open(kind);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let anchor_state =
            window.use_keyed_state(spec.id, cx, |_, _| GpuiTitlebarPopupAnchorState::default());
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds_captured = anchor_state.read(cx).trigger_bounds_captured;
        let trigger_bounds = trigger_bounds_captured.then_some(anchor_bounds);

        let main = div()
            .id(SharedString::from(format!("{}-main", spec.id)))
            .flex()
            .h_full()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .pl(px(if compact { 9.0 } else { 10.0 }))
            .pr(px(if compact { 7.0 } else { 9.0 }))
            .rounded_l(titlebar_split_button_segment_radius(
                TITLEBAR_CONTROL_HEIGHT,
            ))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .text_color(icon_color)
            .text_size(px(12.5))
            .cursor_default()
            .hover(move |this| {
                this.bg(titlebar_split_button_hover_color())
                    .text_color(titlebar_split_button_hover_text_color())
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    primary(this, trigger_bounds, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    menu(this, trigger_bounds, window, cx);
                }),
            )
            .map(|this| {
                if spec.busy {
                    this.child(
                        canvas(
                            move |_bounds, _window, _cx| {},
                            move |bounds, _state: (), window, _cx| {
                                paint_titlebar_git_busy_spinner(bounds, window);
                            },
                        )
                        .size(px(15.0)),
                    )
                } else {
                    this.child(titlebar_svg_icon(spec.icon, spec.icon_size, icon_color))
                }
            })
            .when(!compact, |this| this.child(spec.label));

        let caret = div()
            .id(SharedString::from(format!("{}-caret", spec.id)))
            .flex()
            .h_full()
            .items_center()
            .justify_center()
            .pl(px(5.0))
            .pr(px(6.0))
            .rounded_r(titlebar_split_button_segment_radius(
                TITLEBAR_CONTROL_HEIGHT,
            ))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .cursor_default()
            .when(open, |this| this.bg(titlebar_split_button_open_color()))
            .hover(move |this| this.bg(titlebar_split_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    menu(this, trigger_bounds, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    menu(this, trigger_bounds, window, cx);
                }),
            )
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_CHEVRON_DOWN,
                12.0,
                icon_color,
            ));

        titlebar_split_button_frame(h_flex().id(spec.id), TITLEBAR_CONTROL_HEIGHT)
            .relative()
            .flex_shrink_0()
            .ml(px(4.0))
            .items_center()
            .overflow_hidden()
            .when(spec.dimmed, |this| this.opacity(0.5))
            .child(main)
            .child(titlebar_split_button_divider(TITLEBAR_CONTROL_HEIGHT))
            .child(caret)
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let (first_capture, moved) = anchor_state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        (first_capture, moved)
                    });
                    if first_capture || moved {
                        window.request_animation_frame();
                    }
                }
            })
            .into_any_element()
    }

    /// The header's trailing controls, in the order the mockup lists them: the three split buttons,
    /// the ⋯ menu, then the command terminal and view panel toggles.
    pub(crate) fn render_workarea_header_actions(
        &self,
        compact: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let active_action = self.active_gpui_titlebar_action();
        let actions_icon_path = titlebar_action_icon_path(active_action.as_ref());
        let actions_label = workarea_header_quick_action_label(active_action.as_ref());
        /*
        Quick Actions is a discoverable header control on desktop, including
        before the first Action has been configured. Keep it visible on Windows
        as it is on macOS so its empty-state click can open Settings > Actions;
        Linux retains its existing configured-action-only behavior.
        */
        let show_actions_button =
            cfg!(any(target_os = "macos", target_os = "windows")) || active_action.is_some();
        let git_icon_path = self
            .titlebar_git_menu_state
            .as_ref()
            .map(|state| titlebar_git_action_icon_path(state.primary_action))
            .unwrap_or(TITLEBAR_ICON_GIT_COMMIT);
        let (open_target_icon_path, _open_target_icon_size) = self.titlebar_open_target_icon();
        let pinned_extension_buttons = self.render_titlebar_pinned_extension_buttons(window, cx);
        let buttons = h_flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .items_center()
            .children(pinned_extension_buttons)
            .map(|this| {
                // Prompt Editor and Exit Focus share the same header slot;
                // when both are eligible only Prompt Editor renders.
                if self.prompt_editor_daemon_open {
                    return this.child(self.render_titlebar_prompt_editor_button(cx));
                }
                if let Some(signature) = self.titlebar_exit_focus_control_signature() {
                    return this.child(self.render_titlebar_exit_focus_button(signature, cx));
                }
                this
            })
            .when(
                show_actions_button
                    && !self.titlebar_button_hidden(
                        QUICK_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                        "quickActions",
                    ),
                |this| {
                    this.child(self.render_workarea_header_split_button(
                        WorkareaHeaderSplitButton {
                            id: "ghostex-gpui-workarea-header-button-actions",
                            kind: GpuiTitlebarPopupKind::Actions,
                            icon: actions_icon_path,
                            icon_size: 16.0,
                            label: actions_label,
                            dimmed: self.titlebar_quick_action_button_on_cooldown(),
                            busy: false,
                        },
                        compact,
                        run_quick_action,
                        toggle_quick_actions_menu,
                        window,
                        cx,
                    ))
                },
            )
            .when(
                !self.titlebar_button_hidden(OPEN_IN_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "openIn"),
                |this| {
                    this.child(self.render_workarea_header_split_button(
                        WorkareaHeaderSplitButton {
                            id: "ghostex-gpui-workarea-header-button-open-project",
                            kind: GpuiTitlebarPopupKind::OpenTargets,
                            icon: open_target_icon_path,
                            icon_size: 13.0,
                            label: "Open".into(),
                            dimmed: false,
                            busy: false,
                        },
                        compact,
                        open_active_open_target,
                        toggle_open_targets_menu,
                        window,
                        cx,
                    ))
                },
            )
            .when(
                !self.titlebar_button_hidden(
                    GIT_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                    "gitActions",
                ),
                |this| {
                    this.child(
                        self.render_workarea_header_split_button(
                            WorkareaHeaderSplitButton {
                                id: "ghostex-gpui-workarea-header-button-git",
                                kind: GpuiTitlebarPopupKind::Git,
                                icon: git_icon_path,
                                icon_size: 16.0,
                                label: "Commit".into(),
                                dimmed: false,
                                busy: self
                                    .titlebar_git_menu_state
                                    .as_ref()
                                    .is_some_and(|state| state.is_busy),
                            },
                            compact,
                            toggle_git_menu,
                            toggle_git_menu,
                            window,
                            cx,
                        ),
                    )
                },
            );
        // The ⋯ menu and the two panel toggles never scroll out of reach: only the split buttons
        // and pinned extensions before them can, so a narrow column loses the least-needed first.
        let pinned = h_flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .items_center()
            .gap(px(WORKAREA_HEADER_PINNED_GAP))
            .ml(px(4.0))
            // Everything occasional lives behind the trailing ⋯ menu.
            .when(self.titlebar_more_menu_visible(), |this| {
                this.child(self.render_titlebar_more_button(cx))
            })
            // With a view open the toggles end the view tab strip instead (render_workarea_panel_toggles).
            .when(!self.workarea_header_hosts_view_tab_strip(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .ml(px(self.workarea_header_closing_clearance()))
                        .child(self.render_workarea_panel_toggles(cx)),
                )
            })
            .child(self.render_titlebar_extension_popup_panel(window, cx));
        let controls = h_flex()
            .flex_shrink(1.0)
            .min_w_0()
            .max_w_full()
            .h_full()
            .items_center()
            .pr(px(
                WORKAREA_HEADER_EDGE_PADDING + self.workarea_header_opening_clearance()
            ))
            .child(
                h_flex()
                    .id("ghostex-gpui-workarea-header-controls-scroll")
                    .flex_shrink(1.0)
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .overflow_x_scroll()
                    .child(buttons),
            )
            .child(pinned);
        controls
    }
}
