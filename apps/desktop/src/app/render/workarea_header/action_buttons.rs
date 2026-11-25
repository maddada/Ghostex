//! The header's trailing half: the Start / Open / Commit split buttons and the two panel
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

use super::header_pins::{HEADER_ICON_BOX, HeaderSlot};
use crate::app::claude_web::HeaderControl;
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
    /// Drawn in place of `label` when set (Commit's diff numbers and push / pull counts).
    pub(crate) rich_label: Option<AnyElement>,
    /// Hover text for the button's main half.
    pub(crate) tooltip: Option<SharedString>,
    /// Leaves the icon out while the label is shown (Commit's push / pull arrows stand in for it).
    pub(crate) hide_icon: bool,
}

/// CDXC:Titlebar 2026-11-16 DECISION:
/// User: "instead of showing Commit we should instead show the +123 -213 there for that session's
/// worktree basically and also show up or down arrow to the left of them when there's something
/// that we didn't push or something we should pull". The numbers come from the active project's
/// git read the Commit menu already draws from (`titlebar_git_menu_state`: `git diff --numstat
/// HEAD`, untracked files listed, ahead/behind from the local upstream refs), so no new poll and no
/// fetch. With nothing to show the button keeps the word "Commit", so it is never empty.
/// Then: the same colours and look as the sidebar's numbers, and only while the sidebar is not
/// already drawing them for that project (with Separate by machine off it draws none); when it
/// is, the button stays the plain "Commit".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkareaHeaderGitSummary {
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
    pub(crate) ahead: u64,
    pub(crate) behind: u64,
}

impl WorkareaHeaderGitSummary {
    pub(crate) fn from_state(state: &crate::app::model::GpuiTitlebarGitMenuState) -> Self {
        if !state.is_repo {
            return Self::default();
        }
        Self {
            additions: state.additions,
            deletions: state.deletions,
            ahead: state.ahead_count,
            behind: state.behind_count,
        }
    }

    pub(crate) fn has_diff(self) -> bool {
        self.additions > 0 || self.deletions > 0
    }

    pub(crate) fn is_empty(self) -> bool {
        !self.has_diff() && self.ahead == 0 && self.behind == 0
    }

    /// "123 lines added, 213 removed · 2 commits to push · 1 to pull"
    pub(crate) fn tooltip(self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if self.has_diff() {
            parts.push(format!(
                "{} {} added, {} removed",
                self.additions,
                if self.additions == 1 { "line" } else { "lines" },
                self.deletions
            ));
        }
        if self.ahead > 0 {
            parts.push(format!(
                "{} {} to push",
                self.ahead,
                if self.ahead == 1 { "commit" } else { "commits" }
            ));
        }
        if self.behind > 0 {
            parts.push(format!("{} to pull", self.behind));
        }
        Some(parts.join(" \u{00b7} "))
    }

    /// The pill shows an up, down or both arrow: there are commits to push or pull.
    pub(crate) fn shows_arrows(self) -> bool {
        self.ahead > 0 || self.behind > 0
    }

    /// CDXC:Titlebar 2026-11-25 DECISION:
    /// User (Q4869, "do all 3 please. also I don't want the icon on the left to be shown (the
    /// commit icon) when there's arrows + numbers"): the pill gets narrower. Every number reads
    /// short (7.3k from 1000, 13k from 10000), which ends the old 9999 cap. With lines changed,
    /// commits to push or pull show as one arrow without a count (↑ push, ↓ pull, ↕ both) before
    /// "+N -N"; with no lines changed the arrows keep their counts ("↑3 ↓12"). The commit icon is
    /// left out whenever an arrow shows (`shows_arrows`). The tooltip keeps every number in full.
    /// Line numbers use the sidebar project header's own colours and opacity (`project_status.rs`).
    pub(crate) fn render(self) -> Option<AnyElement> {
        use crate::app::native_sidebar::project_status::{
            PROJECT_DIFF_OPACITY, project_diff_additions_color, project_diff_deletions_color,
        };
        if self.is_empty() {
            return None;
        }
        let arrows: Vec<String> = if self.has_diff() {
            let arrow = match (self.ahead > 0, self.behind > 0) {
                (true, true) => Some("\u{2195}"),
                (true, false) => Some("\u{2191}"),
                (false, true) => Some("\u{2193}"),
                (false, false) => None,
            };
            arrow.into_iter().map(str::to_string).collect()
        } else {
            [("\u{2191}", self.ahead), ("\u{2193}", self.behind)]
                .into_iter()
                .filter(|(_, count)| *count > 0)
                .map(|(arrow, count)| format!("{arrow}{}", compact_count(count)))
                .collect()
        };
        Some(
            h_flex()
                .items_center()
                .gap(px(8.0))
                .children(arrows.into_iter().map(|arrow| div().child(arrow)))
                .when(self.has_diff(), |this| {
                    this.child(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .opacity(PROJECT_DIFF_OPACITY)
                            .child(
                                div()
                                    .text_color(project_diff_additions_color())
                                    .child(format!("+{}", compact_count(self.additions))),
                            )
                            .child(
                                div()
                                    .text_color(project_diff_deletions_color())
                                    .child(format!("-{}", compact_count(self.deletions))),
                            ),
                    )
                })
                .into_any_element(),
        )
    }
}

/// 1000 and up read 7.3k, 10000 and up 13k: the chat core's one compact-number formatter
/// (the same one the context-usage numbers use).
fn compact_count(value: u64) -> String {
    ghostex_gx_chat_core::menus::context::usage::format_context_tokens(Some(value as f64))
}

impl GhostexGpuiApp {
    /// The Project sidebar is drawing the active project's "+N -N" on its header now, so the
    /// Commit button would only repeat it. Mirrors where the sidebar draws a project header (the
    /// classic list, not Unified or the Bots list) and `project_header_shows_diff_stats`.
    fn sidebar_shows_active_project_diff_stats(&self) -> bool {
        if self.native_sidebar.unified.is_some() {
            return false;
        }
        let Some(snapshot) = self.native_sidebar.snapshot.as_deref() else {
            return false;
        };
        if snapshot.bots_mode {
            return false;
        }
        let Some(active) = self.git_active_scoped_project_id() else {
            return false;
        };
        snapshot.groups.iter().any(|group| {
            group.owner_group_id.is_none()
                && self
                    .git_scope_for_group(&group.group_id)
                    .is_ok_and(|scope| scope.scoped_id() == active)
                && crate::app::native_sidebar::project_status::project_header_shows_diff_stats(
                    group,
                    &snapshot.hud,
                )
        })
    }
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
                    this.header_pins.unpin_row_in.set(None);
                    primary(this, trigger_bounds, window, cx);
                }),
            )
            // Right-click unpins (header_pins.rs); the arrow still opens the menu.
            // Right-click opens the menu as the arrow does, ending in "Unpin from header"
            // (header_pins.rs `header_menu_unpin_rows`).
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.header_pins.unpin_row_in.set(Some(kind));
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
                } else if spec.hide_icon && !compact {
                    this
                } else {
                    this.child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .size(px(HEADER_ICON_BOX))
                            .items_center()
                            .justify_center()
                            .child(titlebar_svg_icon(spec.icon, spec.icon_size, icon_color)),
                    )
                }
            })
            .when(!compact, |this| match spec.rich_label {
                Some(rich) => this.child(rich),
                None => this.child(spec.label),
            })
            .when_some(spec.tooltip, |this, tooltip| {
                this.tooltip(move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx))
            });

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
                    this.header_pins.unpin_row_in.set(None);
                    menu(this, trigger_bounds, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.header_pins.unpin_row_in.set(None);
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
                let app = cx.entity().downgrade();
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
                    GhostexGpuiApp::open_pending_titlebar_popup(
                        app.clone(),
                        &[kind],
                        bounds,
                        window,
                        cx,
                    );
                }
            })
            .into_any_element()
    }

    /// The header's trailing controls, in the order the mockup lists them: the three split buttons,
    /// then the command terminal and view panel toggles.
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
        let git_summary = self
            .titlebar_git_menu_state
            .as_ref()
            .filter(|_| !self.sidebar_shows_active_project_diff_stats())
            .map(WorkareaHeaderGitSummary::from_state)
            .unwrap_or_default();
        let (open_target_icon_path, _open_target_icon_size) = self.titlebar_open_target_icon();
        // While Claude is on only the app's own controls stay (app/claude_web/header.rs).
        let shown = |control| self.header_control_shown(control);
        let pinned_extension_buttons = if shown(HeaderControl::PinnedExtensions) {
            self.render_titlebar_pinned_extension_buttons(window, cx)
        } else {
            Vec::new()
        };
        // Pinned or in use (header_pins.rs); each button's own checks still decide if it applies.
        let drawn_slots = HeaderSlot::ALL.map(|slot| self.header_slot_drawn(slot, cx));
        let drawn = move |slot: HeaderSlot| {
            HeaderSlot::ALL
                .iter()
                .position(|candidate| *candidate == slot)
                .is_some_and(|index| drawn_slots[index])
        };
        let show_start = show_actions_button
            && shown(HeaderControl::Actions)
            && !self.built_in_feature_button_hidden(
                ghostex_settings_catalog::built_in_extensions::ACTIONS,
            )
            && drawn(HeaderSlot::Start);
        let show_open = shown(HeaderControl::OpenIn)
            && !self.built_in_feature_button_hidden(
                ghostex_settings_catalog::built_in_extensions::OPEN_IN,
            )
            && drawn(HeaderSlot::Open);
        let show_commit = shown(HeaderControl::Commit)
            && !self.titlebar_button_hidden(
                GIT_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                "gitActions",
            )
            && drawn(HeaderSlot::Commit);
        let mut segments: Vec<AnyElement> = Vec::new();
        if show_start {
            segments.push(self.render_workarea_header_split_button(
                WorkareaHeaderSplitButton {
                    id: "ghostex-gpui-workarea-header-button-actions",
                    kind: GpuiTitlebarPopupKind::Actions,
                    icon: actions_icon_path,
                    icon_size: 16.0,
                    label: actions_label,
                    dimmed: self.titlebar_quick_action_button_on_cooldown(),
                    busy: false,
                    rich_label: None,
                    tooltip: None,
                    hide_icon: false,
                },
                compact,
                run_quick_action,
                toggle_quick_actions_menu,
                window,
                cx,
            ));
        }
        if show_open {
            segments.push(self.render_workarea_header_split_button(
                WorkareaHeaderSplitButton {
                    id: "ghostex-gpui-workarea-header-button-open-project",
                    kind: GpuiTitlebarPopupKind::OpenTargets,
                    icon: open_target_icon_path,
                    icon_size: 13.0,
                    label: "Open".into(),
                    dimmed: false,
                    busy: false,
                    rich_label: None,
                    tooltip: None,
                    hide_icon: false,
                },
                compact,
                open_active_open_target,
                toggle_open_targets_menu,
                window,
                cx,
            ));
        }
        if show_commit {
            segments.push(
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
                        rich_label: git_summary.render(),
                        tooltip: git_summary.tooltip().map(SharedString::from),
                        hide_icon: git_summary.shows_arrows(),
                    },
                    compact,
                    toggle_git_menu,
                    toggle_git_menu,
                    window,
                    cx,
                ),
            );
        }
        /*
        CDXC:Titlebar 2026-11-20 DECISION:
        User: "separate these 2 please, I like them separate like they used to be, 2 buttons not one
        wide button". Start, Open and Commit are each their own outlined split button (main half,
        divider, arrow) with a gap between them again. Supersedes the shared outline of
        2026-11-19 (Q4196 C); the pins, the ⋯ menu and the one icon size from that change stay.
        */
        let buttons = h_flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .items_center()
            .children(pinned_extension_buttons)
            .map(|this| {
                if !shown(HeaderControl::PromptEditorOrExitFocus) {
                    return this;
                }
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
            // A clone or other long job in progress (CDXC:BackgroundJobs in window/jobs_popover_view.rs).
            .children(self.render_workarea_header_jobs_button(cx))
            // Claude and Grok Bot, directly left of Voice (CDXC:Extensions 2026-11-10 in claude_web/header_buttons.rs),
            // each while pinned or on.
            .when(shown(HeaderControl::WebBots), |this| {
                this.children(self.render_workarea_header_web_bot_buttons(cx))
            })
            // The Voice button leads the split buttons (CDXC:VoiceMode in voice_agent/header_button.rs),
            // while pinned or in a call.
            .when(
                crate::app::voice_agent::voice_agent_on()
                    && shown(HeaderControl::Voice)
                    && drawn(HeaderSlot::Voice),
                |this| this.child(self.render_workarea_header_voice_button(cx)),
            )
            .children(segments);
        // The pinned controls and the two panel toggles never scroll out of reach: only the split buttons
        // and pinned extensions before them can, so a narrow column loses the least-needed first.
        let pinned = h_flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .items_center()
            .gap(px(WORKAREA_HEADER_PINNED_GAP))
            .ml(px(6.0))
            // Approvals put aside with Escape or Close, until answered (notice_modal_host.rs).
            .children(self.render_approvals_waiting_pill(compact, cx))
            .children(self.render_workarea_header_box_chip(compact, cx))
            .when(shown(HeaderControl::ClaudeCloud), |this| {
                this.children(self.render_workarea_header_claude_cloud_button(compact, cx))
            })
            // Work and Session details: one spot beside the chat, one panel at a time
            // (work_button.rs).
            .when(drawn(HeaderSlot::Work), |this| {
                this.children(self.render_workarea_header_work_button(cx))
            })
            .when(
                shown(HeaderControl::SessionDetails) && drawn(HeaderSlot::SessionDetails),
                |this| this.children(self.render_workarea_header_session_details_button(cx)),
            )
            .when(drawn(HeaderSlot::Share), |this| {
                this.children(self.render_workarea_header_share_button(cx))
            })
            // The focused session in a window of its own (app/session_popup/).
            .when(drawn(HeaderSlot::Popout), |this| {
                this.children(self.render_workarea_header_session_popup_button(cx))
            })
            // What is not pinned (header_pins.rs). The app menus the old trailing ⋯ held stay in the
            // sidebar's menu (CDXC:Titlebar 2026-11-11 DECISION in titlebar/menu_panels.rs).
            .children(self.render_workarea_header_more_button(cx))
            // With a view open the toggles end the view tab strip instead (render_workarea_panel_toggles).
            .when(
                !self.workarea_header_hosts_view_tab_strip() && shown(HeaderControl::PanelToggles),
                |this| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .ml(px(self.workarea_header_closing_clearance()))
                            .child(self.render_workarea_panel_toggles(cx)),
                    )
                },
            )
            .child(self.render_titlebar_extension_popup_panel(window, cx));

        h_flex()
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
            .child(pinned)
    }
}
