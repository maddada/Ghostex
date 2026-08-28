// C1 wave-2 extraction: the GpuiAppToastWindow entity, its toast-stack model, and consts moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

pub(crate) const GPUI_APP_TOAST_WIDTH: f32 = 356.0;
pub(crate) const GPUI_APP_TOAST_GAP: f32 = 10.0;
pub(crate) const GPUI_APP_TOAST_BOTTOM_MARGIN: f32 = 47.0;
pub(crate) const GPUI_APP_TOAST_MAX_VISIBLE: usize = 4;
pub(crate) const GPUI_APP_TOAST_DEFAULT_DURATION_MS: u64 = 8_000;
pub(crate) const GPUI_APP_TOAST_CLOSE_SIZE: f32 = 18.0;
pub(crate) const GPUI_APP_TOAST_CLOSE_OUTSET: f32 = GPUI_APP_TOAST_CLOSE_SIZE / 2.0;
pub(crate) const GPUI_APP_TOAST_CLOSE_WINDOW_INSET: f32 = 5.0;
pub(crate) const GPUI_APP_TOAST_CLOSE_TOP_INSET: f32 = 5.0;
pub(crate) const GPUI_APP_TOAST_WINDOW_WIDTH: f32 =
    GPUI_APP_TOAST_WIDTH + GPUI_APP_TOAST_CLOSE_OUTSET;
pub(crate) const GPUI_APP_TOAST_WRAPPER_GAP: f32 = GPUI_APP_TOAST_GAP - GPUI_APP_TOAST_CLOSE_OUTSET;
pub(crate) const GPUI_APP_TOAST_HORIZONTAL_PADDING: f32 = 12.0;
pub(crate) const GPUI_APP_TOAST_VERTICAL_PADDING: f32 = 10.0;
pub(crate) const GPUI_APP_TOAST_CONTENT_GAP: f32 = 2.0;
pub(crate) const GPUI_APP_TOAST_TITLE_LINE_HEIGHT: f32 = 18.0;
pub(crate) const GPUI_APP_TOAST_DESCRIPTION_LINE_HEIGHT: f32 = 17.0;
pub(crate) const GPUI_APP_TOAST_TITLE_CHARS_PER_LINE: usize = 36;
pub(crate) const GPUI_APP_TOAST_DESCRIPTION_CHARS_PER_LINE: usize = 40;
pub(crate) const GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID: &str = "gpui-session-chat-file-opening";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuiAppToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl GpuiAppToastLevel {
    pub(crate) fn from_raw(raw: Option<&str>) -> Self {
        match raw {
            Some("success") => Self::Success,
            Some("warning") => Self::Warning,
            Some("error") => Self::Error,
            _ => Self::Info,
        }
    }

    fn container_background(self) -> gpui::Rgba {
        match self {
            Self::Info => rgba(0x202124f2),
            Self::Success => rgba(0x1d2b22f2),
            Self::Warning => rgba(0x33291af2),
            Self::Error => rgba(0x3a1d20f2),
        }
    }

    fn container_border(self) -> gpui::Rgba {
        match self {
            Self::Info => rgba(0xffffff1f),
            Self::Success => rgba(0x4ade804d),
            Self::Warning => rgba(0xfbbf244d),
            Self::Error => rgba(0xf8717152),
        }
    }

    fn title_color(self) -> gpui::Rgba {
        match self {
            Self::Info => rgba(0xe7e7eaff),
            Self::Success => rgba(0xf0fdf4ff),
            Self::Warning => rgba(0xfef3c7ff),
            Self::Error => rgba(0xfff1f2ff),
        }
    }

    fn indicator_color(self) -> gpui::Rgba {
        match self {
            Self::Info => rgba(0x60a5faff),
            Self::Success => rgba(0x4ade80ff),
            Self::Warning => rgba(0xfbbf24ff),
            Self::Error => rgba(0xf87171ff),
        }
    }
}

#[derive(Clone)]
pub(crate) struct GpuiAppToast {
    pub(crate) id: String,
    pub(crate) level: GpuiAppToastLevel,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) loading: bool,
    pub(crate) persistent: bool,
    pub(crate) duration_ms: u64,
    pub(crate) epoch: u64,
}

pub(crate) fn gpui_app_toast_comparable_text(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_matches(|character: char| {
            character.is_whitespace() || matches!(character, '.' | '!' | '?')
        })
        .to_lowercase()
}

pub(crate) fn gpui_normalized_app_toast_description(
    title: &str,
    description: Option<&str>,
) -> Option<String> {
    let description = description?.trim();
    if description.is_empty() {
        return None;
    }
    if gpui_app_toast_comparable_text(title) == gpui_app_toast_comparable_text(description) {
        return None;
    }
    Some(description.to_string())
}

/// Parses the shared `createAppToastRequest` bridge payload. Action buttons are
/// not parsed because no GPUI-side producer sends them yet; add routing back to
/// the sidebar runtime when one does.
pub(crate) fn gpui_app_toast_from_bridge_message(
    message: &serde_json::Value,
    generated_id: String,
) -> Option<GpuiAppToast> {
    let title = message
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())?
        .to_string();
    let description = gpui_normalized_app_toast_description(
        &title,
        message
            .get("description")
            .and_then(serde_json::Value::as_str),
    );
    let duration_ms = message
        .get("durationMs")
        .and_then(serde_json::Value::as_f64)
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .map(|duration| duration as u64)
        .unwrap_or(GPUI_APP_TOAST_DEFAULT_DURATION_MS);
    Some(GpuiAppToast {
        id: message
            .get("toastId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .unwrap_or(generated_id),
        level: GpuiAppToastLevel::from_raw(
            message.get("level").and_then(serde_json::Value::as_str),
        ),
        title,
        description,
        loading: false,
        persistent: message
            .get("persistent")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        duration_ms,
        epoch: 0,
    })
}

pub(crate) fn gpui_app_toast_wrapped_line_count(text: &str, chars_per_line: usize) -> usize {
    text.split('\n')
        .map(|line| line.chars().count().div_ceil(chars_per_line).max(1))
        .sum::<usize>()
        .max(1)
}

pub(crate) fn gpui_app_toast_estimated_height(toast: &GpuiAppToast) -> f32 {
    let title_height =
        gpui_app_toast_wrapped_line_count(&toast.title, GPUI_APP_TOAST_TITLE_CHARS_PER_LINE) as f32
            * GPUI_APP_TOAST_TITLE_LINE_HEIGHT;
    let description_height = toast
        .description
        .as_deref()
        .map(|description| {
            GPUI_APP_TOAST_CONTENT_GAP
                + if toast.id == GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID {
                    1.0
                } else {
                    gpui_app_toast_wrapped_line_count(
                        description,
                        GPUI_APP_TOAST_DESCRIPTION_CHARS_PER_LINE,
                    ) as f32
                } * GPUI_APP_TOAST_DESCRIPTION_LINE_HEIGHT
        })
        .unwrap_or(0.0);
    GPUI_APP_TOAST_VERTICAL_PADDING * 2.0 + title_height + description_height
}

pub(crate) fn gpui_app_toast_stack_height(toasts: &[GpuiAppToast]) -> f32 {
    let toast_heights: f32 = toasts.iter().map(gpui_app_toast_estimated_height).sum();
    toast_heights
        + GPUI_APP_TOAST_GAP * toasts.len().saturating_sub(1) as f32
        + GPUI_APP_TOAST_CLOSE_OUTSET
        + GPUI_APP_TOAST_CLOSE_WINDOW_INSET
}

#[cfg(target_os = "macos")]
pub(crate) fn remove_gpui_app_toast_popup_window_chrome(
    window: &mut Window,
    main_window_native_view: *mut std::ffi::c_void,
) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        unsafe {
            GhostexGpuiRemoveToastPopupWindowChrome(handle.ns_view.as_ptr());
            GhostexGpuiAttachToastPopupToMainWindow(
                handle.ns_view.as_ptr(),
                main_window_native_view,
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn remove_gpui_app_toast_popup_window_chrome(
    _window: &mut Window,
    _main_window_native_view: *mut std::ffi::c_void,
) {
}

#[cfg(target_os = "macos")]
pub(crate) fn prepare_gpui_titlebar_popup_window_chrome(window: &mut Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        unsafe { GhostexGpuiPrepareTitlebarPopupWindow(handle.ns_view.as_ptr()) };
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn prepare_gpui_titlebar_popup_window_chrome(_window: &mut Window) {}

pub(crate) struct GpuiAppToastWindow {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) toasts: Vec<GpuiAppToast>,
    pub(crate) hovered_toast_id: Option<String>,
}

impl GpuiAppToastWindow {
    pub(crate) fn set_toasts(&mut self, toasts: Vec<GpuiAppToast>, cx: &mut gpui::Context<Self>) {
        self.toasts = toasts;
        if self.hovered_toast_id.as_ref().is_some_and(|hovered_id| {
            !self
                .toasts
                .iter()
                .any(|toast| toast.id.as_str() == hovered_id.as_str())
        }) {
            self.hovered_toast_id = None;
        }
        cx.notify();
    }

    fn set_hovered_toast(&mut self, toast_id: &str, hovered: bool, cx: &mut gpui::Context<Self>) {
        if hovered {
            if self.hovered_toast_id.as_deref() != Some(toast_id) {
                self.hovered_toast_id = Some(toast_id.to_string());
                cx.notify();
            }
            return;
        }
        if self.hovered_toast_id.as_deref() == Some(toast_id) {
            self.hovered_toast_id = None;
            cx.notify();
        }
    }

    fn render_close_button(&self, toast_id: String, cx: &mut gpui::Context<Self>) -> AnyElement {
        let app = self.app.clone();
        div()
            .id(format!("ghostex-gpui-app-toast-dismiss-{toast_id}"))
            .absolute()
            .right_0()
            .top(px(GPUI_APP_TOAST_CLOSE_TOP_INSET))
            .flex()
            .size(px(GPUI_APP_TOAST_CLOSE_SIZE))
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(gpui_app_toast_close_button_border_color())
            .bg(gpui_app_toast_close_button_color())
            .text_color(gpui_app_toast_close_button_icon_color())
            .cursor_default()
            .hover(|this| this.bg(gpui_app_toast_close_button_hover_color()))
            .tooltip(|window, cx| Tooltip::new("Dismiss toast").build(window, cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let app = app.clone();
                    let toast_id = toast_id.clone();
                    cx.defer(move |cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.remove_gpui_app_toast(&toast_id, cx);
                        });
                    });
                }),
            )
            .child(titlebar_svg_icon(
                COMMAND_ICON_XMARK,
                8.0,
                gpui_app_toast_close_button_icon_color(),
            ))
            .into_any_element()
    }
}

pub(crate) fn gpui_app_toast_close_button_color() -> Hsla {
    rgb(0x0e0e0e).opacity(0.96).into()
}

pub(crate) fn gpui_app_toast_close_button_hover_color() -> Hsla {
    rgb(0x252525).opacity(0.98).into()
}

pub(crate) fn gpui_app_toast_close_button_border_color() -> Hsla {
    rgb(0xffffff).opacity(0.18).into()
}

pub(crate) fn gpui_app_toast_close_button_icon_color() -> Hsla {
    rgb(0xffffff).opacity(0.90).into()
}

impl Render for GpuiAppToastWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hovered_toast_id = self.hovered_toast_id.clone();

        div()
            .flex()
            .flex_col()
            .justify_end()
            .gap(px(GPUI_APP_TOAST_WRAPPER_GAP))
            .w(px(GPUI_APP_TOAST_WINDOW_WIDTH))
            .h_full()
            .children(self.toasts.iter().map(|toast| {
                let toast_id = toast.id.clone();
                let hover_toast_id = toast.id.clone();
                let show_close_button = hovered_toast_id.as_deref() == Some(toast.id.as_str());
                let description = toast.description.clone();
                let truncate_description_from_start =
                    toast.id == GPUI_SESSION_CHAT_FILE_OPENING_TOAST_ID;
                let indicator = if toast.loading && toast.id != GPUI_GXSERVER_DAEMON_TOAST_ID {
                    canvas(
                        move |_bounds, _window, _cx| {},
                        move |bounds, _state: (), window, _cx| {
                            window.request_animation_frame();
                            paint_agent_gui_loading_spinner(bounds, window);
                        },
                    )
                    .flex_shrink_0()
                    .mt(px((GPUI_APP_TOAST_TITLE_LINE_HEIGHT - 14.0) / 2.0))
                    .size(px(14.0))
                    .into_any_element()
                } else {
                    div()
                        .flex_shrink_0()
                        .mt(px((GPUI_APP_TOAST_TITLE_LINE_HEIGHT - 8.0) / 2.0))
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded(px(4.0))
                        .bg(toast.level.indicator_color())
                        .into_any_element()
                };
                div()
                    .id(format!("ghostex-gpui-app-toast-wrapper-{toast_id}"))
                    .relative()
                    .w(px(GPUI_APP_TOAST_WINDOW_WIDTH))
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        this.set_hovered_toast(&hover_toast_id, *hovered, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w(px(GPUI_APP_TOAST_WIDTH))
                            .mt(px(GPUI_APP_TOAST_CLOSE_OUTSET))
                            .mr(px(GPUI_APP_TOAST_CLOSE_OUTSET))
                            .px(px(GPUI_APP_TOAST_HORIZONTAL_PADDING))
                            .py(px(GPUI_APP_TOAST_VERTICAL_PADDING))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(toast.level.container_border())
                            .bg(toast.level.container_background())
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(7.0))
                                    .child(indicator)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(GPUI_APP_TOAST_CONTENT_GAP))
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .line_height(px(
                                                        GPUI_APP_TOAST_TITLE_LINE_HEIGHT,
                                                    ))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(toast.level.title_color())
                                                    .child(toast.title.clone()),
                                            )
                                            .when_some(description, |text_column, description| {
                                                text_column.child(
                                                    div()
                                                        .w_full()
                                                        .text_size(px(12.0))
                                                        .line_height(px(
                                                            GPUI_APP_TOAST_DESCRIPTION_LINE_HEIGHT,
                                                        ))
                                                        .text_color(rgba(0xffffffb8))
                                                        .when(
                                                            truncate_description_from_start,
                                                            |description| {
                                                                description
                                                                    .overflow_hidden()
                                                                    .whitespace_nowrap()
                                                                    .text_ellipsis_start()
                                                            },
                                                        )
                                                        .child(description),
                                                )
                                            }),
                                    ),
                            ),
                    )
                    .when(show_close_button, |toast_element| {
                        toast_element.child(self.render_close_button(toast_id, cx))
                    })
            }))
    }
}
