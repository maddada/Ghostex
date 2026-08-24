// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: status pet overlay presentation and menu-bar/command-palette activations

use std::collections::HashSet;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ObjectFit;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::StyledImage as _;
use gpui::Window;
use gpui::div;
use gpui::img;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::native_menu::NativeMenu;
use gpui_component::v_flex;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn render_gpui_status_pet_presentation(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIStatusPetOverlay 2026-06-26-05:07:
        Worker 49 added aggregate status activation with exact visible controls and no broad UI, transparent root overlays, hidden hit regions, hit-test reroutes, or Rust-side session materialization fallbacks.

        CDXC:GPUIStatusPetOverlay 2026-06-26-05:30:
        Worker 50 adds the Pet Overlay surface to the bottom-right GPUI-owned stack. When the saved pet setting is disabled, GPUI renders no pet surface; when enabled, only the visible avatar, activity cards, and collapsed status badges are interactive, and all clicks still dispatch one bounded session id through the fixed status/pet activation callback.

        CDXC:GPUIStatusPetOverlay 2026-06-27-20:11:
        The standalone GPUI floating session indicator was removed. Keep this
        stack for the floating pet only; status counts still feed the pet badges,
        menu bar item, and attention notifications without rendering a separate
        status row.
        */
        let show_pet = self.sidebar_pet_overlay.enabled;

        if !show_pet {
            return div()
                .id("ghostex-gpui-status-pet-stack-empty")
                .size(px(0.0))
                .into_any_element();
        }

        let mut stack = v_flex()
            .id("ghostex-gpui-status-pet-stack")
            .absolute()
            .right(px(GPUI_STATUS_PET_STACK_RIGHT_INSET))
            .bottom(px(GPUI_STATUS_PET_STACK_BOTTOM_INSET))
            .w(px(GPUI_STATUS_PET_STACK_WIDTH))
            .items_end()
            .gap(px(8.0));
        if show_pet {
            stack = stack.child(self.render_gpui_pet_overlay(cx));
        }
        stack.into_any_element()
    }

    pub(crate) fn render_gpui_pet_overlay(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let has_activities = !self.sidebar_pet_overlay.activities.is_empty();
        let has_status_badges = self
            .sidebar_pet_overlay
            .status_items
            .iter()
            .any(|item| item.count > 0);

        let mut overlay = v_flex()
            .id("ghostex-gpui-pet-overlay")
            .w(px(GPUI_STATUS_PET_STACK_WIDTH))
            .items_end()
            .gap(px(6.0));
        if has_activities && self.gpui_pet_overlay_activities_visible {
            overlay = overlay.child(self.render_gpui_pet_activity_stack(cx));
        } else if !self.gpui_pet_overlay_activities_visible && has_status_badges {
            overlay = overlay.child(self.render_gpui_pet_status_badges(cx));
        }
        overlay = overlay.child(
            self.render_gpui_pet_avatar(self.sidebar_pet_overlay.selected_pet_id.as_str(), cx),
        );
        overlay.into_any_element()
    }

    pub(crate) fn render_gpui_pet_activity_stack(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let mut stack = v_flex()
            .id("ghostex-gpui-pet-activity-stack")
            .w(px(GPUI_PET_OVERLAY_ACTIVITY_CARD_WIDTH))
            .items_end()
            .gap(px(6.0));
        for (index, activity) in self.sidebar_pet_overlay.activities.iter().enumerate() {
            stack = stack.child(self.render_gpui_pet_activity_card(index, activity, cx));
        }
        stack.into_any_element()
    }

    pub(crate) fn render_gpui_pet_activity_card(
        &self,
        index: usize,
        activity: &GpuiPetOverlayActivityState,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIStatusPetOverlay 2026-06-26-05:36:
        GPUI pet activity cards must match native pet bubbles: render the state as the colored dot only and keep the title as the sole text. Do not show words like "Working" or "Attention" under the title; activation still uses only the exact sanitized session id.
        */
        let state = activity.state;
        let activation_session_id =
            gpui_pet_overlay_activity_activation_session_id(activity).to_string();
        let title = activity.title.clone();
        h_flex()
            .id(format!("ghostex-gpui-pet-activity-card-{index}"))
            .w(px(GPUI_PET_OVERLAY_ACTIVITY_CARD_WIDTH))
            .items_center()
            .gap(px(9.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(gpui_status_pet_surface_border_color())
            .bg(gpui_status_pet_surface_color())
            .px(px(10.0))
            .py(px(8.0))
            .shadow_md()
            .cursor_default()
            .hover(move |this| this.bg(gpui_pet_overlay_activity_hover_color(state)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.dispatch_gpui_status_pet_activation(activation_session_id.as_str(), cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_pet_overlay_context_menu(event.position, window, cx);
                }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(9.0))
                    .rounded_full()
                    .bg(gpui_status_pet_status_color(state)),
            )
            .child(
                v_flex().min_w(px(0.0)).flex_1().gap(px(2.0)).child(
                    div()
                        .w_full()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(gpui_status_pet_surface_text_color())
                        .truncate()
                        .child(title),
                ),
            )
            .into_any_element()
    }

    pub(crate) fn render_gpui_pet_status_badges(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let mut row = h_flex()
            .id("ghostex-gpui-pet-status-badges")
            .w(px(GPUI_PET_OVERLAY_AVATAR_WIDTH))
            .items_center()
            .justify_center()
            .gap(px(4.0));
        for item in self
            .sidebar_pet_overlay
            .status_items
            .iter()
            .filter(|item| item.count > 0)
        {
            row = row.child(self.render_gpui_pet_status_badge(item.status, item.count, cx));
        }
        row.into_any_element()
    }

    pub(crate) fn render_gpui_pet_status_badge(
        &self,
        status: GpuiStatusIndicatorStatus,
        count: u64,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        h_flex()
            .id(format!("ghostex-gpui-pet-status-badge-{}", status.slug()))
            .h(px(GPUI_PET_OVERLAY_STATUS_BADGE_HEIGHT))
            .min_w(px(GPUI_PET_OVERLAY_STATUS_BADGE_HEIGHT))
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(gpui_status_pet_surface_border_color())
            .bg(gpui_status_pet_control_color(status))
            .px(px(7.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::BOLD)
            .text_color(gpui_status_pet_surface_text_color())
            .cursor_default()
            .hover(move |this| this.bg(gpui_status_pet_control_hover_color(status)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.activate_gpui_status_indicator_status(status, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_pet_overlay_context_menu(event.position, window, cx);
                }),
            )
            .child(count.to_string())
            .into_any_element()
    }

    pub(crate) fn render_gpui_pet_avatar(
        &self,
        selected_pet_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIStatusPetOverlay 2026-06-26-05:30:
        GPUI Pet Overlay uses the real bundled WebP spritesheets and must not fall back to another pet asset when `selectedPetId` is wrong, because the parser should accept only known bundled ids.

        CDXC:GPUIStatusPetOverlay 2026-06-26-11:17:
        Worker 53 advances the real bundled 8x9 spritesheet with the shared React avatar frame rows: idle loops slowly, attention uses review, working uses running, and hover uses jumping only when no activity needs attention or work.

        CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
        Worker 57 honors macOS Reduce Motion by rendering one stable frame for the current semantic pet state while retaining the selected bundled pet id. The accessibility preference changes animation scheduling only; it does not substitute pet assets, hide activity rows, or persist system preference data.
        */
        let Some(image) = gpui_pet_overlay_spritesheet_image(selected_pet_id) else {
            return self.render_gpui_pet_unknown_avatar(selected_pet_id);
        };
        let pet_label = gpui_pet_overlay_pet_display_name(selected_pet_id)
            .unwrap_or(selected_pet_id)
            .to_string();
        let frame = gpui_pet_overlay_animation_frame_for_motion_preference(
            self.gpui_pet_overlay_animation_state,
            self.gpui_pet_overlay_animation_started_at,
            Instant::now(),
            self.gpui_pet_overlay_reduce_motion_enabled,
        );
        v_flex()
            .id("ghostex-gpui-pet-avatar-surface")
            .items_center()
            .gap(px(3.0))
            .cursor_default()
            .on_hover(cx.listener(|this, hovered, _window, cx| {
                this.set_gpui_pet_overlay_avatar_hovered(*hovered, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_gpui_pet_overlay_activities_visible(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_pet_overlay_context_menu(event.position, window, cx);
                }),
            )
            .child(
                div()
                    .id("ghostex-gpui-pet-avatar-frame")
                    .relative()
                    .w(px(GPUI_PET_OVERLAY_AVATAR_WIDTH))
                    .h(px(GPUI_PET_OVERLAY_AVATAR_HEIGHT))
                    .overflow_hidden()
                    .child(
                        img(image)
                            .absolute()
                            .left(px(
                                -f32::from(frame.column_index) * GPUI_PET_OVERLAY_AVATAR_WIDTH
                            ))
                            .top(px(
                                -f32::from(frame.row_index) * GPUI_PET_OVERLAY_AVATAR_HEIGHT
                            ))
                            .w(px(GPUI_PET_OVERLAY_AVATAR_WIDTH
                                * GPUI_PET_OVERLAY_SPRITESHEET_COLUMNS))
                            .h(px(
                                GPUI_PET_OVERLAY_AVATAR_HEIGHT * GPUI_PET_OVERLAY_SPRITESHEET_ROWS
                            ))
                            .object_fit(ObjectFit::Fill),
                    ),
            )
            .child(
                div()
                    .max_w(px(128.0))
                    .rounded(px(5.0))
                    .bg(gpui_pet_overlay_label_background_color())
                    .px(px(6.0))
                    .py(px(2.0))
                    .text_size(px(10.0))
                    .text_color(gpui_pet_overlay_secondary_text_color())
                    .truncate()
                    .child(pet_label),
            )
            .into_any_element()
    }

    pub(crate) fn render_gpui_pet_unknown_avatar(&self, selected_pet_id: &str) -> AnyElement {
        v_flex()
            .id("ghostex-gpui-pet-avatar-unknown")
            .w(px(GPUI_PET_OVERLAY_AVATAR_WIDTH))
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .w(px(GPUI_PET_OVERLAY_AVATAR_WIDTH))
                    .h(px(GPUI_PET_OVERLAY_AVATAR_HEIGHT))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(gpui_status_pet_surface_border_color())
                    .bg(gpui_status_pet_surface_color())
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .text_color(gpui_status_pet_surface_text_color())
                    .child("Unknown pet"),
            )
            .child(
                div()
                    .max_w(px(128.0))
                    .text_size(px(10.0))
                    .text_color(gpui_pet_overlay_secondary_text_color())
                    .truncate()
                    .child(selected_pet_id.to_string()),
            )
            .into_any_element()
    }

    pub(crate) fn toggle_gpui_pet_overlay_activities_visible(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIStatusPetOverlay 2026-06-26-11:17:
        GPUI Pet Overlay avatar clicks toggle only the in-stack card/badge presentation. Activity cards and collapsed status badges own their own activation handlers and stop propagation, so selecting a session/status must not also expand or collapse the avatar stack.

        CDXC:GPUIStatusPetOverlay 2026-06-26-11:46:
        Persist the expanded/collapsed activity-card boolean with GPUI shell state so restart restores the same visible pet shape as native `PetOverlayController`. The writer must keep this to one boolean and never persist activity payloads, titles, ids, paths, raw settings JSON, commands, URLs, terminal output, tokens, detached panel origin, or drag state.
        */
        self.gpui_pet_overlay_activities_visible = !self.gpui_pet_overlay_activities_visible;
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn set_gpui_pet_overlay_avatar_hovered(
        &mut self,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gpui_pet_overlay_avatar_hovered == hovered {
            return;
        }
        self.gpui_pet_overlay_avatar_hovered = hovered;
        self.refresh_gpui_pet_overlay_animation_state(cx);
        cx.notify();
    }

    pub(crate) fn set_gpui_pet_overlay_reduce_motion_enabled(
        &mut self,
        reduce_motion_enabled: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gpui_pet_overlay_reduce_motion_enabled == reduce_motion_enabled {
            return;
        }
        self.gpui_pet_overlay_reduce_motion_enabled = reduce_motion_enabled;
        self.refresh_gpui_pet_overlay_animation_state(cx);
        cx.notify();
    }

    pub(crate) fn refresh_gpui_pet_overlay_animation_state(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let next_state = gpui_pet_overlay_animation_state_for_surface(
            &self.sidebar_pet_overlay.activities,
            self.gpui_pet_overlay_avatar_hovered,
        );
        if self.gpui_pet_overlay_animation_state != next_state {
            self.gpui_pet_overlay_animation_state = next_state;
            self.gpui_pet_overlay_animation_started_at = Instant::now();
        }
        if !self.gpui_pet_overlay_reduce_motion_enabled {
            self.ensure_gpui_pet_overlay_animation_ticker(cx);
        }
    }

    pub(crate) fn ensure_gpui_pet_overlay_animation_ticker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gpui_pet_overlay_animation_ticker_active
            || !gpui_pet_overlay_animation_ticker_should_run(
                self.sidebar_pet_overlay.enabled,
                self.gpui_pet_overlay_reduce_motion_enabled,
            )
        {
            return;
        }
        self.gpui_pet_overlay_animation_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_PET_OVERLAY_ANIMATION_TICK)
                    .await;
                let keep_running = this
                    .update(cx, |this, cx| {
                        if !gpui_pet_overlay_animation_ticker_should_run(
                            this.sidebar_pet_overlay.enabled,
                            this.gpui_pet_overlay_reduce_motion_enabled,
                        ) {
                            this.gpui_pet_overlay_animation_ticker_active = false;
                            false
                        } else {
                            cx.notify();
                            true
                        }
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn show_gpui_pet_overlay_context_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIStatusPetOverlay 2026-06-26-11:17:
        The visible GPUI pet surface uses an OS-owned NativeMenu with Sleep Pet first and Go to Ghostex second, matching the native pet host. Menu actions carry no session/project/path/settings payloads; Sleep Pet writes only `petOverlayEnabled: false`, and Go to Ghostex raises the GPUI app/window without selecting a session.
        */
        if !self.sidebar_pet_overlay.enabled {
            return;
        }
        NativeMenu::new()
            .menu("Sleep Pet", Box::new(SleepGpuiPetOverlay))
            .menu("Go to Ghostex", Box::new(GoToGhostexFromGpuiPetOverlay))
            .show(position, window, cx);
    }

    pub(crate) fn sleep_gpui_pet_overlay_from_context_menu(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        if settings_snapshot
            .object()
            .get("petOverlayEnabled")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
        {
            self.sidebar_pet_overlay.enabled = false;
            cx.notify();
            return;
        }

        let mut settings_object = settings_snapshot.object().clone();
        settings_object.insert(
            "petOverlayEnabled".to_string(),
            serde_json::Value::Bool(false),
        );
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            return;
        };
        self.sidebar_pet_overlay.enabled = false;
        self.gpui_pet_overlay_avatar_hovered = false;
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
    }

    pub(crate) fn go_to_ghostex_from_gpui_pet_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.activate(true);
        window.activate_window();
    }

    pub(crate) fn activate_gpui_status_indicator_status(
        &mut self,
        status: GpuiStatusIndicatorStatus,
        cx: &mut gpui::Context<Self>,
    ) {
        let focused_session_id = self
            .sidebar_gxserver_presentation_focus_state
            .focused_session_id
            .as_deref();
        let Some(session_id) = gpui_status_indicator_aggregate_activation_session_id(
            &self.sidebar_session_status_indicators,
            status,
            focused_session_id,
        ) else {
            return;
        };
        self.dispatch_gpui_status_pet_activation(session_id.as_str(), cx);
    }

    pub(crate) fn dispatch_gpui_status_pet_activation(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUIStatusPetOverlay 2026-06-26-05:07:
        Visible GPUI status activation returns only a bounded session id to the sidebar runtime's existing focusSession path. Rust never wakes, creates, restores, or materializes a session from these clicks, and the transient callback shape is deliberately reusable for a later pet slice without exposing a generic event bus, paths, URLs, commands, tokens, titles, or terminal text.
        */
        if !gpui_status_bridge_id_allowed(session_id) {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "sessionId": session_id,
            "type": GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_VERSION,
        });
        let script = gpui_status_pet_activation_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        true
    }

    pub(crate) fn dispatch_gpui_menu_bar_project_activation(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
        Menu-bar project rows route through a fixed first-party sidebar callback carrying only one bounded project id. The sidebar runtime owns project focus and publishing; Rust does not add a generic bus, derive paths/titles, or materialize terminals from project-only clicks.
        */
        if !gpui_status_bridge_id_allowed(project_id) {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "projectId": project_id,
            "type": GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_VERSION,
        });
        let script = gpui_menu_bar_project_activation_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        true
    }

    pub(crate) fn dispatch_gpui_menu_bar_session_activation(
        &mut self,
        project_id: &str,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
        Menu-bar session rows route through a fixed first-party sidebar callback carrying bounded project/session ids. The sidebar then reuses normal session-card focus, including the existing WorkspaceTerminalFocus Rust handoff for local materialization, without renderer JSON, paths, commands, stdout/stderr, tokens, terminal content, or fallback attach behavior.
        */
        if !gpui_status_bridge_id_allowed(project_id) || !gpui_status_bridge_id_allowed(session_id)
        {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "projectId": project_id,
            "sessionId": session_id,
            "type": GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_VERSION,
        });
        let script = gpui_menu_bar_session_activation_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        true
    }

    pub(crate) fn dispatch_gpui_project_board_conversation_request(
        &mut self,
        request: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // The Kanban page's board request is first-party JSON, but Rust still
        // bounds it and requires the envelope fields before it enters the
        // sidebar runtime's script context.
        if !request.is_object() {
            return false;
        }
        let request_id_valid = manage_request_string(request, "requestId")
            .map(|value| value.trim().to_string())
            .is_some_and(|value| {
                !value.is_empty() && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
            });
        if !request_id_valid {
            return false;
        }
        // macOS `appendProjectBoardDebugLog` parity: the board page's debug
        // breadcrumbs persist to the GPUI project-board support log
        // (scenario-gated) while the runtime still answers the state echo.
        // Details arrive as a JSON string and are parsed at the writer
        // boundary like the Swift writers, then sanitized.
        if request.get("action").and_then(serde_json::Value::as_str) == Some("appendDebugLog") {
            let event = request
                .get("event")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if !event.is_empty() {
                let details = request
                    .get("details")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    .unwrap_or(serde_json::Value::Null);
                support_logs::append(support_logs::GpuiSupportLog::ProjectBoard, event, details);
            }
        }
        let message = serde_json::json!({
            "request": request,
            "type": GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_VERSION,
        });
        let script = gpui_project_board_conversation_request_script(&message);
        if script.chars().count() > GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_PAYLOAD_MAX_CHARS {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        sidebar.update(cx, |surface, _| {
            let _ = surface.execute_app_owned_script(&script);
        });
        true
    }

    pub(crate) fn dispatch_gpui_command_palette_session_focus(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // Palette rows carry projected sidebar session ids (combined local or
        // remote-shaped). Rust only bounds the string; the sidebar runtime
        // validates the shape and reuses the reviewed focusSession routing.
        if session_id.is_empty()
            || session_id.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
            || session_id.chars().any(char::is_control)
        {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "sessionId": session_id,
            "type": GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_VERSION,
        });
        let script = gpui_command_palette_session_focus_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        true
    }

    pub(crate) fn dispatch_gpui_command_palette_run_sidebar_command(
        &mut self,
        command_id: &str,
        run_mode: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.dispatch_gpui_run_sidebar_command_with_scope(command_id, run_mode, None, cx)
    }

    /*
    CDXC:GlobalActions 2026-08-01-19:00:
    A run-by-id selector cannot tell a Global Action from a Project Action with
    the same id, so the tab strip stamps its scope and the sidebar runtime
    resolves that list exclusively. The Command Palette keeps sending no scope,
    which the runtime reads as project — unchanged behaviour for every existing
    caller.
    */
    pub(crate) fn dispatch_gpui_run_sidebar_command_with_scope(
        &mut self,
        command_id: &str,
        run_mode: Option<&str>,
        scope: Option<&'static str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // The palette payload is an Action selector only (command id + optional
        // run mode). The sidebar runtime resolves the trusted saved/HUD command
        // and executes through the existing strict SidebarCommandAction bridge;
        // renderer-supplied command text, URLs, or paths never enter this path.
        let bounded = |value: &str| {
            !value.is_empty()
                && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !value.chars().any(char::is_control)
        };
        if !bounded(command_id) || run_mode.is_some_and(|run_mode| !bounded(run_mode)) {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let mut message = serde_json::json!({
            "commandId": command_id,
            "type": GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_VERSION,
        });
        if let Some(run_mode) = run_mode {
            message["runMode"] = serde_json::json!(run_mode);
        }
        if let Some(scope) = scope {
            message["scope"] = serde_json::json!(scope);
        }
        let script = gpui_command_palette_run_sidebar_command_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        true
    }

    pub(crate) fn dispatch_gpui_workspace_tab_session_selected(
        &mut self,
        project_id: &str,
        session_id: &str,
        local_was_sleeping: bool,
        local_runtime_missing: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUIWorkspaceSessionFocus 2026-06-26-08:01:
        Native GPUI workspace tab selection has already mutated Rust pane state, so the sidebar callback may update only local or machine-scoped remote presentation focus. Use a dedicated first-party callback instead of menu-bar session activation so tab clicks cannot re-enter the WorkspaceTerminalFocus attach path and bounce the selected tab.

        CDXC:GPUIWorkspaceSessionFocus 2026-06-27-00:33:
        MacOS reattaches a stale locally sleeping pane tab when gxserver already reports that canonical session running. Send only a true `localWasSleeping` flag for that reconciliation check; ordinary tab selections remain one-way sidebar focus updates.

        CDXC:GPUIWorkspaceSessionFocus 2026-07-11:
        Restored-after-restart Running tabs can have no live terminal owner, no parked owner, and no pending attach payload behind them; selecting one shows an empty body. Send only a true `localRuntimeMissing` flag so the sidebar runtime can reconcile through one bounded WorkspaceTerminalFocus when gxserver still reports that canonical session running, reusing the exact gxserver attach pipeline instead of mounting anything from renderer input.

        Sidebar visibility follows the actual rendered workspace rather than a
        click-history set. Carry the bounded gxserver ids for every active
        rendered Agents leaf or companion slot with the selected id so split
        siblings keep the visible tier and hidden tabs lose it immediately.
        */
        if !gpui_status_bridge_id_allowed(project_id) || !gpui_status_bridge_id_allowed(session_id)
        {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let mut visible_session_ids = self.gpui_sidebar_visible_local_session_ids();
        if !visible_session_ids
            .iter()
            .any(|visible_session_id| visible_session_id == session_id)
        {
            visible_session_ids.push(session_id.to_string());
        }
        let mut message = serde_json::json!({
            "projectId": project_id,
            "sessionId": session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_VERSION,
            "visibleSessionIds": visible_session_ids,
        });
        if local_was_sleeping {
            message["localWasSleeping"] = serde_json::Value::Bool(true);
        }
        if local_runtime_missing {
            message["localRuntimeMissing"] = serde_json::Value::Bool(true);
        }
        let script = gpui_workspace_tab_session_selected_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        true
    }

    pub(crate) fn gpui_sidebar_visible_local_session_ids(&self) -> Vec<String> {
        let shell_session_ids = if self.active_mode == TitlebarMode::Agents {
            self.agents_workspace
                .rendered_leaf_order()
                .into_iter()
                .filter_map(|pane_id| self.agents_workspace.active_session_in_pane(pane_id))
                .collect::<Vec<_>>()
        } else {
            self.current_project_editor_companion_terminal_body_mount_slots()
                .into_iter()
                .map(|slot_id| slot_id.session_id)
                .collect::<Vec<_>>()
        };

        let mut seen = HashSet::new();
        shell_session_ids
            .into_iter()
            .filter_map(|shell_session_id| {
                self.local_workspace_session_mappings
                    .iter()
                    .find_map(|(key, mapped_session_id)| {
                        (*mapped_session_id == shell_session_id).then(|| key.session_id.clone())
                    })
            })
            .filter(|session_id| seen.insert(session_id.clone()))
            .collect()
    }
}
