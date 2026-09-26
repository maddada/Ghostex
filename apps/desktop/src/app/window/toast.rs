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

/// The colours one toast paints with.
struct GpuiAppToastColors {
    background: Hsla,
    border: Hsla,
    title: Hsla,
    description: Hsla,
}

impl GpuiAppToastColors {
    /*
    CDXC:AppModal 2026-09-23 DECISION:
    User: "let's also please make the toasts that we have in the app also use the same effect that the scroll to bottom has and let's only show the x button when we hover them". Under window glass the toast window blurs what is behind each toast (limited to the toasts themselves, not the gaps between them), and each toast takes the chat composer's frosted wash and border instead of a solid tinted card; success, warning and error keep a faint wash and outline of their own colour. The × stays hidden until the toast is hovered. The opaque window keeps the solid cards.
    */
    fn resolve(level: GpuiAppToastLevel, glass: bool, light: bool) -> Self {
        if !glass {
            return Self {
                background: level.container_background().into(),
                border: level.container_border().into(),
                title: level.title_color().into(),
                description: rgba(0xffffffb8).into(),
            };
        }
        let ink: Hsla = if light { gpui::black() } else { gpui::white() };
        let tint = |alpha: f32| -> Hsla { Hsla::from(level.indicator_color()).opacity(alpha) };
        let (background, border) = match level {
            GpuiAppToastLevel::Info => (
                ink.opacity(if light { 0.04 } else { 0.06 }),
                ink.opacity(0.08),
            ),
            _ => (tint(0.10), level.container_border().into()),
        };
        let title: Hsla = if light {
            match level {
                GpuiAppToastLevel::Info => rgb(0x18181b),
                GpuiAppToastLevel::Success => rgb(0x14532d),
                GpuiAppToastLevel::Warning => rgb(0x713f12),
                GpuiAppToastLevel::Error => rgb(0x7f1d1d),
            }
            .into()
        } else {
            level.title_color().into()
        };
        Self {
            background,
            border,
            title,
            description: ink.opacity(if light { 0.62 } else { 0.72 }),
        }
    }
}

#[derive(Clone)]
pub(crate) struct GpuiAppToast {
    pub(crate) id: String,
    pub(crate) level: GpuiAppToastLevel,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) copy_text: Option<String>,
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
/// the Rust store when one does.
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
        copy_text: None,
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
    GPUI_APP_TOAST_VERTICAL_PADDING * 2.0
        + title_height
        + description_height
        + if toast.copy_text.is_some() { 34.0 } else { 0.0 }
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

/// Makes the app-modal window an AppKit child of the main window (CDXC:Onboarding 2026-09-15 in GpuiAppToastWindowChrome.m).
#[cfg(target_os = "macos")]
pub(crate) fn attach_gpui_app_modal_window_to_main_window(
    window: &mut Window,
    main_window_native_view: *mut std::ffi::c_void,
) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    if let RawWindowHandle::AppKit(handle) = handle.as_raw() {
        unsafe {
            GhostexGpuiAttachAppModalWindowToMainWindow(
                handle.ns_view.as_ptr(),
                main_window_native_view,
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn attach_gpui_app_modal_window_to_main_window(
    _window: &mut Window,
    _main_window_native_view: *mut std::ffi::c_void,
) {
}

pub(crate) struct GpuiAppToastWindow {
    pub(crate) app: gpui::WeakEntity<GhostexGpuiApp>,
    pub(crate) toasts: Vec<GpuiAppToast>,
    pub(crate) hovered_toast_id: Option<String>,
    /// The toast cards the window's blurred background was last limited to.
    pub(crate) blur_region: Vec<(Bounds<Pixels>, Pixels)>,
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

/// Whether toasts sit on their own blurred window.
///
/// CDXC:Theming 2026-09-25 WHY:
/// Frosted toasts need their window's blur limited to the cards (`set_background_blur_region`), so the gaps between toasts and the close button's outset stay clear. Windows can only confine a window's blur by clipping the window itself, which would cut the hover close button, so on Windows toasts keep their solid tinted cards while the rest of the app is glass.
pub(crate) fn toast_window_glass() -> bool {
    cfg!(target_os = "macos") && window_glass_active()
}

impl Render for GpuiAppToastWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hovered_toast_id = self.hovered_toast_id.clone();
        let glass = toast_window_glass();
        let light = chrome_uses_light_appearance();
        // Each card reports its painted frame here so the blurred background covers the cards
        // themselves and not the gaps or the close buttons' outset.
        let card_frames: std::rc::Rc<std::cell::RefCell<Vec<(Bounds<Pixels>, Pixels)>>> =
            Default::default();
        let region_frames = card_frames.clone();
        let toast_window = cx.entity().downgrade();

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
                let colors = GpuiAppToastColors::resolve(toast.level, glass, light);
                let card_frames = card_frames.clone();
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
                            .relative()
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.background)
                            .when(glass, |card| {
                                card.child(
                                    canvas(
                                        move |bounds, _, _| {
                                            // Pinned to the padding box, so grow by the 1px
                                            // border to cover the whole card.
                                            card_frames
                                                .borrow_mut()
                                                .push((bounds.dilate(px(1.0)), px(10.0)));
                                        },
                                        |_, _, _, _| {},
                                    )
                                    // Without insets an absolute child sits at the card's
                                    // content origin, inside its padding, which put the blur
                                    // off the card by the padding.
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .bottom_0(),
                                )
                            })
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
                                                    .text_color(colors.title)
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
                                                        .text_color(colors.description)
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
                                            })
                                            .when_some(
                                                toast.copy_text.clone(),
                                                |column, report| {
                                                    column.child(
                                                        div()
                                                            .id(format!(
                                                                "toast-copy-diagnostics-{}",
                                                                toast.id
                                                            ))
                                                            .mt(px(6.0))
                                                            .h(px(26.0))
                                                            .px(px(8.0))
                                                            .flex()
                                                            .items_center()
                                                            .rounded(px(4.0))
                                                            .border_1()
                                                            .border_color(
                                                                colors.title.opacity(0.25),
                                                            )
                                                            .text_size(px(12.0))
                                                            .text_color(colors.title)
                                                            .cursor_pointer()
                                                            .hover(move |button| {
                                                                button
                                                                    .bg(colors.title.opacity(0.094))
                                                            })
                                                            .on_click(move |_, _, cx| {
                                                                cx.stop_propagation();
                                                                gpui_copy_to_clipboard(
                                                                    ClipboardItem::new_string(
                                                                        report.clone(),
                                                                    ),
                                                                    cx,
                                                                );
                                                            })
                                                            .child("Copy diagnostics"),
                                                    )
                                                },
                                            ),
                                    ),
                            ),
                    )
                    .when(show_close_button, |toast_element| {
                        toast_element.child(self.render_close_button(toast_id, cx))
                    })
            }))
            .when(glass, |stack| {
                // Painted after every card, so it sees all of this frame's card frames.
                stack.child(
                    canvas(
                        move |_, window, cx| {
                            let frames = region_frames.borrow().clone();
                            let changed = toast_window
                                .update(cx, |toast_window, _| {
                                    if toast_window.blur_region == frames {
                                        return false;
                                    }
                                    toast_window.blur_region = frames.clone();
                                    true
                                })
                                .unwrap_or(false);
                            if changed {
                                window.set_background_blur_region(frames);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_0(),
                )
            })
    }
}
